class_name HudSelectionVocab

## Selection-card vocabulary — roster row chrome, condition chips, the LAND/HERD row meta, the
## subject drawer geometry, the Move-band button + Band/City pointer, and the activity glyphs.

# Band-status alert types, ordered high → low priority (rendered in this order).
const BAND_ACTIVITY_IDLE := "idle"

# Occupants roster row chrome.
const ROSTER_DOT_SIZE := 9.0

const ROSTER_ROW_MIN_HEIGHT := 30.0

const ROSTER_ROW_SEPARATION := 8

const ROSTER_ROW_H_PADDING := 10.0

const ROSTER_ACCENT_WIDTH := 3.0

const ROSTER_HEADER_FONT_SIZE := 10

# The leading MARK on a land / herd row — the species or site's bundled art where the client has any
# (issue #439), the emoji where it does not. A square box, sized to sit inside `ROSTER_ROW_MIN_HEIGHT`
# (30) with the row's vertical padding intact rather than to fill it.
const ROSTER_ROW_ICON_BOX := 18.0

# The emoji FALLBACK's size. It matches the size the row's own name label renders at — the stock
# default, this client applying no `Theme` — because the glyph used to live INSIDE that label, and
# splitting it out must not resize it.
const ROSTER_ROW_ICON_FONT_SIZE := 16

# Fallback glyph for the land row on a tile carrying no food module. Text-presentation (the line-art
# policy in `FoodIcons`), so unlike an emoji it DRAWS in whatever `font_color` its label carries.
# That colour is now applied EXPLICITLY — `SelectionCardController._roster_row_ink` hands it to
# `HudWidgets.build_marker_icon` at build time and re-applies it on the in-place patch path — because
# the mark is its own bare `Label` since issue #439 and inherits nothing (this client applies no
# `Theme`). Left un-set it would render stock near-white beside an `INK_DIM` name.
const LAND_ROW_GLYPH := "◈"

# Land-row meta, shortest true form: workers on it · else the module it offers · else nothing.
const LAND_META_WORKERS_FORMAT := "%d %s"

const LAND_META_NO_FORAGE := "No forage"

# Herd-row meta: the same `<count> <activity glyph>` form the land row uses, so a hunted herd
# (`1 🏹`) and a foraged hex (`2 🌾`) state their staffing identically down the subject list.
const HERD_META_WORKERS_FORMAT := "%d %s"

# Chip strip font: one notch under the row labels — a chip is a standing condition, not a heading.
const CHIP_FONT_SIZE := 11

# Tag chips are skipped when the tile reports this literal (the `tags_text` "no tags" value): an
# absent condition earns no chip, exactly as it earns no row.
const CHIP_TAGS_NONE := "none"

# The drawer's floor. Below this a compose block is unreadable, so the card is allowed to push the
# dock into its own scroll rather than crushing the controls the player came here to use.
const SUBJECT_DRAWER_MIN_HEIGHT := 180.0

const SUBJECT_DRAWER_BOTTOM_MARGIN := 12.0

# The list ↔ drawer rule: one hairline, the same weight `header_stylebox` draws under a card title.
const SUBJECT_DIVIDER_HEIGHT := 1.0

# A selected PLAYER band's detail lives in the dockable Band/City panel, so its drawer here would
# otherwise be a blank gap. Say where it went instead.
const BAND_PANEL_POINTER_TEXT := "Labor allocation is in the Band / City panel."

# …but REPOSITIONING is a map action, and the player is already on the map with this hex open, so
# Move stays in the drawer beside the pointer (§18). Same words as the Band/City panel's own Orders
# Move — one order, one name.
const MOVE_BAND_BUTTON_TEXT := "Move"

const MOVE_BAND_BUTTON_TOOLTIP := "Relocate the band, then click a destination tile."

# Per-activity glyph for a player band's roster row. `activity` is the kind with the
# most workers (Early-Game Labor): idle | forage | hunt | scout | warrior.
const ACTIVITY_GLYPHS := {
    "idle": "·",
    "forage": "🌾",
    "hunt": "🏹",
    "scout": "🧭",
    "warrior": "🛡",
}

