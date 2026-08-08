class_name KitRoster

## THE KIT ROSTER LAYER (`docs/plan_denial_raid.md`, `equipment.json` `kits`) — the read over
## `SubsistenceSection.kits`, the EFFECTIVE tier a given band gets under a given kit, the honesty test
## against the estimate tables' own kit ids, and the picker row all four compose sheets mount.
##
## WHY IT IS ITS OWN FILE. The control appears on FOUR sheets across TWO controllers (the Band panel's
## hunting-party and denial forms, the herd drawer's assign-hunters block, the land drawer's
## assign-foragers block). A kit describes the crew, so the row sits directly under the crew stepper
## and above every forecast on all four — and a row that has to read identically in four places is
## exactly the thing that must have one implementation. Same measurement that produced `SourceForecast`
## and `HudWidgets`.
##
## EVERYTHING HERE IS `static` AND STATELESS. The roster itself is snapshot data and lives on
## `HudBandLaborState` (the pure data model), threaded in as a parameter — never held here.
##
## **`none` IS AN ORDINARY ROSTER MEMBER, NOT A SENTINEL** (`snapshot.fbs` says so in as many words).
## It is a kit that grants nothing, so a party sent with it runs at the unequipped tiers throughout and
## spends no durability on any component — which is what makes a bare-handed comparison free to run.
## Nothing here special-cases its id: it is not styled as an error, not tagged as an override, and not
## divided off from the others. It renders last because the ROSTER authors it last, and this layer
## preserves wire order.
##
## DEPENDENCY DIRECTION: this file reads `SourceForecast` / `HudWidgets` / `HudStyle` / the vocab
## leaves, and NONE of them may read it back — a `const` cycle between two `class_name`d scripts fails
## to load the whole client.

# ---- the wire's own keys ------------------------------------------------------------------------
# The kit roster + the two job defaults, decoded once per world onto the snapshot dict
# (`native/src/dict/subsistence.rs` → `kits_to_array`).
const KIT_ID_KEY := "id"
const KIT_DISPLAY_NAME_KEY := "display_name"
const KIT_JOBS_KEY := "jobs"

## The three tier axes a kit publishes, and the ONE mapping from each to the consumable component
## behind it (`equipment.json` "One kit, one job"): spears raise ATTACK, a SLED raises the HUNT's
## carry, BASKETS raise the FORAGE web's. **The two carry tiers are not two readings of one number** —
## a band can be out of baskets with its sled untouched — and rendering one on the other's row is the
## defect the three-kit split corrected sim-side.
const KIT_ATTACK_KEY := "attack"
const KIT_HUNT_CARRY_KEY := "hunt_carry_per_worker_biomass"
const KIT_FORAGE_CARRY_KEY := "forage_carry_per_worker_biomass"

## The two axes the expanded roster added. **`pen_carry` is NOT a second reading of `hunt_carry`** —
## a sled drags a carcass in off the range and a pen stands at the camp, so a kit carrying only a
## sled collects a pen at the bare rate. `scout_vantage_range` is what a posted scout vantage can
## make out; how far out it is POSTED is not a kit axis at all (it is three `labor_config` dials).
const KIT_PEN_CARRY_KEY := "pen_carry_per_worker_biomass"
## **DECLARED FOR THE ROSTER'S AXIS VOCABULARY, AND IT HAS NO HINT-LINE CONSUMER.** `tier_hint` is
## written for the two COMPOSE sheets, which are hunt and forage only; Scout is a band-wide role with
## no compose surface, so there is nowhere for a vantage tier to render. The key and its `AXIS_ITEMS`
## row stay because the WIRE carries the axis — `unequipped_tier`/`equipped_tier` read it off the
## roster like any other, and `condition_of` answers for `wayfinding` the moment a Scout row gets a
## sheet. Do not invent that surface here.
const KIT_SCOUT_VANTAGE_KEY := "scout_vantage_range"

## **WHAT THE KIT DOES TO THE QUARRY'S RETREAT** — a multiplier on the species' own wariness, so the
## SPECIES decides what a noisy approach costs (`equipment.md`). Neutral at `1.0`; a trap ships `0`.
const KIT_DISPERSION_KEY := "dispersion"
const DISPERSION_NEUTRAL := 1.0

## **THE SIZE WINDOW A WEAPON'S `attack` IS BOUNDED TO** (`equipment.md` — "An effect can be bounded
## by the quarry's BODY MASS"). A snare holds a hare and not a deer, and above its ceiling the item
## grants **nothing**: the party falls back to the bare hand and the fight's own `max(0, attack −
## defense)` refuses the hunt, with no "you cannot trap that" branch anywhere.
##
## **`0` IS THE SENTINEL FOR UNBOUNDED ON BOTH ENDS, NOT A 0 kg BOUND** — it is these two fields'
## schema default and what every weapon but the passive device ships, which is why `equipment.md`
## names them the deliberate exception to the wire's "the neutral is `1`" rule.
const KIT_ATTACK_MIN_MASS_KEY := "attack_min_body_mass"
const KIT_ATTACK_MAX_MASS_KEY := "attack_max_body_mass"
const MASS_BOUND_UNBOUNDED := 0.0

## **THE TWO TERMS OF THE QUARRY THIS LAYER READS OFF THE SOURCE IT IS HANDED** — one animal's mass
## (against a weapon's size window above) and whether the herd is PENNED. Both are already on the wire
## (`native/src/dict/subsistence.rs`), which is what makes "can this kit change this source's outcome?"
## a question answerable here rather than a new field to ask the sim for. The fight's own `defense` /
## `durability` are NOT spelled here: `SourceForecast.hunt_gate_model_at` owns that pair, and asking it
## is how the offer test and the gate line cannot come to disagree about what a closed gate is.
const QUARRY_BODY_MASS_KEY := "body_mass"
const QUARRY_CORRALLED_KEY := "corralled"

## The BAND's remaining condition per ITEM — one row per item the server's config carries, as
## `{item_id, remaining}` on `equipment.json`'s 0-100 scale (`0` = dry). A dry item steps its role
## down to the unequipped tier and STAYS there — there is no replenishment path yet — and performance
## is FLAT until that cliff, so nothing here may scale a displayed number.
##
## It replaced three fixed keys (`hunting_kit_durability` and friends), because the item table is
## server config: a fixed field set could not carry the trapping kit's `traps`, nor the next item.
const BAND_ITEM_CONDITIONS_KEY := "kit_item_conditions"
const ITEM_CONDITION_ID_KEY := "item_id"
const ITEM_CONDITION_REMAINING_KEY := "remaining"

## **WHICH ITEM BACKS EACH DISPLAY AXIS.** The wire says what a kit *grants* per axis but not which
## item supplies it, so the client carries this mapping — as it always effectively did, when the axis
## was welded to a field name.
##
## **Known limitation, and it is bounded.** A second item supplying the same axis (a bow beside the
## spear) would read its condition off the wrong row here. That is a real gap the day a kit carries
## two weapons, and the fix is for the wire to state the axis→item mapping per kit rather than for
## this table to grow guesses. It cannot misfire on the shipped roster: no two items declare the same
## axis, which the server also enforces for the two carries at config-validate time.
## **The `attack` row answers for the HUNT's weapon.** The warrior kit declares the same stat off a
## different item (`clubs`), which is the one place this table's one-item-per-axis assumption is
## genuinely ambiguous — and it does not misfire, because a warrior kit and a hunting kit are never
## offered on the same sheet: the axis is looked up per JOB, and no job lists both.
const AXIS_ITEMS := {
	"attack": "spears",
	"hunt_carry_per_worker_biomass": "sled",
	"forage_carry_per_worker_biomass": "baskets",
	"pen_carry_per_worker_biomass": "husbandry_gear",
	"scout_vantage_range": "wayfinding",
}

