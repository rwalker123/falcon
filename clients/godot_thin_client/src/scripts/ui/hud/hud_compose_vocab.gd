class_name HudComposeVocab

## Compose / party / send-expedition vocabulary — the tile & herd compose sheets, the parties zone,
## the hunt/forage previews, the investment-forecast strings and the cancel-scope grammar.

# Verb prefixes for the optimistic in-flight label on the disabled cancel button,
# composed with the task action phrase as "<verb> <phrase>…" (e.g. "Cancelling
# Deplete Hunt…", "Starting Foraging…"). Shown from dispatch until the snapshot
# confirms the band's `activity` CHANGED from its value at dispatch.
const CANCEL_ORDER_PENDING_VERB := "Cancelling"

const START_ORDER_PENDING_VERB := "Starting"

# ---- THE FLOOR, IN WORDS — ONE RULE, NOT FOUR ROWS ---------------------------------------------
# There were three per-stance hint tables here (forage, local hunt, expedition), each with a row per
# stance name. A floor has no names: it is a continuous fraction of carrying capacity, so the only
# thing that can be SAID about a particular value is where it sits relative to the FOOD PEAK — and
# that relation is the whole meaning of the dial. One table of five zones replaces the twelve rows.
#
#   below the peak  you are spending the source's future for calories now
#   above it        you are buying ladder progress with calories
#   at 0            you strip it — and what that COSTS differs by web, which is the one per-web clause
#   at 1.0          you take nothing, and a crew with nothing standing above its floor is watching
#                   rather than working (`labor::crew_is_working_the_source`) — no lesson, no build
#
# `%s` in the STRIP row takes the web's own consequence (`FLOOR_STRIP_CONSEQUENCE`), because "a patch
# reseeds from bare ground" and "the herd dies out, for you and for everyone else" are not the same
# warning and must never be blurred into one. Composed by `HudFormat.floor_hint`.
#
# **THE PEAK ZONE SAYS NOTHING, BECAUSE THERE IS NOTHING TO SAY.** It read "the most food this source
# can pay, turn after turn, forever", which is the DEFINITION of the preset the player just clicked,
# restated: it names no consequence, offers no comparison and asks for no decision, so a player who
# reads it knows exactly what they knew before. It is empty rather than absent so the five zones stay
# one enumeration — `HudFormat.floor_hint` answers `""` and every consumer renders no line.
#
# The other four each state something the number does not. `strip`'s is LOAD-BEARING beyond its own
# sentence: it is the only place the sheet says floor 0 is irreversible on the animal web, and the
# reaching verdict DROPS its own "then holds it" clause there on the understanding that this line
# carries the consequence.
#
# **EMPTYING AN ENTRY HERE SILENCES IT ON EVERY CONSUMER — five of them**: the compose readout's
# aside, the expedition compose sheet, the work-row hint, the send-hunt banner and the expedition
# tooltip. That is the intent for a line worth nothing anywhere, and it is a REGRESSION for a line
# worth something somewhere — which is how the peak line was blanked once before, for a reason true of
# one surface, and left a raid rendering three floor presets with nothing saying what they meant. The
# expedition sheet is where such a blanking surfaces first: it has no chart, so its readout's aside is
# the whole of what it says a floor MEANS.
const FLOOR_ZONE_HINTS := {
	"strip": "Take everything — the crew leaves nothing standing. %s",
	"drawdown": "Below the food peak — more food now, taken out of what this source will grow back. It declines while you hold this.",
	"peak": "",
	"learning": "Above the food peak — you give up food to leave more standing, and your people learn faster from what they work.",
	"untouched": "Nothing is taken — the whole stock stays standing. A crew with nothing to work learns nothing and builds nothing.",
}

# The per-WEB half of the strip-it warning, and the reason it is a clause rather than a second table:
# everything else about floor 0 is identical on both webs. A plant stand grows back from its reseed
# floor; a herd taken to nothing is EXTINCT, which is permanent and shared with every other faction.
const FLOOR_STRIP_CONSEQUENCE := {
	"forage": "The patch is stripped bare and has to reseed itself from nothing.",
	"hunt": "It is the last hunt: the herd is gone for good, for you and for everyone else.",
}

# The one thing a detached party changes about the rule above: an expedition's Hunting arm banks BOTH
# products (#337) but accrues NO HUSBANDRY — a known v1 gap, tracked server-side — so the LEARNING
# zone's promise is false for a raid and is replaced rather than appended. Every other zone reads the
# same for a party as for a resident band, which is the point of having one rule.
const FLOOR_LEARNING_HINT_EXPEDITION := "Above the food peak — the party takes less and leaves more standing. A detached party learns no craft, so the calories buy nothing but the herd's health."

# THE THREE INTENT PRESETS' LABELS — the picker's three buttons, keyed by
# `SourceForecast.FLOOR_PRESET_*`. **Naming is not settled** (`docs/plan_harvest_floor.md` §10 Q2);
# they live here precisely so a rename is one edit rather than a sweep. Each names the INTENT, not the
# number: the number is on the slider beside them and in `FLOOR_VALUE_FORMAT`.
const FLOOR_PRESET_LABELS := {
	"strip": "Take everything",
	"peak": "Best harvest",
	"learn": "Learn from it",
}

# A dialled floor stated as itself — `35% left standing`. **"Left standing", not "floor"**: the wire's
# word is a modelling term, and what the player is choosing is how much of the source survives the
# turn. One phrasing, so the slider, the picker face and the work row cannot word it three ways.
const FLOOR_VALUE_FORMAT := "%d%% left standing"

# The floor block's section header, in the same grammar as `Foragers` and `Crop to commit to` —
# `alloc_section_label` upper-cases it. **It names the chart's vertical AXIS and states no value.**
# It carried the live floor while the control below it was a plain slider, whose state was otherwise
# unreadable; the chart that replaced it puts that number on its own draggable flag, so a caption
# that repeated it read `Leave standing: 50% left standing` — the same fact twice in one line, one of
# them saying "standing" twice. The flag names the value, this names what the value is OF.
const FLOOR_CONTROL_LABEL := "Leave standing"

# ---- THE TWO CREW TARGETS (docs/plan_harvest_floor.md §7.6) -------------------------------------
# A floor and a crew are INDEPENDENT statements, so "how many workers" has two answers and the panel
# owes the player both — the rate model had only one and so never had to name either. Both are exact
# and both are clickable; neither is a hidden rule. The face is `<N> clear it now`, the count leading
# because it is what the player compares against the stepper beside it.
const CREW_TARGET_CLEAR_LABEL := "clear it now"
const CREW_TARGET_HOLD_LABEL := "hold it after"
const CREW_TARGET_CLEAR_TOOLTIP := "Enough hands to take everything standing above the floor in a single turn."
const CREW_TARGET_HOLD_TOOLTIP := "Enough hands to take exactly what grows back once it is sitting at the floor — any more go idle."

