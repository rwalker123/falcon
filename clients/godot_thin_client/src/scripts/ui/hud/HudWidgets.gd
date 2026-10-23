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
## line 1, and the yield/policy text (`status_line`) + the status glyph + the ⚠/overstaff notes
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
    # Muted (INK_FAINT) small print: the forecast's yield BAND (`SourceForecast.yield_range_clause`).
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
## the yield/policy text, the status glyph, then the ⚠/overstaff notes — the SAME per-part
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
    # muted yield band (INK_FAINT).
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

## A single-line note Label (⚠ / overstaff / yield band) for the one-line stepper form.
static func build_row_note_label(text: String, color: Color, row_tooltip: String) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", color)
    set_label_tooltip(label, row_tooltip)
    return label

## A secondary status part (line 2 of the two-line form): rendered a touch smaller
## (`HudWorkVocab.ALLOC_SECTION_FONT_SIZE`) than the title.
##
## **`elide` IS FOR A HOST WHOSE WIDTH IS RESERVED RATHER THAN EARNED**, and it is the fix for a
## defect that read as the whole panel being too narrow. A `Label` with autowrap off reports its
## WHOLE text as its minimum width, and a zone host's content is anchored full-rect into a box that
## `clip_contents` — so in a fixed-width zone one long line clamps the entire content column up to
## its own width, and every row in that zone is then laid out past the box and loses its trailing
## control to the clip. Measured on `band_panel_work_inspector_width`: a work inspector sentence
## naming meat and two materials asks **358px of a 356px box**, and the 2px it is over cost the POOLS
## readout its last word, the Builders card its `+`, and every board row its stepper — 14px off the
## right edge of the tab, none of it in the strip that caused it.
##
## `TextServer.OVERRUN_TRIM_ELLIPSIS` drops the reported minimum to one pixel AND marks the cut with
## an `…`, which is why it is the mark rather than `clip_text` — that one cuts mid-word with nothing
## to say it did, and reads as a broken label rather than a shortened one (the trade manifest row's
## rule, `band-city-panel.md`). The full text rides the tooltip through `set_label_tooltip`, since a
## `Label` defaults to `MOUSE_FILTER_IGNORE` and a bare `tooltip_text` on one is a silent no-op.
##
## **IT IS NOT THE DEFAULT, and the hosts are why.** The other callers mount these parts in an
## `HFlowContainer` that WRAPS between parts, inside a surface with width to spare; eliding there
## would shorten prose that fits today. A caller passes `true` when its host cannot grow.
##
## ⛔ **AUTOWRAP IS THE WRONG TOOL *FOR AN ELIDING HOST*, AND IT USED TO BE THE WRONG TOOL FULL STOP.**
## The retired claim read: *"AUTOWRAP IS THE WRONG TOOL HERE and is not an alternative. Both inspector
## strips reserve their height as an exact number of ONE-line parts (`HudWorkVocab.
## WORK_INSPECTOR_NOTE_LINE_HEIGHT`), and the zone clips vertically too — so a line that wrapped would
## take its second row off the bottom of the board, trading a width overflow for a height one."* Both
## halves of that were true of a strip living INSIDE the work zone. The work inspector is a
## viewport-centred `WorkInspectorDialog` now (`docs/plan_standing_upkeep.md` §4.9 item 12d) — no
## zone's box clips it, and its reservation is measured per wrapped line rather than counted in
## one-line parts — so `build_wrapping_status_part` below is the tool for THAT host. Every host still
## inside a reserved-width box (the tile card's flow, the drawer's standing summary, the parties
## inspector's detail lines, the build queue's tooltip) keeps this builder and keeps the elide: for
## them the retired paragraph is still exactly right.
static func build_status_part(text: String, color: Color, elide: bool = false) -> Label:
    var label := Label.new()
    label.text = text
    label.add_theme_color_override("font_color", color)
    label.add_theme_font_size_override("font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
    if elide:
        label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
        set_label_tooltip(label, text)
    return label

## **THE SAME STATUS PART, WRAPPED INSTEAD OF ELIDED — for a host whose width is EARNED rather than
## reserved.** One caller today: `BandPanelController._build_work_inspector`, whose prose lines
## (the overdraw warning, the slipping `note`, the `muted_note`, the site's standing bill) are
## sentences a player has to read to act on, and whose host — the free-floating `WorkInspectorDialog`
## — participates in no zone's layout and clips on neither axis.
##
## ⛔ **A THIRD BUILDER RATHER THAN A THIRD STATE OF `elide`.** The two behaviours are chosen by
## different questions — *can this host grow?* and *is this line prose or a figure?* — and folding
## them into one tri-state parameter would let a caller pass "wrap" to a host that clips, which is the
## defect the elide flag exists to prevent. Widening `build_status_part`'s DEFAULT was the other
## option and is the one thing that must not happen: every other caller sits in a fixed-width box.
##
## **AUTOWRAP KILLS THE MINIMUM WIDTH, WHICH IS WHY IT IS SAFE HERE AND ONLY HERE.** A `Label` with
## autowrap on reports a one-pixel minimum width (Godot's own behaviour) and its full WRAPPED height —
## the exact inverse of the unwrapped form. In a growable host that is what a sentence should ask for;
## in a reserved-width one it would trade a clipped tail for a clipped bottom, which is the retired
## paragraph above.
##
## `WORD_SMART` rather than `WORD`, matching `wrapped_status_part_overflow`'s break flags term for
## term: a `0.05 hurdles` figure longer than the column would OVERFLOW the card under `WORD`, and the
## adaptive break is what keeps it inside.
static func build_wrapping_status_part(text: String, color: Color) -> Label:
    var label := build_status_part(text, color)
    label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    # …and it takes the column's whole width rather than its own natural one, or a wrapped label lays
    # its text out against a rect nobody sized and reports a height measured at a different width than
    # it draws at.
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
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
## look art up by — and a band standing on a settlement stage defined past the bundled three, which is
## not a species at all.
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

## A party's MISSION mark: `hud/<mission>.png` through `HudSprites`, with the mission's text glyph
## (`HudFormat.expedition_mission_glyph`) only as the fallback when the texture is null. A row that
## must keep one `Button` hosts it as `Button.icon` through `set_button_mission_mark` instead.
static func build_mission_mark(mission: String, glyph_color = null) -> Control:
    return build_marker_icon(
        HudSprites.for_mark(HudFormat.expedition_mission_mark(mission)),
        HudFormat.expedition_mission_glyph(mission),
        HudWorkVocab.WORK_ROW_ICON_WIDTH, HudWorkVocab.WORK_ROW_FONT_SIZE, glyph_color)

## Writes a party row's text and mission mark onto a `Button`: the art on `icon` (capped by
## `icon_max_width`, as the action bar's art faces are), else the glyph prefixed to the text.
static func set_button_mission_mark(btn: Button, mission: String, text: String) -> void:
    var tex := HudSprites.for_mark(HudFormat.expedition_mission_mark(mission))
    if tex != null:
        btn.icon = tex
        btn.expand_icon = true
        btn.add_theme_constant_override("icon_max_width", int(HudWorkVocab.WORK_ROW_ICON_WIDTH))
        btn.text = text
    else:
        btn.icon = null
        btn.text = "%s %s" % [HudFormat.expedition_mission_glyph(mission), text]

## The shared −/+ stepper controls (minus, centered count, plus) appended to a row's HBox, so the
## one-line and two-line forms compose the same stepper. `on_change` fires with the new count.
##
## **THE METRIC IS A PARAMETER** for a surface short of width (the starting-loadout picker's rows).
## `metric` is `{button_width, value_width, padding_h}`; anything it omits falls back to the shared
## `WORKER_STEPPER_*` widths. The Work tab's pool rows and site rows all take the defaults, which is what
## puts every stepper on that tab in one column.
##
## **A GREYED `+` SAYS WHY** (`selection-card.md` → "A DISABLED CONTROL SAYS WHY"): `plus_blocked_reason`
## is the caller's sentence for the condition that made `plus_enabled` false, and becomes the `+`'s
## hover. A greyed `−` is always the same fact — there is nobody on it — so it needs no argument.
## Returns the count `Label`, so a caller can restate it (a donor row's `4 → 3`).
static func add_stepper_controls(row: HBoxContainer, count: int, plus_enabled: bool,
        on_change: Callable, compact_chrome: bool = false, metric: Dictionary = {},
        plus_blocked_reason: String = "") -> Label:
    var button_width := float(metric.get(STEPPER_METRIC_BUTTON_WIDTH,
        HudWorkVocab.WORKER_STEPPER_BUTTON_WIDTH))
    var value_width := float(metric.get(STEPPER_METRIC_VALUE_WIDTH,
        HudWorkVocab.WORKER_STEPPER_VALUE_WIDTH))
    var padding_h := int(metric.get(STEPPER_METRIC_PADDING_H, KEEP_BUTTON_PADDING_H))
    var minus := Button.new()
    minus.text = HudWorkVocab.STEPPER_MINUS_FACE
    minus.custom_minimum_size = Vector2(button_width, 0)
    HudStyle.apply_button(minus, "ghost")
    minus.disabled = count <= 0
    if minus.disabled:
        minus.tooltip_text = HudWorkVocab.STEPPER_MINUS_AT_ZERO_REASON
    minus.pressed.connect(func() -> void: on_change.call(count - HudConst.WORKER_STEP))
    row.add_child(minus)
    var value := Label.new()
    value.text = str(count)
    value.custom_minimum_size = Vector2(value_width, 0)
    value.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    value.add_theme_color_override("font_color", HudStyle.INK if count > 0 else HudStyle.INK_FAINT)
    row.add_child(value)
    var plus := Button.new()
    plus.text = HudWorkVocab.STEPPER_PLUS_FACE
    plus.custom_minimum_size = Vector2(button_width, 0)
    HudStyle.apply_button(plus, "ghost")
    plus.disabled = not plus_enabled
    if plus.disabled:
        plus.tooltip_text = plus_blocked_reason
    plus.pressed.connect(func() -> void: on_change.call(count + HudConst.WORKER_STEP))
    row.add_child(plus)
    if compact_chrome:
        for control in [minus, value, plus]:
            compact(control, HudWorkVocab.WORK_STEPPER_FONT_SIZE,
                HudWorkVocab.WORK_STEPPER_PADDING_V, padding_h)
    return value

## **THE MAX STEPPER** (#871) — `−`, the value, `+`, for the most hands the sim may put on a staffed
## source. `0` is "no max" and reads `No max`; the `−` is greyed there (there is nothing lower) and the
## `+` greys at `upper`, the band's working-age people. `on_change` fires with the new Max. The buttons
## carry the aria names `Lower the max` / `Raise the max`. `floor_workers` is the lowest count the stepper
## offers (default 0 = `No max`); a dig crew passes 1, so `No max` is never drawn and `−` greys at 1.
## Returns the value `Label`.
static func add_max_stepper(row: HBoxContainer, max_workers: int, upper: int,
        on_change: Callable, compact_chrome: bool = false, floor_workers: int = 0) -> Label:
    var minus := Button.new()
    minus.text = HudWorkVocab.STEPPER_MINUS_FACE
    minus.custom_minimum_size = Vector2(HudWorkVocab.WORKER_STEPPER_BUTTON_WIDTH, 0)
    HudStyle.apply_button(minus, "ghost")
    minus.disabled = max_workers <= floor_workers
    minus.tooltip_text = HudWorkVocab.MAX_STEPPER_LOWER_ARIA
    if "accessibility_name" in minus:
        minus.set("accessibility_name", HudWorkVocab.MAX_STEPPER_LOWER_ARIA)
    minus.pressed.connect(func() -> void: on_change.call(maxi(max_workers - HudConst.WORKER_STEP, floor_workers)))
    row.add_child(minus)
    var value := Label.new()
    value.text = str(max_workers) if max_workers > 0 else HudWorkVocab.MAX_STEPPER_NO_MAX
    value.custom_minimum_size = Vector2(HudWorkVocab.MAX_STEPPER_VALUE_WIDTH, 0)
    value.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    value.add_theme_color_override("font_color", HudStyle.INK if max_workers > 0 else HudStyle.INK_FAINT)
    value.set_meta(HudWorkVocab.MAX_STEPPER_VALUE_META, max_workers)
    row.add_child(value)
    var plus := Button.new()
    plus.text = HudWorkVocab.STEPPER_PLUS_FACE
    plus.custom_minimum_size = Vector2(HudWorkVocab.WORKER_STEPPER_BUTTON_WIDTH, 0)
    HudStyle.apply_button(plus, "ghost")
    plus.disabled = max_workers >= upper
    plus.tooltip_text = HudWorkVocab.MAX_STEPPER_RAISE_ARIA
    if "accessibility_name" in plus:
        plus.set("accessibility_name", HudWorkVocab.MAX_STEPPER_RAISE_ARIA)
    plus.pressed.connect(func() -> void:
        on_change.call(mini(max_workers + HudConst.WORKER_STEP, maxi(upper, 0))))
    row.add_child(plus)
    row.set_meta(HudWorkVocab.MAX_STEPPER_META, true)
    if compact_chrome:
        for control in [minus, value, plus]:
            compact(control, HudWorkVocab.WORK_STEPPER_FONT_SIZE,
                HudWorkVocab.WORK_STEPPER_PADDING_V)
    return value

## **A STAFFED ROW'S CREW NUMBER** — the read-only stand-in for the stepper the row used to carry: the
## sim's crew, in the stepper's value column so every row's number lines up. Faint at zero.
static func build_crew_number(workers: int) -> Label:
    var value := Label.new()
    value.text = str(workers)
    value.custom_minimum_size = Vector2(HudWorkVocab.CREW_NUMBER_WIDTH, 0)
    value.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    value.add_theme_color_override("font_color", HudStyle.INK if workers > 0 else HudStyle.INK_FAINT)
    value.add_theme_font_size_override("font_size", HudWorkVocab.WORK_STEPPER_FONT_SIZE)
    value.mouse_filter = Control.MOUSE_FILTER_IGNORE
    value.set_meta(HudWorkVocab.CREW_NUMBER_META, workers)
    return value

## **THE STATIC `Max N` CHIP** beside a row's Priority pill — a quiet label, not a control.
static func build_max_chip(max_workers: int) -> Label:
    var chip := Label.new()
    chip.text = HudWorkVocab.MAX_CHIP_FORMAT % max_workers
    chip.add_theme_color_override("font_color", HudStyle.INK_DIM)
    chip.add_theme_font_size_override("font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
    chip.tooltip_text = HudWorkVocab.MAX_CHIP_TOOLTIP
    chip.mouse_filter = Control.MOUSE_FILTER_PASS
    chip.set_meta(HudWorkVocab.MAX_CHIP_META, max_workers)
    return chip

## The three keys a `metric` may carry. **Named**, because producer and reader are different scripts
## and a typo in a `get` here is a silent fallback to the shared width — which is precisely the
## overflow the metric exists to prevent, reappearing with nothing to show it.
const STEPPER_METRIC_BUTTON_WIDTH := "button_width"
const STEPPER_METRIC_VALUE_WIDTH := "value_width"
const STEPPER_METRIC_PADDING_H := "padding_h"

## Squeeze a control into a zone's fixed-height chrome row: smaller type, and a button's stylebox
## chrome trimmed vertically. `HudStyle._button_stylebox` pads 9px top and bottom, which alone makes a
## plain Button ~40px tall — taller than `HudWorkVocab.WORK_ROW_HEIGHT`, `HudWorkVocab.ZONE_HEAD_HEIGHT`, `HudWorkVocab.WORK_CHIPS_HEIGHT`
## and `HudWorkVocab.WORK_PAGER_HEIGHT` put together. Every one of those consts is a height the board's capacity
## maths SUBTRACTS, so a control that renders taller pushes the page off the bottom of the zone.
static func compact(control: Control, font_size: int, padding_v: int,
        padding_h: int = KEEP_BUTTON_PADDING_H) -> void:
    control.add_theme_font_size_override("font_size", font_size)
    trim_button_padding(control, padding_v, padding_h)

## `padding_h`'s "leave the sides exactly as `HudStyle` authored them", which is what every caller
## but one wants: a zone row is short on HEIGHT, not on width. Negative rather than a magic zero,
## because zero is a real trim a caller may ask for.
const KEEP_BUTTON_PADDING_H := -1

## The chrome half of `compact`, on its own for a control that must keep its TYPE SIZE and trim only
## the box around it. Does nothing to a non-Button (a Label has no stylebox to squeeze).
##
## **THE SIDES ARE OPT-IN** (`KEEP_BUTTON_PADDING_H`) and the build queue's reorder arrows are the one
## caller that opts in: `HudStyle`'s 11px side margins are why a 9px `✕` needed a 32px column, and a
## PAIR of arrows sharing that same 32 has no room for four of them.
static func trim_button_padding(control: Control, padding_v: int,
        padding_h: int = KEEP_BUTTON_PADDING_H) -> void:
    if not (control is Button):
        return
    for state in ["normal", "hover", "pressed", "disabled", "focus"]:
        var box: StyleBox = control.get_theme_stylebox(state)
        if box == null:
            continue
        var squeezed: StyleBox = box.duplicate()
        squeezed.content_margin_top = padding_v
        squeezed.content_margin_bottom = padding_v
        if padding_h != KEEP_BUTTON_PADDING_H:
            squeezed.content_margin_left = padding_h
            squeezed.content_margin_right = padding_h
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

## **THE HINT LINE AS RICH TEXT — one wrapped line whose RUNS can differ in colour.** The twin of
## `alloc_hint_label` and deliberately styled to match it term for term (same font size, same
## `AUTOWRAP_WORD_SMART`, same expand-fill), so swapping one for the other does not move the line's
## measured height; `ink` is installed as the DEFAULT colour, which is what an untagged run reads.
##
## It exists because a role card states TWO things on one line — what the gear buys, and whether there
## is enough of it — and only the second is a warning. A `Label` carries one `font_color`, so the card
## could only redden both or neither; an `HBox` of two labels cannot wrap as one line, and giving the
## shortfall its own line would cost the card a ROW, which the band zone's measured stacking budget
## cannot afford.
static func alloc_hint_markup(bbcode: String, ink: Color) -> RichTextLabel:
    var label := RichTextLabel.new()
    label.bbcode_enabled = true
    label.fit_content = true
    label.scroll_active = false
    label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.add_theme_font_size_override("normal_font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
    label.add_theme_color_override("default_color", ink)
    label.add_theme_stylebox_override("normal", HudStyle.empty_stylebox())
    label.text = bbcode
    return label

## An inline text link (the parties inspector's actions).
static func build_inline_link(text: String, ink: Color, on_press: Callable) -> Button:
    var link := Button.new()
    link.text = text
    link.focus_mode = Control.FOCUS_NONE
    link.add_theme_font_size_override("font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
    HudStyle.apply_link_button(link, ink)
    link.pressed.connect(func() -> void: on_press.call())
    return link

## A one-line BBCode readout inside the assign controls (the live forecast / yield preview).
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

## A composition chip's segment key, as `Control` meta — the handle a harness identifies it by,
## since issue #249 made the key a TEXTURE on the PEOPLE bar and left the label holding the count
## alone. Mirrors `POLICY_RUNG_META`'s job for its control.
const COMPOSITION_SEGMENT_META := &"composition_segment"
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
        # The chip's STABLE HANDLE — which segment this is, independent of how its face is drawn.
        # The harnesses identified a chip by SPLITTING ITS LABEL (`"👶 9"` → key, count), which stops
        # working the moment the key becomes a texture and the label is the bare count; a meta is the
        # identity `test-harnesses.md` → "An assertion asks a CONTROL, not the subtree" asks for. The
        # COUNT is deliberately NOT stashed here — that number must still be read off the rendered
        # label, or the assertion checks the harness's own arithmetic instead of the bar's.
        chip.set_meta(COMPOSITION_SEGMENT_META, String(segment.get("key", "")))
        var swatch := ColorRect.new()
        swatch.color = segment.get("color", HudStyle.INK_FAINT)
        swatch.custom_minimum_size = COMPOSITION_SWATCH_SIZE
        swatch.size_flags_vertical = Control.SIZE_SHRINK_CENTER
        swatch.mouse_filter = Control.MOUSE_FILTER_IGNORE
        chip.add_child(swatch)
        # **THE MARK IS ART WHERE THE BRACKET HAS ANY** (issue #249), and it is its own child rather
        # than a prefix inside the count's text — a texture cannot live in a `Label.text`, the same
        # split the roster row's marks took. `PEOPLE_MARKS` is keyed by the GLYPH the segment already
        # carries, so no caller had to learn a second vocabulary to opt in.
        var glyph := String(segment.get("key", ""))
        var mark := HudSprites.for_mark(String(HudWorkVocab.PEOPLE_MARKS.get(glyph, "")))
        if mark != null:
            chip.add_child(build_marker_icon(mark, glyph, HudWorkVocab.PEOPLE_MARK_BOX,
                HudWorkVocab.COMPOSITION_KEY_FONT_SIZE))
        var text := Label.new()
        text.text = ("%d" % int(segment.get("count", 0))) if mark != null \
            else "%s %d" % [glyph, int(segment.get("count", 0))]
        text.add_theme_font_size_override("font_size", HudWorkVocab.COMPOSITION_KEY_FONT_SIZE)
        text.add_theme_color_override("font_color", HudStyle.INK_DIM)
        text.mouse_filter = Control.MOUSE_FILTER_IGNORE
        chip.add_child(text)
        key.add_child(chip)
    if trailing != null:
        key.add_child(trailing)
    return key

## **THE ONE PEOPLE BAR'S SEGMENTS** — children | working | idle | elders — shared by the band zone and
## the faction page so the palette, order and tooltips are written once. `working_parts` is
## `HudBandLaborState.working_breakdown` (summed over bands on the faction page); it becomes the
## working segment's tooltip. `idle_tooltip` is the idle card's reason line. Zero segments are omitted.
static func people_segments(children: int, working: int, idle: int, elders: int,
        working_parts: Dictionary, idle_tooltip: String) -> Array:
    var segments: Array = []
    if children > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_CHILDREN, "count": children,
            "color": HudStyle.VOICE_PIGMENT,
            "tooltip": "%d %s" % [children, HudWorkVocab.PEOPLE_LABEL_CHILDREN]})
    if working > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_WORKING, "count": working,
            "color": HudStyle.INK_DIM, "tooltip": working_tooltip(working, working_parts)})
    if idle > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_KEY_IDLE, "count": idle,
            "color": HudStyle.WARN, "tooltip": idle_tooltip})
    if elders > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_ELDERS, "count": elders,
            "color": HudStyle.VOICE_INK,
            "tooltip": "%d %s" % [elders, HudWorkVocab.PEOPLE_LABEL_ELDERS]})
    return segments

