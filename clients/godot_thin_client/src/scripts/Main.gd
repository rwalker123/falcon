extends Node2D

const SnapshotLoader = preload("res://src/scripts/SnapshotLoader.gd")
const CommandClient = preload("res://src/scripts/CommandClient.gd")
const ScriptHostManager = preload("res://src/scripts/scripting/ScriptHostManager.gd")
const ServerPortsFile = preload("res://src/scripts/ServerPortsFile.gd")
const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

@onready var map_view: Node2D = $MapLayer
@onready var hud: CanvasLayer = $HUD
@onready var camera: Camera2D = $Camera2D
@onready var inspector: CanvasLayer = $Inspector
@onready var band_city_panel: CanvasLayer = $BandCityPanel
## The Workbench's host layer. The surface itself is BUILT (not instanced) into it — see
## `_connect_workbench`.
@onready var workbench_layer: CanvasLayer = $Workbench
@onready var event_dock: CanvasLayer = $EventDockPanel
@onready var pause_layer: CanvasLayer = $PauseLayer
## **THE ONE "is the player typing?" PREDICATE**, shared with `MapView`'s pan/zoom poll and
## `MenuShell`'s focus release. See the file for why it is not a private method here.
const TextEntryFocus := preload("res://src/scripts/TextEntryFocus.gd")

@onready var pause_menu: MenuShell = $PauseLayer/MenuShell

## The designer surface, built into `workbench_layer` at `_connect_workbench` and hidden until `` ` ``.
var workbench: WorkbenchShell = null

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
# The save channel (list / save / load / delete). Owned here for the same reason `ForecastQuery`
# is: the seam holds no socket, and the pause menu that drives it must not reach the network.
var save_slots: SaveSlots = null
# The slot this run is being LOADED from ("" = this run generates a world instead). Set from the
# GameLaunch handoff in _build_world_request; it is what makes _try_send_world_request send
# `load_game` rather than `new_game`, through the same retry and the same reveal gate.
var _pending_load_slot: String = ""
# The config files whose tuning moved between writing the save and loading it, held from the load's
# reply until the loaded world REVEALS — the notice belongs over the world it is warning about, and
# the reply lands while the loading overlay is still up.
var _pending_config_drift: Array = []
var _drift_notice: ConfigDriftNotice = null

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
## The two base chrome layers, named because four other surfaces are placed RELATIVE to them and were
## reasoning about bare literals to do it (`BandCityPanel.LAYER_INDEX`, `EventDockPanel.LAYER_INDEX`,
## `WORKBENCH_LAYER` below, and `OverlayPicker.POPOVER_CANVAS_LAYER`, which has to clear all of them).
const HUD_LAYER = 101
const INSPECTOR_LAYER = 102
# Loading overlay: a CanvasLayer above HUD (101) and Inspector (102), so it fully covers the blank
# map/HUD until the new world reveals.
const LOADING_OVERLAY_LAYER = 150
## The Workbench sits one layer above the Inspector (102) and well under the loading overlay (150):
## the two dev surfaces are never usefully stacked, and whichever was opened last should be the
## readable one. It stays BELOW the event dock (`EventDockPanel.LAYER_INDEX` = 104) for the reason
## that constant gives — the bar must not be drawn under a surface whose reservation is what pulls
## the bar's own edges in, during the frame that reflow lands.
##
## **It ties with `BandCityPanel.LAYER_INDEX` (103), and the tie is resolved by tree order** — the
## `Workbench` node follows `BandCityPanel` in `Main.tscn`, so the Workbench wins. That is the wanted
## side of the tie (the surface being toggled is the one that should be legible mid-reflow), and the
## two never overlap in steady state anyway: the Band panel offsets past the Workbench's reservation.
const WORKBENCH_LAYER = 103
## Toggle action for the designer surface. **Backquote**, which nothing else in the client binds (the
## hotkey table in `clients/godot_thin_client/CLAUDE.md` is the roster) and which costs the game no
## letter it may still want.
const WORKBENCH_TOGGLE_ACTION := "toggle_workbench"
const WORKBENCH_RESERVER := &"workbench"
## Receipt the command log shows for a Workbench-issued command; `%s` is the command's verb. The verb
## alone, because the argument is a JSON patch that would swamp the feed.
const WORKBENCH_COMMAND_MESSAGE := "Workbench: %s sent."
## Label the Workbench's status lines wear on the event dock's System channel.
const WORKBENCH_LOG_LABEL := "Workbench"
const LOADING_OVERLAY_TEXT = "Generating world…"
const LOADING_OVERLAY_FONT_SIZE = 28
const COMMAND_HOST = "127.0.0.1"
const COMMAND_PORT = 41001
const PLAYER_FACTION_ID = 0
# --- THE SHIPMENT MANIFEST'S SPELLING (arc #527, see `format_send_trade_expedition`) --------------
# **THE COMMAND LINE AND THE FEED NOTE SPELL AN AMOUNT DIFFERENTLY, because they are read by
# different readers.** The note is prose for a person and rounds to one decimal; the LINE is an order
# the server checks against a store, and rounding it is a defect — see `cargo_wire_amount`.
#
# One decimal on the note only: `%s` on a GDScript float prints `4.5000001` for a value the picker
# clamped to a stored pile, and the command feed is read by people.
const TRADE_CARGO_NOTE_AMOUNT_FORMAT := "%.1f"
# **THE SIM'S FIXED-POINT PRECISION**, mirroring `core_sim::Scalar::SCALE` / `sim_runtime::
# FIXED_POINT_SCALE` (10^6). It is the finest amount a store can hold, so it is the finest amount a
# manifest may name; `cargo xtask command-guard` drives a fractional pile through the REAL server
# parser, so a drift from the sim's own scale fails there rather than in play.
const SIM_SCALAR_DECIMALS := 6
# **THE OTHER GRID THE AMOUNT HAS TO SURVIVE, and the coarser of the two above ~8 units.** The
# command line is text: the server parses it back with `parse_f32` and only then quantises it to
# `Scalar`, so the value that reaches the store comparison has been through a 32-bit float twice (the
# parse, and the ×10^6 that follows it). `1.1920929e-7` is that float's relative step — `f32::EPSILON`
# — and a 6-decimal amount at 137 units sits ~40 of them apart from its neighbours, so an amount
# floored onto the fixed-point grid ALONE still lands above the pile about 40% of the time.
const WIRE_FLOAT_EPSILON := 1.1920929e-7
# How many of those steps to back off before emitting: one for the parse, one for the multiply, so
# the reconstructed value cannot land above the pile. The cost is the crumb left behind — at 300 food
# it is 0.0002, four orders of magnitude under the tenth of a unit the readouts render.
const WIRE_FLOAT_BACKOFF_STEPS := 2.0
# The feed note's manifest clause — `12.0 food · 4.0 hide`. The ` · ` is the separator this HUD spends
# on separating accounts everywhere else, and the material terms are NEVER merged into a total.
const TRADE_CARGO_TERM_SEPARATOR := " · "
const TRADE_CARGO_FOOD_TERM_FORMAT := "%s food"
# What the note reads when the manifest is empty. The line is still SENT — whether an empty shipment
# is legal is the server's question and it answers with a reason — so the note has to say what was
# asked for rather than pretend a cargo was named.
const TRADE_CARGO_EMPTY_TERM := "nothing"
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
## The config-drift notice sits above the HUD and the Inspector but BELOW the pause menu: it is a
## thing to read about the world, not a modal that should outrank ESC.
const DRIFT_NOTICE_LAYER := 150

const SNAPSHOT_DELTA_FIELDS := [
    "influencer_updates",
    "population_updates",
    "tile_updates",
    "influencer_removed",
    "population_removed"
]

func _ready() -> void:
    # Force content scale mode to handle high DPI and ultrawide monitors
    get_window().content_scale_mode = Window.CONTENT_SCALE_MODE_CANVAS_ITEMS
    get_window().content_scale_aspect = Window.CONTENT_SCALE_ASPECT_EXPAND
    
    # Ensure HUD and Inspector render above the map layer
    if hud != null:
        hud.layer = HUD_LAYER
    if inspector != null:
        inspector.layer = INSPECTOR_LAYER

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
    # The save channel rides the same command client. Built BEFORE the world request, because the
    # world request may itself be a `load_game` that goes out through this seam.
    save_slots = SaveSlots.new()
    save_slots.set_sender(Callable(self, "_send_query"))
    save_slots.op_finished.connect(_on_save_op_finished)
    # The server now boots idle and only generates a world on `new_game` (or restores one on
    # `load_game`); build that request from the landing-screen handoff (or a dev default) and fire it
    # (retried in _process if not yet sent).
    _build_world_request()
    _try_send_world_request()
    script_host_manager = ScriptHostManager.new()
    add_child(script_host_manager)
    script_host_manager.setup(command_client)
    if inspector != null and inspector.has_method("attach_script_host"):
        inspector.call("attach_script_host", script_host_manager)

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
        if hud.has_signal("send_denial_raid_requested") and not hud.is_connected("send_denial_raid_requested", Callable(self, "_on_hud_send_denial_raid")):
            hud.connect("send_denial_raid_requested", Callable(self, "_on_hud_send_denial_raid"))
        if hud.has_signal("send_trade_expedition_requested") and not hud.is_connected("send_trade_expedition_requested", Callable(self, "_on_hud_send_trade_expedition")):
            hud.connect("send_trade_expedition_requested", Callable(self, "_on_hud_send_trade_expedition"))
        if hud.has_signal("recall_expedition_requested") and not hud.is_connected("recall_expedition_requested", Callable(self, "_on_hud_recall_expedition")):
            hud.connect("recall_expedition_requested", Callable(self, "_on_hud_recall_expedition"))
        if hud.has_signal("split_band_requested") and not hud.is_connected("split_band_requested", Callable(self, "_on_hud_split_band")):
            hud.connect("split_band_requested", Callable(self, "_on_hud_split_band"))
        if hud.has_signal("extend_pen_requested") and not hud.is_connected("extend_pen_requested", Callable(self, "_on_hud_extend_pen")):
            hud.connect("extend_pen_requested", Callable(self, "_on_hud_extend_pen"))
        if hud.has_signal("upkeep_mode_requested") and not hud.is_connected("upkeep_mode_requested", Callable(self, "_on_hud_upkeep_mode")):
            hud.connect("upkeep_mode_requested", Callable(self, "_on_hud_upkeep_mode"))
        if hud.has_signal("improvement_requested") and not hud.is_connected("improvement_requested", Callable(self, "_on_hud_improvement")):
            hud.connect("improvement_requested", Callable(self, "_on_hud_improvement"))
        if hud.has_signal("unqueue_requested") and not hud.is_connected("unqueue_requested", Callable(self, "_on_hud_unqueue")):
            hud.connect("unqueue_requested", Callable(self, "_on_hud_unqueue"))
        if hud.has_signal("abandon_requested") and not hud.is_connected("abandon_requested", Callable(self, "_on_hud_abandon")):
            hud.connect("abandon_requested", Callable(self, "_on_hud_abandon"))
        if hud.has_signal("build_kit_requested") and not hud.is_connected("build_kit_requested", Callable(self, "_on_hud_build_kit")):
            hud.connect("build_kit_requested", Callable(self, "_on_hud_build_kit"))
        if hud.has_signal("upkeep_kit_requested") and not hud.is_connected("upkeep_kit_requested", Callable(self, "_on_hud_upkeep_kit")):
            hud.connect("upkeep_kit_requested", Callable(self, "_on_hud_upkeep_kit"))
        if hud.has_signal("build_order_requested") and not hud.is_connected("build_order_requested", Callable(self, "_on_hud_build_order")):
            hud.connect("build_order_requested", Callable(self, "_on_hud_build_order"))
        if hud.has_signal("work_priority_requested") and not hud.is_connected("work_priority_requested", Callable(self, "_on_hud_work_priority")):
            hud.connect("work_priority_requested", Callable(self, "_on_hud_work_priority"))
        if hud.has_signal("set_bench_requested") and not hud.is_connected("set_bench_requested", Callable(self, "_on_hud_set_bench")):
            hud.connect("set_bench_requested", Callable(self, "_on_hud_set_bench"))
        if hud.has_signal("bench_crew_requested") and not hud.is_connected("bench_crew_requested", Callable(self, "_on_hud_bench_crew")):
            hud.connect("bench_crew_requested", Callable(self, "_on_hud_bench_crew"))
        if hud.has_signal("clear_bench_requested") and not hud.is_connected("clear_bench_requested", Callable(self, "_on_hud_clear_bench")):
            hud.connect("clear_bench_requested", Callable(self, "_on_hud_clear_bench"))
        if hud.has_signal("bench_priority_requested") and not hud.is_connected("bench_priority_requested", Callable(self, "_on_hud_bench_priority")):
            hud.connect("bench_priority_requested", Callable(self, "_on_hud_bench_priority"))
        if hud.has_signal("answer_fork_requested") and not hud.is_connected("answer_fork_requested", Callable(self, "_on_hud_answer_fork")):
            hud.connect("answer_fork_requested", Callable(self, "_on_hud_answer_fork"))
        # **THE FORECAST QUERY'S TRANSPORT, injected rather than reached for.** The HUD composes the
        # question out of what it is already rendering; the socket is `Main`'s. `_process` pumps the
        # answers back the other way — a query triggers NO snapshot, so nothing else ever would.
        if hud.has_method("forecast_query"):
            hud.call("forecast_query").set_sender(Callable(self, "_send_query"))
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
    if inspector != null and inspector.has_method("set_streaming_active"):
        inspector.call("set_streaming_active", streaming_mode)
    _ensure_action_binding("toggle_inspector", Key.KEY_I)
    _ensure_action_binding("toggle_victory", Key.KEY_V)
    _ensure_action_binding("toggle_event_dock", Key.KEY_R)
    _ensure_action_binding("toggle_fow", Key.KEY_F)
    _ensure_action_binding(WORKBENCH_TOGGLE_ACTION, Key.KEY_QUOTELEFT)
    if inspector != null and inspector.has_signal("reserved_width_changed") and not inspector.is_connected("reserved_width_changed", Callable(self, "_on_inspector_reserved_width_changed")):
        inspector.connect("reserved_width_changed", Callable(self, "_on_inspector_reserved_width_changed"))
    if inspector != null and inspector.has_method("reserved_width"):
        _apply_reservation(&"inspector", SIDE_LEFT, float(inspector.call("reserved_width")))
    _connect_workbench()
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
    if not pause_menu.apply_theme_requested.is_connected(_on_pause_apply_theme):
        pause_menu.apply_theme_requested.connect(_on_pause_apply_theme)
    if not pause_menu.load_requested.is_connected(_on_pause_load):
        pause_menu.load_requested.connect(_on_pause_load)
    pause_menu.set_save_slots(save_slots)

func _show_pause_menu() -> void:
    if pause_layer != null:
        pause_layer.visible = true