# ---- THE CREW ROW: ONE LINE, NOT A HEADING WITH A CONTROL PUSHED OFF THE OTHER EDGE -------------
# The crew is ONE statement — *this many hands, and here are the two numbers worth matching* — so the
# stepper and both targets ride a single wrapping line under a quiet row-label. It used to render as a
# body-size heading with the stepper flung to the far right by a spacer, and the two targets as
# full-width boxed buttons on a row of their own: three rows and two competing edges for one decision,
# in the half of the panel that is supposed to be quieter than the chart above it.
#
# The label gets the SAME treatment every other section label in this HUD gets
# (`HudWidgets.alloc_section_label` — small, uppercase, `INK_FAINT`), because that is what it is: a
# row-label, not a heading.
const CREW_ROW_LABEL_SEPARATION := 4
const CREW_ROW_SEPARATION := 6

# **THE BUILD DIP, STATED WHERE IT IS BEING DIVIDED BY** (`docs/plan_harvest_floor.md` §3.1). A crew
# preparing a rung carries the rung's `yield_fraction_while_building` — a quarter, on both plant rungs
# — so six foragers move 12 biomass a turn where the patch's own throughput says 48. Every
# "impossible" number on a building sheet follows from that one factor: the two targets, the take, the
# settle point, the verdict's remedy crew. Until this line the only cue was a ticked box further down
# the sheet, which states that a build is running and never states its price, and the sheet read as
# though six foragers simply could not out-take one patch.
#
# It rides the CREW ROW because the two targets beside it are the numbers it explains, in the row
# label's own faint register — the dip is context for the decision, never the decision.
#
# **IT DELIBERATELY DOES NOT SAY "while building"** — that was the middle term of the improvement
# DEAL LINE, which is deleted (its payoff moved onto the running control's face; see
# `IMPROVEMENT_RUNNING_FORMAT`). Two labels on one sheet carrying one phrase is how a search for
# either silently finds the other, measured at seven of the deal's assertions when this line was first
# worded. The wording stays because it says what this note is about — the CREW's carry, not the
# build's terms — and the harness now uses that phrase as the needle proving the deal line is gone.
const CREW_BUILD_DIP_NOTE_FORMAT := "— building this rung, each carries %d%% as much"
# The row-label and its note are one phrase, so they sit closer than the stepper and the pills do.
const CREW_ROW_NOTE_SEPARATION := 5

# A crew TARGET is a PILL, and the shape is the point: the stepper beside it is a boxed control you
# operate, a target is a value you can jump to. Its face carries two registers — the COUNT (what you
# compare against the stepper) over the label naming which of the two answers it is — so, like the
# preset rung's two-line face, it cannot live in one `Button.text`.
const CREW_TARGET_COUNT_FONT_SIZE := 13
const CREW_TARGET_LABEL_FONT_SIZE := 11
const CREW_TARGET_FACE_SEPARATION := 5

# ---- THE READOUT: ONE BOX, THREE REGISTERS (docs/plan_harvest_floor.md §7.1/§7.2) ---------------
# The take, the verdict and the asides answer three different questions, and the panel's bottom half
# read as three unrelated lines at one size until they were bounded and given three deliberately
# different registers. Loudest first, because the order is the reading order:
#
#   a. THE YIELDS ROW — the answer. A big tabular number beside a small uppercase unit and the
#      account's destination (`2.34  FOOD/TURN → CAMP`). The render-only-where-the-vector-pays rule is
#      unchanged: a cash crop shows no food line and a wolf shows no food line at all, because
#      `provisionsPerBiomass` is genuinely 0 there and a `0.00 food` reading would be false, not empty.
#   b. THE VERDICT — which of the crew and the floor is binding, with its severity dot.
#   c. THE ASIDE — the quietest register, cut off by a dashed rule: the idle-crew note and the floor's
#      own teaching line. It is the panel's least urgent information and must never be its loudest.
const READOUT_SEPARATION := 7
const READOUT_YIELD_NUMBER_FONT_SIZE := 15
const READOUT_YIELD_UNIT_FONT_SIZE := 10
const READOUT_YIELD_PART_SEPARATION := 6
# Wide enough that two account readings never read as one four-part phrase; the vertical gap is what
# a wrapped third account drops by.
const READOUT_YIELD_H_SEPARATION := 18
const READOUT_YIELD_V_SEPARATION := 4
# **AN ACCOUNT THE BAND CANNOT BANK READS AS A DASH, NOT AS A NUMBER.** A wild patch's hay is real
# ground truth — the meadow grows it — but a faction without Foddering banks none of it, so the row
# keeps its UNIT (hiding the account would be the hidden gate this repo forbids) and loses its
# QUANTITY. The em-dash is the one glyph that cannot be misread as a quantity, the same reasoning
# `HudFloraVocab.STOCK_UNKNOWN_GLYPH` records for a fogged stock; the reason rides the aside.
const YIELD_LOCKED_GLYPH := "—"

const READOUT_VERDICT_FONT_SIZE := 12
const READOUT_ASIDE_FONT_SIZE := 11
const READOUT_ASIDE_SEPARATION := 4

# Every policy button's tooltip leads with this — the policy name + its full metric ("Sustain — up to
# +0.90/turn"), since the compact button face no longer carries the name. A gated button appends its
# gate reasons below (one per line), so a hover names the rung AND explains any lock.
const POLICY_TOOLTIP_NAME_FORMAT := "%s — %s"

