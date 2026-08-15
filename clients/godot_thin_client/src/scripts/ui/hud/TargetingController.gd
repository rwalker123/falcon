class_name TargetingController
extends RefCounted

## The COMMAND-TARGETING cluster (HUD decomposition, docs/plan_hud_decomposition.md): the three
## remaining targeting flows — move-band (pick a destination TILE), send-expedition (outfit a party,
## then pick a target TILE) and pick-quarry (the parties compose sheet's HERD picker) — plus the
## floating top-centre targeting banner that guides each. Lifted verbatim out of `Hud.gd`.
##
## Built on the LegendController / TurnOrbController / SelectionCardController / DrawerComposeController /
## BandPanelController idiom: `HudLayer` holds one as `_targeting`, hands it the shared `RefCounted`
## state models BY REFERENCE, and keeps thin reflective delegators for the three methods reached BY
## NAME from outside the HUD node (`is_targeting_active` / `cancel_active_targeting` — both probed by
## Main / MapView via `has_method`, a failed probe failing SILENTLY — and `try_dispatch`, called from
## `show_tile_selection` / `notify_hex_selected`).
##
## IT EMITS ITS OWN SIGNALS; `HudLayer` RELAYS each onto the same-named `HudLayer` signal (the
## TurnOrbController pattern — the controller never emits a `HudLayer` signal directly):
## `targeting_changed` · `move_band_requested` · `send_expedition_requested`.
##
## Collaborators + injections:
##   • `_band_labor` — `record_pending_move` (the optimistic move overlay) and the grid pair
##     (`grid_width` / `wrap_horizontal`) the wrap-aware hex distance reads.
##   • `_compose` — the parties compose's quarry + autofill one-shots (`set_party_quarry` /
##     `arm_party_autofill`); the SAME instance HudLayer / DrawerComposeController / BandPanelController
##     hold. (NOT in the original spec's ctor — the pick flow needs it; see the report.)
##   • `_drawercompose` — the cluster's three `close_compose_sheet()` nudges (a targeting flow closes a
##     sheet floating over the map — §15).
##   • `_note_sink` — where the two miss/refusal nudges the quarry pick posts go:
##     `HudLayer.note_system_event`, i.e. the event dock's System channel. It was the retired feed.
##   • `_host` — the HUD CanvasLayer, so this `RefCounted` has a node to parent the banner into (a
##     `RefCounted` cannot `add_child`). The banner is parented into the host's `LayoutRoot` (NOT the
##     bare CanvasLayer) so it keeps insetting with the reserved-edge docks exactly as before.
##   • `_resolve_assign_band_fn` — `_resolve_assign_band` STAYS on HudLayer (DrawerComposeController
##     injects it too). Reached through a typed adapter (`Callable.call` returns `Variant`, which trips
##     warnings-as-errors).
##   • `_after_pending_change_fn` — `_after_pending_change` STAYS on HudLayer (the `_emit_assign_labor`
##     pending path owns it); the move-band dispatch injects it.
##   • `_rerender_band_panel_fn` — the pick flow's `_bandpanel.rerender()`. `_bandpanel` is constructed
##     AFTER `_targeting`, so a direct ref is impossible at construction; it is injected as a lazily
##     bound Callable that resolves `_bandpanel` at call time. (Also not in the original spec's ctor —
##     the construction-order break the spec asked me to flag; see the report.)

# --- The controller's OWN signals (HudLayer connects + relays each; see the class header) ---
# Targeting state changed — relayed to HudLayer.targeting_changed (→ MapView.set_targeting).
signal targeting_changed(info: Dictionary)
# A move-band destination was picked — relayed to HudLayer.move_band_requested.
signal move_band_requested(payload: Dictionary)
# A send-expedition target was picked — relayed to HudLayer.send_expedition_requested.
signal send_expedition_requested(payload: Dictionary)