# ---- THE BUILD READOUT — a rung's price in WORK, and the turns that price buys -------------------
# `docs/plan_unit_costed_work.md` §11. An improvement declares a fixed size in WORK UNITS, a crew
# produces work units per turn, and **TURNS ARE THE OUTPUT** — so the same percentage fills at
# different speeds on different rungs, and fills faster as you add hands, hold a higher escapement
# floor or carry better tools. A bare `Cultivation: 42%` was a complete statement while every rung
# cost the same 25 turns; it is not one now, and none of that is visible without these lines.
#
# The copy lives HERE, with the selection card and its subject drawer, because that is where both
# build meters render — the tile card's plant rungs and the herd drawer's animal ones. The compose
# sheet's pre-commit quote is `HudComposeVocab`'s, composed through the same shared formatters.

# `Preparing 18 / 50 work (42%)` — the verb the row already led with, then the absolutes, then the
# percentage the meter has always shown. **The percentage is NOT re-derived from the absolutes**: the
# wire ships the fraction and the pair separately, and dividing here would be a second opinion about
# one meter.
const BUILD_METER_WORK_FORMAT := "%s %s / %s work (%d%%)"

# The row as it always was, for a source the wire prices no such job on. Not a missing-field path: a
# source can carry a meter for a rung it states no cost for, and `18 / 0 work` reads as a defect
# where a bare percentage reads as an unpriced job.
const BUILD_METER_PERCENT_FORMAT := "%s %d%%"

# Work units read WHOLE where they are whole (`50`) and to one place where they are not (`17.6`).
# The shipped costs are integers and a `50.0` beside them claims a precision the config lacks — the
# rule `_format_danger_scalar` already follows for the same reason.
const BUILD_WORK_DECIMALS := 1

# ---- ONE ROW PER LIVE METER, AND THE TURNS LEAD IT -----------------------------------------------
# The card used to spend FOUR lines on one rung — the meter in work units, an indented turn estimate,
# a `Keepers` head count and a `Keeping` sentence — and reported from play, the last two were
# unreadable: both existed to say *there is nothing to do here* and both said the same number twice.
# What a glance needs off a rung is **how long, or how much is at stake**, so the row is now one line:
#
#     Husbandry   ≈308 turns (0%)      building — the turns LEAD, the meter is context
#     Husbandry   🐄 Domesticated 100% built
#     Cultivation 🌾 Tended 92% ⚠      built and its keeping is short
#
# **THE WORK ABSOLUTES CAME OFF THE CARD.** `0.3 / 100 work` is what you read while COMPOSING a build,
# beside the stepper that moves it, so it stays on the compose sheet (`BUILD_METER_WORK_FORMAT`, still
# the sheet's) and leaves the two surfaces a player scans every turn.
#
# **THE PERCENTAGE STAYS ON A BUILT RUNG, and that is not decoration**: a completed meter sits exactly
# at its own cost, so `92%` is a rung that has already begun eroding — precisely what a glance should
# catch, and the only number on the card that shows it.

# The building row: the sim's own estimate, then the meter in parentheses. `≈` because it IS an
# estimate — it moves with the crew, the floor and the kit — which is the one hedge the never-finishes
# and built forms below deliberately do not wear.
const RUNG_TURNS_FORMAT := "≈%d turns (%d%%)"

# …and its singular, so a build one turn out does not read `≈1 turns (99%)`.
const RUNG_TURNS_ONE_FORMAT := "≈1 turn (%d%%)"

