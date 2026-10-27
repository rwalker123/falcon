class_name RungCardController
extends RefCounted

## **THE RUNG CARD — ONE POPUP, TWO HOSTS** (issue #877). The "Take it to…" card a source's `⌃` opens, the
## crop step behind a plant rung, the ring card on a penned herd, and a working's deposit track. It was
## `BandPanelController`'s alone while only the Work tab's rows opened it; the source's compose sheet
## mounts the same work strip and so opens the SAME card with the SAME commands, which is why it lives
## here and not on either host. `Hud` builds one and hands it to both.
##
## **A `PopupPanel` BECAUSE IT IS A WINDOW.** The Work zone is full and a sheet is a fitted card, so a
## track drawn as a block would slice one or the other. A Window costs neither any height, and an
## embedded subwindow draws above every `CanvasLayer`, the compose sheet's included.
##
## **THE CARD IS REBUILT PER OPEN, NEVER PATCHED.** It is a function of the source's position, the
## faction's knowledge and whatever build is declared, all of which move per snapshot. The popup node is
## reused because a Window is expensive; its CONTENT is not. The margin is the chrome and is never freed
## — its CHILDREN are (clearing the Window's own children frees the margin the next line reaches for,
## and `queue_free` is deferred).
##
## **IT EMITS ITS OWN SIGNALS** and the hosts relay them: `improvement_requested` (a picked rung),
## `crop_chosen` (the crop a plant rung commits, sent BEFORE the declaration so the optimistic overlay the
## declaration writes is the one the rebuilt board reads), `extend_pen_requested`, `unqueue_requested`
## (the in-flight rung's STOP).

signal improvement_requested(payload: Dictionary)
signal crop_chosen(band: Dictionary, model: Dictionary, species: String)
signal extend_pen_requested(payload: Dictionary)
signal unqueue_requested(payload: Dictionary)

var _band_labor: HudBandLaborState = null
var _topbar: FactionReadouts = null
var _host: Node = null
var _popup: PopupPanel = null
var _body: MarginContainer = null

func _init(band_labor: HudBandLaborState, topbar: FactionReadouts, host: Node) -> void:
    _band_labor = band_labor
    _topbar = topbar
    _host = host

## The popup, built on first use. Harnesses read it to see what the card holds.
func card() -> PopupPanel:
    return _ensure_card()

func is_open() -> bool:
    return _popup != null and is_instance_valid(_popup) and _popup.visible

## Take the card down, if one is up. Idempotent, and safe before the card has ever been built.
func dismiss() -> void:
    if is_open():
        _popup.hide()

func _player_knowledge() -> Dictionary:
    return _topbar.faction_tracks(HudConst.PLAYER_FACTION_ID) if _topbar != null else {}

func _knowledge_labels() -> Dictionary:
    return _topbar.knowledge_labels() if _topbar != null else {}

## The RAW wire source the track reads — the patch off the lookup, or the herd's LIVE dict. Herds
## migrate, so a track must never read the assignment's launch-time target.
func source_of(model: Dictionary) -> Dictionary:
    if String(model.get("kind", "")) == SourceForecast.LABOR_KIND_HUNT:
        return _band_labor.find_world_herd(String(model.get("herd_id", "")))
    return _band_labor.forage_patch_lookup().get(
        Vector2i(int(model.get("x", -1)), int(model.get("y", -1))), {})

# ---- the plant / animal track ---------------------------------------------------------------------

## Open the destination track for a forage or hunt row's source. `anchor` is the control that was
## pressed, captured for the popup's rect BEFORE anything re-renders.
##
## `source_override` is the wire source a COMPOSE SHEET renders from, handed in so the card reads the same
## dictionary as the readout beside the strip; the board's rows leave it empty and read the lookups.
func open_track(band: Dictionary, model: Dictionary, anchor: Control,
        source_override: Dictionary = {}) -> void:
    var kind := String(model.get("kind", ""))
    var source := source_override if not source_override.is_empty() else source_of(model)
    var rows := RungLadder.track(kind, source, HudComposeVocab.BARE_FORECAST_PREFIX,
        String(model.get("improvement", "")), _player_knowledge(), band, _knowledge_labels())
    # ⛔ UNREACHABLE BY CONSTRUCTION, AND IT SAYS SO rather than returning in silence: the slot that
    # reaches here is a button only for a rung on offer or under way, and both admit a rung through
    # `RungGates.rung_has_room`, the very test `RungLadder.track` banks a rung with.
    if not RungLadder.has_track(rows):
        push_warning(("RungCardController: a ⌃ was pressed on a source standing at the top of its " +
            "branch (kind=%s) — the offer test and the destination track disagree") % kind)
        return
    var body := _fresh_body()
    body.add_child(RungLadder.build_track(rows,
        func(rung: String) -> void: _pick_rung(band, model, source, rung, anchor),
        HudWorkVocab.RUNG_TRACK_TITLE, _max_builders_by_rung(),
        func() -> void:
            dismiss()
            emit_unqueue(band, model)))
    _popup.popup(_anchor_rect(anchor))

