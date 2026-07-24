class_name HudWidgets

## THE SHARED HUD WIDGET FACTORY (docs/plan_hud_decomposition.md, the `DrawerComposeController` precursor).
##
## WHAT THIS IS. Every reusable Control the HUD builds out of raw Godot nodes and this project's own
## chrome vocabulary: the −/+ worker stepper in both its forms, the row labels and note/status parts
## it composes from, the zone CHROME (the column / block / plain-`Control` wrapper / child-clearing
## primitives all three Band-panel zones and the flat fallback host are assembled from), the stacked
## composition bar + its key (one primitive, two questions — PEOPLE and WORKFORCE), the zone section
## head and its `⋯` menu, the take-policy picker, the dim hint/section labels, the inline text link,
## the BBCode forecast readout — plus the two MUTATORS (`compact`, `set_label_tooltip`) that fix up a
## Control someone else made.
##
## WHY IT IS ITS OWN FILE. These are called from FOUR clusters that are being split apart: the drawer's
## compose blocks, the Band panel's WORK zone, its PARTIES zone, and the selection card. Lifting the
## drawer out of `Hud.gd` while these stayed behind would have meant injecting ~12 `Callable`s into the
## new controller (and the Band-panel extraction would then need the same 12 again). The same measurement
## that produced `SourceForecast` applies: a shared, stateless layer every consumer depends on beats
## injection, and beats a `_hud` back-reference that would weld pure chrome to the god object.
##
## EVERYTHING HERE IS `static` AND STATELESS — no node, no `_hud`, no snapshot cache. That is the
## invariant that makes the file safe to call from anywhere, and it is worth defending: if a function
## you want to add needs HUD state, pass the state in as a parameter instead of holding it (see
## `build_worker_stepper`'s `current_turn`, the one such parameter here).
##
## WHAT DELIBERATELY DID NOT MOVE. A factory that EMITS a HudLayer signal is not a widget factory —
## `_build_extend_pen_control` wires straight into `extend_pen_requested`, so it stays on `HudLayer`
## until it travels to `DrawerComposeController` with its diffing twins. So do the factories that read
## HudLayer members (`_build_band_picker`, `_build_compose_open_button`). The stylebox factories are
## `HudStyle`'s remit and now LIVE there (`role_card_stylebox` / `work_row_stylebox` /
## `work_inspector_stylebox`) — a zone builder styles through `HudStyle`, never by hand.
##
## CONSTS STAY ON `HudLayer` and are read back as `HudLayer.X` — the established pattern
## `SelectionCardController` and `TopBarReadouts` already use. Word/format VOCABULARY lives next door in
## `HudFormat`, which this file calls freely (both are static); the split is so `TopBarReadouts` can
## depend on the formatting without importing a widget factory.

