class_name SelectionCardController
extends RefCounted

## The IDENTITY/LIST half of the selection card (HUD decomposition Phase 2b, docs/plan_hud_decomposition.md):
## the tile-card header, the pinned condition-chip strip, and the whole roster / subject list — plus the
## roster-row clicks and the fresh-hex auto-select. It is the STATE-ISOLATED half — zero drawer coupling,
## zero shared compose/band-tint state — so it split off the monolith cleanly ahead of the drawer (Phase 2c).
##
## Built on the LegendController / TopBarReadouts / TurnOrbController idiom: HudLayer holds one as
## `_selectioncard`, hands it the three card nodes + the two shared `RefCounted` state models (BY REFERENCE —
## both HudLayer and this controller hold the SAME `HudSelectionState` / `HudBandLaborState` instances), and
## RELAYS this controller's own `roster_occupant_selected` onto the HudLayer signal Main connects to.
##
## The row-click seam: a click changes the LIT subject, which must re-render BOTH the list accent (here) AND
## the drawer (HudLayer). So a click mutates `_selection` and emits `subject_changed`; HudLayer connects it and
## drives the whole re-render (close the compose sheet → `_render_selection_panel`, which calls back into
## `render()` + then the drawer). The auto-pick emits only `roster_occupant_selected` (it is already mid-render).
##
## The Phase-2a in-place diff travels intact: the chip / row updaters and their `_tile_chip_slots` /
## `_subject_row_keys` caches move WITH the code, so a same-tile restate still patches nodes in place rather
## than tearing them down (the flash `tile_panel_no_flash` guards).

# --- The controller's OWN signals (HudLayer connects each; see the class header) ---
# The lit subject changed via a roster/land CLICK — HudLayer closes the compose sheet + re-renders the panel.
signal subject_changed
# A roster subject was selected — relayed onto HudLayer.roster_occupant_selected → Main → MapView.select_occupant.
signal roster_occupant_selected(kind: String, id: Variant)

# --- Collaborators handed in by HudLayer ---
var _tile_panel: PanelCard = null
var _tile_chips: HFlowContainer = null
var _subject_list: VBoxContainer = null
# The SAME instances HudLayer holds — pure data, no scene refs. This controller reads + mutates them.
var _selection: HudSelectionState = null
var _band_labor: HudBandLaborState = null
# --- Owned Phase-2a in-place-diff caches (moved off HudLayer) ---
# The ordered chip-slot keys + the ordered roster-row keys (identity + structural flags) last rendered, so an
# unchanged restate patches the existing nodes rather than freeing + rebuilding them (the reflow that flashes).
var _tile_chip_slots: Array = []
var _subject_row_keys: Array = []

func _init(tile_panel: PanelCard, tile_chips: HFlowContainer, subject_list: VBoxContainer,
		selection: HudSelectionState, band_labor: HudBandLaborState) -> void:
	_tile_panel = tile_panel
	_tile_chips = tile_chips
	_subject_list = subject_list
	_selection = selection
	_band_labor = band_labor

## The identity/list render, driven from HudLayer._render_selection_panel: assemble the hex's roster, draw the
## tile-card header + chips, auto-select the fresh-hex subject, and (re)build the subject list. The DRAWER is
## rendered by HudLayer afterwards — this owns everything above it. HudLayer already guards on the drawer nodes
## before calling in, so this needs no `tile_detail` check.
func render(tile_info: Dictionary) -> void:
	if _tile_panel == null:
		return
	_assemble_roster(tile_info)
	_render_tile_card(tile_info)
	_resolve_auto_selected_subject()
	_rebuild_subject_list()

## Assemble the roster for the current hex from the tile's `units`/`herds`, then
## ensure the currently-selected occupant is represented even when the tile_info
## doesn't list it (an inspector-driven herd selection carries an empty tile_info).
func _assemble_roster(tile_info: Dictionary) -> void:
	# Reset then build in place: the roster arrays are mutated by reference (like the old members),
	# so `find_roster_unit` / `find_roster_herd` see the in-progress roster during assembly.
	_selection.set_roster([], [])
	# Occupants are LIVE state, so on a hex the player cannot currently see they are redacted — MapView
	# fog-gates them out of `tile_info` at source, and this re-reads the SAME state flag it tagged (not
	# a second visibility test) so the roster stays honest no matter who feeds it.
	# THE ONE EXCEPTION: your OWN bands are always listed, even on an Unexplored hex. A scouting party
	# is deliberately excluded from fog reveal server-side, so it ROUTINELY stands on a tile it cannot
	# see — hiding it would delete your own expedition from the roster exactly while you're using it.
	# Mirrors `MapView._unit_hidden_by_fog`, which is the same rule for the map/click side.
	var unseen := tile_contents_unseen(tile_info)
	var units_variant: Variant = tile_info.get("units", [])
	if units_variant is Array:
		for entry in units_variant:
			if entry is Dictionary and (not unseen or _is_player_unit(entry as Dictionary)):
				_selection.roster_units().append(entry)
	# Wildlife is never ours — an unseen hex lists no herds at all.
	if not unseen:
		var herds_variant: Variant = tile_info.get("herds", [])
		if herds_variant is Array:
			for entry in herds_variant:
				if entry is Dictionary:
					_selection.roster_herds().append(entry)
	if _selection.has_unit() and find_roster_unit(int(_selection.unit().get("entity", -1))).is_empty():
		_selection.roster_units().append(_selection.unit())
	if _selection.has_herd() and find_roster_herd(String(_selection.herd().get("id", ""))).is_empty():
		_selection.roster_herds().append(_selection.herd())