## A picked rung either declares or asks for a crop, and which it does is a property of the RUNG. The
## press closes the card BEFORE it emits: the declaration re-renders the zone this card is anchored to.
## An animal rung stays one click (`tame` and `corral` commit no species).
func _pick_rung(band: Dictionary, model: Dictionary, source: Dictionary, rung: String,
        anchor: Control) -> void:
    if RungLadder.rung_commits_a_crop(String(model.get("kind", ""))):
        var crops := RungLadder.crop_choices(source, HudComposeVocab.BARE_FORECAST_PREFIX, rung)
        if not crops.is_empty():
            _open_crop_step(band, model, source, rung, crops, anchor)
            return
    dismiss()
    _emit_ready_declaration(band, model, rung)

## The second page of the same card — the crop the plant rung commits to. It defaults to nothing and
## the pick IS the declaration; `Sim picks` is still last. The crop goes first and the rung second, the
## commands' order. Back REBUILDS the track rather than restoring it.
func _open_crop_step(band: Dictionary, model: Dictionary, source: Dictionary, rung: String,
        crops: Array[Dictionary], anchor: Control) -> void:
    var body := _fresh_body()
    body.add_child(RungLadder.build_crop_step(crops,
        func(species: String) -> void:
            dismiss()
            crop_chosen.emit(band, model, species)
            _emit_ready_declaration(band, model, rung),
        func() -> void: open_track(band, model, anchor),
        RungQuotes.places_source(RungQuotes.quote_of_verb(source,
            HudComposeVocab.BARE_FORECAST_PREFIX, rung))))
    _popup.popup(_anchor_rect(anchor))

## **THE DECLARATION** — the verb the source's chosen rung names, on the source the row already stands
## for. It sends the verb and nothing else; `workers` / `floor` / `pending_entity` ride the payload for
## the relay's optimistic write and are not command tokens.
func _emit_ready_declaration(band: Dictionary, model: Dictionary, rung: String) -> void:
    if rung == SourceForecast.IMPROVEMENT_NONE:
        return
    var herd_id := String(model.get("herd_id", ""))
    # A herd's rung is addressed at its LIVE tile; the model's `x`/`y` are the assignment's launch-time
    # ones, which a migrated herd has left.
    var live := _band_labor.find_world_herd(herd_id) if herd_id != "" else {}
    improvement_requested.emit({
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "improvement": rung,
        "kind": String(model.get("kind", "")),
        "x": int(live.get("x", model.get("x", -1))),
        "y": int(live.get("y", model.get("y", -1))),
        "herd_id": herd_id,
        # The source's CURRENT crew, floor, kit and migration flag, restated so the overlay's row is
        # this row: the pending entry REPLACES the merged row it shadows.
        "workers": int(model.get("workers", 0)),
        "floor": float(model.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR)),
        "kit_id": String(model.get("kit_id", KitRoster.NO_KIT_ID)),
        "move_with_herd": bool(model.get("move_with_herd", false)),
        "pending_entity": int(band.get("entity", -1)),
    })

## **THE STOP** — stop the build on a site: `unqueue`, which means *stop this build* now that builds join
## the site's crew (there is no queue to leave). The payload is the site chip's, key for key, so
## `Main.format_unqueue` and the optimistic relay serve every control without knowing which one stopped it.
func emit_unqueue(band: Dictionary, model: Dictionary) -> void:
    unqueue_requested.emit({
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "x": int(model.get("x", -1)),
        "y": int(model.get("y", -1)),
        "herd_id": String(model.get("herd_id", "")),
        # The site's address halves (`Main.site_address`): a working carries its material and a road
        # says it is a road, or the server reads the bare tile as the PATCH on that hex.
        "material": String(model.get("material", "")),
        # A tile-kind improvement is addressed by its branch, which leads (`Main.site_address`).
        "branch": String(model.get("branch", "")),
        "road": bool(model.get("road", false)),
        # The optimistic half's two keys, neither a command token.
        "kind": String(model.get("kind", "")),
        "pending_entity": int(band.get("entity", -1)),
    })

# ---- the ring card ---------------------------------------------------------------------------------

## The ring's price card, opened from the standing-rung mark on a penned herd. It opens a PRICE, not a
## bare commit, so the caret means one thing on every mark that wears it.
func open_ring_card(band: Dictionary, model: Dictionary, anchor: Control,
        source_override: Dictionary = {}) -> void:
    var source := source_override if not source_override.is_empty() else source_of(model)
    if source.is_empty():
        return
    var body := RungLadder.build_ring_card(
        RungLadder.ring_row(source, HudComposeVocab.BARE_FORECAST_PREFIX, band),
        func() -> void:
            dismiss()
            _emit_extend_pen(band, source))
    body.set_meta(HudWorkVocab.RING_CARD_META, true)
    _fresh_body().add_child(body)
    _popup.popup(_anchor_rect(anchor))

