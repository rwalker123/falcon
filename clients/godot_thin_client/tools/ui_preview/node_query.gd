## Generic node/text lookups over a rendered HUD subtree.
##
## Lifted out of `tools/ui_preview.gd` — pure, harness-free helpers, so that adding a state
## to one arc does not touch the same file as adding a state to another. See
## `.claude/rules/client/test-harnesses.md`.

static func has_label_containing(root: Node, text: String) -> bool:
	if root == null:
		return false
	if root is Label and (root as Label).text.contains(text):
		return true
	if root is RichTextLabel and (root as RichTextLabel).text.contains(text):
		return true
	for child in root.get_children():
		if has_label_containing(child, text):
			return true
	return false

## **THE FIRST LABEL CONTAINING `text`, AS ITS WHOLE TEXT** — the reading half of
## `has_label_containing`, for a claim that two renders produced the SAME line rather than that each
## contains one number. Two `contains` assertions are satisfied by two different labels; comparing the
## labels is what makes "nothing moved" a claim. `""` when none matched, which fails a comparison
## rather than satisfying it.
static func label_containing(root: Node, text: String) -> String:
	if root == null:
		return ""
	if root is Label and (root as Label).text.contains(text):
		return (root as Label).text
	if root is RichTextLabel and (root as RichTextLabel).text.contains(text):
		return (root as RichTextLabel).text
	for child in root.get_children():
		var found := label_containing(child, text)
		if found != "":
			return found
	return ""

## ⛔ **A STACKED ACTION BUTTON MATCHES ON ITS FIRST LINE, and without that every claim about the
## tile card's `Assign … ▸` faces would fail rather than testify.** Since those became TWO-LINE
## controls (`HudWidgets.build_stacked_action_button` — the label over the source's standing summary),
## the face is a `Label` painted over an EMPTY-`text` `Button`, so a `text` match alone finds nothing.
## What comes back is still the PRESSABLE button, which is what every caller does with it.
static func find_button_by_text(root: Node, text: String) -> Button:
	if root == null:
		return null
	if root is Button and (root as Button).text == text:
		return root as Button
	if root is Control and (root as Control).has_meta(HudWidgets.STACKED_ACTION_CELL_META) \
			and action_button_face(root as Control) == text:
		return stacked_action_button(root as Control)
	for child in root.get_children():
		var found := find_button_by_text(child, text)
		if found != null:
			return found
	return null

## What a tile-card action button READS, whichever shape it is — a plain `Button.text` (`Road ▸`,
## `Move`) or a stacked cell's first line. Callers assert against this rather than `Button.text`,
## which is empty by construction on a stacked one.
static func action_button_face(node: Node) -> String:
	var cell := stacked_action_cell(node)
	if cell != null:
		var label := find_meta_node(cell, HudWidgets.STACKED_ACTION_LABEL_META)
		return (label as Label).text if label is Label else ""
	return (node as Button).text if node is Button else ""

## The stacked CELL a node belongs to — the node itself, or the ancestor that carries the meta when a
## caller is holding the pressable `Button` out of `find_button_by_text`.
static func stacked_action_cell(node: Node) -> Control:
	var walk := node
	while walk != null:
		if walk is Control and (walk as Control).has_meta(HudWidgets.STACKED_ACTION_CELL_META):
			return walk as Control
		walk = walk.get_parent()
	return null

## A stacked cell's own `Button` — the child carrying the click, the stylebox and the tooltip.
static func stacked_action_button(cell: Control) -> Button:
	if cell == null:
		return null
	for child in cell.get_children():
		if child is Button:
			return child as Button
	return null

## A stacked cell's SECOND LINE — the standing summary flow, or `null` on a source nobody works.
## `node` may be the cell or the button, so a caller holding either can ask.
static func stacked_action_summary(node: Node) -> Control:
	var cell := stacked_action_cell(node)
	if cell == null:
		return null
	return find_meta_node(cell, HudWidgets.STACKED_ACTION_BODY_META) as Control

## A compose sheet's COMMIT button by its own meta, never by face: the face is the thing every crew-noun
## assertion is ABOUT (`Forage` / `Tend` / `Hunt Here` / `Unassign`), so finding it by text could only
## ever confirm the string the caller already assumed.
static func compose_commit_button(root: Node) -> Button:
	var node := find_meta_node(root, HudWidgets.COMPOSE_COMMIT_META)
	return node as Button if node is Button else null

