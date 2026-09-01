class_name KitRoster

## THE KIT ROSTER LAYER (`docs/plan_denial_raid.md`, `equipment.json` `kits`) — the read over
## `SubsistenceSection.kits`, the EFFECTIVE tier a given band gets under a given kit, the test of
## whether a kit is any use on a given quarry, and the picker row every surface that names a kit
## mounts.
##
## WHY IT IS ITS OWN FILE. The control appears on FOUR sheets across TWO controllers (the Band panel's
## hunting-party and denial forms, the herd drawer's assign-hunters block, the land drawer's
## assign-foragers block) **and on the WORKFORCE zone's two band-wide role CARDS**. A kit describes the
## crew, so the row sits directly under the crew stepper and above every forecast on all four — and a
## row that has to read identically in six places is exactly the thing that must have one
## implementation. Same measurement that produced `SourceForecast` and `HudWidgets`.
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
## to load the whole client. **`DetailFormat` joined that list for `role_hint` alone**, and only from
## inside a function body, never as a `const` initializer: the band-wide role line is the Gear
## popover's own wording and must not be a second copy of it. `DetailFormat` reads nothing here.

# ---- the wire's own keys ------------------------------------------------------------------------
# The kit roster + the two job defaults, decoded once per world onto the snapshot dict
# (`native/src/dict/subsistence.rs` → `kits_to_array`).
const KIT_ID_KEY := "id"
const KIT_DISPLAY_NAME_KEY := "display_name"
const KIT_JOBS_KEY := "jobs"

## **WHICH ITEMS THIS KIT CARRIES** — its `equipment.json` `uses` list verbatim, in config order
## (weapon first, haul aid after), which is the order the hint reads them out in. An EMPTY list is a
## real answer — `none` carries nothing and wears nothing — never "unknown".
##
## The tiers below are bare numbers and name no item, so before this list reached the wire a
## condition readout had to GUESS which item produced them. It guessed `attack → spears`, and told a
## Trapping party it carried spears while quoting the SPEARS' remaining condition — exactly backwards
## for a band with fresh traps and dry spears.
const KIT_ITEM_IDS_KEY := "item_ids"

## The three CARRY/FIGHT tier axes a kit publishes, and the ONE mapping from each to the consumable
## component behind it (`equipment.json` "One kit, one job"): spears raise ATTACK, a SLED raises the HUNT's
## carry, BASKETS raise the FORAGE web's. **The two carry tiers are not two readings of one number** —
## a band can be out of baskets with its sled untouched — and rendering one on the other's row is the
## defect the three-kit split corrected sim-side.
const KIT_ATTACK_KEY := "attack"
const KIT_HUNT_CARRY_KEY := "hunt_carry_per_worker_biomass"
const KIT_FORAGE_CARRY_KEY := "forage_carry_per_worker_biomass"

## ⛔ **THERE IS ONE CARRY RATE AND A PEN IS COLLECTED ON IT** (issue #543). A `KIT_PEN_CARRY_KEY`
## stood here reading *"**`pen_carry` is NOT a second reading of `hunt_carry`** — a sled drags a
## carcass in off the range and a pen stands at the camp, so a kit carrying only a sled collects a
## pen at the bare rate"*. That was true while the bare side lived on the **hurdles**: a handling
## crew collected 40 at a pen where a drag-harness crew collected 12. `docs/plan_standing_upkeep.md`
## §4.9 item 12 turned hurdles into a MATERIAL and deleted the item, which put BOTH sides of the pair
## on the sled — two names for one number — so `EquipmentStat::PenCarry` and its three wire fields
## are gone. **What a worker can carry is a fact about the people and their gear, never about the
## ground they stand on**, so a penned herd is priced on `KIT_HUNT_CARRY_KEY` above like any other
## hunt row. Nothing here may reintroduce a pen axis to "restore" the distinction: it does not exist.
## **THE SCOUT'S AXIS, AND IT HAS A SURFACE NOW.** It was declared for the roster's axis vocabulary
## with no hint-line consumer — `tier_hint` was written for the two COMPOSE sheets, which are hunt and
## forage only — and the WORKFORCE zone's role CARDS are the surface that comment said to wait for:
## each carries a picker and a gear line, priced through `role_gear`. See `ROLE_AXES` — and a
## `BandKitTiers` row STATES this axis, so the card reads what the SELECTED kit grants THIS band at
## its live wear (`BAND_KIT_TIERS_KEY`), never the roster's fresh vantage.
const KIT_SCOUT_VANTAGE_KEY := "scout_vantage_range"

## **THE BUILD AXIS — the WORK UNITS one equipped worker DELIVERS per turn, over and above its bare
## hands.** Neutral `0.0`, so `unequipped_tier` (the roster's MINIMUM on an axis) answers `0.0` off
## the `none` kit and `kit_uses` reads *"declares more than neutral"* with no special case. The value
## belongs to the ITEM the kit carries: flint hoes deliver +0.5 a turn on a PLANT build and hurdles
## the same on an ANIMAL one, which is why the branch below is read with it and never without.
##
## ⛔ **IT IS NOT SUBTRACTED FROM ANYTHING, AND IT WAS UNTIL `docs/plan_standing_upkeep.md` §4.8.**
## The axis shipped as *"work units taken off an improvement's cost"* — a lump against the pile,
## granted once however long the job ran — and a job's work requirement never changes now: the term
## is an addend of the SUPPLY the remaining work is divided by, so it is paid every turn the tool is
## held. The magnitudes moved with the meaning and cannot be carried across: the shipped tool
## declared `8.5` as units off a job and declares `0.5` as work added per equipped worker per turn.
## The client evaluates it in exactly one place, `SourceForecast.build_turns_at`.
##
## **IT SUPERSEDES THE RETIRED `build_rate` MULTIPLIER** (`docs/plan_unit_costed_work.md` §6). That
## stat multiplied the CREW's output, and a multiplier cancels the job's cost — it saves the same
## PERCENTAGE of turns on a garden and on a farm alike, which is the shape the work-costed arc exists
## to escape. Stated as a per-worker QUANTITY of work instead, the job's own size decides what the
## tool is worth. The wire still carries `buildRate`, frozen at its neutral `1`, and this client no
## longer decodes it: a reader left on it reads "changes no build" for every kit in the game, which
## silently drops the handling gear's own clause AND withholds the kit carrying it from a herd being
## tamed (see `kit_offer`).
##
## **IT IS NOT A TIER AND HAS NO HINT-LINE HOME.** The four axes above are rates a readout can quote
## per worker; this one prices a build the sheet is not otherwise talking about, and the surface that
## states it is the band panel's gear row (`DisclosureController.kit_breakdown_lines`). What it does
## HERE is decide applicability — see `kit_offer`.
const KIT_BUILD_WORK_KEY := "build_work_per_worker"

## **THE BUILD AXIS'S NEUTRAL — `0.0`, and NOT the multipliers' `1.0`.** It is a quantity of work a
## tool ADDS to what a worker delivers, so *no tool* is *no extra work*; reading the multipliers'
## neutral here would hand every bare-handed crew a free work unit per worker on every build, on top
## of the bare rate the source already publishes. The sim states the same split on
## `EquipmentStat::neutral`.
const BUILD_WORK_NONE := 0.0

## **HOW MANY OF THIS BAND'S WORKERS THIS KIT CAN ACTUALLY EQUIP FOR A BUILD** — the head count at or
## above which extra hands take no further work off a job. `0` (the neutral) means the kit carries
## nothing live that helps, which is every row but the handling gear's on the shipped roster.
##
## **IT IS THE OTHER HALF OF THE AXIS ABOVE, and the pair is what makes the gear term a closed form**
## a compose sheet can evaluate against a crew the player is PROPOSING: coverage arms a prefix of the
## party, so `gear(w) = min(w, this) × the per-worker worth` — piecewise-linear and SATURATING. Both
## facts behind it (the units held and each unit's reach) are the BAND's ledger, which is why the pair
## rides this row rather than a worked source: a rung nobody has started still has a quote, and
## picking another kit re-prices the whole estimate. `build_work_from_gear` on the SOURCE is the
## resolved contribution for the crew that worked it this turn — a different question, and not one a
## stepper can move.
const KIT_BUILD_SATURATING_CREW_KEY := "build_work_saturating_crew"

## **WHICH FOOD WEB THE TWO AXES ABOVE ARE FOR** — `BUILD_BRANCH_PLANT` / `BUILD_BRANCH_ANIMAL`, and
## `BUILD_BRANCH_NONE` for a kit carrying no build tool at all. It rides BOTH the roster's fresh
## `KitOption` row and the band's own resolved `kit_tiers` row, because it is a property of the tool
## and a spent tool declares nothing.
##
## **THE THREE BUILD FIELDS ARE ONE READING** (`equipment.md` → "A `build_work` EFFECT MUST DECLARE
## ITS `branch`"). Flint hoes add +0.5 a turn to a worker raising a Cultivate and NOTHING to one
## raising a Tame; hurdles do the reverse. So
## a worth read without its branch is a number that is real and simply not real HERE — the same
## discipline `attack_max_body_mass` imposes on `attack`, and the same failure if it is skipped: a
## sheet quoting the hurdles' contribution against a garden promises a build that cannot land.
const KIT_BUILD_BRANCH_KEY := "build_work_branch"

## **AND WHICH RUNG OF THAT BRANCH THE TOOL IS FOR** — `route:dirt_road` on the roadbuilding kit,
## `route:paved_road` on the paving one, and `BUILD_RUNG_ANY` on every other kit shipped, whose tool
## serves every rung its branch has.
##
## ⛔ **THE READING IS THREE FIELDS NOW, AND THE THIRD IS WHERE A SHEET LIES MOST QUIETLY.** A
## dressing hammer is worth `2.0` a worker on a `pave` and EXACTLY `BUILD_WORK_NONE` on the `grade`
## underneath it — the branch matches on both, so a reader that stopped at the branch would quote the
## paving kit's uplift on a road being graded, which is the same defect one axis further in as
## quoting a worth with no branch at all.
##
## ⛔ **IT RIDES BOTH TABLES, AND THE TRIPLE IS READ TOGETHER ON EACH.** `KitOption.buildWorkRung` is
## the ROSTER's fresh answer — what `kit_serves_build`, `work_kit_for_branch` and `kit_offer`'s
## greying read — and `BandKitTiers.buildWorkRung` is the BAND's resolved one, which `build_gear`
## reads. Worth + branch alone is wrong on exactly the branch the field was added for: with both road
## kits declaring `route` and nothing else, the roster answered *two kits serve a `grade`* and the
## derivation handed back whichever it listed FIRST, so a `pave` entry opened on the roadbuilding
## kit. The sim pins the roster side (`the roster must name EXACTLY ONE kit for a grade`).
const KIT_BUILD_RUNG_KEY := "build_work_rung"

