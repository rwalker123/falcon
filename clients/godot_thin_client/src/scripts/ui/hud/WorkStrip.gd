class_name WorkStrip
extends RefCounted

## **THE WORK STRIP — ONE CONTROL FOR A SOURCE'S RUNG, WARNINGS AND CREW** (issue #877,
## `docs/plan_labor_at_scale.md` → "Builds join the site's crew"). Built once and mounted by every Work-tab
## row with a crew (forage, hunt, extract, tile, band improvement) AND by the compose sheet of the same
## source, so a row and its sheet can never disagree.
##
## Left to right: the standing rung's mark (a ring button on a penned herd), the ladder slot, the floor
## glyph, `⚠`, `ⓘ`, then a column of the crew number over the crew bar. A part the source does not have
## renders nothing (a tile row has no ladder and no floor).
##
## - **THE LADDER SLOT** is `⌃` + the next rung's verb glyph while a rung is on offer ("Take it to…"),
##   and the SAME button becomes a chip whose background fills to the build's progress while one is in
##   flight (face: verb glyph + percent). A stalled build is that chip with the "no spare hands"
##   hover; there is no separate warning label for it. Pressing it opens the rung card.
## - **`⚠` IS ONE MARK** with one producer, the sim's flags: it shows when any of the row's hazards fires
##   (overdraw, a rung at risk, a short kit, a pen underfed, keeping short) and its hover is their
##   existing sentences, one per line. Nothing here derives a warning.
## - **THE CREW** is the sim's four-way split (keepers, builders, hands with a kit, bare hands). The
##   number is the sum, the bar is `CrewSplitMarks`.
##
## **ONE MODEL, ONE BUILDER.** Every caller builds its model through `model_from_row` and draws it with
## `build`; the producers of the row dictionary (`BandPanelController`) fill one vocabulary.

## ---- the strip model's keys ---------------------------------------------------------------------
const M_RUNG_MARK := "rung_mark"
const M_RUNG_TIP := "rung_tip"
const M_RING_OFFERED := "ring_offered"
const M_LADDER_KIND := "ladder_kind"
const M_LADDER_GLYPH := "ladder_glyph"
const M_LADDER_PROGRESS := "ladder_progress"
const M_LADDER_TIP := "ladder_tip"
const M_FLOOR_GLYPH := "floor_glyph"
const M_FLOOR_TIP := "floor_tip"
const M_WARN_LINES := "warn_lines"
const M_INFO_TIP := "info_tip"
const M_KEEPERS := "keepers"
const M_BUILDERS := "builders"
const M_KITTED := "kitted"
const M_BARE := "bare"
const M_HOLD := "hold"
const M_KIT_NAMES := "kit_names"
const M_CREW := "crew"
const M_NEED_ROW := "need_row"
const M_NEED_CREW := "need_crew"
const M_LENT := "lent"
const M_ROW := "row"

## The row-vocabulary keys the strip reads for the site's build crew: the wire's `builders_staffed` and the
## hover composed from it.
const ROW_BUILDERS_KEY := "builders_staffed"
const ROW_BUILD_TIP_KEY := "build_tip"

## The ladder slot's three kinds, the `WORK_ROW_BUILD_KIND_*` words.
const LADDER_NONE := HudWorkVocab.WORK_ROW_BUILD_KIND_NONE
const LADDER_OFFER := HudWorkVocab.WORK_ROW_BUILD_KIND_OFFER
const LADDER_BUILDING := HudWorkVocab.WORK_ROW_BUILD_KIND_BUILDING
const LADDER_STALLED := HudWorkVocab.WORK_ROW_BUILD_KIND_STALLED

## **A BUILD'S PHASE — THE ONE DECISION THE CHIP'S BORDER AND ITS HOVER BOTH READ.** A declared build is
## worked by its own site's crew (`docs/plan_labor_at_scale.md` → "Builds join the site's crew"), so there are
## two phases and the sim's `builders_staffed` on the row picks between them. `stalled`: nobody is on this
## build (amber border; the hover says every worker is busy on a work row). `building`: at least one
## builder is on it.
const PHASE_STALLED := "stalled"
const PHASE_BUILDING := "building"

static func build_phase(row: Dictionary) -> String:
    return PHASE_BUILDING if int(row.get(ROW_BUILDERS_KEY, 0)) > 0 else PHASE_STALLED

