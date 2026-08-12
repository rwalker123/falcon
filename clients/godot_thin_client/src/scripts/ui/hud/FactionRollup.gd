class_name FactionRollup
extends RefCounted

## **ALL-`static`, STATELESS** — the FACTION PAGE's FOUR zone builders (issue #450), the all-band
## rollup the Band/City dock's cycler pins as its first entry. A band's page declares three zones and
## this one declares four, the extra being `knowledge`; the panel's body is an ordered list rather
## than a fixed triple precisely so a subject can (`BandCityPanel.set_zone_layout`).
##
## **WHY A SHARED LAYER RATHER THAN A CONTROLLER.** This page is a READOUT: no steppers, no compose
## sheet, no open/closed row, nothing that survives a snapshot. It therefore has no per-cluster state
## to own, which is the whole of what makes a controller a controller (`hud-modules.md`). What it does
## have is arithmetic over the player's whole roster, which is exactly the shape the `SourceForecast` /
## `HudFormat` / `DetailFormat` layers already hold — so it lands beside them, with the one piece of
## state it needs threaded in as a PARAMETER: the `HudBandLaborState` model itself, which is where
## every per-snapshot fact this page sums already lives.
##
## **IT IS THE BAND PAGE'S VITALS BLOCK, ONE SCALE UP.** The same rows in the same order through the
## same `DetailFormat.detail_bbcode` renderer — Food, Kit, Morale, Growth — so the two pages cannot
## drift into different vocabularies for the same facts. (A Trade row stood between Food and Kit until
## arc #527 retired the account it summed.) What the faction page spends its
## disclosures on is the thing only it can answer: **which band each total came from**, one clickable
## row per band.
##
## **AN AGGREGATE WHERE ONE IS MEANINGFUL, AN ALERT WHERE IT IS NOT.** A larder sums, so that row
## carries numbers. A RUNWAY does not — it is one larder against one band's drain — and a KIT does not
## either, being three durabilities per band; so those rows carry `⚠ N bands` and the drill-down
## carries which. Morale and Growth are percentages and go through `FactionAggregate`.
##
## **IT RE-DERIVES NOTHING.** Every figure is a SUM or a weighted mean over answers the per-band
## surfaces already give — `DetailFormat.band_net_food` / `band_provisions` / `band_fertility` /
## `kit_condition_face`, `HudBandLaborState.effective_idle` — so a band's own page
## and this one can never disagree about a number. A rollup that computed its own food ledger would be
## a second source of truth for the identity
## `larder_delta == income − consumption − pen_feed − raid_forfeit` the whole food arc keeps closed.
##
## **THE PAGE IS READ-ONLY, DELIBERATELY.** The issue's scope is "counts and where they are, not
## per-worker controls": role steppers, labor assignment and the compose sheets stay on the per-band
## pages, and the cycler is the way to reach a band from here. Nothing on this page emits a signal, so
## none is declared — which is also why the module can be `static` at all.

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

## **A ROW ON THIS PAGE IS THE SIZE THE FACTION TAB'S OWN ROWS ARE.** The `band` zone's vitals — Food,
## Kit, Morale, Growth — are the page's reference row, and every other zone's rows match them,
## so a player moving between the four tabs is reading one list at one size.
##
## **THIS REVERSES AN EARLIER RULE, and the reversal was reported by eye.** The rows were pinned to the
## WORK BOARD's 13 on the argument that what a surface shares with its model is its RELATIONSHIP to the
## thing above it rather than its absolute size — which made a 10pt head over a 13pt row the "correct
## step" and a 10pt head over a 16pt row a defect. Against the vitals rows two tabs away, 13 simply
## reads SMALL: the page's own reference is not the board, it is the other three zones of the same
## page. A small-caps section head over larger rows is this panel's ordinary idiom (`PEOPLE`,
## `WORKFORCE`), so the head→row step being wide is not a hierarchy fault.
##
## The vitals label carries NO size override — it is a bare `RichTextLabel` at Godot's stock default —
## so this const is that default written down. `band_panel_preview._assert_faction_type_scale` reads
## the LIVE rendered size off both and requires them equal, which is what stops the two drifting if
## the engine default ever moves.
const STAT_ROW_FONT_SIZE := HudWorkVocab.FACTION_STAT_ROW_FONT_SIZE

## The gap between a stat row's key and its value. The value is right-aligned by an expanding spacer,
## exactly as `HudWidgets.zone_head` right-aligns its readout, so this is a floor rather than the gap.
const STAT_ROW_SEPARATION := 8

