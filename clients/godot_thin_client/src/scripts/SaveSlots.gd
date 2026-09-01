extends RefCounted
class_name SaveSlots

## **THE SAVE CHANNEL SEAM** — the client's half of `list_saves` / `save_game` / `load_game` /
## `delete_save` (`.claude/rules/core_sim/save-game.md`).
##
## Modelled on `ForecastQuery`, and for the same reasons: every one of these four asks is a round trip
## whose answer arrives on the command socket's second direction rather than in a snapshot, so a
## caller needs a request id, a rule for which reply is still wanted, something honest to render while
## waiting, and a signal when the answer lands. Two copies of that would drift.
##
## **IT OWNS NO SOCKET.** The owner injects a sender and pumps the replies in (`set_sender` /
## `deliver`) — the coordinator-mediation rule. `MenuShell` is a view: it reads this seam and emits
## signals, and it never reaches the network itself. That also makes every state below drivable from
## a harness with no server at all, which is exactly how `menu_preview` renders them.
##
## **A SAVE VERB TRIGGERS NO SNAPSHOT EITHER.** A save and a delete change no world; a load changes it
## completely, but the client learns *that* from the reveal gate rather than from this reply. So the
## reply IS the render input for the pane, and nothing here may wait on a frame.

## The slot list changed — landed, failed, or a save/delete invalidated it.
signal slots_changed

## One save / load / delete finished. `drift` is the config-drift rows, populated only on a
## successful load and empty in the good case (`.claude/rules/core_sim/save-game.md`).
signal op_finished(kind: String, slot: String, ok: bool, error: String, drift: Array)

# ---- the four asks, spelled as `bridge/query.rs` matches them -------------------------------------
const KIND_LIST := "list_saves"
const KIND_SAVE := "save_game"
const KIND_LOAD := "load_game"
const KIND_DELETE := "delete_save"

## **THE RESERVED SLOT.** Mirrors `sim_runtime::commands::AUTOSAVE_SLOT`. The server refuses an
## explicit save naming it; the UI refuses it *first*, so the promise the pane makes ("cannot be
## overwritten by hand") is kept by the affordance rather than discovered through an error.
const AUTOSAVE_SLOT := "autosave"

## Mirrors `core_sim`'s `MAX_SLOT_NAME_LEN`. A slot name becomes a filename, so the server whitelists
## letters, digits, space, `-` and `_` up to this length; the LineEdit enforces the same whitelist so
## a rejected keystroke never becomes a rejected save.
const MAX_SLOT_NAME_LEN := 64

## The one extra character class the whitelist admits beyond letters and digits.
const SLOT_NAME_EXTRA_CHARS := " -_"

# ---- list state ----------------------------------------------------------------------------------

## Nothing has been asked yet — the pane has not been opened this session.
const LIST_IDLE := "idle"
## A `list_saves` is in flight. Render the waiting line, never an empty list: "no saves yet" and
## "we have not asked yet" are different facts and only one of them is a dead end.
const LIST_PENDING := "pending"
## The list landed. It may still be empty, which is the genuine "no saves yet".
const LIST_READY := "ready"
## The ask failed or was refused. `list_error` carries the token.
const LIST_FAILED := "failed"

## **THE SAVE CHANNEL'S REQUEST-ID SPACE, DISJOINT FROM `ForecastQuery`'s BY CONSTRUCTION.**
##
## Both seams are fed from the SAME native drain (`CommandBridge.poll_query_replies`) and both route
## a reply by its `request_id`, so two counters that each started at 1 would each answer the other's
## replies — a forecast landing in the save pane, or worse, a `save_op` landing on a compose sheet.
## Ids are `u64` on the wire, `ForecastQuery` counts up from 1, and every id this seam spends sits at
## or above this floor: a collision would need four billion forecasts in one session, and there is no
## coordination to forget.
const REQUEST_ID_BASE := 1 << 40

## **ONE BLOCK OF IDS PER SEAM INSTANCE, so an id cannot be REUSED ACROSS A SCENE CHANGE.**
##
## The worker's answer channel is process-global (`QUERY_ANSWERS`, an `OnceLock` in `bridge/query.rs`)
## and outlives every scene; a `SaveSlots` does not. A load swaps the scene — `LandingScreen`'s
## `change_scene_to_file`, `Main._on_pause_load`'s `reload_current_scene` — and the world that comes
## up builds a NEW seam. An ask still in flight across that swap is drained by the new scene and
## offered to it, and `list_saves` is exactly such an ask: `refresh()` is not gated on `is_busy()` and
## `_finish_op` re-arms it after every save and delete.
##
## Were every instance to restart at `REQUEST_ID_BASE`, that stale list answer would carry the id the
## new seam had just spent on `load_game`. `_deliver_one` would read the LIST reply's `ok: true`,
## erase the id and finish the LOAD as a success — the drift notice never raised, the loading overlay
## never re-worded, the retry latch left set, and a refused load reported to the player as a
## completed one. So the blocks are made disjoint by construction rather than by hoping the swap is
## quiet.
##
## Ids reserved to one instance. A seam asks a handful — one per pane open, one per verb — so the
## block cannot be walked out of; and the block index below stays small enough that the product is
## nowhere near the `u64` the wire carries.
const IDS_PER_SESSION := 1 << 16

