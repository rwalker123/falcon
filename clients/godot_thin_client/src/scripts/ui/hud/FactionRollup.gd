class_name FactionRollup
extends RefCounted

## **ALL-`static`, STATELESS** — the FACTION PAGE's three zone builders (issue #450), the all-band
## rollup the Band/City dock's cycler pins as its first entry.
##
## **WHY A SHARED LAYER RATHER THAN A CONTROLLER.** This page is a READOUT: no steppers, no compose
## sheet, no open/closed row, nothing that survives a snapshot. It therefore has no per-cluster state
## to own, which is the whole of what makes a controller a controller (`hud-modules.md`). What it does
## have is arithmetic over the player's whole roster, which is exactly the shape the `SourceForecast` /
## `HudFormat` / `DetailFormat` layers already hold — so it lands beside them, with the one piece of
## state it needs threaded in as a PARAMETER: the `HudBandLaborState` model itself, which is where
## every per-snapshot fact this page sums already lives.
##
## **IT RE-DERIVES NOTHING.** Every total is a SUM over answers the per-band surfaces already give —
## `DetailFormat.band_net_food` / `band_provisions` / `band_trade_stock`,
## `HudBandLaborState.effective_idle` / `effective_role_workers` / `band_party_workers`,
## `TopBarReadouts.faction_tracks` — so a band's own page and this one can never disagree about a
## number. A rollup that computed its own food ledger would be a second source of truth for the
## identity `larder_delta == income − consumption − pen_feed − raid_forfeit` the whole food arc keeps
## closed.
##
## **THE PAGE IS READ-ONLY, DELIBERATELY.** The issue's scope is "counts and where they are, not
## per-worker controls": role steppers, labor assignment and the compose sheets stay on the per-band
## pages, and the cycler is the way to reach a band from here. Nothing on this page emits a signal, so
## none is declared — which is also why the module can be `static` at all.

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

## **A STAT ROW TAKES ITS SIZE FROM THE ROW IT IS THE COUNTERPART OF, and the band zone's counterpart
## is the vitals label — which sets NO font-size override at all.** `BandPanelController._build_vitals_label`
## builds a bare `RichTextLabel`, so `Food` / `Trade` / `Morale` render at the stock default; a faction
## `Larder` row pinned at some smaller number reads as a different KIND of thing beside them, which is
## exactly how this shipped first (at 12, four steps under its own counterpart). Passing this sentinel
## means "set no override", so the two track that default TOGETHER rather than through a literal
## somebody has to keep in step — the client has no `Theme` and `Typography.gd` is a no-op shim, so
## there is no other way to say "the same size as an un-overridden Label".
const STAT_ROW_INHERIT_FONT_SIZE := 0

## The gap between a stat row's key and its value. The value is right-aligned by an expanding spacer,
## exactly as `HudWidgets.zone_head` right-aligns its readout, so this is a floor rather than the gap.
const STAT_ROW_SEPARATION := 8

## The knowledge meter's cell count on this page. It reads `TopBarReadouts`' own rather than declaring
## a second one: the top bar and this page draw the SAME track at the same resolution, and two
## constants is how they come to disagree about what half-learned looks like.
const KNOWLEDGE_METER_CELLS := TopBarReadouts.KNOWLEDGE_METER_CELLS

## The knowledge row's `<bar> <percent>%` value.
const KNOWLEDGE_VALUE_FORMAT := "%s %d%%"

# ---- zone builders ----------------------------------------------------------

## Zone `band` — WHO THE FACTION IS: its people, its stores and what it keeps.
##
## The layout mirrors a band's own band zone one rung up — a stacked PEOPLE bar over the vitals the
## bar does not state — so a player cycling off a band onto this page is reading the same shapes at a
## different scale.
##
## **THERE IS NO HEIGHT TIER HERE, and that is a measurement rather than an oversight.** A band's zone
## yields by tier because its chart and its role cards are the two biggest blocks in the client's most
## height-capped box; this page's four blocks measure ~260px of the ~300px a horizontal dock offers,
## so there is nothing to give up. **Re-measure before adding a fifth block** — `band_panel_preview`'s
## `_report_zone_content_extent` prints the number, and the band zone has been at the edge twice.
static func build_band_zone(labor: HudBandLaborState) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    var bands := labor.player_bands()
    var people := _build_people_block(bands)
    if people != null:
        col.add_child(people)
    col.add_child(_build_food_block(bands))
    col.add_child(_build_trade_block(bands))
    col.add_child(_build_herds_block(labor))
    return col

