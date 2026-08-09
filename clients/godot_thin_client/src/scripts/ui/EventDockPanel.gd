extends CanvasLayer
class_name EventDockPanel

## THE EVENT DOCK (issue #272, `docs/event_dock_ux_proposal.html`) — the horizontal notification bar
## that replaces the hidden left-dock command feed and the Inspector's `[SIM]` command stream.
##
## A CanvasLayer that renders a thin strip against the TOP or BOTTOM screen edge and **OVERLAYS the
## map** — it reserves nothing from either surface. **Top/bottom only**: a row of events needs width,
## and a vertical strip has none.
##
## **IT IS THE ONE HUD ELEMENT THAT FLOATS OVER LIVE MAP, and that is deliberate.** It reserved its
## edge at first, like `BandCityPanel` — but a reservation is FULL WIDTH while this strip is bounded
## to the centre band and capped at `MAX_STRIP_WIDTH`, so the map was pushed down across the whole
## window to make room for something that only filled the middle of it. The ends came back as bare
## background: black bars either side of the bar, reported from live play. A notification strip is
## not furniture the game area has to make room for; it floats.
##
## Two consequences of overlaying, both handled and neither obvious:
##   • **It must consume its own clicks.** With no reservation, `MapView`'s hit-testing covers the
##     whole viewport again, so a click on the bar would otherwise ALSO select the hex beneath it.
##     `MapView` reads `_unhandled_input`, so a `MOUSE_FILTER_STOP` control over the pointer consumes
##     the press before it gets there — see `_build`.
##   • **It must be opaque.** Reserved chrome sits on background; an overlay sits on terrain, which
##     can be snow or desert. The card fills with `HudStyle.PANEL_SOLID` (alpha 1.0), so brightness
##     underneath cannot reach the text.
##
## TWO STATES, and only two:
##   • COLLAPSED — the bar. The `recent_count` most recent events that pass the detail floor,
##     NEWEST FIRST, with one exception: an unread Alert pins to the leading slot and holds it until
##     the log is opened. You get chronology and you never lose the raid.
##   • EXPANDED — the turn-grouped log, with World/System channel chips, the detail floor, the row
##     count, the dock edge, and an "Earlier turns" walk-back.
##
## **THE BAR IS STRICTLY THE COLLAPSED STATE.** Opening the log replaces the rows with a one-line
## title rather than keeping them: at four rows the bar printed the log's own newest turn-group a
## second time, directly beneath it — the same four events twice, six inches apart. The prototype
## made that unmissable and it must not come back.
##
## **THE STRIP YIELDS TO THE MAP.** `_cross_axis_size()` is clamped to `MAX_STRIP_HEIGHT_FRACTION`
## of the window and the log scrolls INTERNALLY past it — the same bound `DockScrollFit` put on the
## command feed, for the same reason: a log that can eat the viewport is not a notification.
##
## **THE RESERVATION NEVER DEPENDS ON CONTENT** (the `BandCityPanel` rule, learned as a map flicker
## on every `+` press): the bar reserves `recent_count` rows whether or not it has that many events,
## so only a preference change, an expand/collapse, a dock change or a viewport resize moves it.
##
## The importance model — which kind is an Alert, which channel it rides, what glyph and accent it
## wears, and what each detail floor admits — lives in `hud/hud_event_vocab.gd`, not here.

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

# ---- geometry (canvas-space px) --------------------------------------------
## Render above the map and the HUD, and above the Band/City panel (103): on a shared edge the bar
## hugs the screen edge and the log opens into space the band panel is being pushed out of, so it
## must not be drawn under it during the frame the reflow lands.
const LAYER_INDEX := 104
## One event row. Sized for `ROW_FONT_SIZE` text plus `ROW_PADDING_V` either side, with headroom, so
## the reserved strip is never a pixel short of what the rows actually draw at.
const ROW_HEIGHT := 28.0
const ROW_PADDING_H := 8
const ROW_PADDING_V := 3
## The rung accent rail down a row's leading edge (the prototype's `border-left`). A Routine row
## draws it transparent rather than omitting it, so every row's text starts at the same x.
const ROW_ACCENT_WIDTH := 2
const ROW_SEPARATION := 0
const ROW_ITEM_SEPARATION := 9
const GLYPH_COLUMN_WIDTH := 14.0
## How strongly a pinned alert's row is washed with its own accent — enough to read as held, far
## short of competing with the rail and the glyph that already carry the rung.
const PINNED_WASH_ALPHA := 0.10
## Card inner padding + border. Named because `BAR_CHROME_V` is the exact vertical cost the
## reservation maths has to add on top of the rows.
const PANEL_CONTENT_MARGIN_H := 10
const PANEL_CONTENT_MARGIN_V := 4
const PANEL_BORDER_WIDTH := 1
const BAR_CHROME_V := 2.0 * (float(PANEL_CONTENT_MARGIN_V) + float(PANEL_BORDER_WIDTH))
## Accent seam thickness on the strip's map-facing edge (the `BandCityPanel` treatment).
const SEAM_THICKNESS := 2.0
## The expanded log's target height. The strip as a whole is still clamped by
## `MAX_STRIP_HEIGHT_FRACTION`, and the log's scroll takes whatever is left after its head and foot,
## so this is a target rather than a floor.
const LOG_HEIGHT := 260.0
## **THE WIDEST THE STRIP MAY BE, CENTRED IN THE BAND BETWEEN THE VERTICAL DOCKS.** On an ultrawide
## the bar spanned the whole band, so a row's label sat at one end of two feet of screen and its
## detail at the other, and the pair read as two unrelated things rather than one line. Bounded, they
## read as a column.
##
## **CHOSEN AGAINST TWO MEASUREMENTS, and the larger one wins.** The widest row the shipped fixtures
## produce at the current font sizes is **537px** (a predator raid: `Grey wolves took two from
## Ashfoot` + `Wounded 1 · Warriors 3 · Grey Wolf`), which with the expander (86) and the card chrome
## (31) needs **654**. But the band between the two HUD columns at the project's own 1920 base
## canvas is **1216**, and the ordinary case must render UNCHANGED — a cap below that would shrink the
## bar on every desktop to fix a complaint about ultrawides. So the cap sits just above the base-canvas
## band: no content is ever squeezed (654 ≪ 1280), nothing moves at 1920 or below, and past it — which
## is exactly the ultrawide the report came from — the strip stops growing and centres instead.
##
## Cross-axis only, the same rule as the perpendicular insets: it moves where the strip is DRAWN,
## never what it reserves.
const MAX_STRIP_WIDTH := 1280.0
## **THE NARROWEST THE STRIP MAY BE — the point below which the CARD no longer fits inside it.**
##
## Nothing bounded the strip from below at all. The strip takes whatever the perpendicular insets
## leave, and those are three FIXED logical widths — a docked
## panel's reservation plus the HUD's two authored columns (360 + 344) — so the band collapses as the
## logical viewport shrinks. Measured with the Band panel docked LEFT at `ui_scale` 1.35: a 1422px
## viewport, insets 740 / 344, **a 338px band** — against a card whose own combined minimum is 406, so
## the card drew 68px OUTSIDE the strip it was given while `EventRows` (`clip_contents`) silently cut
## its labels. Reachable at `ui_scale` 1.0 in a window near 1200px wide, for the same arithmetic.
##
## **IT IS THE CARD'S OWN COMBINED MINIMUM, NOT THE WIDEST SHIPPED ROW.** The failure this floor exists
## to prevent is the CARD drawing outside the strip it was given; the widest row is a comfort figure,
## and floored at it the strip was clamped UP through the whole `[card minimum, 654)` band and centred,
## so it drew over the HUD dock columns `Main._update_event_dock_insets` exists to keep it off. Worked:
## a 1200-px logical viewport with nothing docked leaves `1200 − 360 − 344` = **496**, and a 654 strip
## in a 496 band hangs **79px over each column**. In that band the pre-floor behaviour was correct — the
## strip fitted inside its insets AND cleared the card's minimum, and the only cost was `clip_contents`
## trimming the longest label. **Clipping a long label is acceptable; overhanging the columns is not.**
##
## Measured off the live card (`_panel.get_combined_minimum_size().x`, the harness's own probe), and
## re-derivable from its parts rather than trusted: the bar's minimum — one row's own **298** plus the
## expander column's **89** (the button's 80 and the bar's `ROW_ITEM_SEPARATION`) — under the card's
## **20** of chrome. The live report of the overhang quoted **406** for the same card; the pixel is the
## row set it was measured against, not a disagreement about the rule.
##
## **654 IS STILL THE COMFORTABLE WIDTH and the cap's derivation above still names it** — the widest
## shipped row (537) plus the expander and chrome as measured there. What changed is that it is no
## longer a FLOOR: a band between the two draws the strip at the band, with the longest rows clipping
## exactly as they already do above the floor.
##
## **BELOW THE CARD'S MINIMUM the strip still overhangs its insets rather than shrinking further**,
## symmetrically about the band it was offered and then clamped inside the viewport. That is the one
## deliberate trade left: under it there is no arrangement in which the bar both clears every HUD column
## and draws a row at all, and a bar with no room for a row has stopped being a notification.
## The bar's own combined minimum: one event ROW (298 — its glyph column, turn stamp, padding and the
## one-pixel floor `clip_text` leaves the labels) beside the expander COLUMN (89 — the button's 80 and
## one `ROW_ITEM_SEPARATION`).
const CARD_BAR_MIN_WIDTH := 387.0
## What the card adds around that bar: the `PanelContainer`'s stylebox margins and the column's own.
const CARD_CHROME_WIDTH := 20.0
const MIN_STRIP_WIDTH := CARD_BAR_MIN_WIDTH + CARD_CHROME_WIDTH
## The strip may never cover more than this share of the window. It no longer RESERVES anything, so
## this is not about leaving the map room to lay out in — it is about how much live map the overlay
## is allowed to hide. The map is the game; a notification bar that buries it has stopped being a
## notification.
const MAX_STRIP_HEIGHT_FRACTION := 0.5
const LOG_SECTION_SEPARATION := 6
const LOG_HEAD_SEPARATION := 8
const LOG_TURN_GROUP_SEPARATION := 0
const LOG_TURN_HEAD_PADDING_H := 8
const LOG_TURN_HEAD_PADDING_V := 2

