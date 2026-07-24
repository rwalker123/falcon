class_name BandDetailLines
extends RefCounted

## THE STATEFUL BAND DETAIL-LINE PRODUCERS (HUD decomposition, docs/plan_hud_decomposition.md).
##
## WHAT THIS IS. The rows a BAND or a PARTY shows in whichever detail surface is hosting it — the
## Occupants-card drawer, the Band/City panel's vitals label, the parties inspector strip. It is the
## half of the detail-line family that is genuinely STATEFUL: `unit_summary_lines` registers the
## Food/Morale disclosures as it emits their rows, `expedition_summary_lines` resolves a party's
## migrating target off the snapshot herd list, and both need the herd vocabulary. The PURE half —
## `DetailFormat.herd_summary_lines` and the expedition tooltip trio — became statics on
## `DetailFormat` once their one reach-out was threaded in as a parameter.
##
## WHY IT IS ITS OWN FILE. Two consumers render these rows (the drawer via `HudLayer`, the dock via
## `BandPanelController`), so leaving them on `HudLayer` cost `BandPanelController` three of its nine
## Callable injections — `_unit_summary_lines` / `_expedition_summary_lines` / `_expedition_row_tooltip`
## — plus a typed adapter each. It now holds ONE typed ref to this module instead (the same idiom it
## already uses for `_selectioncard` / `_disclosures`) and calls `DetailFormat` statically for the
## tooltip, so its constructor drops to six Callables.
##
## THE INJECTION SURFACE IS ONE CALLABLE. `_herd_label_for_id` stays on `HudLayer` because resolving a
## herd id to a species reads THREE collaborators — the selection card's roster, the current selection,
## and the snapshot herd list — so it cannot fold onto `HudBandLaborState` the way `find_world_herd`
## did. `_is_player_unit` is a trivial private COPY (the `SelectionCardController` /
## `BandPanelController` precedent — a one-line predicate is not worth a Callable).
##
## IT NEVER SEES THE SELECTION MODEL. The old producers read `_selection` at exactly two sites, both
## `tile_info()["terrain_label"]` for the morale row's "it's the hex you're on" payload — ONE display
## string. It is a PARAMETER here (`terrain_label`), so this module holds no selection coupling at all;
## callers pass `SelectionCardController.selected_terrain_label()`.
##
## CONSTS. Same rule as `DetailFormat`: a const lives here iff every one of its readers moved here.
## The band/party row vocabulary below did; the rest lives in its own topic module — the
## `DETAIL_ROW_*` / `BREAKDOWN_KIND_*` disclosure vocabulary in `HudDisclosureVocab`, `MORALE_CAUSE_*`
## and the morale-breakdown indent + sign glyphs in `DetailFormat`, `STORE_ITEM_PROVISIONS` in
## `HudConst`, `OUTPUT_FULL` / `FOOD_FLOW_MIN` in `SourceForecast` — each read as `Module.X`.

# ---- The band's fodder (hay) larder row, shown beneath Food only for a band with a fodder economy
# (it has stockpiled hay, or it pays a pen bread bill it could offset with hay) — so a forager band
# with no animals never sprouts an empty Fodder line.
const BAND_FODDER_ROW_FORMAT := "Fodder: %.1f"

# ---- The hunt party's carry-ceiling FULL badge (shown when carried ≥ cap; the party heads home full).
const HUNT_FULL_BADGE := "· FULL"

# ---- Morale-trend arrow glyphs. Whether a trend reads as flat at all is `DetailFormat.MORALE_TREND_EPSILON`,
# which stays there — `DetailFormat.morale_is_concerning` tests it too.
const MORALE_TREND_FALLING_GLYPH := "▼"
const MORALE_TREND_RISING_GLYPH := "▲"

# ---- Morale-breakdown contribution labels this producer names. (The SIGN glyphs and the indent stay
# on `HudLayer` — `DetailFormat` renders indented sub-lines from them too.) A positive unrest
# contribution reads as "culture" (cohesion), negative as "unrest".
const MORALE_CONTRIB_LABEL_SETTLING := "settling"
const MORALE_CONTRIB_LABEL_CULTURE := "culture"

