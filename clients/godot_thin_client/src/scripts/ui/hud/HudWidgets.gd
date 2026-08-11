class_name HudWidgets

## THE SHARED HUD WIDGET FACTORY (docs/plan_hud_decomposition.md, the `DrawerComposeController` precursor).
##
## WHAT THIS IS. Every reusable Control the HUD builds out of raw Godot nodes and this project's own
## chrome vocabulary: the −/+ worker stepper in both its forms, the row labels and note/status parts
## it composes from, the zone CHROME (the column / block / plain-`Control` wrapper / child-clearing
## primitives all three Band-panel zones and the flat fallback host are assembled from), the stacked
## composition bar + its key (one primitive, two questions — PEOPLE and WORKFORCE), the zone section
## head and its `⋯` menu, the take-policy picker, the harvest floor's chart + its two crew targets +
## its verdict line, the dim hint/section labels, the inline text link, the BBCode forecast readout — plus the two MUTATORS (`compact`, `set_label_tooltip`) that fix up a
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
## `HudFormat`, which this file calls freely (both are static); the split is so `FactionReadouts` can
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

## THE SPECIES / SITE MARK ON A TEXT ROW — bundled art where the client has it, the emoji where it
## does not. One builder, so the HUD's text surfaces cannot drift apart from each other or from the
## map.
##
## WHY IT EXISTS (issue #439). The map has drawn `FaunaSprites` / `SiteSprites` art for a while; the
## HUD's text rows kept rendering `FoodIcons`' emoji, and **Unicode ships ONE deer for the four
## cervids the roster carries** — Red Deer, Wild Elk, Wild Reindeer and Desert Gazelle all resolve to
## 🦌 — so a work row, a roster row and the quarry picker could not tell them apart. Splitting the
## MARKER art fixed the map and left every text surface collided.
##
## WHY A `TextureRect` AND NOT `[img]` BBCODE. `CropRoleSprites` renders through `[img]` because its
## host genuinely IS a `RichTextLabel`; every host here is a `Label` in an `HBoxContainer`. The
## precedent for THIS situation is `StageSprites` + `BandCityPanel.set_header`, which swaps a
## `TextureRect` in for a glyph `Label`. Choose by host widget — do not convert a row to RichTextLabel
## to get an icon into it.
##
## **THE `null` BRANCH IS LOAD-BEARING even at full art coverage**, exactly as it is in the sprite
## tables themselves: it catches a herd label naming a species the client does not know
## (`FoodIcons.species_key_for` → `""`) and the `HERD_DEFAULT` case, neither of which has a key to
## look art up by — and the land row's module-less `◈`, which is not a species at all.
##
## **THE SPRITE IS DRAWN UNTINTED — never set `modulate` on what this returns.** That is the map
## markers' own rule (`.claude/rules/client/sprites-widgets.md`): a full-colour animal carries no
## state, so state rides GEOMETRY beside it — the work row's severity stripe, the roster row's
## ecology dot, the marks column. Tinting a marker was tried on the map, rendered as a slightly
## darker brown animal, and was reverted.
##
## **THE GLYPH BRANCH IS THE ONE THAT TAKES A COLOUR, and it is the caller's to supply.** A bare
## `Label` carries no `font_color` override and this client applies no `Theme` resource, so an
## un-coloured glyph renders at Godot's STOCK near-white — which on a host whose text is `INK_DIM`
## reads as a brighter mark beside a dimmer name, and stops tracking the row's state entirely. The
## glyph used to live INSIDE the host's own label (`"%s %s" % [glyph, name]`) and inherited that
## label's colour for free; splitting it out is what dropped the inheritance, so hosts whose text is
## state-tinted pass `glyph_color` and hosts whose text is stock leave it `null`. `null` means "set
## no override at all", which is exactly what those stock-coloured hosts had before this parameter
## existed. The TEXTURE branch ignores it — see the untinted rule above.
static func build_marker_icon(texture: Texture2D, glyph: String, box_px: float, font_size: int,
        glyph_color = null) -> Control:
    if texture != null:
        var art := TextureRect.new()
        art.texture = texture
        # EXPAND_IGNORE_SIZE so the 256px source cannot set the row's minimum width, and
        # KEEP_ASPECT_CENTERED so the box stays a BOX: the art sits inside it at its own aspect
        # instead of being stretched square (these sources are not square — see `icon_prompts.txt`).
        art.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
        art.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
        art.custom_minimum_size = Vector2(box_px, box_px)
        art.mouse_filter = Control.MOUSE_FILTER_IGNORE
        return art
    var label := Label.new()
    label.text = glyph
    label.add_theme_font_size_override("font_size", font_size)
    if glyph_color != null:
        label.add_theme_color_override("font_color", glyph_color)
    # Width only: the emoji sets its own height off the font, and pinning it would fight the row.
    label.custom_minimum_size = Vector2(box_px, 0.0)
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
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

## A `build_section_menu` entry's OPTIONAL radio-check flag. **ITS ABSENCE IS NOT `false`.** An entry
## carrying the key is a member of a mutually exclusive SET and is built as a radio-check item — the
## menu then states which member is active, which a menu of plain items structurally cannot. An entry
## WITHOUT it is a plain action (`Unassign all …`), and marking one would claim it belongs to a set it
## has no members of; hence the key is tested with `has` rather than read with a `false` default.
const MENU_ENTRY_CHECKED := "checked"

## A `build_section_menu` entry's OPTIONAL `Texture2D` icon — the species/site ART where the client
## has any. Also absent-is-not-empty: an entry without it is built with no icon at all rather than
## with a null one, so every existing text-only menu is byte-identical. It exists because
## **Unicode ships ONE deer** — the same reason `build_marker_icon` does — so a menu that could only
## carry an emoji would render two roster species identically and defeat its own purpose as a
## chooser. A caller that HAS no art for a species falls back to the emoji in the LABEL, which is
## the `build_marker_icon` split read through a `PopupMenu`'s own item API.
const MENU_ENTRY_ICON := "icon"

## The `⋯` section menu: a `MenuButton`, so its popup is a WINDOW and opening it cannot change any
## zone's layout height (the whole zone model depends on heights not moving). `entries` is an ordered
## array of `{label, disabled, on_pick}` dictionaries, each optionally carrying `MENU_ENTRY_CHECKED`
## and/or `MENU_ENTRY_ICON`.
static func build_section_menu(entries: Array, tooltip: String) -> MenuButton:
    var button := MenuButton.new()
    button.text = HudWorkVocab.SECTION_MENU_GLYPH
    button.tooltip_text = tooltip
    button.focus_mode = Control.FOCUS_NONE
    button.custom_minimum_size = Vector2(HudWorkVocab.SECTION_MENU_WIDTH, 0.0)
    HudStyle.apply_button(button, "ghost")
    compact(button, HudWorkVocab.ZONE_HEAD_FONT_SIZE, HudWorkVocab.ZONE_MENU_PADDING_V)
    _fill_menu_popup(button.get_popup(), entries)
    return button

## **A COMPOSE-SHEET FIELD ROW'S KEY LABEL** — `Band:`, `Kit`, `Quarry`. Its whole job is the ONE
## declared width (`HudComposeVocab.COMPOSE_FIELD_KEY_WIDTH`) that makes three rows built by three
## different modules line their value controls up; the reasoning is on that constant. `SIZE_FILL`, not
## `EXPAND` — the key takes exactly its declared width and the CONTROL is the row's only expanding
## child, so a third widget on the row (the quarry chooser) comes out of the value's share rather than
## out of the key's, and a row with two children and a row with three still start their value at the
## same x.
static func build_field_key(text: String) -> Label:
    var key := Label.new()
    key.text = text
    key.custom_minimum_size = Vector2(HudComposeVocab.COMPOSE_FIELD_KEY_WIDTH, 0.0)
    key.size_flags_horizontal = Control.SIZE_FILL
    key.add_theme_color_override("font_color", HudStyle.INK)
    return key

