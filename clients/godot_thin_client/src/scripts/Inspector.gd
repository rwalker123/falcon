extends CanvasLayer
class_name InspectorLayer

## Emitted whenever the width this panel reserves on the left edge changes —
## on show/hide and on live resize. The game area (map + HUD) insets by this
## amount so the Inspector never overlaps other panels.
signal reserved_width_changed(width: float)

## One line of the client's own console chatter, re-broadcast for the EVENT DOCK's System channel
## (`Main._on_inspector_system_event` -> `EventDockPanel.note_system`). The Inspector's log widget
## keeps every one of these — it is the debug console and it stays one — but a dropped command
## socket or a refused command is something the PLAYER must be told, and that console is not where
## they will see it. `alert` is stated by the emitting site rather than derived from the text: this
## file knows which of its own lines is bad news, and a string match on its own log strings would
## only pretend to.
##
## `kind` is the dock's own vocabulary (`HudEventVocab`): `KIND_SYSTEM` for a fault or a state
## change, `KIND_COMMAND_ECHO` for the receipt of a command this client just accepted for sending.
## The dock ignores the latter — it restates an action the player took a moment ago through the UI —
## while this file's log widget goes on printing every one of them, because it is the debug console.
signal system_event(label: String, detail: String, alert: bool, kind: String)

const ScriptManagerPanel := preload("res://src/scripts/scripting/ScriptManagerPanel.gd")
const ScriptHostManager := preload("res://src/scripts/scripting/ScriptHostManager.gd")
# TerrainDefinitions moved to TerrainPanel.

const Typography = preload("res://src/scripts/Typography.gd")

# MAP_SIZE_* constants moved to MapPanel.

# MOUNTAIN_KIND_LABELS / FOOD_MODULE_LABELS moved to TerrainPanel.
# (bit 1 / CAP_CONSTRUCTION was dropped with the retired camp-founding command —
# nothing client-side gates on it now.)
const CAP_INDUSTRY_T1 := 1 << 2
const CAP_INDUSTRY_T2 := 1 << 3
const CAP_POWER := 1 << 4
const CAP_NAVAL_OPS := 1 << 5
const CAP_AIR_OPS := 1 << 6
const CAP_ESPIONAGE_T2 := 1 << 7
const CAP_MEGAPROJECTS := 1 << 8

var capability_flags: int = 0

@onready var sentiment_panel: SentimentInspectorPanel = $RootPanel/TabContainer/Sentiment
@onready var terrain_panel: TerrainInspectorPanel = $RootPanel/TabContainer/Terrain
@onready var map_panel: MapInspectorPanel = $RootPanel/TabContainer/Map
@onready var overlay_panel: OverlayInspectorPanel = $RootPanel/TabContainer/Map/MapVBox/OverlaySection
@onready var culture_panel: CultureInspectorPanel = $RootPanel/TabContainer/Culture
@onready var victory_panel: VictoryInspectorPanel = $RootPanel/TabContainer/Victory
@onready var influencer_panel: InfluencerInspectorPanel = $RootPanel/TabContainer/Influencers
@onready var corruption_panel: CorruptionInspectorPanel = $RootPanel/TabContainer/Corruption
@onready var power_panel: PowerInspectorPanel = $RootPanel/TabContainer/Power
@onready var crisis_panel: CrisisInspectorPanel = $RootPanel/TabContainer/Crisis
## Extracted tab panels that implement the coordinator contract (apply_update/reset).
## Populated in _ready once the @onready handles resolve.
var _tab_panels: Array = []
@onready var knowledge_panel: KnowledgeInspectorPanel = $RootPanel/TabContainer/Knowledge
@onready var great_discoveries_panel: GreatDiscoveriesInspectorPanel = $RootPanel/TabContainer/GreatDiscoveries
@onready var logs_panel: LogsInspectorPanel = $RootPanel/TabContainer/Logs
@onready var root_panel: Panel = $RootPanel
@onready var tab_container: TabContainer = $RootPanel/TabContainer
@onready var fauna_panel: FaunaInspectorPanel = $RootPanel/TabContainer/Fauna
@onready var rollback_ten_button: Button = $RootPanel/CommandToolbar/RollbackTenButton
@onready var rollback_button: Button = $RootPanel/CommandToolbar/RollbackButton
@onready var play_pause_button: Button = $RootPanel/CommandToolbar/PlayPauseButton
@onready var step_one_button: Button = $RootPanel/CommandToolbar/StepOneButton
@onready var step_ten_button: Button = $RootPanel/CommandToolbar/StepTenButton
@onready var scripts_panel: ScriptManagerPanel = $RootPanel/TabContainer/Scripts

