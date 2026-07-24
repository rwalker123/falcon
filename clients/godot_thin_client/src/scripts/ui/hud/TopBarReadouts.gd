class_name TopBarReadouts
extends RefCounted

## Owns the TOP-BAR FACTION READOUTS cluster (docs/plan_hud_decomposition.md): the Sedentarization
## meter, the demographics line, the discovered Wondrous-Sites strip, the intensification-ladder
## knowledge strip, and the left-dock stockpile panel. Built on the LegendController/CommandFeedController
## idiom — HudLayer holds one as `_topbar` and delegates the snapshot `update_*` handlers to it.
##
## It holds PURE DATA + the top-bar label nodes, never `_selection`/`_band_labor`. ONE helper stays on
## HudLayer because it is shared beyond this cluster and is still a HudLayer METHOD, so it is passed in
## as a Callable: `_meter_bar` (the herd-drawer danger bars use it). Everything SHARED-BUT-PURE left
## HudLayer entirely and is called statically rather than injected or duplicated —
## `HudFormat.progress_percent`, `HudWidgets.set_label_tooltip`, and `HudFormat.stockpile_label` (whose
## other reader is the band drawer's accessible-stock rows). The gate helpers read this cluster's
## knowledge back through the public `faction_knowledge()`.

# --- Block-glyph meter widths (the top-bar strip's own display constants) ---
# The Sedentarization meter width. The knowledge strip runs NARROWER because it carries four tracks on
# one top-bar line — at the standard width the line overflowed and clipped its last track off-screen.
const METER_BAR_CELLS := 10
const KNOWLEDGE_METER_CELLS := 5

# --- Discovered Wondrous-Sites strip ---
# Top-bar glyph for the discovered-sites readout (a faceted-gem marker).
const DISCOVERIES_GLYPH := "◈"
# Drawn for a discovered site whose catalog row has NEITHER bundled art nor a non-empty `glyph`.
# A site the player has found must never render as blank space in the strip, so this is the
# last-resort mark: an outline of DISCOVERIES_GLYPH, reading as "a site, kind unillustrated".
const DISCOVERIES_UNKNOWN_GLYPH := "◇"
# Gap between the "◈ Discoveries N" text and the site strip, and between strip entries. Matches the
# double-space the glyphs used to be joined with, now that they are mixed Labels and TextureRects.
const DISCOVERIES_STRIP_SEPARATION := 6

# --- Intensification knowledge strip ---
# Separator between the four knowledge tracks in the top-bar intensification readout.
const INTENSIFICATION_SEGMENT_SEP := "  ·  "
# Leads the top-bar knowledge strip. This strip is the FACTION half of the two-meter split (§4.1) —
# what your PEOPLE have learned, permanently, from cumulative practice — as opposed to the per-source
# build meters in a herd's/patch's own drawer. The prefix exists precisely so the strip cannot be read
# as a stat of whatever happens to be selected; ⚒ (a hammer-and-pick — craft/skill) is bold line art,
# legible at top-bar size (the 🪙/💰 lesson), and used nowhere else in the HUD.
const KNOWLEDGE_STRIP_PREFIX := "⚒ Your people know:  "
# Tracks per row in the "⚒ Your people know" strip. Four tracks on ONE line overflowed the top bar and
# clipped the last one off-screen (the Penning playtest report); grouping into rows of two keeps every
# line short enough to fit common window widths — a top-bar strip is fine on two rows, and no track is
# ever lost off the edge, regardless of how many rungs the ladder grows.
const KNOWLEDGE_STRIP_TRACKS_PER_LINE := 2
# A fully-learned track. Bare, because `KNOWLEDGE_STRIP_PREFIX` already supplies the verb.
const KNOWLEDGE_KNOWN_BADGE := "✔"
# The player-facing name of each track, from the manual's vocabulary (§2a is authoritative). Also the
# order the top-bar knowledge strip renders them in: each web's own ladder, bottom rung first, so the
# strip reads as two ladders climbing rather than four unrelated numbers.
const KNOWLEDGE_TRACK_LABELS := {
	"cultivation": "Cultivation",
	"seed_selection": "Seed Selection",
	"herding": "Herding",
	"penning": "Penning",
}
# Command-feed nudge fired ONCE when a track completes: the rung it unlocks is a new verb the player
# has never seen, so learning the discovery has to say what it bought — and, since the verb is only
# HALF the story, what the verb then asks of them (a per-source meter to fill).
const KNOWLEDGE_UNLOCK_LABELS := {
	"cultivation": "Cultivation learned",
	"seed_selection": "Seed Selection learned",
	"herding": "Herding learned",
	"penning": "Penning learned",
}
# NOTE: `herding` used to read "The Corral policy is now available on domesticated herds." Both
# halves were wrong after the §4.3 reshuffle — Herding gates **Tame** (rung 2) and it is **Penning**
# that gates Corral (rung 3).
const KNOWLEDGE_UNLOCK_NOTES := {
	"cultivation": "The Cultivate policy is now available on Thriving wild patches.",
	"seed_selection": "The Sow policy is now available — but only on rich, well-watered ground.",
	"herding": "The Tame policy is now available on wild herds that can be domesticated.",
	"penning": "The Corral policy is now available on herds you have tamed.",
}

