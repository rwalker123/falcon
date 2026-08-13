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
## `KEEPERS_ROW`, `FULLY_HERDED`, `CORRAL_PROGRESS_COMPLETE`, `PEN_FEED_ROW`) and the expedition
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
# The rung LABELS below (`cultivation_label` / `field_label` / `corral_label`) weld the glyph to its
# words — "🌾 Tended Patch" — which a one-glyph column cannot take. These are the same marks with the
# words stripped, so a mark column reads the glyph and a detail row reads the label WITHOUT either
# slicing the other's string. One home per glyph, and every one of them is REUSED, never minted here:
#   plants:  wild → 🌾 Tended Patch → ▦ Field       animals: wild → ◎ pastoral → 🐄 penned
# The two FIELD/PASTORAL marks come from `FoodIcons.POLICY_ICONS` — the ladder's own table, where each
# verb wears the glyph of THE RUNG IT BUILDS, so `sow`'s ▦ IS the Field's mark and `tame`'s ◎ IS the
# pastoral herd's. **The animal side has no rung glyph of its own and must borrow**: `husbandry_label`
# (Domesticated) and `corral_label` (Corralled) BOTH wear 🐄, so reusing it for the pastoral rung would
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

# The two DEBIT rows, deliberately separate: the people eat (`food_consumption`), and the ANIMALS in
# the band's pens eat (`pen_feed_upkeep` — a confined herd cannot graze, so its keeper hauls it food
# every turn). Both come straight off the same larder, and telling them apart is the entire readout
# of the corral-as-a-managed-population arc: a band whose larder drains because it is feeding its
# herd must be able to SEE that, not just watch the number fall.
const FOOD_LABEL_EATEN := "Eaten (people)"

const FOOD_LABEL_PEN_FEED := "%s Pen feed (animals)" % CORRAL_GLYPH

# The RAID debit (Predators Phase 3): food this band lost to predator raids this turn — the raid twin
# of Pen feed. The sim answers it as `PopulationCohortState.raidForfeit` (the client never re-derives
# it), and it is the fourth term of the larder identity
# `larder_delta == income − consumption − pen_feed − raid_forfeit`. Crossed-swords glyph so the row
# reads as a loss to an attacker, matching the command feed's `predator_raid` alert.
const RAID_GLYPH := "⚔"
const FOOD_LABEL_RAID_FORFEIT := "%s Lost to raids" % RAID_GLYPH

# The TRANSFER pair (arc #527): food that crossed between bands, in or out. They are the fifth and
# sixth terms of the larder identity
#   larder_delta == income − consumption − pen_feed − raid_forfeit + received − sent
# and they close a hole that was NEVER about trade alone: `balance_supply_networks` has been pooling
# food between neighbouring larders every turn since turn one, so any two co-networked bands had a
# Food line that silently did not add up — by the whole transfer, not a rounding drift.
#
# TWO ROWS, NOT ONE SIGNED ONE, matching the two debit rows above and the wire's own shape: a band
# that both sends and receives in one window is doing something, and a net would render that as
# nothing having happened. The received row is an INCOME row (▲ green) and the sent row a DEBIT
# (▼ amber), which the shared `food_breakdown_row` decides from the sign it is handed.
#
# **A PLAIN ARROW PAIR, NOT A HANDSHAKE OR A CART**, for two reasons. What these rows report is
# neighbours pooling as often as it is a shipment arriving, so a trade glyph would promise a deal the
# supply network never made. And the emoji that says "deal" (🤝) is not in this client's fallback
# font: it renders as an INVISIBLE gap — no tofu box, just a wider indent — which is the silent
# failure mode `Typography.gd` is retired for. ⇄ is in the Arrows block the ▸/◀/▲▼ carets already
# come from, so it draws everywhere they do. **One glyph for both rows**, unlike Pen feed and Lost to
# raids: these two are ONE fact in two directions, and the row's own words say which way.
const TRANSFER_GLYPH := "⇄"
const FOOD_LABEL_TRANSFER_RECEIVED := "%s From other bands" % TRANSFER_GLYPH
const FOOD_LABEL_TRANSFER_SENT := "%s To other bands" % TRANSFER_GLYPH

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
const KIT_LABEL_HUSBANDRY_GEAR := "Handling gear"
const KIT_LABEL_WAYFINDING := "Wayfinding"
const KIT_LABEL_CLUBS := "Clubs"
const KIT_DURABILITY_KEY_HUSBANDRY_GEAR := "husbandry_gear"
const KIT_DURABILITY_KEY_WAYFINDING := "wayfinding"
const KIT_DURABILITY_KEY_CLUBS := "clubs"

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
    "husbandry_gear": KIT_LABEL_HUSBANDRY_GEAR,
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
# **IT READS IN WORK UNITS, NOT AS A MULTIPLIER** (`docs/plan_unit_costed_work.md` §6). `×1.5` said
# the gear made the crew faster; what it actually does is take a fixed number of units off the JOB,
# per equipped worker — which is why the same tool is worth a lot on a garden and nearly nothing on a
# farm, and why a multiplier could never say so. A kit whose gear is spent takes nothing off and the
# clause disappears, exactly as the neutral multiplier's did.
const KIT_ROLE_BUILD_WORK_SUFFIX := " · %s work off a tame or a pen, per keeper"
# The contribution reads to one place: the shipped 8.5 is a playtest dial and a second decimal would
# imply a precision the number does not have.
const KIT_BUILD_WORK_DECIMALS := 1
# **The value that means "this gear changes no build"** — the schema's own default and what every kit
# but `husbandry` resolves to. Named so the suffix's suppression reads as a stated rule rather than a
# comparison against a bare literal.
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