# **THE SAME ESTIMATE AS A DATE — the turn this queue entry is expected to complete on**
# (`docs/plan_standing_upkeep.md` §4.7). The BUILD QUEUE block's alone: its counts are CHAINED down
# the list, so `≈42` / `≈61` / `≈98` read as three independent spans when they are cumulative.
#
# **It wears no `≈` and needs no singular.** The hedge belongs to the SPAN — a count that moves with
# the crew — and a stated turn number is the estimate already committed to a date; `turn 41 (0%)` is
# correct at one turn out, which is exactly the case `RUNG_TURNS_ONE_FORMAT` exists for on a span.
const RUNG_COMPLETES_FORMAT := "turn %d (%d%%)"

# **…AND THE SAME DATE UNDER A CLIMB WHOSE PERCENTAGE BELONGS TO ANOTHER RUNG** — `Cultivating 18% ·
# turn 83`, on a queue row titled `Sow` (`docs/plan_standing_upkeep.md` §2.8).
#
# **THE VERB LEADS BECAUSE THE PERCENTAGE IS THE THING BEING QUALIFIED.** A queue entry names a
# DESTINATION and climbs every rung on the way, so the leg in flight is routinely a rung the row's own
# title does not name; `18%` alone under a `Sow` reads as the Field being 18% sown, which is a
# different and false claim from the one it replaces. The participle is
# `HudComposeVocab.IMPROVEMENT_RUNNING_LABELS`' — the same word the compose sheet's running face uses,
# so one job is not called two things on one screen.
#
# **The `·` separates two statements about ONE job**, the row tooltip's own separator: how far into
# the leg, and when the whole climb lands. It is deliberately not a parenthesis — the bracketed form
# above reads as a gloss on the date, which is exactly the attribution this face exists to break.
const RUNG_COMPLETES_LEG_FORMAT := "%s %d%% · turn %d"

# **A BUILT RUNG: its badge, then how full its meter still is.** `100%` is the healthy reading and
# anything less is erosion already under way.
const RUNG_BUILT_FORMAT := "%s %d%%"

# ---- THE FOUR HAZARDS, AND WHY EVERY ONE OF THEM CARRIES A MARK ----------------------------------
# With the `Keeping` row gone, **the ABSENCE of a hazard is the only thing that says this rung is
# fine** — so a failure state that renders bare reads as success, which is exactly the defect the
# unstaffed build was (a declared Cultivate with nobody on it rendered a calm `0%`). Every state below
# therefore leads with `RUNG_HAZARD_GLYPH`, and the tint registry keys the amber off that ONE mark
# rather than off four independent word guesses.

# The mark itself. Shared with `DetailFormat.BUILD_UNSTARTED_VALUE`, `PEN_STARVING_LABEL` and the
# overgrazing sentence, all of which already led with it.
const RUNG_HAZARD_GLYPH := "⚠"

# **A BUILT RUNG WHOSE KEEPING IS SHORT: the badge, the mark, and WHAT IS HAPPENING TO IT.**
#
# **THE MARK USED TO STAND ALONE**, because the three lines beneath it carried the meaning — an
# `At risk:` row with a shortfall and a countdown, and an indented remedy. All three are retired
# (`DetailFormat`'s own note), so a bare `⚠` would be a warning with nothing in it: the state has to
# be ON the row. It is a STATE WORD and not a sentence — the sentence is the row's hover — so the row
# stays one line on a card that clips.
const RUNG_UNDER_KEPT_FORMAT := "%s %s %s"

# The two webs' words, which are their own consequences rather than one shared adjective: ground goes
# back to wild, animals walk away. Both are the verb out of the work board's own note
# (`HudWorkVocab.WORK_ROW_UNDER_KEPT_NOTE` / `WORK_ROW_UNDER_HERDED_NOTE`), so the card's word and the
# board's sentence name one failure. Lower-case: they land mid-value, after the percentage.
const RUNG_UNDER_KEPT_PLANT_WORD := "slipping"

const RUNG_UNDER_KEPT_ANIMAL_WORD := "drifting"

# **HAZARD: staffed EXACTLY AT the rung's rate** (`SourceForecast.BUILD_TURNS_HOLDS`, the wire's own
# `-2`). The `∞` is `DetailFormat.BUILD_TURNS_NEVER_GLYPH`, shared with the larder runway and meaning
# the opposite there, which is why the ink is what separates them. It wears no `≈`: a meter that does
# not advance has no distribution to hedge.
const RUNG_HOLDING_FORMAT := "%s %s turns (%d%%)"

