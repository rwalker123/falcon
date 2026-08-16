class_name HudAttentionVocab

## Turn-orb attention / band-decline vocabulary (the attention-registry producers + the losing-
## population decline reasons).

# Turn-orb attention contract (see TurnOrb.gd). The folded-in Alerts panel became
# three producers here: starving (critical), losing_population (warn), idle_workers (warn) —
# plus a fourth, awaiting_orders (warn): an expedition parked at its objective, burning provisions
# until the player acts. That is structurally the SAME class as idle workers (a demand on the
# player, an efficiency loss, not a crisis), so it shares their WARN severity and, like them, must
# be discoverable from the orb rather than only by having the right band panel open.
const ATTENTION_KIND_STARVING := "starving"

const ATTENTION_KIND_LOSING_POPULATION := "losing_population"

const ATTENTION_KIND_IDLE_WORKERS := "idle_workers"

const ATTENTION_KIND_AWAITING_ORDERS := "awaiting_orders"

# A pen whose keeper could not pay this turn's feed: the herd is SHRINKING every turn, and with it
# the yield a 25-turn investment was built for. It recovers if fed again, so this is a reversible
# loss the player must be told about WHILE it is reversible — exactly what the orb is for.
#
# SEVERITY IS DELIBERATELY WARN, NOT CRITICAL, and that is a framing decision about DOUBLE-REPORTING:
# a pen only goes unfed when the keeper's larder came up short, so the SAME empty larder normally
# also trips `starving` (critical) on that band. The two are not one alert twice — they are two
# different LOSSES from one cause (the people are dying / the herd is dying), with two different
# subjects, two different jumps (the band's tile / the herd's tile) and two different remedies. But
# only ONE of them gets to shout: the band's `starving` row stays the critical headline, and the pen
# row rides below it as the consequence the player would otherwise never see coming.
const ATTENTION_KIND_STARVING_PEN := "starving_pen"

const ATTENTION_PEN_LABEL_FORMAT := "%s pen starving"

# The detail carries the fed fraction and the consequence — and NOTHING else. It deliberately does
# NOT name the keeper band: the orb's rows CLIP at POPOVER_WIDTH (sized to the widest producer), and
# appending "· Band 1" pushed this row past it (rendered, looked at, cut). The row already names the
# herd, and its Jump lands on that herd — the band adds nothing the player can act on here.
const ATTENTION_PEN_DETAIL_FORMAT := "%d%% fed — the herd is shrinking"

## **A BUILT RUNG THE BAND'S KEEPING POOL DID NOT COVER** (issue #442; the test corrected against
## `docs/plan_standing_upkeep.md` §2.5) — a Tended Patch or a Field whose Agriculture pool came up
## short, or a tamed/penned herd whose Husbandry pool did. Both are the same loss: 25 turns of
## investment bleeding back to wild while the player looks somewhere else.
##
## **IT IS THE SHORTFALL, NOT A CREW COUNT, AND BOTH WEBS HAD IT WRONG.** The plant half asked *is
## anybody FORAGING this patch* and the animal half compared the herd's HUNT party against
## `upkeepWorkersNeeded`. Keeping is a band-level POOL: there are no per-source keepers to count, a
## patch nobody harvests is still kept, and a herd whose hunting party is smaller than its keeper
## demand is the ordinary case — so the plant test alarmed on every idle rung and the animal one on
## nearly every managed herd. Both now ask `SourceForecast.is_under_kept`, the one gate the source's
## own card asks, so the orb and the card cannot disagree.
##
## **IT CANNOT LIVE ON THE WORK BOARD, and that is why it is an attention row.** The board lists
## ASSIGNMENTS; a source whose keeping is short may have none, so it is *absent* from the board rather
## than flagged on it — the one state the board structurally cannot report. The orb is the generic
## "something needs you" hub and finds the player wherever they are looking.
##
## WARN, not critical, for the `starving_pen` reason: it is a reversible loss, and the grace means it
## has usually not even begun. **The urgency lives in the DETAIL TEXT, not in a persistent counter row**
## — a permanent countdown on a card would make the player watch a number instead of act on it.
const ATTENTION_KIND_UNDER_KEPT_RUNG := "under_kept_rung"

const ATTENTION_KIND_UNDER_KEPT_HERD := "under_kept_herd"

# `Tended Patch (31, 18) under-kept` — the rung noun comes from HudComposeVocab.IMPROVEMENT_DONE_LABELS,
# never retyped, so the alert and the compose sheet's done-state label name the rung identically.
#
# **THE `at` IS GONE, and it is a hex the row was always naming that way elsewhere**: the work board
# writes `Forage (27, 26)` and the map's own labels carry the bare pair. It was the two characters
# between this label fitting the popover and clipping mid-word, and the whole client already reads a
# following `(x, y)` as *where*.
const ATTENTION_UNDER_KEPT_LABEL_FORMAT := "%s (%d, %d) under-kept"

