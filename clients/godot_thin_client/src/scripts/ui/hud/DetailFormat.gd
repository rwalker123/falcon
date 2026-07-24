class_name DetailFormat

## THE SHARED DETAIL-RENDER LAYER (docs/plan_hud_decomposition.md).
##
## WHAT THIS IS. Everything that turns a list of `"Key: value"` detail LINES into the BBCode the HUD's
## detail surfaces actually show — the renderer (`detail_bbcode`), the per-row key→tint registry it
## consults, and the ~20 label / `*_value_hex` leaves those tints and the line PRODUCERS share. Plus
## the pure band-dict arithmetic behind the Food row (`band_net_food` and friends), which the Band
## panel's food-outlook chart reads too.
##
## WHY IT IS ITS OWN FILE. Four clusters render detail rows through one formatter — the selection
## card's land drawer, its occupant drawer, the Band/City panel's vitals label and the disclosure
## popover — and the coming `BandPanelController` split would otherwise have to carry the formatter
## with it or inject it as a Callable. Same measurement that produced `SourceForecast` / `HudWidgets`.
##
## EVERYTHING HERE IS `static`, STATELESS AND PURE — no node, no `_hud` back-ref, no snapshot cache.
## The two pieces of HUD state the formatter used to reach sideways for are threaded as EXPLICIT
## PARAMETERS instead:
##   * the per-render TINT CONTEXT (`Context` below) — the selected band's food runway / morale /
##     output plus the disclosure carets. These were three `HudLayer` members written by the line
##     producers, reset by four different hosts and read ONLY here; a value passed down cannot be
##     stale, and cannot be reset in the wrong order.
##   * `world_herds` for the Attack/Defense reference bars (`append_danger_component_lines`), the same
##     thread-it-in treatment `SourceForecast` gave the grid-wrap pair.
##
## CONSTS. The rule is: a const lives HERE iff every one of its readers moved here. Everything still
## shared with `HudLayer` (row KEYS like `Food`/`Herders`/`Field`, the morale-breakdown indent + sign
## glyphs, `OVERGRAZING_WARNING`, the recovery-guidance pair, `CORRAL_GLYPH`, the `FOOD_LABEL_*`
## table) stays there and is read back as `HudLayer.X` — the `HudWidgets` / `HudFormat` convention, so
## there is exactly one place each phrase is typed.
##
## The one thing this module deliberately does NOT own is the POPOVER those disclosure carets open:
## that half needs a Node to `add_child` into, so it lives in `DisclosureController`. The two are
## bidirectionally coupled by the `[url]` meta this file emits and that one parses — split by node
## ownership, not by "formatter vs popover".

# ---- Detail-row carets (the disclosure affordance this file RENDERS; the popover it opens is
# `DisclosureController`'s). The meta PREFIX stays on HudLayer — both modules and both harnesses
# read it, so it is shared vocabulary rather than either half's own.
const BREAKDOWN_CARET_OPEN := "▾"
const BREAKDOWN_CARET_CLOSED := "▸"

# ---- Larder-runway vocabulary. The UNIT is spelled in exactly one place (`food_turns_text`) and the
# Food/Provisions/Carried threshold tint recognizes its row by looking for that same word — never a
# bare literal, which is how the guard silently went dead once when the unit changed from days.
const FOOD_UNLIMITED_GLYPH := "∞"
const FOOD_RUNWAY_UNIT := "turn"

# ---- Predators Phase 0 — the four RAW combat components (strength ≠ danger). Keys ≤ 16 chars so
# `_split_kv` aligns them as table rows. Attack/Defense are open-ended (bar relative to the roster
# max); Fights back / Aggressive are native 0..1 (bar + %).
const DANGER_ATTACK_ROW := "Attack"
const DANGER_DEFENSE_ROW := "Defense"
const DANGER_FEROCITY_ROW := "Fights back"
const DANGER_AGGRESSION_ROW := "Aggressive"
const DANGER_BAR_CELLS := 5
## The compact derived line the player reasons about: hunt cost vs unprovoked menace.
const DANGER_DERIVED_ROW := "Danger"
const DANGER_DERIVED_FORMAT := "Hunt %s · Threat %s"

# ---- Herder staffing labels (the row KEY `HERDERS_ROW` stays on HudLayer — the herd-lines producer
# and this file's tint registry both name it).
const HERDERS_STAFFED_FORMAT := "%d / %d"
const HERDERS_UNDER_FORMAT := "%d / %d — under-herded"

