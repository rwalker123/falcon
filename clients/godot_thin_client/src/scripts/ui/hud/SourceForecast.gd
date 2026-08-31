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
# **THE ROUTE BRANCH'S TWO VERBS** (arc #532). `grade` raises a worn trail to a dirt road, `pave`
# raises a dirt road to a paved one — and both NAME A BAND as well as a tile, because a road has no
# work row for its keeper to be inferred from. See `Main.format_improvement`.
const IMPROVEMENT_GRADE := "grade"
const IMPROVEMENT_PAVE := "pave"
# The three ladders, each in RUNG ORDER (low → high). Kept apart because no two webs share a rung,
# and read by nothing that needs "all six".
const FORAGE_IMPROVEMENTS := [IMPROVEMENT_CULTIVATE, IMPROVEMENT_SOW]
const HUNT_IMPROVEMENTS := [IMPROVEMENT_TAME, IMPROVEMENT_CORRAL]
const ROUTE_IMPROVEMENTS := [IMPROVEMENT_GRADE, IMPROVEMENT_PAVE]
# **THE FENCE RING'S JOB TOKEN, AND IT IS NOT A RUNG.** `snapshot::population::resolved_build_job`
# publishes this in the `improvement` slot for a queue entry whose declared job is
# `BuildJob::ExtendPen`: a ring widens the pen rung its herd already stands on, so there is no meter
# for a rung verb to name and the entry publishes the command's own name instead. It is therefore
# absent from `HUNT_IMPROVEMENTS` and from every `FORECAST_BUILD_*_KEYS` table — the ring's meter is
# the herd's own `pen_extend_progress` / `pen_extend_cost` pair, read by `pen_extend_fraction`. This
# token is how a reader tells a RING entry from a RUNG entry with what is already on the wire.
const BUILD_JOB_EXTEND_PEN := "extend_pen"
# A herd at or above this domestication progress is fully tamed (pastoral); its crew are keepers.
const DOMESTICATION_COMPLETE := 1.0
# WHICH KIND OF SOURCE a forecast dict describes, stated explicitly by every `forecast_inputs` caller:
# a herd and a raw wire forage patch share the empty key prefix, so the prefix cannot answer it and a
# shape test on a wire key would misread a source whose snapshot omitted it.
const SOURCE_KIND_HERD := "herd"
const SOURCE_KIND_FORAGE := "forage"
## ⛔ **THE THIRD BRANCH'S SOURCE KIND, AND `source_kind_for_labor` DELIBERATELY DOES NOT ANSWER IT.**
## That function is a two-way alias over the two FOOD WEBS — its `else` is `SOURCE_KIND_HERD`, so a
## road handed to it comes back an animal and every keyed readout after it names the wrong pool. A
## road has no labor row to be routed FROM, so there is nothing for that alias to widen; what needs
## the kind is the queue row's price clause, which resolves it from the entry's own `roadwork` kind.
const SOURCE_KIND_ROUTE := "route"

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
# **THE ROW'S TWO RATES, EACH NAMED** — `+1.96 a turn on average · +1.91 this turn`. The row's FACE is
# `realizedYield`, the forward projection of this source's take; the number beside it is
# `actualYield`, the take the sim resolved THIS turn. They are different quantities and routinely
# differ by the width of the projection window — a patch sitting at its MSY inflection reads `+1.96`
# against `+1.91`, which is the window and not an error — so the line names BOTH rather than
# reconciling them.
#
# **IT READ `Actual %s`, AND THE WORD WAS THE DEFECT.** Quoting one of the two as *the actual*
# asserts the face is something other than actual while naming neither quantity. It is a survivor of
# the pre-`realizedYield` world, when the two WERE one number and the word was merely redundant.
#
# **ITS RATES ARE `format_signed`, NOT `format_yield`.** The unit is carried by the WORDS on both
# sides — *a turn on average*, *this turn* — so the `/turn` suffix would print `+1.96 /turn a turn on
# average`, i.e. the same unit twice in four words.
const YIELD_TOOLTIP_RATES_FORMAT := "%s a turn on average · %s this turn"
# Overstaffing (wasted labor) — DISTINCT from the ⚠ overdraw flag. Every policy caps a source's take at
# its ceiling (policy ceiling / resource biomass), so past `workers_needed` extra workers produce
# nothing HERE and should move elsewhere. A source can be overstaffed while perfectly sustainable (and
# overdrawn while fully used), so this reads as its own WARN-tinted note on the row rather than
# borrowing the ⚠. `workers_needed == 0` (rehydrated save) means "unknown" ⇒ no note, never a wrong one.
# **IT NAMES THE CONSEQUENCE, NOT THE ACTIVITY** (`docs/plan_standing_upkeep.md` §4.7a). It read
# `· only %d of %d working`, and *working* was read as *working ON WHAT* — a player with three
# foragers on a patch at its floor took it for a build claim and reported the row as nonsense
# (*"the Terrain tile says 2 of 3 workers, that doesn't make sense on a forage, I've never tried
# cultivating this tile"*). **The arithmetic was correct and is untouched**: `workers_needed_for_take`
# inverts the TAKE by the throughput the take actually ran at, so on a patch drawn down to its
# escapement floor two gatherers carry the whole regrowth and the third brings nothing home. Nothing
# on either side of it reads a build crew (`systems::labor`: *"both about the TAKE activity alone"*).
const OVERSTAFF_NOTE_FORMAT := " · only %d of %d bring anything home"
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
# The same "is it there at all" question asked in FODDER units — the pen feed-split's hay term
# (`fodder_draw`), the pen's own `pen_fodder_shortfall`, and the band hay ledger's `fodder_need` /
# `fodder_income` pair. Its OWN number rather than `FOOD_FLOW_MIN`'s, for two
# reasons: fodder and food are different units (fodder runs an order of magnitude coarser, being
# `fodder_per_biomass × biomass`), and the reader that gates on it prints ONE decimal. So this is HALF
# OF THE SMALLEST QUANTITY THAT READOUT CAN SHOW — the `COMPONENT_RENDER_MIN` argument at one decimal
# instead of two — which is what keeps the gate from admitting a draw it then renders as `hay 0.0`
# or a shortfall it then renders as `needs 0.0 /turn`.
const FODDER_FLOW_MIN := 0.05
# THE ONE DECIMAL EVERY FODDER READING PRINTS AT, and the number `FODDER_FLOW_MIN` above is defined as
# half of. Fodder is the coarse account — a stock reads in the hundreds where a food rate reads in
# hundredths — so a second decimal here claims a precision the units do not have, and a THIRD surface
# picking its own precision is how a stock and the rate that drains it stop looking like the same
# quantity. Spelled once here; `format_fodder` is the only renderer.
const FODDER_DECIMALS := 1
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
## Where `rescaled_accounts` hands back the MATERIAL vector it crossed off the same carried biomass.
## It is not an account name — the two above are, and a material's account IS its own id — so it is
## spelled apart from them and is not a plausible `materials.json` id (the same disjointness argument
## `yield_rows` already makes for `food` / `fodder`).
const RESCALED_MATERIALS_KEY := "material_rows"

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
## **IT SAYS `next turn`, BECAUSE THAT IS WHAT THE READING IS.** The rows are composed from the room
## next turn's take actually has (`escapement_room_next_turn`), so `per turn` was naming a rate for a
## figure that is one turn's answer — and at equilibrium the two coincide anyway, which is what made
## the old caption survive so long. `now → after` is unchanged: those are the crew buttons' own words
## (`clear it now` / `hold it after`) and they key the ARROW, not the span.
const YIELD_ROW_HEADER := "next turn"
const YIELD_ROW_HEADER_WITH_AFTER := "next turn · now → after"
## **THE ONE RESOLUTION OF THE ROW'S CAPTION**, over the single fact that can key it: do these
## readings carry a holding rate to arrow toward.
##
## **THE THIRD STATE — `per turn · while building` — IS RETIRED WITH THE DIP**
## (`docs/plan_standing_upkeep.md` §2.2). It said *these readings are the DIPPED take*, which was
## only ever true while ONE crew both gathered and built. The build has its own crew now, so a rung
## going up takes nothing off what the gatherers carry and the take under this caption is the plain
## one — a caption marking it as reduced would be describing arithmetic the sim no longer does.
## A caller with no per-turn rate AT ALL (the raid's whole-trip payload) supplies its own `header`
## and never reaches here.
static func yield_row_header(has_after: bool) -> String:
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
##
## **THE MATERIAL VECTOR CROSSES HERE TOO, OFF THE SAME `share`, AND THAT IS THE WHOLE OF WHY IT IS
## HERE.** `systems/labor.rs` banks food at `hunt_yield.apply(take.carried, …)` and materials at
## `credit_material_yield(…, take.carried, …)` — the identical local, one carried biomass — so a
## client that composed the two accounts from two expressions could quote a payout the sim does not
## make. It could and it did: `expected_materials` priced a hunt's materials as a pure crew-throughput
## line (`min(workers × per_worker_material, escapement ceiling)`), skipping the engagement→retreat arm
## and the whole-animal quantiser the food row applies, so a pastoral Wild Boar herd — `engage_rate`
## 0.33, hence exactly ONE animal reached at every crew from one to six — quoted five herders five
## times the bone and hide that one herder brings home, against a food row that (correctly) did not
## move at all. `share` IS `take.carried`, so the two accounts are now one derivation and the drift is
## unrepresentable rather than merely unlikely.
static func rescaled_accounts(src: Dictionary, prefix: String, value: float) -> Dictionary:
    var food_rate := float(src.get(prefix + FORECAST_PROVISIONS_PER_BIOMASS_KEY, 0.0))
    if food_rate <= 0.0:
        return {
            YIELD_ACCOUNT_FOOD: 0.0,
            YIELD_ACCOUNT_FODDER: 0.0,
            RESCALED_MATERIALS_KEY: [] as Array[Dictionary],
        }
    var out := rescaled_from_biomass(src, prefix, value / food_rate)
    # …and the counted account comes back BIT-IDENTICAL rather than through the divide-then-multiply
    # round trip: this entry point was HANDED that number, so restating it is the one thing it can do
    # that the biomass form cannot.
    out[YIELD_ACCOUNT_FOOD] = value
    return out

