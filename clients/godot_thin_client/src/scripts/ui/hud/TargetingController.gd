class_name TargetingController
extends RefCounted

## The COMMAND-TARGETING cluster (HUD decomposition, docs/plan_hud_decomposition.md): the band verbs'
## map picks — move-band (pick a destination TILE), the verb TILE pick Scout and Trade take, and
## pick-quarry (Deny's HERD pick) — plus the floating top-centre targeting banner that guides each.
##
## **THE TARGET COMES FIRST (issue #529).** A verb that composes a sheet arms its pick BEFORE any sheet
## exists; the click resolves the target, writes it onto the pending verb (`ComposeState.set_verb_target`)
## and the sheet then renders in THAT target's drawer. So no pick here dispatches a command except
## Move's, and the scouting send is a direct call (`send_expedition_to`) from a sheet whose tile is
## already chosen.
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
## `targeting_changed` · `move_band_requested` · `send_expedition_requested` · `verb_pick_cancelled`.
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
# A scouting party was sent to a tile — relayed to HudLayer.send_expedition_requested.
signal send_expedition_requested(payload: Dictionary)
# The PLAYER backed out of an armed verb pick (banner Cancel / Esc / right-click). HudLayer routes it
# to `BandPanelController.close_verb_form`, so a cancelled pick leaves no half-composed verb behind.
# NOT emitted by `disarm_verb_picks` — that is the verb's owner tearing its own pick down.
signal verb_pick_cancelled

# --- The quarry rule's own vocabulary -------------------------------------------------------------
## The `min_distance` for a mission with NO beyond-reach rule. `-1` rather than `0`, because the test
## every surface applies is "strictly farther than this": at `0` a herd standing ON the band's own tile
## would fail it, and that herd is a legal denial target. At `-1` every KNOWN distance passes and the
## unknown one (`-1`) still fails, which is the "an unknown distance is never a quarry" half of the
## rule falling out of the same comparison instead of needing a second clause.
const QUARRY_NO_REACH_BOUND := -1
## Where `begin_pick_quarry` files the mission on the pending dict, read back by `_pick_quarry_mission`.
const PICK_QUARRY_MISSION_KEY := "mission"


## **THE TARGETING MODE'S OWN TOKEN, AND THE PLAYER READS IT UPPERCASED.** `_targeting_banner_bbcode`
## prints the `command` it is handed as the banner's lead word, so this string is not plumbing — it is
## the banner (`PREY  Band 1 — click on a herd to hunt`). It is a CLIENT token: no command by this
## name is ever sent, the pick adopts a quarry and nothing more, and MapView keys its halo off
## `need` rather than off this.
##
## ⛔ **`prey`, NOT `quarry` (issue #650)** — the sim's `quarry` verb opens a stone working, so the
## old spelling put the same banner word on hunting a herd and on digging a pit.
const PICK_PREY_COMMAND := "prey"