# --- The quarry rule's own vocabulary -------------------------------------------------------------
## The `min_distance` for a mission with NO beyond-reach rule. `-1` rather than `0`, because the test
## every surface applies is "strictly farther than this": at `0` a herd standing ON the band's own tile
## would fail it, and that herd is a legal denial target. At `-1` every KNOWN distance passes and the
## unknown one (`-1`) still fails, which is the "an unknown distance is never a quarry" half of the
## rule falling out of the same comparison instead of needing a second clause.
const QUARRY_NO_REACH_BOUND := -1
## Where `begin_pick_quarry` files the mission on the pending dict, read back by `_pick_quarry_mission`.
const PICK_QUARRY_MISSION_KEY := "mission"

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
var _band_labor: HudBandLaborState = null
var _compose: ComposeState = null
var _drawercompose: DrawerComposeController = null
var _note_sink: Callable
# The HUD CanvasLayer, so this RefCounted has a node to parent the banner into.
var _host: Node = null

# --- Retained HudLayer helpers, injected (see the class header) ---
var _resolve_assign_band_fn: Callable
var _after_pending_change_fn: Callable
var _rerender_band_panel_fn: Callable

# --- Owned state (moved off HudLayer) ---
# Move-band targeting: the pending band-relocation tile pick. {} when inactive. Holds the band dict.
var _pending_move_band: Dictionary = {}
# Send-expedition targeting: the pending expedition-launch tile pick. {} when inactive. Holds the
# resident band being outfitted plus the chosen party size.
var _pending_send_expedition: Dictionary = {}
# Quarry-pick targeting: the pending HERD pick for the party compose sheet. {} when inactive. Carries
# only the band — party size and the escapement floor are chosen in the sheet AFTER the quarry.
var _pending_pick_quarry: Dictionary = {}
var _targeting_banner: PanelContainer = null
var _targeting_banner_label: RichTextLabel = null

func _init(band_labor: HudBandLaborState, compose: ComposeState,
		drawercompose: DrawerComposeController, note_sink: Callable, host: Node,
		resolve_assign_band: Callable, after_pending_change: Callable,
		rerender_band_panel: Callable) -> void:
	_band_labor = band_labor
	_compose = compose
	_drawercompose = drawercompose
	_note_sink = note_sink
	_host = host
	_resolve_assign_band_fn = resolve_assign_band
	_after_pending_change_fn = after_pending_change
	_rerender_band_panel_fn = rerender_band_panel

# ---- Typed adapter over the one injected HudLayer helper with a return value -------------------

## Resolve the band a targeting flow acts on (the selected player band, else the faction default).
## Retained on HudLayer because DrawerComposeController injects it too. Reached through this typed
## adapter rather than called raw — `Callable.call` returns `Variant`, which trips warnings-as-errors.
func _resolve_assign_band() -> Dictionary:
	return _resolve_assign_band_fn.call()

# ---- The floating targeting banner --------------------------------------------------------------

## Build the top-centre targeting banner (lazily). It floats above the map, telling the player what to
## click next and offering Cancel — the primary targeting feedback. Parented into the HUD's LayoutRoot
## (so it insets with the reserved-edge docks), which a RefCounted reaches through the host node.
func _ensure_targeting_banner() -> void:
	if _targeting_banner != null:
		return
	var center := CenterContainer.new()
	center.name = "TargetingBannerCenter"
	center.anchor_left = 0.0
	center.anchor_right = 1.0
	center.anchor_top = 0.0
	center.anchor_bottom = 0.0
	center.offset_top = 12.0
	# Anchored to the top edge with zero anchored height; grow downward so the
	# container takes its child's (the banner's) height instead of a 0/negative
	# rect that could clip it.
	center.grow_vertical = Control.GROW_DIRECTION_END
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var layout_root := _host.get_node_or_null(^"LayoutRoot")
	layout_root.add_child(center)

	var banner := PanelContainer.new()
	banner.name = "TargetingBanner"
	banner.add_theme_stylebox_override("panel", HudStyle.banner_stylebox())
	banner.visible = false
	center.add_child(banner)

	var hbox := HBoxContainer.new()
	hbox.add_theme_constant_override("separation", 12)
	banner.add_child(hbox)

	var reticle := Label.new()
	reticle.text = "⌖"  # ⌖ target reticle
	reticle.add_theme_color_override("font_color", HudStyle.SIGNAL)
	reticle.add_theme_font_size_override("font_size", 20)
	reticle.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	hbox.add_child(reticle)

	var label := RichTextLabel.new()
	label.name = "TargetingLabel"
	label.bbcode_enabled = true
	label.fit_content = true
	label.scroll_active = false
	label.autowrap_mode = TextServer.AUTOWRAP_OFF
	label.add_theme_stylebox_override("normal", HudStyle.empty_stylebox())
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	hbox.add_child(label)

	var cancel := Button.new()
	cancel.text = "Cancel  (Esc)"
	HudStyle.apply_button(cancel, "ghost")
	cancel.pressed.connect(cancel_active_targeting)
	hbox.add_child(cancel)

	_targeting_banner = banner
	_targeting_banner_label = label