## The working segment's one-sentence tooltip: `18 working: Forage 9 · Hunt 6 · Build 3`.
## No parts (nothing known to be doing anything) reads `18 working age`.
static func working_tooltip(working: int, parts: Dictionary) -> String:
    if parts.is_empty():
        return "%d %s" % [working, HudWorkVocab.PEOPLE_LABEL_WORKING]
    var pieces: Array[String] = []
    for label in parts:
        pieces.append(HudWorkVocab.PEOPLE_WORKING_PART_FORMAT % [label, int(parts[label])])
    return HudWorkVocab.PEOPLE_WORKING_BREAKDOWN_FORMAT % [
        working, HudWorkVocab.PEOPLE_WORKING_BREAKDOWN_SEPARATOR.join(pieces)]

## The People head's readout: the head count plus the away clause (parties are a header clause, not a
## segment: the sim takes a party's members out of `working_age` on launch).
static func people_head_readout(total: int, away: int) -> String:
    var readout := str(total)
    if away > 0:
        readout += HudWorkVocab.PEOPLE_AWAY_FORMAT % away
    return readout

## The IDLE CARD: a bordered amber card, a big amber number over a dim reason line. The card's tooltip
## says idle people are spare capacity. Tagged `IDLE_CARD_META` so a harness finds it by identity.
static func build_idle_card(number_text: String, reason_text: String) -> PanelContainer:
    var card := PanelContainer.new()
    card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    card.add_theme_stylebox_override("panel", HudStyle.idle_card_stylebox())
    card.set_meta(HudWorkVocab.IDLE_CARD_META, true)
    card.tooltip_text = HudWorkVocab.IDLE_CARD_TOOLTIP
    var col := VBoxContainer.new()
    col.add_theme_constant_override("separation", HudWorkVocab.IDLE_CARD_SEPARATION)
    col.mouse_filter = Control.MOUSE_FILTER_IGNORE
    card.add_child(col)
    var number := Label.new()
    number.text = number_text
    number.add_theme_font_size_override("font_size", HudWorkVocab.IDLE_CARD_NUMBER_FONT_SIZE)
    number.add_theme_color_override("font_color", HudStyle.WARN)
    number.set_meta(HudWorkVocab.IDLE_CARD_NUMBER_META, true)
    number.mouse_filter = Control.MOUSE_FILTER_IGNORE
    col.add_child(number)
    var reason := Label.new()
    reason.text = reason_text
    reason.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    reason.add_theme_font_size_override("font_size", HudWorkVocab.IDLE_CARD_REASON_FONT_SIZE)
    reason.add_theme_color_override("font_color", HudStyle.INK_DIM)
    reason.mouse_filter = Control.MOUSE_FILTER_IGNORE
    col.add_child(reason)
    return card

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
## ⛔ **`title_tooltip` IS THE TRAILING OPTIONAL, AND IT EXISTS SO A HEAD CAN CARRY A HOVER WITH NO
## READOUT.** `readout_tooltip` above rides the RIGHT-hand Label, which is built only where a readout
## is stated — so a head whose readout is a conditional MARK (the workings roster's shortfall glyph)
## would lose its hover on exactly the calm band that most needs the words. It goes through
## `set_label_tooltip` for that helper's own reason: a `Label` defaults to `MOUSE_FILTER_IGNORE`, so a
## bare `tooltip_text` here is a silent no-op.
static func zone_head(title: String, readout: String, menu: MenuButton = null, readout_color: Color = HudStyle.INK_DIM, readout_tooltip: String = "", title_tooltip: String = "") -> HBoxContainer:
    var head := HBoxContainer.new()
    head.custom_minimum_size = Vector2(0.0, HudWorkVocab.ZONE_HEAD_HEIGHT)
    head.add_theme_constant_override("separation", HudWorkVocab.ZONE_HEAD_SEPARATION)
    var title_label := alloc_section_label(title)
    if title_tooltip != "":
        set_label_tooltip(title_label, title_tooltip)
    head.add_child(title_label)
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
    fill_menu_popup(button.get_popup(), entries)
    return button

