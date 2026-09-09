extends RefCounted
class_name SeatClaim

## **THE SEAT SEAM — "do I drive this faction?"**
##
## The server takes the faction a command acts on from the seat the sending *connection* claimed, not
## from the `faction_id` on the wire (`.claude/rules/core_sim/factions.md` → "Seats"). So one claim,
## made once on the long-lived command connection, is what makes every band order this client sends
## legal; without it every faction-bearing command is refused with `not_this_connections_seat` and the
## game is unplayable in the most confusing way available — nothing the player clicks does anything,
## and no surface says why.
##
## **THAT IS THE WHOLE REASON THIS SEAM REPORTS.** A refusal is not a log line: it is the difference
## between a working game and a dead one, so it comes out as a signal the owner (`Main`) puts on the
## event dock's System channel and, while the loading overlay is still up, on the overlay itself.
##
## Shaped like `SaveSlots` and `FactionCapacity`: an injected sender, an id from the one shared
## allocator (`QueryRequestIds`), and a `deliver` the owner pumps once a frame with the whole drain.
## The claim's answer rides the query drain because a `ClaimSeatCommand` is a command that is
## ANSWERED — on the socket that made it — exactly as the save verbs are.

## The seat is ours. Emitted once per grant; a re-grant after a reconnect emits again, because the
## seat genuinely went away and came back.
signal seated(faction_id: int)

## The seat is NOT ours, and `error` says why in the server's own vocabulary. Also emitted when the
## claim went unanswered, carrying [`ERROR_TRANSPORT`] — a claim nobody answered leaves the client in
## the same unplayable state a refused one does, so it must not be quieter.
signal refused(faction_id: int, error: String)

## The refusal tokens (`sim_runtime::commands::seat_error`), plus the transport's own. Named rather
## than compared as literals so a typo cannot become a refusal that renders as "unknown".
const ERROR_UNKNOWN_SEAT := "unknown_seat"
const ERROR_SEAT_OCCUPIED := "seat_occupied"
const ERROR_ALREADY_SEATED := "already_seated"
## The bridge's token for "the socket said nothing" — the same one `SaveSlots` reads, from the same
## vocabulary (`query.rs` → `QUERY_ERROR_TRANSPORT`).
const ERROR_TRANSPORT := "transport"

## **WHAT A REFUSAL MEANS TO A PLAYER.** Each names what is wrong with THIS session rather than
## restating the token: a player cannot act on `already_seated`, and the one thing they can do about
## `seat_occupied` is not join this game twice.
const ERROR_PROSE := {
	ERROR_UNKNOWN_SEAT: "This world has no seat for your people; the server may be running a different game.",
	ERROR_SEAT_OCCUPIED: "Another player already holds your seat in this game.",
	ERROR_ALREADY_SEATED: "This connection already holds a seat.",
	ERROR_TRANSPORT: "The server never answered the request to take your seat.",
}
const ERROR_PROSE_UNKNOWN := "The server refused your seat (%s)."

## The one thing the player has to be told, whatever the reason: their orders will not be obeyed.
const REFUSED_HEADLINE := "You do not hold your people's seat — orders will not be obeyed."

const NO_REQUEST_ID := QueryRequestIds.NO_REQUEST_ID

var _sender: Callable = Callable()
## The id this claim is answered under. One id for the life of the seam: the native link re-claims on
## every reconnect under the SAME id, so an answer to the third claim is still an answer to our one
## question.
var _request_id: int = NO_REQUEST_ID
## The faction asked for, or `HudConst.NO_FACTION_ID` before anything was asked.
var _faction_id: int = HudConst.NO_FACTION_ID
## Whether the last answer granted the seat. Read by the owner rather than tracked twice.
var seated_now: bool = false
## The last refusal token, `""` while the seat is held or nothing has been asked.
var refusal: String = ""


func _init() -> void:
	_request_id = QueryRequestIds.reserve_block()


## Inject the transport: `func(faction_id: int, request_id: int) -> bool`, true when the ask reached
## the bridge. Nothing is claimed until one is set.
func set_sender(sender: Callable) -> void:
	_sender = sender


## **ASK FOR THE SEAT.** Called once, at boot, after the command client exists.
##
## A dispatch failure is reported through the same `refused` signal a server refusal takes: from the
## player's side there is no difference between "the ask never left" and "the answer was no" — in both
## cases their orders will not be obeyed, and that is the only fact the report has to carry.
func request(faction_id: int) -> bool:
	if not _sender.is_valid():
		return false
	_faction_id = faction_id
	if bool(_sender.call(faction_id, _request_id)):
		return true
	_finish(false, ERROR_TRANSPORT)
	return false


## Pump the native drain in. Ids that are not ours are ignored, so the same array goes to every other
## seam as well (`Main._pump_forecast_queries`).
func deliver(replies: Array) -> void:
	for reply_variant in replies:
		if reply_variant is Dictionary:
			_deliver_one(reply_variant as Dictionary)


## The player-facing sentence for a token.
static func error_prose(token: String) -> String:
	if ERROR_PROSE.has(token):
		return String(ERROR_PROSE[token])
	return ERROR_PROSE_UNKNOWN % token


func _deliver_one(reply: Dictionary) -> void:
	if int(reply.get("request_id", NO_REQUEST_ID)) != _request_id:
		return
	var ok := bool(reply.get("ok", false))
	# **A REFUSAL AND A DEAD SOCKET ARRIVE IN THE SAME FIELD**, carrying tokens from one vocabulary —
	# the bridge's contract. An unlabelled failure is a transport one, since the server always names
	# its own refusals.
	var error := String(reply.get("error", ""))
	if not ok and error.is_empty():
		error = ERROR_TRANSPORT
	_finish(ok, error)


func _finish(ok: bool, error: String) -> void:
	seated_now = ok
	refusal = "" if ok else error
	if ok:
		seated.emit(_faction_id)
		return
	refused.emit(_faction_id, error)