# **HAZARD: staffed UNDER the rung's rate** (`SourceForecast.BUILD_TURNS_ROTS`, the wire's own `-3`) —
# the crew does not cover the maintenance the meter is billed for, so past the rung's grace the decay
# takes back work the player has already paid for.
#
# **IT WEARS THE SAME `∞` AS THE ROW ABOVE, AND SAYS ONE MORE THING.** Both never finish, so both
# earn the glyph; what a player has to be able to see is that this one is going BACKWARDS while the
# other merely stands still, and *holds* against *loses ground* is the plainest way to say it. The
# INK is the other half — `HudStyle.DANGER` here against the holding row's amber
# (`DetailFormat.rung_value_hex`) — and neither half stands alone: colour without words is a
# distinction nobody can name, words in the same amber read as the same severity.
const RUNG_ROTTING_FORMAT := "%s %s turns, %s (%d%%)"

# The words that tell the two ∞ rows apart, and the NEEDLE `rung_value_hex` keys the red off — passed
# INTO the format above rather than spelled inside it, so the phrase the row prints and the phrase the
# tint tests are one string by construction. (The BUILT badges are the same pattern one row down.)
#
# Lower-case because it lands mid-value, after the count.
const RUNG_ROTTING_PHRASE := "losing ground"

# **NOT A HAZARD: work banked, nobody on it, and the band's keeping is covering it** — the wire's own
# `-2` with no build crew (`SourceForecast.BUILD_PACE_HELD`). Parking a half-built improvement is a
# legitimate thing to do (`docs/plan_standing_upkeep.md` §2.4): the meter stays exactly where the
# player left it, indefinitely.
#
# **IT CARRIES NO `RUNG_HAZARD_GLYPH` AND TAKES THE NEUTRAL INK, and that is the whole point.** With
# the `Keeping:` row retired, the ABSENCE of a mark is the only thing that says a rung is fine — so
# marking a deliberate hold teaches the player to ignore the mark, which costs every other row in this
# family its meaning.
#
# **IT SAYS `Held` RATHER THAN `∞ turns`.** The `∞` states are statements about a CREW, and there is no
# crew here; a row that quoted a crew's never-finishing to describe a parking decision would be
# answering a question nobody asked.
const RUNG_HELD_FORMAT := "Held at %d%%"

# **HAZARD: a rung that is NOT the one in flight, carrying banked work its keeping did not cover.**
# The half of the old *work banked and nobody on it* row that survived, and it survived because the
# sim's answer cannot reach it.
#
# **THE SOURCE PUBLISHES ONE COUNTDOWN AND THE CARD HAS TWO ROWS** (`docs/plan_standing_upkeep.md`
# §4.6a). `buildTurnsRemaining` describes whichever rung `build_verb` names, so the OTHER row has no
# sentinel of its own — `-2` / `-3` replaced this format **for the at-risk meter only**, and for one
# pass nothing replaced it here, which put the Field's `≈30 turns` on a Cultivation meter nobody was
# touching. So a row that is not the rung in flight states what it IS: `RUNG_HELD_FORMAT` where the
# keeping covers it, and this where it does not.
#
# **THE FORK IS `DetailFormat.rung_is_at_risk`** — a published shortfall in EITHER currency (hands or
# goods), routed through the at-risk rung — so this row derives no number of its own and cannot
# disagree with the mark on the built row beside it, which uses the same seam.
const RUNG_REVERTING_FORMAT := "%s Reverting %d%%"

# **HAZARD: builders are on it and the meter is not moving anyway** — the sim's `-1` for a rung whose
# knowledge, site or species gate does not hold, or whose crew is standing over an empty escapement
# room. It is NOT one of the three states a crew size explains, so it gets its own word rather than
# borrowing *Reverting* (which would name the wrong remedy) or rendering as a bare percentage (which
# is the silence this whole family exists to remove).
const RUNG_STALLED_FORMAT := "%s Stalled %d%%"

