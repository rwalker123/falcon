class_name SubjectDrawerController
extends RefCounted

## The selection card's DRAWER RENDER DISPATCH (HUD decomposition Phase 2c-3, docs/plan_hud_decomposition.md):
## the last piece of the selection card to leave `Hud.gd`, after `SelectionCardController` took the
## identity/list half and `DrawerComposeController` took the compose half. It owns the one-drawer
## dispatch (`render_subject_drawer` → land vs occupant), the land-drawer content producer
## (`_tile_terrain_lines` + `_format_food_kind_label`), the occupant/expedition/band-move `%AllocationPanel`
## branches, and the height-capping fit path.
##
## Built on the LegendController / SelectionCardController / DrawerComposeController / BandPanelController
## idiom: `HudLayer` holds one as `_drawer`, hands it the shared `RefCounted` state models BY REFERENCE
## (the SAME `HudSelectionState` / `HudBandLaborState` instances) and the sibling controllers it dispatches
## into, keeps the reflectively-reached coordinator core (`_render_selection_panel`) and the two-host
## disclosure fan-out (`_refresh_disclosure_hosts`) on the HUD node calling IN, and connects the two fit
## signals + `_refit_left_dock` to this controller's `fit_subject_drawer`.
##
## THE MOVE VERB IS A TYPED COLLABORATOR, not a Callable — the drawer's Move button `.connect()`s
## straight to `TargetingController.begin_move_band`, which owns the `_pending_move_band` state and the
## banner (the targeting machinery, with three other modes). `_is_player_unit` is a trivial private COPY
## (the SelectionCardController / BandPanelController precedent).
##
## THE FIT PATH IS THE HIGH-RISK PIECE. `fit_subject_drawer` does `await _host.get_tree().process_frame`
## — a `RefCounted` has no `get_tree()`, so the frame wait is threaded through the injected HOST node
## (the HUD CanvasLayer). A mis-threaded host or a dropped signal reconnection sizes/scrolls the drawer
## wrong while it still RENDERS, so the failure is silent; the fit path and its two signal reconnections
## (`subject_body.minimum_size_changed` and `viewport.size_changed.bind(true)` — the force-past-the-gate
## flag) are wired in HudLayer's `_ready` exactly as before, only repointed here.
##
## Word tables, formats and thresholds stay on `HudLayer` and are read back as `HudLayer.X`, the same
## convention `HudWidgets` / `HudFormat` / `SelectionCardController` / `DrawerComposeController` /
## `BandPanelController` follow — so a phrase is still typed in exactly one place.

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
var _selection: HudSelectionState = null
var _band_labor: HudBandLaborState = null
# Roster/emptiness reads (`tile_contents_unseen`) + the vitals row's `selected_terrain_label`.
var _selectioncard: SelectionCardController = null
# The compose half's drawer-action fill (`build_forage_drawer_actions` / `build_herd_drawer_actions`)
# and the per-snapshot sheet refresh (`refresh_compose_sheet`).
var _drawercompose: DrawerComposeController = null
# The band/city panel: the player-band fork (`has_panel` / `render_band`), the flat host's three zone
# builders, and the parties recall confirm.
var _bandpanel: BandPanelController = null
# The stateful band/party detail-line producer behind the occupant drawer (`unit_summary_lines`).
var _banddetail: BandDetailLines = null
# The HUD CanvasLayer — a `RefCounted` has no `get_tree()`, so the fit's frame wait goes through it.
var _host: Node = null

# --- The command-targeting cluster (see the class header) ---
# The drawer's Move button `.connect()`s straight to `_targeting.begin_move_band`.
var _targeting: TargetingController = null

# --- Scene nodes (handed in by HudLayer; they keep their `@onready` there — a `%Name` node loses
#     `unique_name_in_owner` if reparented, so the nodes stay put and the controller only writes them) ---
var _tile_detail: RichTextLabel = null
var _occupant_detail: RichTextLabel = null
var _allocation_panel: VBoxContainer = null
var _herd_assign_controls: VBoxContainer = null
var _forage_assign_controls: VBoxContainer = null
var _subject_body: VBoxContainer = null
var _subject_scroll: ScrollContainer = null
# The fit ceiling — read only, the room the drawer may claim in the dock beneath the card.
var _left_dock_scroll: ScrollContainer = null