## Recompute targeting state from the pending flows, update the banner, and notify listeners
## (HudLayer relays targeting_changed -> MapView). Call after any pending change.
func _refresh_targeting() -> void:
	_ensure_targeting_banner()
	var info := _current_targeting_info()
	if info.is_empty():
		_targeting_banner.visible = false
	else:
		_targeting_banner.visible = true
		_targeting_banner_label.text = _targeting_banner_bbcode(info)
	targeting_changed.emit(info)

## True while any command-targeting flow is armed. The ESC pause menu (Main._unhandled_input) checks
## this so it yields ESC to MapView's targeting-cancel path instead of stealing it to open the menu.
func is_targeting_active() -> bool:
	return not _current_targeting_info().is_empty()

## The active targeting descriptor, or {} when nothing is targeting. Move-band is the one flow that
## needs a destination tile; send-expedition also a tile; pick-quarry a herd.
func _current_targeting_info() -> Dictionary:
	if not _pending_move_band.is_empty():
		var pos: Array = Array(_pending_move_band.get("pos", []))
		var ox := int(pos[0]) if pos.size() == 2 else int(_pending_move_band.get("current_x", -1))
		var oy := int(pos[1]) if pos.size() == 2 else int(_pending_move_band.get("current_y", -1))
		return {
			"active": true,
			"command": "move",
			"need": "tile",
			"origin_x": ox,
			"origin_y": oy,
			"context_label": String(_pending_move_band.get("id", "Band")),
		}
	if not _pending_send_expedition.is_empty():
		var band: Dictionary = _pending_send_expedition.get("band", {})
		var pos: Array = Array(band.get("pos", []))
		var ox := int(pos[0]) if pos.size() == 2 else int(band.get("current_x", -1))
		var oy := int(pos[1]) if pos.size() == 2 else int(band.get("current_y", -1))
		return {
			"active": true,
			"command": "expedition",
			"need": "tile",
			"origin_x": ox,
			"origin_y": oy,
			"context_label": "%s · %d" % [
				String(band.get("id", "Band")), int(_pending_send_expedition.get("party_workers", 0)),
			],
		}
	if not _pending_pick_quarry.is_empty():
		var band: Dictionary = _pending_pick_quarry.get("band", {})
		var pos: Array = Array(band.get("pos", []))
		var ox := int(pos[0]) if pos.size() == 2 else int(band.get("current_x", -1))
		var oy := int(pos[1]) if pos.size() == 2 else int(band.get("current_y", -1))
		# `need: "herd"` is what makes MapView glow the huntable herds. No party size in the label —
		# none is chosen yet; the sheet asks for it once the quarry is known.
		# `min_distance`: a valid target must lie STRICTLY farther than this from the origin — the
		# render-side half of `is_expedition_quarry`, so the halo cannot offer a herd the pick will
		# refuse. It is THE SAME `quarry_min_distance` the pick itself compares against, so the two
		# cannot drift — including across missions: a hunt puts the band's `hunt_reach` on the wire
		# and a denial raid `QUARRY_NO_REACH_BOUND`, which glows every herd the band can see. Every
		# other targeting mode omits the key and MapView defaults it to 0, which admits everything
		# and so changes nothing for move/scout-tile targeting.
		return {
			"active": true,
			"command": "quarry",
			"need": "herd",
			"origin_x": ox,
			"origin_y": oy,
			"min_distance": quarry_min_distance(band, _pick_quarry_mission()),
			"context_label": String(band.get("id", "Band")),
		}
	return {}

