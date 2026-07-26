extends RefCounted
class_name TurnProfile

## Per-snapshot wall-clock profiler for the CLIENT half of a turn.
##
## The server already reports its own breakdown on the `turn.profile` event (`core_sim/src/
## turn_profile.rs`); this is the mirror for everything that happens after the bytes arrive —
## the FlatBuffers→Dictionary decode, `MapView.display_snapshot`, the HUD fan-out, the Inspector.
## The two halves deliberately render in the SAME flat `label=<milliseconds>` shape so a client
## line and a server line can be read (and grepped) side by side:
##
##     [TurnProfile] apply=48.20 decode=12.10(x3 discarded 2) display=31.40 display.tiles=14.20 …
##
## Like the server's, the label list is FLAT and a parent INCLUDES its children: `display` contains
## every `display.*` entry. The dotted names carry the hierarchy; the numbers are not a partition
## and are not expected to sum to `apply`.
##
## # Ordering
##
## Entries render in the order each label was first OPENED ([`begin`]/[`record_ms`]), not the order
## they closed — which is what keeps `apply` (opened first, closed last) at the head of the line and
## a `display.*` child behind its `display` parent.
##
## # Always constructible, conditionally noisy
##
## [`start`] hands back an instance whether or not profiling is on, and a disabled instance's
## `begin`/`end`/`record_ms` are no-ops that cost one bool test. Call sites therefore stay
## unconditional — no `if profiling:` straddle smeared across three scripts — and only [`emit`]
## consults the flag. A client console line per turn is real noise in normal play, so unlike the
## server (always on, an operator's log is private) this half is gated.

## Environment variable that turns the per-turn client profile OFF.
##
## Set it to any of [`DISABLED_VALUES`] before launching the client (`SHADOW_SCALE_CLIENT_PROFILE=0
## godot …`, or export it from `scripts/run_stack.sh`'s environment) to silence both this line and
## `MapView`'s rolling `_draw` average, which reads the same flag.
const ENV_FLAG := "SHADOW_SCALE_CLIENT_PROFILE"

## Values of [`ENV_FLAG`] read as "off". Anything else — including an unset or empty variable —
## leaves profiling at [`DEFAULT_ENABLED`].
const DISABLED_VALUES: PackedStringArray = ["0", "false", "off", "no"]

## What an UNSET [`ENV_FLAG`] means. `true` while issue #384's client-side turn cost is being
## measured; flipping the shipped default is this one constant, and the env var keeps working
## in whichever direction it then has to.
const DEFAULT_ENABLED := true

## Grep handle shared by every line this module (and `MapView._draw`) prints.
const LINE_PREFIX := "[TurnProfile]"

## `Time.get_ticks_usec()` is microseconds; every rendered figure is milliseconds, matching the
## server's `turn.profile`.
const USEC_PER_MSEC := 1000.0

## `label=<ms>` at two decimals — enough to tell a tenth-of-a-millisecond phase from a free one
## without burying the line in digits (the server's `PHASE_MS_PRECISION`, same reasoning).
const ENTRY_FORMAT := "%s=%.2f"

## Free-form annotation appended to an entry, e.g. `decode=12.10(x3 discarded 2)`.
const NOTE_FORMAT := "(%s)"

## `_index_of` answer for a label that has not been opened yet.
const MISSING_INDEX := -1

## Tri-state cache for the env lookup: resolved once per launch, since the environment cannot
## change under a running client.
const ENABLED_UNRESOLVED := -1
const ENABLED_NO := 0
const ENABLED_YES := 1

static var _enabled_state: int = ENABLED_UNRESOLVED

## Whether this instance records and emits. Frozen at construction so one applied snapshot cannot
## render half a line.
var enabled: bool = false

# Parallel arrays rather than an Array[Dictionary]: a profile holds a dozen entries and is rebuilt
# every turn, so the linear `_index_of` scan is far cheaper than the per-entry allocation would be.
var _labels := PackedStringArray()
var _millis := PackedFloat64Array()
var _notes := PackedStringArray()

## Whether the per-turn client profile is on for this launch (see [`ENV_FLAG`]).
static func is_enabled() -> bool:
	if _enabled_state == ENABLED_UNRESOLVED:
		_enabled_state = ENABLED_YES if _resolve_enabled() else ENABLED_NO
	return _enabled_state == ENABLED_YES

## Test/tooling seam: drop the cached flag so the next lookup re-reads the environment.
static func reset_flag_cache() -> void:
	_enabled_state = ENABLED_UNRESOLVED

## A fresh profile for one applied snapshot, live or inert per [`is_enabled`].
static func start() -> TurnProfile:
	var profile := TurnProfile.new()
	profile.enabled = is_enabled()
	return profile

static func _resolve_enabled() -> bool:
	var raw: String = OS.get_environment(ENV_FLAG).strip_edges().to_lower()
	if raw == "":
		return DEFAULT_ENABLED
	return not DISABLED_VALUES.has(raw)

## Open `label` — reserving its place in the rendered order — and return the start stamp to hand
## back to [`end`]. Returns 0 when disabled; `end` ignores it either way.
func begin(label: String) -> int:
	if not enabled:
		return 0
	_reserve(label)
	return Time.get_ticks_usec()

## Fold the time since `start_usec` into `label`, optionally annotating it (`note` renders
## parenthesised after the figure). Re-ending the same label ACCUMULATES, matching the server.
func end(label: String, start_usec: int, note: String = "") -> void:
	if not enabled:
		return
	record_ms(label, float(Time.get_ticks_usec() - start_usec) / USEC_PER_MSEC, note)

## Record an already-measured duration — for a phase timed somewhere else (the native decoder
## reports microseconds of its own, and `SnapshotLoader` sums a poll's frames before handing the
## total over).
func record_ms(label: String, millis: float, note: String = "") -> void:
	if not enabled:
		return
	var index: int = _reserve(label)
	_millis[index] += millis
	if note != "":
		_notes[index] = note

## Splice another profile's entries in behind this one's, each label prefixed (`"display."` turns
## `MapView`'s `tiles` into `display.tiles`). Used to let `MapView` time its own internals without
## knowing anything about the line `Main` assembles.
func absorb(other: TurnProfile, prefix: String) -> void:
	if not enabled or other == null:
		return
	for i in other._labels.size():
		record_ms(prefix + other._labels[i], other._millis[i], other._notes[i])

func is_empty() -> bool:
	return _labels.is_empty()

## The flat `label=ms label=ms …` body, without the [`LINE_PREFIX`].
func render() -> String:
	var parts := PackedStringArray()
	for i in _labels.size():
		var part: String = ENTRY_FORMAT % [_labels[i], _millis[i]]
		if _notes[i] != "":
			part += NOTE_FORMAT % _notes[i]
		parts.append(part)
	return " ".join(parts)

## Print the assembled line. Silent when disabled or when nothing was recorded.
func emit() -> void:
	if not enabled or is_empty():
		return
	print("%s %s" % [LINE_PREFIX, render()])

## Index of `label`, appending a zeroed slot the first time it is seen — which is what stamps the
## rendered order at OPEN time.
func _reserve(label: String) -> int:
	var index: int = _index_of(label)
	if index != MISSING_INDEX:
		return index
	_labels.append(label)
	_millis.append(0.0)
	_notes.append("")
	return _labels.size() - 1

func _index_of(label: String) -> int:
	for i in _labels.size():
		if _labels[i] == label:
			return i
	return MISSING_INDEX
