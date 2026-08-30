class_name RungGates
extends RefCounted

## **All-`static`, stateless** shared RUNG-GATE layer — the one answer to "may this source climb
## its next rung, and if not, why not?", and (since `wild_fodder_reason`) to the sibling question
## "…and will the work it is already doing actually pay out?". Same shape of answer — what is
## missing, its live progress, the remedy — asked of the same faction knowledge.
##
## **WHY ITS OWN FILE.** These three functions were private to `DrawerComposeController`, which is
## correct while the compose sheet is the only surface asking. It is not: the Band panel's WORK
## board marks a source that can climb, and the MAP marks it on the source's own marker — and a
## renderer must not depend on the HUD's compose controller. Extracting the shared layer BEFORE the
## consumers is the same measurement that produced `SourceForecast` and `HudWidgets`
## (`.claude/rules/client/hud-modules.md` → "Extract shared layers BEFORE controllers"). One
## definition, so the sheet, the board and the map can never disagree about what is climbable.
##
## **STATELESS IS THE INVARIANT** — no node, no `_hud`, no snapshot cache. The one piece of HUD
## state the gates need, **faction knowledge**, is threaded in as a `knowledge` PARAMETER: a
## `{track_key: float}` dict the caller reads off `FactionReadouts.faction_knowledge` (the compose
## sheet through its `_topbar`, the board through the HUD's). Reaching for it here would weld a
## pure layer to the top-bar controller and give the map no way in at all.

## Unmet prerequisites for the FORAGE investment rungs (Cultivate = rung 2, Sow = rung 3), keyed
## policy → Array[String] of reasons (each already carrying its own remedy). Empty when every rung is
## available. Mirrors the sim's `assign_labor` validation.
##
## The two rungs gate on DIFFERENT things, which is the ladder made legible:
##   • Cultivate — Cultivation knowledge, and nothing else.
##   • Sow — Seed Selection knowledge + ground that will take seed. What it needs beyond the craft is
##     the LAND: `patch_sow_site_refusal` is the sim's verdict on this ground, and it is the only gate
##     reason on either web that the player answers by MOVING rather than by working.
##
## **NO RUNG ON EITHER WEB CARRIES A HEALTH GATE** (docs/plan_harvest_floor.md §3.2). The sim replaced
## that cliff with a build the ecology phase does not enter at all: `build_supply` is the builders'
## own output and reads no phase and no floor, so a Stressed source builds at the same rate a Thriving
## one does. A phase term here would refuse a command the sim accepts, which is the defect class this
## file exists to keep out of the client.
##
## **THE FLOOR IS NOT A PACE EITHER, AND ITS ONE EFFECT ON A BUILD RUNS THE OTHER WAY.** A HIGHER
## floor empties the escapement room `max(0, B − floor·K)` sooner, and an empty room is what closes
## the `eligible` gate on the two rungs that carry it — `plant:tended` and `animal:pastoral`. That is
## a GATE and it lives with the gate: `SourceForecast.BUILD_WORK_PREDICATE_IMPROVEMENTS` names the two
## rungs and `build_turns_at` is where the room is tested. `learn_multiplier` paces the KNOWLEDGE
## accrual alone (`SourceForecast.TEACHING_RATE_FLOOR_TAIL`), which is a different meter.
##
## `tile_info` is the `patch_`-PREFIXED tile cross-ref, not the bare wire patch dict —
## `forage_gates_from_patch` below is the bare-keyed twin.
static func forage_gates(tile_info: Dictionary, knowledge: Dictionary) -> Dictionary:
    # The FOOD-PEAK glyph leads every knowledge remedy: practice scales with the floor
    # (`intensification::learn_multiplier` = floor / the food peak), so the peak is the reference the
    # remedy's "the more you leave standing" is measured against.
    var sustain_icon := FoodIcons.for_floor_zone(SourceForecast.FLOOR_ZONE_PEAK)
    var gates := {}
    var cultivate_reasons: Array[String] = []
    var cultivation := track(knowledge, HudFloraVocab.KNOWLEDGE_TRACK_CULTIVATION)
    if cultivation < HudConst.KNOWLEDGE_COMPLETE:
        cultivate_reasons.append(HudFloraVocab.GATE_REASON_CULTIVATION_KNOWLEDGE_FORMAT % [
            HudFormat.progress_percent(cultivation), sustain_icon])
    if not cultivate_reasons.is_empty():
        gates[HudConst.LABOR_POLICY_CULTIVATE] = cultivate_reasons
    var sow_reasons: Array[String] = []
    var seed_selection := track(knowledge, HudFloraVocab.KNOWLEDGE_TRACK_SEED_SELECTION)
    if seed_selection < HudConst.KNOWLEDGE_COMPLETE:
        sow_reasons.append(HudFloraVocab.GATE_REASON_SEED_SELECTION_KNOWLEDGE_FORMAT % [
            HudFormat.progress_percent(seed_selection), sustain_icon])
    var refusal := sow_site_refusal_reason(tile_info)
    if refusal != "":
        sow_reasons.append(refusal)
    if not sow_reasons.is_empty():
        gates[HudConst.LABOR_POLICY_SOW] = sow_reasons
    return gates

