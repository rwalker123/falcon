extends RefCounted
class_name ForecastQuery

## **THE FORECAST QUERY SEAM** — the client's half of the command socket's second direction
## (`sim_runtime/proto/command.proto` → "THE QUERY CHANNEL").
##
## The two pre-launch raid forecasts used to ride the per-turn snapshot: every huntable herd carried a
## (floor × party) table and a (party) table, pre-computed for ONE kit over a FRESH component set. They
## were ~93% of a turn's capture and they were WRONG for anyone who had worn their gear or picked
## another kit. The sheets now ASK, and are answered for their exact (band, kit, party, floor).
##
## **WHY IT IS ITS OWN OBJECT.** Three sheets across two controllers compose a raid — the Band panel's
## hunting-party and denial forms and the herd drawer's expedition branch — and every one of them needs
## the same four things: a request id, a rule for which reply is still wanted, a rule for what to show
## while waiting, and a re-render when the answer lands. That is a seam, not a helper, and two copies of
## it would drift the moment one sheet learned to keep its last answer and the other did not.
##
## **IT OWNS NO SOCKET.** `Main` injects a sender and pumps the replies in (`set_sender` / `deliver`),
## the coordinator-mediation rule: this layer is reachable from the HUD, and the HUD does not reach the
## network. That also makes every state below drivable from a harness with no server at all.
##
## **A QUERY TRIGGERS NO SNAPSHOT.** The server deliberately skips its re-capture for a question that
## changed nothing. The reply IS the render input, so a sheet must never wait on a frame — it waits
## here, and `answered` is what tells it to rebuild.

## Emitted on the main thread when a subject's answer changes — landed, failed, or went stale enough to
## stop being shown. The sheets rebuild off it; the payload is the SUBJECT, not the answer, so a
## listener re-reads through `view()` and cannot render a reply the seam has already superseded.
signal answered(subject: String)

# ---- the two questions, spelled as `bridge/query.rs` matches them ---------------------------------
const KIND_HUNT_TRIP := "hunt_trip_forecast"
const KIND_DENIAL_RAID := "denial_raid_forecast"

# ---- what a sheet gets back -----------------------------------------------------------------------

## An exact answer for the composed key. Render the numbers.
const STATE_READY := "ready"

## An answer is in flight and there is nothing honest to show yet — first open on a subject, or a
## re-query whose previous answer has aged past `STALE_AFTER_MSEC`. Render the placeholder.
const STATE_PENDING := "pending"

## The server refused, or the round trip failed. Render the terse failure, never blank numbers.
const STATE_FAILED := "failed"

## **HOW LONG A SUPERSEDED ANSWER MAY STAND WHILE ITS REPLACEMENT IS IN FLIGHT.**
##
## A stepper tick changes the composed key, so strictly there is no answer for what is on screen. But
## the round trip is local and typically sub-frame, and blanking a sheet's numbers on every `+` press
## is a worse lie than showing the previous party's for one frame — it reads as "this raid has no
## forecast". So the last answer for the same SUBJECT stands while the new one flies.
##
## Past this it stops standing. The threshold is what keeps "briefly one tick behind" from becoming
## "quietly showing a number for a party you are not sending": beyond it the sheet says it is waiting.
const STALE_AFTER_MSEC := 400

## The id of a request that was never made. `0` is never issued — the sequence starts at 1 — so it can
## be told apart from a real reply.
const NO_REQUEST_ID := 0

var _sender: Callable = Callable()
var _next_request_id: int = NO_REQUEST_ID
## subject -> {"answer": Dictionary, "answer_key": String, "asked_key": String, "asked_id": int,
##             "asked_at": int, "error": String, "error_key": String}
var _subjects: Dictionary = {}
## request_id -> subject, for the reply that has not landed yet.
var _inflight: Dictionary = {}

## Inject the transport. `sender` is `func(request_id: int, ask: Dictionary) -> bool` — true when the
## question reached the socket. Nothing here is sent until one is set, so a HUD standing up before the
## command client exists simply asks nothing.
func set_sender(sender: Callable) -> void:
	_sender = sender

## **THE SUBJECT KEY — what "the same forecast, re-asked" means.** Kind + band + herd, and deliberately
## NOT the kit, party or floor: those are exactly the things a player changes while looking at one
## sheet, and they are what the stale-answer rule exists to smooth over. Changing HERD is a different
## question, and its previous answer must never be shown against it.
static func subject_of(kind: String, band_id: int, herd_id: String) -> String:
	return "%s:%d:%s" % [kind, band_id, herd_id]

## …and the exact key, which adds everything the answer actually depends on. Two renders with the same
## key ask nothing new; any difference is a fresh question.
static func key_of(subject: String, kit_id: String, party_workers: int, floor: float) -> String:
	return "%s:%s:%d:%f" % [subject, kit_id, party_workers, floor]

## **ASK, IDEMPOTENTLY.** Call it on every render of a sheet: a key already answered or already in
## flight sends nothing, so the sheets do not need to remember what they asked. `params` is the
## `QueryCommand` payload minus the `kind`, which `kind` supplies.
func ask(kind: String, subject: String, key: String, params: Dictionary) -> void:
	var entry: Dictionary = _subjects.get(subject, {})
	if String(entry.get("answer_key", "")) == key or String(entry.get("asked_key", "")) == key:
		return
	if String(entry.get("error_key", "")) == key:
		# A refused question is not re-asked on every rebuild. The refusals are all client bugs (the
		# sheet composes the request itself), so retrying one would spin the socket rather than fix it.
		return
	if not _sender.is_valid():
		return
	_next_request_id += 1
	var request_id := _next_request_id
	var ask_payload := params.duplicate()
	ask_payload["kind"] = kind
	entry["asked_key"] = key
	entry["asked_id"] = request_id
	entry["asked_at"] = Time.get_ticks_msec()
	_subjects[subject] = entry
	_inflight[request_id] = subject
	if not bool(_sender.call(request_id, ask_payload)):
		_inflight.erase(request_id)
		_fail(subject, key, HudComposeVocab.QUERY_ERROR_TRANSPORT)