func _targeting_banner_bbcode(info: Dictionary) -> String:
	var cmd := String(info.get("command", "")).to_upper()
	var need := String(info.get("need", ""))
	var ctx := String(info.get("context_label", ""))
	var loc := ""
	if need == "band":
		loc = "  [color=#%s](%d, %d)[/color]" % [
			HudStyle.INK_DIM_HEX, int(info.get("origin_x", 0)), int(info.get("origin_y", 0)),
		]
	var instruction := ""
	if need == "band":
		instruction = "click a band to send it here"
	elif cmd == "MOVE":
		instruction = "click a destination tile"
	elif cmd == "EXPEDITION":
		instruction = "click a target tile to scout"
	elif cmd == "QUARRY":
		instruction = "click on a herd to hunt"
	else:
		instruction = "click a tile to survey"
	return "[color=#%s]%s[/color]  [color=#%s]%s[/color]%s   [color=#%s]— %s[/color]" % [
		HudStyle.SIGNAL_HEX, cmd, HudStyle.INK_HEX, ctx, loc, HudStyle.INK_DIM_HEX, instruction,
	]

## Cancel the active targeting (banner Cancel / Esc / right-click all route here).
func cancel_active_targeting() -> void:
	_cancel_pending_move_band()
	_cancel_pending_send_expedition()
	cancel_pick_quarry()

# ---- Move-band -----------------------------------------------------------------------------------

## Move-band: enter tile-targeting; the destination click emits move_band_requested.
func begin_move_band() -> void:
	# Targeting asks the player to click the map — a sheet floating over it is a trap (§15).
	_drawercompose.close_compose_sheet()
	var band := _resolve_assign_band()
	if band.is_empty():
		return
	_pending_move_band = band.duplicate(true)
	_refresh_targeting()

func _cancel_pending_move_band() -> void:
	if _pending_move_band.is_empty():
		return
	_pending_move_band = {}
	_refresh_targeting()

func _try_dispatch_pending_move_band(tile_info: Dictionary) -> void:
	if _pending_move_band.is_empty() or tile_info.is_empty():
		return
	var x := int(tile_info.get("x", -1))
	var y := int(tile_info.get("y", -1))
	if x < 0 or y < 0:
		return
	var band := _pending_move_band
	# The command names the DURABLE `band_id` (see `HudConst.NO_BAND_ID`); the optimistic pending
	# overlay stays filed under the client-local `entity`, which is what every reader of it looks up.
	var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
	var entity := int(band.get("entity", -1))
	if band_id == HudConst.NO_BAND_ID or entity < 0:
		return
	move_band_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": band_id,
		"x": x,
		"y": y,
		# **THE ROLLBACK HANDLE, NOT A COMMAND TOKEN** — the same seam the labor payload carries. The
		# optimistic move below is written here and the send's outcome is known only in `Main`, so the
		# failure path needs the client-local `entity` the overlay is filed under. `format_move_band`
		# does not read it; `Main._on_hud_move_band` hands it back to `drop_pending_move`.
		"pending_entity": entity,
	})
	_pending_move_band = {}
	_refresh_targeting()
	# Optimistic feedback: mark the destination pending until a newer-turn snapshot confirms.
	_band_labor.record_pending_move(entity, x, y)
	_after_pending_change_fn.call()

# ---- Send-expedition -----------------------------------------------------------------------------

