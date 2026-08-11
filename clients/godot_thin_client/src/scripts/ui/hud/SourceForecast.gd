class_name SourceForecast

## THE SHARED SOURCE FORECAST / ESTIMATE LAYER (docs/plan_hud_decomposition.md, phase 2c-2 precursor).
##
## WHAT THIS IS. The pure "what will this source give me?" math the HUD asks from THREE independent
## places: the drawer's compose blocks, the Band panel's WORK zone, and the Band panel's PARTIES zone
## (the raid cluster). It answers three families of question and nothing else:
##   • POST-HOC — `source_yield_readout`: what a worked source actually produced this turn.
##   • PRE-COMMIT — `forecast_inputs` / `max_useful_workers` / `expected_yield`: what it WOULD produce
##     for N workers under a policy, and how many workers can usefully be pointed at it.
##   • THE RAID — `hunt_trip_forecast` and friends: what a detached hunting party delivers, over how
##     many turns, and whether the trip is worth taking at all.
##   • THE FLOOR'S INSTRUMENT — `floor_chart_model` and the layer under it (`regrowth_at`,
##     `project_stock`, `crew_to_clear` / `crew_to_hold`, `harvest_verdict`): the projection the
##     compose sheet's chart draws, the two crew targets beneath it and the sentence saying which of
##     the crew and the floor is BINDING. One model, three readings, so they cannot disagree.
##
## WHY IT IS ITS OWN FILE. The next phase lifts a `DrawerComposeController` out of `Hud.gd`, but this
## layer is called by the work zone and the parties zone TOO, so it cannot travel with the drawer. The
## two alternatives were measured and rejected: pure injection needs 54 `Callable`s, and a `_hud`
## back-reference would weld an already-pure layer to the god object (and the Band-panel extraction
## would then need a SECOND back-ref to the same place). Instead all three consumers depend on this.
##
## EVERYTHING HERE IS `static` AND STATELESS — no node, no `_hud`, no snapshot cache. That is the
## invariant that makes the file safe to call from anywhere, and it is worth defending: if a function
## you want to add needs HUD state, pass the state in as a parameter instead of holding it.
##
## THE ONE THING THAT ISN'T A PLAIN VALUE is the grid-wrap pair. Round-trip travel needs a wrap-aware
## hex distance, which needs (`grid_width`, `wrap_horizontal`) — snapshot facts `HudLayer` receives via
## `set_grid_dimensions`. They are threaded through as EXPLICIT PARAMETERS (`hex_distance_wrapped` →
## `round_trip_travel_turns` → `hunt_trip_forecast` / `expedition_policy_takes`) rather than held as
## module state, so a stale grid can never be captured here. `HudLayer._hex_distance_wrapped` is a
## one-line pass-through that supplies its own members: ONE hex implementation, no duplication.
##
## THE CLIENT MODELS NO ECOLOGY. Every ceiling, per-worker rate and raid estimate is a sim-exported
## number looked up here — never re-derived. See clients/godot_thin_client/CLAUDE.md for the contract.

# The band's productivity modifier at full strength: forecasts are exported at 1.0 and scaled by the
# acting band's `output_multiplier` at payout.
const OUTPUT_FULL := 1.0
# Assignment kinds mirror the sim's LaborAssignment.kind. Only the two SOURCE kinds live here (scout /
# warrior are band-wide roles with no source forecast); `source_yield_readout` branches on them.
const LABOR_KIND_FORAGE := "forage"
const LABOR_KIND_HUNT := "hunt"
# ---- THE ESCAPEMENT FLOOR (docs/plan_harvest_floor.md) ------------------------------------------
# WHERE A CREW STOPS, as a fraction of the source's carrying capacity — and since the harvest-floor
# arc the WHOLE of what the player decides about pressure. It replaced the four harvest stances
# (`sustain`/`surplus`/`deplete`/`eradicate`), which are gone from the sim entirely: the take is
# `max(0, B − floor·K) × rate` at ANY floor in 0..1, so four rows could only ever answer four of the
# questions a player dragging a dial asks. The four names are REJECTED BY NAME at the command
# boundary (`CommandParseError::RetiredStanceToken`), so a stale emitter fails loudly.
const FLOOR_MIN := 0.0
const FLOOR_MAX := 1.0
# The sim's `MSY_BIOMASS_FRACTION` / `DEFAULT_ESCAPEMENT_FLOOR`: the biomass a logistic source
# regrows fastest at, hence the floor that pays the most food per turn forever — the FOOD PEAK every
# other floor is read against. It is also what the sim assumes when a command carries no floor token.
const FLOOR_FOOD_PEAK := 0.5
const DEFAULT_HARVEST_FLOOR := FLOOR_FOOD_PEAK
# THE THREE INTENT PRESETS — marks on the dial, deliberately NOT a set of options. They exist so the
# three decisions a player actually makes are one click each; every value between them is reachable
# through the slider beside them, and the sim accepts any of them.
const FLOOR_PRESET_STRIP := "strip"
const FLOOR_PRESET_PEAK := "peak"
const FLOOR_PRESET_LEARN := "learn"
# Keyed by preset rather than by the float itself: a Dictionary keyed on a float compares by exact
# bits, and the whole point of a continuous dial is that the player's value rarely IS one of these.
const FLOOR_PRESET_VALUES := {
    FLOOR_PRESET_STRIP: FLOOR_MIN,
    FLOOR_PRESET_PEAK: FLOOR_FOOD_PEAK,
    # Above the peak: calories are traded for ladder progress (`intensification::learn_multiplier`
    # scales with the floor). 0.80 is the sim's own top raid-forecast sample.
    FLOOR_PRESET_LEARN: 0.80,
}
# In ASCENDING floor order — the presets read left to right as "take more now" → "leave more
# standing", which is the axis, so a picker never has to sort them.
const FLOOR_PRESETS := [FLOOR_PRESET_STRIP, FLOOR_PRESET_PEAK, FLOOR_PRESET_LEARN]
# Two floors closer than this are the SAME dial position. It is a display tolerance, not a model one:
# `floor_percent` rounds to whole percent, so anything finer cannot be told apart on screen or
# re-selected by clicking the preset it is sitting on.
const FLOOR_EPSILON := 0.005
# The slider's granularity — whole 5% steps of K. Fine enough to sit anywhere between two presets,
# coarse enough that the value is readable and reproducible; the drag in 4b replaces this control.
const FLOOR_STEP := 0.05
const FLOOR_PERCENT_SCALE := 100

# ---- WHERE THE FLOOR SITS RELATIVE TO THE FOOD PEAK ---------------------------------------------
# **ONE RULE, FIVE ZONES — this is what replaced the four per-stance hint tables.** A stance table
# had a row per name; a floor has no names, so the only thing that can be said about it is its
# RELATION to the peak, and that relation is the whole of what the dial means:
#   below the peak  you are spending the source's future for calories now;
#   above it        you are buying ladder progress with calories;
#   at 0            you strip it (and the two webs differ in what that COSTS — a patch reseeds, a
#                   herd dies out — which is the one per-web clause in the vocabulary);
#   at 1.0          you take nothing at all, and the crew is watching rather than working.
# Every surface that used to key text or a glyph on a stance name keys it on one of these instead.
const FLOOR_ZONE_STRIP := "strip"
const FLOOR_ZONE_DRAWDOWN := "drawdown"
const FLOOR_ZONE_PEAK := "peak"
const FLOOR_ZONE_LEARNING := "learning"
const FLOOR_ZONE_UNTOUCHED := "untouched"
# THE IMPROVEMENTS — the second axis: what a crew is building on the source, at most one at a time and
# always the source's NEXT rung. `IMPROVEMENT_NONE` ("") is the wire's own spelling of "building
# nothing", so a caller never has to distinguish it from an absent field.
const IMPROVEMENT_NONE := ""
const IMPROVEMENT_CULTIVATE := "cultivate"
const IMPROVEMENT_SOW := "sow"
const IMPROVEMENT_TAME := "tame"
const IMPROVEMENT_CORRAL := "corral"
# The plant ladder and the animal ladder, each in RUNG ORDER (low → high). Kept apart because the two
# webs never share a rung, and read by nothing that needs "all four".
const FORAGE_IMPROVEMENTS := [IMPROVEMENT_CULTIVATE, IMPROVEMENT_SOW]
const HUNT_IMPROVEMENTS := [IMPROVEMENT_TAME, IMPROVEMENT_CORRAL]
# A herd at or above this domestication progress is fully tamed (pastoral); its crew are keepers.
const DOMESTICATION_COMPLETE := 1.0
# WHICH KIND OF SOURCE a forecast dict describes, stated explicitly by every `forecast_inputs` caller:
# a herd and a raw wire forage patch share the empty key prefix, so the prefix cannot answer it and a
# shape test on a wire key would misread a source whose snapshot omitted it.
const SOURCE_KIND_HERD := "herd"
const SOURCE_KIND_FORAGE := "forage"

## THE ONE MAPPING between the two kind vocabularies — an ASSIGNMENT's `kind` (`LABOR_KIND_*`, the
## sim's own word) and a FORECAST's (`SOURCE_KIND_*`). They coincide on the plant web (`"forage"` both
## ways) and differ on the animal one (`"hunt"` the labor, `"herd"` the source), which is exactly the
## shape that lets a mix-up pass unnoticed on half the code and fail silently on the other half: a
## `"hunt"` handed to `forecast_inputs` takes the FORAGE branch, finds no per-policy row, and answers
## "this source has no forecast". A caller holding a labor kind converts here rather than by hand.
static func source_kind_for_labor(labor_kind: String) -> String:
    return SOURCE_KIND_FORAGE if labor_kind == LABOR_KIND_FORAGE else SOURCE_KIND_HERD

## A floor brought into the legal range the sim validates (`components::floor_is_valid`). Every entry
## point a floor arrives through — the wire, a preset, the slider, a rehydrated compose state — goes
## through here, so an out-of-range value is a clamped dial rather than a negative escapement room.
static func clamp_floor(floor: float) -> float:
    return clampf(floor, FLOOR_MIN, FLOOR_MAX)

## The floor as a whole percent of carrying capacity — the ONE way this client writes a floor as a
## number, so the slider, the picker face, the work row and the hint can never round it differently.
static func floor_percent(floor: float) -> int:
    return int(round(clamp_floor(floor) * FLOOR_PERCENT_SCALE))

## **WHERE THIS FLOOR SITS RELATIVE TO THE FOOD PEAK** — the one classification every floor-keyed
## word and glyph in the HUD reads (see the `FLOOR_ZONE_*` block). The two endpoints are their own
## zones because they are their own facts: at `0` the source is stripped, at `1.0` nothing is taken
## at all and the crew learns and builds nothing (`labor::crew_is_working_the_source`).
static func floor_zone(floor: float) -> String:
    var value := clamp_floor(floor)
    if value <= FLOOR_MIN + FLOOR_EPSILON:
        return FLOOR_ZONE_STRIP
    if value >= FLOOR_MAX - FLOOR_EPSILON:
        return FLOOR_ZONE_UNTOUCHED
    if absf(value - FLOOR_FOOD_PEAK) <= FLOOR_EPSILON:
        return FLOOR_ZONE_PEAK
    return FLOOR_ZONE_DRAWDOWN if value < FLOOR_FOOD_PEAK else FLOOR_ZONE_LEARNING

## The preset this floor IS sitting on, or `""` when it sits between two of them. `""` is the normal
## answer for a dragged dial and must render as "no preset selected" rather than snapping the display
## to the nearest one — a picker that lights a preset the player is not on is stating a false floor.
static func floor_preset_for(floor: float) -> String:
    var value := clamp_floor(floor)
    for preset in FLOOR_PRESETS:
        if absf(value - float(FLOOR_PRESET_VALUES[preset])) <= FLOOR_EPSILON:
            return String(preset)
    return ""

## A preset's floor. Falls back to the default (the food peak) for an unknown key, so a stale saved
## preset name lands on the sim's own default rather than on `0` — "strip it" is the one value that
## must never be reached by accident.
static func floor_for_preset(preset: String) -> float:
    return float(FLOOR_PRESET_VALUES.get(preset, DEFAULT_HARVEST_FLOOR))

# Whole-percent scale for a 0..1 share. The displayed numbers must ALWAYS sum to this: naive rounding
# can land on 99 or 101, and the remainder is absorbed into the largest share (the first entry — the
# wire list is share-descending), which is the one where a ±1 is least visible.
const FLORA_SHARE_PERCENT_TOTAL := 100
# 0 is the "cannot climb this rung" SENTINEL, not a ratio (a real one is never 0), so a row greyed by
# the climbability flags prints no number at all.
const FLORA_CROP_RATIO_NONE := 0.0

# Grazing 2d-δ — the per-species HUSBANDRY CEILING (`HerdTelemetryState.husbandryCeiling`): how far up
# the ladder a species can climb. "wild" = hunt-only (no husbandry track at all); "pastoral" =
# tameable + roams but can NEVER be penned (hide Corral + Extend); "pen" (or empty/absent) = the full
# ladder, everything as today. The herd drawer + assign controls gate their husbandry affordances on it.
const HUSBANDRY_CEILING_WILD := "wild"
const HUSBANDRY_CEILING_PASTORAL := "pastoral"
const HUSBANDRY_CEILING_PEN := "pen"

# Per-source food yield readout on the allocation rows. Yields are food/turn floats; render to
# 2 decimals with an explicit sign ("+0.31 /turn").
const YIELD_DECIMALS := 2
const YIELD_PER_TURN_SUFFIX := " /turn"
# The take's own qualifier, in two spellings of ONE word: the SUFFIX form for a joined sentence, the
# bare NOTE for the readout's yields row, which sets it as its own small-print part beside the number
# and therefore cannot carry the separator that joins a sentence. Written structurally, so the two
# cannot drift into two different words.
const YIELD_RENEWABLE_NOTE := "renewable"
const YIELD_TOOLTIP_RENEWABLE := " · " + YIELD_RENEWABLE_NOTE
const YIELD_TOOLTIP_OVERDRAW := " — overdrawing"
# Overstaffing (wasted labor) — DISTINCT from the ⚠ overdraw flag. Every policy caps a source's take at
# its ceiling (policy ceiling / resource biomass), so past `workers_needed` extra workers produce
# nothing HERE and should move elsewhere. A source can be overstaffed while perfectly sustainable (and
# overdrawn while fully used), so this reads as its own WARN-tinted note on the row rather than
# borrowing the ⚠. `workers_needed == 0` (rehydrated save) means "unknown" ⇒ no note, never a wrong one.
const OVERSTAFF_NOTE_FORMAT := " · only %d of %d working"
const OVERSTAFF_TOOLTIP := "Overstaffed — this source's yield is capped by the stock standing above its escapement floor; the extra workers produce nothing here. Reassign them to another source."
# Joins the yield readout and the overstaffing explanation into one row tooltip.
const TOOLTIP_LINE_SEPARATOR := "\n"
# UNDERSTAFFING (`LaborAssignment.wastedYield`): provisions the source OFFERED that the crew could not
# collect — the party is under-crewed for the kill (an animal too big to fully carry, or an
# over-abundant pulse) and food is left standing. Muted (INK_FAINT), the low-key mirror of the
# WARN-amber overstaff note. Below FOOD_FLOW_MIN ⇒ hidden (0 on a rehydrated save).
const WASTED_NOTE_FORMAT := " · %s wasted"
const WASTED_TOOLTIP := "Under-crewed — this source offered %s the party couldn't carry home. Add workers to collect it."
# Band food flow gate: a rate below this reads as absent rather than as a zero. A claim about the
# SIM — is this band moving food at all — and deliberately NOT the gate a rendered component goes
# through; see `has_component` and `COMPONENT_RENDER_MIN` for why the two are different numbers.
const FOOD_FLOW_MIN := 0.001
# HALF OF THE SMALLEST QUANTITY A READOUT CAN SHOW — `YIELD_DECIMALS` is 2, so anything under 0.005
# rounds to `0.00` and is indistinguishable on screen from the component's absence. It is therefore
# the floor `has_component` uses: a gate whose threshold is finer than its formatter's resolution
# admits values it then prints as zeros, which is the one thing that gate exists to prevent.
const COMPONENT_RENDER_MIN := 0.005
# An EXTRACTIVE rung's policy-button metric: the bare signed rate on the one-line button face, this
# wording in the tooltip so it reads as the ceiling it is (and the four rungs read as ASCENDING).
const POLICY_CAP_FORMAT := "up to %s/turn"

# ---- THE ACCOUNTS A TAKE PAYS (issue #337, arc #527) --------------------------------------------
# A harvest pays a VECTOR, not a food scalar: provisions AND fodder, per the source's own yield vector
# times the crew's take. THE ONE PRESENTATION RULE, applied everywhere below and by every caller:
# **render a component only when it is non-zero.** A hay meadow reads food and fodder, food leading;
# a staple patch reads food only. A `0` printed as a number for a component the source does not
# produce is the false-precision this whole arc exists to remove — it is not "more complete", it is
# wrong.
#
# **THE THIRD ACCOUNT WAS TRADE GOODS AND IT IS RETIRED** (arc #527). A source's non-food, non-feed
# product is MATERIALS — `hide`, `fibre`, `tobacco` — which are not one number and must never be
# summed into one. They have no per-turn scalar on any of these surfaces; the one surface that quotes
# them is the crop picker, per material (`HudFloraVocab.FLORA_CROP_MATERIAL_CLAUSE_FORMAT`).
const COMPONENT_SEPARATOR := " · "
# The joiner for the COMPACT (magnitude-only) pair. A plain space, not `·`, because the surfaces that
# use it — the work-zone filter chips — already spend their `·` separating a count from its total, and
# a second one would read as a third field rather than a second product.
const COMPACT_COMPONENT_SEPARATOR := " "
# The FODDER half of a rung's tooltip. No glyph in it: fodder has no mark of its own (`FoodIcons`
# spends no glyph on it), and borrowing another account's would say the wrong thing. It carries no
# "up to" (unlike `POLICY_CAP_FORMAT`), so the investment payoff tooltip reuses it.
const POLICY_CAP_FODDER_FORMAT := "%s fodder/turn"
# Its MATERIAL twin, for the same rung tooltip — `+0.22 hide/turn`. The material names itself here
# too, so this is `POLICY_CAP_FODDER_FORMAT` with the noun taken from the row rather than baked in.
const POLICY_CAP_MATERIAL_FORMAT := "%s %s/turn"

# ---- THE PICKER FACE'S PRODUCT LINE -------------------------------------------------------------
# THE PRODUCTS IN WORDS, for the policy picker's SECOND line: `0.96 food · 0.40 fodder`. The picker is
# the ONE surface that names its products rather than marking them, and the reason is that two glyph
# families were doing incompatible jobs side by side: the POLICY glyph (♻ ⬆ ⇊ 💀) says which RUNG, and
# a second mark beside it naming the PRODUCT left the eye unable to tell which axis it was reading.
# Line 1 names the rung (`HudFormat.policy_face`), so line 2 names the product.
# NO `+` SIGN, deliberately: every rung is a gain, so a sign on a picker face carries no information.
# It stays on the work rows and map labels, where a `+` genuinely contrasts against consumption.
# The render-only-when-non-zero rule above still governs — a hay meadow's rung reads `0.40 fodder`
# beside its food, and a staple patch's reads food alone.
const PICKER_FOOD_PRODUCT_FORMAT := "%s food"

# THE SECOND ACCOUNT (#426), plant-only. **The word is the ACCOUNT's, not the commodity's** — its
# neighbour on this line is `food`, the name of the account a yield routes to, so a commodity noun
# here would read as a different kind of thing rather than a second account. That is why this says
# `fodder` while the crop-basket rows two lines below say `hay`
# (`HudFloraVocab.FLORA_CROP_HAY_CLAUSE_FORMAT`): a row there names one PLANT and what that plant
# pays, and hay is what hay grass pays.
const PICKER_FODDER_PRODUCT_FORMAT := "%s fodder"

## **A MATERIAL NAMES ITSELF, AND THAT IS THE MARK IT WEARS** — `0.22 hide`, in the same shape its
## neighbour `0.40 fodder` wears, and the same shape the crop picker's basket rows already use
## (`HudFloraVocab.FLORA_CROP_MATERIAL_CLAUSE_FORMAT`). The material CATALOGUE ships no display name,
## so the id IS the display word, and a reader who has learned one of these readouts has learned all
## of them.
##
## There is deliberately NO generic glyph. `⇄` earned its job by being one mark for one scalar
## product; a material has a name, which is a better mark than an arrow saying only "not food", and
## with the trade axis retired there is no generic account left for a generic mark to stand for.
## **Do not add one.**
const PICKER_MATERIAL_PRODUCT_FORMAT := "%s %s"

## The picker face's product line for a source's yield VECTOR — `0.96 food`, `0.62 food · 0.40 fodder`
## (a tended patch carrying a hay crop), `1.80 fodder` (a hay-only meadow). Same food-leads order and
## same render-only-when-non-zero rule as `yield_components`, in words instead of glyphs and without
## the sign; when EVERY component is absent the food zero survives, because a rung whose ceiling is
## honestly empty is a fact worth reading.
##
## **The account order is the wire's, not a ranking** — provisions, then fodder — so a tile reads the
## same left-to-right whichever accounts it pays, and the eye can find an account by position rather
## than by re-reading the words.
##
## `zero_account` decides WHICH account's zero survives when every component is empty, and it is the
## §7.7 correctness fix rather than a formatting option — see `zero_account_of`.
static func picker_products(food: float, fodder: float = 0.0,
        zero_account: String = YIELD_ACCOUNT_FOOD, materials: Array = []) -> String:
    var parts: Array[String] = []
    for row in yield_rows(food, fodder, zero_account, {}, materials):
        var material_id := _row_material_id(row)
        if material_id != "":
            parts.append(PICKER_MATERIAL_PRODUCT_FORMAT % [
                format_magnitude(row[YIELD_ROW_VALUE]), material_id])
        elif String(row[YIELD_ROW_ACCOUNT]) == YIELD_ACCOUNT_FOOD:
            parts.append(PICKER_FOOD_PRODUCT_FORMAT % format_magnitude(row[YIELD_ROW_VALUE]))
        else:
            parts.append(PICKER_FODDER_PRODUCT_FORMAT % format_magnitude(row[YIELD_ROW_VALUE]))
    return COMPONENT_SEPARATOR.join(parts)

# ---- WHICH ACCOUNTS THIS SOURCE PAYS AT ALL (spec §7.7) -----------------------------------------
# The two per-turn accounts by name, plus the answer for a source that pays in NEITHER. They are the
# `zero_account` vocabulary: which component's zero is worth printing when the take is empty.
#
# **`YIELD_ACCOUNT_TRADE` WAS THE THIRD AND IS RETIRED** (arc #527). What a take pays beyond food and
# feed is MATERIALS, which the wire states per material and which no per-turn readout on this layer
# quotes — so there is no third account for a zero to belong to.
const YIELD_ACCOUNT_FOOD := "food"
const YIELD_ACCOUNT_FODDER := "fodder"
const YIELD_ACCOUNT_NONE := ""

## **WHICH ACCOUNT'S ZERO IS A FACT ABOUT THIS SOURCE**, read off its per-biomass yield VECTOR — the
## structural statement of what the source pays, independent of what stands on it today.
##
## The render-only-when-non-zero rule always kept ONE zero: a component that exists and paid nothing
## this turn is worth reading. But *which* component that is was hardcoded to food, and on a hay-only
## meadow that is a claim the wire contradicts — `0.00 food` there is not an empty reading but a false
## one, and it reached the screen exactly when the source was at or below the floor, i.e. when the
## player most needed to know what the source is FOR.
##
## A source with no positive rate in either account answers `YIELD_ACCOUNT_NONE`, and a caller renders
## no line at all: there is no account to be empty in. **An inedible quarry is now such a source** —
## it pays materials and neither of these two — so its readout goes quiet rather than quoting a
## retired trade rate. The wire states no per-herd material quote for it to state instead.
static func zero_account_of(src: Dictionary, prefix: String) -> String:
    if float(src.get(prefix + FORECAST_PROVISIONS_PER_BIOMASS_KEY, 0.0)) > 0.0:
        return YIELD_ACCOUNT_FOOD
    if float(src.get(prefix + FORECAST_FODDER_PER_BIOMASS_KEY, 0.0)) > 0.0:
        return YIELD_ACCOUNT_FODDER
    return YIELD_ACCOUNT_NONE

# The two keys of one `yield_rows` entry — which account it is, and what this take pays into it.
const YIELD_ROW_ACCOUNT := "account"
const YIELD_ROW_VALUE := "value"
## …and what it pays once the source is HOLDING at that floor and paying regrowth alone. Present only
## where it DIFFERS from the take, and only where the crew reaches the floor at all.
const YIELD_ROW_AFTER := "after"

## The UNIT each account is read in — the readout's `2.34  FOOD`. One table, so both accounts are
## named in the same grammar wherever a rate is stated as a number beside a unit rather than joined
## into a sentence (`yield_components`' job).
##
## **The readout states no DESTINATION**, because both accounts land in the same place — the working
## band's own stores. A `→ camp` tail once earned its width by marking trade as the odd account out,
## banked to the faction-wide stockpile; that account is retired and there is nothing left to contrast
## against, so identical tails would only cost the readout the room it wraps in.
##
## **THE `/TURN` IS HOISTED OUT OF THE UNIT AND INTO THE ROW'S HEADER** (`YIELD_ROW_HEADER*`). Stated
## per account it was a copy of one word per row on the sheet's widest line, and the row could not
## afford them once each account began stating a second reading. It is hoisted rather than DELETED
## because a preset's tooltip states bare `up to +0.60/turn` for the ROOM above that floor — a
## quantity takeable ONCE — so with nothing marking the difference the two kinds of number would read
## alike.
const YIELD_ACCOUNT_UNITS := {
    YIELD_ACCOUNT_FOOD: "food",
    YIELD_ACCOUNT_FODDER: "fodder",
}

## The row's header — the unit, said once, plus the KEY to the arrow when there is one to explain.
## `NOW → AFTER` is deliberately the crew buttons' own two words (`clear it now` / `hold it after`),
## which sit directly above it, so the mapping from a crew count to the rate it buys is both verbal
## and spatial. Without a second reading on the row there is no arrow to key, and the header states
## the unit alone.
const YIELD_ROW_HEADER := "per turn"
const YIELD_ROW_HEADER_WITH_AFTER := "per turn · now → after"
## **THE CAPTION A COMPOSED BUILD TAKES, AND IT IS THE WHOLE CAPTION.** While a rung is going up the
## crew carries the rung's `yield_fraction_while_building`, so the readings under this row are the
## DIPPED take — a number the sheet otherwise gives no way to tell apart from the undipped one it
## replaced. It never composes with the arrow's key, because a building sheet states **no arrow**:
## the floor walk is suppressed at the model while a build is composed (`labor-ui.md` → "A COMPOSED
## BUILD SUPPRESSES THE FLOOR WALK"), so the two facts cannot both be true on one row.
const YIELD_ROW_HEADER_WHILE_BUILDING := "per turn · while building"

## **THE ONE RESOLUTION OF THE ROW'S CAPTION**, over the two facts that can key it: is a build
## dipping these readings, and do they carry a holding rate to arrow toward. THREE states, because
## the pair is not independent — a composed build drops the `after` readings, so `while_building`
## always arrives with `has_after` false and the fourth combination is unreachable. It is resolved
## in one place: `build_yields_row` calls it and no caller composes a caption of its own, because
## two sites deciding separately is how the `while building` override once came to swallow the
## arrow's key. A caller with no per-turn rate AT ALL (the raid's whole-trip payload) supplies its
## own `header` and never reaches here.
static func yield_row_header(while_building: bool, has_after: bool) -> String:
    if while_building:
        return YIELD_ROW_HEADER_WHILE_BUILDING
    return YIELD_ROW_HEADER_WITH_AFTER if has_after else YIELD_ROW_HEADER
## The transition inside ONE account's reading: `2.26 → 0.42`. The glyph is the row's second job for
## an arrow — the retired routing suffix (`→ CAMP`) was the first — but the two never coexisted, and
## this one is keyed by the header rather than left to be guessed. The format is written in terms of
## the GLYPH so the mark a harness looks for and the mark the row draws are one string.
const YIELD_AFTER_GLYPH := "→"
const YIELD_AFTER_FORMAT := "%s " + YIELD_AFTER_GLYPH + " %s"

## **WHICH ACCOUNTS A TAKE PAYS, AS ROWS** — the STRUCTURAL half of the render-only-when-non-zero rule,
## and the one definition of it. `yield_components` (a joined sentence), `picker_products` (a rung's
## product line) and `extractive_take_pair` (a rung's tooltip ceiling) all differ only in how they
## SPELL a component; which components exist at all is this function, so a surface that needs the
## numbers rather than the sentence — the compose sheet's readout, whose yields row sets a 15px number
## beside a 10px unit and therefore cannot be given a pre-joined string — asks here.
##
## Food leads, then fodder: the wire's order, not a ranking, so a source reads the same left-to-right
## whichever accounts it pays.
##
## When EVERY component is empty exactly ONE zero survives — `zero_account`'s, the account the source
## STRUCTURALLY pays (`zero_account_of`) — because a component that exists and paid nothing this turn
## is worth reading while `0.00 food` on a hay-only meadow is not empty but false. A source that pays
## into no account at all (`YIELD_ACCOUNT_NONE`) answers an EMPTY array, and its caller renders no line.
## **`after` IS THE SECOND READING EACH ACCOUNT CARRIES**, keyed by account — what this crew takes once
## the source is sitting at its floor and only regrowth is on offer. It rides the SAME row rather than
## a second one because the accounts are one biomass flow through a fixed per-biomass vector, so a
## second row of numbers would carry ONE new fact in every slot; and because the comparison the player
## is making is per account, so the two numbers should touch.
##
## It is attached only where it DIFFERS from the take — a crew at or below the *hold it after* count
## takes the same amount every turn, and an arrow from a number to itself is noise. Whether the crew
## reaches the floor AT ALL is the caller's test: a crew that settles short never reaches the holding
## state, so it passes no `after` and the row reads exactly as it did before this existed.
##
## **"DIFFERS" IS A CLAIM ABOUT WHAT IS SHOWN, SO IT IS ASKED OF THE FORMATTED STRINGS.** The test was
## `is_equal_approx` on the raw floats, which is a claim about the model — and the reading renders
## through `format_magnitude` at `YIELD_DECIMALS`, so any pair differing by less than the display's own
## resolution drew the arrow between two IDENTICAL numbers (`0.26 → 0.26 FOOD`, reported from play,
## beside a second account correctly reading `0.90 → 0.87`). The same reasoning `COMPONENT_RENDER_MIN`
## already records one function along: a gate finer than its formatter's resolution admits exactly what
## it exists to stop.
## **MATERIALS ARE ROWS OF THIS SAME VECTOR** (arc #527 follow-up), appended after the two scalar
## accounts and under the identical non-zero gate — so an inedible quarry reads `0.22 HIDE` where it
## used to read nothing at all, and every surface that iterates this function gains the account for
## free rather than growing a second code path to spell it.
##
## **A MATERIAL ROW'S ACCOUNT IS ITS OWN ID**, not a shared `"material"` tag: the material names
## itself, `YIELD_ACCOUNT_UNITS` has no entry for it, and the unit therefore falls back to the id,
## which is the display word. (The catalogue's ids are disjoint from `food` / `fodder`; a material
## named for an account would be a content collision, not a case to branch on.)
##
## **THEY ALSO ANSWER THE ZERO QUESTION.** A source paying a material pays SOMETHING, so no zero
## survives beside it — which is what stops a wolf reading `0.00 food · 0.22 hide` and reasserting
## the false precision this rule exists to remove. `after` is keyed by account and so reaches a
## material row too, though nothing quotes a material's holding rate today.
static func yield_rows(food: float, fodder: float = 0.0,
        zero_account: String = YIELD_ACCOUNT_FOOD, after: Dictionary = {},
        materials: Array = []) -> Array[Dictionary]:
    var rows: Array[Dictionary] = []
    var pairs: Array = [[YIELD_ACCOUNT_FOOD, food], [YIELD_ACCOUNT_FODDER, fodder]]
    for row_variant in materials:
        if not (row_variant is Dictionary):
            continue
        var material: Dictionary = row_variant
        pairs.append([
            String(material.get(MATERIAL_PAYOFF_ID_KEY, "")),
            float(material.get(MATERIAL_PAYOFF_AMOUNT_KEY, 0.0)),
        ])
    var empty := true
    for pair in pairs:
        if has_component(float(pair[1])):
            empty = false
            break
    for pair in pairs:
        var account := String(pair[0])
        var value := float(pair[1])
        if account == "":
            continue
        if has_component(value) or (empty and zero_account == account):
            var row := {YIELD_ROW_ACCOUNT: account, YIELD_ROW_VALUE: value}
            if after.has(account) and format_magnitude(float(after[account])) \
                    != format_magnitude(value):
                row[YIELD_ROW_AFTER] = float(after[account])
            rows.append(row)
    return rows

## The spelling of ONE row of that vector, given the row's account. The four joiners below differ
## only in which of these tables they reach for, so a new account is spelled once per REGISTER rather
## than once per surface.
static func _row_material_id(row: Dictionary) -> String:
    var account := String(row[YIELD_ROW_ACCOUNT])
    if account == YIELD_ACCOUNT_FOOD or account == YIELD_ACCOUNT_FODDER:
        return ""
    return account

## **ONE TAKE, COUNTED ON THE PROVISIONS AXIS AND VALUED IN EVERY ACCOUNT** — the client mirror of the
## sim's `YieldPair::rescaled_to` (`core_sim/src/fauna_config.rs`), and the companion `yield_rows`
## needs on the animal web.
##
## A quantised take must be counted on ONE axis, because the quantiser divides by a per-animal quantum.
## **But that constraint governs the COUNT, not the CREDIT.** A ratio is unit-free, so the same count
## values in every account the species pays.
##
## **THE AXIS USED TO BE A CHOICE AND IS NOT ANY MORE** (arc #527). It was provisions-or-trade, because
## an inedible quarry's food quantum is honestly `0` and a food-only derivation divides by zero. With
## the trade axis retired there is no second scalar to count on, so provisions is the axis outright —
## and an inedible quarry now answers all-zero here, which `yield_rows` renders as no row at all rather
## than as a false `0.00 food`. What such a species is really worth is materials, which no per-turn
## readout on this layer states.
##
## The reference mix is the source's own PER-BIOMASS vector — the same structural witness
## `zero_account_of` reads. Every term a take is composed from is that vector times one biomass (the
## per-worker rates, the ceilings, and the per-animal quanta, which are `body_mass ×
## <account>PerBiomass`), so the proportion is identical whichever of them is used, and this one is
## the only one still present on a source standing at its floor. `value` comes back BIT-IDENTICAL on
## the provisions account: no divide-then-multiply round trip on the component actually computed.
##
## A source with no positive provisions rate pays nothing anywhere — the degenerate case the sim's
## `rescaled_to` answers `ZERO` for — so it answers zeros rather than dividing by it.
static func rescaled_accounts(src: Dictionary, prefix: String, value: float) -> Dictionary:
    var food_rate := float(src.get(prefix + FORECAST_PROVISIONS_PER_BIOMASS_KEY, 0.0))
    var fodder_rate := float(src.get(prefix + FORECAST_FODDER_PER_BIOMASS_KEY, 0.0))
    if food_rate <= 0.0:
        return {YIELD_ACCOUNT_FOOD: 0.0, YIELD_ACCOUNT_FODDER: 0.0}
    var share := value / food_rate
    return {
        YIELD_ACCOUNT_FOOD: value,
        # No animal pays fodder, so a herd's second account rescales to a structural zero and renders
        # no row — the same answer the account had before this existed.
        YIELD_ACCOUNT_FODDER: fodder_rate * share,
    }