## **THE STRIP MODEL FOR ONE ROW.** `row` is the row-vocabulary dictionary the Work tab's producers
## fill (`BandPanelController._work_source_model` and its extract / tile / improvement siblings). It
## carries the site's own `builders_staffed` and the hover the producers composed from the wire row
## (`build_tip`, `DetailFormat.build_crew_tooltip`).
static func model_from_row(row: Dictionary) -> Dictionary:
    var builders := maxi(int(row.get(ROW_BUILDERS_KEY, 0)), 0)
    var keepers := int(row.get("keepers_staffed", 0))
    var kitted := int(row.get("kitted_staffed", 0))
    var bare := int(row.get("bare_staffed", 0))
    var workers := int(row.get("workers", 0))
    var building_glyph := String(row.get("building_glyph", ""))
    var ready_glyph := String(row.get("ready_glyph", ""))
    var progress := clampf(float(row.get("building_progress", 0.0)), 0.0, 1.0)
    var kind := LADDER_NONE
    var glyph := ""
    var tip := ""
    if building_glyph != "":
        glyph = building_glyph
        kind = LADDER_STALLED if build_phase(row) == PHASE_STALLED else LADDER_BUILDING
        tip = String(row.get(ROW_BUILD_TIP_KEY, ""))
    elif ready_glyph != "":
        kind = LADDER_OFFER
        glyph = ready_glyph
        tip = HudWorkVocab.WORK_STRIP_OFFER_TIP
    var rung_tip := String(row.get("rung_tooltip", ""))
    if bool(row.get("ring_in_flight", false)):
        rung_tip = HudWorkVocab.WORK_ROW_RING_BUILDING_TOOLTIP_FORMAT % HudFormat.progress_percent(
            float(row.get("ring_progress", 0.0)))
    var floor_glyph := ""
    var floor_tip := ""
    if row.has("floor") and String(row.get("kind", "")) in [
            SourceForecast.LABOR_KIND_FORAGE, SourceForecast.LABOR_KIND_HUNT]:
        var floor := float(row.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))
        floor_glyph = FoodIcons.for_floor_zone(SourceForecast.floor_zone(floor))
        floor_tip = HudFormat.floor_hint(floor, String(row.get("kind", "")))
    # THE ONE ⚠ AND THE ⓘ: the sim's flags, as sentences the producers already wrote.
    var lines: Array[String] = []
    for line in (row.get("warn_lines", []) as Array):
        if String(line) != "" and not lines.has(String(line)):
            lines.append(String(line))
    var kept := float(row.get("keep_kept", SourceForecast.NO_UPKEEP_DEMAND))
    var demand := float(row.get("keep_demand", SourceForecast.NO_UPKEEP_DEMAND))
    var tools_short := bool(row.get("keep_tools_short", false))
    var keeping_hint := HudWorkVocab.site_keeping_hint(row.get("tending", {}) as Dictionary, kept,
        demand, tools_short, String(row.get("keep_tools_named", "")))
    var info_tip := ""
    if HudWorkVocab.site_keeping_is_short(kept, demand):
        if keeping_hint != "" and not lines.has(keeping_hint):
            lines.append(keeping_hint)
    elif tools_short:
        info_tip = keeping_hint
    var need_row := bool(row.get("need_row", false))
    return {
        M_RUNG_MARK: String(row.get("rung_glyph", "")),
        M_RUNG_TIP: rung_tip,
        M_RING_OFFERED: bool(row.get("ring_offered", false)),
        M_LADDER_KIND: kind, M_LADDER_GLYPH: glyph, M_LADDER_PROGRESS: progress, M_LADDER_TIP: tip,
        M_FLOOR_GLYPH: floor_glyph, M_FLOOR_TIP: floor_tip,
        M_WARN_LINES: lines, M_INFO_TIP: info_tip,
        M_KEEPERS: keepers, M_BUILDERS: builders, M_KITTED: kitted, M_BARE: bare,
        M_HOLD: int(row.get("hold_crew", 0)), M_KIT_NAMES: String(row.get("kit_names", "")),
        M_CREW: workers + builders,
        M_NEED_ROW: need_row, M_NEED_CREW: int(row.get("muster_crew", 0)),
        M_LENT: int(row.get("lent_to_hunt", 0)),
        M_ROW: row,
    }

