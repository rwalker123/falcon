class_name HudFormat

## THE SHARED HUD FORMAT / VOCABULARY LAYER (docs/plan_hud_decomposition.md).
##
## WHAT THIS IS. The pure `String`/`int` helpers that decide how the HUD SAYS a thing, with no Control
## anywhere in sight: the status-glyph → words → tooltip-line → joined-tooltip chain, the row glyph
## affixes, the policy face, the expedition ROW vocabulary (phase key → phase suffix → the compact
## one-line party summary), the largest-remainder people apportionment + the dependency tooltip, and
## the one 0..1 → whole-percent conversion.
##
## WHY IT IS SEPARATE FROM `HudWidgets`. `FactionReadouts` needs `progress_percent` and nothing else;
## the drawer and the Band panel need the whole status chain AND the widget factory. Keeping the words
## in their own file means the top-bar cluster can depend on the vocabulary without pulling in a
## builder of Controls it never builds. `HudWidgets` depends on THIS, never the other way round.
##
## EVERYTHING HERE IS `static`, STATELESS AND PURE — same invariant as `SourceForecast` and
## `HudWidgets`. The word TABLES (`STATUS_LABELS`, `STATUS_HINTS`, `EXPEDITION_PHASE_LABELS`) live in
## `HudExpeditionVocab` and are read as `HudExpeditionVocab.X`, so there is still exactly one place a
## phrase is typed.
## Where a formatter needs HUD state it takes it as a PARAMETER rather than reaching for it — see
## `panel_expedition_summary`'s `herd_label_for_id` Callable, the `HudWidgets.build_worker_stepper`
## `current_turn` precedent.

## A food module the table cannot name and whose key is empty — the tile carries no module at all.
const FOOD_MODULE_UNKNOWN_LABEL := "Unknown"
## Dependency ratio: dependents per this many working-age adults.
const PEOPLE_DEPENDENCY_BASE := 100
## SHORT on purpose: the chip's face already carries the count, so the tooltip only has to say what a
## dependent IS and who carries them. The long version (which also quoted the ratio) explained the
## jargon without making it any more useful — the ratio itself is gone from the UI entirely.
const PEOPLE_DEPENDENCY_TOOLTIP := """Children and elders — they eat from the larder but cannot be put to work.
%d working-age adults support them."""
## Appended when dependents outnumber workers — the reason the chip is WARN-tinted.
const PEOPLE_DEPENDENCY_HEAVY_TOOLTIP := "\nMore mouths than hands."
## Band/City panel "Active expeditions" mission glyphs — they mirror the map markers
## (MapView EXPEDITION_GLYPH / EXPEDITION_HUNT_GLYPH).
const PANEL_EXPEDITION_SCOUT_GLYPH := "⚑"
const PANEL_EXPEDITION_HUNT_GLYPH := "🏹"
## The DENIAL raid's mark (`docs/plan_denial_raid.md`) — the same 💀 its footer button and its map
## marker wear, so the mission reads identically at every scale.
const PANEL_EXPEDITION_DENY_GLYPH := "💀"
## The SHIPMENT's mark (arc #527) — the same 📦 its footer button and its map marker wear, the rule
## the denial glyph above states: one mission, one glyph, at every scale.
const PANEL_EXPEDITION_TRADE_GLYPH := "📦"
## Positional band names ("Band 1", "Band 2", …), matching the roster's numbering.
const BAND_DISPLAY_NAME_FORMAT := "Band %d"
## The band's hex coordinates in the Band/City panel HEADER, beside its stage word — the header's
## other word is `BAND_DISPLAY_NAME_FORMAT` above, so the pair lives together. Coordinates are the
## band's IDENTITY ("which one am I looking at"), not a vital, which is why they read as a bare
## parenthesised pair here rather than as a labelled `Position:` row in the band zone's vitals.
const BAND_HEADER_POSITION_FORMAT := "(%d, %d)"

