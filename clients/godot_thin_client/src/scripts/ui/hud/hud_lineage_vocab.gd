class_name HudLineageVocab

## THE BREEDING CEILING, AS THE CLIENT STATES IT (issue #691; sim half: `.claude/rules/core_sim/campaign.md`
## → "The breeding ceiling"). An isolated people cannot grow past its founding lines, and the Band tab
## answers three questions about that: how close am I (the `Family limit` row), who is holding the limit
## up (its popover), and what would a split do to it (the split sheet's `FAMILY LIMIT` block).
##
## **EVERY NUMBER HERE IS THE SIM'S.** The ceiling, the head-count, the per-member lines and the two
## world constants (`LineageWorld`) are published; this module only spells them and does the one piece
## of arithmetic a split preview needs (the lines each half would hold), from the same constants the
## sim prices a split with.
##
## **`breeding_ceiling == 0` MEANS "NO LIMIT", NEVER "A LIMIT OF NOBODY"** — the people is free for good,
## and the whole surface (row, bullets, split block) stays away. **A ZERO RESERVE IS THE NOT-PROJECTED
## SENTINEL** (`BandFoodStatus.fertility_is_projected`), so a band with no reading shows no row either:
## no data is not a ceiling.

# ---- the band's published keys (the native decoder's names) ---------------------------------------
const FOUNDING_LINES_KEY := "founding_lines"
const BREEDING_POPULATION_KEY := "breeding_population"
const BREEDING_CEILING_KEY := "breeding_ceiling"
const BREEDING_MEMBERS_KEY := "breeding_members"
const BREEDING_PEOPLES_KEY := "breeding_peoples"
const FERTILITY_CEILING_KEY := "fertility_ceiling"
const MEMBER_BAND_ID_KEY := "band_id"
const MEMBER_LINES_KEY := "lines"
const MEMBER_PEOPLE_KEY := "people"
const MEMBER_FADING_KEY := "fading"
const PEOPLE_FACTION_KEY := "faction"

## `breeding_ceiling` publishes 0 once the people is free for good.
const CEILING_LIFTED := 0
## A ceiling factor of exactly this is "room to spare"; below it the limit is biting.
const FACTOR_UNBOUND := 1.0

# ---- the Family limit row and its popover ---------------------------------------------------------
const VALUE_FORMAT := "%d / %d"
const MEMBER_VALUE_FORMAT := "%d families · %d"
const PEOPLE_VALUE_FORMAT := "+%d families"
const FADING_SUFFIX := " · fading"
const CONTACT_NOTE := "Contact with other factions raises the limit."

# ---- the bullet under the row ---------------------------------------------------------------------
## The amber bullet's lead mark — the work board's `◆`, so a warning reads the same on every tab.
const BULLET_MARK := "◆"
const BULLET_OVER_LIMIT := "Over the limit. Find another faction to grow again."
const BULLET_KIN := "Too closely related. Meet another faction to grow."
const BULLET_ROOM_FORMAT := "Room for %d more. Meet another faction to grow."
## The DIM line (not amber) a people one step from freedom is shown.
const STAY_IN_TOUCH_FORMAT := "Stay in touch until %d and the limit is gone for good."

# ---- the line sentinels `DetailFormat.detail_bbcode` recognises -----------------------------------
## The popover payload is a list of STRINGS rendered by one formatter, so the three line shapes a plain
## `Key: value` cannot carry ride behind a lead control character (never typed by a player, never in a
## label). A TABLE ROW takes the four fields below joined by `FIELD_SEPARATOR`.
const ROW_MARK := "\u001e"
const FAINT_MARK := "\u001d"
const FIELD_SEPARATOR := "\u001f"
## The gap a popover row keeps between its name and its figure (px, `[cell padding=l,t,r,b]`): a
## `[table]` cell has none of its own, so a long name otherwise butts against its value.
const ROW_NAME_GAP_PX := 14

## True for a line `DetailFormat.detail_bbcode` renders through this module.
static func is_lineage_line(line: String) -> bool:
	return line.begins_with(ROW_MARK) or line.begins_with(FAINT_MARK) \
		or line.begins_with(BULLET_MARK + " ")