## Zone `work` — WHAT THE FACTION IS DOING: the whole workforce as one bar, where those hands are
## band by band, and what the faction's craft knowledge is.
##
## **KNOWLEDGE LIVES HERE RATHER THAN IN THE BAND ZONE, and that is the one placement worth
## defending.** A track is not a stock and not a population — it is what the faction's hands may
## ATTEMPT, and every rung it gates is a row on a work board. Putting it beside the workforce also
## keeps the band zone inside a horizontal dock's box without a tier gate (see `build_band_zone`),
## where a five-row knowledge block would have forced one.
static func build_work_zone(labor: HudBandLaborState, knowledge: Dictionary) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    col.add_child(_build_workforce_block(labor))
    col.add_child(_build_bands_block(labor))
    var tracks := _build_knowledge_block(knowledge)
    if tracks != null:
        col.add_child(tracks)
    return col

## Zone `parties` — WHO IS OUT: every detached party across every band, and which band it went from.
##
## `herd_label_for_id` is the caller's own herd-name resolver, threaded in the way
## `HudFormat.panel_expedition_summary` (which this reuses verbatim) already takes it — a stateless
## layer must not reach for the roster, the selection and the herd list that resolver reads.
static func build_parties_zone(labor: HudBandLaborState, herd_label_for_id: Callable) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    var parties := labor.player_expeditions()
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_PARTIES,
        str(parties.size()) if not parties.is_empty() else ""))
    if parties.is_empty():
        block.add_child(HudWidgets.alloc_hint_label(HudWorkVocab.FACTION_PARTIES_EMPTY))
        col.add_child(block)
        return col
    # The party's HOME BAND is the "where they are" half of the issue's ask that a summary row can
    # actually carry: a party's own tile changes every turn and means nothing without the map, while
    # the band it left is what the player cycles to in order to act on it.
    var names := _band_names_by_entity(labor)
    var shown: int = mini(parties.size(), HudWorkVocab.FACTION_LIST_ROWS_MAX)
    for i in range(shown):
        var party: Dictionary = parties[i]
        block.add_child(_stat_row(
            HudFormat.panel_expedition_summary(party, herd_label_for_id),
            String(names.get(int(party.get("home_band_entity", -1)), "")),
            HudStyle.INK_FAINT, HudWorkVocab.WORK_ROW_FONT_SIZE))
    _append_more_row(block, parties.size() - shown)
    col.add_child(block)
    return col

# ---- band zone blocks -------------------------------------------------------

