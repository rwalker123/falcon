class_name RungLadder
extends RefCounted

## **All-`static`, stateless** shared LADDER-TRACK layer — the one answer to *"what does this source's
## branch hold, where does it stand on it, and how far may the player send it?"*
##
## **WHY IT EXISTS AT ALL.** `RungGates` answers *"may this source climb its NEXT rung"*, which is the
## right question for a MARK. A destination picker asks a different one: the sim stores **one position
## per source** in cumulative work units and a queue entry names a **destination rung**, climbing every
## rung between where the source stands and where it was sent
## (`docs/plan_standing_upkeep.md` §2.8). So the surface has to state the WHOLE branch — the rungs
## already paid for, the one the source stands on, the ones it could be taken to, and the ones it
## cannot — and `next_rung_ready` structurally cannot say any of that.
##
## **WHY ITS OWN FILE.** It is called by the work board's row (the `⌃` mark's track) and by nothing
## else *yet*, and it would have been a private method there — except that `BandPanelController` is a
## controller and this is arithmetic over a wire dict, which is the split
## `.claude/rules/client/hud-modules.md` draws between a controller and a shared layer. The same
## measurement that produced `SourceForecast`, `RungGates` and `KitRoster`.
##
## **STATELESS IS THE INVARIANT** — no node, no `_hud`, no snapshot cache. The one piece of HUD state
## the gates need, **faction knowledge**, is threaded in as a `knowledge` PARAMETER exactly as
## `RungGates` threads it, and the ROW BUILDER takes its press handler as a `Callable`.
##
## ⛔ **IT RE-DERIVES NEITHER THE WORK NOR THE TURNS.** A leg's owing and its chained date are the
## sim's (`SourceForecast.build_legs`), read where the queued entry publishes them. A rung NO entry
## covers has no leg, and its owing is the per-rung `workCost − workDone` pair the wire publishes for
## exactly this purpose — the same two numbers `forage::plant_build_legs` subtracts, which the sim's
## own `patch_rung_work_done` calls *"a readout, not a second authority"*. **The TURNS have no such
## pair**: they are chained against a build queue this client cannot see, so a rung with no leg states
## no date at all rather than a number this surface has no right to.

# ---- THE FIVE STATES ------------------------------------------------------------------------------
#
# Read them as one enumeration: every rung of a branch is in exactly one, and the track's whole job is
# to make which one obvious at a glance.

## Already paid for. **It contributes NOTHING to the figures**, which is the property the readout most
## has to make obvious: a previous improvement is a RECEIPT, NOT A DISCOUNT — the player is never
## asked to buy work they have already bought, and is never offered it back either.
const STATE_BANKED := "banked"
## The rung the source stands on today — the highest banked one, or the branch's wild floor.
const STATE_STANDING := "standing"
## Above the standing rung and BELOW the destination a queued entry names: a leg the climb will lay on
## its way, not a thing the player chose for itself.
const STATE_PATH := "path"
## The destination a queued entry names. The climb ends here and the entry leaves the queue.
const STATE_TARGET := "target"
## Reachable on this branch and refused — by knowledge, by this source's own state, or outright by
## what the species or the ground will admit. **A locked rung stays VISIBLE**: a track exists to say
## what the branch holds, and a rung silently missing from it reads as a shorter ladder rather than as
## one this source cannot climb.
const STATE_LOCKED := "locked"
## Above the standing rung, ungated, and not on any chosen path — a destination the player may pick.
const STATE_OPEN := "open"

## The row keys `track` publishes and `build_track` renders. **Named**, because producer and reader are
## different scripts and a typo in a `get` here is a silent empty row.
const ROW_RUNG_KEY := "rung"
const ROW_IMPROVEMENT_KEY := "improvement"
const ROW_NAME_KEY := "name"
const ROW_STATE_KEY := "state"
const ROW_WORK_KEY := "work_remaining"
const ROW_TURNS_KEY := "turns_remaining"
const ROW_REASONS_KEY := "reasons"
const ROW_SELECTABLE_KEY := "selectable"

## A rung whose owing this layer cannot state at all — the wire prices no such job on this source.
## Distinct from `0.0`, which is a real reading meaning *nothing left to pay*.
const WORK_UNKNOWN := -1.0