## Send-expedition: outfit `band` with `party_workers` and enter tile-targeting; the next tile
## click emits send_expedition_requested. Mirrors the move-band pending flow.
func begin_send_expedition(band: Dictionary, party_workers: int) -> void:
	# Targeting asks the player to click the map — a sheet floating over it is a trap (§15).
	_drawercompose.close_compose_sheet()
	if band.is_empty() or party_workers <= 0:
		return
	_pending_send_expedition = {"band": band.duplicate(true), "party_workers": party_workers}
	_refresh_targeting()

func _cancel_pending_send_expedition() -> void:
	if _pending_send_expedition.is_empty():
		return
	_pending_send_expedition = {}
	_refresh_targeting()

func _try_dispatch_pending_send_expedition(tile_info: Dictionary) -> void:
	if _pending_send_expedition.is_empty() or tile_info.is_empty():
		return
	var x := int(tile_info.get("x", -1))
	var y := int(tile_info.get("y", -1))
	if x < 0 or y < 0:
		return
	var band: Dictionary = _pending_send_expedition.get("band", {})
	send_expedition_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"party_workers": int(_pending_send_expedition.get("party_workers", 0)),
		"x": x,
		"y": y,
	})
	_pending_send_expedition = {}
	_refresh_targeting()

# ---- Pick-quarry ---------------------------------------------------------------------------------

## Quarry PICK: enter HERD-targeting so the next map click names the herd the compose sheet is aimed
## at. It dispatches NOTHING — the sheet stays open behind the targeting and fills its Quarry row in,
## then asks for the floor and the party size against that herd.
##
## **THE MISSION RIDES WITH THE PICK** because eligibility is a function of it (`is_expedition_quarry`):
## a hunt's quarry must lie beyond the band's reach and a denial raid's need not. It is carried in the
## pending dict rather than re-asked at the click, so the rule the banner glowed under and the rule the
## click is judged by are the same one.
func begin_pick_quarry(band: Dictionary,
		mission: String = HudComposeVocab.COMPOSE_MISSION_HUNT) -> void:
	# Targeting asks the player to click the map — the tile panel's FLOATING sheet over it is a trap
	# (§15). The DOCKED party sheet is not floating and deliberately stays open.
	_drawercompose.close_compose_sheet()
	if band.is_empty():
		return
	_pending_pick_quarry = {"band": band.duplicate(true), PICK_QUARRY_MISSION_KEY: mission}
	_refresh_targeting()

## The mission the armed pick is composing for, defaulting to the STRICTER hunt rule so a pending dict
## assembled without one (a harness, a future caller) can never accidentally relax the reach rule.
func _pick_quarry_mission() -> String:
	return String(_pending_pick_quarry.get(PICK_QUARRY_MISSION_KEY,
		HudComposeVocab.COMPOSE_MISSION_HUNT))

func cancel_pick_quarry() -> void:
	if _pending_pick_quarry.is_empty():
		return
	# Only the PICK is cancelled: a quarry chosen earlier stays chosen, so Esc during a re-pick
	# returns the player to the form they already had rather than emptying it.
	_pending_pick_quarry = {}
	_refresh_targeting()

func _try_pick_quarry(tile_info: Dictionary) -> void:
	if _pending_pick_quarry.is_empty() or tile_info.is_empty():
		return
	# Resolve the target from the clicked hex's herds (herd markers occupy the hex, so a click on a
	# herd lands here). Pick the first huntable herd on the tile; if none, keep targeting and nudge.
	var herd := _huntable_herd_on_tile(tile_info)
	var fauna_id := String(herd.get("id", "")).strip_edges()
	if fauna_id == "":
		_note_sink.call("Hunt expedition", "No huntable herd there — click on a herd.")
		return
	# A herd INSIDE the band's hunt reach is a local HUNT, not a hunting party's job. Refuse it here
	# and stay in targeting, exactly like the miss above — and say why, since the reach split is
	# invisible on the map. (MapView doesn't glow these, so this is the belt to that braces.) A DENIAL
	# raid has no such rule (`is_expedition_quarry`), so on that mission this branch is reachable only
	# for a herd whose tile the client cannot resolve at all.
	var band: Dictionary = _pending_pick_quarry.get("band", {})
	var mission := _pick_quarry_mission()
	if not is_expedition_quarry(band, herd, mission):
		var band_tile := SourceForecast.band_tile(band)
		_note_sink.call("Hunt expedition", HudComposeVocab.QUARRY_WITHIN_REACH_FORMAT % [
			SourceForecast.herd_display_name(herd),
			_hex_distance_wrapped(band_tile.x, band_tile.y,
				int(herd.get("x", -1)), int(herd.get("y", -1))),
			String(band.get("id", "this band")),
			int(band.get("hunt_reach", 0)),
		])
		return
	# NO no-surplus check here: no floor is chosen yet, so that verdict is unknowable. It lives
	# entirely on the sheet's Send button, which has every input.
	_pending_pick_quarry = {}
	_refresh_targeting()
	choose_quarry(band, herd, mission)

