class_name BandOrdersSheets

## **THE CHIEF'S SECTIONS OF THE COMPOSE SHEETS** (#860) -- the Chief's-order box, the Keep-on-hand
## stepper, the crew / sources boxes, Next turn, and the two strips the hunt / gather / material sheets
## wear. All-`static`; `DrawerComposeController` owns the sheet and hands these a `VBoxContainer` target.
##
## `ctx` (a Dictionary the caller builds) carries: `band`, `orders` (`BandOrdersController`),
## `rebuild` (Callable re-running the sheet), `close` (Callable), `show_on_map` (Callable(x, y)).

const META_CHIEF_BOX := &"band_order_chief_box"
const META_KEEP_BOX := &"band_order_keep_box"
const META_STEPPER_LESS := &"band_order_keep_less"
const META_STEPPER_MORE := &"band_order_keep_more"
const META_STEPPER_VALUE := &"band_order_keep_value"
const META_CREW_BOX := &"band_order_crew_box"
const META_NEXT_BOX := &"band_order_next_box"
const META_RELEASE := &"band_order_release"
const META_CLAIM := &"band_order_claim"
const META_OFFER := &"band_order_offer"
const META_ROW_STRIP := &"band_order_row_strip"
const META_ORDER_LINK := &"band_order_link"
const META_PEN_BAR := &"band_order_pen_bar"
const META_SOURCE_ROW := &"band_order_source_row"

# ---- Pieces ---------------------------------------------------------------------------------------

static func _portrait(band: Dictionary, size: float) -> Control:
    var portrait := LeaderPortrait.make(int(band.get("faction", HudConst.PLAYER_FACTION_ID)), false,
        HudOrdersVocab.is_council(band))
    if is_equal_approx(size, LeaderPortrait.PORTRAIT_SIZE):
        return portrait
    # A scaled Control still reserves its unscaled minimum, so the portrait rides inside a holder
    # of the drawn size and is scaled down within it.
    var holder := Control.new()
    holder.custom_minimum_size = Vector2(size, size)
    holder.size_flags_vertical = Control.SIZE_SHRINK_CENTER
    holder.mouse_filter = Control.MOUSE_FILTER_IGNORE
    portrait.custom_minimum_size = Vector2.ZERO
    portrait.scale = Vector2.ONE * (size / LeaderPortrait.PORTRAIT_SIZE)
    holder.add_child(portrait)
    return holder

static func _box(parent: Container, meta: StringName) -> VBoxContainer:
    var column := HudWidgets.build_readout_box(parent)
    column.get_parent().set_meta(meta, true)
    return column

static func _ghost_button(label: String, tip: String, on_press: Callable) -> Button:
    var button := Button.new()
    button.text = label
    button.tooltip_text = tip
    HudStyle.apply_button(button, "ghost")
    button.pressed.connect(func() -> void: on_press.call())
    return button

## The CHIEF'S ORDER box: portrait, the tag, and `<Leader> keeps stone on hand`.
static func chief_box(band: Dictionary, headline: String) -> Control:
    var box := PanelContainer.new()
    box.add_theme_stylebox_override("panel", HudStyle.readout_stylebox())
    box.set_meta(META_CHIEF_BOX, true)
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudOrdersVocab.SRC_ROW_SEPARATION)
    row.add_child(_portrait(band, LeaderPortrait.PORTRAIT_SIZE))
    var words := VBoxContainer.new()
    words.size_flags_vertical = Control.SIZE_SHRINK_CENTER
    words.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    words.add_child(BandOrdersView.text(HudOrdersVocab.CHIEF_BOX_TAG.to_upper(),
        HudOrdersVocab.SMALL_FONT_SIZE, HudStyle.WARN))
    words.add_child(BandOrdersView.text(headline, HudOrdersVocab.NAME_FONT_SIZE + 1, HudStyle.INK, true))
    row.add_child(words)
    box.add_child(row)
    return box

