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

static func find_button_by_text(root: Node, text: String) -> Button:
	if root == null:
		return null
	if root is Button and (root as Button).text == text:
		return root as Button
	for child in root.get_children():
		var found := find_button_by_text(child, text)
		if found != null:
			return found
	return null

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
