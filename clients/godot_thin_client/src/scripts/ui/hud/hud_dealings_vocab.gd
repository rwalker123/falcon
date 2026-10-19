class_name HudDealingsVocab
extends RefCounted

## **THE DEALINGS VOCABULARY** (early diplomacy, issue #233, `docs/plan_early_diplomacy.md` §7) -- the
## band page's Trade tab (its "Waiting on an answer" and "History" sections), the Peoples tab's block of
## treaty proposals awaiting an answer, the answer sheet they open, the trade sheet's offer and propose modes, the
## scout sheet's pack, the faction page's gift toggle and the turn orb's "a people awaits your answer"
## row. `DealingsRoster` is the tab's one reader; `BandPanelController` the sheets'.
##
## A LEAF: it reads nothing, so it can sit in a `const` initializer anywhere without a load-order cycle
## (`.claude/rules/client/hud-modules.md`). The few `static func`s are pure text builders over the
## decoded `dealings` rows -- they decide nothing the sim owns (a turns-left, a standing link, a word
## cut are all published, never derived here).

# ---- THE WIRE'S TOKENS ------------------------------------------------------------------------------
## `DealingState.kind`. A treaty proposal is the one kind whose terms ride `treaty` rather than the two
## goods lists. A REPLY is the receiver's named return, posted back to the original sender: the sender
## takes it or walks home, and it cannot be countered.
const KIND_OFFER := "offer"
const KIND_REPLY := "reply"
const KIND_PROPOSAL := "proposal"
const KIND_COUNTER := "counter"
const KIND_TREATY_PROPOSAL := "treaty_proposal"
## `DealingState.carrier` on a treaty proposal.
const CARRIER_ROAD := "road"
const CARRIER_ENVOY := "envoy"

## `send_trade_expedition`'s trailing word and `send_expedition`'s `strangers` order.
const TRADE_MODE_GIFT := "gift"
const TRADE_MODE_OFFER := "offer"
## The sheet's third mode: not a shipment word on the wire (it is the `propose_trade` verb), only the
## sheet's own state.
const TRADE_MODE_PROPOSE := "propose"
const STRANGERS_KEEP := "keep"
const STRANGERS_GIVE := "give"
const STRANGERS_OFFER := "offer"

## The goods-list keys of a decoded `DealingGoodsState` (`dict/dealings.rs`), and its two accounts'
## command tokens.
const GOODS_FOOD := "food"
const GOODS_FODDER := "fodder"
const GOODS_MATERIALS := "materials"

## The three diplomacy verbs the `diplomacy_command_requested` signal carries (`Main.format_diplomacy_command`
## dispatches on them).
const COMMAND_ANSWER_DEALING := "answer_dealing"
const COMMAND_PROPOSE_TRADE := "propose_trade"
const COMMAND_ACCEPT_OWN_GIFTS := "set_accept_own_gifts"
## The treaty verbs the Peoples tab and the treaty sheet emit, and the border verb's per-people form.
const COMMAND_PROPOSE_TREATY := "propose_treaty"
const COMMAND_SEND_ENVOY := "send_envoy"
const COMMAND_END_TREATY := "end_treaty"
const COMMAND_SET_BORDERS := "set_open_borders"

## `answer_dealing`'s three answers.
const ANSWER_ACCEPT := "accept"
const ANSWER_REFUSE := "refuse"
const ANSWER_COUNTER := "counter"

# ---- THE WAITING SECTION -------------------------------------------------------------------------------
## The Trade tab's first section and the Peoples tab's block of treaty proposals share one head: both are
## the items that wait on an answer.
const HEAD := "Waiting on an answer"
const HEAD_COUNT_FORMAT := "%d"
const EMPTY := "Nothing waiting"
## Node names the harness finds the pieces by, and the metadata on a row (value: the dealing id).
const BLOCK_NAME := "DealingsRoster"
const ROWS_NAME := "DealingsRosterRows"
const TREATY_BLOCK_NAME := "TreatyProposals"
const ROW_META := &"dealings_row"
const ACTION_META := &"dealings_action"
## The row's gap between its lines (none: they are one row) and its separator glyph.
const ROW_LINE_SEPARATION := 0
const ROW_SEPARATOR := "·"
## The stripe of air between one dealing and the next, so a row's action strip never reads as the next
## row's first line.
const ROW_GAP := 6

## Who a row is about: the other people's name, then the band of theirs it concerns.
const WHO_FORMAT := "%s · %s"
## When no roster, tie or memory can name the other band. A raw id never reaches a label.
const WHO_UNKNOWN_BAND := "a band of theirs"
const INCOMING_GLYPH := "←"
const OUTGOING_GLYPH := "→"