## **THE WHOLE BRANCH, BOTTOM RUNG FIRST — one row per rung, in CLIMB order.**
##
## The order is the wire's own (`SourceForecast.rung_branch_for_kind`) and it is load-bearing twice
## over: the standing rung is the HIGHEST done one, so a branch walked out of order marks the wrong
## rung as banked; and a destination's path is every rung between standing and it, which is a range
## over this list.
##
## `kind` is a LABOR kind (`forage` / `hunt`), `source` the RAW wire source dict, `prefix` the spelling
## its keys are in, `improvement` the assignment's declared verb (the second axis) and `knowledge` the
## faction's `{track: progress}` row.
static func track(kind: String, source: Dictionary, prefix: String, improvement: String,
        knowledge: Dictionary) -> Array[Dictionary]:
    var rows: Array[Dictionary] = []
    var source_kind := SourceForecast.source_kind_for_labor(kind)
    var branch := SourceForecast.rung_branch_for_kind(source_kind)
    var standing := _standing_index(branch, source, prefix)
    # THE DESTINATION A QUEUED ENTRY NAMES, as an index into this branch. `-1` when nothing is queued
    # here, which is the state the picker is usually opened in — and then no rung is `path` or
    # `target`, because nothing has been chosen.
    var destination := _index_of_improvement(branch,
        SourceForecast.build_destination_rung(source, prefix))
    # …and the legs that entry still owes, keyed by rung so a row can ask for its own. The list is
    # EMPTY on an unqueued source, which is why every figure below has a no-leg answer.
    var legs := _legs_by_rung(source, prefix)
    var gates := _gates(kind, source, prefix, knowledge)
    # **THE PATH IS BLOCKED FROM BELOW.** A climb lays every leg between where the source stands and
    # where it was sent, so a barred rung bars everything above it — and the honest thing to state on
    # the rung above is the refusal that actually happened, further down.
    var blocking_row := {}
    for index in range(branch.size()):
        var rung_key := String(branch[index])
        var verb := String(SourceForecast.RUNG_KEY_IMPROVEMENTS.get(rung_key,
            SourceForecast.IMPROVEMENT_NONE))
        var row := {
            ROW_RUNG_KEY: rung_key,
            ROW_IMPROVEMENT_KEY: verb,
            ROW_NAME_KEY: _rung_name(verb),
            ROW_WORK_KEY: WORK_UNKNOWN,
            ROW_TURNS_KEY: SourceForecast.BUILD_TURNS_NO_ESTIMATE,
            ROW_REASONS_KEY: [] as Array[String],
            ROW_SELECTABLE_KEY: false,
        }
        if index < standing:
            row[ROW_STATE_KEY] = STATE_BANKED
            rows.append(row)
            continue
        if index == standing:
            row[ROW_STATE_KEY] = STATE_STANDING
            rows.append(row)
            continue
        # A rung ABOVE the standing one. Its own refusal first, then whatever refused below it.
        var reasons := _rung_refusals(kind, source, prefix, verb, gates)
        if reasons.is_empty() and not blocking_row.is_empty():
            reasons = [HudFloraVocab.GATE_REASON_PATH_BLOCKED_FORMAT % [
                String(blocking_row[ROW_NAME_KEY]),
                String((blocking_row[ROW_REASONS_KEY] as Array)[0])]]
        row[ROW_WORK_KEY] = _work_remaining(source, prefix, verb, legs, rung_key)
        row[ROW_TURNS_KEY] = _leg_turns(legs, rung_key)
        if not reasons.is_empty():
            row[ROW_STATE_KEY] = STATE_LOCKED
            row[ROW_REASONS_KEY] = reasons
            if blocking_row.is_empty():
                blocking_row = row
        elif index == destination:
            row[ROW_STATE_KEY] = STATE_TARGET
            row[ROW_SELECTABLE_KEY] = true
        elif destination > index:
            row[ROW_STATE_KEY] = STATE_PATH
            row[ROW_SELECTABLE_KEY] = true
        else:
            row[ROW_STATE_KEY] = STATE_OPEN
            row[ROW_SELECTABLE_KEY] = true
        rows.append(row)
    return rows

## **IS THERE ANY DESTINATION TO OFFER ON THIS SOURCE?** — the one test a caller asks before opening a
## track, so an empty card is never floated. It is deliberately NOT `next_rung_ready`: that answer is
## the highest UNGATED rung and the track is worth opening for a locked one too, which is the whole of
## why a barred rung stays visible.
static func has_track(rows: Array[Dictionary]) -> bool:
    for row in rows:
        var state := String(row.get(ROW_STATE_KEY, ""))
        if state != STATE_BANKED and state != STATE_STANDING:
            return true
    return false