func _hide_pause_menu() -> void:
    # **HIDING A `CanvasLayer` DOES NOT RELEASE FOCUS.** `CanvasLayer` is not a `CanvasItem`, so its
    # `visible` never reaches the Controls under it as a visibility change and a focused field keeps
    # the keyboard after the menu is gone. That is the stuck-focus half of `MapView`'s polled-input
    # guard: WASD would stay dead for the rest of the session with nothing on screen to explain it.
    if pause_menu != null and pause_menu.has_method("release_text_focus"):
        pause_menu.call("release_text_focus")
    if pause_layer != null:
        pause_layer.visible = false

## Abandon ENDS the run, so the parameters it was built from stop being anybody's answer: the landing
## screen owns the next world's, and leaving this run's armed would let a later theme apply there
## rebuild a world the player already walked away from.
func _on_pause_abandon() -> void:
    var launch: Node = get_node_or_null("/root/GameLaunch")
    if launch != null:
        launch.set("active_new_game", null)
        launch.set("active_load_slot", "")
    get_tree().change_scene_to_file("res://src/ui/LandingScreen.tscn")

func _on_pause_exit() -> void:
    get_tree().quit()

## The Options pane's "Apply now" — install the picked theme and rebuild the scene so it shows. This
## ENDS the run: the reload re-runs `_ready`, which reconnects and sends `new_game`, so the server
## builds a new world rather than handing this one back. That is what the armed button and its caption
## warn about. Nothing quits and nothing is spawned.
func _on_pause_apply_theme() -> void:
    GameLaunch.apply_theme_now()

## **LOADING FROM INSIDE A RUN DISCARDS THAT RUN**, which is what the pause pane's armed
## "Load — discards this run" button says in its own label. It is performed as a SCENE RELOAD with
## the slot armed, not as a `load_game` sent from here: the reload re-runs `_ready`, which puts the
## loading overlay back up, re-captures the reveal baseline and sends the load through the ordinary
## retry-until-answered path (`.claude/rules/core_sim/world-handoff.md`). Sending it in place would
## leave a live HUD rendering the old world while the server built another one.
func _on_pause_load(slot: String) -> void:
    var launch: Node = get_node_or_null("/root/GameLaunch")
    if launch != null:
        launch.set("pending_load_slot", slot)
        launch.set("pending_new_game", null)
    get_tree().reload_current_scene()

## **DECIDE WHICH WORLD THIS RUN IS, AND HOW TO ASK FOR IT.** Either a `load_game <slot>` (the
## `GameLaunch.pending_load_slot` handoff) or a `new_game <preset> <w> <h> <seed> <profile>` built
## from `pending_new_game`, or the dev default when the scene was launched directly.
##
## Clears whichever handoff it consumed so a later scene reload starts fresh, and records what it
## RESOLVED to — `GameLaunch.active_new_game` / `active_load_slot`. The handoff slots are empty from
## here on, so that record is the only thing that can tell a later reload (a theme apply) which world
## this run was, rather than sending it to the dev default.
func _build_world_request() -> void:
    var params: Dictionary = DEV_DEFAULT_NEW_GAME
    var launch: Node = get_node_or_null("/root/GameLaunch")
    # **A PENDING LOAD WINS.** The two slots are never armed together, and a load is the more
    # specific request: it names the exact world to stand up, where new_game only names how to
    # generate one. Consume-and-clear, the same contract `pending_new_game` follows.
    if launch != null:
        _pending_load_slot = String(launch.get("pending_load_slot"))
        if _pending_load_slot != "":
            launch.set("pending_load_slot", "")
            launch.set("pending_new_game", null)
            launch.set("active_load_slot", _pending_load_slot)
            launch.set("active_new_game", null)
            _new_game_command = {
                "line": "",
                "message": "Loading “%s”." % _pending_load_slot,
            }
            return
        launch.set("active_load_slot", "")
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
    # The POST-fallback, post-clamp values, so a re-armed launch asks for exactly the world this run
    # got — including when the fallback is what supplied them.
    if launch != null:
        launch.set("active_new_game", {
            "preset_id": preset,
            "width": width,
            "height": height,
            "seed": seed_value,
            "profile_id": profile,
        })

## Send the pending world request. A `new_game` goes through the SAME transport MapPanel uses for
## map_size (inspector.send_runtime_command → command socket); a `load_game` goes through the save
## seam, because it is answered rather than merely accepted. Retried from _process until it lands, so
## a command socket still connecting at _ready doesn't drop the request.
##
## `_new_game_command` is deliberately KEPT after a successful send — `_tick_new_game_retry`'s answer
## timeout re-sends the very same request, and clearing it here would leave nothing to re-send.
func _try_send_world_request() -> void:
    if _new_game_sent or _new_game_command.is_empty():
        return
    if _pending_load_slot != "":
        # A load is not a text command: it carries a request id and is ANSWERED on the query channel
        # (`.claude/rules/core_sim/save-game.md`), so it goes out through the save seam. The latch is
        # set on a successful dispatch exactly as it is for `new_game` — the reveal gate, not the
        # reply, is what ends the retry.
        if save_slots == null or save_slots.is_busy():
            return
        if save_slots.request_load(_pending_load_slot):
            _new_game_sent = true
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
    _try_send_world_request()
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

## Re-word the overlay that is already up. Its only caller is a REFUSED load: the overlay is the one
## surface the player is looking at, and "Generating world…" is a lie once the server has said no.
func _set_loading_overlay_text(text: String) -> void:
    if loading_overlay == null:
        return
    for child in loading_overlay.get_children():
        if child is Label:
            (child as Label).text = text
            return


## **THE CONFIG-DRIFT NOTICE, over the world it is about.** Raised once, on the reveal that follows a
## successful load, and only when the drift list is non-empty — empty is the good case and gets no
## interruption. The sim deliberately does not save config, so a save written before a balance change
## plays under this build's tuning; the notice names the files so the player knows which numbers moved
## (`.claude/rules/core_sim/save-game.md`).
func _show_config_drift_notice() -> void:
    if _pending_config_drift.is_empty():
        return
    var rows: Array = _pending_config_drift
    # Consumed here, so a later world reveal in the same process cannot re-raise the previous load's
    # warning.
    _pending_config_drift = []
    var layer := CanvasLayer.new()
    layer.layer = DRIFT_NOTICE_LAYER
    _drift_notice = ConfigDriftNotice.new()
    layer.add_child(_drift_notice)
    add_child(layer)
    _drift_notice.show_drift(rows)
    _drift_notice.dismissed.connect(func():
        _drift_notice = null
        layer.queue_free())

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
    if snapshot.has("server_build"):
        _hud_invoke("update_build_info", [String(snapshot["server_build"])])
    if snapshot.has("sedentarization") and SnapshotSections.changed(snapshot, "sedentarization"):
        _hud_invoke("update_sedentarization", [snapshot["sedentarization"]])
    # **`demographics` IS DISPATCHED NOWHERE — the wire field has no client reader at all** since the
    # top bar's `Pop 100 👶34 🛠16 🧓5` line was retired (issue #450). The faction page's PEOPLE bar
    # answers the same question and answers it from the BANDS, apportioned once across the roster, so
    # a second per-faction total would be a second source of truth for the head count. The sim still
    # publishes the section; it joins `accessibleStockpile` as an unread wire table.
    # **THE LADDER'S KNOWLEDGE ROSTER GOES FIRST**, because the progress list below is read AGAINST
    # it: the roster says what there is to learn and where each knowledge sits, and the row beside it
    # says how far this faction has got. A per-world constant, so a delta restates it only on a world
    # rebuild and the `changed` gate skips it every other turn.
    if snapshot.has("ladder_knowledge") and SnapshotSections.changed(snapshot, "ladder_knowledge"):
        _hud_invoke("update_ladder_knowledge", [snapshot["ladder_knowledge"]])
    # …and the ROUTE branch's rung catalog beside it, another per-world constant. It is what lets the
    # tile card's road action open a whole ladder rather than one button per verb, so a rung added to
    # `intensification_ladder.json` reaches the player with no client edit.
    if snapshot.has("route_rungs") and SnapshotSections.changed(snapshot, "route_rungs"):
        _hud_invoke("update_route_rungs", [snapshot["route_rungs"]])
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
    # The CONTACT TIES (arc #527). Their one consumer is the trade sheet's destination picker, which
    # renders a band's ties and nothing else — a tie is what gates a shipment. Gated like every other
    # whole section: absence means unchanged, so a quiet turn leaves the ties standing.
    if snapshot.has("connections") and SnapshotSections.changed(snapshot, "connections"):
        _hud_invoke("update_connections", [snapshot["connections"]])
    if snapshot.has("kits") and SnapshotSections.changed(snapshot, "kits"):
        # The KIT ROSTER + the FOUR job defaults, forwarded as ONE call: the compose sheets' pickers
        # need the list and the "what does the verb take when I name none" answer together, and a
        # roster ingested without its defaults would open every picker on nothing. Gated on `kits`
        # alone — the defaults are scalars riding the same section and change with it. The scout and
        # warrior entries arrived with the expanded roster; before it the band-wide roles had no kit
        # axis and so no default to name.
        _hud_invoke("update_kit_roster", [snapshot["kits"],
            snapshot.get("default_hunt_kit_id", ""), snapshot.get("default_forage_kit_id", ""),
            snapshot.get("default_scout_kit_id", ""), snapshot.get("default_warrior_kit_id", "")])
    # The CRAFTING CATALOGUES, forwarded as ONE call for the reason the kit roster is: they are one
    # fact, and a recipe book ingested without its materials renders a rail with no craft tracks and
    # costs in materials the panel cannot name. **Gated on `craft_knowledge`, not on `materials`** —
    # the other three are per-world constants that ride a delta only on a world rebuild, while a craft
    # is LEARNED, so knowledge is the one of the four that moves in play and therefore the one whose
    # arrival must re-render the panel.
    #
    # The other three are passed as `null` when the frame does not carry them, NOT as `[]`: the
    # ingest ignores a non-Array and lets the last value stand, so absence means unchanged — where an
    # empty array would mean "the world has no materials" and blank the rail.
    if snapshot.has("craft_knowledge") and SnapshotSections.changed(snapshot, "craft_knowledge"):
        _hud_invoke("update_crafting_catalogues", [snapshot.get("materials", null),
            snapshot.get("characteristic_bands", null), snapshot.get("recipes", null),
            snapshot["craft_knowledge"]])
    if snapshot.has("forage_patches") and SnapshotSections.changed(snapshot, "forage_patches"):
        # The HUD needs the forage patches to cap each Current-actions Forage row's worker stepper at
        # the patch's max-useful (the same forecast the compose control reads off tile_info). Same
        # array MapView ingests into `forage_patch_lookup`.
        _hud_invoke("update_forage_patches", [snapshot["forage_patches"]])
    # THE ROADS IN THE GROUND (arc #532). The map ingests the same section into `road_network`; the
    # HUD needs it because a road tile is the only source a route knowledge can be STANDING ON, and
    # the knowledge screen's *"is anything using this"* verdict is asked of the faction's own sources.
    if snapshot.has("routes") and SnapshotSections.changed(snapshot, "routes"):
        _hud_invoke("update_road_network", [snapshot["routes"]])
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
    # appended since the baseline, a full snapshot carries the whole retained ring.
    #
    # **THERE ARE THREE CONSUMERS AND EACH DEFENDS ITSELF DIFFERENTLY; re-ingesting a full ring is
    # harmless only because every one of them does.** The Telling and the event dock ACCUMULATE and
    # de-duplicate — the Telling deliberately NOT reset on a full frame, keeping its own scrolled-off
    # history and de-duping on a signature rather than on `seq`; the dock resets and de-dupes on `seq`.
    # The THIRD is the turn orb's crew-hand-off producer (`AttentionController.ingest_command_events`),
    # which does neither: it is a WINDOW on one turn, so it filters on the event's own `tick` and
    # de-duplicates on `seq` itself. It went in reading every matching row on the array, which turned a
    # resync's twenty-turn ring into twenty turns of hand-offs all dated today.
    if snapshot.has("command_events") and SnapshotSections.changed(snapshot, "command_events"):
        _hud_invoke("ingest_command_events", [snapshot["command_events"]])
    # The dock's four per-frame steps are one seam, because their ORDER is the contract and stating
    # it in one place is what keeps it stated at all — see `apply_event_dock_frame`.
    apply_event_dock_frame(event_dock, snapshot, is_delta)
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
    # The Workbench takes both frame kinds through ONE entry point and gates on its own visibility,
    # so a hidden surface costs a cached reference and nothing else.
    if workbench != null:
        workbench.update_snapshot(snapshot, not is_delta)
    var t_selection: int = profile.begin(PROFILE_SELECTION)
    _refresh_hud_selection()
    profile.end(PROFILE_SELECTION, t_selection)
    # RE-PUSH THE BAND CARD'S LATERAL BOUNDS, here at the end of the fan-out that just moved the HUD
    # columns they measure. The other caller is `_apply_reservation`, and the band panel's reservation
    # is fixed per dock edge — so on its own the bound is sampled on dock/collapse/hide/resize and never
    # again, while the live widths move in ordinary play (the top-bar metrics line grows as its numbers
    # gain digits; `L`/`V`/`R` toggle right-dock cards). A stale bound leaves the card drawn over the
    # readouts, which is the exact failure it exists to prevent. `set_lateral_bounds` early-outs on an
    # unchanged pair, so the per-turn cost is two `maxf`s and a compare — no relayout unless it moved.
    _update_band_panel_lateral_bounds()
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
    if workbench != null:
        workbench.reset_pages()
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
## **THE KIT TOKEN — `kit <id>`, NAMED, SPACE-SEPARATED AND ORDER-INDEPENDENT** (the parser's existing
## `name value` style, as in `queue_espionage_mission … owner 1 target 2`). It is lifted out of the
## tail before any positional form is read, so it may sit anywhere after the role and none of the four
## grammars has to make room for it.
##
## **IT IS OMITTED WHEN THE CHOICE EQUALS THE JOB DEFAULT**, which is also what absent means to the
## parser — so a composition that never touched the picker emits the byte-identical line it emitted
## before the picker existed. `""` on either side
## (a sheet composed before a roster landed, a role with no kit axis) likewise emits nothing.
static func _kit_token(payload: Dictionary) -> String:
    var kit_id := String(payload.get("kit_id", "")).strip_edges()
    var default_id := String(payload.get("default_kit_id", "")).strip_edges()
    if kit_id == "" or kit_id == default_id:
        return ""
    return " kit %s" % kit_id