## The card's chrome: the coordinates as the title, and the pinned chip strip. The terrain ROWS
## are no longer here — they are the land subject's drawer content, rendered by HudLayer's
## `_render_subject_drawer` when the land row is the lit one.
func _render_tile_card(tile_info: Dictionary) -> void:
	if _tile_panel == null:
		return
	_tile_panel.visible = true
	_tile_panel.set_card_kind("Tile")
	var title_text := "—"
	if not tile_info.is_empty():
		title_text = "(%d, %d)" % [int(tile_info.get("x", -1)), int(tile_info.get("y", -1))]
	_tile_panel.set_card_title(title_text)
	_build_tile_chips(tile_info)

## The sight state in plain words — the FULL form, which is the chip's tooltip. "" (FoW off) yields
## no chip at all.
func _tile_sight_value(visibility_state: String) -> String:
	match visibility_state:
		HudLayer.VISIBILITY_ACTIVE:
			return HudLayer.TILE_SIGHT_ACTIVE
		HudLayer.VISIBILITY_DISCOVERED:
			return HudLayer.TILE_SIGHT_REMEMBERED
		HudLayer.VISIBILITY_UNEXPLORED:
			return HudLayer.TILE_SIGHT_UNEXPLORED
		_:
			return ""

## The chip FACE: one or two words. Only the remembered state has a long form to shorten — the
## other two already ARE their short form, so they pass through and cannot drift out of step.
func _tile_sight_chip_value(value: String) -> String:
	return HudLayer.TILE_SIGHT_REMEMBERED_SHORT if value == HudLayer.TILE_SIGHT_REMEMBERED else value

## Value tint for the Sight chip: in-sight reads live (SIGNAL cyan — the HUD's "this is current" color),
## while both unseen states read dim (INK_DIM). The chip states what you KNOW, not what is wrong, so it
## never borrows the WARN/DANGER palette. (The BBCode-hex twin `DetailFormat.sight_value_hex` lives with
## `DetailFormat.detail_bbcode`, the shared key→tint registry that still consults it.)
func _sight_value_color(value: String) -> Color:
	return HudStyle.SIGNAL if value == HudLayer.TILE_SIGHT_ACTIVE else HudStyle.INK_DIM

# ---- The chip strip ---------------------------------------------------------
# Chips carry the tile's STANDING CONDITION — the one-word states you reason with while composing
# an action — pinned above the list so they never scroll away. Numbers stay as rows in the land
# drawer, where their subject is. Each chip is SKIPPED when its field is absent, exactly as the
# equivalent row is: a rehydrated tile must never show an invented rating.
func _build_tile_chips(tile_info: Dictionary) -> void:
	if _tile_chips == null:
		return
	if tile_info.is_empty():
		for child in _tile_chips.get_children():
			child.queue_free()
		_tile_chip_slots = []
		_tile_chips.visible = false
		return
	_tile_chips.visible = true
	var descriptors := _tile_chip_descriptors(tile_info)
	var slots: Array = []
	for descriptor in descriptors:
		slots.append(descriptor["key"])
	# Same SET of chip slots as last render → patch each chip's face/tint/tooltip in place, so a
	# per-snapshot restate never frees + recreates the strip (the reflow that flashes). A slot
	# appearing or disappearing moves the signature → full rebuild.
	if slots == _tile_chip_slots and _tile_chips.get_child_count() == descriptors.size():
		for i in range(descriptors.size()):
			_update_chip(_tile_chips.get_child(i) as PanelContainer, descriptors[i])
		return
	for child in _tile_chips.get_children():
		child.queue_free()
	for descriptor in descriptors:
		_tile_chips.add_child(_make_chip(descriptor["text"], descriptor["tint"], descriptor["tooltip"]))
	_tile_chip_slots = slots

