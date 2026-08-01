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

## The width the last fit settled on. The height fit re-asserts the card's width every pass (the
## layout under it can push the rect around), and re-asserting `target_width` there would silently
## undo `fit_width` on the very next `fit_to_content` — the two fits must agree on one number.
var _fitted_width: float = 0.0

func _ready() -> void:
    if target_width > 0.0:
        _apply_width(target_width)

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

func fit_to_content(content_height: float, extra_height: float = 0.0, scroll: ScrollContainer = null) -> void:
    var desired_height: float = max(content_height + extra_height, min_height)
    var viewport_height: float = _viewport_height()
    var max_available: float = viewport_height - global_position.y - bottom_margin
    var clamped_height: float = clamp(desired_height, min_height, min(max_height, max_available))

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

func _viewport_height() -> float:
    var viewport: Viewport = get_viewport()
    if viewport != null:
        return viewport.get_visible_rect().size.y
    return DisplayServer.window_get_size().y

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
