extends Node

## **The Scenario page's four verbs and the in-game world swap, against a LIVE server** — the sibling of
## `live_seat_probe` (read its header for how to start the server on its own port block and how to run a
## windowed probe through `scripts/preview.sh`). It stands the real `Main.tscn` up, waits for the world,
## then drives the same `scenario_request` service the Workbench's Scenario page is lent:
##
##   1. `list_scenarios` answers files and a schema that parses.
##   2. `read_scenario` answers a checked-in file's text, and `write_scenario` accepts it under a scratch
##      name (a trial build) and REFUSES a broken one with path-prefixed refusals.
##   3. `load_scenario` on the scratch file swaps the world UNDER THE RUNNING CLIENT: `Main`'s applied
##      world epoch advances (its per-world reset ran), the new frame shows the scenario's band where the
##      file put it, and the client still takes a turn on the new world.
##
## The scratch file is removed on the way out. The status is the verdict.

const MAIN_SCENE := preload("res://src/Main.tscn")
const TAG := "scenario_live_probe"
const EXIT_OK := 0
const EXIT_FAILED := 1
const REVEAL_TIMEOUT_MSEC := 75_000
const REPLY_TIMEOUT_MSEC := 120_000
const TURN_TIMEOUT_MSEC := 30_000
const SOURCE_FILE := "core_sim/scenarios/kept_roads.json"
const SCRATCH_FILE := "core_sim/scenarios/zz_probe_scratch.json"
## Where `kept_roads.json` puts its one band (`factions[0].bands[0].at`).
const SCENARIO_BAND_X := 13
const SCENARIO_BAND_Y := 11
const POPULATIONS_KEY := "populations"
const TURN_KEY := "turn"
const NO_TURN := -1

var _main: Node = null
var _failures := 0
var _watchdog: Node = null


func _ready() -> void:
	_watchdog = get_node_or_null("Watchdog")
	_main = MAIN_SCENE.instantiate()
	add_child(_main)
	await _run()
	_finish()


func _fail(message: String) -> void:
	_failures += 1
	push_error("%s: FAIL — %s" % [TAG, message])


func _finish() -> void:
	if _watchdog != null and _watchdog.has_method("disarm"):
		_watchdog.call("disarm")
	var scratch := ProjectSettings.globalize_path("res://").path_join("../../" + SCRATCH_FILE).simplify_path()
	if FileAccess.file_exists(scratch):
		DirAccess.remove_absolute(scratch)
	if _failures > 0:
		print("%s: %d failure(s)." % [TAG, _failures])
		get_tree().quit(EXIT_FAILED)
		return
	print("%s: PASS — verbs answered, the world was swapped under the running client." % TAG)
	get_tree().quit(EXIT_OK)


func _snapshot() -> Dictionary:
	var loader: Object = _main.get("snapshot_loader")
	var last: Variant = loader.get("last_stream_snapshot") if loader != null else null
	return last if last is Dictionary else {}


func _await_until(what: String, timeout_msec: int, predicate: Callable) -> bool:
	var deadline := Time.get_ticks_msec() + timeout_msec
	if _watchdog != null and _watchdog.has_method("note_progress"):
		_watchdog.call("note_progress")
	while Time.get_ticks_msec() < deadline:
		if bool(predicate.call()):
			return true
		await get_tree().process_frame
	print("%s: timed out waiting for %s." % [TAG, what])
	return false


## One scenario verb through the Workbench's own service; returns the reply dictionary ({} on timeout).
func _ask(kind: String, args: Dictionary) -> Dictionary:
	var holder := {"reply": {}}
	var sent: bool = _main.call("_workbench_scenario_request", kind, args,
		func(reply: Dictionary) -> void: holder["reply"] = reply)
	if not sent:
		_fail("%s was not sent" % kind)
		return {}
	await _await_until("%s's answer" % kind, REPLY_TIMEOUT_MSEC,
		func() -> bool: return not (holder["reply"] as Dictionary).is_empty())
	return holder["reply"]