## **THE SELECTIVE-GATHER TOKEN — `take:emmer,flax`, a PREFIX rather than a third positional.** The
## forage tail's two optional tokens are already told apart by shape (a floor parses as a float, a
## species key never does) and a third would be indistinguishable from the commit species, so the
## parser lifts this one out of the tail wherever it sits — exactly like `kit`, and for the same
## reason. One token, comma-separated, because a `flora_config.json` species key is snake_case and
## never contains a comma.
##
## **AN EMPTY SELECTION EMITS NOTHING**, which is what *"take the whole basket"* means to the parser —
## so a composition that never touched the chips produces the byte-identical line it produced before
## this axis existed. That is also why the client must send the FULL selection on every commit: an
## omitted token CLEARS the row's selection sim-side rather than leaving it alone.
const TAKE_SELECTION_PREFIX := " take:"
const TAKE_SELECTION_SEPARATOR := ","
static func _take_species_token(payload: Dictionary) -> String:
    var keys := PackedStringArray()
    for key in payload.get("take_species", []):
        var trimmed := String(key).strip_edges()
        if trimmed != "":
            keys.append(trimmed)
    if keys.is_empty():
        return ""
    return TAKE_SELECTION_PREFIX + TAKE_SELECTION_SEPARATOR.join(keys)

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
            # The kit rides the TAIL as a named pair, so it never has to be disambiguated against the
            # two optional positionals above it (a floor parses as a float, a species key never does,
            # and `kit` is neither).
            forage_line += _kit_token(payload)
            # The selective gather rides the tail too, and for the identical reason: it is lifted out
            # before any positional form is read, so it never has to be disambiguated against the
            # floor or the commit species above it.
            forage_line += _take_species_token(payload)
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
                "line": "assign_labor %d %d hunt %s %s %d%s" % [
                    faction, band_id, herd_id, _format_floor(payload), workers,
                    _kit_token(payload)],
                "message": "Assign %d hunter%s to %s, leaving %s standing." % [
                    workers, "" if workers == 1 else "s", herd_id, _floor_percent_text(payload)],
            }
        "scout", "warrior", "agriculture", "husbandry", "roadwork", "builders":
            # **A BAND-WIDE ROLE CARRIES THE KIT TOKEN TOO, and it is the only optional token these
            # rows take.** They have no tile, no herd, no floor and no species — the sim ignores
            # every one of those on a role target — but `kit_job()` answers for all four
            # roles now, so `assign_labor … scout 3 kit none` is a real selection rather than a token
            # dropped on the floor. Same `_kit_token` omission rule as the other two branches, so a
            # player who never opened the role card's picker emits the line they always did.
            #
            # **THE THREE KEEPING ROLES RIDE THE SAME BRANCH** (`docs/plan_standing_upkeep.md` §2.5,
            # arc #532). `agriculture`, `husbandry` and `roadwork` are band-wide standing roles in
            # exactly the grammar scout and warrior use, so a branch of their own would be the same
            # line typed three times. They send no kit today — the role cards mount no picker, the
            # wire naming no default kit for any of the three jobs (`default_kits.roadwork` is the
            # bare `none` kit, so road keepers work bare-handed and that is intended) — and
            # `_kit_token` omits an empty selection.
            #
            # **AND SO DOES `builders`, which the sim has always parsed and this builder DID NOT
            # NAME** — a role omitted from this match answers `{}`, so the Builders card's stepper
            # emitted no command at all and the pool could not be staffed from the UI. It is the one
            # role here whose card DOES mount a kit picker, and the token is deliberately sent only
            # for a kit the player picked: an explicit `kit` on the builders row overrides the sim's
            # per-entry derivation for good (`BandPanelController._commanded_role_kit_id`).
            return {
                "line": "assign_labor %d %d %s %d%s" % [
                    faction, band_id, kind, workers, _kit_token(payload)],
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
##
## **THE GRAMMAR IS CLOSED AFTER THE FLOOR**, like `send_denial_raid`'s. A second positional (the
## retired fill target, issue #491) is now an `UnexpectedArgument` parse error rather than an ignored
## token, so a token appended here fails the command outright instead of degrading quietly.
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
    # …and the kit LAST, as a named pair. It has to come after the positionals: the parser lifts it
    # out of the tail before reading them, but a human reading the log should see the positional
    # grammar unbroken.
    line += _kit_token(payload)
    # The COMMAND addresses the herd by its id; the FEED NOTE names the species. `game_deer_07` is a
    # database key — meaningless to a player — so it must never reach the feed. Hud sends the display
    # name alongside the key; fall back to the key only if it somehow didn't (better than an empty
    # subject, and it is never the normal path).
    var fauna_label := String(payload.get("fauna_label", "")).strip_edges()
    if fauna_label == "":
        fauna_label = fauna_id
    # The receipt names the one order a raid carries — how deep to draw the herd.
    var orders := "leaving %s standing" % _floor_percent_text(payload)
    return {
        "line": line,
        "message": "Send hunting expedition (%d, %s) after %s." % [
            party_workers, orders, fauna_label],
    }

## `send_denial_raid <faction_id> <band_id> <party_workers> <fauna_id>` (`docs/plan_denial_raid.md`).
##
## **THE GRAMMAR IS CLOSED AT FOUR TOKENS AND A FIFTH IS A HARD PARSE ERROR** — which is the command
## layer saying what the mission says: denial carries no floor and no fill target, so there is no
## optional trailing token to append and none may be invented. That is also why this is a builder of
## its own rather than a branch of `format_send_hunt_expedition`, whose two optional tails would be
## rejected here.
static func format_send_denial_raid(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var party_workers := int(payload.get("party_workers", 0))
    var fauna_id := String(payload.get("fauna_id", "")).strip_edges()
    if party_workers <= 0 or fauna_id == "":
        return {}
    # The COMMAND addresses the herd by its database key; the FEED NOTE names the species, the
    # `format_send_hunt_expedition` rule — `game_deer_07` must never reach a player-facing line.
    var fauna_label := String(payload.get("fauna_label", "")).strip_edges()
    if fauna_label == "":
        fauna_label = fauna_id
    return {
        # **THE ONE THING THE CLOSED GRAMMAR ADMITS, AND IT IS NOT A NUMBER.** A kit is a property of
        # the PARTY, not of the mission, so it is the only order a raid carrying no floor and no fill
        # target still has to give — and it rides as a named pair rather than as the fifth positional
        # the parser refuses.
        "line": "send_denial_raid %d %d %d %s%s" % [faction, band_id, party_workers, fauna_id,
            _kit_token(payload)],
        # The receipt states the whole order, because the whole order is two things: a herd and a
        # party. There is no third clause to quote and none to omit.
        "message": "Send denial raid (%d) against %s." % [party_workers, fauna_label],
    }

## `send_trade_expedition <faction_id> <band_id> <party_workers> <destination_band_id>`
## `[food <amount>] [material <material_id> <amount>]... [kit <id>]` (arc #527, issue #517).
##
## **THE TAIL IS A NAMED, REPEATED MANIFEST, and that is what makes this its own builder.** A
## shipment has no fixed arity — it is a list of lines — and a positional list could not say which of
## two namespaces an id belongs to (`provisions` is a commodity key, `hide` is a material id, and the
## two tables are authored independently). So each row emits `food <amount>` or
## `material <id> <amount>` in the order the player built it, and the parser refuses any other token
## outright rather than dropping it.
##
## **AN EMPTY MANIFEST IS NOT REFUSED HERE.** It parses, and the SERVER answers whether a shipment
## with nothing in it is legal — that question is about the band and its packs. What this refuses is
## a line it cannot address: no band, no destination, or no party.
##
## **EVERY AMOUNT IS FLOORED, NEVER ROUNDED** — `cargo_wire_amount`, and the reason it is a
## correctness rule rather than a formatting one is written there.
static func format_send_trade_expedition(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var destination_band_id := int(payload.get("destination_band_id", HudConst.NO_BAND_ID))
    if destination_band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var party_workers := int(payload.get("party_workers", 0))
    if party_workers <= 0:
        return {}
    var line := "send_trade_expedition %d %d %d %d" % [
        faction, band_id, party_workers, destination_band_id]
    var food_total := 0.0
    var material_terms: Array[String] = []
    for row_variant in Array(payload.get("cargo", [])):
        if not (row_variant is Dictionary):
            continue
        var row: Dictionary = row_variant
        var amount := float(row.get("amount", 0.0))
        if amount <= 0.0:
            continue
        if bool(row.get("is_material", false)):
            var material_id := String(row.get("id", "")).strip_edges()
            if material_id == "":
                continue
            # Each material row is emitted on its own — the server sums the rows of one id itself —
            # so each is floored on its own, and the sum of floors is still inside the pile.
            var material_amount := cargo_wire_amount(amount)
            if material_amount <= 0.0:
                continue
            line += " material %s %s" % [material_id, cargo_wire_text(material_amount)]
            material_terms.append("%s %s" % [
                TRADE_CARGO_NOTE_AMOUNT_FORMAT % material_amount, material_id])
        else:
            # **THE FOOD LINES ARE SUMMED INTO ONE TOKEN, and only the food lines.** `food` names one
            # commodity, so two rows of it are one quantity; two rows of `hide` are two PILES at two
            # ratings and merging them would rebuild the retired trade scalar out of the vector that
            # replaced it. The sheet composes one food row anyway — this is the guard, not a feature.
            food_total += amount
    # …and the food TOTAL is floored once, after the sum: the server compares the whole `food` token
    # against one larder, so flooring the parts would spend the allowance twice.
    food_total = cargo_wire_amount(food_total)
    if food_total > 0.0:
        line += " food %s" % cargo_wire_text(food_total)
    # …and the kit LAST, as a named pair, the `format_send_hunt_expedition` convention: the parser
    # lifts it out of the tail, but a human reading the log sees the positional grammar unbroken.
    line += _kit_token(payload)
    # The COMMAND addresses the destination by its `BandId`; the FEED NOTE names the people. A raw id
    # is a database key and must never reach a player-facing line — the `fauna_label` rule.
    var destination_label := String(payload.get("destination_label", "")).strip_edges()
    if destination_label == "":
        destination_label = str(destination_band_id)
    var manifest := TRADE_CARGO_TERM_SEPARATOR.join(
        ([TRADE_CARGO_FOOD_TERM_FORMAT % (TRADE_CARGO_NOTE_AMOUNT_FORMAT % food_total)] if food_total > 0.0 else [])
        + material_terms)
    if manifest == "":
        manifest = TRADE_CARGO_EMPTY_TERM
    return {
        "line": line,
        "message": "Send shipment (%d) to %s carrying %s." % [
            party_workers, destination_label, manifest],
    }

## **A CARGO AMOUNT AS THE COMMAND LINE MAY NAME IT — FLOORED, NEVER ROUNDED** (arc #527, issue #517).
##
## **Rounding is wrong in one direction, and the direction is fatal.** `resolve_shipment` compares
## STRICTLY (`held < amount` refuses) against a `Scalar` store, and the compose sheet's own `+` clamps
## a press to the pile — `BandPanelController._set_cargo_amount`, whose documented one-press path
## therefore leaves the EXACT fractional held amount on the row (`137.456789` food, a `4.56789` hide
## batch). A `%.1f` there emits `137.5`, and the server answers *"the band holds 137.46 provisions,
## not 137.50"*: the one-press path the sheet teaches is refused. The same rounding can push the
## manifest's mass over the cap AFTER this client's own meter said it fit.
##
## So the emitted amount may never exceed the held one, and this floors it onto **both** grids it has
## to survive:
##
## 1. the sim's fixed-point grid (`SIM_SCALAR_DECIMALS`), the finest quantity a store can hold;
## 2. the 32-bit float the command TEXT is parsed back through (`WIRE_FLOAT_EPSILON` ×
##    `WIRE_FLOAT_BACKOFF_STEPS`), which above ~8 units is the coarser of the two — an amount floored
##    onto the fixed-point grid alone still reconstructs ABOVE the pile roughly 40% of the time,
##    because `parse_f32` and the `×10^6` that follows it each round to nearest.
##
## Answers `0.0` for anything that does not survive as a positive amount, which the caller drops:
## a `food 0.000000` token is a command failure with a reason, not an empty shipment.
static func cargo_wire_amount(amount: float) -> float:
    if amount <= 0.0:
        return 0.0
    var scale := pow(10.0, SIM_SCALAR_DECIMALS)
    var floored := floorf(amount * scale) / scale
    var backoff := floored * WIRE_FLOAT_EPSILON * WIRE_FLOAT_BACKOFF_STEPS
    return maxf(floorf((floored - backoff) * scale) / scale, 0.0)

## …and how it is SPELLED — every digit the sim's fixed point can hold, so the floored value survives
## the trip as text. The digit count is the sim's own precision rather than a literal.
static func cargo_wire_text(amount: float) -> String:
    return ("%%.%df" % SIM_SCALAR_DECIMALS) % amount

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

## `split_band <faction_id> <band_id> <workers>` — a band divides in two where it stands
## (issue #511). Same handle rule as `recall_expedition` above: the band is named by its durable id,
## never by its ECS entity bits. **The grammar is CLOSED at three positional tokens** — the sim's
## parser rejects a fourth outright, the worker count being the only input a split takes.
static func format_split_band(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var workers := int(payload.get("workers", 0))
    if workers <= 0:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    return {
        "line": "split_band %d %d %d" % [faction, band_id, workers],
        "message": "Form a new band.",
    }

## `extend_pen <faction> <x> <y>` targets the pen's ANCHOR TILE, so it names no band at all — it is
## here for company, not because it carries a band handle.
##
## ⛔ **THE GRAMMAR IS CLOSED AT THREE TOKENS, and a trailing crew is a PARSE ERROR**
## (`docs/plan_standing_upkeep.md` §2.5). It took a required worker count for one slice, back when a
## ring staffed a per-source build allocation. A ring rides the same `animal:pen` rung as the pen it
## widens and is funded exactly like every other build now: this DECLARES it, appending an entry to
## the build queue of every band keeping the pen, and that band's `builders` pool raises it when it
## reaches the head.
static func format_extend_pen(payload: Dictionary) -> Dictionary:
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    return {
        "line": "extend_pen %d %d %d" % [faction, x, y],
        "message": "Queue another ring on the pen at (%d, %d)." % [x, y],
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

## ⛔ **THE ROUTE BRANCH'S TWO VERBS NAME A BAND, AND THE REST DO NOT** (arc #532). `grade` and `pave`
## are `cultivate`/`sow`'s grammar **plus a band token** —
## `grade <faction> <band> <x> <y>` — because a patch's keeper is whoever is already foraging it while
## **a road has no work row at all**, so who will keep the tile has to be said out loud. That token is
## also what the sim records as the keeper: issuing the verb declares the job AND names who holds it,
## which are the same act. Read off `SourceForecast.ROUTE_IMPROVEMENTS` rather than restated, so the
## branch's verbs are spelled once.
const IMPROVEMENT_BAND_TARGETED := SourceForecast.ROUTE_IMPROVEMENTS

## The band a road verb names when the payload carries none. A road really has to be somebody's job,
## so this is a REFUSAL rather than a default — see `format_improvement`.
const IMPROVEMENT_NO_BAND := -1

## ⛔ **NONE OF THESE VERBS TAKES A WORKER COUNT, and a trailing one is a PARSE ERROR**
## (`docs/plan_standing_upkeep.md` §2.5). Each carried the build's own crew for one slice; they
## DECLARE now — the verb appends an entry to a build queue, and the hands stand on
## `assign_labor <faction> <band> builders <n>`.
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
            "message": "%s %s — queued for this band's builders." % [
                improvement.capitalize(), herd_id],
        }
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    if improvement in IMPROVEMENT_BAND_TARGETED:
        # **NO BAND, NO COMMAND.** The token is the keeper, and a road with nobody on the hook is not
        # a road the sim will accept — so an absent band is refused here rather than guessed at, the
        # way an absent herd id is one branch up.
        var band := int(payload.get("band_id", IMPROVEMENT_NO_BAND))
        if band == IMPROVEMENT_NO_BAND:
            return {}
        return {
            "line": "%s %d %d %d %d" % [improvement, faction, band, x, y],
            "message": "%s (%d, %d) — this band's road now, queued for its builders." % [
                improvement.capitalize(), x, y],
        }
    return {
        "line": "%s %d %d %d" % [improvement, faction, x, y],
        "message": "%s (%d, %d) — queued for this band's builders." % [
            improvement.capitalize(), x, y],
    }

## **`unqueue <faction> <x> <y>` | `unqueue <faction> <herd_id>` — THE WITHDRAWAL**
## (`docs/plan_standing_upkeep.md` §2.5). It drops the source's build-queue entry on every band of the
## faction working it and leaves the row, its take crew, its kit and the meter exactly as they are.
##
## **IT NAMES A SOURCE, NOT A RUNG, and that is why it is its own builder rather than a branch of
## `format_improvement`.** A band holds at most one entry per source, so there is nothing for a verb
## argument to disambiguate — and the source's two shapes are told apart the way the sim's parser
## tells them apart: **two integer tokens are a TILE, one token is a HERD id**. A herd id wins when it
## is non-empty, which is the only way a caller can state the herd form unambiguously.
##
## **IT IS THE UNDO FOR A DECLARATION, NEVER FOR A BUILD WITH WORK ON IT.** `abandon` is what puts a
## source with a live meter down, and it has its own builder one block up (`format_abandon`) reached
## from the road ladder's own control.
static func format_unqueue(payload: Dictionary) -> Dictionary:
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var herd_id := String(payload.get("herd_id", "")).strip_edges()
    if herd_id != "":
        return {
            "line": "unqueue %d %s" % [faction, herd_id],
            "message": "Withdraw the build queued on %s." % herd_id,
        }
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    return {
        "line": "unqueue %d %d %d" % [faction, x, y],
        "message": "Withdraw the build queued on (%d, %d)." % [x, y],
    }

## **`abandon <faction> <x> <y>` | `abandon <faction> <herd_id>` — PUT THE HOLDING DOWN.**
##
## ⛔ **IT IS NOT `unqueue`, AND THE PAIR IS THE WHOLE POINT OF HAVING TWO BUILDERS.** `unqueue` drops
## a source's build-queue ENTRY and leaves the row, its crew, its kit and the meter exactly as they
## are — the undo for a declaration. This releases the HOLDING: the band's grip on the source and, for
## a road, its keeping and its place in the queue together. Once any work is banked it is the only one
## of the two that does anything, which is why a road handed to the wrong band could not be taken back
## from the UI at all while this had no builder.
##
## ⛔ **IT NAMES A PLACE, NOT A HOLDING, AND THAT HAS A CONSEQUENCE THE CALLER MUST STATE.**
## `handle_abandon` drops the faction's labor rows on that tile as well as the road's keeper, because
## a tile may carry a road AND a patch and putting one down without the other would be silently partial
## on exactly the tiles where a band both farms and keeps a road. **There is no road-only form and this
## client must not invent one** — a command narrower than the sim implements would lie about what the
## button does — so the road ladder's own row says what else goes down with it.
##
## **The two source shapes are told apart the way `format_unqueue` tells them apart**, which is the way
## the sim's parser does: two integer tokens are a TILE, one token is a HERD id, and a non-empty herd
## id wins.
##
## **`abandon_improvement` IS A DIFFERENT, RETIRED VERB** — see its epitaph further down: it cleared an
## assignment's STORED improvement, which no longer exists, and the server refuses that form outright.
static func format_abandon(payload: Dictionary) -> Dictionary:
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var herd_id := String(payload.get("herd_id", "")).strip_edges()
    if herd_id != "":
        return {
            "line": "abandon %d %s" % [faction, herd_id],
            "message": "Put down what your people hold on %s." % herd_id,
        }
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    return {
        "line": "abandon %d %d %d" % [faction, x, y],
        "message": "Put down what your people hold at (%d, %d)." % [x, y],
    }

## **`build_kit <faction> <x> <y> [kit <id>]` | `build_kit <faction> <herd_id> [kit <id>]` — THE
## PER-ENTRY BUILDERS KIT** (`docs/plan_standing_upkeep.md` §4.7a ②). It names a SOURCE and sets a
## property of that source's QUEUE ENTRY on every band of the faction that has it queued; the row, its
## take crew and the banked meter are untouched.
##
## **ITS OWN BUILDER BECAUSE THE BUILDERS' KIT IS PER ENTRY, NOT PER BAND.** `assign_labor` REFUSES a
## `kit` token on the `builders` role now: a build's default is derived from that entry's own food web
## — a hoe for a Cultivate, hurdles for a Tame — and one stored id per band is the one thing that
## derivation cannot express.
##
## ⛔ **AN ABSENT `kit` TOKEN CLEARS THE OVERRIDE back to the derivation, and `_kit_token` is what
## produces it.** Its standing rule — omit the token when the selection equals the default — is
## exactly right here, so a player picking the `(default)` entry emits `build_kit 0 12 34` and the sim
## goes back to deriving. There is no `default` literal to invent, and `none` (bare-handed) survives
## the round trip as the real selection it is.
##
## The two source shapes are told apart the way `format_unqueue` tells them apart, which is the way
## the sim's own parser does: a non-empty herd id is the herd form, else two integer tokens are a tile.
static func format_build_kit(payload: Dictionary) -> Dictionary:
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var kit_face := String(payload.get("kit_id", "")).strip_edges()
    var token := _kit_token(payload)
    var message_kit := kit_face if token != "" else BUILD_KIT_DERIVED_NOTE
    var herd_id := String(payload.get("herd_id", "")).strip_edges()
    if herd_id != "":
        return {
            "line": "build_kit %d %s%s" % [faction, herd_id, token],
            "message": "Raise the build on %s with %s." % [herd_id, message_kit],
        }
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    return {
        "line": "build_kit %d %d %d%s" % [faction, x, y, token],
        "message": "Raise the build on (%d, %d) with %s." % [x, y, message_kit],
    }

## What the command FEED says when the line carries no `kit` token — the player handed the choice
## back, and *"with "* followed by nothing states nothing at all.
const BUILD_KIT_DERIVED_NOTE := "the tools this job derives for itself"

## **`upkeep_kit <faction> <x> <y> [kit <id>]` | `upkeep_kit <faction> <herd_id> [kit <id>]` — THE
## PER-SITE KEEPING KIT** (`docs/plan_standing_upkeep.md` §2.7, surfaced by §4.9 item 12c). It names a
## SOURCE and sets that site's keeping tool on every band of the faction that works it — a WIDER reach
## than `build_kit`'s, because a keeping bill is owed by every band holding the ground and not only by
## whoever queued a build on it. The take crew, its own kit, the queue entry and the meter are
## untouched.
##
## **THE BAND IS THE POOL, NOT THE DECISION.** A kit stored on the band's `agriculture` / `husbandry`
## role row — where this lived until §2.7 — is the one thing a per-site derivation cannot express: one
## pick put the same tool on every site that band kept, with no way back. That is also why the strip's
## picker needs no scope warning: there is no longer a scope to warn about.
##
## ⛔ **`none` AND "NO SELECTION" ARE DIFFERENT STATES, AND GETTING IT BACKWARDS IS SILENT.** An
## ABSENT `kit` token clears the site back to its own web derivation; **`kit none` is bare-handed and
## is a real selection**, which is how a player conserves the tool on one site while its neighbour
## goes on using it. `_kit_token`'s standing rule produces both: it omits the token when the pick
## equals the default, and `none` is an ordinary roster member whose id is never equal to a derived
## kit's. `KitRoster.NO_KIT_ID` (`""`) is the third thing — *nothing to say* — and also omits.
##
## The two source shapes are told apart exactly as `format_build_kit` tells them apart, which is how
## the sim's own parser does it: a non-empty herd id is the herd form, else two integers are a tile.
static func format_upkeep_kit(payload: Dictionary) -> Dictionary:
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var kit_face := String(payload.get("kit_id", "")).strip_edges()
    var token := _kit_token(payload)
    var message_kit := kit_face if token != "" else UPKEEP_KIT_DERIVED_NOTE
    var herd_id := String(payload.get("herd_id", "")).strip_edges()
    if herd_id != "":
        return {
            "line": "upkeep_kit %d %s%s" % [faction, herd_id, token],
            "message": "Keep %s with %s." % [herd_id, message_kit],
        }
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    return {
        "line": "upkeep_kit %d %d %d%s" % [faction, x, y, token],
        "message": "Keep (%d, %d) with %s." % [x, y, message_kit],
    }

## The keeping twin of `BUILD_KIT_DERIVED_NOTE`, and its own string because the two sentences say
## different things about the same absence: a build derives its kit from the ENTRY's food web, a site
## from the SITE's.
const UPKEEP_KIT_DERIVED_NOTE := "the tools this site derives for itself"

## **WHAT THE PLAYER CALLS THE FIRST SLOT OF A QUEUE.** The wire's `position` is a 0-based INDEX and
## stays one; the sim's own reply spells the landed slot `#{landed + 1}`, so the echo beside it adds
## the same one. Named because the two bases are a real distinction here — the token and the sentence
## in one function must not drift onto one number.
const BUILD_QUEUE_POSITION_LABEL_BASE := 1

## **`build_order <faction> <band> <x> <y> <position>` | `build_order <faction> <band> <herd_id>
## <position>` — THE REORDER** (`docs/plan_standing_upkeep.md` §4.7b ③), emitted by the BUILD QUEUE
## block's drag.
##
## **THE ORDER IS THE FUNDING DECISION**: the whole `builders` pool stands on the HEAD entry until its
## meter fills, then on the next — so a position is not a label, it is when the job gets built.
##
## **IT NAMES A BAND where `build_kit` and `unqueue` do not**, and the asymmetry is the sim's: a queue
## belongs to a band, while a kit and a withdrawal are properties of the entry every band holding that
## source has. `position` is 0-based on the WIRE and the sim clamps it to the queue's length.
##
## ⛔ **THE ECHO SPELLS THAT POSITION THE SIM'S WAY — 1-based, `#n`.** `handle_build_order` answers the
## same action with *"… is now #2 in the build queue"* (`core_sim/src/bin/server.rs`), deliberately
## 1-based, and the two land in the dock on the SAME turn: a 0-based echo put two different numbers on
## one drag, one row apart. The wire token stays 0-based — it is the sim's index, not a label.
static func format_build_order(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var position: int = max(0, int(payload.get("position", 0)))
    var herd_id := String(payload.get("herd_id", "")).strip_edges()
    if herd_id != "":
        return {
            "line": "build_order %d %d %s %d" % [faction, band_id, herd_id, position],
            "message": "Move the build on %s to #%d in the build queue."
                % [herd_id, position + BUILD_QUEUE_POSITION_LABEL_BASE],
        }
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    return {
        "line": "build_order %d %d %d %d %d" % [faction, band_id, x, y, position],
        "message": "Move the build on (%d, %d) to #%d in the build queue."
            % [x, y, position + BUILD_QUEUE_POSITION_LABEL_BASE],
    }

## **`work_priority <faction> <band> <x> <y> <level>` | `work_priority <faction> <band> <herd_id>
## <level>` — THE PLAYER'S OWN RANK ON ONE WORKED ROW** (`docs/plan_standing_upkeep.md` §4.9 item 9b),
## emitted by the work inspector's priority picker.
##
## **THE LEVEL IS A WORD ON THE WIRE AND A WORD HERE.** `SourcePriority` numbers Normal 0 because a
## default costs no FlatBuffers bytes, while the band sheds Low, Normal, High — so the ordinal is not
## the order and this builder never sees one. The token it emits is the very string the decoder handed
## the picker, which is why nothing between the button and the socket can re-spell it.
##
## **IT NAMES A BAND, like `build_order` and unlike `build_kit` / `unqueue`.** The ordering it feeds is
## a band's: the shedding walk partitions that band's own rows and the pen-feed split serves that
## band's own stores. The source-addressed verbs are the ones whose subject is the ground rather than
## the holding.
##
## The two source shapes are told apart exactly as `format_build_order` tells them apart, which is how
## the sim's own parser does it: a non-empty herd id is the herd form, else two integer tokens name a
## tile.
static func format_work_priority(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var level := String(payload.get("level", "")).strip_edges().to_lower()
    if not HudWorkVocab.WORK_PRIORITY_FACES.has(level):
        return {}
    # The FEED reads the level the way the picker's own face spells it, so the echo and the button
    # the player pressed carry one word between them.
    var face := String(HudWorkVocab.WORK_PRIORITY_FACES[level])
    var herd_id := String(payload.get("herd_id", "")).strip_edges()
    if herd_id != "":
        return {
            "line": "work_priority %d %d %s %s" % [faction, band_id, herd_id, level],
            "message": "%s is now %s priority for this band." % [herd_id, face.to_lower()],
        }
    var x := int(payload.get("x", -1))
    var y := int(payload.get("y", -1))
    if x < 0 or y < 0:
        return {}
    return {
        "line": "work_priority %d %d %d %d %s" % [faction, band_id, x, y, level],
        "message": "(%d, %d) is now %s priority for this band." % [x, y, face.to_lower()],
    }

## **`set_bench <faction_id> <band_id> recipe <recipe_id>`** — put a recipe on a band's crafting bench
## (`docs/plan_crafting_and_materials.md` §7).
##
## **BOTH TAILS ARE NAMED TOKENS, and this builder sends only the first.** The grammar is
## `recipe <id> [workers <n>]`, and the crew is deliberately omitted: **the player staffs the bench**,
## and a client-chosen crew here would be a second answer to the question the `− n +` stepper exists
## to ask. Naming no crew leaves the crew where it is — nobody on a bench that was idle, and the crew
## already standing there across a swap — so the number is only ever `bench_crew`'s to set.
##
## It takes `<faction_id> <band_id>` first, like `assign_labor`, and names the band by its DURABLE
## `band_id` — never its ECS entity bits, which a rollback renumbers.
static func format_set_bench(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var recipe_id := String(payload.get("recipe_id", "")).strip_edges()
    if recipe_id == "":
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    return {
        "line": "set_bench %d %d recipe %s" % [faction, band_id, recipe_id],
        "message": "Put %s on the bench." % recipe_id,
    }

## **`clear_bench <faction_id> <band_id>`** — take the job off a band's bench. The crew returns to the
## idle pool.
##
## **IT NAMES THE BAND AND NOTHING ELSE**, one job at a time meaning there is no job argument to
## disambiguate. **The materials already drawn for the pass in flight are SPENT** — they were cut for
## the thing the player has stopped making and a band's store has no representation for a half-worked
## pile — which is why the button that emits this states the pile in its tooltip, off the published
## `drawnInputs`, rather than being guarded by a dialog.
static func format_clear_bench(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    return {
        "line": "clear_bench %d %d" % [faction, band_id],
        "message": "Cleared the bench.",
    }

## **`bench_crew <faction_id> <band_id> workers <n>`** — re-crew the running bench, leaving the job and
## its progress alone. `workers` is a NAMED token and is mandatory; `0` is a legal, meaningful value
## (the recipe stays up with nobody on it) rather than a missing argument, so it is never omitted.
static func format_bench_crew(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    var workers: int = max(0, int(payload.get("workers", 0)))
    return {
        "line": "bench_crew %d %d workers %d" % [faction, band_id, workers],
        "message": "Put %d crafter%s on the bench." % [workers, "" if workers == 1 else "s"],
    }

## **`bench_priority <faction_id> <band_id> high|normal|low` — THE PLAYER'S OWN RANK ON THE BENCH**
## (`docs/plan_standing_upkeep.md` §4.9 item 9b), emitted by the crafting panel's bench picker.
##
## **IT IS A SIBLING VERB OF `work_priority`, NOT A TOKEN OF IT.** `work_priority`'s grammar reads a
## lone trailing token as a herd id, so `work_priority <f> <b> bench low` would be ambiguous with a
## herd named `bench`. The two share the three VALUE tokens and nothing else — which is what lets one
## picker serve both, echoing back the word the decoder handed it.
##
## **IT NAMES THE BAND AND NOTHING ELSE**, one bench at a time meaning there is no job argument to
## disambiguate — the same shape `clear_bench` takes, and for the same reason. It is legal on an IDLE
## bench: a rank is a standing statement about the bench rather than about the job on it.
static func format_bench_priority(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var level := String(payload.get("level", "")).strip_edges().to_lower()
    if not HudWorkVocab.WORK_PRIORITY_FACES.has(level):
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    # The FEED reads the level the way the picker's own face spells it, so the echo and the button the
    # player pressed carry one word between them — `format_work_priority`'s rule, one verb over.
    var face := String(HudWorkVocab.WORK_PRIORITY_FACES[level])
    return {
        "line": "bench_priority %d %d %s" % [faction, band_id, level],
        "message": "The bench is now %s priority for this band." % face.to_lower(),
    }

## **RETIRED — `abandon_improvement` and its builder** (`docs/plan_standing_upkeep.md` §2.4). It
## existed to clear an assignment's STORED improvement, back when that field was the commitment; the
## build verb is DERIVED from the meter now, so there is no stored authority left to clear and a
## command that cleared a derived value would either do nothing or fight the derivation.
##
## **Walking away is `cultivate <faction> <x> <y> 0`** — the same set verb that started the build,
## with its crew at zero — so `format_improvement` above is the only builder either direction needs.
## Its proto field is reserved and the server's text parser refuses the retired form outright, so a
## stale client gets an error rather than a no-op it would read as success.
##
## **`upkeep_mode <faction> <band_id> spread|priority`** — how one band splits a keeping POOL it
## cannot stretch (`docs/plan_standing_upkeep.md` §2.5).
##
## **IT IS WHAT IS LEFT OF THE RETIRED `maintain`.** Maintenance left the tile: the keeping is two
## band-level standing roles now (`assign_labor … agriculture|husbandry <workers>`), so *where the
## hands go* is no longer a decision — the pool covers the whole web. What remains is what happens
## when the pool falls short of the summed demand, and both answers are defensible.
##
## **THE MODE IS NEVER GUESSED AT.** The sim owns the set of modes and refuses an unknown one by
## name; this builder declines an empty one rather than substituting the default, because sending
## `spread` for a control that failed to state itself would silently re-fund a band the player had
## put on `priority`. The grammar is CLOSED at three positional tokens.
static func format_upkeep_mode(payload: Dictionary) -> Dictionary:
    var band_id := int(payload.get("band_id", HudConst.NO_BAND_ID))
    if band_id == HudConst.NO_BAND_ID:
        return {}
    var mode := String(payload.get("mode", "")).strip_edges().to_lower()
    if mode == "":
        return {}
    var faction := int(payload.get("faction", PLAYER_FACTION_ID))
    return {
        "line": "upkeep_mode %d %d %s" % [faction, band_id, mode],
        "message": HudWorkVocab.UPKEEP_MODE_COMMAND_MESSAGES.get(mode,
            HudWorkVocab.UPKEEP_MODE_COMMAND_MESSAGE_FALLBACK % mode),
    }

## Send whatever a `format_*` builder produced, or nothing at all when it declined.
##
## **`false` MEANS THE SERVER DID NOT GET IT — declined by the builder or refused by the transport,
## which are the same fact to a caller holding an optimistic overlay.** See `_send_runtime_command`.
func _send_formatted_command(formatted: Dictionary) -> bool:
    if formatted.is_empty():
        return false
    return _send_runtime_command(String(formatted["line"]), String(formatted["message"]))

func _on_hud_cancel_order(band: Dictionary, scope: String) -> void:
    _send_formatted_command(format_cancel_order(band, scope))

## Early-Game Labor (slice 3b): assign/unassign working-age workers to a source or a
## band-wide role. workers==0 removes/zeroes the assignment; the server clamps totals
## to available working-age. Payload built by the HUD allocation panel / assign controls.
##
## **A SEND THAT DID NOT GO TAKES THE HUD'S OPTIMISTIC WRITE WITH IT.** The pending entry is recorded
## in the HUD before the command is emitted (`Hud._emit_assign_labor`) and the outcome is only known
## here, so the two are joined by handing the payload straight back: it carries `pending_entity`, the
## client-local handle the overlay is filed under, and `drop_pending_assign` drops THAT entry alone.
## Reached by `has_method` like every other HUD call from this script, so a client without the method
## simply keeps the old behaviour rather than erroring.
func _on_hud_assign_labor(payload: Dictionary) -> void:
    if not _send_formatted_command(format_assign_labor(payload)):
        _hud_invoke("drop_pending_assign", [payload])

## Early-Game Labor (slice 3b): relocate the band to a destination tile picked on the map.
## Same rollback rule as the labor path above — the destination stops being marked pending the moment
## the command that would have caused it is known not to have gone.
func _on_hud_move_band(payload: Dictionary) -> void:
    if not _send_formatted_command(format_move_band(payload)):
        _hud_invoke("drop_pending_move", [payload])

## Scouting expedition (docs/plan_exploration_and_sites.md §2): outfit a party off a resident
## band and send it to a target tile. The server draws the workers + provisions and spawns the
## detached party (rejects an over-cap party with a feed message).
func _on_hud_send_expedition(payload: Dictionary) -> void:
    _send_formatted_command(format_send_expedition(payload))

## Hunting expedition (docs/plan_exploration_and_sites.md §2b): outfit a party off a resident band
## and send it to follow a herd. The 4th arg is a herd id string, not tile coords.
func _on_hud_send_hunt_expedition(payload: Dictionary) -> void:
    _send_formatted_command(format_send_hunt_expedition(payload))

## Denial raid (`docs/plan_denial_raid.md`): outfit a party off a resident band and send it to break a
## herd. Its own handler because its own command — see `format_send_denial_raid`.
func _on_hud_send_denial_raid(payload: Dictionary) -> void:
    _send_formatted_command(format_send_denial_raid(payload))

## The player loaded a shipment and sent it to another band. Its own handler because its own command
## — the manifest tail no other party verb has; see `format_send_trade_expedition`.
func _on_hud_send_trade_expedition(payload: Dictionary) -> void:
    _send_formatted_command(format_send_trade_expedition(payload))

## Extend a built pen by one fenced ring (Grazing 2d-γ). The server works the ring off over ~25
## turns (rejecting at max radius / unowned / Herding-unknown with a feed message).
func _on_hud_extend_pen(payload: Dictionary) -> void:
    _send_formatted_command(format_extend_pen(payload))

## DECLARE an improvement on a source the band already works (the second axis, issue #442). It is
## sent ALONE now: the work row's `⌃` is the declaration (`docs/plan_standing_upkeep.md` §4.7a ①),
## and a row exists only for a source the band already works — so the sim's *"an improvement verb
## reaches only bands already working the source"* rule is satisfied by construction and no
## `assign_labor` has to precede it.
##
## **IT NAMES NO CREW** (`docs/plan_standing_upkeep.md` §2.5): the verb appends a build-queue entry
## and the band's `builders` pool raises whatever is at the head. An EMPTY `improvement` means *there
## is nothing here to state* — `format_improvement` answers `{}` and nothing is sent.
##
## **A SEND THAT DID NOT GO TAKES THE HUD'S OPTIMISTIC WRITE WITH IT**, exactly as the labor path's
## does: `Hud._on_work_row_improvement_requested` records the pending declaration BEFORE emitting,
## the outcome is known only here, and the payload carries `pending_entity` so the drop names that
## one entry. `format_improvement` never reads that key.
func _on_hud_improvement(payload: Dictionary) -> void:
    if not _send_formatted_command(format_improvement(payload)):
        _hud_invoke("drop_pending_assign", [payload])

## WITHDRAW a declaration — the undo for the verb above, and its own handler because its own command
## and its own grammar (it names a SOURCE, not a rung). See `format_unqueue`.
## **A SEND THAT DID NOT GO TAKES THE HUD'S OPTIMISTIC WITHDRAWAL WITH IT**
## (`docs/plan_standing_upkeep.md` §4.7b ④), the same shape `_on_hud_assign_labor` has: the HUD
## records the withdrawal BEFORE emitting, the outcome is known only here, and the payload carries
## `pending_entity` so the drop names that one entry. `format_unqueue` reads neither that key nor
## `kind`.
func _on_hud_unqueue(payload: Dictionary) -> void:
    if not _send_formatted_command(format_unqueue(payload)):
        _hud_invoke("drop_pending_unqueue", [payload])

## PUT A HOLDING DOWN — the undo for a build with work already banked on it, and the road ladder's own
## release. **No rollback**: the HUD writes nothing optimistic for it, a road having no labor row to
## shadow, so there is nothing a refused send would have to take back.
func _on_hud_abandon(payload: Dictionary) -> void:
    _send_formatted_command(format_abandon(payload))

## NAME THE KIT one queued build is raised with (`docs/plan_standing_upkeep.md` §4.7a ②) — its own
## handler because its own command and its own scope: it names a SOURCE and sets a property of that
## source's queue ENTRY, where `assign_labor` names a band and a role.
##
## **NO ROLLBACK, because there is no optimistic write to roll back.** `buildKitId` is captured LIVE
## rather than turn-written, so the recapture this command triggers carries the new value — the one
## field in the queue block that needs no client-side shadow.
func _on_hud_build_kit(payload: Dictionary) -> void:
    _send_formatted_command(format_build_kit(payload))

## NAME THE KIT one WORK SITE is kept with (`docs/plan_standing_upkeep.md` §2.7, surfaced by §4.9 item
## 12c) — its own handler because its own command and its own scope: it names a SOURCE and sets a
## property of that SITE, where `build_kit` sets a property of that site's queue entry and
## `assign_labor` names a band and a role.
##
## **NO ROLLBACK, because there is no optimistic write to roll back** — `_on_hud_build_kit`'s rule.
## `upkeepKitId` is captured LIVE rather than turn-written, so the recapture this command triggers
## already carries the new value.
func _on_hud_upkeep_kit(payload: Dictionary) -> void:
    _send_formatted_command(format_upkeep_kit(payload))

## RE-ORDER a band's build queue (`docs/plan_standing_upkeep.md` §4.7b ③).
##
## **NO ROLLBACK, because there is no optimistic write to roll back** — `_on_hud_build_kit`'s rule,
## and for the identical reason one field over. `PopulationCohortState.buildQueue` is captured LIVE
## rather than turn-written (§4.9 item 9a), so the reordered list arrives on this command's own
## recapture; the client-side ordering that used to be undone here was a second ordering beside the
## wire's, which is the drift that made a drag paint one order and then jump to another.
func _on_hud_build_order(payload: Dictionary) -> void:
    _send_formatted_command(format_build_order(payload))

## RANK one worked row against the rest of this band's (`docs/plan_standing_upkeep.md` §4.9 item 9b).
##
## **NO ROLLBACK, because there is no optimistic write to roll back** — `_on_hud_build_order`'s rule,
## and for the identical reason one field over. `LaborAssignment.priority` is captured LIVE off the
## allocation rather than turn-written, so the new mark arrives on this command's own recapture; a
## client-side copy would be a second statement of one value, which is the drift §4.9 forbids.
func _on_hud_work_priority(payload: Dictionary) -> void:
    _send_formatted_command(format_work_priority(payload))

## Say how this band splits a keeping pool it cannot stretch. Sent on its own — the fund mode is a
## standing policy on the band's allocation, not part of any source's commit.
func _on_hud_upkeep_mode(payload: Dictionary) -> void:
    _send_formatted_command(format_upkeep_mode(payload))

## Stage a recipe on the band's bench (Materials & Crafting). The player staffs the bench, so this
## sends the recipe alone and the crew stays where it was until the stepper moves it.
func _on_hud_set_bench(payload: Dictionary) -> void:
    _send_formatted_command(format_set_bench(payload))

## Re-crew the running bench, leaving the job and its progress alone.
func _on_hud_bench_crew(payload: Dictionary) -> void:
    _send_formatted_command(format_bench_crew(payload))

## Take the job off the bench. The pile already drawn is spent, which the button said before it was
## pressed.
func _on_hud_clear_bench(payload: Dictionary) -> void:
    _send_formatted_command(format_clear_bench(payload))

## Rank the bench against the band's other work. **No optimistic write, so nothing to roll back** —
## the mark is captured live off the bench and lands on this command's own recapture.
func _on_hud_bench_priority(payload: Dictionary) -> void:
    _send_formatted_command(format_bench_priority(payload))

## Recall an in-flight expedition home (folds workers + provisions back on arrival).
func _on_hud_recall_expedition(payload: Dictionary) -> void:
    _send_formatted_command(format_recall_expedition(payload))

## Split a resident band in two where it stands. The client FORECASTS whether a given worker count
## is viable — the compose sheet disables Send and says why, off the two floors published per-cohort
## — but the SIM's verdict is the authority: a refusal comes back on the `band_founded` event
## channel (`handle_split_band` reports through `CommandEventKind::BandFounded`), not as a reply.
func _on_hud_split_band(payload: Dictionary) -> void:
    _send_formatted_command(format_split_band(payload))

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

## `message` is the line the player may see on the event dock's System channel, and `ack_kind` says
## what it IS. The default — `KIND_COMMAND_ECHO` — is right for every command the player issued
## through the UI: "Advance 1 turn.", "Answered the question.", "Stop improving (12, 8)" restate an
## action just taken, so the dock ignores them (`HudEventVocab.IGNORED_KINDS`) and only the
## Inspector's debug console keeps them. A caller whose message reports a FAULT the client acted on
## by itself passes `HudEventVocab.KIND_SYSTEM` instead — see `_tick_resync`.
##
## **IT ANSWERS WHETHER THE LINE WENT, and that answer is load-bearing for any caller holding an
## OPTIMISTIC overlay.** It used to warn and return nothing, so a send the transport refused
## (`Inspector.gd`'s `err != OK` branch) left the HUD's pending write standing as fact until the next
## turn reconciled it away — a Builders card reading `3` beside a queue reading `⚠ No builders`. Every
## caller that wrote something on the strength of this call must roll that write back on `false`; the
## rest may ignore it exactly as before.
func _send_runtime_command(line: String, message: String,
        ack_kind: String = HudEventVocab.KIND_COMMAND_ECHO) -> bool:
    if inspector != null and inspector.has_method("send_runtime_command"):
        var result: Variant = inspector.call("send_runtime_command", line, message, ack_kind)
        if result is bool and result:
            return true
        push_warning("Command pending or rejected: %s" % line)
    else:
        push_warning("Inspector unavailable; cannot send command: %s" % line)
    return false

## ESC PRECEDENCE, as data. Which surface claims the key, innermost first:
##   (1) an open pause menu resumes;
##   (2) an open COMPOSE SHEET closes — it is the innermost working surface, so it claims ESC ahead
##       of targeting (docs/plan_tile_panel_layout.md §15);
##   (3) active targeting keeps ESC for MapView's targeting-cancel path (we must NOT consume it);
##   (4) the Band panel's WORK INSPECTOR dialog closes;
##   (5) otherwise the pause menu opens.
## Extracted as a pure static so the ORDER can be asserted without standing up the whole app scene
## (ui_preview drives it with the real HUD's own `is_compose_sheet_open` / `is_targeting_active`).
##
## **THE WORK INSPECTOR SITS FOURTH, AND EACH NEIGHBOUR IS A DECISION**
## (`docs/plan_standing_upkeep.md` §4.9 item 12d). It is behind the compose sheet and behind targeting
## because it is the OUTERMOST working surface of the three: the sheet is transient and modal and the
## targeting flow is a question the client has asked and is waiting on, while this dialog is
## PERSISTENT — the one that is still there afterwards, so it is the one that yields. It is ahead of
## the pause menu because a surface with an explicit dismiss must answer ESC before ESC means "leave
## the game"; a persistent card that ignored the key would be the only dismissible surface in the
## client that does.
const ESC_RESUME := "resume"
const ESC_COMPOSE_SHEET := "compose_sheet"
const ESC_TARGETING := "targeting"
const ESC_WORK_INSPECTOR := "work_inspector"
const ESC_PAUSE := "pause"

static func escape_claimant(pause_open: bool, compose_open: bool, targeting: bool,
        work_inspector_open: bool) -> String:
    if pause_open:
        return ESC_RESUME
    if compose_open:
        return ESC_COMPOSE_SHEET
    if targeting:
        return ESC_TARGETING
    if work_inspector_open:
        return ESC_WORK_INSPECTOR
    return ESC_PAUSE

func _unhandled_input(event: InputEvent) -> void:
    if event.is_action_pressed("ui_cancel"):
        var claimant := escape_claimant(
            pause_layer != null and pause_layer.visible,
            hud != null and hud.has_method("is_compose_sheet_open") and bool(hud.call("is_compose_sheet_open")),
            hud != null and hud.has_method("is_targeting_active") and bool(hud.call("is_targeting_active")),
            hud != null and hud.has_method("is_work_inspector_open") and bool(hud.call("is_work_inspector_open")))
        match claimant:
            ESC_RESUME:
                _hide_pause_menu()
                get_viewport().set_input_as_handled()
            ESC_COMPOSE_SHEET:
                hud.call("close_compose_sheet")
                get_viewport().set_input_as_handled()
            ESC_TARGETING:
                return
            ESC_WORK_INSPECTOR:
                hud.call("close_work_inspector")
                get_viewport().set_input_as_handled()
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
    if _inspector_visible() and workbench != null and workbench.is_panel_visible():
        workbench.set_panel_visible(false)

func _inspector_visible() -> bool:
    if inspector == null or not inspector.has_method("is_panel_visible"):
        return false
    return bool(inspector.call("is_panel_visible"))

## Stable stacking order for co-edge reservers: lower priority sits INBOARD (against the screen
## edge). The Inspector and the Workbench are screen-edge reservers; the Band panel stacks outboard
## of either. The two dev surfaces share a priority because only one of them is ever open — opening
## either closes the other (`_toggle_workbench_visibility`) — so what matters is that the Band panel
## offsets past whichever one is showing.
##
## **The event dock is deliberately absent — but that does not mean it is unmoved.** It reserves
## nothing from any of them (it OVERLAYS the map; see `EventDockPanel`'s header), so it takes no
## space, has no entry in `_reservations` and has no priority to be ranked at. What it DOES have is a
## displacement: it is always the innermost thing on its own edge, so `_update_event_dock_edge_offset`
## pushes it past every reserver already there — the band panel keeps the screen edge and the bar
## sits inboard of it. That is a one-way offset off this table, not a row in it: nothing ever stacks
## against the dock, because the dock occupies nothing. Its perpendicular bound is a third question
## again — `_update_event_dock_insets`, which reads the left/right reservers.
const RESERVER_PRIORITY := {&"inspector": 0, &"workbench": 0, &"band_panel": 1}
const BAND_PANEL_RESERVER := &"band_panel"
## The event dock's id in the HUD's OVERLAY registry — a different registry from `_reservations`
## above, and it must stay that way: this one records pixels covered, not space taken. Named as a
## const for the same reason `BAND_PANEL_RESERVER` is, so the push and any future release name the
## same key.
const EVENT_DOCK_OVERLAY := &"event_dock"

## Reserve space for a docked panel by insetting the game area (map + HUD) from
## the given edge, so the panel shrinks the play space instead of overlapping it.
## Fans a reserver's (edge, size) out to both surfaces. `edge` is a Godot Side
## const (SIDE_LEFT/SIDE_TOP/SIDE_RIGHT/SIDE_BOTTOM); `size <= 0` releases it.
func _apply_reservation(id: StringName, edge: int, size: float) -> void:
    if size <= 0.0:
        _reservations.erase(id)
    else:
        _reservations[id] = {"edge": edge, "size": size}
    # THE MAP is exempted for a HORIZONTAL band dock (issue #377): that dock is a floating CARD over
    # live map now, not a full-bleed bar, so insetting the map would blank the whole strip and leave
    # dead space either side of the card — the very thing the islands removed. THE HUD is exempted on the
    # TOP edge always and on the BOTTOM edge only when the card can pay for it; see
    # `_reserver_overlays_hud` for why the two horizontal edges differ and what the bottom edge buys. The
    # band panel keeps its `_reservations` entry either way, which is what still displaces the event dock
    # past it.
    var map_size: float = 0.0 if _reserver_overlays_map(id, edge) else size
    if map_view != null and map_view.has_method("set_reserved_inset"):
        map_view.call("set_reserved_inset", id, edge, map_size)
    push_hud_strip(hud, id, edge, size, _reserver_overlays_hud(id, edge, size))
    # A card sharing its strip with the HUD must be told what to keep clear of.
    _update_band_panel_lateral_bounds()
    # …and the HUD column that shares the strip's TRAILING corner with the parked chrome must be told
    # to stop above it. The mirror image of the line above: that one moves the CARD off the columns,
    # this one moves ONE column off the chrome.
    _update_right_column_bottom_clearance()
    # Co-edge stacking: push the Band panel's leading offset so it sits just past any inboard
    # reserver on its edge (e.g. the Inspector when both are left) instead of overlapping it.
    _update_band_panel_edge_offset()
    # The PERPENDICULAR axis: pull the horizontal event bar in from whatever is docked left/right.
    # Recomputed on EVERY reservation change, not just the dock's own — the band panel changing edge
    # or collapsing has to move the bar.
    _update_event_dock_insets()
    # The bar's OWN axis: push it past whatever reserves the edge it is DOCKED to, so a co-edge band
    # panel is not drawn over. Same "recompute on every change" reason as the line above.
    _update_event_dock_edge_offset()

## Does this reserver FLOAT over the map rather than push it aside? Only one does, and only on one pair
## of edges: the Band/City panel docked TOP or BOTTOM, which since issue #377 draws a content-width card
## centred in its strip with the HUD's chrome cluster beside it. Two islands over live map — so the map
## must keep rendering underneath, exactly as it does under the tile bar.
##
## A VERTICAL band dock is a full-height 380px strip the card still fills edge to edge, so it reserves
## from the map as it always did. Every other reserver is unaffected.
func _reserver_overlays_map(id: StringName, edge: int) -> bool:
    return id == BAND_PANEL_RESERVER and (edge == SIDE_TOP or edge == SIDE_BOTTOM)

## Does this reserver float over the HUD too? **A TOP band dock always; a BOTTOM one when the card can
## afford the columns it would then have to keep clear of.**
##
## The two horizontal edges are NOT symmetric, and that asymmetry is the whole content of this test.
## Insetting the HUD makes `LayoutRoot` yield the strip — and it yields it on ALL FOUR SIDES, since the
## inset shortens `LayoutRoot` itself, so a bottom reservation costs the HUD's left and right dock
## COLUMNS their full height across the entire window. That is right exactly when the HUD has something
## in that strip the card would otherwise be drawn over:
##
## - **TOP** — the HUD's top-right column (turn, faction totals, the Telling card) lives there, and the
##   card is a CENTRED island with open strip either side of it. Yielding pushed that whole column DOWN
##   below the strip, stranding it in the middle of the map while the space it belongs in sat empty
##   beside the card. It belongs BESIDE the card, which is what it gets when the HUD does not yield.
## - **BOTTOM** — the HUD's bottom bar lives there, so `DockRowController` relocates the minimap and turn
##   orb into the card's own rail and `BottomBar` leaves the row (invisible, zero minimum). The strip is
##   then empty of HUD furniture, and the inset is being charged for a bar that is no longer there: the
##   left column stops at the strip's top edge across the WHOLE window, clipping the tile card mid-content
##   in a region the band card never occupies.
##
## **BUT THE INSET WAS SILENTLY DOING A SECOND JOB, and a bare exemption loses it.** The card only clears
## the lateral columns because those columns are SHORT — with the bounds applied instead, a 1920 bottom
## dock's card span falls 1599 → 895, under `wide_shell_min_width()` (1190), and the dock drops out of the
## wide shell into the tabbed one. So the yield is traded, not removed: the HUD keeps its strip only where
## the card can pay the two bounds and STILL stand in the wide shell. Below that width the status quo is
## exactly preserved — at 1920 and under, this returns `false` on `SIDE_BOTTOM` as it always did.
##
## **THE PREDICATE READS AUTHORED WIDTHS, AND THAT IS WHAT MAKES IT ACYCLIC.**
## `HudLayer.lateral_column_widths()` is `max(authored, live)`, and the live term follows a column's
## rendered extent — whose HEIGHT is what this inset decides. Reading it here would make the answer depend
## on its own output. `left_column_width()` / `right_column_width()` are the scene's authored minimums
## (360 / 344), constants no layout can move; the rail width is DECLARED by the HUD's own chrome
## measurement; `_panel_width_extent()` is the viewport. A function of the window, two constants and a
## declared width has no path back to the inset. (`band_panel_preview`'s
## `_assert_bottom_yield_converges` is the measurement rather than the argument.)
##
## **The exemption is only half the fix** — see `_update_band_panel_lateral_bounds` for the other half,
## without which a busy band's card is simply drawn over the column instead of pushing it away.
##
## A vertical dock reserves a strip the card still fills edge to edge, so it yields as it always did.
func _reserver_overlays_hud(id: StringName, edge: int, size: float) -> bool:
    if id != BAND_PANEL_RESERVER:
        return false
    return band_dock_overlays_hud(edge, size, hud, band_city_panel)

## **THE ONE HOME OF THAT RULE.** `static` and node-free on purpose: the offline harnesses fan a
## reservation out to the HUD by hand (`tools/band_panel_preview.gd` has no `Main` to do it for them), and
## every one of them used to RESTATE the rule as `edge != SIDE_TOP`. A restatement is how a harness ends
## up green while testing the predicate this file used to have — so they call this instead, and moving the
## rule moves it for them too.
##
## Degrades to `false` — i.e. to insetting, the status quo — on a missing collaborator or a stale probe,
## which is the safe direction: the failure mode of a wrong `false` is the clipping this exists to fix,
## while a wrong `true` is a card drawn through the HUD's columns.
static func band_dock_overlays_hud(edge: int, size: float, hud_layer: Node, panel: Node) -> bool:
    if edge == SIDE_TOP:
        return true
    if edge != SIDE_BOTTOM or hud_layer == null or panel == null:
        return false
    if not hud_layer.has_method("bottom_chrome_parks_for") \
            or not hud_layer.has_method("bottom_chrome_rail_width") \
            or not hud_layer.has_method("left_column_ceiling") \
            or not hud_layer.has_method("right_column_ceiling") \
            or not panel.has_method("affords_wide_shell_with_bounds"):
        return false
    # (1) The bar has to have LEFT the strip. Until the chrome parks in the card's rail, `BottomBar` is
    #     still in the row and the card would be drawn straight over the minimap — which is the case for a
    #     collapsed panel (46px), a hidden one, and a window too short to hold the chrome stack.
    if not bool(hud_layer.call("bottom_chrome_parks_for", edge, size)):
        return false
    # (2) …and the card has to be able to pay for the columns it would then have to clear.
    #
    #     **THE CEILINGS, NOT THE RESERVATIONS** (`Hud.right_column_ceiling`). The card is placed
    #     against what the columns OCCUPY (`lateral_column_widths`, `max(authored, live)`), so a bound
    #     here that a live readout can exceed makes this answer "afford" for a card that then cannot —
    #     the HUD keeps its strip and the card collapses to the tabbed shell, i.e. the trade this rule
    #     exists to refuse, taken silently. Measured with the reservations: a 75px band of window
    #     widths (2215-2289) in which every bottom dock did exactly that. A ceiling is the only bound
    #     that is BOTH safe against live content and free of the cycle a live read would reopen.
    return bool(panel.call("affords_wide_shell_with_bounds",
        float(hud_layer.call("left_column_ceiling")),
        float(hud_layer.call("right_column_ceiling")),
        float(hud_layer.call("bottom_chrome_rail_width", edge, size))))

## **ONE RESERVER'S STRIP, PUBLISHED TO THE HUD ACROSS BOTH REGISTRIES — AND THEY ARE COMPLEMENTS.**
## `overlays` is `band_dock_overlays_hud`'s verdict; this is what that verdict MEANS to the HUD, and
## it is a `static` beside it for the same reason that one is: the offline harnesses fan a reservation
## out by hand, and a harness that publishes half of this is a harness that renders a card the live
## client bounds (and vice versa).
##
## - **`set_reserved_inset`** — space TAKEN. `LayoutRoot` shrinks, so the HUD's containers reflow
##   beside the strip and every docked card is drawn under it for free.
## - **`set_overlay_inset`** — pixels COVERED. Withheld from the first registry, the strip is still
##   there: the band card stands in it and the HUD's containers simply draw through it, which is the
##   whole content of the exemption. But a FREE-FLOATING card is not laid out by a container — it
##   places itself by arithmetic against `FloatingRoom` — so it is the one surface that is not drawn
##   underneath, and with neither registry naming the strip it is drawn straight THROUGH the panel.
##   Reported in play: the Materials & Crafting ledger sliced mid-row by a BOTTOM-docked Band/City
##   panel. This is the event bar's case reached from the opposite direction, so it takes the event
##   bar's registry.
##
## Exactly one of the two is ever charged, so a strip can never be counted twice, and the rule stays
## `band_dock_overlays_hud`'s alone. **The published depth is absolute from the screen edge, as the
## overlay registry requires**: only a HORIZONTAL band dock ever reaches the overlay branch, and the
## sole reservers that could displace one inboard (`RESERVER_PRIORITY`) are the two LEFT-edge dev
## surfaces, so a horizontal dock's `_edge_offset` is 0 whenever `size` is published here.
static func push_hud_strip(hud_layer: Node, id: StringName, edge: int, size: float,
        overlays: bool) -> void:
    if hud_layer == null:
        return
    if hud_layer.has_method("set_reserved_inset"):
        hud_layer.call("set_reserved_inset", id, edge, 0.0 if overlays else size)
    if hud_layer.has_method("set_overlay_inset"):
        hud_layer.call("set_overlay_inset", id, edge, size if overlays else 0.0)

## Tell the Band panel which HUD columns its card must keep clear of.
##
## Only an edge where the HUD does NOT yield needs them, so this asks `_reserver_overlays_hud` rather
## than testing the edge itself: where the HUD moves out of the strip the card has the whole row and the
## bounds are 0, and where it stays put the card has to be told that the leftmost `left_column_width()`
## and the rightmost `right_column_width()` are spoken for — otherwise a 34-source band's 1570px card
## lands straight through the readouts in a 1920px strip. A band with NO sources makes a narrow card with
## room to spare, which is exactly why the exemption alone LOOKS complete until you open a busy band.
##
## **The SIZE comes out of `_reservations`, not off the panel**, because that is the number the yield rule
## was evaluated against in `_apply_reservation` — re-deriving it here could answer the question with a
## size Main never published (`band-city-panel.md` → "A size the panel DRAWS but never PUBLISHES").
##
## **The widths are the columns' LIVE extents, not the HUD's authored minimums** — `lateral_column_widths`
## takes the greater of the two deliberately, and that is the OPPOSITE of the rule the event dock's own
## bound follows. The dock bounds an EDGE that must not jitter from turn to turn, so authored is right
## there and a column drawing wider merely overlaps it a little. This bound decides whether a CARD is
## drawn THROUGH the readouts: measured at 1920 they render 419px against a 344px authored minimum
## (the metrics line is longer than the minimum allows for), so an authored bound puts the card straight
## through them.
##
## **`band_dock_overlays_hud` MUST bound this from above, and it does so with a CEILING rather than by
## reading it.** That rule decides whether the HUD keeps its strip; the card is then placed against
## these live widths, so a rule whose bound this can exceed says "afford" for a card that cannot, and
## the shell collapses under a HUD that kept its columns. It cannot simply read this (the live term is
## what the rule's own output moves), so it reads `Hud.right_column_ceiling` — a constant no live
## readout can exceed — which makes it conservative rather than merely different.
##
## Live widths MOVE in ordinary play — the metrics line grows as its numbers gain digits, `L`/`V`/`R`
## toggle right-dock cards — which is why this is re-pushed per snapshot from `_apply_snapshot` as well
## as from `_apply_reservation`: the reservation alone changes only on dock/collapse/hide/resize, and a
## bound sampled only there goes stale while the card keeps being placed against it.
func _update_band_panel_lateral_bounds() -> void:
    if band_city_panel == null or not band_city_panel.has_method("set_lateral_bounds"):
        return
    if hud == null or not hud.has_method("lateral_column_widths"):
        return
    var edge: int = int(band_city_panel.call("get_dock")) if band_city_panel.has_method("get_dock") else SIDE_LEFT
    var reserved: Dictionary = _reservations.get(BAND_PANEL_RESERVER, {})
    var size: float = float(reserved.get("size", 0.0))
    if not _reserver_overlays_hud(BAND_PANEL_RESERVER, edge, size):
        band_city_panel.call("set_lateral_bounds", 0.0, 0.0)
        return
    var bounds: Vector2 = band_panel_lateral_bounds(edge, size, hud)
    band_city_panel.call("set_lateral_bounds", bounds.x, bounds.y)

## **WHAT THE CARD MUST KEEP CLEAR OF, AS `(leading, trailing)`** — the columns' live widths, with the
## LEADING term dropped wherever that column is not actually THERE at the strip's height.
##
## **A `static` beside `band_dock_overlays_hud` for the same reason that one is**: the offline harnesses
## fan a reservation out by hand, and a harness that restates this rule renders a card the live client
## bounds differently (and vice versa).
##
## ⛔ **THE LEADING TERM WAS THE COLUMN'S WIDTH CHARGED AT A HEIGHT THE COLUMN IS EMPTY AT, AND IT COST
## A FEATURE.** `BandCityPanel._rail_split` gates the bottom dock's chrome on
## `viewport − split_span − leading >= wide_shell_min_width()`; with the leading term at its authored
## **360** that needs **2012px**, so a 2000px monitor stacked the minimap and the turn orb at one end and
## the split *did not exist on the hardware it was built for*. What is at the leading end of a bottom
## dock's row is not the left column — it is whatever the left column PAINTS there, and measured on a
## 1920x1080 bottom dock its one card stops at **224** against a strip whose top edge is **662**. The
## region spans the row; the card does not. `Hud.left_column_content_reach` is the drawn-content read,
## and it is the same distinction `348e5c09` got the wrong side of on the trailing column.
##
## **IT IS LIVE, NOT A WORST CASE, AND IT HAS TO BE.** The left column keeps its full height on a bottom
## dock deliberately (`_update_right_column_bottom_clearance` — only the RIGHT column yields), so a tile
## card really may descend into the strip; where it does, the bound comes straight back and the row
## stacks. The threshold is 438px away from the shipped card's reach, so this is a subject-scale change
## and not a flicker.
##
## **ONLY THE BOTTOM EDGE ASKS IT.** On a TOP dock the strip is at the top of the window and the left
## column's content STARTS there, so the column is always in the card's band and the question has one
## answer; asking it there would zero a bound that is genuinely owed.
##
## **THE TRAILING TERM IS NOT TREATED THE SAME WAY HERE, and must not be** — the panel already drops it
## for a bottom dock in `BandCityPanel._trailing_bound_for`, because that column is FORCED clear of the
## strip by `set_right_column_bottom_clearance` rather than merely observed to be. One decision per
## column, each where its reason lives.
static func band_panel_lateral_bounds(edge: int, size: float, hud_layer: Node) -> Vector2:
    if hud_layer == null or not hud_layer.has_method("lateral_column_widths"):
        return Vector2.ZERO
    var columns: Vector2 = hud_layer.call("lateral_column_widths")
    if edge != SIDE_BOTTOM or not hud_layer.has_method("left_column_content_reach"):
        return columns
    var strip_top: float = hud_layer.get_viewport().get_visible_rect().size.y - size
    if float(hud_layer.call("left_column_content_reach")) <= strip_top:
        # Nothing is painted in the column at the strip's height, so there is nothing there to clear.
        return Vector2(0.0, columns.y)
    return columns

## Tell the HUD how far above the window's bottom edge its RIGHT column must stop.
##
## **The other half of the conditional inset, and the half `set_reserved_inset` structurally cannot
## do.** When the HUD keeps a BOTTOM band dock's strip, `DockRowController` has parked the minimap,
## the zoom rail and the turn orb into the card's chrome rail — which is pinned FLUSH to the screen's
## trailing edge, the corner the right dock's own cards occupy. Measured on a 2560×1080 canvas: the
## Telling card at its page cap, the Victory card and an 11-row Terrain Types legend put the right
## dock's content at y 170→1151 against a strip whose top edge is 720, so the legend card alone lands
## 334px inside the parked chrome. It is not a hypothetical future card; `L` and `V` reach it today.
##
## **Only the RIGHT column yields, and that asymmetry is the point.** The LEFT column has nothing in
## that strip — the band card is centred and holds clear of it — and its full height across the whole
## window IS the defect the conditional inset exists to fix, so a bottom inset on `LayoutRoot` (which
## shortens both columns, `Hud.set_reserved_inset`) would undo that fix to close this one.
##
## Same shape as `_update_band_panel_lateral_bounds`: the SIZE comes out of `_reservations`, because
## that is the number the yield rule was evaluated against, and the verdict is asked of
## `_reserver_overlays_hud` rather than restated — a clearance applied where the HUD has already
## yielded would charge the column twice for one strip.
func _update_right_column_bottom_clearance() -> void:
    if hud == null or not hud.has_method("set_right_column_bottom_clearance"):
        return
    var edge: int = SIDE_LEFT
    if band_city_panel != null and band_city_panel.has_method("get_dock"):
        edge = int(band_city_panel.call("get_dock"))
    var reserved: Dictionary = _reservations.get(BAND_PANEL_RESERVER, {})
    var size: float = float(reserved.get("size", 0.0))
    var keeps_the_strip: bool = edge == SIDE_BOTTOM \
        and _reserver_overlays_hud(BAND_PANEL_RESERVER, edge, size)
    hud.call("set_right_column_bottom_clearance", size if keeps_the_strip else 0.0)

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
## **This is not `RESERVER_PRIORITY`.** That orders reservers stacked ALONG one shared edge, and the
## dock is not in it at all — it reserves nothing. This is the cross axis, where there is no stacking
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

## The event dock's counterpart to `_update_band_panel_edge_offset`, on the bar's OWN axis: Σ sizes of
## every reserver currently on the SAME edge the bar is docked to (in practice the Band panel when
## both are top or both are bottom; 0 otherwise). The bar is then drawn just past them, so the panel
## keeps the screen edge and the strip sits BELOW it on a top dock / ABOVE it on a bottom one.
##
## **EVERY reserver on that edge counts — there is no priority test here, and that is deliberate.**
## `_update_band_panel_edge_offset` needs one because the Band panel is itself a reserver and has to
## know which co-edge reservers sit inboard of it. The dock is not a reserver at all: it occupies no
## strip, so nothing can ever stack against it and it is by construction the INNERMOST thing on its
## edge. Adding it to `RESERVER_PRIORITY` would be the reflex mistake — it would reintroduce the
## full-width reservation that shipped black bars either side of a centre-bounded strip.
##
## Fed from two places, and it needs both: `_apply_reservation` (a co-edge panel arriving, moving,
## collapsing or hiding) and `dock_changed` (the bar itself moving to the other edge, which changes
## which reservers it must clear).
func _update_event_dock_edge_offset() -> void:
    if event_dock == null or not event_dock.has_method("set_edge_offset") or not event_dock.has_method("get_dock"):
        return
    var dock_edge: int = int(event_dock.call("get_dock"))
    var offset: float = 0.0
    for other_id in _reservations:
        var r: Dictionary = _reservations[other_id]
        if int(r.get("edge", -1)) != dock_edge:
            continue
        offset += float(r.get("size", 0.0))
    event_dock.call("set_edge_offset", offset)

func _on_inspector_reserved_width_changed(width: float) -> void:
    _apply_reservation(&"inspector", SIDE_LEFT, width)

## THE WORKBENCH — the designer surface (`.claude/rules/client/workbench.md`), hosted on its own
## CanvasLayer above the Inspector's and hidden at startup exactly like it: a dev surface that opened
## itself on every launch would cost the player half the map to close.
##
## It is BUILT HERE rather than instanced from a scene because `WorkbenchShell` has no `.tscn` — the
## surface assembles its own chrome, which is what lets a page arrive without a scene edit.
func _connect_workbench() -> void:
    if workbench_layer == null or workbench != null:
        return
    workbench_layer.layer = WORKBENCH_LAYER
    workbench = WorkbenchShell.new()
    workbench.set_anchors_and_offsets_preset(Control.PRESET_LEFT_WIDE)
    workbench.offset_right = WorkbenchVocab.SURFACE_WIDTH
    workbench_layer.add_child(workbench)
    workbench.reserved_width_changed.connect(_on_workbench_reserved_width_changed)
    workbench.set_services(_workbench_services())
    workbench.set_command_connected(command_client != null)
    # Hidden at startup — and the reservation seeded from what a hidden surface reserves (0), so the
    # map starts with the whole viewport rather than a strip it can never reclaim.
    workbench.set_panel_visible(false)
    _apply_reservation(WORKBENCH_RESERVER, SIDE_LEFT, workbench.reserved_width())

## The capabilities the Workbench's pages are lent, by name (`WorkbenchVocab.SERVICE_*`).
##
## **A new page needing a new capability adds a row HERE and reads it by name there** — the shell
## carries this dictionary without reading a single entry, so nothing in between has to change. That
## is the property the whole services indirection exists for; passing these as positional arguments
## is what made the previous version need a shell edit per capability.
func _workbench_services() -> Dictionary:
    return {
        WorkbenchVocab.SERVICE_SEND_COMMAND: Callable(self, "_workbench_send_command"),
        WorkbenchVocab.SERVICE_APPEND_LOG: Callable(self, "_workbench_append_log"),
        WorkbenchVocab.SERVICE_NEW_GAME: Callable(self, "_workbench_new_game"),
    }

## Send one Workbench command down the SAME transport every other client command uses. Answers
## whether it went, so a page can tell "sent" from "no server" — `_send_runtime_command` only warns.
func _workbench_send_command(line: String) -> bool:
    if inspector == null or not inspector.has_method("send_runtime_command"):
        return false
    var verb := line.get_slice(" ", 0)
    var result: Variant = inspector.call("send_runtime_command", line,
        WORKBENCH_COMMAND_MESSAGE % verb)
    return result is bool and result

## The surface's status log goes to the EVENT DOCK's System channel (`R`) — the client's existing
## "what just happened" surface, which is where the retired command feed's audience went. It routes
## through `_note_system_event`, so a client without the dock simply drops the line.
##
## **`KIND_SYSTEM`, not the command echo.** An Apply's "3 overrides sent" is not a receipt for a line
## the player typed; it is the surface reporting a state change it made on their behalf, and
## `HudEventVocab.IGNORED_KINDS` would swallow an echo outright. The command RECEIPTS the Workbench
## generates are separate and do take the echo default — see `_workbench_send_command`.
##
## The Workbench's own Logs page is registered but unbuilt; when it is built, this is the one row
## that moves.
func _workbench_append_log(text: String) -> void:
    _note_system_event(WORKBENCH_LOG_LABEL, text, false)

## Re-issue the CURRENT world's `new_game` line — the same preset/size/seed/profile this session
## launched with, so an override applies to a world the designer can compare against the last one.
## The reveal gate is not disturbed: the rebuild arrives as a higher world epoch like any other.
func _workbench_new_game() -> void:
    if _new_game_command.is_empty():
        return
    _send_runtime_command(_new_game_command["line"], _new_game_command["message"])

func _on_workbench_reserved_width_changed(width: float) -> void:
    _apply_reservation(WORKBENCH_RESERVER, SIDE_LEFT, width)

## THE TWO DEV SURFACES ARE MUTUALLY EXCLUSIVE, and it is a correctness rule, not tidiness. Both
## reserve SIDE_LEFT, so an open pair insets the map and HUD by the SUM of their widths (380 + 560)
## while the Workbench — on the higher CanvasLayer — draws only its own 560, leaving a wide strip of
## bare background and an Inspector that is invisible yet still reserving. Opening one closes the
## other, which is also the behaviour the shared reserver priority already assumed.
func _toggle_workbench_visibility() -> void:
    if workbench == null:
        return
    workbench.set_panel_visible(not workbench.is_panel_visible())
    # The inset update arrives via each surface's reserved_width_changed signal.
    if workbench.is_panel_visible() and _inspector_visible():
        inspector.call("set_panel_visible", false)

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

## Wire the event dock's CONTENT inlets — the HUD's own System notes, the HUD's band roster, the
## Inspector's console chatter — and seed the one geometry it does take, its perpendicular insets.
##
## **THERE IS NO RESERVATION TO FAN OUT.** Unlike the Band/City panel, the dock overlays the map and
## reserves nothing: it has no entry in `_reservations`, no row in `RESERVER_PRIORITY` (whose `0` is
## the Inspector's), and publishes neither `reservation_changed` nor `current_reservation_size()`, so
## nothing here touches `_apply_reservation`. What it does publish is `dock_changed` — which is the
## opposite direction and must not be mistaken for a reservation: it says where the bar WENT, so this
## side can re-measure what displaces it there. Both bounds are read FROM the reservers, never
## contributed to them — `_update_event_dock_insets` on the perpendicular axis,
## `_update_event_dock_edge_offset` on the bar's own.
func _connect_event_dock() -> void:
    if event_dock == null:
        return
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
    # A dock chip moves the bar to the other horizontal edge, which changes WHICH reservers it has to
    # clear — so the offset has to be re-measured there. Nothing in `_apply_reservation` can see this:
    # no reservation changed, only the bar's own edge.
    if event_dock.has_signal("dock_changed") and not event_dock.is_connected(
            "dock_changed", Callable(self, "_on_event_dock_dock_changed")):
        event_dock.connect("dock_changed", Callable(self, "_on_event_dock_dock_changed"))
    # …and the OTHER direction on the same axis: how deep the bar is DRAWN, which the HUD hands to
    # its free-floating cards as a bound. Still not a reservation — nothing here calls
    # `_apply_reservation` and the HUD's own layout does not move — see `_on_event_dock_occupancy_changed`.
    if event_dock.has_signal("occupancy_changed") and not event_dock.is_connected(
            "occupancy_changed", Callable(self, "_on_event_dock_occupancy_changed")):
        event_dock.connect("occupancy_changed", Callable(self, "_on_event_dock_occupancy_changed"))
    # The dock's one PER-ROW signal: the `Work tab` link on a cut, dropped or narrowed labor row.
    if event_dock.has_signal("band_work_tab_requested") and not event_dock.is_connected(
            "band_work_tab_requested", Callable(self, "_on_event_dock_band_work_tab_requested")):
        event_dock.connect("band_work_tab_requested",
            Callable(self, "_on_event_dock_band_work_tab_requested"))
    # Seed BOTH bounds: nothing else will, since the dock never enters `_apply_reservation`'s fan-out.
    # Wiring runs after `_connect_band_city_panel`, so the reservers are already in `_reservations`.
    _update_event_dock_insets()
    _update_event_dock_edge_offset()
    # …and the occupancy, for the same reason ONE step earlier: the dock emits it from its own
    # `_ready`, which runs BEFORE this parent's, so the first emission is gone by the time the
    # connect above happens. Seeded from the panel rather than re-derived here — the depth is its
    # geometry, not ours.
    _push_event_dock_occupancy()

## The bar changed edge; re-measure what displaces it on the new one. The edge is carried on the
## signal for legibility, but `_update_event_dock_edge_offset` re-reads it from the panel — one
## reader of `get_dock()`, so the offset can never be computed against a stale edge.
func _on_event_dock_dock_changed(_edge: int) -> void:
    _update_event_dock_edge_offset()

## **THE BAR COVERS PIXELS; IT STILL TAKES NO SPACE.** The HUD's free-floating cards place
## themselves by arithmetic against a rect rather than by a container, so they are the one kind of
## surface that cannot simply be drawn under the bar — hence `Hud.set_overlay_inset`, which shrinks
## THAT rect and nothing else. It is deliberately not `_apply_reservation`: routing the bar through
## the reservation fan-out would inset the map and the whole HUD layout, which is precisely the
## decision `event-dock.md` records as made the other way.
func _on_event_dock_occupancy_changed(edge: int, extent: float) -> void:
    if hud == null or not hud.has_method("set_overlay_inset"):
        return
    hud.call("set_overlay_inset", EVENT_DOCK_OVERLAY, edge, extent)

func _push_event_dock_occupancy() -> void:
    if event_dock == null or not event_dock.has_method("occupied_extent") \
            or not event_dock.has_method("get_dock"):
        return
    _on_event_dock_occupancy_changed(
        int(event_dock.call("get_dock")), float(event_dock.call("occupied_extent")))

func _on_band_labels_changed(labels: Dictionary) -> void:
    _event_dock_invoke("set_band_labels", [labels])

## The dock's `Work tab` link, relayed to the HUD. **The band arrives as the sim's durable `band_id`**
## — the dock's only handle on a band is an event's `band=` detail token — and the roster join onto
## the client-local entity happens in `HudLayer.show_band_work_tab`, which is where the roster is.
func _on_event_dock_band_work_tab_requested(band_id: int) -> void:
    _hud_invoke("show_band_work_tab", [band_id])

## The HUD's own client-side notes — a quick-hunt refusal, a knowledge unlock, an unanswered fork.
## Every one of them is a fault or a state change the player is owed, so this path states no kind and
## takes the `system` default; the HUD has no acknowledgement path to distinguish.
func _on_system_note_requested(label: String, detail: String) -> void:
    _note_system_event(label, detail, false)

func _on_inspector_system_event(label: String, detail: String, alert: bool, kind: String) -> void:
    _note_system_event(label, detail, alert, kind)

func _note_system_event(label: String, detail: String, alert: bool,
        kind: String = HudEventVocab.KIND_SYSTEM) -> void:
    if event_dock != null and event_dock.has_method("note_system"):
        event_dock.call("note_system", label, detail, alert, kind)

## `R` toggles the event dock — the hotkey the retired left-dock command feed used to own, and the
## persisted `command_feed_suppressed` preference migrates onto the dock's own `suppressed` key.
func _toggle_event_dock_visibility() -> void:
    if event_dock != null and event_dock.has_method("toggle_suppressed"):
        event_dock.call("toggle_suppressed")

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

## Put one composed forecast question on the command socket. Injected into the HUD's `ForecastQuery`
## as its sender; `true` means the frame reached the socket, never that it was answered.
func _send_query(request_id: int, ask: Dictionary) -> bool:
    if command_client == null:
        return false
    return command_client.send_query(request_id, ask)

## Drain the forecast answers that landed this frame into the HUD's seam, and let it retire any
## superseded answer whose stale window has closed. **This is the only path an answer takes** — a
## query deliberately triggers no re-capture server-side, so no snapshot will ever carry one.
## **DRAINED ONCE, DELIVERED TO BOTH SEAMS.** `poll_query_replies` is destructive — it empties the
## native queue — so two drains would race, each swallowing answers meant for the other. The two
## seams tell their own replies apart by `request_id`, and their id spaces are disjoint by
## construction (`SaveSlots.REQUEST_ID_BASE`), so handing each the whole batch is correct.
func _pump_forecast_queries() -> void:
    if command_client == null:
        return
    var replies: Array = command_client.poll_query_replies()
    if save_slots != null:
        save_slots.deliver(replies)
    if hud == null or not hud.has_method("forecast_query"):
        return
    var query: ForecastQuery = hud.call("forecast_query")
    query.deliver(replies)
    query.expire_stale()


## The save channel's answers that `Main` itself cares about — which is only the LOAD's.
##
## A save or a delete is the pause pane's business and is reported there. A load's `config_drift` is
## nobody else's: the menu shell is about to be gone, and this is the only place that will still be
## standing when the loaded world appears.
func _on_save_op_finished(kind: String, slot: String, ok: bool, error: String, drift: Array) -> void:
    if kind != SaveSlots.KIND_LOAD:
        return
    if not ok:
        # The world we asked for is not coming. Say so where the player is looking — the loading
        # overlay — rather than leaving them on "Generating world…" forever.
        push_warning("load_game(%s) refused: %s" % [slot, error])
        _set_loading_overlay_text(SaveSlots.error_prose(error))
        # **ONLY A TRANSPORT FAILURE IS RE-ASKED** — the same split `ForecastQuery` makes. A token the
        # server spelled (`no_such_slot`, `unreadable`) is a statement about THIS slot and re-asking
        # cannot change it, so the retry latch stays set and the reason stands on the overlay. A dead
        # socket says nothing about the ask and heals on its own, so that one is chased.
        if error == SaveSlots.ERROR_TRANSPORT:
            _new_game_sent = false
        return
    _pending_config_drift = drift

func _process(delta: float) -> void:
    _pump_forecast_queries()
    # **THE FIVE TOGGLE HOTKEYS ARE POLLED, so a focused text field does not starve them.**
    # `Input.is_action_just_pressed` samples raw device state and never enters the event system: the
    # `r` in a save's name toggled the event dock behind the menu, and `i`/`v`/`f`/`` ` `` did the
    # same for their panels. This is the SECOND of the client's two polled keyboard reads — the other
    # is `MapView`'s pan/zoom — and both ask the one predicate.
    #
    # **THE GUARD IS THIS BLOCK AND NOTHING ELSE.** Every line below it must keep running while the
    # player types: the query pump is what carries the answer to the save they are naming, and the
    # snapshot drain, the connection poll and the world-request retry all have nothing to do with the
    # keyboard. A guard around the whole of `_process` would stall the very socket the save needs.
    if not TextEntryFocus.held_in(get_viewport()):
        if Input.is_action_just_pressed("toggle_inspector"):
            _toggle_inspector_visibility()
        if Input.is_action_just_pressed("toggle_victory"):
            _toggle_victory_visibility()
        if Input.is_action_just_pressed("toggle_event_dock"):
            _toggle_event_dock_visibility()
        if Input.is_action_just_pressed("toggle_fow"):
            _toggle_fow_overlay()
        if Input.is_action_just_pressed(WORKBENCH_TOGGLE_ACTION):
            _toggle_workbench_visibility()
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
##
## **BOTH SENDS STATE `KIND_SYSTEM` RATHER THAN TAKING THE ECHO DEFAULT.** A resync is not a receipt
## for anything the player did — the client sent it because a frame could not be applied — so it is a
## fault report, and the System channel is where a fault report belongs.
func _tick_resync(delta: float) -> void:
    if snapshot_loader == null:
        return
    if snapshot_loader.resync_needed:
        snapshot_loader.resync_needed = false
        if _resync_pending_accum < 0.0:
            _send_runtime_command("resync", "resync requested (unapplicable delta)",
                HudEventVocab.KIND_SYSTEM)
            _resync_pending_accum = 0.0
        return
    if _resync_pending_accum < 0.0:
        return
    _resync_pending_accum += delta
    if _resync_pending_accum >= RESYNC_ANSWER_TIMEOUT:
        _resync_pending_accum = 0.0
        _send_runtime_command("resync", "resync retry (still no baseline)",
            HudEventVocab.KIND_SYSTEM)

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
    _show_config_drift_notice()

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
## existed (the same staleness the `has_method` probes elsewhere tolerate). It guesses from the
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

## Everything one snapshot frame owes the event dock, in the ONE order that is correct:
## **RESET → CURRENT TURN → RETENTION → INGEST**. Each arrow is load-bearing.
##
## - **`reset()` first, and on a FULL frame only.** It is a correctness requirement rather than
##   hygiene: `CommandEventLog` is checkpoint state, so a ROLLBACK restores it including its
##   `next_seq` counter and the replayed events reuse sequence numbers the client has already seen.
##   A rollback publishes a full frame, so without the clear the dock suppresses every replayed row
##   as a duplicate `seq` and goes on showing a plausible but stale log, silently. It must still be
##   the FIRST step, because the same frame carries the backfill and clearing after the dispatch
##   would wipe what just landed.
## - **The current turn AFTER the reset.** `reset()` sets `_current_turn = -1` and `set_current_turn`
##   only ever RAISES it, so stamping the turn ahead of the clear is simply erased — after which the
##   dock's "current turn" is whatever the newest INGESTED event's tick happened to be (or `-1` on an
##   empty ring, where `_prune()` then no-ops entirely). A client-side `note_system` posted before the
##   next frame would then be stamped and grouped under a turn that is not the one it happened on.
## - **Retention BEFORE the ingest**: the sim's window is hot-reloadable, so a frame can both narrow
##   it and carry the events that must be trimmed against the NEW value.
## - **The ingest last**, since it is what the other three configure.
##
## `static`, taking the dock, so `tools/ui_preview.gd` can drive this exact sequence against a real
## `EventDockPanel` with no `Main` in the tree — the same named-seam pattern
## `tools/snapshot_alias_guard.gd` uses on `MapView`'s ingests. An assertion that re-typed the order
## in the harness would pass on any order this function chose.
static func apply_event_dock_frame(dock: Node, snapshot: Dictionary, is_delta: bool) -> void:
    if dock == null:
        return
    if not is_delta:
        _dock_invoke(dock, "reset")
    # The turn a client-side System event is stamped with, so it groups with the sim events of the
    # turn it actually happened on.
    _dock_invoke(dock, "set_current_turn", [int(snapshot.get("turn", 0))])
    if snapshot.has("command_events_retention_turns"):
        _dock_invoke(dock, "set_retention_turns", [int(snapshot["command_events_retention_turns"])])
    if snapshot.has("command_events") and SnapshotSections.changed(snapshot, "command_events"):
        _dock_invoke(dock, "ingest_events", [snapshot["command_events"]])

## The event dock's twin of `_hud_invoke` — a silent `has_method` probe, so a client running
## without the panel (a harness, a partial scene) simply does nothing rather than erroring.
func _event_dock_invoke(method: String, args: Array = []) -> void:
    _dock_invoke(event_dock, method, args)

static func _dock_invoke(dock: Node, method: String, args: Array = []) -> void:
    if dock != null and dock.has_method(method):
        dock.callv(method, args)

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
