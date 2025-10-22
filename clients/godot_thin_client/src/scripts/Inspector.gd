extends CanvasLayer
class_name InspectorLayer

@onready var sentiment_text: RichTextLabel = $RootPanel/TabContainer/Sentiment/SentimentText
@onready var terrain_text: RichTextLabel = $RootPanel/TabContainer/Terrain/TerrainText
@onready var influencers_text: RichTextLabel = $RootPanel/TabContainer/Influencers/InfluencersText
@onready var corruption_text: RichTextLabel = $RootPanel/TabContainer/Corruption/CorruptionText
@onready var logs_text: RichTextLabel = $RootPanel/TabContainer/Logs/LogsText
@onready var root_panel: Panel = $RootPanel
@onready var tab_container: TabContainer = $RootPanel/TabContainer
@onready var command_status_label: Label = $RootPanel/TabContainer/Commands/StatusLabel
@onready var step_one_button: Button = $RootPanel/TabContainer/Commands/ControlsRow/StepOneButton
@onready var step_ten_button: Button = $RootPanel/TabContainer/Commands/ControlsRow/StepTenButton
@onready var rollback_button: Button = $RootPanel/TabContainer/Commands/ControlsRow/RollbackButton
@onready var autoplay_toggle: CheckButton = $RootPanel/TabContainer/Commands/AutoplayRow/AutoplayToggle
@onready var autoplay_spin: SpinBox = $RootPanel/TabContainer/Commands/AutoplayRow/AutoplayIntervalSpin
@onready var autoplay_label: Label = $RootPanel/TabContainer/Commands/AutoplayRow/AutoplayIntervalLabel
@onready var command_log_text: RichTextLabel = $RootPanel/TabContainer/Commands/LogPanel/LogScroll/LogText

var _axis_bias: Dictionary = {}
var _sentiment: Dictionary = {}
var _influencers: Dictionary = {}
var _corruption: Dictionary = {}
var _terrain_palette: Dictionary = {}
var _terrain_tag_labels: Dictionary = {}
var _tile_records: Dictionary = {}
var _terrain_counts: Dictionary = {}
var _terrain_tag_counts: Dictionary = {}
var _tile_total: int = 0
var _log_messages: Array[String] = []
var _last_turn: int = 0
var command_client: Object = null
var command_connected: bool = false
var stream_active: bool = false
var autoplay_timer: Timer
var command_log: Array[String] = []
const COMMAND_LOG_LIMIT := 40
const TERRAIN_TOP_LIMIT := 5
const TAG_TOP_LIMIT := 6
const LOG_ENTRY_LIMIT := 60
const DEFAULT_FONT_SIZE := 22
const MIN_FONT_SIZE := 12
const MAX_FONT_SIZE := 36
const PANEL_MIN_WIDTH := 320.0
const PANEL_MAX_WIDTH := 560.0
const PANEL_WIDTH_RATIO := 0.28
const PANEL_MARGIN := 16.0

var _viewport: Viewport = null

func _ready() -> void:
    _viewport = get_viewport()
    if _viewport != null:
        _viewport.size_changed.connect(_on_viewport_resized)
    _apply_theme_overrides()
    _update_panel_layout()
    _render_static_sections()
    _setup_command_controls()

func update_snapshot(snapshot: Dictionary) -> void:
    _apply_update(snapshot, true)
    _render_dynamic_sections()

func update_delta(delta: Dictionary) -> void:
    _apply_update(delta, false)
    _render_dynamic_sections()

func _apply_update(data: Dictionary, full_snapshot: bool) -> void:
    if data.has("turn"):
        _last_turn = int(data.get("turn", _last_turn))

    if data.has("axis_bias"):
        var axis_dict: Dictionary = data["axis_bias"]
        _axis_bias = axis_dict.duplicate(true)

    if data.has("sentiment"):
        var sentiment_dict: Dictionary = data["sentiment"]
        _sentiment = sentiment_dict.duplicate(true)

    if full_snapshot and data.has("influencers"):
        _rebuild_influencers(data["influencers"])
    elif data.has("influencer_updates"):
        _merge_influencers(data["influencer_updates"])

    if data.has("influencer_removed"):
        _remove_influencers(data["influencer_removed"])

    if data.has("corruption"):
        var ledger: Dictionary = data["corruption"]
        _corruption = ledger.duplicate(true)

    if data.has("overlays"):
        _ingest_overlays(data["overlays"])

    if full_snapshot and data.has("tiles"):
        _rebuild_tiles(data["tiles"])
    elif data.has("tile_updates"):
        _apply_tile_updates(data["tile_updates"])

    if data.has("tile_removed"):
        _remove_tiles(data["tile_removed"])

    _record_stream_log(data, full_snapshot)

