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
# THE HARVEST STANCES — how hard a crew pulls on a source, and the WHOLE of the `policy` axis since
# issue #442. Shared by forage + hunt, by a resident band and a detached party alike, because the
# other axis (what is being BUILT on the source) is now its own field. Nothing filters this list any
# more: it was six on the compose sheet and four on an expedition precisely because the build verbs
# were crammed in here, and that difference is gone.
const LABOR_HUNT_POLICIES := ["sustain", "surplus", "deplete", "eradicate"]
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
# The Sustain rung by name: the default compose policy, and the ceiling every unknown rung falls back
# to in `forecast_inputs`.
const LABOR_POLICY_SUSTAIN := "sustain"
const DEFAULT_HUNT_POLICY := LABOR_POLICY_SUSTAIN
# The pen rung by name — the composed policy that makes a hunt source MANAGED before the pen exists.
const LABOR_POLICY_CORRAL := "corral"
# A herd at or above this domestication progress is fully tamed (pastoral); its crew are keepers.
const DOMESTICATION_COMPLETE := 1.0
# WHICH KIND OF SOURCE a forecast dict describes, stated explicitly by every `forecast_inputs` caller:
# a herd and a raw wire forage patch share the empty key prefix, so the prefix cannot answer it and a
# shape test (`has("hunt_policy_ceilings")`) would misread a herd whose snapshot omitted the list.
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
const YIELD_TOOLTIP_RENEWABLE := " · renewable"
const YIELD_TOOLTIP_OVERDRAW := " — overdrawing"
# Overstaffing (wasted labor) — DISTINCT from the ⚠ overdraw flag. Every policy caps a source's take at
# its ceiling (policy ceiling / resource biomass), so past `workers_needed` extra workers produce
# nothing HERE and should move elsewhere. A source can be overstaffed while perfectly sustainable (and
# overdrawn while fully used), so this reads as its own WARN-tinted note on the row rather than
# borrowing the ⚠. `workers_needed == 0` (rehydrated save) means "unknown" ⇒ no note, never a wrong one.
const OVERSTAFF_NOTE_FORMAT := " · only %d of %d working"
const OVERSTAFF_TOOLTIP := "Overstaffed — this source's yield is capped at its sustainable/policy ceiling; the extra workers produce nothing here. Reassign them to another source."
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

# ---- THE TWO PRODUCTS (issue #337) --------------------------------------------------------------
# A hunt pays a VECTOR, not a food scalar: provisions AND trade goods, per the species' own hunt-yield
# vector times the policy's intensity. THE ONE PRESENTATION RULE, applied everywhere below and by
# every caller: **render a component only when it is non-zero.** A deer reads food and trade, food
# leading; a wolf (`provisions == 0`) reads TRADE ONLY and never a "0 food" line; a forage patch reads
# food only. A `0` printed as a number for a component the species does not produce is the
# false-precision this whole arc exists to remove — it is not "more complete", it is wrong.
#
# Trade is stated GENERICALLY, with `FoodIcons.TRADE_GOODS_GLYPH` and the word "trade goods". The sim
# models a scalar; naming it per species (pelts/ivory/hide) is a deferred flavor layer.
const TRADE_COMPONENT_SEPARATOR := " · "
# The joiner for the COMPACT (magnitude-only) pair. A plain space, not `·`, because the surfaces that
# use it — the work-zone filter chips — already spend their `·` separating a count from its total, and
# a second one would read as a third field rather than a second product.
const COMPACT_COMPONENT_SEPARATOR := " "
# The bare trade rate as it rides a button face / row suffix: `⇄ +0.35`. No "/turn" — it sits beside a
# food term that already carries one, or under a tooltip that spells the unit out.
const TRADE_COMPONENT_FORMAT := "%s %s"
# The trade half of a rung's tooltip. Spelled with the generic noun so the tooltip, unlike the compact
# face, can never be misread as a second food number. Despite the `CAP` in its name this is the SHARED
# trade-rate clause: it is bare wording with no "up to" in it (unlike `POLICY_CAP_FORMAT`), so the
# INVESTMENT rungs' payoff tooltip ("builds toward …") reuses it rather than duplicating the phrasing.
const POLICY_CAP_TRADE_FORMAT := "%s %s trade goods/turn"
# The FODDER half of the same tooltip. No glyph in it, unlike the trade clause: fodder has no mark of
# its own (`FoodIcons` spends no glyph on it), and borrowing another account's would say the wrong
# thing. Like the trade clause it carries no "up to", so the investment payoff tooltip reuses it.
const POLICY_CAP_FODDER_FORMAT := "%s fodder/turn"
# The trade half of a worked row's tooltip, beside "Actual +0.31 /turn".
const TRADE_TOOLTIP_FORMAT := "Trade goods %s %s/turn"

# ---- THE PICKER FACE'S PRODUCT LINE -------------------------------------------------------------
# THE PRODUCTS IN WORDS, for the policy picker's SECOND line: `0.96 food · 0.24 trade`. The picker is
# the ONE surface that names its products rather than marking them, and the reason is that its two
# glyph families were doing incompatible jobs side by side: the POLICY glyph (♻ ⬆ ⇊ 💀) says which
# RUNG, `FoodIcons.TRADE_GOODS_GLYPH` says which PRODUCT, and adjacent in one line at one weight the
# eye cannot tell which axis it is reading. Line 1 now names the rung (`HudFormat.policy_face`), so
# line 2 names the product — and a WORD is the only mark trade goods can wear here anyway: emoji
# cannot be tinted (🪙 / 💰 / ⚖ were measured and rejected in `FoodIcons`), and the remaining
# text-presentation arrow is exactly the abstract mark that stopped reading.
# NO `+` SIGN, deliberately: every rung is a gain, so a sign on a picker face carries no information.
# It stays on the work rows and map labels, where a `+` genuinely contrasts against consumption.
# The render-only-when-non-zero rule above still governs — a wolf's rung reads `2.70 trade` alone.
const PICKER_FOOD_PRODUCT_FORMAT := "%s food"

const PICKER_TRADE_PRODUCT_FORMAT := "%s trade"

# THE THIRD ACCOUNT (#426), plant-only. **The word is the ACCOUNT's, not the commodity's** — its two
# neighbours on this line are `food` and `trade`, the names of the accounts a yield routes to, so a
# commodity noun here would read as a fourth kind of thing rather than a third account. That is why
# this says `fodder` while the crop-basket rows two lines below say `hay`
# (`HudFloraVocab.FLORA_CROP_HAY_CLAUSE_FORMAT`): a row there names one PLANT and what that plant
# pays, and hay is what hay grass pays.
const PICKER_FODDER_PRODUCT_FORMAT := "%s fodder"

## The picker face's product line for a source's yield VECTOR — `0.96 food · 0.24 trade`, `2.70 trade`
## (inedible quarry), `0.15 food` (a wild patch of a staple), `0.62 food · 0.01 trade · 0.40 fodder`
## (a tended patch carrying a hay crop). Same food-leads order and same render-only-when-non-zero rule
## as `yield_components`, in words instead of glyphs and without the sign; when EVERY component is
## absent the food zero survives, because a rung whose ceiling is honestly empty is a fact worth
## reading.
##
## **The account order is the wire's, not a ranking** — provisions, trade goods, fodder — so a tile
## that pays two of the three reads the same left-to-right whichever two they are, and the eye can
## find an account by position rather than by re-reading the words.
static func picker_products(food: float, trade: float, fodder: float = 0.0) -> String:
    var parts: Array[String] = []
    if has_component(food) or not (has_component(trade) or has_component(fodder)):
        parts.append(PICKER_FOOD_PRODUCT_FORMAT % format_magnitude(food))
    if has_component(trade):
        parts.append(PICKER_TRADE_PRODUCT_FORMAT % format_magnitude(trade))
    if has_component(fodder):
        parts.append(PICKER_FODDER_PRODUCT_FORMAT % format_magnitude(fodder))
    return TRADE_COMPONENT_SEPARATOR.join(parts)

