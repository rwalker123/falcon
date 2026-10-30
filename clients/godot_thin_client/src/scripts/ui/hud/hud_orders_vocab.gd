class_name HudOrdersVocab

## **BAND ORDERS -- the chief's standing orders, in words** (#860, `docs/plan_labor_at_scale.md` ->
## "Band orders -- the chief"). Everything the Work tab's orders group, the stone/wood/food/fodder
## sheets and the hunt/gather sheets' chief strips SAY lives here, with the pure functions that pick
## which words: the order list a band shows, an order's state, its ETA, the summary line.
##
## The wire (`band_orders` on the band dict, `order_kind` / `chief_owned` on each labor row) is read
## through this module only, so a key is spelled once.

# ---- The wire -----------------------------------------------------------------------------------

const BAND_ORDERS_KEY := "band_orders"
const KIND_FOOD := "food"
const KIND_FODDER := "fodder"
const ROW_ORDER_KIND_KEY := "order_kind"
const ROW_CHIEF_OWNED_KEY := "chief_owned"
## `leader_kind` 0 is no leader (`HudFormat.LEADER_KIND_NONE`); 2 is an elders' council.
const LEADER_KIND_NONE := 0
const LEADER_KIND_COUNCIL := 2

const ACTION_SET := "set"
const ACTION_RELEASE := "release"
const ACTION_CLAIM := "claim"
const COMMAND_SET_MESSAGE := "%s order set to keep %d."
const COMMAND_RELEASE_MESSAGE := "You are crewing it yourself."
const COMMAND_CLAIM_MESSAGE := "Handed to your leader."

const PRIORITY_HIGH := "high"
const PRIORITY_NORMAL := "normal"
const PRIORITY_LOW := "low"

# ---- The leader, by name -------------------------------------------------------------------------

const COUNCIL_SUBJECT := "The elders"
const COUNCIL_SUBJECT_LOWER := "the elders"
const GROUP_TITLE_FORMAT := "%s's orders"
const COUNCIL_GROUP_TITLE := "The elders' orders"
const POSSESSIVE_FORMAT := "%s's"
const COUNCIL_POSSESSIVE := "The elders'"

# ---- The order line ------------------------------------------------------------------------------

const ORDER_NAMES := {KIND_FOOD: "Food", KIND_FODDER: "Fodder"}
const ORDER_ICONS := {KIND_FOOD: "🍖", KIND_FODDER: "🌾"}
const ORDER_NAME_META := &"band_order_name"
const ORDER_LINE_META := &"band_order_line"
const ORDER_STATE_META := &"band_order_state"
const ORDER_CREW_BAR_META := &"band_order_crew_bar"
const ORDER_SUMMARY_META := &"band_order_summary"
const ORDER_ALSO_META := &"band_order_also"
const ORDER_GROUP_META := &"band_orders_group"
const ORDER_PRIORITY_META := &"band_order_priority"
## The order line's ways in to its sheet, and the kit clause that opens the bench (playtest: the sheet was
## findable only from a name that did not look clickable).
const ORDER_DETAIL_META := &"band_order_detail"
const ORDER_OPEN_META := &"band_order_open"
const ORDER_KIT_META := &"band_order_kit"
const OPEN_CHEVRON := "›"
const OPEN_TIP_FORMAT := "Set %s %s order"                     # "Set Orrin's food order"
const COUNCIL_POSSESSIVE_LOWER := "the elders'"
const OPEN_CHEVRON_FONT_SIZE := 16
## `· short 3 hoes, 2 spears`: appended in amber to the state; pressing it opens the crafting bench.
const KIT_CLAUSE_FORMAT := "· short %s"
const KIT_ITEM_FORMAT := "%d %s"
const KIT_ITEM_SEPARATOR := ", "
const KIT_TIP_ROW_FORMAT := "%s needs %s"                      # "Tended Patch (29, 24) needs 3 hoes"
const KIT_TIP_BENCH_LINE := "Open the crafting bench"

