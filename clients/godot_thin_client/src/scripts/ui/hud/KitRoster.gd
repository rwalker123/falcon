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

## **WHAT THE KIT DOES TO THE QUARRY'S RETREAT** — a multiplier on the species' own wariness, so the
## SPECIES decides what a noisy approach costs (`equipment.md`). Neutral at `1.0`; a trap ships `0`.
const KIT_DISPERSION_KEY := "dispersion"
const DISPERSION_NEUTRAL := 1.0

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
const AXIS_ITEMS := {
	"attack": "spears",
	"hunt_carry_per_worker_biomass": "sled",
	"forage_carry_per_worker_biomass": "baskets",
}

## Condition at or below which a component is spent. It is the wire's own cliff, not a display
## threshold: the sim equips a component while its remaining condition is strictly positive.
const CONDITION_DRY := 0.0

## Which kit the two pre-launch estimate tables were quoted for — the hunt job's DEFAULT, on every
## herd, always. **Neither table is repriced per kit** (they are ~95% of snapshot capture), and these
## keys exist so a client can SAY so rather than quoting a kitted raid's numbers to a bare-handed
## party. Two keys because they are two tables: if one is later repriced and the other is not, a
## single key would lie about whichever was left behind.
const HERD_TRIP_ESTIMATES_KIT_KEY := "hunt_trip_estimates_kit_id"
const HERD_DENIAL_ESTIMATES_KIT_KEY := "denial_estimates_kit_id"

## The two jobs a kit may be sent on, spelled exactly as the wire's `jobs` entries and as the
## `assign_labor` roles — aliases of `SourceForecast`'s labor kinds so the sheet's verb, the command's
## role and the roster filter can never drift into three spellings of one word.
const JOB_HUNT := SourceForecast.LABOR_KIND_HUNT
const JOB_FORAGE := SourceForecast.LABOR_KIND_FORAGE

## **THE CARRY AXIS EACH JOB IS PRICED ON** — "one item, one job" (`equipment.md`): a SLED raises the
## hunt's haul, BASKETS raise the forage web's, and no kit raises both.
##
## **`priced_source` DERIVES the axis from the job rather than taking it, and that is the whole point
## of the table.** A caller that can name the axis can name the WRONG one, and one did: the compose
## seam passed the key `effective_tiers` answered (`"forage_carry"`) to a roster lookup that spells it
## `forage_carry_per_worker_biomass`, so the reference resolved to `0`, the repricing short-circuited,
## and every kit on every sheet quoted identical numbers with only the hint line moving. Reported from
## play. A job is what a call site actually knows; the axis is this layer's business.
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

## **THE SELECTION A SHEET OPENS ON.** The player's own composed choice when it is still a kit this
## verb may be sent on, otherwise the job's default, otherwise the first kit the job lists. The
## fall-through is what lets one composed id survive a world rebuild, a roster edit, and a sheet
## switching between the hunt and denial missions (which share the `hunt` job) without ever naming a
## kit the command would refuse.
static func resolve_selection(kits: Array, job: String, default_id: String,
		composed_id: String) -> String:
	var offered := kits_for_job(kits, job)
	if offered.is_empty():
		return NO_KIT_ID
	for kit_variant in offered:
		if String((kit_variant as Dictionary).get(KIT_ID_KEY, "")) == composed_id:
			return composed_id
	for kit_variant in offered:
		if String((kit_variant as Dictionary).get(KIT_ID_KEY, "")) == default_id:
			return default_id
	return String((offered[0] as Dictionary).get(KIT_ID_KEY, ""))

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
## **The AXIS is derived from the JOB** (`JOB_CARRY_AXES`), so a caller cannot hand it a key no roster
## entry carries — see that table for the bug this closes.
##
## Answers `src` UNCHANGED where there is nothing to price against: a job with no carry axis, or a
## roster that cannot resolve the selection at all (a world rebuilt under the open sheet). Never a
## guess, and never a partial substitution.
static func priced_source(src: Dictionary, prefix: String, kits: Array, job: String,
		default_kit_id: String, composed_kit_id: String, band: Dictionary) -> Dictionary:
	var carry_key: String = String(JOB_CARRY_AXES.get(job, ""))
	if carry_key.is_empty():
		return src
	var kit := kit_by_id(kits, resolve_selection(kits, job, default_kit_id, composed_kit_id))
	if kit.is_empty():
		return src
	var tiers := effective_tiers(kits, kit, band)
	return repriced_source(src, prefix, float(tiers.get(carry_key, 0.0)),
		equipped_tier(kits, carry_key),
		float(kit.get(KIT_DISPERSION_KEY, DISPERSION_NEUTRAL)))

