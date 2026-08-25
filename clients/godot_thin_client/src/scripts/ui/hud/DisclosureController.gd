class_name DisclosureController
extends RefCounted

## THE DETAIL-ROW DISCLOSURE CLUSTER (docs/plan_hud_decomposition.md) — the NODE-OWNING half of the
## detail-render layer, and the other end of the `[url]` meta `DetailFormat` emits.
##
## WHAT THIS IS. A summary row (Food / Morale) can be clicked to open its breakdown in a shared
## `PopupPanel`. This controller owns the whole of that: the per-render caret state every host's
## renderer reads, the stashed breakdown payloads, the popover node itself, and the click dispatch.
##
## WHY IT IS SPLIT FROM `DetailFormat` ALONG NODE OWNERSHIP RATHER THAN "FORMATTER VS POPOVER". The
## two are BIDIRECTIONALLY coupled — the formatter emits the `[url]` meta only this file can parse,
## and the popover renders THROUGH the formatter — so they ship together. What separates them is that
## this half lazily creates a `PopupPanel` and must `add_child` it, which a static module cannot do
## and a `RefCounted` cannot do either: hence the `popover_host` handed to `setup`, the
## `TurnOrbController` fork-panel pattern.
##
## THE POPOVER, NEVER INLINE — a correctness rule, not a style one. Expanding a breakdown in place
## grew the vitals `RichTextLabel` AFTER the Band panel had already picked its zone height tier, and
## the zone hosts `clip_contents`: the extra lines silently sliced the WORKFORCE row and ate both role
## cards. A Window cannot change a zone's height, which is the same reason the section menus are
## `MenuButton`s and the destructive confirms are `ConfirmationDialog`s.
##
## THE INBOUND RE-RENDER EDGE IS ONE INJECTED CALLABLE. Opening or closing the popover has to flip the
## caret in whichever hosts can be showing one — the Band/City panel's vitals label and the selection
## drawer. That pair is `HudLayer`'s knowledge, not this cluster's, so it arrives as the single
## `refresh_hosts` Callable rather than as two back-references.
##
## CONSTS. Same rule as `DetailFormat`: a const lives here iff every reader moved here. The popover's
## geometry did; `BREAKDOWN_TOGGLE_META_PREFIX` (read by the formatter and by both preview harnesses)
## and `BREAKDOWN_KIND_*` live in `HudDisclosureVocab`, the `FOOD_LABEL_*` table in `DetailFormat` —
## each read as `Module.X`.

## The breakdown popover's card geometry. Fixed width so the rows align in a column like the table
## they came from; the GAP is how far under the clicked row the card floats.
const BREAKDOWN_POPOVER_WIDTH := 300.0
const BREAKDOWN_POPOVER_PADDING := 10
const BREAKDOWN_POPOVER_GAP := 4.0
## The four sides `BREAKDOWN_POPOVER_PADDING` is applied to.
const POPOVER_MARGIN_SIDES := ["left", "top", "right", "bottom"]
## No disclosure is open.
const NO_OPEN_KEY := ""

## The node the lazily-built popover is parented into (a `RefCounted` cannot parent).
var _popover_host: Node = null
## Re-render whichever hosts can be showing a disclosure caret, so it flips with the popover. Injected
## because WHICH hosts those are is HudLayer's knowledge, not this cluster's.
var _refresh_hosts: Callable = Callable()
## Per-render caret state: row-label → {key, open, concerning}. Read by `DetailFormat.detail_bbcode`
## through the render context (`state()` feeds `Context.disclosures`).
var _disclosure_state: Dictionary = {}
## The breakdown rows each disclosure would show, keyed `"<kind>:<entity>"` → Array[String]. Written
## every render by `register` and read by the popover, so the popover never recomputes a number and a
## click needs no band lookup — the meta carries the key. Deliberately NOT cleared with
## `_disclosure_state`: that one is per-render and per-host, and the other host's render must not be
## able to empty the payload behind an open popover.
var _breakdown_payloads: Dictionary = {}
## The disclosure key whose popover is currently up, `NO_OPEN_KEY` = none. It is what "open" means
## now, so it is also what flips the row's caret.
var _breakdown_popover_key: String = NO_OPEN_KEY
## The one popover both hosts share, built lazily on the first disclosure click.
var _breakdown_popover: PopupPanel = null
var _breakdown_popover_label: RichTextLabel = null
## Where a FACTION drill-down row's band link goes. A Callable rather than a typed collaborator
## because the thing it reaches — "make this band the panel's subject" — belongs to the band panel's
## controller, which this one must not depend on (it is wired by whoever owns both).
var _faction_band_jump: Callable = Callable()


