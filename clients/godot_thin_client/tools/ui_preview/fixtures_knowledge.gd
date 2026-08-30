extends RefCounted

## THE LADDER'S KNOWLEDGE ROSTER AND ITS PROGRESS ROW, in the shapes the wire carries them
## (`snapshot_ladder_knowledge` / `snapshot_intensification_knowledge`, decoded by
## `native/src/dict/subsistence.rs`). Shared by every harness that needs a knowledge screen or names a
## discovery: `ui_preview`'s `knowledge_panel` / `turn_orb` / `herd_graze_pen` chapters and
## `band_panel_preview`.
##
## ⛔ **THE ROSTER IS THE DECLARATION NOW, AND THAT IS WHY IT IS A FIXTURE AT ALL.** The client used to
## hold the ladder's node list itself (`HudKnowledgeVocab.LADDER_DOMAINS`), so a harness needed no
## roster — it only ever pushed progress. The columns are built from the wire now: which column a
## knowledge is in (the branch of the rung that TEACHES it), where in that column (that rung's order)
## and whether it is a step or a capability (whether any rung's `unlock_knowledge` names it) all come
## off `intensification_ladder.json` sim-side. **So a harness that pushes no roster renders no ladder
## columns at all**, which is the honest consequence of the panel building itself.
##
## ⛔ **IT IS A TRANSCRIPTION OF THE SHIPPED LADDER, and it is deliberately not derived here.** A
## fixture that recomputed the roster would pass against a producer that had stopped producing one.
## The claim that this transcription MATCHES the config is the sim's
## (`core_sim/src/snapshot/mod.rs::the_published_roster_places_every_knowledge_the_ladder_teaches`);
## what this file is for is proving the CLIENT renders whatever roster arrives.

## The wire keys, spelled once. A typo in one of these is a silent empty column.
const KEY_ID := "knowledge_id"
const KEY_DISPLAY := "display_name"
const KEY_BRANCH := "branch"
const KEY_ORDER := "order"
const KEY_IS_STEP := "is_step"

## The branch tokens the sim publishes — `RungBranch::as_str`, which is also
## `HudKnowledgeVocab.DOMAIN_KEY_*`.
const BRANCH_PLANT := "plant"
const BRANCH_ANIMAL := "animal"
const BRANCH_ROUTE := "route"

## The knowledge ids the shipped ladder teaches.
const KNOWLEDGE_CULTIVATION := "cultivation"
const KNOWLEDGE_SEED_SELECTION := "seed_selection"
const KNOWLEDGE_HERDING := "herding"
const KNOWLEDGE_PENNING := "penning"
const KNOWLEDGE_FODDERING := "foddering"
const KNOWLEDGE_ROADBUILDING := "roadbuilding"
const KNOWLEDGE_PAVING := "paving"

## **THE ROSTER THE SIM PUBLISHES FOR THE SHIPPED LADDER**, in the rungs' own declaration order.
##
## Read it as: *this knowledge is taught by the rung at `order` on `branch`, and `is_step` says
## whether any rung waits on it.* `foddering` is the shipped `false` — the pen rung teaches it and no
## rung is gated by it, which is what puts it under the Herds chain rather than in it.
##
## **`roadbuilding` and `paving` are the proof of the whole arrangement**: they are taught by
## `route:trail` and `route:dirt_road`, they went onto the wire with the ladder's other five, and the
## panel grows a **Roads** column for them without a line of client code naming either.
static func ladder_roster() -> Array:
	return [
		_row(KNOWLEDGE_CULTIVATION, "Cultivation", BRANCH_PLANT, 1, true),
		_row(KNOWLEDGE_SEED_SELECTION, "Seed Selection", BRANCH_PLANT, 2, true),
		_row(KNOWLEDGE_HERDING, "Herding", BRANCH_ANIMAL, 1, true),
		_row(KNOWLEDGE_PENNING, "Penning", BRANCH_ANIMAL, 2, true),
		_row(KNOWLEDGE_FODDERING, "Foddering", BRANCH_ANIMAL, 3, false),
		_row(KNOWLEDGE_ROADBUILDING, "Roadbuilding", BRANCH_ROUTE, 2, true),
		_row(KNOWLEDGE_PAVING, "Paving", BRANCH_ROUTE, 3, true),
	]

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

static func _row(id: String, display: String, branch: String, order: int, is_step: bool) -> Dictionary:
	return {
		KEY_ID: id,
		KEY_DISPLAY: display,
		KEY_BRANCH: branch,
		KEY_ORDER: order,
		KEY_IS_STEP: is_step,
	}