# PRE-COMMIT YIELD FORECAST. The overstaffing note above is POST-HOC — it tells you a turn later that
# workers were wasted. The forecast is the same truth shown WHILE COMPOSING: the sim exports, for the
# forage patch and the herd alike, a `per_worker_yield` plus one take ceiling per policy (the patch as
# scalar fields, the herd as its `hunt_policy_ceilings` list) — all food/turn at the source's CURRENT
# biomass and at output_multiplier 1.0:
#     expected(workers, policy) = min(workers × per_worker_yield, ceiling[policy]) × band output
#     max_useful_workers(policy) = ceil(ceiling[policy] / per_worker_yield)
# The ceilings are already biomass-clamped, so that `min` IS the take. The worker stepper caps at
# max-useful (the `+` goes dead there, explained by MAX_USEFUL_NOTE_FORMAT) so over-assignment is
# impossible up front; the post-hoc note still covers a source whose biomass FELL after staffing.
# max_useful is independent of the band's output multiplier — it scales both terms linearly.
const FORECAST_PER_WORKER_KEY := "per_worker_yield"
# One animal's worth of FOOD — the quantum every whole-animal derivation divides by (the kill rhythm,
# the carry-aware delivered take, the averaging window, the whole-animal worker cap). Herd-only.
#
# **ITS TRADE TWIN IS RETIRED** (`trade_per_animal`, arc #527). It existed so an inedible quarry —
# whose food quantum is honestly `0` — had a second quantum to count on; with the trade axis gone
# there is no second scalar, and such a species simply states no per-turn rate at all.
const FORECAST_FOOD_PER_ANIMAL_KEY := "food_per_animal"
# ---- THE TERMS THE CLIENT COMPOSES A CEILING FROM (docs/plan_harvest_floor.md §5) ---------------
# The standing stock, the capacity it is measured against, and what ONE UNIT of that stock is worth
# in each account. Both webs publish the same four keys, which is what lets ONE composition serve
# them: `ceiling(floor, account) = max(0, B − floor·K) × <account>_per_biomass`.
# (`trade_per_biomass` was the third account and went with the trade axis, arc #527.)
const FORECAST_BIOMASS_KEY := "biomass"
const FORECAST_CAPACITY_KEY := "carrying_capacity"
const FORECAST_PROVISIONS_PER_BIOMASS_KEY := "provisions_per_biomass"
const FORECAST_FODDER_PER_BIOMASS_KEY := "fodder_per_biomass"
# **AND WHAT THAT UNIT OF STOCK IS MADE OF, PER MATERIAL** — the vector that replaced the retired
# `trade_per_biomass` scalar (arc #527 follow-up). It composes at any floor by the SAME rule the two
# scalars do, `max(0, B − floor·K) × rate`, which is why it lives here beside them rather than in a
# branch of its own. Herd-side today; a patch quotes its materials per RUNG on the crop picker
# instead, because a plant's material payoff is a property of the rung you build, not of the stock.
const FORECAST_MATERIAL_PER_BIOMASS_KEY := "material_per_biomass"
# The per-worker twin — what ONE HUNTER brings home per turn, per material. The material sibling of
# `per_worker_yield`, so a band preview clamps `min(workers × rate, ceiling)` per material exactly as
# it does for food, and the build dip rides it exactly as it rides `per_worker`.
const FORECAST_PER_WORKER_MATERIAL_KEY := "per_worker_material"
# **THE RESOLVED YIELD, ON A LABOR ASSIGNMENT** — what this source actually credited to the band's
# `MaterialStore` this turn. Read through `material_rows_of`, never as a forecast: the sim seeds it
# EMPTY pre-commit by design (see there).
const ASSIGNMENT_MATERIAL_YIELD_KEY := "material_yield"
# **THE CREW'S THROUGHPUT IN BIOMASS** — what ONE worker moves before any account conversion, and the
# term everything on the crew side of the panel divides by. Published identically by both webs, which
# is what lets the two worker targets be one expression. It folds the tile's seasonal weight in on the
# plant web (so it is honestly `0` in a dead season) and has none on the animal web.
const FORECAST_PER_WORKER_BIOMASS_KEY := "per_worker_biomass"
# **THE SAMPLED GROWTH CURVE** — the source's own per-turn regrowth in BIOMASS at evenly spaced
# fractions of `K`. See the `## THE SAMPLED GROWTH CURVE` block below for why it is sampled rather
# than composed, and what a reader must never do to it.
const FORECAST_REGROWTH_SAMPLES_KEY := "regrowth_samples"
# One animal's BIOMASS on a whole-animal source (0 on a patch). It is the quantum the *hold* crew
# target rounds up to, and it is in the same units as the curve and the throughput above — unlike
# `food_per_animal`, which is that quantum already converted into provisions.
const FORECAST_BODY_MASS_KEY := "body_mass"
# **THE ENGAGEMENT THROUGHPUT — the THIRD bound on a hunt take** (`docs/plan_hunt_through_combat.md`
# §2, `HerdTelemetryState.engageRate`): how many animals ONE hunter can bring into contact per turn,
# beside the stock standing above the floor and the party's carry. A `min()` composed from those two
# alone quotes a take the sim will never pay — measured at ~30× on a Wild Fowl herd with one hunter,
# whose 40 biomass of carry is 307 birds against 10 of reach.
const FORECAST_ENGAGE_RATE_KEY := "engage_rate"
# **`<= 0` MEANS "NO ENGAGEMENT STAGE", never "reaches nothing".** It is the wire's finite stand-in for
# the sim's `f32::INFINITY` — a PEN (a penned animal is not stalked), a species the roster cannot
# resolve, and the whole PLANT web, which never publishes the field at all. Read it as UNBOUNDED and
# drop the term: that is what leaves forage and corrals byte-identical to before this arm existed.
const NO_ENGAGEMENT_STAGE := 0.0
# The value the dropped term contributes to a `min()` / the crew `max()` — an unbounded reach cannot
# be the binding arm, and `INF` says so without a branch at every call site.
const ENGAGEMENT_UNBOUNDED := INF
# **A PARTY THAT EXISTS REACHES AT LEAST ONE ANIMAL** — the sim's `fauna::animals_engaged` `max(1.0)`.
# A fractional engagement means a small band cannot corner the quarry EFFICIENTLY, not that it cannot
# walk up to it: three hunters do reach a mammoth and then fail at the FIGHT, which is where the gate
# lives. Flooring to zero would put a headcount threshold in front of the attack-vs-defense one.
const ENGAGED_AT_LEAST := 1.0
# **THE RETREAT, AS A TERM — `1 − wariness`** (`HerdTelemetryState.stayFraction`): what fraction of the
# animals a party REACHES actually stays to be fought. It is the stage between the engagement and the
# fight, and it prices BOTH the take and the CREW: a party that keeps one animal in four brings down a
# quarter as much per hand, so it needs four times the hands to draw the same stock down. It is applied
# as its own term rather than folded into `engage_rate`, because the two are separately observable —
# a kit's `dispersion` moves the retreat alone — and the fold makes Big-game and Trapping quote one
# identical hunt.
const FORECAST_STAY_FRACTION_KEY := "stay_fraction"
# **`1` MEANS NOTHING BREAKS OFF**, which is the honest reading for a source with no retreat stage — a
# pen, the whole plant web, and a species the roster cannot resolve — and is the wire's own default, so
# an absent field and a present `1.0` are one answer rather than two.
const STAY_FRACTION_NONE_BREAKS_OFF := 1.0
# **`0` MEANS NOTHING EVER STANDS** — the other end of the same fraction, and the one value that makes
# the crew quotient degenerate: no number of hands lands an animal that always runs, so
# `engage_workers` answers "no crew" there rather than dividing by nothing.
const STAY_FRACTION_ALL_BREAK_OFF := 0.0
# **THE PHASE, AND WHERE ITS BOUNDARIES ARE — both on the wire now.** `ecology_phase` is the source's
# CURRENT band as a word (Thriving / Stressed / Collapsing); the two fractions below are the cut points
# `classify_ecology_phase` used to reach it, **in the same units the floor is in** (fractions of `K`).
# That is what makes them drawable: a floor and a phase band are the same kind of object, so the chart
# can lay the bands behind the floor line on ONE y-axis instead of tinting a single data point.
#
# They are read PER SOURCE and never cached as a pair of client constants — a herd's cuts come from the
# RUNG it stands on (wild / pastoral / pen each carry their own ecology block), so one global pair would
# be right for a wild herd and wrong for a penned one. Copying the sim's `collapse_fraction` /
# `stressed_fraction` into GDScript is the same mistake the sampled growth curve exists to prevent.
const FORECAST_ECOLOGY_PHASE_KEY := "ecology_phase"
const FORECAST_COLLAPSE_FRACTION_KEY := "collapse_fraction"
const FORECAST_STRESSED_FRACTION_KEY := "stressed_fraction"
# The three phase WORDS the sim publishes (`EcologyPhase::as_str`), so a zone this layer derives and a
# phase the source itself reports are tinted through ONE vocabulary (`DetailFormat.ecology_tier_color`).
const ECOLOGY_PHASE_COLLAPSING := "collapsing"
const ECOLOGY_PHASE_STRESSED := "stressed"
const ECOLOGY_PHASE_THRIVING := "thriving"
# **A RUNG-3 MANAGED SOURCE HAS NO ESCAPEMENT ROOM AND IGNORES THE FLOOR ENTIRELY** (sim
# `SourceYieldForecast::managed`): a Field and a Pen are YOURS — you control their reproduction, so
# there is no wild stock to stop short of and the axis honestly collapses onto the one managed
# production they hand over. The wire still carries their raw `biomass`/`carrying_capacity`/rates
# (they are facts about the herd or the crop), so composing an escapement ceiling on one is silently
# wrong; the managed production is the rung's own payoff field, which for a BUILT rung is the live
# number the sim pays. Rung 2 (a Tended Patch, a pastoral herd) is still a wild stand being drawn
# down, so it takes the composition like rung 1.
const FORECAST_MANAGED_FLAG_KEYS := {
    SOURCE_KIND_FORAGE: "is_field",
    SOURCE_KIND_HERD: "corralled",
}
const FORECAST_MANAGED_IMPROVEMENTS := {
    SOURCE_KIND_FORAGE: IMPROVEMENT_SOW,
    SOURCE_KIND_HERD: IMPROVEMENT_CORRAL,
}
# The PAYOFF an IMPROVEMENT buys — the food/turn the source pays once the rung is built (one worker
# suffices). Keyed by improvement, and read ONLY as a payoff lookup: it is no longer how any surface
# asks "is this a build?" (issue #442 — that question is now answered by the assignment's own
# `improvement` field, or by which control the player is looking at).
#
# **THERE IS NO CEILING-KEY TABLE ANY MORE, on either web** (#426) and no dip ROW either (#442). The
# during-build dip is composed — `stance ceiling × <rung>_build_fraction` — in exactly one place,
# `improvement_forecast`, and paired with the payoff below.
const FORECAST_PAYOFF_KEYS := {
    "cultivate": "tended_yield",
    "corral": "corral_yield",
    "sow": "field_yield",
    "tame": "pastoral_yield",
}
# **THE TRADE HALF OF THAT PAYOFF IS RETIRED** (`FORECAST_PAYOFF_TRADE_KEYS`, arc #527) along with
# `tended_trade` / `field_trade` / `pastoral_trade` / `corral_trade` on the wire. A prepared source's
# non-food product is MATERIALS, which no rung face can state as one per-turn number — and the sim
# publishes no per-rung material quote for a HERD at all. The crop picker states the plant web's, per
# material, from the composition entry's own `sow_material_payoff` / `cultivate_material_payoff`.
#
# The FODDER half survives. **PLANT RUNGS ONLY, and that asymmetry is structural rather than pending work:**
# fodder is feed grown for penned animals, and no animal pays it (`fauna_config::YieldAccounts` fills
# a structural zero there), so `tame` and `corral` have no twin here and never will.
const FORECAST_PAYOFF_FODDER_KEYS := {
    "cultivate": "tended_fodder",
    "sow": "field_fodder",
}
# The RUNNING COST the payoff is paid against. Only the pen has one: a corralled herd is a managed
# population that eats from the keeper's larder every turn (`pen_upkeep`), and `corral_yield` is the
# GROSS take with that feed NOT deducted — so advertising the payoff bare would promise a number the
# player never banks. A tended patch has no running cost, hence no entry.
#
# **This asymmetry is deliberate and permanent** (spec §4): the Corral done-state label carries the
# pen's per-turn upkeep and the Tame one does not, because a penned herd cannot graze and a pastoral
# one still can. Do not make the two webs match here.
const FORECAST_FEED_KEYS := {
    "corral": "pen_upkeep",
}
# THE DURING-BUILD DIP, as a FRACTION of the selected stance's ceiling — one wire field per
# improvement (issue #442). The two rungs of a web keep two numbers because their dials are
# independently tunable; folding them onto one would pass every forecast==actual test by today's
# coincidence and lie the moment either is retuned.
const FORECAST_BUILD_FRACTION_KEYS := {
    IMPROVEMENT_CULTIVATE: "cultivate_build_fraction",
    IMPROVEMENT_SOW: "sow_build_fraction",
    IMPROVEMENT_TAME: "tame_build_fraction",
    IMPROVEMENT_CORRAL: "corral_build_fraction",
}
# The identity dip — what a ceiling is multiplied by when NO build is under way. The sim's
# `NO_BUILD_UNDERWAY_DIP`, spelled once here so "not building" is a value rather than a branch.
const NO_BUILD_DIP := 1.0
# THE PLANT BUILD CREW each improvement demands — the plant twin of a managed herd's `herders_needed`,
# and the FLOOR under the worker cap (`plant_crew_floor`). Only the plant rungs declare one: the animal
# rungs take their crew from the herd's own size, which is why there is no `IMPROVEMENT_TAME` row here
# rather than a zero one — a missing key is the honest "this web answers elsewhere".
const FORECAST_CREW_KEYS := {
    IMPROVEMENT_CULTIVATE: "cultivate_crew_needed",
    IMPROVEMENT_SOW: "sow_crew_needed",
}
# The per-source BUILD METER each improvement fills, 0..1. The one place that mapping is written down
# (`RungGates.rung_in_progress` reads it, so the compose sheet, the work board and the map badge can
# never quote different meters for one verb).
const FORECAST_BUILD_METER_KEYS := {
    IMPROVEMENT_CULTIVATE: "cultivation_progress",
    IMPROVEMENT_SOW: "field_progress",
    IMPROVEMENT_TAME: "domestication",
    IMPROVEMENT_CORRAL: "corral_progress",
}
# The wire flag/meter that says an improvement's rung is ALREADY BUILT — what turns the control's
# Running state into its Done state. A bool for the plant rungs (never infer a rung from its float);
# the animal rungs read `corralled` and a full `domestication` meter, since taming has no bool of its
# own. `DONE_FLAG_KEYS` holds the bools; `IMPROVEMENT_TAME` is handled against
# `DOMESTICATION_COMPLETE` by `improvement_is_done`, which is the one definition.
const FORECAST_DONE_FLAG_KEYS := {
    IMPROVEMENT_CULTIVATE: "is_cultivated",
    IMPROVEMENT_SOW: "is_field",
    IMPROVEMENT_CORRAL: "corralled",
}
## **A HIGHER RUNG RETIRES THE ONE BELOW IT, and on the plant web that has to be said out loud** —
## because `Sow` skips rung 2. A Field sown from wild ground carries `cultivation_progress == 0`
## FOREVER (the sim: *"`Sow` needs no prior patch, so a Field may stand on ground that was never
## tended"*), so `is_cultivated` is honestly false on a finished Field. Reading the bare flag made a
## completed Field offer `Cultivate this patch` — a live checkbox for a build the sim treats as
## already built. Reported from play.
##
## **The sim already answers this correctly and this mirrors it**: `forage_rung_already_built` matches
## `Improvement::Cultivate => patch.is_managed()`, whose own docstring says *"a Field is above rung
## 2"* and records that a `cultivate` sent to a wild-sown Field "stalled forever, silently".
##
## **DO NOT reuse `source_is_managed` here, despite the sim's predicate being named `is_managed`.**
## The word means different things on the two sides: the client's is rung-3 ONLY — the "the sim never
## draws this source down" branch, which a Tended Patch deliberately fails — while the sim's is
## `is_field() || is_cultivated()`. It would be right by accident on this line and wrong wherever
## else it is read.
##
## The animal web needs no entry: `Corral` demands a herd already tamed, so its rung 2 cannot be
## skipped, which is why `hunt_rung_already_built` carries no cross-rung term either.
const FORECAST_RETIRED_BY_HIGHER_RUNG := {
    IMPROVEMENT_CULTIVATE: [IMPROVEMENT_SOW],
}
# Below this a component's rate is zero — nothing to divide by. NOT the same question as "did the wire
# carry a forecast", which `known` now answers separately (see `forecast_inputs`).
const FORECAST_MIN_PER_WORKER := 0.0001
# Sentinel for "no forecast data" → the stepper is not forecast-capped.
const MAX_USEFUL_UNBOUNDED := -1
# **A source the wire DOES describe, which pays nothing in any account** — a dead-season patch. Its cap
# is 1: one worker is enough to prove the source is barren, and more is pure waste. It is deliberately
# NOT 0 (that would gate the stepper dead and take the choice away — the sim allows the assignment, and
# "a loss the player stays free to choose" is this codebase's stated stance) and deliberately NOT
# UNBOUNDED, which is what issue #426 was: an unknown forecast REMOVES the ceiling in both cap twins,
# so the one guard against parking a crew on a worthless source was switched off by a worthless source.
const MAX_USEFUL_BARREN := 1
# The patch's per-worker term is FOOD-ONLY (`perWorkerYield`), and the other two accounts are composed
# from `perWorkerBiomass` — the take is component-wise `min(collection, ceiling)` over ONE biomass
# quantity through the SAME rates, so one biomass-space throughput answers all three. See
# `per_worker_biomass`, and the `PER_WORKER_BIOMASS_UNKNOWN` sentinel it retired.
# A whole-animal hunt's kill-credit bank accumulates the smoothed take, then discharges a WHOLE animal
# when it holds a full body. Worst case the turn's rate lands with just under one body already banked,
# so one extra whole animal drops that turn beyond floor(rate / body) — this is that +1.
const HUNT_PEAK_DROP_BANK_BONUS := 1
# A tended patch / corralled herd collapses max-useful to exactly 1, so this note has to read
# "max 1 worker" — pluralize the noun rather than shipping "max 1 workers".
const MAX_USEFUL_NOTE_FORMAT := "max %d %s useful here — more would be idle"
const MAX_USEFUL_NOUN_ONE := "worker"
const MAX_USEFUL_NOUN_MANY := "workers"
# The CONFIRMED-row twin of MAX_USEFUL_NOTE_FORMAT: a worked source's `+` explaining why it is dead
# (see `source_worker_cap_state`). Worded from the row's point of view ("fully staffed") rather than
# the compose stepper's ("max N useful here"), because the player is looking at a running assignment.
const MAX_USEFUL_CAPPED_TOOLTIP := "Fully staffed — this source can use at most %d %s; more would idle here."
# The OTHER binding cap: idle workers run out BELOW the usefulness ceiling, so the `+` caps at labor,
# not usefulness. Named in the "N of M" spirit (N = the labor cap you're at, M = the useful ceiling),
# so a capped `+` reads as "fixable by reassigning labor" rather than as a silent bug.
const LABOR_BOUND_NOTE_FORMAT := "%d of %d useful — free up idle workers to send more"

# **THE RAID'S ROW IS THE ONE ANSWER THE SIM STILL COMPUTES FOR US, and for the opposite reason to the
# retired ceiling lists.** A resident band's ceiling has a closed form the client can evaluate at any
# floor; a raid's trip length does not — it is a bounded forward simulation of "grab the standing
# surplus, come home", so there is no expression to hand over. The client therefore does ZERO
# arithmetic over a raid row: a re-derived `carryCap / rate` closed form is wrong, and wrong by a lot
# (on a FULL Rabbit Warren above the peak only a LONE hunter fills at all). Ask, and read the answer.
#
# **IT IS ASKED FOR, NOT BROADCAST.** The forecast is a request/response on the command socket
# (`ForecastQuery`), answered for the exact band, kit, party and floor the sheet composed — so the row
# below is one answer rather than a cell of a sampled table, and there is no rung to round to. A ROW
# still carries `floor` / `party_workers`, and both are still read off the row.
# Sentinel for "the snapshot doesn't carry the levers/ceiling this forecast needs" (older server).
# A real take rate / ceiling is always ≥ 0, so a negative reads unambiguously as absent → the caller
# renders NO forecast line rather than a misleading zero.
const HUNT_RATE_UNAVAILABLE := -1.0

# A hunting expedition is a GREEDY RAID: it grabs the herd's standing surplus above the policy's floor
# in a burst and comes home. So the headline is the PAYLOAD — the whole animals the raid delivers over
# the turns it takes: "delivers ≈5 Wild Boar over ≈7 turns". `animals` is `HuntTripEstimate.animalsTaken`
# (the sim's forward-simulated answer), `turns` is `turnsToFill` — now "turns until the raid comes home",
# NOT "turns to fill the pack" (a big party leaves a partial pack once it strips the surplus).
const HUNT_FORECAST_DELIVERS_FORMAT := "delivers ≈%d %s over ≈%d turns"
# `turnsToFill == 0` no longer means "won't fill" — under the raid model it means the raid ran the whole
# forecast horizon still delivering (a slow breeder a big party can neither fill nor exhaust). The client
# now HAS the horizon (`expeditionForecastHorizonTurns`), so it quotes a FLOOR on the trip instead of the
# hedge it used to word this as: `over more than 78 turns`, in the SAME span the bounded line's `over ≈36
# turns` is in, so the two are comparable at a glance.
const HUNT_FORECAST_LONG_RAID_FORMAT := "delivers ≈%d %s over more than %d turns"
# The band carries no horizon at all (a fixture that predates the lever) — there is no floor to quote, so
# the line falls back to the hedge. **Quoting `0` here is the one outcome worse than "many".**
const HUNT_FORECAST_LONG_RAID_NO_HORIZON_FORMAT := "delivers ≈%d %s over many turns"
# **THE WIRE'S "THE RAID HAD NOT FINISHED WHEN THE PROJECTION RAN OUT", AND IT NOW MEANS `horizon` AND
# NOTHING ELSE.** `HuntTripForecast::turns_to_fill` is `Option<u32>`, `None` rendered as `0` here, and
# the sim reserves `None` for [`HuntTripBound::Horizon`] alone: a raid that ends by driving the herd
# extinct reports the turn it ended on like any other, because the live arm's lost-herd guard turns the
# party for home that same turn.
#
# **THAT PAIRING IS THE WHOLE OF FIX #2, and it is the ONE test for the three "many turns" surfaces.**
# A floor-`0` (`Take everything`) raid ends by emptying the range, so it used to publish this sentinel
# and read on three surfaces at once as a trip that never completes — `delivers ≈12 Red Deer over many
# turns`, `Away many turns — still delivering at the end of the forecast`, `Send Anyway (long raid)` —
# for the one mission whose whole purpose is to finish. Reading the sentinel is therefore reading
# "horizon", not "no answer": a `herd_lost` row carries a real turn count and takes the bounded branch,
# where its `TRIP_BOUND_CLAUSES` line says the range is empty by the time the party is home. **Never
# add a second `<= 0` test beside this one**, and never let a bound key reach the long branch.
const RAID_TURNS_UNBOUNDED := 0

## Did the sim's projection run its whole length with this raid still going? The ONE reading of
## `RAID_TURNS_UNBOUNDED`, so the one-line form, the trip verdict and the Send button cannot answer it
## three ways — and so a raid that COMPLETES by emptying the range (a real `turns_to_fill` beside a
## `herd_lost` bound) can never take a "many turns" branch on any of them.
static func raid_is_unbounded(hunt_turns: int) -> bool:
    return hunt_turns <= RAID_TURNS_UNBOUNDED

# **THE SCALE EVERY "NEVER COMPLETED" SENTINEL ON THIS WIRE IS RELATIVE TO** — how many turns the sim's
# raid projection runs before giving up (`expedition_config.hunt.forecast_horizon_turns`), echoed onto
# EVERY cohort in the `expeditionViabilityWarnTurns` idiom. ONE lever serves both raid tables (the sim's
# `denial_projection_at` and `hunt_trip_forecast_seeded` read the same field), so `turnsToFill == 0`,
# `turnsToCollapse{,Low,High} == 0` and `expeditionTripBound == "horizon"` are all measured against this
# one number and there is nothing here to pick wrongly between.
#
# **IT IS NOT A TRIP LENGTH, AND QUOTING IT AS ONE IS WORSE THAN THE HEDGE IT REPLACES.** It bounds the
# HUNTING alone — `turnsToFill` excludes travel — while the round trip out and back is a separate,
# already-known term, so the floor on the WHOLE trip is `horizon + round_trip_travel_turns`: *"Away more
# than 78 turns"*, never *"more than 60"*. A number wrong in the REASSURING direction sends the player
# out on a raid they would not have taken.
const COHORT_FORECAST_HORIZON_KEY := "expedition_forecast_horizon_turns"
# The lever is absent (a fixture that predates it). A real horizon is always positive — the sim pins that
# on the exported snapshot — so `0` reads unambiguously as "no bound to quote", and every surface falls
# back to its hedge rather than printing "more than 0 turns".
const FORECAST_HORIZON_UNKNOWN := 0

## **HOW FAR THE SIM'S RAID PROJECTION RAN, off whichever cohort the caller has in hand** — the BAND on a
## launch sheet, the launched PARTY in the in-flight drawer. It is a global lever echoed on every cohort,
## so any cohort answers it; `FORECAST_HORIZON_UNKNOWN` when the dict carries none.
static func forecast_horizon_turns(cohort: Dictionary) -> int:
    return maxi(FORECAST_HORIZON_UNKNOWN,
        int(cohort.get(COHORT_FORECAST_HORIZON_KEY, FORECAST_HORIZON_UNKNOWN)))

## Does this unbounded forecast carry a FLOOR the copy can quote? `false` = no horizon on the wire, so
## every surface says "many turns" rather than inventing one. The ONE reading of the floor keys below,
## for the reason `raid_is_unbounded` is the one reading of the sentinel: the line, the verdict and the
## Send button must not answer it three ways.
static func raid_floor_is_known(forecast: Dictionary) -> bool:
    return int(forecast.get(RAID_TURNS_FLOOR_KEY, FORECAST_HORIZON_UNKNOWN)) > FORECAST_HORIZON_UNKNOWN

# The floor on the WHOLE trip (`horizon + round-trip travel`) and on its hunting half (`horizon` alone).
# They are separate keys rather than a re-use of `turns` / `hunt_turns` because those two are EXACT on the
# bounded branch, and a consumer reading a floor as an exact figure is the failure this arc is fixing.
const RAID_TURNS_FLOOR_KEY := "turns_floor"
const RAID_HUNT_TURNS_FLOOR_KEY := "hunt_turns_floor"
# The FOOD the delivered animals are worth, appended so the party-size tradeoff reads BOTH ways: a
# bigger party takes more animals AND more food.
const HUNT_FORECAST_FOOD_FORMAT := " · ~%d food"
# **ITS TRADE TWIN IS RETIRED** (arc #527): the wire no longer states a raid's non-food payload as a
# scalar, and materials have no per-raid figure to put in its place.
# A finite raid past the band's `expedition_viability_warn_turns` — it still delivers, just slowly. A
# real tradeoff (told, then trusted), so the line stays WARN-amber and the button stays enabled.
const HUNT_FORECAST_SLOW_SUFFIX := " — a slow raid"
# Travel is NOT in `turnsToFill` — that now counts HUNTING turns only (once the party is in reach). The
# round trip out to the herd and back is band-relative (the per-herd estimate table is band-agnostic, so
# it cannot carry it), so the client adds it: ceil(2 × wrap-aware hex_distance(band, herd) /
# band_move_tiles_per_turn), the SAME formula the server's launch feed uses. When travel > 0 the headline
# turns is the TOTAL and this breakdown spells the split out; when 0 the headline is just the hunting turns.
const HUNT_FORECAST_TRAVEL_BREAKDOWN := " (%d hunting + %d travel)"
# The long raid's split, in the bounded breakdown's own shape so the two lines compare term for term. The
# hunting half wears "more than" (the horizon is a floor on it); the travel half is EXACT and must not,
# or the line would claim less than the client actually knows.
const HUNT_FORECAST_LONG_TRAVEL_BREAKDOWN := " (more than %d hunting + %d travel)"
# The horizon-less fallback: no hunting floor to state, so travel rides as a trailing "(+T travel)".
const HUNT_FORECAST_LONG_TRAVEL_SUFFIX := " (+%d travel)"
# The ONE non-viable case under the raid model: the party comes home with nothing. The SENTENCE it
# renders is not one sentence — see `HUNT_EMPTY_REFUSALS`, which keys it off the sim's own `bound`,
# because "the herd is spent" and "your party cannot kill it" are different facts with opposite
# remedies.
# A DENIAL mission is a raid with NO PAYLOAD, not a failed one. It is no longer "Eradicate": since
# issue #337 `delivers_food` says the QUARRY IS INEDIBLE, and Eradicate banks a whole-stock windfall
# like every other rung. The sim decides this — `delivers_food == false` — and the client never infers
# it from the policy string. **The `delivers_trade` half of that test went with the trade axis**
# (arc #527): an inedible quarry pays materials, which the raid wire states no figure for, so a raid
# after one now reads as a denial mission (`.claude/rules/client/labor-ui.md`).
const HUNT_FORECAST_DENIAL_FORMAT := "%s — denial mission: hunts the herd toward extinction, brings nothing home"
const HUNT_FORECAST_WARN_GLYPH := "⚠ "
# When a kill can't be fully carried (a big animal the crew is too small to haul) the surplus meat rots.
# A WARN-tinted suffix flags the fraction wasted — its OWN concern, rendered amber even on a green line.
const HUNT_WASTE_NOTE_FORMAT := "⚠ %d%% wasted"
const HUNT_WASTE_SUFFIX_FORMAT := " · " + HUNT_WASTE_NOTE_FORMAT

# ---- THE TRIP READOUT — the raid's own header and verdict (the expedition compose sheet) ---------
# **A TRIP HAS NO STEADY STATE, WHICH IS WHY IT CANNOT BORROW `YIELD_ROW_HEADER`.** The local sheet's
# `per turn · now → after` keys a rate and the transition into the holding state a resident crew
# settles at. A raid is one bounded errand: the party goes, takes what stands above the floor, and
# comes home — the numbers under this header are the WHOLE trip's, taken once, and there is no
# "after" to arrow toward. So the header names the errand instead of a rate, and neither the `/turn`
# nor the `now → after` key may follow the readout box onto this branch.
const EXPEDITION_TRIP_ROW_HEADER := "this trip"
# The verdict on a raid answers a different question from the local sheet's — nothing here is
# "binding", the party being fixed at launch — so it states the one cost a trip has: how long these
# hands are gone. The split renders only where there is travel to split off; a band already standing
# beside its quarry has none, and "18 turns — 18 hunting, 0 travel" would be three numbers for one.
const EXPEDITION_TRIP_VERDICT_FORMAT := "Away ≈%d turns."
const EXPEDITION_TRIP_VERDICT_SPLIT_FORMAT := "Away ≈%d turns — %d hunting, %d travel."
# `turns_to_fill == 0` is the sim saying the raid ran the whole forecast horizon still delivering. There
# is no TOTAL to quote, but there is a FLOOR — the horizon bounds the hunting and the round trip is known
# — so the verdict states it in the SAME span and the SAME shape the bounded pair above states theirs:
# "Away more than 78 turns — more than 60 hunting, 18 travel." The two are then comparable, which is the
# whole point of quoting a number instead of "many". The bound clause table renders NOTHING for `horizon`
# on the understanding that this sentence carries "still delivering at the end of the forecast" — keep it.
const EXPEDITION_TRIP_LONG_VERDICT_FORMAT := "Away more than %d turns. Still delivering at the end of the forecast."
const EXPEDITION_TRIP_LONG_VERDICT_SPLIT_FORMAT := "Away more than %d turns — more than %d hunting, %d travel. Still delivering at the end of the forecast."
# The horizon-less fallback pair: no floor on the wire, so the hedge stands and travel is named beside it
# rather than folded into a total that cannot be computed.
const EXPEDITION_TRIP_LONG_VERDICT := "Away many turns — still delivering at the end of the forecast."
const EXPEDITION_TRIP_LONG_VERDICT_TRAVEL_FORMAT := "Away many turns — still delivering at the end of the forecast, after %d turns of travel."

# ---- WHICH STOP ENDS THE TRIP (`docs/plan_hunt_through_combat.md` §5.2) -------------------------
# A trip LENGTH alone cannot say WHY the party turned for home — "the pack filled in 4 turns" and "you
# reach the floor in 2 turns with the pack a third full" are different situations carrying the same
# kind of number — so the SIM names the bound and this layer only renders it. These are
# `core_sim::HuntTripBound::as_str` keys, and the client never infers one from the numbers.
const TRIP_BOUND_KEY := "bound"
# **`""` IS "NOT STATED", AND IT IS NOT `TRIP_BOUND_HORIZON`.** On a launched party it means *not
# raiding* (a resident band, a scout, a party already walking a load home); on an estimate row it
# means a snapshot that predates the field. Both render NO clause, which is the only honest answer —
# `horizon`, by contrast, is the projection having run and found no stop.
const TRIP_BOUND_NONE := ""
const TRIP_BOUND_PACK_FULL := "pack_full"
const TRIP_BOUND_FLOOR := "floor"
const TRIP_BOUND_HERD_LOST := "herd_lost"
const TRIP_BOUND_HORIZON := "horizon"
# The sentence each bound adds after the trip's length. `horizon` renders NOTHING: the long-raid
# verdict above already says exactly that ("still delivering at the end of the forecast"), and a
# second spelling of it beside the first would be the same fact twice.
const TRIP_BOUND_CLAUSES := {
    TRIP_BOUND_PACK_FULL: "The pack fills; the herd never reaches your floor.",
    TRIP_BOUND_FLOOR: "The herd reaches your floor first — the party comes home part-loaded.",
    TRIP_BOUND_HERD_LOST: "The herd is wiped out before the party's load is made up.",
    TRIP_BOUND_HORIZON: "",
}

# ---- WHY AN EMPTY RAID IS EMPTY, AND WHO THE PLAYER HAS TO FIX ---------------------------------
# `delivered_food <= 0` is the arithmetic of "the party comes home with nothing", and it is still
# exactly right. What it does NOT say is WHY — and it used to be read as
# saying so, because before the take resolved through the fight (`docs/plan_hunt_through_combat.md`
# §4) a raid could only come home empty by finding the herd already at its floor. It cannot any more:
# a party that cannot bring one animal down inside the projection's horizon lands here too, with the
# herd's surplus standing untouched. Reported from play on a THRIVING Wild Aurochs herd with four
# animals affordable, refused as *"too lean to raid — its surplus is spent"* to a party of one.
#
# **A WRONG EXPLANATION IS WORSE THAN A WRONG NUMBER**: it sends the player to fix the wrong thing.
# The remedies are opposites — wait for the herd to rebuild against send more hunters — so one
# sentence cannot serve both.
#
# **THE SIM ALREADY TELLS THEM APART AND THE CLIENT NEVER INFERS IT FROM THE NUMBERS.** `HuntTripBound`
# names the stop that ended the projection, and the three reachable-with-nothing-delivered ones are
# distinct facts: `floor` is the herd-side stop (the standing surplus is spent), `horizon` is the
# projection running out with the party still empty-handed (it never killed anything — had it killed,
# it would have delivered and this branch would not be taken), and `herd_lost` is the quarry dying
# under a raid that never made up a load. `pack_full` CANNOT reach this branch: it requires a load,
# and a load is a delivery.
#
# Each entry carries the three faces of ONE refusal — the forecast LINE, the send button's face, and
# the spelled-out REASON — so the button cannot say "too lean" over a line naming the party. Adding a
# cause means adding all three at once, which is the point of one table rather than three.
# `line` and `reason` take the quarry's name; `button` takes none.
const HUNT_EMPTY_REFUSALS := {
    # THE HERD-SIDE STOP — the original case, wording unchanged. Party size genuinely cannot fix it:
    # standing surplus is a property of the herd.
    TRIP_BOUND_FLOOR: {
        "line": "%s is too lean to raid — its surplus is spent",
        "button": "Herd too lean to raid",
        "reason": "%s has nothing standing above this floor — the raid would return empty. Wait for the herd to rebuild, lower the floor, or hunt it locally.",
    },
    # THE PARTY-SIDE FAILURE. The herd is NOT at its floor — had it been, the projection would have
    # stopped on `floor` — so the line says so out loud, because the sentence it replaces claimed the
    # opposite. The three remedies are the three terms of the fight: headcount, kit, and the quarry's
    # own defence (`hunt_gate_model`, two lines above this on the sheet, states the arithmetic).
    TRIP_BOUND_HORIZON: {
        "line": "%s stands above your floor — but this party cannot bring one down",
        "button": "Party can't make the kill",
        "reason": "%s has surplus standing; these hunters simply never bring one down in the time a raid allows, so the party returns empty. Send more hunters, arm them better, or pick smaller game.",
    },
    # THE QUARRY DIES UNDER THE RAID WITHOUT PAYING FOR IT — reachable at a floor of 0, where nothing
    # stops the projection before the herd's extinction threshold. Neither the herd nor the party is
    # the thing to change; the QUARRY is.
    TRIP_BOUND_HERD_LOST: {
        "line": "%s is gone before the party can make up a load",
        "button": "Nothing left to raid",
        "reason": "%s collapses before your party lands anything — the raid would return empty. Leave it standing and find another quarry.",
    },
    # THE UNATTRIBUTED REFUSAL, keyed on `TRIP_BOUND_NONE` and used for every bound this branch cannot
    # explain — an estimate row carrying no bound at all, or one of the two party-side stops, which are
    # unreachable here. It names NEITHER side on purpose: guessing is how the defect above happened,
    # and a fixture that forgets its bound should read as unexplained rather than as somebody's fault.
    TRIP_BOUND_NONE: {
        "line": "%s — the raid would return empty",
        "button": "Raid returns empty",
        "reason": "This raid on %s brings nothing home, and the forecast does not say which of the herd and the party is the reason.",
    },
}