# ---- Build-verb labels. "Building" / "Sowing" share the pen's "Fencing N%" convention: a rung under
# construction names the WORK, a finished one wears its own badge word. Each rung's "the meter is
# full" mark is its own const (progress arrives as 0..1 per rung; `CORRAL_PROGRESS_COMPLETE` stays on
# HudLayer because the herd-lines producer passes it in explicitly).
const CORRAL_BUILDING_LABEL := "Building"
const HUSBANDRY_PROGRESS_COMPLETE := 1.0
const CULTIVATION_PROGRESS_COMPLETE := 1.0
const FIELD_PROGRESS_COMPLETE := 1.0
const FIELD_SOWING_LABEL := "Sowing"
const FIELD_BADGE_LABEL := "Field"

# ---- The pen's two starving states (the row KEYS stay on HudLayer).
const PEN_STARVING_LABEL := "⚠ Starving — %d%% fed"
const PEN_FEED_STARVING_FORMAT := "%s — only %d%% paid"

## Separator between the named plants on the tile card's "What grows here" row. (Its partner
## `FLORA_SHARE_FORMAT` stays on HudLayer — the crop picker prints it too.)
const FLORA_SHARE_SEPARATOR := " · "

## The longest `Key` `_split_kv` will align into a table row; anything wider reads as a sentence.
const DETAIL_KEY_MAX_LENGTH := 16
## The separator a data line puts between its key and its value.
const DETAIL_KV_SEPARATOR := ": "


## THE PER-RENDER TINT CONTEXT — what `detail_bbcode` needs to know about the band whose rows it is
## rendering, and nothing else. Built fresh by whichever host is about to render, filled by the line
## PRODUCERS as they emit the rows, and handed to the renderer. It replaced three `HudLayer` members
## (`_selected_band_food_turns` / `_selected_band_morale` / `_selected_band_output`) plus
## `_disclosure_state`, all of which were per-render out-parameters reached sideways.
##
## NAN means "no band" for each scalar: the corresponding row then renders in neutral ink, exactly as
## the old `is_nan` guards decided. `disclosures` is row-label → `{key, open, concerning}` (see
## `DisclosureController.state`); empty means no row wears a caret.
class Context extends RefCounted:
    var food_turns: float = NAN
    var morale: float = NAN
    var output: float = NAN
    var disclosures: Dictionary = {}


# =====================================================================================
#  THE RENDERER
# =====================================================================================

## Render selection detail lines as BBCode: consecutive "Key: value" rows become a 2-column table
## (dim key, bright value, per-row value tint) so the data aligns into columns, while sentences and
## section lines stay full-width and muted. Matches the mockup's Tile Banner body.
##
## `ctx` carries everything band-specific (see `Context`); pass nothing for a surface with no band
## behind it — the popover's own restate, the tile card, the unknown-contents note.
static func detail_bbcode(lines: Array, ctx: Context = null) -> String:
    var context := ctx if ctx != null else Context.new()
    var out := ""
    var table_open := false
    for raw in lines:
        var line := String(raw)
        if line == "":
            if table_open:
                out += "[/table]"
                table_open = false
            out += "\n"
            continue
        # Itemized morale / food breakdown sub-lines render full-width, tinted by their sign
        # glyph (▲ positive = healthy, ▼ negative = amber) — kept two-tone, not a rainbow. The
        # `\n` after `[/table]` forces a block break: a RichTextLabel `[table]` is inline, so text
        # emitted right after it otherwise floats onto the table's top-right when there's room.
        if line.begins_with(HudLayer.MORALE_BREAKDOWN_INDENT):
            if table_open:
                out += "[/table]\n"
                table_open = false
            var row_hex := HudStyle.HEALTHY_HEX if line.contains(HudLayer.MORALE_CONTRIB_POSITIVE_GLYPH) else HudStyle.WARN_HEX
            out += "[color=#%s]%s[/color]\n" % [row_hex, line]
            continue
        # The overgrazing warning is a full-width WARN sentence (biomass > K), tinted with the same
        # WARN_HEX the Ecology/Corral value rows use — not a parallel styling path, just the shared color.
        if line == HudLayer.OVERGRAZING_WARNING:
            if table_open:
                out += "[/table]\n"
                table_open = false
            out += "[color=#%s]%s[/color]\n" % [HudStyle.WARN_HEX, line]
            continue
        var kv := _split_kv(line)
        if kv.is_empty():
            if table_open:
                out += "[/table]\n"
                table_open = false
            out += "[color=#%s]%s[/color]\n" % [HudStyle.INK_DIM_HEX, line]
        else:
            if not table_open:
                out += "[table=2]"
                table_open = true
            out += "[cell]%s[/cell][cell][color=#%s]%s[/color][/cell]" % [
                _key_cell(String(kv[0]), context), _value_hex(String(kv[0]), String(kv[1]), context), kv[1],
            ]
    if table_open:
        out += "[/table]"
    return out