## **DRAW THE STRIP.** `handlers` carries `on_ladder: Callable(anchor: Control)` and
## `on_ring: Callable(anchor: Control)`, each called with the control that was pressed so the rung card
## anchors under it. `sheet` takes the compose sheet's larger squares and lets the marks fill the width.
static func build(model: Dictionary, handlers: Dictionary, sheet: bool = false) -> HBoxContainer:
    var strip := HBoxContainer.new()
    strip.add_theme_constant_override("separation", HudWorkVocab.WORK_STRIP_SEPARATION)
    strip.mouse_filter = Control.MOUSE_FILTER_PASS
    strip.set_meta(HudWorkVocab.WORK_STRIP_META, model)
    var marks := HBoxContainer.new()
    marks.add_theme_constant_override("separation", HudWorkVocab.WORK_STRIP_MARKS_SEPARATION)
    marks.mouse_filter = Control.MOUSE_FILTER_PASS
    marks.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
    if sheet:
        marks.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    strip.add_child(marks)
    _add_rung_mark(marks, model, handlers)
    _add_ladder(marks, model, handlers)
    var floor_glyph := String(model.get(M_FLOOR_GLYPH, ""))
    if floor_glyph != "":
        var floor_label := _mark_label(floor_glyph, HudStyle.INK_DIM, String(model.get(M_FLOOR_TIP, "")))
        floor_label.set_meta(HudWorkVocab.WORK_STRIP_FLOOR_META, floor_glyph)
        floor_label.set_meta(HudWorkVocab.WORK_ROW_MARKS_META, floor_glyph)
        marks.add_child(floor_label)
    var warn_lines: Array = model.get(M_WARN_LINES, [])
    if not warn_lines.is_empty():
        var warn := _mark_label(HudSelectionVocab.RUNG_HAZARD_GLYPH, HudStyle.WARN,
            HudWorkVocab.WORK_STRIP_WARN_SEPARATOR.join(PackedStringArray(warn_lines)))
        warn.set_meta(HudWorkVocab.WORK_STRIP_WARN_META, warn_lines)
        marks.add_child(warn)
    var info_tip := String(model.get(M_INFO_TIP, ""))
    if info_tip != "":
        var info := _mark_label(HudWorkVocab.SITE_KEEPING_TOOLS_MARK, HudStyle.INK_DIM, info_tip)
        info.set_meta(HudWorkVocab.WORK_STRIP_INFO_META, info_tip)
        marks.add_child(info)
    strip.add_child(_build_crew_column(model, sheet))
    return strip

static func _mark_label(text: String, ink: Color, tip: String) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", ink)
    label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    # PASS: a Label needs a non-IGNORE filter for its tooltip to show, and PASS still lets the press
    # reach the row under it (`HudWidgets.set_label_tooltip` sets STOP, which would punch a hole).
    label.mouse_filter = Control.MOUSE_FILTER_PASS if tip != "" else Control.MOUSE_FILTER_IGNORE
    label.tooltip_text = tip
    return label