## A "<label>   − N +" worker-count row. `on_change` is called with the new count
## when either stepper is pressed. `plus_enabled` gates the + (e.g. no idle workers).
## `status` is the row's action status (`FoodIcons.STATUS_WORKING` for a confirmed forage/hunt
## source; "" for the band-wide Scout/Warrior roles, which report no per-action state), and
## `pending` marks an optimistic (not-yet-confirmed) ORDER, which overrides the status: the row
## renders the `◌` glyph instead of `●` and its label reads amber, tying it to the amber pending hex
## on the map. Either way the state is a GLYPH, never a word — `tooltip` carries the words (see the
## action-status vocabulary above); the status line is appended to it here so every caller composes
## it the same way.
## `on_focus_source` (optional) makes the LABEL a clickable inline link that jumps the map to the
## row's source — a Forage tile / a hunted herd's live tile. It is a separate child from the
## steppers, so the −/+ buttons keep working untouched and the count stays right-aligned. Band-wide
## roles (Scout/Warrior) have no tile, so they pass nothing and keep a plain Label.
## `status_line` (default "") is the OPT-IN to the two-line form used ONLY by the Forage/Hunt
## Current-actions rows: when non-empty the title (icon + action + location) + the −/+ stepper ride
## line 1, and the yield/policy text (`status_line`) + the status glyph + the ⚠/overstaff/wasted notes
## drop to an indented, smaller secondary line 2 that WRAPS rather than widening the panel. When "",
## every existing caller (Scout/Warrior, the compose steppers) renders the unchanged single-line HBox.
## `arrival_schedule` (default empty) is the source's projected per-turn deliveries. When it has a GAP
## (`ArrivalStrip.has_gap`) the two-line form gains a third, indented line: the arrival tick strip that
## shows WHEN the steady average actually lands. A continuous source (or an unprojected row) has no
## lumpiness to explain and gets no strip. Ignored by the single-line form.
## `current_turn` is the ONE piece of HUD state this module needs — the sim turn the arrival strip
## labels its cells from (`HudLayer._band_labor.current_turn()`, threaded in rather than held). It is
## read ONLY on the two-line + gappy-schedule path, so it defaults to `ArrivalStrip`'s own
## `UNKNOWN_TURN` sentinel (which that Control already handles by labelling cells relatively) — a
## caller that passes an `arrival_schedule` owes the strip its turn.
static func build_worker_stepper(label_text: String, count: int, plus_enabled: bool, on_change: Callable, pending: bool = false, warn: bool = false, tooltip: String = "", note: String = "", on_focus_source: Callable = Callable(), status: String = "", muted_note: String = "", status_line: String = "", arrival_schedule: PackedFloat32Array = PackedFloat32Array(), current_turn: int = ArrivalStrip.UNKNOWN_TURN) -> Control:
    # Pending is a state of the ORDER, so it wins the glyph slot over whatever the action is doing.
    var status_key := FoodIcons.STATUS_PENDING if pending else status
    var row_tooltip := HudFormat.append_status_tooltip(tooltip, status_key)
    # Pending tints the row's IDENTITY amber (the title — it ties to the amber pending hex on the map);
    # a settled row reads plain INK.
    var row_ink: Color = HudStyle.WARN if pending else HudStyle.INK
    if status_line != "":
        return build_two_line_stepper(
            label_text, count, plus_enabled, on_change, warn, row_tooltip, note,
            on_focus_source, status_key, muted_note, status_line, row_ink, arrival_schedule,
            current_turn)
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", HudLayer.WORKER_STEPPER_SEPARATION)
    if row_tooltip != "":
        row.tooltip_text = row_tooltip
    var row_text := label_text + HudFormat.row_glyph_suffix(FoodIcons.for_status(status_key))
    row.add_child(build_row_name_label(row_text, row_ink, row_tooltip, on_focus_source))
    # Overhunting flag: a WARN-tinted ⚠ sits directly after the label (before the stepper), so an
    # overdrawn herd row pops without recoloring the whole label. Forage never trips this.
    if warn:
        row.add_child(build_row_note_label(HudLayer.OVERHUNT_FLAG, HudStyle.WARN, row_tooltip))
    # Overstaffing note ("· only 1 of 5 working"): WARN-tinted, sits after the label/⚠ so the wasted
    # labor reads at a glance without recoloring the whole row. Deliberately NOT the ⚠ flag — that
    # means "overdrawing" (ecological); this means "extra workers idle here" (see
    # `SourceForecast.source_yield_readout`). The tooltip carries the full explanation.
    if note != "":
        row.add_child(build_row_note_label(note, HudStyle.WARN, row_tooltip))
    # Understaffing note ("· 1.7 wasted"): MUTED (INK_FAINT), the low-key mirror of the WARN overstaff
    # note — it says "the source offered more than the crew carried home" (add workers), a softer nudge
    # than the ecological ⚠. Fed by `wasted_yield`; tooltip carries the full explanation.
    if muted_note != "":
        row.add_child(build_row_note_label(muted_note, HudStyle.INK_FAINT, row_tooltip))
    # A spacer (not name_label's expand) pushes the −/+ stepper to the right edge, keeping the
    # label + ⚠ adjacent at the left.
    var spacer := Control.new()
    spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(spacer)
    add_stepper_controls(row, count, plus_enabled, on_change)
    return row