## **THE SAME CROSSING, ENTERED FROM THE BIOMASS SIDE — which is the side the sim is actually on.**
## `take.carried` is a BIOMASS, and `hunt_yield.apply` / `credit_material_yield` are two valuations of
## it; the food-keyed entry point above divides by the per-biomass rate to recover exactly this
## quantity, so it is written in terms of this one rather than beside it.
##
## **AND IT IS THE ONLY ONE AN INEDIBLE QUARRY CAN USE.** A wolf's `provisionsPerBiomass` is a
## structural `0`, so the division above is undefined and the food-keyed form can only answer zeros —
## which is what left a species hunted purely for its pelts with no crossing at all, and its material
## rows quoted off a parallel crew-throughput line instead. Biomass is what a hunt takes whether or
## not the species converts any of it to food.
static func rescaled_from_biomass(src: Dictionary, prefix: String, carried: float) -> Dictionary:
    return {
        YIELD_ACCOUNT_FOOD: carried
            * float(src.get(prefix + FORECAST_PROVISIONS_PER_BIOMASS_KEY, 0.0)),
        # No animal pays fodder, so a herd's second account rescales to a structural zero and renders
        # no row — the same answer the account had before this existed.
        YIELD_ACCOUNT_FODDER: carried
            * float(src.get(prefix + FORECAST_FODDER_PER_BIOMASS_KEY, 0.0)),
        # …and every material through the same one biomass. `scaled_material_rows` is the ONE
        # vector-times-a-scalar helper, so a material scales exactly as the two scalars beside it do.
        RESCALED_MATERIALS_KEY: scaled_material_rows(
            src.get(prefix + FORECAST_MATERIAL_PER_BIOMASS_KEY, []), carried),
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
# branch of its own. **BOTH WEBS PUBLISH IT** — a patch's basket is made of something too, and a
# stand of tobacco or flax pays that and no food at all — so one composition serves both and the
# patch's copy reaches the sheet `patch_`-prefixed through `MapView._tile_info_at`. The crop picker's
# per-RUNG material rows are a different question (what a COMMITTED patch would pay), not a
# substitute for this one.
const FORECAST_MATERIAL_PER_BIOMASS_KEY := "material_per_biomass"
# The per-worker twin — what ONE HUNTER brings home per turn, per material. The material sibling of
# `per_worker_yield`, so a band preview clamps `min(workers × rate, ceiling)` per material exactly as
# it does for food, and the build dip rides it exactly as it rides `per_worker`.
const FORECAST_PER_WORKER_MATERIAL_KEY := "per_worker_material"
# **THE RESOLVED YIELD, ON A LABOR ASSIGNMENT** — what this source actually credited to the band's
# `MaterialStore` this turn. Read through `material_rows_of`, never as a forecast: the sim seeds it
# EMPTY pre-commit by design (see there).
const ASSIGNMENT_MATERIAL_YIELD_KEY := "material_yield"
# **THE GOOD-SIDE SHORTFALL'S TWO TERMS, ON A LABOR ASSIGNMENT** (`docs/plan_standing_upkeep.md`
# §2.7) — what this row's SOURCE was billed in materials to hold its rung, and what the band's store
# actually paid. The sim publishes BOTH rather than their difference so the row's note can read
# *"Short of hurdles — 0.03 of the 0.05 a turn this pen eats"* with no client arithmetic.
#
# ⛔ **NEVER ADDED TO ANY WORK FIGURE.** The two accounts are billed and judged separately, which is
# what stops a full store papering over missing hands — and it is why a row can be short of a good
# with `upkeep_shortfall` at zero.
const ASSIGNMENT_MATERIAL_UPKEEP_DEMAND_KEY := "material_upkeep_demand"
const ASSIGNMENT_MATERIAL_UPKEEP_SUPPLIED_KEY := "material_upkeep_supplied"
# **THE CREW BEYOND WHICH MORE HANDS ADD NOTHING, *FIGHT INCLUDED*, ON A LABOR ASSIGNMENT** — the
# sim's own `LaborAssignment.huntUsefulWorkers`, the plateau of the same `hunt_crew_take_curve` the
# compose sheet's per-crew rows are drawn from. It rides the work-row map presence-sensitively
# (`HudBandLaborState.effective_worker_map`) and reaches a forecast through
# `with_published_useful_crew`, so a board row and a sheet cannot quote two ceilings for one herd.
#
# **IT IS HUNT-ONLY, and the `0` is why the copy is presence-sensitive**: on a hunt row `0` means
# *no crew is useful here*, and on every non-hunt row it is the same `0` meaning *does not apply*.
# Nothing may read this off a forage row — see `with_published_useful_crew`, whose one caller is the
# board's hunt branch.
const ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY := "hunt_useful_workers"
# **WHAT A WHOLE TRIP LANDS, PER MATERIAL** — on each row of the `HuntTripForecast` reply (the
# composed row and every per-preset one). It is a PAYLOAD, not a rate: no `/turn`, projected off the
# same carried biomass `delivered_food` is, so the two readouts of one raid cannot disagree. On an
# INEDIBLE quarry it is the ENTIRE payload, which is what stops such a raid reading as a denial
# mission with nothing to bring home.
const TRIP_DELIVERED_MATERIAL_KEY := "delivered_material"
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
# **THE ROOM A WHOLE-ANIMAL TAKE IS QUANTISED AGAINST, WHEN IT CANNOT BE STATED IN BIOMASS.** A
# NEGATIVE, so it can never be mistaken for the real reading `0.0` — an empty room, which is a take of
# nothing rather than a question with no answer. It reaches exactly one shape: a MANAGED source (a
# built Pen), whose production is a payoff figure with no escapement room behind it, on a species that
# also pays no food to read that payoff back through.
const NO_ROOM_IN_BIOMASS := -1.0
# The value the dropped term contributes to a `min()` / the crew `max()` — an unbounded reach cannot
# be the binding arm, and `INF` says so without a branch at every call site.
const ENGAGEMENT_UNBOUNDED := INF
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
# **A MANAGED SOURCE HAS NO ESCAPEMENT ROOM AND IGNORES THE FLOOR ENTIRELY** (sim
# `SourceYieldForecast::managed`): a built Pen is YOURS — you control its reproduction, so there is no
# wild stock to stop short of and the axis honestly collapses onto the one managed production it hands
# over. The wire still carries its raw `biomass`/`carrying_capacity`/rates (they are facts about the
# herd), so composing an escapement ceiling on one is silently wrong; the managed production is the
# rung's own payoff field, which for a BUILT rung is the live number the sim pays. Rung 2 (a pastoral
# herd) is still a wild stand being drawn down, so it takes the composition like rung 1.
#
# > #### ⛔ THE PLANT WEB HAS NO MANAGED RUNG. A FIELD IS DRAWN DOWN LIKE ANY OTHER STAND.
# >
# > `SOURCE_KIND_FORAGE: "is_field"` sat in this table until `forage.rs`'s **RETIRED: the whole rung-3
# > MANAGED HARVEST** landed — *"A Field is now foraged through the ordinary `forage_take` path exactly
# > as a tended patch and a wild stand are — floor-live, worker-capped, drawn down"* — and the entry
# > outliving that retirement is what made a completed cash-crop Field read `max 0 workers useful
# > here` beneath a tile card stating two tenders. The chain: a Field took the managed branch of
# > `forecast_inputs`, whose material ceiling comes from `FORECAST_PAYOFF_MATERIAL_KEYS`, which has no
# > plant entry by design (a plant's materials are per PLANT), so its ceiling was structurally `[]`;
# > `off_axis_useful_workers` then divided a `0` room by a live 1.12 tobacco/worker and answered `0`,
# > while `hold_crew` / `reach_crew` — §7.2's rescue for exactly that shape — returned `0` because the
# > source was "managed". Reported from play 2026-08-22.
# >
# > **`field_yield` is still a real quote and is still read** — it is the MSY skim on the Field rung's
# > own curve (`forage::rung_payoff`), i.e. what the rung PAYS, which is what `FORECAST_PAYOFF_KEYS`
# > asks it for. What it is not is a per-turn production a crew collects off a stock nobody draws down.
const FORECAST_MANAGED_FLAG_KEYS := {
    SOURCE_KIND_HERD: "corralled",
}
const FORECAST_MANAGED_IMPROVEMENTS := {
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
# non-food product is MATERIALS, which are a VECTOR and never one per-turn number: the HERD rungs
# state theirs through `FORECAST_PAYOFF_MATERIAL_KEYS` below, and the crop picker states the plant
# web's per PLANT from the composition entry's own `sow_material_payoff` /
# `cultivate_material_payoff`.
#
# The FODDER half survives. **PLANT RUNGS ONLY, and that asymmetry is structural rather than pending work:**
# fodder is feed grown for penned animals, and no animal pays it (`fauna_config::YieldAccounts` fills
# a structural zero there), so `tame` and `corral` have no twin here and never will.
const FORECAST_PAYOFF_FODDER_KEYS := {
    "cultivate": "tended_fodder",
    "sow": "field_fodder",
}
# **AND THE MATERIAL HALF, WHICH IS A VECTOR AND THEREFORE NOT THE TRADE SCALAR RETURNING.** The two
# HERD rungs quoted nothing at all for an inedible quarry: `corral_yield` / `pastoral_yield` are
# provisions, a wolf's are honestly `0`, and the face read `0.00 food` or nothing. `corralMaterial` /
# `pastoralMaterial` are what a prepared herd actually pays, one row per material, so a Tame or a
# Corral on a wolf finally states a payoff.
#
# **THE PLANT WEB IS ABSENT HERE ON PURPOSE.** A patch's material payoff is per PLANT and per rung
# (`sow_material_payoff` / `cultivate_material_payoff` on each composition entry), which the crop
# picker states row by row; a single tile-level rung figure would sum across plants, and summing is
# the retired axis under a new name.
const FORECAST_PAYOFF_MATERIAL_KEYS := {
    "corral": "corral_material",
    "tame": "pastoral_material",
}
# **RETIRED — `FORECAST_FEED_KEYS`, the RUNNING COST a payoff was paid against.** It held exactly one
# entry, `corral -> pen_upkeep`, so a pre-commit Corral row could read `+5.40/turn − 2.40/turn feed`.
# There is no such cost: HUMAN FOOD IS NOT ANIMAL FEED. A pen eats the grass its fenced footprint
# grows and the hay its keeper carries in — both FODDER — and a shortfall STARVES the herd
# (`pen_fed_fraction` < 1) rather than billing the people's larder. `pen_upkeep` is a `(deprecated)`
# wire slot the native reader no longer publishes.
#
# So `corral_yield` STANDS ALONE, and no rung on either web quotes a food-unit running cost. The
# rung's real standing price is in WORK, and it is already stated where every rung's is — the work
# row's `⌃` tooltip, via `FORECAST_BUILD_UPKEEP_DEMAND_KEYS`. Do not mint a second feed term here to
# put the subtraction back.
# **THE DURING-BUILD DIP IS RETIRED, and so is the build's `crew_needed`**
# (`docs/plan_standing_upkeep.md` §2.2). `<rung>BuildFraction` and `<rung>CrewNeeded` are deprecated
# wire slots the native reader no longer publishes, so nothing here reads them and nothing composes
# a fraction from them.
#
# The dip said *"this crew is preparing ground, not gathering"*, which is a statement about a SHARED
# crew and about nothing else. A source carries three independent allocations now — take, build,
# maintain — so what a build costs is the hands standing on it, and the gatherers beside them carry
# what gatherers carry. There is no factor left to multiply anything by: `preparing = 0` is read off
# the model, not off a number.
#
# `crew_needed` was a staffing FLOOR under the source's published `workers_needed`, needed only
# because that count was inverted out of a DIPPED take. With each activity stating its own crew there
# is no blended count for a floor to raise, and `workers_needed` is answered per activity — hands to
# meet the upkeep (`upkeep_workers_needed`), hands to haul the offer (`workers_needed`).
# The per-source BUILD METER each improvement fills, 0..1. The one place that mapping is written down
# (`RungGates.rung_in_progress` reads it, so the compose sheet, the work board and the map badge can
# never quote different meters for one verb).
const FORECAST_BUILD_METER_KEYS := {
    IMPROVEMENT_CULTIVATE: "cultivation_progress",
    IMPROVEMENT_SOW: "field_progress",
    IMPROVEMENT_TAME: "domestication",
    IMPROVEMENT_CORRAL: "corral_progress",
}
# **WHAT THE JOB HAS COST SO FAR AND WHAT IT COSTS IN TOTAL, in WORK UNITS**
# (`docs/plan_unit_costed_work.md` §8). The `*_progress` meter above IS `work_done / work_cost`, so
# these are never divided here and the fraction is never re-derived from them: one number, one
# authority. What they add is the sentence a fraction structurally cannot make — *"18 of 50 work"* —
# which is what makes a rung's SIZE visible now that rungs cost different amounts.
const FORECAST_BUILD_WORK_DONE_KEYS := {
    IMPROVEMENT_CULTIVATE: "cultivation_work_done",
    IMPROVEMENT_SOW: "field_work_done",
    IMPROVEMENT_TAME: "tame_work_done",
    IMPROVEMENT_CORRAL: "corral_work_done",
}
# The cost half. **It is published WHETHER OR NOT A BUILD IS IN FLIGHT** — the resolved price of that
# job on THIS source — which is what lets the compose sheet quote a rung BEFORE the player commits.
const FORECAST_BUILD_WORK_COST_KEYS := {
    IMPROVEMENT_CULTIVATE: "cultivation_work_cost",
    IMPROVEMENT_SOW: "field_work_cost",
    IMPROVEMENT_TAME: "tame_work_cost",
    IMPROVEMENT_CORRAL: "corral_work_cost",
}
# **WHAT THAT RUNG COSTS TO HOLD, PER TURN — the STANDING price beside `workCost`'s one-off one**,
# keyed by improvement exactly as the cost is and published under the same rule: unconditionally,
# whether or not a build is in flight. It is what the offered face quotes as *and this much every
# turn, forever* — the second half of what the player is agreeing to.
#
# **IT IS A PRICE AND NEVER A THRESHOLD** (`docs/plan_standing_upkeep.md` §2.4). It WAS the term the
# build's closed form subtracted, back when the build crew supplied the rate while a meter was being
# raised. **The keeping pool owes it at every fullness now** and a build crew's whole output is
# progress, so a form that still netted it would quote a pace nobody pays — and a note that called it
# a threshold would name a mechanism that no longer exists. What can still eat a build is the ROT,
# which is `FORECAST_METER_ROT_KEY` and is a fact about the source rather than about the rung.
#
# **`upkeep_demand` IS STILL NOT THIS NUMBER, and since §2.8 it is one step further away.** That
# field is the BILL THE KEEPERS WERE HANDED this turn (`forage::patch_keeping_basis`), which is what
# makes `demand − supplied == shortfall` hold exactly — the right answer for *where is my pooled
# shortfall landing* and the wrong one for *what would this rung cost to hold*. On the plant web it
# is now INTERPOLATED up the branch as the one position climbs, so it is not even the rung's own
# rate; this pair is, at every fullness and on a rung nobody has started. The rung is picked with the
# same key table the cost is, so price, meter and rate can never name three different rungs.
const FORECAST_BUILD_UPKEEP_DEMAND_KEYS := {
    IMPROVEMENT_CULTIVATE: "cultivation_upkeep_demand",
    IMPROVEMENT_SOW: "field_upkeep_demand",
    IMPROVEMENT_TAME: "tame_upkeep_demand",
    IMPROVEMENT_CORRAL: "corral_upkeep_demand",
}
# **THE MATERIAL TWIN OF THAT PAIR** — what holding THAT rung costs in goods per turn, keyed by the
# same improvement so price, meter, work rate and goods rate can never name four different rungs.
#
# ⛔ **THIS IS THE RUNG'S RATE; `FORECAST_UPKEEP_MATERIAL_DEMAND_KEY` IS THE STAMPED BILL.** The
# stamp says what the source was BILLED this turn, resolved through the rung it stands on; these say
# what a rung it may not be on yet would COST. **On a source mid-climb the two DISAGREE, and that is
# correct** — the work pair directly above has exactly that relationship. Do not reconcile them.
#
# ⛔ **EMPTY MEANS THE RUNG EATS NO MATERIAL, never zero of something** (the `MaterialPayoff`
# contract): a reader states NO material clause rather than `0 hurdles`. On the shipped ladder both
# plant keys and `tame_upkeep_material_demand` are always empty and only
# `corral_upkeep_material_demand` names a good — so **a pastoral herd looking at the Pen row is the
# only place this is ever non-empty in play**, and it is exactly the place the stamp cannot answer,
# `animal:pastoral` declaring no material of its own. The route branch's stone is what lands in the
# plant-shaped seam next.
const FORECAST_BUILD_UPKEEP_MATERIAL_DEMAND_KEYS := {
    IMPROVEMENT_CULTIVATE: "cultivation_upkeep_material_demand",
    IMPROVEMENT_SOW: "field_upkeep_material_demand",
    IMPROVEMENT_TAME: "tame_upkeep_material_demand",
    IMPROVEMENT_CORRAL: "corral_upkeep_material_demand",
}
# **HOW MANY MORE TURNS THE BUILD NEEDS, AND WHAT THE CREW'S TOOLS TOOK OFF IT.** One of each per
# SOURCE rather than per rung: at most one improvement is ever in flight on one source.
#
# **THIS IS THE SIM'S ANSWER FOR THE CREW ALREADY THERE**, and it is what a surface with no crew
# control renders — the tile card and the herd drawer. It cannot answer for a crew the player is
# PROPOSING, which is what a compose sheet's stepper asks; that is the terms below.
const FORECAST_BUILD_TURNS_KEY := "build_turns_remaining"
# **WHERE THIS SOURCE SITS IN THE WINNING BAND'S BUILD QUEUE** — 0-based, `NOT_IN_ANY_BUILD_QUEUE`
# when no band has queued it (`docs/plan_standing_upkeep.md` §4.6b). It rides the SAME winning band
# as `FORECAST_BUILD_TURNS_KEY` and `FORECAST_BUILD_GEAR_WORK_KEY`, so the three are read as one set;
# the countdown is a CHAINED date — everything ahead of this entry plus its own span at the full
# builders pool — and this is what makes that number explicable.
const FORECAST_BUILD_QUEUE_POSITION_KEY := "build_queue_position"
# **WHY THE QUEUE IS BLOCKED HERE** — `""` on any source that is not a blocked build, else the sim's
# own short cause key for the conjunct of the rung's gate that refused
# (`docs/plan_standing_upkeep.md` §4.6b). It is READ BESIDE `FORECAST_BUILD_TURNS_KEY`'s `-4`, never
# instead of it: the sentinel says the builders are held here, this says why, and a `-4` with no cause
# beside it is the state that shipped and cost a playtest several turns on a Tame nobody could explain.
# **THE SIM DECIDES `eligible`, SO THE SIM SAYS WHY** — this client holds no gate machinery for it and
# must not re-derive one. It rides the SAME winning band as the three fields above.
const FORECAST_BUILD_BLOCKED_REASON_KEY := "build_blocked_reason"
# **WHERE THE QUEUED ENTRY IS TAKING THIS SOURCE, AND WHAT IS LEFT OF THE CLIMB**
# (`docs/plan_standing_upkeep.md` §2.8). A queue entry names a DESTINATION rung rather than a single
# rung — the four verbs always were destinations, and with ONE POSITION PER SOURCE that reading
# became literal: the entry lays every rung between where the source stands and where it was sent,
# in order, and stays at the head until it ARRIVES. So a `sow` declared on untended ground is two
# legs and costs the whole branch.
#
# **BOTH ARE READ AND NEITHER IS RE-DERIVED.** A leg's `work_remaining` is its owing FROM WHERE THE
# SOURCE STANDS (a patch 30 units into a Cultivate owes 20 on that leg, not 50 — a previous
# improvement is a RECEIPT, NOT A DISCOUNT) and its `turns_remaining` is CHAINED behind the legs
# above it against a build queue this client cannot see. Reconstructing either would be a second
# producer of a verdict that already has one, which is the failure the whole `buildTurnsRemaining`
# family exists to prevent.
# **WHAT THE QUEUED ENTRY IS BEING RAISED WITH** — the builders kit id that entry RESOLVES to, `""`
# when the source is in nobody's queue (`docs/plan_standing_upkeep.md` §4.7a ②). It rides the SAME
# winning band as the four fields above it.
#
# **IT IS THE RESOLVED KIT, NEVER THE STORED OVERRIDE**, which is what makes it renderable: the
# builders' default is derived per entry from that entry's own food web, so an entry naming nothing
# would read EMPTY while the pool was out with hurdles. The `(default)` mark beside it is the
# client's own — `KitRoster.build_kit_for_branch` mirrors the sim's roster derivation — exactly as
# the hunt row's per-quarry default mark is.
#
# **AND IT IS CAPTURED LIVE**, so the recapture the `build_kit` command triggers already carries the
# new value and the pick needs no optimistic overlay of its own.
const FORECAST_BUILD_KIT_KEY := "build_kit_id"
const FORECAST_BUILD_DESTINATION_KEY := "build_destination_rung"
const FORECAST_BUILD_LEGS_KEY := "build_legs"
# **WHAT THE SOURCE WILL CARRY AT THAT DESTINATION** — the same `K` as `FORECAST_CAPACITY_KEY`,
# struck at the destination rung's standing instead of the source's own, so the pair is read as ONE
# object: that key says where the climb ends, this says what the ground holds when it gets there.
#
# **IT IS WHY THE TAKE FALLS DURING A BUILD.** The escapement floor is `floor x K` and a rung RAISES
# `K` — `field_capacity_gain` on the plant web, `pastoral_density` / `pen_density` on the animal one —
# so the floor climbs every turn a build runs and the player's take drops underneath it. Without this
# figure a surface can mark that the floor is MOVING and nothing more, and a fall the player paid for
# reads as the source being poor.
#
# **THE DESTINATION'S, NOT NEXT TURN'S** — next turn's position depends on work nobody has banked, so
# it is not even well defined; the destination rung is already named, so its gain is known today.
#
# **AND IT IS STRUCK ON TODAY'S LAND: THE RUNG MOVES, THE LAND DOES NOT.** The sim sums the flow over
# the ground as it stands this turn, so this figure drifts turn to turn exactly as the live capacity
# beside it does. Every wording built on it therefore states the ground it was struck on and never
# promises a future number.
const FORECAST_BUILD_DESTINATION_CAPACITY_KEY := "build_destination_capacity"
# The three keys of one published leg. **Named**, because the producer is the Rust decoder and the
# readers are two GDScript surfaces: a typo in a `get` here is a silent zero, which on the work side
# would price a whole branch as free.
const BUILD_LEG_RUNG_KEY := "rung"
## ⛔ **THE LEG'S OWN NAME, WHERE THE VERB CANNOT PRODUCE ONE.** The two food webs derive a leg's word
## from its improvement verb (`DetailFormat.rung_badge_word`), which is a hard-coded four and answers
## `""` for `grade` / `pave` — so a road's leg would render as a bare price with nothing saying which
## rung it buys. The ROUTE branch's rung names are published per world, so its producer carries the
## name and the renderer prefers it. `""` on every leg either food web builds, which takes the derived
## word exactly as before.
const BUILD_LEG_NAME_KEY := "name"
const BUILD_LEG_WORK_KEY := "work_remaining"
const BUILD_LEG_TURNS_KEY := "turns_remaining"
# …and the fourth key `build_legs` ADDS: the wire's rung crossed to the improvement verb a command
# names, so a caller matching legs against a branch and a caller naming the verb read one row.
const BUILD_LEG_IMPROVEMENT_KEY := "improvement"
# A leg owing this much or less has been paid for. The sim never publishes one
# (`forage::patch_build_legs` pushes a leg only where `owed > LEG_ALREADY_PAID`), which is exactly why
# the client states the boundary rather than trusting the head of the list — see `build_leg_in_flight`.
const BUILD_LEG_NOTHING_OWED := 0.0
# **THE WIRE'S RUNG SPELLING, AND THE VERB THAT NAMES EACH RUNG AS A DESTINATION.** The sim spells a
# rung `<branch>:<id>` — branch-qualified because `wild` names a rung on BOTH webs — and this client
# spells the same thing as an improvement verb, because that is what a command carries. This is the
# ONE place the two vocabularies meet.
#
# **`wild` HAS NO VERB, AND ITS ABSENCE IS THE ANSWER.** You do not build the rung every source
# starts on, so it maps to `IMPROVEMENT_NONE` and a destination picker renders it as the floor of the
# branch rather than as something to take the land to.
const RUNG_KEY_WILD_PLANT := "plant:wild"
const RUNG_KEY_TENDED := "plant:tended"
const RUNG_KEY_FIELD := "plant:field"
const RUNG_KEY_WILD_ANIMAL := "animal:wild"
const RUNG_KEY_PASTORAL := "animal:pastoral"
const RUNG_KEY_PEN := "animal:pen"
# **THE ROUTE BRANCH'S FOUR, ALIASED OFF `HudRouteVocab` RATHER THAN RESTATED.** That leaf is the
# road readout's vocabulary and already spells them; a second spelling here is exactly the "one fact
# written twice" this pair of tables warns about one comment down. The direction is safe by that
# file's own contract — it references this layer inside FUNCTIONS only, never in a `const`, so the
# two do not form a load cycle.
const RUNG_KEY_PATH := HudRouteVocab.RUNG_KEY_PATH
const RUNG_KEY_TRAIL := HudRouteVocab.RUNG_KEY_TRAIL
const RUNG_KEY_DIRT_ROAD := HudRouteVocab.RUNG_KEY_DIRT_ROAD
const RUNG_KEY_PAVED_ROAD := HudRouteVocab.RUNG_KEY_PAVED_ROAD
const RUNG_KEY_IMPROVEMENTS := {
    RUNG_KEY_WILD_PLANT: IMPROVEMENT_NONE,
    RUNG_KEY_TENDED: IMPROVEMENT_CULTIVATE,
    RUNG_KEY_FIELD: IMPROVEMENT_SOW,
    RUNG_KEY_WILD_ANIMAL: IMPROVEMENT_NONE,
    RUNG_KEY_PASTORAL: IMPROVEMENT_TAME,
    RUNG_KEY_PEN: IMPROVEMENT_CORRAL,
    # ⛔ **THE ROUTE BRANCH'S FREE FLOOR IS TWO RUNGS DEEP, AND BOTH DECLARE NO VERB.** A path and
    # the trail above it are both worn in by traffic alone — neither is something a band
    # builds, so both map to `IMPROVEMENT_NONE` exactly as the two `wild` rungs do. The gap this
    # closes is above them: `grade` and `pave` are real verbs now, where the branch declared none at
    # all when these tables were written.
    RUNG_KEY_PATH: IMPROVEMENT_NONE,
    RUNG_KEY_TRAIL: IMPROVEMENT_NONE,
    RUNG_KEY_DIRT_ROAD: IMPROVEMENT_GRADE,
    RUNG_KEY_PAVED_ROAD: IMPROVEMENT_PAVE,
}
# **…AND THE INVERSE: THE RUNG EACH IMPROVEMENT VERB BUILDS.** `improvement_is_done` asks this of
# every rung of every source the HUD draws, so it is written down rather than searched for in the
# crossing above — a reverse scan at call time would walk six rows to answer what a `has` answers.
#
# ⛔ **THE PAIR IS ONE FACT WRITTEN TWICE AND MUST STAY IN STEP.** A rung added to one table and not
# the other is either a verb naming no rung — which reads as *never built*, silently — or a rung no
# verb can reach. The route ladder is in BOTH, which is what closed the gap `grade` / `pave` opened.
#
# **`IMPROVEMENT_NONE` IS ABSENT, AND THAT ABSENCE IS THE ANSWER.** The floor is not something a band
# builds, it is where every source starts, and it is spelled differently on each branch — so *is
# `none` done?* answers `false`, which is what every rung walk in this client already relies on.
const IMPROVEMENT_RUNG_KEYS := {
    IMPROVEMENT_CULTIVATE: RUNG_KEY_TENDED,
    IMPROVEMENT_SOW: RUNG_KEY_FIELD,
    IMPROVEMENT_TAME: RUNG_KEY_PASTORAL,
    IMPROVEMENT_CORRAL: RUNG_KEY_PEN,
    IMPROVEMENT_GRADE: RUNG_KEY_DIRT_ROAD,
    IMPROVEMENT_PAVE: RUNG_KEY_PAVED_ROAD,
}
# **THE RUNG THE SOURCE STANDS ON**, in that same `<branch>:<id>` spelling — the ONE wire field that
# answers *"has this rung been built"* on both webs (`forage::patch_rung_key`, and its animal twin).
# `prefix` spells it, so a `patch_`-prefixed `tile_info` cross-ref and a bare wire row read alike.
const FORECAST_CURRENT_RUNG_KEY := "current_rung"
# …and the branch each web climbs, BOTTOM RUNG FIRST. It is an ORDER as well as a membership list:
# every consumer walks it to say where a source stands and what is above it, so a branch listed out
# of climb order would mark the wrong rung as *banked*.
const RUNG_BRANCH_PLANT := [RUNG_KEY_WILD_PLANT, RUNG_KEY_TENDED, RUNG_KEY_FIELD]
const RUNG_BRANCH_ANIMAL := [RUNG_KEY_WILD_ANIMAL, RUNG_KEY_PASTORAL, RUNG_KEY_PEN]
# **THE ROUTE BRANCH IS FOUR RUNGS, NOT THREE**, because its free floor is two storeys: a path is
# what the first traffic wears in and a trail is what more of it wears over that. Both are below
# the first rung anybody pays for, which is what `rung_above_branch_floor` reads out of the ORDER.
const RUNG_BRANCH_ROUTE := [RUNG_KEY_PATH, RUNG_KEY_TRAIL, RUNG_KEY_DIRT_ROAD,
    RUNG_KEY_PAVED_ROAD]
# …and EVERY BRANCH THIS CLIENT KNOWS, in one list. It is what makes a question about a rung KEY
# answerable without first being told which web the source is on — the wire spells a rung
# `<branch>:<id>`, so the branch travels inside the key. The route web was that promised third entry,
# and it cost exactly one row here and no per-source code anywhere.
const RUNG_BRANCHES := [RUNG_BRANCH_PLANT, RUNG_BRANCH_ANIMAL, RUNG_BRANCH_ROUTE]
const FORECAST_BUILD_GEAR_WORK_KEY := "build_work_from_gear"
# **WHAT IT COSTS TO HOLD THIS SOURCE AT THE RUNG IT STANDS ON**, in work units per turn — the RATE
# half of the ladder beside the build's PILE (`docs/plan_standing_upkeep.md` §2). All four ship on
# BOTH webs under the same names, which is what lets one readout serve a patch and a herd.
#
# **THE SIM ANSWERS AND THIS CLIENT SUBTRACTS NOTHING.** `upkeep_shortfall` is published, not derived
# from `demand − supplied`: it is EXACTLY what the improvement decays by once the shortfall outlasts
# the rung's grace, and a client re-deriving it would be a second authority over the number the whole
# readout exists to make legible (the sim-answers-the-client-renders discipline).
#
# **`upkeep_supplied` IS THIS SOURCE'S SHARE OF ITS BAND'S POOL** (`docs/plan_standing_upkeep.md`
# §2.5), not the hands standing on it: maintenance is a band-level role, so the three fields stopped
# answering *"did you staff this one"* and answer *"where is my pooled shortfall landing"*. Every
# readout of them must be worded that way — a row that reads as a per-source staffing verdict points
# the player at a stepper that no longer exists.
const FORECAST_UPKEEP_DEMAND_KEY := "upkeep_demand"
const FORECAST_UPKEEP_SUPPLIED_KEY := "upkeep_supplied"
const FORECAST_UPKEEP_SHORTFALL_KEY := "upkeep_shortfall"
# **WHAT THIS SOURCE'S KEEPING IS WORTH IN HANDS** — `ceil(demand / PER_WORKER_OUTPUT)`, beside the
# take activity's `SourceYield.workersNeeded` (hands to haul the offer). It is a SIZE, not a staffing
# order: nobody is assigned here any more, so it reads as *this much of the band's keeping pool*.
#
# **IT PUBLISHES ON BOTH SIDES OF COMPLETION, AND IT IS THE SAME NOUN ON BOTH** — the KEEPING pool
# holds a meter at every fullness (`docs/plan_standing_upkeep.md` §2.4), so a source mid-build wants
# keepers for exactly the reason a held rung does and the count answers one question throughout. It
# read `0` mid-build until this arc, which is why nothing may infer *"a build is in flight"* from a
# zero here.
const FORECAST_UPKEEP_CREW_KEY := "upkeep_workers_needed"
# **THE COUNTDOWN, AND THE BOOL THAT MAKES ITS ZERO READABLE.** Both webs ship the pair: the flag says
# *there is a built rung here that can be lost*, the count says how many more turns of SHORTFALL are
# forgiven before the decay starts. `0` under a true flag is *biting this turn*; the flag false is
# *nothing at stake*, which is the same number and the opposite news — read the flag first.
const FORECAST_NEGLECT_GRACE_FLAG_KEY := "has_neglect_grace"
const FORECAST_NEGLECT_GRACE_KEY := "neglect_grace_remaining"
# **WHAT THIS SOURCE'S AT-RISK METER IS LOSING PER TURN, RIGHT NOW**, in work units — the ONE term
# that can still stop a build finishing (`docs/plan_standing_upkeep.md` §2.4).
#
# **IT REPLACED THE RATE IN THE BUILD'S CLOSED FORM, and the two are different questions.** The rung's
# rate is what HOLDING costs and is owed to the band's keeping pool at every fullness; this is what
# the keeping FAILED to cover, already resolved through the grace and the rung's own decay rate. So a
# build crew adds its whole output and the rot is what eats into it: `net = crew work − rot`.
#
# **IT IS PER SOURCE, not per rung**, and it takes no improvement — at most one meter on a source is
# at risk at a time, exactly as `FORECAST_BUILD_TURNS_KEY` is one answer per source. It is also a
# CONSTANT with respect to the build stepper, which is why the sim publishes it rather than leaving
# the client to compose it: the crew the player is dragging changes the progress, never the rot.
#
# **ALWAYS MEANINGFUL, NEVER A SENTINEL** (the `corralYield` rule). `NO_METER_ROT` is a measured
# nothing, and it is the honest reading in three ordinary states: the keeping covers this source, the
# source is still inside its grace, or the rung declares no meter decay at all — which is BOTH animal
# rungs, whose penalty is a shed rather than a bleed, so an animal meter never goes backwards.
# ---- THE MATERIAL HALF OF THE LADDER'S PRICE (`docs/plan_standing_upkeep.md` §2.7) --------------
# **WORK WAS NEVER THE WHOLE PRICE.** A fence swallows hurdles and a road swallows stone: goods that
# go INTO the thing and stay there, spent, where a kit is carried on to the next job. Three lists of
# `{material_id, amount}` per source, beside `<rung>_work_cost` and the `upkeep_*` trio above.
#
# ⛔ **NEVER SUMMED ACROSS GOODS, AND EMPTY MEANS "NO ROW" RATHER THAN ZERO** — the `MaterialPayoff`
# contract every such list on this wire follows. A total is a currency this model does not have (it
# is the retired trade axis under a new name), and a rung that eats nothing must not read as one that
# eats badly.
#
# `FORECAST_BUILD_MATERIAL_COST_KEY` prices **ONE rung** — the one DIRECTLY ABOVE where the source
# stands. A track row two rungs up therefore has no pile to state and states none, exactly as a rung
# the wire prices no work for renders no figure.
#
# ⛔ It was once *"the only pile the wire quotes"*, and that clause is now false: the wire also
# publishes `FORECAST_CORRAL_BUILD_MATERIAL_COST_KEY` below, the pen rung's OWN pile, which is what a
# RING is priced from. Reading the above-selector as universal is what left the ring card quoting no
# pile at all through a whole slice.
const FORECAST_BUILD_MATERIAL_COST_KEY := "build_material_cost"
# …and the pile `animal:pen` swallows to raise, read at THAT rung rather than at the one above the
# source. The one field a RING can be priced from: `animal:pen` is the top of the animal branch, so
# the key above answers `[]` on every corralled herd, which is exactly the row a ring is offered on.
# On a PASTORAL herd the two carry the same pile by construction — same rung, two selectors — and it
# is their DISAGREEMENT on a penned herd that earns this key its place.
const FORECAST_CORRAL_BUILD_MATERIAL_COST_KEY := "corral_build_material_cost"
# What holding this source's OWN current rung swallows per turn (the STAMPED bill, `upkeep_demand`'s
# per-good twin) and what the band's store actually paid toward it. The sim publishes both terms
# rather than their difference, for the reason the work trio does: a client renders and subtracts
# nothing.
const FORECAST_UPKEEP_MATERIAL_DEMAND_KEY := "upkeep_material_demand"
const FORECAST_UPKEEP_MATERIAL_SUPPLIED_KEY := "upkeep_material_supplied"

const FORECAST_METER_ROT_KEY := "meter_rot_per_turn"
## Nothing is bleeding off this meter — the keeping covers it, the grace still holds, or the rung's
## penalty is not a meter bleed at all. A measured nothing, never *no answer*.
const NO_METER_ROT := 0.0
## No standing cost at all — a wild patch, a wild herd, a rung that declares no upkeep. The demand is
## ALWAYS MEANINGFUL (never a sentinel), so this is a measured nothing rather than an absent answer.
const NO_UPKEEP_DEMAND := 0.0
## Nobody is needed to hold something that costs nothing to hold.
const NO_UPKEEP_CREW := 0
## Below this a work rate is nothing to state — the same floor the food flow uses, so a `0.00 work`
## row can never be printed by one readout and suppressed by another.
const UPKEEP_WORK_MIN := 0.005
# **…AND THE SOURCE'S TERM IN THE SAME ESTIMATE** — `build_turns_at` evaluates the closed form from
# it (`.claude/rules/core_sim/yield-forecast.md` → "THE BOUNDARY, stated once": where a closed form
# exists the sim ships the TERMS and the client evaluates it).
#
# It is what ONE worker banks on this source in a turn at the food peak, before the floor multiplier
# and before gear. **It is READ, never assumed** — the sim writes worker output as a sum of terms
# with one term today, so a client holding the constant would quote a number the sim disagrees with
# the moment a second term lands. **The GEAR half is not a source term at all** — both its terms are
# facts about the band's ledger, so they ride the kit row; see `BUILD_GEAR_*` below.
const FORECAST_BUILD_PER_WORKER_TURN_KEY := "build_work_per_worker_turn"
## **NO ESTIMATE — and it MUST render as no line at all, never as `0 turns`.** The sim answers this
## for a stalled build (a crew producing nothing has no finite answer) and for a source nobody is
## working. A missing line is honest; a zero is a promise the build is about to finish.
const BUILD_TURNS_NO_ESTIMATE := -1

## **THE BOUNDARY BELOW WHICH A COUNT HAS NO NUMBER TO STATE** — every reading at or under it either
## has a face of its own (the two never-finishing sentinels, answered before this is reached) or has
## none at all, and `DetailFormat.build_turns_clause` renders NO clause for it. Named because the test
## is a producer's single guard rather than a list repeated at each call site: a bare `-1` compared
## inline is exactly how `≈-1 turns` becomes reachable the next time the wire spells a new sentinel.
const BUILD_TURNS_NONE_TO_STATE := 0

## **THE ANSWER FOR A STATED CREW THAT EXACTLY MATCHES THE ROT** — a build crew banking precisely
## what an under-kept meter is losing each turn (`docs/plan_standing_upkeep.md` §2.4). The crew's
## whole output is progress and the rot is what eats it, so at `crew work == rot` the meter holds
## exactly where it is and no number of turns is ever reached.
##
## **THE TERM IT BALANCES AGAINST CHANGED, AND THE SENTINEL DID NOT.** It used to be the rung's
## maintenance RATE, which the build crew supplied while the meter was going up; the keeping pool owes
## that at every fullness now, so what a crew races is the shortfall's own bleed. A meter whose keeping
## is covered rots at nothing, and every staffed crew on it climbs.
##
## **It is its own sentinel and not `BUILD_TURNS_NO_ESTIMATE`, because the two render differently.**
## No-estimate means *there is no question here yet* — nobody staffed, no priced job, no room above the
## floor — and renders as no clause at all. This one is an ANSWER to a crew the player has stated, and
## the answer is ∞: it must be visible, and visible as a WARNING, because it is the one readout that
## should stop them. A large number in its place would read as a promise.
##
## **IT IS THE WIRE'S OWN VALUE** — `sim_schema::BUILD_METER_HOLDS`, published on
## `buildTurnsRemaining` beside `NO_BUILD_TURNS_ESTIMATE`'s `-1` and passed through verbatim by the
## decoder. The two were ONE sentinel for a release, which is why the tile card and the herd drawer
## rendered no line for a fact the player needed and only the compose sheet — which redid the
## comparison itself — could show it. Nothing in this client derives it from a source the sim has
## answered for; the one place a comparison still happens is `build_turns_at`, which prices a crew
## the sim has never seen.
##
## **IT IS NOT `BUILD_TURNS_ROTS`, AND THE NAME IS THE POINT.** *Holding* wastes the crew's turn;
## *rotting* destroys work already paid for. This constant was `BUILD_TURNS_NEVER` while it covered
## both, which is the same one-name-two-facts mistake one level down from the one the pair exists to
## close — a reader who saw `NEVER` had no way to know a second never-finishing answer existed.
const BUILD_TURNS_HOLDS := -2

## **THE ANSWER FOR A STATED CREW THAT IS LOSING THE BUILD** — the same real, staffed, priced job with
## a NEGATIVE net: the crew banks less per turn than the meter's own ROT, so the decay pass takes back
## more work than the builders put in (`sim_schema::BUILD_METER_ROTS`,
## `docs/plan_standing_upkeep.md` §2.4).
##
## **IT WAS SPLIT OUT OF `BUILD_TURNS_HOLDS` AND THE CLIENT DID NOT FOLLOW, WHICH IS THE WHOLE REASON
## THIS NOTE IS LONG.** `build_turns_remaining` accepted `-1` and `-2` and mapped every other negative
## to *no estimate*, so a real, staffed, priced build that was actively bleeding banked work rendered
## as **no line at all** on the tile card and the herd drawer — indistinguishable from a source nobody
## has touched. **An unrecognised sentinel must render as the
## STALLED hazard, never as silence** — that is what `rung_row_value`'s fallback is for, and it is
## still no substitute for reading the answer the wire actually sent.
##
## Both never-finishing answers wear the same `∞`, because both are true of the meter; what separates
## them is the INK — `HudStyle.WARN` for holding, `HudStyle.DANGER` for rotting — and the phrase
## `HudSelectionVocab.RUNG_ROTTING_PHRASE` the rung row adds, so the card says which of the two it is
## without the player having to know the sentinel.
const BUILD_TURNS_ROTS := -3

## **THE ANSWER FOR AN ENTRY THE QUEUE IS STUCK ON** — `sim_schema::BUILD_QUEUE_BLOCKED`
## (`docs/plan_standing_upkeep.md` §4.6b). The band's builders are STAFFED and standing on this
## entry, and the rung's own gate refuses it, so nothing banks — and, because the whole pool goes on
## the head of the queue until it fills, **nothing behind it moves either**: every entry queued
## behind a blocked head publishes this too.
##
## **IT IS NOT `BUILD_TURNS_NO_ESTIMATE`, AND THAT IS THE WHOLE POINT OF THE FOURTH SENTINEL.** `-1`
## is *there is no question here* — a waiting entry whose gate may well hold by the time it reaches
## the head, a source at the top of its ladder — and renders as no line at all. This one is a
## STANDING FAULT with a remedy, and rendering it as silence is how a stuck queue reads as a working
## one.
##
## **NOR IS IT `BUILD_TURNS_HOLDS` OR `_ROTS`.** Those are answers about the METER's arithmetic under
## a crew; this is an answer about the QUEUE, and its remedy is on a different line entirely — the
## band's keeping, not its builders. A client rendering three of the four cannot derive the fourth:
## the wire is the only thing that knows a refusing gate is sitting at the head of a staffed pool
## rather than merely waiting its turn.
const BUILD_TURNS_QUEUE_BLOCKED := -4

## **THE ANSWER FOR A BUILD THE SIM HAS NOT LOOKED AT YET** — `sim_schema::BUILD_NOT_YET_ESTIMATED`
## (`docs/plan_standing_upkeep.md` §4.9). The player queued this entry SINCE THE LAST TURN RESOLVED,
## so no estimate pass has ever run for it: there is no number because nothing has been asked, not
## because the answer came back empty.
##
## **IT IS NOT `BUILD_TURNS_NO_ESTIMATE`, AND FOLDING THE TWO IS THE DEFECT IT EXISTS TO CLOSE.** `-1`
## is *the sim looked and had no number* — nobody on it, a gate refusing, a running build banking
## nothing — and a card renders that as the `⚠ Stalled` hazard, which is right for a build that has
## had its chance. This is *the sim has not looked*, one command old, and it was reported from play as
## `⚠ Stalled 0%` on a fresh `Cultivate (4, 19)` with two builders standing on it — a warning that
## cleared itself on the next turn, which is the worst shape a warning can have. **It is therefore
## NEUTRAL: no hazard mark, no hazard ink, no pace verdict.** Nothing is wrong; its first turn has not
## run.
##
## **THE DISTINGUISHING FACT IS THE ESTIMATE PASS, NEVER THE METER, and only the sim holds it.** A
## genuinely stalled build sits at `0%` too, so no progress comparison can separate them — what can is
## that `publish_build_chain` stamps every entry it walks with its place in the line and the decay
## passes clear that place every turn, so *live in a band's queue AND still carrying the cleared place*
## is exactly *queued since the last pass*. The client holds neither the queues nor the passes and must
## not re-derive any part of it.
##
## **THE FIFTH SENTINEL IS THE THIRD TIME THIS FAMILY HAS GROWN** (`-3` split out of `-2`, then `-4`
## beside them, each time with a client reader left behind). Both readers that fork on the family —
## `build_pace` here and `DetailFormat.build_sentinel_value` — answer for it inside their own single
## fork; a third fork is the thing those two extractions exist to prevent.
const BUILD_TURNS_NOT_YET_ESTIMATED := -5

## **THIS SOURCE IS IN NO BAND'S BUILD QUEUE** — the neutral of `FORECAST_BUILD_QUEUE_POSITION_KEY`,
## the client's copy of `sim_schema::NOT_IN_ANY_BUILD_QUEUE`. A real position is 0-based, so the
## sentinel sits outside the range exactly as the countdown's negatives do.
const NOT_IN_ANY_BUILD_QUEUE := -1

## The head of a band's build queue — the one entry its whole builders pool is funding.
const BUILD_QUEUE_HEAD := 0

## **THIS SOURCE IS HEADING NOWHERE** — the neutral of `FORECAST_BUILD_DESTINATION_CAPACITY_KEY`, the
## client's copy of `sim_schema::NO_BUILD_DESTINATION_CAPACITY`. No band has queued the source, so
## there is no destination whose capacity could be quoted.
##
## **IT CANNOT BE `0`, AND THAT IS THE WHOLE REASON IT IS A SENTINEL.** A capacity of zero is a REAL
## reading a real source has — barren ground, an overgrazed range, a rock pen — so a zero standing for
## *nothing queued* would tell the player that building here would hold nothing, on every unqueued
## source on the map. A capacity is never negative, so ANY `< 0` is this reading and every surface
## renders NO DESTINATION LINE AT ALL for it: no dash, no zero, no empty clause.
const NO_BUILD_DESTINATION_CAPACITY := -1.0

## **THE NET AT WHICH A METER NEITHER GROWS NOR ROTS** — a build crew banking exactly what the meter
## is bleeding, so `crew work − rot` is exactly this. The client's copy of the sim's
## `intensification::BUILD_BALANCE_HOLDS`, and the ONE cut point `build_turns_at` splits its two
## non-finishing answers on, so the sheet and the card can never disagree about which side of the rate
## a committed crew is on.
##
## Named separately from `BUILD_WORK_NONE` — the same number — because the two are different
## statements: that one is *no work at all*, this one is the **boundary between two published
## answers**. Borrowing the other's name here is how the rot case gets asserted away.
const BUILD_BALANCE_HOLDS := 0.0

## **A METER NOBODY HAS PUT WORK INTO**, and **a meter standing exactly at its cost** — the two edges
## of `build_verb`'s three-state test, in the published fraction's own units (`improvement_progress`
## is `work_done / work_cost`, clamped). They mirror `intensification::RUNG_UNSTARTED` and the sim's
## `*_meter_full()` predicates one for one.
const BUILD_METER_UNSTARTED := 0.0
const BUILD_METER_FULL := 1.0

## **THE ANSWER FOR A BAR A WORKING CREW IS ALREADY AT OR PAST** — one turn, because a build with no
## work left in it completes on the first turn anybody works it. The client's transcription of
## `intensification::BUILD_FINISHES_IN_ONE_TURN`, which the sim returns for the same two states: the
## work is already banked, or the crew's GEAR pays the job off outright.
##
## **Named rather than a bare `1` at the site that returns it, and deliberately NOT
## `BUILD_TURNS_NO_ESTIMATE`** — this is *"there is nothing left to wait for"*, which is a real answer,
## where the sentinel means *"there is no answer"*. Conflating the two is what let a well-equipped crew
## blank the readout at exactly the crew size the arc's *add hands and watch it drop* claim is
## demonstrated by. `DetailFormat.BUILD_TURNS_SINGULAR` is a different constant with the same value:
## that one is the display fork (*which wording does a count of 1 take?*), this one is a build's answer.
const BUILD_FINISHES_IN_ONE_TURN := 1
## The cost below which a job has no stated price — a source the wire does not price this rung on.
## The readout falls back to the bare percentage there rather than printing `18 / 0 work`.
const BUILD_WORK_COST_NONE := 0.0
## **ZERO WORK UNITS**, on whichever end of the estimate is being asked: no work left to do, no work
## banked in a turn, no work a tool takes off the job. Distinct from `BUILD_WORK_COST_NONE`, which is
## a SENTINEL — *"the wire prices no such job here"* — rather than a measured nothing.
const BUILD_WORK_NONE := 0.0
## The proposed crew that has no estimate to give: nobody. `work_per_turn` is zero there, and the
## honest reading of `remaining / 0` is *no answer*, never a huge number.
const BUILD_CREW_NONE := 0

## **THE RUNGS WHOSE BUILD THE SIM GATES ON THE WORK PREDICATE** — *is anything standing above this
## assignment's floor?* (`systems::labor::crew_is_working_the_source`, the term that replaced both
## webs' `EcologyPhase::Thriving` gate). A crew watching a source it is not drawing down learns
## nothing and builds nothing, however hard the floor would otherwise teach.
##
## **IT IS EXACTLY TWO RUNGS, AND THE OMISSION ON THE OTHER TWO IS DELIBERATE, NOT AN OVERSIGHT.**
## `Sow` and `Corral` carry no such term in the sim: bare ground stands below every floor by
## construction, so requiring room would make the create-from-nothing case rung 3 exists for
## impossible, and a pen is fenced around a herd already drawn to its keeper's floor. Adding them
## here would blank the estimate on precisely the jobs whose price the sheet must quote.
const BUILD_WORK_PREDICATE_IMPROVEMENTS := [IMPROVEMENT_CULTIVATE, IMPROVEMENT_TAME]

## An empty escapement room — the boundary `crew_is_working_the_source` compares against, named
## because a bare `0.0` beside `max(0, B − floor·K)` reads as an arbitrary epsilon rather than as the
## exact value that expression is clamped at.
const BUILD_NO_ESCAPEMENT_ROOM := 0.0

## **THE GEAR TERM'S TWO HALVES, keyed on the dict a caller hands `build_turns_at`** — the work units
## one equipped worker ADDS to what it delivers per turn (flint hoes, +0.5 on a plant build), and how
## many of the band's workers this kit can actually equip for one. It is a term of the SUPPLY and
## never a discount off the pile (`docs/plan_standing_upkeep.md` §4.8 — `build_turns_at` states the
## form and is the one place this is evaluated).
## `gear(w) = min(w, saturating crew) × per-worker worth`: coverage arms a PREFIX of
## the party, so the contribution rises with the crew until every unit is in somebody's hands and
## then stops, and the `min` is what makes the form exact rather than approximate.
##
## **THEY ARE THE KIT'S, NOT THE SOURCE'S, and that is what makes the estimate re-price when the
## sheet's picker moves.** Both facts behind them — the units held and each unit's reach — belong to
## the band's ledger, so `PopulationCohortState.kitTiers` carries them and `KitRoster.build_gear` is
## the one reader; a source-scoped cap could only ever answer for the kit whose crew last worked it.
## The keys live HERE rather than in `KitRoster` because that layer reads this one, and a `const`
## cycle between two `class_name`d scripts fails to load the whole client.
const BUILD_GEAR_PER_WORKER := "per_worker"
const BUILD_GEAR_SATURATING_CREW := "saturating_crew"
## **THE PER-WEB *ACHIEVED* BOOL OF EACH RUNG THAT HAS ONE** — `is_cultivated` / `is_field` on the
## plant web, `corralled` on the animal one; `Tame` has never had one, its achievement being its
## meter. **No shipped reader in this client asks them.** The table survives for the test tree
## (`fixtures_rung.gd`, `map_preview`), which reads it to DERIVE a fixture's standing rung rather than
## spell one by hand — one table, so a fixture's flags and its `current_rung` cannot disagree.
##
## ⛔ **IT IS NOT THE *IS THIS BUILT* TEST, AND MUST NOT BE READ AS ONE.** That question is
## `improvement_is_done`, which reads the wire's `current_rung` — one field, both webs, and no
## cross-rung table to keep in step (`RETIRED: FORECAST_RETIRED_BY_HIGHER_RUNG`, below).
##
## ⛔ **RETIRED CLAIM — *"a Field sown from wild ground carries `is_cultivated` false FOREVER"*.**
## That was the two-independent-meters model, and it is why reading these bools for *built* once
## needed a cross-rung table. There is ONE ladder position now and the Field rung's range begins where
## the tended rung's ends, so `is_cultivated()` is `held.is_at_or_above(PlantTended)` and a Field
## answers it TRUE however it was reached. These bools and `current_rung` are two spellings of one
## standing rather than two facts that happen to agree — which is what makes the retired
## `rung_needs_repair` (its epitaph is beside `improvement_progress`) unanswerable.
const FORECAST_DONE_FLAG_KEYS := {
    IMPROVEMENT_CULTIVATE: "is_cultivated",
    IMPROVEMENT_SOW: "is_field",
    IMPROVEMENT_CORRAL: "corralled",
}
## **RETIRED — `FORECAST_RETIRED_BY_HIGHER_RUNG`.** It was a second table saying only that a Field is
## also cultivated, and it existed because *built* was asked of each web's private bools, which cannot
## see a rung skipped by `Sow`. `improvement_is_done` now compares the source's standing rung against
## the rung the verb builds, so *a higher rung retires the one below it* is the ORDER of
## `RUNG_BRANCHES` rather than a table beside it. **The route ladder adds no entry HERE either** — a
## road carries no per-verb achievement flag, its rung string being the whole of its standing.
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
# RETIRED — **`BUILD_BOUND_NOTE_FORMAT`**, the same cap worded to name the sheet's OWN builders
# stepper (`docs/plan_standing_upkeep.md` §2.5). It existed because the take and the build shared one
# source pool, so the nearer lever for freeing a hand was sometimes two rows down on the same sheet.
# A verb states no crew now: the sheet has one stepper, so there is one remedy and
# `LABOR_BOUND_NOTE_FORMAT` is it.

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
# Its MATERIAL twin, one clause per material — `· ~3 hide`. The material names itself, so the noun is
# the row's own id rather than a word baked in here; it is the same shape the food term wears because
# it answers the same question about the same trip.
const HUNT_FORECAST_MATERIAL_FORMAT := " · ~%d %s"
# **IT REPLACED A TRADE SCALAR WITH A VECTOR** (arc #527). The wire's single non-food payload figure
# is retired; what stands in its place is `delivered_material`, one row per material, which
# `_raid_payload_suffix` renders as one clause each — never summed, the standing rule for this
# account. So a raid's payload line quotes `~12 food · ~3 hide` where it used to quote one trade
# number, and an inedible quarry's line is the material clauses alone.
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
# (arc #527), and what replaced it is the MATERIAL arm: `hunt_trip_forecast` takes this branch only
# when the trip lands no material either, so a wolf raid hauling hides is a real delivery and reads
# as one. A raid that brings something home is not denying anything, whatever account that something
# is in (`.claude/rules/client/labor-ui.md`).
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

## The bare magnitude of a FODDER quantity ("100.0", "6.0") — the `format_magnitude` rule struck in the
## coarse account's own resolution (`FODDER_DECIMALS`). Every fodder reading in the HUD goes through
## it: the band's hay stock and its need/growing rate pair, and the pen row's hay draw and hay need.
##
## **A STOCK AND A RATE SHARE ONE RENDERER HERE, where food splits them** (`format_stock` vs
## `format_magnitude`). That split exists because a food rate needs two decimals and a biomass stock
## needs none; fodder's stock and its rate are the SAME units at the same scale, so one decimal is
## right for both, and giving them two renderers would only let them drift apart.
##
## The "/turn" that marks the rate ones rides the CALLER's format string (`" · needs %s hay/turn"`,
## `POLICY_CAP_FODDER_FORMAT`) rather than being appended here — the tight, no-space spelling those
## compact clauses already use, as distinct from `YIELD_PER_TURN_SUFFIX`'s spaced form that follows a
## standalone food figure.
static func format_fodder(value: float) -> String:
    return String.num(absf(value), FODDER_DECIMALS).pad_decimals(FODDER_DECIMALS)

## The bare magnitude of a food rate ("1.74"), for a readout that supplies its own sign in words
## ("− 1.74 feed"). One rounding rule for every food rate the HUD prints.
static func format_magnitude(value: float) -> String:
    return String.num(absf(value), YIELD_DECIMALS).pad_decimals(YIELD_DECIMALS)

## The sign a rate is prefixed with, typed ONCE for both accounts: the food rates below and the
## fodder ones beside them are the same readout convention at two resolutions, and a second spelling
## of "+" is how the two would come to differ in the one character a player reads first.
const RATE_SIGN_POSITIVE := "+"
const RATE_SIGN_NEGATIVE := "-"

## A signed, fixed-decimal food-rate string ("+0.31" / "-0.30"). Actual yields are ≥0, but the
## formatter is sign-aware so it also renders Net (which can go negative) and Consumption (shown
## as a negative cost).
static func format_signed(value: float) -> String:
    return _rate_sign(value) + format_magnitude(value)

## The same, at the FODDER account's own resolution (`format_fodder`, one decimal) — "+5.0" / "-6.0".
## The `Fodder:` row's breakdown prints through this, so a fodder rate is spelled the way every other
## fodder reading in the HUD is spelled and never at the food scale's two decimals.
static func format_signed_fodder(value: float) -> String:
    return _rate_sign(value) + format_fodder(value)

## The sign of a rate, zero counting as positive (a zero income is not a debit).
static func _rate_sign(value: float) -> String:
    return RATE_SIGN_POSITIVE if value >= 0.0 else RATE_SIGN_NEGATIVE

## The same rate with the "/turn" suffix, for the per-source row headline ("+0.31 /turn").
static func format_yield(value: float) -> String:
    return format_signed(value) + YIELD_PER_TURN_SUFFIX

## …and the FODDER account's twin of it ("+5.0 /turn"), for the faction page's `Fodder:` headline.
##
## **THIS TAKES THE SPACED SUFFIX, AND THE COMPACT CLAUSES STILL DO NOT.** `format_fodder`'s note
## above says the "/turn" rides the CALLER in the tight `POLICY_CAP_FODDER_FORMAT` spelling — that
## rule is about a fodder figure riding INSIDE a longer clause (`· needs 6.0 hay/turn`). Here the
## figure stands alone in a vitals row's value cell, in the same slot the food rollup's
## `format_yield` fills one row above, and a faction page spelling its two larders' rates two ways
## would be the drift both rules exist to stop.
static func format_yield_fodder(value: float) -> String:
    return format_signed_fodder(value) + YIELD_PER_TURN_SUFFIX

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

## Whether this source publishes an engagement stage at all. The whole PLANT web does not
## (`NO_ENGAGEMENT_STAGE` — a berry does not fight back), nor does a species the roster cannot resolve.
##
## ⛔ **THIS IS NOT THE TEST FOR "IS THERE A FIGHT HERE?" — `quarry_is_fought` IS.** It used to be, on
## the strength of a pen publishing `NO_ENGAGEMENT_STAGE` too, and that reading is now a lie one source
## kind wide; every caller that meant *fought* has moved. See `quarry_is_fought` for the wire defect
## and for why the two questions had to come apart.
static func has_engagement_stage(engage_rate: float) -> bool:
    return not is_inf(engagement_per_worker(engage_rate))

## The wire's own flag for a herd held behind a fence. Named here, in the layer both the forecast and
## the kit roster read through, so `KitRoster.QUARRY_CORRALLED_KEY` can alias it exactly as
## `SOURCE_ENGAGE_RATE` aliases `FORECAST_ENGAGE_RATE_KEY` — one spelling of one wire key.
const SOURCE_CORRALLED_KEY := "corralled"

## **IS THERE A FIGHT TO LOSE AT THIS SOURCE?** — the ONE predicate every fight-shaped surface takes:
## the kit picker's offer test, the priced gate behind the sheet's numbers, the sheet's own refusal
## line, and the crew cap the Work board's `+` reads. Four surfaces, one answer, so they cannot come to
## disagree about whether the fight is even asked.
##
## **THE ENGAGEMENT STAGE IS THE HONEST HALF, and it covers everything but the pen.** The whole PLANT
## web publishes `NO_ENGAGEMENT_STAGE` — a berry does not fight back, and a patch states no
## `durability` for `hunt_gate_model_at` to be `stated` about — as does a species the roster cannot
## resolve, and refusing a hunt on either would refuse one the sim allows.
##
## ⛔ **THE CORRALLED FLAG IS THE OTHER HALF, BECAUSE THE WIRE'S `engage_rate` IS STILL WRONG ABOUT A
## PEN.** Since `docs/plan_standing_upkeep.md` §4.9 item 12b a penned herd is engaged, retreats and
## fights through the very same take the range runs, at a reach of the species' rate × the pen's
## handling gain — but `core_sim/src/snapshot/subsistence.rs` still filters the published field on
## `is_corralled()` and ships `NO_ENGAGEMENT_STAGE`, deliberately and against its own behaviour, so as
## not to flip the gate other readers use to route pens away from the hunt paths. Its own comment says
## the `0` is no longer the truth, and **issue #572** tracks closing it. Reading that field alone here
## leaves a bare-handed party quoted a real take on a fenced aurochs (`defense 6`) the sim pays nothing
## for — the exact forecast-vs-readout split this predicate exists to close.
##
## **IT IS AN `or`, NOT A FORK, so #572 makes it redundant rather than wrong**: the day a pen publishes
## its real reach the first arm already answers `true` and this clause can be deleted with no change in
## behaviour. `corralled` is decoded on every herd source (`native/src/dict/subsistence.rs`) and is
## absent — so `false` — on every plant one, which is what keeps the plant web out of the pen arm.
static func quarry_is_fought(src: Dictionary, prefix: String) -> bool:
    return is_fought(float(src.get(prefix + FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE)),
        bool(src.get(SOURCE_CORRALLED_KEY, false)))

## The same verdict for a caller holding the two terms already RESOLVED rather than a source and a
## prefix — a COMPOSED forecast, whose keys carry no prefix at all (`forecast_inputs` copies both terms
## onto it side by side for exactly this reader). Spelling the disjunction at the call site instead
## would be a second producer for one rule, which is how a pen came to answer two different ways on
## two surfaces for a release.
static func is_fought(engage_rate: float, corralled: bool) -> bool:
    return corralled or has_engagement_stage(engage_rate)

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
#
# **`hunter_attack` IS THE BEST-EQUIPPED CREW'S TIER, NOT THE BAND'S** (issue #520). It is read off
# `huntCrews[0]` sim-side, so the gate composed from it answers for the best-armed run alone — which
# is the reassuring half of a split party. `hunt_crew_split_model` below is the rest of the sentence.
const BAND_HUNTER_ATTACK_KEY := "hunter_attack"
const HERD_DEFENSE_KEY := "defense"
const HERD_DURABILITY_KEY := "durability"

# **TEN SPEARS ARM TEN HUNTERS AND THE ELEVENTH GOES BARE** (issue #520). `max(0, attack − defense)`
# decides whether a species can be taken AT ALL, so a party the gate clears is not a party that can
# all take it: the crews below the best-equipped one may sit under the same defence at any headcount.
#
# The sentence is in the register the gate already uses — a count, the total it is out of, and what
# the rest of the party is holding — rather than a new visual language. The two tails are the wire's
# own answer to "why can't they": a crew holding NOTHING is bare-handed, a crew holding something the
# defence still shrugs off is merely under-equipped, and `item_ids` is what tells them apart.
const HUNT_CREW_SPLIT_FORMAT := "%s%d of your %d hunters can take %s; the other %d %s and land nothing on it at any headcount."
const HUNT_CREW_SPLIT_BARE_CLAUSE := "are bare-handed"
const HUNT_CREW_SPLIT_UNDER_CLAUSE := "hold too little gear"

# **THE SENTENCE IS ABOUT THE PARTY BEING COMPOSED WHEREVER THERE IS ONE**, and this sentinel is a
# host saying it has no party — the Band/City page's band-level readouts, which speak for the whole
# band. A compose sheet passes its own stepper's count and must never take the default: the band's
# ten spears cover a party of six entirely, and *"7 of your 17 are bare-handed"* over a `HUNTERS 6`
# stepper reads as seven of six.
const HUNT_CREW_PARTY_UNSET := -1

## **WHICH OF THESE HUNTERS CAN ACTUALLY TAKE THIS QUARRY** — `{stated, armed, barred, text}`, both
## counts WHOLE PEOPLE, `text` non-empty only when `stated`.
##
## **THE PARTY IS A PREFIX OF THE CREWS, which is the sim's own coverage model** (`equipment.md` →
## "The partition is by ITEM SET"): each item covers a prefix of the party, and the crews arrive
## best-equipped first, so the first `party_workers` hunters take the best gear the band holds. This
## walk is arithmetic over two published counts — it resolves no tier, no step-down and no coverage
## of its own, which are all the sim's answers already sitting in the rows.
##
## It states nothing at all in six cases, each for its own reason:
## - **ONE CREW.** A uniformly-equipped band publishes exactly one row (never an empty list), so one
##   row IS the normal case and the band-level gate already answers for everybody.
## - **THE PARTY FITS INSIDE THE ARMED PREFIX.** Six hunters drawn from ten spears are all armed;
##   there is no split in THIS party, whatever the rest of the band is holding.
## - **NOBODY ARMED.** The gate's own refusal is rendering instead, and `0 of 17` beside it adds
##   nothing — a party that cannot take the quarry at all is one sentence, not two.
## - **NOBODY ON THE HUNT.** A band with no hunters publishes one crew at `workers 0`; that is not a
##   shortfall of zero out of zero, it is nothing to say.
## - **A PARTY LARGER THAN THE PUBLISHED CREWS.** The crews divide the band's CURRENT hunt workers,
##   and a compose stepper draws on idle ones too — so past that head count the wire's division does
##   not describe the party, and quoting `10 of 13` out of a 12-strong division would invent a row.
## - **A KIT THAT IS NOT THE BAND'S QUOTED ONE** (`kit_id`). The crews are resolved against the hunt
##   job's kit, so quoting them under a kit the player has just picked would describe a division of
##   the party that does not exist for that choice. No line rather than a wrong one.
##
## **THE COUNTS ARE APPORTIONED, NOT ROUNDED APART.** Crew workers are floats (a forecast counts
## hunters in fractions), so rounding each side alone yields a `10` and a `7` that do not make 17.
static func hunt_crew_split_model(band: Dictionary, herd: Dictionary, quarry: String,
        kit_id: String, party_workers: int = HUNT_CREW_PARTY_UNSET) -> Dictionary:
    var blank := {"stated": false, "armed": 0, "barred": 0, "text": ""}
    if kit_id != String(band.get(DetailFormat.BAND_QUOTED_KIT_ID_KEY, "")):
        return blank
    var crews := DetailFormat.band_hunt_crews(band)
    if crews.size() <= 1:
        return blank
    if float(herd.get(HERD_DURABILITY_KEY, 0.0)) <= 0.0:
        return blank
    var headcount := DetailFormat.band_hunt_headcount(band)
    # The party the sentence is about: the composed one, or the whole hunt roster for a host with no
    # stepper. A composed party the crews do not cover is not describable from these rows at all.
    var budget := headcount
    if party_workers != HUNT_CREW_PARTY_UNSET:
        if float(party_workers) > headcount:
            return blank
        budget = float(party_workers)
    var defense := float(herd.get(HERD_DEFENSE_KEY, 0.0))
    var armed := 0.0
    var barred := 0.0
    var barred_bare := true
    var remaining := budget
    for crew in crews:
        if remaining <= DetailFormat.HUNT_CREW_WORKER_EPSILON:
            break
        var taken := minf(maxf(float(crew.get(DetailFormat.HUNT_CREW_WORKERS_KEY, 0.0)), 0.0),
            remaining)
        remaining -= taken
        if maxf(float(crew.get(DetailFormat.HUNT_CREW_ATTACK_KEY, 0.0)) - defense, 0.0) > 0.0:
            armed += taken
            continue
        barred += taken
        if taken > DetailFormat.HUNT_CREW_WORKER_EPSILON \
                and not (crew.get(DetailFormat.HUNT_CREW_ITEM_IDS_KEY, []) as Array).is_empty():
            barred_bare = false
    # **The target is the party the two halves partition.** `armed + barred` is what the crews
    # actually covered of `budget` — the loop stops at whichever runs out first — so that sum, not
    # `budget`, is the total the displayed pair has to add to.
    var parts := HudFormat.apportion_people_to([armed, barred], int(round(armed + barred)))
    if parts[0] <= 0 or parts[1] <= 0:
        return blank
    var clause := HUNT_CREW_SPLIT_BARE_CLAUSE if barred_bare else HUNT_CREW_SPLIT_UNDER_CLAUSE
    return {"stated": true, "armed": parts[0], "barred": parts[1],
        "text": HUNT_CREW_SPLIT_FORMAT % [
            HUNT_FORECAST_WARN_GLYPH, parts[0], parts[0] + parts[1], quarry, parts[1], clause]}

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
## `+0.08 /turn · +0.40 fodder` (a hay meadow), `+0.40 fodder` (a hay-only one), `+0.22 hide` (an
## inedible quarry). One definition, so every surface that states a source's per-turn products states
## them the same way and none of them can print a zero for a component the source does not produce.
## Food leads. When EVERY component is absent the food zero survives ("+0.00 /turn"): a worked source
## that produced nothing this turn is a fact worth reading.
##
## **EVERY ACCOUNT IS SIGNED, AND FOR A RELEASE ONLY THE FOOD ONE WAS.** Food read `+0.20 /turn`
## beside a bare `0.40 fodder` and a bare `0.22 hide` — one list, one register, one account wearing a
## `+` and the others not, which a reader can only take as meaningful. **They are all income**: every
## one of them is a per-turn credit to a store, so they all carry the sign that says so, and the sign
## is what separates them from the standing COSTS this HUD also states (a pen's feed, a rung's
## keeping). The tooltip half of this vocabulary already signed the fodder
## (`POLICY_CAP_FODDER_FORMAT` → `+0.40 fodder/turn`), so the face was the outlier rather than the
## rule.
##
## **`magnitude_components` IS THE UNSIGNED TWIN AND STAYS UNSIGNED** — a filter chip states a LEVEL
## rather than a change, and its own note gives the reason. That pair is what makes this one's sign a
## decision rather than an inconsistency.
##
## The fodder term wears the WORD, not a glyph, because fodder has none — the same reason
## `picker_products` names its accounts. It is plant-only, so every hunt-side caller leaves it
## defaulted and reads exactly as it did.
##
## `zero_account` names the component whose zero survives an all-empty take (`zero_account_of`), so a
## hay-only meadow reads `+0.00 fodder` rather than the `+0.00 /turn` that says its hay is worth no
## meals, and a source that pays nothing in either account renders no line at all.
static func yield_components(food: float, fodder: float = 0.0,
        zero_account: String = YIELD_ACCOUNT_FOOD, materials: Array = []) -> String:
    var parts: Array[String] = []
    for row in yield_rows(food, fodder, zero_account, {}, materials):
        var material_id := _row_material_id(row)
        if material_id != "":
            parts.append(PICKER_MATERIAL_PRODUCT_FORMAT % [
                format_signed(row[YIELD_ROW_VALUE]), material_id])
        elif String(row[YIELD_ROW_ACCOUNT]) == YIELD_ACCOUNT_FOOD:
            parts.append(format_yield(row[YIELD_ROW_VALUE]))
        else:
            parts.append(PICKER_FODDER_PRODUCT_FORMAT % format_signed(row[YIELD_ROW_VALUE]))
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

## **THE ANSWER FOR A TILE THIS CLIENT CANNOT PLACE**, and it is a MEANING rather than a big number: a
## band the wire has not positioned yet is not *far away*, it is *not on the map*. Named because
## comparing against it as if it were a distance — nearest-band pickers do exactly this — silently
## makes an unplaceable band the nearest thing to everything.
const HEX_DISTANCE_UNKNOWN := -1

## Wrap-aware true odd-r hex distance between two offset tiles (mirrors the sim's `hex_distance_wrapped`
## / MapView._hex_distance): bring the target into the source's column frame via _wrapped_col_delta,
## then odd-r offset→axial→cube distance. `HEX_DISTANCE_UNKNOWN` when either tile is unknown.
static func hex_distance_wrapped(a_col: int, a_row: int, b_col: int, b_row: int,
        grid_width: int, wrap_horizontal: bool) -> int:
    if a_col < 0 or a_row < 0 or b_col < 0 or b_row < 0:
        return HEX_DISTANCE_UNKNOWN
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
## **THE ONE READER THAT IS NOT A CEILING IS `build_turns_at`'S WORK PREDICATE** — *is anything
## standing above this floor at all?*, a test on this expression's SIGN rather than a quantity derived
## from it (`systems::labor::crew_is_working_the_source`, on the Cultivate and Tame rungs alone). It is
## admitted for the same reason the ceilings are: the sheet prices a crew and a floor nobody has
## committed, so the sim has no answer to ship for it, and a sheet that omitted the term quoted the
## FASTEST estimate on the axis for a build the sim was not advancing at all. Nothing else may reach
## for it.
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

## **THE ROOM NEXT TURN'S TAKE ACTUALLY HAS — this turn's growth first, then what stands above the
## floor.** The room above is the INSTANTANEOUS one, `B − floor·K`, and on a source held at its floor
## that is EMPTY by construction: a patch harvested back to its floor every turn showed
## `PER TURN 0.00 FOOD` and *"takes nothing until it grows past 103"* beside a work board reading
## `+0.96 /turn` for the same tile. Both were right about different questions — the board quotes the
## sim's forward projection, the sheet quoted the room standing right now — and only one of them is
## the question a compose sheet is asked.
##
## **IT IS THE PROJECTION'S OWN FIRST STEP, not a second model of the take.** `project_stock` regrows
## and THEN takes, every turn, so this is that walk's first turn read out on its own — which is why the
## sheet reconciles with the board automatically at equilibrium, where the take IS the regrowth.
##
## **ZERO STAYS REACHABLE, and that is the honest half.** A source far enough below its floor that one
## turn's growth will not cross it really does pay nothing next turn, and the sheet then says so with
## the same *until it grows past N* sentence — which becomes true exactly when it is shown.
##
## A source with no published curve regrows `0.0` here (`regrowth_at`'s own answer), so this collapses
## to `escapement_room` and every such source reads exactly as it did.
static func escapement_room_next_turn(src: Dictionary, prefix: String, floor: float) -> float:
    var capacity := float(src.get(prefix + FORECAST_CAPACITY_KEY, 0.0))
    if capacity <= 0.0:
        return escapement_room(src, prefix, floor)
    var biomass := clampf(float(src.get(prefix + FORECAST_BIOMASS_KEY, 0.0)), 0.0, capacity)
    var grown := clampf(biomass + regrowth_at(regrowth_samples(src, prefix), biomass / capacity),
        0.0, capacity)
    return maxf(0.0, grown - clamp_floor(floor) * capacity)

## Is this source MANAGED — a built Pen? Such a source is never drawn down, so it pays its managed
## production at EVERY floor and the escapement composition does not apply to it. **The plant web
## answers `false` at every rung**, a Field included: see `FORECAST_MANAGED_FLAG_KEYS`.
##
## **IT IS THE STANDING RUNG THAT DECIDES THIS, NEVER THE COMPOSED ONE.** The wire flag is the only
## input, and a source the crew is merely BUILDING toward rung 3 — mid-`Sow`, mid-`Corral` — is
## deliberately NOT managed, because until the Field or the Pen exists the crew is still drawing the
## WILD stand down, and that drawdown is exactly what the sheet
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
## **ONE BODY, IN THE UNIT THE QUANTISER'S OTHER TWO ARMS ARE IN.** A whole-animal take is
## `min(room, crew carry, what stays) ÷ one body`, and the sim states all four in BIOMASS
## (`fauna::quantise_animal_take` takes a `body_mass` and a biomass ceiling). So does this client: the
## room comes through `escapement_room`, the carry through `per_worker_biomass`, and this is the
## quantum they are divided by.
##
## **IT IS RECOVERED FROM THE FOOD PAIR WHERE ONE EXISTS, AND READ OFF `bodyMass` WHERE IT DOES NOT** —
## which is the same number twice on the wire, `food_per_animal` BEING `body_mass ×
## provisionsPerBiomass` (`snapshot.fbs`). The recovery leads because the room and the carry both reach
## the quantiser THROUGH `provisionsPerBiomass`, and three arms of one `min` must be in one unit: a
## quantum taken from a different pairing than the two numerators makes the count wrong by whatever
## the two pairings disagree by, rather than by a rounding.
##
## **A source that pays no food has no such pair — that is what an inedible quarry IS** — and it is
## exactly the source this function exists for: a wolf's `food_per_animal` and `provisionsPerBiomass`
## are both structural zeros, so the quantum can only be the published `bodyMass`, and without it the
## whole quantiser had nothing to divide by and its material rows fell through to a crew-throughput
## line that ignored the reach.
##
## > **The harness's fixtures are where the two pairings visibly part.** `ForageFx.floorify` derives a
## > patch's / herd's `provisions_per_biomass` from the authored peak ceiling and the room, so a
## > fixture that also states `body_mass` outright ends up with `body_mass × provisions_per_biomass !=
## > food_per_animal` — a herd no server can publish. The recovery makes the take immune to it; the
## > animal COUNTS beside it (`animal_count`, the floor flag, `crew_to_hold`'s rounding) still read the
## > stated field, so those fixtures state one body two ways.
static func body_quantum(src: Dictionary, prefix: String) -> float:
    var per_animal := float(src.get(prefix + FORECAST_FOOD_PER_ANIMAL_KEY, 0.0))
    var rate := float(src.get(prefix + FORECAST_PROVISIONS_PER_BIOMASS_KEY, 0.0))
    if per_animal > 0.0 and rate > 0.0:
        return per_animal / rate
    return maxf(0.0, float(src.get(prefix + FORECAST_BODY_MASS_KEY, 0.0)))

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

## The LEARNING multiplier this floor buys — the sim's `intensification::learn_multiplier`,
## `floor / the food peak`, normalised so the peak is ×1.0. It is what the chart's gradient rail
## encodes, and it is a fact about **these people on this ground**, not a faction knowledge meter: a
## tile knows nothing.
##
## **IT SCALES THE KNOWLEDGE ACCRUAL AND NOTHING ELSE.** It paced the build meter too while one crew
## both gathered and built; `build_supply` reads no floor at all now (`docs/plan_standing_upkeep.md`
## §2.2), so a caller reaching for this to price a build is reading the wrong meter — see
## `build_turns_at`, which takes the floor for the WORK PREDICATE alone.
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
## `animals_engaged` → `animals_stayed` — those take `workers × engage_rate` unrounded and then cut it
## by the retreat, so the crew that lands `n` animals is `ceil(n / (engage_rate × stay))`.
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
static func engage_workers(ceiling: float, body: float, engage_rate: float,
        stay: float) -> int:
    if body <= 0.0:
        return 0
    var reach := engagement_per_worker(engage_rate)
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
static func engagement_per_worker(engage_rate: float) -> float:
    var reach := maxf(engage_rate, 0.0)
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
static func engagement_carry(body_mass: float, engage_rate: float,
        stay: float) -> float:
    if body_mass <= 0.0:
        return ENGAGEMENT_UNBOUNDED
    var reach := engagement_per_worker(engage_rate)
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
        engage_rate: float, stay: float) -> int:
    return maxi(haul_workers(ceiling, body, per_worker),
        engage_workers(ceiling, body, engage_rate, stay))

## **HOW MANY ANIMALS THIS PARTY BRINGS INTO CONTACT THIS TURN** — the client mirror of the sim's
## `fauna::animals_engaged`, and the one definition of it, so no two readings of a herd can disagree
## about how many it could reach. `workers × engage_rate`, **UNROUNDED**: a reach is a RATE, and the
## floor and floor-of-one this used to carry are both retired with the sim's.
##
## **THE ROUNDING IS WHAT MADE EXTRA HUNTERS WORTHLESS.** `floor(w × rate).max(1)` answered one animal
## for every crew from 1 to 6 on the shipped Wild Boar (`engage_rate 0.33`), so four hunters reached
## exactly what one reached — and below `1 / engage_rate` it quoted a reach the turn never delivered,
## three times over for a lone boar hunter. The reach is strictly increasing in the crew now, which is
## the property the whole engagement stage exists to have.
##
## **NOTHING NEEDS A FLOOR OF ONE, BECAUSE NOTHING REACHES ZERO.** The retired minimum defended a
## headcount threshold standing in front of the attack-vs-defense gate — with the floor gone,
## `floor(1 × 0.05)` would have been `0` and a lone mammoth hunter would fail for the wrong reason. A
## plain multiply cannot produce that (a lone mammoth hunter reaches `0.05`), and the sub-one-animal
## fraction is not a failure to arrive: the sim's retreat and wound ledger carry it between turns.
## **A party of no workers engages nothing**, which is a different statement and is why the worker test
## comes first.
##
## `ENGAGEMENT_UNBOUNDED` for a source with no engagement stage, so the caller's `min()` drops the arm.
static func animals_engaged(workers: int, engage_rate: float) -> float:
    if workers <= 0:
        return 0.0
    if engage_rate <= NO_ENGAGEMENT_STAGE:
        return ENGAGEMENT_UNBOUNDED
    return float(workers) * engage_rate

## **HOW MANY OF THE ENGAGED ANIMALS STAY TO BE FOUGHT** — the retreat, the stage between engagement
## and the fight, mirroring `fauna::animals_that_stay` at the quantile a FORECAST reads it at. The sim
## draws a binomial per animal; a forecast cannot draw, so it takes the analytic mean
## `engaged × stay`, which is `snapshot.fbs`'s own `stayers = workers × engageRate × stayFraction`.
## **Neither stage rounds** — `animals_engaged` reaches a rate and this cuts a rate by a fraction, and
## the whole-animal quantisation is the take's, one `min()` further on.
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

# ---- THE SIM'S OWN ANSWER TO "WHAT DOES THIS CREW BRING DOWN" -----------------------------------
#
# > #### ⛔ THE TWO FUNCTIONS ABOVE ARE NOT THAT ANSWER, AND A COMPOSE SHEET MAY NOT USE THEM AS ONE
# >
# > `animals_stayed(animals_engaged(w, rate), stay)` is the engagement and the retreat — the first two
# > of the take's THREE stages. The third is the FIGHT, damage over durability against the quarry's
# > defense and the multi-turn wound ledger it is standing there with, and the client cannot compute
# > it: `combat_config.hit_chance` is deliberately unpublished and the schema names that division as
# > one of the non-linear halves that stay the sim's answer. Measured on a Wild Aurochs with four
# > hunters, the two-stage form read **1.92 food** where the herd paid **0.84** — and every yield
# > beside it was over by the same 2.3×, all four being fixed conversions of one biomass.
# >
# > They survive because the two stages ARE the honest bound on the client's other readings — the
# > projection walk, the crew targets, the stepper's cap — which are statements about how fast a stock
# > can be drawn down rather than about what lands in the stores. The pre-commit TAKE asks
# > (`ForecastQuery.KIND_HUNT_CREW_TAKE`).
#
# **AND NO PER-HUNTER RATE COULD HAVE REPLACED THE CURVE.** On the shipped Wild Boar the per-hunter
# take spans 6× across crews of 1 to 6 (the engagement is a staircase, flat across whole runs and
# stepping at integer boundaries); on Wild Aurochs the binding term flips from the fight to the
# engagement inside the stepper's own range and back again. A row is the WHOLE crew's take per turn:
# **never multiply it by the crew size.**
const CREW_TAKE_WORKERS_KEY := "workers"
const CREW_TAKE_LOW_KEY := "animals_low"
const CREW_TAKE_LIKELY_KEY := "animals_likely"
const CREW_TAKE_HIGH_KEY := "animals_high"

## **THIS CREW'S ROW OUT OF THE CURVE**, or `{}` when the answer does not cover it — a reply that has
## not landed, a crew past the cap the question was asked with, or a crew of none.
##
## The reply is one row per crew from `1`, so the row is at `workers - 1`; the row's own echoed
## `workers` is then CHECKED rather than trusted, which is what the sim echoes it for. A curve whose
## index and echo disagree is a desynchronised answer, and quoting the wrong crew's take off it is
## precisely the class of error this whole channel exists to remove.
static func hunt_crew_take_row(per_crew: Array, workers: int) -> Dictionary:
    if workers <= 0 or workers > per_crew.size():
        return {}
    var row_variant: Variant = per_crew[workers - 1]
    if not (row_variant is Dictionary):
        return {}
    var row: Dictionary = row_variant
    if int(row.get(CREW_TAKE_WORKERS_KEY, 0)) != workers:
        return {}
    return row

## **IS THE BAND DEGENERATE?** — `low == likely == high`, which is what both stochastic stages answer
## at the shipped tuning (`combat_config.hit_chance = 1.0`, a species at `wariness 0`): the two
## binomials return their degenerate identity whatever quantile is asked for, so the three quantiles
## are bit-identical rather than merely close.
##
## **RANGE CHROME THAT ALWAYS RENDERS MANUFACTURES DOUBT THE MODEL DOES NOT HAVE**, so the readout
## prints the bare figure here and the range only where there is one. `is_equal_approx` rather than
## `==`, because the numbers arrive through a `f32` → `f64` widening on the wire.
static func hunt_take_band_is_degenerate(low: float, likely: float, high: float) -> bool:
    return is_equal_approx(low, likely) and is_equal_approx(likely, high)

# ---- READING THE CURVE AS A TABLE, WHICH IS THE WHOLE POINT OF IT ------------------------------
#
# > #### ⛔ EVERY CREW QUESTION ON A HUNT SHEET IS A SEARCH OF THESE ROWS, NOT ARITHMETIC
# >
# > *"How many hunters clear the room this turn"*, *"how many hold it once it is at the floor"*, *"how
# > many draw it down at all"* and *"where do extra hands stop helping"* are four questions about ONE
# > function — the take as a function of crew size — and the sim publishes that function. Inverting it
# > on the client is what produced the 2.3× the take line has already paid for: the closed forms below
# > this section divide a room by `min(carry, engagement_carry)`, and `engagement_carry` is the
# > engagement and the retreat with **no attack, no defense and no durability** in it. A panel whose
# > take came off the curve while its crew pills came off that quotient was two models on one sheet,
# > and it read `13 of 37 useful` two lines above a take of nothing.
# >
# > So the pills are LOOKUPS. `crew_take_reaching` walks the rows for the first crew that meets a
# > target, `crew_take_plateau` walks them for where they stop rising; neither divides anything.
#
# **THE CLOSED FORMS SURVIVE AS THE NO-CURVE PATH**, and they have one caller left: a surface with no
# reply in hand. The Work board's `+` gate (`source_worker_cap_state`) prices a worked row without
# ever asking the socket, and the compose sheet itself renders one frame before its answer lands. Both
# want the honest two-stage bound rather than silence, so every function below takes the curve as an
# OPTIONAL trailing argument and falls back where there is none.

## A curve row's take is unusable as a number — no reply, or a row the fixture/wire left non-finite.
## `animals_engaged` answers `ENGAGEMENT_UNBOUNDED` for a source with no engagement stage (a pen, the
## whole plant web), so an INF row is a real shape on this channel and NOT a curve: "every crew takes
## infinitely many" plateaus at one worker and would cap a pen's stepper at one hand.
const CREW_TAKE_NO_ROW := -1.0

## **HOW CLOSE COUNTS AS REACHING A TARGET.** The curve's own top row is `min(fight, stayed)` and
## `stayed` is itself clamped by the room, so *"the crew that clears the room"* is a search for a row
## that lands ON its ceiling rather than past it — and the two sides of that comparison have travelled
## through a `f32` wire widening and a body-mass division. Relative, not absolute, because the targets
## span a Wild Fowl's hundreds of animals a turn and a mammoth's hundredths.
const CREW_TAKE_REACH_TOLERANCE := 0.001

## **IS THERE A CURVE TO READ?** — non-empty, and every row a finite take. Asked before any of the
## searches below, so a caller gets the pre-curve closed form rather than an answer composed out of
## `INF` or out of a half-decoded reply.
static func has_crew_take_curve(per_crew: Array) -> bool:
    if per_crew.is_empty():
        return false
    for row_variant in per_crew:
        if not (row_variant is Dictionary):
            return false
        var row: Dictionary = row_variant
        if not is_finite(float(row.get(CREW_TAKE_LIKELY_KEY, INF))):
            return false
    return true

## The whole crew's likely take at `workers`, in ANIMALS per turn, or `CREW_TAKE_NO_ROW` where the
## curve does not cover it. **Never multiplied by the crew size** — see `hunt_crew_take_row`.
static func crew_take_likely(per_crew: Array, workers: int) -> float:
    var row := hunt_crew_take_row(per_crew, workers)
    return CREW_TAKE_NO_ROW if row.is_empty() else float(row[CREW_TAKE_LIKELY_KEY])

## **THAT TAKE IN BIOMASS** — the unit the projection walks and the crew targets compare in, so the
## chart's drawdown descends at the rate the sim would actually pay rather than at the rate the crew
## could reach. `ENGAGEMENT_UNBOUNDED` where there is no row or no body, which is exactly what
## `engaged_quantum` answers in the same states: the caller's `min()` drops the arm and the plant web
## and the pens are unmoved.
static func crew_take_biomass(per_crew: Array, workers: int, body_mass: float) -> float:
    if body_mass <= 0.0:
        return ENGAGEMENT_UNBOUNDED
    var animals := crew_take_likely(per_crew, workers)
    return ENGAGEMENT_UNBOUNDED if animals <= CREW_TAKE_NO_ROW else animals * body_mass

## **THE SMALLEST CREW IN THE CURVE WHOSE TAKE REACHES `animals` A TURN**, or `NO_CREW_ANSWER` when no
## crew the question was asked about gets there.
##
## **`NO_CREW_ANSWER` IS A REAL AND USEFUL ANSWER HERE, and the pill states it as `✕`, disabled**
## (`HudWidgets.build_crew_targets`). The curve is asked for crews 1..the band's own pool, so "no row
## reaches it" means *this band cannot do it at any size it can field* — and the retreat makes that
## permanent rather than a matter of pool size, since `stayed` caps at `room × stayFraction` and a
## quarry that scatters can never be cleared in one turn by anybody. Naming a count nobody can staff
## would be the §7.6 failure in its purest form: a target the stepper beside it refuses to reach. The
## pill used to be dropped instead, which said nothing where the answer is a definite *no*.
static func crew_take_reaching(per_crew: Array, animals: float) -> int:
    if not has_crew_take_curve(per_crew) or animals <= 0.0:
        return NO_CREW_ANSWER
    var target := animals * (1.0 - CREW_TAKE_REACH_TOLERANCE)
    for workers in range(1, per_crew.size() + 1):
        if crew_take_likely(per_crew, workers) >= target:
            return workers
    return NO_CREW_ANSWER

## **WHERE THE CURVE STOPS RISING** — the smallest crew no larger crew out-takes, which is what
## *"max N workers useful here"* has always meant and what the closed form could only guess at.
##
## **IT IS THE LAST RISE, NOT THE FIRST FLAT.** The reach itself rises with every hand now, but the
## bounds around it do not: the room clamp and the whole-animal quantiser both hold a run of crews at
## one figure before the next steps, so a scan that stopped at the first crew whose take equalled its
## predecessor's would report the bottom of a tread as the top of the stairs. It was reported on the
## shipped Wild Boar back when the reach was floored and crews one through six all brought the same
## single animal to bay — the tread is shorter now, and the reading it breaks is the same one.
##
## `NO_CREW_ANSWER` where there is no curve; a curve that rises to its own last row plateaus AT that
## row, which is the honest answer to a question asked about a bounded pool — every hand the band has
## is still buying take.
##
## **A CURVE OF ZEROES PLATEAUS AT NOBODY (`PUBLISHED_NO_USEFUL_CREW`), NOT AT ONE.** This walk is a
## deliberate second implementation of the sim's `fauna::hunt_useful_crew`, so it has to agree with it
## everywhere, and the sim's own doc names this case: *"a bare-handed party against a `defense` it
## cannot clear lands exactly zero however many people it sends, and one worker is useful would be a
## false floor."* Seeding at `1` printed exactly that false floor — reported from play as
## `max 1 worker useful here` on a Rabbit Warren whose wire row carried `huntUsefulWorkers: 0`, the
## two readings of one curve disagreeing by the whole of the answer. So the scan starts from nothing
## and only a row that genuinely rises above zero names a crew, which is the sim's loop line for line.
##
## **THIS IS A READING OF THE CURVE, NOT A CAP ON THE STEPPER**, and the distinction is load-bearing:
## `max_useful_workers` turns a zero here into `MAX_USEFUL_BARREN` rather than passing it on, because
## *may I staff this at all* is a different question from *would another hand buy more take*. Its
## comment carries the play report that made the difference visible.
static func crew_take_plateau(per_crew: Array) -> int:
    if not has_crew_take_curve(per_crew):
        return NO_CREW_ANSWER
    var plateau := PUBLISHED_NO_USEFUL_CREW
    var best := 0.0
    for workers in range(1, per_crew.size() + 1):
        var take := crew_take_likely(per_crew, workers)
        # A non-finite row is not a bigger take, it is an unpriceable one — the sim's own guard, kept
        # here so the two walks cannot differ on a source with no engagement stage.
        if is_finite(take) and take > best * (1.0 + CREW_TAKE_REACH_TOLERANCE):
            best = take
            plateau = workers
    return plateau

## > #### ⛔ A CURVE STILL RISING AT ITS LAST ROW HAS NOT ANSWERED — IT HAS RUN OUT OF DOMAIN
##
## The question is asked for crews `1..the band's own pool` (`DrawerComposeController._crew_take_view`),
## because that is every crew the stepper beside it can reach. But two readings on this sheet are about
## crews the band CANNOT field: *"26 of 47 useful — free up idle workers to send more"* names the
## demand-side ceiling as the thing to work toward, and the *clear it now* pill on a Wild Fowl herd
## named 47 hands to a band holding 26. Truncating those at the pool does not make them honest, it
## deletes them.
##
## So the rule is a domain rule, and it is the same one `expedition_useful_cap` already runs on the
## raid branch: **inside the asked range the curve is the only authority; past it, where the panel can
## neither staff nor promise anything, the closed forms answer.** A curve that PLATEAUED below a target
## has answered — no crew reaches it, at any size — and the closed form does not get to overrule that.
## A curve still climbing when the rows ran out has said nothing about the crews past its edge.
static func crew_take_curve_settled(per_crew: Array) -> bool:
    var plateau := crew_take_plateau(per_crew)
    return plateau != NO_CREW_ANSWER and plateau < per_crew.size()

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
        body_mass: float, engage_rate: float,
        stay: float = STAY_FRACTION_NONE_BREAKS_OFF, per_crew: Array = []) -> int:
    if not can_price_crew(carry):
        return NO_CREW_ANSWER
    if room <= 0.0:
        return 0
    # **WITH A CURVE IN HAND THIS IS A SEARCH, NOT A QUOTIENT** — the first crew whose published take
    # covers the room. The quotient below it inverts `engagement_carry`, which has no fight in it, so
    # on a quarry the party can reach but cannot kill it named a crew that clears nothing; the curve
    # simply never reaches the room there and the pill states `✕` (`crew_take_reaching`).
    if has_crew_take_curve(per_crew) and body_mass > 0.0:
        var found := crew_take_reaching(per_crew, room / body_mass)
        if found != NO_CREW_ANSWER:
            return maxi(found, maxi(reaching, 0))
        if crew_take_curve_settled(per_crew):
            # The curve levelled off below the room: nobody clears this herd in a turn, at any size.
            return NO_CREW_ANSWER
        # …still rising when the rows ran out — see `crew_take_curve_settled`.
    var per_worker := minf(carry, engagement_carry(body_mass, engage_rate, stay))
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
        body_mass: float, engage_rate: float, stay: float, per_crew: Array = []) -> int:
    if not can_price_crew(carry):
        return NO_CREW_ANSWER
    var growth := regrowth_at(samples, clamp_floor(floor))
    if growth <= 0.0:
        return 0
    # **THE CURVE ANSWERS IT DIRECTLY** — the first crew whose take covers what grows back. The
    # `take_workers` form below is the sim's `max(haul, engage)`, which sizes the crew that REACHES
    # and CARRIES the regrowth and never asks whether it can kill what it reaches.
    if has_crew_take_curve(per_crew) and body_mass > 0.0:
        var found := crew_take_reaching(per_crew, growth / body_mass)
        if found != NO_CREW_ANSWER or crew_take_curve_settled(per_crew):
            return found
        # …still rising when the rows ran out — see `crew_take_curve_settled`.
    if body_mass > 0.0:
        return take_workers(growth, body_mass, carry, engage_rate, stay)
    return maxi(1, ceili(growth / carry))