# THE COUNTDOWN CLAUSE, and its three readings. `neglectGraceRemaining` is `(grace + 1) - neglect`:
# `0` means the ground is reverting NOW, `N > 0` means it starts in N more turns of shortfall. The
# third case is `hasNeglectGrace == false` — no countdown published — which must render NO number at
# all, because a false bool beside a `0` would otherwise read as the most urgent state there is.
#
# **THEY LOST THEIR SUBJECTS, AND THE ROW IS WHY.** They read `the tending lapses in %d turn%s` and
# `the ground is reverting now` when the clause was the WHOLE detail and had to say what it was about.
# It is the second half of a sentence now — the pool and its bill lead — so the subject is already on
# the row, and every word spent restating it is a word of the countdown that clips off the popover.
const ATTENTION_LAPSE_SOON_FORMAT := "lapses in %d turn%s"

const ATTENTION_LAPSE_NOW := "reverting now"

# **NOT "nobody is keeping it"** — that named a per-source crew that does not exist, and it read as a
# contradiction beside a detail already quoting a PARTIAL shortfall. All this case knows is that the
# source published no countdown, so that is what it says — and it says it in the two words the row
# has left, this being the longest of the three clauses to sit behind the longest of the two bills.
const ATTENTION_LAPSE_UNKNOWN := "no countdown"

# `Aurochs Herd under-kept` — the herd's own label; the plant web names its hex instead, which a herd
# cannot (it migrates).
const ATTENTION_UNDER_KEPT_HERD_LABEL_FORMAT := "%s under-kept"

# **WHERE THE FACT ENDS AND THE CONSEQUENCE BEGINS**, spelled once and INTERPOLATED into the detail
# format below rather than typed inside it. A countdown can only appear after it, and the bill before
# it carries digits of its own — so an assertion that *no countdown is rendered* has to be made about
# the tail, and it must split on the same string the row was joined with.
const ATTENTION_CLAUSE_SEPARATOR := " — "

# `Husbandry short 1 work — sheds in 3 turns`, and `Agriculture short 1.5 work — reverting now` on
# the plant web. ONE format for both webs, because under the pooled model they are one sentence:
# which POOL is short, by how much WORK, and what happens next.
#
# **`a turn` IS DROPPED AND THE RATE IS NOT LOST WITH IT.** Every figure the keeping model states is
# per turn — the pool card's coverage line, the offered rung's standing price, the sim's own demand —
# so the unit is the arc's default rather than this row's claim, and the two words it costs are two
# words of the countdown that would otherwise clip off the popover.
#
# **IT NAMES THE POOL AND QUOTES WORK, and it had to stop doing neither.** It read `%d of %d keepers`,
# and both numbers were meaningless — the left one was the source's HUNT party and the right its
# keeper demand, two different activities subtracted from each other. Hands are also the wrong unit
# now that gear moves how many hands a rate takes, so the bill is quoted in the work units the sim
# bills it in. The pool NAME is the remedy: `Husbandry` and `Agriculture` are the cards the player
# raises, from `HudWorkVocab.keeping_role_name` so the row and those cards are one word.
const ATTENTION_UNDER_KEPT_DETAIL_FORMAT := "%s short %s work" \
    + ATTENTION_CLAUSE_SEPARATOR + "%s"

const ATTENTION_SHED_SOON_FORMAT := "sheds in %d turn%s"

# `shedding animals now` — the noun went for the plant clauses' reason: the row above it already names
# the herd, so *animals* was the third time this card said what was at stake.
const ATTENTION_SHED_NOW := "shedding now"

# English plural suffix for the countdown clauses ("in 1 turn" / "in 2 turns"). Spelled once, beside
# the formats that interpolate it.
const ATTENTION_TURN_PLURAL_SUFFIX := "s"

## The Telling (docs/plan_the_telling.md): a narrative fork awaiting the player's answer.
##
## CRITICAL and, uniquely, `blocking` — it is the one producer that holds the end-turn. That is a
## deliberate asymmetry with every other row: a starving band is a loss you can choose to accept,
## but a fork is the game asking who your people ARE, and letting it scroll past unanswered is the
## one outcome the arc cannot afford. The out is not "ignore it" but the DEFER choice, which the
## panel always offers and always keeps enabled.
##
## It is NON-LOCATING (x/y = -1): the question lives in a panel, not on a hex, so the orb row reads
## `Open ▸` and routes through `panel_requested` rather than a map jump.
const ATTENTION_KIND_DECISION := "decision"

const ATTENTION_NON_LOCATING := -1

## The orb's rows CLIP at POPOVER_WIDTH, and a fork's narration is a paragraph — so the row carries
## only a fixed prompt and the fork's own first clause; the QUESTION itself belongs in the panel.
const ATTENTION_DECISION_LABEL := "A question awaits an answer"

const ATTENTION_DECISION_DETAIL_MAX_CHARS := 64

const ATTENTION_DECISION_DETAIL_ELLIPSIS := "…"

const UNANSWERED_FORK_LABEL := "A question went unanswered"

const UNANSWERED_FORK_DETAIL := "The turn advanced past a pending fork — it will settle as if nothing was said."