static func find_policy_rung(root: Node, policy: String) -> Button:
	if root == null:
		return null
	if root is Button and (root as Button).get_meta(HudWidgets.POLICY_RUNG_META, "") == policy:
		return root as Button
	for child in root.get_children():
		var found := find_policy_rung(child, policy)
		if found != null:
			return found
	return null

## The first node under `root` carrying `meta` — the identity finder for the three 4b controls, which
## carry no text at all (the chart) or a face made of live numbers (the targets, the verdict). A text
## match on any of them would find nothing and pass, which is the failure this idiom exists to avoid.
static func find_meta_node(root: Node, meta: String) -> Node:
	if root == null:
		return null
	if root is Control and (root as Control).has_meta(meta):
		return root
	for child in root.get_children():
		var found := find_meta_node(child, meta)
		if found != null:
			return found
	return null

static func find_crew_target(root: Node, key: String) -> Button:
	if root == null:
		return null
	if root is Button and (root as Button).get_meta(HudWidgets.CREW_TARGET_META, "") == key:
		return root as Button
	for child in root.get_children():
		var found := find_crew_target(child, key)
		if found != null:
			return found
	return null

## The deepest descendant setting a row's minimum width, named by its face — so a failure says WHICH
## control is too wide rather than only that the row is.
static func widest_control_face(root: Control) -> String:
	var best: Control = root
	var stack: Array[Node] = [root]
	while not stack.is_empty():
		var node: Node = stack.pop_back()
		for child in node.get_children():
			stack.append(child)
			if child is Control and (child as Control).get_combined_minimum_size().x \
					> best.get_combined_minimum_size().x:
				best = child as Control
	var face := ""
	if best is Button:
		face = (best as Button).text
	elif best is Label:
		face = (best as Label).text
	elif best is RichTextLabel:
		face = (best as RichTextLabel).get_parsed_text()
	return "%s(%.0f) %s" % [best.get_class(), best.get_combined_minimum_size().x, face.substr(0, 40)]


## **THE RENDERED reason rows of an OPEN turn-orb popover**, in the order they are drawn, each as
## `{label, detail, jump}` read off the Labels themselves — never off `TurnOrb._entries`. A registry
## read would pass on a row the popover never drew, and it would also skip the sort `set_attention`
## applies, so a claim about which row sits ABOVE which could not be made against it. The popover body
## is a header, one Button per entry, and a footer whose Advance button is nested one level deeper —
## so the body's DIRECT Button children are exactly the reason rows.
##
## Takes the ORB NODE rather than the harness: it is `turn_orb`'s original helper, moved here when the
## `starting_loadout` chapter became a second caller, which is the rule for a helper two chapters need.
static func turn_orb_popover_rows(orb: Node) -> Array:
	var rows: Array = []
	if orb == null:
		return rows
	var pop = orb._popover
	if pop == null or pop.get_child_count() == 0:
		return rows
	for row_node in pop.get_child(0).get_children():
		if not (row_node is Button) or row_node.get_child_count() == 0:
			continue
		# The row is stripe / icon / text stack / jump, and the text stack is the only VBox in it, so
		# the label/detail pair is reached structurally rather than by counting siblings.
		for cell in row_node.get_child(0).get_children():
			if not (cell is VBoxContainer) or cell.get_child_count() < 2:
				continue
			# **AND THE AFFORDANCE**, which is the last child of the row's own HBox: `Jump ->` for a
			# locating row, `Open >` for a non-locating kind that a panel branch answers, and EMPTY for
			# one that neither locates nor opens. Read here rather than asserted off the kind, because
			# the failure this catches is a row that WEARS the affordance and does nothing when pressed.
			var jump := ""
			var last: Node = row_node.get_child(0).get_child(row_node.get_child(0).get_child_count() - 1)
			if last is Label:
				jump = String((last as Label).text)
			rows.append({
				"label": String((cell.get_child(0) as Label).text),
				"detail": String((cell.get_child(1) as Label).text),
				"jump": jump,
				# **AND THE ROW ITSELF**, so a caller can PRESS the row it just read rather than fake
				# the orb's signal — the affordance and what the press actually reaches are two
				# different claims, and a producer with more than one subject can get the second one
				# wrong while every rendered word is right.
				"button": row_node,
			})
			break
	return rows
