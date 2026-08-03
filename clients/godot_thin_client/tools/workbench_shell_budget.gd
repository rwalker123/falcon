extends Node

## THE DECOMPOSITION GUARD FOR THE WORKBENCH — the promise `WorkbenchShell.gd`'s own docstring makes.
##
## `Hud.gd` reached ~9,850 lines and `Inspector.gd` ~6,500, both one reasonable-looking method at a
## time, and both needed a decomposition arc to undo. A rule in a document is what did not work last
## time, so the Workbench's shape is asserted instead:
##
##   1. THE SHELL NAMES NO PAGE — its code contains no `pages/` path and no page's `class_name`.
##      **This is the real invariant.** The shell reads `script` out of a `WorkbenchPages.PAGES` row
##      and instantiates it, so adding a page is one registry row plus one file. The moment the shell
##      knows a page by name, that stops being true and the file has somewhere to grow.
##   2. THE LINE BUDGET — a proxy for (1), catching the accretion that has not yet named a page.
##   3. EVERY REGISTERED PAGE RESOLVES — each non-empty `script` is a file that exists and whose
##      script extends `WorkbenchPage`. A typo'd path degrades SILENTLY to the placeholder today:
##      the rail entry renders, the click works, and the page is simply never built.
##   4. THE SHELL NEVER WRITES ITS OWN `size` — the surface's width is `offset_right`. The shell is
##      anchored `PRESET_LEFT_WIDE`: left and right anchors are equal, top and bottom are NOT, and
##      `Control` warns on a `size` write under unequal opposite anchors ("will have their size
##      overridden after `_ready()`"). The warning points at a real asymmetry — `size` is a
##      `Vector2`, so `size.x = w` writes the current, minimum-size-clamped `size.y` back as an
##      explicit bottom offset, pinning a height the anchors are supposed to stretch.
##
## Run as a scene (not `--script`: the registry and the page scripts reach `class_name` globals that
## register only with the project loaded). No GPU needed — this reads source, it renders nothing:
##   godot --headless --path . res://tools/workbench_shell_budget.tscn
## Exits 0 on PASS, 1 on FAIL (CI-usable).

const SHELL_PATH := "res://src/scripts/ui/workbench/WorkbenchShell.gd"

## Ceiling on `WorkbenchShell.gd`, set at the measured size plus ~25% headroom (316 lines when this
## was written).
##
## **WHEN THIS TRIPS, EXTRACT — DO NOT RAISE THE NUMBER.** Every line that could sit in the shell can
## sit somewhere better: feature behaviour belongs to a page under `ui/workbench/pages/`, shared
## drawing to `WorkbenchWidgets` (state threaded in as parameters), and every label, glyph, size and
## threshold to `WorkbenchVocab`. Raising the budget is how the last two god-files were built.
const SHELL_LINE_BUDGET := 400

## The `pages/` directory, as it appears in a path. The shell must not contain this string in code —
## a page path in the coordinator means it is `preload`ing or naming a page rather than reading the
## registry.
const PAGES_DIRECTORY_MARKER := "pages/"

## The base class every registered page must descend from, by global name.
const PAGE_BASE_CLASS := &"WorkbenchPage"

## A write to the shell's own `size`, anchored at the START of a statement — which is what keeps it
## precise. `_rail_panel.custom_minimum_size.x = …` and every other `custom_minimum_size` write in
## the file begin with a node or property name, so the anchor alone excludes them; a `size` write on
## some OTHER node (`child.size = v`) is likewise not this file's business. Compound assignment is
## covered (`size.x += 4` is the same mistake spelled differently), and `==` is excluded, since a
## comparison writes nothing.
const SIZE_WRITE_PATTERN := "^\\s*(?:self\\.)?size(?:\\.[xy])?\\s*[+\\-*/]?=(?!=)"

var _failures: Array[String] = []


func _ready() -> void:
	var source := FileAccess.get_file_as_string(SHELL_PATH)
	if source.is_empty():
		_fail("could not read %s — the shell moved, and this guard is now asserting nothing" % SHELL_PATH)
		_finish()
		return

	_check_line_budget(source)
	_check_shell_names_no_page(source)
	_check_registry_resolves()
	_check_no_size_write(source)
	_finish()


# ---- 2. the line budget ----------------------------------------------------

func _check_line_budget(source: String) -> void:
	var lines := source.split("\n").size()
	if lines > SHELL_LINE_BUDGET:
		_fail(("%s is %d lines, over its %d-line budget. EXTRACT, do not raise the budget: feature "
			+ "behaviour to a page under ui/workbench/pages/, shared drawing to WorkbenchWidgets, "
			+ "every label/glyph/number to WorkbenchVocab.") % [SHELL_PATH, lines, SHELL_LINE_BUDGET])


# ---- 1. the shell names no page --------------------------------------------