# ---- typography ------------------------------------------------------------
## Bumped as a set (the bar and the log share every one of these), with `DETAIL_FONT_SIZE` bumped
## the MOST — it was both the smallest and the one singled out as unreadable, and it sits at the far
## end of the row where it gets the least attention. `ROW_HEIGHT` rose with them: it is what the
## reserved strip is computed from, so leaving it behind would end the strip a few pixels short of
## what the rows actually draw.
const ROW_FONT_SIZE := 14
const GLYPH_FONT_SIZE := 14
const TURN_FONT_SIZE := 12
const DETAIL_FONT_SIZE := 13
const CONTROL_FONT_SIZE := 12
const TURN_HEAD_FONT_SIZE := 11
const CHIP_FONT_SIZE := 11

# ---- the bar's row budget --------------------------------------------------
## How many rows the bar may show. Two rows is ~50px of reserved edge; three is defensible; four
## starts to read as a panel rather than as chrome.
const RECENT_COUNT_MIN := 1
const RECENT_COUNT_MAX := 4
const DEFAULT_RECENT_COUNT := 2
## The row-count control's faces. Derived from the bounds so the control and the clamp agree.
const RECENT_COUNT_CHOICES: Array[int] = [1, 2, 3, 4]

# ---- retention -------------------------------------------------------------
## How many TURNS the client keeps until a snapshot says otherwise. **The sim is the authority** —
## `command_events_retention_turns` overrides this — but a client that has not heard yet must still
## bound its own accumulation, and this matches the sim's shipped default.
const DEFAULT_RETENTION_TURNS := 20
## How many turns the expanded log shows before "Earlier turns" extends it, and by how much.
const LOG_WINDOW_TURNS := 5

# ---- persistence -----------------------------------------------------------
## A new section in the file the voice register, the Telling's collapsed state and `[hud_panels]`
## already use. There are two prefs files already and the panel rules say not to add a third.
const CONFIG_SECTION := "events"
const CONFIG_KEY_EDGE := "edge"
const CONFIG_KEY_RECENT_COUNT := "recent_count"
const CONFIG_KEY_DETAIL_LEVEL := "detail_level"
const CONFIG_KEY_CHANNELS := "channels"
const CONFIG_KEY_SUPPRESSED := "suppressed"
## The retired command feed's own hidden/shown key, in the section `Hud` still owns. Read ONCE, so a
## player who had opened the feed lands on the bar that replaces it open too; then erased, so the
## migration can never re-fire over a later choice made here.
const LEGACY_PANELS_SECTION := "hud_panels"
const LEGACY_KEY_COMMAND_FEED_SUPPRESSED := "command_feed_suppressed"
## The bar ships VISIBLE — unlike the feed it replaces, which shipped hidden because it was six
## read-only receipts in a dock column that had the verbs in it. A notification the player has to go
## and find is the thing this whole arc exists to remove.
const DEFAULT_SUPPRESSED := false
const DEFAULT_EDGE := SIDE_BOTTOM
## Preview harnesses point this at a scratch file so a render can neither READ nor WRITE the
## player's real prefs. Unset, it falls through to `NarrativeForkPanel.config_path()` — so a harness
## that has already isolated THAT file (every one of them has) gets this section isolated with it.
static var config_path_override: String = ""

## Top and bottom only.
const DOCK_EDGES: Array[int] = [SIDE_TOP, SIDE_BOTTOM]

## "No occupancy has been published yet" — a negative depth, which no strip can measure, so the
## first `_apply_dock_layout` always emits even if the strip happens to be suppressed at zero.
const OCCUPANCY_UNPUBLISHED := -1.0

# ---- signals ---------------------------------------------------------------
## The strip moved to the other horizontal edge (the log's dock chips are the only way this
## happens). **This is NOT `reservation_changed` and must never become it** — the dock still reserves
## nothing from either surface. It exists because the strip is DISPLACED by whatever reserves the
## edge it just landed on, so `Main` has to recompute `set_edge_offset` for the new edge; without it
## a dock chip press moves the bar onto the band panel's edge and straight over the top of it.
signal dock_changed(edge: int)

## **HOW DEEP THE STRIP IS DRAWN INTO ITS EDGE — a claim about PIXELS COVERED, not about space
## taken.** Still not `reservation_changed` and still not a step towards becoming one: the map and
## the HUD go on laying out underneath the bar exactly as they always have, and nothing here enters
## `Main._reservations`. What this exists for is the one surface that cannot simply be drawn under —
## a FREE-FLOATING card, which is placed by arithmetic rather than by a container and so has to be
## told which band of the window is already spoken for (`panel-framework.md` → `room_bounds`).
## Emitted from `_apply_dock_layout`, the single choke point every input to the strip's geometry
## already runs through: the edge, the displacement, the row count, expanding, suppressing and the
## viewport's own resize.
signal occupancy_changed(edge: int, extent: float)

# ---- state -----------------------------------------------------------------
var _dock_edge: int = DEFAULT_EDGE
var _suppressed: bool = DEFAULT_SUPPRESSED
var _expanded: bool = false
var _recent_count: int = DEFAULT_RECENT_COUNT
var _detail_level: String = HudEventVocab.DEFAULT_DETAIL_LEVEL
var _channels: Dictionary = {}
var _log_window_turns: int = LOG_WINDOW_TURNS
## How far the strip is pulled in from the LEFT and RIGHT screen edges — the current totals of every
## OTHER reserver on those two edges, pushed by `Main` (`set_perpendicular_insets`). See that method
## for why the bar does not span the raw window.
var _inset_left: float = 0.0
var _inset_right: float = 0.0
## How far the strip is pushed INBOARD from its own docked edge — the current total of every
## reserver sitting on that edge, pushed by `Main` (`set_edge_offset`). The band panel keeps the
## screen edge; the bar sits just past it. See that method.
var _edge_offset: float = 0.0
## The last `(edge, extent)` handed to `occupancy_changed`, so the signal fires on a CHANGE rather
## than on every layout pass. `_apply_dock_layout` runs on a viewport resize and on every preference
## touch, and each emission re-fits whatever free-floating card is listening.
## `OCCUPANCY_UNPUBLISHED` is a depth no strip can have, so the first pass always emits.
var _published_edge: int = DEFAULT_EDGE
var _published_extent: float = OCCUPANCY_UNPUBLISHED
## An unread Alert pins to the leading slot. Opening the log is what marks alerts read — the pin
## exists to survive until the player has actually had a chance to look.
var _alert_seen: bool = true
## The `order` of the row currently wearing the pin, or -1. An `order` rather than the record itself:
## two events can carry equal field values (that is the bug `seq` de-duplication fixes), so identity
## has to be an id, never a `==` on the dictionary.
var _pinned_order: int = -1