## The two-line form of a worker-stepper row (see `build_worker_stepper`'s `status_line`): line 1 =
## the clickable title + spacer + −/+ stepper; line 2 = an indented, smaller secondary status carrying
## the yield/policy text, the status glyph, then the ⚠/overstaff/wasted notes — the SAME per-part
## colors the single-line path uses, just relocated below. Pending tints the TITLE amber (row 1's
## identity) and shows the ◌ glyph on row 2.
static func build_two_line_stepper(label_text: String, count: int, plus_enabled: bool, on_change: Callable, warn: bool, row_tooltip: String, note: String, on_focus_source: Callable, status_key: String, muted_note: String, status_line: String, row_ink: Color, arrival_schedule: PackedFloat32Array,
        current_turn: int) -> VBoxContainer:
    var col := VBoxContainer.new()
    col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    col.add_theme_constant_override("separation", HudLayer.TWO_LINE_STEPPER_SEPARATION)
    # Line 1: title + spacer + stepper. The status glyph is NOT appended to the title here (it lives on
    # line 2); the title keeps its click-to-jump link (or a plain Label for band-wide roles).
    var title_row := HBoxContainer.new()
    title_row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    title_row.add_theme_constant_override("separation", HudLayer.WORKER_STEPPER_SEPARATION)
    title_row.add_child(build_row_name_label(label_text, row_ink, row_tooltip, on_focus_source))
    var spacer := Control.new()
    spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    title_row.add_child(spacer)
    add_stepper_controls(title_row, count, plus_enabled, on_change)
    col.add_child(title_row)
    # Line 2: indented, smaller, wrapping status. A MarginContainer insets it past the icon; an
    # HFlowContainer wraps the parts to the next line rather than widening the panel (its min width is
    # the widest single part, small by construction).
    var status_margin := MarginContainer.new()
    status_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    status_margin.add_theme_constant_override("margin_left", int(HudLayer.STATUS_LINE_INDENT))
    var status_flow := HFlowContainer.new()
    status_flow.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    status_flow.add_theme_constant_override("h_separation", HudLayer.STATUS_LINE_SEPARATION)
    if row_tooltip != "":
        status_flow.tooltip_text = row_tooltip
    # The yield + policy glyph the caller composed (INK), then the status glyph (row_ink — WARN with the
    # ◌ when pending, tying it to the amber title), then ⚠ (WARN), the overstaff note (WARN), and the
    # wasted note (INK_FAINT).
    status_flow.add_child(build_status_part(status_line, HudStyle.INK))
    var status_glyph := FoodIcons.for_status(status_key)
    if status_glyph != "":
        status_flow.add_child(build_status_part(status_glyph, row_ink))
    if warn:
        status_flow.add_child(build_status_part(HudLayer.OVERHUNT_FLAG, HudStyle.WARN))
    if note != "":
        status_flow.add_child(build_status_part(note, HudStyle.WARN))
    if muted_note != "":
        status_flow.add_child(build_status_part(muted_note, HudStyle.INK_FAINT))
    status_margin.add_child(status_flow)
    col.add_child(status_margin)
    # Line 3 (only when the deliveries are LUMPY): the arrival tick strip, indented onto the same
    # gutter as line 2 so it reads as part of the row's secondary information. It stays INSIDE this
    # row's container, so the panel's section-block layout and the wide/tall packing are untouched.
    if ArrivalStrip.has_gap(arrival_schedule):
        var strip_margin := MarginContainer.new()
        strip_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        strip_margin.add_theme_constant_override("margin_left", int(HudLayer.STATUS_LINE_INDENT))
        var strip := ArrivalStrip.new()
        strip.set_schedule(arrival_schedule, current_turn)
        strip_margin.add_child(strip)
        col.add_child(strip_margin)
    return col

## The clickable title (or plain Label) shared by both stepper forms. `on_focus_source` (when valid)
## makes it an inline link that jumps the map to the source; a band-wide role passes nothing.
static func build_row_name_label(text: String, ink: Color, row_tooltip: String, on_focus_source: Callable) -> Control:
    if on_focus_source.is_valid():
        var link := Button.new()
        link.text = text
        link.alignment = HORIZONTAL_ALIGNMENT_LEFT
        HudStyle.apply_link_button(link, ink)
        link.tooltip_text = (row_tooltip + HudLayer.TOOLTIP_LINE_SEPARATOR if row_tooltip != "" else "") + HudLayer.SOURCE_ROW_FOCUS_HINT
        link.pressed.connect(func() -> void: on_focus_source.call())
        return link
    var plain := Label.new()
    plain.text = text
    plain.add_theme_color_override("font_color", ink)
    set_label_tooltip(plain, row_tooltip)
    return plain