const LINE_TURNS_FORMAT := "· %s · keep %s"
const LINE_RELEASED_FORMAT := "· %s"
const LINE_MATERIAL_FORMAT := "· %s on hand · keep %d"
const TURNS_ONE := "%d turn"
const TURNS_MANY := "%d turns"
const HANDS_ONE := "%d hand"
const HANDS_MANY := "%d hands"
const PRIORITY_CHIP_FORMAT := "Priority: %s"

const STATE_FALLING_HOLD_FORMAT := "Falling — %s short of keeping up"
const STATE_FALLING_SUPPLY_FORMAT := "Falling %s a turn — %s"
## Why the supply is short, per order kind: the words after the dash. A material is the stands'.
const SUPPLY_SHORT_WORDS := {
    KIND_FOOD: "the land gives too little",
    KIND_FODDER: "the patches give too little",
}
const SUPPLY_SHORT_MATERIAL_WORDS := "the stands regrow too little"
const STATE_HOLDING_FORMAT := "Holding — no hands to refill (%d wanted)"
const STATE_SLOW_FORMAT := "Refilling slowly — %s short"
const STATE_AT_TARGET := "At target"
const STATE_REFILL_ETA_FORMAT := "Refilling · back to %s in ≈%s"
const STATE_REFILL_FORMAT := "Refilling · back to %s"
const TONE_PLAIN := &"plain"
const TONE_OK := &"ok"
const TONE_WARN := &"warn"
const TONE_BAD := &"bad"

## The crew bar: keeps up / refills / short, and the key under the group.
const CREW_KEEPS := "keeps up"
const CREW_REFILLS := "refills"
const CREW_SHORT := "short"
const CREW_TIP_FORMAT := "%d keep up with what the band uses · %d refill"
const CREW_TIP_SHORT_FORMAT := " · %d short"
const CREW_BAR_WIDTH := 54.0
## A short hand is drawn at this fraction of the warn colour, so a gap reads as missing and not as a
## working crew.
const CREW_SHORT_ALPHA := 0.45

const SUMMARY_FORMAT := "%s orders take %d of %d workers."
const SUMMARY_REFILL_SHORT_FORMAT := "Not enough hands for every order — %s %s."
const SUMMARY_HOLD_SHORT_FORMAT := "Not enough hands to keep up — %s %s running down."
const SUMMARY_WAIT_ONE := "waits"
const SUMMARY_WAIT_MANY := "wait"
const SUMMARY_ARE_ONE := "is"
const SUMMARY_ARE_MANY := "are"
const NAMES_AND := " and "
const NAMES_SEPARATOR := ", "

# ---- The group's folds ---------------------------------------------------------------------------

## The shown orders in the order they matter: Food, Fodder, then the materials in wire order.
const ORDER_RANK := {KIND_FOOD: 0, KIND_FODDER: 1}
const MATERIAL_RANK := 2
const RESERVE_ONE_FORMAT := "+%d site in reserve"
const RESERVE_MANY_FORMAT := "+%d sites in reserve"
const ORDER_CARET_META := &"band_order_caret"
const ORDER_RESERVE_META := &"band_order_reserve"
const CARET_OPEN_TIP := "Hide the sites under this order"
const CARET_CLOSED_TIP := "Show the sites under this order"
const RESERVE_OPEN_TIP := "Hide the reserve sites"
const RESERVE_CLOSED_TIP := "Show the reserve sites"
const CARET_WIDTH := 16.0
const CARET_HEIGHT := 18.0
const CARET_TRIANGLE_SIDE := 7.0
const RESERVE_INDENT := 22

const ALSO_FORMAT := "↳ Also +%s %s from %s"
const ALSO_UNDER_FORMAT := "listed under %s"
const ALSO_AMOUNT_FORMAT := "%.2f"
const ALSO_FODDER_WORD := "fodder"
const ALSO_FOOD_WORD := "food"