## **THE BLOCK INDEX IS THE MONOTONIC MICROSECOND CLOCK, plus a tie-break count.** The clock cannot
## repeat within a process and — unlike a `static var`, whose lifetime is the SCRIPT's rather than the
## process's — is not reset by a scene change. The counter separates two seams built inside the same
## microsecond, which the clock alone cannot; because both terms are non-decreasing and the counter
## rises by one on every call, successive offsets are strictly increasing whatever the clock does.
static var _sessions_started := 0

## The id of a request that was never made.
const NO_REQUEST_ID := 0

# ---- error tokens (mirrors `sim_runtime::commands::save_error` + the bridge's transport token) ----
const ERROR_TRANSPORT := "transport"
const ERROR_NO_ACTIVE_WORLD := "no_active_world"
const ERROR_INVALID_SLOT := "invalid_slot"
const ERROR_RESERVED_SLOT := "reserved_slot"
const ERROR_NO_SUCH_SLOT := "no_such_slot"
const ERROR_IO_FAILED := "io_failed"
const ERROR_UNREADABLE := "unreadable"

## **ONE SENTENCE PER TOKEN, because every one of them is reachable by a player.** Unlike the forecast
## seam — whose refusals are all client bugs and share a single terse line — these name things the
## player did (named the autosave slot, picked a save this build cannot read) or things about the
## machine (a full disk, a server that is not running). A token on screen would be useless in exactly
## those cases.
const ERROR_PROSE := {
	ERROR_TRANSPORT: "No answer from the simulation server. Is it running?",
	ERROR_NO_ACTIVE_WORLD: "There is no world to save yet.",
	ERROR_INVALID_SLOT: "That name cannot be used for a save file.",
	ERROR_RESERVED_SLOT: "The autosave slot is written by the game and cannot be saved over.",
	ERROR_NO_SUCH_SLOT: "That save is no longer on disk.",
	ERROR_IO_FAILED: "The save file could not be read or written.",
	ERROR_UNREADABLE: "That save was written by a different build and cannot be read.",
}

## Shown for a token this build does not know — a server ahead of the client. The token rides along
## because it is the only thing that says which.
const ERROR_PROSE_UNKNOWN := "The server refused (%s)."

# ---- human units ---------------------------------------------------------------------------------
const BYTES_PER_KB := 1024.0
const KB_PER_MB := 1024.0
## Below this a size reads in KB; at or above it, in MB with one decimal. A save is ~1.2 MB.
const MB_THRESHOLD_BYTES := 1024.0 * 1024.0
const SIZE_KB_FORMAT := "%d KB"
const SIZE_MB_FORMAT := "%.1f MB"

const SECONDS_PER_MINUTE := 60
const SECONDS_PER_HOUR := 3600
const SECONDS_PER_DAY := 86400
## Past a week a relative age stops being useful ("53 days ago" is not a date), so the row switches to
## an absolute local date-time.
const RELATIVE_LIMIT_DAYS := 7
const WHEN_JUST_NOW := "just now"
const WHEN_MINUTES_FORMAT := "%d min ago"
const WHEN_HOURS_FORMAT := "%d h ago"
const WHEN_DAYS_FORMAT := "%d days ago"
const WHEN_YESTERDAY := "yesterday"
const WHEN_ABSOLUTE_FORMAT := "%d %s, %02d:%02d"
const WHEN_UNKNOWN := "date unknown"
const MONTH_NAMES := ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
	"Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
## Minutes → seconds, for the system time-zone bias an absolute stamp has to be rendered in.
const SECONDS_PER_MINUTE_F := 60.0

# ---- state ---------------------------------------------------------------------------------------
var _sender: Callable = Callable()
## The next id this seam will spend, inside the block it was handed at construction.
var _next_request_id: int = REQUEST_ID_BASE
## request_id -> kind, for the asks that have not been answered yet.
var _inflight: Dictionary = {}

var list_state: String = LIST_IDLE
var list_error: String = ""
## The rows from the last successful `list_saves`, newest first (the server's order, kept).
var slots: Array = []