# ---- Keeper staffing. The row KEY is read by the herd-lines producer below AND by this file's tint
# registry; `FULLY_HERDED` is the `herded_fraction` wire default (1.0 = fully staffed, also
# unmanaged/vanished herds) — treated as "no problem".
#
# **THE ROW IS `Keepers`, AND IT NAMES A DEMAND ON THE BAND'S HUSBANDRY ROLE**
# (`docs/plan_standing_upkeep.md` §2.5). It read `Herders` while a herd's containment came off its
# HUNTING crew and counted hunters, then counted the per-source `maintain` crew; maintenance has
# since left the tile, so what it states is how much of the band-wide pool this herd claims. The
# `under-herded` word stays: it names the HERD's state (the sim's own), not a crew.
const KEEPERS_ROW := "Keepers"
const FULLY_HERDED := 1.0
## **THE ROW STATES A DEMAND, NOT A STAFFING PAIR** (`docs/plan_standing_upkeep.md` §2.5). It read
## `A / N` while keepers were assigned per herd; maintenance left the tile, so there is no `A` to
## state — this herd draws from the band's `husbandry` pool — and an `A` invented from the pool share
## would be a head count the sim never stated. What survives is `herdersNeeded`, the herd's own
## demand in hands, plus which side of the pool's shortfall this herd landed on.
const HERDERS_STAFFED_FORMAT := "%d — drawn from the band's Husbandry"
const HERDERS_UNDER_FORMAT := "%d — under-herded, the Husbandry pool is short here"

# ---- Build-verb labels. "Building" / "Sowing" share the pen's "Fencing N%" convention: a rung under
# construction names the WORK, a finished one wears its own badge word. Each rung's "the meter is
# full" mark is its own const (progress arrives as 0..1 per rung).
const CORRAL_BUILDING_LABEL := "Building"
# The Tame rung's build verb — the animal twin of `HudFloraVocab.CULTIVATION_PREPARING_LABEL`, and
# the word the plant rungs' own comments already cite ("exactly as the herd's Husbandry row reads
# 'Domesticating N%'"). It was written inline at its one site until the work readout gave every rung
# one composer, at which point a literal there would have been the only verb not stated as one.
const HUSBANDRY_DOMESTICATING_LABEL := "Domesticating"
const CORRAL_PROGRESS_COMPLETE := 1.0
const HUSBANDRY_PROGRESS_COMPLETE := 1.0
const CULTIVATION_PROGRESS_COMPLETE := 1.0
const FIELD_PROGRESS_COMPLETE := 1.0
const FIELD_SOWING_LABEL := "Sowing"
const FIELD_BADGE_LABEL := "Field"

# ---- The pen's standing feed debit + its two starving states. The row KEY is read by the herd-lines
# producer below AND by this file's tint registry.
const PEN_FEED_ROW := "Pen feed"
const PEN_STARVING_LABEL := "⚠ Starving — %d%% fed"
const PEN_FEED_STARVING_FORMAT := "%s — only %d%% paid"

# ---- The penned herd's own rows (the fenced footprint + the three-way feed split). Every reader is
# `herd_summary_lines` below, so the whole block lives here.
const PEN_FOOTPRINT_ROW := "Pen"
const PEN_FOOTPRINT_FORMAT := "radius %d · %d tiles"
const PEN_FEED_SPLIT_ROW := "Fed by pasture"
# The `%s` is the optional hay segment (empty, or `PEN_FEED_SPLIT_HAY_SEGMENT`) spliced between the
# pasture percent and the NET larder bill — so a pen that drew no hay renders exactly the two-term form.
# The larder term reads `pen_larder_bill` (the NET bread bill after pasture + hay), NOT the gross
# `pen_upkeep`; sim-pinned invariant: `pen_upkeep × pen_pasture_fraction + pen_hay_food +
# pen_larder_bill == pen_upkeep`. A self-feeding pen reads "100% · larder 0.0", a scrub pen "0% ·
# larder N.N"; the hay segment shows ONLY when `pen_hay_food >= SourceForecast.FOOD_FLOW_MIN`.
const PEN_FEED_SPLIT_FORMAT := "%d%%%s · larder %.1f food/turn"
const PEN_FEED_SPLIT_HAY_SEGMENT := " · hay %.1f"

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
const HERDERS_SHED_FORMAT := "Under-herded — animals are drifting off. This herd wants %d of the band's Husbandry hands."

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
const OVERGRAZING_WARNING := "⚠ Overgrazing — range can't sustain this herd"

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
    var morale: float = NAN
    ## The band's fertility MULTIPLIER (`hunger x reserve x trend`), 1.0 = its normal birth rate.
    ## NAN when there is no band, or when the sim published no reading yet (the not-projected
    ## sentinel) — in which case no Growth row was emitted to tint.
    var fertility: float = NAN
    var disclosures: Dictionary = {}


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
            out += "[color=#%s]%s[/color]\n" % [row_hex, line]
            continue
        # The overgrazing warning is a full-width WARN sentence (biomass > K), tinted with the same
        # WARN_HEX the Ecology/Corral value rows use — not a parallel styling path, just the shared color.
        if line == OVERGRAZING_WARNING:
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
    elif key == "Husbandry":
        return husbandry_value_hex(value)
    elif key == KEEPERS_ROW:
        # A managed herd's staffing: amber when under-herded (animals shedding), ink when full.
        return herders_value_hex(value)
    elif key == "Cultivation":
        return cultivation_value_hex(value)
    elif key == HudFloraVocab.FIELD_ROW:
        # Plant rung 3 — the patch twin of the Corral row's tint (ink while building, signal once
        # complete). Same shape as Cultivation's; kept its own case because a Field is a different
        # rung with its own badge word, not a Tended Patch at a higher percentage.
        return field_value_hex(value)
    elif key == "Corral":
        return corral_value_hex(value)
    elif key == PEN_FEED_ROW:
        # The pen's running feed cost: amber as a standing debit, red when it goes unpaid.
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

