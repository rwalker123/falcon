extends Node2D

const SnapshotLoader = preload("res://src/scripts/SnapshotLoader.gd")
const CommandClient = preload("res://src/scripts/CommandClient.gd")
const ScriptHostManager = preload("res://src/scripts/scripting/ScriptHostManager.gd")
const LocalizationStore = preload("res://src/scripts/LocalizationStore.gd")
const ServerPortsFile = preload("res://src/scripts/ServerPortsFile.gd")
const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

@onready var map_view: Node2D = $MapLayer
@onready var hud: CanvasLayer = $HUD
@onready var camera: Camera2D = $Camera2D
@onready var inspector: CanvasLayer = $Inspector
@onready var band_city_panel: CanvasLayer = $BandCityPanel
@onready var event_dock: CanvasLayer = $EventDockPanel
@onready var pause_layer: CanvasLayer = $PauseLayer
@onready var pause_menu: MenuShell = $PauseLayer/MenuShell

var snapshot_loader: SnapshotLoader
var streaming_mode: bool = false
var command_client: CommandClient
var _warned_missing_map_view_method: bool = false
var _camera_initialized: bool = false
# Loading gate: hold the loading overlay until a FULL snapshot for a world NEWER than the baseline
# arrives. The server no longer replays a cached frame on connect, but a client that was ALREADY
# connected when the rebuild started can still be handed a pre-rebuild broadcast, so the gate keeps
# rejecting any frame whose world_epoch is <= the baseline captured at _ready and reveals only on the
# rebuild's higher epoch. See _process's streaming block.
var _world_revealed: bool = false
var _reveal_baseline_epoch: int = 0
# The world_epoch of the last FULL snapshot we applied. A change means the snapshot describes a
# DIFFERENT world, which invalidates every per-world client cache — see _reset_per_world_state.
var _world_epoch_applied: int = 0
var loading_overlay: CanvasLayer = null
var script_host_manager: ScriptHostManager = null
var localization_store: LocalizationStore = null
# Per-HUD-method time accumulated during one `_apply_snapshot` fan-out, in microseconds. Filled by
# `_hud_invoke` only while `_hud_profiling` is up (i.e. inside the fan-out block), drained by
# `_record_hud_calls`.
var _hud_call_usec: Dictionary = {}
var _hud_profiling: bool = false
var _victory_analytics_signature: String = ""
# Reserved-edge registry (id → {edge, size}), mirrored from `_apply_reservation` so co-edge
# panels can be STACKED (not just summed): the Band panel is offset inboard by the Σ sizes of
# lower-priority reservers on its edge. The map/HUD inset still uses the per-edge SUM (owned by
# MapView/Hud), which is unchanged — this registry only drives the Band panel's leading offset.
var _reservations: Dictionary = {}
# Pending world-generation command (built from GameLaunch or the dev default) and a sent-once
# latch. Held so it can be retried in _process if the command socket wasn't ready at _ready.
var _new_game_command: Dictionary = {}
var _new_game_sent: bool = false
var _new_game_retry_accum: float = 0.0
var _new_game_elapsed: float = 0.0
# Time since the last ACCEPTED new_game with no world to show for it (see _tick_new_game_retry).
var _new_game_answer_accum: float = 0.0
## Seconds since a `resync` was sent with no full snapshot applied yet; negative means none pending.
var _resync_pending_accum: float = -1.0

# Dev-default world when Main.tscn is launched directly (no landing screen handoff): so a bare
# `godot res://src/Main.tscn` still generates a playable map now that the server boots idle.
const DEV_DEFAULT_NEW_GAME := {
    "preset_id": "earthlike",
    "width": 80,
    "height": 52,
    "seed": 0,
    "profile_id": "late_forager_tribe",
}
const STREAM_HOST = "127.0.0.1"
const STREAM_PORT = 41002

