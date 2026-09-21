extends RefCounted

## THE KNOWLEDGE SCREEN — what your people know, what they are learning, and what they have earned and
## are not using (`docs/plan_knowledge_screen.md` §3, §4).
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS` lists it.
## **The order is load-bearing** — states render into one long-lived `HudLayer`, so a chapter moved is
## a set of frames changed. See `.claude/rules/client/test-harnesses.md`.
##
## ---
##
## ## MOST OF THIS CHAPTER IS PNG-LESS, AND THAT IS THE POINT OF IT
##
## Every claim this screen makes renders as a perfectly plausible PICTURE whatever it says. A filter
## pill reading `2`, a greyed row, the clause *"nothing is using it"*, a `3` on the launcher's pip —
## a screenshot cannot tell a correct one from a wrong one, and neither can a reviewer. So the
## derivation is asked of `KnowledgeRoster` directly, with models staged here, and the frames are for
## the LAYOUT alone.
##
## ## THE FIXTURES DERIVE THEIR STANDING RUNG; THEY NEVER STATE IT
##
## `SourceForecast.improvement_is_done` reads one wire field, `current_rung`, so a hand-built source
## that omits it reads as **nothing has been built here** — which is a plausible frame with every
## other assertion green. Every patch and herd below goes through `fixtures_rung.gd`, the whole test
## tree's ONE transcription of the sim's own derivation, off the same flags the row already carries.
## **No fixture in this file spells a rung key.**
##
## ## AND THE OWNERSHIP FIXTURES DO NOT HARD-CODE THE FIELD UNDER TEST
##
## The plant half's ownership test is the patch's own `owner`, and the animal half's is the band's own
## hunt ASSIGNMENTS — a herd carries no owner field client-side. So the rival-patch and rival-herd
## claims are staged by giving a patch a different faction and by leaving a herd off the band's
## assignment list, which is what the shipped scans actually read.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 145

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
## The ladder's KNOWLEDGE ROSTER and its progress row, in the wire's own shapes. Shared with the
## `turn_orb` and `herd_graze_pen` chapters, which name a discovery and must call it the same thing.
const KnowledgeFx := preload("res://tools/ui_preview/fixtures_knowledge.gd")
## The rung derivation, shared with `map_preview` / `band_panel_preview` / `snapshot_alias_guard`.
const RungFx := preload("res://tools/ui_preview/fixtures_rung.gd")
const NodeQuery := preload("res://tools/ui_preview/node_query.gd")
const InputProbe := preload("res://tools/ui_preview/input_probe.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## This chapter's band. Its own entity, so it cannot be confused with the reference band the rest of
## the run uses.
const KNOWLEDGE_BAND_ENTITY := 981
## A rival faction, for the ownership claims. Anything but `HudConst.PLAYER_FACTION_ID`.
const RIVAL_FACTION := 3

## The ladder progresses the mixed model stages. Chosen so that every one of the five filters has a
## DIFFERENT non-zero count — a fixture in which two filters coincide cannot tell them apart.
const PROGRESS_KNOWN := 1.0
## `close`'s threshold is 0.60, so this is comfortably inside it and this one comfortably outside.
const PROGRESS_CLOSE := 0.71
const PROGRESS_EARLY := 0.18

## The craft ladder's published denominator. The client draws no scale of its own, so a fixture that
## omitted this would put every craft meter at zero.
const CRAFT_THRESHOLD := 20.0
const CRAFT_PROGRESS_CLOSE := 16.0

## The three crafts the shipped roster carries, and the one recipe each that this chapter prices.
const CRAFT_TANNING := "tanning"
const CRAFT_WEAVING := "weaving"
const CRAFT_BONE := "bone_working"
const RECIPE_TUNIC := "hide_tunic"
const RECIPE_BASKET := "reed_basket"
const RECIPE_AWL := "bone_awl"
const ITEM_TUNIC := "tunic"
const ITEM_AWL := "awl"
## A recipe whose output is a MATERIAL rather than an item — the `stock` group, the other half of
## "your people are holding something made of it".
const RECIPE_LEATHER := "cure_leather"
const MATERIAL_LEATHER := "leather"

## The turn the walk starts on, and the one it advances to for the "new this turn" pair. Two DIFFERENT
## turns, because the diff is keyed on the turn changing and a re-render inside one turn must not
## re-arm it.
const TURN_FIRST := 40
const TURN_SECOND := 41
## A THIRD turn, for the knowledge-only delta: the diff has to roll on a frame that carries no
## populations section at all, and that needs a turn the diff has not already been rolled on.
const TURN_THIRD := 42

func run(harness) -> void:
	h = harness
	# ⛔ **THE ROSTER GOES IN FIRST, AND WITHOUT IT THERE IS NO LADDER AT ALL.** The rows are built
	# from the wire now, so this push is what the panel is made of — not a detail of one state.
	h._hud.update_ladder_knowledge(KnowledgeFx.ladder_roster())
	_assert_greyed_zero_tracks()
	_assert_the_roster_builds_the_rows()
	_assert_ladder_usage()
	_assert_craft_usage()
	_assert_filter_counts()
	await _assert_source_ownership()
	await _assert_new_this_turn()
	await _knowledge_frames()
	await _assert_launcher_pip()
	await _assert_opens_on_filter()
	await _assert_late_catalogue_is_not_learned()
	await _assert_loaded_world_is_not_learned()
	await _assert_a_long_domain_name_cannot_move_the_chips()
	await _assert_a_narrow_room_keeps_the_last_row_reachable()

# ---- the greyed `0.0` track --------------------------------------------------

## **A TRACK AT `0.0` IS A NODE.** The faction page's old knowledge block skipped those outright
## (`if progress <= 0.0: continue`), and that skip is what made the whole ladder invisible to a new
## player: a faction that had learned nothing rendered an EMPTY zone, so nothing on screen said there
## was anything to learn at all.
##
## Asked of an EMPTY tracks dict, which is what the wire really sends on turn one — the row is sparse,
## so the declared list is what has to be walked.
func _assert_greyed_zero_tracks() -> void:
	# The ROSTER without a progress row — which is exactly what a faction absent from the ledger sees,
	# and the reason the roster carries no faction of its own.
	var domains := KnowledgeRoster.build_domains(
		{KnowledgeRoster.MODEL_LADDER_ROSTER: KnowledgeFx.ladder_roster()})
	var nodes := KnowledgeRoster.flatten(domains)
	var declared := KnowledgeFx.ladder_roster().size()
	h._assert_hud("knowledge — a faction that knows NOTHING still renders every ladder track (%d of %d)"
			% [nodes.size(), declared],
		nodes.size() == declared)
	var not_begun := 0
	for node in nodes:
		if String(node[HudKnowledgeVocab.NODE_STATE]) == HudKnowledgeVocab.NODE_STATE_NOT_BEGUN:
			not_begun += 1
	h._assert_hud("knowledge — …and every one of them is `not begun`, i.e. GREYED rather than absent (%d of %d)"
			% [not_begun, nodes.size()],
		not_begun == nodes.size() and nodes.size() > 0)
	# **NO CRAFT ROW AT ALL when the wire has published no craft vector**, which is the "never draw
	# an empty domain row" rule. Every row's nodes come off the wire now, so this is the general
	# case rather than the craft fan's special one — the craft vector is simply the one this model
	# leaves out.
	var craft_rows := 0
	for domain in domains:
		if StringName(domain[HudKnowledgeVocab.DOMAIN_KEY]) == HudKnowledgeVocab.DOMAIN_KEY_CRAFT:
			craft_rows += 1
	h._assert_hud("knowledge — a domain with no nodes draws NO row (craft rows %d)" % craft_rows,
		craft_rows == 0)

## ⛔ **THE PANEL BUILDS ITSELF FROM THE ROSTER — THIS IS THE CLAIM THE ARC IS ABOUT.**
##
## Land and Herds used to be hard-coded node lists in `HudKnowledgeVocab`, and the reason was that
## their WIRE was hard-coded too: the ladder's knowledges rode as named float fields, so adding one
## meant adding a schema field. That is why the route branch's two lessons had nowhere to appear and
## the header went on saying *"All 8"*.
##
## Four claims, and each fails differently:
##
## 1. **A ROADS ROW EXISTS, carrying Roadbuilding and Paving** — the visible proof, because nothing
##    in the client names either knowledge and no client edit put them there.
## 2. **The row a knowledge lands in is the BRANCH of the rung that teaches it**, so Paving is on
##    Roads and Penning is on Herds.
## 3. **`foddering` is a CAPABILITY and it FALLS OUT** — no rung's `unlock_knowledge` names it, so the
##    roster says `is_step: false` and it cannot be unspent. A client-side declared set is what this
##    replaced, and the difference is that a knowledge which stops gating a rung now stops being a
##    step with nothing to remember to edit.
## 4. ⛔ **REMOVE A KNOWLEDGE FROM THE ROSTER AND THE PANEL DROPS IT, WITH NO CLIENT EDIT.** This is
##    the falsification, run in the direction that needs no config file: if the node survives, the
##    panel is drawing from something other than the wire.
func _assert_the_roster_builds_the_rows() -> void:
	var roster := KnowledgeFx.ladder_roster()
	var domains := KnowledgeRoster.build_domains({KnowledgeRoster.MODEL_LADDER_ROSTER: roster})
	var roads := _domain_by_key(domains, HudKnowledgeVocab.DOMAIN_KEY_ROUTES)
	h._assert_hud("knowledge — the ROADS row exists, built from the wire's roster alone",
		not roads.is_empty()
			and String(roads[HudKnowledgeVocab.DOMAIN_LABEL])
				== HudKnowledgeVocab.DOMAIN_BRANCH_LABELS[HudKnowledgeVocab.DOMAIN_KEY_ROUTES])
	var road_keys := _node_keys(roads)
	h._assert_hud("knowledge — …and it carries Roadbuilding then Paving, in the teaching rungs' order (%s)"
			% str(road_keys),
		road_keys == [KnowledgeFx.KNOWLEDGE_ROADBUILDING, KnowledgeFx.KNOWLEDGE_PAVING])
	# …and each is NAMED by the sim's own `display_name`, never by a client table.
	h._assert_hud("knowledge — …each named as the roster names it (`%s`)"
			% KnowledgeFx.label_for(KnowledgeFx.KNOWLEDGE_ROADBUILDING),
		_node_label(roads, KnowledgeFx.KNOWLEDGE_ROADBUILDING)
			== KnowledgeFx.label_for(KnowledgeFx.KNOWLEDGE_ROADBUILDING))

	# CLAIM 2 — the branch of the TEACHING rung decides the row. Asserted on a knowledge from each
	# ladder web, so a producer that put everything in one row fails.
	var herds := _domain_by_key(domains, HudKnowledgeVocab.DOMAIN_KEY_HERDS)
	var land := _domain_by_key(domains, HudKnowledgeVocab.DOMAIN_KEY_LAND)
	h._assert_hud("knowledge — a knowledge sits in the row of the branch that TEACHES it",
		_node_keys(land).has(KnowledgeFx.KNOWLEDGE_SEED_SELECTION)
			and _node_keys(herds).has(KnowledgeFx.KNOWLEDGE_PENNING)
			and not _node_keys(land).has(KnowledgeFx.KNOWLEDGE_PENNING))

	# CLAIM 3 — step vs capability, and it is the ROSTER's answer rather than a set in the client.
	var fodder := _node_of(herds, KnowledgeFx.KNOWLEDGE_FODDERING)
	var penning := _node_of(herds, KnowledgeFx.KNOWLEDGE_PENNING)
	h._assert_hud("knowledge — `foddering` is a CAPABILITY, so `unspent` cannot be asked of it",
		not fodder.is_empty()
			and not bool(fodder[HudKnowledgeVocab.NODE_UNSPENT_TESTABLE]))
	h._assert_hud("knowledge — …while a STEP beside it in the same row can be asked",
		not penning.is_empty() and bool(penning[HudKnowledgeVocab.NODE_UNSPENT_TESTABLE]))
	h._assert_hud("knowledge — …and the capability sorts LAST along the chain, at the end of its row (%s)"
			% str(_node_keys(herds)),
		_node_keys(herds).back() == KnowledgeFx.KNOWLEDGE_FODDERING)

	# ⛔ CLAIM 4 — THE FALSIFICATION. A knowledge dropped from the roster is dropped from the panel,
	# and nothing in the client had to change for that to be true.
	var thinner := KnowledgeRoster.build_domains({
		KnowledgeRoster.MODEL_LADDER_ROSTER:
			KnowledgeFx.ladder_roster_without(KnowledgeFx.KNOWLEDGE_PAVING)})
	var thinner_roads := _domain_by_key(thinner, HudKnowledgeVocab.DOMAIN_KEY_ROUTES)
	h._assert_hud("knowledge — a knowledge REMOVED from the roster leaves the panel, with no client edit (%s)"
			% str(_node_keys(thinner_roads)),
		_node_keys(thinner_roads) == [KnowledgeFx.KNOWLEDGE_ROADBUILDING])
	h._assert_hud("knowledge — …and the tally shrinks with it (%d → %d)"
			% [KnowledgeRoster.flatten(domains).size(), KnowledgeRoster.flatten(thinner).size()],
		KnowledgeRoster.flatten(thinner).size() == KnowledgeRoster.flatten(domains).size() - 1)

## One domain out of a built roster, `{}` when the rows hold none of that branch.
func _domain_by_key(domains: Array, key: StringName) -> Dictionary:
	for domain_variant in domains:
		var domain: Dictionary = domain_variant
		if StringName(domain[HudKnowledgeVocab.DOMAIN_KEY]) == key:
			return domain
	return {}

## A row's node keys, in the order the row draws them.
func _node_keys(domain: Dictionary) -> Array[String]:
	var keys: Array[String] = []
	for node_variant in domain.get(HudKnowledgeVocab.DOMAIN_NODES, []):
		keys.append(String((node_variant as Dictionary)[HudKnowledgeVocab.NODE_KEY]))
	return keys

func _node_of(domain: Dictionary, key: String) -> Dictionary:
	for node_variant in domain.get(HudKnowledgeVocab.DOMAIN_NODES, []):
		var node: Dictionary = node_variant
		if String(node[HudKnowledgeVocab.NODE_KEY]) == key:
			return node
	return {}

func _node_label(domain: Dictionary, key: String) -> String:
	var node := _node_of(domain, key)
	return "" if node.is_empty() else String(node[HudKnowledgeVocab.NODE_LABEL])

# ---- "is a source standing on it" -------------------------------------------

## **THE LADDER HALF OF THE UNSPENT VERDICT, as three pairs.** Each pair is the claim: one half alone
## is satisfied by a rule stuck in one position, and both halves render as a plausible row.
func _assert_ladder_usage() -> void:
	var tracks := _tracks_all_known()

	# PAIR 1 — the plant web. A TENDED patch is standing on what Cultivation unlocked; a WILD one is
	# not. Only the patch's own flags move between the two, and the rung is derived from them.
	var tended := _model(tracks, [_patch(4, 4, true, false)], [])
	var wild := _model(tracks, [_patch(4, 4, false, false)], [])
	_assert_unspent("Cultivation", "cultivation", tended, false)
	_assert_unspent("Cultivation", "cultivation", wild, true)

	# **PAIR 2 — AT OR ABOVE, WHICH IS THE ASSERTION THAT KILLS A PER-VERB-FLAG TEST.** A patch reached
	# by `Sow` carries `is_field` and NO `is_cultivated`, so a test on the done FLAGS would report a
	# faction with a working field as not using its Cultivation. The rung comparison gets it right
	# because `plant:field` stands above `plant:tended`.
	var field := _model(tracks, [_patch(4, 4, false, true)], [])
	_assert_unspent("Cultivation (a FIELD stands above Tended)", "cultivation", field, false)
	_assert_unspent("Seed Selection (the field IS its own rung)", "seed_selection", field, false)
	# …and the same field leaves Seed Selection unspent the moment it is only TENDED, which is what
	# stops the claim above passing on a rule that answers "in use" for everything.
	_assert_unspent("Seed Selection (a TENDED patch is below Field)", "seed_selection", tended, true)

	# PAIR 3 — the animal web, whose verbs scan the HERDS rather than the patches. A patch standing on
	# a plant rung must not satisfy an animal knowledge, which is what the pool split is for.
	var penned := _model(tracks, [], [_herd("pen", SourceForecast.DOMESTICATION_COMPLETE, true)])
	var tamed := _model(tracks, [], [_herd("tame", SourceForecast.DOMESTICATION_COMPLETE, false)])
	_assert_unspent("Penning", "penning", penned, false)
	_assert_unspent("Penning (a TAMED herd is below the pen)", "penning", tamed, true)
	_assert_unspent("Herding (a pen stands above pastoral)", "herding", penned, false)
	_assert_unspent("Herding (a plant patch is not an animal rung)", "herding", tended, true)

	# **A KNOWLEDGE THAT UNLOCKS NOTHING IS NEVER UNSPENT**, whatever the faction is standing on. It
	# is not a step with a verb — there is no source that could stand on it — so calling it unused
	# would be a sentence about nothing.
	_assert_unspent("Foddering (it unlocks no rung)", "foddering", _model(tracks, [], []), false)

	# **AND ONLY A KNOWN NODE CAN BE UNSPENT.** A track at 18% has nothing standing on it either, and
	# counting that would put every unlearned thing in the launcher's nudge.
	var learning := _model({"cultivation": PROGRESS_EARLY}, [_patch(4, 4, false, false)], [])
	_assert_unspent("Cultivation at 18% (unlearned, so not `unused`)", "cultivation", learning, false)

## **THE CRAFT HALF: is the faction holding, or making, anything made of this craft.**
##
## Four arms, each staged alone so a failure names which one broke, plus the two negatives that stop
## the whole thing passing on a rule that answers "in use" for every craft.
func _assert_craft_usage() -> void:
	var recipes := _recipes()
	# **THE NEGATIVE FIRST, and it is the load-bearing one.** The crafting panel publishes ONE ROW PER
	# RECIPE, ALWAYS — so if "a recipe of this craft exists" were the test it would answer *in use* for
	# every craft on every turn, forever. The recipe book is fully present here and nothing is held.
	var idle := _craft_model(recipes, {}, {}, [])
	_assert_craft_unspent("Tanning (the recipe book is present and nothing is held)", CRAFT_TANNING, idle, true)

	# ARM 1 — an ITEM the band holds. `count`, never `remaining`: a spent batch is REMOVED, so a
	# worn-out tunic and one never made both read `remaining 0`.
	_assert_craft_unspent("Tanning (the band holds a tunic)", CRAFT_TANNING,
		_craft_model(recipes, {ITEM_TUNIC: 1}, {}, []), false)
	# …and a count of ZERO is the same as holding none, which is what the `> 0` test is for.
	_assert_craft_unspent("Tanning (a batch at count 0 owns nothing)", CRAFT_TANNING,
		_craft_model(recipes, {ITEM_TUNIC: 0}, {}, []), true)

	# ARM 2 — a MATERIAL the band holds, off a `stock` recipe. Same craft, different output kind.
	_assert_craft_unspent("Tanning (the band holds cured leather)", CRAFT_TANNING,
		_craft_model(recipes, {}, {MATERIAL_LEATHER: 4.5}, []), false)

	# **ARM 3 — THE BENCH, which is what stops a faction building its first loom reading unspent for
	# the whole time it is being built.** Nothing held at all here.
	_assert_craft_unspent("Weaving (a basket is on the bench)", CRAFT_WEAVING,
		_craft_model(recipes, {}, {}, [RECIPE_BASKET]), false)

	# **THE CROSS-CRAFT NEGATIVE.** Holding an awl must not make Tanning in use — that is the `craft ==`
	# filter, and without it every craft would go in use the moment the band held anything at all.
	var holds_awl := _craft_model(recipes, {ITEM_AWL: 2}, {}, [])
	_assert_craft_unspent("Bone-working (the band holds an awl)", CRAFT_BONE, holds_awl, false)
	_assert_craft_unspent("Tanning (an AWL is not made of hide)", CRAFT_TANNING, holds_awl, true)

# ---- the filter counts ------------------------------------------------------

## **THE PILL COUNTS, BY EQUALITY, over a model whose five filters all answer DIFFERENTLY.** A fixture
## in which two counts coincide cannot tell those two filters apart, which is how a filter that
## silently answered its neighbour's question would survive.
func _assert_filter_counts() -> void:
	var model := _mixed_model()
	var nodes := KnowledgeRoster.flatten(KnowledgeRoster.build_domains(model))
	# `all` is every node: **every knowledge the ROSTER carries** — which is the whole ladder, route
	# branch included — plus the wire's craft vector. Written as a sum rather than as a literal,
	# because the roster's length is the config's and a literal here would have to be re-typed every
	# time the ladder grew, which is exactly the coupling this arc removed.
	var wanted := {
		HudKnowledgeVocab.FILTER_ALL: KnowledgeFx.ladder_roster().size() + _craft_knowledge_mixed().size(),
		HudKnowledgeVocab.FILTER_LEARNING: 3,
		HudKnowledgeVocab.FILTER_CLOSE: 2,
		HudKnowledgeVocab.FILTER_UNUSED: 1,
		HudKnowledgeVocab.FILTER_NEW: 1,
	}
	for key in wanted:
		var got := KnowledgeRoster.count_matching(nodes, key)
		h._assert_hud("knowledge — the `%s` pill counts %d (got %d)" % [key, int(wanted[key]), got],
			got == int(wanted[key]))
	# **`close` IS A SUBSET OF `learning`, NOT OF EVERYTHING.** A known node sits at 1.0 and would pass
	# a bare `progress >= CLOSE_FRACTION`, which would put every FINISHED track in a filter whose whole
	# question is "what would finish if I kept at it".
	var known_in_close := 0
	for node in nodes:
		if String(node[HudKnowledgeVocab.NODE_STATE]) == HudKnowledgeVocab.NODE_STATE_KNOWN \
				and KnowledgeRoster.matches(node, HudKnowledgeVocab.FILTER_CLOSE):
			known_in_close += 1
	h._assert_hud("knowledge — a KNOWN track is never `close` (%d leaked)" % known_in_close,
		known_in_close == 0)
	# …behind the precondition that there ARE known tracks for one to have leaked from.
	var tally := KnowledgeRoster.tally(nodes)
	h._assert_hud("knowledge — …and the model really holds known tracks, so that is not vacuous (%d)"
			% int(tally[HudKnowledgeVocab.NODE_STATE_KNOWN]),
		int(tally[HudKnowledgeVocab.NODE_STATE_KNOWN]) > 0)
	# The header's tally is taken over the SAME flattened list the rows draw, so the three state
	# counts must partition it exactly — a tally that had drifted onto its own walk would not.
	var summed := int(tally[HudKnowledgeVocab.NODE_STATE_KNOWN]) \
		+ int(tally[HudKnowledgeVocab.NODE_STATE_LEARNING]) \
		+ int(tally[HudKnowledgeVocab.NODE_STATE_NOT_BEGUN])
	h._assert_hud("knowledge — the tally's three states partition the node list (%d of %d)"
			% [summed, nodes.size()],
		summed == nodes.size())

# ---- which sources are the faction's ----------------------------------------

## **A RIVAL'S GROUND AND A HERD NOBODY WORKS ARE NOT THE FACTION'S**, asked of the CONTROLLER, which
## is where the two resolutions live — and they are different resolutions, forced by the wire: a patch
## carries `owner`, a herd carries nothing at all client-side.
func _assert_source_ownership() -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	var band := _band([_hunt_assignment("worked")])
	h._hud.update_band_alerts([band])
	h._hud.update_intensification([_wire_tracks(_tracks_all_known())])
	# **A TENDED PATCH THE RIVAL OWNS, beside one the player owns, one turn apart** — the pair is the
	# claim: a scan that ignored ownership passes the first half, and one that dropped every patch
	# passes the second.
	h._hud.update_forage_patches([_patch(6, 6, true, false, RIVAL_FACTION)])
	await h._settle()
	_assert_controller_unspent("Cultivation (the tended patch is a RIVAL's)", "cultivation", controller, true)
	h._hud.update_forage_patches([_patch(6, 6, true, false)])
	await h._settle()
	_assert_controller_unspent("Cultivation (…and the player's own tended patch counts)", "cultivation",
		controller, false)

	# **THE HERD HALF IS THE BAND'S OWN HUNT ASSIGNMENTS**, because a herd carries no owner field. The
	# pen the band works counts; an identical pen it does not work is invisible, which is the same
	# blindness every other herd-scoped producer in this HUD has.
	h._set_world_herds([_herd("worked", SourceForecast.DOMESTICATION_COMPLETE, true),
		_herd("unworked", SourceForecast.DOMESTICATION_COMPLETE, true)])
	await h._settle()
	_assert_controller_unspent("Penning (the band works a pen)", "penning", controller, false)
	h._hud.update_band_alerts([_band([])])
	await h._settle()
	_assert_controller_unspent("Penning (an unworked pen is not attributable)", "penning",
		controller, true)

# ---- "new this turn" --------------------------------------------------------

## **THE FIRST OBSERVATION LEARNS NOTHING, AND THE SECOND TURN DOES.** A fresh connect or a rehydrated
## save arrives with tracks already complete and no prior value to compare them against, so a diff
## that reported on its first pass would light up every discovery a returning player ever made.
##
## Three claims, and the middle one is what stops the first passing on a diff that never fires:
## nothing on the first observation, the newly-finished track on the next TURN, and nothing again on a
## second snapshot INSIDE that turn.
func _assert_new_this_turn() -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	controller.reset_world_state()
	h._hud.update_overlay(TURN_FIRST, {})
	h._hud.update_intensification([_wire_tracks({"cultivation": PROGRESS_KNOWN})])
	h._hud.update_band_alerts([_band([])])
	await h._settle()
	h._assert_hud("knowledge — the FIRST observation reports nothing new (%s)"
			% str(controller.learned_this_turn().keys()),
		controller.learned_this_turn().is_empty())
	h._hud.update_overlay(TURN_SECOND, {})
	h._hud.update_intensification([_wire_tracks({
		"cultivation": PROGRESS_KNOWN, "herding": PROGRESS_KNOWN})])
	h._hud.update_band_alerts([_band([])])
	await h._settle()
	var learned := controller.learned_this_turn()
	h._assert_hud("knowledge — the next TURN reports exactly the track that finished (%s)"
			% str(learned.keys()),
		learned.size() == 1 and learned.has("herding"))
	# A second snapshot inside one turn must not wipe what the turn has already taught — the baseline
	# is the TURN's, not the frame's, and the server re-captures after every command.
	h._hud.update_band_alerts([_band([])])
	await h._settle()
	h._assert_hud("knowledge — a second snapshot in the SAME turn keeps it (%s)"
			% str(controller.learned_this_turn().keys()),
		controller.learned_this_turn().has("herding"))
	# **AND "NEW THIS TURN" IMPLIES KNOWN.** The diff is a set of keys, and a world boundary or a
	# rehydrated save can hand back a roster in which one of them is no longer complete — a node marked
	# new while reading `not begun` is a sentence about nothing. Staged by pushing an EMPTY tracks row
	# with the diff still holding `herding`, which is the exact shape that rendered `New this turn 1`
	# over a faction that knew nothing. The diff itself must SURVIVE, or this passes because the set was
	# cleared rather than because the node was judged.
	h._hud.update_intensification([_wire_tracks({})])
	await h._settle()
	var fresh := KnowledgeRoster.count_matching(
		KnowledgeRoster.flatten(controller.domains()), HudKnowledgeVocab.FILTER_NEW)
	h._assert_hud("knowledge — a track the diff holds but the roster no longer KNOWS is not new (%d)" % fresh,
		fresh == 0)
	h._assert_hud("knowledge — …and the diff itself still holds it, so that is not vacuous (%s)"
			% str(controller.learned_this_turn().keys()),
		controller.learned_this_turn().has("herding"))
	# **AND THE DIFF ROLLS ON A KNOWLEDGE-ONLY DELTA, not just on the populations seam.** `Main`
	# dispatches each section independently and only when it CHANGED, so a turn that finishes a track
	# and moves nobody never reaches `update_band_alerts` — which was the ONE seam `refresh_snapshot`
	# was called from, so the pip moved while the diff (and an open panel's rows) stayed a turn
	# behind. Pushed with NO `update_band_alerts` at all, which is what makes this a claim about the
	# section rather than about the frame. The `update_overlay` beside it is not a convenience: it is
	# what carries the turn, and `Main` dispatches it ahead of every gated section, so the diff has
	# this turn to roll against by the time the knowledge lands.
	h._hud.update_overlay(TURN_THIRD, {})
	h._hud.update_intensification([_wire_tracks({
		"cultivation": PROGRESS_KNOWN, "herding": PROGRESS_KNOWN, "penning": PROGRESS_KNOWN})])
	await h._settle()
	var rolled := controller.learned_this_turn()
	h._assert_hud("knowledge — a knowledge-only delta rolls the DIFF too, not just the pip (%s)"
			% str(rolled.keys()),
		rolled.size() == 1 and rolled.has("penning"))

# ---- the frames -------------------------------------------------------------

## The LAYOUT, which is the one thing a picture is the right witness for: the ladder rows with the
## rails between their chips, a craft fan with none, the filter pills, and the inline reading.
func _knowledge_frames() -> void:
	h._hud.update_band_alerts([_band([_hunt_assignment("worked")])])
	h._hud.update_forage_patches([_patch(6, 6, true, false)])
	h._set_world_herds([_herd("worked", SourceForecast.DOMESTICATION_COMPLETE, true)])
	h._hud.update_crafting_catalogues([], [], _recipes(), _craft_knowledge_mixed())
	h._hud.update_intensification([_wire_tracks(_tracks_mixed())])
	await h._settle()

	# STATE 1 — the whole screen. A known track, one close, one barely begun, one untouched, and the
	# craft fan under them.
	h._hud.open_knowledge_panel()
	await h._settle()
	_assert_panel_renders()
	await h._save("knowledge_panel")

	# **STATE 2 — THE FACTION THAT KNOWS NOTHING**, which is the frame the whole arc is about: every
	# node drawn and greyed, so a new player can see there is something to learn. The old faction-page
	# block rendered this state as an EMPTY zone.
	h._hud.update_intensification([_wire_tracks({})])
	h._hud.update_crafting_catalogues([], [], _recipes(), _craft_knowledge_untouched())
	await h._settle()
	await h._save("knowledge_panel_untouched")

	# STATE 3 — a node SELECTED, so the inline reading's three sections render, under the row that
	# owns it: what it lets you do, where it stands now, and how it is learned.
	h._hud.update_intensification([_wire_tracks(_tracks_mixed())])
	h._hud.update_crafting_catalogues([], [], _recipes(), _craft_knowledge_mixed())
	await h._settle()
	var chip := NodeQuery.find_meta_node(h._hud.knowledge_panel().panel(), HudKnowledgeVocab.NODE_META)
	h._assert_hud("knowledge — a node chip is a control the harness can find by identity",
		chip != null)
	# **THE CARD'S SIZE IS RECORDED BEFORE THE FIRST PRESS**, because the claim the whole layout rests
	# on is that opening and closing a reading does not move it. See `_assert_card_does_not_breathe`.
	await _assert_card_does_not_breathe()
	# **DRIVEN AS A REAL POINTER PRESS, never `pressed.emit()`.** A chip is a `PanelContainer` with a
	# `gui_input` handler (a Button is not a Container, so it could not lay its face out), and it has no
	# signal of its own to fake — but the rule is the harness contract's either way: an emitted signal
	# calls the connected lambda by hand and passes on a control that is covered, zero-size or filtered
	# out of the hit test, which is exactly the shape this chip shipped in first.
	await _press_node("cultivation")
	_assert_detail_pane()
	_assert_detail_sits_under_its_own_row("cultivation")
	await h._save("knowledge_panel_detail")
	await _assert_selection_toggles()

	# **STATE 4 — A FILTER LIVE, so the DIMMING is in a frame.** Non-matching nodes keep their place
	# at `FILTERED_OUT_ALPHA`; the shape of the tree is most of what this screen teaches.
	var pill := NodeQuery.find_meta_node(h._hud.knowledge_panel().panel(), HudKnowledgeVocab.FILTER_META)
	h._assert_hud("knowledge — a filter pill is a control the harness can find by identity",
		pill != null)
	await _press_filter(HudKnowledgeVocab.FILTER_LEARNING)
	_assert_filter_dims_rather_than_hides()
	await h._save("knowledge_panel_filtered")

	await _assert_the_empty_filter_note_does_not_move_the_card()
	await _assert_escape_closes_only_the_reading()
	h._hud.close_knowledge_panel()
	await h._settle()
	await _assert_the_card_survives_a_planned_domain_count()

## The rows are there, the ladder domains draw their rails and the craft fan does not, and a `0.0`
## track is on screen as a chip rather than absent.
func _assert_panel_renders() -> void:
	var panel: KnowledgePanel = h._hud.knowledge_panel().panel()
	h._assert_hud("knowledge — the panel is open", panel != null and panel.is_open())
	if panel == null:
		return
	h._assert_hud("knowledge — the LAND row rendered",
		NodeQuery.has_label_containing(panel, "Land".to_upper()))
	h._assert_hud("knowledge — the CRAFT row rendered",
		NodeQuery.has_label_containing(panel, HudKnowledgeVocab.DOMAIN_CRAFT_LABEL.to_upper()))
	# **THE RAIL IS THE DOMAIN'S SHAPE, DRAWN**, and it is the one thing that says the ladder rows are
	# ORDERED. Rotated ninety degrees it is the CONNECTOR between two chips, but the claim is
	# unchanged: a ladder domain draws one; the craft fan must not.
	var land := _domain_node(panel, HudKnowledgeVocab.DOMAIN_KEY_LAND)
	var craft := _domain_node(panel, HudKnowledgeVocab.DOMAIN_KEY_CRAFT)
	h._assert_hud("knowledge — a LADDER row draws its rail",
		land != null and NodeQuery.find_meta_node(land, HudKnowledgeVocab.RAIL_META) != null)
	h._assert_hud("knowledge — …and the CRAFT fan draws none",
		craft != null and NodeQuery.find_meta_node(craft, HudKnowledgeVocab.RAIL_META) == null)
	# **AN UNTOUCHED TRACK IS A DRAWN CHIP, GREYED.** The `not begun` WORD moved into the reading's
	# state line when the chip replaced the row (there is no room for a value cell on a chip), so the
	# claim is made against the carrier the chip has: the node is in the tree, wearing the `not_begun`
	# glyph. A track that had been skipped would have neither.
	var untouched := _node_chip(panel, KnowledgeFx.KNOWLEDGE_ROADBUILDING)
	h._assert_hud("knowledge — an untouched track is drawn as a chip (`%s`)"
			% KnowledgeFx.KNOWLEDGE_ROADBUILDING,
		untouched != null)
	h._assert_hud("knowledge — …wearing the `%s` glyph `%s`, i.e. GREYED rather than absent"
			% [HudKnowledgeVocab.NODE_STATE_NOT_BEGUN,
				HudKnowledgeVocab.NODE_GLYPHS[HudKnowledgeVocab.NODE_STATE_NOT_BEGUN]],
		untouched != null and NodeQuery.has_label_containing(untouched,
			HudKnowledgeVocab.NODE_GLYPHS[HudKnowledgeVocab.NODE_STATE_NOT_BEGUN]))

## The detail pane's three heads, once a node is selected. **The unlock copy is `FactionReadouts`'
## own table**, which OUTLIVED the one-shot announcement it was written for: that note is retired and
## this pane is the table's reader now, so this also pins that the panel reads it rather than
## re-authoring a second wording of the same sentence.
func _assert_detail_pane() -> void:
	var panel: KnowledgePanel = h._hud.knowledge_panel().panel()
	if panel == null:
		h._assert_hud("knowledge detail — the panel is open", false)
		return
	h._assert_hud("knowledge detail — `%s` is a head" % HudKnowledgeVocab.DETAIL_HEAD_PRACTISE,
		NodeQuery.has_label_containing(panel, HudKnowledgeVocab.DETAIL_HEAD_PRACTISE.to_upper()))
	# **THE PRACTISE LINE EXISTS NOWHERE ELSE IN THE CLIENT**, so this is the one place it can be
	# wrong. Matched against the vocabulary's own sentence for the track the harness selected.
	var practise := String(HudKnowledgeVocab.PRACTISE_NOTES["cultivation"])
	h._assert_hud("knowledge detail — the PRACTISE line is the authored sentence",
		NodeQuery.has_label_containing(panel, practise))
	var unlock := String(FactionReadouts.KNOWLEDGE_UNLOCK_NOTES["cultivation"])
	h._assert_hud("knowledge detail — the unlock line is `FactionReadouts`' own, not a second copy",
		NodeQuery.has_label_containing(panel, unlock))

## **DIM, NEVER HIDE.** The non-matching rows are still in the tree at `FILTERED_OUT_ALPHA`, which is
## what keeps the shape of the tree legible while a filter is on. Asserted as a PAIR: a matching row
## at full opacity beside a non-matching one faded, since a renderer that faded everything satisfies
## the second half alone.
func _assert_filter_dims_rather_than_hides() -> void:
	var panel: KnowledgePanel = h._hud.knowledge_panel().panel()
	if panel == null:
		h._assert_hud("knowledge filter — the panel is open", false)
		return
	var model := _mixed_model()
	var nodes := KnowledgeRoster.flatten(KnowledgeRoster.build_domains(model))
	var rendered := 0
	var dimmed := 0
	var bright := 0
	for node in nodes:
		# **READ OFF THE CHIP ITSELF.** It used to be read off a `VBoxContainer` host walked up to from
		# the row; there is no row host any more, and the chip is what carries the `modulate`.
		var chip := _node_chip(panel, String(node[HudKnowledgeVocab.NODE_KEY]))
		if chip == null:
			continue
		rendered += 1
		var alpha := chip.modulate.a
		if alpha < 1.0:
			dimmed += 1
		else:
			bright += 1
	h._assert_hud("knowledge filter — every node is STILL RENDERED under a filter (%d of %d)"
			% [rendered, nodes.size()],
		rendered == nodes.size())
	h._assert_hud("knowledge filter — the non-matching ones are DIMMED (%d) and the matching ones are not (%d)"
			% [dimmed, bright],
		dimmed > 0 and bright > 0)

# ---- the row layout's own claims (`docs/plan_knowledge_rows.md`) ------------

## **A ZERO-MATCH FILTER MUST NOT MOVE THE CARD EITHER.** §4 words its claim about READINGS, and the
## intent is the card: this one is centred in its room, so anything that changes its height on a press
## is a lurch in both directions from the middle of the screen. The empty-filter note is a child of
## `_header`, and `_header_height()` feeds `fit_to_content` — so a caption mounted on a filter press
## is exactly the same failure the reading's reserve exists to prevent, arriving through the other
## surface.
##
## **`new` IS THE FILTER THAT IS GENUINELY EMPTY ON THIS BLOCK'S MODEL.** Nothing completes on the
## turn these frames render, so the pill reads `New this turn 0` while `Learning now` reads 3 — which
## makes this a real before/after on ONE card rather than two cards compared. Both counts are asserted
## as preconditions, because a fixture in which `new` had quietly gained a member would make the whole
## block a measurement of a filter that matches something.
##
## **The note is found by `EMPTY_NOTE_META`, never by its text** — the wording is copy and the claim is
## about the control — and the walk presses BACK afterwards, which is what says the note really came
## and went rather than being a permanent fixture the size claim was measured around.
func _assert_the_empty_filter_note_does_not_move_the_card() -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	var panel: KnowledgePanel = controller.panel()
	if panel == null:
		h._assert_hud("knowledge empty-filter — the panel is open", false)
		return
	var nodes := controller.nodes()
	var empty_count := KnowledgeRoster.count_matching(nodes, HudKnowledgeVocab.FILTER_NEW)
	var live_count := KnowledgeRoster.count_matching(nodes, HudKnowledgeVocab.FILTER_LEARNING)
	h._assert_hud("knowledge empty-filter — `%s` really matches nothing and `%s` really matches something (%d, %d)"
			% [HudKnowledgeVocab.FILTER_NEW, HudKnowledgeVocab.FILTER_LEARNING,
				empty_count, live_count],
		empty_count == 0 and live_count > 0)
	h._assert_hud("knowledge empty-filter — …and no note is on screen before the press",
		NodeQuery.find_meta_node(panel, HudKnowledgeVocab.EMPTY_NOTE_META) == null)

	var before := panel.size
	var before_card := panel.card().size
	await _press_filter(HudKnowledgeVocab.FILTER_NEW)
	h._assert_hud("knowledge empty-filter — the zero-match filter renders its note",
		NodeQuery.find_meta_node(panel, HudKnowledgeVocab.EMPTY_NOTE_META) != null)
	h._assert_hud("knowledge empty-filter — …and the panel does not resize around it (%s → %s)"
			% [str(before), str(panel.size)],
		panel.size.is_equal_approx(before))
	h._assert_hud("knowledge empty-filter — …nor does the card inside it (%s → %s)"
			% [str(before_card), str(panel.card().size)],
		panel.card().size.is_equal_approx(before_card))
	# ⛔ **THE NOTE RIDES THE FILTER ROW, SO WIDTH IS THE TERM IT COULD BREAK.** `_header` is outside
	# the scroll, so its minimum reaches the card — and Godot renders a Control at its combined
	# minimum whatever the fit asked for, which is how the old clamp was overruled. The title row is
	# the wider of the two today; a longer clause or a sixth pill is what would change that.
	var minimum := panel.card().get_combined_minimum_size().x
	h._assert_hud("knowledge empty-filter — …and the note has not pushed the card's minimum past `PANEL_WIDTH` (%.0f <= %.0f)"
			% [minimum, HudKnowledgeVocab.PANEL_WIDTH],
		minimum <= HudKnowledgeVocab.PANEL_WIDTH)
	# **THE PLACEMENT IS A LAYOUT CLAIM, so it gets a picture.** Every assertion above is about height
	# and identity; whether the note reads as a remark on the pills or as a crowded sixth one is the
	# one thing only a frame can answer.
	await h._save("knowledge_panel_empty_filter")

	# Back to the filter the frame was left on, which restores the walk AND makes the claim above
	# non-vacuous: a note that never went away would be part of both measurements.
	await _press_filter(HudKnowledgeVocab.FILTER_LEARNING)
	h._assert_hud("knowledge empty-filter — the note goes away again with the filter",
		NodeQuery.find_meta_node(panel, HudKnowledgeVocab.EMPTY_NOTE_META) == null)
	h._assert_hud("knowledge empty-filter — …and the card is where it started (%s → %s)"
			% [str(before_card), str(panel.card().size)],
		panel.card().size.is_equal_approx(before_card))

## **THE CARD DOES NOT RESIZE AS READINGS OPEN AND CLOSE — the claim the whole layout rests on.**
##
## A reading is wider and taller than a bare ladder row, so a card fitted to its CONTENT narrows on
## every close and widens on every open; on a card centred in its room that is a lurch in both
## directions from the middle of the screen, on every click (§4). Two things make it not happen —
## `refit` applies `PANEL_WIDTH` rather than the content's demand, and the detail block is mounted in
## BOTH states at `DETAIL_BLOCK_MIN_HEIGHT` — and this asserts the consequence rather than either
## mechanism, so it survives a different implementation of the same promise.
##
## **BOTH AXES, AND THE TOGGLE BACK AS WELL.** Width alone would pass with the height reserve
## deleted; opening alone would pass with a card that grew on the open and never came back.
func _assert_card_does_not_breathe() -> void:
	var panel: KnowledgePanel = h._hud.knowledge_panel().panel()
	if panel == null:
		h._assert_hud("knowledge size — the panel is open", false)
		return
	var closed_size := panel.size
	var closed_card := panel.card().size
	await _press_node("cultivation")
	var open_size := panel.size
	var open_card := panel.card().size
	h._assert_hud("knowledge size — the panel does not resize when a reading OPENS (%s → %s)"
			% [str(closed_size), str(open_size)],
		open_size.is_equal_approx(closed_size))
	h._assert_hud("knowledge size — …nor does the card inside it (%s → %s)"
			% [str(closed_card), str(open_card)],
		open_card.is_equal_approx(closed_card))
	# …and back again, through the TOGGLE: the same chip pressed a second time.
	await _press_node("cultivation")
	h._assert_hud("knowledge size — …and it comes back to the same size when the reading CLOSES (%s → %s)"
			% [str(open_size), str(panel.size)],
		panel.size.is_equal_approx(closed_size))
	h._assert_hud("knowledge size — …card too (%s → %s)" % [str(open_card), str(panel.card().size)],
		panel.card().size.is_equal_approx(closed_card))
	# ⛔ **AND IT IS THE TALLEST READING THE RESERVE HAS TO COVER, NOT THE ONE THE FRAME HAPPENS TO
	# OPEN.** `DETAIL_BLOCK_MIN_HEIGHT` is a MINIMUM: a reading whose three columns wrap past it makes
	# the block taller than the reserve, and the card breathes again for that node alone — which would
	# be a defect nobody ever sees on `cultivation`. So every node on the roster is opened in turn and
	# the card is required not to move for any of them.
	var tallest := ""
	var tallest_size := closed_size
	for node in h._hud.knowledge_panel().nodes():
		var key := String(node[HudKnowledgeVocab.NODE_KEY])
		await _press_node(key)
		if panel.size.y > tallest_size.y:
			tallest = key
			tallest_size = panel.size
		await _press_node(key)
	h._assert_hud("knowledge size — no reading on the roster makes the card grow (worst `%s` at %s, floor %s)"
			% [tallest, str(tallest_size), str(closed_size)],
		tallest == "")

## **SELECTION IS A TOGGLE, AND ONLY EVER ONE READING IS OPEN** (§4).
##
## Entered with `cultivation` open (the detail frame's state). Pressing the OPEN chip clears the
## selection and leaves the block MOUNTED, reading nothing — the reserve that stops the card
## breathing is the block's own, so the close must not unmount it; pressing a DIFFERENT chip moves the
## reading rather than opening a second one — which is asserted by COUNTING the blocks, because a
## renderer that appended a second one produces a perfectly ordinary-looking card with two paragraphs
## in it.
func _assert_selection_toggles() -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	var panel: KnowledgePanel = controller.panel()
	if panel == null:
		h._assert_hud("knowledge toggle — the panel is open", false)
		return
	h._assert_hud("knowledge toggle — precondition: `cultivation` is the open reading (got `%s`)"
			% controller._selected,
		controller._selected == "cultivation")
	await _press_node("cultivation")
	h._assert_hud("knowledge toggle — pressing the OPEN chip clears the selection (got `%s`)"
			% controller._selected,
		controller._selected == "")
	var closed_blocks := _detail_blocks(panel)
	h._assert_hud("knowledge toggle — …and the block stays MOUNTED with nothing open (%d)"
			% closed_blocks.size(),
		closed_blocks.size() == 1)
	h._assert_hud("knowledge toggle — …carrying no key (`%s`)"
			% (String(closed_blocks[0].get_meta(HudKnowledgeVocab.DETAIL_META, "")) \
				if closed_blocks.size() == 1 else "<no block>"),
		closed_blocks.size() == 1 \
			and String(closed_blocks[0].get_meta(HudKnowledgeVocab.DETAIL_META, "")) == "")
	# A DIFFERENT chip MOVES the reading. `herding` sits on another domain, so this is also the leg
	# that proves the block travels between rows rather than staying where the last one was.
	await _press_node("cultivation")
	await _press_node("herding")
	h._assert_hud("knowledge toggle — a different chip moves the reading (got `%s`)"
			% controller._selected,
		controller._selected == "herding")
	var blocks := _detail_blocks(panel)
	h._assert_hud("knowledge toggle — …and there is exactly ONE reading in the tree (%d)" % blocks.size(),
		blocks.size() == 1)
	_assert_detail_sits_under_its_own_row("herding")
	# Left as the frame found it, so nothing after this block inherits a moved selection.
	await _press_node("herding")

## **THE READING SITS UNDER ITS OWN DOMAIN'S ROW, not merely somewhere in the tree.** Asserted as an
## INDEX inside `_rows`: exactly one past the row carrying the selected node. "It is in the panel"
## would pass with the block appended at the bottom of a twelve-row list, which is the arrangement
## this layout exists to replace.
func _assert_detail_sits_under_its_own_row(key: String) -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	var panel: KnowledgePanel = controller.panel()
	if panel == null:
		h._assert_hud("knowledge placement — the panel is open", false)
		return
	var node := _node_in(controller.nodes(), key)
	var domain := String(node.get(HudKnowledgeVocab.NODE_DOMAIN, "")) if not node.is_empty() else ""
	var rows: VBoxContainer = panel._rows
	var row_index := -1
	var detail_index := -1
	for i in rows.get_child_count():
		var child := rows.get_child(i)
		if not (child is Control):
			continue
		if (child as Control).get_meta(HudKnowledgeVocab.DOMAIN_META, "") == domain:
			row_index = i
		if (child as Control).has_meta(HudKnowledgeVocab.DETAIL_META):
			detail_index = i
	h._assert_hud("knowledge placement — `%s`'s domain row `%s` is in the list (index %d)"
			% [key, domain, row_index],
		row_index >= 0)
	h._assert_hud("knowledge placement — …and its reading is the very next child (row %d, reading %d)"
			% [row_index, detail_index],
		row_index >= 0 and detail_index == row_index + 1)

## Every detail block mounted in the panel. A LIST rather than a first hit, because the claim that
## matters is *exactly one*.
func _detail_blocks(root: Node) -> Array[Control]:
	var found: Array[Control] = []
	if root is Control and (root as Control).has_meta(HudKnowledgeVocab.DETAIL_META):
		found.append(root as Control)
	for child in root.get_children():
		found.append_array(_detail_blocks(child))
	return found

## **ESCAPE CLOSES THE READING AND NOTHING ELSE** (§4). The plan asks ESC to close the reading and
## asks nothing about closing the screen, so with nothing selected the claim test must answer `false`
## and let the key fall through to the pause menu exactly as it did before this arc.
##
## Driven through the two methods `Main._unhandled_input` reaches BY NAME — a `has_method` probe that
## fails SILENTLY, so a rename here is a key that quietly stops working — and the ORDER is asked of
## `Main.escape_claimant` itself, with the real HUD's own readers rather than literals.
func _assert_escape_closes_only_the_reading() -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	await _press_node("cultivation")
	h._assert_hud("knowledge esc — precondition: a reading is open (`%s`)" % controller._selected,
		h._hud.is_knowledge_detail_open())
	h._assert_hud("knowledge esc — ESC claims the reading ahead of the pause menu",
		h.MAIN_SCRIPT.escape_claimant(false, h._hud.is_compose_sheet_open(),
			h._hud.is_targeting_active(), h._hud.is_work_inspector_open(),
			h._hud.is_knowledge_detail_open()) == h.MAIN_SCRIPT.ESC_KNOWLEDGE_DETAIL)
	# …and yields to the WORK INSPECTOR, which is a dialog rather than a paragraph inside a card.
	h._assert_hud("knowledge esc — …and yields to the work inspector",
		h.MAIN_SCRIPT.escape_claimant(false, false, false, true, true)
			== h.MAIN_SCRIPT.ESC_WORK_INSPECTOR)
	h._hud.close_knowledge_detail()
	await h._settle()
	h._assert_hud("knowledge esc — the closer takes the reading down (`%s`)" % controller._selected,
		not h._hud.is_knowledge_detail_open())
	# **THE SCREEN IS STILL OPEN** — this is the half that says ESC closed the reading rather than the
	# card, and it is the one a "close the panel" implementation would fail.
	h._assert_hud("knowledge esc — …and the SCREEN is still open", controller.is_open())
	# …and with nothing selected the key falls through, exactly as it did before this arc.
	h._assert_hud("knowledge esc — with no reading open ESC falls through to the pause menu",
		h.MAIN_SCRIPT.escape_claimant(false, h._hud.is_compose_sheet_open(),
			h._hud.is_targeting_active(), h._hud.is_work_inspector_open(),
			h._hud.is_knowledge_detail_open()) == h.MAIN_SCRIPT.ESC_PAUSE)

## **THE CARD SURVIVES THE DOMAIN COUNT THIS WHOLE ARC IS ABOUT** — §1's claim, asserted rather than
## argued. The column layout's content minimum was `230 × domains + 336`, so the ten to twelve
## branches `docs/plan_civilization_steps.md` commits wanted 2,636–3,096px on a 1,920 viewport. In
## rows, width is a function of ladder DEPTH and not of the branch count at all.
##
## **TWENTY-FOUR, which is the plan's own stress figure** and twice the planned shape. Built here out
## of `KnowledgeFx.ladder_roster()`'s own ROW SHAPE rather than out of a config or a sim: the panel
## builds itself from the roster, so a synthetic roster is exactly what tests that.
##
## **THE MINIMUM IS ASSERTED BESIDE THE WIDTH**, because a card can be SET to 820 while demanding
## more — Godot renders a Control at its `get_combined_minimum_size()` whatever the fit asked for,
## which is precisely how the clamp was overruled before (see `KnowledgePanel`'s docstring).
func _assert_the_card_survives_a_planned_domain_count() -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	h._hud.update_ladder_knowledge(_stress_roster())
	h._hud.update_crafting_catalogues([], [], _recipes(), _craft_knowledge_mixed())
	h._hud.update_intensification([_wire_tracks(_tracks_mixed())])
	controller.open()
	await h._settle()
	var panel: KnowledgePanel = controller.panel()
	if panel == null:
		h._assert_hud("knowledge stress — the panel is open", false)
		return
	# NON-VACUOUS FIRST: a roster that failed to reach the panel would keep the card at 820 for a
	# reason that has nothing to do with the layout.
	var drawn := controller.domains().size()
	h._assert_hud("knowledge stress — the panel really is drawing %d domains (got %d)"
			% [STRESS_DOMAIN_COUNT + 1, drawn],
		drawn == STRESS_DOMAIN_COUNT + 1)
	h._assert_hud("knowledge stress — the card is still `PANEL_WIDTH` at %d domains (%.0f, want %.0f)"
			% [drawn, panel.size.x, HudKnowledgeVocab.PANEL_WIDTH],
		is_equal_approx(panel.size.x, HudKnowledgeVocab.PANEL_WIDTH))
	var minimum := panel.card().get_combined_minimum_size().x
	h._assert_hud("knowledge stress — …and it is not being rendered above a minimum it cannot hold (%.0f <= %.0f)"
			% [minimum, HudKnowledgeVocab.PANEL_WIDTH],
		minimum <= HudKnowledgeVocab.PANEL_WIDTH)
	# ⛔ **AND THE OTHER AXIS — the one the growth MOVED ONTO, which the two claims above cannot
	# see.** §1's promise is that a branch costs one row of HEIGHT "on an axis that already scrolls",
	# and at 24 domains that containment rests entirely on `AutoSizingPanel.fit_to_content` flipping
	# `_scroll.vertical_scroll_mode` from `DISABLED` (its value at `_ready`) to `AUTO`. With that flip
	# regressed the card simply renders past the room's bottom edge — every width claim above stays
	# green, and the frame beside them looks like a long list.
	var room := panel.available_room(HudKnowledgeVocab.VIEWPORT_MARGIN)
	h._assert_hud("knowledge stress — …and the card is still INSIDE the room's height at %d domains (%.0f <= %.0f)"
			% [drawn, panel.size.y, room.size.y],
		panel.size.y <= room.size.y + LAYOUT_EPSILON)
	var scroll := _scroll_of(panel)
	h._assert_hud("knowledge stress — …because the LIST scrolls at %d domains rather than the card growing (vertical mode %d)"
			% [drawn, -1 if scroll == null else scroll.vertical_scroll_mode],
		scroll != null and scroll.vertical_scroll_mode == ScrollContainer.SCROLL_MODE_AUTO)
	await h._save("knowledge_panel_stress")
	controller.close()
	await h._settle()
	# **PUT THE SHIPPED ROSTER BACK.** This HUD is long-lived and the chapters after this one — and
	# the blocks after this in THIS chapter — are written against the real ladder.
	h._hud.update_ladder_knowledge(KnowledgeFx.ladder_roster())
	await h._settle()

## The stress roster's branch count. `docs/plan_knowledge_rows.md` §1's own figure, and twice the
## ten-to-twelve the civilization-steps plan commits.
const STRESS_DOMAIN_COUNT := 24
## How deep each synthetic branch runs. The design caps a ladder at about four rungs and forbids it
## growing, so this is the shape the card's width is actually a function of.
const STRESS_RUNGS_PER_DOMAIN := 3
const STRESS_BRANCH_FORMAT := "branch_%02d"
const STRESS_KNOWLEDGE_FORMAT := "branch_%02d_step_%d"
const STRESS_DISPLAY_FORMAT := "Branch %02d Step %d"

## `STRESS_DOMAIN_COUNT` ladder branches in the wire's own roster shape — built from
## `KnowledgeFx.ladder_roster()`'s row shape, which is the thing the panel is made of. No config and
## no sim: this is a claim about the CLIENT rendering whatever roster arrives.
func _stress_roster() -> Array:
	var roster: Array = []
	for branch in STRESS_DOMAIN_COUNT:
		for step in STRESS_RUNGS_PER_DOMAIN:
			roster.append({
				KnowledgeFx.KEY_ID: STRESS_KNOWLEDGE_FORMAT % [branch, step + 1],
				KnowledgeFx.KEY_DISPLAY: STRESS_DISPLAY_FORMAT % [branch, step + 1],
				KnowledgeFx.KEY_BRANCH: STRESS_BRANCH_FORMAT % branch,
				KnowledgeFx.KEY_ORDER: step + 1,
				KnowledgeFx.KEY_IS_STEP: true,
			})
	return roster

# ---- the gutter is a cap, and the narrow room is reachable ------------------

## Float slack for a comparison between two FITTED pixel rects. Both sides are rounded layout
## numbers, so this is about the last bit rather than about tolerance.
const LAYOUT_EPSILON := 0.5

## A branch token no label table has ever heard of, chosen so `HudKnowledgeVocab.domain_label`'s
## fallback (`String(branch).capitalize()`) produces a name far wider than `ROW_NAME_WIDTH` — which is
## the SHIPPABLE way to reach this state, a new branch drawing with no client edit being the point of
## that fallback. `STRESS_BRANCH_FORMAT` cannot reach it: `BRANCH 07` is short.
const LONG_DOMAIN_BRANCH := "water_management_and_irrigation"
## Two ordinary branches beside it, so "every row's first chip is on one vertical" is a claim about
## more than one row.
const PLAIN_DOMAIN_BRANCHES := ["kilnwork", "netting"]
const GUTTER_RUNGS_PER_DOMAIN := 2

## **THE DOMAIN-NAME GUTTER IS A CAP AS WELL AS A FLOOR.** `custom_minimum_size` is only the floor,
## and a `Label` that neither clips nor trims reports its whole text as its minimum width — so a
## domain label wider than `ROW_NAME_WIDTH` widened THAT ROW'S gutter alone, breaking the one vertical
## the chips start on and desynchronising `DETAIL_INDENT` from it, so a reading no longer lined up
## under the chip it belonged to.
##
## **ASSERTED AS THE VERTICAL, not as the label's width** — that is the promise the constant's own
## comment makes, and it survives a different way of capping the gutter. The label's width is asserted
## beside it as the MECHANISM, and the tooltip as the rule that truncating is only allowed while the
## whole name stays reachable.
func _assert_a_long_domain_name_cannot_move_the_chips() -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	h._hud.update_ladder_knowledge(_gutter_roster())
	h._hud.update_crafting_catalogues([], [], _recipes(), _craft_knowledge_mixed())
	h._hud.update_intensification([_wire_tracks(_tracks_mixed())])
	controller.open()
	await h._settle()
	var panel: KnowledgePanel = controller.panel()
	if panel == null:
		h._assert_hud("knowledge gutter — the panel is open", false)
		return

	# NON-VACUOUS FIRST: the fixture has to have produced a name that really does overflow the gutter,
	# or every claim below passes on three short labels.
	var long_label := HudKnowledgeVocab.domain_label(StringName(LONG_DOMAIN_BRANCH)).to_upper()
	var long_row := _domain_node(panel, StringName(LONG_DOMAIN_BRANCH))
	var long_name := _domain_name_label(long_row)
	var natural := 0.0 if long_name == null else EventDockPanel.natural_label_width(long_name)
	h._assert_hud("knowledge gutter — the fixture's `%s` really is wider than the gutter (%.0f > %.0f)"
			% [long_label, natural, HudKnowledgeVocab.ROW_NAME_WIDTH],
		natural > HudKnowledgeVocab.ROW_NAME_WIDTH)

	# THE CLAIM: every row's first chip starts on ONE vertical, the over-long name included.
	var branches: Array[StringName] = [StringName(LONG_DOMAIN_BRANCH)]
	for plain in PLAIN_DOMAIN_BRANCHES:
		branches.append(StringName(plain))
	var lefts: Array[float] = []
	for branch in branches:
		var row := _domain_node(panel, branch)
		var chip := _first_chip_in(row)
		if chip == null:
			h._assert_hud("knowledge gutter — `%s`'s row has a first chip" % branch, false)
			return
		lefts.append(chip.global_position.x)
	var spread := 0.0
	for left in lefts:
		spread = maxf(spread, absf(left - lefts[0]))
	h._assert_hud("knowledge gutter — every row's first chip starts on ONE vertical, the over-long name included (%s)"
			% str(lefts),
		spread <= LAYOUT_EPSILON)

	# …and the mechanism, so a failure above says WHICH half moved.
	h._assert_hud("knowledge gutter — …because the name label is held to the gutter (%.0f, want %.0f)"
			% [0.0 if long_name == null else long_name.size.x, HudKnowledgeVocab.ROW_NAME_WIDTH],
		long_name != null and is_equal_approx(long_name.size.x, HudKnowledgeVocab.ROW_NAME_WIDTH))
	# **TRUNCATING IS ONLY ALLOWED WHILE THE WHOLE NAME STAYS REACHABLE.** Through
	# `HudWidgets.set_label_tooltip`, so the hover is actually reachable — a `Label` defaults to
	# `MOUSE_FILTER_IGNORE`, where a bare `tooltip_text` is a silent no-op.
	h._assert_hud("knowledge gutter — …and the trimmed name keeps the WHOLE label on its hover (`%s`)"
			% ("" if long_name == null else long_name.tooltip_text),
		long_name != null and long_name.tooltip_text == long_label)
	h._assert_hud("knowledge gutter — …and it can receive that hover at all (a Label ignores the mouse by default)",
		long_name != null and long_name.mouse_filter != Control.MOUSE_FILTER_IGNORE)
	# A name that FITS gets NO tooltip — a hover repeating what is on screen is noise, and the claim
	# above would pass on a panel that tooltipped every row.
	var plain_name := _domain_name_label(_domain_node(panel, StringName(PLAIN_DOMAIN_BRANCHES[0])))
	h._assert_hud("knowledge gutter — …while a name that FITS carries none (`%s`)"
			% ("" if plain_name == null else plain_name.tooltip_text),
		plain_name != null and plain_name.tooltip_text == "")

	controller.close()
	await h._settle()

## `LONG_DOMAIN_BRANCH` plus the two plain ones, in the roster's own row shape.
func _gutter_roster() -> Array:
	var roster: Array = []
	var branches: Array[String] = [LONG_DOMAIN_BRANCH]
	for plain in PLAIN_DOMAIN_BRANCHES:
		branches.append(String(plain))
	for branch in branches:
		for step in GUTTER_RUNGS_PER_DOMAIN:
			roster.append({
				KnowledgeFx.KEY_ID: "%s_step_%d" % [branch, step + 1],
				KnowledgeFx.KEY_DISPLAY: "%s Step %d" % [branch.capitalize(), step + 1],
				KnowledgeFx.KEY_BRANCH: branch,
				KnowledgeFx.KEY_ORDER: step + 1,
				KnowledgeFx.KEY_IS_STEP: true,
			})
	return roster

## The room the reachability claim is staged in: `PANEL_MIN_WIDTH` plus the margin the card insets
## itself by on each side, so `refit` clamps the card to exactly `PANEL_MIN_WIDTH` and the claim is
## made at the narrowest the card is allowed to be.
const NARROW_ROOM_WIDTH := HudKnowledgeVocab.PANEL_MIN_WIDTH \
	+ 2.0 * HudKnowledgeVocab.VIEWPORT_MARGIN
## A SHORT roster for it, because the state under test is the one where the room's HEIGHT does not
## bind: a body taller than the room turns the vertical scroll on for a reason that has nothing to do
## with the horizontal bar, and the claim would pass with the bar's reserve deleted.
const NARROW_ROOM_DOMAINS := 2
const NARROW_ROOM_RUNGS := 2
## How much shorter than the card the squeezed room is made — inside one scrollbar's height (8px in
## this theme) and not zero, so the room genuinely binds and does so by less than the bar.
const NARROW_ROOM_SQUEEZE := 4.0
const NARROW_ROOM_BRANCH_FORMAT := "narrow_%02d"
const NARROW_ROOM_KNOWLEDGE_FORMAT := "narrow_%02d_step_%d"
## **WIDE ON PURPOSE, and it is the SIM's own field** (`display_name`): one chip wearing a name this
## long is wider than the whole interior of a `PANEL_MIN_WIDTH` card, so the horizontal bar is up for
## a reason that survives the reading's columns being re-derived at the narrow width. A name of
## ordinary length leaves the bar's appearance resting on the detail block alone, which is exactly the
## term the fix in `_detail_section_width` removes.
const NARROW_ROOM_DISPLAY_FORMAT := "Terraced Hillside Irrigation Works %02d-%d"

## ⛔ **A ROOM NARROWER THAN THE NOMINAL CARD MUST NOT PUT THE LAST ROW OUT OF REACH.**
##
## The horizontal axis became `SCROLL_MODE_AUTO` in this arc, so an h-scrollbar is reachable for the
## first time — and `ScrollContainer` takes that bar's height off the CHILD'S VIEWPORT. A height fit
## that did not include it returns `desired == clamped`, `AutoSizingPanel` therefore DISABLES the
## vertical scroll, and `_body` (vertically `SIZE_FILL`, not `SIZE_EXPAND`) lays out at its full
## minimum from `y = 0`: the bottom of the list is clipped with no way to scroll to it.
##
## **THE ROOM IS SWAPPED, WHICH IS THE PANEL'S OWN SEAM FOR IT.** `room_bounds` is how a card is told
## what rectangle it may use (`AutoSizingPanel`), and a probe Control standing in for it drives the
## real `_room()` → `refit` path rather than a hand-set width. The card is then asked the question the
## player would: **is the bottom of the list inside the scroll's own VISIBLE area, or can the card be
## scrolled to it** — with the precondition that the bar is up and that the room's height is NOT what
## put it there, so neither leg can pass for the other's reason.
##
## **THE ROOM CHANGE IS DRIVEN AS A REFIT AND THEN A RE-RENDER**, which is the order the app produces
## one in: a window resize re-fits alone (`KnowledgePanelController.refit_room`) and the next snapshot
## re-renders. The claim is made on the SETTLED state, because the interim is where the body's
## minimum is still a frame behind the width it was measured at — and that lag is not a detail: the
## first form of this block asserted on it and an 8px discrepancy between the two measurements masked
## a deleted reserve exactly, so the sabotage passed.
func _assert_a_narrow_room_keeps_the_last_row_reachable() -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	h._hud.update_ladder_knowledge(_narrow_room_roster())
	h._hud.update_crafting_catalogues([], [], [], [])
	h._hud.update_intensification([_wire_tracks(_tracks_mixed())])
	controller.open()
	await h._settle()
	var panel: KnowledgePanel = controller.panel()
	if panel == null:
		h._assert_hud("knowledge narrow-room — the panel is open", false)
		return

	# **WITH A READING OPEN**, the state that has the most to lose — the reading is what sits at the
	# bottom of the list, so it is what a clipped viewport takes first.
	await _press_node(NARROW_ROOM_KNOWLEDGE_FORMAT % [0, 1])

	var home_bounds: Control = panel.room_bounds
	var full_room := panel.available_room(0.0)
	var probe := Control.new()
	probe.name = "KnowledgeNarrowRoomProbe"
	probe.mouse_filter = Control.MOUSE_FILTER_IGNORE
	h._hud.add_child(probe)
	probe.position = full_room.position
	probe.size = Vector2(NARROW_ROOM_WIDTH, full_room.size.y)
	panel.room_bounds = probe
	# The room change alone, which is all a window resize does
	# (`KnowledgePanelController.refit_room` re-fits rather than re-rendering)…
	panel.refit()
	await h._settle()
	# …and then the RE-RENDER the next snapshot brings, which is the state the claim is made on: it is
	# the settled one, where the body's minimum has stopped moving between the fit and the frame after
	# it. Measured on the interim instead, an 8px lag between the two measurements happened to mask a
	# deleted reserve exactly — the sabotage passed.
	controller.render()
	await h._settle()
	panel.refit()
	await h._settle()

	# NON-VACUOUS, LEG 1: the card really did narrow to the room it was given.
	h._assert_hud("knowledge narrow-room — the card narrows to the room (%.0f, want %.0f)"
			% [panel.size.x, HudKnowledgeVocab.PANEL_MIN_WIDTH],
		is_equal_approx(panel.size.x, HudKnowledgeVocab.PANEL_MIN_WIDTH))
	var scroll := _scroll_of(panel)
	if scroll == null:
		h._assert_hud("knowledge narrow-room — the card has its scroll", false)
		return
	# NON-VACUOUS, LEG 2: the horizontal bar is actually up. Read HERE, a settled frame later — which
	# is exactly why `KnowledgePanel` may not read it: inside the fit it answers for the previous
	# layout.
	var hbar := scroll.get_h_scroll_bar()
	h._assert_hud("knowledge narrow-room — …and the horizontal scrollbar is UP, which is the state under test (%s)"
			% str(hbar.visible),
		hbar.visible)

	# ⛔ **AND NOW THE ROOM IS SQUEEZED INTO THE BAR'S OWN HEIGHT, which is the ONLY band where the
	# reserve decides anything.** Anywhere else the answer is the same either way: a room with room to
	# spare lets the card simply grow, and a room several rows short turns the vertical scroll on
	# whatever the bar costs. Between them lies the case the reserve is for — a room shorter than the
	# card by LESS than one scrollbar — so the probe is resized to exactly that, off the height the
	# card has just told us it wants rather than off a typed number.
	var wanted_height := panel.card().size.y
	probe.size = Vector2(NARROW_ROOM_WIDTH,
		wanted_height - NARROW_ROOM_SQUEEZE + 2.0 * HudKnowledgeVocab.VIEWPORT_MARGIN)
	panel.refit()
	await h._settle()
	var ceiling := panel.available_room(HudKnowledgeVocab.VIEWPORT_MARGIN).size.y
	h._assert_hud("knowledge narrow-room — …and the room is now shorter than the card by less than the bar (%.0f short, bar %.0f)"
			% [wanted_height - ceiling, hbar.size.y],
		wanted_height - ceiling > 0.0 and wanted_height - ceiling <= hbar.size.y)

	# THE CLAIM: the card stays INSIDE that room, and what does not fit is scrollable.
	#
	# ⛔ **ASSERTED ON `card()`, THE DRAWN RECT, NEVER ON `panel.size`** — the panel is a plain Control
	# whose size is arithmetic, while the card is a real Container and Godot will not draw one below
	# its own combined minimum. That minimum is where the bar is actually charged: a `ScrollContainer`
	# with its vertical axis DISABLED reports its child's whole height PLUS the horizontal bar, so a
	# fit that did not budget the bar leaves the card demanding more than the fit gave it, and the card
	# spills out of the bottom of the room with the vertical scroll switched off — measured on a
	# sabotaged build at 455 against a 451 room.
	var drawn_inside := panel.card().size.y <= ceiling + LAYOUT_EPSILON
	var can_scroll := scroll.vertical_scroll_mode == ScrollContainer.SCROLL_MODE_AUTO
	h._assert_hud("knowledge narrow-room — the card stays inside the squeezed room (%.0f <= %.0f) and the rest SCROLLS (%s)"
			% [panel.card().size.y, ceiling, str(can_scroll)],
		drawn_inside and can_scroll)

	# **PUT THE ROOM BACK**, then the shipped roster: this HUD is long-lived and three chapters run
	# after this one. A stranded probe would keep every later card measured against a 384px room.
	panel.room_bounds = home_bounds
	controller.close()
	await h._settle()
	probe.queue_free()
	h._hud.update_ladder_knowledge(KnowledgeFx.ladder_roster())
	h._hud.update_crafting_catalogues([], [], [], [])
	await h._settle()

## Two short synthetic branches — see `NARROW_ROOM_DOMAINS` for why the body has to stay short.
func _narrow_room_roster() -> Array:
	var roster: Array = []
	for branch in NARROW_ROOM_DOMAINS:
		for step in NARROW_ROOM_RUNGS:
			roster.append({
				KnowledgeFx.KEY_ID: NARROW_ROOM_KNOWLEDGE_FORMAT % [branch, step + 1],
				KnowledgeFx.KEY_DISPLAY: NARROW_ROOM_DISPLAY_FORMAT % [branch, step + 1],
				KnowledgeFx.KEY_BRANCH: NARROW_ROOM_BRANCH_FORMAT % branch,
				KnowledgeFx.KEY_ORDER: step + 1,
				KnowledgeFx.KEY_IS_STEP: true,
			})
	return roster

## The panel's own scroll, by the name the panel builds it under — the layout claims are about THAT
## node, and a subtree search for a `ScrollContainer` would find whichever one happened to match.
func _scroll_of(panel: KnowledgePanel) -> ScrollContainer:
	var found := panel.find_child("KnowledgeScroll", true, false)
	return found as ScrollContainer if found is ScrollContainer else null

## A domain row's NAME label — the first Label in the row, which is how `_build_domain_row` mounts it
## (name, then the chips' flow). Found structurally rather than by text, the text being the thing
## under test.
func _domain_name_label(row: Node) -> Label:
	if row == null:
		return null
	if row is Label:
		return row as Label
	for child in row.get_children():
		var found := _domain_name_label(child)
		if found != null:
			return found
	return null

## A domain row's FIRST chip, in child order — the one every row's gutter is supposed to line up.
func _first_chip_in(row: Node) -> Control:
	if row == null:
		return null
	if row is Control and String((row as Control).get_meta(HudKnowledgeVocab.NODE_META, "")) != "":
		return row as Control
	for child in row.get_children():
		var found := _first_chip_in(child)
		if found != null:
			return found
	return null

# ---- the launcher and its pip ----------------------------------------------

## **THE LAUNCHER'S PIP, on a REAL `BandCityPanel`.** A literal would prove nothing: the pip is drawn
## into the button's own rect and the count is retained across a mount rebuild, and both of those are
## the panel's answers.
func _assert_launcher_pip() -> void:
	var panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(panel)
	await h.get_tree().process_frame
	h._hud.set_band_city_panel(panel)
	# A model with exactly ONE unspent discovery, staged through the real ingest so the pip's number is
	# the controller's own answer rather than a figure pushed in beside it.
	#
	# **THE SECTIONS GO IN `Main`'s OWN ORDER — knowledge, catalogues, patches, then populations.** A
	# fixture that pushed them any other way would be staging a snapshot no server sends, and this
	# chapter's first cut did exactly that: `update_band_alerts` first, so the pip was computed against
	# the PREVIOUS block's tracks and read 2 where the controller read 1.
	h._hud.update_intensification([_wire_tracks({"cultivation": PROGRESS_KNOWN})])
	h._hud.update_crafting_catalogues([], [], _recipes(), _craft_knowledge_untouched())
	h._hud.update_forage_patches([])
	h._set_world_herds([])
	h._hud.update_band_alerts([_band([])])
	await h._settle()
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	h._assert_hud("knowledge pip — the controller counts 1 unspent discovery (got %d: %s)"
			% [controller.unspent_count(), str(_unspent_keys(controller))],
		controller.unspent_count() == 1)
	h._assert_hud("knowledge pip — …and the launcher wears that number (got %d)"
			% panel.action_pip(BandCityPanel.ACTION_KNOWLEDGE),
		panel.action_pip(BandCityPanel.ACTION_KNOWLEDGE) == 1)
	# **THE PIP SURVIVES A DOCK CHANGE**, which rebuilds the action mount wholesale and throws every
	# button away. A count that lived only on the node would vanish on a dock flip and come back on
	# the next turn tick — invisible in any frame.
	panel.set_dock(SIDE_BOTTOM)
	await h._settle()
	h._assert_hud("knowledge pip — it survives the mount rebuild a dock change causes (got %d)"
			% panel.action_pip(BandCityPanel.ACTION_KNOWLEDGE),
		panel.action_pip(BandCityPanel.ACTION_KNOWLEDGE) == 1)
	h._assert_hud("knowledge pip — …and the pill is drawn INSIDE the button, so it cannot widen the bar",
		_pip_is_inside_its_button(panel))
	# **THE LAUNCHER'S FACE, IN A FRAME** (issue #581): the cairn is bundled ART on the `Button.icon`
	# seam, so a picture is the only witness that it is centred in the 24x24 box, still reads as a
	# stacked tapered tower at that size, and still has the pip sitting over it. Captured on the
	# SUBJECT-ROW mount a bottom dock takes, then on the collapsed RAIL — the smallest surface the art
	# has to survive, and the one the other two cannot stand in for.
	await h._save("knowledge_launcher_mark")
	panel.set_collapsed(true)
	await h._settle()
	await h._save("knowledge_launcher_mark_rail")
	panel.set_collapsed(false)
	await h._settle()
	# **A DELTA THAT CARRIES KNOWLEDGE AND NO POPULATIONS STILL MOVES THE PIP.** `Main` dispatches each
	# section independently and only when it CHANGED, so a turn that finishes a track and moves nobody
	# skips `update_band_alerts` entirely — and that was the one seam the pip used to be pushed from.
	# Asserted with NOTHING else pushed, which is what makes it a claim about the section rather than
	# about the frame.
	h._hud.update_intensification([_wire_tracks({
		"cultivation": PROGRESS_KNOWN, "herding": PROGRESS_KNOWN})])
	await h._settle()
	h._assert_hud("knowledge pip — a knowledge-only delta moves it with no populations section (got %d)"
			% panel.action_pip(BandCityPanel.ACTION_KNOWLEDGE),
		panel.action_pip(BandCityPanel.ACTION_KNOWLEDGE) == 2)

	# **AND IT CLEARS WHEN A SOURCE STARTS USING THE KNOWLEDGE, never when the screen is looked at.**
	# That is the honest trigger, and it is the one the state's own definition gives. The patch and the
	# populations are both pushed here, i.e. the ordinary turn — so this is the `update_band_alerts`
	# path beside the section-only one above.
	h._hud.update_intensification([_wire_tracks({"cultivation": PROGRESS_KNOWN})])
	h._hud.update_forage_patches([_patch(6, 6, true, false)])
	h._hud.update_band_alerts([_band([])])
	await h._settle()
	h._assert_hud("knowledge pip — a tended patch clears it (got %d)"
			% panel.action_pip(BandCityPanel.ACTION_KNOWLEDGE),
		panel.action_pip(BandCityPanel.ACTION_KNOWLEDGE) == 0)

	# **THE LAUNCH EDGE, driven through the real registry.** The press comes back as
	# `action_invoked(ACTION_KNOWLEDGE)`, is relayed as `knowledge_requested`, and opens the screen.
	panel.set_dock(SIDE_LEFT)
	await h._settle()
	# …and the third mount: a vertical dock hangs the actions on their own BAR under the subject block.
	await h._save("knowledge_launcher_mark_bar")
	panel.action_invoked.emit(BandCityPanel.ACTION_KNOWLEDGE)
	await h._settle()
	h._assert_hud("knowledge launcher — the registry's press OPENS the screen",
		h._hud.knowledge_panel().is_open())
	# …and it is a TOGGLE, like every other panel this HUD hangs off a header glyph.
	panel.action_invoked.emit(BandCityPanel.ACTION_KNOWLEDGE)
	await h._settle()
	h._assert_hud("knowledge launcher — …and pressing it again closes it",
		not h._hud.knowledge_panel().is_open())

	h._hud.set_band_city_panel(null)
	panel.queue_free()
	await h.get_tree().process_frame
	await h._settle()

# ---- the orb's rows open this screen ----------------------------------------

## The two turns the routing block walks, and **it is TWO because a diff needs a baseline it set
## itself.** The blocks between here and `_assert_new_this_turn` push tracks without advancing the
## turn, so the screen's `_known_at_turn_start` is still whatever that block left — measured: it
## already held `herding`, so teaching `herding` on one turn produced NOTHING new and the `new`
## filter counted zero, which would have made a landing on it unfalsifiable. The first turn seeds a
## baseline of one track; the second adds the second, which is then genuinely this turn's.
const TURN_FILTER_ROUTE_SEED := 43

const TURN_FILTER_ROUTE := 44

## **THE ORB'S KNOWLEDGE ROW OPENS THIS SCREEN ON THE FILTER IT ASKED ABOUT**
## (`docs/plan_knowledge_screen.md` §5, slice C). Driven through `TurnOrb.panel_requested`, which is
## the signal a non-locating row really emits when pressed, so the branch under test is
## `TurnOrbController._on_turn_orb_panel_requested` and not a method this chapter called directly.
##
## **THE FILTER IS ASSERTED, NEVER LOOKED AT.** A screen opened on the wrong filter renders a perfectly
## ordinary card — the rows are the same, the pills are the same, and only which pill is lit says
## anything — so the claim is read off the drawn chrome (`_live_filter`) rather than saved as a frame.
##
## **THE SCREEN IS DELIBERATELY LEFT ON A DIFFERENT FILTER FIRST, and that is what makes the landing
## falsifiable.** `open_on_filter` exists precisely to OVERRIDE the retained view state, so a fixture
## that opened a panel already sitting on `new` would pass with the branch deleted. The screen is
## parked on `unused` — through the real pill, with real pointer input — before the row is pressed,
## and the fixture keeps BOTH filters non-empty so neither is an empty-handed card.
func _assert_opens_on_filter() -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	controller.close()
	# Two discoveries, neither in use (no patches, no herds), and `herding` finishing on the SECOND of
	# the two turns — so the `unused` filter counts two and the `new` filter counts one.
	h._hud.update_forage_patches([])
	h._set_world_herds([])
	h._hud.update_overlay(TURN_FILTER_ROUTE_SEED, {})
	h._hud.update_intensification([_wire_tracks({"cultivation": PROGRESS_KNOWN})])
	h._hud.update_band_alerts([_band([])])
	await h._settle()
	h._hud.update_overlay(TURN_FILTER_ROUTE, {})
	h._hud.update_intensification([_wire_tracks({
		"cultivation": PROGRESS_KNOWN, "herding": PROGRESS_KNOWN})])
	h._hud.update_band_alerts([_band([])])
	await h._settle()
	var nodes := controller.nodes()
	var unused_count := KnowledgeRoster.count_matching(nodes, HudKnowledgeVocab.FILTER_UNUSED)
	var new_count := KnowledgeRoster.count_matching(nodes, HudKnowledgeVocab.FILTER_NEW)
	h._assert_hud("knowledge route — the fixture has something under BOTH filters (%d unused, %d new)"
			% [unused_count, new_count],
		unused_count > 0 and new_count > 0)
	# **PARK THE SCREEN ON A DIFFERENT FILTER FIRST** — through the pill itself, so what the row has to
	# override is the state a real player would have left behind.
	controller.toggle()
	await h._settle()
	await _press_filter(HudKnowledgeVocab.FILTER_UNUSED)
	controller.close()
	await h._settle()
	# **THE FRESHLY-LEARNED ROW.** It named a discovery; it has to land the player on the list holding
	# it, not on the `unused` the screen was last left on.
	h._hud.turn_orb.panel_requested.emit(HudAttentionVocab.ATTENTION_KIND_KNOWLEDGE_LEARNED,
		TurnOrb.PANEL_SUBJECT_NONE)
	await h._settle()
	h._assert_hud("knowledge route — the orb's knowledge row OPENS the screen", controller.is_open())
	h._assert_hud("knowledge route — …on `%s`, overriding the `%s` it was left on (lit pill: `%s`)"
			% [HudKnowledgeVocab.FILTER_NEW, HudKnowledgeVocab.FILTER_UNUSED, _live_filter()],
		_live_filter() == HudKnowledgeVocab.FILTER_NEW)
	# **PRESSED AGAIN WHILE THE SCREEN IS ALREADY OPEN, it must not toggle shut.** The launcher glyph is
	# a toggle because pressing it means *show me / hide it*; an attention row means *take me to this*,
	# and a press that closed the thing it points at would answer a question nobody asked.
	h._hud.turn_orb.panel_requested.emit(HudAttentionVocab.ATTENTION_KIND_KNOWLEDGE_LEARNED,
		TurnOrb.PANEL_SUBJECT_NONE)
	await h._settle()
	h._assert_hud("knowledge route — a row pressed while the screen is open does NOT toggle it shut",
		controller.is_open())
	h._assert_hud("knowledge route — …and it is still on `%s` (lit pill: `%s`)"
			% [HudKnowledgeVocab.FILTER_NEW, _live_filter()],
		_live_filter() == HudKnowledgeVocab.FILTER_NEW)
	# ⛔ **AND WITH A READING OPEN IT MUST NOT TOGGLE THAT EITHER** (`docs/plan_knowledge_rows.md` §4).
	# Selection is a toggle now, so an external open routed through `_on_node_selected` would CLOSE the
	# row in the one case where the player already had that exact knowledge open — the one case where
	# the orb's row appears to do nothing. Staged with a reading genuinely open, which is what makes
	# the claim falsifiable: with nothing selected, "the selection is untouched" is vacuous.
	await _press_node("cultivation")
	h._assert_hud("knowledge route — precondition: a reading is open before the hand-over (`%s`)"
			% controller._selected,
		controller._selected == "cultivation")
	h._hud.turn_orb.panel_requested.emit(HudAttentionVocab.ATTENTION_KIND_KNOWLEDGE_LEARNED,
		TurnOrb.PANEL_SUBJECT_NONE)
	await h._settle()
	h._assert_hud("knowledge route — the hand-over leaves the screen OPEN", controller.is_open())
	h._assert_hud("knowledge route — …and leaves the reading UNTOUCHED (`%s`)" % controller._selected,
		controller._selected == "cultivation")
	await _press_node("cultivation")
	# **WHY THE ENTRY POINT HAS TO EXIST AT ALL.** The live filter is CONTROLLER state that survives a
	# close, so a plain launcher open reopens on whatever was last set. Parked on `unused` again and
	# reopened through `toggle`, the screen comes back on `unused` — which is what the row would have
	# got had it called `open()`, leaving the player hunting for the discovery it had just named.
	await _press_filter(HudKnowledgeVocab.FILTER_UNUSED)
	controller.close()
	await h._settle()
	controller.toggle()
	await h._settle()
	h._assert_hud("knowledge route — a plain launcher open keeps the LAST filter (`%s`), which is why the row needs its own entry point"
			% _live_filter(),
		_live_filter() == HudKnowledgeVocab.FILTER_UNUSED)
	controller.close()
	await h._settle()

## **THE FILTER THE PANEL IS ACTUALLY RENDERING ON, read off the drawn pills.** The live pill is styled
## `HudStyle.apply_pill_toggle(_, true)` and so carries an OPAQUE `normal` fill, while every quiet pill
## sits at `HudStyle.PILL_QUIET_ALPHA` — zero. Taken from the chrome rather than from the controller's
## own `_filter`, so a panel handed a filter and rendering the previous one fails here.
##
## Answers `&""` unless EXACTLY ONE pill is lit: two lit pills is a rendering fault of its own, and a
## helper that returned the first of them would report a correct-looking filter over a broken row.
func _live_filter() -> StringName:
	var panel: KnowledgePanel = h._hud.knowledge_panel().panel()
	if panel == null:
		return &""
	var lit: Array[StringName] = []
	for spec in HudKnowledgeVocab.FILTERS:
		var key := String(spec[HudKnowledgeVocab.FILTER_SPEC_KEY])
		var pill := _filter_pill(panel, key)
		if not (pill is Button):
			continue
		var box := (pill as Button).get_theme_stylebox("normal")
		if box is StyleBoxFlat and (box as StyleBoxFlat).bg_color.a > HudStyle.PILL_QUIET_ALPHA:
			lit.append(StringName(key))
	return lit[0] if lit.size() == 1 else &""

# ---- a SECTION that arrives after the baseline was seeded -------------------

## The two turns the late-catalogue block walks. **Its own pair**, for the reason the routing block
## has one: the claim is about a baseline this block seeds ITSELF, and a turn the diff has already
## been rolled on would seed it from whatever the previous block left behind.
const TURN_LATE_CATALOGUE_SEED := 45
const TURN_LATE_CATALOGUE := 46

## The loaded-world block's own turn pair, for the same reason the block above has one: it seeds a
## baseline of its OWN, and a turn the diff has already rolled on would seed it from what the
## previous block left. Deliberately far from every other turn in this file, and high — a load
## restores a world mid-campaign, which is the whole premise.
const TURN_LOADED_WORLD_SEED := 71
const TURN_LOADED_WORLD := 72

## The three tracks the reported save (`thirdsave.shdw`, turn 71) carried as long-finished.
const LOADED_WORLD_KNOWN_TRACKS := ["cultivation", "seed_selection", "herding"]

## …and one the save did NOT carry, so the block can prove the diff still WORKS after a load rather
## than merely that it has gone quiet.
const LOADED_WORLD_LEARNED_TRACK := "penning"

## **A CRAFT THE FACTION ALREADY KNEW MUST NOT REPORT AS LEARNED — and the real dispatch ORDER is what
## makes that hard.** `Main._apply_snapshot` sends `update_intensification` BEFORE
## `update_crafting_catalogues`, and both reach the diff. So the pass that seeds the baseline sees the
## ladder tracks beside an EMPTY craft vector, and under a "the first PASS learns nothing" rule the
## baseline is sealed with no crafts in it at all. On the next tick every long-known craft is then in
## `known_now` and absent from the baseline: `learned_this_turn` names them, `KnowledgeRoster._is_new`
## marks each node NEW, and the turn orb grows one *"<Craft> learned"* row per craft the faction has
## held for a hundred turns. The rule that actually holds is per KEY — a key the roster has never
## carried before is a section that has only just arrived, whatever pass it arrives on.
##
## **THE SECTIONS GO IN `Main`'s OWN ORDER, and that IS the claim.** Pushing the catalogues first
## would stage a snapshot no server sends, and the block would pass with the guard deleted.
func _assert_late_catalogue_is_not_learned() -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	# **THE CRAFT VECTOR IS CLEARED FIRST, AND IT HAS TO BE CLEARED THROUGH THE WIRE.** A catalogue is
	# the last value pushed and outlives `reset_world_state`, which drops the DIFF and not the wire's
	# last section — so this long-lived HUD arrives here still holding the previous block's craft rows,
	# and a seeding pass that can already see the three craft KEYS is not the shape a fresh connect
	# has. Measured: without this the block failed for that reason alone, the fold under test never
	# being reached.
	h._hud.update_crafting_catalogues([], [], [], [])
	controller.reset_world_state()
	# THE SEEDING PASS — the turn, then the ladder tracks, and NO catalogues: exactly what the craft
	# row looks like on the frame the baseline is seeded from.
	h._hud.update_overlay(TURN_LATE_CATALOGUE_SEED, {})
	h._hud.update_intensification([_wire_tracks({"cultivation": PROGRESS_KNOWN})])
	await h._settle()
	# …and the catalogues arriving AFTER it, on the SAME turn, carrying crafts the faction already
	# knows — which is the ordinary case rather than an edge one: a faction that has ever crafted
	# anything reconnects into exactly this.
	h._hud.update_crafting_catalogues([], [], _recipes(), _craft_knowledge_all_known())
	await h._settle()
	# The next TURN, with nothing whatever changed: same tracks, same catalogues, one tick later.
	h._hud.update_overlay(TURN_LATE_CATALOGUE, {})
	h._hud.update_intensification([_wire_tracks({"cultivation": PROGRESS_KNOWN})])
	h._hud.update_crafting_catalogues([], [], _recipes(), _craft_knowledge_all_known())
	await h._settle()
	# **NON-VACUOUS FIRST.** A fixture that staged no known craft at all satisfies both claims below
	# for a reason that has nothing to do with the diff, so the roster is asked whether it is really
	# carrying this craft as KNOWN before it is asked whether the craft is new.
	var nodes := controller.nodes()
	var tanning := _node_in(nodes, CRAFT_TANNING)
	var tanning_state := String(tanning.get(HudKnowledgeVocab.NODE_STATE, "")) \
		if not tanning.is_empty() else "absent from the roster"
	h._assert_hud("knowledge late-catalogue — the fixture really does stage `%s` as KNOWN (%s)"
			% [CRAFT_TANNING, tanning_state],
		tanning_state == HudKnowledgeVocab.NODE_STATE_KNOWN)
	h._assert_hud("knowledge late-catalogue — a craft known before the catalogues ARRIVED is not learned this turn (%s)"
			% str(controller.learned_this_turn().keys()),
		not controller.learned_this_turn().has(CRAFT_TANNING))
	# …and the ROSTER agrees, which is the half the turn orb's row and the `new` pill actually read.
	h._assert_hud("knowledge late-catalogue — …and its roster node does not carry `%s` either"
			% HudKnowledgeVocab.NODE_NEW,
		not bool(tanning.get(HudKnowledgeVocab.NODE_NEW, false)))

## **A WORLD THAT ARRIVES ALREADY KNOWING THINGS MUST NOT ANNOUNCE THEM** — the ladder twin of the
## craft block above, and the one that shipped.
##
## Loading a save reloads `Main.tscn`, so every controller here is a NEW object with `_diff_turn` at
## `UNSEEN_TURN`: the first frame of the loaded world is what seeds the baseline, and that frame
## carries tracks the faction finished forty turns ago. `Main._apply_snapshot` sends the ladder's
## ROSTER (a per-world constant, so a load is exactly when it moves) BEFORE the faction's PROGRESS
## row, and BOTH reach the diff — so the baseline was seeded from a model that declared all seven
## knowledges and carried progress for none. `_seen_keys` took the roster, `_known_at_turn_start`
## took nothing, and the progress row landing a moment later on the same turn could not repair it:
## the fold was over keys seen for the FIRST time, and those keys had been spent one refresh earlier.
## The next tick announced *"Cultivation learned"*, *"Seed Selection learned"* and *"Herding
## learned"*. Reported from play.
##
## **THE SECTIONS GO IN `Main`'s OWN ORDER, and that IS the claim** — roster, then progress, both on
## the seeding turn. Pushing them together, or progress first, stages a snapshot no server sends and
## the block passes with the fix reverted.
func _assert_loaded_world_is_not_learned() -> void:
	var controller: KnowledgePanelController = h._hud.knowledge_panel()
	# The craft vector goes out through the wire first, for the reason the block above states: a
	# catalogue outlives `reset_world_state`, and crafts left standing here would be seen by the
	# seeding pass and take no part in the claim.
	h._hud.update_crafting_catalogues([], [], [], [])
	controller.reset_world_state()
	var wire_tracks := {}
	for track in LOADED_WORLD_KNOWN_TRACKS:
		wire_tracks[track] = PROGRESS_KNOWN

	# THE FIRST FRAME OF THE LOADED WORLD, in `Main._apply_snapshot`'s order: the turn, the roster,
	# then the progress row.
	h._hud.update_overlay(TURN_LOADED_WORLD_SEED, {})
	h._hud.update_ladder_knowledge(KnowledgeFx.ladder_roster())
	h._hud.update_intensification([_wire_tracks(wire_tracks)])
	await h._settle()

	# **NON-VACUOUS FIRST, AND THIS IS THE LEG THE WHOLE BLOCK RESTS ON.** A seeding pass that staged
	# no KNOWN track satisfies "nothing was announced" for a reason that has nothing to do with the
	# diff, so the roster is asked whether it is really carrying all three as KNOWN before it is asked
	# whether any of them is new.
	var seeded := controller.nodes()
	for track in LOADED_WORLD_KNOWN_TRACKS:
		var node := _node_in(seeded, track)
		var state := String(node.get(HudKnowledgeVocab.NODE_STATE, "")) \
			if not node.is_empty() else "absent from the roster"
		h._assert_hud("knowledge loaded-world — the loaded frame really does carry `%s` as KNOWN (%s)"
				% [track, state],
			state == HudKnowledgeVocab.NODE_STATE_KNOWN)

	# THE NEXT TURN, with nothing whatever changed — the tick the player presses after loading.
	h._hud.update_overlay(TURN_LOADED_WORLD, {})
	h._hud.update_intensification([_wire_tracks(wire_tracks)])
	await h._settle()

	var learned := controller.learned_this_turn()
	var roster := controller.nodes()
	for track in LOADED_WORLD_KNOWN_TRACKS:
		h._assert_hud("knowledge loaded-world — `%s` was known before the save and is not learned on the tick after it (%s)"
				% [track, str(learned.keys())],
			not learned.has(track))
		# …and the ROSTER agrees, which is the half the turn orb's row actually reads.
		h._assert_hud("knowledge loaded-world — …and `%s`'s roster node does not carry `%s` either"
				% [track, HudKnowledgeVocab.NODE_NEW],
			not bool(_node_in(roster, track).get(HudKnowledgeVocab.NODE_NEW, false)))
	# **THE ORB IS THE SURFACE THAT ACTUALLY SHOUTED**, so it is asked in its own terms rather than
	# inferred from the roster: this is the array `_push_knowledge_attention` hands the orb.
	var announced := AttentionController.knowledge_attention(roster)
	h._assert_hud("knowledge loaded-world — the turn orb raises NO `learned` row after a load (%d rows)"
			% announced.size(),
		announced.is_empty())

	# **AND THE DIFF IS STILL ALIVE** — a fix that simply stopped announcing would pass every claim
	# above. One more turn, with a track that genuinely completes now, must still be announced.
	var with_penning := wire_tracks.duplicate()
	with_penning[LOADED_WORLD_LEARNED_TRACK] = PROGRESS_KNOWN
	h._hud.update_overlay(TURN_LOADED_WORLD + 1, {})
	h._hud.update_intensification([_wire_tracks(with_penning)])
	await h._settle()
	h._assert_hud("knowledge loaded-world — a track that completes AFTER the load is still announced (%s)"
			% str(controller.learned_this_turn().keys()),
		controller.learned_this_turn().has(LOADED_WORLD_LEARNED_TRACK))

## The pip's rect must sit inside its button's. It is an anchored, mouse-transparent child of a
## `Button` — which is not a `Container`, so it contributes nothing to the parent's minimum size — and
## that is exactly the property wanted: a badge that took layout width would make the action bar's
## minimum a function of a snapshot count.
func _pip_is_inside_its_button(panel: BandCityPanel) -> bool:
	var button: Variant = panel._action_buttons.get(BandCityPanel.ACTION_KNOWLEDGE)
	if not (button is Button):
		return false
	var host: Button = button
	var pill := host.get_node_or_null(NodePath(BandCityPanel.ACTION_PIP_NAME))
	if not (pill is Control):
		return false
	return host.get_global_rect().encloses((pill as Control).get_global_rect())

# ---- assertion helpers ------------------------------------------------------

func _assert_unspent(label: String, track: String, model: Dictionary, wanted: bool) -> void:
	_assert_verdict("knowledge unspent", label, _roster_node(model, track), wanted)

func _assert_craft_unspent(label: String, craft: String, model: Dictionary, wanted: bool) -> void:
	_assert_verdict("knowledge craft", label, _roster_node(model, craft), wanted)

## The controller's OWN model, so the two source resolutions under test are the shipped ones.
func _assert_controller_unspent(label: String, track: String,
		controller: KnowledgePanelController, wanted: bool) -> void:
	var node := {}
	for candidate in KnowledgeRoster.flatten(controller.domains()):
		if String(candidate[HudKnowledgeVocab.NODE_KEY]) == track:
			node = candidate
			break
	_assert_verdict("knowledge sources", label, node, wanted)

## **THE MESSAGE NAMES WHAT WAS FOUND, and its first cut named what was WANTED** — it printed
## `not wanted`, which is the expectation restated, so a failure read `in use (got true)` and said
## nothing at all about the verdict. It also reports an ABSENT node distinguishably: a roster that
## dropped a track answers `false` to every question asked of it, and "the node is missing" and "the
## node says in use" are not the same failure.
func _assert_verdict(category: String, label: String, node: Dictionary, wanted: bool) -> void:
	var found := "no such node" if node.is_empty() \
		else (HudKnowledgeVocab.UNSPENT_CLAUSE if bool(node[HudKnowledgeVocab.NODE_UNSPENT]) else "in use")
	h._assert_hud("%s — %s: want `%s`, got `%s`" % [category, label,
			HudKnowledgeVocab.UNSPENT_CLAUSE if wanted else "in use", found],
		not node.is_empty() and bool(node[HudKnowledgeVocab.NODE_UNSPENT]) == wanted)

## Which nodes are unspent, by key — so a count that comes back wrong says WHICH discovery it counted
## rather than only that the number was not 1.
func _unspent_keys(controller: KnowledgePanelController) -> Array[String]:
	var keys: Array[String] = []
	for node in KnowledgeRoster.flatten(controller.domains()):
		if bool(node[HudKnowledgeVocab.NODE_UNSPENT]):
			keys.append(String(node[HudKnowledgeVocab.NODE_KEY]))
	return keys

func _roster_node(model: Dictionary, key: String) -> Dictionary:
	return _node_in(KnowledgeRoster.flatten(KnowledgeRoster.build_domains(model)), key)

## The node carrying this key in a roster the caller has ALREADY flattened. `{}` means the roster is
## not carrying it at all — a different answer from "carrying it, not known", which is why the claims
## that use this print the state they found rather than a bare bool.
func _node_in(roster: Array, key: String) -> Dictionary:
	for node in roster:
		if String(node[HudKnowledgeVocab.NODE_KEY]) == key:
			return node
	return {}

func _domain_node(root: Node, key: StringName) -> Node:
	if root is Control and (root as Control).get_meta(HudKnowledgeVocab.DOMAIN_META, "") == String(key):
		return root
	for child in root.get_children():
		var found := _domain_node(child, key)
		if found != null:
			return found
	return null

## One node's CHIP, found by the key it carries rather than by the face it wears.
func _node_chip(root: Node, key: String) -> Control:
	if root is Control and (root as Control).get_meta(HudKnowledgeVocab.NODE_META, "") == key:
		return root as Control
	for child in root.get_children():
		var found := _node_chip(child, key)
		if found != null:
			return found
	return null

## Press a node row, as a player does. See `_knowledge_frames` for why nothing here fakes a signal.
func _press_node(key: String) -> bool:
	var panel: KnowledgePanel = h._hud.knowledge_panel().panel()
	if panel == null:
		return false
	var chip := _node_chip(panel, key)
	if chip == null:
		return false
	await _click(chip)
	return true

func _press_filter(key: StringName) -> bool:
	var panel: KnowledgePanel = h._hud.knowledge_panel().panel()
	if panel == null:
		return false
	var pill := _filter_pill(panel, String(key))
	if pill == null:
		return false
	await _click(pill)
	return true

## A real left click at a control's own centre, through the viewport. **The control is FREED by the
## press** — every one of these rebuilds the panel — so nothing may touch it after this returns.
func _click(control: Control) -> void:
	var viewport: Viewport = h.get_viewport()
	var point := InputProbe.canvas_to_window(viewport, h.get_window(),
		control.get_global_rect().get_center())
	InputProbe.hover(viewport, point)
	await h.get_tree().process_frame
	InputProbe.press_left(viewport, point)
	await h.get_tree().process_frame
	InputProbe.release_left(viewport, point)
	await h._settle()

func _filter_pill(root: Node, key: String) -> Control:
	if root is Control and (root as Control).get_meta(HudKnowledgeVocab.FILTER_META, "") == key:
		return root as Control
	for child in root.get_children():
		var found := _filter_pill(child, key)
		if found != null:
			return found
	return null

# ---- fixtures ---------------------------------------------------------------

## A `KnowledgeRoster` model. Every field is passed because a partial one produces a roster whose
## verdicts were silently derived from an empty world, which is the shape of a plausible frame with a
## wrong number in it.
func _model(tracks: Dictionary, patches: Array, herds: Array) -> Dictionary:
	return {
		KnowledgeRoster.MODEL_LADDER_ROSTER: KnowledgeFx.ladder_roster(),
		KnowledgeRoster.MODEL_TRACKS: tracks,
		KnowledgeRoster.MODEL_CRAFT_KNOWLEDGE: [],
		KnowledgeRoster.MODEL_PATCHES: patches,
		KnowledgeRoster.MODEL_HERDS: herds,
		KnowledgeRoster.MODEL_RECIPES: [],
		KnowledgeRoster.MODEL_OWNED_ITEMS: {},
		KnowledgeRoster.MODEL_OWNED_MATERIALS: {},
		KnowledgeRoster.MODEL_BENCH_RECIPES: [],
		KnowledgeRoster.MODEL_LEARNED_THIS_TURN: {},
	}

## A model for the CRAFT half: every craft known, so the verdict turns on what is held rather than on
## whether the track is finished.
func _craft_model(recipes: Array, items: Dictionary, materials: Dictionary,
		bench: Array) -> Dictionary:
	var model := _model({}, [], [])
	model[KnowledgeRoster.MODEL_CRAFT_KNOWLEDGE] = _craft_knowledge_all_known()
	model[KnowledgeRoster.MODEL_RECIPES] = recipes
	model[KnowledgeRoster.MODEL_OWNED_ITEMS] = items
	model[KnowledgeRoster.MODEL_OWNED_MATERIALS] = materials
	model[KnowledgeRoster.MODEL_BENCH_RECIPES] = bench
	return model

## **THE MIXED MODEL: five filters, five DIFFERENT answers.** Two tracks known (one of them with a
## source standing on it, so exactly ONE is unspent), one close, one early, one untouched; the craft
## fan adds a known, a close and an untouched. `herding` is the one marked new this turn.
func _mixed_model() -> Dictionary:
	var model := _model(_tracks_mixed(), [_patch(4, 4, true, false)], [])
	model[KnowledgeRoster.MODEL_CRAFT_KNOWLEDGE] = _craft_knowledge_mixed()
	model[KnowledgeRoster.MODEL_RECIPES] = _recipes()
	model[KnowledgeRoster.MODEL_OWNED_ITEMS] = {ITEM_TUNIC: 1}
	model[KnowledgeRoster.MODEL_LEARNED_THIS_TURN] = {"herding": true}
	return model

## `cultivation` known WITH a tended patch under it (so in use), `herding` known with no herd at all
## (so unspent), `seed_selection` close, `penning` barely begun. Every other track the roster carries
## — `foddering` and the route branch's two — is untouched, which an absent key already says.
func _tracks_mixed() -> Dictionary:
	return {
		"cultivation": PROGRESS_KNOWN,
		"herding": PROGRESS_KNOWN,
		"seed_selection": PROGRESS_CLOSE,
		"penning": PROGRESS_EARLY,
	}

func _tracks_all_known() -> Dictionary:
	return KnowledgeFx.tracks_all_at(PROGRESS_KNOWN)

## The wire's shape for the intensification vector — a per-faction row, which is what
## `FactionReadouts` filters to the player faction.
func _wire_tracks(tracks: Dictionary) -> Dictionary:
	return KnowledgeFx.progress_row(HudConst.PLAYER_FACTION_ID, tracks)

## One craft row, in the wire's own shape. `completion_threshold` rides because the client draws no
## scale of its own — a fixture that omitted it would put every craft meter at zero.
func _craft(craft_id: String, display: String, known: bool, progress: float) -> Dictionary:
	return {
		HudCraftingVocab.CRAFT_KNOWLEDGE_FACTION_KEY: HudConst.PLAYER_FACTION_ID,
		HudCraftingVocab.CRAFT_KNOWLEDGE_CRAFT_ID_KEY: craft_id,
		HudCraftingVocab.CRAFT_KNOWLEDGE_DISPLAY_NAME_KEY: display,
		HudCraftingVocab.CRAFT_KNOWLEDGE_KNOWN_KEY: known,
		HudCraftingVocab.CRAFT_KNOWLEDGE_PROGRESS_KEY: progress,
		HudCraftingVocab.CRAFT_KNOWLEDGE_THRESHOLD_KEY: CRAFT_THRESHOLD,
	}

func _craft_knowledge_all_known() -> Array:
	return [
		_craft(CRAFT_TANNING, "Tanning", true, CRAFT_THRESHOLD),
		_craft(CRAFT_WEAVING, "Weaving", true, CRAFT_THRESHOLD),
		_craft(CRAFT_BONE, "Bone-working", true, CRAFT_THRESHOLD),
	]

## Known / close / untouched, so the craft fan contributes one node to each of three filters.
func _craft_knowledge_mixed() -> Array:
	return [
		_craft(CRAFT_TANNING, "Tanning", true, CRAFT_THRESHOLD),
		_craft(CRAFT_WEAVING, "Weaving", false, CRAFT_PROGRESS_CLOSE),
		_craft(CRAFT_BONE, "Bone-working", false, 0.0),
	]

func _craft_knowledge_untouched() -> Array:
	return [
		_craft(CRAFT_TANNING, "Tanning", false, 0.0),
		_craft(CRAFT_WEAVING, "Weaving", false, 0.0),
		_craft(CRAFT_BONE, "Bone-working", false, 0.0),
	]

## The recipe book, one recipe per craft plus a `stock` recipe whose output is a MATERIAL — the two
## output kinds `RecipeOutputState` admits, exactly one of which is set on any row.
func _recipes() -> Array:
	return [
		_recipe(RECIPE_TUNIC, CRAFT_TANNING, ITEM_TUNIC, ""),
		_recipe(RECIPE_LEATHER, CRAFT_TANNING, "", MATERIAL_LEATHER),
		_recipe(RECIPE_BASKET, CRAFT_WEAVING, "basket", ""),
		_recipe(RECIPE_AWL, CRAFT_BONE, ITEM_AWL, ""),
	]

func _recipe(id: String, craft: String, equipment_id: String, material_id: String) -> Dictionary:
	var output := {}
	if equipment_id != "":
		output[HudCraftingVocab.RECIPE_OUTPUT_EQUIPMENT_ID_KEY] = equipment_id
	if material_id != "":
		output[HudCraftingVocab.RECIPE_OUTPUT_MATERIAL_ID_KEY] = material_id
	return {
		HudCraftingVocab.RECIPE_ID_KEY: id,
		HudCraftingVocab.RECIPE_CRAFT_KEY: craft,
		HudCraftingVocab.RECIPE_OUTPUTS_KEY: [output],
	}

## A forage patch, with its standing rung DERIVED from the two flags it carries. **The rung is never
## typed** — see the class docstring.
func _patch(x: int, y: int, tended: bool, field: bool,
		owner: int = HudConst.PLAYER_FACTION_ID) -> Dictionary:
	return RungFx.stamp_patch({
		"x": x, "y": y,
		"has_owner": true,
		"owner": owner,
		"is_cultivated": tended,
		"is_field": field,
	})

## A herd, rung likewise derived. `domestication` is compared against `DOMESTICATION_COMPLETE` for the
## reason the sim stamps `animal:pastoral` there: taming has no bool of its own, its achievement IS
## its meter.
func _herd(herd_id: String, domestication: float, corralled: bool) -> Dictionary:
	return RungFx.stamp_herd({
		# **`id`, NOT `herd_id`** — `HudBandLaborState.find_world_herd` matches on `id`, so a fixture
		# keyed the other way is invisible to the assignment walk and every animal claim reads
		# "nothing is using it" for a reason that has nothing to do with the code under test.
		"id": herd_id,
		"species": "Aurochs",
		"x": 6, "y": 7,
		"domestication": domestication,
		"corralled": corralled,
	})

## A HUNT assignment — the ONLY way a herd can be attributed to the player client-side, a herd
## carrying no owner field.
func _hunt_assignment(herd_id: String) -> Dictionary:
	return {"kind": SourceForecast.LABOR_KIND_HUNT, "fauna_id": herd_id, "workers": 3}

func _band(assignments: Array) -> Dictionary:
	return BandFx.with_band_id({
		"name": "Elderford", "id": "Elderford",
		"entity": KNOWLEDGE_BAND_ENTITY,
		"faction": HudConst.PLAYER_FACTION_ID,
		"size": 30,
		"pos": [71, 18],
		"current_x": 71,
		"current_y": 18,
		"working_age": 16,
		"idle_workers": 6,
		"turns_of_food": 22.0,
		"morale": 0.8,
		"labor_assignments": assignments,
	})