## Normalized event records in ARRIVAL order. Each is
## `{tick, kind, faction, label, detail, seq, rung, channel, glyph, color, order}`.
var _events: Array[Dictionary] = []
## `seq` → true. **THE DE-DUPLICATION KEY IS `seq`, NOT A SYNTHESIZED SIGNATURE.** Every consumer
## used to key on `"%d|%s|%s|%s" % [tick, kind, label, detail]`, which collapses two IDENTICAL
## events in one turn into one — two wolf packs raiding the same band on the same turn were reported
## as a single raid. `seq` is monotonic per event, so it cannot.
##
## **`seq` IS ONE-BASED AND `0` IS A SENTINEL** meaning "this row never went through
## `CommandEventLog::push`" (it is also the FlatBuffers default for an absent field). Treating 0 as a
## real key would collide every such row onto one, so `_ingest_row_is_new` routes them to the
## signature fallback instead.
var _seen_seq: Dictionary = {}
## The fallback for a row carrying no usable `seq` — the old signature, kept ONLY so a mixed frame
## cannot crash or duplicate every row every turn. A row with a real `seq` never touches this.
var _seen_signature: Dictionary = {}
var _ingest_order: int = 0
var _retention_turns: int = DEFAULT_RETENTION_TURNS
## The newest turn the client has seen — what a client-side note is stamped with, and what the
## retention window is measured back from.
var _current_turn: int = -1
## `band_id` (as a String) → the name the CLIENT gives that band. Pushed from `Hud` each snapshot
## via `Main` (`band_labels_changed`). The sim names a band by its durable `BandId`; the client names
## it by its ROSTER POSITION, and the two routinely disagree — so a demographic event's label is
## re-written at RENDER time from the `band=` token rather than stamped at ingest, which also means a
## roster change relabels the rows already held.
var _band_labels: Dictionary = {}

# ---- nodes -----------------------------------------------------------------
var _root: Control
var _panel: PanelContainer
var _seam: ColorRect
var _column: VBoxContainer
var _bar: HBoxContainer
var _rows: VBoxContainer
var _more_button: Button
var _log: VBoxContainer
var _log_head: HFlowContainer
var _log_scroll: ScrollContainer
var _log_body: VBoxContainer
var _log_foot: HBoxContainer
var _earlier_button: Button
var _retention_label: Label
var _channel_chips: Dictionary = {}    # channel:String -> Button
var _detail_chips: Dictionary = {}     # rung:String    -> Button
var _count_chips: Dictionary = {}      # count:int      -> Button
var _dock_chips: Dictionary = {}       # edge:int       -> Button

func _ready() -> void:
	layer = LAYER_INDEX
	_load_prefs()
	_build()
	_apply_dock_layout()
	_render()
	var vp := get_viewport()
	if vp != null:
		vp.size_changed.connect(_on_viewport_resized)

func _on_viewport_resized() -> void:
	_apply_dock_layout()

# ---- ingest ----------------------------------------------------------------

## Merge one batch of snapshot `command_events` dicts (`{tick, kind, faction, label, detail, seq}`).
##
## **THIS IS PER-FRAME HISTORY, NEVER RECONSTRUCTIBLE** — a delta carries only the rows appended
## since the baseline, a full snapshot carries the whole retained ring — so the dock ACCUMULATES and
## de-duplicates rather than replacing. Re-ingesting a full snapshot's ring is therefore harmless,
## and is the backfill for a player who connected mid-session.
##
## NARRATIVE KINDS ARE SKIPPED — they belong to `TellingPanel` (a receipt and a telling want opposite
## retention and density). That test lives THERE, in `handles_kind()`, so a kind can never be claimed
## by both surfaces or dropped by both.
##
## IGNORED KINDS ARE DROPPED HERE TOO, and the position in the loop is the contract — see
## `HudEventVocab.IGNORED_KINDS`.
func ingest_events(events_variant: Variant) -> void:
	if not (events_variant is Array):
		return
	var events_array: Array = events_variant
	var appended := false
	for entry_variant in events_array:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var kind: String = String(entry.get("kind", "")).strip_edges()
		if TellingPanel.handles_kind(kind):
			continue
		# BEFORE the de-duplication below, deliberately: an ignored kind must occupy neither a `seq`
		# slot nor a signature. Filtering after would leave the dock suppressing the first re-ingest
		# of every row it had already dropped, the moment a kind stopped being ignored.
		if HudEventVocab.IGNORED_KINDS.has(kind):
			continue
		var tick: int = int(entry.get("tick", -1))
		var label: String = String(entry.get("label", "")).strip_edges()
		var detail: String = String(entry.get("detail", "")).strip_edges()
		# ONE-BASED: 0 means "unsequenced" (see `_seen_seq`), so it takes the signature path.
		var seq: int = int(entry.get("seq", 0))
		if seq > 0:
			if _seen_seq.has(seq):
				continue
			_seen_seq[seq] = true
		else:
			# No `seq` on this row: fall back to the old signature, so a mixed frame degrades to the
			# previous (lossy) behaviour rather than duplicating every row every frame.
			var signature := "%d|%s|%s|%s" % [tick, kind, label, detail]
			if _seen_signature.has(signature):
				continue
			_seen_signature[signature] = true
		if tick > _current_turn:
			_current_turn = tick
		_append(tick, kind, int(entry.get("faction", -1)), label, detail, seq, "")
		appended = true
	if appended:
		_prune()
		_render()

## Push a CLIENT-SIDE system note — socket state, a rejected command, a rollback. It rides the
## System channel under `HudEventVocab.KIND_SYSTEM`, because a dropped command socket is exactly the
## kind of thing a player must be told, and the Inspector's debug console is not where they see it.
##
## `alert` is the CALLER's severity: a lost socket is an Alert, "connected" is a receipt. Passed
## rather than derived, because the client knows which of its own lines is bad news and a string
## match on its own log text would only pretend to.
##
## `kind` defaults to `KIND_SYSTEM`, so every caller that says nothing keeps saying "this is a fault
## or a state change". The one path that opts out is the command ACKNOWLEDGEMENT, which passes
## `KIND_COMMAND_ECHO` and is dropped here — **the second of the dock's two inlets, and a filter that
## covered only `ingest_events` would be a trap**, since every client-side line arrives through this
## one instead.
func note_system(label: String, detail: String = "", alert: bool = false,
		kind: String = HudEventVocab.KIND_SYSTEM) -> void:
	if HudEventVocab.IGNORED_KINDS.has(kind):
		return
	var rung: String = HudEventVocab.RUNG_ALERT if alert else HudEventVocab.RUNG_ROUTINE
	_append(_current_turn, kind, -1, label, detail, -1, rung)
	_prune()
	_render()

## The sim's authority on how far back the client should keep events. Ignored when absent or
## non-positive — the client's own default still has to bound the accumulation.
func set_retention_turns(turns: int) -> void:
	if turns <= 0 or turns == _retention_turns:
		return
	_retention_turns = turns
	_prune()
	_render()

## The turn a client-side note is stamped with, pushed each snapshot so a system event groups with
## the sim events of the turn it happened on.
func set_current_turn(turn: int) -> void:
	if turn > _current_turn:
		_current_turn = turn

## `band_id` → the client's own name for that band, pushed from the HUD's roster each snapshot.
## The sim writes a positional `Band <BandId>` into a demographic event's label because the snapshot
## carries no band NAME; this is what lets the dock say what every other HUD surface says instead.
func set_band_labels(labels: Dictionary) -> void:
	if labels == _band_labels:
		return
	_band_labels = labels.duplicate()
	_render()

## Drop the accumulated log AND the `seq` de-duplication set.
##
## **CALLED ON EVERY FULL SNAPSHOT, BEFORE THAT SNAPSHOT'S EVENTS ARE INGESTED, and that is a
## CORRECTNESS requirement rather than hygiene.** `CommandEventLog` is checkpoint state, so a
## rollback restores it *including* its `next_seq` counter — the replayed events therefore REUSE
## sequence numbers the client has already seen. A rollback publishes a full frame, so without this
## the dock would silently suppress every replayed row as a duplicate `seq` and go on showing a
## plausible but stale log, with nothing anywhere reporting a fault.
##
## A full snapshot carries the whole retained ring, so nothing is lost by clearing first; the client's
## own System-channel notes are the one thing that is, and a full frame is rare (first frame, resync,
## `new_game`, rollback) enough for that to be the right trade against a silently wrong log.
##
## For the same reason the retention trim keys on `tick`, never on `seq` arithmetic: `seq` is not
## monotonic across a rollback.
func reset() -> void:
	_events.clear()
	_seen_seq.clear()
	_seen_signature.clear()
	_ingest_order = 0
	_current_turn = -1
	_alert_seen = true
	_pinned_order = -1
	_log_window_turns = LOG_WINDOW_TURNS
	_render()

func _append(tick: int, kind: String, faction: int, label: String, detail: String,
		seq: int, rung_override: String) -> void:
	var rung := rung_override if rung_override != "" else _resolve_rung(kind, detail)
	var style := _resolve_style(kind, detail, rung)
	_events.append({
		"tick": tick,
		"kind": kind,
		"faction": faction,
		"label": label if label != "" else kind.capitalize(),
		"detail": detail,
		"seq": seq,
		"rung": rung,
		"channel": String(HudEventVocab.CHANNEL_BY_KIND.get(kind, HudEventVocab.DEFAULT_CHANNEL)),
		"glyph": String(style["glyph"]),
		"color": style["color"] as Color,
		"order": _ingest_order,
	})
	_ingest_order += 1
	# A NEW alert re-arms the pin: "unread" is a property of the alert, not of the dock.
	if rung == HudEventVocab.RUNG_ALERT:
		_alert_seen = false

