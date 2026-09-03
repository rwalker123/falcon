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
## `BandPanelController` precedent — a one-line predicate is not worth a Callable). The `FactionReadouts`
## reference the dormant Fodder row's hover needs is a TYPED collaborator, not a fourth Callable —
## the same cluster `BandPanelController` and `DrawerComposeController` already hold by type.
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

# ---- The band's FODDER larder row, beneath Food, on EVERY player band — one whose larder is live
# (it holds hay, or its pens owe a bill) states the two terms below; one whose larder is not yet a
# thing states the DORMANT form further down this block.
#
# **IT IS THE FOOD ROW, BEAT FOR BEAT**: a two-term summary — the STOCK and the RUNWAY — with the
# flows that move it in a click-to-open disclosure beneath.
#
#   `Fodder: 100.0  (100 turns)`
#     ▲ +5.0  Grown
#     ▼ -6.0  Pens
#
# **THE RATES CAME OFF THE ROW, AND THAT IS A SHAPE FIX RATHER THAN A WIDTH ONE.** The row stated
# `Fodder: 100.0 · need 6.0/turn · growing 5.0/turn · 100 turns` and wrapped to two lines in the
# narrow drawer column for it — carrying on ONE line what the Food row has always split between a
# summary and a pull-down, in a client whose disclosure module documents at the top of its own file
# that breakdown rows are NEVER appended inline. The pair still matters for exactly the reason it was
# added (a pen's fenced footprint has a fixed carrying capacity and its herd does not, so a
# self-feeding pen goes fodder-dependent as the herd grows), and the disclosure is where it now says
# so: see `DisclosureController.fodder_breakdown_lines`.
#
# **THE LABEL IS THE REGISTRATION KEY**, so the row is spelled from `DETAIL_ROW_FODDER` rather than
# typed again: a renamed row that still registered under the old key would lose its caret silently.
# The runway is `DetailFormat.food_turns_text` over `turns_of_fodder` — which the sim computes with
# the very function it computes `turns_of_food` with, **999 and all** — so `∞` here means what it
# means there (this larder is not draining) and there is no second constant, no second branch and no
# second phrasing of "turns of buffer left" anywhere in the client. The whole value cell tints through
# `BandFoodStatus.hex_for_turns` off the context, exactly as the Food row's does.
#
# **AND THE `need` CLAUSE'S AMBER WENT WITH IT.** The runway already says a larder is draining — a
# finite, shrinking number under the shared thresholds — so a separate warn on one term was a second
# rule for one idea, and dropping it is what stops the two larders disagreeing about what worrying
# looks like. `fodder_is_concerning` (the food test, on this account) tints the caret and nothing else.
const BAND_FODDER_ROW_FORMAT := HudDisclosureVocab.DETAIL_ROW_FODDER + ": %s  (%s)"

# ---- **THE DORMANT FODDER ROW LIVES ON `DetailFormat` NOW**, vocabulary and producer both
# (`FODDER_DORMANT_ROW_FORMAT` / `FODDER_DORMANT_VALUE` / `FODDER_LOCKED_TOOLTIP_FORMAT` /
# `FODDER_DORMANT_TOOLTIP` / `fodder_dormant_row`). It moved the moment the FACTION page grew the
# same state: a const lives where every one of its readers can reach it, and the rollup is a static
# layer that must not reach into a stateful producer. One row builder for both scales is also what
# stops the band's dim dash and the faction's coming to mean different things.

# ---- The SAME fodder stock as a CLAUSE on the Food row, for the `compact` (SHORT band-zone tier)
# host. A horizontal dock is short of HEIGHT and has width to spare, so the two larders share one line
# there; a vertical dock is short of WIDTH and keeps them as two rows. **The word is `fodder`, the
# word its own standalone row and the pen's `Fed:` row use** — it read `hay` while the pen rows did,
# and one larder called two things across two tiers of the same panel is the confusion that sweep
# removed. The stock keeps the Fodder row's ONE decimal (`SourceForecast.format_fodder`). Lowercase,
# which is also what keeps `band_panel_preview`'s merge guard honest: the standalone row's key
# `Fodder` must be ABSENT wherever this clause fired, and the two are told apart by case. It carries
# its OWN colour
# rather than inheriting the Food row's value tint: a starving band's hay stock is not itself a red
# reading, and the net rate beside it sets the precedent for a self-tinted run inside that value cell.
#
# **THAT COLOUR IS THE SHORTFALL WARN, and it is the whole of what this tier can say about the trap.**
# The stock alone cannot carry the need/growing pair — this host merges rows precisely because it has
# no height — but the gate that puts the clause on screen now admits a band with a hay BILL and an
# empty store, and `0.0 hay` stated in neutral ink beside a bill it cannot pay would read as *fine*.
# So the clause takes `HudStyle.WARN_HEX` when `_band_fodder_falls_short` says the larder is draining
# — the same crossing-inclusive net the `Fodder:` caret is tinted from, so this tier and the tall ones
# cannot disagree about one larder — and the tall tiers carry the numbers that explain it.
const BAND_FOOD_FODDER_CLAUSE_FORMAT := " · [color=#%s]%s fodder[/color]"

# ---- THE BAND'S STANDING MATERIAL BILL, beneath the two larders (`docs/plan_standing_upkeep.md`
# §2.7) — `Upkeep: 2 hurdles  (7 turns)`. What the things this band has BUILT cost it to keep, in
# goods: a pen frays its fence every turn it stands, a road washes out. Work was never the whole
# price.
#
# **IT IS THE FODDER ROW, BEAT FOR BEAT**: a two-term summary — the STOCK and the RUNWAY — with the
# flows that move it in a click-to-open disclosure beneath.
#
#   `Upkeep: 2 hurdles  (7 turns)`
#     hurdles
#       ▼ -0.05  Wanted
#       ▲ +0  Arriving
#       2  On the shelf
#
# ⛔ **THE VALUE NAMES ONE GOOD, AND THAT IS WHAT KEEPS IT HONEST.** Six hurdles and two rope are not
# eight of anything; a summed materials figure here would be the retired `Trade:` scalar rebuilt out
# of its own replacement, which is the flattening the whole materials model exists to refuse. The good
# quoted is the one in the WORST state — the shortest runway, i.e. the one that runs out first and
# therefore the one a player has to act on — and the rest are one click down.
#
# **THE LABEL IS THE REGISTRATION KEY**, `DETAIL_ROW_UPKEEP` rather than typed again: a renamed row
# that still registered under the old key would lose its caret silently. The runway is
# `DetailFormat.food_turns_text` over the worst good's shelf-against-gap, so `∞` here means what it
# means on both larder rows (this bill is not draining) and there is no second constant, no second
# branch and no second phrasing of "turns of buffer left" anywhere in the client. The whole value cell
# tints through `BandFoodStatus.hex_for_turns` off the context, exactly as the two larder rows do —
# which is how a short good takes the danger ink without a second severity rule beside it.
const BAND_MATERIAL_UPKEEP_ROW_FORMAT := HudDisclosureVocab.DETAIL_ROW_UPKEEP + ": %s  (%s)"

## ONE GOOD'S AMOUNT AND ITS NAME — `2 hurdles`, `0.05 hurdles`. **A material names itself**: the
## catalogue ships no display word, so the id IS the noun (`SourceForecast.PICKER_MATERIAL_PRODUCT_FORMAT`'s
## rule), and the amount is trimmed so a shelf reads `2` while a mending rate reads `0.05`.
const BAND_MATERIAL_TERM_FORMAT := "%s %s"

