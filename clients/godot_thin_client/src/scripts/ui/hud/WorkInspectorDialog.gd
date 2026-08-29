extends AutoSizingPanel
class_name WorkInspectorDialog

## **THE WORK BOARD'S INSPECTOR, REHOSTED OUT OF THE WORK ZONE** (`docs/plan_standing_upkeep.md` §4.9
## item 12d). Same strip, same builder, same content — `BandPanelController._build_work_inspector` is
## untouched — mounted on a viewport-centred card that no zone's budget can see.
##
## **WHY IT LEFT THE ZONE.** The work zone had FOUR pixels of spare on a 1920 bottom dock with a row
## selected (392 drawn into a 396px box). Every expansion overflowed it: the shipped priority picker
## asked 444, item 12c's kits picker 436 — and `_work_board_capacity` is floored at `maxi(1, …)`, so
## the board could give back 4px of `int()` truncation and ZERO rows. Two small bottom docks
## (1152×720, 1024×768) already overflowed with nothing expanded at all. **A window cannot change a
## zone's height**, which is the argument `BandPanelController._open_rung_track` already makes for a
## smaller piece of content in the same zone; the inspector is the piece it was never applied to.
## Hosting it here makes the overflow IMPOSSIBLE rather than made to fit, and hands the board back the
## 84–190px the strip used to reserve.
##
## ⛔ **A `Control` ON A `CanvasLayer`, NEVER A `Popup`.** `Popup` auto-hides on an outside click and
## on parent focus loss, which is exactly the dismissal semantics this surface forbids — the strip
## RE-TARGETS when another board row is selected, so a stepper press elsewhere on the board is
## ordinary use and not a dismissal gesture. `_open_rung_track` is a `PopupPanel` because it is
## transient (open, pick, gone); this is persistent, so `Popup`'s auto-hide is a thing to fight rather
## than a thing to use. A `CanvasLayer`-hosted `Control` gives the one property the slice needs — it
## does not participate in any zone's layout — with no focus semantics and no scrim.
##
## ⛔ **AND IT IS NON-MODAL: NO CATCHER, NO SCRIM.** `ComposeSheet` IS a full-viewport
## `MOUSE_FILTER_STOP` dismiss catcher; this card covers its own rect and nothing else, because the
## board underneath has to stay live for the re-target to be reachable at all. `BandComposeFloat` is
## the precedent (its own header carries the same reason for the quarry picker) and `PanelRoot`'s
## autopsy applies in reverse: every pixel this node claims is a pixel of dead map, so it claims only
## the card.
##
## **THIS IS THE FREE-FLOATING CASE, hence `AutoSizingPanel`** (`.claude/rules/client/panel-framework.md`):
## the card is measured against the VIEWPORT, not against a dock's remaining height, so `PanelCard` +
## `DockScrollFit` is the wrong half of the pair and would misbehave silently. Both axes are fitted
## explicitly because this node is a plain `Control` and no child minimum ever reaches it.
##
## **ONE PLACEMENT, NOT FOUR: CENTRED IN THE ROOM THE PANEL LEAVES.** A bottom dock is a strip with a
## screen of map above it and a side dock a column with map beside it, so the room is over the MAP
## either way and the board stays fully visible. Floating it off the panel's map-facing edge ALIGNED
## TO THE SELECTED ROW reads better and was declined — four dock edges means four placements plus
## clamping, against the one-behaviour argument that also refused to exempt the vertical dock. This is
## still one rule: centre, in a rect computed once.
##
## ⛔ **IT WAS CENTRED IN THE RAW VIEWPORT FOR ONE SLICE, AND THE SECTIONS BROKE IT.** With one
## expansion at a time the card was ~104–156px tall and the viewport's centre was always over map;
## drawing POLICY, PRIORITY and KITS together took it to **340**, and a 340px card centred in a 1080
## viewport spans y=370…710 while a bottom dock's panel card starts at **624**. It covered the header
## and the top of the very board it exists to free — measured, and caught by the assertion that says
## so. The room is cut back off the panel card now (`BandComposeFloat.map_facing_side`, the one table
## that names which side of a docked card faces the map), so *"the board stays visible"* is
## structural rather than a consequence of the card being small.
##
## **NO `room_bounds`, DELIBERATELY.** A card that DODGES the reserved edges is the answer for a
## surface you READ (`panel-framework.md`'s table); this is one you WRITE INTO, so it takes a
## `CanvasLayer` above the docked surfaces and covers them where it must. Giving it both would be two
## mechanisms answering one question, and the room a bottom band dock leaves is not centred on
## anything the player is looking at.