# ---- Also coming in: what no order asks for -----------------------------------------------------

const BY_PRODUCTS: Array[String] = ["hide", "bone", "fibre"]
const BY_PRODUCT_NAMES := {"hide": "Hide", "bone": "Bone", "fibre": "Fibre"}
const BY_PRODUCT_ICONS := {"hide": "🦌", "bone": "🦴", "fibre": "🧵"}
const BY_PRODUCT_SOURCE := {"hide": "hunting", "bone": "hunting", "fibre": "gathering"}
const BY_PRODUCT_ON_HAND_FORMAT := "%s · %s on hand"
const BY_PRODUCT_GAIN_FORMAT := "+%.1f a turn from %s"
const BY_PRODUCT_META := &"band_order_by_product"

# ---- The sheets ---------------------------------------------------------------------------------

const CHIEF_BOX_TAG := "Chief's order"
const CHIEF_KEEPS_MATERIAL_FORMAT := "%s %s on hand"           # "Orrin keeps stone on hand"
const CHIEF_KEEPS_FOOD_FORMAT := "%s the band fed"             # "Orrin keeps the band fed"
const CHIEF_KEEPS_FODDER_FORMAT := "%s the herds fed"
const KEEP_ON_HAND_LABEL := "Keep on hand"
const KEEP_WORD := "keep"
const PRIORITY_LABEL := "Priority"
const KEEP_LESS_TIP_FORMAT := "Keep less %s"
const KEEP_MORE_TIP_FORMAT := "Keep more %s"
const KEEP_LESS_FOOD_TIP := "Keep fewer turns of %s"
const KEEP_MORE_FOOD_TIP := "Keep more turns of %s"
const STEPPER_TURNS_FORMAT := "%d turns"
const KEEP_STEP := 1
const KEEP_MIN := 1

const CREW_NOUNS := {"stone": "diggers", "wood": "foresters"}
const CREW_BOX_FOOD := "Food sources"
const CREW_BOX_FODDER := "Fodder land"
const SENDS_FORMAT := "%s %d %s this turn"                      # "Orrin sends 5 diggers this turn"
const SENDS_TO_FORMAT := "%s %d to %s this turn"                # "Orrin sends 14 to food this turn"
const AT_TARGET_NOBODY_FORMAT := "At target — nobody %s this turn"
const NOBODY_VERBS := {"stone": "digs", "wood": "cuts"}
const SPLIT_FORMAT := "%d with %s · %d bare-handed"
const SPLIT_KIT_WORDS := {"stone": "sleds", "wood": "axes"}
const HOLD_REFILL_FORMAT := "%d keep up with what the band uses, %s toward %d."
const REFILL_ONE := "%d refills"
const REFILL_MANY := "%d refill"
const DIGGING_ELSEWHERE_FORMAT := "%s is digging at %s."
const SITE_NAME_FORMAT := "%s (%d, %d)"
const SITE_COORDS_FORMAT := "(%d, %d)"
const STAND_HERE := "here"
const LEAVES_STANDS := "He leaves every stand to grow back."
const LEAVES_STANDS_COUNCIL := "They leave every stand to grow back."
const STANDS_SHORT_FORMAT := "⚠ Stands in reach regrow %.1f wood a turn — %s uses %.1f."
const AT_TARGET_DIGGERS_FORMAT := "Diggers go back to work the turn stone drops below %d."
const STOCK_FINITE_FORMAT := "%d of %d %s left in this rock. %s does not come back."
const STOCK_RENEWS_FORMAT := "%d of %d %s standing here. %s grows back."

const NEXT_TURN_LABEL := "Next turn"
const NEXT_TURN_NOTHING_FORMAT := "Nothing %s while the pile is full."
const NEXT_TURN_BACK_FORMAT := "Back to %s in ≈%s."
const NEXT_TURN_FALLS_FORMAT := "%s falls %.2f a turn."
const AFTER_EATING_FORMAT := "%s after eating"