# The pen as a managed POPULATION (docs/plan_corral_managed_population.md). A penned herd cannot
# graze: its keeper hauls it `pen_upkeep` food/turn off the band larder. `pen_fed_fraction` is the
# share of that demand the keeper actually paid last turn — anything below fully-fed means the herd
# is SHRINKING and its yield with it, so the Corral row swaps its penned badge for a loud starving
# state and the herd's map glyph tints red. `PenStatus` owns that test (shared with MapView); the two
# starving LABELS are `DetailFormat.PEN_STARVING_LABEL` / `PEN_FEED_STARVING_FORMAT`, beside the row
# builders that are their only readers.
# The pen's feed row in the herd drawer — the NET food-larder bill THIS pen draws per turn
# (`pen_larder_bill`, after pasture + hay), and whether it is being paid. The same bill the feed-split's
# "larder Y.Y" term states, so the two never disagree. The band's own ledger row is the sim-summed
# `pen_feed_upkeep` across all its pens; this is the per-herd figure, which is why the two are never added.
# Grazing 2d-γ — the pen is fenced LAND that grazes itself. Two herd-drawer rows state it:
#   • the FOOTPRINT — "Pen: radius R · N tiles" (`pen_radius` + the SERVER's in-bounds
#     `pen_footprint_tiles` count, displayed VERBATIM — the closed-form hex-disk count is wrong at map
#     edges, so the client never recomputes it).
#   • the FEED SPLIT — "Fed by pasture NN% · hay X.X · larder Y.Y food/turn". The three render-ready
#     terms the sim partitions the pen's GROSS demand into, ALL in food units, ZERO client arithmetic:
#     `pen_pasture_fraction` × 100 (grazed free), `pen_hay_food` (hay's food-equivalent draw), and
#     `pen_larder_bill` (the NET bread bill after pasture + hay). NOTE the larder term reads
#     `pen_larder_bill`, NOT `pen_upkeep` — `pen_upkeep` is the GROSS projection (`upkeep_per_biomass ×
#     biomass`, same basis as `corral_yield`, used only for the pre-commit Corral decision, pinned by
#     `core_sim` `snapshot/mod.rs` `pen_upkeep_*` tests); the honest bill the keeper actually hauls is
#     `pen_larder_bill`. Sim-pinned invariant: `pen_upkeep × pen_pasture_fraction + pen_hay_food +
#     pen_larder_bill == pen_upkeep`. The hay segment shows ONLY when `pen_hay_food >= SourceForecast.FOOD_FLOW_MIN` (a
#     pre-Foddering / no-hay pen renders the two-term form); a self-feeding pen reads "100% · larder
#     0.0", a scrub pen "0% · larder N.N". The Pen-feed row below still carries the debit + starving detail.
# The Extend-pen affordance (Grazing 2d-γ; command `extend_pen <faction> <x> <y>` at the pen anchor).
# On a built pen with no ring in flight it offers "Extend pen"; while a ring is being worked off
# (`pen_extend_progress > 0`) it is replaced by a "Fencing N%" badge — the pen twin of the corral-build
# "Building N%" meter. The server rejects an extend at max radius / unowned / Herding-unknown with a
# feed message, so the client does not pre-gate on those (max radius is not on the wire).
const PEN_EXTEND_LABEL := "Extend pen"

const PEN_EXTEND_TOOLTIP := "Fence another ring around the pen: the keeper works it off over ~25 turns at a reduced take, then the pen grazes more land and feeds itself further. Rejected at the pen-radius maximum."

const PEN_FENCING_LABEL := "Fencing %d%%"

# WHAT COMMITTING TO AN IMPROVEMENT BUYS AND COSTS — the improvement checkbox's tooltip, one entry per
# rung, BOTH webs in one table.
#
# These four were rows of `FORAGE_POLICY_HINTS` / `LOCAL_HUNT_POLICY_HINTS` while the build verbs were
# values of `policy`. They are their own table now for the reason the whole change exists: a stance
# hint answers "how hard am I pulling?" and these answer "what am I building?", and the two questions
# stopped sharing a control. The expedition/local split does not reach here at all — a detached party
# builds nothing, so it has no improvement control to hang a hint on.
const IMPROVEMENT_HINTS := {
    "cultivate": "Cultivate — prepare this patch: a reduced take while you work it, then a much higher tended yield. It must stay staffed or it goes feral.",
    # Sow is plant RUNG 3 — the twin of Corral. Its hint must carry the two things that make it a
    # different bargain from Cultivate: it pays ~nothing while the crop is in the ground (there is no
    # standing stand to take a fraction of), and it out-yields a tended patch ~2×. The "goes feral"
    # warning is one rule for the whole plant web — an abandoned patch bleeds BOTH meters, so a
    # neglected Field reverts to WILD, not to a free tended patch.
    "sow": "Sow — plant a Field on this ground: almost no food while the crop grows, then twice a tended patch's yield. It must stay staffed or it goes feral all the way back to wild.",
    # Tame is animal RUNG 2 — the verb that replaced the hidden Sustain side effect. Its payoff is
    # NOT "free food": 3b retired the passive rung, so the honest promise is yield PER WORKER (~1.5×
    # off the same crew) plus proximity (the herd drifts to the band instead of being chased).
    "tame": "Tame — gentle this herd into livestock: a reduced take while you work it, then it keeps to your band instead of roaming, and the same hunters bring back about half again as much. Your people still work it every turn.",
    # Corral is the ladder's best yield AND its only rung with a running cost. The hint has to carry
    # all three halves of that bargain — the ~25-turn investment dip, the top payoff, and the fact
    # that a penned herd is a POPULATION YOU FEED: its food comes off your larder every turn, and an
    # underfed herd shrinks (and takes its yield down with it). It also still escapes if unstaffed.
    "corral": "Corral — pen this herd: half yield for ~25 turns while you build, then the best yield of any herd. But penned animals can't graze: you feed them from your larder every turn, and an underfed herd shrinks. It must stay staffed or the herd goes wild again.",
}

# Overhunting flag: a worked source whose actual take exceeds its renewable-sustainable ceiling by
# more than this epsilon is overdrawing (depletable herds only — forage is renewable, actual ==
# sustainable, so it never trips). Shown as a WARN-tinted ⚠ on the row + spelled out in the tooltip.
const OVERHUNT_EPSILON := 0.001

const OVERHUNT_FLAG := "⚠"

# A MANAGED hunt source's crew are HERDERS, not a hunt party (`workersNeeded` = max(herders, haulers),
# scaling with herd size). The local stepper labels them so a pen needing several keepers doesn't read
# as a hunt-party bug. See `SourceForecast.is_managed_hunt_source`.
# "Managed" starts the moment the sim asks for keepers — `herders_needed > 0`, the very field the
# drawer's "Herders: A / N" row reads — not only at a finished tame or a built pen, so a herd part-way
# through taming reads Herders on both surfaces at once. A still-WILD herd being tamed owes no keepers
# yet and keeps HUNT_CREW_LABEL.
const HUNT_CREW_LABEL := "Hunters"

const HERD_CREW_LABEL := "Herders"