# --- Owned state (moved off HudLayer, all drawer-only) ---
# One drawer fit in flight at a time — see `fit_subject_drawer`.
var _subject_fit_pending: bool = false
# The last land-drawer BBCode line array (skips a same-lines BBCode reparse) and the last-applied
# drawer content height (skips a same-height reflow).
var _tile_detail_lines_cache: Array = []
var _subject_fit_last_height: float = NAN

func _init(selection: HudSelectionState, band_labor: HudBandLaborState,
        selectioncard: SelectionCardController, drawercompose: DrawerComposeController,
        bandpanel: BandPanelController, banddetail: BandDetailLines, host: Node,
        tile_detail: RichTextLabel, occupant_detail: RichTextLabel, allocation_panel: VBoxContainer,
        herd_assign_controls: VBoxContainer, forage_assign_controls: VBoxContainer,
        subject_body: VBoxContainer, subject_scroll: ScrollContainer, left_dock_scroll: ScrollContainer,
        targeting: TargetingController) -> void:
    _selection = selection
    _band_labor = band_labor
    _selectioncard = selectioncard
    _drawercompose = drawercompose
    _bandpanel = bandpanel
    _banddetail = banddetail
    _host = host
    _tile_detail = tile_detail
    _occupant_detail = occupant_detail
    _allocation_panel = allocation_panel
    _herd_assign_controls = herd_assign_controls
    _forage_assign_controls = forage_assign_controls
    _subject_body = subject_body
    _subject_scroll = subject_scroll
    _left_dock_scroll = left_dock_scroll
    _targeting = targeting

## Player-faction check for a roster/drawer band (a trivial private copy of HudLayer's, the
## SelectionCardController / BandPanelController precedent — a one-line predicate is not worth a Callable).
func _is_player_unit(unit: Dictionary) -> bool:
    return int(unit.get("faction", HudConst.PLAYER_FACTION_ID)) == HudConst.PLAYER_FACTION_ID

# ---- The drawer render dispatch --------------------------------------------------------------

## The single drawer, filled by whichever subject row is lit. Exactly one of the three content
## paths is visible at a time — that is what bounds the card's height.
func render_subject_drawer() -> void:
    if _selection.subject() == HudSelectionState.SUBJECT_LAND:
        _render_land_drawer()
    else:
        _render_occupant_drawer()
    # An OPEN compose sheet re-renders IN PLACE against the fresh subject. This is the SNAPSHOT path
    # (`reapply_selection` → here, every turn), and it must NOT close the sheet — closing would make
    # it unusable under autoplay (§15). A SELECTION change has already closed the sheet by the time it
    # reaches here, so this is a no-op there.
    _drawercompose.refresh_compose_sheet()
    fit_subject_drawer()

## The LAND drawer: the terrain rows + the "Assign foragers" compose block (the land's only action).
## On a hex the player cannot see it also carries the unknown-contents statement — see below.
func _render_land_drawer() -> void:
    if _tile_detail == null:
        return
    _tile_detail.visible = true
    # Skip the `.text` reassignment (and its implicit BBCode reparse + `minimum_size_changed`) when
    # the terrain lines are identical to last render — the common per-snapshot restate of the same
    # hex, where only numbers on OTHER widgets moved.
    var lines := _tile_terrain_lines(_selection.tile_info())
    if lines != _tile_detail_lines_cache:
        # No context: the LAND has no band behind it, and every tint its rows take (Sight,
        # Habitability, Ecology, Cultivation, Field) is a pure function of the row's own value.
        _tile_detail.text = DetailFormat.detail_bbcode(lines)
        _tile_detail_lines_cache = lines.duplicate()
    _drawercompose.build_forage_drawer_actions(_selection.tile_info())
    if _allocation_panel != null:
        _allocation_panel.visible = false
    if _herd_assign_controls != null:
        _herd_assign_controls.visible = false
    _render_unknown_contents_note()