# ---- RETIRED: the fill target, the party-side twin of the floor (§5.2) ---------------------------
# A player-set "come home with N animals" stop shipped here and is GONE, sim and client alike (issue
# #491). Trip length is `carry ÷ (engage_rate × stay_chance × body_mass)` — **party size cancels** —
# so it is a species-and-kit constant, and this lever was the only thing that moved it. It existed to
# escape the trips nobody wants (Wild Fowl 88 turns against Mammoth 1.1); that spread is a TUNING
# problem and is tracked as one on #491, not a second dial for the player to hold. Every raid is now
# the untargeted raid — the default the whole control collapsed to — so `send_hunt_expedition` closes
# after the floor and the trip's bound can only be `pack_full` / `floor` / `herd_lost` / `horizon`.

# THE SEND BUTTON'S FOUR FACES, owned by `style_send_hunt_button`. A trip that is a trap names the cost
# (amber "armed") but is NEVER gated behind a confirm — the player is told, then trusted. Only the
# no-surplus raid, which has no upside at all, disables.
#
# **THE `Send` STEM IS LOAD-BEARING and the base label sheds only its redundant words.** The three
# variants below rewrite this same button as `Send Anyway (…)` / `Send (brings nothing home)`, so the
# resting face has to be the same verb they vary; what it does NOT have to do is restate the sheet's own
# `ASSIGN HUNTERS <herd>` header or name the party a second time. (The disabled no-surplus face is the
# one that leaves the stem, and deliberately: it is a refusal, not a send.)
const SEND_HUNTING_EXPEDITION_BUTTON := "Send Expedition"
const SEND_HUNT_ANYWAY_TURNS_FORMAT := "Send Anyway (≈%d turns)"
# A LONG raid (`turnsToFill == 0`, ran the whole horizon still delivering) still lands animals — enabled,
# and the button now names the FLOOR on the trip in the same clause its bounded twin names the total, so a
# player choosing between two quarries compares two numbers rather than a number and a word.
const SEND_HUNT_LONG_RAID_FORMAT := "Send Anyway (more than %d turns)"
# The horizon-less fallback: no floor to quote, so the button names the haul without a figure.
const SEND_HUNT_LONG_RAID_BUTTON := "Send Anyway (long raid)"
# The ONE blocked case: the raid comes home with nothing in either currency. That is a mistake with no
# upside (unlike a slow-but-delivering raid), so the button is DISABLED and says why plus the way out —
# and its FACE is keyed off the same `bound` the refusal line is, so the button can never contradict
# the sentence above it. See `HUNT_EMPTY_REFUSALS`.
# A denial raid's button states the deal rather than implying failure — the mission IS the point. It
# is the quarry that decides this (pays neither product), not the rung: see HUNT_FORECAST_DENIAL_FORMAT.
const SEND_HUNT_DENIAL_BUTTON := "Send (brings nothing home)"

## **A STANDING STOCK, in the units the rest of the HUD reads one in** — whole biomass, matching the
## drawer's own `Forage biomass 35 / 100` pair. It is NOT `format_magnitude`, which is the food-RATE
## rule (two decimals): a rate of 0.31/turn genuinely needs them and a stock of 1075 does not, and
## spending them there prints `1075.00`, claiming a precision the number does not have.
static func format_stock(value: float) -> String:
    return "%d" % int(round(value))

## The bare magnitude of a food rate ("1.74"), for a readout that supplies its own sign in words
## ("− 1.74 feed"). One rounding rule for every food rate the HUD prints.
static func format_magnitude(value: float) -> String:
    return String.num(absf(value), YIELD_DECIMALS).pad_decimals(YIELD_DECIMALS)

## A signed, fixed-decimal food-rate string ("+0.31" / "-0.30"). Actual yields are ≥0, but the
## formatter is sign-aware so it also renders Net (which can go negative) and Consumption (shown
## as a negative cost).
static func format_signed(value: float) -> String:
    var sign_str := "+" if value >= 0.0 else "-"
    return sign_str + format_magnitude(value)

## The same rate with the "/turn" suffix, for the per-source row headline ("+0.31 /turn").
static func format_yield(value: float) -> String:
    return format_signed(value) + YIELD_PER_TURN_SUFFIX

## True when a rate is a real quantity rather than the absence of one. The gate every
## render-only-when-non-zero decision goes through, so "is this component present?" is answered
## identically for food and fodder and by every surface.
##
## **ITS FLOOR IS THE DISPLAY'S, NOT THE MODEL'S** (#426), and the two are different numbers. This
## used to read `>= FOOD_FLOW_MIN` (0.001) — the *food-flow* floor, which is a claim about the
## simulation — while every caller renders at `YIELD_DECIMALS` (2). A rate in between therefore
## PASSED the gate and then printed as `0.00`: a single forager on a staple patch earned ~0.003 of the
## then-third account a turn, and the preview line duly read `+0.08 /turn · ⇄ +0.00 · 0.13 fodder`.
## That zero is exactly the false precision the render-only-when-non-zero rule exists to remove — the
## gate was letting through the very thing it was written to stop, because it was measuring in
## different units from the formatter it gates.
##
## `FOOD_FLOW_MIN` stays where it is and keeps its own job: whether the BAND has a food flow at all
## is a question about the sim, not about how many decimals a label shows.
static func has_component(rate: float) -> bool:
    return rate >= COMPONENT_RENDER_MIN

# =====================================================================================
#  THE FORECAST'S BAND (docs/plan_hunt_through_combat.md §6.4)
# =====================================================================================
# `actual_yield` stopped being a promise and became an EXPECTATION: the take the sim pays lies inside
# `[actual_yield_low, actual_yield_high]`. (A second pair carried the same band in the retired trade
# account; it went with the axis, arc #527.)
# **A BAND ON A HUNT IS SHIPPED BEHAVIOUR, NOT A BUG** — wariness is authored across the
# whole roster (fauna_config's `combat.wariness`, 0.10 on a mammoth that stands and fights up to 0.85
# on a gazelle that simply is not there), so the RETREAT binomial is live and a raid's reported low
# and high genuinely differ. `hit_chance` still ships `1.0`, so the damage binomial contributes
# nothing to the spread; the width comes from animals breaking off before contact, which is why the
# clause this client writes says *likely* rather than naming a combat roll.
#
# **THE DISTRIBUTION IS STILL DEGENERATE IN THREE PLACES, and each is a real state rather than a
# leftover**: a RESOLVED row (the take happened, so there is nothing left to distribute), a source
# that publishes no retreat stage at all (every forage patch — the plant web has no wariness), and a
# spread narrow enough that both bounds round to one printed string, which `has_yield_range` treats as
# one number by design.
#
# **SO THE READOUT RENDERS ONE NUMBER WHEN THE BOUNDS AGREE**, and the range only when they differ.
# The degenerate case is PINNED rather than merely expected, because that is the half a readout
# decorating every row would still satisfy — see `chapters/hunt.gd`'s `herd_hunt_yield_range` /
# `herd_hunt_yield_point` pair.
#
# **The band is an ANSWER, never a term.** The take passes through the whole-animal quantiser's
# `floor()`, so a band on the animals brought down is not a band on the food; the client renders the
# pair the sim published and composes nothing from wariness or hit-chance.
const YIELD_RANGE_LOW_KEY := "actual_yield_low"
const YIELD_RANGE_HIGH_KEY := "actual_yield_high"
# The band's shape — an en dash between the two bounds. It rides BESIDE the expectation rather than
# replacing it: `actualYield` is the number `forecast == actual` is restated on, so the band
# QUALIFIES the headline and never becomes it.
const YIELD_RANGE_TOOLTIP_FORMAT := "%s–%s"
# The band as a clause on the row's take note and on its tooltip. One word — "likely" — because the
# spread's cause (quarry breaking off before contact) is a species property the row cannot name and a
# sentence the row has no width for.
const YIELD_RANGE_CLAUSE_FORMAT := " · likely %s"

## **IS THERE A BAND HERE AT ALL?** — the ONE test, so no two surfaces can disagree about whether a
## source's take is uncertain. Gated at the FORMATTER's resolution rather than on raw inequality, the
## same call `has_component` makes and for the same reason: bounds that round to one printed string
## are one number on screen, and a `low != high` test would render `0.31–0.31` as a range.
static func has_yield_range(low: float, high: float) -> bool:
    return format_magnitude(high) != format_magnitude(low)

## The band as a bare `6.00–11.00`, or the point's own magnitude when the bounds agree. Magnitudes,
## not signed rates: a take is never negative, and a `+6.00–+11.00` reads as an arithmetic expression.
static func format_yield_range(low: float, high: float) -> String:
    return YIELD_RANGE_TOOLTIP_FORMAT % [format_magnitude(low), format_magnitude(high)] \
        if has_yield_range(low, high) else format_magnitude(low)

## **THE FOOD BAND AS ONE CLAUSE** (` · likely 6.00–11.00`), or `""` when it is not a band — which is
## the shipped case, and what keeps every existing readout byte-identical. Reads the pair straight off
## a labor-assignment / worker-map dict; an assignment from a snapshot that predates the fields
## carries no key at all, and two absent bounds are equal, so it answers `""` there too.
##
## **THE RETIRED TRADE ACCOUNT'S BAND WENT WITH IT** (arc #527). It was read separately and never
## substituted for this one, because an inedible quarry's food band is honestly all-zero while its
## trade band was the whole of what the raid paid. Such a quarry now states no band at all.
static func yield_range_clause(m: Dictionary) -> String:
    var clause := ""
    var food_low := float(m.get(YIELD_RANGE_LOW_KEY, 0.0))
    var food_high := float(m.get(YIELD_RANGE_HIGH_KEY, 0.0))
    if has_yield_range(food_low, food_high):
        clause += YIELD_RANGE_CLAUSE_FORMAT % format_yield_range(food_low, food_high)
    return clause

# =====================================================================================
#  THE PRE-LAUNCH FIGHT (docs/plan_hunt_through_combat.md §2.1, §4.2, §6.5)
# =====================================================================================
# A hunt is a fight, and two of its facts have to be legible BEFORE the party leaves.
#
# **HOW MANY HUNTERS ONE ANIMAL TAKES** is `1 / engageRate` — "twenty hunters to take a mammoth" is a
# number a player can reason about and `0.05` is not. It is composed here from the SAME
# `engagement_per_worker` every crew target divides by, never from a second reading of the pair, so
# the sentence and the stepper's cap can never disagree; and the dip rides it for the same reason it
# rides them (hands gentling a herd are hands not stalking it).
#
# **WHETHER THE FIGHT CAN BE WON AT ALL** is `max(0, hunterAttack − defense)`. At zero the party
# kills nothing at ANY headcount and still takes casualties, which is a real outcome that reads as a
# bug unexplained. The sim deliberately exports no verdict — the expression is linear and exact in
# three terms already on the wire, so it ships as terms and the client asks itself the question.
#
# `durability / effective_attack` turns *"you cannot"* into an effort figure: the hunter-turns ONE
# hunter needs per kill, which is what makes the two ends of the roster comparable (a mammoth at 62
# against a rabbit at 0.1). **It is deliberately not divided by the party**: the herd's accumulated
# wounds are not exported, so a per-party turn count here would be a second, always-pessimistic
# duration model competing with the raid forecast the sim answers when asked.

## Whether the pre-launch fight lines have anything to say about this source. A PEN and the whole
## PLANT web publish no engagement stage (`NO_ENGAGEMENT_STAGE`), which is exactly the byte-identity
## this gate buys: a penned animal is not stalked and a berry does not fight back.
static func has_engagement_stage(engage_rate: float, dip: float) -> bool:
    return not is_inf(engagement_per_worker(engage_rate, dip))

# THE GATE's ONE verdict. It names both terms, because "you cannot" without the arithmetic is a
# tooltip the player has no way to act on: knowing it is the WEAPON and not the headcount is the whole
# lesson (`4.8` — the first spear should feel like a different game). It is also the honesty line the
# `none` kit depends on (`docs/plan_denial_raid.md`): with the estimate tables suppressed for a kit
# they are not quoted at, this is what still answers what the party can and cannot hurt.
#
# **THE WINNABLE BRANCH'S FACE IS RETIRED** (reported from playtest). `0.1 hunter-turns to bring one
# Wild Fowl down` was a species constant that never moved with anything the player was dialling,
# printed directly above a forecast that already prices the whole trip. The MODEL still answers
# `blocked` / `effective_attack`; what went is the sentence for the case that needs none.
const HUNT_GATE_BLOCKED_FORMAT := "%sYour hunters cannot hurt %s — attack %s against its defense %s. No party size changes that: they would take casualties and kill nothing."
# What `attack`/`defense`/`durability` are printed with. They are open-ended strength scalars on a
# human anchor of 1, authored as small whole-ish numbers, so a rate's two decimals would be false
# precision — `attack 20.00` claims a resolution the roster does not have.
const HUNT_GATE_SCALAR_DECIMALS := 0

## **THE COMBAT GATE, COMPOSED CLIENT-SIDE FROM THREE TERMS ALREADY ON THE WIRE** —
## `{stated, blocked, effective_attack, text}` — and `text` is non-empty only when `blocked`.
##
## `stated` is false when the band or the herd is silent about its half: a snapshot that predates the
## fields, or a species the roster cannot resolve (`durability == 0`). Absent terms must render NO
## line — a defaulted `attack 0` would refuse every hunt in the game.
##
## **`blocked` and the forecast's own zero are two signals from different paths, and both are kept.**
## The sim quotes a sub-gate party `0` at every quantile because the fight is already inside
## `hunt_source_yield_preview`; this line is arithmetic over the exported terms. A failure in either
## still leaves the player warned, which is the whole point of not exporting a verdict.
static func hunt_gate_model(band: Dictionary, herd: Dictionary, quarry: String) -> Dictionary:
    if not band.has(BAND_HUNTER_ATTACK_KEY):
        return {"stated": false, "blocked": false, "effective_attack": 0.0, "text": ""}
    return hunt_gate_model_at(float(band.get(BAND_HUNTER_ATTACK_KEY, 0.0)), herd, quarry)

## **THE GATE AT AN ARBITRARY ATTACK TIER** — the same arithmetic over the same two herd terms, asked
## about a party whose own kit has already resolved its effective attack (`KitRoster.effective_tiers`)
## rather than about the band's default-kit tier.
##
## It exists because the gate is the ONE forecast that stays honest for every kit: it is composed from
## wire terms rather than looked up in a table quoted for one kit, so a sheet that has to suppress the
## estimate tables can still say what this party can and cannot hurt. For 15 of 20 roster species a
## bare-handed party's effective attack is 0, and the line says so plainly.
##
## `hunt_gate_model` is exactly this asked at the band's own tier, so the two can never disagree about
## what a gate is; only about whose attack it is.
static func hunt_gate_model_at(attack: float, herd: Dictionary, quarry: String) -> Dictionary:
    var blank := {"stated": false, "blocked": false, "effective_attack": 0.0, "text": ""}
    var defense := float(herd.get(HERD_DEFENSE_KEY, 0.0))
    # `durability` is still the STATED-ness test even though no surviving face quotes it: a species
    # the roster cannot resolve reads `0`, and answering `blocked` about one whose defence we could
    # not look up would refuse a hunt on a gap in the data.
    if float(herd.get(HERD_DURABILITY_KEY, 0.0)) <= 0.0:
        return blank
    var effective := maxf(attack - defense, 0.0)
    if effective > 0.0:
        # **A WINNABLE FIGHT SAYS NOTHING, and `text` is empty rather than absent.** The reading the
        # caller acts on is `blocked`; `effective_attack` stays for anyone composing on the margin.
        return {"stated": true, "blocked": false, "effective_attack": effective, "text": ""}
    return {"stated": true, "blocked": true, "effective_attack": 0.0,
        "text": HUNT_GATE_BLOCKED_FORMAT % [
            HUNT_FORECAST_WARN_GLYPH, quarry,
            String.num(attack, HUNT_GATE_SCALAR_DECIMALS),
            String.num(defense, HUNT_GATE_SCALAR_DECIMALS)]}

# The three wire terms the gate is composed from — the BAND's resolved per-hunter attack (1 bare-
# handed, 20 speared) and the HERD's two defensive axes. `defense` is whether a hit counts at all,
# `durability` is how many counting hits it takes; they blur easily and must not.
const BAND_HUNTER_ATTACK_KEY := "hunter_attack"
const HERD_DEFENSE_KEY := "defense"
const HERD_DURABILITY_KEY := "durability"

## THE ONE DEFINITION of a worked source's FODDER rate (issue #449), read off a labor-assignment /
## worker-map dict — the second account beside the food rate, and the reason a sown hay Field stops
## reading `+0.00` on every compact readout in the HUD.
##
## **A PLAIN READ IS THE WHOLE OF IT.** There is deliberately no `realized_fodder_yield`, because a
## realized rate is a FORWARD PROJECTION and only the ANIMAL web projects one. Fodder is paid by the
## PLANT web alone, so a projected-fodder field would be a constant zero on the only web that can pay
## it, and **the actual IS the honest rate here**.
##
## Plant-only, structurally: no animal pays fodder, so a hunt row reads `0.0` and every hunt-side
## surface renders exactly as it did before this existed.
##
## Its retired sibling was `trade_rate_of`, which had a `realized_trade_yield` sentinel to dodge; both
## the reader and the wire fields went with the trade axis (arc #527).
static func fodder_rate_of(source: Dictionary) -> float:
    return float(source.get("fodder_yield", 0.0))

## THE RENDER-ONLY-WHEN-NON-ZERO JOINER for a per-turn readout: `+0.31 /turn` (food only),
## `+0.08 /turn · 0.40 fodder` (a hay meadow), `0.40 fodder` (a hay-only one). One definition, so
## every surface that states a source's per-turn products states them the same way and none of them
## can print a zero for a component the source does not produce. Food leads. When EVERY component is
## absent the food zero survives ("+0.00 /turn"): a worked source that produced nothing this turn is
## a fact worth reading.
##
## The fodder term wears the WORD, not a glyph, because fodder has none — the same reason
## `picker_products` names its accounts. It is plant-only, so every hunt-side caller leaves it
## defaulted and reads exactly as it did.
##
## `zero_account` names the component whose zero survives an all-empty take (`zero_account_of`), so a
## hay-only meadow reads `0.00 fodder` rather than the `+0.00 /turn` that says its hay is worth no
## meals, and a source that pays nothing in either account renders no line at all.
static func yield_components(food: float, fodder: float = 0.0,
        zero_account: String = YIELD_ACCOUNT_FOOD, materials: Array = []) -> String:
    var parts: Array[String] = []
    for row in yield_rows(food, fodder, zero_account, {}, materials):
        var material_id := _row_material_id(row)
        if material_id != "":
            parts.append(PICKER_MATERIAL_PRODUCT_FORMAT % [
                format_magnitude(row[YIELD_ROW_VALUE]), material_id])
        elif String(row[YIELD_ROW_ACCOUNT]) == YIELD_ACCOUNT_FOOD:
            parts.append(format_yield(row[YIELD_ROW_VALUE]))
        else:
            parts.append(PICKER_FODDER_PRODUCT_FORMAT % format_magnitude(row[YIELD_ROW_VALUE]))
    return COMPONENT_SEPARATOR.join(parts)

## THE COMPACT TWIN of `yield_components`, for a surface that supplies its own framing and has no room
## to repeat "/turn" — today the work zone's per-kind filter chips (`🌿 1 · 0.40 fodder`). Same
## render-only-when-non-zero rule and same food-leads order, but BARE MAGNITUDES: a chip states a
## count and that kind's total, and a `+` beside a count would read as a change rather than a level.
## The point of the pair here is aggregate honesty — a chip whose whole set pays FODDER alone shows
## that total rather than a `0.00` asserting its sources produce nothing.
##
## The fodder term wears the WORD, not a glyph, for `yield_components`' reason: fodder has none. It is
## plant-only, so every hunt-side caller leaves it defaulted and its chip reads exactly as before.
static func magnitude_components(food: float, fodder: float = 0.0,
        materials: Array = []) -> String:
    var parts: Array[String] = []
    for row in yield_rows(food, fodder, YIELD_ACCOUNT_FOOD, {}, materials):
        var material_id := _row_material_id(row)
        if material_id != "":
            parts.append(PICKER_MATERIAL_PRODUCT_FORMAT % [
                format_magnitude(row[YIELD_ROW_VALUE]), material_id])
        elif String(row[YIELD_ROW_ACCOUNT]) == YIELD_ACCOUNT_FOOD:
            parts.append(format_magnitude(row[YIELD_ROW_VALUE]))
        else:
            parts.append(PICKER_FODDER_PRODUCT_FORMAT % format_magnitude(row[YIELD_ROW_VALUE]))
    return COMPACT_COMPONENT_SEPARATOR.join(parts)

## A `{compact, full}` metric pair for an EXTRACTIVE rung, over the source's whole yield VECTOR — the
## metric on every one of the three pickers. Food leads; each component appears only when it is
## non-zero, so a hay-bearing patch's rungs read food-then-fodder and a hay-only meadow's read fodder
## alone. When the rung pays nothing at all the food zero is still printed: `0.00 food` is the honest
## reading of a ceiling that exists and is empty, as opposed to a component the source never had. The
## compact half is the face's product LINE (`picker_products`, named in words); the tooltip keeps the
## signed "up to …" ceiling wording.
##
## **The forage picker comes through here too now** (#426). It used to call a food-only
## `extractive_take`, on the standing claim that the plant web projected no non-food rate — which
## stopped being true the turn the per-policy row reached the wire carrying every account. That
## food-only twin is deleted rather than left as an alias: one joiner is what keeps the three pickers
## wearing one face.
static func extractive_take_pair(food: float, fodder: float = 0.0,
        zero_account: String = YIELD_ACCOUNT_FOOD, materials: Array = []) -> Dictionary:
    var full_parts: Array[String] = []
    for row in yield_rows(food, fodder, zero_account, {}, materials):
        var material_id := _row_material_id(row)
        if material_id != "":
            full_parts.append(POLICY_CAP_MATERIAL_FORMAT % [
                format_signed(row[YIELD_ROW_VALUE]), material_id])
        elif String(row[YIELD_ROW_ACCOUNT]) == YIELD_ACCOUNT_FOOD:
            full_parts.append(POLICY_CAP_FORMAT % format_signed(row[YIELD_ROW_VALUE]))
        else:
            full_parts.append(POLICY_CAP_FODDER_FORMAT % format_signed(row[YIELD_ROW_VALUE]))
    return {
        "compact": picker_products(food, fodder, zero_account, materials),
        "full": COMPONENT_SEPARATOR.join(full_parts),
    }

## The band's current tile (col,row), reading the raw cohort `current_x/y` (snapshot entries) or the
## MapView marker's `pos` fallback; (-1,-1) when unknown.
static func band_tile(band: Dictionary) -> Vector2i:
    var cx := int(band.get("current_x", -1))
    var cy := int(band.get("current_y", -1))
    if cx >= 0 and cy >= 0:
        return Vector2i(cx, cy)
    var pos_variant: Variant = band.get("pos", [])
    if pos_variant is Array and (pos_variant as Array).size() == 2:
        return Vector2i(int((pos_variant as Array)[0]), int((pos_variant as Array)[1]))
    return Vector2i(-1, -1)

## odd-r offset (col,row) → axial (mirrors MapView._offset_to_axial).
static func _offset_to_axial(col: int, row: int) -> Vector2i:
    var q := col - ((row - (row & 1)) >> 1)
    return Vector2i(q, row)

## Shortest signed column delta from→to honoring horizontal wrap (mirrors MapView._wrapped_col_delta),
## so a herd across the seam measures by its short wrapped distance, not the long way across the map.
## Mirrors the sim's `grid_utils::shortest_delta_x` exactly (magnitude only here, no live
## direction effect): keep the direct delta when within half the width, else shift by one width.
## The exact-half tie (`abs(d) == width/2`) keeps the DIRECT signed delta (so `-width/2` stays
## negative), matching the sim, NOT `round()`'s half-away-from-zero — kept consistent with
## MapView._wrapped_col_delta.
static func _wrapped_col_delta(from_col: int, to_col: int, grid_width: int, wrap_horizontal: bool) -> int:
    var d := to_col - from_col
    if wrap_horizontal and grid_width > 0:
        # Integer half-width mirrors the sim's `w / 2` truncation.
        var half_width := grid_width / 2
        if d > half_width:
            d -= grid_width
        elif d < -half_width:
            d += grid_width
    return d

## Wrap-aware true odd-r hex distance between two offset tiles (mirrors the sim's `hex_distance_wrapped`
## / MapView._hex_distance): bring the target into the source's column frame via _wrapped_col_delta,
## then odd-r offset→axial→cube distance. Returns -1 when either tile is unknown.
static func hex_distance_wrapped(a_col: int, a_row: int, b_col: int, b_row: int,
        grid_width: int, wrap_horizontal: bool) -> int:
    if a_col < 0 or a_row < 0 or b_col < 0 or b_row < 0:
        return -1
    var b_eff_col := a_col + _wrapped_col_delta(a_col, b_col, grid_width, wrap_horizontal)
    var a := _offset_to_axial(a_col, a_row)
    var b := _offset_to_axial(b_eff_col, b_row)
    var dq: int = a.x - b.x
    var dr: int = a.y - b.y
    return int((abs(dq) + abs(dr) + abs(dq + dr)) / 2)

## Round-trip TRAVEL turns for a raid party walking from `band` out to `herd` and back — the honest
## remainder of the trip length the sim's answer does not carry: `turns_to_fill` counts HUNTING turns
## only, whichever band asked. Matches the sim launch feed EXACTLY: ceil(2 × wrap-aware hex_distance(band, herd)
## / band_move_tiles_per_turn), from the SELECTED band's tile + the exported move rate.
## Returns 0 — so the forecast degrades to hunting turns only, never a fabricated travel — when the move
## rate isn't on the band dict or a position is unknown. `band_move_tiles_per_turn` (a LaborConfig scalar
## echoed per-cohort) is now decoded in `native/src/lib.rs` and flowed onto the band marker, so this
## lights up on the live wire; it degrades gracefully if a future snapshot omits it.
static func round_trip_travel_turns(band: Dictionary, herd: Dictionary,
        grid_width: int, wrap_horizontal: bool) -> int:
    var move_rate := float(band.get("band_move_tiles_per_turn", 0.0))
    if move_rate <= 0.0:
        return 0
    var origin := band_tile(band)
    var one_way := hex_distance_wrapped(
        origin.x, origin.y, int(herd.get("x", -1)), int(herd.get("y", -1)),
        grid_width, wrap_horizontal)
    if one_way < 0:
        return 0
    return int(ceil(float(TRAVEL_LEGS_PER_ROUND_TRIP * one_way) / move_rate))

# A round trip is out and back. Named because `outbound_travel_turns` divides by it and a bare `2`
# there would read as an unexplained halving rather than as "one of the two legs".
const TRAVEL_LEGS_PER_ROUND_TRIP := 2

## **THE OUTBOUND LEG ALONE — the walk OUT, taken from the round trip rather than measured again.**
##
## There is exactly ONE definition of travel in this client (`round_trip_travel_turns`, which mirrors
## the server's launch feed), and this is a reading of it, not a second one: for an integer `n`,
## `ceil(ceil(x)/n) == ceil(x/n)`, so `ceil(round_trip / 2)` is EXACTLY `ceil(one_way / move_rate)` —
## the turn the party arrives. A second `hex_distance ÷ move_rate` here would be a second definition
## free to drift from the one the hunt readout and the server both use.
##
## **WHO WANTS THE OUTBOUND LEG RATHER THAN THE ROUND TRIP:** a HUNT's payload only counts once it is
## carried home, so its headline is the whole round trip. A DENIAL raid's verdict is about the HERD
## crossing the point of no return — an event that happens on the range, the moment the party has
## walked there and started killing — so the return leg falls outside the span the verdict is about.
static func outbound_travel_turns(band: Dictionary, herd: Dictionary,
        grid_width: int, wrap_horizontal: bool) -> int:
    return int(ceil(float(round_trip_travel_turns(band, herd, grid_width, wrap_horizontal))
        / float(TRAVEL_LEGS_PER_ROUND_TRIP)))

## **THE STOCK STANDING ABOVE `floor`, in biomass** — `max(0, B − floor·K)`, the client half of the
## one expression both take paths pay (`fauna::hunt_escapement_ceiling` /
## `forage::forage_escapement_ceiling`). Multiply by an account's per-biomass rate for that account's
## ceiling.
##
## **THIS IS A DELIBERATE, NARROW EXCEPTION TO "THE SIM EXPORTS THE ANSWER", AND IT HAS A BOUNDARY.**
## That rule exists because a hunt's TAKE is rounded to whole animals — `floor(ceiling / bodyMass)` is
## not linear, so no client can re-derive it and the sim must hand over the result. This expression is
## different in kind: linear and exact in terms already on the wire, so a client evaluating it lands
## on the number the sim would. The division of labour is **the client draws the curve, the sim states
## the take**: `SourceYield.actual` for a COMMITTED assignment is still the sim's answer, quantisation
## and all. Do not let this composition creep from ceilings into takes.
##
## **THE BUILD DIP IS NOT HERE — it multiplies the CREW** (`docs/plan_harvest_floor.md` §3.1). Dipping
## the ceiling let a deeper floor build for free (a fraction of a bigger standing stock still filled
## the crew's baskets), and moving it off the ceiling is what leaves this linear in the floor and
## therefore composable at all. Any surviving code that multiplies a ceiling by a build fraction is
## wrong, and it looks plausible.
static func escapement_room(src: Dictionary, prefix: String, floor: float) -> float:
    var biomass := float(src.get(prefix + FORECAST_BIOMASS_KEY, 0.0))
    var capacity := float(src.get(prefix + FORECAST_CAPACITY_KEY, 0.0))
    return maxf(0.0, biomass - clamp_floor(floor) * capacity)

## Is this source RUNG-3 MANAGED — a Field, or a built Pen? Such a source is never drawn down, so it
## pays its managed production at EVERY floor and the escapement composition does not apply to it (see
## `FORECAST_MANAGED_FLAG_KEYS`).
##
## **IT IS THE STANDING RUNG THAT DECIDES THIS, NEVER THE COMPOSED ONE.** The wire flag is the only
## input, and a source the crew is merely BUILDING toward rung 3 — mid-`Sow`, mid-`Corral` — is
## deliberately NOT managed, because until the Field or the Pen exists the crew is still drawing the
## WILD stand down (at a dipped carry — see `build_dip`), and that drawdown is exactly what the sheet
## has to price. Reading the composed rung here would quote a source that does not exist yet: the
## escapement chart would blank on a stand that is still being harvested, and a pastoral herd's
## ceiling would swap to `corral_yield` while its animals are still being hunted off the range. Rung 2
## (a Tended Patch, a pastoral herd) is not managed either, and for the same reason — it is a wild
## stand and the sim keeps it floor-live.
##
## This function therefore takes NO improvement argument, and that absence is the point: it carried an
## unused `improvement` parameter through five call sites, which read as an invitation to honour it.
static func source_is_managed(src: Dictionary, kind: String, prefix: String) -> bool:
    if not FORECAST_MANAGED_FLAG_KEYS.has(kind):
        return false
    return bool(src.get(prefix + String(FORECAST_MANAGED_FLAG_KEYS[kind]), false))

## **THE CREW'S THROUGHPUT IN BIOMASS, READ OFF THE WIRE** — what ONE worker moves before any account
## conversion, on either web. Everything on the crew side of the panel divides by this: the take's
## per-account crew terms below, and the two worker targets (`crew_to_clear` / `crew_to_hold`).
##
## **IT USED TO BE DERIVED AS `per_worker_yield / provisions_per_biomass`, AND THAT DERIVATION WAS
## `0/0` ON EXACTLY THE SOURCES THAT MOST NEEDED IT** — a sown Field of flax, cotton or hay grass, and
## a wolf: both honestly pay no food, so both numerator and denominator vanish and the panel could
## state no crew number at all. `perWorkerBiomass` exists for that case; do not go back.
##
## **A ZERO IS A REAL READING, NOT AN ABSENT ONE.** A patch's throughput carries the tile's seasonal
## weight, so a dead-season patch honestly moves no biomass per worker. Callers must not divide by it
## (`can_price_crew` is that test), and must not read it as "the wire sent no forecast" — the stock,
## the capacity and the rate vector still describe the patch.
static func per_worker_biomass(src: Dictionary, prefix: String) -> float:
    return maxf(0.0, float(src.get(prefix + FORECAST_PER_WORKER_BIOMASS_KEY, 0.0)))

## Is this source's crew throughput a number a target may be divided by? The one guard between
## `per_worker_biomass` and every quotient taken from it, so a dead season states "no crew answer"
## once rather than producing an infinity in three places.
static func can_price_crew(carry: float) -> bool:
    return carry > FORECAST_MIN_PER_WORKER

# ---- THE SAMPLED GROWTH CURVE (docs/plan_harvest_floor.md §7.3) ---------------------------------
#
# **THE OTHER HALF OF THE BOUNDARY THIS FILE ALREADY SITS ON.** Where a closed form exists the sim
# ships the TERMS and this layer evaluates it — that is the escapement ceiling, `max(0, B − f·K) ×
# rate`, linear and exact, which is what let the four stance rows be retired. Where one does NOT, the
# sim ships ANSWERS and this layer INTERPOLATES between them. The growth curve is the second case, and
# not because it is hard to write down: it is **two different functions**. A patch is pure logistic
# with a reseed floor and NO Allee term; a herd has critical depensation below `collapse_fraction`.
# A GDScript copy of either would be a second growth model with no tests over it, and the drift would
# be INVISIBLE, because a wrong curve still looks like a curve.
#
# So: **interpolate the samples. Never fit them, never extrapolate past them, never smooth them into
# a formula.** Sample `i` of `n` is the source's one-turn biomass delta at `B = i/(n−1) × K`; the
# x-axis is uniform and therefore implicit, which is why no spacing rides the wire.
#
# **THE ANIMAL CURVE GOES NEGATIVE AND THE PLANT CURVE NEVER DOES — render the negatives as DECLINE.**
# Clamping them to zero is the instinctive thing to do with a chart and it would draw a herd crashing
# to extinction as a herd sitting still, which is exactly the asymmetry that makes floor 0 end a herd
# and merely set a patch back.
# The sim's own resolution (`snapshot::REGROWTH_CURVE_SAMPLES`) is a DISPLAY choice, not a model fact,
# so nothing here may assume a count — only that there are at least two points to interpolate between.
const REGROWTH_CURVE_MIN_SAMPLES := 2
# How far the projection walks. It is the chart's x-axis and the verdict's patience in one number: a
# crew that would reach the floor in 300 turns is honestly described as settling short over what the
# player can see, so the verdict and the drawn curve agree by construction.
const PROJECTION_HORIZON_TURNS := 60
# `reached_turn` when the crew never draws the source down to its floor within the horizon.
const PROJECTION_REACHED_NONE := -1
# "At the floor" as a fraction of K. A DISPLAY tolerance, like `FLOOR_EPSILON`: the projection walks
# in floating point, so an exact equality would miss the turn the stock lands on its floor.
const STOCK_FRACTION_EPSILON := 0.0001
# A crew answer the wire cannot price — a dead-season patch moves no biomass per worker, so "how many
# workers" has no denominator. Distinct from `0`, which is a real answer ("none are needed").
const NO_CREW_ANSWER := -1
# How far past the closed-form estimate `crew_that_reaches` probes. The estimate is exact for
# *reaching equilibrium*; the extra steps only cover reaching it within the drawn HORIZON, which is a
# turn count rather than a threshold. Small on purpose: each step is one projection walk.
const CREW_PROBE_STEPS := 8

## The source's sampled regrowth curve, or an EMPTY array when the wire published none. Empty and
## all-zero are different claims — "no curve was sent" (draw no projection) versus "this source does
## not grow" (draw a flat one) — so the absence is preserved rather than defaulted.
static func regrowth_samples(src: Dictionary, prefix: String) -> PackedFloat32Array:
    var raw: Variant = src.get(prefix + FORECAST_REGROWTH_SAMPLES_KEY, null)
    if raw is PackedFloat32Array:
        return raw as PackedFloat32Array
    # A fixture (or a Dictionary round-trip) may hand the same samples over as a plain Array.
    if raw is Array:
        var packed := PackedFloat32Array()
        for value in (raw as Array):
            packed.push_back(float(value))
        return packed
    return PackedFloat32Array()