## The same *hold it after* crew, resolved straight from a SOURCE — the form `forecast_inputs` carries
## so `max_useful_workers` can floor itself on it. `0` (never `NO_CREW_ANSWER`) when there is no crew
## to name, because this answer is only ever a FLOOR on a cap: a dead-season patch prices no crew, and
## a floor of "unpriceable" is a floor of none.
##
## **A MANAGED SOURCE IS EXCLUDED**, on the same grounds its ceiling is: the sim never draws a built
## Pen down, so "the crew that takes what grows back" is not a question its wire curve answers — its
## cap is `production / per_worker`, and flooring that on a wild-drawdown number would staff a source
## against a projection it does not follow. **A FIELD IS NOT SUCH A SOURCE** — the plant web's managed
## rung is retired and a Field is drawn down like any other stand, so this floor answers for one; see
## `FORECAST_MANAGED_FLAG_KEYS` for what a Field reading `0` here cost.
static func hold_crew(src: Dictionary, kind: String, prefix: String, floor: float,
        per_crew: Array = []) -> int:
    if source_is_managed(src, kind, prefix):
        return 0
    var crew := crew_to_hold(regrowth_samples(src, prefix), floor,
        per_worker_biomass(src, prefix),
        float(src.get(prefix + FORECAST_BODY_MASS_KEY, 0.0)),
        float(src.get(prefix + FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE)),
        float(src.get(prefix + FORECAST_STAY_FRACTION_KEY, STAY_FRACTION_NONE_BREAKS_OFF)),
        per_crew)
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
## A MANAGED SOURCE IS EXCLUDED, exactly as it is for the hold crew: the sim never draws a built Pen
## down, so a drawdown projection says nothing about how many hands it can use. A Field is NOT one —
## see `FORECAST_MANAGED_FLAG_KEYS`.
static func reach_crew(src: Dictionary, kind: String, prefix: String, floor: float,
        per_crew: Array = []) -> int:
    if source_is_managed(src, kind, prefix):
        return 0
    var crew := crew_that_reaches(regrowth_samples(src, prefix),
        float(src.get(prefix + FORECAST_BIOMASS_KEY, 0.0)),
        float(src.get(prefix + FORECAST_CAPACITY_KEY, 0.0)), floor,
        per_worker_biomass(src, prefix),
        float(src.get(prefix + FORECAST_BODY_MASS_KEY, 0.0)),
        float(src.get(prefix + FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE)),
        float(src.get(prefix + FORECAST_STAY_FRACTION_KEY, STAY_FRACTION_NONE_BREAKS_OFF)),
        per_crew)
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
        floor: float, carry: float, body_mass: float, engage_rate: float,
        stay: float = STAY_FRACTION_NONE_BREAKS_OFF, per_crew: Array = []) -> int:
    if not can_price_crew(carry) or capacity <= 0.0:
        return NO_CREW_ANSWER
    var start_fraction := clampf(biomass / capacity, 0.0, 1.0)
    var floor_fraction := clamp_floor(floor)
    if start_fraction <= floor_fraction:
        return 0
    var peak := peak_regrowth_between(samples, floor_fraction, start_fraction)
    # **THE SEED IS THE CURVE'S OWN ANSWER TO "WHO OUT-TAKES THE REGROWTH"**, where there is a curve:
    # the smallest crew whose published take covers the largest regrowth on the way down. Seeding off
    # the fightless quotient instead put the probe BELOW the answer on any quarry the fight binds, and
    # the probe's eight steps are a horizon check rather than a search — they cannot climb far.
    var curved := has_crew_take_curve(per_crew) and body_mass > 0.0
    var need := 0
    if curved:
        need = crew_take_reaching(per_crew, maxf(peak, 0.0) / body_mass)
        if need == NO_CREW_ANSWER and crew_take_curve_settled(per_crew):
            # The curve levelled off below the regrowth, so no crew out-takes it at any size and none
            # of them draws the stock down — the verdict's "can't draw it that low" with no remedy.
            return NO_CREW_ANSWER
        if need == NO_CREW_ANSWER:
            # …still rising when the rows ran out — see `crew_take_curve_settled`.
            curved = false
    if not curved:
        var per_worker := minf(carry, engagement_carry(body_mass, engage_rate, stay))
        need = maxi(1, floori(maxf(peak, 0.0) / per_worker) + 1)
    for _step in range(CREW_PROBE_STEPS):
        # **THE PROBE MAY NOT STEP OFF THE END OF THE CURVE.** `crew_take_biomass` answers
        # `ENGAGEMENT_UNBOUNDED` for a crew it has no row for — the honest reading of "the arm says
        # nothing" everywhere else in this file, and a walk given it would descend at its carry alone
        # and report that an unaskable crew reaches the floor.
        if curved and need > per_crew.size():
            return NO_CREW_ANSWER
        var walk := project_stock(samples, biomass, capacity, floor, float(need) * carry,
            crew_take_biomass(per_crew, need, body_mass) if curved \
                else engaged_quantum(need, body_mass, engage_rate, stay))
        if int(walk["reached_turn"]) != PROJECTION_REACHED_NONE:
            return need
        need += 1
    return NO_CREW_ANSWER

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
#
# **IT STATES THE COUNTDOWN AND NOTHING ELSE.** Both readings once closed with *", then holds it —
# taking only what grows back"*, and that clause is gone: what the source does once it ARRIVES is the
# `VERDICT_HOLDS_AT_FLOOR` sentence's own job, said by the sheet the moment it is true, so a
# countdown that also narrated the aftermath was answering a question the player had not reached yet.
#
# **AND THAT IS WHY THERE IS ONE PAIR HERE RATHER THAN TWO.** A STRIPPED twin of each existed solely
# to drop that clause where there was no aftermath to promise — a herd taken to floor 0 is gone, and
# the full sentence contradicted the sheet's own `0 hold it after`. With the clause off the base
# form the two spellings were the same string, and a `regrows` flag choosing between identical
# constants is a fork with one answer; `harvest_verdict` therefore takes no such flag. What stripping
# costs is still said, by the aside's `FLOOR_STRIP_CONSEQUENCE`.
const VERDICT_REACHES_FORMAT := "Reaches the floor in %d turns."
# A crew big enough to clear the source in one turn is common (it is the `clear it now` target), so
# "1 turns" is a reading the panel would print often rather than an edge case worth tolerating.
const VERDICT_REACHES_ONE_TURN := "Reaches the floor next turn."
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
# THE SOURCE IS AT ITS FLOOR AND THE CREW IS LIVING OFF THE REGROWTH — the equilibrium the *reaches*
# branch below promises, seen from inside rather than counted down to. It states no turn count because
# there is nothing left to wait for, and it wears the same words the reaching verdict closes with, so
# arriving at the floor and standing on it read as one state rather than two.
const VERDICT_HOLDS_AT_FLOOR := "At the floor and holding it — taking only what grows back."
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
## Its MATERIAL twin, one clause per material — ` · brings home 3.20 hide`. The verb is repeated
## rather than the two accounts sharing one "brings home", because the food clause is optional and a
## shared verb would strand the materials on a quarry that pays no meat — which is precisely the
## quarry this clause exists for.
const DENIAL_TAKE_MATERIAL_FORMAT := " · brings home %s %s"
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
# **THE TAIL IS ABOUT THE LESSON AND NOTHING ELSE NOW** (`docs/plan_standing_upkeep.md` §2.2). It
# used to fork on whether a build was in flight, because one multiplier paced the build meter and the
# lesson together — *a crew pulling hard on the source it is improving builds slowly*. **With separate
# crews the build crew is not pulling anything**, so `learn_multiplier` scales the KNOWLEDGE accrual
# alone: the floor buys a faster LESSON and nothing else, and a builder told otherwise would be handed
# the wrong remedy for a slow build (the remedy is hands, on the builders' own stepper).
#
# `TEACHING_RATE_BUILD_TAIL` and `TEACHING_BUILD_ONLY_FORMAT` went with the term. A KNOWN lesson on a
# building source now states no line at all, which is what it always meant: the top of the dial buys
# that source nothing further.
const TEACHING_RATE_FLOOR_TAIL := " — a higher floor teaches faster."
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