# A policy button carries its per-policy metric TWICE: the COMPACT product line on the SECOND row of
# the button face (`0.96 food · 0.24 trade` — the first row is the rung's glyph + name) and the VERBOSE
# full string in the tooltip (led by the policy name, and the only one of the two that spells "up
# to …/turn"). Each `*_policy_takes` helper emits both as a `{compact, full}` pair.
# The INVESTMENT rungs (Cultivate/Sow, Tame/Corral) wear a metric too, but it is not an immediate take
# like the extractive rate — it is the PAYOFF the preparation builds TOWARD (the tended/field/pastoral/
# corral yield). A leading arrow marks it on the compact face (`→ 1.20 food`, distinct from an
# extractive rate and never a rung you'd out-earn today); the full tooltip spells it "builds toward
# X/turn".
const POLICY_PAYOFF_COMPACT := "→ %s"

const POLICY_PAYOFF_FULL_FORMAT := "builds toward %s/turn"

# The EXPEDITION picker wears the SAME "up to X/turn" cap metric as the local hunt + forage pickers
# (`POLICY_CAP_FORMAT` via `SourceForecast.extractive_take`): each policy's MAX obtainable food/turn, computed in
# `SourceForecast.expedition_policy_takes` as the max over party sizes of delivered_food / trip_turns. No bespoke
# raid-animals face any more — the three pickers read identically.
# ---- THE IMPROVEMENT CONTROL (issue #442) -----------------------------------------------------
#
# The second axis's whole vocabulary. `INVESTMENT_POLICIES` used to live here — the named set every
# surface consulted to ask "is this policy really a build?" — and it is GONE: the wire answers that
# now, with a field of its own. Nothing below is a set-membership test.
#
# THE CONTROL HAS THREE STATES and each gets one line, in this order of precedence:
#   1. OFFERED  — an unchecked checkbox naming the next rung and its terms.
#   2. RUNNING  — checked, with the build meter; a WARN pause line when the source has left Thriving.
#   3. DONE     — a static state label, with the NEXT rung's checkbox beneath it if there is one.

# What checking the box COMMITS TO, per improvement — the verb phrased against its own subject, so
# "Cultivate" reads as an act on this patch rather than as a rung name floating in a list.
const IMPROVEMENT_OFFER_LABELS := {
    "cultivate": "Cultivate this patch",
    "sow": "Sow a field here",
    "tame": "Tame this herd",
    "corral": "Pen this herd",
}

# What is HAPPENING while it builds — the present participle, so the running line reads as work under
# way rather than as an option still on offer.
const IMPROVEMENT_RUNNING_LABELS := {
    "cultivate": "Cultivating",
    "sow": "Sowing",
    "tame": "Taming",
    "corral": "Building the pen",
}

# The STATE the finished rung leaves the source in — a noun, because nothing is happening any more.
# These are the same four words the work board's rung marks use, and they carry the same glyphs
# (`DetailFormat`'s, resolved at the call site) so a Tended Patch reads identically on the compose
# sheet, in the Band panel and on the map.
const IMPROVEMENT_DONE_LABELS := {
    "cultivate": "Tended Patch",
    "sow": "Field",
    "tame": "Pastoral",
    "corral": "Penned",
}

# `<glyph> <verb phrase> · then <payoff>` — the offered checkbox's face. The terms are what makes it a
# DEAL rather than a dare, and they are quoted from the sim's own payoff for this source and crop.
const IMPROVEMENT_OFFER_FORMAT := "%s %s · then %s"

# The same face where the wire quotes no payoff at all (a species/crop that pays nothing at this rung,
# an older snapshot): the verb alone, never a fabricated "· then +0.00".
const IMPROVEMENT_OFFER_BARE_FORMAT := "%s %s"

## A GATED rung's whole line: the rung's glyph, then the unmet prerequisite in the gate's own words.
## The rung is NOT named here and that is the point — naming it ("Cultivate this patch") reads as an
## offer, and this state exists precisely because there is no offer to make yet. The glyph keeps the
## improvement axis visible and identifiable without making a promise.
const IMPROVEMENT_GATED_FORMAT := "%s %s"

# `<glyph> <participle> — 60% · then <payoff>` — the running checkbox's face. The percent comes from
# the SAME `SourceForecast.improvement_progress` the map badge and the work board read, so the three
# can never quote different meters.
#
# **THE PAYOFF RIDES THIS FACE IN THE OFFER'S OWN `· then` GRAMMAR, and that is what retired the DEAL
# LINE.** The deal read `0.61 food · 1.25 trade → 0.15 food · 0.31 trade while building → 1.39 food ·
# 0.38 trade`, and only its last term was unique to it: the middle term is byte-identical to the
# readout's own PER TURN headline (the same crew, the same dipped forecast), and the first is the
# price of building, which the crew row states qualitatively ("building this rung, each carries 25% as
# much"). So the payoff moved to the face the OFFERED state already states it on, and the two states
# now read identically — `🌱 Cultivate this patch · then 1.39 food` before, `🌱 Cultivating — 40% ·
# then 1.39 food` after. What did NOT travel with the line is its UNSTAFFED variant, which existed
# because the today/dip terms are staffing-scaled while the payoff is not; with only the payoff left
# there is no sequence a zero crew could misread.
const IMPROVEMENT_RUNNING_FORMAT := "%s %s — %d%% · then %s"

# The same face for a rung that also carries a running feed cost (Corral only): the pen's projected
# upkeep, subtracted so the payoff is never quoted gross on the control that commits to it.
const IMPROVEMENT_RUNNING_FEED_FORMAT := "%s %s — %d%% · then %s − %s feed"

# The meter ALONE, for a rung whose deal the wire does not quote (`improvement_forecast` answers `{}`
# — a species that can never be penned, an absent build fraction). Never a fabricated "· then +0.00":
# the same rule the offered box's `IMPROVEMENT_OFFER_BARE_FORMAT` follows.
const IMPROVEMENT_RUNNING_BARE_FORMAT := "%s %s — %d%%"

# `<glyph> <state noun>` — the done label. Static: there is nothing to uncheck and nothing to clear.
const IMPROVEMENT_DONE_FORMAT := "%s %s"

# **THE ONE ASYMMETRY BETWEEN THE TWO WEBS, and it is deliberate** (spec §4): a penned herd cannot
# graze, so someone feeds it every turn. That standing obligation belongs with the standing state, so
# the Corral done label carries it and the Tame one does not. Do not make these match.
const IMPROVEMENT_DONE_UPKEEP_FORMAT := "%s %s · %s fodder/turn upkeep"

