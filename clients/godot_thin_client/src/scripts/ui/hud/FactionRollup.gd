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
## **IT IS THE BAND PAGE'S VITALS BLOCK, ONE SCALE UP.** The same five rows in the same order through
## the same `DetailFormat.detail_bbcode` renderer — Food, Trade, Kit, Morale, Growth — so the two
## pages cannot drift into different vocabularies for the same facts. What the faction page spends its
## disclosures on is the thing only it can answer: **which band each total came from**, one clickable
## row per band.
##
## **AN AGGREGATE WHERE ONE IS MEANINGFUL, AN ALERT WHERE IT IS NOT.** A larder and a trade stock sum,
## so those rows carry numbers. A RUNWAY does not — it is one larder against one band's drain — and a
## KIT does not either, being three durabilities per band; so those rows carry `⚠ N bands` and the
## drill-down carries which. Morale and Growth are percentages and go through `FactionAggregate`.
##
## **IT RE-DERIVES NOTHING.** Every figure is a SUM or a weighted mean over answers the per-band
## surfaces already give — `DetailFormat.band_net_food` / `band_provisions` / `band_trade_stock` /
## `band_fertility` / `kit_condition_face`, `HudBandLaborState.effective_idle` — so a band's own page
## and this one can never disagree about a number. A rollup that computed its own food ledger would be
## a second source of truth for the identity
## `larder_delta == income − consumption − pen_feed − raid_forfeit` the whole food arc keeps closed.
##
## **THE PAGE IS READ-ONLY, DELIBERATELY.** The issue's scope is "counts and where they are, not
## per-worker controls": role steppers, labor assignment and the compose sheets stay on the per-band
## pages, and the cycler is the way to reach a band from here. Nothing on this page emits a signal, so
## none is declared — which is also why the module can be `static` at all.

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

## **EVERY ROW ON THIS PAGE IS A ROW UNDER A `zone_head`, AND THE PANEL HAS EXACTLY ONE OTHER SUCH
## THING: THE WORK BOARD.** So a stat row takes the board's own row size, and the page's whole scale is
## the board's — heads at `ZONE_HEAD_FONT_SIZE` (10), key chips at `COMPOSITION_KEY_FONT_SIZE` (11),
## rows at 13.
##
## **The band zone's VITALS LABEL is the wrong counterpart, and matching it was a shipped mistake.**
## `_build_vitals_label` is a bare `RichTextLabel` at the stock default (~16) — but it sits under NO
## head at all, so borrowing its size put a 10pt heading over a 16pt row and made `FOOD` smaller than
## the `Larder` beneath it: a hierarchy this panel never has, and reported on sight. What is shared
## with a surface is its RELATIONSHIP to the thing above it, not its absolute size.
const STAT_ROW_FONT_SIZE := HudWorkVocab.WORK_ROW_FONT_SIZE

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
static func build_band_zone(labor: HudBandLaborState, disclosures: DisclosureController) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    var bands := labor.player_bands()
    col.add_child(_build_vitals_label(bands, disclosures))
    var people := _build_people_block(bands)
    if people != null:
        col.add_child(people)
    return col

## THE FACTION'S VITALS — the band page's own five rows, one scale up, through the band page's own
## renderer. A fresh `RichTextLabel` each render, so its `meta_clicked` is wired here (bound to ITSELF
## as the popover's anchor), exactly as `BandPanelController._build_vitals_label` does.
##
## **AN AGGREGATE WHERE ONE IS MEANINGFUL, AN ALERT WHERE IT IS NOT.** That is the rule the whole page
## is built on, and it is why the rows are not uniform: a larder and a trade stock genuinely SUM, so
## those rows carry numbers; a runway is one larder against one band's drain and a kit condition is
## three durabilities per band, so neither has a faction value to state and those rows carry the
## ALERT instead. The detail is one click away in every case.
static func _build_vitals_label(bands: Array, disclosures: DisclosureController) -> RichTextLabel:
    var label := RichTextLabel.new()
    label.bbcode_enabled = true
    label.fit_content = true
    label.scroll_active = false
    label.autowrap_mode = TextServer.AUTOWRAP_WORD
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    disclosures.wire_label(label)
    var ctx := DetailFormat.Context.new()
    var lines := _faction_summary_lines(bands, disclosures)
    # The carets are read from the CONTROLLER, not from a context the producer filled — every row
    # here registers its own disclosure as it is built, so the state is complete only after the last
    # of them. `BandDetailLines` hit exactly this ordering trap on the merged Growth clause.
    ctx.disclosures = disclosures.state()
    label.text = DetailFormat.detail_bbcode(lines, ctx)
    return label