# ---- THE GROWTH ROW AS A CLAUSE ON THE MORALE LINE, for the `compact` (SHORT band-zone tier) host —
# the second merge this tier makes, and the same trade for the same reason as the hay clause above:
# HEIGHT is what is scarce in a height-capped horizontal dock, and it has a whole screen of width.
#
# **MORALE AND GROWTH ARE THE RIGHT PAIR.** Both are player-band health scalars, both already carry
# disclosure carets, and they read naturally together. The alternative — dropping a row — is not
# available here: the one row this tier used to drop, `Trade`, is retired outright (arc #527), and the
# row it gained after that, `Gear`, is retired too (`docs/plan_standing_upkeep.md` §4.9 item 12) — its
# height went straight to the `Upkeep:` standing bill, which is stated nowhere else in this client.
# So there is nothing left to give up.
#
# **BOTH `[url]` METAS SURVIVE, which is the whole reason a merge beats a drop.** The vitals block is
# ONE `RichTextLabel`, so a row is a line and merging two is joining two strings: the Growth clause
# carries the identical clickable run a standalone Growth row wears
# (`DetailFormat.inline_disclosure_label`), on the same label, so both popovers keep working and
# neither breakdown is lost. It carries its OWN tint rather than inheriting the morale value cell's,
# exactly as the hay clause does — a falling band's growth is not itself a red reading.
#
# **THE SEPARATOR IS LOAD-BEARING BEYOND ITS LOOK.** Followed directly by `DISCLOSURE_URL_OPEN` it is
# what tells a merged clause from a standalone row structurally, which is the only assertion that can
# catch this tier's layout leaking into the tier above it.
const BAND_MORALE_GROWTH_CLAUSE_SEPARATOR := " · "
const BAND_MORALE_GROWTH_CLAUSE_FORMAT := BAND_MORALE_GROWTH_CLAUSE_SEPARATOR + "%s [color=#%s]%s[/color]"

# **THE BAND'S TRADE ROW IS RETIRED** (`Trade: 12.0 · +0.04 /turn`, arc #527). It read the
# `trade_goods` store key and the `trade_yield` rates, both of which the sim has stopped writing, so
# the row would have stood at a permanent `Trade: 0.0 · +0.00 /turn` on every band forever. What a
# band holds beyond food and fodder is MATERIALS — one pile per material per rating — and that is a
# LIST, not a vitals row: the Crafting panel's rail is where it reads, and no summary line here may
# collapse it back into one number.

# ---- **THE BAND'S `Gear` ROW IS RETIRED** (`docs/plan_standing_upkeep.md` §4.9 item 12), with
# `BAND_KIT_ROW_PREFIX` / `_SEPARATOR` / `_ENTRY_FORMAT`, `_band_kit_line`, its `BAND_KIT_ROW_MAX_ENTRIES`
# budget and the 22px `Zone_band` measurement that budget existed to respect.
#
# **IT DID NOT COMPRESS TO A LINE, AND SOMETHING ELSE ALREADY OWNS IT.** The row was a bounded summary
# of an unbounded list — three of however many items the server publishes, ordered dry-first, with the
# rest hidden behind a caret — because a fourth entry wrapped and overflowed the zone. The CRAFTING
# panel's kit ledger states every item in full, with room for its condition and what it does; the
# Builders card's own gear line was retired in §4.7 for exactly this reason.
#
# **WHAT REPLACES IT IS NOTIFICATION, NOT ANOTHER ROW.** `equipment.json`'s `life_readout` seams
# (`warn_fraction` 0.34, `danger_fraction` 0.10) now reach the event dock as `kit_life` — warn →
# Notable, danger → Alert — so a kit approaching its cliff announces itself instead of waiting to be
# read off a vitals block.
#
# ⛔ **THE `DetailFormat` KIT LEAVES STAY** (`band_states_kit`, `kit_coverage`, `kit_condition_face`,
# `kit_is_equipped`, the label and durability tables): the crafting panel's ledger and the compose
# sheet still read every one of them, and `DisclosureController.kit_breakdown_lines` still composes
# the popover the crafting surface opens.

# ---- The hunt party's carry-ceiling FULL badge (shown when carried ≥ cap; the party heads home full).
const HUNT_FULL_BADGE := "· FULL"

# ---- THE PACK'S MATERIALS — a CLAUSE on the `Carried:` row, and never a row of its own -----------
# `PopulationCohortState.materialBatches` is resolved from `cohort.stores` with NO resident-band gate,
# so a detached party's carried materials have been on the wire the whole trip and nothing rendered
# them: a scout hauled a wolf home and the UI never mentioned the hide.
#
# **IT IS A CLAUSE BECAUSE THE STRIP HAS NO EIGHTH LINE TO GIVE.** The parties inspector strip's
# budget is fully spent at SEVEN lines in a ~300px clipping zone (`band-city-panel.md` → "The parties
# strip's SEVEN lines"), and that section's own rule for a new fact is the band zone's SHORT-tier
# idiom: two facts that read as one sentence cost one row. What the party is carrying home IS the
# `Carried:` sentence.
#
# **A BATCH IS NOT A PAYOFF ROW, AND THE DIFFERENCE IS WHY THEY ARE NOT MERGED.** Everything else this
# arc renders is `{material_id, amount}` — one figure per material. A BATCH is one pile of one
# material AT ONE RATING, carrying a characteristic vector: two piles of `hide` at different readings
# are two batches and two terms, because a mammoth hide and a hare pelt are both `hide` and are not
# the same thing. Summing them is the retired trade scalar rebuilt out of its own replacement.
#
# **THE PER-AXIS READINGS ARE THE CRAFTING PANEL'S REGISTER, deliberately not restated here.** That
# panel already renders every batch's amount beside its `tough: excellent` chips, for the band this
# party folds back into; this row answers *what is coming home and how much*, in a clipping strip that
# cannot afford a characteristic vector per pile. If piles of one material become common on a party,
# the fix is the batch's band NAME inside its term — never a sum.
const PARTY_PACK_CLAUSE_PREFIX := " · "
const PARTY_PACK_ENTRY_FORMAT := "%s %s"

# ---- A SHIPMENT'S OWN TWO ROWS (arc #527) --------------------------------------------------------
# A trade party's readout is the two questions a shipment raises and no others: WHO it is for, and
# WHAT it is carrying. It has no quarry, no floor, no delivery ETA and no trip bound — the mission
# carries no such levers — so every hunt-gated row stays gated and none of them is borrowed here.
#
# The destination is the sim's `expeditionDestinationName`, resolved at LAUNCH and carried, rendered
# verbatim. Its key twin `expeditionDestinationBand` is what the command addresses and NEVER appears.
const TRADE_DESTINATION_ROW := "Bound for"
# `Carrying: 23.2 / 24.0 · 4.0 hide · 1.2 bone` — the shipment beside the pack it fills, and the
# materials as ONE TERM PER MATERIAL, never summed (`_shipment_cargo_clause`). The cap is
# `expedition_carry_cap`, which resolves per MISSION, so this quotes the shipment pack rather than a
# hunt's provisions ceiling.
#
# **THE NUMBER BEFORE THE SLASH IS THE WHOLE PACK'S MASS** (`DetailFormat.shipment_cargo_mass` —
# 18 food + 4 hide + 1.2 bone at a carry weight of 1.0 is the 23.2 above), never the food term alone:
# the cap is what the sim weighs `food + weight × Σ materials` against, and the materials trailing it
# are the SPELLING of that mass, not a second cargo beside it.
const TRADE_CARGO_ROW := "Carrying"
const TRADE_CARGO_CAP_FORMAT := "%s: %s / %s%s"
const TRADE_CARGO_NO_CAP_FORMAT := "%s: %s%s"