## Clearance kept between the card and the viewport edges — `BandComposeFloat`'s own read of the same
## quantity, and the same number for the same reason: enough to read as a floating surface rather than
## as a thing stuck to the window.
const VIEWPORT_MARGIN := 12.0

## The clearance kept between the panel card's map-facing edge and this card — the same read as the
## wide shell's inter-zone gutter, and the same number `BandComposeFloat` keeps for the same seam:
## enough that the two read as two surfaces rather than one.
const ANCHOR_GAP := 12.0

## **THE COLUMN THE STRIP WAS AUTHORED FOR.** Every line in `_build_work_inspector` elides against the
## narrow shell's work-zone width, and `BandCityPanel.ZONE_PARTY_WIDTH` is that width stated once
## (`PANEL_WIDTH − PANEL_CHROME_H`). Naming it rather than a literal is what keeps the card the width
## the wording was measured at — a wider card would un-elide lines whose elision is a decision, and a
## narrower one would wrap them.
const CONTENT_WIDTH := BandCityPanel.ZONE_PARTY_WIDTH

## The card that DRAWS the dialog, in the panel's OWN stylebox so it reads as the panel's surface
## rather than as a second kind of card — the strip inside keeps its role-card stylebox and therefore
## looks exactly as it did sitting on the panel's background. A real `Container` inside this plain
## `Control`, so it reports a true minimum and grows OUT of the card whenever the card is fitted too
## short, which is what makes it the honest thing to measure a fit against.
var _card: PanelContainer = null
var _scroll: ScrollContainer = null
var _body: VBoxContainer = null
## What `BandPanelController._work_inspector_height` asked for at the mounted model. **The height
## arithmetic did not die when the strip left the zone — it CHANGED CONSUMER**: it was a term in the
## zone's budget and it is this card's own minimum height, which is what keeps `reserved >= drawn` a
## claim anybody can still make.
var _reserved: float = 0.0
## The panel card's global rect and which of its sides faces the map, as of the last mount — the two
## terms `_room` cuts with. `BandComposeFloat` holds the identical pair for the identical reason.
var _anchor: Rect2 = Rect2()
var _edge: int = SIDE_TOP
## A fit is in flight — see `refit`, which COALESCES on this rather than discarding.
var _fit_pending: bool = false
## …and a fit was asked for WHILE one was in flight, to be re-run once the in-flight one lands.
##
## ⛔ **A COALESCING GUARD MUST DEFER, NOT DISCARD** — `ComposeSheet._fit_requested`'s contract, which
## this card was written without. `refit` used to `return` on `_fit_pending`, and the request it threw
## away is the one that would have CORRECTED the fit: a re-mount landing after a fit was armed but
## before it resumes leaves the armed fit measuring a body that has just been replaced, and the
## re-mount's own fit — the only thing that would ever measure the new body — was the one dropped.
##
## **ONE COALESCED RE-RUN, NOT A QUEUE.** The flag records THAT a fit was wanted, never how many, so a
## burst collapses to a single extra pass; it is cleared at the START of the run that honours it, so a
## request arriving during that re-run is recorded afresh rather than lost in turn.
var _fit_requested: bool = false

