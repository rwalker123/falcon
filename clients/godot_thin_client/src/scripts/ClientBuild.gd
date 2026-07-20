extends RefCounted

## Read-only discovery of the client's git build stamp.
##
## Mirrors the SERVER's build id (`core_sim/build.rs` stamps `CORE_SIM_BUILD_ID =
## <commit-date>-<short-hash>` at compile time). GDScript has no compile step to hang a build
## script off, so `scripts/run_stack.sh` writes the same `<commit-date>-<short-hash>[-dirty]`
## string to `res://build_stamp.txt` when it builds the client; this helper reads it so the
## bottom-centre build overlay can never show a stale hand-bumped constant again.
##
## Fails SILENTLY, exactly like `ServerPortsFile`: a missing / unreadable / blank stamp degrades
## to the `fallback` the caller passes (Hud's `CLIENT_BUILD` const, `"dev-unknown"`). ui_preview
## and any bare-`godot` launch have no stamp written, so they read the fallback — that is correct,
## not an error.
##
## A static-func helper (no `class_name`, no autoload) for the SAME reasons `ServerPortsFile` is:
## it holds no node state, and `Hud` `preload`s it like a collaborator rather than depending on the
## global class cache. The parsed result is cached once per launch, including the absent one.

## Written by `scripts/run_stack.sh` beside the project (a real `res://` path). Gitignored — a
## per-launch artifact, like the ports file and the `.godot` class cache.
const STAMP_PATH := "res://build_stamp.txt"

## Parsed stamp, or "" when the file is absent/blank. The empty result is cached too, so a missing
## file costs one read per launch rather than one per overlay refresh.
static var _cached_stamp: String = ""
static var _cache_loaded: bool = false

## The build stamp to display: the git stamp when present, else `fallback` (the const the caller owns).
static func current(fallback: String = "dev-unknown") -> String:
	var stamp: String = _stamp()
	return stamp if stamp != "" else fallback

## Test/tooling seam: drop the cache so the next lookup re-reads the file.
static func reset_cache() -> void:
	_cached_stamp = ""
	_cache_loaded = false

static func _stamp() -> String:
	if _cache_loaded:
		return _cached_stamp
	_cache_loaded = true
	_cached_stamp = _load_stamp()
	return _cached_stamp

static func _load_stamp() -> String:
	if not FileAccess.file_exists(STAMP_PATH):
		# Absent is the normal case for a bare-godot / ui_preview launch: stay silent.
		return ""
	var file: FileAccess = FileAccess.open(STAMP_PATH, FileAccess.READ)
	if file == null:
		return ""
	var text: String = file.get_as_text().strip_edges()
	file.close()
	return text