## A strip with the small portrait and one dim sentence, plus a ghost button (`Hand it to Orrin`).
static func offer_strip(band: Dictionary, sentence: String, button: Button) -> Control:
    var strip := PanelContainer.new()
    strip.add_theme_stylebox_override("panel", HudStyle.chip_stylebox(HudStyle.LINE))
    strip.set_meta(META_OFFER, true)
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudOrdersVocab.SRC_ROW_SEPARATION)
    row.add_child(_portrait(band, HudOrdersVocab.PORTRAIT_SMALL))
    var words := BandOrdersView.text(sentence, HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK_DIM, true)
    words.size_flags_vertical = Control.SIZE_SHRINK_CENTER
    row.add_child(words)
    row.add_child(button)
    strip.add_child(row)
    return strip

## `<Leader> could keep stone on hand for you. [Hand it to Orrin]` -- on a material sheet the player runs.
static func material_offer(ctx: Dictionary, material: String) -> Control:
    var band: Dictionary = ctx["band"]
    var orders: BandOrdersController = ctx["orders"]
    var button := _ghost_button(HudOrdersVocab.HAND_IT_FORMAT % HudOrdersVocab.who_lower(band), "",
        func() -> void:
            orders.send_claim(band, material)
            (ctx["rebuild"] as Callable).call())
    button.set_meta(META_CLAIM, material)
    return offer_strip(band, HudOrdersVocab.OFFER_MATERIAL_FORMAT % [HudOrdersVocab.who(band), material],
        button)

## The strip over a hunt / gather sheet. A chief's row: `<Leader> hunts this for the food order.` with
## `Crew it yourself`; a player's: `<Leader> could hunt this for the food order.` with `Hand it to`.
static func row_strip(ctx: Dictionary, order_info: Dictionary, source_kind: String, address: Dictionary) -> Control:
    var band: Dictionary = ctx["band"]
    var orders: BandOrdersController = ctx["orders"]
    var kind := String(order_info["kind"])
    var verbs: Dictionary = HudOrdersVocab.ROW_VERBS.get(kind, {})
    var verb := String(verbs.get("hunt" if source_kind == SourceForecast.LABOR_KIND_HUNT else "forage",
        "works"))
    var chiefs := bool(order_info["chief_owned"])
    var strip: Control
    if chiefs:
        var button := _ghost_button(HudOrdersVocab.CREW_IT_YOURSELF, "", func() -> void:
            orders.send_release(band, kind, address)
            (ctx["rebuild"] as Callable).call())
        button.set_meta(META_RELEASE, kind)
        strip = offer_strip(band, "", button)
        var row: HBoxContainer = strip.get_child(0)
        var words: Label = row.get_child(1)
        row.remove_child(words)
        words.queue_free()
        var rich := RichTextLabel.new()
        rich.bbcode_enabled = true
        rich.fit_content = true
        rich.scroll_active = false
        rich.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
        rich.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        rich.size_flags_vertical = Control.SIZE_SHRINK_CENTER
        rich.add_theme_font_size_override("normal_font_size", HudOrdersVocab.LINE_FONT_SIZE)
        rich.add_theme_color_override("default_color", HudStyle.INK_DIM)
        rich.text = "%s[url=order][color=#%s]%s[/color][/url]%s" % [
            HudOrdersVocab.STRIP_ROW_FORMAT % HudOrdersVocab.who_verb(band, verb),
            HudStyle.INK_HEX, HudOrdersVocab.STRIP_ROW_LINK_FORMAT % kind, HudOrdersVocab.STRIP_ROW_END]
        rich.meta_clicked.connect(func(_meta: Variant) -> void: orders.open_sheet(band, kind))
        rich.set_meta(META_ORDER_LINK, kind)
        row.add_child(rich)
        row.move_child(rich, 1)
    else:
        var offer := _ghost_button(HudOrdersVocab.HAND_IT_FORMAT % HudOrdersVocab.who_lower(band), "",
            func() -> void:
                orders.send_claim(band, kind, address)
                (ctx["rebuild"] as Callable).call())
        offer.set_meta(META_CLAIM, kind)
        strip = offer_strip(band, HudOrdersVocab.OFFER_ROW_FORMAT % [HudOrdersVocab.who(band),
            String(HudOrdersVocab.ROW_VERBS.get(kind, {}).get(
                "hunt" if source_kind == SourceForecast.LABOR_KIND_HUNT else "forage", "work")).trim_suffix("s"),
            kind], offer)
    strip.set_meta(META_ROW_STRIP, kind)
    return strip