## **`""` — AND IT MEANS TWO DIFFERENT THINGS ON THE TWO SIDES OF THE TEST, WHICH IS WHY ONE SPELLING
## IS ENOUGH.** On the KIT it is *this tool serves every rung on its branch*; on the CALLER it is *I
## cannot say which rung is being priced*. `kit_serves_build` reads the kit's side FIRST, so the two
## never have to be told apart — the same shape as the sim's `Option` pair
## (`EquipmentEffect::serves_build`).
const BUILD_RUNG_ANY := ""

## The ladder's own two webs, spelled as the wire spells them (`RungBranch`), and the empty answer
## that is a real reading rather than a gap: a kit with no build tool serves NO branch, which is why
## `none` can never be the derived builders kit and is never greyed for the wrong one either.
const BUILD_BRANCH_PLANT := "plant"
const BUILD_BRANCH_ANIMAL := "animal"
## ⛔ **THE THIRD BRANCH IS NOT A FOOD WEB, AND IT IS SPELLED ANYWAY.** `RungBranch::Route` is
## `"route"` on the wire, and **no shipped kit declares a `build_work` effect serving it** — so
## `build_gear` answers `{}` for every kit today and the route ladder prices a road at bare hands,
## which is the truth about the shipped equipment rather than a gap. **Asking with the branch instead
## of asking with `BUILD_BRANCH_NONE` is what keeps it true**: `NONE` means *no branch test at all*
## and would credit the crook's 0.5 against a road. The day a barrow declares one, the estimate picks
## it up with no client edit.
const BUILD_BRANCH_ROUTE := "route"
const BUILD_BRANCH_NONE := ""

## **WHICH WEB A LABOR KIND'S BUILDS BELONG TO.** The web a queue entry sits on is a fact about the
## SOURCE — a patch is plant, a herd is animal — which is exactly how `systems::labor` stamps an
## entry, so a compose sheet knows its entry's branch from the job it is composing and needs no new
## wire field to ask.
##
## ⛔ **A ROAD IS THE THIRD, AND IT ARRIVES THROUGH THE SAME DOOR.** `BuildSource::Road` publishes
## `kind = roadwork` — `ROADWORK_ROLE_KEY`, the band-wide keeping row's own token, because a road has
## no take row of its own for the queue to join to. So a road entry reaches this table exactly as a
## patch does, and while the row was missing every road entry answered `BUILD_BRANCH_NONE`: not *no
## gear*, but **no branch test at all**, which is what `build_gear` reads as *quote whatever the row
## holds*. A band carrying the paving kit was therefore quoted its `2.0` on every job in the queue.
const LABOR_KIND_ROADWORK := "roadwork"
const LABOR_KIND_BUILD_BRANCHES := {
	SourceForecast.LABOR_KIND_FORAGE: BUILD_BRANCH_PLANT,
	SourceForecast.LABOR_KIND_HUNT: BUILD_BRANCH_ANIMAL,
	LABOR_KIND_ROADWORK: BUILD_BRANCH_ROUTE,
}

## The build branch of a source worked on this labor kind; `BUILD_BRANCH_NONE` for a kind that raises
## nothing (scout, warrior, and the builders row itself, which stands on no source).
static func build_branch_for_kind(kind: String) -> String:
	return String(LABOR_KIND_BUILD_BRANCHES.get(kind, BUILD_BRANCH_NONE))

## **THE BRANCH AS A PLAYER SAYS IT** — `a crop build` / `an animal build`, for the reason line on a
## greyed builders kit. The table lives here rather than in the vocabulary module because a leaf may
## not read a const off a module that reads one off it (`const` initializers evaluate at class load,
## and that cycle fails to load the client); the WORDS are still `HudComposeVocab`'s.
##
## An unrecognised branch reads back as itself — the wire's own token is a poorer sentence than these
## two and a better one than a blank.
static func build_branch_noun(branch: String) -> String:
	match branch:
		BUILD_BRANCH_PLANT:
			return HudComposeVocab.KIT_BUILD_BRANCH_PLANT_NOUN
		BUILD_BRANCH_ANIMAL:
			return HudComposeVocab.KIT_BUILD_BRANCH_ANIMAL_NOUN
		BUILD_BRANCH_ROUTE:
			return HudComposeVocab.KIT_BUILD_BRANCH_ROUTE_NOUN
		_:
			return branch

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
## Aliased off the shared layer, not re-spelled: `SourceForecast.quarry_is_fought` reads the same wire
## key, and two spellings of one field is how two surfaces come to disagree about a pen.
const QUARRY_CORRALLED_KEY := SourceForecast.SOURCE_CORRALLED_KEY

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

## **WHAT EVERY OFFERED KIT WOULD GRANT *THIS* BAND, RIGHT NOW** — one row per roster kit on the
## band's own cohort (`PopulationCohortState.kitTiers`), resolved by the sim against this band's LIVE
## wear. `{kit_id, attack, hunt_carry_per_worker_biomass, forage_carry_per_worker_biomass,
## scout_vantage_range, attack_min_body_mass, attack_max_body_mass, dispersion, exposure}`.
##
## **IT IS THE ANSWER, AND NOTHING HERE MAY RE-DERIVE IT.** This layer used to step a fresh tier down
## by asking whether the item behind an axis still had condition — which needs to know WHICH ITEM
## SUPPLIES WHICH AXIS, and that mapping is per kit: `big_game` gets attack from `spears`, `trapping`
## from `traps`. `KitOption.item_ids` says what a kit carries, never what each item is FOR, and no rule
## over that list recovers it (set-cover and positional order both mis-assign; "any item live" keeps a
## kit at full tier with its weapon dry; "all items dry" keeps it at full tier with only the sled
## left). The live symptom of guessing was a band with FRESH TRAPS AND DRY SPEARS repriced to the bare
## hand under `trapping` — same root cause as the pre-launch estimate tables this arc retired, a fact
## the sim knew that the wire did not carry, and the same fix: publish the answer.
##
## **IT STATES ALL FOUR AXES — the fought, hauled, gathered and SEEN ones.** This doc said *"ALL FIVE
## … the fought, hauled, gathered, COLLECTED and SEEN"*, the COLLECTED one being `pen_carry`, which is
## deleted (issue #543): a pen is collected on the hauled one. `scout_vantage_range` is the axis the
## row's original argument still turns on — it was taken off the ROSTER's fresh tier before the row
## carried it, so a Scout card read `2-tile sight per vantage` while `calculate_visibility` revealed
## at 1, wrong in the reassuring direction.
##
## **THE COHORT'S FLAT `scout_vantage_range` STAYS, and it is not redundant with the row.** It answers
## *this band at its JOB DEFAULT* — the question a readout with no kit selected asks (the Gear
## popover's rows) — and this table answers *what the kit under the cursor would grant*, which is the
## picker's. Neither is derivable from the other: the job default is one kit and the picker offers all
## of them. The cohort's flat `pen_carry_per_worker_biomass` was the second half of that pair and went
## with the axis.
const BAND_KIT_TIERS_KEY := "kit_tiers"
const BAND_KIT_TIERS_ID_KEY := "kit_id"

## Condition at or below which a component is spent. It is the wire's own cliff, not a display
## threshold: the sim equips a component while its remaining condition is strictly positive.
const CONDITION_DRY := 0.0


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

## **THE BUILDING ROLE IS A JOB TOO** (`docs/plan_standing_upkeep.md` §2.5). A build's gear offset used
## to ride the SOURCE ROW's kit — a Corral was priced off the hunt row's husbandry gear — and it is
## read off this role's own row now that the build crew has left the tile.
##
## **THERE ARE TWO BUILDERS KITS, ONE PER WEB — `hurdling` (hurdles) and `tillage` (hoes)** — and
## `husbandry` is not one of them, and since `docs/plan_standing_upkeep.md` §4.9 item 12b it is not a
## kit at all — the weaponless hunt bundle was deleted, `big_game` being a strict superset of it. (The
## `husbandry` JOB is untouched: `hurdling` lists it, and `keeping_kit_for_branch` maps the animal
## branch to it.) The claim this comment used to carry — *`husbandry` is the ONE kit whose items
## declare `build_work`* — was the whole reason an animal-handling bundle was offered for a Cultivate,
## and `hoes` declare the axis too.
##
## ⚠ **THE WIRE NAMES NO DEFAULT FOR IT, AND THE ROW STATES THE DERIVED ANSWER INSTEAD.**
## `SubsistenceSection` publishes `defaultHunt` / `Forage` / `Scout` / `WarriorKitId` and no builders
## twin, so `HudBandLaborState.default_kit_id` answers `""` here and nothing is marked `(default)` —
## the honest statement of what the client knows. What the card opens on is the band's OWN `builders`
## row (`_role_kit_id`), which the sim resolves **per queue entry** before publishing
## (`equipment.md` → "THE BUILDERS' KIT IS DERIVED PER QUEUE ENTRY"): a kit named on the row wins,
## `none` included, and otherwise the roster answers for the head entry's web. So that row is a live
## fact about what the pool is holding this turn rather than a stored id.
const JOB_BUILDERS := "builders"

## **THE AXIS EACH BAND-WIDE ROLE IS PRICED ON** — a Scout's kit buys what a posted vantage can make
## out, a Warrior's buys the `attack` the camp is defended at. Only the two roles with no source to
## work appear: a hunt or forage crew is priced on a CARRY axis instead (`JOB_CARRY_AXES`), and a
## role axis and a carry axis must not collapse into one table.
##
## **THE BUILDERS ARE ABSENT AGAIN, AND THIS TABLE FOLLOWS ITS READERS.** `KIT_BUILD_WORK_KEY` had an
## entry here for exactly as long as the Builders card carried a read-only gear line
## (`docs/plan_standing_upkeep.md` §2.5); §4.7 retired that line — the BUILD QUEUE head states the
## pool's kit adjacent to the jobs it prices — so nothing resolves a build-axis hint any more, and a
## row here with no reader is a lever that looks live. A builders kit picker landing on a queue row
## (§7's ②) puts it back, with that row as its caller.
const ROLE_AXES := {
	JOB_SCOUT: KIT_SCOUT_VANTAGE_KEY,
	JOB_WARRIOR: KIT_ATTACK_KEY,
}

## Is this a BAND-WIDE role — one standing slot, no source, priced on `ROLE_AXES`? The one test, so a
## caller never spells the pair of job names.
static func is_band_wide_role(job: String) -> bool:
	return ROLE_AXES.has(job)

