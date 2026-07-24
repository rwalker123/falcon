class_name DetailFormat

## THE SHARED DETAIL-RENDER LAYER (docs/plan_hud_decomposition.md).
##
## WHAT THIS IS. Everything that turns a list of `"Key: value"` detail LINES into the BBCode the HUD's
## detail surfaces actually show — the renderer (`detail_bbcode`), the per-row key→tint registry it
## consults, and the ~20 label / `*_value_hex` leaves those tints and the line PRODUCERS share. Plus
## the pure band-dict arithmetic behind the Food row (`band_net_food` and friends) and behind the Band
## panel's food-outlook chart, which reads the same family (`band_provisions` = the larder the
## projection starts from, `merged_arrival_schedule` = every source's arrivals summed slot-by-slot).
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
## CONSTS. The rule is: a const lives HERE iff every one of its readers moved here. The herd-drawer
## vocabulary came with `herd_summary_lines` (the pen/husbandry/range/size rows, `OVERGRAZING_WARNING`,
## `HERDERS_ROW`, `FULLY_HERDED`, `CORRAL_PROGRESS_COMPLETE`, `PEN_FEED_ROW`) and the expedition
## delivery/tooltip vocabulary with the tooltip trio, plus the recovery-guidance PAIR (the tint
## registry matches the glyph, the producer emits the text — splitting them across files would put a
## one-string invariant in two). Everything still shared with `HudLayer` (`Food`/`Morale` row keys, the
## morale-breakdown indent + sign glyphs, `CORRAL_GLYPH`, the `FOOD_LABEL_*` table) stays there and is
## read back as `HudLayer.X` — the `HudWidgets` / `HudFormat` convention, so there is exactly one place
## each phrase is typed.
##
## The one thing this module deliberately does NOT own is the POPOVER those disclosure carets open:
## that half needs a Node to `add_child` into, so it lives in `DisclosureController`. The two are
## bidirectionally coupled by the `[url]` meta this file emits and that one parses — split by node
## ownership, not by "formatter vs popover".

# ---- Detail-row carets (the disclosure affordance this file RENDERS; the popover it opens is
# `DisclosureController`'s). The meta PREFIX stays on HudLayer — both modules and both harnesses
# read it, so it is shared vocabulary rather than either half's own.
# ---- Consts absorbed from HudLayer (const/vocabulary extraction) ----
const FOOD_ACTION_FORAGE := "forage"

const FOOD_ACTION_HUNT := "hunt"

# Per-cohort morale cause (snapshot PopulationCohortState.moraleCause; 0 = None).
const MORALE_CAUSE_NONE := 0

const MORALE_CAUSE_TERRAIN := 1

const MORALE_CAUSE_COLD := 2

const MORALE_CAUSE_UNREST := 3

# Plain-language cause labels, shared by the drawer morale line and the alert reason.
# Cold reads "harsh climate" because the server penalty fires on hot OR cold deviation.
const MORALE_CAUSE_LABEL_TERRAIN := "harsh terrain"

const MORALE_CAUSE_LABEL_COLD := "harsh climate"

const MORALE_CAUSE_LABEL_UNREST := "unrest"

# |morale_delta| below this (0.5%/turn) reads as flat (no arrow), so trivial drift — nearly every tile
# bleeds a hair today — isn't shown as a decline. (The ▲/▼ ARROWS are `BandDetailLines`', the only
# thing that draws them.)
const MORALE_TREND_EPSILON := 0.005

# Itemized morale breakdown — the four signed Layer-1 contributions (their sum IS
# morale_delta) rendered as indented sub-lines under the Morale headline when morale is
# concerning or declining. Tinted by sign (▲ positive = healthy, ▼ negative = amber).
const MORALE_BREAKDOWN_INDENT := "    "

# (The two CONTRIBUTION LABELS `settling`/`culture` are `BandDetailLines`', and the recovery-guidance
# pair `DetailFormat`'s — each moved with every one of its readers.)
const MORALE_CONTRIB_POSITIVE_GLYPH := "▲"

const MORALE_CONTRIB_NEGATIVE_GLYPH := "▼"

# Positive-lever morale hints on the action buttons (tooltip suffixes).
const MORALE_HINT_SCOUT := "Scout unknown ground — reveals nearby tiles and lifts the band's spirits (+morale)."

const MORALE_HINT_PERSISTENT := "  Hunting a herd also lifts morale each turn (+morale/turn)."

const CORRAL_GLYPH := "🐄"

