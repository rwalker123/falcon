class_name DetailFormat

## THE SHARED DETAIL-RENDER LAYER (docs/plan_hud_decomposition.md).
##
## WHAT THIS IS. Everything that turns a list of `"Key: value"` detail LINES into the BBCode the HUD's
## detail surfaces actually show — the renderer (`detail_bbcode`), the per-row key→tint registry it
## consults, and the ~20 label / `*_value_hex` leaves those tints and the line PRODUCERS share. Plus
## the pure band-dict arithmetic behind the Food row (`band_net_food` and friends) and behind the Band
## panel's food-outlook chart, which reads the same family (`band_provisions` = the larder the
## projection starts from, `merged_arrival_schedule` = every source's arrivals summed slot-by-slot).
##
## WHY IT IS ITS OWN FILE. Four clusters render detail rows through one formatter — the selection
## card's land drawer, its occupant drawer, the Band/City panel's vitals label and the disclosure
## popover — and the coming `BandPanelController` split would otherwise have to carry the formatter
## with it or inject it as a Callable. Same measurement that produced `SourceForecast` / `HudWidgets`.
##
## EVERYTHING HERE IS `static`, STATELESS AND PURE — no node, no `_hud` back-ref, no snapshot cache.
## The two pieces of HUD state the formatter used to reach sideways for are threaded as EXPLICIT
## PARAMETERS instead:
##   * the per-render TINT CONTEXT (`Context` below) — the selected band's food runway / morale /
##     fertility plus the disclosure carets. These were `HudLayer` members written by the line
##     producers, reset by four different hosts and read ONLY here; a value passed down cannot be
##     stale, and cannot be reset in the wrong order.
##   * `world_herds` for the Attack/Defense reference bars (`append_danger_component_lines`), the same
##     thread-it-in treatment `SourceForecast` gave the grid-wrap pair.
##
## CONSTS. The rule is: a const lives HERE iff every one of its readers moved here. The herd-drawer
## vocabulary came with `herd_summary_lines` (the pen/husbandry/range/size rows, `OVERGRAZING_WARNING`,
## `FULLY_HERDED`, `CORRAL_PROGRESS_COMPLETE`, `PEN_STARVING_LABEL`) and the expedition
## delivery/tooltip vocabulary with the tooltip trio, plus the recovery-guidance PAIR (the tint
## registry matches the glyph, the producer emits the text — splitting them across files would put a
## one-string invariant in two). The rest of that vocabulary now lives HERE too (the morale-breakdown
## indent + sign glyphs, `MORALE_CAUSE_*`, `CORRAL_GLYPH`, the `FOOD_LABEL_*` table), except the
## `Food`/`Morale` row keys, which live in `HudDisclosureVocab` (`DETAIL_ROW_*`) — each read as
## `Module.X`, so there is exactly one place each phrase is typed.
##
## The one thing this module deliberately does NOT own is the POPOVER those disclosure carets open:
## that half needs a Node to `add_child` into, so it lives in `DisclosureController`. The two are
## bidirectionally coupled by the `[url]` meta this file emits and that one parses — split by node
## ownership, not by "formatter vs popover".

# ---- Detail-row carets (the disclosure affordance this file RENDERS; the popover it opens is
# `DisclosureController`'s). The meta PREFIX (`BREAKDOWN_TOGGLE_META_PREFIX`) lives in
# `HudDisclosureVocab` — both modules and both harnesses read it, so it is shared vocabulary rather
# than either half's own.
# ---- Consts absorbed from HudLayer (const/vocabulary extraction) ----
const FOOD_ACTION_FORAGE := "forage"

const FOOD_ACTION_HUNT := "hunt"

# Per-cohort morale cause (snapshot PopulationCohortState.moraleCause; 0 = None).
const MORALE_CAUSE_NONE := 0

const MORALE_CAUSE_TERRAIN := 1

const MORALE_CAUSE_COLD := 2

const MORALE_CAUSE_UNREST := 3

# Plain-language cause labels, shared by the drawer morale line and the alert reason.
# Cold reads "harsh climate" because the server penalty fires on hot OR cold deviation.
const MORALE_CAUSE_LABEL_TERRAIN := "harsh terrain"

const MORALE_CAUSE_LABEL_COLD := "harsh climate"

const MORALE_CAUSE_LABEL_UNREST := "unrest"

# |morale_delta| below this (0.5%/turn) reads as flat (no arrow), so trivial drift — nearly every tile
# bleeds a hair today — isn't shown as a decline. (The ▲/▼ ARROWS are `BandDetailLines`', the only
# thing that draws them.)
const MORALE_TREND_EPSILON := 0.005

# Itemized morale breakdown — the four signed Layer-1 contributions (their sum IS
# morale_delta) rendered as indented sub-lines under the Morale headline when morale is
# concerning or declining. Tinted by sign (▲ positive = healthy, ▼ negative = amber).
const MORALE_BREAKDOWN_INDENT := "    "

# (The two CONTRIBUTION LABELS `settling`/`culture` are `BandDetailLines`', and the recovery-guidance
# pair `DetailFormat`'s — each moved with every one of its readers.)
const MORALE_CONTRIB_POSITIVE_GLYPH := "▲"

const MORALE_CONTRIB_NEGATIVE_GLYPH := "▼"

# Positive-lever morale hints on the action buttons (tooltip suffixes).
const MORALE_HINT_SCOUT := "Scout unknown ground — reveals nearby tiles and lifts the band's spirits (+morale)."

const MORALE_HINT_PERSISTENT := "  Hunting a herd also lifts morale each turn (+morale/turn)."

const CORRAL_GLYPH := "🐄"

# ---- The four LADDER RUNG glyphs, glyph-ONLY -----------------------------------------------------
# The rung BADGES below (`cultivation_built_label` / `field_built_label` / `corral_built_label`) weld
# the glyph to its
# words — "🌾 Tended Patch" — which a one-glyph column cannot take. These are the same marks with the
# words stripped, so a mark column reads the glyph and a detail row reads the label WITHOUT either
# slicing the other's string. One home per glyph, and every one of them is REUSED, never minted here:
#   plants:  wild → 🌾 Tended Patch → ▦ Field       animals: wild → ◎ pastoral → 🐄 penned
# The two FIELD/PASTORAL marks come from `FoodIcons.POLICY_ICONS` — the ladder's own table, where each
# verb wears the glyph of THE RUNG IT BUILDS, so `sow`'s ▦ IS the Field's mark and `tame`'s ◎ IS the
# pastoral herd's. **The animal side has no rung glyph of its own and must borrow**:
# `husbandry_built_label` (Domesticated) and `corral_built_label` (Corralled) BOTH wear 🐄, so reusing
# it for the pastoral rung would
# make pastoral and penned indistinguishable — the one distinction a rung mark exists to draw.
const CULTIVATION_GLYPH := "🌾"

static func field_glyph() -> String:
    return FoodIcons.for_policy(HudConst.LABOR_POLICY_SOW)

static func pastoral_glyph() -> String:
    return FoodIcons.for_policy(HudConst.LABOR_POLICY_TAME)

# ---- Band/City panel identity grid ---------------------------------------------------------------
# The panel's own header already states the band's name + settlement stage, so the summary rows there
# drop the `Unit: <name>` row (a THIRD copy of the same name) and replace `Size: <n>` (population
# under another name) with the labor line — same numbers, one row, in the identity grid where they
# belong. The Occupants-card drawer (FOREIGN bands, and the no-panel ui_preview fallback) keeps
# Unit/Size: it has no panel header naming the band, and a foreign band exposes no worker breakdown.
# The population/workers LINE is gone from the summary entirely: the band zone's People and
# Workforce bars state the same numbers as two readable charts, and a text restatement above them
# was the third telling of one fact.
# Category breakdown rows under Food reuse the morale breakdown's indent + ▲/▼ glyphs, so they flow
# through the SAME `DetailFormat.detail_bbcode` indented-sub-line path (sign-tinted: ▲ income green, ▼
# eaten amber) — no inline color tags, which mis-layout between the KV table segments.
const FOOD_LABEL_GATHERED := "Gathered"

const FOOD_LABEL_HUNTED := "Hunted"

# THE CONSUMPTION DEBIT — what the band's PEOPLE ate. It has no animal counterpart: the `🐄 Pen feed
# (animals)` row that stood beside it is retired with `penFeedUpkeep`, because human food is not
# animal feed. A pen eats the grass its fenced footprint grows and the hay its keeper carries in, and
# what those two leave uncovered starves the herd (`pen_fed_fraction` < 1) instead of draining the
# people's larder — so a pen has nothing to report on THIS ledger at all. The bare word is therefore
# unambiguous again; the "(people)" qualifier only ever existed to contrast with the animals' row.
const FOOD_LABEL_EATEN := "Eaten"

# The RAID debit (Predators Phase 3): food this band lost to predator raids this turn — the ledger's
# only debit beyond consumption. The sim answers it as `PopulationCohortState.raidForfeit` (the
# client never re-derives it), and it is the third term of the larder identity
# `larder_delta == income − consumption − raid_forfeit`. Crossed-swords glyph so the row
# reads as a loss to an attacker, matching the command feed's `predator_raid` alert.
const RAID_GLYPH := "⚔"
const FOOD_LABEL_RAID_FORFEIT := "%s Lost to raids" % RAID_GLYPH

# The TRANSFER pair (arc #527): food that crossed between bands, in or out. They are the fifth and
# sixth terms of the larder identity
#   larder_delta == income − consumption − raid_forfeit + received − sent
# and they close a hole that was NEVER about trade alone: `balance_supply_networks` has been pooling
# food between neighbouring larders every turn since turn one, so any two co-networked bands had a
# Food line that silently did not add up — by the whole transfer, not a rounding drift.
#
# TWO ROWS, NOT ONE SIGNED ONE, matching the debit rows above and the wire's own shape: a band
# that both sends and receives in one window is doing something, and a net would render that as
# nothing having happened. The received row is an INCOME row (▲ green) and the sent row a DEBIT
# (▼ amber), which the shared `food_breakdown_row` decides from the sign it is handed.
#
# **A PLAIN ARROW PAIR, NOT A HANDSHAKE OR A CART**, for two reasons. What these rows report is
# neighbours pooling as often as it is a shipment arriving, so a trade glyph would promise a deal the
# supply network never made. And the emoji that says "deal" (🤝) is not in this client's fallback
# font: it renders as an INVISIBLE gap — no tofu box, just a wider indent — which is the silent
# failure mode `Typography.gd` is retired for. ⇄ is in the Arrows block the ▸/◀/▲▼ carets already
# come from, so it draws everywhere they do. **One glyph for both rows**, unlike Eaten and Lost to
# raids: these two are ONE fact in two directions, and the row's own words say which way.
const TRANSFER_GLYPH := "⇄"
const FOOD_LABEL_TRANSFER_RECEIVED := "%s From other bands" % TRANSFER_GLYPH
const FOOD_LABEL_TRANSFER_SENT := "%s To other bands" % TRANSFER_GLYPH

# ---- THE FODDER LEDGER'S TWO FLOWS, the labels of the `Fodder:` row's own breakdown. The larder has
# exactly two: what the band's fodder Fields GREW this turn (`fodder_income`) and what its pens ATE
# (`fodder_need`, the sim's sum over the pens it keeps). They are named for the THING at each end —
# the harvest and the animals — because the row above states neither and the popover has to answer
# *why is this draining*.
#
# **`Pens`, not `Fed to pens` or `Eaten`**: the food ledger's `Eaten` is the PEOPLE's, and a fodder
# larder that also said "Eaten" would read as the same account twice. One noun per debit.
const FODDER_LABEL_GROWN := "Grown"
const FODDER_LABEL_PENS := "Pens"

# ---- THE THREE KITS (`docs/plan_hunt_through_combat.md` §4.8) ------------------------------------
# ONE KIT, ONE JOB: spears raise a hunter's `attack`, a SLED is the HUNT's carry (a carcass is one
# lumpy object you drag out whole), BASKETS are the FORAGE web's (berries are loose and bounded by
# what you can hold). The three names are typed once, here, because the Kit ROW lists them and the Kit
# BREAKDOWN explains them — and the pairing of a kit with its role is the whole readout: a sled line
# quoting the forage carry, or a basket line quoting the hunt's, is exactly the defect slice 5
# corrected in the sim and the one this client must not reintroduce.
const KIT_LABEL_SPEARS := "Spears"
const KIT_LABEL_SLED := "Sled"
const KIT_LABEL_BASKETS := "Baskets"

# The item ids the labels belong to, and the list the conditions arrive in. The wire carries ONE ROW
# PER ITEM (`{item_id, remaining}`) rather than three fixed floats, because the item table is server
# config — a fixed field set could not have carried the trapping kit's `traps`. A cohort from a
# snapshot that predates the TOE carries an empty list, which is what the Kit row's presence gate
# reads.
const KIT_ITEM_CONDITIONS_KEY := "kit_item_conditions"
const KIT_ITEM_ID_KEY := "item_id"
const KIT_ITEM_REMAINING_KEY := "remaining"

# **UNITS THE BAND OWNS** (`KitItemCondition.count`) — the ownership statement, so nothing has to
# infer it from a condition of zero: a worn-out item and one the band never had both read
# `remaining 0`, and `count > 0` is what separates them.
#
# **IT IS UNITS, AND `KIT_ITEM_WORKERS_HOLDING_KEY` BESIDE IT IS PEOPLE.** The two are the same
# number only while every item is held by one person, which is true of the whole shipped roster and
# is not a rule — see `kit_units_owned` for the one place that distinction is deliberately spent.
const KIT_ITEM_COUNT_KEY := "count"

# **THE BAND HAS NOT STATED HOW MANY IT OWNS** — an item with no published row, or a row from a
# fixture that predates the field. Distinct from `0`, which is the real and sharp answer *it owns
# none*, and every reader must withhold rather than render on it.
const KIT_UNITS_UNSTATED := -1

## **UNITS OF ONE ITEM THE BAND OWNS**, or `KIT_UNITS_UNSTATED` where it states none.
##
## > #### ⛔ THIS IS UNITS. `kit_workers_holding` IS PEOPLE, AND THEY ARE NOT INTERCHANGEABLE
## >
## > A unit arms `workers_per_unit` people — a per-item config number the wire does not carry — and a
## > unit needs its FULL crew or it is not used at all. So this may never stand in for
## > `workers_holding`, which is the sim's own answer for a STAFFED job and is what every
## > coverage readout on a worked row reads.
##
## **The one thing it may answer is a PRE-COMMIT question**, which the sim's people-counts cannot:
## `workers_on_quoted_job` is the allocation's head count, so on a compose sheet where nobody is
## assigned yet both published counts are `0` and say nothing about the crew being composed. Counting
## UNITS against that composed crew is the only honest reading left, and it is exact for every item
## the game ships (`workers_per_unit` defaults to 1 and no shipped item overrides it).
static func kit_units_owned(band: Dictionary, item_id: String) -> int:
    if item_id.is_empty():
        return KIT_UNITS_UNSTATED
    for row in band.get(KIT_ITEM_CONDITIONS_KEY, []):
        if String(row.get(KIT_ITEM_ID_KEY, "")) != item_id:
            continue
        if not (row as Dictionary).has(KIT_ITEM_COUNT_KEY):
            return KIT_UNITS_UNSTATED
        return maxi(int(row[KIT_ITEM_COUNT_KEY]), 0)
    return KIT_UNITS_UNSTATED

# **HOW MANY PEOPLE AN ITEM ACTUALLY REACHES** (issue #520) — the sim's own answer, resolved through
# the same `coverage` seam the take runs through. A unit ARMS A PERSON, so owning one is not arming
# the band: `count` is UNITS and this is PEOPLE, and the two part company the moment the band is
# short of an item (or holds the spawn's reserve above its head count).
#
# **IT CANNOT BE COMPUTED HERE** and must never be inferred from `count` — `workers_per_unit` is a
# per-item config number the wire does not carry (a four-worker net is the first item that is not
# `1`, and a unit needs its FULL crew), and which job is staffed is sim-side too.
const KIT_ITEM_WORKERS_HOLDING_KEY := "workers_holding"

# **ITS DENOMINATOR, AND THE PAIR IS ONE SENTENCE** — *"`workers_holding` of `workers_on_quoted_job`"*.
# The head count of the job the row is quoted at, off the SAME coverage the numerator came from, so
# the two can never describe different jobs. It is what lets a BASKET, a CLUB or a WAYFINDING
# shortfall be stated at all: before it, `Σ hunt_crews.workers` was the only job head count on the
# wire and the other three jobs were silent.
#
# **A ZERO HERE IS "NOBODY IS STAFFED", NOT A SHORTFALL** — see `kit_coverage`, which is the one place
# this is read and the one place the guard lives.
const KIT_ITEM_ON_QUOTED_JOB_KEY := "workers_on_quoted_job"

# **THE HUNT PARTY'S OWN DIVISION** (`PopulationCohortState.huntCrews`) — one row per run of hunters
# holding identical gear, best-equipped FIRST, `workers` summing to the band's hunt head count.
#
# **A UNIFORM BAND PUBLISHES EXACTLY ONE ROW, never an empty list**, so no reader has to tell "no
# crews" from "one crew holding nothing" — and a band with nobody on the hunt job publishes one row
# at `workers 0`, which is the state every reader here has to treat as *nothing to say* rather than
# as a shortfall of zero out of zero.
#
# Each row's `hunter_attack` is that run's own FLAT tier — the gate's left-hand side for those
# workers, and the reason a band-level `hunter_attack` states only the reassuring half.
const HUNT_CREWS_KEY := "hunt_crews"
const HUNT_CREW_WORKERS_KEY := "workers"
const HUNT_CREW_ATTACK_KEY := "hunter_attack"
const HUNT_CREW_ITEM_IDS_KEY := "item_ids"

# **WHICH KIT THE COHORT'S HUNT ANSWERS ARE QUOTED AT** (`PopulationCohortState.kitId`) — the hunt
# JOB's default on a resident band, an in-flight party's own kit. It is what `hunt_crews` divides the
# party against, so a readout quoting the crews under a DIFFERENT kit describes a division that does
# not exist for that choice. Named here, distinct from `KitRoster.KIT_ID_KEY` (a ROSTER entry's own
# id), because the two are different questions that happen to share a spelling on the wire.
const BAND_QUOTED_KIT_ID_KEY := "kit_id"

# Below this many workers a crew (or a coverage) is treated as EMPTY rather than as a fraction of a
# person. The wire counts workers in fractions because a forecast does, so a band with nobody on the
# hunt publishes `workers 0` and a rounding artefact must not read as somebody standing there.
const HUNT_CREW_WORKER_EPSILON := 0.005
const KIT_DURABILITY_KEY_SPEARS := "spears"
const KIT_DURABILITY_KEY_SLED := "sled"
const KIT_DURABILITY_KEY_BASKETS := "baskets"
const KIT_DURABILITY_KEY_TRAPS := "traps"
const KIT_LABEL_TRAPS := "Traps"

# The three items the expanded roster added, and **they have breakdown rows of their own now**. They
# were label-only for exactly one reason — the popover pairs each item with the resolved tier it
# sets, and the cohort published `hunterAttack` / `huntCarry…` / `forageCarry…` and nothing for a pen
# keeper, a scout's vantage or a warrior — so a row could only have quoted a number the sim never
# sent. The wire carries all three now (see `KIT_TIER_KEY_PEN_CARRY` and its two neighbours), so the
# player can finally see a scout kit and a warrior kit dying instead of only its consequences.
## **THE HANDLING GEAR IS `hurdles` NOW, ITEM ID AND LABEL BOTH.** It was `husbandry_gear` — a name
## that described the KIT it happened to sit in rather than the object — and the object is portable
## fence panels you work a beast into, the same thing whether you are raising the pen or butchering
## in it. The client's own label followed: naming it *Handling gear* while the roster called it
## *Hurdles* left the popover row and the picker's hint disagreeing about one item.
const KIT_LABEL_HURDLES := "Hurdles"
const KIT_LABEL_WAYFINDING := "Wayfinding"
const KIT_LABEL_CLUBS := "Clubs"
const KIT_DURABILITY_KEY_HURDLES := "hurdles"
const KIT_DURABILITY_KEY_WAYFINDING := "wayfinding"
const KIT_DURABILITY_KEY_CLUBS := "clubs"

## **THE PLANT WEB'S BUILD TOOL** — a bone blade hafted with fibre, and the second item in the game to
## declare `build_work`. It carries the `tillage` kit and nothing else, so a band that wants both
## webs' tools holds both items.
##
## **IT HAS A LABEL BUT NO BREAKDOWN ROW OF ITS OWN**, and that is the popover's standing rule rather
## than an omission: a row pairs an item with the resolved tier it sets, and the build axis has no
## flat per-band field to quote — it rides the `kit_tiers` row of whichever kit is selected, which is
## what the Builders card's own gear line states. The hoes still NAME themselves wherever the band's
## items are listed, which is the `Gear` summary row and the picker's condition clauses.
const KIT_LABEL_HOES := "Hoes"
const KIT_DURABILITY_KEY_HOES := "hoes"

# What a trap line is FOR, on the disclosure row. It sets no tier the cohort publishes — reach and
# stand-off are properties of the kit, not of the band — so unlike the other three this row states
# its role in words rather than quoting a resolved number.
const KIT_ROLE_TRAPS := "reach on small game, no risk to the trapper"

# Item id → the label the two kit surfaces print. **An id with no entry falls back to the id itself**
# rather than being skipped: the item table is server config, so a client build can legitimately be
# older than the roster it is handed, and showing `bows 62` is honest where showing nothing would
# hide a whole item the band is carrying.
const KIT_ITEM_LABELS := {
    "spears": KIT_LABEL_SPEARS,
    "sled": KIT_LABEL_SLED,
    "baskets": KIT_LABEL_BASKETS,
    "traps": KIT_LABEL_TRAPS,
    KIT_DURABILITY_KEY_HURDLES: KIT_LABEL_HURDLES,
    KIT_DURABILITY_KEY_HOES: KIT_LABEL_HOES,
    "wayfinding": KIT_LABEL_WAYFINDING,
    "clubs": KIT_LABEL_CLUBS,
}

## The display label for an item id — the id itself when this build has no name for it.
static func kit_item_label(item_id: String) -> String:
    return String(KIT_ITEM_LABELS.get(item_id, item_id))

# The RESOLVED tiers each kit sets. **`hunt_carry_per_worker_biomass` and
# `forage_carry_per_worker_biomass` ARE NOT TWO READINGS OF ONE NUMBER** — a band can be out of
# baskets with its sled untouched — so each is named beside its own kit and neither is ever read for
# the other's row.
const KIT_TIER_KEY_ATTACK := "hunter_attack"
const KIT_TIER_KEY_HUNT_CARRY := "hunt_carry_per_worker_biomass"
const KIT_TIER_KEY_FORAGE_CARRY := "forage_carry_per_worker_biomass"

# The three the expanded roster added, resolved off this band's own wear exactly like the three
# above. **EACH IS QUOTED AT A DIFFERENT KIT, AND THE COHORT'S `kit_id` ANSWERS FOR ONLY ONE OF
# THEM** (`snapshot.fbs`, `PopulationCohortState.kitId`): on a resident band `kit_id` names the HUNT
# job's default, so it answers for `hunter_attack`, `hunt_carry…` and `pen_carry…` (a pen is worked
# from a Hunt row) — while the vantage and the warrior's attack resolve through the SCOUT and
# WARRIOR defaults, the same asymmetry `forage_carry…` already has with the FORAGE default. Nothing
# in this readout may look one of them up against `kit_id`: the sim has already resolved every tier
# here, so a row states the number the band GETS and never re-derives it from a kit id.
const KIT_TIER_KEY_PEN_CARRY := "pen_carry_per_worker_biomass"
const KIT_TIER_KEY_SCOUT_VANTAGE := "scout_vantage_range"
const KIT_TIER_KEY_WARRIOR_ATTACK := "warrior_attack"

# **A KIT IS EQUIPPED WHILE ITS CONDITION IS ABOVE ZERO** — the schema's own rule, and the only test
# there is: at 0 the role steps down to its unequipped tier and STAYS there, because nothing
# replenishes kit yet. Not a threshold of the client's choosing.
const KIT_DRY := 0.0

# What a dry kit reads as on the row. A WORD, not a `0`, because the number is not the point: the
# point is which SIDE OF THE CLIFF the role is on.
const KIT_DRY_FACE := "dry"

# The condition's own rounding — it is a 0-100 scale, so a whole number is its full resolution.
const KIT_CONDITION_DECIMALS := 0

# The THREE carry tiers are biomass per worker per turn; one decimal, because the bare-handed forage
# tier is `1.6` and an integer would print it as `2` beside an equipped `8`.
const KIT_CARRY_DECIMALS := 1

# **THE VANTAGE IS A DISTANCE, NOT A RATE, AND IT MUST NOT INHERIT `KIT_CARRY_DECIMALS`.** The wire
# carries it as a float because the effects axis is continuous and a designer must be able to tune it
# so, but the sim ROUNDS it to whole tiles when a posted vantage actually reveals — so a `1.5` that
# reveals at 2 is stated as 2 here rather than as a fractional tile the map cannot draw.
const KIT_VANTAGE_DECIMALS := 0

# The role each kit sets, as the breakdown's own phrasing. The tier is stated FLAT — never scaled by
# the remaining condition — because durability and performance are orthogonal axes: a kit at 3
# performs exactly as one at 97, and then stops.
#
# **THE TWO ATTACK ROWS SAY WHICH FIGHT THEY ARE FOR.** Spears and clubs set the same `attack` stat
# off different items, and a band really does hold two different numbers for it — 20 on the hunt and
# 6 defending the camp — so a bare `attack 6` beside a bare `attack 20` would read as one of them
# being wrong rather than as two answers to two questions.
const KIT_ROLE_ATTACK_FORMAT := "attack %s"
const KIT_ROLE_HUNT_CARRY_FORMAT := "hunt carry %s per hunter"
const KIT_ROLE_FORAGE_CARRY_FORMAT := "gathering %s per forager"
const KIT_ROLE_PEN_CARRY_FORMAT := "pen collection %s per keeper"
# **THE HANDLING GEAR DOES TWO JOBS, AND ITS ROW HAS TO SAY BOTH** (issue #515). Hurdles, halters and
# a butchering stone bound a slaughter at a pen *and* speed the `Tame` and `Corral` builds — so a row
# quoting only the pen rate describes the gear's payoff at the top of the ladder and says nothing
# about the climb that produces it, which is the whole complaint the build axis was added to answer.
#
# **APPENDED ONLY ABOVE NEUTRAL, and its absence is a real reading.** A contribution of `0` means the
# gear is doing nothing to a build — because it is spent, or because this band's hunt job is on a kit
# that does not carry it — and `0 work` is a clause that costs a line's width to say *no*. The row's
# own condition and its stepped-down pen rate already carry that news.
#
# **IT READS IN WORK UNITS PER TURN, NOT AS A MULTIPLIER AND NOT AS A DISCOUNT**
# (`docs/plan_standing_upkeep.md` §4.8). It said `work off a tame or a pen` while the gear was
# subtracted from the job; a kit raises what an equipped worker DELIVERS each turn, so the clause
# states a rate and the job's size never moves. `×1.5` was the reading before that, and this is the
# same fact in the units the meter is quoted in — which is what lets it sit beside a work cost.
# A kit whose gear is spent adds nothing and the clause disappears, exactly as the neutral
# multiplier's did.
const KIT_ROLE_BUILD_WORK_SUFFIX := " · +%s work a turn per keeper on a tame or a pen"
# The contribution reads to one place: the shipped 0.5 is a playtest dial and a second decimal would
# imply a precision the number does not have.
const KIT_BUILD_WORK_DECIMALS := 1
# **The value that means "this gear changes no build"** — the schema's own default and what every kit
# carrying no build tool resolves to (which is every kit but `hurdling`, `tillage` and the `husbandry`
# bundle the hurdles also ride). Named so the suffix's suppression reads as a stated rule rather than
# a comparison against a bare literal.
const KIT_BUILD_WORK_NEUTRAL := 0.0
# Written as `2-tile sight`, not `sight 2 tiles`, because the tier is a small whole number and the
# unit would otherwise have to be pluralized: a bare-handed scout sees `1`, and `sight 1 tiles` is
# the row every value-plus-unit phrasing prints at the bottom of this axis.
const KIT_ROLE_SCOUT_VANTAGE_FORMAT := "%s-tile sight per vantage"
const KIT_ROLE_WARRIOR_ATTACK_FORMAT := "attack %s defending the camp"