## Hand over the node the popover parents into and the one re-render edge back into HudLayer.
func setup(popover_host: Node, refresh_hosts: Callable) -> void:
    _popover_host = popover_host
    _refresh_hosts = refresh_hosts

## Make a detail label's `[url]` metas clickable. Called for BOTH hosts — the selection drawer's
## long-lived `%OccupantDetail` and the Band panel's per-render vitals label — each binding ITSELF as
## the anchor the popover floats under.
func wire_label(label: RichTextLabel) -> void:
    if label == null:
        return
    label.meta_clicked.connect(_on_meta_clicked.bind(label))

## The caret state for the render about to happen — what feeds `DetailFormat.Context.disclosures`.
func state() -> Dictionary:
    return _disclosure_state

## Drop the previous render's carets. Called at the top of the band line producer, NOT inside the Food
## row builder — a foreign band skips that call entirely, and a skipped Food row must not inherit the
## previous render's caret.
func clear_rows() -> void:
    _disclosure_state = {}

## Register a summary row (`row_label`, e.g. "Food"/"Morale") as a click-to-open disclosure: stash the
## rows its popover will show and record the caret state for `DetailFormat.detail_bbcode`. Returns
## whether the affordance is offered at all — a row with nothing to show gets no caret. Shared by both
## disclosure rows and by BOTH hosts (the panel's vitals label and the Occupants-card drawer), which
## is the point: one click behaviour, no `is_panel` fork.
func register(row_label: String, kind: String, band: Dictionary, lines: Array[String]) -> bool:
    if lines.is_empty():
        return false
    var key := DetailFormat.breakdown_key(kind, band)
    _breakdown_payloads[key] = lines
    var concerning := _is_concerning(kind, band)
    _disclosure_state[row_label] = {"key": key, "open": _breakdown_popover_key == key, "concerning": concerning}
    # A live popover restates the numbers it was opened on, so a snapshot refreshes it in place.
    if _breakdown_popover_key == key:
        _refresh_popover_text()
    return true

## Wire where a faction drill-down row's band link goes. Set once, by whoever owns both this
## controller and the band panel's.
func set_faction_band_jump(jump: Callable) -> void:
    _faction_band_jump = jump

## Register a FACTION row's disclosure — the same popover, with the verdict STATED rather than derived.
##
## `register` above asks `_is_concerning(kind, band)`, and a faction row has no band to ask about: its
## verdict is "at least one band is in trouble", which only the caller holds the roster to answer. So
## the flag is a parameter here, and the two registration paths stay honest about which of them knows
## the answer instead of one of them guessing.
##
## The KEY is the ordinary `breakdown_key` on an EMPTY band, i.e. entity `-1` — collision-free against
## every real band (entities are non-negative), so a faction row and a band row of the same kind can
## never share a payload slot.
func register_faction(row_label: String, kind: String, lines: Array[String], concerning: bool) -> bool:
    if lines.is_empty():
        return false
    var key := DetailFormat.breakdown_key(kind, {})
    _breakdown_payloads[key] = lines
    _disclosure_state[row_label] = {"key": key, "open": _breakdown_popover_key == key, "concerning": concerning}
    if _breakdown_popover_key == key:
        _refresh_popover_text()
    return true

