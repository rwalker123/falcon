extends Node2D

const SnapshotLoader = preload("res://src/scripts/SnapshotLoader.gd")
const CommandClient = preload("res://src/scripts/CommandClient.gd")
const ScriptHostManager = preload("res://src/scripts/scripting/ScriptHostManager.gd")
const LocalizationStore = preload("res://src/scripts/LocalizationStore.gd")
const ServerPortsFile = preload("res://src/scripts/ServerPortsFile.gd")

@onready var map_view: Node2D = $MapLayer
@onready var hud: CanvasLayer = $HUD
@onready var camera: Camera2D = $Camera2D
@onready var inspector: CanvasLayer = $Inspector
@onready var band_city_panel: CanvasLayer = $BandCityPanel

var snapshot_loader: SnapshotLoader
var playback_timer: Timer
var streaming_mode: bool = false
var stream_connection_timer: float = 0.0
var command_client: CommandClient
var _warned_stream_fallback: bool = false
var _warned_missing_map_view_method: bool = false
var _camera_initialized: bool = false
var script_host_manager: ScriptHostManager = null
var localization_store: LocalizationStore = null
var _campaign_label_signature: String = ""
var _victory_analytics_signature: String = ""
# Reserved-edge registry (id → {edge, size}), mirrored from `_apply_reservation` so co-edge
# panels can be STACKED (not just summed): the Band panel is offset inboard by the Σ sizes of
# lower-priority reservers on its edge. The map/HUD inset still uses the per-edge SUM (owned by
# MapView/Hud), which is unchanged — this registry only drives the Band panel's leading offset.
var _reservations: Dictionary = {}

const MOCK_DATA_PATH = "res://src/data/mock_snapshots.json"
const TURN_INTERVAL_SECONDS = 1.5
# Stream from the live server by DEFAULT. This used to be false, which meant any launch that
# did not explicitly set STREAM_ENABLED=true rendered `mock_snapshots.json` instead of the
# simulation — scripts/run_stack.sh sets it, so dev never noticed, but the packaged playtest
# build does not, so every package shipped in demo mode. Falling back to mock data is still the
# behaviour when the connection FAILS (see _ready), so a client launched with no server running
# degrades exactly as before; it just no longer ignores a server that IS there.
const STREAM_DEFAULT_ENABLED = true
const STREAM_HOST = "127.0.0.1"
const STREAM_PORT = 41002
const STREAM_CONNECTION_TIMEOUT = 5.0
const CAMERA_PAN_SPEED = 220.0
const COMMAND_HOST = "127.0.0.1"
const COMMAND_PORT = 41001
const PLAYER_FACTION_ID = 0
const SNAPSHOT_DELTA_FIELDS := [
    "influencer_updates",
    "population_updates",
    "tile_updates",
    "trade_link_updates",
    "influencer_removed",
    "population_removed"
]

