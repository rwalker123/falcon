class_name DockRowController
extends RefCounted

## Reflows the HUD's bottom-bar chrome (nav cluster + turn cluster) INTO the Band/City panel's
## reserved row whenever that panel docks to a horizontal edge (issue #324).
##
## WHY IT EXISTS: `Hud.set_reserved_inset` insets `LayoutRoot`, so a T/B dock pushes the whole
## `BottomBar` clear of the panel and stacks it — costing `ContentRow` ~164px on top of the panel's
## own height, which is what drove the terrain panel to scroll bars. Relocating the minimap alone
## recovers ~14px (the turn cluster's 128 and `BottomBar`'s authored 150 hold the floor), so the whole
## bar has to vacate for the budget to actually come back.
##
## THE CHROME SPLITS TO THE ROW'S TWO ENDS WHERE THERE IS ROOM FOR IT — nav cluster (minimap + zoom
## rail) at the LEADING end, turn cluster at the TRAILING end — which is where they sit when the panel
## is docked at the TOP and `BottomBar` stays intact. Below the fit gate they stack in ONE column at the
## trailing end instead, nav above turn, which is what every bottom dock used to do.
##
## ⛔ **ONE COLUMN WAS NOT A PREFERENCE, IT WAS A MEASURED REJECTION — quoted rather than deleted,
## because it was right when it was made and a reader who finds only an absence will re-stack this**:
## *"A gutter at each end was built first and rejected on sight with a real minimap in it: the left rail
## is ~300px, so two opposite gutters pushed the band zone inward AND stranded dead space around the orb,
## costing ~562px of row. One column costs `max(nav, turn)` ≈ 296–302 … That hands the zones ~240px back
## and drops the wide->narrow flip from ~1605px of window width to ~1377px."*
##
## **TWO THINGS MOVED, AND BOTH ARE WHY THAT NO LONGER DECIDES IT.** (1) The card stopped wanting the
## whole row: issue #377 builds its width up from the declared column count and floats it as an ISLAND,
## so room beside it is slack over live map rather than width taken off the zones — the "pushed the band
## zone inward" term is simply gone. (2) The TURN cluster shrank from 260 wide to 116 when its `Turn N`
## caption moved into the orb face, so the second gutter is not a second ~300px rail. Re-measured, the
## split costs `turn + RAIL_SEPARATOR_SPAN` MORE of row than the stack — not the ~562 of two nav rails.
##
## **THE GATE IS THE PANEL'S, AND IT IS `wide_shell_min_width()`** (`BandCityPanel._rail_split`): split
## while both islands plus the card's three-zone minimum still fit the strip, stack when they do not. So
## the old rejection still governs every window too narrow for it, and — because the split is refused
## exactly when it would cost the wide shell — **the wide->narrow flip width is unmoved.**
##
## **THIS CONTROLLER DOES NOT CHOOSE THE ARRANGEMENT**, and that split of duties is the existing one:
## the HUD owns the chrome and MEASURES it (`set_rail_widths` declares both clusters' widths), the panel
## owns the empty slots and decides where they sit. The park itself is unchanged — nav into
## `RAIL_SLOT_TOP`, turn into `RAIL_SLOT_BOTTOM` — and the panel moves the slot HOST between its two
## islands with the parked cluster still inside it.
##
## A TOP dock never reflows at all (`REFLOW_EDGES` is `[SIDE_BOTTOM]`), so its minimap and orb stay in
## the intact `BottomBar` where the scene put them; a vertical dock keeps the stacked column's rules for
## the same reason it always did.
##
## OWNERSHIP: the HUD owns the chrome NODES and this controller does all the reparenting; the panel
## owns only the empty rail SLOTS it parks them into. That is the exact opposite of `set_zones`, which
## takes ownership of (and frees) what it is handed — do NOT "unify" the two.

