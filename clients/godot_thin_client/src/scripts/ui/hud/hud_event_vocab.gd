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
## A vocabulary leaf (`hud-modules.md`) — `const` throughout **except the three THEMED style tables
## and the one function that fills them**: a themed `HudStyle` colour is a `static var`, so a `const`
## table holding one is a parse error and a static-var initializer would freeze at whatever palette
## was loaded before the theme was installed. Everything else here is still a `const`, and it reads only
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
## player looking for a rules problem that does not exist. A command the sim REFUSES has already been
## written to the socket, and its refusal arrives later on the server's own event stream.
##
## ⛔ **AND IT IS NOT THE ONLY WAY A SEND CAN FAIL — THE COMMENT THAT SAID SO IS WHAT HID A BUG FOR A
## WHOLE SLICE.** It read *"`CommandClient.send_line` returns `ERR_CANT_ACQUIRE_RESOURCE` when there
## is no bridge and `ERR_CANT_CONNECT` when the bridge could not deliver, and those are the only two
## values it can produce"* — true of the codes, and false about their meanings, because the bridge
## PARSES a line before it sends it (`bridge/command.rs` → `parse_command_line`). A line the client
## itself could not build comes back as `ERR_CANT_CONNECT` too, and this message then blamed the
## network for it. Reported from play: the Builders pool's `+` emitted
## `assign_labor 0 1 builders 1`, the text grammar did not know the `builders` role, and the dock
## read *"Not connected to the server"* on a client that was connected — so the pool has never been
## staffable since `docs/plan_standing_upkeep.md` §2.5 introduced it, and no build queue could move.
##
## **SO THIS CONST NARROWED RATHER THAN WIDENED.** It fires for `ERR_CANT_ACQUIRE_RESOURCE` alone —
## there is no bridge, nothing was attempted — and `COMMAND_REFUSED_FORMAT` below carries the case
## where the bridge answered with a reason. Widening this one to cover both would have traded a wrong
## message for a vague one.
##
## **The underlying error code is NOT quoted**, because it carries nothing a player can act on and
## this site now has exactly one code; `CommandClient` `push_warning`s the bridge's own message for a
## developer, and the Logs console keeps this line verbatim.
const COMMAND_NOT_SENT_FORMAT := "Not connected to the server — \"%s\" was not sent."

## **THE BRIDGE ANSWERED, AND IT SAID NO** — the third outcome the comment above denied for a slice.
##
## **IT NAMES THE SIDE, WHICH IS THE WHOLE CORRECTION.** *"before it left the client"* is true of
## every failure `CommandClient` reports as `ERR_CANT_CONNECT`: a line the bridge could not PARSE, a
## dispatch that never reached the worker, a write the worker could not make, and a wait that timed
## out. None of them is the server refusing anything, and none of them is a dead network — which is
## what the player was being told.
##
## **AND IT CARRIES THE BRIDGE'S OWN REASON, because that reason names the token.** A parse error
## reads `unexpected token "builders"`, which is the one string in this whole path that says what is
## actually wrong; collapsing it to a category is what made the Builders defect unreadable. It is
## quoted verbatim rather than re-worded — a second phrasing of the parser's own answer could only
## drift from it.
const COMMAND_REFUSED_FORMAT := "Refused before it left the client — \"%s\": %s"