# **NOT A HAZARD: THE SIM HAS NOT LOOKED AT THIS ENTRY YET**
# (`SourceForecast.BUILD_TURNS_NOT_YET_ESTIMATED`, the wire's own `-5`,
# `docs/plan_standing_upkeep.md` §4.9). The player queued the build since the last turn resolved, so
# no estimate pass has run over it: there is no number because nothing has been asked.
#
# ⛔ **IT TAKES NO HAZARD GLYPH AND NO HAZARD INK, and that is the whole of the fix it belongs to.**
# It was folded onto `-1` and wore `RUNG_STALLED_FORMAT`, which put `⚠ Stalled 0%` on a fresh
# `Cultivate (4, 19)` with two builders standing on it — a warning about a build one command old,
# which the next turn then cleared by itself. A mark that appears when nothing is wrong and vanishes
# on its own is how a player is taught to ignore every other mark (`selection-card.md` → "THE ABSENCE
# OF A HAZARD IS THE ONLY SIGNAL THAT THINGS ARE FINE"). So it is one neutral word, and the one
# `*_value_hex` rule falls it through to `INK_HEX` **because it carries neither needle** — which is
# the tint rule working as designed rather than by luck.
#
# **NOR IS IT THE SILENCE `-1` EARNS ELSEWHERE.** A queued entry has a ROW, and a row with an empty
# date column reads as a job the queue has forgotten. *Queued* is a true, complete statement of where
# the entry stands, and the meter beside it is the entry's own — `0%` on the turn it is declared,
# which is honest.
const RUNG_QUEUED_FORMAT := "Queued %d%%"

# **AND IT COVERS A CREWLESS `-1` TOO — there is deliberately no *no estimate* twin.** A crew fork was
# built here for the 99% repair, on the reading that *Stalled* blames builders who do not exist, and
# it was REVERTED: `RungDef::build_accrual`'s `eligible` takes no crew count, so the sim publishes
# `-1` for a refused gate at any staffing, and `chapters/improvements.gd`'s `tile_meter_stalled` pins
# that the card must say so. **The eroded rung the repair is about never reaches this format anyway**
# — `DetailFormat.rung_row_value` answers on the `built` branch first, so a 90% Tended patch reads
# `🌾 Tended 90%` and its slipping, not a countdown.

# **HAZARD: THE QUEUE IS STUCK HERE** (`SourceForecast.BUILD_TURNS_QUEUE_BLOCKED`, the wire's own
# `-4`, `docs/plan_standing_upkeep.md` §4.6b). The band's builders are staffed and standing on this
# entry, its own gate refuses it, so nothing banks — and, the whole pool being on the head of the
# queue, nothing behind it moves either.
#
# **IT IS NOT `RUNG_STALLED_FORMAT`, AND THE DIFFERENCE IS WHAT THE PLAYER DOES NEXT.** *Stalled* is
# a rung nobody is being held up by; this one is holding the band's ENTIRE build programme. It states
# no cause — `DetailFormat.build_blocked_lines` puts that on the line beneath, which is where the
# answer actually is.
#
# **IT IS THE WORD AND THE METER, AND NOTHING ELSE.** It read `— your builders are stuck here` for a
# release, and the tail is what *Blocked* already means: a headline that spells its own word out is
# the first line of a three-line block the player has to read before reaching the one sentence that
# tells them anything. Length is a correctness property on a ~245px card (see `BUILD_BLOCKED_REASONS`
# below), and the cheapest words to cut are the ones that repeat the word beside them.
const RUNG_BLOCKED_FORMAT := "%s Blocked %d%%"