## **THE ONE ADOPTION OF A QUARRY**, shared by the map pick above and the sheet's own chooser (a hex
## can hold more than one herd, and the map click names only the hex — see
## `BandPanelController._build_quarry_row`). Both routes therefore run the same eligibility test, set
## the same state and re-render the same way; a second spelling is how the two would come to leave the
## composition in different shapes. Answers false — changing nothing — for a herd this band cannot
## send a party to, so a caller holding a stale candidate list cannot install one the sheet would then
## refuse on its next render.
func choose_quarry(band: Dictionary, herd: Dictionary,
		mission: String = HudComposeVocab.COMPOSE_MISSION_HUNT) -> bool:
	var fauna_id := String(herd.get("id", "")).strip_edges()
	if fauna_id == "" or not is_expedition_quarry(band, herd, mission):
		return false
	_compose.set_party_quarry(fauna_id)
	# Fill the party to this herd's max-useful cap at the default floor, same one-shot a preset
	# click sets. Party size is meaningless until the quarry is known (the useful count is a property
	# of the HERD), so picking one is the first moment we CAN default it — "give me everyone this raid
	# can use". Consumed on the next render before the clamp; a −/+ tick still overrides freely.
	_compose.arm_party_autofill()
	_rerender_band_panel_fn.call()
	return true

## Is `herd` a valid quarry for a DETACHED party from `band` on `mission`? THE single definition — the
## pick, the sheet's re-validation, the tile chooser and MapView's glow all route through it (the map
## must never promise a target the pick refuses). Wrap-aware, measured from the band's own tile. An
## unknown distance (missing tiles) is NEVER a quarry, on any mission.
##
## **THE BEYOND-REACH RULE BELONGS TO THE HUNT, NOT TO THE EXPEDITION**, which is why the mission is a
## parameter rather than a second definition living somewhere else. A HUNTING party exists precisely
## for game the band cannot work from home, so a nearer herd is a local hunt — the same split the herd
## drawer makes between "Hunt Here" and its expedition branch — and that rule is unchanged. A DENIAL
## raid is not a way of getting food: it is a way of ERASING a herd, and wanting to break the warren
## next door is a coherent order that hunting it at floor 0 cannot express (a hunt is carry-bounded and
## stops at the pack). So denial may target any herd the band can see and reach, in reach or not.
func is_expedition_quarry(band: Dictionary, herd: Dictionary,
		mission: String = HudComposeVocab.COMPOSE_MISSION_HUNT) -> bool:
	var band_tile := SourceForecast.band_tile(band)
	var distance := _hex_distance_wrapped(
		band_tile.x, band_tile.y, int(herd.get("x", -1)), int(herd.get("y", -1)))
	return distance > quarry_min_distance(band, mission)

