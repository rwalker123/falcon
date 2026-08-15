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

## The client's OWN kind — socket state, a rejected command, a rollback, and the HUD's own refusals
## ("Quick-hunt · No idle workers to assign"). It is one of two kinds no snapshot ever carries; the
## Inspector's console chatter is routed in under it, because a dropped command socket is something a
## player must be told rather than something to bury in a debug log.
const KIND_SYSTEM := "system"

## The client's SECOND own kind: a receipt for a command the player just issued through the UI —
## "Advance 1 turn.", "Answered the question.", "Stop improving (12, 8).". It restates an action the
## player has this instant taken, so on a notification bar it is pure plumbing, and it is in
## `IGNORED_KINDS` for that reason.
##
## **THE SPLIT FROM `KIND_SYSTEM` IS WHAT MAKES THAT FILTERABLE AT ALL.** Both used to ride
## `system`, so the channel carried an acknowledgement and a FAULT under one name and no kind-level
## rule could separate them. **The boundary is: a command accepted for sending is an echo;
## everything else is a fault.** A rejected or failed send, a lost socket, a resync forced by an
## unapplicable delta and every HUD-side refusal stay `system` — those are exactly when the player
## needs to hear from this channel.
const KIND_COMMAND_ECHO := "command_echo"

## **THE TRANSPORT FAILURE, WORDED AS ONE** — `Not connected to the server — "assign_labor 0 1
## builders 1" was not sent.`
##
## **IT IS NEVER A SIM REFUSAL, AND THE OLD WORDING SAID IT WAS.** The line read
## `Command failed (assign_labor 0 1 builders 1): can't connect`, which reported from play sent a
## player looking for a rules problem that does not exist. `CommandClient.send_line` returns
## `ERR_CANT_ACQUIRE_RESOURCE` when there is no bridge and `ERR_CANT_CONNECT` when the bridge could
## not deliver, and **those are the only two values it can produce** — a command the sim REFUSES has
## already been written to the socket, and its refusal arrives later on the server's own event
## stream. So this site has exactly one meaning and states it: nothing reached the server, and the
## line the player composed is still theirs to re-issue.
##
## **The underlying error code is NOT quoted**, because it carries nothing a player can act on and
## the two codes mean the same thing here; `CommandClient` already `push_warning`s the bridge's own
## message for a developer, and the Logs console keeps this line verbatim.
const COMMAND_NOT_SENT_FORMAT := "Not connected to the server — \"%s\" was not sent."

# ---- kinds the DOCK ignores ------------------------------------------------
## Kinds the dock drops at INGEST, in both inlets (`ingest_events` and `note_system`), never storing
## them. **Not a detail floor, not a channel toggle and not a render-time skip**: an ignored kind
## cannot appear at any detail level — `Everything` included — on either channel, in the bar or in
## the expanded log, and it occupies neither a `seq` de-duplication slot nor a retention row.
##
## **AN IGNORED KIND IS NOT A RETIRED KIND.** The event still exists and is still emitted: the sim
## goes on writing it, `Inspector`'s debug console goes on printing every command echo in full, and a
## mod may well want to read it. This table is a DISPLAY FILTER ON ONE SURFACE — the player's
## notification bar — and nothing here removes an event from the client.
const IGNORED_KINDS := {
	KIND_COMMAND_ECHO: true,
}