func _ready() -> void:
    # Force content scale mode to handle high DPI and ultrawide monitors
    get_window().content_scale_mode = Window.CONTENT_SCALE_MODE_CANVAS_ITEMS
    get_window().content_scale_aspect = Window.CONTENT_SCALE_ASPECT_EXPAND
    
    # Ensure HUD and Inspector render above the map layer
    if hud != null:
        hud.layer = 101
    if inspector != null:
        inspector.layer = 102

    var ext: Resource = load("res://native/shadow_scale_godot.gdextension")
    if ext == null:
        push_warning("ShadowScale Godot extension not found; streaming disabled.")
    snapshot_loader = SnapshotLoader.new()
    snapshot_loader.load_mock_data(MOCK_DATA_PATH)
    localization_store = LocalizationStore.new()
    localization_store.load_default()
    var stream_enabled: bool = _determine_stream_enabled()
    var stream_host: String = _determine_stream_host()
    var stream_port: int = _determine_stream_port()
    print("[Endpoints] stream=%s:%d (enabled=%s)" % [stream_host, stream_port, stream_enabled])
    if stream_enabled:
        var err: Error = snapshot_loader.enable_stream(stream_host, stream_port)
        if err == OK:
            streaming_mode = true
            _warned_stream_fallback = false
        else:
            push_warning("Godot client: unable to connect to snapshot stream (error %d). Using mock data." % err)
    set_process(true)
    if not streaming_mode:
        _ensure_timer()
    var command_host: String = _determine_command_host()
    var command_port: int = _determine_command_port()
    var command_proto_port: int = _determine_command_proto_port()
    print("[Endpoints] command=%s:%d (proto=%d)" % [command_host, command_port, command_proto_port])
    command_client = CommandClient.new()
    command_client.set_proto_port(command_proto_port)
    var command_err: Error = command_client.connect_to_host(command_host, command_port)
    if command_err == OK:
        command_client.poll()  # poll to update status
    if command_err != OK:
        push_warning("Godot client: unable to connect to command port (error %d)." % command_err)
    if inspector != null and inspector.has_method("set_command_client"):
        inspector.call("set_command_client", command_client, command_err == OK)
    if inspector != null and inspector.has_method("set_hud_layer"):
        inspector.call("set_hud_layer", hud)
    script_host_manager = ScriptHostManager.new()
    add_child(script_host_manager)
    script_host_manager.setup(command_client)
    if inspector != null and inspector.has_method("attach_script_host"):
        inspector.call("attach_script_host", script_host_manager)
    _hud_invoke("set_localization_store", [localization_store])

    # Wire HUD reference to MapView for embedded minimap (must happen before first snapshot)
    if map_view != null and map_view.has_method("set_hud_reference") and hud != null:
        map_view.call("set_hud_reference", hud)

    var initial: Dictionary = {}
    if streaming_mode and not snapshot_loader.last_stream_snapshot.is_empty():
        initial = snapshot_loader.last_stream_snapshot
    else:
        initial = snapshot_loader.current()
    _apply_snapshot(initial)
    if hud != null:
        if hud.has_signal("cancel_order_requested") and not hud.is_connected("cancel_order_requested", Callable(self, "_on_hud_cancel_order")):
            hud.connect("cancel_order_requested", Callable(self, "_on_hud_cancel_order"))
        if hud.has_signal("assign_labor_requested") and not hud.is_connected("assign_labor_requested", Callable(self, "_on_hud_assign_labor")):
            hud.connect("assign_labor_requested", Callable(self, "_on_hud_assign_labor"))
        if hud.has_signal("move_band_requested") and not hud.is_connected("move_band_requested", Callable(self, "_on_hud_move_band")):
            hud.connect("move_band_requested", Callable(self, "_on_hud_move_band"))
        if hud.has_signal("send_expedition_requested") and not hud.is_connected("send_expedition_requested", Callable(self, "_on_hud_send_expedition")):
            hud.connect("send_expedition_requested", Callable(self, "_on_hud_send_expedition"))
        if hud.has_signal("send_hunt_expedition_requested") and not hud.is_connected("send_hunt_expedition_requested", Callable(self, "_on_hud_send_hunt_expedition")):
            hud.connect("send_hunt_expedition_requested", Callable(self, "_on_hud_send_hunt_expedition"))
        if hud.has_signal("recall_expedition_requested") and not hud.is_connected("recall_expedition_requested", Callable(self, "_on_hud_recall_expedition")):
            hud.connect("recall_expedition_requested", Callable(self, "_on_hud_recall_expedition"))
        if hud.has_signal("extend_pen_requested") and not hud.is_connected("extend_pen_requested", Callable(self, "_on_hud_extend_pen")):
            hud.connect("extend_pen_requested", Callable(self, "_on_hud_extend_pen"))
        if hud.has_signal("next_turn_requested") and not hud.is_connected("next_turn_requested", Callable(self, "_on_hud_next_turn")):
            hud.connect("next_turn_requested", Callable(self, "_on_hud_next_turn"))
        if hud.has_signal("roster_occupant_selected") and not hud.is_connected("roster_occupant_selected", Callable(self, "_on_hud_roster_occupant_selected")):
            hud.connect("roster_occupant_selected", Callable(self, "_on_hud_roster_occupant_selected"))
    if inspector != null and inspector.has_method("attach_map_view"):
        inspector.call("attach_map_view", map_view)
    if map_view != null and inspector != null and map_view.has_signal("hex_selected") and inspector.has_method("focus_tile_from_map"):
        map_view.connect("hex_selected", Callable(inspector, "focus_tile_from_map"))
    if map_view != null:
        if map_view.has_signal("unit_selected") and not map_view.is_connected("unit_selected", Callable(self, "_on_map_unit_selected")):
            map_view.connect("unit_selected", Callable(self, "_on_map_unit_selected"))
        if map_view.has_signal("herd_selected") and not map_view.is_connected("herd_selected", Callable(self, "_on_map_herd_selected")):
            map_view.connect("herd_selected", Callable(self, "_on_map_herd_selected"))
        if map_view.has_signal("herd_quick_hunt_requested") and not map_view.is_connected("herd_quick_hunt_requested", Callable(self, "_on_map_herd_quick_hunt")):
            map_view.connect("herd_quick_hunt_requested", Callable(self, "_on_map_herd_quick_hunt"))
        if map_view.has_signal("selection_cleared") and not map_view.is_connected("selection_cleared", Callable(self, "_on_map_selection_cleared")):
            map_view.connect("selection_cleared", Callable(self, "_on_map_selection_cleared"))
        if map_view.has_signal("tile_selected"):
            if hud != null and hud.has_method("show_tile_selection") and not map_view.is_connected("tile_selected", Callable(self, "_on_map_tile_selected")):
                map_view.connect("tile_selected", Callable(self, "_on_map_tile_selected"))
            if hud != null and hud.has_method("notify_hex_selected") and not map_view.is_connected("tile_selected", Callable(hud, "notify_hex_selected")):
                map_view.connect("tile_selected", Callable(hud, "notify_hex_selected"))
        if map_view.has_signal("tile_hovered") and hud != null and hud.has_method("show_tooltip"):
            if not map_view.is_connected("tile_hovered", Callable(hud, "show_tooltip")):
                map_view.connect("tile_hovered", Callable(hud, "show_tooltip"))
        # Targeting mode: HUD publishes the active target request; the map draws
        # the reticle / valid-target glow, and routes Esc/right-click cancels back.
        if hud != null and hud.has_signal("targeting_changed") and map_view.has_method("set_targeting"):
            if not hud.is_connected("targeting_changed", Callable(map_view, "set_targeting")):
                hud.connect("targeting_changed", Callable(map_view, "set_targeting"))
        if hud != null and map_view.has_signal("targeting_cancel_requested") and hud.has_method("cancel_active_targeting"):
            if not map_view.is_connected("targeting_cancel_requested", Callable(hud, "cancel_active_targeting")):
                map_view.connect("targeting_cancel_requested", Callable(hud, "cancel_active_targeting"))
        if hud != null and hud.has_signal("alert_focus_requested") and map_view.has_method("focus_and_select_tile"):
            if not hud.is_connected("alert_focus_requested", Callable(map_view, "focus_and_select_tile")):
                hud.connect("alert_focus_requested", Callable(map_view, "focus_and_select_tile"))
        # Map-zoom rail (bottom-left nav cluster): the ＋/－/⊡ buttons and the live
        # zoom readout all ride the single MapView._apply_zoom path.
        if hud != null and hud.has_signal("map_zoom_step") and map_view.has_method("zoom_step"):
            if not hud.is_connected("map_zoom_step", Callable(map_view, "zoom_step")):
                hud.connect("map_zoom_step", Callable(map_view, "zoom_step"))
        if hud != null and hud.has_signal("map_zoom_fit") and map_view.has_method("fit_to_view"):
            if not hud.is_connected("map_zoom_fit", Callable(map_view, "fit_to_view")):
                hud.connect("map_zoom_fit", Callable(map_view, "fit_to_view"))
        if hud != null and map_view.has_signal("zoom_changed") and hud.has_method("set_zoom_readout"):
            if not map_view.is_connected("zoom_changed", Callable(hud, "set_zoom_readout")):
                map_view.connect("zoom_changed", Callable(hud, "set_zoom_readout"))
            # Seed the readout once from the current factor (no zoom event has fired yet).
            _hud_invoke("set_zoom_readout", [map_view.zoom_factor])
        # Optimistic pending-labor: HUD publishes the per-band pending map, MapView draws the
        # dashed-amber pending hexes for the selected band.
        if hud != null and hud.has_signal("labor_pending_changed") and map_view.has_method("set_labor_pending"):
            if not hud.is_connected("labor_pending_changed", Callable(map_view, "set_labor_pending")):
                hud.connect("labor_pending_changed", Callable(map_view, "set_labor_pending"))
    if map_view != null and map_view.has_signal("overlay_legend_changed") and hud != null and hud.has_method("update_overlay_legend"):
        map_view.connect("overlay_legend_changed", Callable(self, "_on_overlay_legend_changed"))
        if map_view.has_method("refresh_overlay_legend"):
            map_view.call_deferred("refresh_overlay_legend")
    if inspector != null and inspector.has_method("set_streaming_active"):
        inspector.call("set_streaming_active", streaming_mode)
    _ensure_action_binding("toggle_inspector", Key.KEY_I)
    _ensure_action_binding("toggle_legend", Key.KEY_L)
    _ensure_action_binding("toggle_fow", Key.KEY_F)
    if inspector != null and inspector.has_signal("reserved_width_changed") and not inspector.is_connected("reserved_width_changed", Callable(self, "_on_inspector_reserved_width_changed")):
        inspector.connect("reserved_width_changed", Callable(self, "_on_inspector_reserved_width_changed"))
    if inspector != null and inspector.has_method("reserved_width"):
        _apply_reservation(&"inspector", SIDE_LEFT, float(inspector.call("reserved_width")))
    _connect_band_city_panel()