## The rung a kind sits on, PROMOTED to Alert by a `status=` token when the sim's detail says an
## investment was lost. The token test is what separates a rung going feral from the completion that
## preceded it on the same kind — see `HudEventVocab.DETAIL_STATUS_STYLE`.
func _resolve_rung(kind: String, detail: String) -> String:
	if _detail_status_key(detail) != "":
		return HudEventVocab.RUNG_ALERT
	return String(HudEventVocab.RUNG_BY_KIND.get(kind, HudEventVocab.DEFAULT_RUNG))

## Glyph + accent, most specific first: the kind's own threat style, then the detail's `status=`
## style, then the rung's default.
func _resolve_style(kind: String, detail: String, rung: String) -> Dictionary:
	if HudEventVocab.KIND_STYLE.has(kind):
		return HudEventVocab.KIND_STYLE[kind]
	var token := _detail_status_key(detail)
	if token != "":
		return HudEventVocab.DETAIL_STATUS_STYLE[token]
	return HudEventVocab.RUNG_STYLE.get(rung, HudEventVocab.RUNG_STYLE[HudEventVocab.DEFAULT_RUNG])

## **THE SIM'S DETAIL, RENDERED AS PROSE.** `category=settle_site at (64,36)` becomes
## `Settle site · (64, 36)`; `band=3 count=4 direction=out` becomes `departed`.
##
## The wire tokens are a MACHINE CONTRACT — they exist so the client can join a `band=` id to its
## roster name and promote a `status=` row to Alert — and printing them was the defect: an internal
## identifier reached a player-facing bar. **The RAW string stays the input to `_detail_status_key`**,
## which matches whole `key=value` fragments and must never start matching prose.
##
## The walk, and why each branch exists:
##   • A token with `=` opens a fragment. Its KEY is dropped when `DETAIL_KEY_HIDDEN` names it (the
##     label already said it), otherwise the VALUE is rendered and the key kept only when the value
##     is NUMERIC — `Warriors 3` needs its key, `Category Settle site` is worse for having one.
##   • A `(x,y)` token is a coordinate, re-spaced. Ray asked for the hex coordinates to stay.
##   • A bare word is either grammar (`at`, dropped — the ` · ` join does that work) or the
##     CONTINUATION of the previous value. That second case is load-bearing and not hypothetical:
##     the sim writes `species=Grey Wolf`, so a naive space-split loses the `Wolf`.
##
## **THE GENERIC FALLBACK IS THE POINT.** A kind or a token can be added server-side with no schema
## change, so a value with no table row is the COMMON case over time — underscores become spaces and
## the first letter is capitalised, which makes a raw identifier on screen impossible by
## construction rather than by anyone remembering to add a row.
##
## It takes no `kind`: no rule that survived the spec keys on one. The single candidate — hiding
## `bracket`/`cause` on `died`, whose label already says "An elder died of cold in Band 1" — is
## contradicted by `DETAIL_VALUE_LABELS`, which names exactly those values, so the trailing column
## restates them compactly on purpose. Add the parameter when a rule needs it, not before.
static func detail_phrase(detail: String) -> String:
	if detail == "":
		return ""
	if _is_prose_detail(detail):
		return detail
	var fragments: Array[String] = []
	# Parallel to `fragments`: the raw value of the `key=value` each entry came from, or `""` for a
	# coordinate / bare word. A continuation appends to the last entry that HAS one.
	var open_key: String = ""
	var open_value: String = ""
	var open_index := -1
	for token in detail.split(" ", false):
		var coordinate := _coordinate_phrase(token)
		if coordinate != "":
			fragments.append(coordinate)
			open_index = -1
			continue
		var split := token.find("=")
		if split > 0:
			open_key = token.substr(0, split)
			open_value = token.substr(split + 1)
			if HudEventVocab.DETAIL_KEY_HIDDEN.has(open_key):
				open_index = -1
				continue
			fragments.append(_key_value_phrase(open_key, open_value))
			open_index = fragments.size() - 1
			continue
		if HudEventVocab.DETAIL_FILLER_WORDS.has(token):
			continue
		if open_index >= 0:
			# A continuation of the previous value — `species=Grey` then `Wolf`.
			open_value += " " + token
			fragments[open_index] = _key_value_phrase(open_key, open_value)
			continue
		fragments.append(_english(token))
	return HudEventVocab.DETAIL_PHRASE_SEPARATOR.join(fragments)

## **A DETAIL THE SIM WROTE AS A SENTENCE, WHICH IS SHOWN VERBATIM.** Every COMMAND REFUSAL takes
## this path — `emit_command_failure` puts the sim's own explanation in the `detail` slot ("Scouts 1
## cannot start a life here — nobody at home could point at that place…") — and the token walk below
## would split it on spaces and rejoin the words with ` · `, turning one sentence into a column of
## capitalised words.
##
## **A SINGLE BARE TOKEN IS NOT PROSE**, which is why the word count is half the test: `herd_gone` is
## an identifier and must keep going through `_english` to reach the screen as `Herd gone`. Nor is a
## detail carrying a `key=value` fragment or a `(x,y)` coordinate anywhere in it — those are the
## machine contract, and one token of it makes the whole string one.
static func _is_prose_detail(detail: String) -> bool:
	var words := 0
	for token in detail.split(" ", false):
		if token.find("=") > 0 or _coordinate_phrase(token) != "":
			return false
		words += 1
	return words > 1

## One `key=value` as prose. A numeric value keeps its key (`Warriors 3`); an enumerated one is the
## whole phrase (`Settle site`), because the key restates what the value already says.
static func _key_value_phrase(key: String, value: String) -> String:
	if value.is_valid_float():
		return HudEventVocab.DETAIL_NUMERIC_FORMAT % [_english(key), _trimmed_number(value)]
	if HudEventVocab.DETAIL_VALUE_LABELS.has(value):
		return String(HudEventVocab.DETAIL_VALUE_LABELS[value])
	return _english(value)

## `(64,36)` → `(64, 36)`, or `""` when the token is not a coordinate.
static func _coordinate_phrase(token: String) -> String:
	if not token.begins_with("(") or not token.ends_with(")"):
		return ""
	var inner := token.substr(1, token.length() - 2)
	var parts := inner.split(",", false)
	if parts.size() != 2 or not parts[0].is_valid_int() or not parts[1].is_valid_int():
		return ""
	return HudEventVocab.DETAIL_COORDINATE_FORMAT % [int(parts[0]), int(parts[1])]

## A wire number as a READ number. The sim writes casualties with `{:.3}` — that precision is honest
## on the wire, where a `Scalar` really can be fractional, and it is DEBUG OUTPUT on a notification
## bar: `Killed 2.000` is a float where the player is owed a count. Trailing zeros and a bare decimal
## point come off, so `2.000` → `2` while a genuinely fractional `1.750` → `1.75`.
##
## It TRIMS rather than rounds, deliberately — rounding here would state a precision the sim did not,
## and a casualty count that reads `2` when the sim said `1.5` is a lie the player cannot detect.
static func _trimmed_number(value: String) -> String:
	# THE EARLY RETURN IS LOAD-BEARING, not a fast path: `rstrip("0")` on a bare `100` would strip
	# the value's own trailing zeros and yield `1`. Only a number carrying a decimal point has
	# trailing zeros that mean nothing.
	if not value.contains("."):
		return value
	return value.rstrip("0").rstrip(".")

## The generic fallback: an identifier as English. `herd_gone` → `Herd gone`.
static func _english(raw: String) -> String:
	var words := raw.replace("_", " ").strip_edges()
	if words == "":
		return ""
	return words.substr(0, 1).to_upper() + words.substr(1)

## The `status=` fragment this detail carries, or `""`. Matched on the WHOLE space-delimited
## `key=value` fragment against the detail the sim writes (`"status=feral reason=untended …"`), so a
## token can only match the field it names — a bare substring test on `feral` would also fire on a
## species key or a tile label that happened to contain the word.
func _detail_status_key(detail: String) -> String:
	if detail == "":
		return ""
	for token in detail.split(" ", false):
		if HudEventVocab.DETAIL_STATUS_STYLE.has(token):
			return String(token)
	return ""

## Drop whole turns off the back. **THE BOUND IS TURNS, NOT ENTRIES** — add a birth, a death and a
## coming-of-age per band per turn and a fixed entry ring evicts a wolf raid inside two turns, i.e.
## the cap would quietly eat exactly the events it exists to preserve. A turn is also the unit the
## player thinks in and the unit the log groups by.
func _prune() -> void:
	if _current_turn < 0:
		return
	var oldest := _current_turn - _retention_turns + 1
	var kept: Array[Dictionary] = []
	for event in _events:
		if int(event["tick"]) >= oldest:
			kept.append(event)
	_events = kept