## **A FIELD THE SHEET STATES RATHER THAN ASKS** — the field-stack's key label (`build_field_key`, so
## it lines up with the rows that ARE controls) and the value as plain INK text, optionally led by
## `icon` at `icon_px`. It carries `READ_ONLY_FIELD_META` = the key. The value expands and clips, so a
## long species name cannot push the row wider than the sheet.
static func build_read_only_field(key: String, text: String, icon: Texture2D = null,
        icon_px: float = 0.0) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    row.set_meta(READ_ONLY_FIELD_META, key)
    row.add_child(build_field_key(key))
    if icon != null:
        row.add_child(build_marker_icon(icon, "", icon_px, 0))
    var value := Label.new()
    value.text = text
    value.clip_text = true
    value.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    value.add_theme_color_override("font_color", HudStyle.INK)
    row.add_child(value)
    return row

## **A COMPOSE-SHEET FIELD ROW'S KEY LABEL** — `Band:`, `Kit`. Its whole job is the ONE declared width
## (`HudComposeVocab.COMPOSE_FIELD_KEY_WIDTH`) that makes rows built by different modules line their
## value controls up; the reasoning is on that constant. `SIZE_FILL`, not `EXPAND` — the key takes
## exactly its declared width and the CONTROL is the row's only expanding child.
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
## eats it the moment the label reaches the button's edge (`Harvesting kit` does — it read
## `Gathering kit` when measured, one character shorter — so the forage sheet's
## kit picker showed no caret at all) and where it renders as a small low-baseline mark rather than as
## the themed arrow the `Band:` picker one row above already drew. An `OptionButton` reserves the
## arrow's width as an internal right margin, so the icon is drawn OUTSIDE the text's clip rect and no
## face can push it off. It also CHECKS the current entry natively — its popup items are radio-check
## items and it marks the selected one itself — which is the behaviour `_fill_menu_popup` hand-rolls
## through `MENU_ENTRY_CHECKED`.
##
## `entries` is an ordered array of `{label, disabled, tooltip, on_pick}` — `build_section_menu`'s
## contract minus `MENU_ENTRY_CHECKED`, which a native selector OWNS: `selected_index` is both the
## entry the face opens on and the one the popup marks (`NO_ENTRY_SELECTED` for a field nothing has
## been chosen in — and it must be PASSED, never left to Godot; see the deselect in the body), and
## passing a second, hand-rolled mark would let the two disagree. (No `MENU_ENTRY_ICON` either: neither caller has per-entry art, and repeating
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
## `selected_index` for a field nothing has been chosen in yet. Godot's own "no item is selected"
## index, named because every caller of `build_option_picker` has to be able to SAY it.
const NO_ENTRY_SELECTED := -1