## Disable the live controls under `root` (the chief's values stay shown), leaving the buttons whose
## meta is in `keep` and anything inside a node carrying a `keep` meta live.
## Hide every control under `node` that wears `meta` (the chief's row has no `Stop working here`).
static func hide_meta(node: Node, meta: StringName) -> void:
    if node.is_queued_for_deletion():
        return
    if node is Control and node.has_meta(meta):
        (node as Control).visible = false
    for child in node.get_children():
        hide_meta(child, meta)

static func disable_node(node: Node, keep: Array, tip: String) -> void:
    if node.is_queued_for_deletion():
        return
    for meta in keep:
        if node.has_meta(meta):
            return
    if node is BaseButton:
        (node as BaseButton).disabled = true
        (node as Control).tooltip_text = tip
    elif node is Range:
        (node as Range).editable = false
    elif node is Control and node.has_meta(HudWidgets.FLOOR_CHART_META):
        (node as Control).mouse_filter = Control.MOUSE_FILTER_IGNORE
        (node as Control).focus_mode = Control.FOCUS_NONE
    for child in node.get_children():
        disable_node(child, keep, tip)

## The `Keep on hand` box: `12.4 on hand · keep [− 30 +]`. `on_set` receives the new target.
static func keep_box(parent: Container, order: Dictionary, noun: String, on_set: Callable) -> void:
    parent.add_child(HudWidgets.alloc_section_label(HudOrdersVocab.KEEP_ON_HAND_LABEL))
    var box := _box(parent, META_KEEP_BOX)
    var kind := String(order.get("kind", ""))
    var turns_kind := HudOrdersVocab.is_turns_kind(kind)
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudOrdersVocab.SRC_ROW_SEPARATION)
    row.add_child(BandOrdersView.text(HudOrdersVocab.have_text(order) if not turns_kind \
        else HudOrdersVocab.have_text(order), HudOrdersVocab.NAME_FONT_SIZE + 2, HudStyle.INK))
    var gap := Control.new()
    gap.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    gap.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.add_child(gap)
    row.add_child(BandOrdersView.text(HudOrdersVocab.KEEP_WORD, HudOrdersVocab.LINE_FONT_SIZE,
        HudStyle.INK_DIM))
    var target := int(order.get("target", 0))
    var less := Button.new()
    less.text = HudWorkVocab.STEPPER_MINUS_FACE
    less.tooltip_text = (HudOrdersVocab.KEEP_LESS_FOOD_TIP if turns_kind else HudOrdersVocab.KEEP_LESS_TIP_FORMAT) % noun
    HudStyle.apply_button(less, "ghost")
    less.set_meta(META_STEPPER_LESS, kind)
    less.disabled = target <= HudOrdersVocab.KEEP_MIN
    less.pressed.connect(func() -> void: on_set.call(maxi(target - HudOrdersVocab.KEEP_STEP, HudOrdersVocab.KEEP_MIN)))
    row.add_child(less)
    var value := BandOrdersView.text(HudOrdersVocab.STEPPER_TURNS_FORMAT % target if turns_kind else str(target),
        HudOrdersVocab.NAME_FONT_SIZE + 2, HudStyle.INK)
    value.custom_minimum_size.x = HudWorkVocab.WORKER_STEPPER_VALUE_WIDTH * 1.5
    value.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    value.set_meta(META_STEPPER_VALUE, target)
    row.add_child(value)
    var more := Button.new()
    more.text = HudWorkVocab.STEPPER_PLUS_FACE
    more.tooltip_text = (HudOrdersVocab.KEEP_MORE_FOOD_TIP if turns_kind else HudOrdersVocab.KEEP_MORE_TIP_FORMAT) % noun
    HudStyle.apply_button(more, "ghost")
    more.set_meta(META_STEPPER_MORE, kind)
    more.pressed.connect(func() -> void: on_set.call(target + HudOrdersVocab.KEEP_STEP))
    row.add_child(more)
    box.add_child(row)

