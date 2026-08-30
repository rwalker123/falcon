class_name FactionReadouts
extends RefCounted

## Owns the PLAYER FACTION's per-faction snapshot readouts (docs/plan_hud_decomposition.md): its
## sedentarization, its discovered Wondrous Sites and its intensification-ladder knowledge. HudLayer
## holds one as `_topbar` and delegates the snapshot `update_*` handlers to it.
##
## **IT OWNS NO NODES — it is a pure MODEL, and it was `TopBarReadouts` until it stopped being one.**
## It rendered eight Labels in the HUD's top-right block: the Sedentarization meter, the `Pop …`
## demographics line, the `◈ Discoveries N` strip, the `⚒ Your people know:` strip, plus `Turn N` and
## the `Units · Logistics · Sentiment` metrics line. Issue #450 retired that whole block — the
## Band/City dock's FACTION PAGE says all of it better (the PEOPLE bar for the demographics, the
## KNOWLEDGE zone for the other three, the turn orb's own face for `Turn N`) — and the name was
## renamed with it rather than left describing where the data used to be drawn.
##
## **THE INGEST IS THE POINT, AND IT IS WHY THE FILTER LIVES HERE.** All three wire fields are
## per-faction ARRAYS; this is the one place they are filtered to `PLAYER_FACTION_ID` and retained, so
## the faction page reads one answer through `faction_tracks` / `faction_sedentarization` /
## `faction_discovered_sites` rather than walking the arrays again and getting a second opinion about
## whose faction is being reported.
##
## It holds PURE DATA, never `_selection`/`_band_labor`. The gate helpers read this cluster's
## knowledge back through the public `faction_knowledge()`.
##
## **THE DEMOGRAPHICS PATH IS GONE OUTRIGHT, ingest and all** — the faction page's PEOPLE bar sums the
## BANDS and apportions once, so a per-faction total would be a second source of truth for the head
## count. `Main` dispatches the section nowhere and the wire field has no client reader.
##
## THE STOCKPILE PANEL LEFT THIS CLUSTER (issue #381): the left-dock card was retired for the band
## dock's Trade row, which is purely BAND-scoped (a per-turn rate, no faction stock), so no HUD cluster
## reads `faction_inventory` any more and `HudLayer.update_stockpiles` is gone with it. The snapshot key
## still has a consumer — `MapPanel.apply_update` reads it for the scenario description.
## `HudFormat.stockpile_label` went with it: its second reader, the band drawer's accessible-stock
## rows, was retired in the same pass (those rows printed the faction stockpile too).

# The knowledge meter's cell count. It is the FACTION PAGE's resolution now — `FactionRollup` reads
# this const rather than declaring a second one, so the two cannot disagree about what half-learned
# looks like. Its `METER_BAR_CELLS` sibling (the wider Sedentarization meter) went with the top-bar
# strip that was the only thing drawing at that width.
const KNOWLEDGE_METER_CELLS := 5
## The sim's own spelling of "no sedentarization stage reached" — a WIRE token, not a word anyone
## sees, and the same answer as an absent `stage`. Public because the faction page's SETTLING row
## makes the identical test and two spellings of a wire value is how two surfaces come to disagree
## about whether a faction has settled at all.
const SEDENTARIZATION_STAGE_NONE := "none"

## ⛔ **`KNOWLEDGE_TRACK_LABELS` IS RETIRED — THE LADDER'S ROSTER RIDES THE WIRE NOW.**
##
## It was a hard-coded `{track: "Name"}` table that doubled as the DECLARED track list, and its two
## jobs are both the wire's now: `ladder_knowledge` carries one row per knowledge the ladder teaches,
## with the player-facing name resolved sim-side, the branch of the rung that teaches it and that
## rung's order. That is what lets the knowledge screen build its own columns — and why the route
## branch's Roadbuilding and Paving appear with no client edit, where a table like this one had
## nowhere to put them and the panel went on saying *"All 8"*.
##
## The label is read back through `knowledge_label`; the list through `ladder_knowledge`.