## WHY this ground will not take seed, in the manual's voice — "" when it will. Reads the sim's
## `patch_sow_site_refusal` verdict; the client never re-derives it (it has neither the per-biome
## capacity table nor the hydrology). An unknown key still refuses: the sim gates the command on the
## same seam, so offering the button anyway would only produce a failure the player cannot read.
static func sow_site_refusal_reason(tile_info: Dictionary) -> String:
    var key := String(tile_info.get("patch_sow_site_refusal", "")).strip_edges()
    if key == "":
        return ""
    return String(HudFloraVocab.SOW_REFUSAL_REASONS.get(key, HudFloraVocab.SOW_REFUSAL_FALLBACK))

## Unmet prerequisites for the HUNT investment rungs (Tame = rung 2, Corral = rung 3), keyed policy →
## Array[String] of reasons. The herd twin of `forage_gates`.
##
## The §4.3 GATE RESHUFFLE is what this function encodes: ONE knowledge per transition. **Herding
## gates Tame** (it no longer gates Corral, and taming is no longer ungated), and the **new Penning
## gates Corral**. Corral additionally needs THIS herd tamed — the per-source half of the split.
##
## Deliberately NOT gated: the source being Thriving — and nothing on the control says otherwise
## either. The sim paces a build by the FLOOR rather than stopping it on the phase (see
## `forage_gates` above), so the WARN "Paused … only advances while Thriving" line the running control
## used to carry is retired with the gate it described; what the sheet states instead is the live pace
## in the aside's teaching line.
##
## Known gap (pre-existing): no ownership check — the sim's tracks are per-faction, so a herd tamed by
## ANOTHER faction reads as available here while the sim rejects the assign.
static func hunt_gates(herd: Dictionary, knowledge: Dictionary) -> Dictionary:
    # The FOOD-PEAK glyph leads every knowledge remedy: practice scales with the floor
    # (`intensification::learn_multiplier` = floor / the food peak), so the peak is the reference the
    # remedy's "the more you leave standing" is measured against.
    var sustain_icon := FoodIcons.for_floor_zone(SourceForecast.FLOOR_ZONE_PEAK)
    var gates := {}
    var domestication := float(herd.get("domestication", 0.0))
    var tame_reasons: Array[String] = []
    var herding := track(knowledge, HudFloraVocab.KNOWLEDGE_TRACK_HERDING)
    if herding < HudConst.KNOWLEDGE_COMPLETE:
        tame_reasons.append(HudFloraVocab.GATE_REASON_HERDING_KNOWLEDGE_FORMAT % [
            HudFormat.progress_percent(herding), sustain_icon])
    if not tame_reasons.is_empty():
        gates[HudConst.LABOR_POLICY_TAME] = tame_reasons
    var corral_reasons: Array[String] = []
    var penning := track(knowledge, HudFloraVocab.KNOWLEDGE_TRACK_PENNING)
    if penning < HudConst.KNOWLEDGE_COMPLETE:
        corral_reasons.append(HudFloraVocab.GATE_REASON_PENNING_KNOWLEDGE_FORMAT % [
            HudFormat.progress_percent(penning), sustain_icon])
    if domestication < SourceForecast.DOMESTICATION_COMPLETE:
        corral_reasons.append(HudFloraVocab.GATE_REASON_HERD_DOMESTICATED_FORMAT % [
            HudFormat.progress_percent(domestication), FoodIcons.for_policy(HudConst.LABOR_POLICY_TAME)])
    if not corral_reasons.is_empty():
        gates[SourceForecast.IMPROVEMENT_CORRAL] = corral_reasons
    return gates


## **THE ROUTE ARM — may this ROAD be raised to each rung above the one it stands on, and if not,
## why not?** (arc #532 slice 13). Keyed **RUNG KEY → `Array[String]` of reasons**, empty for a rung
## that is ready to order.