# WHICH COMPONENT A SPECIES ACTUALLY PAYS — the client mirror of the sim's `ratio_axis()`: the first
# component with a POSITIVE rate, provisions preferred so every edible species divides exactly as it
# did before this arc, trade for an inedible one. Everything that would otherwise DIVIDE BY A
# PER-ANIMAL QUANTUM (the kill rhythm, the carry-aware delivered take, the averaging window, the
# whole-animal worker cap) picks its axis here — a wolf's `food_per_animal` is honestly 0, so a
# food-only derivation divides by zero and silently produces nothing.
const YIELD_AXIS_PROVISIONS := "provisions"
const YIELD_AXIS_TRADE := "trade"

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
# The species-aware per-worker TRADE rate + one-animal trade quantum (issue #337). Herd-only: a forage
# patch carries neither, so a plant forecast stays food-denominated and the axis resolves to provisions.
const FORECAST_PER_WORKER_TRADE_KEY := "per_worker_trade"
const FORECAST_FOOD_PER_ANIMAL_KEY := "food_per_animal"
const FORECAST_TRADE_PER_ANIMAL_KEY := "trade_per_animal"
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
# The TRADE half of that same payoff (issue #337's render-only-when-non-zero rule, reaching the
# investment rungs): a prepared source pays a VECTOR, so a boar's Tame reads `→ food · trade`.
# **ALL FOUR RUNGS, both webs** — the plant pair joined the herd pair in #426. The claim that gated
# them out ("the plant web projects no trade rate at all") described a wire that no longer exists: a
# tended patch of flax pays trade and no food, and quoting it as `→ 0.00 food` said the rung was
# worthless. These are the patch's SPECIES-BLIND quotes, which is the right answer for a committed
# patch and the fallback for an uncommitted one — the per-crop substitution is the caller's
# (`DrawerComposeController._flora_entry_trade_payoff`).
const FORECAST_PAYOFF_TRADE_KEYS := {
    "corral": "corral_trade",
    "tame": "pastoral_trade",
    "cultivate": "tended_trade",
    "sow": "field_trade",
}
# The FODDER half. **PLANT RUNGS ONLY, and that asymmetry is structural rather than pending work:**
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
# THE per-policy forage rows (#426), keyed by the same policy strings and decoded in one pass from the
# single wire list, so no two accounts can drift. The row carries BOTH halves of
# `min(workers × per_worker, ceiling)` because on the plant web `Deplete` marks trade up and the sim
# applies that markup AFTER the worker cap — so a per-worker term without it reads low by the full
# multiplier the moment labor binds. The markup is already folded in server-side; nothing here applies one.
const FORAGE_ROW_CEILING := "forage_policy_ceilings"
const FORAGE_ROW_CEILING_TRADE := "forage_policy_trade_ceilings"
const FORAGE_ROW_CEILING_FODDER := "forage_policy_fodder_ceilings"
const FORAGE_ROW_PER_WORKER := "forage_policy_per_worker"
const FORAGE_ROW_PER_WORKER_TRADE := "forage_policy_per_worker_trade"
const FORAGE_ROW_PER_WORKER_FODDER := "forage_policy_per_worker_fodder"
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
# The expedition sub-case where freeing idle workers WOULD NOT help: the party-size cap binds
# (idle >= max party), so the advice is wrong — say we're at the party limit instead.
const PARTY_SIZE_BOUND_NOTE_FORMAT := "%d of %d useful — at the max party size"

# The herd's two sim-exported estimate tables. The BAND ceiling list is the herd's renewable per-turn
# FLOW for a resident hunt; the TRIP estimate table is the forward-simulated raid answer. The client
# does ZERO arithmetic over either — a re-derived `carryCap / rate` closed form is wrong, and wrong by
# a lot (on a FULL Rabbit Warren under Surplus only a LONE hunter fills at all). Look it up.
const HERD_BAND_CEILINGS_KEY := "hunt_policy_ceilings"
# The TRADE twin of that list (issue #337) — the same policy keys, the ceiling in trade goods/turn.
# Two dicts rather than one dict of pairs because the decoder fills both in a single pass over the one
# wire list, so they cannot drift, and every existing food-only reader stays untouched.
const HERD_BAND_TRADE_CEILINGS_KEY := "hunt_policy_trade_ceilings"
const HERD_TRIP_ESTIMATES_KEY := "hunt_trip_estimates"
# `hunt_trip_estimates` is keyed "<policy><sep><party_workers>" — the sim's key format, mirrored by
# `hunt_estimate_key` so the single-cell lookup and the whole-row scan can never disagree on it.
const HUNT_ESTIMATE_KEY_SEPARATOR := ":"
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
# forecast horizon still delivering (a slow breeder a big party can neither fill nor exhaust). The
# client has no horizon lever, so it words this "over many turns" rather than a bare number.
const HUNT_FORECAST_LONG_RAID_FORMAT := "delivers ≈%d %s over many turns"
# The FOOD the delivered animals are worth, appended so the party-size tradeoff reads BOTH ways: a
# bigger party takes more animals AND more food.
const HUNT_FORECAST_FOOD_FORMAT := " · ~%d food"
# Its TRADE twin (issue #337), appended AFTER the food term when the quarry pays both and standing
# alone when it is inedible. The generic noun, never a per-species one — the sim ships a scalar.
const HUNT_FORECAST_TRADE_FORMAT := " · %s ~%d trade goods"
# A finite raid past the band's `expedition_viability_warn_turns` — it still delivers, just slowly. A
# real tradeoff (told, then trusted), so the line stays WARN-amber and the button stays enabled.
const HUNT_FORECAST_SLOW_SUFFIX := " — a slow raid"
# Travel is NOT in `turnsToFill` — that now counts HUNTING turns only (once the party is in reach). The
# round trip out to the herd and back is band-relative (the per-herd estimate table is band-agnostic, so
# it cannot carry it), so the client adds it: ceil(2 × wrap-aware hex_distance(band, herd) /
# band_move_tiles_per_turn), the SAME formula the server's launch feed uses. When travel > 0 the headline
# turns is the TOTAL and this breakdown spells the split out; when 0 the headline is just the hunting turns.
const HUNT_FORECAST_TRAVEL_BREAKDOWN := " (%d hunting + %d travel)"
# The long-raid line has no bounded hunting-turn count ("over many turns"), so travel rides as a trailing
# "(+T travel)" rather than a two-part split.
const HUNT_FORECAST_LONG_TRAVEL_SUFFIX := " (+%d travel)"
# The ONE non-viable case under the raid model: the herd is at/below the policy's floor, so there is no
# standing surplus to raid and the party would return empty. NOT a "won't fill" verdict (the raid always
# completes); the herd simply has nothing to give this policy right now.
const HUNT_FORECAST_NO_SURPLUS_FORMAT := "%s is too lean to raid — its surplus is spent"
# A DENIAL mission is a raid with NO PAYLOAD IN EITHER CURRENCY, not a failed one. It is no longer
# "Eradicate": since issue #337 `delivers_food` says the QUARRY IS INEDIBLE, Eradicate banks a
# whole-stock windfall like every other rung, and an inedible quarry still pays pelts. The sim decides
# this — `delivers_food == false AND delivers_trade == false` — and the client never infers it from the
# policy string.
const HUNT_FORECAST_DENIAL_FORMAT := "%s — denial mission: hunts the herd toward extinction, brings nothing home"
const HUNT_FORECAST_WARN_GLYPH := "⚠ "
# When a kill can't be fully carried (a big animal the crew is too small to haul) the surplus meat rots.
# A WARN-tinted suffix flags the fraction wasted — its OWN concern, rendered amber even on a green line.
const HUNT_WASTE_SUFFIX_FORMAT := " · ⚠ %d%% wasted"

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
# but the button names it a long haul rather than quoting a turn count the client can't bound.
const SEND_HUNT_LONG_RAID_BUTTON := "Send Anyway (long raid)"
# The ONE blocked case: the herd has no surplus above the policy's floor. A raid that returns empty is a
# mistake with no upside (unlike a slow-but-delivering one), so the button is DISABLED and says why +
# the way out. Party size can't fix it — surplus is a property of the HERD, not the party — so the
# reason names no alternative size.
const SEND_HUNT_NO_SURPLUS_BUTTON := "Herd too lean to raid"
const SEND_HUNT_NO_SURPLUS_REASON := "%s has no surplus above this policy's floor — the raid would return empty. Wait for the herd to rebuild, ease the policy, or hunt it locally."
# A denial raid's button states the deal rather than implying failure — the mission IS the point. It
# is the quarry that decides this (pays neither product), not the rung: see HUNT_FORECAST_DENIAL_FORMAT.
const SEND_HUNT_DENIAL_BUTTON := "Send (brings nothing home)"

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

