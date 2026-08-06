extends AutoSizingPanel
class_name BandComposeFloat

## THE PARTIES COMPOSE SHEET, FLOATED OFF THE PANEL when its zone cannot hold it.
##
## **WHY IT EXISTS.** The parties zone is a FIXED box — `BandCityPanel`'s reservation changes only on
## dock / collapse / hide / viewport-resize, by design, so a content edit can never re-emit
## `reservation_changed` and flicker the map. The compose sheet is the one piece of zone content that
## simply does not fit that box on a height-capped horizontal dock: measured at 641px of content in a
## 265px zone, and that is already WITHOUT the floor chart, which the SHORT tier gates off. The zone
## hosts `clip_contents`, so what shipped was a silently sliced form — the Send button included.
## Trimming ~380px means deleting most of the controls, and the two alternatives are worse: growing the
## card while a sheet is open re-introduces the content-driven reservation (i.e. the map flicker), and
## re-flowing into columns makes a THIRD layout of a form two recent passes made identical across its
## two entry points. So the sheet stops being confined to the box instead — same single-column layout,
## same builders, same order, hosted here.
##
## **THIS IS THE FREE-FLOATING CASE, hence `AutoSizingPanel`** (`.claude/rules/client/panel-framework.md`):
## the card is measured against the VIEWPORT, not against a dock's remaining height, so `PanelCard` +
## `DockScrollFit` is the wrong half of the pair and would misbehave silently. Both axes are fitted
## explicitly because this node is a plain `Control` and no child minimum ever reaches it.
##
## **IT IS THE CARD AND NOTHING MORE — there is deliberately NO full-screen catcher.** `ComposeSheet`
## (the herd drawer's floating sheet) is a catcher with a card inside it, so a click anywhere outside
## dismisses. That is exactly wrong here: the DOCK's sheet stays open through a map pick, which is what
## makes the quarry picker work — the targeting banner and the herd glow ride on the sheet still being
## open while the player clicks a herd. A catcher would eat that click and close the sheet under it.
## The float therefore covers its own rect and nothing else, and `PanelRoot`'s autopsy applies in
## reverse: a `STOP` control the pointer finds makes the Viewport mark the press handled before
## `MapView._unhandled_input` ever sees it, so every pixel this node claims is a pixel of dead map.
##
## **AND IT NEVER OVERLAPS THE CARD IT CAME FROM.** The room it may use is computed once
## (`_room`) as the viewport inside its margin, cut back to the MAP-FACING side of the panel card;
## the width fit, the height fit and the placement all read that one rect, so non-overlap is
## structural rather than a clamp that happens to work out.
##
## The float knows nothing about missions, quarries or policies: `BandPanelController` hands it a
## fully built sheet and takes it back (or lets it be freed) on the next render.

## Clearance kept between the float and the viewport edges.
const VIEWPORT_MARGIN := 12.0
## The gap between the panel card's map-facing edge and this float — the same read as the wide shell's
## inter-zone gutter: enough to read as two surfaces rather than one seam.
const ANCHOR_GAP := 12.0
## The floor the card's CONTENT width can never go below: the sheet was authored for a zone column, so
## a float narrower than one would re-wrap every row it was designed against — and a re-wrapped sheet
## is a sheet whose measured height no longer matches the number that floated it. The caller declares
## the live zone width per mount; this is the floor under it.
const MIN_CONTENT_WIDTH := BandCityPanel.ZONE_PARTY_WIDTH
## Height floor while the first fit is still a frame away — see `mount`.
const MIN_CARD_HEIGHT := 120.0

## **WHICH SIDE OF THE PANEL CARD FACES THE MAP: the OPPOSITE of the edge it is docked to.** Stated as
## a table rather than as arithmetic on the `Side` enum, whose values are not paired by any identity
## the language guarantees. It lives here because the float is the only thing that asks — the panel
## itself never needs to name the side it is NOT against.
const MAP_FACING_SIDE := {
	SIDE_LEFT: SIDE_RIGHT,
	SIDE_RIGHT: SIDE_LEFT,
	SIDE_TOP: SIDE_BOTTOM,
	SIDE_BOTTOM: SIDE_TOP,
}

## The map-facing side of a card docked to `edge`, for `mount`'s third argument.
static func map_facing_side(edge: int) -> int:
	return int(MAP_FACING_SIDE.get(edge, SIDE_RIGHT))

## The card that DRAWS the float, in the panel's OWN stylebox so it reads as the panel's surface
## rather than as a second kind of card. A real `Container` inside this plain `Control`, so it reports
## a true minimum and grows OUT of the card whenever the card is fitted too short — which is what makes
## it the honest thing to measure a fit against (`band_panel_preview`'s float-holds-its-content
## assertion), exactly as `ComposeSheet._panel` is.
var _card: PanelContainer = null
var _scroll: ScrollContainer = null
var _body: VBoxContainer = null
## The panel card's global rect and which of its sides faces the map, as of the last mount.
var _anchor: Rect2 = Rect2()
var _edge: int = SIDE_RIGHT
var _fit_pending: bool = false

