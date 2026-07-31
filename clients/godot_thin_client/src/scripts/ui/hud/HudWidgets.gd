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
## CONSTS LIVE IN THE TOPIC VOCAB MODULES (`HudConst` / the matching `Hud*Vocab`) and are read as
## `Module.X`. Word/format VOCABULARY lives next door in
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
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    if row_tooltip != "":
        row.tooltip_text = row_tooltip
    var row_text := label_text + HudFormat.row_glyph_suffix(FoodIcons.for_status(status_key))
    row.add_child(build_row_name_label(row_text, row_ink, row_tooltip, on_focus_source))
    # Overhunting flag: a WARN-tinted ⚠ sits directly after the label (before the stepper), so an
    # overdrawn herd row pops without recoloring the whole label. Forage never trips this.
    if warn:
        row.add_child(build_row_note_label(HudComposeVocab.OVERHUNT_FLAG, HudStyle.WARN, row_tooltip))
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
    col.add_theme_constant_override("separation", HudWorkVocab.TWO_LINE_STEPPER_SEPARATION)
    # Line 1: title + spacer + stepper. The status glyph is NOT appended to the title here (it lives on
    # line 2); the title keeps its click-to-jump link (or a plain Label for band-wide roles).
    var title_row := HBoxContainer.new()
    title_row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    title_row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
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
    status_margin.add_theme_constant_override("margin_left", int(HudWorkVocab.STATUS_LINE_INDENT))
    var status_flow := HFlowContainer.new()
    status_flow.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    status_flow.add_theme_constant_override("h_separation", HudWorkVocab.STATUS_LINE_SEPARATION)
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
        status_flow.add_child(build_status_part(HudComposeVocab.OVERHUNT_FLAG, HudStyle.WARN))
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
        strip_margin.add_theme_constant_override("margin_left", int(HudWorkVocab.STATUS_LINE_INDENT))
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
        link.tooltip_text = (row_tooltip + SourceForecast.TOOLTIP_LINE_SEPARATOR if row_tooltip != "" else "") + HudWorkVocab.SOURCE_ROW_FOCUS_HINT
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
## (`HudWorkVocab.ALLOC_SECTION_FONT_SIZE`) than the title.
static func build_status_part(text: String, color: Color) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", color)
    label.add_theme_font_size_override("font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
    return label

## The shared −/+ stepper controls (minus, centered count, plus) appended to a row's HBox, so the
## one-line and two-line forms compose the same stepper. `on_change` fires with the new count.
static func add_stepper_controls(row: HBoxContainer, count: int, plus_enabled: bool, on_change: Callable, compact_chrome: bool = false) -> void:
    var minus := Button.new()
    minus.text = "−"
    minus.custom_minimum_size = Vector2(HudWorkVocab.WORKER_STEPPER_BUTTON_WIDTH, 0)
    HudStyle.apply_button(minus, "ghost")
    minus.disabled = count <= 0
    minus.pressed.connect(func() -> void: on_change.call(count - HudConst.WORKER_STEP))
    row.add_child(minus)
    var value := Label.new()
    value.text = str(count)
    value.custom_minimum_size = Vector2(HudWorkVocab.WORKER_STEPPER_VALUE_WIDTH, 0)
    value.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    value.add_theme_color_override("font_color", HudStyle.INK if count > 0 else HudStyle.INK_FAINT)
    row.add_child(value)
    var plus := Button.new()
    plus.text = "+"
    plus.custom_minimum_size = Vector2(HudWorkVocab.WORKER_STEPPER_BUTTON_WIDTH, 0)
    HudStyle.apply_button(plus, "ghost")
    plus.disabled = not plus_enabled
    plus.pressed.connect(func() -> void: on_change.call(count + HudConst.WORKER_STEP))
    row.add_child(plus)
    if compact_chrome:
        for control in [minus, value, plus]:
            compact(control, HudWorkVocab.WORK_STEPPER_FONT_SIZE, HudWorkVocab.WORK_STEPPER_PADDING_V)

## Squeeze a control into a zone's fixed-height chrome row: smaller type, and a button's stylebox
## chrome trimmed vertically. `HudStyle._button_stylebox` pads 9px top and bottom, which alone makes a
## plain Button ~40px tall — taller than `HudWorkVocab.WORK_ROW_HEIGHT`, `HudWorkVocab.ZONE_HEAD_HEIGHT`, `HudWorkVocab.WORK_CHIPS_HEIGHT`
## and `HudWorkVocab.WORK_PAGER_HEIGHT` put together. Every one of those consts is a height the board's capacity
## maths SUBTRACTS, so a control that renders taller pushes the page off the bottom of the zone.
static func compact(control: Control, font_size: int, padding_v: int) -> void:
    control.add_theme_font_size_override("font_size", font_size)
    trim_button_padding(control, padding_v)

## The chrome half of `compact`, on its own for a control that must keep its TYPE SIZE and trim only
## the box around it. Leaves a button's SIDE padding exactly as `HudStyle` authored it — a zone row is
## short on height and not on width. Does nothing to a non-Button (a Label has no stylebox to squeeze).
static func trim_button_padding(control: Control, padding_v: int) -> void:
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
    label.add_theme_font_size_override("font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
    return label

## A dim wrapping hint line (role explanation / empty-state prompt).
static func alloc_hint_label(text: String) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    label.add_theme_font_size_override("font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
    label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    return label

## An inline text link (the inspector's three actions / the parties footer reasons).
static func build_inline_link(text: String, ink: Color, on_press: Callable) -> Button:
    var link := Button.new()
    link.text = text
    link.focus_mode = Control.FOCUS_NONE
    link.add_theme_font_size_override("font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
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
    label.add_theme_font_size_override("normal_font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
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
## `HudWorkVocab.ZONE_BLOCK_SEPARATION`, which has readers on both sides of this boundary.
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
    key.add_theme_constant_override("h_separation", HudWorkVocab.COMPOSITION_KEY_SEPARATION)
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
        text.add_theme_font_size_override("font_size", HudWorkVocab.COMPOSITION_KEY_FONT_SIZE)
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
    block.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
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
    head.custom_minimum_size = Vector2(0.0, HudWorkVocab.ZONE_HEAD_HEIGHT)
    head.add_theme_constant_override("separation", HudWorkVocab.ZONE_HEAD_SEPARATION)
    head.add_child(alloc_section_label(title))
    var spacer := Control.new()
    spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
    head.add_child(spacer)
    if readout != "":
        var right := Label.new()
        right.text = readout
        right.add_theme_font_size_override("font_size", HudWorkVocab.ZONE_HEAD_FONT_SIZE)
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
    button.text = HudWorkVocab.SECTION_MENU_GLYPH
    button.tooltip_text = tooltip
    button.focus_mode = Control.FOCUS_NONE
    button.custom_minimum_size = Vector2(HudWorkVocab.SECTION_MENU_WIDTH, 0.0)
    HudStyle.apply_button(button, "ghost")
    compact(button, HudWorkVocab.ZONE_HEAD_FONT_SIZE, HudWorkVocab.ZONE_MENU_PADDING_V)
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
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    var key := Label.new()
    key.text = HudComposeVocab.COMPOSE_FIELD_PARTY
    key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(key)
    add_stepper_controls(row, count, count < party_max, on_change)
    return row

## The rung a policy-picker Button stands for, as `Button` meta. THE ONE STABLE HANDLE on a rung: the
## face is presentation and has already changed twice (glyph+metric → glyph+name over metric → that
## same pair as child Labels at two sizes), so a harness matching on `btn.text` breaks with every
## visual pass. `band_panel_preview._picker_rung_buttons` reads this.
const POLICY_RUNG_META := "policy"

## The "send a hunting expedition" CONFIRM button, as `Button` meta — set by BOTH hosts that build
## one (the herd drawer's compose control and the Band panel's parties compose sheet). Same reason as
## the rung meta above, only more so: this button's face is the raid VERDICT
## (`SourceForecast.style_send_hunt_button` writes "Send Expedition" / "Send Anyway (≈54
## turns)" / "Send (brings nothing home)" / "Herd too lean to raid"), so text is the one thing a
## harness cannot match on. `tools/command_guard.gd` presses it through this meta — it is the ONLY
## way to reach those two emit sites, whose payload-building lives in an inline `pressed` lambda.
const SEND_HUNT_CONFIRM_META := "send_hunt_confirm"

## The improvement CONTROL's checkbox, as `Control` meta — the stable handle on the second axis, for
## the same reason `POLICY_RUNG_META` is the stable handle on a rung. Its value is the IMPROVEMENT key
## (`"cultivate"` / `"sow"` / `"tame"` / `"corral"`), so a harness can assert both which rung is
## offered and, from the node's own type, which of the three states it is in: a `CheckBox` is offered
## or running (`button_pressed` tells them apart) and a `Label` is done.
const IMPROVEMENT_CONTROL_META := "improvement"

## **THE IMPROVEMENT CONTROL** (issue #442) — the second axis's whole widget, in the ONE of three
## states the caller resolved, plus its gate reasons / pause line beneath.
##
## `state` is `IMPROVEMENT_STATE_*`. The face and the tooltip are the CALLER's words (it knows the
## rung, its payoff and its meter); what lives here is the SHAPE the three states share, so the two
## webs cannot drift into two different-looking controls:
##
##   OFFERED — an unchecked `CheckBox`, enabled iff ungated. A gated one is **shown, unchecked and
##             explained**, exactly as a gated rung is: disabled, its reasons in the tooltip AND
##             spelled out beneath it, because a greyed control alone does not teach.
##   RUNNING — a checked, **LIVE** `CheckBox`: unchecking abandons the build (`abandon_improvement`).
##   DONE    — a plain `Label`. Nothing to uncheck, nothing to clear.
##
## **UNCHECKING IS NEVER GATED, AND THAT IS LOAD-BEARING.** The abandon path asks for no knowledge, no
## ceiling, no site and pointedly no `Thriving` check, because abandoning a STALLED build is exactly
## when a player reaches for it — so `notes` on a RUNNING control (the WARN pause line) must NOT
## disable it, unlike `notes` on an OFFERED one (unmet prerequisites, which genuinely block). A
## condition that greys a running box is a bug, not a safeguard.
##
## `on_toggle` is called with **the improvement's NEW value** — the rung's key when a box is checked,
## `IMPROVEMENT_NONE` when one is unchecked — so a caller writes the value it is given rather than
## re-deriving the direction from the control's state.
##
## `notes` render beneath in the hint style — gate reasons when offered, the WARN-amber pause line
## when running (`warn_notes` picks the tint). Returns the whole block, so a caller adds one child.
const IMPROVEMENT_STATE_OFFERED := "offered"
const IMPROVEMENT_STATE_RUNNING := "running"
const IMPROVEMENT_STATE_DONE := "done"
## An offer the source cannot take yet. **A LABEL, not a disabled checkbox** — the control's shape
## says whether this is a CHOICE or a FACT, and an unmet prerequisite is a fact. It shipped once as a
## greyed checkbox reading "Cultivate this patch · then 0.04 food …" with the reason on a second line
## beneath, which put an OFFER the player cannot accept directly above the sentence explaining that
## they cannot accept it — the card arguing with itself. The reason is now the control's own text.
const IMPROVEMENT_STATE_GATED := "gated"

static func build_improvement_control(improvement: String, state: String, face: String,
        tooltip: String, on_toggle: Callable, notes: Array = [],
        warn_notes: bool = false) -> VBoxContainer:
    var block := VBoxContainer.new()
    block.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    if state == IMPROVEMENT_STATE_GATED:
        # Same Label treatment as DONE — both are states rather than choices — but in the muted ink a
        # prerequisite deserves rather than DONE's HEALTHY, which would read as an achievement.
        var locked := Label.new()
        locked.text = face
        locked.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
        locked.add_theme_color_override("font_color", HudStyle.INK_FAINT)
        locked.set_meta(IMPROVEMENT_CONTROL_META, improvement)
        set_label_tooltip(locked, tooltip)
        block.add_child(locked)
    elif state == IMPROVEMENT_STATE_DONE:
        # A state, not a choice. `set_label_tooltip` because a bare `tooltip_text` on a Label is a
        # SILENT no-op (Labels default to MOUSE_FILTER_IGNORE).
        var done := Label.new()
        done.text = face
        # NO font-size override, so the control sits at the sheet's body size — the same size the
        # STANCE rungs' names wear (`_policy_rung_line`'s `font_size 0`). The two are peer AXES, and
        # rendering the second one at hint size made the whole decision read as a footnote to the
        # first. The gate/pause notes below it stay at hint size, where they belong.
        done.add_theme_color_override("font_color", HudStyle.HEALTHY)
        done.set_meta(IMPROVEMENT_CONTROL_META, improvement)
        set_label_tooltip(done, tooltip)
        block.add_child(done)
    else:
        var box := CheckBox.new()
        box.text = face
        box.tooltip_text = tooltip
        box.set_meta(IMPROVEMENT_CONTROL_META, improvement)
        var running := state == IMPROVEMENT_STATE_RUNNING
        box.button_pressed = running
        # ONLY an OFFERED box is ever disabled, and only by an unmet prerequisite. A RUNNING one stays
        # live however loudly its notes read — see the ungated-abandon rule above.
        box.disabled = not running and not notes.is_empty()
        if not box.disabled:
            # `toggled`, not `pressed`: the handler needs the NEW state, and reading `button_pressed`
            # back inside a `pressed` handler is the kind of ordering assumption that silently
            # inverts. `pressed` here is the box's state AFTER the click, so it maps straight onto
            # "which improvement is composed now".
            box.toggled.connect(func(pressed: bool) -> void:
                on_toggle.call(improvement if pressed else SourceForecast.IMPROVEMENT_NONE))
        block.add_child(box)
    for note in notes:
        var line := alloc_hint_label(String(note))
        if warn_notes:
            line.add_theme_color_override("font_color", HudStyle.WARN)
        block.add_child(line)
    return block

## ONE RUNG of the policy picker: a clickable, styleable, disable-able `Button` with a TWO-LINE face
## whose lines carry DIFFERENT TYPE — which `Button.text` structurally cannot do (one font size per
## button), so the lines are child Labels and the button's own `text` stays empty.
##
## THE SHAPE. A `MarginContainer` with zero margins lays every child to its full rect and takes its
## minimum size from the largest, so the cell is exactly: the button, filling it (the box, the border,
## the hover/pressed/focus/disabled chrome, the click and the tooltip), with the label stack painted
## over it, inset by `POLICY_PICKER_PADDING_*` and IGNORING the mouse so every click reaches the button
## beneath. The stack is what SIZES the cell — an empty-text Button's minimum is just its own content
## margins — which is the whole reason the button cannot be the parent: a `Button` is not a Container
## and would not grow to fit children, leaving the second line to be laid out by hand.
##
## THE TINT IS ONE COLOUR, DERIVED ONCE, and that is the invariant to preserve. The single-`Button.text`
## face this replaced tinted both lines together for free; here the caller asks
## `HudStyle.button_font_color` ONCE and passes the answer in as `tint`, and line 2 is that same colour
## at `POLICY_PICKER_METRIC_ALPHA`, so a selected, disabled, standing-but-gated — or any future warned —
## rung moves both lines by construction. Never give line 2 a literal colour of its own; that is exactly
## the desynchronisation this note exists to prevent. (`modulate` would also inherit, but it multiplies
## the BOX too, so a disabled rung would be dimmed twice — once by the disabled stylebox's own faded
## fill, once again by the tint.) The tint arrives as a PARAMETER rather than being re-derived from the
## variant here, because the disabled tint now also depends on whether the rung is the SELECTED one —
## a fact this cell does not have and the caller does.
static func _policy_rung_cell(btn: Button, title: String, metric: String,
        tint: Color) -> MarginContainer:
    var cell := MarginContainer.new()
    cell.add_child(btn)
    var pad := MarginContainer.new()
    pad.mouse_filter = Control.MOUSE_FILTER_IGNORE
    for side in ["left", "right"]:
        pad.add_theme_constant_override("margin_" + side, HudWorkVocab.POLICY_PICKER_PADDING_H)
    for side in ["top", "bottom"]:
        pad.add_theme_constant_override("margin_" + side, HudWorkVocab.POLICY_PICKER_PADDING_V)
    var stack := VBoxContainer.new()
    stack.mouse_filter = Control.MOUSE_FILTER_IGNORE
    stack.add_theme_constant_override("separation", HudWorkVocab.POLICY_PICKER_FACE_SEPARATION)
    stack.add_child(_policy_rung_line(title, tint, 0))
    if metric != "":
        stack.add_child(_policy_rung_line(metric,
            Color(tint, tint.a * HudWorkVocab.POLICY_PICKER_METRIC_ALPHA),
            HudWorkVocab.POLICY_PICKER_METRIC_FONT_SIZE))
    pad.add_child(stack)
    cell.add_child(pad)
    return cell

## One line of a rung's face. `font_size` 0 leaves the theme's own size — what line 1 wants, so the
## rung name renders at exactly the size the button's `text` did before the face became Labels.
static func _policy_rung_line(text: String, tint: Color, font_size: int) -> Label:
    var label := Label.new()
    label.text = text
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    label.add_theme_color_override("font_color", tint)
    if font_size > 0:
        label.add_theme_font_size_override("font_size", font_size)
    return label

## The harvest-STANCE radio; `on_pick` fires with the chosen stance. The highlighted option is
## `selected` — REQUIRED, and always the caller's own composed/standing rung: this builder is shared
## by four unrelated surfaces (the work inspector, the party compose sheet, the herd drawer, the
## forage drawer) and owns none of their state.
##
## `options` is `SourceForecast.LABOR_HUNT_POLICIES` for every caller now (issue #442). The parameter
## survives because the picker has no business knowing the stance list, but the per-kind six-rung sets
## it used to be handed are gone: an improvement is its own control, not a fifth and sixth radio.
##
## **NO RUNG HERE IS EVER GATED** (issue #442). A `gates` dict, the greyed-and-explained rendering it
## drove, and the height-saving `collapse_other_gates` opt-in beside it are all gone: a harvest stance
## has no prerequisite and never retires, so every one of the four is always live. Unmet prerequisites
## belong to the IMPROVEMENT axis now, and `HudWidgets.build_improvement_control` renders them in the
## same shown-unchecked-and-explained shape this used to.
##
## A rung's `takes` entry may carry a THIRD key beside `compact`/`full` — **`note`**, a caveat on the
## metric appended under the tooltip's name + metric line. It exists for the hunt sheet's averaging-
## window disclaimer (`HudComposeVocab.HUNT_AVG_WINDOW_FORMAT`), which qualifies the rate on the very
## face it hangs off, and which as a standing body line made the hunt sheet read a paragraph longer than
## the forage sheet beside it. It is per RUNG, not per picker, because the span the rate averages over
## is a property of the rung; a picker whose takes carry no `note` (forage, expedition) is unchanged.
static func build_policy_picker(
    on_pick: Callable,
    selected: String,
    options: Array = SourceForecast.LABOR_HUNT_POLICIES,
    takes: Dictionary = {},
    columns: int = 0) -> VBoxContainer:
    var current := selected
    var block := VBoxContainer.new()
    block.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    # Wrap the rung buttons at most `POLICY_PICKER_COLUMNS` (3) per row (a GridContainer), so a six-rung
    # picker reads 3 + 3 and the four extractive rungs read 3 + 1 with Eradicate alone on the second row.
    # THE CEILING IS UNIFORM ON PURPOSE: four abreast made the expedition launch picker a different
    # creature from the local hunt beside it, and set the widest compose card's width off a row that never
    # needed to be that wide. The lone rung is not stretched — a GridContainer gives it its COLUMN's
    # width, so it sits under the first cell above at exactly that cell's width, which reads deliberate.
    # **THE THREE-ACCOUNT FACE DOES NOT LOWER IT, and that was MEASURED rather than reasoned** (#426):
    # a wide-face ceiling of 2 was built on the assumption that `0.60 food · 0.01 trade · 0.20 fodder`
    # three abreast would overrun the sheet, and the rendered frame says otherwise — the picker comes
    # out 555px against the deer hunt picker's long-standing 546, nothing clips, and 3 + 3 reads
    # better than the 2 + 2 + 2 the ceiling produced. Do not re-add it without a frame that overruns.
    var grid := GridContainer.new()
    # `columns > 0` CLAMPS the default DOWN, never up: a zone is a FIXED-width box, and a picker whose
    # buttons sum past it raises the zone content's minimum width, which pushes the whole zone column
    # out past its host (where it is clipped) — taking the section menu beside it off the edge.
    var wanted := columns if columns > HudWorkVocab.POLICY_PICKER_AUTO_COLUMNS else options.size()
    grid.columns = clampi(wanted, 1, HudWorkVocab.POLICY_PICKER_COLUMNS)
    grid.add_theme_constant_override("h_separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    grid.add_theme_constant_override("v_separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    for policy in options:
        var policy_key := String(policy)
        var btn := Button.new()
        # TWO-LINE FACE, one line per AXIS — the fix for the axis collision the one-line face had:
        #   line 1  WHICH RUNG — `HudFormat.policy_face`, the FoodIcons policy glyph welded to the
        #           rung's NAME (`♻ Sustain`). The same glyph the map's yield labels append, so a
        #           policy reads identically on the picker and on the worked tile/herd, and the same
        #           face the gate-reason lines and the work inspector use.
        #   line 2  WHAT IT PAYS — the per-policy metric with its products NAMED IN WORDS
        #           (`0.96 food · 0.24 trade`, `SourceForecast.picker_products`), one step SMALLER and
        #           one step quieter, so the name leads the glance and the numbers answer it.
        # The old face put the rung glyph and the trade-goods glyph adjacent in ONE line at one weight,
        # where `♻ ⬆ ⇊ 💀` (which rung) and `⇄` (which product) could not be told apart — and dropping
        # the rung's name left `⬆` beside `⇊` reading as good-vs-bad rather than as two rungs of one
        # ladder. Naming the rung is what defuses that, so the glyphs themselves are unchanged.
        # A rung with no metric (the work inspector's picker, which passes none) is line 1 alone.
        var take: Variant = takes.get(policy_key, null)
        var metric := String((take as Dictionary).get("compact", "")) if take is Dictionary else ""
        var full := String((take as Dictionary).get("full", "")) if take is Dictionary else ""
        # The optional per-rung tooltip caveat (see the header) — the hunt sheet's averaging window.
        var note := String((take as Dictionary).get("note", "")) if take is Dictionary else ""
        var is_selected := policy_key == current
        var variant := "primary" if is_selected else "ghost"
        # `policy` meta, not the face string: the face is presentation (it grew a name, then a second
        # line, and its text now lives on a child Label), so a harness that identified a rung by reading
        # `btn.text` broke each time. The meta is the rung's identity and never moves.
        btn.set_meta(POLICY_RUNG_META, policy_key)
        # **THE SELECTED-AND-GATED STATE IS GONE** (issue #442 retiring #420). It existed because a
        # completed build stranded the picker on a dead rung — the band standing on a Cultivate whose
        # patch had just finished. A stance is never retired and never gated, so there is no rung left
        # that is simultaneously the current choice and unavailable, and `HudStyle.apply_button` no
        # longer carries the flag that rendered one.
        HudStyle.apply_button(btn, variant)
        # Tooltip carries the VERBOSE metric the face compacts ("up to +2.33/turn · ⇄ +0.34 trade
        # goods/turn"), led by the rung name.
        var name_line := HudComposeVocab.POLICY_TOOLTIP_NAME_FORMAT % [policy_key.capitalize(), full] \
            if full != "" else policy_key.capitalize()
        btn.pressed.connect(func() -> void: on_pick.call(policy_key))
        btn.tooltip_text = HudFormat.join_tooltip_lines([name_line, note])
        # EXPAND_FILL on the CELL (which is what the grid lays out now), so the rungs sharing a row are
        # equal width and fill the panel content width.
        var cell := _policy_rung_cell(btn, HudFormat.policy_face(policy_key), metric,
            HudStyle.button_font_color(variant, btn.disabled))
        cell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        grid.add_child(cell)
    block.add_child(grid)
    return block