## **A FINISHED BUILD HANDED ITS CREW SOMEWHERE** (`docs/plan_standing_upkeep.md` §2.3). The turn a
## rung completes, its builders have finished the thing they were staffed for: if the finished rung
## declares an upkeep those hands CARRY onto its keeping, and if it does not they are FREE. Either
## way it is a re-allocation the player did not type, on the turn they are deciding what to do next.
##
## **IT IS AN ATTENTION ROW BECAUSE OF WHEN IT HAS TO BE READ.** The sim announces it on the feed,
## which is a log — read after the fact, if at all — and the whole point of the hand-off is that the
## player can re-task those hands BEFORE ending the turn. The orb is the surface that finds them
## wherever they are looking, which is exactly the requirement.
##
## **INFO, not warn.** Nothing is wrong: a build finished, which is the good news, and the row exists
## to hand back a decision rather than to report a loss. It sorts below every real problem.
const ATTENTION_KIND_CREW_HANDOFF := "crew_handoff"

## The row's label — the sim's own sentence, verbatim (*"3 of your cultivate crew stay on (31, 18) to
## keep it"*). Not recomposed here: the sim knows which rung finished, how many hands moved and where
## they went, and a second phrasing of one event is how two surfaces come to describe it differently.
const ATTENTION_HANDOFF_DETAIL_CARRIED := "they are on its keeping now — the source's own sheet moves them"

const ATTENTION_HANDOFF_DETAIL_FREED := "they are idle — the band's work board has them"

## **NON-LOCATING**, like the fork row: the event names its source in words but carries no
## coordinates, and parsing a tile out of a sentence to place a jump is a guess. The row reads
## `Open ▸` rather than promising a hex it cannot reach.
const ATTENTION_HANDOFF_MAX_ROWS := 3

const ATTENTION_HANDOFF_OVERFLOW_LABEL_FORMAT := "+%d more crews changed job"

const ATTENTION_HANDOFF_OVERFLOW_DETAIL := "builds finished and their hands moved"

## **WHICH NON-LOCATING KINDS ACTUALLY OPEN SOMETHING.** A row with no `x`/`y` renders `Open ▸` and
## routes through `panel_requested`, and `TurnOrbController` decides what that opens — so a kind with
## no branch there renders an affordance that does nothing when pressed.
##
## `crew_handoff` is deliberately such a kind: the sim's completion event carries no coordinates, so
## the row can name neither a hex to jump to nor one source to open (a turn may finish several). It
## says WHERE those hands are in words instead, and wears no affordance at all — a promise the row
## cannot keep is worse than no promise.
const ATTENTION_KINDS_WITH_A_PANEL: Array[String] = [ATTENTION_KIND_DECISION]

const ATTENTION_SEVERITY_INFO := "info"

const ATTENTION_SEVERITY_CRITICAL := "critical"

const ATTENTION_SEVERITY_WARN := "warn"

# Awaiting expeditions are listed ONE ROW EACH (not one aggregate like idle workers): each parked
# party is a SEPARATE decision with its own destination, so an aggregate row would have nowhere to
# jump. The popover is positioned ABOVE the orb (`TurnOrb._position_popover`), so an unbounded list
# would climb off the top of the screen and take the `Advance ▸` footer with it — hence a cap, past
# which the remainder folds into a single overflow row that jumps to the first party beyond it.
const ATTENTION_AWAITING_MAX_ROWS := 3

# The under-kept-rung producer caps for THE SAME REASON and therefore at the same number — a mid-game
# empire can hold a dozen improved patches, and the popover climbing off the top of the screen would
# take the `Advance ▸` footer with it. A downward alias rather than a second literal 3: two constants
# holding one number for one reason is how they drift apart.
const ATTENTION_UNDER_KEPT_MAX_ROWS := ATTENTION_AWAITING_MAX_ROWS

const ATTENTION_UNDER_KEPT_OVERFLOW_LABEL_FORMAT := "+%d more under-kept"

const ATTENTION_UNDER_KEPT_OVERFLOW_DETAIL := "Jump to the next one going feral"

const ATTENTION_AWAITING_OVERFLOW_LABEL_FORMAT := "+%d more awaiting orders"

const ATTENTION_AWAITING_OVERFLOW_DETAIL := "Jump to the next parked party"

# The row's context line: "<mission> · <objective>" (the objective is the herd for a hunt party, the
# party's own tile for a scout). Mission words come from HudExpeditionVocab.EXPEDITION_MISSION_LABELS, the demand
# headline from HudExpeditionVocab.EXPEDITION_PHASE_LABELS — neither is retyped here.
const ATTENTION_AWAITING_DETAIL_FORMAT := "%s · %s"

const ATTENTION_TILE_FORMAT := "(%d, %d)"

# Why a band is losing population — appended to the losing_population alert label.
const DECLINE_REASON_STARVING := "starving"

const DECLINE_REASON_LOW_MORALE := "low morale"

# Morale-driven loss is now emigration/relocation (people don't die of low morale —
# see docs/plan_civ_wellbeing.md), so a shrink with emigrants last turn reads this.
const DECLINE_REASON_PEOPLE_LEAVING := "people leaving"