func _ensure_timer() -> void:
    if is_instance_valid(playback_timer):
        return
    playback_timer = Timer.new()
    playback_timer.wait_time = TURN_INTERVAL_SECONDS
    playback_timer.one_shot = false
    playback_timer.autostart = true
    add_child(playback_timer)
    playback_timer.timeout.connect(_on_tick)

func _on_tick() -> void:
    var snapshot: Dictionary = snapshot_loader.advance()
    _apply_snapshot(snapshot)

func _apply_snapshot(snapshot: Dictionary) -> void:
    if snapshot.is_empty():
        return
    var is_delta := _snapshot_is_delta(snapshot)
    _update_campaign_label(snapshot.get("campaign_label", {}))
    var metrics: Dictionary = {}
    if map_view != null and map_view.has_method("display_snapshot"):
        var metrics_variant: Variant = map_view.call("display_snapshot", snapshot)
        metrics = metrics_variant if metrics_variant is Dictionary else {}
    elif not _warned_missing_map_view_method:
        push_warning("Map view missing display_snapshot(); skipping map render.")
        _warned_missing_map_view_method = true
    _hud_invoke("update_overlay", [snapshot.get("turn", 0), metrics])
    if snapshot.has("server_build"):
        _hud_invoke("update_build_info", [String(snapshot["server_build"])])
    if snapshot.has("faction_inventory"):
        _hud_invoke("update_stockpiles", [snapshot["faction_inventory"]])
    if snapshot.has("sedentarization"):
        _hud_invoke("update_sedentarization", [snapshot["sedentarization"]])
    if snapshot.has("demographics"):
        _hud_invoke("update_demographics", [snapshot["demographics"]])
    if snapshot.has("intensification_knowledge"):
        _hud_invoke("update_intensification", [snapshot["intensification_knowledge"]])
    if snapshot.has("discovered_sites"):
        _hud_invoke("update_discoveries", [snapshot["discovered_sites"]])
    if snapshot.has("grid"):
        _hud_invoke("set_grid_dimensions", [snapshot["grid"]])
    if snapshot.has("food_modules"):
        # Forward MapView's ingested food sites (each stamped with terrain_id) rather than the raw wire
        # array, so the HUD Forage-row glyph resolves the SAME terrain-aware icon the map marker draws
        # (riverine_delta splits fish↔reeds by terrain — see FoodIcons). display_snapshot ran above.
        var food_sites: Variant = map_view.food_sites if map_view != null else snapshot["food_modules"]
        _hud_invoke("update_food_modules", [food_sites])
    if snapshot.has("herds"):
        # The HUD needs the live herd positions (herds migrate) to jump the map to a hunted herd
        # from the band panel's Current-actions rows, and to name it. Same array MapView renders.
        _hud_invoke("update_herds", [snapshot["herds"]])
    if snapshot.has("forage_patches"):
        # The HUD needs the forage patches to cap each Current-actions Forage row's worker stepper at
        # the patch's max-useful (the same forecast the compose control reads off tile_info). Same
        # array MapView ingests into `forage_patch_lookup`.
        _hud_invoke("update_forage_patches", [snapshot["forage_patches"]])
    if snapshot.has("populations"):
        _hud_invoke("update_band_alerts", [snapshot["populations"]])
    if not is_delta:
        _hud_invoke("reset_command_feed")
    if snapshot.has("command_events"):
        _hud_invoke("ingest_command_events", [snapshot["command_events"]])
    if snapshot.has("victory"):
        var victory_variant: Variant = snapshot["victory"]
        if victory_variant is Dictionary:
            _hud_invoke("update_victory_state", [victory_variant])
            _emit_victory_analytics(victory_variant)
    if inspector != null:
        if is_delta:
            if inspector.has_method("update_delta"):
                inspector.call("update_delta", snapshot)
        else:
            if inspector.has_method("update_snapshot"):
                inspector.call("update_snapshot", snapshot)
        if snapshot.has("capability_flags"):
            if inspector.has_method("update_capability_flags"):
                inspector.call("update_capability_flags", int(snapshot["capability_flags"]))
        if inspector.has_method("set_streaming_active"):
            inspector.call("set_streaming_active", streaming_mode)
    _refresh_hud_selection()
    _camera_initialized = true
    if script_host_manager != null and script_host_manager.has_host():
        if is_delta:
            script_host_manager.handle_delta(snapshot)
        else:
            script_host_manager.handle_snapshot(snapshot)