## **IS THIS CREW TAKING ANYTHING NEXT TURN?** — the FORWARD-LOOKING twin of `crew_is_taking`, and the
## only one a sentence about what the crew is EARNING may be keyed on.
##
## `crew_is_taking` tests the room standing right now, `B − floor·K`, against the wire's PUBLISHED
## biomass — which is the POST-take stock, so on a source held at its floor it is false by
## construction. That is the intended steady state of a Sustain policy, not an edge case: a patch
## publishing `+0.71 /turn` rendered *"Teaching nothing: nothing is being taken"* beside it, and the
## sim's own lesson gate was meanwhile firing at full multiplier off `biomass_before` — the
## post-regrowth, PRE-take stock (`systems::labor`), which on that patch stands well above the floor.
##
## `room_next_turn` is `escapement_room_next_turn` — this turn's growth first, then what stands above
## the floor — which is the same room the readout's headline and the verdict's at-the-floor refusal
## are composed from, so no two of the three can disagree about whether a take is happening.
static func crew_is_taking_next_turn(workers: int, room_next_turn: float) -> bool:
    return workers > 0 and room_next_turn > 0.0

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
        lesson_known: bool) -> Dictionary:
    if lesson == "":
        return {}
    # **A LESSON ALREADY LEARNED LEAVES NOTHING FOR THE DIAL TO BUY ON THIS SOURCE.** It used to keep
    # a BUILDING half — one multiplier paced both — and that half retired with the floor's term on the
    # build rate (`docs/plan_standing_upkeep.md` §2.2). Silence is the honest answer: the top of the
    # dial teaches a craft the faction already has, and paces no build at all.
    if lesson_known:
        return {}
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
            + TEACHING_RATE_FLOOR_TAIL,
        "teaching": true,
    }