## Turns left, singular and plural. **`1` means answer THIS turn or silence refuses it** -- the amber
## the incoming cell wears is that warning, set beside the number it explains.
const TURNS_LEFT_ONE := "1 turn left"
const TURNS_LEFT_MANY := "%d turns left"
const TURNS_LEFT_TOOLTIP := "Answer before the turn resolves: silence refuses it."

## Outgoing rows (the people we wrote to).
const AWAITING_FORMAT := "Awaiting their answer · %s"
const OUTGOING_TOOLTIP := "Waiting on them. Nothing to do until they answer."

# ---- WHAT A ROW SAYS ---------------------------------------------------------------------------------
const NOTHING := "nothing"
const OFFER_IN_FORMAT := "Offers %s · you name what comes back"
const OFFER_OUT_FORMAT := "You offered %s · they name what comes back"
## A REPLY, from the seat of the band that answers it (the band that named the return, the return, the
## cargo it is for) and of the band that named the return (the party's band, the return).
const REPLY_IN_FORMAT := "%s would give back %s for your %s"
const REPLY_OUT_FORMAT := "Waiting on %s's party to take your return of %s"
const SHIPPER_FORMAT := "You send %s · you get %s"
const CONSIGNEE_IN_FORMAT := "They send %s · they want %s from you"
const CONSIGNEE_OUT_FORMAT := "They send %s · you send back %s"
const TREATY_IN_FORMAT := "Proposes %s"
const TREATY_OUT_FORMAT := "You proposed %s"
const TREATY_ENVOY_TOOLTIP := "Brought by an envoy: the answer walks home with it."
const TREATY_ROAD_TOOLTIP := "Sent along the road between you."
const TERM_SEPARATOR := " · "
const GOODS_SEPARATOR := " · "
const READING_FORMAT := "%s: %s"
const READING_SEPARATOR := ", "
const MATERIAL_GRADE_FORMAT := "%s %s (%s)"
const MATERIAL_PLAIN_FORMAT := "%s %s"
const FOOD_WORD := "food"
const HAY_WORD := "hay"

## A treaty's three terms in words. Agreed trade reads `Agreed trade: 5.0 flint for 10.0 food`.
const TERM_ROAD_RIGHTS := "Road rights"
const TERM_OPEN_BORDERS := "Open borders"
const TERM_BUNDLE_FORMAT := "Agreed trade: %s for %s"
const TERMS_NONE := "no terms"

# ---- THE ACTIONS --------------------------------------------------------------------------------------
const ACTION_ACCEPT := "Accept"
const ACTION_REFUSE := "Refuse"
const ACTION_COUNTER := "Counter"
const ACCEPT_OFFER_TOOLTIP := "Take the cargo, and choose what you send back -- nothing is allowed, and reads as a gift."
const ACCEPT_TOOLTIP := "Agree to the terms as written."
const REFUSE_TOOLTIP := "Turn them down. A shipment goes home with its cargo."
const COUNTER_TOOLTIP := "Answer with different terms. It costs a turn."
## A reply is taken or walked away from: never countered, and no return to name.
const ACTION_TAKE_IT := "Take it"
const ACTION_WALK_HOME := "Reject it"
const TAKE_IT_TOOLTIP := "Take what they name: the exchange settles now."
const WALK_HOME_TOOLTIP := "Turn the return down. The party walks home with its cargo."

# ---- THE ANSWER SHEET ----------------------------------------------------------------------------------
## The sheet opens as a verb sheet of its own mission, off the action bar -- the station sheet's shape.
const COMPOSE_MISSION_DEALING := "dealing"
const SHEET_NAME := "Answer"
const SHEET_TITLE_OFFER_FORMAT := "Answer %s's offer"
const SHEET_TITLE_COUNTER_FORMAT := "Counter %s"
const SHEET_TITLE_TREATY_FORMAT := "Counter %s's treaty"
const SHEET_THEY_OFFER_FORMAT := "They offer %s"
const SHEET_RETURN_SECTION := "What you send back"
const SHEET_RETURN_EMPTY_HINT := "Send nothing back and this is a gift."
const SHEET_ACCEPT_BUTTON := "Accept"
const SHEET_ACCEPT_GIFT_BUTTON := "Accept as a gift"
const SHEET_ACCEPT_HINT := "The cargo lands here; what you chose leaves with the party."
const SHEET_COUNTER_BUTTON := "Send counter"
const SHEET_COUNTER_HINT := "The counter goes back to them and costs a turn."
const SHEET_LIMIT_FORMAT := "The party can carry %s home."
const SHEET_GONE_REASON := "That dealing has been answered or has lapsed."
const SHEET_YOUR_LEG_SECTION := "You send"
const SHEET_THEIR_LEG_SECTION := "They send"
const SHEET_YOU_GET_SECTION := "You get"
const SHEET_THEIR_LEG_HINT := "Their side is named, not drawn: they must hold it when they answer."
const SHEET_ADD_GOOD := "Add a good"
const SHEET_ADD_GOOD_TOOLTIP := "Name another good for that side of the bargain."
const SHEET_NOTHING_TO_ADD := "Nothing more to add."
const SHEET_LEG_EMPTY := "Nothing yet."
const SHEET_PACK_FULL_REASON := "The party cannot carry more than that."
const SHEET_COUNTER_NEEDS_TERMS := "Name something on at least one side."
const SHEET_TREATY_ROAD := "Road rights"
const SHEET_TREATY_BORDERS := "Open borders"
const SHEET_TREATY_BUNDLE_FORMAT := "Keep: %s"
const SHEET_TREATY_COUNTER_HINT := "Tick the terms you will agree to."
const SHEET_TREATY_NEEDS_TERMS := "Keep at least one term, or refuse."