## `extend_pen <faction> <x> <y>`, at the pen's anchor tile (the herd's own). Names no band.
func _emit_extend_pen(band: Dictionary, herd: Dictionary) -> void:
    var x := int(herd.get("x", -1))
    var y := int(herd.get("y", -1))
    if x < 0 or y < 0:
        return
    extend_pen_requested.emit({
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "x": x,
        "y": y,
    })

# ---- the deposit track -----------------------------------------------------------------------------

## The deposit branches' catalog as ordered rows, `[]` before any snapshot has arrived.
func deposit_ladder() -> Array[Dictionary]:
    return HudDepositVocab.deposit_ladder(_topbar.deposit_rungs() if _topbar != null else [])

## A working's destination track — the same Window, renderer and press the plant and animal rungs use.
## `cutters` is the take crew, pending-aware. The press emits the rung's verb through
## `improvement_requested`, its material-targeted arm; no `assign_labor` rides with it.
func open_deposit_track(band: Dictionary, deposit: Dictionary, cutters: int,
        anchor: Control) -> void:
    var ladder := deposit_ladder()
    if ladder.is_empty():
        # The catalog is per world and arrives with the first snapshot; an empty one is a wire this
        # client has not been sent, not a branch with nothing on it.
        push_warning("RungCardController: a working's ⌃ was pressed with no deposit rung catalog " +
            "on the wire — the track cannot state a single rung")
        return
    var rows := RungLadder.deposit_track(deposit, ladder, _player_knowledge(), _knowledge_labels(),
        cutters, SourceForecast.build_crew_staffed(deposit, SourceForecast.BARE_SOURCE_PREFIX), band)
    if not RungLadder.has_track(rows):
        return
    var building := RungLadder.deposit_building_verb(rows)
    # The ladder and the put-down row are ONE column: how far this working can go and the row saying it
    # need not go anywhere at all are the two ends of one decision.
    var column := VBoxContainer.new()
    column.add_theme_constant_override("separation", HudWorkVocab.RUNG_TRACK_ROW_SEPARATION)
    column.add_child(RungLadder.build_track(rows,
        func(verb: String) -> void:
            dismiss()
            # The rung being raised is only followed: the sim refuses a rung the working already
            # stands at or above.
            if verb != building:
                _emit_deposit_declaration(band, deposit, verb),
        HudWorkVocab.RUNG_TRACK_TITLE, _max_builders_by_rung(),
        func() -> void:
            dismiss()
            emit_unqueue(band, _deposit_unqueue_model(deposit))))
    _fresh_body().add_child(column)
    _popup.popup(_anchor_rect(anchor))

## The site halves a deposit's `unqueue` names: the `(tile, material)` pair off the `deposits` row.
func _deposit_unqueue_model(deposit: Dictionary) -> Dictionary:
    var tile := HudDepositVocab.tile_of(deposit)
    return {"x": tile.x, "y": tile.y, "herd_id": "", "material": HudDepositVocab.material_of(deposit),
        "kind": HudConst.LABOR_KIND_EXTRACT}

## `fell|coppice|quarry <faction> <x> <y> <material>`. The pair is read off the `deposits` row itself,
## never a label: one hex holds two workings, so a press that lost the material would raise the other.
func _emit_deposit_declaration(band: Dictionary, deposit: Dictionary, verb: String) -> void:
    if verb == SourceForecast.IMPROVEMENT_NONE or band.is_empty():
        return
    var tile := HudDepositVocab.tile_of(deposit)
    if tile.x < 0 or tile.y < 0:
        return
    var material := HudDepositVocab.material_of(deposit)
    if material == HudDepositVocab.MATERIAL_NONE:
        return
    improvement_requested.emit({
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "improvement": verb,
        "kind": HudConst.LABOR_KIND_EXTRACT,
        "x": tile.x,
        "y": tile.y,
        "material": material,
        "herd_id": "",
    })

# ---- the tile-branch track -------------------------------------------------------------------------

## The tile-sourced branches' catalog rows, `[]` before any snapshot has arrived.
func tile_ladder() -> Array[Dictionary]:
    return HudTileVocab.tile_ladder(_topbar.ladder_rungs() if _topbar != null else [])