# The bare-handed tag on a dry kit's breakdown row — the state worth saying plainly, since there is no
# replenishment path and the role stays there.
const KIT_BARE_HANDS_SUFFIX := " — bare hands"

# **THE SHORTFALL, ON THE ROW AND IN THE POPOVER** (issue #520). A band with ten spears among
# seventeen hunters used to render byte-identically to a fully armed one — the condition says how much
# life is left, and nothing said how many people the item ever reached.
#
# The row's form is a bare fraction because the row is a height-capped summary and already carries the
# item's name in front of it; the popover has room for the sentence. `only` is doing real work in the
# long form: `4 of 17` alone reads as a fact, and the shortfall is the point.
#
# **THE NOUN IS `workers`, NOT `hunters`, and that is the four-job wording.** Every job's coverage
# comes through one path now, so a basket's clause is this same string — and it cannot name the job,
# because the row does not carry one: `workersOnQuotedJob` is a head count and the job behind it is
# resolved sim-side. `workers` is the one noun true of a gatherer, a keeper, a scout and a warrior.
const KIT_COVERAGE_ROW_FORMAT := "%s (%d/%d)"
# **SPELLED STRUCTURALLY, the `RECOVERY_GUIDANCE_TEXT` idiom** — the two clauses below share a tail and
# a harness needs a needle that finds EITHER, so the tail is written once and both formats are built
# from it. Two literals would let a reworded clause slip past an assertion still matching the other.
const KIT_COVERAGE_SHORT_NEEDLE := "workers carry one"
const KIT_COVERAGE_BREAKDOWN_FORMAT := " · only %d of %d " + KIT_COVERAGE_SHORT_NEEDLE

# **NOBODY AT ALL, ON A STAFFED JOB** — the sharpest reading the pair produces, and it takes its own
# sentence because `only 0 of 4` is the arithmetic where *"none of your 4"* is the fact. It is
# reachable with the item LIVE, not just dry: an item needing a full crew (`workers_per_unit > 1`)
# equips nobody until the job is staffed to it, so a band can own a good net and hold it with no one.
const KIT_COVERAGE_BREAKDOWN_NONE_FORMAT := " · none of your %d " + KIT_COVERAGE_SHORT_NEEDLE

# One `    ▲ Spears 87 — attack 20`-style breakdown row, and the sentence beneath the whole set.
const KIT_BREAKDOWN_ROW_FORMAT := "%s%s %s %s — %s"

# **THE CLIFF, IN ONE SENTENCE.** Without it a player reads the conditions as a performance gradient
# and paces their hunting against a number that does not move anything — which is the exact
# misreading the flat-until-expiry model invites.
const KIT_BREAKDOWN_CLIFF_NOTE := "A kit works at full strength until it runs out — the condition is how long you have, not how well it works. There is no way to make more yet."

const BREAKDOWN_CARET_OPEN := "▾"
const BREAKDOWN_CARET_CLOSED := "▸"

# ---- Larder-runway vocabulary. The UNIT is spelled in exactly one place (`food_turns_text`) and the
# Food/Provisions/Carried threshold tint recognizes its row by looking for that same word — never a
# bare literal, which is how the guard silently went dead once when the unit changed from days.
const FOOD_UNLIMITED_GLYPH := "∞"
const FOOD_RUNWAY_UNIT := "turn"

## **THE SAME GLYPH ON A BUILD ESTIMATE, AND ITS MEANING IS INVERTED.** On the Food line `∞` is good
## news — the larder never empties; on a build it is the worst news the sheet can carry — this crew
## never finishes, because it banks no more per turn than the meter is rotting by.
## So it takes a WARNING treatment rather than the neutral ink the runway gets: the one readout that
## should stop the player must not read as reassurance. The glyph is shared deliberately — a player
## who has learned it on the food line reads it here without being taught twice — and the ink is what
## says which way it points.
##
## **BOTH never-finishing sentinels draw it, and the INK is what separates THEM too**: amber for
## `SourceForecast.BUILD_TURNS_HOLDS` (the meter stands still) and red for `BUILD_TURNS_ROTS` (it is
## going backwards). One glyph, three meanings, three colours — see `rung_value_hex`.
const BUILD_TURNS_NEVER_GLYPH := FOOD_UNLIMITED_GLYPH

# ---- Predators Phase 0 — the four RAW combat components (strength ≠ danger). Keys ≤ 16 chars so
# `_split_kv` aligns them as table rows. Attack/Defense are open-ended (bar relative to the roster
# max); Fights back / Aggressive are native 0..1 (bar + %).
const DANGER_ATTACK_ROW := "Attack"
const DANGER_DEFENSE_ROW := "Defense"
const DANGER_FEROCITY_ROW := "Fights back"
const DANGER_AGGRESSION_ROW := "Aggressive"
const DANGER_BAR_CELLS := 5
## The compact derived line the player reasons about: hunt cost vs unprovoked menace.
const DANGER_DERIVED_ROW := "Danger"
const DANGER_DERIVED_FORMAT := "Hunt %s · Threat %s"
## **THE THREE ROWS `Danger` IS MADE OF SIT UNDER IT; `Defense` DOES NOT.** `Hunt` is
## `attack × ferocity` and `Threat` is `attack × aggression` — three of the four components, and
## Defense appears in neither. It answers a different question (how hard the herd is to kill, and on
## the predator side whether something else eats it), so indenting it under Danger would assert a
## contribution it does not make. It rises to sit with Size / Herd / Range, the other facts about what
## this herd IS, where it also stops reading as Attack's natural pair — which is what made it look
## like a fourth input in the first place.
##
## **THE INDENT MUST NOT BEGIN WITH `MORALE_BREAKDOWN_INDENT`.** `detail_bbcode` routes any line
## starting with that 4-space prefix to the FULL-WIDTH sub-line branch, and these rows have to stay
## KV table rows or their bars stop sharing a column — which is the whole point of a bar. Three spaces
## indents inside the key cell and still falls through to `_split_kv`. Guarded in `ui_preview`.
const DANGER_COMPONENT_INDENT := "   "

# ---- Keeper staffing. RETIRED — **the `Keepers:` row is gone** (issue #545): it stated a standing
# demand every turn on a herd where nothing was wrong, read as noise beside the `Keeping:` row saying
# the same number again, and what a player needed from it — the head count — only matters when the
# pool is SHORT, which `HERDERS_SHED_FORMAT` below states and the rung row's own ⚠ marks. `KEEPERS_ROW`,
# `HERDERS_STAFFED_FORMAT`, `HERDERS_UNDER_FORMAT` and `herders_label` / `herders_value_hex` went with
# it. `FULLY_HERDED` is the `herded_fraction` wire default (1.0 = fully staffed, also
# unmanaged/vanished herds) — treated as "no problem". The `under-herded` word survives on the shed
# sentence: it names the HERD's state (the sim's own), not a crew.
const FULLY_HERDED := 1.0

# ---- RETIRED — the per-rung BUILD VERBS (issue #545). `Building` / `Sowing` / `Domesticating` /
# `Preparing` each headlined a card row stating that rung's meter in work units; the row leads with
# the TURN COUNT now (`HudSelectionVocab.RUNG_TURNS_FORMAT`), which is what a glance wants off a build
# and which no verb can say. The compose sheet keeps its own participles
# (`HudComposeVocab.IMPROVEMENT_RUNNING_LABELS`) because a sheet is composing that verb, not reporting
# it. Each rung's "the meter is full" mark stays its own const (progress arrives as 0..1 per rung),
# and the badge word a BUILT rung wears lives beside its `*_built_label`.
const CORRAL_PROGRESS_COMPLETE := 1.0
const HUSBANDRY_PROGRESS_COMPLETE := 1.0
const CULTIVATION_PROGRESS_COMPLETE := 1.0
const FIELD_PROGRESS_COMPLETE := 1.0
const FIELD_BADGE_LABEL := "Field"

# ---- RETIRED — **`PEN_STARVING_LABEL`** (`⚠ Starving — %d%% fed`), the Corral row's starving face.
# It welded two unrelated percentages into one row: a built rung row renders `<label> <meter %>`, so a
# starving pen read `Corral: ⚠ Starving — 47% fed 100%` — how FED the herd is beside how BUILT the pen
# is, with no separator between them. The Corral row means the rung again (`🐄 Corralled 100%`), and
# the whole feed story — including the hazard mark and the tint — is the `Fed:` row below.

# ---- The penned herd's own rows: the fenced footprint, and the `Fed:` row that carries the WHOLE
# feed story. Every reader is `herd_summary_lines` below, so the whole block lives here.
const PEN_FOOTPRINT_ROW := "Pen"
const PEN_FOOTPRINT_FORMAT := "radius %d · %d tiles"

# **THE ROW IS NAMED FOR WHAT IT MEASURES, NOT FOR ONE OF ITS TERMS.** It read `Fed by pasture` and
# then listed hay, naming one feed source in the label and the other in the value. `Fed:` leads with
# `pen_fed_fraction` — how much of its demand the pen actually got — and the terms beneath it say
# where that came from:
#
#     Fed:  100% — all pasture
#     Fed:  100% — 88% pasture · 12% fodder
#     Fed:  ⚠ 47% — 40% pasture · 7% fodder · needs 11.3 more/turn
#     Fed:  ⚠ 40% — 40% pasture · no fodder · needs 12.0 /turn
#
# **THE WORD IS `fodder`, NEVER `hay`.** Fodder is the category (food for livestock, dried hay or
# straw among it) and hay is one instance of it; the band's own store row already says `Fodder`, and
# the sim's fields and units are fodder throughout. One word for one thing.
const PEN_FEED_ROW := "Fed"
const PEN_FEED_VALUE_FORMAT := "%s%d%% — %s%s"

# The hazard mark LEADS the value on a starving pen (`PenStatus.is_starving`, the one test the map
# badge and the turn orb also ask), which is both the mark the player sees and — via
# `pen_feed_value_hex` — what puts the row in DANGER ink. That tint is the one the Corral row's
# retired `contains("starving")` special case used to carry.
const PEN_FEED_HAZARD_PREFIX := "%s " % HudSelectionVocab.RUNG_HAZARD_GLYPH

# **THE TWO SHARES ARE ONE SUBTRACTION, NEVER A DIVISION.** `pen_pasture_fraction` is the share the
# fenced footprint grazed and `pen_fed_fraction` is the share that arrived in total, so the fodder
# share is the DIFFERENCE of two published ratios over the same denominator. The gross demand those
# ratios are shares OF is **not on the wire** — `fodder_draw` is an absolute in fodder units — so
# dividing the draw by a ratio to synthesize a total, or to price the fodder share, is the one thing
# this row must not do. The subtraction is taken on the ROUNDED percentages so the two terms visibly
# add up to the headline, and clamped at zero because `pen_fed_fraction` is clamped at 1.0 and the
# pair can round-cross.
const PEN_FEED_PASTURE_SEGMENT := "%d%% pasture"
const PEN_FEED_FODDER_SEGMENT := " · %d%% fodder"

# **NOTHING CARRIED IN IS A DIFFERENT FACT FROM A LITTLE CARRIED IN**, so a pen whose draw is under
# `SourceForecast.FODDER_FLOW_MIN` says `no fodder` in the share's place rather than `0% fodder`: the
# keeper brought none — because the band has none, or has not learned Foddering — which is a
# different problem from a ration that fell short.
const PEN_FEED_NO_FODDER_SEGMENT := " · no fodder"

# The share the land covers outright: no second term and, by construction, no shortfall. A pen whose
# own fenced footprint feeds it is the one state where the split has nothing to split.
const PEN_FEED_ALL_PASTURE_SEGMENT := "all pasture"

# The pasture share at which the footprint covers the whole feed. `PenStatus.FED_EPSILON` is the slack
# either side of it — the same rounding slack the fed fraction takes, for the same reason: a share
# that lands at 0.998 is a covered pen, not a 99.8% one.
const PEN_PASTURE_COVERS_ALL := 1.0

# **HOW MUCH MORE FODDER A TURN WOULD FIX IT** — `pen_fodder_shortfall`, the sim's own
# `max(0, hay gap − fodderDraw)`, where the gap is what the pen's fenced footprint leaves uncovered.
# It is struck on the same pass as both its terms, so it can never describe a different turn from
# them, and it is the ONLY term of that subtraction on the wire: the gap has no per-pen field, so
# there is no second figure here to difference or to cross-check against.
# It appears ONLY above `SourceForecast.FODDER_FLOW_MIN`:
# a fed pen owes nothing and must not read `needs 0.0`, the false precision that floor exists to stop.
#
# **`more` IS ONLY TRUE WHEN SOMETHING ARRIVED.** A pen drawing nothing is not short of MORE fodder,
# it is short of fodder, so the `no fodder` state takes the plain form. The "/turn" is spelled tight
# in the compact spelling `SourceForecast.POLICY_CAP_FODDER_FORMAT` uses, not `YIELD_PER_TURN_SUFFIX`'s
# spaced standalone form.
const PEN_FEED_SHORTFALL_MORE_SEGMENT := " · needs %s more/turn"
const PEN_FEED_SHORTFALL_SEGMENT := " · needs %s /turn"

# ---- Husbandry-ceiling stand-ins. Rendered in place of the whole husbandry section on a wild-ceiling
# herd, and where the corral affordance would sit on a pastoral one — so the missing controls read as
# intentional, not a bug. Colon-free, so `detail_bbcode` renders them as dim informational sentences
# (the `kv.is_empty()` path).
const HUSBANDRY_WILD_HINT := "Wild game — hunt only"
# A predator is a hunter, not quarry — "game" is a category error for a wolf pack. "hunt only" stays
# correct (you CAN hunt/eradicate a predator); only the "game" noun is wrong. Branched on
# `is_predator` (`prey_sense_radius > 0`) in `herd_summary_lines`.
const HUSBANDRY_WILD_PREDATOR_HINT := "Wild predator — hunt only"
const HUSBANDRY_PASTORAL_HINT := "Herdable, not pennable"

# ---- The under-herded CONSEQUENCE line (fauna neglect-escape arc). Neglect no longer decays a
# managed herd's tameness — an under-herded herd SHEDS whole animals over its labor capacity into a
# nearby wild herd (the animals drift off, tameness leaves with them). So the drawer states the shed,
# its live cost, and the one lever that stops it — never the retired "tameness slipping" story.
#
# **THE LEVER IS THE BAND'S HUSBANDRY ROLE, AND IT IS NAMED AS SUCH** (`docs/plan_standing_upkeep.md`
# §2.5). It read *"Staff all N herders"* while one crew both hunted a herd and held it, then *"Staff
# N KEEPERS"* while the keeping was a stepper on this herd's compose sheet. **Maintenance has since
# left the tile**: a managed herd is held out of the band's `husbandry` pool, so the sentence names
# the role card that moves it and quotes what THIS herd is worth in hands (`upkeepWorkersNeeded`,
# which equals `herdersNeeded` on a held rung — the two differ only while the rung is still being
# BUILT, where the row above states in words that the build's crew holds it). A remedy that named a
# control the player cannot find is the failure this wording exists to avoid.
#
# **IT LEADS WITH THE HAZARD MARK NOW** (issue #545), which is what routes it through
# `detail_bbcode`'s full-width WARN branch. It rendered in the muted INK_DIM a descriptive sentence
# gets, because that branch tested one known sentence by equality — so the one line in the client that
# says animals are drifting off was quieter than the rows around it.
const HERDERS_SHED_FORMAT := "%s Under-herded — animals are drifting off. This herd wants %%d of the band's Husbandry hands." % HudSelectionVocab.RUNG_HAZARD_GLYPH

# ---- THE STANDING-STOCK ROW, AND ITS KEY NAMES ITS UNIT. `Herd: 6 / 11` counts ANIMALS — the unit
# the hunt sheet already delivers in — so the card and the sheet finally read in one currency. It
# falls back to `Biomass: 821 / 1442` for a species the wire published no `body_mass` for, and the
# label switches WITH the unit rather than staying put: `Herd 821` invites reading 821 as a head
# count, which is wrong by the body mass. The two keys are the honest labels for the two readings.
const HERD_STOCK_ROW := "Herd"
const HERD_STOCK_BIOMASS_ROW := "Biomass"
# ---- Herd drawer grazing range (Grazing Phase 2b-iii): the ground the herd grazes — a SEPARATE fact
# from the stock/cap pair the `Herd` row carries. Key ≤ `DETAIL_KEY_MAX_LENGTH` so it aligns as a
# table row beside it.
const HERD_RANGE_ROW := "Range"
# ---- Herd drawer size class: the `<size> game` class the roster row used to carry as its meta. The
# row's meta slot now states the herd's STAFFING, so the size class moved to the drawer.
const HERD_SIZE_ROW := "Size"
const HERD_SIZE_CLASS_FORMAT := "%s game"
# A predator's size class reads "Big predator", not "Big game" — a carnivore is a hunter, not quarry.
# Branched on `is_predator` (`prey_sense_radius > 0`) in `herd_summary_lines`.
const HERD_SIZE_CLASS_PREDATOR_FORMAT := "%s predator"

# ---- Overgrazing: a TRIVIAL honest comparison of two sim-provided numbers (the ecology model is the
# sim's). The epsilon keeps a herd sitting exactly at K from flickering the warning; the warning SENTENCE
# is emitted by the producer below and matched verbatim by `detail_bbcode`'s WARN branch.
const OVERGRAZE_EPSILON := 0.05
const OVERGRAZING_WARNING := "%s Overgrazing — range can't sustain this herd" \
    % HudSelectionVocab.RUNG_HAZARD_GLYPH

# ---- Recovery guidance — a dim line naming the real levers (NOT harvest) when morale is concerning.
# The GLYPH is how `detail_bbcode` recognizes the line; the TEXT is what the morale-breakdown producer
# emits. They are one invariant ("the text begins with the glyph"), so it is spelled structurally here
# rather than as two literals that could drift.
const RECOVERY_GUIDANCE_GLYPH := "↑"
const RECOVERY_GUIDANCE_TEXT := RECOVERY_GUIDANCE_GLYPH + " Recover: move to Hospitable ground · Scout · Hunt"

# ---- Expedition delivery vocabulary (the `expedition_*` producers below are the only readers).
# Marks a hunt party's "Next delivery" line when the party relaunches for repeated trips (Deplete
# policy). Distinct from the Deplete policy glyph already shown (`FoodIcons.for_policy("deplete")` = ⇊),
# so the two never read as duplicated: ↻ = "this trip repeats", ⇊ = "the take presses the herd down hard".
const EXPEDITION_RECURRING_GLYPH := "↻"
# "Next delivery" lines for the two ways a projected-0 forecast can arise, disambiguated on the
# party's own `expedition_target_herd` (which MIGRATES and is often NOT the herd the player is
# looking at). Target still in the herd telemetry but forecast projects 0 → it is at/below its
# policy floor; target absent from telemetry → the herd was lost/replaced and the party is coming home.
const EXPEDITION_NEXT_DELIVERY_NO_SURPLUS := "Next delivery: none — its target herd has no surplus to raid"
const EXPEDITION_NEXT_DELIVERY_TARGET_LOST := "Next delivery: target herd lost — the party is returning home"
# The click affordance on an Active-expeditions row (the whole row is the button there).
const EXPEDITION_ROW_FOCUS_HINT := "Click to show this expedition on the map."
# **THE HUNT PARTY'S ORDERS ROW** — `%s` the floor's own `HudComposeVocab.FLOOR_VALUE_FORMAT` value.
# It carried a second `· `-joined clause for the fill target and is a ONE-clause row since that lever
# retired (issue #491); see `expedition_orders_line` for why it stays ONE row whatever it carries.
const EXPEDITION_ORDERS_ROW_FORMAT := "Orders: %s"
# **THE DENIAL PARTY'S ROW KEY** (`docs/plan_denial_raid.md`), standing where `Next delivery:` stands
# on a hunt party. One word, so `_split_kv` lays it out as a table row beside the others; the VALUE
# carries its own tint, since a verdict's severity is a fact about the forecast and not about the key.
const DENIAL_COLLAPSE_ROW := "Collapse:"
# The quoted-party clause this row used to carry is gone with the sampled ladder that made a nearby
# row the best a launched party could be told about. A query answers the party that was sent, so the
# row states the verdict and nothing about whose numbers they are.

# ---- The tile card's BASKET rows — what the `Foraging` stock above them is MADE OF (flora roster
# F1/F5). Each realized plant reads on its OWN indented row: a role icon, the plant's display name,
# its share of the patch (`HudFloraVocab.FLORA_SHARE_FORMAT`) and the standing biomass that share
# amounts to. Rows reuse the food/morale breakdown's 4-space `MORALE_BREAKDOWN_INDENT`, and the
# indent is doing REAL WORK here — it is what says these rows decompose the row above them rather
# than listing three more resources beside it.
#
# THE ROLE ICON REPLACED ONE GENERIC 🌿 SPRIG ON EVERY ROW, and that is why the tint branch in
# `detail_bbcode` no longer sniffs for a glyph: the leading mark is now `FoodIcons.for_crop_role`,
# which is one of three marks OR nothing at all (an unstated role renders no icon, never a defaulted
# one), so no literal could identify these rows. The renderer tells a descriptive sub-row from a
# SIGNED one by the ▲/▼ the signed families all carry — see `detail_bbcode`.
const FLORA_COMPOSITION_SUBLINE_FORMAT := "%s%s %s"

# ---- The Growth row + its itemized fertility breakdown (the birth path's parallel of the morale
# contributions). The headline states the band's birth rate as a share of NORMAL, which can exceed
# 100% — a well-provisioned band out-breeds its base rate — so the value spells its anchor out rather
# than leaving a bare "150%" to read as a cap. The sub-rows are MULTIPLIERS (`×0.60`), not signed
# deltas: these factors combine by product, and three percentages that refuse to sum to the headline
# would invite arithmetic they cannot support. See `fertility_breakdown_row`.
const GROWTH_ROW_FORMAT := "Growth: %d%% of normal"
# The same reading with the anchor DROPPED, for the SHORT band-zone tier's MERGED Morale+Growth line
# (`BandDetailLines.BAND_MORALE_GROWTH_CLAUSE_FORMAT`). The anchor is what makes a standalone `150%`
# legible, and it is exactly what a merged line cannot afford: the vitals label is `AUTOWRAP_WORD`, so
# a run too wide for the column WRAPS and costs back the very row the merge bought. The suffix is
# recoverable — the Growth disclosure the clause still opens restates the factors in full — and the
# `%` is unambiguous beside a `%` morale reading on the same line.
const GROWTH_VALUE_SHORT_FORMAT := "%d%%"
# The anchor itself, so a reader asserting that the TALL/COMPACT tiers KEPT the full row has a needle
# that cannot drift from the format above.
const GROWTH_ROW_ANCHOR_SUFFIX := " of normal"
const FERTILITY_BREAKDOWN_ROW_FORMAT := "%s%s ×%.2f  %s"
# The three factor labels, in the display order of `docs/plan_population_growth_model.md` §2:
# hunger (the gate) → reserve (stock) → trend (flow). `hunger` is only ever ≤ 1 and `reserve` only
# ever ≥ 1, so each of those labels states its one direction outright; `trend` is two-sided, so it
# forks on sign the way the morale breakdown's culture/unrest row does.
const FERTILITY_LABEL_HUNGER := "short rations"
const FERTILITY_LABEL_RESERVE := "larder reserve"
const FERTILITY_LABEL_TREND_GROWING := "larder growing"
const FERTILITY_LABEL_TREND_SHRINKING := "larder shrinking"

## The longest `Key` `_split_kv` will align into a table row; anything wider reads as a sentence.
const DETAIL_KEY_MAX_LENGTH := 16
## The separator a data line puts between its key and its value.
const DETAIL_KV_SEPARATOR := ": "


## THE PER-RENDER TINT CONTEXT — what `detail_bbcode` needs to know about the band whose rows it is
## rendering, and nothing else. Built fresh by whichever host is about to render, filled by the line
## PRODUCERS as they emit the rows, and handed to the renderer. It replaced three `HudLayer` members
## (`_selected_band_food_turns` / `_selected_band_morale` / `_selected_band_output`) plus
## `_disclosure_state`, all of which were per-render out-parameters reached sideways. The output
## scalar has since gone with the row it tinted: productivity reads on the WORK zone's head now
## (`BandPanelController._build_work_head`), which is Labels rather than BBCode and needs no context.
##
## NAN means "no band" for each scalar: the corresponding row then renders in neutral ink, exactly as
## the old `is_nan` guards decided. `disclosures` is row-label → `{key, open, concerning}` (see
## `DisclosureController.state`); empty means no row wears a caret.
class Context extends RefCounted:
    var food_turns: float = NAN
    ## The FODDER larder's runway, for the `Fodder:` row's value tint — the food field's twin, filled
    ## by `BandDetailLines._band_fodder_line` and read by `_value_hex` through the same
    ## `BandFoodStatus.hex_for_turns` map. NAN when no fodder row was emitted (a band with no fodder
    ## economy, or the `compact` tier, which carries the stock as a clause on Food instead).
    var fodder_turns: float = NAN
    ## The STANDING MATERIAL BILL's runway — the worst good's shelf against the gap its arrivals leave,
    ## for the `Upkeep:` row's value tint, through the same `BandFoodStatus.hex_for_turns` map both
    ## larders read. NAN when no bill row was emitted (a band holding nothing that eats a good), which
    ## is what stops the previous band's tint reaching a row that is not there.
    var material_turns: float = NAN
    var morale: float = NAN
    ## The band's fertility MULTIPLIER (`hunger x reserve x trend`), 1.0 = its normal birth rate.
    ## NAN when there is no band, or when the sim published no reading yet (the not-projected
    ## sentinel) — in which case no Growth row was emitted to tint.
    var fertility: float = NAN
    var disclosures: Dictionary = {}
    ## **THE PER-ROW HOVER, keyed by the row's own KEY** — `{"Cultivation": "This ground is
    ## slipping — …"}`. A detail surface is ONE `RichTextLabel`, so a row cannot carry a
    ## `tooltip_text` of its own; `[hint=…]` is the per-run equivalent and this is what fills it.
    ##
    ## **IT IS A CONTEXT FIELD RATHER THAN A GUESS AT THE RENDERER**, for `_value_hex`'s own reason
    ## one field up: the producer that knows a row is in trouble is the one that wrote the row, and a
    ## renderer sniffing the value for a hazard word would need a new guess per hazard. A key with no
    ## entry renders exactly the BBCode it always did.
    var row_tooltips: Dictionary = {}


# =====================================================================================
#  THE RENDERER
# =====================================================================================