# ---- Accessible-stockpile rows (the band's reachable stores, from `accessible_stockpile`).
const STOCKPILE_RADIUS_FORMAT := "Stockpile: radius %d"
const STOCKPILE_AVAILABLE_FORMAT := "Available: %s"
const STOCKPILE_ENTRY_FORMAT := "%d %s"
const STOCKPILE_ENTRY_SEPARATOR := ", "

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
# The snapshot herd list, for a hunt party's migrating target.
var _band_labor: HudBandLaborState = null
# The Food/Morale caret + popover cluster. `unit_summary_lines` clears its rows, registers the two
# disclosures as it emits them, and reads the caret state back onto the render context.
var _disclosures: DisclosureController = null

# --- The one retained HudLayer helper, injected as a Callable (see the class header) ---
# Reached through the typed adapter below rather than called raw: `Callable.call` returns `Variant`,
# which would push an untyped value into every consumer here.
var _herd_label_for_id_fn: Callable

# --- Owned state (moved off HudLayer) ---
# A PRIVATE HANDSHAKE BETWEEN TWO PRODUCERS IN THIS FILE, and nothing more: `_band_food_line` sets it
# when the band carries real food flow, and `unit_summary_lines` — its only reader — uses it to decide
# whether to register the Food row as a disclosure. The DETAIL FORMATTER never sees it (the caret is
# driven by the registered disclosure state, not by this flag), so it is deliberately NOT part of the
# render context that travels to `DetailFormat`.
var _food_flow_present: bool = false

func _init(band_labor: HudBandLaborState, disclosures: DisclosureController,
        herd_label_for_id: Callable) -> void:
    _band_labor = band_labor
    _disclosures = disclosures
    _herd_label_for_id_fn = herd_label_for_id

## A friendlier label for a herd id. Retained on HudLayer, which resolves it from the roster, the
## current selection AND the snapshot herd list, and which also feeds the targeting banner and the
## command feed from it.
func _herd_label_for_id(herd_id: String) -> String:
    return String(_herd_label_for_id_fn.call(herd_id))

## Player-faction check for a roster/drawer band (a trivial private copy of HudLayer's, the
## `SelectionCardController` / `BandPanelController` precedent).
func _is_player_unit(unit: Dictionary) -> bool:
    return int(unit.get("faction", HudConst.PLAYER_FACTION_ID)) == HudConst.PLAYER_FACTION_ID

# ---- The two public producers ---------------------------------------------------------------------