## **THE CHOICE AS A NATIVE SELECTOR** — an `OptionButton` whose face states the CURRENT choice, for a
## control that is a chooser in its own right rather than an overflow on a section head (the kit
## picker, and the compose sheets' `Band:` picker).
##
## **IT REPLACED A `MenuButton` WHOSE FACE CARRIED A `⌄` GLYPH, AND THE MECHANISM IS THE WHOLE POINT.**
## A `MenuButton` draws no arrow, so the affordance had to be baked into `text` — where `clip_text`
## eats it the moment the label reaches the button's edge (`Gathering kit` did, so the forage sheet's
## kit picker showed no caret at all) and where it renders as a small low-baseline mark rather than as
## the themed arrow the `Band:` picker one row above already drew. An `OptionButton` reserves the
## arrow's width as an internal right margin, so the icon is drawn OUTSIDE the text's clip rect and no
## face can push it off. It also CHECKS the current entry natively — its popup items are radio-check
## items and it marks the selected one itself — which is the behaviour `_fill_menu_popup` hand-rolls
## through `MENU_ENTRY_CHECKED`.
##
## `entries` is an ordered array of `{label, disabled, tooltip, on_pick}` — `build_section_menu`'s
## contract minus `MENU_ENTRY_CHECKED`, which a native selector OWNS: `selected_index` is both the
## entry the face opens on and the one the popup marks, and passing a second, hand-rolled mark would
## let the two disagree. (No `MENU_ENTRY_ICON` either: neither caller has per-entry art, and repeating
## ONE glyph down every row is noise rather than a distinction.)
##
## **`disabled` is the SAME key `_fill_menu_popup` already honours**, so the two menu families state
## an unavailable entry one way. It is an *unpressable row that is still read*, never a hidden one:
## `item_selected` is not emitted for it, and a caller that greys an entry is expected to say why —
## the kit picker puts the reason in the entry's own `label` and repeats it in `tooltip`.
##
## **`face` OVERRIDES the closed face, and the two are deliberately not the same sentence.** A list
## entry may carry a marker that belongs only in the list (the kit roster tags its job default) and
## the face may carry a glyph the list deliberately omits. `select()` writes the item's own text into
## `text`, so the override is applied AFTER it; every caller rebuilds the whole row on a pick, so the
## next selection's write is never the thing left on screen.
##
## `EXPAND_FILL` + `clip_text` are load-bearing together, the picked-quarry button's rule: `clip_text`
## drops the minimum width to ~0, so without the expand the control collapses to a sliver beside its
## key label — and without the clip a long entry name widens the row past the dock column.
## `fit_to_longest_item` is OFF for that same reason: it sets the minimum width from the widest ENTRY,
## which is exactly the dock-widening `clip_text` is here to prevent.
static func build_option_picker(entries: Array, selected_index: int, face: String,
        tooltip: String) -> OptionButton:
    var button := OptionButton.new()
    button.tooltip_text = tooltip
    button.focus_mode = Control.FOCUS_NONE
    button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    button.clip_text = true
    button.fit_to_longest_item = false
    HudStyle.apply_button(button, "ghost")
    var picks: Array[Callable] = []
    for entry_variant in entries:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        var index := picks.size()
        button.add_item(String(entry.get("label", "")), index)
        button.set_item_disabled(index, bool(entry.get("disabled", false)))
        var entry_tooltip := String(entry.get("tooltip", ""))
        if entry_tooltip != "":
            button.set_item_tooltip(index, entry_tooltip)
        var pick: Variant = entry.get("on_pick", null)
        picks.append(pick if pick is Callable else Callable())
    if selected_index >= 0 and selected_index < picks.size():
        button.select(selected_index)
    button.text = face
    button.item_selected.connect(func(index: int) -> void:
        if index >= 0 and index < picks.size() and picks[index].is_valid():
            picks[index].call())
    return button

## The shared popup fill for the `⋯` menu face above — one implementation of the entry contract
## (`{label, disabled, on_pick}` + the optional `MENU_ENTRY_CHECKED` / `MENU_ENTRY_ICON`).
static func _fill_menu_popup(popup: PopupMenu, entries: Array) -> void:
    var picks: Array[Callable] = []
    for entry_variant in entries:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        var index := picks.size()
        var label := String(entry.get("label", ""))
        var icon_variant: Variant = entry.get(MENU_ENTRY_ICON, null)
        var icon: Texture2D = icon_variant as Texture2D if icon_variant is Texture2D else null
        if entry.has(MENU_ENTRY_CHECKED):
            if icon != null:
                popup.add_icon_radio_check_item(icon, label, index)
            else:
                popup.add_radio_check_item(label, index)
            popup.set_item_checked(index, bool(entry[MENU_ENTRY_CHECKED]))
        elif icon != null:
            popup.add_icon_item(icon, label, index)
        else:
            popup.add_item(label, index)
        if icon != null:
            # The bundled art is a 256px source; a `PopupMenu` draws an item icon at its native size
            # and sizes the popup around it, so an uncapped mark would make a menu the width of the
            # screen. Capped to the same width a species mark takes on any other TEXT ROW in this HUD
            # (`build_marker_icon`'s callers), which is what a menu item is.
            popup.set_item_icon_max_width(index, int(HudWorkVocab.WORK_ROW_ICON_WIDTH))
        popup.set_item_disabled(index, bool(entry.get("disabled", false)))
        var pick: Variant = entry.get("on_pick", null)
        picks.append(pick if pick is Callable else Callable())
    popup.id_pressed.connect(func(id: int) -> void:
        if id >= 0 and id < picks.size() and picks[id].is_valid():
            picks[id].call())

## The PARTY the stepper row was built with, as `HBoxContainer` meta — the `CREW_TARGET_COUNT_META`
## idiom, and needed for the same reason: the number lives in a child `Label` that
## `add_stepper_controls` places between two unlabelled buttons, so a harness has no `row.text` to
## read and a positional walk over the children would pass silently on the wrong node. This is the
## SETTLED count, since the row is built from whatever the caller clamped — which is what makes it
## the honest thing to compare a chart's `HarvestFloorChart.crew()` against.
const PARTY_STEPPER_COUNT_META := "party_stepper_count"

## The party stepper row, shared by both missions so they cannot drift apart in shape.
## `key_text` defaults to the word the three EXPEDITION sheets want. The split sheet passes its own,
## because a sheet whose whole thesis is *this is not a party* must not label its one input `Party` —
## the wrong word there teaches the wrong model more effectively than any amount of prose fixes.
static func build_party_stepper_row(count: int, party_max: int, on_change: Callable,
        key_text: String = HudComposeVocab.COMPOSE_FIELD_PARTY) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    row.set_meta(PARTY_STEPPER_COUNT_META, count)
    var key := Label.new()
    key.text = key_text
    key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(key)
    add_stepper_controls(row, count, count < party_max, on_change)
    return row

## The rung a policy-picker Button stands for, as `Button` meta. THE ONE STABLE HANDLE on a rung: the
## face is presentation and has already changed twice (glyph+metric → glyph+name over metric → that
## same pair as child Labels at two sizes), so a harness matching on `btn.text` breaks with every
## visual pass. `band_panel_preview._picker_rung_buttons` reads this.
const POLICY_RUNG_META := "policy"

## The compose sheet's QUARRY CHOOSER, as `MenuButton` meta — the control that appears only when a hex
## holds more than one eligible quarry. It needs a handle of its own because the parties zone builds a
## `⋯` `MenuButton` for its section menu too, so a node-type search finds both, and because the claim
## the harness makes is an ABSENCE with one candidate: a search that could match the wrong menu would
## report a chooser on a sheet that has none.
const QUARRY_CHOICES_META := "quarry_choices"

## The floor CHART, as `HarvestFloorChart` meta — the same stable-handle reasoning, and needed more
## than most: the chart carries no text at all, so a harness has nothing else to find it by. (It
## replaced `FLOOR_SLIDER_META`, whose plain `HSlider` the chart's draggable floor line supersedes.)
const FLOOR_CHART_META := "floor_chart"

