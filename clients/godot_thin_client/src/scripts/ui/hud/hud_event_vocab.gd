class_name HudEventVocab

## Event-dock vocabulary (issue #272, `docs/event_dock_ux_proposal.html` §02/§05) — what an event
## KIND is, how loudly it says it, and how much of it the player asked to hear.
##
## Three questions, kept apart on purpose, because collapsing any two of them is what turns a
## notification system into one undifferentiated stream:
##   • CHANNEL — where it came from. `world` is the sim (a birth, a raid, a discovery); `system` is
##     the client's own plumbing (the command socket dropped, a command was rejected).
##   • RUNG — how loudly. Exactly three, and a kind sits on exactly one. The rung picks the accent
##     and the glyph, and decides which events win a slot when the bar shows fewer than it has.
##   • DETAIL FLOOR — how much. The player's preference is a FLOOR ON THE RUNG LADDER, not a second
##     taxonomy, which is what keeps it three legible options rather than a checklist of kinds.
##
## An ALL-`const` vocabulary leaf (`hud-modules.md`): no funcs, no vars, and it reads only
## `HudStyle`, which reads nothing — so it cannot enter a class-load cycle. The RESOLUTION rules
## that consult these tables live in `EventDockPanel`; the tables live here so a new kind is one
## row in one file rather than a fresh `const` on `HudLayer`.

const HudStyle := preload("res://src/scripts/ui/HudStyle.gd")

# ---- the three importance rungs --------------------------------------------
## "Something was lost, or needs you." Holds its slot until the player has seen it.
const RUNG_ALERT := "alert"
## "The world changed in a way worth knowing." The default floor.
const RUNG_NOTABLE := "notable"
## "A receipt for a thing you asked for." Off by default — this is the retired command feed's
## entire contents.
const RUNG_ROUTINE := "routine"

## Loudest first. Used to rank a pool by importance and to order the detail-level control.
const RUNG_ORDER: Array[String] = [RUNG_ALERT, RUNG_NOTABLE, RUNG_ROUTINE]

## A kind absent from `RUNG_BY_KIND` is a receipt until someone says otherwise — the safe default,
## since an unknown kind that defaulted to Alert would interrupt for anything the sim ever adds.
const DEFAULT_RUNG := RUNG_ROUTINE

# ---- the two channels ------------------------------------------------------
const CHANNEL_WORLD := "world"
const CHANNEL_SYSTEM := "system"
const CHANNEL_ORDER: Array[String] = [CHANNEL_WORLD, CHANNEL_SYSTEM]
const CHANNEL_LABELS := {
	CHANNEL_WORLD: "World",
	CHANNEL_SYSTEM: "System",
}
const DEFAULT_CHANNEL := CHANNEL_WORLD

## The client's OWN kind — socket state, a rejected command, a rollback. It is the one kind no
## snapshot ever carries; the Inspector's console chatter is routed in under it, because a dropped
## command socket is something a player must be told rather than something to bury in a debug log.
const KIND_SYSTEM := "system"

# ---- kind → rung -----------------------------------------------------------
## Straight from §02 of the proposal. Two entries are worth reading twice:
##   • `died` is NOTABLE, not Alert. Bands lose elders to cold as a matter of course, and a rung
##     that interrupts for every one of them trains the player to stop reading the bar — the precise
##     failure the three-rung ladder exists to prevent. A death that MATTERS (a band starving out)
##     announces itself through the starvation and morale channels that already exist.
##   • `born` is ROUTINE while `came_of_age` is NOTABLE: a birth is a mouth, a coming-of-age is a
##     new pair of hands, and only the second changes what the player can do this turn.
const RUNG_BY_KIND := {
	# Alert — violence, an investment lost, and the client's own faults.
	"predator_raid": RUNG_ALERT,
	"hunt_danger": RUNG_ALERT,
	KIND_SYSTEM: RUNG_ALERT,
	# Notable — the world changed in a way worth knowing.
	"died": RUNG_NOTABLE,
	"migrated": RUNG_NOTABLE,
	"came_of_age": RUNG_NOTABLE,
	"site_discovered": RUNG_NOTABLE,
	"found_settlement": RUNG_NOTABLE,
	"campaign_milestone": RUNG_NOTABLE,
	"campaign_victory": RUNG_NOTABLE,
	"expedition_arrived": RUNG_NOTABLE,
	"expedition_returned": RUNG_NOTABLE,
	"tame": RUNG_NOTABLE,
	# Routine — receipts for things the player asked for.
	"born": RUNG_ROUTINE,
	"forage": RUNG_ROUTINE,
	"hunt": RUNG_ROUTINE,
	"sow": RUNG_ROUTINE,
	"cultivate": RUNG_ROUTINE,
	"corral": RUNG_ROUTINE,
	"scout": RUNG_ROUTINE,
	"cancel_order": RUNG_ROUTINE,
	"expedition_sent": RUNG_ROUTINE,
}

# ---- kind → channel --------------------------------------------------------
## Only the client's own kind is not a world event, so this table names the exception and every
## other kind takes `DEFAULT_CHANNEL`. Listing all twenty world kinds here would be a second place
## to forget a row.
const CHANNEL_BY_KIND := {
	KIND_SYSTEM: CHANNEL_SYSTEM,
}

# ---- rung → glyph + accent -------------------------------------------------
## The accent a row wears when no kind- or detail-specific style claims it. `HudStyle` is the
## palette authority — there are no hexes here.
const RUNG_STYLE := {
	RUNG_ALERT: {"glyph": "⚠", "color": HudStyle.DANGER},
	RUNG_NOTABLE: {"glyph": "✦", "color": HudStyle.SIGNAL},
	RUNG_ROUTINE: {"glyph": "◦", "color": HudStyle.INK_FAINT},
}