## The ordered chip descriptors for a tile — one entry per PRESENT slot, each carrying a stable `key`
## (so a render can diff the SET of slots) plus the face/tint/tooltip the chip shows. Mirrors the
## build order EXACTLY: sight → habitability → climate → tags → site, each skipped when its field is
## absent, exactly as the equivalent row is.
func _tile_chip_descriptors(tile_info: Dictionary) -> Array:
	var out: Array = []
	var visibility_state := String(tile_info.get("visibility_state", ""))
	var sight_value := _tile_sight_value(visibility_state)
	if sight_value != "":
		# Short face, full sentence on hover — same value behind both, so they cannot disagree.
		out.append({"key": "sight", "text": _tile_sight_chip_value(sight_value),
			"tint": _sight_value_color(sight_value), "tooltip": sight_value})
	# Nothing else is knowable on ground nobody has stood on — not even its biome.
	if visibility_state == HudLayer.VISIBILITY_UNEXPLORED:
		return out
	if tile_info.has("habitability"):
		var habitability := float(tile_info["habitability"])
		out.append({"key": "habitability", "text": TileHabitability.rating_for(habitability),
			"tint": TileHabitability.color_for(habitability), "tooltip": ""})
	# Climate is INFORMATIONAL, so it wears neutral ink and never the warning palette; the cut
	# points are the SIM's, so until they are published there is no chip rather than a guess.
	if tile_info.has("temperature") and TileClimate.has_bands():
		out.append({"key": "climate", "text": TileClimate.band_for(float(tile_info["temperature"])),
			"tint": HudStyle.INK_DIM, "tooltip": ""})
	var tags_text := String(tile_info.get("tags_text", "")).strip_edges()
	if tags_text != "" and tags_text.to_lower() != HudLayer.CHIP_TAGS_NONE:
		out.append({"key": "tags", "text": tags_text, "tint": HudStyle.INK_DIM, "tooltip": ""})
	var site_name := String(tile_info.get("site_name", "")).strip_edges()
	if site_name != "":
		out.append({"key": "site", "text": site_name, "tint": HudStyle.INK_DIM, "tooltip": ""})
	return out

## Patch an existing chip in place to a new descriptor — the same node the slot held last render, so
## a restate updates the FACE without a teardown. Mirrors `_make_chip`'s tooltip/mouse-filter rule.
func _update_chip(chip: PanelContainer, descriptor: Dictionary) -> void:
	if chip == null:
		return
	var tint: Color = descriptor["tint"]
	var text: String = descriptor["text"]
	var tooltip: String = descriptor["tooltip"]
	chip.add_theme_stylebox_override("panel", HudStyle.chip_stylebox(tint))
	if tooltip != "" and tooltip != text:
		chip.tooltip_text = tooltip
		chip.mouse_filter = Control.MOUSE_FILTER_STOP
	else:
		chip.tooltip_text = ""
		chip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var label := chip.get_child(0) as Label
	if label != null:
		label.text = text
		label.add_theme_color_override("font_color", tint)

## One chip: a pill wearing the palette's chip chrome, tinted by the condition it states. An
## optional `tooltip` carries the long form of a condition whose face had to be short; a chip
## without one stays mouse-transparent, exactly as before.
func _make_chip(text: String, tint: Color, tooltip: String = "") -> PanelContainer:
	var chip := PanelContainer.new()
	chip.add_theme_stylebox_override("panel", HudStyle.chip_stylebox(tint))
	chip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	if tooltip != "" and tooltip != text:
		chip.tooltip_text = tooltip
		chip.mouse_filter = Control.MOUSE_FILTER_STOP
	var label := Label.new()
	label.text = text
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.add_theme_color_override("font_color", tint)
	label.add_theme_font_size_override("font_size", HudLayer.CHIP_FONT_SIZE)
	chip.add_child(label)
	return chip

## True when the hex's LIVE contents (occupants, workable sources) are unknowable right now — a
## remembered or a never-seen tile. MapView already redacts them from `tile_info` at source (it strips
## `herds`/`units`/`food_module*` and fog-gates `_herds_on_tile`); this re-reads the SAME state flag it
## tagged — not a second visibility test — so every consumer stays honest regardless of who feeds it.
## Terrain rows are exempt by design: geography is remembered knowledge, live contents are not. Public
## because HudLayer's drawer half (`_forage_compose_available` / `_render_unknown_contents_note`) asks it too.
func tile_contents_unseen(tile_info: Dictionary) -> bool:
	var state := String(tile_info.get("visibility_state", ""))
	return state == HudLayer.VISIBILITY_DISCOVERED or state == HudLayer.VISIBILITY_UNEXPLORED

## The selected hex's coordinates, as the one key an explicit subject choice is remembered against.
func _selected_tile_coords() -> Vector2i:
	return Vector2i(int(_selection.tile_info().get("x", -1)), int(_selection.tile_info().get("y", -1)))

## Auto-select the subject whose drawer opens on a fresh tile click.
##
## THE RULE IS DELIBERATELY UNCHANGED, PLUS A LAND FALLBACK: first roster unit → else first herd →
## else the land. A hex with no occupants used to hide the Occupants card and leave the Tile card
## showing terrain, which IS "the land is selected" — so the fallback preserves today's behaviour
## rather than introducing a new default. Selecting the land emits `roster_occupant_selected("land",
## …)`, which moves no ring (the hex outline already marks the tile) but CLEARS MapView's occupant
## selection — see `_on_land_row_selected`. The auto-pick emits ONLY `roster_occupant_selected` (never
## `subject_changed`): it runs mid-`render`, so the list/drawer are already about to be drawn.
func _resolve_auto_selected_subject() -> void:
	if not _selection.unit().is_empty() or not _selection.herd().is_empty():
		return
	# THE DEFAULT ONLY APPLIES WHERE THE PLAYER HAS NOT ALREADY CHOSEN. Both occupant dicts are
	# empty either because this is a fresh hex (auto-select) or because the player picked the LAND
	# row here (honour it) — and only the choice tile can tell the two apart. Without this, the
	# per-snapshot `reapply_selection("tile", …)` re-ran the default every turn and stole a
	# deliberately-chosen land selection back to the first band. A genuinely new hex has different
	# coords, so today's first-band → first-herd → land default is preserved exactly.
	if not _selection.tile_info().is_empty() and _selection.choice_tile() == _selected_tile_coords():
		_selection.set_subject(HudSelectionState.SUBJECT_LAND)
		return
	if not _selection.roster_units().is_empty():
		_selection.select_unit((_selection.roster_units()[0] as Dictionary).duplicate(true))
		emit_signal("roster_occupant_selected", "unit", int(_selection.unit().get("entity", -1)))
	elif not _selection.roster_herds().is_empty():
		_selection.select_herd((_selection.roster_herds()[0] as Dictionary).duplicate(true))
		emit_signal("roster_occupant_selected", "herd", String(_selection.herd().get("id", "")))
	else:
		_selection.set_subject(HudSelectionState.SUBJECT_LAND)