## Snapshot-carried axis bias, held here because SentimentPanel renders it (`set_axis_bias`) and
## the coordinator is what sees the snapshot key.
var _axis_bias: Dictionary = {}
# Terrain tile/biome/food state moved to TerrainPanel.
# Culture layer/tension state moved to CulturePanel.
var _map_view: Node = null
# Map-size + scenario state moved to MapPanel.
var _panel_visible: bool = true
## Newest frame seen of EITHER kind, and whether it arrived while the panel was hidden (so the
## panels have not consumed it yet). Together they are the catch-up path that makes the hidden-panel
## skip in `_apply_update` safe — see its header.
var _cached_snapshot: Dictionary = {}
var _hidden_snapshot_pending: bool = false
var _resolved_font_size: int = Typography.DEFAULT_FONT_SIZE
var _last_turn: int = 0
var command_client: Object = null
var command_connected: bool = false
var stream_active: bool = false
var autoplay_timer: Timer
var _hud_layer: Object = null
# TERRAIN_* histogram/limit constants moved to TerrainPanel.
const PANEL_WIDTH_DEFAULT = 340.0
const PANEL_WIDTH_MIN = 260.0
const PANEL_MIN_TOP_OFFSET = 48.0
const PANEL_MARGIN = 16.0
const PANEL_HANDLE_WIDTH = 12.0
const PANEL_TAB_PADDING = 16.0
## Seconds between auto-played turns. Autoplay is a DEV loop driven by the toolbar Play/Pause
## button; this is the interval the retired Commands tab's spin box shipped with, now the single
## rate because nothing in the UI sets one any more.
const AUTOPLAY_INTERVAL_SECONDS := 0.5
# CULTURE_* constants moved to CulturePanel.
var _viewport: Viewport = null
var _panel_width: float = PANEL_WIDTH_DEFAULT
var _is_resizing = false
var _script_host: ScriptHostManager = null
# Overlay channel state moved to OverlayPanel.

func _ready() -> void:
	Typography.initialize()
	_resolved_font_size = Typography.base_font_size()
	set_process(true)
	_viewport = get_viewport()
	if _viewport != null:
		_viewport.size_changed.connect(_on_viewport_resized)
	if root_panel != null:
		root_panel.gui_input.connect(_on_root_panel_gui_input)
		root_panel.focus_mode = Control.FOCUS_CLICK
	# The map-size/scenario/rivers controls are owned by MapPanel.
	_apply_capability_gating()
	apply_typography()
	_tab_panels = [power_panel, crisis_panel, knowledge_panel, sentiment_panel, victory_panel, fauna_panel, great_discoveries_panel, logs_panel, influencer_panel, corruption_panel, map_panel, culture_panel, terrain_panel]
	if map_panel != null:
		map_panel.set_command_hooks(Callable(self, "_send_command"), Callable(self, "_append_command_log"))
	if culture_panel != null:
		culture_panel.set_log_hook(Callable(self, "_append_log_entry"))
	# Terrain owns tile selection + its Export Map button; export_map sends directly via the
	# hook. (The tile scout button was retired with the single-task `scout` command.)
	if terrain_panel != null:
		terrain_panel.set_command_hooks(Callable(self, "_send_command"), Callable(self, "_append_command_log"))
	if logs_panel != null:
		logs_panel.log_entry_received.connect(_on_log_stream_entry)
	if crisis_panel != null:
		crisis_panel.set_command_hooks(Callable(self, "_send_command"), Callable(self, "_append_command_log"))
	if knowledge_panel != null:
		knowledge_panel.set_command_hooks(Callable(self, "_send_command"), Callable(self, "_append_command_log"))
	if victory_panel != null:
		victory_panel.set_log_hook(Callable(self, "_append_command_log"))
	# Fauna is now display-only telemetry (herd list + detail). The follow-herd command it
	# used to emit was retired with the single-task fauna commands (Early-Game Labor slice 3a);
	# hunting is now labor allocation via the HUD.
	_update_panel_layout()
	_render_static_sections()
	_setup_command_controls()

func is_panel_visible() -> bool:
	return _panel_visible

func set_panel_visible(visible: bool) -> void:
	var was_visible: bool = _panel_visible
	_panel_visible = visible
	if root_panel != null:
		root_panel.visible = visible
	set_process(visible)
	set_process_input(visible)
	reserved_width_changed.emit(reserved_width())
	# Catch up on whatever arrived while hidden BEFORE the player can look at the panel, so being
	# shown never reveals stale numbers (see `_apply_update`'s header).
	if visible and not was_visible:
		_catch_up_hidden_snapshot()

## Replay the newest cached full snapshot if the panels have not ingested it — i.e. discharge
## `_hidden_snapshot_pending`. Called on hide→show, and by `_render_static_sections` when a reset
## empties the panels while they are already visible.
##
## Safe to re-run against a snapshot already partly consumed: every panel rebuilds its state from
## the snapshot's keys. **There is no accumulator on this path any more** — `command_events` used to
## be one, appending to a running log nothing later could reconstruct; it belongs to the event dock
## now (`Main` feeds it directly), so a replay here cannot double-log anything.
func _catch_up_hidden_snapshot() -> void:
	if not _hidden_snapshot_pending or _cached_snapshot.is_empty():
		return
	_hidden_snapshot_pending = false
	_apply_update(_cached_snapshot, true)
	_render_dynamic_sections()

func toggle_panel_visibility() -> void:
	set_panel_visible(not _panel_visible)

## Width the docked panel occupies on the left edge (0 when hidden). The game
## area insets by this so the Inspector reserves space instead of overlapping.
func reserved_width() -> float:
	if not _panel_visible:
		return 0.0
	return _panel_width + PANEL_MARGIN * 2.0

func update_snapshot(snapshot: Dictionary) -> void:
	_apply_update(snapshot, true)
	_render_dynamic_sections()
	if snapshot.has("capability_flags"):
		update_capability_flags(int(snapshot["capability_flags"]))

func update_delta(delta: Dictionary) -> void:
	_apply_update(delta, false)
	_render_dynamic_sections()
	if delta.has("capability_flags"):
		update_capability_flags(int(delta["capability_flags"]))