## What stands in when the bridge refused without saying why. It should be unreachable — the code
## that selects this message is the one `CommandClient` sets its reason alongside — so the wording
## admits the gap rather than inventing a cause, which is the mistake the whole pair is a repair for.
const COMMAND_REFUSED_UNKNOWN_REASON := "no reason given"

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
	# **A MATERIAL THE STANDING BILLS EAT FASTER THAN IT ARRIVES** (`docs/plan_standing_upkeep.md`
	# §4.9 item 12). Alert, and it NAMES THE BAND — this line is what replaced the faction `Gear`
	# row's `⚠ 1 band` → *which band* drill-down, and a faction-level warning that says something is
	# wrong and not where is exactly what that path existed to avoid. An investment is about to start
	# coming apart for want of a good, which is the Alert rung's own description
	# (*violence, an investment lost, the client's own faults*).
	#
	# **THE SIM EDGE-GATES IT**, so this rung never means "every turn": one line per band per material
	# on the crossing, not a level test.
	"material_shortfall": RUNG_ALERT,
	# **A KIT CROSSING A `life_readout` SEAM** — and its RUNG is the seam, not the kind, so this entry
	# is the FALLBACK for a line carrying no `severity=` token at all. `DETAIL_STATUS_STYLE` below
	# claims `severity=warn` (Notable) and `severity=danger` (Alert); the quieter of the two is the
	# honest default for an unlabelled one, because a kit wearing out is a thing to know about and
	# only the inner seam is a thing to stop for.
	"kit_life": RUNG_NOTABLE,
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
	# **A KIT WEARING OUT AND A GOOD RUNNING OUT ARE WORLD EVENTS**, on the channel that carries the
	# births and the raids — they happen to a BAND, in the world, on the sim's own turn, exactly as a
	# rung going feral does. They are stated rather than left to `DEFAULT_CHANNEL` for the reason the
	# comment above gives about `KIND_COMMAND_ECHO`: a channel is a fact about the kind, and these
	# two are the ones most easily mistaken for client chatter, since the remedy for both is
	# something the PLAYER does.
	"kit_life": DEFAULT_CHANNEL,
	"material_shortfall": DEFAULT_CHANNEL,
}

# ---- rung → glyph + accent -------------------------------------------------
## The accent a row wears when no kind- or detail-specific style claims it. `HudStyle` is the
## palette authority — there are no hexes here.
## **BUILT IN `apply_palette` BELOW, NOT HERE** — every entry carries a themed `HudStyle` colour, which
## is a `static var`, so a `const` table is a parse error and a static-var initializer would freeze at
## whatever palette was loaded before the theme was installed. Same for the two tables under it.
static var RUNG_STYLE := {}

