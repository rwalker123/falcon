class_name RungGates
extends RefCounted

## **All-`static`, stateless** shared RUNG-GATE layer — the one answer to "may this source climb
## its next rung, and if not, why not?".
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
## `{track_key: float}` dict the caller reads off `TopBarReadouts.faction_knowledge` (the compose
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
    var sustain_icon := FoodIcons.for_policy(SourceForecast.LABOR_POLICY_SUSTAIN)
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
    # A finished patch retires Cultivate outright: the build is DONE (Sustain harvests it, and Sow is the
    # next rung if unlocked). This SUPERSEDES the prep prerequisites — a tended patch's Thriving/knowledge
    # gates are moot — so it replaces the reason list rather than piling on.
    if bool(tile_info.get("is_cultivated", false)):
        cultivate_reasons.clear()
        cultivate_reasons.append(HudFloraVocab.GATE_REASON_ALREADY_TENDED_FORMAT % sustain_icon)
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
    # A finished Field retires Sow, same as a finished patch retires Cultivate.
    if bool(tile_info.get("patch_is_field", false)):
        sow_reasons.clear()
        sow_reasons.append(HudFloraVocab.GATE_REASON_ALREADY_FIELD_FORMAT % sustain_icon)
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
## Deliberately NOT gated: the herd being Thriving. Taming a herd whose phase swings under hunting
## would be un-actionable, so the sim just PAUSES the meter instead — see
## `DrawerComposeController._tame_stalled_hint`, which is how the player is told rather than left to
## guess.
##
## Known gap (pre-existing): no ownership check — the sim's tracks are per-faction, so a herd tamed by
## ANOTHER faction reads as available here while the sim rejects the assign.
static func hunt_gates(herd: Dictionary, knowledge: Dictionary) -> Dictionary:
    var sustain_icon := FoodIcons.for_policy(SourceForecast.LABOR_POLICY_SUSTAIN)
    var gates := {}
    var domestication := float(herd.get("domestication", 0.0))
    var tame_reasons: Array[String] = []
    var herding := track(knowledge, HudFloraVocab.KNOWLEDGE_TRACK_HERDING)
    if herding < HudConst.KNOWLEDGE_COMPLETE:
        tame_reasons.append(HudFloraVocab.GATE_REASON_HERDING_KNOWLEDGE_FORMAT % [
            HudFormat.progress_percent(herding), sustain_icon])
    # A fully tamed herd retires Tame, exactly as a finished patch retires Cultivate — the build is
    # DONE, and re-running the verb would only pay its prep rate forever. It SUPERSEDES the knowledge
    # prerequisite (moot once the meter is full), so it replaces the reason list rather than piling on.
    # The rung is normally HIDDEN at this point (`_build_herd_assign_controls`'s ceiling pass), so this
    # reason is read only when a band standing on Tame has it re-admitted so it can be seen and cleared.
    if domestication >= SourceForecast.DOMESTICATION_COMPLETE:
        tame_reasons.clear()
        tame_reasons.append(HudFloraVocab.GATE_REASON_ALREADY_TAMED_FORMAT % sustain_icon)
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
        gates[SourceForecast.LABOR_POLICY_CORRAL] = corral_reasons
    return gates

## The BARE-KEYED twin of `forage_gates`, for the raw wire patch dict (`forage_patch_lookup`) rather
## than the `patch_`-prefixed `tile_info` cross-ref.
##
## The cross-ref's prefixing is MIXED and always has been — `MapView._tile_info_at` stamps
## `patch_ecology_phase` / `patch_is_field` / `patch_sow_site_refusal` but plain `is_cultivated` — so
## this adapter is the ONE place that mapping is written down. Callers holding a wire patch (the map's
## mark pass, the work board) come through here; nobody re-spells the keys at a call site.
static func forage_gates_from_patch(patch: Dictionary, knowledge: Dictionary) -> Dictionary:
    return forage_gates({
        "is_cultivated": bool(patch.get("is_cultivated", false)),
        "patch_ecology_phase": String(patch.get("ecology_phase", "")),
        "patch_is_field": bool(patch.get("is_field", false)),
        "patch_sow_site_refusal": String(patch.get("sow_site_refusal", "")),
    }, knowledge)

