class_name HudFormat

## THE SHARED HUD FORMAT / VOCABULARY LAYER (docs/plan_hud_decomposition.md).
##
## WHAT THIS IS. The pure `String`/`int` helpers that decide how the HUD SAYS a thing, with no Control
## anywhere in sight: the status-glyph → words → tooltip-line → joined-tooltip chain, the row glyph
## affixes, the policy face, the expedition ROW vocabulary (phase key → phase suffix → the compact
## one-line party summary), the largest-remainder people apportionment + the dependency tooltip, and
## the one 0..1 → whole-percent conversion.
##
## WHY IT IS SEPARATE FROM `HudWidgets`. `FactionReadouts` needs `progress_percent` and nothing else;
## the drawer and the Band panel need the whole status chain AND the widget factory. Keeping the words
## in their own file means the top-bar cluster can depend on the vocabulary without pulling in a
## builder of Controls it never builds. `HudWidgets` depends on THIS, never the other way round.
##
## EVERYTHING HERE IS `static`, STATELESS AND PURE — same invariant as `SourceForecast` and
## `HudWidgets`. The word TABLES (`STATUS_LABELS`, `STATUS_HINTS`, `EXPEDITION_PHASE_LABELS`) live in
## `HudExpeditionVocab` and are read as `HudExpeditionVocab.X`, so there is still exactly one place a
## phrase is typed.
## Where a formatter needs HUD state it takes it as a PARAMETER rather than reaching for it — see
## `panel_expedition_summary`'s `herd_label_for_id` Callable, the `HudWidgets.build_worker_stepper`
## `current_turn` precedent.

## A food module the table cannot name and whose key is empty — the tile carries no module at all.
const FOOD_MODULE_UNKNOWN_LABEL := "Unknown"
## Dependency ratio: dependents per this many working-age adults.
const PEOPLE_DEPENDENCY_BASE := 100
## SHORT on purpose: the chip's face already carries the count, so the tooltip only has to say what a
## dependent IS and who carries them. The long version (which also quoted the ratio) explained the
## jargon without making it any more useful — the ratio itself is gone from the UI entirely.
## The food store is `storage`, never `larder` — a tooltip is read at a glance, and one store carries
## one word across the panel (`DetailFormat.FERTILITY_LABEL_RESERVE` and its trend twins).
const PEOPLE_DEPENDENCY_TOOLTIP := "%d dependents · %d workers"
## Second line when dependents outnumber workers — the reason the chip is WARN-tinted.
const PEOPLE_DEPENDENCY_HEAVY_TOOLTIP := "\n⚠ More mouths than hands"
## Band/City panel "Active expeditions" mission glyphs — they mirror the map markers
## (MapView EXPEDITION_GLYPH).
const PANEL_EXPEDITION_SCOUT_GLYPH := "⚑"
## The DENIAL raid's mark (`docs/plan_denial_raid.md`) — the same 💀 its footer button and its map
## marker wear, so the mission reads identically at every scale.
const PANEL_EXPEDITION_DENY_GLYPH := "💀"
## The SHIPMENT's mark (arc #527) — the same 📦 its footer button and its map marker wear, the rule
## the denial glyph above states: one mission, one glyph, at every scale.
const PANEL_EXPEDITION_TRADE_GLYPH := "📦"
## The STATIONED party's mark (issue #774) — a party that has dropped anchor at a tied band. Text, not
## the FALLBACK only (`hud/station.png` is its art), and `expedition_mission_glyph` must not hand it
## the scout flag, which would read a posted party as one out looking for something.
const PANEL_EXPEDITION_STATION_GLYPH := "⚓"
## An envoy carries a message, not goods: the letter, where a trade party wears the cart.
const PANEL_EXPEDITION_ENVOY_GLYPH := "✉"

## The mark for a party on `expedition_mission` — the ONE resolution of the three constants above, so
## a surface that marks a party by its mission cannot pick a different glyph from the one
## `panel_expedition_summary` prints or the map token draws (`MapView.EXPEDITION_*_GLYPH`). An absent
## or unrecognised mission answers the SCOUT flag, the same last-resort branch the summary falls
## through to: a party with no readable mission is still a venture staked out on the map, and the flag
## is the client's generic mark for one.
static func expedition_mission_glyph(mission: String) -> String:
    var key := mission.strip_edges().to_lower()
    if key == HudExpeditionVocab.EXPEDITION_MISSION_DENY:
        return PANEL_EXPEDITION_DENY_GLYPH
    if key == HudExpeditionVocab.EXPEDITION_MISSION_TRADE:
        return PANEL_EXPEDITION_TRADE_GLYPH
    if key == HudExpeditionVocab.EXPEDITION_MISSION_STATION:
        return PANEL_EXPEDITION_STATION_GLYPH
    if key == HudExpeditionVocab.EXPEDITION_MISSION_ENVOY:
        return PANEL_EXPEDITION_ENVOY_GLYPH
    return PANEL_EXPEDITION_SCOUT_GLYPH

## The `HudSprites` mark id for a party on `mission` — `scout` / `deny` / `trade` / `station`, resolved
## by the same fork `expedition_mission_glyph` takes (an absent or unknown mission is the scout's), so
## a party's art and its fallback glyph cannot name two different missions.
static func expedition_mission_mark(mission: String) -> String:
    var key := mission.strip_edges().to_lower()
    if key == HudExpeditionVocab.EXPEDITION_MISSION_DENY:
        return "deny"
    if key == HudExpeditionVocab.EXPEDITION_MISSION_TRADE:
        return "trade"
    if key == HudExpeditionVocab.EXPEDITION_MISSION_STATION:
        return "station"
    if key == HudExpeditionVocab.EXPEDITION_MISSION_ENVOY:
        return "envoy"
    return "scout"

## **THE LAST-RESORT BAND NAME, KEYED ON THE DURABLE `band_id` — NEVER ON A ROW NUMBER.** Used only
## when the cohort carries no `name`, which a real snapshot never produces (the sim mints one at
## founding for every band); it exists so a hand-built fixture that forgot the key shows a visible
## tell instead of a blank row. The `#` is deliberate: this is an id, so it stays put when another
## band dies, unlike the positional "Band 2" this replaced.
const BAND_ID_FALLBACK_NAME_FORMAT := "Band #%d"
## The band's hex coordinates in the Band/City panel HEADER, beside its stage word — the header's
## other word is `band_name` below, so the pair lives together. Coordinates are the
## band's IDENTITY ("which one am I looking at"), not a vital, which is why they read as a bare
## parenthesised pair here rather than as a labelled `Position:` row in the band zone's vitals.
const BAND_HEADER_POSITION_FORMAT := "(%d, %d)"

