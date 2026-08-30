class_name KnowledgeRoster

## **WHAT THE FACTION KNOWS, AS ONE LIST OF NODES** — the knowledge screen's whole derivation
## (`docs/plan_knowledge_screen.md` §2), split out of `KnowledgePanel` so every claim it makes can be
## asserted without rendering anything.
##
## **THAT SPLIT IS THE POINT OF THE FILE.** The filter counts, the greyed `0.0` track, the derived
## *"nothing is using it"* verdict and the launcher's pip all render as perfectly plausible PICTURES
## whatever they say — a screenshot cannot tell a correct `2 unspent` from a wrong one. So the numbers
## are produced HERE, by pure `static` functions over dictionaries, and the harness asks this module
## rather than reading a Label back.
##
## ALL-`static`, NO STATE — the `RungGates` / `SourceForecast` shape.
##
## ---
##
## ## The three node states, and why the third is drawn
##
## `known` · `learning` (0..1) · `not begun`. **A track at `0.0` is SHOWN, GREYED.** The faction
## page's old knowledge block skipped those (`if progress <= 0.0: continue`), and that skip is what
## made the ladder invisible to a new player: a faction that had learned nothing rendered an EMPTY
## zone, so nothing on screen said there was anything to learn. Removing it is half the value of the
## arc, which is why `build_nodes` walks the DECLARED track list rather than the wire's sparse row.
##
## ## "Unspent" is DERIVED, is never persisted, and does not mean "never used"
##
## Nothing in the sim or the client records that a verb was ever exercised, and a persisted latch
## would make a claim that cannot survive a reinstall. So the question asked is the one the shipped
## fields can actually answer — **is anything using this RIGHT NOW** — and the label follows the
## meaning (`HudKnowledgeVocab.UNSPENT_CLAUSE`: *"nothing is using it"*, never *"never used"*).
##
## Arguably it is the better signal anyway: it comes BACK if the player abandons the thing, where a
## latch would go quiet forever after one use.
##
## - **A ladder knowledge** is in use when at least one of the faction's own sources STANDS ON the
##   step it unlocked — `SourceForecast.improvement_is_done`, i.e. the source's `current_rung` at or
##   above that step. At-or-**above** is deliberate: a Field is standing above Tended, and a faction
##   with a field is certainly using its Cultivation.
## - **A craft knowledge** is in use when the faction holds, or is making, something that is made of
##   it — see `craft_is_in_use`.
## - **A knowledge that unlocks nothing cannot be unspent at all.** `foddering` changes what a pen may
##   draw on rather than unlocking a step, so there is no source that could stand on it, and calling
##   it "unused" would be a sentence about nothing. **That is READ OFF THE ROSTER's `is_step`, not off
##   a declared set here** — the ladder already knows whether any rung waits on a knowledge.
##
## ## Which sources count as the faction's — and why the two webs answer differently
##
## **This is forced by the wire, not chosen.** A forage patch carries `owner` / `has_owner`, so an
## ownership scan of every patch is attributable — the same test `AttentionController`'s under-kept
## rung producer makes. **A herd carries no owner field client-side at all**, so the only way to say
## "ours" is through a band's own hunt assignments, which is exactly why
## `_starving_pen_attention` and `_under_kept_herd_attention` walk assignments instead of
## `world_herds()`. The caller resolves both (`KnowledgePanelController`) and hands the two arrays in.
##
## The consequence worth knowing: a PEN whose keepers were all reassigned drops out of the animal
## scan, so Penning can read unspent while the fence still stands. That is the present-tense reading
## doing its job — nobody is working it — and it is the same blindness every other herd-scoped
## producer in this HUD has.

## The improvement verb each knowledge track gates, taken from `RungGates.RUNG_KNOWLEDGE_TRACKS`
## INVERTED rather than restated. That table is what the compose sheet gates on, so reading it
## backwards is what makes "using it" here and "allowed" there the same question — a second table
## would be a second answer, and the one that drifted would be this one, since nothing else reads it.
##
## `IMPROVEMENT_NONE` for a track that gates no verb.
static func unlock_for_track(track: String) -> String:
	for improvement in RungGates.RUNG_KNOWLEDGE_TRACKS:
		if String(RungGates.RUNG_KNOWLEDGE_TRACKS[improvement]) == track:
			return String(improvement)
	return SourceForecast.IMPROVEMENT_NONE

