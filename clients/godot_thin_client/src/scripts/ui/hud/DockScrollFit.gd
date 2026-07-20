class_name DockScrollFit
extends RefCounted

## Shared sizing for a DOCK CARD whose content grows without bound.
##
## Two left-dock cards now hold a rolling, unbounded list — the command feed and the Telling
## panel — and both need the same thing: grow to fit the content, but never past the space left
## in the dock below the cards above them, so the card scrolls INTERNALLY rather than dragging
## the fixed panels (Tile, Occupants) through the dock's own scroll.
##
## This is deliberately NOT `AutoSizingPanel`. That helper sizes a FREE-FLOATING control against
## the viewport (`global_position` + anchors + `offset_bottom` — see NarrativeForkPanel and the
## Inspector), and a card inside the dock's VBoxContainer has neither: the container overwrites
## its size every layout pass, and the ceiling that matters is the DOCK's remaining height, not
## the window's. `PanelCard` + this helper is the container-side equivalent, and factoring it here
## is what keeps the second such card from re-deriving the first one's height math.

## Grow `scroll` to fit `label`'s content, capped by the room left in `dock_scroll` beneath it.
## `dock_scroll` may be null (an un-docked/preview host), in which case the content height wins.
static func fit(
	scroll: ScrollContainer,
	label: RichTextLabel,
	dock_scroll: ScrollContainer,
	min_height: float,
	bottom_margin: float,
) -> void:
	if scroll == null or label == null:
		return
	var cap: float = label.get_content_height()
	if dock_scroll != null and dock_scroll.size.y > 0.0:
		var top_in_dock: float = scroll.global_position.y - dock_scroll.global_position.y
		var available: float = dock_scroll.size.y - top_in_dock - bottom_margin
		cap = min(cap, max(available, min_height))
	scroll.custom_minimum_size.y = max(cap, 0.0)