func _rebuild_influencers(array_data) -> void:
    _influencers.clear()
    for entry in array_data:
        if not (entry is Dictionary):
            continue
        var info: Dictionary = entry.duplicate(true)
        var id := int(info.get("id", 0))
        _influencers[id] = info

func _merge_influencers(array_data) -> void:
    for entry in array_data:
        if not (entry is Dictionary):
            continue
        var info: Dictionary = entry.duplicate(true)
        var id := int(info.get("id", 0))
        _influencers[id] = info

func _remove_influencers(ids) -> void:
    for id in ids:
        _influencers.erase(int(id))

func _render_dynamic_sections() -> void:
    _render_sentiment()
    _render_influencers()
    _render_corruption()
    _render_terrain()
    _render_logs()

func _render_static_sections() -> void:
    _terrain_palette.clear()
    _terrain_tag_labels.clear()
    _tile_records.clear()
    _terrain_counts.clear()
    _terrain_tag_counts.clear()
    _tile_total = 0
    _log_messages.clear()
    _render_terrain()
    _render_logs()
    command_status_label.text = "Commands: disconnected."
    command_log_text.text = ""

func _apply_theme_overrides() -> void:
    var font_size := DEFAULT_FONT_SIZE
    var env_value := OS.get_environment("INSPECTOR_FONT_SIZE")
    if env_value != "":
        var parsed := int(env_value)
        if parsed >= MIN_FONT_SIZE and parsed <= MAX_FONT_SIZE:
            font_size = parsed
    _apply_font_override(sentiment_text, font_size)
    _apply_font_override(terrain_text, font_size)
    _apply_font_override(influencers_text, font_size)
    _apply_font_override(corruption_text, font_size)
    _apply_font_override(logs_text, font_size)
    _apply_font_override(command_status_label, font_size)
    _apply_font_override(step_one_button, font_size)
    _apply_font_override(step_ten_button, font_size)
    _apply_font_override(rollback_button, font_size)
    _apply_font_override(autoplay_toggle, font_size)
    _apply_font_override(autoplay_label, font_size)
    _apply_font_override(command_log_text, font_size)
    _apply_font_override(tab_container, font_size)
    _apply_font_override(autoplay_spin, font_size)

    if root_panel != null:
        var panel_style := StyleBoxFlat.new()
        panel_style.bg_color = Color(0.09, 0.09, 0.12, 0.97)
        panel_style.border_color = Color(0.2, 0.22, 0.26, 1.0)
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
        var tab_style := StyleBoxFlat.new()
        tab_style.bg_color = Color(0.13, 0.13, 0.17, 0.99)
        tab_style.border_color = Color(0.22, 0.24, 0.28, 1.0)
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

func _setup_command_controls() -> void:
    step_one_button.pressed.connect(_on_step_one_button_pressed)
    step_ten_button.pressed.connect(_on_step_ten_button_pressed)
    rollback_button.pressed.connect(_on_rollback_button_pressed)
    autoplay_toggle.toggled.connect(_on_autoplay_toggled)
    autoplay_spin.value_changed.connect(_on_autoplay_interval_changed)
    autoplay_spin.min_value = 0.2
    autoplay_spin.max_value = 5.0
    autoplay_spin.step = 0.1
    if autoplay_spin.value < 0.2:
        autoplay_spin.value = 0.5
    autoplay_toggle.button_pressed = false
    autoplay_timer = Timer.new()
    autoplay_timer.one_shot = false
    autoplay_timer.wait_time = float(autoplay_spin.value)
    add_child(autoplay_timer)
    autoplay_timer.timeout.connect(_on_autoplay_timeout)
    _update_command_status()
    _append_command_log("Command console ready.")

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
        _append_command_log("Command endpoint disconnected.")
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
            _append_command_log("Command endpoint unavailable (%s:%d)." % [host_unavailable, port_unavailable])
        else:
            _append_command_log("Command endpoint unavailable.")