## A two-column popover row: `left` in `left_hex`, `right` in `right_hex`.
static func table_row(left: String, left_hex: String, right: String, right_hex: String) -> String:
	return ROW_MARK + FIELD_SEPARATOR.join([left, left_hex, right, right_hex])

## A last-line note in the faintest ink.
static func faint_line(text: String) -> String:
	return FAINT_MARK + text

## The amber bullet line.
static func bullet_line(text: String) -> String:
	return "%s %s" % [BULLET_MARK, text]

## The BBCode for one lineage line; `table_open` says whether a `[table=2]` is already open so
## consecutive rows share one (their columns align). Returns `{bbcode, table_open}`.
static func line_bbcode(line: String, table_open: bool) -> Dictionary:
	if line.begins_with(ROW_MARK):
		var fields := line.substr(ROW_MARK.length()).split(FIELD_SEPARATOR)
		var out := "" if table_open else "[table=2]"
		out += "[cell padding=0,0,%d,0][color=#%s]%s[/color][/cell][cell][color=#%s]%s[/color][/cell]" % [
			ROW_NAME_GAP_PX, fields[1], fields[0], fields[3], fields[2]]
		return {"bbcode": out, "table_open": true}
	var closing := "[/table]\n" if table_open else ""
	if line.begins_with(FAINT_MARK):
		return {"bbcode": "%s[color=#%s]%s[/color]\n" % [
			closing, HudStyle.INK_FAINT.to_html(false), line.substr(FAINT_MARK.length())],
			"table_open": false}
	return {"bbcode": "%s[color=#%s]%s[/color]\n" % [closing, HudStyle.WARN_HEX, line],
		"table_open": false}

# ---- readings --------------------------------------------------------------------------------------

## The fourth fertility factor, neutral where the band publishes none.
static func ceiling_factor(band: Dictionary) -> float:
	return float(band.get(FERTILITY_CEILING_KEY, FACTOR_UNBOUND))

static func ceiling_of(band: Dictionary) -> int:
	return int(band.get(BREEDING_CEILING_KEY, CEILING_LIFTED))

static func population_of(band: Dictionary) -> int:
	return int(band.get(BREEDING_POPULATION_KEY, 0))

## Does this band state a limit at all? Lifted, or with no projected reading, it does not.
static func limit_stated(band: Dictionary) -> bool:
	return ceiling_of(band) != CEILING_LIFTED and BandFoodStatus.fertility_is_projected(band)

## Is the limit biting this turn (the row and its caret wear amber)?
static func limit_binds(band: Dictionary) -> bool:
	return ceiling_factor(band) < FACTOR_UNBOUND

## `142 / 198`.
static func row_value(band: Dictionary) -> String:
	return VALUE_FORMAT % [population_of(band), ceiling_of(band)]

## The one line under the row, by precedence — `""` when there is nothing to say. Amber lines come
## back through `bullet_line`; the dim "stay in touch" line comes back through `faint_line`.
static func note_line(band: Dictionary) -> String:
	var population := population_of(band)
	var ceiling := ceiling_of(band)
	var factor := ceiling_factor(band)
	if population > ceiling:
		return bullet_line(BULLET_OVER_LIMIT)
	if factor <= 0.0:
		return bullet_line(BULLET_KIN)
	if factor < FACTOR_UNBOUND:
		return bullet_line(BULLET_ROOM_FORMAT % (ceiling - population))
	var free := LineageWorld.free_breeding_at()
	if free > 0 and ceiling == free:
		return faint_line(STAY_IN_TOUCH_FORMAT % free)
	return ""

