class_name DisclosureController
extends RefCounted

## THE DETAIL-ROW DISCLOSURE CLUSTER (docs/plan_hud_decomposition.md) — the NODE-OWNING half of the
## detail-render layer, and the other end of the `[url]` meta `DetailFormat` emits.
##
## WHAT THIS IS. A summary row (Food / Morale) can be clicked to open its breakdown in a shared
## `PopupPanel`. This controller owns the whole of that: the per-render caret state every host's
## renderer reads, the stashed breakdown payloads, the popover node itself, and the click dispatch.
##
## WHY IT IS SPLIT FROM `DetailFormat` ALONG NODE OWNERSHIP RATHER THAN "FORMATTER VS POPOVER". The
## two are BIDIRECTIONALLY coupled — the formatter emits the `[url]` meta only this file can parse,
## and the popover renders THROUGH the formatter — so they ship together. What separates them is that
## this half lazily creates a `PopupPanel` and must `add_child` it, which a static module cannot do
## and a `RefCounted` cannot do either: hence the `popover_host` handed to `setup`, the
## `TurnOrbController` fork-panel pattern.
##
## THE POPOVER, NEVER INLINE — a correctness rule, not a style one. Expanding a breakdown in place
## grew the vitals `RichTextLabel` AFTER the Band panel had already picked its zone height tier, and
## the zone hosts `clip_contents`: the extra lines silently sliced the WORKFORCE row and ate both role
## cards. A Window cannot change a zone's height, which is the same reason the section menus are
## `MenuButton`s and the destructive confirms are `ConfirmationDialog`s.
##
## THE INBOUND RE-RENDER EDGE IS ONE INJECTED CALLABLE. Opening or closing the popover has to flip the
## caret in whichever hosts can be showing one — the Band/City panel's vitals label and the selection
## drawer. That pair is `HudLayer`'s knowledge, not this cluster's, so it arrives as the single
## `refresh_hosts` Callable rather than as two back-references.
##
## CONSTS. Same rule as `DetailFormat`: a const lives here iff every reader moved here. The popover's
## geometry did; `BREAKDOWN_TOGGLE_META_PREFIX` (read by the formatter and by both preview harnesses),
## `BREAKDOWN_KIND_*` and the `FOOD_LABEL_*` table did not, and are read back as `HudLayer.X`.

## The breakdown popover's card geometry. Fixed width so the rows align in a column like the table
## they came from; the GAP is how far under the clicked row the card floats.
const BREAKDOWN_POPOVER_WIDTH := 300.0
const BREAKDOWN_POPOVER_PADDING := 10
const BREAKDOWN_POPOVER_GAP := 4.0
## The four sides `BREAKDOWN_POPOVER_PADDING` is applied to.
const POPOVER_MARGIN_SIDES := ["left", "top", "right", "bottom"]
## No disclosure is open.
const NO_OPEN_KEY := ""

## The node the lazily-built popover is parented into (a `RefCounted` cannot parent).
var _popover_host: Node = null
## Re-render whichever hosts can be showing a disclosure caret, so it flips with the popover. Injected
## because WHICH hosts those are is HudLayer's knowledge, not this cluster's.
var _refresh_hosts: Callable = Callable()
## Per-render caret state: row-label → {key, open, concerning}. Read by `DetailFormat.detail_bbcode`
## through the render context (`state()` feeds `Context.disclosures`).
var _disclosure_state: Dictionary = {}
## The breakdown rows each disclosure would show, keyed `"<kind>:<entity>"` → Array[String]. Written
## every render by `register` and read by the popover, so the popover never recomputes a number and a
## click needs no band lookup — the meta carries the key. Deliberately NOT cleared with
## `_disclosure_state`: that one is per-render and per-host, and the other host's render must not be
## able to empty the payload behind an open popover.
var _breakdown_payloads: Dictionary = {}
## The disclosure key whose popover is currently up, `NO_OPEN_KEY` = none. It is what "open" means
## now, so it is also what flips the row's caret.
var _breakdown_popover_key: String = NO_OPEN_KEY
## The one popover both hosts share, built lazily on the first disclosure click.
var _breakdown_popover: PopupPanel = null
var _breakdown_popover_label: RichTextLabel = null