## Condition at or below which a component is spent. It is the wire's own cliff, not a display
## threshold: the sim equips a component while its remaining condition is strictly positive.
const CONDITION_DRY := 0.0

## Which kit the two pre-launch estimate tables were quoted for — THIS HERD'S OWN default
## (`HERD_DEFAULT_KIT_KEY`), on every herd, always. **Neither table is repriced per kit** (they are
## ~95% of snapshot capture), and these keys exist so a client can SAY so rather than quoting a kitted
## raid's numbers to a bare-handed party. Two keys because they are two tables: if one is later
## repriced and the other is not, a single key would lie about whichever was left behind.
const HERD_TRIP_ESTIMATES_KIT_KEY := "hunt_trip_estimates_kit_id"
const HERD_DENIAL_ESTIMATES_KIT_KEY := "denial_estimates_kit_id"

## **THE KIT THIS QUARRY WANTS** (`equipment.md` → "Which kit a QUARRY wants is DERIVED") — the roster
## id the hunt sheet opens on for THIS herd, and the one `assign_labor … hunt <herd> <n>` resolves when
## the command names no kit. The sim scores every hunt kit's per-hunter-turn take against the species
## at the FRESH tier and publishes the winner where it clears the job default by a margin.
##
## **IT IS NOT A SECOND OPINION ABOUT THE JOB DEFAULT — it is a NARROWER answer, and it wins.** A
## Rabbit Warren's `wariness 0.75` loses a spear party three animals in four to the retreat while the
## trap's `dispersion 0` keeps all of them, so a sheet opening on the job's `big_game` defaulted the
## player onto a ~4× worse tool on exactly the quarry the roster has a right one for.
##
## `""` is "this herd has no answer" — a species the roster cannot resolve, and every forage row and
## every sheet with no source in hand — and falls back to the job default, exactly as the sim does.
const HERD_DEFAULT_KIT_KEY := "default_kit_id"

## The two jobs a kit may be sent on, spelled exactly as the wire's `jobs` entries and as the
## `assign_labor` roles — aliases of `SourceForecast`'s labor kinds so the sheet's verb, the command's
## role and the roster filter can never drift into three spellings of one word.
const JOB_HUNT := SourceForecast.LABOR_KIND_HUNT
const JOB_FORAGE := SourceForecast.LABOR_KIND_FORAGE

## **The two BAND-WIDE roles have a kit axis now.** They had none while nothing in the roster was
## gear for them — `LaborAssignment.kitId` published `""` on those rows — and the wayfinding and
## warrior kits are what changed that. Spelled the same as the wire's `jobs` entries and the
## `assign_labor` roles, like the pair above.
const JOB_SCOUT := "scout"
const JOB_WARRIOR := "warrior"

## **THE CARRY AXIS EACH JOB IS PRICED ON** — "one item, one job" (`equipment.md`): a SLED raises the
## hunt's haul, BASKETS raise the forage web's, and no kit raises both.
##
## **`priced_source` DERIVES the axis rather than taking it, and that is the whole point of the
## table.** A caller that can name the axis can name the WRONG one, and one did: the compose seam
## passed the key `effective_tiers` answered (`"forage_carry"`) to a roster lookup that spells it
## `forage_carry_per_worker_biomass`, so the reference resolved to `0`, the repricing short-circuited,
## and every kit on every sheet quoted identical numbers with only the hint line moving. Reported from
## play. A job is what a call site actually knows; the axis is this layer's business.
##
## **IT IS THE JOB'S ANSWER, NOT THE LAST WORD** — a penned herd overrides it. `carry_axis_for` is the
## whole of that rule and the only thing anything should ask; nothing outside it reads this table.
const JOB_CARRY_AXES := {
	JOB_HUNT: KIT_HUNT_CARRY_KEY,
	JOB_FORAGE: KIT_FORAGE_CARRY_KEY,
}

## The `OptionButton` the kit row mounts, as meta — the stable handle for the preview harnesses. A
## node-type search finds the compose sheets' `Band:` picker too (and, before the control became an
## `OptionButton`, the quarry chooser and the zone `⋯` menus), so it needs a handle of its own exactly
## as `QUARRY_CHOICES_META` does.
const KIT_PICKER_META := "kit_picker"

## The hint label beneath it, as meta: the claim a harness makes about the effective tier is about
## THAT line, and it must not be able to match the picker's face or a neighbouring hint.
const KIT_HINT_META := "kit_hint"

## No kit named — what a payload carries before a roster is known, and what the command builders read
## as "the player named none, so omit the token".
const NO_KIT_ID := ""

# ---- reading the roster -------------------------------------------------------------------------

## The kits a sheet composing `job` may offer, in WIRE ROSTER ORDER.
##
## **THE ORDER IS THE ROSTER'S, NOT A SORT OF OURS.** `equipment.json` authors `none` last and the
## capture preserves that, so the null choice already lands at the bottom of the menu without this
## layer knowing which entry is null. A client-side "put `none` last" rule would be exactly the
## special-casing `snapshot.fbs` forbids, and it would silently disagree with the roster the day a
## designer reorders it.
##
## A kit named for a job outside its own `jobs` list is a COMMAND FAILURE server-side, never a silent
## fall back to the default — which is why the filter is here rather than being left to the sim.
static func kits_for_job(kits: Array, job: String) -> Array:
	var matching: Array = []
	for entry_variant in kits:
		if not (entry_variant is Dictionary):
			continue
		var kit: Dictionary = entry_variant
		var jobs_variant: Variant = kit.get(KIT_JOBS_KEY, [])
		if jobs_variant is Array and (jobs_variant as Array).has(job):
			matching.append(kit)
	return matching

## The roster entry with this id, `{}` when the roster does not carry it (an id held over from a
## previous world, or a sheet composed before the first snapshot landed).
static func kit_by_id(kits: Array, kit_id: String) -> Dictionary:
	for entry_variant in kits:
		if entry_variant is Dictionary and String((entry_variant as Dictionary).get(
				KIT_ID_KEY, "")) == kit_id:
			return entry_variant
	return {}

## This kit's player-facing name, falling back to its id — a roster entry with no display name is a
## config gap, and a blank picker face states nothing at all.
static func kit_display_name(kit: Dictionary) -> String:
	var display := String(kit.get(KIT_DISPLAY_NAME_KEY, "")).strip_edges()
	return display if display != "" else String(kit.get(KIT_ID_KEY, ""))

## …and the same for a bare id, resolved through the roster. Used by the honesty line, which names a
## kit the sheet is NOT currently rendering.
static func display_name_for_id(kits: Array, kit_id: String) -> String:
	var kit := kit_by_id(kits, kit_id)
	return kit_display_name(kit) if not kit.is_empty() else kit_id

## **THE DEFAULT THAT ACTUALLY APPLIES HERE — the SOURCE's own, falling back to the JOB's.** The one
## home of that precedence, so the sheet's opening selection, the picker's `(default)` mark and the
## estimate tables' honesty test cannot each answer it differently.
##
## **THE SOURCE OVERRIDES THE JOB, and it is a narrower answer rather than a competing one**
## (`HERD_DEFAULT_KIT_KEY`). Only a HUNT row has a source that publishes one: the forage web's patches
## carry no such field, and passing them through here is what keeps the two webs on one call.
##
## **THE SOURCE ARRIVES AS A PARAMETER, never reached for.** The two sheets that price a herd already
## hold it — it is the same dict the offer test reads `corralled` off — so this layer stays stateless.
static func default_kit_for(job: String, source: Dictionary, job_default_id: String) -> String:
	if job != JOB_HUNT:
		return job_default_id
	var stated := String(source.get(HERD_DEFAULT_KIT_KEY, "")).strip_edges()
	return stated if stated != "" else job_default_id

