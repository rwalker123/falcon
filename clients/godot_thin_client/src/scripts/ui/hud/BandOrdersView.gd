class_name BandOrdersView

## **THE WORK TAB'S ORDERS GROUP** (#860) -- `<Leader>'s orders`, drawn from the band's `band_orders`
## wire. All-`static`: it is handed the already-built row nodes (today's `_build_work_row` /
## `_build_extract_row`, unchanged) and the controller for the order-name link, and returns one
## `VBoxContainer`. The words and the state logic are `HudOrdersVocab`'s.

# ---- Small shared widgets (the sheet sections use them too) --------------------------------------

static func text(content: String, size: int, ink: Color, wrap: bool = false) -> Label:
    var label := Label.new()
    label.text = content
    label.add_theme_font_size_override("font_size", size)
    label.add_theme_color_override("font_color", ink)
    label.mouse_filter = Control.MOUSE_FILTER_PASS
    if wrap:
        label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
        label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    return label

static func priority_chip(level: String) -> Control:
    var chip := PanelContainer.new()
    chip.add_theme_stylebox_override("panel", HudStyle.chip_stylebox(HudStyle.LINE))
    chip.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
    chip.set_meta(HudOrdersVocab.ORDER_PRIORITY_META, HudWorkVocab.work_priority_of(level))
    chip.add_child(text(HudOrdersVocab.PRIORITY_CHIP_FORMAT % HudOrdersVocab.priority_face(level),
        HudOrdersVocab.SMALL_FONT_SIZE, HudStyle.INK_DIM))
    return chip

## The Priority chip as a BUTTON (the order line's): the same chip face, lit with the signal border on hover,
## pressing it opens the order's sheet where the Priority picker is.
static func priority_button(level: String, tip: String, on_press: Callable) -> Button:
    var chip := Button.new()
    chip.text = HudOrdersVocab.PRIORITY_CHIP_FORMAT % HudOrdersVocab.priority_face(level)
    chip.focus_mode = Control.FOCUS_NONE
    chip.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
    chip.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
    chip.tooltip_text = tip
    chip.add_theme_font_size_override("font_size", HudOrdersVocab.SMALL_FONT_SIZE)
    chip.add_theme_stylebox_override("normal", HudStyle.chip_stylebox(HudStyle.LINE))
    chip.add_theme_stylebox_override("hover", HudStyle.chip_stylebox(HudStyle.SIGNAL))
    chip.add_theme_stylebox_override("pressed", HudStyle.chip_stylebox(HudStyle.SIGNAL))
    chip.add_theme_color_override("font_color", HudStyle.INK_DIM)
    chip.add_theme_color_override("font_hover_color", HudStyle.SIGNAL)
    chip.add_theme_color_override("font_pressed_color", HudStyle.SIGNAL)
    chip.set_meta(HudOrdersVocab.ORDER_PRIORITY_META, HudWorkVocab.work_priority_of(level))
    chip.pressed.connect(func() -> void: on_press.call())
    return chip

## The three-way crew bar: keeps up / refills / short, a tooltip carrying the numbers.
static func crew_bar(order: Dictionary) -> Control:
    var keeps := int(order.get("hold_heads", 0))
    var refills := int(order.get("refill_heads", 0))
    var short := HudOrdersVocab.hold_short(order) + HudOrdersVocab.refill_short(order)
    var tip := HudOrdersVocab.CREW_TIP_FORMAT % [keeps, refills]
    if short > 0:
        tip += HudOrdersVocab.CREW_TIP_SHORT_FORMAT % short
    var short_color := HudStyle.WARN
    short_color.a = HudOrdersVocab.CREW_SHORT_ALPHA
    var segments: Array = []
    if keeps > 0:
        segments.append({"count": keeps, "color": HudStyle.HEALTHY, "tooltip": tip})
    if refills > 0:
        segments.append({"count": refills, "color": HudStyle.CREW_BUILD_LINE, "tooltip": tip})
    if short > 0:
        segments.append({"count": short, "color": short_color, "tooltip": tip})
    if segments.is_empty():
        segments.append({"count": 1, "color": HudStyle.LINE_SOFT, "tooltip": tip})
    var bar := HudWidgets.build_composition_bar(segments)
    bar.custom_minimum_size.x = HudOrdersVocab.CREW_BAR_WIDTH
    bar.set_meta(HudOrdersVocab.ORDER_CREW_BAR_META, tip)
    bar.tooltip_text = tip
    return bar

# ---- The group -------------------------------------------------------------------------------------

