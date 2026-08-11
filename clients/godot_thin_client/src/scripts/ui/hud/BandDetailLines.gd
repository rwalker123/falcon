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

# ---- The SAME hay stock as a CLAUSE on the Food row, for the `compact` (SHORT band-zone tier) host.
# A horizontal dock is short of HEIGHT and has width to spare, so the two larders share one line
# there; a vertical dock is short of WIDTH and keeps them as two rows. The word is `hay`, the
# vocabulary the flora basket rows already use (`HudFloraVocab.FLORA_CROP_HAY_CLAUSE_FORMAT`), and the
# stock keeps the Fodder row's ONE decimal. It carries its OWN colour rather than inheriting the Food
# row's value tint: a starving band's hay stock is not itself a red reading, and the net rate beside it
# sets the precedent for a self-tinted run inside that value cell.
const BAND_FOOD_HAY_CLAUSE_FORMAT := " · [color=#%s]%.1f hay[/color]"

# ---- THE GROWTH ROW AS A CLAUSE ON THE MORALE LINE, for the `compact` (SHORT band-zone tier) host —
# the second merge this tier makes, and the same trade for the same reason as the hay clause above:
# HEIGHT is what is scarce in a height-capped horizontal dock, and it has a whole screen of width.
#
# **MORALE AND GROWTH ARE THE RIGHT PAIR.** Both are player-band health scalars, both already carry
# disclosure carets, and they read naturally together. The alternative — dropping a row — is not
# available here: `Kit` is the row the tier gained and it is NOT droppable (a spent kit is stated
# nowhere else in the client and is not recoverable from any other surface), and the one row this tier
# used to drop — `Trade` — is retired outright (arc #527), so there is nothing left to give up.
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

# ---- The band's KIT row (`docs/plan_hunt_through_combat.md` §4.8) — `Kit: Spears 87 · Sled 54 ·
# Baskets dry`. Three consumable kits, start-stocked and not craftable, each with its own condition
# and its own job; a dry one has stepped its role down to bare hands FOR GOOD, so it reads DANGER
# rather than merely dim.
#
# **THE ROW IS THE CLOCK, THE DISCLOSURE IS THE CLIFF.** What a player needs at a glance is how long
# until each kit runs out and which side of the line they are already on — never a gauge, never a
# bar, and never a number scaled by what is left, because performance is FLAT until expiry and any
# gradient drawn here would claim a taper the model does not have. What each kit actually DOES lives
# one click down, where there is room to say it and to say that it stops.
# **"Gear", not "Kit" — the row lists ITEMS.** A kit is the named loadout a crew is SENT OUT WITH
# and is chosen in the compose sheet's Kit picker; this row is the condition of the equipment the
# band owns. Labelling item conditions "Kit" is the same two-nouns confusion the config carried until
# the items were renamed off `*_kit`, and it read as "your kit is Spears, Sled, Baskets" — which is
# not a kit at all.
const BAND_KIT_ROW_PREFIX := "Gear: "
const BAND_KIT_ROW_SEPARATOR := " · "
const BAND_KIT_ROW_ENTRY_FORMAT := "%s [color=#%s]%s[/color]"

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
        # The band's fodder (hay) larder, beneath its food larder — shown only for a band with a
        # fodder economy: it has stockpiled hay, or it pays a pen bread bill it could offset with hay.
        # **In the `compact` tier it is not a row at all**: `_band_food_line` has already carried the
        # stock as a clause on the Food line, because the SHORT tier's scarcity is HEIGHT and that
        # host has width to spend. See `BAND_FOOD_HAY_CLAUSE_FORMAT`.
        if not compact and _band_has_fodder_economy(unit_data):
            lines.append(BAND_FODDER_ROW_FORMAT % float(unit_data.get("fodder_store", 0.0)))
        # THE BAND'S KIT, beneath its larders and above its morale: three consumable tools whose
        # condition only ever falls, and whose expiry silently drops a whole role to bare hands. It is
        # our OWN bands' business, like Food — a rival's equipment is not ours to count.
        # **Gated on the field being STATED, never on a value**: a dry kit is `0` and is the single
        # most important reading here, so only an absent field may suppress the row.
        if DetailFormat.band_states_kit(unit_data):
            lines.append(_band_kit_line(unit_data))
            _disclosures.register(HudDisclosureVocab.DETAIL_ROW_KIT,
                HudDisclosureVocab.BREAKDOWN_KIND_KIT, unit_data,
                _disclosures.kit_breakdown_lines(unit_data))
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

# ---- The band rows `unit_summary_lines` assembles -------------------------------------------------

## Does this band have a fodder economy at all — hay in store, or a pen bill it could offset with hay?
## The ONE test behind both spellings of that larder (the standalone `Fodder:` row and the `compact`
## host's clause on the Food line), so the two hosts can never disagree about when it exists.
func _band_has_fodder_economy(unit_data: Dictionary) -> bool:
    return float(unit_data.get("fodder_store", 0.0)) > SourceForecast.FOOD_FLOW_MIN \
        or float(unit_data.get("pen_feed_upkeep", 0.0)) > SourceForecast.FOOD_FLOW_MIN