## **THE SELECTION A SHEET OPENS ON.** The player's own composed choice when it is still a kit this
## verb may be sent on **and one this quarry can be worked with**, otherwise this QUARRY'S default
## (`default_kit_for` — the herd's own, else the job's), otherwise the first kit the job lists. The
## fall-through is what lets one composed id survive a world rebuild, a roster edit, and a sheet
## switching between the hunt and denial missions (which share the `hunt` job) without ever naming a
## kit the command would refuse.
##
## **THE COMPOSED CHOICE STILL OUTRANKS THE DEFAULT, and that is why the composed id is dropped on a
## SOURCE CHANGE rather than being overridden here** (`ComposeState.reset_hunt_kit` /
## `set_party_quarry`). A player who picked `none` on this animal to compare bare-handed must keep it
## across the re-render their own click causes; what they must not keep is a choice made about a
## DIFFERENT animal, since the default is now a fact about the quarry.
##
## **THE QUARRY IS OPTIONAL AND ABSENT MEANS "NO APPLICABILITY QUESTION"** — the forage sheets pass
## none and resolve exactly as they did before the offer test existed. Where one IS passed, a
## WITHHELD kit is skipped at every step, which is what stops a trapping selection made on a warren
## from surviving into a Red Deer's sheet as a greyed row the picker is nonetheless opened on.
##
## **`kit_offer` is asked at the FRESH tier, so this list never reshuffles as gear wears** — see there.
static func resolve_selection(kits: Array, job: String, default_id: String,
		composed_id: String, quarry: Dictionary = {}, prefix: String = "") -> String:
	var offered := kits_for_job(kits, job)
	if offered.is_empty():
		return NO_KIT_ID
	var selectable: Array = []
	for kit_variant in offered:
		if kit_is_offered(kits, kit_variant, job, quarry, prefix):
			selectable.append(kit_variant)
	# A roster whose every hunt kit is withheld cannot happen while it carries a null kit (one is
	# always offered), but a config is free to drop that entry — and a picker with no entries at all
	# would be a worse answer than an unfiltered one.
	if selectable.is_empty():
		selectable = offered
	for kit_variant in selectable:
		if String((kit_variant as Dictionary).get(KIT_ID_KEY, "")) == composed_id:
			return composed_id
	var effective_default := default_kit_for(job, quarry, default_id)
	for kit_variant in selectable:
		if String((kit_variant as Dictionary).get(KIT_ID_KEY, "")) == effective_default:
			return effective_default
	return String((selectable[0] as Dictionary).get(KIT_ID_KEY, ""))

# ---- the EFFECTIVE tier -------------------------------------------------------------------------

## **THE UNEQUIPPED TIER ON ONE AXIS, READ OFF THE ROSTER ITSELF.** Every kit publishes all three
## tiers and publishes the UNEQUIPPED one on each axis it does not use, so the minimum across the
## roster on an axis IS that axis's bare-handed tier — no second copy of the TOE table, and no
## client-side knowledge of which component each kit masks in.
##
## `INF` when the roster is empty, which the one caller reads as "say nothing": with no roster there
## is no bare-handed tier to step down to, and inventing one would quote a number the sim never sent.
static func unequipped_tier(kits: Array, axis_key: String) -> float:
	var lowest := INF
	for entry_variant in kits:
		if not (entry_variant is Dictionary):
			continue
		lowest = minf(lowest, float((entry_variant as Dictionary).get(axis_key, 0.0)))
	return lowest

## **THE EQUIPPED REFERENCE TIER ON ONE AXIS — the MAXIMUM across the roster**, and the exact twin of
## `unequipped_tier` above, read off the same roster for the same reason.
##
## **IT IS THE RATE EVERY SOURCE ROW IS PUBLISHED AT**, which is what makes it the denominator of the
## repricing rather than merely a number the roster happens to contain. A herd's `perWorkerBiomass` is
## `labor_config.hunt.per_worker_biomass_capacity`, a patch's is
## `labor_config.forage.per_worker_biomass_capacity × seasonalWeight` — and a kit that USES the
## component publishes exactly that `labor_config` capacity on its axis (`snapshot/capture.rs` →
## `kit_roster_states`, which resolves the tier through the take path's own seam). Every kit that does
## NOT use it publishes the unequipped tier, which is lower. So the roster's max IS the capacity, and
## the ratio `effective / max` is the fraction of the published rate this crew actually moves.
##
## **THE SEASONAL WEIGHT IS WHY THIS IS NOT THE SOURCE'S OWN `perWorkerBiomass`.** A `KitOption`'s
## `forage_carry_per_worker_biomass` is the throughput *before* the tile's weight (`equipment.md` — the
## wire says so in as many words), while the patch publishes the weight folded in. Dividing by the
## patch's number therefore divides the season back out and multiplies a season-free tier by it — the
## crew's rate comes out season-BLIND, which is wrong in the direction that looks right, worldgen
## pinning every weight at `1.0` today. Dividing by the roster's own tier leaves the season on the
## published rate where the sim put it.
##
## `0.0` when the roster is empty or states nothing on this axis, which `repriced_source` reads as
## "no reference, so no repricing" — the same fail-quiet the zero published rate gets.
static func equipped_tier(kits: Array, axis_key: String) -> float:
	var highest := 0.0
	for entry_variant in kits:
		if not (entry_variant is Dictionary):
			continue
		highest = maxf(highest, float((entry_variant as Dictionary).get(axis_key, 0.0)))
	return highest

## The wire keys this repricing substitutes — **taken from `SourceForecast`'s own constants, never
## typed out here.**
##
## Spelling them by hand is how the first version shipped broken: it scaled `"per_worker"`, and the
## key food actually reads is `per_worker_yield`. Trade repriced, food did not, and the sheet quoted
## a five-fold trade change beside an unmoved food line. A literal cannot be wrong in a way the
## compiler or a rename would catch; a constant reference can.
##
## **`per_worker_biomass` carries more than its own account.** On the forage web `forecast_inputs`
## DERIVES trade and fodder from it (`carry × <account>_per_biomass`), so scaling it reprices those
## two for free; on the hunt web trade is published in its own right and needs its own entry.
const SOURCE_PER_WORKER_KEYS := [
	SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY,
	SourceForecast.FORECAST_PER_WORKER_KEY,
	SourceForecast.FORECAST_PER_WORKER_TRADE_KEY,
]
const SOURCE_PER_WORKER_BIOMASS := SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY
const SOURCE_ENGAGE_RATE := SourceForecast.FORECAST_ENGAGE_RATE_KEY
## `1 − wariness` — the fraction of what a party reaches that stays to be fought. Absent on a source
## with no retreat stage (a pen, the whole plant web), which reads as "nothing breaks off".
const SOURCE_STAY_FRACTION := SourceForecast.FORECAST_STAY_FRACTION_KEY
const STAY_FRACTION_NONE_BREAKS_OFF := SourceForecast.STAY_FRACTION_NONE_BREAKS_OFF

