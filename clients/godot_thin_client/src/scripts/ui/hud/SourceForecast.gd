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
# EXTRACTIVE take policies — the four rungs that take from a wild source without changing it. Shared
# by forage + hunt (and the only ones a hunting EXPEDITION can carry: a detached party builds no pen).
const LABOR_HUNT_POLICIES := ["sustain", "surplus", "market", "eradicate"]
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
# Band food flow gate: a rate below this reads as absent rather than as a zero.
const FOOD_FLOW_MIN := 0.001
# An EXTRACTIVE rung's policy-button metric: the bare signed rate on the one-line button face, this
# wording in the tooltip so it reads as the ceiling it is (and the four rungs read as ASCENDING).
const POLICY_CAP_FORMAT := "up to %s/turn"

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
const FORECAST_CEILING_KEYS := {
    "sustain": "ceiling_sustain",
    "surplus": "ceiling_surplus",
    "market": "ceiling_market",
    "eradicate": "ceiling_eradicate",
    # The INVESTMENT rungs' ceiling is the DIP yield paid while the patch is being prepared — so the
    # same expected(workers, policy) math shows the cost of the investment while composing.
    "cultivate": "ceiling_cultivate",
    # Plant rung 3. Its OWN field rather than reusing `ceiling_cultivate`: the two plant rungs' dips
    # are independently tunable, and folding them onto one number would pass every forecast==actual
    # test by coincidence and lie the moment either rung is retuned.
    "sow": "ceiling_sow",
    # NOTE — this dict is the FORAGE PATCH's ceiling map, and ONLY that. A patch carries no policy
    # list, so a scalar field per rung is its whole representation. Every HERD policy — the four
    # extractive rungs plus `tame` and `corral` — resolves instead through the `hunt_policy_ceilings`
    # LIST via `hunt_policy_ceiling`; the herd's matching scalars are deprecated schema slots and are
    # no longer decoded. That's why `tame` and `corral` are absent here (their payoffs, `pastoral_yield`
    # / `corral_yield`, ARE real scalars and live in FORECAST_PAYOFF_KEYS). Adding a herd rung here
    # would read a field the wire no longer carries and quote a 0 dip.
}
# The PAYOFF the investment buys — the food/turn the source pays once prepared (one worker suffices).
# Only the investment rungs have one; an extractive rung's forecast is a single number.
#
# `tame` → `pastoral_yield`: the sim now exports the Tame rung's payoff (the pastoral MSY once the herd
# is tamed), the pastoral twin of `corral_yield`, so Tame renders the same dip→payoff pair as its three
# siblings. Tame's DURING-BUILDING dip has no scalar ceiling field (there is no `ceilingTame`); it rides
# the `hunt_policy_ceilings` LIST, so `forecast_inputs` resolves Tame's dip through `hunt_policy_ceiling`
# rather than a `FORECAST_CEILING_KEYS` scalar (adding a key there would silently quote Sustain's ceiling).
const FORECAST_PAYOFF_KEYS := {
    "cultivate": "tended_yield",
    "corral": "corral_yield",
    "sow": "field_yield",
    "tame": "pastoral_yield",
}
# The RUNNING COST the payoff is paid against. Only the pen has one: a corralled herd is a managed
# population that eats from the keeper's larder every turn (`pen_upkeep`), and `corral_yield` is the
# GROSS take with that feed NOT deducted — so advertising the payoff bare would promise a number the
# player never banks. A tended patch has no running cost, hence no entry.
const FORECAST_FEED_KEYS := {
    "corral": "pen_upkeep",
}
# Below this a worker produces nothing here (a dead-season forage tile with no forecast fields).
# Dividing by it would blow max-useful up to infinity, so instead: no forecast row,
# and the stepper keeps its plain idle-worker cap.
const FORECAST_MIN_PER_WORKER := 0.0001
# Sentinel for "no forecast data" → the stepper is not forecast-capped.
const MAX_USEFUL_UNBOUNDED := -1
# A whole-animal hunt's kill-credit bank accumulates the smoothed take, then discharges a WHOLE animal
# when it holds a full body. Worst case the turn's rate lands with just under one body already banked,
# so one extra whole animal drops that turn beyond floor(rate / body) — this is that +1.
const HUNT_PEAK_DROP_BANK_BONUS := 1
# A tended patch / corralled herd collapses max-useful to exactly 1, so this note has to read
# "max 1 worker" — pluralize the noun rather than shipping "max 1 workers".
const MAX_USEFUL_NOTE_FORMAT := "max %d %s useful here — more would be idle"
const MAX_USEFUL_NOUN_ONE := "worker"
const MAX_USEFUL_NOUN_MANY := "workers"
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
# An Eradicate expedition is a DENIAL mission, not a failed raid: it delivers no food BY DESIGN (the sim
# says so via `delivers_food`, the client never infers it from the policy string).
const HUNT_FORECAST_DENIAL_FORMAT := "%s — denial mission: hunts the herd toward extinction, delivers no food"
const HUNT_FORECAST_WARN_GLYPH := "⚠ "
# When a kill can't be fully carried (a big animal the crew is too small to haul) the surplus meat rots.
# A WARN-tinted suffix flags the fraction wasted — its OWN concern, rendered amber even on a green line.
const HUNT_WASTE_SUFFIX_FORMAT := " · ⚠ %d%% wasted"