func _ready() -> void:
	# `target_width` is declared per mount, so the base class's `_ready` width application has nothing
	# to do yet — call it anyway rather than skipping the parent hook.
	super()
	name = "WorkInspectorDialog"
	set_meta(HudWorkVocab.WORK_INSPECTOR_DIALOG_META, true)
	# The card eats its own clicks and ONLY its own: a press on the strip must never also select the
	# hex behind it, and a press one pixel outside must still reach the board (which is the whole
	# non-modal property) or `MapView._unhandled_input`.
	mouse_filter = Control.MOUSE_FILTER_STOP
	bottom_margin = VIEWPORT_MARGIN
	# The caller centres this card once the fit is done, so at fit time it is nowhere in particular
	# and may use the room's WHOLE height — see `AutoSizingPanel.centred_in_room` for why deriving the
	# ceiling from `global_position.y` instead would park the card at the top of the room for a whole
	# rendered frame.
	centred_in_room = true
	visible = false

	_card = PanelContainer.new()
	_card.name = "WorkInspectorCard"
	_card.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_card.mouse_filter = Control.MOUSE_FILTER_STOP
	_card.add_theme_stylebox_override("panel", BandCityPanel.panel_card_stylebox())
	add_child(_card)

	# THE ONE SCROLL, and it is not a breach of the panel's no-`ScrollContainer` rule: that rule is
	# about content whose height feeds back into a FIXED reservation, and this card reserves nothing at
	# all. Disabled unless `fit_to_content` finds the content taller than the room, so the ordinary
	# case has no scrollbar — and the ceiling here is a real 720px-minimum viewport rather than a
	# 300px zone, which is why the worst-case strip simply fits.
	_scroll = ScrollContainer.new()
	_scroll.name = "WorkInspectorScroll"
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_card.add_child(_scroll)

	_body = VBoxContainer.new()
	_body.name = "WorkInspectorBody"
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# **THE CONTENT ITSELF ASKS FOR THE FIT, WHICH IS WHAT MAKES A WRONG ONE RECOVERABLE**
	# (`ComposeSheet`'s own hookup, and this card was written without it). Every fit above is a
	# measurement taken at ONE instant, and the frame it is taken in is not something the card
	# controls; this edge means the body says so whenever the number it reports changes, so a fit
	# taken while the tree was mid-rebuild is corrected on the frame it settles instead of standing
	# until some unrelated event re-mounts. The coalescer collapses the burst a rebuild emits.
	_body.minimum_size_changed.connect(refit)
	_scroll.add_child(_body)

# ---- public API -------------------------------------------------------------

## Adopt `strip` — the `PanelContainer` `_build_work_inspector` just built — and show the card centred
## in the viewport. `reserved` is `_work_inspector_height`'s answer for the same model.
##
## **THE CONTENT IS REBUILT PER RENDER, NEVER PATCHED**, the rung track's own rule: every figure on the
## strip is a function of the source's yield, the band's shelf and whatever is queued, all of which
## move per snapshot. Re-mounting is also what RE-TARGETS the dialog when another board row is
## selected — the card stays up and its body changes, which is the behaviour a close-and-reopen would
## destroy.
func mount(strip_control: Control, reserved: float, card_rect: Rect2, map_facing: int) -> void:
	_reserved = reserved
	_anchor = card_rect
	_edge = map_facing
	min_height = reserved + _card_chrome().y
	target_width = CONTENT_WIDTH + _card_chrome_width()
	_clear_body()
	strip_control.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body.add_child(strip_control)
	# **VISIBLE BEFORE THE FIT, and that is load-bearing**: `Container._sort_children` early-returns on
	# a hidden subtree, so a card kept hidden until it had been measured would never lay its content
	# out and would measure the unwrapped lower bound forever.
	visible = true
	# …and it shows at its real width the first time, before the frame `refit` measures in. Only the
	# first time: a card already fitted is at a perfectly good width to lay its content out at, and
	# re-applying the nominal on a re-target draws one frame at a width it is about to leave again.
	if not has_fitted_width():
		fit_width(0.0, 0.0)
	_place()
	refit()

## Take the card down and drop the strip it was holding. Called whenever the inspector closes — the
## `✕`, ESC, the row leaving the board, a band switch, the work zone leaving the screen. A card
## outliving its row is the worst outcome available here, so every one of those paths ends on this
## method rather than on a conditional.
func dismiss() -> void:
	_clear_body()
	_reserved = 0.0
	visible = false

func is_open() -> bool:
	return visible and _body != null and _body.get_child_count() > 0