## The faction's PEOPLE bar: the same children/working/elders stack a band's own zone draws, summed
## across every band and apportioned ONCE at the end.
##
## **THE BRACKETS ARE SUMMED FRACTIONAL AND APPORTIONED ONCE, never apportioned per band and added.**
## Rounding each band to whole people first and summing the results reproduces the very off-by-one
## `HudFormat.apportion_people` exists to remove, one band at a time: four bands each carrying a `.5`
## remainder lose two people between the bar and the total beside it. Summing first leaves one
## remainder to distribute, which is the case that function was written for.
##
## Returns null when no band carries an age structure at all, so the block is OMITTED rather than
## rendered from a fabricated split — the band zone's own rule.
static func _build_people_block(bands: Array) -> VBoxContainer:
    var raw: Array[float] = [0.0, 0.0, 0.0]
    for band_variant in bands:
        if not (band_variant is Dictionary):
            continue
        var band: Dictionary = band_variant
        raw[0] += float(band.get("age_children", 0.0))
        # `age_working` is the age COHORT and `working_age` the count of ASSIGNABLE workers — two
        # quantities that track each other. The band zone falls back to the latter when the cohort
        # field is absent, and this sum must fall back the same way or a mixed roster would drop a
        # whole band's middle bracket out of the faction total.
        var working := float(band.get("age_working", 0.0))
        raw[1] += working if working > 0.0 else float(band.get("working_age", 0))
        raw[2] += float(band.get("age_elders", 0.0))
    var whole := HudFormat.apportion_people(raw)
    var children: int = whole[0]
    var working_whole: int = whole[1]
    var elders: int = whole[2]
    var total := children + working_whole + elders
    if total <= 0:
        return null
    var segments: Array = []
    if children > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_CHILDREN, "count": children,
            "color": HudStyle.VOICE_PIGMENT,
            "tooltip": "%d %s" % [children, HudWorkVocab.PEOPLE_LABEL_CHILDREN]})
    if working_whole > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_WORKING, "count": working_whole,
            "color": HudStyle.INK_DIM,
            "tooltip": "%d %s" % [working_whole, HudWorkVocab.PEOPLE_LABEL_WORKING]})
    if elders > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_ELDERS, "count": elders,
            "color": HudStyle.VOICE_INK,
            "tooltip": "%d %s" % [elders, HudWorkVocab.PEOPLE_LABEL_ELDERS]})
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_PEOPLE, str(total)))
    block.add_child(HudWidgets.build_composition_bar(segments))
    # The band's page spends this trailing slot on its dependency count. **The faction's dependency
    # ratio is deliberately NOT restated here**: dependents are fed per BAND, so a faction average
    # hides the band that is in trouble — the same reasoning that took the figure off the top bar.
    # What the slot carries instead is the one thing a TOTAL needs, which is how many bands it spans.
    block.add_child(HudWidgets.build_composition_key(segments, _bands_chip(bands.size())))
    return block

## The faction larder: what is stored, headed by what it gains or loses a turn.
##
## **THE LEDGER IS NOT BROKEN OUT, and that is the band page's own shape rather than a cut for room.**
## A band states `Food: 74 (93 turns) · -0.81 /turn` on ONE line and puts Gathered / Hunted / Eaten /
## Pen feed behind a disclosure popover; this page had grown a four-row inline ledger the per-band
## surface deliberately does not have. Both figures the rollup owes are still here — the STOCK on the
## row and the RATE on the head — and the breakdown is one cycle away, on the band that owns it.
##
## Measured, that is also what keeps the zone inside a horizontal dock: the four-row form read **328px
## of a 300px box** at the vitals type size, and this one reads 247. **Every row here is now
## unconditional**, so that figure is the zone's height rather than its best case.
##
## **THERE IS NO FACTION-WIDE RUNWAY**, the `(93 turns)` a band's own row carries. Turns-of-food is a
## property of one larder against one band's drain; averaged across bands it hides the band that is
## starving behind the ones that are not — the same reason the top bar's dependency figure was taken
## off it.
static func _build_food_block(bands: Array) -> VBoxContainer:
    var larder := 0.0
    var net := 0.0
    for band_variant in bands:
        if not (band_variant is Dictionary):
            continue
        var band: Dictionary = band_variant
        larder += DetailFormat.band_provisions(band)
        # **THE NET IS SUMMED FROM EACH BAND'S OWN `band_net_food`**, never recomposed from separate
        # income and drain totals — so this page can never quote a net the band pages do not add up to.
        # It carries all four terms of the larder identity, `pen_feed` and the EPISODIC `raid_forfeit`
        # included, which is the other reason not to rebuild it out of the two figures on screen.
        net += DetailFormat.band_net_food(band)
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.FACTION_HEADER_FOOD,
        SourceForecast.format_signed(net), null,
        HudStyle.HEALTHY if net >= 0.0 else HudStyle.DANGER))
    block.add_child(_stat_row(HudWorkVocab.FACTION_ROW_LARDER,
        SourceForecast.format_stock(larder), HudStyle.INK))
    return block