## THREAT / CASUALTY kinds carry the SAME danger hue as the map overlay that draws them, so the bar
## accent and the map wash speak one danger language. Absorbed verbatim from the retired
## `CommandFeedController.KIND_STYLE` (Predators Phase 3). Consulted only for the kinds it names;
## every other kind takes its rung's style above.
static var KIND_STYLE := {}

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
## **A ROW THIS TABLE CLAIMS TAKES ITS `rung` FROM HERE, WHATEVER ITS KIND SAYS** — that is the whole
## point of it, and it is the only override above `RUNG_BY_KIND`. The rung rides in the STYLE entry
## rather than in a second table beside it, deliberately: two tables would be two memberships, and a
## token added to one and forgotten in the other is a row that wears the loss accent at the kind's own
## rung — i.e. looks perfectly right in a frame and is invisible at the player's floor.
##
## **THE FIVE TOKENS SPLIT ACROSS TWO RUNGS, and the ladder's own words decide which:**
##   • ALERT — `feral` and `lapsed`, both listed under Alert in §02. An investment is GONE: a rung has
##     reverted, or a labor row has been destroyed outright and taken its queued build with it.
##   • NOTABLE — `trimmed`, `pruned` and `stalled`, where the player has less than they set and
##     nothing is destroyed.
##     Routine is *"bracket transitions, and receipts for things the player asked for"*, and a crew
##     cut is the opposite of a receipt: the player asked for six and got three. Notable is *"the
##     world changed in a way worth knowing"*, which is exactly what a shrunken crew is. **Not
##     Alert** — this ladder's own calibration puts a DEATH at Notable (see `RUNG_BY_KIND`), and a
##     shed crew is a consequence of one.
##
## ⛔ **`stalled` IS THE THIRD NOTABLE TOKEN, AND NEITHER OF THE OTHER TWO WOULD HAVE BEEN TRUE**
## (`systems::labor::announce_shed_bench`). A short band thins the crafting BENCH, and on the last
## hand the job stops: `trimmed` says *the crew is smaller and the source is still worked*, and a
## bench at zero is not worked; `lapsed` says *the row is GONE and its investment with it*, and the
## bench keeps its recipe, its progress, its finished count and the materials it had already drawn —
## re-staffing resumes rather than restarts. So `lapsed` would be false AND would shout, on a state
## one command undoes.
##
## **A BENCH THAT STILL HAS HANDS ON IT IS A `trimmed`**, in that token's own terms, and reuses it —
## `kind=bench` rather than a fourth token. This one exists only for the state neither describes.
##
## **AND IT NEEDED A ROW HERE AT ALL BECAUSE `craft` IS NOT IN `RUNG_BY_KIND`**, so it takes
## `DEFAULT_RUNG` (`RUNG_ROUTINE`) — under the dock's own default floor. Without this entry a craft
## crew disappearing would announce itself to nobody, which is the exact defect the `trimmed` /
## `pruned` split was added to close, one web over.
##
## **AND THE TWO RUNGS ARE DRAWN APART** — `STATUS_SHED_GLYPH` against `STATUS_REDUCED_GLYPH`, below.
## For a release they were not, which left this split filter-only and unreadable on the line.
##
## **WITHOUT THE SPLIT BOTH NEW TOKENS WERE SILENT.** `trimmed` and `pruned` ride their VERB's kind
## (`forage` / `hunt` / `cultivate` / `corral`), every one of which is `RUNG_ROUTINE`, and the dock's
## `DEFAULT_DETAIL_LEVEL` is `RUNG_NOTABLE` — so a band going 6 → 3 announced itself to nobody on
## default settings, which is the exact defect the sim-side announcement was added to close.
##
## ⛔ **THE GLYPH TRACKS THE RUNG, AND THAT IS A RULE RATHER THAN A COINCIDENCE.** All four tokens wore
## `⚠` for a release, so the split above did real work in FILTERING and was invisible on the line — and
## a player reading two identically-drawn rows at two different rungs concludes the ladder is
## arbitrary. Reported from play as *"losing hunts and scouts is an alert but foragers are notable"*,
## which is not the rule at all: the rule is trimmed-vs-lapsed, and it only LOOKS like a kind split
## because a scout usually stands one or two hands and lapses on the first shed, where a forage row
## trims several times first.
##
## So the mark is the RUNG's, not the status's:
##   • `STATUS_SHED_GLYPH` (`⚠`) on the ALERT pair, and nowhere else in this table. It means *something
##     is wrong* everywhere else in this HUD, and it means that here too: the row is gone.
##   • `STATUS_REDUCED_GLYPH` (`▾`) on the NOTABLE set. A downward mark says *less than you set* —
##     fewer hands on the row, a narrower take from it, or a bench standing idle — which is exactly
##     what those three tokens report, and it is the difference the `⚠` was swallowing.
##
## **THE COLOUR DOES NOT MOVE.** Both pairs stay `HudStyle.WARN`: a trim is still unwelcome and still
## the player's to reverse, so demoting the amber would trade an over-loud row for an invisible one.
## The glyph is what carries the rung; the amber carries *this is not good news*.
##
## **NOTHING ELSE MOVES EITHER** — no rung, no `RUNG_ORDER`, no detail-level floor, no filtering. This
## is a render fix for a ladder that was already correct and could not be seen.
##
## **A FUTURE STATUS ADDED AT `RUNG_NOTABLE` WEARING `⚠` IS VISIBLY WRONG**, and that is the whole
## reason this is written down beside the table rather than left to be inferred from it — the two
## consts are named for their RUNGS, so the wrong pairing does not even read as a sentence.
##
## **THE TABLE IS NOT `status=`-ONLY, AND THE TWO `severity=` ROWS ARE WHY IT SAYS `key=value`**
## (`docs/plan_standing_upkeep.md` §4.9 item 12). A `kit_life` line's rung is the `life_readout` SEAM
## it crossed, which the sim writes as `severity=warn|danger` — resolved by
## `snapshot::crafting::life_severity` off `equipment.json`'s own `warn_fraction` 0.34 /
## `danger_fraction` 0.10, so ⛔ **no threshold is invented on this side.** The mechanism this table
## already is — a rung overridden by a whole space-delimited fragment — is exactly the job, so the
## split rides it rather than a parallel table beside it.
##
## **THE GLYPH TRACKS THE RUNG THERE TOO**: `⚠` on `severity=danger`, `▾` on `severity=warn`. A kit at
## a tenth of its life is an investment about to be lost; one at a third is *less than you had*, which
## is precisely what the reduced mark means everywhere else here.
##
## **`status=outrunning` AGREES WITH ITS KIND RATHER THAN OVERRIDING IT** — `material_shortfall` is
## already Alert. It is listed for the two things only a member of this table gets: the `⚠` its rung
## names, and eligibility for `DETAIL_STATUS_WORK_LINK`'s band jump, which `_detail_status_key`
## returns no token for otherwise. **Warn amber, not the raid crimson**, exactly as `feral` and
## `lapsed`: a loss the player can still head off by crafting or by holding less, not an attack.
##
## **BUILT IN `apply_palette` BELOW, NOT HERE** — every entry carries a themed `HudStyle` colour,
## which is a `static var`, so a `const` table is a parse error. The `rung` beside the colour is NOT
## themed and does not change with the palette; it rides in the same entry because a row's rung and
## its accent are one membership.
static var DETAIL_STATUS_STYLE := {}

