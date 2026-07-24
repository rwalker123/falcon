class_name HudFormat

## THE SHARED HUD FORMAT / VOCABULARY LAYER (docs/plan_hud_decomposition.md).
##
## WHAT THIS IS. The pure `String`/`int` helpers that decide how the HUD SAYS a thing, with no Control
## anywhere in sight: the status-glyph → words → tooltip-line → joined-tooltip chain, the row glyph
## affixes, the policy face, the expedition phase words, and the one 0..1 → whole-percent conversion.
##
## WHY IT IS SEPARATE FROM `HudWidgets`. `TopBarReadouts` needs `progress_percent` and nothing else;
## the drawer and the Band panel need the whole status chain AND the widget factory. Keeping the words
## in their own file means the top-bar cluster can depend on the vocabulary without pulling in a
## builder of Controls it never builds. `HudWidgets` depends on THIS, never the other way round.
##
## EVERYTHING HERE IS `static`, STATELESS AND PURE — same invariant as `SourceForecast` and
## `HudWidgets`. The word TABLES (`STATUS_LABELS`, `STATUS_HINTS`, `EXPEDITION_PHASE_LABELS`) stay on
## `HudLayer` and are read back as `HudLayer.X`, so there is still exactly one place a phrase is typed.

## "<glyph> " for a resolved glyph, "" for none — so a Current-actions row degrades to bare text
## (no stray leading space) when the resource can't be resolved.
static func source_icon_prefix(icon: String) -> String:
    return "%s " % icon if icon != "" else ""

## A trailing glyph on a row ("  ♻" / "  ●"), separated from the label — "" for an unknown/absent
## glyph, so a row with no policy / no status renders bare rather than trailing whitespace.
static func row_glyph_suffix(glyph: String) -> String:
    return "" if glyph == "" else HudLayer.ROW_GLYPH_SEPARATOR + glyph

## Humanize an expedition phase id ("awaiting" → "Awaiting orders").
static func expedition_phase_label(phase: String) -> String:
    var key := phase.strip_edges().to_lower()
    if HudLayer.EXPEDITION_PHASE_LABELS.has(key):
        return HudLayer.EXPEDITION_PHASE_LABELS[key]
    return key.capitalize()

## The WORDS behind a status glyph. Order-level statuses come from `HudLayer.STATUS_LABELS`; an expedition
## PHASE reads from `HudLayer.EXPEDITION_PHASE_LABELS` (`expedition_phase_label`), which stays the single
## source of truth for the phase words — they are never re-typed here.
static func status_label(status: String) -> String:
    var key := status.strip_edges().to_lower()
    if key == "":
        return ""
    if HudLayer.STATUS_LABELS.has(key):
        return String(HudLayer.STATUS_LABELS[key])
    return expedition_phase_label(key)

## One tooltip line spelling a status glyph out: the word plus its behaviour hint ("Pending — starts
## when you advance the turn"); a status whose word says it all (`Working`) renders bare.
static func status_tooltip_line(status: String) -> String:
    var label := status_label(status)
    if label == "":
        return ""
    var hint := String(HudLayer.STATUS_HINTS.get(status.strip_edges().to_lower(), ""))
    return label if hint == "" else HudLayer.STATUS_HINT_FORMAT % [label, hint]

## Append the status words to a row tooltip. The glyph on the row is terse by design, so the hover
## must carry what it encodes — composed WITH the tooltip the row already had (yield readout,
## overstaffing explanation, policy hint), never replacing it.
static func append_status_tooltip(tooltip: String, status: String) -> String:
    var status_line := status_tooltip_line(status)
    if status_line == "":
        return tooltip
    return status_line if tooltip == "" else tooltip + HudLayer.TOOLTIP_LINE_SEPARATOR + status_line

## Join the non-empty parts of a row tooltip (yield readout · policy behaviour · …) into one block.
static func join_tooltip_lines(lines: Array) -> String:
    var parts: Array[String] = []
    for line in lines:
        var text := String(line)
        if text != "":
            parts.append(text)
    return HudLayer.TOOLTIP_LINE_SEPARATOR.join(parts)

## A rung's display FACE — its `FoodIcons` glyph welded to its name. The one policy vocabulary every
## rung readout shares (the gate-reason lines, the work inspector's standing-investment line and its
## confirm), so a rung can never read one way beside the picker and another in the dialog.
static func policy_face(policy: String) -> String:
    return "%s%s" % [source_icon_prefix(FoodIcons.for_policy(policy)), policy.capitalize()]

## A 0..1 progress track (knowledge / domestication) as a whole percent. 0 is a MEANINGFUL reading in
## a gate reason — it tells the player they haven't started the track at all.
static func progress_percent(progress: float) -> int:
    return int(round(clampf(progress, 0.0, 1.0) * HudLayer.PROGRESS_PERCENT_SCALE))