## The ladder's KNOWLEDGE ROSTER as the wire sent it — an ordered array of
## `{knowledge_id, display_name, branch, order, is_step}`. **Per WORLD, not per faction**, which is the
## whole reason it is a section of its own: a faction that has learned nothing has no progress row at
## all, and a roster carried on that row would leave a new player's screen with nothing on it to say
## there was anything to learn.
var _ladder_knowledge: Array = []
## **WHAT EACH DISCOVERY LETS THE FACTION'S HANDS DO — one sentence per ladder track, and this table
## OUTLIVED THE ANNOUNCEMENT IT WAS WRITTEN FOR.**
##
## It was the body of a one-shot System-channel nudge (`_announce_knowledge_unlock`, retired with its
## `KNOWLEDGE_UNLOCK_LABELS` companion — `docs/plan_knowledge_screen.md` §5). The copy did not go with
## it: **`KnowledgeRoster` reads this table** for the knowledge screen's detail pane, under its *"What
## it lets you do"* head, and `HudKnowledgeVocab` deliberately does not re-author the sentences so the
## screen and any other surface naming a discovery cannot describe it differently.
##
## It also remains the DECLARED SET of tracks that unlock something — `foddering` is in it and gates
## no verb, which `HudKnowledgeVocab.UNLOCKLESS_TRACKS` is what states.
##
## NOTE: `herding` used to read "The Corral policy is now available on domesticated herds." Both
## halves were wrong after the §4.3 reshuffle — Herding gates **Tame** (rung 2) and it is **Penning**
## that gates Corral (rung 3).
const KNOWLEDGE_UNLOCK_NOTES := {
	"cultivation": "The Cultivate policy is now available on Thriving wild patches.",
	"seed_selection": "The Sow policy is now available — but only on rich, well-watered ground.",
	"herding": "The Tame policy is now available on wild herds that can be domesticated.",
	"penning": "The Corral policy is now available on herds you have tamed.",
	# **THE ROUTE BRANCH'S TWO LESSONS.** Each names the verb it opens, in the siblings' voice. They
	# are COPY, not structure: a knowledge with no note here still draws — the panel simply has
	# nothing to say about it — which is what keeps the roster wire-driven.
	"roadbuilding": "The Grade order is now available on trails your traffic has worn in.",
	"paving": "The Pave order is now available on dirt roads you already keep.",
	# The one note that names no new VERB, because this discovery unlocks none: it is what keeping a
	# pen taught your people, and what it buys is an account they could not bank before. Said in the
	# siblings' voice — what the capability bought, and where it now lands.
	"foddering": "Hay you gather now goes into the fodder store and feeds your pens.",
}

# --- Owned state (moved off HudLayer) ---
# Per-faction intensification knowledge from the latest snapshot: entity → {cultivation, herding, …},
# each 0..1. Backs the faction page's knowledge rows, the knowledge screen's LAND and HERDS columns
# (through `faction_tracks`) and the policy-gate reasons (through `faction_knowledge()`).
var _intensification_knowledge: Dictionary = {}
## The player faction's own sedentarization entry (`{score, stage}`) and discovered sites from the
## latest snapshot — RETAINED, not merely rendered, because the Band/City panel's faction page draws
## both in its KNOWLEDGE zone (issue #450) and must read the same answer this strip does.
##
## **THIS CLUSTER IS WHERE THE PLAYER-FACTION FILTER LIVES**, which is the whole reason the page reads
## them from here rather than off the snapshot itself: both wire fields are PER-FACTION arrays, and a
## second walk of them looking for `PLAYER_FACTION_ID` is a second chance to disagree about which
## faction is being reported. Stored as the raw entry / raw array, so what a reader gets is what the
## snapshot said.
var _sedentarization: Dictionary = {}
var _discovered_sites: Array = []

## WORLD BOUNDARY (`Main._reset_per_world_state` → `HudLayer.reset_world_state`): drop every top-bar
## cache that belongs to ONE world, then re-render each strip off the now-empty caches.
##
## THE RE-RENDER IS THE POINT, and `_intensification_knowledge` is why this method exists. A freshly
## generated world sends `intensification_knowledge: []`, and `_ingest_intensification` MERGES — an
## empty array writes nothing, so without this the strip kept showing the PREVIOUS game's
## `Herding ✔`.
##
## THE FACTION STOCKPILE IS NO LONGER RESET HERE, and no longer needs to be: it left this cluster with
## the Stockpiles card (issue #381), and the band dock's Trade row that replaced it holds no cached
## stock at all — it renders a per-turn rate straight off the band dict, which every snapshot restates.
##
## Sedentarization and demographics are rebuilt wholesale from each snapshot and so need no cache
## clearing, but they are re-rendered empty here too: they only update when their key is PRESENT, so
## a world change is the one moment stale values could otherwise persist unchallenged.
func reset_world_state() -> void:
	_intensification_knowledge.clear()
	# The roster is a per-WORLD constant, so a delta never restates it — which means a new world's own
	# roster is the only thing that can replace it, and until that arrives the previous game's ladder
	# would otherwise still be on screen.
	_ladder_knowledge.clear()
	update_intensification([])
	update_discoveries([])
	update_sedentarization([])

