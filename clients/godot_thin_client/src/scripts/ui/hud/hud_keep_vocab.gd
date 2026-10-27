class_name HudKeepVocab

## KEEP-ON-HAND vocabulary (#874) — a MINED material (stone) is not crewed by hand. The band holds a
## target stock of it, the sim picks the one site and staffs the dig to the target, and the player's whole
## say is the target and a priority. This file is the readers for the band's `keep_on_hand` entries and
## the words the Work tab's `Dig stone` row and the Stone sheet speak (`docs/prototypes/labor_at_scale/
## Stone.dc.html`, `Main.dc.html` → the `stock` job).
##
## ⛔ **WHICH MATERIALS ARE MINED IS THE WIRE'S ANSWER, NEVER A LIST HERE.** The sim publishes a
## `keep_on_hand` entry for every mined material the faction has found rock of (or holds a target for),
## so membership in the BAND's entries is the one test (`is_mined`) — "stone" is spelled nowhere in the
## client's logic, and a new mined material needs no edit.
##
## ⛔ **ON HAND IS NOT ON THE WIRE.** It is the band's `material_store` total of the material
## (`on_hand_of`), the same shelf every other surface reads.
##
## ⛔ **THE KEEP CHIP IS A LABEL.** The target changes only from the sheet's `−` / `+` stepper, in steps of
## `KEEP_STEP`, and `0` reads `None`.

## One press of the sheet's `−` / `+` moves the target by this many units. One, like every stepper in the game:
## a step of 10 left a band that wants a few stone nothing between None and 10.
const KEEP_STEP := 1
## The target's face at 0 (no order: nobody digs).
const KEEP_NONE_WORD := "None"
const KEEP_STEPPER_LOWER_ARIA := "Keep less %s"
const KEEP_STEPPER_RAISE_ARIA := "Keep more %s"
## The stepper's value column, wide enough for `None` and a four-digit target at the sheet's font.
const KEEP_STEPPER_VALUE_WIDTH := 56.0

## The band-dict key the native decoder writes the entries under.
const BAND_KEY := "keep_on_hand"
const ENTRY_MATERIAL_KEY := "material"
const ENTRY_TARGET_KEY := "target"
const ENTRY_PRIORITY_KEY := "priority"
const ENTRY_DRAW_KEY := "draw"
const ENTRY_HAS_SITE_KEY := "has_site"
const ENTRY_SITE_X_KEY := "site_x"
const ENTRY_SITE_Y_KEY := "site_y"

## ---- THE WORK TAB ROW ---------------------------------------------------------------------------
const DIG_ROW_NAME_FORMAT := "Dig %s"
const DIG_ROW_SITE_FORMAT := "%s (%d, %d) — the game picks the site"
const DIG_ROW_NO_SITE := "No rock known yet"
## The row's chip names the order alone: on hand is the sheet's, and the status line already says
## whether the pile is short (`Refilling to N`) or full (`At target`).
const KEEP_CHIP_FORMAT := "Keep %d"
const KEEP_CHIP_NONE := "Keep: none"
const KEEP_CHIP_TOOLTIP := "Set on the %s sheet (click the row)"
const STATUS_AT_TARGET := "At target — nobody digs"
const STATUS_NOBODY_FREE := "Nobody free to dig"
const STATUS_REFILLING_FORMAT := "Refilling to %d"
const STATUS_NO_ROCK := "No rock to dig"
const KEEP_CHIP_META := &"keep_chip"
const KEEP_STATUS_META := &"keep_status"
const DIG_ROW_META := &"dig_row"