const SHOW_ON_MAP := "Show on map"
const CREW_IT_YOURSELF := "Crew it yourself"
const HAND_IT_FORMAT := "Hand it to %s"
## A released order's state line on the Work tab: the player crews it, and the link hands it back.
const CREWED_BY_YOU := "You crew it — "
const ORDER_HAND_BACK_META := &"band_order_hand_back"
const ORDER_RELEASED_META := &"band_order_released"
const DONE := "Done"
const OFFER_MATERIAL_FORMAT := "%s could keep %s on hand for you."
const OFFER_ROW_FORMAT := "%s could %s this for the %s order."
const STRIP_ROW_FORMAT := "%s this for "
const STRIP_ROW_LINK_FORMAT := "the %s order"
const STRIP_ROW_END := "."
## Source kind -> [what the chief does (singular verb), the same unconjugated], per order.
const ROW_VERBS := {
    KIND_FOOD: {"hunt": "hunts", "forage": "gathers"},
    KIND_FODDER: {"hunt": "cuts", "forage": "cuts"},
}

const FOOD_EATS_FORMAT := "%s eats %.1f food a turn."
const FODDER_EATS_FORMAT := "The herds eat %.1f fodder a turn beyond their range."
const FODDER_NO_HERDS_FORMAT := "No tamed or penned herd eats fodder yet. The order starts the turn %s keeps one."
const WHAT_EATS_IT := "What eats it"
const PEN_FODDER_FORMAT := "%.1f fodder a turn"
const PEN_SPLIT_TIP_FORMAT := "Range covers %d%% of what this herd eats; fodder covers the rest."
const PEN_SPLIT_KEY := "Green is what each herd's range covers; the rest is fodder."
const PEN_NAME_FORMAT := "%s (%d, %d)"
const PEN_BAR_HEIGHT := 6.0

const SRC_BEST := "Best"
const SRC_LEARNING := "Learning"
const SRC_FLOOR_YOURS_FORMAT := "%d%% left · yours"
const ROW_HARVEST_FORMAT := "Harvest (%d, %d)"
const SRC_TAKE_FORMAT := "+%.2f"
const EASE_BEST_FORMAT := "%s works every row at Best and never overdraws."
const EASE_LEARNING_FORMAT := "Above %d turns — %s eases every row to Learning."
const CUT_BEST_FORMAT := "%s cuts every meadow at Best and never overdraws."
const NO_ROWS := "Nobody is sent while the order is met."
const BYPRODUCT_FORMAT := "Also brings in hide +%.1f and bone +%.1f a turn."
const FODDER_COMES_HOME_FORMAT := "+%.2f fodder comes home with the food from %s."

const CHIEF_ROW_DISABLED_TIP_FORMAT := "%s sets this while the order runs it. Press Crew it yourself to take it back."

## The widths/heights a sheet section is drawn at.
const PORTRAIT_SMALL := 32.0
const SECTION_SEPARATION := 6
const BOX_PADDING := 8
const SRC_ROW_SEPARATION := 8
const SRC_NAME_MIN_WIDTH := 120.0
const GROUP_PADDING := 8
const GROUP_SEPARATION := 4
const ORDER_ICON_WIDTH := 18.0
const LINE_FONT_SIZE := 12
const SMALL_FONT_SIZE := 10
const STATE_FONT_SIZE := 11
const NAME_FONT_SIZE := 13
const ALSO_FONT_SIZE := 11
const KEY_SWATCH := 8.0

# ---- Reading the wire ---------------------------------------------------------------------------

## Does this band have a leader at all (so band orders exist for it)?
static func has_leader(band: Dictionary) -> bool:
    return int(band.get("leader_kind", LEADER_KIND_NONE)) != LEADER_KIND_NONE \
        and String(band.get("leader_name", "")).strip_edges() != ""

static func is_council(band: Dictionary) -> bool:
    return int(band.get("leader_kind", LEADER_KIND_NONE)) == LEADER_KIND_COUNCIL