func set_streaming_active(active: bool) -> void:
    if stream_active == active:
        return
    stream_active = active
    if stream_active:
        _append_command_log("Streaming snapshots active.")
    else:
        _append_command_log("Streaming unavailable; using mock playback.")
        if autoplay_toggle.button_pressed:
            _disable_autoplay(true)
    _update_command_status()

func _update_command_status() -> void:
    var status_text: String = "Commands:"
    if command_client == null or not command_client.has_method("status"):
        status_text += " disabled."
        command_connected = false
    else:
        var st_variant = command_client.call("status")
        var st: int = st_variant if typeof(st_variant) == TYPE_INT else StreamPeerTCP.STATUS_NONE
        var host_value: String = "?"
        var port_value: int = 0
        if command_client.has_method("get"):
            var maybe_host = command_client.call("get", "host")
            var maybe_port = command_client.call("get", "port")
            if typeof(maybe_host) == TYPE_STRING:
                host_value = maybe_host
            if typeof(maybe_port) in [TYPE_INT, TYPE_FLOAT]:
                port_value = int(maybe_port)
        match st:
            StreamPeerTCP.STATUS_CONNECTED:
                status_text += " connected (%s:%d)." % [host_value, port_value]
                command_connected = true
            StreamPeerTCP.STATUS_CONNECTING:
                status_text += " connecting..."
                command_connected = false
            StreamPeerTCP.STATUS_ERROR:
                status_text += " error."
                command_connected = false
            _:
                status_text += " disconnected."
                command_connected = false
    if stream_active:
        status_text += " Streaming: active."
    else:
        status_text += " Streaming: paused."
    command_status_label.text = status_text

func _append_command_log(entry: String) -> void:
    command_log.append(entry)
    while command_log.size() > COMMAND_LOG_LIMIT:
        command_log.pop_front()
    command_log_text.text = "\n".join(command_log)
    if command_log_text.get_line_count() > 0:
        command_log_text.scroll_to_line(command_log_text.get_line_count() - 1)

func _ensure_command_connection() -> bool:
    if command_client == null:
        command_connected = false
        _update_command_status()
        return false
    if command_client != null and command_client.has_method("is_connection_active") and command_client.call("is_connection_active"):
        command_connected = true
        _update_command_status()
        return true
    command_connected = false
    _update_command_status()
    return false

func _send_command(line: String, success_message: String) -> bool:
    if not _ensure_command_connection():
        _append_command_log("Command unavailable (not connected).")
        return false
    var err: Error = command_client.send_line(line)
    if err != OK:
        _append_command_log("Command failed (%s): %s" % [line, error_string(err)])
        _update_command_status()
        return false
    _append_command_log(success_message)
    _update_command_status()
    return true

func _send_turn(steps: int) -> bool:
    return _send_command("turn %d" % steps, "+%d turns requested." % steps)

func _on_step_one_button_pressed() -> void:
    _send_turn(1)

func _on_step_ten_button_pressed() -> void:
    _send_turn(10)

func _on_rollback_button_pressed() -> void:
    if _last_turn <= 0:
        _append_command_log("Rollback unavailable (turn 0).")
        return
    var target: int = max(_last_turn - 1, 0)
    _send_command("rollback %d" % target, "Rollback to turn %d requested." % target)

func _on_autoplay_toggled(pressed: bool) -> void:
    if pressed:
        if not _ensure_command_connection():
            autoplay_toggle.button_pressed = false
            _append_command_log("Auto-play requires an active command connection.")
            return
        autoplay_timer.wait_time = float(autoplay_spin.value)
        autoplay_timer.start()
        _append_command_log("Auto-play enabled (%.2fs)." % autoplay_timer.wait_time)
    else:
        _disable_autoplay(false)

func _on_autoplay_interval_changed(value: float) -> void:
    if autoplay_timer != null and not autoplay_timer.is_stopped():
        autoplay_timer.wait_time = value
        _append_command_log("Auto-play interval set to %.2fs." % value)

func _on_autoplay_timeout() -> void:
    if not _send_turn(1):
        _disable_autoplay(true)