func _emit_victory_analytics(data: Dictionary) -> void:
    if data.is_empty():
        return
    var winner_variant: Variant = data.get("winner", {})
    if not (winner_variant is Dictionary):
        return
    var winner: Dictionary = winner_variant
    var mode: String = String(winner.get("mode", "")).strip_edges()
    if mode == "":
        return
    var tick: int = int(winner.get("tick", -1))
    var signature := "%s#%d" % [mode, tick]
    if signature == _victory_analytics_signature:
        return
    _victory_analytics_signature = signature
    var label: String = String(winner.get("label", mode)).strip_edges()
    if label == "":
        label = mode
    var faction: int = int(winner.get("faction", -1))
    print("[analytics] victory mode=\"%s\" label=\"%s\" faction=%d tick=%d" % [mode, label, faction, tick])

## After each snapshot re-renders the map, refresh the HUD selection panel with the
## selected occupant's/tile's fresh data so it stays live across turn advances instead
## of going stale until the user reselects the hex. Routes through `reapply_selection`,
## NOT the click handlers, so it never re-consumes pending forage/scout/hunt/follow.
func _refresh_hud_selection() -> void:
    if map_view == null or hud == null or not map_view.has_method("refresh_selection_payload"):
        return
    var payload_variant: Variant = map_view.call("refresh_selection_payload")
    if not (payload_variant is Dictionary):
        return
    var payload: Dictionary = payload_variant
    _hud_invoke("reapply_selection", [String(payload.get("kind", "none")), payload.get("data", {})])

func _on_map_unit_selected(unit: Dictionary) -> void:
    _hud_invoke("show_unit_selection", [unit])

func _on_map_herd_selected(herd: Dictionary) -> void:
    _hud_invoke("show_herd_selection", [herd])