## The dock edges the chrome relocates for. **BOTTOM ONLY** — and the TOP edge's removal is a fix, not a
## narrowing (issue #377).
##
## The whole reason for relocating (see the header) is that `Hud.set_reserved_inset` insets `LayoutRoot`,
## so the panel pushes `BottomBar` clear of itself and STACKS it, costing `ContentRow` the bar's height on
## top of the panel's own. **That only happens on a BOTTOM dock**, where the inset and the bar are on the
## same edge. A TOP dock insets the top; `BottomBar` sits at the bottom of `RootColumn` and never moves,
## so there was nothing to recover — and relocating anyway dragged the minimap and turn orb to the TOP of
## the screen, where the player has to hunt for chrome that has a fixed home. Shipping `[SIDE_TOP,
## SIDE_BOTTOM]` generalised a bottom-edge problem to both horizontal edges by symmetry rather than by
## measurement.
##
## A vertical dock is a `PANEL_WIDTH` strip with no room beside its zones, and keeps today's stacking.
const REFLOW_EDGES: Array[int] = [SIDE_BOTTOM]
## What the card's own vertical chrome costs, folded into the fit gate — see `_required_height`. Read off
## the panel's public consts rather than duplicated, so the two cannot drift.
const CARD_CHROME_V := 2.0 * (float(BandCityPanel.PANEL_CONTENT_MARGIN_V) + BandCityPanel.PANEL_BORDER_WIDTH)

var _bottom_bar: HBoxContainer = null
var _nav: Control = null
var _turn: Control = null
var _panel: BandCityPanel = null
## `BottomBar`'s authored `custom_minimum_size.y` from `HudLayer.tscn`, captured at construction. The
## scene is the single source of truth for it — never hardcode the number here.
var _bottom_bar_min_height: float = 0.0
## Where each cluster LIVES when the bar is intact: `[{node, parent, index, anchors, offsets, flags}]`,
## captured before any reflow so the return is exact. A preset applied on park must not leak into the
## un-reflowed layout, so the anchors/offsets/size-flags travel with the parent + child index.
var _home: Array = []
## True while the chrome is parked in the panel's rails. Guards both transitions, so repeated `apply`
## calls with the same verdict are no-ops.
var _is_reflowed: bool = false

func _init(bottom_bar: HBoxContainer, nav_backing: Control, turn_cluster: Control) -> void:
	_bottom_bar = bottom_bar
	_nav = nav_backing
	_turn = turn_cluster
	if _bottom_bar != null:
		_bottom_bar_min_height = _bottom_bar.custom_minimum_size.y
	for cluster in [_nav, _turn]:
		_capture_home(cluster)
	# The minimap's width AND height are a function of the map's grid aspect
	# (`MinimapPanel.resize_to_aspect` scales `embedded_height × aspect`, clamped into
	# [min_width, max_width]), so the nav cluster's minimum CHANGES on a new map. While reflowed,
	# re-declare the rail width whenever either cluster's minimum moves. Connected once,
	# `is_connected`-guarded; `BandCityPanel.set_rail_widths` no-ops on an unchanged pair, so this
	# cannot churn. The stack's HEIGHT needs no push — the slot containers read their own child's
	# minimum, so the column re-centres itself.
	for cluster in [_nav, _turn]:
		if cluster != null and not cluster.minimum_size_changed.is_connected(_push_rail_width):
			cluster.minimum_size_changed.connect(_push_rail_width)

## Injected by `Hud.set_band_city_panel`, the same seam `BandPanelController` is handed the panel
## through. A null panel means "never reflow".
func set_panel(panel: BandCityPanel) -> void:
	_panel = panel

## React to the Band/City panel's reservation. `edge` is a Godot Side const; `size` is the reserved
## cross-axis size (0 when hidden, `COLLAPSED_SIZE` when railed).
func apply(edge: int, size: float) -> void:
	var wants_reflow := parks_for(edge, size)
	if wants_reflow == _is_reflowed:
		return
	if wants_reflow:
		_park()
	else:
		_restore()