## Route one snapshot/delta into the coordinator's own state and then out to the tab panels.
##
## # The hidden-panel skip
##
## The Inspector ships HIDDEN (`Main` calls `set_panel_visible(false)` at startup; `I` re-opens it)
## and the whole fan-out below — every panel's `apply_update` plus `_render_dynamic_sections` — was
## measured at **113 ms per turn** on an 80×52 map, 61 % of the client's per-turn apply cost, spent
## rendering a panel nobody can see. So while hidden this method stops after the prefix and the
## snapshot is stashed for `_catch_up_hidden_snapshot` to replay on show.
##
## **EVERY frame may now be skipped, and that is a reversal worth understanding.** The rule used to
## be "only a FULL snapshot may be skipped", because a delta described a *change* against state the
## panels already held and nothing later could reconstruct a dropped one. Delta streaming (#386)
## inverted the premise: the native decoder maintains a cached world and republishes it **whole** on
## every frame, so a merged delta frame is byte-equivalent to a full snapshot of the same state and
## the NEXT frame reconstructs anything dropped. Self-containment, not payload kind, is what the
## skip ever depended on — and now both kinds have it.
##
## That is why `_cached_snapshot` holds the newest frame of either kind. Replaying it through the
## `full_snapshot` path is correct because the base keys it carries (`tiles`, `populations`,
## `culture_layers`, …) are complete: the decoder patches them from each delta's `*_updates` rather
## than leaving them at the baseline, which it did not always do (`docs/plan_delta_streaming.md`
## §8.2). **If that ever regresses, this skip silently serves a stale panel** — the two are one
## contract, so do not weaken one without the other.
##
## Measured: the hidden fan-out was 16–30 ms per turn once deltas became the steady-state carrier,
## ~60 % of the client's `apply`. The old gate skipped only full snapshots, which by then arrived
## once per world.
##
## **The prefix below runs hidden or not**, and the dividing line is not "cheap" but
## *reconstructible*: anything consuming per-turn history no later snapshot carries must run whatever
## else is skipped, while everything after the gate rebuilds panel state from snapshot keys and is
## therefore recoverable. The prefix's one ACCUMULATOR — `command_events` — moved out to the event
## dock, so what is left above the gate is the cheap per-frame scalars. **Anything added to this
## method must be classified the same way before it is placed**, and an accumulator added back here
## has to sit above the gate.
func _apply_update(data: Dictionary, full_snapshot: bool) -> void:
	# Held by REFERENCE, not deep-copied: the native decoder builds a fresh Dictionary tree per
	# frame and no consumer mutates it in place (MapView duplicates the sub-dicts it keeps), so
	# a copy would cost exactly the work this gate exists to avoid.
	_cached_snapshot = data
	if data.has("turn"):
		_last_turn = int(data.get("turn", _last_turn))
	if data.has("capability_flags"):
		capability_flags = int(data["capability_flags"])

	# campaign_profiles / campaign_label / faction_inventory / grid are consumed by
	# MapPanel via the _tab_panels fan-out at the end of this method.
	# `command_events` is NOT read here. It was the one accumulator on this path — the reason the
	# prefix above the visibility gate exists at all — and it belongs to the event dock now, which
	# `Main` feeds directly. The Commands tab is a debug console, never a notification surface.
	# food_modules + tiles/tile_updates/tile_removed are consumed by TerrainPanel via the
	# _tab_panels fan-out at the end of this method.

	# `_hidden_snapshot_pending` means "a frame has arrived that the panels have NOT ingested" —
	# set it when one is skipped, clear it when one is actually fanned out. Both kinds set it now,
	# because `_cached_snapshot` holds both and either one replays into a complete panel state.
	if not _panel_visible:
		_hidden_snapshot_pending = true
		return
	_hidden_snapshot_pending = false

	if data.has("axis_bias"):
		var axis_dict: Dictionary = data["axis_bias"]
		_axis_bias = axis_dict.duplicate(true)
		if sentiment_panel != null:
			sentiment_panel.set_axis_bias(_axis_bias)

	# Influencer roster + corruption ledger are owned by InfluencerPanel / CorruptionPanel
	# and ingested via the _tab_panels fan-out at the end of this method.

	if data.has("overlays"):
		_ingest_overlays(data["overlays"])

	# culture_layers / culture_layer_updates / culture_layer_removed / culture_tensions are
	# consumed by CulturePanel via the _tab_panels fan-out; it renders from
	# _render_dynamic_sections with the coordinator-supplied influencer resonance.

	# Fan the update out to extracted tab panels last, so any coordinator-side
	# routing above (e.g. overlays.crisis_annotations via _ingest_overlays) is
	# already applied and a panel's own keys (e.g. crisis_overlay) win on conflict.
	for panel in _tab_panels:
		if panel != null:
			panel.apply_update(data, full_snapshot)

func _render_dynamic_sections() -> void:
	# Second half of the hidden-panel skip (see `_apply_update`): this is the coordinator's own
	# render pass, and both `update_snapshot`/`update_delta` call it right after `_apply_update`.
	# Guarded HERE rather than at those two call sites so a future third caller cannot miss it —
	# `_catch_up_hidden_snapshot` runs it only once the panel is already visible.
	if not _panel_visible:
		return
	# TerrainPanel renders in its own apply_update (no external dependency).
	# CulturePanel renders here so the coordinator can supply the influencer-resonance
	# summary (pulled from InfluencerPanel — panels stay decoupled).
	if culture_panel != null:
		culture_panel.render(influencer_panel.aggregate_resonance() if influencer_panel != null else {})