# ---- The subject list ------------------------------------------------------

## Rebuild the subject rows: the LAND first (no group header — it is not one of a group), then a
## `Bands (N)` sub-group and a `Wildlife (N)` sub-group, each a dim uppercase header + one
## selectable row per occupant. The row matching the current selection is styled as selected.
func _rebuild_subject_list() -> void:
	if _subject_list == null:
		return
	var descriptors := _subject_row_descriptors()
	var keys: Array = []
	for descriptor in descriptors:
		keys.append(descriptor["key"])
	# Membership (the ordered set of row identities + their structural flags) unchanged → patch every
	# row in place, so a per-snapshot restate updates names/sizes/dots/selection without freeing the
	# Buttons (whose `pressed` bindings we keep intact). A band/herd entering or leaving the hex — or a
	# row's own structure changing — moves a key, so the whole list rebuilds.
	if keys == _subject_row_keys and _subject_list.get_child_count() == descriptors.size():
		for i in range(descriptors.size()):
			_update_subject_row(_subject_list.get_child(i), descriptors[i])
		return
	for child in _subject_list.get_children():
		child.queue_free()
	for descriptor in descriptors:
		_subject_list.add_child(_build_subject_row(descriptor))
	_subject_row_keys = keys

## The ordered subject-row descriptors: the LAND first, then a `Bands (N)` sub-group and a
## `Wildlife (N)` sub-group, then the unseen-hint. Each carries a stable `key` — the row's identity
## PLUS the structural flags that decide its optional child nodes (a band's activity glyph, a herd's
## staffing meta) — so a key change is exactly the case that must rebuild rather than patch.
func _subject_row_descriptors() -> Array:
	var rows: Array = []
	if not _selection.tile_info().is_empty():
		rows.append({"key": ["land"], "kind": "land"})
	if not _selection.roster_units().is_empty():
		rows.append({"key": ["header", "bands"], "kind": "header",
			"title": "Bands", "count": _selection.roster_units().size()})
		for unit in _selection.roster_units():
			var u: Dictionary = unit
			rows.append({"key": ["band", int(u.get("entity", -1)), _is_player_unit(u)], "kind": "band", "data": u})
	if not _selection.roster_herds().is_empty():
		rows.append({"key": ["header", "wildlife"], "kind": "header",
			"title": "Wildlife", "count": _selection.roster_herds().size()})
		for herd in _selection.roster_herds():
			var h: Dictionary = herd
			rows.append({"key": ["herd", String(h.get("id", "")), _herd_row_meta(h) != ""], "kind": "herd", "data": h})
	# Reached only when your OWN unit is on a hex you can't see (everything else was redacted): say so,
	# or the lone row would read as "and nothing else is here" — which we cannot know.
	if tile_contents_unseen(_selection.tile_info()) and not (_selection.roster_units().is_empty() and _selection.roster_herds().is_empty()):
		rows.append({"key": ["hint"], "kind": "hint"})
	return rows

## Build the node for one subject-row descriptor.
func _build_subject_row(descriptor: Dictionary) -> Control:
	match String(descriptor["kind"]):
		"land":
			return _build_land_row(_selection.tile_info())
		"header":
			return _roster_group_header(String(descriptor["title"]), int(descriptor["count"]))
		"band":
			return _build_band_row(descriptor["data"])
		"herd":
			return _build_herd_row(descriptor["data"])
		_:
			return HudWidgets.alloc_hint_label(HudLayer.OCCUPANTS_UNSEEN_OTHERS_HINT)

## Patch one existing subject-row node in place. Headers + the hint are static once membership is
## fixed (their counts are implied by it), so only the three selectable rows carry an updater.
func _update_subject_row(node: Node, descriptor: Dictionary) -> void:
	match String(descriptor["kind"]):
		"land":
			_update_land_row(node as Button, _selection.tile_info())
		"band":
			_update_band_row(node as Button, descriptor["data"])
		"herd":
			_update_herd_row(node as Button, descriptor["data"])