# ---- THE TRADE SHEET'S TWO NEW MODES -------------------------------------------------------------------
const TRADE_MODE_SECTION := "How it is sent"
const TRADE_MODE_GIFT_LABEL := "Gift"
const TRADE_MODE_OFFER_LABEL := "Offer"
const TRADE_MODE_PROPOSE_LABEL := "Propose a trade"
const TRADE_MODE_GIFT_HINT := "It lands on arrival, and goodwill rises by what it was worth to them."
const TRADE_MODE_OFFER_HINT := "The party waits in their camp; they name what comes back, or refuse."
const TRADE_MODE_PROPOSE_HINT := "Name both sides before anyone walks. They answer this turn; nothing is drawn until they accept."
const TRADE_MODE_PROPOSE_NO_LINK := "Needs a kept road all the way to them."
const TRADE_SEND_GIFT_BUTTON := "Send shipment"
const TRADE_SEND_OFFER_BUTTON := "Offer shipment"
const TRADE_PROPOSE_BUTTON := "Propose trade"
const TRADE_PROPOSE_HINT := "Send the proposal along the road."
const TRADE_YOU_GIVE_SECTION := "You give"
const TRADE_YOU_ASK_SECTION := "You ask for"
const TRADE_ASK_EMPTY := "Name what you want back."
const TRADE_PROPOSE_NEEDS_TERMS := "Name what you give or what you ask for."
const TRADE_PROPOSE_NEEDS_DESTINATION := "Pick a band on the map to trade with."

# ---- THE SCOUT SHEET'S PACK ------------------------------------------------------------------------------
const SCOUT_PACK_SECTION := "Pack for strangers"
const SCOUT_STRANGERS_SECTION := "When it meets strangers"
const SCOUT_STRANGERS_KEEP_LABEL := "Keep"
const SCOUT_STRANGERS_GIVE_LABEL := "Give"
const SCOUT_STRANGERS_OFFER_LABEL := "Offer"
const SCOUT_STRANGERS_KEEP_HINT := "The pack stays on the party."
const SCOUT_STRANGERS_GIVE_HINT := "The first people it meets are handed the pack; they may name something back."
const SCOUT_STRANGERS_OFFER_HINT := "The first people it meets are offered the pack; they name what comes back."
const SCOUT_STRANGERS_NEEDS_PACK := "Load a pack first: Give and Offer need something to hand over."

# ---- THE FACTION PAGE'S GIFT TOGGLE ------------------------------------------------------------------------
const ACCEPT_GIFTS_LABEL := "Accept gifts from our own people"
const ACCEPT_GIFTS_HINT := "A shipment from one of your own bands lands as soon as it arrives."
const ACCEPT_GIFTS_COMMAND_MESSAGE_ON := "Gifts from your own people will land on arrival."
const ACCEPT_GIFTS_COMMAND_MESSAGE_OFF := "Gifts from your own people will wait for your answer."
## Node name and metas of the setting row, the actions key it emits through, and the room it takes.
const GIFT_ROW_NAME := "DealingsGiftSetting"
const GIFT_TOGGLE_META := &"dealings_gift_toggle"
const ACTION_ACCEPT_GIFTS := "accept_gifts"
const ACCEPT_GIFTS_TOKEN_ON := "on"
const ACCEPT_GIFTS_TOKEN_OFF := "off"

# ---- THE TURN ORB ------------------------------------------------------------------------------------------
const ATTENTION_KIND_DEALING := "dealing_answer"
const ATTENTION_LABEL := "A people awaits your answer"
const ATTENTION_LABEL_MANY_FORMAT := "%d peoples await your answer"
const ATTENTION_DETAIL_FORMAT := "%s · %s"
## One row per receiving band, capped like the awaiting-orders rows (a popover taller than the screen
## puts its own `Advance` footer off it).
const ATTENTION_MAX_ROWS := 3
const ATTENTION_OVERFLOW_LABEL_FORMAT := "%d more dealings await your answer"
const ATTENTION_OVERFLOW_DETAIL := "Open the Trade tab"