# ---- Band/City panel identity grid ---------------------------------------------------------------
# The panel's own header already states the band's name + settlement stage, so the summary rows there
# drop the `Unit: <name>` row (a THIRD copy of the same name) and replace `Size: <n>` (population
# under another name) with the labor line — same numbers, one row, in the identity grid where they
# belong. The Occupants-card drawer (FOREIGN bands, and the no-panel ui_preview fallback) keeps
# Unit/Size: it has no panel header naming the band, and a foreign band exposes no worker breakdown.
# The population/workers LINE is gone from the summary entirely: the band zone's People and
# Workforce bars state the same numbers as two readable charts, and a text restatement above them
# was the third telling of one fact.
# Category breakdown rows under Food reuse the morale breakdown's indent + ▲/▼ glyphs, so they flow
# through the SAME `DetailFormat.detail_bbcode` indented-sub-line path (sign-tinted: ▲ income green, ▼
# eaten amber) — no inline color tags, which mis-layout between the KV table segments.
const FOOD_LABEL_GATHERED := "Gathered"

const FOOD_LABEL_HUNTED := "Hunted"

# The two DEBIT rows, deliberately separate: the people eat (`food_consumption`), and the ANIMALS in
# the band's pens eat (`pen_feed_upkeep` — a confined herd cannot graze, so its keeper hauls it food
# every turn). Both come straight off the same larder, and telling them apart is the entire readout
# of the corral-as-a-managed-population arc: a band whose larder drains because it is feeding its
# herd must be able to SEE that, not just watch the number fall.
const FOOD_LABEL_EATEN := "Eaten (people)"

const FOOD_LABEL_PEN_FEED := "%s Pen feed (animals)" % CORRAL_GLYPH

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

# ---- Herder staffing. The row KEY is read by the herd-lines producer below AND by this file's tint
# registry; `FULLY_HERDED` is the `herded_fraction` wire default (1.0 = fully staffed, also
# unmanaged/vanished herds) — treated as "no problem". Staffed reads "N / N" (calm), under-herded
# "A / N — under-herded" (amber).
const HERDERS_ROW := "Herders"
const FULLY_HERDED := 1.0
const HERDERS_STAFFED_FORMAT := "%d / %d"
const HERDERS_UNDER_FORMAT := "%d / %d — under-herded"

# ---- Build-verb labels. "Building" / "Sowing" share the pen's "Fencing N%" convention: a rung under
# construction names the WORK, a finished one wears its own badge word. Each rung's "the meter is
# full" mark is its own const (progress arrives as 0..1 per rung).
const CORRAL_BUILDING_LABEL := "Building"
const CORRAL_PROGRESS_COMPLETE := 1.0
const HUSBANDRY_PROGRESS_COMPLETE := 1.0
const CULTIVATION_PROGRESS_COMPLETE := 1.0
const FIELD_PROGRESS_COMPLETE := 1.0
const FIELD_SOWING_LABEL := "Sowing"
const FIELD_BADGE_LABEL := "Field"

# ---- The pen's standing feed debit + its two starving states. The row KEY is read by the herd-lines
# producer below AND by this file's tint registry.
const PEN_FEED_ROW := "Pen feed"
const PEN_STARVING_LABEL := "⚠ Starving — %d%% fed"
const PEN_FEED_STARVING_FORMAT := "%s — only %d%% paid"

# ---- The penned herd's own rows (the fenced footprint + the three-way feed split). Every reader is
# `herd_summary_lines` below, so the whole block lives here.
const PEN_FOOTPRINT_ROW := "Pen"
const PEN_FOOTPRINT_FORMAT := "radius %d · %d tiles"
const PEN_FEED_SPLIT_ROW := "Fed by pasture"
# The `%s` is the optional hay segment (empty, or `PEN_FEED_SPLIT_HAY_SEGMENT`) spliced between the
# pasture percent and the NET larder bill — so a pen that drew no hay renders exactly the two-term form.
# The larder term reads `pen_larder_bill` (the NET bread bill after pasture + hay), NOT the gross
# `pen_upkeep`; sim-pinned invariant: `pen_upkeep × pen_pasture_fraction + pen_hay_food +
# pen_larder_bill == pen_upkeep`. A self-feeding pen reads "100% · larder 0.0", a scrub pen "0% ·
# larder N.N"; the hay segment shows ONLY when `pen_hay_food >= SourceForecast.FOOD_FLOW_MIN`.
const PEN_FEED_SPLIT_FORMAT := "%d%%%s · larder %.1f food/turn"
const PEN_FEED_SPLIT_HAY_SEGMENT := " · hay %.1f"