## THE KEY→TINT REGISTRY: which hex a row's VALUE renders in, keyed on the row's own label. Every
## detail surface in the game consults this one table, which is why the tile card's Sight /
## Habitability / Ecology cases live beside the band's Food / Morale / Output ones.
static func _value_hex(key: String, value: String, ctx: Context) -> String:
    if key == "Food" or key == "Provisions" or key == "Carried":
        # The band larder / expedition provisions / hunt-party carried-food row tints by the
        # larder-runway thresholds. It recognizes the row by the SHARED `FOOD_RUNWAY_UNIT` the one
        # renderer (`food_turns_text`) spells the runway with — never a bare literal, which is how
        # this guard silently went dead when the unit changed — or by the ∞ glyph for a band that is
        # not food-limited.
        if not is_nan(ctx.food_turns) and (value.contains(FOOD_RUNWAY_UNIT) or value.contains(FOOD_UNLIMITED_GLYPH)):
            return BandFoodStatus.hex_for_turns(ctx.food_turns)
    elif key == "Morale":
        # The player band's morale row tints by the morale thresholds.
        if not is_nan(ctx.morale):
            return BandFoodStatus.hex_for_morale(ctx.morale)
    elif key == "Output":
        # The productivity row tints by the output buckets (ink → amber → red).
        if not is_nan(ctx.output):
            return BandFoodStatus.hex_for_output(ctx.output)
    elif key == "Forage":
        # The tile's gather module reads in the success/ETA amber.
        return HudStyle.WARN_HEX
    elif key == "Habitability":
        # The tile's habitability rating tints by its bucket (green→red).
        return TileHabitability.hex_for_rating(value)
    elif key == HudLayer.TILE_SIGHT_KEY:
        # The tile's sight state: live cyan when in sight, dim when only remembered/unknown.
        return sight_value_hex(value)
    elif key == "Ecology" or key == HudLayer.PASTURE_ECOLOGY_KEY:
        # Shared by the herd drawer, the forage-patch tile card and the tile card's PASTURE row — one
        # phase tint (neutral/amber/red) for every ecology in the game. The pasture row keeps its own
        # KEY only so a forage tile doesn't print two rows named "Ecology"; the styling path is
        # deliberately not forked.
        return ecology_value_hex(value)
    elif key == "Husbandry":
        return husbandry_value_hex(value)
    elif key == HudLayer.HERDERS_ROW:
        # A managed herd's staffing: amber when under-herded (tameness slipping), ink when full.
        return herders_value_hex(value)
    elif key == "Cultivation":
        return cultivation_value_hex(value)
    elif key == HudLayer.FIELD_ROW:
        # Plant rung 3 — the patch twin of the Corral row's tint (ink while building, signal once
        # complete). Same shape as Cultivation's; kept its own case because a Field is a different
        # rung with its own badge word, not a Tended Patch at a higher percentage.
        return field_value_hex(value)
    elif key == "Corral":
        return corral_value_hex(value)
    elif key == HudLayer.PEN_FEED_ROW:
        # The pen's running feed cost: amber as a standing debit, red when it goes unpaid.
        return pen_feed_value_hex(value)
    return HudStyle.INK_HEX

## A disclosure row (Food/Morale) renders its key as a clickable `[url]` + ▸/▾ caret, which opens its
## breakdown in the shared POPOVER via `meta_clicked` → `DisclosureController` (never inline — see the
## BREAKDOWN_* consts). The caret is ▾ only while THIS row's popover is up. A CONCERNING row wears the
## caret in WARN rather than SIGNAL: the breakdown no longer opens itself, so the invitation to read
## it has to be visible.
static func _key_cell(key: String, ctx: Context) -> String:
    if not ctx.disclosures.has(key):
        return "[color=#%s]%s[/color]" % [HudStyle.INK_DIM_HEX, key]
    var st: Dictionary = ctx.disclosures[key]
    var caret := BREAKDOWN_CARET_OPEN if bool(st.get("open", false)) else BREAKDOWN_CARET_CLOSED
    var caret_hex := HudStyle.WARN_HEX if bool(st.get("concerning", false)) else HudStyle.SIGNAL_HEX
    return "[url=%s%s][color=#%s]%s %s[/color][/url]" % [
        HudLayer.BREAKDOWN_TOGGLE_META_PREFIX, String(st.get("key", "")),
        caret_hex, key, caret,
    ]