# WHAT UNCHECKING A RUNNING IMPROVEMENT DOES, per web — the second half of the running control's
# tooltip (`abandon_improvement <faction> forage <x> <y>` / `… hunt <herd_id>`).
#
# **UNCHECKING IS ALWAYS LEGAL.** There is no knowledge, ceiling, site or Thriving gate on the abandon
# path, deliberately: abandoning a STALLED build is exactly when a player reaches for it, so gating it
# on the conditions that STARTED the build would make the remedy unreachable in the one case it is
# for. Nothing here may render a gate reason, and nothing may grey the box.
#
# **IT IS NOT A CANCEL-AND-REFUND, AND THE TWO WEBS GENUINELY DIFFER** — so one shared sentence would
# have to lie to one of them. The command does not touch the meter at all; it hands the source back to
# its own existing rule, which is the same state walking the band away reaches:
#   • PLANT  — `cultivation_progress` / `field_progress` BLEED at the rung's `decay_per_turn` on every
#     turn nobody is improving the patch. Slow (~100 turns to zero), but real, and the copy must not
#     imply the work is banked.
#   • ANIMAL — the meter is KEPT (`domestication` is monotone-up and the pen rung's decay is 0), so the
#     copy may say so plainly.
# Neither line promises progress BACK, because neither web gives any.
const IMPROVEMENT_ABANDON_HINTS := {
    "forage": "Unchecking stops the work — the crew keeps foraging at the stance you chose and stops paying the build dip. Nothing is refunded, and an unworked patch's progress slowly bleeds away.",
    "hunt": "Unchecking stops the work — the crew keeps hunting at the stance you chose and stops paying the build dip. Nothing is refunded, but the progress already made is kept.",
}

# Joins the rung's own hint to the abandon consequence in the running control's tooltip. The two are
# different questions ("what does this rung buy?" / "what happens if I stop?") and read as two lines.
const IMPROVEMENT_TOOLTIP_SEPARATOR := "\n\n"

# WHY THE METER IS NOT MOVING. A build accrues only while its source is Thriving, and that is
# deliberately NOT a gate on starting it (a source's phase swings as it is worked, so refusing the
# verb would be un-actionable churn) — the sim just PAUSES the meter, losing nothing. This line is the
# only thing standing between the player and a hidden rule, so it states the pause, names the cause
# (the live phase) and names the remedy, which is the opposite of "work harder".
#
# It was `TAME_STALLED_HINT_FORMAT`, animal-only, because the plant web had no control to hang it on.
# Both webs pause identically and both say so now. %s = the source's live `ecology_phase`.
const IMPROVEMENT_PAUSED_FORMAT := "⚠ Paused — the source is %s, and this only advances while Thriving. Progress is not lost: ease off and it resumes as the source recovers."

# A ZERO PAYOFF IS DATA, NOT A MISSING NUMBER — and it is the single most valuable thing the running
# control can say. The pen's harvest is constant ESCAPEMENT (take only the biomass standing above
# `K/2`), so a herd at or below the MSY point honestly pays **0.00** until it rebuilds: penning it
# would eat feed every turn and pay nothing. That must never be suppressed, blanked, or em-dashed away
# — a player who pens a depleted herd because the UI declined to show them a zero has been actively
# misled. So the zero renders in full on the control's face, and this WARN-inked note beneath it names
# the remedy (let it rebuild). The feed term still shows, because the feed is what makes a zero payoff
# a net LOSS rather than merely a nothing.
#
# **IT OUTLIVED THE DEAL LINE IT WAS WRITTEN FOR, DELIBERATELY.** It rides
# `HudWidgets.build_improvement_control`'s note slot now — the same slot the paused-build line uses,
# with the same WARN ink — because it is a warning about the rung, not a footnote to a forecast row.
# (The "is it zero" floor is the shared `SourceForecast.FOOD_FLOW_MIN` — one definition of "below
# this, there is no flow here", used by the band ledger's rows and by this note alike.)
const IMPROVEMENT_DEAL_DEPLETED_NOTE := "⚠ Too depleted to pen — it would eat feed and pay nothing until the herd rebuilds."

# How a forecast dict SPELLS its field keys — a key spelling, nothing more.
#
# Two dict shapes carry them BARE and so share one prefix: a herd dict, and the RAW wire
# forage-patch dict (decoded in native `forage_patches_to_array`, stored in `_band_labor.forage_patch_lookup()`,
# and read by the Current-actions Forage row). Only `tile_info` carries the patch's fields under a
# `patch_` prefix, because that is a cross-ref MapView stamps on in `_tile_info_at`.
#
# ⚠ A PREFIX CANNOT IDENTIFY A SOURCE KIND — that is why the bare case is ONE const and not two
# same-valued ones. It used to be two (a `HERD_*` and a `WIRE_FORAGE_PATCH_*`, both `""`), and
# having a herd-sounding name for the empty string invited `prefix == HERD_…` as an "is this a
# herd?" test; it read as discriminating and was not, so it silently routed forage patches down the
# herd branch and left the `+` button dead on every Current-actions Forage row. Pass `SOURCE_KIND_*`
# when you need the kind; a prefix only ever tells you how to spell a key.
const BARE_FORECAST_PREFIX := ""

const FORAGE_FORECAST_PREFIX := "patch_"

const SEND_EXPEDITION_HINT := "Detach a party to scout distant territory, then click a target tile."

const SEND_EXPEDITION_BUTTON := "Send scouting party…"

# Hunting expedition (PR 2, docs/plan_exploration_and_sites.md §2b): a detached party that follows a
# migratory herd, accumulates food, and drops it at the band. Launched from a resident band by
# picking a herd (herd-target click, not a tile), and Recalled like a scout expedition.
const SEND_HUNT_EXPEDITION_HINT := "Detach a party to follow a migratory herd, then click on the herd."

# Distance-aware herd-hunt affordance (docs/plan_exploration_and_sites.md §2b): clicking a herd
# offers a LOCAL hunt when it's within the SELECTED band's hunt_reach, or a hunting EXPEDITION when
# it's beyond. One compose control (worker/party stepper + policy), two labels/commands keyed off the
# wrap-aware hex distance from the selected band's own tile.
#
# **THE COMMIT BUTTON IS A VERB, and it does not restate the sheet's own header.** The sheet is already
# titled `ASSIGN HUNTERS <herd>`, so "Assign Local Hunt" spent its whole width saying what the eyebrow
# above it had just said; the forage twin has read the bare verb `Forage` all along, and the two sheets
# now match in grammar as they do in control order. "Here" is what carries the local-vs-expedition
# distinction — the only thing "Local" was contributing — against the expedition branch's `Send …`.
const ASSIGN_LOCAL_HUNT_BUTTON := "Hunt Here"