## Render selection detail lines as BBCode: consecutive "Key: value" rows become a 2-column table
## (dim key, bright value, per-row value tint) so the data aligns into columns, while sentences and
## section lines stay full-width and muted. Matches the mockup's Tile Banner body.
##
## `ctx` carries everything band-specific (see `Context`); pass nothing for a surface with no band
## behind it — the popover's own restate, the tile card, the unknown-contents note.
static func detail_bbcode(lines: Array, ctx: Context = null) -> String:
    var context := ctx if ctx != null else Context.new()
    var out := ""
    var table_open := false
    for raw in lines:
        var line := String(raw)
        if line == "":
            if table_open:
                out += "[/table]"
                table_open = false
            out += "\n"
            continue
        # INDENTED SUB-ROWS — ONE branch for every family that hangs rows beneath a headline row.
        # They render full-width (never as a lopsided table row), and THE SIGN GLYPH DECIDES THE TINT:
        #   • ▲ → HEALTHY green, ▼ → WARN amber. The itemized morale / food / fertility breakdowns,
        #     kept deliberately two-tone rather than a rainbow.
        #   • NEITHER → neutral ink. The tile card's basket rows, which are DESCRIPTIVE — a plant's
        #     share of the ground is not a good or a bad thing — so they must not borrow the two-tone.
        # **The neutral case is matched by the ABSENCE of a sign, not by a mark of its own**, and that
        # is why this is one branch instead of the two it used to be: the basket rows now lead with a
        # crop ROLE icon that is one of three marks or nothing at all (an unstated role renders no
        # icon), so no literal can identify them. The old pair keyed its neutral branch off the single
        # 🌿 sprig every basket row then wore, and had to be tested first because both matched the
        # same indent. Any future indented family is neutral until it carries a sign, which is the
        # safe default — the old fallthrough tinted it WARN.
        # The `\n` after `[/table]` forces a block break: a RichTextLabel `[table]` is inline, so text
        # emitted right after it otherwise floats onto the table's top-right when there's room.
        if line.begins_with(DetailFormat.MORALE_BREAKDOWN_INDENT):
            if table_open:
                out += "[/table]\n"
                table_open = false
            var row_hex := HudStyle.INK_HEX
            if line.contains(DetailFormat.MORALE_CONTRIB_POSITIVE_GLYPH):
                row_hex = HudStyle.HEALTHY_HEX
            elif line.contains(DetailFormat.MORALE_CONTRIB_NEGATIVE_GLYPH):
                row_hex = HudStyle.WARN_HEX
            # **THE BUILD'S `∞` IS THE THIRD SIGN THIS BRANCH RECOGNISES**, and it takes the same amber
            # a negative contribution does: a crew that never finishes is the one reading on a source
            # card that should stop the player. The larder runway draws the
            # identical glyph for the OPPOSITE news and never lands here — it is a `Key: value` row,
            # tinted by `_value_hex` — so the mark can be shared while the ink stays disjoint.
            elif line.contains(DetailFormat.BUILD_TURNS_NEVER_GLYPH):
                row_hex = HudStyle.WARN_HEX
            # **A MISSING GOOD IS RED WHERE MISSING HANDS ARE AMBER**
            # (`docs/plan_standing_upkeep.md` §2.7), and the fork is the one NAMED lead-in that means
            # it: `Short of …`. Twelve keepers do not mend a fence with no hurdles, so the two
            # shortfalls must not read alike — a stalled build and an under-kept source both draw an
            # indented sentence here, and until this arm they drew it in the same ink.
            #
            # ⛔ **THE INK IS THE RENDERER'S BECAUSE THE SENTENCE CANNOT CARRY IT.** The same string
            # goes verbatim into the build queue row's plain-text `tooltip_text`, where a `[color=…]`
            # run would print its own markup. Keyed on the lead rather than on a list of known
            # sentences, which is the rule the hazard-glyph branch below already states.
            elif line.strip_edges().begins_with(
                    HudSelectionVocab.BUILD_BLOCKED_MATERIAL_SHORT_LEAD):
                row_hex = HudStyle.DANGER_HEX
            out += "[color=#%s]%s[/color]\n" % [row_hex, line]
            continue
        # **A FULL-WIDTH SENTENCE THAT LEADS WITH THE HAZARD MARK IS A WARNING, and that is now the
        # rule rather than a list of known sentences.** It tested `line == OVERGRAZING_WARNING`
        # exactly, so every other hazard sentence in the game rendered in the muted INK_DIM a
        # descriptive line gets — including the under-herded shed, which is the one line in the client
        # that says animals are drifting off. `HudSelectionVocab.RUNG_HAZARD_GLYPH` is the same mark
        # the rung rows carry, so one needle covers both shapes.
        if line.begins_with(HudSelectionVocab.RUNG_HAZARD_GLYPH):
            if table_open:
                out += "[/table]\n"
                table_open = false
            out += "[color=#%s]%s[/color]\n" % [HudStyle.WARN_HEX, line]
            continue
        var kv := _split_kv(line)
        if kv.is_empty():
            if table_open:
                out += "[/table]\n"
                table_open = false
            out += "[color=#%s]%s[/color]\n" % [HudStyle.INK_DIM_HEX, line]
        else:
            if not table_open:
                out += "[table=2]"
                table_open = true
            out += "[cell]%s[/cell][cell][color=#%s]%s[/color][/cell]" % [
                _key_cell(String(kv[0]), context), _value_hex(String(kv[0]), String(kv[1]), context), kv[1],
            ]
    if table_open:
        out += "[/table]"
    return out

## **THE HOVER A DETAIL BLOCK CARRIES, JOINED FROM THE ROWS THAT REGISTERED ONE** — and it is the
## BLOCK's, not the row's, because THE ENGINE WILL NOT CARRY A PER-ROW ONE HERE.
##
## ⛔ **`[hint=…]` IS NOT PARSED BY THIS GODOT BUILD, AND IT FAILS LOUDLY IN THE MIDDLE OF A TABLE.**
## It was tried first, being the documented BBCode for a per-run tooltip: the tag rendered LITERALLY,
## and the parser did not recover — every tag after it in that cell (`[color=…]`, `[/cell]`,
## `[/table]`) printed as text too, so the rung row read
## `[hint=This ground is slipping…][color=#f2b13f]⚠ Blocked 96%[/color][/hint][/cell][/table]` on
## screen. Rendered and looked at, not reasoned about. Do not re-add it without rendering it first.
##
## So the remedy rides `RichTextLabel.tooltip_text`, which answers for the whole block — and the block
## is exactly ONE SOURCE (the land drawer describes one patch, the herd drawer one herd), of which at
## most one rung is ever at risk (`SourceForecast.at_risk_rung`). The hover therefore states one
## source's remedy on a surface about that source, which is a wider target than the row and never a
## different subject. **A `RichTextLabel` is NOT a `Label`**, so a bare `tooltip_text` is live here
## rather than the silent no-op `HudWidgets.set_label_tooltip` exists for.
##
## `""` for a block whose every rung is being kept, which is every calm card in the game — and an
## empty `tooltip_text` shows no tooltip at all, so nothing is offered where nothing is wrong.
static func block_tooltip(ctx: Context) -> String:
    if ctx == null:
        return ""
    var lines: Array[String] = []
    for key in ctx.row_tooltips:
        var hover := String(ctx.row_tooltips[key])
        if hover != "" and not lines.has(hover):
            lines.append(hover)
    return "\n".join(lines)

## THE KEY→TINT REGISTRY: which hex a row's VALUE renders in, keyed on the row's own label. Every
## detail surface in the game consults this one table, which is why the tile card's Sight /
## Habitability / Ecology cases live beside the band's Food / Morale / Growth ones.
static func _value_hex(key: String, value: String, ctx: Context) -> String:
    if key == HudDisclosureVocab.DETAIL_ROW_FOOD or key == "Provisions" or key == "Carried":
        # The band larder / expedition provisions / hunt-party carried-food row tints by the
        # larder-runway thresholds. It recognizes the row by the SHARED `FOOD_RUNWAY_UNIT` the one
        # renderer (`food_turns_text`) spells the runway with — never a bare literal, which is how
        # this guard silently went dead when the unit changed — or by the ∞ glyph for a band that is
        # not food-limited.
        if not is_nan(ctx.food_turns) and (value.contains(FOOD_RUNWAY_UNIT) or value.contains(FOOD_UNLIMITED_GLYPH)):
            return BandFoodStatus.hex_for_turns(ctx.food_turns)
    elif key == HudDisclosureVocab.DETAIL_ROW_FODDER:
        # The fodder larder's row, tinted by ITS runway through the same threshold map the Food row
        # uses — one severity rule for both larders, which is what lets the row's own amber `need`
        # clause be retired rather than replaced. Recognized by the SHARED runway spelling, exactly as
        # the food case above is, so a re-worded runway cannot leave this reading a stale tint.
        if not is_nan(ctx.fodder_turns) and (value.contains(FOOD_RUNWAY_UNIT) or value.contains(FOOD_UNLIMITED_GLYPH)):
            return BandFoodStatus.hex_for_turns(ctx.fodder_turns)
    elif key == HudDisclosureVocab.DETAIL_ROW_UPKEEP:
        # The standing material bill's row, tinted by the WORST good's runway through the same
        # threshold map both larders use — one severity rule for every account that can run out, which
        # is what lets a short good take the danger ink without a second grading beside it.
        # Recognized by the SHARED runway spelling, exactly as the two cases above are, so a re-worded
        # runway cannot leave this reading a stale tint.
        if not is_nan(ctx.material_turns) and (value.contains(FOOD_RUNWAY_UNIT) \
                or value.contains(FOOD_UNLIMITED_GLYPH)):
            return BandFoodStatus.hex_for_turns(ctx.material_turns)
    elif key == HudDisclosureVocab.DETAIL_ROW_MORALE:
        # The player band's morale row tints by the morale thresholds.
        if not is_nan(ctx.morale):
            return BandFoodStatus.hex_for_morale(ctx.morale)
    elif key == HudDisclosureVocab.DETAIL_ROW_GROWTH:
        # The band's birth rate as a share of normal, tinted by the fertility buckets. Same
        # ink → amber → red grading as `BandFoodStatus.color_for_output` and for the same reason: normal
        # growth is normal, not a "good", so the top bucket is neutral ink even when the band
        # out-breeds its base rate.
        if not is_nan(ctx.fertility):
            return BandFoodStatus.hex_for_fertility(ctx.fertility)
    elif key == "Habitability":
        # The tile's habitability rating tints by its bucket (green→red).
        return TileHabitability.hex_for_rating(value)
    elif key == HudConst.TILE_SIGHT_KEY:
        # The tile's sight state: live cyan when in sight, dim when only remembered/unknown.
        return sight_value_hex(value)
    elif key == HERD_STOCK_ROW or key == HERD_STOCK_BIOMASS_ROW \
            or key == HudFloraVocab.FORAGING_KEY or key == HudFloraVocab.GRAZING_KEY:
        # ONE phase tint (neutral/amber/red) for every ecology in the game — the herd's own stock row
        # and the tile card's two food-web stock rows, all of which carry their phase INLINE after the
        # stock (`205 / 205 · ⚠ Stressed`) instead of on a standalone `Ecology` row beneath them.
        # `ecology_value_hex` matches the phase WORD wherever it sits in the value, so folding the
        # rows in forked nothing: the styling path is still the single shared one.
        return ecology_value_hex(value)
    elif key == HUSBANDRY_ROW:
        return husbandry_value_hex(value)
    elif key == CULTIVATION_ROW:
        return cultivation_value_hex(value)
    elif key == HudFloraVocab.FIELD_ROW:
        # Plant rung 3 — the patch twin of the Corral row's tint (ink while building, signal once
        # complete). Same shape as Cultivation's; kept its own case because a Field is a different
        # rung with its own badge word, not a Tended Patch at a higher percentage.
        return field_value_hex(value)
    elif key == CORRAL_ROW:
        return corral_value_hex(value)
    elif key == PEN_FEED_ROW:
        # The penned herd's feed row: red while the pen is starving. This is the case the Corral row
        # above used to carry, and it belongs to the row that states the fed fraction.
        return pen_feed_value_hex(value)
    return HudStyle.INK_HEX

## The BBCode a clickable disclosure run OPENS with. Named because it is the needle that tells a
## disclosure rendered as its OWN table row (`detail_bbcode` emits `[cell]` immediately before it)
## from one MERGED into another row's value cell (where a separator precedes it) — a structural
## difference the parsed text cannot show, and the only thing that can catch a tier merge leaking into
## the tier above it.
const DISCLOSURE_URL_OPEN := "[url="

## **THE SAME CLICKABLE RUN, FOR A ROW MERGED INTO ANOTHER ROW'S VALUE CELL.** A merged row is still a
## disclosure — the vitals block is ONE `RichTextLabel`, so both `[url]` metas live on the same label
## and both popovers keep working — and it must wear the identical label + caret + tint a standalone
## row wears, so this delegates rather than re-spelling the run. `""` when the row registered no
## disclosure, which is the caller's cue to state the label plainly.
static func inline_disclosure_label(key: String, ctx: Context) -> String:
    if ctx == null or not ctx.disclosures.has(key):
        return ""
    return _key_cell(key, ctx)

## A disclosure row (Food/Morale) renders its key as a clickable `[url]` + ▸/▾ caret, which opens its
## breakdown in the shared POPOVER via `meta_clicked` → `DisclosureController` (never inline — see the
## BREAKDOWN_* consts). The caret is ▾ only while THIS row's popover is up. A CONCERNING row wears the
## caret in WARN rather than SIGNAL: the breakdown no longer opens itself, so the invitation to read
## it has to be visible.
static func _key_cell(key: String, ctx: Context) -> String:
    if not ctx.disclosures.has(key):
        return "[color=#%s]%s[/color]" % [HudStyle.INK_DIM_HEX, key]
    var st: Dictionary = ctx.disclosures[key]
    var caret := BREAKDOWN_CARET_OPEN if bool(st.get("open", false)) else BREAKDOWN_CARET_CLOSED
    var caret_hex := HudStyle.WARN_HEX if bool(st.get("concerning", false)) else HudStyle.SIGNAL_HEX
    return DISCLOSURE_URL_OPEN + "%s%s][color=#%s]%s %s[/color][/url]" % [
        HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX, String(st.get("key", "")),
        caret_hex, key, caret,
    ]

## Split a "Key: value" data line into [key, value]; returns [] for sentence lines (trailing period),
## long keys, or non-matching text so those stay full-width rather than becoming a lopsided table row.
static func _split_kv(line: String) -> Array:
    if line.ends_with("."):
        return []
    # The recovery-guidance line reads as a dim sentence, not a lopsided table row.
    if line.begins_with(RECOVERY_GUIDANCE_GLYPH):
        return []
    var idx := line.find(DETAIL_KV_SEPARATOR)
    if idx <= 0:
        return []
    var key := line.substr(0, idx)
    if key.length() > DETAIL_KEY_MAX_LENGTH:
        return []
    var value := line.substr(idx + DETAIL_KV_SEPARATOR.length())
    if value.strip_edges() == "":
        return []
    return [key, value]


# =====================================================================================
#  ROW LABELS + THEIR VALUE TINTS
#  Each pair is "how the row READS" beside "what colour that reading is", so a label tweak and its
#  tint guard can never drift apart.
# =====================================================================================

## **IS THIS GROUND A GATHERING SITE — i.e. can anyone work it at all?** The sim's plant rungs 1–3
## all carry `requires_gathering_site` (`intensification_ladder.json`), so this one predicate answers
## whether Forage, Cultivate and Sow are available here, and therefore whether the card may speak in
## the human-food vocabulary at all.
##
## **Gated on the module KEY, never its label** — a tile with no site still ships the label `"None"`,
## which would read as a site called "None". Same test `SelectionCardController._land_row_meta` uses
## for its `No forage` meta and `DrawerComposeController._forage_compose_available` for the Assign
## button; it lives here so the three cannot drift, which is exactly how issue #464 happened — the
## drawer rendered a full stand on ground the other two had already declared unworkable.
##
## **The wire only ever carries the curated sites** (`foodModules` ← `FoodSiteRegistry`), so presence
## IS the answer; there is no "has a module but is not a site" case to distinguish client-side.
static func tile_is_gathering_site(tile_info: Dictionary) -> bool:
    return String(tile_info.get("food_module", "")).strip_edges() != ""

## **THE ONE ANSWER TO "WHAT FORAGE CAPACITY DOES THIS CARD SHOW"** — the patch's live ceiling where
## the player can see it, the tile's own ground `K` where they only remember it. `0.0` where the
## ground carries no patch at all, which is every caller's "print no row" test.
##
## **The two keys are not interchangeable and the pick is the fog rule** (`MapView.FOW_DISCOVERED_HIDDEN_KEYS`
## header). `patch_carrying_capacity` is the tile's `K` times the interpolated `field_capacity_gain`,
## so it carries the ladder position and is REDACTED on a Discovered hex; `patch_tile_capacity` is
## terrain and survives. Presence therefore IS the visibility here — a card that gets the ceiling is
## looking at the patch, one that does not falls back to the ground beneath it.
##
## **NOBODY ELSE MAY WRITE THIS `or`.** The stock row and the basket's capacity guard both need it,
## one level apart, and two sites answering one question is precisely how this card ends up stating a
## ceiling its own decomposition disagrees with. It is a ROW reader only: the harvest-floor arithmetic
## takes `patch_carrying_capacity` straight, because a floor is a fraction of the stand ACTUALLY
## standing here and that instrument does not render on a hex the player cannot see.
static func patch_capacity(tile_info: Dictionary) -> float:
    if tile_info.has("patch_carrying_capacity"):
        return float(tile_info["patch_carrying_capacity"])
    return float(tile_info.get("patch_tile_capacity", 0.0))

## In-sight reads LIVE, both unseen states read remembered. The one test behind both the row's BBCode
## hex and the chip's Color, so the two forms cannot drift apart.
static func sight_is_live(value: String) -> bool:
    return value == HudConst.TILE_SIGHT_ACTIVE

## Value tint for the Sight row: in-sight reads live (SIGNAL cyan — the HUD's "this is current"
## color), while both unseen states read dim (INK_DIM). The row states what you KNOW, not what is
## wrong, so it never borrows the WARN/DANGER palette.
static func sight_value_hex(value: String) -> String:
    return HudStyle.SIGNAL_HEX if sight_is_live(value) else HudStyle.INK_DIM_HEX

## Player-facing label for a herd's / patch's / pasture's ecology phase. Stressed/Collapsing carry a
## warning glyph; `detail_bbcode` additionally tints the value (see `ecology_value_hex`).
static func ecology_phase_label(phase: String) -> String:
    match phase:
        "collapsing":
            return "⚠ Collapsing"
        "stressed":
            return "⚠ Stressed"
        "thriving":
            return "Thriving"
        _:
            return phase.capitalize()

## BBCode hex for an "Ecology" value: red for a collapsing group, amber for stressed, normal ink
## otherwise. Matched on the lowercased phase stems ("collaps"/"stress" from `EcologyPhase::as_str`)
## so tinting survives glyph/capitalization tweaks to the label.
static func ecology_value_hex(value: String) -> String:
    var normalized := value.to_lower()
    if normalized.contains("collaps"):
        return HudStyle.DANGER_HEX
    if normalized.contains("stress"):
        return HudStyle.WARN_HEX
    return HudStyle.INK_HEX

## The same phase as a green/amber/red TIER COLOR, for a surface that paints rather than tints text:
## the roster's vitality dot and the harvest-floor chart's standing-stock band. It differs from
## `ecology_value_hex` in exactly one place and deliberately — a healthy phase reads HEALTHY here
## (a dot/band says "how is it doing?") where a detail VALUE reads plain ink (nothing is wrong).
static func ecology_tier_color(phase: String) -> Color:
    var normalized := phase.strip_edges().to_lower()
    if normalized.contains("collaps"):
        return HudStyle.DANGER
    if normalized.contains("stress"):
        return HudStyle.WARN
    return HudStyle.HEALTHY

## Append the Predators combat-component rows (Attack / Defense / Fights back / Aggressive) plus the
## compact derived-danger summary. Attack + Defense are open-ended, so their bars normalize against
## the max across the KNOWN herds, Elevation-style — a herd reads relative to the roster, and falls
## back to a full bar if it IS the reference (no other herds, or it holds the max). Ferocity +
## Aggression are native 0..1 → bar + %, using the readable behaviour labels the player parses.
##
## `world_herds` is THREADED IN rather than reached for: this layer holds no snapshot state, so the
## roster it normalizes against must be the caller's (`HudBandLaborState.world_herds()` today).
static func append_danger_component_lines(lines: Array[String], herd_data: Dictionary, world_herds: Array) -> void:
    var attack := float(herd_data.get("attack", 0.0))
    var defense := float(herd_data.get("defense", 0.0))
    var ferocity := clampf(float(herd_data.get("ferocity", 0.0)), 0.0, 1.0)
    var aggression := clampf(float(herd_data.get("aggression", 0.0)), 0.0, 1.0)
    # Defense LEADS and stands flat: it is not in either product below. See `DANGER_COMPONENT_INDENT`.
    lines.append("%s: %s" % [DANGER_DEFENSE_ROW, _danger_open_row(defense, "defense", world_herds)])
    # THE ANSWER, THEN ITS WORKING. The derived line used to sit LAST, under four rows of equal
    # weight, so a card stated four inputs and a conclusion in one undifferentiated column. Leading
    # with it and indenting its factors also makes the arithmetic nearly readable off the page: attack
    # is in both terms, and the other two split them.
    lines.append("%s: %s" % [DANGER_DERIVED_ROW, DANGER_DERIVED_FORMAT % [
        _format_danger_scalar(attack * ferocity), _format_danger_scalar(attack * aggression),
    ]])
    lines.append("%s%s: %s" % [DANGER_COMPONENT_INDENT, DANGER_ATTACK_ROW,
        _danger_open_row(attack, "attack", world_herds)])
    lines.append("%s%s: %s" % [DANGER_COMPONENT_INDENT, DANGER_FEROCITY_ROW,
        _danger_unit_row(ferocity)])
    lines.append("%s%s: %s" % [DANGER_COMPONENT_INDENT, DANGER_AGGRESSION_ROW,
        _danger_unit_row(aggression)])

## An OPEN-ENDED component (attack/defense): a bar relative to the roster max + the raw value. The bar
## normalizes against the biggest value of that component across `world_herds`; with no reference (max
## 0 / no herds) it degrades to the bare value with no bar, since a lone herd has nothing to compare to.
static func _danger_open_row(value: float, key: String, world_herds: Array) -> String:
    var reference := _world_herd_component_max(key, world_herds)
    var raw := _format_danger_scalar(value)
    if reference <= 0.0:
        return raw
    return "%s %s" % [HudFormat.meter_bar(value / reference * 100.0, DANGER_BAR_CELLS), raw]

## A NATIVE 0..1 component (ferocity/aggression): a bar + percent.
static func _danger_unit_row(value: float) -> String:
    return "%s %d%%" % [
        HudFormat.meter_bar(value * HudConst.PROGRESS_PERCENT_SCALE, DANGER_BAR_CELLS),
        int(round(value * HudConst.PROGRESS_PERCENT_SCALE)),
    ]

## The largest value of an open-ended combat component across the known herds — the reference the
## Attack/Defense bars normalize against (the Elevation-view idiom for an unbounded field).
static func _world_herd_component_max(key: String, world_herds: Array) -> float:
    var reference := 0.0
    for herd in world_herds:
        if herd is Dictionary:
            reference = maxf(reference, float((herd as Dictionary).get(key, 0.0)))
    return reference

## Format a combat scalar for display: whole numbers bare (`8`), fractions to one decimal (`0.5`),
## trailing zero stripped — the components read against the human-strength anchor of 1.0.
static func _format_danger_scalar(value: float) -> String:
    if is_equal_approx(value, round(value)):
        return "%d" % int(round(value))
    return String.num(value, 1)

## Tile-count label for a herd's grazing range from its hex radius — "the ground this herd grazes".
## The hex-disk count `1 + 3r(r+1)`: radius 0 → 1 tile (small game, its own hex), 1 → 7, 2 → 19. Same
## count the map ring draws, so the readout and the ring can never disagree. Singular for a lone tile.
static func graze_range_label(range_radius: int) -> String:
    var tiles := 1 + 3 * range_radius * (range_radius + 1)
    if tiles == 1:
        return "1 tile"
    return "%d tiles" % tiles

## **THE BUILD METER'S VALUE — one spelling for both webs' four rungs** (`docs/plan_unit_costed_work.md`
## §11): `Preparing 18 / 50 work (42%)`. The four `*_label` functions below compose through it, so a
## plant rung and an animal rung state a job's size the same way.
##
## **THE PERCENTAGE IS THE CALLER'S, NEVER `work_done / work_cost`.** The wire ships the fraction and
## the two absolutes as separate fields and they are exactly each other; dividing here would be a
## second authority over one meter, and the first turn the two disagreed the row would say so.
##
## A `work_cost` at `BUILD_WORK_COST_NONE` states the percentage alone — the row this always was. That
## is a source the wire prices no such job on, not a missing field: `18 / 0 work` reads as a defect
## where a bare percentage reads as an unpriced job.
static func build_meter_value(verb: String, progress: float,
        work_done: float, work_cost: float) -> String:
    if work_cost <= SourceForecast.BUILD_WORK_COST_NONE:
        return HudSelectionVocab.BUILD_METER_PERCENT_FORMAT % [
            verb, HudFormat.progress_percent(progress)]
    return HudSelectionVocab.BUILD_METER_WORK_FORMAT % [
        verb, format_work_units(work_done), format_work_units(work_cost),
        HudFormat.progress_percent(progress)]