# ---- Morale-trend arrow glyphs. Whether a trend reads as flat at all is `DetailFormat.MORALE_TREND_EPSILON`,
# which stays there — `DetailFormat.morale_is_concerning` tests it too.
const MORALE_TREND_FALLING_GLYPH := "▼"
const MORALE_TREND_RISING_GLYPH := "▲"

# ---- Morale-breakdown contribution labels this producer names. (The SIGN glyphs and the indent stay
# on `HudLayer` — `DetailFormat` renders indented sub-lines from them too.) A positive unrest
# contribution reads as "culture" (cohesion), negative as "unrest".
const MORALE_CONTRIB_LABEL_SETTLING := "settling"
const MORALE_CONTRIB_LABEL_CULTURE := "culture"

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
# The snapshot herd list, for a hunt party's migrating target.
var _band_labor: HudBandLaborState = null
# The Food/Morale caret + popover cluster. `unit_summary_lines` clears its rows, registers the two
# disclosures as it emits them, and reads the caret state back onto the render context.
var _disclosures: DisclosureController = null
# The FACTION-scope readout cluster, for ONE question: how far along is the player's Foddering? A
# band's dormant `Fodder:` row states the live percent when the craft is what is missing, and
# knowledge is held faction-scoped — no band dict carries it. A TYPED collaborator, exactly as
# `BandPanelController` and `DrawerComposeController` hold the same cluster for their own gate
# reasons, so the class header's "the injection surface is ONE CALLABLE" is unchanged. Read for
# nothing else.
var _topbar: FactionReadouts = null

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
        herd_label_for_id: Callable, topbar: FactionReadouts = null) -> void:
    _band_labor = band_labor
    _disclosures = disclosures
    _herd_label_for_id_fn = herd_label_for_id
    _topbar = topbar