## The kind of the save/load/delete currently in flight (`""` when none). One at a time: these are
## button presses on a modal-feeling pane, and a second press while the first is unanswered would
## race two writes at the same file.
var op_in_flight: String = ""
var op_slot: String = ""


func _init() -> void:
	_next_request_id = _new_session_start()


## The first id of a fresh block, and the reason every seam gets one is at `IDS_PER_SESSION`.
static func _new_session_start() -> int:
	_sessions_started += 1
	return REQUEST_ID_BASE + (Time.get_ticks_usec() + _sessions_started) * IDS_PER_SESSION


## Inject the transport. `sender` is `func(request_id: int, ask: Dictionary) -> bool` — true when the
## ask reached the socket. Nothing is sent until one is set, so a pane standing up before its owner
## has a command client simply asks nothing and renders `LIST_IDLE`.
func set_sender(sender: Callable) -> void:
	_sender = sender


## **ASK FOR THE LIST.** Called when the pane opens and after any op that changes what is on disk.
## A refresh while one is already in flight is dropped rather than queued — the answer in flight is
## the answer to the same question.
func refresh() -> void:
	if list_state == LIST_PENDING:
		return
	list_state = LIST_PENDING
	list_error = ""
	slots_changed.emit()
	if not _dispatch(KIND_LIST, ""):
		list_state = LIST_FAILED
		list_error = ERROR_TRANSPORT
		slots_changed.emit()


## Write the run to `slot`. Refused locally when the name is invalid or reserved, so the server's
## rules are enforced by the affordance rather than reported after the fact.
func request_save(slot: String) -> bool:
	return _request_op(KIND_SAVE, slot)


## Replace the running world with `slot`. **The caller owns the world handoff** — the reply says only
## whether the load was accepted; which world the client then shows is the reveal gate's answer
## (`.claude/rules/core_sim/world-handoff.md`).
func request_load(slot: String) -> bool:
	return _request_op(KIND_LOAD, slot)


## Remove `slot` from disk. The autosave slot is refused here too: it is the one save a player cannot
## re-make by hand, so an accidental delete of it is unrecoverable in a way the others are not.
func request_delete(slot: String) -> bool:
	return _request_op(KIND_DELETE, slot)


## Is an op still unanswered? The pane disables its buttons on this rather than tracking its own flag.
func is_busy() -> bool:
	return op_in_flight != ""


## Pump the native drain in. Called once a frame by the owner; ids that are not ours are ignored, so
## the same array can be handed to `ForecastQuery` as well.
func deliver(replies: Array) -> void:
	for reply_variant in replies:
		if reply_variant is Dictionary:
			_deliver_one(reply_variant as Dictionary)


# ---- validation ----------------------------------------------------------------------------------

## **WHY A NAME IS REFUSED, in the player's words — or `""` when it is fine.** The same whitelist the
## server applies (`validate_slot_name`), applied before the ask so a bad name is a caption under the
## field and not a round trip that comes back `invalid_slot`.
static func slot_name_error(name: String) -> String:
	var trimmed := name.strip_edges()
	if trimmed.is_empty():
		return "Give the save a name."
	if trimmed.length() > MAX_SLOT_NAME_LEN:
		return "Names are at most %d characters." % MAX_SLOT_NAME_LEN
	if trimmed.to_lower() == AUTOSAVE_SLOT:
		return ERROR_PROSE[ERROR_RESERVED_SLOT]
	for i in trimmed.length():
		if not _is_slot_char(trimmed[i]):
			return "Letters, digits, spaces, - and _ only."
	return ""


## One character of the whitelist. Spelled as an allow-list, exactly as the server spells it: the
## point is that `..`, `/`, `\`, a drive letter and every control character are refused by one rule
## rather than by a list of the traversal spellings anyone thought of.
static func _is_slot_char(ch: String) -> bool:
	if ch.length() != 1:
		return false
	var code := ch.unicode_at(0)
	if (code >= 48 and code <= 57) or (code >= 65 and code <= 90) or (code >= 97 and code <= 122):
		return true
	return SLOT_NAME_EXTRA_CHARS.contains(ch)


## Is this row the slot the game writes on its own cadence? Rows say so, and the verbs refuse it.
static func is_reserved(slot: String) -> bool:
	return slot.to_lower() == AUTOSAVE_SLOT


# ---- formatting ----------------------------------------------------------------------------------

## A blob size in the unit a player reads. A save is ~1.2 MB, so MB with one decimal is the working
## unit and KB is the tail for a tiny world.
static func format_size(size_bytes: int) -> String:
	if size_bytes >= int(MB_THRESHOLD_BYTES):
		return SIZE_MB_FORMAT % (float(size_bytes) / BYTES_PER_KB / KB_PER_MB)
	return SIZE_KB_FORMAT % int(round(float(size_bytes) / BYTES_PER_KB))


