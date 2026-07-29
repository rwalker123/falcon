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
## `tile_info` is the `patch_`-PREFIXED tile cross-ref, not the bare wire patch dict — see
## `forage_gates_from_patch` for the bare-keyed twin.
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

## One faction-knowledge track's 0..1 progress out of the caller's `knowledge` dict, 0.0 when absent.
## A missing track is "not learned", never "learned" — an absent key must gate, not open, or a
## snapshot that omits a track would silently unlock every rung it guards.
static func track(knowledge: Dictionary, key: String) -> float:
    return float(knowledge.get(key, 0.0))