# --- Per-turn client profile (TurnProfile.gd) -------------------------------------------------
# Phase labels for the one `[TurnProfile]` line printed per applied snapshot. Flat and dotted, the
# server's `turn.profile` convention: a parent INCLUDES its children, so `apply` contains all of
# these and `display` contains every `display.*` MapView contributes.
const PROFILE_APPLY := "apply"
const PROFILE_DECODE := "decode"
const PROFILE_DECODE_NATIVE := "decode.native"
const PROFILE_DISPLAY := "display"
const PROFILE_DISPLAY_PREFIX := "display."
const PROFILE_HUD := "hud"
const PROFILE_HUD_PREFIX := "hud."
const PROFILE_INSPECTOR := "inspector"
const PROFILE_SELECTION := "selection"
const PROFILE_SCRIPTING := "scripting"
## Annotates `decode` with how many frames the poll converted and how many of those it then threw
## away — `decode=12.10(x3 discarded 2)`.
const DECODE_NOTE_FORMAT := "x%d discarded %d"
## An individual HUD fan-out call is only worth its own `hud.<method>` entry above this. Below it
## a call is noise against a turn measured in tens of milliseconds, and eighteen such entries would
## bury the line; the `hud` aggregate always reports the whole block regardless.
const HUD_CALL_REPORT_MIN_MSEC := 0.5
# Loading overlay: a CanvasLayer above HUD (101) and Inspector (102), so it fully covers the blank
# map/HUD until the new world reveals.
const LOADING_OVERLAY_LAYER = 150
const LOADING_OVERLAY_TEXT = "Generating world…"
const LOADING_OVERLAY_FONT_SIZE = 28
const COMMAND_HOST = "127.0.0.1"
const COMMAND_PORT = 41001
const PLAYER_FACTION_ID = 0
# Startup map zoom applied on each world reveal ("zoom level 2" = MapView zoom_factor 2.0, on the
# continuous 1.0=cover-fit … 4.0 scale). Named so it stays tunable.
const STARTUP_ZOOM_FACTOR := 2.0
# new_game retry: the command bridge connects synchronously, so the _ready send almost always
# lands; these bound the belt-and-suspenders retry so a permanent rejection (e.g. a sim_runtime
# that doesn't yet parse the verb) can't spam the command log every frame.
const NEW_GAME_RETRY_INTERVAL = 0.5
const NEW_GAME_RETRY_DEADLINE = 5.0
# How long a SENT new_game may go unanswered (no full snapshot for a newer world) before we send it
# again. The two failure modes are NOT symmetric, and this constant is biased hard toward one of
# them: interrupting a healthy generation costs the player a DIFFERENT world than the one already
# being built (a `seed 0` re-roll) plus a second full worldgen, and it would happen on every large-map
# start on a slow machine; waiting too long merely means a rare dropped first frame self-heals late.
# So this is ~7x the MEASURED worst case rather than a snug fit — `new_game.begin`→`new_game.completed`
# for the largest offered map (Huge, 128x80) is 4.4s in a DEBUG build, which is what the client runs.
const NEW_GAME_ANSWER_TIMEOUT := 30.0
## How long a SENT `resync` may go unanswered before we send it again.
##
## Retried until ANSWERED, not until sent — the same reasoning as NEW_GAME_ANSWER_TIMEOUT: a client
## with no applicable baseline renders a frozen world and cannot recover on its own, while a
## redundant `resync` costs the server one full encode. Much shorter than the new_game timeout
## because nothing has to be generated — the server already holds the world and only re-encodes it.
const RESYNC_ANSWER_TIMEOUT := 2.0
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

    # Startup view defaults that must be seated BEFORE the first world renders (the rest — zoom +
    # centre-on-band — need the loaded world and are applied at reveal, see _apply_startup_view):
    #   1. Inspector hidden by default (the player re-opens it with `I`). Done here once so it is
    #      never shown even under the loading overlay, and re-hides on a Main reload (restart).
    if inspector != null and inspector.has_method("set_panel_visible"):
        inspector.call("set_panel_visible", false)
    # (Fog of war is NOT seated here — it is server state now. `MapView._fow_enabled` starts on and
    # is corrected by the first snapshot's `fog_enabled`; see _sync_fog_of_war.)
    ClientSettings.changed.connect(_on_client_settings_changed)

    var ext: Resource = load("res://native/shadow_scale_godot.gdextension")
    if ext == null:
        push_warning("ShadowScale Godot extension not found; streaming disabled.")
    snapshot_loader = SnapshotLoader.new()
    localization_store = LocalizationStore.new()
    localization_store.load_default()
    # Loading gate: capture the last-revealed world epoch (persisted across Main.tscn reloads by the
    # GameLaunch autoload) as the reveal baseline, show the loading overlay, and hold the blank
    # map/HUD behind it until a FULL snapshot with a higher epoch (the freshly generated world)
    # arrives — so a pre-rebuild frame of the world we are replacing can never be shown. The client
    # ALWAYS streams — there is no mock playback fallback.
    var launch_node: Node = get_node_or_null("/root/GameLaunch")
    if launch_node != null:
        _reveal_baseline_epoch = int(launch_node.get("last_world_epoch"))
    _world_revealed = false
    _show_loading_overlay()
    var stream_host: String = _determine_stream_host()
    var stream_port: int = _determine_stream_port()
    print("[Endpoints] stream=%s:%d" % [stream_host, stream_port])
    var err: Error = snapshot_loader.enable_stream(stream_host, stream_port)
    if err != OK:
        # Stay in the loading state — there is no mock fallback. The map reveals only once a live
        # snapshot for the new world arrives (the stream retries via poll/status in _process).
        push_warning("Godot client: unable to connect to snapshot stream (error %d); holding loading screen." % err)
    # The client ALWAYS streams; even on a failed initial connect we hold the loading overlay
    # rather than degrade to a demo playback.
    streaming_mode = true
    set_process(true)
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
    # The server now boots idle and only generates a world on `new_game`; build that command from
    # the landing-screen handoff (or a dev default) and fire it (retried in _process if not yet sent).
    _build_new_game_command()
    _try_send_new_game()
    script_host_manager = ScriptHostManager.new()
    add_child(script_host_manager)
    script_host_manager.setup(command_client)
    if inspector != null and inspector.has_method("attach_script_host"):
        inspector.call("attach_script_host", script_host_manager)
    _hud_invoke("set_localization_store", [localization_store])

    # Wire HUD reference to MapView for embedded minimap (must happen before first snapshot)
    if map_view != null and map_view.has_method("set_hud_reference") and hud != null:
        map_view.call("set_hud_reference", hud)

    # Deliberately apply NO initial snapshot: the map/HUD stay blank behind the loading overlay
    # until the new world's first full snapshot arrives (see the _process reveal gate).
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
        if hud.has_signal("improvement_requested") and not hud.is_connected("improvement_requested", Callable(self, "_on_hud_improvement")):
            hud.connect("improvement_requested", Callable(self, "_on_hud_improvement"))
        if hud.has_signal("answer_fork_requested") and not hud.is_connected("answer_fork_requested", Callable(self, "_on_hud_answer_fork")):
            hud.connect("answer_fork_requested", Callable(self, "_on_hud_answer_fork"))
        if hud.has_signal("next_turn_requested") and not hud.is_connected("next_turn_requested", Callable(self, "_on_hud_next_turn")):
            hud.connect("next_turn_requested", Callable(self, "_on_hud_next_turn"))
        if hud.has_signal("roster_occupant_selected") and not hud.is_connected("roster_occupant_selected", Callable(self, "_on_hud_roster_occupant_selected")):
            hud.connect("roster_occupant_selected", Callable(self, "_on_hud_roster_occupant_selected"))
    if inspector != null and inspector.has_method("set_turn_advance_observer"):
        inspector.call("set_turn_advance_observer", Callable(self, "_on_inspector_turn_advanced"))
    if inspector != null and inspector.has_method("attach_map_view"):
        inspector.call("attach_map_view", map_view)
    if map_view != null and inspector != null and map_view.has_signal("hex_selected") and inspector.has_method("focus_tile_from_map"):
        map_view.connect("hex_selected", Callable(inspector, "focus_tile_from_map"))
    if map_view != null:
        if map_view.has_signal("unit_selected") and not map_view.is_connected("unit_selected", Callable(self, "_on_map_unit_selected")):
            map_view.connect("unit_selected", Callable(self, "_on_map_unit_selected"))
        if map_view.has_signal("herd_selected") and not map_view.is_connected("herd_selected", Callable(self, "_on_map_herd_selected")):
            map_view.connect("herd_selected", Callable(self, "_on_map_herd_selected"))
        if map_view.has_signal("land_selected") and not map_view.is_connected("land_selected", Callable(self, "_on_map_land_selected")):
            map_view.connect("land_selected", Callable(self, "_on_map_land_selected"))
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
        if hud != null and hud.has_signal("faction_knowledge_changed") and map_view.has_method("set_faction_knowledge"):
            if not hud.is_connected("faction_knowledge_changed", Callable(map_view, "set_faction_knowledge")):
                hud.connect("faction_knowledge_changed", Callable(map_view, "set_faction_knowledge"))
    if map_view != null and map_view.has_signal("overlay_legend_changed") and hud != null and hud.has_method("update_overlay_legend"):
        map_view.connect("overlay_legend_changed", Callable(self, "_on_overlay_legend_changed"))
        if map_view.has_method("refresh_overlay_legend"):
            map_view.call_deferred("refresh_overlay_legend")
    if inspector != null and inspector.has_method("set_streaming_active"):
        inspector.call("set_streaming_active", streaming_mode)
    _ensure_action_binding("toggle_inspector", Key.KEY_I)
    _ensure_action_binding("toggle_legend", Key.KEY_L)
    _ensure_action_binding("toggle_victory", Key.KEY_V)
    _ensure_action_binding("toggle_event_dock", Key.KEY_R)
    _ensure_action_binding("toggle_fow", Key.KEY_F)
    if inspector != null and inspector.has_signal("reserved_width_changed") and not inspector.is_connected("reserved_width_changed", Callable(self, "_on_inspector_reserved_width_changed")):
        inspector.connect("reserved_width_changed", Callable(self, "_on_inspector_reserved_width_changed"))
    if inspector != null and inspector.has_method("reserved_width"):
        _apply_reservation(&"inspector", SIDE_LEFT, float(inspector.call("reserved_width")))
    _connect_band_city_panel()
    _connect_event_dock()
    _connect_pause_menu()

## The ESC pause overlay ($PauseLayer): hidden until ESC opens it. Resume hides it, Abandon
## returns to the landing screen, Exit quits. New Game is deliberately absent in pause mode —
## Abandon routes back to the landing screen, which owns the New Game flow.
func _connect_pause_menu() -> void:
    if pause_layer != null:
        pause_layer.visible = false
    if pause_menu == null:
        return
    pause_menu.mode = MenuShell.PAUSE
    if not pause_menu.resume_requested.is_connected(_hide_pause_menu):
        pause_menu.resume_requested.connect(_hide_pause_menu)
    if not pause_menu.abandon_requested.is_connected(_on_pause_abandon):
        pause_menu.abandon_requested.connect(_on_pause_abandon)
    if not pause_menu.exit_requested.is_connected(_on_pause_exit):
        pause_menu.exit_requested.connect(_on_pause_exit)

func _show_pause_menu() -> void:
    if pause_layer != null:
        pause_layer.visible = true

func _hide_pause_menu() -> void:
    if pause_layer != null:
        pause_layer.visible = false

func _on_pause_abandon() -> void:
    get_tree().change_scene_to_file("res://src/ui/LandingScreen.tscn")

func _on_pause_exit() -> void:
    get_tree().quit()

## Build the `new_game <preset> <w> <h> <seed> <profile>` command from the GameLaunch handoff, or
## the dev default when launched directly. Clears the handoff so a later scene reload starts fresh.
func _build_new_game_command() -> void:
    var params: Dictionary = DEV_DEFAULT_NEW_GAME
    var launch: Node = get_node_or_null("/root/GameLaunch")
    if launch != null and launch.get("pending_new_game") is Dictionary:
        params = launch.get("pending_new_game")
        launch.set("pending_new_game", null)
    var preset := String(params.get("preset_id", DEV_DEFAULT_NEW_GAME["preset_id"]))
    var width := maxi(1, int(params.get("width", DEV_DEFAULT_NEW_GAME["width"])))
    var height := maxi(1, int(params.get("height", DEV_DEFAULT_NEW_GAME["height"])))
    # Clamp the seed to >= 0 at the wire boundary: the server parses it as a u64, so a negative
    # seed fails the parse and the world never generates. Catches every caller (GameLaunch + dev
    # default). 0 stays "derive from the run clock".
    var seed_value := maxi(0, int(params.get("seed", DEV_DEFAULT_NEW_GAME["seed"])))
    var profile := String(params.get("profile_id", DEV_DEFAULT_NEW_GAME["profile_id"]))
    _new_game_command = {
        "line": "new_game %s %d %d %d %s" % [preset, width, height, seed_value, profile],
        "message": "New game: %s (%dx%d) seed %d." % [preset, width, height, seed_value],
    }

## Send the pending new_game command through the SAME transport MapPanel uses for map_size
## (inspector.send_runtime_command → command socket). Retried from _process until it lands, so a
## command socket still connecting at _ready doesn't drop the world-generation request.
## `_new_game_command` is deliberately KEPT after a successful send — `_tick_new_game_retry`'s
## answer timeout re-sends the very same line, and clearing it here would leave nothing to re-send.
func _try_send_new_game() -> void:
    if _new_game_sent or _new_game_command.is_empty():
        return
    if inspector == null or not inspector.has_method("send_runtime_command"):
        return
    var result: Variant = inspector.call("send_runtime_command", _new_game_command["line"], _new_game_command["message"])
    if result is bool and result:
        _new_game_sent = true