## INGEST the player faction's Sedentarization entry. **It renders nothing** — this was a compact
## top-bar text meter until the top-right block was retired (issue #450), and the faction page's
## KNOWLEDGE zone draws it now, off the cache below.
##
## The top bar's own `score < 1.0` hide rule went with the label, deliberately: that was a
## presentation choice for a one-line strip, and `faction_sedentarization()` hands the raw entry over
## so the caller decides what an unsettled faction reads as.
func update_sedentarization(sedentarization_variant: Variant) -> void:
	_sedentarization = {}
	if sedentarization_variant is Array:
		for entry in sedentarization_variant:
			if entry is Dictionary and int(entry.get("faction", -1)) == HudConst.PLAYER_FACTION_ID:
				_sedentarization = entry
				break

## INGEST the player faction's discovered Wondrous Sites. **It renders nothing** — this was a
## `◈ Discoveries N` count plus a strip of one mark per distinct site KIND until the top-right block
## was retired (issue #450), and the faction page's KNOWLEDGE zone lists them now, off the cache below.
##
## **THE STRIP'S TWO NUMBERS SURVIVED THE MOVE, and that is the part worth carrying forward**: the
## count is INSTANCES found and the marks were KINDS, so three peaks read `3` behind one mark and the
## pair was regularly misread as disagreeing with itself. The zone states both in full — the head
## counts instances, the rows count kinds — which is what the strip had no room to do. `WonderSprites`
## / `DISCOVERIES_UNKNOWN_GLYPH` and the art-then-emoji-then-fallback precedence went with the strip;
## the zone is a column of text rows and resolves no art.
func update_discoveries(discovered_variant: Variant) -> void:
	var sites: Array = []
	if discovered_variant is Array:
		for entry in discovered_variant:
			if entry is Dictionary and int(entry.get("faction", -1)) == HudConst.PLAYER_FACTION_ID:
				var faction_sites: Variant = entry.get("sites", [])
				if faction_sites is Array:
					sites = faction_sites
				break
	_discovered_sites = sites

## INGEST the intensification tracks, which is now the whole of what this method does. **It renders
## nothing** — it was the `⚒ Your people know:` strip until the top-right block was retired (issue
## #450), and the faction page's KNOWLEDGE zone draws the tracks now, off `faction_tracks`. **And it
## announces nothing** — the one-shot unlock nudge it used to fire is retired
## (`docs/plan_knowledge_screen.md` §5); see `_ingest_intensification` below.
##
## The strip's own two display rules went with it: the `KNOWLEDGE_STRIP_TRACKS_PER_LINE` wrap (a
## content-sized top-bar block cannot autowrap, so a fifth track ran off the right edge) and the
## all-known cyan tint. The zone is one row per track and needs neither.
func update_intensification(intensification_variant: Variant) -> void:
	_ingest_intensification(intensification_variant)