## A summary row's gaps, and its FLAG's fixed slot. The slot is reserved on every row including the
## calm ones — that is what makes a scan down the left edge answer "does anything need me?" without
## reading, and a slot that appeared only on flagged rows would ragged every name beside it.
const SUMMARY_ROW_SEPARATION := 6

const SUMMARY_FLAG_WIDTH := 14.0

## The knowledge meter's cell count on this page. It reads `FactionReadouts`' own rather than declaring
## a second one: the top bar and this page draw the SAME track at the same resolution, and two
## constants is how they come to disagree about what half-learned looks like.
const KNOWLEDGE_METER_CELLS := FactionReadouts.KNOWLEDGE_METER_CELLS

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
## is built on, and it is why the rows are not uniform: a larder genuinely SUMS, so that row carries
## numbers; a runway is one larder against one band's drain and a kit condition is three durabilities
## per band, so neither has a faction value to state and those rows carry the ALERT instead. The
## detail is one click away in every case.
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

## The rows, in the band page's order, registering each row's per-band drill-down as it goes.
static func _faction_summary_lines(bands: Array, disclosures: DisclosureController) -> Array[String]:
    var lines: Array[String] = []
    lines.append(_food_line(bands, disclosures))
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

## **THE `Trade: 15.6 · +0.39 /turn` ROW IS RETIRED** (arc #527) with the account it summed. A faction's
## materials do not roll up into one figure — that is the flattening the materials model exists to
## refuse — so nothing replaces it here, and a per-material faction total would need a surface of its
## own rather than a vitals row.

## `Kit: ⚠ 2 bands` / `Kit: all equipped`. **THE DURABILITIES DO NOT AGGREGATE AT ALL** — three per
## band, and a mean of them describes no band that exists — so this row is the alert and nothing else.
## Which band is bare-handed is the fact to act on, and it is one click away.
##
## **A BAND THAT IS SHORT OF A KIT COUNTS TOO, AND "all equipped" MEANS IT** (issue #520). The tally
## was dry bands alone, so a band holding ten spears for seventeen hunters was reported as equipped —
## wrong in exactly the reassuring direction the arc exists to remove, and the page is where a player
## looks to find WHICH band needs gear. The two states share one count deliberately: this row is a
## count of bands worth opening, not a diagnosis, and the drill-down row's note says which it is.
static func _kit_line(bands: Array, disclosures: DisclosureController) -> String:
    var rows: Array[String] = []
    var flagged := 0
    for i in range(bands.size()):
        var band: Dictionary = bands[i]
        if not DetailFormat.band_states_kit(band):
            continue
        var is_dry := DetailFormat.band_kit_is_dry(band)
        var is_short := DetailFormat.band_kit_is_short(band)
        if is_dry or is_short:
            flagged += 1
        # A DRY kit leads a short one: the step down is permanent and the shortfall is not, so a band
        # in both states is named by the loss it cannot undo.
        var note := ""
        if is_dry:
            note = HudWorkVocab.FACTION_KIT_DRY_NOTE
        elif is_short:
            note = HudWorkVocab.FACTION_KIT_SHORT_NOTE
        rows.append(_band_row(band, i, _kit_face(band), note))
    if rows.is_empty():
        return ""
    disclosures.register_faction(HudDisclosureVocab.DETAIL_ROW_KIT,
        HudDisclosureVocab.BREAKDOWN_KIND_KIT, rows, flagged > 0)
    var value := _alert_text(flagged, HudStyle.DANGER_HEX) if flagged > 0 \
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

## ONE SUMMARY ROW — the shape Work and Parties share, so the two tabs read as one idea.
##
## **THE FLAG LEADS AND ITS SLOT IS FIXED-WIDTH, on every row including the calm ones.** A scan down
## the left edge answers "does anything need me?" before a word is read, and a slot that appeared only
## on flagged rows would ragged the names and destroy exactly that. It costs 14px of a 354px row.
##
## **THE TWO ACTS TAKE TWO OWNERS, and on the Parties tab they are DIFFERENT ENTITIES.** `owner` keys
## the toggle (it is what `BandPanelController._faction_open_row` matches, so it must be the thing the
## row is ABOUT); `jump_owner` keys the name link. A party's row is named for the band it LEFT and
## jumps there — one owner for both would have made the link read one subject and select another,
## which is exactly how it shipped. The Work tab passes the same entity twice, a row about a band
## jumping to that band.
static func _summary_row(name: String, summary: String, severity: String, owner: int,
        jump_owner: int, on_toggle: Callable, on_jump: Callable) -> Control:
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", SUMMARY_ROW_SEPARATION)
    var flag := Label.new()
    flag.text = HudWorkVocab.FACTION_FLAG_GLYPH if severity != "" else ""
    flag.custom_minimum_size = Vector2(SUMMARY_FLAG_WIDTH, 0.0)
    flag.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    flag.add_theme_font_size_override("font_size", STAT_ROW_FONT_SIZE)
    flag.add_theme_color_override("font_color", _severity_color(severity))
    row.add_child(flag)
    # The NAME jumps to the subject it names; the rest of the row toggles its detail. Two different
    # acts, so two different targets rather than one control that has to guess which the player meant
    # — and two different OWNERS, since the thing a row names is not always the thing it is about.
    var jump := HudWidgets.build_inline_link(name, HudStyle.SIGNAL, on_jump.bind(jump_owner))
    jump.add_theme_font_size_override("font_size", STAT_ROW_FONT_SIZE)
    row.add_child(jump)
    var body := HudWidgets.build_inline_link(summary, HudStyle.INK_DIM, on_toggle.bind(owner))
    body.add_theme_font_size_override("font_size", STAT_ROW_FONT_SIZE)
    body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(body)
    return row

