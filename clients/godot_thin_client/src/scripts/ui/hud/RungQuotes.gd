extends RefCounted
class_name RungQuotes

## **THE LADDER'S PER-RUNG QUOTES, READ GENERICALLY** (issue #175).
##
## A patch, a herd and each plant in a patch's basket publish `rung_quotes`: one quote per rung ABOVE
## the branch's root, in climb order, each keyed by the catalog's `rung_key` and carrying that rung's
## own numbers (`reached`, `progress`, `work_done`, `work_cost`, `food_yield`, ...). Nothing in the
## client names a rung by a hard-coded key any more: **which rung a number belongs to is a JOIN
## against `ladder_rungs`**, done once where a snapshot arrives (`enrich_sources`), which stamps each
## quote with the rung's `verb`, `order`, `branch`, `traits`, `unlock_knowledge` and `label`. Every
## reader below then asks by VERB (the unit a command and a control already speak) or by TRAIT (what a
## rung IS: `enclosed` is a pen; the gate bools say the rest), so a fourth step added to the config appears with no
## GDScript edit.
##
## ⛔ **A QUOTE THAT THE CATALOG DOES NOT CARRY IS KEPT, NOT DROPPED**: it has no verb and no order, so
## it sorts last and no verb-keyed reader finds it, but it is still in `quotes()` for a renderer that
## wants every rung.
##
## All-`static`, stateless: the catalog arrives as an argument and nothing is held here.

const QUOTES_KEY := "rung_quotes"
const KEY_RUNG := "rung_key"
const KEY_VERB := "verb"
const KEY_ORDER := "order"
const KEY_BRANCH := "branch"
const KEY_TRAITS := "traits"
const KEY_UNLOCK := "unlock_knowledge"
const KEY_REQUIRES_RUNG := "requires_rung"
const KEY_EARNS := "earns_knowledge"
const KEY_LABEL := "label"
const KEY_LABEL_FALLBACK := "label_fallback"
const KEY_REACHED := "reached"
const KEY_PROGRESS := "progress"
const KEY_WORK_DONE := "work_done"
const KEY_WORK_COST := "work_cost"
const KEY_BUILD_MATERIAL := "build_material_cost"
const KEY_UPKEEP_DEMAND := "upkeep_demand"
const KEY_UPKEEP_MATERIAL := "upkeep_material_demand"
const KEY_FOOD_YIELD := "food_yield"
const KEY_FODDER := "fodder"
const KEY_MATERIAL_PAYOFF := "material_payoff"
const KEY_SITE_REFUSAL := "site_refusal"
## A crop-picker quote's own keys (one per plant, per rung above the plant root).
const KEY_CAN_CLIMB := "can_climb"
const KEY_FOOD_PAYOFF := "food_payoff"
const KEY_YIELD_RATIO := "yield_ratio"
const KEY_FODDER_PAYOFF := "fodder_payoff"

## The rung traits a reader asks by. Spelled once; the config owns what carries them.
const TRAIT_ENCLOSED := "enclosed"
## The catalog's own gate bools on a rung (always present on the wire): the rung below must be HELD
## (not merely reached by web), the step commits a CROP, and the step PLACES a source.
const KEY_REQUIRES_HELD_BELOW := "requires_held_rung_below"
const KEY_REQUIRES_CROP := "requires_crop"
const KEY_PLACES_SOURCE := "places_source"
## What the catalog says a rung's ground and its keepers can come to: the lessons that loosen its site
## rule, the size tiers an enclosed rung derives, and the lessons' gains on the herds it holds.
const KEY_SITE_RELAXATIONS := "site_relaxations"
const KEY_SIZE_TIERS := "size_tiers"
const KEY_KNOWLEDGE_GAINS := "knowledge_gains"
## The server's verdict on a rung this viewer may not place a source on YET, because a lesson would
## open the ground. `""` means the viewer may place one now.
const SITE_REFUSAL_NEEDS_LESSON := "needs_lesson"
const TRAIT_MANAGED := "managed"
const TRAIT_DOMESTICATED := "domesticated"