## ⛔ **KEYED ON THE RUNG AND NOT ON THE VERB, WHICH IS THE ONE PLACE THIS BRANCH DIFFERS FROM THE
## OTHER TWO.** A route rung may carry NO verb at all — the free floor is worn in by traffic and
## nothing declares it — so a verb-keyed table would have two entries spelling `""` and could not tell
## `path` from `trail`. The plant and animal webs have no such rung, which is why their gates are
## keyed the way they are and why widening those was never an option.
##
## ⛔ **IT ANSWERS FOR EVERY RUNG ABOVE THE STANDING ONE, GATED OR NOT — the absence of a key is the
## READY answer.** The ladder sheet shows the whole branch, so a refused rung is rendered and
## explained rather than withheld; a mark would want the opposite, and this branch has no mark.
##
## Four gates, in the order a player reads them, and the FIRST is the one that supersedes:
##   1. **THE GROUND** — `requires_rung`. A dirt road wants a trail beneath it and a trail is worn in
##      only by traffic, so **a road cannot be built on bare ground**. Stated first because telling
##      somebody to learn Paving while they stand on a path is a remedy for the wrong thing.
##   2. **NOBODY DECLARES IT** — `verb == ""`. Not a refusal for want of anything; there is simply no
##      order to give, and the row says so rather than looking broken.
##   3. **THE CRAFT** — `unlock_knowledge`, at the same `KNOWLEDGE_COMPLETE` bar every other track is
##      read at. **The craft's NAME comes from the ladder's knowledge roster** (`labels`), never from
##      a table here, so a rung added to `intensification_ladder.json` names its own unlock.
##   4. **THE KEEPER** — a band to name. `grade`/`pave` carry a band token that IS the keeper, and
##      `Main.IMPROVEMENT_NO_BAND` refuses the command outright rather than guessing one, so the row
##      states it before the press instead of failing silently after it.
##
## `ladder` is `HudRouteVocab.route_ladder`'s ordered catalog, `road` the raw `routes` row for the
## tile, `labels` a `{knowledge_id: display_name}` lookup off the ladder's knowledge roster, and
## `band` the acting band — `{}` meaning none is picked, which is gate 4.
static func route_gates(road: Dictionary, ladder: Array[Dictionary], knowledge: Dictionary,
        labels: Dictionary, band: Dictionary) -> Dictionary:
    var gates := {}
    var standing_key := HudRouteVocab.rung_of(road)
    var standing_order := HudRouteVocab.ladder_order_of(ladder, standing_key)
    var standing_name := HudRouteVocab.ladder_rung_name(ladder, standing_key)
    for entry in ladder:
        if HudRouteVocab.catalog_order(entry) <= standing_order:
            continue
        var reasons: Array[String] = []
        var requires := HudRouteVocab.catalog_requires_rung(entry)
        if requires != HudRouteVocab.RUNG_CATALOG_NONE \
                and HudRouteVocab.ladder_order_of(ladder, requires) > standing_order:
            reasons.append(HudRouteVocab.GATE_REASON_ROAD_NEEDS_RUNG_FORMAT % [
                HudRouteVocab.ladder_rung_name(ladder, requires).to_lower(),
                standing_name.to_lower()])
        var verb := HudRouteVocab.catalog_verb(entry)
        if verb == HudRouteVocab.RUNG_CATALOG_NONE:
            reasons.append(HudRouteVocab.GATE_REASON_ROAD_WORN_IN)
            gates[HudRouteVocab.catalog_rung_key(entry)] = reasons
            continue
        var unlock := HudRouteVocab.catalog_unlock_knowledge(entry)
        if unlock != HudRouteVocab.RUNG_CATALOG_NONE \
                and track(knowledge, unlock) < HudConst.KNOWLEDGE_COMPLETE:
            # ⛔ **THE REMEDY IS THE RUNG THAT TEACHES THE CRAFT, LOOKED UP — never the rung
            # beneath the gated one.** The two coincide on the four rungs that ship, which is
            # exactly why reading `requires_rung` here looked correct; the config is free to break
            # the pairing, and this is the one gate whose wrong answer sends a player to stand on
            # the wrong ground. `""` where nothing on this branch teaches it, which the composer
            # states by dropping the remedy rather than by naming a rung that does not.
            reasons.append(route_knowledge_reason(unlock, track(knowledge, unlock), labels,
                HudRouteVocab.ladder_rung_teaching(ladder, unlock)))
        # **THE KEEPER IS LAST, because it is the one gate the player closes without leaving the
        # card** — every reason above it is a thing to go and do, and this one is a click.
        if band.is_empty():
            reasons.append(HudRouteVocab.GATE_REASON_ROAD_NO_KEEPER)
        if not reasons.is_empty():
            gates[HudRouteVocab.catalog_rung_key(entry)] = reasons
    return gates