## The standing rung's mark; on a penned herd a ghost button that opens the ring card.
static func _add_rung_mark(marks: HBoxContainer, model: Dictionary, handlers: Dictionary) -> void:
    var glyph := String(model.get(M_RUNG_MARK, ""))
    var ring := bool(model.get(M_RING_OFFERED, false))
    if glyph == "" and not ring:
        # No mark to draw, so no column to spend: the run itself carries the (blank) rung stamp, which is
        # how a board finds a row whether or not it stands on a rung mark.
        marks.set_meta(HudWorkVocab.WORK_ROW_RUNG_META, "")
        return
    var mark: Control
    if ring:
        var ring_btn := Button.new()
        ring_btn.text = HudWorkVocab.WORK_ROW_RING_FORMAT % glyph
        ring_btn.focus_mode = Control.FOCUS_NONE
        ring_btn.tooltip_text = HudWorkVocab.WORK_ROW_RING_TOOLTIP
        HudStyle.apply_button(ring_btn, "ghost")
        HudWidgets.compact(ring_btn, HudWorkVocab.WORK_ROW_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
        ring_btn.add_theme_color_override("font_color", HudStyle.SIGNAL)
        var on_ring: Callable = handlers.get("on_ring", Callable())
        if on_ring.is_valid():
            ring_btn.pressed.connect(func() -> void: on_ring.call(ring_btn))
        ring_btn.mouse_filter = Control.MOUSE_FILTER_STOP
        mark = ring_btn
    else:
        mark = _mark_label(glyph, HudStyle.SIGNAL, String(model.get(M_RUNG_TIP, "")))
    mark.custom_minimum_size = Vector2(HudWorkVocab.WORK_ROW_RUNG_WIDTH, 0.0)
    mark.set_meta(HudWorkVocab.WORK_ROW_RUNG_META, glyph)
    mark.set_meta(HudWorkVocab.WORK_ROW_RING_META, ring)
    marks.add_child(mark)

## The ladder slot: the `⌃` offer, or the chip filling to the build's progress. Both open the rung card.
static func _add_ladder(marks: HBoxContainer, model: Dictionary, handlers: Dictionary) -> void:
    var kind := String(model.get(M_LADDER_KIND, LADDER_NONE))
    if kind == LADDER_NONE:
        return
    var glyph := String(model.get(M_LADDER_GLYPH, ""))
    var building := kind != LADDER_OFFER
    var face := HudWorkVocab.WORK_ROW_BUILDING_FORMAT % [glyph,
        HudFormat.progress_percent(float(model.get(M_LADDER_PROGRESS, 0.0)))] \
        if building else HudWorkVocab.WORK_ROW_READY_FORMAT % glyph
    var on_ladder: Callable = handlers.get("on_ladder", Callable())
    var chip := build_chip(face, float(model.get(M_LADDER_PROGRESS, 0.0)) if building else 0.0,
        String(model.get(M_LADDER_TIP, "")), building, kind == LADDER_STALLED,
        func(anchor: Control) -> void:
            if on_ladder.is_valid():
                on_ladder.call(anchor))
    chip.set_meta(HudWorkVocab.WORK_STRIP_LADDER_META, kind)
    chip.set_meta(HudWorkVocab.WORK_ROW_BUILD_KIND_META, kind)
    chip.set_meta(HudWorkVocab.WORK_ROW_BUILD_STATE_META, face)
    chip.set_meta(HudWorkVocab.WORK_ROW_MODEL_META, model.get(M_ROW, {}))
    marks.add_child(chip)

## **A BUTTON THAT FILLS.** A `PanelContainer` over one rect: the ghost button (chrome and hit area,
## FIRST so it draws beneath), the progress fill, then the face, transparent to the pointer so every
## click lands on the button. `on_press` receives the container as the anchor for the rung card.
static func build_chip(face: String, progress: float, tip: String, filled: bool, warn: bool,
        on_press: Callable) -> PanelContainer:
    var chip := PanelContainer.new()
    chip.add_theme_stylebox_override("panel", StyleBoxEmpty.new())
    chip.custom_minimum_size = Vector2(HudWorkVocab.WORK_STRIP_LADDER_MIN_WIDTH, 0.0)
    chip.tooltip_text = tip
    var button := Button.new()
    button.focus_mode = Control.FOCUS_NONE
    button.tooltip_text = tip
    HudStyle.apply_button(button, "ghost")
    HudWidgets.trim_button_padding(button, HudWorkVocab.WORK_STRIP_CHIP_PADDING_V)
    if warn:
        for state in ["normal", "hover", "pressed"]:
            var box := button.get_theme_stylebox(state).duplicate() as StyleBoxFlat
            if box != null:
                box.border_color = HudStyle.WARN
                button.add_theme_stylebox_override(state, box)
    button.pressed.connect(func() -> void: on_press.call(chip))
    chip.add_child(button)
    if filled:
        chip.add_child(fill_layer(progress))
    chip.add_child(_chip_face(face, warn))
    return chip

static func _chip_face(face: String, warn: bool) -> MarginContainer:
    var inner := MarginContainer.new()
    inner.mouse_filter = Control.MOUSE_FILTER_IGNORE
    for side in ["margin_left", "margin_right"]:
        inner.add_theme_constant_override(side, HudWorkVocab.WORK_STRIP_CHIP_PADDING_H)
    var label := Label.new()
    label.text = face
    label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    label.add_theme_color_override("font_color", HudStyle.WARN if warn else HudStyle.INK)
    inner.add_child(label)
    return inner

## **THE PROGRESS FILL**: a full-rect layer whose coloured child spans the left `progress` of it. Stamped
## with the progress it was drawn at so a harness reads the number and not the pixels.
static func fill_layer(progress: float) -> Control:
    var layer := Control.new()
    layer.mouse_filter = Control.MOUSE_FILTER_IGNORE
    var amount := clampf(progress, 0.0, 1.0)
    var bar := ColorRect.new()
    bar.mouse_filter = Control.MOUSE_FILTER_IGNORE
    bar.color = Color(HudStyle.SIGNAL_DEEP, HudWorkVocab.WORK_STRIP_FILL_ALPHA)
    bar.anchor_left = 0.0
    bar.anchor_top = 0.0
    bar.anchor_bottom = 1.0
    bar.anchor_right = amount
    bar.offset_left = 0.0
    bar.offset_right = 0.0
    bar.offset_top = 0.0
    bar.offset_bottom = 0.0
    layer.add_child(bar)
    layer.set_meta(HudWorkVocab.WORK_STRIP_FILL_META, amount)
    return layer

## The crew column: the number over the bar. A need row keeps its need-count label in the number slot.
static func _build_crew_column(model: Dictionary, sheet: bool) -> VBoxContainer:
    var column := VBoxContainer.new()
    column.add_theme_constant_override("separation", 0)
    column.mouse_filter = Control.MOUSE_FILTER_PASS
    column.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
    column.set_meta(HudWorkVocab.WORK_STRIP_CREW_META, int(model.get(M_CREW, 0)))
    var need_row := bool(model.get(M_NEED_ROW, false))
    var number: Control
    if need_row:
        var crew := int(model.get(M_NEED_CREW, 0))
        var label := Label.new()
        label.text = HudWorkVocab.need_count_text(crew)
        label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_STEPPER_FONT_SIZE)
        label.add_theme_color_override("font_color", HudStyle.WARN if crew > 0 else HudStyle.INK_DIM)
        label.mouse_filter = Control.MOUSE_FILTER_IGNORE
        label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
        label.set_meta(HudWorkVocab.WORK_ROW_NEED_COUNT_META, label.text)
        number = label
    else:
        number = _crew_number(int(model.get(M_CREW, 0)), int(model.get(M_LENT, 0)))
    number.size_flags_horizontal = Control.SIZE_SHRINK_END
    column.add_child(number)
    if not need_row:
        var bar := CrewSplitMarks.build(int(model.get(M_KEEPERS, 0)), int(model.get(M_KITTED, 0)),
            int(model.get(M_BARE, 0)), String(model.get(M_KIT_NAMES, "")), int(model.get(M_HOLD, 0)),
            sheet, int(model.get(M_BUILDERS, 0)))
        bar.custom_minimum_size.x = maxf(bar.custom_minimum_size.x, HudWorkVocab.WORK_STRIP_CREW_WIDTH)
        bar.size_flags_horizontal = Control.SIZE_SHRINK_END
        column.add_child(bar)
    return column

