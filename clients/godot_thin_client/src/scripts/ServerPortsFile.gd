extends RefCounted
class_name ServerPortsFile

## Read-only discovery of the server's ports file.
##
## The packaged playtest build pins the client to the default port block
## (snapshot_flat 41002 / command 41001 / log 41003). When those ports are busy at
## launch the server binds a different free block and publishes its choice to a
## ports file; this helper locates, reads, parses and caches that file so the two
## halves still find each other.
##
## Precedence at every call site is: explicit env var -> ports file -> hardcoded
## default. The env var must keep winning because scripts/run_stack.sh passes
## explicit STREAM_PORT/COMMAND_PORT/LOG_PORT/*_HOST and its behaviour must not
## change.
##
## This is a pure reader: the client never writes, deletes, or liveness-checks the
## file. A stale file left by a crashed server is expected and tolerated — the
## existing connect/retry paths already handle a refused connection.
##
## Implemented as a static-func helper rather than an autoload because it holds no
## node state and is needed by both Main.gd (before the scene tree settles) and
## LogsPanel.gd; both `preload` it the same way they preload their other
## collaborators, and the static cache below gives us the once-per-launch read
## without an entry in project.godot's [autoload] block.

## Full path to the ports file; when set it is used verbatim and the
## per-platform derivation below is skipped.
const ENV_PORTS_FILE := "SIM_PORTS_FILE"

## Environment variables consulted when deriving the per-platform default path.
const ENV_LOCALAPPDATA := "LOCALAPPDATA"
const ENV_HOME := "HOME"
const ENV_XDG_STATE_HOME := "XDG_STATE_HOME"

## Path fragments of the derived location. The server derives the same path from
## the same rules; keeping them as named constants avoids bare literals and makes
## a contract change greppable.
const APP_DIR_NAME := "ShadowScale"
const PORTS_FILE_NAME := "ports.json"
const MACOS_SUPPORT_FRAGMENT := "Library/Application Support"
const LINUX_STATE_FRAGMENT := ".local/state"

## OS.get_name() values we branch on.
const OS_NAME_WINDOWS := "Windows"
const OS_NAME_MACOS := "macOS"

## Keys published by the server in the ports file.
const KEY_HOST := "host"
const KEY_COMMAND := "command"
const KEY_LOG := "log"
## NOTE: the client's snapshot *stream* is the FlatBuffers socket, i.e.
## "snapshot_flat" — NOT "snapshot", which is the legacy JSON snapshot socket.
## Reading the wrong key yields a client that connects to a live socket and then
## silently never renders.
const KEY_SNAPSHOT_FLAT := "snapshot_flat"

## Valid TCP port range for a server-bound listener (port 0 is "any", which the
## server never publishes).
const PORT_MIN := 1
const PORT_MAX := 65535

## Parsed contents, or an empty Dictionary when the file is absent/invalid. The
## "absent/invalid" result is cached too, so a missing file costs one stat per
## launch rather than one per resolver call.
static var _cached_entries: Dictionary = {}
static var _cache_loaded: bool = false

## Absolute path of the ports file for this platform, or "" if it cannot be
## derived (e.g. no HOME in the environment).
static func resolve_path() -> String:
	var explicit: String = OS.get_environment(ENV_PORTS_FILE)
	if explicit != "":
		return explicit
	var os_name: String = OS.get_name()
	if os_name == OS_NAME_WINDOWS:
		var local_app_data: String = OS.get_environment(ENV_LOCALAPPDATA)
		if local_app_data == "":
			return ""
		return "%s/%s/%s" % [local_app_data.replace("\\", "/"), APP_DIR_NAME, PORTS_FILE_NAME]
	var home: String = OS.get_environment(ENV_HOME)
	if os_name == OS_NAME_MACOS:
		if home == "":
			return ""
		return "%s/%s/%s/%s" % [home, MACOS_SUPPORT_FRAGMENT, APP_DIR_NAME, PORTS_FILE_NAME]
	var xdg_state: String = OS.get_environment(ENV_XDG_STATE_HOME)
	if xdg_state != "":
		return "%s/%s/%s" % [xdg_state, APP_DIR_NAME, PORTS_FILE_NAME]
	if home == "":
		return ""
	return "%s/%s/%s/%s" % [home, LINUX_STATE_FRAGMENT, APP_DIR_NAME, PORTS_FILE_NAME]

## Host published by the server, or "" when unavailable.
static func get_host() -> String:
	var entries: Dictionary = _entries()
	if not entries.has(KEY_HOST):
		return ""
	var host: Variant = entries[KEY_HOST]
	if typeof(host) != TYPE_STRING or String(host) == "":
		return ""
	return String(host)

## Port for `key`, or 0 when absent/invalid so callers can fall through to their
## hardcoded default.
static func get_port(key: String) -> int:
	var entries: Dictionary = _entries()
	if not entries.has(key):
		return 0
	var value: Variant = entries[key]
	# JSON numbers decode as floats; accept only whole numbers in range and reject
	# strings outright, so a malformed file degrades to the defaults.
	if typeof(value) != TYPE_FLOAT and typeof(value) != TYPE_INT:
		return 0
	var as_float: float = float(value)
	if as_float != floor(as_float):
		return 0
	var port: int = int(as_float)
	if port < PORT_MIN or port > PORT_MAX:
		return 0
	return port

## Test/tooling seam: drop the cache so the next lookup re-reads the file.
static func reset_cache() -> void:
	_cached_entries = {}
	_cache_loaded = false

static func _entries() -> Dictionary:
	if _cache_loaded:
		return _cached_entries
	_cache_loaded = true
	_cached_entries = _load_entries()
	if not _cached_entries.is_empty():
		# One informational line, and only when the file is actually used. Reported as
		# the RESOLVED values (ports are ints here, not the raw JSON floats), so the log
		# says exactly what the client will dial.
		print("[PortsFile] Discovered server ports at %s: host=%s snapshot_flat=%d command=%d log=%d" % [
			resolve_path(),
			get_host(),
			get_port(KEY_SNAPSHOT_FLAT),
			get_port(KEY_COMMAND),
			get_port(KEY_LOG)
		])
	return _cached_entries

static func _load_entries() -> Dictionary:
	var path: String = resolve_path()
	if path == "":
		return {}
	# A real filesystem path, not res:// or user:// — FileAccess.open handles both.
	if not FileAccess.file_exists(path):
		# Absent is the normal case for a default-ported server: stay silent.
		return {}
	var file: FileAccess = FileAccess.open(path, FileAccess.READ)
	if file == null:
		return {}
	var text: String = file.get_as_text()
	file.close()
	# JSON.new().parse() rather than the JSON.parse_string() static: the static pushes an
	# engine-level ERROR to the console on malformed input. A playtester running a
	# normally-ported server must never see an error because of this feature, so the
	# failure is reported by return code and swallowed here.
	var json: JSON = JSON.new()
	if json.parse(text) != OK:
		return {}
	var parsed: Variant = json.data
	if typeof(parsed) != TYPE_DICTIONARY:
		return {}
	return parsed
