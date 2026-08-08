extends Control
class_name WorkbenchShell

## The Workbench — the designer/dev surface replacing the legacy Inspector.
##
## **THIS FILE IS A COORDINATOR AND NOTHING ELSE.** It builds the rail and the content host, routes
## the page registry, owns show/hide and the edge reservation, and fans snapshots at the active
## page. It does not know a single page's snapshot keys, controls or commands — it cannot, because
## it never names a page: `WorkbenchPages.PAGES` is the only place pages are declared and the shell
## reads `script` out of a row.
##
## That is the invariant worth defending. `Hud.gd` reached 9,850 lines one reasonable-looking method
## at a time, and the thing that would have stopped it is a coordinator with nowhere to put feature
## code. So: **a feature belongs to a page (`ui/workbench/pages/`), shared drawing to
## `WorkbenchWidgets`, and every label/glyph/number to `WorkbenchVocab`.** If something seems to
## belong here, it is one of those three. `tools/workbench_shell_budget.gd` fails the build if this
## file outgrows its budget, because a rule in a document is what did not work last time.
##
## The surface reserves its screen edge through the shared reservation registry (reserver id
## `&"workbench"`), so it composes with the Band/City panel instead of overlapping it — see
## `.claude/rules/client/panel-framework.md` → "Reserved-edge docking".

## Emitted when the surface's reserved width changes (show/hide, drag-resize). `Main` fans this into
## `MapView.set_reserved_inset` / `Hud.set_reserved_inset`.
signal reserved_width_changed(width: float)

const RESERVER_ID := &"workbench"

var _rail: VBoxContainer
var _rail_panel: PanelContainer
var _content_host: VBoxContainer
var _page_title: Label
var _page_subtitle: Label
var _page_host: VBoxContainer
var _scroll: ScrollContainer
var _footer_rule: PanelContainer
var _footer_host: VBoxContainer

var _active_id: StringName = &""
var _pages: Dictionary = {}          # StringName -> WorkbenchPage
var _page_built: Dictionary = {}     # StringName -> true once `build()` has run for that page
var _page_actions: Dictionary = {}   # StringName -> Control (or null for a page with no actions)
var _rail_entries: Dictionary = {}   # StringName -> Button
var _rail_collapsed := false
var _surface_width := WorkbenchVocab.SURFACE_WIDTH
var _panel_visible := true
## The host's lent capabilities (name → Callable), set once by `Main` and handed to every page as it
## is built, so a page never reaches back here. **The shell does not read a single entry** — it
## carries the dictionary, which is what lets a new page need a new capability without touching this
## file. See `WorkbenchVocab.SERVICE_*`.
var _services: Dictionary = {}
var _command_connected := false
## The newest frame, held BY REFERENCE for the catch-up replay when a hidden surface is shown. Same
## contract as the Inspector's: deep-copying would cost exactly the work the visibility gate saves.
var _cached_frame: Dictionary = {}
var _hidden_frame_pending := false


func _ready() -> void:
	_build_chrome()
	_build_rail()
	if _active_id == &"":
		show_page(WorkbenchPages.default_id())


# ---- chrome ----------------------------------------------------------------