## Hand over the node the popover parents into and the one re-render edge back into HudLayer.
func setup(popover_host: Node, refresh_hosts: Callable) -> void:
    _popover_host = popover_host
    _refresh_hosts = refresh_hosts

## Make a detail label's `[url]` metas clickable. Called for BOTH hosts — the selection drawer's
## long-lived `%OccupantDetail` and the Band panel's per-render vitals label — each binding ITSELF as
## the anchor the popover floats under.
func wire_label(label: RichTextLabel) -> void:
    if label == null:
        return
    label.meta_clicked.connect(_on_meta_clicked.bind(label))

## The caret state for the render about to happen — what feeds `DetailFormat.Context.disclosures`.
func state() -> Dictionary:
    return _disclosure_state

## Drop the previous render's carets. Called at the top of the band line producer, NOT inside the Food
## row builder — a foreign band skips that call entirely, and a skipped Food row must not inherit the
## previous render's caret.
func clear_rows() -> void:
    _disclosure_state = {}

## Register a summary row (`row_label`, e.g. "Food"/"Morale") as a click-to-open disclosure: stash the
## rows its popover will show and record the caret state for `DetailFormat.detail_bbcode`. Returns
## whether the affordance is offered at all — a row with nothing to show gets no caret. Shared by both
## disclosure rows and by BOTH hosts (the panel's vitals label and the Occupants-card drawer), which
## is the point: one click behaviour, no `is_panel` fork.
func register(row_label: String, kind: String, band: Dictionary, lines: Array[String]) -> bool:
    if lines.is_empty():
        return false
    var key := DetailFormat.breakdown_key(kind, band)
    _breakdown_payloads[key] = lines
    var concerning := DetailFormat.food_is_concerning(band) if kind == HudDisclosureVocab.BREAKDOWN_KIND_FOOD \
        else DetailFormat.morale_is_concerning(band)
    _disclosure_state[row_label] = {"key": key, "open": _breakdown_popover_key == key, "concerning": concerning}
    # A live popover restates the numbers it was opened on, so a snapshot refreshes it in place.
    if _breakdown_popover_key == key:
        _refresh_popover_text()
    return true