## **THE KIT'S ATTACK AGAINST THIS QUARRY** — the kit's own number inside its size window, and the
## band's unequipped attack outside it.
##
## A snare holds a hare and not a deer, so asking a kit for its attack without naming the animal gets
## the kit's BEST case — which would tell a player the trapping kit can take a Red Deer. `0` on a
## bound means unbounded, which every weapon but the passive device is.
static func attack_against(kit: Dictionary, body_mass: float, unequipped_attack: float) -> float:
	var low := float(kit.get("attack_min_body_mass", 0.0))
	var high := float(kit.get("attack_max_body_mass", 0.0))
	if (low > 0.0 and body_mass < low) or (high > 0.0 and body_mass > high):
		return unequipped_attack
	return float(kit.get(KIT_ATTACK_KEY, unequipped_attack))

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
static func effective_tiers(kits: Array, kit: Dictionary, band: Dictionary) -> Dictionary:
	var fresh_attack := float(kit.get(KIT_ATTACK_KEY, 0.0))
	var fresh_hunt := float(kit.get(KIT_HUNT_CARRY_KEY, 0.0))
	var fresh_forage := float(kit.get(KIT_FORAGE_CARRY_KEY, 0.0))
	var conditions: Array = band.get(BAND_ITEM_CONDITIONS_KEY, [])
	if conditions.is_empty():
		return {
			KIT_ATTACK_KEY: fresh_attack,
			KIT_HUNT_CARRY_KEY: fresh_hunt,
			KIT_FORAGE_CARRY_KEY: fresh_forage,
			"stated": false,
		}
	return {
		KIT_ATTACK_KEY: _tier_after_wear(kits, KIT_ATTACK_KEY, fresh_attack,
			condition_of(band, KIT_ATTACK_KEY)),
		KIT_HUNT_CARRY_KEY: _tier_after_wear(kits, KIT_HUNT_CARRY_KEY, fresh_hunt,
			condition_of(band, KIT_HUNT_CARRY_KEY)),
		KIT_FORAGE_CARRY_KEY: _tier_after_wear(kits, KIT_FORAGE_CARRY_KEY, fresh_forage,
			condition_of(band, KIT_FORAGE_CARRY_KEY)),
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
static func tier_hint(kits: Array, kit: Dictionary, band: Dictionary, job: String) -> String:
	if kit.is_empty():
		return ""
	var tiers := effective_tiers(kits, kit, band)
	var parts: Array[String] = []
	if job == JOB_FORAGE:
		parts.append(HudComposeVocab.KIT_HINT_FORAGE_CARRY_FORMAT % _tier_face(
			float(tiers[KIT_FORAGE_CARRY_KEY])))
		_append_condition(parts, kits, kit, band, tiers, KIT_FORAGE_CARRY_KEY,
			HudComposeVocab.KIT_COMPONENT_BASKETS)
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
## job default when the herd is silent. It never guesses: the sim prices both tables at the hunt job's
## default on every herd, so the default IS the honest answer where the field is unset, and the two
## readings agree on live data.
static func estimates_quoted_kit(herd: Dictionary, table_key: String, default_id: String) -> String:
	var stated := String(herd.get(table_key, "")).strip_edges()
	return stated if stated != "" else default_id

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
## There is deliberately **NO disabled/unavailable state**: every kit in the roster is always
## selectable — a worn component degrades the tier rather than removing the kit — and the wire carries
## no availability field to invent one from. A spent component is said in the HINT, where it belongs.
##
## Returns `null` when the job offers no kit at all, so a sheet whose verb the roster does not cover
## renders exactly as it did before the picker existed.
static func build_kit_row(kits: Array, job: String, selected_id: String, default_id: String,
		band: Dictionary, on_pick: Callable) -> VBoxContainer:
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
	var entries: Array = []
	# **THE SELECTION IS AN INDEX, because an `OptionButton` marks the current entry itself.** The
	# roster order IS the list order (this layer sorts nothing), so the index of the resolved kit is
	# the whole of what the control needs to open on it and to draw its radio dot.
	var selected_index := -1
	for kit_variant in offered:
		var kit: Dictionary = kit_variant
		var kit_id := String(kit.get(KIT_ID_KEY, ""))
		var label := kit_display_name(kit)
		if kit_id == default_id:
			label += HudComposeVocab.KIT_DEFAULT_ENTRY_SUFFIX
		if kit_id == selected_id:
			selected_index = entries.size()
		entries.append({
			"label": label,
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
	var hint_text := tier_hint(kits, selected, band, job)
	if hint_text != "":
		var hint := HudWidgets.alloc_hint_label(hint_text)
		hint.set_meta(KIT_HINT_META, true)
		block.add_child(hint)
	return block