# ---- COMMAND RECEIPTS ---------------------------------------------------------------------------------------
const ANSWER_MESSAGE_ACCEPT := "Accepted the dealing."
const ANSWER_MESSAGE_ACCEPT_GIFT := "Accepted the dealing as a gift."
const ANSWER_MESSAGE_REFUSE := "Refused the dealing."
const ANSWER_MESSAGE_COUNTER := "Countered the dealing."
const ANSWER_MESSAGE_TAKE_RETURN := "Took the return."
const ANSWER_MESSAGE_WALK_HOME := "Walked home with the cargo."
const PROPOSE_MESSAGE_FORMAT := "Propose a trade to %s: %s for %s."

# ---- THE HISTORY SECTION ---------------------------------------------------------------------------------
## **ON THE ROAD** -- the Trade tab's section between `Waiting on an answer` and `This turn`: this band's
## trade parties that no dealing row stands for. A party standing in a camp on a pending dealing is shown
## once, by that dealing's row, which carries the party's Jump and Recall links instead.
const ROAD_HEAD := "On the road"
const ROAD_BLOCK_NAME := "TradeOnTheRoad"
const ROAD_ROWS_NAME := "TradeOnTheRoadRows"
## Row meta (value: the party's entity id) and the meta of the party links, on the road rows AND on the
## dealing rows that stand for a party (value: `PARTY_LINK_JUMP` / `PARTY_LINK_RECALL`).
const ROAD_ROW_META := &"trade_road_row"
const PARTY_LINK_META := &"dealings_party_link"
const PARTY_LINK_JUMP := "jump"
const PARTY_LINK_RECALL := "recall"
## The `actions` keys the Trade tab hands the roster for a party's two links: the Parties inspector's own
## Jump and Recall, reused rather than re-implemented.
const ACTION_PARTY_JUMP := "party_jump"
const ACTION_PARTY_RECALL := "party_recall"
const ACTION_PARTY_RECALL_VERB := "party_recall_verb"
const ACTION_PARTY_RECALL_TIP := "party_recall_tip"
## Phase words of a road row. (A party standing in a camp with no dealing behind it reads the shared
## `Waiting on an answer`, `HudExpeditionVocab.EXPEDITION_PHASE_WAITING_ON_ANSWER_LABEL`.)
const PHASE_HEADING_OUT := "Heading out"
const PHASE_COMING_HOME := "Coming home"
## Line 1 `→ Brackwater · Heading out` (the glyph is the row's direction glyph), line 2 `Carrying 6.0 hay`.
const ROAD_PHASE_FORMAT := "%s · %s"
const ROAD_CARRYING_FORMAT := "Carrying %s"
const ROAD_UNKNOWN_DESTINATION := "a band of theirs"
## The direction glyph of a party walking home (a heading-out or camped one wears `OUTGOING_GLYPH`).
const ROAD_HOME_GLYPH := "↩"

## The Trade tab's third section: one record per trade, from this people's point of view.
const HISTORY_HEAD := "History"
const HISTORY_EMPTY := "No trades yet"
const HISTORY_BLOCK_NAME := "TradeHistory"
const HISTORY_ROWS_NAME := "TradeHistoryRows"
const HISTORY_ROW_META := &"trade_history_row"
## Line 1: `Turn 12 · Ashfell · Brackwater · Traded` -- the turn, the counterparty, the outcome. The lead
## and the joiner are separate so the outcome word can wear its own ink; the whole line is the hover.
const HISTORY_LEAD_FORMAT := "Turn %d · %s"
const HISTORY_OUTCOME_JOINER := " · "
const HISTORY_LINE_FORMAT := "Turn %d · %s · %s"
## Line 2 where goods moved: `gave 5.0 food · got 2.0 hide`, each side omitted when it is empty.
const HISTORY_GAVE_FORMAT := "gave %s"
const HISTORY_GOT_FORMAT := "got %s"
const HISTORY_SIDES_SEPARATOR := " · "
## Line 2 where nothing moved: `offered 5.0 food for 2.0 hide`. A side that was empty is left out, and a
## record that named only what was wanted reads `asked for 2.0 hide`.
const HISTORY_OFFERED_FORMAT := "offered %s"
const HISTORY_OFFERED_FOR_FORMAT := "offered %s for %s"
const HISTORY_ASKED_FORMAT := "asked for %s"
## `TradeRecordState.outcome`'s tokens.
const OUTCOME_EXCHANGED := "exchanged"
const OUTCOME_GIFT_GIVEN := "gift_given"
const OUTCOME_GIFT_RECEIVED := "gift_received"
const OUTCOME_DELIVERED := "delivered"
const OUTCOME_DECLINED := "declined"
const OUTCOME_REFUSED := "refused"
const OUTCOME_EXPIRED := "expired"
const OUTCOME_WITHDRAWN := "withdrawn"
const OUTCOME_FAILED := "failed"
## The outcome in the tab's own words.
const OUTCOME_WORDS := {
	OUTCOME_EXCHANGED: "Traded",
	OUTCOME_GIFT_GIVEN: "Gift given",
	OUTCOME_GIFT_RECEIVED: "Gift received",
	OUTCOME_DELIVERED: "Delivered",
	OUTCOME_DECLINED: "Declined",
	OUTCOME_REFUSED: "Refused",
	OUTCOME_EXPIRED: "Lapsed",
	OUTCOME_WITHDRAWN: "Withdrawn",
	OUTCOME_FAILED: "Failed",
}
## The outcomes in which goods changed hands. Every other outcome is a deal that did not happen, whose
## goods lists state what was offered and what was asked.
const OUTCOMES_MOVED: Array[String] = [OUTCOME_EXCHANGED, OUTCOME_GIFT_GIVEN, OUTCOME_GIFT_RECEIVED,
	OUTCOME_DELIVERED]