## **THE RUNG ROW'S WHOLE VALUE — one composer, four rungs, both webs** (issue #545). The tile card's
## plant rungs and the herd drawer's animal ones render through this and nothing else, so a rung's
## state cannot be worded one way on a patch and another on a herd.
##
## **IT IS ONE ROW, AND THAT IS THE POINT.** A rung used to cost four lines here — the meter in work
## units, an indented turn estimate, a `Keepers` head count and a `Keeping` sentence — and the last
## two were reported from play as unreadable: both existed to say *there is nothing to do*, and both
## said the same number twice. What a glance wants is how long, or how much is at stake.
##
## **THE ABSENCE OF A HAZARD IS NOW THE ONLY SIGNAL THAT THINGS ARE FINE**, so every failure state
## below carries `RUNG_HAZARD_GLYPH` and none may render bare. Four of them, in the order they are
## tested:
##
## | state | reads | why it is not the one above it |
## |---|---|---|
## | built | `🌾 Tended 100%` (+ `⚠` when the keeping is short) | achievement is the stamped retention bar, not the meter's fullness |
## | declared, nobody on it, nothing banked | `⚠ Not started — no builders assigned` | there is no meter to state — `0 / 50 (0%)` is that zero written three ways |
## | **work banked, nobody on it, keeping COVERS it** | **`Held at 42%` — NO MARK, NEUTRAL INK** | **it is a player's decision, not a failure** |
## | banked work on a rung that is NOT the one in flight | `Held at 42%`, or `⚠ Reverting 42%` if the keeping is short | the source's ONE countdown is about the OTHER rung |
## | a crew on it banking exactly the ROT | `⚠ ∞ turns (42%)` | somebody IS on it and their turn is being wasted; the remedy is MORE of them |
## | under the rot, staffed or not | `⚠ ∞ turns, losing ground (42%)` | work already bought is going back — so it is RED, not amber |
## | the builders standing on it, its gate refusing | `⚠ Blocked 42% — your builders are held here` | the whole QUEUE is stuck, not just this rung, and the remedy is off the build line entirely (`build_blocked_lines`) |
## | staffed, and nothing accrues anyway | `⚠ Stalled 42%` | a gate or an empty escapement room, which no crew size fixes |
## | otherwise | `≈11 turns (42%)` | the healthy reading, and the only one with no mark |
##
## **THE HELD ROW IS THE ONE STATE HERE THAT IS NOT A FAILURE, AND THAT IS WHY IT MATTERS**
## (`docs/plan_standing_upkeep.md` §4.6a). Parking a half-built improvement — take the builders off a
## Cultivate at 50%, leave the keeping staffed — holds it there indefinitely, and marking it teaches
## the player to ignore the mark, which costs every other row above its meaning. It says `Held` in
## WORDS rather than `∞ turns`, because `∞` is a statement about a crew and there is no crew here.
##
## **IT REPLACED `⚠ Reverting 42%` ON THE RUNG IN FLIGHT.** That row fired on *work banked and nobody
## on it* — a client-side inference that a parked meter must be bleeding, true only while an unbuilt
## rung was billed to its build crew. The wire answers it now for the meter it is about (`-2` held,
## `-3` losing), so the rotting row covers the half that really is a loss and this covers the half that
## is not. **`build_crew` is not a second opinion about that**: it is the one fact `BUILD_METER_HOLDS`
## cannot carry, and it chooses only between two wordings of one sentinel.
##
## > #### ⛔ THE COUNTDOWN IS PER SOURCE AND THE CARD HAS TWO ROWS
## >
## > `buildTurnsRemaining` describes **one** rung — whichever `build_verb` names — and so does
## > `meterRotPerTurn`, and the two need not even be the same rung: a Cultivate abandoned at 60% with a
## > `Sow` declared over it puts the ROT on the Cultivate and the COUNTDOWN on the Field. A row that is
## > not the rung in flight therefore has no countdown of its own, and printing the source's would put
## > the Field's `≈30 turns` on a Cultivation meter nobody is touching — **reported by review, and the
## > same routing mistake the built row's `⚠` had**.
## >
## > So such a row states what it IS rather than a number that is not its own: `Held at 42%` where the
## > keeping covers it, and **`RUNG_REVERTING_FORMAT`'s `⚠ Reverting 42%` where it does not**. That
## > format was retired with the client-side sliding inference and had to come back for exactly this:
## > the sim's `-3` replaced it **for the at-risk meter only**, and nothing replaced it for the other
## > row. The fork is `rung_is_under_kept` — the published shortfall routed through `at_risk_rung` —
## > which is the same seam the built row's mark uses and derives no number of its own.
## >
## > **`declared_rung` is a STRING rather than the bool it replaced** so both facts a row needs come
## > from one place: *is this the rung the player declared* (the unstarted row) and *is this the rung
## > in flight* (`build_verb`'s own answer, which honours a declaration only at a zero meter). Two
## > separately-passed bools could disagree; one string cannot.
##
## **`built` IS THE ACHIEVEMENT FLAG, NEVER `progress >= 1`**, and the two genuinely differ: a rung
## that has eroded to 92% is still tended AND is being repaired, which is why fullness and achievement
## stay orthogonal (`SourceForecast.build_verb`'s own note). Passing the meter here would make a
## rung's LOSS and a rung's REPAIR one edge.
##
## **THE BUILT ROW'S `⚠` IS ROUTED TO THE AT-RISK RUNG, NOT PAINTED ON EVERY BUILT ONE** (§4.6a).
## `is_under_kept` answers for the SOURCE — one pool, one shortfall — and `rung_is_under_kept` is what
## puts that answer on the row it belongs to: **only one meter on a source is ever at risk**, the
## newest one carrying work, which is what the published shortfall is resolved through
## (`SourceForecast.at_risk_rung`). A patch mid-Sow is billed for the FIELD, so a mark on the tended
## row beneath would point the player at ground that is fine.
##
## **THE ROUTING USED TO BE ACCIDENTAL, WHICH IS WHY IT HAD TO BECOME DELIBERATE.** The test carried a
## `build_is_in_flight` gate — there to keep the mark off a source whose bill the BUILDERS owed — and
## it was incidentally suppressing the built row on exactly this shape. The pooled keeping deleted that
## gate's own reason, and with it gone nothing routed the mark at all. **It decides which ROW shows a
## number, never the number**, which is the same job `build_verb` already does for the build verb, off
## the same table and the same newest-first walk.
##
## `built_label` is the rung's own badge, glyph included, and is the caller's because one of them
## forks on something no rung shares — a penned herd that is starving states that instead.
static func rung_row_value(src: Dictionary, prefix: String, improvement: String, kind: String,
        built_label: String, built: bool, progress: float, build_crew: int,
        declared_rung: String) -> String:
    var percent := HudFormat.progress_percent(progress)
    if built:
        var face := HudSelectionVocab.RUNG_BUILT_FORMAT % [built_label, percent]
        if SourceForecast.rung_is_under_kept(src, prefix, kind, improvement):
            # **A BARE `⚠` WAS A MARK WITH NO WORD, and the three lines that used to explain it are
            # gone** (the `At risk:` retirement above). So the state joins the meter: `slipping` on the
            # plant web, `drifting` on the animal one — the two webs' own consequence, in the same
            # words the work board's note has always used, and a STATE rather than a sentence because
            # the sentence is the row's hover.
            return HudSelectionVocab.RUNG_UNDER_KEPT_FORMAT % [face,
                HudSelectionVocab.RUNG_HAZARD_GLYPH, rung_under_kept_word(kind)]
        return face
    if declared_rung == improvement and progress <= BUILD_METER_EMPTY:
        return BUILD_UNSTARTED_VALUE
    # **A ROW THAT IS NOT THE RUNG IN FLIGHT MAY NOT PRINT THE COUNTDOWN**, because there is exactly
    # ONE of those per source and the card has two rows. See the note above.
    if SourceForecast.build_verb(src, prefix, kind, declared_rung) != improvement:
        if SourceForecast.rung_is_under_kept(src, prefix, kind, improvement):
            return HudSelectionVocab.RUNG_REVERTING_FORMAT % [
                HudSelectionVocab.RUNG_HAZARD_GLYPH, percent]
        return HudSelectionVocab.RUNG_HELD_FORMAT % percent
    return build_countdown_value(SourceForecast.build_turns_remaining(src, prefix),
        build_crew, percent)

## **THE REMEDY, ON THE HOVER OF THE ROW THAT IS SLIPPING** — the whole of what replaced the `At risk:`
## row and its indented instruction. Registers `HudWorkVocab.under_kept_tooltip_for_source` against the
## rung row's own key and NOTHING else: no shortfall, no countdown, because a card cannot act on either
## (the work board takes the countdown — see that producer's flag).
##
## Silent on a rung whose keeping is paid, which is every rung on every calm card in the game, and
## silent for a caller that passes no context — so a host that renders these lines without one gets
## exactly the BBCode it always did.
static func note_under_kept_hover(ctx: Context, row_key: String, src: Dictionary, prefix: String,
        kind: String, improvement: String) -> void:
    if ctx == null or not SourceForecast.rung_is_under_kept(src, prefix, kind, improvement):
        return
    # **AND WHEN THE MISSING THING IS A GOOD, THE HOVER NAMES IT** (`docs/plan_standing_upkeep.md`
    # §2.7) — the card's half of the work row's third arm, read off the SOURCE's own published
    # material pair rather than a labor row's copy, because a card has no assignment in hand. The
    # remedy differs in kind, so the sentence must: no staffing stepper mends a fence with no
    # hurdles. `""` on every rung that eats no material, which falls straight back to the role
    # sentence.
    ctx.row_tooltips[row_key] = HudWorkVocab.under_kept_tooltip_for_source(kind,
        HudWorkVocab.material_short_note_for_source(kind,
            SourceForecast.upkeep_material_demand(src, prefix),
            SourceForecast.upkeep_material_supplied(src, prefix)))

## **WHAT TAMING IS BUYING, ON THE HUSBANDRY ROW ITSELF** — the ceiling, the best breeding rate and the
## sustainable yield, the three things a rung on this ladder actually moves.
##
## **IT BELONGS HERE AND NOT ON THE ASSIGN-HERDERS SHEET.** That panel answers *how many hands, at what
## floor, for what this turn*; what the LADDER buys is a property of the herd, true whether or not
## anybody is composing an assignment against it, and the sheet's Work zone has neither the height nor
## the width to carry three more readings. Collapsed into the row's hover it costs the card nothing.
##
## **ALL THREE CLIMB WHILE THE TAME RUNS**, which is the point of stating them together: the take falls
## during a build because the floor beneath it is rising, and a player reading the take alone reads
## that as the herd being poor. The rising CLAUSE is stated only while `buildTurnsRemaining` says the
## climb is under way; no magnitude, the wire publishing no next-turn capacity to quote.
##
## `""` — and no hover — for a herd whose curve or body the wire did not describe, which is the same
## silence every other derived reading on this card keeps.
const HUSBANDRY_PAYOFF_HEADING := "What taming is buying"
const HUSBANDRY_PAYOFF_CEILING_FORMAT := "Ceiling %s"
## **THE BREEDING LINE IS A RATE, AND IT IS ROUNDED AS ONE** (`DetailFormat.animal_rate_face`), which
## is what the two `%s` are: the fractional head count and the species. It read
## `HUSBANDRY_PAYOFF_BREEDING_FORMAT % SourceForecast.stock_face(...)` and was wrong twice over —
## `stock_face` carries its own `≈`, so every herd this hover has ever appeared on rendered
## `Breeds back up to ≈≈3 Red Deer a turn`; and `stock_face` floors the count at one body, which is
## right for a STANDING herd and a lie about a per-turn curve (a mammoth on a range peaking at 50
## biomass a turn read `≈1 Mammoth`, eight times the truth). The sustainable line below rounds the
## same curve at the same two decimals, so one hover no longer states one curve two ways.
const HUSBANDRY_PAYOFF_BREEDING_FORMAT := "Breeds back up to ≈%s %s a turn"
## **AND ITS RATE IS `format_signed`, NOT `format_yield`** — the rule `SourceForecast`'s own
## `YIELD_TOOLTIP_RATES_FORMAT` states: the unit is carried by the WORDS (*a turn*), so the `/turn`
## suffix printed it twice in four words (`Sustainable +1.74 /turn a turn at the best-harvest floor`).
## The MAGNITUDE is untouched — both formatters round at `YIELD_DECIMALS`, which is the half of
## `format_yield` this line was reaching for.
const HUSBANDRY_PAYOFF_SUSTAINABLE_FORMAT := "Sustainable %s a turn at the best-harvest floor"
const HUSBANDRY_PAYOFF_CLIMBING := "All three are climbing while the taming runs."
## **…AND WHERE THE CEILING STOPS CLIMBING**, on a herd whose destination the wire states — the line
## above with the number the player is actually buying folded into it, rather than a second line
## beneath it repeating its subject.
##
## **IT NAMES THE CEILING, THE RUNG AND THE GROUND, in that order, because all three are the reading.**
## `buildDestinationCapacity` is this range's `K` at the rung the build was sent to, and it is struck
## on the land AS IT STANDS TODAY (the rung moves, the land does not) — so it drifts turn to turn
## exactly as the live `Ceiling` two lines above it does. *"would carry"* and *"as it stands today"*
## are what keep it a reading of the present rather than a promise about the future; the sim quotes
## no date and neither may this.
##
## **NOTHING IS SAID ABOUT THE OTHER TWO.** The breeding rate and the sustainable yield climb with the
## ceiling, but the wire quotes a destination for the ceiling ALONE, so the sentence names the one
## figure it has and leaves the others to the clause they already share.
const HUSBANDRY_PAYOFF_DESTINATION_FORMAT := \
        "All three are climbing while the taming runs: %s would carry %s on this ground as it stands today."
static func husbandry_payoff_hover(herd_data: Dictionary, prefix: String) -> String:
    var body_mass := float(herd_data.get(prefix + SourceForecast.FORECAST_BODY_MASS_KEY, 0.0))
    var capacity := float(herd_data.get(prefix + SourceForecast.FORECAST_CAPACITY_KEY, 0.0))
    var samples := SourceForecast.regrowth_samples(herd_data, prefix)
    if body_mass <= 0.0 or capacity <= 0.0 or not SourceForecast.has_growth_curve(samples):
        return ""
    var quarry := SourceForecast.herd_display_name(herd_data)
    # The BEST the curve ever pays, at the stock fraction it pays it at — the herd's own peak, not the
    # rate at whatever floor a sheet happens to be composing. That is what a rung raises.
    var peak_biomass := maxf(SourceForecast.regrowth_at(samples,
        SourceForecast.growth_peak_fraction(samples)), 0.0)
    var lines: Array[String] = [
        HUSBANDRY_PAYOFF_HEADING,
        HUSBANDRY_PAYOFF_CEILING_FORMAT % SourceForecast.stock_face(capacity, body_mass, quarry),
        HUSBANDRY_PAYOFF_BREEDING_FORMAT % [animal_rate_face(peak_biomass / body_mass), quarry],
    ]
    # …and what that regrowth is worth in the store, at the floor the ladder is actually run at. A
    # species that pays no food states no such line rather than a zero.
    var provisions := float(herd_data.get(
        prefix + SourceForecast.FORECAST_PROVISIONS_PER_BIOMASS_KEY, 0.0))
    if provisions > 0.0:
        lines.append(HUSBANDRY_PAYOFF_SUSTAINABLE_FORMAT % SourceForecast.format_signed(
            maxf(SourceForecast.regrowth_at(samples, SourceForecast.FLOOR_FOOD_PEAK), 0.0)
                * provisions))
    if int(herd_data.get(prefix + SourceForecast.FORECAST_BUILD_TURNS_KEY,
            SourceForecast.BUILD_TURNS_NONE_TO_STATE)) > SourceForecast.BUILD_TURNS_NONE_TO_STATE:
        lines.append(husbandry_payoff_climbing_line(herd_data, prefix, body_mass, quarry))
    return "\n".join(lines)

## The climbing line's two faces — with the destination ceiling where the wire states one, without it
## where it does not. **A source no band has queued renders NO CLAUSE AT ALL rather than a zero**: a
## range really can carry nothing, so the sentinel is the only thing that can say *there is nowhere
## this is heading*, and `states_destination_capacity` is the one test that reads it.
##
## The rung is named through `rung_badge_word`, the same table the card's own badges use, so the
## sentence and the badge beneath it cannot call one rung two things. An unnameable rung falls back to
## the bare climbing line for the same reason the chart's flag does: a figure with nothing to anchor
## it to is the bare second number this arc keeps refusing to print.
static func husbandry_payoff_climbing_line(herd_data: Dictionary, prefix: String, body_mass: float,
        quarry: String) -> String:
    var destination := SourceForecast.build_destination_capacity(herd_data, prefix)
    var rung := rung_badge_word(SourceForecast.build_destination_rung(herd_data, prefix))
    if not SourceForecast.states_destination_capacity(destination) or rung.is_empty():
        return HUSBANDRY_PAYOFF_CLIMBING
    return HUSBANDRY_PAYOFF_DESTINATION_FORMAT % [rung,
        SourceForecast.stock_face(destination, body_mass, quarry)]

## **WHAT AN UNDER-KEPT RUNG IS DOING, IN ONE WORD, PER WEB** — the state that rides the built row's
## `⚠`. The pair is the work board's two notes reduced to their verb (`WORK_ROW_UNDER_KEPT_NOTE` says
## *this ground is slipping*, `WORK_ROW_UNDER_HERDED_NOTE` *animals drifting off*), so the card's word
## and the board's sentence cannot describe two different failures.
static func rung_under_kept_word(kind: String) -> String:
    return HudSelectionVocab.RUNG_UNDER_KEPT_ANIMAL_WORD \
        if kind == SourceForecast.SOURCE_KIND_HERD \
        else HudSelectionVocab.RUNG_UNDER_KEPT_PLANT_WORD

## **A RUNG'S BADGE WORD, KEYED BY ITS IMPROVEMENT** — `Tended`, `Field`, `Domesticated`, `Corralled`,
## the four `*_built_label` words with their glyphs stripped. The countdown sentence names the rung it
## is counting down (`Tended is lost in 3 turns.`) and a glyph in a sentence reads as a typo, so this
## is the badges' own words rather than a fifth spelling of them. `""` for a rung with no badge —
## `IMPROVEMENT_NONE`, and anything the ladder gains before this table does — which is the caller's cue
## to state the remedy without a countdown rather than to count down an unnamed thing.
static func rung_badge_word(improvement: String) -> String:
    match improvement:
        SourceForecast.IMPROVEMENT_CULTIVATE: return CULTIVATION_BUILT_WORD
        SourceForecast.IMPROVEMENT_SOW: return FIELD_BADGE_LABEL
        SourceForecast.IMPROVEMENT_TAME: return HUSBANDRY_BUILT_WORD
        SourceForecast.IMPROVEMENT_CORRAL: return CORRAL_BUILT_WORD
    return ""

## **THE SENTINEL FORK, ON ITS OWN — the countdown half of `rung_row_value`.** Everything above it in
## that function is ROUTING (which of a card's two rows may print the source's one countdown, and
## whether this row is a built badge or a bare declaration); this is the part that reads the wire's
## `buildTurnsRemaining` and answers for every value it can carry — a positive count, `-2` holding,
## `-3` rotting, `-4` the queue blocked, `-5` queued and not yet estimated, `-1` no answer.
##
## **IT IS EXTRACTED RATHER THAN COPIED, and that is the whole point.** The BUILD QUEUE block's date
## column asks exactly this question and has none of the routing problem — a queue entry IS the rung,
## so there is no second row to misattribute a number to. A second fork there would be a second place
## for a newly-spelled sentinel to be missed, which is the mistake this family has already made twice
## (`-3` split out of `-2`, then `-4` added beside them, each time with a reader left behind).
##
## **IT TAKES THE COUNTDOWN, NOT THE SOURCE**, so a caller that has already read the wire's value —
## the work board's models do, once per render — hands it straight over rather than reaching into the
## dict again. `SourceForecast.build_turns_remaining` is still the ONE reader of the field and still
## the one place an unrecognised negative is normalised.
##
## `percent` is the meter's own fullness, supplied by the caller because the two callers read it from
## different places — a rung row from its own meter, a queue row from the entry's.
static func build_countdown_value(turns: int, build_crew: int, percent: int) -> String:
    var sentinel := build_sentinel_value(turns, build_crew, percent)
    if sentinel != "":
        return sentinel
    if turns == BUILD_TURNS_SINGULAR:
        return HudSelectionVocab.RUNG_TURNS_ONE_FORMAT % percent
    return HudSelectionVocab.RUNG_TURNS_FORMAT % [turns, percent]

## **THE SAME VALUE AS A COMPLETION DATE — the turn this entry is estimated to LAND on**
## (`docs/plan_standing_upkeep.md` §4.7). `turn 82 (0%)` rather than `≈42 turns (0%)`.
##
## **IT IS THE BUILD QUEUE BLOCK'S ALONE, AND THAT IS THE WHOLE DISTINCTION.** The queue is a
## SCHEDULE whose order is the player's own input — its counts are CHAINED, so `≈42` / `≈61` / `≈98`
## down a list read as three independent spans when they are cumulative, which is what made them
## ambiguous in play. A rung row and a compose sheet answer *what does this cost me*, which is a
## DURATION and has no chain to be misread, so both keep `build_countdown_value`.
##
## **NO SINGULAR FORK.** `turn 41 (0%)` is correct at one turn out, so `BUILD_TURNS_SINGULAR` — which
## exists to keep `≈1 turns` off a countdown — has nothing to do here.
##
## `current_turn` is `HudBandLaborState.current_turn()`, threaded in because this layer holds no
## snapshot.
##
## **`leg` NAMES THE RUNG THE PERCENTAGE IS ABOUT, and on a two-leg entry that is NOT the row's
## title** (`docs/plan_standing_upkeep.md` §2.8). `percent` is the leg in flight's fullness, so the
## face leads with that leg's participle — `Cultivating 18% · turn 83` under a row titled `Sow`.
## Without the verb the same number is worse than the `0%` it replaces: the reader attributes it to
## the destination the title names.
##
## **THE SENTINELS TAKE NO VERB, and that is not an oversight.** Each names a STATE of the ENTRY —
## blocked, stalled, holding, rotting — which is a fact about the whole climb rather than about one
## leg, and a hazard face that also carried a participle would put two subjects on a 118px column.
## They still state the leg's `percent`, which is the half of this fix that applies to every face.
##
## `""` — or a rung with no participle — renders the bare dated face, which is what every caller
## emitted before a leg was in the question.
static func build_completion_value(turns: int, build_crew: int, percent: int,
        current_turn: int, leg: String = "") -> String:
    var sentinel := build_sentinel_value(turns, build_crew, percent)
    if sentinel != "":
        return sentinel
    var verb := String(HudComposeVocab.IMPROVEMENT_RUNNING_LABELS.get(leg, ""))
    if verb == "":
        return HudSelectionVocab.RUNG_COMPLETES_FORMAT % [current_turn + turns, percent]
    return HudSelectionVocab.RUNG_COMPLETES_LEG_FORMAT % [verb, percent, current_turn + turns]

## **THE SENTINEL BRANCHES ON THEIR OWN, so the two faces above cannot fork twice.** It answers `""`
## for a real positive count — the one case the two callers word differently — and every sentinel the
## wire can put on `buildTurnsRemaining` otherwise. **A second fork is how this client has twice been
## left behind by a newly-spelled sentinel** (`-3` split out of `-2`, then `-4` added beside them), so
## a caller that wants a new rendering of a COUNT writes it in terms of this rather than beside it.
## **`-5` IS THE THIRD TIME**, and it landed INSIDE this fork for exactly that reason.
static func build_sentinel_value(turns: int, build_crew: int, percent: int) -> String:
    if turns == SourceForecast.BUILD_TURNS_HOLDS:
        if build_crew <= SourceForecast.BUILD_CREW_NONE:
            return HudSelectionVocab.RUNG_HELD_FORMAT % percent
        return HudSelectionVocab.RUNG_HOLDING_FORMAT % [
            HudSelectionVocab.RUNG_HAZARD_GLYPH, BUILD_TURNS_NEVER_GLYPH, percent]
    if turns == SourceForecast.BUILD_TURNS_ROTS:
        return HudSelectionVocab.RUNG_ROTTING_FORMAT % [
            HudSelectionVocab.RUNG_HAZARD_GLYPH, BUILD_TURNS_NEVER_GLYPH,
            HudSelectionVocab.RUNG_ROTTING_PHRASE, percent]
    # **THE QUEUE IS STUCK ON THIS ENTRY** (`docs/plan_standing_upkeep.md` §4.6b) — the band's
    # builders are staffed and standing here and this rung's own gate refuses them, so nothing banks
    # and nothing behind it moves. It wears NO `∞`: that mark is a statement about a crew's
    # arithmetic, and no crew size is the remedy here. The remedy is `build_blocked_lines`' sub-row
    # beneath, paired with the shortfall this same row already publishes.
    if turns == SourceForecast.BUILD_TURNS_QUEUE_BLOCKED:
        return HudSelectionVocab.RUNG_BLOCKED_FORMAT % [
            HudSelectionVocab.RUNG_HAZARD_GLYPH, percent]
    # ⛔ **AND `-1` TAKES NO CREW FORK, unlike `BUILD_TURNS_HOLDS` above.** It is the tempting symmetry
    # and it is wrong twice over: `RungDef::build_accrual`'s `eligible` reads the STOCK against the
    # floor and takes no crew count at all, so the sim answers `-1` on a refused gate at ANY staffing —
    # and gating the client's own answer on a crew is a defect this client already shipped once and
    # fixed (`chapters/improvements.gd`'s `tile_meter_stalled`, where the sheet answered the neutral
    # *held* while the card said `⚠ Stalled`: two producers disagreeing about one meter).
    # **THE SIM HAS NOT LOOKED AT THIS ENTRY YET** (`SourceForecast.BUILD_TURNS_NOT_YET_ESTIMATED`,
    # `docs/plan_standing_upkeep.md` §4.9). Queued since the last turn resolved, so no estimate pass
    # has run over it — which is a different fact from `-1`'s *the pass ran and had no number*, and it
    # is the one the branch below used to swallow.
    #
    # ⛔ **NO HAZARD GLYPH AND NO CREW FORK.** Nothing is wrong: a build one command old with a staffed
    # pool on it is not a stall, and folding it into `-1` put `⚠ Stalled 0%` on a fresh `Cultivate`
    # until the next turn cleared it. The crew is irrelevant for the same reason it is on `-1` — this
    # says what the SIM has done, not what a crew is doing — so a staffed and an unstaffed fresh entry
    # read alike here, and whether anybody is on it is the queue's own head/tail question.
    #
    # **IT IS TESTED BEFORE `-1`** so the two cannot be read as one by a future edit that widens either
    # test, and it is written HERE rather than beside this function: a caller wanting a new rendering
    # of a count writes it in terms of this fork, which is what stopped `-3` and `-4` being missed a
    # third time.
    if turns == SourceForecast.BUILD_TURNS_NOT_YET_ESTIMATED:
        return HudSelectionVocab.RUNG_QUEUED_FORMAT % percent
    if turns == SourceForecast.BUILD_TURNS_NO_ESTIMATE:
        return HudSelectionVocab.RUNG_STALLED_FORMAT % [
            HudSelectionVocab.RUNG_HAZARD_GLYPH, percent]
    return ""

## A meter with nothing banked on it at all — the boundary between *declared* and *under way*, and the
## one value at which a rung row states a sentence rather than a number.
const BUILD_METER_EMPTY := 0.0

## **THE ONE TINT RULE FOR ALL FOUR RUNG ROWS.** A value that says the meter is going BACKWARDS under a
## crew is red, any other value carrying the hazard mark is amber, a value carrying its rung's BUILT
## badge is signal green, and everything else is neutral ink — so the four `*_value_hex` leaves are one
## shape and a new hazard state cannot ship without its colour.
##
## **THE RULE IS WHY A NON-HAZARD STATE NEEDS NO LEAF OF ITS OWN.** `RUNG_QUEUED_FORMAT` (`-5`, queued
## and not yet estimated) carries neither needle, so it falls through to `INK_HEX` — the neutral it
## wants — without a branch. That is the rule working rather than a coincidence: a face that MEANT to
## be amber would have to earn it by wearing the hazard mark, which is the property this test exists
## to enforce in both directions.
##
## **THE ROTTING TEST RUNS FIRST BECAUSE THAT ROW WEARS BOTH NEEDLES.** It leads with the hazard mark
## like every other failure state — it must, or the mark stops meaning *something is wrong here* — so
## an amber branch tested first would swallow it and the schema's promised red/yellow split would exist
## in the wire and nowhere on screen.
##
## `built_needle` is the rung's own badge word, lowercased by the caller's own const, and
## `RUNG_ROTTING_PHRASE` is the same idea for the red: the row PRINTS the phrase this tests, so the
## words and the test cannot drift. The starving pen is the single case that outranks the mark, and it
## says so in its own leaf rather than here.
## **AN EMPTY `built_needle` MEANS THE CALLER HAS NO BUILT BADGE TO MATCH**, and it must be guarded
## rather than left to `contains`: every string contains the empty one, so an unguarded call would
## paint every value signal green. The BUILD QUEUE block's date column is that caller — a queue entry
## is by construction a rung that is NOT built, the sim pruning an entry off the queue when its meter
## fills — so it has no badge word and must not invent one.
static func rung_value_hex(value: String, built_needle: String) -> String:
    if value.contains(HudSelectionVocab.RUNG_ROTTING_PHRASE):
        return HudStyle.DANGER_HEX
    if value.contains(HudSelectionVocab.RUNG_HAZARD_GLYPH):
        return HudStyle.WARN_HEX
    if built_needle != "" and value.to_lower().contains(built_needle):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