## A quantity of WORK UNITS: whole numbers bare (`50`), fractions to one place (`17.6`). One unit is
## one worker-turn at the food peak with no gear, so a cost reads itself — and the shipped costs are
## integers, which a trailing `.0` would dress up as a measured figure.
static func format_work_units(value: float) -> String:
    if is_equal_approx(value, round(value)):
        return "%d" % int(round(value))
    return String.num(value, HudSelectionVocab.BUILD_WORK_DECIMALS)

## **WHAT THE JOB COSTS AND WHAT IT WOULD TAKE — the compose sheet's pre-commit quote**, as
## `50 work, ≈25 turns` (or `50 work` alone where there is no estimate to state). The caller resolves
## the turns half; on the compose sheet that is `SourceForecast.build_turns_at`, evaluated against
## the crew and floor the player is proposing, because a quote for a job nobody has started is
## precisely what the sim's own `buildTurnsRemaining` cannot answer.
## `""` for a rung the wire prices nothing on, which renders as no clause rather than a bare verb
## wearing an em-dash.
static func build_price_clause(work_cost: float, turns: int) -> String:
    if work_cost <= SourceForecast.BUILD_WORK_COST_NONE:
        return ""
    var price := HudComposeVocab.BUILD_PRICE_WORK_FORMAT % format_work_units(work_cost)
    if turns == SourceForecast.BUILD_TURNS_NO_ESTIMATE:
        return price
    return HudComposeVocab.BUILD_PRICE_TURNS_FORMAT % [price, build_turns_clause(turns)]

## **THE COMPOSE SHEET'S TURN CLAUSE — `≈20 turns`, or `≈1 turn`** — the count and its noun, decided in
## ONE place for both compose faces (the offered face's price and the running face's tail). They quote
## one estimate about one job, so a build one turn out that read `≈1 turns` on the sheet beside the
## tile card's `≈1 turn at this crew` would be the same number worded two ways on one screen.
## `HudSelectionVocab.BUILD_TURNS_ROW_ONE` is that card's half of the same pair.
static func build_turns_clause(turns: int) -> String:
    if turns == BUILD_TURNS_SINGULAR:
        return HudComposeVocab.BUILD_TURNS_COUNT_ONE
    return HudComposeVocab.BUILD_TURNS_COUNT_FORMAT % turns

## **THE TWO SUB-ROWS UNDER A RUNNING BUILD METER** — the sim's turn estimate, and what the crew's
## tools took off the job — indented so they read as an expansion of the meter row above them rather
## than as two more facts about the source. Both webs' hosts (the tile card's plant rungs, the herd
## drawer's animal ones) append this, so neither can grow a shape the other lacks.
##
## **A `-1` TURN ESTIMATE RENDERS NO LINE AT ALL.** The sim answers it for a stalled build and for a
## source nobody works, and a `0 turns` in its place promises a build about to land — the failure
## `BUILD_TURNS_NO_ESTIMATE` exists to name. The gear line is likewise absent at zero: a `−0 work`
## advertises a tool that did nothing.
##
## `prefix` spells the keys, so one call serves a `patch_`-prefixed `tile_info` and a bare herd dict.
## (The producer it describes is `build_estimate_lines`, below the keeping row.)

## The KEEPING row's key and its two value forms. `Keeping` rather than `Upkeep` because the row
## answers *what does it take to hold this*, in hands and work, and "upkeep" is already the pen feed's
## word for a bill paid in food.
const UPKEEP_ROW := "Keeping"

## **IT LEADS WITH THE POOL, BECAUSE THE ROW'S QUESTION CHANGED** (`docs/plan_standing_upkeep.md`
## §2.5). `upkeepSupplied` used to be the keepers standing here and read as *"did you staff this
## one"*; maintenance is a band-level role now and the number is this source's SHARE of the band's
## pool, so the row answers *"where is my shortfall landing"*. Wording it as a per-source staffing
## verdict would send the player looking for a stepper that no longer exists — the lever is the
## band's Agriculture / Husbandry card.
const UPKEEP_VALUE_FORMAT := "the pool covers %s of %s work — worth %d %s"

## The keeper noun, singular and plural. Its own pair rather than a `(s)` suffix, which reads as a
## form field rather than as a sentence.
const UPKEEP_KEEPER_ONE := "keeper"

const UPKEEP_KEEPER_MANY := "keepers"

## **A RUNG STILL GOING UP IS OWED ITS BUILDERS, NOT KEEPERS**, and this row says so in words.
##
## **ITS DEMAND IS NOT ZERO, WHICH IS EXACTLY WHY THE NUMBERS ARE SUPPRESSED HERE.** A meter still
## being raised is billed what it would LOSE — the plant web's rot rate, the animal web's whole
## keeping, since the animals are standing there whether or not the fence is up — so a patch mid-
## Cultivate publishes a small non-zero `upkeepDemand`. **No pool covers it**: the sim leaves an
## unbuilt rung out of the band's keeping pool entirely and credits its BUILD crew instead
## (`patch_upkeep_supply` / `herd_upkeep_supply`), so printing the pooled `x of y work` here would
## invite the player to raise a role that will never pay this bill.
const UPKEEP_MID_BUILD_VALUE := "still being built — its own crew holds it, no keepers are owed yet"

## **…AND THE SAME ROW WHEN NOBODY IS PAYING IT.** A rung still going up is owed its BUILD crew, so a
## build that was walked away from goes unpaid and the meter slides back — while the row above said
## the build's crew was holding it, which is the reassuring direction on a source that is bleeding.
## The two are told apart by the SHORTFALL and not by a crew count: `upkeepWorkersNeeded` is `0` in
## both cases (those hands are the build's either way) and the wire publishes no builder requirement,
## so a headcount here would be a client inventing one.
const UPKEEP_UNBUILT_VALUE := "nobody is building it — this rung is sliding back"