## **THE SOURCE, REPRICED FOR THE KIT THE CREW IS BEING SENT WITH** — a copy of the wire's own terms
## with two substitutions, handed to the ordinary forecast so **every** consumer downstream (the take,
## the waste, the crew targets, the chart) picks the kit up without knowing it exists.
##
## **It is pure arithmetic on published terms, and deliberately knows nothing about hunting or
## gathering.** A source that publishes no engagement and no retreat — a patch, a pen — simply has no
## key for the second substitution, so the same call does the right thing on both webs.
##
## 1. **Per-worker throughput scales by `carry / reference`**, where `reference` is the roster's own
##    EQUIPPED tier on that axis (`equipped_tier`) — the rate every source row is published at. All
##    four currencies scale together; they are one throughput expressed four ways.
## 2. **`stay_fraction` becomes the kit's EFFECTIVE retreat**, `1 − (1 − stay) × dispersion`, which is
##    `snapshot.fbs`'s own formula for what a kit does to that field. It is the retreat's ONE home on
##    the client, so the take arms downstream read a stay fraction that already knows the kit.
##
## **THE RETREAT DOES NOT TOUCH `engage_rate`, AND THAT IS THE CORRECTION THIS PAIR EXISTS FOR.**
## Folding it into the reach reprices the take and the CREW COUNT together, and the sim does not treat
## them together: `fauna::hunt_engage_workers` sizes a crew on the RAW reach — the hands that can get
## to the herd — while `HuntParty::stayers` cuts only what those hands bring down. The fold made the
## sheet's stepper cap disagree with the sim's own `workersNeeded`, which `ui_preview`'s "the compose
## stepper caps at the crew the SIM asks for" caught at once. Substituting the retreat on its own field
## keeps the two arms separable, which is what lets `SourceForecast` apply it to the take alone.
##
## **THE REFERENCE IS THE ROSTER'S TIER, NOT THE SOURCE'S OWN `per_worker_biomass`** — see
## `equipped_tier` for why (the seasonal weight, and a harness fixture whose recovered throughput is
## its own arbitrary number rather than a claim about anyone's carry).
##
## **CALL IT ONCE PER SOURCE.** With a reference the substitution does not overwrite, this is no longer
## idempotent: a second pass multiplies by the ratio again. `DrawerComposeController._kit_priced_source`
## is the one seam, and each forecast/rates producer prices at its own top rather than passing a priced
## dict into another producer that prices too.
##
## **This is where the trapping kit's whole advantage lands.** A spear party on a `wariness 0.75`
## warren keeps one animal in four; a device that is not there to be seen (`dispersion 0`) keeps all
## of them, and that is the difference the sheet has to show.
##
## The non-linear halves stay the sim's answer — the whole-animal quantiser and the fight — exactly as
## `yield-forecast.md`'s "THE BOUNDARY" requires; nothing here re-derives a take.
static func repriced_source(src: Dictionary, prefix: String, carry: float, reference: float,
		dispersion: float) -> Dictionary:
	var out := src.duplicate()
	# **A zero reference or a zero carry is a real reading** (an empty roster; a dead-season patch moves
	# no biomass), so there is no ratio to take and no repricing to do — never a division that would
	# land an INF in three keys.
	if reference > 0.0 and carry > 0.0 and not is_equal_approx(carry, reference):
		var ratio := carry / reference
		for key in SOURCE_PER_WORKER_KEYS:
			var full: String = prefix + String(key)
			if out.has(full):
				out[full] = float(out[full]) * ratio
	# **THE RETREAT, ON ITS OWN FIELD.** Absent = no retreat stage (a patch, a pen), which skips without
	# a special branch: there is no key to substitute and the take arms read the wire's own `1`.
	var stay_key := prefix + SOURCE_STAY_FRACTION
	if out.has(stay_key):
		var stay := clampf(float(out[stay_key]), 0.0, 1.0)
		out[stay_key] = clampf(1.0 - (1.0 - stay) * maxf(dispersion, 0.0), 0.0, 1.0)
	return out

## **THE CARRY AXIS THIS SOURCE IS COLLECTED ON — the job's answer (`JOB_CARRY_AXES`), overridden by
## a PENNED herd.** `""` for a job with no carry axis at all, which `priced_source` reads as "nothing
## to price against".
##
## **THE AXIS IS A PROPERTY OF THE SOURCE, AND A JOB-KEYED TABLE ALONE COULD NOT SAY SO.** A corralled
## herd is worked from a Hunt row, so pricing it by job read the SLED's tier — while the sim collects
## a pen on `EquipmentStat::PenCarry`, which only the husbandry kit supplies. That UNDER-stated the
## very kit the pen exists for and OVER-stated every kit carrying a sled and no handling gear, and on
## the shipped roster (where husbandry and stalking both carry a sled) the two errors cancelled into
## *every hunt kit quotes a pen the same number* — a perfectly plausible sheet, which is why only a
## driven assertion can hold it. A sled drags a carcass in off the range; a pen stands at the camp.
##
## **THE CORRAL STATE COMES OFF `src`, AND THAT IS NOT A REACH FOR STATE** — on the hunt job `src` IS
## the herd, handed in as a parameter exactly like the body mass the weapon's size window is tested
## against, and read through the same `QUARRY_CORRALLED_KEY` the offer test and the fight's gate use.
##
## The forage web has one carry and no override: a patch is a patch.
static func carry_axis_for(job: String, src: Dictionary) -> String:
	if job == JOB_HUNT and bool(src.get(QUARRY_CORRALLED_KEY, false)):
		return KIT_PEN_CARRY_KEY
	return String(JOB_CARRY_AXES.get(job, ""))

## **THE COMPOSE SHEETS' ONE PRICING SEAM — resolve the kit, then reprice the source at it.**
##
## `repriced_source` above is pure arithmetic on two tiers; this is the step that decides WHICH tiers,
## and it is the half that has twice been where the feature died. It lives here rather than on a
## controller because BOTH controllers need it: `DrawerComposeController` prices the herd/land drawer's
## compose sheets and `BandPanelController` prices the dock's raid chart, and a second copy of a
## resolve-then-reprice is exactly how one entry point comes to quote a kit the other does not.
##
## **STATELESS, so the roster and the job default arrive as PARAMETERS** — they are snapshot data and
## live on `HudBandLaborState`, which this layer must never reach for.
##
## **The AXIS is derived from the job AND the source** (`carry_axis_for`), so a caller cannot hand it
## a key no roster entry carries — see that function and `JOB_CARRY_AXES` for the two bugs this closes.
##
## **THE REFERENCE TIER IS READ ON THE SAME AXIS AS THE CREW'S**, in one expression below, because
## they are the numerator and the denominator of one ratio: switching the axis without switching the
## reference resolves the denominator to `0` off a roster that states nothing there, the repricing
## short-circuits, and every kit quotes identical numbers. That is not hypothetical — it is exactly
## how the forage spelling bug shipped.
##
## **THE FIGHT'S GATE IS PRICED HERE, BEFORE ANY REPRICING** (`hunt_gate_closes`). A kit whose attack
## cannot clear this quarry's defence — because the band's weapon is spent, or because the quarry is
## outside the weapon's size window — brings home **exactly nothing**, and a ratio applied to a take
## that never happens is a lie in the reassuring direction. It is priced here rather than left to the
## picker's greying because the greying is not everywhere: the Band panel's raid chart calls this
## function with no picker in sight, so filtering the LIST cannot make the NUMBER honest.
##
## **THE QUARRY'S TERMS COME OFF `src`, AND THAT IS NOT A REACH FOR STATE** — on the hunt job `src`
## IS the herd being priced. What this layer must never do is consult `HudBandLaborState`, and it
## does not: the roster, the band and the source all arrive as parameters.
##
## Answers `src` UNCHANGED where there is nothing to price against: a job with no carry axis, or a
## roster that cannot resolve the selection at all (a world rebuilt under the open sheet). Never a
## guess, and never a partial substitution.
static func priced_source(src: Dictionary, prefix: String, kits: Array, job: String,
		default_kit_id: String, composed_kit_id: String, band: Dictionary) -> Dictionary:
	var carry_key := carry_axis_for(job, src)
	if carry_key.is_empty():
		return src
	var kit := kit_by_id(kits, resolve_selection(kits, job, default_kit_id, composed_kit_id,
		src, prefix))
	if kit.is_empty():
		return src
	if job == JOB_HUNT and hunt_gate_closes(kits, kit, band, src, prefix):
		return gate_closed_source(src, prefix)
	var tiers := effective_tiers(kits, kit, band)
	return repriced_source(src, prefix, float(tiers.get(carry_key, 0.0)),
		equipped_tier(kits, carry_key),
		float(kit.get(KIT_DISPERSION_KEY, DISPERSION_NEUTRAL)))

