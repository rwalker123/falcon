extends Node

## Headless regression guard for the HIDDEN-INSPECTOR SKIP (`Inspector._apply_update`).
##
## The Inspector ships hidden (`Main` calls `set_panel_visible(false)` at startup; `I` re-opens it)
## and used to run its whole tab-panel fan-out every turn anyway — 113 ms per turn on an 80×52 map,
## 61 % of the client's per-turn apply cost, spent rendering a panel nobody could see. The fix skips
## that work while hidden and replays the newest full snapshot when the panel is shown.
##
## Skipping work is only safe because of two properties, and BOTH are silent when broken — a
## regression shows up as stale numbers or a missing log line, never as an error. So both are
## asserted here:
##
##   1. CATCH-UP — after being shown, the panels hold the data from the LATEST snapshot, not
##      whatever they had when they were hidden. (Break it by dropping `_catch_up_hidden_snapshot`
##      or the `_cached_snapshot` write.)
##   2. ACCUMULATORS STILL RUN — `_ingest_command_events` builds a running log out of per-turn
##      EVENT arrays, which no later snapshot carries. Skipping it while hidden would silently lose
##      command-feed history. (Break it by moving the visibility gate above that call.)
##
## It also asserts the skip actually happens (3) — otherwise the guard would keep passing after the
## optimization was reverted — and that replaying a snapshot does not double-log its events (4).
##
## Run as a scene (NOT --script: the Inspector's panels reach autoloads that only register when the
## project is loaded). No GPU needed — this is state plumbing, not rendering:
##   godot --headless --path . res://tools/inspector_hidden_guard.tscn
## Exits 0 on PASS, 1 on FAIL (CI-usable).

const INSPECTOR_LAYER := preload("res://src/ui/InspectorLayer.tscn")

## Snapshot fixtures. `influencers` is the observable: it is full-snapshot-driven, rebuilt wholesale
## by `InfluencerPanel._rebuild_influencers`, and readable through the panel's public
## `get_influencers()` — so its size is a direct, un-mocked witness of whether the fan-out ran.
## `command_events` is the accumulator witness (one NEW event per snapshot; the coordinator dedupes
## on tick|kind|label|detail, which is what makes the catch-up replay safe).
const SNAPSHOT_HIDDEN_FIRST := {
	"turn": 1,
	"influencers": [{"id": 1, "name": "A", "scope": "Global"}],
	"command_events": [{"tick": 1, "kind": "order", "label": "first", "detail": ""}],
}
const SNAPSHOT_HIDDEN_SECOND := {
	"turn": 2,
	"influencers": [{"id": 1, "name": "A", "scope": "Global"}, {"id": 2, "name": "B", "scope": "Local"}],
	"command_events": [{"tick": 2, "kind": "order", "label": "second", "detail": ""}],
}
const SNAPSHOT_WHILE_VISIBLE := {
	"turn": 3,
	"influencers": [
		{"id": 1, "name": "A", "scope": "Global"},
		{"id": 2, "name": "B", "scope": "Local"},
		{"id": 3, "name": "C", "scope": "Regional"},
	],
	"command_events": [{"tick": 3, "kind": "order", "label": "third", "detail": ""}],
}

## Expected roster sizes at each checkpoint, named so an assertion reads as its intent.
const ROSTER_EMPTY := 0
const ROSTER_AFTER_CATCH_UP := 2
const ROSTER_WHILE_VISIBLE := 3
## Distinct command events the coordinator must have logged at each checkpoint.
const EVENTS_AFTER_FIRST_HIDDEN := 1
const EVENTS_AFTER_CATCH_UP := 2
const EVENTS_AFTER_VISIBLE := 3

var _failures: Array[String] = []

func _ready() -> void:
	var inspector: Node = INSPECTOR_LAYER.instantiate()
	add_child(inspector)
	await get_tree().process_frame

	var roster: Node = inspector.influencer_panel
	if roster == null:
		_fail("InspectorLayer has no influencer_panel — the guard's observable is gone; pick another full-snapshot-driven panel.")
		_finish(inspector)
		return

	# --- Hidden: the fan-out is skipped, the accumulator is not ---------------------------------
	inspector.set_panel_visible(false)
	inspector.update_snapshot(SNAPSHOT_HIDDEN_FIRST.duplicate(true))
	_expect_size(roster.get_influencers(), ROSTER_EMPTY,
		"the tab-panel fan-out RAN while the Inspector was hidden — the per-turn skip is gone")
	_expect_size(inspector._seen_command_events, EVENTS_AFTER_FIRST_HIDDEN,
		"a command event that arrived while hidden was NOT ingested — the accumulator is behind the visibility gate, and that history is unrecoverable")

	# A second hidden snapshot: catch-up must land on the LATEST one, not the first.
	inspector.update_snapshot(SNAPSHOT_HIDDEN_SECOND.duplicate(true))
	_expect_size(roster.get_influencers(), ROSTER_EMPTY,
		"the fan-out ran on the second hidden snapshot")

	# --- Shown: catch-up replays the newest snapshot --------------------------------------------
	inspector.set_panel_visible(true)
	_expect_size(roster.get_influencers(), ROSTER_AFTER_CATCH_UP,
		"showing the Inspector did NOT catch it up to the latest snapshot — the panel is displaying stale data")
	_expect_size(inspector._seen_command_events, EVENTS_AFTER_CATCH_UP,
		"the catch-up replay double-logged (or dropped) command events — the _seen_command_events dedupe is what makes replay safe")

	# --- Visible: the ordinary path is untouched -------------------------------------------------
	inspector.update_snapshot(SNAPSHOT_WHILE_VISIBLE.duplicate(true))
	_expect_size(roster.get_influencers(), ROSTER_WHILE_VISIBLE,
		"a snapshot applied while VISIBLE did not reach the panels — the gate is firing when it must not")
	_expect_size(inspector._seen_command_events, EVENTS_AFTER_VISIBLE,
		"a command event applied while visible was not ingested")

	_finish(inspector)

func _expect_size(collection: Variant, want: int, why: String) -> void:
	var got: int = -1
	if collection is Dictionary:
		got = (collection as Dictionary).size()
	elif collection is Array:
		got = (collection as Array).size()
	else:
		_fail("%s (got %s, not a Dictionary/Array)" % [why, type_string(typeof(collection))])
		return
	if got != want:
		_fail("%s (size %d, expected %d)" % [why, got, want])

func _fail(msg: String) -> void:
	_failures.append(msg)

func _finish(inspector: Node) -> void:
	inspector.queue_free()
	if _failures.is_empty():
		print("inspector_hidden_guard: PASS — a hidden Inspector skips the fan-out, keeps ingesting command events, and shows current data when re-opened")
		get_tree().quit(0)
	else:
		printerr("inspector_hidden_guard: FAIL — %d problem(s):" % _failures.size())
		for msg in _failures:
			printerr("  - ", msg)
		get_tree().quit(1)