## Split a "Key: value" data line into [key, value]; returns [] for sentence lines (trailing period),
## long keys, or non-matching text so those stay full-width rather than becoming a lopsided table row.
static func _split_kv(line: String) -> Array:
    if line.ends_with("."):
        return []
    # The recovery-guidance line reads as a dim sentence, not a lopsided table row.
    if line.begins_with(HudLayer.RECOVERY_GUIDANCE_GLYPH):
        return []
    var idx := line.find(DETAIL_KV_SEPARATOR)
    if idx <= 0:
        return []
    var key := line.substr(0, idx)
    if key.length() > DETAIL_KEY_MAX_LENGTH:
        return []
    var value := line.substr(idx + DETAIL_KV_SEPARATOR.length())
    if value.strip_edges() == "":
        return []
    return [key, value]


# =====================================================================================
#  ROW LABELS + THEIR VALUE TINTS
#  Each pair is "how the row READS" beside "what colour that reading is", so a label tweak and its
#  tint guard can never drift apart.
# =====================================================================================

## In-sight reads LIVE, both unseen states read remembered. The one test behind both the row's BBCode
## hex and the chip's Color, so the two forms cannot drift apart.
static func sight_is_live(value: String) -> bool:
    return value == HudLayer.TILE_SIGHT_ACTIVE

## Value tint for the Sight row: in-sight reads live (SIGNAL cyan — the HUD's "this is current"
## color), while both unseen states read dim (INK_DIM). The row states what you KNOW, not what is
## wrong, so it never borrows the WARN/DANGER palette.
static func sight_value_hex(value: String) -> String:
    return HudStyle.SIGNAL_HEX if sight_is_live(value) else HudStyle.INK_DIM_HEX

## Player-facing label for a herd's / patch's / pasture's ecology phase. Stressed/Collapsing carry a
## warning glyph; `detail_bbcode` additionally tints the value (see `ecology_value_hex`).
static func ecology_phase_label(phase: String) -> String:
    match phase:
        "collapsing":
            return "⚠ Collapsing"
        "stressed":
            return "⚠ Stressed"
        "thriving":
            return "Thriving"
        _:
            return phase.capitalize()

## BBCode hex for an "Ecology" value: red for a collapsing group, amber for stressed, normal ink
## otherwise. Matched on the lowercased phase stems ("collaps"/"stress" from `EcologyPhase::as_str`)
## so tinting survives glyph/capitalization tweaks to the label.
static func ecology_value_hex(value: String) -> String:
    var normalized := value.to_lower()
    if normalized.contains("collaps"):
        return HudStyle.DANGER_HEX
    if normalized.contains("stress"):
        return HudStyle.WARN_HEX
    return HudStyle.INK_HEX

## Append the Predators combat-component rows (Attack / Defense / Fights back / Aggressive) plus the
## compact derived-danger summary. Attack + Defense are open-ended, so their bars normalize against
## the max across the KNOWN herds, Elevation-style — a herd reads relative to the roster, and falls
## back to a full bar if it IS the reference (no other herds, or it holds the max). Ferocity +
## Aggression are native 0..1 → bar + %, using the readable behaviour labels the player parses.
##
## `world_herds` is THREADED IN rather than reached for: this layer holds no snapshot state, so the
## roster it normalizes against must be the caller's (`HudBandLaborState.world_herds()` today).
static func append_danger_component_lines(lines: Array[String], herd_data: Dictionary, world_herds: Array) -> void:
    var attack := float(herd_data.get("attack", 0.0))
    var defense := float(herd_data.get("defense", 0.0))
    var ferocity := clampf(float(herd_data.get("ferocity", 0.0)), 0.0, 1.0)
    var aggression := clampf(float(herd_data.get("aggression", 0.0)), 0.0, 1.0)
    lines.append("%s: %s" % [DANGER_ATTACK_ROW, _danger_open_row(attack, "attack", world_herds)])
    lines.append("%s: %s" % [DANGER_DEFENSE_ROW, _danger_open_row(defense, "defense", world_herds)])
    lines.append("%s: %s" % [DANGER_FEROCITY_ROW, _danger_unit_row(ferocity)])
    lines.append("%s: %s" % [DANGER_AGGRESSION_ROW, _danger_unit_row(aggression)])
    # The compact derived line the player actually reasons about: hunt cost vs unprovoked menace.
    lines.append("%s: %s" % [DANGER_DERIVED_ROW, DANGER_DERIVED_FORMAT % [
        _format_danger_scalar(attack * ferocity), _format_danger_scalar(attack * aggression),
    ]])