# --- Top-bar label nodes (handed in by HudLayer) ---
var turn_label: Label = null
var metrics_label: Label = null
var sedentarization_label: Label = null
var demographics_label: Label = null
var discoveries_row: HBoxContainer = null
var discoveries_label: Label = null
var discoveries_strip: HBoxContainer = null
var intensification_label: Label = null
var stockpile_panel: PanelContainer = null
var stockpile_list: VBoxContainer = null

# --- Collaborators ---
var _command_feed: CommandFeedController = null
# The one shared-beyond-cluster helper still kept on HudLayer, called back through a Callable (header).
var _meter_bar_fn: Callable

# --- Owned state (moved off HudLayer) ---
# Per-faction intensification knowledge from the latest snapshot: entity → {cultivation, herding, …},
# each 0..1. Backs the top-bar meters AND the policy-gate reasons (via faction_knowledge()); the
# previous value is what makes the one-shot unlock nudge possible.
var _intensification_knowledge: Dictionary = {}
# "<faction>:<track>" keys already announced to the command feed, so the nudge fires once.
var _knowledge_announced: Dictionary = {}
var _stockpile_totals: Dictionary = {}

func _init(
	turn_label_: Label,
	metrics_label_: Label,
	sedentarization_label_: Label,
	demographics_label_: Label,
	discoveries_row_: HBoxContainer,
	discoveries_label_: Label,
	discoveries_strip_: HBoxContainer,
	intensification_label_: Label,
	stockpile_panel_: PanelContainer,
	stockpile_list_: VBoxContainer,
	command_feed: CommandFeedController,
	meter_bar_fn: Callable,
) -> void:
	turn_label = turn_label_
	metrics_label = metrics_label_
	sedentarization_label = sedentarization_label_
	demographics_label = demographics_label_
	discoveries_row = discoveries_row_
	discoveries_label = discoveries_label_
	discoveries_strip = discoveries_strip_
	intensification_label = intensification_label_
	stockpile_panel = stockpile_panel_
	stockpile_list = stockpile_list_
	_command_feed = command_feed
	_meter_bar_fn = meter_bar_fn

## The LABEL-rendering half of HudLayer.update_overlay's fan-out: the turn readout + the metrics line.
## The turn-orb / band-labor side effects stay on HudLayer (they are not top-bar readouts).
func render_overlay(turn: int, metrics: Dictionary) -> void:
	turn_label.text = "Turn %d" % turn
	var unit_count: int = int(metrics.get("unit_count", 0))
	var avg_logistics: float = float(metrics.get("avg_logistics", 0.0))
	var avg_sentiment: float = float(metrics.get("avg_sentiment", 0.0))
	metrics_label.text = "Units: %d | Logistics: %.2f | Sentiment: %.2f" % [unit_count, avg_logistics, avg_sentiment]

## Show the player faction's Sedentarization pressure as a compact top-bar text meter.
## Hidden until the score is meaningful; tinted amber (soft) / cyan (hard) as it climbs.
func update_sedentarization(sedentarization_variant: Variant) -> void:
	if sedentarization_label == null:
		return
	var score := 0.0
	var stage := ""
	if sedentarization_variant is Array:
		for entry in sedentarization_variant:
			if entry is Dictionary and int(entry.get("faction", -1)) == HudLayer.PLAYER_FACTION_ID:
				score = float(entry.get("score", 0.0))
				stage = String(entry.get("stage", ""))
				break
	if score < 1.0:
		sedentarization_label.visible = false
		return
	sedentarization_label.visible = true
	var suffix := "" if stage == "" or stage == "none" else " · %s" % stage
	sedentarization_label.text = "Sedentarization  %s  %d/100%s" % [_meter_bar_fn.call(score, METER_BAR_CELLS), int(round(score)), suffix]
	sedentarization_label.add_theme_color_override("font_color", _sedentarization_color(stage))