## **THE WHOLE ROSTER: every domain that has nodes, each holding its nodes in order.**
##
## ⛔ **THE LADDER COLUMNS ARE BUILT FROM THE WIRE, NOT FROM A DECLARED LIST.** The sim publishes one
## `ladder_knowledge` row per knowledge the ladder teaches, carrying the BRANCH of the rung that
## teaches it (which column), that rung's ORDER (where in the column) and whether any rung's
## `unlock_knowledge` names it (step, or capability hanging off the bottom). All three are derived
## from `intensification_ladder.json` sim-side, so **a knowledge added to that config appears here
## with no client edit** — which is exactly what the retired hard-coded `LADDER_DOMAINS` could not do,
## and why the route branch's Roadbuilding and Paving had nowhere to show.
##
## `model` is one dictionary rather than ten parameters because every field is needed to answer any
## node's `unspent`, and a caller that could pass some of them would produce a roster whose verdicts
## were silently derived from an empty world. See `MODEL_*` for the keys.
static func build_domains(model: Dictionary) -> Array[Dictionary]:
	var domains: Array[Dictionary] = []
	# **COLUMN ORDER IS THE ROSTER'S OWN**, i.e. the ladder config's rung order — first branch to
	# teach anything is the first column. Declaring an order here would be one more thing to edit when
	# a branch is added, which is the whole defect this replaced.
	var branch_order: Array[StringName] = []
	var by_branch := {}
	for entry_variant in model.get(MODEL_LADDER_ROSTER, []):
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var branch := StringName(String(entry.get(HudKnowledgeVocab.ROSTER_BRANCH, "")))
		if String(branch) == "":
			continue
		if not by_branch.has(branch):
			by_branch[branch] = []
			branch_order.append(branch)
		(by_branch[branch] as Array).append(entry)
	for branch in branch_order:
		var rows: Array = by_branch[branch]
		# **WITHIN A COLUMN, THE ORDER IS THE TEACHING RUNG'S**, bottom step first, so a column read
		# top to bottom reads as a progression. A CAPABILITY sorts after every step at the same rung —
		# `foddering` is taught by the top animal rung and belongs under the chain, not inside it.
		rows.sort_custom(_before_on_the_ladder)
		var nodes: Array[Dictionary] = []
		for row_variant in rows:
			nodes.append(_ladder_node(row_variant as Dictionary, branch, model))
		# **NEVER DRAW AN EMPTY DOMAIN COLUMN** — a column appears the turn its first branch does, and
		# an empty one teaches the player that a whole area of the game is closed to them when in
		# truth it does not exist yet. A branch reaches `branch_order` only by having a row, so this
		# cannot happen today; it is stated because the guard is the rule.
		if nodes.is_empty():
			continue
		domains.append({
			HudKnowledgeVocab.DOMAIN_KEY: branch,
			HudKnowledgeVocab.DOMAIN_LABEL: HudKnowledgeVocab.domain_label(branch),
			HudKnowledgeVocab.DOMAIN_SHAPE: HudKnowledgeVocab.DOMAIN_SHAPE_LADDER,
			HudKnowledgeVocab.DOMAIN_NODES: nodes,
		})
	var crafts: Array[Dictionary] = []
	for track_variant in model.get(MODEL_CRAFT_KNOWLEDGE, []):
		if track_variant is Dictionary:
			crafts.append(_craft_node(track_variant as Dictionary, model))
	if not crafts.is_empty():
		domains.append({
			HudKnowledgeVocab.DOMAIN_KEY: HudKnowledgeVocab.DOMAIN_KEY_CRAFT,
			HudKnowledgeVocab.DOMAIN_LABEL: HudKnowledgeVocab.DOMAIN_CRAFT_LABEL,
			HudKnowledgeVocab.DOMAIN_SHAPE: HudKnowledgeVocab.DOMAIN_SHAPE_FAN,
			HudKnowledgeVocab.DOMAIN_NODES: crafts,
		})
	return domains