## **THE TRACK AS CONTROLS** — one row per rung, a `Button` where the rung may be picked and a `Label`
## where it may not.
##
## **THE SHAPE IS THE STATEMENT**, this client's standing rule for the improvement control: a button
## is a CHOICE, and a banked rung, the rung you stand on and an unmet prerequisite are all FACTS. A
## greyed button on a locked rung would offer an act the sim refuses, one press away from a job that
## queues and then blocks.
##
## `on_pick` takes the row's improvement VERB — the destination, which is what the command carries.
static func build_track(rows: Array[Dictionary], on_pick: Callable) -> VBoxContainer:
    var column := VBoxContainer.new()
    column.add_theme_constant_override("separation", HudWorkVocab.RUNG_TRACK_ROW_SEPARATION)
    column.custom_minimum_size = Vector2(HudWorkVocab.RUNG_TRACK_WIDTH, 0.0)
    var title := Label.new()
    title.text = HudWorkVocab.RUNG_TRACK_TITLE
    title.add_theme_color_override("font_color", HudStyle.INK_DIM)
    title.add_theme_font_size_override("font_size", HudWorkVocab.RUNG_TRACK_TITLE_FONT_SIZE)
    column.add_child(title)
    for row in rows:
        column.add_child(_build_row(row, on_pick))
        for reason in (row.get(ROW_REASONS_KEY, []) as Array):
            column.add_child(_build_reason(String(reason)))
    return column