## What to render for `key` right now: `{"state", "answer", "error"}`. `answer` is `{}` in every state
## but `STATE_READY`, so a caller that ignores `state` renders nothing rather than zeros.
##
## **A STALE ANSWER COMES BACK AS `STATE_READY` AND IS NOT MARKED**, within `STALE_AFTER_MSEC`. It is
## the previous tick's numbers for the same subject, on screen for a fraction of a frame; a badge
## saying so would flicker harder than the numbers it apologises for. Past the threshold it is not
## returned at all — see the const.
func view(subject: String, key: String) -> Dictionary:
	var entry: Dictionary = _subjects.get(subject, {})
	if entry.is_empty():
		return {"state": STATE_PENDING, "answer": {}, "error": ""}
	if String(entry.get("answer_key", "")) == key:
		return {"state": STATE_READY, "answer": entry.get("answer", {}), "error": ""}
	if String(entry.get("error_key", "")) == key:
		return {"state": STATE_FAILED, "answer": {}, "error": String(entry.get("error", ""))}
	var answer: Dictionary = entry.get("answer", {})
	if not answer.is_empty() and String(entry.get("asked_key", "")) == key \
			and Time.get_ticks_msec() - int(entry.get("asked_at", 0)) < STALE_AFTER_MSEC:
		return {"state": STATE_READY, "answer": answer, "error": ""}
	return {"state": STATE_PENDING, "answer": {}, "error": ""}

## **HAS THIS QUESTION STOPPED BEING IN FLIGHT?** — true for an answer and for a refusal, false only
## while one is still coming.
##
## It exists for the ONE-SHOT problem, and the sheets' crew auto-fill is what has it: a promise to fill
## the stepper with the SIM'S plateau is a promise about a number that rides the reply, while the render
## that ARMS the one-shot is by construction the render that has just asked. Spending it there fills the
## party from the no-answer fallback and the real answer lands a frame later with nothing left to apply
## it to — the fill silently becomes "as many hands as the band has", which is a different promise.
##
## **A FAILURE COUNTS AS SETTLED**, deliberately: a question that will never be answered must not leave
## a one-shot armed for the rest of the composing act.
static func answer_settled(view: Dictionary) -> bool:
	return String(view.get("state", STATE_PENDING)) != STATE_PENDING

## Pump the native drain (`CommandBridge.poll_query_replies`) in. Called once a frame by `Main`; an
## empty array is the ordinary case and costs nothing.
func deliver(replies: Array) -> void:
	for reply_variant in replies:
		if reply_variant is Dictionary:
			_deliver_one(reply_variant as Dictionary)

## **A SUBJECT WHOSE STALE WINDOW HAS JUST CLOSED HAS TO REDRAW.** Nothing else would tell it: no reply
## landed, so `deliver` emits nothing, and a sheet showing a superseded answer would keep showing it
## until the player touched something. Called from the same per-frame pump.
func expire_stale() -> void:
	var now := Time.get_ticks_msec()
	for subject in _subjects:
		var entry: Dictionary = _subjects[subject]
		if String(entry.get("asked_key", "")) == "" or entry.get("answer", {}).is_empty():
			continue
		if bool(entry.get("stale_announced", false)):
			continue
		if now - int(entry.get("asked_at", 0)) >= STALE_AFTER_MSEC:
			entry["stale_announced"] = true
			_subjects[subject] = entry
			answered.emit(String(subject))

## Forget everything. A new world's ids describe different bands and herds, so an answer held across
## one would be quoted against a subject that no longer exists.
func reset() -> void:
	_subjects.clear()
	_inflight.clear()

func _deliver_one(reply: Dictionary) -> void:
	var request_id := int(reply.get("request_id", NO_REQUEST_ID))
	if not _inflight.has(request_id):
		return
	var subject := String(_inflight[request_id])
	_inflight.erase(request_id)
	var entry: Dictionary = _subjects.get(subject, {})
	# **DROP A REPLY THE SHEET HAS MOVED PAST.** The player stepped the party twice while the first
	# answer was in flight; landing it now would overwrite the newer question's key with older numbers
	# and the sheet would settle on the wrong party. The id that is still wanted is the one recorded
	# when the CURRENT key was asked.
	if int(entry.get("asked_id", NO_REQUEST_ID)) != request_id:
		return
	var key := String(entry.get("asked_key", ""))
	if not bool(reply.get("ok", false)):
		_fail(subject, key, String(reply.get("error", HudComposeVocab.QUERY_ERROR_TRANSPORT)))
		return
	entry["answer"] = reply
	entry["answer_key"] = key
	entry["asked_key"] = ""
	entry["asked_id"] = NO_REQUEST_ID
	entry["stale_announced"] = false
	entry["error"] = ""
	entry["error_key"] = ""
	_subjects[subject] = entry
	answered.emit(subject)

func _fail(subject: String, key: String, token: String) -> void:
	var entry: Dictionary = _subjects.get(subject, {})
	entry["error"] = token
	entry["error_key"] = key
	entry["asked_key"] = ""
	entry["asked_id"] = NO_REQUEST_ID
	# The previous answer goes with the failure: it was for another party, and standing it beside a
	# failure line would present stale numbers as the answer to a question that was refused.
	entry["answer"] = {}
	entry["answer_key"] = ""
	_subjects[subject] = entry
	answered.emit(subject)