## The LAND row — the same shape as a band/herd row, because the land is the same KIND of thing:
## a subject on this hex you can put workers on.
##
## Label = the BIOME name (more informative than a generic "The land", and it leaves the card title
## as the coordinates). Glyph = the tile's food-module icon where it carries one — the SAME icon the
## map marker draws, so a source reads identically in the panel and on the map — else the neutral
## `◈`. Dot = the patch's ecology tier, the same vitality vocabulary as the band/herd dots.
func _build_land_row(tile_info: Dictionary) -> Button:
	var selected := _selection.subject() == HudSelectionState.SUBJECT_LAND
	var patch_phase := String(tile_info.get("patch_ecology_phase", "")).strip_edges()
	var dot_color := _ecology_tier_color(patch_phase) if patch_phase != "" else HudStyle.INK_FAINT
	var module_key := String(tile_info.get("food_module", "")).strip_edges()
	var glyph := HudLayer.LAND_ROW_GLYPH
	if module_key != "":
		glyph = FoodIcons.for_site(module_key, false, int(tile_info.get("terrain_id", -1)))
	var button := _make_roster_button(selected)
	var row := _make_roster_row(selected, dot_color)
	var terrain_label := String(tile_info.get("terrain_label", "Unknown"))
	var name_label := _roster_name_label("%s %s" % [glyph, terrain_label], selected)
	row.add_child(name_label)
	var meta := _land_row_meta(tile_info)
	var meta_label: Label = null
	if meta != "":
		meta_label = _roster_meta_label(meta)
		row.add_child(meta_label)
	button.add_child(row)
	button.pressed.connect(_on_land_row_selected)
	_store_row_refs(button, row, name_label, meta_label, null)
	return button

## The land row's meta is never empty (`_land_row_meta` returns `No forage` at minimum), so its label
## always exists — the `_update_land_row` patch relies on that.
func _update_land_row(button: Button, tile_info: Dictionary) -> void:
	var selected := _selection.subject() == HudSelectionState.SUBJECT_LAND
	_apply_row_selection(button, selected)
	var patch_phase := String(tile_info.get("patch_ecology_phase", "")).strip_edges()
	_set_row_dot(button, _ecology_tier_color(patch_phase) if patch_phase != "" else HudStyle.INK_FAINT)
	var module_key := String(tile_info.get("food_module", "")).strip_edges()
	var glyph := HudLayer.LAND_ROW_GLYPH
	if module_key != "":
		glyph = FoodIcons.for_site(module_key, false, int(tile_info.get("terrain_id", -1)))
	_set_row_name(button, "%s %s" % [glyph, String(tile_info.get("terrain_label", "Unknown"))], selected)
	_set_row_meta(button, _land_row_meta(tile_info))

## The land row's meta, shortest true form: the foragers on this hex · else that the patch is
## unworked · else that there is nothing to gather here.
func _land_row_meta(tile_info: Dictionary) -> String:
	var workers := _forage_workers_on_tile(int(tile_info.get("x", -1)), int(tile_info.get("y", -1)))
	# Gated on the module KEY, never on its label: a tile with no module still ships the label
	# `"None"`, which would render as a source called "None" instead of the honest "No forage".
	if workers > 0 or String(tile_info.get("food_module", "")).strip_edges() != "":
		# An UNWORKED patch reads `0 🌾`, not its module label. The row already LEADS with that
		# module's own glyph, so the label restated it — and at dock width the row was the ONE
		# place the name truncated (`Savanna Gras…`) while the drawer's `Forage:` row and the
		# compose sheet's header both printed it whole. The zero form is parallel to the staffed
		# one, so "nobody is working this" reads at a glance instead of needing a comparison.
		return HudLayer.LAND_META_WORKERS_FORMAT % [workers, HudLayer.ACTIVITY_GLYPHS[HudLayer.LABOR_KIND_FORAGE]]
	return HudLayer.LAND_META_NO_FORAGE

## Foragers this faction has on (x, y), summed across every player band — the row states the hex's
## staffing, not one band's share of it.
func _forage_workers_on_tile(x: int, y: int) -> int:
	if x < 0 or y < 0:
		return 0
	var total := 0
	var bands: Array = _band_labor.player_bands() if not _band_labor.player_bands().is_empty() else [_band_labor.player_band()]
	for band_variant in bands:
		if band_variant is Dictionary and not (band_variant as Dictionary).is_empty():
			total += int(_band_labor.workers_for_forage(band_variant, x, y))
	return total

## The land row was clicked. It emits `roster_occupant_selected` with the THIRD kind, `"land"` (an
## additive kind on the existing `(kind, id)` contract — no id, so `LAND_SUBJECT_ID`), because
## MapView holds its OWN occupant selection: picking a band there clears the herd, and picking the
## land must clear both. There is still no map ring to move — the hex outline already marks the tile
## and `selected_tile` is untouched — but without this the next snapshot's
## `refresh_selection_payload` keeps answering `kind: "unit"` off the stale `selected_unit_id` and
## restores the band, which made the land unselectable on any occupied hex. `subject_changed` drives
## HudLayer's re-render (close sheet → re-render the panel + drawer).
func _on_land_row_selected() -> void:
	_selection.note_choice_tile(_selected_tile_coords())
	_selection.select_land()
	emit_signal("subject_changed")
	emit_signal("roster_occupant_selected", HudSelectionState.SUBJECT_LAND, HudLayer.LAND_SUBJECT_ID)