func _disable_autoplay(log_message: bool) -> void:
    if autoplay_timer != null and not autoplay_timer.is_stopped():
        autoplay_timer.stop()
        if log_message:
            _append_command_log("Auto-play paused.")
    if autoplay_toggle.button_pressed:
        autoplay_toggle.button_pressed = false

func _render_sentiment() -> void:
    var lines: Array[String] = []
    lines.append("[b]Turn[/b] %d" % _last_turn)

    if not _axis_bias.is_empty():
        lines.append("[b]Axis Bias[/b]")
        for key in ["knowledge", "trust", "equity", "agency"]:
            var bias_value := float(_axis_bias.get(key, 0.0))
            lines.append(" • %s: %.3f" % [key.capitalize(), bias_value])

    if not _sentiment.is_empty():
        lines.append("")
        lines.append("[b]Axis Totals[/b]")
        for key in ["knowledge", "trust", "equity", "agency"]:
            if not _sentiment.has(key):
                continue
            var axis: Dictionary = _sentiment[key]
            var total := float(axis.get("total", 0.0))
            var policy := float(axis.get("policy", 0.0))
            var incidents := float(axis.get("incidents", 0.0))
            var influencer_val := float(axis.get("influencers", 0.0))
            lines.append(" • %s: %.3f (policy %.3f | incidents %.3f | influencers %.3f)"
                % [key.capitalize(), total, policy, incidents, influencer_val])

            var drivers = axis.get("drivers", [])
            var count := 0
            for driver in drivers:
                if count >= 3:
                    break
                if not (driver is Dictionary):
                    continue
                var driver_dict: Dictionary = driver
                var label := str(driver_dict.get("label", "Unnamed"))
                var category := str(driver_dict.get("category", ""))
                var value := float(driver_dict.get("value", 0.0))
                var weight := float(driver_dict.get("weight", 0.0))
                lines.append("    · [%s] %s: %.3f × %.3f" % [category, label, value, weight])
                count += 1

    sentiment_text.text = "\n".join(lines)

func _render_influencers() -> void:
    if _influencers.is_empty():
        influencers_text.text = "[b]Influencers[/b]\nNo roster data received yet."
        return

    var entries: Array = _influencers.values()
    entries.sort_custom(Callable(self, "_compare_influencers"))

    var lines: Array[String] = []
    lines.append("[b]Influencers[/b] (%d tracked)" % entries.size())
    var limit: int = min(entries.size(), 8)
    for index in range(limit):
        var info: Dictionary = entries[index]
        var id := int(info.get("id", 0))
        var name := str(info.get("name", "Unnamed"))
        var lifecycle := str(info.get("lifecycle", ""))
        var influence := float(info.get("influence", 0.0))
        var growth := float(info.get("growth_rate", 0.0))
        var support := float(info.get("support_charge", 0.0))
        var suppress := float(info.get("suppress_pressure", 0.0))
        lines.append("%d. %s [ID %d] — %s" % [index + 1, name, id, lifecycle])
        lines.append("    influence %.3f | growth %.3f | support %.3f | suppress %.3f"
            % [influence, growth, support, suppress])

        var domains_variant = info.get("domains")
        if domains_variant is PackedStringArray:
            var domain_str := _join_strings(domains_variant)
            if domain_str != "":
                lines.append("    domains: %s" % domain_str)

    influencers_text.text = "\n".join(lines)

func _compare_influencers(a: Dictionary, b: Dictionary) -> bool:
    var a_score := float(a.get("influence", 0.0))
    var b_score := float(b.get("influence", 0.0))
    return a_score > b_score

func _render_corruption() -> void:
    if _corruption.is_empty():
        corruption_text.text = "[b]Corruption[/b]\nNo ledger data received yet."
        return

    var lines: Array[String] = []
    lines.append("[b]Corruption[/b]")
    lines.append("Reputation modifier: %.3f" % float(_corruption.get("reputation_modifier", 0.0)))
    lines.append("Audit capacity: %d" % int(_corruption.get("audit_capacity", 0)))

    var entries = _corruption.get("entries", [])
    if entries.size() == 0:
        lines.append("No active incidents.")
    else:
        lines.append("Active incidents:")
        for entry in entries:
            if not (entry is Dictionary):
                continue
            var info: Dictionary = entry
            var subsystem := str(info.get("subsystem", "Unknown"))
            var intensity := float(info.get("intensity", 0.0))
            var timer := int(info.get("exposure_timer", 0))
            var last_tick := int(info.get("last_update_tick", 0))
            lines.append(" • %s: intensity %.3f | τ=%d | updated %d"
                % [subsystem, intensity, timer, last_tick])

    corruption_text.text = "\n".join(lines)