## The bare trade rate with its glyph — `⇄ +0.35`. The ONE way this client writes a trade number.
static func format_trade(value: float) -> String:
    return TRADE_COMPONENT_FORMAT % [FoodIcons.TRADE_GOODS_GLYPH, format_signed(value)]

## True when a rate is a real quantity rather than the absence of one. The gate every
## render-only-when-non-zero decision goes through, so "is this component present?" is answered
## identically for food, trade and fodder and by every surface.
##
## **ITS FLOOR IS THE DISPLAY'S, NOT THE MODEL'S** (#426), and the two are different numbers. This
## used to read `>= FOOD_FLOW_MIN` (0.001) — the *food-flow* floor, which is a claim about the
## simulation — while every caller renders at `YIELD_DECIMALS` (2). A rate in between therefore
## PASSED the gate and then printed as `0.00`: a single forager on a staple patch earns ~0.003 trade
## goods a turn, and the preview line duly read `+0.08 /turn · ⇄ +0.00 · 0.13 fodder`. That zero is
## exactly the false precision the render-only-when-non-zero rule exists to remove — the gate was
## letting through the very thing it was written to stop, because it was measuring in different units
## from the formatter it gates.
##
## `FOOD_FLOW_MIN` stays where it is and keeps its own job: whether the BAND has a food flow at all
## is a question about the sim, not about how many decimals a label shows.
static func has_component(rate: float) -> bool:
    return rate >= COMPONENT_RENDER_MIN

## THE ONE DEFINITION of a worked source's trade rate, read off a labor-assignment / worker-map dict.
##
## **THE SENTINEL IS THE VALUE `0`, NOT AN ABSENT KEY — and getting that wrong made every FORAGE
## source's trade invisible.** `realized_trade_yield` is the steady forward-projected rate, and it is
## `0.0` on every forage source by design (the plant web's trade PROJECTION is a documented sim-side
## gap, `core_sim/src/forage.rs` `PLANT_TRADE_FORECAST_NOT_YET_PROJECTED`, which says in as many words
## that it is "a KNOWN GAP, not a claim that plants sell nothing"). The trade a gather ACTUALLY earned
## ships beside it in `trade_yield`.
##
## Both readers used to spell the fallback as `has("realized_trade_yield") ? … : trade_yield`, which
## is **dead code**: `native/src/dict/population.rs` inserts the key UNCONDITIONALLY, so `has()` is
## always true on live data and the `0.0` sentinel won every time. A cash-crop patch selling 0.04
## trade/turn therefore rendered `+0.00` — the exact reading the sentinel's own comment warns against.
## Testing the VALUE is what makes the fallback fire. Trade income is never negative, so `> 0` is a
## complete test for "the projection has something to say".
static func trade_rate_of(source: Dictionary) -> float:
    var realized := float(source.get("realized_trade_yield", 0.0))
    return realized if realized > 0.0 else float(source.get("trade_yield", 0.0))

## THE RENDER-ONLY-WHEN-NON-ZERO JOINER for a per-turn readout: `+0.31 /turn · ⇄ +0.12` (both),
## `+0.31 /turn` (food only), `⇄ +0.12` (trade only — a wolf), `+0.08 /turn · 0.40 fodder` (a hay
## meadow). One definition, so every surface that states a source's per-turn products states them the
## same way and none of them can print a zero for a component the source does not produce. Food leads.
## When EVERY component is absent the food zero survives ("+0.00 /turn"): a worked source that
## produced nothing this turn is a fact worth reading.
##
## The fodder term wears the WORD, not a glyph, because fodder has none — the same reason
## `picker_products` names its accounts. It is plant-only, so every hunt-side caller leaves it
## defaulted and reads exactly as it did.
static func yield_components(food: float, trade: float, fodder: float = 0.0) -> String:
    var parts: Array[String] = []
    if has_component(food) or not (has_component(trade) or has_component(fodder)):
        parts.append(format_yield(food))
    if has_component(trade):
        parts.append(format_trade(trade))
    if has_component(fodder):
        parts.append(PICKER_FODDER_PRODUCT_FORMAT % format_magnitude(fodder))
    return TRADE_COMPONENT_SEPARATOR.join(parts)

## THE COMPACT TWIN of `yield_components`, for a surface that supplies its own framing and has no room
## to repeat "/turn" — today the work zone's per-kind filter chips (`🦌 2 · 0.20 ⇄ 0.22`). Same
## render-only-when-non-zero rule and same food-leads order, but BARE MAGNITUDES: a chip states a
## count and that kind's total, and a `+` beside a count would read as a change rather than a level.
## The point of the pair here is aggregate honesty — a hunt chip covering one deer and one wolf must
## not report only the deer, and a chip whose whole set pays trade alone shows the trade total rather
## than a `0.00` asserting its sources produce nothing.
static func magnitude_components(food: float, trade: float) -> String:
    var parts: Array[String] = []
    if has_component(food) or not has_component(trade):
        parts.append(format_magnitude(food))
    if has_component(trade):
        parts.append(TRADE_COMPONENT_FORMAT % [FoodIcons.TRADE_GOODS_GLYPH, format_magnitude(trade)])
    return COMPACT_COMPONENT_SEPARATOR.join(parts)