## A tile improvement's destination track (belief's monument, food stores' pit, ...): the same Window,
## renderer and press the deposit track uses. `crew` is the count of bands HOLDING a tile row on it (`tile_rows_held`; not the staffed count) and
## `tile_info` the land card's dict, whose `belief` the site gate reads. The press emits
## `build <verb> <f> <x> <y>` through `improvement_requested`; no `assign_labor` rides with it.
func open_tile_track(band: Dictionary, tile_info: Dictionary, branch: String, crew: int,
        anchor: Control) -> void:
    if tile_ladder().is_empty():
        push_warning("RungCardController: a tile branch's step card was opened with no tile rung " +
            "catalog on the wire -- the track cannot state a single rung")
        return
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    var rows := tile_track_rows(band, tile_info, branch, crew)
    if not RungLadder.has_track(rows):
        return
    var building := RungLadder.tile_building_verb(rows)
    _fresh_body().add_child(RungLadder.build_track(rows,
        func(verb: String) -> void:
            dismiss()
            # The rung being raised is only followed: the sim refuses a rung already stood at or above.
            if verb != building:
                _emit_tile_declaration(band, x, y, verb),
        HudWorkVocab.RUNG_TRACK_TITLE, _max_builders_by_rung(),
        func() -> void:
            dismiss()
            emit_unqueue(band, {"x": x, "y": y, "herd_id": "", "branch": branch,
                "material": branch, "kind": HudConst.LABOR_KIND_TILE})))
    _popup.popup(_anchor_rect(anchor))

## The rows of one tile branch's track on one hex -- what the card renders, and what a harness re-asks of
## a drawn `⌃` to prove its press opens something.
func tile_track_rows(band: Dictionary, tile_info: Dictionary, branch: String,
        crew: int) -> Array[Dictionary]:
    var ladder := tile_ladder()
    var x := int(tile_info.get("x", -1))
    var y := int(tile_info.get("y", -1))
    var row := _band_labor.tile_improvement_at(x, y, branch)
    var site := HudTileVocab.site_of(row, ladder, branch, x, y)
    # A Work-tab row has no land card behind it: the belief standing on the tile comes off the map's
    # lookup (a tile with none has no entry, which reads as zero).
    if not tile_info.has("belief"):
        tile_info = tile_info.duplicate()
        tile_info["belief"] = _band_labor.tile_belief_at(x, y)
    return RungLadder.tile_track(site, ladder, _player_knowledge(), _knowledge_labels(), crew,
        tile_info, SourceForecast.build_crew_staffed(row, SourceForecast.BARE_SOURCE_PREFIX), band)

## `build <verb> <faction> <x> <y>` -- a tile improvement names a bare tile (its crew is the site crew, so
## no band token). It carries no `pending_entity`: the declaration lands on the improvement's own meter.
func _emit_tile_declaration(band: Dictionary, x: int, y: int, verb: String) -> void:
    if verb == SourceForecast.IMPROVEMENT_NONE or band.is_empty() or x < 0 or y < 0:
        return
    improvement_requested.emit({
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "improvement": verb,
        "kind": HudConst.LABOR_KIND_TILE,
        "x": x,
        "y": y,
        "herd_id": "",
    })

## The rung catalog's builder caps - what the "Take it to..." card reads "up to N builders at once" from.
func _max_builders_by_rung() -> Dictionary:
    return RungLadder.max_builders_by_rung(_topbar.ladder_rungs() if _topbar != null else [])

# ---- the popup ---------------------------------------------------------------------------------------

## The card's body, emptied. The margin is chrome and survives; only its children go.
func _fresh_body() -> MarginContainer:
    _ensure_card()
    HudWidgets.clear_children(_body)
    return _body

## Where the card sits: a zero-height rect just under the pressed control, in SCREEN space (what
## `Popup.popup` wants). It must be in the tree, not merely alive: `get_screen_transform` on a detached
## `CanvasItem` is an engine error plus an identity transform — a card at the top-left of the screen.
func _anchor_rect(anchor: Control) -> Rect2i:
    if anchor == null or not is_instance_valid(anchor) or not anchor.is_inside_tree():
        return Rect2i()
    var xform := anchor.get_screen_transform()
    var below := xform * Vector2(0.0, anchor.size.y + HudWorkVocab.RUNG_TRACK_GAP)
    return Rect2i(Vector2i(below), Vector2i.ZERO)

func _ensure_card() -> PopupPanel:
    if _popup != null and is_instance_valid(_popup):
        return _popup
    var card := PopupPanel.new()
    card.name = HudWorkVocab.RUNG_TRACK_META
    card.set_meta(HudWorkVocab.RUNG_TRACK_META, true)
    card.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
    var margin := MarginContainer.new()
    for side in DisclosureController.POPOVER_MARGIN_SIDES:
        margin.add_theme_constant_override("margin_%s" % side, HudWorkVocab.RUNG_TRACK_PADDING)
    card.add_child(margin)
    _host.add_child(card)
    _popup = card
    _body = margin
    return card