# THE SEND BUTTON'S FOUR FACES, owned by `style_send_hunt_button`. A trip that is a trap names the cost
# (amber "armed") but is NEVER gated behind a confirm — the player is told, then trusted. Only the
# no-surplus raid, which has no upside at all, disables.
const SEND_HUNTING_EXPEDITION_BUTTON := "Send hunting party"
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
# Eradicate's button states the deal rather than implying failure — the mission IS the point.
const SEND_HUNT_DENIAL_BUTTON := "Send (delivers no food)"

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

## A `{compact, full}` metric pair for an EXTRACTIVE rung's per-turn cap — the bare signed rate on the
## button face, the "up to X/turn" wording in the tooltip. Shared by the hunt + forage takes helpers.
static func extractive_take(rate: float) -> Dictionary:
    var signed := format_signed(rate)
    return {"compact": signed, "full": POLICY_CAP_FORMAT % signed}

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
    var ceilings_variant: Variant = herd.get(HERD_BAND_CEILINGS_KEY, {})
    if not (ceilings_variant is Dictionary) or not (ceilings_variant as Dictionary).has(policy):
        return HUNT_RATE_UNAVAILABLE
    return float((ceilings_variant as Dictionary)[policy])

## PRE-COMMIT FORECAST (the compose-time counterpart to `source_yield_readout`'s post-hoc note).
## Pull the source's per-worker yield + the take ceiling for `policy` — both food/turn at its
## CURRENT biomass, at output_multiplier 1.0. `src` is a herd dict (bare keys) or a tile_info (the
## patch's fields, `patch_`-prefixed); `known` is false for a dead-season source or an older
## snapshot that carries no forecast fields, in which case callers show no row and apply no cap.
## An INVESTMENT policy additionally carries `payoff` (the tended/corral yield the preparation buys)
## and `investment: true`, so `_forecast_yield_row` can state the deal instead of one number.
## `kind` is the caller-stated SOURCE_KIND_*; `prefix` only spells the scalar keys (the two are
## independent — a forage patch reaches here under either forage prefix).
static func forecast_inputs(src: Dictionary, kind: String, prefix: String, policy: String) -> Dictionary:
    var per_worker := float(src.get(prefix + FORECAST_PER_WORKER_KEY, 0.0))
    # The DIP ceiling paid while the source is prepared. The two source kinds carry it differently, so
    # branch on the kind the CALLER STATED — the prefix cannot answer this (a herd and a raw wire
    # forage patch share the empty prefix), and neither can the dict's shape:
    #   HERD  → the `hunt_policy_ceilings` LIST is the herd's ONLY wire representation (the old
    #           per-policy `ceilingSustain`/… scalars are deprecated schema slots), so every herd rung
    #           — Sustain/Surplus/Market/Eradicate, Tame, Corral — resolves through it.
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
    elif policy in FORECAST_CEILING_KEYS:
        ceiling = float(src.get(prefix + String(FORECAST_CEILING_KEYS[policy]), 0.0))
    else:
        ceiling = float(src.get(prefix + String(FORECAST_CEILING_KEYS[DEFAULT_HUNT_POLICY]), 0.0))
    # Keyed off `policy` (not a Sustain-fallback key) so `tame` — absent from FORECAST_CEILING_KEYS —
    # is still recognized as the investment rung it is. For every other rung `policy` IS its ceiling key,
    # so this is identical to the old `policy_key in …` test.
    var investment: bool = policy in FORECAST_PAYOFF_KEYS
    var payoff := 0.0
    if investment:
        payoff = float(src.get(prefix + String(FORECAST_PAYOFF_KEYS[policy]), 0.0))
    # The rung's RUNNING COST (Corral only — the pen's feed). `feed_rung` says the payoff is a GROSS
    # figure that a per-turn cost is paid out of; `feed` is that cost, and is 0 — i.e. unknown, not
    # free — while the herd is still un-penned (see FORECAST_FEED_KEYS).
    var feed_rung: bool = policy in FORECAST_FEED_KEYS
    var feed := 0.0
    if feed_rung:
        feed = float(src.get(prefix + String(FORECAST_FEED_KEYS[policy]), 0.0))
    # WHOLE-ANIMAL HUNT: a take of whole animals via a kill-credit bank (`food_per_animal` = one animal's
    # yield in food; 0/absent for a forage patch). The peak-turn carry need is quantized to whole bodies
    # (see `max_useful_workers`), so it must fire ONLY for an extractive hunt of a live herd — never a
    # forage patch (no food_per_animal), an investment rung (Tame/Corral collapse to 1), or a corralled
    # herd (managed `worker_tend` harvest, whose forecast already collapses every ceiling to per_worker).
    var food_per_animal := float(src.get(prefix + "food_per_animal", 0.0))
    var whole_animal: bool = food_per_animal > 0.0 and not investment \
        and not bool(src.get("corralled", false))
    return {
        "per_worker": per_worker,
        "ceiling": ceiling,
        "payoff": payoff,
        "investment": investment,
        "feed_rung": feed_rung,
        "feed": feed,
        "food_per_animal": food_per_animal,
        "whole_animal": whole_animal,
        "known": per_worker >= FORECAST_MIN_PER_WORKER,
    }