## The outcomes worn in the DANGER ink (the deal was turned down or broke) and in the faint one (it ran
## out unanswered or was taken back); the rest wear HEALTHY, the ink Accept wears.
const OUTCOMES_DANGER: Array[String] = [OUTCOME_DECLINED, OUTCOME_REFUSED, OUTCOME_FAILED]
const OUTCOMES_FAINT: Array[String] = [OUTCOME_EXPIRED, OUTCOME_WITHDRAWN]

# ---- PURE TEXT BUILDERS ---------------------------------------------------------------------------------------

## `true` when a decoded goods dict names nothing in any account.
static func goods_are_empty(goods: Dictionary) -> bool:
	if float(goods.get(GOODS_FOOD, 0.0)) > 0.0 or float(goods.get(GOODS_FODDER, 0.0)) > 0.0:
		return false
	for batch_variant in Array(goods.get(GOODS_MATERIALS, [])):
		if batch_variant is Dictionary and float((batch_variant as Dictionary).get("amount", 0.0)) > 0.0:
			return false
	return true

## One account per term and **one term per material pile -- never summed**: food and hay are two
## larders that never convert, and two piles of one material at two ratings are two goods.
static func goods_terms(goods: Dictionary) -> PackedStringArray:
	var terms := PackedStringArray()
	var amount_format := "%.1f"
	var food := float(goods.get(GOODS_FOOD, 0.0))
	if food > 0.0:
		terms.append(MATERIAL_PLAIN_FORMAT % [amount_format % food, FOOD_WORD])
	var fodder := float(goods.get(GOODS_FODDER, 0.0))
	if fodder > 0.0:
		terms.append(MATERIAL_PLAIN_FORMAT % [amount_format % fodder, HAY_WORD])
	for batch_variant in Array(goods.get(GOODS_MATERIALS, [])):
		if not (batch_variant is Dictionary):
			continue
		var batch: Dictionary = batch_variant
		var amount := float(batch.get("amount", 0.0))
		if amount <= 0.0:
			continue
		var grade := reading_text(batch)
		var id := String(batch.get("material_id", ""))
		if grade == "":
			terms.append(MATERIAL_PLAIN_FORMAT % [amount_format % amount, id])
		else:
			terms.append(MATERIAL_GRADE_FORMAT % [amount_format % amount, id, grade])
	return terms

## A pile's readings in the material's declared axis order (`tough: good, supple: poor`), `""` for a
## pile with none (a bundle line is unrated: nothing has been drawn yet).
static func reading_text(batch: Dictionary) -> String:
	var parts := PackedStringArray()
	for reading_variant in Array(batch.get("readings", [])):
		if reading_variant is Dictionary:
			var reading: Dictionary = reading_variant
			parts.append(READING_FORMAT % [String(reading.get("axis", "")),
				String(reading.get("band_name", ""))])
	return READING_SEPARATOR.join(parts)

## A goods list as one sentence fragment, `NOTHING` when it names nothing.
static func goods_text(goods: Dictionary) -> String:
	var terms := goods_terms(goods)
	if terms.is_empty():
		return NOTHING
	return GOODS_SEPARATOR.join(terms)

## A leg (`key -> amount`) as words, `NOTHING` when empty: `5.0 food · 2.0 hide`.
static func leg_text(leg: Dictionary) -> String:
	var keys: Array = leg.keys()
	keys.sort()
	var parts := PackedStringArray()
	for key_variant in keys:
		var key := String(key_variant)
		var amount := float(leg[key])
		if amount > 0.0:
			parts.append(MATERIAL_PLAIN_FORMAT % [LEG_AMOUNT_FORMAT % amount, leg_label(key).to_lower()])
	if parts.is_empty():
		return NOTHING
	return GOODS_SEPARATOR.join(parts)