## The distance a quarry must lie STRICTLY beyond for `mission` — the ONE number both halves of the
## rule are expressed in, so `is_expedition_quarry` and the `min_distance` MapView glows by are
## literally the same value rather than two derivations of it.
##
## Missions are tested for the one that RELAXES the rule, so an unrecognised mission string keeps the
## hunt's stricter bound: the failure mode of the exclusion is a refused pick the player can see, and
## of the inclusion a silently relaxed hunt. Floored at `QUARRY_NO_REACH_BOUND` so `distance > min`
## always implies a KNOWN distance, which is what lets the one comparison carry both rules.
func quarry_min_distance(band: Dictionary, mission: String) -> int:
	if mission == HudComposeVocab.COMPOSE_MISSION_DENY:
		return QUARRY_NO_REACH_BOUND
	return maxi(int(band.get("hunt_reach", 0)), QUARRY_NO_REACH_BOUND)

## Every herd on `(x, y)` this band could send a party to, in the snapshot's own order — the candidate
## set the compose sheet's quarry chooser offers when a hex holds more than one.
##
## **It is derived LIVE from `world_herds`, not stashed at the pick**, for the reason the sheet
## re-resolves its quarry every render: herds migrate, so a set captured when the click landed would
## go on offering a herd that has walked off the tile. It is the same array `tile_info.herds` is built
## from (`Hud.update_herds` and `MapView._herds_on_tile` both read the snapshot's `herds`), so the
## click's own resolution and this list cannot disagree about what is standing there.
##
## Eligibility runs through `is_expedition_quarry` like every other quarry question, for the SAME
## `mission` the sheet asking is composing — a denial sheet whose quarry is in reach would otherwise
## offer a chooser that filtered out the very herd standing beside it. On any one tile that test is
## uniform — it reads only the herd's own x/y — so the filter either keeps the whole hex or drops it;
## it is here because THIS is the definition, not because the answers could differ.
func eligible_quarries_on_tile(band: Dictionary, x: int, y: int,
		mission: String = HudComposeVocab.COMPOSE_MISSION_HUNT) -> Array:
	var candidates: Array = []
	if x < 0 or y < 0:
		return candidates
	for herd_variant in _band_labor.world_herds():
		if not (herd_variant is Dictionary):
			continue
		var herd: Dictionary = herd_variant as Dictionary
		if int(herd.get("x", -1)) != x or int(herd.get("y", -1)) != y:
			continue
		if not bool(herd.get("huntable", false)):
			continue
		if String(herd.get("id", "")).strip_edges() == "":
			continue
		if not is_expedition_quarry(band, herd, mission):
			continue
		candidates.append(herd)
	return candidates

## The first huntable herd DICT on a hex's tile_info, or {} when there is none. The target click
## resolves its id from this.
func _huntable_herd_on_tile(tile_info: Dictionary) -> Dictionary:
	var herds_variant: Variant = tile_info.get("herds", [])
	if not (herds_variant is Array):
		return {}
	for herd_variant in (herds_variant as Array):
		if herd_variant is Dictionary and bool((herd_variant as Dictionary).get("huntable", false)):
			var herd: Dictionary = herd_variant as Dictionary
			if String(herd.get("id", "")).strip_edges() != "":
				return herd
	return {}

# ---- Dispatch ------------------------------------------------------------------------------------

## Try to resolve every armed flow against a clicked tile, in the SAME order as before (move-band,
## send-expedition, quarry-pick). HudLayer's `show_tile_selection` / `notify_hex_selected` call this.
func try_dispatch(tile_info: Dictionary) -> void:
	_try_dispatch_pending_move_band(tile_info)
	_try_dispatch_pending_send_expedition(tile_info)
	_try_pick_quarry(tile_info)

## Wrap-aware odd-r hex distance between two offset tiles, supplying the snapshot's grid geometry to
## the ONE implementation (`SourceForecast.hex_distance_wrapped`). The grid pair lives on `_band_labor`
## (fed by HudLayer.set_grid_dimensions). -1 for an unknown tile.
func _hex_distance_wrapped(a_col: int, a_row: int, b_col: int, b_row: int) -> int:
	return SourceForecast.hex_distance_wrapped(
		a_col, a_row, b_col, b_row, _band_labor.grid_width(), _band_labor.wrap_horizontal())