# **THE HUNT WEB'S SECOND COMMIT VERB, and its absence was a bug.** `_herd_crew_noun` has always
# resolved Hunters/Herders off the standing rung, and the header, the stepper and the drawer's open
# button all followed it — but the commit button was hard-coded, so an `ASSIGN HERDERS` sheet over a
# `Herders` stepper committed with `Hunt Here`. Reported from play. A penned or fully-tamed herd is
# not hunted; its crew keep it.
#
# The verb is derived from the crew noun the same way on both webs — Foragers→Forage, Tenders→Tend,
# Hunters→Hunt Here, Herders→Herd Here — so a noun can never acquire a verb that does not belong to
# it. `Here` carries the local-vs-expedition distinction against the expedition branch's `Send …`,
# which is why it survives on this web and appears on neither plant verb.
const ASSIGN_LOCAL_HERD_BUTTON := "Herd Here"

# Range-aware forage assign: foraging is stationary gathering (NO expedition fallback), so a tile
# beyond the selected band's `work_range` disables the button rather than offering an alternative.
const FORAGE_ASSIGN_BUTTON := "Forage"

# The plant web's SECOND commit verb, for a crew that is not gathering. A managed source — a Tended
# Patch or a Field — is never gather-drawn (the sim's `is_managed()` branch), and the ladder config
# says so in its own vocabulary: the `wild` rung's harvest primitive is `worker_take` while `tended`
# and `field` both declare `worker_tend`. So the button follows the rung the source STANDS on. See
# `PLANT_ASSIGN_BUTTONS`, which keys the pair off the ONE resolved noun so a header saying `Tenders`
# can never sit over a button saying `Forage`. (This comment once said the hunt web already worked
# that way. It did not — only its noun did, and its button was hard-coded until `HUNT_ASSIGN_BUTTONS`
# below; the animal web now keys its verb the same way, from the same kind of table.)
const TEND_ASSIGN_BUTTON := "Tend"

# `workers == 0` IS THE SIM'S UNASSIGN (server.rs: "Unassigning (workers == 0) is always allowed — a
# player must be able to abandon a source"), and the Work zone's unassign paths depend on it. So the
# submit is gated on whether it would CHANGE anything, never on the raw count: at 0 on a source this
# band already works it is a legitimate unassign and says so, and at 0 on a source it does not work it
# is a no-op and the button is dead. A client-side floor of 1 would fix the no-op and break the
# unassign.
#
# **ONE WORD, BOTH SHEETS.** The forage sheet and the LOCAL hunt sheet reach the same three states from
# the same rule, so the face is defined once: two consts holding "Unassign" is exactly how the two
# sheets drift apart. (The EXPEDITION branch is not in this family — a raid is a launch, not an
# edit of a standing assignment, so a party of 0 is simply refused.)
const UNASSIGN_BUTTON := "Unassign"

# The hunt web's twin, per CREW NOUN — a wild herd is staffed by hunters and a managed one by herders
# (`HERD_CREW_LABEL`), so the dead button's explanation names whoever the stepper above it just asked
# for. Keyed by the crew label the sheet already resolved, so the two can never disagree.
# The COMMIT VERB per hunt-web crew noun — the twin of `PLANT_ASSIGN_BUTTONS`, keyed the same way off
# the label `_herd_crew_noun` has already resolved, so the stepper's noun, the button's verb and the
# hint's singular below are three readings of ONE answer.
const HUNT_ASSIGN_BUTTONS := {
    HUNT_CREW_LABEL: ASSIGN_LOCAL_HUNT_BUTTON,
    HERD_CREW_LABEL: ASSIGN_LOCAL_HERD_BUTTON,
}

const HUNT_NOOP_HINTS := {
    HUNT_CREW_LABEL: "Nobody assigned yet — send at least one hunter.",
    HERD_CREW_LABEL: "Nobody assigned yet — send at least one herder.",
}

# ---- THE COMPOSE SHEET (docs/plan_tile_panel_layout.md §10-§15) -------------------------------
# Composing is modal by nature — open, decide, commit, done — so the two ~270px compose blocks live
# in a floating sheet (`ui/hud/ComposeSheet.gd`) rather than permanently in the drawer. The drawer
# keeps the detail rows, gains a one-line STANDING-ASSIGNMENT summary, and ends in the button below.
const FORAGE_CREW_LABEL := "Foragers"

# The plant web's MANAGED crew noun — the twin of `HERD_CREW_LABEL` on the animal side, and resolved
# by the ONE function `SourceForecast.plant_crew_label`. A wild stand is drawn down by FORAGERS; a
# Tended Patch or a Field is kept by TENDERS, the ladder's own `worker_tend` harvest primitive put
# into words. `Tenders` deliberately spans BOTH upper rungs: `Farmers` reads right on a Field and
# wrong on a Tended Patch, and two nouns to learn is better than three.
#
# **A BUILD IN FLIGHT DOES NOT MOVE THE NOUN** — a crew part-way through a Cultivate or a Sow is
# foraging the wild stand *and* clearing ground (which is exactly what the build dip charges them
# for), so the word changes only when the rung COMPLETES. This is where the plant web parts from the
# animal one: `_herd_crew_noun` reads the composed improvement axis, because a herd being penned owes
# keepers before the pen exists. A patch owes nobody anything until it is managed.
const TEND_CREW_LABEL := "Tenders"

# The COMMIT VERB and the dead-button hint per plant crew noun, in the hunt web's own idiom
# (`HUNT_NOOP_HINTS`): keyed by the label the sheet has ALREADY resolved, so the stepper's noun, the
# button's verb and the hint's singular are three readings of one answer and cannot disagree.
const PLANT_ASSIGN_BUTTONS := {
    FORAGE_CREW_LABEL: FORAGE_ASSIGN_BUTTON,
    TEND_CREW_LABEL: TEND_ASSIGN_BUTTON,
}

const PLANT_NOOP_HINTS := {
    FORAGE_CREW_LABEL: "Nobody assigned yet — send at least one forager.",
    TEND_CREW_LABEL: "Nobody assigned yet — send at least one tender.",
}

# `Assign foragers ▸` / `Assign hunters ▸` / `Assign herders ▸` — the noun is the same one the
# sheet's stepper uses, so the drawer and the sheet can never disagree about who is being staffed.
const COMPOSE_OPEN_BUTTON_FORMAT := "Assign %s ▸"

const COMPOSE_SHEET_EYEBROW_FORMAT := "Assign %s"