func _render_terrain() -> void:
    if _tile_total <= 0:
        terrain_text.text = """[b]Terrain Overlay[/b]
No terrain data received yet. Palette legend remains available on the HUD."""
        return

    var lines: Array[String] = []
    lines.append("[b]Terrain Overview[/b]")
    lines.append("Tracked tiles: %d" % _tile_total)

    var terrain_entries: Array[Dictionary] = []
    for key in _terrain_counts.keys():
        var terrain_id := int(key)
        var count := int(_terrain_counts[key])
        if count <= 0:
            continue
        var percent := (float(count) / float(max(_tile_total, 1))) * 100.0
        terrain_entries.append({
            "id": terrain_id,
            "count": count,
            "percent": percent,
            "label": _label_for_terrain(terrain_id)
        })
    terrain_entries.sort_custom(Callable(self, "_compare_terrain_entries"))

    var limit: int = min(terrain_entries.size(), TERRAIN_TOP_LIMIT)
    if limit > 0:
        lines.append("Top biomes:")
        for idx in range(limit):
            var entry: Dictionary = terrain_entries[idx]
            lines.append(" • %s (ID %d): %d tiles (%.1f%%)"
                % [entry.get("label", "Unknown"), int(entry.get("id", -1)), int(entry.get("count", 0)), float(entry.get("percent", 0.0))])

    var tag_entries: Array[Dictionary] = []
    for key in _terrain_tag_counts.keys():
        var mask := int(key)
        var count := int(_terrain_tag_counts[key])
        if count <= 0:
            continue
        var percent := (float(count) / float(max(_tile_total, 1))) * 100.0
        tag_entries.append({
            "mask": mask,
            "count": count,
            "percent": percent,
            "label": _label_for_tag(mask)
        })
    tag_entries.sort_custom(Callable(self, "_compare_tag_entries"))

    var tag_limit: int = min(tag_entries.size(), TAG_TOP_LIMIT)
    if tag_limit > 0:
        lines.append("")
        lines.append("Tag coverage:")
        for idx in range(tag_limit):
            var entry2: Dictionary = tag_entries[idx]
            lines.append(" • %s: %d tiles (%.1f%%)"
                % [entry2.get("label", "Tag"), int(entry2.get("count", 0)), float(entry2.get("percent", 0.0))])

    terrain_text.text = "\n".join(lines)

func _render_logs() -> void:
    if _log_messages.is_empty():
        logs_text.text = "[b]Logs[/b]\nNo stream events received yet."
        return
    var lines: Array[String] = []
    lines.append("[b]Logs[/b]")
    for entry in _log_messages:
        lines.append(entry)
    logs_text.text = "\n".join(lines)

func _apply_font_override(control: Control, size: int) -> void:
    if control == null:
        return
    if control is RichTextLabel:
        var rich: RichTextLabel = control
        rich.add_theme_font_size_override("normal_font_size", size)
        rich.add_theme_font_size_override("bold_font_size", size)
        rich.add_theme_font_size_override("italics_font_size", size)
        rich.add_theme_font_size_override("mono_font_size", max(size - 1, MIN_FONT_SIZE))
    else:
        control.add_theme_font_size_override("font_size", size)

func _update_panel_layout() -> void:
    if root_panel == null:
        return
    var viewport_size: Vector2 = get_viewport().get_visible_rect().size
    var desired_width: float = clamp(viewport_size.x * PANEL_WIDTH_RATIO, PANEL_MIN_WIDTH, PANEL_MAX_WIDTH)
    root_panel.offset_left = PANEL_MARGIN
    root_panel.offset_right = PANEL_MARGIN + desired_width
    root_panel.offset_top = PANEL_MARGIN
    root_panel.offset_bottom = -PANEL_MARGIN
    root_panel.custom_minimum_size = Vector2(PANEL_MIN_WIDTH, 0)

func _on_viewport_resized() -> void:
    _update_panel_layout()