## **DOES THIS KIT'S WEAPON REACH AN ANIMAL OF THIS MASS AT ALL?** — the size window, and the ONE
## home of it, so the fresh-tier offer test and the wear-resolved gate cannot read the bound two ways.
## An absent or `0` bound is UNBOUNDED (`MASS_BOUND_UNBOUNDED`), which is every weapon but the passive
## device, so a roster that states neither field behaves exactly as it did before the bound existed.
static func attack_reaches(kit: Dictionary, body_mass: float) -> bool:
	var low := float(kit.get(KIT_ATTACK_MIN_MASS_KEY, MASS_BOUND_UNBOUNDED))
	var high := float(kit.get(KIT_ATTACK_MAX_MASS_KEY, MASS_BOUND_UNBOUNDED))
	if low > MASS_BOUND_UNBOUNDED and body_mass < low:
		return false
	return not (high > MASS_BOUND_UNBOUNDED and body_mass > high)

## **THE KIT'S FRESH ATTACK AGAINST THIS QUARRY** — the kit's own number inside its size window, and
## the roster's unequipped attack outside it.
##
## A snare holds a hare and not a deer, so asking a kit for its attack without naming the animal gets
## the kit's BEST case — which would tell a player the trapping kit can take a Red Deer.
static func attack_against(kit: Dictionary, body_mass: float, unequipped_attack: float) -> float:
	if not attack_reaches(kit, body_mass):
		return unequipped_attack
	return float(kit.get(KIT_ATTACK_KEY, unequipped_attack))

## **THIS BAND'S ATTACK, UNDER THIS KIT, AGAINST THIS ANIMAL** — the composition of the two floors,
## and the number every hunt-arm FORECAST must be gated on.
##
## The two reach the same bare-handed tier by different routes and both are real: wear steps a spent
## weapon down (`effective_tiers`), and the mass bound says the weapon was never in play against this
## animal in the first place. Outside the window there is nothing left for condition to decide, so the
## window is tested first and the band's own condition only decides the in-window case.
##
## **This is the quarry-aware twin of `effective_tiers`'s `attack`, and every take path must use it.**
## Reading the bare tier is `equipment.md`'s `hunter_profile_unbounded` — *"the best this kit can do
## against something"* — which is honest only on a surface with no target in hand. A compose sheet has
## one, and quoting the unbounded reading there is what let a trapping party be sold a Red Deer.
static func effective_attack_against(kits: Array, kit: Dictionary, band: Dictionary,
		body_mass: float) -> float:
	var worn := float(effective_tiers(kits, kit, band).get(KIT_ATTACK_KEY, 0.0))
	if attack_reaches(kit, body_mass):
		return worn
	var bare := unequipped_tier(kits, KIT_ATTACK_KEY)
	# An unreadable roster states no bare-handed tier, so there is nothing to step down TO and the
	# in-window reading stands — the same fail-quiet `_tier_after_wear` takes.
	return worn if is_inf(bare) else bare

# ---- IS THIS KIT ANY USE ON THIS SOURCE? --------------------------------------------------------

## The offer verdict's two keys — `offered`, and the REASON a withheld kit states on its own row.
## A greyed entry that does not say why teaches nothing, and "a snare cannot hold a Red Deer" is a
## fact about the world worth learning once.
const OFFER_OFFERED_KEY := "offered"
const OFFER_REASON_KEY := "reason"

## The build dip the offer test asks the source's reach at — the NEUTRAL multiplier, i.e. no dip. A
## dip is a property of what the player is currently COMPOSING (hands gentling a herd are hands not
## stalking it), and which kits a sheet offers must be a property of the (kit × quarry) pair alone,
## or ticking Tame would silently reshuffle the picker. The dip cannot open or close an engagement
## STAGE anyway; it only narrows a reach the species already has.
const OFFER_NO_BUILD_DIP := 1.0

## **DOES THIS KIT SUPPLY ANY AXIS AT ALL?** — the derived reading of "the null kit", and the reason
## nothing here spells the id `none`. A kit that beats the roster's bare-handed tier on no axis grants
## nothing anywhere, so there is no source it can be *inapplicable* to; it is the free bare-handed
## comparison the whole wear model exists to protect, and it is never withheld. A future `fishing` kit
## with an empty `uses` gets the same treatment for the same reason.
static func kit_supplies_any(kits: Array, kit: Dictionary) -> bool:
	for axis_key in AXIS_ITEMS:
		if kit_uses(kits, kit, String(axis_key)):
			return true
	return false

## **THE OFFER TEST — `{offered, reason}` for ONE kit against ONE source.**
##
## > Offer a kit as selectable only if something it declares can change this source's outcome.
##
## It introduces no config: every term is something the kit already declares against something the
## source already publishes. Two rules, and both are about APPLICABILITY, never about wear:
##
## 1. **A weapon that cannot reach the quarry.** The fight's own gate, asked at the kit's FRESH attack
##    resolved against this animal's mass. A trap rated to hold a hare grants nothing against a Red
##    Deer, so the party is bare-handed, `max(0, 1 − 1)` is zero, and the sim refuses the hunt — the
##    sheet used to price that party a real take, and it brought home exactly nothing.
## 2. **A kit whose contribution is an axis this source cannot read.** `pen_carry` is read on a
##    CORRALLED herd and nowhere else, so a kit supplying it adds nothing to a wild hunt.
##
## **THE PEN RULE IS ASKED FIRST, so a kit reads the same reason on every quarry.** The husbandry kit
## fails both tests on a Red Deer (it carries no weapon either), and *"what it adds is only used on a
## penned herd"* is the fact about the kit; *"it cannot bring one down"* is a fact about the deer that
## would then not be stated on a rabbit, where the same kit is withheld for the same reason.
##
## **A PEN IS NOT FOUGHT, so rule 1 does not run on one.** A penned animal is slaughtered rather than
## stalked and publishes no engagement stage — the same predicate the gate LINE is mounted behind —
## and without that guard a corralled Red Deer would withhold every kit but the spear line.
##
## **RESOLVED AT THE FRESH TIER, AND THAT IS THE LOAD-BEARING CONSTRAINT.** Which kits are offered and
## which is default are properties of (kit × quarry); the band's wear moves the QUOTED number and the
## hint line and nothing else. A band whose spears are dry still sees the stalking kit listed,
## selectable and default against a Red Deer, quoting zero, with the hint saying the spears are gone —
## because a picker that reshuffled between turns would leave the player unable to tell a kit that
## *cannot* work on this animal from one that has merely worn out.
static func kit_offer(kits: Array, kit: Dictionary, job: String, quarry: Dictionary,
		prefix: String) -> Dictionary:
	# No quarry in hand and no hunt to have: the forage sheets, and a sheet composed before the wire
	# named a source. Nothing to be inapplicable to.
	if job != JOB_HUNT or kit.is_empty() or quarry.is_empty():
		return _kit_offered()
	if not kit_supplies_any(kits, kit):
		return _kit_offered()
	var penned := bool(quarry.get(QUARRY_CORRALLED_KEY, false))
	if kit_uses(kits, kit, KIT_PEN_CARRY_KEY) and not penned:
		return _kit_withheld(HudComposeVocab.KIT_WITHHELD_REASON_PEN_ONLY)
	if penned:
		return _kit_offered()
	if not SourceForecast.has_engagement_stage(
			float(quarry.get(prefix + SOURCE_ENGAGE_RATE, SourceForecast.NO_ENGAGEMENT_STAGE)),
			OFFER_NO_BUILD_DIP):
		return _kit_offered()
	var bare := unequipped_tier(kits, KIT_ATTACK_KEY)
	if is_inf(bare):
		return _kit_offered()
	var quarry_name := SourceForecast.herd_display_name(quarry)
	var gate := SourceForecast.hunt_gate_model_at(
		attack_against(kit, float(quarry.get(QUARRY_BODY_MASS_KEY, 0.0)), bare),
		quarry, quarry_name)
	# **`stated` FIRST.** A species the roster cannot resolve publishes `durability 0`, and withholding
	# every kit on a gap in the data would refuse a hunt the sim would have allowed.
	if bool(gate["stated"]) and bool(gate["blocked"]):
		return _kit_withheld(HudComposeVocab.KIT_WITHHELD_REASON_CANNOT_HURT % quarry_name)
	return _kit_offered()