## Show the player faction's age structure (children / working / elders) and the dependency
## ratio — the core demographic tension. Hidden until the faction has population.
func update_demographics(demographics_variant: Variant) -> void:
	if demographics_label == null:
		return
	var children := 0
	var working := 0
	var elders := 0
	var found := false
	if demographics_variant is Array:
		for entry in demographics_variant:
			if entry is Dictionary and int(entry.get("faction", -1)) == HudLayer.PLAYER_FACTION_ID:
				children = int(entry.get("children", 0))
				working = int(entry.get("working", 0))
				elders = int(entry.get("elders", 0))
				found = true
				break
	var total := children + working + elders
	if not found or total <= 0:
		demographics_label.visible = false
		return
	demographics_label.visible = true
	# NO DEPENDENCY RATIO HERE. This strip is the FACTION total across every band, and dependents are
	# fed per BAND — a band with more mouths than hands is in trouble whatever the faction average says,
	# and a healthy faction average can hide it. So the ratio belongs to the Band panel's PEOPLE block
	# (where it is stated as a dependent COUNT with the ratio in its tooltip) and is out of place as a
	# faction figure. The strip states the composition and nothing else.
	demographics_label.text = "Pop %d  👶%d 🛠%d 🧓%d" % [total, children, working, elders]
	demographics_label.add_theme_color_override("font_color", HudStyle.INK_DIM)

## Show the player faction's discovered Wondrous Sites as a compact top-bar readout: the count
## (`◈ Discoveries N`) followed by a strip of one mark per distinct site KIND, so landmark vs
## settle_site reads at a glance. Hidden until at least one site is known.
##
## THE TWO NUMBERS MEAN DIFFERENT THINGS AND ARE BOTH RIGHT: `N` is `sites.size()`, the count of
## INSTANCES found (site identity is the tile `(x, y)`); the strip shows KINDS, so three peaks are
## `N = 3` behind one peak mark. Do not "reconcile" them to a unique count.
##
## KEYED ON `site_id`, NOT on the glyph — the same rule `SecondaryMarkerRenderer` and `WonderSprites`
## follow. `site_id` is the sim's stable catalog key (`core_sim/src/data/sites_config.json`), while
## `glyph` is presentation the server happens to also send: two distinct sites may share one emoji
## (the fixture's `sky_arch` reuses ⛰), and deduping on the glyph collapsed them into a single strip
## entry that then silently disagreed with the count beside it.
func update_discoveries(discovered_variant: Variant) -> void:
	if discoveries_row == null or discoveries_label == null or discoveries_strip == null:
		return
	var sites: Array = []
	if discovered_variant is Array:
		for entry in discovered_variant:
			if entry is Dictionary and int(entry.get("faction", -1)) == HudLayer.PLAYER_FACTION_ID:
				var faction_sites: Variant = entry.get("sites", [])
				if faction_sites is Array:
					sites = faction_sites
				break
	if sites.is_empty():
		discoveries_row.visible = false
		return
	discoveries_row.visible = true
	discoveries_row.add_theme_constant_override("separation", DISCOVERIES_STRIP_SEPARATION)
	discoveries_label.text = "%s Discoveries %d" % [DISCOVERIES_GLYPH, sites.size()]
	discoveries_label.add_theme_color_override("font_color", HudStyle.SIGNAL)
	_rebuild_discoveries_strip(sites)

