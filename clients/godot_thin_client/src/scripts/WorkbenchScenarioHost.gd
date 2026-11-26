extends RefCounted
class_name WorkbenchScenarioHost

## **`Main`'s end of the Workbench's two scenario services** — `scenario_request` and `pick_tile`
## (`WorkbenchVocab.SERVICE_*`). Held in its own object, as `SaveSlots` is, so `Main` carries two thin
## delegators rather than a request table and a pick latch.
##
## **Request half** — modelled on `SaveSlots`: it owns no socket (the owner injects a sender and pumps the
## native drain into `deliver`), draws ids from `QueryRequestIds` so they cannot collide with another
## seam on the same drain, and routes a reply to the Callable that asked for it by `request_id`. A reply
## for an id this seam does not hold is ignored, so the one drained batch can be handed to every seam.
##
## **Pick half** — a ONE-SHOT latch. `arm_pick` stores the callbacks; the owner offers it every map hex
## click (`offer_hex`) and Escape (`cancel_pick`). A second `arm_pick` while armed CANCELS rather than
## re-arming, so the page's pick button toggles. **The pick is delivered IN ADDITION to the click's
## normal selection**, not instead of it: the map's `hex_selected` has already been handled by the time
## the owner sees it, and a designer filling in a tile coordinate wants the tile card up beside it anyway.

## The four scenario asks, spelled as `bridge/query.rs` matches them.
const KIND_LIST := "list_scenarios"
const KIND_READ := "read_scenario"
const KIND_WRITE := "write_scenario"
const KIND_LOAD := "load_scenario"
const KINDS := [KIND_LIST, KIND_READ, KIND_WRITE, KIND_LOAD]

## `(request_id: int, ask: Dictionary) -> bool` — true when the ask reached the socket.
var _sender: Callable = Callable()
var _next_request_id: int = QueryRequestIds.REQUEST_ID_BASE
## request_id -> the `on_reply` Callable waiting on it.
var _inflight: Dictionary = {}
var _pick: Callable = Callable()
var _pick_cancel: Callable = Callable()


func _init() -> void:
	_next_request_id = QueryRequestIds.reserve_block()


func set_sender(sender: Callable) -> void:
	_sender = sender


## Send one scenario verb. `args` carries `path` / `text` as the verb needs. False (and nothing sent) for an
## unknown kind, no sender, or a socket that refused.
func request(kind: String, args: Dictionary, on_reply: Callable) -> bool:
	if not KINDS.has(kind) or not _sender.is_valid():
		return false
	_next_request_id += 1
	var request_id := _next_request_id
	var ask := args.duplicate()
	ask["kind"] = kind
	_inflight[request_id] = on_reply
	if not bool(_sender.call(request_id, ask)):
		_inflight.erase(request_id)
		return false
	return true


## Pump the native drain in. A reply nobody here asked for is left for the next seam.
func deliver(replies: Array) -> void:
	for reply_variant in replies:
		if not (reply_variant is Dictionary):
			continue
		var reply: Dictionary = reply_variant
		var request_id := int(reply.get("request_id", QueryRequestIds.NO_REQUEST_ID))
		if not _inflight.has(request_id):
			continue
		var on_reply: Callable = _inflight[request_id]
		_inflight.erase(request_id)
		if on_reply.is_valid():
			on_reply.call(reply)


# ---- the one-shot pick -------------------------------------------------------

## Arm a pick. Already armed → this press cancels it (and answers true: the press was handled).
func arm_pick(on_pick: Callable, on_cancel: Callable = Callable()) -> bool:
	if is_pick_armed():
		cancel_pick()
		return true
	_pick = on_pick
	_pick_cancel = on_cancel
	return true


func is_pick_armed() -> bool:
	return _pick.is_valid()


## A hex was clicked. If a pick is armed, deliver it and disarm; answers whether it was consumed.
func offer_hex(col: int, row: int) -> bool:
	if not is_pick_armed():
		return false
	var pick := _pick
	_pick = Callable()
	_pick_cancel = Callable()
	pick.call(col, row)
	return true


## Escape (or a second arm): drop the pick and tell the page it was cancelled. Answers whether one was armed.
func cancel_pick() -> bool:
	if not is_pick_armed():
		return false
	var cancel := _pick_cancel
	_pick = Callable()
	_pick_cancel = Callable()
	if cancel.is_valid():
		cancel.call()
	return true