## **THE ALERT PAIR'S MARK** — `feral` and `lapsed`, an investment GONE. The same `⚠` the ladder's own
## `RUNG_ALERT` style wears and the same one every hazard in this HUD wears, which is exactly why it
## is worth keeping exclusive: a mark that also appears on a routine crew cut means nothing anywhere.
const STATUS_SHED_GLYPH := "⚠"

## **THE NOTABLE PAIR'S MARK** — `trimmed` and `pruned`, the source still worked and by less than the
## player set. `▾` rather than a second hazard: it points DOWN, which is the whole content of both
## tokens, and it reads as a quantity changing rather than as a fault.
##
## **IT IS ONE MARK FOR ALL THREE, BECAUSE THEY ARE ONE CLASS.** A trim cuts the hands, a prune
## narrows what the hands still standing there take, and a stall leaves a bench with none — three
## mechanisms, one sentence to the player (*you asked for more than you are getting*), one rung, and
## therefore one mark. A per-status pictogram would put the glyph back to tracking the MECHANISM,
## which is the thing this rule exists to stop, and `stalled` is the first token added since the rule
## was written — it takes the mark its RUNG names and nothing else is decided about it.
const STATUS_REDUCED_GLYPH := "▾"

## Install the current `HudStyle` palette into the three style tables above and the turn stamp's ink.
## Called by `HudPalette.apply()` after `HudStyle.apply_palette`; takes no palette of its own, because
## every value here is a HUD colour this module only re-states in an event's vocabulary.
static func apply_palette() -> void:
	RUNG_STYLE = {
		RUNG_ALERT: {"glyph": "⚠", "color": HudStyle.DANGER},
		RUNG_NOTABLE: {"glyph": "✦", "color": HudStyle.SIGNAL},
		RUNG_ROUTINE: {"glyph": "◦", "color": HudStyle.INK_FAINT},
	}
	KIND_STYLE = {
		"predator_raid": {"glyph": "⚔", "color": HudStyle.THREAT_ACCENT},
		"hunt_danger": {"glyph": "⚠", "color": HudStyle.HUNT_DANGER_ACCENT},
	}
	DETAIL_STATUS_STYLE = {
		"status=feral": {"glyph": STATUS_SHED_GLYPH, "color": HudStyle.WARN, "rung": RUNG_ALERT},
		"status=lapsed": {"glyph": STATUS_SHED_GLYPH, "color": HudStyle.WARN, "rung": RUNG_ALERT},
		"status=trimmed": {"glyph": STATUS_REDUCED_GLYPH, "color": HudStyle.WARN, "rung": RUNG_NOTABLE},
		"status=pruned": {"glyph": STATUS_REDUCED_GLYPH, "color": HudStyle.WARN, "rung": RUNG_NOTABLE},
		"status=stalled": {"glyph": STATUS_REDUCED_GLYPH, "color": HudStyle.WARN, "rung": RUNG_NOTABLE},
		"severity=warn": {"glyph": STATUS_REDUCED_GLYPH, "color": HudStyle.WARN,
			"rung": RUNG_NOTABLE},
		"severity=danger": {"glyph": STATUS_SHED_GLYPH, "color": HudStyle.WARN,
			"rung": RUNG_ALERT},

		"status=outrunning": {"glyph": STATUS_SHED_GLYPH, "color": HudStyle.WARN,
			"rung": RUNG_ALERT},
	}
	TURN_STAMP_COLOR = HudStyle.INK_DIM

