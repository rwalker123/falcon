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

# The player-facing name of each track, from the manual's vocabulary (§2a is authoritative). Also the
# order the top-bar knowledge strip renders them in: each web's own ladder, bottom rung first, so the
# strip reads as two ladders climbing rather than a list of unrelated numbers.
## **`foddering` COMES LAST AND IS NOT A RUNG TRANSITION.** The four above are one per
## rung-transition; this one is the capability the PEN rung teaches, so it reads as the animal
## ladder's continuation rather than as a sixth rung — which is exactly where the strip's order puts
## it, directly after `penning`.
const KNOWLEDGE_TRACK_LABELS := {
	"cultivation": "Cultivation",
	"seed_selection": "Seed Selection",
	"herding": "Herding",
	"penning": "Penning",
	"foddering": "Foddering",
}
# Command-feed nudge fired ONCE when a track completes: the rung it unlocks is a new verb the player
# has never seen, so learning the discovery has to say what it bought — and, since the verb is only
# HALF the story, what the verb then asks of them (a per-source meter to fill).
const KNOWLEDGE_UNLOCK_LABELS := {
	"cultivation": "Cultivation learned",
	"seed_selection": "Seed Selection learned",
	"herding": "Herding learned",
	"penning": "Penning learned",
	"foddering": "Foddering learned",
}
# NOTE: `herding` used to read "The Corral policy is now available on domesticated herds." Both
# halves were wrong after the §4.3 reshuffle — Herding gates **Tame** (rung 2) and it is **Penning**
# that gates Corral (rung 3).
const KNOWLEDGE_UNLOCK_NOTES := {
	"cultivation": "The Cultivate policy is now available on Thriving wild patches.",
	"seed_selection": "The Sow policy is now available — but only on rich, well-watered ground.",
	"herding": "The Tame policy is now available on wild herds that can be domesticated.",
	"penning": "The Corral policy is now available on herds you have tamed.",
	# The one note that names no new VERB, because this discovery unlocks none: it is what keeping a
	# pen taught your people, and what it buys is an account they could not bank before. Said in the
	# siblings' voice — what the capability bought, and where it now lands.
	"foddering": "Hay you gather now goes into the fodder store and feeds your pens.",
}

# --- Top-bar label nodes (handed in by HudLayer) ---
# --- Collaborators ---
## Where the one-shot knowledge-unlock nudge goes: `HudLayer.note_system_event`, i.e. the event
## dock's System channel. It was the retired left-dock command feed.
var _note_sink: Callable

# --- Owned state (moved off HudLayer) ---
# Per-faction intensification knowledge from the latest snapshot: entity → {cultivation, herding, …},
# each 0..1. Backs the top-bar meters AND the policy-gate reasons (via faction_knowledge()); the
# previous value is what makes the one-shot unlock nudge possible.
var _intensification_knowledge: Dictionary = {}
# "<faction>:<track>" keys already announced, so the nudge fires once.
var _knowledge_announced: Dictionary = {}
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

func _init(note_sink: Callable) -> void:
	_note_sink = note_sink

## WORLD BOUNDARY (`Main._reset_per_world_state` → `HudLayer.reset_world_state`): drop every top-bar
## cache that belongs to ONE world, then re-render each strip off the now-empty caches.
##
## THE RE-RENDER IS THE POINT, and `_intensification_knowledge` is why this method exists. A freshly
## generated world sends `intensification_knowledge: []`, and `_ingest_intensification` MERGES — an
## empty array writes nothing, so without this the strip kept showing the PREVIOUS game's
## `Herding ✔`. `_knowledge_announced` rides along because a track re-learned in the new world
## deserves its unlock nudge again.
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
	_knowledge_announced.clear()
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

## INGEST the intensification tracks — and fire the one-shot unlock nudge, which is the whole of what
## this method still DOES beyond caching. **It renders nothing** — it was the `⚒ Your people know:`
## strip until the top-right block was retired (issue #450), and the faction page's KNOWLEDGE zone
## draws the tracks now, off `faction_tracks`.
##
## The strip's own two display rules went with it: the `KNOWLEDGE_STRIP_TRACKS_PER_LINE` wrap (a
## content-sized top-bar block cannot autowrap, so a fifth track ran off the right edge) and the
## all-known cyan tint. The zone is one row per track and needs neither.
func update_intensification(intensification_variant: Variant) -> void:
	_ingest_intensification(intensification_variant)

## Capture the per-faction intensification tracks off the snapshot AND announce the moment one
## COMPLETES — the transition (`< 1.0` last snapshot, `>= 1.0` now) is exactly when a new policy
## becomes usable, and nothing else in the HUD would tell the player. One-shot per faction+track
## (`_knowledge_announced`), so it never re-fires on subsequent snapshots; a track already complete
## on the first snapshot we see (fresh connect / rehydrated save) has no prior value and is NOT
## announced — a nudge about something learned long ago is noise.
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
		var previous: Dictionary = _intensification_knowledge.get(faction, {})
		# Every track the ladder defines, off the one list — so adding a rung's knowledge is a
		# KNOWLEDGE_TRACK_LABELS entry plus a decoder field, never an edit here.
		var current := {}
		for track in KNOWLEDGE_TRACK_LABELS:
			current[track] = float(row.get(track, 0.0))
		for track in KNOWLEDGE_UNLOCK_NOTES:
			if not previous.has(track):
				continue
			if float(previous[track]) >= HudConst.KNOWLEDGE_COMPLETE:
				continue
			if float(current[track]) < HudConst.KNOWLEDGE_COMPLETE:
				continue
			_announce_knowledge_unlock(faction, String(track))
		_intensification_knowledge[faction] = current

## Post the one-shot "policy unlocked" nudge to the command feed. Player faction only — another
## faction's tech is not the player's to see, and every other intensification readout filters the
## same way; the announced set is still keyed per faction so the dedupe is correct for all of them.
func _announce_knowledge_unlock(faction: int, track: String) -> void:
	var key := "%d:%s" % [faction, track]
	if _knowledge_announced.has(key):
		return
	_knowledge_announced[key] = true
	if faction != HudConst.PLAYER_FACTION_ID:
		return
	_note_sink.call(String(KNOWLEDGE_UNLOCK_LABELS[track]), String(KNOWLEDGE_UNLOCK_NOTES[track]))

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
