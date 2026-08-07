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
##   • Cultivate — Cultivation knowledge + a Thriving patch (you improve what is already there).
##   • Sow — Seed Selection knowledge + ground that will take seed. It needs NO prior patch and no
##     Thriving gate (seed travels, and sown ground starts at the reseed floor — i.e. Collapsing — so
##     a health gate would forbid the very case the rung exists for). What it needs instead is the
##     LAND: `patch_sow_site_refusal` is the sim's verdict on this ground, and it is the only gate
##     reason on either web that the player answers by MOVING rather than by working.
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
    var phase := String(tile_info.get("patch_ecology_phase", "")).strip_edges().to_lower()
    if phase != HudFloraVocab.ECOLOGY_PHASE_THRIVING:
        var phase_label := phase.capitalize() if phase != "" else HudFloraVocab.GATE_PHASE_UNKNOWN_LABEL
        cultivate_reasons.append(HudFloraVocab.GATE_REASON_PATCH_THRIVING_FORMAT % phase_label)
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
## Deliberately NOT gated: the source being Thriving. Building on a source whose phase swings as it is
## worked would be un-actionable, so the sim just PAUSES the meter instead — see
## `DrawerComposeController._improvement_paused_note`, the WARN line the improvement control renders
## on BOTH webs, which is how the player is told rather than left to guess.
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
## re-spelling of the two keys `forage_gates` still reads, and there is no longer a mixed convention
## to write down.
static func forage_gates_from_patch(patch: Dictionary, knowledge: Dictionary) -> Dictionary:
    return forage_gates({
        "patch_ecology_phase": String(patch.get("ecology_phase", "")),
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
            if rung != current and not SourceForecast.improvement_is_done(source, prefix, rung) \
                    and _any_crop_allows(source, prefix, CROP_LEGALITY_FLAGS[rung]):
                admitted.append(rung)
    elif kind == SourceForecast.LABOR_KIND_HUNT:
        gates = hunt_gates(source, knowledge)
        # The husbandry CEILING says how far up the ladder this SPECIES can climb, and a rung above it
        # is withheld OUTRIGHT rather than gated: no amount of knowledge or work will ever pen an
        # aurochs whose ceiling is "pastoral", so offering it gated would imply a reachable prerequisite.
        var ceiling := SourceForecast.husbandry_ceiling(source)
        for rung in [SourceForecast.IMPROVEMENT_CORRAL, SourceForecast.IMPROVEMENT_TAME]:
            var admits := ceiling == SourceForecast.HUSBANDRY_CEILING_PEN \
                if rung == SourceForecast.IMPROVEMENT_CORRAL \
                else ceiling != SourceForecast.HUSBANDRY_CEILING_WILD
            if rung != current and admits \
                    and not SourceForecast.improvement_is_done(source, prefix, rung):
                admitted.append(rung)
    for rung in admitted:
        if not gates.has(rung):
            return _ready(rung)
    if allow_gated and not admitted.is_empty():
        var nearest: String = admitted[admitted.size() - 1]
        var answer := _ready(nearest)
        answer["reasons"] = gate_reasons_for(gates, nearest)
        return answer
    return {}

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
## Keyed on the IMPROVEMENT, not on a non-zero meter: a half-built patch nobody works is not "in
## progress", and its standing rung is what the rung glyph is for. `SourceForecast` names the meter
## each verb fills — one definition, so this and the compose control quote the same percent.
##
## The `kind` guard keeps the two webs' verbs apart: a herd has no `cultivation_progress`, and an
## improvement reaching the wrong web would answer a meter of 0 rather than nothing at all.
static func rung_in_progress(kind: String, source: Dictionary, improvement: String) -> Dictionary:
    var current := improvement.strip_edges().to_lower()
    var web: Array = SourceForecast.FORAGE_IMPROVEMENTS if kind == SourceForecast.LABOR_KIND_FORAGE \
        else SourceForecast.HUNT_IMPROVEMENTS if kind == SourceForecast.LABOR_KIND_HUNT \
        else []
    if not (current in web):
        return {}
    var answer := _ready(current)
    answer["progress"] = SourceForecast.improvement_progress(
        source, HudComposeVocab.BARE_FORECAST_PREFIX, current)
    return answer

## Whether ANY plant in this patch's composition may climb the rung `flag` names — species-GLOBAL
## legality ("can this plant ever climb this rung"), never "is it a wise crop here". `share` answers
## that other question, and a marginal share must never suppress the mark: a legal crop at 4% is still
## a rung the player can choose.
##
## An ABSENT composition answers **false**, which is the honest reading: the flags ride every
## `ForagePatchState`, so a patch without them is one the client cannot vouch for, and the mark exists
## to promise the verb is available.
static func _any_crop_allows(patch: Dictionary, prefix: String, flag: String) -> bool:
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