## The band summary rows. **No row here restates what its host's own header already shows.** Both
## hosts name the band above the detail — the Band/City dock in its panel header, the Occupants card
## in the band's roster row — and the roster row also carries the band's SIZE, so neither the
## `Unit: <name>` row nor the `Size: <n>` row survives.
## Nor does it state the population: the band zone's People + Workforce bars carry that, and the
## Occupants-card drawer has no worker breakdown to show for a band that isn't ours anyway.
##
## `terrain_label` is the SELECTED TILE's biome name — the morale row's "it's the hex you're on"
## payload, and the only thing these producers ever asked the selection model for. Passed in so this
## module holds no selection coupling.
func unit_summary_lines(unit_data: Dictionary, terrain_label: String,
        ctx: DetailFormat.Context = null) -> Array[String]:
    # The tint context is an OUT-PARAMETER of this producer, not a member: the caller (each of the two
    # detail hosts) builds it and hands it straight to the formatter. Defaulted so the preview
    # harnesses can still ask for the lines alone.
    var context := ctx if ctx != null else DetailFormat.Context.new()
    if bool(unit_data.get("is_expedition", false)):
        return expedition_summary_lines(unit_data, context)
    var lines: Array[String] = []
    # Disclosure carets + the tint context are rebuilt per render. Reset BOTH here, not inside
    # `_band_food_line` — a foreign band skips that call entirely (below), and a skipped Food row
    # must not inherit the previous render's caret or its food-turns tint.
    _disclosures.clear_rows()
    _food_flow_present = false
    context.food_turns = NAN
    # Food, like Morale below, is our OWN bands' business only. A rival's cohort carries no
    # `turns_of_food`/`stores` on the wire, so rendering the row for one printed a FABRICATED
    # `Food 0 (∞)` in healthy green — the UI claiming we'd counted a larder we cannot see. A foreign
    # band shows only what we can honestly observe from outside: where it is (Position) and roughly
    # how many (its roster row's size).
    if _is_player_unit(unit_data):
        lines.append(_band_food_line(unit_data, context))
        # Category-aggregated food breakdown under Food: a click-to-open disclosure. `_band_food_line`
        # set `_food_flow_present` (a PRIVATE handshake between the two — the formatter never reads
        # it); `DisclosureController.register` stashes the rows for the popover and records the row so
        # the formatter draws the caret + clickable meta. The rows are NEVER appended here — inline
        # growth is what clipped the zone.
        if _food_flow_present:
            _disclosures.register(HudDisclosureVocab.DETAIL_ROW_FOOD, HudDisclosureVocab.BREAKDOWN_KIND_FOOD, unit_data,
                _disclosures.food_breakdown_lines(unit_data))
        # The band's fodder (hay) larder, beneath its food larder — shown only for a band with a
        # fodder economy: it has stockpiled hay, or it pays a pen bread bill it could offset with hay.
        var fodder_store := float(unit_data.get("fodder_store", 0.0))
        if fodder_store > SourceForecast.FOOD_FLOW_MIN or float(unit_data.get("pen_feed_upkeep", 0.0)) > SourceForecast.FOOD_FLOW_MIN:
            lines.append(BAND_FODDER_ROW_FORMAT % fodder_store)
    # Morale is our own bands' business only (a non-player band's morale isn't ours
    # to see); morale drives productivity + migration (a harsh tile erodes it until
    # people begin leaving), while deaths stay starvation/cold-driven.
    if _is_player_unit(unit_data):
        lines.append(_band_morale_line(unit_data, terrain_label, context))
        # Productivity ties visibly to morale: show the Output row when discontent is
        # dragging yield below full (near Morale, tinted by how low it is).
        var output_line := _band_output_line(unit_data, context)
        if output_line != "":
            lines.append(output_line)
        # Itemized morale breakdown: the SAME click-to-open disclosure as Food, in the same popover.
        # Only offered when there's actually a breakdown to show (a contribution above the epsilon, or
        # the concerning recovery line) — `register` declines an empty payload.
        _disclosures.register(HudDisclosureVocab.DETAIL_ROW_MORALE, HudDisclosureVocab.BREAKDOWN_KIND_MORALE, unit_data,
            _morale_breakdown_lines(unit_data, terrain_label))
    var pos_array: Array = Array(unit_data.get("pos", []))
    if pos_array.size() == 2:
        lines.append("Position: (%d, %d)" % [int(pos_array[0]), int(pos_array[1])])
    # Per-source labor is now shown by the allocation panel (a real −/+ control set),
    # not as drawer text; the old single-task harvest/scout summaries are retired.
    var stockpile_variant: Variant = unit_data.get("accessible_stockpile", {})
    if stockpile_variant is Dictionary:
        var stockpile_lines := _accessible_stockpile_lines(stockpile_variant)
        if not stockpile_lines.is_empty():
            lines.append("")
            lines.append_array(stockpile_lines)
    # The carets this render registered are the LAST thing the context needs; read them back here so
    # every caller gets a fully-filled context by simply passing it in.
    context.disclosures = _disclosures.state()
    return lines