## A summary row's expanded detail: its own lines first, then its alerts. The ALERTS come last because
## the lines are context and the alerts are the ask — a reader who has expanded a flagged row is
## looking for what to do, and it should be the thing their eye stops on.
static func _summary_detail(lines: Array, alerts: Array) -> VBoxContainer:
    var box := HudWidgets.make_zone_block()
    for line in lines:
        box.add_child(_stat_row("    " + String((line as Array)[0]), String((line as Array)[1]),
            HudStyle.INK_DIM))
    for alert_variant in alerts:
        var alert: Dictionary = alert_variant
        box.add_child(_stat_row(
            "    %s %s" % [HudWorkVocab.FACTION_FLAG_GLYPH, String(alert.get("label", ""))],
            String(alert.get("detail", "")),
            _severity_color(String(alert.get("severity", "")))))
    return box

## The orb's own two severities, in the orb's own colours — there is no third, and a page that
## invented one would be describing a state the attention model cannot produce.
static func _severity_color(severity: String) -> Color:
    if severity == HudAttentionVocab.ATTENTION_SEVERITY_CRITICAL:
        return HudStyle.DANGER
    if severity == "":
        return HudStyle.INK_FAINT
    return HudStyle.WARN

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

## Zone `work` — WHERE THE HANDS ARE AND WHAT WANTS ATTENTION: the whole workforce as one bar, then
## one row per band carrying its work summary and its alert flag, expandable to the detail.
##
## **THE ALERTS ARE READ, NEVER RE-DERIVED.** `AttentionController.build_band_attention` is already the
## faction's one answer to "which band needs you", and the turn orb already renders it — so this page
## groups that same array by `owner` rather than asking the same questions again. Two surfaces
## computing "is this band in trouble" is how they come to disagree, and the orb is the one the player
## already trusts.
##
## `open_owner` is which band's detail is expanded (`OPEN_NONE` for none) and `on_toggle` is how a row
## reports a click — the `_work_open_key` idiom, since the open row is per-render state and this layer
## holds none.
##
## **THE KNOWLEDGE TRACKS LEFT THIS ZONE for a zone of their own** — see `build_knowledge_zone`.
static func build_work_zone(labor: HudBandLaborState, attention: Array,
        open_owner: int, on_toggle: Callable, on_jump: Callable) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    col.add_child(_build_workforce_block(labor))
    col.add_child(_build_bands_block(labor, attention, open_owner, on_toggle, on_jump))
    return col

