extends RefCounted

## The event dock: rungs, channels, insets and the pinned alert.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 136

const WorldFx := preload("res://tools/ui_preview/fixtures_world.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## Injected for the `event_dock_*` block at the end of the run and freed again. Like the band panel
## it is its OWN CanvasLayer (not part of `HudLayer.tscn`), so it exists only for the states that
## judge it and cannot leak a reserved strip into the other 200-odd frames.
const EVENT_DOCK_SCENE := preload("res://src/ui/EventDockPanel.tscn")

## The CANVAS every frame is composed in, which is NOT the window: `project.godot` stretches
## `canvas_items` from a 1920-wide base with an `expand` aspect, so a control's own geometry is in
## these units while the PNG is in `PREVIEW_CANVAS_SIZE`. The event dock's width cap is a canvas
## number, so the narrow-case assertion has to compare against this one.
const PREVIEW_CANVAS_SIZE_BASE := Vector2i(1920, 1152)

## A deliberately ULTRAWIDE window for the one state that exercises the event dock's width cap — the
## configuration the "way too wide for its content" report came from, and one no other frame reaches.
## Rendered outside `_ensure_canvas`'s pinned-canvas guard (which exists to keep every OTHER frame
## comparable), then the canvas is re-pinned.
const ULTRAWIDE_WINDOW_SIZE := Vector2i(2560, 900)

## A pale sand/snow tone for the ONE state that renders the event bar over bright ground. The strip
## overlays live map now, and every other frame in this set puts it on a near-black backdrop, so its
## opacity has never been tested against anything. Not a real terrain sample — the point is a
## worst-case bright field behind the rows, and a screenshot of actual desert would be less extreme.
const BRIGHT_TERRAIN_COLOR := Color(0.90, 0.86, 0.76)

## The LEFT reservation the floor probe pushes. Chosen so the band it leaves on
## `PREVIEW_CANVAS_SIZE_BASE` is the **338px** the live report produced (a 1422px logical viewport at
## `ui_scale` 1.35 with the Band panel docked LEFT), rather than merely "some small number": the claim
## is about the configuration that was reported, and a squeeze picked for arithmetic convenience would
## drift away from it the first time either HUD column's authored width moved.
const FLOOR_PROBE_RESERVED_LEFT := 878.0

## The logical viewport the OVERHANG case was reported at — 1200px wide, which the HUD's two authored
## columns (360 + 344) leave a 496px band. That band is above the card's own minimum and below the
## comfortable width the floor used to clamp to, so it is the one range in which the strip was drawn
## outside the insets it was given.
const OVERHANG_PROBE_VIEWPORT_WIDTH := 1200.0
## The LEFT reservation that stages it here. This canvas floors at 1920 (`_pin_canvas` sets the window
## and `canvas_items` stretches from a 1920 base), so the reported viewport is reached by reserving the
## difference away rather than by shrinking the window — the band is what the strip's rule reads, and
## it cannot tell the two apart.
const OVERHANG_PROBE_RESERVED_LEFT := float(PREVIEW_CANVAS_SIZE_BASE.x) - OVERHANG_PROBE_VIEWPORT_WIDTH

## The two horizontal edges the shell-flip claim is walked on. Both, because `_apply_dock_layout`
## writes the two against DIFFERENT anchors, so a placement correct on one can be wrong on the other —
## the same reason the co-edge frames above are a pair.
const SHELL_FLIP_EDGES: Array[int] = [SIDE_TOP, SIDE_BOTTOM]
## The scale the flip is walked at — the one it was reported at.
##
## **`ui_scale` 1.0 IS NOT WALKED HERE, AND THAT IS A MEASUREMENT RATHER THAN AN OMISSION.** This
## harness cannot reach the narrow shell on a horizontal dock at 1.0: `project.godot` stretches
## `canvas_items` from a 1920 base with an `expand` aspect and `_pin_canvas` sets only the WINDOW size,
## so the canvas floors at 1920 however small the window is — and 1920 less the two HUD columns is
## **1216 against a 1190 threshold**, i.e. the wide shell holds by 26px. Asserted at 1.0 the
## precondition simply failed, refusing to claim anything, which is what it is for. The mechanism's
## 1.0 coverage lives in `band_panel_preview`, which pins `content_scale_size` as well and can
## therefore stand the panel in the narrow shell on a horizontal dock with no scale at all.
const SHELL_FLIP_SCALES := [1.35]

## The largest bar the dock offers, referenced rather than written as a 4 so the state and the
## panel's own `RECENT_COUNT_MAX` cannot drift.
const EVENT_DOCK_MAX_ROWS := EventDockPanel.RECENT_COUNT_MAX

## Sub-pixel slack for the co-edge rect claims. The offset and the panel's reserved size are the SAME
## float, so the two rects meet exactly — but they are read back through two CanvasLayers' global
## transforms, and an equality on a transformed float is a trap. Small enough that a real overlap (a
## whole strip, ≥ `COLLAPSED_SIZE`) can never hide inside it.
const CO_EDGE_RECT_EPSILON := 0.5

## How many `predator_raid` rows the fixture carries on turn 47 — TWO, deliberately identical apart
## from `seq`. This is the number the old signature de-duplication answered 1 to.
const EVENT_DOCK_DUPLICATE_RAIDS := 2

## The harness's stand-in for `Main._update_event_dock_insets`: the vertical reservation total on each
## side PLUS the HUD's own authored side column. `Main` is never instanced here, so the sum is
## restated — but every term is read live off the same nodes `Main` reads, so a change to either
## column's authored width lands here without an edit.
func _preview_push_event_dock_insets(dock: EventDockPanel, reserved_left: float, reserved_right: float) -> void:
	dock.set_perpendicular_insets(
		reserved_left + h._hud.left_column_width(), reserved_right + h._hud.right_column_width())

## How many retained events of one kind the dock is holding — read off its own accumulator, since the
## claim is about DE-DUPLICATION and a rendered row count would also be filtered by the detail floor.
func _preview_event_kind_count(dock: EventDockPanel, kind: String) -> int:
	var count := 0
	for event in dock._events:
		if String(event["kind"]) == kind:
			count += 1
	return count

func _preview_event_channels_all_on(dock: EventDockPanel) -> bool:
	for channel in HudEventVocab.CHANNEL_ORDER:
		if not bool(dock._channels.get(String(channel), false)):
			return false
	return true

## A scratch `narrative.cfg` that EXISTS and carries another panel's section, but no `[events]` —
## the shape every upgrading player's file has on first launch into this build.
func _write_event_prefs_without_section() -> void:
	var cfg := ConfigFile.new()
	cfg.set_value("hud_panels", "legend_suppressed", true)
	cfg.save(EventDockPanel.config_path())

func _write_event_prefs_with_channels(channels: Array) -> void:
	var cfg := ConfigFile.new()
	cfg.set_value("hud_panels", "legend_suppressed", true)
	cfg.set_value("events", "channels", channels)
	cfg.save(EventDockPanel.config_path())

## `rendered` reads the label the dock would DRAW (`_row_label`, i.e. after the band substitution)
## rather than the raw one it stored. The band-label assertions have to ask the rendered one — the
## substitution is deliberately a render-time resolution, so a raw read would pass on a dock that
## never re-labels anything.
func _preview_event_label_count(dock: EventDockPanel, label: String, rendered: bool = false) -> int:
	var count := 0
	for event in dock._events:
		var found: String = dock._row_label(event) if rendered else String(event["label"])
		if found == label:
			count += 1
	return count

## The same count over the FILTERED pool — everything the current detail floor and channel toggles
## admit. It is the strongest surface an ignored kind could still leak onto without being drawn, and
## it is asked ONLY beside a positive companion, so "nothing is visible" cannot pass for "the ignored
## row is not visible".
func _preview_visible_label_count(dock: EventDockPanel, label: String) -> int:
	var count := 0
	for event in dock._visible_events():
		if String(event["label"]) == label:
			count += 1
	return count

## What `_preview_event_rung` answers for a label the dock is not holding — a value no rung can take,
## so an absent row fails its claim instead of accidentally matching one.
const EVENT_DOCK_RUNG_ABSENT := ""

## The rung one RETAINED row resolved to. Read off the accumulator, where `_append` stamped it, so the
## claim is about the RUNG ITSELF and not about whichever floor the dock happens to be drawing — a
## row rendered at the wrong importance looks perfectly right in a frame that renders everything.
func _preview_event_rung(dock: EventDockPanel, label: String) -> String:
	for event in dock._events:
		if String(event["label"]) == label:
			return String(event["rung"])
	return EVENT_DOCK_RUNG_ABSENT

## Every `Work tab` link the dock has DRAWN, across the bar and the expanded log. A BUTTON walk rather
## than `_preview_dock_labels`' Label one, deliberately: what is under test is that the row offers a
## PRESS, and a `Label` reading `Work tab` would satisfy any text search while doing nothing at all.
func _preview_dock_work_links(dock: EventDockPanel) -> Array[Button]:
	var found: Array[Button] = []
	var stack: Array[Node] = [dock._rows, dock._log_body]
	while not stack.is_empty():
		var node: Node = stack.pop_back()
		if node == null:
			continue
		for child in node.get_children():
			stack.append(child)
		if node is Button and (node as Button).text == HudEventVocab.WORK_TAB_LINK_TEXT:
			found.append(node as Button)
	return found

## **THE MARK ONE DRAWN ROW IS WEARING, SAMPLED OFF THE RENDER.** `_make_event_row` builds each row
## as an `HBoxContainer` whose FIRST child is the glyph `Label`, so this finds the row by the text of
## its LABEL column and answers the text of its glyph column — the mark the player is actually looking
## at, rather than the one this chapter would get by asking the vocab that is under test.
##
## Walks the bar AND the expanded log, like `_preview_dock_work_links`, so the claim does not depend
## on which surface is open.
func _preview_dock_row_glyph(dock: EventDockPanel, label: String) -> String:
	var line := _preview_dock_row_line(dock, label)
	if line == null:
		return EVENT_DOCK_GLYPH_ABSENT
	for child in line.get_children():
		if child is Label:
			return (child as Label).text
	return EVENT_DOCK_GLYPH_ABSENT

## …and its INK, off the same node. The decision keeps both rungs amber — the glyph carries the rung
## and the colour carries *this is not good news* — so a fix that quietly demoted the trim to a calm
## ink would be a different change from the one that was asked for.
func _preview_dock_row_glyph_color(dock: EventDockPanel, label: String) -> Color:
	var line := _preview_dock_row_line(dock, label)
	if line == null:
		return EVENT_DOCK_GLYPH_COLOR_ABSENT
	for child in line.get_children():
		if child is Label:
			return (child as Label).get_theme_color("font_color")
	return EVENT_DOCK_GLYPH_COLOR_ABSENT

## The `HBoxContainer` of the drawn row whose label column reads `label`, or `null`. Shared by the two
## readers above so they cannot come to disagree about which row they are describing.
func _preview_dock_row_line(dock: EventDockPanel, label: String) -> HBoxContainer:
	var stack: Array[Node] = [dock._rows, dock._log_body]
	while not stack.is_empty():
		var node: Node = stack.pop_back()
		if node == null:
			continue
		for child in node.get_children():
			stack.append(child)
		if not (node is HBoxContainer):
			continue
		for child in node.get_children():
			if child is Label and (child as Label).text == label:
				return node as HBoxContainer
	return null

## What the two readers answer for a row the dock has not DRAWN — values no row can carry, so a
## missing row fails its claim rather than accidentally matching one.
const EVENT_DOCK_GLYPH_ABSENT := ""
const EVENT_DOCK_GLYPH_COLOR_ABSENT := Color(0.0, 0.0, 0.0, 0.0)

## ---- A SHORT BAND SHEDS A CREW (`systems::labor::announce_shed_crew`) -------------------------
## The five rows of the shed fixture, spelled out so the assertions compare against strings this
## chapter states rather than against strings recomposed through the code under test.

## **THE SIM'S SHAPE TODAY, and the link's negative case in one row.** `announce_shed_crew` writes the
## SOURCE — `kind=`, `x=`/`y=` — and no `band=`, so this row is Notable and offers no jump: the client
## will not recover a band by reading `foragers at (60, 0)` out of the label.
const SHED_TRIMMED_LINKLESS_LABEL := "foragers at (60, 0) cut to 3 — too few workers"

const SHED_TRIMMED_LINKLESS_DETAIL := "status=trimmed reason=too_few_workers kind=forage x=60 y=0 workers=3 lost=3"

## **THE SAME LINE ONCE THE SIM NAMES THE BAND** — the one token this row needs and does not yet
## carry. Staged here because the client half is what is under test, and stated as the ASK it is:
## `band={id}` on the shed detail, beside the source it already writes.
const SHED_TRIMMED_LINKED_LABEL := "hunters on aurochs-4 cut to 2 — too few workers"

const SHED_TRIMMED_LINKED_DETAIL := "status=trimmed reason=too_few_workers kind=hunt herd=aurochs-4 band=3 workers=2 lost=1"

const SHED_TRIMMED_LINKED_BAND := 3

## The row DESTROYED outright, which stays on the ALERT rung it already had: the queue entry goes with
## it on the turn's prune, so a build commitment is lost and not merely thinned.
const SHED_LAPSED_LABEL := "foragers at (58, 4) disbanded — too few workers"

const SHED_LAPSED_DETAIL := "status=lapsed reason=too_few_workers kind=forage x=58 y=4 band=7 workers=2 lost=2"

const SHED_LAPSED_BAND := 7

## The take a commitment narrowed under the crew still standing there (`server.rs handle_assign_labor`).
const SHED_PRUNED_LABEL := "kelp no longer stands here — dropped from the take"

const SHED_PRUNED_DETAIL := "status=pruned reason=not_here role=forage dropped=kelp"

## **THE CRAFTING BENCH LOSING ITS LAST HAND** (`systems::labor::announce_shed_bench`). A third shed
## token, because neither existing one is true: the bench is not *still worked* (`trimmed`) and it is
## not GONE (`lapsed`) — the recipe, the progress, the finished count and the drawn materials all stay
## and re-staffing resumes. It is Notable and recoverable, so it takes `trimmed`'s rung and mark and
## never `lapsed`'s.
##
## **THE KIND IS `craft`, WHICH IS NOT IN `RUNG_BY_KIND`** — so without its `DETAIL_STATUS_STYLE` row
## this line takes `DEFAULT_RUNG` (`RUNG_ROUTINE`) and falls under the dock's own default floor. That
## is what the rung claim below is really testing.
const SHED_STALLED_LABEL := "the bench stalled — too few workers"

const SHED_STALLED_DETAIL := "status=stalled reason=too_few_workers kind=bench workers=0 lost=2 band=5"

const SHED_STALLED_BAND := 5

## **THE BENCH THINNED BUT NOT STOPPED, which is the token the new one must NOT have taken over.**
## `announce_shed_bench` reuses `status=trimmed` with `kind=bench` where hands remain, so this row
## proves the existing entry still serves the craft web rather than the new token being reached for
## on every bench line.
const SHED_BENCH_TRIMMED_LABEL := "crafters cut to 2 — too few workers"

const SHED_BENCH_TRIMMED_DETAIL := "status=trimmed reason=too_few_workers kind=bench workers=2 lost=1 band=5"

## **THE POSITIVE COMPANION, AND THE CLAIMS ABOVE ARE HOLLOW WITHOUT IT.** An ordinary `forage`
## receipt — the same KIND the two trimmed rows ride — with no `status=` token at all. It must stay
## Routine and must NOT reach the default floor, or "the shed rows are visible" would only be saying
## that the dock shows everything.
const SHED_RECEIPT_LABEL := "Ashfoot Forage x4"

## The band ids the linked rows name, sorted, which is what a press of each must hand back. **Both
## craft rows are in it**: a stalled bench and a thinned one are each a labor row the sim changed
## unasked, and a bench is staffed from the Work tab like every other crew.
## **STATED IN SORTED ORDER**, because the claim sorts what it collected before comparing — the dock's
## draw order is the log's, not this list's, and a sorted expectation is what keeps the claim about
## WHICH bands were asked rather than about the order they came back in. Band 5 twice: both craft rows
## name the same band, and a set would have hidden the second one.
const SHED_LINK_BANDS: Array[int] = [SHED_TRIMMED_LINKED_BAND, SHED_STALLED_BAND,
	SHED_STALLED_BAND, SHED_LAPSED_BAND]

## **THE NARROWEST A DRAWN LINK MAY BE, and this is a regression floor rather than a design figure.**
## The link shipped for one build with `clip_text` set — which keeps a `Button`'s text out of its
## minimum size, and beside a label that is `SIZE_EXPAND_FILL` therefore allocated it exactly ZERO
## pixels. Every count-and-press claim below passed on that build; the frame showed no link at all.
## Well under the word at the row's detail type size, and unreachably far above the zero.
const WORK_LINK_MIN_DRAWN_WIDTH := 20.0

func _event_dock_shed_fixture() -> Array:
	return [
		{"tick": 84, "kind": "forage", "faction": 0,
			"label": SHED_RECEIPT_LABEL, "detail": "", "seq": 951},
		{"tick": 84, "kind": "forage", "faction": 0,
			"label": SHED_TRIMMED_LINKLESS_LABEL, "detail": SHED_TRIMMED_LINKLESS_DETAIL, "seq": 952},
		{"tick": 84, "kind": "hunt", "faction": 0,
			"label": SHED_TRIMMED_LINKED_LABEL, "detail": SHED_TRIMMED_LINKED_DETAIL, "seq": 953},
		{"tick": 84, "kind": "forage", "faction": 0,
			"label": SHED_LAPSED_LABEL, "detail": SHED_LAPSED_DETAIL, "seq": 954},
		{"tick": 84, "kind": "forage", "faction": 0,
			"label": SHED_PRUNED_LABEL, "detail": SHED_PRUNED_DETAIL, "seq": 955},
		# The two CRAFT rows, in the SAME frame as the four above — the third rung has to be visible
		# beside the other two, or "they read apart" is a claim about a picture nobody took.
		{"tick": 84, "kind": "craft", "faction": 0,
			"label": SHED_STALLED_LABEL, "detail": SHED_STALLED_DETAIL, "seq": 956},
		{"tick": 84, "kind": "craft", "faction": 0,
			"label": SHED_BENCH_TRIMMED_LABEL, "detail": SHED_BENCH_TRIMMED_DETAIL, "seq": 957},
	]

## The tick one RETAINED row carries — the stamp `note_system` took off the dock's current turn.
## Read off the accumulator like its label twin, because the claim is about the stamp applied at
## ingest and not about which turn group the log happens to be drawing.
func _preview_event_tick(dock: EventDockPanel, label: String) -> int:
	for event in dock._events:
		if String(event["label"]) == label:
			return int(event["tick"])
	return EVENT_DOCK_TICK_ABSENT

## The two FULL-FRAME fixtures for the current-turn ordering, as `Main._apply_snapshot` sees a frame.
##
## The resync one is the reported failure made concrete: a resync at turn 500 whose newest retained
## event is five turns old. The gap is what makes the two orders answer differently — with the stamp
## taken before the clear, the dock's current turn decays to 495.
##
## The retention window is stated, and it is deliberately NOT `DEFAULT_RETENTION_TURNS`: an unchanged
## value early-outs of `set_retention_turns`, which would leave that step of the sequence inert here.
## It is wide enough that the turn-495 row survives the prune at turn 500 — the premise the first
## assertion checks rather than assumes.
const EVENT_DOCK_TICK_ABSENT := -9999

const EVENT_DOCK_RESYNC_TURN := 500

const EVENT_DOCK_RESYNC_EVENT_TICK := 495

const EVENT_DOCK_RESYNC_RETENTION := 30

const EVENT_DOCK_RESYNC_SEQ := 901

const EVENT_DOCK_RESYNC_LABEL := "The herd moved north"

const EVENT_DOCK_RESYNC_NOTE_LABEL := "Command socket restored"

## The empty-ring frame: a later turn, a ring the retention window has emptied. Nothing is ingested,
## so only the sequence itself can leave the dock knowing what turn it is.
const EVENT_DOCK_EMPTY_RING_TURN := 512

const EVENT_DOCK_EMPTY_RING_NOTE_LABEL := "Resync requested (unapplicable delta)"

func _event_dock_resync_frame() -> Dictionary:
	return {
		"turn": EVENT_DOCK_RESYNC_TURN,
		"command_events_retention_turns": EVENT_DOCK_RESYNC_RETENTION,
		"command_events": [{"tick": EVENT_DOCK_RESYNC_EVENT_TICK, "kind": "migrated", "faction": 0,
			"label": EVENT_DOCK_RESYNC_LABEL, "detail": "", "seq": EVENT_DOCK_RESYNC_SEQ}],
	}

func _event_dock_empty_ring_frame() -> Dictionary:
	return {
		"turn": EVENT_DOCK_EMPTY_RING_TURN,
		"command_events_retention_turns": EVENT_DOCK_RESYNC_RETENTION,
		"command_events": [],
	}

## The rollback fixture pair: two batches REUSING the same `seq` values with different labels, which
## is exactly what a restored `CommandEventLog` replays (its `next_seq` counter is checkpoint state).
const EVENT_DOCK_ROLLBACK_SEQ := 501

const EVENT_DOCK_ROLLBACK_BEFORE_LABEL := "Hunters brought back red deer"

const EVENT_DOCK_ROLLBACK_AFTER_LABEL := "The hunt came home empty"

func _event_dock_rollback_before() -> Array:
	return [{"tick": 60, "kind": "hunt", "faction": 0,
		"label": EVENT_DOCK_ROLLBACK_BEFORE_LABEL, "detail": "", "seq": EVENT_DOCK_ROLLBACK_SEQ}]

func _event_dock_rollback_after() -> Array:
	return [{"tick": 60, "kind": "hunt", "faction": 0,
		"label": EVENT_DOCK_ROLLBACK_AFTER_LABEL, "detail": "", "seq": EVENT_DOCK_ROLLBACK_SEQ}]

## The IGNORED-KIND fixture (`HudEventVocab.IGNORED_KINDS`). Each inlet gets its own label, so a leak
## names the inlet that leaked rather than merely the kind.
##
## **The `ingest_events` row is CONSTRUCTED, not quoted**, the way the digit-boundary trap is: today's
## sim emits no `command_echo` — every one of them is minted client-side and arrives through
## `note_system` — but a filter that covered one inlet and not the other would be a trap the moment a
## mod or a later sim wrote the kind onto the wire, so the harness reaches the case the code claims.
const EVENT_DOCK_ECHO_INGEST_LABEL := "Advance 1 turn."

const EVENT_DOCK_ECHO_NOTE_LABEL := "Answered the question."

## The POSITIVE COMPANIONS, in the same batch: a genuine System-channel fault and an ordinary world
## event. Without them every assertion below passes on a dock that ignores everything.
const EVENT_DOCK_SYSTEM_FAULT_LABEL := "Command endpoint unavailable."

const EVENT_DOCK_ECHO_COMPANION_LABEL := "A wolf pack raided the camp"

## The seq the ignored row carries, re-used afterwards by a row that must land — proving the drop
## happened BEFORE the de-duplication rather than after it.
const EVENT_DOCK_ECHO_SEQ := 701

const EVENT_DOCK_ECHO_SEQ_REUSE_LABEL := "A child came of age"

func _event_dock_ignored_kind_fixture() -> Array:
	return [
		{"tick": 63, "kind": HudEventVocab.KIND_COMMAND_ECHO, "faction": 0,
			"label": EVENT_DOCK_ECHO_INGEST_LABEL, "detail": "", "seq": EVENT_DOCK_ECHO_SEQ},
		{"tick": 63, "kind": "predator_raid", "faction": 0,
			"label": EVENT_DOCK_ECHO_COMPANION_LABEL, "detail": "", "seq": 702},
	]

func _event_dock_ignored_seq_reuse_fixture() -> Array:
	return [{"tick": 63, "kind": "came_of_age", "faction": 0,
		"label": EVENT_DOCK_ECHO_SEQ_REUSE_LABEL, "detail": "", "seq": EVENT_DOCK_ECHO_SEQ}]

## Two rows carrying the SENTINEL `seq` of 0 and differing only in label. Keyed on `seq` they would
## collide onto one; routed to the signature fallback they are two.
const EVENT_DOCK_ZERO_SEQ_ROWS := 2

func _event_dock_zero_seq_fixture() -> Array:
	return [
		{"tick": 61, "kind": "forage", "faction": 0, "label": "An unsequenced row", "detail": "", "seq": 0},
		{"tick": 61, "kind": "forage", "faction": 0, "label": "A second unsequenced row", "detail": "", "seq": 0},
	]

## The band-relabel fixture. The roster knows `band=3` as `Band 1` (its ROSTER POSITION, not its id)
## and `band=30` as `Band 2`, and knows nothing of `band=9`.
##
## **The third row is the DIGIT-BOUNDARY trap, and it is CONSTRUCTED rather than quoted.** The sim
## names exactly one band per label today (`systems::population::push_migration_events` writes
## `"4 left Band 3"`), so no live event reaches the trap — but a plain `String.replace` of `Band 3`
## finds the `Band 3` inside `Band 30` first and corrupts the label to `Band 10`, which is a bug
## waiting for the first label that names two bands (a split or a merge is the obvious next one).
## A fixture that cannot reach the state it claims makes the assertion decorative, so this one
## reaches it. Note the honest limitation it also pins: only the band the `band=` token NAMES is
## substituted — the second band keeps whatever the sim called it.
const EVENT_DOCK_BAND_LABELS := {"3": "Band 1", "30": "Band 2"}

const EVENT_DOCK_RELABELLED := "A child came of age in Band 1"

const EVENT_DOCK_UNKNOWN_BAND_LABEL := "A child came of age in Band 9"

const EVENT_DOCK_DIGIT_BOUNDARY_LABEL := "Four left Band 1 for Band 30"

## Both roles re-labelled in ONE line (arc #527): the sender through `band=`, the destination through
## `destination=`. `Band 3` → `Band 1` and `band 30` → `Band 2`, off the same roster.
const EVENT_DOCK_SHIPMENT_RELABELLED := "Band 1 delivered 12 food to Band 2"

func _event_dock_band_label_fixture() -> Array:
	return [
		{"tick": 62, "kind": "came_of_age", "faction": 0,
			"label": "A child came of age in Band 3", "detail": "band=3 count=1", "seq": 601},
		# A shipment landing (arc #527): the SENDING band in the sim's capitalised spelling
		# (`systems::population::band_label`) and the RECEIVING one in the lower-case spelling
		# `ExpeditionMission::destination_display` falls back to, with both ids in `detail`. One line
		# carrying BOTH tokens is the only shape that can show the two roles resolving separately
		# rather than one overwriting the other.
		{"tick": 62, "kind": "trade_delivered", "faction": 0,
			"label": "Band 3 delivered 12 food to band 30",
			"detail": "status=delivered band=3 destination=30 expedition=7", "seq": 604},
		{"tick": 62, "kind": "came_of_age", "faction": 0,
			"label": "A child came of age in Band 9", "detail": "band=9 count=1", "seq": 602},
		{"tick": 62, "kind": "migrated", "faction": 0,
			"label": "Four left Band 3 for Band 30", "detail": "band=3 count=4 direction=out", "seq": 603},
	]

## The PIN fixture: one Alert, deliberately OLD, under enough newer Notable rows that a 4-row bar
## cannot reach it on chronology alone. That is the whole test — the raid must claim the leading slot
## rather than being pushed off by the receipts that followed it.
func _event_dock_pin_fixture() -> Array:
	return [
		{"tick": 40, "kind": "predator_raid", "faction": 0, "label": "Grey wolves took two from Ashfoot", "detail": "killed=2.000 wounded=1.000 warriors=3 species=Grey Wolf", "seq": 101},
		{"tick": 41, "kind": "came_of_age", "faction": 0, "label": "A child came of age in Ashfoot", "detail": "count=1", "seq": 102},
		{"tick": 42, "kind": "site_discovered", "faction": 0, "label": "The Weeping Arch", "detail": "at=18,31", "seq": 103},
		{"tick": 43, "kind": "died", "faction": 0, "label": "An elder died of cold in Windhollow", "detail": "cause=cold", "seq": 104},
		{"tick": 44, "kind": "migrated", "faction": 0, "label": "Four left Ashfoot for Windhollow", "detail": "count=4 direction=out", "seq": 105},
		{"tick": 45, "kind": "expedition_arrived", "faction": 0, "label": "Expedition reached 24,9 — awaiting orders", "detail": "", "seq": 106},
		{"tick": 46, "kind": "tame", "faction": 0, "label": "The aurochs herd has grown tame", "detail": "", "seq": 107},
	]

## ---- BAND FISSION — a split, and a split REFUSED (issue #511) ---------------------------------
## The split's two rows, spelled out here so the assertions compare against strings the chapter
## states rather than against strings recomposed through the code under test.

## The DEED's label and its detail, in the sim's own shape (`server.rs handle_split_band`): every
## token numeric, so none has to be last. Both halves stand on the parent's tile — a split has no
## destination — so `x`/`y` name the one place the two bands share.
const FOUNDING_LABEL := "Ashfoot split off a new band of 6 workers at (39, 26)"

const FOUNDING_DETAIL := "status=split band=71257 parent=71204 x=39 y=26 workers=6 share=0.375 provisions=12.75"

## The REFUSAL, which is how a worker count the sim will not honour reaches the player.
## `emit_command_failure` writes `"<Kind display> failed"` as the label and the sim's explanation as
## the detail — PROSE, not tokens, which is the case the dock's prose branch exists for. Two
## sentences because `SplitRefusals::explanation` reports EVERY applicable refusal, never the first.
const FOUNDING_REFUSAL_LABEL := "Band founded failed"

const FOUNDING_REFUSAL_DETAIL := "Windhollow cannot split — a new band starts with 4 workers and this one would have 2. the home band would keep 3 workers, below its floor of 5."

func _event_dock_founding_fixture() -> Array:
	return [
		{"tick": 71, "kind": "band_founded", "faction": 0,
			"label": FOUNDING_LABEL, "detail": FOUNDING_DETAIL, "seq": 901},
		{"tick": 71, "kind": "band_founded", "faction": 0,
			"label": FOUNDING_REFUSAL_LABEL, "detail": FOUNDING_REFUSAL_DETAIL, "seq": 902},
	]

## Assert the event bar clears one HUD region — **and that the claim is not vacuous**.
##
## The HUD's regions occupy different vertical bands, so most bar/region pairs share no `y` at all
## and "these two rects do not intersect" is trivially true of them: a BOTTOM bar cannot reach the
## top-bar readouts however wrong its horizontal bound is. A block of such claims passes with the fix
## reverted, which is the failure this guard exists to prevent — so the overlap on the PERPENDICULAR
## axis is required first, and a pair that does not share one fails as VACUOUS rather than passing.
## Every string the dock currently RENDERS — bar rows, log rows, chips, the foot — as flat text.
## The raw-token guard walks this rather than the event records, because the records are supposed to
## hold `key=value`: the claim is about what reaches the screen.
## Does this rendered string carry a trailing-zero decimal — `2.000`, `1.50`? The wire's `{:.3}`
## casualty format produces them and a rendered row must not. Stated as a PROPERTY of any numeric
## word rather than a list of known strings, so a new `{:.N}` field on a future kind is covered
## without an edit here. `is_valid_float` is the precision that matters: without it an endpoint like
## `127.0.0.1:41000` in a system note would read as a padded decimal and fail for nothing.
func _has_padded_decimal(text: String) -> bool:
	for word in text.split(" ", false):
		if word.contains(".") and word.ends_with("0") and word.is_valid_float():
			return true
	return false

## The harness's own backdrop `ColorRect` (the mid-tone ground every frame renders against), found
## by walking rather than held, so the bright-terrain state cannot go stale against a `_ready` edit.
func _preview_backdrop() -> ColorRect:
	for child in h.get_children():
		if not (child is CanvasLayer):
			continue
		for grandchild in child.get_children():
			if grandchild is ColorRect:
				return grandchild as ColorRect
	return null

## Nine points across a rect — the four corners, the four edge midpoints and the centre, each pulled
## `RECT_PROBE_INSET` inside so a sample sits within the rect rather than on its boundary.
##
## The centre alone is not enough for the click-through test: it lands on an event row, which is a
## `PanelContainer` and consumes by default, so the claim passes even with the root and the card set
## to `IGNORE`. The margins and the strip beside the expander are where a press would actually fall
## through, and those are corners and edges.
func _preview_rect_probe_points(rect: Rect2) -> Array[Vector2]:
	var lo := rect.position + Vector2(RECT_PROBE_INSET, RECT_PROBE_INSET)
	var hi := rect.end - Vector2(RECT_PROBE_INSET, RECT_PROBE_INSET)
	var mid := rect.get_center()
	return [
		Vector2(lo.x, lo.y), Vector2(mid.x, lo.y), Vector2(hi.x, lo.y),
		Vector2(lo.x, mid.y), mid, Vector2(hi.x, mid.y),
		Vector2(lo.x, hi.y), Vector2(mid.x, hi.y), Vector2(hi.x, hi.y),
	]

## How far inside a rect a probe point sits. Two canvas px — enough to be unambiguously within the
## rect after the canvas→window scale, small enough to still land in a 4px content margin.
const RECT_PROBE_INSET := 2.0

## Did a left-press at this WINDOW point survive the GUI pass and reach `_unhandled_input`?
##
## `MapView` picks hexes there, so "reaches it" is exactly "would have selected the hex underneath".
## Driven with `Viewport.push_input`, which runs the real dispatch — GUI picking first, unhandled
## after — rather than inspecting hover state, which this harness does not maintain.
func _preview_press_reaches_map(window_point: Vector2) -> bool:
	h._unhandled_press_seen = false
	var press := InputEventMouseButton.new()
	press.button_index = MOUSE_BUTTON_LEFT
	press.pressed = true
	press.position = window_point
	h.get_viewport().push_input(press)
	await h.get_tree().process_frame
	return h._unhandled_press_seen

## Canvas coordinates → WINDOW coordinates, which is what `Input.warp_mouse` takes. `project.godot`
## stretches `canvas_items` from a 1920 base with an `expand` aspect, so a control's own rect and the
## cursor live in different units and a warp using the raw rect lands somewhere else entirely.
## The arithmetic itself lives in `InputProbe`, which the crafting chapter's gesture probe reads too —
## a second copy is a second chance for two harness claims to disagree about where a point is.
func _canvas_to_window(canvas_point: Vector2) -> Vector2:
	return InputProbe.canvas_to_window(h.get_viewport(), h.get_window(), canvas_point)

## The shared pointer-input layer.
const InputProbe := preload("res://tools/ui_preview/input_probe.gd")

func _preview_dock_labels(dock: EventDockPanel) -> Array[String]:
	var found: Array[String] = []
	var stack: Array[Node] = [dock._rows, dock._log_body]
	while not stack.is_empty():
		var node: Node = stack.pop_back()
		if node == null:
			continue
		for child in node.get_children():
			stack.append(child)
		if node is Label:
			found.append((node as Label).text)
	return found

func _assert_bar_clears(dock: EventDockPanel, region: Control, what: String) -> void:
	var bar := dock._root.get_global_rect()
	var box := region.get_global_rect()
	if bar.position.y >= box.end.y or box.position.y >= bar.end.y:
		h._assert_hud("VACUOUS — the bar and %s share no vertical band, so 'they do not overlap' claims nothing" % what, false)
		return
	h._assert_hud("the bar clears %s (they share a vertical band, so this is a real claim)" % what,
		not bar.intersects(box))

## The harness's stand-in for `Main._update_event_dock_edge_offset`: Σ sizes of every reserver sitting
## on the edge the bar is docked to. `Main` is never instanced here, so the sum is restated — but it
## reads the live panel's own `get_dock()` / `current_reservation_size()`, so a panel that changes
## edge, collapses or hides moves the bar here exactly as it does live. **No priority test**, matching
## `Main`: the dock reserves nothing, so it is always the innermost thing on its edge.
func _preview_push_event_dock_edge_offset(dock: EventDockPanel, reservers: Array) -> void:
	var offset := 0.0
	for reserver: BandCityPanel in reservers:
		if int(reserver.get_dock()) == dock.get_dock():
			offset += float(reserver.current_reservation_size())
	dock.set_edge_offset(offset)

## The CO-EDGE non-overlap claim: the bar and a panel docked to the SAME horizontal edge. Stated as a
## rect test rather than judged from a PNG — an overlapping strip renders a perfectly plausible bar,
## which is exactly how this reached live play.
##
## Vacuity is guarded on the HORIZONTAL band here, the opposite axis to `_assert_bar_clears`: two
## things on one horizontal edge trivially share a vertical band, so the question that can be answered
## for free is whether their x-spans meet. The strip is centred and capped, so a panel narrower than
## the gap either side would make this claim about nothing.
## THE OFFSET IS ONLY AS FRESH AS THE PANEL'S LAST **PUBLICATION**, and every co-edge claim above
## reads `current_reservation_size()` live at the moment it pushes — which cannot see a stale one.
##
## `Main` does not poll. It keeps `_reservations`, written ONLY by `reservation_changed`, and
## `_update_event_dock_edge_offset` sums that dictionary. So a size the panel DRAWS at but never
## published is a bar placed by the old number — and the panel has two setters that relayout without
## emitting (`set_lateral_bounds`, `set_rail_width`), both of which feed `_available_card_span()` and
## therefore the SHELL, and therefore — since the strip's cross axis carries the active shell's own
## chrome — the reserved size itself. Measured on a TOP dock: the panel drew 395 while `Main` held
## 360, and the bar sat 35px inside the card.
##
## **SO THIS BLOCK CONSUMES THE SIGNAL RATHER THAN CALLING THE GETTER.** That is the whole difference
## between it and the frames above; wired the other way it passes with the defect in place.
##
## **THE FLIP IS DRIVEN BY THE LATERAL BOUNDS, not by the interface scale**, because that is the
## general mechanism and it is reachable at `ui_scale` 1.0: `Main` re-pushes those bounds every
## snapshot on a TOP dock, and on this canvas they take the card's span from 1920 (wide shell) to
## ~1141 (narrow). The scale is only how a player reaches the same shell on a 1920 monitor. The high
## scale is then walked as well, because it is the configuration that was reported — and it is
## restored, and the restore asserted, before this returns.
func _assert_shell_flip_republishes(dock: EventDockPanel, panel: BandCityPanel) -> void:
	# `Main._reservations` + `_update_event_dock_edge_offset`, restated — fed by what the panel
	# PUBLISHES and by nothing else.
	var published := {"edge": panel.get_dock(), "size": panel.current_reservation_size()}
	var record := func(edge: int, size: float) -> void:
		published["edge"] = edge
		published["size"] = size
		dock.set_edge_offset(size if edge == dock.get_dock() else 0.0)
	panel.reservation_changed.connect(record)

	for edge in SHELL_FLIP_EDGES:
		for scale_variant in SHELL_FLIP_SCALES:
			await _walk_shell_flip(dock, panel, published, float(scale_variant), edge)

	panel.reservation_changed.disconnect(record)
	dock.set_edge_offset(0.0)
	h._hud.set_reserved_inset(&"band_panel", SIDE_TOP, 0.0)
	h._hud.set_reserved_inset(&"band_panel", SIDE_BOTTOM, 0.0)
	await h._settle()
	h._assert_hud("shell flip: the interface scale is restored, so no later frame inherits it",
		is_equal_approx(h.get_window().content_scale_factor, float(ClientSettings.UI_SCALE_DEFAULT)))

## One (edge, scale) pass: stand the panel in the WIDE shell, push the bounds `Main` pushes, and
## require that the bar followed the size the panel now draws at.
func _walk_shell_flip(dock: EventDockPanel, panel: BandCityPanel, published: Dictionary,
		scale: float, edge: int) -> void:
	var label := "%s at scale %.2f" % ["TOP" if edge == SIDE_TOP else "BOTTOM", scale]
	# The MEMBER, never `set_ui_scale` — the setter writes the developer's own config file.
	ClientSettings.ui_scale = scale
	ClientSettings.changed.emit()
	# **THE BOUNDS ARE CLEARED BEFORE THE DOCK CHANGE, NOT AFTER, and the order is the claim's.**
	# `set_dock` publishes unconditionally, so clearing afterwards leaves the panel's last PUBLISHED
	# size the narrow one it happened to be carrying from the previous pass — and the flip below then
	# lands back on that same number, so the stale-publication defect passes by coincidence. Measured:
	# with the republish removed, the BOTTOM pass went green while the TOP one failed. Cleared first,
	# `set_dock` publishes the WIDE size and the flip has somewhere to be stale FROM.
	panel.set_lateral_bounds(0.0, 0.0)
	panel.set_dock(edge)
	dock.set_dock(edge)
	# `Main._on_event_dock_dock_changed` — the bar's OWN edge changing re-runs the offset sum, since it
	# changes which reservers it has to clear. Without it the offset recorded while the bar was still on
	# the other edge (zero, correctly) stands, and every claim below fails on the harness's wiring
	# rather than on the panel's.
	dock.set_edge_offset(float(published["size"]) if int(published["edge"]) == dock.get_dock() else 0.0)
	await h._settle()
	h._hud.set_reserved_inset(&"band_panel", edge, published["size"])
	await h._settle()
	var wide_strip: float = panel._root.get_global_rect().size.y

	# THE PER-SNAPSHOT RE-PUSH `Main._update_band_panel_lateral_bounds` makes on a TOP dock. On a
	# BOTTOM dock `Main` pushes zeroes (the HUD yields that strip), so the flip is driven there by the
	# SAME setter with the same numbers — the claim is about the setter, not about which edge supplies
	# the widths.
	var columns: Vector2 = h._hud.lateral_column_widths()
	panel.set_lateral_bounds(columns.x, columns.y)
	await h._settle()
	h._hud.set_reserved_inset(&"band_panel", edge, published["size"])
	await h._settle()
	var narrow_strip: float = panel._root.get_global_rect().size.y

	# The reported configuration, rendered: both docked to one horizontal edge with the panel in the
	# narrow shell at a high interface scale. The assertions below carry the claim — an overlapping bar
	# renders a perfectly plausible strip — but this is the one picture of the case, and the co-edge
	# frames above are all the WIDE shell at 1.0.
	await h._save("event_dock_shell_flip_%s" % ["top" if edge == SIDE_TOP else "bottom"])

	# PRECONDITION. Without it every claim below passes on a panel whose shell never moved — which is
	# most of this harness's canvases, and exactly why the defect had no failing state.
	h._assert_hud("shell flip %s: the bounds really did flip the shell (strip %.0f → %.0f)"
			% [label, wide_strip, narrow_strip],
		not is_equal_approx(wide_strip, narrow_strip))
	# THE CLAIM. `published` is the last thing `reservation_changed` carried; the rect is what the
	# panel draws. A gap between them is a bar placed by a number nobody is drawing.
	h._assert_hud("shell flip %s: the panel published the size it now draws (%.0f published, %.0f drawn)"
			% [label, float(published["size"]), narrow_strip],
		is_equal_approx(float(published["size"]), narrow_strip))
	# …AND ITS CONSEQUENCE, which is the reported defect: the bar sits entirely past the card.
	var bar := dock._root.get_global_rect()
	var card := panel._panel.get_global_rect()
	if edge == SIDE_TOP:
		h._assert_hud("shell flip %s: the bar begins at or past the card's far edge (bar top %.0f, card bottom %.0f)"
				% [label, bar.position.y, card.end.y],
			bar.position.y >= card.end.y - CO_EDGE_RECT_EPSILON)
	else:
		h._assert_hud("shell flip %s: the bar ends at or before the card's near edge (bar bottom %.0f, card top %.0f)"
				% [label, bar.end.y, card.position.y],
			bar.end.y <= card.position.y + CO_EDGE_RECT_EPSILON)
	_assert_bar_clears_co_edge(dock, panel, "the %s-docked panel after a shell flip" % [
		"TOP" if edge == SIDE_TOP else "BOTTOM"])

	# **THE NARROW SHELL'S CARD TAKES THE WHOLE AVAILABLE SPAN** — defect (4)'s claim, asserted where
	# the narrow shell on a horizontal dock actually is. `_card_width()` says so in its docstring
	# ("it is reached only when there is too little room for three zones, i.e. exactly when there is
	# nothing to give back"), and nothing checked it; the wide shell's centred content-width card is
	# the OTHER branch and is untouched. The remaining gap at the card's leading edge is
	# `_bound_leading`, the HUD column this card must not be drawn over — which is why the claim is
	# span equality and not "the card reaches the screen edge".
	h._assert_hud("shell flip %s: the narrow card fills its available span (%.0f drawn of %.0f available, leading bound %.0f)"
			% [label, card.size.x, panel._available_card_span(), panel._bound_leading],
		is_equal_approx(card.size.x, panel._available_card_span()))

	ClientSettings.ui_scale = ClientSettings.UI_SCALE_DEFAULT
	ClientSettings.changed.emit()
	await h._settle()

func _assert_bar_clears_co_edge(dock: EventDockPanel, panel: BandCityPanel, what: String) -> void:
	var bar := dock._root.get_global_rect()
	var box := panel._root.get_global_rect()
	if bar.position.x >= box.end.x or box.position.x >= bar.end.x:
		h._assert_hud("VACUOUS — the bar and %s share no horizontal band, so 'they do not overlap' claims nothing" % what, false)
		return
	h._assert_hud("co-edge: the bar clears %s — bar %s vs panel %s" % [what, bar, box],
		not bar.intersects(box))

## THE DISPLACED STRIP MUST STILL LAND ON SCREEN. `_apply_dock_layout` places the bar at
## `[_edge_offset, _edge_offset + cross]` in from its own rim, and the two heights are set by SEPARATE
## clamps — `BandCityPanel.MAX_WIDE_HEIGHT_FRACTION` (0.6) and `EventDockPanel.MAX_STRIP_HEIGHT_FRACTION`
## (0.5) — which sum to 1.1 of the viewport with nothing bounding the pair. What actually holds the
## line is that BOTH fractions are dominated by absolute caps: `PANEL_HEIGHT_WIDE` (360) plus the
## tallest strip the dock builds (a 1-row title bar + `LOG_HEIGHT` + the section gap = 304) is **664**,
## against a layout height that never drops below **1080** — `project.godot` stretches `canvas_items`
## from a 1920×1080 base with an `expand` aspect, so a short WINDOW yields a wide canvas, never a short
## one, and `_viewport_size()` is floored at the base height. So the two fractions are never both
## binding and this claim has real margin today. **The margin is therefore PRINTED**: a strip ending
## 2px inside the viewport and one ending 400px inside are the same green line otherwise, and the
## whole point of the assertion is to notice if `LOG_HEIGHT` or `PANEL_HEIGHT_WIDE` ever grows into it.
##
## Judged as a RECT, never from the frame — a strip whose "Earlier turns" footer has fallen off the
## bottom of the window renders an entirely plausible log above it.
func _assert_strip_within_viewport(dock: EventDockPanel, what: String) -> void:
	var bar := dock._root.get_global_rect()
	var viewport_height: float = dock._viewport_size().y
	var slack: float = viewport_height - (dock._edge_offset + dock._cross_axis_size())
	h._assert_hud("%s: offset %.0f + strip %.0f stays inside the %.0f-px viewport (%.0f px of slack; bar rect %.0f..%.0f)"
			% [what, dock._edge_offset, dock._cross_axis_size(), viewport_height, slack,
				bar.position.y, bar.end.y],
		slack >= 0.0 and bar.position.y >= -CO_EDGE_RECT_EPSILON
			and bar.end.y <= viewport_height + CO_EDGE_RECT_EPSILON)

func run(harness) -> void:
	h = harness

	# ---- THE EVENT DOCK (issue #272) ---------------------------------------------------------
	# Its own CanvasLayer, injected here and freed below, so it exists only for the frames that judge
	# it. The reservation is pushed into the HUD by hand — `Main` owns that fan-out and `Main` is
	# never instanced here — so the frames show the HUD reflowing off the reserved strip exactly as
	# it does live.
	h._hud.clear_selection()
	h._hud._selection._selected_tile_info.clear()
	await h._settle()
	var event_dock: EventDockPanel = EVENT_DOCK_SCENE.instantiate()
	h.add_child(event_dock)
	await h.get_tree().process_frame
	_preview_push_event_dock_insets(event_dock, 0.0, 0.0)

	# **THE DOCK RESERVES NOTHING FROM EITHER SURFACE.** Asserted as the absence of the API rather
	# than as a zero it might publish: a reservation of 0 and no reservation at all look identical
	# from the outside, and the bug this replaces (a full-width `SIDE_TOP` reservation behind a
	# centre-bounded strip, so the map dropped and the ends came back as black bars) was a live
	# reservation, not a zero one. If either member returns, `Main` can be wired back to it by
	# reflex and nothing else here would notice.
	h._assert_hud("the dock publishes no `reservation_changed` — it overlays the map, it does not reserve",
		not event_dock.has_signal("reservation_changed"))
	h._assert_hud("…and no `current_reservation_size` for anything to fan out",
		not event_dock.has_method("current_reservation_size"))

	# THE ROLLBACK REGRESSION. `CommandEventLog` is checkpoint state, so a rollback restores it
	# INCLUDING its `next_seq` counter and the replayed events REUSE sequence numbers the client has
	# already seen. A rollback publishes a FULL frame, which is why `Main` clears the dock on every
	# full snapshot BEFORE applying its events — without that clear the dock suppresses every replayed
	# row as a duplicate `seq` and goes on showing a plausible but stale log, silently. Drive exactly
	# that: a batch, then `reset()` (what the full frame does), then rows REUSING those `seq` values
	# with different labels. The new labels must be what the dock holds.
	event_dock.reset()
	event_dock.ingest_events(_event_dock_rollback_before())
	h._assert_hud("rollback: the pre-rollback batch is held",
		_preview_event_label_count(event_dock, EVENT_DOCK_ROLLBACK_BEFORE_LABEL) == 1)
	event_dock.reset()
	event_dock.ingest_events(_event_dock_rollback_after())
	h._assert_hud("rollback: a replayed event REUSING a seen `seq` is shown, not swallowed as a duplicate",
		_preview_event_label_count(event_dock, EVENT_DOCK_ROLLBACK_AFTER_LABEL) == 1)
	h._assert_hud("rollback: …and the pre-rollback row it replaced is gone, not stacked beside it",
		_preview_event_label_count(event_dock, EVENT_DOCK_ROLLBACK_BEFORE_LABEL) == 0)

	# THE FULL FRAME'S TURN IS THE DOCK'S CURRENT TURN — the ordering twin of the clear above.
	# `reset()` sets `_current_turn = -1` and `set_current_turn` only ever RAISES it, so stamping the
	# turn BEFORE the clear is simply erased: the dock's idea of "now" then decays to the newest
	# RETAINED event's tick — or `-1` on an empty ring, where `_prune()` no-ops entirely — and a
	# client-side `note_system` posted before the next frame is stamped, and grouped in the expanded
	# log, under a turn it did not happen on.
	#
	# **Driven through `Main.apply_event_dock_frame`, the shipped sequence itself.** An assertion that
	# re-typed reset → turn → retention → ingest here would pass on whatever order `Main` chose, which
	# is the only thing under test. Read off `_current_turn` and the stored row's `tick`, never off the
	# rendered rows: the stamp is applied at ingest, and a render-scoped read narrows to whichever turn
	# groups the log happens to be showing.
	h.MAIN_SCRIPT.apply_event_dock_frame(event_dock, _event_dock_resync_frame(), false)
	# The premise first. If the frame's own event were pruned away, "the newest retained event's tick"
	# would be a claim about an empty ring and the turn assertion below would prove nothing.
	h._assert_hud("resync: the full frame's own event is retained",
		_preview_event_label_count(event_dock, EVENT_DOCK_RESYNC_LABEL) == 1)
	h._assert_hud("full snapshot: the dock's current turn is the FRAME's turn, not the newest event's tick",
		event_dock._current_turn == EVENT_DOCK_RESYNC_TURN)
	event_dock.note_system(EVENT_DOCK_RESYNC_NOTE_LABEL)
	h._assert_hud("…so a client note posted after that frame is stamped with the frame's turn",
		_preview_event_tick(event_dock, EVENT_DOCK_RESYNC_NOTE_LABEL) == EVENT_DOCK_RESYNC_TURN)
	# THE EMPTY-RING CASE, which no ingested event can rescue: nothing is appended, so the current turn
	# is whatever the sequence left behind — the frame's turn, or the `-1` that renders as `T—`.
	h.MAIN_SCRIPT.apply_event_dock_frame(event_dock, _event_dock_empty_ring_frame(), false)
	h._assert_hud("full snapshot with an EMPTY ring: the current turn is still the frame's turn",
		event_dock._current_turn == EVENT_DOCK_EMPTY_RING_TURN)
	event_dock.note_system(EVENT_DOCK_EMPTY_RING_NOTE_LABEL)
	h._assert_hud("…and a note on an empty ring is stamped with it, not left unstamped",
		_preview_event_tick(event_dock, EVENT_DOCK_EMPTY_RING_NOTE_LABEL) == EVENT_DOCK_EMPTY_RING_TURN)
	# Hand the retention window back to the shipped default: the frames below print it in the expanded
	# log's footer, and this block's fixture deliberately states a different one to keep the sequence's
	# retention step live rather than early-outing on an unchanged value.
	event_dock.set_retention_turns(EventDockPanel.DEFAULT_RETENTION_TURNS)

	# A `seq` of ZERO is a SENTINEL, not a key: it is the FlatBuffers default and means the row never
	# went through `CommandEventLog::push`. Keyed on, every such row would collide onto one. Two rows
	# that differ only in label must therefore both survive.
	event_dock.reset()
	event_dock.ingest_events(_event_dock_zero_seq_fixture())
	h._assert_hud("seq 0 is a sentinel, not a key: two unsequenced rows do not collide",
		event_dock._events.size() == EVENT_DOCK_ZERO_SEQ_ROWS)

	# THE BAND NAME IS THE CLIENT'S. The sim writes a positional `Band <BandId>` because the snapshot
	# carries no band name; the HUD's roster says that band is `Band 1`, and the dock must say so too
	# — bounded at a digit boundary, so a `Band 3` fixture cannot rewrite the `Band 30` beside it.
	event_dock.reset()
	event_dock.set_band_labels(EVENT_DOCK_BAND_LABELS)
	event_dock.ingest_events(_event_dock_band_label_fixture())
	h._assert_hud("band label: the sim's positional `Band 3` is re-labelled to the roster's own name",
		_preview_event_label_count(event_dock, EVENT_DOCK_RELABELLED, true) == 1)
	h._assert_hud("band label: an id the roster does not know keeps the sim's own label untouched",
		_preview_event_label_count(event_dock, EVENT_DOCK_UNKNOWN_BAND_LABEL, true) == 1)
	h._assert_hud("band label: the substitution stops at a DIGIT boundary (`Band 3` ≠ `Band 30`)",
		_preview_event_label_count(event_dock, EVENT_DOCK_DIGIT_BOUNDARY_LABEL, true) == 1)
	# **A SHIPMENT'S `destination=` TAKES THE SAME SWAP** (arc #527), and it needed a SECOND spelling
	# rather than a second use of the first: the sim writes a sending band as `Band 3` and a
	# shipment's destination as `band 30` — different case, different producer — so one shared format
	# would have looked right and never fired. Both roles resolve in this one line.
	h._assert_hud("band label: a shipment's `destination=` id is re-labelled too",
		_preview_event_label_count(event_dock, EVENT_DOCK_SHIPMENT_RELABELLED, true) == 1)
	event_dock.set_band_labels({})

	# THE PREFS FILE THAT EXISTS BUT HAS NO `[events]` SECTION — i.e. every player upgrading into
	# this build, whose `narrative.cfg` already carries the voice register and `[hud_panels]`. This
	# escaped the first pass because the harness pointed the override at a path that did not exist
	# at all, so `ConfigFile.load` failed and `_load_prefs` returned before it ever read a key.
	# `channels` is the ONLY key whose absence cannot be expressed as a plain default: absent means
	# "every channel on", a stored EMPTY array means the player turned them all off, and collapsing
	# those two is what a naive `[]` default would do. Both branches are walked here.
	_write_event_prefs_without_section()
	event_dock._load_prefs()
	h._assert_hud("prefs: an existing file with no [events] section leaves every channel ON",
		_preview_event_channels_all_on(event_dock))
	_write_event_prefs_with_channels([])
	event_dock._load_prefs()
	h._assert_hud("prefs: a STORED empty channel list is all-off, not mistaken for an absent key",
		not _preview_event_channels_all_on(event_dock))
	_write_event_prefs_without_section()
	event_dock._load_prefs()

	# AN IGNORED KIND IS DROPPED AT INGEST, IN BOTH INLETS. `Advance 1 turn.` is the line that
	# produced this rule: a receipt for a button the player pressed a second ago, restated on the
	# notification bar as if it were news. It rides `command_echo` now, which
	# `HudEventVocab.IGNORED_KINDS` names, and the claim is the strong one — never STORED, not merely
	# never shown — so every assertion below reads the dock's own accumulator rather than its rows.
	#
	# THE COMPANIONS ARE THE POINT. A genuine System fault and a world event share the batch, so a
	# dock that simply ignored everything would fail here instead of passing.
	var prior_detail: String = event_dock._detail_level
	event_dock.reset()
	event_dock.ingest_events(_event_dock_ignored_kind_fixture())
	event_dock.note_system(EVENT_DOCK_ECHO_NOTE_LABEL, "", false, HudEventVocab.KIND_COMMAND_ECHO)
	event_dock.note_system(EVENT_DOCK_SYSTEM_FAULT_LABEL, "", true)
	h._assert_hud("ignored kind: the positive companion — a genuine System FAULT is held",
		_preview_event_label_count(event_dock, EVENT_DOCK_SYSTEM_FAULT_LABEL) == 1)
	h._assert_hud("ignored kind: …and so is the world event that shared the batch",
		_preview_event_label_count(event_dock, EVENT_DOCK_ECHO_COMPANION_LABEL) == 1)
	h._assert_hud("ignored kind: nothing of the kind is in the STORED pool at all",
		_preview_event_kind_count(event_dock, HudEventVocab.KIND_COMMAND_ECHO) == 0)
	h._assert_hud("ignored kind: dropped through the `ingest_events` inlet",
		_preview_event_label_count(event_dock, EVENT_DOCK_ECHO_INGEST_LABEL) == 0)
	h._assert_hud("ignored kind: dropped through the `note_system` inlet too — a filter on one is a trap",
		_preview_event_label_count(event_dock, EVENT_DOCK_ECHO_NOTE_LABEL) == 0)
	# …and it consumed no de-duplication slot on the way out: a LATER row reusing the ignored row's
	# `seq` must still land. Filtering after the de-dup would swallow this one.
	event_dock.ingest_events(_event_dock_ignored_seq_reuse_fixture())
	h._assert_hud("ignored kind: it burned no `seq` slot — a later row reusing that seq still lands",
		_preview_event_label_count(event_dock, EVENT_DOCK_ECHO_SEQ_REUSE_LABEL) == 1)
	# THE CONFIGURATION WHERE EVERY OTHER FILTER HAS GIVEN UP: the `Everything` floor with both
	# channels on. A detail-floor or channel-toggle mechanism leaks here; ignoring at ingest cannot.
	event_dock.set_detail_level(HudEventVocab.RUNG_ROUTINE)
	for channel_variant in HudEventVocab.CHANNEL_ORDER:
		event_dock.set_channel_enabled(String(channel_variant), true)
	h._assert_hud("ignored kind: still absent at the `Everything` floor with both channels on",
		_preview_visible_label_count(event_dock, EVENT_DOCK_ECHO_INGEST_LABEL) == 0
			and _preview_visible_label_count(event_dock, EVENT_DOCK_ECHO_NOTE_LABEL) == 0)
	h._assert_hud("ignored kind: …a floor at which the System fault beside it IS visible",
		_preview_visible_label_count(event_dock, EVENT_DOCK_SYSTEM_FAULT_LABEL) == 1)
	event_dock.set_detail_level(prior_detail)

	event_dock.reset()
	event_dock.ingest_events(WorldFx.event_dock_fixture())
	h._assert_hud("seq de-dup: two identical same-turn raids are TWO events, not one",
		_preview_event_kind_count(event_dock, "predator_raid") == EVENT_DOCK_DUPLICATE_RAIDS)
	# And the SIGNATURE fallback still de-dupes a row with no usable `seq`, so a mixed frame cannot
	# duplicate every row every turn. It is a degrade path, not a second mechanism — it carries the
	# old collapse-two-identical-rows bug for exactly the rows that give it no better key.
	var seqless := [{"tick": 47, "kind": "forage", "label": "A row with no seq", "detail": ""}]
	event_dock.ingest_events(seqless)
	event_dock.ingest_events(seqless)
	h._assert_hud("seq de-dup: a row carrying no usable seq still falls back to the signature",
		_preview_event_label_count(event_dock, "A row with no seq") == 1)

	# The client's own System-channel note — the Inspector's console chatter routed onto the dock,
	# in the shape `Inspector._append_command_log` emits it (the LINE is the label; there is no
	# separate detail, or the only words that matter end up at the far end of the bar).
	event_dock.note_system("Command socket lost — reconnecting", "", true)

	# event_dock_bottom — THE SHIPPED DEFAULT: bottom edge, 2 rows, the `notable` floor. Opened and
	# closed first, which is what a player does and what marks the alerts read, so this frame shows
	# the plain newest-first bar rather than the pinned one (that is its own state below).
	event_dock.set_expanded(true)
	event_dock.set_expanded(false)
	await h._settle()
	await h._save("event_dock_bottom")
	# **THE DEMOGRAPHIC RUNGS SPLIT ON HEAD-COUNT**, and both directions were reported live, so both
	# are pinned. A rung table is one dict entry away from either regression at any time.
	#   • A change to how many people the band HAS must reach the default floor. `born` shipped
	#     Routine — below `DEFAULT_DETAIL_LEVEL` — so a birth never appeared unless the player chose
	#     "Everything": a population counter ticking up while the bar said nothing.
	#   • A bracket TRANSITION must not. `came_of_age` shipped Notable and was reported as too much
	#     noise — it fires constantly while the population never moves, filling the default floor
	#     with rows that answer no question.
	# Asserted as a PAIR: either one alone passes on a table that has collapsed every demographic
	# kind onto the same rung, which is exactly the state both reports were complaining about.
	var default_floor: Array = HudEventVocab.DETAIL_FLOOR[HudEventVocab.DEFAULT_DETAIL_LEVEL]
	for kind in ["born", "died", "migrated"]:
		h._assert_hud("head-count change `%s` passes the DEFAULT detail floor" % kind,
			default_floor.has(String(HudEventVocab.RUNG_BY_KIND[kind])))
	for kind in ["came_of_age", "aged"]:
		h._assert_hud("bracket transition `%s` is BELOW the default floor — it is not news" % kind,
			not default_floor.has(String(HudEventVocab.RUNG_BY_KIND[kind])))

	# **NOTHING DOCKED LEFT OR RIGHT, AND THE BAR STILL CLEARS THE HUD'S OWN FURNITURE.** This is the
	# case the first inset fix got wrong: it bounded the bar against edge RESERVERS, and the left
	# dock, the right dock and the top-bar readout block are not reservers. Reported live as a bar
	# sitting over `Turn N` / `Units` / `Pop`.
	#
	# **EACH CLAIM IS MADE WHERE IT IS NON-VACUOUS, and `_assert_bar_clears` enforces that.** The
	# HUD's regions occupy different vertical bands, so most bar/region pairs never share any y at
	# all and "they do not overlap" is true of them for free: a BOTTOM bar sits in the BottomBar's
	# band (nav backing + turn orb), and a TOP bar in the ContentRow's — which since issue #450 deleted
	# the `TopBar` is where the two DOCKS start, the readout block that used to occupy that band having
	# gone with it. Asserting the wrong pair passes
	# with the fix reverted — which is exactly what the first version of this block did.
	_assert_bar_clears(event_dock, h._hud.nav_backing, "the bottom-left nav backing (minimap + zoom rail)")
	_assert_bar_clears(event_dock, h._hud.turn_orb, "the bottom-right turn orb")
	# THE COLUMNS ARE AUTHORED, AND THE BAR IS BOUNDED BY THE AUTHORED NUMBER. If a card or a metrics
	# string ever outgrows its column, the live rect passes the authored width and the bar's bound is
	# a lie — so it fails HERE rather than by overlapping in play.
	h._assert_hud("the LEFT column renders no wider than the authored width the bar is bounded by (%.0f)"
			% h._hud.left_column_width(),
		h._hud.left_dock_region.get_global_rect().size.x <= h._hud.left_column_width())
	h._assert_hud("the RIGHT dock renders no wider than the authored column (%.0f)" % h._hud.right_column_width(),
		h._hud.right_dock_region.get_global_rect().size.x <= h._hud.right_column_width())
	# **THE THIRD REGION IS GONE WITH THE BLOCK IT MEASURED** (issue #450). It asserted that the
	# top-bar readout block rendered no wider than the authored column; that block — `Turn N` /
	# `Units` / `Sedentarization` / `Pop` / the knowledge strip — is retired, and its 344px `TurnBlock`
	# with it, so `right_column_width()` is the dock's own authored minimum and the claim above is the
	# whole of the right side.

	# **THE REPORTED CASE, RE-AIMED AT WHAT NOW OCCUPIES THAT BAND.** A TOP bar used to share the
	# `TopBar`'s vertical band with the readout block; with the top bar deleted the RIGHT DOCK begins
	# at y = 0, so the dock is what a top bar can now be drawn over — and it is the region the bound is
	# computed from either way. Nothing is docked left or right, so a bound that only knew about
	# reservers puts the bar straight over it.
	event_dock.set_dock(SIDE_TOP)
	await h._settle()
	_assert_bar_clears(event_dock, h._hud.right_dock_region, "the HUD's right dock (the story card)")

	# **NOTHING MOVES DOWN**, asserted HERE with the dock actually on `SIDE_TOP` — `offset_top` is
	# the offset a TOP strip would move, and made on the bottom edge the claim is true for free.
	#
	# **THIS CLAIM WEAKENED when the dock stopped reserving, and the comment says so rather than
	# pretending otherwise.** It used to pin an EXEMPTION — the dock reserved, and `MAP_ONLY_RESERVERS`
	# kept that reservation off the HUD. There is no reservation now, so what is left is the absence
	# of one, and the two assertions at the top of this block (no signal, no size method) are the
	# stronger statement of it. This survives as the BEHAVIOURAL half: whatever the dock is doing, the
	# HUD's own rect must not move. It can still fail — someone wiring the dock into
	# `set_reserved_inset` breaks it — it simply no longer describes a mechanism.
	h._assert_hud("the dock does not move the HUD, on the edge where a TOP strip would (LayoutRoot offsets stay 0)",
		is_zero_approx(h._hud.layout_root.offset_top) and is_zero_approx(h._hud.layout_root.offset_bottom))
	# THE NEGATIVE CONTROL, and it is what stops the line above being a statement about a
	# reserved-inset path that quietly does nothing: the same strip height pushed under a DIFFERENT id
	# DOES move the HUD.
	h._hud.set_reserved_inset(&"preview_probe", SIDE_TOP, event_dock._root.size.y)
	await h._settle()
	h._assert_hud("…and the reserved-inset path is live: another reserver's SIDE_TOP strip does move it",
		h._hud.layout_root.offset_top > 0.0)
	h._hud.set_reserved_inset(&"preview_probe", SIDE_TOP, 0.0)
	await h._settle()

	# A bar tall enough to reach the ContentRow is the only one that can touch the two DOCKS, so the
	# expanded log on the bottom edge is where that pair is asserted.
	event_dock.set_dock(SIDE_BOTTOM)
	event_dock.set_expanded(true)
	await h._settle()
	_assert_bar_clears(event_dock, h._hud.left_dock_region, "the HUD's LEFT dock")
	_assert_bar_clears(event_dock, h._hud.right_dock_region, "the HUD's RIGHT dock")
	event_dock.set_expanded(false)
	await h._settle()

	# event_dock_top_expanded — the OTHER edge, log open. Two claims: the bar reads as a one-line
	# title and NOT as a second copy of the log's newest turn-group (the failure the prototype made
	# unmissable), and the log opens INWARD from the top edge with the bar still hugging it.
	event_dock.set_dock(SIDE_TOP)
	event_dock.set_expanded(true)
	await h._settle()
	await h._save("event_dock_top_expanded")
	h._assert_hud("expanded: the bar is ONE row, not a reprint of the log's newest turn-group",
		event_dock._rows.get_child_count() == 1)

	# event_dock_everything_expanded — the `routine` floor, i.e. every receipt the retired feed used
	# to carry, with the log open. This is the state the strip could eat the map in, so it is where
	# the yield cap is asserted.
	event_dock.set_detail_level(HudEventVocab.RUNG_ROUTINE)
	await h._settle()
	await h._save("event_dock_everything_expanded")

	# event_dock_alerts_only — the quietest setting on the narrowest bar: one row, alerts only. The
	# `status=feral` row must be here (a `cultivate` kind PROMOTED to Alert by its detail token) and
	# every routine receipt must be gone.
	event_dock.set_expanded(false)
	event_dock.set_dock(SIDE_BOTTOM)
	event_dock.set_recent_count(1)
	event_dock.set_detail_level(HudEventVocab.RUNG_ALERT)
	await h._settle()
	await h._save("event_dock_alerts_only")

	# event_dock_pinned_alert — 4 rows at the `notable` floor over a FRESH ingest, so the alerts are
	# unread again. Turn 47's raid is inside the window; the pin is judged on the deeper one, so the
	# fixture's alerts sit far enough back that the newest four rows cannot contain them.
	event_dock.set_recent_count(EVENT_DOCK_MAX_ROWS)
	event_dock.set_detail_level(HudEventVocab.RUNG_NOTABLE)
	event_dock.reset()
	event_dock.ingest_events(_event_dock_pin_fixture())
	await h._settle()
	await h._save("event_dock_pinned_alert")
	h._assert_hud("pinned alert: the unread raid holds the LEADING slot",
		event_dock._pinned_order >= 0)

	# ---- THE BAR LIVES BETWEEN THE VERTICAL DOCKS -------------------------------------------
	# Reported live: a `SIDE_TOP` bar spanning the full window, drawn at layer 104 over the
	# `SIDE_LEFT` band panel at 103, covering its tab bar. `RESERVER_PRIORITY` cannot fix that —
	# it orders reservers stacked ALONG one edge, and TOP and LEFT are not co-edge — so the bar's
	# own EXTENT is pulled in by the live left/right reservation totals instead.
	#
	# A REAL `BandCityPanel` supplies the number. A literal would prove nothing about the two
	# rects actually clearing each other, which is the whole claim.
	event_dock.set_expanded(false)
	event_dock.set_recent_count(EVENT_DOCK_MAX_ROWS)
	event_dock.set_dock(SIDE_TOP)
	var inset_panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(inset_panel)
	await h.get_tree().process_frame
	inset_panel.set_dock(SIDE_LEFT)
	var left_reserved: float = inset_panel.current_reservation_size()
	h._hud.set_reserved_inset(&"band_panel", SIDE_LEFT, left_reserved)
	await h._settle()
	# The band panel DOES inset the HUD, so its left dock now sits inside the reserved strip — which
	# is why the two terms ADD rather than compete.
	var expected_left: float = left_reserved + h._hud.left_column_width()

	# THE NEGATIVE CONTROL, taken FIRST and against the same two live nodes: with the insets at zero
	# the rects really do overlap. So the assertion below is not satisfiable by two panels that
	# happen never to meet, and the state it describes is reachable rather than hypothetical.
	event_dock.set_perpendicular_insets(0.0, 0.0)
	await h._settle()
	h._assert_hud("inset control: at zero inset the bar genuinely DOES overlap a left-docked panel",
		event_dock._root.get_global_rect().intersects(inset_panel._root.get_global_rect()))

	_preview_push_event_dock_insets(event_dock, left_reserved, 0.0)
	await h._settle()
	await h._save("event_dock_inset_left_panel")
	h._assert_hud("inset: the top bar starts past the left-docked panel AND the HUD's own left dock (%.0f + %.0f)"
			% [left_reserved, h._hud.left_column_width()],
		is_equal_approx(event_dock._root.offset_left, expected_left))
	h._assert_hud("inset: …and it overlaps neither the docked panel nor the HUD's left dock",
		not event_dock._root.get_global_rect().intersects(inset_panel._root.get_global_rect())
			and not event_dock._root.get_global_rect().intersects(h._hud.left_dock_region.get_global_rect()))
	h._assert_hud("inset: …and it still clears the HUD's right dock on the far side",
		not event_dock._root.get_global_rect().intersects(h._hud.right_dock_region.get_global_rect()))

	# The BOTTOM edge takes the same inset — the bug was about the horizontal axis, so both edges
	# must be fixed and a fix that only reached `SIDE_TOP` has to fail here.
	event_dock.set_dock(SIDE_BOTTOM)
	await h._settle()
	await h._save("event_dock_inset_bottom_panel")
	h._assert_hud("inset: the BOTTOM bar takes the same bound",
		is_equal_approx(event_dock._root.offset_left, expected_left)
			and not event_dock._root.get_global_rect().intersects(inset_panel._root.get_global_rect()))

	h._hud.set_reserved_inset(&"band_panel", SIDE_LEFT, 0.0)
	_preview_push_event_dock_insets(event_dock, 0.0, 0.0)
	inset_panel.queue_free()
	await h.get_tree().process_frame
	await h._settle()

	# ---- NO RAW WIRE TOKEN EVER REACHES A ROW -------------------------------------------------
	# The defect: rows printed the sim's detail verbatim, so one read `category=settle_site at
	# (64,36)`. Stated as the GENERAL property rather than spot-checking three strings — every Label
	# the dock renders, bar and log, must be free of `=`. The two guards under it are what stop that
	# being vacuous: the walk must have seen labels at all, and the pool must actually CONTAIN a raw
	# `=` for one to have been able to leak.
	event_dock.set_dock(SIDE_BOTTOM)
	event_dock.set_detail_level(HudEventVocab.RUNG_ROUTINE)
	event_dock.set_expanded(true)
	await h._settle()
	var raw_tokens := 0
	for event in event_dock._events:
		if String(event["detail"]).contains("="):
			raw_tokens += 1
	h._assert_hud("precondition: the pool really does hold raw `key=value` details (%d of them)" % raw_tokens,
		raw_tokens > 0)
	var scanned := 0
	var leaked := ""
	for label in _preview_dock_labels(event_dock):
		scanned += 1
		if label.contains("=") and leaked == "":
			leaked = label
	h._assert_hud("precondition: the scan actually walked the rendered rows (%d labels)" % scanned,
		scanned > 0)
	h._assert_hud("no rendered row carries a raw wire token — %d labels scanned, worst offender %s"
			% [scanned, "none" if leaked == "" else "\"%s\"" % leaked],
		leaked == "")

	# **NO RENDERED DETAIL CARRIES A TRAILING-ZERO DECIMAL.** The sim writes casualties with `{:.3}`,
	# which is honest on the wire (a `Scalar` really can be fractional) and DEBUG OUTPUT on a
	# notification bar — `Killed 2.000` is a float where the player is owed a count. Stated as the
	# general property, like the `=` one, and guarded the same way: the pool must actually hold a
	# `.000` for one to have reached the screen.
	# Re-seeded so the casualty rows are on the NEWEST turns and therefore inside the log's window.
	# THIS MATTERS: the pin fixture that ran before this put its raid seven turns back, outside the
	# five the log shows, so the scan walked rows that never had a padded number in them and passed
	# with the trim reverted. The precondition below counts the POOL, so it cannot catch that on its
	# own — the scan has to cover the whole pool too.
	event_dock.reset()
	event_dock.ingest_events(WorldFx.event_dock_fixture())
	await h._settle()
	var padded_wire := 0
	for event in event_dock._events:
		if String(event["detail"]).contains(".000"):
			padded_wire += 1
	h._assert_hud("precondition: the pool really does hold `{:.3}` wire numbers (%d of them)" % padded_wire,
		padded_wire > 0)
	# TWO scans, and the second is what makes the first honest. The rendered labels are what the
	# player actually sees; `detail_phrase` over EVERY retained event is the complete property, and it
	# cannot go vacuous by an event drifting out of the log's five-turn window.
	var padded := ""
	for label in _preview_dock_labels(event_dock):
		if _has_padded_decimal(label) and padded == "":
			padded = label
	for event in event_dock._events:
		var phrase := EventDockPanel.detail_phrase(String(event["detail"]))
		if _has_padded_decimal(phrase) and padded == "":
			padded = phrase
	h._assert_hud("no detail renders with a trailing-zero decimal, on screen or in the pool — worst offender %s"
			% ("none" if padded == "" else "\"%s\"" % padded),
		padded == "")
	# **THE TRIM IS NOT A ROUND**, and this is the assertion that stops someone "simplifying" it into
	# an `int()`. A casualty count reading `2` when the sim said `1.5` is a lie the player cannot
	# detect, so a genuinely fractional value has to survive intact.
	h._assert_hud("a fractional wire number survives UN-ROUNDED (`wounded=1.750` -> `%s`)"
			% EventDockPanel.detail_phrase("wounded=1.750"),
		EventDockPanel.detail_phrase("wounded=1.750") == "Wounded 1.75")
	h._assert_hud("…while a whole one loses its padding (`wounded=2.000` -> `%s`)"
			% EventDockPanel.detail_phrase("wounded=2.000"),
		EventDockPanel.detail_phrase("wounded=2.000") == "Wounded 2")
	# A bare integer must not be touched — `rstrip("0")` on `100` would answer `1`, which the trim
	# avoids only by returning early when there is no decimal point at all.
	h._assert_hud("…and a whole number with trailing zeros is left ALONE (`warriors=100` -> `%s`)"
			% EventDockPanel.detail_phrase("warriors=100"),
		EventDockPanel.detail_phrase("warriors=100") == "Warriors 100")
	h._assert_hud("the LABEL's own casualty count is not repeated beside it (`killed=3.000 wounded=1.000` -> `%s`)"
			% EventDockPanel.detail_phrase("killed=3.000 wounded=1.000"),
		EventDockPanel.detail_phrase("killed=3.000 wounded=1.000") == "Wounded 1")

	# AN UNKNOWN KEY AND AN UNKNOWN VALUE STILL RENDER AS ENGLISH. The sim adds kinds and tokens with
	# no schema change, so a token with no table row is the COMMON case over time — the generic
	# fallback is what makes a raw identifier on screen impossible by construction rather than by
	# anyone remembering to add a row. Asserted on `detail_phrase` directly: a rendered row would also
	# pass while silently dropping the fragment, which is the other way to get this wrong.
	h._assert_hud("unknown VALUE renders as English (`quarry_state=half_eaten` -> `%s`)"
			% EventDockPanel.detail_phrase("quarry_state=half_eaten"),
		EventDockPanel.detail_phrase("quarry_state=half_eaten") == "Half eaten")
	h._assert_hud("unknown NUMERIC key keeps its key (`spoiled_units=7` -> `%s`)"
			% EventDockPanel.detail_phrase("spoiled_units=7"),
		EventDockPanel.detail_phrase("spoiled_units=7") == "Spoiled units 7")
	h._assert_hud("the reported row renders as prose (`category=settle_site at (64,36)` -> `%s`)"
			% EventDockPanel.detail_phrase("category=settle_site at (64,36)"),
		EventDockPanel.detail_phrase("category=settle_site at (64,36)") == "Settle site · (64, 36)")
	h._assert_hud("a value containing a SPACE survives the token walk (`species=Grey Wolf`)",
		EventDockPanel.detail_phrase("killed=2.000 species=Grey Wolf").ends_with("Grey Wolf"))
	h._assert_hud("keys the LABEL already carries are dropped (`band=3 count=4 direction=out` -> `%s`)"
			% EventDockPanel.detail_phrase("band=3 count=4 direction=out"),
		EventDockPanel.detail_phrase("band=3 count=4 direction=out") == "departed")
	event_dock.set_expanded(false)
	event_dock.set_detail_level(HudEventVocab.RUNG_NOTABLE)
	await h._settle()

	# ---- THE OVERLAY'S TWO OBLIGATIONS --------------------------------------------------------
	# The strip floats over live map now, which makes two things its problem that were the
	# reservation's before.
	#
	# **1. IT MUST EAT ITS OWN CLICKS.** `MapView` picks hexes out of `_unhandled_input`, so a control
	# over the pointer that does not CONSUME the press lets the same click select the hex behind the
	# bar.
	#
	# **Driven through the REAL dispatch** (`Viewport.push_input`) against this harness's own
	# `_unhandled_input`, which stands in for MapView's: the GUI pass runs first, and a press it
	# consumes never becomes unhandled. That is the exact mechanism, end to end.
	#
	# The first version of this asked `gui_get_hovered_control()` after an `Input.warp_mouse`, and it
	# answered "nothing" — over the bar AND over the Telling panel, a `PanelContainer` that certainly
	# consumes. Hover state does not update in this harness. It reported a failure that was the
	# probe's, not the dock's, and its "negative control" passed either way because it was written as
	# `null or not-a-descendant`. The control below is the other way round: open canvas MUST reach
	# `_unhandled_input`, so a probe that never fires fails there instead of passing everywhere.
	event_dock.set_dock(SIDE_BOTTOM)
	event_dock.set_expanded(false)
	await h._settle()
	var bar_rect := event_dock._root.get_global_rect()
	h._assert_hud("precondition: open canvas DOES reach _unhandled_input, so this probe can see a miss",
		await _preview_press_reaches_map(h.MOUSE_PARK_POSITION))
	# **SAMPLED ACROSS THE WHOLE RECT, not just the centre.** A press in the middle lands on a row —
	# a `PanelContainer`, `STOP` by default — so it is consumed whatever the root and the card do,
	# and the first version of this passed with BOTH of their filters set to `IGNORE`. What the rows
	# do not cover is the card's own margins and the strip either side of the expander, and a click
	# there is exactly the one that would fall through to the hex behind the bar.
	var leaked_at := Vector2(-1.0, -1.0)
	for point in _preview_rect_probe_points(bar_rect):
		if await _preview_press_reaches_map(_canvas_to_window(point)):
			leaked_at = point
			break
	h._assert_hud("no press anywhere inside the bar reaches the map's input path (%s)"
			% ("all %d sample points consumed" % _preview_rect_probe_points(bar_rect).size()
				if leaked_at.x < 0.0 else "leaked at %s" % leaked_at),
		leaked_at.x < 0.0)
	# The filters that make that true, read back beside the behaviour — the behavioural test says the
	# rect is covered, these say by WHAT, so a future `IGNORE` added for a hover effect is legible as
	# the cause rather than as a mystery regression.
	h._assert_hud("…because the root and its card both STOP the pointer (root %d, card %d)"
			% [event_dock._root.mouse_filter, event_dock._panel.mouse_filter],
		event_dock._root.mouse_filter == Control.MOUSE_FILTER_STOP
			and event_dock._panel.mouse_filter == Control.MOUSE_FILTER_STOP)

	# **2. IT MUST BE OPAQUE.** Reserved chrome sat on the HUD's own background; an overlay sits on
	# terrain, which can be snow or desert. Every other frame in this set renders it over a near-black
	# backdrop, so opacity has never been under any pressure at all.
	h._assert_hud("the strip's fill is fully opaque, so bright terrain cannot reach the row text (alpha %.2f)"
			% HudStyle.PANEL_SOLID.a,
		is_equal_approx(HudStyle.PANEL_SOLID.a, 1.0))
	var backdrop := _preview_backdrop()
	var dark_backdrop := backdrop.color if backdrop != null else Color.BLACK
	if backdrop != null:
		backdrop.color = BRIGHT_TERRAIN_COLOR
	await h._settle()
	await h._save("event_dock_over_bright_terrain")
	if backdrop != null:
		backdrop.color = dark_backdrop
	await h._settle()

	# ---- THE ULTRAWIDE CAP --------------------------------------------------------------------
	# The configuration the complaint came from, and one nothing else in this set reaches: the bar
	# spanned the whole band, so a row's label sat at one end of two feet of screen and its detail at
	# the other. BOTH halves are asserted, because a cap hard-wired on would fail the narrow case and
	# one hard-wired off would fail the wide one.
	var band_now: float = float(PREVIEW_CANVAS_SIZE_BASE.x) - event_dock._inset_left - event_dock._inset_right
	h._assert_hud("below the cap the strip fills the band exactly as before (%.0f of %.0f available)"
			% [event_dock._root.size.x, band_now],
		is_equal_approx(event_dock._root.size.x, band_now) and band_now < EventDockPanel.MAX_STRIP_WIDTH)

	h.get_window().size = ULTRAWIDE_WINDOW_SIZE
	await h.get_tree().process_frame
	await h.get_tree().process_frame
	RenderingServer.force_draw()
	await h.get_tree().process_frame
	var wide_band: float = event_dock._viewport_size().x - event_dock._inset_left - event_dock._inset_right
	h._assert_hud("precondition: the ultrawide band (%.0f) is genuinely wider than the cap (%.0f)"
			% [wide_band, EventDockPanel.MAX_STRIP_WIDTH],
		wide_band > EventDockPanel.MAX_STRIP_WIDTH)
	h._assert_hud("at ultrawide the strip stops at the cap (%.0f) instead of spanning the band (%.0f)"
			% [event_dock._root.size.x, wide_band],
		is_equal_approx(event_dock._root.size.x, EventDockPanel.MAX_STRIP_WIDTH))
	var lead_gap: float = event_dock._root.offset_left - event_dock._inset_left
	var trail_gap: float = event_dock._viewport_size().x - event_dock._inset_right - event_dock._root.offset_right
	h._assert_hud("…and it is CENTRED in the band, not pinned to an edge (%.0f leading / %.0f trailing)"
			% [lead_gap, trail_gap],
		is_equal_approx(lead_gap, trail_gap))
	# **THE ONE FRAME THIS HARNESS WRITES OUTSIDE `_save`**, because it wants the ultrawide canvas as
	# it stands here — `_save` would `_capture` against the PINNED canvas and reject this very frame.
	# So the save's own error handling has to be restated inline; it cannot be inherited — and it is
	# restated in `_capture`'s OWN shape, arm for arm, since the two paths must agree about what is a
	# failure. A null image is the dummy renderer (someone ran this `--headless`), i.e. no viewport to
	# read back rather than a frame that came out wrong: it warns and skips, exactly as `_capture`
	# does. A failed WRITE with a real image in hand is a genuine failure and goes through `h._fail`.
	# TYPED, because `h` is untyped and the `:=` on `save_png` below cannot infer a return type
	# through a Variant chain — the `button_faces` chapter's idiom for the same reason.
	var wide_image: Image = h.get_viewport().get_texture().get_image()
	if wide_image == null:
		push_warning("ui_preview: null image (dummy renderer?) — skipping event_dock_ultrawide.png; run without --headless to capture")
	else:
		var wide_err := wide_image.save_png("%s/event_dock_ultrawide.png" % h.OUT_DIR)
		if wide_err != OK:
			h._fail("failed to save event_dock_ultrawide (err %d)" % wide_err)
		else:
			print("ui_preview: saved event_dock_ultrawide.png")
	h._pin_canvas(h.get_window())
	await h._settle()

	# ---- …AND THE FLOOR, WHICH IS THE SAME RULE READ FROM BELOW --------------------------------
	# The cap's own derivation says "no content is ever squeezed"; nothing enforced it downward. The
	# band is `viewport − insets`, and the insets are three FIXED logical widths (a docked panel's
	# reservation + the HUD's two authored columns), so it collapses as the logical viewport does.
	# Reported at `ui_scale` 1.35 with the Band panel docked LEFT: a 1422px viewport, insets 740/344,
	# **a 338px band** — under the dock card's own 406px minimum, so the card drew outside the strip
	# and `EventRows` (`clip_contents`) cut its labels.
	#
	# **REPRODUCED BY RESERVATION, NOT BY A SCALE, and deliberately.** The strip's rule reads the
	# BAND and knows nothing about `content_scale_factor`; the arithmetic that squeezes the band is
	# identical whether the viewport shrank or a wider panel docked. Pushing a scale here would be
	# window state in the middle of a chapter — the one thing `interface_scale.gd` runs last to avoid.
	_preview_push_event_dock_insets(event_dock, FLOOR_PROBE_RESERVED_LEFT, 0.0)
	await h._settle()
	var squeezed_band: float = float(PREVIEW_CANVAS_SIZE_BASE.x) \
		- event_dock._inset_left - event_dock._inset_right
	h._assert_hud("precondition: the squeezed band (%.0f) is genuinely under the floor (%.0f)"
			% [squeezed_band, EventDockPanel.MIN_STRIP_WIDTH],
		squeezed_band < EventDockPanel.MIN_STRIP_WIDTH)
	h._assert_hud("under the floor the strip stops shrinking at %.0f instead of taking the %.0f band"
			% [event_dock._root.size.x, squeezed_band],
		is_equal_approx(event_dock._root.size.x, EventDockPanel.MIN_STRIP_WIDTH))
	# It overhangs its INSETS — that is the trade — but never the window: a strip hanging off the
	# screen edge loses exactly the text the floor exists to save.
	h._assert_hud("…and the floored strip is still wholly on screen (%.0f..%.0f of %d)"
			% [event_dock._root.offset_left, event_dock._root.offset_right,
				PREVIEW_CANVAS_SIZE_BASE.x],
		event_dock._root.offset_left >= -CO_EDGE_RECT_EPSILON
			and event_dock._root.offset_right <= float(PREVIEW_CANVAS_SIZE_BASE.x) + CO_EDGE_RECT_EPSILON)
	# THE PAIRED NEGATIVE, and without it a floor hard-wired ON passes everything above: with the
	# squeeze released the strip must go straight back to filling its band.
	_preview_push_event_dock_insets(event_dock, 0.0, 0.0)
	await h._settle()
	var released_band: float = float(PREVIEW_CANVAS_SIZE_BASE.x) \
		- event_dock._inset_left - event_dock._inset_right
	h._assert_hud("…and with the squeeze released it FILLS its band again (%.0f of %.0f, floor %.0f)"
			% [event_dock._root.size.x, released_band, EventDockPanel.MIN_STRIP_WIDTH],
		is_equal_approx(event_dock._root.size.x, released_band)
			and released_band > EventDockPanel.MIN_STRIP_WIDTH)

	# THE STRIP DOES NOT BURY THE MAP. It reserves nothing now, so this is no longer about leaving the
	# map room to lay out in — it is about how much LIVE MAP the overlay hides, which is the same
	# `MAX_STRIP_HEIGHT_FRACTION` bound and a claim worth keeping. **Measured on the DRAWN rect**
	# (`_root.size.y`), since the published size it used to read no longer exists. Both ways the dock
	# can grow — the widest BAR (`RECENT_COUNT_MAX` rows, log closed) and the LOG open (which
	# collapses the bar to one title line) — because they are alternatives, not addends, so neither
	# is the worst case by inspection. A picture cannot carry this at all: a strip that had eaten 90%
	# of the screen would still render as a plausible bar.
	var widest_bar := event_dock._root.size.y
	event_dock.set_expanded(true)
	await h._settle()
	var open_log := event_dock._root.size.y
	var strip_cap = float(h.PREVIEW_CANVAS_SIZE.y) * EventDockPanel.MAX_STRIP_HEIGHT_FRACTION
	h._assert_hud("the strip does not bury the map: %d rows = %.0f px drawn, log open = %.0f px, cap %.0f of a %d px canvas"
			% [EVENT_DOCK_MAX_ROWS, widest_bar, open_log, strip_cap, h.PREVIEW_CANVAS_SIZE.y],
		maxf(widest_bar, open_log) <= strip_cap)

	# ---- ON A SHARED EDGE THE PANEL KEEPS THE RIM AND THE BAR IS DISPLACED --------------------
	# Reported from live play with a screenshot: with the Band/City panel and the bar on the SAME
	# edge, the bar drew straight over the panel. The perpendicular insets above cannot reach it —
	# LEFT/RIGHT is a different axis — and neither can `RESERVER_PRIORITY`, which the dock is not in
	# and must not join (that is the full-width reservation this arc removed). The bar's OWN axis
	# needed the treatment `BandCityPanel.set_edge_offset` already gives the panel:
	# `Main._update_event_dock_edge_offset` sums the reservers on the docked edge and pushes the bar
	# inboard past them. The panel keeps the screen edge; the bar sits BELOW it on a top dock and
	# ABOVE it on a bottom one.
	#
	# **Judged as a RECT NON-OVERLAP against a REAL panel, never from the frame.** An overlapping
	# strip renders a perfectly plausible-looking bar — which is exactly why this reached live play.
	event_dock.set_expanded(false)
	event_dock.set_dock(SIDE_TOP)
	var co_edge_panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(co_edge_panel)
	await h.get_tree().process_frame
	co_edge_panel.set_dock(SIDE_TOP)
	var co_edge_reserved: float = co_edge_panel.current_reservation_size()
	h._hud.set_reserved_inset(&"band_panel", SIDE_TOP, co_edge_reserved)
	await h._settle()

	# THE NEGATIVE CONTROL, taken FIRST and on the same two live nodes: at zero offset — the shipped
	# behaviour — the rects genuinely DO overlap. So the assertions below are not satisfiable by two
	# panels that happen never to meet, and the state they describe is the reported bug.
	event_dock.set_edge_offset(0.0)
	await h._settle()
	h._assert_hud("co-edge control: at zero offset the TOP bar genuinely DOES overlap a TOP-docked panel",
		event_dock._root.get_global_rect().intersects(co_edge_panel._root.get_global_rect()))

	_preview_push_event_dock_edge_offset(event_dock, [co_edge_panel])
	await h._settle()
	await h._save("event_dock_co_edge_top")
	h._assert_hud("co-edge TOP: the bar is displaced by the panel's whole reserved strip (%.0f)" % co_edge_reserved,
		is_equal_approx(event_dock._edge_offset, co_edge_reserved))
	h._assert_hud("co-edge TOP: …so the bar begins at or past where the panel ends (bar top %.0f, panel bottom %.0f)"
			% [event_dock._root.get_global_rect().position.y, co_edge_panel._root.get_global_rect().end.y],
		event_dock._root.get_global_rect().position.y >= co_edge_panel._root.get_global_rect().end.y - CO_EDGE_RECT_EPSILON)
	_assert_bar_clears_co_edge(event_dock, co_edge_panel, "the TOP-docked Band/City panel")

	# THE BOTTOM EDGE IS THE MIRROR, and it must be asserted separately: the two branches of
	# `_apply_dock_layout` write different offsets against different anchors, so a fix reaching only
	# `SIDE_TOP` has to fail here.
	event_dock.set_dock(SIDE_BOTTOM)
	co_edge_panel.set_dock(SIDE_BOTTOM)
	var co_edge_reserved_bottom: float = co_edge_panel.current_reservation_size()
	h._hud.set_reserved_inset(&"band_panel", SIDE_BOTTOM, co_edge_reserved_bottom)
	_preview_push_event_dock_edge_offset(event_dock, [co_edge_panel])
	await h._settle()
	await h._save("event_dock_co_edge_bottom")
	h._assert_hud("co-edge BOTTOM: the bar is displaced by the panel's whole reserved strip (%.0f)" % co_edge_reserved_bottom,
		is_equal_approx(event_dock._edge_offset, co_edge_reserved_bottom))
	h._assert_hud("co-edge BOTTOM: …so the bar ends at or before where the panel begins (bar bottom %.0f, panel top %.0f)"
			% [event_dock._root.get_global_rect().end.y, co_edge_panel._root.get_global_rect().position.y],
		event_dock._root.get_global_rect().end.y <= co_edge_panel._root.get_global_rect().position.y + CO_EDGE_RECT_EPSILON)
	_assert_bar_clears_co_edge(event_dock, co_edge_panel, "the BOTTOM-docked Band/City panel")

	# COLLAPSING THE PANEL MUST BRING THE BAR BACK DOWN WITH IT. The offset is a live read of what the
	# panel currently reserves, not a latched dock-edge constant, so railing it frees the strip it was
	# holding — and a bar that stayed put would leave a band of dead map between the two.
	co_edge_panel.set_collapsed(true)
	var co_edge_railed: float = co_edge_panel.current_reservation_size()
	h._hud.set_reserved_inset(&"band_panel", SIDE_BOTTOM, co_edge_railed)
	_preview_push_event_dock_edge_offset(event_dock, [co_edge_panel])
	await h._settle()
	await h._save("event_dock_co_edge_collapsed")
	h._assert_hud("precondition: the railed panel really does reserve less than the open one (%.0f < %.0f)"
			% [co_edge_railed, co_edge_reserved_bottom],
		co_edge_railed < co_edge_reserved_bottom)
	h._assert_hud("co-edge COLLAPSED: the offset tracks down to the rail (%.0f)" % co_edge_railed,
		is_equal_approx(event_dock._edge_offset, co_edge_railed))
	_assert_bar_clears_co_edge(event_dock, co_edge_panel, "the collapsed BOTTOM rail")

	# THE NON-SHARED-EDGE CONTROL: a panel on the OTHER horizontal edge displaces nothing, so the bar
	# goes back to hugging its own rim. Without this an offset that simply summed every reserver
	# regardless of edge would pass everything above.
	co_edge_panel.set_collapsed(false)
	event_dock.set_dock(SIDE_TOP)
	h._hud.set_reserved_inset(&"band_panel", SIDE_BOTTOM, co_edge_panel.current_reservation_size())
	_preview_push_event_dock_edge_offset(event_dock, [co_edge_panel])
	await h._settle()
	await h._save("event_dock_co_edge_control")
	h._assert_hud("non-shared edge: a BOTTOM-docked panel displaces the TOP bar not at all (offset %.1f)"
			% event_dock._edge_offset,
		is_equal_approx(event_dock._edge_offset, 0.0))
	h._assert_hud("non-shared edge: …so the bar sits flush against its own screen edge (bar top %.0f)"
			% event_dock._root.get_global_rect().position.y,
		absf(event_dock._root.get_global_rect().position.y) <= CO_EDGE_RECT_EPSILON)

	# THE DISPLACED STRIP AT ITS TALLEST: co-edge TOP with the log OPEN. Every co-edge frame above is
	# the COLLAPSED bar, so the configuration where the strip's own far edge could run off the bottom
	# of the screen — the panel holding 360px of rim and the bar wanting 304 more — has never been
	# rendered or measured. The claim is a rect (see `_assert_strip_within_viewport`), and the frame
	# is worth having beside it because it is the only picture of the log opening BELOW a co-edge
	# panel rather than against the screen edge.
	co_edge_panel.set_dock(SIDE_TOP)
	event_dock.set_dock(SIDE_TOP)
	h._hud.set_reserved_inset(&"band_panel", SIDE_BOTTOM, 0.0)
	h._hud.set_reserved_inset(&"band_panel", SIDE_TOP, co_edge_panel.current_reservation_size())
	_preview_push_event_dock_edge_offset(event_dock, [co_edge_panel])
	event_dock.set_expanded(true)
	await h._settle()
	await h._save("event_dock_co_edge_expanded")
	h._assert_hud("precondition: the log is OPEN and the bar really is displaced, so the strip is at its tallest (%.0f px at offset %.0f)"
			% [event_dock._cross_axis_size(), event_dock._edge_offset],
		event_dock._expanded and event_dock._edge_offset > 0.0)
	_assert_bar_clears_co_edge(event_dock, co_edge_panel, "the TOP-docked panel with the log OPEN")
	_assert_strip_within_viewport(event_dock, "co-edge TOP with the log open")
	event_dock.set_expanded(false)
	h._hud.set_reserved_inset(&"band_panel", SIDE_TOP, 0.0)
	await h._settle()

	await _assert_shell_flip_republishes(event_dock, co_edge_panel)

	h._hud.set_reserved_inset(&"band_panel", SIDE_BOTTOM, 0.0)
	event_dock.set_edge_offset(0.0)
	co_edge_panel.queue_free()
	await h.get_tree().process_frame
	await h._settle()

	# ---- A SQUEEZED STRIP STAYS INSIDE THE INSETS IT WAS GIVEN ---------------------------------
	# The band the probe above stages (338) is under EVERY candidate floor, so it can only ever show
	# the overhang trade. This one stages a band the floor USED to overhang and now fits inside: 496,
	# which is what a 1200px logical viewport leaves with NOTHING docked (1200 − 360 − 344, the HUD's
	# two authored columns) — reachable by dragging the interface scale up on an ordinary monitor.
	#
	# **REPRODUCED BY RESERVATION, NOT BY A SCALE**, for the reason the floor probe states: the strip's
	# rule reads the BAND, and the arithmetic that squeezes it is identical whether the viewport shrank
	# or a panel docked — so the reservation that shrinks this canvas to the reported viewport stages
	# the reported band exactly.
	#
	# **THE CLAIM IS PHRASED IN THE VIEWPORT AND THE INSETS, never in `MIN_STRIP_WIDTH`**: an assertion
	# written in the implementation's own terms stays green when the implementation moves, which is how
	# a strip clamped UP to a comfortable width — and drawn 79px over each HUD column — passed every
	# width claim in this chapter.
	event_dock.set_dock(SIDE_TOP)
	event_dock.set_expanded(false)
	_preview_push_event_dock_insets(event_dock, OVERHANG_PROBE_RESERVED_LEFT, 0.0)
	await h._settle()
	await h._save("event_dock_narrow_band")
	var narrow_viewport: float = event_dock._viewport_size().x
	var narrow_band: float = narrow_viewport - event_dock._inset_left - event_dock._inset_right
	h._assert_hud("precondition: the insets really do squeeze the band (%.0f of the %.0f canvas, insets %.0f / %.0f)"
			% [narrow_band, narrow_viewport, event_dock._inset_left, event_dock._inset_right],
		narrow_band > 0.0 and narrow_band < narrow_viewport)
	h._assert_hud("the squeezed strip starts at or right of the left inset (strip %.0f, inset %.0f)"
			% [event_dock._root.offset_left, event_dock._inset_left],
		event_dock._root.offset_left >= event_dock._inset_left - CO_EDGE_RECT_EPSILON)
	h._assert_hud("…and ends at or left of the right inset (strip %.0f, viewport %.0f less inset %.0f)"
			% [event_dock._root.offset_right, narrow_viewport, event_dock._inset_right],
		event_dock._root.offset_right <= narrow_viewport - event_dock._inset_right + CO_EDGE_RECT_EPSILON)
	_preview_push_event_dock_insets(event_dock, 0.0, 0.0)
	await h._settle()

	# ---- BAND FISSION — a split, and a split REFUSED (issue #511) ------------------------------
	# Both rows are `band_founded` and both sit on the ALERT rung: the kind is rare, player-initiated
	# and irreversible, and the command's REFUSALS ride the same kind — a refused irreversible order
	# is exactly as loud as a taken one. Rendered at the ALERTS-ONLY floor, which is what proves the
	# rung rather than merely showing two rows the `notable` floor would have admitted anyway.
	#
	# **THE REFUSAL'S DETAIL IS PROSE, NOT TOKENS**, which no other fixture in this chapter stages:
	# `emit_command_failure` puts the sim's own sentence in the `detail` slot, and the token walk
	# would split it on spaces and rejoin the words with ` · ` — one sentence as a column of
	# capitalised words. The pair is the claim: the token row proves the walk still runs.
	event_dock.set_dock(SIDE_BOTTOM)
	event_dock.set_expanded(false)
	event_dock.set_recent_count(EVENT_DOCK_MAX_ROWS)
	event_dock.set_detail_level(HudEventVocab.RUNG_ALERT)
	event_dock.reset()
	event_dock.ingest_events(_event_dock_founding_fixture())
	await h._settle()
	await h._save("event_dock_band_founded")
	var founding_text := " ".join(_preview_dock_labels(event_dock))
	h._assert_hud("a split reaches the ALERTS-ONLY floor — both the deed and its refusal",
		founding_text.contains(FOUNDING_LABEL) and founding_text.contains(FOUNDING_REFUSAL_LABEL))
	h._assert_hud("the refusal's prose detail is shown as ONE sentence, not as middot-joined words — got \"%s\""
			% EventDockPanel.detail_phrase(FOUNDING_REFUSAL_DETAIL),
		EventDockPanel.detail_phrase(FOUNDING_REFUSAL_DETAIL) == FOUNDING_REFUSAL_DETAIL)
	# …and the token walk is UNTOUCHED by that branch, which is the half a prose-only claim cannot
	# make: a detail that IS the machine contract must still be rendered from it. Every token of a
	# split's detail is numeric — `status=split` aside — so none of them has to be last, and the walk
	# has to reach ALL of them.
	var founded_phrase := EventDockPanel.detail_phrase(FOUNDING_DETAIL)
	h._assert_hud("a split's token detail is still rendered as prose — got \"%s\"" % founded_phrase,
		not founded_phrase.contains("=") and founded_phrase.contains(
			HudEventVocab.DETAIL_PHRASE_SEPARATOR))

	# ---- A SHORT BAND SHEDS A CREW — the rung, and the way to what it cut -----------------------
	# **THE DEFECT THIS BLOCK EXISTS FOR IS AN ABSENCE.** `status=trimmed` and `status=pruned` ride
	# their VERB's kind (`forage` / `hunt`), every one of which is `RUNG_ROUTINE`, and the dock's
	# default floor is `RUNG_NOTABLE` — so a band going 6 → 3 said nothing at all to a player on
	# default settings, which reads as the number the player just set moving on its own.
	#
	# **RENDERED AT THE DEFAULT FLOOR, and the floor is the whole point.** A frame taken at
	# `Everything` would show the plain receipt too and prove nothing; here the rows that must be
	# heard are the rows that are drawn, and the receipt beside them is filtered out. Every floor
	# claim below reads `_visible_events()` rather than the drawn bar, so it is about the FLOOR and
	# not about which surface happens to be open.
	#
	# ⛔ **AND THE LOG IS OPEN, because the bar caps at `RECENT_COUNT_MAX` (4) and this fixture stages
	# SIX status rows.** All three rungs have to be in ONE render — that is the whole claim the glyph
	# assertions make — and collapsed, the two oldest are pushed off the bar, which silently turned
	# `trimmed` and the linked hunt row into rows nobody could sample. Expanded, `_render_bar` draws a
	# single title line and `_log_body` draws every visible event, so there is no double-count either.
	#
	# **THE RUNGS ARE ASSERTED, NOT INFERRED FROM THE PICTURE.** A row drawn at the wrong importance
	# is invisible to a default player and looks perfectly fine in a frame that renders everything, so
	# every claim below reads the stamped rung off the accumulator.
	event_dock.set_dock(SIDE_BOTTOM)
	event_dock.set_recent_count(EVENT_DOCK_MAX_ROWS)
	event_dock.set_detail_level(HudEventVocab.DEFAULT_DETAIL_LEVEL)
	event_dock.reset()
	event_dock.ingest_events(_event_dock_shed_fixture())
	event_dock.set_expanded(true)
	await h._settle()
	await h._save("event_dock_crew_cut")
	h._assert_hud("a crew merely CUT is Notable — the player asked for six and got three, which is no receipt (got %s)"
			% _preview_event_rung(event_dock, SHED_TRIMMED_LINKLESS_LABEL),
		_preview_event_rung(event_dock, SHED_TRIMMED_LINKLESS_LABEL) == HudEventVocab.RUNG_NOTABLE)
	h._assert_hud("a take NARROWED under the crew is Notable too (got %s)"
			% _preview_event_rung(event_dock, SHED_PRUNED_LABEL),
		_preview_event_rung(event_dock, SHED_PRUNED_LABEL) == HudEventVocab.RUNG_NOTABLE)
	h._assert_hud("a row DESTROYED outright stays on the Alert rung it already had (got %s)"
			% _preview_event_rung(event_dock, SHED_LAPSED_LABEL),
		_preview_event_rung(event_dock, SHED_LAPSED_LABEL) == HudEventVocab.RUNG_ALERT)
	h._assert_hud("…and the SAME KIND with no `status=` token is still a Routine receipt (got %s)"
			% _preview_event_rung(event_dock, SHED_RECEIPT_LABEL),
		_preview_event_rung(event_dock, SHED_RECEIPT_LABEL) == HudEventVocab.RUNG_ROUTINE)
	h._assert_hud("the cut reaches the DEFAULT floor, which is the whole defect",
		_preview_visible_label_count(event_dock, SHED_TRIMMED_LINKLESS_LABEL) == 1)
	# ---- …AND THE TWO RUNGS ARE DRAWN APART ----------------------------------------------------
	# **THE SECOND DEFECT ON THIS FRAME, AND IT IS THE OPPOSITE SHAPE OF THE FIRST.** The rungs above
	# were always right; all four rows then drew the SAME `⚠`, so the ladder did real work in
	# filtering and was invisible on the line. Reported from play as *"losing hunts and scouts is an
	# alert but foragers are notable"* — which is not the rule at all, and is exactly the conclusion a
	# player reaches from two identically-drawn rows at two different rungs.
	#
	# **SAMPLED OFF THE RENDER, never recomposed.** The expectation is the vocab's named const and the
	# reading is the drawn `Label`'s text, so this compares the glyph the PLAYER sees against the one
	# the table promises — where composing both sides through `HudEventVocab` would assert only that
	# the table agrees with itself.
	#
	# **THE FRAME ALREADY HELD BOTH ROWS, WHICH IS WHY THE CLAIM CAN BE MADE AT ALL.** A trimmed line
	# and a lapsed line are in one fixture and one render here; a frame with only one of them is green
	# with the fix and green with the defect restored.
	var trimmed_glyph := _preview_dock_row_glyph(event_dock, SHED_TRIMMED_LINKLESS_LABEL)
	var lapsed_glyph := _preview_dock_row_glyph(event_dock, SHED_LAPSED_LABEL)
	var pruned_glyph := _preview_dock_row_glyph(event_dock, SHED_PRUNED_LABEL)
	h._assert_hud("a CUT crew is drawn with the reduction mark \"%s\" (got \"%s\")"
			% [HudEventVocab.STATUS_REDUCED_GLYPH, trimmed_glyph],
		trimmed_glyph == HudEventVocab.STATUS_REDUCED_GLYPH)
	h._assert_hud("…a DESTROYED row still wears the hazard \"%s\" (got \"%s\")"
			% [HudEventVocab.STATUS_SHED_GLYPH, lapsed_glyph],
		lapsed_glyph == HudEventVocab.STATUS_SHED_GLYPH)
	h._assert_hud("⛔ …so the Notable row and the Alert row READ APART in one frame, which is the whole defect (\"%s\" vs \"%s\")"
			% [trimmed_glyph, lapsed_glyph], trimmed_glyph != lapsed_glyph)
	h._assert_hud("…and a NARROWED take, being the same rung as a cut, wears the same mark (got \"%s\")"
			% pruned_glyph, pruned_glyph == trimmed_glyph)
	# ---- …AND THE BENCH, whose stall is the THIRD Notable token -------------------------------
	# **NEITHER EXISTING TOKEN WAS TRUE OF IT** (`systems::labor::announce_shed_bench`): a bench at
	# zero is not *still worked*, so it is not a `trimmed`; and it keeps its recipe, its progress, its
	# finished count and its drawn materials, so it is not `lapsed` — which would be false AND would
	# shout, on a state one command undoes.
	#
	# **THE RUNG CLAIM IS REALLY ABOUT `RUNG_BY_KIND`.** `craft` is not in it, so it takes
	# `DEFAULT_RUNG` (`RUNG_ROUTINE`) — under the dock's own default floor. Without the
	# `DETAIL_STATUS_STYLE` row this line announces a craft crew disappearing to nobody, which is the
	# `trimmed` / `pruned` defect one web over.
	var stalled_glyph := _preview_dock_row_glyph(event_dock, SHED_STALLED_LABEL)
	h._assert_hud("a STALLED bench is Notable — recoverable, and nothing destroyed (got %s)"
			% _preview_event_rung(event_dock, SHED_STALLED_LABEL),
		_preview_event_rung(event_dock, SHED_STALLED_LABEL) == HudEventVocab.RUNG_NOTABLE)
	h._assert_hud("…and NOT the Alert a `lapsed` row earns, which is the token it is not",
		_preview_event_rung(event_dock, SHED_STALLED_LABEL) != HudEventVocab.RUNG_ALERT)
	h._assert_hud("…so it wears the reduction mark beside the cut, not the hazard beside the loss (got \"%s\")"
			% stalled_glyph, stalled_glyph == trimmed_glyph and stalled_glyph != lapsed_glyph)
	h._assert_hud("…and it reaches the DEFAULT floor, which `craft` alone would not have",
		_preview_visible_label_count(event_dock, SHED_STALLED_LABEL) == 1)
	# **THE BENCH THAT WAS ONLY THINNED STILL READS AS A `trimmed`** — the negative that stops the new
	# token quietly becoming *every* bench line. Same kind, same band, one hand still on it.
	h._assert_hud("a bench merely CUT is still a `trimmed` — Notable, same mark (rung %s, mark \"%s\")"
			% [_preview_event_rung(event_dock, SHED_BENCH_TRIMMED_LABEL),
				_preview_dock_row_glyph(event_dock, SHED_BENCH_TRIMMED_LABEL)],
		_preview_event_rung(event_dock, SHED_BENCH_TRIMMED_LABEL) == HudEventVocab.RUNG_NOTABLE
			and _preview_dock_row_glyph(event_dock, SHED_BENCH_TRIMMED_LABEL) == trimmed_glyph)
	# **THE AMBER DOES NOT MOVE.** A trim is still unwelcome and still the player's to reverse, so the
	# fix must not have traded an over-loud row for an invisible one — the glyph carries the rung, the
	# colour carries *this is not good news*, and both rows keep it.
	h._assert_hud("…and BOTH keep the WARN amber — the glyph carries the rung, not the colour",
		_preview_dock_row_glyph_color(event_dock, SHED_TRIMMED_LINKLESS_LABEL) == HudStyle.WARN
			and _preview_dock_row_glyph_color(event_dock, SHED_LAPSED_LABEL) == HudStyle.WARN)
	# ⛔ **THE RULE ITSELF, over the WHOLE table — the claim a new row can actually break.** Every
	# assertion above names a token this chapter already stages; a status added later at
	# `RUNG_NOTABLE` wearing the hazard would pass all of them and be exactly the defect again. The
	# invariant is an iff in both directions, so neither a hazard on a Notable row nor a reduction
	# mark on an Alert one survives it.
	var mismarked: Array[String] = []
	for token in HudEventVocab.DETAIL_STATUS_STYLE:
		var entry: Dictionary = HudEventVocab.DETAIL_STATUS_STYLE[token]
		var is_alert: bool = String(entry["rung"]) == HudEventVocab.RUNG_ALERT
		var wants: String = HudEventVocab.STATUS_SHED_GLYPH if is_alert \
			else HudEventVocab.STATUS_REDUCED_GLYPH
		if String(entry["glyph"]) != wants:
			mismarked.append("%s(%s wants %s)" % [token, String(entry["glyph"]), wants])
	h._assert_hud("⛔ …and EVERY status token's glyph tracks its rung, so a new one cannot ship mismarked (%s)"
		% str(mismarked), mismarked.is_empty())
	# ⛔ **AND THE PRECONDITION THAT MAKES THAT INVARIANT MEAN ANYTHING.** The check above is an
	# equality against one of two consts, so if the two ever became the SAME string it would go on
	# passing over a table where every row is marked identically — which is the defect. Measured: with
	# `STATUS_REDUCED_GLYPH` set back to `⚠` the invariant passes and only the row-level claims fail,
	# so this is the claim that keeps the set honest rather than a restatement of it.
	h._assert_hud("…the two marks BEING DIFFERENT is what makes that invariant mean anything (\"%s\" vs \"%s\")"
			% [HudEventVocab.STATUS_SHED_GLYPH, HudEventVocab.STATUS_REDUCED_GLYPH],
		HudEventVocab.STATUS_SHED_GLYPH != HudEventVocab.STATUS_REDUCED_GLYPH)
	h._assert_hud("…while the plain receipt beside it does not — the floor is still doing its job",
		_preview_visible_label_count(event_dock, SHED_RECEIPT_LABEL) == 0)
	# **THE JUMP TO WHAT WAS CUT.** The link is offered only where the sim NAMED a band, so the two
	# rows carrying `band=` wear one and the row that carries only its source does not — the client
	# does not recover a band by parsing `foragers at (60, 0)` out of a rendered sentence.
	var work_links := _preview_dock_work_links(event_dock)
	h._assert_hud("exactly the %d rows that NAME a band offer the jump (got %d links)"
			% [SHED_LINK_BANDS.size(), work_links.size()],
		work_links.size() == SHED_LINK_BANDS.size())
	var linkless_event := {"detail": SHED_TRIMMED_LINKLESS_DETAIL}
	h._assert_hud("…and the row that names no band offers none, rather than jumping somewhere plausible",
		event_dock._work_tab_link_band(linkless_event) == HudConst.NO_BAND_ID)
	var thinnest_link := 0.0
	for link in work_links:
		thinnest_link = link.size.x if thinnest_link == 0.0 else minf(thinnest_link, link.size.x)
	h._assert_hud("…and each is DRAWN wide enough to read and to hit (thinnest %.0f px)"
			% thinnest_link, thinnest_link >= WORK_LINK_MIN_DRAWN_WIDTH)
	var asked_bands: Array[int] = []
	var link_sink := func(band_id: int) -> void: asked_bands.append(band_id)
	event_dock.band_work_tab_requested.connect(link_sink)
	for link in work_links:
		link.pressed.emit()
	event_dock.band_work_tab_requested.disconnect(link_sink)
	h._assert_hud("pressing each link asks once — %d presses, %d asks"
			% [work_links.size(), asked_bands.size()],
		asked_bands.size() == work_links.size())
	asked_bands.sort()
	h._assert_hud("…each carrying the band ITS OWN row named (asked %s, want %s)"
			% [str(asked_bands), str(SHED_LINK_BANDS)], asked_bands == SHED_LINK_BANDS)

	event_dock.queue_free()
	await h.get_tree().process_frame
	await h._settle()