func _band_tiles() -> Array:
	var out: Array = []
	for row in _snapshot().get(POPULATIONS_KEY, []):
		if row is Dictionary and int(row.get("faction", -1)) == HudConst.PLAYER_FACTION_ID \
				and not bool(row.get("is_expedition", false)):
			out.append([int(row.get("current_x", -1)), int(row.get("current_y", -1))])
	return out


func _run() -> void:
	var revealed: bool = await _await_until("the world to arrive", REVEAL_TIMEOUT_MSEC,
		func() -> bool: return bool(_main.get("_world_revealed")))
	if not revealed:
		_fail("no world arrived")
		return
	var list := await _ask("list_scenarios", {})
	var files: Array = list.get("files", [])
	var schema: Variant = JSON.parse_string(String(list.get("schema", "")))
	print("%s: list ok=%s files=%d schema_is_object=%s" % [TAG, list.get("ok"), files.size(), schema is Dictionary])
	if not bool(list.get("ok", false)) or not files.has(SOURCE_FILE) or not (schema is Dictionary):
		_fail("list_scenarios did not answer the files and a schema")
		return
	var read := await _ask("read_scenario", {"path": SOURCE_FILE})
	var text := String(read.get("text", ""))
	if not bool(read.get("ok", false)) or not (JSON.parse_string(text) is Dictionary):
		_fail("read_scenario did not answer a JSON text")
		return
	var broken: Dictionary = (JSON.parse_string(text) as Dictionary).duplicate(true)
	broken["factions"][0]["bands"][0]["at"] = [0, 0]
	broken["world"]["width"] = 0
	var refused := await _ask("write_scenario", {"path": SCRATCH_FILE, "text": JSON.stringify(broken, "  ")})
	print("%s: broken write ok=%s refusals=%s" % [TAG, refused.get("ok"), refused.get("refusals")])
	if bool(refused.get("ok", true)) or (refused.get("refusals", []) as Array).is_empty():
		_fail("a broken scenario was not refused with refusals")
	var written := await _ask("write_scenario", {"path": SCRATCH_FILE, "text": text})
	print("%s: write ok=%s refusals=%s" % [TAG, written.get("ok"), written.get("refusals")])
	if not bool(written.get("ok", false)):
		_fail("a loadable scenario was refused")
		return

	var epoch_before := int(_main.get("_world_epoch_applied"))
	var turn_before := int(_snapshot().get(TURN_KEY, NO_TURN))
	var loaded := await _ask("load_scenario", {"path": SCRATCH_FILE})
	print("%s: load ok=%s refusals=%s epoch_before=%d" % [TAG, loaded.get("ok"), loaded.get("refusals"), epoch_before])
	if not bool(loaded.get("ok", false)):
		_fail("load_scenario was refused")
		return
	var swapped: bool = await _await_until("the client to take the new world", TURN_TIMEOUT_MSEC,
		func() -> bool: return int(_main.get("_world_epoch_applied")) != epoch_before)
	if not swapped:
		_fail("the world epoch never changed on the client — the swap did not reach Main's per-world reset")
		return
	var landed: bool = await _await_until("the scenario's band on the new frame", TURN_TIMEOUT_MSEC,
		func() -> bool: return _band_tiles() == [[SCENARIO_BAND_X, SCENARIO_BAND_Y]])
	print("%s: epoch %d -> %d, bands now at %s" % [TAG, epoch_before, int(_main.get("_world_epoch_applied")), _band_tiles()])
	if not landed:
		_fail("the client's frame does not show the scenario's band at (%d, %d): %s"
			% [SCENARIO_BAND_X, SCENARIO_BAND_Y, _band_tiles()])
		return
	var turn_after := int(_snapshot().get(TURN_KEY, NO_TURN))
	_main.call("_on_hud_next_turn", 1)
	var advanced: bool = await _await_until("a turn on the new world", TURN_TIMEOUT_MSEC,
		func() -> bool: return int(_snapshot().get(TURN_KEY, NO_TURN)) > turn_after)
	print("%s: turn %d (was %d on the old world) -> %d" % [TAG, turn_after, turn_before, int(_snapshot().get(TURN_KEY, NO_TURN))])
	if not advanced:
		_fail("the client took no turn on the loaded world")