## An OPEN-ENDED component (attack/defense): a bar relative to the roster max + the raw value. The bar
## normalizes against the biggest value of that component across `world_herds`; with no reference (max
## 0 / no herds) it degrades to the bare value with no bar, since a lone herd has nothing to compare to.
static func _danger_open_row(value: float, key: String, world_herds: Array) -> String:
    var reference := _world_herd_component_max(key, world_herds)
    var raw := _format_danger_scalar(value)
    if reference <= 0.0:
        return raw
    return "%s %s" % [HudLayer._meter_bar(value / reference * 100.0, DANGER_BAR_CELLS), raw]

## A NATIVE 0..1 component (ferocity/aggression): a bar + percent.
static func _danger_unit_row(value: float) -> String:
    return "%s %d%%" % [
        HudLayer._meter_bar(value * HudLayer.PROGRESS_PERCENT_SCALE, DANGER_BAR_CELLS),
        int(round(value * HudLayer.PROGRESS_PERCENT_SCALE)),
    ]

## The largest value of an open-ended combat component across the known herds — the reference the
## Attack/Defense bars normalize against (the Elevation-view idiom for an unbounded field).
static func _world_herd_component_max(key: String, world_herds: Array) -> float:
    var reference := 0.0
    for herd in world_herds:
        if herd is Dictionary:
            reference = maxf(reference, float((herd as Dictionary).get(key, 0.0)))
    return reference

## Format a combat scalar for display: whole numbers bare (`8`), fractions to one decimal (`0.5`),
## trailing zero stripped — the components read against the human-strength anchor of 1.0.
static func _format_danger_scalar(value: float) -> String:
    if is_equal_approx(value, round(value)):
        return "%d" % int(round(value))
    return String.num(value, 1)

## Tile-count label for a herd's grazing range from its hex radius — "the ground this herd grazes".
## The hex-disk count `1 + 3r(r+1)`: radius 0 → 1 tile (small game, its own hex), 1 → 7, 2 → 19. Same
## count the map ring draws, so the readout and the ring can never disagree. Singular for a lone tile.
static func graze_range_label(range_radius: int) -> String:
    var tiles := 1 + 3 * range_radius * (range_radius + 1)
    if tiles == 1:
        return "1 tile"
    return "%d tiles" % tiles

## Player-facing husbandry label from domestication progress (0.0–1.0). Fully tamed shows a livestock
## glyph; in-progress shows the percentage. `detail_bbcode` tints a Domesticated value via
## `husbandry_value_hex`.
static func husbandry_label(progress: float) -> String:
    if progress >= HUSBANDRY_PROGRESS_COMPLETE:
        return "%s Domesticated" % HudLayer.CORRAL_GLYPH
    return "Domesticating %d%%" % int(round(progress * HudLayer.PROGRESS_PERCENT_SCALE))

## BBCode hex for a "Husbandry" value: signal (positive) for a domesticated herd, normal ink while
## it's still being tamed. Matched on the label produced by `husbandry_label`.
static func husbandry_value_hex(value: String) -> String:
    if value.to_lower().contains("domesticated"):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

## The "Herders" row value: a calm "N / N" when fully staffed, an amber "A / N — under-herded" when
## the herd is decaying for lack of herders. Fully staffed uses [needed, needed] (assigned == needed
## at frac 1.0); under-herded uses the rounded assigned count. Tinted via `herders_value_hex`.
static func herders_label(assigned: int, needed: int, herded_fraction: float) -> String:
    if herded_fraction >= HudLayer.FULLY_HERDED:
        return HERDERS_STAFFED_FORMAT % [needed, needed]
    return HERDERS_UNDER_FORMAT % [assigned, needed]

## BBCode hex for a "Herders" value: WARN (amber) while the herd is under-herded and its tameness is
## slipping, normal ink when fully staffed. Matched on the label from `herders_label`, mirroring
## `corral_value_hex` / the overgrazing warning's shared WARN tint.
static func herders_value_hex(value: String) -> String:
    if value.to_lower().contains("under-herded"):
        return HudStyle.WARN_HEX
    return HudStyle.INK_HEX