## An EMPTY occupant list is a claim of emptiness the client cannot back up, so on a hex the player
## cannot see the list carries the land row and nothing else, and the drawer says so out loud. This
## is the whole point of the fog gate — silence would read as "nothing here".
##
## Skipped when the list DOES carry occupant rows: that only happens for your own party on an
## unseen hex, and `_rebuild_subject_list` already appends `OCCUPANTS_UNSEEN_OTHERS_HINT` there.
func _render_unknown_contents_note() -> void:
    if _occupant_detail == null:
        return
    var unseen := _selectioncard.tile_contents_unseen(_selection.tile_info())
    var roster_empty := _selection.roster_units().is_empty() and _selection.roster_herds().is_empty()
    if not unseen or not roster_empty:
        _occupant_detail.visible = false
        _occupant_detail.text = ""
        return
    _occupant_detail.visible = true
    var message := HudConst.OCCUPANTS_UNKNOWN_UNEXPLORED \
        if String(_selection.tile_info().get("visibility_state", "")) == HudConst.VISIBILITY_UNEXPLORED \
        else HudConst.OCCUPANTS_UNKNOWN_REMEMBERED
    _occupant_detail.text = DetailFormat.detail_bbcode([message])

## Cap the drawer against the room left in the dock beneath the card, so a crowded hex scrolls
## INSIDE the drawer rather than dragging the whole dock.
##
## WAITS A WHOLE FRAME, not just `call_deferred`, and that is load-bearing. The drawer's content
## height is a function of its WIDTH — the detail label wraps, and the card's width is itself set by
## whichever compose block is showing — so a measurement taken before the new subject has been laid
## out reports the PREVIOUS subject's wrapping. On a card that just got narrower that under-reports
## the height and the drawer caps short with a scrollbar over content that would have fit. A
## deferred call is flushed inside the same frame and is not enough; one `process_frame` is.
## Coalesced, so the render + the body's own `minimum_size_changed` collapse into one fit. The frame
## wait is threaded through the injected HOST — a `RefCounted` has no `get_tree()`.
func fit_subject_drawer(force: bool = false) -> void:
    if _subject_scroll == null or _subject_body == null or _subject_fit_pending:
        return
    _subject_fit_pending = true
    await _host.get_tree().process_frame
    _subject_fit_pending = false
    if _subject_scroll == null or _subject_body == null:
        return
    # Once the teardown/rebuild flash is gone, a same-structure restate settles to the SAME content
    # height, so the awaited resize (which reflows the drawer) is pure churn — skip it unless the
    # height actually moved, or a caller FORCES it because the dock ROOM changed (window resize, feed
    # toggle) while the content did not.
    var content_height := _subject_body.get_combined_minimum_size().y
    if not force and is_equal_approx(content_height, _subject_fit_last_height):
        return
    _subject_fit_last_height = content_height
    DockScrollFit.fit_height(
        _subject_scroll,
        content_height,
        _left_dock_scroll,
        HudSelectionVocab.SUBJECT_DRAWER_MIN_HEIGHT,
        HudSelectionVocab.SUBJECT_DRAWER_BOTTOM_MARGIN,
    )

# ---- The land-drawer content producer -------------------------------------------------------