## The faction's trade goods: the stock its bands hold between them, and what they earn a turn.
##
## Rendered UNCONDITIONALLY, at `+0.00` when the faction earns none — the per-band Trade row's own
## rule, and for its reason: a row that vanished at zero read in playtest as "this cannot trade at
## all" rather than "it earns none right now".
static func _build_trade_block(bands: Array) -> VBoxContainer:
    var stock := 0.0
    var rate := 0.0
    for band_variant in bands:
        if not (band_variant is Dictionary):
            continue
        var band: Dictionary = band_variant
        stock += DetailFormat.band_trade_stock(band)
        rate += DetailFormat.band_trade_income(band)
    var block := HudWidgets.make_zone_block()
    # The stock carries ONE decimal, the per-band Trade row's own rule: the sim accumulates sub-unit
    # trade income rather than rounding it off each turn, so an integer readout would show a `0` stuck
    # for ~100 turns beside a visibly non-zero rate.
    block.add_child(HudWidgets.zone_head(HudWorkVocab.FACTION_HEADER_TRADE,
        HudWorkVocab.FACTION_TRADE_STOCK_FORMAT % stock))
    block.add_child(_stat_row(HudWorkVocab.FACTION_ROW_PER_TURN,
        SourceForecast.format_signed(rate), HudStyle.INK))
    return block

## What the faction KEEPS: the managed herds its bands staff, and how many of those are penned.
##
## **A KEPT HERD IS ONE THE SIM ASKS FOR KEEPERS ON** (`herders_needed > 0`), resolved through the
## bands' own hunt assignments — which is what makes the count unambiguously THIS faction's. A wild
## herd being hunted is not kept, and `world_herds()` alone cannot answer whose a managed herd is.
## The herd is re-resolved LIVE through `find_world_herd`, never read off the assignment's launch-time
## target: herds migrate, and the rung travels with the animals.
static func _build_herds_block(labor: HudBandLaborState) -> VBoxContainer:
    var seen := {}
    var pens := 0
    for band_variant in labor.player_bands():
        if not (band_variant is Dictionary):
            continue
        for assignment_variant in HudBandLaborState.labor_assignments_of(band_variant as Dictionary):
            if not (assignment_variant is Dictionary):
                continue
            var assignment: Dictionary = assignment_variant
            if String(assignment.get("kind", "")) != SourceForecast.LABOR_KIND_HUNT:
                continue
            var herd_id := String(assignment.get("fauna_id", "")).strip_edges()
            if herd_id.is_empty() or seen.has(herd_id):
                continue
            var herd := labor.find_world_herd(herd_id)
            if herd.is_empty() or int(herd.get("herders_needed", 0)) <= 0:
                continue
            seen[herd_id] = true
            if bool(herd.get("corralled", false)):
                pens += 1
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.FACTION_HEADER_HERDS, str(seen.size())))
    block.add_child(_stat_row(HudWorkVocab.FACTION_ROW_PENS, str(pens), HudStyle.INK))
    return block

# ---- work zone blocks -------------------------------------------------------