## The axis this role's kit is priced on, `""` for a job that works a source instead.
static func role_axis(job: String) -> String:
	return String(ROLE_AXES.get(job, ""))

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
## ⛔ **IT IS THE WHOLE ANSWER NOW.** This doc read *"IT IS THE JOB'S ANSWER, NOT THE LAST WORD — a
## penned herd overrides it"*, and `carry_axis_for` carried that override to a `pen_carry` axis of its
## own. The axis is deleted (issue #543): **a pen is collected on the hunt's haul**, so the job
## decides and the source has nothing to say about it. `carry_axis_for` is still the only thing
## anything should ask — a caller that spells an axis can spell the wrong one, which is the bug
## `priced_source` documents — but it is now a lookup and no longer a rule.
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
## ⛔ **THE BRANCH TRAVELS WITH ITS RUNG HERE TOO, and it did not.** The filter below is
## `kit_offer`'s, which reads the rung — so a caller that named a route BRANCH and no rung had both
## road kits refused by the third arm, the `selectable` list collapse back to the unfiltered `offered`,
## and the "which kits may this pool hold" question silently answered as though the branch had never
## been passed. It is the same omission that put `No kit` on the build queue header, one function
## along; the two are fixed together because `_role_kit_id` calls both in one breath.
static func resolve_selection(kits: Array, job: String, default_id: String,
		composed_id: String, quarry: Dictionary = {}, prefix: String = "",
		build_branch: String = BUILD_BRANCH_NONE,
		build_rung: String = BUILD_RUNG_ANY) -> String:
	var offered := kits_for_job(kits, job)
	if offered.is_empty():
		return NO_KIT_ID
	var selectable: Array = []
	for kit_variant in offered:
		if kit_is_offered(kits, kit_variant, job, quarry, prefix, build_branch, build_rung):
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
## key food actually reads is `per_worker_yield`. One account repriced, food did not, and the sheet
## quoted a five-fold change beside an unmoved food line. A literal cannot be wrong in a way the
## compiler or a rename would catch; a constant reference can.
##
## **`per_worker_biomass` carries more than its own account.** On the forage web `forecast_inputs`
## DERIVES fodder from it (`carry × fodder_per_biomass`), so scaling it reprices that account for
## free. (A third entry rode this list for the retired trade axis, arc #527.)
##
## **THE MATERIAL ACCOUNT IS NOT ON THIS LIST BECAUSE IT IS NOT A SCALAR** — see
## `SOURCE_PER_WORKER_VECTOR_KEYS` below, which is the same substitution one type further out.
const SOURCE_PER_WORKER_KEYS := [
	SourceForecast.FORECAST_PER_WORKER_BIOMASS_KEY,
	SourceForecast.FORECAST_PER_WORKER_KEY,
]

## **THE SAME REPRICING, FOR THE ACCOUNT THAT TRAVELS AS A VECTOR.** `per_worker_material` is
## `[{material_id, amount}]`, so it cannot ride the list above: `float(out[key]) * ratio` throws on an
## `Array`, which is why "just add the key" is the wrong repair and why the account sat unrepriced
## through the whole life of the kit seam.
##
## **The bug it closes is the one this file's other constant already records for food.** A worn or
## lesser kit produced a correctly reduced FOOD line beside an unmoved HIDE line — `expected_materials`
## clamps `min(workers × per_worker_material, ceiling)` off whatever rate reaches it, so the sheet
## over-stated a raid's materials with a worse kit and under-stated them with a better one, on BOTH
## webs (a hunt's pelts, a gather's fibre).
##
## **ONE RATIO FOR EVERY ROW**, through `SourceForecast.scaled_material_rows` rather than a loop
## written here: the materials are one biomass flow through a fixed per-biomass vector, exactly as the
## food and fodder accounts are, so a kit that moves half the biomass moves half of each of them. A
## per-material factor would be a claim about equipment the wire does not make.
const SOURCE_PER_WORKER_VECTOR_KEYS := [
	SourceForecast.FORECAST_PER_WORKER_MATERIAL_KEY,
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
## **THE RETREAT DOES NOT TOUCH `engage_rate`, AND THAT IS THE CORRECTION THIS PAIR EXISTS FOR.** The
## two stages are separately observable — `engage_rate` is a fact about the QUARRY and `dispersion`
## moves the retreat alone — so folding one into the other makes Big-game and Trapping quote the
## identical hunt on a herd whose whole difference is how much of what they reach stands still.
## Substituting on its own field keeps the arms separable, and `SourceForecast` then spends the retreat
## on BOTH the take and the crew — a party that keeps one animal in four needs four times the hands to
## draw the same stock down.
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
		# …and the same ratio through the vector account, row by row. Absent on a source that pays no
		# material, which skips without a special branch exactly as the retreat below does.
		for key in SOURCE_PER_WORKER_VECTOR_KEYS:
			var full: String = prefix + String(key)
			if out.has(full):
				out[full] = SourceForecast.scaled_material_rows(out[full], ratio)
	# **THE RETREAT, ON ITS OWN FIELD.** Absent = no retreat stage (a patch, a pen), which skips without
	# a special branch: there is no key to substitute and the take arms read the wire's own `1`.
	var stay_key := prefix + SOURCE_STAY_FRACTION
	if out.has(stay_key):
		var stay := clampf(float(out[stay_key]), 0.0, 1.0)
		out[stay_key] = clampf(1.0 - (1.0 - stay) * maxf(dispersion, 0.0), 0.0, 1.0)
	return out

## **THE CARRY AXIS THIS JOB IS COLLECTED ON** (`JOB_CARRY_AXES`). `""` for a job with no carry axis
## at all, which `priced_source` reads as "nothing to price against".
##
## ⛔ **IT TOOK THE SOURCE AND FORKED ON A PENNED HERD, AND IT DOES NOT ANY MORE** (issue #543). The
## dead rule: *"the job's answer, overridden by a PENNED herd … a corralled herd is worked from a Hunt
## row, so pricing it by job read the SLED's tier — while the sim collects a pen on
## `EquipmentStat::PenCarry`, a stat of its own and not the hunt haul's."* That stat is deleted. The
## fork it corrected was real while the hurdles were an ITEM (handling gear collected 40, a
## drag-harness crew 12); §4.9 item 12 made hurdles a material and put both sides on the sled, so the
## two axes became two names for one number. **A pen is collected on the hunt's haul, at the sled's
## own graded rate** — `recipes.json` grades `hunt_carry` (poor 30 / fair 34 / good 40 / excellent
## 46), which the flat pen rate never tracked — so a keeper and a stalker on the same kit haul the
## same and neither is quoted a fixed 40.
##
## It keeps its own name rather than being folded into `JOB_CARRY_AXES.get`, because `priced_source`
## and `tier_hint` must ask ONE question: a caller that can spell an axis can spell the wrong one, and
## one did (see `JOB_CARRY_AXES`).
static func carry_axis_for(job: String) -> String:
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
## **The AXIS is derived from the job** (`carry_axis_for`), so a caller cannot hand it a key no roster
## entry carries — see that function and `JOB_CARRY_AXES` for the two bugs this closes.
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
	var carry_key := carry_axis_for(job)
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

## **DOES A WEAPON'S SIZE WINDOW REACH AN ANIMAL OF THIS MASS AT ALL?** — the ONE home of the bound,
## so the fresh-tier offer test and the wear-resolved gate cannot read it two ways. An absent or `0`
## bound is UNBOUNDED (`MASS_BOUND_UNBOUNDED`), which is every weapon but the passive device, so a
## roster that states neither field behaves exactly as it did before the bound existed.
##
## **`stated` IS THE ROW THE ATTACK IS ALSO BEING READ FROM** — a roster entry for the fresh reading,
## the band's own `kit_tiers` row for the worn one. The two must come off ONE dict: reading the bounds
## off `KitOption` while reading the attack off the band would quote a band with dry traps the bare
## hand's attack (correct) inside the TRAPS' 1 kg ceiling (wrong), so a bare-handed party after a
## rabbit would be told it had no weapon for it.
static func attack_reaches(stated: Dictionary, body_mass: float) -> bool:
	var low := float(stated.get(KIT_ATTACK_MIN_MASS_KEY, MASS_BOUND_UNBOUNDED))
	var high := float(stated.get(KIT_ATTACK_MAX_MASS_KEY, MASS_BOUND_UNBOUNDED))
	if low > MASS_BOUND_UNBOUNDED and body_mass < low:
		return false
	return not (high > MASS_BOUND_UNBOUNDED and body_mass > high)

## **THE KIT'S FRESH ATTACK AGAINST THIS QUARRY** — the roster's own number inside its size window,
## and the roster's unequipped attack outside it. **Deliberately wear-BLIND**: its one caller is
## `kit_offer`, and which kits a sheet offers is a property of (kit × quarry) that must not reshuffle
## as gear wears. The worn reading is `effective_attack_against`.
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
## weapon down (the sim's own `kit_tiers` row), and the mass bound says the weapon was never in play
## against this animal in the first place. **BOTH COME OFF THAT ROW**, which is what makes a kit whose
## mass-bounded weapon has run dry have no size window at all rather than its fresh one; the roster's
## bounds are the FRESH-KIT reference and stand only where the band states no row.
##
## **This is the quarry-aware twin of `effective_tiers`'s `attack`, and every take path must use it.**
## Reading the bare tier is `equipment.md`'s `hunter_profile_unbounded` — *"the best this kit can do
## against something"* — which is honest only on a surface with no target in hand. A compose sheet has
## one, and quoting the unbounded reading there is what let a trapping party be sold a Red Deer.
static func effective_attack_against(kits: Array, kit: Dictionary, band: Dictionary,
		body_mass: float) -> float:
	var resolved := band_kit_tiers(band, String(kit.get(KIT_ID_KEY, "")))
	var stated: Dictionary = resolved if not resolved.is_empty() else kit
	var worn := float(stated.get(KIT_ATTACK_KEY, 0.0))
	if attack_reaches(stated, body_mass):
		return worn
	var bare := unequipped_tier(kits, KIT_ATTACK_KEY)
	# An unreadable roster states no bare-handed tier, so there is nothing to step down TO and the
	# in-window reading stands — the same fail-quiet the absent-row fall-through takes.
	return worn if is_inf(bare) else bare

# ---- IS THIS KIT ANY USE ON THIS SOURCE? --------------------------------------------------------

## The offer verdict's two keys — `offered`, and the REASON a withheld kit states on its own row.
## A greyed entry that does not say why teaches nothing, and "a snare cannot hold a Red Deer" is a
## fact about the world worth learning once.
const OFFER_OFFERED_KEY := "offered"
const OFFER_REASON_KEY := "reason"

## **DOES THIS KIT CARRY ANYTHING AT ALL?** — the derived reading of "the null kit", and the reason
## nothing here spells the id `none`. A kit carrying no item grants nothing anywhere, so there is no
## source it can be *inapplicable* to; it is the free bare-handed comparison the whole wear model
## exists to protect, and it is never withheld. A future `fishing` kit with an empty `uses` gets the
## same treatment for the same reason.
##
## **READ OFF `KitOption.item_ids`, NOT INFERRED FROM THE TIERS.** An empty list is the wire's own
## answer to this question (`snapshot.fbs`: "an EMPTY vector is a real answer … never unknown"), where
## sweeping the axes for one that beats bare was a re-derivation of a fact already stated.
static func kit_supplies_any(kit: Dictionary) -> bool:
	return not kit_item_ids(kit).is_empty()

## ⛔ **THE PEN RULE AND ITS `kit_reaches_a_wild_hunt` HELPER ARE BOTH GONE** (issue #543), because
## the axis they were asked about is. The rule withheld a kit whose only contribution was `pen_carry`
## from a WILD quarry — *"what it adds is only used on a penned herd"* — and its history is worth
## keeping because it cost a shipped feature twice: it first asked the proxy
## `kit_uses(pen_carry) and not penned`, reading *"supplies the pen axis"* as *"contributes nothing a
## wild hunt can read"*, and when `hurdles` left the roster and both sides of the axis landed on the
## **sled** that proxy became true of every hunt kit — all three greyed on every wild quarry, the
## sheet falling through to the null kit while the picker still marked the stalking kit `(default)`.
## A wild hunt could not be equipped at all. The helper was the direct question that repaired it.
##
## With `EquipmentStat::PenCarry` deleted no kit supplies a pen-only axis, so the rule could only ever
## answer false: **there is no kit that is useful at a pen and useless on the range**, and a
## sled-and-no-weapon kit is withheld from a fought quarry by the WEAPON rule as it always was.

## **THE FIGHT PREDICATE LIVES IN `SourceForecast.quarry_is_fought`, NOT HERE.** Four surfaces ask it
## — this file's offer test and priced gate, the compose sheet's refusal line, and the crew cap the
## Work board's `+` reads — and the two lower ones cannot reach up into this module, so the shared
## layer is the only home that all four can share. Its docstring owns the reason the test is not
## `has_engagement_stage`: the wire still publishes `NO_ENGAGEMENT_STAGE` for a pen, against the sim's
## own behaviour, until issue #572.

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
## 2. **A BUILDERS kit whose tool serves the other web.** `build_branch` is the entry's own web and a
##    kit's `build_work_branch` is its tool's; outside its branch the contribution is the neutral
##    `0.0`, so the kit is exactly as inapplicable as a snare on a Red Deer. This rule needs no
##    quarry — a builders row stands on no source — which is why it is answered before the two above
##    and off its own parameter. **Its live reader is `resolve_selection`'s selectable list, not a
##    picker**: the Builders role card mounts no control (`band-city-panel.md`), so what this rule
##    does today is keep that card's gear line and the build queue's header off a kit the head
##    entry's web cannot use. The withheld REASON it composes has no display surface until the
##    per-entry picker lands on the queue row.
##
## ⛔ **THERE WAS A THIRD RULE — the PEN rule — AND IT IS DELETED** (issue #543). It withheld a kit
## whose only contribution was `pen_carry` from a wild quarry, and it was asked BEFORE the weapon rule
## *"so a kit that trips it reads the same reason on every quarry"*. Its own note already recorded
## that it *"matches no kit on the shipped roster"*; with the axis itself gone it can match nothing at
## all, on any roster. A kit carrying a sled and NO weapon is withheld from a fought quarry by the
## WEAPON rule below, which is where that case belonged. See `kit_supplies_any` above for the full
## history of what the rule cost.
##
## **A PEN IS FOUGHT NOW, so rule 1 runs on one exactly as it runs on the range**
## (`docs/plan_standing_upkeep.md` §4.9 item 12b). The take resolves engage → retreat → fight at every
## rung and the fight is the species' own `defense` against the party's `attack`, unchanged by the
## fence: **containment solves the catching, weapons solve the killing.** This guard used to read *"a
## penned animal is slaughtered rather than stalked"* and spared a pen the weapon rule entirely, which
## was right for a sim that let a bare-handed band butcher a fenced aurochs; the sim quotes that band
## nothing and pays it nothing now (`core_sim/tests/hunt_useful_crew_on_the_wire.rs`).
##
## **So a corralled Red Deer DOES withhold every kit but the spear line, and that is the point rather
## than the cost.** Its `defense 1` against the bare hand's `attack 1` is `max(0, 1 − 1)`, and a trap's
## attack is bounded off a deer by `max_body_mass` — so only the spear line can bring one down, penned
## or not, and a picker that hid that would be quoting a take the sim refuses.
##
## ⛔ **THE WIRE'S `engage_rate` CANNOT ANSWER "IS THERE A FIGHT HERE?" FOR A PEN** — it still
## publishes `NO_ENGAGEMENT_STAGE` there. `quarry_is_fought` owns that reading; both this test and the
## priced gate take it, and nothing here may re-derive it from the engagement field alone.
##
## **RESOLVED AT THE FRESH TIER, AND THAT IS THE LOAD-BEARING CONSTRAINT.** Which kits are offered and
## which is default are properties of (kit × quarry); the band's wear moves the QUOTED number and the
## hint line and nothing else. A band whose spears are dry still sees the stalking kit listed,
## selectable and default against a Red Deer, quoting zero, with the hint saying the spears are gone —
## because a picker that reshuffled between turns would leave the player unable to tell a kit that
## *cannot* work on this animal from one that has merely worn out.
static func kit_offer(kits: Array, kit: Dictionary, job: String, quarry: Dictionary,
		prefix: String, build_branch: String = BUILD_BRANCH_NONE,
		build_rung: String = BUILD_RUNG_ANY) -> Dictionary:
	# **THE BUILDERS ROW HAS ITS OWN APPLICABILITY QUESTION, and it is the same one asked of a snare
	# against a Red Deer**: can what this kit carries change the outcome of the job in front of it? A
	# hoe adds nothing to a `Tame` and a crook nothing to a `Cultivate` — the axis is the EXTRA work a
	# worker delivers, never units off the job — so a builders kit serving the other web is greyed WITH
	# ITS REASON rather than hidden: a player should learn that a hoe is not for stock, and
	# invisibility is what let the wrong tool be offered in the first place. `none` is never withheld here for the reason it is never withheld anywhere: it carries
	# nothing, so there is no job it can be inapplicable *to*.
	if job == JOB_BUILDERS:
		if kit.is_empty() or build_branch == BUILD_BRANCH_NONE or not kit_supplies_any(kit):
			return _kit_offered()
		if kit_serves_build(kits, kit, build_branch, build_rung):
			return _kit_offered()
		# ⛔ **A ROAD TOOL ON THE WRONG RUNG IS REFUSED FOR A DIFFERENT REASON, AND THE SENTENCE HAS
		# TO SAY SO.** *"its tools are no use on a road build"* is FALSE of the paving kit in front of
		# a `grade` — a dressing hammer is exactly a road tool — and a reason a player can see is
		# wrong is worse than none. What is true is that its worth resolves to the neutral on every
		# rung but the one it names, so the rung-bound refusal says THAT.
		#
		# ⛔ **THE FORK IS `kit_serves_branch`, NEVER `kit_serves_build` ASKED WITH NO RUNG.** The
		# unqualified ask is the THIRD ARM and it answers `false` for exactly the tools this line is
		# trying to recognise — so using it here sends every rung-bound kit down the BRANCH sentence,
		# which is the false one. *Does this tool serve the branch* and *would it serve an
		# unqualified caller* are different questions with different answers.
		if kit_serves_branch(kits, kit, build_branch):
			return _kit_withheld(HudComposeVocab.KIT_WITHHELD_REASON_BUILD_RUNG)
		return _kit_withheld(HudComposeVocab.KIT_WITHHELD_REASON_BUILD_BRANCH_FORMAT
			% build_branch_noun(build_branch))
	# No quarry in hand and no hunt to have: the forage sheets, and a sheet composed before the wire
	# named a source. Nothing to be inapplicable to.
	if job != JOB_HUNT or kit.is_empty() or quarry.is_empty():
		return _kit_offered()
	if not kit_supplies_any(kit):
		return _kit_offered()
	# **THE BUILD AXIS IS ASKED FIRST, and it is what stopped the retired pen rule from lying.** Gear that
	# speeds a rung's build meter is applicable to any herd with a rung left to climb — which is
	# exactly the climb the handling kit was being withheld from, on the strength of its OTHER axis
	# being pen-only. A kit that can change this source's outcome is offered whatever else it lacks,
	# so the weapon rule below never runs on it either: a crook does not have to bring a deer down to
	# be the right thing to carry while you are gentling one. **The shipped `hurdling` kit is not on
	# a hunt sheet to reach this** — it lists the `builders` and `husbandry` jobs, not `hunt` — so
	# what this arm serves today is a build-capable hunt kit the roster does not currently carry.
	if kit_uses(kits, kit, KIT_BUILD_WORK_KEY) and RungGates.hunt_rung_remains(quarry, prefix):
		return _kit_offered()
	if not SourceForecast.quarry_is_fought(quarry, prefix):
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
		prefix: String, build_branch: String = BUILD_BRANCH_NONE,
		build_rung: String = BUILD_RUNG_ANY) -> bool:
	return bool(kit_offer(kits, kit, job, quarry, prefix, build_branch,
		build_rung)[OFFER_OFFERED_KEY])