## The order a quote with no catalog row falls to: after every rung the catalog does place.
const ORDER_UNPLACED := 1000000

# ---- ENRICHMENT (the one join) -------------------------------------------------------------------

## Stamp every quote of every source in `sources` (patches or herds) -- and each patch's composition
## entries' quotes -- from the catalog, in place. Idempotent; a quote the catalog lacks keeps what it has.
static func enrich_sources(sources: Variant, catalog: Array, prefix: String = "") -> void:
	if not (sources is Array):
		return
	var by_key := _by_rung_key(catalog)
	for source_variant in (sources as Array):
		if not (source_variant is Dictionary):
			continue
		var source: Dictionary = source_variant
		_enrich_list(source.get(prefix + QUOTES_KEY, []), by_key)
		var composition: Variant = source.get(prefix + "composition", [])
		if composition is Array:
			for entry in (composition as Array):
				if entry is Dictionary:
					_enrich_list((entry as Dictionary).get(QUOTES_KEY, []), by_key)

static func _by_rung_key(catalog: Array) -> Dictionary:
	var by_key: Dictionary = {}
	for row in catalog:
		if row is Dictionary:
			by_key[String((row as Dictionary).get(KEY_RUNG, ""))] = row
	return by_key

static func _enrich_list(quotes_variant: Variant, by_key: Dictionary) -> void:
	if not (quotes_variant is Array):
		return
	for quote_variant in (quotes_variant as Array):
		if not (quote_variant is Dictionary):
			continue
		var quote: Dictionary = quote_variant
		var row: Variant = by_key.get(String(quote.get(KEY_RUNG, "")))
		if not (row is Dictionary):
			continue
		var rung: Dictionary = row
		quote[KEY_VERB] = String(rung.get(KEY_VERB, ""))
		quote[KEY_ORDER] = int(rung.get(KEY_ORDER, ORDER_UNPLACED))
		quote[KEY_BRANCH] = String(rung.get(KEY_BRANCH, ""))
		quote[KEY_TRAITS] = rung.get(KEY_TRAITS, [])
		quote[KEY_UNLOCK] = String(rung.get(KEY_UNLOCK, ""))
		quote[KEY_REQUIRES_RUNG] = String(rung.get(KEY_REQUIRES_RUNG, ""))
		quote[KEY_EARNS] = String(rung.get(KEY_EARNS, ""))
		quote[KEY_SITE_RELAXATIONS] = rung.get(KEY_SITE_RELAXATIONS, [])
		quote[KEY_SIZE_TIERS] = rung.get(KEY_SIZE_TIERS, [])
		quote[KEY_KNOWLEDGE_GAINS] = rung.get(KEY_KNOWLEDGE_GAINS, [])
		quote[KEY_REQUIRES_HELD_BELOW] = bool(rung.get(KEY_REQUIRES_HELD_BELOW, false))
		quote[KEY_REQUIRES_CROP] = bool(rung.get(KEY_REQUIRES_CROP, false))
		quote[KEY_PLACES_SOURCE] = bool(rung.get(KEY_PLACES_SOURCE, false))
		quote[KEY_LABEL] = String(rung.get(KEY_LABEL, ""))
		quote[KEY_LABEL_FALLBACK] = String(rung.get(KEY_LABEL_FALLBACK, ""))

# ---- READERS ---------------------------------------------------------------------------------------

## Does this rung want the rung below it HELD first? The catalog's `requires_held_rung_below`.
static func requires_held_below(quote: Dictionary) -> bool:
	return bool(quote.get(KEY_REQUIRES_HELD_BELOW, false))

## Does climbing to this rung commit a crop? The catalog's `requires_crop`.
static func requires_crop(quote: Dictionary) -> bool:
	return bool(quote.get(KEY_REQUIRES_CROP, false))

## Does climbing to this rung place a source (a sown field)? The catalog's `places_source`.
static func places_source(quote: Dictionary) -> bool:
	return bool(quote.get(KEY_PLACES_SOURCE, false))