## Every order the band's wire holds, in wire order.
static func all_orders(band: Dictionary) -> Array:
    var out: Array = []
    for entry in band.get(BAND_ORDERS_KEY, []):
        if entry is Dictionary:
            out.append(entry)
    return out

## The orders the Work tab shows: the wire's, in its order, with fodder skipped while nothing eats hay.
static func shown_orders(band: Dictionary) -> Array:
    if not has_leader(band):
        return []
    var out: Array = []
    for order in all_orders(band):
        if String(order.get("kind", "")) == KIND_FODDER and not bool(order.get("has_consumers", false)):
            continue
        out.append(order)
    # Rank pass: Food, Fodder, then the materials in wire order.
    var ranked: Array = []
    for rank in range(MATERIAL_RANK + 1):
        for order in out:
            if order_rank(String(order.get("kind", ""))) == rank:
                ranked.append(order)
    return ranked

static func order_rank(kind: String) -> int:
    return int(ORDER_RANK.get(kind, MATERIAL_RANK))

static func order_of(band: Dictionary, kind: String) -> Dictionary:
    if not has_leader(band):
        return {}
    for order in all_orders(band):
        if String(order.get("kind", "")) == kind:
            return order
    return {}

static func is_turns_kind(kind: String) -> bool:
    return kind == KIND_FOOD or kind == KIND_FODDER

## Is a row one the chief runs -- the wire's own bool, and the order that lists it.
static func row_is_chiefs(row: Dictionary) -> bool:
    return bool(row.get(ROW_CHIEF_OWNED_KEY, false)) and row_order_kind(row) != ""

static func row_order_kind(row: Dictionary) -> String:
    return String(row.get(ROW_ORDER_KIND_KEY, ""))

static func is_chief_owned(order: Dictionary) -> bool:
    return bool(order.get("chief_owned", false))

## The orders the chief is working: a released order (`chief_owned` false) is the player's and is left out.
static func chief_orders(orders: Array) -> Array:
    var out: Array = []
    for order in orders:
        if is_chief_owned(order as Dictionary):
            out.append(order)
    return out

# ---- Names ---------------------------------------------------------------------------------------

## "Orrin" / "The elders" -- the subject of a sentence.
static func who(band: Dictionary) -> String:
    return COUNCIL_SUBJECT if is_council(band) else String(band.get("leader_name", "")).strip_edges()

## ...mid-sentence: "Hand it to Orrin" / "Hand it to the elders".
static func who_lower(band: Dictionary) -> String:
    return COUNCIL_SUBJECT_LOWER if is_council(band) else String(band.get("leader_name", "")).strip_edges()

## The subject and its verb agreeing: `("keeps")` -> "Orrin keeps" / "The elders keep".
static func who_verb(band: Dictionary, singular_verb: String) -> String:
    var verb := singular_verb
    if is_council(band) and verb.ends_with("s"):
        verb = verb.substr(0, verb.length() - 1)
    return "%s %s" % [who(band), verb]

static func possessive(band: Dictionary) -> String:
    return COUNCIL_POSSESSIVE if is_council(band) else POSSESSIVE_FORMAT % who(band)

## "Orrin's" / "the elders'" -- the possessive mid-sentence.
static func possessive_lower(band: Dictionary) -> String:
    return COUNCIL_POSSESSIVE_LOWER if is_council(band) else possessive(band)

## `Set Orrin's food order`: the tooltip on every way in to the order's sheet.
static func open_tip(band: Dictionary, kind: String) -> String:
    return OPEN_TIP_FORMAT % [possessive_lower(band), order_name(kind).to_lower()]