## **DOES THIS KIT'S BUILD TOOL SERVE THIS BUILD?** — the fresh-tier trio read as one answer: a worth
## above the neutral, a branch that matches, and — where the tool names one — the RUNG that matches.
## Any part alone is a lie about the shipped roster: the worth alone offers the crook for a garden,
## the branch alone offers a kit whose tool has no contribution to make, and the branch-and-worth pair
## offers the PAVING kit on a road being graded.
##
## ⛔ **THE THREE ARMS, AND THE THIRD IS THE ONE THAT FAILS SILENTLY AND GENEROUSLY.** They are the
## sim's, in the sim's order (`EquipmentEffect::serves_build`):
##
## | the tool names… | the caller names… | answer |
## |---|---|---|
## | no rung | anything | **serves** — every rung on its branch |
## | a rung | that same rung | **serves** |
## | a rung | NO rung | **does not serve** |
##
## The last arm is `false` on purpose. A surface that cannot say which rung is being priced must be
## quoted NOTHING rather than the tool's headline worth: the turn always knows its rung, so an answer
## given to a caller that does not is a promise made where the pairing was lost. Getting it backwards
## reads as a working uplift on every road in the game, which is why it is written as a table here
## rather than as a boolean expression a reader has to run in their head.
##
## **RESOLVED OFF THE ROSTER, never off the band's worn row**, like every other applicability test
## here: which build a kit serves is a property of (kit × roster), and a band whose hoes are spent has
## picked the right kit and worn it out, which is a different sentence from picking the wrong one.
## **DOES THIS KIT'S BUILD TOOL SERVE THIS BRANCH AT ALL — the first two terms of the trio, without
## the third.** It is the question *is this a road tool* as against *is this the road tool for THIS
## rung*, and the two must not be collapsed: `kit_serves_build` asked with no rung is the THIRD ARM,
## which answers `false` for every rung-bound tool by design.
##
## Its one caller is `kit_offer`, choosing which refusal is the true one to print.
static func kit_serves_branch(kits: Array, kit: Dictionary, branch: String) -> bool:
	if branch == BUILD_BRANCH_NONE:
		return false
	return kit_uses(kits, kit, KIT_BUILD_WORK_KEY) \
		and String(kit.get(KIT_BUILD_BRANCH_KEY, BUILD_BRANCH_NONE)) == branch