## Workers beyond this produce nothing at this source under the selected policy —
## ceil(ceiling / per_worker). MAX_USEFUL_UNBOUNDED when there's no forecast data. A tended patch /
## corralled herd reports every ceiling == per_worker, so this collapses to 1 (policy irrelevant).
static func max_useful_workers(forecast: Dictionary) -> int:
    if not bool(forecast.get("known", false)):
        return MAX_USEFUL_UNBOUNDED
    var per_worker := float(forecast["per_worker"])
    var ceiling := float(forecast["ceiling"])
    # WHOLE-ANIMAL HUNT: the cap is the carriers needed to HAUL the animals that drop on the worst turn,
    # not ceil(smoothed-rate / per_worker). An 80-biomass aurochs drops all at once; one hunter carrying
    # <per_worker> food wastes the rest, so the smoothed rate under-counts. Worst case the kill-credit
    # bank holds just under one body when the turn's rate lands, so floor(ceiling / food_per_animal) + 1
    # whole animals drop, each worth food_per_animal — carry that peak, not the average flow.
    var food_per_animal := float(forecast.get("food_per_animal", 0.0))
    if bool(forecast.get("whole_animal", false)) and food_per_animal > 0.0:
        var animals := floori(ceiling / food_per_animal) + HUNT_PEAK_DROP_BANK_BONUS
        var peak_drop_food := float(animals) * food_per_animal
        return ceili(peak_drop_food / per_worker)
    return int(ceilf(ceiling / per_worker))

## The take `workers` would ACTUALLY produce here: min(workers × per_worker, ceiling), scaled by the
## acting band's output multiplier (the sim exports the forecast at 1.0).
static func expected_yield(forecast: Dictionary, workers: int, band: Dictionary) -> float:
    var raw := minf(float(workers) * float(forecast.get("per_worker", 0.0)),
        float(forecast.get("ceiling", 0.0)))
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
    if bool(m.get("has_yield", false)):
        var actual := float(m.get("actual_yield", 0.0))
        var sustainable := float(m.get("sustainable_yield", 0.0))
        # A source overdraws when its take draws the stock below what it sustains. This is the
        # sim-answered `overdraws` flag (policy-driven: `!managed && policy.overdraws()`), NOT the
        # client-derived `actual > sustainable` — which false-positives on a hunt's kill turn (cashing a
        # banked whole animal spikes `actual` above the steady `sustainable` even under Sustain). Forage
        # on Sustain reads clean; a Surplus/Market/Eradicate patch or an over-hunted herd trips ⚠.
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
        label_suffix = " %s" % format_yield(rate)
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
    }