## The whole faction's workforce as ONE stacked bar — the band zone's WORKFORCE block summed, in the
## same saturated palette and the same segment order, so the two read as one chart at two scales.
##
## It carries no role CARDS: those are the steppers this page deliberately does not have.
static func _build_workforce_block(labor: HudBandLaborState) -> VBoxContainer:
    var idle := 0
    var forage_workers := 0
    var hunt_workers := 0
    var role_workers := 0
    var party_workers := 0
    var working_age := 0
    for band_variant in labor.player_bands():
        if not (band_variant is Dictionary):
            continue
        var band: Dictionary = band_variant
        idle += labor.effective_idle(band)
        working_age += int(band.get("working_age", 0))
        # The PENDING-AWARE worker map, exactly as the band zone reads it — a just-issued assign must
        # count here too, or cycling to this page would undo the optimistic feedback the dock gives.
        var merged := labor.effective_worker_map(band)
        for key in merged:
            var model: Dictionary = merged[key]
            var workers := int(model.get("workers", 0))
            match String(model.get("kind", "")):
                SourceForecast.LABOR_KIND_FORAGE: forage_workers += workers
                SourceForecast.LABOR_KIND_HUNT: hunt_workers += workers
        role_workers += int(labor.effective_role_workers(band, HudConst.LABOR_KIND_SCOUT).get("workers", 0)) \
            + int(labor.effective_role_workers(band, HudConst.LABOR_KIND_WARRIOR).get("workers", 0))
        party_workers += labor.band_party_workers(band)
    var segments: Array = []
    for spec in [
        [HudWorkVocab.WORKFORCE_KEY_FORAGE, forage_workers, HudStyle.HEALTHY],
        [HudWorkVocab.WORKFORCE_KEY_HUNT, hunt_workers, HudStyle.SIGNAL],
        [HudWorkVocab.WORKFORCE_KEY_ROLES, role_workers, HudStyle.VOICE_INK],
        [HudWorkVocab.WORKFORCE_KEY_PARTIES, party_workers, HudStyle.WARN],
        [HudWorkVocab.WORKFORCE_KEY_IDLE, idle, HudStyle.INK_FAINT],
    ]:
        if int(spec[1]) > 0:
            segments.append({"key": String(spec[0]), "count": int(spec[1]), "color": spec[2],
                "tooltip": "%s: %d" % [String(spec[0]), int(spec[1])]})
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_WORKFORCE,
        HudWorkVocab.WORKFORCE_IDLE_FORMAT % [idle, working_age],
        null, HudStyle.SIGNAL if idle > 0 else HudStyle.INK_DIM))
    if not segments.is_empty():
        block.add_child(HudWidgets.build_composition_bar(segments))
        block.add_child(HudWidgets.build_composition_key(segments))
    return block

## WHERE those hands are: one row per band, its workforce and its idle count.
##
## Idle is tinted SIGNAL on a band with hands to spend, exactly as the WORKFORCE head above it tints
## its own idle readout — this row is the reason the player cycles to that band, so it must be
## findable at a glance rather than read row by row.
static func _build_bands_block(labor: HudBandLaborState) -> VBoxContainer:
    var bands := labor.player_bands()
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_WORK, str(bands.size())))
    var shown: int = mini(bands.size(), HudWorkVocab.FACTION_LIST_ROWS_MAX)
    for i in range(shown):
        var band: Dictionary = bands[i]
        var idle := labor.effective_idle(band)
        block.add_child(_stat_row(
            HudFormat.band_display_name(band, i + 1),
            HudWorkVocab.FACTION_BAND_ROW_FORMAT % [int(band.get("working_age", 0)), idle],
            HudStyle.SIGNAL if idle > 0 else HudStyle.INK_FAINT,
            HudWorkVocab.WORK_ROW_FONT_SIZE))
    _append_more_row(block, bands.size() - shown)
    return block

## The faction's craft knowledge — one row per track being learned, in the intensification ladder's
## own order, off the SAME `TopBarReadouts.faction_tracks` row the top-bar strip and every rung gate
## read. A finished track reads the word rather than a full meter.
##
## **A track the faction has not begun is HIDDEN**, the top-bar strip's own rule: the snapshot row is
## sparse and an unstarted rung is noise. Every track unstarted ⇒ no block at all, rather than a
## heading over nothing.
static func _build_knowledge_block(knowledge: Dictionary) -> VBoxContainer:
    var rows: Array = []
    for track in TopBarReadouts.KNOWLEDGE_TRACK_LABELS:
        var progress := float(knowledge.get(track, 0.0))
        if progress <= 0.0:
            continue
        rows.append([String(TopBarReadouts.KNOWLEDGE_TRACK_LABELS[track]), progress])
    if rows.is_empty():
        return null
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.FACTION_HEADER_KNOWLEDGE, ""))
    for row in rows:
        var progress: float = row[1]
        var known := progress >= HudConst.KNOWLEDGE_COMPLETE
        var value := HudWorkVocab.FACTION_KNOWLEDGE_KNOWN if known \
            else KNOWLEDGE_VALUE_FORMAT % [
                HudFormat.meter_bar(progress, KNOWLEDGE_METER_CELLS),
                HudFormat.progress_percent(progress)]
        block.add_child(_stat_row(String(row[0]), value,
            HudStyle.SIGNAL if known else HudStyle.INK_DIM, HudWorkVocab.WORK_ROW_FONT_SIZE))
    return block