static func build_option_picker(entries: Array, selected_index: int, face: String,
        tooltip: String) -> OptionButton:
    var button := OptionButton.new()
    button.tooltip_text = tooltip
    button.focus_mode = Control.FOCUS_NONE
    button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    button.clip_text = true
    button.fit_to_longest_item = false
    HudStyle.apply_option_button(button)
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
    # **THE PICKER STATES ITS OWN SELECTION, ALWAYS — INCLUDING "NOTHING".** `OptionButton.add_item`
    # selects the first SELECTABLE item it is handed, so a picker built for a field nothing has been
    # chosen in comes back holding an entry anyway, while the `face` write below paints `Choose…`
    # over it. Godot then refuses to report a pick of the entry it believes is already current — no
    # `item_selected`, no `on_pick` — so the one entry a player cannot choose is the very one the
    # widget seated itself on, and the popup closes as if nothing was clicked. That is not a corner:
    # it is the FIRST live entry of every list, which on the shipment sheet was the only one.
    # `select(NO_ENTRY_SELECTED)` is Godot's own deselect, and it runs BEFORE the face write because
    # selecting rewrites `text`.
    var chosen := selected_index if selected_index >= 0 and selected_index < picks.size() \
        else NO_ENTRY_SELECTED
    button.select(chosen)
    button.text = face
    button.item_selected.connect(func(index: int) -> void:
        if index >= 0 and index < picks.size() and picks[index].is_valid():
            picks[index].call())
    return button