## THE FACTION PAGE's header identity (issue #450), beside the band header vocabulary above because
## the two fill the SAME three header slots and must be read together. The panel's header takes a
## stage id, a glyph, a name and a stage word; a faction has no settlement stage and no tile, so it
## supplies an empty stage id (no bundled art resolves, `StageSprites.for_stage("")` answers null),
## its own glyph, and its BAND COUNT where a band puts its stage word — the identity fact at that
## scale. It supplies no position label, so the header's coordinate slot hides itself.
## **The glyph is deliberately NOT `⚑`**, which is the scouting mission's on the parties rows, nor any
## of the settlement stages' `⛺ 🛖 🏘️` — a faction page marked with a settlement stage would read as
## one more band in the cycle, which is precisely the thing it is not.
const FACTION_PAGE_GLYPH := "❖"

## "Your people", the voice the knowledge strip already speaks in (`⚒ Your people know:`) rather than
## a bare "Faction" — the panel names a thing the player commands, not a database row.
const FACTION_PAGE_NAME := "Your people"

const FACTION_BANDS_LABEL_FORMAT := "%d bands"

const FACTION_BANDS_LABEL_ONE := "1 band"

## The faction header's second line: how many bands this page is summing. Singular is spelled out
## rather than left to a `%d bands` that would read "1 bands" the whole of the early game, which is
## every player's first hour.
static func faction_bands_label(count: int) -> String:
    return FACTION_BANDS_LABEL_ONE if count == 1 else FACTION_BANDS_LABEL_FORMAT % count

## The food-module display names. This table came here WITH `food_module_label`, its only reader —
## the words belong to the vocabulary layer, not to the compose builders that print them.
const FOOD_MODULE_LABELS := {
    "coastal_littoral": "Coastal Littoral",
    "riverine_delta": "Riverine Delta",
    "savanna_grassland": "Savanna Grassland",
    "temperate_forest": "Temperate Forest",
    "boreal_arctic": "Boreal Arctic",
    "montane_highland": "Montane Highland",
    "wetland_swamp": "Wetland Swamp",
    "semi_arid_scrub": "Semi-Arid Scrub",
    "coastal_upwelling": "Coastal Upwelling",
    "mixed_woodland": "Mixed Woodland",
}

## A food module's display name, from the table above; an unlisted key humanizes its own id, and an
## empty one reads "Unknown" (the compose sheet's header fallback when the tile carries no label).
static func food_module_label(module_key: String) -> String:
    if module_key == "":
        return FOOD_MODULE_UNKNOWN_LABEL
    return String(FOOD_MODULE_LABELS.get(module_key, module_key.capitalize().replace("_", " ")))

## Best-effort readable band name: a positional "Band N". (Cohorts carry no top-level
## band label in the snapshot yet — see the server-side follow-up.)
static func band_display_name(_entry: Dictionary, index: int) -> String:
    return BAND_DISPLAY_NAME_FORMAT % index

## "<glyph> " for a resolved glyph, "" for none — so a Current-actions row degrades to bare text
## (no stray leading space) when the resource can't be resolved.
static func source_icon_prefix(icon: String) -> String:
    return "%s " % icon if icon != "" else ""

## A trailing glyph on a row ("  ♻" / "  ●"), separated from the label — "" for an unknown/absent
## glyph, so a row with no policy / no status renders bare rather than trailing whitespace.
static func row_glyph_suffix(glyph: String) -> String:
    return "" if glyph == "" else HudExpeditionVocab.ROW_GLYPH_SEPARATOR + glyph

## Humanize an expedition phase id ("awaiting" → "Awaiting orders").
static func expedition_phase_label(phase: String) -> String:
    var key := phase.strip_edges().to_lower()
    if HudExpeditionVocab.EXPEDITION_PHASE_LABELS.has(key):
        return HudExpeditionVocab.EXPEDITION_PHASE_LABELS[key]
    return key.capitalize()

## The WORDS behind a status glyph. Order-level statuses come from `HudExpeditionVocab.STATUS_LABELS`; an expedition
## PHASE reads from `HudExpeditionVocab.EXPEDITION_PHASE_LABELS` (`expedition_phase_label`), which stays the single
## source of truth for the phase words — they are never re-typed here.
static func status_label(status: String) -> String:
    var key := status.strip_edges().to_lower()
    if key == "":
        return ""
    if HudExpeditionVocab.STATUS_LABELS.has(key):
        return String(HudExpeditionVocab.STATUS_LABELS[key])
    return expedition_phase_label(key)