## **THE `Color` TWIN OF THE RULE ABOVE, for a host that is a `Label` rather than BBCode** — the
## `BandFoodStatus.color_for_morale` / `hex_for_morale` pairing, and taken for the same reason: a
## `Label` takes an `add_theme_color_override` and can do nothing with a hex string.
##
## **IT IS WRITTEN IN TERMS OF `rung_value_hex`, never as a second fork**, so the ink a rung value
## takes is decided in exactly one place whichever kind of host is asking. `built_needle` defaults to
## empty for the caller that has no badge to match (see above).
static func rung_value_color(value: String, built_needle: String = "") -> Color:
    return Color.html(rung_value_hex(value, built_needle))

## A quantity of WORK UNITS: whole numbers bare (`50`), fractions to one place (`17.6`). One unit is
## one worker-turn at the food peak with no gear, so a cost reads itself — and the shipped costs are
## integers, which a trailing `.0` would dress up as a measured figure.
static func format_work_units(value: float) -> String:
    if is_equal_approx(value, round(value)):
        return "%d" % int(round(value))
    return String.num(value, HudSelectionVocab.BUILD_WORK_DECIMALS)

## Fixed-decimal, then trailing zeros AND a trailing dot stripped. `String.num` keeps a lone ".0", so
## format fixed and strip the tail ourselves (rstrip stops at the first non-matching char, so integer
## zeros survive).
static func format_trimmed(value: float, decimals: int) -> String:
    var text := ("%." + str(decimals) + "f") % value
    if "." in text:
        text = text.rstrip("0")
        if text.ends_with("."):
            text = text.rstrip(".")
    return text

## **A RATE IN ANIMALS PER TURN, AND THE ONE PLACE ONE IS ROUNDED** — `2.3`, `0.13`, `<0.01`. The
## compose sheet's take sentence, its band and its binding-limit line all read through here, and so
## does the herd card's payoff hover, so a fractional take and a fractional breeding rate cannot be
## rounded two different ways on two surfaces describing the same curve.
##
## **A POSITIVE RATE NEVER COMES BACK AS `0`** (`HudComposeVocab.HUNT_ANIMAL_RATE_MIN_SHOWN`). Two
## decimals cannot state a rate of `0.004`, and printing the rounded figure told a player that a crew
## which does eventually bring an animal down brings down none — the reported `≈0 WILD AUROCHS/TURN`.
## Under the floor the face becomes `<0.01`, which is a small number rather than an absence.
##
## **IT IS NOT `SourceForecast.stock_face`, and the difference is the whole reason it exists.** That
## one counts a STANDING herd and floors at one body, because a fifth of a body is still an animal on
## the map; a RATE has no such floor — a range that regrows a fifth of a mammoth a turn regrows a
## fifth of a mammoth, and rounding it up to one overstates the herd eightfold.
static func animal_rate_face(value: float) -> String:
    if value > 0.0 and value < HudComposeVocab.HUNT_ANIMAL_RATE_MIN_SHOWN:
        return HudComposeVocab.HUNT_ANIMAL_RATE_BELOW_MIN_FORMAT % format_trimmed(
            HudComposeVocab.HUNT_ANIMAL_RATE_MIN_SHOWN,
            HudComposeVocab.HUNT_ANIMAL_RATE_DECIMALS)
    return format_trimmed(value, HudComposeVocab.HUNT_ANIMAL_RATE_DECIMALS)

## **WHAT THE JOB COSTS, WHAT IT WOULD TAKE, AND WHAT HOLDING IT COSTS FOREVER — the compose sheet's
## pre-commit quote**, as `50 work, ≈25 turns · 2 work a turn from Agriculture to hold` (or `50 work`
## alone where there
## is neither an estimate nor a standing bill to state). The caller resolves the turns half; on the
## compose sheet that is `SourceForecast.build_turns_at`, evaluated against the crew and floor the
## player is proposing, because a quote for a job nobody has started is precisely what the sim's own
## `buildTurnsRemaining` cannot answer.
##
## **THE STANDING PRICE IS A PRICE AND NOT A THRESHOLD** (`docs/plan_standing_upkeep.md` §2.4). It is
## the quoted rung's `SourceForecast.build_upkeep_demand`, which for one slice was the term the build's
## closed form subtracted and which the BUILDERS stepper stated as a bar to clear. The keeping pool
## owes it at every fullness now, so it buys the player nothing to compare against a build crew — what
## it answers is *and this much every turn, forever*, which is the half of the commitment the one-off
## price beside it cannot state. **In WORK, never in hands**: the model is denominated in work units
## end to end, and how many hands the rate takes depends on what they carry.
##
## A rung the wire prices no rate on states no standing clause — a `0 work a turn` bill reads as a
## defect, and a rung that is free to hold should say nothing rather than say nothing twice.
## `""` for a rung the wire prices nothing on at all, which renders as no clause rather than a bare
## verb wearing an em-dash.
##
## **THE CLAUSE NAMES THE POOL THAT PAYS, so `kind` is required.** `… · 2 work a turn to hold` said
## what the rate was and never who owed it, and reported from play it read on the compose sheet as a
## demand on the crew under the stepper — which it is not. The role word is
## `HudWorkVocab.keeping_role_name`, the same per-web pair the work row's under-kept note keys on, so
## the two surfaces cannot send the player to two different cards. `kind` is a SOURCE kind
## (`SOURCE_KIND_*`).
static func build_price_clause(work_cost: float, turns: int, upkeep: float,
        kind: String) -> String:
    if work_cost <= SourceForecast.BUILD_WORK_COST_NONE:
        return ""
    var price := HudComposeVocab.BUILD_PRICE_WORK_FORMAT % format_work_units(work_cost)
    # **THE SENTINEL TEST IS THE CLAUSE PRODUCER'S** — this asks whether it was given words, rather
    # than re-listing which values have none. A second list is how one of them comes to be missed.
    var turns_clause := build_turns_clause(turns)
    if turns_clause != "":
        price = HudComposeVocab.BUILD_PRICE_TURNS_FORMAT % [price, turns_clause]
    if upkeep < SourceForecast.UPKEEP_WORK_MIN:
        return price
    return HudComposeVocab.BUILD_PRICE_UPKEEP_FORMAT % [price, format_work_units(upkeep),
        HudWorkVocab.keeping_role_name(kind)]

## RETIRED — **`build_turns_never(turns)`**, which answered *"is this the estimate that has to STOP the
## player?"*. Its doc called it the single test both compose faces gate their warning ink on, and it had
## been reached by nobody since `SourceForecast.build_pace` took that job: the pace CLASSIFIES the
## sentinel and the ink follows from the class (`HudWidgets.improvement_pace_color`), which is what lets
## the same fork carry three colours where a bool can only carry two. A live-looking test with no
## callers is the worst kind of stale — it went on special-casing `-2` alone, so a reader checking
## whether this client had followed the sentinel split would have found a *yes* that meant nothing.

## **THE COMPOSE SHEET'S TURN CLAUSE — `≈20 turns`, or `≈1 turn`** — the count and its noun, decided in
## ONE place for both compose faces (the offered face's price and the running face's tail). They quote
## one estimate about one job, so a build one turn out that read `≈1 turns` on the sheet beside the
## tile card's `≈1 turn at this crew` would be the same number worded two ways on one screen.
## `HudSelectionVocab.BUILD_TURNS_ROW_ONE` is that card's half of the same pair.
##
## **TWO NON-FINISHING SENTINELS READ `∞ turns` AND THE THIRD STATE READS `held`** — a crew banking
## exactly the meter's own rot (`BUILD_TURNS_HOLDS` **with a crew**) and one under it
## (`BUILD_TURNS_ROTS`) take the `∞`, because neither ever reaches a turn count. **What tells those two
## apart on this surface is the INK, which is not this function's** — the face is one Control and takes
## one colour, applied by the host from `SourceForecast.build_pace` (amber holding, red losing). The
## tile card, which has a whole row to spend, additionally says *losing ground* in words
## (`rung_row_value`); a compose face has one line already carrying the meter and the price.
##
## **THE `∞` MAY NOT BE SPENT ON A BENIGN STATE, WHICH IS WHY THE CREW REACHES THIS FUNCTION**
## (`docs/plan_standing_upkeep.md` §4.6a). `BUILD_METER_HOLDS` with NOBODY on it is a build parked on
## purpose, and the glyph is `BUILD_TURNS_NEVER_GLYPH` — the larder runway's own `∞`, whose entire
## justification is that a player learns a mark once and reads it everywhere. Spending it on a state
## where nothing is wrong teaches that `∞` sometimes means nothing is wrong, which costs the two states
## where it means a great deal. So the parked reading says **`held`**, in the neutral ink
## `BUILD_PACE_HELD` already takes, and the sheet and the card then say the SAME WORD about the same
## state — the property that makes a two-producer pair trustworthy.
##
## `build_crew` is the proposal's own stepper. `BUILD_CREW_ANY` for a caller with no staffing in hand,
## which reads as *somebody is on it* and keeps the `∞` — the conservative arm, since a warning wrongly
## withheld is the failure this whole family exists to prevent.
static func build_turns_clause(turns: int,
        build_crew: int = SourceForecast.BUILD_CREW_ANY) -> String:
    if turns == SourceForecast.BUILD_TURNS_HOLDS \
            and build_crew <= SourceForecast.BUILD_CREW_NONE:
        return HudComposeVocab.BUILD_TURNS_HELD
    if turns == SourceForecast.BUILD_TURNS_HOLDS or turns == SourceForecast.BUILD_TURNS_ROTS:
        return HudComposeVocab.BUILD_TURNS_NEVER_FORMAT % BUILD_TURNS_NEVER_GLYPH
    # ⛔ **NO FINITE COUNT MEANS NO CLAUSE, AND THAT TEST IS THE PRODUCER'S NOW.** Every sentinel with
    # a face is answered above; anything else — `BUILD_TURNS_NO_ESTIMATE`, a `0`, whatever the wire
    # spells next — has no number to state, and this fell THROUGH to `"≈%d turns" % turns` and would
    # have rendered `≈-1 turns` on the first caller that forgot to filter. Both callers did filter,
    # so nothing shipped it; a format that CAN render a missing number will, on the next sentinel,
    # which is why the guard is here rather than repeated at each call site. **Callers ask whether
    # they were given a clause** (`build_price_clause`, `DrawerComposeController`'s running face),
    # so there is exactly one test and a new sentinel cannot outrun it.
    if turns <= SourceForecast.BUILD_TURNS_NONE_TO_STATE:
        return ""
    if turns == BUILD_TURNS_SINGULAR:
        return HudComposeVocab.BUILD_TURNS_COUNT_ONE
    return HudComposeVocab.BUILD_TURNS_COUNT_FORMAT % turns

## RETIRED — **THE `At risk:` ROW, ITS SHORTFALL AND ITS COUNTDOWN, AND THE REMEDY LINE UNDER IT.**
## An under-kept Tended patch used to spend three lines of the card on one fact: the rung row, an
## `At risk: short 2 work — this rung is lost in 3 turns` beneath it, and an indented
## *"This ground is slipping — raise this band's Agriculture role."* under that. Reported from play as
## the card SHOUTING — three lines and two figures for a state whose whole content is *this is
## slipping, staff the role*.
##
## **IT IS ONE ROW NOW, AND THE REMEDY IS THE ROW'S HOVER.** `rung_row_value` says
## `🌾 Tended 100% ⚠ slipping` (`🐄 Domesticated 100% ⚠ drifting` on the animal web) and the row's
## `[hint=…]` carries `HudWorkVocab.under_kept_tooltip_for_source` — the same sentence, one hover
## away, with nothing else in it.
##
## **NO FIGURE SURVIVES ON THIS SURFACE, and the countdown's absence is the deliberate half.** The
## work board carries it (`HudWorkVocab.under_kept_tooltip` takes a rung and a grace), because that is
## where staffing is decided this turn and *how long you have* is actionable there. A card is where
## you look at the ground, and a number you cannot act on from it is noise. **ONE producer with a
## flag** — a second sentence-builder for the card is how the two surfaces come to phrase one hazard
## differently.
##
## `UPKEEP_RISK_ROW`, `UPKEEP_LOST_SOON_FORMAT` and `UPKEEP_LOST_NOW_FORMAT` went with the row, as did
## `at_risk_lines` itself and the WARN case the tint registry kept for its key.

## **THE ONE SUB-ROW A RUNNING BUILD STILL HANGS BENEATH ITSELF** — what the crew's tools ADD to what
## it banks each turn, indented so it reads as an expansion of the meter row above it.
##
## **THE TURN ESTIMATE LEFT THIS PRODUCER AND BECAME THE ROW ITSELF** (issue #545). It was an indented
## `≈11 turns at this crew` under a meter stating the same build in work units — two lines for one
## fact — and the rung row now LEADS with the count (`rung_row_value`), which is what a glance wants
## off a build. The `at this crew` tail went with it: the row is the crew's answer, and the estimate
## and the state can no longer disagree because they are one string.
##
## **THE GEAR LINE STAYED, and it renders only above zero** (a `+0 work` advertises a tool that did
## nothing). It is the only way a player can tell a tool is worth carrying at all, it is conditional
## rather than permanent chrome, and it was no part of the four-line block that made a rung unreadable.
##
## **WHAT IT STATES TURNED OVER WITH THE MODEL** (`docs/plan_standing_upkeep.md` §4.8):
## `buildWorkFromGear` is *what the pool's kits add per turn*, not *units taken off the job*, so the
## row is a rate with a `+` rather than a discount with a `−`. The line that would once have read
## `−17 work off this job` reads `+1.0 work a turn`.
##
## `prefix` spells the keys, so one call serves a `patch_`-prefixed `tile_info` and a bare herd dict.
static func build_gear_lines(source: Dictionary, prefix: String) -> Array[String]:
    var lines: Array[String] = []
    var gear := SourceForecast.build_work_from_gear(source, prefix)
    if gear > BUILD_GEAR_WORK_NONE:
        lines.append("%s%s" % [MORALE_BREAKDOWN_INDENT,
            HudSelectionVocab.BUILD_GEAR_WORK_ROW_FORMAT % format_work_units(gear)])
    return lines

## **WHY THE BUILDERS ARE HELD HERE, AND WHAT WOULD FREE THEM** — the sub-row(s) beneath
## `RUNG_BLOCKED_FORMAT`, whose headline says the pool is stuck and deliberately states no cause
## (`docs/plan_standing_upkeep.md` §4.6b).
##
## **IT USED TO RENDER ONLY WHERE THE KEEPING WAS SHORT, AND THAT GATE WAS THE BUG.** The build
## surface could not see WHY the gate refused, so it paired the block with the one fact the same row
## already published — a shortfall — and said nothing at all when there was none. Reported from play:
## a Tame sat at `⚠ Blocked 32%`, the player staffed the keeping the sub-row named, and the block
## stayed with the row now silent. The real refusal was the herd standing below its escapement floor,
## which nothing on any surface said. **The sim publishes the cause now** (`buildBlockedReason`), so
## this states it for every key and invents nothing: the wording table is
## `HudSelectionVocab.BUILD_BLOCKED_REASONS` and an unrecognised key — or a `-4` carrying no key —
## takes `BUILD_BLOCKED_FALLBACK` rather than an empty line, which on a marked row would read as
## *there is no cause*.
##
## **BOTH SURFACES THAT SHOW A BLOCKED BUILD CALL THIS** — the source's own card (the tile card and
## the herd drawer, indented under the rung row) and the BUILD QUEUE row's tooltip, which passes an
## empty `indent` because a tooltip has no rung row to hang beneath. One producer, so the two cannot
## come to disagree about a refusal, which is this arc's most repeated failure.
##
## > ### ⛔ THE BLOCK IS TWO LINES — THE HEADLINE AND ONE SHORT CAUSE. NOTHING ELSE RENDERS.
## >
## > **THREE LINES OF PROSE UNDER A HAZARD ROW IS NOT A READOUT.** It drew a headline that spelled out
## > what *Blocked* means, then a cause, then a remedy, then — where the keeping was short — a second
## > remedy, in a ~245px column standing between the rung row and an `At risk:` countdown. Rejected on
## > sight: *"You don't need 'your builders are stuck here', that's what Blocked means. You also don't
## > need the 2nd and 3rd sentence."*
## >
## > **THE CUT SENTENCES ARE DELETED, NOT PARKED.** A tooltip carrying them was built and put to Ray;
## > the answer was *"delete them"*. So this row emits exactly one line, `RUNG_BLOCKED_REMEDY_FORMAT`
## > is retired, and the causes themselves are one sentence each in
## > `HudSelectionVocab.BUILD_BLOCKED_REASONS` — which is where a new key's wording rule lives.
## > **Nothing here trims a string at render time**: the table says what the card shows, so this
## > producer and the queue row's tooltip cannot come to show different halves of one refusal.
## >
## > **WHAT THE PLAYER STILL GETS INSTEAD OF THE REMEDY** is the rung row above: an under-kept source
## > says `⚠ slipping` beside its meter and carries the role that pays on the row's own hover. Do NOT
## > re-add a remedy here as one the block is missing.
##
## `prefix` spells the keys, so one call serves a `patch_`-prefixed `tile_info` and a bare herd dict.
static func build_blocked_lines(src: Dictionary, prefix: String, kind: String,
        indent: String = MORALE_BREAKDOWN_INDENT) -> Array[String]:
    var lines: Array[String] = []
    if SourceForecast.build_turns_remaining(src, prefix) \
            != SourceForecast.BUILD_TURNS_QUEUE_BLOCKED:
        return lines
    lines.append("%s%s" % [indent, build_blocked_reason_text(
        SourceForecast.build_blocked_reason(src, prefix), kind,
        SourceForecast.build_material_cost(src, prefix))])
    return lines

## The sim's cause key in the player's own words. An unknown key — and the empty one, which a `-4`
## should never carry — answers the fallback: we still know the builders are stuck, and saying so is
## honest where guessing at a cause or rendering nothing is not.
##
## **ONE CAUSE IS WORDED PER WEB, AND `kind` IS WHAT FORKS IT.** The escapement sentence has to say
## *animals* on one web and *growing* on the other; every other cause reads identically on a patch and
## on a herd and is stated once. It was the same argument the retired keeping line forked on, which is
## why the parameter is still `kind` and not a bool: `HudWorkVocab.keeping_role_name` asks it the same
## way, so there is no second way of asking which web this source is.
## **AND ONE CAUSE NAMES A GOOD, WHICH IS WHY `pile` IS A PARAMETER** (`docs/plan_standing_upkeep.md`
## §2.7). The wire's key is only `materials`; WHICH good ran out is the rung's own build pile, so the
## sentence is composed rather than looked up. `[]` — a wire this client is behind on — takes the
## unnamed form rather than inventing a material.
static func build_blocked_reason_text(key: String, kind: String,
        pile: Array[Dictionary] = [] as Array[Dictionary]) -> String:
    if key == HudSelectionVocab.BUILD_BLOCKED_REASON_ESCAPEMENT:
        return HudSelectionVocab.BUILD_BLOCKED_ESCAPEMENT_HERD \
            if kind == SourceForecast.SOURCE_KIND_HERD \
            else HudSelectionVocab.BUILD_BLOCKED_ESCAPEMENT_PLANT
    if key == HudSelectionVocab.BUILD_BLOCKED_REASON_MATERIALS:
        # **EVERY GOOD IN THE PILE, JOINED — never a sum and never the first of them.** A rung eating
        # two goods is blocked on whichever the store cannot cover, and the wire does not say which,
        # so naming one would name a winner the sim did not.
        var goods: Array[String] = []
        for row in pile:
            var id := String(row.get(SourceForecast.MATERIAL_PAYOFF_ID_KEY, ""))
            if id != "":
                goods.append(id)
        if goods.is_empty():
            return HudSelectionVocab.BUILD_BLOCKED_MATERIALS_UNNAMED
        return HudSelectionVocab.BUILD_BLOCKED_MATERIALS_FORMAT % HudWorkVocab \
            .RUNG_TRACK_PRICE_SEPARATOR.join(goods)
    return String(HudSelectionVocab.BUILD_BLOCKED_REASONS.get(
        key, HudSelectionVocab.BUILD_BLOCKED_FALLBACK))

# RETIRED — `build_blocked_states_keeping`, which answered *"is the blocked sub-row already stating
# the keeping?"* over the `escapement`-and-actually-short pairing. It existed for ONE consumer, the
# retired `at_risk_lines`, and for one reason: to stop that row printing the under-kept remedy while
# the blocked row one line above was printing it too.
#
# **THE QUESTION STOPPED HAVING AN ANSWER when the blocked row's remedy was cut**
# (`HudSelectionVocab`, above `BUILD_BLOCKED_REASONS`). A predicate kept past that is not a harmless
# leftover: it went on suppressing, so on exactly that pairing the card named a shortfall and a
# countdown with **no role that pays them** — the only statement of who pays, removed by a guard
# against a duplicate that no longer exists.
#
# **DO NOT RE-ADD IT AS A CONDITION ANYWHERE.** If a future row states the keeping again, the two
# rows are what disagree, and one of them is the one to change.

## The one turn count that takes the singular row — a build one turn from done.
const BUILD_TURNS_SINGULAR := 1

## Below this the pool's kits ADD nothing to what it banks this turn (no build in flight, or nothing
## carried that serves this web), and the gear row is not rendered at all — a `+0 work a turn`
## advertises a tool that did nothing.
const BUILD_GEAR_WORK_NONE := 0.0

## **A RUNG DECLARED WITH NOBODY ON IT AND NOTHING BUILT YET, AS A ROW VALUE** —
## `SourceForecast.BUILD_UNSTAFFED_UNSTARTED` on whichever rung row would otherwise say nothing at
## all. It is a SENTENCE rather than a meter because there is no meter to state: the job has not
## moved, so `0 / 50 work (0%)` would be three ways of writing the same zero, and that zero beside a
## verb is exactly the *"it looks like it is working"* reading this row exists to remove.
##
## **THE ROW APPEARS AT A METER OF ZERO, which is the whole point.** Every rung row is gated on
## `progress > 0`, so a declared build nobody has staffed printed nothing on the tile card and nothing
## in the herd drawer, while the map drew a `0%` badge over it — a standing commitment that said
## *fine* on all three surfaces. The sim is not withholding anything here: it publishes the
## declaration and the crew, and the absence of a `buildTurnsRemaining` for an unstaffed source is
## correct (`SourceForecast.unstaffed_build_state` carries the full autopsy).
##
## **The OTHER unstaffed state keeps the words it already had.** A meter above zero with nobody on it
## is `RUNG_REVERTING_LABEL` in the same WARN ink, because a rung sliding back is losing work off the
## same job and the existing three-state rows already say so.
## The needle every rung row's tint registry matches this value on, so the amber is decided once for
## four rows rather than by four independent substring guesses. The value is BUILT FROM it
## (`RECOVERY_GUIDANCE_TEXT`'s idiom) so the words and the test cannot drift apart.
const BUILD_UNSTAFFED_NEEDLE := "no builders"

const BUILD_UNSTARTED_VALUE := "%s Not started — %s assigned" % [
    HudSelectionVocab.RUNG_HAZARD_GLYPH, BUILD_UNSTAFFED_NEEDLE]

## **THE THREE RUNG ROWS' OWN KEYS** (the fourth, the Field's, is `HudFloraVocab.FIELD_ROW`). They were
## bare literals at both ends — the producer that writes the row and `_value_hex`'s registry that inks
## it — and a THIRD reader arrived with the row hovers, which are keyed by exactly this string: a
## tooltip filed under a key no row carries attaches to nothing, silently. One spelling, three readers.
const HUSBANDRY_ROW := "Husbandry"

const CULTIVATION_ROW := "Cultivation"

const CORRAL_ROW := "Corral"

## The Husbandry rung's BUILT badge — the word a fully-tamed herd wears, glyph included, handed to
## `rung_row_value` as the face its percentage follows. `HUSBANDRY_BUILT_NEEDLE` is the same word
## lowercased, so the tint and the label cannot drift.
const HUSBANDRY_BUILT_WORD := "Domesticated"

const HUSBANDRY_BUILT_NEEDLE := "domesticated"

static func husbandry_built_label() -> String:
    return "%s %s" % [DetailFormat.CORRAL_GLYPH, HUSBANDRY_BUILT_WORD]

## BBCode hex for a "Husbandry" value — the shared rung rule: amber on the hazard mark, signal green
## on the built badge, neutral ink for a build under way.
static func husbandry_value_hex(value: String) -> String:
    return rung_value_hex(value, HUSBANDRY_BUILT_NEEDLE)

## The Cultivation rung's BUILT badge. **`Tended`, not `Tended Patch`** — the word now carries a
## percentage behind it (`🌾 Tended 92%`), and a noun-phrase plus a number read as two facts jammed
## into one cell. The row's own key already says which rung this is.
const CULTIVATION_BUILT_WORD := "Tended"

const CULTIVATION_BUILT_NEEDLE := "tended"

static func cultivation_built_label() -> String:
    return "%s %s" % [CULTIVATION_GLYPH, CULTIVATION_BUILT_WORD]

## BBCode hex for a "Cultivation" value — the shared rung rule (`rung_value_hex`): amber on the hazard
## mark, signal green on the built badge, neutral ink for a build under way.
##
## **THE THREE WARN CASES COLLAPSED INTO ONE TEST.** It matched `no builders`, then the word
## `Reverting`, and each new hazard state needed its own substring guess — which is how a state ships
## without its colour. The mark is now the needle, and `rung_row_value` puts it on every hazard.
static func cultivation_value_hex(value: String) -> String:
    return rung_value_hex(value, CULTIVATION_BUILT_NEEDLE)

## The plant RUNG-3 badge — a completed **Field**, wearing its own glyph so it reads as a DIFFERENT
## THING from a 🌾 Tended patch rather than as a bigger number, which is the whole point of rung 3.
static func field_built_label() -> String:
    return "%s %s" % [field_glyph(), FIELD_BADGE_LABEL]

## BBCode hex for a "Field" value — the shared rung rule, needled on this rung's own badge word.
static func field_value_hex(value: String) -> String:
    return rung_value_hex(value, FIELD_BADGE_LABEL.to_lower())