## The strip currently mounted, or `null`. For assertions and for the controller's bookkeeping.
func mounted_strip() -> Control:
	if _body == null or _body.get_child_count() == 0:
		return null
	return _body.get_child(0) as Control

## The `PanelContainer` that draws the card. A real Container, so its combined minimum is the honest
## measure of whether the card is holding its content or quietly growing out of itself.
func card() -> PanelContainer:
	return _card

## What the mounted model reserved — `_work_inspector_height`'s answer, carried so a harness can
## compare it against what the card DREW without re-deriving it and agreeing by construction.
func reserved_height() -> float:
	return _reserved

## The room this card places and sizes itself in. Published because a harness asking
## `available_room()` would get the RAW viewport and judge the fit against a rect the card does not
## use — the one seam rule, read from outside.
func room() -> Rect2:
	return _room()

## Re-fit the card to its content and re-centre it. Coalesced across one frame for the reason
## `BandComposeFloat.refit` is: the content's height is a function of the card's width, so a
## measurement taken in the same frame the body was rebuilt reports the PREVIOUS content's wrapping.
##
## ⛔ **THAT SENTENCE IS TRUE AND IT IS NOT THE WHOLE REASON, WHICH IS HOW THE CARD CAME TO DRAW AT
## FULL ROOM HEIGHT AROUND 300px OF CONTENT.** It reads as though the frame's only job were to let a
## wrap re-settle, i.e. as though a stale measurement were merely a line or two out. It is not: a
## `VBoxContainer` whose children have not been SORTED reports its autowrap labels at a wrap width of
## zero — one word per line — so the reading is not the previous content's, it is nobody's. Measured
## on this very body at the instant of a mount: **736 against the 278 it settles at**, and 3773
## against 408 on the fullest strip in the harness. A fit taken there asks `fit_to_content` for more
## than the room has, the room's ceiling wins, and the card is left spanning the WHOLE room with its
## content drawn compactly at the top and no scrollbar — the shipped defect, reported from play as
## *"sometimes the job panel is displaying full height … no real pattern"*. The pattern was the
## ordering: a mount landing between a fit being armed and that fit resuming.
##
## **THREE THINGS KEEP THAT FIT HONEST, and none of them is a frame count**: the request that would
## have corrected it is deferred rather than dropped (`_fit_requested`), the height is read a frame
## AFTER the width is applied rather than in the same pass (below), and the body itself asks for a
## re-fit whenever its minimum moves (`_body.minimum_size_changed`, wired in `_ready`) — which is what
## makes a fit taken at a bad instant recoverable rather than permanent.
func refit() -> void:
	if not visible or _body == null:
		return
	if _fit_pending:
		_fit_requested = true
		return
	_fit_pending = true
	# Cleared by the run that is about to honour it, so a request arriving DURING this pass is
	# recorded afresh rather than being swallowed by the one already being served.
	_fit_requested = false
	await get_tree().process_frame
	_fit_pending = false
	if not visible or _body == null:
		return
	var room := _room()
	max_width = maxf(room.size.x, target_width)
	fit_width(_body.get_combined_minimum_size().x, _card_chrome_width())
	# **A SECOND FRAME, BECAUSE THE WIDTH FIT ABOVE INVALIDATES THE HEIGHT READING BELOW IT** —
	# `ComposeSheet.refit`'s own wait, for its own measured reason. Godot's container sort is
	# DEFERRED, so a combined minimum height read in the pass that just moved the card's width still
	# reports the PREVIOUS width's wrapping. `_fit_pending` is already false here, so a request
	# arriving during this second wait is not swallowed — it simply runs, and applies the LATER of the
	# two measurements, which is the more settled one.
	await get_tree().process_frame
	if not visible or _body == null:
		return
	room = _room()
	max_height = room.size.y
	fit_to_content(_body.get_combined_minimum_size().y, _card_chrome().y, _scroll)
	_place()
	# The deferred request, honoured now that this pass has landed. Fire-and-forget, like every call
	# site: nothing awaits `refit`, and awaiting our own re-run would only make this coroutine outlive
	# the fit it was asked for.
	if _fit_requested:
		_fit_requested = false
		refit()