## The row that only appears when the keeping is UNDERPAID — its own key, so the tint registry can ink
## it as a warning without inking the bill above it.
const UPKEEP_RISK_ROW := "At risk"

const UPKEEP_LOST_SOON_FORMAT := "short %s work — this rung is lost in %d turn%s"

const UPKEEP_LOST_NOW_FORMAT := "short %s work — this rung is being lost NOW"

## **THE KEEPING ROW — what it costs to HOLD this source, and how long it has if nobody pays**
## (`docs/plan_standing_upkeep.md` §2, §2.4). One producer for BOTH webs, because the four upkeep
## fields ship under the same names on a patch and on a herd, and a card that worded the plant web's
## bill differently from the animal web's would be answering *"what does it cost to hold this?"* twice.
##
## **THE EDGE IS A CLIFF, WHICH IS WHY THE COUNTDOWN IS ON THE CARD AND NOT ONLY IN AN ALERT.** A
## completed meter sits exactly at its own cost, so the FIRST bleeding turn drops it below and the rung
## is lost — three unkept turns costs a tended patch, two costs a Field. A player who loses a 25-turn
## investment with no warning reads it as a bug, so the warning stands wherever the improvement does.
##
## Nothing here is derived: the demand, the supply and the shortfall are three published fields, and
## the countdown is `neglectGraceRemaining` read through its own flag. Empty on a source that owes
## nothing AND has nothing at risk — a wild patch prints no row rather than a `0.00 work` one.
static func upkeep_lines(src: Dictionary, prefix: String) -> Array[String]:
    var lines: Array[String] = []
    var state := SourceForecast.upkeep_state(src, prefix)
    if not SourceForecast.has_upkeep(state) and not bool(state.get("at_risk", false)):
        return lines
    var demand := float(state["demand"])
    var supplied := float(state["supplied"])
    var crew := int(state["crew"])
    # **THE BILL AND WHAT WAS PAID AGAINST IT, in one row.** `crew` is the maintain activity's own
    # `workers_needed`; it is `0` while the rung is still going up, because those hands are the
    # BUILD's — which the row says in words rather than printing "0 keepers".
    #
    # **AND THE MID-BUILD WORDS FORK ON THE SHORTFALL.** *"The build's crew holds it"* is true of a
    # build being worked and false of one that was walked away from — the same `0` crew, opposite
    # news — so a shortfall against a zero keeper demand states that nobody is paying instead. The
    # work board's ⚠ reads the identical test (`SourceForecast.is_unbuilt_and_unpaid`).
    var unbuilt := crew <= SourceForecast.NO_UPKEEP_CREW and SourceForecast.upkeep_is_short(state)
    var upkeep_value := UPKEEP_VALUE_FORMAT % [format_work_units(supplied),
        format_work_units(demand), crew, UPKEEP_KEEPER_ONE if crew == 1 else UPKEEP_KEEPER_MANY]
    if crew <= SourceForecast.NO_UPKEEP_CREW:
        upkeep_value = UPKEEP_UNBUILT_VALUE if unbuilt else UPKEEP_MID_BUILD_VALUE
    lines.append("%s: %s" % [UPKEEP_ROW, upkeep_value])
    if not SourceForecast.upkeep_is_short(state):
        return lines
    # **THE SHORTFALL IS THE DECAY, CONTINUOUSLY** (§2.4): half the hands means it slides at half rate.
    # The countdown beside it is the rung's remaining grace, which is what turns *it is bleeding* into
    # *you have two turns*.
    var grace := int(state["grace"])
    if not bool(state.get("at_risk", false)) or grace <= 0:
        lines.append("%s: %s" % [UPKEEP_RISK_ROW,
            UPKEEP_LOST_NOW_FORMAT % format_work_units(float(state["shortfall"]))])
        return lines
    lines.append("%s: %s" % [UPKEEP_RISK_ROW, UPKEEP_LOST_SOON_FORMAT % [
        format_work_units(float(state["shortfall"])), grace,
        "" if grace == 1 else HudAttentionVocab.ATTENTION_TURN_PLURAL_SUFFIX]])
    return lines

## **THE TWO SUB-ROWS UNDER A RUNNING BUILD METER** — see the block above `upkeep_lines` for the
## full note; this is the producer it describes.
static func build_estimate_lines(source: Dictionary, prefix: String) -> Array[String]:
    var lines: Array[String] = []
    var turns := SourceForecast.build_turns_remaining(source, prefix)
    if turns != SourceForecast.BUILD_TURNS_NO_ESTIMATE:
        var row := HudSelectionVocab.BUILD_TURNS_ROW_ONE if turns == BUILD_TURNS_SINGULAR \
            else HudSelectionVocab.BUILD_TURNS_ROW_FORMAT % turns
        lines.append("%s%s" % [MORALE_BREAKDOWN_INDENT, row])
    var gear := SourceForecast.build_work_from_gear(source, prefix)
    if gear > BUILD_GEAR_WORK_NONE:
        lines.append("%s%s" % [MORALE_BREAKDOWN_INDENT,
            HudSelectionVocab.BUILD_GEAR_WORK_ROW_FORMAT % format_work_units(gear)])
    return lines

## The one turn count that takes the singular row — a build one turn from done.
const BUILD_TURNS_SINGULAR := 1

## Below this the crew's tools took nothing off the job (no build in flight, or nothing carried that
## helps), and the gear row is not rendered at all.
const BUILD_GEAR_WORK_NONE := 0.0

