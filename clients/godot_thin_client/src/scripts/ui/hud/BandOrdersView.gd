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

## `rows_by_kind`: order kind -> Array[Control] (today's rows under that order). `also_by_kind`: order
## kind -> Array of `{text, under}` for the dashed `↳ Also` lines.
static func build_group(band: Dictionary, controller: BandOrdersController,
        rows_by_kind: Dictionary, also_by_kind: Dictionary) -> VBoxContainer:
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
        column.add_child(_order_line(band, order, controller))
        for node in rows_by_kind.get(kind, []):
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

static func _order_line(band: Dictionary, order: Dictionary, controller: BandOrdersController) -> Control:
    var kind := String(order.get("kind", ""))
    var state := HudOrdersVocab.state_of(order)
    var line := HBoxContainer.new()
    line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    line.add_theme_constant_override("separation", HudOrdersVocab.GROUP_SEPARATION)
    line.set_meta(HudOrdersVocab.ORDER_LINE_META, kind)
    var icon := text(HudOrdersVocab.order_icon(kind), HudOrdersVocab.NAME_FONT_SIZE, HudStyle.INK)
    icon.custom_minimum_size.x = HudOrdersVocab.ORDER_ICON_WIDTH
    icon.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
    line.add_child(icon)
    var middle := VBoxContainer.new()
    middle.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    middle.add_theme_constant_override("separation", 0)
    var head := HBoxContainer.new()
    head.add_theme_constant_override("separation", HudOrdersVocab.GROUP_SEPARATION)
    var name_link := HudWidgets.build_inline_link(HudOrdersVocab.order_name(kind), HudStyle.INK,
        func() -> void: controller.open_sheet(band, kind))
    name_link.add_theme_font_size_override("font_size", HudOrdersVocab.NAME_FONT_SIZE)
    name_link.set_meta(HudOrdersVocab.ORDER_NAME_META, kind)
    head.add_child(name_link)
    var detail := text(HudOrdersVocab.line_text(order), HudOrdersVocab.SMALL_FONT_SIZE + 1,
        HudStyle.INK_DIM)
    detail.clip_text = true
    detail.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    detail.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(detail)
    middle.add_child(head)
    var state_label := text(String(state["text"]), HudOrdersVocab.STATE_FONT_SIZE,
        HudOrdersVocab.tone_color(state["tone"]), true)
    state_label.set_meta(HudOrdersVocab.ORDER_STATE_META, String(state["text"]))
    middle.add_child(state_label)
    middle.add_child(priority_chip(String(order.get("priority", HudOrdersVocab.PRIORITY_NORMAL))))
    line.add_child(middle)
    var crew := VBoxContainer.new()
    crew.add_theme_constant_override("separation", HudOrdersVocab.GROUP_SEPARATION)
    var number := text(str(HudOrdersVocab.crew_of(order)), HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK)
    number.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    crew.add_child(number)
    crew.add_child(crew_bar(order))
    line.add_child(crew)
    return line

static func _also_line(also: Dictionary) -> Control:
    var panel := PanelContainer.new()
    panel.add_theme_stylebox_override("panel", HudStyle.chip_stylebox(HudStyle.LINE_SOFT))
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudOrdersVocab.GROUP_PADDING)
    var main := text(String(also.get("text", "")), HudOrdersVocab.ALSO_FONT_SIZE, HudStyle.INK_DIM)
    main.clip_text = true
    main.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    main.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    main.tooltip_text = main.text
    main.mouse_filter = Control.MOUSE_FILTER_PASS
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
        var left := text(String(entry["left"]), HudOrdersVocab.LINE_FONT_SIZE - 1, HudStyle.INK_DIM)
        left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        row.add_child(left)
        row.add_child(text(String(entry["right"]), HudOrdersVocab.LINE_FONT_SIZE - 1, HudStyle.INK_DIM))
        nodes.append(row)
    return nodes