## Does this row have something worth reading right now? Each disclosure kind owns its own answer
## (`DetailFormat.*_is_concerning`), and the verdict only ever tints the caret WARN — no popover ever
## opens itself. A dispatch rather than a chain of ternaries because there are four kinds now and a
## fifth would otherwise be a nested conditional nobody can read.
func _is_concerning(kind: String, band: Dictionary) -> bool:
    match kind:
        HudDisclosureVocab.BREAKDOWN_KIND_FOOD:
            return DetailFormat.food_is_concerning(band)
        HudDisclosureVocab.BREAKDOWN_KIND_GROWTH:
            return DetailFormat.growth_is_concerning(band)
        HudDisclosureVocab.BREAKDOWN_KIND_FODDER:
            # The FOOD test on the fodder account, through the same runway thresholds — see
            # `DetailFormat.fodder_is_concerning`. The row's own amber `need` clause is retired, so
            # this is the only place a fodder larder is judged worrying, and it judges it by the rule
            # the food larder is judged by.
            return DetailFormat.fodder_is_concerning(band)
        HudDisclosureVocab.BREAKDOWN_KIND_KIT:
            # A kit that has RUN OUT is concerning; one merely wearing down is not. There is no
            # replenishment path, so the step down is permanent — the caret is the one warning the
            # player gets, and gating it on a remaining-condition threshold would either cry wolf
            # every turn or fire after the loss it was meant to announce.
            #
            # **A SHORTFALL COUNTS TOO** (issue #520). A band holding ten spears for seventeen
            # hunters has no dry item and is still fighting with half a party; the caret is the
            # invitation to open the popover that says which item and by how many.
            return DetailFormat.band_kit_is_dry(band) or DetailFormat.band_kit_is_short(band)
        _:
            return DetailFormat.morale_is_concerning(band)