## ⛔ **`rung` HAS NO DEFAULT, AND THAT IS THE GUARD RATHER THAN AN INCONVENIENCE.** It carried
## `BUILD_RUNG_ANY` as a default and a caller that simply forgot the argument got the THIRD ARM —
## `false` for every rung-bound tool — with no error and no warning anywhere. That shipped twice: the
## build queue HEADER read `1 builders · No kit` over an entry whose own dropdown named the
## Roadbuilding kit the sim was demonstrably using. A missing argument is a GDScript parse error, so a
## caller that cannot name a rung now has to SAY so by passing `BUILD_RUNG_ANY` — which is the
## sentence the third arm answers, stated on purpose instead of arrived at by omission.
##
## `work_kit_for_branch` and `build_kit_for_branch` are required for the same reason, they being the
## two lookups whose answer this predicate decides.
static func kit_serves_build(kits: Array, kit: Dictionary, branch: String,
		rung: String) -> bool:
	if branch == BUILD_BRANCH_NONE:
		return false
	if not kit_uses(kits, kit, KIT_BUILD_WORK_KEY):
		return false
	if String(kit.get(KIT_BUILD_BRANCH_KEY, BUILD_BRANCH_NONE)) != branch:
		return false
	var declared := String(kit.get(KIT_BUILD_RUNG_KEY, BUILD_RUNG_ANY))
	if declared == BUILD_RUNG_ANY:
		return true
	return rung != BUILD_RUNG_ANY and declared == rung

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
## **THE PLANT WEB IS EXCLUDED AND THE PEN IS NOT, by the same `quarry_is_fought` test `kit_offer`
## takes** — a patch states no `durability` for the gate to be `stated` about, but a penned herd
## resolves the ordinary fight (`docs/plan_standing_upkeep.md` §4.9 item 12b). Skipping a pen here is
## what let the ASSIGN HUNTERS sheet reprice a real `per_worker` and a real crew target for a party
## the sim pays nothing, one surface over from the greying `kit_offer` had already stopped doing.
static func hunt_gate_closes(kits: Array, kit: Dictionary, band: Dictionary, quarry: Dictionary,
		prefix: String) -> bool:
	if kit.is_empty() or quarry.is_empty():
		return false
	if not SourceForecast.quarry_is_fought(quarry, prefix):
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

## **THE EFFECTIVE TIER A GIVEN BAND GETS UNDER A GIVEN KIT — READ, NEVER DERIVED.**
##
## One lookup into the band's own `kit_tiers` (see `BAND_KIT_TIERS_KEY` for why that field exists and
## why no client-side rule can stand in for it). `stated` is false when the band publishes no row for
## this kit at all — a band the wire has not described yet — and then the ROSTER's fresh tiers stand
## and the hint prints no condition clause, the same "absent terms render no line" convention
## `hunt_gate_model` takes.
##
## **IT IS KEYED BY THE ROSTER'S OWN AXIS CONSTANTS**, so a tier and the roster entry it came from are
## reachable by ONE name. It used to answer short keys (`"hunt_carry"` / `"forage_carry"`) while the
## roster spelled them in full, and that split shipped a silent bug the moment a caller needed BOTH:
## `_kit_priced_source` read this dict with the short key and `equipped_tier` with the same string,
## which no roster entry carries — so the reference came back `0`, the repricing short-circuited, and
## every kit on every compose sheet quoted identical numbers. Reported from play.
##
## **EVERY AXIS COMES OFF THE ROW, AND THE ONLY FALL-BACK LEFT IS THE WHOLE-ROW ONE.** A per-KEY
## fall-through to the roster used to stand in for `scout_vantage_range` (and, until issue #543
## deleted it, `pen_carry`), which the wire's table did not carry; it does now (see
## `BAND_KIT_TIERS_KEY`), so a fall-through per key would
## be a path no live frame can reach that quietly re-quotes the FRESH tier the moment a row is
## malformed — which is the exact reading this field exists to remove. What must not happen either way
## is a client-side step-down from `kit_item_conditions`: which item supplies which axis is per kit,
## and guessing it is what repriced a band with fresh traps and dry spears to the bare hand.
static func effective_tiers(kits: Array, kit: Dictionary, band: Dictionary) -> Dictionary:
	var resolved := band_kit_tiers(band, String(kit.get(KIT_ID_KEY, "")))
	if resolved.is_empty():
		return {
			KIT_ATTACK_KEY: float(kit.get(KIT_ATTACK_KEY, TIER_ABSENT)),
			KIT_HUNT_CARRY_KEY: float(kit.get(KIT_HUNT_CARRY_KEY, TIER_ABSENT)),
			KIT_FORAGE_CARRY_KEY: float(kit.get(KIT_FORAGE_CARRY_KEY, TIER_ABSENT)),
			"stated": false,
		}
	return {
		KIT_ATTACK_KEY: _row_tier(resolved, KIT_ATTACK_KEY),
		KIT_HUNT_CARRY_KEY: _row_tier(resolved, KIT_HUNT_CARRY_KEY),
		KIT_FORAGE_CARRY_KEY: _row_tier(resolved, KIT_FORAGE_CARRY_KEY),
		"stated": true,
	}

## What an axis reads where nothing states it — a roster entry that predates the axis, or a band row
## that omits it. It is **not** a tier the game ships; it is the under-promise, the same direction
## `condition_of` errs in, and the honest answer for a wire this client cannot read.
const TIER_ABSENT := 0.0

## **ONE AXIS OFF THE BAND'S OWN ROW.** A row states every axis `BandKitTiers` carries, so an absent
## key is a wire this client does not understand rather than a gap to paper over — and it reads
## `TIER_ABSENT` rather than the roster's fresh tier, because quoting a fresh number for gear the
## server never confirmed is the reassuring lie the per-band field was published to end. The
## whole-row absence is a different question and `effective_tiers` / `role_gear` answer it above.
## **It is a read, never a derivation** — nothing here consults `kit_item_conditions`.
static func _row_tier(resolved: Dictionary, axis_key: String) -> float:
	return float(resolved.get(axis_key, TIER_ABSENT))

## **THIS BAND'S RESOLVED ROW FOR ONE KIT**, `{}` when it publishes none. The one reader of
## `BAND_KIT_TIERS_KEY`, so the tiers, the mass window and the two multipliers are all fetched through
## the same lookup and cannot come from two different rows.
##
## **THE MASS BOUNDS AND THE MULTIPLIERS RIDE HERE TOO, and a gate must read them from HERE.** A spent
## item contributes no bound either, so a kit whose mass-bounded weapon has run dry has NO size window
## rather than its fresh one; `KitOption`'s own bounds are the fresh-kit reference only.
static func band_kit_tiers(band: Dictionary, kit_id: String) -> Dictionary:
	if kit_id.is_empty():
		return {}
	for row_variant in band.get(BAND_KIT_TIERS_KEY, []):
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		if String(row.get(BAND_KIT_TIERS_ID_KEY, "")) == kit_id:
			return row
	return {}

## **The remaining condition of one ITEM, by its `equipment.json` id** — `CONDITION_DRY` when the band
## publishes no row for it.
##
## Absent reads as dry deliberately: the caller has already established that the band stated *some*
## condition, so a missing row for one item is a wire the client does not understand — and quoting a
## kitted number for gear the server never confirmed is the failure mode this whole model exists to
## prevent. Erring toward the unequipped tier under-promises instead.
static func condition_of(band: Dictionary, item_id: String) -> float:
	if item_id.is_empty():
		return CONDITION_DRY
	for row in band.get(BAND_ITEM_CONDITIONS_KEY, []):
		if String(row.get(ITEM_CONDITION_ID_KEY, "")) == item_id:
			return float(row.get(ITEM_CONDITION_REMAINING_KEY, CONDITION_DRY))
	return CONDITION_DRY

## **WHAT A BAND-WIDE ROLE ACTUALLY GETS UNDER THIS KIT** — `{axis, tier, stated}`, the role twin of
## `effective_tiers` and it exists BECAUSE that one answers only the four source axes: a Scout's kit
## is priced on `scout_vantage_range`, which is not one of them.
##
## **THE TIER READS THE BAND'S OWN ROW, EXACTLY AS `effective_tiers` DOES** (`_row_tier`), so a
## Warrior card reads the band's sim-resolved `attack` under the warrior kit — clubs, not spears — and
## a Scout card reads its sim-resolved `scout_vantage_range` under the wayfinding kit, 1 tile once
## that gear is dry rather than the roster's fresh 2. **Never a client-side step-down**: the item
## behind an axis is per kit, and guessing it is the defect the per-band field exists to remove.
##
## **The ROSTER's fresh tier stands only where the band states no row for this kit at all** — a band
## the wire has not described yet, which is the same whole-row fall-back `effective_tiers` takes and
## the reason `kits` is unread here without being droppable: this function's twin has the identical
## `(roster, kit, band)` shape and likewise never reads the roster.
##
## `stated` is false when the band publishes no item conditions at all: the card then prints no
## condition clause, rather than a client quoting `dry` at gear the server never described.
##
## `{}` for a job that is not band-wide, or a kit the roster could not resolve.
static func role_gear(kits: Array, kit: Dictionary, band: Dictionary, job: String) -> Dictionary:
	var axis := role_axis(job)
	if axis.is_empty() or kit.is_empty():
		return {}
	var resolved := band_kit_tiers(band, String(kit.get(KIT_ID_KEY, "")))
	var tier := float(kit.get(axis, TIER_ABSENT)) if resolved.is_empty() else _row_tier(resolved, axis)
	return {
		ROLE_GEAR_AXIS_KEY: axis,
		ROLE_GEAR_TIER_KEY: tier,
		ROLE_GEAR_STATED_KEY: not (band.get(BAND_ITEM_CONDITIONS_KEY, []) as Array).is_empty(),
	}

## `role_gear`'s keys. Named rather than spelled at each reader for the reason every dict contract in
## this layer is: a typo in a `get` is a silent zero.
const ROLE_GEAR_AXIS_KEY := "axis"
const ROLE_GEAR_TIER_KEY := "tier"
const ROLE_GEAR_STATED_KEY := "stated"