func _join_strings(values: PackedStringArray) -> String:
    var parts: Array[String] = []
    for value in values:
        parts.append(String(value))
    var result := ""
    for i in range(parts.size()):
        result += parts[i]
        if i < parts.size() - 1:
            result += ", "
    return result

func _compare_terrain_entries(a: Dictionary, b: Dictionary) -> bool:
    var a_count := int(a.get("count", 0))
    var b_count := int(b.get("count", 0))
    if a_count == b_count:
        return int(a.get("id", -1)) < int(b.get("id", -1))
    return a_count > b_count

func _compare_tag_entries(a: Dictionary, b: Dictionary) -> bool:
    var a_count := int(a.get("count", 0))
    var b_count := int(b.get("count", 0))
    if a_count == b_count:
        return int(a.get("mask", 0)) < int(b.get("mask", 0))
    return a_count > b_count

func _label_for_terrain(terrain_id: int) -> String:
    if _terrain_palette.has(terrain_id):
        return str(_terrain_palette[terrain_id])
    for key in _terrain_palette.keys():
        if int(key) == terrain_id:
            return str(_terrain_palette[key])
    return "Terrain %d" % terrain_id

func _label_for_tag(mask: int) -> String:
    if _terrain_tag_labels.has(mask):
        return str(_terrain_tag_labels[mask])
    for key in _terrain_tag_labels.keys():
        if int(key) == mask:
            return str(_terrain_tag_labels[key])
    return "Tag %d" % mask

func _ingest_overlays(overlays: Variant) -> void:
    if not (overlays is Dictionary):
        return
    var overlay_dict: Dictionary = overlays
    if overlay_dict.has("terrain_palette"):
        var palette_variant: Variant = overlay_dict["terrain_palette"]
        if palette_variant is Dictionary:
            _terrain_palette = (palette_variant as Dictionary).duplicate(true)
    if overlay_dict.has("terrain_tag_labels"):
        var tag_variant: Variant = overlay_dict["terrain_tag_labels"]
        if tag_variant is Dictionary:
            _terrain_tag_labels = (tag_variant as Dictionary).duplicate(true)

func _rebuild_tiles(tile_entries) -> void:
    _tile_records.clear()
    _terrain_counts.clear()
    _terrain_tag_counts.clear()
    _tile_total = 0
    if tile_entries is Array:
        for entry in tile_entries:
            _store_tile(entry)
    _tile_total = _tile_records.size()

func _apply_tile_updates(tile_entries) -> void:
    if not (tile_entries is Array):
        return
    for entry in tile_entries:
        if not (entry is Dictionary):
            continue
        var info: Dictionary = entry
        var entity := int(info.get("entity", -1))
        if entity >= 0:
            _forget_tile(entity)
        _store_tile(info)
    _tile_total = _tile_records.size()

func _remove_tiles(ids) -> void:
    if ids is Array:
        for id_value in ids:
            _forget_tile(int(id_value))
    elif ids is PackedInt64Array:
        var packed: PackedInt64Array = ids
        for idx in packed:
            _forget_tile(int(idx))
    elif ids is PackedInt32Array:
        var packed32: PackedInt32Array = ids
        for idx in packed32:
            _forget_tile(int(idx))
    _tile_total = max(_tile_records.size(), 0)

func _store_tile(entry) -> void:
    if not (entry is Dictionary):
        return
    var info: Dictionary = entry
    var entity := int(info.get("entity", -1))
    if entity < 0:
        return
    var terrain_id := int(info.get("terrain", -1))
    var tags_mask := int(info.get("terrain_tags", 0))
    var record := {
        "terrain": terrain_id,
        "tags": tags_mask
    }
    _tile_records[entity] = record
    _tile_total = max(_tile_records.size(), _tile_total + 1)
    _bump_terrain_count(terrain_id, 1)
    _bump_tag_counts(tags_mask, 1)

func _forget_tile(entity_id: int) -> void:
    if not _tile_records.has(entity_id):
        return
    var record: Dictionary = _tile_records[entity_id]
    var terrain_id := int(record.get("terrain", -1))
    var tags_mask := int(record.get("tags", 0))
    _bump_terrain_count(terrain_id, -1)
    _bump_tag_counts(tags_mask, -1)
    _tile_records.erase(entity_id)
    _tile_total = max(_tile_records.size(), _tile_total - 1)