func _render_static_sections() -> void:
	if power_panel != null:
		power_panel.reset()
	if fauna_panel != null:
		fauna_panel.reset()
	if sentiment_panel != null:
		sentiment_panel.reset()
	if knowledge_panel != null:
		knowledge_panel.reset()
	if crisis_panel != null:
		crisis_panel.reset()
	if victory_panel != null:
		victory_panel.reset()
	if great_discoveries_panel != null:
		great_discoveries_panel.reset()
	if logs_panel != null:
		logs_panel.reset()
	if terrain_panel != null:
		terrain_panel.reset()
	if culture_panel != null:
		culture_panel.reset()
	if overlay_panel != null:
		overlay_panel.reset()
	if map_panel != null:
		map_panel.reset()
	_panel_width = PANEL_WIDTH_DEFAULT
	_update_command_controls_enabled()
	# Every panel is now empty, so a cached full snapshot is once again un-ingested (the invariant
	# `_hidden_snapshot_pending` tracks) — mark it so, otherwise a reset while hidden would strand
	# the panels blank. Visibility does not enter into the flag: when the panel IS visible there is
	# no later show to trigger the replay, so discharge it right here instead of leaving the panels
	# blank until the next full snapshot arrives.
	_hidden_snapshot_pending = not _cached_snapshot.is_empty()
	if _panel_visible:
		_catch_up_hidden_snapshot()

func apply_typography() -> void:
	Typography.initialize()
	_resolved_font_size = Typography.base_font_size()
	if root_panel != null:
		Typography.apply_theme(root_panel)
		var panel_style = StyleBoxFlat.new()
		panel_style.bg_color = Color(0.09, 0.09, 0.12, 0.6)
		panel_style.border_color = Color(0.2, 0.22, 0.26, 0.6)
		panel_style.border_width_top = 1
		panel_style.border_width_bottom = 1
		panel_style.border_width_left = 1
		panel_style.border_width_right = 1
		panel_style.corner_radius_top_left = 6
		panel_style.corner_radius_top_right = 6
		panel_style.corner_radius_bottom_left = 6
		panel_style.corner_radius_bottom_right = 6
		root_panel.add_theme_stylebox_override("panel", panel_style)
	if tab_container != null:
		Typography.apply(tab_container, Typography.STYLE_CONTROL)
		var tab_style = StyleBoxFlat.new()
		tab_style.bg_color = Color(0.13, 0.13, 0.17, 0.6)
		tab_style.border_color = Color(0.22, 0.24, 0.28, 0.6)
		tab_style.border_width_top = 1
		tab_style.border_width_bottom = 0
		tab_style.border_width_left = 1
		tab_style.border_width_right = 1
		tab_style.corner_radius_top_left = 6
		tab_style.corner_radius_top_right = 6
		tab_style.corner_radius_bottom_left = 0
		tab_style.corner_radius_bottom_right = 0
		tab_container.add_theme_stylebox_override("panel", tab_style)
		tab_container.tab_alignment = 0

	# Terrain widgets are styled by TerrainPanel.apply_typography().
	var control_nodes: Array = [
		rollback_ten_button,
		rollback_button,
		play_pause_button,
		step_one_button,
		step_ten_button
	]
	_apply_typography_style(control_nodes, Typography.STYLE_CONTROL)

	if crisis_panel != null:
		crisis_panel.apply_typography()
	if knowledge_panel != null:
		knowledge_panel.apply_typography()
	if sentiment_panel != null:
		sentiment_panel.apply_typography()
	if great_discoveries_panel != null:
		great_discoveries_panel.apply_typography()
	if logs_panel != null:
		logs_panel.apply_typography()
	if influencer_panel != null:
		influencer_panel.apply_typography()
	if corruption_panel != null:
		corruption_panel.apply_typography()
	if overlay_panel != null:
		overlay_panel.apply_typography()
	if map_panel != null:
		map_panel.apply_typography()
	if culture_panel != null:
		culture_panel.apply_typography()
	if terrain_panel != null:
		terrain_panel.apply_typography()

	_update_panel_layout()

func _setup_command_controls() -> void:
	if rollback_ten_button != null:
		rollback_ten_button.pressed.connect(_on_rollback_ten_button_pressed)
	if rollback_button != null:
		rollback_button.pressed.connect(_on_rollback_button_pressed)
	if play_pause_button != null:
		play_pause_button.pressed.connect(_on_play_pause_button_pressed)
		play_pause_button.button_pressed = false
	if step_one_button != null:
		step_one_button.pressed.connect(_on_step_one_button_pressed)
	if step_ten_button != null:
		step_ten_button.pressed.connect(_on_step_ten_button_pressed)
	# Autoplay's only control is the toolbar Play/Pause button above; the timer that steps the
	# turns lives here with the rest of the command hub.
	autoplay_timer = Timer.new()
	autoplay_timer.one_shot = false
	autoplay_timer.wait_time = AUTOPLAY_INTERVAL_SECONDS
	add_child(autoplay_timer)
	autoplay_timer.timeout.connect(_on_autoplay_timeout)
	# Terrain-tab command buttons (export/scout/found) are owned by TerrainPanel.
	_update_command_status()
	_append_command_log("Command console ready.")

