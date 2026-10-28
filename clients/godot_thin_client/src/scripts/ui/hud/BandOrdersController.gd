class_name BandOrdersController
extends RefCounted

## **BAND ORDERS -- the client's one door to the chief's standing orders** (#860,
## `.claude/rules/client/band-orders.md`). It owns three things and nothing else:
##
##   1. the SENDS: `band_order <faction> <band> <kind> set|release|claim`, each a payload on
##      `band_order_requested` that `HudLayer` relays and `Main.format_band_order` renders;
##   2. the LOOKUPS the Work tab and the sheets share: which labor assignment a row model is, whether it
##      is the chief's, the order a row is listed under;
##   3. the request to OPEN an order's sheet (`order_sheet_requested`), which `DrawerComposeController`
##      answers.
##
## What is DRAWN lives in `BandOrdersView` (the Work tab group) and `BandOrdersSheets` (the sheet
## sections) -- all-`static`, handed this controller for their sends.

## A set / release / claim, ready for `Main.format_band_order`.
signal band_order_requested(payload: Dictionary)
## The player pressed an order's name on the Work tab: open its Food / Fodder / material sheet.
signal order_sheet_requested(band_entity: int, kind: String)

var _band_labor: HudBandLaborState
## `(x, y) -> String` terrain label, injected by `Main` from the map; empty Callable until then.
var _terrain_label_lookup: Callable = Callable()

func _init(band_labor: HudBandLaborState) -> void:
    _band_labor = band_labor

func set_terrain_label_lookup(lookup: Callable) -> void:
    _terrain_label_lookup = lookup

## `Karst Highland (21, 14)` when the map names the terrain, else `(21, 14)`.
func site_name(x: int, y: int) -> String:
    var label := ""
    if _terrain_label_lookup.is_valid():
        label = String(_terrain_label_lookup.call(x, y))
    if label == "":
        return HudOrdersVocab.SITE_COORDS_FORMAT % [x, y]
    return HudOrdersVocab.SITE_NAME_FORMAT % [label, x, y]

# ---- Sends --------------------------------------------------------------------------------------

func _payload(band: Dictionary, kind: String, action: String) -> Dictionary:
    return {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
        "kind": kind,
        "action": action,
    }

## `band_order ... set <target> <priority>`.
func send_set(band: Dictionary, kind: String, target: int, priority: String) -> void:
    var payload := _payload(band, kind, HudOrdersVocab.ACTION_SET)
    payload["target"] = maxi(target, HudOrdersVocab.KEEP_MIN)
    payload["priority"] = priority
    band_order_requested.emit(payload)

## `band_order ... release [row]`: the player crews it. `row` is `{}` for the whole kind, `{x, y}` for a
## patch, `{herd_id}` for a herd.
func send_release(band: Dictionary, kind: String, row: Dictionary = {}) -> void:
    var payload := _payload(band, kind, HudOrdersVocab.ACTION_RELEASE)
    payload.merge(row)
    band_order_requested.emit(payload)

## `band_order ... claim [row]`: the chief takes it back.
func send_claim(band: Dictionary, kind: String, row: Dictionary = {}) -> void:
    var payload := _payload(band, kind, HudOrdersVocab.ACTION_CLAIM)
    payload.merge(row)
    band_order_requested.emit(payload)

func open_sheet(band: Dictionary, kind: String) -> void:
    order_sheet_requested.emit(int(band.get("entity", -1)), kind)

# ---- Lookups ------------------------------------------------------------------------------------

## The confirmed labor assignment behind a Work-tab row model (`kind`, `x`, `y`, `herd_id`,
## `material`), or `{}` for a row the wire does not hold yet (a pending one).
func assignment_of_model(band: Dictionary, model: Dictionary) -> Dictionary:
    var x := int(model.get("x", -1))
    var y := int(model.get("y", -1))
    match String(model.get("kind", "")):
        SourceForecast.LABOR_KIND_FORAGE:
            return _band_labor.forage_assignment_of(band, x, y)
        SourceForecast.LABOR_KIND_HUNT:
            return _band_labor.hunt_assignment_of(band, String(model.get("herd_id", "")))
        HudConst.LABOR_KIND_EXTRACT:
            return _band_labor.extract_assignment_of(band, x, y, String(model.get("material", "")))
    return {}

## The address a release / claim carries for a row model: `{herd_id}` or `{x, y}`.
static func row_address(model: Dictionary) -> Dictionary:
    var herd_id := String(model.get("herd_id", ""))
    if herd_id != "":
        return {"herd_id": herd_id}
    return {"x": int(model.get("x", -1)), "y": int(model.get("y", -1))}

## Which side of the Work tab a row model falls on: the order it is listed under (`kind`, the chief's),
## `YOURS` (a player-crewed row of an ordered kind) or `OTHER` (stays in today's sections).
const SIDE_ORDER := "order"
const SIDE_YOURS := "yours"
const SIDE_OTHER := "other"

func side_of(band: Dictionary, model: Dictionary) -> Dictionary:
    var assignment := assignment_of_model(band, model)
    if HudOrdersVocab.row_is_chiefs(assignment):
        return {"side": SIDE_ORDER, "kind": HudOrdersVocab.row_order_kind(assignment)}
    var model_kind := String(model.get("kind", ""))
    if model_kind == SourceForecast.LABOR_KIND_FORAGE or model_kind == SourceForecast.LABOR_KIND_HUNT:
        if not HudOrdersVocab.order_of(band, HudOrdersVocab.KIND_FOOD).is_empty():
            return {"side": SIDE_YOURS, "kind": ""}
    elif model_kind == HudConst.LABOR_KIND_EXTRACT:
        if not HudOrdersVocab.order_of(band, String(model.get("material", ""))).is_empty():
            return {"side": SIDE_YOURS, "kind": ""}
    return {"side": SIDE_OTHER, "kind": ""}

## The chief-owned order a SOURCE sheet's row sits under, `{}` when the row is not under one. For the
## hunt / gather sheets: `{order, chief_owned, kind}` where `chief_owned` says the row is his.
func order_for_source(band: Dictionary, source_kind: String, x: int, y: int, herd_id: String) -> Dictionary:
    if not HudOrdersVocab.has_leader(band):
        return {}
    var assignment: Dictionary
    if source_kind == SourceForecast.LABOR_KIND_HUNT:
        assignment = _band_labor.hunt_assignment_of(band, herd_id)
    else:
        assignment = _band_labor.forage_assignment_of(band, x, y)
    if HudOrdersVocab.row_is_chiefs(assignment):
        var kind := HudOrdersVocab.row_order_kind(assignment)
        return {"kind": kind, "chief_owned": true, "order": HudOrdersVocab.order_of(band, kind)}
    # A player row: the offer names the food order, the one a gathered or hunted source feeds.
    # ...or the fodder order when the row mostly brings in hay.
    var kind := HudOrdersVocab.KIND_FOOD
    if HudOrdersVocab.product_of(assignment, HudOrdersVocab.KIND_FODDER) \
            > HudOrdersVocab.product_of(assignment, HudOrdersVocab.KIND_FOOD):
        kind = HudOrdersVocab.KIND_FODDER
    var order := HudOrdersVocab.order_of(band, kind)
    if order.is_empty() or not HudOrdersVocab.is_chief_owned(order):
        return {}
    return {"kind": kind, "chief_owned": false, "order": order}