## Ladder order within one column: by the teaching rung's `order`, and a CAPABILITY after a STEP that
## shares it. The second term is what puts `foddering` under the animal chain rather than beside
## `penning` — both are taught by rungs of the same branch, and only one of them is a step somebody
## climbs to.
static func _before_on_the_ladder(a: Dictionary, b: Dictionary) -> bool:
	var order_a := int(a.get(HudKnowledgeVocab.ROSTER_ORDER, 0))
	var order_b := int(b.get(HudKnowledgeVocab.ROSTER_ORDER, 0))
	if order_a != order_b:
		return order_a < order_b
	return bool(a.get(HudKnowledgeVocab.ROSTER_IS_STEP, false)) \
		and not bool(b.get(HudKnowledgeVocab.ROSTER_IS_STEP, false))

## Every node of every domain, flattened — what the tally and the filter counts are taken over, so
## they cannot be computed off a different list than the one drawn.
static func flatten(domains: Array) -> Array[Dictionary]:
	var nodes: Array[Dictionary] = []
	for domain_variant in domains:
		if not (domain_variant is Dictionary):
			continue
		for node_variant in (domain_variant as Dictionary).get(HudKnowledgeVocab.DOMAIN_NODES, []):
			if node_variant is Dictionary:
				nodes.append(node_variant as Dictionary)
	return nodes

## **DOES THIS NODE MATCH THIS FILTER?** One function for the pill's COUNT and for the row's DIMMING,
## which is what stops a pill reading `2` over a body that dims three — the failure a separate count
## would produce silently, since both look right on their own.
static func matches(node: Dictionary, filter: StringName) -> bool:
	match filter:
		HudKnowledgeVocab.FILTER_LEARNING:
			return String(node.get(HudKnowledgeVocab.NODE_STATE, "")) == HudKnowledgeVocab.NODE_STATE_LEARNING
		HudKnowledgeVocab.FILTER_CLOSE:
			# **CLOSE IS A SUBSET OF LEARNING, NOT OF EVERYTHING.** A `known` node is at 1.0 and would
			# pass a bare `progress >= CLOSE_FRACTION`, which would put every finished track in a
			# filter whose whole question is "what would finish if I kept at it".
			return String(node.get(HudKnowledgeVocab.NODE_STATE, "")) == HudKnowledgeVocab.NODE_STATE_LEARNING \
				and float(node.get(HudKnowledgeVocab.NODE_PROGRESS, 0.0)) >= HudKnowledgeVocab.CLOSE_FRACTION
		HudKnowledgeVocab.FILTER_UNUSED:
			return bool(node.get(HudKnowledgeVocab.NODE_UNSPENT, false))
		HudKnowledgeVocab.FILTER_NEW:
			return bool(node.get(HudKnowledgeVocab.NODE_NEW, false))
		_:
			return true

## How many of `nodes` a filter matches — the number on its pill.
static func count_matching(nodes: Array, filter: StringName) -> int:
	var found := 0
	for node_variant in nodes:
		if node_variant is Dictionary and matches(node_variant as Dictionary, filter):
			found += 1
	return found

## The header's tally: how many are known, being learned, untouched, and earned-but-idle. Taken over
## the SAME flattened list the columns draw, for `matches`' reason.
static func tally(nodes: Array) -> Dictionary:
	var known := 0
	var learning := 0
	var not_begun := 0
	for node_variant in nodes:
		if not (node_variant is Dictionary):
			continue
		match String((node_variant as Dictionary).get(HudKnowledgeVocab.NODE_STATE, "")):
			HudKnowledgeVocab.NODE_STATE_KNOWN:
				known += 1
			HudKnowledgeVocab.NODE_STATE_LEARNING:
				learning += 1
			_:
				not_begun += 1
	return {
		HudKnowledgeVocab.NODE_STATE_KNOWN: known,
		HudKnowledgeVocab.NODE_STATE_LEARNING: learning,
		HudKnowledgeVocab.NODE_STATE_NOT_BEGUN: not_begun,
		TALLY_UNSPENT: count_matching(nodes, HudKnowledgeVocab.FILTER_UNUSED),
	}