## **WHAT THIS KIT TAKES OFF A BUILD FOR THIS BAND** — the two halves of the turn estimate's gear
## term, as `SourceForecast.BUILD_GEAR_PER_WORKER` / `BUILD_GEAR_SATURATING_CREW`, so a caller carries
## one object rather than two loose floats it could hand over in the wrong order.
##
## **BOTH COME OFF THE BAND'S OWN RESOLVED ROW, never the roster's fresh tier** — a kit whose tool has
## worn out contributes nothing and holds nobody, and quoting the roster there would promise a build
## that lands sooner than it can. `band_kit_tiers` is the one reader of that row, which is what keeps
## this and the band panel's gear line from coming from two different ones.
##
## **THE BRANCH AND THE RUNG ARE PART OF THE READING, NOT A FILTER ON TOP OF IT.** A row whose
## `build_work_branch` disagrees with the build being priced contributes the neutral `0.0`, and so
## does one whose `build_work_rung` names a DIFFERENT rung of the branch they agree on — exactly as
## the sim's `EquipmentEffect::serves_build` does. So a sheet handed the wrong web's kit quotes an
## ungeared build rather than the crook's `0.5` against a garden, and a road being GRADED quotes an
## ungeared build rather than the paving kit's `2.0`. `BUILD_BRANCH_NONE` asks for no branch test at
## all, which is what a caller with no build in front of it wants.
##
## ⛔ **`BUILD_RUNG_ANY` IS NOT THE BRANCH'S `NONE`, and the asymmetry is deliberate.** A caller that
## names no BRANCH is asking for no test; a caller that names no RUNG has asked a question it could
## not qualify, and a rung-bound row answers it `{}` rather than with a worth the sim will not pay.
## `kit_serves_build` owns the three arms.
##
## `{}` for a kit this band publishes no row for, which `SourceForecast.build_turns_at` reads as the
## ungeared case — the same direction `TIER_ABSENT` errs in, and the honest answer for a wire this
## client cannot read.
static func build_gear(band: Dictionary, kit_id: String,
		build_branch: String = BUILD_BRANCH_NONE,
		build_rung: String = BUILD_RUNG_ANY) -> Dictionary:
	var resolved := band_kit_tiers(band, kit_id)
	if resolved.is_empty():
		return {}
	if build_branch != BUILD_BRANCH_NONE:
		if String(resolved.get(KIT_BUILD_BRANCH_KEY, BUILD_BRANCH_NONE)) != build_branch:
			return {}
		var declared := String(resolved.get(KIT_BUILD_RUNG_KEY, BUILD_RUNG_ANY))
		if declared != BUILD_RUNG_ANY \
				and (build_rung == BUILD_RUNG_ANY or declared != build_rung):
			return {}
	return {
		SourceForecast.BUILD_GEAR_PER_WORKER: float(resolved.get(KIT_BUILD_WORK_KEY, TIER_ABSENT)),
		SourceForecast.BUILD_GEAR_SATURATING_CREW: int(resolved.get(
			KIT_BUILD_SATURATING_CREW_KEY, SourceForecast.BUILD_CREW_NONE)),
	}

## **THE BUILDERS KIT ONE QUEUE ENTRY IMPLIES** — the client's half of `equipment.md`'s "THE
## BUILDERS' KIT IS DERIVED PER QUEUE ENTRY", and the ONE precedence, so the compose sheets' gear
## term and the Builders card's picker cannot answer it differently.
##
## 1. **The kit the band's `builders` row publishes, where its tool serves THIS entry's web.** That
##    row is already the sim's resolved answer — a kit named on the row, else the roster's answer for
##    the HEAD entry's branch — so taking it whenever it fits is how the client reads the answer
##    rather than re-deriving one.
## 2. **A row kit that does not serve the HEAD's web either is a PIN, and it is kept.** The
##    derivation cannot have produced it — it would have answered for the head's own branch — so the
##    player named it, and a named kit wins for every entry in the queue. This is what preserves
##    sending the pool out bare (`none`): with anything queued, a published `none` can only be a
##    deliberate one, and the entry is priced at no gear rather than at the tool the roster would
##    have handed it.
## 3. **Otherwise the roster's own answer for this branch** (`build_kit_for_branch`), which is what
##    the sim will resolve when this entry reaches the head of the queue. Without this step the first
##    Cultivate a player ever declares would be quoted bare-handed — the queue is empty until they
##    commit, so the row publishes `default_kits.builders` (`none`) — and the estimate would jump the
##    moment the decision was already taken.
## 4. **Otherwise the published row anyway**, including `none`: a roster with no kit for this web
##    leaves the pool on whatever it is holding, which is the sim's own fall-back.
##
## ⚠ **ONE CASE REMAINS AMBIGUOUS AND IS QUOTED OPTIMISTICALLY, and the wire is why.** The `builders`
## row publishes the RESOLVED kit and not the stored one, so a player who pinned the kit the HEAD's
## own web would have derived anyway — `hurdling` while a Tame leads the queue — is indistinguishable
## here from one who named nothing. Rule 3 then prices a plant entry at `tillage` where the sim will
## keep the pin and take nothing off it. Closing it needs the stored id (or a "was it named" flag)
## beside the resolved one on the wire; every other combination is resolved above.
## **AND EVERY RULE ABOVE IS ASKED AT THE RUNG BEING WORKED, NOT AT THE ENTRY'S DESTINATION.** A
## `pave` declared on bare ground grades first, so the tool that entry wants TODAY is the mattock;
## quoting the dressing hammer because the player named the top of the ladder would price a job
## nobody is doing yet. The rung a caller asks with is `next_rung_key` off the rung the source HOLDS.
static func builders_kit_for(kits: Array, row_kit_id: String, branch: String,
		head_branch: String = BUILD_BRANCH_NONE, rung: String = BUILD_RUNG_ANY,
		head_rung: String = BUILD_RUNG_ANY) -> String:
	var row_kit := kit_by_id(kits, row_kit_id)
	if not row_kit.is_empty() and kit_serves_build(kits, row_kit, branch, rung):
		return row_kit_id
	if head_branch != BUILD_BRANCH_NONE \
			and build_kit_for_branch(kits, head_branch, head_rung) != NO_KIT_ID \
			and not kit_serves_build(kits, row_kit, head_branch, head_rung):
		return row_kit_id
	var derived := build_kit_for_branch(kits, branch, rung)
	return derived if derived != NO_KIT_ID else row_kit_id

## **THE ROSTER'S OWN ANSWER FOR ONE WEB** — the earliest `builders` entry, in roster order, whose
## build tool serves `branch` at the fresh tier. `NO_KIT_ID` when the roster carries none, which is a
## real answer: nothing on it helps that web build.
##
## **The sim's `EquipmentConfig::build_kit_for_branch`, asked of the published roster** — same order,
## same fresh-tier test, and the same reason it is a lookup rather than a table: ⛔ no `web → kit id`
## match is spelled anywhere, so a third build tool is a roster edit on both halves.
static func build_kit_for_branch(kits: Array, branch: String, rung: String) -> String:
	return work_kit_for_branch(kits, JOB_BUILDERS, branch, rung)

## **THE ROSTER'S ANSWER FOR ONE JOB AND ONE WEB** — the earliest entry offering `job`, in roster
## order, whose build tool serves `branch` at the fresh tier. The sim's `EquipmentConfig::work_kit_for`,
## which is the ONE lookup both its derivations are: the builders' kit and the KEEPING pool's are the
## same question asked of two job lists, so a second walk here could only drift from this one.
static func work_kit_for_branch(kits: Array, job: String, branch: String,
		rung: String) -> String:
	if branch == BUILD_BRANCH_NONE:
		return NO_KIT_ID
	for kit_variant in kits_for_job(kits, job):
		if kit_serves_build(kits, kit_variant, branch, rung):
			return String((kit_variant as Dictionary).get(KIT_ID_KEY, NO_KIT_ID))
	return NO_KIT_ID

## The two KEEPING roles and the web each keeps — `agriculture` keeps ground, `husbandry` keeps
## animals. Spelled here as the file spells `JOB_SCOUT` and `JOB_BUILDERS`, so this layer keeps
## depending on nothing but `SourceForecast` and the leaves below it.
##
## ⛔ **THERE IS NO `roadwork` ROW, AND ITS ABSENCE IS THE FINDING RATHER THAN THE GAP.** The sim does
## gear road upkeep — `RungBranch::Route` maps to `KitJob::Roadwork` and both road kits list that job
## — but it asks per ROAD, at the rung that road STANDS on (`systems::labor`'s keeping claim), and
## this table answers per POOL. A band keeps many roads at many rungs, so there is no one rung for a
## pool-wide lookup to name, and the third arm of `kit_serves_build` would answer `NO_KIT_ID` for
## every rung-bound tool anyway.
##
## **Nothing on this client would read the answer either.** The two keeping surfaces are
## `BandPanelController`'s pool coverage — which takes the ROAD pool's supply, demand and shortfall
## straight off the cohort, because the road rows are fog-filtered and summing them would understate
## a real bill — and the work inspector's upkeep picker, whose job is `agriculture` or `husbandry` by
## construction (a road has no labor row to inspect). Adding a row here would add a derivation with
## no reader and no rung to be right about.
const JOB_AGRICULTURE := "agriculture"
const JOB_HUSBANDRY := "husbandry"
const KEEPING_JOB_BUILD_BRANCHES := {
	JOB_AGRICULTURE: BUILD_BRANCH_PLANT,
	JOB_HUSBANDRY: BUILD_BRANCH_ANIMAL,
}

## **THE KIT A BAND'S KEEPING POOL IS ACTUALLY WORKING WITH** — the client's read of the sim's
## `EquipmentConfig::keeping_kit_for`, which derives the tool off the ROSTER because
## `default_kits.agriculture` / `.husbandry` are both `none` and a pool that waited to be handed a kit
## would keep bare-handed forever.
##
## **THE ROW'S OWN `kitId` IS DELIBERATELY NOT CONSULTED, unlike the builders'.** The sim honours a
## kit NAMED on the keeping row (`named_kit_on`), and the wire publishes that row's kit already
## RESOLVED — so a row nobody has named reads back as the job default `none`, which is
## indistinguishable here from a deliberate bare-handed pin (the trap `builders_kit_for` records one
## rule over). There is no keeping kit picker in this client, so the only way to make that pin is the
## command line; deriving is therefore right for every band a player can produce from the UI, and a
## pin would be quoted one tool too generous rather than silently ignored.
##
## `NO_KIT_ID` for a role that keeps no web, and for a roster carrying no tool that serves it — a real
## answer meaning the pool works bare-handed.
## ⛔ **AND IT ASKS `BUILD_RUNG_ANY` OUT LOUD, which the table two blocks up is the reason for.** The
## two branches this can name are the PLANT and ANIMAL webs, whose kits bind no rung, so every one of
## their tools serves every rung of its branch and the unqualified ask is the right one. It is stated
## rather than defaulted because `work_kit_for_branch` takes no default — a lookup that cannot name a
## rung has to say so, so that the day a ROUTE row joins that table the omission is a decision on the
## page instead of a `NO_KIT_ID` nobody can see.
static func keeping_kit_for(kits: Array, role_kind: String) -> String:
	return work_kit_for_branch(kits, role_kind,
		String(KEEPING_JOB_BUILD_BRANCHES.get(role_kind, BUILD_BRANCH_NONE)), BUILD_RUNG_ANY)

## **THE ROSTER'S BARE ENTRY FOR ONE JOB — the kit that carries nothing.** `kit_supplies_any` is the
## derived reading of "the null kit" (`item_ids` empty), which is why nothing here spells the id
## `none`; a future kit with an empty `uses` is the same thing and gets the same answer.
##
## **IT EXISTS FOR THE ROLE CARD THAT HAS NOTHING TO DERIVE.** `resolve_selection`'s terminal
## fall-through is `selectable[0]`, i.e. ROSTER ORDER — right for a job whose default is always
## resolvable, and wrong for the `builders` row, where "nothing is chosen and nothing can be derived"
## is a real and common state (a band with an empty build queue). Falling to roster order there
## presents `hurdling` as a decision the player never made, and pins the pool to the animal web the
## moment they touch the stepper. The honest face is the bare kit.
##
## `NO_KIT_ID` when the job lists no bare entry at all, which every caller renders as no selection.
static func bare_kit_id(kits: Array, job: String) -> String:
	for kit_variant in kits_for_job(kits, job):
		if not kit_supplies_any(kit_variant as Dictionary):
			return String((kit_variant as Dictionary).get(KIT_ID_KEY, NO_KIT_ID))
	return NO_KIT_ID