## **THE STATUS TOKENS WHOSE ROW OFFERS THE WORK TAB** — a labor row the sim changed without being
## asked, so the player is owed a way to go and look at what is left of it. `EventDockPanel` renders
## `WORK_TAB_LINK_TEXT` on such a row and emits `band_work_tab_requested`.
##
## **A SUBSET OF `DETAIL_STATUS_STYLE`, NOT A COPY OF IT.** `feral` is a SOURCE reverting — nothing on
## the band's work board changed, so the Work tab answers nothing there. The four here are the ones
## where a crew is smaller or gone than the player left it: `trimmed` cut it, `lapsed` destroyed the
## row, `pruned` narrowed what the crew still standing there takes, and `stalled` took the last hand
## off the crafting bench.
##
## **`stalled` EARNS ITS ROW ON THE SAME TEST THE OTHER THREE PASS**: the sim changed a labor row
## without being asked, so the player is owed a way to go and look at what is left of it — and the
## bench's crew is staffed from the Work tab like any other. `announce_shed_bench` writes `band=`, so
## the link has the one token it needs.
##
## ⛔ **THE LINK ONLY APPEARS WHERE THE DETAIL CARRIES A `band=` TOKEN** (`DETAIL_BAND_KEY`), because
## a jump has to name a band and the client will not recover one by reading the label's prose.
## `systems::labor::announce_shed_crew`, the three lapse sites beside it and the server's own
## `status=pruned` line all write it, as the band's **durable `BandId`** — the handle this panel
## joins the roster on, and deliberately not the ECS entity, which is the same `u64` and is what
## `command_guard` exists because someone once sent instead.
##
## **A cohort carrying no durable id still emits its line and simply renders linkless**, which is the
## demographic feed's own rule rather than a fabricated `band=0`.
const DETAIL_STATUS_WORK_LINK := {
	"status=trimmed": true,
	"status=lapsed": true,
	"status=pruned": true,
	"status=stalled": true,
	# **`status=outrunning` IS HOW THE MATERIAL ALERT NAMES ITS BAND**
	# (`docs/plan_standing_upkeep.md` §4.9 item 12), and without it the line names none. The sim's
	# label is *"Hurdles is running out"* — no band in it — so `SIM_BAND_LABEL_FORMAT` has nothing to
	# rewrite, and `band` sits in `DETAIL_KEY_HIDDEN` on the premise that the label already said it.
	# The link is what closes that gap, and closing it is the whole reason this kind exists: it
	# replaces the faction `Gear` row's `⚠ 1 band` → *which band* drill-down, which was a jump.
	#
	# **AND THE WORK TAB IS THE RIGHT DESTINATION, not merely an available one.** The bill is what
	# this band's improvements demand, and what the player can DO about it is on that tab: staff the
	# bench that makes the good, or stop holding a rung. `announce_material_shortfall` writes `band=`
	# as the durable `BandId`, so the link has the one token it needs.
	"status=outrunning": true,
}

## The link's own words. Deliberately the same string as `HudComposeVocab.WORK_TAB_LINK_TEXT` and
## deliberately a SEPARATE const: the compose sheet's copy is a word inside a sentence, this one is a
## control at the end of a row, and this file is a cycle-free leaf that reads only `HudStyle`.
const WORK_TAB_LINK_TEXT := "Work tab"

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
	# Lower-case with their neighbours, and for their reason: the phrase continues the label
	# ("foragers at (60, 0) cut to 3 — too few workers" · "trimmed"), it does not head a column.
	"trimmed": "trimmed",
	"pruned": "pruned",
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
static var TURN_STAMP_COLOR: Color = Color()   # DERIVED in `apply_palette`
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