func _roster_group_header(title: String, count: int) -> Label:
	var label := Label.new()
	label.text = "%s (%d)" % [title.to_upper(), count]
	label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	label.add_theme_font_size_override("font_size", HudLayer.ROSTER_HEADER_FONT_SIZE)
	return label

## One selectable band row. A Button (row click) hosts a mouse-transparent HBox
## laying out: a selection accent, a vitality dot (BandFoodStatus color for a
## player band, neutral for others), the name, the size, and an activity glyph.
func _build_band_row(unit: Dictionary) -> Button:
	var entity_id := int(unit.get("entity", -1))
	var is_player := _is_player_unit(unit)
	var selected := not _selection.unit().is_empty() and int(_selection.unit().get("entity", -1)) == entity_id
	# Neutral tint for a non-player band's vitality dot (we can't see their larder).
	var dot_color := HudStyle.INK_FAINT
	var glyph := ""
	if is_player:
		dot_color = BandFoodStatus.color_for_turns(float(unit.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS)))
		glyph = _activity_glyph(String(unit.get("activity", "")))
	var button := _make_roster_button(selected)
	var row := _make_roster_row(selected, dot_color)
	var name_label := _roster_name_label(String(unit.get("id", "Band")), selected)
	row.add_child(name_label)
	var meta_label := _roster_meta_label(str(int(unit.get("size", 0))))
	row.add_child(meta_label)
	var glyph_label: Label = null
	if glyph != "":
		glyph_label = _roster_glyph_label(glyph, String(unit.get("activity", "")) == HudLayer.BAND_ACTIVITY_IDLE)
		row.add_child(glyph_label)
	# Surface the data-driven settlement-stage label (e.g. "Nomadic band") on hover; omit when
	# the band has no resolved stage (pre-stage / missing snapshot).
	var stage_label := String(unit.get("settlement_stage_label", "")).strip_edges()
	if stage_label != "":
		button.tooltip_text = stage_label
	button.add_child(row)
	button.pressed.connect(_on_roster_row_selected.bind("unit", entity_id))
	_store_row_refs(button, row, name_label, meta_label, glyph_label)
	return button

## Patch a band row in place. `is_player` (hence the glyph's presence) is stable per entity and rides
## the row key, so the glyph label is present here exactly when it was built.
func _update_band_row(button: Button, unit: Dictionary) -> void:
	var entity_id := int(unit.get("entity", -1))
	var is_player := _is_player_unit(unit)
	var selected := not _selection.unit().is_empty() and int(_selection.unit().get("entity", -1)) == entity_id
	_apply_row_selection(button, selected)
	var dot_color := HudStyle.INK_FAINT
	if is_player:
		dot_color = BandFoodStatus.color_for_turns(float(unit.get("turns_of_food", BandFoodStatus.UNLIMITED_TURNS)))
	_set_row_dot(button, dot_color)
	_set_row_name(button, String(unit.get("id", "Band")), selected)
	_set_row_meta(button, str(int(unit.get("size", 0))))
	if is_player and button.has_meta("glyph_label"):
		var activity := String(unit.get("activity", ""))
		var glyph_label := button.get_meta("glyph_label") as Label
		glyph_label.text = _activity_glyph(activity)
		glyph_label.add_theme_color_override("font_color",
			HudStyle.INK_FAINT if activity == HudLayer.BAND_ACTIVITY_IDLE else HudStyle.INK_DIM)
	button.tooltip_text = String(unit.get("settlement_stage_label", "")).strip_edges()

## One selectable wildlife row: an ecology-tier dot, the species glyph + name, and — as the meta —
## the hunters on it. Selecting it drives the drawer + the map ring to the herd.
func _build_herd_row(herd: Dictionary) -> Button:
	var herd_id := String(herd.get("id", ""))
	var selected := not _selection.herd().is_empty() and String(_selection.herd().get("id", "")) == herd_id
	var dot_color := _ecology_tier_color(String(herd.get("ecology_phase", "")))
	var button := _make_roster_button(selected)
	var row := _make_roster_row(selected, dot_color)
	var label := String(herd.get("label", herd.get("id", "Herd")))
	var glyph := FoodIcons.for_herd(label)
	var name_text := String(herd.get("species", label))
	var name_label := _roster_name_label("%s %s" % [glyph, name_text], selected)
	row.add_child(name_label)
	# The fauna id (`game_fowl_27`) is a DATABASE KEY, not player-facing text: it is the handle the
	# code addresses this herd with (the `pressed` bind below, and every `assign_labor`/`tame`/
	# `send_hunt_expedition` command), so it stays as DATA and never as a rendered label. The row
	# shows the species and, as its meta, how many hunters are on it; the size class reads in the
	# drawer.
	var meta := _herd_row_meta(herd)
	var meta_label: Label = null
	if meta != "":
		meta_label = _roster_meta_label(meta)
		row.add_child(meta_label)
	button.tooltip_text = label
	button.add_child(row)
	button.pressed.connect(_on_roster_row_selected.bind("herd", herd_id))
	_store_row_refs(button, row, name_label, meta_label, null)
	return button

