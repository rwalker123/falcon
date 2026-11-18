extends RefCounted
class_name WorldStatusPoll

## **"IS A WORLD ALREADY RUNNING UNDERNEATH THIS MENU?"** — the landing screen's poll of the server's
## faction-free `world_status` question (`docs/plan_scenarios.md` §2.7). A world can be built without
## the client asking: a scenario loaded from a terminal, or by `scripts/run_stack.sh --scenario`. The
## menu has to be able to take such a world rather than insist on generating its own.
##
## **THE ATTACH RULE, in one place.**
##   - `attach_any` (the launcher said a world was loaded BEFORE this client started): attach to the
##     first reply with `world_active`.
##   - otherwise the FIRST reply's `world_epoch` is the baseline — whatever was running when the menu
##     came up is a world the player has not chosen to join — and only a LATER reply showing an
##     active world with a HIGHER epoch attaches (a world started from a terminal while the menu is up).
##
## Owns no socket: the sender is injected, like `FactionCapacity`'s, and replies are handed in by the
## screen's single once-a-frame drain. One ask is in flight at a time and the next waits for
## `POLL_INTERVAL_SECONDS`, so a slow server is never queued behind its own poll.

## Emitted once, with the epoch of the world to attach to.
signal attach_requested(epoch: int)

const KIND_WORLD_STATUS := "world_status"

## How often the menu asks. A world appearing under an open menu is a developer's act, so a second of
## latency is invisible and the question costs the server nothing (it never touches the world).
const POLL_INTERVAL_SECONDS := 1.0

## "No reply seen yet" for the baseline.
const NO_EPOCH := -1

var attach_any: bool = false

var _send: Callable = Callable()
var _first_request_id: int = QueryRequestIds.NO_REQUEST_ID
var _asks_made: int = 0
var _in_flight_id: int = QueryRequestIds.NO_REQUEST_ID
var _since_last_ask: float = POLL_INTERVAL_SECONDS
var _baseline_epoch: int = NO_EPOCH
var _attached: bool = false


func _init() -> void:
	_first_request_id = QueryRequestIds.reserve_block()


func set_sender(sender: Callable) -> void:
	_send = sender


## Advance the poll clock; asks when one is due and none is outstanding.
func tick(delta: float) -> void:
	if _attached or not _send.is_valid() or _in_flight_id != QueryRequestIds.NO_REQUEST_ID:
		return
	_since_last_ask += delta
	if _since_last_ask < POLL_INTERVAL_SECONDS:
		return
	_since_last_ask = 0.0
	# **The poll never ends, so its ids WRAP inside the block** rather than walking out of it into the
	# next seam's (`QueryRequestIds.IDS_PER_SESSION`, ~18 h at one ask a second). Reuse is safe because
	# only one ask is ever in flight: an id comes round again long after its answer was drained.
	var request_id := _first_request_id + (_asks_made % QueryRequestIds.IDS_PER_SESSION)
	_asks_made += 1
	if bool(_send.call(request_id, {"kind": KIND_WORLD_STATUS})):
		_in_flight_id = request_id


## Take this frame's drained replies; every seam is handed the whole batch and keeps its own ids.
func deliver(replies: Array) -> void:
	for reply in replies:
		if not (reply is Dictionary):
			continue
		if int(reply.get("request_id", QueryRequestIds.NO_REQUEST_ID)) != _in_flight_id:
			continue
		_in_flight_id = QueryRequestIds.NO_REQUEST_ID
		if bool(reply.get("ok", false)):
			_read_status(bool(reply.get("world_active", false)), int(reply.get("world_epoch", 0)))


func _read_status(world_active: bool, epoch: int) -> void:
	if _attached:
		return
	if attach_any:
		if world_active:
			_attach(epoch)
		return
	if _baseline_epoch == NO_EPOCH:
		_baseline_epoch = epoch
		return
	if world_active and epoch > _baseline_epoch:
		_attach(epoch)


func _attach(epoch: int) -> void:
	_attached = true
	attach_requested.emit(epoch)