## **THE LEADER, in the Band tab's leader block** (issue #859) -- the block at the top of the tab
## that holds the chief-progress bar until there is a leader, so the two never draw together. `leader_kind` is the wire's
## enum: 0 none, 1 chief, 2 elder council. A council has a named person too -- its speaker.
const LEADER_KIND_NONE := 0
const LEADER_KIND_CHIEF := 1
const LEADER_KIND_ELDER_COUNCIL := 2
const BAND_LEADER_CHIEF_FORMAT := "Chief %s"
const BAND_LEADER_COUNCIL_FORMAT := "Elders' council — %s speaks"
## **THE CHIEF-PROGRESS BAR** (#859) under the header's sub line, while a band has no leader. Four
## sources in this order, each a distinct hue; the remainder to the threshold is the empty cell.
const CHIEF_LABEL_FORMAT := "%d / %d"
## The word before the progress bar in the Band tab's leader block (#859): `Chief <bar> 7 / 10`.
const CHIEF_ROW_LABEL := "Chief"
## The compact bar's filled and empty cell glyphs -- the block glyphs `meter_bar` draws.
const CHIEF_BAR_FILLED_GLYPH := "▰"
const CHIEF_BAR_EMPTY_GLYPH := "▱"
const CHIEF_TOOLTIP_HEADLINE := "A band gains a leader once it does enough work that needs an organiser."
const CHIEF_TOOLTIP_SOURCE_FORMAT := "%s: %d"
const CHIEF_TOOLTIP_SOURCE_SEPARATOR := " · "
## `{key, label, color}` per source: the wire key it reads, the words the tooltip uses.
const CHIEF_SOURCE_KEYS := ["leadership_from_builds", "leadership_from_splits",
    "leadership_from_herd", "leadership_from_stages"]
const CHIEF_SOURCE_LABELS := ["Cultivate, Farm, Tame or Corral", "Splits", "Following a herd", "Settlement stages"]

## THE FACTION PAGE's header identity (issue #450), beside the band header vocabulary above because
## the two fill the SAME three header slots and must be read together. The panel's header takes a
## stage id, a glyph, a name and a stage word; a faction has no settlement stage and no tile, so it
## supplies an empty stage id (no bundled art resolves, `StageSprites.for_stage("")` answers null),
## its own glyph, and its BAND COUNT where a band puts its stage word — the identity fact at that
## scale. It supplies no position label, so the header's coordinate slot hides itself.
## **The glyph is deliberately NOT `⚑`**, which is the scouting mission's on the parties rows, nor any
## of the settlement stages' `⛺ 🛖 🏘️` — a faction page marked with a settlement stage would read as
## one more band in the cycle, which is precisely the thing it is not.
const FACTION_PAGE_GLYPH := "❖"

## "Your people", the voice the knowledge strip already speaks in (`⚒ Your people know:`) rather than
## a bare "Faction" — the panel names a thing the player commands, not a database row.
const FACTION_PAGE_NAME := "Your people"

const FACTION_BANDS_LABEL_FORMAT := "%d bands"

const FACTION_BANDS_LABEL_ONE := "1 band"

## The faction header's second line: how many bands this page is summing. Singular is spelled out
## rather than left to a `%d bands` that would read "1 bands" the whole of the early game, which is
## every player's first hour.
static func faction_bands_label(count: int) -> String:
    return FACTION_BANDS_LABEL_ONE if count == 1 else FACTION_BANDS_LABEL_FORMAT % count

## `Your people · 2 bands` — the header's second line once the faction has a NAME to lead with. The
## "your" moves down here rather than being lost: the title is the people's own name now, and without
## this a player reading `Veldari` over a band count could not tell whose rollup it is.
const FACTION_PAGE_OWNED_FORMAT := "%s · %s"

## THE FACTION PAGE'S TITLE — the player's people by the name the sim minted for them
## (`FactionNames`), or `Your people` before the table has arrived. Read through the store rather than
## through `FactionMark.faction_name`, whose fallback is `Faction 0`: a database row is the wrong thing
## to title the player's own page with.
static func faction_page_title() -> String:
    var name := FactionNames.name_of(HudConst.PLAYER_FACTION_ID)
    return name if name != "" else FACTION_PAGE_NAME

## …and its second line: the band count, prefixed `Your people ·` exactly when the title is a name.
static func faction_page_subtitle(count: int) -> String:
    var bands := faction_bands_label(count)
    if FactionNames.name_of(HudConst.PLAYER_FACTION_ID) == "":
        return bands
    return FACTION_PAGE_OWNED_FORMAT % [FACTION_PAGE_NAME, bands]

## The food-module display names. This table came here WITH `food_module_label`, its only reader —
## the words belong to the vocabulary layer, not to the compose builders that print them.
const FOOD_MODULE_LABELS := {
    "coastal_littoral": "Coastal Littoral",
    "riverine_delta": "Riverine Delta",
    "savanna_grassland": "Savanna Grassland",
    "temperate_forest": "Temperate Forest",
    "boreal_arctic": "Boreal Arctic",
    "montane_highland": "Montane Highland",
    "wetland_swamp": "Wetland Swamp",
    "semi_arid_scrub": "Semi-Arid Scrub",
    "coastal_upwelling": "Coastal Upwelling",
    "mixed_woodland": "Mixed Woodland",
}

## A food module's display name, from the table above; an unlisted key humanizes its own id, and an
## empty one reads "Unknown" (the compose sheet's header fallback when the tile carries no label).
static func food_module_label(module_key: String) -> String:
    if module_key == "":
        return FOOD_MODULE_UNKNOWN_LABEL
    return String(FOOD_MODULE_LABELS.get(module_key, module_key.capitalize().replace("_", " ")))