## One route knowledge gate's sentence — **a HEAD saying what is missing, plus a REMEDY saying what to
## do about it, appended only when there is one.**
##
## **The craft is named by the ladder's own roster** — the sim resolves every discovery's
## `display_name`, and a client table beside it is how the knowledge screen and a gate reason come to
## call one craft two things. A roster that does not carry the id yet states the PROGRESS without
## inventing a name, which is the honest half rather than a silent blank.
##
## ⛔ **`teaches` IS THE RUNG WHOSE `earns_knowledge` NAMES THIS CRAFT, AND IT IS LOOKED UP.** It was
## the rung directly BENEATH the gated one for a slice, read off `requires_rung` — which produces
## byte-identical sentences on the shipped four rungs and the wrong rung the moment a config has a
## rung teach a craft that opens something further up. A gate reason is a REMEDY: naming the wrong
## rung there tells the player to go and do the wrong thing, which is the one place this class of
## inference actually costs something.
##
## `""` — no rung on this branch teaches it — drops the remedy clause entirely. That is a real state
## (a route rung may be gated on a craft another branch earns), and a dangling *"keep a  carrying
## traffic"* would be worse than a head that simply states what is missing.
static func route_knowledge_reason(unlock: String, progress: float, labels: Dictionary,
        teaches: String) -> String:
    var name := String(labels.get(unlock, "")).strip_edges()
    var percent := HudFormat.progress_percent(progress)
    var head := HudRouteVocab.GATE_REASON_ROAD_KNOWLEDGE_HEAD_UNNAMED_FORMAT % percent \
        if name == "" else HudRouteVocab.GATE_REASON_ROAD_KNOWLEDGE_HEAD_FORMAT % [name, percent]
    if teaches.strip_edges() == "":
        return head
    return head + HudRouteVocab.GATE_REASON_ROAD_KNOWLEDGE_REMEDY_FORMAT % teaches.to_lower()
## **WILL THE HAY THIS CREW GATHERS ACTUALLY BE BANKED?** — `""` when it will, the reason when it will
## not. The plant twin in shape of the rung gates above, and a deliberate BROADENING of this file's
## remit: from "may this source climb its next rung" to "…and will the work it is doing actually pay
## out". Same kind of answer (what is missing, how far along, and the remedy), same statelessness, so
## it belongs beside them rather than in a second gate layer.
##
## The sim credits a wild patch's fodder take only to a faction that has learned **Foddering**, or on a
## patch already COMMITTED to a crop — committing IS the bid, so the crop's hay is paid unconditionally
## (`systems/labor.rs`: `patch.species.is_some() || knows(faction, FODDERING)`). Foddering is earned by
## KEEPING A PENNED HERD, so a pre-pastoral band structurally cannot have it: the meadow publishes a
## real `fodder_per_biomass` and the band banks none of it.
##
## **It takes the committed-species STRING, not the patch dict, deliberately.** Every caller has
## already read that key, and the `patch_`-prefixed-vs-bare trap this file documents is not worth
## re-entering for one lookup. **And it must be the PUBLISHED commitment** (`patch_committed_species`),
## never the composed improvement: a Cultivate the player has ticked but not committed is not a bid the
## sim has accepted, and quoting it would unlock a credit that is still being refused.
##
## Partial progress refuses the credit exactly like every other track — this is a 0..1 learning meter,
## and only `>= KNOWLEDGE_COMPLETE` is "known".
static func wild_fodder_reason(committed_species: String, knowledge: Dictionary) -> String:
    if committed_species.strip_edges() != "":
        return ""
    var foddering := track(knowledge, HudFloraVocab.KNOWLEDGE_TRACK_FODDERING)
    if foddering >= HudConst.KNOWLEDGE_COMPLETE:
        return ""
    # TWO remedies, both real and both reachable from where the player is standing: learn the craft by
    # keeping a pen (the corral rung's glyph), or commit this patch (the cultivate rung's glyph, whose
    # control is directly below this line on the same sheet).
    return HudFloraVocab.GATE_REASON_WILD_FODDER_FORMAT % [
        HudFormat.progress_percent(foddering),
        FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CORRAL),
        FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CULTIVATE)]

## The BARE-KEYED twin of `forage_gates`, for the raw wire patch dict (`forage_patch_lookup`) rather
## than the `patch_`-prefixed `tile_info` cross-ref.
##
## **The cross-ref's prefixing is UNIFORM now** (issue #442): `patch_is_cultivated` /
## `patch_cultivation_progress` joined their already-prefixed rung-3 twins when
## `SourceForecast.improvement_is_done` started spelling every key as `prefix + name` and the lone
## exception would have made it answer "not built" on a tended patch. So this adapter is a plain
## re-spelling of the one key `forage_gates` still reads, and there is no longer a mixed convention
## to write down.
static func forage_gates_from_patch(patch: Dictionary, knowledge: Dictionary) -> Dictionary:
    return forage_gates({
        "patch_sow_site_refusal": String(patch.get("sow_site_refusal", "")),
    }, knowledge)