## Capture the per-faction intensification tracks off the snapshot.
##
## **IT NO LONGER ANNOUNCES ANYTHING, AND THE PREVIOUS VALUE WENT WITH THE ANNOUNCEMENT.** This ingest
## carried the client's only "a track just completed" detector: it compared each track's prior value
## against the new one and posted a one-shot `"<Track> learned"` note to the event dock's System
## channel. The turn orb's freshly-learned row supersedes it (`docs/plan_knowledge_screen.md` §5) — a
## completion is announced there and nowhere else — and two surfaces reporting one event from two
## independently-derived diffs is exactly how they come to disagree about which turn it happened on.
##
## The surviving diff is `KnowledgePanelController`'s, which asks a DIFFERENT question: not
## fire-once-ever per faction+track, but *since the turn ticked*, over BOTH knowledge webs at once and
## off the roster the screen itself draws.
func _ingest_intensification(intensification_variant: Variant) -> void:
	if not (intensification_variant is Array):
		return
	for entry in intensification_variant:
		if not (entry is Dictionary):
			continue
		var row := entry as Dictionary
		var faction := int(row.get("faction", -1))
		if faction < 0:
			continue
		# ⛔ **EVERY TRACK COMES OFF THE ROW'S OWN LIST.** The wire publishes one reading per knowledge
		# the ladder declares — sparse in VALUE, never in MEMBERSHIP — so adding a knowledge to
		# `intensification_ladder.json` needs no edit here and no client-side table of track names.
		# This walked a hard-coded label table until the ladder's knowledges became a list.
		var current := {}
		var knowledges: Variant = row.get("knowledges", {})
		if knowledges is Dictionary:
			for track in (knowledges as Dictionary):
				current[String(track)] = float((knowledges as Dictionary)[track])
		_intensification_knowledge[faction] = current

## **INGEST THE LADDER'S KNOWLEDGE ROSTER** — the `ladder_knowledge` section, retained whole.
##
## A per-world constant, so `Main` dispatches it only when it CHANGED and a non-Array leaves the last
## value standing, matching every other catalogue setter in this HUD: absence means unchanged, never
## *"this world has no ladder"*.
##
## It renders nothing here. This cluster is the one place a per-world/per-faction knowledge fact is
## retained, and the knowledge screen reads its columns off this.
func update_ladder_knowledge(roster_variant: Variant) -> void:
	if not (roster_variant is Array):
		return
	_ladder_knowledge = roster_variant

## The roster as the wire sent it, BY REFERENCE (this HUD's accessor convention; every reader is
## read-only). `[]` before any snapshot has arrived.
func ladder_knowledge() -> Array:
	return _ladder_knowledge

## **WHAT THIS CLIENT CALLS ONE KNOWLEDGE** — the sim's own `display_name`, so no surface authors a
## second spelling of a discovery's name. `""` for a knowledge the roster does not carry, which is the
## answer a caller must be able to act on rather than a name it can print.
func knowledge_label(track: String) -> String:
	for entry_variant in _ladder_knowledge:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		if String(entry.get("knowledge_id", "")) == track:
			return String(entry.get("display_name", ""))
	return ""

## A faction's progress (0..1) on one intensification track; 0 when the faction has not begun it
## (the snapshot row is sparse) or no snapshot has arrived yet. PUBLIC because the rung-gate reasons
## (`RungGates.forage_gates` / `hunt_gates`) read this cluster's knowledge back.
func faction_knowledge(faction: int, track: String) -> float:
	return float(faction_tracks(faction).get(track, 0.0))

## The WHOLE `{track: progress}` row for a faction — the value `faction_knowledge` reads one key out
## of, and the shape `RungGates` takes as its `knowledge` parameter. Public so a gate caller threads
## the row in ONCE instead of calling `faction_knowledge` per track (five tracks × two webs), and so
## the map's mark model can be derived against the same value the compose sheet gates on.
##
## Returned BY REFERENCE, matching this HUD's accessor convention. Every reader is read-only; the
## ingest in `_ingest_intensification` is the sole writer.
func faction_tracks(faction: int) -> Dictionary:
	return _intensification_knowledge.get(faction, {})

## The PLAYER faction's sedentarization entry (`{score, stage, …}`) as the snapshot sent it, or `{}`
## when it has sent none. Public for the Band/City panel's faction page, whose KNOWLEDGE zone draws
## the same figure this strip does — see `_sedentarization` for why the read belongs here and not on
## the snapshot. Returned BY REFERENCE, this HUD's accessor convention; every reader is read-only.
##
## **NOT gated on the strip's own `score < 1.0` hide rule.** That threshold is a TOP-BAR presentation
## choice — a nearly-zero score is noise on a one-line strip — and a zone with a heading has a
## different answer available to it. The caller decides what an unsettled faction reads as.
func faction_sedentarization() -> Dictionary:
	return _sedentarization

## The PLAYER faction's discovered Wondrous Sites as the snapshot sent them (`[]` when none). Public
## for the same reason and with the same reference semantics as `faction_sedentarization`.
func faction_discovered_sites() -> Array:
	return _discovered_sites