## One tooltip line spelling a status glyph out: the word plus its behaviour hint ("Pending — starts
## when you advance the turn"); a status whose word says it all (`Working`) renders bare.
static func status_tooltip_line(status: String) -> String:
    var label := status_label(status)
    if label == "":
        return ""
    var hint := String(HudExpeditionVocab.STATUS_HINTS.get(status.strip_edges().to_lower(), ""))
    return label if hint == "" else HudExpeditionVocab.STATUS_HINT_FORMAT % [label, hint]

## Append the status words to a row tooltip. The glyph on the row is terse by design, so the hover
## must carry what it encodes — composed WITH the tooltip the row already had (yield readout,
## overstaffing explanation, policy hint), never replacing it.
static func append_status_tooltip(tooltip: String, status: String) -> String:
    var status_line := status_tooltip_line(status)
    if status_line == "":
        return tooltip
    return status_line if tooltip == "" else tooltip + SourceForecast.TOOLTIP_LINE_SEPARATOR + status_line

## Join the non-empty parts of a row tooltip (yield readout · policy behaviour · …) into one block.
static func join_tooltip_lines(lines: Array) -> String:
    var parts: Array[String] = []
    for line in lines:
        var text := String(line)
        if text != "":
            parts.append(text)
    return SourceForecast.TOOLTIP_LINE_SEPARATOR.join(parts)

## A rung's display FACE — its `FoodIcons` glyph welded to its name. The one policy vocabulary every
## **A CLICKABLE RUN INSIDE A BBCODE LINE** — the `[url]` the host's `meta_clicked` dispatches on,
## tinted and underlined so it reads as a link rather than as coloured prose.
##
## **IT IS A FORMAT HELPER, NOT A WIDGET, and that is the split it belongs on.** `HudWidgets`'
## `build_inline_link` builds a whole `Button` for a link that stands ALONE on its row; this one is
## for a link inside a SENTENCE, which structurally cannot be a sibling control — a wrapped line
## cannot break inside an atomic child. The compose sheet's offered rung is the caller
## (`docs/plan_standing_upkeep.md` §4.7a ①); `DetailFormat`'s disclosure carets spell the same run by
## hand because theirs also carries a caret glyph and a per-row tint fork.
##
## The `[u]` is inside the `[color]` deliberately: Godot draws the underline in the CURRENT colour, so
## the other nesting leaves a tinted word underscored in the label's default ink.
static func bbcode_link(text: String, meta: String, hex: String) -> String:
    return "[url=%s][color=#%s][u]%s[/u][/color][/url]" % [meta, hex, text]

## rung readout shares (the gate-reason lines, the work inspector's standing-investment line and its
## confirm), so a rung can never read one way beside the picker and another in the dialog.
static func policy_face(policy: String) -> String:
    return "%s%s" % [source_icon_prefix(FoodIcons.for_policy(policy)), policy.capitalize()]

## **THE PLANT WEB'S CREW NOUN, AND THE ONE PLACE IT IS DECIDED.** A wild stand is drawn down by
## FORAGERS; a Tended Patch or a Field is kept by TENDERS. The authority is the ladder config itself
## (`core_sim/src/data/intensification_ladder.json`), where the `wild` rung declares the harvest
## primitive `worker_take` and both upper rungs declare `worker_tend` — a managed source is never
## gather-drawn (the sim's `is_managed()` branch), so a crew standing on one is not foraging at all.
##
## **ONE TEST ANSWERS BOTH UPPER RUNGS.** `improvement_is_done(…, CULTIVATE)` asks whether the patch
## STANDS at or above `plant:tended` — a completed Field does, even though `is_cultivated` is honestly
## false on it because `Sow` needs no prior patch — so it is true on a Tended Patch AND on a Field sown
## straight from wild ground, and a separate `SOW` test would only be a second spelling of the same
## answer, free to drift. That at-or-above reading is what is relied on here.
##
## **A BUILD IN FLIGHT KEEPS THE WILD NOUN**, deliberately: this reads the rung the patch STANDS on and
## never a composed improvement, so people part-way through a Cultivate or a Sow — who really are
## foraging the stand while they clear ground, which is what the build dip charges them for — stay
## Foragers until the rung COMPLETES. The animal web's `_herd_crew_noun` does read the composed axis,
## because a herd being penned owes keepers before the pen exists; a patch owes nobody anything.
##
## **DISPLAY ONLY.** The command is still `assign_labor` with kind `forage`
## (`SourceForecast.LABOR_KIND_FORAGE`); nothing on the wire moves with this word.
##
## `prefix` spells the keys, so a `patch_`-prefixed `tile_info` and a bare wire patch both work.
static func plant_crew_label(src: Dictionary, prefix: String) -> String:
    return HudComposeVocab.TEND_CREW_LABEL \
        if SourceForecast.improvement_is_done(src, prefix, SourceForecast.IMPROVEMENT_CULTIVATE) \
        else HudComposeVocab.FORAGE_CREW_LABEL

