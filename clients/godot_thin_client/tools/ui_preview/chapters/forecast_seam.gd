extends RefCounted

## The forecast-query seam's own rules — the world boundary, the two failure classes, and the
## plateau cap's direction.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.
##
## It is LAST and it renders NOTHING — no `_save`, so the frame set's count and its bit-identity claim
## are untouched. Every claim here is about a decision no picture can carry: a cached answer that must
## not survive a world change, a refusal that must stick, a failure that must be retried, and which end
## of a plateau a stepper seeds on. Each of the four renders exactly the same plausible sheet whichever
## way it goes.
##
## It leaves the seam EMPTY and the harness's canned answerer reinstalled, so a chapter appended after
## it starts where every other one does.

const ForecastFx := preload("res://tools/ui_preview/fixtures_forecast.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## The subject these guards ask about. A band id and a herd id in the shape a new world hands out
## again — the collision the world-boundary reset exists for is that both of these are reused.
const GUARD_BAND_ID := 1
const GUARD_HERD_ID := "herd_red_deer_00"
## A SECOND quarry, so a claim about one subject cannot be satisfied by the state of the other.
const REFUSED_HERD_ID := "herd_aurochs_00"
## …and the two the failure-class pair uses, one per class for the same reason.
const TRANSPORT_HERD_ID := "herd_wild_boar_00"
const RETRY_FRESH_HERD_ID := "herd_reindeer_00"
const GUARD_KIT_ID := "hunting_kit"
const GUARD_PARTY := 4
const GUARD_FLOOR := 0.35

## One of the server's own refusal tokens (`sim_runtime::commands::query_error`). Any of the seven
## would do — what is under test is the CLASS, not this spelling.
const SERVER_REFUSAL_TOKEN := "unknown_herd"

## The plateau probe's inputs. `SCANNED_PLATEAU` stands for `HuntTripForecastReply.useful_cap` and
## `AMPLE_IDLE` is a supply side deliberately well above it, so the answer is the demand side alone.
const SCANNED_PLATEAU := 5
const AMPLE_IDLE := 12

func run(harness) -> void:
	h = harness
	_assert_world_change_drops_the_answers()
	_assert_failure_classes_differ()
	_assert_useful_cap_is_the_last_rising_party()
	# Back to the state every other chapter runs in: an empty seam with the canned answerer on it.
	h._hud.forecast_query().reset()
	ForecastFx.install(h._hud)

# ---- THE WORLD BOUNDARY -------------------------------------------------------------------------
# `Main._reset_per_world_state` → `HudLayer.reset_world_state` → `ForecastQuery.reset`. The seam is
# keyed by kind + band + herd, and a NEW WORLD REUSES BOTH HANDLES: band ids restart low and a herd id
# is derived from its species and index (`herd_red_deer_00`). So the previous world's answer matches
# the new world's composed key EXACTLY and renders as `STATE_READY` — the old world's numbers, on a
# sheet with nothing on it to say so — while a held REFUSAL is worse still, `ask` declining to re-put a
# question it holds a server token for.
#
# **NO FRAME COULD CARRY THIS.** A stale answer renders as a perfectly ordinary forecast, and the fix
# is invisible in every one of this harness's 200-odd frames; the claim is only sayable as "the seam
# answers PENDING for a key it answered READY for a moment ago".
func _assert_world_change_drops_the_answers() -> void:
	var query: ForecastQuery = h._hud.forecast_query()
	query.reset()
	var answered_subject := _seed_answer(query, GUARD_HERD_ID)
	var refused_subject := _seed_refusal(query, REFUSED_HERD_ID, SERVER_REFUSAL_TOKEN)
	# The preconditions: without them "PENDING after the reset" passes on a seam that never held
	# anything, which is the whole failure mode being guarded against.
	h._assert_hud("world boundary — the seam holds an ANSWER to clear",
		String(query.view(answered_subject, _key_for(answered_subject)).get("state", "")) \
			== ForecastQuery.STATE_READY)
	h._assert_hud("world boundary — the seam holds a REFUSAL to clear",
		String(query.view(refused_subject, _key_for(refused_subject)).get("state", "")) \
			== ForecastQuery.STATE_FAILED)
	h._hud.reset_world_state()
	h._assert_hud("world reset drops the answer (a new world reuses the band and herd ids)",
		String(query.view(answered_subject, _key_for(answered_subject)).get("state", "")) \
			== ForecastQuery.STATE_PENDING)
	h._assert_hud("world reset drops the refusal (a stale token would stick for good)",
		String(query.view(refused_subject, _key_for(refused_subject)).get("state", "")) \
			== ForecastQuery.STATE_PENDING)
	# …and the reset must leave the question ASKABLE again, which the state alone does not say: an
	# entry cleared but still refused would read PENDING forever and ask nothing.
	#
	# **THE TALLY IS AN ARRAY BECAUSE A LAMBDA CAPTURES A `bool` BY VALUE.** A captured flag set inside
	# the sender is invisible out here, so the assertion would read `false` however the seam behaved.
	var reasked: Array[String] = []
	query.set_sender(func(_request_id: int, ask: Dictionary) -> bool:
		reasked.append(String(ask.get("herd_id", "")))
		return true)
	_ask(query, refused_subject, REFUSED_HERD_ID)
	h._assert_hud("…and the same question is put again in the new world",
		reasked.has(REFUSED_HERD_ID))

# ---- THE TWO FAILURE CLASSES --------------------------------------------------------------------
# A server token names something wrong with the QUESTION, which the sheet composed itself, so re-asking
# is pointless and — `ask` running once per render — would spin the socket. `QUERY_ERROR_TRANSPORT`
# names something wrong with the SOCKET, which heals on its own, so holding it forever leaves a sheet
# reading `No forecast available (transport)` for the rest of the session after a server restart.
#
# **THE PAIR IS THE CLAIM.** A rule that never retried satisfies the first assertion alone and a rule
# that always retried satisfies the second alone, so neither is worth anything without the other.
#
# **THE CLOCK IS MOVED, NOT WAITED OUT.** `TRANSPORT_RETRY_AFTER_MSEC` is seconds and this harness
# renders a whole HUD walk; back-dating the stamp the predicate reads is the same experiment as
# sleeping through it, and costs the run nothing.
func _assert_failure_classes_differ() -> void:
	var query: ForecastQuery = h._hud.forecast_query()
	query.reset()
	var server_subject := _seed_refusal(query, GUARD_HERD_ID, SERVER_REFUSAL_TOKEN)
	var transport_subject := _seed_refusal(query, TRANSPORT_HERD_ID,
		HudComposeVocab.QUERY_ERROR_TRANSPORT)
	_backdate_failure(query, server_subject)
	_backdate_failure(query, transport_subject)
	var asked: Array[String] = []
	query.set_sender(func(_request_id: int, ask: Dictionary) -> bool:
		asked.append(String(ask.get("herd_id", "")))
		return true)
	_ask(query, server_subject, GUARD_HERD_ID)
	_ask(query, transport_subject, TRANSPORT_HERD_ID)
	h._assert_hud("a server refusal is never re-asked (the sheet composed the question)",
		not asked.has(GUARD_HERD_ID))
	h._assert_hud("a transport failure IS re-asked once the backoff has elapsed",
		asked.has(TRANSPORT_HERD_ID))
	# The sheet must not flicker while the retry flies: the failure stands until an answer lands, so
	# the player sees one transition when the server comes back rather than two.
	h._assert_hud("…and the failure still renders while the retry is in flight",
		String(query.view(transport_subject, _key_for(transport_subject)).get("state", "")) \
			== ForecastQuery.STATE_FAILED)
	# The other half of the backoff: a transport failure is not re-asked on the very NEXT render, which
	# is the socket-spinning the no-retry rule was written to prevent in the first place.
	query.reset()
	var fresh_subject := _seed_refusal(query, RETRY_FRESH_HERD_ID,
		HudComposeVocab.QUERY_ERROR_TRANSPORT)
	asked.clear()
	# **RE-INSTALL THE RECORDER**: `_seed_refusal` puts its own sender on to drive the round trip, so a
	# claim made without this one reads an empty tally whatever the seam does.
	query.set_sender(func(_request_id: int, ask: Dictionary) -> bool:
		asked.append(String(ask.get("herd_id", "")))
		return true)
	_ask(query, fresh_subject, RETRY_FRESH_HERD_ID)
	h._assert_hud("a transport failure is NOT re-asked on the next render (that would spin the socket)",
		asked.is_empty())

# ---- WHICH END OF THE PLATEAU THE STEPPER SEEDS ON ----------------------------------------------
# `HuntTripForecastReply.useful_cap` is the LAST party at which the delivered payload was still RISING
# — the sim asserts both sides of it — so `useful_cap + 1` is the first party that adds nothing and the
# cap is the figure itself. Read as "the first useless party" instead, every raid in the game goes out
# one worker short of its own plateau, and the sheet renders exactly as happily either way.
func _assert_useful_cap_is_the_last_rising_party() -> void:
	# An empty herd carries no engagement stage, so the crew FLOOR contributes nothing and the answer
	# is the scan alone — which is what this claim is about.
	var capped := SourceForecast.expedition_useful_cap({}, {}, GUARD_FLOOR, SCANNED_PLATEAU, AMPLE_IDLE)
	h._assert_hud("the raid's party cap IS the scanned plateau, not one either side of it",
		int(capped.get("cap", -1)) == SCANNED_PLATEAU)
	h._assert_hud("…and the supply side still binds below it",
		int(SourceForecast.expedition_useful_cap({}, {}, GUARD_FLOOR, SCANNED_PLATEAU,
			SCANNED_PLATEAU - 1).get("cap", -1)) == SCANNED_PLATEAU - 1)

# ---- the seam, driven directly ------------------------------------------------------------------

## The composed key for a subject, at the one (kit, party, floor) every claim here uses.
func _key_for(subject: String) -> String:
	return ForecastQuery.key_of(subject, GUARD_KIT_ID, GUARD_PARTY, GUARD_FLOOR)

## Put the hunt question for `herd_id` through the seam's real `ask`, which is the only entry point a
## sheet has and the one the retry rule lives in.
func _ask(query: ForecastQuery, subject: String, herd_id: String) -> void:
	query.ask(ForecastQuery.KIND_HUNT_TRIP, subject, _key_for(subject), {
		"faction_id": HudConst.PLAYER_FACTION_ID,
		"band_id": GUARD_BAND_ID,
		"herd_id": herd_id,
		"kit_id": GUARD_KIT_ID,
		"party_workers": GUARD_PARTY,
		"floor": GUARD_FLOOR,
		"preset_floors": SourceForecast.preset_floors(),
		"max_party_workers": AMPLE_IDLE,
	})

## Ask, then land a reply — the round trip a healthy socket makes, so the entry ends up holding an
## ANSWER rather than a hand-written one. Returns the subject.
func _seed_answer(query: ForecastQuery, herd_id: String) -> String:
	var subject := ForecastQuery.subject_of(ForecastQuery.KIND_HUNT_TRIP, GUARD_BAND_ID, herd_id)
	var landed: Array[int] = []
	query.set_sender(func(request_id: int, _ask: Dictionary) -> bool:
		landed.append(request_id)
		return true)
	_ask(query, subject, herd_id)
	for request_id in landed:
		query.deliver([{"request_id": request_id, "ok": true,
			"kind": ForecastQuery.KIND_HUNT_TRIP, "at_composed": {}, "per_preset": [],
			"useful_cap": SCANNED_PLATEAU}])
	return subject

## The same round trip, refused — through `deliver`, so the entry is failed exactly as a live reply
## fails it and the token is the one the seam would have stored.
func _seed_refusal(query: ForecastQuery, herd_id: String, token: String) -> String:
	var subject := ForecastQuery.subject_of(ForecastQuery.KIND_HUNT_TRIP, GUARD_BAND_ID, herd_id)
	var landed: Array[int] = []
	query.set_sender(func(request_id: int, _ask: Dictionary) -> bool:
		landed.append(request_id)
		return true)
	_ask(query, subject, herd_id)
	for request_id in landed:
		query.deliver([{"request_id": request_id, "ok": false, "error": token}])
	return subject

## Move a held failure's stamp back past the backoff, so the retry is due NOW. The predicate reads the
## wall clock, so this is exactly what waiting would produce and it does not stall the walk.
func _backdate_failure(query: ForecastQuery, subject: String) -> void:
	var entry: Dictionary = query._subjects.get(subject, {})
	entry["error_at"] = int(entry.get("error_at", 0)) - ForecastQuery.TRANSPORT_RETRY_AFTER_MSEC
	query._subjects[subject] = entry