# ---- Husbandry-ceiling stand-ins. Rendered in place of the whole husbandry section on a wild-ceiling
# herd, and where the corral affordance would sit on a pastoral one — so the missing controls read as
# intentional, not a bug. Colon-free, so `detail_bbcode` renders them as dim informational sentences
# (the `kv.is_empty()` path).
const HUSBANDRY_WILD_HINT := "Wild game — hunt only"
const HUSBANDRY_PASTORAL_HINT := "Herdable, not pennable"

# ---- The under-herded CONSEQUENCE line. A managed herd slipping below full staffing loses tameness,
# so the drawer states why Penning stalled and the one lever that fixes it.
const HERDERS_SLIPPING_FORMAT := "Tameness slipping — teaching Herding, not Penning. Staff all %d herders to hold it."

# ---- Herd drawer grazing range (Grazing Phase 2b-iii): the ground the herd grazes — a SEPARATE fact
# from the biomass/cap pair the `Biomass` row carries. Key ≤ `DETAIL_KEY_MAX_LENGTH` so it aligns as a
# table row beside Biomass.
const HERD_RANGE_ROW := "Range"
# ---- Herd drawer size class: the `<size> game` class the roster row used to carry as its meta. The
# row's meta slot now states the herd's STAFFING, so the size class moved to the drawer.
const HERD_SIZE_ROW := "Size"
const HERD_SIZE_CLASS_FORMAT := "%s game"

# ---- Overgrazing: a TRIVIAL honest comparison of two sim-provided numbers (the ecology model is the
# sim's). The epsilon keeps a herd sitting exactly at K from flickering the warning; the warning SENTENCE
# is emitted by the producer below and matched verbatim by `detail_bbcode`'s WARN branch.
const OVERGRAZE_EPSILON := 0.05
const OVERGRAZING_WARNING := "⚠ Overgrazing — range can't sustain this herd"

# ---- Recovery guidance — a dim line naming the real levers (NOT harvest) when morale is concerning.
# The GLYPH is how `detail_bbcode` recognizes the line; the TEXT is what the morale-breakdown producer
# emits. They are one invariant ("the text begins with the glyph"), so it is spelled structurally here
# rather than as two literals that could drift.
const RECOVERY_GUIDANCE_GLYPH := "↑"
const RECOVERY_GUIDANCE_TEXT := RECOVERY_GUIDANCE_GLYPH + " Recover: move to Hospitable ground · Scout · Hunt"

# ---- Expedition delivery vocabulary (the `expedition_*` producers below are the only readers).
# Marks a hunt party's "Next delivery" line when the party relaunches for repeated trips (Market
# policy). Distinct from the Market policy glyph already shown (`FoodIcons.for_policy("market")` = ⇄),
# so the two never read as duplicated: ↻ = "this trip repeats", ⇄ = "the take is sold as trade goods".
const EXPEDITION_RECURRING_GLYPH := "↻"
# "Next delivery" lines for the two ways a projected-0 forecast can arise, disambiguated on the
# party's own `expedition_target_herd` (which MIGRATES and is often NOT the herd the player is
# looking at). Target still in the herd telemetry but forecast projects 0 → it is at/below its
# policy floor; target absent from telemetry → the herd was lost/replaced and the party is coming home.
const EXPEDITION_NEXT_DELIVERY_NO_SURPLUS := "Next delivery: none — its target herd has no surplus to raid"
const EXPEDITION_NEXT_DELIVERY_TARGET_LOST := "Next delivery: target herd lost — the party is returning home"
# The click affordance on an Active-expeditions row (the whole row is the button there).
const EXPEDITION_ROW_FOCUS_HINT := "Click to show this expedition on the map."

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
        if line.begins_with(DetailFormat.MORALE_BREAKDOWN_INDENT):
            if table_open:
                out += "[/table]\n"
                table_open = false
            var row_hex := HudStyle.HEALTHY_HEX if line.contains(DetailFormat.MORALE_CONTRIB_POSITIVE_GLYPH) else HudStyle.WARN_HEX
            out += "[color=#%s]%s[/color]\n" % [row_hex, line]
            continue
        # The overgrazing warning is a full-width WARN sentence (biomass > K), tinted with the same
        # WARN_HEX the Ecology/Corral value rows use — not a parallel styling path, just the shared color.
        if line == OVERGRAZING_WARNING:
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
    if key == HudDisclosureVocab.DETAIL_ROW_FOOD or key == "Provisions" or key == "Carried":
        # The band larder / expedition provisions / hunt-party carried-food row tints by the
        # larder-runway thresholds. It recognizes the row by the SHARED `FOOD_RUNWAY_UNIT` the one
        # renderer (`food_turns_text`) spells the runway with — never a bare literal, which is how
        # this guard silently went dead when the unit changed — or by the ∞ glyph for a band that is
        # not food-limited.
        if not is_nan(ctx.food_turns) and (value.contains(FOOD_RUNWAY_UNIT) or value.contains(FOOD_UNLIMITED_GLYPH)):
            return BandFoodStatus.hex_for_turns(ctx.food_turns)
    elif key == HudDisclosureVocab.DETAIL_ROW_MORALE:
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
    elif key == HudConst.TILE_SIGHT_KEY:
        # The tile's sight state: live cyan when in sight, dim when only remembered/unknown.
        return sight_value_hex(value)
    elif key == "Ecology" or key == HudFloraVocab.PASTURE_ECOLOGY_KEY:
        # Shared by the herd drawer, the forage-patch tile card and the tile card's PASTURE row — one
        # phase tint (neutral/amber/red) for every ecology in the game. The pasture row keeps its own
        # KEY only so a forage tile doesn't print two rows named "Ecology"; the styling path is
        # deliberately not forked.
        return ecology_value_hex(value)
    elif key == "Husbandry":
        return husbandry_value_hex(value)
    elif key == HERDERS_ROW:
        # A managed herd's staffing: amber when under-herded (tameness slipping), ink when full.
        return herders_value_hex(value)
    elif key == "Cultivation":
        return cultivation_value_hex(value)
    elif key == HudFloraVocab.FIELD_ROW:
        # Plant rung 3 — the patch twin of the Corral row's tint (ink while building, signal once
        # complete). Same shape as Cultivation's; kept its own case because a Field is a different
        # rung with its own badge word, not a Tended Patch at a higher percentage.
        return field_value_hex(value)
    elif key == "Corral":
        return corral_value_hex(value)
    elif key == PEN_FEED_ROW:
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
        HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX, String(st.get("key", "")),
        caret_hex, key, caret,
    ]