static func priority_block(parent: Container, order: Dictionary, on_pick: Callable) -> void:
    parent.add_child(HudWidgets.alloc_section_label(HudOrdersVocab.PRIORITY_LABEL))
    parent.add_child(HudWidgets.build_work_priority_picker(on_pick,
        String(order.get("priority", HudOrdersVocab.PRIORITY_NORMAL)), false))

## `Next turn`: the order's take with its unit, then one or two dim lines.
static func next_turn_box(parent: Container, take: float, unit: String, lines: Array,
        right_note: String = "") -> void:
    var box := _box(parent, META_NEXT_BOX)
    box.add_child(HudWidgets.alloc_section_label(HudOrdersVocab.NEXT_TURN_LABEL))
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudOrdersVocab.SRC_ROW_SEPARATION)
    row.add_child(BandOrdersView.text("%.2f" % take, HudOrdersVocab.NAME_FONT_SIZE + 2, HudStyle.INK))
    row.add_child(BandOrdersView.text(unit.to_upper(), HudOrdersVocab.SMALL_FONT_SIZE, HudStyle.INK_FAINT))
    if right_note != "":
        var gap := Control.new()
        gap.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        row.add_child(gap)
        row.add_child(BandOrdersView.text(right_note, HudOrdersVocab.SMALL_FONT_SIZE, HudStyle.INK_DIM))
    box.add_child(row)
    for line in lines:
        if String(line) != "":
            box.add_child(BandOrdersView.text(String(line), HudOrdersVocab.SMALL_FONT_SIZE + 1,
                HudStyle.INK_FAINT, true))

static func footer(ctx: Dictionary, kind: String, show_map: Callable, with_release: bool) -> Control:
    var band: Dictionary = ctx["band"]
    var orders: BandOrdersController = ctx["orders"]
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudOrdersVocab.SRC_ROW_SEPARATION)
    if show_map.is_valid():
        var link := HudWidgets.build_inline_link(HudOrdersVocab.SHOW_ON_MAP, HudStyle.INK, show_map)
        link.set_meta(HudComposeVocab.SHOW_ON_MAP_META, true)
        row.add_child(link)
    var gap := Control.new()
    gap.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    gap.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.add_child(gap)
    if with_release:
        var release := _ghost_button(HudOrdersVocab.CREW_IT_YOURSELF, "", func() -> void:
            orders.send_release(band, kind)
            (ctx["rebuild"] as Callable).call())
        release.set_meta(META_RELEASE, kind)
        row.add_child(release)
    var done := Button.new()
    done.text = HudOrdersVocab.DONE
    done.set_meta(HudComposeVocab.DONE_BUTTON_META, true)
    HudStyle.apply_button(done, "primary")
    done.pressed.connect(func() -> void: (ctx["close"] as Callable).call())
    row.add_child(done)
    return row

# ---- The material sheets (stone, wood) ------------------------------------------------------------

## The chief's rows of a material: `labor_assignments` extract rows of `material` he owns.
static func material_rows(band: Dictionary, material: String) -> Array:
    var rows: Array = []
    for row in band.get("labor_assignments", []):
        if row is Dictionary and String((row as Dictionary).get("kind", "")) == HudConst.LABOR_KIND_EXTRACT \
                and String((row as Dictionary).get("material", "")) == material \
                and HudOrdersVocab.row_is_chiefs(row as Dictionary):
            rows.append(row)
    return rows