## Retry the new_game request until it is ANSWERED, not merely SENT. Two phases, in order:
##
##   1. NOT YET SENT — retry the send every NEW_GAME_RETRY_INTERVAL and stop after
##      NEW_GAME_RETRY_DEADLINE, so a permanent rejection (e.g. a sim_runtime that doesn't parse the
##      verb) can't spam the command log every frame. Unchanged.
##   2. SENT, STILL NO WORLD — the transport accepted the command but no full snapshot for a newer
##      world ever came back. Accepting is not answering: the rebuild's broadcast can reach the
##      snapshot server's channel BEFORE the accept thread has added our socket to its client list,
##      and that first frame is then dropped with nothing else coming. After NEW_GAME_ANSWER_TIMEOUT
##      we clear the sent latch so the send path re-fires.
##
## Phase 2 repeats INDEFINITELY (each round re-arms phase 1's own bounded burst) because a
## permanently stuck loading screen is unrecoverable for the player, whereas a re-sent new_game just
## builds another fresh world. The reveal gate below is what makes that safe — it holds until a world
## newer than the baseline arrives, so a duplicate world cannot be shown half-applied.
func _tick_new_game_retry(delta: float) -> void:
    if _world_revealed:
        # The request was answered; there is nothing left to chase.
        return
    if _new_game_command.is_empty():
        return
    if _new_game_sent:
        _new_game_answer_accum += delta
        if _new_game_answer_accum < NEW_GAME_ANSWER_TIMEOUT:
            return
        _new_game_answer_accum = 0.0
        push_warning("new_game went unanswered for %.0fs (no world arrived); re-sending." % NEW_GAME_ANSWER_TIMEOUT)
        _new_game_sent = false
        _new_game_elapsed = 0.0
        return
    _new_game_elapsed += delta
    _new_game_retry_accum += delta
    if _new_game_retry_accum < NEW_GAME_RETRY_INTERVAL:
        return
    _new_game_retry_accum = 0.0
    _try_send_new_game()
    if not _new_game_sent and _new_game_elapsed >= NEW_GAME_RETRY_DEADLINE:
        _new_game_sent = true  # stop retrying this burst; likely a permanent rejection

## Build the fullscreen loading overlay (a CanvasLayer above HUD/Inspector) shown from _ready and
## hidden on world reveal. Dark ground + a centered "Generating world…" label, styled on-brand with
## the dark HUD console look (HudStyle palette). Held until the new world's first full snapshot.
func _show_loading_overlay() -> void:
    if loading_overlay != null:
        loading_overlay.visible = true
        return
    loading_overlay = CanvasLayer.new()
    loading_overlay.layer = LOADING_OVERLAY_LAYER
    var ground := ColorRect.new()
    ground.color = HudStyle.GROUND
    ground.set_anchors_preset(Control.PRESET_FULL_RECT)
    loading_overlay.add_child(ground)
    var label := Label.new()
    label.text = LOADING_OVERLAY_TEXT
    label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
    label.set_anchors_preset(Control.PRESET_FULL_RECT)
    label.add_theme_color_override("font_color", HudStyle.SIGNAL)
    label.add_theme_font_size_override("font_size", LOADING_OVERLAY_FONT_SIZE)
    loading_overlay.add_child(label)
    add_child(loading_overlay)

func _hide_loading_overlay() -> void:
    if loading_overlay != null:
        loading_overlay.visible = false

## Push one decoded snapshot (or delta) through the whole client: map render, HUD fan-out,
## Inspector, selection refresh, script host.
##
## THE PER-TURN PROFILE LIVES HERE because this is the only place that sees all of it. The line is
## emitted at the bottom via `TurnProfile.emit()`; every `profile.begin/end` pair below is inert
## unless the flag is on (see `TurnProfile.ENV_FLAG`).
func _apply_snapshot(snapshot: Dictionary) -> void:
    if snapshot.is_empty():
        return
    var profile := TurnProfile.start()
    var t_apply: int = profile.begin(PROFILE_APPLY)
    _record_decode_phase(profile)
    var is_delta := _snapshot_is_delta(snapshot)
    # WORLD BOUNDARY. A full snapshot carrying a new `world_epoch` describes a DIFFERENT world — a
    # `new_game`, or a `map_size` rebuild, which regenerates in place with no scene reload — and every
    # client cache keyed to the old one is now a lie (a fresh world sends `intensification_knowledge:
    # []`, which MERGES to nothing and leaves the previous game's "⚒ Your people know" strip standing).
    # Reset FIRST, then apply: this same snapshot carries the new world's backfill (the command_events
    # ring, the knowledge rows, the herd list), and resetting after the dispatch would wipe it.
    if not is_delta:
        # A full frame is the answer a pending `resync` was waiting for, whoever caused it.
        _resync_pending_accum = -1.0
    if not is_delta and snapshot.has("world_epoch"):
        var snapshot_epoch := int(snapshot["world_epoch"])
        if snapshot_epoch != _world_epoch_applied:
            _reset_per_world_state()
            _world_epoch_applied = snapshot_epoch
    _sync_fog_of_war(snapshot, is_delta)
    var metrics: Dictionary = {}
    var t_display: int = profile.begin(PROFILE_DISPLAY)
    if map_view != null and map_view.has_method("display_snapshot"):
        var metrics_variant: Variant = map_view.call("display_snapshot", snapshot)
        metrics = metrics_variant if metrics_variant is Dictionary else {}
    elif not _warned_missing_map_view_method:
        push_warning("Map view missing display_snapshot(); skipping map render.")
        _warned_missing_map_view_method = true
    profile.end(PROFILE_DISPLAY, t_display)
    # MapView times its own ingest blocks; splice them in behind `display` as `display.*`.
    if map_view != null:
        profile.absorb(map_view.get("last_display_profile"), PROFILE_DISPLAY_PREFIX)
    var t_hud: int = profile.begin(PROFILE_HUD)
    _hud_profiling = profile.enabled
    # Every call below pairs `snapshot.has(key)` with the change manifest, and needs BOTH.
    #
    # `has()` alone stopped being a change signal when the client began rendering from merged delta
    # frames: the decoder patches its cached world and republishes it whole, so every key is now
    # present on every frame and every one of these fired every turn. It is still the right OUTER
    # guard — an absent key means the frame never carried the section at all, and for the ones whose
    # comments call the guard load-bearing (`pending_forks`) absence must keep meaning "unchanged,
    # do not clear". `SnapshotSections.changed` supplies what `has()` no longer can, and answers
    # `true` for any frame with no manifest, so a full snapshot still fans out everything.
    #
    # `populations` and `herds` move on essentially every turn, so `update_band_alerts` (3.5-13 ms)
    # is not expected to skip; the wins are the quiet sections — `intensification_knowledge`,
    # `discovered_sites`, `faction_inventory`, `food_modules`, `sedentarization`.
    _hud_invoke("update_overlay", [snapshot.get("turn", 0), metrics])
    # The turn a client-side System event is stamped with, so it groups with the sim events of the
    # turn it actually happened on.
    _event_dock_invoke("set_current_turn", [int(snapshot.get("turn", 0))])
    if snapshot.has("server_build"):
        _hud_invoke("update_build_info", [String(snapshot["server_build"])])
    if snapshot.has("sedentarization") and SnapshotSections.changed(snapshot, "sedentarization"):
        _hud_invoke("update_sedentarization", [snapshot["sedentarization"]])
    if snapshot.has("demographics") and SnapshotSections.changed(snapshot, "demographics"):
        _hud_invoke("update_demographics", [snapshot["demographics"]])
    if snapshot.has("intensification_knowledge") and SnapshotSections.changed(snapshot, "intensification_knowledge"):
        _hud_invoke("update_intensification", [snapshot["intensification_knowledge"]])
    if snapshot.has("discovered_sites") and SnapshotSections.changed(snapshot, "discovered_sites"):
        _hud_invoke("update_discoveries", [snapshot["discovered_sites"]])
    if snapshot.has("grid"):
        _hud_invoke("set_grid_dimensions", [snapshot["grid"]])
    if snapshot.has("food_modules") and SnapshotSections.changed(snapshot, "food_modules"):
        # Forward MapView's ingested food sites (each stamped with terrain_id) rather than the raw wire
        # array, so the HUD Forage-row glyph resolves the SAME terrain-aware icon the map marker draws
        # (riverine_delta splits fish↔reeds by terrain — see FoodIcons). display_snapshot ran above.
        var food_sites: Variant = map_view.food_sites if map_view != null else snapshot["food_modules"]
        _hud_invoke("update_food_modules", [food_sites])
    if snapshot.has("herds") and SnapshotSections.changed(snapshot, "herds"):
        # The HUD needs the live herd positions (herds migrate) to jump the map to a hunted herd
        # from the band panel's Current-actions rows, and to name it. Same array MapView renders.
        _hud_invoke("update_herds", [snapshot["herds"]])
    if snapshot.has("forage_patches") and SnapshotSections.changed(snapshot, "forage_patches"):
        # The HUD needs the forage patches to cap each Current-actions Forage row's worker stepper at
        # the patch's max-useful (the same forecast the compose control reads off tile_info). Same
        # array MapView ingests into `forage_patch_lookup`.
        _hud_invoke("update_forage_patches", [snapshot["forage_patches"]])
    # The Telling (docs/plan_the_telling.md). The `has()` guard is LOAD-BEARING: a delta carries a
    # field only when it CHANGED, so absence means "unchanged", never "cleared" — clearing the
    # cached forks on absence would drop the end-turn gate every quiet turn.
    if snapshot.has("pending_forks") and SnapshotSections.changed(snapshot, "pending_forks"):
        _hud_invoke("update_pending_forks", [snapshot["pending_forks"]])
    if snapshot.has("stance_axes") and SnapshotSections.changed(snapshot, "stance_axes"):
        _hud_invoke("update_stance_axes", [snapshot["stance_axes"]])
    if snapshot.has("voice_medium") and SnapshotSections.changed(snapshot, "voice_medium"):
        _hud_invoke("update_voice_medium", [snapshot["voice_medium"]])
    if snapshot.has("populations") and SnapshotSections.changed(snapshot, "populations"):
        _hud_invoke("update_band_alerts", [snapshot["populations"]])
    # `command_events` is PER-FRAME HISTORY, never reconstructible: a delta carries only the rows
    # appended since the baseline, a full snapshot carries the whole retained ring. Both consumers
    # (the Telling, and the event dock) accumulate and de-duplicate, so re-ingesting a full
    # snapshot's ring is harmless.
    #
    # **THE DOCK IS CLEARED ON EVERY FULL SNAPSHOT, BEFORE THAT SNAPSHOT'S EVENTS ARE APPLIED**, and
    # the order is the contract — the same frame carries the backfill, so clearing after the dispatch
    # would wipe what just landed. It is a correctness requirement, not hygiene: `CommandEventLog` is
    # checkpoint state, so a ROLLBACK restores it including its `next_seq` counter and the replayed
    # events reuse sequence numbers the client has already seen. A rollback publishes a full frame, so
    # without this clear the dock would suppress every replayed row as a duplicate `seq` and go on
    # showing a plausible but stale log, silently. (The Telling is deliberately NOT reset here — it
    # keeps its own scrolled-off history and de-dupes on a signature, not on `seq`.)
    if not is_delta:
        _event_dock_invoke("reset")
    # Retention BEFORE the ingest: the sim's window is hot-reloadable, so a frame can both narrow it
    # and carry the events that must be trimmed against the NEW value.
    if snapshot.has("command_events_retention_turns"):
        _event_dock_invoke("set_retention_turns", [int(snapshot["command_events_retention_turns"])])
    if snapshot.has("command_events") and SnapshotSections.changed(snapshot, "command_events"):
        _hud_invoke("ingest_command_events", [snapshot["command_events"]])
        _event_dock_invoke("ingest_events", [snapshot["command_events"]])
    if snapshot.has("victory") and SnapshotSections.changed(snapshot, "victory"):
        var victory_variant: Variant = snapshot["victory"]
        if victory_variant is Dictionary:
            _hud_invoke("update_victory_state", [victory_variant])
            _emit_victory_analytics(victory_variant)
    _hud_profiling = false
    profile.end(PROFILE_HUD, t_hud)
    _record_hud_calls(profile)
    var t_inspector: int = profile.begin(PROFILE_INSPECTOR)
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
    profile.end(PROFILE_INSPECTOR, t_inspector)
    var t_selection: int = profile.begin(PROFILE_SELECTION)
    _refresh_hud_selection()
    profile.end(PROFILE_SELECTION, t_selection)
    _camera_initialized = true
    var t_scripting: int = profile.begin(PROFILE_SCRIPTING)
    if script_host_manager != null and script_host_manager.has_host():
        if is_delta:
            script_host_manager.handle_delta(snapshot)
        else:
            script_host_manager.handle_snapshot(snapshot)
    profile.end(PROFILE_SCRIPTING, t_scripting)
    profile.end(PROFILE_APPLY, t_apply)
    profile.emit()

