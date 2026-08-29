extends Control
class_name AutoSizingPanel

## A free-floating card that sizes itself to its content against the VIEWPORT (the dock-card twin is
## `PanelCard` + `DockScrollFit` — see `.claude/rules/client/panel-framework.md`).
##
## **THIS NODE IS A PLAIN `Control`, WHICH IS WHY BOTH AXES NEED AN EXPLICIT FIT.** Only a `Container`
## aggregates its children's minimum sizes, so nothing a child demands ever reaches this node: it is
## whatever size we set it to, and its children lay out around that number whether or not they fit.
## `target_width` is therefore a NOMINAL width — the size the card reads at when its content is
## narrow — not a cap and not a measurement. A caller whose content can outgrow it must say so with
## `fit_width`, exactly as it says how tall the content is with `fit_to_content`; a caller that never
## does keeps the fixed-width behaviour it always had.
@export var target_width: float = 0.0
@export var min_height: float = 80.0
@export var max_height: float = 420.0
## The width ceiling for `fit_width`, declared per fit from the live viewport by the caller (the same
## way `max_height` is). `0` means "never grow past `target_width`".
@export var max_width: float = 0.0
@export var bottom_margin: float = 24.0

## **THE ROOM A FREE-FLOATING CARD MAY USE IS THE VIEWPORT MINUS THE RESERVED EDGES, NOT THE RAW
## VIEWPORT.** A docked panel does not overlap the game — it RESERVES a strip of one screen edge and
## every other surface lives in what is left (`.claude/rules/client/panel-framework.md` →
## "Reserved-edge docking"). A free-floating card is the one thing that used to miss that: the
## registry insets `MapView` and the HUD's `LayoutRoot`, and a card measured against
## `get_viewport().get_visible_rect()` is measured against a rectangle nothing else in the client is
## using any more — so it grows over the docked panel and over whatever overlays the edge it just
## claimed.
##
## **AND IT IS NOT ONLY THE RESERVED EDGES.** A surface can cover a band of the window without
## reserving it — the event dock overlays the map by design — and a docked panel is drawn UNDER such
## a bar by its container while a free-floating card, placed by arithmetic, is drawn THROUGH it. So
## the HUD keeps a second rect for exactly this: `FloatingRoom`, `LayoutRoot` pulled further off
## every overlay (`Hud.set_overlay_inset`). Set this to THAT node and both the placement
## (`available_room`) and the height ceiling (`fit_to_content`) fall out of one rect.
##
## `null` keeps the raw viewport, which is the right answer for a card that IS a reserver (the
## Inspector reserves its own edge, so it must be measured against the whole window).
var room_bounds: Control = null

## **WHERE THE CARD WILL SIT IS WHAT DECIDES HOW MUCH ROOM THE HEIGHT FIT MAY SPEND**, so the caller
## that places the card declares it here.
##
## `false` — the card is pinned where it is and grows DOWNWARD (the compose sheets, the fork card):
## the room it may use is the room BELOW its own top edge, and a fit that took the whole room would
## run it off the bottom with its internal scroll left disabled.
##
## `true` — the caller CENTRES the card in the room once the fit is done, so at fit time the card is
## nowhere in particular and the room it may use is the WHOLE room. **This exists so such a card never
## has to be MOVED in order to be measured.** The ceiling is derived from the room rect, not from
## `global_position.y`; deriving it from the position instead means parking the card at the top of the
## room, taking the measurement, and centring it again — and since the measurement can only be taken a
## frame after the content was mounted, the parked position is a whole RENDERED frame at the wrong
## place. That is a visible jump on both axes every time such a card re-renders.
##
## The constraint the park was protecting is unchanged and is the reason this flag is not simply the
## default: **the ceiling has to be the whole room rather than the room below a centred card.** A
## centred card measured against the room beneath it throws away everything above it, which is the
## defect that once clamped the Materials & Crafting ledger to four rows.
var centred_in_room: bool = false

## The width the last fit settled on. The height fit re-asserts the card's width every pass (the
## layout under it can push the rect around), and re-asserting `target_width` there would silently
## undo `fit_width` on the very next `fit_to_content` — the two fits must agree on one number.
var _fitted_width: float = 0.0

func _ready() -> void:
    if target_width > 0.0:
        _apply_width(target_width)

## Whether a width has ever been applied to this card. A caller mounting fresh content applies its
## NOMINAL width before the frame it measures in (`fit_width(0, 0)`) — but only the FIRST time, a card
## that has already been fitted being at a perfectly good width to lay its content out at. Re-applying
## the nominal on a re-render draws one frame at a width the card is about to leave again.
func has_fitted_width() -> bool:
    return _fitted_width > 0.0