## The unspent key on a `tally` result. The other three are the node states themselves.
const TALLY_UNSPENT := "unspent"

# ---- the model the caller hands in ---------------------------------------------------------------
## **WHAT THERE IS TO LEARN** — the ladder's knowledge roster as the wire sent it
## (`FactionReadouts.ladder_knowledge`), one row per knowledge. **This is the DECLARATION**: the
## columns, their order and each node's place in them are read off it, so the panel needs no track
## list of its own.
const MODEL_LADDER_ROSTER := "ladder_roster"
## `{track: 0..1}` — the player faction's intensification row, `FactionReadouts.faction_tracks`.
const MODEL_TRACKS := "tracks"
## The player faction's craft rows, already filtered to `PLAYER_FACTION_ID` by whoever ingests them.
const MODEL_CRAFT_KNOWLEDGE := "craft_knowledge"
## The faction's OWN forage patches — filtered on the patch's own `owner`, which the wire carries.
const MODEL_PATCHES := "patches"
## The herds the faction's bands WORK — resolved through their hunt assignments, because a herd
## carries no owner field client-side.
const MODEL_HERDS := "herds"
## The road TILES the faction's own bands KEEP. **A road is the one source with no labor row**, so
## ownership is read straight off the road (`has_keeper` / `keeper_band_id`) rather than through
## assignments the way a herd's is, and rather than through an `owner` field the way a patch's is —
## a third answer to *"is this ours"*, forced by a third wire shape.
const MODEL_ROADS := "roads"
## The world's recipe book (`SubsistenceSection.recipes`), for the craft half's "made of it" join.
const MODEL_RECIPES := "recipes"
## `{item_id: count}` and `{material_id: amount}`, summed over the player's bands.
const MODEL_OWNED_ITEMS := "owned_items"
const MODEL_OWNED_MATERIALS := "owned_materials"
## Recipe ids currently ON a player band's bench.
const MODEL_BENCH_RECIPES := "bench_recipes"
## `{knowledge_key: true}` for whatever finished THIS turn — the caller's diff, never derived here
## (this module is pure and sees one snapshot).
const MODEL_LEARNED_THIS_TURN := "learned_this_turn"

# ---- the two node builders -----------------------------------------------------------------------