## The crew number, right-aligned over the bar. A DONOR row shows its standing count struck through
## beside what is left after the loan.
static func _crew_number(crew: int, lent: int) -> Control:
    if lent <= 0:
        var value := HudWidgets.build_crew_number(crew)
        value.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
        return value
    var rich := RichTextLabel.new()
    rich.bbcode_enabled = true
    rich.fit_content = true
    rich.scroll_active = false
    rich.autowrap_mode = TextServer.AUTOWRAP_OFF
    rich.custom_minimum_size = Vector2(HudWorkVocab.WORK_ROW_LENT_VALUE_WIDTH, 0.0)
    rich.mouse_filter = Control.MOUSE_FILTER_IGNORE
    rich.add_theme_font_size_override("normal_font_size", HudWorkVocab.WORK_STEPPER_FONT_SIZE)
    rich.add_theme_color_override("default_color", HudStyle.INK)
    rich.text = "[right][color=#%s][s]%d[/s][/color] → %d[/right]" % [
        HudStyle.INK_DIM.to_html(false), crew, crew - lent]
    rich.set_meta(HudWorkVocab.WORK_ROW_LENT_COUNT_META,
        HudWorkVocab.WORK_ROW_LENT_COUNT_FORMAT % [crew, crew - lent])
    return rich