## Player-facing husbandry label from domestication progress (0.0–1.0). Fully tamed shows a livestock
## glyph; in-progress shows the verb, the work the Tame has absorbed and what it costs on THIS herd
## (a Steppe Runner is several times a rabbit's job). `detail_bbcode` tints a Domesticated value via
## `husbandry_value_hex`.
static func husbandry_label(progress: float,
        work_done: float = 0.0,
        work_cost: float = SourceForecast.BUILD_WORK_COST_NONE) -> String:
    if progress >= HUSBANDRY_PROGRESS_COMPLETE:
        return "%s Domesticated" % DetailFormat.CORRAL_GLYPH
    return build_meter_value(HUSBANDRY_DOMESTICATING_LABEL, progress, work_done, work_cost)

## BBCode hex for a "Husbandry" value: signal (positive) for a domesticated herd, normal ink while
## it's still being tamed. Matched on the label produced by `husbandry_label`.
static func husbandry_value_hex(value: String) -> String:
    if value.to_lower().contains("domesticated"):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

## The "Keepers" row value: a calm *"N — drawn from the band's Husbandry"* when the pool covers this
## herd, an amber *"N — under-herded, the Husbandry pool is short here"* when it does not (and animals
## are shedding). Tinted via `herders_value_hex`.
##
## **IT STATES ONE NUMBER, and it is a DEMAND** (`docs/plan_standing_upkeep.md` §2.5) — `needed`, the
## herd's ownership-gated keeper requirement, positive from the moment it is yours. The `A / N` pair
## it replaced counted keepers ASSIGNED to this herd; maintenance left the tile, so no such count
## exists and one derived from the pool share would be a head count the sim never published.
##
## **`under_kept` IS PASSED IN RATHER THAN DERIVED FROM THIS NUMBER, and the two genuinely differ.**
## `needed` says what the herd will owe; the ALARM belongs to the keeping demand
## `upkeepWorkersNeeded`, which is `0` while the rung is still being BUILT because those hands are
## the build crew's. So a herd mid-Tame reads a calm `Keepers: 4` (the `Keeping:` row below it says
## the build's crew holds it) instead of a warning about a shed that is not happening.
## `SourceForecast.is_under_kept` is the one answer; the work board reads the same one.
static func herders_label(needed: int, under_kept: bool) -> String:
    if not under_kept:
        return HERDERS_STAFFED_FORMAT % needed
    return HERDERS_UNDER_FORMAT % needed

## BBCode hex for a "Herders" value: WARN (amber) while the herd is under-herded (shedding animals),
## normal ink when fully staffed. Matched on the label from `herders_label`, mirroring
## `corral_value_hex` / the overgrazing warning's shared WARN tint.
static func herders_value_hex(value: String) -> String:
    if value.to_lower().contains("under-herded"):
        return HudStyle.WARN_HEX
    return HudStyle.INK_HEX

## Player-facing cultivation label for a forage patch — THREE states, not two. A fully-tended patch
## shows a crop glyph; a meter below complete reads as a BUILD while somebody is building it and as a
## LOSS while nobody is. Mirrors `husbandry_label`; `detail_bbcode` tints via `cultivation_value_hex`.
##
## **"Preparing 99%" WAS THE MOST MISLEADING ROW ON THE CARD.** A meter that is bleeding back toward
## wild wore the same word, in the same neutral ink, as a fresh build one turn from done — the two
## states differ only in which DIRECTION the number is moving, which a percentage cannot show. So the
## decaying case gets its own word (the sim's own: an abandoned rung "goes feral — the ground is
## reverting") and WARN ink.
##
## `building` is the DISTINGUISHING FACT and must be the improvement axis, never the meter: a patch is
## reverting exactly when its meter is short of complete and no crew is building that rung
## (`HudBandLaborState.forage_effort_at(...).improvement`). A test on progress alone cannot tell the
## two apart at any value.
static func cultivation_label(progress: float, cultivated: bool, building: bool = false,
        work_done: float = 0.0,
        work_cost: float = SourceForecast.BUILD_WORK_COST_NONE) -> String:
    if cultivated or progress >= CULTIVATION_PROGRESS_COMPLETE:
        return "%s Tended Patch" % CULTIVATION_GLYPH
    # Lead with the VERB, exactly as the herd's Husbandry row reads "Domesticating N%" — a bare
    # percentage buried in the tile card was easy to miss and broke parity with the animal side.
    # The work absolutes ride behind it: a REVERTING meter is losing units off the same job, so the
    # two states state the same pair and only the verb and the ink say which way it is moving.
    var verb := HudFloraVocab.CULTIVATION_PREPARING_LABEL if building \
        else HudFloraVocab.RUNG_REVERTING_LABEL
    return build_meter_value(verb, progress, work_done, work_cost)

## BBCode hex for a "Cultivation" value: signal (positive) for a tended patch, WARN while the meter is
## reverting (nobody is building it and the ground is going back), normal ink while it is being built.
## Matched on the label from `cultivation_label`.
static func cultivation_value_hex(value: String) -> String:
    var normalized := value.to_lower()
    if normalized.contains("tended"):
        return HudStyle.SIGNAL_HEX
    if normalized.contains(HudFloraVocab.RUNG_REVERTING_LABEL.to_lower()):
        return HudStyle.WARN_HEX
    return HudStyle.INK_HEX

## Player-facing label for the plant RUNG-3 meter — the patch twin of `corral_label` and the rung
## above `cultivation_label`. While the crop is going in it reads as a BUILD ("Sowing 40%"), using the
## same building-verb convention as the pen's "Building 40%" / the fence's "Fencing 60%"; once
## complete it is a **Field**, badged with its own glyph so it reads as a DIFFERENT THING from a
## 🌾 Tended Patch rather than as a bigger number — which is the whole point of rung 3.
##
## THREE states here too, and the decaying one shares `cultivation_label`'s word rather than inventing
## a rung-specific one: what is happening is the same fact on both rungs — the ground is going back —
## and the ROW's name already says which rung is losing it.
static func field_label(progress: float, is_field: bool, building: bool = false,
        work_done: float = 0.0,
        work_cost: float = SourceForecast.BUILD_WORK_COST_NONE) -> String:
    if is_field or progress >= FIELD_PROGRESS_COMPLETE:
        return "%s %s" % [field_glyph(), FIELD_BADGE_LABEL]
    var verb := FIELD_SOWING_LABEL if building else HudFloraVocab.RUNG_REVERTING_LABEL
    return build_meter_value(verb, progress, work_done, work_cost)