## A single-line note Label (⚠ / overstaff / wasted) for the one-line stepper form.
static func build_row_note_label(text: String, color: Color, row_tooltip: String) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", color)
    set_label_tooltip(label, row_tooltip)
    return label

## A secondary status part (line 2 of the two-line form): rendered a touch smaller
## (`HudLayer.ALLOC_SECTION_FONT_SIZE`) than the title.
static func build_status_part(text: String, color: Color) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", color)
    label.add_theme_font_size_override("font_size", HudLayer.ALLOC_SECTION_FONT_SIZE)
    return label

## The shared −/+ stepper controls (minus, centered count, plus) appended to a row's HBox, so the
## one-line and two-line forms compose the same stepper. `on_change` fires with the new count.
static func add_stepper_controls(row: HBoxContainer, count: int, plus_enabled: bool, on_change: Callable, compact_chrome: bool = false) -> void:
    var minus := Button.new()
    minus.text = "−"
    minus.custom_minimum_size = Vector2(HudLayer.WORKER_STEPPER_BUTTON_WIDTH, 0)
    HudStyle.apply_button(minus, "ghost")
    minus.disabled = count <= 0
    minus.pressed.connect(func() -> void: on_change.call(count - HudLayer.WORKER_STEP))
    row.add_child(minus)
    var value := Label.new()
    value.text = str(count)
    value.custom_minimum_size = Vector2(HudLayer.WORKER_STEPPER_VALUE_WIDTH, 0)
    value.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    value.add_theme_color_override("font_color", HudStyle.INK if count > 0 else HudStyle.INK_FAINT)
    row.add_child(value)
    var plus := Button.new()
    plus.text = "+"
    plus.custom_minimum_size = Vector2(HudLayer.WORKER_STEPPER_BUTTON_WIDTH, 0)
    HudStyle.apply_button(plus, "ghost")
    plus.disabled = not plus_enabled
    plus.pressed.connect(func() -> void: on_change.call(count + HudLayer.WORKER_STEP))
    row.add_child(plus)
    if compact_chrome:
        for control in [minus, value, plus]:
            compact(control, HudLayer.WORK_STEPPER_FONT_SIZE, HudLayer.WORK_STEPPER_PADDING_V)

## Squeeze a control into a zone's fixed-height chrome row: smaller type, and a button's stylebox
## chrome trimmed vertically. `HudStyle._button_stylebox` pads 9px top and bottom, which alone makes a
## plain Button ~40px tall — taller than `HudLayer.WORK_ROW_HEIGHT`, `HudLayer.ZONE_HEAD_HEIGHT`, `HudLayer.WORK_CHIPS_HEIGHT`
## and `HudLayer.WORK_PAGER_HEIGHT` put together. Every one of those consts is a height the board's capacity
## maths SUBTRACTS, so a control that renders taller pushes the page off the bottom of the zone.
static func compact(control: Control, font_size: int, padding_v: int) -> void:
    control.add_theme_font_size_override("font_size", font_size)
    if not (control is Button):
        return
    for state in ["normal", "hover", "pressed", "disabled", "focus"]:
        var box: StyleBox = control.get_theme_stylebox(state)
        if box == null:
            continue
        var squeezed: StyleBox = box.duplicate()
        squeezed.content_margin_top = padding_v
        squeezed.content_margin_bottom = padding_v
        control.add_theme_stylebox_override(state, squeezed)

## Give a `Label` a tooltip AND the hover it needs to show one. **`Label` defaults to
## `MOUSE_FILTER_IGNORE`**, so setting `tooltip_text` on one and walking away is a SILENT no-op — the
## text is stored, the mouse never reaches the control, nothing ever appears. Six labels across this
## HUD shipped tooltips that had never once been seen. Route every Label tooltip through here.
static func set_label_tooltip(label: Label, text: String) -> void:
    label.tooltip_text = text
    label.mouse_filter = Control.MOUSE_FILTER_STOP if text != "" else Control.MOUSE_FILTER_IGNORE