## A hunt source is MANAGED (its crew are herders/keepers, not a hunt party) once the herd is penned,
## fully tamed (pastoral), or being penned under the composed Corral policy. `workersNeeded` on such a
## source scales with the HERD (max herders, haulers), so the crew label must read as herders.
static func is_managed_hunt_source(herd: Dictionary, policy: String) -> bool:
    return bool(herd.get("corralled", false)) \
        or float(herd.get("domestication", 0.0)) >= DOMESTICATION_COMPLETE \
        or policy == LABOR_POLICY_CORRAL

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
            # Fodder crops pay hay, not provisions — carried through so the picker row can show the
            # hay value in place of the 0× provisions ratio a fodder crop would otherwise read.
            "sow_fodder_payoff": float(entry.get("sow_fodder_payoff", 0.0)),
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
    # A denial mission (eradicate) delivers no food BY DESIGN — never a payload, never a failure. This
    # carve-out MUST come first: it takes animals (down to the 0 floor) but banks none as food.
    if not bool(estimate.get("delivers_food", false)):
        return {"available": true, "denial": true, "empty": false}
    # delivered_food == 0 = the herd is at/below the policy's floor: no standing surplus to raid, the
    # party returns empty. The ONE non-viable case (the raid always completes; the herd has nothing).
    # NOT `animals_taken == 0`: a party too small to carry a whole animal now KILLS one and hauls the
    # fraction its pack holds (mirroring the local hunt), so `animals_taken >= 1` whenever there's any
    # surplus — the delivered PAYLOAD (with waste) is the honest bind, not the whole-animal kill count.
    var delivered_food := float(estimate.get("delivered_food", 0.0))
    if delivered_food <= 0.0:
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
        # whole kill and overstates a partial). Guaranteed > 0 here (empty returned above otherwise).
        "food": int(round(delivered_food)), "waste_pct": waste_pct,
    }

## Render a `hunt_trip_forecast` result as its one-line BBCode readout — the three states in their
## three colors (cyan viable / amber too-slow / red returns-empty), or "" when the forecast isn't
## available (a herd with no exported estimate → the caller shows no line at all). SHARED by both hunt-expedition entry
## points: the targeting banner (band-first flow) and the herd panel's live compose block (herd-first
## flow), so the two can never drift apart.
static func hunt_forecast_line_bbcode(forecast: Dictionary, herd_name: String) -> String:
    if not bool(forecast.get("available", false)):
        return ""
    # A denial mission (Eradicate) brings nothing home BY DESIGN — say what it does, amber, no payload.
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
    # A real raid: headline the delivered PAYLOAD (the animal count over turns + the food it LANDS), then
    # the waste. `food` is the sim's `delivered_food` (always set on a delivering forecast).
    var animals := int(forecast.get("animals", 0))
    var food: String = HUNT_FORECAST_FOOD_FORMAT % int(forecast["food"]) if forecast.has("food") else ""
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
        var delivered := float((cell_variant as Dictionary).get("delivered_food", 0.0))
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
## the four read ASCENDING (Sustain < Surplus < Market < Eradicate; deeper floors free more surplus). The
## metric is WORKER-INDEPENDENT: the max over every party size of `delivered_food / trip_turns`, where
## `trip_turns = turns_to_fill + round-trip travel` (a far herd's best rate is correctly lower). A bigger
## party delivers more food in fewer turns, so the rate rises then plateaus — the max is the honest cap.
## Eradicate is a DENIAL rung (`delivers_food == false`, `delivered_food == 0`): it never qualifies, so it
## carries no rate and falls back to its name + skull glyph — its existing denial treatment. A table SCAN,
## zero client arithmetic. Empty when the herd carries no estimates (older snapshot / non-huntable).
static func expedition_policy_takes(band: Dictionary, herd: Dictionary,
        grid_width: int, wrap_horizontal: bool) -> Dictionary:
    var takes := {}
    var estimates_variant: Variant = herd.get(HERD_TRIP_ESTIMATES_KEY, {})
    if not (estimates_variant is Dictionary):
        return takes
    var estimates := estimates_variant as Dictionary
    var travel := round_trip_travel_turns(band, herd, grid_width, wrap_horizontal)
    for policy in LABOR_HUNT_POLICIES:
        var best_rate := -1.0
        for key in estimates:
            var parts := String(key).split(HUNT_ESTIMATE_KEY_SEPARATOR)
            if parts.size() != 2 or String(parts[0]) != String(policy):
                continue
            var cell: Dictionary = estimates[key]
            if not bool(cell.get("delivers_food", false)):
                continue
            var delivered := float(cell.get("delivered_food", 0.0))
            var trip_turns := int(cell.get("turns_to_fill", 0)) + travel
            if delivered <= 0.0 or trip_turns <= 0:
                continue
            best_rate = maxf(best_rate, delivered / float(trip_turns))
        if best_rate >= 0.0:
            takes[String(policy)] = extractive_take(best_rate)
    return takes

## Style the hunt-expedition send button from the live forecast. Two treatments, and the line between
## them is the point:
##   DELIVERING (viable / slow / long / denial) — the raid lands something (animals, or the denial it
##     promises). "primary" for a brisk raid; "armed" amber for a slow/long raid (`Send Anyway (≈54
##     turns)` / `Send Anyway (long raid)`) or a denial (`Send (delivers no food)`) — ENABLED either
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
        # Eradicate: no food comes home, but that IS the mission — state the deal, don't cry failure.
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