## **WHAT ONE ROW IS SHORT OF**, off its `kit_toe` lines: `{on-screen item name: count short}`. The count is
## `required - filled` rounded UP (a sub-unit gap is a unit still to make), the name the roster's display name
## lower-cased -- never the id. A line the roster has no name for is skipped, like the row's own warning.
static func row_kit_short(toe: Array, item_names: Dictionary) -> Dictionary:
    var short: Dictionary = {}
    for line_variant in toe:
        if not (line_variant is Dictionary) or not HudWorkVocab.pool_toe_row_is_short(line_variant):
            continue
        var line: Dictionary = line_variant
        var item := String(item_names.get(String(line.get(HudBandLaborState.POOL_TOE_ITEM_KEY, "")), "")) \
            .strip_edges().to_lower()
        if item == "":
            continue
        var required := maxf(float(line.get(HudBandLaborState.POOL_TOE_REQUIRED_KEY, 0.0)), 0.0)
        var filled := clampf(float(line.get(HudBandLaborState.POOL_TOE_FILLED_KEY, 0.0)), 0.0, required)
        short[item] = int(short.get(item, 0)) + maxi(ceili(required - filled), 0)
    return short

## `3 hoes, 2 spears`: items sorted by the count short, descending (ties by name).
static func kit_items_text(short: Dictionary) -> String:
    var names: Array = short.keys()
    names.sort_custom(func(a: String, b: String) -> bool:
        if int(short[a]) != int(short[b]):
            return int(short[a]) > int(short[b])
        return a < b)
    var parts: Array[String] = []
    for item in names:
        parts.append(KIT_ITEM_FORMAT % [int(short[item]), item])
    return KIT_ITEM_SEPARATOR.join(PackedStringArray(parts))

## **THE ORDER LINE'S KIT CLAUSE** from `entries` (`{name, short}` per chief row of the order):
## `{clause: "· short 3 hoes", tip: one line per short row}`, or `{}` where nothing is short.
static func kit_clause(entries: Array) -> Dictionary:
    var total: Dictionary = {}
    var lines: Array[String] = []
    for entry_variant in entries:
        var entry: Dictionary = entry_variant
        var short: Dictionary = entry.get("short", {})
        if short.is_empty():
            continue
        for item in short:
            total[item] = int(total.get(item, 0)) + int(short[item])
        lines.append(KIT_TIP_ROW_FORMAT % [String(entry.get("name", "")), kit_items_text(short)])
    if total.is_empty():
        return {}
    lines.append(KIT_TIP_BENCH_LINE)
    return {"clause": KIT_CLAUSE_FORMAT % kit_items_text(total), "tip": "\n".join(PackedStringArray(lines))}

static func group_title(band: Dictionary) -> String:
    return COUNCIL_GROUP_TITLE if is_council(band) else GROUP_TITLE_FORMAT % who(band)

static func order_name(kind: String) -> String:
    if ORDER_NAMES.has(kind):
        return String(ORDER_NAMES[kind])
    return kind.capitalize()

static func order_icon(kind: String) -> String:
    if ORDER_ICONS.has(kind):
        return String(ORDER_ICONS[kind])
    return FoodIcons.for_material(kind)

static func priority_face(level: String) -> String:
    return String(HudWorkVocab.WORK_PRIORITY_FACES.get(HudWorkVocab.work_priority_of(level), "Normal"))

static func hands(count: int) -> String:
    return (HANDS_ONE if count == 1 else HANDS_MANY) % count

static func turns(count: int) -> String:
    return (TURNS_ONE if count == 1 else TURNS_MANY) % count

# ---- The order's numbers --------------------------------------------------------------------------

static func target_text(order: Dictionary) -> String:
    var target := int(order.get("target", 0))
    return turns(target) if is_turns_kind(String(order.get("kind", ""))) else str(target)

static func have_text(order: Dictionary) -> String:
    var on_hand := float(order.get("on_hand", 0.0))
    if is_turns_kind(String(order.get("kind", ""))):
        return turns(int(floorf(on_hand)))
    return "%.1f on hand" % on_hand

## A released order's detail, no target (nothing is being worked toward it): `· 12 turns` / `· 12.4 on hand`.
static func released_line_text(order: Dictionary) -> String:
    return LINE_RELEASED_FORMAT % have_text(order)