## The chief version of a stone / wood sheet (boards 2 and 3). `stock_line` is the site's own stock
## sentence; `tile` is the sheet's hex.
static func build_material_chief(target: VBoxContainer, ctx: Dictionary, order: Dictionary,
        material: String, tile: Vector2i, stock_line: String) -> void:
    var band: Dictionary = ctx["band"]
    var orders: BandOrdersController = ctx["orders"]
    var who := HudOrdersVocab.who(band)
    target.add_child(chief_box(band, HudOrdersVocab.CHIEF_KEEPS_MATERIAL_FORMAT % [
        HudOrdersVocab.who_verb(band, "keeps"), material]))
    if stock_line != "":
        target.add_child(HudWidgets.alloc_hint_label(stock_line))
    var priority := String(order.get("priority", HudOrdersVocab.PRIORITY_NORMAL))
    var current_target := int(order.get("target", 0))
    keep_box(target, order, material, func(new_target: int) -> void:
        orders.send_set(band, material, new_target, priority))
    priority_block(target, order, func(level: String) -> void:
        orders.send_set(band, material, current_target, level))
    var crew_noun := String(HudOrdersVocab.CREW_NOUNS.get(material, "workers"))
    target.add_child(HudWidgets.alloc_section_label(crew_noun))
    var box := _box(target, META_CREW_BOX)
    var rows := material_rows(band, material)
    var crew := HudOrdersVocab.crew_of(order)
    var at_target := int(order.get("refill_wanted", 0)) == 0 and crew == 0
    var verb := String(HudOrdersVocab.NOBODY_VERBS.get(material, "works"))
    var headline := HudOrdersVocab.AT_TARGET_NOBODY_FORMAT % verb if at_target \
        else HudOrdersVocab.SENDS_FORMAT % [HudOrdersVocab.who_verb(band, "sends"), crew, crew_noun]
    box.add_child(BandOrdersView.text(headline, HudOrdersVocab.NAME_FONT_SIZE + 2, HudStyle.INK, true))
    var hold := int(order.get("hold_heads", 0))
    var refill := int(order.get("refill_heads", 0))
    if material == "wood":
        for row in rows:
            var line := HBoxContainer.new()
            line.add_theme_constant_override("separation", HudOrdersVocab.SRC_ROW_SEPARATION)
            var row_tile := Vector2i(int(row.get("x", -1)), int(row.get("y", -1)))
            var here := row_tile == tile
            var name_text := HudOrdersVocab.SITE_NAME_FORMAT % [material.capitalize(), row_tile.x, row_tile.y]
            if here:
                name_text += " · " + HudOrdersVocab.STAND_HERE
            var name_label := BandOrdersView.text(name_text, HudOrdersVocab.LINE_FONT_SIZE,
                HudStyle.INK if here else HudStyle.INK_DIM)
            name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
            line.add_child(name_label)
            line.add_child(BandOrdersView.text(str(int(row.get("workers", 0))),
                HudOrdersVocab.SMALL_FONT_SIZE + 1, HudStyle.INK_FAINT))
            line.add_child(BandOrdersView.text(HudOrdersVocab.SRC_TAKE_FORMAT % HudOrdersVocab.product_of(row, material),
                HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK_DIM))
            line.set_meta(META_SOURCE_ROW, row_tile)
            box.add_child(line)
    elif not at_target:
        var site := Vector2i(int(order.get("site_x", -1)), int(order.get("site_y", -1)))
        if bool(order.get("has_site", false)) and site != tile:
            box.add_child(BandOrdersView.text(HudOrdersVocab.DIGGING_ELSEWHERE_FORMAT % [who,
                orders.site_name(site.x, site.y)], HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK_DIM, true))
        else:
            var refill_text := (HudOrdersVocab.REFILL_ONE if refill == 1 else HudOrdersVocab.REFILL_MANY) % refill
            box.add_child(BandOrdersView.text(HudOrdersVocab.HOLD_REFILL_FORMAT % [hold, refill_text,
                current_target], HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK_DIM, true))
    if at_target:
        box.add_child(BandOrdersView.text(HudOrdersVocab.AT_TARGET_DIGGERS_FORMAT % current_target
            if material == "stone" else "", HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK_DIM, true))
    var kitted := 0
    var bare := 0
    for row in rows:
        kitted += int(row.get("kitted_staffed", 0))
        bare += int(row.get("bare_staffed", 0))
    if kitted + bare > 0:
        box.add_child(BandOrdersView.text(HudOrdersVocab.SPLIT_FORMAT % [kitted,
            String(HudOrdersVocab.SPLIT_KIT_WORDS.get(material, "kits")), bare],
            HudOrdersVocab.SMALL_FONT_SIZE + 1, HudStyle.INK_FAINT))
    if material == "wood":
        box.add_child(BandOrdersView.text(HudOrdersVocab.LEAVES_STANDS_COUNCIL \
            if HudOrdersVocab.is_council(band) else HudOrdersVocab.LEAVES_STANDS,
            HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK_DIM, true))
        var sustainable := float(order.get("sustainable", 0.0))
        var draw := float(order.get("draw", 0.0))
        if sustainable < draw:
            box.add_child(BandOrdersView.text(HudOrdersVocab.STANDS_SHORT_FORMAT % [sustainable,
                HudFormat.band_name(band), draw], HudOrdersVocab.LINE_FONT_SIZE, HudStyle.WARN, true))
    var lines: Array = []
    var eta := HudOrdersVocab.eta_turns(order)
    if int(order.get("refill_wanted", 0)) == 0:
        lines.append(HudOrdersVocab.NEXT_TURN_NOTHING_FORMAT % ("dug" if material == "stone" else "cut"))
    elif eta > 0:
        lines.append(HudOrdersVocab.NEXT_TURN_BACK_FORMAT % [HudOrdersVocab.target_text(order),
            HudOrdersVocab.turns(eta)])
    elif float(order.get("take", 0.0)) < float(order.get("draw", 0.0)):
        lines.append(HudOrdersVocab.NEXT_TURN_FALLS_FORMAT % [material.capitalize(),
            float(order.get("draw", 0.0)) - float(order.get("take", 0.0))])
    next_turn_box(target, float(order.get("take", 0.0)), material, lines)
    var site_x := int(order.get("site_x", tile.x))
    var site_y := int(order.get("site_y", tile.y))
    target.add_child(footer(ctx, material,
        func() -> void: (ctx["show_on_map"] as Callable).call(site_x, site_y), true))