## The source's quotes in climb order. `prefix` spells the key, so a `patch_`-prefixed `tile_info`
## cross-ref and a bare wire row read alike.
static func quotes(src: Dictionary, prefix: String = "") -> Array[Dictionary]:
	var rows: Array[Dictionary] = []
	var raw: Variant = src.get(prefix + QUOTES_KEY, [])
	if raw is Array:
		for quote in (raw as Array):
			if quote is Dictionary:
				rows.append(quote as Dictionary)
	rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return order_of(a) < order_of(b))
	return rows

static func order_of(quote: Dictionary) -> int:
	return int(quote.get(KEY_ORDER, ORDER_UNPLACED))

static func verb_of(quote: Dictionary) -> String:
	return String(quote.get(KEY_VERB, ""))

## The verbs of the rungs above the root that have one, in climb order -- the ladder a source offers.
static func verbs(src: Dictionary, prefix: String = "") -> Array[String]:
	var out: Array[String] = []
	for quote in quotes(src, prefix):
		var verb := verb_of(quote)
		if verb != "":
			out.append(verb)
	return out

## The quote whose rung carries `verb`, `{}` when the source has none.
static func quote_of_verb(src: Dictionary, prefix: String, verb: String) -> Dictionary:
	if verb == "":
		return {}
	for quote in quotes(src, prefix):
		if verb_of(quote) == verb:
			return quote
	return {}

static func quote_of_rung(src: Dictionary, prefix: String, rung_key: String) -> Dictionary:
	if rung_key == "":
		return {}
	for quote in quotes(src, prefix):
		if String(quote.get(KEY_RUNG, "")) == rung_key:
			return quote
	return {}

## One numeric field of the verb's quote, `default` where the source quotes no such rung.
static func number(src: Dictionary, prefix: String, verb: String, field: String,
		default: float = 0.0) -> float:
	var quote := quote_of_verb(src, prefix, verb)
	return float(quote.get(field, default)) if not quote.is_empty() else default

static func array_field(src: Dictionary, prefix: String, verb: String, field: String) -> Array:
	var quote := quote_of_verb(src, prefix, verb)
	var value: Variant = quote.get(field, [])
	return value as Array if value is Array else []

## Has the source climbed to (or past) the rung `verb` raises?
static func reached(src: Dictionary, prefix: String, verb: String) -> bool:
	return bool(quote_of_verb(src, prefix, verb).get(KEY_REACHED, false))

## The highest quote the source has reached, `{}` while it stands on the branch's root.
static func held(src: Dictionary, prefix: String = "") -> Dictionary:
	var best: Dictionary = {}
	for quote in quotes(src, prefix):
		if bool(quote.get(KEY_REACHED, false)):
			best = quote
	return best

## The verb of the rung the source stands on, `""` on the root (or an unplaced rung).
static func held_verb(src: Dictionary, prefix: String = "") -> String:
	return verb_of(held(src, prefix))

## The lowest-order quote not yet reached -- the next step -- `{}` at the top of the branch.
static func next(src: Dictionary, prefix: String = "") -> Dictionary:
	for quote in quotes(src, prefix):
		if not bool(quote.get(KEY_REACHED, false)):
			return quote
	return {}

static func has_trait(quote: Dictionary, trait_name: String) -> bool:
	var traits: Variant = quote.get(KEY_TRAITS, [])
	return traits is Array and (traits as Array).has(trait_name)

## The quote whose rung carries `trait`, `{}` when the branch has none.
static func quote_with_trait(src: Dictionary, prefix: String, trait_name: String) -> Dictionary:
	for quote in quotes(src, prefix):
		if has_trait(quote, trait_name):
			return quote
	return {}

## **IS THE SOURCE ENCLOSED** -- the rung carrying the `enclosed` trait is reached. That is what
## "penned" means, asked of the config's own word for a rung rather than of one rung's key.
static func is_enclosed(src: Dictionary, prefix: String = "") -> bool:
	return bool(quote_with_trait(src, prefix, TRAIT_ENCLOSED).get(KEY_REACHED, false))