## **DOES THIS KIT SUPPLY THIS AXIS AT ALL?** — a kit supplies an axis exactly when its FRESH tier
## there beats the roster's bare-handed one. It answers an APPLICABILITY question only — *"can this
## kit change what this source pays?"* — and it is asked at the fresh tier for that reason.
##
## **IT MUST NEVER BE USED TO NAME AN ITEM.** That was the old hint line's mistake: an axis does not
## identify the component behind it (`big_game` supplies `attack` from `spears`, `trapping` from
## `traps`), and the wire states membership outright now — see `kit_item_ids`.
static func kit_uses(kits: Array, kit: Dictionary, axis_key: String) -> bool:
	var bare := unequipped_tier(kits, axis_key)
	return not is_inf(bare) and float(kit.get(axis_key, 0.0)) > bare

## **THE ITEMS THIS KIT CARRIES, IN CONFIG ORDER** — the wire's own list, `[]` for a kit that carries
## nothing (`none`) and for an entry that predates a roster.
##
## **THIS REPLACED THE `kit_uses(kits, kit, axis_key)` INFERENCE FOR NAMING GEAR**, which asked whether
## the kit's tier on an axis beat the roster's bare-handed one and called that "the kit uses this
## component". Membership is now stated, so it is read rather than deduced — and the deduction could
## not tell `traps` from `spears` in the first place, both being `attack` at the same tier.
static func kit_item_ids(kit: Dictionary) -> Array:
	var items_variant: Variant = kit.get(KIT_ITEM_IDS_KEY, [])
	return items_variant if items_variant is Array else []

## **THE HINT LINE — THE EFFECTIVE TIER, NEVER THE FRESH ONE.** `attack 20.0 · carry 40.0 per hunter ·
## spears 74 · sled 58` on a hunt sheet, `carry 8.0 per gatherer · baskets 61` on a forage one: the
## tiers this band gets, then the condition of each item the kit actually carries, so a band one turn
## from running dry can see it coming. `""` when the kit is unknown.
##
## **THE ITEM CLAUSES ARE THE KIT'S OWN LIST, NOT ONE PER AXIS** (`KIT_ITEM_IDS_KEY`, in config order,
## so the weapon still reads before the haul aid). Per-axis clauses had to name the item from the axis,
## which is a guess: the Trapping kit read `attack 20.0 · carry 40.0 per hunter · spears 100 · sled 100`
## — naming gear it does not carry and quoting the SPEARS' wear, so a band with fresh traps and dry
## spears read exactly backwards. It now reads `traps`, with the traps' own condition.
##
## The number of clauses therefore follows the KIT rather than the job: `big_game` and `trapping` state
## two, `gathering` one, `none` none at all.
##
## ⛔ **THE TIER ARM TOOK THE QUARRY AND FORKED ON A PEN, AND IT DOES NEITHER NOW** (issue #543). The
## dead reading: *"a hunt row works two different things through one verb, and they read disjoint
## axes — a WILD herd is stalked and hauled (`attack` and the sled's carry); a PEN is collected
## (`pen 40.0 per keeper`, **no attack**)"*, with a `quarry` parameter carried the whole way down so
## the arm could be gated on the SOURCE.
##
## **BOTH HALVES OF THAT ARE FALSE NOW.** A pen resolves the ordinary fight (`docs/plan_standing_
## upkeep.md` §4.9 item 12b — containment solves the catching, weapons solve the killing), so the
## weapon clause belongs on a pen; and `EquipmentStat::PenCarry` is deleted, so the haul clause is the
## sled's on a pen exactly as on the range. The arm's own justification — *"at a pen, `pen 12.0 per
## keeper` beside `pen 40.0 per keeper` is the whole visible difference the handling gear buys"* — was
## the hurdles-vs-sled split, which §4.9 item 12 ended by making hurdles a material. **A hunt row now
## states one pair of clauses at every rung**, so the parameter went with the fork.
##
## **`crew` IS THE PARTY BEING COMPOSED, AND IT IS WHAT KEEPS THE TIERS FROM SPEAKING FOR IT.** The
## tiers above describe ONE person; a band holding one spear and composing eight hunters read
## `attack 20.0` while the sim priced seven of the eight bare-handed inside the take curve. So a
## caller that has a stepper hands its value here and the line states the coverage beside the tiers
## — see `_append_coverage` for where the count comes from and what it may not be used for. A caller
## with no party (`KIT_CREW_UNCOMPOSED`) renders exactly as it did before the clause existed.
static func tier_hint(kits: Array, kit: Dictionary, band: Dictionary, job: String,
		crew: int = KIT_CREW_UNCOMPOSED) -> String:
	if kit.is_empty():
		return ""
	# **A BAND-WIDE ROLE READS ONE AXIS AND ITS OWN ITEM**, and it takes a branch of its own rather
	# than a fourth arm below: those arms are keyed by CARRY axis and resolve their conditions through
	# `effective_tiers`, which is job-blind and would price a warrior's `attack` off the spears.
	if is_band_wide_role(job):
		return role_hint(kits, kit, band, job)
	var tiers := effective_tiers(kits, kit, band)
	var parts: Array[String] = []
	if job == JOB_FORAGE:
		parts.append(HudComposeVocab.KIT_HINT_FORAGE_CARRY_FORMAT % _tier_face(
			float(tiers[KIT_FORAGE_CARRY_KEY])))
	else:
		# ⛔ **A PENNED HERD TAKES THIS ARM TOO** (issue #543). A third arm stood above it, keyed by
		# `carry_axis_for(job, quarry) == KIT_PEN_CARRY_KEY`, and it printed ONE clause — `pen 40.0 per
		# keeper` — in place of the two below. The axis is deleted, so a pen is a hunt row like any
		# other: it states the weapon AND the haul, which is strictly more than the pen clause said and
		# is the same haul number the pen was collected at.
		parts.append(HudComposeVocab.KIT_HINT_ATTACK_FORMAT % _tier_face(
			float(tiers[KIT_ATTACK_KEY])))
		parts.append(HudComposeVocab.KIT_HINT_HUNT_CARRY_FORMAT % _tier_face(
			float(tiers[KIT_HUNT_CARRY_KEY])))
	# **AFTER EVERY TIER AND BEFORE EVERY CONDITION**, because it qualifies all of the first group and
	# none of the second: a tier is what one equipped worker gets, a condition is one item's own life.
	_append_coverage(parts, band, kit, crew)
	for item_variant in kit_item_ids(kit):
		_append_condition(parts, band, tiers, String(item_variant))
	return HudComposeVocab.KIT_HINT_SEPARATOR.join(parts)

## **THE BAND-WIDE ROLE CARDS' HINT** — `2-tile sight per vantage · Wayfinding 100`, the effect this
## band's Scout or Warrior actually gets under this kit, then the condition of the gear it carries.
##
## **IT IS THE GEAR POPOVER'S OWN VOCABULARY, DELIBERATELY** (`DetailFormat.KIT_ROLE_*`,
## `kit_item_label`, `kit_condition_face`), rather than this file's compose-sheet wording. That
## popover already renders `▲ Wayfinding 66 — 2-tile sight per vantage` for the same pair on the same
## band, and a card that phrased its own version would give the player two ways of reading one number.
##
## **THE ITEMS ARE THE KIT'S OWN LIST** (`kit_item_ids`), exactly as the compose hint's are, so the
## card names the gear the wire says this kit carries rather than an item guessed from its axis. A
## `none` selection carries nothing and reads as its bare-handed effect alone; the clause is withheld
## entirely until the band has STATED its conditions, rather than a client quoting `dry` at gear the
## server never described.
static func role_hint(kits: Array, kit: Dictionary, band: Dictionary, job: String) -> String:
	var gear := role_gear(kits, kit, band, job)
	if gear.is_empty():
		return ""
	var parts: Array[String] = []
	var phrase := _role_effect_phrase(job, float(gear[ROLE_GEAR_TIER_KEY]))
	if phrase != "":
		parts.append(phrase)
	if bool(gear[ROLE_GEAR_STATED_KEY]):
		for item_variant in kit_item_ids(kit):
			var item_id := String(item_variant)
			if item_id.is_empty():
				continue
			parts.append(HudComposeVocab.KIT_HINT_ROLE_ITEM_FORMAT % [
				DetailFormat.kit_item_label(item_id), DetailFormat.kit_condition_face(band, item_id)])
	return HudComposeVocab.KIT_HINT_SEPARATOR.join(parts)

## What this role's tier BUYS, in words. **A vantage is a DISTANCE and the camp's attack is a small
## whole number**, so each takes the rounding the Gear popover already gives it — the vantage its own
## (the sim reveals in whole tiles), the attack the popover's shared whole-number face — and neither
## may inherit the carries' one decimal.
static func _role_effect_phrase(job: String, tier: float) -> String:
	match job:
		JOB_SCOUT:
			return DetailFormat.KIT_ROLE_SCOUT_VANTAGE_FORMAT % String.num(
				tier, DetailFormat.KIT_VANTAGE_DECIMALS)
		JOB_WARRIOR:
			return DetailFormat.KIT_ROLE_WARRIOR_ATTACK_FORMAT % String.num(
				tier, DetailFormat.KIT_CONDITION_DECIMALS)
	return ""

## One item's condition clause, appended only where there is something true to say: the band has to
## have stated its conditions at all. **The item names ITSELF** — the caller is walking the kit's own
## list, so there is no axis→item key to keep in step with anything, and an item the kit does not
## carry can no longer be reached from here.
## **NO PARTY IS BEING COMPOSED** — a host with no stepper (the WORKFORCE zone's role cards, a sheet
## rendered before a count exists). The coverage clause is a statement about a specific crew, so with
## no crew there is nothing to say and the line is byte-identical to what it was before it existed.
## `-1` and not `0`: an empty crew is a real composed value, and it equips nobody.
const KIT_CREW_UNCOMPOSED := -1