## Player-facing cultivation label for a forage patch. A fully-tended patch shows a crop glyph; an
## in-progress patch shows the percentage. Mirrors `husbandry_label`; `detail_bbcode` tints a Tended
## value via `cultivation_value_hex`.
static func cultivation_label(progress: float, cultivated: bool) -> String:
    if cultivated or progress >= CULTIVATION_PROGRESS_COMPLETE:
        return "🌾 Tended Patch"
    # Lead with the build VERB, exactly as the herd's Husbandry row reads "Domesticating N%" — a bare
    # percentage buried in the tile card was easy to miss and broke parity with the animal side.
    return "%s %d%%" % [
        HudLayer.CULTIVATION_PREPARING_LABEL, int(round(progress * HudLayer.PROGRESS_PERCENT_SCALE)),
    ]

## BBCode hex for a "Cultivation" value: signal (positive) for a tended patch, normal ink while it's
## still being cultivated. Matched on the label from `cultivation_label`.
static func cultivation_value_hex(value: String) -> String:
    if value.to_lower().contains("tended"):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

## Player-facing label for the plant RUNG-3 meter — the patch twin of `corral_label` and the rung
## above `cultivation_label`. While the crop is going in it reads as a BUILD ("Sowing 40%"), using the
## same building-verb convention as the pen's "Building 40%" / the fence's "Fencing 60%"; once
## complete it is a **Field**, badged with its own glyph so it reads as a DIFFERENT THING from a
## 🌾 Tended Patch rather than as a bigger number — which is the whole point of rung 3.
static func field_label(progress: float, is_field: bool) -> String:
    if is_field or progress >= FIELD_PROGRESS_COMPLETE:
        return "%s %s" % [FoodIcons.for_policy(HudLayer.LABOR_POLICY_SOW), FIELD_BADGE_LABEL]
    return "%s %d%%" % [FIELD_SOWING_LABEL, HudFormat.progress_percent(progress)]

## BBCode hex for a "Field" value: signal (positive) for a completed Field, normal ink while the crop
## is still going in. Matched on the label from `field_label`, mirroring `cultivation_value_hex`.
static func field_value_hex(value: String) -> String:
    if value.to_lower().contains(FIELD_BADGE_LABEL.to_lower()):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

## The tile's plant composition as one compact line — `Hazel 45% · Oak Mast 30% · Berry Scrub 25%`.
##
## The wire list is ALREADY sorted (share descending, then species key ascending) and its shares sum
## to 1, so this only formats: the order is the sim's and is never re-derived here.
##
## THE DISPLAYED PERCENTAGES ALWAYS SUM TO 100. Rounding each share independently can total 99 or 101
## — a decomposition that visibly fails to decompose — so the remainder is folded into the LARGEST
## share, i.e. the first entry, where a ±1 is proportionally smallest. Returns "" for a tile with no
## composition (a biome that carries no forage), so no empty row renders.
static func flora_composition_text(composition: Variant) -> String:
    var entries := SourceForecast.flora_basket_entries(composition)
    if entries.is_empty():
        return ""
    var parts: Array[String] = []
    for entry in entries:
        parts.append(HudLayer.FLORA_SHARE_FORMAT % [String(entry["display_name"]), int(entry["percent"])])
    return FLORA_SHARE_SEPARATOR.join(parts)

## Player-facing corral label from pen-build progress (0.0–1.0) — the herd twin of
## `cultivation_label`. A finished pen shows the livestock glyph; an in-progress one reads
## "Building N%", naming the work under way. A finished pen whose keeper did NOT pay this turn's feed
## reads the STARVING state instead of the penned badge — the herd is losing biomass every turn,
## which is the one fact the player must not be able to miss. `detail_bbcode` tints via
## `corral_value_hex`.
static func corral_label(progress: float, corralled: bool, fed_fraction: float) -> String:
    if corralled or progress >= HudLayer.CORRAL_PROGRESS_COMPLETE:
        if PenStatus.is_starving(fed_fraction):
            return PEN_STARVING_LABEL % int(round(fed_fraction * HudLayer.PROGRESS_PERCENT_SCALE))
        return "%s Corralled" % HudLayer.CORRAL_GLYPH
    return "%s %d%%" % [CORRAL_BUILDING_LABEL, int(round(progress * HudLayer.PROGRESS_PERCENT_SCALE))]