## Drawer readout for a selected expedition (docs/plan_exploration_and_sites.md §2 / §2b):
## mission, humanized phase, party size, and carried food (from stores/turnsOfFood). A hunt
## expedition (§2b) also lists the target herd it follows. Expeditions have no labor in v1, so
## this replaces the band's labor/morale rows entirely.
## Like the band + herd drawers, it carries NO identity row: an expedition rides the same
## roster path as a band, so its roster row (`_build_band_row`) already shows the very
## `id` the old `Unit:` line printed — nothing is lost with it (unlike the herd's fauna id, which
## had to move INTO the row). `Policy` / `Phase` deliberately keep their WORDS here: the compact
## Active-expeditions row is where the glyph vocabulary belongs; this block IS the disclosure.
func expedition_summary_lines(unit_data: Dictionary, ctx: DetailFormat.Context = null) -> Array[String]:
    # Same out-parameter contract as `unit_summary_lines`: the Carried/Provisions rows tint by the
    # party's own food runway, which is stashed on the context below. Defaulted for the harnesses.
    var context := ctx if ctx != null else DetailFormat.Context.new()
    var lines: Array[String] = []
    var mission := String(unit_data.get("expedition_mission", ""))
    var is_hunt := mission == HudExpeditionVocab.EXPEDITION_MISSION_HUNT
    # The party's OWN target, resolved once: the `Target:` row's live position and the delivery line's
    # lost-vs-lean disambiguation are the same herd, so they must not be looked up twice.
    var target_herd: Dictionary = _band_labor.expedition_target_herd(unit_data) if is_hunt else {}
    lines.append("Mission: %s" % DetailFormat.expedition_mission_label(mission))
    if is_hunt:
        # The migratory herd it follows (species label from the fauna_id, falling back to the id).
        # A hunt party's target MIGRATES and is often NOT the herd on the tile the player is looking
        # at, so when the target is still in the telemetry with a live position we append it — the
        # player can then tell "my party is bound to a boar at (68, 30)" from a healthy boar nearby.
        # When the target is absent (lost/replaced), the delivery line already says so, so we leave
        # the row as just the species/id.
        var herd_id := String(unit_data.get("expedition_target_herd", "")).strip_edges()
        if herd_id != "":
            var target_line := "Target: %s" % _herd_label_for_id(herd_id)
            if not target_herd.is_empty():
                var tx := int(target_herd.get("x", -1))
                var ty := int(target_herd.get("y", -1))
                if tx >= 0 and ty >= 0:
                    target_line += " (%d, %d)" % [tx, ty]
            lines.append(target_line)
        # The launched take policy (Sustain/Surplus/Market/Eradicate).
        var policy := String(unit_data.get("expedition_hunt_policy", "")).strip_edges()
        if policy != "":
            lines.append("Policy: %s" % policy.capitalize())
    var phase := String(unit_data.get("expedition_phase", "")).strip_edges()
    if phase != "":
        lines.append("Phase: %s" % HudFormat.expedition_phase_label(phase))
    # NO `Party` row: it printed `unit_data["size"]` — the exact field the roster row already shows as
    # its size meta (`Hunters 1 … 5`), so it was the band `Size` restatement under another name.
    # Food it carries — larder-drawn provisions for a scout, the hunted haul for a hunt party —
    # turns from turnsOfFood. Reuse the food-turns tint context, read back by the formatter.
    var turns: float = float(unit_data.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
    context.food_turns = turns
    var carried := 0
    var stores_variant: Variant = unit_data.get("stores", {})
    if stores_variant is Dictionary:
        if is_hunt:
            # The hunt party lives off its own kills; its store item key isn't fixed, so total it.
            for qty in (stores_variant as Dictionary).values():
                carried += int(round(float(qty)))
        else:
            carried = int(round(float((stores_variant as Dictionary).get(HudConst.STORE_ITEM_PROVISIONS, 0.0))))
    if is_hunt:
        # Carried X / cap + a FULL badge at the carry ceiling (the party heads home when full).
        var cap := int(round(float(unit_data.get("expedition_carry_cap", 0.0))))
        if cap > 0:
            var full_badge := "  %s" % HUNT_FULL_BADGE if carried >= cap else ""
            lines.append("Carried: %d / %d  (%s)%s" % [carried, cap, DetailFormat.food_turns_text(turns), full_badge])
        else:
            lines.append("Carried: %d  (%s)" % [carried, DetailFormat.food_turns_text(turns)])
        # Next-delivery forecast (the in-flight twin of the pre-launch hunt trip estimate): ALWAYS
        # shown for a hunt party once the field is on the wire, because a projected 0 is a real,
        # decision-relevant answer ("this herd has no surplus to raid") that a `> 0` guard used to
        # hide. The gate is `has(...)`, not `> 0`: the native decoder always inserts the field now, so
        # present-and-0 is a genuine no-surplus; an ABSENT key (older build) renders nothing rather
        # than a false "none".
        if unit_data.has("expedition_projected_delivery"):
            lines.append(DetailFormat.expedition_next_delivery_line(unit_data, target_herd))
    else:
        lines.append("Provisions: %d  (%s)" % [carried, DetailFormat.food_turns_text(turns)])
    var pos_array: Array = Array(unit_data.get("pos", []))
    if pos_array.size() == 2:
        lines.append("Position: (%d, %d)" % [int(pos_array[0]), int(pos_array[1])])
    return lines

# ---- The band rows `unit_summary_lines` assembles -------------------------------------------------

## Selection-panel band food row: "Food  <provisions>  (<turns>)" — provisions from
## the band's larder stores, turns from `turns_of_food` (∞ when not food-limited).
## Stashes the turns on the render context so `DetailFormat.detail_bbcode` can
## tint the value by the shared warn/critical thresholds.
func _band_food_line(unit_data: Dictionary, ctx: DetailFormat.Context) -> String:
    var turns: float = float(unit_data.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
    ctx.food_turns = turns
    var provisions := 0
    var stores_variant: Variant = unit_data.get("stores", {})
    if stores_variant is Dictionary:
        provisions = int(round(float((stores_variant as Dictionary).get(HudConst.STORE_ITEM_PROVISIONS, 0.0))))
    var line := "Food: %d  (%s)" % [provisions, DetailFormat.food_turns_text(turns)]
    # For player bands with real flow, append the net per-turn rate (sign-tinted, inline) and mark
    # the Food label a clickable disclosure. `_food_flow_present` is read ONLY by
    # `unit_summary_lines`, which decides whether to register that disclosure — the formatter never
    # sees it. An enemy band shows the bare larder line, exactly as before.
    _food_flow_present = false
    if _is_player_unit(unit_data) and DetailFormat.band_has_food_flow(unit_data):
        # The headline "/turn" is the STEADY net: income (Gathered + Hunted — the realized average,
        # so it no longer swings turn-to-turn) minus what the people (Eaten) and the pens (Pen feed)
        # draw off the larder. The breakdown below itemizes the income rows and the debits.
        var net := DetailFormat.band_net_food(unit_data)
        var net_hex := HudStyle.HEALTHY_HEX if net >= 0.0 else HudStyle.DANGER_HEX
        line += " · [color=#%s]%s[/color]" % [net_hex, SourceForecast.format_yield(net)]
        _food_flow_present = true
    return line

## Selection-panel band morale row: "Morale: 41% ▼ — harsh terrain (Karst Cavern Mouth)".
## Morale, its per-turn trend, and the dominant cause come from the snapshot cohort dict
## (decoded in `native/src/lib.rs population_to_dict`). A falling trend appends the named
## cause; Terrain names the band's tile (the "it's the hex you're on" payload — `terrain_label`,
## already stripped by the caller). A rehydrated save reports delta 0 / cause None for one turn, so
## the row degrades to a bare percentage.
## Stashes morale on the render context so `DetailFormat.detail_bbcode` tints the value.
func _band_morale_line(unit_data: Dictionary, terrain_label: String, ctx: DetailFormat.Context) -> String:
    var morale: float = float(unit_data.get("morale", 1.0))
    ctx.morale = morale
    var text := "Morale: %d%%" % int(round(morale * 100.0))
    var delta: float = float(unit_data.get("morale_delta", 0.0))
    if delta <= -DetailFormat.MORALE_TREND_EPSILON:
        text += " %s" % MORALE_TREND_FALLING_GLYPH
        # Name the cause only when morale is actually concerning — a healthy band
        # drifting slowly (nearly every tile bleeds a little today) shouldn't be
        # branded "harsh climate/terrain". Below the warn threshold, spell it out.
        if morale < BandFoodStatus.warn_morale():
            var cause := int(unit_data.get("morale_cause", DetailFormat.MORALE_CAUSE_NONE))
            var cause_label := DetailFormat.morale_cause_label(cause)
            if cause_label != "":
                if cause == DetailFormat.MORALE_CAUSE_TERRAIN and terrain_label != "":
                    cause_label = "%s (%s)" % [cause_label, terrain_label]
                text += " — %s" % cause_label
    elif delta >= DetailFormat.MORALE_TREND_EPSILON:
        text += " %s" % MORALE_TREND_RISING_GLYPH
    return text

## Selection-panel band productivity row: "Output: 56%" — the modifier-stack result
## (snapshot `output_multiplier`, discontent being Phase 1's sole modifier). Only shown
## below full output; stashes the value on the render context so `DetailFormat.detail_bbcode`
## tints it by the output.{warn,critical} buckets (ink → amber → red).
func _band_output_line(unit_data: Dictionary, ctx: DetailFormat.Context) -> String:
    var output: float = float(unit_data.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    if output >= SourceForecast.OUTPUT_FULL:
        return ""
    ctx.output = output
    return "Output: %d%%" % int(round(output * 100.0))

## Itemized morale breakdown: the four signed Layer-1 contributions (their sum IS morale_delta) as
## indented sub-lines, each above the breakdown epsilon rendered as `    ▲ +1.0%  settling`
## (`DetailFormat.detail_bbcode` tints by sign glyph). Now a click-to-expand disclosure (like Food): the
## contributions always compute so the row can be manually opened in the good state; the
## recovery-guidance line is appended ONLY when morale is concerning (don't tell a healthy band to
## "recover"). Returns [] when there is nothing to disclose (no contribution + not concerning).
func _morale_breakdown_lines(unit_data: Dictionary, terrain_label: String) -> Array[String]:
    var lines: Array[String] = []
    var terrain_row_label := DetailFormat.MORALE_CAUSE_LABEL_TERRAIN
    if terrain_label != "":
        terrain_row_label = "%s (%s)" % [DetailFormat.MORALE_CAUSE_LABEL_TERRAIN, terrain_label]
    var unrest_value := float(unit_data.get("morale_unrest", 0.0))
    # (value, label) in the display order of the spec: settling, terrain, climate, unrest.
    var contributions := [
        [float(unit_data.get("morale_settling", 0.0)), MORALE_CONTRIB_LABEL_SETTLING],
        [float(unit_data.get("morale_terrain", 0.0)), terrain_row_label],
        [float(unit_data.get("morale_climate", 0.0)), DetailFormat.MORALE_CAUSE_LABEL_COLD],
        [unrest_value, MORALE_CONTRIB_LABEL_CULTURE if unrest_value > 0.0 else DetailFormat.MORALE_CAUSE_LABEL_UNREST],
    ]
    var epsilon := BandFoodStatus.morale_breakdown_epsilon()
    for entry in contributions:
        var value: float = entry[0]
        if absf(value) < epsilon:
            continue
        var glyph := DetailFormat.MORALE_CONTRIB_POSITIVE_GLYPH if value > 0.0 else DetailFormat.MORALE_CONTRIB_NEGATIVE_GLYPH
        var sign_str := "+" if value > 0.0 else "−"
        lines.append("%s%s %s%.1f%%  %s" % [
            DetailFormat.MORALE_BREAKDOWN_INDENT, glyph, sign_str, absf(value) * 100.0, entry[1],
        ])
    # Recovery guidance is a "you have a problem" prompt — only when concerning.
    if DetailFormat.morale_is_concerning(unit_data):
        lines.append(DetailFormat.RECOVERY_GUIDANCE_TEXT)
    return lines

## The band's reachable stores: a radius line plus one comma-joined `<qty> <Item>` run. Travels with
## `unit_summary_lines`, its only caller; the item wording is `HudFormat.stockpile_label`, shared with
## the left-dock stockpile panel so an item is spelled the same in both.
func _accessible_stockpile_lines(stockpile: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var radius := int(stockpile.get("radius", 0))
    var entries_variant: Variant = stockpile.get("entries", [])
    var entries: Array = entries_variant if entries_variant is Array else []
    if entries.is_empty():
        return lines
    var formatted: Array[String] = []
    for entry in entries:
        if not (entry is Dictionary):
            continue
        var item := String(entry.get("item", ""))
        var qty := int(entry.get("quantity", 0))
        if item == "" and qty == 0:
            continue
        formatted.append(STOCKPILE_ENTRY_FORMAT % [qty, HudFormat.stockpile_label(item)])
    if formatted.is_empty():
        return lines
    lines.append(STOCKPILE_RADIUS_FORMAT % radius)
    lines.append(STOCKPILE_AVAILABLE_FORMAT % STOCKPILE_ENTRY_SEPARATOR.join(formatted))
    return lines