## A dim uppercase section header inside the allocation panel ("Current actions" / "Band roles").
static func alloc_section_label(text: String) -> Label:
    var label := Label.new()
    label.text = text.to_upper()
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    label.add_theme_font_size_override("font_size", HudLayer.ALLOC_SECTION_FONT_SIZE)
    return label

## A dim wrapping hint line (role explanation / empty-state prompt).
static func alloc_hint_label(text: String) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    label.add_theme_font_size_override("font_size", HudLayer.ALLOC_SECTION_FONT_SIZE)
    label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    return label

## An inline text link (the inspector's three actions / the parties footer reasons).
static func build_inline_link(text: String, ink: Color, on_press: Callable) -> Button:
    var link := Button.new()
    link.text = text
    link.focus_mode = Control.FOCUS_NONE
    link.add_theme_font_size_override("font_size", HudLayer.ALLOC_SECTION_FONT_SIZE)
    HudStyle.apply_link_button(link, ink)
    link.pressed.connect(func() -> void: on_press.call())
    return link

## A one-line BBCode readout inside the assign controls (the live hunt-trip forecast / yield preview).
## Sized like the hint lines it sits among, but BBCode-capable so the forecast keeps its state colors.
static func forecast_label(bbcode: String) -> RichTextLabel:
    var label := RichTextLabel.new()
    label.bbcode_enabled = true
    label.fit_content = true
    label.scroll_active = false
    label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.add_theme_font_size_override("normal_font_size", HudLayer.ALLOC_SECTION_FONT_SIZE)
    label.add_theme_stylebox_override("normal", HudStyle.empty_stylebox())
    label.text = bbcode
    return label

# ---- Zone chrome (the containers all three zones + the flat fallback host are built from) --------

## The bar's height and its cell gap. Named here with `build_composition_bar`, their only reader.
const COMPOSITION_BAR_HEIGHT := 9.0
const COMPOSITION_BAR_SEPARATION := 2
## A segment's stretch ratio floor, so a 1-person segment is still a visible sliver rather than 0px.
const COMPOSITION_MIN_RATIO := 1.0
const COMPOSITION_SWATCH_SIZE := Vector2(8.0, 8.0)
const COMPOSITION_SWATCH_SEPARATION := 4
## The gap between a zone column's SECTIONS (blocks); the tighter within-block gap is
## `HudLayer.ZONE_BLOCK_SEPARATION`, which has readers on both sides of this boundary.
const ZONE_SECTION_SEPARATION := 12

## A proportional stacked bar. `segments` are `{key, count, color, tooltip}`; zero-count segments are
## dropped by the caller. Widths come from `size_flags_stretch_ratio`, so the bar fills its zone at
## any width without any measuring. Shared by the band zone's PEOPLE and WORKFORCE blocks — one
## stacked-bar primitive, two questions.
static func build_composition_bar(segments: Array) -> HBoxContainer:
    var bar := HBoxContainer.new()
    bar.custom_minimum_size = Vector2(0.0, COMPOSITION_BAR_HEIGHT)
    bar.add_theme_constant_override("separation", COMPOSITION_BAR_SEPARATION)
    for segment_variant in segments:
        var segment: Dictionary = segment_variant
        var cell := ColorRect.new()
        cell.color = segment.get("color", HudStyle.INK_FAINT)
        cell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        cell.size_flags_stretch_ratio = maxf(float(segment.get("count", 0)), COMPOSITION_MIN_RATIO)
        cell.custom_minimum_size = Vector2(0.0, COMPOSITION_BAR_HEIGHT)
        cell.tooltip_text = String(segment.get("tooltip", ""))
        cell.mouse_filter = Control.MOUSE_FILTER_STOP
        bar.add_child(cell)
    return bar