## The "Pen feed" row's value: what this pen demands per turn, plus — when the keeper is short — how
## much of it was actually paid. Amber/red-tinted via `pen_feed_value_hex`.
static func pen_feed_label(upkeep: float, fed_fraction: float) -> String:
    var demand := SourceForecast.format_yield(-upkeep)
    if PenStatus.is_starving(fed_fraction):
        return PEN_FEED_STARVING_FORMAT % [
            demand, int(round(fed_fraction * HudLayer.PROGRESS_PERCENT_SCALE)),
        ]
    return demand

## BBCode hex for a "Corral" value: DANGER for a starving pen (the herd is shrinking NOW), signal
## (positive) once penned and fed, normal ink while it's being built. Matched on the label from
## `corral_label`, mirroring `cultivation_value_hex`.
static func corral_value_hex(value: String) -> String:
    var normalized := value.to_lower()
    if normalized.contains("starving"):
        return HudStyle.DANGER_HEX
    if normalized.contains("corralled"):
        return HudStyle.SIGNAL_HEX
    return HudStyle.INK_HEX

## BBCode hex for the "Pen feed" value: DANGER while the pen goes unfed (the herd is shrinking), WARN
## otherwise — a paid pen is still a standing debit on the larder, never good news.
static func pen_feed_value_hex(value: String) -> String:
    if value.to_lower().contains("paid"):
        return HudStyle.DANGER_HEX
    return HudStyle.WARN_HEX


# =====================================================================================
#  PURE LEAVES THE LINE PRODUCERS SHARE
# =====================================================================================

## Humanize an expedition mission id ("scout" → "Scouting expedition"); falls back to a capitalized
## token for an unknown/future mission (e.g. PR 2's "hunt").
static func expedition_mission_label(mission: String) -> String:
    var key := mission.strip_edges().to_lower()
    if HudLayer.EXPEDITION_MISSION_LABELS.has(key):
        return HudLayer.EXPEDITION_MISSION_LABELS[key]
    return key.capitalize() if key != "" else "Expedition"

## Plain-language label for a morale cause (0=None,1=Terrain,2=Cold,3=Unrest); "" for None or
## unknown. Shared by the drawer morale line and the losing-population alert reason.
static func morale_cause_label(cause: int) -> String:
    match cause:
        HudLayer.MORALE_CAUSE_TERRAIN:
            return HudLayer.MORALE_CAUSE_LABEL_TERRAIN
        HudLayer.MORALE_CAUSE_COLD:
            return HudLayer.MORALE_CAUSE_LABEL_COLD
        HudLayer.MORALE_CAUSE_UNREST:
            return HudLayer.MORALE_CAUSE_LABEL_UNREST
        _:
            return ""

## Human-readable food runway: the ∞ glyph when the source is not food-limited, otherwise a whole
## count of TURNS — spelled from the shared `FOOD_RUNWAY_UNIT`, which the Food-row tint guard in
## `_value_hex` also keys on, so the two can never disagree about the unit. One helper feeds every
## surface that shows it (the band Food line, the expedition Carried/Provisions rows, and the turn-orb
## starving alert), so the unit is stated in exactly one place.
static func food_turns_text(runway: float) -> String:
    if not BandFoodStatus.is_limited(runway):
        return FOOD_UNLIMITED_GLYPH
    var turns := int(round(runway))
    if turns == 1:
        return "%d %s" % [turns, FOOD_RUNWAY_UNIT]
    return "%d %ss" % [turns, FOOD_RUNWAY_UNIT]

## True when the band's morale warrants surfacing the itemized breakdown + recovery guidance: below
## the warn threshold, or falling by more than the trend epsilon.
static func morale_is_concerning(unit_data: Dictionary) -> bool:
    var morale := float(unit_data.get("morale", 1.0))
    var delta := float(unit_data.get("morale_delta", 0.0))
    return morale < BandFoodStatus.warn_morale() or delta <= -HudLayer.MORALE_TREND_EPSILON


# =====================================================================================
#  BAND FOOD ARITHMETIC
#  Pure `band`-dict math, shared by the Food summary row, its breakdown, and the Band panel's
#  FOOD OUTLOOK chart — which is why it lives here rather than travelling with either one.
# =====================================================================================

## Net per-turn food flow: income − what the PEOPLE eat − what the band's penned ANIMALS eat.
## Positive → the larder is growing. `pen_feed_upkeep` is the sim's own answer for the third term
## (`PopulationCohortState.penFeedUpkeep` — the food this band actually PAID for pen feed this turn,
## summed across every pen it keeps); the client must NOT re-derive it by summing the herds'
## `pen_upkeep`, and the identity `larder_delta == income − consumption − pen_feed` is pinned sim-side
## (`integration_tests/tests/pen_food_ledger.rs`). Omitting the term made this row LIE: a band with a
## Red Deer pen showed a surplus overstated by the ~1.74/turn its herd ate, then drained anyway.
static func band_net_food(band: Dictionary) -> float:
    return band_food_income(band) \
        - float(band.get("food_consumption", 0.0)) \
        - band_pen_feed(band)