## Split a "Key: value" data line into [key, value]; returns [] for sentence lines (trailing period),
## long keys, or non-matching text so those stay full-width rather than becoming a lopsided table row.
static func _split_kv(line: String) -> Array:
    if line.ends_with("."):
        return []
    # The recovery-guidance line reads as a dim sentence, not a lopsided table row.
    if line.begins_with(RECOVERY_GUIDANCE_GLYPH):
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
    return value == HudConst.TILE_SIGHT_ACTIVE

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
        HudLayer._meter_bar(value * HudConst.PROGRESS_PERCENT_SCALE, DANGER_BAR_CELLS),
        int(round(value * HudConst.PROGRESS_PERCENT_SCALE)),
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
        return "%s Domesticated" % DetailFormat.CORRAL_GLYPH
    return "Domesticating %d%%" % int(round(progress * HudConst.PROGRESS_PERCENT_SCALE))

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
    if herded_fraction >= FULLY_HERDED:
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
        HudFloraVocab.CULTIVATION_PREPARING_LABEL, int(round(progress * HudConst.PROGRESS_PERCENT_SCALE)),
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
        return "%s %s" % [FoodIcons.for_policy(HudConst.LABOR_POLICY_SOW), FIELD_BADGE_LABEL]
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
        parts.append(HudFloraVocab.FLORA_SHARE_FORMAT % [String(entry["display_name"]), int(entry["percent"])])
    return FLORA_SHARE_SEPARATOR.join(parts)

## Player-facing corral label from pen-build progress (0.0–1.0) — the herd twin of
## `cultivation_label`. A finished pen shows the livestock glyph; an in-progress one reads
## "Building N%", naming the work under way. A finished pen whose keeper did NOT pay this turn's feed
## reads the STARVING state instead of the penned badge — the herd is losing biomass every turn,
## which is the one fact the player must not be able to miss. `detail_bbcode` tints via
## `corral_value_hex`.
static func corral_label(progress: float, corralled: bool, fed_fraction: float) -> String:
    if corralled or progress >= CORRAL_PROGRESS_COMPLETE:
        if PenStatus.is_starving(fed_fraction):
            return PEN_STARVING_LABEL % int(round(fed_fraction * HudConst.PROGRESS_PERCENT_SCALE))
        return "%s Corralled" % DetailFormat.CORRAL_GLYPH
    return "%s %d%%" % [CORRAL_BUILDING_LABEL, int(round(progress * HudConst.PROGRESS_PERCENT_SCALE))]