## One node of a ladder column, built from its ROSTER ROW — the wire's declaration of the knowledge —
## crossed with this faction's progress on it.
static func _ladder_node(entry: Dictionary, domain: StringName, model: Dictionary) -> Dictionary:
	var track := String(entry.get(HudKnowledgeVocab.ROSTER_KNOWLEDGE_ID, ""))
	var tracks: Dictionary = model.get(MODEL_TRACKS, {})
	# **THE ROSTER IS WALKED, NOT THE PROGRESS ROW**, so a track the faction has never touched is an
	# absent key that reads `0.0` and becomes a GREYED node — rather than no node at all, which is
	# what the old faction-page block did. It is also why the roster carries no faction: a faction
	# with no progress row at all still has a full screen to look at.
	var progress := clampf(float(tracks.get(track, 0.0)), 0.0, 1.0)
	var state := _state_for(progress, progress >= HudConst.KNOWLEDGE_COMPLETE)
	var unlocks := unlock_for_track(track)
	# ⛔ **STEP OR CAPABILITY IS THE CONFIG'S ANSWER, and `unspent` follows it.** A knowledge no rung
	# waits on has nothing to stand on it, so *"nothing is using it"* would be a sentence about
	# nothing. The client used to declare that set; the roster derives it from the ladder.
	var is_step := bool(entry.get(HudKnowledgeVocab.ROSTER_IS_STEP, false))
	var testable := is_step and unlocks != SourceForecast.IMPROVEMENT_NONE
	var in_use := ladder_in_use_count(unlocks, model) if testable else 0
	# The player-facing name is the SIM's (`display_name`), so no client surface authors a second
	# spelling of a discovery. The raw id is the honest fallback for a row that carried none.
	var label := String(entry.get(HudKnowledgeVocab.ROSTER_DISPLAY_NAME, "")).strip_edges()
	return {
		HudKnowledgeVocab.NODE_KEY: track,
		HudKnowledgeVocab.NODE_LABEL: label if label != "" else track,
		HudKnowledgeVocab.NODE_DOMAIN: domain,
		HudKnowledgeVocab.NODE_STATE: state,
		HudKnowledgeVocab.NODE_PROGRESS: progress,
		HudKnowledgeVocab.NODE_UNLOCKS: unlocks,
		HudKnowledgeVocab.NODE_IN_USE_COUNT: in_use,
		HudKnowledgeVocab.NODE_UNSPENT_TESTABLE: testable,
		# **ONLY A KNOWN NODE CAN BE UNSPENT.** A track at 40% has nothing standing on it either, and
		# calling that "earned and unused" would put every unlearned thing in the nudge count.
		HudKnowledgeVocab.NODE_UNSPENT: state == HudKnowledgeVocab.NODE_STATE_KNOWN \
			and testable and in_use == 0,
		# The unlock copy is READ from `FactionReadouts`, not re-authored — one sentence per track, so no
		# two surfaces naming a discovery can describe it differently. That table OUTLIVED the one-shot
		# unlock announcement it was written for (retired, §5); this pane is its reader now. **It is
		# COPY, not structure**: a knowledge with no note draws with nothing to say rather than not
		# drawing, which is what keeps the roster wire-driven.
		HudKnowledgeVocab.NODE_NOTE: String(FactionReadouts.KNOWLEDGE_UNLOCK_NOTES.get(track, "")),
		HudKnowledgeVocab.NODE_PRACTISE: String(HudKnowledgeVocab.PRACTISE_NOTES.get(track, "")),
		HudKnowledgeVocab.NODE_NEW: _is_new(track, state, model),
	}

static func _craft_node(row: Dictionary, model: Dictionary) -> Dictionary:
	var craft_id := String(row.get(HudCraftingVocab.CRAFT_KNOWLEDGE_CRAFT_ID_KEY, ""))
	var known := bool(row.get(HudCraftingVocab.CRAFT_KNOWLEDGE_KNOWN_KEY, false))
	var display := String(row.get(HudCraftingVocab.CRAFT_KNOWLEDGE_DISPLAY_NAME_KEY, "")).strip_edges()
	if display == "":
		display = craft_id
	var in_use := craft_is_in_use(craft_id, model)
	return {
		HudKnowledgeVocab.NODE_KEY: craft_id,
		HudKnowledgeVocab.NODE_LABEL: display,
		HudKnowledgeVocab.NODE_DOMAIN: HudKnowledgeVocab.DOMAIN_KEY_CRAFT,
		HudKnowledgeVocab.NODE_STATE: _state_for(craft_fraction(row), known),
		HudKnowledgeVocab.NODE_PROGRESS: craft_fraction(row),
		# A craft gates RECIPES rather than a step a source stands on, so it names no improvement verb
		# — but it is still unspent-testable, because "is anything made of it" is answerable.
		HudKnowledgeVocab.NODE_UNLOCKS: SourceForecast.IMPROVEMENT_NONE,
		HudKnowledgeVocab.NODE_IN_USE_COUNT: 1 if in_use else 0,
		HudKnowledgeVocab.NODE_UNSPENT_TESTABLE: true,
		HudKnowledgeVocab.NODE_UNSPENT: known and not in_use,
		HudKnowledgeVocab.NODE_NOTE: HudKnowledgeVocab.CRAFT_UNLOCK_NOTE_FORMAT % display,
		HudKnowledgeVocab.NODE_PRACTISE: HudKnowledgeVocab.CRAFT_PRACTISE_NOTE,
		HudKnowledgeVocab.NODE_NEW: _is_new(craft_id, _state_for(craft_fraction(row), known), model),
	}