## The five rows, in the band page's order, registering each row's per-band drill-down as it goes.
static func _faction_summary_lines(bands: Array, disclosures: DisclosureController) -> Array[String]:
    var lines: Array[String] = []
    lines.append(_food_line(bands, disclosures))
    lines.append(_trade_line(bands, disclosures))
    lines.append(_kit_line(bands, disclosures))
    lines.append(_morale_line(bands, disclosures))
    var growth := _growth_line(bands, disclosures)
    if growth != "":
        lines.append(growth)
    return lines

## `Food: 229 · −5.86 /turn` (+ the alert clause). **NO RUNWAY.** Turns-of-food is one larder against
## one band's drain; a faction figure would be an average that hides the band that is starving, so the
## runway stays on the per-band rows and its ALERT is what reaches this one.
static func _food_line(bands: Array, disclosures: DisclosureController) -> String:
    var larder := 0.0
    var net := 0.0
    var rows: Array[String] = []
    var starving := 0
    for i in range(bands.size()):
        var band: Dictionary = bands[i]
        larder += DetailFormat.band_provisions(band)
        # Summed from each band's own `band_net_food`, never recomposed — that identity carries the
        # episodic `raid_forfeit` and the pen feed, and a rebuilt net would drift from the band pages.
        net += DetailFormat.band_net_food(band)
        var turns := float(band.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
        if BandFoodStatus.is_critical(turns):
            starving += 1
        rows.append(_band_row(band, i, "%s · %s" % [
            SourceForecast.format_stock(DetailFormat.band_provisions(band)),
            SourceForecast.format_signed(DetailFormat.band_net_food(band))],
            DetailFormat.food_turns_text(turns)))
    disclosures.register_faction(HudDisclosureVocab.DETAIL_ROW_FOOD,
        HudDisclosureVocab.BREAKDOWN_KIND_FOOD, rows, starving > 0)
    return "%s%s%s · [color=#%s]%s[/color]%s" % [
        HudDisclosureVocab.DETAIL_ROW_FOOD, DetailFormat.DETAIL_KV_SEPARATOR,
        SourceForecast.format_stock(larder),
        HudStyle.HEALTHY_HEX if net >= 0.0 else HudStyle.DANGER_HEX,
        SourceForecast.format_yield(net),
        _alert_clause(starving, HudStyle.DANGER_HEX)]

## `Trade: 15.6 · +0.39 /turn`. Both terms sum and nothing consumes trade goods, so this is the one
## row with no alert to carry — the same reason a band's Trade row is never concerning.
static func _trade_line(bands: Array, disclosures: DisclosureController) -> String:
    var stock := 0.0
    var rate := 0.0
    var rows: Array[String] = []
    for i in range(bands.size()):
        var band: Dictionary = bands[i]
        stock += DetailFormat.band_trade_stock(band)
        rate += DetailFormat.band_trade_income(band)
        rows.append(_band_row(band, i, "%s · %s" % [
            HudWorkVocab.FACTION_TRADE_STOCK_FORMAT % DetailFormat.band_trade_stock(band),
            SourceForecast.format_signed(DetailFormat.band_trade_income(band))], ""))
    disclosures.register_faction(HudDisclosureVocab.DETAIL_ROW_TRADE,
        HudDisclosureVocab.BREAKDOWN_KIND_TRADE, rows, false)
    return "%s%s%s · [color=#%s]%s[/color]" % [
        HudDisclosureVocab.DETAIL_ROW_TRADE, DetailFormat.DETAIL_KV_SEPARATOR,
        HudWorkVocab.FACTION_TRADE_STOCK_FORMAT % stock,
        HudStyle.HEALTHY_HEX if SourceForecast.has_component(rate) else HudStyle.INK_DIM_HEX,
        SourceForecast.format_yield(rate)]

## `Kit: ⚠ 2 bands` / `Kit: all equipped`. **THE DURABILITIES DO NOT AGGREGATE AT ALL** — three per
## band, and a mean of them describes no band that exists — so this row is the alert and nothing else.
## Which band is bare-handed is the fact to act on, and it is one click away.
static func _kit_line(bands: Array, disclosures: DisclosureController) -> String:
    var rows: Array[String] = []
    var dry := 0
    for i in range(bands.size()):
        var band: Dictionary = bands[i]
        if not DetailFormat.band_states_kit(band):
            continue
        var is_dry := DetailFormat.band_kit_is_dry(band)
        if is_dry:
            dry += 1
        rows.append(_band_row(band, i, _kit_face(band),
            HudWorkVocab.FACTION_KIT_DRY_NOTE if is_dry else ""))
    if rows.is_empty():
        return ""
    disclosures.register_faction(HudDisclosureVocab.DETAIL_ROW_KIT,
        HudDisclosureVocab.BREAKDOWN_KIND_KIT, rows, dry > 0)
    var value := _alert_text(dry, HudStyle.DANGER_HEX) if dry > 0 \
        else "[color=#%s]%s[/color]" % [HudStyle.INK_DIM_HEX, HudWorkVocab.FACTION_KIT_ALL_EQUIPPED]
    return "%s%s%s" % [HudDisclosureVocab.DETAIL_ROW_KIT, DetailFormat.DETAIL_KV_SEPARATOR, value]

## `Morale: 78% ▼` (+ the alert clause). Population-weighted through `FactionAggregate`, and the ARROW
## is the weighted mean of the bands' own `morale_delta` — NOT a diff of this mean against last turn's,
## which would swing every time a band is founded or lost and report a trend that never happened.
static func _morale_line(bands: Array, disclosures: DisclosureController) -> String:
    var mean := FactionAggregate.weighted_mean(bands, "morale")
    var delta := FactionAggregate.weighted_mean(bands, "morale_delta")
    var rows: Array[String] = []
    var concerning := 0
    for i in range(bands.size()):
        var band: Dictionary = bands[i]
        if DetailFormat.morale_is_concerning(band):
            concerning += 1
        rows.append(_band_row(band, i, "%d%%%s" % [
            int(round(float(band.get("morale", 0.0)) * 100.0)), _trend_glyph(float(band.get("morale_delta", 0.0)))],
            DetailFormat.morale_cause_label(int(band.get("morale_cause", DetailFormat.MORALE_CAUSE_NONE)))))
    disclosures.register_faction(HudDisclosureVocab.DETAIL_ROW_MORALE,
        HudDisclosureVocab.BREAKDOWN_KIND_MORALE, rows, concerning > 0)
    return "%s%s%d%%%s%s" % [
        HudDisclosureVocab.DETAIL_ROW_MORALE, DetailFormat.DETAIL_KV_SEPARATOR,
        int(round(mean * 100.0)), _trend_glyph(delta),
        _alert_clause(concerning, HudStyle.WARN_HEX)]

## `Growth: 74% of normal` (+ the alert clause). Weighted exactly as Morale is — the two rows must
## agree about what an average of a percentage means, or one of them is lying.
## Renders NOTHING when no band has a projected reading: no data is not a stalled faction.
static func _growth_line(bands: Array, disclosures: DisclosureController) -> String:
    var rows: Array[String] = []
    var concerning := 0
    var total := 0.0
    var weight := 0.0
    for i in range(bands.size()):
        var band: Dictionary = bands[i]
        if not BandFoodStatus.fertility_is_projected(band):
            continue
        var w := FactionAggregate.band_weight(band)
        total += DetailFormat.band_fertility(band) * w
        weight += w
        if DetailFormat.growth_is_concerning(band):
            concerning += 1
        rows.append(_band_row(band, i, HudWorkVocab.FACTION_PERCENT_FORMAT % int(
            round(DetailFormat.band_fertility(band) * 100.0)), ""))
    if rows.is_empty():
        return ""
    disclosures.register_faction(HudDisclosureVocab.DETAIL_ROW_GROWTH,
        HudDisclosureVocab.BREAKDOWN_KIND_GROWTH, rows, concerning > 0)
    return "%s%s%d%% of normal%s" % [
        HudDisclosureVocab.DETAIL_ROW_GROWTH, DetailFormat.DETAIL_KV_SEPARATOR,
        int(round((total / weight) * 100.0)) if weight > 0.0 else 0,
        _alert_clause(concerning, HudStyle.WARN_HEX)]

## One band's three kit conditions, composed from the SAME leaves `BandDetailLines._band_kit_line`
## uses — `kit_condition_face` spells a dry kit as the WORD, and `kit_is_equipped` decides its ink, so
## a drill-down row and the band's own Kit row can never describe one kit differently.
static func _kit_face(band: Dictionary) -> String:
    var entries: Array[String] = []
    for kit in [
        [DetailFormat.KIT_LABEL_SPEARS, DetailFormat.KIT_DURABILITY_KEY_SPEARS],
        [DetailFormat.KIT_LABEL_SLED, DetailFormat.KIT_DURABILITY_KEY_SLED],
        [DetailFormat.KIT_LABEL_BASKETS, DetailFormat.KIT_DURABILITY_KEY_BASKETS],
    ]:
        var key := String(kit[1])
        entries.append("[color=#%s]%s[/color]" % [
            HudStyle.INK_HEX if DetailFormat.kit_is_equipped(band, key) else HudStyle.DANGER_HEX,
            DetailFormat.kit_condition_face(band, key)])
    return BandDetailLines.BAND_KIT_ROW_SEPARATOR.join(entries)

## One row of a drill-down: the band's name, its value, and an optional dim note. The NAME is a
## clickable `[url]` so the popover doubles as the page's second, better way to reach a band.
static func _band_row(band: Dictionary, index: int, value: String, note: String) -> String:
    var suffix := "  [color=#%s]%s[/color]" % [HudStyle.INK_DIM_HEX, note] if note != "" else ""
    return "%s%s%d][color=#%s]%s[/color][/url]  %s%s" % [
        DetailFormat.DISCLOSURE_URL_OPEN, HudDisclosureVocab.FACTION_BAND_JUMP_META_PREFIX,
        int(band.get("entity", -1)), HudStyle.SIGNAL_HEX,
        HudFormat.band_display_name(band, index + 1), value, suffix]

## `  ⚠ 2 bands` — the alert clause a row appends when some band is in trouble. The COUNT and nothing
## more: which band it is lives one click away, which is the whole division of labour on this page.
static func _alert_clause(count: int, hex: String) -> String:
    return "  " + _alert_text(count, hex) if count > 0 else ""

static func _alert_text(count: int, hex: String) -> String:
    var word := HudWorkVocab.FACTION_ALERT_ONE if count == 1 else HudWorkVocab.FACTION_ALERT_MANY % count
    return "[color=#%s]%s %s[/color]" % [hex, HudWorkVocab.FACTION_ALERT_GLYPH, word]

## The band page's own trend arrow, on a delta this page composed. Silent inside the epsilon, so a
## faction that is merely holding steady does not wear an arrow that means nothing.
static func _trend_glyph(delta: float) -> String:
    if absf(delta) < DetailFormat.MORALE_TREND_EPSILON:
        return ""
    return " [color=#%s]%s[/color]" % [
        HudStyle.HEALTHY_HEX if delta > 0.0 else HudStyle.DANGER_HEX,
        BandDetailLines.MORALE_TREND_RISING_GLYPH if delta > 0.0 else BandDetailLines.MORALE_TREND_FALLING_GLYPH]

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
            HudStyle.INK_FAINT))
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
            HudStyle.SIGNAL if idle > 0 else HudStyle.INK_FAINT))
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
            HudStyle.SIGNAL if known else HudStyle.INK_DIM))
    return block

# ---- leaves -----------------------------------------------------------------

## One `key ……… value` readout row: a dim key, an expanding spacer, then the value in its own tint.
##
## The same shape `HudWidgets.zone_head` gives a section head, one type size down — this page is a
## column of readouts, and a row that measured like a head would leave the section heads meaningless.
## Both labels route their tooltip through `HudWidgets.set_label_tooltip`, since a bare `tooltip_text`
## on a `Label` is a SILENT no-op at Godot's `MOUSE_FILTER_IGNORE` default.
## **ONE SIZE FOR EVERY ROW ON THE PAGE** — there is deliberately no per-zone size parameter. Every one
## of these rows is a row under a `zone_head`, so they are all the same kind of thing, and a page whose
## zones disagreed about how big a row is would read as two designs sharing a card.
static func _stat_row(key: String, value: String, value_color: Color,
        tooltip: String = "") -> HBoxContainer:
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", STAT_ROW_SEPARATION)
    var key_label := Label.new()
    key_label.text = key
    key_label.add_theme_font_size_override("font_size", STAT_ROW_FONT_SIZE)
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
    value_label.add_theme_font_size_override("font_size", STAT_ROW_FONT_SIZE)
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