## ---- THE STONE SHEET ----------------------------------------------------------------------------
const SHEET_SECTION_KEEP := "Keep on hand"
const SHEET_SECTION_DIGGERS := "Diggers"
const SHEET_SECTION_NEXT_TURN := "Next turn"
const SHEET_ON_HAND_FORMAT := "%s on hand"
## A pile short of a whole stone is stated to this many places, so a first dig of 0.4 never reads `0`.
const SHEET_ON_HAND_DECIMALS := 1
const SHEET_KEEP_KEY := "keep"
const SHEET_USES_FORMAT := "%s uses %s %s a turn — its builds and upkeep."
const SHEET_USES_NOTHING_FORMAT := "%s uses no %s right now."
const SHEET_CREW_FORMAT := "%s sends %d diggers this turn"
const SHEET_CREW_ONE_FORMAT := "%s sends 1 digger this turn"
const SHEET_CREW_AT_TARGET := "At target — nobody digs this turn"
const SHEET_CREW_NOBODY := "Nobody free to dig this turn"
const SHEET_SITE_HERE_LEAD := "Here, the best rock in reach."
const SHEET_SITE_HOLD_FORMAT := " %d %s up with what the band uses,"
const SHEET_SITE_REFILL_FORMAT := " %d %s toward %d."
const SHEET_VERB_KEEP_ONE := "keeps"
const SHEET_VERB_KEEP_MANY := "keep"
const SHEET_VERB_REFILL_ONE := "refills"
const SHEET_VERB_REFILL_MANY := "refill"
const SHEET_SITE_ELSEWHERE_FORMAT := "The game is digging at (%d, %d)"
const SHEET_SITE_AT_TARGET_FORMAT := "Diggers go back to work the turn %s drops below %d."
const SHEET_SITE_NO_TARGET := "Raise the target and diggers go to work."
const SHEET_SPLIT_KITTED_FORMAT := "%d with %s"
const SHEET_SPLIT_BARE_FORMAT := "%d bare-handed"
const SHEET_SPLIT_KEEPERS_FORMAT := "%d keep it up"
const SHEET_SPLIT_SEPARATOR := " · "
const SHEET_NEXT_TURN_UNIT_FORMAT := "%s"
const SHEET_TAKE_FORMAT := "%.2f"
const SHEET_BACK_TO_FORMAT := "Back to %d in ≈%d turns."
const SHEET_BACK_TO_ONE_FORMAT := "Back to %d in ≈1 turn."
const SHEET_NOT_GAINING := "The take only keeps up with what the band uses."
const SHEET_NOTHING_DUG := "Nothing dug while the pile is full."
const SHEET_STOCK_FORMAT := "%d of %d %s left in this rock."
const SHEET_NEVER_RENEWS := " %s does not come back."
const SHEET_NO_ROCK := "No rock known yet — nothing to dig."
## The one line a site-less sheet states in place of the Diggers box, and the flag its stand-in deposit wears.
const SHEET_NO_SITE_LINE := "No rock known yet — diggers go out once the band finds some."
const KEEP_ONLY_KEY := "keep_only"
const SHEET_BOX_META := &"keep_sheet_box"
const SHEET_ON_HAND_META := &"keep_sheet_on_hand"
const SHEET_TARGET_META := &"keep_sheet_target"
const SHEET_CREW_META := &"keep_sheet_crew"
const SHEET_SITE_META := &"keep_sheet_site"
const SHEET_SPLIT_META := &"keep_sheet_split"
const SHEET_TAKE_META := &"keep_sheet_take"
const SHEET_ETA_META := &"keep_sheet_eta"
const SHEET_USES_META := &"keep_sheet_uses"
const SHEET_STOCK_META := &"keep_sheet_stock"

## How a status reads — the three states of the `stock` job.
enum Status { AT_TARGET, NOBODY_FREE, REFILLING, NO_ROCK }


## ---- READERS ------------------------------------------------------------------------------------
static func entries_of(band: Dictionary) -> Array[Dictionary]:
	var entries: Array[Dictionary] = []
	var raw: Variant = band.get(BAND_KEY, [])
	if not (raw is Array):
		return entries
	for entry in raw:
		if entry is Dictionary:
			entries.append(entry)
	return entries

## The band's entry for one material, `{}` when it has none (the material is not mined, or the faction
## has found no rock of it).
static func entry_of(band: Dictionary, material: String) -> Dictionary:
	for entry in entries_of(band):
		if String(entry.get(ENTRY_MATERIAL_KEY, "")) == material:
			return entry
	return {}

## Is `material` one the band keeps on hand instead of crewing? The one test every surface asks.
static func is_mined(band: Dictionary, material: String) -> bool:
	return material != "" and not entry_of(band, material).is_empty()

static func target_of(entry: Dictionary) -> int:
	return int(entry.get(ENTRY_TARGET_KEY, 0))

static func draw_of(entry: Dictionary) -> float:
	return float(entry.get(ENTRY_DRAW_KEY, 0.0))

static func site_of(entry: Dictionary) -> Vector2i:
	if not bool(entry.get(ENTRY_HAS_SITE_KEY, false)):
		return Vector2i(-1, -1)
	return Vector2i(int(entry.get(ENTRY_SITE_X_KEY, -1)), int(entry.get(ENTRY_SITE_Y_KEY, -1)))

