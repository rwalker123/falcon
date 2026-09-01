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
const EXPECTED_CHECKPOINTS := 86

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
	# ⛔ **THE ROSTER GOES IN FIRST, AND WITHOUT IT THERE IS NO LADDER AT ALL.** The columns are built
	# from the wire now, so this push is what the panel is made of — not a detail of one state.
	h._hud.update_ladder_knowledge(KnowledgeFx.ladder_roster())
	_assert_greyed_zero_tracks()
	_assert_the_roster_builds_the_columns()
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
	# **NO CRAFT COLUMN AT ALL when the wire has published no craft vector**, which is the "never draw
	# an empty domain column" rule. Every column's nodes come off the wire now, so this is the general
	# case rather than the craft fan's special one — the craft vector is simply the one this model
	# leaves out.
	var craft_columns := 0
	for domain in domains:
		if StringName(domain[HudKnowledgeVocab.DOMAIN_KEY]) == HudKnowledgeVocab.DOMAIN_KEY_CRAFT:
			craft_columns += 1
	h._assert_hud("knowledge — a domain with no nodes draws NO column (craft columns %d)" % craft_columns,
		craft_columns == 0)

## ⛔ **THE PANEL BUILDS ITSELF FROM THE ROSTER — THIS IS THE CLAIM THE ARC IS ABOUT.**
##
## Land and Herds used to be hard-coded node lists in `HudKnowledgeVocab`, and the reason was that
## their WIRE was hard-coded too: the ladder's knowledges rode as named float fields, so adding one
## meant adding a schema field. That is why the route branch's two lessons had nowhere to appear and
## the header went on saying *"All 8"*.
##
## Four claims, and each fails differently:
##
## 1. **A ROADS COLUMN EXISTS, carrying Roadbuilding and Paving** — the visible proof, because nothing
##    in the client names either knowledge and no client edit put them there.
## 2. **The column a knowledge lands in is the BRANCH of the rung that teaches it**, so Paving is on
##    Roads and Penning is on Herds.
## 3. **`foddering` is a CAPABILITY and it FALLS OUT** — no rung's `unlock_knowledge` names it, so the
##    roster says `is_step: false` and it cannot be unspent. A client-side declared set is what this
##    replaced, and the difference is that a knowledge which stops gating a rung now stops being a
##    step with nothing to remember to edit.
## 4. ⛔ **REMOVE A KNOWLEDGE FROM THE ROSTER AND THE PANEL DROPS IT, WITH NO CLIENT EDIT.** This is
##    the falsification, run in the direction that needs no config file: if the node survives, the
##    panel is drawing from something other than the wire.
func _assert_the_roster_builds_the_columns() -> void:
	var roster := KnowledgeFx.ladder_roster()
	var domains := KnowledgeRoster.build_domains({KnowledgeRoster.MODEL_LADDER_ROSTER: roster})
	var roads := _domain_by_key(domains, HudKnowledgeVocab.DOMAIN_KEY_ROUTES)
	h._assert_hud("knowledge — the ROADS column exists, built from the wire's roster alone",
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

	# CLAIM 2 — the branch of the TEACHING rung decides the column. Asserted on a knowledge from each
	# ladder web, so a producer that put everything in one column fails.
	var herds := _domain_by_key(domains, HudKnowledgeVocab.DOMAIN_KEY_HERDS)
	var land := _domain_by_key(domains, HudKnowledgeVocab.DOMAIN_KEY_LAND)
	h._assert_hud("knowledge — a knowledge sits in the column of the branch that TEACHES it",
		_node_keys(land).has(KnowledgeFx.KNOWLEDGE_SEED_SELECTION)
			and _node_keys(herds).has(KnowledgeFx.KNOWLEDGE_PENNING)
			and not _node_keys(land).has(KnowledgeFx.KNOWLEDGE_PENNING))

	# CLAIM 3 — step vs capability, and it is the ROSTER's answer rather than a set in the client.
	var fodder := _node_of(herds, KnowledgeFx.KNOWLEDGE_FODDERING)
	var penning := _node_of(herds, KnowledgeFx.KNOWLEDGE_PENNING)
	h._assert_hud("knowledge — `foddering` is a CAPABILITY, so `unspent` cannot be asked of it",
		not fodder.is_empty()
			and not bool(fodder[HudKnowledgeVocab.NODE_UNSPENT_TESTABLE]))
	h._assert_hud("knowledge — …while a STEP beside it in the same column can be asked",
		not penning.is_empty() and bool(penning[HudKnowledgeVocab.NODE_UNSPENT_TESTABLE]))
	h._assert_hud("knowledge — …and the capability sorts UNDER the chain, last on its column (%s)"
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

## One domain out of a built roster, `{}` when the columns hold none of that branch.
func _domain_by_key(domains: Array, key: StringName) -> Dictionary:
	for domain_variant in domains:
		var domain: Dictionary = domain_variant
		if StringName(domain[HudKnowledgeVocab.DOMAIN_KEY]) == key:
			return domain
	return {}

## A column's node keys, in the order the column draws them.
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
	# The header's tally is taken over the SAME flattened list the columns draw, so the three state
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
	# was called from, so the pip moved while the diff (and an open panel's columns) stayed a turn
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

## The LAYOUT, which is the one thing a picture is the right witness for: two ladder columns with
## their rails, a craft fan with none, the filter pills, and the detail pane.
func _knowledge_frames() -> void:
	h._hud.update_band_alerts([_band([_hunt_assignment("worked")])])
	h._hud.update_forage_patches([_patch(6, 6, true, false)])
	h._set_world_herds([_herd("worked", SourceForecast.DOMESTICATION_COMPLETE, true)])
	h._hud.update_crafting_catalogues([], [], _recipes(), _craft_knowledge_mixed())
	h._hud.update_intensification([_wire_tracks(_tracks_mixed())])
	await h._settle()

	# STATE 1 — the whole screen. A known track, one close, one barely begun, one untouched, and the
	# craft fan beside them.
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

	# STATE 3 — a node SELECTED, so the detail pane's three sections render: what it lets you do, how
	# it is learned, and where it stands now.
	h._hud.update_intensification([_wire_tracks(_tracks_mixed())])
	h._hud.update_crafting_catalogues([], [], _recipes(), _craft_knowledge_mixed())
	await h._settle()
	var row := NodeQuery.find_meta_node(h._hud.knowledge_panel().panel(), HudKnowledgeVocab.NODE_META)
	h._assert_hud("knowledge — a node row is a control the harness can find by identity",
		row != null)
	# **DRIVEN AS A REAL POINTER PRESS, never `pressed.emit()`.** A row is a `PanelContainer` with a
	# `gui_input` handler (a Button is not a Container, so it could not lay its face out), and it has no
	# signal of its own to fake — but the rule is the harness contract's either way: an emitted signal
	# calls the connected lambda by hand and passes on a control that is covered, zero-size or filtered
	# out of the hit test, which is exactly the shape this row shipped in first.
	await _press_node("cultivation")
	_assert_detail_pane()
	await h._save("knowledge_panel_detail")

	# **STATE 4 — A FILTER LIVE, so the DIMMING is in a frame.** Non-matching nodes keep their place
	# at `FILTERED_OUT_ALPHA`; the shape of the tree is most of what this screen teaches.
	var pill := NodeQuery.find_meta_node(h._hud.knowledge_panel().panel(), HudKnowledgeVocab.FILTER_META)
	h._assert_hud("knowledge — a filter pill is a control the harness can find by identity",
		pill != null)
	await _press_filter(HudKnowledgeVocab.FILTER_LEARNING)
	_assert_filter_dims_rather_than_hides()
	await h._save("knowledge_panel_filtered")

	h._hud.close_knowledge_panel()
	await h._settle()

## The columns are there, the ladder domains draw a rail and the craft fan does not, and a `0.0` track
## is on screen as a row rather than absent.
func _assert_panel_renders() -> void:
	var panel: KnowledgePanel = h._hud.knowledge_panel().panel()
	h._assert_hud("knowledge — the panel is open", panel != null and panel.is_open())
	if panel == null:
		return
	h._assert_hud("knowledge — the LAND column rendered",
		NodeQuery.has_label_containing(panel, "Land".to_upper()))
	h._assert_hud("knowledge — the CRAFT column rendered",
		NodeQuery.has_label_containing(panel, HudKnowledgeVocab.DOMAIN_CRAFT_LABEL.to_upper()))
	# **THE RAIL IS THE DOMAIN'S SHAPE, DRAWN**, and it is the one thing that says the ladder columns
	# are ORDERED. A ladder domain has one; the craft fan must not.
	var land := _domain_node(panel, HudKnowledgeVocab.DOMAIN_KEY_LAND)
	var craft := _domain_node(panel, HudKnowledgeVocab.DOMAIN_KEY_CRAFT)
	h._assert_hud("knowledge — a LADDER column draws its rail",
		land != null and NodeQuery.find_meta_node(land, HudKnowledgeVocab.RAIL_META) != null)
	h._assert_hud("knowledge — …and the CRAFT fan draws none",
		craft != null and NodeQuery.find_meta_node(craft, HudKnowledgeVocab.RAIL_META) == null)
	# The `not begun` word is the greyed track's own value cell — a row that had been skipped would
	# leave it nowhere on screen.
	h._assert_hud("knowledge — an untouched track renders its `%s` row" % HudKnowledgeVocab.NODE_VALUE_NOT_BEGUN,
		NodeQuery.has_label_containing(panel, HudKnowledgeVocab.NODE_VALUE_NOT_BEGUN))

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
		var row := _node_row(panel, String(node[HudKnowledgeVocab.NODE_KEY]))
		if row == null:
			continue
		rendered += 1
		var host := row.get_parent()
		while host != null and not (host is VBoxContainer):
			host = host.get_parent()
		var alpha := (host as Control).modulate.a if host is Control else 1.0
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
## ordinary card — the columns are the same, the pills are the same, and only which pill is lit says
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
	h._hud.turn_orb.panel_requested.emit(HudAttentionVocab.ATTENTION_KIND_KNOWLEDGE_LEARNED)
	await h._settle()
	h._assert_hud("knowledge route — the orb's knowledge row OPENS the screen", controller.is_open())
	h._assert_hud("knowledge route — …on `%s`, overriding the `%s` it was left on (lit pill: `%s`)"
			% [HudKnowledgeVocab.FILTER_NEW, HudKnowledgeVocab.FILTER_UNUSED, _live_filter()],
		_live_filter() == HudKnowledgeVocab.FILTER_NEW)
	# **PRESSED AGAIN WHILE THE SCREEN IS ALREADY OPEN, it must not toggle shut.** The launcher glyph is
	# a toggle because pressing it means *show me / hide it*; an attention row means *take me to this*,
	# and a press that closed the thing it points at would answer a question nobody asked.
	h._hud.turn_orb.panel_requested.emit(HudAttentionVocab.ATTENTION_KIND_KNOWLEDGE_LEARNED)
	await h._settle()
	h._assert_hud("knowledge route — a row pressed while the screen is open does NOT toggle it shut",
		controller.is_open())
	h._assert_hud("knowledge route — …and it is still on `%s` (lit pill: `%s`)"
			% [HudKnowledgeVocab.FILTER_NEW, _live_filter()],
		_live_filter() == HudKnowledgeVocab.FILTER_NEW)
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
	# column looks like on the frame the baseline is seeded from.
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

func _node_row(root: Node, key: String) -> Control:
	if root is Control and (root as Control).get_meta(HudKnowledgeVocab.NODE_META, "") == key:
		return root as Control
	for child in root.get_children():
		var found := _node_row(child, key)
		if found != null:
			return found
	return null

## Press a node row, as a player does. See `_knowledge_frames` for why nothing here fakes a signal.
func _press_node(key: String) -> bool:
	var panel: KnowledgePanel = h._hud.knowledge_panel().panel()
	if panel == null:
		return false
	var row := _node_row(panel, key)
	if row == null:
		return false
	await _click(row)
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
		"id": "Band 1",
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