## The two CREW TARGETS, as `Button` meta — value `CREW_TARGET_CLEAR` or `CREW_TARGET_HOLD`, so a
## harness can tell which target it found without matching a face that carries a live number.
const CREW_TARGET_META := "crew_target"
const CREW_TARGET_CLEAR := "clear"
const CREW_TARGET_HOLD := "hold"
## …and the COUNT it offers, as a second meta. The pill's face is a two-Label stack over an
## empty-`text` Button (see `_crew_target_pill`), so the number a harness has to read is not in
## `btn.text` and a `text.split(" ")[0]` on one finds nothing — silently, which is the failure the
## meta-as-handle idiom exists to prevent.
const CREW_TARGET_COUNT_META := "crew_target_count"

## The READOUT's yields row, as `Control` meta — the same stable-handle reasoning as the three above.
## Its face is a flow of Labels at three sizes carrying live numbers, so there is no single `text` to
## match and a needle search would find whichever Label happened to hold it.
const YIELDS_ROW_META := "yields_row"
## The READOUT's IMPROVEMENT-DEAL block, as `Control` meta — the labelled row stating what the rung
## on the table will pay once it stands.
##
## **IT IS A SIBLING OF THE YIELDS FLOW, NEVER A ROW INSIDE IT, and the meta is what lets a harness
## say so.** Two harness contracts read the flow structurally — `Readout.yields_header` finds it by
## `YIELDS_ROW_META` and takes `parent.get_child(index - 1)` as its caption, and both webs' take
## assertions parse `yields_text` by splitting on an account word — so a deal term folded into the
## flow would corrupt the caption AND put unparseable tokens in front of the numbers. Its own block,
## its own meta, its own reader.
const IMPROVEMENT_DEAL_META := "improvement_deal"
## The readout ASIDE block's identity. Its lines are plain Labels at one size and the teaching one
## carries live numbers, so a harness matching text would find whichever Label happened to hold the
## needle — or nothing, and pass.
const READOUT_ASIDE_META := "readout_aside"
## The TEACHING line's own identity inside that block. Its siblings (the idle note, the floor hint)
## also move with the floor, so an assertion that the ASIDE changed is satisfied by either of them
## and says nothing about this line — measured: blanking the teaching note entirely still passed a
## whole-aside comparison. A claim about this sentence has to be able to find this sentence.
const READOUT_TEACHING_META := "readout_teaching"
## The LOCKED-ACCOUNT line's own identity, for the same reason one line up: it leads the aside, and its
## siblings (the floor hint, the teaching line) move with the floor while this one does not, so
## "the aside changed" cannot testify about it in either direction. It explains a `—` the player is
## looking at, which is why it comes FIRST — the other two are standing copy.
const READOUT_LOCKED_ACCOUNT_META := "readout_locked_account"

## The CREW ROW's own label, as `Control` meta. It names the crew from the composed improvement axis
## (`Hunters` vs `Herders`), which is a real claim about the sheet — and it renders UPPERCASE, exactly
## like the sheet's eyebrow two rows above it, so a text search for the crew noun matches the eyebrow
## and passes without ever reaching the stepper.
const CREW_ROW_LABEL_META := "crew_row_label"

## The crew row's BUILD-DIP note, as `Control` meta — its own, because it must be assertable by
## ABSENCE as well as by presence ("no build in flight, so no dip is claimed"), and the row label
## beside it renders either way.
const CREW_ROW_DIP_META := "crew_row_dip"

## The VERDICT line, as `Control` meta — value is the severity (`SourceForecast.VERDICT_*`), which is
## the assertable half: the sentence carries turn counts and percentages that move with the fixture.
const VERDICT_META := "verdict"

## **THE PRE-LAUNCH FIGHT'S ONE REMAINING LINE** (`docs/plan_hunt_through_combat.md` §2.1 / §6.5), with
## a meta because it must be assertable by ABSENCE as well as by presence — a pen and the whole plant
## web render none, and that emptiness is the byte-identity claim this arc has to hold.
##
## **THE ENGAGEMENT FIGURE'S OWN META WENT WITH THE LINE** (`One hunter brings 10 Wild Fowl into
## contact.`): a species constant that never moved with anything the player was dialling. So did the
## gate's WINNABLE face (`0.1 hunter-turns to bring one down`) — the meta now only ever rides a
## refusal, and a winnable fight renders no line at all, which `Readout.HUNT_GATE_ABSENT` is the
## assertion for.
const HUNT_GATE_META := "hunt_gate"

## The "send a hunting expedition" CONFIRM button, as `Button` meta — set by BOTH hosts that build
## one (the herd drawer's compose control and the Band panel's parties compose sheet). Same reason as
## the rung meta above, only more so: this button's face is the raid VERDICT
## (`SourceForecast.style_send_hunt_button` writes "Send Expedition" / "Send Anyway (≈54
## turns)" / "Send (brings nothing home)" / "Herd too lean to raid"), so text is the one thing a
## harness cannot match on. `tools/command_guard.gd` presses it through this meta — it is the ONLY
## way to reach those two emit sites, whose payload-building lives in an inline `pressed` lambda.
const SEND_HUNT_CONFIRM_META := "send_hunt_confirm"

## The DENIAL raid's confirm button (`docs/plan_denial_raid.md`). **Its OWN meta, not the hunt one**,
## and for the reason the two forms are separate at all: a harness that pressed "the send button" on a
## parties compose sheet could not tell which MISSION it had just launched, and the two emit different
## signals with different, non-interchangeable payloads (a denial payload carries no floor, and its
## command grammar rejects one). Its face is the collapse verdict, so text is not matchable either.
const SEND_DENIAL_CONFIRM_META := "send_denial_confirm"

## A parties-footer MISSION LAUNCH button (`⚑ Scout` / `🏹 Hunt` / `💀 Deny`), as `Button` meta, carrying
## the MISSION key it opens the compose sheet on. It is the entry point to a composing act — the press
## a player makes and the only path that opens a sheet with nothing filled in — so a harness that
## cannot reach it can only ever stage the compose sheet by writing `_party_compose_open` directly,
## which is how the EMPTY form in a tall dock went uncovered through two reports of the same defect.
## Keyed on the mission rather than a bare `true` because all three buttons are built by one builder
## and their faces (which carry the mission glyph) are exactly what a harness must not match on.
const MISSION_LAUNCH_META := "mission_launch"

## A compose sheet's COMMIT button, as `Button` meta — set by both sheets' builders. Its face is the
## thing under test whenever the crew noun moves (`Forage` / `Tend` / `Unassign` on the plant web,
## `Hunt Here` / the raid verdict on the animal one), so a harness that found it BY text could only
## ever confirm the string it already assumed. Identity is the only stable handle.
const COMPOSE_COMMIT_META := "compose_commit"

## The improvement CONTROL's checkbox, as `Control` meta — the stable handle on the second axis, for
## the same reason `POLICY_RUNG_META` is the stable handle on a rung. Its value is the IMPROVEMENT key
## (`"cultivate"` / `"sow"` / `"tame"` / `"corral"`), so a harness can assert both which rung is
## offered and, from the node's own type, which of the three states it is in: a `CheckBox` is offered
## or running (`button_pressed` tells them apart) and a `Label` is done.
const IMPROVEMENT_CONTROL_META := "improvement"