## Fold the decode cost of the poll that produced this snapshot into `profile`.
##
## Read off `SnapshotLoader`'s `last_poll_*` fields rather than measured here: the decode happens
## in `poll_stream`, one call up the stack, and both `_apply_snapshot` call sites run immediately
## after a poll that returned a frame — so the fields describe THIS batch's arrival.
##
## Those fields describe the whole BATCH, and a poll can return several frames. `consume_poll_profile`
## makes the cost land on the FIRST frame applied and report nothing for the rest, so the numbers
## still sum to one poll's decode rather than being multiplied by the frame count.
func _record_decode_phase(profile: TurnProfile) -> void:
    if snapshot_loader == null:
        return
    if not snapshot_loader.consume_poll_profile():
        return
    var note: String = DECODE_NOTE_FORMAT % [
        snapshot_loader.last_poll_decoded_frames,
        snapshot_loader.last_poll_discarded_frames,
    ]
    profile.record_ms(PROFILE_DECODE, snapshot_loader.last_poll_decode_msec, note)
    profile.record_ms(PROFILE_DECODE_NATIVE, snapshot_loader.last_poll_native_decode_msec)

## Drain `_hud_call_usec` into `profile`, keeping only the calls worth naming
## (`HUD_CALL_REPORT_MIN_MSEC`). Insertion order is call order, so the entries read as the fan-out ran.
func _record_hud_calls(profile: TurnProfile) -> void:
    for method: String in _hud_call_usec:
        var millis: float = float(_hud_call_usec[method]) / TurnProfile.USEC_PER_MSEC
        if millis >= HUD_CALL_REPORT_MIN_MSEC:
            profile.record_ms(PROFILE_HUD_PREFIX + method, millis)
    _hud_call_usec.clear()

## Drop every client-side cache that belongs to ONE world. The coordinator only decides WHEN — each
## surface owns its own reset and is reached through the same silent `has_method` probe the rest of
## Main uses, so a surface without one simply skips (it merges nothing worth clearing).
func _reset_per_world_state() -> void:
    _hud_invoke("reset_world_state")
    # The event dock needs no clear here: a world change always arrives on a FULL snapshot, and the
    # `command_events` dispatch below clears it on every one of those (see the note there — a
    # rollback reuses `seq`, so the full-frame clear is a correctness requirement in its own right).
    if map_view != null and map_view.has_method("reset_world_state"):
        map_view.call("reset_world_state")
    # The victory analytics line prints once per DISTINCT value, so a new world whose winner happens
    # to match the old one's would print nothing at all without this.
    _victory_analytics_signature = ""

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

## The select-then-cycle click reached the LAND stop of an occupied hex. Distinct from
## `selection_cleared` (an empty hex) because the HUD must record it as a DELIBERATE choice —
## otherwise its fresh-hex auto-pick puts the first band straight back.
func _on_map_land_selected() -> void:
    _hud_invoke("show_land_selection")

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

# ---- Band-addressed command TEXT -----------------------------------------------------------------
#
# THE BAND HANDLE ON THE WIRE IS `band_id`, NEVER `entity`. `PopulationCohortState.bandId` is the
# sim's durable band identity; ECS entity bits are allocation state a rollback renumbers, so a
# command naming one resolved to nothing when replayed. The server's `resolve_starting_unit_entity`
# now accepts `band_id` alone — and BOTH are `u64`, so sending the wrong one compiles, parses and
# transmits perfectly while every band order silently no-ops. `tools/command_guard.gd` exists
# because of that: it drives the real HUD emit path and hands these lines to the real server-side
# parser (`sim_runtime::command_text`), asserting the number that comes back is the snapshot's
# `band_id` and not its `entity`.
#
# The builders below are PURE statics for the same reason `escape_claimant` is — the emitted text is
# a client→server contract that fails at RUNTIME rather than at build time, so it has to be
# assertable without standing up the whole app scene. Each returns `{line, message}`, or an EMPTY
# dictionary meaning "nothing to send" (an unresolvable band, a missing target). Keep them free of
# node state; every grammar below is quoted from `sim_runtime/src/command_text.rs`'s `usage:`
# strings, which are the authority.

