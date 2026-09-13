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

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 16

const ForecastFx := preload("res://tools/ui_preview/fixtures_forecast.gd")
const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")

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
	_assert_the_key_carries_the_bands_gear()
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

# ---- THE BAND'S GEAR IS PART OF THE QUESTION ----------------------------------------------------
# Reported from play: two ASSIGN HUNTERS sheets for the same band and herd at the same crew of 3 — one
# with three Stalking kits, one reading `1 of 3 Stalking kits available` — rendered a BYTE-IDENTICAL
# NEXT TURN panel: the same 0.23 FOOD / 0.02 BONE / 0.01 FIBRE / 0.16 HIDE, the same
# `≈0.76 Red Deer/turn`, the same *"max 3 workers useful here"*.
#
# **THE SERVER WAS NEVER WRONG.** `answer_hunt_crew_take` prices the curve off the band's live wear and
# publishes `armed_crew` precisely so the plateau can be explained. `ForecastQuery.key_of` was keyed on
# band · herd · kit · party · floor with the gear NOWHERE IN IT, and `ask` returns early whenever the
# key it holds an answer for matches — so the first answer stood for the session however the ledger
# moved. **NO FRAME COULD CARRY THIS**: a stale forecast renders as a perfectly ordinary one.

## The item the gear claims move, and the kit that carries it. `big_game` is spears AND a sled, so the
## fingerprint has two terms and a claim that only ever read the first would pass here — which is why
## the terms name their items.
const GEAR_KEY_KIT_ID := BandFx.KIT_ID_BIG_GAME
const GEAR_KEY_ITEM := BandFx.KIT_ITEM_SPEARS

## The three single-field edits the claims below are built from. Each moves ONE published number on
## ONE item row, so a key that changed for any other reason would be changing for the wrong reason.
const GEAR_KEY_STOCK_AFTER := 2
const GEAR_KEY_DEMAND_AFTER := 9.0
const GEAR_KEY_CONDITION_AFTER := 11.0

## GUARD: **the key moves with the LEDGER and not with the WEAR — and the pair is the claim.**
##
## ⛔ Either half alone passes a broken implementation. *"Stock changes the key"* is satisfied by a
## key that hashes the whole condition row, which then re-asks every turn as gear wears for an answer
## that did not move; *"condition does not change the key"* is satisfied by the shipped bug, where
## nothing about the gear changed it. Both, together, are what pin the fingerprint's contents.
##
## **WHY CONDITION IS EXCLUDED**: coverage is struck against `live_units`, a COUNT of batches with
## condition left, so a unit arms the same share at 91% as at 12% — while its `remaining` moves every
## turn. Counts move only when a unit is gained, lost or finally expires, which is when the answer
## moves too.
func _assert_the_key_carries_the_bands_gear() -> void:
	var kits := BandFx.kit_roster_fixture()
	var subject := ForecastQuery.subject_of(ForecastQuery.KIND_HUNT_CREW_TAKE, GUARD_BAND_ID,
		GUARD_HERD_ID)
	var base := BandFx.with_equipped_kit(BandFx.band_fixture())
	var base_key := _gear_key(subject, base, kits)
	# LIVENESS FIRST: a kit that carries items must produce a NON-empty fingerprint, or every
	# difference claimed below is a difference between two empty strings.
	h._assert_hud("gear key — a kit that carries items puts a fingerprint in the key (\"%s\")"
			% base_key,
		KitRoster.gear_fingerprint(kits, GEAR_KEY_KIT_ID, base) \
			!= KitRoster.GEAR_FINGERPRINT_ITEMLESS)
	# 1. THE STOCK — the band gains or loses a unit of something this kit carries.
	var restocked := _with_item_field(base, GEAR_KEY_ITEM, DetailFormat.KIT_ITEM_COUNT_KEY,
		GEAR_KEY_STOCK_AFTER)
	h._assert_hud("gear key — a change in UNITS OWNED is a new question (\"%s\" vs \"%s\")"
			% [base_key, _gear_key(subject, restocked, kits)],
		_gear_key(subject, restocked, kits) != base_key)
	# 2. THE COMPETING DEMAND — another row staffs up on the same item. After the sim's rationing
	# change a prospective row's share is `live_units × w ÷ (other_demand + w)`, so this moves the
	# answer exactly as the stock does, and a key on stock alone would leave THIS staleness behind.
	var contested := _with_item_field(base, GEAR_KEY_ITEM,
		DetailFormat.KIT_ITEM_ON_QUOTED_JOB_KEY, GEAR_KEY_DEMAND_AFTER)
	h._assert_hud("gear key — …and so is a change in the WORKERS competing for it (\"%s\")"
			% _gear_key(subject, contested, kits),
		_gear_key(subject, contested, kits) != base_key)
	# 3. THE CONDITION — and this one must NOT move the key.
	var worn := _with_item_field(base, GEAR_KEY_ITEM, DetailFormat.KIT_ITEM_REMAINING_KEY,
		GEAR_KEY_CONDITION_AFTER)
	h._assert_hud("gear key — but WEAR alone is NOT a new question, or every turn re-asks for nothing (\"%s\")"
			% _gear_key(subject, worn, kits),
		_gear_key(subject, worn, kits) == base_key)
	# 4. AN ITEMLESS KIT HAS NOTHING TO INVALIDATE ON, and its key must not move under any of the three.
	h._assert_hud("gear key — an ITEMLESS kit keys the same however the ledger moves",
		ForecastQuery.key_of(subject, BandFx.KIT_ID_NONE, GUARD_PARTY, GUARD_FLOOR, base, kits)
			== ForecastQuery.key_of(subject, BandFx.KIT_ID_NONE, GUARD_PARTY, GUARD_FLOOR,
				restocked, kits))

## The key one band composes for the gear claims — the real `key_of`, at the one kit those claims use.
func _gear_key(subject: String, band: Dictionary, kits: Array) -> String:
	return ForecastQuery.key_of(subject, GEAR_KEY_KIT_ID, GUARD_PARTY, GUARD_FLOOR, band, kits)

## A copy of `band` with ONE field of ONE `kitItemConditions` row replaced. Deep-duplicated, because
## the fixture's rows are shared dictionaries and mutating one in place would edit the band every
## other claim in this block compares against.
func _with_item_field(band: Dictionary, item_id: String, field: String, value: Variant) -> Dictionary:
	var copy := band.duplicate(true)
	for row_variant in copy.get(DetailFormat.KIT_ITEM_CONDITIONS_KEY, []):
		var row: Dictionary = row_variant
		if String(row.get(DetailFormat.KIT_ITEM_ID_KEY, "")) == item_id:
			row[field] = value
	return copy

# ---- the seam, driven directly ------------------------------------------------------------------

## The composed key for a subject, at the one (kit, party, floor) every claim here uses.
##
## **`GUARD_KIT_ID` IS NOT A ROSTER ID, and that is what makes these keys stable.**
## `ForecastQuery.key_of` folds the band's gear in through `KitRoster.gear_fingerprint`, which cannot
## look up the items of a kit the roster does not carry and answers the empty fingerprint — so the
## world-boundary and failure-class claims below are about the seam's own bookkeeping and nothing
## else, exactly as they were before the gear term existed. The gear term has its own claims, which
## use a REAL kit and a real ledger.
func _key_for(subject: String) -> String:
	return ForecastQuery.key_of(subject, GUARD_KIT_ID, GUARD_PARTY, GUARD_FLOOR,
		BandFx.band_fixture(), BandFx.kit_roster_fixture())

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
