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

const ATTENTION_SEVERITY_CRITICAL := "critical"

const ATTENTION_SEVERITY_WARN := "warn"

# Awaiting expeditions are listed ONE ROW EACH (not one aggregate like idle workers): each parked
# party is a SEPARATE decision with its own destination, so an aggregate row would have nowhere to
# jump. The popover is positioned ABOVE the orb (`TurnOrb._position_popover`), so an unbounded list
# would climb off the top of the screen and take the `Advance ▸` footer with it — hence a cap, past
# which the remainder folds into a single overflow row that jumps to the first party beyond it.
const ATTENTION_AWAITING_MAX_ROWS := 3

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