func _ready() -> void:
	# `target_width` is declared per mount from the zone box, so the base class's `_ready` width
	# application has nothing to do yet — call it anyway rather than skipping the parent hook.
	super()
	name = "PartyComposeFloat"
	# The float eats its own clicks and only its own: a press on the sheet must never also select the
	# hex behind it, and a press one pixel outside must still reach `MapView._unhandled_input`.
	mouse_filter = Control.MOUSE_FILTER_STOP
	min_height = MIN_CARD_HEIGHT
	bottom_margin = VIEWPORT_MARGIN
	visible = false

	_card = PanelContainer.new()
	_card.name = "FloatCard"
	_card.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_card.mouse_filter = Control.MOUSE_FILTER_STOP
	_card.add_theme_stylebox_override("panel", BandCityPanel.panel_card_stylebox())
	add_child(_card)

	# THE ONE SCROLL, and it is not a breach of the panel's no-`ScrollContainer` rule: that rule is
	# about content whose height feeds back into a FIXED reservation. This card is measured against the
	# viewport, so its ceiling is real room — and a short window genuinely can leave less of it than the
	# form needs. Disabled unless `fit_to_content` finds the content taller than the room, so the
	# ordinary case has no scrollbar at all.
	_scroll = ScrollContainer.new()
	_scroll.name = "FloatScroll"
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_card.add_child(_scroll)

	_body = VBoxContainer.new()
	_body.name = "FloatBody"
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.add_child(_body)

# ---- public API -------------------------------------------------------------

## Adopt `sheet` (already detached from the zone footer that built it) and float it beside the panel
## card. `card_rect` is the card's global rect, `edge` the side of it that faces the map, and
## `zone_width` the width of the ZONE the sheet was measured in. Takes ownership: the previous render's
## sheet is freed here.
##
## **`target_width` IS THE ZONE WIDTH PLUS THIS CARD'S OWN CHROME, never the zone width itself.**
## `AutoSizingPanel`'s width is the card's OUTER one, and a sheet handed `zone_width` minus a border,
## two content margins and a scroll gutter re-wraps — which would make the very measurement that
## floated it wrong, in the direction that keeps it floating. The content column is what has to match.
func mount(sheet: Control, card_rect: Rect2, edge: int, zone_width: float) -> void:
	_anchor = card_rect
	_edge = edge
	target_width = maxf(zone_width, MIN_CONTENT_WIDTH) + _card_chrome_width()
	for child in _body.get_children():
		_body.remove_child(child)
		child.queue_free()
	sheet.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body.add_child(sheet)
	# **VISIBLE BEFORE THE FIT, and that is load-bearing**: `Container._sort_children` early-returns on
	# a hidden subtree, so a float kept hidden until it had been measured would never lay its content
	# out and would measure the unwrapped lower bound forever. It shows at the provisional size for the
	# one frame `refit` waits, exactly as `ComposeSheet.open` does.
	visible = true
	# **AND IT SHOWS AT ITS REAL WIDTH, not at whatever the last mount left.** The height `refit` reads
	# a frame from now is a function of THIS width — a card that spent that frame at its previous width
	# (zero, on the first mount ever) reports the wrapping of a column that no longer exists, and
	# `fit_to_content` then leaves the card 100px taller than its content with nothing to fill it.
	# `fit_width(0, 0)` is the base class's own way of saying "apply the nominal": with no content
	# measurement it can only resolve to `target_width`.
	fit_width(0.0, 0.0)
	_place()
	refit()

## Hide the float and drop the sheet it was holding. Called whenever the sheet stops floating — it
## closed, it was cancelled or sent, the panel band changed, the zone grew enough to hold it again, or
## the narrow shell moved to another tab. A float outliving its sheet is the worst outcome available
## here, so every one of those paths ends up on this method rather than on a conditional.
func dismiss() -> void:
	if _body != null:
		for child in _body.get_children():
			_body.remove_child(child)
			child.queue_free()
	visible = false

func is_floating() -> bool:
	return visible and _body != null and _body.get_child_count() > 0

## The sheet currently floated, or `null`. For assertions and for the controller's own bookkeeping.
func sheet() -> Control:
	if _body == null or _body.get_child_count() == 0:
		return null
	return _body.get_child(0) as Control

## The `PanelContainer` that draws the card. A real Container, so its combined minimum is the honest
## measure of whether this card is holding its content or quietly growing out of itself.
func card() -> PanelContainer:
	return _card