## **HOW MANY OF THE COMPOSED CREW THIS KIT REACHES** — appended as `3 of 8 equipped`, or not at all.
##
## > #### ⛔ IT COUNTS UNITS, AND THAT IS THE ONE THING THE WIRE CANNOT ANSWER FOR A PRE-COMMIT SHEET
## >
## > `KitItemCondition.workersHolding` is the sim's own people-count and is the right reading
## > everywhere it applies — but it is quoted against `workersOnQuotedJob`, which is
## > `allocation.workers_on_job(...)`, and that is **0** on a sheet where nobody is assigned yet. Both
## > published counts are therefore silent about the party the player is building. What is left is
## > `DetailFormat.kit_units_owned`, and counting units against a composed crew is exact for every
## > item the roster ships (`workers_per_unit` defaults to 1 and no shipped item overrides it).
## > **Nothing about the FIGHT is re-derived here** — the attack tier stays the sim's, unblended.
##
## **THE COUNT IS THE KIT'S, NOT ONE AXIS'S**, and it is the `min` across the items the kit carries.
## This file may not map an axis to the component behind it — `big_game` takes its attack from
## `spears` and `trapping` from `traps`, and guessing that is what once printed the spears' condition
## on a trap party's row — so *"who gets what this line quotes"* is the only answerable form of the
## question, and a worker the kit does not fully reach does not get it.
##
## Silent in three states, each for its own reason:
## - **NO PARTY** (`KIT_CREW_UNCOMPOSED`) or an empty one — nothing to be a fraction of.
## - **A KIT THAT CARRIES NOTHING** (`none`): its tier IS the bare-handed one, which every worker
##   gets, so `0 of 8 equipped` would report a shortfall against gear the kit never claimed.
## - **A BAND THAT STATES NO COUNT** for one of the items — the same withholding `_append_condition`
##   takes over an unstated condition, rather than a client quoting `0` at a ledger it has not read.
static func _append_coverage(parts: Array[String], band: Dictionary, kit: Dictionary,
		crew: int) -> void:
	if crew <= 0:
		return
	var items := kit_item_ids(kit)
	if items.is_empty():
		return
	var reached := crew
	for item_variant in items:
		var owned := DetailFormat.kit_units_owned(band, String(item_variant))
		if owned == DetailFormat.KIT_UNITS_UNSTATED:
			return
		reached = mini(reached, owned)
	parts.append(HudComposeVocab.KIT_HINT_COVERAGE_FORMAT % [reached, crew])

static func _append_condition(parts: Array[String], band: Dictionary, tiers: Dictionary,
		item_id: String) -> void:
	if not bool(tiers.get("stated", false)) or item_id.is_empty():
		return
	var condition := condition_of(band, item_id)
	if condition <= CONDITION_DRY:
		parts.append(HudComposeVocab.KIT_HINT_DRY_FORMAT % item_id)
	else:
		parts.append(HudComposeVocab.KIT_HINT_CONDITION_FORMAT % [item_id, int(condition)])

static func _tier_face(value: float) -> String:
	return String.num(value, HudComposeVocab.KIT_TIER_DECIMALS)

# **THE HONESTY RULE IS GONE, AND SO IS EVERY FUNCTION THAT SERVED IT.**
#
# `estimates_quoted_kit` / `estimates_apply_to` / `estimates_quoted_note` and the two
# `HERD_*_ESTIMATES_KIT_KEY` wire keys existed because the pre-launch tables were computed at ONE kit
# — the hunt job's default — over a FRESH component set, so a sheet composing anything else had to
# refuse to present them and say whose numbers it was withholding. The sim is ASKED now
# (`ForecastQuery`) and answers the exact kit and wear the sheet composed, so there is no other kit's
# raid to disown. A sheet's numbers are always its own.

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
## **`key_text` IS THE FIELD KEY, AND `""` MEANS THE HOST HAS ALREADY NAMED THE ROW.** Every compose
## sheet takes the default and gets the family's declared-width `Kit` label. The WORKFORCE zone's role
## CARDS pass `""`: the card is already headed `Scout` / `Warrior`, and its ~175px width cannot spend
## `COMPOSE_FIELD_KEY_WIDTH` (64) on a third word — measured, the key leaves ~109px for the control
## and `🧭 Wayfinding kit` clips inside it. With no key the picker is the row's only child and takes
## the card.
##
## **`compact_chrome` SQUEEZES THE CONTROL INTO A ZONE CARD** (`HudWidgets.compact` — the WORK zone's
## own row type size and padding, the trim the board's steppers and chips take). It buys BOTH of the
## things a ~137px role card is short of, and both were measured on a rendered frame:
##
## - **HEIGHT.** The ghost stylebox pads 9px top and bottom, so an untrimmed picker is ~42px — a fifth
##   of the card, and enough to tip the band flank's two columns past `band_panel_preview`'s levelness
##   floor.
## - **WIDTH, which is the half a padding trim alone does not fix.** `clip_text` is on, so at the
##   default type size `🧭 Wayfinding kit` came back as **`🧭 Wayfinding ki`** in both a 380px side dock
##   and a two-column horizontal one — the face naming a kit whose name it had eaten the end of.
##
## A compose sheet passes `false` and is byte-identical: it is a free-standing form with a whole
## column to spend, and the family's 42px height is what makes its `Band:` / `Kit` / `Quarry` rows
## line up.
##
## Returns `null` when the job offers no kit at all, so a sheet whose verb the roster does not cover
## renders exactly as it did before the picker existed.
## **`crew` IS THE PARTY THE SHEET IS COMPOSING** — the stepper's own value, passed straight to the
## HINT so the line can state how far the band's gear reaches into it. `KIT_CREW_UNCOMPOSED` (the
## default) is a host with no stepper, and renders exactly as it did before the clause existed.
static func build_kit_row(kits: Array, job: String, selected_id: String, default_id: String,
		band: Dictionary, on_pick: Callable, quarry: Dictionary = {},
		prefix: String = "", key_text: String = HudComposeVocab.COMPOSE_FIELD_KIT,
		compact_chrome: bool = false, crew: int = KIT_CREW_UNCOMPOSED) -> VBoxContainer:
	var offered := kits_for_job(kits, job)
	if offered.is_empty():
		return null
	var selected := kit_by_id(offered, selected_id)
	var block := VBoxContainer.new()
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
	if key_text != "":
		row.add_child(HudWidgets.build_field_key(key_text))
	var glyph := String(HudComposeVocab.KIT_JOB_GLYPHS.get(job,
		HudComposeVocab.KIT_JOB_GLYPH_FALLBACK))
	# **THE MARK FOLLOWS THE ID THE SHEET ACTUALLY OPENED ON** — `default_kit_for`, the same
	# precedence `resolve_selection` used. Tagging the JOB's default while the sheet opens on the
	# HERD's would have the picker contradict itself on every small-game herd: Trapping selected,
	# `(default)` printed on Stalking.
	var effective_default := default_kit_for(job, quarry, default_id)
	var listing := kit_entries(kits, job, selected_id, effective_default, on_pick, quarry, prefix)
	var entries: Array = listing[KIT_ENTRIES_KEY]
	var selected_index := int(listing[KIT_ENTRIES_SELECTED_KEY])
	# The face carries the JOB GLYPH and no default suffix, which is why it is stated separately from
	# the list: the glyph says what this crew is walking out to do (one per sheet, so repeating it down
	# every row would be noise), and `(default)` is a note about an entry rather than about the choice.
	# **THE MARK IS ART WHERE THE JOB HAS ANY** (issue #249), the glyph where it does not. It rides
	# the `OptionButton`'s own `icon` PROPERTY — free on this widget, the dropdown chevron being the
	# separate `arrow` THEME item — and the face then carries the kit's name alone, art OR glyph and
	# never both. UNTINTED: `apply_option_button` sets no `icon_*_color` and the stock theme's
	# resolves to opaque white, so the mark renders in its authored two-tone fill.
	var job_sprite := HudSprites.for_mark(String(HudComposeVocab.KIT_JOB_MARKS.get(job,
		HudComposeVocab.KIT_JOB_MARK_FALLBACK)))
	var face := HudComposeVocab.KIT_PICKER_FACE_FORMAT % [glyph, kit_display_name(selected)]
	if job_sprite != null:
		face = HudComposeVocab.KIT_PICKER_FACE_FORMAT_SPRITE % kit_display_name(selected)
	var picker := HudWidgets.build_option_picker(entries, selected_index, face,
		HudComposeVocab.KIT_PICKER_TOOLTIP)
	if job_sprite != null:
		picker.icon = job_sprite
		picker.add_theme_constant_override("icon_max_width",
			HudComposeVocab.KIT_PICKER_ICON_MAX_WIDTH)
	picker.set_meta(KIT_PICKER_META, true)
	if compact_chrome:
		HudWidgets.compact(picker, HudWorkVocab.WORK_STEPPER_FONT_SIZE,
			HudWorkVocab.WORK_STEPPER_PADDING_V)
	row.add_child(picker)
	block.add_child(row)
	var hint_text := tier_hint(kits, selected, band, job, crew)
	if hint_text != "":
		var hint := HudWidgets.alloc_hint_label(hint_text)
		hint.set_meta(KIT_HINT_META, true)
		block.add_child(hint)
	return block

const KIT_ENTRIES_KEY := "entries"
const KIT_ENTRIES_SELECTED_KEY := "selected"

## **THE KIT LIST ITSELF — `build_option_picker` entries plus the index to open on**, lifted out of
## `build_kit_row` so a host that cannot use that row's LAYOUT still gets its SEMANTICS: the roster
## order, the `(default)` mark, the greying of a kit that cannot change this job's outcome, and the
## reason on the greyed entry's own face.
##
## **ITS SECOND CALLER IS THE BUILD QUEUE ROW'S SETTINGS STRIP** (`docs/plan_standing_upkeep.md`
## §4.7a ②), which needs a FIXED-HEIGHT control: `build_kit_row` returns a two-child block whose
## second child (`tier_hint`) appears only sometimes, and this zone reserves its heights before it
## draws them. Sharing the list rather than the row is what keeps one answer to *which kits, marked
## how* while the two hosts differ in what they can spend on chrome.
##
## `default_id` here is the EFFECTIVE default — already resolved by the caller — because the two hosts
## resolve it differently: a hunt sheet's is the quarry's, and a queue entry's is the roster's answer
## for that entry's own food web.
static func kit_entries(kits: Array, job: String, selected_id: String, default_id: String,
		on_pick: Callable, quarry: Dictionary = {}, prefix: String = "",
		build_branch: String = BUILD_BRANCH_NONE,
		build_rung: String = BUILD_RUNG_ANY) -> Dictionary:
	var entries: Array = []
	# **THE SELECTION IS AN INDEX, because an `OptionButton` marks the current entry itself.** The
	# roster order IS the list order (this layer sorts nothing), so the index of the resolved kit is
	# the whole of what the control needs to open on it and to draw its radio dot.
	var selected_index := HudWidgets.NO_ENTRY_SELECTED
	for kit_variant in kits_for_job(kits, job):
		var kit: Dictionary = kit_variant
		var kit_id := String(kit.get(KIT_ID_KEY, ""))
		var label := kit_display_name(kit)
		if kit_id == default_id:
			label += HudComposeVocab.KIT_DEFAULT_ENTRY_SUFFIX
		if kit_id == selected_id:
			selected_index = entries.size()
		var offer := kit_offer(kits, kit, job, quarry, prefix, build_branch, build_rung)
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
	return {KIT_ENTRIES_KEY: entries, KIT_ENTRIES_SELECTED_KEY: selected_index}