## **WILL the chrome park for this (edge, size)?** `apply`'s own verdict, exposed as a pure function so
## `Main.band_dock_overlays_hud` can ask it BEFORE the reflow has happened — the two are separate
## listeners on the same `reservation_changed`, and `Main`'s runs first, so a state read (`_is_reflowed`)
## would answer for the previous reservation. Being a function of (edge, size) and the chrome's own
## minimums, it gives both callers the same answer whichever order they arrive in.
##
## It is what makes the HUD's bottom-edge yield honest: the strip is only free of HUD furniture once the
## chrome has moved into the card's rail. A panel collapsed to its 46px rail, a hidden one, and a window
## too short for the stack all decline here, and in every one of them `BottomBar` is still in the row.
func parks_for(edge: int, size: float) -> bool:
	return _panel != null and REFLOW_EDGES.has(edge) and size >= _required_height()

## The rail width this controller WOULD declare at that verdict — `_push_rail_width`'s number, answered
## before it is pushed, and 0 when the chrome stays home (which is what `_restore` declares). Read by
## `Main.band_dock_overlays_hud` for the same order-independence reason as `parks_for`.
func rail_width_for(edge: int, size: float) -> float:
	return _measured_rail_width() if parks_for(edge, size) else 0.0

## The chrome only fits if the reserved strip can hold the whole STACK — both clusters plus the gap
## between them plus the card's own vertical chrome. This ONE test covers every case that must fall back
## to the stacked layout — the panel collapsed to its rail (46px), the panel hidden (0), and a window
## short enough that `MAX_WIDE_HEIGHT_FRACTION` clamps the strip below the chrome — so none of them
## needs its own branch. It measures a COLUMN now rather than the taller of two side-by-side rails.
## `CARD_CHROME_V` is in it deliberately: the gate is compared against the RESERVED cross-axis size,
## which includes the card's border + content margins, so leaving it out would pass a strip whose
## interior cannot actually hold the stack (a short window is exactly the case being tested).
func _required_height() -> float:
	var nav_height := 0.0 if _nav == null else _nav.get_combined_minimum_size().y
	var turn_height := 0.0 if _turn == null else _turn.get_combined_minimum_size().y
	return nav_height + turn_height + float(BandCityPanel.RAIL_SLOT_SEPARATION) + CARD_CHROME_V

## Park the chrome into the panel's rails and drop `BottomBar` out of layout so `ContentRow`
## reclaims its height.
func _park() -> void:
	_is_reflowed = true
	# Declare the width FIRST, so the rail is sized and visible before its clusters arrive.
	_push_rail_width()
	_move_into(_nav, _panel.rail_slot_host(BandCityPanel.RAIL_SLOT_TOP))
	_move_into(_turn, _panel.rail_slot_host(BandCityPanel.RAIL_SLOT_BOTTOM))
	if _bottom_bar != null:
		# BOTH: a zero-minimum but VISIBLE HBox still participates in `RootColumn`'s separation.
		_bottom_bar.custom_minimum_size.y = 0.0
		_bottom_bar.visible = false

## Hand the chrome back to `BottomBar`, exactly where the scene authored it.
func _restore() -> void:
	_is_reflowed = false
	# Retire the rail FIRST, so the panel re-chooses its shell against the full row before the clusters
	# leave (and the rail is hidden and zero-width when they do).
	if _panel != null:
		_panel.set_rail_widths(0.0, 0.0)
	for entry_variant in _home:
		var entry: Dictionary = entry_variant
		_move_home(entry)
	if _bottom_bar != null:
		_bottom_bar.custom_minimum_size.y = _bottom_bar_min_height
		_bottom_bar.visible = true

## Declare BOTH clusters' widths. The HUD owns the chrome, so the HUD is what measures it — the panel
## only ever stores what it is told, and it is the panel that decides whether the two go to opposite
## ends or into one column (`BandCityPanel._rail_split`). Declaring the PAIR rather than the `max` is
## what makes that decision possible without the panel measuring anything. Also the
## `minimum_size_changed` handler (a new map resizes the minimap), and a no-op unless the chrome is
## actually parked.
func _push_rail_width() -> void:
	if not _is_reflowed or _panel == null:
		return
	_panel.set_rail_widths(_cluster_width(_nav), _cluster_width(_turn))