## Patch a herd row in place. The meta's presence (huntable-or-staffed) is stable per herd and rides
## the row key, so the meta label is present here exactly when it was built.
func _update_herd_row(button: Button, herd: Dictionary) -> void:
	var herd_id := String(herd.get("id", ""))
	var selected := not _selection.herd().is_empty() and String(_selection.herd().get("id", "")) == herd_id
	_apply_row_selection(button, selected)
	_set_row_dot(button, _ecology_tier_color(String(herd.get("ecology_phase", ""))))
	var label := String(herd.get("label", herd.get("id", "Herd")))
	var glyph := FoodIcons.for_herd(label)
	_set_row_name(button, "%s %s" % [glyph, String(herd.get("species", label))], selected)
	_set_row_meta(button, _herd_row_meta(herd))
	button.tooltip_text = label

## The herd row's meta — the deliberate twin of `_land_row_meta`'s rule: a workable source states
## its staffing, anything else states nothing. A huntable herd with nobody on it reads `0 🏹`,
## parallel to the staffed form (and to the land row's `0 🌾`), so "nobody is working this" reads at
## a glance. A NON-huntable herd is not a source at all, so it earns no meta — exactly as a
## module-less tile earns no worker meta.
func _herd_row_meta(herd: Dictionary) -> String:
	var herd_id := String(herd.get("id", "")).strip_edges()
	var workers := _hunt_workers_on_herd(herd_id)
	if workers > 0 or bool(herd.get("huntable", false)):
		return HudLayer.HERD_META_WORKERS_FORMAT % [workers, HudLayer.ACTIVITY_GLYPHS[HudLayer.LABOR_KIND_HUNT]]
	return ""

## Hunters this faction has on `herd_id`, summed across BOTH ways a herd can be worked: standing
## local hunts assigned by any player band, and detached hunting expeditions committed to it (in
## whatever phase — a party en route to a herd is hunting it). The row states the herd's TOTAL
## staffing, not one band's or one mechanism's share of it — the same rule
## `_forage_workers_on_tile` documents for a hex.
func _hunt_workers_on_herd(herd_id: String) -> int:
	if herd_id == "":
		return 0
	var total := 0
	var bands: Array = _band_labor.player_bands() if not _band_labor.player_bands().is_empty() else [_band_labor.player_band()]
	for band_variant in bands:
		if band_variant is Dictionary and not (band_variant as Dictionary).is_empty():
			total += int(_band_labor.workers_for_hunt(band_variant, herd_id))
	for exp_variant in _band_labor.player_expeditions():
		if not (exp_variant is Dictionary):
			continue
		var exp: Dictionary = exp_variant
		if String(exp.get("expedition_mission", "")).strip_edges().to_lower() != HudLayer.EXPEDITION_MISSION_HUNT:
			continue
		if String(exp.get("expedition_target_herd", "")).strip_edges() != herd_id:
			continue
		total += int(exp.get("size", 0))
	return total

## A roster row's clickable Button shell: selected rows read as "primary", others
## as "ghost". Toggle_mode is off — selection is driven by a rebuild, not the
## button's own toggle state, so re-clicking the selected row can't un-highlight it.
func _make_roster_button(selected: bool) -> Button:
	var button := Button.new()
	button.focus_mode = Control.FOCUS_NONE
	button.custom_minimum_size = Vector2(0, HudLayer.ROSTER_ROW_MIN_HEIGHT)
	HudStyle.apply_button(button, "primary" if selected else "ghost")
	return button

## The mouse-transparent HBox overlaying a roster button, anchored to fill it,
## carrying the left selection accent + the vitality/ecology dot.
func _make_roster_row(selected: bool, dot_color: Color) -> HBoxContainer:
	var row := HBoxContainer.new()
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.set_anchors_preset(Control.PRESET_FULL_RECT)
	row.offset_left = HudLayer.ROSTER_ROW_H_PADDING
	row.offset_right = -HudLayer.ROSTER_ROW_H_PADDING
	row.add_theme_constant_override("separation", HudLayer.ROSTER_ROW_SEPARATION)
	var accent := ColorRect.new()
	accent.custom_minimum_size = Vector2(HudLayer.ROSTER_ACCENT_WIDTH, 0)
	accent.color = HudStyle.SIGNAL if selected else Color(0, 0, 0, 0)
	accent.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(accent)
	var dot := ColorRect.new()
	dot.custom_minimum_size = Vector2(HudLayer.ROSTER_DOT_SIZE, HudLayer.ROSTER_DOT_SIZE)
	dot.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	dot.color = dot_color
	dot.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(dot)
	# Stash the accent + dot so an in-place row update can reach them without indexing children.
	row.set_meta("accent", accent)
	row.set_meta("dot", dot)
	return row

## Stash a row's inner widgets on its Button, so `_update_*_row` patches them without positional
## child indexing (whose offsets vary with the optional meta/glyph labels). The accent + dot live on
## the row HBox; the caller passes the labels it built (a null one is simply not stored).
func _store_row_refs(button: Button, row: HBoxContainer, name_label: Label, meta_label, glyph_label) -> void:
	button.set_meta("accent", row.get_meta("accent"))
	button.set_meta("dot", row.get_meta("dot"))
	button.set_meta("name_label", name_label)
	if meta_label != null:
		button.set_meta("meta_label", meta_label)
	if glyph_label != null:
		button.set_meta("glyph_label", glyph_label)