## The shared popup fill — one implementation of the entry contract (`{label, disabled, on_pick}` + the
## optional `MENU_ENTRY_CHECKED` / `MENU_ENTRY_ICON`), for the `⋯` menu face above and for a free
## `PopupMenu` with no face (the Deny pick's herd chooser, `TargetingController._open_quarry_chooser`).
static func fill_menu_popup(popup: PopupMenu, entries: Array) -> void:
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

## **A DISABLED CONTROL'S HOVER: THE REASONS FIRST, THEN WHAT THE CONTROL IS.** `reasons` empty is the
## enabled face — `tooltip` alone, unchanged — so a caller hands the gate's list over without
## branching. Every reason is its own line (`HudWorkVocab.DISABLED_REASON_SEPARATOR`): fixing one
## must not hide that another still stands. See `selection-card.md` → "A DISABLED CONTROL SAYS WHY".
static func disabled_tooltip(reasons: PackedStringArray, tooltip: String) -> String:
    if reasons.is_empty():
        return tooltip
    var lead := HudWorkVocab.DISABLED_REASON_SEPARATOR.join(reasons)
    return lead if tooltip == "" \
        else HudWorkVocab.DISABLED_REASON_SEPARATOR.join(PackedStringArray([lead, tooltip]))

## The party stepper row, shared by both missions so they cannot drift apart in shape.
## `key_text` defaults to the word the three EXPEDITION sheets want. The split sheet passes its own,
## because a sheet whose whole thesis is *this is not a party* must not label its one input `Party` —
## the wrong word there teaches the wrong model more effectively than any amount of prose fixes.
## `at_max_reason` is the greyed `+`'s hover — the party is every hand the sheet may spend.
static func build_party_stepper_row(count: int, party_max: int, on_change: Callable,
        key_text: String = HudComposeVocab.COMPOSE_FIELD_PARTY,
        at_max_reason: String = HudComposeVocab.PARTY_AT_MAX_REASON) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    row.set_meta(PARTY_STEPPER_COUNT_META, count)
    var key := Label.new()
    key.text = key_text
    key.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(key)
    add_stepper_controls(row, count, count < party_max, on_change, false, {}, at_max_reason)
    return row

## The rung a policy-picker Button stands for, as `Button` meta. THE ONE STABLE HANDLE on a rung: the
## face is presentation and has already changed twice (glyph+metric → glyph+name over metric → that
## same pair as child Labels at two sizes), so a harness matching on `btn.text` breaks with every
## visual pass. `band_panel_preview._picker_rung_buttons` reads this.
const POLICY_RUNG_META := "policy"

## The floor CHART, as `HarvestFloorChart` meta — the same stable-handle reasoning, and needed more
## than most: the chart carries no text at all, so a harness has nothing else to find it by. (It
## replaced `FLOOR_SLIDER_META`, whose plain `HSlider` the chart's draggable floor line supersedes.)
const FLOOR_CHART_META := "floor_chart"

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

## **THE KEEPING ROW'S TWO METAS ARE RETIRED** (`docs/plan_standing_upkeep.md` §2.5).
## `CREW_ROW_MAINTAIN_META` and `MAINTAIN_VERDICT_META` tagged a stepper and a verdict on a row that
## no longer exists: maintenance left the tile, so neither compose sheet composes a keeping crew and
## there is nothing here for a harness to find. What a source's keeping costs is stated by
## `DetailFormat.at_risk_lines` on the land card and the herd drawer.

## **RETIRED — `BUILD_CREW_ROW_META`** (`docs/plan_standing_upkeep.md` §2.5), the third meta on this
## list to go the same way. It tagged the BUILDERS stepper on the improvement control; a verb
## declares and names no hands, so there is no per-source build crew and nothing here to find. The
## pool is a standing role card, and the queue that spends it is the band's.

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

# ⛔ **RETIRED: `HUNT_CREW_SPLIT_META`**, the handle on the split line above. It took a SECOND meta
# rather than riding the gate's *"for the reason that one is read by value: `HUNT_GATE_META` is `true`
# exactly when the fight is refused, so a split line wearing it would answer 'the sheet says this
# fight is winnable' to a harness asking whether it is blocked"* — a good rule, and it retires with
# the only node that ever wore it. `HUNT_GATE_META` is untouched.

## --- THE CARGO ROW'S HANDLES (issue #620) --------------------------------------------------------
## A shipment cargo row, as `HBoxContainer` meta, carrying the row's own KEY
## (`BandPanelController.TRADE_FOOD_ROW_KEY` / `TRADE_FODDER_ROW_KEY` / a batch key). **The row used to
## be found by walking children positionally** — the name label first, the `+` last — and that walk
## broke silently the moment a fourth control (`Max`) joined the row: it found the new button and
## pressed it believing it was the `+`. A key is the one handle that survives a control being added.
const CARGO_ROW_KEY_META := "cargo_row_key"

## …and WHICH control of that row a node is, as `Control` meta — one of the four values below. Keyed
## this way rather than as four booleans: a harness asserting
## about "the row's controls" wants the SET of them, and a positional walk over an `HBoxContainer` is
## exactly what this replaced.
const CARGO_CONTROL_META := "cargo_control"
const CARGO_CONTROL_MINUS := "minus"
const CARGO_CONTROL_FIELD := "field"
const CARGO_CONTROL_PLUS := "plus"
const CARGO_CONTROL_MAX := "max"
## A cargo GOOD's main line (the material's own row, over its grade lines) — `CARGO_ROW_KEY_META`
## is on it too, valued the good's key; a grade line carries that meta alone, valued its pile's key.
const CARGO_GOOD_ROW_META := "cargo_good_row"
## The grade disclosure on a cargo good, valued the good's key.
const CARGO_GRADES_TOGGLE_META := "cargo_grades_toggle"

## The "send a shipment" CONFIRM button, as `Button` meta (arc #527) — its own handle rather than the
## hunt one's, because the shipment form reaches it from FIVE branches (no ties, no destination
## chosen, no stores, an unsendable manifest, and the live send) and a harness's claim about which
## branch rendered is only checkable if all five wear one findable mark. It is set on the DISABLED
## branches too: a form that built nothing at all would otherwise satisfy an absence assertion.
const SEND_TRADE_CONFIRM_META := "send_trade_confirm"
## The station sheet's send (issue #774) — live or greyed, the one handle a harness finds it by.
const SEND_STATION_CONFIRM_META := "send_station_confirm"

## The DENIAL raid's confirm button (`docs/plan_denial_raid.md`). **Its OWN meta, not the hunt one**,
## and for the reason the two forms are separate at all: a harness that pressed "the send button" on a
## parties compose sheet could not tell which MISSION it had just launched, and the two emit different
## signals with different, non-interchangeable payloads (a denial payload carries no floor, and its
## command grammar rejects one). Its face is the collapse verdict, so text is not matchable either.
const SEND_DENIAL_CONFIRM_META := "send_denial_confirm"

## The SCOUTING party's confirm button, and its own meta for the same reason the denial one is: the
## three missions' sends are different signals with non-interchangeable payloads, so a harness that
## pressed "the send button" on a parties compose sheet could not say which it launched. Its face is
## fixed text (`SEND_EXPEDITION_BUTTON`), but the payload it composes now carries a KIT — which is
## reachable only through the inline `pressed` lambda, and therefore only through this handle.
const SEND_EXPEDITION_CONFIRM_META := "send_expedition_confirm"