## The band's store total of one material — the `material_store` shelf, never a sum across goods.
static func on_hand_of(band: Dictionary, material: String) -> float:
	var held := 0.0
	for row in SourceForecast.material_payoff_rows(band.get(DetailFormat.BAND_MATERIAL_STORE_KEY, [])):
		if String(row.get(SourceForecast.MATERIAL_PAYOFF_ID_KEY, "")) == material:
			held += float(row.get(SourceForecast.MATERIAL_PAYOFF_AMOUNT_KEY, 0.0))
	return held

## On hand as the player reads it: a whole pile as a whole number, a part-stone to `SHEET_ON_HAND_DECIMALS`
## places — FLOORED either way, so the face never claims the target is met a hair early.
static func on_hand_face(on_hand: float) -> String:
	var whole := floorf(on_hand)
	if on_hand - whole <= 0.0:
		return str(int(whole))
	var scale := pow(10.0, SHEET_ON_HAND_DECIMALS)
	return String.num(floorf(on_hand * scale) / scale, SHEET_ON_HAND_DECIMALS)

## The sim's own test: at or above the target, which includes target 0, nobody digs.
static func at_target(target: int, on_hand: float) -> bool:
	return float(target) <= on_hand

## A material as a person reads it, lower-cased for mid-sentence use (`stone`).
static func word(material: String) -> String:
	return HudLoadoutVocab.material_label(material).to_lower()

static func target_face(target: int) -> String:
	return str(target) if target > 0 else KEEP_NONE_WORD

static func status_of(entry: Dictionary, on_hand: float, crew: int) -> Status:
	var target := target_of(entry)
	if at_target(target, on_hand):
		return Status.AT_TARGET
	if not bool(entry.get(ENTRY_HAS_SITE_KEY, false)):
		return Status.NO_ROCK
	if crew <= 0:
		return Status.NOBODY_FREE
	return Status.REFILLING

static func status_text(status: Status, target: int) -> String:
	match status:
		Status.AT_TARGET:
			return STATUS_AT_TARGET
		Status.NOBODY_FREE:
			return STATUS_NOBODY_FREE
		Status.NO_ROCK:
			return STATUS_NO_ROCK
		_:
			return STATUS_REFILLING_FORMAT % target

static func chip_text(target: int) -> String:
	if target <= 0:
		return KEEP_CHIP_NONE
	return KEEP_CHIP_FORMAT % target

## The turns until the pile is back at the target at the rate the crew nets over the band's own draw;
## `-1` where the take does not out-run the draw (there is no honest answer).
static func turns_to_target(target: int, on_hand: float, take: float, draw: float) -> int:
	var net := take - draw
	if net <= 0.0:
		return -1
	return int(ceil((float(target) - on_hand) / net))

## The sheet's split line: the keepers, the hands with the kit, the bare hands — a zero clause is dropped.
static func split_line(keepers: int, kitted: int, bare: int, kit_names: String) -> String:
	var parts: Array[String] = []
	if keepers > 0:
		parts.append(SHEET_SPLIT_KEEPERS_FORMAT % keepers)
	if kitted > 0:
		parts.append(SHEET_SPLIT_KITTED_FORMAT % [kitted,
			kit_names if kit_names != "" else HudWorkVocab.CREW_BAR_GEAR_FALLBACK])
	if bare > 0:
		parts.append(SHEET_SPLIT_BARE_FORMAT % bare)
	return SHEET_SPLIT_SEPARATOR.join(PackedStringArray(parts))

## **THE SITE LINE ON THE PICKED SITE**, the hold clause dropped when nobody is needed to hold it (a band that
## uses none of the material): `Here, the best rock in reach. 1 refills toward 10.` The verb agrees with the
## count (`1 keeps` / `2 keep`, `1 refills` / `2 refill`).
static func site_here_line(hold: int, refill: int, target: int) -> String:
	var line := SHEET_SITE_HERE_LEAD
	if hold > 0:
		line += SHEET_SITE_HOLD_FORMAT % [hold, SHEET_VERB_KEEP_ONE if hold == 1 else SHEET_VERB_KEEP_MANY]
		# the hold clause ends on a comma and the refill clause follows it
	line += SHEET_SITE_REFILL_FORMAT % [refill, SHEET_VERB_REFILL_ONE if refill == 1 else SHEET_VERB_REFILL_MANY,
		target]
	return line

static func stock_line(deposit: Dictionary, material: String, renews: bool) -> String:
	var line := SHEET_STOCK_FORMAT % [int(round(HudDepositVocab.stock_of(deposit))),
		int(round(float(deposit.get("capacity", 0.0)))), word(material)]
	if not renews:
		line += SHEET_NEVER_RENEWS % HudLoadoutVocab.material_label(material)
	return line