## Is there a curve to interpolate at all? One point cannot be interpolated between, so it is no more
## usable than none.
static func has_growth_curve(samples: PackedFloat32Array) -> bool:
    return samples.size() >= REGROWTH_CURVE_MIN_SAMPLES

## **THE SOURCE'S ONE-TURN REGROWTH AT A GIVEN STOCK**, `stock_fraction` in `0..1` of K — linearly
## interpolated between the two samples that bracket it. The value may be NEGATIVE (a herd below its
## Allee point declines whether or not it is hunted); callers must carry that sign through rather than
## clamping it, which is the whole reason the curve is sampled instead of composed.
static func regrowth_at(samples: PackedFloat32Array, stock_fraction: float) -> float:
    if not has_growth_curve(samples):
        return 0.0
    var last := samples.size() - 1
    var position := clampf(stock_fraction, 0.0, 1.0) * float(last)
    var low := clampi(int(floor(position)), 0, last)
    var high := mini(low + 1, last)
    return lerpf(samples[low], samples[high], position - float(low))

## **THE FOOD PEAK, DERIVED FROM THE CURVE ITSELF** — the sampled stock the source regrows fastest at,
## as a fraction of K. It is deliberately NOT `FLOOR_FOOD_PEAK` restated beside the samples: one
## number derived two ways is how the chart's mark and the chart's curve start disagreeing the first
## time either moves. Answers the food-peak constant only when there is no curve to read.
static func growth_peak_fraction(samples: PackedFloat32Array) -> float:
    if not has_growth_curve(samples):
        return FLOOR_FOOD_PEAK
    var last := samples.size() - 1
    var best := 0
    for i in range(samples.size()):
        if samples[i] > samples[best]:
            best = i
    return float(best) / float(last)

## The largest regrowth anywhere in `low..high` (fractions of K) — the growth a crew has to out-carry
## to draw the stock across that band at all. Read off the samples inside the band plus its two
## interpolated endpoints, so a band narrower than one sample spacing still has an answer.
static func peak_regrowth_between(samples: PackedFloat32Array, low: float, high: float) -> float:
    if not has_growth_curve(samples):
        return 0.0
    var lo := clampf(minf(low, high), 0.0, 1.0)
    var hi := clampf(maxf(low, high), 0.0, 1.0)
    var peak := maxf(regrowth_at(samples, lo), regrowth_at(samples, hi))
    var last := samples.size() - 1
    for i in range(samples.size()):
        var fraction := float(i) / float(last)
        if fraction > lo and fraction < hi:
            peak = maxf(peak, samples[i])
    return peak

## **THE PHASE LADDER AS DRAWABLE ZONES** (§7.3) — the source's own cut points, bottom-up, as
## `[{low, high, phase}]` in fractions of `K`: `B/K < collapse` is Collapsing, `< stressed` is
## Stressed, the rest is Thriving. It is a re-statement of the wire's two fractions in the chart's
## coordinates and nothing more — the CLASSIFICATION still belongs to the sim, which publishes the
## word a source currently wears; this only says where that word would change hands.
##
## Empty when the ladder is not a ladder (either fraction absent — the `0` default — or out of order),
## because a half-published pair would paint a zone whose edge is not a threshold. The three phase
## words are the sim's own, so the caller tints a zone through the same `ecology_tier_color` the
## standing-stock band and the roster dot already use.
##
## On the ANIMAL web the first boundary is also the Allee point — the stock `regrowth_samples` turns
## negative at — so the zone edge and the curve's sign change are two views of one cliff. They come
## from one config field in the sim; if a render ever shows them apart, the disagreement is real.
static func phase_zones(src: Dictionary, prefix: String) -> Array[Dictionary]:
    var collapse := clampf(float(src.get(prefix + FORECAST_COLLAPSE_FRACTION_KEY, 0.0)), 0.0, 1.0)
    var stressed := clampf(float(src.get(prefix + FORECAST_STRESSED_FRACTION_KEY, 0.0)), 0.0, 1.0)
    if collapse <= 0.0 or stressed <= collapse:
        return []
    return [
        {"low": 0.0, "high": collapse, "phase": ECOLOGY_PHASE_COLLAPSING},
        {"low": collapse, "high": stressed, "phase": ECOLOGY_PHASE_STRESSED},
        {"low": stressed, "high": 1.0, "phase": ECOLOGY_PHASE_THRIVING},
    ]

## The learning/build multiplier this floor buys — the sim's `intensification::learn_multiplier`,
## `floor / the food peak`, normalised so the peak is ×1.0. It is what the chart's gradient rail
## encodes, and it is a fact about **these people on this ground**, not a faction knowledge meter: a
## tile knows nothing.
static func learn_multiplier(floor: float) -> float:
    return clamp_floor(floor) / FLOOR_FOOD_PEAK

## **THE WHOLE-ANIMAL HAUL CREW**, mirroring the sim's `fauna::hunt_haul_workers` exactly: the worst
## turn drops `floor(ceiling / body) + 1` whole bodies (the kill-credit bank can hold just under one
## when the turn's rate lands), and every one of them has to be carried. `ceiling`, `body` and
## `per_worker` must be in the SAME units — biomass for the crew targets, the paid account for
## `max_useful_workers`, since an animal count is a ratio and a ratio is unit-free.
static func haul_workers(ceiling: float, body: float, per_worker: float) -> int:
    if body <= 0.0 or not can_price_crew(per_worker):
        return 0
    return ceili(float(peak_animal_drop(ceiling, body)) * body / per_worker)

## **THE MOST WHOLE ANIMALS A `ceiling` CAN DROP IN ONE TURN** — `floor(ceiling / body) + 1`, the
## client mirror of the sim's `fauna::peak_animal_drop`, and shared by BOTH crew terms below for the
## reason the sim shares it: two terms sized against different drops can never be reconciled.
## `body` is assumed positive — every caller checks it first.
static func peak_animal_drop(ceiling: float, body: float) -> int:
    return floori(maxf(ceiling, 0.0) / body) + HUNT_PEAK_DROP_BANK_BONUS

## **THE WHOLE-ANIMAL ENGAGEMENT CREW**, mirroring the sim's `fauna::hunt_engage_workers`: how many
## hunters it takes to bring the peak animal drop DOWN in one turn. It is the inverse of the pair
## `animals_engaged` → `animals_stayed` — those floor `workers × engage_rate × dip` and then cut it by
## the retreat, so the crew that lands `n` animals is `ceil(n / (engage_rate × dip × stay))`.
##
## **THE RETREAT PRICES THE CREW AS WELL AS THE TAKE**, because a party that keeps one animal in four
## needs four times the hands to draw the same stock down. Sizing on the RAW reach put this crew — and
## through `take_workers` the stepper's own cap — BELOW the *clear it now* pill beside it, which has
## divided by `engagement_carry`'s retreat-aware reach all along: 82 against 108 on a played Wild Boar
## herd, the sheet naming a crew the panel then refused to let the player assign. 108 is the honest
## number.
##
## **`stay <= STAY_FRACTION_ALL_BREAK_OFF` ANSWERS `0`, and that is not the degenerate reading it
## looks like.** Nothing the party reaches ever stands, so the take is identically zero at every crew
## size — there is no number of hands that achieves it, and the crew NEEDED to achieve the take is
## therefore none. The `max()` in `take_workers` then keeps the haul crew, exactly as it does for a
## source with no engagement stage at all.
##
## **IT CANNOT BE FOLDED INTO `haul_workers`, because the two scale on DIFFERENT UNITS** — hauling is
## per biomass (one hauler carries 40), engaging is per animal (one hunter reaches 10 fowl or 0.05
## mammoths) — so neither dominates across the roster. A Wild Fowl herd with ~470 head above its floor
## is ~61 biomass: **two** haulers clear it and **47** hunters are needed to reach it, so a cap sized
## on carry alone said "more hands would be idle" about the very hands the take was short of. The
## mammoth inverts it (one hunter reaches the peak drop; twenty are needed to carry it home).
##
## **THE DIP RIDES THIS CREW TOO** (`docs/plan_harvest_floor.md` §3.1) — hands gentling a herd are
## hands not stalking it, so it takes proportionally more of them to corner the same drop.
##
## `0` where the term has nothing to say: a source with no engagement stage (`NO_ENGAGEMENT_STAGE` —
## a pen, the plant web) or a degenerate body/dip. The `max()` then keeps the haul crew and neither
## web regresses. Units on `ceiling`/`body` are free, exactly as they are for `haul_workers`.
static func engage_workers(ceiling: float, body: float, engage_rate: float, dip: float,
        stay: float) -> int:
    if body <= 0.0:
        return 0
    var reach := engagement_per_worker(engage_rate, dip)
    if is_inf(reach):
        return 0
    var landed := animals_stayed(reach, stay)
    if landed <= STAY_FRACTION_ALL_BREAK_OFF:
        return 0
    return ceili(float(peak_animal_drop(ceiling, body)) / landed)

## **THE ANIMALS ONE WORKER BRINGS INTO CONTACT PER TURN** — `engageRate × dip`, and the ONE
## composition of that pair every engagement quotient in this file divides by. It exists so the three
## of them (the engagement crew above, the engagement carry below, and through it every crew target)
## cannot be written against three spellings of one product.
##
## `ENGAGEMENT_UNBOUNDED` where there is no engagement stage at all — a pen, the whole plant web, a dip
## of zero — so a caller drops the term with a `min()` or an `is_inf` rather than a per-site branch on
## `NO_ENGAGEMENT_STAGE`. **The dip rides it** (`docs/plan_harvest_floor.md` §3.1): hands gentling a
## herd are hands not stalking it.
static func engagement_per_worker(engage_rate: float, dip: float) -> float:
    var reach := maxf(engage_rate, 0.0) * maxf(dip, 0.0)
    if reach <= NO_ENGAGEMENT_STAGE or is_inf(reach):
        return ENGAGEMENT_UNBOUNDED
    return reach

## **THE BIOMASS ONE WORKER BRINGS INTO CONTACT PER TURN** — `bodyMass × engageRate × dip`, the
## engagement stage's exact twin of the haul side's `perWorkerBiomass × dip`. Stating the reach in the
## room's OWN units is what lets a crew target stay one quotient: the hands that move a room in a turn
## are `room ÷ min(carry, this)`, because a take is bounded by both and the smaller one binds — which
## is the sim's `min(carryable, engaged)` read backwards.
##
## `ENGAGEMENT_UNBOUNDED` for a source with no engagement stage AND for one with no body to count
## (every forage patch), so `min(carry, …)` collapses to the carry alone and the plant web and the pens
## are byte-identical to before this arm existed. That is the regression that matters most here.
##
## **IT IS WHAT A WORKER BRINGS DOWN, SO IT CARRIES THE RETREAT** — and so, since the retreat began
## pricing crews as well as takes, does the sim-mirror `engage_workers` beside it. Every crew answer on
## this sheet now divides by the same retreat-aware reach, which is what closed the gap where *clear it
## now* named 108 hands and the stepper capped at 82. **`stay` is REQUIRED**: it has no default, so a
## call site cannot silently take the raw reach by omission, which is exactly how the two arms came to
## disagree.
static func engagement_carry(body_mass: float, engage_rate: float, dip: float,
        stay: float) -> float:
    if body_mass <= 0.0:
        return ENGAGEMENT_UNBOUNDED
    var reach := engagement_per_worker(engage_rate, dip)
    return ENGAGEMENT_UNBOUNDED if is_inf(reach) \
        else body_mass * animals_stayed(reach, stay)

## **THE TAKE-SIDE CREW FOR A WHOLE-ANIMAL SOURCE** — `max(haul, engage)`, the client mirror of the
## sim's `fauna::hunt_take_workers`, and the one place that `max` is written down. Two jobs, one crew,
## two units: reach the animals, then carry them home. `max()`, never `+` — one crew covering its
## busiest job.
##
## Units on `ceiling`/`body` are free (an animal count is a ratio), so this answers in biomass for the
## crew targets and in the paid account for the worker cap, exactly as its two halves do.
##
## `stay` is a pass-through to the engage half; the haul side knows nothing about a retreat, a body
## weighing the same whether it was easy or hard to bring down.
static func take_workers(ceiling: float, body: float, per_worker: float,
        engage_rate: float, dip: float, stay: float) -> int:
    return maxi(haul_workers(ceiling, body, per_worker),
        engage_workers(ceiling, body, engage_rate, dip, stay))

## **HOW MANY ANIMALS THIS PARTY BRINGS INTO CONTACT THIS TURN** — the client mirror of the sim's
## `fauna::animals_engaged`, and the one definition of it, so no two readings of a herd can disagree
## about how many it could reach. `floor(workers × engage_rate × dip)`, never below one for a party
## that exists (`ENGAGED_AT_LEAST`); a party of no workers engages nothing, which is a different
## statement and is why the worker test comes first.
##
## `ENGAGEMENT_UNBOUNDED` for a source with no engagement stage, so the caller's `min()` drops the arm.
static func animals_engaged(workers: int, engage_rate: float, dip: float) -> float:
    if workers <= 0:
        return 0.0
    if engage_rate <= NO_ENGAGEMENT_STAGE:
        return ENGAGEMENT_UNBOUNDED
    return maxf(floorf(float(workers) * engage_rate * maxf(dip, 0.0)), ENGAGED_AT_LEAST)

## **HOW MANY OF THE ENGAGED ANIMALS STAY TO BE FOUGHT** — the retreat, the stage between engagement
## and the fight, mirroring `fauna::animals_that_stay` at the quantile a FORECAST reads it at. The sim
## draws a binomial per animal; a forecast cannot draw, so it takes the analytic mean
## `floor(engaged) × stay`, which is `snapshot.fbs`'s own `stayers = workers × engageRate ×
## buildFraction × stayFraction` with the engagement already floored. `animals_engaged` floors, so
## nothing here does.
##
## **IT BOUNDS THE TAKE AND, THROUGH `engage_workers`, THE CREW.** A hand that keeps one animal in four
## draws a stock down a quarter as fast, so the crew that draws it down at all is four times as large —
## which is why `engage_workers` / `engagement_carry` / `take_workers` and both crew-target pills all
## divide by what STAYS. They read it through this function rather than through a retreat folded into
## `engage_rate`: the two terms are separately observable, a kit's `dispersion` moving only the second,
## and the fold makes Big-game and Trapping quote the identical hunt.
##
## An UNBOUNDED engagement passes straight through: a pen and the whole plant web have no retreat to
## take because they have no engagement stage, and iterating `INF` down by a fraction is still `INF`.
## `stay >= 1` is the exact identity the wire's own default gives a source with no retreat stage, and a
## species the roster cannot resolve reads it too.
static func animals_stayed(engaged: float, stay: float) -> float:
    if is_inf(engaged) or engaged <= 0.0 or stay >= STAY_FRACTION_NONE_BREAKS_OFF:
        return engaged
    return engaged * clampf(stay, 0.0, STAY_FRACTION_NONE_BREAKS_OFF)

## ***CLEAR IT NOW*** — the crew that takes everything standing above the floor in ONE turn:
## `room ÷ (perWorkerBiomass × dip)`, a closed form in terms already on the wire. Deliberately NOT
## rounded to whole animals: this is the number of hands, and a crew that over-carries simply finishes
## the draw. `0` when nothing stands above the floor; `NO_CREW_ANSWER` when the throughput is zero.
##
## **THE QUOTIENT ALONE IS A CREW THAT CLEARS NOTHING, AND IT IS THE MORE ATTRACTIVE PILL.** The room
## is the stock standing above the floor *now*; the source regrows before the crew takes anything
## (`project_stock` regrows, then takes), so wherever the regrowth across the traversed band exceeds
## the room, the quotient names FEWER hands than `crew_that_reaches` — and a crew that cannot out-take
## the regrowth does not clear the patch in one turn, or in any number of them. Reported from play: a
## `5 clear it now` beside a verdict saying 7 foragers would be needed, two lines apart. So the target
## is FLOORED on the reaching crew. The one-turn drain still wins wherever the room is large, which is
## the case the label was written for; a `reaching` of `NO_CREW_ANSWER` (no priceable crew, nothing
## standing above the floor) floors on nothing.
##
## **THE ROOM IS DIVIDED BY WHAT A WORKER CAN MOVE, WHICH ON A HUNT IS NOT WHAT THEY CAN CARRY.**
## `min(carry, engagement_carry)` is the sim's `min(carryable, engaged)` read backwards: a take is
## bounded by both, so the crew that clears a room in one turn is set by the SMALLER of the two.
## Reported from play on a Red Deer herd — six hunters carry sixteen deer (`6 × 40 ÷ 15`) and REACH
## six (`6 × engageRate 1`), so `6 clear it now` named a crew that needed three turns, beside a worker
## cap that had already become engagement-aware. A source with no engagement stage answers
## `ENGAGEMENT_UNBOUNDED`, so the `min` collapses to the carry and forage and pens are unmoved.
static func crew_to_clear(room: float, carry: float, reaching: int,
        body_mass: float, engage_rate: float, dip: float,
        stay: float = STAY_FRACTION_NONE_BREAKS_OFF) -> int:
    if not can_price_crew(carry):
        return NO_CREW_ANSWER
    if room <= 0.0:
        return 0
    var per_worker := minf(carry, engagement_carry(body_mass, engage_rate, dip, stay))
    return maxi(maxi(1, ceili(room / per_worker)), maxi(reaching, 0))

## ***HOLD IT AFTER*** — the crew that takes exactly what grows back at the floor, so the stock sits
## there: the interpolated regrowth at the floor over the same carry, **rounded up to one whole body
## on a whole-animal source** (`haul_workers`, the sim's own rounding rather than a second one
## invented here).
##
## `0` is a real answer with two causes, and both are worth stating: at floor `1.0` nothing is ever
## taken, and below a herd's Allee point the regrowth is NEGATIVE — there is no take that holds a
## stock which is falling on its own.
##
## **ON A WHOLE-ANIMAL SOURCE IT IS `take_workers`, NOT `haul_workers`** — the hands that hold a herd
## have to REACH the regrowth as well as carry it, and on a light-bodied quarry reaching is by far the
## larger of the two (the sim's `hunt_take_workers` `max`, one crew covering its busiest job). The
## engagement half answers 0 for a pen and for a species with no engagement stage, so the `max`
## collapses back to the haul crew and neither web moves.
static func crew_to_hold(samples: PackedFloat32Array, floor: float, carry: float,
        body_mass: float, engage_rate: float, dip: float, stay: float) -> int:
    if not can_price_crew(carry):
        return NO_CREW_ANSWER
    var growth := regrowth_at(samples, clamp_floor(floor))
    if growth <= 0.0:
        return 0
    if body_mass > 0.0:
        return take_workers(growth, body_mass, carry, engage_rate, dip, stay)
    return maxi(1, ceili(growth / carry))

## The same *hold it after* crew, resolved straight from a SOURCE — the form `forecast_inputs` carries
## so `max_useful_workers` can floor itself on it. `0` (never `NO_CREW_ANSWER`) when there is no crew
## to name, because this answer is only ever a FLOOR on a cap: a dead-season patch prices no crew, and
## a floor of "unpriceable" is a floor of none.
##
## **A RUNG-3 MANAGED SOURCE IS EXCLUDED**, on the same grounds its ceiling is: the sim never draws a
## Field or a built Pen down, so "the crew that takes what grows back" is not a question its wire
## curve answers — its cap is `production / per_worker`, and flooring that on a wild-drawdown number
## would staff a source against a projection it does not follow.
static func hold_crew(src: Dictionary, kind: String, prefix: String, floor: float,
        improvement: String) -> int:
    if source_is_managed(src, kind, prefix):
        return 0
    var dip := build_dip(src, prefix, improvement)
    var crew := crew_to_hold(regrowth_samples(src, prefix), floor,
        per_worker_biomass(src, prefix) * dip,
        float(src.get(prefix + FORECAST_BODY_MASS_KEY, 0.0)),
        float(src.get(prefix + FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE)), dip,
        float(src.get(prefix + FORECAST_STAY_FRACTION_KEY, STAY_FRACTION_NONE_BREAKS_OFF)))
    return maxi(crew, 0)

## The *reaching* crew — `crew_that_reaches` resolved straight from a SOURCE, in the form
## `forecast_inputs` carries so the worker cap can floor itself on it too. `0` (never
## `NO_CREW_ANSWER`) when there is no crew to name, for the reason `hold_crew` gives: this answer is
## only ever a FLOOR on a cap.
##
## **THE CAP HAS TO CARRY IT OR THE *CLEAR* TARGET BECOMES UNREACHABLE.** `crew_to_clear` now floors
## on this number, and §7.6's standing rule is that neither target may name a crew the stepper beside
## it refuses to reach — so the same floor has to reach `max_useful_workers`. It is also true on its
## own terms: hands between the one-turn quotient and the reaching crew do strictly more than the
## quotient's (they draw the stock further down every turn instead of settling above the floor), so
## capping there reported them useless while the verdict was naming them as the remedy.
##
## A RUNG-3 MANAGED SOURCE IS EXCLUDED, exactly as it is for the hold crew: the sim never draws a
## Field or a built Pen down, so a drawdown projection says nothing about how many hands it can use.
static func reach_crew(src: Dictionary, kind: String, prefix: String, floor: float,
        improvement: String) -> int:
    if source_is_managed(src, kind, prefix):
        return 0
    var dip := build_dip(src, prefix, improvement)
    var crew := crew_that_reaches(regrowth_samples(src, prefix),
        float(src.get(prefix + FORECAST_BIOMASS_KEY, 0.0)),
        float(src.get(prefix + FORECAST_CAPACITY_KEY, 0.0)), floor,
        per_worker_biomass(src, prefix) * dip,
        float(src.get(prefix + FORECAST_BODY_MASS_KEY, 0.0)),
        float(src.get(prefix + FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE)), dip,
        float(src.get(prefix + FORECAST_STAY_FRACTION_KEY, STAY_FRACTION_NONE_BREAKS_OFF)))
    return maxi(crew, 0)

## **THE PROJECTION** — the stock's trajectory under this crew at this floor, one turn at a time:
## regrow by the interpolated curve, then take `min(crew carry, the room above the floor)`. Returns
## `{series, reached_turn, settled_fraction}`, `series[i]` being the stock as a fraction of K after
## turn `i` (entry 0 is the stock today).
##
## **IT DOES NOT QUANTISE THE TAKE TO WHOLE ANIMALS, and that is the boundary holding.** A hunt's take
## is `floor(ceiling / bodyMass)` — not linear, so no client may re-derive it, and `SourceYield.actual`
## on a committed assignment is the sim's answer. What is drawn here is the crew's smoothed carry
## against the source's own growth: a projection, never a promise.
##
## **IT DOES BOUND THE TAKE BY WHAT THE PARTY CAN REACH** (`docs/plan_hunt_through_combat.md` §2), and
## that is not a quantisation — it is the third arm of the sim's own `min`. `engage_total` is the
## BIOMASS the crew brings into contact in a turn (`engaged_quantum(workers, bodyMass, …)`, i.e. the
## floored whole-animal count times the body), so the walk descends at the rate the sim would actually
## pay rather than at the rate the crew could carry. `ENGAGEMENT_UNBOUNDED` — the default, and what a
## pen and the whole plant web resolve to — drops the arm and leaves every existing caller's walk
## byte-identical.
static func project_stock(samples: PackedFloat32Array, biomass: float, capacity: float,
        floor: float, carry_total: float,
        engage_total: float = ENGAGEMENT_UNBOUNDED) -> Dictionary:
    var series := PackedFloat32Array()
    var reached := PROJECTION_REACHED_NONE
    if capacity <= 0.0:
        return {"series": series, "reached_turn": reached, "settled_fraction": 0.0}
    var stock := clampf(biomass, 0.0, capacity)
    var start_fraction := stock / capacity
    var floor_fraction := clamp_floor(floor)
    # What this crew can move in one turn: the smaller of what it CARRIES and what it REACHES.
    var crew_take := minf(maxf(carry_total, 0.0), maxf(engage_total, 0.0))
    series.push_back(start_fraction)
    for turn in range(PROJECTION_HORIZON_TURNS):
        # REGROW — the curve's own answer at this stock, sign and all. A negative sample is a decline
        # the crew did not cause and cannot stop.
        stock = clampf(stock + regrowth_at(samples, stock / capacity), 0.0, capacity)
        # TAKE — the escapement room, capped by what the crew can carry and reach.
        stock = maxf(0.0, stock - minf(crew_take,
            maxf(0.0, stock - floor_fraction * capacity)))
        var fraction := stock / capacity
        series.push_back(fraction)
        if reached == PROJECTION_REACHED_NONE \
                and fraction <= floor_fraction + STOCK_FRACTION_EPSILON \
                and fraction < start_fraction - STOCK_FRACTION_EPSILON:
            reached = turn + 1
    return {
        "series": series,
        "reached_turn": reached,
        "settled_fraction": series[series.size() - 1],
    }

## The smallest crew that WOULD draw this source to the floor within the horizon — the number the
## "can't draw it that low" verdict names. The closed form is exact for reaching equilibrium (a crew
## must out-carry the largest regrowth in the band it has to cross); the probe steps past it only
## cover reaching that equilibrium within the drawn horizon. `NO_CREW_ANSWER` when the throughput
## cannot be priced or no crew inside the probe reaches it.
##
## **THE CREW HAS TO OUT-TAKE THAT REGROWTH, AND ON A HUNT ITS TAKE IS BOUNDED BY REACH** — so the
## closed form divides by `min(carry, engagement_carry)` and each probe walks a projection carrying
## the same bound. Dividing by the carry alone named a crew that cannot draw the herd down at all,
## which is the number the verdict offers as the remedy and the number the *clear* target floors on.
## **THE CLOSED FORM AND THE PROBE WALKS MUST CARRY THE SAME `stay`**, or the seed lands below the
## answer and the probe spends its budget climbing back to it.
static func crew_that_reaches(samples: PackedFloat32Array, biomass: float, capacity: float,
        floor: float, carry: float, body_mass: float, engage_rate: float, dip: float,
        stay: float = STAY_FRACTION_NONE_BREAKS_OFF) -> int:
    if not can_price_crew(carry) or capacity <= 0.0:
        return NO_CREW_ANSWER
    var start_fraction := clampf(biomass / capacity, 0.0, 1.0)
    var floor_fraction := clamp_floor(floor)
    if start_fraction <= floor_fraction:
        return 0
    var peak := peak_regrowth_between(samples, floor_fraction, start_fraction)
    var per_worker := minf(carry, engagement_carry(body_mass, engage_rate, dip, stay))
    var need := maxi(1, floori(maxf(peak, 0.0) / per_worker) + 1)
    for _step in range(CREW_PROBE_STEPS):
        var walk := project_stock(samples, biomass, capacity, floor, float(need) * carry,
            engaged_quantum(need, body_mass, engage_rate, dip, stay))
        if int(walk["reached_turn"]) != PROJECTION_REACHED_NONE:
            return need
        need += 1
    return NO_CREW_ANSWER

## **IS THIS CREW ACTUALLY DRAWING THE SOURCE DOWN?** — the projection's own answer, and the gate on
## the ⚠ overdraw flag both compose sheets carry.
##
## The flag's own test is a take against the **food-peak** ceiling, which on a source standing at or
## below that peak is `take > 0` — i.e. a fact about the FLOOR, not about the stock. So a patch whose
## projection climbs could render `⚠ overdraws the patch` directly above a verdict reading *it settles
## at 53% and holds there*: the panel saying the stock falls and rises in the same breath. Reported
## from play. Nothing is being overdrawn while the stock rises, whatever the floor is — and the
## projection is the one instrument that already knows, so the two sentences are now readings of it.
##
## **`true` WHERE THERE IS NOTHING TO CONSULT**, which keeps this purely subtractive: a source with no
## capacity, no published curve, or a rung-3 managed one has no drawdown projection at all, so the
## flag is left exactly as it was rather than suppressed on the strength of a walk that was never
## taken.
##
## **IT WALKS THE ENGAGEMENT-BOUND PROJECTION, THE SAME ONE THE VERDICT IS WRITTEN OFF.** The gate and
## the sentence beneath it are two readings of ONE walk — that is the whole point of the gate — so a
## carry-only walk here would fall where the verdict's rises and put `⚠ overdraws the herd` back above
## *it settles at 84% and holds there*, in the one case the arm exists for: a party that cannot reach
## what it could carry. It is not the safe direction either, however subtractive the gate is; a flag
## kept by a projection the panel does not believe is the same contradiction the flag was gated to
## remove. A source with no engagement stage resolves to `ENGAGEMENT_UNBOUNDED` and walks exactly the
## carry-bound projection it always did.
static func take_draws_down(src: Dictionary, kind: String, prefix: String, floor: float,
        workers: int, improvement: String) -> bool:
    var capacity := float(src.get(prefix + FORECAST_CAPACITY_KEY, 0.0))
    var samples := regrowth_samples(src, prefix)
    if capacity <= 0.0 or not has_growth_curve(samples) \
            or source_is_managed(src, kind, prefix):
        return true
    var biomass := float(src.get(prefix + FORECAST_BIOMASS_KEY, 0.0))
    var dip := build_dip(src, prefix, improvement)
    var carry := per_worker_biomass(src, prefix) * dip
    var crew := maxi(workers, 0)
    var walk := project_stock(samples, biomass, capacity, clamp_floor(floor),
        float(crew) * carry,
        engaged_quantum(crew, float(src.get(prefix + FORECAST_BODY_MASS_KEY, 0.0)),
            float(src.get(prefix + FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE)), dip,
            float(src.get(prefix + FORECAST_STAY_FRACTION_KEY,
                STAY_FRACTION_NONE_BREAKS_OFF))))
    return float(walk["settled_fraction"]) \
        < clampf(biomass / capacity, 0.0, 1.0) - STOCK_FRACTION_EPSILON

# ---- THE VERDICT (docs/plan_harvest_floor.md §7.1) ----------------------------------------------
#
# **THE POINT OF THE WHOLE REDESIGN.** The four-stance picker let a player select Eradicate with one
# worker and never eradicate anything: the stance said what was intended and nothing said whether the
# crew could do it. The crew and the floor are INDEPENDENT statements now, so the panel can compare
# them and say which one is BINDING — and that sentence is the answer the sheet exists to give.
#
# Three states, three severities, in the raid verdict's own ok/slow/blocked vocabulary (a fourth,
# `blocked` with no crew at all, is the same shape and not a fourth claim).
const VERDICT_OK := "ok"
const VERDICT_SLOW := "slow"
const VERDICT_BLOCKED := "blocked"
# THE FLOOR BINDS — the crew is big enough, so the floor is what the source settles at.
const VERDICT_REACHES_FORMAT := "Reaches the floor in %d turns, then holds it — taking only what grows back."
# A crew big enough to clear the source in one turn is common (it is the `clear it now` target), so
# "1 turns" is a reading the panel would print often rather than an edge case worth tolerating.
const VERDICT_REACHES_ONE_TURN := "Reaches the floor next turn, then holds it — taking only what grows back."
# **…AND THE SAME TWO WITH NO SECOND CLAUSE, because at some floors there is nothing to hold.** A HERD
# taken to floor 0 is gone for good: nothing regrows, so "then holds it — taking only what grows back"
# promised an aftermath the sheet's own `0 hold it after` was simultaneously denying. The clause is
# dropped rather than reworded — what stripping costs is already the aside's `FLOOR_STRIP_CONSEQUENCE`
# sentence ("the herd is gone for good, for you and for everyone else"), and a verdict restating it
# would say one fact twice.
#
# **The test is the REGROWTH at this floor, not the web and not floor 0.** A patch stripped to 0
# reseeds from bare ground, so it genuinely does hold at 0 and pay what grows back — the full sentence
# is true there. Branching on "fauna at floor 0" would get that case wrong in the direction of saying
# less than is true, and would miss any other floor a source cannot grow at.
const VERDICT_REACHES_STRIPPED_FORMAT := "Reaches the floor in %d turns."
const VERDICT_REACHES_ONE_TURN_STRIPPED := "Reaches the floor next turn."
# THE CREW BINDS — the take equals the regrowth somewhere ABOVE the floor, and that is where it stops.
# The crew that WOULD reach it is named, because "add hands" is the remedy and a verdict that
# withholds the number is a puzzle rather than an answer.
const VERDICT_SETTLES_FORMAT := "This crew can't draw it that low. It settles at %d%% and holds there"
const VERDICT_SETTLES_CREW_FORMAT := " — %d %s would reach the floor."
const VERDICT_SETTLES_END := "."
# NEITHER BINDS — there is nothing above the floor to take, so the crew's size is irrelevant until the
# source grows back past it. The STOCK, not the percent, because the point is the quantity the source
# has to reach — and in whatever unit the flag above it is flying (`stock_face`), because the two name
# one threshold and a sheet reading "leave 50% · ≈11 Red Deer" over "grows past 1075" states it twice
# in two currencies.
const VERDICT_AT_FLOOR_FORMAT := "Already at or below the floor. This crew takes nothing until it grows past %s."
# No crew at all is its own reading and must not render as "reaches the floor in 0 turns".
const VERDICT_NO_CREW := "No one assigned. Nothing is taken and it grows back on its own."

# ---- THE DENIAL RAID — a MISSION, not a floor (`docs/plan_denial_raid.md`) -----------------------
# **IT CARRIES NO FLOOR AND NO RATE, WHICH IS WHY NONE OF THE RAID VOCABULARY ABOVE APPLIES TO IT.**
# A hunting raid's readout answers "what comes home, and when"; a denial party deliberately publishes
# no `expeditionProjectedDelivery` / `expeditionEtaTurns` / `expeditionTripBound` at all, because its
# goal is not a delivery — it is to push the herd BELOW `ecology.collapse_fraction`, where growth
# zeroes and the decline is irreversible, and then walk away (§1.1). So its readout is a COLLAPSE
# VERDICT, and `expeditionFloor` (`0.0`) / `expeditionFillTarget` (`0`) must never be rendered for it:
# they are the mission reporting that it has no such lever, not values it chose.
# The requirement is `DenialRaidForecastReply.party_needed`, searched CONTIGUOUSLY to the asking band's
# own last worker — so `0` says "no party YOU can field does this", a fact the player can act on,
# rather than "no party anyone happened to sample did".
# **`0` IS "NO QUOTED PARTY DRIVES THIS HERD DOWN", and it is NEVER "send nobody".** Three honest
# situations reach it (a quarry nothing can bring into contact, a requirement past the sim's quoting
# bound, a herd out-growing the whole table), all told apart by the rows' own `outcome` — so the
# client renders the verdict, never seeds the stepper here, and never invents a figure for the copy.
const DENIAL_PARTY_NEEDED_NONE := 0
# **THE SHORT-HANDED FACE — the one state in which this sheet's Send is DISABLED.** Named for the
# BAND's shortfall rather than for the raid's outcome, because that is what the player has to fix; the
# `repelled` face beside it ("Send Anyway") would read as an offer the button is refusing to honour.
const DENIAL_SHORT_HANDED_BUTTON := "Not Enough Hunters"
# …and the reason beneath it, in the sheet's own hint register, stating BOTH numbers: what the herd
# requires and what the band actually has. The stepper above it is already sitting at the second.
const DENIAL_SHORT_HANDED_REASON_FORMAT := "%s needs %d hunters and this band has only %d idle. Free up workers before this raid can break the herd."
# **`0` MEANS "NOT WITHIN THE HORIZON" ON THAT END, never "immediately".** `Low` is the FEWEST turns
# — the optimistic draw, where more animals stay and more strikes land — so a positive `low` beside a
# `0` `high` reads "only on a good run".
const DENIAL_TURNS_BEYOND_HORIZON := 0
# The sim's own `DenialOutcome` keys. `repelled` and `horizon` are the pair this arc insists on, and
# they are NOT interchangeable: `repelled` is a verdict about the PARTY (its kills do not outpace the
# herd's regrowth, so no amount of waiting gets there), `horizon` is a verdict about the CLOCK (the
# projection ran out). Rendering one for the other blames the herd for the party's problem, which
# this arc has already shipped twice.
const DENIAL_OUTCOME_PAST_RECOVERY := "past_recovery"
const DENIAL_OUTCOME_HERD_LOST := "herd_lost"
const DENIAL_OUTCOME_REPELLED := "repelled"
const DENIAL_OUTCOME_HORIZON := "horizon"
# The unattributed key — an estimate row that carries no outcome at all. It names NEITHER side, for
# the reason `HUNT_EMPTY_REFUSALS`' own unattributed entry does: guessing is the defect.
const DENIAL_OUTCOME_NONE := ""
# **THE TWO OUTCOMES IN WHICH THE RAID ACTUALLY WORKED** — the herd goes under the Allee threshold
# and cannot come back, or falls past its extinction floor and despawns. Everything else is a raid
# that did NOT get there, and the reason to name the set rather than to spell "not repelled" at each
# reader is that those are DIFFERENT SETS: **`horizon` is not `repelled` and it is not a success
# either.** A horizon row means the projection ran its whole length with the herd still standing —
# the party may well get there after it, and it may not — so quoting one as the party that breaks
# this herd promises an outcome the sim declined to state. That is exactly the defect this constant
# exists to make unexpressible; it shipped as "the first row that is not `repelled`" and opened a
# Wild Aurochs sheet on a party of 5 under the verdict *"still standing when the forecast runs out"*.
#
# **It is the same set the table below marks `VERDICT_OK`**, and must stay so: severity there is the
# raid's verdict and not a tint choice — `denial_outcome_succeeds` and the Send button's primary face
# are two readings of one question. `band_panel_preview` asserts the two agree entry by entry.
const DENIAL_SUCCESS_OUTCOMES := [DENIAL_OUTCOME_PAST_RECOVERY, DENIAL_OUTCOME_HERD_LOST]

