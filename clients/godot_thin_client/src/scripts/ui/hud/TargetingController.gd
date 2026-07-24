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
##   • `_command_feed` — the two miss/refusal `note()` nudges the quarry pick posts.
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

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
var _band_labor: HudBandLaborState = null
var _compose: ComposeState = null
var _drawercompose: DrawerComposeController = null
var _command_feed: CommandFeedController = null
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
# only the band — party size and policy are chosen in the sheet AFTER the quarry.
var _pending_pick_quarry: Dictionary = {}
var _targeting_banner: PanelContainer = null
var _targeting_banner_label: RichTextLabel = null

func _init(band_labor: HudBandLaborState, compose: ComposeState,
		drawercompose: DrawerComposeController, command_feed: CommandFeedController, host: Node,
		resolve_assign_band: Callable, after_pending_change: Callable,
		rerender_band_panel: Callable) -> void:
	_band_labor = band_labor
	_compose = compose
	_drawercompose = drawercompose
	_command_feed = command_feed
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
		# refuse. Every other targeting mode omits the key and MapView defaults it to 0, which admits
		# everything and so changes nothing for move/scout-tile targeting.
		return {
			"active": true,
			"command": "quarry",
			"need": "herd",
			"origin_x": ox,
			"origin_y": oy,
			"min_distance": int(band.get("hunt_reach", 0)),
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
	var bits := int(band.get("entity", -1))
	move_band_requested.emit({
		"faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
		"band": bits,
		"x": x,
		"y": y,
	})
	_pending_move_band = {}
	_refresh_targeting()
	# Optimistic feedback: mark the destination pending until a newer-turn snapshot confirms.
	_band_labor.record_pending_move(bits, x, y)
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
		"band": int(band.get("entity", -1)),
		"party_workers": int(_pending_send_expedition.get("party_workers", 0)),
		"x": x,
		"y": y,
	})
	_pending_send_expedition = {}
	_refresh_targeting()

# ---- Pick-quarry ---------------------------------------------------------------------------------

## Quarry PICK: enter HERD-targeting so the next map click names the herd the compose sheet is aimed
## at. It dispatches NOTHING — the sheet stays open behind the targeting and fills its Quarry row in,
## then asks for policy and party size against that herd.
func begin_pick_quarry(band: Dictionary) -> void:
	# Targeting asks the player to click the map — the tile panel's FLOATING sheet over it is a trap
	# (§15). The DOCKED party sheet is not floating and deliberately stays open.
	_drawercompose.close_compose_sheet()
	if band.is_empty():
		return
	_pending_pick_quarry = {"band": band.duplicate(true)}
	_refresh_targeting()

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
		_command_feed.note("Hunt expedition", "No huntable herd there — click on a herd.")
		return
	# A herd INSIDE the band's hunt reach is a local hunt, not a party's job. Refuse it here and stay
	# in targeting, exactly like the miss above — and say why, since the reach split is invisible on
	# the map. (MapView doesn't glow these, so this is the belt to that braces.)
	var band: Dictionary = _pending_pick_quarry.get("band", {})
	if not is_expedition_quarry(band, herd):
		var band_tile := SourceForecast.band_tile(band)
		_command_feed.note("Hunt expedition", HudComposeVocab.QUARRY_WITHIN_REACH_FORMAT % [
			SourceForecast.herd_display_name(herd),
			_hex_distance_wrapped(band_tile.x, band_tile.y,
				int(herd.get("x", -1)), int(herd.get("y", -1))),
			String(band.get("id", "this band")),
			int(band.get("hunt_reach", 0)),
		])
		return
	# NO no-surplus check here: no policy is chosen yet, so that verdict is unknowable. It lives
	# entirely on the sheet's Send button, which has every input.
	_compose.set_party_quarry(fauna_id)
	# Fill the party to this herd's max-useful cap for the default policy, same one-shot a policy
	# click sets. Party size is meaningless until the quarry is known (the useful count is a property
	# of the HERD), so picking one is the first moment we CAN default it — "give me everyone this raid
	# can use". Consumed on the next render before the clamp; a −/+ tick still overrides freely.
	_compose.arm_party_autofill()
	_pending_pick_quarry = {}
	_refresh_targeting()
	_rerender_band_panel_fn.call()

## Is `herd` a valid quarry for a DETACHED party from `band`? A hunting party exists precisely for
## game the band cannot work from home, so the answer is the SAME split the herd drawer makes when it
## chooses between "Assign Local Hunt" and the expedition branch: strictly beyond the band's
## `hunt_reach`, wrap-aware, measured from the band's own tile. THE single definition — the pick, the
## sheet's re-validation and MapView's glow all route through it (the map must never promise a target
## the pick refuses). An unknown distance (missing tiles) is NOT a quarry, mirroring the drawer's
## fallback to the local hunt.
func is_expedition_quarry(band: Dictionary, herd: Dictionary) -> bool:
	var band_tile := SourceForecast.band_tile(band)
	var distance := _hex_distance_wrapped(
		band_tile.x, band_tile.y, int(herd.get("x", -1)), int(herd.get("y", -1)))
	return distance >= 0 and distance > int(band.get("hunt_reach", 0))

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