## The verb of the enclosed rung, `""` where the branch has none.
static func enclosed_verb(src: Dictionary, prefix: String = "") -> String:
	return verb_of(quote_with_trait(src, prefix, TRAIT_ENCLOSED))

## **DOES THE SOURCE'S HUSBANDRY CEILING ADMIT THIS RUNG?** The ceiling is the id of the highest rung
## the species can reach (`"wild"` = the root, `"pastoral"`, `"pen"`, or any rung the config adds), so
## a quote is admitted when its order does not pass the ceiling rung's. A source with no ceiling (every
## plant, and a herd row that states none) admits everything; a ceiling that names the branch root --
## or any rung the catalog does not carry -- admits nothing, the rung being withheld OUTRIGHT rather than
## gated because no amount of work will ever raise it.
static func admits(src: Dictionary, prefix: String, quote: Dictionary) -> bool:
	if quote.is_empty():
		return false
	var ceiling := String(src.get(prefix + "husbandry_ceiling", "")).strip_edges().to_lower()
	if ceiling == "":
		return true
	var ceiling_quote := quote_of_rung(src, prefix, String(quote.get(KEY_BRANCH, "")) + ":" + ceiling)
	if ceiling_quote.is_empty():
		return false
	return order_of(quote) <= order_of(ceiling_quote)

## `"<branch>:"` of the source's ladder (read off its first quote), `""` when it quotes no rung -- the
## prefix a rung id is joined to for a lookup by `<branch>:<id>`.
static func branch_prefix(src: Dictionary, prefix: String = "") -> String:
	var rows := quotes(src, prefix)
	if rows.is_empty():
		return ""
	return String(rows[0].get(KEY_BRANCH, "")) + ":"

## **HOW FAR A HERD IS FROM DOMESTICATED**, 0..1: the progress of the `domesticated` rung that is not
## also the `enclosed` one (a pen is a further step, not the taming), full once any such rung is
## reached. What `domestication` was before the quotes.
static func domestication_progress(src: Dictionary, prefix: String = "") -> float:
	var best := 0.0
	for quote in quotes(src, prefix):
		if not has_trait(quote, TRAIT_DOMESTICATED) or has_trait(quote, TRAIT_ENCLOSED):
			continue
		if bool(quote.get(KEY_REACHED, false)):
			return 1.0
		best = maxf(best, float(quote.get(KEY_PROGRESS, 0.0)))
	return clampf(best, 0.0, 1.0)

## The best-known name of a quote's rung: `label`, else `label_fallback`, else the humanised rung id.
static func name_of(quote: Dictionary) -> String:
	var label := String(quote.get(KEY_LABEL, "")).strip_edges()
	if label != "":
		return label
	var fallback := String(quote.get(KEY_LABEL_FALLBACK, "")).strip_edges()
	if fallback != "":
		return fallback
	return HudRouteVocab.humanize_rung_key(String(quote.get(KEY_RUNG, "")))

# ---- THE SITE THE SERVER ADMITS, AND THE LESSON THAT WOULD WIDEN IT --------------------------------

## **MAY A SOURCE BE PLACED ON THIS GROUND NOW?** The server's verdict, never the client's: some quote
## whose rung `places_source` either stands already or carries an empty `site_refusal`. This is what
## opens a compose sheet on ground that is not a gathering site, and the ONLY thing that does -- the
## client holds no site rule of its own (`requires_gathering_site`, a capacity floor, fresh water).
static func placement_admitted(src: Dictionary, prefix: String = "") -> bool:
	for quote in quotes(src, prefix):
		if not places_source(quote):
			continue
		if bool(quote.get(KEY_REACHED, false)):
			return true
		if String(quote.get(KEY_SITE_REFUSAL, "")).strip_edges() == "":
			return true
	return false

