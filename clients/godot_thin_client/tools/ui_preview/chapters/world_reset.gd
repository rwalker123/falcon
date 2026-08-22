extends RefCounted

## The world-boundary guard: what a new world must clear.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 28

const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## `RungGates`' third argument is the IMPROVEMENT axis, and these probes pass "this crew is building
## nothing". Named rather than a bare "" so a reader cannot mistake it for an omitted stance.
const RUNG_BUILDING_NOTHING := ""

func run(harness) -> void:
	h = harness

	# WORLD-BOUNDARY GUARD — `Hud.reset_world_state()`, the HUD half of the stale-world fix.
	# A freshly generated world sends `intensification_knowledge: []`, which MERGES to nothing, so the
	# previous game's "⚒ Your people know" strip survived into the new one; the Telling panel is
	# deliberately never reset per snapshot, so its old beats stayed in the book. Seed BOTH the way a
	# played-out world leaves them, then reset and assert they are gone. A PNG alone would not prove
	# this — a hidden strip and a strip that was never seeded look identical — so the frame is captured
	# for the eye and the two assertions carry the claim.
	h._hud._selection._selected_tile_info.clear()
	h._hud.clear_selection()
	h._hud.update_intensification([{"faction": 0, "cultivation": 0.55, "herding": 1.0}])
	h._hud._telling.reset()
	h._hud.ingest_command_events(ForageFx.telling_fixture_events())
	await h._settle()
	# **ASKED OF THE CACHE, NOT A LABEL** — the top-bar knowledge strip this used to read is retired
	# with the whole top-right block (issue #450), and the cache is the better witness anyway: it is
	# what `reset_world_state` exists to clear, and what the faction page's KNOWLEDGE zone reads.
	h._assert_hud("world-boundary guard seeds faction knowledge to clear",
		not h._hud._topbar.faction_tracks(HudConst.PLAYER_FACTION_ID).is_empty())
	h._assert_hud("world-boundary guard seeds a telling to clear",
		not h._hud._telling._entries.is_empty())
	h._hud.reset_world_state()
	await h._settle()
	h._assert_hud("world reset drops the faction's knowledge (a new world knows nothing)",
		h._hud._topbar.faction_tracks(HudConst.PLAYER_FACTION_ID).is_empty())
	h._assert_hud("world reset empties the Telling (a new world is a different story)",
		h._hud._telling._entries.is_empty())
	await h._save("world_reset")

	# ---- the RUNG-READY predicate (issue #412) ---------------------------------------------------
	# `RungGates.next_rung_ready` is what decides whether a worked source wears the ⌃ mark on the map
	# and on the work board, and it is pure — no node, no snapshot — so it is asserted DIRECTLY over
	# constructed sources rather than inferred from a picture. A PNG could not carry these claims: an
	# absent mark and a mark the renderer happened to skip look identical.
	#
	# Each pair below pins ONE of the three conditions (§3 of the design doc) by flipping exactly one
	# input, so a regression names which condition broke.
	var rr_knows_all := {"cultivation": 1.0, "seed_selection": 1.0, "herding": 1.0, "penning": 1.0}
	var rr_knows_none := {"cultivation": 0.4, "seed_selection": 0.0, "herding": 0.3, "penning": 0.0}
	var rr_wild_patch := {
		"ecology_phase": "thriving", "is_cultivated": false, "is_field": false,
		"sow_site_refusal": "too_dry",
		"composition": [{"can_cultivate": true, "can_sow": true}],
	}
	var rr_tended_sowable := {
		"ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
		"sow_site_refusal": "",
		"composition": [{"can_cultivate": true, "can_sow": true}],
	}
	# THE ORDERING FIXTURE, and it has to be a WILD patch on sowable ground. `is_cultivated` retires
	# Cultivate outright, so on a TENDED patch the two rungs are mutually exclusive and the answer is
	# Sow whichever order they are tested in — an ordering assertion there passes for the wrong reason
	# (measured: swapping the branches left it green). Sow needs NO prior patch, so this is the one
	# shape that clears BOTH gates at once and can tell the orders apart.
	var rr_wild_sowable := {
		"ecology_phase": "thriving", "is_cultivated": false, "is_field": false,
		"sow_site_refusal": "",
		"composition": [{"can_cultivate": true, "can_sow": true}],
	}
	var rr_wild_herd := {"domestication": 0.0, "husbandry_ceiling": "pen"}
	var rr_tamed_herd := {"domestication": 1.0, "husbandry_ceiling": "pen"}
	var rr_forever_wild := {"domestication": 0.0, "husbandry_ceiling": "wild"}
	# UNGATED: knowledge is the difference, nothing else.
	h._assert_hud("ready — a Thriving wild patch offers Cultivate once Cultivation is known",
		String(RungGates.next_rung_ready("forage", rr_wild_patch, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) == "cultivate")
	h._assert_hud("ready — the same patch offers NOTHING while Cultivation is unlearned",
		RungGates.next_rung_ready("forage", rr_wild_patch, RUNG_BUILDING_NOTHING, rr_knows_none).is_empty())
	# HIGHEST RUNG FIRST — the claim, on the only shape that can carry it (see rr_wild_sowable).
	h._assert_hud("ready — a wild patch clearing BOTH gates answers the HIGHER rung (Sow, not Cultivate)",
		String(RungGates.next_rung_ready("forage", rr_wild_sowable, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) == "sow")
	# And the retire rule that makes the tended case mutually exclusive in the first place.
	h._assert_hud("ready — a tended patch offers Sow, its Cultivate rung being retired as finished",
		String(RungGates.next_rung_ready("forage", rr_tended_sowable, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) == "sow")
	# OFFERED, the LAND half: the wild patch above is refused for Sow by the ground itself.
	h._assert_hud("ready — dry ground withholds Sow even with Seed Selection known",
		String(RungGates.next_rung_ready("forage", rr_wild_patch, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) != "sow")
	# NOT ALREADY RUNNING: the verb in flight is progress, not an opportunity.
	h._assert_hud("ready — a patch already being cultivated offers nothing (the verb is in flight)",
		RungGates.next_rung_ready("forage", rr_wild_patch, "cultivate", rr_knows_all).is_empty())
	# The animal web: same three conditions, one knowledge per transition.
	h._assert_hud("ready — a wild herd offers Tame once Herding is known",
		String(RungGates.next_rung_ready("hunt", rr_wild_herd, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) == "tame")
	h._assert_hud("ready — a fully tamed herd advances to Corral, not back to Tame",
		String(RungGates.next_rung_ready("hunt", rr_tamed_herd, RUNG_BUILDING_NOTHING, rr_knows_all).get("policy", "")) == "corral")
	# OFFERED, the SPECIES half: a "wild"-ceiling animal never climbs, however much we know.
	h._assert_hud("ready — a wild-ceiling species offers nothing at any knowledge level",
		RungGates.next_rung_ready("hunt", rr_forever_wild, RUNG_BUILDING_NOTHING, rr_knows_all).is_empty())
	# The mark names the rung with the SAME glyph the policy picker uses, never a private one.
	# THE RUNG UNDER WAY — the state that used to render nothing. `next_rung_ready` excludes the verb
	# in flight (a patch mid-Cultivate is progress, not an opportunity), which was right and left the
	# in-flight case unmarked, so an actively-cultivated patch looked emptier than an untouched one.
	# **THE GATE'S THIRD ARGUMENT IS THE IMPROVEMENT AXIS** (issue #442), never the harvest stance.
	var rr_building_patch := {
		"ecology_phase": "thriving", "is_cultivated": false, "is_field": false,
		"cultivation_progress": 0.42, "sow_site_refusal": "too_dry",
		"composition": [{"can_cultivate": true, "can_sow": false}],
	}
	h._assert_hud("building — a patch under Cultivate reports that verb and its meter",
		RungGates.rung_in_progress("forage", rr_building_patch, "cultivate") == \
			{"policy": "cultivate", "glyph": FoodIcons.for_policy("cultivate"), "progress": 0.42})
	# **KEYED ON THE METER, and the third argument is a pending DECLARATION**
	# (`docs/plan_standing_upkeep.md` §2.4). It was keyed on the declaration alone, and this pair
	# asserted the opposite of what it asserts now: *a meter is not work*. That was the sim's model
	# until the build verb became derived — a rung that eroded back below its cost is building again
	# with nothing declared, and the mark has to follow it or the slide is invisible on the one
	# surface a player would catch it on.
	h._assert_hud("building — a patch carrying progress is building it, declared or not",
		RungGates.rung_in_progress("forage", rr_building_patch, RUNG_BUILDING_NOTHING) == \
			RungGates.rung_in_progress("forage", rr_building_patch, "cultivate"))
	# The STANCE still answers nothing — it is not a rung of either web, so it is not a declaration
	# the derivation can honour. What answers here is the meter, which is why the claim is that a
	# stance CHANGES nothing rather than that it silences the mark.
	h._assert_hud("building — a STANCE in the improvement slot changes no answer, never a meter",
		RungGates.rung_in_progress("forage", rr_building_patch, "sustain") == \
			RungGates.rung_in_progress("forage", rr_building_patch, RUNG_BUILDING_NOTHING))
	# …and the DECLARATION half, which no meter can carry: bare ground with a verb declared on it is
	# building that verb, because a zero meter is the one state the player answers for.
	h._assert_hud("building — a declaration answers for a meter at ZERO, and only there",
		String(RungGates.rung_in_progress("forage", {}, "cultivate").get("policy", "")) == "cultivate"
			and RungGates.rung_in_progress("forage", {}, RUNG_BUILDING_NOTHING).is_empty())
	# …and a FULL meter is MAINTAINING, not building — the third state, and the edge the derivation
	# turns on. A stale declaration on it is inert rather than re-running a finished job.
	h._assert_hud("building — a meter at its cost is maintaining, whatever is declared on it",
		RungGates.rung_in_progress("forage", {"cultivation_progress": 1.0}, "cultivate").is_empty())
	# Each verb names its OWN meter — reading the wrong one would report a confident wrong number.
	h._assert_hud("building — Sow reads field_progress, not the cultivation meter beside it",
		float(RungGates.rung_in_progress("forage", {"field_progress": 0.7, "cultivation_progress": 0.2},
			"sow").get("progress", -1.0)) == 0.7)
	h._assert_hud("building — Corral reads corral_progress on a herd",
		float(RungGates.rung_in_progress("hunt", {"corral_progress": 0.25, "domestication": 1.0},
			"corral").get("progress", -1.0)) == 0.25)
	# The two answers are mutually exclusive, which is what lets one badge slot carry both states.
	h._assert_hud("building and ready are mutually exclusive on one source",
		RungGates.next_rung_ready("forage", rr_building_patch, "cultivate", rr_knows_all).is_empty() \
			and not RungGates.rung_in_progress("forage", rr_building_patch, "cultivate").is_empty())
	h._assert_hud("ready — the answer carries the policy's own glyph",
		String(RungGates.next_rung_ready("hunt", rr_tamed_herd, RUNG_BUILDING_NOTHING, rr_knows_all).get("glyph", "")) == FoodIcons.for_policy("corral"))
	# THE OFFER TWIN (issue #442) — `next_rung_offered` shares `next_rung_ready`'s ordering and differs
	# on the gate alone, which is the difference between a MARK (promises the verb is available) and the
	# compose CONTROL (teaches that the verb exists and what it costs). A gated rung must therefore be
	# absent from one and present, with its reasons, in the other.
	h._assert_hud("offered — an unlearned Cultivate is still OFFERED, so the control can teach it",
		String(RungGates.next_rung_offered("forage", rr_wild_patch, RUNG_BUILDING_NOTHING,
			rr_knows_none).get("policy", "")) == "cultivate")
	h._assert_hud("…carrying the reason that explains the lock",
		not RungGates.next_rung_offered("forage", rr_wild_patch, RUNG_BUILDING_NOTHING,
			rr_knows_none).get("reasons", []).is_empty())
	h._assert_hud("offered — the SAME rung is withheld from the READY answer a mark reads",
		RungGates.next_rung_ready("forage", rr_wild_patch, RUNG_BUILDING_NOTHING,
			rr_knows_none).is_empty())
	# Highest-first is SHARED, so the sowable-wild-ground answer must agree across both entry points.
	h._assert_hud("offered — highest rung first, exactly as the mark orders it",
		String(RungGates.next_rung_offered("forage", rr_wild_sowable, RUNG_BUILDING_NOTHING,
			rr_knows_all).get("policy", "")) == "sow")
	# A species that can never climb is withheld from BOTH — an unreachable prerequisite must never be
	# offered gated, because greying it would imply the lock could be lifted.
	h._assert_hud("offered — a wild-ceiling species is offered nothing, gated or otherwise",
		RungGates.next_rung_offered("hunt", rr_forever_wild, RUNG_BUILDING_NOTHING,
			rr_knows_none).is_empty())