## The player faction's progress on ONE knowledge track, 0..1 — the dormant Fodder row's only reach
## outside the band dict. `0.0` with no readouts cluster, which is what the preview harnesses that
## construct this producer bare get, and which reads as "not learned" — the honest answer for a
## client that has been told nothing.
func _player_knowledge(track: String) -> float:
    return _topbar.faction_knowledge(HudConst.PLAYER_FACTION_ID, track) if _topbar != null else 0.0

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
## `compact` is the SHORT band-zone tier saying **HEIGHT is what is scarce here, not width** — which is
## exactly the horizontal (T/B) dock, whose band zone is height-capped and CLIPS rather than scrolls
## while having a whole screen of width to spend. It is the row-level twin of
## `BandPanelController.build_band_zone`'s gate on `_build_food_outlook_block`, and it buys two rows:
##   * the Fodder row is MERGED onto the Food line as a hay clause rather than dropped — a hay stock
##     has no other home, and one wider line is exactly the trade this host wants to make.
## (It used to buy a second row by dropping the Trade row; that row is retired outright, arc #527.)
## Defaults false, so the drawer host and both harnesses are unaffected.
## `with_position` is the host saying whether it has somewhere ELSE to state the band's coordinates.
## The Band/City dock does — they are IDENTITY, so they live in its panel header beside the stage
## word (`BandCityPanel.set_header`), and repeating them as a vitals row cost that height-capped zone
## a row it could not spare. The Occupants-card drawer does NOT: it renders FOREIGN bands, whose
## position is nearly all we can honestly observe, and it has no header to carry it — so it keeps the
## row and this defaults true. **Deliberately its own parameter and NOT keyed off `compact`**, which
## is the band zone's HEIGHT TIER: the dock drops this row in every tier, tall or short.
## `denial_view` is carried straight through to `expedition_summary_lines` — a party selected on the map
## renders through THIS entry point, so the drawer host answers the same query the dock's parties strip
## does. See that producer for why the answer is a parameter.
func unit_summary_lines(unit_data: Dictionary, terrain_label: String,
        ctx: DetailFormat.Context = null, compact: bool = false,
        with_position: bool = true, denial_view: Dictionary = {}) -> Array[String]:
    # The tint context is an OUT-PARAMETER of this producer, not a member: the caller (each of the two
    # detail hosts) builds it and hands it straight to the formatter. Defaulted so the preview
    # harnesses can still ask for the lines alone.
    var context := ctx if ctx != null else DetailFormat.Context.new()
    if bool(unit_data.get("is_expedition", false)):
        return expedition_summary_lines(unit_data, context, denial_view)
    var lines: Array[String] = []
    # Disclosure carets + the tint context are rebuilt per render. Reset BOTH here, not inside
    # `_band_food_line` — a foreign band skips that call entirely (below), and a skipped Food row
    # must not inherit the previous render's caret or its food-turns tint.
    _disclosures.clear_rows()
    _food_flow_present = false
    context.food_turns = NAN
    # …and the fodder runway beside it, for the same reason and with the same reach: a band with no
    # fodder economy emits no Fodder row, and a stale runway from the last band rendered would tint a
    # row that is not there — or, worse, the next band's.
    context.fodder_turns = NAN
    # …and the standing bill's runway, for exactly the same reason: a band that owes no goods emits no
    # `Upkeep:` row, and last render's tint would colour a row that is not there — or the next band's.
    context.material_turns = NAN
    # Food, like Morale below, is our OWN bands' business only. A rival's cohort carries no
    # `turns_of_food`/`stores` on the wire, so rendering the row for one printed a FABRICATED
    # `Food 0 (∞)` in healthy green — the UI claiming we'd counted a larder we cannot see. A foreign
    # band shows only what we can honestly observe from outside: where it is (Position) and roughly
    # how many (its roster row's size).
    if _is_player_unit(unit_data):
        lines.append(_band_food_line(unit_data, context, compact))
        # Category-aggregated food breakdown under Food: a click-to-open disclosure. `_band_food_line`
        # set `_food_flow_present` (a PRIVATE handshake between the two — the formatter never reads
        # it); `DisclosureController.register` stashes the rows for the popover and records the row so
        # the formatter draws the caret + clickable meta. The rows are NEVER appended here — inline
        # growth is what clipped the zone.
        if _food_flow_present:
            _disclosures.register(HudDisclosureVocab.DETAIL_ROW_FOOD, HudDisclosureVocab.BREAKDOWN_KIND_FOOD, unit_data,
                _disclosures.food_breakdown_lines(unit_data))
        # The band's fodder (hay) larder, beneath its food larder — **on every one of our bands**,
        # live where the band HAS stockpiled hay or OWES its pens one, dormant where it does not.
        # An always-present row is what makes the account discoverable before a player has one; the
        # gate that used to hide it now picks the form. See `DetailFormat.FODDER_DORMANT_ROW_FORMAT`.
        # **In the `compact` tier it is not a row at all**: `_band_food_line` has already carried the
        # stock as a clause on the Food line, because the SHORT tier's scarcity is HEIGHT and that
        # host has width to spend. That clause stays GATED — the tier trades this row for a stock,
        # and a dim `— fodder` clause states nothing the row it stands in does not. See
        # `BAND_FOOD_FODDER_CLAUSE_FORMAT`.
        if not compact:
            if DetailFormat.band_has_fodder_economy(unit_data):
                lines.append(_band_fodder_line(unit_data, context))
                # …and its two flows in the SAME click-to-open popover Food uses, registered rather
                # than appended for the identical reason: inline growth in a fixed-height zone is what
                # clipped the Band panel once already. A larder with neither flow above the floor
                # registers no disclosure at all — `register` declines an empty payload — so a caret
                # never promises rows that are not there.
                _disclosures.register(HudDisclosureVocab.DETAIL_ROW_FODDER,
                    HudDisclosureVocab.BREAKDOWN_KIND_FODDER, unit_data,
                    _disclosures.fodder_breakdown_lines(unit_data))
            else:
                # **NOTHING IS REGISTERED HERE**, deliberately: there is no flow to put behind a
                # caret, so the row renders as a plain dim key with no clickable run at all.
                lines.append(_band_fodder_dormant_line(context))
        # **THE STANDING MATERIAL BILL, BESIDE THE TWO LARDERS** (`docs/plan_standing_upkeep.md`
        # §2.7) — what this band's holdings swallow every turn in GOODS. A pen frays its fence; a
        # road washes out. It is the Fodder row beat for beat: one summary line naming the good in
        # the worst state, and the three terms that explain it — wanted, arriving, on the shelf —
        # in the click-to-open popover beneath, one block per good.
        #
        # **IT SURVIVES THE `compact` TIER, AND THE ROW IT SPENDS IS `Gear`'s.** The SHORT tier is
        # short of HEIGHT, so a row added there has to come from somewhere — and the row this slice
        # retired is exactly the one that tier had gained. Net zero rows in every tier, and the tier
        # keeps a fact rather than trading one away.
        #
        # ⛔ **NO DORMANT FORM, WHICH IS THE ONE PLACE IT DIVERGES FROM FODDER.** A band with no
        # fodder economy still gets a dim `Fodder: —` because there is a *"you could have this"*
        # story to tell — the Foddering craft is a thing to go and learn. A band holding nothing
        # that eats a good has no such story: the bill is a CONSEQUENCE of what you have built, so
        # a row promising one before anything is built would be a readout for an economy the
        # player has not chosen to have. It renders no row at all.
        #
        # **AND NOTHING IS REGISTERED WHEN THERE IS NO BILL** — `register` declines an empty
        # payload, and a caret must never promise rows that are not there.
        if DetailFormat.band_has_material_upkeep(unit_data):
            lines.append(_band_material_upkeep_line(unit_data, context))
            _disclosures.register(HudDisclosureVocab.DETAIL_ROW_UPKEEP,
                HudDisclosureVocab.BREAKDOWN_KIND_UPKEEP, unit_data,
                _disclosures.material_upkeep_breakdown_lines(unit_data))
    # Morale is our own bands' business only (a non-player band's morale isn't ours
    # to see); morale drives productivity + migration (a harsh tile erodes it until
    # people begin leaving), while deaths stay starvation/cold-driven.
    if _is_player_unit(unit_data):
        # **BUILT, THEN REGISTERED, THEN APPENDED — in that order, because the merge needs all three.**
        # The SHORT tier joins Growth onto this line, and the clause carries Growth's own clickable
        # run, which does not exist until `register` has recorded its caret state. So the morale line
        # is held rather than appended, both disclosures are registered, and only then does the tier
        # decide whether this is one line or two.
        var morale_line := _band_morale_line(unit_data, terrain_label, context, compact)
        # **NO `Output:` ROW.** Productivity ties visibly to morale, but the multiplier's CONSEQUENCE
        # is the work board: every rate the WORK zone shows is already scaled by it. So it renders as
        # an item of that zone's head (`BandPanelController._build_work_head`), under the same
        # below-full gate, and this height-capped column keeps the row.
        # Itemized morale breakdown: the SAME click-to-open disclosure as Food, in the same popover.
        # Only offered when there's actually a breakdown to show (a contribution above the epsilon, or
        # the concerning recovery line) — `register` declines an empty payload.
        _disclosures.register(HudDisclosureVocab.DETAIL_ROW_MORALE, HudDisclosureVocab.BREAKDOWN_KIND_MORALE, unit_data,
            _morale_breakdown_lines(unit_data, terrain_label))
        # Growth: the birth path's parallel of the morale cluster above — a headline row plus the
        # SAME click-to-open disclosure, itemizing the three named fertility factors. The player
        # could already see both the inputs (Food, larder) and the effect (the People bar); this is
        # the attribution between them. Skipped entirely for a band the sim has published no reading
        # for — a rehydrated cohort has no fertility yet, and inventing a 0% growth row for it would
        # be the very "no data read as famine" mistake the sim guards against on its own side.
        var growth_line := _band_growth_line(unit_data, context)
        if growth_line != "":
            _disclosures.register(HudDisclosureVocab.DETAIL_ROW_GROWTH, HudDisclosureVocab.BREAKDOWN_KIND_GROWTH,
                unit_data, _fertility_breakdown_lines(unit_data))
        # THE TIER DECIDES: two rows, or one line carrying both. `compact` is the SHORT band-zone
        # tier — see `BAND_MORALE_GROWTH_CLAUSE_FORMAT` for why this pair and why a merge rather than
        # a drop. A band with no published growth reading merges nothing and keeps its bare Morale
        # row, in every tier.
        if compact and growth_line != "":
            lines.append(morale_line + _band_growth_clause(unit_data, context))
        else:
            lines.append(morale_line)
            if growth_line != "":
                lines.append(growth_line)
    if with_position:
        var pos_array: Array = Array(unit_data.get("pos", []))
        if pos_array.size() == 2:
            lines.append("Position: (%d, %d)" % [int(pos_array[0]), int(pos_array[1])])
    # Per-source labor is now shown by the allocation panel (a real −/+ control set),
    # not as drawer text; the old single-task harvest/scout summaries are retired.
    #
    # **THE `Stockpile: radius N` / `Available: …` ROWS ARE GONE** (issue #381). They read as the
    # band's own reachable stores and were nothing of the kind: `accessible_stockpile_state`
    # (`core_sim/src/snapshot/population.rs`) returns the WHOLE faction stockpile, gated only on the
    # band sitting within `stockpile_access_radius` of the faction's START position — a half-built
    # proximity idea whose shipped radius is 0, so the rows showed for a band that had not left the
    # start hex and vanished the moment it moved. Beside the (since-retired) Trade row on the same
    # panel they printed the same faction number twice, one wearing a meaningless `radius 0`.
    # The carets this render registered are the LAST thing the context needs; read them back here so
    # every caller gets a fully-filled context by simply passing it in.
    context.disclosures = _disclosures.state()
    return lines