func attach_script_host(manager: ScriptHostManager) -> void:
	if _script_host != null:
		if _script_host.is_connected("script_log", Callable(self, "_on_script_log_from_package")):
			_script_host.disconnect("script_log", Callable(self, "_on_script_log_from_package"))
		if _script_host.is_connected("script_alert", Callable(self, "_on_script_alert_from_package")):
			_script_host.disconnect("script_alert", Callable(self, "_on_script_alert_from_package"))
		if _script_host.is_connected("script_event", Callable(self, "_on_script_event_from_package")):
			_script_host.disconnect("script_event", Callable(self, "_on_script_event_from_package"))
	_script_host = manager
	if scripts_panel != null:
		scripts_panel.set_manager(manager)
	if _script_host != null:
		_script_host.script_log.connect(_on_script_log_from_package)
		_script_host.script_alert.connect(_on_script_alert_from_package)
		_script_host.script_event.connect(_on_script_event_from_package)

func set_command_client(client: Object, connected: bool) -> void:
	command_client = client
	var was_connected: bool = command_connected
	command_connected = connected and command_client != null and command_client.has_method("is_connection_active") and command_client.call("is_connection_active")
	_update_command_status()
	if command_connected and not was_connected:
		var host_value: String = "?"
		if command_client.has_method("get"):
			var host_variant = command_client.call("get", "host")
			if typeof(host_variant) == TYPE_STRING:
				host_value = host_variant
		var port_value: int = 0
		if command_client.has_method("get"):
			var port_variant = command_client.call("get", "port")
			if typeof(port_variant) in [TYPE_INT, TYPE_FLOAT]:
				port_value = int(port_variant)
		_append_command_log("Connected to command endpoint %s:%d." % [host_value, port_value])
	elif not command_connected and was_connected:
		_append_command_log("Command endpoint disconnected.", true)
	elif not command_connected and not was_connected:
		if command_client != null:
			var host_unavailable: String = "?"
			if command_client.has_method("get"):
				var host_unavailable_variant = command_client.call("get", "host")
				if typeof(host_unavailable_variant) == TYPE_STRING:
					host_unavailable = host_unavailable_variant
			var port_unavailable: int = 0
			if command_client.has_method("get"):
				var port_unavailable_variant = command_client.call("get", "port")
				if typeof(port_unavailable_variant) in [TYPE_INT, TYPE_FLOAT]:
					port_unavailable = int(port_unavailable_variant)
			_append_command_log("Command endpoint unavailable (%s:%d)." % [host_unavailable, port_unavailable], true)
		else:
			_append_command_log("Command endpoint unavailable.", true)

func set_streaming_active(active: bool) -> void:
	if stream_active == active:
		return
	stream_active = active
	if stream_active:
		_append_command_log("Streaming snapshots active.")
	else:
		_append_command_log("Streaming unavailable.", true)
		if autoplay_timer != null and not autoplay_timer.is_stopped():
			_disable_autoplay(true)
	_update_command_status()

## Re-read the command socket's state into `command_connected` and re-gate every panel that
## enables its controls on it. It used to also format a human-readable status line, but the only
## reader of that line was the retired Commands tab's status label — the state itself has many
## readers, so the resolution stays and the prose goes. Transitions the PLAYER must hear are
## logged by `set_command_client` / `set_streaming_active`, which reach the event dock.
func _update_command_status() -> void:
	if command_client == null or not command_client.has_method("status"):
		command_connected = false
	else:
		var st_variant = command_client.call("status")
		var st: int = st_variant if typeof(st_variant) == TYPE_INT else StreamPeerTCP.STATUS_NONE
		command_connected = st == StreamPeerTCP.STATUS_CONNECTED
	_update_command_controls_enabled()

## One line of client-side console chatter. It goes to the Logs tab's buffer, AND out on
## `system_event` so the event dock can put it on the System channel — **which is now the only
## surface a player sees it on**, the Commands tab that mirrored every line having been retired.
##
## `alert` defaults to `false`, which is what keeps the five panel-injected `Callable`s (Map /
## Terrain / Crisis / Knowledge / Victory) working unchanged: a panel's own command receipt is a
## Routine note. The FAILURE sites in this file pass `true` explicitly.
##
## `kind` defaults to `KIND_SYSTEM` for the same reason: every existing site keeps meaning "the
## player should hear this", and only `_send_command`'s ACCEPTED-send line opts into
## `KIND_COMMAND_ECHO`, which the dock ignores. The Logs buffer records every line regardless of
## kind — it is the debug console now.
func _append_command_log(entry: String, alert: bool = false,
		kind: String = HudEventVocab.KIND_SYSTEM) -> void:
	_append_log_entry("[CMD] %s" % entry, "COMMAND", "inspector.command")
	# THE LINE IS THE LABEL, not a detail beside one. A dock row renders its label at full size on
	# the leading edge and its detail as small faint text on the TRAILING one — so passing a fixed
	# "Command" label and the message as detail strands the only words that matter at the far end
	# of a screen-wide bar. The channel chip already says where the line came from.
	system_event.emit(entry, "", alert, kind)

func _update_command_controls_enabled() -> void:
	var connected = command_connected
	if map_panel != null:
		map_panel.set_command_connected(connected)
	# Terrain's tile action buttons are gated inside TerrainPanel (connection + tile
	# selection + construction capability).
	if terrain_panel != null:
		terrain_panel.set_command_connected(connected)
	if fauna_panel != null:
		fauna_panel.set_command_connected(connected)
	if knowledge_panel != null:
		knowledge_panel.set_command_connected(connected)