## `rows_by_kind`: order kind -> Array of `{ROW_NODE: Control, ROW_STAFFED: bool}` (today's rows under that
## order). `also_by_kind`: order kind -> Array of `{text, under}` for the dashed `↳ Also` lines.
## `kit_by_kind`: order kind -> Array of `{name, short}` for the chief rows under it (`HudOrdersVocab.row_kit_short`).
## Every order is FOLDED until the player opens it (`BandOrdersController.is_open`).
const ROW_NODE := "node"
const ROW_STAFFED := "staffed"

static func build_group(band: Dictionary, controller: BandOrdersController,
        rows_by_kind: Dictionary, also_by_kind: Dictionary, kit_by_kind: Dictionary = {}) -> VBoxContainer:
    var orders := HudOrdersVocab.shown_orders(band)
    var group := VBoxContainer.new()
    group.set_meta(HudOrdersVocab.ORDER_GROUP_META, true)
    group.add_theme_constant_override("separation", HudOrdersVocab.GROUP_SEPARATION)
    group.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var box := PanelContainer.new()
    box.add_theme_stylebox_override("panel", HudStyle.readout_stylebox())
    box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var column := VBoxContainer.new()
    column.add_theme_constant_override("separation", HudOrdersVocab.GROUP_SEPARATION)
    box.add_child(column)
    group.add_child(box)
    column.add_child(HudWidgets.alloc_section_label(HudOrdersVocab.group_title(band)))
    for order_variant in orders:
        var order: Dictionary = order_variant
        var kind := String(order.get("kind", ""))
        var is_open := controller.is_open(band, kind)
        column.add_child(_order_line(band, order, controller, is_open,
            HudOrdersVocab.kit_clause(kit_by_kind.get(kind, []))))
        if not is_open:
            continue
        var staffed: Array = []
        var reserve: Array = []
        for entry in rows_by_kind.get(kind, []):
            (staffed if bool((entry as Dictionary)[ROW_STAFFED]) else reserve).append(
                (entry as Dictionary)[ROW_NODE])
        for node in staffed:
            column.add_child(node as Control)
        if not reserve.is_empty():
            var reserve_open := controller.is_open(band, kind, BandOrdersController.PART_RESERVE)
            column.add_child(_reserve_line(band, kind, reserve.size(), reserve_open, controller))
            if reserve_open:
                for node in reserve:
                    column.add_child(node as Control)
        for also in also_by_kind.get(kind, []):
            column.add_child(_also_line(also as Dictionary))
    var summary := HudOrdersVocab.summary_of(band, orders)
    column.add_child(HudWidgets.build_dashed_rule())
    var summary_label := text(String(summary["line"]), HudOrdersVocab.LINE_FONT_SIZE,
        HudStyle.INK_DIM, true)
    summary_label.set_meta(HudOrdersVocab.ORDER_SUMMARY_META, String(summary["line"]))
    column.add_child(summary_label)
    if String(summary["warning"]) != "":
        column.add_child(text(String(summary["warning"]), HudOrdersVocab.LINE_FONT_SIZE,
            HudOrdersVocab.tone_color(summary["tone"]), true))
    column.add_child(_legend())
    return group

## A drawn disclosure triangle on a flat button, the Work sections' chevron idiom.
static func _caret(is_open: bool, tip: String, on_press: Callable) -> Button:
    var caret := Button.new()
    caret.focus_mode = Control.FOCUS_NONE
    caret.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
    caret.custom_minimum_size = Vector2(HudOrdersVocab.CARET_WIDTH, HudOrdersVocab.CARET_HEIGHT)
    caret.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
    caret.tooltip_text = tip
    HudStyle.apply_button(caret, "ghost")
    var triangle := DisclosureTriangle.new()
    triangle.side = HudOrdersVocab.CARET_TRIANGLE_SIDE
    triangle.expanded = is_open
    triangle.color = HudStyle.INK_DIM
    triangle.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
    caret.add_child(triangle)
    caret.set_meta(HudOrdersVocab.ORDER_CARET_META, is_open)
    caret.pressed.connect(func() -> void: on_press.call())
    return caret