## **THE IMPROVEMENT CONTROL** (issue #442) — the second axis's whole widget, in the ONE of four
## states the caller resolved, plus its trailing notes.
##
## `state` is `IMPROVEMENT_STATE_*`. The face and the tooltip are the CALLER's words (it knows the
## rung, its payoff, its meter and — when gated — its reason); what lives here is the SHAPE the four
## states share, so the two webs cannot drift into two different-looking controls:
##
##   OFFERED — an unchecked, enabled `CheckBox` naming the rung and its terms.
##   GATED   — a **`Label`, not a disabled checkbox**, whose text IS the reason the caller resolved;
##             the tooltip is the rung's hint, NOT the reasons. See the const's own note for why the
##             greyed-checkbox form was retired.
##   RUNNING — a checked, **LIVE** `CheckBox`: unchecking abandons the build (`abandon_improvement`).
##   DONE    — a plain `Label`. Nothing to uncheck, nothing to clear.
##
## **UNCHECKING IS NEVER GATED, AND THAT IS LOAD-BEARING.** The abandon path asks for no knowledge, no
## ceiling, no site and pointedly no `Thriving` check, because abandoning a STALLED build is exactly
## when a player reaches for it — so `notes` on a RUNNING control (the WARN pause line) must NOT
## disable it, which is why the `disabled` rule below tests the state and not just the notes. A
## condition that greys a running box is a bug, not a safeguard.
##
## `on_toggle` is called with **the improvement's NEW value** — the rung's key when a box is checked,
## `IMPROVEMENT_NONE` when one is unchecked — so a caller writes the value it is given rather than
## re-deriving the direction from the control's state.
##
## `notes` render beneath in the hint style — the WARN-amber pause line when running (`warn_notes`
## picks the tint), and on a GATED control the SECOND and later gate reasons, the first having become
## the control's own text. An OFFERED control passes none: an offer with an unmet prerequisite is a
## GATED one. Returns the whole block, so a caller adds one child.
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
        locked.add_theme_font_size_override("font_size",
            HudWorkVocab.POLICY_PICKER_NAME_FONT_SIZE)
        locked.set_meta(IMPROVEMENT_CONTROL_META, improvement)
        set_label_tooltip(locked, tooltip)
        block.add_child(locked)
    elif state == IMPROVEMENT_STATE_DONE:
        # A state, not a choice. `set_label_tooltip` because a bare `tooltip_text` on a Label is a
        # SILENT no-op (Labels default to MOUSE_FILTER_IGNORE).
        var done := Label.new()
        done.text = face
        # **THE SAME SIZE THE STANCE RUNGS' NAMES WEAR** (`POLICY_PICKER_NAME_FONT_SIZE`). The two are
        # peer AXES, and rendering the second one at hint size made the whole decision read as a
        # footnote to the first. It used to say this by carrying NO override at all, which was true
        # only while the rung name carried none either; that stopped being true when the preset face
        # was stepped down, and the tie is written out now rather than implied. The gate/pause notes
        # below it stay at hint size, where they belong.
        done.add_theme_font_size_override("font_size",
            HudWorkVocab.POLICY_PICKER_NAME_FONT_SIZE)
        done.add_theme_color_override("font_color", HudStyle.HEALTHY)
        done.set_meta(IMPROVEMENT_CONTROL_META, improvement)
        set_label_tooltip(done, tooltip)
        block.add_child(done)
    else:
        var box := CheckBox.new()
        box.text = face
        box.tooltip_text = tooltip
        box.add_theme_font_size_override("font_size", HudWorkVocab.POLICY_PICKER_NAME_FONT_SIZE)
        # **WITHOUT THIS THE BOX IS NOT THERE.** The stock CheckBox art is drawn for a light surface,
        # so on this console the unchecked indicator reserves its width and paints nothing — an offer
        # with no control on it. `HudStyle.apply_checkbox` has the whole autopsy.
        HudStyle.apply_checkbox(box)
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
    stack.add_child(_policy_rung_line(title, tint, HudWorkVocab.POLICY_PICKER_NAME_FONT_SIZE))
    if metric != "":
        stack.add_child(_policy_rung_line(metric,
            Color(tint, tint.a * HudWorkVocab.POLICY_PICKER_METRIC_ALPHA),
            HudWorkVocab.POLICY_PICKER_METRIC_FONT_SIZE))
    pad.add_child(stack)
    cell.add_child(pad)
    return cell

## One line of a rung's face, at the size its row of the type scale carries. Line 1 used to pass 0 —
## "leave the theme's own size" — which is how a preset came to be rendered at the panel's largest
## type; both lines are explicit now (`POLICY_PICKER_NAME_FONT_SIZE` / `_METRIC_FONT_SIZE`).
static func _policy_rung_line(text: String, tint: Color, font_size: int) -> Label:
    var label := Label.new()
    label.text = text
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    label.add_theme_color_override("font_color", tint)
    label.add_theme_font_size_override("font_size", font_size)
    return label