## Comments are stripped first, because the shell's own docstring *explains* the rule and says
## `ui/workbench/pages/` while doing so. What is forbidden is the coordinator's CODE reaching a page,
## not its prose describing that it must not.
func _check_shell_names_no_page(source: String) -> void:
	var code := _strip_comments(source)
	if code.contains(PAGES_DIRECTORY_MARKER):
		_fail(("%s names a page path (%s) in CODE. The shell must reach pages ONLY through "
			+ "WorkbenchPages.PAGES — a preload or a literal path here is what makes adding a page "
			+ "need a shell edit.") % [SHELL_PATH, PAGES_DIRECTORY_MARKER])
	for class_name_used in _registered_page_class_names():
		if code.contains(class_name_used):
			_fail(("%s names the page class `%s`. The shell must know pages only as rows in "
				+ "WorkbenchPages.PAGES.") % [SHELL_PATH, class_name_used])


## The source with every comment removed — whole-line comments and trailing ones alike. A `#` inside
## a string literal would be cut too; the shell has none, and erring toward stripping can only make
## this check more permissive, never falsely fail it.
static func _strip_comments(source: String) -> String:
	var out := ""
	for line in source.split("\n"):
		var hash_at := line.find("#")
		out += (line if hash_at < 0 else line.substr(0, hash_at)) + "\n"
	return out


## The `class_name` of every page the registry declares — the names the shell is forbidden to use.
## Derived from the registry rather than listed here, so a page added tomorrow is covered today.
func _registered_page_class_names() -> Array[String]:
	var names: Array[String] = []
	for row in WorkbenchPages.PAGES:
		var script := _page_script(row)
		if script == null:
			continue
		var global_name := String(script.get_global_name())
		if not global_name.is_empty() and not names.has(global_name):
			names.append(global_name)
	return names


# ---- 3. every registered page resolves -------------------------------------

func _check_registry_resolves() -> void:
	var built := 0
	for row in WorkbenchPages.PAGES:
		var path: String = row.get("script", "")
		var id: String = String(row.get("id", ""))
		if path.is_empty():
			# A declared-but-unbuilt page. Deliberate, and rendered as the placeholder.
			continue
		built += 1
		if not ResourceLoader.exists(path):
			_fail(("page `%s` points at %s, which does not exist. A missing page script degrades "
				+ "SILENTLY to the placeholder — the rail entry still renders and the click still "
				+ "works.") % [id, path])
			continue
		var script := _page_script(row)
		if script == null:
			_fail("page `%s` points at %s, which is not a GDScript." % [id, path])
			continue
		if not _extends_page_base(script):
			_fail(("page `%s` (%s) does not extend %s, so the shell cannot host it — it would fail "
				+ "on the first `build()`.") % [id, path, PAGE_BASE_CLASS])
	if built == 0:
		_fail("no registry row declares a script — this guard is asserting nothing")


func _page_script(row: Dictionary) -> GDScript:
	var path: String = row.get("script", "")
	if path.is_empty() or not ResourceLoader.exists(path):
		return null
	var resource: Resource = load(path)
	return resource as GDScript


## Walk the inheritance chain rather than instantiating: a page is a `Node`, and standing one up
## just to type-check it would run its `_init` and need freeing.
static func _extends_page_base(script: GDScript) -> bool:
	var cursor: Script = script
	while cursor != null:
		if cursor.get_global_name() == PAGE_BASE_CLASS:
			return true
		cursor = cursor.get_base_script()
	return false


# ---- 4. the shell never writes its own size --------------------------------

## Scanned line by line rather than over `_strip_comments`' output, because the line NUMBER is the
## whole value of the report — "the shell writes `size` somewhere" is not actionable.
##
## Comment lines are skipped, and that is load-bearing rather than tidy: `_emit_reserved_width`'s
## docstring explains this very rule and spells `size.x = w` while doing so, so a guard that trips on
## its own explanation would be reverted rather than obeyed.
func _check_no_size_write(source: String) -> void:
	var pattern := RegEx.create_from_string(SIZE_WRITE_PATTERN)
	var lines := source.split("\n")
	for index in lines.size():
		var line: String = lines[index]
		if line.strip_edges().begins_with("#"):
			continue
		if pattern.search(line) == null:
			continue
		_fail(("%s writes its own `size` at line %d: `%s`. Set the surface's width through "
			+ "`offset_right` instead (`offset_right = offset_left + _surface_width`). The shell is "
			+ "anchored PRESET_LEFT_WIDE — left/right anchors equal, top/bottom NOT — so Control "
			+ "warns on a `size` write, and because `size` is a Vector2 the write also pins the "
			+ "current minimum-size-clamped `size.y` as an explicit bottom offset, freezing a height "
			+ "the anchors are supposed to stretch.")
			% [SHELL_PATH, index + 1, line.strip_edges()])


# ---- reporting -------------------------------------------------------------

func _fail(msg: String) -> void:
	_failures.append(msg)


func _finish() -> void:
	if _failures.is_empty():
		var lines := FileAccess.get_file_as_string(SHELL_PATH).split("\n").size()
		print(("workbench_shell_budget: PASS — the shell is %d/%d lines, names no page, never writes "
			+ "its own `size`, and every registered page resolves") % [lines, SHELL_LINE_BUDGET])
		get_tree().quit(0)
	else:
		printerr("workbench_shell_budget: FAIL — %d problem(s):" % _failures.size())
		for msg in _failures:
			printerr("  - ", msg)
		get_tree().quit(1)