## Roster-row selection in the HUD Occupants card drives the map selection ring to
## the chosen band/herd (no hex click).
func _on_hud_roster_occupant_selected(kind: String, id: Variant) -> void:
    if map_view != null and map_view.has_method("select_occupant"):
        map_view.call("select_occupant", kind, id)

## Double-click a herd on the map → the HUD assigns the player band's idle workers to
## hunt it (Sustain). All the band/idle-worker resolution lives in the HUD.
func _on_map_herd_quick_hunt(herd_id: String) -> void:
    _hud_invoke("quick_assign_hunters", [herd_id])

func _on_map_selection_cleared() -> void:
    _hud_invoke("clear_selection")

func _on_map_tile_selected(tile_info: Dictionary) -> void:
    _hud_invoke("show_tile_selection", [tile_info])
    _hud_invoke("notify_hex_selected", [tile_info])

func _on_hud_cancel_order(band: Dictionary) -> void:
    var band_bits := int(band.get("entity", -1))
    if band_bits < 0:
        return
    var faction := int(band.get("faction", PLAYER_FACTION_ID))
    # cancel_order is repurposed (Early-Game Labor slice 3a) to clear ALL of a band's
    # labor assignments — the "Clear all" affordance returns the band fully idle.
    var line := "cancel_order %d %d" % [faction, band_bits]
    _send_runtime_command(line, "Clear all labor assignments for band.")

## Early-Game Labor (slice 3b): assign/unassign working-age workers to a source or a
## band-wide role. workers==0 removes/zeroes the assignment; the server clamps totals
## to available working-age. Payload built by the HUD allocation panel / assign controls.
func _on_hud_assign_labor(payload: Dictionary) -> void:
    var band_bits := int(payload.get("band", -1))
    if band_bits < 0:
        return
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var kind := String(payload.get("kind", "")).strip_edges().to_lower()
    var workers: int = max(0, int(payload.get("workers", 0)))
    var line := ""
    var message := ""
    match kind:
        "forage":
            var fx := int(payload.get("x", -1))
            var fy := int(payload.get("y", -1))
            if fx < 0 or fy < 0:
                return
            var fpolicy := String(payload.get("policy", "sustain")).strip_edges().to_lower()
            if fpolicy == "":
                fpolicy = "sustain"
            line = "assign_labor %d %d forage %d %d %s %d" % [faction, band_bits, fx, fy, fpolicy, workers]
            message = "Assign %d forager%s to (%d, %d) (%s)." % [workers, "" if workers == 1 else "s", fx, fy, fpolicy]
        "hunt":
            var herd_id := String(payload.get("herd_id", "")).strip_edges()
            if herd_id == "":
                return
            var policy := String(payload.get("policy", "sustain")).strip_edges().to_lower()
            if policy == "":
                policy = "sustain"
            line = "assign_labor %d %d hunt %s %s %d" % [faction, band_bits, herd_id, policy, workers]
            message = "Assign %d hunter%s to %s (%s)." % [workers, "" if workers == 1 else "s", herd_id, policy]
        "scout", "warrior":
            line = "assign_labor %d %d %s %d" % [faction, band_bits, kind, workers]
            message = "Assign %d worker%s to %s." % [workers, "" if workers == 1 else "s", kind]
        _:
            return
    _send_runtime_command(line, message)

## Early-Game Labor (slice 3b): relocate the band to a destination tile picked on the map.
func _on_hud_move_band(payload: Dictionary) -> void:
    var band_bits := int(payload.get("band", -1))
    if band_bits < 0:
        return
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return
    var line := "move_band %d %d %d %d" % [faction, band_bits, x, y]
    _send_runtime_command(line, "Move band to (%d, %d)." % [x, y])

## Scouting expedition (docs/plan_exploration_and_sites.md §2): outfit a party off a resident
## band and send it to a target tile. The server draws the workers + provisions and spawns the
## detached party (rejects an over-cap party with a feed message).
func _on_hud_send_expedition(payload: Dictionary) -> void:
    var band_bits := int(payload.get("band", -1))
    if band_bits < 0:
        return
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var party_workers := int(payload.get("party_workers", 0))
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if party_workers <= 0 or x < 0 or y < 0:
        return
    var line := "send_expedition %d %d %d %d %d" % [faction, band_bits, party_workers, x, y]
    _send_runtime_command(line, "Send scouting expedition (%d) to (%d, %d)." % [party_workers, x, y])