func _build_chrome() -> void:
	var backing := PanelContainer.new()
	backing.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	backing.add_theme_stylebox_override("panel", WorkbenchWidgets.surface_stylebox())
	add_child(backing)

	var split := HBoxContainer.new()
	split.add_theme_constant_override("separation", 0)
	backing.add_child(split)

	_rail_panel = PanelContainer.new()
	_rail_panel.add_theme_stylebox_override("panel", WorkbenchWidgets.rail_stylebox())
	_rail_panel.custom_minimum_size.x = WorkbenchVocab.RAIL_WIDTH
	split.add_child(_rail_panel)

	_rail = VBoxContainer.new()
	_rail.add_theme_constant_override("separation", WorkbenchVocab.RAIL_ENTRY_GAP)
	_rail_panel.add_child(_rail)

	_content_host = VBoxContainer.new()
	_content_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_content_host.add_theme_constant_override("separation", WorkbenchVocab.CONTENT_GAP)
	var pad := MarginContainer.new()
	pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	for side in ["left", "right", "top", "bottom"]:
		pad.add_theme_constant_override("margin_" + side, WorkbenchVocab.CONTENT_PADDING)
	pad.add_child(_content_host)
	split.add_child(pad)

	_page_title = Label.new()
	_page_title.add_theme_color_override("font_color", HudStyle.INK)
	_page_title.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_PAGE_TITLE)
	_content_host.add_child(_page_title)

	_page_subtitle = WorkbenchWidgets.build_caption("", HudStyle.INK_DIM)
	_content_host.add_child(_page_subtitle)

	var rule := PanelContainer.new()
	rule.custom_minimum_size.y = 1.0
	rule.add_theme_stylebox_override("panel", HudStyle.hairline_stylebox())
	_content_host.add_child(rule)

	_scroll = ScrollContainer.new()
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_content_host.add_child(_scroll)

	_page_host = VBoxContainer.new()
	_page_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_page_host.add_theme_constant_override("separation", WorkbenchVocab.CONTENT_GAP)
	_scroll.add_child(_page_host)

	# THE FOOTER REGION — outside `_scroll`, so a page's actions stay on screen however long its body
	# is. The shell parents whatever `build_actions()` hands back and knows nothing else about it;
	# the rule above it is hidden for a page with no actions, so an empty region draws no chrome.
	_footer_rule = PanelContainer.new()
	_footer_rule.custom_minimum_size.y = 1.0
	_footer_rule.add_theme_stylebox_override("panel", HudStyle.hairline_stylebox())
	_content_host.add_child(_footer_rule)

	_footer_host = VBoxContainer.new()
	_footer_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_footer_host.add_theme_constant_override("separation", WorkbenchVocab.CONTENT_GAP)
	_content_host.add_child(_footer_host)


func _build_rail() -> void:
	for child in _rail.get_children():
		child.queue_free()
	_rail_entries.clear()

	var header := VBoxContainer.new()
	header.add_theme_constant_override("separation", 0)
	var title := Label.new()
	title.text = WorkbenchVocab.SURFACE_TITLE if not _rail_collapsed \
		else WorkbenchVocab.SURFACE_TITLE.substr(0, 1)
	title.add_theme_color_override("font_color", HudStyle.INK)
	title.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_SURFACE_TITLE)
	header.add_child(title)
	if not _rail_collapsed:
		header.add_child(WorkbenchWidgets.build_caption(
			"%s  ·  %s" % [WorkbenchVocab.SURFACE_SUBTITLE, WorkbenchVocab.SURFACE_HOTKEY]))
	var header_pad := MarginContainer.new()
	header_pad.add_theme_constant_override("margin_left", 12)
	header_pad.add_theme_constant_override("margin_right", 10)
	header_pad.add_theme_constant_override("margin_top", 12)
	header_pad.add_theme_constant_override("margin_bottom", 12)
	header_pad.add_child(header)
	_rail.add_child(header_pad)

	for section in WorkbenchPages.sections():
		if not _rail_collapsed:
			var label := WorkbenchWidgets.build_section_label(section)
			var label_pad := MarginContainer.new()
			label_pad.add_theme_constant_override("margin_left", 12)
			label_pad.add_theme_constant_override("margin_top", WorkbenchVocab.RAIL_SECTION_GAP)
			label_pad.add_theme_constant_override("margin_bottom", 4)
			label_pad.add_child(label)
			_rail.add_child(label_pad)
		for page in WorkbenchPages.pages_in(section):
			var id: StringName = page.get("id", &"")
			var entry := WorkbenchWidgets.build_rail_entry(page.get("glyph", "·"),
				page.get("title", ""), id == _active_id, _rail_collapsed)
			entry.pressed.connect(show_page.bind(id))
			_rail.add_child(entry)
			_rail_entries[id] = entry

	_rail_panel.custom_minimum_size.x = WorkbenchVocab.RAIL_WIDTH_COLLAPSED if _rail_collapsed \
		else WorkbenchVocab.RAIL_WIDTH