## BBCode hex for a "Field" value: signal (positive) for a completed Field, WARN while it reverts,
## normal ink while the crop is still going in. Matched on the label from `field_label`, mirroring
## `cultivation_value_hex`.
static func field_value_hex(value: String) -> String:
    var normalized := value.to_lower()
    if normalized.contains(FIELD_BADGE_LABEL.to_lower()):
        return HudStyle.SIGNAL_HEX
    if normalized.contains(HudFloraVocab.RUNG_REVERTING_LABEL.to_lower()):
        return HudStyle.WARN_HEX
    return HudStyle.INK_HEX

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
## Derived from the entries' already-rounded PERCENTS rather than the raw shares, so a row's two
## numbers are consistent with each other (38% of 205 really is the 78 printed beside it) — and the
## remainder is folded into the FIRST (largest) entry, exactly as `flora_basket_entries` folds the
## percentage remainder, because a decomposition that visibly fails to add up is worse than a ±1 on
## the row where it is proportionally smallest. Returns zeros for a stripped or stockless surface;
## those rows print no biomass at all.
static func _flora_biomass_split(entries: Array[Dictionary], stock: float) -> Array[int]:
    var split: Array[int] = []
    if stock <= 0.0:
        split.resize(entries.size())
        split.fill(0)
        return split
    var total := 0
    for entry in entries:
        var value := int(round(
            float(entry["percent"]) / float(SourceForecast.FLORA_SHARE_PERCENT_TOTAL) * stock))
        total += value
        split.append(value)
    split[0] = split[0] + int(round(stock)) - total
    return split

## Player-facing corral label from pen-build progress (0.0–1.0) — the herd twin of
## `cultivation_label`. A finished pen shows the livestock glyph; an in-progress one reads
## "Building N%", naming the work under way. A finished pen whose keeper did NOT pay this turn's feed
## reads the STARVING state instead of the penned badge — the herd is losing biomass every turn,
## which is the one fact the player must not be able to miss. `detail_bbcode` tints via
## `corral_value_hex`.
static func corral_label(progress: float, corralled: bool, fed_fraction: float,
        work_done: float = 0.0,
        work_cost: float = SourceForecast.BUILD_WORK_COST_NONE) -> String:
    if corralled or progress >= CORRAL_PROGRESS_COMPLETE:
        if PenStatus.is_starving(fed_fraction):
            return PEN_STARVING_LABEL % int(round(fed_fraction * HudConst.PROGRESS_PERCENT_SCALE))
        return "%s Corralled" % DetailFormat.CORRAL_GLYPH
    return build_meter_value(CORRAL_BUILDING_LABEL, progress, work_done, work_cost)

## The "Pen feed" row's value: what this pen demands per turn, plus — when the keeper is short — how
## much of it was actually paid. Amber/red-tinted via `pen_feed_value_hex`.
static func pen_feed_label(upkeep: float, fed_fraction: float) -> String:
    var demand := SourceForecast.format_yield(-upkeep)
    if PenStatus.is_starving(fed_fraction):
        return PEN_FEED_STARVING_FORMAT % [
            demand, int(round(fed_fraction * HudConst.PROGRESS_PERCENT_SCALE)),
        ]
    return demand

## BBCode hex for a "Corral" value: DANGER for a starving pen (the herd is shrinking NOW), signal
## (positive) once penned and fed, normal ink while it's being built. Matched on the label from
## `corral_label`, mirroring `cultivation_value_hex`.
static func corral_value_hex(value: String) -> String:
    var normalized := value.to_lower()
    if normalized.contains("starving"):
        return HudStyle.DANGER_HEX
    if normalized.contains("corralled"):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

## BBCode hex for the "Pen feed" value: DANGER while the pen goes unfed (the herd is shrinking), WARN
## otherwise — a paid pen is still a standing debit on the larder, never good news.
static func pen_feed_value_hex(value: String) -> String:
    if value.to_lower().contains("paid"):
        return HudStyle.DANGER_HEX
    return HudStyle.WARN_HEX


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

## Net per-turn food flow: income − what the PEOPLE eat − what the band's penned ANIMALS eat − what
## PREDATORS raided off the larder this turn.
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
## Positive → the larder is growing. `pen_feed_upkeep` is
## the sim's own answer for the third term (`PopulationCohortState.penFeedUpkeep` — the food this band
## actually PAID for pen feed this turn, summed across every pen it keeps) and `raid_forfeit` is the
## fourth (`PopulationCohortState.raidForfeit`, Predators Phase 3 — food lost to raids this turn); the
## client must NOT re-derive either, and the full identity
## `larder_delta == income − consumption − pen_feed − raid_forfeit + transfers` is pinned sim-side
## (`integration_tests/tests/{pen_food_ledger,raid_food_ledger,transfer_food_ledger}.rs`) — the
## BREAKDOWN is what states it in full, this headline being the steady rate rather than the ledger.
## Raids are EPISODIC, so this net can swing the turn one lands — the forward food-outlook chart
## deliberately does NOT project raid_forfeit forward (a past loss is not a steady drain).
static func band_net_food(band: Dictionary) -> float:
    return band_food_income(band) \
        - float(band.get("food_consumption", 0.0)) \
        - band_pen_feed(band) \
        - band_raid_forfeit(band)