## The popover's rows: one per member band, one per other people, then the contact note.
## `band_name` resolves a band id to the roster's name (`""` = unknown).
static func popover_lines(band: Dictionary, band_name: Callable) -> Array[String]:
	var lines: Array[String] = []
	for member_variant in band.get(BREEDING_MEMBERS_KEY, []):
		var member: Dictionary = member_variant
		var id := int(member.get(MEMBER_BAND_ID_KEY, 0))
		var label := String(band_name.call(id))
		if label == "":
			# The sim's own `Band 3` spelling, the event feed's fallback for a band the roster does not hold -
			# reused so one unknown band is never named two ways.
			label = HudEventVocab.SIM_BAND_LABEL_FORMAT % id
		var fading := bool(member.get(MEMBER_FADING_KEY, false))
		var value := MEMBER_VALUE_FORMAT % [int(member.get(MEMBER_LINES_KEY, 0)),
			int(member.get(MEMBER_PEOPLE_KEY, 0))]
		lines.append(table_row(label, HudStyle.INK_DIM_HEX,
			value + (FADING_SUFFIX if fading else ""),
			HudStyle.WARN_HEX if fading else HudStyle.INK_HEX))
	for people_variant in band.get(BREEDING_PEOPLES_KEY, []):
		var people: Dictionary = people_variant
		var fading := bool(people.get(MEMBER_FADING_KEY, false))
		var value := PEOPLE_VALUE_FORMAT % int(people.get(MEMBER_LINES_KEY, 0))
		lines.append(table_row(FactionMark.faction_name(int(people.get(PEOPLE_FACTION_KEY, 0))),
			HudStyle.READY.to_html(false), value + (FADING_SUFFIX if fading else ""),
			HudStyle.WARN_HEX if fading else HudStyle.INK_HEX))
	lines.append(faint_line(CONTACT_NOTE))
	return lines

# ---- the split preview -----------------------------------------------------------------------------

## Round half up — `round()` in GDScript rounds half away from zero, which differs only for negatives,
## but the sim's own rule is stated as half-up so this says so.
static func round_half_up(value: float) -> int:
	return int(floor(value + 0.5))

## The founding lines a split's new band takes: its share of the parent's, at least 1 and (for a band
## holding more than one) at most all but 1. A one-line band copies its line: it takes 1 and keeps 1.
static func split_taken(lines: int, share: float) -> int:
	if lines <= 1:
		return 1
	return clampi(round_half_up(float(lines) * share), 1, lines - 1)

## The lines the home band holds after the split.
static func split_home_lines(lines: int, taken: int) -> int:
	return lines if lines <= 1 else lines - taken

## Σ lines over the breeding population: the members' plus the other peoples'.
static func union_lines(band: Dictionary) -> int:
	var total := 0
	for member in band.get(BREEDING_MEMBERS_KEY, []):
		total += int((member as Dictionary).get(MEMBER_LINES_KEY, 0))
	for people in band.get(BREEDING_PEOPLES_KEY, []):
		total += int((people as Dictionary).get(MEMBER_LINES_KEY, 0))
	return total

## `{new_limit, home_limit}` — each half's ceiling if the two lose touch.
static func split_limits(band: Dictionary, taken: int) -> Dictionary:
	var k := LineageWorld.people_per_line()
	var free := LineageWorld.free_breeding_at()
	var union := union_lines(band)
	if int(band.get(FOUNDING_LINES_KEY, 0)) <= 1:
		var half := k / 2
		return {"new_limit": half, "home_limit": mini((union - 1) * k + half, free)}
	return {"new_limit": mini(taken * k, free), "home_limit": mini((union - taken) * k, free)}

## Does the sheet draw the `FAMILY LIMIT` block? Only for a people still under a limit, with a
## projected reading and the world constants in hand to price the halves with.
static func split_block_shown(band: Dictionary) -> bool:
	return limit_stated(band) and LineageWorld.people_per_line() > 0 \
		and LineageWorld.free_breeding_at() > 0

## The amber warning under the block, `""` when neither half would sit at its limit.
static func split_warning(band: Dictionary, limits: Dictionary, new_people: int) -> String:
	var home_people := population_of(band) - new_people
	if home_people >= int(limits["home_limit"]):
		return bullet_line(HudComposeVocab.SPLIT_LIMIT_HOME_AT)
	if new_people >= int(limits["new_limit"]):
		return bullet_line(HudComposeVocab.SPLIT_LIMIT_NEW_AT)
	return ""