# ---- leaves -----------------------------------------------------------------

## One `key ……… value` readout row: a dim key, an expanding spacer, then the value in its own tint.
##
## The same shape `HudWidgets.zone_head` gives a section head, one type size down — this page is a
## column of readouts, and a row that measured like a head would leave the section heads meaningless.
## Both labels route their tooltip through `HudWidgets.set_label_tooltip`, since a bare `tooltip_text`
## on a `Label` is a SILENT no-op at Godot's `MOUSE_FILTER_IGNORE` default.
## `font_size` is `STAT_ROW_INHERIT_FONT_SIZE` for a VITALS row (the band zone's, which must read at
## the size the band page's own vitals do) and an explicit size for a LIST row (the work zone's, which
## are board rows and take `HudWorkVocab.WORK_ROW_FONT_SIZE` for it). The page therefore carries the
## same two-size hierarchy the band page does, in the same two zones.
static func _stat_row(key: String, value: String, value_color: Color,
        font_size: int = STAT_ROW_INHERIT_FONT_SIZE, tooltip: String = "") -> HBoxContainer:
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", STAT_ROW_SEPARATION)
    var key_label := Label.new()
    key_label.text = key
    if font_size != STAT_ROW_INHERIT_FONT_SIZE:
        key_label.add_theme_font_size_override("font_size", font_size)
    key_label.add_theme_color_override("font_color", HudStyle.INK_DIM)
    # **NO `clip_text` ON THE KEY.** It zeroes a Label's minimum width, and the spacer beside it is the
    # row's only expanding child — so a clipped key is squeezed to NOTHING and the row renders as a
    # right-aligned number with no name at all. That shipped once and is invisible to the zone-bounds
    # and content-fits assertions, both of which a zero-width Label passes comfortably.
    HudWidgets.set_label_tooltip(key_label, tooltip)
    row.add_child(key_label)
    var spacer := Control.new()
    spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
    row.add_child(spacer)
    var value_label := Label.new()
    value_label.text = value
    if font_size != STAT_ROW_INHERIT_FONT_SIZE:
        value_label.add_theme_font_size_override("font_size", font_size)
    value_label.add_theme_color_override("font_color", value_color)
    HudWidgets.set_label_tooltip(value_label, tooltip)
    row.add_child(value_label)
    return row

## The PEOPLE key's trailing chip: how many bands the bar is summed over.
static func _bands_chip(count: int) -> Control:
    var chip := Label.new()
    chip.text = HudWorkVocab.FACTION_BANDS_CHIP_ONE if count == 1 \
        else HudWorkVocab.FACTION_BANDS_CHIP_FORMAT % count
    chip.add_theme_font_size_override("font_size", HudWorkVocab.COMPOSITION_KEY_FONT_SIZE)
    chip.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    chip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    chip.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    return chip

## **NO SILENT CAPS.** Both lists on this page are bounded (the zones clip and neither list pages), so
## what the bound drops is STATED. A truncated list with nothing under it reads as the whole roster,
## which is the one way a rollup can lie about a total it is printing directly above.
static func _append_more_row(block: VBoxContainer, remaining: int) -> void:
    if remaining <= 0:
        return
    block.add_child(HudWidgets.alloc_hint_label(HudWorkVocab.FACTION_LIST_MORE_FORMAT % remaining))

## Entity → positional display name, for the parties zone's "which band did this party leave" column.
## The index is the roster's, so a party's home band reads by the SAME name the cycler gives it.
static func _band_names_by_entity(labor: HudBandLaborState) -> Dictionary:
    var names := {}
    var bands := labor.player_bands()
    for i in range(bands.size()):
        var band: Dictionary = bands[i]
        names[int(band.get("entity", -1))] = HudFormat.band_display_name(band, i + 1)
    return names