## The key under a stacked bar: one `▪ <key> <count>` chip per segment. An `HFlowContainer` so a
## narrow zone wraps the key rather than widening (the zone has a fixed width to respect).
static func build_composition_key(segments: Array, trailing: Control = null) -> HFlowContainer:
    var key := HFlowContainer.new()
    key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    key.add_theme_constant_override("h_separation", HudLayer.COMPOSITION_KEY_SEPARATION)
    for segment_variant in segments:
        var segment: Dictionary = segment_variant
        var chip := HBoxContainer.new()
        chip.add_theme_constant_override("separation", COMPOSITION_SWATCH_SEPARATION)
        chip.tooltip_text = String(segment.get("tooltip", ""))
        var swatch := ColorRect.new()
        swatch.color = segment.get("color", HudStyle.INK_FAINT)
        swatch.custom_minimum_size = COMPOSITION_SWATCH_SIZE
        swatch.size_flags_vertical = Control.SIZE_SHRINK_CENTER
        swatch.mouse_filter = Control.MOUSE_FILTER_IGNORE
        chip.add_child(swatch)
        var text := Label.new()
        text.text = "%s %d" % [String(segment.get("key", "")), int(segment.get("count", 0))]
        text.add_theme_font_size_override("font_size", HudLayer.COMPOSITION_KEY_FONT_SIZE)
        text.add_theme_color_override("font_color", HudStyle.INK_DIM)
        text.mouse_filter = Control.MOUSE_FILTER_IGNORE
        chip.add_child(text)
        key.add_child(chip)
    if trailing != null:
        key.add_child(trailing)
    return key

## A zone's content column: the VBox every zone builder fills.
static func make_zone_column() -> VBoxContainer:
    var col := VBoxContainer.new()
    col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    col.size_flags_vertical = Control.SIZE_EXPAND_FILL
    col.add_theme_constant_override("separation", ZONE_SECTION_SEPARATION)
    return col

## A tight sub-block inside a zone (bar + key + cards belong together, closer than the zone's own
## section spacing).
static func make_zone_block() -> VBoxContainer:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", HudLayer.ZONE_BLOCK_SEPARATION)
    return block

## Wrap a zone column in the plain `Control` the panel parents into its fixed-size zone host (the host
## reports no minimum size, so the content must anchor itself — see BandCityPanel `_make_zone_host`).
static func wrap_zone(content: VBoxContainer) -> Control:
    var host := Control.new()
    host.mouse_filter = Control.MOUSE_FILTER_IGNORE
    host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    host.size_flags_vertical = Control.SIZE_EXPAND_FILL
    host.add_child(content)
    content.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
    return host

## Detach-then-free a container's children. `queue_free` alone leaves them parented for the rest of
## the frame, so a rebuild-in-place (the work zone's re-page) would briefly stack old rows under new.
static func clear_children(node: Node) -> void:
    for child in node.get_children():
        node.remove_child(child)
        child.queue_free()

## A zone section head: an uppercase title on the left, a dim readout on the right, and an optional
## trailing `⋯` menu button. The one head vocabulary all three zones use.
static func zone_head(title: String, readout: String, menu: MenuButton = null, readout_color: Color = HudStyle.INK_DIM, readout_tooltip: String = "") -> HBoxContainer:
    var head := HBoxContainer.new()
    head.custom_minimum_size = Vector2(0.0, HudLayer.ZONE_HEAD_HEIGHT)
    head.add_theme_constant_override("separation", HudLayer.ZONE_HEAD_SEPARATION)
    head.add_child(alloc_section_label(title))
    var spacer := Control.new()
    spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
    head.add_child(spacer)
    if readout != "":
        var right := Label.new()
        right.text = readout
        right.add_theme_font_size_override("font_size", HudLayer.ZONE_HEAD_FONT_SIZE)
        right.add_theme_color_override("font_color", readout_color)
        set_label_tooltip(right, readout_tooltip)
        head.add_child(right)
    if menu != null:
        head.add_child(menu)
    return head