# ---- the filtered pool -----------------------------------------------------

## Every retained event that passes the detail floor and whose channel is enabled, NEWEST FIRST.
##
## The sort is `(tick, seq, order)` descending. `seq` is the sim's own monotonic ordering within a
## turn; a client-side note has none, so it sorts to the TOP of its turn — it just happened, and the
## turn's sim events are already history by the time the client speaks.
func _visible_events() -> Array[Dictionary]:
	var floor_rungs: Array = HudEventVocab.DETAIL_FLOOR.get(
		_detail_level, HudEventVocab.DETAIL_FLOOR[HudEventVocab.DEFAULT_DETAIL_LEVEL])
	var pool: Array[Dictionary] = []
	for event in _events:
		if not floor_rungs.has(String(event["rung"])):
			continue
		if not bool(_channels.get(String(event["channel"]), true)):
			continue
		pool.append(event)
	pool.sort_custom(_newest_first)
	return pool

func _newest_first(a: Dictionary, b: Dictionary) -> bool:
	var a_tick := int(a["tick"])
	var b_tick := int(b["tick"])
	if a_tick != b_tick:
		return a_tick > b_tick
	var a_seq := int(a["seq"])
	var b_seq := int(b["seq"])
	if a_seq != b_seq:
		# A client note (seq -1) is the newest thing in its turn, not the oldest.
		if a_seq < 0:
			return true
		if b_seq < 0:
			return false
		return a_seq > b_seq
	return int(a["order"]) > int(b["order"])

## The rows the BAR shows: newest first, with the pinned-alert exception. An unread Alert that would
## otherwise fall outside the window claims the LEADING slot instead of vanishing — so the bar stays
## chronological AND a raid can never be pushed off by two forage receipts in the same turn.
##
## Sets `_pinned_order` as a side effect, which is what `_render_bar` tints the held row from.
func _bar_rows() -> Array[Dictionary]:
	_pinned_order = -1
	var pool := _visible_events()
	var rows: Array[Dictionary] = []
	if pool.is_empty():
		return rows
	for i in range(mini(_recent_count, pool.size())):
		rows.append(pool[i])
	if _alert_seen:
		return rows
	var top_alert: Dictionary = {}
	for event in pool:
		if String(event["rung"]) == HudEventVocab.RUNG_ALERT:
			top_alert = event
			break
	if top_alert.is_empty():
		return rows
	var alert_order := int(top_alert["order"])
	for row in rows:
		if int(row["order"]) == alert_order:
			return rows       # already in the window; it needs no pin
	var pinned_rows: Array[Dictionary] = [top_alert]
	for i in range(maxi(0, _recent_count - 1)):
		if i >= rows.size():
			break
		pinned_rows.append(rows[i])
	_pinned_order = alert_order
	return pinned_rows

## The distinct turns present in the visible pool, newest first.
func _visible_turns(pool: Array[Dictionary]) -> Array[int]:
	var turns: Array[int] = []
	for event in pool:
		var tick := int(event["tick"])
		if not turns.has(tick):
			turns.append(tick)
	return turns

# ---- public controls -------------------------------------------------------

## Dock to `SIDE_TOP` / `SIDE_BOTTOM`. Re-anchors, persists, and announces the move so `Main` can
## re-measure the new edge's displacement (`dock_changed` → `set_edge_offset`). It reserves nothing,
## so there is no reservation to re-emit.
func set_dock(edge: int) -> void:
	if not DOCK_EDGES.has(edge) or edge == _dock_edge:
		return
	_dock_edge = edge
	_apply_dock_layout()
	_render()
	_save_prefs()
	dock_changed.emit(_dock_edge)

func get_dock() -> int:
	return _dock_edge

## **THE BAR LIVES BETWEEN THE VERTICAL DOCKS, NOT ACROSS THEM.**
##
## A left- or right-docked panel owns its full-height column; a top/bottom strip that spanned the raw
## window would be drawn straight over the top of it. That is not a stacking-order problem to solve
## with `layer` — the band panel is a genuine occupant of that column and the bar has no business
## there — so the strip's own EXTENT is pulled in by the live `SIDE_LEFT` / `SIDE_RIGHT` reservation
## totals instead. (Reported live: a `SIDE_TOP` bar covering the `SIDE_LEFT` band panel's tab bar.)
##
## **This is the PERPENDICULAR axis and it is NOT `RESERVER_PRIORITY`.** Priority orders reservers
## stacked ALONG one shared edge — the dock stays 0 there, so it still hugs its own edge — while this
## is the cross axis, where no stacking question arises: every vertical reserver simply takes room
## the horizontal bar may not use. The two are easy to conflate and neither substitutes for the other.
##
## **It bounds where the strip is DRAWN.** The strip reserves nothing from either surface, so there
## is no second effect to reason about — the columns it stops short of are simply the ones it must
## not cover.
func set_perpendicular_insets(left: float, right: float) -> void:
	var new_left := maxf(left, 0.0)
	var new_right := maxf(right, 0.0)
	if is_equal_approx(new_left, _inset_left) and is_equal_approx(new_right, _inset_right):
		return
	_inset_left = new_left
	_inset_right = new_right
	_apply_dock_layout()

## **THE BAND PANEL KEEPS THE SCREEN EDGE; THE BAR IS DISPLACED PAST IT.** The twin of the insets
## above, on the strip's OWN axis: `Main` sums every reserver sitting on this dock's edge
## (`_update_event_dock_edge_offset`) and pushes the total here, so a co-edge band panel — top with
## top, bottom with bottom — is no longer drawn over. The bar sits BELOW a top-docked panel and
## ABOVE a bottom-docked one; the strip is pushed inboard, never shrunk.
##
## **This does not make the dock a reserver.** It still takes no space from the map or the HUD and
## still has no entry in `Main._reservations` — it is the innermost thing on whatever edge it is on,
## which is exactly why the sum needs no priority ordering (`BandCityPanel.set_edge_offset` has one;
## this deliberately does not).
func set_edge_offset(px: float) -> void:
	var offset: float = maxf(px, 0.0)
	if is_equal_approx(offset, _edge_offset):
		return
	_edge_offset = offset
	_apply_dock_layout()

## Hide/show the whole strip — the `R` hotkey, and the successor to the retired feed's own suppress
## flag (whose persisted value migrates onto this one). A hidden strip reserves nothing.
func set_suppressed(suppressed: bool) -> void:
	if suppressed == _suppressed:
		return
	_suppressed = suppressed
	if _root != null:
		_root.visible = not _suppressed
	_apply_dock_layout()
	_save_prefs()

func toggle_suppressed() -> void:
	set_suppressed(not _suppressed)

func is_suppressed() -> bool:
	return _suppressed

func set_expanded(expanded: bool) -> void:
	if expanded == _expanded:
		return
	_expanded = expanded
	# Opening the log is what marks alerts read — the pin exists to survive until the player has
	# actually had a chance to look at them.
	if _expanded:
		_alert_seen = true
		_log_window_turns = LOG_WINDOW_TURNS
	_apply_dock_layout()
	_render()

func is_expanded() -> bool:
	return _expanded

func set_recent_count(count: int) -> void:
	var clamped: int = clampi(count, RECENT_COUNT_MIN, RECENT_COUNT_MAX)
	if clamped == _recent_count:
		return
	_recent_count = clamped
	_apply_dock_layout()
	_render()
	_save_prefs()

func set_detail_level(level: String) -> void:
	if not HudEventVocab.DETAIL_FLOOR.has(level) or level == _detail_level:
		return
	_detail_level = level
	_log_window_turns = LOG_WINDOW_TURNS
	_render()
	_save_prefs()

func set_channel_enabled(channel: String, enabled: bool) -> void:
	if bool(_channels.get(channel, true)) == enabled:
		return
	_channels[channel] = enabled
	_log_window_turns = LOG_WINDOW_TURNS
	_render()
	_save_prefs()

# ---- construction ----------------------------------------------------------

func _build() -> void:
	_root = Control.new()
	_root.name = "EventDockRoot"
	_root.visible = not _suppressed
	# **THE STRIP EATS ITS OWN CLICKS.** It overlays live map, and `MapView` picks hexes out of
	# `_unhandled_input` — so a control under the pointer that does NOT consume the press lets the
	# same click select the hex behind the bar. `MOUSE_FILTER_STOP` is the Control default, but it is
	# set here EXPLICITLY: this is the first HUD element where the default is load-bearing rather than
	# incidental, and a future `IGNORE` added for some hover effect would silently reintroduce the
	# click-through. The card below it is the surface that actually covers the rect; the root is the
	# backstop for anything the card's layout leaves bare.
	_root.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(_root)

	_panel = PanelContainer.new()
	_panel.name = "EventDockCard"
	_panel.add_theme_stylebox_override("panel", _panel_stylebox())
	_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	_panel.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_root.add_child(_panel)

	_column = VBoxContainer.new()
	_column.name = "EventDockColumn"
	_column.add_theme_constant_override("separation", LOG_SECTION_SEPARATION)
	_panel.add_child(_column)

	_bar = _build_bar()
	_column.add_child(_bar)

	_log = _build_log()
	_column.add_child(_log)

	_seam = ColorRect.new()
	_seam.name = "EventDockSeam"
	_seam.color = HudStyle.SIGNAL_DEEP
	_seam.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(_seam)