## `cancel_order <faction_id> [band_id] [all|work|roles]` — scope is `all` / `work` / `roles` (the
## server rejects anything else as a parse error). `work` clears Forage + Hunt only, leaving standing
## roles, parties and an in-progress move alone; the Work zone's bulk action sends that.
static func format_cancel_order(band: Dictionary, scope: String) -> Dictionary:
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(band.get("faction", PLAYER_FACTION_ID))
    return {
        "line": "cancel_order %d %d %s" % [faction, band_id, scope],
        "message": "Clear labor assignments (%s) for band." % scope,
    }

# THE FLOOR'S WIRE SPELLING — two decimals, which is finer than the slider's own `FLOOR_STEP` (5%)
# and therefore round-trips every value the UI can produce. It is deliberately NOT `str(float)`:
# GDScript renders a float with up to 14 significant digits, so a dial value that is not exactly
# representable would put `0.30000000000000004` on the command line.
const FLOOR_COMMAND_DECIMALS := 2

## The optional floor token, formatted for the command line. Absent/garbage falls back to the sim's
## own default rather than to `0` — "take everything" is the one value that must never be reached by
## a missing field.
static func _format_floor(payload: Dictionary) -> String:
    return String.num(SourceForecast.clamp_floor(
        float(payload.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))), FLOOR_COMMAND_DECIMALS)

## The same floor for the command-feed NOTE, in the player's units: `50%`. The feed says what the
## player chose, not what the wire carries.
static func _floor_percent_text(payload: Dictionary) -> String:
    return "%d%%" % SourceForecast.floor_percent(
        float(payload.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR)))

