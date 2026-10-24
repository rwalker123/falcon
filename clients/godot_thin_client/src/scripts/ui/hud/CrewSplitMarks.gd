class_name CrewSplitMarks
extends Control

## **ONE SMALL SQUARE PER WORKER THE SIM STAFFED** (#871) — how a staffed row's crew divides into
## keepers (the site's keeping hands), builders (hands raising the site's next rung), hands holding a
## whole kit, and bare hands.
##
## The keepers fill in a muted earth, then the builders in a cool blue, then the kitted hands in the
## bright green, then the bare hands in a quieter green; the whole run is aligned to the END of the
## control, under the crew number. It is drawn, not glyphs, so the four read as a bar of colour.
##
## **IT DRAWS; IT DOES NOT MODEL.** The four counts are the sim's — the row's own `keepers_staffed` /
## `kitted_staffed` / `bare_staffed` (and the build's hands) on a Work-tab row or a working source's sheet, the client's
## forecast crew on a source nobody works yet — and the caller hands them in. The words on its hover are
## `HudWorkVocab.crew_bar_words`, so the picture and the sentence are about one set of figures.
##
## **INFORMATION, NOT AN ALERT**: no warning ink and no ⚠. The colours are `HudStyle.CREW_TEND` /
## `CREW_BUILD` / `CREW_KITTED` / `CREW_BARE`, derived from the palette's own earth and green.
##
## **A WIDE CREW NEVER WIDENS ITS ROW.** The control claims only one mark plus a `+N` as its minimum
## width and fits the rest at draw time against the width it was actually given: first the marks
## shrink toward `CREW_SPLIT_MARK_SIZE_FLOOR`, then the first N are drawn and the rest are counted.
## The sentence on the hover still states the whole crew.
##
## **REACHABLE BY HOVER AND BY KEYBOARD.** The hover is the native tooltip; a keyboard focus floats the
## same sentence in a small card under the marks, because Godot shows no tooltip on focus.

var _keepers := 0
var _kitted := 0
var _bare := 0
var _builders := 0
var _mark_size := HudWorkVocab.CREW_SPLIT_MARK_SIZE
var _gap := HudWorkVocab.CREW_SPLIT_MARK_GAP
var _radius := HudWorkVocab.CREW_SPLIT_MARK_RADIUS
var _words := ""
var _focus_card: PanelContainer = null

## Build the bar for a crew of `keepers + builders + kitted + bare` (four segments, drawn in that order,
## always aligned to the end). `kit_names` is the kit's on-screen item words (`baskets`), `hold_crew` the
## wire's hands-to-hold figure (`0` = none). `sheet` takes the compose sheet's larger squares; a Work-tab
## row uses the smaller ones.
static func build(keepers: int, kitted: int, bare: int, kit_names: String, hold_crew: int,
        sheet: bool, builders: int = 0) -> CrewSplitMarks:
    var marks := CrewSplitMarks.new()
    marks.setup(keepers, kitted, bare, kit_names, hold_crew, sheet, builders)
    return marks

func setup(keepers: int, kitted: int, bare: int, kit_names: String, hold_crew: int,
        sheet: bool, builders: int = 0) -> void:
    _keepers = maxi(keepers, 0)
    _builders = maxi(builders, 0)
    _kitted = maxi(kitted, 0)
    _bare = maxi(bare, 0)
    if sheet:
        _mark_size = HudWorkVocab.CREW_SPLIT_SHEET_MARK_SIZE
        _gap = HudWorkVocab.CREW_SPLIT_SHEET_MARK_GAP
        _radius = HudWorkVocab.CREW_SPLIT_SHEET_MARK_RADIUS
    _words = HudWorkVocab.crew_bar_words(_keepers, _kitted, _bare, kit_names, hold_crew, _builders)
    tooltip_text = _words
    if "accessibility_description" in self:
        set("accessibility_description", _words)
    set_meta(HudWorkVocab.CREW_SPLIT_META,
        {"keepers": _keepers, "builders": _builders, "kitted": _kitted, "bare": _bare})
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
    return _keepers + _builders + _kitted + _bare

## The four segments it draws.
func keepers() -> int:
    return _keepers

func builders() -> int:
    return _builders

func kitted() -> int:
    return _kitted

func bare() -> int:
    return _bare