## THE READY TEST — the next rung this source could climb RIGHT NOW, as
## `{policy, glyph}`, or `{}` when there is nothing to offer.
##
## It is not a new judgement: it is the test the compose sheet already runs to decide whether to grey a
## rung, plus the two "is this rung offered at all" passes that sit beside it there. Three conditions,
## all of which must hold (docs/plan_worked_source_marks.md §3):
##
##  1. **ADMITTED** — the species or the land admits the rung, and it is not already BUILT. Hunt: the
##     husbandry ceiling ("wild" / "pastoral" / "pen"). Forage: at least one composition entry that
##     `can_cultivate` / `can_sow`, the species-global legality flag.
##  2. **UNGATED** — `forage_gates` / `hunt_gates` return no reason for it (knowledge complete, the
##     per-source prerequisite met, the ground willing).
##  3. **NOT ALREADY RUNNING** — the source's `improvement` is not that verb. A patch mid-Cultivate is
##     progress, not an opportunity, and marking it would never clear.
##
## `improvement` is the SECOND AXIS (issue #442), not the harvest stance — `""` for a crew building
## nothing. It was the `policy` field while a build verb was a value of it.
static func next_rung_ready(kind: String, source: Dictionary, improvement: String,
        knowledge: Dictionary, prefix: String = HudComposeVocab.BARE_FORECAST_PREFIX) -> Dictionary:
    return _next_rung(kind, source, prefix, improvement, knowledge, false)

## THE OFFER TEST — the ONE improvement the compose sheet's control puts in front of the player, as
## `{policy, glyph, reasons}`; `reasons` is empty when the rung is ready to start and carries its unmet
## prerequisites when it is not.
##
## The difference from `next_rung_ready` is condition 2 alone, and it is the difference between a MARK
## and a CONTROL. A mark promises the verb is available, so a gated rung must not wear one. The control
## is how the player DISCOVERS the rung exists and what it costs to unlock, so a gated improvement is
## **shown, unchecked and explained**, exactly as a gated policy rung has always been.
##
## **THE ORDERING LIVES HERE, ONCE, and both answers read it** (`_next_rung`): highest rung first among
## those that are ready, falling back to the LOWEST admitted-but-gated rung when none is. Highest-first
## is what makes sowable wild ground offer `Sow` rather than `Cultivate` — both clear their gates
## there, and answering with the lower rung would erase the distinction. Lowest-first for the gated
## fallback is the mirror of the same reasoning: if you can start nothing, the useful thing to name is
## the NEAREST rung you could work toward, not the furthest.
static func next_rung_offered(kind: String, source: Dictionary, improvement: String,
        knowledge: Dictionary, prefix: String = HudComposeVocab.BARE_FORECAST_PREFIX) -> Dictionary:
    return _next_rung(kind, source, prefix, improvement, knowledge, true)

## The shared body of the two answers above. `allow_gated` is the whole difference between them.
static func _next_rung(kind: String, source: Dictionary, prefix: String, improvement: String,
        knowledge: Dictionary, allow_gated: bool) -> Dictionary:
    var current := improvement.strip_edges().to_lower()
    var gates := {}
    var admitted: Array[String] = []
    if kind == SourceForecast.LABOR_KIND_FORAGE:
        gates = forage_gates(source, knowledge) if prefix == HudComposeVocab.FORAGE_FORECAST_PREFIX \
            else forage_gates_from_patch(source, knowledge)
        # HIGHEST RUNG FIRST. `can_sow` / `can_cultivate` are SPECIES-GLOBAL legality ("can this plant
        # ever climb this rung"), never "is it wise here" — a marginal share must not suppress a rung.
        for rung in [SourceForecast.IMPROVEMENT_SOW, SourceForecast.IMPROVEMENT_CULTIVATE]:
            if rung != current and rung_has_room(source, prefix, rung) \
                    and any_crop_allows(source, prefix, CROP_LEGALITY_FLAGS[rung]):
                admitted.append(rung)
    elif kind == SourceForecast.LABOR_KIND_HUNT:
        gates = hunt_gates(source, knowledge)
        admitted = hunt_rungs_admitted(source, prefix, current)
    for rung in admitted:
        if not gates.has(rung):
            return _ready(rung)
    if allow_gated and not admitted.is_empty():
        var nearest: String = admitted[admitted.size() - 1]
        var answer := _ready(nearest)
        answer["reasons"] = gate_reasons_for(gates, nearest)
        return answer
    return {}