## Hunting expedition (docs/plan_exploration_and_sites.md §2b): outfit a party off a resident band
## and send it to follow a herd. The 4th arg is a herd id string, not tile coords.
func _on_hud_send_hunt_expedition(payload: Dictionary) -> void:
    var band_bits := int(payload.get("band", -1))
    if band_bits < 0:
        return
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var party_workers := int(payload.get("party_workers", 0))
    var fauna_id := String(payload.get("fauna_id", "")).strip_edges()
    if party_workers <= 0 or fauna_id == "":
        return
    # Optional trailing policy (sustain|surplus|market|eradicate); server defaults Sustain if omitted.
    var policy := String(payload.get("policy", "")).strip_edges()
    var line := "send_hunt_expedition %d %d %d %s" % [faction, band_bits, party_workers, fauna_id]
    if policy != "":
        line += " %s" % policy
    # The COMMAND addresses the herd by its id; the FEED NOTE names the species. `game_deer_07` is a
    # database key — meaningless to a player — so it must never reach the feed. Hud sends the display
    # name alongside the key; fall back to the key only if it somehow didn't (better than an empty
    # subject, and it is never the normal path).
    var fauna_label := String(payload.get("fauna_label", "")).strip_edges()
    if fauna_label == "":
        fauna_label = fauna_id
    _send_runtime_command(line, "Send hunting expedition (%d, %s) after %s." % [party_workers, policy if policy != "" else "sustain", fauna_label])

## Extend a built pen by one fenced ring (Grazing 2d-γ). `extend_pen <faction> <x> <y>` targets the
## pen's anchor tile; the server works the ring off over ~25 turns (rejecting at max radius / unowned /
## Herding-unknown with a feed message).
func _on_hud_extend_pen(payload: Dictionary) -> void:
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return
    var line := "extend_pen %d %d %d" % [faction, x, y]
    _send_runtime_command(line, "Extend pen at (%d, %d)." % [x, y])

## Recall an in-flight expedition home (folds workers + provisions back on arrival).
func _on_hud_recall_expedition(payload: Dictionary) -> void:
    var expedition_bits := int(payload.get("expedition", -1))
    if expedition_bits < 0:
        return
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var line := "recall_expedition %d %d" % [faction, expedition_bits]
    _send_runtime_command(line, "Recall expedition.")

func _on_hud_next_turn(steps: int) -> void:
    var clamped_steps: int = max(1, steps)
    var line := "turn %d" % clamped_steps
    var suffix := "s" if clamped_steps != 1 else ""
    _send_runtime_command(line, "Advance %d turn%s." % [clamped_steps, suffix])

func _send_runtime_command(line: String, message: String) -> void:
    if inspector != null and inspector.has_method("send_runtime_command"):
        var result: Variant = inspector.call("send_runtime_command", line, message)
        if result is bool and result:
            return
        push_warning("Command pending or rejected: %s" % line)
    else:
        push_warning("Inspector unavailable; cannot send command: %s" % line)

func skip_to_next_turn() -> void:
    if streaming_mode:
        return
    _apply_snapshot(snapshot_loader.advance())

func skip_to_previous_turn() -> void:
    if streaming_mode:
        return
    _apply_snapshot(snapshot_loader.rewind())

func _unhandled_input(event: InputEvent) -> void:
    if event.is_action_pressed("ui_right"):
        skip_to_next_turn()
    elif event.is_action_pressed("ui_left"):
        skip_to_previous_turn()

func _toggle_inspector_visibility() -> void:
    if inspector == null:
        return
    if inspector.has_method("toggle_panel_visibility"):
        inspector.call("toggle_panel_visibility")
    elif inspector.has_method("set_panel_visible") and inspector.has_method("is_panel_visible"):
        var current_visible: bool = bool(inspector.call("is_panel_visible"))
        inspector.call("set_panel_visible", not current_visible)
    # The inset update arrives via the inspector's reserved_width_changed signal.

## Stable stacking order for co-edge reservers: lower priority sits INBOARD (against the screen
## edge). The Inspector is always the screen-edge reserver; the Band panel stacks outboard of it.
const RESERVER_PRIORITY := {&"inspector": 0, &"band_panel": 1}
const BAND_PANEL_RESERVER := &"band_panel"

## Reserve space for a docked panel by insetting the game area (map + HUD) from
## the given edge, so the panel shrinks the play space instead of overlapping it.
## Fans a reserver's (edge, size) out to both surfaces. `edge` is a Godot Side
## const (SIDE_LEFT/SIDE_TOP/SIDE_RIGHT/SIDE_BOTTOM); `size <= 0` releases it.
func _apply_reservation(id: StringName, edge: int, size: float) -> void:
    if size <= 0.0:
        _reservations.erase(id)
    else:
        _reservations[id] = {"edge": edge, "size": size}
    if map_view != null and map_view.has_method("set_reserved_inset"):
        map_view.call("set_reserved_inset", id, edge, size)
    if hud != null and hud.has_method("set_reserved_inset"):
        hud.call("set_reserved_inset", id, edge, size)
    # Co-edge stacking: push the Band panel's leading offset so it sits just past any inboard
    # reserver on its edge (e.g. the Inspector when both are left) instead of overlapping it.
    _update_band_panel_edge_offset()