## Zone `knowledge` — WHAT THE FACTION HAS LEARNED, BECOME AND FOUND: its craft tracks, how far it has
## SETTLED, and the Wondrous Sites it has discovered.
##
## **IT IS A ZONE RATHER THAN A BLOCK BECAUSE THREE THINGS ANSWER ONE QUESTION.** The tracks were the
## work zone's last block, and the placement argument for them there still stands as far as it went —
## a track is not a stock and not a population, it is what the faction's hands may ATTEMPT, and every
## rung it gates is a row on a work board. What broke it is the other two: Settling and Discoveries
## are the same KIND of fact (what the faction is and what it knows, neither of which any band owns),
## and a work zone carrying all three would have been the roster of hands plus a second page stapled
## under it, in a box that clips. The four-zone body is what the panel grew to hold them —
## `BandCityPanel.set_zone_layout`.
##
## **THE THREE BLOCKS ARE EACH OMITTED WHEN THEY HAVE NOTHING TO SAY**, the top-bar strip's own rule
## in all three cases: an unstarted track is noise, an unsettled faction has no meter worth a row, and
## a faction that has found nothing gets no heading over an empty list. A page that renders every
## heading regardless states three facts a new game does not have yet.
##
## **`full` IS A HEIGHT TIER, AND DISCOVERIES IS WHAT YIELDS.** At the page's row size the three blocks
## measure 336px against the ~300px a HORIZONTAL dock's zone offers, and this zone CLIPS — so the
## height-capped shell drops one, the `_build_food_outlook_block` idiom. Discoveries is the one to
## drop on two counts: it is the only list here with no ceiling of its own (Settling is one row and
## the craft ladder is five), and it is permanent geographic knowledge rather than a call to act —
## the top bar's own `◈ Discoveries N` still carries the count. What survives is what the player might
## DO something about: how settled they are, and what their hands may attempt.
##
## The caller decides the tier from the box the panel is offering this zone — see
## `BandPanelController.render_faction` and `HudWorkVocab.FACTION_KNOWLEDGE_FULL_MIN_HEIGHT`.
static func build_knowledge_zone(knowledge: Dictionary, sedentarization: Dictionary,
        sites: Array, full: bool) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    var settling := _build_settling_block(sedentarization)
    if settling != null:
        col.add_child(settling)
    var tracks := _build_knowledge_block(knowledge)
    if tracks != null:
        col.add_child(tracks)
    if not full:
        return col
    var discoveries := _build_discoveries_block(sites)
    if discoveries != null:
        col.add_child(discoveries)
    return col

## Group an attention array by the entity each entry is ABOUT. Entries with no owner (the
## unworked-rung producer, whose patches belong to the faction rather than to any band) collect under
## `AttentionController.OWNER_NONE`, where the caller renders them as their own row.
static func group_attention(attention: Array) -> Dictionary:
    var by_owner := {}
    for entry_variant in attention:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        var owner := int(entry.get("owner", AttentionController.OWNER_NONE))
        if not by_owner.has(owner):
            by_owner[owner] = []
        (by_owner[owner] as Array).append(entry)
    return by_owner

## The most severe severity in a set of alerts — what the row's FLAG shows, since a row carries one
## mark and a critical alert must not be hidden behind a warn one.
static func worst_severity(entries: Array) -> String:
    var worst := ""
    for entry_variant in entries:
        var severity := String((entry_variant as Dictionary).get("severity", ""))
        if severity == HudAttentionVocab.ATTENTION_SEVERITY_CRITICAL:
            return severity
        if severity != "":
            worst = severity
    return worst

## Zone `parties` — WHO IS OUT: every detached party across every band, and which band it went from.
##
## `herd_label_for_id` is the caller's own herd-name resolver, threaded in the way
## `HudFormat.panel_expedition_summary` (which this reuses verbatim) already takes it — a stateless
## layer must not reach for the roster, the selection and the herd list that resolver reads.
##
## **`on_jump` IS HANDED A BAND ENTITY HERE, NOT A PARTY'S** — the row is named for the band the party
## left, so its link must select that band (`BandPanelController.jump_to_band_entity`). `on_toggle`
## still gets the party's.
static func build_parties_zone(labor: HudBandLaborState, herd_label_for_id: Callable, attention: Array,
        open_owner: int, on_toggle: Callable, on_jump: Callable) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    var parties := labor.player_expeditions()
    var by_owner := group_attention(attention)
    var block := HudWidgets.make_zone_block()
    var flagged := 0
    for party_variant in parties:
        if by_owner.has(int((party_variant as Dictionary).get("entity", -1))):
            flagged += 1
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_PARTIES,
        HudWorkVocab.FACTION_FLAGGED_FORMAT % [flagged, parties.size()] if flagged > 0 \
            else (str(parties.size()) if not parties.is_empty() else ""),
        null, HudStyle.WARN if flagged > 0 else HudStyle.INK_DIM))
    if parties.is_empty():
        block.add_child(HudWidgets.alloc_hint_label(HudWorkVocab.FACTION_PARTIES_EMPTY))
        col.add_child(block)
        return col
    # The party's HOME BAND is the "where is it" a summary row can honestly carry: a party's own tile
    # changes every turn and means nothing without the map, while the band it left is what the player
    # cycles to in order to act on it — so the NAME jumps to that band.
    #
    # **THE ROW'S TWO OWNERS ARE DIFFERENT ENTITIES HERE, and that is the whole reason `_summary_row`
    # takes them separately.** The TOGGLE is keyed on the PARTY, which is what the row's detail is
    # about and what `_faction_open_row` matches; the NAME's jump is keyed on the HOME BAND, which is
    # what the name says. Binding one entity to both made a link reading `Band 2` select the
    # expedition — the row named one subject and delivered another.
    var names := _band_names_by_entity(labor)
    var shown: int = mini(parties.size(), HudWorkVocab.FACTION_LIST_ROWS_MAX)
    for i in range(shown):
        var party: Dictionary = parties[i]
        var entity := int(party.get("entity", -1))
        var alerts: Array = by_owner.get(entity, [])
        block.add_child(_summary_row(
            String(names.get(int(party.get("home_band_entity", -1)), "")),
            HudFormat.panel_expedition_summary(party, herd_label_for_id,
                labor.band_label_for_id),
            worst_severity(alerts), entity, int(party.get("home_band_entity", -1)),
            on_toggle, on_jump))
        if entity == open_owner:
            block.add_child(_summary_detail(_party_detail_lines(party), alerts))
    _append_more_row(block, parties.size() - shown)
    col.add_child(block)
    return col