## A `{compact, full}` metric pair for an EXTRACTIVE rung, over the source's whole yield VECTOR — the
## metric on every one of the three pickers. Food leads; each component appears only when it is
## non-zero, so a wolf's four rungs read as four ascending TRADE numbers (never four zeros), a deer's
## read food-then-trade, and a hay-bearing patch's read food-trade-fodder. When the rung pays nothing
## at all the food zero is still printed: `0.00 food` is the honest reading of a ceiling that exists
## and is empty, as opposed to a component the source never had. The compact half is the face's
## product LINE (`picker_products`, named in words); the tooltip keeps the signed "up to …" ceiling
## wording.
##
## **The forage picker comes through here too now** (#426). It used to call a food-only
## `extractive_take`, on the standing claim that the plant web projected no non-food rate — which
## stopped being true the turn the per-policy row reached the wire carrying all three accounts. That
## food-only twin is deleted rather than left as an alias: one joiner is what keeps the three pickers
## wearing one face.
static func extractive_take_pair(food: float, trade: float, fodder: float = 0.0) -> Dictionary:
    var show_food := has_component(food) or not (has_component(trade) or has_component(fodder))
    var full_parts: Array[String] = []
    if show_food:
        full_parts.append(POLICY_CAP_FORMAT % format_signed(food))
    if has_component(trade):
        full_parts.append(POLICY_CAP_TRADE_FORMAT % [
            FoodIcons.TRADE_GOODS_GLYPH, format_signed(trade)])
    if has_component(fodder):
        full_parts.append(POLICY_CAP_FODDER_FORMAT % format_signed(fodder))
    return {
        "compact": picker_products(food, trade, fodder),
        "full": TRADE_COMPONENT_SEPARATOR.join(full_parts),
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
## remainder of the trip length the band-agnostic `hunt_trip_estimates` table cannot carry (one row
## serves every band). Matches the sim launch feed EXACTLY: ceil(2 × wrap-aware hex_distance(band, herd)
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
    return int(ceil(float(2 * one_way) / move_rate))

## The sim-exported per-turn BAND take ceiling for `policy` on `herd` (`hunt_policy_ceilings` — the
## herd's renewable FLOW), or `HUNT_RATE_UNAVAILABLE` when the snapshot carries none. NEVER derived
## here — the ecology/MSY model that produces these numbers lives in the sim.
static func hunt_policy_ceiling(herd: Dictionary, policy: String) -> float:
    return _ceiling_from(herd, HERD_BAND_CEILINGS_KEY, policy)

## The same ceiling in TRADE GOODS/turn (`hunt_policy_trade_ceilings`). `HUNT_RATE_UNAVAILABLE` when
## the snapshot carries no trade row — which a caller must read as "unknown", never as "no trade".
static func hunt_policy_trade_ceiling(herd: Dictionary, policy: String) -> float:
    return _ceiling_from(herd, HERD_BAND_TRADE_CEILINGS_KEY, policy)

static func _ceiling_from(herd: Dictionary, key: String, policy: String) -> float:
    var ceilings_variant: Variant = herd.get(key, {})
    if not (ceilings_variant is Dictionary) or not (ceilings_variant as Dictionary).has(policy):
        return HUNT_RATE_UNAVAILABLE
    return float((ceilings_variant as Dictionary)[policy])

## The component this HERD actually pays, from its per-worker vector (the sim's `ratio_axis()` rule:
## the first component with a positive rate, provisions preferred). `per_worker_yield` /
## `per_worker_trade` are the SPECIES-AWARE per-herd rates — never the cohort's species-blind
## `hunt_per_worker_provisions`, which reports a positive food rate beside a wolf's zero food ceilings.
static func herd_yield_axis(herd: Dictionary) -> String:
    if has_component(float(herd.get(FORECAST_PER_WORKER_KEY, 0.0))):
        return YIELD_AXIS_PROVISIONS
    if has_component(float(herd.get(FORECAST_PER_WORKER_TRADE_KEY, 0.0))):
        return YIELD_AXIS_TRADE
    return YIELD_AXIS_PROVISIONS

## The herd's per-worker rate, per-policy ceiling and one-animal quantum ON THE AXIS IT PAYS —
## everything the carry/cadence arithmetic divides by, resolved once so no caller picks a component by
## hand. `{axis, per_worker, ceiling, per_animal}`; `ceiling` is `HUNT_RATE_UNAVAILABLE` when the herd
## carries no row for `policy`.
static func herd_axis_rates(herd: Dictionary, policy: String) -> Dictionary:
    var axis := herd_yield_axis(herd)
    if axis == YIELD_AXIS_TRADE:
        return {
            "axis": axis,
            "per_worker": float(herd.get(FORECAST_PER_WORKER_TRADE_KEY, 0.0)),
            "ceiling": hunt_policy_trade_ceiling(herd, policy),
            "per_animal": float(herd.get(FORECAST_TRADE_PER_ANIMAL_KEY, 0.0)),
        }
    return {
        "axis": axis,
        "per_worker": float(herd.get(FORECAST_PER_WORKER_KEY, 0.0)),
        "ceiling": hunt_policy_ceiling(herd, policy),
        "per_animal": float(herd.get(FORECAST_FOOD_PER_ANIMAL_KEY, 0.0)),
    }

## PRE-COMMIT FORECAST (the compose-time counterpart to `source_yield_readout`'s post-hoc note).
## Pull the source's per-worker yield + the take ceiling for the STANCE `policy` — both food/turn at
## its CURRENT biomass, at output_multiplier 1.0. `src` is a herd dict (bare keys) or a tile_info (the
## patch's fields, `patch_`-prefixed); `known` is false for a dead-season source or an older
## snapshot that carries no forecast fields, in which case callers show no row and apply no cap.
##
## **`policy` IS ALWAYS ONE OF THE FOUR STANCES** (issue #442). This used to answer for a build verb
## too, carrying `investment` / `payoff*` / `feed*` alongside — and that overload is exactly what the
## split removed: a build's dip and payoff are a function of the IMPROVEMENT *and* the stance
## together, which no single-policy lookup can express. `improvement_forecast` composes those, and it
## is written in terms of this.
##
## `kind` is the caller-stated SOURCE_KIND_*; `prefix` only spells the scalar keys (the two are
## independent — a forage patch reaches here under either forage prefix).
## One cell out of a per-policy FORAGE row dict — the plant twin of `hunt_policy_ceiling`, and the one
## place the `patch_`-prefix question is asked for these six keys.
##
## Falls back to the DEFAULT policy's cell for an unrecognized policy (as the herd side does), then to
## `0.0`. Returns `HUNT_RATE_UNAVAILABLE` (< 0) when the row dict is **absent or empty**, which is how a
## caller tells "this snapshot carries no forage forecast" from "every rung honestly pays zero" — the
## distinction issue #426 exists to restore, and one a per-worker scalar could never express.
static func forage_row_cell(
        src: Dictionary, prefix: String, dict_key: String, policy: String) -> float:
    var rows: Dictionary = src.get(prefix + dict_key, {})
    if rows.is_empty():
        return HUNT_RATE_UNAVAILABLE
    return float(rows.get(policy, rows.get(DEFAULT_HUNT_POLICY, 0.0)))

## Does the wire describe this source's forecast **at all**? A PRESENCE test, not a rate test.
##
## For a FORAGE patch the per-policy row is the witness: present ⇒ described (even if every account is
## zero), absent ⇒ an older snapshot or a harness fixture that seeded no forecast. For a HERD it stays
## the per-worker pair, whose `or` clause already handles the inedible-species case correctly — the
## plant web's failure was that a patch has no `perWorkerTrade` for that `or` to rescue it with.
static func forecast_is_known(src: Dictionary, kind: String, prefix: String) -> bool:
    if kind == SOURCE_KIND_HERD:
        return float(src.get(prefix + FORECAST_PER_WORKER_KEY, 0.0)) >= FORECAST_MIN_PER_WORKER \
            or float(src.get(prefix + FORECAST_PER_WORKER_TRADE_KEY, 0.0)) >= FORECAST_MIN_PER_WORKER
    var rows: Dictionary = src.get(prefix + FORAGE_ROW_CEILING, {})
    return not rows.is_empty()

## **THE ONE PLACE A BUILD'S CEILING DIP IS RESOLVED** — `<rung>BuildFraction` off the source, or
## `NO_BUILD_DIP` when nothing is in flight. It is the client twin of `LadderConfig::build_dip`, and
## every ceiling in this file goes through it, so the compose sheet, the work board and the deal line
## cannot apply the dip differently (or, as they did, three of them not at all).
##
## A `<= 0` fraction is the wire saying it does not describe this rung's build on this source — a
## species that can never be penned, a patch with no such rung. That is NOT "the build pays zero", so
## it answers the identity rather than collapsing every ceiling to nothing; `improvement_forecast`
## makes the same call the other way, declining to quote a deal it cannot price.
static func build_dip(src: Dictionary, prefix: String, improvement: String) -> float:
    if not FORECAST_BUILD_FRACTION_KEYS.has(improvement):
        return NO_BUILD_DIP
    var fraction := float(src.get(
        prefix + String(FORECAST_BUILD_FRACTION_KEYS[improvement]), 0.0))
    return fraction if fraction > 0.0 else NO_BUILD_DIP

## **`improvement` IS THE DIP, AND LEAVING IT OUT WAS A THREE-SURFACE BUG.** While a build runs the
## sim's ceiling is `stance ceiling × <rung>BuildFraction` — the stance-only answer is the ceiling of a
## crew that is NOT building, which is exactly not the crew asking. Passed, every ceiling here (and
## therefore `axis_ceiling`) carries the dip, so the compose stepper's cap, the green forecast line and
## the overdraw verdict resolved off this all agree with the sim rather than with each other.
## `IMPROVEMENT_NONE` (the default) is the identity, so a pure harvest reads exactly as before.
##
## **`improvement_forecast` deliberately does NOT pass one.** Its first term is the un-dipped stance
## baseline — the `1.27 → 0.32 while building → 5.76` deal — and dipping it there would quote the dip
## twice and erase the number the whole three-term line exists to show.
static func forecast_inputs(src: Dictionary, kind: String, prefix: String, policy: String,
        improvement: String = IMPROVEMENT_NONE) -> Dictionary:
    # The dip factor for the rung in flight, or 1.0 when nothing is being built here. A 0/absent
    # fraction means the wire does not describe this rung's build on this source, which is NOT "the
    # build pays nothing" — so it falls back to the identity and the ceilings stay undipped, exactly
    # as `improvement_forecast` declines to quote a deal it cannot price.
    var dip := build_dip(src, prefix, improvement)
    var per_worker := float(src.get(prefix + FORECAST_PER_WORKER_KEY, 0.0))
    # The DIP ceiling paid while the source is prepared. The two source kinds carry it differently, so
    # branch on the kind the CALLER STATED — the prefix cannot answer this (a herd and a raw wire
    # forage patch share the empty prefix), and neither can the dict's shape:
    #   HERD  → the `hunt_policy_ceilings` LIST is the herd's ONLY wire representation (the old
    #           per-policy `ceilingSustain`/… scalars are deprecated schema slots), so every herd rung
    #           — Sustain/Surplus/Deplete/Eradicate, Tame, Corral — resolves through it.
    #   FORAGE→ a patch has no such list; its per-policy scalars are its only representation.
    # `hunt_policy_ceiling` returns HUNT_RATE_UNAVAILABLE (< 0) for a herd with no row, which falls
    # back to Sustain's row exactly as the old scalar lookup did, then clamps to 0. That 0 never
    # manufactures a row: `known` is decided by `per_worker` alone, so a herd with no forecast data
    # still reads "not known" and callers show no row and apply no cap.
    var ceiling := 0.0
    if kind == SOURCE_KIND_HERD:
        ceiling = hunt_policy_ceiling(src, policy)
        if ceiling < 0.0:
            ceiling = hunt_policy_ceiling(src, DEFAULT_HUNT_POLICY)
        ceiling = maxf(ceiling, 0.0)
    else:
        # FORAGE: the per-policy ROW is the ceiling's only wire representation now (#426) — the six flat
        # `ceiling*` scalars it replaced are deprecated slots, exactly as the herd's were. Read through
        # the row so the food half and its two non-food siblings below cannot come from different places.
        ceiling = maxf(forage_row_cell(src, prefix, FORAGE_ROW_CEILING, policy), 0.0)
    # WHOLE-ANIMAL HUNT: a take of whole animals via a kill-credit bank (`food_per_animal` = one animal's
    # yield in food; 0/absent for a forage patch). The peak-turn carry need is quantized to whole bodies
    # (see `max_useful_workers`), so it must fire ONLY for a hunt of a live, un-penned herd — never a
    # forage patch (no food_per_animal) or a corralled herd (managed `worker_tend` harvest, whose
    # forecast already collapses every ceiling to per_worker). It no longer excludes a "build rung":
    # the stance is always a stance now, and a crew building a pen still takes whole animals while it
    # does so — the dip scales the ceiling, it does not change the rhythm.
    var food_per_animal := float(src.get(prefix + FORECAST_FOOD_PER_ANIMAL_KEY, 0.0))
    # THE SECOND PRODUCT (issue #337) and THE THIRD (#426). A herd carries a per-worker TRADE rate, a
    # per-policy trade ceiling and a per-animal trade quantum beside the food ones; a **forage patch now
    # carries its own per-policy trade AND fodder**, read off the same row the food ceiling came from.
    # The AXIS is what the whole-animal arithmetic divides by: for an INEDIBLE species the food
    # quantum is honestly 0, so deriving a cadence or a carry cap from food alone divides by zero and
    # yields nothing at all.
    var per_worker_trade := float(src.get(prefix + FORECAST_PER_WORKER_TRADE_KEY, 0.0))
    var trade_per_animal := float(src.get(prefix + FORECAST_TRADE_PER_ANIMAL_KEY, 0.0))
    var ceiling_trade := 0.0
    var ceiling_fodder := 0.0
    var per_worker_fodder := 0.0
    if kind == SOURCE_KIND_HERD:
        ceiling_trade = hunt_policy_trade_ceiling(src, policy)
        if ceiling_trade < 0.0:
            ceiling_trade = hunt_policy_trade_ceiling(src, DEFAULT_HUNT_POLICY)
        ceiling_trade = maxf(ceiling_trade, 0.0)
    else:
        # FORAGE: every account comes off the per-policy row, INCLUDING the per-worker terms — which is
        # why they are read here rather than from a patch-level scalar. `Deplete`'s trade markup is
        # already folded into both halves server-side, so `min(w × per_worker, ceiling)` is honest per
        # component and nothing here knows a markup exists. **The plant web has no third patch-level
        # per-worker scalar to fall back on, deliberately: a policy-blind scalar cannot state a
        # policy-dependent rate.**
        ceiling_trade = maxf(forage_row_cell(src, prefix, FORAGE_ROW_CEILING_TRADE, policy), 0.0)
        ceiling_fodder = maxf(forage_row_cell(src, prefix, FORAGE_ROW_CEILING_FODDER, policy), 0.0)
        per_worker = maxf(forage_row_cell(src, prefix, FORAGE_ROW_PER_WORKER, policy), per_worker)
        per_worker_trade = maxf(
            forage_row_cell(src, prefix, FORAGE_ROW_PER_WORKER_TRADE, policy), 0.0)
        per_worker_fodder = maxf(
            forage_row_cell(src, prefix, FORAGE_ROW_PER_WORKER_FODDER, policy), 0.0)
    # **THE DIP RIDES THE CEILINGS ONLY, PER ACCOUNT** — never the per-worker rates. The sim caps a
    # build's take at `min(workers × per_worker, ceiling × dip)` (`forage::forage_take`), so a crew is
    # as productive per head while it builds; what shrinks is how much the ground will give up. Applied
    # here, before the axis triple is derived, so `axis_ceiling` and every consumer of it — the worker
    # cap, the preview line, the overdraw test — read one dipped number.
    ceiling *= dip
    ceiling_trade *= dip
    ceiling_fodder *= dip
    var trade_axis: bool = not has_component(per_worker) and has_component(per_worker_trade)
    var axis_per_worker := per_worker_trade if trade_axis else per_worker
    var axis_ceiling := ceiling_trade if trade_axis else ceiling
    var axis_per_animal := trade_per_animal if trade_axis else food_per_animal
    var whole_animal: bool = axis_per_animal > 0.0 and not bool(src.get("corralled", false))
    return {
        "per_worker": per_worker,
        "ceiling": ceiling,
        "food_per_animal": food_per_animal,
        "per_worker_trade": per_worker_trade,
        "ceiling_trade": ceiling_trade,
        # THE THIRD ACCOUNT (#426) — plant-only: no animal pays fodder, so a herd reads 0 here and every
        # hunt-side answer is unchanged.
        "per_worker_fodder": per_worker_fodder,
        "ceiling_fodder": ceiling_fodder,
        "trade_per_animal": trade_per_animal,
        # The axis triple every divide-by-a-quantum consumer reads (`max_useful_workers` and the local
        # preview), so no caller has to know which product this species pays.
        "axis": YIELD_AXIS_TRADE if trade_axis else YIELD_AXIS_PROVISIONS,
        "axis_per_worker": axis_per_worker,
        "axis_ceiling": axis_ceiling,
        "axis_per_animal": axis_per_animal,
        "whole_animal": whole_animal,
        # **A PRESENCE test, not a rate test** (#426). It used to be `per_worker >= ε`, which conflated
        # "the wire carried no forecast" with "the rate is genuinely zero" — and its own docstring said it
        # meant the former. A zero-conversion crop makes the latter real, so the two came apart and the
        # compose sheet answered by going silent on the one state it most needed to report.
        "known": forecast_is_known(src, kind, prefix),
    }

## **THE WHOLE DEAL AN IMPROVEMENT OFFERS, composed in ONE place** (issue #442) — the stance the crew
## is holding, the dipped take it accepts while it builds, and the payoff the finished rung pays:
##
##     +0.96  →  +0.24 while building  →  +1.20 /turn
##      stance          preparing              payoff
##
## `preparing = stanceCeiling × <rung>BuildFraction`, **per account**, exactly as the take is capped
## per account: a hay meadow's fodder dips by the same factor its food does, and quoting the food
## component alone would have understated a fodder crop's build to nothing. The stance term is the
## number the old "Preparing: +X → then +Y" line structurally could not show, because a build verb
## used to BE the policy — there was no stance left to quote.
##
## Returns `{}` when `improvement` is `IMPROVEMENT_NONE` or the source carries no forecast, so a caller
## renders no deal rather than a deal made of zeros. `stance` is the four-rung harvest stance the crew
## holds; the two are independent, and a non-Sustain stance beside a running build is LEGAL (it defeats
## itself through the ecology — the meter accrues only while the source is Thriving).
##
## The `feed` term is the pen's per-turn upkeep and rides ONLY the Corral rung (`FORECAST_FEED_KEYS`) —
## the one asymmetry between the two webs, and a deliberate one.
static func improvement_forecast(src: Dictionary, kind: String, prefix: String, stance: String,
        improvement: String) -> Dictionary:
    if improvement == IMPROVEMENT_NONE or not FORECAST_PAYOFF_KEYS.has(improvement):
        return {}
    var stance_forecast := forecast_inputs(src, kind, prefix, stance)
    if not bool(stance_forecast["known"]):
        return {}
    # The dip factor. 0/absent means the wire does not describe this rung's build on this source
    # (a species that can never be penned, an older snapshot) — the deal is then unquotable, so say
    # nothing rather than render a `× 0` dip that reads as "building pays you nothing".
    var fraction := float(src.get(
        prefix + String(FORECAST_BUILD_FRACTION_KEYS[improvement]), 0.0))
    if fraction <= 0.0:
        return {}
    var payoff := float(src.get(prefix + String(FORECAST_PAYOFF_KEYS[improvement]), 0.0))
    # The payoff's non-food components — a VECTOR like every other yield in this model. Trade reaches
    # all four rungs; fodder is plant-only (no animal pays it — a structural zero, not a gap). A rung
    # with no twin in a table resolves to 0.0 and renders as nothing, which is the rule.
    var payoff_trade := 0.0
    if FORECAST_PAYOFF_TRADE_KEYS.has(improvement):
        payoff_trade = float(src.get(prefix + String(FORECAST_PAYOFF_TRADE_KEYS[improvement]), 0.0))
    var payoff_fodder := 0.0
    if FORECAST_PAYOFF_FODDER_KEYS.has(improvement):
        payoff_fodder = float(src.get(prefix + String(FORECAST_PAYOFF_FODDER_KEYS[improvement]), 0.0))
    var feed_rung: bool = FORECAST_FEED_KEYS.has(improvement)
    var feed := 0.0
    if feed_rung:
        feed = float(src.get(prefix + String(FORECAST_FEED_KEYS[improvement]), 0.0))
    return {
        "improvement": improvement,
        "stance": stance,
        "build_fraction": fraction,
        # The stance's OWN forecast, carried whole so a caller can price the crew's real take through
        # `expected_yield_account` per account rather than quoting a bare ceiling the crew may not
        # reach. The dip is that take multiplied by `build_fraction` — the sim applies the fraction to
        # the yield, so applying it to the capped take is the same arithmetic in the same order.
        "stance_forecast": stance_forecast,
        # THE THREE TERMS, each a full account vector. `stance_*` is what the crew takes today,
        # `preparing_*` what it takes while the rung is built, `payoff_*` what the built rung pays.
        "stance_ceiling": float(stance_forecast["ceiling"]),
        "stance_ceiling_trade": float(stance_forecast["ceiling_trade"]),
        "stance_ceiling_fodder": float(stance_forecast["ceiling_fodder"]),
        "preparing": float(stance_forecast["ceiling"]) * fraction,
        "preparing_trade": float(stance_forecast["ceiling_trade"]) * fraction,
        "preparing_fodder": float(stance_forecast["ceiling_fodder"]) * fraction,
        "payoff": payoff,
        "payoff_trade": payoff_trade,
        "payoff_fodder": payoff_fodder,
        "feed_rung": feed_rung,
        "feed": feed,
    }

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
    return bool(src.get(prefix + String(FORECAST_DONE_FLAG_KEYS[improvement]), false))

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
    # whole animals drop, each worth food_per_animal — carry that peak, not the average flow.
    var per_animal := float(forecast.get("axis_per_animal", forecast.get("food_per_animal", 0.0)))
    if bool(forecast.get("whole_animal", false)) and per_animal > 0.0:
        var animals := floori(ceiling / per_animal) + HUNT_PEAK_DROP_BANK_BONUS
        var peak_drop := float(animals) * per_animal
        return ceili(peak_drop / per_worker)
    return int(ceilf(ceiling / per_worker))

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

## The take `workers` would ACTUALLY produce here: min(workers × per_worker, ceiling), scaled by the
## acting band's output multiplier (the sim exports the forecast at 1.0).
static func expected_yield(forecast: Dictionary, workers: int, band: Dictionary) -> float:
    return expected_yield_account(forecast, workers, band, "per_worker", "ceiling")

## The same take on ANY ONE account (#426). `min(workers × per_worker, ceiling)` is applied PER
## COMPONENT, never to a total: the sim caps each account against its own ceiling, and a patch whose
## labor binds on food can be ceiling-bound on fodder in the same turn. The account keys are
## `forecast_inputs`' own (`per_worker`/`ceiling`, `per_worker_trade`/`ceiling_trade`,
## `per_worker_fodder`/`ceiling_fodder`) — passed in rather than switched on here, so adding a fourth
## account is a call site, not an edit to this function.
## **`ceiling_scale` GOES INSIDE THE `min`, AND THAT IS THE WHOLE POINT OF THE PARAMETER.** A build's
## dip is a factor on the CEILING (`forage::forage_take` caps at `min(workers × per_worker, ceiling ×
## dip)`), so scaling the already-capped take instead — `min(…) × dip` — is a different number
## whenever the crew is labour-bound below the dipped ceiling, and the client under-reported by exactly
## the dip there. `NO_BUILD_DIP` (the default) leaves every non-build caller bit-identical.
static func expected_yield_account(forecast: Dictionary, workers: int, band: Dictionary,
        per_worker_key: String, ceiling_key: String,
        ceiling_scale: float = NO_BUILD_DIP) -> float:
    var raw := minf(float(workers) * float(forecast.get(per_worker_key, 0.0)),
        float(forecast.get(ceiling_key, 0.0)) * ceiling_scale)
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
    # Its trade twin — 0 for a source that pays no trade, which is exactly what suppresses the line.
    var trade_rate := 0.0
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
        # THE SECOND PRODUCT (issue #337): the same steady/actual split in trade goods. Rendered ONLY
        # when non-zero, so a deer row reads `+0.31 /turn · ⇄ +0.12`, a wolf row reads `⇄ +0.12`
        # ALONE (never a "+0.00 /turn" that says its pelts are worth no meat), and a patch that sells
        # nothing is unchanged. `trade_rate_of` owns the forage fallback — see its header for why the
        # `has()` spelling this line used to carry never fired.
        trade_rate = trade_rate_of(m)
        if has_component(trade_rate):
            tooltip += TRADE_COMPONENT_SEPARATOR + (TRADE_TOOLTIP_FORMAT % [
                FoodIcons.TRADE_GOODS_GLYPH, format_signed(trade_rate)])
        label_suffix = " " + yield_components(rate, trade_rate)
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
    var muted_note := ""
    var wasted := float(m.get("wasted_yield", 0.0))
    if wasted >= FOOD_FLOW_MIN:
        muted_note = WASTED_NOTE_FORMAT % format_magnitude(wasted)
        var wasted_tip := WASTED_TOOLTIP % format_yield(wasted)
        tooltip = wasted_tip if tooltip == "" else tooltip + TOOLTIP_LINE_SEPARATOR + wasted_tip
    return {
        "label_suffix": label_suffix, "warn": warn, "note": note,
        "muted_note": muted_note, "tooltip": tooltip, "rate": rate,
        # The trade component, so a caller that renders its own sentence (the work inspector) states
        # the same two products the row headline does instead of only the food one.
        "trade_rate": trade_rate,
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
            # A sown FIELD of a cash crop pays trade, not provisions or fodder — same shape, the trade
            # account. **Both are scoped to the FIELD rung deliberately**: a sown Field is 100% its crop,
            # so there a cash crop's `sow_payoff` really is exactly 0. One rung down it is not — a TENDED
            # patch only WEEDS, so it keeps paying the volunteers' calories (#433), which is why the
            # `cultivate_*` pair below is a separate quote and not a scaled copy of these.
            "sow_trade_payoff": float(entry.get("sow_trade_payoff", 0.0)),
            # THE TENDED-RUNG TWINS of the two above (#419). The `sow_*` pair are Field payoffs, so a
            # Cultivate row that read them stated rung 3's number on rung 2 — `10.2 trade` for cotton
            # beside a rung that pays a fraction of it. Both rungs ride the entry; the picker reads the
            # pair its policy names, exactly as it already does for the ratio and the food payoff.
            "cultivate_fodder_payoff": float(entry.get("cultivate_fodder_payoff", 0.0)),
            "cultivate_trade_payoff": float(entry.get("cultivate_trade_payoff", 0.0)),
        })
    if entries.is_empty():
        return entries
    entries[0]["percent"] = int(entries[0]["percent"]) + FLORA_SHARE_PERCENT_TOTAL - total
    return entries