## `· 12 turns · keep 20 turns` / `· 12.4 on hand · keep 30`.
static func line_text(order: Dictionary) -> String:
    if is_turns_kind(String(order.get("kind", ""))):
        return LINE_TURNS_FORMAT % [have_text(order), target_text(order)]
    return LINE_MATERIAL_FORMAT % ["%.1f" % float(order.get("on_hand", 0.0)), int(order.get("target", 0))]

## Whole turns until the order is back at its target, rounded up, or `-1` when the take does not
## outrun the draw (so there is no honest figure).
static func eta_turns(order: Dictionary) -> int:
    var draw := float(order.get("draw", 0.0))
    var take := float(order.get("take", 0.0))
    var gap := float(order.get("target", 0)) - float(order.get("on_hand", 0.0))
    if take <= draw or gap <= 0.0:
        return -1
    var surplus := take - draw
    var raw := gap * draw / surplus if is_turns_kind(String(order.get("kind", ""))) else gap / surplus
    return maxi(ceili(raw - 0.000001), 1)

static func crew_of(order: Dictionary) -> int:
    return int(order.get("hold_heads", 0)) + int(order.get("refill_heads", 0))

static func hold_short(order: Dictionary) -> int:
    return maxi(int(order.get("hold_wanted", 0)) - int(order.get("hold_heads", 0)), 0)

static func refill_short(order: Dictionary) -> int:
    return maxi(int(order.get("refill_wanted", 0)) - int(order.get("refill_heads", 0)), 0)

## Is the order falling in earnest: a hold crew short AND the take under the draw.
static func is_falling(order: Dictionary) -> bool:
    return hold_short(order) > 0 and float(order.get("take", 0.0)) < float(order.get("draw", 0.0))

## `{text, tone}`: the first state that matches --
## 1. the supply is below the draw (any kind): amber Falling, naming where the shortfall is;
## 2. a hold crew short AND the take under the draw: red Falling;
## 3. a hold crew short, or a refill crew short that has some heads: amber Refilling slowly;
## 4. a refill wanted with no refill heads: amber Holding;
## 5. on hand has reached the target: At target;
## 6. else green Refilling.
static func state_of(order: Dictionary) -> Dictionary:
    var kind := String(order.get("kind", ""))
    var draw := float(order.get("draw", 0.0))
    var sustainable := float(order.get("sustainable", 0.0))
    if sustainable < draw:
        var why := String(SUPPLY_SHORT_WORDS.get(kind, SUPPLY_SHORT_MATERIAL_WORDS))
        return {"text": STATE_FALLING_SUPPLY_FORMAT % ["%.1f" % (draw - sustainable), why],
            "tone": TONE_WARN}
    var short_hold := hold_short(order)
    if is_falling(order):
        return {"text": STATE_FALLING_HOLD_FORMAT % hands(short_hold), "tone": TONE_BAD}
    var wanted := int(order.get("refill_wanted", 0))
    var heads := int(order.get("refill_heads", 0))
    var short_refill := refill_short(order)
    if short_hold > 0 or (short_refill > 0 and heads > 0):
        return {"text": STATE_SLOW_FORMAT % hands(short_hold + short_refill), "tone": TONE_WARN}
    if wanted > 0 and heads == 0:
        return {"text": STATE_HOLDING_FORMAT % wanted, "tone": TONE_WARN}
    if float(order.get("on_hand", 0.0)) >= float(order.get("target", 0)):
        return {"text": STATE_AT_TARGET, "tone": TONE_PLAIN}
    var eta := eta_turns(order)
    if eta < 0:
        return {"text": STATE_REFILL_FORMAT % target_text(order), "tone": TONE_OK}
    return {"text": STATE_REFILL_ETA_FORMAT % [target_text(order), turns(eta)], "tone": TONE_OK}