## The "Pen feed" row's value: what this pen demands per turn, plus — when the keeper is short — how
## much of it was actually paid. Amber/red-tinted via `pen_feed_value_hex`.
static func pen_feed_label(upkeep: float, fed_fraction: float) -> String:
    var demand := SourceForecast.format_yield(-upkeep)
    if PenStatus.is_starving(fed_fraction):
        return PEN_FEED_STARVING_FORMAT % [
            demand, int(round(fed_fraction * HudConst.PROGRESS_PERCENT_SCALE)),
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
    if HudExpeditionVocab.EXPEDITION_MISSION_LABELS.has(key):
        return HudExpeditionVocab.EXPEDITION_MISSION_LABELS[key]
    return key.capitalize() if key != "" else "Expedition"

## Plain-language label for a morale cause (0=None,1=Terrain,2=Cold,3=Unrest); "" for None or
## unknown. Shared by the drawer morale line and the losing-population alert reason.
static func morale_cause_label(cause: int) -> String:
    match cause:
        DetailFormat.MORALE_CAUSE_TERRAIN:
            return DetailFormat.MORALE_CAUSE_LABEL_TERRAIN
        DetailFormat.MORALE_CAUSE_COLD:
            return DetailFormat.MORALE_CAUSE_LABEL_COLD
        DetailFormat.MORALE_CAUSE_UNREST:
            return DetailFormat.MORALE_CAUSE_LABEL_UNREST
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
    return morale < BandFoodStatus.warn_morale() or delta <= -DetailFormat.MORALE_TREND_EPSILON


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
    return sum_realized_yield(band, SourceForecast.LABOR_KIND_FORAGE) \
        + sum_realized_yield(band, SourceForecast.LABOR_KIND_HUNT)

## What this band paid to feed its pens this turn (food/turn). 0 for a band that keeps no corral.
static func band_pen_feed(band: Dictionary) -> float:
    return float(band.get("pen_feed_upkeep", 0.0))

## The band's larder (provisions) as a float — the starting point of the food-outlook projection and
## the number the Food summary row prints (rounded there). Here beside the rest of the band food
## arithmetic the chart and the Food line share.
static func band_provisions(band: Dictionary) -> float:
    var stores_variant: Variant = band.get("stores", {})
    if stores_variant is Dictionary:
        return float((stores_variant as Dictionary).get(HudConst.STORE_ITEM_PROVISIONS, 0.0))
    return 0.0

## The band-wide merged arrival schedule: element-wise sum of every source's `arrival_schedule`, so
## slot i is ALL the food landing i+1 turns from now. Length = the longest schedule present (they are
## all `arrivals_horizon_turns` long in practice); empty when no source was projected, which is the
## signal to omit the Food-outlook block entirely rather than draw a flat starving line.
static func merged_arrival_schedule(band: Dictionary) -> PackedFloat32Array:
    var merged := PackedFloat32Array()
    for a in HudLayer._labor_assignments_of(band):
        if not (a is Dictionary):
            continue
        var schedule := HudBandLaborState.as_schedule((a as Dictionary).get("arrival_schedule", null))
        if schedule.is_empty():
            continue
        if merged.size() < schedule.size():
            merged.resize(schedule.size())
        for i in range(schedule.size()):
            merged[i] += schedule[i]
    return merged

## True when the band carries a meaningful food flow (income, consumption, or pen feed above the
## floor) — so a decode miss reads as "no flow" (net readout + breakdown omitted, not zeroed).
##
## **The income term MUST be the same `band_food_income` the headline sums, not the wire's lumpy
## `food_income`.** They diverged once and it hid the readout exactly when it was needed: a starving
## band has `food_consumption` 0 (an empty larder debits nothing) and a whole-animal hunt pays 0 on a
## wait turn, so a band with a perfectly good STEADY income failed all three tests and lost its net
## line and breakdown entirely. Gate on the same number you display.
static func band_has_food_flow(band: Dictionary) -> bool:
    return band_food_income(band) >= SourceForecast.FOOD_FLOW_MIN \
        or float(band.get("food_consumption", 0.0)) >= SourceForecast.FOOD_FLOW_MIN \
        or band_pen_feed(band) >= SourceForecast.FOOD_FLOW_MIN

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
    var glyph := DetailFormat.MORALE_CONTRIB_POSITIVE_GLYPH if value > 0.0 else DetailFormat.MORALE_CONTRIB_NEGATIVE_GLYPH
    return "%s%s %s  %s" % [DetailFormat.MORALE_BREAKDOWN_INDENT, glyph, SourceForecast.format_signed(value), label]


# =====================================================================================
#  PURE LINE PRODUCERS
#
#  The detail-line producers that turned out to hold no HUD state at all once their single
#  reach-out was threaded in as a parameter (`world_herds` for the herd drawer's danger bars,
#  the already-resolved `target_herd` for the expedition delivery lines). The STATEFUL producers
#  — the band summary rows, which read the labor model and register disclosures — live in
#  `BandDetailLines` instead.
# =====================================================================================

## The HERD drawer's rows. The split with the roster row above this drawer: the ROW carries identity
## (species glyph + name) and STAFFING (`1 🏹`) — so no `Herd` / `Species` row here, which would be
## the same name a second time. The SIZE class lives here because the row's one meta slot now belongs
## to the staffing count, and the drawer is where the facts that don't fit the row live. Everything
## below it is what the row can't show anyway: the herd's state.
##
## `world_herds` is THREADED IN (it is only ever forwarded to `append_danger_component_lines`, whose
## Attack/Defense bars normalize against the roster) — the same treatment the tint `Context` gets, and
## what makes this producer pure. Callers pass `HudBandLaborState.world_herds()`.
static func herd_summary_lines(herd_data: Dictionary, world_herds: Array) -> Array[String]:
    var lines: Array[String] = []
    var size_class := String(herd_data.get("size_class", "")).strip_edges()
    if size_class != "":
        lines.append("%s: %s" % [HERD_SIZE_ROW, HERD_SIZE_CLASS_FORMAT % size_class.capitalize()])
    # Biomass carries the herd's CURRENT head vs the K its range supports as a `current / max` pair
    # (`11636 / 11636`) — the convention the forage patch ("Forage biomass: 84 / 120") and the tile
    # card ("Pasture: 236 / 240") already use. K is derived each turn from the graze on the herd's
    # range; an overgrazed herd has `biomass > K`, so the pair honestly reads `current > max` (e.g.
    # `2100 / 1352`) — a FEATURE that makes the overshoot visible in the numbers (the ⚠ row below
    # spells out the consequence). The `~` the old standalone `Carrying cap` row carried is dropped:
    # a `current / max` pair already implies the max is the derived ceiling. Guard: a herd momentarily
    # on barren range derives K = 0, so `carrying_capacity <= 0` falls back to the bare `Biomass: X`
    # (never `X / 0`) and suppresses the overgrazing test below.
    var corralled := bool(herd_data.get("corralled", false))
    var carrying_capacity := float(herd_data.get("carrying_capacity", 0.0))
    var biomass: float = float(herd_data.get("biomass", 0.0))
    if biomass > 0.0:
        if carrying_capacity > 0.0:
            lines.append("Biomass: %d / %d" % [int(round(biomass)), int(round(carrying_capacity))])
        else:
            lines.append("Biomass: %.0f" % biomass)
    # The grazing range — WHY the herd is this size (the tiles it grazes / derives K over). A CORRALLED
    # herd doesn't roam-graze a range, so its Range row + overgrazing test are meaningless (its K is a
    # frozen pen-time value); the penned herd keeps the merged `Biomass: X / Y` pair, plainly.
    if not corralled:
        var range_radius := int(herd_data.get("graze_range_radius", 0))
        lines.append("%s: %s" % [HERD_RANGE_ROW, graze_range_label(range_radius)])
    # Overgrazing: biomass exceeds what the range can sustainably feed (both numbers sim-provided — the
    # client compares, it does NOT re-derive the ecology). Suppressed for a corralled herd and when K is
    # unknown. The `X / Y` pair above already shows X > Y; this row states the consequence.
    if not corralled and carrying_capacity > 0.0 and biomass > carrying_capacity * (1.0 + OVERGRAZE_EPSILON):
        lines.append(OVERGRAZING_WARNING)
    var phase := String(herd_data.get("ecology_phase", "")).strip_edges().to_lower()
    if phase != "":
        lines.append("Ecology: %s" % ecology_phase_label(phase))
    # Predators Phase 0 — the four RAW combat components (strength ≠ danger), shown for EVERY herd
    # (a rabbit reads all-empty, a mammoth reads high-attack/high-fights-back/zero-aggressive — the
    # "deadly to hunt, no camp threat" story at a glance). No verdict word; each row is a relative bar
    # + the raw value, Elevation-style.
    append_danger_component_lines(lines, herd_data, world_herds)
    # Grazing 2d-δ — how far up the husbandry ladder THIS species can climb gates the whole section.
    # A WILD-ceiling herd shows NO husbandry track at all (just the hunt-only hint); a PASTORAL one
    # keeps the domestication track but can never be penned (hint where Corral would sit); a PEN one
    # (or empty/absent) shows the full ladder, exactly as before.
    var ceiling := SourceForecast.husbandry_ceiling(herd_data)
    if ceiling == SourceForecast.HUSBANDRY_CEILING_WILD:
        lines.append(HUSBANDRY_WILD_HINT)
    else:
        var domestication := float(herd_data.get("domestication", 0.0))
        if domestication > 0.0:
            lines.append("Husbandry: %s" % husbandry_label(domestication))
        # Staffing deficit — the fix for the silent "🐄 Domesticated but Penning stalled" playtest bug.
        # A managed herd needs `herders_needed` herders every turn to hold its tameness; understaffed,
        # its domestication decays, the herd slips back to WILD and stops earning Penning. Surface it
        # so the player has a signal to staff more herders. Shown only for a managed herd
        # (`herders_needed > 0`); `herded_fraction` defaults to FULLY_HERDED, so an unmanaged herd never
        # trips it. Fully staffed reads a calm "N / N"; under-herded an amber "A / N — under-herded".
        var herders_needed := int(herd_data.get("herders_needed", 0))
        if herders_needed > 0:
            var herded_fraction := float(herd_data.get("herded_fraction", FULLY_HERDED))
            var herders_assigned := int(round(herded_fraction * herders_needed))
            lines.append("%s: %s" % [HERDERS_ROW, herders_label(herders_assigned, herders_needed, herded_fraction)])
            # Make the CONSEQUENCE explicit when the herd is slipping AND has real tameness to lose:
            # a muted one-liner naming why Penning has stalled and the single lever that fixes it.
            if herded_fraction < FULLY_HERDED and domestication > 0.0:
                lines.append(HERDERS_SLIPPING_FORMAT % herders_needed)
        # A corralled herd is penned by the band (intensification ladder). SIGNAL-tinted, mirroring the
        # Husbandry/Ecology row treatment. While the keepers are still BUILDING the pen (0 < progress < 1
        # under the Corral policy) the same row reports the meter — the animal twin of the tile card's
        # "Cultivation N%" row, so the investment the player committed to is visibly under way.
        # A PENNED herd is a managed population: it eats from its keeper's larder every turn, and an
        # underfed one is shrinking right now. That is the loudest thing the drawer can say about it, so
        # the Corral row itself flips to the starving state (DANGER-tinted via `corral_value_hex`) and a
        # "Pen feed" row states the demand and how much of it the keeper actually paid.
        # The whole corral/pen readout is PEN-ceiling only — a pastoral herd can never be penned (the
        # server never builds one), so its Corral/pen rows are suppressed and a hint stands in their place.
        if ceiling == SourceForecast.HUSBANDRY_CEILING_PEN:
            var corral_progress := float(herd_data.get("corral_progress", 0.0))
            var fed_fraction := PenStatus.fed_fraction(herd_data)
            if bool(herd_data.get("corralled", false)):
                lines.append("Corral: %s" % corral_label(CORRAL_PROGRESS_COMPLETE, true, fed_fraction))
                # The pen is fenced LAND (Grazing 2d-γ): its footprint (radius + the SERVER's in-bounds
                # tile count, shown verbatim) and the feed SPLIT — how much of the herd's feed its own
                # grazed footprint covers vs what the keeper still hauls from the larder.
                var pen_radius := int(herd_data.get("pen_radius", 0))
                var footprint_tiles := int(herd_data.get("pen_footprint_tiles", 0))
                lines.append("%s: %s" % [PEN_FOOTPRINT_ROW, PEN_FOOTPRINT_FORMAT % [pen_radius, footprint_tiles]])
                # The larder term is the NET bread bill (`pen_larder_bill`), NOT the gross `pen_upkeep`.
                var larder_bill := float(herd_data.get("pen_larder_bill", 0.0))
                var pasture_fraction := float(herd_data.get("pen_pasture_fraction", 0.0))
                # Hay is the middle feed term, in food-equivalent units (`pen_hay_food`, NOT the
                # grass-unit `fodder_draw`), shown ONLY when the pen drew hay. pasture_food + hay +
                # larder == gross pen_upkeep (sim-pinned), so the three never double-count.
                var hay_food := float(herd_data.get("pen_hay_food", 0.0))
                var hay_segment := ""
                if hay_food >= SourceForecast.FOOD_FLOW_MIN:
                    hay_segment = PEN_FEED_SPLIT_HAY_SEGMENT % hay_food
                lines.append("%s: %s" % [PEN_FEED_SPLIT_ROW, PEN_FEED_SPLIT_FORMAT \
                    % [int(round(pasture_fraction * HudConst.PROGRESS_PERCENT_SCALE)), hay_segment, larder_bill]])
                # The standing "Pen feed" debit is the SAME food-larder bill the split's larder term
                # states (`pen_larder_bill`, net of pasture + hay), not the gross `pen_upkeep` — so a
                # pen fed for free by pasture + hay shows NO debit row, and the two never disagree.
                if larder_bill >= SourceForecast.FOOD_FLOW_MIN:
                    lines.append("%s: %s" % [PEN_FEED_ROW, pen_feed_label(larder_bill, fed_fraction)])
            elif corral_progress > 0.0:
                lines.append("Corral: %s" % corral_label(corral_progress, false, PenStatus.FULLY_FED))
        elif ceiling == SourceForecast.HUSBANDRY_CEILING_PASTORAL:
            lines.append(HUSBANDRY_PASTORAL_HINT)
    var x := int(herd_data.get("x", -1))
    var y := int(herd_data.get("y", -1))
    if x >= 0 and y >= 0:
        lines.append("Position: (%d, %d)" % [x, y])
    var next_x := int(herd_data.get("next_x", -1))
    var next_y := int(herd_data.get("next_y", -1))
    if next_x >= 0 and next_y >= 0:
        lines.append("Next waypoint: (%d, %d)" % [next_x, next_y])
    return lines

## An Active-expeditions row's hover text: everything the glyphs encode, in words — the mission, the
## hunt policy's behaviour hint, the phase + what it means, and the click affordance.
##
## `target_herd` is the party's OWN target resolved from the snapshot herd list ({} when it has none
## or the herd is gone) — threaded in for the same reason `world_herds` is: this layer holds no
## snapshot state. Callers pass `HudBandLaborState.expedition_target_herd(exp)`.
static func expedition_row_tooltip(exp: Dictionary, phase: String, target_herd: Dictionary) -> String:
    var mission := String(exp.get("expedition_mission", "")).strip_edges().to_lower()
    var policy_hint := ""
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_HUNT:
        var policy := String(exp.get("expedition_hunt_policy", "")).strip_edges().to_lower()
        policy_hint = String(HudComposeVocab.SEND_HUNT_POLICY_HINTS.get(policy, ""))
    return HudFormat.join_tooltip_lines([
        expedition_mission_label(mission), policy_hint,
        HudFormat.status_tooltip_line(phase), _expedition_delivery_tooltip_line(exp, mission, target_herd),
        EXPEDITION_ROW_FOCUS_HINT])

## The full-wording next-delivery line for a hunt row's tooltip — the compact `· ~14 in 6t` token on
## the row itself is legible-but-terse in the 300px column, so hover carries the same phrasing the
## drawer's `BandDetailLines.expedition_summary_lines` prints. Empty (dropped by
## `HudFormat.join_tooltip_lines`) for a scout party or a party not yet projecting a delivery.
static func _expedition_delivery_tooltip_line(exp: Dictionary, mission: String, target_herd: Dictionary) -> String:
    if mission != HudExpeditionVocab.EXPEDITION_MISSION_HUNT or not exp.has("expedition_projected_delivery"):
        return ""
    return expedition_next_delivery_line(exp, target_herd)

## The robust "Next delivery: …" wording, shared by the parties inspector strip
## (`BandDetailLines.expedition_summary_lines`) and the row tooltip (`expedition_row_tooltip`) so the
## two can never disagree. Caller has already confirmed this is a hunt party carrying the field. A
## projected 0 is a REAL answer, but it means one of TWO things — and the party's TARGET herd (which
## migrates and is often NOT the herd the player is inspecting) tells them apart: if the target id is
## still in the herd telemetry the raid returns empty because that herd is at/below its policy floor;
## if `target_herd` came back empty the target was lost/replaced and the party is coming home. Never
## blank the line as if there were no forecast at all, and never imply it is the herd on the tile the
## player is looking at.
static func expedition_next_delivery_line(exp: Dictionary, target_herd: Dictionary) -> String:
    var delivery := float(exp.get("expedition_projected_delivery", 0.0))
    if delivery <= 0.0:
        if target_herd.is_empty():
            return EXPEDITION_NEXT_DELIVERY_TARGET_LOST
        return EXPEDITION_NEXT_DELIVERY_NO_SURPLUS
    var amount := int(round(delivery))
    var eta := int(exp.get("expedition_eta_turns", 0))
    var line := ""
    if eta > 0:
        var turns_word := "turn" if eta == 1 else "turns"
        line = "Next delivery: ~%d food in %d %s" % [amount, eta, turns_word]
    else:
        line = "Next delivery: ~%d food (raid underway)" % amount
    if bool(exp.get("expedition_recurring", false)):
        line += "  %s" % EXPEDITION_RECURRING_GLYPH
    return line