## **A CRAFT TRACK'S 0..1, AGAINST THE SIM'S OWN DENOMINATOR.** `completion_threshold` rides on the
## wire precisely so the client draws no scale of its own (`CraftKnowledgeState`), and a guessed
## denominator would put this panel's meter and the crafting panel's rail at different fills for one
## track. A `known` craft reads `1.0` whatever its raw progress says — the sim's `known` is the
## authority on completion, not an inequality the client re-derives.
static func craft_fraction(row: Dictionary) -> float:
	if bool(row.get(HudCraftingVocab.CRAFT_KNOWLEDGE_KNOWN_KEY, false)):
		return 1.0
	var threshold := float(row.get(HudCraftingVocab.CRAFT_KNOWLEDGE_THRESHOLD_KEY, 0.0))
	if threshold <= 0.0:
		return 0.0
	return clampf(float(row.get(HudCraftingVocab.CRAFT_KNOWLEDGE_PROGRESS_KEY, 0.0)) / threshold, 0.0, 1.0)

## **"NEW THIS TURN" IMPLIES KNOWN, and the conjunction is what makes it coherent.** The caller's diff
## is a set of keys that FINISHED during the turn, and within one turn a track cannot un-finish — but a
## world boundary or a rehydrated save can hand back a roster in which a key of that set is no longer
## complete, and a node marked new while reading `not begun` is a sentence about nothing. It showed up
## the moment a fixture pushed an empty tracks row after a completion: the pill read `New this turn 1`
## over a faction that knew nothing at all.
static func _is_new(key: String, state: String, model: Dictionary) -> bool:
	if state != HudKnowledgeVocab.NODE_STATE_KNOWN:
		return false
	return bool((model.get(MODEL_LEARNED_THIS_TURN, {}) as Dictionary).get(key, false))

## `known` is the sim's flag / the `>= KNOWLEDGE_COMPLETE` crossing; everything above zero is being
## learned; zero is untouched. **`not_begun` is a STATE, not an absence** — see the file docstring.
static func _state_for(progress: float, known: bool) -> String:
	if known:
		return HudKnowledgeVocab.NODE_STATE_KNOWN
	if progress > 0.0:
		return HudKnowledgeVocab.NODE_STATE_LEARNING
	return HudKnowledgeVocab.NODE_STATE_NOT_BEGUN

# ---- "is anything using it" -----------------------------------------------------------------------

## **HOW MANY OF THE FACTION'S SOURCES STAND ON THE STEP `improvement` BUILDS.**
##
## `SourceForecast.improvement_is_done` and nothing else — it reads the wire's `current_rung` against
## the rung the verb builds, which is ONE field for both webs and the same call the compose sheet's
## own "already built" test makes. **Never the per-verb done FLAGS**: those answer "was this exact
## verb run", and a Field reached by `Sow` carries no `is_cultivated`, so a flag test would report a
## faction with a working field as not using its Cultivation.
##
## Which pool is scanned is a property of the VERB (`FORAGE_IMPROVEMENTS` / `HUNT_IMPROVEMENTS` /
## `ROUTE_IMPROVEMENTS`), not a branch here: a plant verb can only be built on a patch, an animal verb
## only on a herd, and a route verb only on a road tile.
##
## ⛔ **THE ROAD ARM ASKS THE SAME QUESTION THROUGH A DIFFERENT FIELD NAME.** A patch and a herd
## publish their standing as `current_rung`; a road publishes its as `rung`. So the road arm makes
## `improvement_is_done`'s own comparison — `rung_at_or_above(standing, the rung this verb builds)` —
## rather than a second rule, which is what keeps *"is anything using this"* one question on three webs.
static func ladder_in_use_count(improvement: String, model: Dictionary) -> int:
	if improvement == SourceForecast.IMPROVEMENT_NONE:
		return 0
	if SourceForecast.ROUTE_IMPROVEMENTS.has(improvement):
		return _roads_standing_on(improvement, model)
	var pool: Array = []
	if SourceForecast.FORAGE_IMPROVEMENTS.has(improvement):
		pool = model.get(MODEL_PATCHES, [])
	elif SourceForecast.HUNT_IMPROVEMENTS.has(improvement):
		pool = model.get(MODEL_HERDS, [])
	var found := 0
	for source_variant in pool:
		if not (source_variant is Dictionary):
			continue
		if SourceForecast.improvement_is_done(source_variant as Dictionary,
				HudComposeVocab.BARE_FORECAST_PREFIX, improvement):
			found += 1
	return found

