extends RefCounted
class_name FactionCapacity

## **THE "HOW MANY PEOPLES MAY I ASK FOR?" SEAM** — the client's half of the `faction_capacity` query
## (`clients/godot_thin_client/native/src/bridge/query.rs` → `QUERY_KIND_FACTION_CAPACITY`).
##
## The ceiling on other peoples is a property of the GRID, not of the running world: it falls out of
## how many starts fit at each start archetype's `min_separation`. The answer is one row per ARCHETYPE
## of the chosen start profile, in catalog order (`archetypes`), each carrying the number of starts the
## grid seats at that archetype's separation (`site_count`). The New Game screen asks, for the width,
## height and profile the player has actually picked, and re-asks when any of them changes.
##
## **THE CEILING CONTRACT** (`ArchetypeCapacity` in `command.proto`): how many of archetype k a player
## may ask for is `max(0, site_count_k - 1 - sum over j < k of count_j)`, the 1 being the human seat and
## `count_j` the CURRENT counts of the EARLIER rows. That is the one piece of arithmetic this file
## implements (`ceiling_at`); the server applies the same formula and clamps, so a client inside it is
## granted exactly what it asked for.
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
## refuse a count the map could seat or offer one it could not, so the New Game screen sends no count
## at all instead — which the server answers with its unattended roster, i.e. a world with no rivals
## in it. There is deliberately no "assumed" maximum here to fall back on, and the caption on that
## screen states the consequence rather than implying a number.
const NO_COUNT := -1

## The seat the contract subtracts from every row's `site_count`: the human's own start, which is
## never one of the counted peoples (`ArchetypeCapacity` in `command.proto`).
const HUMAN_SEATS := 1

## The bridge's own failure token, in the same vocabulary the server's refusals use.
## **IT IS THE ONE TOKEN THAT MEANS "NO SERVER ANSWERED AT ALL"** — nothing reached a listener, so
## nothing about the map, the count or the run is known. Every other token in this field came FROM a
## server, which is a different fact and a different caption (`MenuShell._apply_rivals_caption`).
const ERROR_TRANSPORT := "transport"
## The one non-transport refusal this question can actually come back with today: a capacity ask that
## reached a server and was routed to the world answerer instead of the pre-world one
## (`sim_runtime::commands::query_error::WRONG_ANSWERER`). Named so the state "a server answered and
## still gave no count" is drivable from `menu_preview` with a token from the real vocabulary rather
## than an invented string.
const ERROR_WRONG_ANSWERER := "wrong_answerer"

var state: String = STATE_IDLE
var error: String = ""
## The archetype rows of the current answer, in catalog order: `{id, display_label, site_count,
## default_count}`. Empty until an answer lands, and after a failure — there is no honest row to show.
var archetypes: Array = []
## The grid the current answer (or the ask in flight) is about. An answer is only ever read against
## these, so a reply for a size the player has since changed cannot be shown for the new one.
var asked_width: int = 0
var asked_height: int = 0
## The start profile the answer is about. A different profile is a different roster, so it is part of
## the question exactly as the grid is.
var asked_profile_id: String = ""

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


## **ASK FOR THIS GRID'S CEILINGS.** Called when the setup pane is opened and whenever the map size or
## the start profile changes, because the answer is about the world being made and not about the one
## the server is holding.
##
## Re-asking the same grid and profile while an answer for them is already in hand is dropped: the
## ceilings are a pure function of both, so a second round trip cannot say anything new.
func request(width: int, height: int, profile_id: String = "") -> void:
	if width <= 0 or height <= 0:
		return
	if width == asked_width and height == asked_height and profile_id == asked_profile_id \
			and (state == STATE_PENDING or state == STATE_READY):
		return
	asked_width = width
	asked_height = height
	asked_profile_id = profile_id
	state = STATE_PENDING
	error = ""
	archetypes = []
	capacity_changed.emit()
	if not _dispatch(width, height, profile_id):
		state = STATE_FAILED
		error = ERROR_TRANSPORT
		capacity_changed.emit()


## Ask the same question again after a failure. Never driven by a rebuild — that would spin the socket
## once per redraw. `MenuShell` calls it when the setup pane is re-opened, and, while the server is
## UNREACHABLE, on its own slow clock (`MenuShell.RIVALS_RETRY_SECONDS`), because that state now
## blocks the run and so has to heal without the player finding the one control that re-asks.
## A no-op in every state but `STATE_FAILED`, so a retry landing on an ask already in flight costs
## nothing.
func retry() -> void:
	if state != STATE_FAILED:
		return
	var width := asked_width
	var height := asked_height
	var profile_id := asked_profile_id
	asked_width = 0
	asked_height = 0
	request(width, height, profile_id)