## **WHICH ANIMAL RUNGS THIS HERD COULD STILL CLIMB — highest first, knowledge NOT considered.**
##
## The husbandry CEILING says how far up the ladder this SPECIES can climb, and a rung above it is
## withheld OUTRIGHT rather than gated: no amount of knowledge or work will ever pen an aurochs whose
## ceiling is `"pastoral"`, so offering it gated would imply a reachable prerequisite. A rung already
## *finished* is likewise not something left to climb.
##
## **`exclude` drops the rung a crew is already building**, which is what `_next_rung` wants — a herd
## mid-Tame is progress, not an opportunity. Pass `""` to ask the plain question *"is there any rung
## on this herd at all"*, which is what the KIT OFFER test wants: gear that speeds a build helps the
## build **currently running** most of all, so excluding it there would withhold the handling kit on
## exactly the herd it is doing its work on.
##
## **Extracted so the rung picker and `KitRoster.kit_offer` share ONE definition of "a rung remains".**
## Two copies of a ceiling comparison is how the picker comes to offer a Corral the kit list has
## already decided is impossible, or the reverse.
static func hunt_rungs_admitted(source: Dictionary, prefix: String,
        exclude: String = "") -> Array[String]:
    var admitted: Array[String] = []
    var ceiling := SourceForecast.husbandry_ceiling(source)
    for rung in [SourceForecast.IMPROVEMENT_CORRAL, SourceForecast.IMPROVEMENT_TAME]:
        var admits := ceiling == SourceForecast.HUSBANDRY_CEILING_PEN \
            if rung == SourceForecast.IMPROVEMENT_CORRAL \
            else ceiling != SourceForecast.HUSBANDRY_CEILING_WILD
        if rung != exclude and admits and rung_has_room(source, prefix, rung):
            admitted.append(rung)
    return admitted

## **IS THERE WORK LEFT TO PUT INTO THIS RUNG?** — the admission test both webs' rung walks share.
##
## ⛔ **IT IS THE SAME QUESTION THE DESTINATION TRACK ASKS, AND THAT IDENTITY IS THE POINT.**
## `RungLadder.track` marks every rung at or below the standing one BANKED or STANDING off this same
## `improvement_is_done`, and `RungLadder.has_track` is *is there a row above those*. So a rung
## admitted here is a rung the track will offer, and a `⌃` can never be drawn over a card with no
## rungs left — the dead control reported from play, where a finished Field wore an offer mark whose
## press did nothing at all.
##
## **IT USED TO BE `not improvement_is_done(...) or rung_needs_repair(...)`**, a second reading of
## *is this rung done* laid beside the first: the flag-and-meter pair versus the published standing.
## They could disagree by one `f32` ULP, and when they did this answered TRUE for a rung the track had
## already banked. That test is RETIRED (`SourceForecast`, beside `improvement_progress`) — the meter
## is a publication of the standing now, so *achieved* and *full* are one fact and there is nothing
## left for a second term to add.
##
## **A RUNG THE SOURCE STANDS ON OR ABOVE ANSWERS FALSE, which is the half that must not move.** It
## has nothing to put work into, and offering it would put a `⌃` on every finished improvement in
## the game.
static func rung_has_room(source: Dictionary, prefix: String, rung: String) -> bool:
    return not SourceForecast.improvement_is_done(source, prefix, rung)

## **HAS THIS HERD ANY RUNG LEFT TO CLIMB?** — the question a build-speeding kit's applicability turns
## on, asked through the same seam the picker admits rungs with. Knowledge-blind by construction, like
## every other term in `KitRoster.kit_offer`: what a kit CAN change on a source is a property of the
## pair, and a faction that has not learned Penning yet will learn it while still holding this herd.
static func hunt_rung_remains(source: Dictionary, prefix: String) -> bool:
    return not hunt_rungs_admitted(source, prefix).is_empty()

## The species-GLOBAL legality flag each plant rung reads off a composition entry — the one place that
## mapping is written down.
const CROP_LEGALITY_FLAGS := {
    SourceForecast.IMPROVEMENT_SOW: "can_sow",
    SourceForecast.IMPROVEMENT_CULTIVATE: "can_cultivate",
}

## The unmet-prerequisite reasons a gates dict holds for one rung, as a typed `Array[String]` — the
## shape `HudWidgets.build_improvement_control` renders beneath the box.
static func gate_reasons_for(gates: Dictionary, rung: String) -> Array[String]:
    var raw: Variant = gates.get(rung, null)
    var reasons: Array[String] = []
    if raw is Array:
        for reason in (raw as Array):
            reasons.append(String(reason))
    return reasons