## The STEADY total food income = Gathered + Hunted (Σ per-source realized average across the band's
## forage + hunt assignments). Summed from the SAME per-source realized values as the breakdown rows, so
## it equals Gathered + Hunted exactly — the honest long-run average of the lumpy per-turn take, so it
## does NOT swing. It feeds the headline net (`band_net_food` = income − Eaten − Pen feed) and the
## `food_is_concerning` gate. **Deliberately summed from the rows rather than read off a band-level
## wire field** — a separately-computed total could drift from the Gathered/Hunted rows it sits above,
## and this way the headline equals them by construction. (A cohort-level `foodIncomeAverage` existed
## for one commit and was retired as redundant; do not reintroduce it.)
static func band_food_income(band: Dictionary) -> float:
    return sum_realized_yield(band, SourceForecast.LABOR_KIND_FORAGE) \
        + sum_realized_yield(band, SourceForecast.LABOR_KIND_HUNT)

## What this band paid to feed its pens this turn (food/turn). 0 for a band that keeps no corral.
static func band_pen_feed(band: Dictionary) -> float:
    return float(band.get("pen_feed_upkeep", 0.0))

## What predators raided off this band's larder this turn (food, `PopulationCohortState.raidForfeit`).
## 0 when no raid landed — the ledger then omits the row entirely, exactly like Pen feed.
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
## the same basis as `Gathered` / `Eaten` / pen upkeep / raid forfeit beside it. On the turn's own
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
        or band_pen_feed(band) >= SourceForecast.FOOD_FLOW_MIN \
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

## **HAS ANY KIT RUN OUT?** — what tints the Kit row's caret WARN, and the row's own value. It is the
## whole of what "concerning" means here: running dry is a permanent step down to bare hands, and a
## kit merely wearing is not a fact to shout about, because nothing the player can do changes its
## rate. `false` for a band that states no kit at all.
## **It sweeps whatever the server published**, rather than the three items this file happens to have
## labels for — an item the client cannot name is still an item the band can run out of, and reading
## only the known ones would hide exactly the cliff this warning exists for.
static func band_kit_is_dry(band: Dictionary) -> bool:
    if not band_states_kit(band):
        return false
    for row in band.get(KIT_ITEM_CONDITIONS_KEY, []):
        if float(row.get(KIT_ITEM_REMAINING_KEY, KIT_DRY)) <= KIT_DRY:
            return true
    return false

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