## The category breakdown sub-lines under Food, one indented row per present category, mirroring the
## morale breakdown: `    ▲ +0.48  Gathered` / `    ▲ +0.46  Hunted` / `    ▼ −0.68  Eaten (people)`
## / `    ▼ −1.74  🐄 Pen feed (animals)` (income ▲ green, debits ▼ amber via the shared
## indented-sub-line tint). Only categories above the floor — a band with no pen shows no feed row.
##
## THREE kinds of row, not two: the pen's feed is a debit on the same larder as the people's meals,
## but it is a DIFFERENT decision (shrink the herd vs starve the band), so it gets its own line.
func food_breakdown_lines(band: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var gathered := DetailFormat.sum_realized_yield(band, SourceForecast.LABOR_KIND_FORAGE)
    if gathered >= SourceForecast.FOOD_FLOW_MIN:
        lines.append(DetailFormat.food_breakdown_row(gathered, DetailFormat.FOOD_LABEL_GATHERED))
    var hunted := DetailFormat.sum_realized_yield(band, SourceForecast.LABOR_KIND_HUNT)
    if hunted >= SourceForecast.FOOD_FLOW_MIN:
        lines.append(DetailFormat.food_breakdown_row(hunted, DetailFormat.FOOD_LABEL_HUNTED))
    var eaten := float(band.get("food_consumption", 0.0))
    if eaten >= SourceForecast.FOOD_FLOW_MIN:
        lines.append(DetailFormat.food_breakdown_row(-eaten, DetailFormat.FOOD_LABEL_EATEN))
    var pen_feed := DetailFormat.band_pen_feed(band)
    if pen_feed >= SourceForecast.FOOD_FLOW_MIN:
        lines.append(DetailFormat.food_breakdown_row(-pen_feed, DetailFormat.FOOD_LABEL_PEN_FEED))
    return lines

## Meta dispatcher for the summary-row disclosures (Food/Morale): the `[url]` meta IS the disclosure
## key, so the handler needs no band lookup and no host flag — the SAME click behaviour wherever the
## row renders. `anchor` is the label that emitted the click, bound at wire time; it is what the
## popover positions under.
func _on_meta_clicked(meta: Variant, anchor: Control) -> void:
    var payload := String(meta)
    if not payload.begins_with(HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX):
        return
    var key := payload.substr(HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX.length())
    if _breakdown_popover_key == key:
        _close_popover()
        return
    _open_popover(key, anchor)

## Open a disclosure's breakdown in the popover, anchored under the clicked row. The anchor rect is
## captured BEFORE the hosts re-render, because that render frees the very label we are anchoring to
## (the panel builds a fresh vitals label each time).
func _open_popover(key: String, anchor: Control) -> void:
    var lines := _lines_for(key)
    if lines.is_empty():
        return
    var anchor_rect := _anchor_rect(anchor)
    _breakdown_popover_key = key
    _notify_hosts()
    var popover := _ensure_popover()
    _refresh_popover_text()
    popover.popup(anchor_rect)

## Dismiss the breakdown popover, if any. Idempotent — `popup_hide` runs the same teardown, so a
## click-away / Esc and an explicit close converge on one path.
func _close_popover() -> void:
    if _breakdown_popover != null and _breakdown_popover.visible:
        _breakdown_popover.hide()
        return
    _on_popover_hidden()

func _on_popover_hidden() -> void:
    if _breakdown_popover_key == NO_OPEN_KEY:
        return
    _breakdown_popover_key = NO_OPEN_KEY
    _notify_hosts()

## The rows a disclosure key's popover shows — stashed by `register`, never recomputed.
func _lines_for(key: String) -> Array[String]:
    var stored: Variant = _breakdown_payloads.get(key, null)
    var lines: Array[String] = []
    if stored is Array:
        for line in (stored as Array):
            lines.append(String(line))
    return lines

## Where the popover sits: a zero-height rect at the bottom-left of the clicked row, in SCREEN space
## (what `Popup.popup` wants). `get_screen_transform` folds in the window position and the canvas
## stretch, both of which this HUD has.
func _anchor_rect(anchor: Control) -> Rect2i:
    if anchor == null or not is_instance_valid(anchor):
        return Rect2i()
    var xform := anchor.get_screen_transform()
    var below := xform * Vector2(0.0, anchor.size.y + BREAKDOWN_POPOVER_GAP)
    return Rect2i(Vector2i(below), Vector2i.ZERO)

## The popover itself: a `PopupPanel`, so it is a WINDOW — it cannot change any zone's height, which
## is the whole reason the breakdown moved here. Styled through `HudStyle` like every other card.
func _ensure_popover() -> PopupPanel:
    if _breakdown_popover != null and is_instance_valid(_breakdown_popover):
        return _breakdown_popover
    var popover := PopupPanel.new()
    popover.name = "BreakdownPopover"
    popover.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
    var margin := MarginContainer.new()
    for side in POPOVER_MARGIN_SIDES:
        margin.add_theme_constant_override("margin_%s" % side, BREAKDOWN_POPOVER_PADDING)
    popover.add_child(margin)
    var label := RichTextLabel.new()
    label.bbcode_enabled = true
    label.fit_content = true
    label.scroll_active = false
    label.autowrap_mode = TextServer.AUTOWRAP_WORD
    label.custom_minimum_size = Vector2(BREAKDOWN_POPOVER_WIDTH, 0.0)
    margin.add_child(label)
    popover.popup_hide.connect(_on_popover_hidden)
    _popover_host.add_child(popover)
    _breakdown_popover = popover
    _breakdown_popover_label = label
    return popover

## Restate the open popover from the current payload — the breakdown CONTENT is unchanged from the
## inline form: the same indented ▲/▼ rows through the same shared two-tone tint path. It carries no
## band tint context of its own: every row it shows is an indented sub-line, which the formatter tints
## by sign glyph alone.
func _refresh_popover_text() -> void:
    if _breakdown_popover_label == null or not is_instance_valid(_breakdown_popover_label):
        return
    _breakdown_popover_label.text = DetailFormat.detail_bbcode(_lines_for(_breakdown_popover_key))

## Re-render whichever hosts can be showing a disclosure caret, so it flips with the popover. Both
## hosts, unconditionally — that is the `is_panel` fork this cluster exists to remove.
func _notify_hosts() -> void:
    if _refresh_hosts.is_valid():
        _refresh_hosts.call()