## A treaty's terms in words (`Road rights · Open borders · Agreed trade: 5.0 flint for 10.0 food`). A
## bundle composed on a sheet carries its own words (`give_text` / `ask_text`); a published one its goods.
static func terms_text(terms: Dictionary) -> String:
	var parts := PackedStringArray()
	if bool(terms.get("road_rights", false)):
		parts.append(TERM_ROAD_RIGHTS)
	if bool(terms.get("open_borders", false)):
		parts.append(TERM_OPEN_BORDERS)
	for bundle_variant in Array(terms.get("bundles", [])):
		if bundle_variant is Dictionary:
			var bundle: Dictionary = bundle_variant
			var carried := String(bundle["give_text"]) if bundle.has("give_text") \
				else goods_text(Dictionary(bundle.get("sent", {})))
			var paid := String(bundle["ask_text"]) if bundle.has("ask_text") \
				else goods_text(Dictionary(bundle.get("returned", {})))
			parts.append(TERM_BUNDLE_FORMAT % [carried, paid])
	if parts.is_empty():
		return TERMS_NONE
	return TERM_SEPARATOR.join(parts)

## `1 turn left` / `3 turns left`.
static func turns_left_text(turns: int) -> String:
	return TURNS_LEFT_ONE if turns == 1 else TURNS_LEFT_MANY % turns

## Metadata the harness finds the sheet's choice buttons and leg rows by (value: the choice id / leg key).
const CHOICE_META := &"dealings_choice"
const LEG_ROW_META := &"dealings_leg_row"
const LEG_STEP_META := &"dealings_leg_step"
const LEG_ADD_META := &"dealings_leg_add"
const SHEET_ACTION_META := &"dealings_sheet_action"

# ---- A LEG: the side of a bargain this band does not hold ---------------------------------------------
## A leg is `{key -> amount}` with one key per account: `food`, `fodder`, or `material:<id>`. It is
## the editor's state for a list the other people holds -- named, not drawn -- and it round-trips to
## the picker's row shape and back to a command line.
const LEG_KEY_FOOD := "food"
const LEG_KEY_FODDER := "fodder"
const LEG_KEY_MATERIAL_PREFIX := "material:"
const LEG_STEP := 1.0
const LEG_AMOUNT_FORMAT := "%.1f"
const LEG_LABEL_FOOD := "Food"
const LEG_LABEL_HAY := "Hay"
const LEG_REMOVE_TOOLTIP := "Take this good off the list."
const LEG_LESS_TOOLTIP := "One less."
const LEG_MORE_TOOLTIP := "One more."

## A goods dict as a leg (`{key -> amount}`), the editor's seed for the side a counter prefills.
static func leg_from_goods(goods: Dictionary) -> Dictionary:
	var leg := {}
	var food := float(goods.get(GOODS_FOOD, 0.0))
	if food > 0.0:
		leg[LEG_KEY_FOOD] = food
	var fodder := float(goods.get(GOODS_FODDER, 0.0))
	if fodder > 0.0:
		leg[LEG_KEY_FODDER] = fodder
	for batch_variant in Array(goods.get(GOODS_MATERIALS, [])):
		if batch_variant is Dictionary:
			var batch: Dictionary = batch_variant
			var amount := float(batch.get("amount", 0.0))
			if amount > 0.0:
				var key := LEG_KEY_MATERIAL_PREFIX + String(batch.get("material_id", ""))
				leg[key] = float(leg.get(key, 0.0)) + amount
	return leg

## The picker's row shape for a leg, food and hay first then each material by id: what
## `Main.cargo_manifest` spells onto a command line.
static func leg_rows(leg: Dictionary) -> Array:
	var rows: Array = []
	var keys: Array = leg.keys()
	keys.sort()
	for key_variant in keys:
		var key := String(key_variant)
		var amount := float(leg[key])
		if amount <= 0.0:
			continue
		if key == LEG_KEY_FOOD:
			rows.append({"id": HudConst.STORE_ITEM_PROVISIONS, "is_material": false, "amount": amount})
		elif key == LEG_KEY_FODDER:
			rows.append({"id": HudConst.CARGO_ITEM_FODDER, "is_material": false, "amount": amount})
		elif key.begins_with(LEG_KEY_MATERIAL_PREFIX):
			rows.append({"id": key.trim_prefix(LEG_KEY_MATERIAL_PREFIX), "is_material": true,
				"amount": amount})
	return rows