func _build_bar() -> HBoxContainer:
	var bar := HBoxContainer.new()
	bar.name = "EventBar"
	bar.add_theme_constant_override("separation", ROW_ITEM_SEPARATION)

	_rows = VBoxContainer.new()
	_rows.name = "EventRows"
	_rows.add_theme_constant_override("separation", ROW_SEPARATION)
	_rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_rows.clip_contents = true
	bar.add_child(_rows)

	_more_button = Button.new()
	_more_button.name = "EventExpander"
	_more_button.focus_mode = Control.FOCUS_NONE
	_more_button.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_more_button.add_theme_font_size_override("font_size", CONTROL_FONT_SIZE)
	HudStyle.apply_button(_more_button, "ghost")
	_trim_button(_more_button)
	_more_button.pressed.connect(_on_more_pressed)
	bar.add_child(_more_button)
	return bar

func _build_log() -> VBoxContainer:
	var log_column := VBoxContainer.new()
	log_column.name = "EventLog"
	log_column.add_theme_constant_override("separation", LOG_SECTION_SEPARATION)
	log_column.size_flags_vertical = Control.SIZE_EXPAND_FILL
	log_column.visible = false

	_log_head = HFlowContainer.new()
	_log_head.name = "EventLogHead"
	_log_head.add_theme_constant_override("h_separation", LOG_HEAD_SEPARATION)
	_log_head.add_theme_constant_override("v_separation", LOG_HEAD_SEPARATION)
	log_column.add_child(_log_head)

	_log_head.add_child(_make_control_label(HudEventVocab.CHANNELS_LABEL))
	for channel in HudEventVocab.CHANNEL_ORDER:
		var channel_chip := _make_chip(String(HudEventVocab.CHANNEL_LABELS[channel]))
		channel_chip.pressed.connect(_on_channel_chip_pressed.bind(String(channel)))
		_log_head.add_child(channel_chip)
		_channel_chips[String(channel)] = channel_chip

	_log_head.add_child(_make_control_label(HudEventVocab.DETAIL_LABEL))
	for rung in HudEventVocab.RUNG_ORDER:
		var detail_chip := _make_chip(String(HudEventVocab.DETAIL_FLOOR_LABELS[rung]))
		detail_chip.pressed.connect(_on_detail_chip_pressed.bind(String(rung)))
		_log_head.add_child(detail_chip)
		_detail_chips[String(rung)] = detail_chip

	_log_head.add_child(_make_control_label(HudEventVocab.ROWS_LABEL))
	for count in RECENT_COUNT_CHOICES:
		var count_chip := _make_chip(str(count))
		count_chip.pressed.connect(_on_count_chip_pressed.bind(count))
		_log_head.add_child(count_chip)
		_count_chips[count] = count_chip

	_log_head.add_child(_make_control_label(HudEventVocab.DOCK_LABEL))
	for edge in DOCK_EDGES:
		var glyph: String = HudEventVocab.DOCK_TOP_GLYPH if edge == SIDE_TOP \
			else HudEventVocab.DOCK_BOTTOM_GLYPH
		var dock_chip := _make_chip(glyph)
		dock_chip.pressed.connect(_on_dock_chip_pressed.bind(edge))
		_log_head.add_child(dock_chip)
		_dock_chips[edge] = dock_chip

	_log_scroll = ScrollContainer.new()
	_log_scroll.name = "EventLogScroll"
	_log_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_log_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	_log_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	log_column.add_child(_log_scroll)

	_log_body = VBoxContainer.new()
	_log_body.name = "EventLogBody"
	_log_body.add_theme_constant_override("separation", LOG_TURN_GROUP_SEPARATION)
	_log_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_log_scroll.add_child(_log_body)

	_log_foot = HBoxContainer.new()
	_log_foot.name = "EventLogFoot"
	_log_foot.add_theme_constant_override("separation", LOG_HEAD_SEPARATION)
	log_column.add_child(_log_foot)

	_earlier_button = Button.new()
	_earlier_button.name = "EarlierTurns"
	_earlier_button.text = HudEventVocab.EARLIER_TURNS_TEXT
	_earlier_button.focus_mode = Control.FOCUS_NONE
	_earlier_button.add_theme_font_size_override("font_size", CONTROL_FONT_SIZE)
	HudStyle.apply_button(_earlier_button, "ghost")
	_trim_button(_earlier_button)
	_earlier_button.pressed.connect(_on_earlier_pressed)
	_log_foot.add_child(_earlier_button)

	_retention_label = _make_label("", TURN_FONT_SIZE, HudStyle.INK_FAINT)
	_retention_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_log_foot.add_child(_retention_label)
	return log_column