## `+N sites in reserve`: the unstaffed chief rows, folded behind one dim toggle line.
static func _reserve_line(band: Dictionary, kind: String, count: int, is_open: bool,
        controller: BandOrdersController) -> Control:
    var line := HBoxContainer.new()
    line.add_theme_constant_override("separation", HudOrdersVocab.GROUP_SEPARATION)
    line.set_meta(HudOrdersVocab.ORDER_RESERVE_META, kind)
    var toggle := func() -> void:
        controller.toggle_fold(band, kind, BandOrdersController.PART_RESERVE)
    var indent := Control.new()
    indent.custom_minimum_size.x = HudOrdersVocab.RESERVE_INDENT
    line.add_child(indent)
    line.add_child(_caret(is_open,
        HudOrdersVocab.RESERVE_OPEN_TIP if is_open else HudOrdersVocab.RESERVE_CLOSED_TIP, toggle))
    var words := (HudOrdersVocab.RESERVE_ONE_FORMAT if count == 1
        else HudOrdersVocab.RESERVE_MANY_FORMAT) % count
    var link := HudWidgets.build_inline_link(words, HudStyle.INK_FAINT, toggle)
    link.add_theme_font_size_override("font_size", HudOrdersVocab.STATE_FONT_SIZE)
    line.add_child(link)
    return line

static func _order_line(band: Dictionary, order: Dictionary, controller: BandOrdersController,
        is_open: bool, kit: Dictionary = {}) -> Control:
    var kind := String(order.get("kind", ""))
    var state := HudOrdersVocab.state_of(order)
    var line := HBoxContainer.new()
    line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    line.add_theme_constant_override("separation", HudOrdersVocab.GROUP_SEPARATION)
    line.set_meta(HudOrdersVocab.ORDER_LINE_META, kind)
    line.add_child(_caret(is_open,
        HudOrdersVocab.CARET_OPEN_TIP if is_open else HudOrdersVocab.CARET_CLOSED_TIP,
        func() -> void: controller.toggle_fold(band, kind)))
    var icon := text(HudOrdersVocab.order_icon(kind), HudOrdersVocab.NAME_FONT_SIZE, HudStyle.INK)
    icon.custom_minimum_size.x = HudOrdersVocab.ORDER_ICON_WIDTH
    icon.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
    line.add_child(icon)
    var middle := VBoxContainer.new()
    middle.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    middle.add_theme_constant_override("separation", 0)
    # A flow, not a row: the name and its `· 12 turns · keep 20 turns` detail sit on one line while they fit
    # and the detail drops under the name when they do not, whole -- never elided, never wider than the zone.
    var head := HFlowContainer.new()
    head.add_theme_constant_override("h_separation", HudOrdersVocab.GROUP_SEPARATION)
    head.add_theme_constant_override("v_separation", 0)
    head.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var open_sheet := func() -> void: controller.open_sheet(band, kind)
    var open_tip := HudOrdersVocab.open_tip(band, kind)
    var name_link := HudWidgets.build_inline_link(HudOrdersVocab.order_name(kind), HudStyle.INK, open_sheet)
    name_link.tooltip_text = open_tip
    name_link.add_theme_font_size_override("font_size", HudOrdersVocab.NAME_FONT_SIZE)
    name_link.set_meta(HudOrdersVocab.ORDER_NAME_META, kind)
    head.add_child(name_link)
    var detail := HudWidgets.build_inline_link(HudOrdersVocab.line_text(order), HudStyle.INK_DIM, open_sheet)
    detail.add_theme_font_size_override("font_size", HudOrdersVocab.SMALL_FONT_SIZE + 1)
    detail.alignment = HORIZONTAL_ALIGNMENT_LEFT
    detail.tooltip_text = open_tip
    detail.set_meta(HudOrdersVocab.ORDER_DETAIL_META, kind)
    head.add_child(detail)
    middle.add_child(head)
    var state_label := text(String(state["text"]), HudOrdersVocab.STATE_FONT_SIZE,
        HudOrdersVocab.tone_color(state["tone"]), true)
    state_label.set_meta(HudOrdersVocab.ORDER_STATE_META, String(state["text"]))
    if kit.is_empty():
        middle.add_child(state_label)
    else:
        # The state, then its amber kit clause under it. BOTH wrap in the column's own width: a non-wrapping
        # state label (the old flow) asked for its whole sentence as a minimum width and pushed the Work
        # tab wider than the dock whenever the kit clause was up.
        var stack := VBoxContainer.new()
        stack.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        stack.add_theme_constant_override("separation", 0)
        stack.add_child(state_label)
        var clause := HudWidgets.build_inline_link(String(kit["clause"]), HudStyle.WARN,
            func() -> void: controller.open_bench(band))
        clause.add_theme_font_size_override("font_size", HudOrdersVocab.STATE_FONT_SIZE)
        clause.tooltip_text = String(kit["tip"])
        clause.set_meta(HudOrdersVocab.ORDER_KIT_META, String(kit["clause"]))
        clause.alignment = HORIZONTAL_ALIGNMENT_LEFT
        clause.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
        clause.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        stack.add_child(clause)
        middle.add_child(stack)
    middle.add_child(priority_button(String(order.get("priority", HudOrdersVocab.PRIORITY_NORMAL)),
        open_tip, open_sheet))
    line.add_child(middle)
    var crew := VBoxContainer.new()
    crew.add_theme_constant_override("separation", HudOrdersVocab.GROUP_SEPARATION)
    var number := text(str(HudOrdersVocab.crew_of(order)), HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK)
    number.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    crew.add_child(number)
    crew.add_child(crew_bar(order))
    line.add_child(crew)
    var open_link := HudWidgets.build_inline_link(HudOrdersVocab.OPEN_CHEVRON, HudStyle.INK_DIM, open_sheet)
    open_link.add_theme_font_size_override("font_size", HudOrdersVocab.OPEN_CHEVRON_FONT_SIZE)
    open_link.size_flags_vertical = Control.SIZE_SHRINK_CENTER
    open_link.tooltip_text = open_tip
    open_link.set_meta(HudOrdersVocab.ORDER_OPEN_META, kind)
    line.add_child(open_link)
    return line