# RETIRED — `RUNG_BLOCKED_REMEDY_FORMAT`, the KEEPING line: *"You are short of hands to look after it
# — put someone on this band's %s."* It rode the blocked block as a THIRD sentence, on the one pairing
# it was a lever for (an `escapement` refusal on a source whose keeping is also short), and Ray cut it
# with the cause's own remedy sentence: *"You don't need 'your builders are stuck here', that's what
# Blocked means. You also don't need the 2nd and 3rd sentence."*
#
# **IT IS NOT MISSING AND MUST NOT BE PUT BACK AS A "REMEDY THE BLOCK FORGOT".** The block is two
# things now — the headline and one short cause — and everything this line said is still on the card
# one row down, where the `At risk:` row states the shortfall, its countdown and the role that pays it.
# A third sentence between a hazard row and that countdown is the paragraph the trim exists to end.
#
# **AND THE `At risk:` ROW HAD TO STOP YIELDING TO IT.** `DetailFormat.build_blocked_states_keeping`
# suppressed that row's own under-kept note on this exact pairing, so cutting this line without
# deleting that predicate left the pairing with a shortfall, a countdown and nobody to put on it. Both
# are retired together; the autopsy is where the predicate was.

# **THE CAUSE ITSELF, ONE SENTENCE PER KEY** (`ForagePatchState.buildBlockedReason` /
# `HerdTelemetryState.buildBlockedReason`, `docs/plan_standing_upkeep.md` §4.6b). The sim decides
# which conjunct of the rung's gate refused and ships a short lowercase key; the WORDING is the
# client's, on `HudFloraVocab.SOW_REFUSAL_REASONS`' precedent — including its fallback, because a key
# this client has not learned still refuses and must say the one thing we do know.
#
# **THIS IS THE ROW THAT DID NOT EXIST.** The remedy above rendered only where the keeping was short,
# so a player who covered the keeping watched `⚠ Blocked 32%` sit there with NO cause at all — the
# real refusal being the herd standing below its escapement floor, which no surface named. Every key
# answers now, and the one the playtest hit is the first of them.
#
# > ### ⛔ WRITE THESE IN THE WORDS THE UI ALREADY SHOWS THE PLAYER
# >
# > The first draft shipped to a frame and was rejected on sight: *"That is non-sense, try to write it
# > in english that someone could understand."* It said **floor**, **gate**, **rung**, **source**,
# > **entry**, **the sim** and **this client** — every one of them a term from this codebase that has
# > never appeared on screen. The player's word for the floor is the compose sheet's slider label,
# > **LEAVE STANDING** (`leave 50% · ≈43 Wild Sheep`); their word for a rung is whatever the verb on
# > the button says. **A sentence a player cannot parse is worse than the silence it replaced**, since
# > it costs them the time to try.
# >
# > **AND IT HAS TO FIT.** The card is ~245px wide, so a sentence over ~120 characters wraps past
# > three lines and stops being read at all. Length is a correctness property here, not a preference.
#
# **EACH SENTENCE NAMES SOMETHING THE PLAYER CAN DO, or says plainly that there is nothing**
# (`species_ceiling`), which is the gate-reason rule this client already follows: naming a
# prerequisite without naming the lever tells a player the door is locked and not where the key is.
#
# > ### ⛔ ONE SENTENCE PER KEY MEANS ONE, AND FOUR OF THEM LOST A SECOND
# >
# > The block draws TWO lines — the headline and this — and that is the whole of it. Ray cut the rest on
# > sight: *"You don't need 'your builders are stuck here', that's what Blocked means. You also don't
# > need the 2nd and 3rd sentence."* So an entry that STATED its cause and then told the player what to
# > do about it was two sentences where the card affords one, and the second went:
# >
# > - `site` — *"You will have to build somewhere else."*
# > - `undeclared` — *"Remove it from the build queue and order the next step instead."*
# > - `unworked` — *"Send a crew back, or remove the job from the build queue."*
# > - `BUILD_BLOCKED_FALLBACK` — *"Your builders will wait here until it changes."*
# >
# > (The escapement pair below lost its own, and the keeping line went with them.)
# >
# > **THE ENTRIES THAT SURVIVE WHOLE FUSE THEIR LEVER INTO THE CAUSE with an em-dash** — *"…— the Know
# > tab shows how far along they are"*, *"…— pick one on the job's row in the build queue"* — which is
# > one sentence and is why they were not touched. **That is the shape to write a new key in**: state
# > the refusal, and where a lever fits in the same breath, name it in the same sentence. A key whose
# > remedy will not fit that way states the refusal alone; the block is a READOUT, and the surfaces
# > that carry instructions are the build queue's own row and the `At risk:` row beneath this one.
# > Length is a correctness property here — measure a rewrite in `ui_preview_out/tile_meter_blocked.png`.
const BUILD_BLOCKED_REASONS := {
    "knowledge": "Your people have not learned how to do this yet — the Know tab shows how far along they are.",
    "no_crop": "No crop has been chosen for this patch — pick one on the job's row in the build queue.",
    "species_ceiling": "This kind of animal will not go any further, however much your people learn.",
    "rung_below": "It has to be tamed before it can be penned.",
    "owned_by_other": "Another band already holds this — yours cannot build here.",
    "site": "This ground will never support it.",
    "ring_idle": "Nobody is extending this pen.",
    "undeclared": "This job is out of date — the land has moved on since you ordered it.",
    "unworked": "This band has nobody working here any more.",
}

