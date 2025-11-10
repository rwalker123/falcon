extends Control
class_name AutoSizingPanel

@export var target_width: float = 0.0
@export var min_height: float = 80.0
@export var max_height: float = 420.0
@export var bottom_margin: float = 24.0

func _ready() -> void:
    if target_width > 0.0:
        _apply_width(target_width)

func fit_to_content(content_height: float, extra_height: float = 0.0, scroll: ScrollContainer = null) -> void:
    var desired_height: float = max(content_height + extra_height, min_height)
    var viewport_height: float = _viewport_height()
    var max_available: float = viewport_height - global_position.y - bottom_margin
    var clamped_height: float = clamp(desired_height, min_height, min(max_height, max_available))

    if target_width > 0.0:
        _apply_width(target_width)

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