## Re-fit the card to its content and re-place it. Coalesced across one frame for the reason
## `ComposeSheet.refit` is: the content's height is a function of the card's width, so a measurement
## taken in the same frame the body was rebuilt reports the PREVIOUS content's wrapping.
func refit() -> void:
	if not visible or _fit_pending or _body == null:
		return
	_fit_pending = true
	await get_tree().process_frame
	_fit_pending = false
	if not visible or _body == null:
		return
	var room := _room()
	var chrome := _card_chrome()
	max_width = maxf(room.size.x, target_width)
	fit_width(_body.get_combined_minimum_size().x, _card_chrome_width())
	# Place BEFORE the height fit as well as after it: `AutoSizingPanel.fit_to_content` derives its
	# real ceiling from `global_position.y`, so a card fitted at last frame's position would clamp
	# against the wrong room and then be moved on top of it.
	_place()
	max_height = room.size.y
	fit_to_content(_body.get_combined_minimum_size().y, chrome.y, _scroll)
	_place()

# ---- geometry ---------------------------------------------------------------

## What the card's own chrome costs on each axis. Read off the stylebox the card actually DRAWS with —
## which is exactly what `PanelContainer` adds to its child's minimum — rather than restated from the
## panel's margin constants, so a change to the card's padding cannot leave this card mis-fitted by the
## difference (the failure `panel-framework.md` records: a card fitted too short does not fail, it grows
## out of itself and under-reports the scroll it needs).
func _card_chrome() -> Vector2:
	return BandCityPanel.panel_card_stylebox().get_minimum_size()

## The width chrome — the card's own, plus the gutter the scrollbar is laid over. Reserved
## unconditionally: the ceiling here is the VIEWPORT, so a taller or shorter window turns the internal
## scrollbar on and off, and a gutter reserved only while scrolling would jump the card's width on
## every fit.
func _card_chrome_width() -> float:
	return _card_chrome().x + _scroll_gutter()

## **THE ROOM THE FLOAT MAY USE — the one rect the width fit, the height fit and the placement all
## read.** It is the viewport inside `VIEWPORT_MARGIN`, cut back to the MAP-FACING side of the panel
## card with `ANCHOR_GAP` of clearance. Because every geometry decision below is made against this
## rect rather than against the raw viewport, "the float does not overlap the card" is structural: a
## card too tall for the room scrolls, it does not creep back across the seam.
func _room() -> Rect2:
	var room := _viewport_rect().grow(-VIEWPORT_MARGIN)
	match _edge:
		SIDE_RIGHT:
			return _cut_to(room, _anchor.end.x + ANCHOR_GAP, room.end.x, true)
		SIDE_LEFT:
			return _cut_to(room, room.position.x, _anchor.position.x - ANCHOR_GAP, true)
		SIDE_BOTTOM:
			return _cut_to(room, _anchor.end.y + ANCHOR_GAP, room.end.y, false)
		_:
			return _cut_to(room, room.position.y, _anchor.position.y - ANCHOR_GAP, false)

## `room` narrowed to `[lo, hi]` on one axis, never inverted (a card wider than the window leaves a
## zero-extent room rather than a negative one).
func _cut_to(room: Rect2, lo: float, hi: float, horizontal: bool) -> Rect2:
	var low: float = maxf(lo, room.position.x if horizontal else room.position.y)
	var high: float = minf(hi, room.end.x if horizontal else room.end.y)
	var span: float = maxf(high - low, 0.0)
	if horizontal:
		return Rect2(Vector2(low, room.position.y), Vector2(span, room.size.y))
	return Rect2(Vector2(room.position.x, low), Vector2(room.size.x, span))

## Pin the card flush to the card's map-facing seam on the anchored axis, and aligned with the card's
## leading edge on the other — clamped into the room, which by construction cannot push it back over
## the panel card.
func _place() -> void:
	var room := _room()
	var pos := position
	match _edge:
		SIDE_RIGHT:
			pos.x = room.position.x
			pos.y = clampf(_anchor.position.y, room.position.y, maxf(room.end.y - size.y, room.position.y))
		SIDE_LEFT:
			pos.x = maxf(room.end.x - size.x, room.position.x)
			pos.y = clampf(_anchor.position.y, room.position.y, maxf(room.end.y - size.y, room.position.y))
		SIDE_BOTTOM:
			pos.y = room.position.y
			pos.x = clampf(_anchor.position.x, room.position.x, maxf(room.end.x - size.x, room.position.x))
		_:
			pos.y = maxf(room.end.y - size.y, room.position.y)
			pos.x = clampf(_anchor.position.x, room.position.x, maxf(room.end.x - size.x, room.position.x))
	position = pos

## The room the vertical scrollbar needs, whether or not it is currently shown — see
## `_card_chrome_width`, which is the only caller and carries the reason.
func _scroll_gutter() -> float:
	if _scroll == null:
		return 0.0
	return _scroll.get_v_scroll_bar().get_combined_minimum_size().x

func _viewport_rect() -> Rect2:
	var viewport := get_viewport()
	if viewport != null:
		return viewport.get_visible_rect()
	return Rect2(Vector2.ZERO, Vector2(DisplayServer.window_get_size()))