## Drawer readout for a selected expedition (docs/plan_exploration_and_sites.md §2 / §2b):
## **THE PARTY'S PACK, PER BATCH** — ` · 2.4 hide · 1.1 hide`, or `""` when it carries none, which is
## every scouting party that has walked past nothing worth skinning. The empty answer is what keeps
## the `Carried:` row byte-identical for the parties this does not concern.
##
## **ONE TERM PER BATCH, NEVER MERGED BY MATERIAL** — see `PARTY_PACK_ENTRY_FORMAT` for why two piles
## of `hide` are two terms and what would be lost by adding them. The reader's cue that they are
## piles rather than an accounting error is the same ` · ` this HUD spends on separating accounts
## everywhere else.
##
## **THE BATCH KEYS ARE THE CRAFTING PANEL'S OWN** (`HudCraftingVocab`), read rather than re-declared:
## one vocabulary for one wire shape is what stops a party's pack and the band's rail naming the same
## pile two ways. The amount likewise wears `BATCH_AMOUNT_FORMAT`, so a pile reads the same to one
## decimal wherever it is quoted.
##
## A batch naming no material is dropped: an id is what a term is FOR, and a nameless amount could
## only be rendered as the summed scalar this arc exists to refuse.
func _party_pack_clause(unit_data: Dictionary) -> String:
    var terms: Array[String] = []
    for batch_variant in unit_data.get(HudCraftingVocab.BAND_MATERIAL_BATCHES_KEY, []):
        if not (batch_variant is Dictionary):
            continue
        var batch: Dictionary = batch_variant
        var material_id := String(batch.get(HudCraftingVocab.BATCH_MATERIAL_ID_KEY, "")).strip_edges()
        var amount := float(batch.get(HudCraftingVocab.BATCH_AMOUNT_KEY, 0.0))
        if material_id == "" or not SourceForecast.has_component(amount):
            continue
        terms.append(PARTY_PACK_ENTRY_FORMAT % [
            HudCraftingVocab.BATCH_AMOUNT_FORMAT % amount, material_id])
    if terms.is_empty():
        return ""
    return PARTY_PACK_CLAUSE_PREFIX + PARTY_PACK_CLAUSE_PREFIX.join(terms)

## mission, humanized phase, party size, and carried food (from stores/turnsOfFood). A hunt
## expedition (§2b) also lists the target herd it follows. Expeditions have no labor in v1, so
## this replaces the band's labor/morale rows entirely.
## Like the band + herd drawers, it carries NO identity row: an expedition rides the same
## roster path as a band, so its roster row (`_build_band_row`) already shows the very
## `id` the old `Unit:` line printed — nothing is lost with it (unlike the herd's fauna id, which
## had to move INTO the row). `Policy` / `Phase` deliberately keep their WORDS here: the compact
## Active-expeditions row is where the glyph vocabulary belongs; this block IS the disclosure.
##
## **`denial_view` IS THE HOST'S ANSWER TO A QUERY, NOT A LOOKUP THIS PRODUCER MAKES.** The `Collapse:`
## row's forecast left the snapshot and became a request/response on the command socket, and asking
## needs a request id, a staleness rule and a re-render when the answer lands — controller business.
## So the host that renders the strip (`BandPanelController`, which holds the `ForecastQuery` and
## re-renders on `answered`) composes the question and passes `ForecastQuery.view()` down. Defaulted to
## `{}` for the OTHER host and for the preview harnesses: a caller with no seam to ask through renders
## no collapse row at all, rather than a pending placeholder it will never resolve.
func expedition_summary_lines(unit_data: Dictionary, ctx: DetailFormat.Context = null,
        denial_view: Dictionary = {}) -> Array[String]:
    # Same out-parameter contract as `unit_summary_lines`: the Carried/Provisions rows tint by the
    # party's own food runway, which is stashed on the context below. Defaulted for the harnesses.
    var context := ctx if ctx != null else DetailFormat.Context.new()
    var lines: Array[String] = []
    var mission := String(unit_data.get("expedition_mission", ""))
    var is_hunt := mission == HudExpeditionVocab.EXPEDITION_MISSION_HUNT
    # **A DENIAL RAID IS A RAID FOR THE ROWS IT SHARES AND NOT FOR ONE OF THE ORDERS**
    # (`docs/plan_denial_raid.md`). It has a target herd, a party and a pack, so `is_raid` gates the
    # Target and Carried rows; it has NO floor, NO fill target and NO delivery ETA — those read `0.0`
    # / `0` / absent because the mission has no such lever — so they stay gated on `is_hunt` alone,
    # and what a denial party shows in their place is its COLLAPSE VERDICT.
    var is_deny := mission == HudExpeditionVocab.EXPEDITION_MISSION_DENY
    var is_raid := is_hunt or is_deny
    # The party's OWN target, resolved once: the `Target:` row's live position, the delivery line's
    # lost-vs-lean disambiguation and the denial verdict's own lookup are the same herd, so they must
    # not be looked up twice.
    var target_herd: Dictionary = _band_labor.expedition_target_herd(unit_data) if is_raid else {}
    lines.append("Mission: %s" % DetailFormat.expedition_mission_label(mission))
    # **A SHIPMENT'S ROWS ARE ITS OWN, AND IT BORROWS NONE OF THE RAID'S** (arc #527). It has no
    # quarry, no floor, no delivery ETA and no trip bound — the mission carries no such levers — so
    # the raid branches below stay closed to it and it answers the two questions a shipment raises:
    # who it is for, and what is in the packs. Returned early rather than woven in, because the
    # Provisions row beneath the raid branches would restate a pack this party states properly.
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_TRADE:
        return _shipment_summary_lines(unit_data, context, lines)
    if is_raid:
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
    if is_hunt:
        # The party's ORDERS row — where the raid stops as a fraction of the herd's capacity. Always
        # on the wire for a hunt party, and every value is meaningful (including `0`), so the row is
        # stated unconditionally rather than gated on being non-empty as the retired policy string was.
        # **It stays ONE row whatever it carries, because this producer's output lands in the parties
        # zone's height-capped, clipping inspector strip** — see `DetailFormat.expedition_orders_line`.
        # **A DENIAL PARTY IS NOT IN THIS BRANCH**: its `expeditionFloor` reads `0.0` because it HAS no
        # such orders, so rendering the row would put a lever on screen that the mission does not carry
        # and the command grammar cannot express.
        lines.append(DetailFormat.expedition_orders_line(unit_data, mission))
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
        if is_raid:
            # A raiding party lives off its own kills; its store item key isn't fixed, so total it.
            for qty in (stores_variant as Dictionary).values():
                carried += int(round(float(qty)))
        else:
            carried = int(round(float((stores_variant as Dictionary).get(HudConst.STORE_ITEM_PROVISIONS, 0.0))))
    # The pack's MATERIALS, composed before the branch because both `Carried:` spellings take it.
    var pack := _party_pack_clause(unit_data)
    if is_raid:
        # Carried X / cap + a FULL badge at the carry ceiling (the party heads home when full).
        # **A DENIAL PARTY SHOWS THIS ROW AND IT READS NEAR-EMPTY, WHICH IS THE POINT** — it banks
        # whatever it can haul on the way home, a rounding error against what it killed. Suppressing
        # it would hide the mission's own cost.
        var cap := int(round(float(unit_data.get("expedition_carry_cap", 0.0))))
        if cap > 0:
            var full_badge := "  %s" % HUNT_FULL_BADGE if carried >= cap else ""
            lines.append("Carried: %d / %d  (%s)%s%s" % [carried, cap, DetailFormat.food_turns_text(turns), pack, full_badge])
        else:
            lines.append("Carried: %d  (%s)%s" % [carried, DetailFormat.food_turns_text(turns), pack])
        # **THE DENIAL PARTY'S OWN READOUT, IN PLACE OF A DELIVERY ETA.** The mission publishes none —
        # its verdict is whether the herd goes past the point of no return — so the HOST asks the
        # forecast query on this party's behalf and hands the answer in (see the parameter's note).
        # Rendered before the hunt-only lines below so the two missions read in the same slot.
        if is_deny:
            var collapse_line := DetailFormat.expedition_collapse_line(
                unit_data, target_herd, denial_view)
            if collapse_line != "":
                lines.append(collapse_line)
        # Next-delivery forecast (the in-flight twin of the pre-launch hunt trip estimate): ALWAYS
        # shown for a hunt party once the field is on the wire, because a projected 0 is a real,
        # decision-relevant answer ("this herd has no surplus to raid") that a `> 0` guard used to
        # hide. The gate is `has(...)`, not `> 0`: the native decoder always inserts the field now, so
        # present-and-0 is a genuine no-surplus; an ABSENT key (older build) renders nothing rather
        # than a false "none".
        if is_hunt and unit_data.has("expedition_projected_delivery"):
            lines.append(DetailFormat.expedition_next_delivery_line(unit_data, target_herd))
        # **WHICH STOP ENDS THE TRIP** — the sim's own answer for THIS party's real orders, off the
        # same in-flight forward simulation the ETA above comes from. A `""` bound (not raiding: a
        # party already walking a load home, or a snapshot predating the field) renders no line, so
        # the strip never states a stop for a party that is not hunting toward one.
        var bound_line := DetailFormat.expedition_trip_bound_line(unit_data, mission)
        if bound_line != "":
            lines.append(bound_line)
    if not is_raid:
        # **A SCOUT CARRIES THE CLAUSE TOO**, and it is not decoration: a scouting party that walks
        # over a kill banks its materials exactly as a raid does, and the mission it was sent on is no
        # reason to hide what is in its pack.
        lines.append("Provisions: %d  (%s)%s" % [carried, DetailFormat.food_turns_text(turns), pack])
    var pos_array: Array = Array(unit_data.get("pos", []))
    if pos_array.size() == 2:
        lines.append("Position: (%d, %d)" % [int(pos_array[0]), int(pos_array[1])])
    return lines