## Selection-panel band food row: "Food  <provisions>  (<turns>)" — provisions from
## the band's larder stores, turns from `turns_of_food` (∞ when not food-limited).
## Stashes the turns on the render context so `DetailFormat.detail_bbcode` can
## tint the value by the shared warn/critical thresholds.
## `merge_fodder` is the `compact` host asking for the hay stock to ride this line instead of taking a
## row of its own — see `BAND_FOOD_HAY_CLAUSE_FORMAT`.
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
        # so it no longer swings turn-to-turn) minus what the people (Eaten) and the pens (Pen feed)
        # draw off the larder. The breakdown below itemizes the income rows and the debits.
        var net := DetailFormat.band_net_food(unit_data)
        var net_hex := HudStyle.HEALTHY_HEX if net >= 0.0 else HudStyle.DANGER_HEX
        line += " · [color=#%s]%s[/color]" % [net_hex, SourceForecast.format_yield(net)]
        _food_flow_present = true
    # The hay larder, on this line rather than beneath it, for the height-scarce host only. The gate
    # is the same one the standalone row uses, so a band with no fodder economy renders no clause.
    if merge_fodder and _band_has_fodder_economy(unit_data):
        line += BAND_FOOD_HAY_CLAUSE_FORMAT % [
            HudStyle.INK_DIM_HEX, float(unit_data.get("fodder_store", 0.0))]
    return line

## Selection-panel band KIT row: `Kit: Spears 87 · Sled 54 · Baskets dry` — the band's three
## consumable kits and how much is left of each, with a spent one named in DANGER ink.
##
## **THE CONDITION IS A CLOCK, NOT A PERFORMANCE READING.** Durability and performance are orthogonal
## axes: a kit works at its full tier until it hits zero and then the role steps down permanently. So
## this row prints the number flat, tints only the ZERO, and draws no bar — a filled gauge here would
## say "half a sled hauls half as much", which is exactly wrong.
##
## **ALL THREE ARE ALWAYS LISTED, including on a band that neither hunts nor forages today.** Each
## kit wears on its own quantum (spears per animal killed, the sled per biomass hauled, baskets per
## biomass gathered), so what a band is doing this turn does not predict which kit is closest to
## running out — and a row that hid the idle ones would hide the very kit whose loss is about to
## change what the band CAN do.
##
## **IT SURVIVES THE `compact` TIER.** A spent kit is stated nowhere else in the client at all, and
## it is not recoverable from any other surface.
## How many items the compact kit row can show before it wraps and overflows `Zone_band`. Three is
## what the zone was sized for and what it carried for the whole of the minimal TOE; a fourth entry
## overflowed it by 22px. **Raising it means re-measuring the zone**, not just changing this number.
const BAND_KIT_ROW_MAX_ENTRIES := 3

func _band_kit_line(unit_data: Dictionary) -> String:
    var entries: Array[String] = []
    # **THE ROW IS A FIXED-HEIGHT ZONE AND CANNOT GROW PER ITEM.** Listing every item the server
    # publishes wrapped it to a second line and overflowed `Zone_band` by 22px the moment `traps`
    # was added (caught by `band_panel_preview`'s `_assert_zone_content_fits`, not by eye) — and the
    # item table is config, so the next item would do it again.
    #
    # So the row is a SUMMARY with a bounded budget: the items that need a decision first, then
    # whatever fits. **Dry items lead, then SHORT ones** — running dry is a permanent step down to
    # bare hands, and a shortfall is the other half of the party standing there with nothing; both
    # are decisions, and the rest fill the remaining slots in roster order. The full per-item
    # breakdown is the disclosure (`DisclosureController.kit_breakdown_lines`), which scrolls and
    # therefore can carry them all.
    #
    # Nothing is hidden silently: `DetailFormat.band_kit_is_dry` and `band_kit_is_short` — what
    # together tint the caret WARN — sweep EVERYTHING the server published, so an item pushed off
    # this row still raises the warning that sends the player to the breakdown.
    var conditions: Array = unit_data.get(DetailFormat.KIT_ITEM_CONDITIONS_KEY, [])
    var ordered: Array = []
    for row in conditions:
        if float(row.get(DetailFormat.KIT_ITEM_REMAINING_KEY, DetailFormat.KIT_DRY)) \
                <= DetailFormat.KIT_DRY:
            ordered.append(row)
    for row in conditions:
        if not ordered.has(row) and int(DetailFormat.kit_coverage(unit_data,
                String(row.get(DetailFormat.KIT_ITEM_ID_KEY, "")))["short"]) > 0:
            ordered.append(row)
    for row in conditions:
        if not ordered.has(row):
            ordered.append(row)
    for row in ordered.slice(0, BAND_KIT_ROW_MAX_ENTRIES):
        var item_id := String(row.get(DetailFormat.KIT_ITEM_ID_KEY, ""))
        # **THREE STATES, NOT TWO** (issue #520). A live item that reaches everybody is neutral INK;
        # one that has run out is DANGER, the permanent step down; one that is live but reaches only
        # part of the party is WARN — its gear works perfectly for whoever holds it, which is exactly
        # why it must not read as the cliff, and equally why it must not read as *fine*.
        var coverage := DetailFormat.kit_coverage(unit_data, item_id)
        var short := int(coverage["short"]) > 0
        var face := DetailFormat.kit_condition_face(unit_data, item_id)
        var hex := HudStyle.DANGER_HEX
        if DetailFormat.kit_is_equipped(unit_data, item_id):
            hex = HudStyle.WARN_HEX if short else HudStyle.INK_HEX
        if short:
            face = DetailFormat.KIT_COVERAGE_ROW_FORMAT % [
                face, coverage["holding"], coverage["headcount"]]
        entries.append(BAND_KIT_ROW_ENTRY_FORMAT % [
            DetailFormat.kit_item_label(item_id), hex, face])
    return BAND_KIT_ROW_PREFIX + BAND_KIT_ROW_SEPARATOR.join(entries)

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