## The boolean half of the verdict, for callers with nothing to say about the reason.
static func kit_is_offered(kits: Array, kit: Dictionary, job: String, quarry: Dictionary,
		prefix: String) -> bool:
	return bool(kit_offer(kits, kit, job, quarry, prefix)[OFFER_OFFERED_KEY])

## Freshly built each call rather than returned from a `const` Dictionary — a `const` container is not
## deeply read-only in GDScript, so one caller mutating the shared verdict would poison every later
## one.
static func _kit_offered() -> Dictionary:
	return {OFFER_OFFERED_KEY: true, OFFER_REASON_KEY: ""}

static func _kit_withheld(reason: String) -> Dictionary:
	return {OFFER_OFFERED_KEY: false, OFFER_REASON_KEY: reason}

## **DOES THE FIGHT REFUSE THIS PARTY OUTRIGHT?** — the same two rules as rule 1 above, asked at the
## band's own WORN tier rather than at the fresh one, because this one decides a NUMBER rather than a
## choice. A dry-speared band against a Red Deer kills nothing, and the sheet must say zero.
##
## The pen and the plant web are excluded by the same engagement-stage guard `kit_offer` takes: there
## is no fight to lose at a pen, and a patch states no `durability` for the gate to be `stated` about.
static func hunt_gate_closes(kits: Array, kit: Dictionary, band: Dictionary, quarry: Dictionary,
		prefix: String) -> bool:
	if kit.is_empty() or quarry.is_empty() or bool(quarry.get(QUARRY_CORRALLED_KEY, false)):
		return false
	if not SourceForecast.has_engagement_stage(
			float(quarry.get(prefix + SOURCE_ENGAGE_RATE, SourceForecast.NO_ENGAGEMENT_STAGE)),
			OFFER_NO_BUILD_DIP):
		return false
	var gate := SourceForecast.hunt_gate_model_at(effective_attack_against(kits, kit, band,
		float(quarry.get(QUARRY_BODY_MASS_KEY, 0.0))), quarry, "")
	return bool(gate["stated"]) and bool(gate["blocked"])

## What a party that cannot hurt the quarry moves per worker. **It is not a repricing** — there is no
## ratio that expresses "the fight is refused" — so every per-worker currency is substituted flat and
## the ordinary forecast downstream quotes a zero take, a zero waste and a zero crew target without
## knowing why.
const GATE_CLOSED_PER_WORKER := 0.0

## The source with its throughput zeroed, for a kit whose gate `hunt_gate_closes` says is shut. The
## RETREAT is deliberately not substituted beside it: a stay fraction describes what a party keeps of
## what it brings down, and this one brings nothing down.
static func gate_closed_source(src: Dictionary, prefix: String) -> Dictionary:
	var out := src.duplicate()
	for key in SOURCE_PER_WORKER_KEYS:
		var full: String = prefix + String(key)
		if out.has(full):
			out[full] = GATE_CLOSED_PER_WORKER
	return out

## **WHAT THIS BAND ACTUALLY GETS UNDER THIS KIT** — `{attack, hunt_carry, forage_carry, stated}`.
##
## `KitOption`'s numbers are for a FRESH kit; the band's real condition is on its own cohort. A
## component the kit uses but the band has run dry delivers the UNEQUIPPED tier, so quoting the fresh
## number to a band with spent spears is a lie of exactly the class this arc keeps correcting.
##
## **NO "does this kit use that component?" TEST IS NEEDED, AND THAT IS THE POINT OF THE FORM.** A kit
## that does not use a component already publishes the unequipped tier on that axis, so stepping down
## to the unequipped tier is a no-op there and the whole rule collapses to one line per axis:
##
##     effective(axis) = kit(axis) when the band still has condition in the component, else unequipped(axis)
##
## `stated` is false when the band says nothing about its condition at all (the key is absent, not
## zero — `0` is a real reading meaning DRY). Then the fresh tiers stand and the hint prints no
## condition clause, the same "absent terms render no line" convention `hunt_gate_model` takes.
##
## **IT IS KEYED BY THE ROSTER'S OWN AXIS CONSTANTS, so a tier and the roster entry it came from are
## reachable by ONE name.** It used to answer short keys (`"hunt_carry"` / `"forage_carry"`) while the
## roster spelled them `hunt_carry_per_worker_biomass` / `forage_carry_per_worker_biomass`, and that
## split shipped a silent bug the moment a caller needed BOTH: `_kit_priced_source` read this dict with
## the short key and `equipped_tier` with the same string, which no roster entry carries — so the
## reference came back `0`, the repricing short-circuited, and every kit on every compose sheet quoted
## identical numbers. Reported from play. `attack` was always the wire's own spelling and is unchanged;
## one name per axis is what makes the two lookups impossible to spell apart.
## **THE PEN CARRY IS WEAR-RESOLVED LIKE THE OTHER THREE**, and for the same reason: a band whose
## `husbandry_gear` is dry collects its pen at the bare-handed tier, so quoting the roster's fresh
## `40.0` to it is exactly the lie this whole function exists to prevent.
static func effective_tiers(kits: Array, kit: Dictionary, band: Dictionary) -> Dictionary:
	var fresh_attack := float(kit.get(KIT_ATTACK_KEY, 0.0))
	var fresh_hunt := float(kit.get(KIT_HUNT_CARRY_KEY, 0.0))
	var fresh_forage := float(kit.get(KIT_FORAGE_CARRY_KEY, 0.0))
	var fresh_pen := float(kit.get(KIT_PEN_CARRY_KEY, 0.0))
	var conditions: Array = band.get(BAND_ITEM_CONDITIONS_KEY, [])
	if conditions.is_empty():
		return {
			KIT_ATTACK_KEY: fresh_attack,
			KIT_HUNT_CARRY_KEY: fresh_hunt,
			KIT_FORAGE_CARRY_KEY: fresh_forage,
			KIT_PEN_CARRY_KEY: fresh_pen,
			"stated": false,
		}
	return {
		KIT_ATTACK_KEY: _tier_after_wear(kits, KIT_ATTACK_KEY, fresh_attack,
			condition_of(band, KIT_ATTACK_KEY)),
		KIT_HUNT_CARRY_KEY: _tier_after_wear(kits, KIT_HUNT_CARRY_KEY, fresh_hunt,
			condition_of(band, KIT_HUNT_CARRY_KEY)),
		KIT_FORAGE_CARRY_KEY: _tier_after_wear(kits, KIT_FORAGE_CARRY_KEY, fresh_forage,
			condition_of(band, KIT_FORAGE_CARRY_KEY)),
		KIT_PEN_CARRY_KEY: _tier_after_wear(kits, KIT_PEN_CARRY_KEY, fresh_pen,
			condition_of(band, KIT_PEN_CARRY_KEY)),
		"stated": true,
	}