## The tile card's BASKET — one indented row per realized plant, sitting directly under the `Foraging`
## stock they DECOMPOSE (`🌾 Wild Tubers 38%  (78)` / `⇄ Cotton Fields 31%  (63)` / …).
##
## **THERE IS NO HEADER ROW AND NO DISCLOSURE.** A `What grows here` heading made the list read as a
## fourth resource standing beside the stocks rather than as the composition of one of them; the
## indent under `Foraging` says the same thing without a word, and always-visible is what lets a
## player see at a glance that (say) 62% of what grows on this ground is not food.
##
## The wire list is ALREADY sorted (share descending, then species key ascending) and its shares sum
## to 1, so this only formats: the order is the sim's and is rendered VERBATIM, never re-derived here.
##
## THE DISPLAYED PERCENTAGES ALWAYS SUM TO 100. `SourceForecast.flora_basket_entries` folds the
## rounding remainder into the LARGEST share (the first entry), so a basket that naively rounds to
## 99/101 still decomposes to 100. Returns [] for a tile with no composition (a biome that carries no
## forage), so no row renders.
##
## **`stock` IS THE PATCH'S STANDING BIOMASS, and it is what makes the decomposition checkable.**
## A share is a ratio and cannot be added to anything, so each row also states the biomass it amounts
## to — `percent × stock`, off the ALREADY-ROUNDED percent so the two columns of a row can never
## disagree, with the same largest-share remainder fold applied again so the biomasses sum to the
## STANDING half of the `Foraging` row above them EXACTLY. Pass `0` (the default) for a surface with
## no stock in hand and the rows render shares alone.
##
## **It is the STANDING STOCK, never the carrying capacity** (`_flora_biomass_split` states the same
## rule where the arithmetic lives, and `SubjectDrawerController` passes `patch_biomass`). On a
## drawn-down patch reading `90 / 100` these rows sum to **90**, not 100: they say what the stand
## the player is looking at is made of, and splitting the ceiling would decompose a full patch that
## is not there. A reader who expects 100 is reading the wrong half of that row.
##
## `committed_species` is the patch's committed crop KEY (not its display name) — "" for an
## uncommitted patch. That one member's row is marked in `HudStyle.SIGNAL`, the tint this HUD already
## spends on a standing investment (`cultivation_value_hex` / `field_value_hex`, the work board's rung
## mark), so the eye joins the `Crop:` row to the share it currently holds in the basket. It is a
## MARK, not a filter: every member still lists, because a commitment REWEIGHTS the basket over the
## build rather than emptying it, and watching the other shares fall is the feedback. The tint rides
## the row as inline BBCode, nested inside the neutral wrap `detail_bbcode` puts on every indented
## row — so it covers the NAME, SHARE AND BIOMASS only, leaving the indent and the role icon outside
## the tag, because that branch recognizes the row by `begins_with(MORALE_BREAKDOWN_INDENT)`.
##
## `icon_px` is the box the role mark's bundled art is drawn in, threaded from the HOST LABEL's font
## size (`SubjectDrawerController._tile_terrain_lines`) rather than written here — a static layer
## cannot ask a label how big its text is, and a literal would be exactly the hardcoded pixel size
## the HUD's other sprite-in-text surface already refuses to write (`Hud.update_discoveries`). `0`
## yields the emoji fallback, which is what every non-drawer caller gets.
static func flora_composition_lines(
    composition: Variant, committed_species: String = "", stock: float = 0.0, icon_px: int = 0
) -> Array[String]:
    var entries := SourceForecast.flora_basket_entries(composition)
    var lines: Array[String] = []
    if entries.is_empty():
        return lines
    var committed := committed_species.strip_edges()
    var biomass := _flora_biomass_split(entries, stock)
    for index in entries.size():
        var entry: Dictionary = entries[index]
        var face := HudFloraVocab.FLORA_SHARE_FORMAT % [String(entry["display_name"]), int(entry["percent"])]
        # A STRIPPED patch prints shares alone rather than three zeros: 38% of nothing is a
        # true statement about the stand, `(0)` three times is noise about the same fact.
        if stock > 0.0:
            face += HudFloraVocab.FLORA_SHARE_BIOMASS_CLAUSE_FORMAT % biomass[index]
        if committed != "" and String(entry["species"]) == committed:
            face = "[color=#%s]%s[/color]" % [HudStyle.SIGNAL_HEX, face]
        # An UNSTATED role renders the blank slot, never a defaulted icon — see `FoodIcons.for_crop_role`.
        # FOUR STEPS, EACH WITH ONE JOB, and the order is the fallback chain: this plant's own
        # SPECIES art, else the role mark (bundled art at `icon_px`, else its emoji), else the
        # transparent slot boxed to the SAME width as a mark, else the text spacer for when there is
        # no art to match the width of at all.
        #
        # **SPECIES OUTRANKS ROLE BECAUSE IT IS THE MORE SPECIFIC FACT.** An icon's job on this list
        # is to make a row findable at a glance, and "this is Wild Emmer" locates a row that "this is
        # a staple" cannot — three marks cannot separate five rows. The role is not demoted to
        # decoration by that: it remains the reading on every row with no species art — `hay_grass`
        # today, the roster's one permanent gap (`FloraSprites` covers 32 of 33), and any plant whose
        # art has not been drawn yet.
        #
        # **THE TWO NEVER RENDER TOGETHER**, deliberately: species art REPLACES the role mark rather
        # than sitting beside it, because two glyph families adjacent at one weight is the axis
        # collision `.claude/rules/client/labor-ui.md` records twice.
        #
        # The `[img]` box is the SAME `icon_px` in both tiers, so a row with species art and a row
        # with a role mark occupy identical width and the name column cannot go ragged.
        var icon := FoodIcons.for_flora_species(String(entry.get("species", "")), icon_px)
        if icon == "":
            icon = FoodIcons.for_crop_role(String(entry.get("role", "")), icon_px)
        if icon == "":
            icon = FoodIcons.crop_role_spacer(icon_px)
        if icon == "":
            icon = HudFloraVocab.FLORA_ROLE_ICON_UNSTATED
        lines.append(FLORA_COMPOSITION_SUBLINE_FORMAT % [MORALE_BREAKDOWN_INDENT, icon, face])
    return lines

## Each basket entry's share of the STANDING stock, as whole units summing EXACTLY to `round(stock)`.
##
## **It decomposes what is STANDING, not the capacity.** The row above reads `90 / 100`, and these
## rows say what those 90 are made of — splitting the capacity instead would describe a full patch
## the player is not looking at, and the two numbers on one card would disagree about which stand is
## being talked about.
##
## **THE QUANTITY IS THE WIRE'S OWN** — `ForagePatchState.compositionStandingBiomass`, folded onto each
## entry by the decoder and read here through `standing_biomass`. It used to be re-derived as
## `percent × stock`, which agreed with the wire in production and is exactly the shape this arc has
## shipped three defects of: two seams answering one question, drifting the first time either moves.
## The compose sheet's species chips read the same key, so the card and the sheet cannot come to spell
## one stand two ways.
##
## **THE ROUNDING RECONCILIATION STAYS, and it is display-only.** A decomposition that visibly fails to
## add up is worse than a ±1, so each figure is rounded and the remainder folded into the FIRST
## (largest) entry — the same fold `flora_basket_entries` applies to the percentages. That adjusts
## PRESENTATION of the wire's numbers; it does not produce them.
##
## **A BASKET THE WIRE QUOTED NO QUANTITY FOR FALLS BACK TO THE SHARE SPLIT**, all-or-nothing rather
## than per row: a column mixing stated and derived figures would be two producers inside one list. It
## is the "the server stated nothing" case, not a second opinion about a stand it did state.
##
## Returns zeros for a stripped or stockless surface; those rows print no biomass at all.
static func _flora_biomass_split(entries: Array[Dictionary], stock: float) -> Array[int]:
    var split: Array[int] = []
    if stock <= 0.0:
        split.resize(entries.size())
        split.fill(0)
        return split
    var stated := true
    for entry in entries:
        if not bool(entry.get("has_standing_biomass", false)):
            stated = false
            break
    var total := 0
    for entry in entries:
        var value := int(round(float(entry["standing_biomass"]))) if stated else int(round(
            float(entry["percent"]) / float(SourceForecast.FLORA_SHARE_PERCENT_TOTAL) * stock))
        total += value
        split.append(value)
    split[0] = split[0] + int(round(stock)) - total
    return split

## Player-facing corral label — the herd twin of `cultivation_built_label`. A finished pen shows the
## livestock glyph; an in-progress one is the rung row's own meter, so this states only the badge.
##
## **IT NO LONGER SPEAKS FOR THE FEED.** It took the fed fraction and returned a STARVING face, which
## a built rung row then rendered beside its own build meter — `⚠ Starving — 47% fed 100%`, two
## unrelated percentages with nothing between them. The feed story is the `Fed:` row's whole job now,
## warning mark and DANGER tint included, and this rung means the rung again. `detail_bbcode` tints
## via `corral_value_hex`.
static func corral_built_label() -> String:
    return "%s %s" % [DetailFormat.CORRAL_GLYPH, CORRAL_BUILT_WORD]

## The Corral rung's BUILT badge word and its lowercased needle.
const CORRAL_BUILT_WORD := "Corralled"

const CORRAL_BUILT_NEEDLE := "corralled"

## BBCode hex for a "Corral" value — **the shared rung rule and nothing else**. The starving special
## case above it (`value.contains("starving")` → DANGER) is deleted with the label that produced that
## string: a pen's feed is not a fact about how built its fence is, and the `Fed:` row carries both
## the mark and the red now.
static func corral_value_hex(value: String) -> String:
    return rung_value_hex(value, CORRAL_BUILT_NEEDLE)

## BBCode hex for the penned herd's `Fed:` value — DANGER while the pen is starving, ordinary ink
## otherwise. It reads the hazard prefix the value was BUILT with (`PEN_FEED_HAZARD_PREFIX`) rather
## than re-deriving `PenStatus.is_starving` from a fraction this layer no longer holds, which is the
## same idiom the full-width WARN branch of `detail_bbcode` uses to find a hazard sentence.
static func pen_feed_value_hex(value: String) -> String:
    if value.begins_with(PEN_FEED_HAZARD_PREFIX):
        return HudStyle.DANGER_HEX
    return HudStyle.INK_HEX

## The penned herd's `Fed:` value — the WHOLE feed story in one line: how much of its demand arrived,
## which of its two sources it came from, and how much more fodder a turn would close the gap.
##
##     100% — all pasture
##     100% — 88% pasture · 12% fodder
##     ⚠ 47% — 40% pasture · 7% fodder · needs 11.3 more/turn
##     ⚠ 40% — 40% pasture · no fodder · needs 12.0 /turn
##
## **THE FODDER SHARE IS `fed − pasture`.** Both are published shares of the same (unpublished) gross
## demand, so their difference is the share fodder covered — arithmetic the wire supports. What it
## does NOT support is dividing `fodder_draw` (an absolute) by a ratio to reconstruct that gross; the
## gross is not on the wire and may not be synthesized. The subtraction runs on the rounded
## percentages so the terms add up to the headline on screen, and clamps at zero because
## `pen_fed_fraction` is clamped at 1.0 and the two can round-cross.
##
## **`no fodder` IS NOT `0% fodder`.** Below `SourceForecast.FODDER_FLOW_MIN` the keeper carried
## nothing in at all — no store, or no Foddering — which is a different problem from a short ration,
## and the shortfall clause drops its `more` to match: there is no "more" than nothing.
static func pen_feed_value(herd_data: Dictionary) -> String:
    var fed_fraction := PenStatus.fed_fraction(herd_data)
    var fed_percent := int(round(fed_fraction * HudConst.PROGRESS_PERCENT_SCALE))
    var pasture_percent := int(round(
        float(herd_data.get("pen_pasture_fraction", 0.0)) * HudConst.PROGRESS_PERCENT_SCALE))
    var hazard := PEN_FEED_HAZARD_PREFIX if PenStatus.is_starving(fed_fraction) else ""
    # Where the feed came from. A footprint that covers the whole demand has nothing to split.
    var sources := PEN_FEED_ALL_PASTURE_SEGMENT
    if float(herd_data.get("pen_pasture_fraction", 0.0)) \
            < PEN_PASTURE_COVERS_ALL - PenStatus.FED_EPSILON:
        sources = PEN_FEED_PASTURE_SEGMENT % pasture_percent
        if float(herd_data.get("fodder_draw", 0.0)) >= SourceForecast.FODDER_FLOW_MIN:
            sources += PEN_FEED_FODDER_SEGMENT % maxi(0, fed_percent - pasture_percent)
        else:
            sources += PEN_FEED_NO_FODDER_SEGMENT
    # …and what would close the gap: the sim's own `max(0, need − draw)`, silent when the pen is fed.
    var shortfall := float(herd_data.get("pen_fodder_shortfall", 0.0))
    var shortfall_segment := ""
    if shortfall >= SourceForecast.FODDER_FLOW_MIN:
        var shortfall_format := PEN_FEED_SHORTFALL_MORE_SEGMENT \
            if float(herd_data.get("fodder_draw", 0.0)) >= SourceForecast.FODDER_FLOW_MIN \
            else PEN_FEED_SHORTFALL_SEGMENT
        shortfall_segment = shortfall_format % SourceForecast.format_fodder(shortfall)
    return PEN_FEED_VALUE_FORMAT % [hazard, fed_percent, sources, shortfall_segment]


# =====================================================================================
#  PURE LEAVES THE LINE PRODUCERS SHARE
# =====================================================================================

## Humanize an expedition mission id ("scout" → "Scouting expedition"); falls back to a capitalized
## token for an unknown/future mission (e.g. PR 2's "hunt").
static func expedition_mission_label(mission: String) -> String:
    var key := mission.strip_edges().to_lower()
    if HudExpeditionVocab.EXPEDITION_MISSION_LABELS.has(key):
        return HudExpeditionVocab.EXPEDITION_MISSION_LABELS[key]
    return key.capitalize() if key != "" else "Expedition"

## Plain-language label for a morale cause (0=None,1=Terrain,2=Cold,3=Unrest); "" for None or
## unknown. Shared by the drawer morale line and the losing-population alert reason.
static func morale_cause_label(cause: int) -> String:
    match cause:
        DetailFormat.MORALE_CAUSE_TERRAIN:
            return DetailFormat.MORALE_CAUSE_LABEL_TERRAIN
        DetailFormat.MORALE_CAUSE_COLD:
            return DetailFormat.MORALE_CAUSE_LABEL_COLD
        DetailFormat.MORALE_CAUSE_UNREST:
            return DetailFormat.MORALE_CAUSE_LABEL_UNREST
        _:
            return ""

## Human-readable food runway: the ∞ glyph when the source is not food-limited, otherwise a whole
## count of TURNS — spelled from the shared `FOOD_RUNWAY_UNIT`, which the Food-row tint guard in
## `_value_hex` also keys on, so the two can never disagree about the unit. One helper feeds every
## surface that shows it (the band Food line, the expedition Carried/Provisions rows, and the turn-orb
## starving alert), so the unit is stated in exactly one place.
static func food_turns_text(runway: float) -> String:
    if not BandFoodStatus.is_limited(runway):
        return FOOD_UNLIMITED_GLYPH
    var turns := int(round(runway))
    if turns == 1:
        return "%d %s" % [turns, FOOD_RUNWAY_UNIT]
    return "%d %ss" % [turns, FOOD_RUNWAY_UNIT]

## True when the band's morale warrants surfacing the itemized breakdown + recovery guidance: below
## the warn threshold, or falling by more than the trend epsilon.
static func morale_is_concerning(unit_data: Dictionary) -> bool:
    var morale := float(unit_data.get("morale", 1.0))
    var delta := float(unit_data.get("morale_delta", 0.0))
    return morale < BandFoodStatus.warn_morale() or delta <= -DetailFormat.MORALE_TREND_EPSILON


# =====================================================================================
#  BAND FOOD ARITHMETIC
#  Pure `band`-dict math, shared by the Food summary row, its breakdown, and the Band panel's
#  FOOD OUTLOOK chart — which is why it lives here rather than travelling with either one.
# =====================================================================================

## Net per-turn food flow: income − what the PEOPLE eat − what PREDATORS raided off the larder this
## turn. **The band's penned ANIMALS are not a term** — a pen is fed by its fenced footprint's grass
## and by hay, never from the food larder, so it cannot move this number.
##
## **THE TRANSFER PAIR IS DELIBERATELY NOT A TERM HERE** (arc #527). Transfers are what CROSSED
## between larders — a past event, not a
## rate — and they are itemized as their own two breakdown rows (`⇄ From other bands` / `⇄ To other
## bands`) where a past event belongs. Folding them into this headline made it a different quantity
## from the runway printed BESIDE IT on the same row: the sim's `turnsOfFood` is computed from
## per-source income and excludes transfers entirely, so `-39.0/turn (20 turns)` would be two numbers
## on one line computing on different bases. Matching the sim's basis is the point.
##
## A shipment is also bounded only by the manifest the player builds — up to the whole larder —
## unlike `raid_forfeit`, which is capped at a fraction of one turn's income, so a band with income 6
## and consumption 5 that sent 40 read `-39.0` in DANGER red with a WARN caret on an economy that had
## not changed, and `+1.0` again the next frame. **Closing the real gap is sim-side work** (issues
## #547 / #548 — the sim projecting a steady transfer forward), so until then this headline does NOT
## reflect a neighbour's recurring supply-network contribution, and no client-side fold-in may fake
## one.
##
## Positive → the larder is growing. `raid_forfeit` is the sim's own answer for the third term
## (`PopulationCohortState.raidForfeit`, Predators Phase 3 — food lost to raids this turn); the
## client must NOT re-derive it, and the full identity
## `larder_delta == income − consumption − raid_forfeit + transfers` is pinned sim-side
## (`integration_tests/tests/{pen_food_ledger,raid_food_ledger,transfer_food_ledger}.rs`) — the
## BREAKDOWN is what states it in full, this headline being the steady rate rather than the ledger.
## Raids are EPISODIC, so this net can swing the turn one lands — the forward food-outlook chart
## deliberately does NOT project raid_forfeit forward (a past loss is not a steady drain).
static func band_net_food(band: Dictionary) -> float:
    return band_food_income(band) \
        - float(band.get("food_consumption", 0.0)) \
        - band_raid_forfeit(band)

## The STEADY total food income = Gathered + Hunted (Σ per-source realized average across the band's
## forage + hunt assignments). Summed from the SAME per-source realized values as the breakdown rows, so
## it equals Gathered + Hunted exactly — the honest long-run average of the lumpy per-turn take, so it
## does NOT swing. It feeds the headline net (`band_net_food` = income − Eaten − Lost to raids) and the
## `food_is_concerning` gate. **Deliberately summed from the rows rather than read off a band-level
## wire field** — a separately-computed total could drift from the Gathered/Hunted rows it sits above,
## and this way the headline equals them by construction. (A cohort-level `foodIncomeAverage` existed
## for one commit and was retired as redundant; do not reintroduce it.)
static func band_food_income(band: Dictionary) -> float:
    return sum_realized_yield(band, SourceForecast.LABOR_KIND_FORAGE) \
        + sum_realized_yield(band, SourceForecast.LABOR_KIND_HUNT)

## What predators raided off this band's larder this turn (food, `PopulationCohortState.raidForfeit`).
## 0 when no raid landed — the ledger then omits the row entirely.
static func band_raid_forfeit(band: Dictionary) -> float:
    return float(band.get("raid_forfeit", 0.0))

## Food that CROSSED IN from another band over the snapshot window
## (`PopulationCohortState.transferReceived`) — a supply-network pooling, a shipment landing, a
## party's pack coming home. 0 for a band nobody sent to.
##
## **THE WINDOW IS THE SNAPSHOT WINDOW, NOT THE TURN**, because a launch draw happens when a command
## is applied, between two published frames. The sim accumulates across exactly the interval a
## client's own `larder_delta` measures and clears after the capture.
##
## **NOTHING RENDERS THIS PAIR — the breakdown reads the per-turn twins below.** It is the term that
## closes the larder identity between two TURN frames, and it is legitimately `0.0` on any frame a
## command refreshed. A readout reading it shows a transfer for exactly as long as the player does
## nothing, then loses it.
static func band_transfer_received(band: Dictionary) -> float:
    return float(band.get("transfer_received", 0.0))

## …and what crossed OUT (`PopulationCohortState.transferSent`). Its own magnitude rather than a
## signed net with the term above: a band that both sends and receives in one window is doing
## something, and a net would render that as nothing having happened.
static func band_transfer_sent(band: Dictionary) -> float:
    return float(band.get("transfer_sent", 0.0))

## What crossed IN **on the turn** (`PopulationCohortState.transferReceivedTurn`) — the reading the
## Food breakdown's `⇄ From other bands` row is made of. 0 for a band nobody sent to, which the
## ledger renders as no row at all.
##
## **PER-TURN STATE ON THE COHORT, so a recapture re-reads it unchanged** — which is the whole
## difference from the accumulating twin above, and the reason every row of this breakdown is now on
## the same basis as `Gathered` / `Eaten` / raid forfeit beside it. On the turn's own
## frame the two agree exactly.
static func band_transfer_received_turn(band: Dictionary) -> float:
    return float(band.get("transfer_received_turn", 0.0))

## …and what left for them on the turn (`PopulationCohortState.transferSentTurn`). Two named
## magnitudes here as well, for the same reason the pair above are two.
static func band_transfer_sent_turn(band: Dictionary) -> float:
    return float(band.get("transfer_sent_turn", 0.0))

## The band's larder (provisions) as a float — the starting point of the food-outlook projection and
## the number the Food summary row prints (rounded there). Here beside the rest of the band food
## arithmetic the chart and the Food line share.
static func band_provisions(band: Dictionary) -> float:
    var stores_variant: Variant = band.get("stores", {})
    if stores_variant is Dictionary:
        return float((stores_variant as Dictionary).get(HudConst.STORE_ITEM_PROVISIONS, 0.0))
    return 0.0

## The band-wide merged arrival schedule: element-wise sum of every source's `arrival_schedule`, so
## slot i is ALL the food landing i+1 turns from now. Length = the longest schedule present (they are
## all `arrivals_horizon_turns` long in practice); empty when no source was projected, which is the
## signal to omit the Food-outlook block entirely rather than draw a flat starving line.
static func merged_arrival_schedule(band: Dictionary) -> PackedFloat32Array:
    var merged := PackedFloat32Array()
    for a in HudBandLaborState.labor_assignments_of(band):
        if not (a is Dictionary):
            continue
        var schedule := HudBandLaborState.as_schedule((a as Dictionary).get("arrival_schedule", null))
        if schedule.is_empty():
            continue
        if merged.size() < schedule.size():
            merged.resize(schedule.size())
        for i in range(schedule.size()):
            merged[i] += schedule[i]
    return merged

## True when the band carries a meaningful food flow (income, consumption, or pen feed above the
## floor) — so a decode miss reads as "no flow" (net readout + breakdown omitted, not zeroed).
##
## **The income term MUST be the same `band_food_income` the headline sums, not the wire's lumpy
## `food_income`.** They diverged once and it hid the readout exactly when it was needed: a starving
## band has `food_consumption` 0 (an empty larder debits nothing) and a whole-animal hunt pays 0 on a
## wait turn, so a band with a perfectly good STEADY income failed all three tests and lost its net
## line and breakdown entirely. Gate on the same number you display.
##
## **The transfer terms are the PER-TURN pair for that same reason** — they are what the two `⇄` rows
## render, and a gate on the accumulating pair goes false on a command-refreshed frame while the rows
## it protects still have values.
static func band_has_food_flow(band: Dictionary) -> bool:
    return band_food_income(band) >= SourceForecast.FOOD_FLOW_MIN \
        or float(band.get("food_consumption", 0.0)) >= SourceForecast.FOOD_FLOW_MIN \
        or band_raid_forfeit(band) >= SourceForecast.FOOD_FLOW_MIN \
        or band_transfer_received_turn(band) >= SourceForecast.FOOD_FLOW_MIN \
        or band_transfer_sent_turn(band) >= SourceForecast.FOOD_FLOW_MIN

## Sum of per-source `realized_yield` (the STEADY per-source average, food/turn) across this band's
## labor assignments of one kind — the category total behind the Food breakdown (Gathered = forage,
## Hunted = hunt). Reads the steady average (not the lumpy `actual_yield`) so the rows don't swing AND
## sum to the steady headline income (`band_food_income`); falls back to `actual_yield` if absent.
static func sum_realized_yield(band: Dictionary, kind: String) -> float:
    var total := 0.0
    for a in HudBandLaborState.labor_assignments_of(band):
        if a is Dictionary and String((a as Dictionary).get("kind", "")).strip_edges().to_lower() == kind:
            var d := a as Dictionary
            total += float(d["realized_yield"]) if d.has("realized_yield") else float(d.get("actual_yield", 0.0))
    return total

# =====================================================================================
#  SHIPMENT ARITHMETIC (arc #527, issue #517)
#  What a trade party's packs are carrying, in the ONE expression the sim checks a manifest with.
#  Two surfaces ask it about the same pack — the compose sheet's live mass meter, which prices a
#  manifest before it is sent (`BandPanelController._trade_manifest_mass`), and the in-flight
#  `Carrying:` row, which reports one already walking (`BandDetailLines._shipment_summary_lines`) —
#  so it lives here, beside the band food arithmetic, for the reason that family does: two copies of
#  a formula are two answers about one pack.
# =====================================================================================

## **THE SIM'S OWN MASS EXPRESSION, HELD VERBATIM** — food counts as itself, and every unit of every
## material costs `expedition_trade_material_carry_weight` of pack space. That lever is a per-cohort
## echo of the sim's config, so a tuning change moves both surfaces and the server's refusal together.
##
## **THE FOOD TERM ALONE IS NOT THE MASS**, and reading it as one is what this exists to stop: a party
## carrying 2 food and 10 hide against a cap of 12 is FULL, and a row dividing the 2 by the 12 renders
## a full pack as one-sixth full.
static func shipment_mass(food: float, material_total: float, material_carry_weight: float) -> float:
    return food + material_total * material_carry_weight

## Σ of an IN-FLIGHT party's per-material cargo amounts (`expedition_cargo_materials`, the wire's
## per-material total across the batches it holds). **A pack-space input, never a readout** — the
## materials are rendered one term per material and are never summed on screen, a total of hide and
## bone being the retired trade axis under a new name.
static func shipment_cargo_material_total(unit_data: Dictionary) -> float:
    var total := 0.0
    for row_variant in unit_data.get("expedition_cargo_materials", []):
        if row_variant is Dictionary:
            total += float((row_variant as Dictionary).get(HudCraftingVocab.BATCH_AMOUNT_KEY, 0.0))
    return total

## The mass an in-flight party's cargo store weighs — `shipment_mass` over the two wire accounts and
## the cohort's own carry-weight lever, so the `Carrying:` row's numerator is the number the compose
## sheet's meter showed for the same manifest.
static func shipment_cargo_mass(unit_data: Dictionary) -> float:
    return shipment_mass(
        float(unit_data.get("expedition_cargo_food", 0.0)),
        shipment_cargo_material_total(unit_data),
        float(unit_data.get("expedition_trade_material_carry_weight", 0.0)))

# =====================================================================================
#  **THE BAND TRADE ARITHMETIC IS RETIRED** (arc #527)
#  `band_trade_stock` / `sum_realized_trade` / `band_trade_income` / `band_has_trade_flow` went with
#  the `TRADE_GOODS` commodity and the `trade_yield` wire fields they read. There is no scalar to put
#  in their place and there must not be one: a harvest's non-food product is MATERIALS, which the band
#  holds as `material_batches` (one pile per material per rating) and which the Crafting panel reads
#  as such. Summing them into a band-wide "goods" figure would be this family back under a new noun.
# =====================================================================================