## One mark per distinct site KIND, first-seen order preserved. Called per snapshot, so the previous
## children are freed rather than accumulated.
func _rebuild_discoveries_strip(sites: Array) -> void:
	for child in discoveries_strip.get_children():
		child.queue_free()
		discoveries_strip.remove_child(child)
	discoveries_strip.add_theme_constant_override("separation", DISCOVERIES_STRIP_SEPARATION)
	# Sprites are sized to the readout's own font so the strip keeps the text baseline it had when
	# every entry was an emoji — derived, never a hardcoded pixel size.
	var mark_size := discoveries_label.get_theme_font_size("font_size")
	var seen: Array[String] = []
	for site in sites:
		if not (site is Dictionary):
			continue
		var entry := site as Dictionary
		var site_id := String(entry.get("site_id", "")).strip_edges()
		# A catalog-pruned site with no id still deserves one entry, so fall back to its tile —
		# instance identity — rather than dropping it or collapsing every such site together.
		var key := site_id
		if key == "":
			key = "%d,%d" % [int(entry.get("x", 0)), int(entry.get("y", 0))]
		if seen.has(key):
			continue
		seen.append(key)
		# Presentation precedence: bundled art, else the server's emoji, else the fallback mark.
		var name := String(entry.get("display_name", "")).strip_edges()
		var texture := WonderSprites.for_site_id(site_id)
		if texture != null:
			discoveries_strip.add_child(_discoveries_sprite(texture, mark_size, name))
		else:
			discoveries_strip.add_child(_discoveries_glyph_label(entry, name))

## A strip entry backed by bundled art, boxed to the text's font size and drawn aspect-preserved so
## a non-square illustration still sits on the same baseline as its emoji neighbours.
func _discoveries_sprite(texture: Texture2D, mark_size: int, site_name: String) -> TextureRect:
	var rect := TextureRect.new()
	rect.texture = texture
	rect.custom_minimum_size = Vector2(mark_size, mark_size)
	rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	rect.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	rect.tooltip_text = site_name
	return rect

## A strip entry drawn as text: the site's server-provided emoji, or the named fallback mark when the
## catalog row carries neither art nor a glyph.
func _discoveries_glyph_label(entry: Dictionary, site_name: String) -> Label:
	var label := Label.new()
	var glyph := String(entry.get("glyph", "")).strip_edges()
	label.text = glyph if glyph != "" else DISCOVERIES_UNKNOWN_GLYPH
	label.add_theme_color_override("font_color", HudStyle.SIGNAL)
	label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	HudWidgets.set_label_tooltip(label, site_name)
	return label

## THE FACTION HALF OF THE TWO-METER SPLIT (docs/plan_intensification_ladder.md §4.1) — the player
## faction's intensification-ladder knowledge as a compact top-bar block-glyph strip, mirroring the
## Sedentarization readout.
##
## This strip is deliberately the ONLY place a knowledge meter appears. The split it enforces:
##   • KNOWLEDGE — "what my PEOPLE can do at all". Faction-wide, permanent, earned by cumulative
##     practice. It lives HERE, in the top bar, prefixed and labelled as your people's craft.
##   • PER-SOURCE PROGRESS — "have I done it to THIS herd/patch yet". Local, decays if abandoned. It
##     lives in that source's own drawer row (Husbandry / Corral / Cultivation / Field).
## They are different KINDS of thing, so they never share a surface: no knowledge percentage is ever
## rendered into a herd's or a patch's stat grid, where it would read as one more number about that
## animal. The two are related in exactly one place — a gated verb's reason line under the policy
## picker (`_hunt_policy_gates` / `_forage_policy_gates`), which names the knowledge, its live
## percent, and the practice that fills it. That line is what teaches the ladder.
##
## All FOUR tracks render (slice 4 added Seed Selection + Penning), in `KNOWLEDGE_TRACK_LABELS` order
## — each web's ladder bottom-rung-first — so the strip reads as two ladders climbing rather than
## four unrelated numbers. A track is hidden until the faction begins learning it (the snapshot row is
## sparse, and an unstarted rung is noise); a completed track reads "✔ known" (SIGNAL) not a full bar.
func update_intensification(intensification_variant: Variant) -> void:
	_ingest_intensification(intensification_variant)
	if intensification_label == null:
		return
	var segments: Array[String] = []
	var all_known := true
	for track in KNOWLEDGE_TRACK_LABELS:
		var progress := faction_knowledge(HudLayer.PLAYER_FACTION_ID, String(track))
		if progress <= 0.0:
			continue
		segments.append("%s %s" % [
			String(KNOWLEDGE_TRACK_LABELS[track]), _knowledge_meter_text(progress)])
		all_known = all_known and progress >= HudLayer.KNOWLEDGE_COMPLETE
	if segments.is_empty():
		intensification_label.visible = false
		return
	intensification_label.visible = true
	# The prefix is what makes this strip read as YOUR PEOPLE'S SKILL rather than as a stat of
	# whatever is currently selected — the one line of copy carrying the §4.1 distinction in the HUD.
	# Break the tracks into rows of KNOWLEDGE_STRIP_TRACKS_PER_LINE so a fourth (or future fifth) track
	# can NEVER run off the right edge: the strip lives in a content-sized top-bar block, so a single
	# long line just widens the block until it clips (the Penning playtest report) — autowrap can't
	# engage without a bounded width. Explicit rows bound each line regardless of window width. The
	# prefix rides the first row.
	var rows: Array[String] = []
	var line_segs: Array[String] = []
	for seg in segments:
		line_segs.append(seg)
		if line_segs.size() >= KNOWLEDGE_STRIP_TRACKS_PER_LINE:
			rows.append(INTENSIFICATION_SEGMENT_SEP.join(line_segs))
			line_segs = []
	if not line_segs.is_empty():
		rows.append(INTENSIFICATION_SEGMENT_SEP.join(line_segs))
	intensification_label.text = "%s%s" % [KNOWLEDGE_STRIP_PREFIX, "\n".join(rows)]
	# Cyan once every learned track is fully known; neutral while any is still in progress.
	intensification_label.add_theme_color_override(
		"font_color", HudStyle.SIGNAL if all_known else HudStyle.INK_DIM)