func _bump_terrain_count(terrain_id: int, delta: int) -> void:
    if terrain_id < 0 or delta == 0:
        return
    var current := int(_terrain_counts.get(terrain_id, 0)) + delta
    if current <= 0:
        _terrain_counts.erase(terrain_id)
    else:
        _terrain_counts[terrain_id] = current

func _bump_tag_counts(mask: int, delta: int) -> void:
    if mask == 0 or delta == 0:
        return
    var remaining := mask
    while remaining != 0:
        var bit := remaining & -remaining
        if bit <= 0:
            break
        if delta > 0 and not _terrain_tag_labels.has(bit):
            _terrain_tag_labels[bit] = "Tag %d" % bit
        var current := int(_terrain_tag_counts.get(bit, 0)) + delta
        if current <= 0:
            _terrain_tag_counts.erase(bit)
        else:
            _terrain_tag_counts[bit] = current
        remaining &= remaining - 1

func _record_stream_log(data: Dictionary, full_snapshot: bool) -> void:
    var parts: Array[String] = []
    if full_snapshot:
        if _tile_total > 0:
            parts.append("%d tiles" % _tile_total)
        if data.has("populations"):
            var pop_count := _count_entries(data["populations"])
            if pop_count > 0:
                parts.append("%d population cohorts" % pop_count)
        if _influencers.size() > 0:
            parts.append("%d influencers" % _influencers.size())
        if parts.is_empty():
            return
        _append_log_entry("Turn %d snapshot → %s" % [_last_turn, ", ".join(parts)])
        return

    if data.has("tile_updates"):
        var tile_updates := _count_entries(data["tile_updates"])
        if tile_updates > 0:
            parts.append("tile updates +%d" % tile_updates)
    if data.has("tile_removed"):
        var tile_removed := _count_entries(data["tile_removed"])
        if tile_removed > 0:
            parts.append("tiles removed %d" % tile_removed)
    if data.has("population_updates"):
        var pop_updates := _count_entries(data["population_updates"])
        if pop_updates > 0:
            parts.append("population updates +%d" % pop_updates)
    if data.has("population_removed"):
        var pop_removed := _count_entries(data["population_removed"])
        if pop_removed > 0:
            parts.append("pop removed %d" % pop_removed)
    if data.has("generation_updates"):
        var gen_updates := _count_entries(data["generation_updates"])
        if gen_updates > 0:
            parts.append("generation updates +%d" % gen_updates)
    if data.has("generation_removed"):
        var gen_removed := _count_entries(data["generation_removed"])
        if gen_removed > 0:
            parts.append("gen removed %d" % gen_removed)
    if data.has("influencer_updates"):
        var infl_updates := _count_entries(data["influencer_updates"])
        if infl_updates > 0:
            parts.append("influencer updates +%d" % infl_updates)
    if data.has("influencer_removed"):
        var infl_removed := _count_entries(data["influencer_removed"])
        if infl_removed > 0:
            parts.append("influencers removed %d" % infl_removed)
    if data.has("corruption"):
        parts.append("corruption ledger refresh")

    if parts.is_empty():
        return
    _append_log_entry("Turn %d delta → %s" % [_last_turn, ", ".join(parts)])

func _append_log_entry(entry: String) -> void:
    _log_messages.append(entry)
    while _log_messages.size() > LOG_ENTRY_LIMIT:
        _log_messages.pop_front()

func _count_entries(payload) -> int:
    match typeof(payload):
        TYPE_ARRAY:
            return (payload as Array).size()
        TYPE_PACKED_INT32_ARRAY:
            return (payload as PackedInt32Array).size()
        TYPE_PACKED_INT64_ARRAY:
            return (payload as PackedInt64Array).size()
        TYPE_PACKED_FLOAT32_ARRAY:
            return (payload as PackedFloat32Array).size()
        TYPE_PACKED_STRING_ARRAY:
            return (payload as PackedStringArray).size()
        TYPE_PACKED_VECTOR2_ARRAY:
            return (payload as PackedVector2Array).size()
        TYPE_PACKED_BYTE_ARRAY:
            return (payload as PackedByteArray).size()
        _:
            return 0