# **THE ONE CAUSE THAT IS WORDED PER WEB** — the state the playtest actually hit, and the only one
# whose sentence would be a lie in the other web's nouns: *animals* and *hunters* on one side,
# *growing* and *gatherers* on the other, against ONE slider both webs label **Leave standing**. The
# fork is `kind`, the same parameter `HudWorkVocab.keeping_role_name` picks Agriculture or Husbandry
# with one line down in `DetailFormat.build_blocked_lines` — one test on one argument, not a second
# way of asking which web this is.
#
# **NAME THE SLIDER, NOT THE MODEL.** "Escapement floor" is this repo's term; the player has only ever
# seen `Leave standing`, so that is the phrase these quote — in the UI's own capitalisation and in
# quotes, so it reads as a control rather than as prose.
#
# **ONE SENTENCE, AND IT HAS BEEN CUT TWICE.** The first wording spelled the consequence out — *"so
# nobody can work this herd — and taming only moves while they do"* — and rendered SIX lines in the
# ~245px card. Cutting that left three (headline, cause, remedy), which is still a paragraph standing
# between a hazard row and an `At risk:` countdown, so Ray cut the remedy too: RETIRED — *"Lower
# \"Leave standing\", or wait for the herd to grow back."* / *"…or wait for it to grow back."*
#
# **THE SLIDER IS STILL NAMED, WHICH IS WHY THESE READ AS INSTRUCTIONS WITHOUT CARRYING ONE.** *"than
# you leave standing"* quotes the control the player would move; a second sentence telling them to move
# it says the same thing twice. **ONE line is the budget** — measure a rewrite in
# `ui_preview_out/tile_meter_blocked.png` rather than in characters.
# **THE ONE CAUSE THAT NAMES A GOOD** (`docs/plan_standing_upkeep.md` §2.7 / §4.9 item 12), and the
# second cause worded off the table above rather than in it — because the table is keyed by wire key
# and the wire's key is just `materials`: WHICH good is read from the rung's own pile
# (`SourceForecast.build_material_cost`), so the sentence names the thing the store ran out of.
#
# ⛔ **THE REMEDY IS NOT THE BUILDERS ROLE, AND SAYING SO IS THE POINT.** A build the store cannot
# cover is not short of hands — there is no affordability gate on a rung (§2.5 retired the five verbs'
# own), so it QUEUES AND STALLS: the arm runs, banks nothing and wastes the crew's turn. Adding
# builders changes none of that. The lever is the bench that makes the good, or a trade, and both are
# off the build line entirely — the same shape `escapement`'s remedy has.
#
# **IT FUSES ITS LEVER INTO THE CAUSE with an em-dash**, which is this table's own rule for a key that
# survives whole, and it measures 58 characters against the card's ~120 budget.
const BUILD_BLOCKED_REASON_MATERIALS := "materials"