## Capture the per-faction intensification tracks off the snapshot AND announce the moment one
## COMPLETES — the transition (`< 1.0` last snapshot, `>= 1.0` now) is exactly when a new policy
## becomes usable, and nothing else in the HUD would tell the player. One-shot per faction+track
## (`_knowledge_announced`), so it never re-fires on subsequent snapshots; a track already complete
## on the first snapshot we see (fresh connect / rehydrated save) has no prior value and is NOT
## announced — a nudge about something learned long ago is noise.
func _ingest_intensification(intensification_variant: Variant) -> void:
	if not (intensification_variant is Array):
		return
	for entry in intensification_variant:
		if not (entry is Dictionary):
			continue
		var row := entry as Dictionary
		var faction := int(row.get("faction", -1))
		if faction < 0:
			continue
		var previous: Dictionary = _intensification_knowledge.get(faction, {})
		# Every track the ladder defines, off the one list — so adding a rung's knowledge is a
		# KNOWLEDGE_TRACK_LABELS entry plus a decoder field, never an edit here.
		var current := {}
		for track in KNOWLEDGE_TRACK_LABELS:
			current[track] = float(row.get(track, 0.0))
		for track in KNOWLEDGE_UNLOCK_NOTES:
			if not previous.has(track):
				continue
			if float(previous[track]) >= HudLayer.KNOWLEDGE_COMPLETE:
				continue
			if float(current[track]) < HudLayer.KNOWLEDGE_COMPLETE:
				continue
			_announce_knowledge_unlock(faction, String(track))
		_intensification_knowledge[faction] = current

## Post the one-shot "policy unlocked" nudge to the command feed. Player faction only — another
## faction's tech is not the player's to see, and every other intensification readout filters the
## same way; the announced set is still keyed per faction so the dedupe is correct for all of them.
func _announce_knowledge_unlock(faction: int, track: String) -> void:
	var key := "%d:%s" % [faction, track]
	if _knowledge_announced.has(key):
		return
	_knowledge_announced[key] = true
	if faction != HudLayer.PLAYER_FACTION_ID:
		return
	_command_feed.note(String(KNOWLEDGE_UNLOCK_LABELS[track]), String(KNOWLEDGE_UNLOCK_NOTES[track]))

## A faction's progress (0..1) on one intensification track; 0 when the faction has not begun it
## (the snapshot row is sparse) or no snapshot has arrived yet. PUBLIC because the HudLayer policy-gate
## reasons (`_forage_policy_gates` / `_hunt_policy_gates`) read this cluster's knowledge back.
func faction_knowledge(faction: int, track: String) -> float:
	var tracks: Dictionary = _intensification_knowledge.get(faction, {})
	return float(tracks.get(track, 0.0))