## Pump the native drain in. Called once a frame by the owner; ids that are not ours are ignored, so
## the same array can be handed to `SaveSlots` as well.
func deliver(replies: Array) -> void:
	for reply_variant in replies:
		if reply_variant is Dictionary:
			_deliver_one(reply_variant as Dictionary)


## **DID THE ASK EVER REACH A SERVER?** `true` only for a failure carrying [`ERROR_TRANSPORT`] —
## the ask never got an answer from anything, so the server is unreachable and no world can be built
## from this screen at all. A failure carrying any other token is a server that answered without a
## usable count, which is a lesser fact: the world can still be built, just without a chosen rival
## count. **The classification lives here** because this seam owns the token vocabulary; the views
## over it ask rather than compare strings.
func server_is_unreachable() -> bool:
	return state == STATE_FAILED and error == ERROR_TRANSPORT


## The ceiling for the archetype at `index`, by the contract formula, given `counts` — the CURRENT count
## of each row, in catalog order (only the entries BEFORE `index` are read; a `NO_COUNT` or missing
## entry counts as 0). `0` when there is no answer or `index` is out of range.
func ceiling_at(index: int, counts: Array) -> int:
	if state != STATE_READY or index < 0 or index >= archetypes.size():
		return 0
	var taken := 0
	for earlier in range(mini(index, counts.size())):
		taken += maxi(0, int(counts[earlier]))
	var sites := int((archetypes[index] as Dictionary).get("site_count", 0))
	return maxi(0, sites - HUMAN_SEATS - taken)


## Is `count` a count the answer permits for the archetype at `index`, given the earlier rows'
## `counts`? `NO_COUNT` — "let the server decide" — is permitted in every state, because it is the
## absence of a pick rather than a pick.
func permits(index: int, count: int, counts: Array) -> bool:
	if count == NO_COUNT:
		return true
	if state != STATE_READY:
		return false
	return count >= 0 and count <= ceiling_at(index, counts)


## The pick clamped to what the answer allows for the archetype at `index`, so a count chosen on a big
## map cannot survive a switch to a small one. `NO_COUNT` in, `NO_COUNT` out — an unmade pick is not
## clamped into a made one.
func clamp_count(index: int, count: int, counts: Array) -> int:
	if count == NO_COUNT or state != STATE_READY:
		return count
	return clampi(count, 0, ceiling_at(index, counts))


## **EVERY ROW RE-CLAMPED, IN ORDER.** A ceiling depends on the rows before it, so changing one row
## can shrink every later one; this walks the rows once, each clamped against the already-clamped
## earlier ones. `counts` is read by position, and a missing entry takes the row's opening pick.
## Returns one count per archetype row.
func clamp_all(counts: Array) -> Array:
	var out: Array = []
	for index in range(archetypes.size()):
		var wanted: int = int(counts[index]) if index < counts.size() else default_at(index)
		out.append(clamp_count(index, wanted, out))
	return out


## The server's opening pick for the archetype at `index`, before it is held inside the ceiling the
## earlier rows leave. `NO_COUNT` when there is no such row.
func default_at(index: int) -> int:
	if state != STATE_READY or index < 0 or index >= archetypes.size():
		return NO_COUNT
	return int((archetypes[index] as Dictionary).get("default_count", 0))


## Every row's opening pick, each clamped against the earlier ones (the server already did, so this
## keeps "the opening picks are always grantable" true of this seam alone).
func default_counts() -> Array:
	var wanted: Array = []
	for index in range(archetypes.size()):
		wanted.append(default_at(index))
	return clamp_all(wanted)


func _dispatch(width: int, height: int, profile_id: String) -> bool:
	if not _sender.is_valid():
		return false
	_next_request_id += 1
	var request_id := _next_request_id
	_inflight_request_id = request_id
	if not bool(_sender.call(request_id, {"kind": KIND_CAPACITY, "width": width, "height": height, "profile_id": profile_id})):
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
		archetypes = []
		capacity_changed.emit()
		return
	archetypes = []
	var rows_variant: Variant = reply.get("archetypes", [])
	if rows_variant is Array:
		for row_variant in (rows_variant as Array):
			if row_variant is Dictionary:
				var row := row_variant as Dictionary
				archetypes.append({
					"id": String(row.get("id", "")),
					"display_label": String(row.get("display_label", "")),
					"site_count": maxi(0, int(row.get("site_count", 0))),
					"default_count": maxi(0, int(row.get("default_count", 0))),
				})
	state = STATE_READY
	error = ""
	capacity_changed.emit()