static func leg_is_empty(leg: Dictionary) -> bool:
	for key in leg:
		if float(leg[key]) > 0.0:
			return false
	return true

## What a leg's key is called on screen: the account's word, or the material's generic id.
static func leg_label(key: String) -> String:
	if key == LEG_KEY_FOOD:
		return LEG_LABEL_FOOD
	if key == LEG_KEY_FODDER:
		return LEG_LABEL_HAY
	return key.trim_prefix(LEG_KEY_MATERIAL_PREFIX)

## A decoded goods dict as the picker's own row shape (`{id, is_material, amount}`), which is what
## `Main.cargo_manifest` spells onto a command line: food and hay by their store keys, each material by
## its generic id. A bundle or a proposal's leg carries no ratings, so a material row is one amount.
static func goods_rows(goods: Dictionary) -> Array:
	var rows: Array = []
	var food := float(goods.get(GOODS_FOOD, 0.0))
	if food > 0.0:
		rows.append({"id": HudConst.STORE_ITEM_PROVISIONS, "is_material": false, "amount": food})
	var fodder := float(goods.get(GOODS_FODDER, 0.0))
	if fodder > 0.0:
		rows.append({"id": HudConst.CARGO_ITEM_FODDER, "is_material": false, "amount": fodder})
	for batch_variant in Array(goods.get(GOODS_MATERIALS, [])):
		if batch_variant is Dictionary and float((batch_variant as Dictionary).get("amount", 0.0)) > 0.0:
			var batch: Dictionary = batch_variant
			rows.append({"id": String(batch.get("material_id", "")), "is_material": true,
				"amount": float(batch.get("amount", 0.0))})
	return rows


# ---- THE PEOPLES TAB'S DIPLOMACY BLOCK (C2) -------------------------------------------------------------
## Each other people gets a block: line 1 its name with what THEY think of us and what WE think of them,
## line 2 the treaty in force, then an action strip. The words come from the published cuts; the numbers
## stay on the hover.
const PEOPLES_SECTION := "Treaties and regard"
const BANDS_SECTION := "Bands we know"
const PEOPLES_BLOCK_NAME := "PeoplesDiplomacy"
const PEOPLE_ROW_META := &"peoples_diplomacy_row"
const PEOPLE_ACTION_META := &"peoples_diplomacy_action"
const PEOPLE_GOODWILL_META := &"peoples_goodwill_cell"
const PEOPLE_ACTION_PROPOSE := "propose"
const PEOPLE_ACTION_END := "end_treaty"
const PEOPLE_ACTION_BORDERS := "borders"
## The gap between one people's block and the next.
const PEOPLE_ROW_GAP := 6

const WORD_WARY := "wary"
const WORD_NEUTRAL := "neutral"
const WORD_FRIENDLY := "friendly"
const THEY_FORMAT := "They: %s"
const WE_FORMAT := "We: %s"
const THEY_TOOLTIP_FORMAT := "They think of us as %s, as of turn %d."
const WE_TOOLTIP_FORMAT := "We think of them as %s."
const REASONS_HEAD := "Because of:"
const REASONS_NONE := "Nothing has moved it yet."
const REASON_FORMAT := "%s %s"
const REASON_AMOUNT_FORMAT := "%+.1f"
## The published goodwill components, in the order the hover lists them, and what each is called.
const REASON_COMPONENTS := [
	["gifts", "gifts"],
	["exchanges", "trade"],
	["stationed", "people stationed there"],
	["link", "a road between us"],
	["treaty", "our treaty"],
	["breach", "a broken treaty"],
]
## A component this small is rounding, not a reason.
const REASON_EPSILON := 0.05

const TREATY_NONE := "No treaty"
const TREATY_SINCE_FORMAT := "In force since turn %d"
const TREATY_PROPOSE_LABEL := "Propose a treaty"
const TREATY_PROPOSE_TOOLTIP := "Name the terms: road rights, open borders, agreed trade."
const TREATY_END_LABEL := "End treaty"
const TREATY_END_TOOLTIP := "End it at once. It is a breach, and costs their goodwill."
const TREATY_END_CONFIRM_FORMAT := "End the treaty with %s? It is a breach, and costs their goodwill."
const TREATY_END_CONFIRM_OK := "End treaty"
const TREATY_END_MESSAGE_FORMAT := "Ended the treaty with %s."
const BORDERS_OPEN_LABEL := "Borders open"
const BORDERS_CLOSED_LABEL := "Borders closed"
const BORDERS_DEFAULT_TOOLTIP := "Following your general setting. Press to set it for this people alone."
const BORDERS_OVERRIDE_TOOLTIP := "Set for this people alone. Press to flip it."
const BORDERS_TREATY_TOOLTIP := "Your treaty holds them open."
const BORDERS_BREACH_CONFIRM_FORMAT := "Closing your borders to %s ends your treaty with them. It is a breach, and costs their goodwill."
const BORDERS_BREACH_CONFIRM_OK := "Close borders"
const BORDERS_MESSAGE_OPEN_FORMAT := "Borders open to %s."
const BORDERS_MESSAGE_CLOSED_FORMAT := "Borders closed to %s."

