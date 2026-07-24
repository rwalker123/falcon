class_name HudExpeditionVocab

## Expedition + action-status vocabulary. `STATUS_HINTS` embeds the `EXPEDITION_PHASE_*` keys, so the
## two families MUST live in one module (load-time cycle safety).

# Scouting expedition (docs/plan_exploration_and_sites.md §2). A detached party is a cohort
# tagged Expedition flowing through the same populations[] array as a band; it carries no labor
# in v1, so its drawer shows a dedicated mission/phase/party/provisions readout + Recall/Move
# instead of the labor-allocation panel. The outfit affordance (party stepper + send) lives on a
# resident band's allocation panel.
const EXPEDITION_MISSION_SCOUT := "scout"

const EXPEDITION_MISSION_HUNT := "hunt"

const EXPEDITION_PHASE_OUTBOUND := "outbound"

# One source: the phase key `awaiting` is also the status-glyph key + the orb producer's key.
const EXPEDITION_PHASE_AWAITING := FoodIcons.STATUS_AWAITING

const EXPEDITION_PHASE_HUNTING := "hunting"

const EXPEDITION_PHASE_DELIVERING := "delivering"

const EXPEDITION_PHASE_RETURNING := "returning"

const EXPEDITION_MISSION_LABELS := {
	"scout": "Scouting expedition",
	"hunt": "Hunting expedition",
}

const EXPEDITION_PHASE_LABELS := {
	"outbound": "Outbound",
	"awaiting": "Awaiting orders",
	"returning": "Returning",
	"hunting": "Hunting",
	"delivering": "Delivering",
}

# ---- Action-status vocabulary: row GLYPHS, tooltip WORDS ---------------------------------------
# A Current-actions / Active-expeditions row states its state with a GLYPH (`FoodIcons.STATUS_ICONS`,
# the one glyph registry, exactly like the policy icons) and moves the WORDS into the row tooltip.
# The rows were spelling everything out (`🌰 Forage (27, 26) [sustain] · pending`) — long, and the
# pending row is ALREADY amber, so "· pending" repeated what the tint said.
# Two orthogonal layers (see `FoodIcons.STATUS_ICONS`), kept deliberately separate:
#   • STATUS — what the action is doing: a confirmed local forage/hunt row has no sim phase, it is
#     simply `working`; an expedition's is the sim's `ExpeditionPhase`.
#   • `pending` — a state of the ORDER, not the action (composed locally, not yet acknowledged by the
#     sim, resolves on turn advance). It rides on ANY row, is a MODIFIER (never a phase member), and
#     takes the row's glyph slot + keeps the amber label tint that ties it to the pending map hex.
# EXCEPTION — `awaiting` KEEPS ITS WORDS. It is not a status but a DEMAND ON THE PLAYER: the party is
# parked at its objective burning provisions until you act. A status you already expect is fine to
# hide behind a hover; a call to action must never require one. So an awaiting row renders
# glyph + WARN-tinted words, while every other state is glyph-only.
# Separates a row's trailing glyphs from its label (and from each other): "🌰 Forage (27, 26)  ♻  ●".
const ROW_GLYPH_SEPARATOR := "  "

# Word forms for the two ORDER-level statuses. The expedition PHASE words are NOT duplicated here —
# `HudFormat.status_label` reads them from `EXPEDITION_PHASE_LABELS`, their single source of truth.
const STATUS_LABELS := {
	FoodIcons.STATUS_PENDING: "Pending",
	FoodIcons.STATUS_WORKING: "Working",
}

# The one-line behaviour hint the tooltip appends after the status word ("" = the word says it all).
const STATUS_HINTS := {
	FoodIcons.STATUS_PENDING: "starts when you advance the turn",
	FoodIcons.STATUS_WORKING: "",
	EXPEDITION_PHASE_OUTBOUND: "heading to the target",
	EXPEDITION_PHASE_AWAITING: "parked at the objective — it needs an order",
	EXPEDITION_PHASE_HUNTING: "taking food from the herd",
	EXPEDITION_PHASE_DELIVERING: "bringing the haul home",
	EXPEDITION_PHASE_RETURNING: "heading home",
}

const STATUS_HINT_FORMAT := "%s — %s"