## **IS ANY ITEM SHORT OF THE PEOPLE WHO NEED IT?** — the shortfall twin of `band_kit_is_dry`, and the
## other half of what tints the Gear caret WARN. A partly-armed band is not a worn one: the gear works
## perfectly for whoever holds it, and the loss is that the rest of the party is standing there with
## nothing.
static func band_kit_is_short(band: Dictionary) -> bool:
    for row in band.get(KIT_ITEM_CONDITIONS_KEY, []):
        if int(kit_coverage(band, String(row.get(KIT_ITEM_ID_KEY, "")))["short"]) > 0:
            return true
    return false

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
    var glyph := DetailFormat.MORALE_CONTRIB_POSITIVE_GLYPH if value > 0.0 else DetailFormat.MORALE_CONTRIB_NEGATIVE_GLYPH
    return "%s%s %s  %s" % [DetailFormat.MORALE_BREAKDOWN_INDENT, glyph, SourceForecast.format_signed(value), label]


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
static func herd_summary_lines(herd_data: Dictionary, world_herds: Array) -> Array[String]:
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
        # **THE METER SAYS WORK, NOT JUST PERCENT** (`docs/plan_unit_costed_work.md` §11) — the animal
        # twin of the tile card's rows. A Tame's cost carries the SPECIES' own multiplier, so this is
        # where a Steppe Runner reads as several times a rabbit's job rather than as the same meter
        # filling more slowly.
        var domestication := float(herd_data.get("domestication", 0.0))
        var herd_prefix: String = HudComposeVocab.BARE_FORECAST_PREFIX
        if domestication > 0.0:
            lines.append("Husbandry: %s" % husbandry_label(domestication,
                SourceForecast.build_work_done(
                    herd_data, herd_prefix, SourceForecast.IMPROVEMENT_TAME),
                SourceForecast.build_work_cost(
                    herd_data, herd_prefix, SourceForecast.IMPROVEMENT_TAME)))
            # **WHICH RUNG THE ESTIMATE DESCRIBES IS READ OFF THE SOURCE, not off a crew.** The ladder
            # is strictly sequential and at most one improvement is ever in flight, so an incomplete
            # Tame IS the build these per-source fields answer for; the Corral branch below takes them
            # once taming is done. A herd nobody works reports no estimate and renders no line.
            if domestication < HUSBANDRY_PROGRESS_COMPLETE:
                lines.append_array(build_estimate_lines(herd_data, herd_prefix))
        # Staffing deficit (fauna neglect-escape arc). A managed herd needs keeping every turn to HOLD
        # its animals — underfunded, it SHEDS whole animals over its labor capacity into a nearby wild
        # herd (they drift off; tameness leaves with them, it is never decayed). The number is the
        # herd's own DEMAND in hands (`docs/plan_standing_upkeep.md` §2.5) — the keeping is a band-wide
        # pool now, so there is no per-herd crew to count, and `round(herded_fraction · needed)` would
        # reconstruct last turn's RESOLVED fraction and read a stale count the moment the pool moves.
        #
        # **THE ROW'S PRESENCE AND ITS ALARM ARE TWO DIFFERENT QUESTIONS.** It is SHOWN for any herd
        # that owes keepers at all (`herders_needed > 0`, the ownership gate — a wild herd reports 0 and
        # never trips it), and it goes AMBER only when this herd's SHARE of the band's keeping pool
        # failed to cover it, which is `SourceForecast.is_under_kept` and the same test the work
        # board's ⚠ makes. See `herders_label` for why the two cannot be collapsed.
        var herders_needed := int(herd_data.get("herders_needed", 0))
        if herders_needed > 0:
            var under_kept := SourceForecast.is_under_kept(herd_data, herd_prefix)
            lines.append("%s: %s" % [KEEPERS_ROW, herders_label(herders_needed, under_kept)])
            # State the CONSEQUENCE when the keeping is short AND the herd is owned: a muted one-liner
            # naming the shed and the single lever that stops it. It quotes the KEEPING demand, so the
            # number is what this herd claims of the band's Husbandry role.
            if under_kept and domestication > 0.0:
                lines.append(HERDERS_SHED_FORMAT % SourceForecast.keepers_wanted(
                    herd_data, herd_prefix))
        # A corralled herd is penned by the band (intensification ladder). SIGNAL-tinted, mirroring the
        # Husbandry/Ecology row treatment. While the keepers are still BUILDING the pen (0 < progress < 1
        # under the Corral policy) the same row reports the meter — the animal twin of the tile card's
        # "Cultivation N%" row, so the investment the player committed to is visibly under way.
        # A PENNED herd is a managed population: it eats from its keeper's larder every turn, and an
        # underfed one is shrinking right now. That is the loudest thing the drawer can say about it, so
        # the Corral row itself flips to the starving state (DANGER-tinted via `corral_value_hex`) and a
        # "Pen feed" row states the demand and how much of it the keeper actually paid.
        # The whole corral/pen readout is PEN-ceiling only — a pastoral herd can never be penned (the
        # server never builds one), so its Corral/pen rows are suppressed and a hint stands in their place.
        if ceiling == SourceForecast.HUSBANDRY_CEILING_PEN:
            var corral_progress := float(herd_data.get("corral_progress", 0.0))
            var fed_fraction := PenStatus.fed_fraction(herd_data)
            if bool(herd_data.get("corralled", false)):
                lines.append("Corral: %s" % corral_label(CORRAL_PROGRESS_COMPLETE, true, fed_fraction))
                # The pen is fenced LAND (Grazing 2d-γ): its footprint (radius + the SERVER's in-bounds
                # tile count, shown verbatim) and the feed SPLIT — how much of the herd's feed its own
                # grazed footprint covers vs what the keeper still hauls from the larder.
                var pen_radius := int(herd_data.get("pen_radius", 0))
                var footprint_tiles := int(herd_data.get("pen_footprint_tiles", 0))
                lines.append("%s: %s" % [PEN_FOOTPRINT_ROW, PEN_FOOTPRINT_FORMAT % [pen_radius, footprint_tiles]])
                # The larder term is the NET bread bill (`pen_larder_bill`), NOT the gross `pen_upkeep`.
                var larder_bill := float(herd_data.get("pen_larder_bill", 0.0))
                var pasture_fraction := float(herd_data.get("pen_pasture_fraction", 0.0))
                # Hay is the middle feed term, in food-equivalent units (`pen_hay_food`, NOT the
                # grass-unit `fodder_draw`), shown ONLY when the pen drew hay. pasture_food + hay +
                # larder == gross pen_upkeep (sim-pinned), so the three never double-count.
                var hay_food := float(herd_data.get("pen_hay_food", 0.0))
                var hay_segment := ""
                if hay_food >= SourceForecast.FOOD_FLOW_MIN:
                    hay_segment = PEN_FEED_SPLIT_HAY_SEGMENT % hay_food
                lines.append("%s: %s" % [PEN_FEED_SPLIT_ROW, PEN_FEED_SPLIT_FORMAT \
                    % [int(round(pasture_fraction * HudConst.PROGRESS_PERCENT_SCALE)), hay_segment, larder_bill]])
                # The standing "Pen feed" debit is the SAME food-larder bill the split's larder term
                # states (`pen_larder_bill`, net of pasture + hay), not the gross `pen_upkeep` — so a
                # pen fed for free by pasture + hay shows NO debit row, and the two never disagree.
                if larder_bill >= SourceForecast.FOOD_FLOW_MIN:
                    lines.append("%s: %s" % [PEN_FEED_ROW, pen_feed_label(larder_bill, fed_fraction)])
            elif corral_progress > 0.0:
                lines.append("Corral: %s" % corral_label(corral_progress, false, PenStatus.FULLY_FED,
                    SourceForecast.build_work_done(
                        herd_data, herd_prefix, SourceForecast.IMPROVEMENT_CORRAL),
                    SourceForecast.build_work_cost(
                        herd_data, herd_prefix, SourceForecast.IMPROVEMENT_CORRAL)))
                # Penning is a flat job for every species — a fence is a fence — so unlike the Tame
                # row above this cost carries no species multiplier, and the estimate beside it moves
                # only with the keeper crew, their floor and their kit.
                lines.append_array(build_estimate_lines(herd_data, herd_prefix))
        elif ceiling == SourceForecast.HUSBANDRY_CEILING_PASTORAL:
            lines.append(HUSBANDRY_PASTORAL_HINT)
    # **NO `Position` ROW.** These lines render in ONE place — the tile card's subject drawer — and
    # the card's own header states the hex two rows above them (`TILE (34, 24)`), so a herd stating
    # it again was the same coordinate pair twice on one card. `Next waypoint` below is a different
    # fact — where it is HEADING, which nothing else on the card says — and stays.
    # **WHAT IT COSTS TO HOLD THIS HERD AT ITS RUNG, and how long it has if nobody pays**
    # (`docs/plan_standing_upkeep.md` §2) — the animal web's half of the one keeping row both webs
    # share. A wild herd owes nothing and prints none.
    lines.append_array(upkeep_lines(herd_data, HudComposeVocab.BARE_FORECAST_PREFIX))
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