# **THE SHARED LEAD-IN THAT MEANS *A GOOD IS MISSING*, and it is load-bearing beyond its words.**
# `DetailFormat.detail_bbcode` tints an indented sub-line DANGER on exactly this prefix, which is what
# gives a missing MATERIAL the red the work row's own good-shortfall note takes
# (`HudWorkVocab.WORK_ROW_MATERIAL_SHORT_FORMAT`, which is built from it). **Missing hands are amber
# and a missing good is red** — one rule, two surfaces, one string.
#
# ⛔ **IT MAY NOT CARRY BBCODE.** These sentences go verbatim into the build queue row's plain-text
# `tooltip_text` as well as into the card's BBCode, so a `[color=…]` run here would print its own
# markup on the hover. The ink is the RENDERER's, keyed on this named prefix.
const BUILD_BLOCKED_MATERIAL_SHORT_LEAD := "Short of "

const BUILD_BLOCKED_MATERIALS_FORMAT := BUILD_BLOCKED_MATERIAL_SHORT_LEAD \
    + "%s — the bench or a trade, not more builders."

# …and the same sentence where the wire quotes no pile at all. **An empty `buildMaterialCost` means
# "this rung eats nothing"**, which cannot be true of a build blocked ON materials — so this is the
# honest reading of a wire this client is behind on, and it names no good rather than inventing one.
const BUILD_BLOCKED_MATERIALS_UNNAMED := BUILD_BLOCKED_MATERIAL_SHORT_LEAD \
    + "what this needs — the bench or a trade, not more builders."

const BUILD_BLOCKED_ESCAPEMENT_HERD := "Fewer animals here than you leave standing."

const BUILD_BLOCKED_ESCAPEMENT_PLANT := "Less growing here than you leave standing."

# The key the ESCAPEMENT refusal ships under — named because it is the ONE cause worded per web (the
# pair above), and `DetailFormat` tests it by name so that rule is written once. It was TWO rules until
# the keeping line retired: this was also the one refusal that could be short of keeping as well, which
# earned a second sentence and a suppression on the `At risk:` row. Both are gone, and this key is now
# tested for the per-web wording alone.
const BUILD_BLOCKED_REASON_ESCAPEMENT := "escapement"

# An unrecognized cause key — or a `-4` shipped with no key at all — still leaves the builders stuck,
# so this says exactly that and guesses at nothing. It is the honest reading of a wire this client is
# behind on, and it is never an empty line: silence on a marked row reads as *no cause exists*, which
# is the state this whole block exists to end.
# RETIRED with the block's second sentence — *"Your builders will wait here until it changes."*, which
# restated the headline's own word.
const BUILD_BLOCKED_FALLBACK := "Something is stopping this build that this version cannot explain."

# `your gear: +1.0 work a turn` — what the crew's tools ADD to what it banks, in the units the meter
# is quoted in. It is the only way a player can tell a tool is worth carrying at all: a build's pace
# is its crew plus this, and a kit left at camp is the difference between the two.
#
# ⛔ **IT READ `−8.5 work off this job` AND THAT IS BACKWARDS NOW** (`docs/plan_standing_upkeep.md`
# §4.8). `buildWorkFromGear` was *"what the tools took OFF the cost"* — a lump against the pile,
# granted once — and it is *"what the pool's kits ADD per turn"*, an addend on the supply. The sign
# had to turn with it: a `−` on a contribution reads as the tool costing the player work. **A job's
# work requirement never changes**, so nothing comes off the job to be quoted here.
#
# Rendered only above zero — a `+0` advertises a tool that did nothing. The plus is a plain ASCII
# `+`, matching every other per-turn rate this HUD states.
const BUILD_GEAR_WORK_ROW_FORMAT := "your gear: +%s work a turn"