## THE RUNG UNDER WAY — the twin of `next_rung_ready`, as `{policy, glyph, progress}` (progress 0..1),
## or `{}` when this source is not building anything.
##
## `next_rung_ready` deliberately answers `{}` for a source already building that verb: a patch
## mid-Cultivate is progress, not an opportunity. That reasoning is right and the CONSEQUENCE was
## wrong — it left the in-flight case with no mark at all, so a patch you are actively cultivating
## looked exactly like a patch nobody has touched, while the untouched one beside it advertised `⌃`.
## The two answers are one axis in two states, and the badge shows whichever applies.
##
## **KEYED ON THE METER, with `improvement` demoted to a pending DECLARATION**
## (`docs/plan_standing_upkeep.md` §2.4). It was keyed on the stored verb alone, on the reasoning that
## a half-built patch nobody works is not "in progress" — which stopped being the sim's model when the
## verb became derived: a completed rung that erodes back below its cost re-enters the building state
## with nothing declared, and the mark has to follow it or the one surface a player would notice the
## slide on goes quiet. `SourceForecast.build_verb` is the single derivation and it names the meter
## each verb fills, so this, the compose control and the work board quote one rung at one percent.
##
## The `kind` guard keeps the two webs' verbs apart: a herd has no `cultivation_progress`, and an
## improvement reaching the wrong web would answer a meter of 0 rather than nothing at all. It is a
## LABOR kind here (`LABOR_KIND_*`), and an unrecognised one answers `{}` rather than defaulting into
## a web.
static func rung_in_progress(kind: String, source: Dictionary, improvement: String) -> Dictionary:
    if kind != SourceForecast.LABOR_KIND_FORAGE and kind != SourceForecast.LABOR_KIND_HUNT:
        return {}
    var current := SourceForecast.build_verb(source, HudComposeVocab.BARE_FORECAST_PREFIX,
        SourceForecast.source_kind_for_labor(kind), improvement)
    if current == SourceForecast.IMPROVEMENT_NONE:
        return {}
    var answer := _ready(current)
    answer["progress"] = SourceForecast.improvement_progress(
        source, HudComposeVocab.BARE_FORECAST_PREFIX, current)
    return answer

## **THE SAME ANSWER RE-POINTED AT THE LEG IN FLIGHT** — `{policy, glyph, progress}` for the rung the
## crew is standing on RIGHT NOW, rather than for the destination the queue entry names.
##
## **A CLIMB IS ONE ENTRY AND SEVERAL RUNGS** (`docs/plan_standing_upkeep.md` §2.8). A `sow` ordered on
## untended ground clears the ground first, so `rung_in_progress` — which honours the declaration
## wherever the declared meter is at zero — answers `sow` at **0%** for as long as that clearing takes.
## Both numbers are correct and neither is the one the player is watching: reported from play as a
## Work tab reading `0%` in two places beside a tile card reading `18%`, for the same job, on the same
## turn. **A progress number must never sit at zero while work is going in.**
##
## **THE DESTINATION IS NOT LOST, it moves to where it was already stated** — the queue row's TITLE
## names the job the player ordered and the date column is still the whole climb's. What this changes
## is the rung the PERCENTAGE and its glyph are about.
##
## **IT RE-POINTS AN ANSWER, IT DOES NOT PRODUCE ONE — `building` is `rung_in_progress`'s, ASKED
## rather than re-derived.** That is `SourceForecast.build_is_stalled`'s own discipline and it buys
## the same thing: the caller keeps the declared rung it started from (the queue entry's PRICE is the
## whole climb's and must not follow a leg), and no second resolution of the verb can drift from the
## first. A caller with `{}` gets `{}` — a source building nothing has no leg in flight either.
##
## A source with no published legs falls straight through, which is the honest answer for an eroded
## rung being repaired, for a source no band has queued, and for a fixture that states none.
static func leg_in_progress(source: Dictionary, building: Dictionary) -> Dictionary:
    var answer := building
    if answer.is_empty():
        return answer
    var leg := SourceForecast.build_leg_in_flight(source, HudComposeVocab.BARE_FORECAST_PREFIX)
    var rung := String(leg.get(SourceForecast.BUILD_LEG_IMPROVEMENT_KEY,
        SourceForecast.IMPROVEMENT_NONE))
    if rung == SourceForecast.IMPROVEMENT_NONE or rung == String(answer.get("policy", "")):
        return answer
    var out := _ready(rung)
    # The leg's OWN published fraction — the same `<rung>Progress` the tile card renders, which is why
    # the two surfaces agree by construction rather than by two careful derivations.
    out["progress"] = SourceForecast.improvement_progress(
        source, HudComposeVocab.BARE_FORECAST_PREFIX, rung)
    return out