## A party's expanded detail — its mission, what it is carrying and how long it has been out. Read off
## the party's own cohort dict; the full in-flight readout stays on the band's parties zone, which has
## the room and the controls for it.
static func _party_detail_lines(party: Dictionary) -> Array:
    var lines: Array = []
    lines.append([HudWorkVocab.FACTION_PARTY_MISSION,
        DetailFormat.expedition_mission_label(String(party.get("expedition_mission", "")))])
    lines.append([HudWorkVocab.FACTION_PARTY_CREW, str(int(party.get("size", 0)))])
    var phase := String(party.get("expedition_phase", "")).strip_edges()
    if phase != "":
        lines.append([HudWorkVocab.FACTION_PARTY_PHASE, HudFormat.expedition_phase_label(phase)])
    return lines

# ---- band zone blocks -------------------------------------------------------

## The faction's PEOPLE bar: the same children/working/elders stack a band's own zone draws, summed
## across every band.
##
## **THE BRACKETS ARE WHOLE PEOPLE ON THE WIRE, so this is a plain sum of integers.** It used to sum
## the raw fixed-point Scalars and apportion the total once at the end, because rounding each band
## first and adding the results lost a person per `.5` remainder. That whole problem left with the
## fractions: the sim rounds once and guarantees `children + working_age + elders == size` per band,
## so adding whole numbers is exact and the faction total is just the sum of the bands' head counts.
##
## Returns null when no band carries an age structure at all, so the block is OMITTED rather than
## rendered from a fabricated split — the band zone's own rule.
static func _build_people_block(bands: Array) -> VBoxContainer:
    var children := 0
    var working_whole := 0
    var elders := 0
    var total := 0
    for band_variant in bands:
        if not (band_variant is Dictionary):
            continue
        var band: Dictionary = band_variant
        children += int(band.get("children", 0))
        working_whole += int(band.get("working_age", 0))
        elders += int(band.get("elders", 0))
        total += int(band.get("size", 0))
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

# ---- knowledge zone blocks --------------------------------------------------

## HOW FAR THE FACTION HAS SETTLED — the sedentarization score as a meter, under the player-facing
## word the manual uses (`Settling`) rather than the sim's own.
##
## **IT IS A REAL HEAD WITH THE READING ON A ROW BENEATH IT**, like every other block on this page.
## It was briefly a head carrying its whole readout on the head LINE — one line for a block that
## states one fact, and 22px of a box the zone was over. Reported by eye: that is not what a head is.
## A head names the section and the row states the reading, and a block with one row is still a block;
## the height it costs is paid for by the zone's own height TIER instead (see the zone builder).
##
## **THE ROW IS KEYED BY THE STAGE**, not by a word restating the head above it: `SETTLING` over
## `Sedentarization` would be a row with no information in its left column, and the stage is the one
## fact the meter does not already carry. A faction the sim reports no stage for reads `Nomadic`,
## which is what a score below the first threshold means. Returns null when the snapshot has carried
## no sedentarization at all — never a `0/100` meter, which claims a measurement that was not made.
##
## **THE STAGE ARRIVES AS A WIRE TOKEN AND IS MAPPED THROUGH A TABLE.** `SedentarizationStage::as_str()`
## spells it `none` / `soft` / `hard`, which is a database value and not a word anyone should see —
## it rendered as a lowercase enum key beside the capitalised `Nomadic` the empty case already got.
## `HudWorkVocab.FACTION_SETTLING_STAGE_LABELS` is the one home for those words, and an unknown or
## absent stage falls back to the `none` row rather than passing the raw token through.
static func _build_settling_block(sedentarization: Dictionary) -> VBoxContainer:
    if sedentarization.is_empty():
        return null
    var score := float(sedentarization.get("score", 0.0))
    var token := String(sedentarization.get("stage", "")).strip_edges()
    # `none` is the sim's own spelling of "no stage reached" and an absent `stage` is the same answer,
    # normalized here through the ONE const both surfaces share rather than a second spelling of the
    # wire value. Anything this build does not recognise falls through the table to the same word.
    if token == "":
        token = FactionReadouts.SEDENTARIZATION_STAGE_NONE
    var stage := String(HudWorkVocab.FACTION_SETTLING_STAGE_LABELS.get(
        token, HudWorkVocab.FACTION_SETTLING_NOMADIC))
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.FACTION_HEADER_SETTLING, ""))
    # **`HudFormat.meter_bar` TAKES A 0–100 SCORE, NOT A FRACTION**, and sedentarization is already on
    # that scale — so the score goes in RAW and `FACTION_SETTLING_SCALE` names only what the `62/100`
    # beside the bar is out of. A fraction here fills zero cells at every score below 50, which is
    # exactly how the knowledge block below shipped its bars empty.
    block.add_child(_stat_row(stage,
        HudWorkVocab.FACTION_SETTLING_VALUE_FORMAT % [
            HudFormat.meter_bar(score, KNOWLEDGE_METER_CELLS),
            int(round(score)), HudWorkVocab.FACTION_SETTLING_SCALE],
        HudStyle.INK_DIM))
    return block