# ---- kind → rung -----------------------------------------------------------
## Straight from §02 of the proposal. Two entries are worth reading twice:
##   • `died` is NOTABLE, not Alert. Bands lose elders to cold as a matter of course, and a rung
##     that interrupts for every one of them trains the player to stop reading the bar — the precise
##     failure the three-rung ladder exists to prevent. A death that MATTERS (a band starving out)
##     announces itself through the starvation and morale channels that already exist.
##   • **The demographic kinds split on HEAD-COUNT, and that one line settles all five of them.**
##     `born` / `died` / `migrated` change how many people the band HAS, so they are Notable.
##     `came_of_age` and `aged` move one person between brackets and leave the total untouched, so
##     they are Routine.
##
##     Both halves were learned the hard way, in opposite directions. `born` shipped ROUTINE, i.e.
##     below `DEFAULT_DETAIL_LEVEL`, so a birth never appeared unless the player chose "Everything" —
##     a population counter ticking up while the bar said nothing. `came_of_age` shipped NOTABLE and
##     was reported from a playthrough as **too much noise**: it fires constantly and the population
##     never moves, so it filled the default floor with rows that answered no question.
##
##     Two retired framings, both describing something real on the wrong axis. "A birth is a mouth, a
##     coming-of-age is a pair of hands" measured how much a turn's LABOUR changed; "anything that
##     touches the working-age population" measured which BRACKET moved. Neither is what a rung is
##     for. The rung asks whether the world changed in a way worth knowing, and a settlement gaining
##     or losing a person is the plainest such change there is — while a person having a birthday is
##     not, however consequential the bracket it moves them into.
const RUNG_BY_KIND := {
	# Alert — violence, an investment lost, and the client's own faults.
	"predator_raid": RUNG_ALERT,
	"hunt_danger": RUNG_ALERT,
	KIND_SYSTEM: RUNG_ALERT,
	# **A FOUNDING IS AN ALERT, AND `expedition_arrived` SITTING AT NOTABLE TWO LINES DOWN IS WHY THIS
	# NEEDS SAYING.** The Notable rung is for things that happen to a band as a matter of course — a
	# death, a migration, a party reaching its objective. A founding is the opposite on every count:
	# rare, player-initiated, and the first act in the band economy that cannot be undone
	# (`docs/plan_band_fission.md` §Q6, issue #510). The same kind carries the command's REFUSALS, and
	# a refused irreversible order is exactly as loud as a taken one.
	"band_founded": RUNG_ALERT,
	# Notable — the world changed in a way worth knowing.
	"died": RUNG_NOTABLE,
	"migrated": RUNG_NOTABLE,
	"site_discovered": RUNG_NOTABLE,
	"found_settlement": RUNG_NOTABLE,
	"campaign_milestone": RUNG_NOTABLE,
	"campaign_victory": RUNG_NOTABLE,
	"expedition_arrived": RUNG_NOTABLE,
	"expedition_returned": RUNG_NOTABLE,
	"tame": RUNG_NOTABLE,
	"born": RUNG_NOTABLE,
	# **A SHIPMENT LANDING IS NOTABLE, NOT AN ALERT** (arc #527). It is the one expedition event that
	# happens where OTHER PEOPLE live, which is what earns it a kind of its own — but the ladder asks
	# how loudly, not how novel, and the Notable rung is for a change in the world worth knowing
	# about. It sits exactly beside `expedition_arrived`: a party reached where it was going and did
	# the thing it was sent to do. Alert is for violence, an investment lost and an irreversible
	# player-initiated act (`band_founded`), and a delivery the player asked for turns ago is none of
	# those. A REFUSED shipment is not this kind at all — a rejected command rides `system`, which is
	# already Alert.
	"trade_delivered": RUNG_NOTABLE,
	# Routine — bracket transitions, and receipts for things the player asked for.
	"came_of_age": RUNG_ROUTINE,
	"aged": RUNG_ROUTINE,
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
## Only the client's own kinds are not world events, so this table names the exceptions and every
## other kind takes `DEFAULT_CHANNEL`. Listing all twenty world kinds here would be a second place
## to forget a row.
##
## `KIND_COMMAND_ECHO` is listed even though `IGNORED_KINDS` means the dock never reaches this lookup
## for it: the channel a kind belongs to is a fact about the kind, independent of one surface's
## decision to hide it. Left out, dropping it from `IGNORED_KINDS` would silently file command
## receipts on the WORLD channel beside the births and the raids.
const CHANNEL_BY_KIND := {
	KIND_SYSTEM: CHANNEL_SYSTEM,
	KIND_COMMAND_ECHO: CHANNEL_SYSTEM,
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

## **THE SECOND TOKEN THAT NAMES A BAND** (arc #527): the band a SHIPMENT was sent to or landed at,
## repeated as `destination=<id>` beside a label that names it through the sim's own fallback
## spelling. Its own key because it is a different ROLE in the sentence — a line can name the sending
## band as `band=` and the receiving one as `destination=` at once — and one of the two would be
## rewritten with the other's name if they shared a key.
const DETAIL_DESTINATION_KEY := "destination"

## **THE SIM'S OWN SPELLING FOR THE DESTINATION, and it is LOWER-CASE** —
## `ExpeditionMission::destination_display`'s last-resort tier, `format!("band {}", id)`, which is the
## normal path today because bands have no names. Byte-identical to the sim or the substitution is a
## silent no-op, exactly as `SIM_BAND_LABEL_FORMAT` warns; it is a SEPARATE const rather than a reuse
## precisely because the two producers differ in case, and sharing one would have made the swap look
## correct while never firing.
const SIM_DESTINATION_LABEL_FORMAT := "band %d"

## **EVERY `detail` TOKEN THAT NAMES A BAND, and the sim's rendering of each.** `EventDockPanel` walks
## this rather than carrying one hand-written swap per producer — the client's band name is a ROSTER
## POSITION and the sim's is a durable id, so every place the sim writes a band into a sentence needs
## the same join, and a table is what stops the next producer growing a fifth copy of it.
const BAND_ID_TOKEN_LABELS := {
	DETAIL_BAND_KEY: SIM_BAND_LABEL_FORMAT,
	DETAIL_DESTINATION_KEY: SIM_DESTINATION_LABEL_FORMAT,
}

# ---- rendering a detail as PROSE -------------------------------------------
## **THE `key=value` TOKENS ARE A MACHINE CONTRACT, NEVER A STRING TO SHOW.** The dock used to print
## the sim's wire detail verbatim, so a row read `category=settle_site at (64,36)` — an internal
## identifier on a player-facing surface. The tokens exist for the client to PARSE (the `band=` join,
## the `status=` alert promotion); what the player sees is rendered from them.
##
## `EventDockPanel.detail_phrase` walks these three tables in order; the tables live here because
## they are vocabulary, the walk lives there beside the other detail parsing.

## Keys the LABEL already carries, dropped rather than said twice: `band` is substituted INTO the
## label, `count` is why the label reads "Two children…" / "Four left…", and an `expedition` entity
## id was never for reading.
const DETAIL_KEY_HIDDEN := {
	"band": true,
	"count": true,
	"expedition": true,
	# `killed` is the LABEL's own number — the sim writes "The aurochs hunt cost the party three
	# lives" and then `killed=3.000` beside it, so showing both says one thing twice in two
	# different notations. `wounded` deliberately stays: it is the half the label never carries.
	"killed": true,
}

## Bare words that are grammar, not content. The ` · ` join supplies the separation `at` was doing.
const DETAIL_FILLER_WORDS := {
	"at": true,
}

## The ENUMERATED values, in English. Entries are used VERBATIM — several are deliberately lowercase
## (`cold`, `elder`, `feral`), because they read as a phrase continuing the label rather than as a
## heading, and only the generic fallback capitalises.
const DETAIL_VALUE_LABELS := {
	"settle_site": "Settle site",
	"wondrous": "Wondrous site",
	"landmark": "Landmark",
	"out": "departed",
	"in": "arrived",
	"hunger": "hunger",
	"cold": "cold",
	# `age` is the death every fed band in fair weather actually experiences. Its label is TWO words
	# where the token is one — the wire spells it `age` because a token is a contract, the row says
	# "old age" because the row is prose. Without this entry the generic fallback would render it as
	# `Age`, which is legible but not English in the sentence it lands in.
	"age": "old age",
	"child": "child",
	"working": "worker",
	"elder": "elder",
	"feral": "feral",
	"lapsed": "lapsed",
	"untended": "untended",
}

## Fragment separator. A middot rather than a comma: the fragments are peers, not a list.
const DETAIL_PHRASE_SEPARATOR := " · "
## A coordinate arrives from the sim as `(64,36)` and is re-spaced to `(64, 36)`. Ray asked for the
## hex coordinates to stay; only their typography changes.
const DETAIL_COORDINATE_FORMAT := "(%d, %d)"
## `Warriors 3` — a NUMERIC value is meaningless without its key, where an enumerated one is
## meaningless with it (`Category Settle site`). That split is what makes the generic fallback
## readable for tokens no table names. The number itself goes through
## `EventDockPanel._trimmed_number` first, so the wire's `{:.3}` never reaches the row as `3.000`.
const DETAIL_NUMERIC_FORMAT := "%s %s"

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
