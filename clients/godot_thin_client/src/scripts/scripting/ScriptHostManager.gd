extends Node
class_name ScriptHostManager

signal packages_changed(packages_snapshot)
signal script_alert(script_id, data: Dictionary)
signal script_log(script_id, level: String, message: String)
signal script_event(script_id, event_name: String, payload: Variant)

const HOST_CLASS_NAME := "ScriptHostBridge"
const RES_SCRIPTS_ROOT := "res://addons/shared_scripts"
const USER_SCRIPTS_ROOT := "user://scripts"
const MANIFEST_FILENAME := "manifest.json"
const SCRIPT_TICK_BUDGET_MS := 8.0

enum PackageFields { MANIFEST, MANIFEST_PATH, ENTRY_PATH, ENABLED, SCRIPT_ID, SUBSCRIPTIONS, LAST_ERROR }

var _host: Object = null
var _command_client: CommandClient = null
var _packages: Dictionary = {}
var _active_scripts: Dictionary = {}

func setup(command_client: CommandClient) -> void:
    _command_client = command_client
    _initialize_host()
    _scan_packages()
    set_process(_host != null)

func has_host() -> bool:
    return _host != null

func packages_snapshot() -> Array:
    var list: Array = []
    for key in _packages.keys():
        var pkg: Dictionary = _packages[key]
        list.append({
            "key": key,
            "manifest": pkg.get(PackageFields.MANIFEST, {}),
            "manifest_path": pkg.get(PackageFields.MANIFEST_PATH, ""),
            "entry_path": pkg.get(PackageFields.ENTRY_PATH, ""),
            "enabled": pkg.get(PackageFields.ENABLED, false),
            "script_id": pkg.get(PackageFields.SCRIPT_ID, -1),
            "subscriptions": _ensure_array(pkg.get(PackageFields.SUBSCRIPTIONS, [])),
            "last_error": pkg.get(PackageFields.LAST_ERROR, ""),
        })
    return list

func enable_package(key: String) -> Dictionary:
    if _host == null:
        return {"ok": false, "error": "Scripting host unavailable"}
    if not _packages.has(key):
        return {"ok": false, "error": "Unknown package"}
    var pkg: Dictionary = _packages[key]
    if pkg.get(PackageFields.ENABLED, false):
        return {"ok": true, "script_id": pkg.get(PackageFields.SCRIPT_ID, -1)}
    var entry_path: String = pkg.get(PackageFields.ENTRY_PATH, "")
    var source_text := _read_text_file(entry_path)
    if source_text.is_empty():
        pkg[PackageFields.LAST_ERROR] = "Unable to read script entry file"
        _packages[key] = pkg
        _emit_packages_changed()
        return {"ok": false, "error": pkg[PackageFields.LAST_ERROR]}
    var manifest: Dictionary = pkg.get(PackageFields.MANIFEST, {})
    if pkg.get(PackageFields.SUBSCRIPTIONS, []).size() > 0:
        var capabilities: Array = _ensure_array(manifest.get("capabilities", []))
        if not capabilities.has("telemetry.subscribe"):
            pkg[PackageFields.LAST_ERROR] = "Manifest declares subscriptions but missing telemetry.subscribe capability"
            _packages[key] = pkg
            _emit_packages_changed()
            return {"ok": false, "error": pkg[PackageFields.LAST_ERROR]}
    var spawn_result: Dictionary = _host.spawn_script(manifest.duplicate(true), source_text)
    if not spawn_result.get("ok", false):
        var err: String = spawn_result.get("error", "Failed to spawn script")
        pkg[PackageFields.LAST_ERROR] = err
        _packages[key] = pkg
        _emit_packages_changed()
        return {"ok": false, "error": err}
    var script_id: int = spawn_result.get("script_id", -1)
    pkg[PackageFields.ENABLED] = true
    pkg[PackageFields.SCRIPT_ID] = script_id
    pkg[PackageFields.LAST_ERROR] = ""
    pkg[PackageFields.SUBSCRIPTIONS] = _variant_collection_to_array(_host.subscriptions(script_id))
    _packages[key] = pkg
    _active_scripts[script_id] = key
    _emit_packages_changed()
    return {"ok": true, "script_id": script_id}