## THE WONDROUS SITES THE FACTION HAS FOUND — one row per site KIND with how many of it, under a head
## carrying the INSTANCE total.
##
## **THE TWO NUMBERS MEAN DIFFERENT THINGS AND BOTH ARE RIGHT**, which is the top bar's own rule
## (`FactionReadouts.update_discoveries`) stated in full rather than as a strip: the head's `N` counts
## INSTANCES found, and the rows are KINDS — three peaks are one row reading `3`. The top bar shows
## the pair as a count beside a strip of marks and is regularly misread as disagreeing with itself;
## with a row per kind there is nothing left to reconcile.
##
## **KEYED ON `site_id`, NOT on the glyph** — the same rule `SecondaryMarkerRenderer`, `WonderSprites`
## and the top-bar strip follow: two distinct sites may share one emoji, and deduping on presentation
## collapses them into one row that then disagrees with the count above it. A catalog-pruned site with
## no id falls back to its TILE, i.e. instance identity, so it gets its own row rather than being
## merged with every other id-less site.
static func _build_discoveries_block(sites: Array) -> VBoxContainer:
    if sites.is_empty():
        return null
    # First-seen order, which is discovery order — the list reads as a history rather than as a
    # ranking, and nothing here has a magnitude worth ranking by.
    var order: Array[String] = []
    var counts := {}
    var names := {}
    for site_variant in sites:
        if not (site_variant is Dictionary):
            continue
        var site: Dictionary = site_variant
        var key := String(site.get("site_id", "")).strip_edges()
        if key == "":
            key = "%d,%d" % [int(site.get("x", 0)), int(site.get("y", 0))]
        if not counts.has(key):
            order.append(key)
            counts[key] = 0
            var display := String(site.get("display_name", "")).strip_edges()
            names[key] = display if display != "" else HudWorkVocab.FACTION_DISCOVERY_UNNAMED
        counts[key] = int(counts[key]) + 1
    if order.is_empty():
        return null
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.FACTION_HEADER_DISCOVERIES,
        str(sites.size()), null, HudStyle.SIGNAL))
    var shown: int = mini(order.size(), HudWorkVocab.FACTION_LIST_ROWS_MAX)
    for i in range(shown):
        var key: String = order[i]
        block.add_child(_stat_row(String(names[key]),
            HudWorkVocab.FACTION_DISCOVERY_COUNT_FORMAT % int(counts[key]), HudStyle.INK_DIM))
    _append_more_row(block, order.size() - shown)
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
    var bench_workers := 0
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
        bench_workers += labor.bench_workers(band)
    var segments: Array = []
    for spec in [
        [HudWorkVocab.WORKFORCE_KEY_FORAGE, forage_workers, HudStyle.HEALTHY],
        [HudWorkVocab.WORKFORCE_KEY_HUNT, hunt_workers, HudStyle.SIGNAL],
        [HudWorkVocab.WORKFORCE_KEY_ROLES, role_workers, HudStyle.VOICE_INK],
        # Party crews are NOT a segment here either — the sim removes them from the parent band's
        # `working_age` on launch, so they sit outside the denominator these segments partition.
        # They read as the header's "away" clause (`WORKFORCE_AWAY_FORMAT`).
        # The bench's crew, for the reason the band zone's own bar carries one: `effective_idle` nets
        # it out, so a faction total without this segment loses those hands off a bar that is supposed
        # to partition the same `working_age` the header sums.
        [HudWorkVocab.WORKFORCE_KEY_BENCH, bench_workers, HudStyle.VOICE_PIGMENT],
        [HudWorkVocab.WORKFORCE_KEY_IDLE, idle, HudStyle.INK_FAINT],
    ]:
        if int(spec[1]) > 0:
            segments.append({"key": String(spec[0]), "count": int(spec[1]), "color": spec[2],
                "tooltip": "%s: %d" % [String(spec[0]), int(spec[1])]})
    var readout := HudWorkVocab.WORKFORCE_IDLE_FORMAT % [idle, working_age]
    if party_workers > 0:
        readout += HudWorkVocab.WORKFORCE_AWAY_FORMAT % party_workers
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_WORKFORCE, readout,
        null, HudStyle.SIGNAL if idle > 0 else HudStyle.INK_DIM,
        HudWorkVocab.WORKFORCE_AWAY_TOOLTIP if party_workers > 0 else ""))
    if not segments.is_empty():
        block.add_child(HudWidgets.build_composition_bar(segments))
        block.add_child(HudWidgets.build_composition_key(segments))
    return block