## A band VERB button on the tile panel's band drawer (issue #529), as `Button` meta carrying the
## verb's registry id (`HudComposeVocab.VERB_*`). The drawer's row is built from the same list as the
## Band panel's action bar, so a harness asserting "the same five ids, in order" reads it by this
## handle — the faces are art or a glyph, and neither is text a harness may match on.
const VERB_BUTTON_META := "band_verb"

## The pending verb's sheet as mounted in its band's drawer — its node name and its `Control` meta,
## whose value is the verb's MISSION (`BandPanelController.build_verb_form`). How the drawer finds the
## sheet it is replacing, and how a harness finds which verb's sheet a drawer is showing.
const VERB_FORM_NAME := "BandVerbForm"
const VERB_FORM_META := "band_verb_form"

## A READ-ONLY field row on a verb's sheet (`build_read_only_field`), as meta carrying the field's
## key — the Deny sheet's pre-selected PREY, the Trade sheet's pre-selected destination (`To`). The handle a harness proves "stated, not offered" by, since
## the row holds a `Label` where a chooser would hold a `Button`.
const READ_ONLY_FIELD_META := "read_only_field"

## The Deny sheet's PREY CHOOSER, as `MenuButton` meta — the `⋯` that appears only when the prey's hex
## holds more than one eligible herd. Its own handle because the parties zone builds a `⋯` section menu
## too, and the claim a harness makes is an ABSENCE with one candidate.
const QUARRY_CHOICES_META := "quarry_choices"

## A read-only field row's `✕`, as `Button` meta — clears the Deny sheet's pre-selected prey or the
## Trade sheet's pre-selected destination back to unset.
const FIELD_CLEAR_META := "field_clear"

## A compose sheet's COMMIT button, as `Button` meta — set by both sheets' builders. Its face is the
## thing under test whenever the crew noun moves (`Forage` / `Tend` / `Unassign` on the plant web,
## `Hunt Here` / the raid verdict on the animal one), so a harness that found it BY text could only
## ever confirm the string it already assumed. Identity is the only stable handle.
const COMPOSE_COMMIT_META := "compose_commit"

## The compose sheet's WORK PARTY section, as meta on its host (`DrawerComposeController.
## _mount_work_party_section`), and each of its lines carrying that line's own text — the
## `WORK_ROW_PARTY_META` treatment, so a harness reads what was drawn rather than recomposing it.
const WORK_PARTY_SECTION_META := "work_party_section"
const WORK_PARTY_LINE_META := "work_party_line"

## RETIRED — **`CHECKBOX_LIVE_FONT_COLOR_SLOTS`**, every font-colour slot a live `CheckBox` drew its
## face in (`docs/plan_standing_upkeep.md` §4.7a ①). It existed because `HudStyle.apply_checkbox`
## writes all of them, so a caller re-tinting a face had to write all of them or the colour flickered
## back to neutral under the pointer. This control has no checkbox left to tint: a `Label` draws one
## `font_color` and nothing hovers it. `HudStyle.apply_checkbox` itself SURVIVES — it is the client's
## dark-surface checkbox treatment and belongs with the palette, not with this control.

## RETIRED — **`BUILD_WORK_FLOOR_META`**, the BUILD crew stepper's threshold as `Control` meta
## (`docs/plan_standing_upkeep.md` §2.4). It carried the quoted rung's upkeep rate, which a build crew
## had to beat before any of its output was progress; the keeping pool owes that rate at every
## fullness now, so **no rung states a build-crew threshold** and there is no number for a harness to
## read off this row. `SourceForecast.min_build_work` and `HudComposeVocab.CREW_BUILD_FLOOR_TOOLTIP`
## went with it, and the rate itself is the offered face's STANDING PRICE
## (`HudComposeVocab.BUILD_PRICE_UPKEEP_FORMAT`).

## The PLANT a crop-picker row stands for, as `Button` meta — its `FloraShareInfo.species` key, which
## is the sim's own id and the very string the row's art is composed from (`FloraSprites`). The
## stable-handle reasoning of the three consts above, plus one this family has of its own: the row's
## FACE is `<display name> <share>% · <payoff>`, so every other handle onto a row goes through the
## LABEL — a different axis from the key, and deliberately allowed to disagree with it (a fixture
## pairs the roster id `wild_emmer` with the label "Wild Grain" on purpose). A harness asking whether
## THIS SPECIES' art reached the row must ask by the key it was resolved with, or the claim is about
## the label instead.
const FLORA_CROP_ROW_SPECIES_META := "flora_crop_row_species"



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