## `Food`, `Food and Wood`, `Food, Stone and Wood`.
static func join_names(names: Array) -> String:
    if names.size() <= 1:
        return "" if names.is_empty() else String(names[0])
    var head := NAMES_SEPARATOR.join(names.slice(0, names.size() - 1))
    return head + NAMES_AND + String(names[names.size() - 1])

## The summary's `{line, warning, tone}` over the shown orders.
static func summary_of(band: Dictionary, orders: Array) -> Dictionary:
    var takes := 0
    var falling: Array = []
    var waiting: Array = []
    for order in chief_orders(orders):
        takes += crew_of(order)
        var order_label := order_name(String(order.get("kind", "")))
        if is_falling(order) and float(order.get("sustainable", 0.0)) >= float(order.get("draw", 0.0)):
            falling.append(order_label)
        if hold_short(order) > 0 or refill_short(order) > 0:
            waiting.append(order_label)
    var line := SUMMARY_FORMAT % [possessive(band), takes, int(band.get("working_age", 0))]
    var warning := ""
    var tone := TONE_PLAIN
    if not falling.is_empty():
        warning = SUMMARY_HOLD_SHORT_FORMAT % [join_names(falling),
            SUMMARY_ARE_ONE if falling.size() == 1 else SUMMARY_ARE_MANY]
        tone = TONE_BAD
    elif not waiting.is_empty():
        warning = SUMMARY_REFILL_SHORT_FORMAT % [join_names(waiting),
            SUMMARY_WAIT_ONE if waiting.size() == 1 else SUMMARY_WAIT_MANY]
        tone = TONE_WARN
    return {"line": line, "warning": warning, "tone": tone}

static func tone_color(tone: StringName) -> Color:
    match tone:
        TONE_BAD:
            return HudStyle.DANGER
        TONE_WARN:
            return HudStyle.WARN
        TONE_OK:
            return HudStyle.HEALTHY
    return HudStyle.INK_DIM

# ---- Rows: what a labor row brings in --------------------------------------------------------------

## What a row brought in of an order's product last turn, per turn. `row` is a labor assignment dict.
static func product_of(row: Dictionary, kind: String) -> float:
    match kind:
        KIND_FOOD:
            if row.has("realized_yield"):
                return float(row["realized_yield"])
            return float(row.get("actual_yield", 0.0))
        KIND_FODDER:
            return float(row.get("fodder_yield", 0.0))
    for entry in row.get("material_yield", []):
        if entry is Dictionary and String((entry as Dictionary).get("material_id", "")) == kind:
            return float((entry as Dictionary).get("amount", 0.0))
    return 0.0

## The band's store of one material (`material_store` rows).
static func store_amount(band: Dictionary, material: String) -> float:
    var total := 0.0
    for entry in band.get("material_store", []):
        if entry is Dictionary and String((entry as Dictionary).get("material_id", "")) == material:
            total += float((entry as Dictionary).get("amount", 0.0))
    return total

## `Hide · 14 on hand` + `+0.4 a turn from hunting` rows for the by-products on hand or gained this
## turn, in the vocabulary's fixed order. Each `{material, icon, left, right}`.
static func by_product_rows(band: Dictionary) -> Array:
    var rows: Array = []
    for material in BY_PRODUCTS:
        var on_hand := store_amount(band, material)
        var gain := 0.0
        for row in band.get("labor_assignments", []):
            if row is Dictionary:
                gain += product_of(row as Dictionary, material)
        if on_hand <= 0.0 and gain <= 0.0:
            continue
        var right := ""
        if gain > 0.0:
            right = BY_PRODUCT_GAIN_FORMAT % [gain, String(BY_PRODUCT_SOURCE.get(material, ""))]
        rows.append({"material": material, "icon": String(BY_PRODUCT_ICONS.get(material, "")),
            "left": BY_PRODUCT_ON_HAND_FORMAT % [String(BY_PRODUCT_NAMES.get(material, material)),
                str(int(roundf(on_hand)))],
            "right": right})
    return rows
