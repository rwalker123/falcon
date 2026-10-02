class_name CrewSplitMarks
extends Control

## **ONE SMALL SQUARE PER WORKER** — how a site's crew divides between tending it and taking from it.
##
## The tending share fills from the left in a muted earth, the takers are bright, and a person split
## between the two is one square shaded in part (the tending fraction from the left). It is drawn, not
## glyphs, so a part-person reads as a fraction rather than as a third kind of mark.
##
## **IT DRAWS; IT DOES NOT MODEL.** The tending figure is the sim's — the row's own `keep_hands` on a
## Work-tab row, the crew curve row's `keep_hands` on a compose sheet — and the caller hands it in.
## ⛔ Never the site's `upkeep_hands`: that sums every band keeping the site, so it is no row's figure. Whether a row
## carries marks at all is `HudWorkVocab.crew_split_shown`, and the words on its hover are
## `HudWorkVocab.crew_split_words`, so the picture and the sentence are about one figure.
##
## **INFORMATION, NOT AN ALERT**: no warning ink and no ⚠. The two colours are `HudStyle.CREW_TEND` /
## `CREW_TAKE`, derived from the palette's own earth and green.
##
## **A WIDE CREW NEVER WIDENS ITS ROW.** The control claims only one mark plus a `+N` as its minimum
## width and fits the rest at draw time against the width it was actually given: first the marks
## shrink toward `CREW_SPLIT_MARK_SIZE_FLOOR`, then the first N are drawn and the rest are counted.
## The sentence on the hover still states the whole crew.
##
## **REACHABLE BY HOVER AND BY KEYBOARD.** The hover is the native tooltip; a keyboard focus floats the
## same sentence in a small card under the marks, because Godot shows no tooltip on focus.

var _crew := 0
var _keep := 0.0
var _mark_size := HudWorkVocab.CREW_SPLIT_MARK_SIZE
var _gap := HudWorkVocab.CREW_SPLIT_MARK_GAP
var _radius := HudWorkVocab.CREW_SPLIT_MARK_RADIUS
var _align_end := true
var _words := ""
var _focus_card: PanelContainer = null

## Build the marks for a crew of `crew` of whom `keep_hands` tend. `sheet` takes the compose sheet's
## larger squares and aligns them to the start; a Work-tab row aligns them under its stepper.
static func build(crew: int, keep_hands: float, labor_kind: String, sheet: bool) -> CrewSplitMarks:
    var marks := CrewSplitMarks.new()
    marks.setup(crew, keep_hands, labor_kind, sheet)
    return marks

func setup(crew: int, keep_hands: float, labor_kind: String, sheet: bool) -> void:
    _crew = maxi(crew, 0)
    _keep = maxf(keep_hands, 0.0)
    if sheet:
        _mark_size = HudWorkVocab.CREW_SPLIT_SHEET_MARK_SIZE
        _gap = HudWorkVocab.CREW_SPLIT_SHEET_MARK_GAP
        _radius = HudWorkVocab.CREW_SPLIT_SHEET_MARK_RADIUS
    _align_end = not sheet
    _words = HudWorkVocab.crew_split_words(_crew, keep_hands, labor_kind)
    tooltip_text = _words
    if "accessibility_description" in self:
        set("accessibility_description", _words)
    set_meta(HudWorkVocab.CREW_SPLIT_META, {"crew": _crew, "keep": keep_hands})
    # PASS, so the row's own click still reaches the row and the tooltip still shows.
    mouse_filter = Control.MOUSE_FILTER_PASS
    focus_mode = Control.FOCUS_ALL
    size_flags_vertical = Control.SIZE_SHRINK_CENTER
    custom_minimum_size = Vector2(_min_width(), _mark_size)
    if not focus_entered.is_connected(_show_focus_card):
        focus_entered.connect(_show_focus_card)
        focus_exited.connect(_hide_focus_card)
    queue_redraw()

## The words the hover states — what `crew_split_words` answered for this crew.
func words() -> String:
    return _words

## How many workers this control was handed.
func crew() -> int:
    return _crew

## The tending share it draws, capped at the crew.
func keep_hands() -> float:
    return _keep

## **THE LAYOUT IT WOULD DRAW AT `width`** — `{size, gap, drawn, overflow}`. Pure, so a harness can
## ask the overflow question of the same arithmetic the draw uses.
func layout_for(width: float) -> Dictionary:
    var n := _crew
    var mark := _mark_size
    var gap := _gap
    if n <= 0:
        return {"size": mark, "gap": gap, "drawn": 0, "overflow": 0}
    if _row_width(n, mark, gap) > width:
        # Shrink toward the floor first, the gap in proportion, so a mid-sized crew keeps every mark.
        var ratio := clampf(width / _row_width(n, mark, gap),
            HudWorkVocab.CREW_SPLIT_MARK_SIZE_FLOOR / _mark_size, 1.0)
        mark = _mark_size * ratio
        gap = _gap * ratio
    var drawn := n
    if _row_width(n, mark, gap) > width:
        var room := width - _overflow_width(n) - gap
        drawn = clampi(int(floor((room + gap) / (mark + gap))), 1, n - 1)
    return {"size": mark, "gap": gap, "drawn": drawn, "overflow": n - drawn}