static func _also_line(also: Dictionary) -> Control:
    var panel := PanelContainer.new()
    panel.add_theme_stylebox_override("panel", HudStyle.chip_stylebox(HudStyle.LINE_SOFT))
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudOrdersVocab.GROUP_PADDING)
    # Wraps rather than elides: the whole `↳ Also +0.36 food from Harvest (31, 24)` is the point of the line.
    var main := text(String(also.get("text", "")), HudOrdersVocab.ALSO_FONT_SIZE, HudStyle.INK_DIM, true)
    main.tooltip_text = main.text
    row.add_child(main)
    row.add_child(text(String(also.get("under", "")), HudOrdersVocab.SMALL_FONT_SIZE, HudStyle.INK_FAINT))
    panel.add_child(row)
    panel.set_meta(HudOrdersVocab.ORDER_ALSO_META, String(also.get("text", "")))
    return panel

static func _legend() -> Control:
    var key := HBoxContainer.new()
    key.add_theme_constant_override("separation", HudOrdersVocab.SECTION_SEPARATION * 2)
    var short_color := HudStyle.WARN
    short_color.a = HudOrdersVocab.CREW_SHORT_ALPHA
    for entry in [[HudStyle.HEALTHY, HudOrdersVocab.CREW_KEEPS],
            [HudStyle.CREW_BUILD_LINE, HudOrdersVocab.CREW_REFILLS],
            [short_color, HudOrdersVocab.CREW_SHORT]]:
        var item := HBoxContainer.new()
        item.add_theme_constant_override("separation", HudOrdersVocab.GROUP_SEPARATION)
        var swatch := ColorRect.new()
        swatch.color = entry[0]
        swatch.custom_minimum_size = Vector2(HudOrdersVocab.KEY_SWATCH, HudOrdersVocab.KEY_SWATCH)
        swatch.size_flags_vertical = Control.SIZE_SHRINK_CENTER
        item.add_child(swatch)
        item.add_child(text(String(entry[1]), HudOrdersVocab.SMALL_FONT_SIZE, HudStyle.INK_FAINT))
        key.add_child(item)
    return key

## `Hide · 14 on hand` / `+0.4 a turn from hunting`: the read-only by-product lines.
static func by_product_lines(band: Dictionary) -> Array:
    var nodes: Array = []
    for entry in HudOrdersVocab.by_product_rows(band):
        var row := HBoxContainer.new()
        row.add_theme_constant_override("separation", HudOrdersVocab.GROUP_PADDING)
        row.set_meta(HudOrdersVocab.BY_PRODUCT_META, String(entry["material"]))
        var icon := text(String(entry["icon"]), HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK)
        icon.custom_minimum_size.x = HudOrdersVocab.ORDER_ICON_WIDTH
        row.add_child(icon)
        var left := text(String(entry["left"]), HudOrdersVocab.LINE_FONT_SIZE - 1, HudStyle.INK_DIM, true)
        row.add_child(left)
        row.add_child(text(String(entry["right"]), HudOrdersVocab.LINE_FONT_SIZE - 1, HudStyle.INK_DIM))
        nodes.append(row)
    return nodes