## THREAT / CASUALTY kinds carry the SAME danger hue as the map overlay that draws them, so the bar
## accent and the map wash speak one danger language. Absorbed verbatim from the retired
## `CommandFeedController.KIND_STYLE` (Predators Phase 3). Consulted only for the kinds it names;
## every other kind takes its rung's style above.
const KIND_STYLE := {
	"predator_raid": {"glyph": "⚔", "color": HudStyle.THREAT_ACCENT},
	"hunt_danger": {"glyph": "⚠", "color": HudStyle.HUNT_DANGER_ACCENT},
}

## **AN INVESTMENT LOST, ON A CHANNEL THAT OTHERWISE CARRIES GOOD NEWS** (issue #442). A rung going
## feral, and an assignment dropped because the band ran out of people, ride their VERB's own kind
## (`cultivate` / `sow` / `forage` / `hunt`) — deliberately, so a rung's whole life reads on one
## channel — which means the loss is the same KIND as the completion that preceded it. `KIND_STYLE`
## cannot separate them; the sim's own `status=` token can.
##
## **MATCHED AS A WHOLE SPACE-DELIMITED `key=value` FRAGMENT, NEVER A BARE SUBSTRING** — the sim
## writes `"status=feral reason=untended …"`, and a substring test on `feral` would also fire on a
## species key or a tile label containing the word. WARN amber rather than the raid crimson: this is
## a loss the player caused by looking away and can still reverse, not an attack.
##
## A row this table claims is promoted to the ALERT rung whatever its kind says — that is the whole
## point of it, and §02 lists both tokens under Alert.
const DETAIL_STATUS_STYLE := {
	"status=feral": {"glyph": "⚠", "color": HudStyle.WARN},
	"status=lapsed": {"glyph": "⚠", "color": HudStyle.WARN},
}

# ---- the detail floor ------------------------------------------------------
## A floor admits its own rung and everything LOUDER. Three settings, no more: the preference is a
## floor on one ladder rather than a per-kind checklist, which is what keeps it legible.
const DETAIL_FLOOR := {
	RUNG_ALERT: [RUNG_ALERT],
	RUNG_NOTABLE: [RUNG_ALERT, RUNG_NOTABLE],
	RUNG_ROUTINE: [RUNG_ALERT, RUNG_NOTABLE, RUNG_ROUTINE],
}
const DETAIL_FLOOR_LABELS := {
	RUNG_ALERT: "Alerts only",
	RUNG_NOTABLE: "Notable",
	RUNG_ROUTINE: "Everything",
}
## Alerts alone are too quiet to feel like a living world; everything is the retired feed.
const DEFAULT_DETAIL_LEVEL := RUNG_NOTABLE

# ---- the detail tokens the dock itself reads -------------------------------
## The band a demographic event happened to. **The snapshot carries no band NAME**, so the sim puts
## its own durable `BandId` in the label as a positional fallback and repeats the id here — which is
## what lets the client re-label the row with whatever IT calls that band. The client's name is a
## ROSTER position (`HudFormat.band_display_name`), the sim's is a durable id, and the two routinely
## disagree; the token is the only thing that can join them.
const DETAIL_BAND_KEY := "band"
## **The exact string the sim writes when it names a band** (`core_sim` `systems::population::
## band_label`). It is byte-identical to `HudFormat.BAND_DISPLAY_NAME_FORMAT` and must stay so: the
## client substitutes its own name by replacing this rendering of the `band=` id, so a drift on
## either side silently turns the substitution into a no-op rather than an error. Substituted only at
## a digit boundary, or `Band 3` would also rewrite the `Band 3` inside `Band 30`.
const SIM_BAND_LABEL_FORMAT := "Band %d"

# ---- row chrome ------------------------------------------------------------
## The `T47` stamp on every row. `INK_DIM` rather than a bespoke blue: the turn is METADATA beside
## the label, and a colour of its own would be a fourth accent competing with the three rungs.
const TURN_STAMP_COLOR := HudStyle.INK_DIM
const TURN_STAMP_FORMAT := "T%d"
## A client-side note has no sim turn behind it, so it wears the turn the client last saw — and
## before the first snapshot there is none. That case prints this instead of a fabricated `T0`.
const TURN_STAMP_UNKNOWN := "T—"
## What the bar says when the floor admits nothing.
const EMPTY_TEXT := "Nothing to report."
## The one-line title the bar becomes while the log is open. THE BAR IS THE COLLAPSED STATE: keeping
## the recent rows while the log is open prints the log's own newest turn-group a second time,
## directly beneath it, which is what the four-row prototype made unmissable.
const EXPANDED_TITLE_FORMAT := "All events — turns %d–%d"
const LOG_TURN_HEAD_FORMAT := "Turn %d"
const LOG_RETENTION_FORMAT := "showing %d of %d retained turns · the sim keeps %d"
const EARLIER_TURNS_TEXT := "Earlier turns"
const EARLIER_TURNS_EXHAUSTED_TEXT := "No earlier turns"
const CHANNELS_LABEL := "Channels"
const DETAIL_LABEL := "Detail"
const ROWS_LABEL := "Rows"
const DOCK_LABEL := "Dock"
const MORE_TEXT := "more"
const CLOSE_TEXT := "close"
## Caret glyphs for the expander. It always points AWAY from the docked edge — i.e. at the map the
## log will open into — so the control reads as a direction rather than as decoration.
const CARET_UP := "▴"
const CARET_DOWN := "▾"
const DOCK_TOP_GLYPH := "▲"
const DOCK_BOTTOM_GLYPH := "▼"