## **THE VERB A CREW MUST NAME A CROP FOR, ON GROUND IT MAY NOT PLAIN-GATHER.** The placing rung the
## server admits for this ground (empty `site_refusal`) while it does not yet stand there: its verb
## (`sow`), `""` otherwise. Off the gathering sites the server accepts a forage crew only with a crop
## named, and that same command queues the build; so the sheet's crop chips stop being a choice and
## become the commit, and a bare gather is not offered.
static func offsite_placing_verb(src: Dictionary, prefix: String = "") -> String:
	for quote in quotes(src, prefix):
		if not places_source(quote):
			continue
		if bool(quote.get(KEY_REACHED, false)):
			return ""
		if String(quote.get(KEY_SITE_REFUSAL, "")).strip_edges() == "":
			return verb_of(quote)
	return ""

## The lesson id a placing rung's refusal is waiting on (`""` when none is): the knowledge of the last
## `site_relaxation`, which is the one the server's own refusal sentence names, set against a quote the
## server answered `needs_lesson`.
static func lesson_to_place(src: Dictionary, prefix: String = "") -> String:
	for quote in quotes(src, prefix):
		if not places_source(quote):
			continue
		if String(quote.get(KEY_SITE_REFUSAL, "")).strip_edges() != SITE_REFUSAL_NEEDS_LESSON:
			continue
		var relaxations: Variant = quote.get(KEY_SITE_RELAXATIONS, [])
		if relaxations is Array and not (relaxations as Array).is_empty():
			var last: Variant = (relaxations as Array).back()
			if last is Dictionary:
				return String((last as Dictionary).get("knowledge", ""))
	return ""

## The rung that earns `knowledge_id`, by its catalog label (`Farm` for `worked_land`), `""` when the
## source's quotes name none. The `work a farm` of a teaching line.
static func rung_teaching(src: Dictionary, prefix: String, knowledge_id: String) -> String:
	if knowledge_id == "":
		return ""
	for quote in quotes(src, prefix):
		if String(quote.get(KEY_EARNS, "")) == knowledge_id:
			return name_of(quote)
	return ""

# ---- SIZE TIERS (a pen fenced past a threshold is a Ranch) ------------------------------------------

## The size tiers the source's enclosed rung derives, in the catalog's order (`[]` for none).
static func size_tiers(src: Dictionary, prefix: String = "") -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	for quote in quotes(src, prefix):
		var rows: Variant = quote.get(KEY_SIZE_TIERS, [])
		if rows is Array:
			for row in (rows as Array):
				if row is Dictionary:
					out.append(row)
	return out

## The tier row whose `id` the herd's `size_tier` names, `{}` for none.
static func current_size_tier(src: Dictionary, prefix: String = "") -> Dictionary:
	var held := String(src.get(prefix + "size_tier", "")).strip_edges()
	if held == "":
		return {}
	for row in size_tiers(src, prefix):
		if String(row.get("id", "")) == held:
			return row
	return {}

## The lowest tier ABOVE the herd's current one that its pen has not reached: the one more rings
## would make it. `{}` where the herd already holds the top tier, or has no pen at all.
static func next_size_tier(src: Dictionary, prefix: String = "") -> Dictionary:
	var held_rings := -1
	var current := current_size_tier(src, prefix)
	if not current.is_empty():
		held_rings = int(current.get("min_rings", 0))
	var best := {}
	for row in size_tiers(src, prefix):
		var rings := int(row.get("min_rings", 0))
		if rings > held_rings and (best.is_empty() or rings < int(best.get("min_rings", 0))):
			best = row
	return best

# ---- CROP PICKER QUOTES (one set per plant in the basket) -------------------------------------------

## One composition entry's quote for `verb`, `{}` when the plant quotes no such rung.
static func entry_quote(entry: Dictionary, verb: String) -> Dictionary:
	return quote_of_verb(entry, "", verb)

static func entry_can_climb(entry: Dictionary, verb: String) -> bool:
	return bool(entry_quote(entry, verb).get(KEY_CAN_CLIMB, false))