# ---- The food and fodder sheets -------------------------------------------------------------------

## Every labor row listed under `kind` (the chief's), then the player's rows that bring the kind's
## product in (marked `yours`). Each `{row, yours}`.
static func order_sources(band: Dictionary, kind: String) -> Array:
    var sources: Array = []
    for row_variant in band.get("labor_assignments", []):
        if not (row_variant is Dictionary):
            continue
        var row: Dictionary = row_variant
        var row_kind := String(row.get("kind", ""))
        if row_kind != SourceForecast.LABOR_KIND_FORAGE and row_kind != SourceForecast.LABOR_KIND_HUNT:
            continue
        if HudOrdersVocab.row_is_chiefs(row):
            if HudOrdersVocab.row_order_kind(row) == kind:
                sources.append({"row": row, "yours": false})
        elif HudOrdersVocab.product_of(row, kind) > 0.0:
            sources.append({"row": row, "yours": true})
    return sources

static func source_name(row: Dictionary, herd_label: Callable) -> String:
    if String(row.get("kind", "")) == SourceForecast.LABOR_KIND_HUNT:
        return HudWorkVocab.WORK_ROW_HUNT_FORMAT % String(herd_label.call(String(row.get("herd_id", ""))))
    return HudOrdersVocab.ROW_HARVEST_FORMAT % [int(row.get("x", -1)), int(row.get("y", -1))]

