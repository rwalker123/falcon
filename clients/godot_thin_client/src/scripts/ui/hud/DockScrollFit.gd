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
##
## "Room left beneath it" means room left for THIS card — the cards stacked BELOW it in the dock
## keep their own. A growing card used to be the LAST one in its dock, so the two were the same
## number and the distinction never came up; the Telling panel now sits at the TOP of the right
## dock, and claiming everything below it pushed every following card (Victory, Terrain Types)
## clean out of the visible dock — pressing their hotkey appeared to do nothing at all.
static func fit(
	scroll: ScrollContainer,
	label: RichTextLabel,
	dock_scroll: ScrollContainer,
	min_height: float,
	bottom_margin: float,
) -> void:
	if scroll == null or label == null:
		return
	fit_height(scroll, label.get_content_height(), dock_scroll, min_height, bottom_margin)

## The same cap, measured from a CONTENT HEIGHT the caller already has.
##
## `fit` above measures a RichTextLabel, which is all this helper ever wrapped while its only
## clients were the command feed and the Telling panel. The selection card's drawer is a VBox of
## live controls (detail label + compose block), whose height comes from
## `get_combined_minimum_size().y` instead — so the measurement moves to the caller and the height
## MATH stays here, in exactly one copy. `fit` is now written in terms of this.
static func fit_height(
	scroll: ScrollContainer,
	content_height: float,
	dock_scroll: ScrollContainer,
	min_height: float,
	bottom_margin: float,
) -> void:
	if scroll == null:
		return
	var cap: float = content_height
	if dock_scroll != null and dock_scroll.size.y > 0.0:
		var top_in_dock: float = scroll.global_position.y - dock_scroll.global_position.y
		var available: float = (dock_scroll.size.y - top_in_dock - bottom_margin
			- _height_reserved_below(scroll, dock_scroll))
		cap = min(cap, max(available, min_height))
	scroll.custom_minimum_size.y = max(cap, 0.0)

## Height the cards stacked BELOW this one need, so a growing card leaves them their room.
##
## Only VISIBLE siblings count, which is what makes this cooperate with `PanelDock.set_relevant`:
## a suppressed Victory / Terrain Types card reserves nothing and the growing card reclaims the
## space, so the default (both hidden) layout is unchanged and toggling one on simply gives it
## back. Each sibling is measured at its own combined minimum — these are fixed-content cards, so
## the measurement does not depend on the growing card's height and cannot oscillate with it.
static func _height_reserved_below(scroll: ScrollContainer, dock_scroll: ScrollContainer) -> float:
	var stack: Node = _dock_stack(dock_scroll)
	if stack == null:
		return 0.0
	var card: Node = _card_in_stack(scroll, stack)
	if card == null:
		return 0.0
	var separation: float = 0.0
	if stack is VBoxContainer:
		separation = float((stack as VBoxContainer).get_theme_constant("separation"))
	var reserved: float = 0.0
	var below := false
	for child in stack.get_children():
		if child == card:
			below = true
			continue
		if not below:
			continue
		var sibling := child as Control
		if sibling == null or not sibling.visible:
			continue
		reserved += sibling.get_combined_minimum_size().y + separation
	return reserved

## The dock's panel stack — the VBoxContainer the dock reparents its cards into.
static func _dock_stack(dock_scroll: ScrollContainer) -> Node:
	for child in dock_scroll.get_children():
		if child is VBoxContainer:
			return child
	return null

## Walk up from the card's inner scroll to the ancestor that IS a card in the dock stack.
static func _card_in_stack(scroll: ScrollContainer, stack: Node) -> Node:
	var node: Node = scroll
	while node != null and node.get_parent() != stack:
		node = node.get_parent()
	return node