## **THE FLOOR CONTROL** — the three intent PRESETS as a radio over `on_pick(floor: float)`, with the
## dialled value itself highlighted when it sits on one of them and none highlighted when it does not.
## It replaced the harvest-stance radio: there are no stances, so what a button can offer is a
## shortcut to a value, and the value is what travels.
##
## **A PRESET THE PLAYER IS NOT ON MUST NOT LIGHT UP.** `SourceForecast.floor_preset_for` answers `""`
## for anything between two presets — which is most of the dial once the slider is used — and lighting
## the nearest one instead would state a floor the crew is not holding.
##
## `takes` is keyed by PRESET (`SourceForecast.FLOOR_PRESET_*`), each a `{compact, full}` pair, and may
## carry a THIRD key — **`note`**, a caveat on the metric appended under the tooltip's name + metric
## line. It exists for the hunt sheet's averaging-window disclaimer
## (`HudComposeVocab.HUNT_AVG_WINDOW_FORMAT`), which qualifies the rate on the very face it hangs off,
## and which as a standing body line made the hunt sheet read a paragraph longer than the forage sheet
## beside it. A picker whose takes carry no `note` (forage, expedition) is unchanged.
##
## **NO PRESET IS EVER GATED** — a floor has no prerequisite and never retires, exactly as a stance
## did not since #442. Unmet prerequisites belong to the IMPROVEMENT axis, and
## `build_improvement_control` renders them.
static func build_floor_picker(
    on_pick: Callable,
    selected_floor: float,
    takes: Dictionary = {},
    columns: int = 0) -> VBoxContainer:
    var current_preset := SourceForecast.floor_preset_for(selected_floor)
    var block := VBoxContainer.new()
    block.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    # Wrap the preset buttons at most `POLICY_PICKER_COLUMNS` (3) per row (a GridContainer). Three
    # presets fit one row exactly; the ceiling stays uniform so a zone-hosted picker (which clamps
    # DOWN via `columns`) reads as the same creature as the compose sheet's.
    var grid := GridContainer.new()
    # `columns > 0` CLAMPS the default DOWN, never up: a zone is a FIXED-width box, and a picker whose
    # buttons sum past it raises the zone content's minimum width, which pushes the whole zone column
    # out past its host (where it is clipped) — taking the section menu beside it off the edge.
    var wanted := columns if columns > HudWorkVocab.POLICY_PICKER_AUTO_COLUMNS \
        else SourceForecast.FLOOR_PRESETS.size()
    grid.columns = clampi(wanted, 1, HudWorkVocab.POLICY_PICKER_COLUMNS)
    grid.add_theme_constant_override("h_separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    grid.add_theme_constant_override("v_separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    for preset_variant in SourceForecast.FLOOR_PRESETS:
        var preset := String(preset_variant)
        var floor_value := SourceForecast.floor_for_preset(preset)
        var btn := Button.new()
        # **A ONE-LINE FACE: WHICH INTENT, AND NO NUMBER** — `HudFormat.floor_preset_face`, the zone
        # glyph welded to the preset's label (`♻ Best harvest`). The same glyph the map's yield labels
        # append, so a floor reads identically on the picker and on the worked tile/herd.
        #
        # **THE METRIC CAME OFF THE FACE AND KEPT ITS TOOLTIP.** Nine numbers stood across the top of
        # the sheet and every one of them misled:
        #   • they are the ROOM above that floor — takeable ONCE — while every number below them is a
        #     per-turn rate, and nothing on the face said which kind it was;
        #   • they rank the presets BACKWARDS from the decision they annotate. `Take everything` reads
        #     twice `Best harvest` because it frees twice the standing stock, while in the long run it
        #     pays ~nothing and `Best harvest` pays the peak forever;
        #   • they are in FOOD/FODDER units directly above a chart whose axis is BIOMASS, with
        #     nothing relating the two;
        #   • and they are worker-independent, so they alone sit still while the whole sheet under
        #     them moves with the stepper.
        # The readout below answers all of it — crew-aware, per-turn, and now stating the burst and
        # the steady rate both — so the face is left saying what the button is FOR. `full` still rides
        # the tooltip for anyone who wants the magnitude without clicking through.
        var take: Variant = takes.get(preset, null)
        var full := String((take as Dictionary).get("full", "")) if take is Dictionary else ""
        var note := String((take as Dictionary).get("note", "")) if take is Dictionary else ""
        var is_selected := preset == current_preset
        var variant := "primary" if is_selected else "ghost"
        # The PRESET key as meta, not the face string: the face is presentation (glyph, label, a
        # second line), so a harness identifying a button by `btn.text` would read an empty string.
        btn.set_meta(POLICY_RUNG_META, preset)
        HudStyle.apply_button(btn, variant)
        # Tooltip carries the VERBOSE metric the face compacts, led by the preset's own label and the
        # NUMBER it stands for — the one place the two spellings of a floor sit together, so a player
        # can learn that "Best harvest" is 50% left standing without dragging the slider to find out.
        # **THE TOOLTIP CARRIES THE LONG FORM THE FACE SHORTENED.** The faces are one word each so
        # three presets fit a 354px dock column; the phrase each stands for is the first thing this
        # tooltip says, beside the number, so nothing the shortening took is unreachable.
        var preset_name := "%s (%s)" % [
            HudComposeVocab.FLOOR_PRESET_LONG_LABELS.get(preset,
                HudComposeVocab.FLOOR_PRESET_LABELS.get(preset, preset)),
            HudComposeVocab.FLOOR_VALUE_FORMAT % SourceForecast.floor_percent(floor_value)]
        var name_line := HudComposeVocab.POLICY_TOOLTIP_NAME_FORMAT % [preset_name, full] \
            if full != "" else preset_name
        btn.pressed.connect(func() -> void: on_pick.call(floor_value))
        btn.tooltip_text = HudFormat.join_tooltip_lines([name_line, note])
        # EXPAND_FILL on the CELL (which is what the grid lays out now), so the presets sharing a row
        # are equal width and fill the panel content width.
        # **A PRESET AT REST IS QUIETER THAN A BUTTON AT REST.** `button_font_color` answers `INK` for
        # a ghost, which is the right weight for an ACTION; a preset is a shortcut to a value sitting
        # above the chart that is the real control, so an unpicked one reads `INK_DIM`. The selected
        # and disabled answers are the shared table's, unchanged — this steps only the resting one.
        var tint := HudStyle.button_font_color(variant, btn.disabled)
        if not is_selected and not btn.disabled:
            tint = HudStyle.INK_DIM
        var cell := _policy_rung_cell(btn, HudFormat.floor_preset_face(preset), "", tint)
        cell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        grid.add_child(cell)
    block.add_child(grid)
    return block

## **THE DIAL BETWEEN THE PRESETS** — a whole-percent slider over the same floor the presets set, so
## every value in `0..1` is reachable and the three buttons stay shortcuts rather than the whole axis.
## `on_change` fires with the new floor.
##
## It is deliberately PLAIN: 4b replaces it with the chart's draggable floor, and a bespoke control
## built here would be thrown away. `FLOOR_STEP` is the granularity — fine enough to sit anywhere
## between two presets, coarse enough that the value is readable and reproducible.
##
## **THE HEADER NAMES THE AXIS AND STATES NO VALUE**, so it never has to be refreshed on a drag. The
## chart owns the readback: it is `FOCUS_ALL`, handles its own arrow keys, and redraws its floor flag
## from the model, which puts the number on the control being moved instead of a line above it. A
## header that repeated the flag was the same fact twice — the caption did that while the control was
## a plain slider with no readout of its own, and outlived it. It stays a header rather than nothing
## because every other block in the sheet has one and the chart's vertical axis is otherwise unnamed.
static func build_floor_chart(model: Dictionary, on_change: Callable) -> VBoxContainer:
    var block := VBoxContainer.new()
    block.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    block.add_child(alloc_section_label(HudComposeVocab.FLOOR_CONTROL_LABEL))
    var chart := HarvestFloorChart.new()
    chart.set_meta(FLOOR_CHART_META, true)
    chart.set_model(model)
    # **TWO ARGUMENTS, AND THE SECOND ONE IS THE WHOLE CONTRACT.** A committed change rebuilds the
    # compose controls, which frees this node; a live one must not, or the drag in flight dies with
    # it. The caller decides what "update in place" means for its own sheet.
    chart.floor_changed.connect(func(value: float, committed: bool) -> void:
        on_change.call(value, committed))
    block.add_child(chart)
    return block

## **THE EXPEDITION'S READOUT — the same box, the same three registers, a different question.** The
## branch used to answer with one wrapped bbcode sentence carrying every fact at once (the animals,
## the turns, the split, the food and the waste), beside a local sheet that laid the same kinds of
## fact out in a bounded well. Two sheets on one panel, reading nothing alike.
##
## What must NOT carry over is the local readout's PER-TURN framing: the header
## (`EXPEDITION_TRIP_ROW_HEADER`), the absent `now → after` on every row, and a verdict about the
## trip's length rather than about which of the crew and the floor binds — all three because a raid
## is one bounded errand, not a rate a resident crew settles into.
##
## Only a DELIVERING trip reaches here (`SourceForecast.hunt_trip_delivers`); the refused states keep
## their sentence, an empty box being worse than the line it replaced.
##
## **IT LIVES IN THE SHARED WIDGET LAYER BECAUSE TWO CONTROLLERS RENDER IT.** It was private to
## `DrawerComposeController` while the Band panel's dock sheet answered the same question with a
## one-line bbcode sentence — and the two drifted, as a copied control always does: on a Wild Fowl
## flock the drawer laid out a full box and the dock rendered NOTHING. Both sheets call this now, so
## the raid has one readout. Everything it needs (`trip`, the quarry's name, the composed floor)
## arrives as a PARAMETER — no controller state, which is what let it move at all.
static func mount_trip_readout(parent: VBoxContainer, trip: Dictionary, quarry: String,
        floor_value: float) -> void:
    var column := build_readout_box(parent)
    # The waste rides the yields row's own `waste` slot, exactly as the local hunt's does — a kill the
    # party could not haul is the animal web's concern on both branches, and it is amber either way.
    var waste_pct := float(trip.get("waste_pct", 0.0))
    column.add_child(build_yields_row(
        _trip_yield_rows(trip, quarry),
        HudStyle.INK,
        "",
        HudStyle.HEALTHY,
        SourceForecast.HUNT_WASTE_NOTE_FORMAT % int(round(waste_pct * 100.0)) \
            if waste_pct > 0.0 else "",
        SourceForecast.EXPEDITION_TRIP_ROW_HEADER))
    column.add_child(build_verdict_line(SourceForecast.hunt_trip_verdict(trip)))
    # THE ASIDE IS THE FLOOR HINT AND NOTHING ELSE. The local readout's other line — the live teaching
    # rate — has no counterpart here: an expedition accrues no husbandry (the gap
    # `FLOOR_LEARNING_HINT_EXPEDITION` already names in the learning zone), so a teaching line would
    # quote a multiplier this party never earns. A zone with nothing to say renders no aside at all,
    # rather than a dashed rule over empty space.
    # The COMPOSED floor, not the estimate row's nearest sample: the hint explains the preset the
    # player is holding, and the sampling is a fact about the forecast table rather than about them.
    var hint := HudFormat.floor_hint(floor_value, SourceForecast.LABOR_KIND_HUNT, true)
    if hint != "":
        column.add_child(build_readout_aside(
            [readout_aside_line(hint)]))
## The trip's payload as yields rows: the ANIMALS the party brings back, then whatever accounts those
## bodies pay.
##
## **THE ANIMAL COUNT LEADS, IN THE LOCAL HUNT ROW'S OWN IDIOM** — its `YIELD_ROW_NUMBER` /
## `YIELD_ROW_UNIT` overrides, the quarry as the unit and `YIELD_ACCOUNT_NONE` as the account,
## because a body is not an account. It borrows the `≈` FACE vocabulary and deliberately not the
## `/turn` UNIT one: this is a whole-trip count, and the header above already says so.
##
## The food row goes through `SourceForecast.yield_rows`, so the render-only-where-the-vector-pays
## rule keeps one definition. `YIELD_ACCOUNT_NONE` as the zero account means NO row is synthesised
## when it is empty; that state cannot arrive here anyway (it is `empty`, and the caller took the
## sentence branch), so a fabricated zero would be a reading of nothing. (A trade row rode beside the
## food one until arc #527 retired that account; the MATERIAL rows are what replaced it, and on an
## inedible quarry they are the only rows under the animal count.)
##
## No `after` on any row: a trip has no holding state to arrow toward.
static func _trip_yield_rows(trip: Dictionary, quarry: String) -> Array[Dictionary]:
    var animals := int(trip.get("animals", 0))
    var rows: Array[Dictionary] = [{
        SourceForecast.YIELD_ROW_ACCOUNT: SourceForecast.YIELD_ACCOUNT_NONE,
        SourceForecast.YIELD_ROW_VALUE: float(animals),
        YIELD_ROW_NUMBER: HudComposeVocab.HUNT_ANIMAL_RATE_FACE_FORMAT % animals,
        YIELD_ROW_UNIT: quarry,
    }]
    rows.append_array(SourceForecast.yield_rows(
        float(trip.get("food", 0.0)), 0.0, SourceForecast.YIELD_ACCOUNT_NONE, {},
        trip.get(SourceForecast.TRIP_DELIVERED_MATERIAL_KEY, [])))
    return rows

## **THE TWO CREW TARGETS** (`docs/plan_harvest_floor.md` §7.6) — the distinction the rate model never
## had. A floor and a crew are independent statements, so there are two different worker numbers and
## the player is owed both:
##
##   • ***clear it now*** — the hands that take everything above the floor in one turn;
##   • ***hold it after*** — the hands that take exactly what grows back, once it is there.
##
## Both are exact, both are stated, and each is a TARGET you can click to staff. Neither is a hidden
## rule the player has to infer from a stepper going dead. A target the wire cannot price
## (`NO_CREW_ANSWER` — a dead-season patch has no throughput to divide by) is not rendered at all,
## rather than shown as a zero that would read as "nobody is needed".
## **THEY ARE PILLS ON THE CREW'S OWN LINE, not full-width boxes on a row of their own.** A target is
## a VALUE you can jump to, while the stepper beside it is a control you operate, and the two shapes
## have to say which is which: two boxed buttons spanning the panel read as the primary action of the
## whole sheet, which is the Assign button's job. `CREW_TARGET_COUNT_META` carries the count, because
## the face is two Labels over an empty-`text` Button (a count and its label at one size are one
## undifferentiated phrase) and `btn.text` is therefore empty.
static func build_crew_targets(model: Dictionary, workers: int, on_pick: Callable) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudComposeVocab.CREW_ROW_SEPARATION)
    for spec in [
        [CREW_TARGET_CLEAR, int(model.get("crew_to_clear", SourceForecast.NO_CREW_ANSWER)),
            HudComposeVocab.CREW_TARGET_CLEAR_LABEL, HudComposeVocab.CREW_TARGET_CLEAR_TOOLTIP],
        [CREW_TARGET_HOLD, int(model.get("crew_to_hold", SourceForecast.NO_CREW_ANSWER)),
            HudComposeVocab.CREW_TARGET_HOLD_LABEL, HudComposeVocab.CREW_TARGET_HOLD_TOOLTIP],
    ]:
        var count := int(spec[1])
        if count == SourceForecast.NO_CREW_ANSWER:
            continue
        var btn := Button.new()
        btn.tooltip_text = String(spec[3])
        btn.set_meta(CREW_TARGET_META, String(spec[0]))
        btn.set_meta(CREW_TARGET_COUNT_META, count)
        # The target the crew is ALREADY on wears the selected fill, so the two numbers double as a
        # readout of where the current staffing sits between them.
        var selected := workers == count
        HudStyle.apply_pill_button(btn, selected)
        btn.pressed.connect(func() -> void: on_pick.call(count))
        # The tint is the SHARED TABLE's answer for this button's own state — `btn.disabled` included,
        # exactly as `build_floor_picker` asks it. `apply_pill_button` already writes a `disabled`
        # stylebox, so the box can fade; a face built from child Labels cannot follow it through the
        # theme (see `_crew_target_pill`), so the state has to reach the tint here or the box would
        # fade under two lines still at full brightness.
        row.add_child(_crew_target_pill(btn, count, String(spec[2]),
            HudStyle.button_font_color("primary" if selected else "ghost", btn.disabled)))
    return row