## **THE LAYOUT IT WOULD DRAW AT `width`** — `{size, gap, drawn, overflow}`. Pure, so a harness can
## ask the overflow question of the same arithmetic the draw uses.
func layout_for(width: float) -> Dictionary:
    var n := crew()
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
    var full := _row_width(crew(), _mark_size, _gap)
    var floored := HudWorkVocab.CREW_SPLIT_MARK_SIZE_FLOOR + _gap + _overflow_width(crew())
    return minf(full, floored)

func _draw() -> void:
    var plan := layout_for(size.x)
    var drawn := int(plan["drawn"])
    if drawn <= 0:
        return
    var mark := float(plan["size"])
    var gap := float(plan["gap"])
    var overflow := int(plan["overflow"])
    var segments := drawn_segments(drawn)
    var total := _row_width(drawn, mark, gap)
    if overflow > 0:
        total += gap + _overflow_width(overflow)
    var x := size.x - total
    var y := (size.y - mark) * 0.5
    for i in drawn:
        _draw_mark(Rect2(x, y, mark, mark), segments[i])
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

## Which segment the i-th mark belongs to: keepers first, then the builders, then the kitted hands,
## then the bare ones.
## **WHICH SEGMENT EACH DRAWN MARK WEARS, when the crew is wider than the room.** A crew that fits draws one
## mark per worker in segment order. One that does not draws `drawn` marks and counts the rest as `+N`, and
## those marks are shared out so that EVERY non-empty segment keeps at least one (a plain "first N" would
## draw only keepers and builders and hide the hands holding a kit and the bare ones), the remainder going
## to the segments in proportion to their size.
func drawn_segments(drawn: int) -> Array[int]:
    var counts: Array[int] = [_keepers, _kitted, _bare, _builders]
    var out: Array[int] = []
    if drawn >= crew():
        for i in crew():
            out.append(_segment_of(i))
        return out
    var shown: Array[int] = [0, 0, 0, 0]
    var spare := drawn
    for seg in 4:
        if counts[seg] > 0 and spare > 0:
            shown[seg] = 1
            spare -= 1
    var extra_total := 0
    for seg in 4:
        extra_total += counts[seg] - shown[seg]
    var budget := spare
    for seg in 4:
        if extra_total <= 0:
            break
        var share := mini(counts[seg] - shown[seg], budget * (counts[seg] - shown[seg]) / extra_total)
        shown[seg] += share
        spare -= share
    # What proportional rounding left over goes to whichever segment still has unshown workers, largest first.
    while spare > 0:
        var best := -1
        for seg in 4:
            if counts[seg] - shown[seg] > 0 and (best < 0 or counts[seg] - shown[seg] > counts[best] - shown[best]):
                best = seg
        if best < 0:
            break
        shown[best] += 1
        spare -= 1
    for seg in [SEGMENT_KEEPER, SEGMENT_BUILDER, SEGMENT_KITTED, SEGMENT_BARE]:
        for _k in shown[seg]:
            out.append(seg)
    return out

func _segment_of(i: int) -> int:
    if i < _keepers:
        return SEGMENT_KEEPER
    if i < _keepers + _builders:
        return SEGMENT_BUILDER
    if i < _keepers + _builders + _kitted:
        return SEGMENT_KITTED
    return SEGMENT_BARE

const SEGMENT_KEEPER := 0
const SEGMENT_KITTED := 1
const SEGMENT_BARE := 2
const SEGMENT_BUILDER := 3

## One mark, filled and outlined in its segment's colours.
func _draw_mark(rect: Rect2, segment: int) -> void:
    var radius := int(round(float(_radius) * rect.size.x / _mark_size))
    var fill := HudStyle.CREW_TEND
    var line := HudStyle.CREW_TEND_LINE
    if segment == SEGMENT_BUILDER:
        fill = HudStyle.CREW_BUILD
        line = HudStyle.CREW_BUILD_LINE
    elif segment == SEGMENT_KITTED:
        fill = HudStyle.CREW_KITTED
        line = HudStyle.CREW_KITTED_LINE
    elif segment == SEGMENT_BARE:
        fill = HudStyle.CREW_BARE
        line = HudStyle.CREW_BARE_LINE
    var base := StyleBoxFlat.new()
    base.bg_color = fill
    base.set_corner_radius_all(radius)
    base.draw(get_canvas_item(), rect)
    var edge := StyleBoxFlat.new()
    edge.draw_center = false
    edge.set_border_width_all(HudWorkVocab.CREW_SPLIT_MARK_BORDER)
    edge.border_color = line
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