func _make_label(text: String, font_size: int, color: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", color)
	return label

func _make_control_label(text: String) -> Label:
	var label := _make_label(text.to_upper(), TURN_FONT_SIZE, HudStyle.INK_FAINT)
	label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	return label

func _make_chip(text: String) -> Button:
	var chip := Button.new()
	chip.text = text
	chip.focus_mode = Control.FOCUS_NONE
	chip.add_theme_font_size_override("font_size", CHIP_FONT_SIZE)
	return chip

## Squeeze a button's default chrome. `HudStyle` pads its buttons 9px top and bottom, which alone
## would make the expander taller than the whole two-row bar it sits in.
func _trim_button(button: Button) -> void:
	for state in ["normal", "hover", "pressed", "disabled", "focus"]:
		var box: StyleBox = button.get_theme_stylebox(state)
		if box == null:
			continue
		var trimmed: StyleBox = box.duplicate()
		trimmed.content_margin_top = ROW_PADDING_V
		trimmed.content_margin_bottom = ROW_PADDING_V
		button.add_theme_stylebox_override(state, trimmed)

# ---- render ----------------------------------------------------------------

func _render() -> void:
	if _rows == null:
		return
	_render_bar()
	_render_log()
	_refresh_chips()

func _render_bar() -> void:
	for child in _rows.get_children():
		_rows.remove_child(child)
		child.queue_free()
	var pool := _visible_events()
	var rows: Array[Dictionary] = []
	if _expanded:
		_pinned_order = -1
	else:
		rows = _bar_rows()

	if _expanded:
		# THE BAR IS THE COLLAPSED STATE — one title line, never a reprint of the log's own newest
		# turn-group (see the header).
		var turns := _visible_turns(pool)
		var title := HudEventVocab.EMPTY_TEXT
		if not turns.is_empty():
			title = HudEventVocab.EXPANDED_TITLE_FORMAT % [turns[turns.size() - 1], turns[0]]
		_rows.add_child(_make_message_row(title))
	elif rows.is_empty():
		_rows.add_child(_make_message_row(HudEventVocab.EMPTY_TEXT))
	else:
		for event in rows:
			_rows.add_child(_make_event_row(event, int(event["order"]) == _pinned_order))

	var badge: int = pool.size() if _expanded else maxi(0, pool.size() - rows.size())
	# The badge runs HOT while an unread alert is still waiting behind the bar.
	var hot := not _alert_seen and not _expanded and badge > 0
	_more_button.text = "%d  %s  %s" % [
		badge,
		HudEventVocab.CLOSE_TEXT if _expanded else HudEventVocab.MORE_TEXT,
		_caret_glyph(),
	]
	_more_button.add_theme_color_override("font_color",
		HudStyle.DANGER if hot else HudStyle.INK_DIM)

## The caret points AWAY from the docked edge when collapsed (the direction the log will open into)
## and back at the edge when expanded (the direction it will close), so it reads as a direction
## rather than as decoration.
func _caret_glyph() -> String:
	var opens_up := _dock_edge == SIDE_BOTTOM
	if _expanded:
		return HudEventVocab.CARET_DOWN if opens_up else HudEventVocab.CARET_UP
	return HudEventVocab.CARET_UP if opens_up else HudEventVocab.CARET_DOWN

func _make_message_row(text: String) -> Control:
	var row := PanelContainer.new()
	row.add_theme_stylebox_override("panel", _row_stylebox(Color(0.0, 0.0, 0.0, 0.0), false))
	row.custom_minimum_size.y = ROW_HEIGHT
	var label := _make_label(text, ROW_FONT_SIZE, HudStyle.INK_FAINT)
	label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(label)
	return row

func _make_event_row(event: Dictionary, pinned: bool) -> Control:
	var accent: Color = event["color"]
	var row := PanelContainer.new()
	row.add_theme_stylebox_override("panel", _row_stylebox(accent, pinned))
	row.custom_minimum_size.y = ROW_HEIGHT
	row.tooltip_text = _row_tooltip(event)

	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", ROW_ITEM_SEPARATION)
	line.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(line)

	var glyph := _make_label(String(event["glyph"]), GLYPH_FONT_SIZE, accent)
	glyph.custom_minimum_size.x = GLYPH_COLUMN_WIDTH
	glyph.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	line.add_child(glyph)

	line.add_child(_make_label(_turn_stamp(int(event["tick"])), TURN_FONT_SIZE,
		HudEventVocab.TURN_STAMP_COLOR))

	var label := _make_label(_row_label(event), ROW_FONT_SIZE, _label_color(event))
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.clip_text = true
	label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	line.add_child(label)

	# THE PHRASE, never the raw tokens — see `detail_phrase`. Rendered here rather than stamped at
	# ingest for the same reason the band label is: the raw detail stays the input to
	# `_detail_status_key`, and a stored phrase would start matching prose.
	var detail := detail_phrase(String(event["detail"]))
	if detail != "":
		line.add_child(_make_label(detail, DETAIL_FONT_SIZE, HudStyle.INK_FAINT))
	return row

## An Alert's label carries its accent; a Routine one recedes to `INK_DIM`; a Notable one stays on
## the shared ink. The rung reads off the rail and the glyph — tinting every label would turn the
## bar into a colour chart and cost the alerts the contrast that makes them alerts.
func _label_color(event: Dictionary) -> Color:
	var rung := String(event["rung"])
	if rung == HudEventVocab.RUNG_ALERT:
		return event["color"]
	if rung == HudEventVocab.RUNG_ROUTINE:
		return HudStyle.INK_DIM
	return HudStyle.INK

func _turn_stamp(tick: int) -> String:
	if tick < 0:
		return HudEventVocab.TURN_STAMP_UNKNOWN
	return HudEventVocab.TURN_STAMP_FORMAT % tick

## The row's label with the sim's positional band name swapped for the CLIENT's, when the event
## carries a `band=` token naming a band the roster knows. Resolved here rather than at ingest so a
## roster change relabels rows already held, and so a row that arrives before the first
## `set_band_labels` is not stuck with the fallback forever.
##
## **The swap is bounded at a DIGIT boundary**, or the sim's `Band 3` would also rewrite the `Band 3`
## inside `Band 30`. A plain `String.replace` is exactly that bug.
func _row_label(event: Dictionary) -> String:
	var label := String(event["label"])
	var raw_id := _detail_token(String(event["detail"]), HudEventVocab.DETAIL_BAND_KEY)
	if raw_id == "" or not _band_labels.has(raw_id):
		return label
	var client_name := String(_band_labels[raw_id])
	var sim_name: String = HudEventVocab.SIM_BAND_LABEL_FORMAT % int(raw_id)
	if client_name == sim_name:
		return label
	var at := label.find(sim_name)
	while at >= 0:
		var after := at + sim_name.length()
		if after >= label.length() or not label.substr(after, 1).is_valid_int():
			return label.substr(0, at) + client_name + label.substr(after)
		at = label.find(sim_name, at + 1)
	return label

## The value of one space-delimited `key=value` token in a detail string, or `""`. The same whole-
## fragment discipline `_detail_status_key` uses, for the same reason.
func _detail_token(detail: String, key: String) -> String:
	if detail == "":
		return ""
	var prefix := key + "="
	for token in detail.split(" ", false):
		if token.begins_with(prefix):
			return token.substr(prefix.length())
	return ""

func _row_tooltip(event: Dictionary) -> String:
	var parts: Array[String] = [_row_label(event)]
	var detail := detail_phrase(String(event["detail"]))
	if detail != "":
		parts.append(detail)
	parts.append("%s · %s" % [String(event["kind"]), String(event["rung"])])
	return "\n".join(parts)

func _row_stylebox(accent: Color, pinned: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(accent.r, accent.g, accent.b, PINNED_WASH_ALPHA) if pinned \
		else Color(0.0, 0.0, 0.0, 0.0)
	sb.border_width_left = ROW_ACCENT_WIDTH
	sb.border_color = accent
	sb.content_margin_left = ROW_PADDING_H
	sb.content_margin_right = ROW_PADDING_H
	sb.content_margin_top = ROW_PADDING_V
	sb.content_margin_bottom = ROW_PADDING_V
	return sb

func _render_log() -> void:
	if _log == null:
		return
	_log.visible = _expanded
	if not _expanded:
		return
	for child in _log_body.get_children():
		_log_body.remove_child(child)
		child.queue_free()
	var pool := _visible_events()
	var turns := _visible_turns(pool)
	var shown: int = mini(_log_window_turns, turns.size())
	for i in range(shown):
		var turn := turns[i]
		var group := VBoxContainer.new()
		group.add_theme_constant_override("separation", LOG_TURN_GROUP_SEPARATION)
		group.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		var head := PanelContainer.new()
		head.add_theme_stylebox_override("panel", _turn_head_stylebox())
		head.add_child(_make_label(HudEventVocab.LOG_TURN_HEAD_FORMAT % turn,
			TURN_HEAD_FONT_SIZE, HudStyle.INK_FAINT))
		group.add_child(head)
		for event in pool:
			if int(event["tick"]) == turn:
				group.add_child(_make_event_row(event, false))
		_log_body.add_child(group)

	var exhausted := shown >= turns.size()
	_earlier_button.disabled = exhausted
	_earlier_button.text = HudEventVocab.EARLIER_TURNS_EXHAUSTED_TEXT if exhausted \
		else HudEventVocab.EARLIER_TURNS_TEXT
	_retention_label.text = HudEventVocab.LOG_RETENTION_FORMAT % [
		shown, turns.size(), _retention_turns]

func _turn_head_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.GROUND_2
	sb.border_width_bottom = 1
	sb.border_color = HudStyle.LINE_SOFT
	sb.content_margin_left = LOG_TURN_HEAD_PADDING_H
	sb.content_margin_right = LOG_TURN_HEAD_PADDING_H
	sb.content_margin_top = LOG_TURN_HEAD_PADDING_V
	sb.content_margin_bottom = LOG_TURN_HEAD_PADDING_V
	return sb

func _refresh_chips() -> void:
	for channel in _channel_chips:
		_apply_chip_state(_channel_chips[channel], bool(_channels.get(channel, true)))
	for rung in _detail_chips:
		_apply_chip_state(_detail_chips[rung], String(rung) == _detail_level)
	for count in _count_chips:
		_apply_chip_state(_count_chips[count], int(count) == _recent_count)
	for edge in _dock_chips:
		_apply_chip_state(_dock_chips[edge], int(edge) == _dock_edge)

func _apply_chip_state(chip: Button, selected: bool) -> void:
	HudStyle.apply_pill_button(chip, selected)
	chip.add_theme_color_override("font_color",
		HudStyle.SIGNAL if selected else HudStyle.INK_FAINT)

# ---- handlers --------------------------------------------------------------

func _on_more_pressed() -> void:
	set_expanded(not _expanded)

func _on_earlier_pressed() -> void:
	_log_window_turns += LOG_WINDOW_TURNS
	_render_log()

func _on_channel_chip_pressed(channel: String) -> void:
	set_channel_enabled(channel, not bool(_channels.get(channel, true)))

func _on_detail_chip_pressed(rung: String) -> void:
	set_detail_level(rung)

func _on_count_chip_pressed(count: int) -> void:
	set_recent_count(count)

func _on_dock_chip_pressed(edge: int) -> void:
	set_dock(edge)

# ---- layout ----------------------------------------------------------------

func _apply_dock_layout() -> void:
	if _root == null:
		return
	var cross := _cross_axis_size()
	# The strip stops short of whatever is docked left/right (`set_perpendicular_insets`) and is then
	# BOUNDED AT BOTH ENDS inside the band that leaves — capped at `MAX_STRIP_WIDTH` and floored at
	# `MIN_STRIP_WIDTH` — and centred in it. Both anchors sit at 0 so the offsets are absolute from the
	# left edge: the width is computed, not derived from the window.
	var window := _viewport_size()
	var band := maxf(window.x - _inset_left - _inset_right, 0.0)
	# The FLOOR yields to the window, never the other way round: a viewport narrower than one row has
	# nowhere to put the overhang, and a strip hanging off the screen edge loses the same text the
	# floor exists to save.
	var width := clampf(band, minf(MIN_STRIP_WIDTH, window.x), MAX_STRIP_WIDTH)
	# `band - width` is NEGATIVE exactly when the floor bound, so the same expression that centres a
	# capped strip in a wide band overhangs a floored one symmetrically into the columns either side.
	# The clamp then keeps it on screen.
	var leading := _inset_left + (band - width) * 0.5
	leading = clampf(leading, 0.0, maxf(window.x - width, 0.0))
	_root.anchor_left = 0.0
	_root.anchor_right = 0.0
	_root.offset_left = leading
	_root.offset_right = leading + width
	# On its OWN axis the strip starts `_edge_offset` in from the docked edge, so a co-edge reserver
	# (the band panel, top with top or bottom with bottom) holds the rim and the bar sits just past
	# it. Near edge at `_edge_offset`, far edge at `_edge_offset + cross` — the `BandCityPanel`
	# `_apply_root_anchors` idiom, mirrored.
	var near := _edge_offset
	var far := _edge_offset + cross
	if _dock_edge == SIDE_TOP:
		_root.anchor_top = 0.0
		_root.anchor_bottom = 0.0
		_root.offset_top = near
		_root.offset_bottom = far
	else:
		_root.anchor_top = 1.0
		_root.anchor_bottom = 1.0
		_root.offset_top = -far
		_root.offset_bottom = -near
	_order_column()
	_position_seam()
	_publish_occupancy()

## **THE BAND OF THE WINDOW THIS STRIP COVERS, MEASURED FROM THE SCREEN EDGE IT IS DOCKED TO.** Its
## own drawn depth PLUS the displacement that pushed it inboard of a co-edge reserver, because the
## question a free-floating card asks is "how far in from the edge is clear", and answering with the
## strip's height alone would leave the card free to sit in the gap the band panel is holding.
##
## **A hidden strip covers nothing** — `set_suppressed` runs through `_apply_dock_layout`, so the
## room grows back the moment the player presses `R`. There is deliberately no "empty" case beside
## it: the bar keeps its authored height with no events in it (`_bar_height`), because a strip that
## shrank on a quiet turn would move the map underneath it, so an empty bar still covers its band.
func occupied_extent() -> float:
	if _suppressed or _root == null:
		return 0.0
	return _edge_offset + _cross_axis_size()

func _publish_occupancy() -> void:
	var extent := occupied_extent()
	if _dock_edge == _published_edge and is_equal_approx(extent, _published_extent):
		return
	_published_edge = _dock_edge
	_published_extent = extent
	occupancy_changed.emit(_dock_edge, extent)

## THE BAR HUGS THE SCREEN EDGE AND THE LOG OPENS INWARD, on both edges. On a bottom dock the column
## therefore reads log-then-bar; on a top dock, bar-then-log. One `move_child` rather than two
## layouts, so nothing about a row can differ between the two edges.
func _order_column() -> void:
	if _column == null or _bar == null or _log == null:
		return
	_column.move_child(_bar, 1 if _dock_edge == SIDE_BOTTOM else 0)

## Pin the accent seam to the strip's map-facing edge (opposite the dock edge).
func _position_seam() -> void:
	if _seam == null:
		return
	_seam.anchor_left = 0.0
	_seam.anchor_right = 1.0
	_seam.offset_left = 0.0
	_seam.offset_right = 0.0
	if _dock_edge == SIDE_TOP:
		_seam.anchor_top = 1.0
		_seam.anchor_bottom = 1.0
		_seam.offset_top = -SEAM_THICKNESS
		_seam.offset_bottom = 0.0
	else:
		_seam.anchor_top = 0.0
		_seam.anchor_bottom = 0.0
		_seam.offset_top = 0.0
		_seam.offset_bottom = SEAM_THICKNESS

## The strip's own drawn height. **Content-independent by construction** — it reads only the
## preference (`_recent_count`), the expanded flag and the viewport. That mattered acutely while the
## strip reserved its edge (a content-driven size re-emitted the reservation and flickered the map
## every turn); it still matters now that it overlays, because a bar that changed height as events
## arrived would make the map jump underneath it.
func _cross_axis_size() -> float:
	var height := _bar_height()
	if _expanded:
		height += LOG_HEIGHT + float(LOG_SECTION_SEPARATION)
	return minf(height, _viewport_size().y * MAX_STRIP_HEIGHT_FRACTION)

## Expanded, the bar is one title line; collapsed, exactly `recent_count` rows whether or not it has
## that many events — an empty bar that shrank would move the map on every quiet turn.
func _bar_height() -> float:
	var rows := 1 if _expanded else _recent_count
	return ROW_HEIGHT * float(rows) + BAR_CHROME_V

func _viewport_size() -> Vector2:
	var vp := get_viewport()
	if vp != null:
		return vp.get_visible_rect().size
	return Vector2(0.0, LOG_HEIGHT)

func _panel_stylebox() -> StyleBoxFlat:
	# Square-edged strip: it meets the screen edge, so no rounding and no shadow.
	# **`PANEL_SOLID`, NOT `PANEL`** — the translucent one every docked card uses. Those sit on the
	# HUD's own background; this floats over live terrain, which can be snow or desert, and a
	# translucent strip would put a bright map straight through the row text.
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.PANEL_SOLID
	sb.set_border_width_all(PANEL_BORDER_WIDTH)
	sb.border_color = HudStyle.LINE
	sb.content_margin_left = PANEL_CONTENT_MARGIN_H
	sb.content_margin_right = PANEL_CONTENT_MARGIN_H
	sb.content_margin_top = PANEL_CONTENT_MARGIN_V
	sb.content_margin_bottom = PANEL_CONTENT_MARGIN_V
	return sb

# ---- persistence -----------------------------------------------------------

## The prefs file actually used — this panel's scratch override when a harness set one, else
## whatever `NarrativeForkPanel` resolves (which honours ITS own override, so a harness that has
## already isolated `narrative.cfg` gets this section isolated with it).
static func config_path() -> String:
	return config_path_override if config_path_override != "" else NarrativeForkPanel.config_path()

func _load_prefs() -> void:
	for channel in HudEventVocab.CHANNEL_ORDER:
		_channels[String(channel)] = true
	var cfg := ConfigFile.new()
	if cfg.load(config_path()) != OK:
		return
	var edge := int(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_EDGE, DEFAULT_EDGE))
	if DOCK_EDGES.has(edge):
		_dock_edge = edge
	_recent_count = clampi(
		int(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_RECENT_COUNT, DEFAULT_RECENT_COUNT)),
		RECENT_COUNT_MIN, RECENT_COUNT_MAX)
	var level := String(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_DETAIL_LEVEL,
		HudEventVocab.DEFAULT_DETAIL_LEVEL))
	if HudEventVocab.DETAIL_FLOOR.has(level):
		_detail_level = level
	# GUARD WITH `has_section_key`, NEVER a `null` default: `ConfigFile.get_value` treats a null
	# default as "no default was given" and pushes an engine ERROR. It cannot be defaulted to `[]`
	# either — an absent key means "every channel on", while a stored empty array means the player
	# turned them all off, and those must not collapse to the same branch.
	if cfg.has_section_key(CONFIG_SECTION, CONFIG_KEY_CHANNELS):
		var enabled_variant: Variant = cfg.get_value(CONFIG_SECTION, CONFIG_KEY_CHANNELS, [])
		if enabled_variant is Array:
			var enabled: Array = enabled_variant
			for channel in HudEventVocab.CHANNEL_ORDER:
				_channels[String(channel)] = enabled.has(String(channel))
	# MIGRATION — the retired command feed's hidden/shown key. A player who had opened the feed with
	# `R` lands on the bar that replaces it open too. Read once and ERASED, so a stale key in the
	# other section can never overwrite a later choice made here.
	if cfg.has_section_key(CONFIG_SECTION, CONFIG_KEY_SUPPRESSED):
		_suppressed = bool(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_SUPPRESSED, DEFAULT_SUPPRESSED))
	elif cfg.has_section_key(LEGACY_PANELS_SECTION, LEGACY_KEY_COMMAND_FEED_SUPPRESSED):
		_suppressed = bool(cfg.get_value(
			LEGACY_PANELS_SECTION, LEGACY_KEY_COMMAND_FEED_SUPPRESSED, DEFAULT_SUPPRESSED))
		cfg.erase_section_key(LEGACY_PANELS_SECTION, LEGACY_KEY_COMMAND_FEED_SUPPRESSED)
		cfg.set_value(CONFIG_SECTION, CONFIG_KEY_SUPPRESSED, _suppressed)
		cfg.save(config_path())

func _save_prefs() -> void:
	var cfg := ConfigFile.new()
	cfg.load(config_path())   # preserve every other section; ignore load errors
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_EDGE, _dock_edge)
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_RECENT_COUNT, _recent_count)
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_DETAIL_LEVEL, _detail_level)
	var enabled: Array[String] = []
	for channel in HudEventVocab.CHANNEL_ORDER:
		if bool(_channels.get(String(channel), true)):
			enabled.append(String(channel))
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_CHANNELS, enabled)
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_SUPPRESSED, _suppressed)
	cfg.save(config_path())
