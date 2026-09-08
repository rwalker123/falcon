extends RefCounted
class_name FactionCapacity

## **THE "HOW MANY RIVALS MAY I ASK FOR?" SEAM** — the client's half of the `faction_capacity` query
## (`clients/godot_thin_client/native/src/bridge/query.rs` → `QUERY_KIND_FACTION_CAPACITY`).
##
## The ceiling on rival peoples is a property of the GRID, not of the running world: it falls out of
## how many starts fit at `faction_start_min_separation` (`core_sim`'s `faction_start_capacity`). So
## the New Game screen cannot compute it, and must not restate it — it asks, for the width and height
## the player has actually picked, and re-asks when that pick changes.
##
## **ANSWERABLE WITH NO WORLD RUNNING**, which is the whole point: the server boots idle and publishes
## no frame until `new_game`, and this question is asked before that command exists.
##
## Modelled on `SaveSlots`, and for the same reasons: a round trip whose answer arrives on the command
## socket's second direction needs a request id, a rule for which reply is still wanted, something
## honest to render while waiting, and a signal when the answer lands.
##
## **IT OWNS NO SOCKET.** The owner injects a sender and pumps the replies in (`set_sender` /
## `deliver`) — the coordinator-mediation rule. `MenuShell` is a view over it and never reaches the
## network itself, which is also what makes every state below drivable from a harness with no server.

## The answer changed — landed, failed, or a new grid put a fresh ask in flight.
signal capacity_changed

## The ask, spelled as `bridge/query.rs` matches it. Both directions use this one kind.
const KIND_CAPACITY := "faction_capacity"

# ---- state ---------------------------------------------------------------------------------------

## Nothing has been asked yet — no grid has been offered to this seam.
const STATE_IDLE := "idle"
## An ask is in flight. There is no honest number to show yet, and 0 is not a stand-in for one.
const STATE_PENDING := "pending"
## The ceiling landed, for `asked_width` × `asked_height`.
const STATE_READY := "ready"
## The ask failed or was refused. `error` carries the token.
const STATE_FAILED := "failed"

## **WHAT AN UNANSWERED SEAM IS WORTH: nothing, and it says so.** A guessed ceiling would either
## refuse a count the map could seat or offer one it could not, and the New Game screen's fallback is
## to send no count at all and let the server use its configured default — so there is deliberately no
## "assumed" maximum here to fall back on.
const NO_COUNT := -1

## The bridge's own failure token, in the same vocabulary the server's refusals use.
const ERROR_TRANSPORT := "transport"

var state: String = STATE_IDLE
var error: String = ""
## The server's opening count for the asked grid — already clamped to `max_ai_factions`, so it is
## always a grantable pick. `NO_COUNT` until an answer lands.
var default_count: int = NO_COUNT
## The most rivals the asked grid can seat. **May legitimately be 0** on a grid too small to hold a
## second start at the configured separation; the control renders that as "you are alone", not as a
## failure. `NO_COUNT` until an answer lands.
var max_count: int = NO_COUNT
## The grid the current answer (or the ask in flight) is about. An answer is only ever read against
## these, so a reply for a size the player has since changed cannot be shown for the new one.
var asked_width: int = 0
var asked_height: int = 0

var _sender: Callable = Callable()
var _next_request_id: int = QueryRequestIds.REQUEST_ID_BASE
## The one id still owed an answer (`NO_REQUEST_ID` when none). One ask at a time: a second grid makes
## the first answer irrelevant, so it supersedes rather than queues.
var _inflight_request_id: int = QueryRequestIds.NO_REQUEST_ID


func _init() -> void:
	_next_request_id = QueryRequestIds.reserve_block()


## Inject the transport. `sender` is `func(request_id: int, ask: Dictionary) -> bool` — true when the
## ask reached the socket. Nothing is sent until one is set, so a screen standing up before its owner
## has a command client simply asks nothing and renders `STATE_IDLE`.
func set_sender(sender: Callable) -> void:
	_sender = sender