## **A TRADE PARTY'S ROWS** (arc #527) — `Mission` (already appended by the caller), who it is bound
## for, the phase it is in, and the shipment beside the pack it fills. Four lines at most, which is
## inside the parties strip's own budget: the seven-line worst case is a HUNT party's, and a shipment
## has none of the three rows that make that case (Orders, Next delivery, the trip bound).
##
## **THE DESTINATION IS THE SIM'S DISPLAY TWIN, RENDERED VERBATIM.** `expedition_destination_band` is
## the key the command addresses and never appears on screen; the NAME was resolved at launch and
## rides the mission because a party outlives its destination's presence in the viewer's world.
func _shipment_summary_lines(unit_data: Dictionary, context: DetailFormat.Context,
        lines: Array[String]) -> Array[String]:
    # **ONE RESOLUTION FOR THE DESTINATION'S NAME, SHARED WITH THE PARTIES ROW AND THE PICKER.** The
    # sim's published name when there is one — and there is not, today, because bands have no names
    # and the sim declines to guess — else this client's own label for that band, joined on
    # `expedition_destination_band`. A destination neither tier can name renders NO ROW, rather than
    # the raw `BandId`, which is the key the command addresses and never a label.
    var destination := HudFormat.expedition_destination_label(unit_data,
        _band_labor.band_label_for_id)
    if destination != "":
        lines.append("%s: %s" % [TRADE_DESTINATION_ROW, destination])
    var phase := String(unit_data.get("expedition_phase", "")).strip_edges()
    if phase != "":
        lines.append("Phase: %s" % HudFormat.expedition_phase_label(phase))
    # The party's own runway still tints the cargo row, the `Carried:` contract: a shipment eats out
    # of its provisions on the road exactly as a scout does, and the walk is where a haul's cost lives.
    context.food_turns = float(unit_data.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS))
    # **THE NUMERATOR IS THE PACK'S MASS, NOT ITS FOOD** — `expedition_carry_cap` is what the sim
    # checks `food + weight × Σ materials` against, so putting the food alone over it renders a full
    # pack as a near-empty one. `DetailFormat.shipment_cargo_mass` is the compose sheet's own meter
    # expression, shared so the pre-launch price and the in-flight report answer for one pack.
    var cargo_mass := DetailFormat.shipment_cargo_mass(unit_data)
    var cargo := _shipment_cargo_clause(unit_data)
    var cap := float(unit_data.get("expedition_carry_cap", 0.0))
    if cap > 0.0:
        lines.append(TRADE_CARGO_CAP_FORMAT % [TRADE_CARGO_ROW,
            HudCraftingVocab.BATCH_AMOUNT_FORMAT % cargo_mass,
            HudCraftingVocab.BATCH_AMOUNT_FORMAT % cap, cargo])
    else:
        lines.append(TRADE_CARGO_NO_CAP_FORMAT % [TRADE_CARGO_ROW,
            HudCraftingVocab.BATCH_AMOUNT_FORMAT % cargo_mass, cargo])
    var pos_array: Array = Array(unit_data.get("pos", []))
    if pos_array.size() == 2:
        lines.append("Position: (%d, %d)" % [int(pos_array[0]), int(pos_array[1])])
    return lines

## **THE SHIPMENT'S MATERIALS, ONE TERM PER MATERIAL AND NEVER SUMMED** — ` · 4.0 hide · 1.2 bone`,
## `""` when the shipment is food alone. `expedition_cargo_materials` is the wire's per-material
## total across the batches the party holds (its `MaterialPayoff` rows), so this reads it as it
## arrives; adding hide to bone would be the retired trade axis rebuilt under a new name.
##
## **IT IS NOT `_party_pack_clause`.** That one reads `material_batches`, the party's OWN pack — what
## a scout skinned on the road, and what a trade party's escort is carrying for itself — and a
## shipment is the cargo store beside it. Rendering one for the other would let a hungry escort's
## kit read as goods bound for another people.
func _shipment_cargo_clause(unit_data: Dictionary) -> String:
    var terms: Array[String] = []
    for row_variant in unit_data.get("expedition_cargo_materials", []):
        if not (row_variant is Dictionary):
            continue
        var row: Dictionary = row_variant
        var material_id := String(row.get(HudCraftingVocab.BATCH_MATERIAL_ID_KEY, "")).strip_edges()
        var amount := float(row.get(HudCraftingVocab.BATCH_AMOUNT_KEY, 0.0))
        if material_id == "" or not SourceForecast.has_component(amount):
            continue
        terms.append(PARTY_PACK_ENTRY_FORMAT % [
            HudCraftingVocab.BATCH_AMOUNT_FORMAT % amount, material_id])
    if terms.is_empty():
        return ""
    return PARTY_PACK_CLAUSE_PREFIX + PARTY_PACK_CLAUSE_PREFIX.join(terms)

# ---- The band rows `unit_summary_lines` assembles -------------------------------------------------

## **THE `has fodder OR owes a bill` TEST NOW LIVES ON `DetailFormat.band_has_fodder_economy`.** It
## moved there when the faction page's own `Fodder:` row started asking it — the rollup is a static
## layer and cannot reach a producer's private — and a second copy of the test is how two surfaces
## come to disagree about when a larder exists. Every reader here calls it by that name.