## One knowledge track's readout: a compact block-glyph bar + the live percent while in progress, a
## "✔ known" badge once complete. `progress` is 0..1.
##
## Deliberately TIGHTER than the Sedentarization meter beside it (which keeps the shared 10-cell
## `_meter_bar`): slice 4 took this strip from two tracks to FOUR, and at 10 cells + the word
## "learning" per track the line overflowed the top bar and clipped its last track off-screen —
## verified in the first cut of `two_meter_split.png`. The percent replaces the word rather than
## joining it: it is strictly more informative, and it is the SAME number the gate reason under a
## locked verb quotes, so the strip and the reason line visibly agree.
func _knowledge_meter_text(progress: float) -> String:
	# A bare ✔ rather than "✔ known": the strip's own prefix already reads "Your people know:", so the
	# word was saying it twice — and the two it cost per known track were enough to push the fourth
	# track's percent onto the frame edge.
	if progress >= HudLayer.KNOWLEDGE_COMPLETE:
		return KNOWLEDGE_KNOWN_BADGE
	return "%s %d%%" % [
		_meter_bar_fn.call(progress * HudLayer.PROGRESS_PERCENT_SCALE, KNOWLEDGE_METER_CELLS),
		HudFormat.progress_percent(progress)]

## Tint the Sedentarization readout: cyan when the pressure is hard, amber when soft, neutral otherwise.
func _sedentarization_color(stage: String) -> Color:
	match stage:
		"hard":
			return HudStyle.SIGNAL
		"soft":
			return HudStyle.WARN
		_:
			return HudStyle.INK_DIM

func update_stockpiles(faction_inventory_variant: Variant) -> void:
	if stockpile_panel == null:
		return
	var faction_array: Array = faction_inventory_variant if faction_inventory_variant is Array else []
	var next_totals: Dictionary = {}
	for faction_entry in faction_array:
		if not (faction_entry is Dictionary):
			continue
		if int(faction_entry.get("faction", -1)) != HudLayer.PLAYER_FACTION_ID:
			continue
		var inventory_variant: Variant = faction_entry.get("inventory", [])
		if inventory_variant is Array:
			var inventory_entries: Array = inventory_variant
			for stock_entry in inventory_entries:
				if not (stock_entry is Dictionary):
					continue
				var item_name := String(stock_entry.get("item", "")).strip_edges()
				if item_name == "":
					continue
				next_totals[item_name] = int(stock_entry.get("quantity", 0))
		break
	var combined_keys: Array = []
	for key in _stockpile_totals.keys():
		if not combined_keys.has(key):
			combined_keys.append(key)
	for key in next_totals.keys():
		if not combined_keys.has(key):
			combined_keys.append(key)
	combined_keys.sort()
	var panel_entries: Array = []
	for key in combined_keys:
		var amount := int(next_totals.get(key, 0))
		var previous := int(_stockpile_totals.get(key, 0))
		if amount == 0 and previous == 0:
			continue
		var delta := float(amount - previous)
		panel_entries.append({
			"label": HudFormat.stockpile_label(key),
			"amount": amount,
			"delta": delta,
		})
	_stockpile_totals = next_totals
	if stockpile_list == null or stockpile_panel == null:
		return
	for child in stockpile_list.get_children():
		child.queue_free()
	if panel_entries.is_empty():
		stockpile_panel.visible = false
		return
	stockpile_panel.visible = true
	for entry in panel_entries:
		stockpile_list.add_child(_build_stockpile_row(entry))

func _build_stockpile_row(entry: Dictionary) -> Control:
	var row := HBoxContainer.new()
	row.custom_minimum_size = Vector2(0, 24)
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var label := Label.new()
	label.text = String(entry.get("label", "Stockpile"))
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(label)
	var amount_label := Label.new()
	amount_label.text = str(entry.get("amount", 0))
	amount_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	amount_label.custom_minimum_size = Vector2(60, 0)
	row.add_child(amount_label)
	var delta := float(entry.get("delta", 0.0))
	if not is_equal_approx(delta, 0.0):
		var delta_label := Label.new()
		delta_label.text = ("+%.0f" % delta) if delta > 0.0 else ("%.0f" % delta)
		delta_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		delta_label.custom_minimum_size = Vector2(60, 0)
		delta_label.modulate = Color(0.6, 0.9, 0.6) if delta > 0.0 else Color(0.95, 0.6, 0.5)
		row.add_child(delta_label)
	return row