# ---- The escapement floor, in words --------------------------------------------------------------

## A FLOOR PRESET's display FACE — its zone glyph welded to its label (`💀 Take everything`). The one
## floor vocabulary every preset readout shares, so a preset can never read one way on the picker and
## another in a tooltip.
static func floor_preset_face(preset: String) -> String:
    var floor := SourceForecast.floor_for_preset(preset)
    var glyph := FoodIcons.for_floor_zone(SourceForecast.floor_zone(floor))
    return "%s%s" % [source_icon_prefix(glyph), HudComposeVocab.FLOOR_PRESET_LABELS.get(preset, "")]

## **THE ONE SENTENCE SAID ABOUT A FLOOR** — the replacement for the three per-stance hint tables, and
## the whole of what the client says about harvest pressure. It is composed rather than looked up
## because two facts vary independently of the zone: WHAT STRIPPING COSTS differs by web (a patch
## reseeds, a herd is gone for good), and a detached party earns no craft, so the learning zone's
## promise is false for a raid. Everything else is one table of five.
##
## `kind` is a `SourceForecast.LABOR_KIND_*`; `expedition` marks a detached party.
static func floor_hint(floor: float, kind: String, expedition: bool = false) -> String:
    var zone := SourceForecast.floor_zone(floor)
    if expedition and zone == SourceForecast.FLOOR_ZONE_LEARNING:
        return HudComposeVocab.FLOOR_LEARNING_HINT_EXPEDITION
    var text := String(HudComposeVocab.FLOOR_ZONE_HINTS.get(zone, ""))
    if zone == SourceForecast.FLOOR_ZONE_STRIP:
        return text % String(HudComposeVocab.FLOOR_STRIP_CONSEQUENCE.get(kind, ""))
    return text

## A 0..1 progress track (knowledge / domestication) as a whole percent. 0 is a MEANINGFUL reading in
## a gate reason — it tells the player they haven't started the track at all.
static func progress_percent(progress: float) -> int:
    return int(round(clampf(progress, 0.0, 1.0) * HudConst.PROGRESS_PERCENT_SCALE))

# ---- People: apportionment + the dependency vocabulary -------------------------------------------

## Divide whole people into fractional parts SO THEY STILL SUM TO AN EXPLICIT TOTAL — the
## largest-remainder method: floor every part, then hand the leftover people out to the biggest
## fractions, biggest first. `round()` per part does NOT preserve the total, and a panel that
## disagrees with itself about how many people are in a band reads as a bug in both readouts.
##
## **This is the client's OWN arithmetic, never a second opinion on the sim's.** The age brackets
## arrive as whole people already — the sim rounds them once and guarantees they sum to `size` — so
## nothing rounds them here. What genuinely needs rounding is the SPLIT SHEET: 9 whole children
## divided by a 40% share the player chose is 3.6 children, and somebody has to decide.
##
## **The split sheet is why this takes an explicit target.** It apportions both halves of a band in
## ONE pass so their displayed people sum to the band's own displayed total — and it holds the chosen
## worker count out of the parts entirely, that being an integer the player picked rather than a
## fraction to round. So the target is not `round(sum(parts))`; it is the band's people minus those
## pinned workers.
static func apportion_people_to(parts: Array[float], target: int) -> Array[int]:
    var whole: Array[int] = []
    var assigned := 0
    for part in parts:
        var floored: int = maxi(int(floor(part)), 0)
        whole.append(floored)
        assigned += floored
    var leftover := target - assigned
    while leftover > 0:
        var best := -1
        var best_fraction := -1.0
        for i in range(parts.size()):
            var fraction: float = maxf(parts[i], 0.0) - float(whole[i])
            if fraction > best_fraction:
                best_fraction = fraction
                best = i
        if best < 0:
            break
        whole[best] += 1
        leftover -= 1
    return whole