## THE READY TEST — the next rung this source could climb RIGHT NOW, as
## `{policy, glyph}`, or `{}` when there is nothing to offer.
##
## It is not a new judgement: it is the test the compose sheet already runs to decide whether to grey a
## rung, plus the two "is this rung offered at all" passes that sit beside it there. Three conditions,
## all of which must hold (docs/plan_worked_source_marks.md §3):
##
##  1. **OFFERED** — the species or the land admits the rung. Hunt: the husbandry ceiling ("wild" /
##     "pastoral" / "pen"), the SAME filter `_build_herd_assign_controls` applies. Forage: at least one
##     composition entry that `can_cultivate` / `can_sow`, the species-global legality flag.
##  2. **UNGATED** — `forage_gates` / `hunt_gates` return no reason for it (knowledge complete, the
##     per-source prerequisite met, the rung not already finished, the ground willing).
##  3. **NOT ALREADY RUNNING** — the source's current policy is not that verb. A patch mid-Cultivate is
##     progress, not an opportunity, and marking it would never clear.
##
## **HIGHEST RUNG FIRST**, the ordering `BandPanelController._work_source_rung` already depends on and
## for the same reason: a herd that can be corralled can also technically be re-tamed, and answering
## with the lower rung would erase the distinction the mark exists to draw.
static func next_rung_ready(kind: String, source: Dictionary, policy: String,
        knowledge: Dictionary) -> Dictionary:
    var current := policy.strip_edges().to_lower()
    if kind == SourceForecast.LABOR_KIND_FORAGE:
        var gates := forage_gates_from_patch(source, knowledge)
        if current != HudConst.LABOR_POLICY_SOW and not gates.has(HudConst.LABOR_POLICY_SOW) \
                and _any_crop_allows(source, "can_sow"):
            return _ready(HudConst.LABOR_POLICY_SOW)
        if current != HudConst.LABOR_POLICY_CULTIVATE and not gates.has(HudConst.LABOR_POLICY_CULTIVATE) \
                and _any_crop_allows(source, "can_cultivate"):
            return _ready(HudConst.LABOR_POLICY_CULTIVATE)
        return {}
    if kind == SourceForecast.LABOR_KIND_HUNT:
        var hunt := hunt_gates(source, knowledge)
        var ceiling := SourceForecast.husbandry_ceiling(source)
        if current != SourceForecast.LABOR_POLICY_CORRAL and not hunt.has(SourceForecast.LABOR_POLICY_CORRAL) \
                and ceiling == SourceForecast.HUSBANDRY_CEILING_PEN:
            return _ready(SourceForecast.LABOR_POLICY_CORRAL)
        if current != HudConst.LABOR_POLICY_TAME and not hunt.has(HudConst.LABOR_POLICY_TAME) \
                and ceiling != SourceForecast.HUSBANDRY_CEILING_WILD:
            return _ready(HudConst.LABOR_POLICY_TAME)
        return {}
    return {}

## THE RUNG UNDER WAY — the twin of `next_rung_ready`, as `{policy, glyph, progress}` (progress 0..1),
## or `{}` when this source is not building anything.
##
## `next_rung_ready` deliberately answers `{}` for a source whose policy IS the verb: a patch
## mid-Cultivate is progress, not an opportunity. That reasoning is right and the CONSEQUENCE was
## wrong — it left the in-flight case with no mark at all, so a patch you are actively cultivating
## looked exactly like a patch nobody has touched, while the untouched one beside it advertised `⌃`.
## The two answers are one axis in two states, and the badge shows whichever applies.
##
## Keyed on the POLICY, not on a non-zero meter: a half-built patch nobody works is not "in progress",
## and its standing rung is what the rung glyph is for. Each investment verb names the meter it fills —
## the one place that mapping is written down.
static func rung_in_progress(kind: String, source: Dictionary, policy: String) -> Dictionary:
    var current := policy.strip_edges().to_lower()
    var meter := ""
    if kind == SourceForecast.LABOR_KIND_FORAGE:
        if current == HudConst.LABOR_POLICY_CULTIVATE:
            meter = "cultivation_progress"
        elif current == HudConst.LABOR_POLICY_SOW:
            meter = "field_progress"
    elif kind == SourceForecast.LABOR_KIND_HUNT:
        if current == HudConst.LABOR_POLICY_TAME:
            meter = "domestication"
        elif current == SourceForecast.LABOR_POLICY_CORRAL:
            meter = "corral_progress"
    if meter == "":
        return {}
    var answer := _ready(current)
    answer["progress"] = clampf(float(source.get(meter, 0.0)), 0.0, 1.0)
    return answer

## Whether ANY plant in this patch's composition may climb the rung `flag` names — species-GLOBAL
## legality ("can this plant ever climb this rung"), never "is it a wise crop here". `share` answers
## that other question, and a marginal share must never suppress the mark: a legal crop at 4% is still
## a rung the player can choose.
##
## An ABSENT composition answers **false**, which is the honest reading: the flags ride every
## `ForagePatchState`, so a patch without them is one the client cannot vouch for, and the mark exists
## to promise the verb is available.
static func _any_crop_allows(patch: Dictionary, flag: String) -> bool:
    var composition: Variant = patch.get("composition", [])
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
static func track(knowledge: Dictionary, key: String) -> float:
    return float(knowledge.get(key, 0.0))