# ---- geometry ---------------------------------------------------------------

## Centre the card in the room, clamped inside it — one placement, whatever edge the panel is docked
## to. `position` is parent-local and the parent is a `CanvasLayer` carrying an identity transform, so
## this IS the global rect.
func _place() -> void:
	var room := _room()
	position = Vector2(
		room.position.x + maxf((room.size.x - size.x) * 0.5, 0.0),
		room.position.y + maxf((room.size.y - size.y) * 0.5, 0.0))

## **THE ROOM THE CARD MAY USE — the one rect the width fit, the height fit and the placement all
## read.** The viewport inside `VIEWPORT_MARGIN`, cut back to the MAP-FACING side of the panel card
## with `ANCHOR_GAP` of clearance. Because every geometry decision is made against this rect rather
## than against the raw viewport, *"it never covers the board"* is structural: a card too tall for the
## room scrolls, it does not creep back across the seam.
##
## An UNSET anchor (a zero rect, i.e. a panel that could not state its geometry) leaves the whole
## viewport, which is the honest answer rather than a guessed cut — `BandComposeFloat`'s rule that the
## drastic branch must be positively justified, read one surface over.
func _room() -> Rect2:
	var room := available_room(VIEWPORT_MARGIN)
	if _anchor.size.x <= 0.0 or _anchor.size.y <= 0.0:
		return room
	match _edge:
		SIDE_RIGHT:
			return _cut_to(room, _anchor.end.x + ANCHOR_GAP, room.end.x, true)
		SIDE_LEFT:
			return _cut_to(room, room.position.x, _anchor.position.x - ANCHOR_GAP, true)
		SIDE_BOTTOM:
			return _cut_to(room, _anchor.end.y + ANCHOR_GAP, room.end.y, false)
		_:
			return _cut_to(room, room.position.y, _anchor.position.y - ANCHOR_GAP, false)

## `room` narrowed to `[lo, hi]` on one axis, never inverted (a panel taller than the window leaves a
## zero-extent room rather than a negative one). `BandComposeFloat._cut_to`'s twin.
func _cut_to(room: Rect2, lo: float, hi: float, horizontal: bool) -> Rect2:
	var low: float = maxf(lo, room.position.x if horizontal else room.position.y)
	var high: float = minf(hi, room.end.x if horizontal else room.end.y)
	var span: float = maxf(high - low, 0.0)
	if horizontal:
		return Rect2(Vector2(low, room.position.y), Vector2(span, room.size.y))
	return Rect2(Vector2(room.position.x, low), Vector2(room.size.x, span))

## What the card's own chrome costs on each axis. Read off the stylebox the card actually DRAWS with —
## which is exactly what `PanelContainer` adds to its child's minimum — rather than restated from the
## panel's margin constants, so a change to the card's padding cannot leave this card mis-fitted by
## the difference.
func _card_chrome() -> Vector2:
	return BandCityPanel.panel_card_stylebox().get_minimum_size()

## The width chrome — the card's own, plus the gutter the scrollbar is laid over. Reserved
## unconditionally: the ceiling here is the VIEWPORT, so a taller or shorter window turns the internal
## scroll on and off, and a gutter reserved only while scrolling would jump the card's width on every
## fit.
func _card_chrome_width() -> float:
	return _card_chrome().x + _scroll_gutter()

## The room the vertical scrollbar needs, whether or not it is currently shown — see
## `_card_chrome_width`, which is the only caller and carries the reason.
func _scroll_gutter() -> float:
	if _scroll == null:
		return 0.0
	return _scroll.get_v_scroll_bar().get_combined_minimum_size().x

## **`remove_child` BEFORE `queue_free`, never `queue_free` alone.** The free is deferred, so a body
## cleared with `queue_free` still reports its old child on the very next `add_child` and the card
## measures two strips — `BandComposeFloat` carries the same loop for the same reason.
func _clear_body() -> void:
	if _body == null:
		return
	for child in _body.get_children():
		_body.remove_child(child)
		child.queue_free()