## **THE CLIENT'S ONE BAND-NAMING RULE.** Every surface that prints a band — the cycler, the band
## picker, the faction page's drill-downs, the map markers, the event dock's `band=` substitution —
## resolves its words here, so one band cannot be called two things on two screens.
##
## **THE SIM OWNS THE NAME** (issue #615): the cohort's `name` is minted at founding and is IDENTITY,
## so nothing here derives one by counting. The rule this replaced numbered ROSTER POSITIONS, and it
## shipped two different counts — the picker skipped expedition parties and the map did not, so one
## live party made the map call a band `Band 5` while the picker called the same band `Band 4`, and
## any band death silently renamed every band after it.
##
## **AN EXPEDITION PARTY IS ITS BAND'S PEOPLE, TAGGED WITH WHAT THEY ARE OUT DOING** — `Ashfell
## (Scout)`. The party publishes its home band's name verbatim (it is not a second identity) while
## carrying its own `band_id`, so the suffix is what lets the map draw the band and its party as
## distinct markers without inventing a second name for one people. An unknown or absent mission
## degrades to the bare name rather than to an empty `Ashfell ()`.
##
## **THE SUFFIX IS COMPOSED HERE, NOT AT THE CALL SITES**, and that is the point: a caller that
## appended its own mission word would be a second naming rule, which is exactly what this function
## exists to make impossible.
##
## An empty `name` falls back to `BAND_ID_FALLBACK_NAME_FORMAT` — see its own comment for why that
## is unreachable from a real snapshot.
static func band_name(entry: Dictionary) -> String:
    var name := String(entry.get("name", "")).strip_edges()
    if name == "":
        name = BAND_ID_FALLBACK_NAME_FORMAT % int(entry.get("band_id", HudConst.NO_BAND_ID))
    if not bool(entry.get("is_expedition", false)):
        return name
    var mission := String(entry.get("expedition_mission", "")).strip_edges().to_lower()
    if not HudExpeditionVocab.PARTY_SHORT_LABELS.has(mission):
        return name
    return HudExpeditionVocab.PARTY_NAME_FORMAT % [name, HudExpeditionVocab.PARTY_SHORT_LABELS[mission]]

## The leader block's text line ("Chief Orrin" or "Elders' council — Orrin speaks"), `""` when the
## band has no leader (the chief-progress bar fills the block instead, see `band_chief_progress`).
static func band_leader_phrase(entry: Dictionary) -> String:
    var leader_name := String(entry.get("leader_name", "")).strip_edges()
    if leader_name == "":
        return ""
    match int(entry.get("leader_kind", LEADER_KIND_NONE)):
        LEADER_KIND_CHIEF:
            return BAND_LEADER_CHIEF_FORMAT % leader_name
        LEADER_KIND_ELDER_COUNCIL:
            return BAND_LEADER_COUNCIL_FORMAT % leader_name
    return ""

## The segment hues, in `CHIEF_SOURCE_KEYS` order. Read at call time because `HudStyle`'s palette is
## installed at runtime.
static func chief_source_colors() -> Array:
    return [HudStyle.HEALTHY, HudStyle.READY, HudStyle.WARN, HudStyle.SIGNAL]

## **PROGRESS TOWARD A CHIEF**, or `{}` when there is nothing to draw: the band already has a leader,
## or carries no threshold (a foreign cohort). Otherwise `{label, tooltip, segments, remainder,
## total}`: `segments` is `[{key, points, color}]` in source order, `remainder` the points still to
## go (the empty cell), `total` the threshold. The label's number is practice floored and capped at
## the threshold, whole numbers.
static func band_chief_progress(entry: Dictionary) -> Dictionary:
    if int(entry.get("leader_kind", LEADER_KIND_NONE)) != LEADER_KIND_NONE:
        return {}
    var threshold := float(entry.get("leadership_threshold", 0.0))
    if threshold <= 0.0:
        return {}
    var practice := minf(float(entry.get("leadership_practice", 0.0)), threshold)
    var colors := chief_source_colors()
    var segments: Array = []
    var lines: Array[String] = []
    for i in range(CHIEF_SOURCE_KEYS.size()):
        var points := float(entry.get(CHIEF_SOURCE_KEYS[i], 0.0))
        segments.append({"key": CHIEF_SOURCE_KEYS[i], "points": points, "color": colors[i]})
        lines.append(CHIEF_TOOLTIP_SOURCE_FORMAT % [CHIEF_SOURCE_LABELS[i], int(round(points))])
    return {
        "label": CHIEF_LABEL_FORMAT % [int(floor(practice)), int(round(threshold))],
        "tooltip": CHIEF_TOOLTIP_HEADLINE + "\n" + CHIEF_TOOLTIP_SOURCE_SEPARATOR.join(lines),
        "segments": segments,
        "remainder": maxf(threshold - practice, 0.0),
        "total": threshold,
    }

## The compact four-source bar as BBCode: one coloured `▰` per point, each source in its own hue, then
## the remainder as `▱`. One cell per point of the threshold, so it is as wide as the threshold (10).
static func chief_bar_bbcode(progress: Dictionary) -> String:
    var cells := int(round(float(progress.get("total", 0.0))))
    var out := ""
    var used := 0
    for segment in progress.get("segments", []):
        var count := mini(int(round(float(segment.get("points", 0.0)))), cells - used)
        if count <= 0:
            continue
        used += count
        out += "[color=#%s]%s[/color]" % [(segment.get("color", HudStyle.INK_FAINT) as Color).to_html(false),
            CHIEF_BAR_FILLED_GLYPH.repeat(count)]
    if cells > used:
        out += "[color=#%s]%s[/color]" % [HudStyle.INK_FAINT.to_html(false),
            CHIEF_BAR_EMPTY_GLYPH.repeat(cells - used)]
    return out

## **A BAND WITH ITS PEOPLE** -- `Netherford, Lothari` -- or the people alone when no band is named
## (a builder the roster does not hold, or the wire's 0).
static func band_with_people(band_label: String, faction: int) -> String:
    var people := FactionMark.faction_name(faction)
    if band_label.strip_edges() == "":
        return people
    return HudSelectionVocab.BAND_PEOPLE_FORMAT % [band_label, people]

## "<glyph> " for a resolved glyph, "" for none — so a Current-actions row degrades to bare text
## (no stray leading space) when the resource can't be resolved.
static func source_icon_prefix(icon: String) -> String:
    return "%s " % icon if icon != "" else ""

## A trailing glyph on a row ("  ♻" / "  ●"), separated from the label — "" for an unknown/absent
## glyph, so a row with no policy / no status renders bare rather than trailing whitespace.
static func row_glyph_suffix(glyph: String) -> String:
    return "" if glyph == "" else HudExpeditionVocab.ROW_GLYPH_SEPARATOR + glyph