# **THE TURN COUNT IS AN ESTIMATE, AND EVERY FORM OF IT WEARS `≈`.** `turns_to_collapse` is an
# integral over many stochastic retreat draws, so a lucky run really can finish sooner than the
# reported low (measured: a seeded raid landed on turn 7 against a reported low of 8). The band is a
# claim about the EXPECTATION, not a promise per run — hence `≈` on both ends and the caveat below.
# **THE LEAD FIGURE, AND IT IS THE EXPECTATION WHEREVER THE SIM BOUNDED ONE.** Every other number on
# this sheet — the kill count, the food hauled, the waste left on the range — is priced at
# `turns_to_collapse`, so a sentence leading with any other draw describes a different raid from the
# take line beneath it. Reported from play: a Red Deer raid read *"≈12 turns on a good run"* over a
# take of 180 kills, which is the forty-seven-turn expectation's take. The old rule dropped the
# expectation entirely whenever `high` was unbounded — the one number that matched the rest of the
# sheet was the one never printed.
const DENIAL_TURNS_ONE_FORMAT := "≈%d turns"
# **WHICH SPAN THE FIGURE IS IN, and neither surface may leave it to be inferred** (reported from play).
# The collapse table counts turns spent WORKING the herd; the party still has to walk there, and the
# hunt readout on the same sheet has always added its round trip. A pre-launch verdict states the total
# FROM LAUNCH; an in-flight one quotes the table's own raiding turns, a launched party's remaining walk
# not being knowable from the drawer's inputs.
const DENIAL_SPAN_FROM_LAUNCH := " from launch"
const DENIAL_SPAN_OF_RAIDING := " of raiding"
# The clause the lead figure rides in, appended to the outcome sentence rather than baked into each
# entry's format: an outcome that quotes no turns must still render its outcome (below).
const DENIAL_TURNS_LEAD_FORMAT := " in %s%s"
# **THE EXPECTATION ITSELF RAN PAST THE HORIZON, so only luck gets there at all.** This is the one
# place "on a good run" is the right words — and it must still say outright that the raid is not
# expected to finish, or a lone optimistic number reads as the answer.
const DENIAL_ONLY_GOOD_RUN_LEAD_FORMAT := " only on a good run — %s%s"
const DENIAL_SPREAD_NOT_EXPECTED := ", and the raid is not expected to finish inside the forecast"
# **THE SPREAD, STATED AFTER THE EXPECTATION RATHER THAN INSTEAD OF IT.** The ordinary case names both
# ends; the reported case names the good end and says the bad one is unbounded. An unbounded end is
# always SAID to be unbounded — silently dropping it is what let a lucky-run figure stand alone.
const DENIAL_SPREAD_RANGE_FORMAT := " — between %d and %d depending on the run"
const DENIAL_SPREAD_OPEN_HIGH_FORMAT := " — as few as %d on a good run, and a bad one may not finish"
# …and where there IS travel folded into that total, how much of it is the walk — the hunt line's
# `(7 hunting + 3 travel)` split, in the one term a denial total actually adds. Rendered only when
# there is travel to split off, exactly as the hunt breakdown is.
const DENIAL_TRAVEL_SPLIT_FORMAT := " (%d of them travel)"
# The forecast's own travel key, and the sentinel for "no band was supplied, so this forecast states
# the AT-THE-HERD span". A real leg is never negative, so `-1` reads unambiguously as absent — the
# `HUNT_RATE_UNAVAILABLE` idiom. It is NOT `0`: a band standing on its quarry has a real zero-turn
# walk and must still read "from launch".
const DENIAL_TRAVEL_KEY := "travel"
const DENIAL_TRAVEL_UNKNOWN := -1
# **HOW LONG THE FORECAST THAT "RAN OUT" ACTUALLY IS** — `expeditionForecastHorizonTurns`, carried onto
# the forecast so the `horizon` verdict can say it. `FORECAST_HORIZON_UNKNOWN` (`0`) when the caller had
# no cohort carrying the lever, in which case the verdict keeps its hedge.
const DENIAL_HORIZON_TURNS_KEY := "horizon_turns"
# The caveat, in the panel's own hint register. It is what keeps the band from reading as a guarantee.
const DENIAL_ESTIMATE_CAVEAT := "An estimate over many raids — the fight is chancy, so a lucky run finishes sooner."

# **THE VERDICT TABLE — one entry per outcome, all four faces of it in ONE place** (the
# `HUNT_EMPTY_REFUSALS` idiom, and for the same reason: the line, the button and the spelled-out
# reason are three views of one answer, and three lookups are free to disagree).
#
# `turns` is whether THIS outcome has a turn count to quote at all. A `repelled` party never gets
# there, so quoting a number would be a promise the sim did not make; the outcome word is the whole
# answer. **That is also the structural guarantee behind "never render a blank turn count without its
# outcome"** — the line IS the outcome, and the turn clause is only ever appended to it.
const DENIAL_VERDICTS := {
    # It works: the herd goes under the Allee threshold and cannot come back. The mission's own
    # success condition, so the send is the plain primary one.
    DENIAL_OUTCOME_PAST_RECOVERY: {
        "line": "%s past recovery",
        "turns": true,
        "button": "Send Denial Raid",
        "severity": VERDICT_OK,
        "reason": "",
    },
    # It works HARDER than asked: the herd falls past its extinction floor and despawns entirely. Not
    # a failure — a bigger version of the same success, and the copy must not read as a warning.
    DENIAL_OUTCOME_HERD_LOST: {
        "line": "%s wiped out",
        "turns": true,
        "button": "Send Denial Raid",
        "severity": VERDICT_OK,
        "reason": "",
    },
    # **A VERDICT ABOUT THE PARTY.** Its kills per turn sit at or below the herd's own regrowth, so
    # the raid never gets there however long it works — the remedy is HANDS, and the herd is not the
    # thing to fix. It still LAUNCHES (a raid that cannot get there keeps working the herd until it
    # is recalled, §6 Q2), so this warns and never blocks.
    # **TWO REASONS, AND WHICH ONE RENDERS IS A FACT ABOUT THE SIM'S ANSWER, NOT ABOUT THE WORDING.**
    # "Send more hunters" is correct on the merits and useless in hand: it prescribes hands without
    # naming how many, while the reply's `party_needed` states the exact figure. So where there IS a
    # number, `reason_counted` names it — `%s` the quarry, `%d` the party
    # — and where there is not (`DENIAL_PARTY_NEEDED_NONE`), the bare `reason` stands verbatim,
    # because inventing a figure there would be a promise the sim did not make.
    DENIAL_OUTCOME_REPELLED: {
        "line": "%s breeds back faster than this party kills — it is never pushed past recovery",
        "turns": false,
        "button": "Send Anyway (never collapses)",
        "severity": VERDICT_SLOW,
        "reason": "This party's kills do not outpace %s's regrowth. Send more hunters — the herd is not the problem.",
        "reason_counted": "This party's kills do not outpace %s's regrowth. It takes %d hunters to push this herd past recovery — the herd is not the problem.",
    },
    # **A VERDICT ABOUT THE CLOCK.** The projection ran its whole length; the party may well get there
    # after it. Deliberately worded so it cannot be mistaken for the party being outmatched.
    #
    # **`line_bounded` SAYS HOW LONG THAT LENGTH IS, IN THIS SHEET'S OWN SPAN.** "When the forecast runs
    # out" names a clock the player cannot see, so where the horizon is on the wire the sentence quotes
    # it — shifted onto the launch clock by the same outbound walk `denial_turns_clause` shifts its
    # figures by, and closed by the same `from launch` / `of raiding` words, or the two spans on one
    # sheet would mean different things. `%s` quarry, `%d` turns, `%s` span. The bare `line` stands where
    # no cohort carried the lever: a hedge beats a number that is wrong in the reassuring direction.
    DENIAL_OUTCOME_HORIZON: {
        "line": "%s is still standing when the forecast runs out",
        "line_bounded": "%s is still standing after %d turns%s",
        "turns": false,
        "button": "Send Anyway (no collapse in sight)",
        "severity": VERDICT_SLOW,
        "reason": "The forecast ran its whole length without %s going past recovery. A bigger party gets there sooner.",
    },
    DENIAL_OUTCOME_NONE: {
        "line": "%s — the forecast does not say whether this raid breaks the herd",
        "turns": false,
        "button": "Send Denial Raid",
        "severity": VERDICT_SLOW,
        "reason": "This raid on %s has no stated outcome, so the forecast names neither the herd nor the party.",
    },
}

# **THE WASTE READOUT — stated, never hidden, and never dressed as a warning** (§3). On a hunt
# `wasted` is the occasional overflow of an animal too big to haul and wears `HUNT_WASTE_NOTE_FORMAT`'s
# `⚠`; on a raid it is essentially the whole take, and it is the POINT of the mission. So it is a
# quiet factual line — what the party kills, the little it hauls home, and what it leaves standing
# dead on the range — in the aside's own ink rather than amber.
#
# **THE WASTE WAS A PAIR AND IS ONE FIGURE AGAIN** (arc #527). Its second half was the trade goods a
# kill wasted beside its meat; that account is retired and the materials replacing it carry no raid
# figure, so the line states the FOOD left standing dead on the range and nothing else.
const DENIAL_TAKE_KILLS_FORMAT := "kills ≈%d %s"
const DENIAL_TAKE_FOOD_FORMAT := " · brings home %s food"
const DENIAL_TAKE_LEFT_FORMAT := " · leaves %s on the range"
# §7.2 — WORKERS ABOVE THE HOLD NUMBER ARE STILL NEVER RELEASED. At-the-floor is the most reversible
# condition in the model (drop the floor, or let the season move the hold number, and they are wanted
# again), and this repo only rewrites an assignment for PERMANENT conditions. What changed is that the
# panel no longer NARRATES it: `4 of your 6 foragers go idle once it is holding — only 2 can carry
# what grows back` was arithmetic over two numbers already on screen a centimetre above it, the
# stepper's count and the `hold it after` pill's. The pill is also a BUTTON — clicking it sets the
# count — so the remedy was never a sentence away.
# THE ASIDE'S SECOND LINE — the teaching RATE, which is what `learn_multiplier` buys and the chart's
# gradient rail only gestures at. Cyan (a live state) whenever the crew is actually taking something
# at a floor above zero; otherwise it names WHICH of the sim's two non-degeneracy ends the player is
# standing on (`core_sim/src/intensification.rs` → "BOTH ENDS ARE NON-DEGENERATE").
const TEACHING_RATE_FORMAT := "Teaching %s at ×%.2f"
# The tail is the fact worth stating BESIDE the number, and which fact that is depends on whether a
# build is in flight: since slice 3 the same multiplier paces the build meter and the lesson, so a
# builder is told the two move together rather than being told again to raise the floor.
const TEACHING_RATE_FLOOR_TAIL := " — a higher floor teaches faster."
const TEACHING_RATE_BUILD_TAIL := " and building at the same rate."
# …and the same sentence with its TEACHING half dropped, for a lesson the faction has already
# finished learning. One multiplier paces the lesson and the build meter alike, so when the lesson is
# known the build is the whole of what the top of the dial still buys — and the line that went on
# saying "Teaching cultivation at ×1.00" long after the player learned Cultivation was reported from
# play. Same decimals and the same "a higher floor …" shape as the two tails above, because it is the
# same fact about the same number.
const TEACHING_BUILD_ONLY_FORMAT := "Building at ×%.2f — a higher floor builds faster."
# `floor = 0` — the multiplier itself is zero: stripping teaches nothing.
const TEACHING_NOTHING_STRIPPED := "Teaching nothing: nothing is left standing."
# …and the other end: the escapement room is empty (or nobody is assigned), so the sim's work
# predicate is false and no practice happens at any multiplier. Watching teaches nothing.
const TEACHING_NOTHING_UNWORKED := "Teaching nothing: nothing is being taken."

## **WHAT WORKING A SOURCE AT ITS STANDING RUNG TEACHES** — the client half of the sim's per-rung
## `earns_knowledge` (`core_sim/src/data/intensification_ladder.json`, applied by
## `systems::labor::credit_rung_lesson`). Keyed by the highest rung the source has BUILT, never by the
## verb in flight: the same crew learns Herding on a wild herd and Penning on a tamed one, so a herd
## mid-Corral is still teaching Penning while it builds.
##
## **The rung-3 entries are deliberately absent, and not because the sim has none** (a pen earns
## Foddering): a Field and a built Pen are MANAGED, so the sheet draws no floor axis there at all and
## has no rate to state — see `floor_chart_model`'s `known`.
##
## The value is the lesson's own word in the sentence's lower case; the husbandry ceiling does NOT
## suppress it, matching the sim, where the credit is read off the rung alone — hunting a wolf that
## can never be tamed still teaches Herding.
const RUNG_LESSONS := {
    SOURCE_KIND_FORAGE: {
        IMPROVEMENT_NONE: "cultivation",
        IMPROVEMENT_CULTIVATE: "seed selection",
    },
    SOURCE_KIND_HERD: {
        IMPROVEMENT_NONE: "herding",
        IMPROVEMENT_TAME: "penning",
    },
}

## The lesson this source's STANDING rung teaches, or "" when its rung declares none. The standing
## rung is the highest one actually BUILT (highest first, for the reason the improvement control's
## DONE branch tests that way: a Field is also cultivated, a penned herd also fully tamed).
static func rung_lesson(kind: String, src: Dictionary, prefix: String) -> String:
    var lessons: Dictionary = RUNG_LESSONS.get(kind, {})
    if lessons.is_empty():
        return ""
    var idx := _standing_rung_index(kind, src, prefix)
    var standing := IMPROVEMENT_NONE if idx < 0 else String(_improvement_ladder(kind)[idx])
    return String(lessons.get(standing, ""))

## **DOES THE FACTION ALREADY KNOW THE LESSON THIS SOURCE TEACHES?** — the test that stops the aside
## teaching a craft the player finished learning twenty turns ago (reported from play: a wild patch
## read `Teaching cultivation at ×1.00` forever, because `RUNG_LESSONS` keys off the source's rung and
## nothing else).
##
## **THE TRACK IS RESOLVED FROM THE NEXT RUNG UP, NOT STORED BESIDE THE WORD**, and that is the whole
## reason this is a function rather than a second column in `RUNG_LESSONS`: the lesson a standing rung
## teaches IS the knowledge that gates the rung above it (a wild patch teaches `cultivation`, which is
## what gates Cultivate), and `RungGates.RUNG_KNOWLEDGE_TRACKS` already writes that mapping down. A
## per-lesson track key here would be a second spelling of it, free to drift the first time a rung's
## knowledge is renamed. `knowledge` is the faction's `{track: progress}` row, threaded in — this
## layer holds no snapshot and must never reach for one.
static func rung_lesson_known(kind: String, src: Dictionary, prefix: String,
        knowledge: Dictionary) -> bool:
    var ladder := _improvement_ladder(kind)
    var next_idx := _standing_rung_index(kind, src, prefix) + 1
    if next_idx >= ladder.size():
        return false
    return not RungGates.knowledge_gate_unmet(String(ladder[next_idx]), knowledge)

## The index in this web's ladder of the highest rung the source has BUILT — `-1` for a source still
## standing on wild ground, which is `IMPROVEMENT_NONE`'s row in every rung-keyed table.
static func _standing_rung_index(kind: String, src: Dictionary, prefix: String) -> int:
    var ladder := _improvement_ladder(kind)
    for i in range(ladder.size() - 1, -1, -1):
        if improvement_is_done(src, prefix, String(ladder[i])):
            return i
    return -1

static func _improvement_ladder(kind: String) -> Array:
    return FORAGE_IMPROVEMENTS if kind == SOURCE_KIND_FORAGE else HUNT_IMPROVEMENTS

## **IS THIS CREW TAKING ANYTHING?** — the client's reading of the sim's work predicate
## (`systems::labor::crew_is_working_the_source`, `standing_above_floor > 0`), plus the crew term the
## sim gets for free (an assignment always has workers; a compose sheet dialled to 0 does not).
## `STOCK_FRACTION_EPSILON` is the same display tolerance the verdict's at-the-floor branch uses, so
## the two sentences can never disagree about whether the source is being worked.
static func crew_is_taking(workers: int, biomass: float, capacity: float, floor: float) -> bool:
    return workers > 0 \
        and biomass > clamp_floor(floor) * capacity + STOCK_FRACTION_EPSILON * capacity

## The aside's teaching line as `{text, teaching}` — `teaching` is whether the source is actually
## being taught at a rate, which is what earns the line SIGNAL cyan. `{}` when this rung teaches
## nothing at all, which is the caller's cue to render no line rather than an empty one.
##
## **THE LINE DOES TWO JOBS AND ONLY ONE OF THEM DIES WITH THE LESSON.** `Teaching cultivation at
## ×1.00 and building at the same rate` is a claim about the CRAFT and a claim about the BUILD METER,
## paced by one multiplier — so once the faction knows the craft the first half is false and the
## second is as true as ever. A known lesson therefore keeps the BUILDING sentence while a build is in
## flight and renders NO LINE at all when there is none, rather than going on teaching a craft the
## player has finished (`lesson_known`, resolved by `rung_lesson_known` from the faction's own
## tracks). The `TEACHING_NOTHING_*` ends below are UNLEARNED-only for the same reason: they name why
## no lesson is being earned, which is not a question for someone who already has it.
static func teaching_note(lesson: String, floor: float, taking: bool,
        building: bool, lesson_known: bool) -> Dictionary:
    if lesson == "":
        return {}
    if lesson_known:
        # **THE BUILD HALF IS GATED ON THE SAME WORK PREDICATE THE LESSON IS**, and that is a fact
        # about the sim, not a display nicety: build accrual and knowledge accrual are paced by the
        # one `learn_multiplier` and gated by the one `crew_is_working_the_source`, so a crew taking
        # nothing is building nothing. Without this the line reads `Building at ×1.00` beside a
        # verdict saying no one is assigned — the multiplier is a function of the FLOOR alone, so it
        # happily reads 1.00 at the food peak with an empty crew. (At a STRIPPED floor it would read
        # 0.00 and merely look odd; the unworked case is the one that states a falsehood.)
        if not building or not taking:
            return {}
        return {
            "text": TEACHING_BUILD_ONLY_FORMAT % learn_multiplier(floor),
            "teaching": true,
        }
    # "The floor is at zero" is `floor_zone`'s own STRIP test, not a second spelling of it — the same
    # answer the 💀 glyph and the strip hint are keyed off, so the aside cannot disagree with them.
    var stripped := floor_zone(floor) == FLOOR_ZONE_STRIP
    if stripped or not taking:
        return {
            "text": TEACHING_NOTHING_STRIPPED if stripped else TEACHING_NOTHING_UNWORKED,
            "teaching": false,
        }
    return {
        "text": (TEACHING_RATE_FORMAT % [lesson, learn_multiplier(floor)]) \
            + (TEACHING_RATE_BUILD_TAIL if building else TEACHING_RATE_FLOOR_TAIL),
        "teaching": true,
    }

## The verdict for a crew at a floor, as `{severity, text}`. `crew_noun` is the sheet's own word for
## these workers (foragers / hunters / herders), lower-cased by the caller that owns it.
##
## `regrows` is whether this floor is one the source can grow AT — false for a herd taken to 0, which
## is gone rather than held. It defaults TRUE so a caller that has no curve to ask keeps the sentence
## it had; the one caller that composes a projection resolves it from the same samples the projection
## walks. See `VERDICT_REACHES_STRIPPED_FORMAT`.
static func harvest_verdict(walk: Dictionary, workers: int, biomass: float, capacity: float,
        floor: float, reaching_crew: int, crew_noun: String,
        body_mass: float = 0.0, quarry: String = "", regrows: bool = true) -> Dictionary:
    if workers <= 0:
        return {"severity": VERDICT_BLOCKED, "text": VERDICT_NO_CREW}
    var floor_stock := clamp_floor(floor) * capacity
    if not crew_is_taking(workers, biomass, capacity, floor):
        return {
            "severity": VERDICT_BLOCKED,
            "text": VERDICT_AT_FLOOR_FORMAT % stock_face(floor_stock, body_mass, quarry),
        }
    var reached := int(walk.get("reached_turn", PROJECTION_REACHED_NONE))
    if reached != PROJECTION_REACHED_NONE:
        var one_turn := VERDICT_REACHES_ONE_TURN if regrows else VERDICT_REACHES_ONE_TURN_STRIPPED
        var many := VERDICT_REACHES_FORMAT if regrows else VERDICT_REACHES_STRIPPED_FORMAT
        return {
            "severity": VERDICT_OK,
            "text": one_turn if reached == 1 else many % reached,
        }
    var settled := int(round(float(walk.get("settled_fraction", 0.0)) * FLOOR_PERCENT_SCALE))
    var text := VERDICT_SETTLES_FORMAT % settled
    text += (VERDICT_SETTLES_CREW_FORMAT % [reaching_crew, crew_noun]) \
        if reaching_crew > 0 else VERDICT_SETTLES_END
    return {"severity": VERDICT_SLOW, "text": text}

## **THE WHOLE INSTRUMENT, COMPOSED ONCE** — everything the chart draws, the two crew targets, the
## verdict and the idle-crew note, from one forecast and one walk of the projection. It exists so the
## widget, the targets and the sentence beneath them can never disagree about the same curve: they are
## three readings of ONE model, which is exactly what §7.3 merged the stock bar and the projection to
## achieve.
##
## `known` is false — and the caller renders NO chart — for a source with no capacity (the floor axis
## does nothing there), one the wire published no curve for, and a RUNG-3 MANAGED source, whose stock
## the sim never draws down: composing an escapement projection on a Field would draw a decline that
## cannot happen.
## `lesson_known` is the faction's answer to "have we already learned what this source teaches?"
## (`rung_lesson_known`), and it is a PARAMETER because this layer is all-`static` and holds no
## snapshot: knowledge belongs to the faction, not to the source, so nothing here can look it up.
static func floor_chart_model(src: Dictionary, kind: String, prefix: String, floor: float,
        workers: int, improvement: String, crew_noun: String,
        lesson_known: bool) -> Dictionary:
    var capacity := float(src.get(prefix + FORECAST_CAPACITY_KEY, 0.0))
    var biomass := float(src.get(prefix + FORECAST_BIOMASS_KEY, 0.0))
    var samples := regrowth_samples(src, prefix)
    var known: bool = capacity > 0.0 and has_growth_curve(samples) \
        and not source_is_managed(src, kind, prefix)
    var floor_value := clamp_floor(floor)
    if not known:
        return {"known": false, "floor": floor_value}
    # The dip is bound on its own rather than folded straight into the carry, because the crew row
    # STATES it (`build_dip` below) — one resolution, so the note and the numbers it explains cannot
    # come from two different reads.
    var dip := build_dip(src, prefix, improvement)
    var carry := per_worker_biomass(src, prefix) * dip
    # Bound once and passed to EVERYTHING below — the walk, all three crew targets, the verdict and
    # out on the model. The flag draws from the model and the verdict is composed here, so a second
    # read of the same keys is how the sheet ends up naming one threshold in two units
    # (see `stock_face`) — and, since `9f716262`, how the targets end up carry-only beside a worker
    # cap and a per-turn take that are not.
    var body_mass := float(src.get(prefix + FORECAST_BODY_MASS_KEY, 0.0))
    # **THE THIRD BOUND ON A HUNT TAKE** (`docs/plan_hunt_through_combat.md` §2). A patch publishes no
    # such field and a pen publishes `NO_ENGAGEMENT_STAGE`, so both read as unbounded and every number
    # composed below is exactly what it was before the arm existed.
    var engage_rate := float(src.get(prefix + FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE))
    # **THE RETREAT, BOUND ONCE BESIDE THE REACH** — the kit's own effective one, `repriced_source`
    # having already folded its `dispersion` into the source's `stay_fraction`. It is what makes a kit
    # visible on a sheet whose every other figure is an estimate-table lookup quoted at ONE kit: the
    # curve, the settle point and the verdict are composed here, from the herd's own wire terms.
    var stay := float(src.get(prefix + FORECAST_STAY_FRACTION_KEY, STAY_FRACTION_NONE_BREAKS_OFF))
    var walk := project_stock(samples, biomass, capacity, floor_value, float(workers) * carry,
        engaged_quantum(workers, body_mass, engage_rate, dip, stay))
    # **ALL THREE CREW ANSWERS CARRY THE RETREAT.** They ask different questions about different stocks
    # — hold the regrowth, clear the room this turn, draw the stock down at all — and they may disagree
    # on that account, but they may not disagree about how many animals a hand lands. `crew_to_hold`
    # was the last one sized on the RAW reach, on the grounds that it is the sim's `hunt_take_workers`
    # and the stepper cap floors on it; the consequence was a cap BELOW the *clear it now* pill on the
    # same sheet (82 against 108), naming a crew the panel then refused to let the player assign.
    var hold := crew_to_hold(samples, floor_value, carry, body_mass, engage_rate, dip, stay)
    var reaching := crew_that_reaches(samples, biomass, capacity, floor_value, carry, body_mass,
        engage_rate, dip, stay)
    var quarry := herd_display_name(src) if kind == SOURCE_KIND_HERD else ""
    return {
        "known": true,
        # **THE CREW EVERYTHING BELOW WAS COMPOSED AGAINST**, carried on the model rather than left
        # implicit at the call site. The walk, the settled fraction and the verdict are all functions
        # of it, so a sheet that composes this model BEFORE it clamps its own stepper draws a chart
        # for a crew the row beneath refuses to show — a one-frame disagreement no capture can see.
        # Carrying it makes that comparison a rendered-against-rendered assertion
        # (`HarvestFloorChart.crew()` against the stepper's `PARTY_STEPPER_COUNT_META`) instead of a
        # claim about a controller field.
        "workers": workers,
        "capacity": capacity,
        "stock_fraction": clampf(biomass / capacity, 0.0, 1.0),
        # **THE PHASE THE SOURCE REPORTS, NOT ONE DERIVED HERE.** The chart tints the standing-stock
        # band with it, so the bar's colour and the floor's position share one y-axis — which is the
        # merge §7.3 asks for.
        "phase": String(src.get(prefix + FORECAST_ECOLOGY_PHASE_KEY, "")),
        # …and the BOUNDARIES that word changes at, as horizontal zones behind everything else. Same
        # axis, same tier colours, so the player drags the floor against the bands rather than against
        # a remembered number. Empty for a source whose cuts the wire did not state.
        "phase_zones": phase_zones(src, prefix),
        "floor": floor_value,
        "samples": samples,
        "peak_fraction": growth_peak_fraction(samples),
        "series": walk["series"],
        "reached_turn": int(walk["reached_turn"]),
        "settled_fraction": float(walk["settled_fraction"]),
        # **THE QUARRY AND ITS BODY, so the flag can state the floor in ANIMALS.** A patch publishes
        # no `body_mass` and gets no quarry name, so this pair is exactly the branch that leaves a
        # FORAGE flag reading in biomass while a herd's reads in the unit the rest of its sheet uses.
        "body_mass": body_mass,
        "quarry": quarry,
        "learn_multiplier": learn_multiplier(floor_value),
        "crew_to_clear": crew_to_clear(escapement_room(src, prefix, floor_value), carry, reaching,
            body_mass, engage_rate, dip, stay),
        "crew_to_hold": hold,
        # **THE DIP THE TWO TARGETS ABOVE WERE DIVIDED BY** (§3.1), carried so the crew row can SAY
        # it. Every impossible-looking number on a building sheet follows from it — six foragers move
        # 12 biomass a turn at the rung's quarter carry, not 48 — and the only other cue is a ticked
        # box further down the sheet, which states the build without stating its price.
        "build_dip": dip,
        # `regrows` from the SAME samples the projection walks and `crew_to_hold` divides — so the
        # verdict's promise of an aftermath, the `hold it after` count and the readout's `after`
        # reading are three consequences of one number and cannot contradict each other. They did:
        # this sheet read `0 hold it after` beside "then holds it — taking only what grows back".
        "verdict": harvest_verdict(walk, workers, biomass, capacity, floor_value, reaching,
            crew_noun, body_mass, quarry, regrowth_at(samples, floor_value) > 0.0),
        # THE ASIDE'S SECOND LINE, composed HERE rather than at the render site for the same reason
        # the verdict and the idle note are: it is a function of the floor, so it has to be recomposed
        # by every live drag, and this model IS what a drag recomposes. `improvement` is the box the
        # player has ticked, so a builder's sentence follows the checkbox too.
        "teaching_note": teaching_note(rung_lesson(kind, src, prefix), floor_value,
            crew_is_taking(workers, biomass, capacity, floor_value),
            improvement != IMPROVEMENT_NONE, lesson_known),
    }

## The herd's per-worker rate, ceiling AT `floor` and one-animal quantum ON THE AXIS IT IS QUANTISED
## ON — everything the carry/cadence arithmetic divides by, resolved once so no caller picks a
## component by hand. `{per_worker, ceiling, hold_ceiling, per_animal, …}`.
##
## **THE AXIS IS PROVISIONS AND IS NO LONGER A CHOICE** (arc #527). It was provisions-or-trade — the
## sim's `ratio_axis()` rule, first component with a positive rate — because an inedible quarry's food
## quantum is honestly `0` and a food-only derivation divides by zero. The trade account is retired,
## so there is no second scalar to fall back to and `herd_yield_axis` is deleted; an inedible quarry
## now answers zeros here, and every consumer's own guard (`per_animal > 0`, `has_component`) turns
## its readouts off rather than quoting a false food rate.
##
## **`improvement` IS REQUIRED, and it is required because the default was the bug.** This used to take
## `forecast_inputs`' `IMPROVEMENT_NONE` default, so every take composed from these rates was priced
## UNDIPPED while the sim pays `workers × per_worker × build_dip` — a herd mid-Tame or mid-Corral quoted
## roughly 4× what it would be paid, and the sheet's own worker cap and chart (which DO carry the verb)
## disagreed with the take beside them. A caller that genuinely wants the undipped rates — the SUSTAIN
## reference the take is judged against — must now say `IMPROVEMENT_NONE` out loud rather than get it by
## omission. The dip rides `per_worker` ALONE (`forecast_inputs` → §3.1): `ceiling` and `per_animal` come
## back undipped either way, so a build changes what the crew CARRIES, never the room or the body it is
## quantised against.
static func herd_axis_rates(herd: Dictionary, floor: float, improvement: String) -> Dictionary:
    var forecast := forecast_inputs(herd, SOURCE_KIND_HERD, "", floor, improvement)
    return {
        "per_worker": float(forecast["axis_per_worker"]),
        "ceiling": float(forecast["axis_ceiling"]),
        # The ceiling ONCE THE HERD IS AT ITS FLOOR — one turn's regrowth, on the same axis. Carried
        # beside the room so the delivered take can be quantised against either without a second
        # composition: whole-animal quantisation must run on BOTH readings or the holding rate would
        # be a smooth number beside a bodies-per-turn one.
        "hold_ceiling": float(forecast["axis_hold_ceiling"]),
        "per_animal": float(forecast["axis_per_animal"]),
        # **THE ENGAGEMENT PAIR, so the delivered take can bound itself on the party's REACH.** The
        # quantised take (`DrawerComposeController._hunt_delivered_and_waste`) composes its own
        # `collection` rather than calling `expected_yield_account`, so the third arm has to reach it
        # here or the sheet's headline stays carry-bound while the worker cap beside it is not. It
        # bounds the whole-animal COUNT with `animals_engaged`, which is why the pair travels raw and
        # undipped: that function applies the dip, and it is the ONE mirror of the sim's arithmetic.
        "engage_rate": float(forecast["engage_rate"]),
        "dip": float(forecast["dip"]),
        # **THE RETREAT TRAVELS WITH THE PAIR, AND ONLY THE TAKE MAY SPEND IT.** The quantised take
        # bounds its animal count on what STAYS, not on what is reached — the sim's `stayers` between
        # the engagement and the fight — while every crew size on this sheet is measured on the raw
        # reach beside it. Carried here rather than re-read off the herd so the sheet's headline and
        # `expected_yield_account`'s arm cannot resolve the kit's dispersion two ways.
        "stay": float(forecast["stay"]),
    }

## Does the wire describe this source's forecast **at all**? A PRESENCE test, not a rate test — the
## distinction #426 exists to restore, and one a rate threshold structurally cannot draw.
##
## **THE PER-BIOMASS VECTOR IS THE WITNESS ON BOTH WEBS NOW**, which is what let the two branches
## collapse: a source the wire describes states what one unit of its stock is worth in at least one
## account, whatever stands on it today. The old forage witness was the per-policy row's presence
## (retired) and the old herd witness was the per-worker pair — which reads zero on a herd stripped to
## its floor, the exact state the sheet most needs to report.
##
## A MANAGED source is described by its payoff instead: a Field's stock is not what it pays from, so
## its per-biomass vector is beside the point and may honestly be anything.
static func forecast_is_known(src: Dictionary, kind: String, prefix: String) -> bool:
    if source_is_managed(src, kind, prefix):
        return true
    if zero_account_of(src, prefix) != YIELD_ACCOUNT_NONE:
        return true
    # **A MATERIAL VECTOR IS A WITNESS TOO** (arc #527 follow-up), and it is the one that makes an
    # INEDIBLE quarry a described source rather than an unknown one. `zero_account_of` cannot answer
    # this question: it names which SCALAR zero is worth printing, and a material's empty answer is
    # rendered as no row at all, so it has no zero to nominate. Without this arm a wolf reads
    # `known == false` everywhere — no floor-preset caps, no compose readout, no worker cap — which is
    # the client calling a fully-described herd undescribed because the one account it pays is not a
    # scalar.
    return not material_payoff_rows(
        src.get(prefix + FORECAST_MATERIAL_PER_BIOMASS_KEY, [])).is_empty()

## **THE ONE PLACE A BUILD'S DIP IS RESOLVED** — `<rung>BuildFraction` off the source, or
## `NO_BUILD_DIP` when nothing is in flight. It is the client twin of `LadderConfig::build_dip`, and
## every crew term in this file goes through it, so the compose sheet, the work board and the deal
## line cannot apply the dip differently (or, as they did, three of them not at all).
##
## A `<= 0` fraction is the wire saying it does not describe this rung's build on this source — a
## species that can never be penned, a rung-3 source with nothing left to build
## (`NO_BUILD_REMAINING_FRACTION`). That is NOT "the build pays zero", so it answers the identity
## rather than collapsing every take to nothing; `improvement_forecast` makes the same call the other
## way, declining to quote a deal it cannot price.
##
## **A RUNG THIS SOURCE HAS ALREADY BUILT DIPS NOTHING** (`live_improvement`), and that is the sim's
## own rule rather than a client kindness: `intensification::BuildDips::of` says *"[the identity]
## equally when the rung it names has nothing left to build — a crew standing on a finished source is
## harvesting, not preparing."* The WIRE cannot say it for rung 2 — `BuildDips::for_branch` publishes
## `Some(fraction)` for **both** rungs whatever the source has climbed, and only a rung-3 *managed*
## forecast carries `NOTHING_LEFT_TO_BUILD` — so the fraction alone is not enough and the source's own
## done flags settle it here.
static func build_dip(src: Dictionary, prefix: String, improvement: String) -> float:
    if live_improvement(src, prefix, improvement) == IMPROVEMENT_NONE:
        return NO_BUILD_DIP
    if not FORECAST_BUILD_FRACTION_KEYS.has(improvement):
        return NO_BUILD_DIP
    var fraction := float(src.get(
        prefix + String(FORECAST_BUILD_FRACTION_KEYS[improvement]), 0.0))
    return fraction if fraction > 0.0 else NO_BUILD_DIP