func disable_package(key: String) -> Dictionary:
    if not _packages.has(key):
        return {"ok": false, "error": "Unknown package"}
    var pkg: Dictionary = _packages[key]
    if not pkg.get(PackageFields.ENABLED, false):
        return {"ok": true}
    var script_id: int = pkg.get(PackageFields.SCRIPT_ID, -1)
    if _host != null and script_id >= 0:
        _host.shutdown_script(script_id)
    _active_scripts.erase(script_id)
    pkg[PackageFields.ENABLED] = false
    pkg[PackageFields.SCRIPT_ID] = -1
    pkg[PackageFields.SUBSCRIPTIONS] = []
    _packages[key] = pkg
    _emit_packages_changed()
    return {"ok": true}

func reload_package(key: String) -> Dictionary:
    var disable_result := disable_package(key)
    if not disable_result.get("ok", false):
        return disable_result
    return enable_package(key)

func broadcast_topic(topic: String, payload: Variant) -> void:
    if _host == null:
        return
    for key in _packages.keys():
        var pkg: Dictionary = _packages[key]
        if not pkg.get(PackageFields.ENABLED, false):
            continue
        var subs: Array = pkg.get(PackageFields.SUBSCRIPTIONS, [])
        if subs.has(topic):
            var script_id: int = pkg.get(PackageFields.SCRIPT_ID, -1)
            if script_id >= 0:
                _host.dispatch_event(script_id, topic, payload)

func handle_snapshot(snapshot: Dictionary) -> void:
    broadcast_topic("world.snapshot", snapshot)

func handle_delta(delta: Dictionary) -> void:
    broadcast_topic("world.delta", delta)

func refresh() -> void:
    _scan_packages()

func _initialize_host() -> void:
    if ClassDB.class_exists(HOST_CLASS_NAME):
        _host = ClassDB.instantiate(HOST_CLASS_NAME)
    if _host == null:
        push_warning("ScriptHostBridge unavailable; shared scripting disabled")

func _scan_packages() -> void:
    _packages.clear()
    _active_scripts.clear()
    _scan_root_for_manifests(RES_SCRIPTS_ROOT)
    _scan_root_for_manifests(USER_SCRIPTS_ROOT)
    _emit_packages_changed()

func _scan_root_for_manifests(root_path: String) -> void:
    var dir := DirAccess.open(root_path)
    if dir == null:
        return
    dir.list_dir_begin()
    while true:
        var name := dir.get_next()
        if name == "":
            break
        if name.begins_with("."):
            continue
        var full_path := root_path.path_join(name)
        if dir.current_is_dir():
            var manifest_path := full_path.path_join(MANIFEST_FILENAME)
            if FileAccess.file_exists(manifest_path):
                _load_manifest(manifest_path)
            else:
                _scan_root_for_manifests(full_path)
        elif name == MANIFEST_FILENAME:
            _load_manifest(full_path)
    dir.list_dir_end()

func _load_manifest(manifest_path: String) -> void:
    if _host == null or not FileAccess.file_exists(manifest_path):
        return
    var file := FileAccess.open(manifest_path, FileAccess.READ)
    if file == null:
        return
    var manifest_json := file.get_as_text()
    file.close()
    if manifest_json.is_empty():
        return
    var result: Dictionary = _host.parse_manifest(manifest_path, manifest_json)
    if not result.get("ok", false):
        push_warning("Manifest parse error for %s: %s" % [manifest_path, result.get("error", "unknown")])
        return
    var manifest_dict: Dictionary = result.get("manifest", {})
    var entry_path: String = result.get("entry_path", "")
    var key := _package_key(manifest_dict, manifest_path)
    _packages[key] = {
        PackageFields.MANIFEST: manifest_dict,
        PackageFields.MANIFEST_PATH: manifest_path,
        PackageFields.ENTRY_PATH: entry_path,
        PackageFields.ENABLED: false,
        PackageFields.SCRIPT_ID: -1,
        PackageFields.SUBSCRIPTIONS: _ensure_array(manifest_dict.get("subscriptions", [])),
        PackageFields.LAST_ERROR: "",
    }

func _package_key(manifest: Dictionary, manifest_path: String) -> String:
    var manifest_id: String = manifest.get("id", manifest_path)
    return "%s@%s" % [manifest_id, manifest_path]

func _emit_packages_changed() -> void:
    emit_signal("packages_changed", packages_snapshot())