## One cluster's declared minimum width, or 0 when it is absent.
func _cluster_width(cluster: Control) -> float:
	return 0.0 if cluster == null else cluster.get_combined_minimum_size().x

## The STACKED arrangement's rail width — the wider of the two clusters.
##
## ⛔ **IT IS THE STACKED FIGURE EVEN WHERE THE ROW SPLITS, AND THAT IS SOUND RATHER THAN STALE.** Its
## one reader is `rail_width_for`, which feeds `BandCityPanel.affords_wide_shell_with_bounds` — a
## question about whether the card can still stand in its three-zone shell. The stacked span is the
## SMALLER of the two arrangements, and the panel splits only where the LARGER one still clears
## `wide_shell_min_width()`; so wherever this predicate answers "affords", the arrangement the panel
## actually picks affords it too. Reporting the split total here would make the predicate refuse the
## HUD's yield on windows where the panel would simply have stacked instead.
func _measured_rail_width() -> float:
	return maxf(_cluster_width(_nav), _cluster_width(_turn))

## Reparent a cluster into its rail slot. **No anchors, offsets or presets are written here, and that is
## the point**: a slot is a `MarginContainer`, so it lays the cluster out and the panel's
## `ALIGNMENT_CENTER` stack centres the pair in the strip. `PRESET_FULL_RECT` would stretch the cluster
## over the whole slot, and the tempting `set_anchors_and_offsets_preset(PRESET_HCENTER_WIDE)` silently
## does NOT centre a plain `Control` — see `BandCityPanel._build_rail`'s note 3 for the measured proof.
## The park DOES overwrite the cluster's size flags (to centre it across a column wider than it is), so
## `_home` captures and restores them.
func _move_into(cluster: Control, host: Control) -> void:
	if cluster == null or host == null:
		return
	var parent := cluster.get_parent()
	if parent == host:
		return
	if parent != null:
		parent.remove_child(cluster)
	host.add_child(cluster)
	cluster.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	cluster.size_flags_vertical = Control.SIZE_SHRINK_CENTER

## Record where a cluster lives while `BottomBar` is intact, plus the layout properties a park
## overwrites, so `_restore` is exact rather than approximate.
func _capture_home(cluster: Control) -> void:
	if cluster == null or cluster.get_parent() == null:
		return
	_home.append({
		"node": cluster,
		"parent": cluster.get_parent(),
		"index": cluster.get_index(),
		"anchors": [cluster.anchor_left, cluster.anchor_top, cluster.anchor_right, cluster.anchor_bottom],
		"offsets": [cluster.offset_left, cluster.offset_top, cluster.offset_right, cluster.offset_bottom],
		"flags": [cluster.size_flags_horizontal, cluster.size_flags_vertical],
	})

## Put one cluster back at its authored parent + child INDEX and restore the layout properties the
## park preset overwrote. `_home` is captured in `BottomBar` order, so replaying it in order lands
## every cluster at the index the scene gave it.
func _move_home(entry: Dictionary) -> void:
	var cluster: Control = entry["node"]
	var parent: Node = entry["parent"]
	if cluster == null or parent == null:
		return
	var current := cluster.get_parent()
	if current != parent:
		if current != null:
			current.remove_child(cluster)
		parent.add_child(cluster)
	parent.move_child(cluster, int(entry["index"]))
	var anchors: Array = entry["anchors"]
	cluster.anchor_left = anchors[0]
	cluster.anchor_top = anchors[1]
	cluster.anchor_right = anchors[2]
	cluster.anchor_bottom = anchors[3]
	var offsets: Array = entry["offsets"]
	cluster.offset_left = offsets[0]
	cluster.offset_top = offsets[1]
	cluster.offset_right = offsets[2]
	cluster.offset_bottom = offsets[3]
	var flags: Array = entry["flags"]
	cluster.size_flags_horizontal = flags[0]
	cluster.size_flags_vertical = flags[1]