## Food is "concerning" when the larder is net-draining OR the runway is below the warn threshold —
## mirroring `morale_is_concerning`'s below-warn / falling gate. It no longer auto-EXPANDS anything
## (a popover that pops itself open on a snapshot would be worse than the clipping it replaced); it
## marks the row's caret WARN, so a row with something worth reading still says so at a glance.
static func food_is_concerning(band: Dictionary) -> bool:
    var turns := float(band.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
    return band_net_food(band) < 0.0 \
        or (BandFoodStatus.is_limited(turns) and turns < BandFoodStatus.warn_turns())

## Is the band's FODDER larder worth opening right now — **the food test, on the fodder account**.
## Draining at all, or a runway inside the shared warn line, exactly as `food_is_concerning` reads it,
## through the SAME `BandFoodStatus` thresholds the runway is tinted by. That sameness is the point:
## the `Fodder:` row's own amber `need` clause is retired, so "worrying" is one rule for both larders
## and the two cannot disagree about what it looks like.
static func fodder_is_concerning(band: Dictionary) -> bool:
    var turns := float(band.get("turns_of_fodder", BandFoodStatus.UNLIMITED_TURNS))
    return band_net_fodder(band) < 0.0 \
        or (BandFoodStatus.is_limited(turns) and turns < BandFoodStatus.warn_turns())

## What the band's fodder larder is doing per turn: grown less eaten. The ONE subtraction of that
## pair in the client, so the caret's verdict and the popover's two rows cannot describe different
## turns.
static func band_net_fodder(band: Dictionary) -> float:
    return float(band.get("fodder_income", 0.0)) - float(band.get("fodder_need", 0.0))

## The band's standing FODDER stock — the fodder twin of `band_provisions`, and here for the same
## reason: the band's own `Fodder:` row, its gate and the faction page's rollup all state this stock,
## and three `band.get("fodder_store", …)` calls are three chances to read a renamed key in two of
## them. **A stock, not a rate** — render it through `SourceForecast.format_fodder`.
static func band_fodder_store(band: Dictionary) -> float:
    return float(band.get("fodder_store", 0.0))

## ---- THE DORMANT FODDER ROW, AT BOTH SCALES ------------------------------------------------------
##
## `Fodder: —`, dim, with the reason on the block's hover. The band's own row renders it for a band
## with no fodder economy (`BandDetailLines._band_fodder_dormant_line`) and the FACTION page renders
## it when NO band on the roster has one (`FactionRollup._fodder_line`) — one builder, because two
## surfaces spelling "there is no fodder here" two ways is the disagreement this state was added to
## remove. A faction reading `Fodder: 0.0 · +0.0 /turn` in full ink while every one of its bands read
## a dim dash is what it looked like before, and it read as a defect.
##
## **THE VALUE IS A DASH AND NEVER A ZERO.** The live format on an empty larder renders
## `Fodder: 0.0  (∞)`, and a full-ink zero beside a healthy infinity reads as *this has fodder and is
## fine* — the opposite of what the state means. The em-dash is the glyph this HUD already uses for an
## account with no quantity to state (`HudComposeVocab.YIELD_LOCKED_GLYPH`, the wild patch's
## unbankable hay; `HudFloraVocab.STOCK_UNKNOWN_GLYPH`, a fogged stock), read from that const rather
## than typed again.
##
## **THE DIM IS A SELF-TINTED RUN INSIDE THE VALUE CELL**, the `BandDetailLines
## .BAND_FOOD_FODDER_CLAUSE_FORMAT` idiom: `_value_hex`'s `Fodder` case keys on the RUNWAY spelling
## and leaves anything else in neutral ink, so the row has to carry its own colour.
const FODDER_DORMANT_ROW_FORMAT := HudDisclosureVocab.DETAIL_ROW_FODDER + ": [color=#%s]%s[/color]"

const FODDER_DORMANT_VALUE := HudComposeVocab.YIELD_LOCKED_GLYPH

## **NO FODDERING — nothing here can bank hay, at any price.** Foddering is what keeping a penned herd
## teaches, so a pre-pastoral faction banks none of what its meadows grow. That is a sentence the
## client already says, on the forage panel's yields row
## (`HudFloraVocab.GATE_REASON_WILD_FODDER_FORMAT`), so this reads the SHARED clause out of it rather
## than wording one lock twice — the patch-only half of that sentence ("or commit this patch to its
## crop") is dropped, neither a band row nor a faction row having a patch to commit.
## Format args: %d = the live faction Foddering percent, then the CORRAL glyph.
const FODDER_LOCKED_TOOLTIP_FORMAT := "Hay stays in the field: " \
    + HudFloraVocab.FODDERING_NOT_LEARNED_CLAUSE + "."

## **KNOWS FODDERING, KEEPS NOTHING YET — nothing is wrong.** There is simply no fodder economy here,
## so the sentence is calm and descriptive: it says what the row WILL hold, which is the whole reason
## the row renders at all with nothing to put in it.
const FODDER_DORMANT_TOOLTIP := "No fodder yet. Once your people keep a pen or grow a fodder crop, the hay store and what the pens draw on it read here."

## The dormant row, and the hover that says why it is dim — the WHOLE of that state, so a caller adds
## nothing to it but the faction's Foddering.
##
## **TWO REASONS, TWO SENTENCES, and the knowledge one goes first because it is the harder wall.** A
## faction that cannot bank hay at all is blocked by a whole rung; one that simply keeps no pen is not
## blocked by anything, and folding the two into one sentence would tell a pastoral player their
## working ladder is locked.
##
## **THE RUNWAY CONTEXT IS DELIBERATELY LEFT ALONE.** Callers reset `Context.fodder_turns` to `NAN`
## per render; writing a real `turns_of_fodder` here (999 for anything that drains nothing) would tint
## the dash HEALTHY green through `_value_hex`'s runway branch — a calm reading of an account that
## does not exist.
##
## **IT REGISTERS NO DISCLOSURE AND MUST NOT**, which is the caller's half of the contract: there is
## no flow to put behind a caret, and a caret over an empty pull-down is worse than no pull-down.
static func fodder_dormant_row(ctx: Context, foddering: float) -> String:
    if ctx != null:
        if foddering >= HudConst.KNOWLEDGE_COMPLETE:
            ctx.row_tooltips[HudDisclosureVocab.DETAIL_ROW_FODDER] = FODDER_DORMANT_TOOLTIP
        else:
            ctx.row_tooltips[HudDisclosureVocab.DETAIL_ROW_FODDER] = FODDER_LOCKED_TOOLTIP_FORMAT % [
                HudFormat.progress_percent(foddering),
                FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CORRAL)]
    return FODDER_DORMANT_ROW_FORMAT % [HudStyle.INK_DIM_HEX, FODDER_DORMANT_VALUE]

## Does this band have a FODDER ECONOMY at all — **does it HOLD hay, or does it OWE a hay bill?**
##
## **THE ROW NO LONGER HANGS ON THIS, AND THAT IS THE WHOLE OF THE CHANGE.** It was the gate on
## whether the `Fodder:` row rendered at all; the row is unconditional now (a band that cannot do
## fodder yet renders it DORMANT, so the account is discoverable before there is one), and this test
## says which of the two the row is. Its other readers are unmoved: the `compact` tier's merged
## clause still fires on it, and `BandDetailLines` still refuses to register a disclosure without it.
##
## It lives here, beside the fodder arithmetic, because the faction rollup asks it too — one test
## behind every spelling of "this band has a fodder larder", so no two surfaces can disagree about
## when one exists.
##
## Each term takes `FODDER_FLOW_MIN` rather than the food-scale `FOOD_FLOW_MIN`: this account renders
## at ONE decimal, so the finer floor admits a store that then prints as `Fodder: 0.0`.
static func band_has_fodder_economy(band: Dictionary) -> bool:
    return band_fodder_store(band) >= SourceForecast.FODDER_FLOW_MIN \
        or float(band.get("fodder_need", 0.0)) >= SourceForecast.FODDER_FLOW_MIN

## ---- THE BAND'S STANDING MATERIAL BILL (`docs/plan_standing_upkeep.md` §2.7) ---------------------
##
## **WORK WAS NEVER THE WHOLE PRICE.** A pen frays its fence every turn it stands; a road washes out.
## The band's holdings therefore owe a rate in GOODS beside the rate in hands, and this is the ledger
## that answers it: what is wanted, what arrives, and what is on the shelf — per good.
##
## ⛔ **THE SIM SUMS `material_upkeep_need` AND THIS CLIENT MUST NOT.** It is `fodder_need`'s own rule
## for `fodder_need`'s own reason: herd rows are FOG-FILTERED, so a total rebuilt here from the pens
## on screen silently drops one out of sight the band still owes for. Every accessor below reads the
## published band figure and folds nothing.
##
## ⛔ **AND NOTHING IS EVER SUMMED ACROSS GOODS.** Six hurdles and two rope are not eight of anything —
## that total is the retired trade axis under a new name, and it is the flattening the whole materials
## model exists to refuse. The row's headline names ONE good; the popover states them all, one block
## each.
const BAND_MATERIAL_UPKEEP_NEED_KEY := "material_upkeep_need"
const BAND_MATERIAL_UPKEEP_INCOME_KEY := "material_upkeep_income"
const BAND_MATERIAL_STORE_KEY := "material_store"

## The keys one bill ROW carries, beside `SourceForecast.MATERIAL_PAYOFF_ID_KEY`. Named because the
## row is produced here and read by three surfaces (the band row, its popover, the faction rollup).
const MATERIAL_BILL_NEED_KEY := "need"
const MATERIAL_BILL_INCOME_KEY := "income"
const MATERIAL_BILL_STORE_KEY := "store"
const MATERIAL_BILL_RUNWAY_KEY := "turns"