## The `⋯` section menu: a `MenuButton`, so its popup is a WINDOW and opening it cannot change any
## zone's layout height (the whole zone model depends on heights not moving). `entries` is an ordered
## array of `{label, disabled, on_pick}` dictionaries.
static func build_section_menu(entries: Array, tooltip: String) -> MenuButton:
    var button := MenuButton.new()
    button.text = HudLayer.SECTION_MENU_GLYPH
    button.tooltip_text = tooltip
    button.focus_mode = Control.FOCUS_NONE
    button.custom_minimum_size = Vector2(HudLayer.SECTION_MENU_WIDTH, 0.0)
    HudStyle.apply_button(button, "ghost")
    compact(button, HudLayer.ZONE_HEAD_FONT_SIZE, HudLayer.ZONE_MENU_PADDING_V)
    var popup := button.get_popup()
    var picks: Array[Callable] = []
    for entry_variant in entries:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        var index := picks.size()
        popup.add_item(String(entry.get("label", "")), index)
        popup.set_item_disabled(index, bool(entry.get("disabled", false)))
        var pick: Variant = entry.get("on_pick", null)
        picks.append(pick if pick is Callable else Callable())
    popup.id_pressed.connect(func(id: int) -> void:
        if id >= 0 and id < picks.size() and picks[id].is_valid():
            picks[id].call())
    return button

## The party stepper row, shared by both missions so they cannot drift apart in shape.
static func build_party_stepper_row(count: int, party_max: int, on_change: Callable) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudLayer.WORKER_STEPPER_SEPARATION)
    var key := Label.new()
    key.text = HudLayer.COMPOSE_FIELD_PARTY
    key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(key)
    add_stepper_controls(row, count, count < party_max, on_change)
    return row

## The unmet-prerequisite reasons a `gates` dict holds for one policy — empty (available) for an
## absent key. The single reader of the gates contract, so callers never re-assert its shape.
static func gate_reasons(gates: Dictionary, policy: String) -> Array:
    var reasons: Variant = gates.get(policy, null)
    return reasons if reasons is Array else []