# The standing staffing being edited, shown INSIDE the sheet (the header carries verb + subject).
const COMPOSE_NOW_STAFFED_FORMAT := "Now %d%s"

const COMPOSE_PENDING_SUFFIX := " · pending"

# The drawer's one-line summary of what is ALREADY standing on this source: `♻ 3 foragers · +2.74
# /turn`. The rate comes from `SourceForecast.source_yield_readout` — never recomputed here.
const STANDING_SUMMARY_FORMAT := "%s %d %s"

const STANDING_SUMMARY_SEPARATOR := " ·"

## The parties inspector strip's two inline links (mirrors the work inspector's Jump/Unassign).
const PARTY_INSPECT_JUMP := "Jump to party"

const PARTY_INSPECT_RECALL := "Recall"

## PARTIES zone.
const PARTIES_HEADER_FORMAT := "%d out · %d workers"

const PARTIES_EMPTY_HINT := "No parties in the field."

const PARTY_MENU_TOOLTIP := "Bulk actions for parties in the field."

const PARTY_RECALL_GLYPH := "✕"

const PARTY_RECALL_TOOLTIP := "Recall — the party walks home"

const PARTY_RECALL_WIDTH := 24.0

## The per-row recall stays VISIBLE (parties have no other removal path) but rests dimmed, so it
## reads as available without competing with the row it sits on.
const PARTY_RECALL_REST_ALPHA := 0.45

const PARTY_RECALL_ALL_FORMAT := "Recall all parties (%d)"

const PARTY_RECALL_CONFIRM_FORMAT := "Recall all %d parties? They walk home carrying what they have."

const PARTY_RECALL_CONFIRM_OK := "Recall all"

## Single-party recall confirm — wraps each BUTTON handler (row ✕, inspector Recall, drawer Recall), NOT
## the shared emit `_on_recall_expedition_pressed` (which "Recall all" already loops under its OWN one
## confirm — confirming inside the emit would pop N prompts after a confirmed "Recall all").
const PARTY_RECALL_ONE_CONFIRM_FORMAT := "Recall the %s party? It walks home carrying what it has."

const PARTY_RECALL_ONE_CONFIRM_OK := "Recall"

## The %s a scout party fills into the recall prompt — a bare word, since "Recall the Scouting
## expedition party?" (the full mission label) reads doubled; a hunt party fills its herd name.
const PARTY_RECALL_SCOUT_LABEL := "scouting"

## The parties inspector strip is DENSER than the work inspector (up to 6 detail lines vs ~1), and the
## T/B parties zone is height-capped at ~300px, so its detail lines are tightened a touch below
## HudWorkVocab.ZONE_BLOCK_SEPARATION to keep the strip + a party row + the bottom-pinned footer inside the box.
const PARTIES_INSPECTOR_LINE_SEPARATION := 4

const SEND_PARTY_NO_IDLE_REASON := "No idle workers to spare. Free some from Work."

## The compose sheet — MISSION FIRST: the footer launches straight into a mission, so the sheet is
## always already on one and the policy picker is unreachable except under Hunt.
const COMPOSE_MISSION_SCOUT := "scout"

const COMPOSE_MISSION_HUNT := "hunt"

const COMPOSE_MISSION_LABEL_SCOUT := "⚑ Scout"

const COMPOSE_MISSION_LABEL_HUNT := "🏹 Hunt"

const COMPOSE_TITLE_SCOUT := "Setup a scouting party…"

const COMPOSE_TITLE_HUNT := "Setup a hunting party…"

const COMPOSE_FIELD_PARTY := "Party"

const COMPOSE_FIELD_POLICY := "Policy"

## The QUARRY is the hunt form's FIRST question: the herd sets the useful party size, the per-policy
## take and the trip length, so every field below it is unanswerable until it is picked.
const COMPOSE_FIELD_QUARRY := "Quarry"

const COMPOSE_QUARRY_CHOOSE := "Choose…"

const COMPOSE_QUARRY_HINT := "Choose a quarry — the rest of the form follows from it."

const COMPOSE_QUARRY_TOOLTIP_FORMAT := "%s (%d, %d)\nClick to choose a different herd."

const COMPOSE_QUARRY_LABEL_FORMAT := "%s %s"

# The picked quarry's face carries the species' bundled ART where there is any (issue #439), as the
# Button's own `icon` rather than a glyph in its text. The source PNGs are 256px, which a Button
# would otherwise reserve in full and blow the compose row's width apart, so the icon is capped
# through the stock `icon_max_width` theme constant — sized to sit with the button's label rather
# than to be read on its own, the row already naming the herd in words beside it.
const COMPOSE_QUARRY_ICON_MAX_WIDTH := 20

## The refusal when the player picks a herd the band can already work from home. The hunt_reach split
## is a rule the map does not spell out, so the refusal is where it gets taught — it names the herd,
## the distance, the reach that binds and the local alternative.
const QUARRY_WITHIN_REACH_FORMAT := "%s is %d tiles away — inside %s's hunt reach (%d). Hunt it from the herd itself instead of sending a party."

const COMPOSE_OF_IDLE_FORMAT := "of %d idle"

const COMPOSE_CANCEL_TOOLTIP := "Cancel"

const CANCEL_SCOPE_ALL := "all"

const CANCEL_SCOPE_WORK := "work"

const CANCEL_SCOPE_ROLES := "roles"