## **THIS BAND'S BILL, ONE ROW PER GOOD IT OWES** — `[{material_id, need, income, store, turns}]`, in
## the wire's own order, `[]` for a band holding nothing that eats a good.
##
## **THE GOODS ARE THE ONES `need` NAMES, and no others.** A band may hold a store of flint it owes
## nothing for; that is the Crafting panel's rail to state, not a standing bill. Income and store are
## looked up per good against the need — an ABSENT entry in either is a real ZERO (the sim drops a
## ledger row holding nothing), which is the worst reading there is and exactly what a `has()` gate
## would skip.
##
## **THE RUNWAY IS THE FOOD ROW'S IDEA ON THIS ACCOUNT**: the shelf against the gap the arrivals do
## not cover, `BandFoodStatus.UNLIMITED_TURNS` where the goods arrive at least as fast as they are
## eaten — the same `∞` the two larders spell, so a player learns the mark once.
static func band_material_bill(band: Dictionary) -> Array[Dictionary]:
    var income := _material_amounts(band.get(BAND_MATERIAL_UPKEEP_INCOME_KEY, []))
    var store := _material_amounts(band.get(BAND_MATERIAL_STORE_KEY, []))
    var rows: Array[Dictionary] = []
    for row in SourceForecast.material_payoff_rows(band.get(BAND_MATERIAL_UPKEEP_NEED_KEY, [])):
        var id := String(row.get(SourceForecast.MATERIAL_PAYOFF_ID_KEY, ""))
        var need := float(row.get(SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
        if need < SourceForecast.MATERIAL_FLOW_MIN:
            continue
        var arriving := float(income.get(id, 0.0))
        var held := float(store.get(id, 0.0))
        rows.append({
            SourceForecast.MATERIAL_PAYOFF_ID_KEY: id,
            MATERIAL_BILL_NEED_KEY: need,
            MATERIAL_BILL_INCOME_KEY: arriving,
            MATERIAL_BILL_STORE_KEY: held,
            MATERIAL_BILL_RUNWAY_KEY: material_runway(held, need, arriving),
        })
    return rows

## **THE GOOD IN THE WORST STATE — the one the row headlines**, `{}` when the band owes nothing. The
## shortest runway wins, because that is which good runs out first and therefore which one a player
## has to act on; ties keep the wire's order, which is the sim's own id order.
static func band_material_worst(band: Dictionary) -> Dictionary:
    var worst := {}
    for row in band_material_bill(band):
        if worst.is_empty() or float(row[MATERIAL_BILL_RUNWAY_KEY]) \
                < float(worst[MATERIAL_BILL_RUNWAY_KEY]):
            worst = row
    return worst

## Does this band hold anything that eats a good? The ONE gate behind every spelling of "there is a
## standing bill here", so the band row, its popover and the faction rollup cannot disagree.
static func band_has_material_upkeep(band: Dictionary) -> bool:
    return not band_material_bill(band).is_empty()

## One good's runway: the shelf against the gap the arrivals leave. `UNLIMITED_TURNS` — the `∞` both
## larders already spell — where nothing is draining, which is a band whose bench and fields keep up.
static func material_runway(store: float, need: float, income: float) -> float:
    var gap := need - income
    if gap < SourceForecast.MATERIAL_FLOW_MIN:
        return BandFoodStatus.UNLIMITED_TURNS
    return store / gap

## A `MaterialPayoff` list as `{material_id: amount}`. Private because a LOOKUP is a reading aid and
## never a payload: nothing outside this file may iterate it, or the per-good rows stop being the
## contract.
static func _material_amounts(raw: Variant) -> Dictionary:
    var amounts := {}
    for row in SourceForecast.material_payoff_rows(raw):
        amounts[String(row.get(SourceForecast.MATERIAL_PAYOFF_ID_KEY, ""))] = float(
            row.get(SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
    return amounts

## Per-row-per-band disclosure key — also the `[url]` meta payload and the popover's identity.
static func breakdown_key(kind: String, band: Dictionary) -> String:
    return "%s:%d" % [kind, int(band.get("entity", -1))]

## True when the band's growth warrants surfacing the itemized breakdown: its birth rate has fallen
## below the warn bucket. Mirrors `food_is_concerning` / `morale_is_concerning` — it EXPANDS nothing
## (the popover never pops itself open), it only tints the row's caret WARN so a row worth reading
## says so at a glance. A band with no reading yet is never "concerning": no data is not a famine.
static func growth_is_concerning(band: Dictionary) -> bool:
    if not BandFoodStatus.fertility_is_projected(band):
        return false
    return band_fertility(band) < BandFoodStatus.warn_fertility()

## The band's fertility MULTIPLIER — the product of the three exported factors, i.e. its birth rate as
## a share of the base `birth_rate` the sim would otherwise apply. The factors combine by PRODUCT, not
## by sum (unlike the morale contributions), which is the whole reason the breakdown rows below are
## spelled as `x0.60` multipliers rather than signed percentages: read down the disclosure and they
## multiply out to this headline. Returns the neutral 1.0 when the sim published no reading.
static func band_fertility(band: Dictionary) -> float:
    if not BandFoodStatus.fertility_is_projected(band):
        return BandFoodStatus.FERTILITY_NEUTRAL
    return float(band.get("fertility_hunger", BandFoodStatus.FERTILITY_NEUTRAL)) \
        * float(band.get("fertility_reserve", BandFoodStatus.FERTILITY_NEUTRAL)) \
        * float(band.get("fertility_trend", BandFoodStatus.FERTILITY_NEUTRAL))

## One `    ▼ ×0.60  short rations`-style fertility breakdown row. It reuses the morale breakdown's
## indent + ▲/▼ sign glyph so `detail_bbcode`'s shared indented-sub-line branch tints it (▲ above
## neutral = healthy green, ▼ below = amber) with no parallel styling path — but the VALUE is a
## multiplier, not a signed delta, because these factors multiply. Three signed percentages that
## refuse to add up to the headline would invite exactly the arithmetic they cannot support.
static func fertility_breakdown_row(factor: float, label: String) -> String:
    var glyph := DetailFormat.MORALE_CONTRIB_POSITIVE_GLYPH if factor > BandFoodStatus.FERTILITY_NEUTRAL \
        else DetailFormat.MORALE_CONTRIB_NEGATIVE_GLYPH
    return FERTILITY_BREAKDOWN_ROW_FORMAT % [DetailFormat.MORALE_BREAKDOWN_INDENT, glyph, factor, label]

## **DOES THIS BAND STATE ITS KIT AT ALL?** — `has()`, never `> 0`, because a dry kit is a real and
## important reading and only an ABSENT field means "not stated". One test behind the Kit row and its
## disclosure, so a band cannot show one without the other.
static func band_states_kit(band: Dictionary) -> bool:
    return not (band.get(KIT_ITEM_CONDITIONS_KEY, []) as Array).is_empty()

## The band's hunt crews, best-equipped first. Empty for a cohort that publishes none (a snapshot
## predating the field), which every reader below treats as *nothing to say*.
static func band_hunt_crews(band: Dictionary) -> Array:
    return band.get(HUNT_CREWS_KEY, []) as Array

## **THE BAND'S HUNT HEAD COUNT** — `Σ` the crews' workers, which is the sim's own denominator and the
## only one on the wire. Never re-derived from a labor assignment: the crews are what the take was
## resolved against, so a second count could disagree with the split it is the denominator for.
static func band_hunt_headcount(band: Dictionary) -> float:
    var total := 0.0
    for crew in band_hunt_crews(band):
        total += maxf(float(crew.get(HUNT_CREW_WORKERS_KEY, 0.0)), 0.0)
    return total

## How many workers hold this item, `0` when the band publishes no row for it. **A `0` is three
## different sentences** — nobody staffed on the job, the band owns none, or no quoted kit carries the
## item — so this number alone never states a shortfall; `kit_coverage` below is what does.
static func kit_workers_holding(band: Dictionary, item_id: String) -> float:
    for row in band.get(KIT_ITEM_CONDITIONS_KEY, []):
        if String(row.get(KIT_ITEM_ID_KEY, "")) == item_id:
            return maxf(float(row.get(KIT_ITEM_WORKERS_HOLDING_KEY, 0.0)), 0.0)
    return 0.0

## **HOW FAR AN ITEM REACHES INTO THE JOB THAT USES IT** — `{stated, holding, short, headcount}`, all
## three counts WHOLE PEOPLE, `stated` false when there is nothing to say.
##
## **THE DENOMINATOR IS PUBLISHED, NOT DERIVED — AND ALL FOUR JOBS COME THROUGH HERE.**
## `workers_on_quoted_job` is the head count of the job the row is quoted at, resolved off the SAME
## coverage that produced `workers_holding`, so the pair provably describes ONE job and a basket's
## shortfall is stated exactly the way a spears shortfall is. The hunt had a private path while
## `Σ hunt_crews.workers` was the only job head count on the wire; it does not any more, and it must
## not grow one back — a second denominator is a second answer.
##
## **THE ZEROS ARE A RENDERING CONTRACT** (`.claude/rules/core_sim/equipment.md`):
## - `workers_on_quoted_job == 0` → **NOBODY IS STAFFED on that job.** `0 of 0` is not a shortfall —
##   a band with no gatherers needed no basket and none went unheld — so it must not tint anything,
##   and **nothing may divide by it**. That is this function's early return, and it is also what an
##   item NO quoted kit carries (a bench tool) reads.
## - a POSITIVE denominator with `workers_holding == 0` → the real shortfall, and the sharpest one:
##   the job is staffed and every worker on it is at the unequipped tier. It renders `0 of 4`.
##
## **THE THREE COUNTS ARE APPORTIONED, NOT ROUNDED INDEPENDENTLY.** Both halves are fractional, and
## rounding each on its own gives a `4 of 17` whose remainder is 13 — the largest-remainder split is
## the same reason the PEOPLE brackets sum to the band's own size.
static func kit_coverage(band: Dictionary, item_id: String) -> Dictionary:
    var blank := {"stated": false, "holding": 0, "short": 0, "headcount": 0}
    for row in band.get(KIT_ITEM_CONDITIONS_KEY, []):
        if String(row.get(KIT_ITEM_ID_KEY, "")) != item_id:
            continue
        var staffed := maxf(float(row.get(KIT_ITEM_ON_QUOTED_JOB_KEY, 0.0)), 0.0)
        if staffed <= HUNT_CREW_WORKER_EPSILON:
            return blank
        var holding := minf(maxf(float(row.get(KIT_ITEM_WORKERS_HOLDING_KEY, 0.0)), 0.0), staffed)
        # The two halves partition the job's own head count, so that is the target they must sum to.
        var parts := HudFormat.apportion_people_to(
            [holding, staffed - holding], int(round(staffed)))
        return {"stated": true, "holding": parts[0], "short": parts[1],
            "headcount": parts[0] + parts[1]}
    return blank

## Is this item still equipped? The schema's own rule and the only test there is (see `KIT_DRY`).
##
## **An item with no published row reads as DRY, not as equipped.** A missing row means the server
## never confirmed the gear; promising a kitted tier on that silence is the failure this whole model
## exists to prevent, so it errs toward the unequipped answer.
static func kit_is_equipped(band: Dictionary, item_id: String) -> bool:
    return kit_condition(band, item_id) > KIT_DRY

## One item's remaining condition, `KIT_DRY` when the band publishes no row for it.
static func kit_condition(band: Dictionary, item_id: String) -> float:
    for row in band.get(KIT_ITEM_CONDITIONS_KEY, []):
        if String(row.get(KIT_ITEM_ID_KEY, "")) == item_id:
            return float(row.get(KIT_ITEM_REMAINING_KEY, KIT_DRY))
    return KIT_DRY

## One kit's condition as the Kit ROW says it — the whole number, or the word for a kit that has run
## out. **Never a bar, never a fraction of a maximum**: performance is flat until expiry, so a filled
## gauge would draw a taper the model does not have.
static func kit_condition_face(band: Dictionary, item_id: String) -> String:
    return String.num(kit_condition(band, item_id), KIT_CONDITION_DECIMALS) \
        if kit_is_equipped(band, item_id) else KIT_DRY_FACE

## One `    ▲ Spears 87 — attack 20` breakdown row. Through the SAME ▲/▼ sign glyphs the food and
## morale breakdowns tint by, so the popover has one two-tone rule rather than a per-family one.
##
## **THE GLYPH ASKS WHETHER THE ITEM IS SOUND, NOT WHETHER IT EXISTS.** It used to be a bare
## `equipped` test; a partly-armed band's spears are equipped and are *also* the band's biggest
## problem, so a shortfall takes ▼ (WARN ink over the whole row) exactly as a dry item does. The two
## states then say WHICH they are in words — `— bare hands` for the cliff, the coverage clause for the
## shortfall — rather than being told apart by the glyph.
##
## `role` is composed by the caller from THAT KIT's own tier. It is a parameter rather than a lookup
## here on purpose: the wrong pairing (a sled quoting the forage carry) is the defect this arc keeps
## reproducing, so the pairing is written once per kit at the one call site and is assertable there.
static func kit_breakdown_row(band: Dictionary, item_id: String, label: String,
        role: String) -> String:
    var equipped := kit_is_equipped(band, item_id)
    var coverage := kit_coverage(band, item_id)
    var short := int(coverage["short"]) > 0
    var glyph := MORALE_CONTRIB_NEGATIVE_GLYPH if (not equipped or short) \
        else MORALE_CONTRIB_POSITIVE_GLYPH
    var suffix := "" if equipped else KIT_BARE_HANDS_SUFFIX
    if short:
        # **NOBODY HOLDING IT GETS ITS OWN SENTENCE.** `only 0 of 4 workers carry one` is arithmetic
        # where "none of your 4" is the fact, and this is the reading the pair exists to make sayable.
        suffix += KIT_COVERAGE_BREAKDOWN_NONE_FORMAT % coverage["headcount"] \
            if int(coverage["holding"]) <= 0 \
            else KIT_COVERAGE_BREAKDOWN_FORMAT % [coverage["holding"], coverage["headcount"]]
    return KIT_BREAKDOWN_ROW_FORMAT % [MORALE_BREAKDOWN_INDENT, glyph, label,
        kit_condition_face(band, item_id), role + suffix]

## One `    ▲ +0.48  Gathered`-style breakdown row (morale-indent + sign glyph → shared tint path).
static func food_breakdown_row(value: float, label: String) -> String:
    return _breakdown_row(value, SourceForecast.format_signed(value), label)

## The FODDER larder's breakdown row — the same indent, the same ▲/▼ and the same tint as the food
## row above, with the number at the FODDER account's own resolution (`format_fodder`, one decimal).
##
## **THE SHAPE IS SHARED AND THE RESOLUTION IS NOT.** Every convention that makes a breakdown row
## READ as one is common (`_breakdown_row`), so the two ledgers cannot drift apart on the glyph, the
## indent or the sign; but a fodder rate is a one-decimal quantity wherever the client prints it
## (`SourceForecast.format_fodder` — its stock and its rate share that renderer), and printing
## `+5.00 Grown` under a `100.0` stock would state the same account at two precisions.
static func fodder_breakdown_row(value: float, label: String) -> String:
    return _breakdown_row(value, SourceForecast.format_signed_fodder(value), label)

## ---- THE STANDING MATERIAL BILL'S BREAKDOWN ROWS -------------------------------------------------
##
## The `Upkeep:` popover's shape, one block per good: the good's NAME, then what is wanted, then what
## arrives, then what is on the shelf. Three producers because the three lines are three kinds of
## statement — a heading, a signed flow, a stock — and the renderer tells them apart by exactly that.

## The block's heading — the good, capitalised, on its own full-width line. **NOT indented**, which is
## what keeps it out of `detail_bbcode`'s signed-sub-line branch and reading as the thing the rows
## under it are about. **A material names itself**: the catalogue ships no display word, so the id IS
## the noun (`SourceForecast.PICKER_MATERIAL_PRODUCT_FORMAT`'s rule).
static func material_bill_heading(material_id: String) -> String:
    return material_id.substr(0, 1).to_upper() + material_id.substr(1)

## One SIGNED term of the block — `    ▼ -0.05  Wanted`. The shared indent and the ▲/▼ the sign picks,
## exactly as the food and fodder ledgers' rows, so the three accounts cannot drift apart on the
## glyph. Materials print at `MATERIAL_BILL_DECIMALS` and are trimmed, which is finer than either
## larder because a fence's mending bill is `0.05` a turn and a one-decimal rendering would read `0.1`.
static func material_bill_row(value: float, label: String) -> String:
    return _breakdown_row(value, MATERIAL_BILL_SIGNED_FORMAT % [
        MATERIAL_BILL_POSITIVE_SIGN if value > 0.0 else MATERIAL_BILL_NEGATIVE_SIGN,
        format_trimmed(absf(value), MATERIAL_BILL_DECIMALS)], label)

## The block's STOCK line — `    2  On the shelf`. It carries the shared indent and **NO SIGN**, which
## is what routes it to the neutral ink: a stock is neither good news nor bad, and the runway on the
## summary row above already said which. Deliberately not `_breakdown_row`, whose whole job is to pick
## a ▲/▼ — a stock has no direction to pick one from.
static func material_bill_stock_row(value: float) -> String:
    return "%s%s  %s" % [MORALE_BREAKDOWN_INDENT,
        format_trimmed(value, MATERIAL_BILL_DECIMALS), MATERIAL_LABEL_STORE]

## The three labels, in the order a player asks them: what the band's holdings want, what its sources
## and its bench bring in, and what is left on the shelf between the two.
const MATERIAL_LABEL_WANTED := "Wanted"
const MATERIAL_LABEL_ARRIVING := "Arriving"
const MATERIAL_LABEL_STORE := "On the shelf"

## A material figure's precision, and the sign it wears. Two decimals is what every material readout in
## this client prints at, and one step finer than the shipped pen's 0.05-a-turn fence bill — a coarser
## rendering would show that whole rate as a rounding artefact.
const MATERIAL_BILL_DECIMALS := 2
const MATERIAL_BILL_SIGNED_FORMAT := "%s%s"
const MATERIAL_BILL_POSITIVE_SIGN := "+"
## The TYPOGRAPHIC minus this client signs a debit with everywhere, not the ASCII hyphen — the same
## glyph `SourceForecast.format_signed` writes, so one column of numbers reads as one column.
const MATERIAL_BILL_NEGATIVE_SIGN := "−"

## Is this band's standing bill worth opening — **the food test, on the material account**. A good
## arriving slower than it is eaten, or a shelf inside the shared warn line, read through the SAME
## `BandFoodStatus` thresholds both larders' carets read. One rule for every account that can run out,
## which is what stops three surfaces disagreeing about what worrying looks like.
static func material_upkeep_is_concerning(band: Dictionary) -> bool:
    var worst := band_material_worst(band)
    if worst.is_empty():
        return false
    var turns := float(worst[MATERIAL_BILL_RUNWAY_KEY])
    return BandFoodStatus.is_limited(turns) and turns < BandFoodStatus.warn_turns()

## The shape BOTH ledgers' breakdown rows have: the shared indent, the ▲/▼ the sign picks, the
## already-formatted magnitude and the label. It takes the number as TEXT because the two accounts
## round differently and nothing else about the row does.
static func _breakdown_row(value: float, magnitude: String, label: String) -> String:
    var glyph := DetailFormat.MORALE_CONTRIB_POSITIVE_GLYPH if value > 0.0 else DetailFormat.MORALE_CONTRIB_NEGATIVE_GLYPH
    return "%s%s %s  %s" % [DetailFormat.MORALE_BREAKDOWN_INDENT, glyph, magnitude, label]


# =====================================================================================
#  PURE LINE PRODUCERS
#
#  The detail-line producers that turned out to hold no HUD state at all once their single
#  reach-out was threaded in as a parameter (`world_herds` for the herd drawer's danger bars,
#  the already-resolved `target_herd` for the expedition delivery lines). The STATEFUL producers
#  — the band summary rows, which read the labor model and register disclosures — live in
#  `BandDetailLines` instead.
# =====================================================================================

## The HERD drawer's rows. The split with the roster row above this drawer: the ROW carries identity
## (species glyph + name) and STAFFING (`1 🏹`) — so no `Herd` / `Species` row here, which would be
## the same name a second time. The SIZE class lives here because the row's one meta slot now belongs
## to the staffing count, and the drawer is where the facts that don't fit the row live. Everything
## below it is what the row can't show anyway: the herd's state.
##
## `world_herds` is THREADED IN (it is only ever forwarded to `append_danger_component_lines`, whose
## Attack/Defense bars normalize against the roster) — the same treatment the tint `Context` gets, and
## what makes this producer pure. Callers pass `HudBandLaborState.world_herds()`.
##
## **THE `assigned_keepers` PARAMETER IS GONE** (`docs/plan_standing_upkeep.md` §2.5). It carried the
## `maintain` crews summed across the player's bands, and maintenance left the tile: a managed herd
## is held out of its band's `husbandry` pool, so the drawer's own `Keeping:` row states this herd's
## share of it and the `Keepers:` row states the demand alone. Nothing in this producer needs a
## caller-supplied head count any more.
## **`unstaffed_build` IS THREADED IN FOR THE SAME REASON `world_herds` IS** — it is a fact about the
## player's LABOR ROW, which a pure producer over one herd dict cannot see, and it is the herd drawer's
## half of the declared-but-unstaffed readout: the rung a band has promised here and put nobody on
## (`HudBandLaborState.unstaffed_build_hunt`, `IMPROVEMENT_NONE` for none). It renders the Husbandry or
## Corral row that a meter of zero would otherwise suppress entirely; see `BUILD_UNSTARTED_VALUE`.
## `build_crew` is the SECOND labor fact this pure producer cannot see, threaded in for the same
## reason `unstaffed_build` is: `BUILD_METER_HOLDS` is a crew treading water with a crew on it and a
## build **parked on purpose** without one, and only the first is a hazard
## (`docs/plan_standing_upkeep.md` §4.6a). `BUILD_CREW_NONE` is the safe default and the honest one for
## a caller that was not told — on this web the wire cannot publish `HOLDS` with a crew on it at all,
## no animal rung declaring a `meter_decay`, so a staffed herd never reaches the fork.
##
## **`ctx` IS AN OUT-PARAMETER FOR THE ROW HOVERS AND NOTHING ELSE** — the `Context` the host is about
## to render this list through, filled here because the producer that knows a rung is slipping is the
## one that wrote its row (`Context.row_tooltips`). A caller with no host context passes none, and
## every line comes back exactly as before.
static func herd_summary_lines(herd_data: Dictionary, world_herds: Array,
        unstaffed_build: String = SourceForecast.IMPROVEMENT_NONE,
        build_crew: int = SourceForecast.BUILD_CREW_NONE,
        ctx: Context = null) -> Array[String]:
    var lines: Array[String] = []
    # A predator is a hunter, not quarry — the SAME `prey_sense_radius > 0` signal the map's prey-sense
    # ring keys on (carnivore == 4, herbivore == 0). A herbivore's drawer is byte-for-byte unchanged.
    var is_predator := int(herd_data.get("prey_sense_radius", 0)) > 0
    var size_class := String(herd_data.get("size_class", "")).strip_edges()
    if size_class != "":
        var size_format := HERD_SIZE_CLASS_PREDATOR_FORMAT if is_predator else HERD_SIZE_CLASS_FORMAT
        lines.append("%s: %s" % [HERD_SIZE_ROW, size_format % size_class.capitalize()])
    # The stock row carries what is standing vs the K its range supports as a `current / max` pair —
    # the convention the tile card's own `Foraging` / `Grazing` rows use, and now in the same unit the
    # hunt sheet answers in (see `HERD_STOCK_ROW`). K is derived each turn from the graze on the
    # herd's range; an overgrazed herd has `biomass > K`, so the pair honestly reads `current > max`
    # (e.g. `15 / 11`) — a FEATURE that makes the overshoot visible in the numbers (the ⚠ row below
    # spells out the consequence), and the reason this is a PAIR rather than a fill percentage. Guard:
    # a herd momentarily on barren range derives K = 0, so `carrying_capacity <= 0` falls back to the
    # bare `X` (never `X / 0`) and suppresses the overgrazing test below.
    #
    # **THE ECOLOGY PHASE RIDES THIS ROW** rather than standing as one of its own, exactly as it does
    # on the two food-web stock rows above it (`HudFloraVocab.STOCK_PHASE_CLAUSE_FORMAT`, whose
    # comment carries the reasoning): the phase is a condition OF the stock, and `_value_hex` keys
    # both row names to `ecology_value_hex`, which matches the phase word wherever in the value it
    # sits. So folding costs no styling fork.
    var corralled := bool(herd_data.get("corralled", false))
    var carrying_capacity := float(herd_data.get("carrying_capacity", 0.0))
    var biomass: float = float(herd_data.get("biomass", 0.0))
    var phase := String(herd_data.get("ecology_phase", "")).strip_edges().to_lower()
    if biomass > 0.0:
        var body_mass := float(herd_data.get("body_mass", 0.0))
        var head := SourceForecast.animal_count(biomass, body_mass)
        var stock_row := HERD_STOCK_ROW if head != SourceForecast.ANIMAL_COUNT_NONE \
            else HERD_STOCK_BIOMASS_ROW
        var current := head if head != SourceForecast.ANIMAL_COUNT_NONE else int(round(biomass))
        var value := str(current)
        if carrying_capacity > 0.0:
            # The ceiling counts in the SAME unit as the stock above it, or the pair states a ratio
            # between two different things. `animal_count`'s floor-at-one applies to it too: a range
            # that supports less than one body still supports one.
            var ceiling := SourceForecast.animal_count(carrying_capacity, body_mass) \
                if head != SourceForecast.ANIMAL_COUNT_NONE else int(round(carrying_capacity))
            value = "%d / %d" % [current, ceiling]
        if phase != "":
            value = HudFloraVocab.STOCK_PHASE_CLAUSE_FORMAT % [value, ecology_phase_label(phase)]
        lines.append("%s: %s" % [stock_row, value])
    # The grazing range — WHY the herd is this size (the tiles it grazes / derives K over). A CORRALLED
    # herd doesn't roam-graze a range, so its Range row + overgrazing test are meaningless (its K is a
    # frozen pen-time value); the penned herd keeps the merged `Biomass: X / Y` pair, plainly.
    if not corralled:
        var range_radius := int(herd_data.get("graze_range_radius", 0))
        lines.append("%s: %s" % [HERD_RANGE_ROW, graze_range_label(range_radius)])
    # Overgrazing: biomass exceeds what the range can sustainably feed (both numbers sim-provided — the
    # client compares, it does NOT re-derive the ecology). Suppressed for a corralled herd and when K is
    # unknown. The `X / Y` pair above already shows X > Y; this row states the consequence.
    if not corralled and carrying_capacity > 0.0 and biomass > carrying_capacity * (1.0 + OVERGRAZE_EPSILON):
        lines.append(OVERGRAZING_WARNING)
    # Predators Phase 0 — the four RAW combat components (strength ≠ danger), shown for EVERY herd
    # (a rabbit reads all-empty, a mammoth reads high-attack/high-fights-back/zero-aggressive — the
    # "deadly to hunt, no camp threat" story at a glance). No verdict word; each row is a relative bar
    # + the raw value, Elevation-style.
    append_danger_component_lines(lines, herd_data, world_herds)
    # Grazing 2d-δ — how far up the husbandry ladder THIS species can climb gates the whole section.
    # A WILD-ceiling herd shows NO husbandry track at all (just the hunt-only hint); a PASTORAL one
    # keeps the domestication track but can never be penned (hint where Corral would sit); a PEN one
    # (or empty/absent) shows the full ladder, exactly as before.
    var ceiling := SourceForecast.husbandry_ceiling(herd_data)
    if ceiling == SourceForecast.HUSBANDRY_CEILING_WILD:
        lines.append(HUSBANDRY_WILD_PREDATOR_HINT if is_predator else HUSBANDRY_WILD_HINT)
    else:
        # **ONE ROW PER LIVE METER, AND THE TURNS LEAD IT** (issue #545) — the animal twin of the tile
        # card's rows, through the same `rung_row_value` fork, so a rung's four hazard states cannot
        # be worded one way on a patch and another on a herd. The work absolutes came off this
        # surface with them: `0.3 / 100 work` is compose-sheet detail.
        var domestication := float(herd_data.get("domestication", 0.0))
        var herd_prefix: String = HudComposeVocab.BARE_FORECAST_PREFIX
        var tamed := domestication >= HUSBANDRY_PROGRESS_COMPLETE
        var tame_declared := unstaffed_build == SourceForecast.IMPROVEMENT_TAME
        # **BOTH ROWS RENDER WHEN BOTH METERS ARE LIVE.** A herd holding a finished Tame while its pen
        # goes up states `Husbandry: 🐄 Domesticated 100%` over `Corral: ≈40 turns (8%)` — one row
        # would silently drop either the rung you hold or the build in flight.
        if tamed or domestication > BUILD_METER_EMPTY or tame_declared:
            # **A HERD NOBODY IS ON READS AS *HELD* HERE, and that is the sim's own `-2`**
            # (`docs/plan_standing_upkeep.md` §4.6a). No animal rung declares a `meter_decay`, so an
            # animal meter never goes backwards: an abandoned Tame is parked exactly where it was
            # left, which is a decision rather than a failure and takes no mark. It rendered
            # `⚠ Stalled` while the wire answered `-1` for an unstaffed source.
            lines.append("%s: %s" % [HUSBANDRY_ROW, rung_row_value(herd_data, herd_prefix,
                SourceForecast.IMPROVEMENT_TAME, SourceForecast.SOURCE_KIND_HERD,
                husbandry_built_label(), tamed, domestication, build_crew, unstaffed_build)])
            note_under_kept_hover(ctx, HUSBANDRY_ROW, herd_data, herd_prefix,
                SourceForecast.SOURCE_KIND_HERD, SourceForecast.IMPROVEMENT_TAME)
            # …and, on a row with no REMEDY to state, what the ladder is buying. The remedy wins the
            # slot outright when both apply: a herd shedding animals is not asking what a finished
            # Tame would be worth.
            if ctx != null and not ctx.row_tooltips.has(HUSBANDRY_ROW):
                var payoff := husbandry_payoff_hover(herd_data, herd_prefix)
                if payoff != "":
                    ctx.row_tooltips[HUSBANDRY_ROW] = payoff
            if not tamed:
                lines.append_array(build_gear_lines(herd_data, herd_prefix))
                # …and, on a BLOCKED queue alone, what frees it. This is the web the measured case
                # lives on: a half-tamed herd drawn to its escapement floor by hunters while its
                # keeping goes unpaid (`build_blocked_lines`).
                lines.append_array(build_blocked_lines(herd_data, herd_prefix,
                    SourceForecast.SOURCE_KIND_HERD))
        # **THE CONSEQUENCE OF AN UNDER-KEPT HERD IS THE ONE KEEPER FACT THAT SURVIVED THE `Keepers:`
        # ROW** (issue #545). That row stated a demand every turn, on a herd where nothing was wrong,
        # and read as noise; this fires ONLY when the band's Husbandry pool failed to cover this herd
        # (`SourceForecast.is_under_kept`, the same test the work board's ⚠ and the Husbandry row's
        # own mark make), and it is the only place in the client that says animals are drifting off.
        # It carries the head count because a head count only matters when it is short.
        if int(herd_data.get("herders_needed", 0)) > 0 and domestication > BUILD_METER_EMPTY \
                and SourceForecast.is_under_kept(herd_data, herd_prefix):
            lines.append(HERDERS_SHED_FORMAT % SourceForecast.keepers_wanted(
                herd_data, herd_prefix))
        # A corralled herd is penned by the band (intensification ladder). SIGNAL-tinted, mirroring the
        # Husbandry/Ecology row treatment. While the keepers are still BUILDING the pen (0 < progress < 1
        # under the Corral policy) the same row reports the meter — the animal twin of the tile card's
        # "Cultivation N%" row, so the investment the player committed to is visibly under way.
        # A PENNED herd is a managed population: it eats its fenced footprint's grass plus whatever
        # fodder its keeper carries in, and an underfed one is shrinking right now. That is the loudest
        # thing the drawer can say about it, and the `Fed:` row below says it — the mark, the fed
        # fraction, where the feed came from and what would fix it. **The Corral row states the RUNG
        # and nothing else**: it wore the starving face beside its own build meter, which read as two
        # unrelated percentages on one row. There is no companion cost row either — a pen bills the
        # FOOD larder for nothing, so the shortfall has a consequence, never a price.
        # The whole corral/pen readout is PEN-ceiling only — a pastoral herd can never be penned (the
        # server never builds one), so its Corral/pen rows are suppressed and a hint stands in their place.
        if ceiling == SourceForecast.HUSBANDRY_CEILING_PEN:
            var corral_progress := float(herd_data.get("corral_progress", 0.0))
            if bool(herd_data.get("corralled", false)):
                lines.append("%s: %s" % [CORRAL_ROW, rung_row_value(herd_data, herd_prefix,
                    SourceForecast.IMPROVEMENT_CORRAL, SourceForecast.SOURCE_KIND_HERD,
                    corral_built_label(), true, CORRAL_PROGRESS_COMPLETE,
                    SourceForecast.BUILD_CREW_NONE, SourceForecast.IMPROVEMENT_NONE)])
                note_under_kept_hover(ctx, CORRAL_ROW, herd_data, herd_prefix,
                    SourceForecast.SOURCE_KIND_HERD, SourceForecast.IMPROVEMENT_CORRAL)
                # The pen is fenced LAND (Grazing 2d-γ): its footprint (radius + the SERVER's in-bounds
                # tile count, shown verbatim) and the `Fed:` row — how much of its demand arrived, which
                # of its TWO sources it came from, and how much more fodder a turn would close the gap.
                var pen_radius := int(herd_data.get("pen_radius", 0))
                var footprint_tiles := int(herd_data.get("pen_footprint_tiles", 0))
                lines.append("%s: %s" % [PEN_FOOTPRINT_ROW, PEN_FOOTPRINT_FORMAT % [pen_radius, footprint_tiles]])
                lines.append("%s: %s" % [PEN_FEED_ROW, pen_feed_value(herd_data)])
            elif corral_progress > 0.0 \
                    or unstaffed_build == SourceForecast.IMPROVEMENT_CORRAL:
                # Penning is a flat job for every species — a fence is a fence — so unlike the Tame
                # row above this cost carries no species multiplier, and the turns leading the row
                # move only with the keeper crew, their floor and their kit.
                lines.append("%s: %s" % [CORRAL_ROW, rung_row_value(herd_data, herd_prefix,
                    SourceForecast.IMPROVEMENT_CORRAL, SourceForecast.SOURCE_KIND_HERD,
                    corral_built_label(), false, corral_progress,
                    build_crew, unstaffed_build)])
                note_under_kept_hover(ctx, CORRAL_ROW, herd_data, herd_prefix,
                    SourceForecast.SOURCE_KIND_HERD, SourceForecast.IMPROVEMENT_CORRAL)
                lines.append_array(build_gear_lines(herd_data, herd_prefix))
                lines.append_array(build_blocked_lines(herd_data, herd_prefix,
                    SourceForecast.SOURCE_KIND_HERD))
        elif ceiling == SourceForecast.HUSBANDRY_CEILING_PASTORAL:
            lines.append(HUSBANDRY_PASTORAL_HINT)
    # **NO `Position` ROW.** These lines render in ONE place — the tile card's subject drawer — and
    # the card's own header states the hex two rows above them (`TILE (34, 24)`), so a herd stating
    # it again was the same coordinate pair twice on one card. `Next waypoint` below is a different
    # fact — where it is HEADING, which nothing else on the card says — and stays.
    # **WHAT THIS HERD IS ABOUT TO LOSE IS ON THE RUNG ROW ITSELF NOW** — `⚠ drifting` beside the
    # meter, with the role that pays it on that row's hover. The `At risk:` row that used to stand
    # here, its shortfall and its countdown are retired; `HERDERS_SHED_FORMAT` above is what still
    # states the consequence in a sentence, and it states it once.
    var next_x := int(herd_data.get("next_x", -1))
    var next_y := int(herd_data.get("next_y", -1))
    if next_x >= 0 and next_y >= 0:
        lines.append("Next waypoint: (%d, %d)" % [next_x, next_y])
    return lines

## An Active-expeditions row's hover text: everything the glyphs encode, in words — the mission, what
## the party's escapement FLOOR means for the herd, the phase + what it means, and the click
## affordance.
##
## `target_herd` is the party's OWN target resolved from the snapshot herd list ({} when it has none
## or the herd is gone) — threaded in for the same reason `world_herds` is: this layer holds no
## snapshot state. Callers pass `HudBandLaborState.expedition_target_herd(exp)`.
##
## `denial_view` is the same already-answered forecast the parties strip's `Collapse:` row renders,
## handed in for the same reason (`expedition_collapse_line`). A DENIAL party's orders are just "this
## herd, these hands", so what its hover adds is the one thing the row cannot show: the collapse
## verdict. `join_tooltip_lines` drops the `""` a hunt or a scout answers here, so neither gains a line.
static func expedition_row_tooltip(exp: Dictionary, phase: String, target_herd: Dictionary,
		denial_view: Dictionary = {}) -> String:
    var mission := String(exp.get("expedition_mission", "")).strip_edges().to_lower()
    # THE PARTY'S ORDERS — `expedition_floor`, where this raid stops (the retired
    # `expeditionHuntPolicy` string is a `(deprecated)` wire slot). `1.0` is the sim's value for a
    # scout or a resident band, and it is a legal raid floor too, so the hint is gated on the MISSION
    # rather than on the number.
    var floor_hint := ""
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_HUNT:
        floor_hint = HudFormat.floor_hint(
            float(exp.get("expedition_floor", SourceForecast.DEFAULT_HARVEST_FLOOR)),
            SourceForecast.LABOR_KIND_HUNT, true)
    var collapse_line := expedition_collapse_line(exp, target_herd, denial_view) \
        if mission == HudExpeditionVocab.EXPEDITION_MISSION_DENY else ""
    return HudFormat.join_tooltip_lines([
        expedition_mission_label(mission), floor_hint,
        expedition_orders_line(exp, mission),
        HudFormat.status_tooltip_line(phase), _expedition_delivery_tooltip_line(exp, mission, target_herd),
        expedition_trip_bound_line(exp, mission), collapse_line,
        EXPEDITION_ROW_FOCUS_HINT])

## **THE PARTY'S ORDERS** — how deep to draw the herd: *"Orders: 30% left standing"*.
##
## **IT IS A MERGED ROW THAT NOW CARRIES ONE CLAUSE, and it stays merged.** It was `Leaves standing:`
## and `Fill target:` as two rows, then one sentence stating both; the fill target is retired (issue
## #491 — trip length is a species-and-kit constant, so the lever moved nothing party size did not
## already fix), and what is left is the floor alone. The ROW is what the parties inspector strip
## budgeted for: that strip is the detail panel for a launched party, lives in a `clip_contents` zone
## capped at ~300px on a horizontal dock, and a hunt party carrying every optional line at once overran
## it — so a second orders row must not come back for the next order the party learns to carry.
##
## `""` for a scout, a denial party or a resident band — none of them evaluates a floor.
static func expedition_orders_line(exp: Dictionary, mission: String) -> String:
    if mission != HudExpeditionVocab.EXPEDITION_MISSION_HUNT:
        return ""
    var floor_value: String = HudComposeVocab.FLOOR_VALUE_FORMAT % SourceForecast.floor_percent(
        float(exp.get("expedition_floor", SourceForecast.DEFAULT_HARVEST_FLOOR)))
    return EXPEDITION_ORDERS_ROW_FORMAT % floor_value

## **WHICH STOP WILL END THIS PARTY'S RAID**, in the same words the pre-launch readout uses
## (`SourceForecast.TRIP_BOUND_CLAUSES`) — one table, so what the sheet promised and what the party
## reports cannot be phrased differently.
##
## `""` on the wire is NOT RAIDING (a resident band, a scout, or a party already walking a load home)
## and is deliberately distinct from `"horizon"`, which is the projection having found no stop; both
## render nothing, but for reasons that are not interchangeable and must not be collapsed here.
static func expedition_trip_bound_line(exp: Dictionary, mission: String) -> String:
    if mission != HudExpeditionVocab.EXPEDITION_MISSION_HUNT:
        return ""
    return SourceForecast.trip_bound_clause(
        {SourceForecast.TRIP_BOUND_KEY: String(exp.get("expedition_trip_bound",
            SourceForecast.TRIP_BOUND_NONE))})

## The full-wording next-delivery line for a hunt row's tooltip — the compact `· ~14 in 6t` token on
## the row itself is legible-but-terse in the 300px column, so hover carries the same phrasing the
## drawer's `BandDetailLines.expedition_summary_lines` prints. Empty (dropped by
## `HudFormat.join_tooltip_lines`) for a scout party or a party not yet projecting a delivery.
static func _expedition_delivery_tooltip_line(exp: Dictionary, mission: String, target_herd: Dictionary) -> String:
    if mission != HudExpeditionVocab.EXPEDITION_MISSION_HUNT or not exp.has("expedition_projected_delivery"):
        return ""
    return expedition_next_delivery_line(exp, target_herd)

## **THE IN-FLIGHT DENIAL READOUT** (`docs/plan_denial_raid.md` §3) — the collapse verdict where a
## hunt party shows `Next delivery`. A denial party publishes no `expeditionProjectedDelivery` /
## `expeditionEtaTurns` / `expeditionTripBound` at all, deliberately: its question is not when food
## arrives, it is whether the herd goes past the point of no return.
##
## **THE ANSWER IS HANDED IN, AND THAT IS WHAT KEEPS THIS LAYER STATIC.** The sim publishes no
## per-party collapse field and the pre-launch denial TABLE this row used to read is gone — the
## forecast is a request/response on the command socket now (`ForecastQuery`). A query needs a request
## id, a staleness rule and a socket, and a formatter that held any of the three would be a controller;
## so the PARTIES STRIP asks on the launched party's own behalf (a detached party is a band, so
## `DenialRaidForecastQuery` takes it unchanged) and passes the answer down. It is the treatment
## `target_herd` already gets one argument to the left, for the same reason.
##
## `view` is `ForecastQuery.view()`'s `{state, answer, error}`. Every state renders SOMETHING once the
## party is a denial party with a live target: the verdict when it is ready, and otherwise the row
## saying whether the answer is still coming or has failed — a row that appeared only on success would
## pop into the strip a frame after it opens and change its height under the player.
##
## `""` when the target is gone from telemetry (the `Target:` row above already says the herd is not
## there), for a ready answer the sim priced at no party at all, and for an EMPTY `view` — which is a
## caller with no query seam to ask through (the Occupants drawer, a preview harness), not a question
## awaiting an answer. Rendering the pending row for one would promise a number nothing will deliver.
##
## The value carries its OWN `[color]`, the `_band_food_line` precedent: the verdict's severity is a
## fact about the forecast and not about the row KEY, so it cannot come from `_value_hex`'s key registry.
##
## **IT PASSES NO BAND TO THE FORECAST, SO THE VERDICT READS "…of raiding" RATHER THAN "…from launch"**
## — and that is the honest span here, not an omission. The launch sheet adds the OUTBOUND WALK because
## it knows where the party is starting from; this party has already left, its remaining walk is not on
## the wire (a denial mission publishes no `expeditionEtaTurns`), and adding the walk from the HOME
## BAND's tile would quote a leg the party may have finished turns ago. `denial_forecast` names the span
## it is quoting either way, so the two surfaces cannot be read as the same clock.
static func expedition_collapse_line(exp: Dictionary, target_herd: Dictionary,
        view: Dictionary) -> String:
    if target_herd.is_empty() or view.is_empty():
        return ""
    var state := String(view.get("state", ForecastQuery.STATE_PENDING))
    if state == ForecastQuery.STATE_PENDING:
        return "%s %s" % [DENIAL_COLLAPSE_ROW, HudComposeVocab.DENIAL_FORECAST_PENDING]
    if state == ForecastQuery.STATE_FAILED:
        return "%s %s" % [DENIAL_COLLAPSE_ROW,
            HudComposeVocab.FORECAST_FAILED_FORMAT % String(view.get("error", ""))]
    var answer: Dictionary = view.get("answer", {})
    # **THE PARTY IS HANDED IN FOR THE FORECAST HORIZON AND FOR NOTHING ELSE** — still no band, so still
    # no travel term and still the "…of raiding" span. The horizon is a global lever echoed onto every
    # cohort, so this launched party answers it exactly as its home band would; without it the verdict
    # falls back to naming a clock the player cannot see.
    var forecast := SourceForecast.denial_forecast(target_herd,
        answer.get("at_composed", {}), {}, 0, false, exp)
    var verdict := SourceForecast.denial_verdict_bbcode(forecast,
        SourceForecast.herd_display_name(target_herd))
    if verdict == "":
        return ""
    return "%s %s" % [DENIAL_COLLAPSE_ROW, verdict]

## The robust "Next delivery: …" wording, shared by the parties inspector strip
## (`BandDetailLines.expedition_summary_lines`) and the row tooltip (`expedition_row_tooltip`) so the
## two can never disagree. Caller has already confirmed this is a hunt party carrying the field. A
## projected 0 is a REAL answer, but it means one of TWO things — and the party's TARGET herd (which
## migrates and is often NOT the herd the player is inspecting) tells them apart: if the target id is
## still in the herd telemetry the raid returns empty because that herd is at/below its policy floor;
## if `target_herd` came back empty the target was lost/replaced and the party is coming home. Never
## blank the line as if there were no forecast at all, and never imply it is the herd on the tile the
## player is looking at.
static func expedition_next_delivery_line(exp: Dictionary, target_herd: Dictionary) -> String:
    var delivery := float(exp.get("expedition_projected_delivery", 0.0))
    if delivery <= 0.0:
        if target_herd.is_empty():
            return EXPEDITION_NEXT_DELIVERY_TARGET_LOST
        return EXPEDITION_NEXT_DELIVERY_NO_SURPLUS
    var amount := int(round(delivery))
    var eta := int(exp.get("expedition_eta_turns", 0))
    var line := ""
    if eta > 0:
        var turns_word := "turn" if eta == 1 else "turns"
        line = "Next delivery: ~%d food in %d %s" % [amount, eta, turns_word]
    else:
        line = "Next delivery: ~%d food (raid underway)" % amount
    if bool(exp.get("expedition_recurring", false)):
        line += "  %s" % EXPEDITION_RECURRING_GLYPH
    return line