## The take-policy radio; `on_pick` fires with the chosen policy. The highlighted option is
## `selected` — REQUIRED, and always the caller's own composed/standing rung: this builder is shared
## by four unrelated surfaces (the work inspector, the party compose sheet, the herd drawer, the
## forage drawer) and owns none of their state. `options` is the option set for this source kind —
## the four extractive rungs by default, plus that kind's INVESTMENT rungs on the forage/herd assign
## controls (HudLayer.FORAGE_POLICY_OPTIONS / HudLayer.HUNT_POLICY_OPTIONS). A `selected` that is not in `options`
## simply highlights nothing; a caller offering a narrower set than its source can stand on owes the
## player a line saying so (see `HudLayer.WORK_INSPECT_STANDING_INVESTMENT_FORMAT`).
##
## `gates` maps a policy → an Array[String] of its unmet-prerequisite reasons (empty / absent =
## available). A gated option is **shown, greyed, and explained** rather than hidden: it is disabled,
## its tooltip carries every reason (one per line), and the reasons render under the row — one
## compact line when there is a single reason, a "<policy> needs:" header + one bullet per reason
## when there are several (each reason now names its remedy, so two on one line would not fit). The
## player discovers the rung, what it costs to unlock, AND how to unlock it, BEFORE trying to use it.
static func build_policy_picker(
    on_pick: Callable,
    selected: String,
    options: Array = HudLayer.LABOR_HUNT_POLICIES,
    gates: Dictionary = {},
    takes: Dictionary = {},
    columns: int = 0,
    collapse_other_gates: bool = false) -> VBoxContainer:
    var current := selected
    var block := VBoxContainer.new()
    block.add_theme_constant_override("separation", HudLayer.WORKER_STEPPER_SEPARATION)
    # Wrap the rung buttons 3 per row (a GridContainer) so the six-rung pickers read as two rows of
    # three; a small picker (≤4 rungs, the expedition) stays a single row so it never strands a lone
    # sub-width button. Each button EXPAND_FILLs so the three in a row are equal width and fill the panel.
    var grid := GridContainer.new()
    # `columns > 0` overrides the width-driven default: a zone is a FIXED-width box, and a picker whose
    # buttons sum past it raises the zone content's minimum width, which pushes the whole zone column
    # out past its host (where it is clipped) — taking the section menu beside it off the edge.
    if columns > 0:
        grid.columns = columns
    else:
        grid.columns = maxi(1, options.size()) if options.size() <= HudLayer.POLICY_PICKER_MAX_SINGLE_ROW else HudLayer.POLICY_PICKER_COLUMNS
    grid.add_theme_constant_override("h_separation", HudLayer.WORKER_STEPPER_SEPARATION)
    grid.add_theme_constant_override("v_separation", HudLayer.WORKER_STEPPER_SEPARATION)
    for policy in options:
        var policy_key := String(policy)
        var icon := FoodIcons.for_policy(policy_key)
        var reasons := gate_reasons(gates, policy_key)
        var btn := Button.new()
        # ONE-LINE FACE: the FoodIcons policy glyph (the same icon the map's yield labels append, so a
        # policy reads identically on the picker and on the worked tile/herd) + the compact per-policy
        # metric, NO name — so the rungs stay compact enough to wrap 3-per-row (see the grid above)
        # without overflow. The name + full metric live in the
        # tooltip. The metrics still read as ASCENDING (Sustain < Surplus < Market < Eradicate). A rung
        # with no metric (older snapshot / metric-less gated rung) falls back to the name so the face
        # is never a lone glyph.
        var take: Variant = takes.get(policy_key, null)
        var compact_face := String((take as Dictionary).get("compact", "")) if take is Dictionary else ""
        var full := String((take as Dictionary).get("full", "")) if take is Dictionary else ""
        var face := compact_face if compact_face != "" else policy_key.capitalize()
        btn.text = "%s%s" % [HudFormat.source_icon_prefix(icon), face]
        # EXPAND_FILL so the buttons sharing a grid row are equal width and fill the panel content width.
        btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        HudStyle.apply_button(btn, "primary" if policy_key == current else "ghost")
        # Tooltip names the rung for EVERY button (the face no longer does), led by its full metric;
        # a gated button appends its gate reasons below, so a hover tells you what the rung is AND why
        # it is locked.
        var name_line := HudLayer.POLICY_TOOLTIP_NAME_FORMAT % [policy_key.capitalize(), full] \
            if full != "" else policy_key.capitalize()
        var tooltip_lines: Array[String] = [name_line]
        btn.disabled = not reasons.is_empty()
        if btn.disabled:
            tooltip_lines.append_array(reasons)
        else:
            btn.pressed.connect(func() -> void: on_pick.call(policy_key))
        btn.tooltip_text = HudLayer.GATE_REASON_TOOLTIP_SEPARATOR.join(tooltip_lines)
        grid.add_child(btn)
    block.add_child(grid)
    # Spell the unmet prerequisites out in the panel — a greyed button alone doesn't teach. A caller
    # that is TIGHT ON HEIGHT may opt into collapsing the rungs it is not composing (see
    # HudLayer.GATE_REASON_COLLAPSED_ONE_FORMAT); by default every gated rung still teaches in full.
    for policy in options:
        var policy_key := String(policy)
        var reasons := gate_reasons(gates, policy_key)
        if reasons.is_empty():
            continue
        var titled := HudFormat.policy_face(policy_key)
        if collapse_other_gates and policy_key != current:
            # Collapsed: the count, plus every reason in the line's own tooltip. A Label ignores the
            # mouse by default, so the filter must be set with the text or the tooltip never shows.
            var collapsed := alloc_hint_label(HudLayer.GATE_REASON_COLLAPSED_ONE_FORMAT % titled \
                if reasons.size() == 1 \
                else HudLayer.GATE_REASON_COLLAPSED_MANY_FORMAT % [titled, reasons.size()])
            set_label_tooltip(collapsed, HudLayer.GATE_REASON_TOOLTIP_SEPARATOR.join(reasons))
            block.add_child(collapsed)
            continue
        if reasons.size() == 1:
            block.add_child(alloc_hint_label(HudLayer.GATE_REASON_LINE_FORMAT % [titled, reasons[0]]))
            continue
        block.add_child(alloc_hint_label(HudLayer.GATE_REASON_HEADER_FORMAT % titled))
        for reason in reasons:
            block.add_child(alloc_hint_label(HudLayer.GATE_REASON_BULLET_FORMAT % reason))
    return block
