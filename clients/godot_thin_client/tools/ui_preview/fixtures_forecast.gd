extends RefCounted

## **THE CANNED FORECAST-QUERY ANSWERER** — what stands in for the sim on the two preview harnesses.
##
## The pre-launch raid forecasts left the snapshot and became a request/response on the command
## socket, so a HUD with no server gets no answer at all and every raid readout renders its pending
## placeholder. That is correct behaviour and a useless render gate: the frames this arc exists to
## judge are the ones with numbers on them.
##
## So the harnesses install a sender here. It answers out of the FIXTURE HERD'S OWN raid tables
## (`hunt_trip_estimates` / `denial_estimates`, still authored on the herd fixtures for exactly this
## purpose), which is why every expectation written against the table era still holds: the numbers a
## sheet renders are the numbers the fixture states, and only the path they travel changed.
##
## **THE ROUNDING LIVES HERE NOW, AND THAT IS WHERE IT BELONGS.** The sim answers the exact (floor,
## party) it is asked for; a fixture TABLE is sampled, so this stand-in reads the nearest sampled row —
## the resolution the client used to make and no longer may. A harness fixture rounding to its own
## samples is a property of the fixture; a client rounding to the sim's was a lie about the answer.
##
## **IT ANSWERS DEFERRED, NEVER INLINE.** `ForecastQuery.ask` is called during a sheet's render, and
## delivering inside that call would re-enter the render through `answered`. `call_deferred` lands the
## answer at the end of the frame instead, which is also how the real socket behaves — the reply is
## always at least a frame late.

const SourceForecastRef := preload("res://src/scripts/ui/hud/SourceForecast.gd")

## The herd fixtures' own raid tables, by the key they are authored under.
const HERD_RAID_TABLE_KEY := "hunt_trip_estimates"
const HERD_DENIAL_TABLE_KEY := "denial_estimates"

## No sampled row was found at all — the fixture herd publishes no table, which is a real state (a
## non-huntable quarry) and renders as the sheet's own "no forecast" branch.
const NO_ROW := {}

## Install the answerer on a HUD's `ForecastQuery`. Idempotent per HUD: a second install replaces the
## sender with an equivalent one.
static func install(hud: Node) -> void:
	if hud == null or not hud.has_method("forecast_query"):
		return
	var query: ForecastQuery = hud.call("forecast_query")
	query.set_sender(func(request_id: int, ask: Dictionary) -> bool:
		query.deliver.call_deferred([answer(hud, request_id, ask)])
		return true)

## One canned reply, in the shape `native/src/bridge/query.rs` decodes.
static func answer(hud: Node, request_id: int, ask: Dictionary) -> Dictionary:
	var is_denial := String(ask.get("kind", "")) == ForecastQuery.KIND_DENIAL_RAID
	var herd := _herd_for_id(hud, String(ask.get("herd_id", "")),
		HERD_DENIAL_TABLE_KEY if is_denial else HERD_RAID_TABLE_KEY)
	var reply := {"request_id": request_id, "ok": true,
		"kind": String(ask.get("kind", ""))}
	if is_denial:
		var rows: Array = herd.get(HERD_DENIAL_TABLE_KEY, [])
		reply["at_composed"] = _denial_row(rows, int(ask.get("party_workers", 0)))
		reply["party_needed"] = _party_needed(rows)
		return reply
	var table: Dictionary = herd.get(HERD_RAID_TABLE_KEY, {})
	var party := int(ask.get("party_workers", 0))
	var floor_value := float(ask.get("floor", 0.0))
	reply["at_composed"] = _raid_row(table, floor_value, party)
	var presets: Array = []
	for preset_floor in ask.get("preset_floors", []):
		presets.append(_raid_row(table, float(preset_floor), party))
	reply["per_preset"] = presets
	reply["useful_cap"] = _useful_cap(table, floor_value)
	return reply

## **THE PLATEAU, SCANNED OVER THE FIXTURE'S OWN PARTY AXIS.** The sim walks `1..=max` contiguously;
## a fixture table carries the sizes it was authored with, so this walks those. It scans the component
## the QUARRY pays — an inedible species delivers 0 food at every size, and a food-only scan would
## find no plateau and drop the stepper's cap.
static func _useful_cap(table: Dictionary, floor_value: float) -> int:
	var sampled := _nearest_floor(table, floor_value)
	var rows := _rows_at_floor(table, sampled)
	rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return int(a.get("party_workers", 0)) < int(b.get("party_workers", 0)))
	var previous := -1.0
	var plateau := 0
	for row_variant in rows:
		var row: Dictionary = row_variant
		var workers := int(row.get("party_workers", 0))
		if workers <= 0:
			continue
		var delivered := float(row.get("delivered_food", 0.0)) if bool(row.get("delivers_food", false)) \
			else float(row.get("delivered_trade", 0.0))
		if delivered > previous:
			previous = delivered
			# A payload that has not risen ABOVE ZERO is not a plateau — a raid every size comes home
			# empty from is flat at zero, and reading that flatness as "the first size was enough"
			# is what once capped the stepper at one worker.
			if delivered > 0.0:
				plateau = workers
		else:
			break
	return plateau