# --- The verb picks' banners (issue #529) ---------------------------------------------------------
## Each banner's lead word is the verb, and every banner names the band by its NAME
## (`HudFormat.band_name`) — never `Band <id>`, which is a handle, not something a player calls a band.
const DENY_PICK_COMMAND := "deny"
const VERB_PICK_COMMAND_SCOUT := "scout"
const VERB_PICK_COMMAND_TRADE := "trade"
const MOVE_COMMAND := "move"
## The instruction after the band's name, keyed by the banner's command.
const BANNER_INSTRUCTIONS := {
	MOVE_COMMAND: "click a destination tile",
	VERB_PICK_COMMAND_SCOUT: "click a tile to scout toward",
	VERB_PICK_COMMAND_TRADE: "click a band to trade with",
	DENY_PICK_COMMAND: "click a herd to deny",
	PICK_PREY_COMMAND: "click on a herd to hunt",
}
## A verb pick's pending-dict keys.
const VERB_PICK_BAND_KEY := "band"
const VERB_PICK_MISSION_KEY := "mission"
## The note title a quarry pick's refusal posts under, per mission — the hunt title predates the verbs
## and keeps its word; a denial raid is named by its verb.
const HUNT_PICK_NOTE_TITLE := "Hunt expedition"

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
# The verb TILE pick (Scout / Trade): {band, mission} while armed, {} when inactive. It dispatches
# nothing — the click resolves the verb's target onto `_compose` and the sheet opens there.
var _pending_verb_pick: Dictionary = {}
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
			"command": MOVE_COMMAND,
			"need": "tile",
			"origin_x": ox,
			"origin_y": oy,
			"context_label": HudFormat.band_name(_pending_move_band),
		}
	if not _pending_verb_pick.is_empty():
		var band: Dictionary = _pending_verb_pick.get(VERB_PICK_BAND_KEY, {})
		var pos: Array = Array(band.get("pos", []))
		var ox := int(pos[0]) if pos.size() == 2 else int(band.get("current_x", -1))
		var oy := int(pos[1]) if pos.size() == 2 else int(band.get("current_y", -1))
		var trade := String(_pending_verb_pick.get(VERB_PICK_MISSION_KEY, "")) \
			== HudComposeVocab.COMPOSE_MISSION_TRADE
		return {
			"active": true,
			"command": VERB_PICK_COMMAND_TRADE if trade else VERB_PICK_COMMAND_SCOUT,
			"need": "tile",
			"origin_x": ox,
			"origin_y": oy,
			"context_label": HudFormat.band_name(band),
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
		var deny := _pick_quarry_mission() == HudComposeVocab.COMPOSE_MISSION_DENY
		return {
			"active": true,
			"command": DENY_PICK_COMMAND if deny else PICK_PREY_COMMAND,
			"need": "herd",
			"origin_x": ox,
			"origin_y": oy,
			"min_distance": quarry_min_distance(band, _pick_quarry_mission()),
			"context_label": HudFormat.band_name(band),
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
	else:
		instruction = String(BANNER_INSTRUCTIONS.get(String(info.get("command", "")),
			"click a tile to survey"))
	return "[color=#%s]%s[/color]  [color=#%s]%s[/color]%s   [color=#%s]— %s[/color]" % [
		HudStyle.SIGNAL_HEX, cmd, HudStyle.INK_HEX, ctx, loc, HudStyle.INK_DIM_HEX, instruction,
	]

## Cancel the active targeting (banner Cancel / Esc / right-click all route here). **An armed VERB pick
## takes its verb with it** (`verb_pick_cancelled`): a pick is the first step of a verb, so backing out
## of it is backing out of the verb, and a verb left pending with no pick would open its sheet on the
## next unrelated click.
func cancel_active_targeting() -> void:
	var had_verb := not _pending_verb_pick.is_empty() or not _pending_pick_quarry.is_empty()
	_cancel_pending_move_band()
	disarm_verb_picks()
	if had_verb:
		verb_pick_cancelled.emit()

## Take down any armed verb pick WITHOUT announcing a cancel — the verb's owner closing its own verb
## (a sheet's ✕, a send, a new verb pressed over the old one).
func disarm_verb_picks() -> void:
	_cancel_pending_verb_pick()
	cancel_pick_quarry()

# ---- Move-band -----------------------------------------------------------------------------------

## Move-band: enter tile-targeting; the destination click emits move_band_requested. `band` is the
## verb's own band where a surface names one (the Band panel's bar, the drawer's verb row); empty
## falls back to `_resolve_assign_band` — the selected player band, else the panel band.
func begin_move_band(band: Dictionary = {}) -> void:
	# Targeting asks the player to click the map — a sheet floating over it is a trap (§15).
	_drawercompose.close_compose_sheet()
	if band.is_empty():
		band = _resolve_assign_band()
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

# ---- The verb TILE pick (Scout / Trade) ----------------------------------------------------------

## Arm the tile pick for `mission` on `band` — Scout (any tile) or Trade (a tile holding a band this
## one is tied to). The click writes the target onto the pending verb and opens its sheet there.
func begin_verb_pick(band: Dictionary, mission: String) -> void:
	# Targeting asks the player to click the map — a sheet floating over it is a trap (§15).
	_drawercompose.close_compose_sheet()
	if band.is_empty():
		return
	_pending_verb_pick = {VERB_PICK_BAND_KEY: band.duplicate(true), VERB_PICK_MISSION_KEY: mission}
	_refresh_targeting()

func _cancel_pending_verb_pick() -> void:
	if _pending_verb_pick.is_empty():
		return
	_pending_verb_pick = {}
	_refresh_targeting()

func _try_verb_pick(tile_info: Dictionary) -> void:
	if _pending_verb_pick.is_empty() or tile_info.is_empty():
		return
	var x := int(tile_info.get("x", -1))
	var y := int(tile_info.get("y", -1))
	if x < 0 or y < 0:
		return
	var band: Dictionary = _pending_verb_pick.get(VERB_PICK_BAND_KEY, {})
	var destination := HudConst.NO_BAND_ID
	if String(_pending_verb_pick.get(VERB_PICK_MISSION_KEY, "")) == HudComposeVocab.COMPOSE_MISSION_TRADE:
		destination = trade_destination_at(band, x, y)
		if destination == HudConst.NO_BAND_ID:
			# The quarry pick's rule for a miss: say so and stay armed.
			_note_sink.call(HudComposeVocab.TRADE_PICK_MISS_TITLE, HudComposeVocab.TRADE_PICK_MISS_TEXT)
			return
	_pending_verb_pick = {}
	_refresh_targeting()
	_compose.set_verb_target(Vector2i(x, y), "", destination)
	# The selection already landed on this tile (the click that resolved the pick selected it first);
	# the re-render is what mounts the sheet in its drawer.
	_rerender_band_panel_fn.call()

## The durable `band_id` of a band `band` holds a LIVE tie with, standing on (x, y) — or
## `HudConst.NO_BAND_ID`. The candidates are exactly the shipment sheet's: `connections_for_band`,
## live ties only. A tied band still in the roster is found where it stands; one that is not is found
## where the tie last saw it, which is the only position the client has for it.
func trade_destination_at(band: Dictionary, x: int, y: int) -> int:
	for tie_variant in _band_labor.connections_for_band(int(band.get("band_id", HudConst.NO_BAND_ID))):
		var tie: Dictionary = tie_variant as Dictionary
		if not HudBandLaborState.tie_is_live(tie):
			continue
		var subject := int(tie.get("subject_band_id", HudConst.NO_BAND_ID))
		var tile := Vector2i(int(tie.get("last_seen_x", -1)), int(tie.get("last_seen_y", -1)))
		var standing := _band_labor.player_band_by_band_id(subject)
		if not standing.is_empty():
			tile = SourceForecast.band_tile(standing)
		if tile == Vector2i(x, y):
			return subject
	return HudConst.NO_BAND_ID

## Send a scouting party of `party_workers` from `band` to `tile` — the Scout sheet's send. The tile
## was picked BEFORE the sheet opened, so this emits straight away; there is no second map pick.
##
## **THE KIT RIDES THE PAYLOAD** with the job default beside it, because `Main._kit_token` omits the
## tail when the two agree — which is what lets a composition that never touched the picker emit the
## byte-identical line it emitted before the picker existed.
func send_expedition_to(band: Dictionary, party_workers: int, tile: Vector2i,
		kit_id: String = KitRoster.NO_KIT_ID,
		default_kit_id: String = KitRoster.NO_KIT_ID) -> void:
	if band.is_empty() or party_workers <= 0 or tile.x < 0 or tile.y < 0:
		return
	send_expedition_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
		"party_workers": party_workers,
		"x": tile.x,
		"y": tile.y,
		"kit_id": kit_id,
		"default_kit_id": default_kit_id,
	})