## WHERE those hands are: one row per band, its workforce and its idle count.
##
## Idle is tinted SIGNAL on a band with hands to spend, exactly as the WORKFORCE head above it tints
## its own idle readout — this row is the reason the player cycles to that band, so it must be
## findable at a glance rather than read row by row.
static func _build_bands_block(labor: HudBandLaborState, attention: Array, open_owner: int,
        on_toggle: Callable, on_jump: Callable) -> VBoxContainer:
    var bands := labor.player_bands()
    var by_owner := group_attention(attention)
    var block := HudWidgets.make_zone_block()
    var flagged := 0
    for band_variant in bands:
        if by_owner.has(int((band_variant as Dictionary).get("entity", -1))):
            flagged += 1
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_WORK,
        HudWorkVocab.FACTION_FLAGGED_FORMAT % [flagged, bands.size()] if flagged > 0 else str(bands.size()),
        null, HudStyle.WARN if flagged > 0 else HudStyle.INK_DIM))
    var shown: int = mini(bands.size(), HudWorkVocab.FACTION_LIST_ROWS_MAX)
    for i in range(shown):
        var band: Dictionary = bands[i]
        var entity := int(band.get("entity", -1))
        var alerts: Array = by_owner.get(entity, [])
        block.add_child(_summary_row(HudFormat.band_display_name(band, i + 1),
            _work_summary(labor, band), worst_severity(alerts), entity, entity, on_toggle, on_jump))
        if entity == open_owner:
            block.add_child(_summary_detail(_work_detail_lines(labor, band), alerts))
    _append_more_row(block, bands.size() - shown)
    # The faction's own alerts — a patch is owned by the FACTION, so an unworked rung has no band whose
    # row it could sit on. Its own row rather than dropped: it is a real call to act, and the turn orb
    # already raises it, so a Work tab that silently omitted it would disagree with the orb.
    var orphans: Array = by_owner.get(AttentionController.OWNER_NONE, [])
    if not orphans.is_empty():
        block.add_child(_summary_row(HudWorkVocab.FACTION_LAND_ROW, "",
            worst_severity(orphans), AttentionController.OWNER_NONE, AttentionController.OWNER_NONE,
            on_toggle, on_jump))
        if open_owner == AttentionController.OWNER_NONE:
            block.add_child(_summary_detail([], orphans))
    return block

## What a band is DOING, in one clipped line: how many sources it works and what it keeps. The COUNTS
## rather than the names — a row is one line and a band works up to 34 sources.
static func _work_summary(labor: HudBandLaborState, band: Dictionary) -> String:
    var sources := 0
    var pens := 0
    for assignment_variant in HudBandLaborState.labor_assignments_of(band):
        var assignment: Dictionary = assignment_variant
        var kind := String(assignment.get("kind", ""))
        if kind != SourceForecast.LABOR_KIND_FORAGE and kind != SourceForecast.LABOR_KIND_HUNT:
            continue
        sources += 1
        var herd_id := String(assignment.get("fauna_id", "")).strip_edges()
        if herd_id.is_empty():
            continue
        var herd := labor.find_world_herd(herd_id)
        if not herd.is_empty() and bool(herd.get("corralled", false)):
            pens += 1
    var parts: Array[String] = [HudWorkVocab.FACTION_SOURCES_ONE if sources == 1 \
        else HudWorkVocab.FACTION_SOURCES_FORMAT % sources]
    if pens > 0:
        parts.append(HudWorkVocab.FACTION_PENS_ONE if pens == 1 \
            else HudWorkVocab.FACTION_PENS_FORMAT % pens)
    return HudWorkVocab.FACTION_SUMMARY_SEPARATOR.join(parts)