## One axis of the rule above. An unreadable roster (`INF` — no bare-handed tier on the wire) leaves
## the fresh tier standing rather than substituting a guess.
static func _tier_after_wear(kits: Array, axis_key: String, fresh: float,
		condition: float) -> float:
	if condition > CONDITION_DRY:
		return fresh
	var bare := unequipped_tier(kits, axis_key)
	return fresh if is_inf(bare) else bare

## **The remaining condition of the item backing an AXIS**, `CONDITION_DRY` when the band publishes
## no row for it.
##
## Absent reads as dry deliberately: the caller has already established that the band stated *some*
## condition, so a missing row for one item is a wire the client does not understand — and quoting a
## kitted number for gear the server never confirmed is the failure mode this whole model exists to
## prevent. Erring toward the unequipped tier under-promises instead.
static func condition_of(band: Dictionary, axis_key: String) -> float:
	var item_id: String = AXIS_ITEMS.get(axis_key, "")
	if item_id.is_empty():
		return CONDITION_DRY
	for row in band.get(BAND_ITEM_CONDITIONS_KEY, []):
		if String(row.get(ITEM_CONDITION_ID_KEY, "")) == item_id:
			return float(row.get(ITEM_CONDITION_REMAINING_KEY, CONDITION_DRY))
	return CONDITION_DRY

## **IS THIS COMPONENT PART OF THIS KIT?** — a kit uses a component exactly when its tier on that
## component's axis beats the roster's bare-handed one. It answers a DISPLAY question only (whether
## the hint quotes that component's condition), never a number: `none` spends no durability, so
## printing `spears 74` beside it would describe wear it will never cause.
static func kit_uses(kits: Array, kit: Dictionary, axis_key: String) -> bool:
	var bare := unequipped_tier(kits, axis_key)
	return not is_inf(bare) and float(kit.get(axis_key, 0.0)) > bare

## **THE HINT LINE — THE EFFECTIVE TIER, NEVER THE FRESH ONE.** `attack 20.0 · carry 40.0 per hunter ·
## spears 74 · sled 58` on a hunt sheet, `carry 8.0 per gatherer · baskets 61` on a forage one: the
## tiers this band gets, then the condition of each component the kit actually consumes, so a band one
## turn from running dry can see it coming. `""` when the kit is unknown.
##
## **THE HUNT BRANCH STATES WHAT THIS SOURCE WILL ACTUALLY READ, WHICH IS WHY IT TAKES THE QUARRY.**
## A hunt row works two different things through one verb, and they read disjoint axes:
##
## - **A WILD herd is stalked and hauled** — `attack`, the sled's carry, and those two conditions.
##   Byte-identical to what this line rendered before the pen axis existed.
## - **A PEN is collected** — `pen 40.0 per keeper`, and the conditions of the handling gear and the
##   SLED. **No attack and no spears**: a penned beast is slaughtered rather than stalked, it
##   publishes no engagement stage (the same predicate the gate LINE is mounted behind), and the sim
##   charges no weapon for the kill.
##
## **AT A PEN THE TIER AND THE CONDITIONS ANSWER DIFFERENT QUESTIONS, WHICH IS WHY THE SLED APPEARS
## UNDER ONE AND NOT THE OTHER.** Only `pen_carry` sets the rate — a sled drags a carcass in off the
## range and does nothing for a pen at the camp — but the sim charges a pen slaughter over TWO
## quanta: the handling gear for what was butchered and the sled for what was hauled home. So the
## sled's tier is a number nothing on this sheet will read, while the sled's condition is wear the
## player is actually paying.
##
## **THE PEN LINE IS GATED ON THE SOURCE, NOT ON `kit_uses`, AND THE DIFFERENCE IS THE POINT.** Gating
## it on the kit printed a pen tier for a husbandry kit selected against a *wild* herd — a number that
## would never be read — while withholding it from the sled-only kit at a pen, which is the one place
## the player needs to see it: at a pen, `pen 12.0 per keeper` beside `pen 40.0 per keeper` is the
## whole visible difference the handling gear buys. The condition clauses still gate on `kit_uses`,
## for their own reason and the sim's: a kit that carries no sled wears none out.
##
## `quarry` is optional and absent means WILD: a sheet composed before the wire named a source, and
## both forage sheets, render exactly as they did.
static func tier_hint(kits: Array, kit: Dictionary, band: Dictionary, job: String,
		quarry: Dictionary = {}) -> String:
	if kit.is_empty():
		return ""
	var tiers := effective_tiers(kits, kit, band)
	var parts: Array[String] = []
	if job == JOB_FORAGE:
		parts.append(HudComposeVocab.KIT_HINT_FORAGE_CARRY_FORMAT % _tier_face(
			float(tiers[KIT_FORAGE_CARRY_KEY])))
		_append_condition(parts, kits, kit, band, tiers, KIT_FORAGE_CARRY_KEY,
			HudComposeVocab.KIT_COMPONENT_BASKETS)
	elif carry_axis_for(job, quarry) == KIT_PEN_CARRY_KEY:
		parts.append(HudComposeVocab.KIT_HINT_PEN_CARRY_FORMAT % _tier_face(
			float(tiers[KIT_PEN_CARRY_KEY])))
		_append_condition(parts, kits, kit, band, tiers, KIT_PEN_CARRY_KEY,
			HudComposeVocab.KIT_COMPONENT_HUSBANDRY_GEAR)
		_append_condition(parts, kits, kit, band, tiers, KIT_HUNT_CARRY_KEY,
			HudComposeVocab.KIT_COMPONENT_SLED)
	else:
		parts.append(HudComposeVocab.KIT_HINT_ATTACK_FORMAT % _tier_face(
			float(tiers[KIT_ATTACK_KEY])))
		parts.append(HudComposeVocab.KIT_HINT_HUNT_CARRY_FORMAT % _tier_face(
			float(tiers[KIT_HUNT_CARRY_KEY])))
		_append_condition(parts, kits, kit, band, tiers, KIT_ATTACK_KEY,
			HudComposeVocab.KIT_COMPONENT_SPEARS)
		_append_condition(parts, kits, kit, band, tiers, KIT_HUNT_CARRY_KEY,
			HudComposeVocab.KIT_COMPONENT_SLED)
	return HudComposeVocab.KIT_HINT_SEPARATOR.join(parts)

## One item's condition clause, appended only where there is something true to say: the kit has to
## actually use the item, and the band has to have stated its condition. The axis names the item
## through `AXIS_ITEMS`, so there is no second key to keep in step with it.
static func _append_condition(parts: Array[String], kits: Array, kit: Dictionary,
		band: Dictionary, tiers: Dictionary, axis_key: String, component: String) -> void:
	if not bool(tiers.get("stated", false)) or not kit_uses(kits, kit, axis_key):
		return
	var condition := condition_of(band, axis_key)
	if condition <= CONDITION_DRY:
		parts.append(HudComposeVocab.KIT_HINT_DRY_FORMAT % component)
	else:
		parts.append(HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [component, int(condition)])

static func _tier_face(value: float) -> String:
	return String.num(value, HudComposeVocab.KIT_TIER_DECIMALS)

# ---- THE HONESTY RULE ---------------------------------------------------------------------------

