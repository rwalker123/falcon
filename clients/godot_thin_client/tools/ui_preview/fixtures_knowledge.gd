extends RefCounted

## THE LADDER'S KNOWLEDGE ROSTER AND ITS PROGRESS ROW, in the shapes the wire carries them
## (`snapshot_ladder_knowledge` / `snapshot_intensification_knowledge`, decoded by
## `native/src/dict/subsistence.rs`). Shared by every harness that needs a knowledge screen or names a
## discovery: `ui_preview`'s `knowledge_panel` / `turn_orb` / `herd_graze_pen` chapters and
## `band_panel_preview`.
##
## ⛔ **THE ROSTER IS THE DECLARATION NOW, AND THAT IS WHY IT IS A FIXTURE AT ALL.** The client used to
## hold the ladder's node list itself (`HudKnowledgeVocab.LADDER_DOMAINS`), so a harness needed no
## roster — it only ever pushed progress. The domain ROWS are built from the wire now: which row a
## knowledge is in (the branch of the rung that TEACHES it), where along that row (that rung's order)
## and whether it is a step or a capability (whether any rung's `unlock_knowledge` names it) all come
## off `intensification_ladder.json` sim-side — as does the SUBJECT AREA the branch's rows are
## gathered under, off that config's `branches` table. **So a harness that pushes no roster renders no ladder
## rows at all**, which is the honest consequence of the panel building itself.
##
## ⛔ **IT IS A TRANSCRIPTION OF THE SHIPPED LADDER, and it is deliberately not derived here.** A
## fixture that recomputed the roster would pass against a producer that had stopped producing one.
## The claim that this transcription MATCHES the config is the sim's
## (`core_sim/src/snapshot/mod.rs::the_published_roster_places_every_knowledge_the_ladder_teaches`);
## what this file is for is proving the CLIENT renders whatever roster arrives.

## The wire keys, spelled once. A typo in one of these is a silent empty row.
const KEY_ID := "knowledge_id"
const KEY_DISPLAY := "display_name"
const KEY_BRANCH := "branch"
const KEY_ORDER := "order"
const KEY_IS_STEP := "is_step"
## …and the SUBJECT AREA of the branch that teaches it, one level above `KEY_BRANCH`.
const KEY_AREA := "area"

## The branch tokens the sim publishes — `RungBranch::as_str`. The first three are also
## `HudKnowledgeVocab.DOMAIN_KEY_LAND` / `_HERDS` / `_ROUTES`; the last two have no client constant
## at all and take `domain_label`'s capitalized fallback, which is the roster driving the panel.
const BRANCH_PLANT := "plant"
const BRANCH_ANIMAL := "animal"
const BRANCH_ROUTE := "route"
const BRANCH_FORESTRY := "forestry"
const BRANCH_EXTRACTION := "extraction"

## The subject-area tokens the shipped `intensification_ladder.json` `branches` table names, and
## `HudKnowledgeVocab.AREA_LABELS`' first three keys. **A transcription, like everything else here.**
const AREA_FOOD := "food"
const AREA_MAKING := "making"
const AREA_WORKS := "works"

## **THE DISPLAY ORDER THE SIM PUBLISHES** — the config's `areas` list verbatim
## (`SubsistenceSection.ladderAreas`), including the three areas no branch teaches under yet. Those
## three are the point of transcribing the whole list rather than the reachable part of it: an area
## with no domains is never drawn, so a producer that published every area would look identical to
## one that published the right ones unless the fixture carries the empty ones too.
static func ladder_areas() -> Array:
	return [AREA_FOOD, AREA_MAKING, AREA_WORKS, "reach", "lore", "war"]

## The knowledge ids the shipped ladder teaches.
const KNOWLEDGE_CULTIVATION := "cultivation"
const KNOWLEDGE_SEED_SELECTION := "seed_selection"
const KNOWLEDGE_HERDING := "herding"
const KNOWLEDGE_PENNING := "penning"
const KNOWLEDGE_FODDERING := "foddering"
const KNOWLEDGE_ROADBUILDING := "roadbuilding"
const KNOWLEDGE_PAVING := "paving"
const KNOWLEDGE_WOODCRAFT := "woodcraft"
const KNOWLEDGE_CONSERVATIONISM := "conservationism"
const KNOWLEDGE_QUARRYING := "quarrying"

