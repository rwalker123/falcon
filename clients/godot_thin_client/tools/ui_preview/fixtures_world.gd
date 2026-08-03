## World-level fixtures: terrain legend, victory, telling and the event dock.
##
## Lifted out of `tools/ui_preview.gd` — pure, harness-free helpers, so that adding a state
## to one arc does not touch the same file as adding a state to another. See
## `.claude/rules/client/test-harnesses.md`.

const TELLING_MEDIUM_ORAL := "oral"

## Victory progress shaped as `Hud._refresh_victory_status` consumes it: no winner declared yet and
## a few modes at differing progress, so the card has real height when it is toggled on and the
## progress sort (highest first) is visible.
static func victory_state_fixture() -> Dictionary:
	return {
		"winner": {},
		"modes": [
			{"id": "cultural_ascendancy", "progress_pct": 0.42, "achieved": false},
			{"id": "great_works", "progress_pct": 0.18, "achieved": false},
			{"id": "hegemony", "progress_pct": 0.06, "achieved": false},
		],
	}

## The dock's main fixture — the proposal's own prototype vocabulary, carried on the real wire shape
## (`{tick, kind, faction, label, detail, seq}`). It spans six turns so the log has turn-groups to
## walk, covers all three rungs, both channels' worth of styling, and the three ways a row's accent
## is decided: the kind's own threat style (`predator_raid` ⚔ crimson, `hunt_danger` ⚠ amber), a
## `status=` detail token PROMOTING a routine kind to Alert (`cultivate status=feral`), and the
## plain rung defaults.
##
## The casualty rows carry the sim's REAL wire shape — `killed=` / `wounded=` written with `{:.3}`,
## never a `losses=` key the sim does not have. That fidelity is what gives the trailing-zero scan
## something to catch; a tidier invented fixture made the claim vacuous, and the precondition beside
## it said so out loud.
##
## `seq` is monotonic across the whole array, oldest first, exactly as the sim appends it.
static func event_dock_fixture() -> Array:
	return [
		{"tick": 42, "kind": "forage", "faction": 0, "label": "Foragers returned with 9 provisions", "detail": "", "seq": 1},
		{"tick": 42, "kind": "tame", "faction": 0, "label": "The aurochs herd has grown tame", "detail": "", "seq": 2},
		{"tick": 43, "kind": "born", "faction": 0, "label": "A child was born in Windhollow", "detail": "count=1", "seq": 3},
		{"tick": 43, "kind": "found_settlement", "faction": 0, "label": "Windhollow was settled", "detail": "", "seq": 4},
		{"tick": 44, "kind": "scout", "faction": 0, "label": "Two workers sent to scout the northern ridge", "detail": "", "seq": 5},
		{"tick": 44, "kind": "came_of_age", "faction": 0, "label": "A child came of age in Windhollow", "detail": "count=1", "seq": 6},
		{"tick": 44, "kind": "campaign_milestone", "faction": 0, "label": "Ashfoot has become a hamlet", "detail": "", "seq": 7},
		{"tick": 45, "kind": "corral", "faction": 0, "label": "Corral raised at Ashfoot", "detail": "", "seq": 8},
		{"tick": 45, "kind": "cultivate", "faction": 0, "label": "The upper patch has gone feral", "detail": "status=feral", "seq": 9},
		{"tick": 45, "kind": "expedition_arrived", "faction": 0, "label": "Expedition reached 24,9 — awaiting orders", "detail": "", "seq": 10},
		{"tick": 45, "kind": "died", "faction": 0, "label": "An elder died of cold in Windhollow", "detail": "cause=cold bracket=elders", "seq": 11},
		{"tick": 46, "kind": "hunt", "faction": 0, "label": "Hunters brought back red deer", "detail": "", "seq": 12},
		{"tick": 46, "kind": "born", "faction": 0, "label": "A child was born in Ashfoot", "detail": "count=1", "seq": 13},
		{"tick": 46, "kind": "site_discovered", "faction": 0, "label": "The Weeping Arch", "detail": "category=landmark at=18,31", "seq": 14},
		{"tick": 46, "kind": "hunt_danger", "faction": 0, "label": "The aurochs hunt cost the party three lives", "detail": "killed=3.000 wounded=1.000 species=Aurochs", "seq": 15},
		{"tick": 47, "kind": "sow", "faction": 0, "label": "Barley sown on the river terrace", "detail": "", "seq": 16},
		{"tick": 47, "kind": "forage", "faction": 0, "label": "Foragers returned with 12 provisions", "detail": "", "seq": 17},
		{"tick": 47, "kind": "migrated", "faction": 0, "label": "Four left Ashfoot for Windhollow", "detail": "count=4 direction=out", "seq": 18},
		{"tick": 47, "kind": "came_of_age", "faction": 0, "label": "Two children came of age in Ashfoot", "detail": "count=2", "seq": 19},
		# THE DE-DUPLICATION PAIR — byte-identical apart from `seq`. Two packs, one turn, one band.
		{"tick": 47, "kind": "predator_raid", "faction": 0, "label": "Grey wolves took two from Ashfoot", "detail": "killed=2.000 wounded=1.000 warriors=3 species=Grey Wolf", "seq": 20},
		{"tick": 47, "kind": "predator_raid", "faction": 0, "label": "Grey wolves took two from Ashfoot", "detail": "killed=2.000 wounded=1.000 warriors=3 species=Grey Wolf", "seq": 21},
	]

## Three player bands sharing the hex, spanning the food-status tiers (green /
## amber / red) and distinct activities (harvest / scout / idle glyphs).
static func occupied_units_fixture() -> Array:
	return [
		{"id": "Band Fen", "entity": 301, "faction": 0, "size": 120, "pos": [58, 24],
			"turns_of_food": 15.0, "activity": "harvest", "stores": {"provisions": 180.0}},
		{"id": "Band Ash", "entity": 302, "faction": 0, "size": 86, "pos": [58, 24],
			"turns_of_food": 7.0, "activity": "scout", "stores": {"provisions": 40.0}},
		{"id": "Band Bryn", "entity": 303, "faction": 0, "size": 54, "pos": [58, 24],
			"turns_of_food": 2.0, "activity": "idle", "stores": {"provisions": 8.0}},
	]