## **HOW LONG AGO, or the date once "ago" stops meaning anything.** `0` is the server's "the
## filesystem would not say", which is a different fact from "just now" and must not render as one.
static func format_when(modified_unix_seconds: int) -> String:
	if modified_unix_seconds <= 0:
		return WHEN_UNKNOWN
	var now := int(Time.get_unix_time_from_system())
	var age := now - modified_unix_seconds
	if age < 0:
		# A clock skew, or a save written by another machine. "In the future" is not a useful thing
		# to tell a player, so it reads as the freshest bucket.
		age = 0
	if age < SECONDS_PER_MINUTE:
		return WHEN_JUST_NOW
	if age < SECONDS_PER_HOUR:
		return WHEN_MINUTES_FORMAT % (age / SECONDS_PER_MINUTE)
	if age < SECONDS_PER_DAY:
		return WHEN_HOURS_FORMAT % (age / SECONDS_PER_HOUR)
	var days := age / SECONDS_PER_DAY
	if days == 1:
		return WHEN_YESTERDAY
	if days < RELATIVE_LIMIT_DAYS:
		return WHEN_DAYS_FORMAT % days
	var bias_seconds := int(Time.get_time_zone_from_system().get("bias", 0)) * int(SECONDS_PER_MINUTE_F)
	var when := Time.get_datetime_dict_from_unix_time(modified_unix_seconds + bias_seconds)
	var month_index: int = clampi(int(when.get("month", 1)) - 1, 0, MONTH_NAMES.size() - 1)
	return WHEN_ABSOLUTE_FORMAT % [
		int(when.get("day", 1)),
		MONTH_NAMES[month_index],
		int(when.get("hour", 0)),
		int(when.get("minute", 0)),
	]


## The player-facing sentence for a refusal token.
static func error_prose(token: String) -> String:
	if ERROR_PROSE.has(token):
		return String(ERROR_PROSE[token])
	return ERROR_PROSE_UNKNOWN % token


# ---- internals -----------------------------------------------------------------------------------

func _request_op(kind: String, slot: String) -> bool:
	if is_busy():
		return false
	if kind != KIND_LOAD:
		# A LOAD of the autosave slot is exactly what the rolling backup is FOR; only writing to it
		# and deleting it are refused.
		if is_reserved(slot):
			_finish_op(kind, slot, false, ERROR_RESERVED_SLOT, [])
			return false
	if kind == KIND_SAVE:
		var problem := slot_name_error(slot)
		if problem != "":
			_finish_op(kind, slot, false, ERROR_INVALID_SLOT, [])
			return false
	op_in_flight = kind
	op_slot = slot
	if not _dispatch(kind, slot):
		_finish_op(kind, slot, false, ERROR_TRANSPORT, [])
		return false
	return true


func _dispatch(kind: String, slot: String) -> bool:
	if not _sender.is_valid():
		return false
	_next_request_id += 1
	var request_id := _next_request_id
	_inflight[request_id] = kind
	if not bool(_sender.call(request_id, {"kind": kind, "slot": slot})):
		_inflight.erase(request_id)
		return false
	return true


func _deliver_one(reply: Dictionary) -> void:
	var request_id := int(reply.get("request_id", NO_REQUEST_ID))
	if not _inflight.has(request_id):
		return
	var kind := String(_inflight[request_id])
	_inflight.erase(request_id)
	var ok := bool(reply.get("ok", false))
	# **A REFUSAL AND A DEAD SOCKET ARRIVE IN THE SAME FIELD**, carrying tokens from the same
	# vocabulary — the bridge's contract. `""` would leave the pane saying nothing went wrong on a
	# reply that says it did, so an unlabelled failure reads as a transport one.
	var error := String(reply.get("error", ""))
	if not ok and error.is_empty():
		error = ERROR_TRANSPORT
	if kind == KIND_LIST:
		if ok:
			slots = reply.get("slots", []) as Array
			list_state = LIST_READY
			list_error = ""
		else:
			list_state = LIST_FAILED
			list_error = error
		slots_changed.emit()
		return
	var slot := String(reply.get("slot", op_slot))
	var drift: Array = reply.get("config_drift", []) as Array
	_finish_op(kind, slot, ok, error, drift)


## The ONE place an op ends, so `op_in_flight` cannot drift from what was announced — and so a
## successful save or delete always re-asks for the list, which is now stale by construction.
func _finish_op(kind: String, slot: String, ok: bool, error: String, drift: Array) -> void:
	op_in_flight = ""
	op_slot = ""
	op_finished.emit(kind, slot, ok, error, drift)
	if ok and (kind == KIND_SAVE or kind == KIND_DELETE):
		refresh()
