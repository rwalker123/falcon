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
# sentence: it is the only place the sheet says floor 0 is irreversible on the animal web. The
# reaching verdict states a bare countdown and nothing about an aftermath (`VERDICT_REACHES_FORMAT`),
# so this line is the whole of what says what arriving there costs.
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
# **ONE WORD EACH, BECAUSE THREE PRESETS MUST FIT ONE ROW IN A 354px DOCK COLUMN.** The long forms
# below wrapped the picker onto two rows there — `💀 Take everything` and `♻ Best harvest` on the
# first, `↑ Learn from it` orphaned on a second — which is what forced the zone's own 2-column
# constant. Nothing is lost: the long form leads every preset's TOOLTIP (`FLOOR_PRESET_LONG_LABELS`
# below), each face keeps its zone glyph, and the sentence that actually explains the choice is the
# floor hint under the picker, which never shortened.
const FLOOR_PRESET_LABELS := {
	"strip": "Everything",
	"peak": "Best",
	"learn": "Learning",
}

# …and the phrase each is short FOR, which leads the preset's tooltip beside the number it stands
# for. It is a separate table rather than a suffix so the two can be worded independently: a face is
# a name and a tooltip is a sentence's opening, and folding them cost the long form the moment the
# face shortened.
const FLOOR_PRESET_LONG_LABELS := {
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

# **A TARGET NO CREW REACHES STILL SHOWS — DISABLED, WITH A ✕ WHERE THE COUNT GOES.** It used to
# vanish, which reads as the sheet having nothing to say about clearing at all; the pill is the one
# place that question is answered, so it stays and answers it in the negative.
#
# **✕ AND DELIBERATELY NOT ∞.** An infinity is a QUANTITY — it says "an infinite crew would do it" and
# invites the player to keep adding hunters. They do not help: the take curve plateaus, and a quarry
# that scatters can never be cleared in one turn by anybody. `✕` says *this cannot be done*, which is
# the true statement and the only one the model supports.
const CREW_TARGET_UNREACHABLE_FACE := "✕"

# …and the REASON, on the hover, because the sheet stays quiet while the why stays reachable. One
# sentence each, because the two targets fail differently: a clear that no crew reaches is often
# permanent rather than a matter of pool size — a wary quarry breaks off and retreats, so the last of
# it is never standing there to be taken — while a hold that no crew reaches is the take flattening
# out below what the source puts back.
#
# **EACH LEADS WITH WHAT IS TRUE ON EVERY SOURCE AND QUALIFIES THE REST.** The same `✕` renders for a
# second reason — a source with no throughput to divide by, a patch in deep winter — and there is no
# quarry on a forage tile to break off and retreat. So the retreat rides a CONDITIONAL clause rather
# than an assertion, and the flat-take half says only that more hands do not lift it.
const CREW_TARGET_CLEAR_UNREACHABLE_TOOLTIP := "No crew this band can field clears it in one turn, and where a quarry breaks off and retreats no crew ever could, at any size."
const CREW_TARGET_HOLD_UNREACHABLE_TOOLTIP := "No crew this band can field takes what grows back here — more hands do not lift the take past it."

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

# **THE KEEPERS ROW AND ITS WHOLE VOCABULARY ARE RETIRED** (`docs/plan_standing_upkeep.md` §2.5).
# `CREW_ROW_MAINTAIN_LABEL`, `CREW_MAINTAIN_WANTS_FORMAT`, `CREW_MAINTAIN_MID_BUILD_NOTE`, the two
# loss formats and `CREW_MAINTAIN_HELD_TEXT` described a stepper on the sheet that staffed one
# source's keeping. Maintenance left the tile — the keeping is the band's `agriculture` /
# `husbandry` role — so a sheet has no keeping crew to compose, and what a source's keeping costs is
# stated by `DetailFormat.at_risk_lines` where the source itself is described — and only when it is
# going UNPAID, the standing bill having been retired with the `Keeping:` row (issue #545).
#
# RETIRED — **`CREW_ROW_BUILD_LABEL`** (`BUILDERS`), the row label of the build crew's own stepper
# (`docs/plan_standing_upkeep.md` §2.5). A verb DECLARES and names no hands: the stepper is gone with
# the trailing worker count the improvement commands used to take, and the pool it would have staffed
# is a standing role card on the Band panel.
# The crew row's note separation, beside its label.
const CREW_ROW_NOTE_SEPARATION := 5

# RETIRED — **`CREW_BUILD_FLOOR_TOOLTIP`**, `%s work a turn holds this rung — only the surplus is
# progress`, the BUILDERS row's threshold (`docs/plan_standing_upkeep.md` §2.4).
#
# **THE THRESHOLD IT NAMED NO LONGER EXISTS.** It was the quoted rung's maintenance rate, which the
# build crew supplied while the meter was below its cost, so a crew under it banked nothing. The
# keeping pool owes that rate at every fullness now and a builder's whole output is progress — there
# is no build-crew threshold on any rung, and a hover that stated one would be a warning outliving its
# mechanism, which is the failure this arc keeps producing. `SourceForecast.min_build_work` and
# `HudWidgets.BUILD_WORK_FLOOR_META` went with it.
#
# The RATE is not lost: it is the STANDING PRICE on the offered face now
# (`BUILD_PRICE_UPKEEP_FORMAT`), beside the build's one-off one, which is what it always was.

# RETIRED — **`BUILD_NO_HANDS_REASON`**, the reason a dead improvement box carried
# (`docs/plan_standing_upkeep.md` §2.5). Declaring a rung used to declare a build WITH a crew, and the
# sim refused a count the band could not staff — so an empty build pool greyed the offer out. A verb
# names no crew now: declaring APPENDS a queue entry, which is legal and free whether or not anybody
# is on the `builders` role. What says nobody is on it is the DECLARED state's *not started* warning.
# (The offer stopped being a control at all in §4.7a — the `⌃` on the Work board declares.)

# A crew TARGET is a PILL, and the shape is the point: the stepper beside it is a boxed control you
# operate, a target is a value you can jump to. Its face carries two registers — the COUNT (what you
# compare against the stepper) over the label naming which of the two answers it is — so, like the
# preset rung's two-line face, it cannot live in one `Button.text`.
const CREW_TARGET_COUNT_FONT_SIZE := 13
const CREW_TARGET_LABEL_FONT_SIZE := 11
const CREW_TARGET_FACE_SEPARATION := 5

# ---- THE READOUT: ONE BOX, FOUR REGISTERS (docs/plan_harvest_floor.md §7.1/§7.2) ----------------
# The take, the verdict and the asides answer three different questions, and the panel's bottom half
# read as three unrelated lines at one size until they were bounded and given deliberately different
# registers. Those three are PERMANENT; the deal (a2) is CONDITIONAL, rendering only where there is a
# rung to state, which is why the box shows three registers as often as four. Loudest first, because
# the order is the reading order:
#
#   a. THE YIELDS ROW — the answer. A big tabular number beside a small uppercase unit, and NOTHING
#      else (`2.34  FOOD`). The rate lives in the CAPTION, so a `/TURN` on the reading says it twice;
#      and a `→ CAMP` destination tail earned its width only while trade was the odd account out,
#      banked faction-wide — #381 routed it band-local and #527 retired it outright, after which the
#      suffix was identical words on the readout's widest line (`labor-ui.md` → "A reading states its
#      unit and NO destination"). The render-only-where-the-vector-pays rule is unchanged: a hay-only
#      meadow shows no food line at all, because `provisionsPerBiomass` is genuinely 0 there and a
#      `0.00 food` reading would be false, not empty.
#   a2. THE IMPROVEMENT DEAL — ONE labelled row a composed or offered rung adds beneath the take:
#      what the finished rung will pay. It renders only where there is a rung to state, which is why
#      it is a register rather than a fourth permanent row. **A SECOND, `WITHOUT THE BUILD` BASELINE
#      ROW WAS TRIED AND RETIRED**: the dip multiplies the CREW, so a crew big enough to saturate the
#      source pays none of it and the baseline printed the headline straight back — and a sheet with
#      no rung composed states it live anyway, in the register the player is already reading.
#      `Readout.improvement_deal_rows(...) == 1` and `deal_repeats_a_yields_number` pin that removal;
#      the long form is in `labor-ui.md` → "THE PAYOFF LIVES IN THE READOUT".
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
# The deal row reads at the VERDICT's size, which is the register it belongs to: it explains the take
# above it, so it must not compete with that take, and it is a live consequence of the composed rung
# rather than a footnote, so it must not sink to the aside's.
const READOUT_DEAL_VALUE_FONT_SIZE := READOUT_VERDICT_FONT_SIZE
const READOUT_ASIDE_FONT_SIZE := 11
const READOUT_ASIDE_SEPARATION := 4

# Every policy button's tooltip leads with this — the policy name + its full metric ("Sustain — up to
# +0.90/turn"), since the compact button face no longer carries the name. A gated button appends its
# gate reasons below (one per line), so a hover names the rung AND explains any lock.
const POLICY_TOOLTIP_NAME_FORMAT := "%s — %s"

# The pen as a managed POPULATION (docs/plan_corral_managed_population.md). **A PEN IS FED BY LAND AND
# BY FODDER, NEVER BY THE PEOPLE'S LARDER** — human food is not animal feed. It eats the grass its
# fenced footprint grows plus whatever FODDER its keeper carries in, and `pen_fed_fraction` is the
# share of its demand those two covered last turn. Anything below fully-fed means the herd is SHRINKING
# and its yield with it, so the drawer's `Fed:` row leads with a ⚠ in DANGER ink and the herd's map
# glyph tints red. `PenStatus` owns that test (shared with MapView).
#
# **THE CORRAL ROW STATES THE RUNG AND NOTHING ELSE.** It wore the starving face
# (`PEN_STARVING_LABEL`, retired) beside its own build meter, so a starving pen read
# `Corral: ⚠ Starving — 47% fed 100%` — how FED the herd is against how BUILT its fence is, two
# unrelated percentages with no separator. Feed is the `Fed:` row's whole job, mark and tint included.
#
# **THERE IS NO PEN-FEED COST ROW ANYWHERE** — not in the herd drawer, not on the band's food ledger,
# not beside the pre-commit Corral payoff. A pen bills the food larder for nothing, so a shortfall has
# no price to quote; it has a CONSEQUENCE, and the starving state is it.
#
# Grazing 2d-γ — the pen is fenced LAND that grazes itself. Two herd-drawer rows state it:
#   • the FOOTPRINT — "Pen: radius R · N tiles" (`pen_radius` + the SERVER's in-bounds
#     `pen_footprint_tiles` count, displayed VERBATIM — the closed-form hex-disk count is wrong at map
#     edges, so the client never recomputes it).
#   • the FEED ROW — `Fed:`, which carries the whole story in four states:
#         Fed:  100% — all pasture
#         Fed:  100% — 88% pasture · 12% fodder
#         Fed:  ⚠ 47% — 40% pasture · 7% fodder · needs 11.3 more/turn
#         Fed:  ⚠ 40% — 40% pasture · no fodder · needs 12.0 /turn
#     The headline is `pen_fed_fraction`, the first term `pen_pasture_fraction`, the second their
#     DIFFERENCE (the share fodder covered) and the last `pen_fodder_shortfall`, the sim's own
#     `max(0, need − draw)`. **THE WORD IS `fodder`, never `hay`** — the category, as the band's own
#     store row names it.
#     **THE GROSS DEMAND IS NOT ON THE WIRE**: two ratios, a draw and two GAPS are published, no
#     absolute for the total, so the fodder share is a SUBTRACTION of two shares of the same demand
#     and may never be `fodder_draw` divided by a ratio. The shortfall shows only at or above
#     `SourceForecast.FODDER_FLOW_MIN` — a pen its own land covers says nothing rather than
#     `needs 0.0` — and `fodder_draw` under that floor prints `no fodder` in the share's place, nothing
#     carried in being a different fact from a little carried in (its shortfall drops the `more` to
#     match). **The shortfall is published whether or not the keeper can act on it** (unlike
#     `fodder_draw` it is not gated on Foddering), because a fixed footprint under a growing herd is a
#     RISING need and that is the one thing a player must learn before an animal dies of it.
# The Extend-pen affordance (Grazing 2d-γ; command `extend_pen <faction> <x> <y>` at the pen anchor).
# ⛔ THE TILE-CARD CONTROL IT DESCRIBED IS RETIRED, and the dead claim is quoted rather than deleted
# because it reads like a live spec: *"On a built pen with no ring in flight it offers \"Extend pen\";
# while a ring is being worked off (`pen_extend_progress > 0`) it is replaced by a \"Fencing N%\" badge
# — the pen twin of the corral-build \"Building N%\" meter."* §4.9 item 12c moved the declaration to the
# work row's standing-rung mark, where it opens a PRICE card instead of committing on the click. The
# server still rejects an extend at max radius / unowned / Herding-unknown with a feed message, so the
# client still does not pre-gate on those (max radius is not on the wire).
#
# **`pen_extend_progress` IS WORK, NOT A FRACTION**, banked against `pen_extend_cost` on the same herd
# dict, so every percentage of it is `SourceForecast.pen_extend_fraction` — the build queue row's and
# the work row's mark hover — and never the bare field scaled by `PROGRESS_PERCENT_SCALE`, which reads
# 69 banked work units as `6900%`.
# ⛔ RETIRED — **`PEN_EXTEND_LABEL` (`Extend pen`), `PEN_EXTEND_TOOLTIP` AND `PEN_FENCING_LABEL`**
# (`docs/plan_standing_upkeep.md` §4.9 item 12c), with the tile-card control that wore them
# (`DrawerComposeController`, where the whole retirement is recorded). The button was the one build
# declared from somewhere other than the work tab; it is a `⌃` on the work row's standing-rung mark
# now, and it opens a PRICE rather than committing on the click, a ring drawing `animal:pen`'s own
# hurdle pile since §2.7.
#
# The dead tooltip, quoted because it is the only place the ring's MECHANICS were ever written down
# for the player: *"Queue another ring around the pen. A ring is the same job as the pen it widens, so
# it joins the band's build queue like any other job and its builders raise it when it reaches the
# head. Then the pen grazes more land and feeds itself further. Rejected at the pen-radius maximum."*
# The ring card states the first two sentences as PRICES now; the pen-radius refusal is still the
# server's feed message, max radius not being on the wire.
#
# `PEN_EXTEND_CREW_LABEL` (`Fencers`) went earlier: the verb took a trailing worker count for one
# slice, and `extend_pen <faction> <x> <y>` is closed at three tokens again (§2.5).

# …and **`PEN_FENCING_VERB` (`Fencing`) RETIRES WITH THEM**, which was worth checking rather than
# assuming: the badge's face and its `DetailFormat.build_meter_value` hover were its only readers, and
# the BUILD QUEUE row — the one surface that quotes a ring's meter now — states a bare percentage
# beside the rung's own verb (`Corral <herd>`, a ring deriving the verb of the rung it widens). No
# surface left needs a word for the ring's meter.

# WHAT COMMITTING TO AN IMPROVEMENT BUYS AND COSTS — the improvement control's tooltip, one entry per
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

# The overhunting flag itself. **What it MEANS is `LaborAssignment.overdraws` and nothing else** —
# the sim's own verdict, intent AND ability, read off the source's standing row by every surface that
# flies this mark. The `OVERHUNT_EPSILON` that used to sit here was the tolerance on a client-side
# `actual > sustainable` comparison; that predicate is the one the schema forbids outright (a first
# harvest of a stocked source exceeds one turn's regrowth at every floor), and it is deleted rather
# than merely unused.
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
# the button face (`0.96 food · 0.40 fodder` — the first row is the rung's glyph + name) and the VERBOSE
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
# THE CONTROL'S STATES each get one line, in this order of precedence — and **every one of them is a
# `Label`** since §4.7a ①, this sheet having no commit left to offer:
#   1. OFFERED  — the next rung and its terms, plus the remedy naming the control that takes it.
#   2. DECLARED — the same face plus `◷ Queued`, over the *not started* warning.
#   3. RUNNING  — the build meter and its pace.
#   4. DONE     — the state the finished rung leaves the source in, with the NEXT rung beneath it.
#   (GATED replaces 1 wherever a prerequisite is unmet: the reason IS the line.)

# What the rung COMMITS TO, per improvement — the verb phrased against its own subject, so
# "Cultivate" reads as an act on this patch rather than as a rung name floating in a list.
const IMPROVEMENT_OFFER_LABELS := {
    "cultivate": "Cultivate this patch",
    "sow": "Sow a field here",
    "tame": "Tame this herd",
    "corral": "Pen this herd",
}

# What is HAPPENING while it builds — the present participle, so the running line reads as work under
# way rather than as an option still on offer.
#
# **FOUR SINGLE WORDS, and the Corral's used to be a PHRASE** (`Building the pen`). It is the craft's
# own name — the rung earns `Penning` — so the set is uniform without inventing a word, and the
# phrase's extra width was a real cost on the BUILD QUEUE's date column, which states a participle
# beside a percentage and a turn in one clipping slot (`HudSelectionVocab.RUNG_COMPLETES_LEG_FORMAT`).
# There the phrase pushed the column past what the row can afford, and what a clip takes off the end
# of that string is the DATE.
const IMPROVEMENT_RUNNING_LABELS := {
    "cultivate": "Cultivating",
    "sow": "Sowing",
    "tame": "Taming",
    "corral": "Penning",
}

# ⛔ **THE TABLE ABOVE IS AN OVERRIDE LIST, NOT THE ROSTER OF VERBS THAT MAY RUN.** It holds the four
# whose participle English (or this game's own vocabulary) does not give for free — `corral` reads
# `Penning`, which no morphology derives — and every other verb is gerunded here.
#
# **THAT IS WHAT LETS A RUNG NAME ITSELF.** The ROUTE branch's verbs (`grade`, `pave`) arrive from the
# published rung catalog, so a fifth rung added to `intensification_ladder.json` reads `Grading` /
# `Paving` / its own participle in the build queue's date column with no client edit at all — the same
# property the catalog's `display_name` already buys the queue row's FACE. A hard-coded pair here
# would have had to be extended by hand for every rung the config ever grows.
#
# **THE DERIVATION IS THE ONE REGULAR ENGLISH RULE and nothing more**: drop a silent trailing `e`,
# append `ing`, capitalize. It is deliberately not a conjugator — a verb it gets wrong is a verb that
# belongs in the override table above, which is what that table is for.
#
# `""` in and `""` out, because an entry with no verb has no participle and the callers' own bare
# dated face is the right answer there (see `DetailFormat.build_completion_value`).
const IMPROVEMENT_RUNNING_SUFFIX := "ing"
const IMPROVEMENT_RUNNING_SILENT_E := "e"

static func improvement_running_label(verb: String) -> String:
    var stem := verb.strip_edges().to_lower()
    if stem == "":
        return ""
    var named := String(IMPROVEMENT_RUNNING_LABELS.get(stem, ""))
    if named != "":
        return named
    if stem.ends_with(IMPROVEMENT_RUNNING_SILENT_E):
        stem = stem.substr(0, stem.length() - IMPROVEMENT_RUNNING_SILENT_E.length())
    return (stem + IMPROVEMENT_RUNNING_SUFFIX).capitalize()

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

# `<glyph> <verb phrase>` — the offered face, and the ONLY one it has. **THE PAYOFF IS NOT
# ON IT**: the face's `· then <payoff>` sat directly above a PER TURN box quoting a DIFFERENT number
# for the same source, and a player reading the two together had no way to know which question each
# was answering. The payoff is now a labelled row inside that box (`IMPROVEMENT_PAYOFF_ROW_LABELS`),
# beside the take it is meant to be compared against, and the box states nothing but the choice.
const IMPROVEMENT_OFFER_BARE_FORMAT := "%s %s"

# ---- THE JOB'S PRICE, QUOTED BEFORE THE PLAYER COMMITS (docs/plan_unit_costed_work.md §11) --------
# **A VERB WITH NO PRICE WAS SURVIVABLE ONLY WHILE EVERY VERB COST THE SAME 25 TURNS.** A rung now
# declares a fixed size in WORK UNITS and turns are the OUTPUT, so the sheet has to say what this
# improvement costs on this source and what the crew standing here would take to finish it — or the
# player picks between rungs whose prices differ by a factor and are nowhere on screen.
#
# `workCost` is published whether or not a build is in flight, which is what makes the quote possible
# pre-commit at all. The turns half is the SIM's estimate and is dropped entirely where it answers
# "no estimate" — see `SourceForecast.BUILD_TURNS_NO_ESTIMATE`.
const BUILD_PRICE_WORK_FORMAT := "%s work"

# **THE COUNT AND ITS NOUN, spelled once for BOTH compose faces** — the offered face's price clause
# and the running face's tail read the same estimate, so a job one turn out must not say `≈1 turns` on
# one of them. `HudSelectionVocab.BUILD_TURNS_ROW_ONE` is the same pair on the tile card's row; this
# is the compose sheet's, which carries no `at this crew` tail (the stepper IS on this panel).
const BUILD_TURNS_COUNT_FORMAT := "≈%d turns"

const BUILD_TURNS_COUNT_ONE := "≈1 turn"

# **A CREW THAT NEVER FINISHES, in the same slot the count would have taken** — the ∞ face of BOTH
# `SourceForecast.BUILD_TURNS_HOLDS` and `BUILD_TURNS_ROTS`, which differ on this surface by INK
# alone (`SourceForecast.build_pace`: amber holding, red losing) because a compose face is one Control
# and takes one colour. It wears no `≈`: every other reading here is an estimate that could come in
# early or late, and this one is not an estimate at all — at or below what the meter is ROTTING by it
# does not advance, so there is no distribution to hedge. The glyph itself is
# `DetailFormat.BUILD_TURNS_NEVER_GLYPH`, shared with the larder runway and inked as a warning here
# because on a build it is the opposite news.
const BUILD_TURNS_NEVER_FORMAT := "%s turns"

# **A BUILD PARKED ON PURPOSE — the same `BUILD_METER_HOLDS` with NOBODY on it**
# (`docs/plan_standing_upkeep.md` §4.6a). It takes a WORD rather than the `∞` above, and the word is
# the tile card's own (`HudSelectionVocab.RUNG_HELD_FORMAT`), so the two producers say the same thing
# about the same state.
#
# **THE `∞` IS NOT AVAILABLE TO A BENIGN STATE.** That glyph is the larder runway's, shared on the
# strength of a player learning a mark once and reading it everywhere; spending it where nothing is
# wrong teaches that it sometimes means nothing is wrong, which costs the two states where it means a
# great deal. The ink is neutral either way — `BUILD_PACE_HELD` is in no colour table — so without the
# word this face would be the arc's loudest mark in its quietest colour, saying nothing.
#
# It carries no count and no `≈` for the same reason the `∞` does not: a parked meter is not an
# estimate that could come in early or late, it is a standing fact about ground the player put down.
const BUILD_TURNS_HELD := "held"

# `50 work, ≈25 turns` — the price with its estimate. Takes the clause ALREADY SPELLED, never a raw
# count, so the singular can only be decided in one place (`DetailFormat.build_turns_clause`).
const BUILD_PRICE_TURNS_FORMAT := "%s, %s"

# `50 work, ≈25 turns · 2 work a turn from Agriculture to hold` — **THE STANDING PRICE, BESIDE THE
# ONE-OFF ONE** (`docs/plan_standing_upkeep.md` §2.4). A rung costs a PILE once and a RATE forever,
# and an offer that quotes only the pile is quoting half of what the player is agreeing to.
#
# **IT NAMES THE POOL THAT PAYS, and it did not for a release.** The clause read `· 2 work a turn to
# hold`, and reported from play it was meaningless in that context: the rate never said WHO owes it,
# so on a sheet whose every other number is about the crew under the stepper it read as a demand on
# that crew. It is not — it is the band's AGRICULTURE or HUSBANDRY role, and those are the only two
# controls that move it. The role word is `HudWorkVocab.keeping_role_name`, i.e. the same per-web
# pair the work row's under-kept note already names, so the two surfaces cannot send the player to
# different cards.
#
# **IT IS A PRICE, NOT A THRESHOLD, and the wording still carries that.** `to hold` names what the
# rate buys and `from <role>` who supplies it; the retired `holds this rung — only the surplus is
# progress` named a bar a build crew had to clear, which is the mechanism slice 6a deleted. Nothing
# here compares it to a crew.
#
# **IN WORK UNITS, because a supplier's output is not one.** How many hands the rate takes depends on
# what they carry, so a head count here would be a number that goes stale with the band's gear.
#
# The `·` separator is the compose sheet's own, dividing two facts about one offer where the comma
# above divides two halves of one price.
#
# **IT DELIBERATELY DOES NOT SAY `then`.** `· then ` is the RETIRED payoff clause's own phrase — the
# `🌱 Cultivate this patch · then 1.20 food` face this arc replaced with the readout's labelled row —
# and it is still the needle every "the face quotes no payoff" assertion greps for. A standing price
# wearing it makes those assertions find this clause instead, on a sheet where both would be
# legitimate. Same avoidance, same reason, as the crew note's refusal to say `while building`.
const BUILD_PRICE_UPKEEP_FORMAT := "%s · %s work a turn from %s to hold"

# RETIRED — **`IMPROVEMENT_OFFER_PRICED_FORMAT`** (`%s — %s`), the offered face joined to its price:
# `🌱 Cultivate this patch — 50 work, ≈25 turns · 2 work a turn from Agriculture to hold`
# (`docs/plan_standing_upkeep.md` §4.7a ①).
#
# **The price left this sheet** — Ray, from play: *"That information should be on the work tab. No
# need to have it here, it is useless."* — so the pile and the standing rate ride the `⌃` mark's own
# tooltip and the turn count rides the BUILD QUEUE row's date. `DetailFormat.build_price_clause` still
# composes the pair; only this JOIN had nothing left to join.
#
# It is retired rather than left standing because a caller-less format const reads as live code, and
# the shape is one line to write again if the `⌃`'s hover ever wants the price on the same line as
# its sentence rather than beneath it.

## A GATED rung's whole line: the rung's glyph, then the unmet prerequisite in the gate's own words.
## The rung is NOT named here and that is the point — naming it ("Cultivate this patch") reads as an
## offer, and this state exists precisely because there is no offer to make yet. The glyph keeps the
## improvement axis visible and identifiable without making a promise.
const IMPROVEMENT_GATED_FORMAT := "%s %s"

# `<glyph> <participle> 18 / 50 work (42%)` — the running face, and the ONLY one it has.
# The meter comes from `DetailFormat.build_meter_value`, i.e. the same composer the tile card's and
# the herd drawer's rows use, over the same `SourceForecast.improvement_progress` the map badge and
# the work board read — so no two surfaces can quote different meters or word them differently. The
# payoff left this face with the offered one's, for the reason recorded on
# `IMPROVEMENT_OFFER_BARE_FORMAT`; the meter is what this control uniquely knows.
const IMPROVEMENT_RUNNING_BARE_FORMAT := "%s %s"

# …and the estimate of what is left, appended where there is one. **It belongs on the FACE rather
# than in the control's note slot** — those notes are WARN-inked (the pen's zero-payoff warning) and a
# falling turn count is neither a warning nor a problem; it is the number the player watches drop as
# they step the crew up one row above. Takes the clause already spelled, like the price format above.
const IMPROVEMENT_RUNNING_TURNS_FORMAT := "%s — %s"

# **THE DEAL'S ROW KEYS, one per rung** — the label the readout's payoff row wears, naming the STATE
# the finished rung leaves the source in rather than the verb that gets it there (`ONCE TENDED`, not
# `CULTIVATE`). The verb is already on the box two rows up; what this row adds is when the number
# beside it starts arriving, which is a condition and reads as one.
const IMPROVEMENT_PAYOFF_ROW_LABELS := {
    "cultivate": "once tended",
    "sow": "once sown",
    "tame": "once tamed",
    "corral": "once penned",
}


# The pen's running upkeep, subtracted from the payoff on the row that states it (Corral only), so
# the deal is never quoted gross on the register that commits to it. `corralYield` does NOT deduct
# the feed, which is why this suffix has to be composed here rather than read off one wire field.
# **RETIRED — `IMPROVEMENT_DEAL_FEED_FORMAT`** (`"%s − %s feed"`), the pre-commit Corral row's
# running-cost column. It subtracted `pen_upkeep` from `corralYield`, and that food-unit pen bill is
# retired: a pen eats pasture and hay, never the larder, so the payoff has nothing to be netted
# against and the row states `corralYield` bare. Do not reintroduce a separator with nothing after it.
#
# **AND THE HAY BILL CANNOT TAKE ITS PLACE, which was checked rather than assumed.** Quoting what the
# pen WILL cost in hay would be a better row than the bare payoff, but every hay figure on the wire is
# a fact about a pen that EXISTS: `penFodderShortfall` publishes `0` on the unpenned herd this row is
# composed for, and the gap it is drawn from — `max(0, demand − footprint_intake)` — is not published
# per pen at all. Nothing else on the wire reconstructs it either: the herd's
# `fodderPerBiomass` is its YIELD rate (structurally `0` on every animal — no animal PAYS fodder) and
# not the feed coefficient, and the footprint intake of a fence that has not been built is nowhere.
# So the projected need would have to be synthesized from a ratio or minted as a new wire field, and
# both are forbidden here. **The row states the payoff alone until the SIM publishes a projected
# demand for an unpenned herd.**

# `<glyph> <state noun>` — the done label. Static: there is nothing to uncheck and nothing to clear.
const IMPROVEMENT_DONE_FORMAT := "%s %s"

# **THE ONE ASYMMETRY BETWEEN THE TWO WEBS, and it is deliberate** (spec §4): a penned herd cannot
# graze, so someone feeds it every turn. That standing obligation belongs with the standing state, so
# the Corral done label carries it and the Tame one does not. Do not make these match.
# **RETIRED — `IMPROVEMENT_DONE_UPKEEP_FORMAT`**, the Corral done-state's `· N.NN /turn upkeep`
# clause, for the same reason: it quoted the retired `pen_upkeep`. Both webs' done faces are now the
# bare `IMPROVEMENT_DONE_FORMAT`, and the standing price a built rung really carries is WORK, stated
# on the work row's `⌃` tooltip for every rung alike.

# **RETIRED — `IMPROVEMENT_ABANDON_HINTS`** (`docs/plan_standing_upkeep.md` §2.4). Two per-web
# sentences described what UNCHECKING a running build did, and there is no uncheck: the running
# control is a state Label now, because `abandon_improvement` cleared a STORED verb and the verb is
# derived from the meter. Walking away is taking the BUILDERS to zero, which the stepper beneath the
# control says in the only way that cannot go stale — a number the player sets.
#
# **What the two lines said is still TRUE and still differs by web**, which is why the retirement is
# recorded rather than the strings quietly deleted: unstaffing a plant build lets its meter BLEED at
# the rung's `decay_per_turn` (~100 turns to zero) while an animal meter is KEPT (`domestication` is
# monotone-up and the pen rung declares no decay). Nothing is refunded on either web. That fact now
# reaches the player through the source's own `Keeping:` / `At risk:` rows, which state the live
# shortfall and the turns of grace left rather than a hypothetical about a control.
#
# **THERE IS NO PHASE-KEYED PAUSE LINE, and `IMPROVEMENT_PAUSED_FORMAT` is not coming back.** It read
# "⚠ Paused — the source is Stressed, and this only advances while Thriving. … ease off and it
# resumes", which was true of a sim that gated every build on `EcologyPhase::Thriving`.
# `docs/plan_harvest_floor.md` §3.2 removed that gate, so the line rendered a WARN "Paused" beneath a
# meter the same face showed advancing. Nothing replaced it, and nothing should: since the build crew
# left the tile (`docs/plan_standing_upkeep.md` §2.2) `build_supply` is the builders' own output and
# reads neither the phase nor the floor, so there is no pace here to state. **The one thing the floor
# still does to a build it does BACKWARDS** — a higher floor empties the escapement room sooner, and
# an empty room closes the `eligible` gate on `plant:tended` / `animal:pastoral` — and the sheet says
# THAT by dropping its turn estimate entirely, which is where that gate is read
# (`SourceForecast.build_turns_at`). A note claiming the floor sets the RATE would send a player with
# a slow build to the wrong dial; the remedy is hands, on the band's Builders role.

# A ZERO PAYOFF IS DATA, NOT A MISSING NUMBER — and it is the single most valuable thing the running
# control can say. The pen's harvest is constant ESCAPEMENT (take only the biomass standing above
# `K/2`), so a herd at or below the MSY point honestly pays **0.00** until it rebuilds: penning it
# would eat feed every turn and pay nothing. That must never be suppressed, blanked, or em-dashed away
# — a player who pens a depleted herd because the UI declined to show them a zero has been actively
# misled. So the zero renders in full on the readout's payoff row, and this WARN-inked note names the
# remedy (let it rebuild). The feed term still shows, because the feed is what makes a zero payoff a
# net LOSS rather than merely a nothing.
#
# **IT HAS OUTLIVED TWO HOMES FOR THE ZERO IT EXPLAINS, DELIBERATELY.** The zero was a deal LINE's
# third term, then the control's own face, and is now the readout's payoff row; this note has stayed
# on `HudWidgets.build_improvement_control`'s note slot throughout — the same slot the paused-build
# line uses, with the same WARN ink — because it is a warning about the RUNG, which is what the
# control is, rather than a footnote to whichever register currently prints the number.
# (The "is it zero" floor is the shared `SourceForecast.FOOD_FLOW_MIN` — one definition of "below
# this, there is no flow here", used by the band ledger's rows and by this note alike.)
const IMPROVEMENT_DEAL_DEPLETED_NOTE := "⚠ Too depleted to pen — it would eat feed and pay nothing until the herd rebuilds."

# **A RUNG WITH NOBODY ON IT SAYS SO, IN THE SAME NOTE SLOT AND THE SAME AMBER**, keyed by
# `SourceForecast.unstaffed_build_state`'s two answers. Declaring a build and staffing nobody is a
# LEGAL, meaningful order — it commits the crop, and the player may staff it next turn — so this
# never blocks the commit; what it fixes is that the order was invisible. The sheet quoted
# `Cultivating 0 / 50 work (0%)` and nothing else, because a crew of nobody has no turn estimate to
# print, and an honest absence of information read as *fine*.
#
# **THE TWO ANSWERS NEED DIFFERENT WORDS, which is the whole reason they are two.** Nothing has been
# built yet — so nothing is being lost, and the remedy is simply hands — versus a meter that already
# holds work and is bleeding it back, where the same hands are also stopping a loss. A single line
# covering both would understate one and overstate the other. Neither is either `∞ turns` state one
# rung over, both of which are a crew that EXISTS and is too small to outrun the meter's own rot
# (`SourceForecast.BUILD_TURNS_HOLDS` matching it, `BUILD_TURNS_ROTS` under it).
#
# They are two flat consts rather than a state-keyed table because a table would put a live
# `SourceForecast.*` reference in a `const` initializer here, and a vocabulary module is kept a
# cycle-free LEAF; the two-branch pick lives at the one call site that already holds both states.
# **IT NAMES THE ROLE, BECAUSE THERE IS NO STEPPER BELOW IT ANY MORE**
# (`docs/plan_standing_upkeep.md` §2.5). It read *"Set the builders below to begin"* while this
# control carried a BUILDERS stepper of its own; that stepper is retired, so the sentence pointed at
# a control the player cannot find — the warning-outliving-its-mechanism failure this arc keeps
# producing. The lever is the band's Builders role card, and the line says so.
const BUILD_UNSTARTED_NOTE := "⚠ Not started — nobody is on this band's Builders role."

# **THE DECLARED STATE'S OWN WORD** (`docs/plan_standing_upkeep.md` §4.7a ①). Its face is the OFFER's
# rung name, one composer for both states, so without this clause a queued rung and an unqueued one
# would read identically — which is exactly the distinction the retired checkbox's TICK used to carry.
#
# **IT RIDES THE FACE RATHER THAN THE NOTE SLOT, and the note slot is why.** Both notes DECLARED can
# carry (`BUILD_UNSTARTED_NOTE`, `IMPROVEMENT_DEAL_DEPLETED_NOTE`) are warnings, so that state passes
# `warn_notes` and the whole array renders amber. A job waiting its turn is not a warning, and a
# neutral fact in the warning register is how a player learns to stop reading the register.
#
# **IT STATES NO QUEUE POSITION AND NO DATE, and that is a decision rather than an omission.** The
# compose sheet says what the ground IS and what it would PAY; WHERE the job sits in the band's list
# and WHEN it lands are the Work tab's, which is the surface that can reorder it.
const BUILD_QUEUED_CLAUSE := "◷ Queued"

# `<rung> · ◷ Queued`. The `·` is the compose sheet's own separator between two facts about one rung.
const IMPROVEMENT_DECLARED_FORMAT := "%s · %s"

# **THE OFFERED LINE IS A POINTER, AND IT IS ONE LINE** (`docs/plan_standing_upkeep.md` §4.7a ①, ③).
#
# It shipped for a day as a FACT line (`🌱 Cultivate this patch — 50 work · 2 work a turn from
# Agriculture to hold`) with a REMEDY note beneath it (`Queue it from the Work tab's ⌃.`). Reported
# from play, twice over: the fact line **reads as an imperative** — as the button it used to be — and
# the remedy under it is a second sentence saying the first one is not a control. One line says both.
#
# **AND THE PRICE IS NOT ON IT.** Ray, on the same pass: *"That information should be on the work tab.
# No need to have it here, it is useless."* The pile and the standing rate are what a job COSTS, and
# the surface that queues, funds and orders jobs is the one where a cost is actionable — so they ride
# the `⌃` mark's own tooltip (`HudWorkVocab.WORK_ROW_READY_TRACK_TOOLTIP`) and the turn count
# rides the BUILD QUEUE row's date. **The rung's PAYOFF stays here**, in the `PER TURN` readout's
# `ONCE TENDED` row: what it pays is the "should I?" this sheet exists to answer, and is not a cost.
#
# **`Work tab` IS A LINK**, rendered as a BBCode `[url]` on the one state that carries it — see
# `WORK_TAB_LINK_META`. That is why these are BBCode-bearing strings rather than plain ones.
#
# **AND IT LANDS ON THE ACTING BAND'S BOARD, not merely on a Work tab.** It switched the tab alone
# for one release, so from the FACTION page it arrived at the faction's Work ROLLUP — which has no
# `⌃` on it at all, so the sentence delivered the player somewhere that could not do what it
# promised. Reported from play on PR #562. The signal carries the band; see
# `DrawerComposeController.work_tab_requested` and `BandPanelController.show_work_tab`.
#
# **THE UNWORKED FORM IS ONE SENTENCE TOO, and the limit it states is Ray's decision** (§4.7a ③): a
# `⌃` lives on a WORK ROW, a band has a work row only for a source it already works, and that is the
# sim's own rule that an improvement verb reaches only such bands. He chose to keep the rule and say
# so where the player meets it, rather than relax it.
#
# **THREE FLAT CONSTS rather than a keyed table**, `BUILD_UNSTARTED_NOTE`'s own reasoning: a table
# keyed on `SourceForecast.LABOR_KIND_*` would put a live cross-class reference in a `const`
# initializer, and a vocabulary module is kept a cycle-free LEAF.
const BUILD_OFFER_WORKED_FORMAT := "%s from the %s."

const BUILD_OFFER_UNWORKED_PLANT_FORMAT := "Send gatherers here first, then %s from the %s."

const BUILD_OFFER_UNWORKED_ANIMAL_FORMAT := "Send hunters here first, then %s from the %s."

# The link's own words, and the `[url]` meta `DrawerComposeController` emits and its `meta_clicked`
# handler parses. A distinct string from the tab's own label so the two cannot be confused by a reader
# — this is the SENTENCE's word for the tab, and it happens to match.
const WORK_TAB_LINK_TEXT := "Work tab"

const WORK_TAB_LINK_META := "work_tab"

# RETIRED — **`BUILD_SLIDING_NOTE`**, `⚠ No builders — this rung is sliding back. Set the builders
# below to hold it.` (`docs/plan_standing_upkeep.md` §4.6a).
#
# **IT FILLED A SILENCE THAT NO LONGER EXISTS.** At a build crew of zero the sheet's own estimate used
# to drop out, so the face stopped at the meter and this note said what was happening. `build_turns_at`
# answers at zero now — the wire's own `∞` in the losing red, or the neutral held reading — so the note
# would restate the line directly above it.
#
# **AND ITS CLAIM WAS WRONG HALF THE TIME.** *No builders* does not mean *sliding back*: with the
# keeping pool holding a meter at any fullness, a parked build whose keeping is covered stays exactly
# where it was put, which is a legitimate thing to do and not a warning.

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

# **THE PLANT WEB'S ONE COMMIT VERB, AT EVERY RUNG** (`docs/plan_standing_upkeep.md` §4.9 item 12c).
# Range-aware: taking from a stand is stationary work (NO expedition fallback), so a tile beyond the
# selected band's `work_range` disables the button rather than offering an alternative.
#
# ⛔ **IT WAS A PAIR — `FORAGE_ASSIGN_BUTTON` (`"Forage"`) AND `TEND_ASSIGN_BUTTON` (`"Tend"`) — AND
# THE FORK IS RETIRED, NOT MISLAID.** The dead claim, verbatim: *"A managed source — a Tended Patch
# or a Field — is never gather-drawn (the sim's `is_managed()` branch), and the ladder config says so
# in its own vocabulary: the `wild` rung's harvest primitive is `worker_take` while `tended` and
# `field` both declare `worker_tend`. So the button follows the rung the source STANDS on."* All of
# that is still true of the SIM and none of it needed a second player-facing word: on a Field the
# sheet read `ASSIGN TENDERS` and then offered the *Gathering* kit, which looks like a bug and is not
# — the tending is the AGRICULTURE pool's, and a hoe does nothing for a harvest. Reported from play by
# Ray, who knows how it works and was still caught by it in the moment. `Harvest` is neutral between
# wild and cultivated, which is the whole defect, and the rung MARK on the row still says which
# ground it is.
#
# **THE HUNT WEB KEEPS ITS PAIR** (`HUNT_ASSIGN_BUTTONS`): `Hunters`/`Herders` is specific and
# collides with nothing.
const HARVEST_ASSIGN_BUTTON := "Harvest"

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
# **THE PLANT WEB'S CREW NOUN, AND THERE IS ONLY ONE** (`docs/plan_standing_upkeep.md` §4.9 item
# 12c) — the twin of `HUNT_CREW_LABEL` on the animal side, still resolved through the ONE function
# `HudFormat.plant_crew_label` so every surface reads the word from one place.
#
# **THE WORD LANDS IN TWO GRAMMATICAL SLOTS AND TAKES TWO FORMS.** This NOUN names the crew — the
# sheet eyebrow `Assign harvesters`, the stepper's row, the drawer's open button, the standing
# summary `♻ 3 harvesters` — while `HARVEST_ASSIGN_BUTTON` above is the VERB the row label and the
# commit button take. One string cannot fill both: `Assign harvest` and `♻ 3 harvest` are the
# readings that prove it. That is why the noun→verb tables below SURVIVE the collapse rather than
# retiring with the fork they used to hold.
#
# ⛔ **IT WAS A PAIR — `FORAGE_CREW_LABEL` (`"Foragers"`) AND `TEND_CREW_LABEL` (`"Tenders"`).** The
# dead claim, verbatim: *"A wild stand is drawn down by FORAGERS; a Tended Patch or a Field is kept
# by TENDERS, the ladder's own `worker_tend` harvest primitive put into words"*, and *"a build in
# flight does not move the noun"* — a crew part-way through a Cultivate or a Sow stayed `Foragers`
# until the rung COMPLETED. Item 12c retired the fork because the second word was already taken: a
# Field's sheet read `ASSIGN TENDERS` and then offered the *Gathering* kit, the tending being the
# Agriculture pool's. `Harvesters` is neutral between wild and cultivated and survives the tech
# ladder where `gatherers` would not — and the crew never changed, only the ground did, which the
# rung mark on the row already says.
const HARVEST_CREW_LABEL := "Harvesters"

# The COMMIT VERB and the dead-button hint per plant crew noun, in the hunt web's own idiom
# (`HUNT_NOOP_HINTS`): keyed by the label the sheet has ALREADY resolved, so the stepper's noun, the
# button's verb and the hint's singular are three readings of one answer and cannot disagree. **ONE
# ENTRY EACH NOW** — these tables are the noun→verb seam, not the rung fork, so collapsing the noun
# collapses them without touching their shape.
const PLANT_ASSIGN_BUTTONS := {
    HARVEST_CREW_LABEL: HARVEST_ASSIGN_BUTTON,
}

const PLANT_NOOP_HINTS := {
    HARVEST_CREW_LABEL: "Nobody assigned yet — send at least one harvester.",
}

# `Assign harvesters ▸` / `Assign hunters ▸` / `Assign herders ▸` — the noun is the same one the
# sheet's stepper uses, so the drawer and the sheet can never disagree about who is being staffed.
const COMPOSE_OPEN_BUTTON_FORMAT := "Assign %s ▸"

const COMPOSE_SHEET_EYEBROW_FORMAT := "Assign %s"

# The drawer's one-line summary of what is ALREADY standing on this source: `♻ 3 harvesters · +2.74
# /turn`. The rate comes from `SourceForecast.source_yield_readout` — never recomputed here.
const STANDING_SUMMARY_FORMAT := "%s %d %s"

const STANDING_SUMMARY_SEPARATOR := " ·"

## The parties inspector strip's two inline links (mirrors the work inspector's Jump/Unassign). The
## second one's face is the VERB PAIR below, since which verb the sim will honour is not fixed.
const PARTY_INSPECT_JUMP := "Jump to party"

## PARTIES zone.
const PARTIES_HEADER_FORMAT := "%d out · %d workers"

const PARTIES_EMPTY_HINT := "No parties in the field."

const PARTY_MENU_TOOLTIP := "Bulk actions for parties in the field."

const PARTY_RECALL_GLYPH := "✕"

## **THE VERB FOLLOWS THE SIM, and these two words are the whole of the fork.** A party still standing
## in its home band's camp with no map report owed folds back the instant the command lands
## (`core_sim` `cancel_party_standing_in_camp`), so the order is a CANCEL of something that never took
## effect; one in the field walks home over turns, which is a RECALL. Every single-party surface — the
## row ✕'s tooltip, the parties inspector link, the Occupants drawer's button — reads the ONE predicate
## `HudBandLaborState.party_cancels_in_camp` and picks a side here, so no two of them can promise
## different things about the same press.
const PARTY_RECALL_VERB := "Recall"

const PARTY_CANCEL_VERB := "Cancel"

const PARTY_RECALL_TOOLTIP := "Recall — the party walks home"

## The cancel branch's tooltip names WHY it is instant, since "Cancel" alone reads as if it might be
## the same round trip under a friendlier word.
const PARTY_CANCEL_TOOLTIP := "Cancel — the party never left camp, so it folds back at once"

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

## ---- Form a new band — the fission verb (issue #511, `docs/plan_band_fission.md`)
##
## **THE PLAYER PICKS ONE NUMBER: how many workers leave.** Children, elders and every store divide
## on the share it implies, so the new band is a smaller copy of the one it came from rather than a
## party with a composition of its own. That is the model, not a simplification of it — per-bracket
## allocation would let a band that cannot feed itself split off the people who cannot feed it, and a
## proportional share closes that by shape rather than by a lever guarding against it.
##
## **IT IS NOT AN EXPEDITION.** No destination, no walk, no arrival: both halves stand where the band
## stood, and the new one moves under the ordinary move order.
## The split's ONE input, and deliberately NOT `Party`: what is being composed is a band, and the
## sheet's whole claim is that it is not sending anyone out.
const SPLIT_STEPPER_LABEL := "Workers"

## What the share works out to, under the stepper. The player is choosing a size, so the size is what
## the sheet echoes back.
const SPLIT_SHARE_FORMAT := "%d%% of the band goes with them"

## The two readouts, in the order the decision is made: what you are making, then what it costs.
const SPLIT_NEW_BAND_HEADER := "The new band"
const SPLIT_HOME_AFTER_HEADER := "The home band, after"

const SPLIT_ROW_PEOPLE := "People"
const SPLIT_ROW_BRACKETS := "Workers · children · elders"
const SPLIT_ROW_PROVISIONS := "Provisions"

## Provisions to one decimal. Food is not people, so it is not apportioned to whole units — the
## larder genuinely divides on the share.
const SPLIT_STOCK_FORMAT := "%.1f"
const SPLIT_ROW_WORKERS := "Workers"
const SPLIT_ROW_CHILDREN := "Children"
const SPLIT_ROW_ELDERS := "Elders"

## `workers · children · elders`, all three already apportioned to whole bodies.
const SPLIT_BRACKETS_FORMAT := "%d · %d · %d"

## A now → after pair. Both sides are whole people from the SAME apportionment pass, so the two
## halves sum to the band's own displayed total.
const SPLIT_BEFORE_AFTER_FORMAT := "%s → %s"

const SPLIT_BAND_BUTTON := "Form the band"

const SPLIT_BAND_HINT := "Split this band in two where it stands. The new band moves like any other."

## What the player is told AFTER a viable split, under the button — the thing that is easy to miss
## about a verb whose result appears on the tile you are already looking at.
const SPLIT_BAND_AFTER_NOTE := "It appears on this tile. Move it like any band."

## ---- WHY a split would be refused
##
## **THE BUTTON IS DISABLED AND SAYS WHY; IT IS NEVER HIDDEN.** A control that vanishes teaches
## nothing — the player who sees the verb only when it is legal never learns that a new band needs
## four workers.
##
## **SECOND PERSON, ONE SENTENCE PER REASON.** Both floors are independent and both can hold at once,
## which is why each is a whole sentence joined by `SPLIT_BLOCKED_SEPARATOR` rather than a clause
## list — fixing one otherwise just reveals the other, one refusal at a time.
##
## Each `%d` is a floor the SIM published on the cohort, never a client-side copy of the config.
const SPLIT_BLOCKED_NEW_TOO_SMALL := "Too few people — a new band starts with %d workers."

const SPLIT_BLOCKED_PARENT_TOO_SMALL := "This band would keep %d workers, below the %d it must hold."

const SPLIT_BLOCKED_SEPARATOR := "\n"

## The wire keys the two floors ride on a cohort dict (`native/src/dict/population.rs`).
##
## **THE FLOORS CROSS THE WIRE, THE VERDICT DOES NOT.** The sheet moves a stepper, so a published
## verdict would need one field per possible composition; what the client renders is a pair of
## thresholds the sim owns, the same shape the per-source forecast uses when it publishes rates
## rather than an answer per party size.
const SPLIT_MIN_WORKERS_KEY := "founding_min_workers"
const SPLIT_PARENT_MIN_WORKERS_KEY := "founding_parent_min_workers"

## The cohort dict's age brackets, in WHOLE PEOPLE as the wire carries them. There are three, not
## four: `working_age` IS the working bracket — the sim publishes no second worker number for a
## reader to disagree with — and the three sum to the cohort's `size` by construction.
const SPLIT_AGE_CHILDREN_KEY := "children"
const SPLIT_WORKING_AGE_KEY := "working_age"
const SPLIT_AGE_ELDERS_KEY := "elders"

## The parties inspector strip is DENSER than the work inspector (up to SEVEN detail lines vs ~1), and
## the T/B parties zone is height-capped at ~300px and CLIPS, so its detail lines are tightened well
## below HudWorkVocab.ZONE_BLOCK_SEPARATION to keep the strip + a party row + the bottom-pinned footer
## inside the box.
##
## **IT WAS 4, AND THE WORST CASE IS WHAT MOVED IT.** A hunt party carrying every optional line at once
## — the one `band_panel_worst_case_party` stages — needs 9 gaps in that column, so each pixel here
## costs the zone nine: at 4 the strip alone measured 218px of a 300px box that also owes a 20px head,
## a 42px party row, a 42px footer and four 6px block gaps. Two pixels is the whole of what padding
## could pay (going lower closes the gap between two 14px lines to nothing); the rest came from merging
## the two ORDERS lines into one — see `DetailFormat.expedition_orders_line`.
const PARTIES_INSPECTOR_LINE_SEPARATION := 2

## Why the three EXPEDITION buttons are disabled — and it says "expedition" because the fourth button
## beside them is NOT one: a split is gated on the band's workers, not on its idle ones, so it can be
## live in the same row. A hint that named the row rather than the missions would flatly contradict a
## button the player can press.
const SEND_PARTY_NO_IDLE_REASON := "No idle workers to spare for an expedition. Free some from Work."

## The compose sheet — MISSION FIRST: the footer launches straight into a mission, so the sheet is
## always already on one and the policy picker is unreachable except under Hunt.
const COMPOSE_MISSION_SCOUT := "scout"

const COMPOSE_MISSION_HUNT := "hunt"

const COMPOSE_MISSION_LABEL_SCOUT := "⚑ Scout"

const COMPOSE_MISSION_LABEL_HUNT := "🏹 Hunt"

## **THE THIRD VERB** (`docs/plan_denial_raid.md` §3). Denial is a MISSION rather than a preset on the
## hunt form, because the thing it changes is a BOUND and not a number: the party never stops
## engaging, so it carries no floor, no fill target and no crew preset — a herd and a party size, and
## nothing else. `floor` must never appear anywhere in its UI.
const COMPOSE_MISSION_DENY := "deny"

## **THE FOURTH VERB, AND IT IS NOT A MISSION AT ALL** — a split makes a band rather than sending a
## party. It sits beside the other three because this is where the player already comes to divide
## people out of a band; nothing else about it is an expedition.
const COMPOSE_MISSION_SPLIT := "split"

## 💀 is the STRIP zone's own glyph (`FoodIcons.FLOOR_ZONE_ICONS`), and it is right here for the same
## reason it is right there: leaving nothing standing. It cannot collide with a floor glyph on this
## control — a denial form has no floor picker at all — and the three footer buttons name their
## missions in words beside their marks.
const COMPOSE_MISSION_LABEL_DENY := "💀 Deny"

const COMPOSE_MISSION_LABEL_SPLIT := "⌂ Split"

# =====================================================================================
#  THE SHIPMENT (arc #527, issue #517) — the FIFTH footer button and the FOURTH mission
# =====================================================================================

## **THE FOURTH MISSION.** A shipment is a party that walks it: it names another BAND rather than a
## herd or a tile, and what it carries is a manifest drawn off its home band's own stores. It is a
## mission and not a mode of the hunt form for the plainest possible reason — it shares no field with
## one. No quarry, no floor, no policy, no trip forecast; a destination, a party and a cargo list.
const COMPOSE_MISSION_TRADE := "trade"

## 📦 is the mark the shipment wears on all three surfaces — this button, the parties row and the map
## marker — the `💀` rule: one mission, one glyph, so a party's mark means the same thing wherever it
## is drawn.
const COMPOSE_MISSION_LABEL_TRADE := "📦 Trade"

## Mission → `HudSprites` MARK ID, for the launch buttons whose glyph is PICTOGRAPHIC (issue #249).
## A mission listed here puts its mark on the `Button`'s own `icon` property and takes its label from
## `MISSION_LABELS_SPRITE` below, the verb without the leading glyph; one absent from it keeps its
## `COMPOSE_MISSION_LABEL_*` glyph face.
##
## **THE IDS ARE THE ACTIVITY'S AGAIN** — `hunt` is the file the roster row, the work board's filter
## chip and the kit picker all draw, so a hunting party is marked the same way wherever it is spoken
## about. `scout` likewise retires the ⚑ FLAG for the drawn footprints, which is a vocabulary change
## and not just an art one: the flag was this grid's own mark for a mission the rest of the client
## already spelled with a compass, and one job may not have two drawings.
##
## ⛔ **`split` IS ABSENT AND IT IS NOT A GAP.** `⌂` is a text-presentation SYMBOLIC glyph, which
## #249 leaves as text — and a split is not a mission at all (it makes a band rather than sending a
## party), so the grid reading four drawn marks and one glyph states that difference rather than
## hiding it.
const MISSION_MARKS := {
	COMPOSE_MISSION_SCOUT: "scout",
	COMPOSE_MISSION_HUNT: "hunt",
	COMPOSE_MISSION_DENY: "deny",
	COMPOSE_MISSION_TRADE: "trade",
}

## The launch faces once their mark is bundled ART — the verb alone, the glyph gone, because a
## `Button` carries art on its `icon` PROPERTY and a face that kept the glyph would say its mission
## twice. Art OR glyph, never both — the rule the work chips and the kit picker already follow.
const MISSION_LABELS_SPRITE := {
	COMPOSE_MISSION_SCOUT: "Scout",
	COMPOSE_MISSION_HUNT: "Hunt",
	COMPOSE_MISSION_DENY: "Deny",
	COMPOSE_MISSION_TRADE: "Trade",
}

## What a launch button's art may occupy, through the stock `icon_max_width` theme constant. The
## sources are 256px and a `Button` reserves its icon's drawn size in its MINIMUM, so uncapped art
## would set the whole 3+2 grid's cell size. Sized to the face's own text so the mark reads as the
## glyph it replaced.
const MISSION_ICON_MAX_WIDTH := 16

## **HOW MANY LAUNCH BUTTONS FIT ONE ROW OF THE PARTIES FOOTER, and the fifth is what forced the
## question.** Four fit a 354px dock column at ~62px each; a fifth takes them to ~48, which
## `📦 Trade` does not fit — and the zone `clip_contents`, so what shipped for one render was a
## button SLICED OFF THE EDGE rather than a narrower row. A `GridContainer` at this ceiling wraps to
## 3 + 2, the `build_floor_picker` idiom for exactly this shape (its six rungs wrap 3 + 3), and the
## second row costs the footer one row of height in a zone whose list above it is the `EXPAND_FILL`
## child that gives it up.
const PARTY_FOOTER_COLUMNS := 3

const COMPOSE_TITLE_TRADE := "Load a shipment…"

## The footer button's hover text. It names the one thing that gates the verb — a live tie — because
## a player whose bands have met nobody will find every destination greyed out and must be told why
## by something other than the empty list.
const SEND_TRADE_EXPEDITION_HINT := "Detach a party to carry food and materials to another band you have a tie with."

const SEND_TRADE_EXPEDITION_BUTTON := "Send shipment…"

## The destination row's key. `To` rather than `Destination`: the row is one of the field stack that
## `COMPOSE_FIELD_KEY_WIDTH` (64px, sized for `Quarry`) lines up, and the short word leaves the
## picker its whole share of a 354px dock column.
const COMPOSE_FIELD_DESTINATION := "To"

const COMPOSE_DESTINATION_CHOOSE := "Choose…"

## **WHY THE LIST IS EMPTY, WHEN IT IS.** A band that has met nobody holds no ties, and a picker with
## no entries says nothing at all — so the sheet states the gate in the sim's own terms rather than
## rendering a dead control.
const COMPOSE_DESTINATION_NO_TIES := "This band knows no other band yet. Ties form by standing where you can see each other."

const COMPOSE_DESTINATION_HINT := "Choose who the shipment is for — the manifest below is drawn from this band's stores."

## **A PARKED TIE IS SHOWN, DISABLED, WITH THIS AS ITS REASON — never hidden.** Strength `0` means
## *"we know such a people exist and have no current dealings"*, which is a different statement from
## having never met them, and it is the thing the player has to learn: the TIE is what gates trade,
## so a destination that has decayed out of reach must be visible decaying rather than absent.
const COMPOSE_DESTINATION_PARKED_REASON := "no current tie — nothing can flow"

const COMPOSE_DESTINATION_ENTRY_PARKED_FORMAT := "%s — %s"

## **THE REMEMBERED POSITION, WORDED AS ONE.** A connection grants `Discovered` and never `Seen`
## (`.claude/rules/core_sim/connections.md` → the keystone), so where a band was the last time this
## one saw them is ALL that is known — a remembered band behaves exactly like a remembered herd. The
## sentence therefore states the turn it was learned and never a live position, and every figure
## derived from it wears the `≈` below.
const COMPOSE_DESTINATION_REMEMBERED_FORMAT := "Last seen at (%d, %d), turn %d — they may have moved since."

## **WHAT A BAND THIS FACTION CANNOT NAME IS CALLED.** A tie's subject that is still in the roster is
## named exactly as the cycler and the band picker name it; one that is not is a band we only
## REMEMBER, and the only thing known about it is where it stood — so it is named by that. The raw
## `BandId` is a database key and never reaches a player-facing label.
const COMPOSE_DESTINATION_REMEMBERED_LABEL_FORMAT := "Band near (%d, %d)"

## The approximate walk, off the remembered position. `≈` is load-bearing: the distance is measured
## to where they WERE, so the party may arrive to find nobody and walk on. Omitted entirely when the
## band publishes no move rate or either tile is unknown, rather than quoting a fabricated 0.
const COMPOSE_DESTINATION_ETA_FORMAT := "≈%d turns out, if they are still there."

const COMPOSE_CARGO_SECTION := "Cargo"

## The FOOD row's face. The larder is one commodity, so it is one row — beside one row per MATERIAL
## PILE, which are never summed into a second scalar (the retired trade axis under a new name).
const COMPOSE_CARGO_FOOD_LABEL := "Food"

## The FODDER row's face (issue #590). **"Hay" is what the player calls it and `fodder` is what the
## wire calls it**, the `fauna_id` / `fauna_label` rule: `HudConst.CARGO_ITEM_FODDER` is the id the
## row carries and the token the command spells, and it never reaches a label.
##
## It is a SECOND COMMODITY ROW, not a second food row. Hay and bread are separate accounts that
## never convert, so the sheet lists them as two rows and the manifest reads as a list — a figure
## totalling the two would be the retired trade-goods axis under a new name.
const COMPOSE_CARGO_FODDER_LABEL := "Hay"

## `4.0 hide · tough: excellent` — **THE RATING IS THE ROW'S POINT.** A mammoth hide and a hare pelt
## are both `hide`; a manifest that named only the material would let a player ship the wrong one and
## never know. The readings are the band's own, in the material's declared axis order.
const COMPOSE_CARGO_MATERIAL_FORMAT := "%s · %s"

const COMPOSE_CARGO_READING_FORMAT := "%s: %s"

const COMPOSE_CARGO_READING_SEPARATOR := " · "

## What the band still holds behind a row — **one of the two ceilings a row is bounded by**, stated so
## the player can see how much of the pile the manifest has taken. The other is the pack, and which of
## them binds is whichever is smaller (`BandPanelController._trade_row_max`).
const COMPOSE_CARGO_HELD_FORMAT := "of %s"

## The row's hover text: the WHOLE face — rating included — beside what the band still holds. The
## face itself clips in a 354px dock column, so this is where a long rating stays readable rather
## than lost; the row is ellipsed, never truncated away.
const COMPOSE_CARGO_TOOLTIP_FORMAT := "%s — %s"

## **HOW MUCH ONE PRESS MOVES.** Whole units, because that is how a shipment is talked about; the
## clamp to the ROW's ceiling means a `+` on a 0.6 pile still loads 0.6 rather than refusing, and one
## on a 7.35-unit pack remainder lands on 7.3 rather than overshooting — so no fraction is
## unreachable and no press composes a load the meter beside it then refuses.
const COMPOSE_CARGO_STEP := 1.0

## **THE ROW IS A TYPED FIELD BETWEEN THE TWO STEPPERS** (issue #620). A 6-worker party's full hay
## load is 72 whole-unit presses at the step above, which is not a control — so the amount is a
## `LineEdit` the player can type into, with the steppers kept beside it for the nudge they are good
## at and a `Max` button for the one answer nobody wants to spell.
##
## ⛔ **A `LineEdit`, NEVER A `SpinBox` and never a custom key-eating control.**
## `TextEntryFocus.is_text_entry` — the ONE definition of "the player is typing", read by
## `KeyboardArbiter` — answers `node is LineEdit or node is TextEdit`. A control the arbiter does not
## recognise leaves every polled gameplay key live while the player types into it: WASD pans the map
## and the single-letter panel toggles fire, on the keystrokes meant for the number.
const COMPOSE_CARGO_FIELD_WIDTH := 58.0

## How wide the typed amount may be. A cargo amount is a quantity of a pile, so it needs digits, one
## point and a sign's worth of slack — never a paragraph.
const COMPOSE_CARGO_FIELD_MAX_LENGTH := 12

## **HOW MANY DECIMALS A COMPOSED AMOUNT KEEPS, and it is a FLOOR onto that grid rather than a
## round.** It matches `HudCraftingVocab.BATCH_AMOUNT_FORMAT`'s one decimal on purpose: what the field
## shows and what the manifest ships are then the same number, instead of a row reading `6.0` while
## `6.04998` goes on the wire. **Flooring can never carry a manifest over a cap and rounding can** —
## a tenth left behind is invisible, a tenth over is a server refusal the player did not cause.
const COMPOSE_CARGO_AMOUNT_DECIMALS := 1

## The FOOD row's weight in `DetailFormat.shipment_mass` — food counts as itself. Named because the
## row-headroom arithmetic needs a weight per row and the other two are wire levers; a bare `1.0`
## there would read as a fudge rather than as the mass expression's own first term.
const COMPOSE_CARGO_FOOD_CARRY_WEIGHT := 1.0

## The `Max` button's face and its three readings. **A disabled button that explains itself beats an
## enabled one that does nothing**, so the two dead states carry WHICH of the two caps stopped them:
## the row is already at the largest amount that fits, or there is no room (or nothing held) at all.
const COMPOSE_CARGO_MAX_FACE := "Max"
const COMPOSE_CARGO_MAX_HINT := "Load the most of this that will still fit."
const COMPOSE_CARGO_MAX_AT_CAP_HINT := "Already carrying the most of this that will fit."
const COMPOSE_CARGO_MAX_NO_ROOM_HINT := "No room for this — take something off, or send more hands."
## How wide the `Max` face sits. Wider than a stepper's button because it carries a WORD.
const COMPOSE_CARGO_MAX_BUTTON_WIDTH := 42.0

## The typed field's hover text — the three keys that act on it, because none of them is visible.
const COMPOSE_CARGO_FIELD_HINT := "Type an amount and press Enter. Esc puts the last one back."

## The live mass meter — `▰▰▰▱▱ 30 / 40`. **Every number in it comes off the wire**
## (`expedition_trade_per_worker_carry` × the party for the cap;
## `expedition_trade_fodder_carry_weight` on the hay and `expedition_trade_material_carry_weight` on
## the material total for the mass), never a literal: the sim refuses an over-cap manifest naming
## both sides, and a client quoting a lever of its own would be one config edit from a meter that
## disagrees with the refusal it exists to prevent. The carry term arrives ALREADY RESOLVED (issue
## #626) — the sim has applied whatever carry depends on, and multiplying it by the party is the whole
## of this client's share; the two weights are verbatim levers, being properties of the goods rather
## than of who carries them.
##
## **THREE TERMS, NOT TWO** (issue #590) — `DetailFormat.shipment_mass` holds the whole expression,
## and a reader that drops the hay term UNDER-PRICES every manifest with a bale in it: the meter
## says it fits and the send is refused, which is the exact failure the material lever ships to
## prevent. Pricing is the only thing the three accounts share; nothing on screen sums them.
const COMPOSE_CARGO_MASS_FORMAT := "%s  %s / %s"

const COMPOSE_CARGO_MASS_CELLS := 10

const COMPOSE_CARGO_MASS_LABEL := "Mass"

## **THE SERVER'S REFUSAL STAYS THE AUTHORITY; these two only stop the player meeting it.** An
## over-cap manifest and an empty one are both command failures with a reason — the meter and this
## sentence exist so the send button can say so before it is pressed.
const COMPOSE_CARGO_OVER_CAP_REASON := "Too heavy for this party — add hands or take goods off."

const COMPOSE_CARGO_EMPTY_REASON := "Nothing loaded yet — a shipment carries something."

## The band holds nothing a shipment could carry. A different statement from an empty manifest: there
## is nothing to load, so the rows are absent rather than sitting at zero.
const COMPOSE_CARGO_NO_STORES := "This band has no food, hay or materials to send."

## **HOW MUCH THE COMPOSE SHEET MAY OVERSHOOT THE PARTIES ZONE BEFORE IT LEAVES IT** (see
## `BandComposeFloat` and `BandPanelController._party_compose_floats`). The requirement is summed from
## per-control minimum sizes while the box is a laid-out rect, so the two can differ by a subpixel on a
## sheet that genuinely fits exactly; one pixel of slack keeps a rounding difference from floating a
## sheet the zone holds. It is deliberately not a design margin — a sheet two pixels too tall for a
## `clip_contents` host is two pixels sliced, and it floats.
const COMPOSE_FLOAT_SLACK := 1.0

## **THE NARROWEST PARTIES COLUMN A COMPOSE MEASUREMENT MAY BE BELIEVED AT**
## (`BandPanelController._party_compose_measurable`). A column with no width at all has not been
## anchored into a zone host yet, and nothing measured under it means anything. It is a
## NOT-YET-LAID-OUT test rather than a design minimum: the shipped zone columns are ~354px (side dock
## flank) and wider, so no real column is anywhere near it.
##
## **IT IS ONLY HALF THE TEST, AND THE HALF IT IS NOT IS WHY THIS DEFECT WAS REPORTED TWICE.** A column
## width says NOTHING about whether the column's contents have been laid out, because the two are
## established by different mechanisms: the column is anchored `PRESET_FULL_RECT` into its zone host,
## so Godot hands it the host's width SYNCHRONOUSLY the instant it is reparented, while everything
## inside it is sized by the container sort, which is DEFERRED through the message queue. Measured on
## the empty hunt form in the instant between the two: `col.size.x == 356`, a wholly plausible reading,
## beside `col.get_combined_minimum_size().y == 1278` where the laid-out answer is **207** — every
## autowrap `Label` under it shaping one word per line. 1278px floats that sheet out of every dock this
## client has, and the high-water mark then holds it there for the rest of the composing act: the
## reported picture exactly, `Quarry: Choose…` and a disabled Send floating out of a dock with 800px to
## spare. The other half of the test is the SHEET having been FITTED to this column — see
## `_party_compose_measurable`. **A bare width floor on the sheet does not do it either**: an unsorted
## Control still clamps its own size up to its own combined minimum, so the unlaid-out sheet measures a
## perfectly non-zero 220×903. Only the RELATION between the two widths distinguishes the states.
const COMPOSE_MEASURE_MIN_COLUMN_WIDTH := 1.0

## **HOW MANY FRAMES THE DEFERRED MEASUREMENT WILL WAIT FOR A LAYOUT PASS** before giving up on this
## composing act's render (`BandPanelController._measure_party_compose`). One `process_frame` is the
## normal cost and covers every path measured here; the retry exists because ONE bad reading latches
## for the rest of the composition, so "wait another frame" has to be cheaper than "record it anyway",
## and because the alternative to waiting — returning — leaves the mark unmeasured until the next
## render arms a new one. Bounded rather than open so a sheet whose zone never lays out (a collapsed
## panel, a hidden dock) cannot spin a coroutine for the session; giving up leaves the sheet INLINE,
## which is the safe direction the whole fork is biased toward.
const COMPOSE_MEASURE_MAX_FRAMES := 4

const COMPOSE_TITLE_SCOUT := "Setup a scouting party…"

const COMPOSE_TITLE_HUNT := "Setup a hunting party…"

const COMPOSE_TITLE_DENY := "Setup a denial raid…"

const COMPOSE_TITLE_SPLIT := "Form a new band…"

## The footer button's hover text. It names the deal the whole mission is — kills without stopping,
## brings almost nothing home — because that is the ONE thing a player must know before pressing it.
const SEND_DENIAL_RAID_HINT := "Detach a party to break a herd. It never stops engaging, so it kills far more than it can carry and brings almost nothing home."

## The denial form's own quarry hint. The hunt form's says the rest of the form follows from the
## quarry; on this form the quarry and the party size ARE the whole form, so it says what the number
## under it will answer instead.
const COMPOSE_DENY_QUARRY_HINT := "Choose a herd to break — the collapse estimate follows from it."

## **THE WIDTH EVERY FIELD ROW'S KEY LABEL RESERVES, so the three controls line up as one stack.**
## `Band:`, `Kit` and `Quarry` are three different words in front of three different widget types
## (two `OptionButton`s and a `Button`), and each row is built by a different module — so without one
## declared width the value controls start at three different x positions and the sheet reads as
## three unrelated widgets rather than one form.
##
## **The two obvious alternatives were both measured and both lose.** A key at its natural width puts
## each control against its own word (`Kit` is 22px, `Quarry` 55), which is the ragged edge this
## exists to remove. A key at `SIZE_EXPAND_FILL` splits the row 50/50 — the shape the Kit and Quarry
## rows shipped with — and on a ~245px sheet that leaves the control ~119px, which `🧺 Harvesting
## kit` plus a themed arrow does not fit (it read `Gathering kit` when the width was measured, one
## character shorter, so the conclusion holds a fortiori): the fix for a clipped affordance would have clipped the name
## instead. A declared floor gives the key exactly what the longest key needs and hands the whole
## remainder to the control, which is the axis that has something to lose.
##
## 64 is the widest key on any of the four sheets — `Quarry`, measured at 55px against this client's
## unthemed default font — plus a gutter, so no key can push its own row's control out of line.
const COMPOSE_FIELD_KEY_WIDTH := 64.0

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

## **A HEX CAN HOLD MORE THAN ONE HERD, AND THE MAP CLICK NAMES ONLY THE HEX.** `try_dispatch` is
## handed a TILE, so a click on a tile carrying a rabbit warren and a wolf pack can resolve to just
## one of them and re-clicking resolves to the same one — there was no way to reach the other. The
## Quarry row therefore grows a chooser LISTING the tile's eligible quarries, and it appears ONLY
## when there are two or more: one quarry is the common case and it renders exactly as before.
## It is the `⋯` the zone heads already use, so the panel keeps ONE "there are choices here" glyph.
## A chooser entry names the herd the same way the picked-quarry button does, so the row and the menu
## cannot describe one herd differently: bundled ART where the species has any (as the item's own
## icon), else the emoji through `COMPOSE_QUARRY_LABEL_FORMAT`. Unicode ships ONE deer, so two roster
## species can share a glyph — which is exactly why the art branch exists in the menu too.
const COMPOSE_QUARRY_CHOICES_TOOLTIP := "Another herd shares this hex — choose which one to raid."

## The refusal when the player picks a herd the band can already work from home. The hunt_reach split
## is a rule the map does not spell out, so the refusal is where it gets taught — it names the herd,
## the distance, the reach that binds and the local alternative.
const QUARRY_WITHIN_REACH_FORMAT := "%s is %d tiles away — inside %s's hunt reach (%d). Hunt it from the herd itself instead of sending a party."

const COMPOSE_OF_IDLE_FORMAT := "of %d idle"

# ---- THE KIT (`docs/plan_denial_raid.md`, `equipment.json` `kits`) -------------------------------
## **A KIT DESCRIBES THE CREW, SO ITS ROW SITS DIRECTLY UNDER THE CREW STEPPER AND ABOVE EVERY
## FORECAST** — it moves the fight (the attack tier) and the haul (the carry tier), so every figure
## below it is a function of it. One row, four sheets: the hunting-party form, the denial form, the
## herd drawer's assign-hunters block and the land drawer's assign-foragers block.
const COMPOSE_FIELD_KIT := "Kit"

## The picker's closed face: the job's glyph and the kit's own display name. **A NATIVE `OptionButton`,
## not a pill row and no longer a `MenuButton`** (`HudWidgets.build_option_picker`) — the roster grows
## toward a dozen kits and a row of pills cannot hold that in a 354px dock column, and a control that
## draws its own arrow is the only kind whose affordance a long kit name cannot push off the edge.
##
## **THE CARET IS NOT IN THIS STRING, AND PUTTING ONE BACK RE-CREATES THE DEFECT IT WAS TAKEN OUT
## FOR.** The face used to end in a `⌄` text glyph, because a `MenuButton` draws no arrow of its own.
## `Harvesting kit` is long enough to reach the button's edge (it read `Gathering kit` when this was
## measured, one character shorter), so on the forage sheet the caret was
## clipped away entirely — present in the string, never drawn — while on the hunt sheet it rendered as
## a small low-baseline mark that read as a stray comma beside the `Band:` picker's themed arrow one
## row above. An `OptionButton` draws the arrow as an ICON in reserved right-hand margin that
## `clip_text` cannot eat, so a glyph here would now be a SECOND affordance saying the same thing —
## and the one that clips.
const KIT_PICKER_FACE_FORMAT := "%s %s"

## The glyph is keyed by the JOB THE SHEET IS COMPOSING, never by the kit's id: ids come from
## `equipment.json` and a client-side table keyed on them goes stale the moment a kit is added. The
## glyph says what the crew is walking out to do, which is the same for every kit on one sheet.
## **The two BAND-WIDE roles have faces here too**, since the WORKFORCE zone's role cards mount the
## same picker. Keyed by job like the two above, so a roster that adds a wayfinding kit needs no entry.
##
## **THIS TABLE IS THE FALLBACK HALF NOW** (issue #249): all four jobs have bundled art in
## `KIT_JOB_MARKS` below and render it, so these four render only when a load fails. The subjects
## they name are the EMOJI's — a compass for the scout, an axe for the warrior — and the art
## deliberately does not follow them: the drawn scout is FOOTPRINTS (a compass is several thousand
## years early for this roster) and the drawn warrior is a SHIELD (an axe collides with `hunt.png`'s
## bow in this very control, where the two faces alternate). Read the art's subject off
## `assets/icons/icon_prompts.txt`, never off the glyph it replaced.
const KIT_JOB_GLYPHS := {
	"hunt": "🏹",
	"forage": "🧺",
	"scout": "🧭",
	"warrior": "🪓",
}

## The picker face's BUNDLED ART, keyed by the same JOB (issue #249). A job listed here puts its
## mark on the `OptionButton`'s own `icon` property and drops the leading `%s` from the face
## (`KIT_PICKER_FACE_FORMAT_SPRITE`); one absent from it, or one whose art fails to load, keeps the
## glyph above.
##
## **THE IDS ARE `HudSelectionVocab.ACTIVITY_MARKS`' OWN, AND THAT IS THE POINT.** Both tables are
## keyed by the same four jobs, so the file the roster row draws for a hunting band is the file this
## picker draws on the hunt sheet — one activity, one mark, wherever it renders. The four emoji above
## did NOT have that property: the roster spelled forage 🌾 and this table spells it 🧺, and warrior
## was a 🛡 there and a 🪓 here, which is one job drawn two ways and exactly the drift `hunt.png` was
## written to end.
##
## Coverage is COMPLETE for the four jobs the roster ships, and `KIT_JOB_MARK_FALLBACK` covers the
## fifth case — a job the table has never heard of — so this picker has no glyph face left at all.
const KIT_JOB_MARKS := {
	"hunt": "hunt",
	"forage": "forage",
	"scout": "scout",
	"warrior": "warrior",
}

## The MARK for a job with no entry above — the art twin of `KIT_JOB_GLYPH_FALLBACK`, and the only
## mark in `hud/` whose subject is deliberately GENERIC. It is a carrying basket: it must read as
## *some gear, unspecified* beside four faces that name a specific job, and it must not be
## `trade.png`'s gathered sack, which is the one other "thing you carry" in the family. A handle and
## a hard rim against a knotted neck is what keeps those two apart at row size.
const KIT_JOB_MARK_FALLBACK := "kit_fallback"

## The picker face once its mark is bundled ART — the kit's name alone, the leading `%s` gone,
## because a `Button` carries art on its `icon` PROPERTY and a face that kept the glyph would state
## its job twice. Art OR glyph, never both.
const KIT_PICKER_FACE_FORMAT_SPRITE := "%s"

## What the mark may occupy on that face, through the stock `icon_max_width` theme constant. The
## source PNGs are 256px and a `Button` reserves its icon's drawn size in its MINIMUM, so without a
## cap one art-bearing picker would set the whole compose row's height. Sized to the face's own text,
## so the mark reads as the glyph it replaced rather than as a picture beside a word.
const KIT_PICKER_ICON_MAX_WIDTH := 16

## The fallback face glyph for a job with no glyph of its own — the roster's `jobs` is wire data, so a
## job this table has never heard of must still render a legible face rather than an empty one.
const KIT_JOB_GLYPH_FALLBACK := "🎒"

const KIT_PICKER_TOOLTIP := "What this crew carries. A kit decides what they can hurt and how much they can haul — the line beneath it is this band's own tier, after wear. A kit that could change nothing about this quarry is greyed out and says why."

## **A KIT THAT CANNOT WORK ON THIS QUARRY IS GREYED AND STATES ITS REASON, on the entry's own face.**
## Greyed rather than hidden: *"a snare cannot hold a Red Deer"* is a fact about the world worth
## teaching once, and a kit that simply vanished from three of four sheets is exactly what let the
## picker quote a real take for a hunt that brought home nothing. `%s` the kit's name (with its
## `(default)` mark if it carries one), `%s` the reason.
const KIT_WITHHELD_ENTRY_FORMAT := "%s — %s"
## The WEAPON rule's reason — the kit's fresh attack, resolved against this animal's mass, cannot
## clear its defence. `%s` the quarry. It names the ANIMAL rather than the weapon because what the
## player is choosing between is kits, and the animal is the term that changes under them.
const KIT_WITHHELD_REASON_CANNOT_HURT := "nothing it carries can bring down a %s"
## ⛔ **THE PEN RULE'S REASON IS GONE WITH THE RULE** (issue #543). It read
## *"what it adds is only used on a penned herd"* and fired on a kit whose only contribution was
## `pen_carry`, an `EquipmentStat` that no longer exists — a pen is collected on the hunt's haul, so
## no kit can be pen-only. `KitRoster.kit_offer` owns what the deletion cost and why the rule was
## right to exist while the hurdles were an item.
## The BUILD-BRANCH rule's reason — this kit's tool serves the other food web, so on the build in
## front of it the contribution is the neutral zero. `%s` is the web the entry is on, as a noun a
## player recognises from the ladder rather than as the wire's `plant` / `animal` token.
##
## Worded for the ENTRY rather than for the tool by name, the pen rule's discipline one axis over:
## the fact is that this job cannot read what the kit adds, so `tillage` in front of a `Tame` and
## whatever ships next both get the same sentence.
const KIT_WITHHELD_REASON_BUILD_BRANCH_FORMAT := "its tools are no use on %s"
## The two webs as the picker says them. The wire's tokens are `plant` / `animal`; a player is
## choosing between a garden and a flock, so the reason line says that instead.
##
## **KEYED BY BRANCH IN `KitRoster`, NOT HERE.** A vocabulary leaf must not read a const off a module
## that reads one off it — `const` initializers evaluate at class load, so that cycle fails to load
## the whole client — and `KitRoster` already reads this file.
const KIT_BUILD_BRANCH_PLANT_NOUN := "a crop build"
const KIT_BUILD_BRANCH_ANIMAL_NOUN := "an animal build"
## …and the third branch, which is not a food web at all. `route` is the wire's token; what the
## player is looking at is a road.
const KIT_BUILD_BRANCH_ROUTE_NOUN := "a road build"

## ⛔ **THE RUNG-BOUND REFUSAL, AND IT IS A DIFFERENT SENTENCE FROM THE BRANCH ONE ON PURPOSE.** The
## paving kit in front of a `grade` shares the road's branch — *its tools are no use on a road build*
## would be plainly false, and a reason a player can see is wrong is worse than no reason at all.
## What is true is that a tool bound to one rung resolves to the neutral on every other, so this says
## that instead, in the ladder's own word (`ROAD_PROGRESS_UNNAMED_FORMAT` already says *rung* to the
## player).
##
## **It names no rung**, deliberately: this leaf holds no catalog and the rung's display name lives on
## one, so a format taking a key would print `route:paved_road` at the player the first time a caller
## passed the wrong string.
const KIT_WITHHELD_REASON_BUILD_RUNG := "its tools are for a different rung"

## **THE JOB'S DEFAULT IS MARKED, NOT SEPARATED.** The player needs to know which kit the verb takes
## when they name none; that is a note on an ordinary entry, and a divider would imply the roster has
## two classes of member. `none` has none of that treatment either — it is an ordinary kit that grants
## nothing, and it sorts last only because the roster authors it last.
const KIT_DEFAULT_ENTRY_SUFFIX := "  (default)"

# The hint line under the picker — `attack 20.0 · carry 40.0 per hunter · spears 74 · sled 58`.
const KIT_HINT_SEPARATOR := " · "
const KIT_HINT_ATTACK_FORMAT := "attack %s"
const KIT_HINT_HUNT_CARRY_FORMAT := "carry %s per hunter"
const KIT_HINT_FORAGE_CARRY_FORMAT := "carry %s per gatherer"
## ⛔ **THERE IS NO PEN CLAUSE ON THE HINT LINE ANY MORE** (issue #543). A
## `KIT_HINT_PEN_CARRY_FORMAT := "pen %s per keeper"` stood here arguing *"a sled drags a carcass in
## off the range; a pen stands at the camp, and what bounds a slaughter there is handling gear — so a
## kit carrying only a sled collects a pen at the bare rate."* Handling gear left the roster when
## hurdles became a material, both sides of the rate landed on the sled, and `EquipmentStat::PenCarry`
## was deleted. A penned herd's hint states `KIT_HINT_ATTACK_FORMAT` and `KIT_HINT_HUNT_CARRY_FORMAT`
## like any other hunt row — the same haul number the pen clause used to print, plus the weapon,
## because a pen is fought.
## **HOW MANY OF THE COMPOSED CREW THIS KIT ACTUALLY REACHES** — `3 of 8 equipped`, printed after the
## tier clauses and before the item conditions.
##
## **THE TIERS ABOVE IT DESCRIBE A PERSON, NOT THE PARTY, and without this clause the line let the
## party inherit them.** A band holding ONE spear and composing EIGHT hunters read `attack 20.0`
## while the sim priced seven of the eight bare-handed inside the take curve: the take was right and
## the line was wrong about why.
##
## **IT STATES THE COVERAGE AND NEVER BLENDS THE ATTACK.** A crew-averaged tier would describe
## nobody, and it would be a third number for a division the sim has already published
## (`PopulationCohortState.huntCrews`). The count is the crew the WHOLE kit reaches, not one axis's,
## because this client may not map an axis to the component behind it — see `KitRoster.tier_hint`.
const KIT_HINT_COVERAGE_FORMAT := "%d of %d equipped"
## A component's remaining condition on `equipment.json`'s 0-100 scale, and the word for a spent one.
## **Performance is FLAT until expiry** (durability and performance are orthogonal axes), so this
## number never scales anything above it — it says how much longer the tier lasts, not how good it is.
## **THE ITEM NAMES ITSELF — there is no table of them here.** These two formats take the wire's own
## `KitOption.item_ids` entry, which is the `equipment.json` id (`spears` / `traps` / `sled` /
## `baskets`). The three `KIT_COMPONENT_*` constants that used to supply the name are deleted: they were
## reached through an axis→item guess, and on the Trapping kit that guess printed `spears`.
const KIT_HINT_CONDITION_FORMAT := "%s %d"
const KIT_HINT_DRY_FORMAT := "%s dry"
## **A BAND-WIDE ROLE'S ITEM CLAUSE** — `Wayfinding 100`, `Clubs dry`. It takes `DetailFormat`'s own
## capitalised item LABEL and condition FACE rather than the compose hint's raw wire id, because the
## Gear popover states the identical pair for the identical band (`▲ Wayfinding 66 — …`) and two
## spellings of one reading is how a card and the popover it sits above come to disagree. One format
## for both states: the face is the number or the word `dry`, so this line needs no dry twin.
const KIT_HINT_ROLE_ITEM_FORMAT := "%s %s"
## Tier decimals. The tiers span 1.0 (bare hands) to 40.0 (a sled), authored as small round numbers,
## so one decimal states them without claiming a precision the roster does not have.
const KIT_TIER_DECIMALS := 1

# ---- THE FORECAST QUERY's two non-answers -------------------------------------------------------
#
# **THE FOUR "PRICED FOR ANOTHER KIT / ANOTHER PARTY" LINES ARE GONE, AND NOTHING REPLACES THEM.**
# They apologised for a pre-sampled table: quoted at ONE kit over a FRESH component set, on a floor ×
# party LADDER, so a sheet composing anything else had to say whose numbers it was about to show, or
# refuse to show them. The sim is asked now, and answers the exact (band, kit, party, floor) — there
# is no nearest rung to name and no other kit's raid to disown. A sheet's numbers are always its own.

## **WHILE THE ANSWER IS IN FLIGHT.** First open on a quarry, or a re-query whose previous answer has
## aged past `ForecastQuery.STALE_AFTER_MSEC`. It stands in place of the readout box — never beside
## zeros, which would read as a raid that lands nothing.
const RAID_FORECAST_PENDING := "Costing the raid…"

## The denial twin. Two lines rather than one because the two sheets state different things (a payload
## and a collapse), and a shared "waiting…" would be the only word on either that did not name what it
## was waiting for.
const DENIAL_FORECAST_PENDING := "Costing the raid's toll on the herd…"

## **THE ONE FAILURE LINE, AND IT IS DELIBERATELY NOT SEVEN.** The server's refusal tokens
## (`sim_runtime::commands::query_error`) are all CLIENT BUGS if they ever fire in normal play — the
## sheet composes the request out of the band, herd, kit and party it is already rendering — so prose
## per token would be seven sentences for states the UI is supposed to make unreachable. The token
## rides the line so a report can name it; the player gets one honest "this is not answering".
const FORECAST_FAILED_FORMAT := "No forecast available (%s)."

## The transport's own token, mirroring `native/src/bridge/query.rs`'s `QUERY_ERROR_TRANSPORT`. It is
## the ONE token that is not the server's: a socket that never answered has no `query_error` to give.
const QUERY_ERROR_TRANSPORT := "transport"

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
#   the EXPEDITION's trip is ASKED FOR (`ForecastQuery` → `HuntTripForecastReply.at_composed`:
#       {floor, party_workers, turns_to_fill, delivers_food, …}), forward-simulated
#       server-side for the exact band, kit, party and floor the sheet composed. A trip length is NOT
#       a rate division: above the peak the ceiling is a *stock*, so the party strips the headroom in
#       a turn or two and then crawls at the herd's regrowth trickle. A re-derived `carryCap / rate`
#       closed form is wrong, and wrong by a lot — on a FULL Rabbit Warren a LONE hunter fills in 23
#       turns while a party of 4 never fills within the sim's horizon. So the client does ZERO
#       arithmetic here — it asks, and reads the answer.
# **THIS IS THE BOUNDARY OF THE CLIENT-COMPOSES-THE-CEILING EXCEPTION.** The ceiling is composable
# because it is linear; a raid's trip has no closed form, and a hunt's TAKE is rounded to whole
# animals (`floor(ceiling / bodyMass)`), which is not linear either. The client draws the curve; the
# sim states the take.
# (`delivers_food` says the QUARRY IS EDIBLE (#337) rather than marking a denial mission, so a raid at
# the bare floor delivers like any other. Its `delivers_trade` sibling went with arc #527's axis, so a
# DENIAL raid is now one whose quarry pays no food — a property of the SPECIES;
# `SourceForecast.hunt_trip_forecast` owns that test, and its header records what an inedible quarry
# consequently reads as.)
#
# **THE THREE PRESET FLOORS ARE MARKS ON A DIAL, NOT A SET OF OPTIONS.** The floor is continuous, the
# launch command accepts ANY value in `0.0..=1.0`, and a question carries whatever the chart was
# dragged to — the presets ride the ask as `preset_floors` purely so the three buttons get a face in
# the same round trip. Treating one of them as an offered stance would undo the whole arc.
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

# The clause the ⚠ carries on a hunt. **The compose preview and the confirmed allocation rows read
# ONE field for it** — `LaborAssignment.overdraws` off the source's standing row — so the sheet, the
# tile card's tooltip and the map badge cannot say three things about one herd. The preview used to
# derive its own from the steady forecast, on the reasoning that a composition has no assignment yet;
# what that actually produced was a fourth predicate disagreeing with the other three.
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

# **THE SMALLEST RATE TWO DECIMALS CAN STATE, AND THE FACE FOR EVERYTHING UNDER IT.** A positive take
# rounded to `0` is the one thing this readout may never print: a party that cannot finish a body this
# turn still finishes one eventually — the wound ledger carries the damage between turns — so `≈0`
# says *"hunting this is pointless"* about a crew that is genuinely feeding the band. `<0.01` is the
# honest face; the cadence clause beside it then says how long the wait actually is.
const HUNT_ANIMAL_RATE_MIN_SHOWN := 0.01
const HUNT_ANIMAL_RATE_BELOW_MIN_FORMAT := "<%s"

# ---- A FRACTIONAL ANIMAL IS THE NORMAL CASE, SO THE LINE SAYS WHAT IT MEANS --------------------
# **THE WHOLE-ANIMAL QUANTUM IS A TIMING EFFECT, NOT A CEILING.** A Wild Aurochs (`durability 150`) is
# engaged one animal at a time by every crew from one hunter to eleven, and the blow such a crew lands
# is capped by the body in front of it — so `floor(damage / durability)` is `0` for all of them while
# the expected rate is `0.75` a turn. The sim publishes the rate and carries the remainder on its wound
# ledger; the panel's job is to make the WAIT legible rather than to round it to nothing.
#
# **A DECIMAL ALONE DOES NOT DO THAT.** `≈0.75 Wild Aurochs/turn` is exact and still reads as "not
# quite one", which a player converts to "nothing happens" — the very reading the `≈0` bug produced.
# So a take under one animal a turn states its CADENCE too, in the unit the player waits in. Above one
# a turn nothing is appended: the decimal is self-explanatory there and the line does not grow for the
# ordinary case.
#
# **IT IS AN APPOSITION, NOT A COLUMN.** It closes `HUNT_LIMIT_CREW_FORMAT`'s sentence — the rate
# restated in the unit the player waits in — rather than standing as a third field of a
# `·`-separated strip, which is what it was while a take line of its own sat above the yields.
const HUNT_TAKE_CADENCE_THRESHOLD := 1.0
const HUNT_TAKE_CADENCE_FORMAT := ", about one every %s turns"

# The cadence's own precision — ONE decimal, trimmed like the rate beside it (1.34→"1.3", 2.00→"2").
# Two would assert a precision the quantile band underneath it does not have; zero would round a
# 1.3-turn wait to "every 1 turns", which is the same lie as `≈0` wearing different clothes.
const HUNT_CADENCE_DECIMALS := 1

# ---- THE PRE-COMMIT TAKE, AND WHY IT IS STATED ONCE (`ForecastQuery.KIND_HUNT_CREW_TAKE`) -------
# **THE TAKE IS THE SIM'S ANSWER.** The panel lets the player move the crew before committing, so the
# take has to be re-answered as the stepper moves — and the client cannot answer it: the fight is
# damage over durability against the quarry's defense and the wound ledger it is standing there with,
# and `combat_config.hit_chance` is deliberately unpublished. Composed here without it, the sheet read
# **1.92 food** where a Wild Aurochs paid **0.84** to four hunters, and bone, fibre and hide were over
# by the same 2.3×.
#
# **IT USED TO BE STATED TWICE, WITH THE ACCOUNTS SANDWICHED BETWEEN.** A line of its own led the
# yields block — `≈0.75 WILD BOAR/TURN · 0 – 1 · ABOUT ONE EVERY 1.3 TURNS` — above a `NEXT TURN` row
# whose binding-limit sentence, two lines further down, quoted the same rate again. The line is gone.
# The band below and the cadence above fold into `HUNT_LIMIT_CREW_FORMAT`, which is now the only place
# the crew's take is stated, and the yields' caption alone says which point of the band they are
# quoted at (`YIELD_HEADER_AT_LIKELY_SUFFIX`).

## The band, appended to the take that sentence quotes. **IT IS OMITTED ENTIRELY WHERE THE BAND IS
## DEGENERATE** — which is every reading at the shipped tuning (`combat_config.hit_chance = 1.0`),
## both stochastic stages answering their degenerate identity at any quantile. Range chrome that
## always renders manufactures doubt the model does not have, so
## `SourceForecast.hunt_take_band_is_degenerate` decides whether it appears at all rather than the
## numbers being printed equal to each other.
##
## **PARENTHESES RATHER THAN A `·` FIELD**, for the cadence clause's reason: what it qualifies is a
## sentence now, and a middot strip inside one reads as a fragment of the strip this stopped being.
const HUNT_TAKE_BAND_FORMAT := " (%s – %s)"

## **WHILE THE ANSWER IS IN FLIGHT** — the local hunt's twin of `RAID_FORECAST_PENDING`, and separate
## for that constant's own reason: the two sheets are waiting on different questions, and a shared
## "waiting…" would be the only word on either that did not name what it was waiting for. It stands in
## place of the readout's numbers; the chart, the crew targets and the combat gate above it are
## composed from wire terms and stay.
const HUNT_TAKE_PENDING := "Costing what this crew brings down…"

## **HOW OFTEN A LIVE FLOOR DRAG MAY PUT THE CURVE QUESTION ON THE SOCKET.**
##
## The curve is FLOOR-DEPENDENT — its rows are bounded by the room standing above the escapement
## floor — so a sheet that asks only at the COMMITTED floor states the take for the floor the player
## started from for the whole length of a drag. The drag therefore re-asks; `ForecastQuery.key_of`
## already carries the floor, so a new floor is a new question without anything else changing.
##
## **THE SHAPE IS A LEADING-EDGE RATE LIMIT, not a quiet-window debounce, and the release is why.** A
## quiet window has to fire on a TIMER after the motion stops — a timer this seam has no node to hang
## off, and one that would answer for a floor the player may already have let go of. A rate limit needs
## no clock of its own: the first motion asks at once, so the sheet starts converging immediately, and
## the drag's FINAL floor is guaranteed to be asked whether or not it falls inside a closed window,
## because releasing the drag rebuilds the sheet and the rebuild asks at the committed floor. The
## trailing edge is therefore already owned by the commit, and the only thing left to bound is the
## middle of the gesture.
##
## **THE VALUE IS A HUMAN-MOTION FIGURE, not a frame count.** Under it, one ask per emitted step:
## `HarvestFloorChart` quantises to whole percent, so a fast sweep of the plot puts dozens of distinct
## questions on the command socket in well under a second. Over it, the number visibly lags the line
## the player is dragging. Roughly a tenth of a second is the interval at which a readout still reads
## as "following the drag" while a full-height sweep costs a handful of round trips rather than a
## hundred.
const HUNT_CREW_TAKE_DRAG_ASK_INTERVAL_MSEC := 120

## **THE LARGEST CREW THE TAKE CURVE MAY BE ASKED ABOUT** — `core_sim`'s own `MAX_CREW_TAKE_WORKERS`,
## restated on this side because the client composes the ask.
##
## **THE SERVER REFUSES ABOVE IT RATHER THAN CLAMPING** (`query_error::INVALID_CREW`), which is the
## right rule there — a curve silently answered for a smaller crew than was asked about has a last row
## that is not the plateau the caller thinks it is. It also means an over-large ask costs the player a
## sheet: the take, band, cadence and yields all drop out and `No forecast available (invalid_crew)`
## stands in their place. So the CLIENT clamps, at the one place the question is composed, and the
## question that goes out is one the sim will answer.
##
## **NOTHING IN PLAY REACHES IT.** The ask carries the band's own hunt crew pool
## (`HudBandLaborState.source_crew_pool_hunt` — idle hands plus the ones already on this herd), and a
## thousand hunters on one herd is an order of magnitude past anything the demographics produce. The
## bound is a guard against a bug on this side, not a rule the player can feel.
const HUNT_CREW_TAKE_MAX_WORKERS := 1000

# ---- WHICH LIMIT IS BINDING, AND ITS REMEDY ----------------------------------------------------
# **IT REPLACES THE `settles at N%` ADVISORY ON THIS WEB.** That sentence is composed from the
# projection walk, which carries the engagement and the retreat and NOT the fight — so on the one web
# where the fight is half the answer it named the wrong remedy at the wrong size ("12 herders would
# reach the floor"). The three limits below are all the take actually has, and the smallest of them is
# the one worth stating.

## **THE HERD IS UNDER THE FLOOR THE PLAYER SET** (`biomass < floor × carryingCapacity`). The same
## slot, never an extra block: a herd cannot be below its floor and limited by something else at the
## same time, so the player is never in both states.
const HUNT_LIMIT_BELOW_FLOOR := "Below your breeding floor — taking the surplus only."

## THE HERD BINDS — the take already equals what grows back, so hands added past it eat the stock.
##
## **THE RATE IS THE WHOLE LINE.** It once carried a trailing clause spelling out the consequence
## (*"— more hands would take from the stock, not the surplus"*), which is the sentence restating what
## the number already says to anyone who reads it beside their own crew. Reported from play as too
## wordy. A limit line names the limit; it does not argue.
const HUNT_LIMIT_SUSTAINABLE_FORMAT := "The herd breeds back ≈%s %s a turn."

## THE FLOOR BINDS — there is little standing above it to take, and the floor is the player's own dial.
const HUNT_LIMIT_ROOM_FORMAT := "Only ≈%s %s stand above your floor — lower it, or let the herd grow."

## **THE CREW BINDS — and this is the one limit whose figure IS the take**, which is why the band and
## the cadence ride this sentence and no other. It wears the ⚠-amber severity.
##
## **IT NAMES NO REMEDY, and the stepper's own `max N useful here — more would be idle` is why.** It
## used to close *"— add hands to take more"*: a clause naming no count, two lines under a control
## already stating the exact crew past which a hand is idle, and flatly contradicting it for any crew
## inside one of that cap. The other three limits keep their tails because each names something to
## DO (lower the floor, let the herd grow) or a consequence to weigh (hands past this take stock, not
## surplus); this one named neither.
##
## It names the crew NOUN the stepper above it uses (`hunters` / `herders`), so the sentence and the
## control it describes cannot call one crew two things.
##
## **THE RATE IS BUILT FROM THE SHARED FACE/UNIT PAIR, not spelled out as prose.** It read
## *"≈0.75 Wild Boar a turn"* while the take estimate above the rows carried the `/turn` form, so the
## one figure the sheet states twice was stated in two different units of English; taking the head
## from `HUNT_DELIVERED_FORMAT`'s own pieces is what makes that unrepeatable. The trailing `%s` is the
## band-and-cadence tail, `""` on a certain take of a whole animal or more.
const HUNT_LIMIT_CREW_FORMAT := "These %s bring down " + HUNT_ANIMAL_RATE_FACE_FORMAT + " " \
    + HUNT_ANIMAL_RATE_UNIT_FORMAT + "%s."

## **THE CAPTION OVER THE YIELDS SAYS WHICH POINT OF THE BAND THEY ARE QUOTED AT.** They are single
## numbers by decision — four bands would assert four independent rolls, and the four accounts are
## fixed conversions of ONE carried biomass — so the caption is what keeps them honest beside a take
## line that does carry a range.
const YIELD_HEADER_AT_LIKELY_SUFFIX := " · at the likely take"