## The Band panel's leading offset = Σ sizes of all lower-priority reservers currently on the SAME
## edge as the Band panel (today just the Inspector when both dock left; 0 otherwise). Recomputed
## on every reservation change, so the panel tracks the Inspector's show/hide + live drag-resize.
func _update_band_panel_edge_offset() -> void:
    if band_city_panel == null or not band_city_panel.has_method("set_edge_offset") or not band_city_panel.has_method("get_dock"):
        return
    var band_edge: int = int(band_city_panel.call("get_dock"))
    var band_priority: int = int(RESERVER_PRIORITY.get(BAND_PANEL_RESERVER, 1))
    var offset: float = 0.0
    for other_id in _reservations:
        if other_id == BAND_PANEL_RESERVER:
            continue
        var r: Dictionary = _reservations[other_id]
        if int(r.get("edge", -1)) != band_edge:
            continue
        if int(RESERVER_PRIORITY.get(other_id, 0)) < band_priority:
            offset += float(r.get("size", 0.0))
    band_city_panel.call("set_edge_offset", offset)

func _on_inspector_reserved_width_changed(width: float) -> void:
    _apply_reservation(&"inspector", SIDE_LEFT, width)

## Wire the dockable Band/City panel onto the slice-1 reservation fan-out and seed
## its initial reservation (mirrors the inspector: children _ready before us, so the
## panel's own startup emit is missed — we query its current dock + size here).
func _connect_band_city_panel() -> void:
    if band_city_panel == null:
        return
    if band_city_panel.has_signal("reservation_changed") and not band_city_panel.is_connected("reservation_changed", Callable(self, "_on_band_panel_reservation_changed")):
        band_city_panel.connect("reservation_changed", Callable(self, "_on_band_panel_reservation_changed"))
    # Inject the panel into the HUD (band detail relocates into it) and relay the cycler.
    if hud != null and hud.has_method("set_band_city_panel"):
        hud.call("set_band_city_panel", band_city_panel)
    if band_city_panel.has_signal("cycle_requested") and hud != null and hud.has_method("cycle_panel_band") and not band_city_panel.is_connected("cycle_requested", Callable(hud, "cycle_panel_band")):
        band_city_panel.connect("cycle_requested", Callable(hud, "cycle_panel_band"))
    if band_city_panel.has_signal("subject_activated") and hud != null and hud.has_method("focus_panel_band") and not band_city_panel.is_connected("subject_activated", Callable(hud, "focus_panel_band")):
        band_city_panel.connect("subject_activated", Callable(hud, "focus_panel_band"))
    if band_city_panel.has_method("get_dock") and band_city_panel.has_method("current_reservation_size"):
        _apply_reservation(&"band_panel", int(band_city_panel.call("get_dock")), float(band_city_panel.call("current_reservation_size")))

func _on_band_panel_reservation_changed(edge: int, size: float) -> void:
    _apply_reservation(&"band_panel", edge, size)

func _toggle_legend_visibility() -> void:
    if hud == null:
        return
    if hud.has_method("toggle_legend"):
        _hud_invoke("toggle_legend")

var _fow_active: bool = false

func _toggle_fow_overlay() -> void:
    if map_view == null:
        return
    _fow_active = not _fow_active
    if map_view.has_method("set_fow_enabled"):
        map_view.call("set_fow_enabled", _fow_active)

func _update_campaign_label(raw_value: Variant) -> void:
    var label_dict: Dictionary = {}
    if raw_value is Dictionary:
        label_dict = raw_value.duplicate(true)
    if hud != null and hud.has_method("update_campaign_label"):
        _hud_invoke("update_campaign_label", [label_dict])
    var title_text: String = _resolve_campaign_field(label_dict, "title")
    var subtitle_text: String = _resolve_campaign_field(label_dict, "subtitle")
    var title_key := String(label_dict.get("title_loc_key", ""))
    var subtitle_key := String(label_dict.get("subtitle_loc_key", ""))
    var profile_id := String(label_dict.get("profile_id", ""))
    var signature := "%s|%s|%s|%s|%s" % [
        profile_id,
        title_text,
        subtitle_text,
        title_key,
        subtitle_key
    ]
    if signature == _campaign_label_signature:
        return
    _campaign_label_signature = signature
    if title_text != "" or subtitle_text != "" or title_key != "" or subtitle_key != "":
        print("[analytics] campaign_label title=\"%s\" subtitle=\"%s\" loc_title=\"%s\" loc_subtitle=\"%s\"" % [
            title_text,
            subtitle_text,
            title_key,
            subtitle_key
        ])

func _resolve_campaign_field(label_dict: Dictionary, field: String) -> String:
    var raw_text := String(label_dict.get(field, ""))
    var loc_key_field := "%s_loc_key" % field
    var loc_key := String(label_dict.get(loc_key_field, ""))
    if localization_store != null and loc_key != "":
        var localized: String = localization_store.resolve(loc_key, raw_text)
        if localized.strip_edges() != "":
            return localized
    return raw_text