func _ensure_command_connection() -> bool:
	if command_client == null:
		command_connected = false
		_update_command_status()
		return false
	if not command_client.has_method("ensure_connected"):
		command_connected = false
		_update_command_status()
		return false
	var ensure_err: Error = command_client.call("ensure_connected")
	match ensure_err:
		OK:
			command_connected = true
			_update_command_status()
			return true
		ERR_BUSY:
			command_connected = false
			_append_command_log("Command pending: command socket still connecting.")
			_update_command_status()
			return false
		_:
			command_connected = false
			_append_command_log("Command unavailable (%s)." % error_string(ensure_err), true)
			_update_command_status()
			return false

## **THE ACCEPTED-SEND LINE IS THE ONE ACKNOWLEDGEMENT PATH IN THE CLIENT**, so it is the one place
## `KIND_COMMAND_ECHO` is stated. That is the boundary in code: `success_message` restates an action
## the player just took and rides the echo kind the dock ignores, while both failure exits — no
## connection, and a refused write — stay `KIND_SYSTEM` (the second as an Alert), because a command
## that did NOT go is exactly what the System channel is for.
##
## `ack_kind` exists for the caller whose "success message" is not a receipt at all: `Main`'s
## `resync` is sent by the CLIENT after it drops an unapplicable delta, so its line reports a fault
## the player never asked for and it passes `KIND_SYSTEM` back in.
func _send_command(line: String, success_message: String,
		ack_kind: String = HudEventVocab.KIND_COMMAND_ECHO) -> bool:
	if not _ensure_command_connection():
		return false
	var err: Error = command_client.call("send_line", line)
	if err == ERR_BUSY:
		command_client.call("poll")
		err = command_client.call("send_line", line)
	if err != OK:
		# **A SEND THAT FAILS HERE IS A TRANSPORT FAILURE AND NOTHING ELSE**, so it says so —
		# `send_line` only ever answers "no bridge" or "the bridge could not deliver", and a command
		# the SIM refuses has already gone down the socket and comes back on the server's own stream.
		# The old `Command failed (…): can't connect` read as a rules rejection; see
		# `HudEventVocab.COMMAND_NOT_SENT_FORMAT`.
		_append_command_log(HudEventVocab.COMMAND_NOT_SENT_FORMAT % line, true)
		_update_command_status()
		return false
	_append_command_log(success_message, false, ack_kind)
	_update_command_status()
	return true

func send_runtime_command(line: String, success_message: String,
		ack_kind: String = HudEventVocab.KIND_COMMAND_ECHO) -> bool:
	return _send_command(line, success_message, ack_kind)

## Optional observer invoked after a turn is advanced through THIS coordinator — i.e. the dev
## toolbar and autoplay, which are DELIBERATELY NOT gated by the client-side end-turn gate the
## turn orb applies (docs/plan_the_telling.md §1a: autoplay disables itself on a failed advance,
## so a hard gate here would deadlock the dev loop, and the server auto-expires an unanswered
## fork to its defer branch anyway). Main uses it to make that consequence VISIBLE rather than
## silent — skipping the question is a coherent dev-tool act, but it must not go unremarked.
var _turn_advance_observer: Callable = Callable()

func set_turn_advance_observer(observer: Callable) -> void:
	_turn_advance_observer = observer

func _send_turn(steps: int) -> bool:
	var sent := _send_command("turn %d" % steps, "+%d turns requested." % steps)
	if sent and _turn_advance_observer.is_valid():
		_turn_advance_observer.call(steps)
	return sent

func _request_rollback(steps: int) -> void:
	if _last_turn <= 0:
		_append_command_log("Rollback unavailable (turn 0).")
		return
	var target: int = max(_last_turn - steps, 0)
	if target == _last_turn:
		_append_command_log("Rollback unavailable (turn 0).")
		return
	_send_command("rollback %d" % target, "Rollback to turn %d requested." % target)

func _on_step_one_button_pressed() -> void:
	_send_turn(1)

func _on_step_ten_button_pressed() -> void:
	_send_turn(10)

func _on_rollback_ten_button_pressed() -> void:
	_request_rollback(10)

func _on_rollback_button_pressed() -> void:
	_request_rollback(1)

func _on_play_pause_button_pressed() -> void:
	# The toolbar Play/Pause is autoplay's ONLY control now that the Commands tab (which carried a
	# second, mirrored toggle) is gone; _on_autoplay_toggled is still the one entry point so the
	# button, the timer and the log line can never disagree.
	_on_autoplay_toggled(play_pause_button.button_pressed)

## Start/stop the autoplay timer and keep the toolbar button showing the truth. Assigning
## `button_pressed` fires `toggled`, not `pressed`, and the button is wired on `pressed` — so
## writing the mirror here cannot re-enter this method.
func _on_autoplay_toggled(pressed: bool) -> void:
	if play_pause_button != null and play_pause_button.button_pressed != pressed:
		play_pause_button.button_pressed = pressed
	if pressed:
		if not _ensure_command_connection():
			if play_pause_button != null:
				play_pause_button.button_pressed = false
			_append_command_log("Auto-play requires an active command connection.")
			return
		if autoplay_timer != null:
			autoplay_timer.wait_time = AUTOPLAY_INTERVAL_SECONDS
			autoplay_timer.start()
		_append_command_log("Auto-play enabled (%.2fs)." % AUTOPLAY_INTERVAL_SECONDS)
	else:
		_disable_autoplay(false)