## Dependents per 100 working-age adults — the ratio itself, which only the tooltips render now.
static func dependency_per_hundred(dependents: int, working: int) -> int:
    if working <= 0:
        return 0
    return int(round(float(dependents) / float(working) * float(PEOPLE_DEPENDENCY_BASE)))

## What "dependents" MEANS, in the player's terms. The ratio is no longer shown anywhere — it only
## decides the WARN tint — so it stays out of the words too. `PEOPLE_DEPENDENCY_HEAVY` lives in
## `HudWorkVocab` (the chip's own tint reads it too) and is read as `HudWorkVocab.X`.
static func dependency_tooltip(dependents: int, working: int) -> String:
    var text: String = PEOPLE_DEPENDENCY_TOOLTIP % working
    if dependency_per_hundred(dependents, working) > HudWorkVocab.PEOPLE_DEPENDENCY_HEAVY:
        text += PEOPLE_DEPENDENCY_HEAVY_TOOLTIP
    return text

# ---- Expedition row vocabulary -------------------------------------------------------------------

## The expedition's sim phase key, normalized (the wire's `ExpeditionPhase` string).
static func expedition_phase_key(exp: Dictionary) -> String:
    return String(exp.get("expedition_phase", "")).strip_edges().to_lower()

## The phase as it renders ON the row: the glyph alone, except `awaiting`, which keeps its words
## (`▮▮ Awaiting orders`) — a demand on the player must read without a hover.
static func expedition_phase_suffix(phase: String) -> String:
    var suffix := row_glyph_suffix(FoodIcons.for_status(phase))
    if phase == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
        return "%s %s" % [suffix, expedition_phase_label(phase)]
    return suffix

## Compact one-line expedition summary: hunt → `🏹 <herd> · <Policy>  <phase glyph>`;
## scout → `⚑ → (x, y)  <phase glyph>`. Policy AND phase read as GLYPHS here exactly as they do on the
## Current-actions rows (one concept, one rendering, in both sections of the same panel); the words
## live in the tooltip. A scout gets no floor glyph at all (it harvests nothing) → `row_glyph_suffix`
## emits nothing, so the row carries the phase glyph alone with no orphaned separator. Only `awaiting` keeps
## its words (`expedition_phase_suffix`). The next-delivery detail is NOT here — it lives on the
## parties inspector strip a row click opens (`_build_parties_inspector` → `BandDetailLines.expedition_summary_lines`).
##
## `herd_label_for_id` is the herd vocabulary, THREADED IN rather than reached for: resolving a herd id
## to a species needs the roster + the current selection + the snapshot herd list, which is HUD state
## this stateless layer must not hold (the `HudWidgets.build_worker_stepper` `current_turn` precedent).
## It is called ONLY on the hunt branch, so a scout row resolves nothing.
## **WHAT A SHIPMENT'S DESTINATION IS CALLED** — the sim's published name when it has one, else
## whatever THIS CLIENT calls that band, joined on `expedition_destination_band`.
##
## **`expeditionDestinationName` IS EMPTY ON EVERY LIVE SHIPMENT TODAY, AND THAT IS DELIBERATE.**
## Bands have no names in this game, so the sim publishes nothing rather than guessing — it briefly
## published the unit ARCHETYPE (`"BandForager"`, the same string for every band), which made the row
## disagree with the label the rest of the HUD gives that same band. Empty means "no name", exactly as
## an empty material row means "no row".
##
## **The published name still wins when it is non-empty**, and that is the half that matters later:
## the day a second faction lands (#513) a foreign band's name can only come from the sim, because
## this client holds no roster to resolve one from.
##
## `band_label_for_id` is `HudBandLaborState.band_label_for_id` threaded in as a PARAMETER, the
## `herd_label_for_id` treatment: a stateless layer must not reach for the roster that resolver reads.
## `""` when neither tier answers — a caller renders no destination at all rather than the raw
## `BandId`, which is a database key.
static func expedition_destination_label(exp: Dictionary,
        band_label_for_id: Callable = Callable()) -> String:
    var published := String(exp.get("expedition_destination_name", "")).strip_edges()
    if published != "":
        return published
    if not band_label_for_id.is_valid():
        return ""
    return String(band_label_for_id.call(
        int(exp.get("expedition_destination_band", HudConst.NO_BAND_ID)))).strip_edges()