## Is this band's fodder larder DRAINING — the slow trap, stated as a comparison.
## **Its ONE reader is the `compact` tier's merged clause**, which is the only host with no room to
## state the pair it is a verdict about: that tier trades the whole Fodder row for a stock clause on
## the Food line, so the tint is all it can say about a larder the band's Fields are not keeping up.
##
## **THE FULL ROW NO LONGER ASKS.** Its `need` clause and that clause's amber are retired with the
## rates themselves — the runway says the larder is draining, under the same thresholds the Food row
## uses — so this is not a second opinion about the standalone row's severity any more.
##
## ⛔ **THE VERDICT IS `DetailFormat.band_net_fodder` NEGATED, NEVER `need − income` RECOMPUTED HERE.**
## That subtraction omits the LOCAL crossings the net counts, and a camp a neighbour tops up every
## turn — need 6.0, harvest 5.0, 2.0 arriving over the local exchange — has a RISING larder. Its
## `Fodder:` caret is calm on the net, and this clause used to take the amber anyway: one panel saying
## two things about one larder, which is the disagreement `band_net_fodder` exists to end. The amber
## marks a bill the band cannot pay, and a band a neighbour feeds can pay it. **Local crossings only,
## never route** — that is `band_net_fodder`'s own rule, and riding it is how this stays on it.
##
## **STRICTLY WORSE THAN THE FLOW FLOOR**, so a band whose hay exactly balances does not flicker amber
## on float noise, and a band owing nothing at all is never warned — the `fodder_need` gate above,
## which is about whether there is a bill rather than about how the larder is moving.
func _band_fodder_falls_short(unit_data: Dictionary) -> bool:
    var need := float(unit_data.get("fodder_need", 0.0))
    if need < SourceForecast.FODDER_FLOW_MIN:
        return false
    return -DetailFormat.band_net_fodder(unit_data) >= SourceForecast.FODDER_FLOW_MIN

## The band's fodder larder as the Food row's twin: the STOCK and the RUNWAY, and nothing else. The
## two flows that move it are the disclosure `unit_summary_lines` registers on this row — see
## `BAND_FODDER_ROW_FORMAT` for why they are not on it.
##
## Stashes the runway on the render context so `DetailFormat._value_hex` tints the value by the shared
## runway thresholds, the same handshake `_band_food_line` makes one row above.
func _band_fodder_line(unit_data: Dictionary, ctx: DetailFormat.Context) -> String:
    var turns := float(unit_data.get("turns_of_fodder", BandFoodStatus.UNLIMITED_TURNS))
    ctx.fodder_turns = turns
    return BAND_FODDER_ROW_FORMAT % [
        SourceForecast.format_fodder(DetailFormat.band_fodder_store(unit_data)),
        DetailFormat.food_turns_text(turns)]

## The band's standing material bill as the Fodder row's twin: the STOCK and the RUNWAY of the good in
## the WORST state, and nothing else. The per-good detail is the disclosure `unit_summary_lines`
## registers on this row — see `BAND_MATERIAL_UPKEEP_ROW_FORMAT` for why the row names one good.
##
## Stashes the runway on the render context so `DetailFormat._value_hex` tints the value by the shared
## runway thresholds, the same handshake `_band_food_line` and `_band_fodder_line` make above.
##
## **THE CALLER HAS ALREADY ASKED WHETHER THERE IS A BILL** (`band_has_material_upkeep`), which is why
## this reads the worst row without a fallback: an empty answer here would be a row about nothing, and
## the gate is what stops it being drawn at all.
func _band_material_upkeep_line(unit_data: Dictionary, ctx: DetailFormat.Context) -> String:
    var worst := DetailFormat.band_material_worst(unit_data)
    var turns := float(worst.get(DetailFormat.MATERIAL_BILL_RUNWAY_KEY,
        BandFoodStatus.UNLIMITED_TURNS))
    ctx.material_turns = turns
    return BAND_MATERIAL_UPKEEP_ROW_FORMAT % [
        BAND_MATERIAL_TERM_FORMAT % [
            DetailFormat.format_trimmed(float(worst.get(DetailFormat.MATERIAL_BILL_STORE_KEY, 0.0)),
                HudWorkVocab.RUNG_TRACK_MATERIAL_DECIMALS),
            String(worst.get(SourceForecast.MATERIAL_PAYOFF_ID_KEY, ""))],
        DetailFormat.food_turns_text(turns)]

## The SAME row on a band with no fodder economy — a dim em-dash and the reason on the block's hover,
## built by `DetailFormat.fodder_dormant_row` so this row and the FACTION page's twin cannot diverge.
##
## **ALL THIS SIDE CONTRIBUTES IS THE FACTION'S FODDERING**, which is the one fact the shared builder
## cannot read for itself: knowledge is faction-scoped and no band dict carries it.
func _band_fodder_dormant_line(ctx: DetailFormat.Context) -> String:
    return DetailFormat.fodder_dormant_row(ctx,
        _player_knowledge(HudFloraVocab.KNOWLEDGE_TRACK_FODDERING))

## Selection-panel band food row: "Food  <provisions>  (<turns>)" — provisions from
## the band's larder stores, turns from `turns_of_food` (∞ when not food-limited).
## Stashes the turns on the render context so `DetailFormat.detail_bbcode` can
## tint the value by the shared warn/critical thresholds.
## `merge_fodder` is the `compact` host asking for the fodder stock to ride this line instead of
## taking a row of its own — see `BAND_FOOD_FODDER_CLAUSE_FORMAT`.
func _band_food_line(unit_data: Dictionary, ctx: DetailFormat.Context, merge_fodder: bool = false) -> String:
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
        # so it no longer swings turn-to-turn) minus what the people eat and what raids take off the
        # larder. The breakdown below itemizes the income rows and the debits.
        var net := DetailFormat.band_net_food(unit_data)
        var net_hex := HudStyle.HEALTHY_HEX if net >= 0.0 else HudStyle.DANGER_HEX
        line += " · [color=#%s]%s[/color]" % [net_hex, SourceForecast.format_yield(net)]
        _food_flow_present = true
    # The hay larder, on this line rather than beneath it, for the height-scarce host only. The gate
    # is the same one the standalone row uses, so a band with no fodder economy renders no clause —
    # and so is the WARN, which is the only thing this tier has room to say about a bill the band's
    # Fields are not covering (see `BAND_FOOD_FODDER_CLAUSE_FORMAT`).
    if merge_fodder and DetailFormat.band_has_fodder_economy(unit_data):
        var fodder_hex := HudStyle.WARN_HEX if _band_fodder_falls_short(unit_data) else HudStyle.INK_DIM_HEX
        line += BAND_FOOD_FODDER_CLAUSE_FORMAT % [
            fodder_hex, SourceForecast.format_fodder(float(unit_data.get("fodder_store", 0.0)))]
    return line

## **`_band_kit_line` AND `BAND_KIT_ROW_MAX_ENTRIES` ARE RETIRED** with the `Gear` row itself — see
## the vocabulary block at the top of this file for why the row went and what replaced it. The 22px
## `Zone_band` overflow the entry budget was measured against retires with them: no row here grows
## per item any more.