## Re-apply a row's selection styling in place: the button's primary/ghost chrome + the left accent.
func _apply_row_selection(button: Button, selected: bool) -> void:
	HudStyle.apply_button(button, "primary" if selected else "ghost")
	if button.has_meta("accent"):
		(button.get_meta("accent") as ColorRect).color = HudStyle.SIGNAL if selected else Color(0, 0, 0, 0)

func _set_row_dot(button: Button, color: Color) -> void:
	if button.has_meta("dot"):
		(button.get_meta("dot") as ColorRect).color = color

func _set_row_name(button: Button, text: String, selected: bool) -> void:
	if not button.has_meta("name_label"):
		return
	var label := button.get_meta("name_label") as Label
	label.text = text
	label.add_theme_color_override("font_color", HudStyle.INK if selected else HudStyle.INK_DIM)

func _set_row_meta(button: Button, text: String) -> void:
	if button.has_meta("meta_label"):
		(button.get_meta("meta_label") as Label).text = text

## The row's IDENTITY — never elastic, never truncated. It takes its natural width and the meta
## beside it absorbs whatever is left (see `_roster_meta_label`).
func _roster_name_label(text: String, selected: bool) -> Label:
	var label := Label.new()
	label.text = text
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.add_theme_color_override("font_color", HudStyle.INK if selected else HudStyle.INK_DIM)
	return label

func _roster_meta_label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	# The META is the row's ELASTIC, EXPENDABLE half: it claims the slack after the name (hence the
	# right alignment the rows have always read with) and, when the row runs out of width in a 320px
	# dock, it is the meta that gives — ellipsised, not hard-cut, and never the name, which is the
	# row's identity. Free for the short band/herd metas ("120", "1 🏹"); it is the land row's
	# long module label that would otherwise push past the card's edge, and that label also reads in
	# full in the drawer.
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	label.clip_text = true
	label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.add_theme_color_override("font_color", HudStyle.INK_DIM)
	return label

func _roster_glyph_label(glyph: String, dim: bool) -> Label:
	var label := Label.new()
	label.text = glyph
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.add_theme_color_override("font_color", HudStyle.INK_FAINT if dim else HudStyle.INK_DIM)
	return label

func _activity_glyph(activity: String) -> String:
	return String(HudLayer.ACTIVITY_GLYPHS.get(activity.strip_edges().to_lower(), HudLayer.ACTIVITY_GLYPHS[HudLayer.BAND_ACTIVITY_IDLE]))

## Shared green/amber/red tier for a herd's ecology phase, matching the band
## food dot so map/roster/drawer agree: thriving→green, stressed→amber,
## collapsing→red. Matched on the phase stems from `EcologyPhase::as_str`.
func _ecology_tier_color(phase: String) -> Color:
	var normalized := phase.strip_edges().to_lower()
	if normalized.contains("collaps"):
		return HudStyle.DANGER
	if normalized.contains("stress"):
		return HudStyle.WARN
	return HudStyle.HEALTHY

## The roster occupant matching `entity_id`. Public because HudLayer's band/labor navigation
## (`_select_band_on_map`, `_on_panel_expedition_selected`, `_herd_label_for_id`) resolves through it.
func find_roster_unit(entity_id: int) -> Dictionary:
	for unit in _selection.roster_units():
		if unit is Dictionary and int((unit as Dictionary).get("entity", -1)) == entity_id:
			return unit
	return {}

func find_roster_herd(herd_id: String) -> Dictionary:
	if herd_id == "":
		return {}
	for herd in _selection.roster_herds():
		if herd is Dictionary and String((herd as Dictionary).get("id", "")) == herd_id:
			return herd
	return {}

## A roster row was clicked: make it the selected occupant (mutate + `subject_changed`), then relay
## `roster_occupant_selected` so the map ring follows.
func _on_roster_row_selected(kind: String, id: Variant) -> void:
	select_roster_occupant(kind, id)
	emit_signal("roster_occupant_selected", kind, id)

## Make `(kind, id)` the lit subject and ask HudLayer to re-render (via `subject_changed`, which closes
## the compose sheet + re-renders the panel + drawer). Public because HudLayer's band/labor navigation
## (`_select_band_on_map`, `_focus_labor_source`, `_on_panel_expedition_selected`) selects through it —
## those callers emit `roster_occupant_selected` themselves, so this does not.
func select_roster_occupant(kind: String, id: Variant) -> void:
	_selection.note_choice_tile(_selected_tile_coords())
	if kind == "unit":
		_selection.select_unit(find_roster_unit(int(id)).duplicate(true))
	else:
		_selection.select_herd(find_roster_herd(String(id)).duplicate(true))
	emit_signal("subject_changed")

## Player-faction check for a roster/drawer band (mirrors MapView._is_player_unit / HudLayer._is_player_unit —
## a trivial pure predicate kept local rather than threaded in as a Callable).
func _is_player_unit(unit: Dictionary) -> bool:
	return int(unit.get("faction", HudLayer.PLAYER_FACTION_ID)) == HudLayer.PLAYER_FACTION_ID