## The STEADY total food income = Gathered + Hunted (Σ per-source realized average across the band's
## forage + hunt assignments). Summed from the SAME per-source realized values as the breakdown rows, so
## it equals Gathered + Hunted exactly — the honest long-run average of the lumpy per-turn take, so it
## does NOT swing. It feeds the headline net (`band_net_food` = income − Eaten − Pen feed) and the
## `food_is_concerning` gate. **Deliberately summed from the rows rather than read off a band-level
## wire field** — a separately-computed total could drift from the Gathered/Hunted rows it sits above,
## and this way the headline equals them by construction. (A cohort-level `foodIncomeAverage` existed
## for one commit and was retired as redundant; do not reintroduce it.)
static func band_food_income(band: Dictionary) -> float:
    return sum_realized_yield(band, HudLayer.LABOR_KIND_FORAGE) \
        + sum_realized_yield(band, HudLayer.LABOR_KIND_HUNT)

## What this band paid to feed its pens this turn (food/turn). 0 for a band that keeps no corral.
static func band_pen_feed(band: Dictionary) -> float:
    return float(band.get("pen_feed_upkeep", 0.0))

## True when the band carries a meaningful food flow (income, consumption, or pen feed above the
## floor) — so a decode miss reads as "no flow" (net readout + breakdown omitted, not zeroed).
##
## **The income term MUST be the same `band_food_income` the headline sums, not the wire's lumpy
## `food_income`.** They diverged once and it hid the readout exactly when it was needed: a starving
## band has `food_consumption` 0 (an empty larder debits nothing) and a whole-animal hunt pays 0 on a
## wait turn, so a band with a perfectly good STEADY income failed all three tests and lost its net
## line and breakdown entirely. Gate on the same number you display.
static func band_has_food_flow(band: Dictionary) -> bool:
    return band_food_income(band) >= HudLayer.FOOD_FLOW_MIN \
        or float(band.get("food_consumption", 0.0)) >= HudLayer.FOOD_FLOW_MIN \
        or band_pen_feed(band) >= HudLayer.FOOD_FLOW_MIN

## Sum of per-source `realized_yield` (the STEADY per-source average, food/turn) across this band's
## labor assignments of one kind — the category total behind the Food breakdown (Gathered = forage,
## Hunted = hunt). Reads the steady average (not the lumpy `actual_yield`) so the rows don't swing AND
## sum to the steady headline income (`band_food_income`); falls back to `actual_yield` if absent.
static func sum_realized_yield(band: Dictionary, kind: String) -> float:
    var total := 0.0
    for a in HudLayer._labor_assignments_of(band):
        if a is Dictionary and String((a as Dictionary).get("kind", "")).strip_edges().to_lower() == kind:
            var d := a as Dictionary
            total += float(d["realized_yield"]) if d.has("realized_yield") else float(d.get("actual_yield", 0.0))
    return total

## Food is "concerning" when the larder is net-draining OR the runway is below the warn threshold —
## mirroring `morale_is_concerning`'s below-warn / falling gate. It no longer auto-EXPANDS anything
## (a popover that pops itself open on a snapshot would be worse than the clipping it replaced); it
## marks the row's caret WARN, so a row with something worth reading still says so at a glance.
static func food_is_concerning(band: Dictionary) -> bool:
    var turns := float(band.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
    return band_net_food(band) < 0.0 \
        or (BandFoodStatus.is_limited(turns) and turns < BandFoodStatus.warn_turns())

## Per-row-per-band disclosure key — also the `[url]` meta payload and the popover's identity.
static func breakdown_key(kind: String, band: Dictionary) -> String:
    return "%s:%d" % [kind, int(band.get("entity", -1))]

## One `    ▲ +0.48  Gathered`-style breakdown row (morale-indent + sign glyph → shared tint path).
static func food_breakdown_row(value: float, label: String) -> String:
    var glyph := HudLayer.MORALE_CONTRIB_POSITIVE_GLYPH if value > 0.0 else HudLayer.MORALE_CONTRIB_NEGATIVE_GLYPH
    return "%s%s %s  %s" % [HudLayer.MORALE_BREAKDOWN_INDENT, glyph, SourceForecast.format_signed(value), label]
