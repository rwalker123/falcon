class_name HudFormat

## THE SHARED HUD FORMAT / VOCABULARY LAYER (docs/plan_hud_decomposition.md).
##
## WHAT THIS IS. The pure `String`/`int` helpers that decide how the HUD SAYS a thing, with no Control
## anywhere in sight: the status-glyph → words → tooltip-line → joined-tooltip chain, the row glyph
## affixes, the policy face, the expedition ROW vocabulary (phase key → phase suffix → the compact
## one-line party summary), the largest-remainder people apportionment + the dependency tooltip, and
## the one 0..1 → whole-percent conversion.
##
## WHY IT IS SEPARATE FROM `HudWidgets`. `TopBarReadouts` needs `progress_percent` and nothing else;
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
## A stockpile entry with no item key at all — falls back to the section's own name.
const STOCKPILE_UNKNOWN_LABEL := "Stockpile"
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
## Positional band names ("Band 1", "Band 2", …), matching the roster's numbering.
const BAND_DISPLAY_NAME_FORMAT := "Band %d"

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
## rung readout shares (the gate-reason lines, the work inspector's standing-investment line and its
## confirm), so a rung can never read one way beside the picker and another in the dialog.
static func policy_face(policy: String) -> String:
    return "%s%s" % [source_icon_prefix(FoodIcons.for_policy(policy)), policy.capitalize()]

## A 0..1 progress track (knowledge / domestication) as a whole percent. 0 is a MEANINGFUL reading in
## a gate reason — it tells the player they haven't started the track at all.
static func progress_percent(progress: float) -> int:
    return int(round(clampf(progress, 0.0, 1.0) * HudConst.PROGRESS_PERCENT_SCALE))

# ---- People: apportionment + the dependency vocabulary -------------------------------------------

## Round fractional age brackets to whole people SO THEY STILL SUM TO THE WHOLE BAND — the
## largest-remainder method: floor every part, then hand the leftover people out to the biggest
## fractions, biggest first. `round()` per part does NOT preserve the total (9.29 + 16.54 + 4.64 =
## 30.47 rounds to 9 + 17 + 5 = 31), and a Band panel that disagrees with the top bar about how many
## people are in the band reads as a bug in both.
static func apportion_people(parts: Array[float]) -> Array[int]:
    var whole: Array[int] = []
    var assigned := 0
    var total := 0.0
    for part in parts:
        var floored: int = maxi(int(floor(part)), 0)
        whole.append(floored)
        assigned += floored
        total += maxf(part, 0.0)
    var leftover := roundi(total) - assigned
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

## Humanize a raw stockpile item key (`dried_fish` → `Dried Fish`). Word-cased token by token rather
## than through `String.capitalize()`, which also rewrites punctuation and separators. TWO surfaces
## share it — the left-dock stockpile panel (`TopBarReadouts`) and the band drawer's accessible-stock
## rows (`BandDetailLines`) — which is why it lives here instead of being injected into either.
## An empty/blank key falls back to the section's own name rather than an empty cell.
static func stockpile_label(raw_value: String) -> String:
    var trimmed := raw_value.strip_edges()
    if trimmed == "":
        return STOCKPILE_UNKNOWN_LABEL
    var tokens: PackedStringArray = trimmed.split("_", false)
    if tokens.is_empty():
        return trimmed.capitalize()
    var parts: Array[String] = []
    for token in tokens:
        if token == "":
            continue
        var head := token.substr(0, 1).to_upper()
        var tail := ""
        if token.length() > 1:
            tail = token.substr(1, token.length() - 1)
        parts.append(head + tail)
    if parts.is_empty():
        return trimmed.capitalize()
    return " ".join(parts)

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
## live in the tooltip. A scout has no policy → `for_policy` returns "" → `row_glyph_suffix` emits
## nothing, so the row carries the phase glyph alone with no orphaned separator. Only `awaiting` keeps
## its words (`expedition_phase_suffix`). The next-delivery detail is NOT here — it lives on the
## parties inspector strip a row click opens (`_build_parties_inspector` → `BandDetailLines.expedition_summary_lines`).
##
## `herd_label_for_id` is the herd vocabulary, THREADED IN rather than reached for: resolving a herd id
## to a species needs the roster + the current selection + the snapshot herd list, which is HUD state
## this stateless layer must not hold (the `HudWidgets.build_worker_stepper` `current_turn` precedent).
## It is called ONLY on the hunt branch, so a scout row resolves nothing.
static func panel_expedition_summary(exp: Dictionary, herd_label_for_id: Callable) -> String:
    var mission := String(exp.get("expedition_mission", "")).strip_edges().to_lower()
    var phase_suffix := expedition_phase_suffix(expedition_phase_key(exp))
    var policy_suffix := row_glyph_suffix(
        FoodIcons.for_policy(String(exp.get("expedition_hunt_policy", ""))))
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_HUNT:
        var herd := String(herd_label_for_id.call(String(exp.get("expedition_target_herd", "")).strip_edges()))
        return "%s %s%s%s" % [
            PANEL_EXPEDITION_HUNT_GLYPH, herd, policy_suffix, phase_suffix]
    var x := int(exp.get("current_x", -1))
    var y := int(exp.get("current_y", -1))
    return "%s → (%d, %d)%s%s" % [
        PANEL_EXPEDITION_SCOUT_GLYPH, x, y, policy_suffix, phase_suffix]

## A block-glyph bar for a 0–100 score. `cells` is passed by every caller — the Sedentarization meter
## (via TopBarReadouts) at the standard width, the knowledge strip narrower, the herd-drawer danger
## rows narrower still. Lives here (the pure format layer) because THREE clusters read it and it
## touches no member; DetailFormat's danger bars and TopBarReadouts' meters call it as
## `HudFormat.meter_bar` rather than taking a Callable injection.
static func meter_bar(score: float, cells: int) -> String:
    var filled := int(round(clampf(score / 100.0, 0.0, 1.0) * float(cells)))
    return "▰".repeat(filled) + "▱".repeat(cells - filled)