## Is this party waiting on an ANSWER rather than on orders -- a shipment or an envoy standing in a camp?
## `awaiting` only; any other phase of any mission reads as it always did.
static func waits_on_an_answer(phase: String, mission: String) -> bool:
    return phase.strip_edges().to_lower() == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING \
        and HudExpeditionVocab.MISSIONS_WAITING_ON_AN_ANSWER.has(mission.strip_edges().to_lower())

## Is this party parked and WAITING FOR THE PLAYER'S ORDERS -- the demand amber and the hot badge mean? A
## shipment or an envoy standing in a camp is parked too, but it waits on an answer and asks nothing.
static func awaits_orders(exp: Dictionary) -> bool:
    var phase := expedition_phase_key(exp)
    return phase == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING \
        and not waits_on_an_answer(phase, String(exp.get("expedition_mission", "")))

## Humanize an expedition phase id ("awaiting" → "Awaiting orders"). `mission`, where the caller has it,
## rewrites `awaiting` on a trade or envoy party to `Waiting on an answer`: telling the player to give a
## shipment orders is wrong, since it is waiting in their camp for the answer.
static func expedition_phase_label(phase: String, mission: String = "") -> String:
    var key := phase.strip_edges().to_lower()
    if waits_on_an_answer(key, mission):
        return HudExpeditionVocab.EXPEDITION_PHASE_WAITING_ON_ANSWER_LABEL
    if HudExpeditionVocab.EXPEDITION_PHASE_LABELS.has(key):
        return HudExpeditionVocab.EXPEDITION_PHASE_LABELS[key]
    return key.capitalize()

## The WORDS behind a status glyph. Order-level statuses come from `HudExpeditionVocab.STATUS_LABELS`; an expedition
## PHASE reads from `HudExpeditionVocab.EXPEDITION_PHASE_LABELS` (`expedition_phase_label`), which stays the single
## source of truth for the phase words — they are never re-typed here.
static func status_label(status: String, mission: String = "") -> String:
    var key := status.strip_edges().to_lower()
    if key == "":
        return ""
    if HudExpeditionVocab.STATUS_LABELS.has(key):
        return String(HudExpeditionVocab.STATUS_LABELS[key])
    return expedition_phase_label(key, mission)

## One tooltip line spelling a status glyph out: the word plus its behaviour hint ("Pending — starts
## when you advance the turn"); a status whose word says it all (`Working`) renders bare.
static func status_tooltip_line(status: String, mission: String = "") -> String:
    var label := status_label(status, mission)
    if label == "":
        return ""
    var hint := HudExpeditionVocab.STATUS_HINT_WAITING_ON_ANSWER if waits_on_an_answer(status, mission) \
        else String(HudExpeditionVocab.STATUS_HINTS.get(status.strip_edges().to_lower(), ""))
    return label if hint == "" else HudExpeditionVocab.STATUS_HINT_FORMAT % [label, hint]

## Append the status words to a row tooltip. The glyph on the row is terse by design, so the hover
## must carry what it encodes — composed WITH the tooltip the row already had (yield readout,
## overstaffing explanation, policy hint), never replacing it.
static func append_status_tooltip(tooltip: String, status: String) -> String:
    var status_line := status_tooltip_line(status)
    if status_line == "":
        return tooltip
    return status_line if tooltip == "" else tooltip + SourceForecast.TOOLTIP_LINE_SEPARATOR + status_line

## Join the non-empty parts of a row tooltip (yield readout · policy behaviour · …) into one block.
static func join_tooltip_lines(lines: Array) -> String:
    var parts: Array[String] = []
    for line in lines:
        var text := String(line)
        if text != "":
            parts.append(text)
    return SourceForecast.TOOLTIP_LINE_SEPARATOR.join(parts)


## rung readout shares (the gate-reason lines, the work inspector's standing-investment line and its
## confirm), so a rung can never read one way beside the picker and another in the dialog.
static func policy_face(policy: String) -> String:
    return "%s%s" % [source_icon_prefix(FoodIcons.for_policy(policy)), policy.capitalize()]

## **THE PLANT WEB'S CREW NOUN, AND THE ONE PLACE IT IS DECIDED.** It is `Harvesters` at every rung —
## wild stand, Tended Patch, Field — because the crew never changes; only the ground does, and the
## row's rung MARK already says which ground it is (`docs/plan_standing_upkeep.md` §4.9 item 12c).
##
## **IT STAYS A FUNCTION THOUGH IT NO LONGER FORKS.** Four code call sites and this docstring are the
## documented seam for the plant web's crew word; inlining the const would scatter it across the
## drawer, the sheet and the work board, which is exactly the shape the seam was built to prevent.
##
## ⛔ **THE RUNG-KEYED FORK IS RETIRED, AND HERE IS WHAT IT CLAIMED.** Verbatim: *"A wild stand is
## drawn down by FORAGERS; a Tended Patch or a Field is kept by TENDERS. The authority is the ladder
## config itself … where the `wild` rung declares the harvest primitive `worker_take` and both upper
## rungs declare `worker_tend`"*; the test was `improvement_is_done(…, CULTIVATE)`, i.e. *does this
## patch STAND at or above `plant:tended`* — true on a Tended Patch and on a Field sown straight from
## wild ground; and *"A BUILD IN FLIGHT KEEPS THE WILD NOUN, deliberately … people part-way through a
## Cultivate or a Sow — who really are foraging the stand while they clear ground — stay Foragers
## until the rung COMPLETES."* Every one of those statements is still true of the SIM. What retired
## them is that the second player-facing word was already taken: a Field's sheet read `ASSIGN TENDERS`
## and then offered the *Gathering* kit, which looks like a bug and is not — the tending is the
## AGRICULTURE pool's, and a hoe does nothing for a harvest. Reported from play by Ray, who knows how
## it works and was still caught by it in the moment.
##
## **THE VERB IS A SEPARATE SLOT** — `HudComposeVocab.HARVEST_ASSIGN_BUTTON` (`Harvest`) — and the
## noun→verb tables (`PLANT_ASSIGN_BUTTONS` / `PLANT_NOOP_HINTS`) still key off what this returns.
##
## **DISPLAY ONLY.** The command is still `assign_labor` with kind `forage`
## (`SourceForecast.LABOR_KIND_FORAGE`); nothing on the wire moves with this word.
##
## `src` / `prefix` are kept so every caller keeps reading the SOURCE it is naming a crew for — the
## day a rung earns its own noun again, the answer is already in hand.
static func plant_crew_label(_src: Dictionary, _prefix: String) -> String:
    return HudComposeVocab.HARVEST_CREW_LABEL