## PRE-COMMIT FORECAST (the compose-time counterpart to `source_yield_readout`'s post-hoc note).
## The source's per-worker throughput + the take ceiling AT `floor`, per account — all per turn at its
## CURRENT biomass, at output_multiplier 1.0. `src` is a herd dict (bare keys) or a tile_info (the
## patch's fields, `patch_`-prefixed); `known` is false for a source the wire does not describe, in
## which case callers show no row and apply no cap.
##
## **`floor` REPLACED THE STANCE STRING, and the ceiling is COMPOSED rather than looked up.** There is
## no per-stance row on either web any more (`foragePolicyCeilings` / `huntPolicyCeilings` are retired
## `(deprecated)` slots that read zero), because four rows cannot answer a continuous dial. The client
## evaluates `max(0, B − floor·K) × <account>PerBiomass` — see `escapement_room` for why that is a
## sound exception to "the sim exports the answer", and where the exception stops.
##
## **THE BUILD DIP MULTIPLIES THE CREW, NOT THE CEILING** (§3.1). It rides `per_worker*` here — and
## therefore `axis_per_worker`, hence `max_useful_workers`' divisor and `expected_yield`'s crew term —
## while every ceiling stays undipped: the source offers what stands above the floor whether the crew
## is harvesting it or building on it. `IMPROVEMENT_NONE` (the default) is the identity, so a pure
## harvest reads exactly as before, and a crew big enough to saturate the source's stock pays no dip
## at all (which is the client-visible consequence of the move: the remedy for a slow build is hands).
##
## `kind` is the caller-stated SOURCE_KIND_*; `prefix` only spells the wire keys (the two are
## independent — a herd and a raw wire patch share the empty prefix).
static func forecast_inputs(src: Dictionary, kind: String, prefix: String, floor: float,
        improvement: String = IMPROVEMENT_NONE) -> Dictionary:
    var dip := build_dip(src, prefix, improvement)
    # ---- THE CEILING, PER ACCOUNT ---------------------------------------------------------------
    # A rung-3 MANAGED source (a Field, a built Pen) is never drawn down: it pays its managed
    # production at every floor, so it reads the rung's own payoff fields instead of composing an
    # escapement room out of a stock the sim does not touch.
    var ceiling := 0.0
    var ceiling_fodder := 0.0
    # …AND THE CEILING ONCE THE SOURCE IS SITTING AT THE FLOOR, which is a DIFFERENT quantity and the
    # one the readout's `after` reading is capped by. The ceilings above are the ROOM — everything
    # standing above the floor, takeable ONCE. What a source pays every turn thereafter is what it
    # REGROWS at that floor, which is why a big crew's headline take is a burst and not a rate.
    var hold_ceiling := 0.0
    var hold_ceiling_fodder := 0.0
    # …AND THE SAME PAIR PER MATERIAL. It is a VECTOR rather than a third scalar, so it travels as
    # `[{material_id, amount}]` all the way to the readout and is never summed on the way.
    var ceiling_material: Array[Dictionary] = []
    var hold_ceiling_material: Array[Dictionary] = []
    if source_is_managed(src, kind, prefix):
        var rung := String(FORECAST_MANAGED_IMPROVEMENTS[kind])
        ceiling = float(src.get(prefix + String(FORECAST_PAYOFF_KEYS[rung]), 0.0))
        if FORECAST_PAYOFF_FODDER_KEYS.has(rung):
            ceiling_fodder = float(src.get(prefix + String(FORECAST_PAYOFF_FODDER_KEYS[rung]), 0.0))
        # **A RUNG-3 MANAGED SOURCE HAS NO BURST TO SPEND.** The sim never draws a Field or a built Pen
        # down, so its payoff IS its every-turn rate: now and after are the same number, and the
        # readout renders one reading rather than an arrow pointing at itself.
        hold_ceiling = ceiling
        hold_ceiling_fodder = ceiling_fodder
        # **A MANAGED RUNG QUOTES NO MATERIAL, and that is the wire's answer rather than a gap this
        # layer should paper over.** A rung's payoff fields are scalars; the crop picker states a
        # plant's materials per RUNG from its own `sow_material_payoff` / `cultivate_material_payoff`,
        # which is a different question asked on a different surface (`land-readouts.md`).
    else:
        # ONE composition, both webs — the terms it reads are published identically by
        # `HerdTelemetryState` and `ForagePatchState`, which is what collapsed two branches into none.
        var room := escapement_room(src, prefix, floor)
        ceiling = room * float(src.get(prefix + FORECAST_PROVISIONS_PER_BIOMASS_KEY, 0.0))
        ceiling_fodder = room * float(src.get(prefix + FORECAST_FODDER_PER_BIOMASS_KEY, 0.0))
        # The material vector through the SAME room, by the same rule — which is the whole reason
        # `material_per_biomass` is a per-biomass rate and not a pre-composed ceiling: it answers at
        # whatever floor the player has dragged to, exactly as the two scalars do.
        ceiling_material = scaled_material_rows(
            src.get(prefix + FORECAST_MATERIAL_PER_BIOMASS_KEY, []), room)
        # ONE turn's regrowth AT the floor, through the SAME per-biomass vector the room goes through
        # — which is why the accounts stay in one ratio in both readings, and why a second row of them
        # would carry one new fact in every slot. `crew_to_hold` divides this same growth by the crew's
        # carry, so the *hold it after* button and the `after` rate are two readings of one number and
        # cannot disagree.
        var growth := regrowth_at(regrowth_samples(src, prefix), clamp_floor(floor))
        hold_ceiling = growth * float(src.get(prefix + FORECAST_PROVISIONS_PER_BIOMASS_KEY, 0.0))
        hold_ceiling_fodder = growth * float(src.get(prefix + FORECAST_FODDER_PER_BIOMASS_KEY, 0.0))
        hold_ceiling_material = scaled_material_rows(
            src.get(prefix + FORECAST_MATERIAL_PER_BIOMASS_KEY, []), growth)
    # ---- THE CREW'S THROUGHPUT, PER ACCOUNT, DIPPED ---------------------------------------------
    var per_worker := float(src.get(prefix + FORECAST_PER_WORKER_KEY, 0.0))
    var per_worker_fodder := 0.0
    # A patch publishes a per-worker term for FOOD alone, so its other account is composed from the one
    # biomass throughput both share (both operands of the take's `min` are the same biomass through the
    # same rates). **THE THROUGHPUT IS NOW A WIRE FIELD** — it used to be recovered as
    # `per_worker_yield / provisions_per_biomass`, which is `0/0` on a Field of a food-less crop, and
    # that hole is what `crew_unknown` existed to paper over. A zero here is a dead season and composes
    # honest zeros, so there is nothing left to declare unknown.
    if kind == SOURCE_KIND_FORAGE:
        var carry := per_worker_biomass(src, prefix)
        per_worker_fodder = carry * float(src.get(prefix + FORECAST_FODDER_PER_BIOMASS_KEY, 0.0))
    per_worker *= dip
    per_worker_fodder *= dip
    # **THE DIP RIDES THE MATERIAL CREW TERM TOO**, for the reason it rides the other two: a crew
    # preparing a rung is not harvesting at full rate, whatever the account. Herd-side only today —
    # a patch publishes no per-worker material vector, and reads an empty one.
    var per_worker_material := scaled_material_rows(
        src.get(prefix + FORECAST_PER_WORKER_MATERIAL_KEY, []), dip)
    # WHOLE-ANIMAL HUNT: a take of whole animals (`food_per_animal` = one animal's yield in food; 0 or
    # absent for a forage patch, which harvests grain by the handful). The peak-turn carry need is
    # quantized to whole bodies (see `max_useful_workers`), so it fires ONLY for a hunt of a live,
    # un-penned herd — never a forage patch and never a corralled one, whose managed harvest has no
    # kill rhythm. A crew building a pen still takes whole animals while it does so: the dip scales
    # the crew, it does not change the rhythm.
    var food_per_animal := float(src.get(prefix + FORECAST_FOOD_PER_ANIMAL_KEY, 0.0))
    var whole_animal: bool = food_per_animal > 0.0 and not bool(src.get("corralled", false))
    return {
        "per_worker": per_worker,
        "ceiling": ceiling,
        "food_per_animal": food_per_animal,
        # THE SECOND ACCOUNT (#426) — plant-only: no animal pays fodder, so a herd reads 0 here and
        # every hunt-side answer is unchanged.
        "per_worker_fodder": per_worker_fodder,
        "ceiling_fodder": ceiling_fodder,
        # THE MATERIAL ACCOUNT, as a VECTOR (arc #527 follow-up) — what an inedible quarry is FOR,
        # and the pair a compose sheet reads instead of the assignment's resolved `material_yield`
        # (which is empty pre-commit by design; `material_rows_of` says why). `expected_materials`
        # is the clamp over these two, and it is the food side's own `min` one account further out.
        FORECAST_PER_WORKER_MATERIAL_KEY: per_worker_material,
        "material_ceiling": ceiling_material,
        "hold_material_ceiling": hold_ceiling_material,
        # The HOLD ceilings, keyed to match their room twins so `expected_yield_account` reaches either
        # by name and no second take function exists to drift from the first.
        "hold_ceiling": hold_ceiling,
        "hold_ceiling_fodder": hold_ceiling_fodder,
        # The QUANTISED-AXIS triple every divide-by-a-quantum consumer reads (`max_useful_workers` and
        # the local preview). **The axis is provisions and is no longer a choice** (arc #527): the
        # trade account it could otherwise resolve to is retired, so these are aliases of the food
        # terms above rather than a resolution — kept under their own names because they mark WHICH
        # question a consumer is asking, and an inedible quarry's zeros here are a real answer.
        "axis_per_worker": per_worker,
        "axis_ceiling": ceiling,
        "axis_hold_ceiling": hold_ceiling,
        "axis_per_animal": food_per_animal,
        "whole_animal": whole_animal,
        # The floor this forecast was composed at, carried so a caller can re-state it without holding
        # the dial itself — and so a cached forecast can never be read against a different floor.
        "floor": clamp_floor(floor),
        # WHICH ACCOUNT'S ZERO IS A FACT ABOUT THIS SOURCE (§7.7). It is read off the per-biomass rate
        # vector, not off this turn's ceilings: a herd stripped to its floor pays nothing in ANY
        # account, and the question "which account would it pay?" still has an answer.
        "zero_account": zero_account_of(src, prefix),
        # **THE CREW'S THROUGHPUT IN BIOMASS**, undipped — the term the two worker targets divide by.
        # It is carried raw rather than dipped because a target answers a question about the CREW
        # (`crew_to_clear` / `crew_to_hold` apply the dip themselves, from the same `dip` above).
        "per_worker_biomass": per_worker_biomass(src, prefix),
        "dip": dip,
        # **THE ENGAGEMENT THROUGHPUT, RAW** (`docs/plan_hunt_through_combat.md` §2) — carried
        # undipped beside the `dip` above for the reason `per_worker_biomass` is: the two consumers
        # (`expected_yield_account`'s reach arm and `max_useful_workers`' engagement crew) apply the
        # dip themselves, through `animals_engaged` / `engage_workers`, which are the client's ONE
        # mirror of the sim's arithmetic. A forage patch publishes no such field, so it reads
        # `NO_ENGAGEMENT_STAGE` and both consumers drop the term.
        "engage_rate": float(src.get(prefix + FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE)),
        # **THE RETREAT — the take's OWN term, and the ONE the kit's `dispersion` reaches the sheet
        # through.** `KitRoster.repriced_source` has already folded the kit into the source's
        # `stay_fraction` (the wire's own `clamp(1 - (1 - stay) x dispersion)`), so what arrives here is
        # this party's effective retreat rather than the species' bare one. Absent = nothing breaks off,
        # which every forage patch and every pen reads and which keeps both webs unmoved.
        #
        # **ONLY `expected_yield_account`'s reach arm may spend it.** `max_useful_workers`' engagement
        # crew reads `engage_rate` alone, deliberately — see `animals_stayed`.
        "stay": float(src.get(prefix + FORECAST_STAY_FRACTION_KEY, STAY_FRACTION_NONE_BREAKS_OFF)),
        # **THE *HOLD IT AFTER* CREW, CARRIED SO THE WORKER CAP CAN FLOOR ITSELF ON IT** (§7.2). It is
        # the same number the chart's second crew target offers — the hands that take exactly what
        # grows back at this floor — and `max_useful_workers` takes the max of it and the one-turn
        # count. See there for why the cap, not the target, was the wrong number.
        "hold_crew": hold_crew(src, kind, prefix, floor, improvement),
        # **AND THE CREW THAT REACHES THE FLOOR**, the cap's second projection-derived floor. See
        # `reach_crew`: the *clear it now* target is floored on it, and a target the stepper cannot
        # reach is the panel arguing with itself.
        "reach_crew": reach_crew(src, kind, prefix, floor, improvement),
        # **A PRESENCE test, not a rate test** (#426). It used to be `per_worker >= ε`, which conflated
        # "the wire carried no forecast" with "the rate is genuinely zero" — and its own docstring said
        # it meant the former. A zero-conversion crop makes the latter real, so the two came apart and
        # the compose sheet answered by going silent on the one state it most needed to report.
        "known": forecast_is_known(src, kind, prefix),
    }

## **THE WHOLE DEAL AN IMPROVEMENT OFFERS, composed in ONE place** (issue #442) — the take the crew
## holds today, the dipped take it accepts while it builds, and the payoff the finished rung pays:
##
##     +0.96  ->  +0.24 while building  ->  +1.20 /turn
##      today          preparing               payoff
##
## **THE DIP RIDES THE CREW, SO `preparing` IS NOT A SCALED CEILING** (`docs/plan_harvest_floor.md`
## §3.1). It is `min(workers x per_worker x fraction, ceiling)` — which differs from
## `min(...) x fraction` wherever the crew is already ceiling-bound, and that is exactly the case the
## move exists to fix: a crew big enough to saturate the source pays no dip at all, so the remedy for
## a slow build is more hands rather than a deeper floor. The three terms are therefore priced by the
## CALLER through `expected_yield_account`, which is why `base_forecast` and `build_forecast` are both
## carried whole rather than a pair of pre-multiplied numbers.
##
## Returns `{}` when `improvement` is `IMPROVEMENT_NONE` or the source carries no forecast, so a caller
## renders no deal rather than a deal made of zeros. `floor` is the crew's escapement floor; the two
## axes are independent, and a floor below the food peak beside a running build is LEGAL (it defeats
## itself through the ecology — the meter accrues only while the source is Thriving).
##
## The `feed` term is the pen's per-turn upkeep and rides ONLY the Corral rung (`FORECAST_FEED_KEYS`) —
## the one asymmetry between the two webs, and a deliberate one.
static func improvement_forecast(src: Dictionary, kind: String, prefix: String, floor: float,
        improvement: String) -> Dictionary:
    if improvement == IMPROVEMENT_NONE or not FORECAST_PAYOFF_KEYS.has(improvement):
        return {}
    var base_forecast := forecast_inputs(src, kind, prefix, floor)
    if not bool(base_forecast["known"]):
        return {}
    # The dip factor. 0/absent means the wire does not describe this rung's build on this source
    # (a species that can never be penned, a rung-3 source with nothing left to build) — the deal is
    # then unquotable, so say nothing rather than render a `x 0` dip reading "building pays nothing".
    var fraction := float(src.get(
        prefix + String(FORECAST_BUILD_FRACTION_KEYS[improvement]), 0.0))
    if fraction <= 0.0:
        return {}
    var payoff := float(src.get(prefix + String(FORECAST_PAYOFF_KEYS[improvement]), 0.0))
    # The payoff's non-food component — a VECTOR like every other yield in this model. Fodder is
    # plant-only (no animal pays it — a structural zero, not a gap), so a herd rung has no twin in the
    # table, resolves to 0.0 and renders as nothing, which is the rule. (The trade twin that reached
    # all four rungs went with the trade axis, arc #527.)
    var payoff_fodder := 0.0
    if FORECAST_PAYOFF_FODDER_KEYS.has(improvement):
        payoff_fodder = float(src.get(prefix + String(FORECAST_PAYOFF_FODDER_KEYS[improvement]), 0.0))
    var feed_rung: bool = FORECAST_FEED_KEYS.has(improvement)
    var feed := 0.0
    if feed_rung:
        feed = float(src.get(prefix + String(FORECAST_FEED_KEYS[improvement]), 0.0))
    return {
        "improvement": improvement,
        "floor": clamp_floor(floor),
        "build_fraction": fraction,
        # The crew's forecast WITHOUT the build (what it takes today) and WITH it (what it takes while
        # the rung goes up) — two whole forecasts rather than two numbers, so each term is priced per
        # account through the same `expected_yield_account` the committed row will be.
        "base_forecast": base_forecast,
        "build_forecast": forecast_inputs(src, kind, prefix, floor, improvement),
        # The un-crewed reference the picker faces quote: what the SOURCE offers at this floor. The
        # ceiling is dip-free by construction now, so there is exactly one of these, not one per term.
        "ceiling": float(base_forecast["ceiling"]),
        "ceiling_fodder": float(base_forecast["ceiling_fodder"]),
        "payoff": payoff,
        "payoff_fodder": payoff_fodder,
        "feed_rung": feed_rung,
        "feed": feed,
        # Which account's zero is worth printing on any of the three terms (§7.7).
        "zero_account": String(base_forecast["zero_account"]),
    }

## **THE VERB THIS SOURCE IS ACTUALLY BUILDING** — `improvement` when its rung is still to climb,
## `IMPROVEMENT_NONE` when the source has ALREADY BUILT it. A verb naming a standing rung is a STALE
## VERB, not a build in flight, and it must price no crew: the sim clears an assignment's
## `improvement` the turn the rung completes, so the wire and a composition that outlived the build
## disagree, and only the source's own done flags can tell them apart.
##
## **The improvement control already made exactly this test** (`_build_improvement_control` renders
## DONE rather than RUNNING for one) — so before this existed, a finished Tended Patch showed
## `🌾 Tended Patch` while its crew targets and its take were still quoted at the Cultivate build's
## 25% carry: the panel said the build was over and priced it as running. Reported from play, where
## the sheet asked for 6 foragers to hold a patch the sim's own standing rate showed 2 already
## holding. Every crew term goes through `build_dip`, so stating it once here is what keeps the two
## halves of the panel dividing by ONE throughput.
static func live_improvement(src: Dictionary, prefix: String, improvement: String) -> String:
    return IMPROVEMENT_NONE if improvement_is_done(src, prefix, improvement) else improvement

## Is this improvement's rung ALREADY BUILT on this source? The test that turns the improvement
## control's Running state into its Done state, and the one definition of it.
##
## The plant rungs answer from their own bool (`is_cultivated` / `is_field`) — **never from the float
## beside it**, which is a build meter and reads 1.0 for one turn before the flag flips. `Tame` has no
## bool on the wire, so it reads a full `domestication` meter; `Corral` has both and takes the bool.
## `prefix` spells the keys, so this works against a `patch_`-prefixed tile_info and a bare herd alike.
static func improvement_is_done(src: Dictionary, prefix: String, improvement: String) -> bool:
    if improvement == IMPROVEMENT_TAME:
        return float(src.get(prefix + FORECAST_BUILD_METER_KEYS[IMPROVEMENT_TAME], 0.0)) \
            >= DOMESTICATION_COMPLETE
    if not FORECAST_DONE_FLAG_KEYS.has(improvement):
        return false
    if bool(src.get(prefix + String(FORECAST_DONE_FLAG_KEYS[improvement]), false)):
        return true
    for higher_variant in FORECAST_RETIRED_BY_HIGHER_RUNG.get(improvement, []):
        if bool(src.get(prefix + String(FORECAST_DONE_FLAG_KEYS[String(higher_variant)]), false)):
            return true
    return false

## How far along this improvement's build meter is, 0..1. Clamped, so a wire value that overshoots
## cannot render a >100% meter. See `FORECAST_BUILD_METER_KEYS` for which meter each verb fills.
static func improvement_progress(src: Dictionary, prefix: String, improvement: String) -> float:
    if not FORECAST_BUILD_METER_KEYS.has(improvement):
        return 0.0
    return clampf(float(src.get(
        prefix + String(FORECAST_BUILD_METER_KEYS[improvement]), 0.0)), 0.0, 1.0)

## Workers beyond this produce nothing at this source under the selected policy —
## ceil(ceiling / per_worker). A tended patch / corralled herd reports every ceiling == per_worker, so
## this collapses to 1 (policy irrelevant).
##
## **IT IS FLOORED ON THE *HOLD* CREW, AND THAT IS THE WHOLE POINT OF §7.2.** The division above
## answers "how many hands clear the room standing THIS turn"; the hold crew answers "how many take
## the regrowth EVERY turn", and the second is a claim about usefulness that the first cannot bound.
## Consider the limit: a source sitting exactly at its floor has no room, so the quotient is `0` —
## *no workers are useful here* — while the hold crew is positive, because next turn it grows and
## those hands take exactly that growth. Telling the player to drop a crew they need on the very next
## turn is arithmetically defensible and practically false, which is the failure the verdict line
## exists to remove. So the cap is the `max()` it always structurally was, with one more term:
##
##     max_useful = max(ceil(room / (carry × dip)), hold_crew, reach_crew, <the caller's crew floors>)
##
## `reach_crew` is the same argument one step along: where the regrowth across the band beats the
## room, the crew that DRAWS THE SOURCE DOWN is larger than the one-turn quotient, so the cap said
## "these hands are useless" about the very crew the verdict was naming as the remedy — and the
## *clear it now* target, which is floored on that number, would have named a count the `+` refused.
##
## Folding it in HERE rather than at the call sites is what keeps the two cap twins
## (`source_worker_cap_state` and `DrawerComposeController._forecast_worker_cap`) unable to disagree:
## both divide through this one function, so neither can be given the floor without the other. It is
## also what makes the chart's *hold it after* target reachable by the stepper beside it — a clickable
## target the `+` refuses to reach is a panel arguing with itself.
##
## **Three outcomes, and telling them apart IS issue #426:**
## - `MAX_USEFUL_UNBOUNDED` — the wire describes no forecast, so there is no ceiling to impose.
## - `MAX_USEFUL_BARREN` (1) — described, and it pays nothing in the account it is counted on. The cap
##   stays LIVE, which is the fix: unbounded here let a worthless source absorb the whole idle crew
##   (measured at 7 workers on a source that can use 1), because both cap twins read unbounded as
##   "no ceiling".
## - a real `ceil(ceiling / per_worker)`.
static func max_useful_workers(forecast: Dictionary) -> int:
    if not bool(forecast.get("known", false)):
        return MAX_USEFUL_UNBOUNDED
    # ON THE AXIS THE SPECIES PAYS (issue #337): a wolf's food per-worker and ceiling are both 0, so
    # the food-denominated cap would read ceil(0/0) and cap the crew at nothing. The axis triple falls
    # back to the food pair for every edible species and every forage patch, so this is a widening.
    var per_worker := float(forecast.get("axis_per_worker", forecast.get("per_worker", 0.0)))
    var ceiling := float(forecast.get("axis_ceiling", forecast.get("ceiling", 0.0)))
    if per_worker < FORECAST_MIN_PER_WORKER:
        # **Described, and barren on every account it could be counted on** — a dead-season patch. Not
        # unbounded: we know what this source pays, and it is nothing, so the honest ceiling is one
        # worker. Returning UNBOUNDED here was the second half of #426 — it did not merely drop the
        # "max N useful" note, it removed the ceiling from both cap twins, so the guard against parking
        # a crew on a worthless source was disabled by precisely the sources it exists for.
        return MAX_USEFUL_BARREN
    # WHOLE-ANIMAL HUNT: the cap is the carriers needed to HAUL the animals that drop on the worst turn,
    # not ceil(smoothed-rate / per_worker). An 80-biomass aurochs drops all at once; one hunter carrying
    # <per_worker> food wastes the rest, so the smoothed rate under-counts. Worst case the kill-credit
    # bank holds just under one body when the turn's rate lands, so floor(ceiling / food_per_animal) + 1
    # whole animals drop, each worth food_per_animal — carry that peak, not the average flow. It is
    # `haul_workers`, the ONE mirror of the sim's rounding, in the paid account's units rather than in
    # biomass (an animal count is a ratio, so either set of units gives the same crew).
    var per_animal := float(forecast.get("axis_per_animal", forecast.get("food_per_animal", 0.0)))
    # BOTH PROJECTION-DERIVED FLOORS: the crew that takes the regrowth every turn, and the crew that
    # draws the stock down to the floor at all (`reach_crew` — the number the *clear it now* target is
    # floored on, and therefore the number the stepper has to be able to reach).
    var hold := maxi(maxi(int(forecast.get("hold_crew", 0)), 0),
        maxi(int(forecast.get("reach_crew", 0)), 0))
    if bool(forecast.get("whole_animal", false)) and per_animal > 0.0:
        # **TWO JOBS, ONE CREW, TWO UNITS** — reach the animals, then carry them home. This is the
        # sim's `fauna::hunt_take_workers`, `max(haul, engage)` and never `+`: one crew covering its
        # busiest job. Sizing it on carry alone told a Wild Fowl player "max 2 workers useful here"
        # while ~470 birds stood above the floor and each hunter reached ten of them — the advice was
        # backwards, and it was backwards for the whole life of the engagement field's absence.
        # `take_workers` answers the haul crew alone for a pen and for a species with no engagement
        # stage, exactly as this line did before that `max` had a name of its own — and it now has
        # one because `crew_to_hold` asks the same question about the regrowth.
        #
        # **AND IT DIVIDES BY WHAT STAYS, so the cap can never sit below a target the sheet names.**
        # The retreat is the source's own effective one, `repriced_source` having already folded the
        # kit's `dispersion` into it, and it reaches here through the forecast rather than off the
        # herd so the cap and the two pills cannot resolve one kit's dispersion two ways.
        return maxi(take_workers(ceiling, per_animal, per_worker,
            float(forecast.get("engage_rate", NO_ENGAGEMENT_STAGE)),
            float(forecast.get("dip", NO_BUILD_DIP)),
            float(forecast.get("stay", STAY_FRACTION_NONE_BREAKS_OFF))), hold)
    return maxi(int(ceilf(ceiling / per_worker)), hold)

## The herding crew this herd demands, as a FLOOR on a local-hunt worker cap — the one definition,
## read by BOTH cap twins so a worked row and a compose stepper can never gate differently.
##
## A managed herd needs `herders_needed` hands EVERY turn to HOLD it, but the take/prepare max-useful
## knows nothing about that: it answers "2 workers saturate this herd's take", and the row's `+` then
## goes dead two below the crew the sim is asking for — while the SAME row renders the under-herded ⚠.
##
## A crew that is BUILDING an improvement reads the ownership-INDEPENDENT `herders_needed_if_managed`
## instead, because the improvement is what MAKES the herd managed: a still-wild herd reports
## `herders_needed == 0` right up until the Population stage sets ownership, so the plain field would
## pin the player at the 1-worker prep count and the herd would read under-herded the moment it became
## theirs. The two fields are equal on an already-managed herd, so this is safe either way.
##
## `building` is the IMPROVEMENT axis (issue #442) — the assignment's own `improvement != ""` on a
## worked row, the composed improvement on the compose sheet. It used to be read off the forecast's
## `investment` flag, which only existed while a build verb was a value of `policy`.
static func herd_crew_floor(herd: Dictionary, building: bool) -> int:
    if building:
        return int(herd.get("herders_needed_if_managed", 0))
    return int(herd.get("herders_needed", 0))

## **The PLANT twin of `herd_crew_floor`** (issue #442) — the crew the rung being built on this patch
## demands (`<rung>CrewNeeded`), as a FLOOR on the compose stepper's worker cap. Same shape, same
## contract, same one definition: a RAISE, never a new cap, and `0` for a patch that is only being
## gathered.
##
## It exists because the dip and the cap fight each other. While a build runs the ceiling the cap
## divides is `stance × buildFraction`, so `ceil(ceiling / perWorker)` collapses — a 25-turn
## improvement asked for FEWER hands than gathering the same ground, and the sim's own
## `source_crew_needed` (`max(build crew, take crew)`) then reported the row overstaffed at the count
## the sheet had just capped it to. The rung's crew is the standing half of exactly that `max`.
##
## `prefix` spells the keys, so this reads a `patch_`-prefixed tile_info and a raw wire patch alike.
static func plant_crew_floor(patch: Dictionary, prefix: String, improvement: String) -> int:
    if not FORECAST_CREW_KEYS.has(improvement):
        return 0
    return int(patch.get(prefix + String(FORECAST_CREW_KEYS[improvement]), 0))

## Per-SOURCE `+`-gate for a CONFIRMED Current-actions Forage/Hunt row — the worked-row twin of the
## compose stepper's `max_useful_workers` cap (`DrawerComposeController._forecast_worker_cap`), and
## beside it so the two can never disagree. A source's `+` may add a worker only while the band has an
## idle worker AND this source is below its own max-useful ceiling, so a single source can't absorb
## workers past the point they help. An unknown forecast (MAX_USEFUL_UNBOUNDED — no wire data) falls
## back to the plain `idle > 0` gate. Returns `{can_add, note}`; `note` is set ONLY when max-useful (not
## idle) is what stopped the `+`, so the row tooltip explains a dead button rather than leaving it
## mysterious (the idle-exhausted gate explains itself). Scout/Warrior are band-wide roles with no
## ceiling — they keep the plain gate and never call this.
##
## `useful_floor` IS WHAT KEEPS THE TWIN PROMISE HONEST. The compose side folds a managed herd's
## herding crew into its usefulness ceiling; a row that did not would flag a herd under-herded and then
## disable the very `+` that fixes it. A HUNT caller therefore passes
## `herd_crew_floor(herd, building)` — the one definition of that number, keyed on the IMPROVEMENT
## axis (`building` is a bool: the assignment's own `improvement != ""` on a worked row, the composed
## improvement on the compose sheet) — and a FORAGE caller passes nothing, a patch owing no crew.
## The floor is a RAISE, never a new cap, and an UNBOUNDED forecast stays unbounded; a wild herd
## reports 0, so `max(useful, 0)` is a no-op there.
##
## **THE *HOLD* CREW IS NOT PASSED HERE, DELIBERATELY.** It is a floor on usefulness for every source
## on both webs — not a demand one KIND of source makes — so it lives inside `max_useful_workers`,
## where both twins pick it up without either caller being trusted to remember it. `useful_floor`
## stays what it always was: the crew this particular caller's rung is asking for.
static func source_worker_cap_state(forecast: Dictionary, workers: int, idle: int,
        useful_floor: int = 0) -> Dictionary:
    var useful := max_useful_workers(forecast)
    if useful != MAX_USEFUL_UNBOUNDED:
        useful = maxi(useful, useful_floor)
    if useful == MAX_USEFUL_UNBOUNDED or workers < useful:
        return {"can_add": idle > 0, "note": ""}
    # At/over this source's max-useful: the `+` is capped by the source, not by idle. Explain only
    # when idle workers remain (else the idle-exhausted gate already reads for itself).
    var note := ""
    if idle > 0:
        var noun := MAX_USEFUL_NOUN_ONE if useful == 1 else MAX_USEFUL_NOUN_MANY
        note = MAX_USEFUL_CAPPED_TOOLTIP % [useful, noun]
    return {"can_add": false, "note": note}

## The take `workers` would ACTUALLY produce here: min(workers × per_worker, ceiling, the party's
## reach), scaled by the acting band's output multiplier (the sim exports the forecast at 1.0).
static func expected_yield(forecast: Dictionary, workers: int, band: Dictionary) -> float:
    return expected_yield_account(forecast, workers, band, "per_worker", "ceiling",
        FORECAST_FOOD_PER_ANIMAL_KEY)

## **THE ENGAGEMENT ARM OF THE TAKE, IN ONE ACCOUNT'S UNITS** — `floor(workers × engageRate × dip)`
## whole animals, each worth this account's per-animal quantum. That quantum IS `bodyMass ×
## <account>PerBiomass` (the wire publishes the product as `food_per_animal`), so
## this is the schema's `reach(workers, rung)` with no second derivation of the body.
##
## `ENGAGEMENT_UNBOUNDED` — i.e. the arm drops out of the caller's `min()` — in the two cases where it
## has nothing to say: a source with **no engagement stage** (a pen, the plant web), and an account
## with **no whole-animal quantum** at all (fodder, which no animal pays). Neither is "reaches
## nothing", and treating either as zero would collapse a take the sim pays in full.
static func engagement_reach(forecast: Dictionary, workers: int, per_animal_key: String) -> float:
    return engaged_quantum(workers, float(forecast.get(per_animal_key, 0.0)),
        float(forecast.get("engage_rate", NO_ENGAGEMENT_STAGE)),
        float(forecast.get("dip", NO_BUILD_DIP)),
        float(forecast.get("stay", STAY_FRACTION_NONE_BREAKS_OFF)))

## The same arm with its quantum handed in rather than looked up — the form the CHART's projection
## needs, whose quantum is `bodyMass` (the curve, the room and the throughput are all biomass there)
## rather than an account's `*_per_animal`. `engagement_reach` is this function reading a forecast, so
## the sheet's take and the chart's projection bound themselves on ONE definition.
##
## **IT BOUNDS THE WHOLE-ANIMAL COUNT AND THEN CONVERTS**, never the other way about: `animals_engaged`
## floors `workers × engageRate × dip` to whole animals exactly as the sim does, and the quantum is
## applied to that count. Multiplying first and flooring after can land a whole engagement one animal
## short on a rounding.
## **THE RETREAT IS APPLIED HERE AND NOT ONE STAGE EARLIER**, in the sim's own order — engage, retreat,
## then convert — so `stay` cuts the whole-animal count the quantum then values. It defaults to the
## wire's "nothing breaks off", which is what leaves a pen, the plant web and every source that
## publishes no retreat byte-identical to before the stage existed.
static func engaged_quantum(workers: int, per_animal: float, engage_rate: float,
        dip: float, stay: float = STAY_FRACTION_NONE_BREAKS_OFF) -> float:
    if per_animal <= 0.0:
        return ENGAGEMENT_UNBOUNDED
    return animals_stayed(animals_engaged(workers, engage_rate, dip), stay) * per_animal

## The same take on ANY ONE account (#426). `min(workers × per_worker, ceiling)` is applied PER
## COMPONENT, never to a total: the sim caps each account against its own ceiling, and a patch whose
## labor binds on food can be ceiling-bound on fodder in the same turn. The account keys are
## `forecast_inputs`' own (`per_worker`/`ceiling`, `per_worker_fodder`/`ceiling_fodder`) — passed in
## rather than switched on here, so adding a third account is a call site, not an edit to this
## function.
##
## **THE BUILD DIP IS ALREADY IN THE PER-WORKER TERM AND THERE IS NO SCALE PARAMETER** (§3.1). It used
## to be a `ceiling_scale` argument that went inside the `min`, because the sim then dipped the
## CEILING; the sim now dips the CREW, so the factor belongs to whichever forecast the caller passes —
## `improvement_forecast`'s `build_forecast` carries it, its `base_forecast` does not, and the two
## terms of the deal are the same call against different forecasts. A surviving `× fraction` on a
## ceiling anywhere is now wrong, and it looks plausible.
##
## **THE `crew_unknown` ESCAPE HATCH IS GONE, and the wire is why.** A Field of flax used to have no
## per-worker term this layer could compute (`per_worker_yield / provisions_per_biomass` is `0/0`
## there), so the account quoted the SOURCE's whole ceiling rather than report `0.00` of a rung whose
## real product is not food. `perWorkerBiomass` states that throughput directly on both webs, so every
## account is priced by the crew that works it and the `min` is honest everywhere.
##
## **THE `min` HAS A THIRD ARM ON THE ANIMAL WEB** (`docs/plan_hunt_through_combat.md` §2). Engagement
## caps how many animals a party can *reach at all*, and the two arms above it cannot express that: a
## crew's carry and the stock above the floor both say a lone hunter takes 307 Wild Fowl a turn where
## the sim pays ten. `per_animal_key` is the account's own whole-animal quantum
## (`food_per_animal`), and it defaults to **empty on purpose** — an account with
## no quantum (fodder; a source the wire states none for) has no engagement arm rather than a zero
## one, which is the same "unbounded, not nothing" reading `NO_ENGAGEMENT_STAGE` gets.
static func expected_yield_account(forecast: Dictionary, workers: int, band: Dictionary,
        per_worker_key: String, ceiling_key: String, per_animal_key: String = "") -> float:
    var ceiling := float(forecast.get(ceiling_key, 0.0))
    var raw := minf(minf(float(workers) * float(forecast.get(per_worker_key, 0.0)), ceiling),
        engagement_reach(forecast, workers, per_animal_key))
    return raw * float(band.get("output_multiplier", OUTPUT_FULL))