# ---- Pick-quarry ---------------------------------------------------------------------------------

## Quarry PICK: enter HERD-targeting so the next map click names the herd the compose sheet is aimed
## at. It dispatches NOTHING — the click writes the herd onto the pending Deny (`ComposeState`) and the
## sheet opens in that herd's drawer, asking for the party size against it.
##
## **THE MISSION RIDES WITH THE PICK** because eligibility is a function of it (`is_expedition_quarry`):
## a hunt's quarry must lie beyond the band's reach and a denial raid's need not. It is carried in the
## pending dict rather than re-asked at the click, so the rule the banner glowed under and the rule the
## click is judged by are the same one.
func begin_pick_quarry(band: Dictionary,
		mission: String = HudComposeVocab.COMPOSE_MISSION_HUNT) -> void:
	# Targeting asks the player to click the map — the tile panel's FLOATING sheet over it is a trap
	# (§15).
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
		_note_sink.call(_pick_note_title(), "No huntable herd there — click on a herd.")
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
		_note_sink.call(_pick_note_title(), HudComposeVocab.PREY_WITHIN_REACH_FORMAT % [
			SourceForecast.herd_display_name(herd),
			_hex_distance_wrapped(band_tile.x, band_tile.y,
				int(herd.get("x", -1)), int(herd.get("y", -1))),
			HudFormat.band_name(band),
			int(band.get("hunt_reach", 0)),
		])
		return
	# NO no-surplus check here: no floor is chosen yet, so that verdict is unknowable. It lives
	# entirely on the sheet's Send button, which has every input.
	_pending_pick_quarry = {}
	_refresh_targeting()
	# **THE CLICK IS THE DENY VERB'S TARGET** (issue #529): the herd's hex anchors the sheet, which opens
	# in that hex's drawer. Written before the adoption, whose re-render is what mounts the sheet.
	if mission == HudComposeVocab.COMPOSE_MISSION_DENY:
		_compose.set_verb_target(Vector2i(int(herd.get("x", -1)), int(herd.get("y", -1))), fauna_id)
	choose_quarry(band, herd, mission)