## **THE CREW ROW'S BUILD-DIP NOTE** — *"— while building, each carries 25% as much"*, the one line
## that makes the two targets beside it arithmetic rather than magic. `dip` is
## `SourceForecast.floor_chart_model`'s own `build_dip`, so the note and the targets are divided by
## one number by construction; `null` (no note at all) at the identity, because a crew that is only
## gathering carries a full load and saying so would be noise on every non-building sheet.
##
## It wears the row label's exact treatment — `INK_FAINT` at the section-label size — but is NOT
## uppercased: it is a sentence about the row, not a second row-label.
static func build_crew_dip_note(dip: float) -> Label:
    if dip >= SourceForecast.NO_BUILD_DIP:
        return null
    var note := Label.new()
    note.text = HudComposeVocab.CREW_BUILD_DIP_NOTE_FORMAT % HudFormat.progress_percent(dip)
    note.set_meta(CREW_ROW_DIP_META, true)
    note.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    note.add_theme_font_size_override("font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
    return note

## ONE crew-target pill: an empty-`text` Button under a two-Label face, the horizontal twin of
## `_policy_rung_cell` and for the same structural reason — two font sizes cannot live in one
## `Button.text`. The COUNT leads at the pill's own size in full ink (it is what the player compares
## against the stepper); the label naming which answer it is follows, one step down and one step
## quieter. Both tints derive from the ONE colour the caller resolved, so a selected pill moves as a
## unit; never give the label a colour of its own.
static func _crew_target_pill(btn: Button, count: int, label_text: String,
        tint: Color) -> MarginContainer:
    var cell := MarginContainer.new()
    cell.size_flags_vertical = Control.SIZE_SHRINK_CENTER
    cell.add_child(btn)
    var pad := MarginContainer.new()
    pad.mouse_filter = Control.MOUSE_FILTER_IGNORE
    pad.add_theme_constant_override("margin_left", HudStyle.PILL_PADDING_H)
    pad.add_theme_constant_override("margin_right", HudStyle.PILL_PADDING_H)
    pad.add_theme_constant_override("margin_top", HudStyle.PILL_PADDING_V)
    pad.add_theme_constant_override("margin_bottom", HudStyle.PILL_PADDING_V)
    var face := HBoxContainer.new()
    face.mouse_filter = Control.MOUSE_FILTER_IGNORE
    face.add_theme_constant_override("separation", HudComposeVocab.CREW_TARGET_FACE_SEPARATION)
    face.add_child(_pill_face_line(str(count), tint,
        HudComposeVocab.CREW_TARGET_COUNT_FONT_SIZE))
    face.add_child(_pill_face_line(label_text,
        Color(tint, tint.a * HudWorkVocab.POLICY_PICKER_METRIC_ALPHA),
        HudComposeVocab.CREW_TARGET_LABEL_FONT_SIZE))
    pad.add_child(face)
    cell.add_child(pad)
    return cell

static func _pill_face_line(text: String, tint: Color, font_size: int) -> Label:
    var label := Label.new()
    label.text = text
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
    label.add_theme_color_override("font_color", tint)
    label.add_theme_font_size_override("font_size", font_size)
    return label

## Severity → the dot and the text tint, in the raid verdict's own ok/slow/blocked vocabulary. Kept
## beside the widget rather than on `SourceForecast`, which states the verdict and owns no palette.
const VERDICT_SEVERITY_COLORS := {
    SourceForecast.VERDICT_OK: HudStyle.HEALTHY,
    SourceForecast.VERDICT_SLOW: HudStyle.WARN,
    SourceForecast.VERDICT_BLOCKED: HudStyle.DANGER,
}
## The dot leading the verdict — the severity as a mark, so the state is readable before the sentence.
const VERDICT_DOT := "●"
const VERDICT_DOT_FONT_SIZE := 9

## **THE VERDICT LINE** (§7.1) — which of the two statements is BINDING, the crew or the floor. It is
## the sentence the whole redesign exists to make sayable: the four-stance picker let a player select
## Eradicate with one worker and never eradicate anything, because nothing compared the intent with
## the hands. `verdict` is `SourceForecast.harvest_verdict`'s `{severity, text}`.
static func build_verdict_line(verdict: Dictionary) -> HBoxContainer:
    var severity := String(verdict.get("severity", SourceForecast.VERDICT_OK))
    var tint: Color = VERDICT_SEVERITY_COLORS.get(severity, HudStyle.INK_DIM)
    var row := HBoxContainer.new()
    row.set_meta(VERDICT_META, severity)
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    var dot := Label.new()
    dot.text = VERDICT_DOT
    dot.vertical_alignment = VERTICAL_ALIGNMENT_TOP
    dot.add_theme_color_override("font_color", tint)
    dot.add_theme_font_size_override("font_size", VERDICT_DOT_FONT_SIZE)
    row.add_child(dot)
    var text := Label.new()
    text.text = String(verdict.get("text", ""))
    text.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    text.add_theme_color_override("font_color", tint)
    text.add_theme_font_size_override("font_size", HudComposeVocab.READOUT_VERDICT_FONT_SIZE)
    row.add_child(text)
    return row

## The READOUT's own well — the bordered box the yields row, the verdict and the aside share. Returns
## the `PanelContainer`'s inner column, so a caller adds registers to it and never sees the chrome.
static func build_readout_box(parent: Container) -> VBoxContainer:
    var box := PanelContainer.new()
    box.add_theme_stylebox_override("panel", HudStyle.readout_stylebox())
    box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var column := VBoxContainer.new()
    column.add_theme_constant_override("separation", HudComposeVocab.READOUT_SEPARATION)
    box.add_child(column)
    parent.add_child(box)
    return column

## **THE YIELDS ROW** — the readout's first and loudest register, because it is the ANSWER: what this
## crew, at this floor, brings home per turn. Each entry of `rows` is a
## `SourceForecast.yield_rows`-shaped `{account, value}`, rendered as a big tabular NUMBER beside a
## small uppercase UNIT carrying the account's destination (`2.34  FOOD/TURN → CAMP`) — the routing
## suffix one step quieter still, because it is part of the unit rather than a third fact.
##
## **A ROW EXISTS ONLY WHERE THE VECTOR PAYS.** The array is `yield_rows`' answer verbatim, so a cash
## crop has no food row and a wolf has none either — `provisionsPerBiomass` is genuinely `0` on both,
## which makes `0.00 food` a FALSE reading rather than an empty one. Never synthesise a row here.
##
## `note` is the take's own qualifier (`· renewable`, or the overdraw sentence) and `waste` the
## whole-animal line where one applies; both sit in the row's own flow at the unit's size in the tint
## the caller resolved, so a warning never has to compete with the number it is warning about.
##
## **THE HEADER CARRIES THE UNIT AND THE ARROW'S KEY, so neither is repeated per account.** It is a
## `VBoxContainer` now rather than the bare flow — the flow keeps `YIELDS_ROW_META`, so everything
## that reaches for the row by identity still finds the readings and not the caption over them.
##
## `header` OVERRIDES that caption for a caller whose readings are not a per-turn rate at all — the
## raid's whole-trip payload, which has no `/turn` and no holding state to arrow toward.
##
## `while_building` is the OTHER key a per-turn caption can carry: these readings are the DIPPED
## take. It is a FLAG rather than a second header string, and that is the whole point — the caption is
## resolved in ONE place (`SourceForecast.yield_row_header`) over that flag and the `has_after` this
## function is the only place that knows, so the caption and the marks under it cannot be composed
## separately. A caller that composed its own caption did exactly that once: the `while building`
## string replaced `now → after` and left the row's arrow unkeyed.
##
## **A BUILDING ROW CARRIES NO ARROW TO KEY.** The floor walk is suppressed at the yield model while a
## build is composed, so `has_after` is false whenever `while_building` is true and the resolver has
## three states rather than four. `has_after` is still read here rather than assumed, because it is
## the ROWS that decide it and a widget that inferred it from the flag would be a second opinion.
static func build_yields_row(rows: Array, number_tint: Color, note: String, note_tint: Color,
        waste: String, header: String = "", while_building: bool = false) -> VBoxContainer:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", HudComposeVocab.READOUT_YIELD_V_SEPARATION)
    var has_after := rows.any(func(row: Dictionary) -> bool:
        return row.has(SourceForecast.YIELD_ROW_AFTER))
    var caption := header if header != "" \
        else SourceForecast.yield_row_header(while_building, has_after)
    block.add_child(alloc_section_label(caption))
    var flow := HFlowContainer.new()
    flow.set_meta(YIELDS_ROW_META, true)
    flow.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    flow.add_theme_constant_override("h_separation", HudComposeVocab.READOUT_YIELD_H_SEPARATION)
    flow.add_theme_constant_override("v_separation", HudComposeVocab.READOUT_YIELD_V_SEPARATION)
    for row in rows:
        flow.add_child(_yield_reading(row, number_tint))
    if note != "":
        flow.add_child(_readout_unit_label(note, note_tint))
    if waste != "":
        flow.add_child(_readout_unit_label(waste, HudStyle.WARN))
    block.add_child(flow)
    return block

## One account's reading: the number, then its unit. `unit` is the CALLER's none-of-my-business — a
## raid's trip payload leads with the BODIES it brings back, stated in the quarry's own name rather
## than in an account's — so a row may carry it as its own override of the account table. **A
## per-turn RATE is not such a caller**: what a turn of work pays is an account, so both webs' live
## readouts take the table's unit and the whole-animal reading is the chart's above them.
const YIELD_ROW_UNIT := "unit"
const YIELD_ROW_NUMBER := "number"
## **ONE ACCOUNT'S NUMBER, MUTED, WHATEVER THE ROW TINT IS.** The tint passed to `build_yields_row` is
## a WHOLE-ROW parameter — it says how the TAKE reads (amber when it overdraws, ink otherwise) — and
## one account being unbankable is not a property of the take. So a row may opt its number out into
## `INK_FAINT` on its own, which is what lets the locked fodder reading sit beside two live ones in an
## overdrawing row without either claim contradicting the other.
const YIELD_ROW_MUTED := "muted"
static func _yield_reading(row: Dictionary, number_tint: Color) -> HBoxContainer:
    var account := String(row.get(SourceForecast.YIELD_ROW_ACCOUNT, ""))
    var pair := HBoxContainer.new()
    pair.add_theme_constant_override("separation", HudComposeVocab.READOUT_YIELD_PART_SEPARATION)
    var number := Label.new()
    # **THE TRANSITION RIDES THE NUMBER LABEL, at the number's own size.** The `after` rate is the one
    # a long-run decision turns on, so it is not demoted to the unit's small print; and a separate
    # Label would let an account's two halves wrap apart onto different lines, which is the one thing
    # this reading cannot survive.
    #
    # **A CALLER THAT SUPPLIES ITS OWN FACE OWNS ALL OF IT**, transition included — the raid payload's
    # animal count is a composed `≈8`, not a magnitude this widget formats, so composing an arrow here
    # would set a raw float beside an `≈`-prefixed one AND append a second arrow to a face that
    # already has one. Such a row still declares `YIELD_ROW_AFTER`, so the header knows to key it.
    # (The trip states no transition at all, so nothing currently exercises that pairing; the rule
    # holds for the next composed face rather than describing a live one.)
    if row.has(YIELD_ROW_NUMBER):
        number.text = String(row[YIELD_ROW_NUMBER])
    elif row.has(SourceForecast.YIELD_ROW_AFTER):
        number.text = SourceForecast.YIELD_AFTER_FORMAT % [
            SourceForecast.format_magnitude(float(row.get(SourceForecast.YIELD_ROW_VALUE, 0.0))),
            SourceForecast.format_magnitude(float(row[SourceForecast.YIELD_ROW_AFTER]))]
    else:
        number.text = SourceForecast.format_magnitude(
            float(row.get(SourceForecast.YIELD_ROW_VALUE, 0.0)))
    number.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
    number.add_theme_color_override("font_color",
        HudStyle.INK_FAINT if bool(row.get(YIELD_ROW_MUTED, false)) else number_tint)
    number.add_theme_font_size_override("font_size",
        HudComposeVocab.READOUT_YIELD_NUMBER_FONT_SIZE)
    pair.add_child(number)
    # **AN ACCOUNT WITH NO TABLE ENTRY IS A MATERIAL, AND ITS UNIT IS ITS OWN NAME** (arc #527
    # follow-up). `yield_rows` puts a material's id in the account slot precisely because the
    # material names itself — the catalogue ships no display name, so the id IS the display word —
    # and defaulting to `""` here printed a bare `0.22` with nothing saying what it was.
    var unit := String(row.get(YIELD_ROW_UNIT,
        SourceForecast.YIELD_ACCOUNT_UNITS.get(account, account)))
    pair.add_child(_readout_unit_label(unit, HudStyle.INK_FAINT))
    return pair

## The readout's small-print Label — the unit, the route, the take's qualifier and the waste line all
## share one size, because they are all annotations on the number beside them.
static func _readout_unit_label(text: String, tint: Color) -> Label:
    var label := Label.new()
    label.text = text.to_upper()
    label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
    label.add_theme_color_override("font_color", tint)
    label.add_theme_font_size_override("font_size", HudComposeVocab.READOUT_YIELD_UNIT_FONT_SIZE)
    return label

## The row's two halves as a dict, so a caller answers "the row, or none" with ONE value (`{}`) and
## `_mount_readout` has no second flag to keep in step with it.
const IMPROVEMENT_DEAL_ROW_LABEL := "label"
const IMPROVEMENT_DEAL_ROW_VALUE := "value"

## **THE IMPROVEMENT DEAL** — the readout's register between the take and the verdict, and it is
## exactly ONE labelled row: what the rung on the table will pay once it stands
## (`ONCE TENDED  1.39 food · 0.38 fodder`).
##
## The key wears the readout's own small-print uppercase — the same `_readout_unit_label` treatment
## the yields row gives an ACCOUNT, because a term of the deal is read exactly as an account's unit
## is: the word tells you what the number beside it is. The value takes the verdict's size, which is
## the register this block belongs to (louder than the aside it sits above, quieter than the take it
## explains) and `SIGNAL`, this HUD's word for a live promise everywhere else.
##
## **IT TAKES A ROW, NOT A LIST OF THEM, AND THAT IS THE SHAPE THE BLOCK IS ALLOWED.** It shipped
## briefly as an array API over a `{label, value, tint}` triple, for a second row stating the
## undipped take — see `labor-ui.md` for why that row went. An array parameter with one possible
## length, and a tint parameter with one possible value, are both the unused-API liability this repo
## already refuses; the block is a single row, so the signature says so.
static func build_improvement_deal(label: String, value: String) -> VBoxContainer:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", HudComposeVocab.READOUT_YIELD_V_SEPARATION)
    block.set_meta(IMPROVEMENT_DEAL_META, true)
    var line := HBoxContainer.new()
    line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    line.add_theme_constant_override("separation", HudComposeVocab.READOUT_YIELD_PART_SEPARATION)
    line.add_child(_readout_unit_label(label, HudStyle.INK_FAINT))
    var reading := Label.new()
    reading.text = value
    reading.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
    reading.add_theme_color_override("font_color", HudStyle.SIGNAL)
    reading.add_theme_font_size_override("font_size",
        HudComposeVocab.READOUT_DEAL_VALUE_FONT_SIZE)
    line.add_child(reading)
    block.add_child(line)
    return block

## ONE line of the aside, as the `{text, color}` pair `build_readout_aside` renders. The ink is a
## PARAMETER rather than a per-line branch inside the builder because only the CALLER knows whether a
## line is a standing note or a live state: the aside's own colour is `INK_FAINT`, and the one line
## that departs from it — the teaching RATE, which exists only while the crew is actually earning it —
## wears `SIGNAL`, this HUD's word for a live state everywhere else (the Sight chip, the selection
## accent, the turn orb's calm pulse).
static func readout_aside_line(text: String, color: Color = HudStyle.INK_FAINT,
        meta: String = "") -> Dictionary:
    return {"text": text, "color": color, "meta": meta}

## The readout's ASIDE register — the quietest thing on the sheet, cut off from the verdict above it
## by a DASHED rule. A solid hairline is a division between two blocks of equal standing (that is what
## `HudStyle.hairline_stylebox` draws); the aside is a footnote to what is above it, and the dashes
## are what say so. Returns the whole block, so a caller adds one child.
##
## Every line is a `readout_aside_line` pair; a line with no `text` is dropped rather than rendered as
## an empty row, since a source with nothing to say in one register still has the others.
static func build_readout_aside(lines: Array) -> VBoxContainer:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", HudComposeVocab.READOUT_ASIDE_SEPARATION)
    block.set_meta(READOUT_ASIDE_META, true)
    block.add_child(build_dashed_rule())
    for line in lines:
        var entry: Dictionary = line
        var text := String(entry.get("text", ""))
        if text == "":
            continue
        var label := Label.new()
        label.text = text
        label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
        label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        label.add_theme_color_override("font_color", entry.get("color", HudStyle.INK_FAINT))
        var meta := String(entry.get("meta", ""))
        if meta != "":
            label.set_meta(meta, true)
        label.add_theme_font_size_override("font_size", HudComposeVocab.READOUT_ASIDE_FONT_SIZE)
        block.add_child(label)
    return block

## A 1px DASHED horizontal rule. Godot has no dashed border on any `StyleBox`, so it is drawn — via
## the `draw` SIGNAL rather than a `Control` subclass, since this module is all-`static` and a
## one-rule widget does not earn a script of its own.
static func build_dashed_rule() -> Control:
    var rule := Control.new()
    rule.custom_minimum_size = Vector2(0.0, HudStyle.DASHED_RULE_HEIGHT)
    rule.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
    # **WITHOUT THIS THE RULE IS INVISIBLE.** A Control draws once on entering the tree, BEFORE its
    # container has laid it out, so the first (and only) pass runs at `size.x == 0` and the dash loop
    # never executes a single iteration — a rule that is silently absent rather than wrong.
    rule.resized.connect(rule.queue_redraw)
    # **THE THIN-LINE PRIMITIVE, NOT A WIDTH-1 ONE, AND THAT IS NOT A STYLE CHOICE.** `draw_line` with
    # an explicit width builds a QUAD one unit tall — and this client renders through a `canvas_items`
    # stretch at a fractional scale (~0.78), so that quad covers 0.78 of a device pixel and whether it
    # rasterises at all is decided by where the rule happens to land. Measured: it vanished entirely,
    # and it vanished just as completely painted in `SIGNAL` cyan, which is what ruled out "too faint"
    # as the explanation. Godot's thin-line primitive (`width <= 0`, the default) is one DEVICE pixel
    # whatever the scale, which is exactly what a hairline wants. A `draw_rect` of the same height
    # fails the same way.
    rule.draw.connect(func() -> void:
        var y := rule.size.y * 0.5
        var x := 0.0
        while x < rule.size.x:
            var dash := minf(HudStyle.DASHED_RULE_DASH, rule.size.x - x)
            rule.draw_line(Vector2(x, y), Vector2(x + dash, y), HudStyle.LINE_SOFT)
            x += HudStyle.DASHED_RULE_DASH + HudStyle.DASHED_RULE_GAP)
    return rule