## **ASK FOR THIS GRID'S CEILING.** Called when the setup pane is opened and whenever the map size
## changes, because the answer is about the map being made and not about the one the server is
## holding.
##
## Re-asking the same grid while an answer for it is already in hand is dropped: the ceiling is a pure
## function of the grid, so a second round trip cannot say anything new.
func request(width: int, height: int) -> void:
	if width <= 0 or height <= 0:
		return
	if width == asked_width and height == asked_height and (state == STATE_PENDING or state == STATE_READY):
		return
	asked_width = width
	asked_height = height
	state = STATE_PENDING
	error = ""
	default_count = NO_COUNT
	max_count = NO_COUNT
	capacity_changed.emit()
	if not _dispatch(width, height):
		state = STATE_FAILED
		error = ERROR_TRANSPORT
		capacity_changed.emit()


## Ask the same question again after a failure. The setup pane calls this when it is re-opened rather
## than on every rebuild: a dead server heals on its own, but a rebuild-driven retry would spin the
## socket for as long as the screen is up.
func retry() -> void:
	if state != STATE_FAILED:
		return
	var width := asked_width
	var height := asked_height
	asked_width = 0
	asked_height = 0
	request(width, height)


## Pump the native drain in. Called once a frame by the owner; ids that are not ours are ignored, so
## the same array can be handed to `SaveSlots` as well.
func deliver(replies: Array) -> void:
	for reply_variant in replies:
		if reply_variant is Dictionary:
			_deliver_one(reply_variant as Dictionary)


## Is `count` a rival count this grid's answer permits? `NO_COUNT` — "let the server decide" — is
## permitted in every state, because it is the absence of a pick rather than a pick.
func permits(count: int) -> bool:
	if count == NO_COUNT:
		return true
	if state != STATE_READY:
		return false
	return count >= 0 and count <= max_count


## The pick clamped to what the answer allows, so a count chosen on a big map cannot survive a switch
## to a small one. `NO_COUNT` in, `NO_COUNT` out — an unmade pick is not clamped into a made one.
func clamp_count(count: int) -> int:
	if count == NO_COUNT or state != STATE_READY:
		return count
	return clampi(count, 0, max_count)


func _dispatch(width: int, height: int) -> bool:
	if not _sender.is_valid():
		return false
	_next_request_id += 1
	var request_id := _next_request_id
	_inflight_request_id = request_id
	if not bool(_sender.call(request_id, {"kind": KIND_CAPACITY, "width": width, "height": height})):
		_inflight_request_id = QueryRequestIds.NO_REQUEST_ID
		return false
	return true


func _deliver_one(reply: Dictionary) -> void:
	var request_id := int(reply.get("request_id", QueryRequestIds.NO_REQUEST_ID))
	if request_id != _inflight_request_id or request_id == QueryRequestIds.NO_REQUEST_ID:
		return
	_inflight_request_id = QueryRequestIds.NO_REQUEST_ID
	var ok := bool(reply.get("ok", false))
	if not ok:
		# **A REFUSAL AND A DEAD SOCKET ARRIVE IN THE SAME FIELD**, carrying tokens from the same
		# vocabulary — the bridge's contract. An unlabelled failure reads as a transport one rather
		# than as nothing having gone wrong.
		state = STATE_FAILED
		error = String(reply.get("error", ""))
		if error.is_empty():
			error = ERROR_TRANSPORT
		default_count = NO_COUNT
		max_count = NO_COUNT
		capacity_changed.emit()
		return
	max_count = maxi(0, int(reply.get("max_ai_faction_count", 0)))
	# The server clamps its configured default to the same ceiling before answering; clamping again
	# here costs nothing and keeps "the opening pick is always grantable" true of this seam alone.
	default_count = clampi(int(reply.get("default_ai_faction_count", 0)), 0, max_count)
	state = STATE_READY
	error = ""
	capacity_changed.emit()