func _row_width(count: int, mark: float, gap: float) -> float:
    return float(count) * mark + float(maxi(count - 1, 0)) * gap

func _overflow_text(count: int) -> String:
    return HudWorkVocab.CREW_SPLIT_OVERFLOW_FORMAT % count

func _overflow_width(count: int) -> float:
    var font := get_theme_default_font()
    if font == null:
        return 0.0
    return font.get_string_size(_overflow_text(count), HORIZONTAL_ALIGNMENT_LEFT, -1,
        HudWorkVocab.ALLOC_SECTION_FONT_SIZE).x

## The legibility floor: the whole crew where it is small, else one floor-sized mark and its count.
func _min_width() -> float:
    var full := _row_width(_crew, _mark_size, _gap)
    var floored := HudWorkVocab.CREW_SPLIT_MARK_SIZE_FLOOR + _gap + _overflow_width(_crew)
    return minf(full, floored)

func _draw() -> void:
    var plan := layout_for(size.x)
    var drawn := int(plan["drawn"])
    if drawn <= 0:
        return
    var mark := float(plan["size"])
    var gap := float(plan["gap"])
    var overflow := int(plan["overflow"])
    var total := _row_width(drawn, mark, gap)
    if overflow > 0:
        total += gap + _overflow_width(overflow)
    var x := size.x - total if _align_end else 0.0
    var y := (size.y - mark) * 0.5
    for i in drawn:
        _draw_mark(Rect2(x, y, mark, mark), clampf(_keep - float(i), 0.0, 1.0))
        x += mark + gap
    if overflow > 0:
        var font := get_theme_default_font()
        if font != null:
            var font_size := HudWorkVocab.ALLOC_SECTION_FONT_SIZE
            var baseline := (size.y + font.get_ascent(font_size) - font.get_descent(font_size)) * 0.5
            draw_string(font, Vector2(x, baseline), _overflow_text(overflow),
                HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, HudStyle.INK_DIM)
    if has_focus():
        var ring := StyleBoxFlat.new()
        ring.draw_center = false
        ring.set_border_width_all(HudWorkVocab.CREW_SPLIT_MARK_BORDER)
        ring.border_color = HudStyle.SIGNAL
        ring.set_corner_radius_all(_radius)
        ring.draw(get_canvas_item(), Rect2(Vector2.ZERO, size))

## One mark: `tend` of it (0..1) is the tending share, filled from the left.
func _draw_mark(rect: Rect2, tend: float) -> void:
    var radius := int(round(float(_radius) * rect.size.x / _mark_size))
    var base := StyleBoxFlat.new()
    base.bg_color = HudStyle.CREW_TEND if tend >= 1.0 else HudStyle.CREW_TAKE
    base.set_corner_radius_all(radius)
    base.draw(get_canvas_item(), rect)
    if tend > 0.0 and tend < 1.0:
        var part := StyleBoxFlat.new()
        part.bg_color = HudStyle.CREW_TEND
        part.corner_radius_top_left = radius
        part.corner_radius_bottom_left = radius
        part.draw(get_canvas_item(), Rect2(rect.position, Vector2(rect.size.x * tend, rect.size.y)))
    var edge := StyleBoxFlat.new()
    edge.draw_center = false
    edge.set_border_width_all(HudWorkVocab.CREW_SPLIT_MARK_BORDER)
    edge.border_color = HudStyle.CREW_TEND_LINE if tend > 0.0 else HudStyle.CREW_TAKE_LINE
    edge.set_corner_radius_all(radius)
    edge.draw(get_canvas_item(), rect)

## Godot shows no tooltip on keyboard focus, so focus floats the same sentence in a small card.
func _show_focus_card() -> void:
    queue_redraw()
    _hide_focus_card()
    var card := PanelContainer.new()
    card.top_level = true
    card.mouse_filter = Control.MOUSE_FILTER_IGNORE
    card.add_theme_stylebox_override("panel", HudStyle.popup_panel_stylebox())
    var label := Label.new()
    label.text = _words
    label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    label.custom_minimum_size = Vector2(HudWorkVocab.CREW_SPLIT_FOCUS_CARD_WIDTH, 0.0)
    label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    label.add_theme_color_override("font_color", HudStyle.INK)
    card.add_child(label)
    add_child(card)
    var at := global_position + Vector2(0.0, size.y + _gap)
    var room := get_viewport_rect().size.x - HudWorkVocab.CREW_SPLIT_FOCUS_CARD_WIDTH
    card.global_position = Vector2(clampf(at.x, 0.0, maxf(room, 0.0)), at.y)
    _focus_card = card

func _hide_focus_card() -> void:
    queue_redraw()
    if _focus_card != null and is_instance_valid(_focus_card):
        _focus_card.queue_free()
    _focus_card = null

## The floating focus card, `null` while the marks are not focused.
func focus_card() -> PanelContainer:
    return _focus_card