## One rung's line: its name on the left, its state or its price on the right.
static func _build_row(row: Dictionary, on_pick: Callable) -> Control:
    var state := String(row.get(ROW_STATE_KEY, ""))
    var selectable := bool(row.get(ROW_SELECTABLE_KEY, false)) and on_pick.is_valid()
    var line := HBoxContainer.new()
    line.set_meta(HudWorkVocab.RUNG_TRACK_ROW_META, String(row.get(ROW_IMPROVEMENT_KEY, "")))
    line.set_meta(HudWorkVocab.RUNG_TRACK_STATE_META, state)
    var tint := _state_color(state)
    var name := Label.new()
    name.text = String(row.get(ROW_NAME_KEY, ""))
    name.clip_text = true
    name.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    name.custom_minimum_size = Vector2(HudWorkVocab.RUNG_TRACK_NAME_WIDTH, 0.0)
    name.add_theme_color_override("font_color", tint)
    name.add_theme_font_size_override("font_size", HudWorkVocab.RUNG_TRACK_ROW_FONT_SIZE)
    name.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(name)
    var face := _row_face(row, state)
    if not selectable:
        var value := Label.new()
        value.text = face
        value.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        value.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
        value.add_theme_color_override("font_color", tint)
        value.add_theme_font_size_override("font_size", HudWorkVocab.RUNG_TRACK_ROW_FONT_SIZE)
        value.mouse_filter = Control.MOUSE_FILTER_IGNORE
        line.add_child(value)
        return line
    var pick := Button.new()
    pick.text = face
    pick.focus_mode = Control.FOCUS_NONE
    pick.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(pick, "ghost")
    # The board's own row treatment — the default button chrome is nearly two lines tall, which on a
    # three-rung track is half the card.
    HudWidgets.compact(pick, HudWorkVocab.RUNG_TRACK_ROW_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
    # AFTER `apply_button`, which writes its own `font_color`: a rung already on the chosen path keeps
    # the cyan that says so, and the hover brightening stays as the affordance.
    pick.add_theme_color_override("font_color", tint)
    var verb := String(row.get(ROW_IMPROVEMENT_KEY, ""))
    pick.pressed.connect(func() -> void: on_pick.call(verb))
    line.add_child(pick)
    return line

## A locked rung's reason, beneath its own row and in the aside's quiet ink — the shape a gate reason
## takes everywhere else in this client.
static func _build_reason(text: String) -> Label:
    var label := Label.new()
    label.text = text
    label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    label.custom_minimum_size = Vector2(HudWorkVocab.RUNG_TRACK_WIDTH, 0.0)
    label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    label.add_theme_font_size_override("font_size", HudWorkVocab.RUNG_TRACK_REASON_FONT_SIZE)
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    return label

## **WHAT THE RIGHT-HAND SLOT SAYS.** A rung at or below where the source stands states its STATE and
## no figure at all — that is the receipt-not-discount property, rendered: a banked rung is not a
## discount to be quoted, it is a thing already bought.
static func _row_face(row: Dictionary, state: String) -> String:
    match state:
        STATE_BANKED: return HudWorkVocab.RUNG_TRACK_STATE_BANKED
        STATE_STANDING: return HudWorkVocab.RUNG_TRACK_STATE_STANDING
        STATE_LOCKED: return HudWorkVocab.RUNG_TRACK_STATE_LOCKED
    var work := float(row.get(ROW_WORK_KEY, WORK_UNKNOWN))
    if work <= WORK_UNKNOWN:
        # The wire prices no such job on this source. The state word is the whole of what is known.
        return _state_word(state)
    var turns := DetailFormat.build_turns_clause(int(row.get(ROW_TURNS_KEY,
        SourceForecast.BUILD_TURNS_NO_ESTIMATE)))
    if turns == "":
        return HudWorkVocab.RUNG_TRACK_COST_UNDATED_FORMAT % DetailFormat.format_work_units(work)
    return HudWorkVocab.RUNG_TRACK_COST_FORMAT % [DetailFormat.format_work_units(work), turns]

static func _state_word(state: String) -> String:
    match state:
        STATE_PATH: return HudWorkVocab.RUNG_TRACK_STATE_PATH
        STATE_TARGET: return HudWorkVocab.RUNG_TRACK_STATE_TARGET
    return ""

## The row's ink. `SIGNAL` is what this HUD spells a LIVE state in — the rung you stand on and the one
## you are heading for — and `SIGNAL_DEEP` is the one step down a rung under construction already
## wears on the work board, which is exactly what a leg on the way is.
static func _state_color(state: String) -> Color:
    match state:
        STATE_BANKED: return HudStyle.INK_FAINT
        STATE_STANDING: return HudStyle.SIGNAL
        STATE_PATH: return HudStyle.SIGNAL_DEEP
        STATE_TARGET: return HudStyle.SIGNAL
        STATE_LOCKED: return HudStyle.INK_DIM
    return HudStyle.INK

## **WHERE THE SOURCE STANDS — the HIGHEST banked rung, `0` (the wild floor) when none is.** Walked
## top-down for the reason `BandPanelController._work_source_rung` tests the higher rung first: a
## Field is ALSO cultivated, so a bottom-up walk would stop at the rung beneath the one it stands on.
##
## **`improvement_is_done` IS THE TEST, and it carries `FORECAST_RETIRED_BY_HIGHER_RUNG`** — a Field
## sown straight from wild ground reports `is_cultivated` false forever, and its Cultivate is still
## bought and paid for.
static func _standing_index(branch: Array, source: Dictionary, prefix: String) -> int:
    for index in range(branch.size() - 1, 0, -1):
        var verb := String(SourceForecast.RUNG_KEY_IMPROVEMENTS.get(String(branch[index]),
            SourceForecast.IMPROVEMENT_NONE))
        if verb != SourceForecast.IMPROVEMENT_NONE \
                and SourceForecast.improvement_is_done(source, prefix, verb):
            return index
    return 0

## Which rung of this branch an improvement VERB names — `-1` for none, which is what
## `IMPROVEMENT_NONE` answers and is why an unqueued source has no path and no target.
static func _index_of_improvement(branch: Array, improvement: String) -> int:
    if improvement == SourceForecast.IMPROVEMENT_NONE:
        return -1
    for index in range(branch.size()):
        if String(SourceForecast.RUNG_KEY_IMPROVEMENTS.get(String(branch[index]), "")) == improvement:
            return index
    return -1

## The queued entry's published legs, keyed by their rung so a row can ask for its own. `{}` on an
## unqueued source.
static func _legs_by_rung(source: Dictionary, prefix: String) -> Dictionary:
    var by_rung := {}
    for leg in SourceForecast.build_legs(source, prefix):
        by_rung[String(leg.get(SourceForecast.BUILD_LEG_RUNG_KEY, ""))] = leg
    return by_rung

## **WHAT THIS RUNG STILL OWES, FROM WHERE THE SOURCE STANDS.** The queued entry's own leg where it
## publishes one — that figure is the sim's and is what the whole feature reads — and otherwise the
## per-rung `workCost − workDone` pair the wire publishes for exactly this pre-commit question.
##
## `WORK_UNKNOWN` where the wire prices no such job on this source, which every face renders as no
## figure rather than as a free rung.
static func _work_remaining(source: Dictionary, prefix: String, improvement: String,
        legs: Dictionary, rung_key: String) -> float:
    if legs.has(rung_key):
        return float((legs[rung_key] as Dictionary).get(SourceForecast.BUILD_LEG_WORK_KEY, 0.0))
    var cost := SourceForecast.build_work_cost(source, prefix, improvement)
    if cost <= SourceForecast.BUILD_WORK_COST_NONE:
        return WORK_UNKNOWN
    return maxf(cost - SourceForecast.build_work_done(source, prefix, improvement), 0.0)

## The leg's CHAINED date, and `BUILD_TURNS_NO_ESTIMATE` where no entry covers this rung. **There is
## no pair to fall back on here**: a date is chained against a build queue the client cannot see, so
## an unqueued rung honestly has none.
static func _leg_turns(legs: Dictionary, rung_key: String) -> int:
    if not legs.has(rung_key):
        return SourceForecast.BUILD_TURNS_NO_ESTIMATE
    return int((legs[rung_key] as Dictionary).get(SourceForecast.BUILD_LEG_TURNS_KEY,
        SourceForecast.BUILD_TURNS_NO_ESTIMATE))

## The rung's own NAME — `DetailFormat.rung_badge_word`'s word behind the glyph the work row's rung
## mark already draws, so the track and the board name one rung one way.
static func _rung_name(improvement: String) -> String:
    if improvement == SourceForecast.IMPROVEMENT_NONE:
        return HudWorkVocab.RUNG_TRACK_WILD_NAME
    return HudWorkVocab.RUNG_TRACK_NAME_FORMAT % [
        FoodIcons.for_policy(improvement), DetailFormat.rung_badge_word(improvement)]

## The KNOWLEDGE and SOURCE gates, through `RungGates` — the ONE definition, so the track, the `⌃`
## mark, the map badge and the compose sheet cannot disagree about what is climbable.
static func _gates(kind: String, source: Dictionary, prefix: String,
        knowledge: Dictionary) -> Dictionary:
    if kind == SourceForecast.LABOR_KIND_HUNT:
        return RungGates.hunt_gates(source, knowledge)
    return RungGates.forage_gates_from_patch(source, knowledge) \
        if prefix == HudComposeVocab.BARE_FORECAST_PREFIX else RungGates.forage_gates(source, knowledge)

## **EVERY REASON THIS RUNG IS REFUSED — `[]` when it is not.** Two kinds, and the ORDER is the point:
## an OUTRIGHT bar comes first, because *no aurochs will ever be penned* supersedes *your people know
## Penning 45%* — quoting the knowledge on a species that can never use it sends the player to earn a
## craft that buys them nothing here.
static func _rung_refusals(kind: String, source: Dictionary, prefix: String, improvement: String,
        gates: Dictionary) -> Array[String]:
    var outright := _outright_bar(kind, source, prefix, improvement)
    if outright != "":
        return [outright] as Array[String]
    return RungGates.gate_reasons_for(gates, improvement)

## **WHAT THE SPECIES OR THE GROUND WILL NEVER ADMIT** — `""` when the rung is admitted at all.
##
## `RungGates.hunt_rungs_admitted` / `_any_crop_allows` WITHHOLD such a rung, which is right for a
## MARK: a mark promises the verb is available. A TRACK says what the branch holds, so the same answer
## has to arrive as a visible refusal instead — see `HudFloraVocab.GATE_REASON_SPECIES_CEILING_FORMAT`.
static func _outright_bar(kind: String, source: Dictionary, prefix: String,
        improvement: String) -> String:
    if kind == SourceForecast.LABOR_KIND_HUNT:
        # **ONLY THE PEN RUNG HAS A CEILING TO CLEAR.** `Tame` is admitted by anything above `wild`,
        # and a `wild`-ceiling herd's whole branch above the floor is barred — which the Corral row
        # states, the Tame row states through the same test, and neither invents a second wording for.
        var ceiling := SourceForecast.husbandry_ceiling(source)
        if improvement == SourceForecast.IMPROVEMENT_CORRAL:
            return "" if ceiling == SourceForecast.HUSBANDRY_CEILING_PEN \
                else HudFloraVocab.GATE_REASON_SPECIES_NEVER_PENNED
        if improvement == SourceForecast.IMPROVEMENT_TAME \
                and ceiling == SourceForecast.HUSBANDRY_CEILING_WILD:
            return HudFloraVocab.GATE_REASON_SPECIES_NEVER_TAMED
        return ""
    var flag := String(RungGates.CROP_LEGALITY_FLAGS.get(improvement, ""))
    if flag == "" or RungGates.any_crop_allows(source, prefix, flag):
        return ""
    return HudFloraVocab.GATE_REASON_CROP_CANNOT_CLIMB_FORMAT \
        % DetailFormat.rung_badge_word(improvement)