# ---- page routing ----------------------------------------------------------

## Open the page registered under `id`. Instantiates it on first open and keeps it alive after, so
## a page's state survives being switched away from.
func show_page(id: StringName) -> void:
	var row := WorkbenchPages.find(id)
	if row.is_empty():
		return
	_active_id = id
	_page_title.text = row.get("title", "")
	_page_subtitle.text = row.get("subtitle", "")

	# A PAGE is only DETACHED (it is cached and reopened); anything else in the host was made for
	# this visit alone — the placeholder caption is rebuilt on every open of an unbuilt page — so
	# detaching it without freeing leaks one Label per visit. The footer's children are never freed
	# here: those are the cached `_page_actions` controls, re-parented on the next open.
	for child in _page_host.get_children():
		_page_host.remove_child(child)
		if not (child is WorkbenchPage):
			child.queue_free()
	for child in _footer_host.get_children():
		_footer_host.remove_child(child)

	var page := _page_for(id, row)
	if page == null:
		_page_host.add_child(WorkbenchWidgets.build_caption(WorkbenchVocab.PLACEHOLDER_BODY))
	else:
		# PARENT, THEN BUILD — a page's `build()` may reach its tree (a theme, a window size, an
		# ancestor's width), and an orphaned node answers those with nulls and defaults rather than
		# an error. Building here rather than in `_page_for` is what makes the contract in
		# `WorkbenchPage`'s docstring true; `_page_built` keeps it once-only across reopens.
		_page_host.add_child(page)
		if not _page_built.has(id):
			_page_built[id] = true
			page.build()
			_page_actions[id] = page.build_actions()
			page.set_command_connected(_command_connected)
	var actions: Control = _page_actions.get(id, null)
	if actions != null:
		_footer_host.add_child(actions)
	_footer_rule.visible = actions != null
	_refresh_rail_state()

	# CATCH THE NEWLY ACTIVE PAGE UP ON THE FRAME ALREADY IN HAND. `update_snapshot` fans at the
	# ACTIVE page only, and frames arrive on turn resolution and world-mutating commands — there is no
	# heartbeat — so a page activated BETWEEN frames would otherwise sit on its empty state until the
	# next turn. This is coordinator fan-out, not page knowledge: the page is reached through the base
	# contract, never a concrete type. The visibility gate still holds (a hidden surface ingests
	# nothing), and `_hidden_frame_pending` is deliberately untouched — it is discharged only by an
	# actual fan-out, and is already false on this path, which is reached only while visible.
	if page != null and _panel_visible and not _cached_frame.is_empty():
		page.apply_update(_cached_frame, true)


## Instantiate (once) and return the page for `id`, or null when the registry row declares no script.
## The instance is NOT built here — see `show_page`.
func _page_for(id: StringName, row: Dictionary) -> WorkbenchPage:
	if _pages.has(id):
		return _pages[id]
	var path: String = row.get("script", "")
	if path.is_empty():
		return null
	var script: GDScript = load(path)
	if script == null:
		push_warning("[Workbench] page script missing: %s" % path)
		return null
	var page: WorkbenchPage = script.new()
	page.page_id = id
	page.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	page.set_services(_services)
	_pages[id] = page
	return page


func _refresh_rail_state() -> void:
	for id in _rail_entries:
		var row := WorkbenchPages.find(id)
		var entry: Button = _rail_entries[id]
		var rebuilt := WorkbenchWidgets.build_rail_entry(row.get("glyph", "·"),
			row.get("title", ""), id == _active_id, _rail_collapsed)
		# Copy the freshly-built treatment onto the live button rather than replacing the node, so
		# the rail never rebuilds (and never loses a pressed connection) on a page switch.
		for state in ["normal", "hover", "pressed"]:
			entry.add_theme_stylebox_override(state, rebuilt.get_theme_stylebox(state))
		entry.add_theme_color_override("font_color", rebuilt.get_theme_color("font_color"))
		entry.text = rebuilt.text
		rebuilt.queue_free()