## The `hunt_trip_estimates` key the sim exports a (policy, party size) estimate under. One definition —
## the lookup and the plateau scan must agree on the key format or the scan silently finds nothing.
static func hunt_estimate_key(policy: String, workers: int) -> String:
    return "%s%s%d" % [policy, HUNT_ESTIMATE_KEY_SEPARATOR, workers]

## The raid `workers` from `band` deliver hunting `herd` under `policy`. A PURE TABLE LOOKUP into the
## sim's forward-simulated `hunt_trip_estimates` (`HERD_TRIP_ESTIMATES_KEY`) — ZERO arithmetic: the sim
## grabs the herd's standing surplus above the policy floor in a burst and reports the whole animals it
## lands (`animals_taken`) and the turns until the party comes home (`turns_to_fill`, NOT "turns to fill
## the pack"). The ecology/MSY model is never reproduced here. (The LOCAL band hunt preview DOES compute
## — see `_hunt_take_rate` over the band ceiling `hunt_policy_ceilings`.) Returns {available, denial,
## empty, animals, turns, food, long_raid, slow}: `available` false = the snapshot carries no estimate
## for this (policy, party size) (older server → the caller shows no forecast at all).
static func hunt_trip_forecast(band: Dictionary, herd: Dictionary, policy: String, workers: int,
        grid_width: int, wrap_horizontal: bool) -> Dictionary:
    var estimates_variant: Variant = herd.get(HERD_TRIP_ESTIMATES_KEY, {})
    if workers <= 0 or not (estimates_variant is Dictionary):
        return {"available": false}
    var key := hunt_estimate_key(policy, workers)
    var estimates := estimates_variant as Dictionary
    if not estimates.has(key):
        return {"available": false}
    var estimate: Dictionary = estimates[key]
    # A DENIAL mission carries nothing home at all. **`delivers_food == false` alone no longer means
    # that** (issue #337): it was redefined to say the QUARRY IS INEDIBLE, and an inedible quarry still
    # pays pelts — a wolf raid reads `delivers_food false, delivers_trade true` and is a real delivery,
    # while Eradicate on a deer now banks a whole-stock windfall like every other rung. So the denial
    # carve-out fires only when the species pays NEITHER product.
    if not bool(estimate.get("delivers_food", false)) \
            and not bool(estimate.get("delivers_trade", false)):
        return {"available": true, "denial": true, "empty": false}
    # Nothing delivered in EITHER currency = the herd is at/below the policy's floor: no standing
    # surplus to raid, the party returns empty. The ONE non-viable case (the raid always completes;
    # the herd has nothing). Reading food alone here would call every wolf raid "too lean".
    # NOT `animals_taken == 0`: a party too small to carry a whole animal now KILLS one and hauls the
    # fraction its pack holds (mirroring the local hunt), so `animals_taken >= 1` whenever there's any
    # surplus — the delivered PAYLOAD (with waste) is the honest bind, not the whole-animal kill count.
    var delivered_food := float(estimate.get("delivered_food", 0.0))
    var delivered_trade := float(estimate.get("delivered_trade", 0.0))
    if delivered_food <= 0.0 and delivered_trade <= 0.0:
        return {"available": true, "denial": false, "empty": true}
    var animals := int(estimate.get("animals_taken", 0))
    # turns_to_fill == 0 = the raid ran the whole horizon still delivering (a long raid). A warn
    # threshold of 0 means the server sent none — report the raid, judge nothing. `turns_to_fill` now
    # counts HUNTING turns only; the band-relative round trip is added on top so the headline is honest.
    var hunt_turns := int(estimate.get("turns_to_fill", 0))
    var long_raid: bool = hunt_turns <= 0
    var travel := round_trip_travel_turns(band, herd, grid_width, wrap_horizontal)
    var total := hunt_turns + travel
    var warn_turns := int(band.get("expedition_viability_warn_turns", 0))
    var slow: bool = not long_raid and warn_turns > 0 and total > warn_turns
    # Waste fraction: killed-but-not-carried food over total killed. A small party on big game raids one
    # animal and hauls only the pack's worth, wasting the rest — a high % here is informative, not a block.
    var wasted_food := float(estimate.get("wasted_food", 0.0))
    var killed := delivered_food + wasted_food
    var waste_pct := (wasted_food / killed) if killed > 0.0 else 0.0
    return {
        "available": true, "denial": false, "empty": false,
        "animals": animals, "turns": total, "hunt_turns": hunt_turns, "travel": travel,
        "long_raid": long_raid, "slow": slow,
        # The delivered PAYLOAD in food — what the party actually LANDS (a partial for a small party),
        # straight from the sim's forward-simulated raid, NOT animals × food_per_animal (which counts the
        # whole kill and overstates a partial). It may be 0 on an inedible quarry, whose whole payload
        # rides `trade`; at least one of the two is > 0 here (empty returned above otherwise), and each
        # is rendered only when it is.
        "food": delivered_food, "trade": delivered_trade, "waste_pct": waste_pct,
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
    # No surplus above the policy's floor → the raid returns empty. The ONE non-viable case (red).
    if bool(forecast.get("empty", false)):
        return "[color=#%s]%s%s[/color]" % [
            HudStyle.DANGER_HEX, HUNT_FORECAST_WARN_GLYPH,
            HUNT_FORECAST_NO_SURPLUS_FORMAT % herd_name,
        ]
    # A real raid: headline the delivered PAYLOAD (the animal count over turns + what it LANDS), then
    # the waste. The payload is `delivered_food` and/or `delivered_trade`, each named only when the
    # quarry actually pays it — so an Eradicate deer raid quotes its windfall and a wolf raid quotes
    # pelts, neither of them a "~0 food".
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
        # Ran the whole horizon still delivering (no bounded turn count) — a slow but real haul (amber).
        var long_text: String = HUNT_FORECAST_LONG_RAID_FORMAT % [animals, herd_name]
        var long_travel := int(forecast.get("travel", 0))
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

## The raid's delivered payload as a trailing " · ~20 food · ⇄ ~3 trade goods" — each component only
## when the quarry pays it, food leading. "" when the forecast carries no payload at all.
static func _raid_payload_suffix(forecast: Dictionary) -> String:
    var suffix := ""
    var food := float(forecast.get("food", 0.0))
    if has_component(food):
        suffix += HUNT_FORECAST_FOOD_FORMAT % int(round(food))
    var trade := float(forecast.get("trade", 0.0))
    if has_component(trade):
        suffix += HUNT_FORECAST_TRADE_FORMAT % [FoodIcons.TRADE_GOODS_GLYPH, int(round(trade))]
    return suffix

## The raid returns empty: the sim's estimate for THIS (policy, party size) says the herd has no surplus
## above the policy's floor (`animals_taken == 0`). The single definition of the blocked case — both
## entry points (panel button + targeting click) gate on it.
static func hunt_trip_no_surplus(forecast: Dictionary) -> bool:
    return bool(forecast.get("available", false)) and bool(forecast.get("empty", false))

## The ONE sentence spoken about a no-surplus raid — shared verbatim by the herd panel (reason line +
## disabled-button tooltip) and the targeting-click command-feed refusal, so the two entry points can
## never disagree. Under the raid model party size cannot fix it (surplus is a property of the HERD, not
## the party), so — unlike the retired row scan — there is no alternative size to name.
static func hunt_no_surplus_reason(herd: Dictionary) -> String:
    return SEND_HUNT_NO_SURPLUS_REASON % herd_display_name(herd)

## Max party the band can detach as a hunting expedition: min(idle_workers, max_expedition_party_size),
## falling back to idle when the cap is absent/0 (mirrors the compose sheet's `party_max`). The SUPPLY
## side of the party stepper — what the band can spare; `expedition_useful_cap` below is the DEMAND
## side (what the raid can use), and the stepper takes the tighter of the two.
static func expedition_party_cap(band: Dictionary) -> int:
    var idle := int(band.get("idle_workers", 0))
    var cap := int(band.get("max_expedition_party_size", 0))
    return mini(idle, cap) if cap > 0 else idle

## The max-useful party for a raid: `delivered_food` PLATEAUS with party size once the standing surplus
## (not the pack) binds, so beyond the plateau extra hunters raise the payload by nothing. Scan the current
## policy's row for the smallest size at which delivered food stops rising and cap there — the raid twin of
## `_forecast_worker_cap`, and it mirrors its `{cap, note}` shape + "max N useful" note so the expedition
## and local pickers explain a dead `+` the same way. Scans DELIVERED FOOD (not the whole-animal
## `animals_taken`, which sits at 1 across every small-party size on big game — its leading-zeros plateau
## fooled the old scan into capping at 1; with partials delivered food rises smoothly, so the cap tracks
## the true bind). A table SCAN, zero client arithmetic. Returns the full `assignable` (no note) when the
## row carries no data or never plateaus within the band's reach.
static func expedition_useful_cap(band: Dictionary, herd: Dictionary, policy: String, assignable: int) -> Dictionary:
    var estimates_variant: Variant = herd.get(HERD_TRIP_ESTIMATES_KEY, {})
    if not (estimates_variant is Dictionary):
        return {"cap": assignable, "note": ""}
    var estimates := estimates_variant as Dictionary
    # Scan the herd's FULL exported absorption range — every party size the estimate table carries for
    # this policy, NOT the idle/party-limited cap — so `plateau` is the herd's true max-useful party
    # even when it exceeds what we can field right now. The returned cap still clamps to `assignable`
    # below, so this widens ONLY the explanatory note (it lets a labor-bound stepper name the ceiling
    # it's working toward, "N of M useful"), never the cap: within reach the loop breaks exactly as before.
    var scan_cap := 1
    for key in estimates:
        var parts := String(key).split(HUNT_ESTIMATE_KEY_SEPARATOR)
        if parts.size() == 2 and String(parts[0]) == policy:
            scan_cap = maxi(scan_cap, int(parts[1]))
    var prev_delivered := -1.0
    var plateau := 0
    for workers in range(1, scan_cap + 1):
        var cell_variant: Variant = estimates.get(hunt_estimate_key(policy, workers), null)
        if not (cell_variant is Dictionary):
            continue
        # Scan the component this QUARRY pays (issue #337): an inedible species delivers 0 food at
        # every party size, so a food-only scan finds no plateau at all and the party stepper loses
        # its max-useful cap. Edibility is a species property, so this picks the same component in
        # every cell of the row.
        var cell := cell_variant as Dictionary
        var delivered := float(cell.get("delivered_food", 0.0)) if bool(cell.get("delivers_food", false)) \
            else float(cell.get("delivered_trade", 0.0))
        if delivered > prev_delivered:
            prev_delivered = delivered
            plateau = workers   # the payload is still rising — this size is useful
        else:
            break               # the payload stopped rising — the previous size is the plateau
    if plateau <= 0:
        return {"cap": assignable, "note": ""}
    var useful: int = mini(plateau, assignable)
    if useful >= assignable:
        # Labor-bound below the plateau: the party capped at what you can field, not at usefulness.
        # `assignable = min(idle, max_party_size)`, so distinguish which constraint binds — freeing
        # idle workers only helps when idle is the binder; if the party-size cap binds, say so.
        var labor_note := ""
        if plateau > assignable:
            var idle := int(band.get("idle_workers", 0))
            var max_party := int(band.get("max_expedition_party_size", 0))
            if max_party > 0 and idle >= max_party:
                labor_note = PARTY_SIZE_BOUND_NOTE_FORMAT % [assignable, plateau]
            else:
                labor_note = LABOR_BOUND_NOTE_FORMAT % [assignable, plateau]
        return {"cap": assignable, "note": labor_note}
    var noun := MAX_USEFUL_NOUN_ONE if useful == 1 else MAX_USEFUL_NOUN_MANY
    return {"cap": useful, "note": MAX_USEFUL_NOTE_FORMAT % [useful, noun]}

## Each extractive policy's MAX obtainable food/turn — the raid twin of the local hunt's per-policy cap,
## so all three pickers (forage / local hunt / expedition) wear the same "up to X/turn" button metric and
## the four read ASCENDING (Sustain < Surplus < Deplete < Eradicate; deeper floors free more surplus). The
## metric is WORKER-INDEPENDENT: the max over every party size of `delivered_food / trip_turns`, where
## `trip_turns = turns_to_fill + round-trip travel` (a far herd's best rate is correctly lower). A bigger
## party delivers more food in fewer turns, so the rate rises then plateaus — the max is the honest cap.
## BOTH PRODUCTS ride the metric (issue #337): each component's best rate is scanned independently and
## rendered only when non-zero, so an inedible quarry's four rungs read as four ascending TRADE rates
## instead of four blanks. A rung that lands NOTHING in either currency — a true denial mission, which
## is now a property of the QUARRY and not of the Eradicate rung — carries no rate and falls back to its
## name + glyph. A table SCAN, zero client arithmetic. Empty when the herd carries no estimates.
static func expedition_policy_takes(band: Dictionary, herd: Dictionary,
        grid_width: int, wrap_horizontal: bool) -> Dictionary:
    var takes := {}
    var estimates_variant: Variant = herd.get(HERD_TRIP_ESTIMATES_KEY, {})
    if not (estimates_variant is Dictionary):
        return takes
    var estimates := estimates_variant as Dictionary
    var travel := round_trip_travel_turns(band, herd, grid_width, wrap_horizontal)
    for policy in LABOR_HUNT_POLICIES:
        var best_food := -1.0
        var best_trade := -1.0
        for key in estimates:
            var parts := String(key).split(HUNT_ESTIMATE_KEY_SEPARATOR)
            if parts.size() != 2 or String(parts[0]) != String(policy):
                continue
            var cell: Dictionary = estimates[key]
            var trip_turns := int(cell.get("turns_to_fill", 0)) + travel
            if trip_turns <= 0:
                continue
            # Each component gates on its OWN delivers flag: `delivers_food == false` now means the
            # quarry is inedible, so gating the whole row on it would blank a wolf's every rung.
            if bool(cell.get("delivers_food", false)):
                var delivered := float(cell.get("delivered_food", 0.0))
                if delivered > 0.0:
                    best_food = maxf(best_food, delivered / float(trip_turns))
            if bool(cell.get("delivers_trade", false)):
                var delivered_trade := float(cell.get("delivered_trade", 0.0))
                if delivered_trade > 0.0:
                    best_trade = maxf(best_trade, delivered_trade / float(trip_turns))
        if best_food >= 0.0 or best_trade >= 0.0:
            takes[String(policy)] = extractive_take_pair(maxf(best_food, 0.0), maxf(best_trade, 0.0))
    return takes

## Style the hunt-expedition send button from the live forecast. Two treatments, and the line between
## them is the point:
##   DELIVERING (viable / slow / long / denial) — the raid lands something (animals, or the denial it
##     promises). "primary" for a brisk raid; "armed" amber for a slow/long raid (`Send Anyway (≈54
##     turns)` / `Send Anyway (long raid)`) or a denial (`SEND_HUNT_DENIAL_BUTTON`) — ENABLED either
##     way: the player is told, then trusted.
##   NO SURPLUS (`animals_taken == 0`) — the raid returns empty, a mistake with no upside. DISABLED,
##     with the reason and the way out (party size can't fix it, so the reason names no alternative).
## No confirm dialogs either way.
static func style_send_hunt_button(button: Button, forecast: Dictionary, reason: String) -> void:
    # NO SURPLUS — the one blocked case. Disabled, and it says WHY plus what to do instead (the button is
    # the last thing the player looks at before clicking, so the reason belongs on it). Same words as the
    # panel line and the targeting refusal, from the one helper.
    if hunt_trip_no_surplus(forecast):
        button.text = SEND_HUNT_NO_SURPLUS_BUTTON
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
        button.text = SEND_HUNT_LONG_RAID_BUTTON
        HudStyle.apply_button(button, "armed")
        return
    if bool(forecast.get("slow", false)):
        button.text = SEND_HUNT_ANYWAY_TURNS_FORMAT % int(forecast.get("turns", 0))
        HudStyle.apply_button(button, "armed")
        return
    # A brisk, delivering raid (or no forecast at all — older server): the plain primary send.
    button.text = SEND_HUNTING_EXPEDITION_BUTTON
    HudStyle.apply_button(button, "primary")