## The event-dock title a quarry pick's refusal posts under — the verb's own name on a denial raid.
func _pick_note_title() -> String:
	if _pick_quarry_mission() == HudComposeVocab.COMPOSE_MISSION_DENY:
		return String(HudComposeVocab.verb_for_mission(HudComposeVocab.COMPOSE_MISSION_DENY)[
			HudComposeVocab.VERB_KEY_TOOLTIP])
	return HUNT_PICK_NOTE_TITLE

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
	# A pending Deny follows the herd its sheet is now composed against — the `⋯` chooser re-aims it
	# at another herd on the same hex without moving the sheet's anchor.
	if _compose.verb_mission() == mission and mission == HudComposeVocab.COMPOSE_MISSION_DENY:
		_compose.set_verb_herd(fauna_id)
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

## Try to resolve every armed flow against a clicked tile: move-band, the verb tile pick, the quarry
## pick. HudLayer's `show_tile_selection` / `notify_hex_selected` call this.
func try_dispatch(tile_info: Dictionary) -> void:
	_try_dispatch_pending_move_band(tile_info)
	_try_verb_pick(tile_info)
	_try_pick_quarry(tile_info)

## Wrap-aware odd-r hex distance between two offset tiles, supplying the snapshot's grid geometry to
## the ONE implementation (`SourceForecast.hex_distance_wrapped`). The grid pair lives on `_band_labor`
## (fed by HudLayer.set_grid_dimensions). -1 for an unknown tile.
func _hex_distance_wrapped(a_col: int, a_row: int, b_col: int, b_row: int) -> int:
	return SourceForecast.hex_distance_wrapped(
		a_col, a_row, b_col, b_row, _band_labor.grid_width(), _band_labor.wrap_horizontal())