## The LAND DRAWER's rows: only what a CHIP CANNOT CARRY.
##
## The pinned chip strip above the list already states this tile's standing condition — Sight,
## Habitability, Climate, Tags, Site — so printing those as rows here restated the strip verbatim,
## and `Biome` restated the land ROW's own label (the "no restated identity" rule,
## docs/plan_tile_panel_layout.md §8). The chips REPLACE those rows; what is left is the numbers and
## the stocks, whose subject is the land: Height · the rivers · Pasture · Forage · the patch's
## biomass/ecology · the two build meters — plus the FoW sentences, which are statements, not
## conditions, and have no chip.
##
## `_render_land_drawer` is the ONE caller (the map hover tooltip builds its own text in
## `show_tooltip`), so the trim is local to the drawer.
func _tile_terrain_lines(tile_info: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    if tile_info.is_empty():
        lines.append("Hover or click a tile to inspect details.")
        return lines
    # Fog of War: never-seen tiles reveal nothing; remembered (Discovered) tiles
    # show only their last-known terrain, not current contents. See MapView
    # _apply_visibility_to_info, which redacts the hidden fields before this runs.
    # The Sight CHIP states which of the three states this hex is in; the sentence says what that
    # costs you, which is the part a chip cannot carry.
    var visibility_state := String(tile_info.get("visibility_state", ""))
    if visibility_state == HudConst.VISIBILITY_UNEXPLORED:
        lines.append("Not yet scouted — send a band to reveal this area.")
        return lines
    if tile_info.has("height_display"):
        lines.append("Height: %s" % String(tile_info["height_display"]))
    # Hex-edge rivers — which SIDES of this tile carry water (the sides a crossing cost will
    # apply to). Terrain-intrinsic permanent geography, so it renders before the discovered
    # early-return, like Pasture below. Guarded on the key so a rehydrated snapshot
    # degrades to no row instead of a wrong one; RiverEdges returns [] on a riverless tile, so it
    # never emits an empty "River:" label. Same formatter the map hover tooltip uses.
    if tile_info.has("river_edges"):
        lines.append_array(RiverEdges.summary_lines(int(tile_info["river_edges"])))
    # (A discovered Wondrous Site is a standing condition of the ground — it rides the chip strip.)
    # PASTURE — the animal-edible stock (see PASTURE_KEY). Surfaced BEFORE the discovered
    # early-return because, like the biome on the land row and the habitability chip, grass is a property of the
    # GROUND: you can read a steppe from a ridge, and a remembered tile already remembers its biome.
    # (What a remembered tile redacts is live CONTENTS — the bands and herds standing on it.) Only
    # when the ground carries pasture at all, so a glacier prints nothing rather than "0 / 0".
    var graze_capacity := float(tile_info.get("graze_capacity", 0.0))
    if graze_capacity > 0.0:
        lines.append("%s: %.0f / %.0f" % [
            HudFloraVocab.PASTURE_KEY, float(tile_info.get("graze_biomass", 0.0)), graze_capacity
        ])
        var graze_phase := String(tile_info.get("graze_ecology_phase", "")).strip_edges().to_lower()
        if graze_phase != "":
            lines.append("%s: %s" % [HudFloraVocab.PASTURE_ECOLOGY_KEY, DetailFormat.ecology_phase_label(graze_phase)])
    if visibility_state == HudConst.VISIBILITY_DISCOVERED:
        lines.append("Last seen — information incomplete. Scout to update.")
        return lines
    var food_label := String(tile_info.get("food_module_label", "None")).strip_edges()
    if food_label == "":
        food_label = "None"
    var food_kind := String(tile_info.get("food_kind", "")).strip_edges()
    var food_line := "Forage: %s" % food_label
    if food_kind != "":
        food_line = "%s — %s" % [food_line, _format_food_kind_label(food_kind)]
    # NOTE: the module's `seasonal_weight` is deliberately NOT printed — it is an internal
    # yield coefficient, meaningless to the player (it still drives the sim's yield math).
    lines.append(food_line)
    # WHAT GROWS HERE / CROP — the named plants behind the Forage line above (flora roster F1/S1).
    # It reads directly under the module because it says what that module's basket IS; the
    # stock/ecology rows below then say how much of it there is and how it is faring. ONE row, two
    # states: an UNCOMMITTED patch names the whole wild basket, a COMMITTED one names the single crop
    # it was tended to — never both, since committing displaces the rest of the basket.
    var crop_name := String(tile_info.get("patch_committed_display_name", "")).strip_edges()
    if String(tile_info.get("patch_committed_species", "")).strip_edges() != "" and crop_name != "":
        lines.append("%s: %s" % [HudFloraVocab.FLORA_CROP_ROW, crop_name])
    else:
        var composition_text := DetailFormat.flora_composition_text(tile_info.get("patch_composition", []))
        if composition_text != "":
            lines.append("%s: %s" % [HudFloraVocab.FLORA_COMPOSITION_ROW, composition_text])
    # Standing forage stock vs the patch's ceiling — the patch counterpart to a herd's "Biomass"
    # row, so a foraged patch reads like wild game does ("how much there is"). Foraging draws the
    # biomass down and it regrows logistically toward the capacity. Only rendered when the snapshot
    # carries a real patch (capacity > 0), so a plain food-module tile with no patch stays bare.
    var patch_capacity := float(tile_info.get("patch_carrying_capacity", 0.0))
    if patch_capacity > 0.0:
        lines.append("Forage biomass: %.0f / %.0f" % [float(tile_info.get("patch_biomass", 0.0)), patch_capacity])
    # Ecology phase of the patch — ALWAYS shown for any tile carrying a patch (not just a
    # cultivated one): the phase gates whether cultivation can accrue at all, so it is the
    # single most important condition on a forage tile. Same row name / label / tint as the
    # herd's Ecology row (`DetailFormat.ecology_phase_label` + `ecology_value_hex`), so a stressed patch
    # and a stressed herd read identically.
    var patch_phase := String(tile_info.get("patch_ecology_phase", "")).strip_edges().to_lower()
    if patch_phase != "":
        lines.append("Ecology: %s" % DetailFormat.ecology_phase_label(patch_phase))
    # Forage-patch intensification ladder: while a patch is being tended it shows the
    # cultivation progress; once cultivated it reads as a "Tended Patch" (SIGNAL tint).
    # Mirrors the herd Husbandry row. Only when the snapshot carries the field so we
    # never invent a state on a patch that isn't being worked.
    if bool(tile_info.get("is_cultivated", false)):
        lines.append("Cultivation: %s" % DetailFormat.cultivation_label(1.0, true))
    elif tile_info.has("cultivation_progress"):
        var cultivation_progress := float(tile_info["cultivation_progress"])
        if cultivation_progress > 0.0:
            lines.append("Cultivation: %s" % DetailFormat.cultivation_label(cultivation_progress, false))
    # PLANT RUNG 3 — the Field, on its OWN row beside Cultivation. The patch carries TWO independent
    # build meters (a Field may stand on ground that was never tended: seed travels, so `Sow` needs no
    # prior patch), so they are two rows, never one merged "progress" number. This is the per-source
    # half of the two-meter split (§4.1) — the FACTION's Seed Selection knowledge is NOT shown here;
    # it lives in the top-bar knowledge strip, because it is a property of your people, not of this
    # ground. Both rows are the source's own, and both decay if the patch is abandoned.
    if bool(tile_info.get("patch_is_field", false)):
        lines.append("%s: %s" % [HudFloraVocab.FIELD_ROW, DetailFormat.field_label(1.0, true)])
    elif tile_info.has("patch_field_progress"):
        var field_progress := float(tile_info["patch_field_progress"])
        if field_progress > 0.0:
            lines.append("%s: %s" % [HudFloraVocab.FIELD_ROW, DetailFormat.field_label(field_progress, false)])
    return lines

func _format_food_kind_label(kind_value: String) -> String:
    if kind_value == "":
        return ""
    var tokens: PackedStringArray = kind_value.split("_", false)
    if tokens.is_empty():
        return kind_value.capitalize()
    var parts: Array[String] = []
    for token in tokens:
        if token == "":
            continue
        var head := token.substr(0, 1).to_upper()
        var tail := ""
        if token.length() > 1:
            tail = token.substr(1, token.length() - 1)
        parts.append(head + tail)
    if parts.is_empty():
        return kind_value.capitalize()
    return " ".join(parts)

# ---- The occupant drawer + its %AllocationPanel branches --------------------------------------

## The detail drawer + action buttons for the currently-selected occupant. Shares the one drawer
## with the land, so it hides the land's content first — exactly one subject fills it.
func _render_occupant_drawer() -> void:
    if _occupant_detail == null:
        return
    if _tile_detail != null:
        _tile_detail.visible = false
    if _forage_assign_controls != null:
        _forage_assign_controls.visible = false
    # This render's tint context, constructed LOCALLY: the band line producers below fill it as they
    # emit rows, and it is handed to the formatter at the bottom. Nothing outlives this call.
    var ctx := DetailFormat.Context.new()
    var is_band := not _selection.unit().is_empty()
    var is_herd := not _selection.herd().is_empty()
    var is_expedition := is_band and bool(_selection.unit().get("is_expedition", false))
    var is_player_band := is_band and not is_expedition and _is_player_unit(_selection.unit())
    # A selected player band is the panel's subject: its detail + labor allocation render into the
    # dockable Band/City panel (docs/plan_band_city_dock.md §3), and the Occupants card shows NO
    # band detail (the roster still lists it). Falls back to the legacy in-card drawer only when no
    # panel is injected (e.g. the HUD-only ui_preview harness).
    if is_player_band and _bandpanel.has_panel():
        _bandpanel.render_band(_selection.unit())
        # The drawer is now VISIBLE furniture rather than a hidden card, so an empty one reads as a
        # rendering fault. Point at where the band's detail actually went instead of leaving a gap.
        _occupant_detail.visible = true
        _occupant_detail.text = DetailFormat.detail_bbcode([HudSelectionVocab.BAND_PANEL_POINTER_TEXT])
        # The one order that stays HERE (§18): repositioning is a map action. Player resident bands
        # only — this branch is already player-band-gated, and a foreign band's orders aren't ours.
        _build_band_move_actions()
        if _herd_assign_controls != null:
            _herd_assign_controls.visible = false
        return
    # Herd / expedition / non-player band (or no-panel fallback) → the Occupants card drawer,
    # unchanged. Expedition → Recall/Move panel; player band (fallback) → allocation panel; herd →
    # assign-hunters controls. All mutually exclusive with the current selection.
    _occupant_detail.visible = true
    var lines: Array[String] = []
    if not _selection.unit().is_empty():
        lines = _banddetail.unit_summary_lines(
            _selection.unit(), _selectioncard.selected_terrain_label(), ctx)
    elif not _selection.herd().is_empty():
        lines = DetailFormat.herd_summary_lines(_selection.herd(), _band_labor.world_herds())
    _occupant_detail.text = DetailFormat.detail_bbcode(lines, ctx)
    if is_expedition:
        _build_expedition_panel(_selection.unit())
    elif is_player_band:
        _build_allocation_panel(_selection.unit())
    elif _allocation_panel != null:
        _allocation_panel.visible = false
    if is_herd:
        _drawercompose.build_herd_drawer_actions(_selection.herd())
    elif _herd_assign_controls != null:
        _herd_assign_controls.visible = false

## Stack the three ZONE contents into `target` — the legacy flat host (the Occupants card's
## %AllocationPanel, used by the no-dock `ui_preview` harness). It renders exactly what the dock
## renders, through the SAME three builders (`BandPanelController.build_*_zone`); there is no second
## layout to maintain.
##
## It writes the drawer's `%AllocationPanel` node — HudLayer's, passed in — so it stays with the
## drawer render dispatch; the controller never needs a second host. Its two siblings on the same host
## (`_build_band_move_actions` / `_build_expedition_panel`) are branches of `_render_occupant_drawer`
## and live here for the same reason.
func _build_allocation_panel(band: Dictionary, target: VBoxContainer = null) -> void:
    var container: VBoxContainer = target if target != null else _allocation_panel
    if container == null:
        return
    HudWidgets.clear_children(container)
    var is_player := not band.is_empty() and _is_player_unit(band)
    container.visible = is_player
    if not is_player:
        return
    container.add_child(_bandpanel.build_band_zone(band, false))
    container.add_child(_bandpanel.build_work_zone(band))
    container.add_child(_bandpanel.build_parties_zone(band))
    # The docked path offers Move from `_build_band_move_actions`; this host must offer it too, or a
    # selected player band has no way to be moved at all here (see `_make_band_move_actions`).
    container.add_child(_make_band_move_actions())

## The selected PLAYER band's one drawer action (§18): Move. Shares the allocation-panel host with
## `_build_expedition_panel` and `_build_allocation_panel` — all three branches are mutually
## exclusive on the selected occupant, so the fallback path's own Orders Move is never doubled.
##
## Wired straight to `_targeting.begin_move_band`, which resolves through `_resolve_assign_band()` and so
## already targets the band selected in THIS list — the whole point on a hex carrying several.
## `Clear all` is deliberately NOT here: it returns every worker to idle, a heavier action that
## belongs beside the labor allocation it clears.
func _build_band_move_actions() -> void:
    if _allocation_panel == null:
        return
    for child in _allocation_panel.get_children():
        child.queue_free()
    _allocation_panel.visible = true
    _allocation_panel.add_child(_make_band_move_actions())

## The Move row itself, so the two hosts that offer it build the SAME control rather than two that
## can drift. **Both hosts must offer it**: the docked path adds it beside the panel pointer, and the
## NO-PANEL fallback appends it under the band content — the fallback used to inherit a Move from the
## allocation stack's Orders block, and when the Band panel rework deleted that block the fallback
## silently offered no way to move a band at all. `ui_preview`'s "exactly ONE Move button" assertion
## is what catches either half of that going wrong (none offered, or one offered twice).
func _make_band_move_actions() -> HBoxContainer:
    var actions := HBoxContainer.new()
    actions.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    var move_btn := Button.new()
    move_btn.text = HudSelectionVocab.MOVE_BAND_BUTTON_TEXT
    HudStyle.apply_button(move_btn, "ghost")
    move_btn.tooltip_text = HudSelectionVocab.MOVE_BAND_BUTTON_TOOLTIP
    move_btn.pressed.connect(_targeting.begin_move_band)
    actions.add_child(move_btn)
    return actions

## The dedicated panel for a selected in-flight expedition (no labor in v1): an awaiting-orders
## callout (echoing the pulsing map ring) plus Move (retarget via move_band on the expedition
## entity) and Recall. Reuses the allocation-panel host; player expeditions only.
func _build_expedition_panel(expedition: Dictionary) -> void:
    if _allocation_panel == null:
        return
    for child in _allocation_panel.get_children():
        child.queue_free()
    var is_player := not expedition.is_empty() and _is_player_unit(expedition)
    _allocation_panel.visible = is_player
    if not is_player:
        return
    var phase := String(expedition.get("expedition_phase", "")).strip_edges().to_lower()
    if phase == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
        var callout := HudWidgets.alloc_hint_label("Reached its objective — Recall it home, or Move it onward.")
        callout.add_theme_color_override("font_color", HudStyle.WARN)
        _allocation_panel.add_child(callout)
    var actions := HBoxContainer.new()
    actions.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    var move_btn := Button.new()
    move_btn.text = "Move"
    HudStyle.apply_button(move_btn, "ghost")
    move_btn.tooltip_text = "Send the expedition onward, then click a target tile."
    move_btn.pressed.connect(_targeting.begin_move_band)
    actions.add_child(move_btn)
    # Already homeward-bound: the button reads its state ("Returning", disabled) rather than a
    # mysterious grayed-out "Recall". Otherwise it's an enabled "Recall" that folds the party home.
    var returning := phase == HudExpeditionVocab.EXPEDITION_PHASE_RETURNING
    var recall_btn := Button.new()
    recall_btn.text = "Returning" if returning else "Recall"
    HudStyle.apply_button(recall_btn, "primary")
    recall_btn.tooltip_text = "Heading home — folds workers + provisions back on arrival." if returning \
        else "Order the expedition home (folds workers + provisions back on arrival)."
    recall_btn.disabled = returning
    recall_btn.pressed.connect(func() -> void: _bandpanel.confirm_recall_expedition(expedition))
    actions.add_child(recall_btn)
    _allocation_panel.add_child(actions)