## **HOW MANY OF THE FACTION'S OWN ROAD TILES STAND ON THE RUNG `improvement` BUILDS.**
##
## The caller has already filtered the pool to roads this faction's bands KEEP — that is where
## ownership lives on this web — so the only question left here is the rung, asked at-or-ABOVE for the
## reason every other web asks it that way: a paved road is standing above a dirt road, and a faction
## with a paved road is certainly using its Roadbuilding.
static func _roads_standing_on(improvement: String, model: Dictionary) -> int:
	var target := String(SourceForecast.IMPROVEMENT_RUNG_KEYS.get(improvement, ""))
	if target == "":
		return 0
	var found := 0
	for road_variant in model.get(MODEL_ROADS, []):
		if not (road_variant is Dictionary):
			continue
		if SourceForecast.rung_at_or_above(HudRouteVocab.rung_of(road_variant as Dictionary), target):
			found += 1
	return found

## **IS ANYTHING THE FACTION HOLDS, OR IS MAKING, MADE OF THIS CRAFT?**
##
## A craft is the knowledge that lets a bench WORK a material, and every recipe names the craft it
## needs (`RecipeDefState.craft`). So the join is: any recipe of this craft whose OUTPUT the faction
## is carrying, or which is on a bench right now.
##
## **IT IS NOT "does a recipe of this craft exist in the ledger".** The crafting panel publishes ONE
## ROW PER RECIPE, ALWAYS — that is its contract, so a recipe's mere presence is true of every craft
## on every turn and would answer "in use" for all of them forever.
##
## **OWNERSHIP IS `count` / `amount`, NEVER `remaining`** — the crafting panel's own rule: a batch
## that runs out of units is REMOVED, so a worn-out item and one never made both read `remaining 0`,
## and only the count can tell "we have some" from "we have none".
##
## The bench arm is what keeps a faction that has just started its first loom from reading unspent
## the whole time the loom is being built.
static func craft_is_in_use(craft_id: String, model: Dictionary) -> bool:
	if craft_id == "":
		return false
	var owned_items: Dictionary = model.get(MODEL_OWNED_ITEMS, {})
	var owned_materials: Dictionary = model.get(MODEL_OWNED_MATERIALS, {})
	var bench: Array = model.get(MODEL_BENCH_RECIPES, [])
	for recipe_variant in model.get(MODEL_RECIPES, []):
		if not (recipe_variant is Dictionary):
			continue
		var recipe: Dictionary = recipe_variant
		if String(recipe.get(HudCraftingVocab.RECIPE_CRAFT_KEY, "")) != craft_id:
			continue
		if bench.has(String(recipe.get(HudCraftingVocab.RECIPE_ID_KEY, ""))):
			return true
		for output_variant in recipe.get(HudCraftingVocab.RECIPE_OUTPUTS_KEY, []):
			if not (output_variant is Dictionary):
				continue
			var output: Dictionary = output_variant
			# **EXACTLY ONE OF THE TWO IS SET** on a recipe output (`RecipeOutputState`), so both are
			# asked and an unset one is an absent key rather than a zero to compare.
			var item_id := String(output.get(HudCraftingVocab.RECIPE_OUTPUT_EQUIPMENT_ID_KEY, ""))
			if item_id != "" and int(owned_items.get(item_id, 0)) > 0:
				return true
			var material_id := String(output.get(HudCraftingVocab.RECIPE_OUTPUT_MATERIAL_ID_KEY, ""))
			if material_id != "" and float(owned_materials.get(material_id, 0.0)) > 0.0:
				return true
	return false