## **WHICH KIT A HERD'S ESTIMATE TABLE IS QUOTED FOR** — the id the wire states, falling back to the
## default that applies to this herd when the table id is unset. It never guesses: the sim prices both
## tables at THIS HERD'S OWN default (all three ids come off one quoted party), so that default IS the
## honest answer where the field is missing, and the two readings agree on live data.
##
## **THE FALLBACK HAS TO BE THE SAME DEFAULT THE SHEET OPENS ON, or the refusal fires on every
## small-game herd.** A sheet opened on the warren's `trapping` while this answered the job's
## `big_game` would compare two ids that can never match and suppress the trip readout for a table
## that was in fact priced for the very kit selected — the exact inversion the per-quarry default was
## introduced to prevent.
static func estimates_quoted_kit(herd: Dictionary, table_key: String, default_id: String) -> String:
	var stated := String(herd.get(table_key, "")).strip_edges()
	return stated if stated != "" else default_kit_for(JOB_HUNT, herd, default_id)

## **MAY THIS SHEET PRESENT THE TABLE AS THE ANSWER FOR `selected_id`?** Compare the ids — never
## assume the default is selected.
##
## `huntTripEstimates` / `denialEstimates` are quoted for ONE kit and repricing them per kit was
## scoped out, so a sheet whose selection differs must suppress the turn-count verdict and the take
## line rather than showing figures computed for a different kit. The error is not a small one: the
## kit moves the take through BOTH the fight (attack tier) and the haul (sled tier), and a `none`
## party against a Red Deer's defense 1 has an effective attack of ZERO — no party size works at all.
static func estimates_apply_to(herd: Dictionary, table_key: String, default_id: String,
		selected_id: String) -> bool:
	return estimates_quoted_kit(herd, table_key, default_id) == selected_id

## The sentence a sheet renders in place of the suppressed figures — it names the kit the table IS
## priced for and the kit the player picked, so "why is there no turn count?" is answered on the sheet
## rather than inferred from an absence. `""` when the two agree (nothing to explain).
static func estimates_quoted_note(kits: Array, herd: Dictionary, table_key: String,
		default_id: String, selected_id: String, format: String) -> String:
	var quoted := estimates_quoted_kit(herd, table_key, default_id)
	if quoted == selected_id:
		return ""
	return format % [display_name_for_id(kits, quoted), display_name_for_id(kits, selected_id)]

# ---- the control --------------------------------------------------------------------------------

## **THE KIT ROW: a key label, an `OptionButton` naming the current kit, and the effective-tier hint
## beneath it.** Mounted directly under the party/crew stepper and above the forecast on all four
## compose sheets, because a kit describes the crew and moves every figure below it.
##
## **It is the SAME family of control as the `Band:` picker above it and the `Quarry` button beside
## it** — one declared key width (`HudWidgets.build_field_key`), one ghost chrome, one height — so the
## sheet's field rows read as one form. The affordance is the control's own themed arrow, never a
## glyph in the face: see `HudComposeVocab.KIT_PICKER_FACE_FORMAT`.
##
## **WEAR NEVER DISABLES AN ENTRY, AND APPLICABILITY DOES.** These are two different axes and the
## file used to carry only the first, so they are worth stating together:
##
## - **A worn component degrades the TIER, never the choice.** Every kit stays selectable however
##   spent the band's gear is, because the step-down is already said in the HINT and a picker that
##   dropped a kit as it wore out would reshuffle between turns.
## - **A kit that cannot change THIS quarry's outcome is greyed** (`kit_offer`, resolved at the fresh
##   tier) — a snare against a Red Deer, handling gear against a herd with no pen. It is greyed rather
##   than hidden, and it states its REASON on its own row, because *"a snare cannot hold a Red Deer"*
##   is a fact about the world worth teaching once and invisibility is what let the sheet quote a
##   take for a hunt that brought home nothing. A greyed entry is not selectable.
##
## The two never contradict: the first is about the band, the second about the pair (kit × quarry),
## and only the second is allowed to remove a choice.
##
## `quarry` / `prefix` are optional — a sheet with no source in hand (both forage sheets) passes none
## and every kit is offered, exactly as before the test existed. **The quarry reaches the HINT too**,
## which is how a pen's row states the keeper's carry where a wild herd's states the hunter's.
##
## Returns `null` when the job offers no kit at all, so a sheet whose verb the roster does not cover
## renders exactly as it did before the picker existed.
static func build_kit_row(kits: Array, job: String, selected_id: String, default_id: String,
		band: Dictionary, on_pick: Callable, quarry: Dictionary = {},
		prefix: String = "") -> VBoxContainer:
	var offered := kits_for_job(kits, job)
	if offered.is_empty():
		return null
	var selected := kit_by_id(offered, selected_id)
	var block := VBoxContainer.new()
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
	row.add_child(HudWidgets.build_field_key(HudComposeVocab.COMPOSE_FIELD_KIT))
	var glyph := String(HudComposeVocab.KIT_JOB_GLYPHS.get(job,
		HudComposeVocab.KIT_JOB_GLYPH_FALLBACK))
	# **THE MARK FOLLOWS THE ID THE SHEET ACTUALLY OPENED ON** — `default_kit_for`, the same
	# precedence `resolve_selection` used. Tagging the JOB's default while the sheet opens on the
	# HERD's would have the picker contradict itself on every small-game herd: Trapping selected,
	# `(default)` printed on Stalking.
	var effective_default := default_kit_for(job, quarry, default_id)
	var entries: Array = []
	# **THE SELECTION IS AN INDEX, because an `OptionButton` marks the current entry itself.** The
	# roster order IS the list order (this layer sorts nothing), so the index of the resolved kit is
	# the whole of what the control needs to open on it and to draw its radio dot.
	var selected_index := -1
	for kit_variant in offered:
		var kit: Dictionary = kit_variant
		var kit_id := String(kit.get(KIT_ID_KEY, ""))
		var label := kit_display_name(kit)
		if kit_id == effective_default:
			label += HudComposeVocab.KIT_DEFAULT_ENTRY_SUFFIX
		if kit_id == selected_id:
			selected_index = entries.size()
		var offer := kit_offer(kits, kit, job, quarry, prefix)
		var reason := String(offer[OFFER_REASON_KEY])
		# The reason rides the ENTRY'S OWN FACE, not only its tooltip: a disabled popup row is the one
		# control in this HUD a player cannot hover to interrogate on every platform, and a grey row
		# with no words is the invisibility this test exists to end.
		if not bool(offer[OFFER_OFFERED_KEY]):
			label = HudComposeVocab.KIT_WITHHELD_ENTRY_FORMAT % [label, reason]
		entries.append({
			"label": label,
			"disabled": not bool(offer[OFFER_OFFERED_KEY]),
			"tooltip": reason,
			"on_pick": func() -> void: on_pick.call(kit_id),
		})
	# The face carries the JOB GLYPH and no default suffix, which is why it is stated separately from
	# the list: the glyph says what this crew is walking out to do (one per sheet, so repeating it down
	# every row would be noise), and `(default)` is a note about an entry rather than about the choice.
	var picker := HudWidgets.build_option_picker(entries, selected_index,
		HudComposeVocab.KIT_PICKER_FACE_FORMAT % [glyph, kit_display_name(selected)],
		HudComposeVocab.KIT_PICKER_TOOLTIP)
	picker.set_meta(KIT_PICKER_META, true)
	row.add_child(picker)
	block.add_child(row)
	var hint_text := tier_hint(kits, selected, band, job, quarry)
	if hint_text != "":
		var hint := HudWidgets.alloc_hint_label(hint_text)
		hint.set_meta(KIT_HINT_META, true)
		block.add_child(hint)
	return block
