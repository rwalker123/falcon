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
##   3. A HIDDEN DELTA DOES NOT DISCHARGE THE CATCH-UP, AND THE CATCH-UP REPLAYS THE NEWEST FRAME —
##      whichever kind that is. The original bug: clearing `_hidden_snapshot_pending` on the delta
##      path made `_catch_up_hidden_snapshot` a no-op, so full→delta→show opened the panels holding
##      only what the delta carried. That is still asserted. What changed is the reason it is a bug:
##      it used to be "a delta is applied in full while hidden (never skippable)", and since delta
##      streaming (#386) BOTH kinds are skippable — see `DELTA_WHILE_HIDDEN` for why, and for what
##      replaces the old assertion.
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
## **Three roster sizes, all deliberately DIFFERENT** — 3 before the pair arrives, 5 in the full
## snapshot, 6 in the delta — because each wrong answer has to be distinguishable from the others.
## Reading back 3 after the show means the catch-up never ran; 5 means it replayed the OLDER frame;
## only 6 is the newest state.
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
## A **MERGED, COMPLETE** delta frame — the only kind the live client can now produce, and the
## reason this fixture's shape changed.
##
## It carries the wholesale `influencers` roster (6) *and* the sparse `influencer_updates` key,
## because a real merged frame carries both: the native decoder patches each base key from the
## delta's `*_updates` list and republishes the whole world, then also publishes the sparse list the
## panels' delta branch consumes (`native/src/snapshot/cache.rs`).
##
## **The history this replaces, so it cannot come back.** This fixture used to be a PARTIAL dict —
## `influencer_updates` only, no `influencers` — shaped like the server's between-turn on-demand
## feeds. Against that fixture the case asserted that a hidden delta was applied IN FULL (never
## skipped), because a partial frame describes a change against state the panels already hold and
## nothing later can reconstruct a dropped one. Delta streaming (#386) inverted that premise: the
## decoder maintains a cached world, so every frame is self-contained and the NEXT one reconstructs
## anything dropped. **Caching and replaying a partial frame is still unsafe — it is simply no
## longer producible**, so if a code path ever starts handing `update_delta` a partial dict again,
## the hidden skip in `Inspector._apply_update` must be reverted with it, not worked around here.
##
## `command_events` stays per-frame history (an accumulator, never reconstructible), so it is
## ingested while hidden whatever else is skipped — that is what cases 2 and 5's event counts pin.
const DELTA_WHILE_HIDDEN := {
	"turn": 4,
	"influencers": [
		{"id": 1, "name": "A", "scope": "Global"},
		{"id": 2, "name": "B", "scope": "Local"},
		{"id": 3, "name": "C", "scope": "Regional"},
		{"id": 4, "name": "D", "scope": "Global"},
		{"id": 5, "name": "E", "scope": "Regional"},
		{"id": 6, "name": "F", "scope": "Local"},
	],
	"influencer_updates": [{"id": 6, "name": "F", "scope": "Local"}],
	"command_events": [{"tick": 4, "kind": "order", "label": "fifth", "detail": ""}],
}

## Expected roster sizes at each checkpoint, named so an assertion reads as its intent.
const ROSTER_EMPTY := 0
const ROSTER_AFTER_CATCH_UP := 2
const ROSTER_WHILE_VISIBLE := 3
## While hidden, BOTH the full snapshot and the delta are skipped, so the roster is still whatever
## the last VISIBLE frame left. Stated as its own name (rather than reusing `ROSTER_WHILE_VISIBLE`
## at the call site) because it asserts a different thing: not "the visible path works" but "neither
## hidden frame reached the panels".
const ROSTER_UNCHANGED_WHILE_HIDDEN := ROSTER_WHILE_VISIBLE
## After the show, the catch-up must have replayed the NEWEST frame — the delta, not the full
## snapshot before it. Distinct from both `ROSTER_UNCHANGED_WHILE_HIDDEN` (catch-up never ran) and
## the full snapshot's 5 (catch-up replayed the older frame).
const ROSTER_AFTER_DELTA_CATCH_UP := 6
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

	# --- Hidden, full THEN delta: skip both, then catch up to the NEWEST -------------------------
	# `_hidden_snapshot_pending` means "a frame has arrived that the panels have not ingested". The
	# original bug was clearing it on the delta path: `_catch_up_hidden_snapshot` became a no-op and
	# the panels opened holding only what the delta carried — stale, and self-healing on the next
	# turn, which is why neither review nor the first version of this guard caught it.
	#
	# Since delta streaming both kinds are skipped and `_cached_snapshot` holds the newest of
	# either, so the three assertions below pin three different things: that the delta was skipped
	# too (the new one), that the catch-up still ran, and that it replayed the NEWEST frame rather
	# than the older full snapshot. The last subsumes the original assertion.
	inspector.set_panel_visible(false)
	inspector.update_snapshot(SNAPSHOT_HIDDEN_BEFORE_DELTA.duplicate(true))
	inspector.update_delta(DELTA_WHILE_HIDDEN.duplicate(true))
	_expect_size(roster.get_influencers(), ROSTER_UNCHANGED_WHILE_HIDDEN,
		"the tab-panel fan-out RAN for a DELTA received while the Inspector was hidden — since delta streaming a merged delta frame is a complete world and is skippable exactly like a full snapshot, and that skip is ~60% of the client's per-turn apply cost")
	_expect_size(inspector._seen_command_events, EVENTS_AFTER_HIDDEN_DELTA,
		"a command event carried by a hidden full snapshot or delta was not ingested — that history is unrecoverable, so the accumulator must run whatever else is skipped")

	inspector.set_panel_visible(true)
	_expect_size(roster.get_influencers(), ROSTER_AFTER_DELTA_CATCH_UP,
		"showing the Inspector after a hidden full snapshot AND a hidden delta did not leave the panels holding the NEWEST frame — a roster of %d means the catch-up never ran (a hidden delta wrongly discharged `_hidden_snapshot_pending`), and one of %d means it replayed the older full snapshot instead of the delta after it (`_cached_snapshot` is not being written on the delta path)" % [ROSTER_UNCHANGED_WHILE_HIDDEN, SNAPSHOT_HIDDEN_BEFORE_DELTA["influencers"].size()])
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