## Resolve a worked source's row readout. Two INDEPENDENT signals ride the same row:
##   • overdraw (`warn` → the ⚠ flag) — ecological: the take exceeds the renewable ceiling.
##   • overstaffed (`note` → "· only N of M working") — labor: the source's take was capped below
##     what the assigned workers could produce, so the surplus workers idled HERE and should be
##     reassigned. True for ALL policies (every source has a ceiling), and orthogonal to overdraw —
##     a source can be overstaffed while perfectly sustainable, or overdrawn while fully used.
## Parts are empty when the source carries no confirmed data (pending assign), so
## the row degrades to bare rather than asserting a wrong state.
static func source_yield_readout(m: Dictionary, kind: String) -> Dictionary:
    var label_suffix := ""
    var warn := false
    var tooltip := ""
    # The honest per-turn rate the row headlines (and the caller derives the kill-rhythm from).
    var rate := 0.0
    # Its FODDER twin (issue #449) — 0 on every hunt row (no animal pays feed) and on any patch
    # growing nothing a pen eats, which is what suppresses the term everywhere it does not belong.
    var fodder_rate := 0.0
    # …and its MATERIAL twin, as a VECTOR (arc #527 follow-up). Empty on every row whose source pays
    # no material AND on every row whose take has not resolved yet, which render identically — no row.
    var material_rows: Array[Dictionary] = []
    if bool(m.get("has_yield", false)):
        var actual := float(m.get("actual_yield", 0.0))
        var sustainable := float(m.get("sustainable_yield", 0.0))
        # A source overdraws when its take draws the stock below what it sustains. This is the
        # sim-answered `overdraws` flag (policy-driven: `!managed && policy.overdraws()`), NOT the
        # client-derived `actual > sustainable` — which false-positives on a hunt's kill turn (cashing a
        # banked whole animal spikes `actual` above the steady `sustainable` even under Sustain). Forage
        # on Sustain reads clean; a Surplus/Deplete/Eradicate patch or an over-hunted herd trips ⚠.
        warn = bool(m.get("overdraws", false))
        var renewable := kind == LABOR_KIND_FORAGE and not warn
        tooltip = "Actual %s" % format_yield(actual)
        # **THE ACTUAL IS AN EXPECTATION NOW, AND ITS BAND RIDES BESIDE IT** (§6.4). The headline
        # stays the expectation — that is what `forecast == actual` is restated on — and the band
        # QUALIFIES it rather than replacing it. `""` where the distribution is degenerate, which is
        # every row shipped today and is what keeps this string byte-identical to what it printed
        # before.
        tooltip += yield_range_clause(m)
        if renewable:
            tooltip += YIELD_TOOLTIP_RENEWABLE
        else:
            tooltip += " · Sustainable %s" % format_yield(sustainable)
            if warn:
                tooltip += YIELD_TOOLTIP_OVERDRAW
        # HEADLINE the row with the STEADY realized average, never the lumpy pulse. `realized_yield` is
        # the honest long-run average of this source's `actual_yield`, so BOTH hunt and forage read it:
        # forage's realized ≈ its old `actual` (no visible change), while hunt switches off the
        # `sustainable` ceiling to the true realized average — which is what makes the row (and the
        # Food-line income these rows sum into) steady. The pulse's overdraw is still carried by
        # the ⚠ flag + tooltip. Falls back to the old sustainable/actual split if `realized_yield` is
        # absent (older snapshot).
        if m.has("realized_yield"):
            rate = float(m["realized_yield"])
        else:
            rate = sustainable if kind == LABOR_KIND_HUNT else actual
        # THE SECOND PRODUCT (issue #449), under the SAME render-only-when-non-zero gate: a sown hay
        # Field pays no provisions, so without this its row headlined `+0.00 /turn` while it fed the
        # band's pens every turn. The word rather than a glyph — fodder has none, the reason
        # `yield_components` gives — and the tooltip reuses the rung tooltips' own fodder wording
        # rather than spelling the account a third way.
        #
        # **THE THIRD PRODUCT IS A VECTOR OF MATERIALS** — the trade-goods scalar that rode here
        # (issue #337) was retired with the axis, and `material_yield` replaced it per material
        # (arc #527 follow-up). It is what an INEDIBLE quarry is for: a wolf pays no provisions and no
        # feed, so without this its row headlined `+0.00 /turn` while its pelts landed in the band's
        # `MaterialStore` every turn — the same defect the fodder term fixed one account earlier.
        # **The resolved yield, never a forecast**: a pre-commit row seeds it empty by design, which
        # is why the compose sheet reads the herd's rates instead (`material_rows_of`).
        fodder_rate = fodder_rate_of(m)
        material_rows = material_rows_of(m)
        if has_component(fodder_rate):
            tooltip += COMPONENT_SEPARATOR + (POLICY_CAP_FODDER_FORMAT % format_signed(fodder_rate))
        for row in material_rows:
            if has_component(float(row[MATERIAL_PAYOFF_AMOUNT_KEY])):
                tooltip += COMPONENT_SEPARATOR + (POLICY_CAP_MATERIAL_FORMAT % [
                    format_signed(row[MATERIAL_PAYOFF_AMOUNT_KEY]),
                    String(row[MATERIAL_PAYOFF_ID_KEY])])
        # `zero_account` stays defaulted: a source that produced nothing in ANY account still prints
        # its `+0.00 /turn`, which is a fact worth reading and is what this row has always said.
        label_suffix = " " + yield_components(rate, fodder_rate, YIELD_ACCOUNT_FOOD, material_rows)
    # Overstaffing: fewer workers were needed than are assigned, so the remainder produced nothing
    # here. `workers_needed == 0` means "unknown" (rehydrated) → no note.
    var note := ""
    var workers := int(m.get("workers", 0))
    var needed := int(m.get("workers_needed", 0))
    if needed > 0 and workers > needed:
        note = OVERSTAFF_NOTE_FORMAT % [needed, workers]
        tooltip = OVERSTAFF_TOOLTIP if tooltip == "" \
            else tooltip + TOOLTIP_LINE_SEPARATOR + OVERSTAFF_TOOLTIP
    # UNDERSTAFFING: `wasted_yield` is food the source offered that the crew could not collect — the
    # party is under-crewed for the kill. A muted note (the low-key mirror of the overstaff note); the
    # tooltip spells it out. Below FOOD_FLOW_MIN ⇒ hidden (0 on a rehydrated save).
    #
    # **THE ANIMAL WEB ONLY, and that is a claim about the NUMBER rather than about any one surface.**
    # One wire field carries two opposite facts. On a herd it is `killed_biomass − carried`: meat the
    # crew killed and left to rot, gone for good, and a genuine call to send more hands. On a patch it
    # is `escapement_room − take` — stock the crew simply did not reach, which the sim's own note says
    # outright is "not lost, it simply stays in the stock and regrows". Nothing rots and nothing is
    # owed.
    #
    # It also fired on the WRONG SIDE of the condition there. `max(0, room − take)` is positive
    # whenever a crew does not clear the whole escapement room in ONE turn — the ordinary state on a
    # patch, and the state the compose sheet actively recommends: its `hold it after` target is by
    # construction far below its `clear it now` one, so a player who staffs the sustainable number was
    # told they were wasting food every turn, forever, with no action that would ever clear it. On a
    # herd `killed > carried` is genuinely exceptional, which is why the note never read wrong there.
    #
    # Understaffing a BUILD is a real loss and a real prompt — a Cultivate or a Tame accrues at
    # `min(workers / crew_needed, 1)` and decays when neglected — but this note has never carried that
    # signal, so nothing is lost by silencing it here.
    # **THE BAND, ON THE ROW ITSELF** (§6.4) — the same clause the tooltip carries, in the muted
    # register the wasted note already uses, so all three hosts of this readout (the work board's
    # rows, the drawer's standing summary, the stepper's status line) show it without a channel of
    # their own. `""` while the distribution is degenerate, so no row grows a band where there is
    # none: that emptiness is the assertion, not a hope.
    var muted_note := yield_range_clause(m) if bool(m.get("has_yield", false)) else ""
    var wasted := float(m.get("wasted_yield", 0.0))
    if kind != LABOR_KIND_FORAGE and wasted >= FOOD_FLOW_MIN:
        muted_note += WASTED_NOTE_FORMAT % format_magnitude(wasted)
        var wasted_tip := WASTED_TOOLTIP % format_yield(wasted)
        tooltip = wasted_tip if tooltip == "" else tooltip + TOOLTIP_LINE_SEPARATOR + wasted_tip
    return {
        "label_suffix": label_suffix, "warn": warn, "note": note,
        "muted_note": muted_note, "tooltip": tooltip, "rate": rate,
        # The FODDER component (#449), so a surface composing its own string — the work row's one-slot
        # rate, the WORK header's totals, the work inspector's sentence — states the second account
        # rather than reading a hay Field as a dead tile.
        "fodder_rate": fodder_rate,
        # …and the MATERIAL component, as the vector it is. Same job, one account further out: the
        # one-slot rate column, the map's yield label, the WORK header's totals and the inspector
        # sentence all compose their own string and must state what an inedible quarry pays.
        "material_rows": material_rows,
    }

## A hunt source is MANAGED (its crew are herders/keepers, not a hunt party) once the herd is penned,
## fully tamed (pastoral), being penned under the composed Corral policy, or **owed a herder crew by
## the sim**. `workersNeeded` on such a source scales with the HERD (max herders, haulers), so the
## crew label must read as herders.
##
## THE `herders_needed > 0` CLAUSE IS THE SIM'S OWN STATEMENT THAT THIS HERD OWES KEEPERS. The field
## is ownership-gated (`fauna::herd_herders_needed`), so it goes positive the moment the herd becomes
## OWNED — part-way through taming, well before `domestication` reaches completion. The drawer's
## "Herders: A / N — under-herded" row gates on the SAME field and the SAME `> 0` test, so the two
## surfaces can no longer disagree: without this clause the sheet's stepper and title said "Hunters"
## directly beside a drawer demanding 4 herders every turn.
##
## Deliberately NOT `improvement == IMPROVEMENT_TAME`: a still-WILD herd being tamed reports
## `herders_needed == 0`, is not yet owned, and its crew genuinely hunts at a reduced take that turn —
## "Hunters" is the honest word there. Corral is different because it BUILDS the pen the keepers hold.
##
## The last clause reads the IMPROVEMENT axis (issue #442), not the stance: a crew building a pen is
## keeping animals whether it is holding Sustain or Deplete while it does so.
static func is_managed_hunt_source(herd: Dictionary, improvement: String) -> bool:
    return bool(herd.get("corralled", false)) \
        or float(herd.get("domestication", 0.0)) >= DOMESTICATION_COMPLETE \
        or int(herd.get("herders_needed", 0)) > 0 \
        or improvement == IMPROVEMENT_CORRAL

## A herd's player-facing name (species → label → id). One definition, shared by the targeting banner's
## forecast line and the command-feed refusal, so a herd is never called two different things.
## **BIOMASS AS WHOLE ANIMALS — the unit every other number on a hunt surface is already in** (the
## readout's `≈0.41 Grey Wolf/turn`, the raid's `delivers ≈8 Wild Boar`, `crew_to_hold`'s divide by
## `body_mass`). The tile card's herd row and the floor flag both read it, so a herd is counted one
## way wherever it is counted.
##
## `ANIMAL_COUNT_NONE` means *no count can be stated* — a species the wire published no `body_mass`
## for, or no biomass at all. Both callers fall back to biomass on it, which is also what keeps a
## FORAGE patch's readouts unchanged: a patch has no body.
##
## **A positive biomass never counts ZERO.** Rounding alone reads `0` for a herd holding a fifth of a
## body — visibly alive on the map, and huntable, since the sim's own kill step floors at one body
## (`min(affordable, max(1, carryable))`). So does this.
const ANIMAL_COUNT_NONE := -1
static func animal_count(biomass: float, body_mass: float) -> int:
    if body_mass <= 0.0 or biomass <= 0.0:
        return ANIMAL_COUNT_NONE
    return maxi(1, int(round(biomass / body_mass)))

## **A STANDING QUANTITY IN THE UNIT ITS SOURCE COUNTS IN** — `98` for a patch, `≈11 Red Deer` for a
## herd. Both surfaces that name a floor's THRESHOLD read it from here: the chart's flag and the
## at-floor verdict under it. They are two statements of one number, and they diverged the moment the
## flag learned to count animals while the verdict went on quoting `grows past 1075` — so the cure is
## that there is no second place for a rendering to live, not a second place kept in step by hand.
const STOCK_ANIMALS_FORMAT := "≈%d %s"
static func stock_face(stock: float, body_mass: float, quarry: String) -> String:
    var animals := animal_count(stock, body_mass)
    if animals != ANIMAL_COUNT_NONE and quarry != "":
        return STOCK_ANIMALS_FORMAT % [animals, quarry]
    return format_stock(stock)

static func herd_display_name(herd: Dictionary) -> String:
    return String(herd.get("species", herd.get("label", herd.get("id", "This herd"))))

## The species' husbandry ceiling (Grazing 2d-δ) normalized to one of the three known values.
## Empty/absent/unrecognized ⇒ "pen" (the full ladder), so an un-tagged herd behaves as it did
## before the field existed. Read by the herd drawer + assign controls to gate husbandry affordances.
static func husbandry_ceiling(herd_data: Dictionary) -> String:
    var ceiling := String(herd_data.get("husbandry_ceiling", "")).strip_edges().to_lower()
    if ceiling == HUSBANDRY_CEILING_WILD or ceiling == HUSBANDRY_CEILING_PASTORAL:
        return ceiling
    return HUSBANDRY_CEILING_PEN

## The tile's basket as display-ready rows — `{species, display_name, percent, can_cultivate, can_sow}`
## in WIRE ORDER (share DESC, then species key ASC; never re-sorted here), with the rounding already
## resolved. THE ONE decomposition of the composition list: the "What grows here" row and the crop
## picker both read it, so the percentage a plant shows in the picker can never disagree with the
## percentage the row shows for that same plant.
##
## THE PERCENTAGES ALWAYS SUM TO 100 — rounding each share independently can total 99 or 101 (a
## decomposition that visibly fails to decompose), so the remainder is folded into the LARGEST share,
## i.e. the first entry, where a ±1 is proportionally smallest. `can_cultivate` / `can_sow` are the
## species-GLOBAL rung legality flags; a plant that is on this tile but carries neither still gets a
## row, because its presence is a fact about the land.
static func flora_basket_entries(composition: Variant) -> Array[Dictionary]:
    var entries: Array[Dictionary] = []
    if not (composition is Array):
        return entries
    var total := 0
    for entry_variant in composition:
        if not (entry_variant is Dictionary):
            continue
        var entry: Dictionary = entry_variant
        var name := String(entry.get("display_name", "")).strip_edges()
        if name == "":
            continue
        var percent := int(round(float(entry.get("share", 0.0)) * FLORA_SHARE_PERCENT_TOTAL))
        total += percent
        entries.append({
            "species": String(entry.get("species", "")).strip_edges(),
            "display_name": name,
            "percent": percent,
            "can_cultivate": bool(entry.get("can_cultivate", false)),
            "can_sow": bool(entry.get("can_sow", false)),
            "cultivate_yield_ratio": float(entry.get("cultivate_yield_ratio", FLORA_CROP_RATIO_NONE)),
            "sow_yield_ratio": float(entry.get("sow_yield_ratio", FLORA_CROP_RATIO_NONE)),
            # Carried through so the compose sheet's "→ then" term can quote the SELECTED crop's own
            # payoff; without these the row renders a correct ratio above a forecast that ignores it.
            "cultivate_payoff": float(entry.get("cultivate_payoff", 0.0)),
            "sow_payoff": float(entry.get("sow_payoff", 0.0)),
            # A sown FIELD of a fodder crop pays hay, not provisions — carried through so the picker row
            # can show the hay value in place of the 0× provisions ratio it would otherwise read.
            "sow_fodder_payoff": float(entry.get("sow_fodder_payoff", 0.0)),
            # THE TENDED-RUNG TWIN of the one above (#419). The `sow_*` payoffs are FIELD figures, so a
            # Cultivate row that read them stated rung 3's number on rung 2 — a Field's managed rate on
            # a rung that pays an MSY skim off a merely-weeded basket. Both rungs ride the entry; the
            # picker reads the one its policy names, exactly as it does for the ratio and the food
            # payoff.
            "cultivate_fodder_payoff": float(entry.get("cultivate_fodder_payoff", 0.0)),
            # **WHAT A CASH CROP PAYS, PER MATERIAL** (arc #527) — the replacement for the retired
            # `sow_trade_payoff` / `cultivate_trade_payoff` scalars. Each is an ARRAY of
            # `{material_id, amount}` rows, one per material this plant would yield per turn at that
            # rung on this tile, and it is carried through VERBATIM: the picker renders one row per
            # entry and **must never sum them into one materials/turn figure**, which is the retired
            # trade axis under a new name.
            #
            # **AN EMPTY ARRAY IS "NO ROW", NEVER "ZERO"** — the wire's own contract, and the reason the
            # default here is `[]` rather than a sentinel. A grain Field honestly pays no material; a
            # `0` in its place would read as a cash crop that pays badly.
            #
            # **THE TWO RUNGS DIFFER IN KIND, not by a scale factor.** A sown Field is 100% its crop
            # (#433), so a grain Field quotes nothing at all; a TENDED patch is a weeded basket whose
            # volunteers are still standing, so a tended grain honestly quotes the fibre its neighbours
            # pay. Read each rung's own vector — one never implies the other.
            "sow_material_payoff": material_payoff_rows(entry.get("sow_material_payoff", [])),
            "cultivate_material_payoff":
                material_payoff_rows(entry.get("cultivate_material_payoff", [])),
            # WHAT THIS PLANT IS FOR — the sim's own display tag ("staple"/"fodder"/"cash"), carried
            # so the tile card's basket rows can lead with a role icon. **`""` is UNSTATED and must
            # stay `""`**: defaulting a missing tag to "staple" would invent a fact, and re-deriving
            # one from the payoffs above is wrong twice over — they are rung-2/rung-3 numbers folding
            # in the weeding and conversion gains, and they read all-zero for a species that cannot
            # climb on this ground, which is exactly where the role is still true and useful.
            "role": String(entry.get("role", "")).strip_edges().to_lower(),
        })
    if entries.is_empty():
        return entries
    entries[0]["percent"] = int(entries[0]["percent"]) + FLORA_SHARE_PERCENT_TOTAL - total
    return entries

## The two keys of ONE per-material payoff row, as `native/src/dict/subsistence.rs` writes them.
## `material_id` is the `materials.json` id (`fibre`, `tobacco`, `grape`) — the same id the material
## CATALOGUE (`SubsistenceSection.materials`) and a band's `material_batches` are keyed by, which is
## what lets a picker row and the Crafting panel's rail name one material identically.
const MATERIAL_PAYOFF_ID_KEY := "material_id"
const MATERIAL_PAYOFF_AMOUNT_KEY := "amount"

## One rung's per-material quote, normalized to `[{material_id, amount}]`.
##
## **AN EMPTY ANSWER IS "THIS PLANT PAYS NO MATERIAL", WHICH IS A REAL ANSWER** — the caller renders no
## row for it, never a `0`. A row naming no material is dropped: an id is what a row is FOR, and a
## nameless amount could only be rendered as the summed scalar this arc exists to refuse.
static func material_payoff_rows(raw: Variant) -> Array[Dictionary]:
    var rows: Array[Dictionary] = []
    if not (raw is Array):
        return rows
    for row_variant in raw:
        if not (row_variant is Dictionary):
            continue
        var row: Dictionary = row_variant
        var material_id := String(row.get(MATERIAL_PAYOFF_ID_KEY, "")).strip_edges()
        if material_id == "":
            continue
        rows.append({
            MATERIAL_PAYOFF_ID_KEY: material_id,
            MATERIAL_PAYOFF_AMOUNT_KEY: float(row.get(MATERIAL_PAYOFF_AMOUNT_KEY, 0.0)),
        })
    return rows

## **THE ONE DEFINITION of a worked source's MATERIAL rows** — the resolved yield, read off a
## labor-assignment / worker-map dict. The material twin of `fodder_rate_of`, and the reason an
## inedible quarry stops reading `+0.00` on every compact readout in the HUD (arc #527 follow-up).
##
## **A PLAIN READ IS THE WHOLE OF IT, and there is deliberately no realized/projected sibling.** The
## sim seeds this EMPTY on a pre-commit row by design: projecting materials needs the take in BIOMASS,
## while the forecast resolves in currency space where an inedible species has no positive axis to
## resolve on. So an empty answer here means either "this source pays no material" or "no take has
## resolved yet", and **both render as no row** — which is why the compose sheet must read the herd's
## two RATES instead of this (`forecast_inputs`' `per_worker_material` / `material_ceiling`).
##
## **NEVER SUM THE ROWS.** One materials/turn figure is the retired trade axis under a new name.
static func material_rows_of(source: Dictionary) -> Array[Dictionary]:
    return material_payoff_rows(source.get(ASSIGNMENT_MATERIAL_YIELD_KEY, []))

## **THE ONE-SLOT SURFACES' MATERIAL ARM** — every material this source pays, SIGNED, joined in the
## sentence idiom (`+0.22 hide`), and `""` when there is nothing to say. The empty answer is the whole
## point: it is the gate a fall-through tests, so "this source pays no material" and "this source pays
## a material" are one call rather than a condition each caller re-derives.
##
## **EVERY material, never the first one.** Picking one of a vector names a winner the sim does not
## name; summing them is the retired trade axis under a new name. A species pays few materials, and
## both callers size to their measured run rather than clipping.
static func signed_material_components(rows: Array) -> String:
    var parts: Array[String] = []
    for row in material_payoff_rows(rows):
        var amount := float(row[MATERIAL_PAYOFF_AMOUNT_KEY])
        if has_component(amount):
            parts.append(PICKER_MATERIAL_PRODUCT_FORMAT % [
                format_signed(amount), String(row[MATERIAL_PAYOFF_ID_KEY])])
    return COMPONENT_SEPARATOR.join(parts)

## One per-material vector times a scalar — a per-biomass vector through the escapement room, a
## per-worker vector through the build dip, or any of them through the band's `output_multiplier`.
## **Every material scales by the SAME factor**, because they are one biomass flow through a fixed
## per-biomass vector, exactly as the food and fodder accounts are.
static func scaled_material_rows(rows: Array, factor: float) -> Array[Dictionary]:
    var out: Array[Dictionary] = []
    for row_variant in rows:
        if not (row_variant is Dictionary):
            continue
        var row: Dictionary = row_variant
        out.append({
            MATERIAL_PAYOFF_ID_KEY: String(row.get(MATERIAL_PAYOFF_ID_KEY, "")),
            MATERIAL_PAYOFF_AMOUNT_KEY:
                float(row.get(MATERIAL_PAYOFF_AMOUNT_KEY, 0.0)) * factor,
        })
    return out