# ---- THE TREATY SHEET ---------------------------------------------------------------------------------------
const COMPOSE_MISSION_TREATY := "treaty"
const TREATY_SHEET_NAME := "Treaty"
const TREATY_SHEET_TITLE_FORMAT := "Propose a treaty to %s"
const TREATY_VIA_ROAD_FORMAT := "Over the road to %s: it reaches them this turn."
const TREATY_VIA_ENVOY_FORMAT := "No road to them: an envoy walks the proposal to %s, and the answer walks home."
const TREATY_VIA_ENVOY_NOBODY := "No road to them, and no band of theirs to walk to."
const TREATY_TERMS_SECTION := "Terms"
const TREATY_BUNDLES_SECTION := "Agreed trade"
const TREATY_BUNDLES_HINT := "Each shipment between you is paid by the bundle on arrival."
const TREATY_BUNDLE_HEAD_FORMAT := "Agreed trade %d"
const TREATY_BUNDLE_SENT_SECTION := "A shipment carries"
const TREATY_BUNDLE_PAID_SECTION := "The receiver pays back"
const TREATY_BUNDLE_NEEDS_SENT := "Each agreed trade needs something carried."
const TREATY_ADD_BUNDLE := "Add agreed trade"
const TREATY_ADD_BUNDLE_TOOLTIP := "A standing exchange: what one side sends, what the other pays."
const TREATY_REMOVE_BUNDLE_TOOLTIP := "Take this agreed trade off the proposal."
const TREATY_PROPOSE_BUTTON := "Propose treaty"
const TREATY_PROPOSE_HINT := "Send the proposal along the road."
const TREATY_ENVOY_BUTTON := "Send envoy"
const TREATY_ENVOY_HINT := "Send a party with the proposal. You hear the answer when it walks home."
const TREATY_NEEDS_TERMS := "Tick a term or add agreed trade."
const TREATY_ENVOY_TARGET_SECTION := "Send to"
const TREATY_ENVOY_NO_TARGET := "No band of theirs you still have a tie with. Meet one first."
const TREATY_ENVOY_NO_WORKERS := "No free workers to send."
const TREATY_ENVOY_PEOPLE_LABEL := "Envoy"
const TREATY_ENVOY_PEOPLE_AT_MAX_REASON := "That is every free worker."
const TREATY_SENT_MESSAGE_FORMAT := "Proposed a treaty to %s: %s."
const TREATY_ENVOY_MESSAGE_FORMAT := "Sent an envoy to %s with a treaty proposal: %s."
const TREATY_SHEET_ADD_HINT := "Add agreed trade of your own to the counter."
const TREATY_CHOICE_META := &"treaty_target_choice"
const TREATY_BUNDLE_META := &"treaty_bundle"
const TREATY_BUNDLE_REMOVE_META := &"treaty_bundle_remove"
const TREATY_ADD_BUNDLE_META := &"treaty_add_bundle"
const TREATY_SEND_META := &"treaty_send"

# ---- AN ENVOY PARTY'S ROW ----------------------------------------------------------------------------------
const ENVOY_TERMS_FORMAT := "Carrying: %s"
const ENVOY_ROW_FORMAT := "%s · %s"

## The word for a published goodwill total against the published cuts: below `wary_below` wary, above
## `friendly_above` friendly, between neutral. `""` when the sim has stated no cuts -- never a default.
static func goodwill_word(total: float, cuts: Dictionary) -> String:
	if not cuts.has("wary_below") or not cuts.has("friendly_above"):
		return ""
	if total < float(cuts["wary_below"]):
		return WORD_WARY
	if total > float(cuts["friendly_above"]):
		return WORD_FRIENDLY
	return WORD_NEUTRAL

## The non-zero components of one goodwill row in words, signed, strongest first: `+1.2 gifts`.
static func reason_lines(row: Dictionary) -> PackedStringArray:
	var ranked: Array = []
	for component in REASON_COMPONENTS:
		var amount := float(row.get(String(component[0]), 0.0))
		if absf(amount) >= REASON_EPSILON:
			ranked.append([amount, String(component[1])])
	ranked.sort_custom(func(a: Array, b: Array) -> bool: return absf(float(a[0])) > absf(float(b[0])))
	var lines := PackedStringArray()
	for entry in ranked:
		lines.append(REASON_FORMAT % [REASON_AMOUNT_FORMAT % float(entry[0]), String(entry[1])])
	return lines