## **THE RANK CONTROL** — the three priority levels as a radio over `on_pick(level: String)`, with a
## single hint line beneath saying what the rank does (`docs/plan_standing_upkeep.md` §4.9 item 9b).
##
## **IT IS `build_floor_picker`'S SHAPE ON PURPOSE, down to the cell builder.** The two live one link
## apart in the same inspector strip, they are mutually exclusive, and a player who has used one has
## learned the other; a second layout for three buttons in a row would be a new thing to learn for no
## new meaning. `_policy_rung_cell` is shared rather than copied, so the resting/selected tints, the
## padding and the two-line face allowance move for both at once.
##
## **THE ORDER IS `HudWorkVocab.WORK_PRIORITY_LEVELS`, NEVER THE WIRE'S.** `SourcePriority` numbers
## Normal first (a default costs no wire bytes) while the band sheds Low, Normal, High — so a picker
## built by walking the ordinals would put the default in the leftmost slot and read as a scale that
## starts in the middle. The vocab states the reading order once and this is its only consumer.
##
## **THE HINT NAMES NO RESOURCE**, deliberately: the rank orders workers and pen feed today and
## whatever the next scarcity handler is tomorrow, and a per-consumer list would grow forever. It is
## ELIDING with its own hover, the treatment every other one-line label in this strip takes, since the
## zone is a fixed-width box and this sentence is the longest thing in the block.
static func build_work_priority_picker(on_pick: Callable, selected_level: String,
        show_hint: bool = true) -> VBoxContainer:
    var current := HudWorkVocab.work_priority_of(selected_level)
    var block := VBoxContainer.new()
    block.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    var grid := GridContainer.new()
    # The three levels fit one row exactly, at the SAME ceiling the floor presets wrap on — a zone is
    # a fixed-width box, and a picker whose buttons sum past it widens the whole zone column.
    grid.columns = mini(HudWorkVocab.WORK_PRIORITY_LEVELS.size(), HudWorkVocab.POLICY_PICKER_COLUMNS)
    grid.add_theme_constant_override("h_separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    grid.add_theme_constant_override("v_separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    for level in HudWorkVocab.WORK_PRIORITY_LEVELS:
        var btn := Button.new()
        var is_selected := level == current
        var variant := "primary" if is_selected else "ghost"
        # The LEVEL as meta, not the face: the face is presentation, and the level is the token the
        # command carries — so a harness reads the button by what it would SEND.
        btn.set_meta(HudWorkVocab.WORK_PRIORITY_RUNG_META, level)
        HudStyle.apply_button(btn, variant)
        btn.pressed.connect(func() -> void: on_pick.call(level))
        # A LEVEL AT REST IS QUIETER THAN A BUTTON AT REST — `build_floor_picker`'s rule: an unpicked
        # rung is a shortcut to a value rather than an action, so it reads `INK_DIM`.
        var tint := HudStyle.button_font_color(variant, btn.disabled)
        if not is_selected:
            tint = HudStyle.INK_DIM
        # A host that draws no hint line still keeps the reason: it rides every level's hover.
        if not show_hint:
            btn.tooltip_text = HudWorkVocab.WORK_PRIORITY_HINT
        var cell := _policy_rung_cell(btn, String(HudWorkVocab.WORK_PRIORITY_FACES.get(level, level)),
            "", tint)
        cell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        grid.add_child(cell)
    block.add_child(grid)
    if show_hint:
        block.add_child(build_status_part(HudWorkVocab.WORK_PRIORITY_HINT, HudStyle.INK_FAINT, true))
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

# ⛔ **RETIRED: `mount_hunt_crew_split(parent, band, herd, quarry, kit_id, party_workers)`** — the
# partly-equipped party's line, *"⚠ 4 of your 6 hunters can take Woolly Mammoth; the other 2 hold too
# little gear and land nothing on it at any headcount."* (issue #520). Both compose hosts mounted it
# from the hunt gate's `else`, i.e. on a fight the party COULD make.
#
# Reported from play, on a sheet carrying it directly under `KitRoster.shortfall_line`'s
# *"1 of 15 Stalking kits available"*: *"The yellow message mentions the hunters. I'm not sure we need
# that message at all, we have the stalking kit message, it seems the second is redundant, you can
# remove it."* Two sentences about one shortfall, on one sheet.
#
# **THE GATE'S REFUSAL IS NOT THIS AND DID NOT GO WITH IT.** `HUNT_GATE_META` and the DANGER line it
# tags are untouched: a fight the party cannot make at all still says so. What was removed is the
# CLEARED-gate half. `SourceForecast.hunt_crew_split_model` went with the mount, and
# `labor-ui.md` → "RETIRED — the hunt crew-split sentence" records what the removal costs.

## The three handles a stacked action button's parts are found by — the CELL a caller mounts, the
## first line whose text a patch rewrites, and the second line a patch hands to the summary's own
## updater. They are metas rather than child indices because the face is three containers deep and a
## positional read would be asserting against this builder's nesting rather than against the parts.
const STACKED_ACTION_CELL_META := "stacked_action_cell"
const STACKED_ACTION_LABEL_META := "stacked_action_label"
const STACKED_ACTION_BODY_META := "stacked_action_body"

## ⛔ **A TILE-CARD ACTION BUTTON WHOSE FACE IS TWO LINES — the label over its own readout.**
##
## Ray, on the deposit drawer: *"See the 2 foresters +0.60 wood. That looks strange there, I know that
## is the existing pattern with foragers and hunters, maybe roads, not sure. But I think that would
## look more at home inside the button. We should make it the second line on the button."* So the
## standing summary stops being a SIBLING ROW above the button and becomes the button's own second
## line, on every web at once — the pattern is what he is objecting to, and two shapes for one readout
## is the inconsistency this rework keeps removing.
##
## **IT IS `_crew_target_pill`'s MECHANISM, DELIBERATELY.** Two font sizes cannot live in one
## `Button.text` and a `\n` cannot carry two of them either, so the control is a `MarginContainer`
## CELL holding an EMPTY-`text` `Button` with the face painted over it — which is the shape the crew
## pills, the policy rungs and the two-line steppers already take, for the same reason.
##
## **THE CELL IS WHAT SIZES THE CONTROL**, which is why the parent must be a `MarginContainer` and not
## the `Button`: a `Button` is not a `Container` and would not grow to fit children. The button keeps
## its stylebox, its click, its focus, its disabled state and its `tooltip_text`; what it loses is its
## text.
##
## ⛔ **EVERY CONTROL IN THE FACE IS `MOUSE_FILTER_IGNORE`, and that is not decoration — it is what
## keeps the whole button pressable.** A `Label` defaults to IGNORE, so a plain face is safe by
## accident; a summary's note labels have been through `set_label_tooltip`, which sets **STOP** for
## exactly the opposite reason (a Label's own `tooltip_text` is a silent no-op otherwise), and each of
## those would be a dead patch over the button. `mute_button_face` is therefore called on the whole
## subtree here AND again by any patch path that re-runs `set_label_tooltip` on it.
##
## **SO THE TOOLTIP MOVES TO THE BUTTON.** With the face muted, the summary's own hover cannot fire;
## a `Button` shows `tooltip_text` natively and is the whole control's hit target, so the caller sets
## it there. The flow's own `tooltip_text` is left in place as well — it costs nothing and it is what
## the harnesses read the summary's hover off.
##
## `second_line` is `null` on a source nobody works, and the face is then ONE line with no blank
## second one: a summary exists only where this faction already works the source.
static func build_stacked_action_button(btn: Button, label_text: String,
        second_line: Control = null) -> MarginContainer:
    btn.text = ""
    var cell := MarginContainer.new()
    cell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    cell.set_meta(STACKED_ACTION_CELL_META, true)
    cell.add_child(btn)
    var pad := MarginContainer.new()
    pad.mouse_filter = Control.MOUSE_FILTER_IGNORE
    pad.add_theme_constant_override("margin_left", HudStyle.BUTTON_PADDING_H)
    pad.add_theme_constant_override("margin_right", HudStyle.BUTTON_PADDING_H)
    pad.add_theme_constant_override("margin_top", HudStyle.BUTTON_PADDING_V)
    pad.add_theme_constant_override("margin_bottom", HudStyle.BUTTON_PADDING_V)
    var face := VBoxContainer.new()
    face.add_theme_constant_override("separation", HudComposeVocab.TILE_ACTION_FACE_SEPARATION)
    var label := Label.new()
    label.text = label_text
    label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    label.add_theme_font_size_override("font_size", HudComposeVocab.TILE_ACTION_LABEL_FONT_SIZE)
    label.set_meta(STACKED_ACTION_LABEL_META, true)
    face.add_child(label)
    if second_line != null:
        second_line.set_meta(STACKED_ACTION_BODY_META, true)
        # **CENTRED UNDER THE LABEL, which is centred** — a SHORT summary
        # (`♻ 2 foresters · +0.60 /turn`) left against the button's left edge under a centred label
        # reads as two unrelated things in one box.
        #
        # ⛔ **AND A WRAPPING CONTAINER IS CENTRED BY ITS OWN `alignment`, NEVER BY `SHRINK_CENTER`.**
        # The standing summary is an `HFlowContainer`; shrinking it hands it its MINIMUM width — the
        # widest single child — so every part after the widest one wraps onto a line of its own, and
        # a lone `⚠` under `💀 3 hunters · +0.63 /turn` is the shape that produces. Keeping the flow
        # full width and centring its content keeps the wrap where the button's width puts it.
        if second_line is FlowContainer:
            (second_line as FlowContainer).alignment = FlowContainer.ALIGNMENT_CENTER
        else:
            second_line.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
        face.add_child(second_line)
    pad.add_child(face)
    cell.add_child(pad)
    mute_button_face(pad)
    return cell

## Make a stacked button's whole painted face inert, and size its SECOND LINE as one readout.
##
## ⛔ **RE-RUN IT AFTER ANY PATCH THAT TOUCHES THE FACE.** `set_label_tooltip` sets
## `MOUSE_FILTER_STOP`, and the summary's in-place patch path calls it on every note label it
## rewrites — so a face muted only at build time grows a dead patch over the button the first time a
## note's text moves. Nothing else in the face can re-arm itself.
##
## **THE FONT SIZE RIDES HERE RATHER THAN IN THE SUMMARY'S OWN BUILDERS**, because those builders are
## shared with the sibling-row hosts (the work inspector, the parties strip) that want their own
## registers: `build_status_part` pins `HudWorkVocab.ALLOC_SECTION_FONT_SIZE` and
## `build_row_note_label` pins nothing at all, i.e. the stock 16. Inside a button the second line is
## ONE readout and reads as one size.
static func mute_button_face(root: Control) -> void:
    if root == null:
        return
    root.mouse_filter = Control.MOUSE_FILTER_IGNORE
    if root is Label and not root.has_meta(STACKED_ACTION_LABEL_META):
        (root as Label).add_theme_font_size_override("font_size",
            HudComposeVocab.TILE_ACTION_SUMMARY_FONT_SIZE)
    for child in root.get_children():
        if child is Control:
            mute_button_face(child as Control)

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
## **BUILT IN `apply_palette`, NOT HERE** — its values are themed `HudStyle` colours, which are
## `static var`s: a `const` table is a parse error against one, and a static-var initializer would
## freeze at whatever palette was loaded before the theme was installed.
static var VERDICT_SEVERITY_COLORS := {}

## Install the current `HudStyle` palette into this file's one themed table. Called by
## `HudPalette.apply()` after `HudStyle.apply_palette`.
static func apply_palette() -> void:
    VERDICT_SEVERITY_COLORS = {
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
## The compose sheet's rot bullet (#706) carries this meta with its own text, so a harness reads what
## was drawn under the PER TURN numbers.
const WORK_PARTY_ROT_META := &"work_party_rot"

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
## `note` is the take's own qualifier (`· renewable`, or the overdraw sentence); it sits in the row's
## own flow at the unit's size in the tint the caller resolved, so a warning never has to compete with
## the number it is warning about.
##
## **THE HEADER CARRIES THE UNIT AND THE ARROW'S KEY, so neither is repeated per account.** It is a
## `VBoxContainer` now rather than the bare flow — the flow keeps `YIELDS_ROW_META`, so everything
## that reaches for the row by identity still finds the readings and not the caption over them.
##
## `header` OVERRIDES that caption for a caller whose readings are not a per-turn rate at all — the
## raid's whole-trip payload, which has no `/turn` and no holding state to arrow toward.
##
## **THE `while_building` KEY IS RETIRED WITH THE DIP** (`docs/plan_standing_upkeep.md` §2.2). It said
## *these readings are the DIPPED take*; a build has its own crew now, so the readings are the plain
## take whether or not a rung is going up. `has_after` is the only fact left that can key the caption,
## and it is read from the ROWS here — the one place that knows — so the caption and the marks under
## it cannot be composed separately.
## `header_suffix` QUALIFIES the resolved caption instead of replacing it — which `header` cannot do,
## the two states it forks on (`now → after` or not) being resolved in here off the rows. The hunt
## sheet's *" · at the likely take"* is what wants it: those readings are single numbers under a take
## line that carries a band, and the caption is the only thing that can say which point of it they are
## quoted at. It is appended to a caller-supplied `header` too, so the two never fight.
static func build_yields_row(rows: Array, number_tint: Color, note: String, note_tint: Color,
        header: String = "", header_suffix: String = "") -> VBoxContainer:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", HudComposeVocab.READOUT_YIELD_V_SEPARATION)
    var has_after := rows.any(func(row: Dictionary) -> bool:
        return row.has(SourceForecast.YIELD_ROW_AFTER))
    var caption := (header if header != "" \
        else SourceForecast.yield_row_header(has_after)) + header_suffix
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

## The readout's small-print Label — the unit, the route, the take's qualifier all
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


# ---- THE SPECIES CHIPS — which plants a forage crew carries home ---------------------------------
#
# `HudFloraVocab` owns the words and the two states; this owns the CONTROL. One chip per named plant in
# the tile's basket, in a wrapping flow so a five-plant basket costs a second line rather than a
# horizontal scroll.
#
# **A CHIP IS A `MarginContainer` CELL, NOT A `Button`, and for the policy rung's structural reason**:
# a `Button` is not a `Container`, so it will not grow to fit a child, and the face is a child here
# because its type size and its ink are set independently of the box's. The Button underneath keeps the
# box, the click, the focus and the disabled state; the face is painted over it at
# `MOUSE_FILTER_IGNORE` so the press reaches through.
#
# **THE PILL IS THE WHOLE AFFORDANCE — there is no mark.** A filled, bordered pill says *selected* and
# plain text says *not*, which is one signal for one fact; a checkbox glyph beside it was a second
# saying the same thing, and it went with the third state it was drawn to distinguish
# (`HudFloraVocab` → `TAKE_STATE_SELECTED`). The unselected chip draws NO box at rest —
# `HudStyle.apply_pill_toggle`, not `apply_pill_button`, whose resting chrome would leave every chip
# in a box and make the selection a fill difference rather than a presence one.
#
# **THE FACE IS INSET BY THE PILL'S OWN PADDING**, read off `HudStyle` rather than named again here.
# The Button's stylebox content margins apply to `Button.text`, which is empty — so the drawn pill is
# sized by the overlay's minimum, and an unpadded overlay put the pill's right edge exactly on the
# face's closing parenthesis.
const SPECIES_CHIP_META := "species_chip"
## The chip's state (`HudFloraVocab.TAKE_STATE_*`), so a harness reads the selection off the CONTROL
## rather than off ink it cannot measure.
const SPECIES_CHIP_STATE_META := "species_chip_state"
## The row itself, so an assertion can find it without matching a plant's display name.
const SPECIES_CHIP_ROW_META := "species_chip_row"

## Build the chip row. Each `entries` element is `{species, face, state}`; `on_toggle` fires with the
## species key. WHICH chips are lit is the caller's answer, since only it knows whether the ground is
## being committed — and a chip carries no tooltip, its face already being the whole of what it knows.
## The chip entry key carrying why the chip is disabled — `""` (absent) for a live chip.
const SPECIES_CHIP_DISABLED_REASON_KEY := "disabled_reason"

static func build_species_chips(entries: Array, on_toggle: Callable) -> HFlowContainer:
    var row := HFlowContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.set_meta(SPECIES_CHIP_ROW_META, true)
    row.add_theme_constant_override("h_separation", HudFloraVocab.TAKE_CHIP_SEPARATION)
    row.add_theme_constant_override("v_separation", HudFloraVocab.TAKE_CHIP_SEPARATION)
    for entry_variant in entries:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        var species := String(entry.get("species", ""))
        if species == "":
            continue
        row.add_child(_species_chip(entry, species, on_toggle))
    return row

static func _species_chip(entry: Dictionary, species: String,
        on_toggle: Callable) -> MarginContainer:
    var state := String(entry.get("state", HudFloraVocab.TAKE_STATE_SELECTED))
    var selected := state == HudFloraVocab.TAKE_STATE_SELECTED
    var btn := Button.new()
    btn.text = ""
    btn.set_meta(SPECIES_CHIP_META, species)
    btn.set_meta(SPECIES_CHIP_STATE_META, state)
    HudStyle.apply_pill_toggle(btn, selected)
    btn.pressed.connect(func() -> void: on_toggle.call(species))
    # **A CHIP THAT CAN DO NOTHING IS DISABLED AND SAYS WHY** (`selection-card.md` → "A DISABLED
    # CONTROL SAYS WHY") — the entry's `disabled_reason`, `""` on every live chip.
    var reason := String(entry.get(SPECIES_CHIP_DISABLED_REASON_KEY, ""))
    if reason != "":
        btn.disabled = true
        btn.tooltip_text = disabled_tooltip(PackedStringArray([reason]), "")
    var face := Label.new()
    face.text = String(entry.get("face", ""))
    face.mouse_filter = Control.MOUSE_FILTER_IGNORE
    face.add_theme_color_override("font_color",
        HudStyle.INK if selected else HudStyle.INK_DIM)
    face.add_theme_font_size_override("font_size", HudFloraVocab.TAKE_CHIP_FONT_SIZE)
    var pad := MarginContainer.new()
    pad.mouse_filter = Control.MOUSE_FILTER_IGNORE
    pad.add_theme_constant_override("margin_left", HudStyle.PILL_PADDING_H)
    pad.add_theme_constant_override("margin_right", HudStyle.PILL_PADDING_H)
    pad.add_theme_constant_override("margin_top", HudStyle.PILL_PADDING_V)
    pad.add_theme_constant_override("margin_bottom", HudStyle.PILL_PADDING_V)
    pad.add_child(face)
    var cell := MarginContainer.new()
    cell.add_child(btn)
    cell.add_child(pad)
    return cell