func _read_text_file(path: String) -> String:
    if path.is_empty() or not FileAccess.file_exists(path):
        return ""
    var file := FileAccess.open(path, FileAccess.READ)
    if file == null:
        return ""
    var text := file.get_as_text()
    file.close()
    return text

func _process(delta: float) -> void:
    if _host == null:
        return
    for key in _packages.keys():
        var pkg: Dictionary = _packages[key]
        if not pkg.get(PackageFields.ENABLED, false):
            continue
        var script_id: int = pkg.get(PackageFields.SCRIPT_ID, -1)
        if script_id < 0:
            continue
        _host.tick_script(script_id, delta, SCRIPT_TICK_BUDGET_MS)
        var responses: Array = _host.poll_responses(script_id)
        if responses.size() > 0:
            _handle_responses(key, pkg, responses)

func _handle_responses(key: String, pkg: Dictionary, responses: Array) -> void:
    var updated := false
    var script_id: int = pkg.get(PackageFields.SCRIPT_ID, -1)
    for response_variant in responses:
        if typeof(response_variant) != TYPE_DICTIONARY:
            continue
        var response: Dictionary = response_variant
        match response.get("type", ""):
            "ready":
                pass
            "log":
                emit_signal("script_log", script_id, response.get("level", "info"), response.get("message", ""))
            "error":
                pkg[PackageFields.LAST_ERROR] = response.get("message", "Runtime error")
                emit_signal("script_log", script_id, "error", pkg[PackageFields.LAST_ERROR])
                updated = true
            "request":
                _handle_host_request(script_id, response)
            "alert":
                emit_signal("script_alert", script_id, response)
            "event":
                emit_signal("script_event", script_id, response.get("event", ""), response.get("payload", {}))
            "subscriptions":
                pkg[PackageFields.SUBSCRIPTIONS] = _coerce_array(response.get("topics", []))
                updated = true
            "over_budget":
                emit_signal("script_log", script_id, "warn", "Script exceeded budget (%s ms)" % response.get("elapsed_ms", 0.0))
            "terminated":
                _active_scripts.erase(script_id)
                pkg[PackageFields.ENABLED] = false
                pkg[PackageFields.SCRIPT_ID] = -1
                pkg[PackageFields.SUBSCRIPTIONS] = []
                updated = true
            _:
                pass
    _packages[key] = pkg
    if updated:
        _emit_packages_changed()

func _handle_host_request(script_id: int, response: Dictionary) -> void:
    var op: String = response.get("op", "")
    var payload: Variant = response.get("payload", {})
    match op:
        "commands.issue":
            _handle_command_issue(script_id, payload)
        _:
            emit_signal("script_log", script_id, "warn", "Unhandled host request: %s" % op)

func _handle_command_issue(script_id: int, payload: Variant) -> void:
    if _command_client == null:
        emit_signal("script_log", script_id, "error", "Command client unavailable")
        return
    var line: String = ""
    if typeof(payload) == TYPE_DICTIONARY:
        line = payload.get("line", "")
    if line.is_empty():
        emit_signal("script_log", script_id, "warn", "commands.issue missing line payload")
        return
    var err: Error = _command_client.send_line(line)
    if err != OK:
        emit_signal("script_log", script_id, "error", "Command failed (%s)" % err)
        if script_id >= 0 and _host != null:
            _host.dispatch_event(script_id, "commands.issue.result", {"ok": false, "line": line, "error": err})
    else:
        emit_signal("script_log", script_id, "info", "Command issued: %s" % line)
        if script_id >= 0 and _host != null:
            _host.dispatch_event(script_id, "commands.issue.result", {"ok": true, "line": line})

func _variant_collection_to_array(value) -> Array:
    var result: Array = []
    if value == null:
        return result
    for item in value:
        result.append(item)
    return result

func _coerce_array(value: Variant) -> Array:
    match typeof(value):
        TYPE_ARRAY:
            return value.duplicate(true)
        TYPE_PACKED_STRING_ARRAY:
            return Array(value)
        TYPE_OBJECT:
            return _variant_collection_to_array(value)
    return []

func _ensure_array(value) -> Array:
    if typeof(value) == TYPE_ARRAY:
        return value.duplicate(true)
    return _variant_collection_to_array(value)