## The category breakdown sub-lines under Food, one indented row per present category, mirroring the
## morale breakdown: `    ▲ +0.48  Gathered` / `    ▲ +0.46  Hunted` / `    ▼ −0.68  Eaten` (income ▲
## green, debits ▼ amber via the shared indented-sub-line tint). Only categories above the floor.
##
## **THE PEN HAS NO ROW HERE**, and the `🐄 Pen feed (animals)` line that used to sit beside `Eaten` is
## retired with `penFeedUpkeep`: human food is not animal feed, so a pen never draws on this larder at
## all. The two-debit reading it existed for — *is my larder draining because of my people or my
## herd?* — has only one answer left on this ledger, and the herd's side of it is the drawer's marked
## `Fed:` row instead.
func food_breakdown_lines(band: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var gathered := DetailFormat.sum_realized_yield(band, SourceForecast.LABOR_KIND_FORAGE)
    if gathered >= SourceForecast.FOOD_FLOW_MIN:
        lines.append(DetailFormat.food_breakdown_row(gathered, DetailFormat.FOOD_LABEL_GATHERED))
    var hunted := DetailFormat.sum_realized_yield(band, SourceForecast.LABOR_KIND_HUNT)
    if hunted >= SourceForecast.FOOD_FLOW_MIN:
        lines.append(DetailFormat.food_breakdown_row(hunted, DetailFormat.FOOD_LABEL_HUNTED))
    var eaten := float(band.get("food_consumption", 0.0))
    if eaten >= SourceForecast.FOOD_FLOW_MIN:
        lines.append(DetailFormat.food_breakdown_row(-eaten, DetailFormat.FOOD_LABEL_EATEN))
    # The raid debit (Predators Phase 3): food a predator took off the larder this turn. A THIRD kind
    # of row beside Eaten — same larder, a different story (guard the camp vs feed the people) — so it
    # gets its own line, and only when a raid actually landed (0 → omitted).
    var raid_forfeit := DetailFormat.band_raid_forfeit(band)
    if raid_forfeit >= SourceForecast.FOOD_FLOW_MIN:
        lines.append(DetailFormat.food_breakdown_row(-raid_forfeit, DetailFormat.FOOD_LABEL_RAID_FORFEIT))
    # **THE TWO TRANSFER ROWS** (arc #527) — food that crossed between bands, which passes through
    # NEITHER of the income rows above (what THIS band's workers produced) nor either debit (what its
    # own people ate, what a raid took). A FOURTH and FIFTH kind of row, and they are not a trade feature:
    # `balance_supply_networks` pools between neighbours every turn, so before these rows a player
    # watching two co-networked bands saw both larders move for no stated reason.
    #
    # Rendered as income and debit respectively — `food_breakdown_row` takes the sign — and each is
    # omitted at zero, exactly like Lost to raids.
    #
    # **THEY READ THE PER-TURN PAIR, NEVER THE ACCUMULATING ONE** (issue #517). `transferReceived` /
    # `transferSent` accumulate over the PUBLICATION window and are cleared the moment the turn's
    # capture reads them, and the sim re-captures after every dispatched command — so on any frame a
    # command refreshed they read 0 and both rows vanished, which is what a player saw the instant
    # they did anything after a transfer landed. Every other row here is a per-turn value that
    # survives a recapture; `transferReceivedTurn` / `transferSentTurn` put these two on that same
    # basis. Rendering whichever of the two is non-zero is NOT the fix — it would make a row's
    # meaning depend on when it was looked at.
    var received := DetailFormat.band_transfer_received_turn(band)
    if received >= SourceForecast.FOOD_FLOW_MIN:
        lines.append(DetailFormat.food_breakdown_row(received, DetailFormat.FOOD_LABEL_TRANSFER_RECEIVED))
    var sent := DetailFormat.band_transfer_sent_turn(band)
    if sent >= SourceForecast.FOOD_FLOW_MIN:
        lines.append(DetailFormat.food_breakdown_row(-sent, DetailFormat.FOOD_LABEL_TRANSFER_SENT))
    return lines

## The FODDER larder's two flows, the rows under the `Fodder:` summary: what the band's fodder Fields
## GREW this turn and what its pens ATE. The fodder twin of `food_breakdown_lines`, and the reason
## that summary is two terms again.
##
## **THESE TWO RATES USED TO RIDE THE ROW ITSELF** — `Fodder: 100.0 · need 6.0/turn · growing
## 5.0/turn · 100 turns` — which wrapped to two lines in the narrow drawer column. That is a SHAPE
## problem, not a width one: the Food row has always split a two-term summary from a click-to-open
## breakdown, and the comment at the top of this file says why the rows are never appended inline.
##
## **`fodder_need` IS THE SIM'S SUM over the band's pens** (each pen's own share is its
## `pen_hay_need`), never re-summed here from the herd rows — those are fog-filtered, so a pen out of
## sight would silently drop out of a bill the band certainly still owes. `fodder_income` is the RAW
## harvest its fodder Fields took this turn.
##
## Each row is omitted below `SourceForecast.FODDER_FLOW_MIN` rather than printed as a zero, exactly
## as the food rows are: a band with no pens owes nothing, and `-0.0 Pens` is a debit invented on a
## band that pays none. A larder with neither flow registers no disclosure at all — `register`
## declines an empty payload — so the row keeps its caret honest: no caret, nothing behind it.
func fodder_breakdown_lines(band: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var grown := float(band.get("fodder_income", 0.0))
    if grown >= SourceForecast.FODDER_FLOW_MIN:
        lines.append(DetailFormat.fodder_breakdown_row(grown, DetailFormat.FODDER_LABEL_GROWN))
    var eaten := float(band.get("fodder_need", 0.0))
    if eaten >= SourceForecast.FODDER_FLOW_MIN:
        lines.append(DetailFormat.fodder_breakdown_row(-eaten, DetailFormat.FODDER_LABEL_PENS))
    return lines

## **`trade_breakdown_lines` IS RETIRED** (arc #527) with the Trade row it opened. It itemized the
## band's trade income by category (Gathered / Hunted); the account it summed no longer exists, and
## the materials that replaced it are held as batches rather than earned as a per-turn rate a
## category row could sum.

## **EVERY ITEM THE BAND CARRIES, EACH BESIDE THE ROLE IT SETS**
## (`docs/plan_hunt_through_combat.md` §4.8) — the Kit row's breakdown, and the only place a band's
## resolved tiers are stated.
##
## **THE PAIRING IS THE READOUT.** Spears → the hunt's `attack`, the SLED → the HUNT's carry, BASKETS
## → the FORAGE web's, HANDLING GEAR → the PEN's, WAYFINDING → a scout vantage's sight, CLUBS → the
## `attack` the camp is DEFENDED at. Every one of those is a separate wire field with its own
## durability behind it (a band can be out of baskets with its sled untouched, and it fights raids
## with clubs while it hunts with spears), so each row reads its OWN tier key and no two are ever
## interchanged — that substitution, baskets boosting the hunt, is the defect slice 5 corrected in
## the sim and the one this readout would otherwise carry forward into the UI.
##
## **THE ROWS ARE UNCONDITIONAL, including on a band with nobody on Scout or Warrior.** This popover
## answers "what does this band's gear DO", not "what is it doing this turn" — and a row that
## vanished when the role was unstaffed would hide the cliff exactly when a player is deciding
## whether to staff it again.
##
## **THE TIERS ARE READ, NEVER RE-DERIVED, AND NOTHING HERE TOUCHES `kit_id`.** The sim has already
## resolved each one through its OWN job's default kit, and that cohort id answers for the hunt job
## alone — quoting the vantage or the warrior's attack against it would read a scout's reach and a
## defender's fighting tier off the hunting kit (see `DetailFormat.KIT_TIER_KEY_PEN_CARRY`).
##
## **NOTHING HERE IS SCALED BY THE REMAINING CONDITION.** Performance is flat until expiry, so a kit
## at 3 quotes the same tier as one at 97 — the condition says how long, the tier says how well, and
## the closing sentence is what stops a player reading the first as the second.
func kit_breakdown_lines(band: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    if not DetailFormat.band_states_kit(band):
        return lines
    lines.append(DetailFormat.kit_breakdown_row(band, DetailFormat.KIT_DURABILITY_KEY_SPEARS,
        DetailFormat.KIT_LABEL_SPEARS, DetailFormat.KIT_ROLE_ATTACK_FORMAT % String.num(
            float(band.get(DetailFormat.KIT_TIER_KEY_ATTACK, 0.0)),
            DetailFormat.KIT_CONDITION_DECIMALS)))
    lines.append(DetailFormat.kit_breakdown_row(band, DetailFormat.KIT_DURABILITY_KEY_SLED,
        DetailFormat.KIT_LABEL_SLED, DetailFormat.KIT_ROLE_HUNT_CARRY_FORMAT % String.num(
            float(band.get(DetailFormat.KIT_TIER_KEY_HUNT_CARRY, 0.0)),
            DetailFormat.KIT_CARRY_DECIMALS)))
    lines.append(DetailFormat.kit_breakdown_row(band, DetailFormat.KIT_DURABILITY_KEY_BASKETS,
        DetailFormat.KIT_LABEL_BASKETS, DetailFormat.KIT_ROLE_FORAGE_CARRY_FORMAT % String.num(
            float(band.get(DetailFormat.KIT_TIER_KEY_FORAGE_CARRY, 0.0)),
            DetailFormat.KIT_CARRY_DECIMALS)))
    # **Traps state their REACH, not a carry or an attack** — the term that actually binds on light
    # game, where a weapon's damage buys nothing because there is no defence to clear.
    lines.append(DetailFormat.kit_breakdown_row(band, DetailFormat.KIT_DURABILITY_KEY_TRAPS,
        DetailFormat.KIT_LABEL_TRAPS, DetailFormat.KIT_ROLE_TRAPS))
    # **HANDLING GEAR IS THE PEN'S CARRY AND IT IS NOT THE SLED'S.** A sled drags a carcass in off
    # the range; a pen stands at the camp, so a band on a stalking kit collects its pen at the bare
    # rate however healthy the sled is. Two rows, two tiers, both quoted at the hunt job's default.
    # **AND IT TAKES WORK OFF THE CLIMB, which is the other half of what it is for** (issue #515, and
    # `docs/plan_unit_costed_work.md` §6 for the change from a rate to work units). The build axis has
    # no flat per-band field — it rides the band's own `kit_tiers` row, resolved here rather than in
    # `DetailFormat` so the pure format layer keeps depending on nothing.
    var husbandry_role := DetailFormat.KIT_ROLE_PEN_CARRY_FORMAT % String.num(
        float(band.get(DetailFormat.KIT_TIER_KEY_PEN_CARRY, 0.0)),
        DetailFormat.KIT_CARRY_DECIMALS)
    var build_work := float(KitRoster.band_kit_tiers(band, String(band.get(
        DetailFormat.BAND_QUOTED_KIT_ID_KEY, ""))).get(
            KitRoster.KIT_BUILD_WORK_KEY, DetailFormat.KIT_BUILD_WORK_NEUTRAL))
    if build_work > DetailFormat.KIT_BUILD_WORK_NEUTRAL:
        husbandry_role += DetailFormat.KIT_ROLE_BUILD_WORK_SUFFIX % String.num(
            build_work, DetailFormat.KIT_BUILD_WORK_DECIMALS)
    lines.append(DetailFormat.kit_breakdown_row(band,
        DetailFormat.KIT_DURABILITY_KEY_HURDLES, DetailFormat.KIT_LABEL_HURDLES,
        husbandry_role))
    # **WAYFINDING STATES TILES, NOT A BIOMASS RATE**, so it takes the vantage's own rounding rather
    # than the carries'. It is how far each POSTED vantage sees — how far out they are posted is not
    # a kit axis at all, so this row must not be read as the scouting reach.
    lines.append(DetailFormat.kit_breakdown_row(band, DetailFormat.KIT_DURABILITY_KEY_WAYFINDING,
        DetailFormat.KIT_LABEL_WAYFINDING, DetailFormat.KIT_ROLE_SCOUT_VANTAGE_FORMAT % String.num(
            float(band.get(DetailFormat.KIT_TIER_KEY_SCOUT_VANTAGE, 0.0)),
            DetailFormat.KIT_VANTAGE_DECIMALS)))
    # **CLUBS ARE THE DEFENDERS' ATTACK AND THEY ARE NOT THE SPEARS'.** Same stat, different item and
    # a different kit: a raid is fought at the camp with whatever is by the fire, so this row reads
    # `warrior_attack` and never `hunter_attack` — the tier `kit_id` would wrongly answer for.
    lines.append(DetailFormat.kit_breakdown_row(band, DetailFormat.KIT_DURABILITY_KEY_CLUBS,
        DetailFormat.KIT_LABEL_CLUBS, DetailFormat.KIT_ROLE_WARRIOR_ATTACK_FORMAT % String.num(
            float(band.get(DetailFormat.KIT_TIER_KEY_WARRIOR_ATTACK, 0.0)),
            DetailFormat.KIT_CONDITION_DECIMALS)))
    # A full-width sentence, deliberately NOT indented: `detail_bbcode` routes the indent to the
    # two-tone sub-row branch, and this is a caveat on every row rather than one more of them.
    lines.append(DetailFormat.KIT_BREAKDOWN_CLIFF_NOTE)
    return lines

## Meta dispatcher for the summary-row disclosures (Food/Morale): the `[url]` meta IS the disclosure
## key, so the handler needs no band lookup and no host flag — the SAME click behaviour wherever the
## row renders. `anchor` is the label that emitted the click, bound at wire time; it is what the
## popover positions under.
func _on_meta_clicked(meta: Variant, anchor: Control) -> void:
    var payload := String(meta)
    # A FACTION drill-down row: change the panel's subject to that band and close the popover behind
    # it. Dispatched BEFORE the breakdown branch and on its own prefix — a band jump and a breakdown
    # toggle are different acts, and a shared prefix would make which one fired a matter of parsing.
    if payload.begins_with(HudDisclosureVocab.FACTION_BAND_JUMP_META_PREFIX):
        var entity := int(payload.substr(HudDisclosureVocab.FACTION_BAND_JUMP_META_PREFIX.length()))
        _close_popover()
        if _faction_band_jump.is_valid():
            _faction_band_jump.call(entity)
        return
    if not payload.begins_with(HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX):
        return
    var key := payload.substr(HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX.length())
    if _breakdown_popover_key == key:
        _close_popover()
        return
    _open_popover(key, anchor)

## Open a disclosure's breakdown in the popover, anchored under the clicked row. The anchor rect is
## captured BEFORE the hosts re-render, because that render frees the very label we are anchoring to
## (the panel builds a fresh vitals label each time).
func _open_popover(key: String, anchor: Control) -> void:
    var lines := _lines_for(key)
    if lines.is_empty():
        return
    var anchor_rect := _anchor_rect(anchor)
    _breakdown_popover_key = key
    _notify_hosts()
    var popover := _ensure_popover()
    _refresh_popover_text()
    popover.popup(anchor_rect)

## Dismiss the breakdown popover, if any. Idempotent — `popup_hide` runs the same teardown, so a
## click-away / Esc and an explicit close converge on one path.
func _close_popover() -> void:
    if _breakdown_popover != null and _breakdown_popover.visible:
        _breakdown_popover.hide()
        return
    _on_popover_hidden()

func _on_popover_hidden() -> void:
    if _breakdown_popover_key == NO_OPEN_KEY:
        return
    _breakdown_popover_key = NO_OPEN_KEY
    _notify_hosts()

## The rows a disclosure key's popover shows — stashed by `register`, never recomputed.
func _lines_for(key: String) -> Array[String]:
    var stored: Variant = _breakdown_payloads.get(key, null)
    var lines: Array[String] = []
    if stored is Array:
        for line in (stored as Array):
            lines.append(String(line))
    return lines

## Where the popover sits: a zero-height rect at the bottom-left of the clicked row, in SCREEN space
## (what `Popup.popup` wants). `get_screen_transform` folds in the window position and the canvas
## stretch, both of which this HUD has.
func _anchor_rect(anchor: Control) -> Rect2i:
    if anchor == null or not is_instance_valid(anchor):
        return Rect2i()
    var xform := anchor.get_screen_transform()
    var below := xform * Vector2(0.0, anchor.size.y + BREAKDOWN_POPOVER_GAP)
    return Rect2i(Vector2i(below), Vector2i.ZERO)

## The popover itself: a `PopupPanel`, so it is a WINDOW — it cannot change any zone's height, which
## is the whole reason the breakdown moved here. Styled through `HudStyle` like every other card.
func _ensure_popover() -> PopupPanel:
    if _breakdown_popover != null and is_instance_valid(_breakdown_popover):
        return _breakdown_popover
    var popover := PopupPanel.new()
    popover.name = "BreakdownPopover"
    popover.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
    var margin := MarginContainer.new()
    for side in POPOVER_MARGIN_SIDES:
        margin.add_theme_constant_override("margin_%s" % side, BREAKDOWN_POPOVER_PADDING)
    popover.add_child(margin)
    var label := RichTextLabel.new()
    label.bbcode_enabled = true
    label.fit_content = true
    label.scroll_active = false
    label.autowrap_mode = TextServer.AUTOWRAP_WORD
    label.custom_minimum_size = Vector2(BREAKDOWN_POPOVER_WIDTH, 0.0)
    # **THE POPOVER'S OWN LABEL TAKES METAS NOW.** Every breakdown until the faction page was inert
    # prose — indented ▲/▼ rows with nothing to click — so this label had no handler at all. The
    # faction rows are LINKS TO BANDS, so it needs one; it binds ITSELF as the anchor like every other
    # wired label, which is inert for the jump branch and correct if a breakdown ever grows a caret.
    label.meta_clicked.connect(_on_meta_clicked.bind(label))
    margin.add_child(label)
    popover.popup_hide.connect(_on_popover_hidden)
    _popover_host.add_child(popover)
    _breakdown_popover = popover
    _breakdown_popover_label = label
    return popover

## Restate the open popover from the current payload — the breakdown CONTENT is unchanged from the
## inline form: the same indented ▲/▼ rows through the same shared two-tone tint path. It carries no
## band tint context of its own: every row it shows is an indented sub-line, which the formatter tints
## by sign glyph alone.
func _refresh_popover_text() -> void:
    if _breakdown_popover_label == null or not is_instance_valid(_breakdown_popover_label):
        return
    _breakdown_popover_label.text = DetailFormat.detail_bbcode(_lines_for(_breakdown_popover_key))

## Re-render whichever hosts can be showing a disclosure caret, so it flips with the popover. Both
## hosts, unconditionally — that is the `is_panel` fork this cluster exists to remove.
func _notify_hosts() -> void:
    if _refresh_hosts.is_valid():
        _refresh_hosts.call()