static func panel_expedition_summary(exp: Dictionary, herd_label_for_id: Callable,
        band_label_for_id: Callable = Callable()) -> String:
    var mission := String(exp.get("expedition_mission", "")).strip_edges().to_lower()
    var phase_suffix := expedition_phase_suffix(expedition_phase_key(exp))
    # The party's FLOOR as its zone glyph — the same mark the work board gives a resident crew, so a
    # raid and a hunt at the same pressure read alike. A SCOUT reports `1.0` (it harvests nothing),
    # which is a real zone, so the glyph is resolved on the hunt branch alone and a scout row keeps
    # its phase glyph with no orphaned separator.
    var floor_suffix := row_glyph_suffix(FoodIcons.for_floor_zone(SourceForecast.floor_zone(
        float(exp.get("expedition_floor", SourceForecast.FLOOR_MAX))))) \
        if String(exp.get("expedition_mission", "")).strip_edges().to_lower() \
            == HudExpeditionVocab.EXPEDITION_MISSION_HUNT else ""
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_HUNT:
        var herd := String(herd_label_for_id.call(String(exp.get("expedition_target_herd", "")).strip_edges()))
        return "%s %s%s%s" % [
            PANEL_EXPEDITION_HUNT_GLYPH, herd, floor_suffix, phase_suffix]
    # DENIAL — the hunt row's shape with the mission's own mark and NO floor glyph. Its
    # `expedition_floor` reads `0.0`, which is a real zone (`strip`), so borrowing the hunt branch's
    # glyph would mark a raid with a pressure it never chose — the mission has no floor at all.
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_DENY:
        var quarry := String(herd_label_for_id.call(String(exp.get("expedition_target_herd", "")).strip_edges()))
        return "%s %s%s" % [PANEL_EXPEDITION_DENY_GLYPH, quarry, phase_suffix]
    # A SHIPMENT names WHO IT IS FOR — never by `expedition_destination_band`, which is the key the
    # command addresses. `expedition_destination_label` is the one resolution (the sim's published
    # name, else this client's own label for that band), so this row, the drawer's `Bound for` row and
    # the destination picker cannot call one band three things.
    # No floor glyph: a shipment harvests nothing, so it has no pressure to mark.
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_TRADE:
        var destination := expedition_destination_label(exp, band_label_for_id)
        # An unresolvable destination leaves the row as the mission's mark and its phase, rather than
        # an orphaned separator in front of nothing.
        if destination == "":
            return "%s%s" % [PANEL_EXPEDITION_TRADE_GLYPH, phase_suffix]
        return "%s %s%s" % [PANEL_EXPEDITION_TRADE_GLYPH, destination, phase_suffix]
    var x := int(exp.get("current_x", -1))
    var y := int(exp.get("current_y", -1))
    return "%s → (%d, %d)%s%s" % [
        PANEL_EXPEDITION_SCOUT_GLYPH, x, y, floor_suffix, phase_suffix]

## A block-glyph bar for a 0–100 score. `cells` is passed by every caller — the Sedentarization meter
## (via FactionReadouts) at the standard width, the knowledge strip narrower, the herd-drawer danger
## rows narrower still. Lives here (the pure format layer) because THREE clusters read it and it
## touches no member; DetailFormat's danger bars and FactionReadouts' meters call it as
## `HudFormat.meter_bar` rather than taking a Callable injection.
static func meter_bar(score: float, cells: int) -> String:
    var filled := int(round(clampf(score / 100.0, 0.0, 1.0) * float(cells)))
    return "▰".repeat(filled) + "▱".repeat(cells - filled)