## **THE ROSTER THE SIM PUBLISHES FOR THE SHIPPED LADDER**, in the rungs' own declaration order.
##
## Read it as: *this knowledge is taught by the rung at `order` on `branch`, and `is_step` says
## whether any rung waits on it.* `foddering` is the shipped `false` — the pen rung teaches it and no
## rung is gated by it, which is what puts it under the Herds chain rather than in it.
##
## **`roadbuilding` and `paving` are the proof of the whole arrangement**: they are taught by
## `route:trail` and `route:dirt_road`, they went onto the wire with the ladder's other eight, and the
## panel grows a **Roads** row for them without a line of client code naming either.
##
## ⛔ **ALL TEN, ACROSS ALL FIVE BRANCHES — a transcription missing a branch is the defect, not a
## smaller fixture.** This carried `plant` / `animal` / `route` alone for one slice, so the claims
## built on it described a four-domain screen that no server publishes: the shipped ladder teaches
## **six** domains, `forestry` and `extraction` both sitting under **Making** beside the craft fan.
## A short transcription passes against a producer that has stopped producing the rest, which is the
## one thing this file exists not to do.
static func ladder_roster() -> Array:
	return [
		_row(KNOWLEDGE_CULTIVATION, "Cultivation", BRANCH_PLANT, 1, true, AREA_FOOD),
		_row(KNOWLEDGE_SEED_SELECTION, "Seed Selection", BRANCH_PLANT, 2, true, AREA_FOOD),
		_row(KNOWLEDGE_HERDING, "Herding", BRANCH_ANIMAL, 1, true, AREA_FOOD),
		_row(KNOWLEDGE_PENNING, "Penning", BRANCH_ANIMAL, 2, true, AREA_FOOD),
		_row(KNOWLEDGE_FODDERING, "Foddering", BRANCH_ANIMAL, 3, false, AREA_FOOD),
		_row(KNOWLEDGE_ROADBUILDING, "Roadbuilding", BRANCH_ROUTE, 2, true, AREA_WORKS),
		_row(KNOWLEDGE_PAVING, "Paving", BRANCH_ROUTE, 3, true, AREA_WORKS),
		_row(KNOWLEDGE_WOODCRAFT, "Woodcraft", BRANCH_FORESTRY, 1, true, AREA_MAKING),
		_row(KNOWLEDGE_CONSERVATIONISM, "Conservationism", BRANCH_FORESTRY, 2, true, AREA_MAKING),
		_row(KNOWLEDGE_QUARRYING, "Quarrying", BRANCH_EXTRACTION, 1, true, AREA_MAKING),
	]

# ---- the two DEGENERATE rosters, one per FALLBACK ----------------------------------------------
# `docs/plan_knowledge_rows.md` §5. Both states are reachable by an incomplete config edit and both
# have to DRAW: a knowledge that vanishes because a config edit was half-finished is the worst
# failure this screen has, and it is one it has actually shipped.

## **FALLBACK 1: A BRANCH WHOSE DESCRIPTOR NAMES NO AREA.** Its `area` is the wire's own `""`, which
## is what a branch missing from the config's `branches` table publishes.
const BRANCH_UNPLACED := "salvage"
const KNOWLEDGE_UNPLACED := "scavenging"
## **FALLBACK 2: AN AREA THE CLIENT HAS NO WORD FOR.** `husbandry` is a plausible future area token
## and is deliberately absent from `HudKnowledgeVocab.AREA_LABELS`, so the heading falls back to the
## capitalized wire token exactly as an unlisted BRANCH's row name does.
const BRANCH_UNLABELLED := "dairying"
const KNOWLEDGE_UNLABELLED := "milking"
const AREA_UNLABELLED := "husbandry"

## The shipped roster plus one row per fallback — so both draw BESIDE the ordinary headings rather
## than alone, which is what makes "they still draw" a claim about placement and not just presence.
static func ladder_roster_with_fallbacks() -> Array:
	var roster := ladder_roster()
	roster.append(_row(KNOWLEDGE_UNPLACED, "Scavenging", BRANCH_UNPLACED, 1, true, ""))
	roster.append(_row(KNOWLEDGE_UNLABELLED, "Milking", BRANCH_UNLABELLED, 1, true, AREA_UNLABELLED))
	return roster

## The same roster with one knowledge taken out — **the falsification handle for "a knowledge added to
## the config appears with no client edit"**, run in the other direction because a removal is the half
## that can be proved without editing a config file. Nothing in the client names the dropped
## knowledge, so if the panel still draws it, it is drawing from something other than the roster.
static func ladder_roster_without(knowledge: String) -> Array:
	var kept: Array = []
	for row_variant in ladder_roster():
		var row: Dictionary = row_variant
		if String(row[KEY_ID]) != knowledge:
			kept.append(row)
	return kept

## Every knowledge id the roster carries, in its order — what a harness walks when it wants "the whole
## ladder" and must not restate the list.
static func ladder_track_ids(roster: Array = []) -> Array[String]:
	var ids: Array[String] = []
	for row_variant in (roster if not roster.is_empty() else ladder_roster()):
		ids.append(String((row_variant as Dictionary)[KEY_ID]))
	return ids

## What this client calls one knowledge — the roster's own `display_name`, which the sim resolves. A
## harness asserting on a discovery's NAME reads it from here rather than from a table of its own, for
## the reason the client does: one spelling.
static func label_for(knowledge: String) -> String:
	for row_variant in ladder_roster():
		var row: Dictionary = row_variant
		if String(row[KEY_ID]) == knowledge:
			return String(row[KEY_DISPLAY])
	return ""

## **ONE FACTION'S PROGRESS ROW, in the wire's own shape** — a per-faction record whose knowledges ride
## as a `{knowledge_id: 0..1}` map. It is SPARSE IN VALUE and never in membership on the real wire; a
## fixture may pass a partial map, and an absent knowledge reads `0.0`, which is exactly what an
## untouched track does.
static func progress_row(faction: int, tracks: Dictionary) -> Dictionary:
	var knowledges := {}
	for track in tracks:
		knowledges[String(track)] = float(tracks[track])
	return {"faction": faction, "knowledges": knowledges}

## Every track on the roster at one value — the "knows everything" / "knows nothing" fixture, taken
## over the roster rather than over a list here so it grows with the ladder.
static func tracks_all_at(progress: float) -> Dictionary:
	var tracks := {}
	for track in ladder_track_ids():
		tracks[track] = progress
	return tracks

static func _row(id: String, display: String, branch: String, order: int, is_step: bool,
		area: String) -> Dictionary:
	return {
		KEY_ID: id,
		KEY_DISPLAY: display,
		KEY_BRANCH: branch,
		KEY_ORDER: order,
		KEY_IS_STEP: is_step,
		KEY_AREA: area,
	}