# A resident BAND and a detached EXPEDITION are told apart by the sim, and the client reads a
# DIFFERENT thing for each — never one for the other:
#   the BAND's ceiling is COMPOSED (`SourceForecast.forecast_inputs`) from `biomass`,
#       `carryingCapacity` and the herd's per-biomass yield vector: `max(0, B − floor·K) × rate`,
#       which is linear and exact, so the client lands on the number the sim would at ANY floor.
#       With the cohort's levers that makes the LOCAL hunt preview pure arithmetic.
#   `hunt_trip_estimates`  {"<floor>:<workers>" → {floor, party_workers, turns_to_fill,
#       delivers_food, delivers_trade, …}} — the sim's PRE-LAUNCH TRIP ESTIMATE, forward-simulated
#       server-side. An expedition's trip length is NOT a rate division: above the peak the ceiling is
#       a *stock*, so the party strips the headroom in a turn or two and then crawls at the herd's
#       regrowth trickle. A re-derived `carryCap / rate` closed form is wrong, and wrong by a lot — on
#       a FULL Rabbit Warren a LONE hunter fills in 23 turns while a party of 4 never fills within the
#       sim's horizon. So the client does ZERO arithmetic here — it looks the answer up.
# **THIS IS THE BOUNDARY OF THE CLIENT-COMPOSES-THE-CEILING EXCEPTION.** The ceiling is composable
# because it is linear; a raid's trip has no closed form, and a hunt's TAKE is rounded to whole
# animals (`floor(ceiling / bodyMass)`), which is not linear either. The client draws the curve; the
# sim states the take.
# (`delivers_food` WAS REDEFINED by #337 and no longer marks a denial mission: it now says the QUARRY
# IS EDIBLE, with `delivers_trade` as its sibling, so a wolf reads `delivers_food = false,
# delivers_trade = true` — pelts, no meat — and a raid at the bare floor delivers like any other. A
# DENIAL raid is one that lands nothing in EITHER currency, which is a property of the SPECIES;
# `SourceForecast.hunt_trip_forecast` owns that test.)
#
# **THE SAMPLED FLOORS ARE MARKS ON A DIAL, NOT A SET OF OPTIONS.** The sim samples the continuum
# (`snapshot::RAID_FORECAST_FLOOR_SAMPLES`) because it cannot ship a formula for a forward
# simulation; the launch command accepts ANY floor in `0.0..=1.0`, and the preview quotes the nearest
# sampled row. Treating a sample as an offered stance would undo the whole arc.
#
# The only thing the client computes for a raid is the display verdict:
#     viable = turns <= expedition_viability_warn_turns   (the band's own exported lever)
# Live per-turn yield preview for the LOCAL hunt branch. A resident hunt has no carry cap, so
# turns-to-fill is meaningless there; the number that decides a standing assignment is the food/turn
# it will produce — the sim's hunt take:
#     rate = min(workers × per_worker_yield × dip, ceiling(floor)) × output_multiplier
# The band applies its morale/discontent productivity modifier (`output_multiplier`) at payout; a
# detached expedition does not, which is why the two branches show different numbers from the same
# exported fields. (pinned sim-side by core_sim/tests/expedition_hunt.rs.)
const LOCAL_HUNT_YIELD_FORMAT := "≈ %s"

# The Sustain ceiling IS the herd's sustainable yield, so a take above it draws the herd down — flagged
# with the same ⚠ / WARN amber. This is the COMPOSE preview, which derives the flag from the steady
# forecast via `_is_overdraw` (there is no assignment yet, so no wire `overdraws` field); the CONFIRMED
# allocation rows instead read the sim-answered `overdraws` bool off the assignment.
const LOCAL_HUNT_OVERDRAW_NOTE := "overdraws the herd"
const LOCAL_HUNT_OVERDRAW_SUFFIX := " — " + LOCAL_HUNT_OVERDRAW_NOTE

# The FORAGE twin of the hunt overdraw suffix: a take above the patch's Sustain ceiling draws its
# biomass down. Forage is smooth food (no whole-animal rhythm), so the preview shows a bare rate + this.
const LOCAL_FORAGE_OVERDRAW_NOTE := "overdraws the patch"
const LOCAL_FORAGE_OVERDRAW_SUFFIX := " — " + LOCAL_FORAGE_OVERDRAW_NOTE

# The two bare notes by LABOR KIND, for the readout's yields row — which sets the clause as its own
# small-print part beside the number rather than joining it into the sentence above. Keyed exactly as
# `FLOOR_STRIP_CONSEQUENCE` is (the `SourceForecast.LABOR_KIND_*` values), so one lookup answers "what
# does overdrawing this web cost?" wherever it is asked.
const LOCAL_OVERDRAW_NOTES := {
	"forage": LOCAL_FORAGE_OVERDRAW_NOTE,
	"hunt": LOCAL_HUNT_OVERDRAW_NOTE,
}

# CARRY-AWARE ANIMALS-FIRST preview. A hunt delivers WHOLE animals via a kill-credit bank, so an
# unquantized food/turn rate credits fractional-animal throughput the crew can never carry home (the sim
# itself quantizes to whole bodies). The line instead leads with the honest carry-aware delivered rate in
# ANIMALS: `≈<rate> <animal>/turn`, rate = delivered ÷ food_per_animal (`_hunt_delivered_and_waste`).
# **THE LINE AND THE READOUT'S ROW ARE THE SAME UTTERANCE SPLIT AT THE SPACE.** The sentence form
# joins them; the readout's yields row sets the rate as a big number beside its unit as small print,
# so it needs the two halves separately. Written structurally, so the split and the joined line can
# never name the quarry two different ways. `≈` rides the NUMBER — it qualifies the rate, not the
# animal.
const HUNT_ANIMAL_RATE_FACE_FORMAT := "≈%s"
const HUNT_ANIMAL_RATE_UNIT_FORMAT := "%s/turn"
const HUNT_DELIVERED_FORMAT := HUNT_ANIMAL_RATE_FACE_FORMAT + " " + HUNT_ANIMAL_RATE_UNIT_FORMAT

# The delivered animals-per-turn rate is a long-run average of lumpy whole-animal delivery — you take
# WHOLE animals, so per-turn delivery varies. A STABLE, worker-independent disclaimer naming the
# averaging span, computed PER RUNG from that rung's own ceiling by `_hunt_avg_window_turns` (a faster
# policy averages over a different span), so it is worker-independent and never blinks out.
#
# **IT LIVES IN THE RUNG BUTTON'S TOOLTIP, not in the panel body** — appended under the tooltip's
# name + metric line by `HudWidgets.build_policy_picker` (the `note` key of the rung's take pair, which
# `_hunt_policy_takes` fills and the forage/expedition pickers leave unset). It is load-bearing — a
# player who reads "0.68 food/turn" off a mammoth and then goes six turns with nothing would reasonably
# conclude the readout lied — but it is a caveat on ONE number, and as a standing body line it cost the
# hunt sheet a full wrapped sentence that the forage sheet has no counterpart for. A tooltip is where a
# caveat on a figure belongs; the figure it qualifies is on the same control.
const HUNT_AVG_WINDOW_FORMAT := "This estimate is a long-run average over ~%d turns — you take whole animals, so per-turn delivery varies."

# The averaging window's upper clamp: near-integer animals/turn rates make the "extra animal" cycle span
# read absurdly long, so cap it at a plausible span.
const HUNT_WINDOW_MAX_TURNS := 12

# Animals-per-turn rate formatting: up to 2 decimals, trailing zeros/dot stripped (1.90→"1.9", 1.00→"1",
# 0.65→"0.65"). `String.num` already trims (unlike the padded food-rate formatter).
const HUNT_ANIMAL_RATE_DECIMALS := 2