# ---- The escapement floor, in words --------------------------------------------------------------

## A FLOOR PRESET's display FACE — its zone glyph welded to its label (`💀 Take everything`). The one
## floor vocabulary every preset readout shares, so a preset can never read one way on the picker and
## another in a tooltip.
static func floor_preset_face(preset: String) -> String:
    var floor := SourceForecast.floor_for_preset(preset)
    var glyph := FoodIcons.for_floor_zone(SourceForecast.floor_zone(floor))
    return "%s%s" % [source_icon_prefix(glyph), HudComposeVocab.FLOOR_PRESET_LABELS.get(preset, "")]

## **THE ONE SENTENCE SAID ABOUT A FLOOR** — the replacement for the three per-stance hint tables, and
## the whole of what the client says about harvest pressure. It is composed rather than looked up
## because one fact varies independently of the zone: WHAT STRIPPING COSTS differs by web (a patch
## reseeds, a herd is gone for good). Everything else is one table of five.
##
## `kind` is a `SourceForecast.LABOR_KIND_*`.
static func floor_hint(floor: float, kind: String) -> String:
    var zone := SourceForecast.floor_zone(floor)
    var text := String(HudComposeVocab.FLOOR_ZONE_HINTS.get(zone, ""))
    if zone == SourceForecast.FLOOR_ZONE_STRIP:
        return text % String(HudComposeVocab.FLOOR_STRIP_CONSEQUENCE.get(kind, ""))
    return text

## A 0..1 progress track (knowledge / domestication) as a whole percent. 0 is a MEANINGFUL reading in
## a gate reason — it tells the player they haven't started the track at all.
##
##
## **THE `clampf` ABOVE IS WHAT MAKES FLOORING SAFE AT THE TOP END, and it is not incidental.** A
## genuinely complete source publishes `>= 1.0` — often a hair over — so the clamp pins it to exactly
## `1.0` and it still prints `100`. Flooring therefore makes `100%` reachable ONLY at a true
## completion, which is the property the readout is supposed to have. Do not "fix" this back to
## `round()`: the clamp is not protecting against the rounding, it is what lets the floor be exact.
static func progress_percent(progress: float) -> int:
    return int(floor(clampf(progress, 0.0, 1.0) * HudConst.PROGRESS_PERCENT_SCALE))

# ---- People: apportionment + the dependency vocabulary -------------------------------------------

## Divide whole people into fractional parts SO THEY STILL SUM TO AN EXPLICIT TOTAL — the
## largest-remainder method: floor every part, then hand the leftover people out to the biggest
## fractions, biggest first. `round()` per part does NOT preserve the total, and a panel that
## disagrees with itself about how many people are in a band reads as a bug in both readouts.
##
## **This is the client's OWN arithmetic, never a second opinion on the sim's.** The age brackets
## arrive as whole people already — the sim rounds them once and guarantees they sum to `size` — so
## nothing rounds them here. What genuinely needs rounding is the SPLIT SHEET: 9 whole children
## divided by a 40% share the player chose is 3.6 children, and somebody has to decide.
##
## **The split sheet is why this takes an explicit target.** It apportions both halves of a band in
## ONE pass so their displayed people sum to the band's own displayed total — and it holds the chosen
## worker count out of the parts entirely, that being an integer the player picked rather than a
## fraction to round. So the target is not `round(sum(parts))`; it is the band's people minus those
## pinned workers.
static func apportion_people_to(parts: Array[float], target: int) -> Array[int]:
    var whole: Array[int] = []
    var assigned := 0
    for part in parts:
        var floored: int = maxi(int(floor(part)), 0)
        whole.append(floored)
        assigned += floored
    var leftover := target - assigned
    while leftover > 0:
        var best := -1
        var best_fraction := -1.0
        for i in range(parts.size()):
            var fraction: float = maxf(parts[i], 0.0) - float(whole[i])
            if fraction > best_fraction:
                best_fraction = fraction
                best = i
        if best < 0:
            break
        whole[best] += 1
        leftover -= 1
    return whole

## Dependents per 100 working-age adults — the ratio itself, which only the tooltips render now.
static func dependency_per_hundred(dependents: int, working: int) -> int:
    if working <= 0:
        return 0
    return int(round(float(dependents) / float(working) * float(PEOPLE_DEPENDENCY_BASE)))

## What "dependents" MEANS, in the player's terms. The ratio is no longer shown anywhere — it only
## decides the WARN tint — so it stays out of the words too. `PEOPLE_DEPENDENCY_HEAVY` lives in
## `HudWorkVocab` (the chip's own tint reads it too) and is read as `HudWorkVocab.X`.
static func dependency_tooltip(dependents: int, working: int) -> String:
    var text: String = PEOPLE_DEPENDENCY_TOOLTIP % [dependents, working]
    if dependency_per_hundred(dependents, working) > HudWorkVocab.PEOPLE_DEPENDENCY_HEAVY:
        text += PEOPLE_DEPENDENCY_HEAVY_TOOLTIP
    return text

# ---- Expedition row vocabulary -------------------------------------------------------------------

## The expedition's sim phase key, normalized (the wire's `ExpeditionPhase` string).
static func expedition_phase_key(exp: Dictionary) -> String:
    return String(exp.get("expedition_phase", "")).strip_edges().to_lower()

## The phase as it renders ON the row: the glyph alone, except `awaiting`, which keeps its words
## (`▮▮ Awaiting orders`) — a demand on the player must read without a hover.
static func expedition_phase_suffix(phase: String, mission: String = "") -> String:
    var suffix := row_glyph_suffix(FoodIcons.for_status(phase))
    if phase == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
        return "%s %s" % [suffix, expedition_phase_label(phase, mission)]
    return suffix