## Grow the card to the width its CONTENT demands, never below `target_width` and never past
## `max_width`. `extra_width` is the chrome between the measured content and the card's outer edge
## (the panel stylebox's left+right content margins, plus any scroll gutter).
##
## Called with a content width that fits inside `target_width`, this is a no-op — which is why the
## height fit does not do it implicitly: a caller that has never measured its content width must keep
## reading at its nominal width rather than collapsing to whatever its widest child happens to be.
func fit_width(content_width: float, extra_width: float = 0.0) -> void:
    if target_width <= 0.0:
        return
    var desired: float = max(content_width + extra_width, target_width)
    var ceiling: float = max_width if max_width > 0.0 else target_width
    _apply_width(clamp(desired, target_width, max(ceiling, target_width)))

## The rect this card may place and size itself in, inset by `margin` on every side: `room_bounds`
## where one was given, else the whole viewport. One seam, so a caller's placement and its height
## ceiling can never disagree about how much room there is.
func available_room(margin: float = 0.0) -> Rect2:
    return _bounds_rect().grow(-margin)

## **THE ROOM'S CEILING OUTRANKS `min_height`, AND THAT PRECEDENCE IS WRITTEN OUT RATHER THAN LEFT TO
## `clamp`'S ARGUMENT ORDER.** `min_height` above the ceiling is not a mistake and not rare — a card
## whose content genuinely wants more than the room has is the ordinary short-window case
## (`WorkInspectorDialog` reaches it on a 1152x720 bottom dock: a 298px minimum against a 291px room),
## and the answer there must be the ROOM, with the internal scroll carrying the rest. That is what
## keeps *"a free-floating card never covers what it was cut back off"* structural.
##
## `clamp(v, lo, hi)` with `lo > hi` happens to return `hi` in Godot, so this is bit-identical to the
## expression it replaces — which is exactly the problem with the old form: the one case where the two
## bounds disagree was decided by which comparison Godot writes first, on a card whose whole
## containment claim rests on the answer.
func fit_to_content(content_height: float, extra_height: float = 0.0, scroll: ScrollContainer = null) -> void:
    var desired_height: float = max(content_height + extra_height, min_height)
    var ceiling: float = min(max_height, _height_ceiling())
    var clamped_height: float = min(desired_height, ceiling)

    if target_width > 0.0:
        _apply_width(max(_fitted_width, target_width))

    _apply_height(clamped_height)

    if scroll != null:
        if desired_height > clamped_height + 0.5:
            scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
            scroll.scroll_vertical = clamp(scroll.scroll_vertical, 0, int(desired_height - clamped_height))
        else:
            scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
            scroll.scroll_vertical = 0

## The tallest this card may be, in the ROOM's terms — always the room's own rect, never the window's,
## since a bottom reservation ends the room early and a card measured to the window would size itself
## straight through the panel holding that strip.
##
## The two branches are the two placements `centred_in_room` names: a card that stays where it is may
## use what lies below its top edge, a card that will be centred afterwards may use the room's whole
## height. Neither reads the card's position for a card it is about to move.
func _height_ceiling() -> float:
    var bounds := _bounds_rect()
    if centred_in_room:
        return bounds.size.y - bottom_margin
    return bounds.end.y - global_position.y - bottom_margin

func _bounds_rect() -> Rect2:
    if room_bounds != null and is_instance_valid(room_bounds):
        return room_bounds.get_global_rect()
    return _viewport_rect()

func _viewport_rect() -> Rect2:
    var viewport: Viewport = get_viewport()
    if viewport != null:
        return viewport.get_visible_rect()
    return Rect2(Vector2.ZERO, Vector2(DisplayServer.window_get_size()))

func _apply_width(width: float) -> void:
    _fitted_width = width
    if is_equal_approx(anchor_left, anchor_right):
        if is_equal_approx(anchor_left, 0.0):
            offset_right = offset_left + width
        elif is_equal_approx(anchor_left, 1.0):
            offset_left = offset_right - width
    custom_minimum_size.x = width
    size.x = width

func _apply_height(height: float) -> void:
    if is_equal_approx(anchor_top, anchor_bottom):
        if is_equal_approx(anchor_top, 0.0):
            offset_bottom = offset_top + height
        elif is_equal_approx(anchor_top, 1.0):
            offset_top = offset_bottom - height
    custom_minimum_size.y = height
    size.y = height