## The Food / Fodder sheet body (boards 5 and 6). `pens` are the tamed and penned herds that draw fodder (fodder only).
static func build_order_sheet(target: VBoxContainer, ctx: Dictionary, order: Dictionary,
        pens: Array, herd_label: Callable) -> void:
    var band: Dictionary = ctx["band"]
    var orders: BandOrdersController = ctx["orders"]
    var kind := String(order.get("kind", ""))
    var is_food := kind == HudOrdersVocab.KIND_FOOD
    var who := HudOrdersVocab.who(band)
    target.add_child(chief_box(band, (HudOrdersVocab.CHIEF_KEEPS_FOOD_FORMAT if is_food \
        else HudOrdersVocab.CHIEF_KEEPS_FODDER_FORMAT) % HudOrdersVocab.who_verb(band, "keeps")))
    var draw := float(order.get("draw", 0.0))
    if is_food:
        target.add_child(HudWidgets.alloc_hint_label(HudOrdersVocab.FOOD_EATS_FORMAT % [
            HudFormat.band_name(band), draw]))
    elif pens.is_empty():
        target.add_child(HudWidgets.alloc_hint_label(HudOrdersVocab.FODDER_EATS_FORMAT % draw))
    else:
        target.add_child(HudWidgets.alloc_section_label(HudOrdersVocab.WHAT_EATS_IT))
        var pen_box := _box(target, META_CREW_BOX)
        for pen_variant in pens:
            var pen: Dictionary = pen_variant
            var hay := float(pen.get("fodder_draw", 0.0)) + float(pen.get("pen_fodder_shortfall", 0.0))
            var pasture := clampf(float(pen.get("pen_pasture_fraction", 0.0)), 0.0, 1.0)
            var head := HBoxContainer.new()
            var pen_name := BandOrdersView.text(HudOrdersVocab.PEN_NAME_FORMAT % [
                SourceForecast.herd_display_name(pen), int(pen.get("x", -1)), int(pen.get("y", -1))],
                HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK)
            pen_name.size_flags_horizontal = Control.SIZE_EXPAND_FILL
            head.add_child(pen_name)
            head.add_child(BandOrdersView.text(HudOrdersVocab.PEN_FODDER_FORMAT % hay,
                HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK_DIM))
            pen_box.add_child(head)
            var tip := HudOrdersVocab.PEN_SPLIT_TIP_FORMAT % int(roundf(pasture * 100.0))
            var bar := HudWidgets.build_composition_bar([
                {"count": pasture * 100.0, "color": HudStyle.HEALTHY, "tooltip": tip},
                {"count": (1.0 - pasture) * 100.0, "color": HudStyle.WARN, "tooltip": tip}])
            bar.custom_minimum_size.y = HudOrdersVocab.PEN_BAR_HEIGHT
            bar.set_meta(META_PEN_BAR, pasture)
            pen_box.add_child(bar)
        pen_box.add_child(BandOrdersView.text(HudOrdersVocab.PEN_SPLIT_KEY,
            HudOrdersVocab.SMALL_FONT_SIZE + 1, HudStyle.INK_FAINT, true))
    var priority := String(order.get("priority", HudOrdersVocab.PRIORITY_NORMAL))
    var current_target := int(order.get("target", 0))
    keep_box(target, order, kind, func(new_target: int) -> void:
        orders.send_set(band, kind, new_target, priority))
    priority_block(target, order, func(level: String) -> void:
        orders.send_set(band, kind, current_target, level))
    target.add_child(HudWidgets.alloc_section_label(
        HudOrdersVocab.CREW_BOX_FOOD if is_food else HudOrdersVocab.CREW_BOX_FODDER))
    var box := _box(target, META_CREW_BOX)
    var crew := HudOrdersVocab.crew_of(order)
    box.add_child(BandOrdersView.text(HudOrdersVocab.SENDS_TO_FORMAT % [
        HudOrdersVocab.who_verb(band, "sends"), crew, kind], HudOrdersVocab.NAME_FONT_SIZE + 2,
        HudStyle.INK, true))
    var sources := order_sources(band, kind)
    var above := int(order.get("refill_wanted", 0)) == 0 and float(order.get("on_hand", 0.0)) >= float(current_target)
    for source in sources:
        var row: Dictionary = source["row"]
        var line := HBoxContainer.new()
        line.add_theme_constant_override("separation", HudOrdersVocab.SRC_ROW_SEPARATION)
        line.set_meta(META_SOURCE_ROW, source_name(row, herd_label))
        var yours := bool(source["yours"])
        var name_label := BandOrdersView.text(source_name(row, herd_label), HudOrdersVocab.LINE_FONT_SIZE,
            HudStyle.WARN if yours else HudStyle.INK_DIM)
        name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        name_label.clip_text = true
        name_label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
        line.add_child(name_label)
        var setting := HudOrdersVocab.SRC_LEARNING if above else HudOrdersVocab.SRC_BEST
        if yours:
            setting = HudOrdersVocab.SRC_FLOOR_YOURS_FORMAT % SourceForecast.floor_percent(
                float(row.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR)))
        line.add_child(BandOrdersView.text(setting, HudOrdersVocab.SMALL_FONT_SIZE + 1, HudStyle.INK_FAINT))
        line.add_child(BandOrdersView.text(str(int(row.get("workers", 0))),
            HudOrdersVocab.SMALL_FONT_SIZE + 1, HudStyle.INK_FAINT))
        line.add_child(BandOrdersView.text(HudOrdersVocab.SRC_TAKE_FORMAT % HudOrdersVocab.product_of(row, kind),
            HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK_DIM))
        box.add_child(line)
    if kind == HudOrdersVocab.KIND_FODDER:
        box.add_child(BandOrdersView.text(HudOrdersVocab.CUT_BEST_FORMAT % who, HudOrdersVocab.LINE_FONT_SIZE,
            HudStyle.INK_DIM, true))
    else:
        var ease := (HudOrdersVocab.EASE_LEARNING_FORMAT % [current_target, who]) if above \
            else HudOrdersVocab.EASE_BEST_FORMAT % who
        box.add_child(BandOrdersView.text(ease, HudOrdersVocab.LINE_FONT_SIZE, HudStyle.INK_DIM, true))
    # Next turn.
    var take := float(order.get("take", 0.0))
    var net := take - draw
    var lines: Array = []
    var eta := HudOrdersVocab.eta_turns(order)
    if eta > 0:
        lines.append(HudOrdersVocab.NEXT_TURN_BACK_FORMAT % [HudOrdersVocab.target_text(order),
            HudOrdersVocab.turns(eta)])
    elif net < 0.0:
        lines.append(HudOrdersVocab.NEXT_TURN_FALLS_FORMAT % [HudOrdersVocab.order_name(kind), -net])
    if is_food:
        var hide := 0.0
        var bone := 0.0
        for source in sources:
            hide += HudOrdersVocab.product_of(source["row"], "hide")
            bone += HudOrdersVocab.product_of(source["row"], "bone")
        if hide > 0.0 or bone > 0.0:
            lines.append(HudOrdersVocab.BYPRODUCT_FORMAT % [hide, bone])
    else:
        for other in order_sources(band, HudOrdersVocab.KIND_FOOD):
            var hay_here := HudOrdersVocab.product_of(other["row"], HudOrdersVocab.KIND_FODDER)
            if hay_here > 0.0:
                lines.append(HudOrdersVocab.FODDER_COMES_HOME_FORMAT % [hay_here, source_name(other["row"], herd_label)])
    var note := (HudOrdersVocab.AFTER_EATING_FORMAT % ("%+.2f" % net)) if is_food else ("%+.2f" % net)
    next_turn_box(target, take, "food" if is_food else "fodder", lines, note)
    target.add_child(footer(ctx, kind, Callable(), false))