## Compact one-line expedition summary: deny → `💀 <herd>  <phase glyph>`; trade →
## `📦 <destination>  <phase glyph>`; scout → `⚑ → (x, y)  <phase glyph>`. The phase reads as a GLYPH
## here exactly as it does on the Current-actions rows (one concept, one rendering, in both sections
## of the same panel); the words live in the tooltip. No mission carries a floor glyph (none of them
## harvests on a floor) → `row_glyph_suffix` emits nothing, so the row carries the phase glyph alone
## with no orphaned separator. Only `awaiting` keeps
## its words (`expedition_phase_suffix`). The next-delivery detail is NOT here — it lives on the
## parties inspector strip a row click opens (`_build_parties_inspector` → `BandDetailLines.expedition_summary_lines`).
##
## `herd_label_for_id` is the herd vocabulary, THREADED IN rather than reached for: resolving a herd id
## to a species needs the roster + the current selection + the snapshot herd list, which is HUD state
## this stateless layer must not hold (the `HudWidgets.build_worker_stepper` `current_turn` precedent).
## It is called ONLY on the deny branch, so a scout or trade row resolves nothing.
## **WHAT A SHIPMENT'S DESTINATION IS CALLED** — the sim's published name when it has one, else
## whatever THIS CLIENT calls that band, joined on `expedition_destination_band`.
##
## **`expeditionDestinationName` IS EMPTY ON EVERY LIVE SHIPMENT TODAY, AND THAT IS DELIBERATE.**
## Bands have no names in this game, so the sim publishes nothing rather than guessing — it briefly
## published the unit ARCHETYPE (`"BandForager"`, the same string for every band), which made the row
## disagree with the label the rest of the HUD gives that same band. Empty means "no name", exactly as
## an empty material row means "no row".
##
## **The published name still wins when it is non-empty**, and that is the half that matters later:
## the day a second faction lands (#513) a foreign band's name can only come from the sim, because
## this client holds no roster to resolve one from.
##
## `band_label_for_id` is `HudBandLaborState.band_label_for_id` threaded in as a PARAMETER, the
## `herd_label_for_id` treatment: a stateless layer must not reach for the roster that resolver reads.
## `""` when neither tier answers — a caller renders no destination at all rather than the raw
## `BandId`, which is a database key.
static func expedition_destination_label(exp: Dictionary,
        band_label_for_id: Callable = Callable()) -> String:
    var published := String(exp.get("expedition_destination_name", "")).strip_edges()
    if published != "":
        return published
    if not band_label_for_id.is_valid():
        return ""
    return String(band_label_for_id.call(
        int(exp.get("expedition_destination_band", HudConst.NO_BAND_ID)))).strip_edges()

## A party row's TEXT, with no mission mark in it: the mark is drawn beside this string by the host
## (`HudWidgets.build_mission_mark`, or a `Button.icon`), so the emoji of the fallback glyph never
## sits inside a plain Label string where each platform renders it differently.
static func panel_expedition_summary(exp: Dictionary, herd_label_for_id: Callable,
        band_label_for_id: Callable = Callable()) -> String:
    var mission := String(exp.get("expedition_mission", "")).strip_edges().to_lower()
    var phase_suffix := expedition_phase_suffix(expedition_phase_key(exp), mission)
    # DENIAL — the quarry with the mission's own mark and NO floor glyph: the mission has no floor.
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_DENY:
        var quarry := String(herd_label_for_id.call(String(exp.get("expedition_target_herd", "")).strip_edges()))
        return "%s%s" % [quarry, phase_suffix]
    # A SHIPMENT names WHO IT IS FOR — never by `expedition_destination_band`, which is the key the
    # command addresses. `expedition_destination_label` is the one resolution (the sim's published
    # name, else this client's own label for that band), so this row, the drawer's `Bound for` row and
    # the destination picker cannot call one band three things.
    # No floor glyph: a shipment harvests nothing, so it has no pressure to mark.
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_TRADE:
        var destination := expedition_destination_label(exp, band_label_for_id)
        # An unresolvable destination leaves the row as the mission's mark and its phase, rather than
        # an orphaned separator in front of nothing.
        if destination == "":
            return phase_suffix.strip_edges()
        return "%s%s" % [destination, phase_suffix]
    # **AN ENVOY names who it is for and what it carries**: `Brackwater · Road rights · Open borders`,
    # the proposal's terms in words (they are the whole of its errand). One line, clipped by the row,
    # with the whole sentence on the row's hover.
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_ENVOY:
        var envoy_to := expedition_destination_label(exp, band_label_for_id)
        var envoy_terms := HudDealingsVocab.terms_text(Dictionary(exp.get("envoy_terms", {})))
        if envoy_to == "":
            return "%s%s" % [envoy_terms, phase_suffix]
        return "%s%s" % [HudDealingsVocab.ENVOY_ROW_FORMAT % [envoy_to, envoy_terms], phase_suffix]
    # A STATIONED party names the band it is posted with, in a sentence per phase. The phase reads in
    # the sentence, so only `returning` keeps its glyph; `stationed` has none and `outbound`'s arrow
    # would repeat "Going to stay". Carries no food runway and no cargo — the home band feeds it.
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_STATION:
        var host := expedition_destination_label(exp, band_label_for_id)
        var phase := expedition_phase_key(exp)
        var sentence_format := HudExpeditionVocab.STATION_ROW_OUTBOUND_FORMAT
        if phase == HudExpeditionVocab.EXPEDITION_PHASE_STATIONED:
            sentence_format = HudExpeditionVocab.STATION_ROW_STATIONED_FORMAT
        elif phase == HudExpeditionVocab.EXPEDITION_PHASE_RETURNING:
            sentence_format = HudExpeditionVocab.STATION_ROW_RETURNING_FORMAT
        var tail := phase_suffix if phase == HudExpeditionVocab.EXPEDITION_PHASE_RETURNING else ""
        return "%s%s" % [sentence_format % host, tail]
    var x := int(exp.get("current_x", -1))
    var y := int(exp.get("current_y", -1))
    return "→ (%d, %d)%s" % [x, y, phase_suffix]

## A block-glyph bar for a 0–100 score. `cells` is passed by every caller — the Sedentarization meter
## (via FactionReadouts) at the standard width, the knowledge strip narrower, the herd-drawer danger
## rows narrower still. Lives here (the pure format layer) because THREE clusters read it and it
## touches no member; DetailFormat's danger bars and FactionReadouts' meters call it as
## `HudFormat.meter_bar` rather than taking a Callable injection.
static func meter_bar(score: float, cells: int) -> String:
    var filled := int(round(clampf(score / 100.0, 0.0, 1.0) * float(cells)))
    return "▰".repeat(filled) + "▱".repeat(cells - filled)

# ---------------------------------------------------------------------------------------------------
# PROGRESS TOWARD A TRIBE (#861) -- the Band tab's Tribe readout, under the leader block.
# ---------------------------------------------------------------------------------------------------