## **WHAT A CREW OF `workers` TAKES, PER MATERIAL** — `min(workers × per_worker, ceiling)` evaluated
## once per material, which is the food side's own clamp applied one account further out. `forecast`
## is a `forecast_inputs` answer and `ceiling_key` picks the ROOM (`material_ceiling`) or the
## every-turn regrowth (`hold_material_ceiling`), the same pair `expected_yield_account` chooses
## between.
##
## **The two vectors are unioned by id, never zipped by position.** They come off the same species so
## they list the same materials today, but a missing term must read as `0` — a rate with no ceiling is
## a herd standing at its floor, which takes nothing and correctly renders no row.
static func expected_materials(workers: float, forecast: Dictionary,
        ceiling_key: String) -> Array[Dictionary]:
    var ceilings: Dictionary = {}
    for row_variant in forecast.get(ceiling_key, []):
        var row: Dictionary = row_variant
        ceilings[String(row.get(MATERIAL_PAYOFF_ID_KEY, ""))] = \
            float(row.get(MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
    var out: Array[Dictionary] = []
    for row_variant in forecast.get(FORECAST_PER_WORKER_MATERIAL_KEY, []):
        var row: Dictionary = row_variant
        var material_id := String(row.get(MATERIAL_PAYOFF_ID_KEY, ""))
        var crew := workers * float(row.get(MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
        out.append({
            MATERIAL_PAYOFF_ID_KEY: material_id,
            MATERIAL_PAYOFF_AMOUNT_KEY: minf(crew, float(ceilings.get(material_id, 0.0))),
        })
    return out

## **THE RAID ROW'S OWN KEYS.** A forecast row is what it always was — the sim's forward-simulated
## answer for one (floor, party) — and it still spells `floor` / `party_workers` / `turns_to_fill` /
## `bound` / `delivers_*` / `animals_taken` / `delivered_*` / `wasted_food` exactly as the snapshot
## table did. **What moved is where the row COMES FROM**, not what a row is: `native/src/bridge/query.rs`
## decodes the reply under the same names, so every reader below is unchanged from the table era.
##
## **NOTHING HERE IS ROUNDED, AND NO SENTENCE APOLOGISES FOR A ROUNDING.** A row answers the exact
## party and floor the sheet composed, so the take it states is the take of the raid on screen. Any
## copy that names a party OTHER than the composed one is describing a contract that no longer exists.

## The raid `workers` from `band` deliver hunting `herd` at `floor`, read off the ANSWER the sheet
## asked for — ZERO arithmetic: the sim grabs the herd's standing surplus above the floor in a burst
## and reports the whole animals it lands (`animals_taken`) and the turns until the party comes home
## (`turns_to_fill`, NOT "turns to fill the pack"). The ecology/MSY model is never reproduced here,
## and unlike a resident band's ceiling it cannot be: the trip is a bounded forward simulation with no
## closed form, which is exactly why the sim answers this one rather than exporting terms for it. The
## launch command sends the same floor the question carried. Returns {available, denial, empty,
## animals, turns, food, long_raid, slow}: `available` false = there is no answer to read yet (in
## flight, refused, or a non-huntable herd → the caller shows no forecast at all).
static func hunt_trip_forecast(band: Dictionary, herd: Dictionary, estimate: Dictionary,
        grid_width: int, wrap_horizontal: bool) -> Dictionary:
    if estimate.is_empty():
        return {"available": false}
    # A DENIAL mission carries nothing home at all. `delivers_food == false` says the QUARRY IS
    # INEDIBLE (issue #337), and Eradicate on a deer banks a whole-stock windfall like every other rung
    # rather than landing here.
    #
    # **THE `delivers_trade` HALF OF THIS TEST WENT WITH THE TRADE AXIS** (arc #527). An inedible quarry
    # used to read `delivers_food false, delivers_trade true` and count as a real delivery; what it
    # really pays is MATERIALS, and the raid wire states no figure for those, so such a raid reads as a
    # denial mission until a per-raid material payload exists to quote.
    if not bool(estimate.get("delivers_food", false)):
        return {"available": true, "denial": true, "empty": false}
    # **WHICH STOP ENDS THIS SAMPLED TRIP**, off the row rather than inferred from the numbers here.
    #
    # **IT IS READ BEFORE THE EMPTY BRANCH BECAUSE THE EMPTY BRANCH IS WHAT NEEDS IT MOST** — an empty
    # raid is empty for one of three unrelated reasons and only the sim can tell them apart; see
    # `HUNT_EMPTY_REFUSALS`.
    var bound := String(estimate.get(TRIP_BOUND_KEY, TRIP_BOUND_NONE))
    # Nothing delivered = the party comes home with nothing, whatever the reason. The ONE non-viable
    # case. NOT `animals_taken == 0`: a party too small to carry a whole animal now KILLS one and hauls the
    # fraction its pack holds (mirroring the local hunt), so `animals_taken >= 1` whenever there's any
    # surplus — the delivered PAYLOAD (with waste) is the honest bind, not the whole-animal kill count.
    #
    # **THE ARITHMETIC IS STILL RIGHT; WHAT MOVED IS THE EXPLANATION.** This branch once asserted the
    # herd was at its floor, because before the take resolved through the fight that was the only way
    # to land here. It is not any more, so the `bound` travels out and `HUNT_EMPTY_REFUSALS` says which
    # of the herd and the party the player has to fix.
    var delivered_food := float(estimate.get("delivered_food", 0.0))
    if delivered_food <= 0.0:
        return {"available": true, "denial": false, "empty": true, TRIP_BOUND_KEY: bound}
    var animals := int(estimate.get("animals_taken", 0))
    # `turns_to_fill == RAID_TURNS_UNBOUNDED` = the raid ran the whole horizon still delivering (a long
    # raid), and since the floor-0 fix that is `horizon` and nothing else — a `herd_lost` raid completes
    # and reports its turn, so it lands on the bounded branch below. A warn threshold of 0 means the
    # server sent none — report the raid, judge nothing. `turns_to_fill` counts HUNTING turns only; the
    # band-relative round trip is added on top so the headline is honest.
    var hunt_turns := int(estimate.get("turns_to_fill", RAID_TURNS_UNBOUNDED))
    var long_raid: bool = raid_is_unbounded(hunt_turns)
    var travel := round_trip_travel_turns(band, herd, grid_width, wrap_horizontal)
    var total := hunt_turns + travel
    var warn_turns := int(band.get("expedition_viability_warn_turns", 0))
    var slow: bool = not long_raid and warn_turns > 0 and total > warn_turns
    # **THE FLOOR ON AN UNBOUNDED RAID, IN THE SAME SPAN `total` IS IN.** The horizon bounds the HUNTING
    # only, so the trip's floor is it PLUS the very round trip added one line above — quoting the horizon
    # alone would understate the trip by the whole walk. Zero on a bounded raid (there is a real total)
    # and zero when the band carries no horizon (nothing to quote), which `raid_floor_is_known` reads.
    var horizon := forecast_horizon_turns(band)
    var hunt_turns_floor := horizon if long_raid else FORECAST_HORIZON_UNKNOWN
    var turns_floor := (hunt_turns_floor + travel) if hunt_turns_floor > FORECAST_HORIZON_UNKNOWN \
        else FORECAST_HORIZON_UNKNOWN
    # Waste fraction: killed-but-not-carried food over total killed. A small party on big game raids one
    # animal and hauls only the pack's worth, wasting the rest — a high % here is informative, not a block.
    var wasted_food := float(estimate.get("wasted_food", 0.0))
    var killed := delivered_food + wasted_food
    var waste_pct := (wasted_food / killed) if killed > 0.0 else 0.0
    return {
        "available": true, "denial": false, "empty": false,
        "animals": animals, "turns": total, "hunt_turns": hunt_turns, "travel": travel,
        "long_raid": long_raid, "slow": slow, TRIP_BOUND_KEY: bound,
        RAID_TURNS_FLOOR_KEY: turns_floor, RAID_HUNT_TURNS_FLOOR_KEY: hunt_turns_floor,
        # The delivered PAYLOAD in food — what the party actually LANDS (a partial for a small party),
        # straight from the sim's forward-simulated raid, NOT animals × food_per_animal (which counts the
        # whole kill and overstates a partial). It is > 0 here (empty returned above otherwise).
        "food": delivered_food, "waste_pct": waste_pct,
    }

## Render a `hunt_trip_forecast` result as its one-line BBCode readout — the three states in their
## three colors (cyan viable / amber too-slow / red returns-empty), or "" when the forecast isn't
## available (a herd with no exported estimate → the caller shows no line at all). SHARED by both hunt-expedition entry
## points: the targeting banner (band-first flow) and the herd panel's live compose block (herd-first
## flow), so the two can never drift apart.
static func hunt_forecast_line_bbcode(forecast: Dictionary, herd_name: String) -> String:
    if not bool(forecast.get("available", false)):
        return ""
    # A denial mission brings nothing home BY DESIGN — say what it does, amber, no payload. It is the
    # QUARRY that decides this (pays neither product), never the Eradicate rung, which delivers.
    if bool(forecast.get("denial", false)):
        return "[color=#%s]%s[/color]" % [
            HudStyle.WARN_HEX, HUNT_FORECAST_DENIAL_FORMAT % herd_name,
        ]
    # The raid comes home with nothing — the ONE non-viable case (red). WHICH refusal it is comes off
    # the sim's `bound`, never off these numbers: the herd being spent and the party being unable to
    # make the kill are the same zero with opposite remedies.
    if bool(forecast.get("empty", false)):
        return "[color=#%s]%s%s[/color]" % [
            HudStyle.DANGER_HEX, HUNT_FORECAST_WARN_GLYPH,
            String(hunt_empty_refusal(forecast)["line"]) % herd_name,
        ]
    # A real raid: headline the delivered PAYLOAD (the animal count over turns + what it LANDS), then
    # the waste. The payload is `delivered_food`, named only when the quarry actually pays it — so an
    # Eradicate deer raid quotes its windfall rather than a "~0 food".
    var animals := int(forecast.get("animals", 0))
    var food := _raid_payload_suffix(forecast)
    # The waste % rides BELOW the food as its own WARN-amber segment (even on a cyan line — a high-waste
    # partial is informative, not a block). Empty when the raid carried its full kill home.
    var waste := ""
    var waste_pct := float(forecast.get("waste_pct", 0.0))
    if waste_pct > 0.0:
        waste = "[color=#%s]%s[/color]" % [
            HudStyle.WARN_HEX, HUNT_WASTE_SUFFIX_FORMAT % int(round(waste_pct * 100.0))]
    if bool(forecast.get("long_raid", false)):
        # Ran the whole horizon still delivering — a slow but real haul (amber). No exact total, so the
        # line quotes the FLOOR (`horizon + travel`) in the bounded form's own span and shape; without a
        # horizon on the wire there is no floor and it falls back to the hedge.
        var long_travel := int(forecast.get("travel", 0))
        var long_text: String
        if raid_floor_is_known(forecast):
            long_text = HUNT_FORECAST_LONG_RAID_FORMAT % [
                animals, herd_name, int(forecast.get(RAID_TURNS_FLOOR_KEY, 0))]
            if long_travel > 0:
                long_text += HUNT_FORECAST_LONG_TRAVEL_BREAKDOWN % [
                    int(forecast.get(RAID_HUNT_TURNS_FLOOR_KEY, 0)), long_travel]
        else:
            long_text = HUNT_FORECAST_LONG_RAID_NO_HORIZON_FORMAT % [animals, herd_name]
            if long_travel > 0:
                long_text += HUNT_FORECAST_LONG_TRAVEL_SUFFIX % long_travel
        return "[color=#%s]%s%s%s[/color]%s" % [
            HudStyle.WARN_HEX, long_text, food, HUNT_FORECAST_SLOW_SUFFIX, waste,
        ]
    # `turns` is the TOTAL (hunting + round-trip travel); the breakdown spells the split out when there's
    # travel to show — a band-relative addition the band-agnostic estimate table can't carry.
    var turns := int(forecast.get("turns", 0))
    var text: String = HUNT_FORECAST_DELIVERS_FORMAT % [animals, herd_name, turns]
    var travel := int(forecast.get("travel", 0))
    if travel > 0:
        text += HUNT_FORECAST_TRAVEL_BREAKDOWN % [int(forecast.get("hunt_turns", 0)), travel]
    # Slow raid (past the band's warn threshold) — still a real delivery, just a long one: amber, told
    # then trusted. A brisk raid reads income-cyan.
    if bool(forecast.get("slow", false)):
        return "[color=#%s]%s%s%s%s[/color]%s" % [
            HudStyle.WARN_HEX, HUNT_FORECAST_WARN_GLYPH, text, food, HUNT_FORECAST_SLOW_SUFFIX, waste,
        ]
    return "[color=#%s]%s%s[/color]%s" % [HudStyle.SIGNAL_HEX, text, food, waste]

## The raid's delivered payload as a trailing " · ~20 food" — rendered only when the quarry pays it,
## so "" when the forecast carries no payload at all. It carried a second, trade-goods component until
## arc #527 retired that account.
static func _raid_payload_suffix(forecast: Dictionary) -> String:
    var suffix := ""
    var food := float(forecast.get("food", 0.0))
    if has_component(food):
        suffix += HUNT_FORECAST_FOOD_FORMAT % int(round(food))
    return suffix

## The raid returns empty: the sim's estimate for THIS (floor, party size) delivers nothing. The
## single definition of the blocked case — both entry points (panel button + targeting
## click) gate on it. **It says THAT, never WHY** — `hunt_empty_refusal` is what answers why, and the
## two were one function for as long as there was only one why.
static func hunt_trip_returns_empty(forecast: Dictionary) -> bool:
    return bool(forecast.get("available", false)) and bool(forecast.get("empty", false))

## **DOES THIS TRIP HAVE A PAYLOAD TO PUT IN A READOUT?** The three states that do NOT — no estimate
## at all, a denial quarry that pays neither product, and a raid that comes home empty (whether the
## herd is spent or the party cannot make the kill) — each have exactly one thing to say and say it as
## a sentence (`hunt_forecast_line_bbcode`); only a delivering raid has
## an animal count, a yield vector and a trip length to lay out as rows. The compose sheet branches on
## this so a non-viable raid can never render an empty box, which would read as a raid that delivers
## nothing measurable rather than one that is refused.
static func hunt_trip_delivers(forecast: Dictionary) -> bool:
    return bool(forecast.get("available", false)) \
        and not bool(forecast.get("denial", false)) and not bool(forecast.get("empty", false))

## The trip's length as the readout's VERDICT — `{severity, text}`, the shape `HudWidgets`
## `build_verdict_line` renders for both webs. A raid has no crew-versus-floor contest to adjudicate
## (the party is fixed at launch), so what its verdict states is the one price every trip charges:
## how many turns these hands are away, and where those turns go. Severity is the SAME judgement the
## one-line form and the Send button already make — `slow` past the band's warn threshold, `long_raid`
## when the sim's estimate never bounded the trip — so the box, the sentence and the button cannot
## disagree about whether a raid is worth the wait.
##
## **AND IT NAMES WHICH STOP ENDS THE TRIP** (§5.2), because the length alone cannot: a raid that
## comes home on its fill target and one that comes home on the floor are different decisions wearing
## the same turn count. The clause is the SIM's answer (`TRIP_BOUND_CLAUSES` off `bound`), and a
## forecast that carries no bound — an estimate row from a snapshot that predates the field — renders
## the sentence it always did.
static func hunt_trip_verdict(forecast: Dictionary) -> Dictionary:
    var travel := int(forecast.get("travel", 0))
    var clause := trip_bound_clause(forecast)
    if bool(forecast.get("long_raid", false)):
        # The floor on the whole span, split exactly as the bounded verdict splits its total — so "Away
        # ≈36 turns — 18 hunting, 18 travel" and "Away more than 78 turns — more than 60 hunting, 18
        # travel" answer the same question and can be read against each other.
        var long_text: String
        if raid_floor_is_known(forecast):
            var floor_total := int(forecast.get(RAID_TURNS_FLOOR_KEY, 0))
            long_text = EXPEDITION_TRIP_LONG_VERDICT_SPLIT_FORMAT % [
                floor_total, int(forecast.get(RAID_HUNT_TURNS_FLOOR_KEY, 0)), travel] if travel > 0 \
                else EXPEDITION_TRIP_LONG_VERDICT_FORMAT % floor_total
        else:
            long_text = EXPEDITION_TRIP_LONG_VERDICT_TRAVEL_FORMAT % travel if travel > 0 \
                else EXPEDITION_TRIP_LONG_VERDICT
        return {
            "severity": VERDICT_SLOW,
            "text": _with_bound_clause(long_text, clause),
        }
    var turns := int(forecast.get("turns", 0))
    var text := EXPEDITION_TRIP_VERDICT_SPLIT_FORMAT % [
        turns, int(forecast.get("hunt_turns", 0)), travel] if travel > 0 \
        else EXPEDITION_TRIP_VERDICT_FORMAT % turns
    return {
        "severity": VERDICT_SLOW if bool(forecast.get("slow", false)) else VERDICT_OK,
        "text": _with_bound_clause(text, clause),
    }


## The trip's length and the stop that ends it, as one sentence — or the length alone when the bound
## has nothing to add (`""` = not stated; `horizon` = the length sentence already said it).
static func _with_bound_clause(text: String, clause: String) -> String:
    return text if clause == "" else "%s %s" % [text, clause]


## **WHICH STOP ENDS THIS TRIP, IN WORDS** — the sim's `bound` key through `TRIP_BOUND_CLAUSES`, and
## `""` for the two states with nothing to add. The readout box folds it into its verdict; the Band
## panel's dock sheet, whose forecast is the one-LINE form, renders it as its own quiet line beneath.
## One table, so the two surfaces cannot describe the same stop differently.
static func trip_bound_clause(forecast: Dictionary) -> String:
    return String(TRIP_BOUND_CLAUSES.get(
        String(forecast.get(TRIP_BOUND_KEY, TRIP_BOUND_NONE)), ""))

## **WHY THIS RAID COMES HOME EMPTY** — the `HUNT_EMPTY_REFUSALS` entry for the sim's own `bound`, i.e.
## the `{line, button, reason}` triple every surface of the refusal is composed from. THE ONE
## resolution of that key, so the sentence, the button face and the spelled-out reason are three faces
## of one answer rather than three lookups free to disagree.
##
## A bound the branch cannot explain — an estimate row that carries none, or one of the two party-side
## stops, which structurally cannot land here (both require a delivered load) — falls to the
## unattributed entry rather than to a guess. Guessing is the defect this exists to fix.
static func hunt_empty_refusal(forecast: Dictionary) -> Dictionary:
    var bound := String(forecast.get(TRIP_BOUND_KEY, TRIP_BOUND_NONE))
    return HUNT_EMPTY_REFUSALS.get(bound, HUNT_EMPTY_REFUSALS[TRIP_BOUND_NONE])

## The ONE sentence spoken about an empty raid — shared verbatim by the herd panel (reason line +
## disabled-button tooltip) and the Band panel's dock sheet, so the two entry points can never
## disagree. **It takes the FORECAST as well as the herd** because which sentence it is depends on the
## sim's `bound`: "wait for the herd to rebuild" and "send more hunters" are opposite instructions and
## a reason that names the wrong one is worse than no reason at all.
static func hunt_empty_refusal_reason(forecast: Dictionary, herd: Dictionary) -> String:
    return String(hunt_empty_refusal(forecast)["reason"]) % herd_display_name(herd)

# ---- THE DENIAL RAID's readout (`docs/plan_denial_raid.md` §1.1 / §3) ---------------------------

## What `workers` from this band do to `herd` on a DENIAL raid — the reply's row for the composed party,
## plus the ONE term the sim's answer does not carry (the outbound walk). Returns
## `{available, outcome, turns, low, high, travel, animals, food, wasted}`.
##
## **THE TURN COUNTS ARE FROM LAUNCH, AND THE OUTBOUND WALK IS WHY** (reported from play). The sim's
## `turns_to_collapse` counts turns of RAIDING — the party has to reach the herd before it can kill
## anything — so a bare "≈5–8 turns" beside a hunt line that HAS always added its round trip made two
## missions on one sheet quote turn counts meaning different things. Each bounded end therefore gains
## the outbound leg, and `DENIAL_SPAN_FROM_LAUNCH` names the span out loud.
##
## **THE RETURN LEG IS DELIBERATELY NOT IN IT.** The verdict is about the HERD crossing the point of no
## return, which happens on the range the moment the party is there and killing; the walk home comes
## after the event and is not part of the span the sentence is about. A hunt is the opposite case — its
## payload only counts once carried home — which is exactly why it adds the whole round trip.
##
## **NO `band` = NO TRAVEL TERM, AND THAT IS A STATED SPAN, NOT A DEFAULT.** A launched party's
## remaining walk is unknowable here (the sim publishes no per-party arrival for a denial mission), so
## the in-flight caller passes no band, the forecast carries `DENIAL_TRAVEL_UNKNOWN`, and the sentence
## says "of raiding" instead of "from launch". Both surfaces name their span; neither is bare.
##
## `available == false` = the snapshot carries no denial row for this party size (a non-huntable herd,
## a party larger than the sim sampled) → the caller renders NO verdict at all rather than a blank.
## **`horizon_cohort` IS FOR THE CALLER WITH NO BAND, AND IT IS READ FOR THE HORIZON ONLY.** The forecast
## horizon is a global lever echoed onto EVERY cohort, so the launch sheet's `band` already answers it and
## passes nothing here; the in-flight drawer, which deliberately passes no band (see the travel note
## above), hands in the launched PARTY's own cohort. It is never consulted for travel — a launched party's
## remaining walk is not on the wire, which is the whole reason that caller passes no band.
static func denial_forecast(herd: Dictionary, row: Dictionary, band: Dictionary = {},
        grid_width: int = 0, wrap_horizontal: bool = false,
        horizon_cohort: Dictionary = {}) -> Dictionary:
    if row.is_empty():
        return {"available": false}
    var travel := DENIAL_TRAVEL_UNKNOWN if band.is_empty() \
        else outbound_travel_turns(band, herd, grid_width, wrap_horizontal)
    return {
        "available": true,
        # The sim's own key, carried through untouched — every branch below asks THIS, never the
        # numbers, because a `0` turn count is reachable from two unrelated outcomes.
        "outcome": String(row.get("outcome", DENIAL_OUTCOME_NONE)).strip_edges().to_lower(),
        # Each end shifted onto the launch clock, and `0` (beyond the horizon on that end) left alone —
        # see `_denial_turns_from_launch`.
        "turns": _denial_turns_from_launch(
            int(row.get("turns_to_collapse", DENIAL_TURNS_BEYOND_HORIZON)), travel),
        "low": _denial_turns_from_launch(
            int(row.get("turns_to_collapse_low", DENIAL_TURNS_BEYOND_HORIZON)), travel),
        "high": _denial_turns_from_launch(
            int(row.get("turns_to_collapse_high", DENIAL_TURNS_BEYOND_HORIZON)), travel),
        DENIAL_TRAVEL_KEY: travel,
        # The lever off whichever cohort the caller had — the band on a launch sheet, the party in flight.
        DENIAL_HORIZON_TURNS_KEY: forecast_horizon_turns(
            horizon_cohort if not horizon_cohort.is_empty() else band),
        "animals": int(row.get("animals_killed", 0)),
        "food": float(row.get("delivered_food", 0.0)),
        # The FOOD wasted — killed and left standing dead on the range. It carried a trade twin until
        # arc #527 retired that account; what a kill is worth beyond meat is materials, which this row
        # states no figure for.
        "wasted": float(row.get("wasted_food", 0.0)),
    }

## A collapse turn count moved onto the clock the player is actually on — the raiding turns plus the
## walk out. **`DENIAL_TURNS_BEYOND_HORIZON` (`0`) is not a turn count and must not be shifted**: it
## means the projection never bounded that end, and `travel` turns of walking do not bound it either.
## An unknown travel term (`DENIAL_TRAVEL_UNKNOWN`, the in-flight caller) leaves the count as the sim
## stated it, which is what the "of raiding" clause then says.
static func _denial_turns_from_launch(turns: int, travel: int) -> int:
    if turns <= DENIAL_TURNS_BEYOND_HORIZON or travel <= 0:
        return turns
    return turns + travel

## **THE ONE RESOLUTION OF THE OUTCOME KEY** — the `{line, turns, button, severity, reason}` entry
## every surface of the verdict is composed from, so the sentence, the Send button's face and the
## spelled-out reason are three views of one answer. An outcome this table cannot explain falls to the
## unattributed entry rather than to a guess (the `hunt_empty_refusal` rule).
static func denial_verdict(forecast: Dictionary) -> Dictionary:
    var outcome := String(forecast.get("outcome", DENIAL_OUTCOME_NONE))
    return DENIAL_VERDICTS.get(outcome, DENIAL_VERDICTS[DENIAL_OUTCOME_NONE])

## **DID THIS PARTY'S RAID GET THERE?** — the ONE test over `DENIAL_SUCCESS_OUTCOMES`, so no reader
## has to spell the set and none can spell it as "not `repelled`". `horizon` answers FALSE: a
## projection that ran out is not a raid that worked, and treating it as one is what quoted a party
## that never breaks the herd as the party that does.
static func denial_outcome_succeeds(outcome: String) -> bool:
    return DENIAL_SUCCESS_OUTCOMES.has(outcome)

## **THE FIGURE THE SENTENCE LEADS ON — the EXPECTATION where the sim bounded it, the good run only
## where it did not.** `0` is "beyond the horizon" on that end and never a turn count, so an unbounded
## expectation falls through to `low`; both unbounded means the projection bounded nothing and there is
## nothing to lead with.
static func _denial_lead_turns(forecast: Dictionary) -> int:
    var turns := int(forecast.get("turns", DENIAL_TURNS_BEYOND_HORIZON))
    if turns > DENIAL_TURNS_BEYOND_HORIZON:
        return turns
    return int(forecast.get("low", DENIAL_TURNS_BEYOND_HORIZON))

## That figure as a phrase, or `""` when the forecast has no number to give — which is also the gate
## every caller uses to decide whether a verdict quotes a number at all (`DENIAL_ESTIMATE_CAVEAT`
## qualifies a figure, so it must not print where there is none). It wears `≈` because the band is a
## claim about many draws, not a promise about this one.
##
## **IT IS THE LEAD ALONE, NOT THE WHOLE RANGE.** The spread rides `denial_turns_clause`, which is what
## keeps "which number leads" from being answerable in two places.
static func denial_turns_phrase(forecast: Dictionary) -> String:
    var lead := _denial_lead_turns(forecast)
    if lead <= DENIAL_TURNS_BEYOND_HORIZON:
        return ""
    return DENIAL_TURNS_ONE_FORMAT % lead

## **THE TURN CLAUSE — the EXPECTATION, its SPAN, then the spread.** `""` when the forecast has no
## number to quote, so the outcome sentence stands alone. Five shapes, and the `0`-means-unbounded
## convention holds on every end:
##
## 1. all three bounded — `" in ≈20 turns from launch — between 12 and 31 depending on the run"`
## 2. `high` unbounded — `" in ≈47 turns from launch — as few as 12 on a good run, and a bad one may
##    not finish"`
## 3. the EXPECTATION unbounded — `" only on a good run — ≈12 turns from launch, and the raid is not
##    expected to finish inside the forecast"`
## 4. `low == high` — the distribution is degenerate, so the lead figure IS the whole answer and no
##    spread renders
## 5. nothing bounded — no clause at all
##
## **THE EXPECTATION LEADS WHEREVER IT EXISTS**, because every other number on the sheet is priced at
## it; leading with the lucky end described a different raid from the take line two rows down (reported
## from play). **AN UNBOUNDED END IS STATED, NEVER DROPPED** — the rule this replaced quoted `low`
## alone whenever `high` ran past the horizon, which is how a lone optimistic figure came to read as
## the answer. And no branch can produce a bare count: the span rides the lead figure in every one, the
## hunt readout on the same sheet quoting a round-trip TOTAL that an unqualified denial count read as.
##
## The travel split renders only where there is travel to split off — a band standing beside its quarry
## has none, and "(0 of them travel)" would be a term for nothing.
static func denial_turns_clause(forecast: Dictionary) -> String:
    var phrase := denial_turns_phrase(forecast)
    if phrase == "":
        return ""
    var travel := int(forecast.get(DENIAL_TRAVEL_KEY, DENIAL_TRAVEL_UNKNOWN))
    var span := denial_span(forecast)
    var turns := int(forecast.get("turns", DENIAL_TURNS_BEYOND_HORIZON))
    var low := int(forecast.get("low", DENIAL_TURNS_BEYOND_HORIZON))
    var high := int(forecast.get("high", DENIAL_TURNS_BEYOND_HORIZON))
    var clause := ""
    if turns <= DENIAL_TURNS_BEYOND_HORIZON:
        # The lead figure is the GOOD RUN (`_denial_lead_turns` fell through), so the sentence says so
        # and says the raid is not expected to finish — never the bare optimistic number.
        clause = DENIAL_ONLY_GOOD_RUN_LEAD_FORMAT % [phrase, span] + DENIAL_SPREAD_NOT_EXPECTED
    else:
        clause = DENIAL_TURNS_LEAD_FORMAT % [phrase, span]
        if low > DENIAL_TURNS_BEYOND_HORIZON and high > DENIAL_TURNS_BEYOND_HORIZON:
            # `low == high` is the degenerate distribution: the lead already IS both ends, and
            # "between 8 and 8" would be a spread for nothing.
            if low != high:
                clause += DENIAL_SPREAD_RANGE_FORMAT % [low, high]
        elif low > DENIAL_TURNS_BEYOND_HORIZON:
            clause += DENIAL_SPREAD_OPEN_HIGH_FORMAT % low
    if travel > 0:
        clause += DENIAL_TRAVEL_SPLIT_FORMAT % travel
    return clause

## The verdict as one plain sentence — the OUTCOME always, the turn phrase only when that outcome has
## one to quote and the sim bounded it. **The outcome leads and the number is a clause on it**, which
## is the structural form of "never render a blank turn count without its outcome": there is no branch
## in which the number can render alone, and none in which its absence renders as silence.
static func denial_verdict_text(forecast: Dictionary, herd_name: String) -> String:
    if not bool(forecast.get("available", false)):
        return ""
    var entry := denial_verdict(forecast)
    # An outcome whose SENTENCE carries the forecast's own length (the `horizon` verdict) composes it here
    # rather than through the turn clause: the clause states the collapse figures, and this outcome has
    # none — what it states is how long the projection ran before giving up.
    var bounded := _denial_bounded_line(entry, forecast, herd_name)
    var text := bounded if bounded != "" else String(entry["line"]) % herd_name
    if bool(entry["turns"]):
        text += denial_turns_clause(forecast)
    return text

## **WHICH CLOCK THIS SHEET IS QUOTING** — `from launch` where the outbound walk is known, `of raiding`
## where it is not (the in-flight drawer). The ONE resolution, so the turn clause and the horizon sentence
## cannot name two different spans in one verdict.
static func denial_span(forecast: Dictionary) -> String:
    return DENIAL_SPAN_OF_RAIDING \
        if int(forecast.get(DENIAL_TRAVEL_KEY, DENIAL_TRAVEL_UNKNOWN)) == DENIAL_TRAVEL_UNKNOWN \
        else DENIAL_SPAN_FROM_LAUNCH

## The outcome's sentence with the forecast's own LENGTH in it, or `""` when this outcome has no bounded
## form or the wire carried no horizon. The figure is shifted onto the launch clock by
## `_denial_turns_from_launch` — the same shift the collapse figures take — so the sentence and the clause
## beneath it are on one clock; where travel is unknown it stays the raiding-turn count and says so.
static func _denial_bounded_line(entry: Dictionary, forecast: Dictionary, herd_name: String) -> String:
    var format := String(entry.get("line_bounded", ""))
    var horizon := int(forecast.get(DENIAL_HORIZON_TURNS_KEY, FORECAST_HORIZON_UNKNOWN))
    if format == "" or horizon <= FORECAST_HORIZON_UNKNOWN:
        return ""
    var travel := int(forecast.get(DENIAL_TRAVEL_KEY, DENIAL_TRAVEL_UNKNOWN))
    return format % [herd_name, _denial_turns_from_launch(horizon, travel), denial_span(forecast)]

## …and the same sentence tinted: SIGNAL cyan for a raid that gets there, WARN amber for one that does
## not. It never reads DANGER — a denial raid that cannot break the herd is a bad bargain, not a
## refusal (it still launches and keeps working the herd until recalled).
static func denial_verdict_bbcode(forecast: Dictionary, herd_name: String) -> String:
    var text := denial_verdict_text(forecast, herd_name)
    if text == "":
        return ""
    var hex := HudStyle.SIGNAL_HEX if String(denial_verdict(forecast)["severity"]) == VERDICT_OK \
        else HudStyle.WARN_HEX
    return "[color=#%s]%s[/color]" % [hex, text]

## **THE WASTE, STATED AND NOT ALARMED ABOUT** — what the raid kills, the little it hauls home, and
## what it leaves dead on the range. Quiet ink, no `⚠`: on a hunt an unhauled kill is a mistake, on a
## raid it is the mission. `""` when the forecast has no take to describe.
static func denial_take_bbcode(forecast: Dictionary, herd_name: String) -> String:
    if not bool(forecast.get("available", false)):
        return ""
    var animals := int(forecast.get("animals", 0))
    if animals <= 0:
        return ""
    var text := DENIAL_TAKE_KILLS_FORMAT % [animals, herd_name]
    # The food only when the quarry actually pays it — the render-only-when-non-zero rule, so an
    # inedible quarry's raid states its kills alone rather than a false `0.00 food`.
    var food := float(forecast.get("food", 0.0))
    if has_component(food):
        text += DENIAL_TAKE_FOOD_FORMAT % format_magnitude(food)
    # …and the waste under the same rule, so nothing here can render a fabricated `0.00`.
    var wasted := denial_waste_face(forecast)
    if wasted != "":
        text += DENIAL_TAKE_LEFT_FORMAT % wasted
    return "[color=#%s]%s[/color]" % [HudStyle.INK_DIM_HEX, text]

## **WHAT THE RAID LEAVES ON THE RANGE** — the subject of the take line's waste clause, and the ONE
## spelling of it, so any second surface that states a raid's waste states it in the same words. `""`
## when the forecast wastes nothing measurable, which is the caller's signal to render no clause at all
## rather than an empty one.
##
## It renders BARE, the take line's own order and the reading an edible quarry has always had, under
## the render-only-when-non-zero rule the delivered figure one line above already follows — so a wolf
## pack (which binds no carry and therefore wastes nothing) renders no clause instead of an
## honest-looking zero. It stated a second, trade-goods term until arc #527 retired that account.
static func denial_waste_face(forecast: Dictionary) -> String:
    var food := float(forecast.get("wasted", 0.0))
    return format_magnitude(food) if has_component(food) else ""

## **THE ONE READING OF `DenialRaidForecastReply.party_needed`** — the smallest party the sim quotes whose raid actually
## SUCCEEDS in driving this herd past recovery (never a `horizon` row, which only means the projection
## ran out), `DENIAL_PARTY_NEEDED_NONE` when it quotes none. The stepper's seed and the
## repelled refusal's count BOTH come through here, so the control and the sentence beside it cannot
## disagree about the number. It is NOT a cap and may exceed the band's idle workers — that is the
## honest "you need more people than you have", and only the stepper, which knows the band, clamps it.
static func denial_party_needed(reply: Dictionary) -> int:
    return int(reply.get("party_needed", DENIAL_PARTY_NEEDED_NONE))

## The spelled-out reason a denial raid will not get there — `""` for the two outcomes that do, so the
## sheet renders no line rather than an empty one.
##
## **THE REPELLED REFUSAL NAMES THE PARTY THE SIM QUOTES, WHENEVER IT QUOTES ONE.** Which of the
## outcome's two reasons renders is decided by `denial_party_needed`, never by the wording: with a
## figure the sentence carries `[quarry, needed]`, without one it falls back to the numberless form
## that takes the quarry alone. An outcome with no counted variant (every other one) is unaffected.
static func denial_refusal_reason(forecast: Dictionary, herd: Dictionary, needed: int) -> String:
    var entry := denial_verdict(forecast)
    var counted := String(entry.get("reason_counted", ""))
    if counted != "" and needed > DENIAL_PARTY_NEEDED_NONE:
        return counted % [herd_display_name(herd), needed]
    var reason := String(entry["reason"])
    return "" if reason == "" else reason % herd_display_name(herd)

## **THE ONE CONDITION THAT DISABLES A DENIAL SEND: the band cannot field the party this herd
## REQUIRES.** Not "the chosen party is too small" — that is the player's call to under-size a raid and
## it is warned about, not blocked (see `style_send_denial_button`) — but "no party this band can put
## in the field reaches the requirement at all", which is a fact about the BAND and not a choice.
##
## **`DENIAL_PARTY_NEEDED_NONE` IS NOT SHORT-HANDED.** `0` is not "not enough hunters": per
## `snapshot.fbs` it also covers a quarry nothing can bring into contact (wariness ≥ 1), where more
## hands never help, and a requirement past the sim's quoting bound. There is no number to compare, so
## the verdict copy governs and the button behaves as it always has.
static func denial_is_short_handed(needed: int, idle: int) -> bool:
    return needed > DENIAL_PARTY_NEEDED_NONE and needed > idle

## …and the sentence that says so, `""` when the band is not short-handed. Both numbers, off the SAME
## `denial_party_needed` reading the stepper's seed and the repelled refusal use, so the sheet cannot
## disable a Send over one figure while quoting another.
static func denial_short_handed_reason(herd: Dictionary, needed: int, idle: int) -> String:
    if not denial_is_short_handed(needed, idle):
        return ""
    return DENIAL_SHORT_HANDED_REASON_FORMAT % [herd_display_name(herd), needed, idle]

## The denial Send button, off the SAME entry the verdict line came from. With no forecast at all (a
## party size the sim did not sample) it takes the plain primary face rather than a warning it cannot
## justify.
##
## **IT DISABLES IN EXACTLY ONE CASE, AND THE DISTINCTION IS THE WHOLE RULE.** A party the player has
## CHOSEN to under-size still launches: a raid that cannot break the herd keeps working it until it is
## recalled (§6 Q2), so a stepped-down `repelled` party warns and the player is trusted, exactly as a
## slow hunting raid is. That reasoning does not carry to a band that cannot field the required party
## AT ALL (`short_handed`) — there is no party to trust the player with, so the button goes
## visible-and-disabled-with-its-reason, the same shape as the sheet's no-quarry branch.
static func style_send_denial_button(button: Button, forecast: Dictionary,
        short_handed: bool = false) -> void:
    if short_handed:
        button.disabled = true
        button.text = DENIAL_SHORT_HANDED_BUTTON
        HudStyle.apply_button(button, "ghost")
        return
    button.disabled = false
    if not bool(forecast.get("available", false)):
        button.text = String(DENIAL_VERDICTS[DENIAL_OUTCOME_PAST_RECOVERY]["button"])
        HudStyle.apply_button(button, "primary")
        return
    var entry := denial_verdict(forecast)
    button.text = String(entry["button"])
    HudStyle.apply_button(button,
        "primary" if String(entry["severity"]) == VERDICT_OK else "armed")

## **THE SUPPLY SIDE OF THE PARTY STEPPER — the band's IDLE WORKFORCE, and nothing else.** What the
## band can spare is the only thing that bounds how many hunters may walk out of camp;
## `expedition_useful_cap` below is the DEMAND side (what the raid can actually use at the kill), and
## the stepper takes the tighter of the two. Kept as a named function rather than inlined because it is
## the seam TWO entry points read — the herd drawer's expedition branch and the dock's hunt form.
##
## **`max_expedition_party_size` IS NOT A RULES CAP, AND NOTHING IN THE CLIENT READS IT ANY MORE.** It
## was the per-cohort echo of how far the pre-launch estimate TABLES had been sampled, i.e. the point
## past which a preview had to quote a smaller party's numbers and say so. There is no sampled axis
## left to declare: a sheet asks for the party it has composed and is answered for that party, and the
## `max_party_workers` it sends is the band's own idle workforce, which is what bounds the sim's
## contiguous search. The sim holds no rules cap on any of the three launch verbs either, so a clamp
## here would be the client enforcing a limit that exists nowhere.
static func expedition_party_cap(band: Dictionary) -> int:
    return int(band.get("idle_workers", 0))

## **THE PARTY THAT CAN REACH THIS HERD'S STANDING SURPLUS** — `engage_workers` over the room above the
## floor, in the room's own BIOMASS units (the quotient is a ratio, so the units are free exactly as
## they are for the local cap's account-denominated call). The raid twin of the floor
## `max_useful_workers` takes through `take_workers`, and it reuses the SAME primitive rather than
## restating it: a second definition of the engagement crew is precisely what let the two sheets drift.
##
## **ONLY THE ENGAGE HALF OF `take_workers`, and that is deliberate.** The haul half is sized on
## `perWorkerBiomass`, a RESIDENT crew's throughput; a raid hauls in its PACK
## (`expedition.hunt.per_worker_carry`), which is not on the wire — and the pack side is exactly what
## the plateau scan already watches the delivered payload run into. Engagement is the arm the scan
## cannot see, so it is the arm this adds.
##
## **A DETACHED PARTY BUILDS NOTHING**, so the dip is `IMPROVEMENT_NONE`'s — resolved through
## `build_dip` rather than written as a bare 1.0, so "not building" stays one value in one place.
## `0` for a herd with no engagement stage (a pen; a species the roster cannot resolve) and for one
## with no body to count, which is what leaves every raid predating this field byte-identical.
static func expedition_engage_crew(herd: Dictionary, floor: float) -> int:
    # A raw herd dict carries its forecast fields BARE — the `patch_` prefix belongs to the tile_info
    # cross-ref, and this layer never reads the compose vocabulary that names it.
    var prefix := ""
    return engage_workers(escapement_room(herd, prefix, floor),
        float(herd.get(prefix + FORECAST_BODY_MASS_KEY, 0.0)),
        float(herd.get(prefix + FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE)),
        build_dip(herd, prefix, IMPROVEMENT_NONE),
        # The party's OWN retreat: `advance_expeditions` runs `HuntParty::stayers` exactly as a
        # resident hunt does, and the herd handed in here is already kit-priced, so this is the
        # effective stay fraction under the raid's chosen kit rather than the quarry's bare wariness.
        float(herd.get(prefix + FORECAST_STAY_FRACTION_KEY, STAY_FRACTION_NONE_BREAKS_OFF)))

## The max-useful party for a raid, from the reply's own plateau scan plus the one thing the scan
## cannot see. `{cap, note}`, the raid twin of `_forecast_worker_cap` — same shape, same "max N useful"
## note, so the expedition and local pickers explain a dead `+` the same way.
##
## **ONLY THE SCAN MOVED SERVER-SIDE.** `scanned` is `HuntTripForecastReply.useful_cap`: the LAST party
## at which the delivered payload was still RISING, walked CONTIGUOUSLY from 1 to the band's own idle
## workers. It used to be a client scan over the snapshot table's sampled rungs, which could only ever
## find a sampled plateau — the last rung still rising, not the last PARTY still rising. `0` means the
## scan found none (the payload never rose above zero, or was still rising at the band's last worker),
## and reads exactly as the old no-plateau case did.
##
## **IT IS THE LARGEST PARTY STILL WORTH SENDING, SO THIS SEEDS AND CLAMPS *ON* IT — never one above.**
## `useful_cap + 1` is by construction the first party that adds nothing (the sim asserts both sides of
## that: the payload rises into `useful_cap` and does not rise past it), so treating the figure as
## "the first useless party" and stepping under it would strand a raid one worker short of its own
## plateau, on a sheet that renders perfectly happily either way.
##
## **THE ENGAGEMENT ARM STAYS HERE, and it is why this function still exists**
## (`docs/plan_hunt_through_combat.md` 2). A scan can only report a bind it can WATCH the payload run
## into; the crew that brings a quarry into CONTACT is derivable from fields the herd row already
## carries (`expedition_engage_crew`), and a herd needing six hunters per animal with four animals
## standing wants ~30 while every small party delivers nothing at all. It is a FLOOR on the demand
## side, never a cap — `assignable` still binds below — so the note names the ceiling the player is
## working toward instead of calling the missing hands idle. That is the reading that fixed the sheet
## saying *"max 1 worker useful here"* two lines above *"6 hunters bring one Wild Aurochs into
## contact"*.
static func expedition_useful_cap(band: Dictionary, herd: Dictionary, floor: float,
        scanned: int, assignable: int) -> Dictionary:
    var plateau: int = maxi(scanned, expedition_engage_crew(herd, floor))
    if plateau <= 0:
        return {"cap": assignable, "note": ""}
    var useful: int = mini(plateau, assignable)
    if useful >= assignable:
        # Labor-bound below the plateau: the party capped at what you can field, not at usefulness.
        # **THERE IS ONLY ONE SUPPLY CONSTRAINT**, `assignable` — the band's idle workforce — so
        # freeing idle workers is ALWAYS the remedy and there is only one note.
        var labor_note := ""
        if plateau > assignable:
            labor_note = LABOR_BOUND_NOTE_FORMAT % [assignable, plateau]
        return {"cap": assignable, "note": labor_note}
    var noun := MAX_USEFUL_NOUN_ONE if useful == 1 else MAX_USEFUL_NOUN_MANY
    return {"cap": useful, "note": MAX_USEFUL_NOTE_FORMAT % [useful, noun]}

## Each FLOOR PRESET's obtainable rate as a raid — the expedition twin of the local hunt's per-preset
## cap, so all three pickers (forage / local hunt / expedition) wear the same face and the presets read
## DESCENDING in take (strip it > the food peak > learn from it: a lower floor frees more surplus).
##
## **ONE ROW PER PRESET, ANSWERED IN THE SAME ROUND TRIP** — `HuntTripForecastReply.per_preset`, in the
## order the presets were asked for, which is `FLOOR_PRESETS`. The metric is
## `delivered / (turns_to_fill + round-trip travel)`, so a far herd's rate is correctly lower.
##
## **IT IS THIS PARTY'S RATE NOW, NOT THE BEST OVER ALL PARTY SIZES.** The table era scanned every
## sampled party at a preset's floor and took the maximum, because the table was there and a
## worker-independent face was cheap. A query answers the party the sheet has composed, and quoting a
## rate for a party the player is not sending is the exact class of error this arc removed — so the
## buttons now move with the crew stepper, which is honest and is the visible behaviour change.
##
## The FOOD component rides the metric, read only when the quarry pays it — a preset that lands
## nothing carries no rate and falls back to its name + glyph. (A second, trade-goods component rode
## beside it until arc #527 retired that account; an inedible quarry's presets are consequently blank,
## the raid wire stating no material payload for them to quote.) An UNBOUNDED raid has no length and
## its travel is not one, so it is skipped outright rather than quoted as `delivered / travel`.
static func expedition_policy_takes(band: Dictionary, herd: Dictionary, per_preset: Array,
        grid_width: int, wrap_horizontal: bool) -> Dictionary:
    var takes := {}
    var travel := round_trip_travel_turns(band, herd, grid_width, wrap_horizontal)
    var zero_account := zero_account_of(herd, "")
    for index in mini(per_preset.size(), FLOOR_PRESETS.size()):
        var row_variant: Variant = per_preset[index]
        if not (row_variant is Dictionary):
            continue
        var row := row_variant as Dictionary
        var hunt_turns := int(row.get("turns_to_fill", RAID_TURNS_UNBOUNDED))
        if raid_is_unbounded(hunt_turns):
            continue
        var trip_turns := hunt_turns + travel
        if trip_turns <= 0:
            continue
        var food := 0.0
        if bool(row.get("delivers_food", false)):
            food = maxf(0.0, float(row.get("delivered_food", 0.0)) / float(trip_turns))
        if food > 0.0:
            takes[String(FLOOR_PRESETS[index])] = extractive_take_pair(food, 0.0, zero_account)
    return takes

## **THE FLOORS THE PRESET ROW IS ASKED FOR**, in `FLOOR_PRESETS` order — which is the order the reply
## answers in and therefore the order `expedition_policy_takes` reads back. One list, so the ask and
## the read cannot index the presets differently.
static func preset_floors() -> Array:
    var floors: Array = []
    for preset in FLOOR_PRESETS:
        floors.append(floor_for_preset(String(preset)))
    return floors

## Style the hunt-expedition send button from the live forecast. Two treatments, and the line between
## them is the point:
##   DELIVERING (viable / slow / long / denial) — the raid lands something (animals, or the denial it
##     promises). "primary" for a brisk raid; "armed" amber for a slow/long raid (`Send Anyway (≈54
##     turns)` / `Send Anyway (long raid)`) or a denial (`SEND_HUNT_DENIAL_BUTTON`) — ENABLED either
##     way: the player is told, then trusted.
##   RETURNS EMPTY (nothing delivered in either currency) — a mistake with no upside. DISABLED, with
##     the reason and the way out, both keyed off the sim's `bound` so the face and the sentence above
##     it name the SAME culprit.
## No confirm dialogs either way.
static func style_send_hunt_button(button: Button, forecast: Dictionary, reason: String) -> void:
    # RETURNS EMPTY — the one blocked case. Disabled, and it says WHY plus what to do instead (the button
    # is the last thing the player looks at before clicking, so the reason belongs on it). **Its FACE comes
    # from the same `HUNT_EMPTY_REFUSALS` entry the line and the reason do**: a button reading "Herd too
    # lean to raid" under a line naming the PARTY is the same misattribution one control further on.
    if hunt_trip_returns_empty(forecast):
        button.text = String(hunt_empty_refusal(forecast)["button"])
        button.disabled = true
        button.tooltip_text = reason
        HudStyle.apply_button(button, "ghost")
        return
    if bool(forecast.get("denial", false)):
        # Nothing comes home in either currency, but that IS the mission — state the deal, don't cry
        # failure. NOT keyed on the Eradicate rung: an Eradicate deer raid banks a windfall and lands here
        # as a normal delivery.
        button.text = SEND_HUNT_DENIAL_BUTTON
        HudStyle.apply_button(button, "armed")
        return
    if bool(forecast.get("long_raid", false)):
        # The FLOOR on the trip, in the same clause the slow face states its total — so the last control
        # the player looks at before clicking carries a number rather than the word "long".
        button.text = (SEND_HUNT_LONG_RAID_FORMAT % int(forecast.get(RAID_TURNS_FLOOR_KEY, 0))) \
            if raid_floor_is_known(forecast) else SEND_HUNT_LONG_RAID_BUTTON
        HudStyle.apply_button(button, "armed")
        return
    if bool(forecast.get("slow", false)):
        button.text = SEND_HUNT_ANYWAY_TURNS_FORMAT % int(forecast.get("turns", 0))
        HudStyle.apply_button(button, "armed")
        return
    # A brisk, delivering raid (or no forecast at all — older server): the plain primary send.
    button.text = SEND_HUNTING_EXPEDITION_BUTTON
    HudStyle.apply_button(button, "primary")