## Whether ANY plant in this patch's composition may climb the rung `flag` names — species-GLOBAL
## legality ("can this plant ever climb this rung"), never "is it a wise crop here". `share` answers
## that other question, and a marginal share must never suppress the mark: a legal crop at 4% is still
## a rung the player can choose.
##
## An ABSENT composition answers **false**, which is the honest reading: the flags ride every
## `ForagePatchState`, so a patch without them is one the client cannot vouch for, and the mark exists
## to promise the verb is available.
##
## **PUBLIC because the DESTINATION TRACK asks it too** (`RungLadder._outright_bar`). A mark WITHHOLDS
## an inadmissible rung; a track SHOWS it and says why, so both surfaces have to reach the same
## legality flag — and a second reading of `composition` is how the picker comes to offer a Sow the
## board has already called impossible.
static func any_crop_allows(patch: Dictionary, prefix: String, flag: String) -> bool:
    var composition: Variant = patch.get(prefix + "composition", [])
    if not (composition is Array):
        return false
    for entry_variant in composition:
        if entry_variant is Dictionary and bool((entry_variant as Dictionary).get(flag, false)):
            return true
    return false

## The answer shape: the rung's policy key plus the glyph naming it. The CHEVRON that makes a mark read
## as "available" rather than "done" is the RENDERER's chrome, not part of this answer — the verb and
## standing-rung glyphs collide (▦ is both "Sow" and "this is a Field"), so a bare glyph must never be
## the whole message.
static func _ready(policy: String) -> Dictionary:
    return {"policy": policy, "glyph": FoodIcons.for_policy(policy)}

## One faction-knowledge track's 0..1 progress out of the caller's `knowledge` dict, 0.0 when absent.
## A missing track is "not learned", never "learned" — an absent key must gate, not open, or a
## snapshot that omits a track would silently unlock every rung it guards.
## **THE KNOWLEDGE TRACK EACH RUNG GATES ON** — one knowledge per transition (`§4.3`), so this is a
## map and not a search. It is what lets a caller tell a knowledge gate apart from a SOURCE gate
## without reading the reason's words.
const RUNG_KNOWLEDGE_TRACKS := {
    SourceForecast.IMPROVEMENT_CULTIVATE: HudFloraVocab.KNOWLEDGE_TRACK_CULTIVATION,
    SourceForecast.IMPROVEMENT_SOW: HudFloraVocab.KNOWLEDGE_TRACK_SEED_SELECTION,
    SourceForecast.IMPROVEMENT_TAME: HudFloraVocab.KNOWLEDGE_TRACK_HERDING,
    SourceForecast.IMPROVEMENT_CORRAL: HudFloraVocab.KNOWLEDGE_TRACK_PENNING,
    # **THE ROUTE BRANCH'S TWO** (arc #532). They are in this table for the same reason the four above
    # are, and for one more: `KnowledgeRoster` INVERTS it to ask "what does this knowledge unlock",
    # which is how the knowledge screen tells a discovery nothing is using from one there is nothing
    # to use. A route rung missing here would read as a knowledge that unlocks nothing at all.
    SourceForecast.IMPROVEMENT_GRADE: HudFloraVocab.KNOWLEDGE_TRACK_ROADBUILDING,
    SourceForecast.IMPROVEMENT_PAVE: HudFloraVocab.KNOWLEDGE_TRACK_PAVING,
}

## **Is this rung blocked on KNOWLEDGE specifically?** — the same `track < KNOWLEDGE_COMPLETE` test the
## gate builders above make, asked on its own so a caller can drop the knowledge reason without
## matching its text. The gate builders append the knowledge reason FIRST, so when this answers `true`
## the reason to drop is `reasons[0]`.
##
## Its one caller is the compose sheet, where that reason is BOTH redundant and vacuous: the aside
## states the same lesson live and quantified, and the remedy ("forage a wild patch to learn it") names
## the very work the sheet is composing. Every other surface keeps it — see `labor-ui.md`.
static func knowledge_gate_unmet(rung: String, knowledge: Dictionary) -> bool:
    var key := String(RUNG_KNOWLEDGE_TRACKS.get(rung, ""))
    return key != "" and track(knowledge, key) < HudConst.KNOWLEDGE_COMPLETE

static func track(knowledge: Dictionary, key: String) -> float:
    return float(knowledge.get(key, 0.0))