## The wire's `tribe_standing` for a band with no local partner: 1 no urn, 2 no supply network, 3 alone
## (0 = a partner is local; a 0 with no rows cannot happen and reads as alone).
const TRIBE_STANDING_NO_URN := 1
const TRIBE_STANDING_NO_NETWORK := 2
## The word before a row's bar in the Tribe readout: `Tribe  Sablemoor <bar> 8.2 / 9`.
const TRIBE_ROW_LABEL := "Tribe"
const TRIBE_SCORE_FORMAT := "%.1f / %s"
const TRIBE_NUMBER_FORMAT := "%.1f"
const TRIBE_WHOLE_FORMAT := "%d"
## Rows drawn before the rest fold into a `+N more` line (whose tooltip names every one).
const TRIBE_MAX_ROWS := 3
const TRIBE_MORE_FORMAT := "+%d more"
const TRIBE_MORE_LINE_FORMAT := "%s %s"
const TRIBE_NEEDS_CHIEF := "needs a chief"
const TRIBE_APART_FORMAT := "Apart from %s: shared history fading"
const TRIBE_NAME_SEPARATOR := ", "
const TRIBE_NOTE_NO_URN := "Tribe: no urn yet — needs a place where belief stands"
const TRIBE_NOTE_NO_NETWORK := "Tribe: not in a supply network"
const TRIBE_NOTE_ALONE := "Tribe: no other band of your people shares this urn and network"
## A score within this of the threshold counts as reaching it (float noise from summing seven terms).
const TRIBE_SCORE_EPSILON := 0.001
const TRIBE_HEADLINE_FORMAT := "%s: %s of %s to form a tribe"
const TRIBE_INPUT_LINE_FORMAT := "%s %s / %s — %s"
const TRIBE_NEXT_FORMAT := "Next: %s"
const TRIBE_NEXT_CLOSE := "Close enough — a tribe forms with a chief among you"
const TRIBE_NEXT_ONE_TRIBE := "already one tribe"
const TRIBE_NEXT_TURNS_FORMAT := "about %d more turns together"
const TRIBE_NEXT_ONE_TURN := "about 1 more turn together"
const TRIBE_NEXT_MONUMENT := "Raise a monument at the shared urn"
const TRIBE_NEXT_BELIEF := "More belief at the urn"
const TRIBE_NEXT_TIE := "A stronger tie"
## The closeness inputs in the wire's (and the bar's) order: key, words, what feeds it. `history`'s
## sentence is a format taking the turns that fill it; `acting` is dropped while it contributes 0.
const TRIBE_INPUT_KEYS := ["origin", "likeness", "tie", "history", "belief", "monument", "acting"]
const TRIBE_INPUT_LABELS := ["Shared origin", "Culture", "Tie", "History", "Belief", "Monument", "Acting"]
const TRIBE_INPUT_HINTS := [
    "both descend from the same founding band",
    "how alike your ways are",
    "how strong your connection is",
    "grows each turn you share an urn and supply network (full after %d turns)",
    "belief standing at the shared urn",
    "a kept monument at the shared urn",
    "bands acting together",
]
const TRIBE_KEY_HISTORY := "history"
const TRIBE_KEY_ACTING := "acting"

## One hue per input, in `TRIBE_INPUT_KEYS` order. Runtime-read for the palette, as the chief bar's.
static func tribe_input_colors() -> Array:
    return [HudStyle.HEALTHY, HudStyle.READY, HudStyle.SIGNAL, HudStyle.VOICE_PIGMENT,
        HudStyle.BELIEF, HudStyle.TRADE, HudStyle.HUNT_DANGER_ACCENT]

## A pair's score: the sum of its contribution (the wire says the score IS that sum).
static func tribe_pair_score(pair: Dictionary) -> float:
    var total := 0.0
    var contribution: Dictionary = pair.get("contribution", {})
    for key in TRIBE_INPUT_KEYS:
        total += float(contribution.get(key, 0.0))
    return total

## One decimal, rounded.
static func tribe_decimal(value: float) -> String:
    return TRIBE_NUMBER_FORMAT % value

## A "most"/threshold figure: whole when it is whole (`9`), one decimal otherwise.
static func tribe_most(value: float) -> String:
    if absf(value - roundf(value)) < TRIBE_SCORE_EPSILON:
        return TRIBE_WHOLE_FORMAT % int(roundf(value))
    return tribe_decimal(value)

## The score as shown: truncated (never rounded UP) to one decimal, so `8.96 / 9` cannot read `9.0 / 9`
## while the pair is not yet close.
static func tribe_score_text(score: float, threshold: float) -> String:
    var shown := score if score >= threshold - TRIBE_SCORE_EPSILON else floorf(score * 10.0) / 10.0
    return TRIBE_SCORE_FORMAT % [shown, tribe_most(threshold)]

## **THE NEXT STEP** sentence for one pair, `""` when nothing is unfilled to name. Close enough (score
## at the threshold) says so, or that the pair is already one tribe. Otherwise, if history alone can
## close the gap at its per-turn rate (weight / build turns) it names the turns, `ceil(gap / rate)`;
## failing that it names the biggest unfilled input among monument / belief / tie.
static func tribe_next_step(entry: Dictionary, pair: Dictionary) -> String:
    var threshold := float(entry.get("tribe_threshold", 0.0))
    var score := tribe_pair_score(pair)
    if score >= threshold - TRIBE_SCORE_EPSILON:
        return TRIBE_NEXT_ONE_TRIBE if bool(pair.get("same_tribe", false)) else TRIBE_NEXT_CLOSE
    var weights: Dictionary = entry.get("tribe_weights", {})
    var contribution: Dictionary = pair.get("contribution", {})
    var gap := threshold - score
    var build_turns := float(entry.get("tribe_history_build_turns", 0.0))
    var history_weight := float(weights.get(TRIBE_KEY_HISTORY, 0.0))
    var history_room := history_weight - float(contribution.get(TRIBE_KEY_HISTORY, 0.0))
    if build_turns > 0.0 and history_weight > 0.0 and history_room >= gap - TRIBE_SCORE_EPSILON:
        var turns := int(ceil(gap / (history_weight / build_turns) - TRIBE_SCORE_EPSILON))
        return TRIBE_NEXT_ONE_TURN if turns <= 1 else TRIBE_NEXT_TURNS_FORMAT % turns
    var best := ""
    var best_room := TRIBE_SCORE_EPSILON
    for candidate in [["monument", TRIBE_NEXT_MONUMENT], ["belief", TRIBE_NEXT_BELIEF],
            ["tie", TRIBE_NEXT_TIE]]:
        var room := float(weights.get(candidate[0], 0.0)) - float(contribution.get(candidate[0], 0.0))
        if room > best_room:
            best_room = room
            best = candidate[1]
    return best