## Collapse the rail to glyphs, or expand it again.
func set_rail_collapsed(collapsed: bool) -> void:
	if _rail_collapsed == collapsed:
		return
	_rail_collapsed = collapsed
	_build_rail()
	_emit_reserved_width()


# ---- shell services --------------------------------------------------------

## Hand the surface the host's services. Fanned out to every page already built, and to each one
## built after.
func set_services(services: Dictionary) -> void:
	_services = services
	for id in _pages:
		_pages[id].set_services(services)


func set_command_connected(connected: bool) -> void:
	_command_connected = connected
	for id in _pages:
		_pages[id].set_command_connected(connected)


## Fan one frame at the ACTIVE page only. A hidden surface ingests nothing and replays on show —
## the same gate the Inspector needed (`.claude/rules/client/inspector-panels.md`), applied from the
## start here. `_hidden_frame_pending` is discharged only by an actual fan-out, never by a skip.
func update_snapshot(data: Dictionary, full_snapshot: bool) -> void:
	_cached_frame = data
	if not _panel_visible:
		_hidden_frame_pending = true
		return
	_hidden_frame_pending = false
	var page: WorkbenchPage = _pages.get(_active_id, null)
	if page != null:
		page.apply_update(data, full_snapshot)


## WORLD BOUNDARY (`Main._reset_per_world_state`): the world every built page has been describing is
## gone, so each one drops the state it derived from it. The cached FRAME goes with them — replaying
## a dead world's last frame into a freshly reset page is exactly the staleness this exists to stop.
##
## Fanned at EVERY built page, not just the active one: a hidden page holds its old world's numbers
## just as wrongly, and would show them the moment the rail switched to it.
func reset_pages() -> void:
	_cached_frame = {}
	_hidden_frame_pending = false
	for id in _pages:
		_pages[id].reset()


func set_panel_visible(value: bool) -> void:
	if _panel_visible == value:
		return
	_panel_visible = value
	visible = value
	if value and _hidden_frame_pending and not _cached_frame.is_empty():
		_hidden_frame_pending = false
		var page: WorkbenchPage = _pages.get(_active_id, null)
		if page != null:
			page.apply_update(_cached_frame, true)
	_emit_reserved_width()


func is_panel_visible() -> bool:
	return _panel_visible


## Width this surface reserves from the viewport — zero while hidden, so the map reclaims the strip.
func reserved_width() -> float:
	return _surface_width if _panel_visible else 0.0


## Resize the surface, clamped to the declared bounds.
func set_surface_width(width: float) -> void:
	var clamped := clampf(width, WorkbenchVocab.SURFACE_MIN_WIDTH, WorkbenchVocab.SURFACE_MAX_WIDTH)
	if is_equal_approx(clamped, _surface_width):
		return
	_surface_width = clamped
	_emit_reserved_width()


## Width is set through the RIGHT OFFSET, never through `size`. The surface is anchored
## `PRESET_LEFT_WIDE` — left/right anchors equal, top/bottom NOT — and `Control` warns on a `size`
## write under unequal opposite anchors ("will have their size overridden after _ready()"), which
## `Main` triggers because it hides the surface from its own `_ready`, before the deferred callback
## that clears the warning has run. The warning is pointing at a real asymmetry: `size` is a
## `Vector2`, so `size.x = w` also writes the current — minimum-size-clamped — `size.y` back as an
## explicit bottom offset, pinning a height the anchors are supposed to stretch. The offset touches
## the horizontal axis only, and is the same knob `Main` and `workbench_preview` seed the surface
## with.
func _emit_reserved_width() -> void:
	offset_right = offset_left + _surface_width
	reserved_width_changed.emit(reserved_width())