func _on_autoplay_timeout() -> void:
	if not _send_turn(1):
		_disable_autoplay(true)

## Stop autoplay from any cause — the toggle, a failed advance, a lost stream. The button un-presses
## with the timer: it is the only autoplay affordance left, so a stopped timer under a still-lit
## button would be a lie the player has no way to correct except by clicking twice.
func _disable_autoplay(log_message: bool) -> void:
	if autoplay_timer != null and not autoplay_timer.is_stopped():
		autoplay_timer.stop()
		if log_message:
			_append_command_log("Auto-play paused.")
	if play_pause_button != null:
		play_pause_button.button_pressed = false

func attach_map_view(view: Node) -> void:
	_map_view = view
	if map_panel != null:
		map_panel.set_map_view(view)
	if overlay_panel != null:
		overlay_panel.set_map_view(view)
	if culture_panel != null:
		culture_panel.set_map_view(view)
	if terrain_panel != null:
		terrain_panel.set_map_view(view)

func set_hud_layer(layer: Object) -> void:
	_hud_layer = layer
	_update_panel_layout()

## Inbound MapView hex-selection (wired in Main.gd to inspector.focus_tile_from_map);
## forwarded to the Terrain tab which owns tile drill-down.
func focus_tile_from_map(col: int, row: int, terrain_id: int) -> void:
	if terrain_panel != null:
		terrain_panel.focus_tile_from_map(col, row, terrain_id)

func _on_log_stream_entry(entry: Dictionary) -> void:
	# Cross-panel dispatch of a raw log-stream entry (LogsPanel owns display/sparkline).
	if knowledge_panel != null:
		knowledge_panel.ingest_log_entry(entry)

func get_resolved_font_size() -> int:
	return _resolved_font_size

func _apply_typography_style(controls: Array, style: StringName) -> void:
	for control in controls:
		if control is Control:
			Typography.apply(control, style)

func _panel_top_offset() -> float:
	var baseline := PANEL_MARGIN + Typography.line_height(Typography.STYLE_HEADING)
	baseline = max(baseline, PANEL_MIN_TOP_OFFSET)
	if _hud_layer != null and _hud_layer.has_method("get_upper_stack_height"):
		var height_variant: Variant = _hud_layer.call("get_upper_stack_height")
		if typeof(height_variant) in [TYPE_FLOAT, TYPE_INT]:
			baseline = max(baseline, float(height_variant))
	return baseline

func _update_panel_layout() -> void:
	if root_panel == null:
		return
	var required_width: float = PANEL_WIDTH_MIN
	if tab_container != null:
		var min_from_content: float = tab_container.get_combined_minimum_size().x
		var actual_content: float = tab_container.size.x
		var inner_width: float = max(min_from_content, actual_content)
		if inner_width > 0.0:
			required_width = max(required_width, inner_width + PANEL_TAB_PADDING)
	var max_width: float = _max_panel_width()
	if required_width > max_width:
		required_width = max_width
	_panel_width = clamp(_panel_width, required_width, max_width)
	root_panel.offset_left = PANEL_MARGIN
	root_panel.offset_right = PANEL_MARGIN + _panel_width
	root_panel.offset_top = _panel_top_offset()
	root_panel.offset_bottom = -PANEL_MARGIN
	root_panel.custom_minimum_size = Vector2(_panel_width, 0)
	reserved_width_changed.emit(reserved_width())

func _on_viewport_resized() -> void:
	_update_panel_layout()

func _max_panel_width() -> float:
	var target_viewport = _viewport if _viewport != null else get_viewport()
	if target_viewport == null:
		return PANEL_WIDTH_DEFAULT
	var viewport_size = target_viewport.get_visible_rect().size
	var max_allowed = viewport_size.x - (PANEL_MARGIN * 2.0)
	return max(max_allowed, PANEL_WIDTH_MIN)

func _is_in_resize_region(local_pos: Vector2) -> bool:
	return root_panel != null and local_pos.x >= (root_panel.size.x - PANEL_HANDLE_WIDTH)

func _on_root_panel_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		var mouse_event = event as InputEventMouseButton
		if mouse_event.button_index == MOUSE_BUTTON_LEFT:
			if mouse_event.pressed and _is_in_resize_region(mouse_event.position):
				_is_resizing = true
				root_panel.mouse_default_cursor_shape = Control.CURSOR_HSIZE
				root_panel.grab_focus()
				root_panel.accept_event()
			elif not mouse_event.pressed and _is_resizing:
				_is_resizing = false
				root_panel.mouse_default_cursor_shape = Control.CURSOR_ARROW
				root_panel.accept_event()
	elif event is InputEventMouseMotion:
		var motion = event as InputEventMouseMotion
		if _is_resizing:
			_panel_width = clamp(_panel_width + motion.relative.x, PANEL_WIDTH_MIN, _max_panel_width())
			_update_panel_layout()
			root_panel.accept_event()
		else:
			if _is_in_resize_region(motion.position):
				root_panel.mouse_default_cursor_shape = Control.CURSOR_HSIZE
			else:
				root_panel.mouse_default_cursor_shape = Control.CURSOR_ARROW