## `assign_labor <faction_id> <band> forage <x> <y> [floor] [species] <workers>`
##             | `hunt <herd_id> [floor] <workers>` | `scout <workers>` | `warrior <workers>`
##
## **THE OPTIONAL TOKEN IS A NUMBER, NOT A STANCE WORD.** The four harvest stances are deleted from
## the sim and `sim_runtime::command_text` REJECTS them BY NAME
## (`CommandParseError::RetiredStanceToken`), precisely so a stale emitter fails loudly instead of
## being silently reinterpreted as a crop key. The two optional forage tokens are disjoint by
## construction — a floor only ever parses as a float, a species key never does — so the parser
## tells them apart without the client having to pad the line.
static func format_assign_labor(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var kind := String(payload.get("kind", "")).strip_edges().to_lower()
    var workers: int = max(0, int(payload.get("workers", 0)))
    match kind:
        "forage":
            var fx := int(payload.get("x", -1))
            var fy := int(payload.get("y", -1))
            if fx < 0 or fy < 0:
                return {}
            var ffloor := _format_floor(payload)
            # The crop selection (Flora Roster S1) is the SECOND optional token and the worker count
            # is always last: `forage <x> <y> [floor] [species] <workers>`. It can only ride a line
            # that already carries a floor (the floor comes first), which the client always sends; an
            # empty species is simply omitted, and the sim then commits to the tile's dominant legal
            # plant.
            var fspecies := String(payload.get("species", "")).strip_edges().to_lower()
            var forage_line := ""
            if fspecies == "":
                forage_line = "assign_labor %d %d forage %d %d %s %d" % [faction, band_id, fx, fy, ffloor, workers]
            else:
                forage_line = "assign_labor %d %d forage %d %d %s %s %d" % [faction, band_id, fx, fy, ffloor, fspecies, workers]
            return {
                "line": forage_line,
                "message": "Assign %d forager%s to (%d, %d), leaving %s standing." % [
                    workers, "" if workers == 1 else "s", fx, fy, _floor_percent_text(payload)],
            }
        "hunt":
            var herd_id := String(payload.get("herd_id", "")).strip_edges()
            if herd_id == "":
                return {}
            return {
                "line": "assign_labor %d %d hunt %s %s %d" % [
                    faction, band_id, herd_id, _format_floor(payload), workers],
                "message": "Assign %d hunter%s to %s, leaving %s standing." % [
                    workers, "" if workers == 1 else "s", herd_id, _floor_percent_text(payload)],
            }
        "scout", "warrior":
            return {
                "line": "assign_labor %d %d %s %d" % [faction, band_id, kind, workers],
                "message": "Assign %d worker%s to %s." % [workers, "" if workers == 1 else "s", kind],
            }
    return {}

## `move_band <faction_id> <band> <x> <y>`
static func format_move_band(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    return {
        "line": "move_band %d %d %d %d" % [faction, band_id, x, y],
        "message": "Move band to (%d, %d)." % [x, y],
    }

## `send_expedition <faction_id> <band_id> <party_workers> <x> <y>`
static func format_send_expedition(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var party_workers := int(payload.get("party_workers", 0))
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if party_workers <= 0 or x < 0 or y < 0:
        return {}
    return {
        "line": "send_expedition %d %d %d %d %d" % [faction, band_id, party_workers, x, y],
        "message": "Send scouting expedition (%d) to (%d, %d)." % [party_workers, x, y],
    }

## `send_hunt_expedition <faction_id> <band_id> <party_workers> <fauna_id> [floor]`
## The trailing floor is optional and is a NUMBER in `0.0..=1.0` — the four stance words are rejected
## by name at parse. The server defaults the food peak when it is omitted; the client always sends it.
static func format_send_hunt_expedition(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var party_workers := int(payload.get("party_workers", 0))
    var fauna_id := String(payload.get("fauna_id", "")).strip_edges()
    if party_workers <= 0 or fauna_id == "":
        return {}
    var line := "send_hunt_expedition %d %d %d %s %s" % [
        faction, band_id, party_workers, fauna_id, _format_floor(payload)]
    # The COMMAND addresses the herd by its id; the FEED NOTE names the species. `game_deer_07` is a
    # database key — meaningless to a player — so it must never reach the feed. Hud sends the display
    # name alongside the key; fall back to the key only if it somehow didn't (better than an empty
    # subject, and it is never the normal path).
    var fauna_label := String(payload.get("fauna_label", "")).strip_edges()
    if fauna_label == "":
        fauna_label = fauna_id
    return {
        "line": line,
        "message": "Send hunting expedition (%d, leaving %s standing) after %s." % [
            party_workers, _floor_percent_text(payload), fauna_label],
    }

## `recall_expedition <faction_id> <expedition_band_id>` — a detached party is a band, addressed by
## the same durable id. Non-optional, unlike the `[band_id]` of `scout` / `cancel_order`.
static func format_recall_expedition(payload: Dictionary) -> Dictionary:
    var expedition_band_id := int(payload.get("expedition_band_id", HudConst.NO_BAND_ID))
    if expedition_band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    return {
        "line": "recall_expedition %d %d" % [faction, expedition_band_id],
        "message": "Recall expedition.",
    }

## `extend_pen <faction> <x> <y>` targets the pen's ANCHOR TILE, so it names no band at all — it is
## here for company, not because it carries a band handle.
static func format_extend_pen(payload: Dictionary) -> Dictionary:
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    return {
        "line": "extend_pen %d %d %d" % [faction, x, y],
        "message": "Extend pen at (%d, %d)." % [x, y],
    }

## **THE SECOND AXIS's commands** (issue #442): `cultivate <faction> <x> <y>` / `sow <faction> <x> <y>`
## / `corral <faction> <x> <y>` address a TILE, and `tame <faction> <herd_id>` addresses a HERD —
## taming is the verb you reach for on a ROAMING herd, identified by who follows it rather than by
## where it stands this turn, while a pen is a place. That split is the sim's, mirrored here.
##
## Each sets ONLY the improvement, on whichever bands already work the source — which is why the
## compose sheet sends `assign_labor` FIRST: an improvement verb aimed at an unworked source is
## rejected outright.
const IMPROVEMENT_HERD_TARGETED := ["tame"]

static func format_improvement(payload: Dictionary) -> Dictionary:
    var improvement := String(payload.get("improvement", "")).strip_edges().to_lower()
    if improvement == "":
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    if improvement in IMPROVEMENT_HERD_TARGETED:
        var herd_id := String(payload.get("herd_id", "")).strip_edges()
        if herd_id == "":
            return {}
        return {
            "line": "%s %d %s" % [improvement, faction, herd_id],
            "message": "%s %s." % [improvement.capitalize(), herd_id],
        }
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    return {
        "line": "%s %d %d %d" % [improvement, faction, x, y],
        "message": "%s (%d, %d)." % [improvement.capitalize(), x, y],
    }

## **`abandon_improvement <faction> forage <x> <y>` / `abandon_improvement <faction> hunt <herd_id>`**
## — the command that stops a build (issue #442). Alias `abandon`.
##
## **IT NAMES A SOURCE, NOT A VERB, AND THAT IS WHY IT IS ITS OWN BUILDER.** At most one improvement
## is ever in flight on a source, so naming the source names the thing being abandoned — and the
## targeting rule that follows is the WEB's (`forage` → tile, `hunt` → herd), not the verb's. The set
## verbs above target by VERB (`tame` names a herd; `cultivate`/`sow`/`corral` name a tile), and
## `corral` is the case that proves the two rules genuinely differ: it is a HERD's rung addressed by
## the pen's PLACE, so a shared branch would send `abandon_improvement <f> <x> <y>` for a herd.
##
## **Always allowed.** The server gates it on nothing — no knowledge, no ceiling, no site, no
## `Thriving` — because abandoning a STALLED build is the case it exists for. The only rejections are
## an unknown kind and a source building nothing, neither of which this can produce.
static func format_abandon_improvement(payload: Dictionary) -> Dictionary:
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var kind := String(payload.get("kind", "")).strip_edges().to_lower()
    if kind == SourceForecast.LABOR_KIND_HUNT:
        var herd_id := String(payload.get("herd_id", "")).strip_edges()
        if herd_id == "":
            return {}
        return {
            "line": "abandon_improvement %d %s %s" % [faction, kind, herd_id],
            "message": "Stop improving %s." % herd_id,
        }
    if kind != SourceForecast.LABOR_KIND_FORAGE:
        return {}
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    return {
        "line": "abandon_improvement %d %s %d %d" % [faction, kind, x, y],
        "message": "Stop improving (%d, %d)." % [x, y],
    }

## Send whatever a `format_*` builder produced, or nothing at all when it declined.
func _send_formatted_command(formatted: Dictionary) -> void:
    if formatted.is_empty():
        return
    _send_runtime_command(String(formatted["line"]), String(formatted["message"]))

func _on_hud_cancel_order(band: Dictionary, scope: String) -> void:
    _send_formatted_command(format_cancel_order(band, scope))

## Early-Game Labor (slice 3b): assign/unassign working-age workers to a source or a
## band-wide role. workers==0 removes/zeroes the assignment; the server clamps totals
## to available working-age. Payload built by the HUD allocation panel / assign controls.
func _on_hud_assign_labor(payload: Dictionary) -> void:
    _send_formatted_command(format_assign_labor(payload))

## Early-Game Labor (slice 3b): relocate the band to a destination tile picked on the map.
func _on_hud_move_band(payload: Dictionary) -> void:
    _send_formatted_command(format_move_band(payload))

## Scouting expedition (docs/plan_exploration_and_sites.md §2): outfit a party off a resident
## band and send it to a target tile. The server draws the workers + provisions and spawns the
## detached party (rejects an over-cap party with a feed message).
func _on_hud_send_expedition(payload: Dictionary) -> void:
    _send_formatted_command(format_send_expedition(payload))

## Hunting expedition (docs/plan_exploration_and_sites.md §2b): outfit a party off a resident band
## and send it to follow a herd. The 4th arg is a herd id string, not tile coords.
func _on_hud_send_hunt_expedition(payload: Dictionary) -> void:
    _send_formatted_command(format_send_hunt_expedition(payload))

## Extend a built pen by one fenced ring (Grazing 2d-γ). The server works the ring off over ~25
## turns (rejecting at max radius / unowned / Herding-unknown with a feed message).
func _on_hud_extend_pen(payload: Dictionary) -> void:
    _send_formatted_command(format_extend_pen(payload))

## Commit — or abandon — an improvement on a source the band already works (the second axis, issue
## #442). Sent immediately after the `assign_labor` that guarantees the crew is there.
##
## An EMPTY `improvement` is the abandon: `""` is the wire's own spelling of "building nothing", so
## the compose state, the payload and this branch all read one value rather than a parallel flag. The
## two commands keep separate builders because they target their source by different rules — see
## `format_abandon_improvement`.
func _on_hud_improvement(payload: Dictionary) -> void:
    if String(payload.get("improvement", "")).strip_edges() == "":
        _send_formatted_command(format_abandon_improvement(payload))
        return
    _send_formatted_command(format_improvement(payload))

## Recall an in-flight expedition home (folds workers + provisions back on arrival).
func _on_hud_recall_expedition(payload: Dictionary) -> void:
    _send_formatted_command(format_recall_expedition(payload))

func _on_hud_next_turn(steps: int) -> void:
    var clamped_steps: int = max(1, steps)
    var line := "turn %d" % clamped_steps
    var suffix := "s" if clamped_steps != 1 else ""
    _send_runtime_command(line, "Advance %d turn%s." % [clamped_steps, suffix])

## The Inspector's dev toolbar / autoplay advanced a turn. That path is deliberately NOT gated on
## a pending narrative fork (docs/plan_the_telling.md §1a) — but it must not be SILENT: note the
## skip in the command feed so a developer sees the question went unanswered.
func _on_inspector_turn_advanced(_steps: int) -> void:
    if hud == null:
        return
    if hud.has_method("has_pending_fork") and hud.call("has_pending_fork"):
        _hud_invoke("note_unanswered_fork")

## The Telling: answer a pending narrative fork. The next snapshot is authoritative; the HUD has
## already dropped the fork from its local cache so the end-turn gate lifts immediately.
func _on_hud_answer_fork(payload: Dictionary) -> void:
    var beat_id := String(payload.get("beat_id", "")).strip_edges()
    var choice_id := String(payload.get("choice_id", "")).strip_edges()
    if beat_id == "" or choice_id == "":
        return
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    _send_runtime_command(
        "answer_fork %d %s %s" % [faction, beat_id, choice_id],
        "Answered the question."
    )

func _send_runtime_command(line: String, message: String) -> void:
    if inspector != null and inspector.has_method("send_runtime_command"):
        var result: Variant = inspector.call("send_runtime_command", line, message)
        if result is bool and result:
            return
        push_warning("Command pending or rejected: %s" % line)
    else:
        push_warning("Inspector unavailable; cannot send command: %s" % line)

## ESC PRECEDENCE, as data. Which surface claims the key, innermost first:
##   (1) an open pause menu resumes;
##   (2) an open COMPOSE SHEET closes — it is the innermost working surface, so it claims ESC ahead
##       of targeting (docs/plan_tile_panel_layout.md §15);
##   (3) active targeting keeps ESC for MapView's targeting-cancel path (we must NOT consume it);
##   (4) otherwise the pause menu opens.
## Extracted as a pure static so the ORDER can be asserted without standing up the whole app scene
## (ui_preview drives it with the real HUD's own `is_compose_sheet_open` / `is_targeting_active`).
const ESC_RESUME := "resume"
const ESC_COMPOSE_SHEET := "compose_sheet"
const ESC_TARGETING := "targeting"
const ESC_PAUSE := "pause"

static func escape_claimant(pause_open: bool, compose_open: bool, targeting: bool) -> String:
    if pause_open:
        return ESC_RESUME
    if compose_open:
        return ESC_COMPOSE_SHEET
    if targeting:
        return ESC_TARGETING
    return ESC_PAUSE

func _unhandled_input(event: InputEvent) -> void:
    if event.is_action_pressed("ui_cancel"):
        var claimant := escape_claimant(
            pause_layer != null and pause_layer.visible,
            hud != null and hud.has_method("is_compose_sheet_open") and bool(hud.call("is_compose_sheet_open")),
            hud != null and hud.has_method("is_targeting_active") and bool(hud.call("is_targeting_active")))
        match claimant:
            ESC_RESUME:
                _hide_pause_menu()
                get_viewport().set_input_as_handled()
            ESC_COMPOSE_SHEET:
                hud.call("close_compose_sheet")
                get_viewport().set_input_as_handled()
            ESC_TARGETING:
                return
            _:
                _show_pause_menu()
                get_viewport().set_input_as_handled()

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
##
## **The event dock is deliberately absent.** It reserves nothing from either surface — it OVERLAYS
## the map (see `EventDockPanel`'s header) — so it has no place in a stacking order and no entry in
## `_reservations`. Its horizontal bound comes from `_update_event_dock_insets`, which reads the
## OTHER reservers; that is the perpendicular axis and a different question from this one.
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
    # The PERPENDICULAR axis: pull the horizontal event bar in from whatever is docked left/right.
    # Recomputed on EVERY reservation change, not just the dock's own — the band panel changing edge
    # or collapsing has to move the bar.
    _update_event_dock_insets()

## The Band panel's leading offset = Σ sizes of all lower-priority reservers currently on the SAME
## edge as the Band panel (today just the Inspector when both dock left; 0 otherwise). Recomputed
## on every reservation change, so the panel tracks the Inspector's show/hide + live drag-resize.
func _update_band_panel_edge_offset() -> void:
    if band_city_panel == null or not band_city_panel.has_method("set_edge_offset") or not band_city_panel.has_method("get_dock"):
        return
    var band_edge: int = int(band_city_panel.call("get_dock"))
    var band_priority: int = int(RESERVER_PRIORITY.get(BAND_PANEL_RESERVER, 0))
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

## The event dock's counterpart to `_update_band_panel_edge_offset`, on the OTHER axis.
##
## The bar is top/bottom only, so every `SIDE_LEFT` / `SIDE_RIGHT` reserver is a full-height column
## the bar must live BESIDE rather than across: it starts to the right of whatever is docked left and
## finishes to the left of whatever is docked right. Sums the per-edge totals and hands the pair
## over. Every reserver counts — the dock is not one of them, so there is nothing to exclude.
##
## **A RESERVATION IS ONLY HALF THE BOUND.** The HUD's own side columns — the left dock, and on the
## right the dock plus the top-bar readout block — are not reservers, so bounding against
## reservations alone still drew the bar over `Turn N` / `Units` / `Pop`. They live INSIDE whatever
## strip the docks reserved, so the two terms ADD rather than compete: `reservation + column`. Both
## column widths are AUTHORED (`Hud.left_column_width` / `right_column_width` read
## `custom_minimum_size.x` off the scene), so the bar's edges are a function of constants and cannot
## move when the player selects a tile or a metric gains a digit.
##
## **This is not `RESERVER_PRIORITY`.** That orders reservers stacked ALONG one shared edge (the dock
## is 0 there, so it still hugs its own edge); this is the cross axis, where there is no stacking
## question — a vertical column simply takes room the horizontal bar may not use. Conflating the two
## is the easy mistake: priority would never have fixed this, because TOP and LEFT are not co-edge and
## `_update_band_panel_edge_offset` correctly ignores each other's edges.
func _update_event_dock_insets() -> void:
    if event_dock == null or not event_dock.has_method("set_perpendicular_insets"):
        return
    var left: float = 0.0
    var right: float = 0.0
    for other_id in _reservations:
        var r: Dictionary = _reservations[other_id]
        match int(r.get("edge", -1)):
            SIDE_LEFT:
                left += float(r.get("size", 0.0))
            SIDE_RIGHT:
                right += float(r.get("size", 0.0))
    if hud != null and hud.has_method("left_column_width"):
        left += float(hud.call("left_column_width"))
    if hud != null and hud.has_method("right_column_width"):
        right += float(hud.call("right_column_width"))
    event_dock.call("set_perpendicular_insets", left, right)

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
    # SECOND listener on the same reservation (issue #324): on a horizontal dock the HUD parks its
    # bottom-bar chrome into the panel's row. Order against `_on_band_panel_reservation_changed` does
    # not matter — the reflow reads only `(edge, size)`.
    if band_city_panel.has_signal("reservation_changed") and hud != null and hud.has_method("reflow_dock_row") and not band_city_panel.is_connected("reservation_changed", Callable(hud, "reflow_dock_row")):
        band_city_panel.connect("reservation_changed", Callable(hud, "reflow_dock_row"))
    if band_city_panel.has_method("get_dock") and band_city_panel.has_method("current_reservation_size"):
        var dock_edge := int(band_city_panel.call("get_dock"))
        var dock_size := float(band_city_panel.call("current_reservation_size"))
        _apply_reservation(&"band_panel", dock_edge, dock_size)
        # Seed the reflow too, so a session that boots already docked bottom reflows immediately
        # rather than waiting for the first dock change.
        if hud != null and hud.has_method("reflow_dock_row"):
            hud.call("reflow_dock_row", dock_edge, dock_size)

func _on_band_panel_reservation_changed(edge: int, size: float) -> void:
    _apply_reservation(&"band_panel", edge, size)

## Wire the event dock onto the reservation fan-out and seed its initial reservation (the panel's
## own startup emit happens before we are ready, exactly as with the Band/City panel, so query it).
## Its priority is 0, so it always hugs its edge and never needs an edge offset of its own.
func _connect_event_dock() -> void:
    if event_dock == null:
        return
    # **The dock is wired for CONTENT ONLY — there is no reservation to fan out.** It overlays the
    # map rather than reserving an edge, so nothing here touches `_apply_reservation`.
    # The HUD's own client-side notes (a quick-hunt refusal, a knowledge unlock, an unanswered fork)
    # used to land in the command feed. They are System-channel events now, and the HUD relays them
    # rather than reaching for a panel it does not own.
    if hud != null and hud.has_signal("system_note_requested") and not hud.is_connected(
            "system_note_requested", Callable(self, "_on_system_note_requested")):
        hud.connect("system_note_requested", Callable(self, "_on_system_note_requested"))
    # THE DOCK NAMES A BAND THE WAY THE REST OF THE HUD DOES. The snapshot carries no band NAME, so
    # the sim writes a positional `Band <BandId>` into a demographic event's label and repeats the id
    # in the detail's `band=` token; the client's own name is a ROSTER POSITION, which the HUD owns.
    # So the HUD publishes the map and the dock does the substitution — the sim's label is never
    # changed, and neither surface reaches into the other.
    if hud != null and hud.has_signal("band_labels_changed") and not hud.is_connected(
            "band_labels_changed", Callable(self, "_on_band_labels_changed")):
        hud.connect("band_labels_changed", Callable(self, "_on_band_labels_changed"))
    # The Inspector's console chatter — connection state, a command sent or refused, a rollback.
    # A dropped command socket is something the player must be told, and the debug console is not
    # where they will see it.
    if inspector != null and inspector.has_signal("system_event") and not inspector.is_connected(
            "system_event", Callable(self, "_on_inspector_system_event")):
        inspector.connect("system_event", Callable(self, "_on_inspector_system_event"))
    # Seed the perpendicular insets: nothing else will, since the dock never enters
    # `_apply_reservation`'s fan-out. Wiring runs after `_connect_band_city_panel`, so the vertical
    # reservers are already in `_reservations` by now.
    _update_event_dock_insets()

func _on_band_labels_changed(labels: Dictionary) -> void:
    _event_dock_invoke("set_band_labels", [labels])

func _on_system_note_requested(label: String, detail: String) -> void:
    _note_system_event(label, detail, false)

func _on_inspector_system_event(label: String, detail: String, alert: bool) -> void:
    _note_system_event(label, detail, alert)

func _note_system_event(label: String, detail: String, alert: bool) -> void:
    if event_dock != null and event_dock.has_method("note_system"):
        event_dock.call("note_system", label, detail, alert)

## `R` toggles the event dock — the hotkey the retired left-dock command feed used to own, and the
## persisted `command_feed_suppressed` preference migrates onto the dock's own `suppressed` key.
func _toggle_event_dock_visibility() -> void:
    if event_dock != null and event_dock.has_method("toggle_suppressed"):
        event_dock.call("toggle_suppressed")

func _toggle_legend_visibility() -> void:
    if hud == null:
        return
    if hud.has_method("toggle_legend"):
        _hud_invoke("toggle_legend")

func _toggle_victory_visibility() -> void:
    if hud == null:
        return
    if hud.has_method("toggle_victory"):
        _hud_invoke("toggle_victory")

# FOG OF WAR IS SERVER-AUTHORITATIVE. The sim owns `fog_enabled` on its config — it gates BOTH the
# visibility raster and the herd display list, which is the point: with fog off the fauna the filter
# used to drop are now genuinely sent, so the Fauna tab shows them without any client-side special
# case. The client is therefore NOT an authority. One direction only:
#
#     preference (ClientSettings) → `set_fog` command → server → snapshot `fog_enabled` → render
#
# `F` and the Options checkbox are the SAME state because both write only the preference. The client
# never flips its render flag on its own; it waits for the snapshot to say so. Nothing may write
# `ClientSettings` FROM a snapshot — that would close the loop into an echo.

# The last `fog_enabled` the server reported, or UNKNOWN before the first snapshot carries it. This
# is the resend guard: a `set_fog` goes out only when the preference DISAGREES with it, so the
# `ClientSettings.changed` handler and the per-snapshot reconcile can't ping-pong.
const FOG_SERVER_STATE_UNKNOWN := -1
const FOG_SERVER_STATE_OFF := 0
const FOG_SERVER_STATE_ON := 1
var _fog_server_state: int = FOG_SERVER_STATE_UNKNOWN

## `F` — flip the PREFERENCE, nothing else. `_on_client_settings_changed` turns it into a command.
func _toggle_fow_overlay() -> void:
    ClientSettings.set_fog_of_war_enabled(not ClientSettings.fog_of_war_enabled)

func _on_client_settings_changed() -> void:
    _push_fog_preference()

## Send `set_fog` iff the preference disagrees with the server's last reported state. Silent before
## the first snapshot (nothing to disagree with) and silent with no connection — on the landing menu
## the preference just persists, and the reconcile in `_sync_fog_of_war` sends it once a world loads.
func _push_fog_preference() -> void:
    var desired: bool = ClientSettings.fog_of_war_enabled
    if _fog_server_state == FOG_SERVER_STATE_UNKNOWN:
        return
    if desired == (_fog_server_state == FOG_SERVER_STATE_ON):
        return
    _send_runtime_command(
        "set_fog %s" % ("on" if desired else "off"),
        "Fog of war %s." % ("enabled" if desired else "disabled")
    )

## Snapshot → render. The ONLY caller of `set_fow_enabled` in the live client (the offline
## `tools/map_preview` and `tools/blend_probe` harnesses still drive it directly, which is why it
## stays a public setter). A missing key means an older server: assume fog on.
## Then RECONCILE: if the server disagrees with the persisted preference, send `set_fog` once. That
## is what carries "I play with fog off" across `new_game` into a freshly generated world, and it is
## idempotent — the next snapshot agrees and the guard above goes quiet.
## The DELTA guard is defensive, not currently exercised: the native decoder resolves `fog_enabled`
## from its own cached `Option` and so always emits the key. It is here because the failure mode if
## that ever stops holding is nasty and silent — on a delta an absent key means "unchanged", not
## "fog on", and taking the default there would strobe the fog back on every turn.
func _sync_fog_of_war(snapshot: Dictionary, is_delta: bool) -> void:
    if is_delta and not snapshot.has("fog_enabled"):
        return
    var enabled: bool = bool(snapshot.get("fog_enabled", true))
    _fog_server_state = FOG_SERVER_STATE_ON if enabled else FOG_SERVER_STATE_OFF
    if map_view != null and map_view.has_method("set_fow_enabled"):
        map_view.call("set_fow_enabled", enabled)
    _push_fog_preference()

func _process(delta: float) -> void:
    if Input.is_action_just_pressed("toggle_inspector"):
        _toggle_inspector_visibility()
    if Input.is_action_just_pressed("toggle_legend"):
        _toggle_legend_visibility()
    if Input.is_action_just_pressed("toggle_victory"):
        _toggle_victory_visibility()
    if Input.is_action_just_pressed("toggle_event_dock"):
        _toggle_event_dock_visibility()
    if Input.is_action_just_pressed("toggle_fow"):
        _toggle_fow_overlay()
    if command_client != null:
        command_client.poll()
        command_client.ensure_connected()
    _tick_new_game_retry(delta)
    if streaming_mode:
        _tick_resync(delta)
        # EVERY frame the poll returns is applied, in order. The loader already dropped the ones a
        # later full snapshot superseded; what is left are frames whose content exists nowhere else
        # (see `SnapshotLoader.poll_stream`).
        var streamed_frames: Array[Dictionary] = snapshot_loader.poll_stream(delta)
        if not streamed_frames.is_empty():
            if inspector != null and inspector.has_method("set_streaming_active"):
                inspector.call("set_streaming_active", true)
            for streamed in streamed_frames:
                if _world_revealed:
                    _apply_snapshot(streamed)
                else:
                    _try_reveal_world(streamed)

## Ask the server to republish a full world when the decoder dropped a delta it could not apply,
## and keep asking until one lands.
##
## The drop itself is correct and deliberate — merging a delta onto the wrong baseline produces a
## world that is silently wrong rather than visibly broken (`docs/plan_delta_streaming.md` §3.3).
## But dropping alone leaves the client frozen, so the request is the other half of that contract.
func _tick_resync(delta: float) -> void:
    if snapshot_loader == null:
        return
    if snapshot_loader.resync_needed:
        snapshot_loader.resync_needed = false
        if _resync_pending_accum < 0.0:
            _send_runtime_command("resync", "resync requested (unapplicable delta)")
            _resync_pending_accum = 0.0
        return
    if _resync_pending_accum < 0.0:
        return
    _resync_pending_accum += delta
    if _resync_pending_accum >= RESYNC_ANSWER_TIMEOUT:
        _resync_pending_accum = 0.0
        _send_runtime_command("resync", "resync retry (still no baseline)")

## Loading gate: while the world is not yet revealed, decide whether a streamed snapshot is the
## freshly generated world (reveal + apply) or a pre-rebuild frame of the OLD one (ignore).
##
## The gate holds the loading overlay until a FULL snapshot for a world NEWER than the baseline
## arrives. The server no longer replays a cached frame to a reconnecting client, but a client that
## was ALREADY connected when the rebuild began can still receive a broadcast of the world it is
## replacing, and that frame must NOT be shown. So we reveal only on a FULL snapshot whose
## world_epoch EXCEEDS the baseline captured at _ready:
##   - fresh boot: baseline 0 → reveal on epoch 1;
##   - restart:    baseline N (persisted) → ignore any lingering epoch-N frame → reveal on N+1.
## A delta arriving before that full snapshot is ignored (it has no complete world to reveal).
## Defensive: a snapshot with no world_epoch key (pre-change server) reveals on the first full
## snapshot, so the client can never get stuck on a black loading screen.
func _try_reveal_world(streamed: Dictionary) -> void:
    var is_delta := _snapshot_is_delta(streamed)
    if is_delta:
        return
    var has_epoch := streamed.has("world_epoch")
    var epoch := int(streamed.get("world_epoch", 0))
    if has_epoch and epoch <= _reveal_baseline_epoch:
        # A pre-rebuild frame of the previous world — hold the loading overlay.
        return
    _world_revealed = true
    _hide_loading_overlay()
    var launch_node: Node = get_node_or_null("/root/GameLaunch")
    if launch_node != null:
        launch_node.set("last_world_epoch", epoch)
    _apply_snapshot(streamed)
    _apply_startup_view()

## Per-world-reveal view defaults that need the LOADED world: seat the startup zoom and centre on the
## player's starting band. Called from _try_reveal_world AFTER the reveal snapshot is applied (so the
## band's tile is populated via Hud.update_band_alerts) and the overlay is hidden — so every new world
## (fresh boot or Abandon→New Game restart) opens at zoom 2 centred on the band. Inspector-hidden +
## FoW-on are seated once in _ready (they don't need the world). This fires ONCE per world (deltas
## don't re-reveal), so a player's later zoom/pan/inspector changes persist.
func _apply_startup_view() -> void:
    if map_view == null:
        return
    # ORDER: set the zoom FIRST (so the hex radius is at the target), THEN centre — focus_on_tile
    # centres at the current zoom.
    if map_view.has_method("set_zoom_factor"):
        map_view.call("set_zoom_factor", STARTUP_ZOOM_FACTOR)
    var band_tile := Vector2i(-1, -1)
    if hud != null and hud.has_method("get_player_band_tile"):
        band_tile = hud.call("get_player_band_tile")
    if band_tile.x >= 0 and band_tile.y >= 0 and map_view.has_method("focus_on_tile"):
        map_view.call("focus_on_tile", band_tile.x, band_tile.y)

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

## Is this frame a delta? Read off `frame_kind`, which the decoder stamps from the envelope's own
## payload discriminant — the authoritative answer, not an inference.
##
## The `SNAPSHOT_DELTA_FIELDS` fallback below is for a native extension built before `frame_kind`
## existed (the same staleness the `has_method` probes elsewhere tolerate). It guesses from six
## delta-only keys and is only correct by accident: the delta codec emits an EMPTY vector rather
## than omitting an untouched section, so `tile_updates` rides every delta including one that
## changed no tile. Misclassifying a delta as a full snapshot resets the command feed and can trip
## the world-epoch reset, so the guess is a fallback, never the contract.
func _snapshot_is_delta(snapshot: Dictionary) -> bool:
    if snapshot.has("frame_kind"):
        return String(snapshot["frame_kind"]) == "delta"
    for field in SNAPSHOT_DELTA_FIELDS:
        if snapshot.has(field):
            return true
    return false

## The event dock's twin of `_hud_invoke` — a silent `has_method` probe, so a client running
## without the panel (a harness, a partial scene) simply does nothing rather than erroring.
func _event_dock_invoke(method: String, args: Array = []) -> void:
    if event_dock != null and event_dock.has_method(method):
        event_dock.callv(method, args)

func _hud_invoke(method: String, args: Array = []) -> Variant:
    var result: Variant = null
    if hud != null and hud.has_method(method):
        # print("[HUD->Main]", method, args)  # Commented out to reduce log spam
        if _hud_profiling:
            # Only inside `_apply_snapshot`'s fan-out, and only while profiling: everything else
            # reaching the HUD through this helper stays on the untimed branch.
            var started: int = Time.get_ticks_usec()
            result = hud.callv(method, args)
            _hud_call_usec[method] = int(_hud_call_usec.get(method, 0)) + (Time.get_ticks_usec() - started)
        else:
            result = hud.callv(method, args)
    return result

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
    # The stream is the FlatBuffers snapshot socket ("snapshot_flat"). The
    # "snapshot" key it used to be confused with is gone — that socket was
    # retired in #388 and its port slot is now reserved and unbound.
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