func _process(delta: float) -> void:
    if Input.is_action_just_pressed("toggle_inspector"):
        _toggle_inspector_visibility()
    if Input.is_action_just_pressed("toggle_legend"):
        _toggle_legend_visibility()
    if Input.is_action_just_pressed("toggle_fow"):
        _toggle_fow_overlay()
    if command_client != null:
        command_client.poll()
        command_client.ensure_connected()
    if streaming_mode:
        var streamed: Dictionary = snapshot_loader.poll_stream(delta)
        if not streamed.is_empty():
            if inspector != null and inspector.has_method("set_streaming_active"):
                inspector.call("set_streaming_active", true)
            _apply_snapshot(streamed)
            stream_connection_timer = 0.0
            _warned_stream_fallback = false
        else:
            var status: int = snapshot_loader.stream_status()
            match status:
                StreamPeerTCP.STATUS_CONNECTED, StreamPeerTCP.STATUS_CONNECTING:
                    stream_connection_timer = 0.0
                _:
                    stream_connection_timer += delta
                    if stream_connection_timer > STREAM_CONNECTION_TIMEOUT:
                        if not _warned_stream_fallback:
                            push_warning("Godot client: snapshot stream unavailable; falling back to mock playback.")
                            _warned_stream_fallback = true
                        streaming_mode = false
                        snapshot_loader.disable_stream()
                        _ensure_timer()
                        stream_connection_timer = 0.0
                        if inspector != null and inspector.has_method("set_streaming_active"):
                            inspector.call("set_streaming_active", false)

func _on_overlay_legend_changed(legend: Dictionary) -> void:
    if hud != null and hud.has_method("update_overlay_legend"):
        _hud_invoke("update_overlay_legend", [legend])

func _ensure_action_binding(action_name: String, keycode: Key) -> void:
    if not InputMap.has_action(action_name):
        InputMap.add_action(action_name)
    var events := InputMap.action_get_events(action_name)
    for event in events:
        if event is InputEventKey:
            var key_event := event as InputEventKey
            if key_event.physical_keycode == keycode or key_event.keycode == keycode:
                return
    var ev := InputEventKey.new()
    ev.physical_keycode = keycode
    ev.keycode = keycode
    InputMap.action_add_event(action_name, ev)

func _snapshot_is_delta(snapshot: Dictionary) -> bool:
    for field in SNAPSHOT_DELTA_FIELDS:
        if snapshot.has(field):
            return true
    return false

func _hud_invoke(method: String, args: Array = []) -> Variant:
    var result: Variant = null
    if hud != null and hud.has_method(method):
        # print("[HUD->Main]", method, args)  # Commented out to reduce log spam
        result = hud.callv(method, args)
    return result

func _determine_stream_enabled() -> bool:
    var env_flag: String = OS.get_environment("STREAM_ENABLED")
    if env_flag != "":
        return env_flag.to_lower() == "true"
    return STREAM_DEFAULT_ENABLED

# Endpoint resolution order is uniform across stream/command/log:
#   explicit env var -> ports file published by the server -> hardcoded default.
# The env var wins so run_stack.sh (which exports explicit hosts/ports) is
# unaffected; the ports file only covers the packaged build, where the server may
# have had to bind a non-default block because the defaults were busy.
func _determine_stream_host() -> String:
    var env_host: String = OS.get_environment("STREAM_HOST")
    if env_host != "":
        return env_host
    var discovered_host: String = ServerPortsFile.get_host()
    if discovered_host != "":
        return discovered_host
    return STREAM_HOST

func _determine_stream_port() -> int:
    var env_port: String = OS.get_environment("STREAM_PORT")
    if env_port != "":
        var parsed: int = int(env_port)
        if parsed > 0:
            return parsed
    # The stream is the FlatBuffers snapshot socket ("snapshot_flat"), not the
    # legacy JSON "snapshot" socket.
    var discovered_port: int = ServerPortsFile.get_port(ServerPortsFile.KEY_SNAPSHOT_FLAT)
    if discovered_port > 0:
        return discovered_port
    return STREAM_PORT

func _determine_command_host() -> String:
    var env_host: String = OS.get_environment("COMMAND_HOST")
    if env_host != "":
        return env_host
    var discovered_host: String = ServerPortsFile.get_host()
    if discovered_host != "":
        return discovered_host
    return COMMAND_HOST

func _determine_command_port() -> int:
    var env_port: String = OS.get_environment("COMMAND_PORT")
    if env_port != "":
        var parsed: int = int(env_port)
        if parsed > 0:
            return parsed
    var discovered_port: int = ServerPortsFile.get_port(ServerPortsFile.KEY_COMMAND)
    if discovered_port > 0:
        return discovered_port
    return COMMAND_PORT

func _determine_command_proto_port() -> int:
    var env_port: String = OS.get_environment("COMMAND_PROTO_PORT")
    if env_port != "":
        var parsed: int = int(env_port)
        if parsed > 0:
            return parsed
    # No explicit COMMAND_PROTO_PORT override: the command endpoint is a single
    # socket, so the protobuf port must follow the resolved command port (COMMAND_PORT
    # env / default) — NOT a stale hardcoded default. run_stack now exports both
    # COMMAND_PORT and COMMAND_PROTO_PORT, but this fallback keeps any launcher that
    # sets only COMMAND_PORT (or a bare --port-base run) correct: without it a
    # non-default port base would send commands to 41001 while the server binds
    # PORT_BASE+1, giving "connection refused" on every command.
    return _determine_command_port()