## The hover for one pair's row: the headline, one line per input (`got / most — what feeds it`, acting
## omitted while it contributes 0) and the next step.
static func tribe_row_tooltip(entry: Dictionary, pair: Dictionary) -> String:
    var threshold := float(entry.get("tribe_threshold", 0.0))
    var weights: Dictionary = entry.get("tribe_weights", {})
    var contribution: Dictionary = pair.get("contribution", {})
    var lines: Array[String] = [TRIBE_HEADLINE_FORMAT % [String(pair.get("partner_name", "")),
        tribe_decimal(tribe_pair_score(pair)), tribe_most(threshold)]]
    for i in range(TRIBE_INPUT_KEYS.size()):
        var key: String = TRIBE_INPUT_KEYS[i]
        var got := float(contribution.get(key, 0.0))
        if key == TRIBE_KEY_ACTING and got <= 0.0:
            continue
        var hint: String = TRIBE_INPUT_HINTS[i]
        if key == TRIBE_KEY_HISTORY:
            hint = hint % int(roundf(float(entry.get("tribe_history_build_turns", 0.0))))
        lines.append(TRIBE_INPUT_LINE_FORMAT % [TRIBE_INPUT_LABELS[i], tribe_decimal(got),
            tribe_most(float(weights.get(key, 0.0))), hint])
    var next_step := tribe_next_step(entry, pair)
    if next_step != "":
        lines.append(TRIBE_NEXT_FORMAT % next_step)
    return "\n".join(lines)

## The split bar for one pair as BBCode: one cell per point of the threshold, each input's cells in its
## own hue (at least one cell for a positive input, never past the width), zero inputs skipped, the
## remainder `▱`. A score over the threshold fills the bar.
static func tribe_bar_bbcode(entry: Dictionary, pair: Dictionary) -> String:
    var contribution: Dictionary = pair.get("contribution", {})
    var colors := tribe_input_colors()
    var segments: Array = []
    for i in range(TRIBE_INPUT_KEYS.size()):
        var points := float(contribution.get(TRIBE_INPUT_KEYS[i], 0.0))
        if points <= 0.0:
            continue
        var cells := maxi(int(roundf(points)), 1)
        segments.append({"key": TRIBE_INPUT_KEYS[i], "points": float(cells), "color": colors[i]})
    return chief_bar_bbcode({"total": float(entry.get("tribe_threshold", 0.0)), "segments": segments})

## **THE TRIBE READOUT** for a band, or `{}` for a band with no threshold (a foreign row). Otherwise
## `{rows, more, apart, note}`: `rows` is up to `TRIBE_MAX_ROWS` of `{name, score, label, bar, tooltip,
## needs_chief}` for the LOCAL partners, best score first; `more` is `{text, tooltip}` or `{}` for the
## partners past the cap (every one named, no content lost); `apart` the one line for parted partners
## whose shared history still fades, `""` for none; `note` the dim standing line when no partner is
## local, `""` otherwise.
static func band_tribe_progress(entry: Dictionary) -> Dictionary:
    var threshold := float(entry.get("tribe_threshold", 0.0))
    if threshold <= 0.0:
        return {}
    var local_pairs: Array = []
    var apart_names: Array[String] = []
    for pair in entry.get("tribe_pairs", []):
        if bool(pair.get("local", false)):
            local_pairs.append(pair)
        elif float(pair.get("history", 0.0)) > 0.0:
            apart_names.append(String(pair.get("partner_name", "")))
    local_pairs.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
        var score_a := tribe_pair_score(a)
        var score_b := tribe_pair_score(b)
        if absf(score_a - score_b) < TRIBE_SCORE_EPSILON:
            return String(a.get("partner_name", "")) < String(b.get("partner_name", ""))
        return score_a > score_b)
    var no_leader := int(entry.get("leader_kind", LEADER_KIND_NONE)) == LEADER_KIND_NONE
    var rows: Array = []
    var more_lines: Array[String] = []
    for i in range(local_pairs.size()):
        var pair: Dictionary = local_pairs[i]
        var score := tribe_pair_score(pair)
        if i >= TRIBE_MAX_ROWS:
            more_lines.append(TRIBE_MORE_LINE_FORMAT % [String(pair.get("partner_name", "")),
                tribe_score_text(score, threshold)])
            continue
        rows.append({
            "name": String(pair.get("partner_name", "")),
            "score": score,
            "label": tribe_score_text(score, threshold),
            "bar": tribe_bar_bbcode(entry, pair),
            "tooltip": tribe_row_tooltip(entry, pair),
            "needs_chief": score >= threshold - TRIBE_SCORE_EPSILON and not bool(pair.get("same_tribe", false))
                and no_leader and not bool(pair.get("partner_has_leader", false)),
        })
    var more := {}
    if not more_lines.is_empty():
        more = {"text": TRIBE_MORE_FORMAT % more_lines.size(), "tooltip": "\n".join(more_lines)}
    var note := ""
    if rows.is_empty():
        match int(entry.get("tribe_standing", 0)):
            TRIBE_STANDING_NO_URN:
                note = TRIBE_NOTE_NO_URN
            TRIBE_STANDING_NO_NETWORK:
                note = TRIBE_NOTE_NO_NETWORK
            _:
                note = TRIBE_NOTE_ALONE
    return {
        "rows": rows,
        "more": more,
        "apart": TRIBE_APART_FORMAT % TRIBE_NAME_SEPARATOR.join(apart_names) if not apart_names.is_empty() else "",
        "note": note,
    }

## One row's BBCode: dim `Tribe`, the partner, the bar, the number, then an amber `needs a chief` when
## the pair is close but no one leads either band.
static func tribe_row_bbcode(row: Dictionary) -> String:
    var text := "[color=#%s]%s[/color] %s %s %s" % [HudStyle.INK_DIM.to_html(false), TRIBE_ROW_LABEL,
        String(row.get("name", "")), String(row.get("bar", "")), String(row.get("label", ""))]
    if bool(row.get("needs_chief", false)):
        text += " [color=#%s]%s[/color]" % [HudStyle.WARN.to_html(false), TRIBE_NEEDS_CHIEF]
    return text

## A dim one-line note's BBCode.
static func tribe_note_bbcode(text: String) -> String:
    return "[color=#%s]%s[/color]" % [HudStyle.INK_FAINT.to_html(false), text]