func _ingest_overlays(overlays: Variant) -> void:
	if not (overlays is Dictionary):
		return
	var overlay_dict: Dictionary = overlays
	# The biome palette + tag labels arrive on the overlays key but belong to Terrain.
	if overlay_dict.has("terrain_palette") and terrain_panel != null:
		var palette_variant: Variant = overlay_dict["terrain_palette"]
		if palette_variant is Dictionary:
			terrain_panel.set_terrain_palette(palette_variant as Dictionary)
	if overlay_dict.has("terrain_tag_labels") and terrain_panel != null:
		var tag_variant: Variant = overlay_dict["terrain_tag_labels"]
		if tag_variant is Dictionary:
			terrain_panel.set_terrain_tag_labels(tag_variant as Dictionary)
	if overlay_dict.has("crisis_annotations") and crisis_panel != null:
		crisis_panel.ingest_annotations(overlay_dict["crisis_annotations"])
	# Overlay channels are owned by OverlayPanel; hand it the payload plus Terrain's tag
	# labels (which gate the terrain-tags channel).
	if overlay_panel != null:
		var tag_labels: Dictionary = terrain_panel.get_terrain_tag_labels() if terrain_panel != null else {}
		overlay_panel.ingest(overlay_dict, tag_labels)

func _on_script_log_from_package(script_id: int, level: String, message: String) -> void:
	var prefix: String = "[SCRIPT %d]" % script_id if script_id >= 0 else "[SCRIPT]"
	var normalized_level: String = _normalize_log_level(level)
	var target: String = "script.%d" % script_id if script_id >= 0 else "script"
	var entry: String = "%s %s" % [prefix, message]
	_append_log_entry(entry, normalized_level, target)

func _on_script_alert_from_package(script_id: int, data: Dictionary) -> void:
	var title: String = data.get("title", "Alert")
	var level: String = data.get("level", "info")
	var body: String = data.get("message", "")
	var prefix: String = "[SCRIPT %d]" % script_id if script_id >= 0 else "[SCRIPT]"
	var normalized_level: String = _normalize_log_level(level)
	var target: String = "script.%d" % script_id if script_id >= 0 else "script"
	_append_log_entry("%s alert (%s): %s" % [prefix, normalized_level.to_lower(), title], normalized_level, target)
	if not body.is_empty():
		_append_log_entry("  %s" % body, normalized_level, target)

func _on_script_event_from_package(script_id: int, event_name: String, payload: Variant) -> void:
	if event_name == "commands.issue.result" and typeof(payload) == TYPE_DICTIONARY:
		var ok: bool = payload.get("ok", false)
		var line: String = payload.get("line", "")
		var prefix: String = "[SCRIPT %d]" % script_id if script_id >= 0 else "[SCRIPT]"
		var target: String = "script.%d" % script_id if script_id >= 0 else "script"
		if ok:
			_append_log_entry("%s command acknowledged: %s" % [prefix, line], "INFO", target)
		else:
			_append_log_entry("%s command failed: %s" % [prefix, line], "WARN", target)

func _append_log_entry(entry: String, level: String = "INFO", target: String = "inspector", timestamp_ms: int = -1) -> void:
	# Thin forwarder: synthetic log lines (command log, culture tensions, script logs)
	# are recorded/displayed by the LogsPanel, which owns the log buffer.
	if logs_panel != null:
		logs_panel.append_entry(entry, level, target, timestamp_ms)

# Small local copy for the script-alert display strings, which need the normalized
# level before handing off (LogsPanel re-normalizes on record).
func _normalize_log_level(level: String) -> String:
	var upper: String = level.to_upper()
	match upper:
		"WARNING":
			return "WARN"
		"ERR":
			return "ERROR"
		_:
			return upper

# Capability gating
func update_capability_flags(flags: int) -> void:
	capability_flags = flags
	_apply_capability_gating()

func _apply_capability_gating() -> void:
	# Power stays a clickable tab; when its capability is locked the panel renders an
	# explanation of how it unlocks rather than being greyed out (see PowerPanel).
	if power_panel != null:
		power_panel.set_available(_has_flag(CAP_POWER))
	if great_discoveries_panel != null:
		great_discoveries_panel.set_available(_has_flag(CAP_MEGAPROJECTS))
	# Knowledge stays a clickable tab; the panel renders a locked explanation while gated.
	if knowledge_panel != null:
		knowledge_panel.set_available(_has_flag(CAP_ESPIONAGE_T2))
	# Terrain is an always-available inspection tab (biome list, tile drill-down, terrain
	# highlight) with no capability-gated actions.
	_set_tab_enabled("Terrain", true)
	# Crisis stays a clickable tab; the panel renders a locked explanation while gated.
	if crisis_panel != null:
		crisis_panel.set_available(_has_flag(CAP_MEGAPROJECTS))
	# Influencers stays a clickable tab; the panel renders a locked explanation while gated.
	if influencer_panel != null:
		influencer_panel.set_available(_has_flag(CAP_INDUSTRY_T1) or _has_flag(CAP_INDUSTRY_T2))

func _set_tab_enabled(name: String, enabled: bool) -> void:
	if tab_container == null:
		return
	for i in range(tab_container.get_tab_count()):
		if tab_container.get_tab_title(i) == name:
			tab_container.set_tab_disabled(i, not enabled)
			break

func _has_flag(bit: int) -> bool:
	return (capability_flags & bit) != 0