## Selection-panel band morale row: "Morale: 41% ▼ — harsh terrain (Karst Cavern Mouth)".
## Morale, its per-turn trend, and the dominant cause come from the snapshot cohort dict
## (decoded in `native/src/lib.rs population_to_dict`). A falling trend appends the named
## cause; Terrain names the band's tile (the "it's the hex you're on" payload — `terrain_label`,
## already stripped by the caller). A rehydrated save reports delta 0 / cause None for one turn, so
## the row degrades to a bare percentage.
## Stashes morale on the render context so `DetailFormat.detail_bbcode` tints the value.
func _band_morale_line(unit_data: Dictionary, terrain_label: String, ctx: DetailFormat.Context,
        compact: bool = false) -> String:
    var morale: float = float(unit_data.get("morale", 1.0))
    ctx.morale = morale
    var text := "Morale: %d%%" % int(round(morale * 100.0))
    var delta: float = float(unit_data.get("morale_delta", 0.0))
    if delta <= -DetailFormat.MORALE_TREND_EPSILON:
        text += " %s" % MORALE_TREND_FALLING_GLYPH
        # Name the cause only when morale is actually concerning — a healthy band
        # drifting slowly (nearly every tile bleeds a little today) shouldn't be
        # branded "harsh climate/terrain". Below the warn threshold, spell it out.
        #
        # **NOT IN THE `compact` TIER, where the Growth clause shares this line.** The cause is the
        # longest run this row can carry (`harsh terrain (Karst Cavern Mouth)`), the label is
        # `AUTOWRAP_WORD`, and a merged line that wraps costs back the very row the merge bought — a
        # fix that measures as no fix, with nothing failing. The trend GLYPH stays, so the row still
        # says morale is falling; the cause is recoverable from the disclosure popover this row's
        # caret opens, which is what makes it the clause that yields rather than the merge.
        if morale < BandFoodStatus.warn_morale() and not compact:
            var cause := int(unit_data.get("morale_cause", DetailFormat.MORALE_CAUSE_NONE))
            var cause_label := DetailFormat.morale_cause_label(cause)
            if cause_label != "":
                if cause == DetailFormat.MORALE_CAUSE_TERRAIN and terrain_label != "":
                    cause_label = "%s (%s)" % [cause_label, terrain_label]
                text += " — %s" % cause_label
    elif delta >= DetailFormat.MORALE_TREND_EPSILON:
        text += " %s" % MORALE_TREND_RISING_GLYPH
    return text

## Selection-panel band growth row: "Growth: 23% of normal" — the band's birth rate as a share of the
## base rate the sim would otherwise apply (`fertility_hunger × fertility_reserve × fertility_trend`,
## neutral at 1.0). Unlike Output it is shown at EVERY level, including above normal: it is the row
## the fertility disclosure hangs on, and a player asking "why is growth slow?" has to be able to
## find it in the good state too — the same reasoning that keeps the morale contributions computing
## when morale is fine. Stashes the multiplier on the render context so `DetailFormat.detail_bbcode`
## tints it by the fertility buckets (ink → amber → red).
##
## Returns "" when the sim published no reading (a rehydrated cohort, whose factors are derived and
## not persisted). No row, no disclosure — never a fabricated 0%.
func _band_growth_line(unit_data: Dictionary, ctx: DetailFormat.Context) -> String:
    if not BandFoodStatus.fertility_is_projected(unit_data):
        return ""
    var fertility := DetailFormat.band_fertility(unit_data)
    ctx.fertility = fertility
    return DetailFormat.GROWTH_ROW_FORMAT % int(round(fertility * 100.0))

## The Growth row rendered as a CLAUSE on the Morale line, for the SHORT band-zone tier — the pair to
## `_band_growth_line`, and the only place the two can differ is the anchor suffix, which a merged
## line cannot afford (`DetailFormat.GROWTH_VALUE_SHORT_FORMAT`).
##
## It reads `ctx.fertility`, which `_band_growth_line` has already stashed, and tints from the SAME
## `BandFoodStatus.hex_for_fertility` buckets `DetailFormat._value_hex` would have given a standalone
## row — so the merge changes where the number sits and nothing about what it says. The label is the
## identical clickable run a standalone row wears, so the fertility popover survives the merge; a
## Growth row that registered no disclosure (an empty breakdown) falls back to the plain word, which
## is what keeps the reading legible rather than unlabelled.
func _band_growth_clause(unit_data: Dictionary, ctx: DetailFormat.Context) -> String:
    var fertility := DetailFormat.band_fertility(unit_data)
    # **THE CARETS HAVE TO BE ON THE CONTEXT BEFORE THIS RUN IS BUILT, and they are not yet.** Every
    # other disclosure is drawn by `detail_bbcode` from a context this producer fills on its LAST
    # line — so a clause built mid-producer reads an empty `disclosures` and silently falls back to
    # the plain word, losing the caret and the click with it (measured: the merged line rendered
    # `Growth 188%`). Reading the controller's live state here is the fix, and the same assignment
    # runs again at the end: it is idempotent, and this is the one row rendered before its turn.
    ctx.disclosures = _disclosures.state()
    var label := DetailFormat.inline_disclosure_label(HudDisclosureVocab.DETAIL_ROW_GROWTH, ctx)
    if label == "":
        label = HudDisclosureVocab.DETAIL_ROW_GROWTH
    return BAND_MORALE_GROWTH_CLAUSE_FORMAT % [
        label, BandFoodStatus.hex_for_fertility(fertility),
        DetailFormat.GROWTH_VALUE_SHORT_FORMAT % int(round(fertility * 100.0))]

## Itemized fertility breakdown: the three named factors as indented sub-lines, each rendered as a
## MULTIPLIER — `    ▼ ×0.60  short rations` — because they combine by product, so reading down the
## list multiplies out to the Growth headline above (`DetailFormat.detail_bbcode` tints by the sign
## glyph, the same path the morale breakdown uses). Only factors that actually moved off the neutral
## 1.0 list, so a thriving band's disclosure shows what is *helping* rather than three no-op rows.
##
## A neutral `trend` — which is exactly what the sim publishes when a band's flow telemetry is not
## projected — therefore renders as NOTHING, never as a deficit. That is the client half of the
## sim's no-data rule.
func _fertility_breakdown_lines(unit_data: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    if not BandFoodStatus.fertility_is_projected(unit_data):
        return lines
    var trend := float(unit_data.get("fertility_trend", BandFoodStatus.FERTILITY_NEUTRAL))
    # (factor, label) in the model's own order: hunger (the gate) → reserve (stock) → trend (flow).
    var factors := [
        [float(unit_data.get("fertility_hunger", BandFoodStatus.FERTILITY_NEUTRAL)), DetailFormat.FERTILITY_LABEL_HUNGER],
        [float(unit_data.get("fertility_reserve", BandFoodStatus.FERTILITY_NEUTRAL)), DetailFormat.FERTILITY_LABEL_RESERVE],
        [trend, DetailFormat.FERTILITY_LABEL_TREND_GROWING if trend > BandFoodStatus.FERTILITY_NEUTRAL \
            else DetailFormat.FERTILITY_LABEL_TREND_SHRINKING],
    ]
    var epsilon := BandFoodStatus.fertility_breakdown_epsilon()
    for entry in factors:
        var factor: float = entry[0]
        if absf(factor - BandFoodStatus.FERTILITY_NEUTRAL) < epsilon:
            continue
        lines.append(DetailFormat.fertility_breakdown_row(factor, entry[1]))
    return lines

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