static func _raid_row(table: Dictionary, floor_value: float, party: int) -> Dictionary:
	var rows := _rows_at_floor(table, _nearest_floor(table, floor_value))
	return _nearest_party_row(rows, "party_workers", party)

static func _denial_row(rows: Array, party: int) -> Dictionary:
	return _nearest_party_row(rows, "party_workers", party)

## The smallest party whose row SUCCEEDED — `DenialRaidForecastReply.party_needed`'s stand-in.
##
## **THE TEST IS `denial_outcome_succeeds`, NOT "is not repelled"**, and the difference is a shipped
## defect: `horizon` is neither, so the looser test names a row whose projection merely RAN OUT as the
## party that breaks the herd. The success set lives in `SourceForecast` so the verdict copy and this
## cannot diverge.
static func _party_needed(rows: Array) -> int:
	var best := SourceForecastRef.DENIAL_PARTY_NEEDED_NONE
	for row_variant in rows:
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		if not SourceForecastRef.denial_outcome_succeeds(String(row.get("outcome", ""))):
			continue
		var workers := int(row.get("party_workers", 0))
		if workers > 0 and (best == SourceForecastRef.DENIAL_PARTY_NEEDED_NONE or workers < best):
			best = workers
	return best

static func _nearest_floor(table: Dictionary, want: float) -> float:
	var best := INF
	var best_gap := INF
	for key in table:
		var row_variant: Variant = table[key]
		if not (row_variant is Dictionary):
			continue
		var sampled := float((row_variant as Dictionary).get("floor", 0.0))
		var gap := absf(sampled - want)
		if gap < best_gap:
			best_gap = gap
			best = sampled
	return best

static func _rows_at_floor(table: Dictionary, sampled: float) -> Array:
	var rows: Array = []
	if is_inf(sampled):
		return rows
	for key in table:
		var row_variant: Variant = table[key]
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		if is_equal_approx(float(row.get("floor", 0.0)), sampled):
			rows.append(row)
	return rows

## Nearest sampled party, the LOWER of a tie — over-quoting a bigger party's take against a smaller
## one is the more misleading direction.
static func _nearest_party_row(rows: Array, party_key: String, want: int) -> Dictionary:
	var best := NO_ROW
	var best_party := 0
	var best_gap := INF
	for row_variant in rows:
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		var party := int(row.get(party_key, 0))
		if party <= 0:
			continue
		var gap := absf(float(party - want))
		if gap < best_gap or (gap == best_gap and party < best_party):
			best_gap = gap
			best_party = party
			best = row
	return best

## **THE HERD THIS ANSWER IS ABOUT — WHICHEVER OF THE TWO FIXTURE SOURCES ACTUALLY CARRIES THE TABLE.**
##
## The sim answers from one world. A harness has two places a herd can live and they disagree in
## opposite directions, so `table_key` — the table the ask needs — is what picks between them:
##
##  - A drawer sheet is HANDED its herd, so a chapter may select one without ever pushing it through
##    `update_herds`. `ui_preview` does this constantly; a roster-only lookup answers `{}` for every
##    one of those states, which renders as a herd with no forecast rather than the missing fixture.
##  - `band_panel_preview` restages ONE quarry id with a DIFFERENT table (viable, then repelled, then
##    the deep-party one) while the SELECTION still holds the dict from before the swap. A
##    selection-first lookup answers the stale table and the frame renders the state it was staged
##    out of.
##
## Preferring the source that HAS the asked-for table settles both, and settles them on the only
## evidence available: a fixture with no table cannot be what a raid readout is being judged on. Where
## both carry one the world roster wins, it being the more recent push.
static func _herd_for_id(hud: Node, herd_id: String, table_key: String) -> Dictionary:
	if herd_id.is_empty():
		return {}
	var from_roster := NO_ROW
	for entry_variant in hud._band_labor.world_herds():
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		if String(entry.get("id", "")) == herd_id:
			from_roster = entry
			break
	if not from_roster.get(table_key, {}).is_empty():
		return from_roster
	var selected: Dictionary = hud._selection.herd()
	if String(selected.get("id", "")) == herd_id \
			and not selected.get(table_key, {}).is_empty():
		return selected
	return from_roster if not from_roster.is_empty() else NO_ROW