## A band's expanded WORK detail: one line per worked source, and its pens and herds — which is where
## the pen and herd alerts sit, beside the thing they are about rather than as a floating warning.
static func _work_detail_lines(labor: HudBandLaborState, band: Dictionary) -> Array:
    var lines: Array = []
    for assignment_variant in HudBandLaborState.labor_assignments_of(band):
        var assignment: Dictionary = assignment_variant
        var kind := String(assignment.get("kind", ""))
        if kind != SourceForecast.LABOR_KIND_FORAGE and kind != SourceForecast.LABOR_KIND_HUNT:
            continue
        var herd_id := String(assignment.get("fauna_id", "")).strip_edges()
        var name := ""
        if herd_id.is_empty():
            name = HudWorkVocab.FACTION_SOURCE_FORAGE_FORMAT % [
                int(assignment.get("target_x", -1)), int(assignment.get("target_y", -1))]
        else:
            var herd := labor.find_world_herd(herd_id)
            name = HudWorkVocab.FACTION_SOURCE_HUNT_FORMAT % (
                SourceForecast.herd_display_name(herd) if not herd.is_empty() else herd_id)
        lines.append([name, HudWorkVocab.FACTION_SOURCE_CREW_FORMAT % [
            int(assignment.get("workers", 0)),
            SourceForecast.format_signed(float(assignment.get("actual_yield", 0.0)))]])
    return lines

## The faction's craft knowledge — one row per track being learned, in the intensification ladder's
## own order, off the SAME `FactionReadouts.faction_tracks` row the top-bar strip and every rung gate
## read. A finished track reads the word rather than a full meter.
##
## **A track the faction has not begun is HIDDEN**, the top-bar strip's own rule: the snapshot row is
## sparse and an unstarted rung is noise. Every track unstarted ⇒ no block at all, rather than a
## heading over nothing.
static func _build_knowledge_block(knowledge: Dictionary) -> VBoxContainer:
    var rows: Array = []
    for track in FactionReadouts.KNOWLEDGE_TRACK_LABELS:
        var progress := float(knowledge.get(track, 0.0))
        if progress <= 0.0:
            continue
        rows.append([String(FactionReadouts.KNOWLEDGE_TRACK_LABELS[track]), progress])
    if rows.is_empty():
        return null
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.FACTION_HEADER_KNOWLEDGE, ""))
    for row in rows:
        var progress: float = row[1]
        var known := progress >= HudConst.KNOWLEDGE_COMPLETE
        # **THE SCALE CONVERSION IS THE POINT OF THIS LINE.** A track's progress is `0..1` and
        # `HudFormat.meter_bar` grades a `0..100` score, so a bare `progress` fills zero cells at every
        # value under 0.5 — which is how every meter on this page shipped EMPTY, indistinguishable
        # from an unstarted track beside a live percent. `FactionReadouts._knowledge_meter_text` scales
        # the same way for the same reason; the two draw one track at one resolution.
        var value := HudWorkVocab.FACTION_KNOWLEDGE_KNOWN if known \
            else KNOWLEDGE_VALUE_FORMAT % [
                HudFormat.meter_bar(progress * HudConst.PROGRESS_PERCENT_SCALE, KNOWLEDGE_METER_CELLS),
                HudFormat.progress_percent(progress)]
        block.add_child(_stat_row(String(row[0]), value,
            HudStyle.SIGNAL if known else HudStyle.INK_DIM))
    return block

# ---- leaves -----------------------------------------------------------------

## One `key ……… value` readout row: a dim key, an expanding spacer, then the value in its own tint.
##
## The same shape `HudWidgets.zone_head` gives a section head, several type sizes UP — a small-caps
## head over larger rows is this panel's ordinary idiom (`PEOPLE`, `WORKFORCE`), and the rows are what
## the player is here to read.
## Both labels route their tooltip through `HudWidgets.set_label_tooltip`, since a bare `tooltip_text`
## on a `Label` is a SILENT no-op at Godot's `MOUSE_FILTER_IGNORE` default.
## **ONE SIZE FOR EVERY ROW ON THE PAGE** — there is deliberately no per-zone size parameter, and the
## size is the `band` zone's vitals rows' (see `STAT_ROW_FONT_SIZE`). Every one of these rows is a row
## under a `zone_head`, so they are all the same kind of thing, and a page whose zones disagreed about
## how big a row is would read as two designs sharing a card.
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