## The verdict for a crew at a floor, as `{severity, text}`. `crew_noun` is the sheet's own word for
## these workers (foragers / hunters / herders), lower-cased by the caller that owns it.
##
## **IT TAKES NO `regrows` TERM.** One existed to drop the reaching sentence's *"then holds it"*
## clause where there was no aftermath to promise; the clause is off both readings now, so the flag
## chose between two identical strings. See `VERDICT_REACHES_FORMAT`.
static func harvest_verdict(walk: Dictionary, workers: int, biomass: float, capacity: float,
        floor: float, reaching_crew: int, crew_noun: String,
        body_mass: float = 0.0, quarry: String = "",
        takes_next_turn: bool = true) -> Dictionary:
    if workers <= 0:
        return {"severity": VERDICT_BLOCKED, "text": VERDICT_NO_CREW}
    var floor_stock := clamp_floor(floor) * capacity
    # **THE AT-FLOOR REFUSAL IS FORWARD-LOOKING NOW, and it has to be**: the headline above it states
    # what NEXT turn's take is, so a sentence keyed on the INSTANTANEOUS room would say *"takes
    # nothing"* over a live figure on every source held at its floor — which is exactly the pair
    # reported from play. `takes_next_turn` is the same room that headline is composed from, so the
    # two cannot disagree, and the sentence is true precisely when it renders.
    if not takes_next_turn:
        return {
            "severity": VERDICT_BLOCKED,
            "text": VERDICT_AT_FLOOR_FORMAT % stock_face(floor_stock, body_mass, quarry),
        }
    # **STANDING AT THE FLOOR AND PAYING THE REGROWTH IS ITS OWN STATE**, and neither branch below can
    # word it: the walk never "reaches" a floor it started on (`project_stock` requires a DESCENT), so
    # this would fall through to *"this crew can't draw it that low"* about a source already there.
    if not crew_is_taking(workers, biomass, capacity, floor):
        return {"severity": VERDICT_OK, "text": VERDICT_HOLDS_AT_FLOOR}
    var reached := int(walk.get("reached_turn", PROJECTION_REACHED_NONE))
    if reached != PROJECTION_REACHED_NONE:
        return {
            "severity": VERDICT_OK,
            "text": VERDICT_REACHES_ONE_TURN if reached == 1 else VERDICT_REACHES_FORMAT % reached,
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
## **`per_crew` IS THE SIM'S TAKE CURVE, AND EVERY CREW ANSWER ON THIS MODEL IS A SEARCH OF IT** —
## the walk's take arm, both pills, the reaching crew and therefore the verdict's *"settles at N%"*
## and the count it offers as the remedy. Empty on the plant web, on the raid branch and for the one
## frame before the reply lands, where every one of them falls back to the closed forms. See the
## curve-reading section above for why leaving them there was the panel being two models at once.
static func floor_chart_model(src: Dictionary, kind: String, prefix: String, floor: float,
        workers: int, crew_noun: String, lesson_known: bool,
        per_crew: Array = []) -> Dictionary:
    var capacity := float(src.get(prefix + FORECAST_CAPACITY_KEY, 0.0))
    var biomass := float(src.get(prefix + FORECAST_BIOMASS_KEY, 0.0))
    var samples := regrowth_samples(src, prefix)
    var known: bool = capacity > 0.0 and has_growth_curve(samples) \
        and not source_is_managed(src, kind, prefix)
    var floor_value := clamp_floor(floor)
    if not known:
        return {"known": false, "floor": floor_value}
    var carry := per_worker_biomass(src, prefix)
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
    # **THE WALK DESCENDS AT WHAT THE PARTY PUTS ON THE GROUND**, which is the curve's row where there
    # is one and the fightless engagement quantum where there is not. Both are the take's arm in
    # biomass, so the picture is the same picture — only its third bound got the fight.
    var curve_take := crew_take_biomass(per_crew, workers, body_mass) \
        if has_crew_take_curve(per_crew) else ENGAGEMENT_UNBOUNDED
    var walk := project_stock(samples, biomass, capacity, floor_value, float(workers) * carry,
        curve_take if is_finite(curve_take) \
            else engaged_quantum(workers, body_mass, engage_rate, stay))
    # **ALL THREE CREW ANSWERS CARRY THE RETREAT.** They ask different questions about different stocks
    # — hold the regrowth, clear the room this turn, draw the stock down at all — and they may disagree
    # on that account, but they may not disagree about how many animals a hand lands. `crew_to_hold`
    # was the last one sized on the RAW reach, on the grounds that it is the sim's `hunt_take_workers`
    # and the stepper cap floors on it; the consequence was a cap BELOW the *clear it now* pill on the
    # same sheet (82 against 108), naming a crew the panel then refused to let the player assign.
    var hold := crew_to_hold(samples, floor_value, carry, body_mass, engage_rate, stay, per_crew)
    var reaching := crew_that_reaches(samples, biomass, capacity, floor_value, carry, body_mass,
        engage_rate, stay, per_crew)
    var quarry := herd_display_name(src) if kind == SOURCE_KIND_HERD else ""
    # **THE ROOM NEXT TURN, RESOLVED ONCE.** Two sentences on this model are keyed on it — the
    # verdict's at-the-floor refusal and the teaching line's "nothing is being taken" — and both were
    # once free to answer it from a different reading. Composing it here is what makes them one answer.
    var room_next_turn := escapement_room_next_turn(src, prefix, floor_value)
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
        # **IS THE FLOOR ITSELF MOVING?** The flag states the floor in ANIMALS — `floor × K` divided
        # by a body — and a build in flight raises `K` every turn, so that count climbs while the
        # percentage beside it sits still. A player who cannot see it move reads the take falling as
        # the herd being poor rather than as the threshold rising under it, which is the reading a
        # gentling herd produces for as long as the build runs.
        #
        # **A DIRECTION, NEVER A MAGNITUDE.** Nothing on the wire says what next turn's capacity is —
        # `buildTurnsRemaining` says only how much longer the climb has to run — so the flag marks that
        # the number is in motion and declines to guess how far.
        #
        # **BOTH TERMS, AND NEITHER ALONE.** A COUNTDOWN without a rung in flight is a fixture's (and
        # the wire's) idle figure on a source nobody is building — the healthy grazing herd publishes
        # one at a meter of zero — so it would mark every ordinary sheet. A RUNG in flight without a
        # countdown is a build that is parked, held or blocked (`BUILD_TURNS_*` are all negative), and
        # nothing is rising there either. `build_verb` is the same newest-meter-first walk the rung
        # rows read, so the flag and the card cannot disagree about whether a build is running.
        "floor_climbing": build_verb(src, prefix, kind) != IMPROVEMENT_NONE
            and int(src.get(prefix + FORECAST_BUILD_TURNS_KEY,
                BUILD_TURNS_NONE_TO_STATE)) > BUILD_TURNS_NONE_TO_STATE,
        # **…AND WHAT IT IS CLIMBING TOWARD.** The mark above says the threshold is moving; these two
        # say where it stops — the capacity the source will hold at the rung the build was sent to,
        # and that rung's own badge word. The flag multiplies the first by the live floor, which is
        # why the CAPACITY rides the model rather than a composed string: the player drags the floor
        # without recomposing this dict, and a precomposed count would freeze at the floor it was
        # struck at.
        #
        # **`NO_BUILD_DESTINATION_CAPACITY` AND `""` ARE THE ABSENT READINGS**, and the flag renders
        # NO CLAUSE for either — an unqueued source has no destination, and a rung this client's table
        # cannot name has no word to hang a figure on. Never a zero: a real source really can hold
        # nothing.
        "destination_capacity": build_destination_capacity(src, prefix),
        "destination_rung": DetailFormat.rung_badge_word(build_destination_rung(src, prefix)),
        "learn_multiplier": learn_multiplier(floor_value),
        "crew_to_clear": crew_to_clear(escapement_room(src, prefix, floor_value), carry, reaching,
            body_mass, engage_rate, stay, per_crew),
        "crew_to_hold": hold,
        # `takes_next_turn` from the SAME room the readout's headline is composed from
        # (`escapement_room_next_turn`), so the sentence and the number above it are one answer.
        "verdict": harvest_verdict(walk, workers, biomass, capacity, floor_value, reaching,
            crew_noun, body_mass, quarry, room_next_turn > 0.0),
        # THE ASIDE'S SECOND LINE, composed HERE rather than at the render site for the same reason
        # the verdict and the idle note are: it is a function of the floor, so it has to be recomposed
        # by every live drag, and this model IS what a drag recomposes. **It takes no `improvement`**:
        # the line is about the LESSON alone now (`docs/plan_standing_upkeep.md` §2.2), and a build in
        # flight neither speeds it up nor is paced by it.
        # …and its `taking` term is the FORWARD room, the same one the verdict above is keyed on
        # (`crew_is_taking_next_turn`). It read the instantaneous `crew_is_taking`, which is false by
        # construction on any source held at its floor — so a patch publishing a live take, whose
        # lesson the sim was crediting at full multiplier, was told *"nothing is being taken"* while
        # the verdict one row up correctly read *"At the floor and holding it"*.
        "teaching_note": teaching_note(rung_lesson(kind, src, prefix), floor_value,
            crew_is_taking_next_turn(workers, room_next_turn), lesson_known),
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
## UNDIPPED while the sim paid `workers × per_worker × build_dip` — a herd mid-Tame or mid-Corral quoted
## roughly 4× what it would be paid, and the sheet's own worker cap and chart (which DO carry the verb)
## disagreed with the take beside them. A caller that genuinely wants the undipped rates — the SUSTAIN
## reference the take is judged against — must now say `IMPROVEMENT_NONE` out loud rather than get it by
## omission. The dip rides `per_worker` ALONE (`forecast_inputs` → §3.1): `ceiling` and `per_animal` come
## back undipped either way, so a build changes what the crew CARRIES, never the room or the body it is
## quantised against.
static func herd_axis_rates(herd: Dictionary, floor: float) -> Dictionary:
    var forecast := forecast_inputs(herd, SOURCE_KIND_HERD, "", floor)
    return {
        "per_worker": float(forecast["axis_per_worker"]),
        "ceiling": float(forecast["axis_ceiling"]),
        # The ceiling ONCE THE HERD IS AT ITS FLOOR — one turn's regrowth, on the same axis. Carried
        # beside the room so the delivered take can be quantised against either without a second
        # composition: whole-animal quantisation must run on BOTH readings or the holding rate would
        # be a smooth number beside a bodies-per-turn one.
        "hold_ceiling": float(forecast["axis_hold_ceiling"]),
        # …and the ceiling NEXT TURN'S take actually has — this turn's growth first, then the room
        # above the floor. The readout's headline is composed against it, for the reason
        # `expected_next_turn_yield` carries: a herd held at its floor has an EMPTY room and a real
        # take, and quoting the room reads `0.00` beside a work board quoting the regrowth.
        "next_ceiling": float(forecast["axis_next_ceiling"]),
        "per_animal": float(forecast["axis_per_animal"]),
        # **THE QUANTISER'S OWN THREE TERMS, IN BIOMASS.** A take of whole animals is
        # `min(room, crew carry, what stays) ÷ one body`, and every one of those is a BIOMASS — so
        # stating them in food is a conversion the quantiser does not need and that an inedible quarry
        # cannot make. `body_mass` is the quantum, `carry` the crew's throughput, and the two rooms the
        # numerators; `DrawerComposeController._hunt_delivered_and_waste` is their one consumer.
        "body_mass": body_quantum(herd, ""),
        "carry": per_worker_biomass(herd, ""),
        "hold_room": float(forecast["hold_room_biomass"]),
        "next_room": float(forecast["next_room_biomass"]),
        # **THE ENGAGEMENT PAIR, so the delivered take can bound itself on the party's REACH.** The
        # quantised take (`DrawerComposeController._hunt_delivered_and_waste`) composes its own
        # `collection` rather than calling `expected_yield_account`, so the third arm has to reach it
        # here or the sheet's headline stays carry-bound while the worker cap beside it is not. It
        # bounds the whole-animal COUNT with `animals_engaged`, which is why the pair travels raw and
        # undipped: that function applies the dip, and it is the ONE mirror of the sim's arithmetic.
        "engage_rate": float(forecast["engage_rate"]),
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
## A MANAGED source is described by its payoff instead: a built Pen's stock is not what it pays from,
## so its per-biomass vector is beside the point and may honestly be anything. **No plant rung takes
## that arm** — a Field is drawn down and answers this question through its own rate vector, exactly
## as a wild stand does.
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
## **NO BUILD TERM REACHES THESE RATES** (`docs/plan_standing_upkeep.md` §2.2). The during-build dip
## multiplied `per_worker*` here until the allocations split; the build has its own crew now, so a
## rung going up takes nothing off what the gatherers carry and this forecast answers the same
## numbers whether or not one is in flight. That is why it takes no `improvement`.
##
## `kind` is the caller-stated SOURCE_KIND_*; `prefix` only spells the wire keys (the two are
## independent — a herd and a raw wire patch share the empty prefix).
## **`per_crew` IS THE SIM'S TAKE CURVE** — carried through to the two projection-derived crews and
## back out on the dict, so `max_useful_workers` reads the same answer the chart's pills were drawn
## from. Empty everywhere there is no reply to hand (the plant web, the Work board's row gate), where
## the closed forms answer exactly as they did before the channel existed.
static func forecast_inputs(src: Dictionary, kind: String, prefix: String, floor: float,
        per_crew: Array = []) -> Dictionary:
    # ---- THE CEILING, PER ACCOUNT ---------------------------------------------------------------
    # A MANAGED source (a built Pen) is never drawn down: it pays its managed production at every
    # floor, so it reads the rung's own payoff fields instead of composing an escapement room out of a
    # stock the sim does not touch. **A FIELD IS NOT ONE** — every plant rung takes the composition
    # below, which is what gives a cash Field a material ceiling at all
    # (`FORECAST_MANAGED_FLAG_KEYS`).
    var ceiling := 0.0
    var ceiling_fodder := 0.0
    # …AND THE CEILING ONCE THE SOURCE IS SITTING AT THE FLOOR, which is a DIFFERENT quantity and the
    # one the readout's `after` reading is capped by. The ceilings above are the ROOM — everything
    # standing above the floor, takeable ONCE. What a source pays every turn thereafter is what it
    # REGROWS at that floor, which is why a big crew's headline take is a burst and not a rate.
    var hold_ceiling := 0.0
    var hold_ceiling_fodder := 0.0
    # …AND THE ROOM PER MATERIAL. It is a VECTOR rather than a third scalar, so it travels as
    # `[{material_id, amount}]` all the way to the readout and is never summed on the way. **The two
    # scalars' hold twins have no material sibling** — nothing states a per-material `after`, so the
    # one that was computed here went out on every forecast for no reader (`expected_materials`).
    var ceiling_material: Array[Dictionary] = []
    # …AND THE ROOM **NEXT TURN'S TAKE** ACTUALLY HAS — this turn's growth first, then what stands
    # above the floor. It is a THIRD kind of ceiling beside the two above and answers a third question:
    # the ROOM is what is standing there now (takeable once, which is what a preset's `up to +N/turn`
    # tooltip quotes), the HOLD is what the source pays once it is sitting at the floor, and this is
    # what the crew will BANK on the next resolved turn. Only the readout's headline reads it.
    var next_ceiling := 0.0
    var next_ceiling_fodder := 0.0
    var next_ceiling_material: Array[Dictionary] = []
    # …AND THE SAME TWO ROOMS IN **BIOMASS**, which is the unit a whole-animal take is quantised in.
    # A body is a biomass and a take is a count of bodies, so the quantiser divides a room by a
    # `body_mass` — and a room stated in FOOD can only answer that division for a species that pays
    # food, which is precisely the species an inedible quarry is not. `NO_ROOM_IN_BIOMASS` is the one
    # shape that has no such answer: a MANAGED source whose production is a payoff figure rather than
    # an escapement room, on a species with no per-biomass food rate to state that payoff through.
    var hold_room_biomass := NO_ROOM_IN_BIOMASS
    var next_room_biomass := NO_ROOM_IN_BIOMASS
    if source_is_managed(src, kind, prefix):
        var rung := String(FORECAST_MANAGED_IMPROVEMENTS[kind])
        ceiling = float(src.get(prefix + String(FORECAST_PAYOFF_KEYS[rung]), 0.0))
        if FORECAST_PAYOFF_FODDER_KEYS.has(rung):
            ceiling_fodder = float(src.get(prefix + String(FORECAST_PAYOFF_FODDER_KEYS[rung]), 0.0))
        # **A RUNG-3 MANAGED SOURCE HAS NO BURST TO SPEND.** The sim never draws a Field or a built Pen
        # down, so its payoff IS its every-turn rate: now and after are the same number, and the
        # readout renders one reading rather than an arrow pointing at itself.
        # **A MANAGED HERD'S MATERIAL PAYOFF IS ITS RUNG'S OWN VECTOR** — a built Pen or a tamed herd
        # pays `corral_material` / `pastoral_material` every turn, which is the whole of what a
        # prepared inedible quarry is worth. A managed PATCH has no entry in the table: a plant's
        # materials are per PLANT, stated row by row on the crop picker, and a tile-level figure would
        # sum across the basket.
        if FORECAST_PAYOFF_MATERIAL_KEYS.has(rung):
            ceiling_material = material_payoff_rows(
                src.get(prefix + String(FORECAST_PAYOFF_MATERIAL_KEYS[rung]), []))
        hold_ceiling = ceiling
        hold_ceiling_fodder = ceiling_fodder
        # A managed source pays its production every turn, so *next turn* and *the room* are the same
        # number and the readout renders one reading rather than an arrow pointing at itself.
        next_ceiling = ceiling
        next_ceiling_fodder = ceiling_fodder
        next_ceiling_material = ceiling_material
        # A pen's production has no escapement room behind it, so the only biomass expression of it is
        # the payoff read back through the species' own per-biomass rate. A species that pays no food
        # states none, and the quantiser declines rather than inventing one.
        var managed_rate := float(src.get(prefix + FORECAST_PROVISIONS_PER_BIOMASS_KEY, 0.0))
        if managed_rate > 0.0:
            hold_room_biomass = ceiling / managed_rate
            next_room_biomass = hold_room_biomass
    else:
        # ONE composition, both webs — the terms it reads are published identically by
        # `HerdTelemetryState` and `ForagePatchState`, which is what collapsed two branches into none.
        #
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
        # …and the FORWARD room through the SAME per-biomass vector the other two go through, so the
        # headline's accounts stay in one ratio: a forward food figure beside an instantaneous
        # material one would be two different turns stated on one row.
        var next_room := escapement_room_next_turn(src, prefix, floor)
        next_ceiling = next_room * float(src.get(prefix + FORECAST_PROVISIONS_PER_BIOMASS_KEY, 0.0))
        next_ceiling_fodder = next_room \
            * float(src.get(prefix + FORECAST_FODDER_PER_BIOMASS_KEY, 0.0))
        next_ceiling_material = scaled_material_rows(
            src.get(prefix + FORECAST_MATERIAL_PER_BIOMASS_KEY, []), next_room)
        # The same two rooms, unconverted — the quantiser's own denominator-free readings.
        hold_room_biomass = growth
        next_room_biomass = next_room
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
    # **BOTH WEBS PUBLISH IT**, and the plant one arrived second: a cash-crop patch's gather banks
    # fibre and tobacco, and until this reached the wire the forage compose sheet quoted only the food
    # and the feed. It reads through the SAME prefixed key as the herd's, which is what let one
    # composition serve both — see `FORECAST_MATERIAL_PER_BIOMASS_KEY`.
    #
    # **THE PATCH'S TERM HAS THE SEASONAL WEIGHT ALREADY FOLDED IN**, exactly as `per_worker_biomass`
    # does, so it is honestly EMPTY in a dead season and must NOT be divided by anything here. A
    # client that re-applied the weight would double it; one that recovered a rate by dividing by it
    # would divide by zero on the very tile the emptiness is describing.
    var per_worker_material := material_payoff_rows(
        src.get(prefix + FORECAST_PER_WORKER_MATERIAL_KEY, []))
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
        #
        # **THERE IS NO `hold_material_ceiling` BESIDE IT, and the two scalars' hold twins below are
        # not an argument for one.** It was published for a release and read by nobody: the only
        # consumer's hold arm was unreachable, because a per-material `after` reading is not something
        # any surface states (`expected_materials` carries the long form). A ceiling nothing clamps
        # against is a wire-shaped invitation to clamp against it wrongly.
        FORECAST_PER_WORKER_MATERIAL_KEY: per_worker_material,
        "material_ceiling": ceiling_material,
        # The HOLD ceilings, keyed to match their room twins so `expected_yield_account` reaches either
        # by name and no second take function exists to drift from the first.
        "hold_ceiling": hold_ceiling,
        "hold_ceiling_fodder": hold_ceiling_fodder,
        # …and the NEXT-TURN ones, keyed the same way for the same reason. **The readout's headline is
        # their ONE consumer**: the presets quote the ROOM (a quantity takeable once) and the worker
        # cap divides the ROOM (how many hands the standing stock can use), and neither of those
        # questions is *what lands next turn*.
        "next_ceiling": next_ceiling,
        "next_ceiling_fodder": next_ceiling_fodder,
        "next_material_ceiling": next_ceiling_material,
        # …and the two of them in BIOMASS, for the whole-animal quantiser alone. See the declarations
        # above for why the pair exists and what `NO_ROOM_IN_BIOMASS` means.
        "hold_room_biomass": hold_room_biomass,
        "next_room_biomass": next_room_biomass,
        # The QUANTISED-AXIS triple every divide-by-a-quantum consumer reads (`max_useful_workers` and
        # the local preview). **The axis is provisions and is no longer a choice** (arc #527): the
        # trade account it could otherwise resolve to is retired, so these are aliases of the food
        # terms above rather than a resolution — kept under their own names because they mark WHICH
        # question a consumer is asking, and an inedible quarry's zeros here are a real answer.
        "axis_per_worker": per_worker,
        "axis_ceiling": ceiling,
        "axis_hold_ceiling": hold_ceiling,
        "axis_next_ceiling": next_ceiling,
        "axis_per_animal": food_per_animal,
        "whole_animal": whole_animal,
        # The floor this forecast was composed at, carried so a caller can re-state it without holding
        # the dial itself — and so a cached forecast can never be read against a different floor.
        "floor": clamp_floor(floor),
        # WHICH ACCOUNT'S ZERO IS A FACT ABOUT THIS SOURCE (§7.7). It is read off the per-biomass rate
        # vector, not off this turn's ceilings: a herd stripped to its floor pays nothing in ANY
        # account, and the question "which account would it pay?" still has an answer.
        "zero_account": zero_account_of(src, prefix),
        # **THE CREW'S THROUGHPUT IN BIOMASS** — the term the two worker targets divide by.
        "per_worker_biomass": per_worker_biomass(src, prefix),
        # **THE ENGAGEMENT THROUGHPUT, RAW** (`docs/plan_hunt_through_combat.md` §2) — carried
        # undipped beside the `dip` above for the reason `per_worker_biomass` is: the two consumers
        # (`expected_yield_account`'s reach arm and `max_useful_workers`' engagement crew) apply the
        # dip themselves, through `animals_engaged` / `engage_workers`, which are the client's ONE
        # mirror of the sim's arithmetic. A forage patch publishes no such field, so it reads
        # `NO_ENGAGEMENT_STAGE` and both consumers drop the term.
        "engage_rate": float(src.get(prefix + FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE)),
        # **THE FENCE, CARRIED BESIDE THE REACH BECAUSE THE TWO ARE ONE QUESTION** — `quarry_is_fought`
        # needs both terms, and a downstream reader holding only a composed forecast (the Work board's
        # `with_published_useful_crew`, whose other argument is a LABOR ROW and carries no herd fields)
        # could otherwise reach neither. Copied raw, exactly as `engage_rate` is, so the verdict stays
        # in the one function that owns it. `false` on every plant source, which never publishes it.
        SOURCE_CORRALLED_KEY: bool(src.get(prefix + SOURCE_CORRALLED_KEY, false)),
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
        "hold_crew": hold_crew(src, kind, prefix, floor, per_crew),
        # **AND THE CREW THAT REACHES THE FLOOR**, the cap's second projection-derived floor. See
        # `reach_crew`: the *clear it now* target is floored on it, and a target the stepper cannot
        # reach is the panel arguing with itself.
        "reach_crew": reach_crew(src, kind, prefix, floor, per_crew),
        # **THE CURVE ITSELF, so the worker cap is a search of the same rows the pills were.** It rides
        # the forecast rather than reaching `max_useful_workers` as a second argument because the cap
        # has TWO twins (`source_worker_cap_state` and `DrawerComposeController._forecast_worker_cap`)
        # and they must keep dividing through one function — a parameter only one of them knew to pass
        # is how they would come apart.
        "per_crew": per_crew,
        # **A PRESENCE test, not a rate test** (#426). It used to be `per_worker >= ε`, which conflated
        # "the wire carried no forecast" with "the rate is genuinely zero" — and its own docstring said
        # it meant the former. A zero-conversion crop makes the latter real, so the two came apart and
        # the compose sheet answered by going silent on the one state it most needed to report.
        "known": forecast_is_known(src, kind, prefix),
    }

## **THE WHOLE DEAL AN IMPROVEMENT OFFERS, composed in ONE place** (issue #442) — the take the crew
## holds today, and the payoff the finished rung pays:
##
##     +0.96  ->  +1.20 /turn
##      today        payoff
##
## **THE MIDDLE TERM IS RETIRED WITH THE DIP** (`docs/plan_standing_upkeep.md` §2.2). It was the
## *preparing* take — what the crew accepted while it built — and it existed because one crew did
## both jobs. A build has its own crew now, so the gatherers' take is the same number before, during
## and after the build, and a second reading of it beside the first would state one fact twice.
## `base_forecast` is therefore the only forecast carried, priced by the CALLER through
## `expected_yield_account`.
##
## Returns `{}` when `improvement` is `IMPROVEMENT_NONE` or the source carries no forecast, so a caller
## renders no deal rather than a deal made of zeros. `floor` is the crew's escapement floor; the two
## axes are independent, and a floor below the food peak beside a running build is LEGAL (it defeats
## itself through the ecology — the meter accrues only while the source is Thriving).
##
## **THERE IS NO `feed` TERM** — the pen's food-unit upkeep is retired (see `FORECAST_FEED_KEYS`
## above), so the deal a rung quotes is its payoff and nothing subtracted from it, on BOTH webs.
static func improvement_forecast(src: Dictionary, kind: String, prefix: String, floor: float,
        improvement: String) -> Dictionary:
    if improvement == IMPROVEMENT_NONE or not FORECAST_PAYOFF_KEYS.has(improvement):
        return {}
    var base_forecast := forecast_inputs(src, kind, prefix, floor)
    if not bool(base_forecast["known"]):
        return {}
    # **A RUNG THE WIRE PRICES NO JOB FOR ON THIS SOURCE IS UNQUOTABLE** — a species that can never
    # be penned, a rung this source has already climbed past. The build's own cost is what says so
    # now (it is published whether or not a build is in flight), where the retired dip fraction used
    # to; a deal composed anyway would advertise a payoff for a rung that cannot be built here.
    if build_work_cost(src, prefix, improvement) <= BUILD_WORK_COST_NONE:
        return {}
    var payoff := float(src.get(prefix + String(FORECAST_PAYOFF_KEYS[improvement]), 0.0))
    # The payoff's non-food component — a VECTOR like every other yield in this model. Fodder is
    # plant-only (no animal pays it — a structural zero, not a gap), so a herd rung has no twin in the
    # table, resolves to 0.0 and renders as nothing, which is the rule. (The trade twin that reached
    # all four rungs went with the trade axis, arc #527.)
    var payoff_fodder := 0.0
    if FORECAST_PAYOFF_FODDER_KEYS.has(improvement):
        payoff_fodder = float(src.get(prefix + String(FORECAST_PAYOFF_FODDER_KEYS[improvement]), 0.0))
    # …and the MATERIAL half of the payoff, a VECTOR. Herd rungs only (see
    # `FORECAST_PAYOFF_MATERIAL_KEYS`), so a plant rung resolves empty and its deal row is unchanged.
    var payoff_material: Array[Dictionary] = []
    if FORECAST_PAYOFF_MATERIAL_KEYS.has(improvement):
        payoff_material = material_payoff_rows(
            src.get(prefix + String(FORECAST_PAYOFF_MATERIAL_KEYS[improvement]), []))
    return {
        "improvement": improvement,
        "floor": clamp_floor(floor),
        # The crew's forecast — ONE, priced per account through the same `expected_yield_account` the
        # committed row will be.
        "base_forecast": base_forecast,
        # The un-crewed reference the picker faces quote: what the SOURCE offers at this floor.
        "ceiling": float(base_forecast["ceiling"]),
        "ceiling_fodder": float(base_forecast["ceiling_fodder"]),
        "payoff": payoff,
        "payoff_fodder": payoff_fodder,
        "payoff_material": payoff_material,
        # Which account's zero is worth printing on any of the three terms (§7.7).
        "zero_account": String(base_forecast["zero_account"]),
    }

## **THE VERB THIS SOURCE IS BUILDING, DERIVED FROM ITS METERS** — the client's transcription of
## `forage::patch_build_verb` / `fauna::herd_build_verb` (`docs/plan_standing_upkeep.md` §2.4), and
## the ONE answer to *"is a build in flight here, and which rung"*.
##
## | meter | state | who declares |
## |---|---|---|
## | **zero** | nothing in flight | **the player** — a wild patch could climb to tended *or* be sown |
## | **between zero and its cost** | building that rung, **implied** | nobody — the progress banked on it IS the answer |
## | **at its cost** | maintaining | nobody |
##
## **`declared` IS ONLY A PENDING DECLARATION, honoured solely at a zero meter.** It is the
## assignment's `improvement` (or a box the player just ticked), and it stopped being the authority
## when the sim stopped storing one: a rung that has banked work needs no restatement, and a spent
## declaration is inert here rather than having to be cleaned up somewhere.
##
## **NEWEST METER FIRST**, exactly as the sim resolves it, so a Field with progress on it governs the
## tended ground beneath — a `Cultivate` declared on a Field is DEAD rather than stalled.
##
## **WHAT IT FIXED.** Completion used to free the stored verb, so a completed rung that eroded back
## below its cost re-entered the *building* state with nothing set and could not be repaired until the
## player re-issued the command. They never withdrew that intent: **a player who has paid for a rung
## and watched it slip adds HANDS, not a re-declaration.** It also retired `abandon_improvement`,
## which existed to clear a stored verb; taking the hands off is what the grammar offers now, and it
## leaves the declaration standing (`labor-ui.md` → "RETIRED — `abandon_improvement`").
##
## ⛔ **RETIRED CLAIM — *"the meter's fullness and the rung's achievement are two facts and must stay
## orthogonal: a patch at 99% is building AND still tended"*.** They are ONE fact now. The sim
## publishes each per-rung meter as a publication of the standing verdict, so a rung the source holds
## reads exactly full and a rung short of its cost is a rung the source does not hold — which is why
## `RungGates.rung_has_room` is a bare `not improvement_is_done` and the repair test that stood on the
## disagreement is retired (see its epitaph, beside `improvement_progress`).
##
## **THE READING HERE IS UNCHANGED AND STILL RIGHT**: a meter strictly between zero and its cost is a
## rung being RAISED, which is exactly the rung the standing has not reached. What is gone is the
## claim that this could contradict `improvement_is_done` on the same rung.
##
## `kind` is a SOURCE kind (`SOURCE_KIND_*`); a caller holding a labor kind converts through
## `source_kind_for_labor`.
static func build_verb(src: Dictionary, prefix: String, kind: String,
        declared: String = IMPROVEMENT_NONE) -> String:
    var ladder := _improvement_ladder(kind)
    var wanted := declared.strip_edges().to_lower()
    for i in range(ladder.size() - 1, -1, -1):
        var rung := String(ladder[i])
        var progress := improvement_progress(src, prefix, rung)
        if progress > BUILD_METER_UNSTARTED:
            # A meter carrying work answers for itself, in both directions: still climbing means this
            # rung is in flight, full means the source has moved on to maintaining it.
            return IMPROVEMENT_NONE if progress >= BUILD_METER_FULL else rung
        if wanted == rung:
            # The meter is at zero, which is the one state the player's declaration answers for.
            return rung
    return IMPROVEMENT_NONE

## **WHICH RUNG'S METER IS THE ONE AT RISK — the newest one carrying any work at all**, full or not.
## The client's transcription of `forage::patch_unwinding_rung` (and its animal twin), and the rung the
## published `upkeepDemand` / `upkeepShortfall` / `meterRotPerTurn` are resolved THROUGH.
##
## **ONLY ONE METER ON A SOURCE IS EVER AT RISK, so only one row may carry the mark**
## (`docs/plan_standing_upkeep.md` §2.4). A patch mid-Sow is billed for the FIELD; the tended rung
## underneath it is not being billed at all, so a `⚠` on the tended row points the player at ground
## that is fine — and the absence of a mark is the only thing that says a rung is healthy, so a false
## one costs every other row its meaning.
##
## **THIS DECIDES WHICH ROW DISPLAYS A NUMBER, NEVER THE NUMBER.** The shortfall is the sim's and is
## untouched; what the client resolves is where to put it, which is the same job `build_verb` already
## does for the build verb, off the same table and the same newest-first walk.
##
## **IT IS NOT `build_verb`, AND THE DIFFERENCE IS A FULL METER.** That one answers *what is being
## BUILT*, so it returns `IMPROVEMENT_NONE` at a meter standing at its cost — the source has moved on
## to maintaining it. Maintaining it is precisely when it is at risk, so this returns the rung.
##
## `IMPROVEMENT_NONE` where no meter carries anything: a wild source, which is billed nothing.
static func at_risk_rung(src: Dictionary, prefix: String, kind: String) -> String:
    var ladder := _improvement_ladder(kind)
    for i in range(ladder.size() - 1, -1, -1):
        var rung := String(ladder[i])
        if improvement_progress(src, prefix, rung) > BUILD_METER_UNSTARTED:
            return rung
    return IMPROVEMENT_NONE

## **IS *THIS* RUNG THE ONE THE SHORTFALL IS ABOUT?** `is_under_kept` answers for the SOURCE — one
## pool, one shortfall, one at-risk meter — and this is what routes that answer to a row. A rung that
## is not the at-risk one is not being billed, so it renders its ordinary face.
static func rung_is_under_kept(src: Dictionary, prefix: String, kind: String,
        improvement: String) -> bool:
    return at_risk_rung(src, prefix, kind) == improvement and is_under_kept(src, prefix)

## **IS THIS RUNG PROMISED AND UNMANNED — AND WHICH OF THE TWO WAYS?** `BUILD_STAFFED` when somebody
## is on it (or nothing is declared at all), else one of the two unstaffed states.
##
## **THE SILENCE THIS EXISTS TO BREAK.** The sim publishes `buildTurnsRemaining = -1` for an unstaffed
## source and that is CORRECT — nobody has promised anything there, so there is genuinely no estimate
## — and every meter surface renders `-1` as no line at all. So a declared build with no builders read
## as *fine*: a sheet quoting `Cultivating 0 / 50 work (0%)`, a `0%` rung badge on the map, and not one
## word anywhere saying nothing was happening. A declared-but-unstaffed build is an **actionable
## standing fact**, not an absence of information, exactly as the `∞` one state over is
## (`BUILD_TURNS_HOLDS` / `BUILD_TURNS_ROTS`), and the client DERIVES it rather than
## asking for a wire field because the declaration, the crew and the meter are all already published.
##
## **IT ANSWERS ONE STATE NOW, AND `BUILD_UNSTAFFED_SLIDING` IS WHY IT USED TO ANSWER TWO**
## (`docs/plan_standing_upkeep.md` §4.6a):
##
## | crew | meter | state | what it means |
## |---|---|---|---|
## | 0 | 0 | `BUILD_UNSTAFFED_UNSTARTED` | not started — nobody assigned, nothing banked |
## | 0 | >0 | (not this function's) the wire's `BUILD_TURNS_HOLDS` / `BUILD_TURNS_ROTS` | held on purpose, or losing ground |
## | >0 | any | `BUILD_STAFFED` | somebody is on it; the estimate speaks for the pace |
##
## **THE MIDDLE ROW LEFT THIS FUNCTION, because it was an INFERENCE and the sim now answers it.**
## `BUILD_UNSTAFFED_SLIDING` read *work banked + nobody on it ⇒ the meter is bleeding*, which was true
## while an unbuilt rung was billed to its build crew. **The keeping pool holds a meter at any
## fullness now**, so a parked build whose keeping is met simply HOLDS — a legitimate thing to do —
## and one whose keeping is short LOSES. The sim publishes exactly that fork (`-2` / `-3`) for zero
## builders, and `build_pace` classifies it. A client-side rot test here would be a second opinion
## about a number the sim owns, and the two would drift.
##
## `declared` is the assignment's own `improvement`, which `build_verb` honours only where the meter it
## names is at zero; everything else the meters answer for themselves, so a rung the player has just
## ticked is picked up as `UNSTARTED` and a meter with work on it is left to the wire's own verdict.
## `kind` is a SOURCE kind, like `build_verb`'s.
static func unstaffed_build_state(src: Dictionary, prefix: String, kind: String,
        declared: String, build_workers: int) -> String:
    var verb := build_verb(src, prefix, kind, declared)
    if verb == IMPROVEMENT_NONE:
        return BUILD_STAFFED
    return unstaffed_build_of(improvement_progress(src, prefix, verb), build_workers)

## **THE SAME FORK ASKED OF AN ALREADY-RESOLVED RUNG** — the caller holds the verb's meter and knows
## a build is in flight, so there is nothing left to derive. It exists so the MAP's badge, which has
## just resolved both through `RungGates.rung_in_progress`, can ask without resolving the verb a
## second time and without a renderer reaching for a HUD vocabulary module to spell the key prefix.
## `unstaffed_build_state` is written in terms of it, so there is one fork and not two.
static func unstaffed_build_of(progress: float, build_workers: int) -> String:
    if build_workers > BUILD_CREW_NONE or progress > BUILD_METER_UNSTARTED:
        return BUILD_STAFFED
    return BUILD_UNSTAFFED_UNSTARTED

## **IS THIS SOURCE'S METER GOING BACKWARDS? — the wire's verdict, asked of a BARE source dict.**
## The map's badge companion to `unstaffed_build_of`, and it exists for the same reason: a renderer
## that has already resolved the rung should not have to reach for a HUD vocabulary module to spell a
## key prefix. `build_pace` is the one classifier, so the map cannot form an opinion the tile card and
## the compose sheet do not share.
##
## **THE CREW IS PART OF THE QUESTION** — `BUILD_METER_HOLDS` is a crew treading water when there is a
## crew and a build parked on purpose when there is not, and only the first is news. Both answer
## `false` here regardless; the crew is passed so that this cannot start meaning *not climbing*.
static func build_is_losing(src: Dictionary, build_workers: int) -> bool:
    return build_pace(build_turns_remaining(src, BARE_SOURCE_PREFIX), build_workers) \
        == BUILD_PACE_LOSING

## **IS THIS BUILD STALLED? — THE ONE VERDICT EVERY SURFACE THAT MARKS A BUILD WEARS A `⚠` FOR.**
## `true` when the rung is declared-and-never-started with nobody on it, OR when the wire says its
## meter is going backwards; `false` for a climbing build AND for one merely PARKED with its keeping
## covered, whose number is honest and whose state is a decision rather than a failure.
##
## **IT EXISTS BECAUSE THE MAP AND THE WORK BOARD ANSWERED DIFFERENTLY.** The map's source badge forked
## on this pair and dropped the percentage; `BandPanelController._build_work_row` had no such fork and
## printed a confident `▦45%` whatever the staffing — one screen, two verdicts, and the wrong one was
## the one with the number on it. Both surfaces call THIS now: two producers of one verdict is exactly
## how they came apart, and the fix is one producer rather than two careful copies.
##
## `src` is the BARE source dict (the raw wire patch or herd) and `progress` is the meter the caller
## has already resolved through `RungGates.rung_in_progress` — asked, not re-derived, so the plate's
## warning and its glyph provably describe the same verb. `build_workers` is the BAND's `builders`
## POOL, not a per-source crew (§2.5): a verb declares and names no hands.
static func build_is_stalled(src: Dictionary, progress: float, build_workers: int) -> bool:
    return build_is_losing(src, build_workers) \
        or build_is_unstaffed(unstaffed_build_of(progress, build_workers))

## A source dict whose forecast keys are UNPREFIXED — a herd, or the raw wire forage patch. The same
## empty string `HudComposeVocab.BARE_FORECAST_PREFIX` spells for the HUD; it is restated here so the
## readers above can be reached from the map layer, which holds no HUD vocabulary.
const BARE_SOURCE_PREFIX := ""

## Nothing for this fork to warn about: builders on it, work already banked (the wire's own verdict
## covers that), or no rung in flight at all.
const BUILD_STAFFED := ""
## Declared, nobody on it, and the meter has never moved — *not started*. **The one state left here**,
## and it stays because nothing else says it: the sim answers `-1` for it, correctly (there is no crew
## to estimate for), and an honest absence of information reads as *fine* on a rung the player just
## ticked and cannot staff.
const BUILD_UNSTAFFED_UNSTARTED := "unstarted"

## RETIRED — **`BUILD_UNSTAFFED_SLIDING`**, *work banked and nobody on it, so the meter is bleeding*
## (`docs/plan_standing_upkeep.md` §4.6a). It was an INFERENCE from the staffing, and it is wrong under
## the pooled keeping: a parked build whose keeping is met holds exactly where it is, indefinitely and
## on purpose. The sim publishes the real fork for zero builders — `BUILD_TURNS_HOLDS` when the keeping
## covers it, `BUILD_TURNS_ROTS` when it does not — and `build_pace` is the ONE classifier of it.

## Is this state the one the surfaces warn on? Named rather than compared inline, so a caller cannot
## accidentally read `BUILD_STAFFED`'s emptiness as the wrong answer.
static func build_is_unstaffed(state: String) -> bool:
    return state == BUILD_UNSTAFFED_UNSTARTED

## **WHICH WAY THE METER IS MOVING — the four states a build line's INK says**
## (`docs/plan_standing_upkeep.md` §2.4). A build crew's whole output is progress and an under-kept
## meter's ROT is what eats it, so the pace is that difference and it has a sign — plus one state that
## is not a pace at all but a player's decision:
##
## | net | crew | pace | the face reads |
## |---|---|---|---|
## | **> 0** | any | `BUILD_PACE_GROWING` | a real turn count |
## | **= 0** | **none** | `BUILD_PACE_HELD` | held exactly where it was parked — **neutral, no mark** |
## | **= 0** | on it | `BUILD_PACE_HOLDING` | `∞` — a crew treading water |
## | **< 0** | any | `BUILD_PACE_LOSING` | `∞` — it is going back the way it came |
##
## **THE CLIENT DOES NOT DERIVE THE SIGN, and that is the whole discipline of this function.** It
## classifies the sentinels the estimate already answered with; comparing a composed `work_per_turn`
## against zero would be a second opinion about a number the sim owns, which is the drift that once
## quoted `≈50 turns` for a build that never moved. **The crew is not a second opinion** — it is the
## one fact the wire's single `BUILD_METER_HOLDS` cannot carry, and it decides only whether the reader
## is being told about a crew or about a parking decision.
##
## **PARKING A HALF-BUILT IMPROVEMENT IS A LEGITIMATE THING TO DO** (§2.4). Take the builders off a
## Cultivate at 50%, keep the keeping staffed, and the meter holds there indefinitely — so it must not
## wear the hazard treatment. Marking a deliberate hold is how a player is taught to ignore the mark,
## which costs every OTHER state its meaning (`selection-card.md` → "THE ABSENCE OF A HAZARD IS THE
## ONLY SIGNAL THAT THINGS ARE FINE").
##
## **EACH ∞ SENTINEL HAS ITS OWN PACE, because the wire publishes two of them.**
## **A `-5` IS NOT A PACE AT ALL**, and it is answered here rather than left to the fall-through: the
## sim has run no estimate pass over the entry, so there is no direction to classify. See the arm
## itself for what falling through would have painted.
##
## `sim_schema::BUILD_METER_HOLDS` is the meter standing still and `BUILD_METER_ROTS` the meter losing
## ground — the red the schema promises for work already paid for and now bleeding. The amber used to
## cover both, which told a player whose build was being destroyed the same thing it tells one merely
## treading water.
static func build_pace(turns: int, build_workers: int = BUILD_CREW_ANY) -> String:
    if turns == BUILD_TURNS_QUEUE_BLOCKED:
        return BUILD_PACE_BLOCKED
    if turns == BUILD_TURNS_ROTS:
        return BUILD_PACE_LOSING
    if turns == BUILD_TURNS_HOLDS:
        return BUILD_PACE_HOLDING if build_workers > BUILD_CREW_NONE else BUILD_PACE_HELD
    # ⛔ **A BUILD THE SIM HAS NOT LOOKED AT HAS NO PACE, AND MUST NOT FALL THROUGH TO ONE.** Without
    # this arm `-5` lands on `BUILD_PACE_GROWING` — the arm that means *the meter climbs and the face
    # quotes a real turn count* — which paints a compose face HEALTHY green off a number that does not
    # exist (`HudWidgets.improvement_pace_color`). Nor is it `HOLDING` or `LOSING`: those are verdicts
    # about a crew's arithmetic, and no arithmetic has been done here. `UNKNOWN` is the honest answer
    # and the neutral render, which is exactly what this state wants.
    if turns == BUILD_TURNS_NOT_YET_ESTIMATED:
        return BUILD_PACE_UNKNOWN
    if turns == BUILD_TURNS_NO_ESTIMATE:
        return BUILD_PACE_UNKNOWN
    return BUILD_PACE_GROWING

## **THE CALLER THAT IS NOT TALKING ABOUT A PARTICULAR CREW** — a producer classifying a sentinel with
## no staffing in hand. It reads as *somebody is on it*, which is the conservative arm: it keeps the
## `∞` states wearing their warning rather than silently promoting one to a deliberate hold.
const BUILD_CREW_ANY := 1

## No answer about the meter's direction — a rung the wire prices nothing on, a gated or stalled
## build. It renders in the neutral live ink, never as a verdict.
const BUILD_PACE_UNKNOWN := ""
## The surplus is positive: the meter climbs and the face quotes a real turn count.
const BUILD_PACE_GROWING := "growing"
## A crew is on it and banks exactly what the meter loses: it holds and no finish date is ever
## reached. **A wasted turn, so it keeps the warning treatment.**
const BUILD_PACE_HOLDING := "holding"
## **NOBODY IS ON IT AND NOTHING IS BEING LOST** — the meter is parked where the player left it, and
## the band's keeping is covering it. Not a failure, so it takes the neutral ink and no hazard mark;
## the only thing that distinguishes it from `HOLDING` is that there is no crew to be wasting a turn.
const BUILD_PACE_HELD := "held"
## The rate is going unpaid: the meter is losing ground.
const BUILD_PACE_LOSING := "losing"
## **THE QUEUE IS STUCK ON THIS ENTRY** — the band's builders are staffed and standing here, its own
## gate refuses it, and nothing behind it moves either (`BUILD_TURNS_QUEUE_BLOCKED`). A FOURTH arm,
## because blocked ≠ holding ≠ rotting ≠ silent: it is a hazard like the two `∞` states, and unlike
## either of them no number of builders fixes it — the remedy is off the build line entirely.
const BUILD_PACE_BLOCKED := "blocked"

## Is a build in flight on this source at all? The bare question three warnings and the keeping row
## ask, named so none of them spells the `!= IMPROVEMENT_NONE` for itself.
static func build_is_in_flight(src: Dictionary, prefix: String, kind: String,
        declared: String = IMPROVEMENT_NONE) -> bool:
    return build_verb(src, prefix, kind, declared) != IMPROVEMENT_NONE

## Is this improvement's rung ALREADY BUILT on this source? The test that turns the improvement
## control's Running state into its Done state, and the one definition of it.
##
## **IT IS ONE COMPARISON, ON ONE WIRE FIELD: is the rung the source STANDS on at or above the rung
## this verb builds?** `current_rung` is branch-qualified (`plant:field`, `animal:pen`), so the same
## read serves both webs and a third one costs a row in two tables and nothing here. `prefix` spells
## the key, so a `patch_`-prefixed `tile_info` cross-ref and a bare wire row both work.
##
## ⛔ **IT USED TO READ EACH WEB'S PRIVATE FLAGS** (`is_cultivated` / `is_field` / `corralled`, with
## `Tame` a special case against `DOMESTICATION_COMPLETE`) and needed a fourth table to say a Field is
## also cultivated. The sim publishes the position those flags were being reassembled into — the two
## are provably one fact, `forage::patch_rung_key(patch)` being `patch.standing().held` and
## `is_cultivated()` being `held.is_at_or_above(PlantTended)` — so the reassembly is gone, along with
## the way it silently forgot a rung the flags could not see.
##
## **A VERB NAMING NO RUNG ANSWERS `false`**, `IMPROVEMENT_NONE` included: nobody builds the floor.
static func improvement_is_done(src: Dictionary, prefix: String, improvement: String) -> bool:
    if not IMPROVEMENT_RUNG_KEYS.has(improvement):
        return false
    return rung_at_or_above(String(src.get(prefix + FORECAST_CURRENT_RUNG_KEY, "")),
        String(IMPROVEMENT_RUNG_KEYS[improvement]))

## **WHICH RUNG DOES THIS SOURCE STAND ON, AS AN IMPROVEMENT KEY** — `IMPROVEMENT_RUNG_KEYS` read
## backwards against the ONE wire field that states the position (`current_rung`, `<branch>:<id>`).
## `IMPROVEMENT_NONE` on a source standing on its branch's floor, which is every wild patch and every
## wild herd; nobody built the floor.
##
## **IT IS AN EXACT MATCH, NEVER AT-OR-ABOVE, which is what makes it different from
## `improvement_is_done`.** That test asks *has this verb's rung been reached* and answers `true` for
## Cultivate on a Field; this asks *what is the source STANDING on* and there is exactly one answer.
## Neither can stand in for the other.
##
## **ONE FORK, SO A ROW'S MARK AND ITS FACE CANNOT NAME TWO DIFFERENT RUNGS**
## (`docs/plan_standing_upkeep.md` §4.9 item 12c). The work board's rung-mark resolver and the work
## inspector's head line both ask this. Before it, the mark reassembled the position out of each web's
## private flags (`is_field` / `is_cultivated` / `corralled` / a `domestication` threshold) — the very
## reassembly `improvement_is_done`'s own ⛔ records the sim publishing outright.
##
## `prefix` spells the key, so a `patch_`-prefixed `tile_info` cross-ref and a bare wire row both work.
static func standing_improvement(src: Dictionary, prefix: String) -> String:
    var standing := String(src.get(prefix + FORECAST_CURRENT_RUNG_KEY, ""))
    if standing == "":
        return IMPROVEMENT_NONE
    for improvement in IMPROVEMENT_RUNG_KEYS:
        if String(IMPROVEMENT_RUNG_KEYS[improvement]) == standing:
            return String(improvement)
    return IMPROVEMENT_NONE

## ⛔ **RETIRED — `rung_needs_repair`.** It asked *"has this source ACHIEVED this rung and then let it
## SLIP?"* — the rung's own stamped flag true beside a meter short of its cost — and it was the client
## half of the 99% repair: the sim's `cultivate` / `sow` / `corral` locks refuse on the METER rather
## than on the achieved rung, so an eroded-but-achieved rung was legal to re-queue while the offer test
## filtered it out as *built*, and the row said DONE and BUILDING at once while offering nothing.
##
## **THE STATE IT NAMED IS NOW UNREPRESENTABLE, WHICH IS WHY THE TEST IS GONE RATHER THAN FIXED.**
## The sim publishes each per-rung meter as a PUBLICATION of the standing verdict instead of a second
## reading of it (`intensification::rung_work_done`): a rung the standing HOLDS reads its full width,
## full stop, and the wire's fraction divides that width by the very cost it came from. Every flag in
## `FORECAST_DONE_FLAG_KEYS` implies that holding — `is_cultivated()` is
## `held.is_at_or_above(PlantTended)`, `is_field()` is `held == PlantField`, and `corral_at` seats the
## position at the top of `animal:pen`, which nothing on the animal web ever lowers. So *achieved*
## implies *meter exactly full*, by construction. Plant decay re-derives the standing on the same call
## that moves the position, so a Field slipping does not sit under `plant:field` wearing its flag; and
## `Tame` never had a row in that table to fall through.
##
## ⛔ **ITS ONLY LIVE TRIGGER WAS A FLOAT ARTIFACT, WHICH IS THE CLASS THIS DELETION CLOSES.** The
## meter used to be published as `(position − base) / width` against a completion test of
## `position >= base + width`, and in `f32` those disagree by one ULP for most Field prices: a
## finished Field published `0.99999994`, this test read it as *achieved and short*, and the work row
## drew a `⌃` offering to build the Field it was already standing on — over a destination track with
## no rung above the standing one, so the press did nothing at all. **The offer test and the track
## test ask ONE question now** (`RungGates.rung_has_room`, which is `not improvement_is_done`), so a
## `⌃` cannot be drawn for a track with no rows however the two readings drift.

## How far along this improvement's build meter is, 0..1. Clamped, so a wire value that overshoots
## cannot render a >100% meter. See `FORECAST_BUILD_METER_KEYS` for which meter each verb fills.
static func improvement_progress(src: Dictionary, prefix: String, improvement: String) -> float:
    if not FORECAST_BUILD_METER_KEYS.has(improvement):
        return 0.0
    return clampf(float(src.get(
        prefix + String(FORECAST_BUILD_METER_KEYS[improvement]), 0.0)), 0.0, 1.0)

## **HOW MUCH WORK THIS IMPROVEMENT HAS ABSORBED SO FAR**, in work units — the numerator of the
## percentage `improvement_progress` answers, read rather than reconstructed from it.
static func build_work_done(src: Dictionary, prefix: String, improvement: String) -> float:
    if not FORECAST_BUILD_WORK_DONE_KEYS.has(improvement):
        return 0.0
    return maxf(float(src.get(
        prefix + String(FORECAST_BUILD_WORK_DONE_KEYS[improvement]), 0.0)), 0.0)

## **WHAT THIS IMPROVEMENT COSTS ON THIS SOURCE**, in work units — a Tame carrying the species' own
## cost multiplier, a Corral not (a fence is a fence). `BUILD_WORK_COST_NONE` where the wire prices
## no such job here, which every readout treats as "state the percentage alone".
static func build_work_cost(src: Dictionary, prefix: String, improvement: String) -> float:
    if not FORECAST_BUILD_WORK_COST_KEYS.has(improvement):
        return BUILD_WORK_COST_NONE
    return maxf(float(src.get(
        prefix + String(FORECAST_BUILD_WORK_COST_KEYS[improvement]), BUILD_WORK_COST_NONE)),
        BUILD_WORK_COST_NONE)

# ---- THE IN-FLIGHT FENCE RING'S METER -----------------------------------------------------------
# The two herd-dict keys the ring's meter lives in, BOTH IN WORK UNITS. `pen_extend_progress` is the
# work banked toward the ring in flight and `pen_extend_cost` is the `animal:pen` rung's own
# `work_cost`, which `Herd::accrue_pen_extension` stamps beside it while the meter is incomplete.
# The pair is unprefixed: it rides the raw herd dict `herds_to_array` decodes, which is the same
# `BARE_FORECAST_PREFIX` source every other herd readout reads.
const PEN_EXTEND_PROGRESS_KEY := "pen_extend_progress"
const PEN_EXTEND_COST_KEY := "pen_extend_cost"

# What `pen_extend_fraction` answers for a ring the wire has not priced yet — an EMPTY meter, which
# is what a ring with nothing banked is.
const PEN_EXTEND_EMPTY_METER := 0.0

## **WORK BANKED TOWARD THE RING IN FLIGHT**, in work units — the numerator of `pen_extend_fraction`,
## and the left-hand absolute of the badge's `Fencing N / M work (P%)` hover.
static func pen_extend_work_done(herd: Dictionary) -> float:
    return maxf(float(herd.get(PEN_EXTEND_PROGRESS_KEY, 0.0)), 0.0)

## **WHAT THE RING IN FLIGHT COMPLETES AT**, in work units — the `animal:pen` rung's own `work_cost`,
## stamped on the herd by the sim's accrual seam. `BUILD_WORK_COST_NONE` where no ring has been
## priced yet.
static func pen_extend_cost(herd: Dictionary) -> float:
    return maxf(float(herd.get(PEN_EXTEND_COST_KEY, BUILD_WORK_COST_NONE)), BUILD_WORK_COST_NONE)

## **IS A RING BEING RAISED ON THIS PEN RIGHT NOW?** — the gate that decides whether the work row's
## standing-rung mark offers a `⌃` or states a ring already under way
## (`docs/plan_standing_upkeep.md` §4.9 item 12c), and the client half of the sim's own
## `Herd::pen_extending`. No new sim gate is needed: a second ring cannot be declared over the first
## because the caret is not drawn while this answers `true`.
##
## ⛔ **IT IS THE NUMERATOR, NOT THE FRACTION, and that is deliberate.** `begin_pen_extension` sets the
## flag and `accrue_pen_extension` stamps the cost, so a ring declared this turn has BOTH fields at
## zero — and a fraction test would put it in the meter's hands, which renders `0%` for a ring that has
## simply not been worked yet. Reading the work banked keeps a declared-but-unaccrued ring out of the
## readout entirely rather than showing it as stalled at nothing. `_build_extend_pen_control` carried
## this same test and this same reasoning before the control moved to the work row.
static func pen_ring_is_in_flight(herd: Dictionary) -> bool:
    return pen_extend_work_done(herd) > BUILD_WORK_COST_NONE

## **THE RING'S METER AS A FRACTION — the ONE place that division is written.** Two surfaces quote a
## ring: the build queue's percentage for an `extend_pen` entry, and the work row's standing-rung mark,
## whose hover states it (`HudWorkVocab.WORK_ROW_RING_BUILDING_TOOLTIP_FORMAT`). Both come through
## here, so one ring can never be quoted two ways.
##
## ⛔ **THE THIRD SURFACE IS RETIRED.** The dead claim, quoted because it named a control that no longer
## exists: *"Two surfaces quote a ring: the herd drawer's WARN-amber `Fencing N%` badge (and its
## in-place patch) and the build queue's percentage."* §4.9 item 12c took the tile card's `Extend pen`
## button and that badge out with it (`_build_extend_pen_control`, `_apply_fencing_badge`,
## `PEN_FENCING_LABEL`) — a ring is declared from the work row's mark now, and the badge was a third
## statement of one meter beside the queue row that dates it.
##
## **`pen_extend_progress` IS WORK, NOT A FRACTION.** It was normalized `0..1` until unit-costed work
## landed; a reader that still scales it by `PROGRESS_PERCENT_SCALE` prints `Fencing 6900%` off 69
## banked work units. The denominator is on the wire beside it, so there is nothing to guess.
##
## **IT IS NOT A RUNG METER AND HAS NO `FORECAST_BUILD_*_KEYS` ROW.** A ring widens the pen rung its
## herd already stands on — the herd reads `Corralled 100%` for the ring's whole life — so
## `improvement_progress` has nothing to answer for it and `build_completion_value`'s ladder credit is
## structurally zero.
##
## **A ZERO DENOMINATOR IS AN UNPRICED RING, NOT A FULL ONE.** `Herd::begin_pen_extension` leaves both
## fields at zero and `accrue_pen_extension` is what stamps the cost, so a ring that has banked no
## turn yet has no denominator to divide by: `0 / 0` is *no ring*, not `0%` and certainly not `100%`.
## It answers `PEN_EXTEND_EMPTY_METER` there rather than dividing. Both live readers additionally gate
## on `pen_ring_is_in_flight` — the NUMERATOR — so neither renders that state at all. (The retired
## drawer badge gated on the same `pen_extend_progress > 0` test, under its own name.)
static func pen_extend_fraction(herd: Dictionary) -> float:
    var cost := pen_extend_cost(herd)
    if cost <= BUILD_WORK_COST_NONE:
        return PEN_EXTEND_EMPTY_METER
    return clampf(pen_extend_work_done(herd) / cost, 0.0, 1.0)

## **WHAT THIS IMPROVEMENT'S RUNG COSTS TO HOLD, PER TURN** — the STANDING price of the rung being
## quoted, read at the rung being PRICED rather than at the rung the source is billed for today. It
## sits on the offered face beside `build_work_cost`'s one-off price: *this much to build it, this
## much every turn to keep it*.
##
## **IT IS NOT A TERM OF THE BUILD'S PACE AND MUST NOT BE SUBTRACTED FROM ONE**
## (`docs/plan_standing_upkeep.md` §2.4). It was, while the build crew supplied the rate below the
## meter's cost; the keeping pool owes it at every fullness now, so `build_turns_at` nets the
## `meter_rot_per_turn` instead and this is a price the player reads rather than a bar the crew
## clears. See `FORECAST_BUILD_UPKEEP_DEMAND_KEYS`, and `upkeep_state`'s `demand` for the other
## question — *what is this source billed right now*.
##
## `NO_UPKEEP_DEMAND` for a rung the wire states no rate on, which is an honest measured nothing (the
## `corralYield` rule): a rung that costs nothing to hold is free to keep once it is built.
static func build_upkeep_demand(src: Dictionary, prefix: String, improvement: String) -> float:
    if not FORECAST_BUILD_UPKEEP_DEMAND_KEYS.has(improvement):
        return NO_UPKEEP_DEMAND
    return maxf(float(src.get(
        prefix + String(FORECAST_BUILD_UPKEEP_DEMAND_KEYS[improvement]), NO_UPKEEP_DEMAND)),
        NO_UPKEEP_DEMAND)

## **WHAT THIS SOURCE'S AT-RISK METER IS LOSING PER TURN, IN WORK UNITS** — the one term that can stop
## a build finishing (`docs/plan_standing_upkeep.md` §2.4), and the term `build_turns_at` nets off the
## build crew's output.
##
## **PER SOURCE, SO IT TAKES NO IMPROVEMENT.** At most one meter on a source carries work at risk, and
## the sim has already resolved which — through the grace, through the rung's own decay rate and
## through what the band's keeping pool actually covered. This client re-derives none of that: it is
## published for the same reason the shortfall is, that a second authority over the number a readout
## exists to explain is how two surfaces come to disagree about one build.
##
## `NO_METER_ROT` is a MEASURED NOTHING and the commonest reading there is — a kept meter, a source
## inside its grace, or a rung whose penalty is a shed rather than a bleed (both animal rungs). A
## source rotting at nothing is a source every staffed builder advances.
static func meter_rot_per_turn(src: Dictionary, prefix: String) -> float:
    return maxf(float(src.get(prefix + FORECAST_METER_ROT_KEY, NO_METER_ROT)), NO_METER_ROT)

## **THE SIM'S OWN TURN ESTIMATE** for whatever this source is building. Per SOURCE, not per rung, so
## it takes no improvement.
##
## **FIVE ANSWERS, AND THE CLIENT COMPARES NOTHING TO TELL THEM APART.** A count is a finish date at
## the crew that is on it; `BUILD_TURNS_HOLDS` is *this staffing holds the meter where it is*, an amber
## `∞`; `BUILD_TURNS_ROTS` is *this staffing is losing work already paid for*, a red one;
## `BUILD_TURNS_QUEUE_BLOCKED` is *the builders are standing here and this rung's own gate refuses
## them*, a hazard whose remedy is off the build line entirely; `BUILD_TURNS_NOT_YET_ESTIMATED` is
## *the sim has not looked at this entry yet*, which is neutral and renders as `Queued`; and
## `BUILD_TURNS_NO_ESTIMATE` is *there is genuinely no answer*, which renders as no line.
##
## **EVERY SENTINEL THE WIRE SPELLS MUST BE PASSED THROUGH, and each new one is a fresh chance to get
## this wrong.** This read accepted `>= 0` and `-2` and flattened everything else to *no answer*, so
## when the sim split `-3` out of `-2` a real, staffed, priced build that was actively bleeding banked
## work rendered as NO LINE — the same silence the tile card and the herd drawer showed for a source
## nobody had touched. Twice now, silence has read as success on this exact row.
##
## **AND THE FIFTH IS THE ONE THIS READER WAS LAST LEFT BEHIND BY.** `BUILD_NOT_YET_ESTIMATED` was
## collapsed onto `-1` by the fall-through below — the client had never heard of it — and the build
## queue's date column then wore the `⚠ Stalled 0%` hazard on an entry queued one command ago. Passing
## a sentinel through is the whole job of this function; each new one is a fresh chance to get it
## wrong, and this is the third time.
##
## **THE SIM DRAWS TWO BOUNDARIES HERE THAT A CLIENT-SIDE COMPARISON WOULD BLUR**, and both reach this
## reader as `-1`: an UNSTAFFED source has promised nothing (a comparison would call every idle
## improvement on the map a never-finisher), and a build whose knowledge, site or species gate does not
## hold accrues nothing for a reason that has nothing to do with staffing. Neither is this client's to
## re-derive — it holds no gates — so an unrecognised negative reads as *no answer* rather than being
## guessed at. **That fallback is a floor, not a licence**: it keeps a future sentinel legible as the
## STALLED hazard instead of blank, and it is not a substitute for reading a value the wire already
## publishes.
static func build_turns_remaining(src: Dictionary, prefix: String) -> int:
    var turns := int(src.get(prefix + FORECAST_BUILD_TURNS_KEY, BUILD_TURNS_NO_ESTIMATE))
    if turns >= 0 or turns == BUILD_TURNS_HOLDS or turns == BUILD_TURNS_ROTS \
            or turns == BUILD_TURNS_QUEUE_BLOCKED or turns == BUILD_TURNS_NOT_YET_ESTIMATED:
        return turns
    return BUILD_TURNS_NO_ESTIMATE

## **WHERE THIS SOURCE SITS IN THE WINNING BAND'S BUILD QUEUE** — 0-based, `NOT_IN_ANY_BUILD_QUEUE`
## when no band has queued it (`docs/plan_standing_upkeep.md` §4.6b).
##
## It is read BESIDE `build_turns_remaining`, never instead of it: the countdown is the sum of every
## entry ahead of this one plus its own span at the full builders pool, so on its own it cannot tell
## forty turns of work from eight turns of work queued behind four other jobs. Anything below zero is
## normalised to the sentinel — a negative position is not a place in a line.
static func build_queue_position(src: Dictionary, prefix: String) -> int:
    var position := int(src.get(prefix + FORECAST_BUILD_QUEUE_POSITION_KEY, NOT_IN_ANY_BUILD_QUEUE))
    return position if position >= BUILD_QUEUE_HEAD else NOT_IN_ANY_BUILD_QUEUE

## **WHY THIS SOURCE'S BUILD IS BLOCKED** — the sim's own cause key, `""` when it is not blocked or
## when the wire says nothing. Read it BESIDE `build_turns_remaining`'s `BUILD_TURNS_QUEUE_BLOCKED`,
## which is what says the builders are standing here at all; this only ever says WHICH conjunct
## refused (`docs/plan_standing_upkeep.md` §4.6b). Passed through verbatim, unrecognised keys
## included — the wording table is the client's and answers an unknown key honestly rather than
## dropping it, exactly as `HudFloraVocab.SOW_REFUSAL_FALLBACK` does for a site refusal.
## **THE BUILDERS KIT THE WINNING BAND'S QUEUE ENTRY RESOLVES TO** — `""` for a source no band has
## queued, which is also the honest answer for a wire that says nothing. Read it BESIDE
## `build_queue_position`: this is a property of the ENTRY, so a source with no position has no kit.
static func build_kit_id(src: Dictionary, prefix: String) -> String:
    return String(src.get(prefix + FORECAST_BUILD_KIT_KEY, "")).strip_edges()

static func build_blocked_reason(src: Dictionary, prefix: String) -> String:
    return String(src.get(prefix + FORECAST_BUILD_BLOCKED_REASON_KEY, "")).strip_edges()

## **WHERE THE QUEUED ENTRY IS TAKING THIS SOURCE** — the destination rung as an IMPROVEMENT VERB,
## `IMPROVEMENT_NONE` when no band has queued it. It is the wire's `<branch>:<id>` read through the
## one crossing table (`RUNG_KEY_IMPROVEMENTS`), so nothing else in the client has to know how the sim
## spells a rung.
##
## Read it BESIDE `build_legs`: this says where the climb ENDS, those say what is LEFT of it. The
## entry retires when the source reaches this rung's top and not when an intermediate rung fills, so
## a two-leg `sow` holds the head of the queue through its Cultivate leg.
static func build_destination_rung(src: Dictionary, prefix: String) -> String:
    var key := String(src.get(prefix + FORECAST_BUILD_DESTINATION_KEY, "")).strip_edges()
    return String(RUNG_KEY_IMPROVEMENTS.get(key, IMPROVEMENT_NONE))

## **WHAT THIS SOURCE WILL CARRY AT THAT DESTINATION** — the wire's own figure, passed through, with
## every "no destination" reading normalised to `NO_BUILD_DESTINATION_CAPACITY`.
##
## Read it BESIDE `build_destination_rung`, never instead of it: that names the rung the climb ends
## on, this is the ceiling the ground holds once it is there. It is the term that explains a FALLING
## take — the escapement floor is `floor x K` and the rung raises `K`, so the floor climbs every turn
## the build runs — and it is the only figure on the wire that says what it is climbing toward.
##
## **A `0.0` HERE IS A REAL CAPACITY AND SURVIVES** (barren ground, an overgrazed range, a rock pen);
## only a NEGATIVE is the absent reading, which is the whole reason the sentinel lives out of range.
## Nothing derived here: the client holds neither the rung gains nor the land the flow is summed over.
static func build_destination_capacity(src: Dictionary, prefix: String) -> float:
    var capacity := float(src.get(prefix + FORECAST_BUILD_DESTINATION_CAPACITY_KEY,
        NO_BUILD_DESTINATION_CAPACITY))
    return capacity if capacity >= 0.0 else NO_BUILD_DESTINATION_CAPACITY

## **IS THERE A DESTINATION TO QUOTE AT ALL** — the ONE test every surface makes on the value above,
## so a reader cannot invent a second spelling of *absent* and render a `0` for it. Exact, not
## approximate: `build_destination_capacity` normalises to the sentinel or to a real `>= 0` reading,
## and there is nothing in between.
static func states_destination_capacity(capacity: float) -> bool:
    return capacity > NO_BUILD_DESTINATION_CAPACITY

## **THE LEGS THE QUEUED ENTRY STILL HAS TO LAY**, in climb order, first-incomplete first — the FIRST
## row is the leg in flight. `[]` when the source is not queued, or has already arrived, and that
## emptiness is a real answer rather than an absence to fill in.
##
## Each row is normalised to `{rung, improvement, work_remaining, turns_remaining}` — the wire's rung
## key crossed to a verb beside it, so a caller matching legs against a branch and a caller naming the
## verb read one row. **NOTHING IS RECOMPUTED**: the work is the leg's owing from where the source
## stands now, and the turns are chained behind the legs above it. A row the wire cannot name a rung
## for is DROPPED rather than carried as a nameless leg, which would render as a step to nowhere.
static func build_legs(src: Dictionary, prefix: String) -> Array[Dictionary]:
    var rows: Array[Dictionary] = []
    var raw: Variant = src.get(prefix + FORECAST_BUILD_LEGS_KEY, [])
    if not (raw is Array):
        return rows
    for entry in (raw as Array):
        if not (entry is Dictionary):
            continue
        var leg: Dictionary = entry
        var key := String(leg.get(BUILD_LEG_RUNG_KEY, "")).strip_edges()
        if not RUNG_KEY_IMPROVEMENTS.has(key):
            continue
        rows.append({
            BUILD_LEG_RUNG_KEY: key,
            BUILD_LEG_IMPROVEMENT_KEY: String(RUNG_KEY_IMPROVEMENTS[key]),
            BUILD_LEG_WORK_KEY: maxf(float(leg.get(BUILD_LEG_WORK_KEY, 0.0)), 0.0),
            # The four negatives are the countdown's own and are passed through VERBATIM — a leg
            # cannot be dated when the entry carrying it cannot, and flattening one into another is
            # how this client twice lost a sentinel the wire had already spelled.
            BUILD_LEG_TURNS_KEY: int(leg.get(BUILD_LEG_TURNS_KEY, BUILD_TURNS_NO_ESTIMATE)),
        })
    return rows

## **THE LEG THE CREW IS ACTUALLY ON** — the first published leg still owing work, as one `build_legs`
## row; `{}` for a source with nothing queued and for one whose climb has arrived.
##
## **IT IS THE ANSWER A DECLARATION CANNOT GIVE.** `build_verb` names the rung the player ORDERED
## (`forage::patch_build_verb` honours a declaration at or above the rung being raised), which for a
## `sow` on untended ground is the Field — a rung standing at 0% while the crew clears the ground
## beneath it. Every readout that quoted that rung's meter therefore sat at zero for the whole first
## leg, which reads as a job that is not moving.
##
## **THE FRACTION TO PUT BESIDE IT IS THE PER-RUNG ONE THE WIRE ALREADY PUBLISHES** —
## `improvement_progress` at this leg's own verb, i.e. the source's one position clamped into that
## rung's span (`forage::patch_rung_work_done`). Nothing here divides `work_remaining` by anything: a
## second derivation of a number the sim publishes is how this arc has shipped defects before.
##
## **A SCAN RATHER THAN `legs[0]`, deliberately.** The sim drops a rung the position has already paid
## for, so on an honest payload the head IS the leg in flight — but a fixture, or a producer that one
## day publishes the whole branch, can carry a paid leg, and a reader that took the head on faith
## would then name a rung nobody is working.
static func build_leg_in_flight(src: Dictionary, prefix: String) -> Dictionary:
    for leg in build_legs(src, prefix):
        if float(leg.get(BUILD_LEG_WORK_KEY, BUILD_LEG_NOTHING_OWED)) > BUILD_LEG_NOTHING_OWED:
            return leg
    return {}

## The BRANCH a source climbs, bottom rung first — `RUNG_BRANCH_PLANT` for a patch, `RUNG_BRANCH_ANIMAL`
## for a herd. One picker, so the two webs' tracks cannot be walked in two different orders.
static func rung_branch_for_kind(source_kind: String) -> Array:
    return RUNG_BRANCH_ANIMAL if source_kind == SOURCE_KIND_HERD else RUNG_BRANCH_PLANT

## **HAS THIS SOURCE ACTUALLY BEEN IMPROVED** — is the rung it STANDS on (the wire's `current_rung`)
## above the BOTTOM rung of whichever branch that key belongs to? Wild land and a wild herd answer
## `false`; every rung a band ever built answers `true`.
##
## **IT TAKES A KEY, NOT A SOURCE, AND THAT IS THE WHOLE POINT.** The branch travels inside the key,
## so this is ONE read for the plant web, the animal web and the branch that does not exist yet —
## where the alternative is every consumer hand-writing `is_cultivated`/`is_field` on one web and
## `domestication`/`corralled` on the other, and growing a third reader the day a route ladder ships.
##
## ⛔ **AN UNKNOWN KEY ANSWERS `false`, DELIBERATELY** — `""` included, which is what a hand-built
## fixture carries. A key naming a branch this client has not been taught means a STALE CLIENT, and
## the two ways of being wrong are not symmetric: `false` shows the player nothing, which is visible
## and safe, while `true` would call every source on that branch improved — its untouched FLOOR
## included — which is exactly the defect this test exists to remove.
##
## **IT IS NOT `_standing_rung_index`, AND MUST NOT BE FOLDED INTO IT.** That one asks *how far up its
## own web's ladder has this SOURCE come*, and needs a source to ask it of; this one asks whether a
## RUNG KEY names anything above its branch's floor, and needs only the key. Two questions.
## (Its own retired reasoning — *"that one reads the stamped achievement flags and carries repair
## semantics"* — died twice over: `_standing_rung_index` reads `improvement_is_done`, i.e. the very
## position the sim publishes, and there are no repair semantics left to carry.)
static func rung_above_branch_floor(rung_key: String) -> bool:
    var key := rung_key.strip_edges()
    if key == "":
        return false
    for branch_variant in RUNG_BRANCHES:
        var branch: Array = branch_variant
        var idx := branch.find(key)
        if idx >= 0:
            return idx > 0
    return false

## **IS THE RUNG A SOURCE STANDS ON AT OR ABOVE THIS ONE?** — the comparison `improvement_is_done` is,
## and the only place the branch ORDER in `RUNG_BRANCHES` is turned into a verdict. Both arguments are
## wire rung keys; the branch travels inside each of them, so this needs no source and no web.
##
## **AT OR ABOVE, NOT EQUAL, because a higher rung retires the one below it.** A Field sown straight
## from wild ground never banked a Cultivate — the sim's `Sow` needs no prior patch — and offering
## `Cultivate this patch` on finished ground was a defect reported from play. `plant:field` is one
## step above `plant:tended` in the branch list, which is the whole of the answer now.
##
## ⛔ **AN UNKNOWN OR EMPTY KEY ANSWERS `false`, ON EITHER SIDE.** `""` is what a hand-built fixture and
## a redacted remembered hex carry, and a key naming a branch this client has not been taught means a
## STALE CLIENT. The two ways of being wrong are not symmetric: `false` reads as *nothing has been
## built here*, which offers the player a rung they may already hold and is visible and harmless,
## while `true` would call every rung on that branch BUILT and quietly retire the whole climb.
## A standing key on a DIFFERENT branch from the target is unknown in exactly this sense.
static func rung_at_or_above(rung_key: String, target_rung_key: String) -> bool:
    var standing := rung_key.strip_edges()
    var target := target_rung_key.strip_edges()
    if standing == "" or target == "":
        return false
    for branch_variant in RUNG_BRANCHES:
        var branch: Array = branch_variant
        var target_idx := branch.find(target)
        if target_idx < 0:
            continue
        return branch.find(standing) >= target_idx
    return false

## **RETIRED — `build_is_queue_head`, *is this source at the head of the queue that funds it?***
##
## ⛔ **IT COULD NOT ANSWER FOR A PARTICULAR BAND, AND BOTH ITS CALLERS WERE ASKING FOR ONE**
## (`docs/plan_standing_upkeep.md` §4.9 item 9a). It read `build_queue_position == 0`, which is
## published per SOURCE and rides the WINNING band — the soonest estimate among the bands working it —
## so it meant *some* band has this at its head, routinely not the band asking. The Builders card
## derived the wrong web's kit and the compose sheet drew a running meter over an entry standing third
## in the acting band's line. `HudBandLaborState.is_band_build_head` / `head_build_branch` answer off
## `PopulationCohortState.buildQueue`, the only list that is a particular band's.

## **THE WORK UNITS THE POOL'S KITS ADD TO ITS OUTPUT THIS TURN** — `workers × gear_per_worker`, the
## gear-only remainder of the build's supply (`intensification::gear_work_supply`). `0` when no build
## is in flight or the crew carries nothing that helps, which is what every readout gates its gear
## line on — a `+0 work` line advertises a tool that did nothing.
##
## ⛔ **IT MEANT *what the tools took OFF the job* AND DOES NOT ANY MORE** (§4.8). Nothing divides by
## it and nothing subtracts it: the pace is `crew × per-worker + this`, of which this is one addend,
## and quoting it apart is what lets a surface separate *what these people can do* from *what their
## tools are worth*. A reader phrasing it as a discount states the opposite of what the sim sends.
static func build_work_from_gear(src: Dictionary, prefix: String) -> float:
    return maxf(float(src.get(prefix + FORECAST_BUILD_GEAR_WORK_KEY, BUILD_WORK_NONE)),
        BUILD_WORK_NONE)

## **WHAT THIS SOURCE COSTS TO HOLD, READ IN ONE PLACE** — the four published upkeep numbers plus the
## neglect countdown beside them, as one dict, so the tile card, the herd drawer, the work board and
## the turn orb cannot read a different subset and reach different conclusions.
##
## **NOTHING HERE IS DERIVED.** `shortfall` is the wire's own field, never `demand − supplied`: it IS
## the decay, and a client that subtracted would be a second authority over the number the readout
## exists to make legible. `supplied` is this source's SHARE of its band's keeping pool (§2.5), not a
## crew on the tile. `crew` is what the RATE is worth in hands — the keeping pool's share once the rung
## stands, the minimum viable BUILD crew while it is still going up, one arithmetic either way.
##
## ⛔ **`demand` IS THE BILL THE KEEPERS WERE HANDED, NOT THE LIVE COST OF HOLDING THE RUNG** (§2.8).
## It was the second thing until the plant web went to ONE POSITION: the keeping rate INTERPOLATES up
## the branch now, so a build banks work between the turn the supply is stamped and the turn it is
## judged, and a lagged supply against a moving bill is permanently short. The sim publishes
## `forage::patch_keeping_basis` — the demand the pass actually answered — which is what makes
## `demand − supplied == shortfall` hold exactly, and is why the trio may be read as one statement.
##
## **SO NOTHING MAY QUOTE IT AS *what would this rung cost to hold*.** That question is the per-rung
## `<rung>UpkeepDemand` pair (`build_upkeep_demand`), which answers for a rung nobody has started —
## the ladder's rate scaled by THIS patch's own tender-load, so the quote is the bill the keeping pool
## will actually be handed rather than a figure true of one biome. This one answers *where is my pooled shortfall landing*, and every
## surviving reader words it that way: the pool card's coverage line, the fund-mode row's presence and
## `_queued_keeping_load`'s already-billed test. The one surface that ever said *"holding this costs
## N"* per source — the `Keeping:` row — was retired in issue #545.
##
## `at_risk` is the pair's gate — `has_neglect_grace` — and it means *there is something here that can
## be lost*. It has to be read before `grace`, exactly as `has_owner` is before `owner`: `0` grace on
## an at-risk source means **the penalty is biting this turn**, and `0` on one that is not at risk
## means nothing is at stake at all. The two are the same number and opposite news.
##
## `prefix` spells the keys, so one call serves a `patch_`-prefixed tile_info and a bare herd dict.
static func upkeep_state(src: Dictionary, prefix: String) -> Dictionary:
    return {
        "demand": maxf(float(src.get(prefix + FORECAST_UPKEEP_DEMAND_KEY, NO_UPKEEP_DEMAND)),
            NO_UPKEEP_DEMAND),
        "supplied": maxf(float(src.get(prefix + FORECAST_UPKEEP_SUPPLIED_KEY, NO_UPKEEP_DEMAND)),
            NO_UPKEEP_DEMAND),
        "shortfall": maxf(float(src.get(prefix + FORECAST_UPKEEP_SHORTFALL_KEY, NO_UPKEEP_DEMAND)),
            NO_UPKEEP_DEMAND),
        "crew": maxi(int(src.get(prefix + FORECAST_UPKEEP_CREW_KEY, NO_UPKEEP_CREW)),
            NO_UPKEEP_CREW),
        "at_risk": bool(src.get(prefix + FORECAST_NEGLECT_GRACE_FLAG_KEY, false)),
        "grace": maxi(int(src.get(prefix + FORECAST_NEGLECT_GRACE_KEY, 0)), 0),
    }

## Does this source cost anything to hold? The one test every upkeep readout gates on, so a rung that
## declares no upkeep renders no row rather than a `0.00 work` one.
static func has_upkeep(state: Dictionary) -> bool:
    return float(state.get("demand", NO_UPKEEP_DEMAND)) >= UPKEEP_WORK_MIN

## Is the keeping being underpaid THIS turn? The gate on every warning the shortfall drives.
static func upkeep_is_short(state: Dictionary) -> bool:
    return float(state.get("shortfall", NO_UPKEEP_DEMAND)) >= UPKEEP_WORK_MIN

## **THE PILE THE RUNG ABOVE THIS SOURCE SWALLOWS TO RAISE** — one row per good, `[]` when the wire
## quotes none. See `FORECAST_BUILD_MATERIAL_COST_KEY`: it prices exactly ONE rung, so a caller may
## only attach it to the rung directly above where the source stands.
static func build_material_cost(src: Dictionary, prefix: String) -> Array[Dictionary]:
    return material_payoff_rows(src.get(prefix + FORECAST_BUILD_MATERIAL_COST_KEY, []))

## **THE PILE `animal:pen` ITSELF SWALLOWS TO RAISE** — one row per good, `[]` when the wire quotes
## none. Published at every position, unscaled, exactly as `corral_work_cost` beside it is, so it
## prices the CLIMB to the pen on a pastoral herd and ANOTHER RING on one already penned. The caller
## disambiguates with `current_rung`, as it already does for the work half.
##
## ⛔ **THIS IS NOT `build_material_cost` ABOVE, AND THE TWO MUST NOT BE COLLAPSED.** That one prices
## the rung DIRECTLY ABOVE where the source stands. `animal:pen` is the top of the animal branch, so
## on a CORRALLED herd — the only source a ring is ever offered from — it is empty, correctly and by
## design. A ring priced from it would state no pile at all, which is precisely the gap this key
## closes. On a pastoral herd the two agree by construction, which is why the difference is easy to
## miss and easier to "simplify" away.
static func corral_build_material_cost(src: Dictionary, prefix: String) -> Array[Dictionary]:
    return material_payoff_rows(src.get(prefix + FORECAST_CORRAL_BUILD_MATERIAL_COST_KEY, []))

## **WHAT HOLDING THE OFFERED RUNG WOULD COST IN GOODS, PER TURN** — the material half of
## `build_upkeep_demand`, read at the rung being PRICED rather than at the rung the source is billed
## for today. The `⌃` track's hold aside quotes the two together: *then 1.00 work + 0.05 hurdles a
## turn to hold*.
##
## ⛔ **`upkeep_material_demand` BELOW IS A DIFFERENT QUESTION** — that is the stamped bill this
## source's CURRENT rung was handed, and on a source mid-climb the two disagree by design, exactly as
## the work pair does. See `FORECAST_BUILD_UPKEEP_MATERIAL_DEMAND_KEYS`.
##
## `[]` for a rung the wire names no material on, which the caller renders as NO material clause
## rather than a zero — and which is every shipped rung but `animal:pen`.
static func build_upkeep_material_demand(src: Dictionary, prefix: String,
        improvement: String) -> Array[Dictionary]:
    if not FORECAST_BUILD_UPKEEP_MATERIAL_DEMAND_KEYS.has(improvement):
        return [] as Array[Dictionary]
    return material_payoff_rows(src.get(
        prefix + String(FORECAST_BUILD_UPKEEP_MATERIAL_DEMAND_KEYS[improvement]), []))

## **WHAT HOLDING THIS SOURCE'S CURRENT RUNG SWALLOWS PER TURN** — one row per good, `[]` for a rung
## that eats no material, which is every rung on the shipped ladder but `animal:pen`.
static func upkeep_material_demand(src: Dictionary, prefix: String) -> Array[Dictionary]:
    return material_payoff_rows(src.get(prefix + FORECAST_UPKEEP_MATERIAL_DEMAND_KEY, []))

## …and what the band's store actually PAID toward it, on the same shape. Read BESIDE the demand and
## never subtracted into it by anyone but `material_upkeep_shortfalls`, which is the one place the
## pair becomes a verdict.
static func upkeep_material_supplied(src: Dictionary, prefix: String) -> Array[Dictionary]:
    return material_payoff_rows(src.get(prefix + FORECAST_UPKEEP_MATERIAL_SUPPLIED_KEY, []))

## **EVERY GOOD THIS BILL WENT SHORT OF, PAIRED WITH BOTH ITS TERMS** — `[{material_id, demand,
## supplied}]`, `[]` when every good was covered. The ONE place the demand/supplied pair is turned
## into a verdict, so the work row's note, its hover and any future reader cannot disagree about what
## "short of hurdles" means.
##
## **BOTH TERMS RIDE THE ROW, never their difference**: the note states *"0.03 of the 0.05 a turn"*,
## which is the sim's own two numbers rendered. The row order is the wire's.
##
## ⛔ **A GOOD MISSING FROM `supplied` IS A ZERO PAYMENT, NOT AN ABSENT ANSWER.** The sim drops a
## ledger entry that holds nothing (`material_payoffs` filters `> 0`), so the store paying NOTHING at
## all toward a good publishes an empty supplied list — the worst shortfall there is, and the one a
## `has()` gate would silently skip.
static func material_upkeep_shortfalls(demand: Array[Dictionary],
        supplied: Array[Dictionary]) -> Array[Dictionary]:
    var paid := {}
    for row in supplied:
        paid[String(row.get(MATERIAL_PAYOFF_ID_KEY, ""))] = float(
            row.get(MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
    var short: Array[Dictionary] = []
    for row in demand:
        var id := String(row.get(MATERIAL_PAYOFF_ID_KEY, ""))
        var wanted := float(row.get(MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
        var got := float(paid.get(id, 0.0))
        if wanted - got < MATERIAL_FLOW_MIN:
            continue
        short.append({
            MATERIAL_PAYOFF_ID_KEY: id,
            MATERIAL_UPKEEP_DEMAND_KEY: wanted,
            MATERIAL_UPKEEP_SUPPLIED_KEY: got,
        })
    return short

## The keys `material_upkeep_shortfalls` writes beside the material id. Named for the same reason the
## payoff pair above is: producer and reader are different scripts.
const MATERIAL_UPKEEP_DEMAND_KEY := "demand"
const MATERIAL_UPKEEP_SUPPLIED_KEY := "supplied"

## **THE FLOOR EVERY MATERIAL RATE IS READ AGAINST** — below it a good is not short, not arriving and
## not owed. Materials are quoted at two decimals throughout this client (a pen's fence rate is
## `0.05/turn`), so the floor is one step under the finest figure anything prints rather than
## `FODDER_FLOW_MIN`'s 0.05, which would swallow the shipped pen bill whole.
const MATERIAL_FLOW_MIN := 0.005

## **HOW MANY KEEPERS THIS SOURCE WANTS** — `upkeepWorkersNeeded`, the MAINTAIN activity's own
## `workers_needed`, read through the one `upkeep_state` reader so the compose sheet's `KEEPERS` row
## and every warning about it quote one number.
##
## **`0` IS A REAL ANSWER AND MEANS *NOBODY IS OWED HANDS HERE***: a wild source, standing on no rung
## that costs anything to hold.
##
## **IT IS PUBLISHED ON BOTH SIDES OF COMPLETION NOW, AND IT IS THE SAME NOUN ON BOTH**
## (`docs/plan_standing_upkeep.md` §2.4). The keeping pool owes the rate from the first work banked
## until the last, so `ceil(demand / PER_WORKER_OUTPUT)` is one arithmetic and one sentence — *hands
## to meet the demand* — whether the meter is going up or standing still. It read `0` mid-build on the
## since-retired premise that an unfinished meter owed no keeping, and for one slice after that it
## additionally meant *the minimum viable build crew*: that reading is gone with the fullness test,
## because a build crew supplies nothing towards the rate and its whole output is progress.
static func keepers_wanted(src: Dictionary, prefix: String) -> int:
    return int(upkeep_state(src, prefix).get("crew", NO_UPKEEP_CREW))

## RETIRED — **`min_build_work(src, prefix, improvement)`**, *"the work a build crew must beat before
## any of it is progress"*, which the BUILDERS stepper stated as its threshold
## (`docs/plan_standing_upkeep.md` §2.4).
##
## **THERE IS NO BUILD-CREW THRESHOLD LEFT TO STATE.** It answered the quoted rung's own upkeep rate,
## which was the term the closed form subtracted while the build crew supplied the rate; the keeping
## pool owes that at every fullness now and a builder's whole output is progress, so the smallest
## useful build crew is one hand. A note naming a threshold that no longer exists is the exact failure
## this arc keeps producing — a warning outliving its mechanism — so it went, along with
## `HudWidgets.BUILD_WORK_FLOOR_META` and `HudComposeVocab.CREW_BUILD_FLOOR_TOOLTIP`.
##
## The rate itself is not lost and did not merely move: it is the **standing price** on the offered
## face now (`build_upkeep_demand`, rendered beside `workCost` by `DetailFormat.build_price_clause`),
## which is what it always was — a price, never a bar to clear.

## **THE ONE UNDER-KEPT TEST, AND IT IS NOW GENUINELY ONE** — this source stands on, or is raising, a
## rung that wants keeping, and the band's pool did not cover it.
##
## **IT IS A SHORTFALL TEST, AND IT HAD TO BECOME ONE** (`docs/plan_standing_upkeep.md` §2.5).
## It compared a CREW COUNT — the `maintain` hands on this source — against `upkeepWorkersNeeded`,
## which was the right question while the keeping was staffed per source. Maintenance left the tile:
## there are no per-source keepers to count, so that reading went to `0 < wanted` on every managed
## source in the game and the ⚠ would have been permanently up, which is worse than absent. What the
## sim decides per source is its SHARE of the band's pool, and `upkeepShortfall` is precisely
## *"the share did not cover this one"* — the same number the decay and the animal web's shed read.
##
## **IT STILL FIRES BEFORE THE ANIMALS GO.** The old objection to a shortfall test was that the
## shortfall described last turn's decay, so the notice arrived after the loss; the rung's GRACE is
## what answers it — `neglectGraceRemaining` counts the shortfall turns forgiven before the penalty
## begins, and the `At risk:` row beside this warning states it.
##
## **`is_unbuilt_and_unpaid` MERGED INTO IT, AND THE DISTINCTION IT KEPT HAD NOTHING LEFT UNDER IT**
## (§4.6a). There were two tests split on `build_is_in_flight` because the two states were owed by
## DIFFERENT CREWS — a rung that stood was owed its keepers, a rung still going up was owed its
## builders. **One pool owes both now**, at every fullness, so the shortfall means the same thing and
## the remedy is the same sentence on either side of completion. Keeping them apart preserved a
## distinction with nothing under it, and made the work board tell a player to staff BUILDERS for a
## bill the keeping pool owes.
##
## **WHAT THE MERGE COST**: the note no longer distinguishes a rung being RAISED from one being HELD.
## It does not need to — the player does the same thing either way — and the source's own card still
## says which, on the rung row that carries the meter.
##
## The positive demand stays beside the shortfall because a rung declaring no upkeep can be short of
## nothing. Reads the same shortfall the `At risk:` row does, so a source warning here always has that
## row beside it explaining what it costs and how long is left.
static func is_under_kept(src: Dictionary, prefix: String) -> bool:
    var state := upkeep_state(src, prefix)
    return int(state.get("crew", NO_UPKEEP_CREW)) > NO_UPKEEP_CREW and upkeep_is_short(state)

## RETIRED — **`is_unbuilt_and_unpaid(src, prefix, kind)`**, *"a rung still going up whose builders are
## not paying the rate"*. See `is_under_kept` above: one pool owes an at-risk meter at every fullness,
## so there was one state wearing two names, and the `build_is_in_flight` split that kept them mutually
## exclusive went with them.

## **WHAT A POOL OF `workers` SUPPLIES IN ONE TURN, in work units** — the client's copy of
## `intensification::pool_work_supply`, and the ONE expression both halves of the work model divide
## by: a build divides its pile by it (`build_turns_at`), and a KEEPING pool covers its bill out of it
## (the pool card's coverage mark). Two spellings of it is how the two surfaces come to disagree about
## what one hand is worth, which is the same two-producers failure the closed form already guards.
##
## **THE GEAR TERM SATURATES IN THE CREW.** Coverage arms a PREFIX of the pool, so ten sets of hoes
## among twenty keepers raise ten of them and the other ten still bring their hands — hence the `min`
## on the head count, which is what makes the form exact rather than approximate.
##
## `per_worker_turn` is the BARE rate the SOURCE publishes (`build_work_per_worker_turn`), never a
## constant held here: the sim writes worker output as a sum of terms, and a client assuming today's
## `1.0` would quote a number the sim disagrees with the moment a second term lands.
static func pool_work_supply(workers: int, per_worker_turn: float,
        kit_gear: Dictionary) -> float:
    var crew := maxi(workers, BUILD_CREW_NONE)
    var equipped := mini(crew, maxi(int(kit_gear.get(
        BUILD_GEAR_SATURATING_CREW, BUILD_CREW_NONE)), BUILD_CREW_NONE))
    return float(crew) * maxf(per_worker_turn, BUILD_WORK_NONE) \
        + float(equipped) * gear_per_worker(kit_gear)

## The kit's own addend, per EQUIPPED worker per turn — floored, so a malformed row cannot make a
## worker worse than bare-handed. Its own reader because two call sites ask it: the supply above, and
## `build_turns_at`'s *is this job priced at all* guard, which must see the bare rate and the tool
## separately (a supply the kit alone pays is a real answer).
static func gear_per_worker(kit_gear: Dictionary) -> float:
    return maxf(float(kit_gear.get(BUILD_GEAR_PER_WORKER, BUILD_WORK_NONE)), BUILD_WORK_NONE)

## **THE BARE WORK ONE WORKER BANKS ON THIS SOURCE IN A TURN**, off the wire's own
## `buildWorkPerWorkerTurn`. `BUILD_WORK_NONE` where the source states none — a measured nothing that
## every caller then guards, never a licence to substitute a constant.
static func build_work_per_worker_turn(src: Dictionary, prefix: String) -> float:
    return maxf(float(src.get(prefix + FORECAST_BUILD_PER_WORKER_TURN_KEY, BUILD_WORK_NONE)),
        BUILD_WORK_NONE)

# ⛔ **RETIRED — `BARE_WORK_PER_WORKER_TURN`, THIS CLIENT'S TRANSCRIPTION OF
# `intensification::PER_WORKER_OUTPUT`.** It existed for one slice because the route branch had no
# published rate to divide by: every SOURCE states its own `build_work_per_worker_turn`, and a road
# has no source row at all. **`RouteRungState.buildWorkPerWorkerTurn` carries it now**
# (`HudRouteVocab.branch_build_work_per_worker_turn`), so the rate is read like every other and the
# constant is gone.
#
# ⛔ **DO NOT PUT IT BACK AS A FALLBACK.** The sim writes worker output as a SUM OF TERMS; a
# transcription goes stale in silence the day a second term lands, which is precisely what a default
# would hide. A missing or zero rate is answered as *no estimate* and *no clause* — never as `1.0`.

## **HOW MANY HANDS A STANDING WORK RATE IS WORTH**, `ceil(work / per_worker_turn)` — the arithmetic
## the sim runs to publish `upkeepWorkersNeeded` from a demand, so a client-side conversion of the
## SHORTFALL lands in the same units as the wire's conversion of the demand.
##
## ⛔ **THE RATE IS A PARAMETER AND HAS NO DEFAULT.** The caller resolves it from whatever publishes it
## — a source row on the two food webs, the rung catalog on the route branch — and
## `BUILD_CREW_NONE` is the honest answer where nothing does: *this client cannot price the gap*,
## which renders as no clause rather than as a head count struck against an invented rate.
##
## **NEVER a subtraction of two head counts.** The keeping is a band-wide pool and its share of any
## one source is the sim's answer; the client holds no per-source head count.
static func workers_for_work(work: float, per_worker_turn: float) -> int:
    if work <= BUILD_WORK_NONE or per_worker_turn <= BUILD_WORK_NONE:
        return BUILD_CREW_NONE
    return ceili(work / per_worker_turn)

## **HOW MANY TURNS THIS RUNG WOULD TAKE AT A CREW AND A FLOOR THE PLAYER IS PROPOSING** — the ONE
## home of the client's turn estimate, and the reason the compose sheet's number moves when the
## stepper does.
##
## **IT IS NOT A SECOND OPINION ABOUT `build_turns_remaining`; IT ANSWERS A DIFFERENT QUESTION.** That
## field is the sim's answer for the crew ALREADY working the source, which is the right and only
## thing for the tile card and the herd drawer — neither has a crew control, so there is no proposal
## to price. A compose sheet has one, and *"add hands and watch it drop"* is the whole point of the
## reading it sits beside. So the sim ships the TERMS as well as the answer and this evaluates them
## (`.claude/rules/core_sim/yield-forecast.md` → "THE BOUNDARY, stated once" — the ceiling's
## discipline, not a per-band ledger term's). Evaluated at the COMMITTED crew and floor the two agree
## exactly, which is the safety argument for having both: a sheet that could disagree with the card
## would lie about the very decision the card then reports.
##
##     gear(b)  = min(b, kit_gear's saturating crew) × kit_gear's per-worker worth
##     net(b)   = b × PER_WORKER_TURN + gear(b) − <this SOURCE's meter rot per turn>
##     turns(b) = ceil((cost − done) / net(b))
##
## ⛔ **THE GEAR TERM IS IN THE DENOMINATOR, and it was in the numerator until
## `docs/plan_standing_upkeep.md` §4.8.** `cost − done − gear(b)` granted the kit's help as a LUMP
## against the pile, once, however long the job ran; a tool is used every turn it is held, so it
## raises what a worker DELIVERS and the job's size never moves. Transcribed from
## `core_sim/tests/build_turns_closed_form.rs::client_turns_estimate`, which is the safety argument
## for the client evaluating any of this — **do not re-derive it here.** The magnitudes moved with the
## meaning and cannot be carried across: the shipped tool declared `8.5` as units off a job and
## declares `0.5` as work added per equipped worker per turn.
##
## **THE MAINTENANCE RATE IS NOT IN THIS FORM, AND REMOVING IT IS SLICE 6a**
## (`docs/plan_standing_upkeep.md` §2.4). The build crew used to supply the rate while a meter was
## below its cost, so the pace was `crew − rate`; the keeping pool owes it at every fullness now and a
## build crew's whole output is progress. **What can still eat a build is the ROT** — a meter whose
## keeping is short bleeds work already bought, and builders raising it more slowly than that are
## losing ground — so the term stayed and its identity changed. `core_sim/tests/
## build_turns_closed_form.rs` pins the two forms equal on the exported snapshot, and the rot does not
## vary with the build crew, which is exactly why the sim publishes it rather than the client
## composing it. **A crew banking exactly the rot answers `BUILD_TURNS_HOLDS` and one under it
## answers `BUILD_TURNS_ROTS`** — both real answers about a stated crew, where
## `BUILD_TURNS_NO_ESTIMATE` names an absent one.
##
## **THIS IS THE ONE COMPARISON THE CLIENT STILL MAKES, and it survives because the sim cannot answer
## the question it asks.** `buildTurnsRemaining` publishes all four answers — including both
## non-finishing ones — for the crew ALREADY on the source, and every crewless surface reads it
## through `build_turns_remaining` rather than re-deriving anything. A stepper the player is dragging
## is a crew the sim has never seen, so there is nothing to read; what makes that safe is that the two
## agree exactly at the COMMITTED crew, which is now checkable on three states rather than two.
##
## **THE "NO ANSWER" BOUNDARY IS *IS THERE WORK BANKED*, NOT *IS ANYONE STAFFED*** (§4.6a). A
## proposal of nobody on a meter carrying work is a real question with a real answer — the meter HOLDS
## where the keeping covers it and ROTS where it does not — and it is this same form at `b = 0`, which
## is exactly what the sim publishes for zero builders. Nothing banked and nobody on it is the DECLARED
## state and stays `BUILD_TURNS_NO_ESTIMATE`, its own *not started* warning speaking for it.
##
## **THE SIM'S REMAINING BOUNDARY IS HONOURED BY CONSTRUCTION, NOT BY A SECOND OPINION**: a rung whose
## knowledge, site or species gate refuses it is never priced at all — a GATED control spends its whole
## slot on the reason and quotes no price, so this is never reached for one.
##
## **`b` IS THE BUILD'S OWN CREW, and the gear is quoted at it** (`docs/plan_standing_upkeep.md`
## §2.2). A source carries three allocations now; the builders are the ones filling this meter, so a
## tool's contribution — a rate per worker — is summed over the *builders*, and the coverage behind it
## is resolved over the builders too. The ROT is not quoted at any crew: it is what the KEEPING pool
## failed to cover, which is a fact about the source that this stepper cannot move.
##
## **THE FLOOR IS NO LONGER A FACTOR ON THE RATE.** `learn_multiplier(floor)` scaled the accrual while
## one crew both gathered and built — *a crew pulling hard on the source it is improving builds
## slowly*. **With separate crews the build crew is not pulling anything**, so the sim's
## `build_accrual` takes no floor at all and neither does this. `learn_multiplier` still paces the
## KNOWLEDGE accrual, which is where *how much you leave standing shapes what you learn* still holds —
## and that is why `floor` stays a parameter here: the work PREDICATE below reads it.
##
## `kit_gear` is the BUILDERS' kit, as `BUILD_GEAR_PER_WORKER` / `BUILD_GEAR_SATURATING_CREW` off the
## band's own resolved row (`KitRoster.build_gear`) — the caller passes it because which kit the pool
## carries is a fact about the BAND'S `builders` row rather than about this source, and it is what
## lets a kit swap re-price the whole estimate. `{}` is a legal reading and means the crew carries
## nothing that helps.
##
## ⛔ **IT IS NOT THE COMPOSE SHEET'S OWN PICKER, which chooses what the TAKE crew carries.** Both
## sheets passed their own selection for a release — the GATHERING kit on the forage sheet, which
## declares no build axis at all — and `DrawerComposeController._build_gear_for` is the seam that
## resolves the entry's own kit and its web instead (`labor-ui.md` → "A BUILD IS PRICED AT THE
## **BUILDERS'** KIT").
##
## **THE WORK PREDICATE IS PART OF THE FORM, and leaving it out is a lie at the loudest end of the
## slider.** `RungDef::build_accrual`'s `eligible` carries `crew_is_working_the_source` on Cultivate
## and Tame, so a floor above the source's own stock fraction accrues NOTHING — while
## `learn_multiplier` is at its largest there. Without this term four foragers dragged to a 100% floor
## were quoted the FASTEST estimate on the whole axis for a build the sim was not advancing at all,
## beside a tile card correctly rendering no turn line. `escapement_room` is the client's copy of
## `max(0, B − floor·K)`; see `BUILD_WORK_PREDICATE_IMPROVEMENTS` for why it rides two rungs and not
## four, and `escapement_room`'s own note for why THIS is the one place that composition may be
## reached for.
##
## `BUILD_TURNS_NO_ESTIMATE` — rendering as no clause at all — for every case with no finite answer:
## a rung the wire prices nothing on, a source banking nothing per worker-turn, a rung standing over an
## empty escapement room, and a rung with **nothing banked and nobody on it**. Each is the sim's own
## `None`, which it reserves for a build that is **stalled** and for nothing else. **A crew of nobody
## is NOT on that list any more** (§4.6a): on a meter carrying work it is a real, common state with a
## real answer, and the boundary is `is there work banked` — see above.
##
## **A METER ALREADY AT ITS COST IS `BUILD_FINISHES_IN_ONE_TURN`, NOT "no estimate".** A bar at or
## below zero completes on the first worked turn (`docs/plan_unit_costed_work.md` §6.2), which is an
## ANSWER; withholding the line there blanked the readout on a finished build.
##
## **GEAR CAN NO LONGER REACH THAT BRANCH, and that is the move stated as a consequence** (§4.8). It
## used to be the common way in — a start-stocked band's handling gear covered a 50-unit Tame at six
## keepers, so the estimate fell 25 → 13 → 4 → 2 → *nothing* as hands were added — because the kit was
## subtracted from the pile. A kit raises the RATE now, so more hands make the count smaller and never
## make the job vanish.
static func build_turns_at(src: Dictionary, prefix: String, improvement: String, workers: int,
        floor: float, kit_gear: Dictionary) -> int:
    # **A PROPOSAL OF NOBODY IS AN ANSWER WHEREVER WORK IS BANKED** (`docs/plan_standing_upkeep.md`
    # §4.6a). It was flatly *no estimate*, on the premise that a crew of nobody has promised nothing;
    # what the sim now publishes for zero builders is the meter's own fate — it HOLDS where the keeping
    # covers it and ROTS where it does not — and both are exactly this form evaluated at `b = 0`. So
    # the boundary moved from *is anyone staffed* to **is there work banked**, which is what the
    # branches below produce on their own once this early-out stops swallowing them.
    #
    # Nothing banked AND nobody on it is still no answer: that is the DECLARED state, and the rung's
    # own *not started* warning is what speaks for it.
    if workers <= BUILD_CREW_NONE \
            and build_work_done(src, prefix, improvement) <= BUILD_WORK_NONE:
        return BUILD_TURNS_NO_ESTIMATE
    var cost := build_work_cost(src, prefix, improvement)
    if cost <= BUILD_WORK_COST_NONE:
        return BUILD_TURNS_NO_ESTIMATE
    # **THE WORK PREDICATE, on the two rungs that carry it in the sim.** No room above the floor means
    # no build, so no estimate — the same `max(0, B − floor·K)` the yield curve composes, and the only
    # other place the client may compose it.
    # **IT IS ASKED AT EVERY STAFFING, INCLUDING NONE, BECAUSE THE SIM ASKS IT THAT WAY.**
    # `RungDef::build_accrual`'s `eligible` carries `crew_is_working_the_source`, which reads the STOCK
    # against the floor and takes no crew count at all — so a floor above the source's own stock makes
    # the sim answer `-1` whatever the staffing. It was gated on `workers > BUILD_CREW_NONE` for one
    # pass, on the reasoning that nothing accrues at zero builders anyway; that is true and it is not
    # this predicate's question, and the gate made the sheet answer the neutral *held* on a source the
    # card correctly called `⚠ Stalled`. **Two producers disagreeing about one meter is the exact thing
    # the closed-form equality exists to prevent.**
    if BUILD_WORK_PREDICATE_IMPROVEMENTS.has(improvement) \
            and escapement_room(src, prefix, floor) <= BUILD_NO_ESCAPEMENT_ROOM:
        return BUILD_TURNS_NO_ESTIMATE
    var per_worker_turn := build_work_per_worker_turn(src, prefix)
    # **THE GEAR TERM SATURATES IN THE CREW, and the `min` is on the HEAD COUNT.** Coverage arms a
    # prefix of the pool, so an eleventh builder with ten sets of hurdles between them adds only their
    # own hands — without the `min` this would keep crediting gear the band does not hold. The whole
    # sum is `pool_work_supply`'s, so the pace a build divides by and the supply a keeping pool covers
    # its bill out of are ONE expression rather than two spellings of it.
    #
    # ⛔ **IT IS AN ADDEND ON THE SUPPLY, NOT A LUMP OFF THE JOB** (`docs/plan_standing_upkeep.md`
    # §4.8, transcribed from `core_sim/tests/build_turns_closed_form.rs::client_turns_estimate`). It
    # used to be `workCost − workDone − gear(w)`: the kit's help granted ONCE against the pile,
    # however long the job ran. A tool is used every turn it is held, so it raises what a worker
    # DELIVERS per turn and the numerator is the job, whole. **`buildWorkPerWorkerTurn` stays the BARE
    # rate on the wire** rather than arriving pre-averaged, which is what preserves this saturating
    # prefix for a crew the sim has never resolved — the one crew a compose sheet exists for.
    var supply := pool_work_supply(workers, per_worker_turn, kit_gear)
    if per_worker_turn <= BUILD_WORK_NONE and gear_per_worker(kit_gear) <= BUILD_WORK_NONE:
        # Nothing prices this job at all — no bare rate and no tool — so there is no rate to divide by
        # and no crew size that would change it: an absent question rather than a crew that falls
        # short. **The gear half of the test arrived with the term's move**: a supply the kit alone
        # pays is a real answer, and testing the bare rate on its own would have withheld it.
        return BUILD_TURNS_NO_ESTIMATE
    # **THE ROT COMES OFF THE TOP, AND IT IS THE ONLY THING THAT DOES** — the published
    # `meter_rot_per_turn`, what this source's at-risk meter is losing while the band's keeping pool
    # falls short of it. Read, never composed: the shortfall, the grace and the rung's decay rate are
    # all resolved sim-side, and a client re-deriving any of them would be a second authority over the
    # number the whole readout explains.
    #
    # **IT WAS THE QUOTED RUNG'S UPKEEP RATE, AND THE FULLNESS TEST IS WHAT DELETED THAT**
    # (`docs/plan_standing_upkeep.md` §2.4). The build crew supplied the rate while a meter was below
    # its cost, so only its surplus was progress; the keeping pool owes the rate at every fullness now
    # and a builder's whole output is progress. **On a rung nobody has started that leaves
    # `workCost / crew`** — nothing is banked, so there is nothing to rot — which is the right answer
    # and not the issue-#545 defect returning: one builder against `plant:tended` really does bank one
    # work a turn, and the 2.0 is the keeping pool's bill rather than a tax on the build.
    #
    # **IT IS CONSTANT WITH RESPECT TO THE STEPPER**, which is why the sim can publish it and the
    # client can still price a crew the sim has never seen: dragging the builders moves the progress
    # and never the rot.
    var work_per_turn := supply - meter_rot_per_turn(src, prefix)
    # **AND THE NON-FINISHING ANSWER FORKS ON THE SIGN, exactly where the sim forks it**
    # (`intensification::build_turns_estimate`, split on `BUILD_BALANCE_HOLDS`). A sheet that answered
    # `HOLDS` for both would quote an amber `∞` for the crew the tile card renders in red — the two
    # producers disagreeing about the very decision the stepper is being dragged through, which is the
    # one thing having two producers is not allowed to cost.
    if work_per_turn < BUILD_BALANCE_HOLDS:
        return BUILD_TURNS_ROTS
    if work_per_turn <= BUILD_WORK_NONE:
        return BUILD_TURNS_HOLDS
    # **THE NUMERATOR IS THE JOB, WHOLE — no gear comes off it** (§4.8). A job's work requirement never
    # changes: a 50-work Cultivate costs 50 work with hoes, without hoes, and with any tool that ever
    # ships. What the kit decides is how fast the pile is worked off, which is the denominator above.
    var remaining := cost - build_work_done(src, prefix, improvement)
    # **A METER ALREADY AT ITS COST FINISHES ON THE FIRST WORKED TURN** — an answer, and the sim's own
    # (`intensification::build_turns_remaining` returns `Some(BUILD_FINISHES_IN_ONE_TURN)` here).
    # Answering `BUILD_TURNS_NO_ESTIMATE` instead left the tile card quoting `≈1 turn` beside a sheet
    # quoting nothing for the same build.
    if remaining <= BUILD_WORK_NONE:
        return BUILD_FINISHES_IN_ONE_TURN
    return ceili(remaining / work_per_turn)

## **THE SAME SATURATING QUOTIENT, ASKED OF EVERY ACCOUNT THE AXIS IS NOT** — the crew past which this
## source pays nothing MORE in fodder or in any one material. `NO_CREW_ANSWER` when no off-axis
## account prices a crew at all, which is the only reading that means *barren*.
##
## **IT EXISTS BECAUSE THE AXIS STOPPED BEING A CHOICE.** `axis_per_worker` used to resolve to
## whichever of provisions/trade the species actually paid, so an inedible quarry was priced on the
## account it pays; arc #527 retired the trade half and left the triple a plain alias of the food
## pair, which quietly turned the food-denominated barren test into a test on **every** source that
## pays no food. A hay meadow and a tobacco stand are not dead-season patches.
##
## **THE MAX ACROSS ACCOUNTS, NOT THE MIN OR THE FIRST.** The cap answers "beyond this crew, nobody
## adds anything", so it has to be the LARGEST crew any single account can still use — one that
## saturated the fodder account while a material row was still short would call the hands taking that
## material idle. On a wild source every account is one biomass flow through a fixed per-biomass
## vector, so the quotients agree and the `max` is free; on a rung-3 managed source the payoffs are
## independent per-turn figures and it is doing real work.
##
## Each account's arithmetic is the food side's, verbatim: a rate below `FORECAST_MIN_PER_WORKER` is
## nothing to divide by and abstains, and a zero ceiling over a live rate is `0` — the source standing
## at its floor, which §7.2's crew floors then answer for.
static func off_axis_useful_workers(forecast: Dictionary) -> int:
    var crew := NO_CREW_ANSWER
    var per_worker_fodder := float(forecast.get("per_worker_fodder", 0.0))
    if per_worker_fodder >= FORECAST_MIN_PER_WORKER:
        crew = maxi(crew, ceili(float(forecast.get("ceiling_fodder", 0.0)) / per_worker_fodder))
    # **THE MATERIAL ACCOUNT IS A VECTOR AND IS ASKED ROW BY ROW** — never summed, the standing rule
    # for this account (`material_rows_of`). The two vectors are unioned BY ID exactly as
    # `expected_materials` unions them, so a rate with no matching ceiling reads a `0` room rather
    # than pairing itself with whatever row happens to sit at the same index.
    var ceilings := {}
    for row_variant in forecast.get("material_ceiling", []):
        if not (row_variant is Dictionary):
            continue
        var ceiling_row: Dictionary = row_variant
        ceilings[String(ceiling_row.get(MATERIAL_PAYOFF_ID_KEY, ""))] = \
            float(ceiling_row.get(MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
    for row_variant in forecast.get(FORECAST_PER_WORKER_MATERIAL_KEY, []):
        if not (row_variant is Dictionary):
            continue
        var row: Dictionary = row_variant
        var rate := float(row.get(MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
        if rate < FORECAST_MIN_PER_WORKER:
            continue
        var room := float(ceilings.get(String(row.get(MATERIAL_PAYOFF_ID_KEY, "")), 0.0))
        crew = maxi(crew, ceili(room / rate))
    return crew

## **DOES THIS SOURCE PRICE A WORKER IN ANY ACCOUNT AT ALL?** — a test on the RATES alone, deliberately
## not on the rooms: it asks what this source PAYS, which is a structural fact, where every ceiling
## above is a function of the floor and of what happens to be standing there this turn.
##
## **IT IS THE PREDICATE THE STAGED-CREW GUARD IS SCOPED BY**, and that guard is an ASSERTION rather
## than a clamp — `ui_preview`'s `forage_cash_crop_field` pins that a sheet stages at least the crew
## already committed on a source paying into any account, and `DrawerComposeController
## ._forecast_worker_cap` records why a runtime floor there was tried and reverted.
##
## **THE SCOPE IS WHAT KEEPS `MAX_USEFUL_BARREN` REACHABLE.** A source that genuinely pays nothing in
## any account answers `false`, so the barren cap of one worker still binds however many hands are
## standing on it — the same argument `max_useful_workers` makes for withholding its own crew floors
## there.
static func pays_any_account(forecast: Dictionary) -> bool:
    if float(forecast.get("axis_per_worker", forecast.get("per_worker", 0.0))) \
            >= FORECAST_MIN_PER_WORKER:
        return true
    if float(forecast.get("per_worker_fodder", 0.0)) >= FORECAST_MIN_PER_WORKER:
        return true
    # **THE MATERIAL ACCOUNT IS ASKED ROW BY ROW, never summed** — the standing rule for this account,
    # and here any single paying row is enough to answer the question.
    for row_variant in forecast.get(FORECAST_PER_WORKER_MATERIAL_KEY, []):
        if not (row_variant is Dictionary):
            continue
        if float((row_variant as Dictionary).get(MATERIAL_PAYOFF_AMOUNT_KEY, 0.0)) \
                >= FORECAST_MIN_PER_WORKER:
            return true
    return false

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
## - `MAX_USEFUL_BARREN` (1) — described, and it pays nothing in ANY account. The cap stays LIVE,
##   which is the fix: unbounded here let a worthless source absorb the whole idle crew (measured at
##   7 workers on a source that can use 1), because both cap twins read unbounded as "no ceiling".
## - a real `ceil(ceiling / per_worker)`, on the axis if the axis pays and off it if it does not.
## The take curve a forecast was composed with, in the shape the searches above want. `[]` — i.e. no
## curve, i.e. the closed forms — for every forecast built without one.
static func per_crew_of(forecast: Dictionary) -> Array:
    var rows: Variant = forecast.get("per_crew", [])
    return rows if rows is Array else []

## The forecast slot the sim's published plateau travels in — deliberately spelled differently from
## the assignment key it is copied from, so a forecast composed off a HERD dict (which carries no
## such key) can never pick one up by accident.
const FORECAST_PUBLISHED_USEFUL_CREW_KEY := "published_useful_crew"

## **NO CREW IS USEFUL HERE** — the sim's `fauna::NO_USEFUL_CREW` as it arrives on the wire, and the
## cap `max_useful_workers` returns for it. A bare-handed party against a `defense` it cannot clear
## lands exactly zero however many people it sends, so *"one worker is useful"* would be a false
## floor and **no crew floor applies to it**, for the same reason none applies to `MAX_USEFUL_BARREN`:
## flooring a cap on the hands that would take the regrowth staffs a crew against a take of zero.
const PUBLISHED_NO_USEFUL_CREW := 0

## **CARRY THE SIM'S OWN CEILING ONTO A FORECAST** — a copy of the work row's
## `ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY`, made ONLY where the row actually published one.
##
## **THE PRESENCE TEST IS THE WHOLE FUNCTION.** The wire's `0` is *no crew is useful here* on a hunt
## row and *does not apply* on every other row, and those two readings must not collapse: a forage
## row copied through here would cap its `+` at nothing. So the copy happens in ONE place — the Work
## board's hunt branch — and a source that published nothing is returned untouched, which leaves the
## closed forms answering exactly as they did.
static func with_published_useful_crew(forecast: Dictionary, source: Dictionary) -> Dictionary:
    if not source.has(ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY):
        return forecast
    # **THE FIGHT IS THE GATE, exactly as it is on the sheet's fight lines** — one predicate, four
    # surfaces (`quarry_is_fought`). The wire publishes this field on every `Hunt` row, and a PEN is a
    # hunt row; what is dropped here is the PLANT web, whose rows carry no such cap to copy.
    #
    # ⛔ **THIS READ `has_engagement_stage` AND SO DISCARDED THE SIM'S ANSWER AT EVERY PEN.** The
    # stated reason was that *"a penned beast is collected rather than stalked, so it states
    # `NO_ENGAGEMENT_STAGE` and its cap was never the fightless quotient this replaces"* — true while a
    # pen resolved no fight, and false the moment §4.9 item 12b made it resolve the ordinary one. The
    # sim now publishes `NO_USEFUL_CREW` for a bare-handed pen it will pay nothing for
    # (`core_sim/tests/hunt_useful_crew_on_the_wire.rs` →
    # `a_penned_rows_cap_is_its_own_curve_and_the_fight_gates_it_too`), and throwing that answer away
    # left the Work board's `+` offering hands the sim had just called useless. The cap is READ from
    # the wire here, never derived, so keeping it for a pen adopts the sim's own curve — the room above
    # the floor, the keepers' reach and calmed retreat, the fight, and their haul — rather than binding
    # a pen to a stalking plateau, which is what the retired sentence was rightly afraid of.
    if not is_fought(float(forecast.get(FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE)),
            bool(forecast.get(SOURCE_CORRALLED_KEY, false))):
        return forecast
    var out := forecast.duplicate()
    out[FORECAST_PUBLISHED_USEFUL_CREW_KEY] = maxi(
        int(source[ASSIGNMENT_HUNT_USEFUL_WORKERS_KEY]), PUBLISHED_NO_USEFUL_CREW)
    return out

## The ceiling the sim published for this source, or `NO_CREW_ANSWER` where it published none — which
## is every surface with no assigned row behind it (the compose sheet, which holds the curve itself,
## and every pre-commit forecast).
static func published_useful_crew(forecast: Dictionary) -> int:
    if not forecast.has(FORECAST_PUBLISHED_USEFUL_CREW_KEY):
        return NO_CREW_ANSWER
    return maxi(int(forecast[FORECAST_PUBLISHED_USEFUL_CREW_KEY]), PUBLISHED_NO_USEFUL_CREW)

static func max_useful_workers(forecast: Dictionary) -> int:
    if not bool(forecast.get("known", false)):
        return MAX_USEFUL_UNBOUNDED
    # ON THE AXIS THE SPECIES PAYS (issue #337): a wolf's food per-worker and ceiling are both 0, so
    # the food-denominated cap would read ceil(0/0) and cap the crew at nothing.
    #
    # **THE AXIS TRIPLE IS A PLAIN ALIAS OF THE FOOD PAIR NOW** (arc #527 retired the trade account it
    # could otherwise resolve to), so reading it is no longer a widening — it is the food question
    # under another name, and the widening this branch used to get for free has to be written out
    # below. `off_axis_useful_workers` is that, and it is why the barren test is no longer this
    # quotient's `if`.
    var per_worker := float(forecast.get("axis_per_worker", forecast.get("per_worker", 0.0)))
    var ceiling := float(forecast.get("axis_ceiling", forecast.get("ceiling", 0.0)))
    # BOTH PROJECTION-DERIVED FLOORS: the crew that takes the regrowth every turn, and the crew that
    # draws the stock down to the floor at all (`reach_crew` — the number the *clear it now* target is
    # floored on, and therefore the number the stepper has to be able to reach). Resolved ABOVE the
    # off-axis branch, because a source that pays no food has the same two targets drawn beside it and
    # the same §7.6 promise to keep: neither pill may name a crew the stepper refuses to reach. They
    # are denominated in BIOMASS, so they answer for a hay meadow exactly as they do for a wheat one.
    var hold := maxi(maxi(int(forecast.get("hold_crew", 0)), 0),
        maxi(int(forecast.get("reach_crew", 0)), 0))
    # **WHERE THE SIM'S CURVE STOPS RISING**, bound once for both branches below. It REPLACES the
    # account quotient rather than joining it in a `max`: the quotient's answer is "the hands that
    # carry and reach this ceiling" and the curve's is "the hands past which nothing more is taken",
    # and where they differ the curve is the one that has resolved the fight. `NO_CREW_ANSWER` on
    # every surface with no reply in hand, which is where the quotients keep their job.
    var per_crew := per_crew_of(forecast)
    var plateau := crew_take_plateau(per_crew)
    if plateau != NO_CREW_ANSWER and not crew_take_curve_settled(per_crew):
        # **THE CURVE RAN OUT OF ROWS WHILE STILL CLIMBING**, so it has proved that every crew it was
        # asked about is still buying take and has said nothing about the ones past that. The closed
        # form answers the tail, exactly as it does with no reply at all — with the curve's own last
        # row as a FLOOR under it, so the cap can never land below a crew the rows showed to be useful.
        # `expedition_useful_cap` runs the identical shape on the raid branch.
        hold = maxi(hold, per_crew.size())
        plateau = NO_CREW_ANSWER
    if plateau == PUBLISHED_NO_USEFUL_CREW:
        # **A CURVE OF ZEROES CAPS THE STEPPER AT ONE, NOT AT NONE** — `MAX_USEFUL_BARREN`, and the
        # barren-patch arm below already carries the argument verbatim: *we know what this source pays,
        # and it is nothing, so the honest ceiling is one worker.*
        #
        # **THE SHEET'S CAP AND THE SIM'S SCALAR ANSWER DIFFERENT QUESTIONS, and reading them as one
        # is what broke this.** `fauna::hunt_useful_crew` gates the Work board's `+` on an ALREADY-WORKED
        # row — *would another hand buy more take* — where `NO_USEFUL_CREW` is the honest floor. This
        # cap gates *may I start this at all*, and the answer to that is never *no*: a player must be
        # able to staff a herd sitting at (or under) its floor, and be told the take begins once it
        # grows past it. Returning the sim's zero here left the stepper pinned at `0` with a dead `+`
        # and `max 0 workers useful here` beneath it — reported from play on a Wild Aurochs.
        #
        # **NO FLOORS APPLY TO IT**, exactly as they do not to the barren answer one branch down:
        # `hold` and `reach` can both be large on a source paying nothing, and flooring on them would
        # park a crew against a take of zero.
        return MAX_USEFUL_BARREN
    if plateau == NO_CREW_ANSWER:
        # **THE WORK BOARD READS THE SIM'S ANSWER RATHER THAN INVERTING THE TAKE.** A worked row is
        # priced with no query reply in hand, so the curve above is empty there and the closed form
        # below used to answer — `take_workers`, which divides by `engagement_carry`: the engagement
        # and the retreat with **no attack, no defense and no durability** in it. On a fight-bound
        # quarry that read 2.3× high, so the board quoted a different ceiling from the compose sheet
        # for the same herd. The sim now publishes the plateau of its OWN curve on every assigned
        # hunt row, and this is where the board picks it up — the same slot the curve's plateau
        # occupies, so both twins (`source_worker_cap_state` and
        # `DrawerComposeController._forecast_worker_cap`) inherit it without either being told.
        var published := published_useful_crew(forecast)
        if published == PUBLISHED_NO_USEFUL_CREW:
            # **NO CREW IS USEFUL HERE, and no floor may raise that.** The sim has said this party
            # brings nothing down at any headcount; flooring on the hands that would take the
            # regrowth would staff a crew against a take of zero, which is the parking
            # `MAX_USEFUL_BARREN` refuses one account over.
            return PUBLISHED_NO_USEFUL_CREW
        plateau = published
    if per_worker < FORECAST_MIN_PER_WORKER:
        # **THE AXIS IS SILENT — ASK THE OTHER ACCOUNTS BEFORE CALLING THE SOURCE BARREN.** Reported
        # from play on a wild patch of 56% tobacco + 44% hay grass: it pays no food by construction
        # (tobacco pays its own material, hay pays fodder and fibre), so this branch fired and printed
        # `max 1 worker useful here — more would be idle` beneath the sheet's own `13 clear it now`
        # and `2 hold it after`, with the `+` dead at 1. Three numbers, no two agreeing.
        var off_axis := off_axis_useful_workers(forecast)
        if off_axis == NO_CREW_ANSWER:
            # **Described, and barren on every account it could be counted on** — a dead-season patch.
            # Not unbounded: we know what this source pays, and it is nothing, so the honest ceiling is
            # one worker. Returning UNBOUNDED here was the second half of #426 — it did not merely drop
            # the "max N useful" note, it removed the ceiling from both cap twins, so the guard against
            # parking a crew on a worthless source was disabled by precisely the sources it exists for.
            #
            # **NO FLOORS APPLY TO THIS ANSWER, and that is deliberate.** A source can price a crew
            # (`per_worker_biomass > 0`, hence a positive hold/reach crew) while paying into nothing at
            # all; flooring the barren cap on those targets would staff hands against a take of zero,
            # which is the very parking this constant exists to refuse.
            return MAX_USEFUL_BARREN
        # **A CURVE BINDS THE OFF-AXIS CAP TOO**, and it has to: every account a quarry pays is a
        # conversion of the ONE biomass the party puts on the ground, so a wolf's pelt cap and a
        # deer's food cap are the same crew question asked in two currencies. Below the barren test,
        # which is a statement about the source rather than about the crew and still wins.
        if plateau != NO_CREW_ANSWER:
            return maxi(plateau, hold)
        return maxi(off_axis, hold)
    # WHOLE-ANIMAL HUNT: the cap is the carriers needed to HAUL the animals that drop on the worst turn,
    # not ceil(smoothed-rate / per_worker). An 80-biomass aurochs drops all at once; one hunter carrying
    # <per_worker> food wastes the rest, so the smoothed rate under-counts. Worst case the kill-credit
    # bank holds just under one body when the turn's rate lands, so floor(ceiling / food_per_animal) + 1
    # whole animals drop, each worth food_per_animal — carry that peak, not the average flow. It is
    # `haul_workers`, the ONE mirror of the sim's rounding, in the paid account's units rather than in
    # biomass (an animal count is a ratio, so either set of units gives the same crew).
    var per_animal := float(forecast.get("axis_per_animal", forecast.get("food_per_animal", 0.0)))
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
        # **WHERE THE CURVE STOPS RISING IS WHAT "USEFUL" MEANS**, and the curve is the only thing
        # that knows. `take_workers` is `max(haul, engage)` — the crew that REACHES the peak drop and
        # CARRIES it home — and it never asks whether that crew can kill what it reaches, so on a Wild
        # Aurochs it declared 13 of 37 hunters useful while the 14th, the 20th and the 30th were all
        # still buying take. `crew_take_plateau` answers the question the note actually poses.
        if plateau != NO_CREW_ANSWER:
            return maxi(plateau, hold)
        return maxi(take_workers(ceiling, per_animal, per_worker,
            float(forecast.get("engage_rate", NO_ENGAGEMENT_STAGE)),
            float(forecast.get("stay", STAY_FRACTION_NONE_BREAKS_OFF))), hold)
    return maxi(int(ceilf(ceiling / per_worker)), hold)

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
## **`useful_floor` IS RETIRED, AND WITH IT `herd_crew_floor`** (`docs/plan_standing_upkeep.md` §2.2).
## Both twins used to RAISE this ceiling to a managed herd's `herdersNeeded`, because one crew both
## hunted and held the animals: a cap sized on the take alone went dead below the count the sim was
## asking for, while the same row rendered the under-herded ⚠. **Those keepers are the MAINTAIN
## allocation now.** Flooring the TAKE cap on them made the hunt stepper demand hands that belong to
## another crew — and the crew that answers `herdersNeeded` has its own stepper, its own ceiling
## (`idle + this source's keepers`) and its own command.
##
## **THE *HOLD* CREW IS STILL FOLDED IN, and it is a different thing** — the hands that take what the
## source REGROWS, a fact about this take at this floor rather than a demand a kind of source makes.
## It lives inside `max_useful_workers`, where both twins pick it up without either caller being
## trusted to remember it.
static func source_worker_cap_state(forecast: Dictionary, workers: int, idle: int) -> Dictionary:
    var useful := max_useful_workers(forecast)
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

## **WHAT THIS CREW ACTUALLY BANKS NEXT TURN** — the same `min` against `next_ceiling`, which is the
## room after this turn's growth (`escapement_room_next_turn`). It is the compose readout's HEADLINE
## and nothing else reads it.
##
## **A SOURCE HELD AT ITS FLOOR IS WHY IT EXISTS.** `ceiling` is the room standing right now, which on
## such a source is EMPTY by construction — so the sheet read `PER TURN 0.00 FOOD` and *"takes nothing
## until it grows past 103"* beside a work board quoting `+0.96 /turn` for the same tile, both correct
## about different questions. At equilibrium this answers the regrowth, which is what reconciles the
## two automatically.
##
## **THE OTHER TWO CEILINGS KEEP THEIR OWN QUESTIONS** and must not be re-pointed at this one: a
## preset's `up to +N/turn` quotes the ROOM (a quantity takeable once) and `max_useful_workers`
## divides the ROOM (how many hands the standing stock can use).
static func expected_next_turn_yield(forecast: Dictionary, workers: int,
        band: Dictionary) -> float:
    return expected_yield_account(forecast, workers, band, "per_worker", "next_ceiling",
        FORECAST_FOOD_PER_ANIMAL_KEY)

## **THE ENGAGEMENT ARM OF THE TAKE, IN ONE ACCOUNT'S UNITS** — `workers × engageRate` animals,
## unrounded as the sim's reach is, each worth this account's per-animal quantum. That quantum IS `bodyMass ×
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
        float(forecast.get("stay", STAY_FRACTION_NONE_BREAKS_OFF)))

## The same arm with its quantum handed in rather than looked up — the form the CHART's projection
## needs, whose quantum is `bodyMass` (the curve, the room and the throughput are all biomass there)
## rather than an account's `*_per_animal`. `engagement_reach` is this function reading a forecast, so
## the sheet's take and the chart's projection bound themselves on ONE definition.
##
## **IT BOUNDS THE ANIMAL COUNT AND THEN CONVERTS**, never the other way about: `animals_engaged`
## answers in ANIMALS, the retreat cuts that count, and only then does the quantum value it. Converting
## first would price a reach in one account's units and then cut it in another's — and the whole-animal
## quantisation, which is the take's and not the reach's, would have nothing left to round.
## **THE RETREAT IS APPLIED HERE AND NOT ONE STAGE EARLIER**, in the sim's own order — engage, retreat,
## then convert — so `stay` cuts the whole-animal count the quantum then values. It defaults to the
## wire's "nothing breaks off", which is what leaves a pen, the plant web and every source that
## publishes no retreat byte-identical to before the stage existed.
static func engaged_quantum(workers: int, per_animal: float, engage_rate: float,
        stay: float = STAY_FRACTION_NONE_BREAKS_OFF) -> float:
    if per_animal <= 0.0:
        return ENGAGEMENT_UNBOUNDED
    return animals_stayed(animals_engaged(workers, engage_rate), stay) * per_animal

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
        # HEADLINE the row with the STEADY realized average, never the lumpy pulse. `realized_yield` is
        # the honest long-run average of this source's `actual_yield`, so BOTH hunt and forage read it:
        # forage's realized ≈ its old `actual` (no visible change), while hunt switches off the
        # `sustainable` ceiling to the true realized average — which is what makes the row (and the
        # Food-line income these rows sum into) steady. The pulse's overdraw is still carried by
        # the ⚠ flag + tooltip. Falls back to the old sustainable/actual split if `realized_yield` is
        # absent (older snapshot).
        #
        # **IT IS RESOLVED BEFORE THE TOOLTIP NOW, because the tooltip NAMES IT.** The two are one
        # call's `rate` and `tooltip` keys and are the same source's two quantities, so a tooltip
        # composed above the headline could only quote the other one anonymously — which is exactly
        # what it did.
        if m.has("realized_yield"):
            rate = float(m["realized_yield"])
        else:
            rate = sustainable if kind == LABOR_KIND_HUNT else actual
        tooltip = YIELD_TOOLTIP_RATES_FORMAT % [format_signed(rate), format_signed(actual)]
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
## "Keepers: A / N" row is SHOWN on the SAME field and the SAME `> 0` test, so the two
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

## **THE SAME QUANTITY WITH THE SPECIES LEFT OFF** — `≈15`, for the SECOND figure in a pair whose
## first has already named what is being counted: the floor flag's `leave 50% · ≈11 Red Deer ↑ ≈15 at
## Corralled` states one threshold at two standings, and repeating the quarry between them reads as
## two different animals rather than one herd at two ceilings.
##
## **IT IS A SECOND DECORATION, NEVER A SECOND COUNT.** `animal_count` is still the one place biomass
## becomes a head count, so this cannot drift from `stock_face` in the way that matters; only the
## trailing noun differs. A patch has no body and reads identically through both — biomass names no
## species either way.
const STOCK_ANIMALS_UNQUALIFIED_FORMAT := "≈%d"
static func stock_face_unqualified(stock: float, body_mass: float) -> String:
    var animals := animal_count(stock, body_mass)
    if animals != ANIMAL_COUNT_NONE:
        return STOCK_ANIMALS_UNQUALIFIED_FORMAT % animals
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
            # **THE RAW SHARE RIDES BESIDE THE ROUNDED PERCENT, and the two are not interchangeable.**
            # `percent` is a DISPLAY figure — rounded, with the remainder folded into the first row so
            # the column sums to 100 — and arithmetic done on it inherits that fold. This is the wire's
            # own fraction, and it is what `selection_rates` weights the per-species conversion rates
            # by: a rate composed off the display percent would be off by the fold on every tile.
            "share": float(entry.get("share", 0.0)),
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
            # **WHAT SOWING THIS CROP WOULD COST, in work units** (`docs/plan_standing_upkeep.md`
            # §4.15) — the one figure on this entry that is not a payoff, and the COST half of the
            # crop decision. A Sow is priced by how much of the tile the chosen crop still has to
            # replace, so the patch's own `field_work_cost` prices exactly ONE crop (its commitment,
            # or the rung's auto-pick) while every other row of a crop list quoted that same number.
            #
            # **PRESENCE IS ITS OWN KEY, and here it means *this plant cannot climb to a Field on
            # this ground*** — the sim omits the figure rather than publishing a `0`, the multiplier
            # being floored precisely because laying the rows and putting the seed in costs work on
            # any ground. A missing-means-zero reading would advertise a free Sow for a job that
            # cannot be ordered at all.
            #
            # ⛔ **IT IS NEITHER DERIVED FROM `share` NOR A DERIVATION OF IT.** A committed patch's
            # published `share` is its REWEIGHTED one while this is struck on the tile's own basket,
            # which is what the sim charges against; the two are deliberately different questions.
            "sow_work_cost": float(entry.get("sow_work_cost", 0.0)),
            "has_sow_work_cost": entry.has("sow_work_cost"),
            # WHAT THIS PLANT IS FOR — the sim's own display tag ("staple"/"fodder"/"cash"), carried
            # so the tile card's basket rows can lead with a role icon. **`""` is UNSTATED and must
            # stay `""`**: defaulting a missing tag to "staple" would invent a fact, and re-deriving
            # one from the payoffs above is wrong twice over — they are rung-2/rung-3 numbers folding
            # in the weeding and conversion gains, and they read all-zero for a species that cannot
            # climb on this ground, which is exactly where the role is still true and useful.
            "role": String(entry.get("role", "")).strip_edges().to_lower(),
            # **HOW MUCH OF THIS PLANT IS STANDING** (`ForagePatchState.compositionStandingBiomass`,
            # folded onto the entry by the decoder) — carried through so the compose sheet's species
            # chips can quote a QUANTITY. A selective gather asks *"is there enough emmer here to be
            # worth two hands"*, and the client holds no capacity arithmetic: the sim states this
            # exactly as it states the take.
            #
            # **PRESENCE IS ITS OWN KEY, because `0.0` is a real reading** — a stand drawn to nothing
            # is not the same fact as a server that quoted no biomass at all, and only the second may
            # render no clause. It is also the ONE producer of the quantity: the tile card's basket
            # rows read this same number rather than re-deriving `share × stock`.
            "standing_biomass": float(entry.get("standing_biomass", 0.0)),
            "has_standing_biomass": entry.has("standing_biomass"),
            # **WHAT ONE UNIT OF THIS PLANT CONVERTS AT**
            # (`ForagePatchState.compositionProvisionsPerBiomass` and its fodder twin, folded onto the
            # entry by the decoder) — the patch's standing rung, the favored crop's gain already in,
            # and NOT pre-scaled by the share above. They are what let the compose sheet price a
            # NARROWING live: `provisionsPerBiomass` on the patch is the basket AVERAGE, so a sheet
            # holding only that quoted the same number however many chips were ticked.
            #
            # **PRESENCE IS ITS OWN KEY HERE TOO, and here it earns its keep twice over**: a cash crop
            # honestly pays `0.0` food, so a missing-means-zero reading would make an unstated rate and
            # a real one indistinguishable on exactly the plants this feature is about.
            "provisions_per_biomass": float(entry.get("provisions_per_biomass", 0.0)),
            "has_provisions_per_biomass": entry.has("provisions_per_biomass"),
            "fodder_per_biomass": float(entry.get("fodder_per_biomass", 0.0)),
            "has_fodder_per_biomass": entry.has("fodder_per_biomass"),
            # **…AND WHAT ONE UNIT OF IT IS MADE OF** (`compositionMaterialPerBiomass[i].rows`, the
            # same seam a third time). It is the account the whole selective gather was argued on:
            # baskets are made of fibre and baskets are what let a gatherer carry more food, so
            # *tick cotton, see how much fibre* is the first thing a player tries, and
            # `material_per_biomass` on the PATCH is basket-averaged and cannot answer it.
            #
            # **AN EMPTY LIST IS "NO ROW", NEVER ZERO** — a grain pays no material and says so — which
            # is why presence still needs its own key beside it: an entry the wrapper vector never
            # reached is a server that stated nothing, and only that one makes the selection
            # unquotable. Carried VERBATIM; `selection_rates` composes it per MATERIAL ID and nothing
            # anywhere sums it into one materials/turn figure.
            "material_per_biomass": material_payoff_rows(entry.get("material_per_biomass", [])),
            "has_material_per_biomass": entry.has("material_per_biomass"),
        })
    if entries.is_empty():
        return entries
    entries[0]["percent"] = int(entries[0]["percent"]) + FLORA_SHARE_PERCENT_TOTAL - total
    return entries

# ---- THE SELECTIVE GATHER — pricing a NARROWED take ---------------------------------------------
#
# A forage crew may name the plants it carries home. The patch's own `provisions_per_biomass` is the
# BASKET AVERAGE, so a sheet holding only that quotes the same take however many chips are ticked —
# live for the worker stepper beside it and inert for the control that is the whole decision. The wire
# answers that with two per-species vectors folded onto the basket entries
# (`flora_basket_entries`' `provisions_per_biomass` / `fodder_per_biomass`), and these two functions
# are the ONE place this client composes them.
#
# **THE COMPOSITION, and every term of it comes off the wire:**
#
#     available = max(0, biomass − floor·K) × Σ_S share        <- the selected plants' stand
#     rate      = Σ_S (share × rate) ÷ Σ_S share               <- the rate WITHIN the selection
#     take      = min(workers × perWorkerBiomass, available) ; food = take × rate
#
# **`narrowed_source` EXPRESSES ALL OF IT AS A SOURCE DICT rather than as a second take model**, which
# is what keeps the narrowed sheet and the whole-basket sheet one piece of code: the stand, the rates
# and the crew throughput are substituted, and `forecast_inputs` / `max_useful_workers` /
# `expected_yield_account` / `hold_crew` / `reach_crew` / the chart then answer for the selection
# through the identical arithmetic. A narrowed take that had its own model would drift from the
# whole-basket one the first time either moved — and the `now → after` walk, which is exactly what the
# player must see move when a chip is ticked, would have had to be written twice.

## The keys of a composed selection: is it QUOTABLE at all, and the three accounts if so. The material
## one is a `[{material_id, amount}]` VECTOR like every other material reading in this file — never a
## scalar, which is the retired trade axis under a new name.
const SELECTION_KNOWN := "known"
const SELECTION_SHARE := "share"
const SELECTION_PROVISIONS := "provisions"
const SELECTION_FODDER := "fodder"
const SELECTION_MATERIAL := "material"

## **AND WHY IT IS UNQUOTABLE, BECAUSE THERE ARE TWO REASONS AND THEY HAVE DIFFERENT REMEDIES.** The
## key is meaningless on a QUOTED selection and is present regardless, so a reader never has to know
## which arm produced the dict.
##
## `SELECTION_REASON_UNPRICED` — the wire priced no per-species rate for something the player ticked.
## Nothing the player can do fixes it; the sheet says so and quotes nothing.
##
## `SELECTION_REASON_ABSENT` — the ticked plants are not in this tile's basket at all, or carry no
## share of it. **That state has an obvious remedy the unpriced sentence never mentions** — tick a
## plant that grows here — and it is reachable through a roster change or a natural composition shift
## that drops a plant a standing selection still names. (A commitment now prunes a crew's stale
## `take_species`, which is what made it rare rather than what made it impossible.)
##
## **A CASH CROP PAYING `0.0` IS NEITHER OF THESE.** It is fully quoted, and `yield_rows`' own
## render-where-it-pays rule decides whether a FOOD row exists at all.
const SELECTION_REASON := "reason"
const SELECTION_REASON_UNPRICED := "unpriced"
const SELECTION_REASON_ABSENT := "absent"

## Compose `selection` (species keys, EMPTY meaning the whole basket) against `basket` —
## `flora_basket_entries`' answer for the same tile.
##
## **`known` IS FALSE WHERE THE WIRE STATED NO RATE FOR SOMETHING THE PLAYER TICKED**, and that is the
## honest-silence case: this client cannot recover a per-species conversion from anything else it
## holds, so the sheet says the narrowing is unquoted rather than quoting it at the basket's numbers
## under a narrowed heading — the quote-vs-payout defect this arc has shipped before.
##
## **A `0.0` RATE IS NOT THAT CASE.** A cash crop pays no food and says so; the selection is fully
## quoted, its food rate is zero, and `yield_rows`' own render-where-it-pays rule then decides whether
## a FOOD row exists at all. Presence travels on the entry's `has_*` key for exactly this reason.
##
## An empty selection returns `known = false` as well — there is nothing to narrow, and the caller
## reads the patch unchanged.
static func selection_rates(basket: Array[Dictionary],
        selection: PackedStringArray) -> Dictionary:
    var unquoted := {SELECTION_KNOWN: false, SELECTION_SHARE: 0.0,
        SELECTION_PROVISIONS: 0.0, SELECTION_FODDER: 0.0,
        SELECTION_MATERIAL: ([] as Array[Dictionary]),
        SELECTION_REASON: SELECTION_REASON_ABSENT}
    if selection.is_empty() or basket.is_empty():
        # Nothing ticked, or a tile whose basket the wire does not describe. Neither is a NARROWING,
        # so no caller renders an aside for it — the reason stands at its default rather than
        # claiming the wire priced something badly.
        return unquoted
    var share_sum := 0.0
    var provisions := 0.0
    var fodder := 0.0
    # **THE MATERIAL ARM MERGES BY ID, and that is the half a single-species fixture cannot check.**
    # Two ticked plants both paying `fibre` compose into ONE fibre rate — which is what a rate means,
    # and what the store sums the same way — so the weighted sums accumulate into a per-id map rather
    # than a list. A last-write-wins composition passes a one-plant selection and is wrong by a factor
    # on cotton beside flax. Insertion ORDER is the basket's (the wire's), kept in `material_order`,
    # so the rendered rows do not reshuffle between renders.
    var material_weighted := {}
    var material_order: Array[String] = []
    var matched := 0
    for entry in basket:
        if not selection.has(String(entry.get("species", ""))):
            continue
        # A plant the wire quoted no rate for makes the WHOLE selection unquotable: the composition is
        # a weighted mean, so one missing term is not a term that can be left out of it. **The
        # material arm is gated on PRESENCE, never on emptiness** — an empty row list is a plant that
        # pays no material, which is a real answer and composes as a zero contribution.
        if not bool(entry.get("has_provisions_per_biomass", false)) \
                or not bool(entry.get("has_fodder_per_biomass", false)) \
                or not bool(entry.get("has_material_per_biomass", false)):
            # **THIS PLANT IS IN THE BASKET AND THE WIRE PRICED IT BADLY** — the one arm that is
            # genuinely about the SERVER rather than about the selection, so it is the one arm that
            # overwrites the default reason.
            unquoted[SELECTION_REASON] = SELECTION_REASON_UNPRICED
            return unquoted
        var share := maxf(float(entry.get("share", 0.0)), 0.0)
        matched += 1
        share_sum += share
        provisions += share * float(entry.get("provisions_per_biomass", 0.0))
        fodder += share * float(entry.get("fodder_per_biomass", 0.0))
        for row in (entry.get("material_per_biomass", []) as Array):
            var material_id := String((row as Dictionary).get(MATERIAL_PAYOFF_ID_KEY, ""))
            if material_id == "":
                continue
            if not material_weighted.has(material_id):
                material_weighted[material_id] = 0.0
                material_order.append(material_id)
            material_weighted[material_id] = float(material_weighted[material_id]) \
                + share * float((row as Dictionary).get(MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
    # A selection naming nothing this tile grows, or naming only plants the tile carries no share of,
    # divides by zero — there is no stand to price and no mean to take. **The remedy is the player's**
    # (tick a plant that grows here), which is why this reason is worded apart from the unpriced one.
    if matched == 0 or share_sum <= 0.0:
        return unquoted
    # **THE DENOMINATOR IS THE WHOLE SELECTION'S SHARE, on every material.** A plant that pays no fibre
    # contributes a zero to the fibre mean rather than leaving the mean to the plants that do — the
    # selection is one crew gathering one stand, and dividing each material by only its own payers
    # would quote a narrowing to `flax + oak mast` the same fibre rate as `flax` alone.
    var materials: Array[Dictionary] = []
    for material_id in material_order:
        materials.append({
            MATERIAL_PAYOFF_ID_KEY: material_id,
            MATERIAL_PAYOFF_AMOUNT_KEY: float(material_weighted[material_id]) / share_sum,
        })
    return {
        SELECTION_KNOWN: true,
        SELECTION_SHARE: share_sum,
        SELECTION_PROVISIONS: provisions / share_sum,
        SELECTION_FODDER: fodder / share_sum,
        SELECTION_MATERIAL: materials,
    }

## The patch as the SELECTED plants alone — a copy of `src` with the selection folded into the terms
## every take reading is composed from. Returns `src` untouched for an unquotable selection, so a
## caller that forgot to check still renders the whole basket rather than a scaled ghost of it.
##
## **THE STAND SCALES AND THE CREW DOES NOT**, which is the composition above written into a dict:
## `biomass`, `carrying_capacity` and the regrowth curve all take the selection's share (so
## `escapement_room` returns `Σshare × (B − floor·K)` exactly, and the stock FRACTION `B/K` the curve
## and the chart are read at is untouched), while `per_worker_biomass` stays whole — a worker's basket
## does not shrink because they walk past the flax.
##
## **`per_worker_yield` IS RE-COMPOSED, NOT SCALED.** The wire's is `perWorkerBiomass × basket rate`;
## this crew converts at the SELECTION's rate, so it is multiplied out again from the throughput
## rather than nudged. It is substituted BEFORE `KitRoster.repriced_source` runs, so the kit's carry
## ratio still lands on it exactly as it lands on the whole-basket figure.
##
## **AND THE MATERIAL VECTORS ARE SUBSTITUTED THE SAME WAY, per material id.** `material_per_biomass`
## takes the selection's composed rate vector; `per_worker_material` is RE-COMPOSED off the throughput
## exactly as `per_worker_yield` is, because the wire's is that throughput at the basket's rate. An
## empty answer stays empty and renders NO material row (`yield_rows`' "empty means no row" rule) —
## which is what a selection of pure grain honestly is, not a column of zeros.
static func narrowed_source(src: Dictionary, prefix: String, rates: Dictionary) -> Dictionary:
    if not bool(rates.get(SELECTION_KNOWN, false)):
        return src
    var share := clampf(float(rates.get(SELECTION_SHARE, 0.0)), 0.0, 1.0)
    var out := src.duplicate()
    out[prefix + FORECAST_BIOMASS_KEY] = float(src.get(prefix + FORECAST_BIOMASS_KEY, 0.0)) * share
    out[prefix + FORECAST_CAPACITY_KEY] = float(src.get(prefix + FORECAST_CAPACITY_KEY, 0.0)) * share
    var samples := regrowth_samples(src, prefix)
    var scaled := PackedFloat32Array()
    for sample in samples:
        scaled.push_back(sample * share)
    out[prefix + FORECAST_REGROWTH_SAMPLES_KEY] = scaled
    var provisions := float(rates.get(SELECTION_PROVISIONS, 0.0))
    out[prefix + FORECAST_PROVISIONS_PER_BIOMASS_KEY] = provisions
    out[prefix + FORECAST_FODDER_PER_BIOMASS_KEY] = float(rates.get(SELECTION_FODDER, 0.0))
    var carry := per_worker_biomass(src, prefix)
    out[prefix + FORECAST_PER_WORKER_KEY] = carry * provisions
    var materials: Array[Dictionary] = rates.get(SELECTION_MATERIAL, [] as Array[Dictionary])
    out[prefix + FORECAST_MATERIAL_PER_BIOMASS_KEY] = materials
    out[prefix + FORECAST_PER_WORKER_MATERIAL_KEY] = scaled_material_rows(materials, carry)
    return out

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

## **THE MATERIAL ARM OF A PER-TURN READOUT** — every material this source pays, SIGNED, joined in the
## sentence idiom (`+0.22 hide`), and `""` when there is nothing to say. The empty answer is the whole
## point: it is the gate a fall-through tests, so "this source pays no material" and "this source pays
## a material" are one call rather than a condition each caller re-derives.
##
## **EVERY material, never the first one, and never a count of the rest.** Picking one of a vector
## names a winner the sim does not name; summing them is the retired trade axis under a new name. Both
## callers size to their measured run rather than clipping — the map's on-tile plate, whose pill is
## drawn to the text's own width, and the work board row's ACCOUNTS LINE, which has the whole row.
##
## **A BOUNDED FORM (`+0.24 fibre +3`) EXISTED AND IS RETIRED, and the reason is worth keeping.** A
## `Label` with no overrun behaviour reports its WHOLE text as its minimum width, so an unbounded join
## does not merely run long — it sets the ROW's minimum, and in a fixed-width zone that lays every row
## out past the box while the zone's `clip_contents` slices the right edge off all of them (measured:
## 528px of a 356px box on a four-cash-crop patch, with the row's name allocated Godot's 1px floor).
## The cap was what left a bounded, meaningful string behind the ellipsis in a **46px** rate column.
## That column is gone — the board row's accounts moved to a full-width second line
## (`band-city-panel.md` → "THE ROW IS TWO LINES") — so **no caller has one fixed slot any more**, and
## the bound was deleted rather than left parameterised: an unreachable cap is a thing the next reader
## assumes is load-bearing.
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

## **WHAT A CREW OF `workers` TAKES, PER MATERIAL** — `min(workers × per_worker, material_ceiling)`
## evaluated once per material. `forecast` is a `forecast_inputs` answer.
##
## > #### ⛔ IT IS THE FOOD SIDE'S OWN CLAMP **ON THE PLANT WEB ONLY** — the animal web has three more arms
## >
## > This docstring used to claim the clamp was "the food side's own clamp applied one account further
## > out" without qualification, and that false premise WAS a defect. On a PATCH it holds: the food
## > row is `min(workers × per_worker, ceiling)` against its own matching ceiling, there is no
## > engagement stage and no whole-animal quantum, so both accounts are linear in workers and track by
## > construction. On a HERD the food row is `min(room, crew carry, engagement→retreat) × the per-body
## > carry clamp`, quantised to whole bodies — four bounds, of which this expression has one — so a
## > pastoral Wild Boar herd (`engage_rate` 0.33, whose reach was floored back then and so pinned at
## > exactly ONE animal reached for every crew from one to six) quoted five herders
## > five times the bone and hide one herder brings home, beside a food row that correctly did not
## > move. The sim credits BOTH accounts off one `take.carried` (`systems/labor.rs`), so quote and
## > payout provably disagreed.
## >
## > **The animal web does not use this any more.** A hunt's material rows cross out of the delivered
## > biomass through `rescaled_accounts`, beside the food and fodder ones. The one animal-web caller
## > left is the INEDIBLE quarry, whose food axis is a structural zero — there is no carried biomass to
## > cross from, and its `body_mass` is not a term any client-side quantiser here can reach — so it is
## > still quoted on crew throughput alone and is still over-quoted at a crew the engagement bound
## > pins. Closing that needs the quantiser expressed in BIOMASS, which needs a `body_mass` such a herd
## > publishes and the harness fixtures do not derive.
##
## **IT TAKES NO CEILING SELECTOR, and that is deliberate.** It had one, mirroring the pair
## `expected_yield_account` chooses between — the ROOM and the every-turn regrowth — and the hold arm
## was never reachable: its only caller passed the room at every call site, so `hold_material_ceiling`
## was computed, published on every forecast, and read by nobody. A **per-material `after` reading is
## not a thing any surface asks for** (the sheet's `now → after` is stated for food and fodder alone),
## so the selector was a supported-looking path to an answer nothing wanted — and one a caller could
## name wrong, since an unknown key reads as *every ceiling is zero*, i.e. a silent empty take. Add
## the pair back the day a surface genuinely wants the material `after`, with that surface as its
## caller.
##
## **The two vectors are unioned by id, never zipped by position.** They come off the same species so
## they list the same materials today, but a missing term must read as `0` — a rate with no ceiling is
## a herd standing at its floor, which takes nothing and correctly renders no row.
## The two material rooms `expected_materials` can clamp against — the standing one and the forward
## one. Named because a mistyped key clamps every material to zero, i.e. renders a source that pays
## no materials at all.
const MATERIAL_CEILING_ROOM_KEY := "material_ceiling"
const MATERIAL_CEILING_NEXT_TURN_KEY := "next_material_ceiling"

## `ceiling_key` names WHICH room the clamp is against — the standing one by default, and the
## forward `next_material_ceiling` for the readout's headline, so the material rows and the two
## scalars beside them describe the same TURN. An unknown key would clamp everything to zero, which
## is why the two spellings are the callers' own constants rather than hand-typed strings.
static func expected_materials(workers: float, forecast: Dictionary,
        ceiling_key: String = MATERIAL_CEILING_ROOM_KEY) -> Array[Dictionary]:
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
    # **WHAT THE PARTY BRINGS HOME BESIDES MEAT**, read before every branch below because two of them
    # turn on whether it is empty. `delivered_material` is the trip's whole material payload, and on an
    # inedible quarry it is the trip's whole payload full stop.
    var materials := material_payoff_rows(estimate.get(TRIP_DELIVERED_MATERIAL_KEY, []))
    var lands_material := false
    for row in materials:
        if has_component(float(row[MATERIAL_PAYOFF_AMOUNT_KEY])):
            lands_material = true
            break
    # A DENIAL mission carries nothing home at all. `delivers_food == false` says the QUARRY IS
    # INEDIBLE (issue #337), and Eradicate on a deer banks a whole-stock windfall like every other rung
    # rather than landing here.
    #
    # **THE TEST IS "BRINGS NOTHING HOME", AND THAT IS WHY THE MATERIAL ARM BELONGS IN IT.** Its
    # `delivers_trade` half went with the trade axis (arc #527), which left an inedible quarry reading
    # as a denial mission — false the moment the sim began projecting `delivered_material`, since a
    # wolf raid lands hides and the party is not going out to deny anything. So an inedible quarry is a
    # denial mission ONLY when it lands no material either; a raid that hauls something is a real
    # delivery whatever account that something is in.
    if not bool(estimate.get("delivers_food", false)) and not lands_material:
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
    # **"NOTHING DELIVERED" MEANS NOTHING IN ANY ACCOUNT.** A wolf's `delivered_food` is honestly `0`
    # at every party size, so a food-only test would send every material-landing raid down the
    # returns-empty branch and print a refusal at a party that is coming home loaded.
    var delivered_food := float(estimate.get("delivered_food", 0.0))
    if delivered_food <= 0.0 and not lands_material:
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
        # …and the same payload per MATERIAL, never summed into one figure. Empty for the many quarries
        # made of nothing anyone builds with, which renders no clause rather than a zero.
        TRIP_DELIVERED_MATERIAL_KEY: materials,
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

## The raid's delivered payload as a trailing " · ~20 food · ~3 hide" — each component rendered only
## when the quarry pays it, so "" when the forecast carries no payload at all. It carried a
## trade-goods scalar until arc #527 retired that account; what replaced it is the MATERIAL VECTOR,
## and on an inedible quarry it is the whole of this suffix.
##
## **THE MATERIALS ARE A PAYLOAD, SO THEY WEAR THE FOOD TERM'S OWN `~` HEDGE AND ITS WHOLE-UNIT
## ROUNDING** — this line quotes a trip, not a rate, and a `0.22` beside a `~20 food` would read as a
## per-turn number smuggled onto a per-trip line. It is the ONE place a material is rounded: every
## other material readout in the client is a rate at `YIELD_DECIMALS`.
static func _raid_payload_suffix(forecast: Dictionary) -> String:
    var suffix := ""
    var food := float(forecast.get("food", 0.0))
    if has_component(food):
        suffix += HUNT_FORECAST_FOOD_FORMAT % int(round(food))
    for row in material_payoff_rows(forecast.get(TRIP_DELIVERED_MATERIAL_KEY, [])):
        var amount := float(row[MATERIAL_PAYOFF_AMOUNT_KEY])
        if has_component(amount):
            suffix += HUNT_FORECAST_MATERIAL_FORMAT % [
                int(round(amount)), String(row[MATERIAL_PAYOFF_ID_KEY])]
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
        # …and the same haul per MATERIAL. A denial raid on an INEDIBLE quarry brings home nothing
        # else, so without this its take line stated its kills and stopped — the mission's one
        # consolation, unrendered.
        TRIP_DELIVERED_MATERIAL_KEY: material_payoff_rows(
            row.get(TRIP_DELIVERED_MATERIAL_KEY, [])),
        # The FOOD wasted — killed and left standing dead on the range. It carried a trade twin until
        # arc #527 retired that account; the sim states no per-material WASTE for a denial row, so the
        # waste clause is food alone while the haul clause above is a vector.
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
    # …and what it brings home in MATERIALS, one clause per material under the same rule. On an
    # inedible quarry this is the whole of the haul: the raid's kills used to be all its take line
    # could state, which read as a mission that destroys and salvages nothing.
    for row in material_payoff_rows(forecast.get(TRIP_DELIVERED_MATERIAL_KEY, [])):
        var amount := float(row[MATERIAL_PAYOFF_AMOUNT_KEY])
        if has_component(amount):
            text += DENIAL_TAKE_MATERIAL_FORMAT % [
                format_magnitude(amount), String(row[MATERIAL_PAYOFF_ID_KEY])]
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
## `0` for a herd with no engagement stage (a pen; a species the roster cannot resolve) and for one
## with no body to count, which is what leaves every raid predating this field byte-identical.
static func expedition_engage_crew(herd: Dictionary, floor: float) -> int:
    # A raw herd dict carries its forecast fields BARE — the `patch_` prefix belongs to the tile_info
    # cross-ref, and this layer never reads the compose vocabulary that names it.
    var prefix := ""
    return engage_workers(escapement_room(herd, prefix, floor),
        float(herd.get(prefix + FORECAST_BODY_MASS_KEY, 0.0)),
        float(herd.get(prefix + FORECAST_ENGAGE_RATE_KEY, NO_ENGAGEMENT_STAGE)),
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
## nothing carries no rate and falls back to its name + glyph. **The MATERIAL component rides beside
## it**, `delivered_material` through the same divide, which is what stopped an inedible quarry's
## presets rendering blank: the trade scalar that used to fill that slot went with arc #527, and for
## one release a wolf's rungs quoted nothing at all because the gate below tested food alone. A rung
## reaches the picker when it pays SOMETHING. An UNBOUNDED raid has no length and its travel is not
## one, so it is skipped outright rather than quoted as `delivered / travel`.
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
        # **THE MATERIALS ARE A RATE HERE, UNLIKE ON THE TRIP LINE.** This face is a per-turn metric —
        # the max obtainable over party sizes — so the trip's whole payload divides by the trip, in
        # exactly the step the food term above takes. The two spellings of one payload are the
        # register's difference, not the model's: `_raid_payload_suffix` quotes the TRIP.
        var materials := scaled_material_rows(
            material_payoff_rows(row.get(TRIP_DELIVERED_MATERIAL_KEY, [])),
            1.0 / float(trip_turns))
        # A rung reaches the picker when it pays SOMETHING — food or a material. Gating on food alone
        # left an inedible quarry's rungs blank, which is the same "worth nothing" reading the whole
        # arc removes.
        if food > 0.0 or signed_material_components(materials) != "":
            takes[String(FLOOR_PRESETS[index])] = extractive_take_pair(
                food, 0.0, zero_account, materials)
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
