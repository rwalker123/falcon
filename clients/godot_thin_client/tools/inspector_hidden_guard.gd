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
##   3. A HIDDEN DELTA DOES NOT DISCHARGE THE CATCH-UP — a delta is applied in full while hidden
##      (never skippable), but it does not mean the pending full snapshot has been ingested. Clear
##      `_hidden_snapshot_pending` on the delta path and full→delta→show opens the panels holding
##      only the delta's data. (Break it by clearing that flag outside `if full_snapshot:`.)
##
## It also asserts the skip actually happens (4) — otherwise the guard would keep passing after the
## optimization was reverted — and that replaying a snapshot does not double-log its events (5).
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
## Case 5 fixtures — a full snapshot skipped while hidden, then a DELTA while still hidden.
##
## The two roster sizes are deliberately DIFFERENT so the assertion can fail. The delta is applied
## in full (deltas are never skipped) and merges ONE influencer onto the roster the panels already
## hold from `SNAPSHOT_WHILE_VISIBLE` (3 + 1 = 4); only replaying this full snapshot rebuilds the
## roster wholesale to 5. So a delta that wrongly discharges `_hidden_snapshot_pending` — skipping
## the catch-up — reads back 4, not 5.
const SNAPSHOT_HIDDEN_BEFORE_DELTA := {
	"turn": 4,
	"influencers": [
		{"id": 1, "name": "A", "scope": "Global"},
		{"id": 2, "name": "B", "scope": "Local"},
		{"id": 3, "name": "C", "scope": "Regional"},
		{"id": 4, "name": "D", "scope": "Global"},
		{"id": 5, "name": "E", "scope": "Regional"},
	],
	"command_events": [{"tick": 4, "kind": "order", "label": "fourth", "detail": ""}],
}
## Shaped like the server's between-turn on-demand feeds (`update_influencers` /
## `update_command_events`), which `Main._snapshot_is_delta` routes to `update_delta` — the live
## path on which a delta reaches a hidden Inspector.
const DELTA_WHILE_HIDDEN := {
	"turn": 4,
	"influencer_updates": [{"id": 6, "name": "F", "scope": "Local"}],
	"command_events": [{"tick": 4, "kind": "order", "label": "fifth", "detail": ""}],
}

## Expected roster sizes at each checkpoint, named so an assertion reads as its intent.
const ROSTER_EMPTY := 0
const ROSTER_AFTER_CATCH_UP := 2
const ROSTER_WHILE_VISIBLE := 3
## While hidden the delta alone is applied, onto the roster left by `SNAPSHOT_WHILE_VISIBLE`.
const ROSTER_AFTER_HIDDEN_DELTA := 4
## After the show, the replayed full snapshot must have rebuilt the roster wholesale.
const ROSTER_AFTER_DELTA_CATCH_UP := 5
## Distinct command events the coordinator must have logged at each checkpoint.
const EVENTS_AFTER_FIRST_HIDDEN := 1
const EVENTS_AFTER_CATCH_UP := 2
const EVENTS_AFTER_VISIBLE := 3
## The hidden full snapshot's event, then the hidden delta's; the replay on show re-presents the
## former and must dedupe it.
const EVENTS_AFTER_HIDDEN_DELTA := 5

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

	# --- Hidden, full THEN delta: the delta must not discharge the pending replay ----------------
	# `_hidden_snapshot_pending` means "a full snapshot arrived that the panels have not ingested".
	# A delta changes panel state but does not pay off that debt, and deltas do reach a hidden
	# Inspector live (`Main._snapshot_is_delta` routes the server's between-turn on-demand feeds).
	# Clearing the flag on the delta path made `_catch_up_hidden_snapshot` a no-op, so the panels
	# opened holding only what the delta carried — stale, and self-healing on the next turn's full
	# snapshot, which is why neither review nor the first version of this guard caught it.
	inspector.set_panel_visible(false)
	inspector.update_snapshot(SNAPSHOT_HIDDEN_BEFORE_DELTA.duplicate(true))
	inspector.update_delta(DELTA_WHILE_HIDDEN.duplicate(true))
	_expect_size(roster.get_influencers(), ROSTER_AFTER_HIDDEN_DELTA,
		"a DELTA that arrived while hidden was not applied in full — deltas are never skippable, and nothing later reconstructs a dropped one")
	_expect_size(inspector._seen_command_events, EVENTS_AFTER_HIDDEN_DELTA,
		"a command event carried by a hidden full snapshot or delta was not ingested")

	inspector.set_panel_visible(true)
	_expect_size(roster.get_influencers(), ROSTER_AFTER_DELTA_CATCH_UP,
		"a delta received while hidden CANCELLED the catch-up replay of the full snapshot before it — the Inspector opened holding only the delta's data, which is the stale-when-opened failure the replay exists to prevent")
	_expect_size(inspector._seen_command_events, EVENTS_AFTER_HIDDEN_DELTA,
		"the catch-up replay after a hidden delta double-logged (or dropped) command events")

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
