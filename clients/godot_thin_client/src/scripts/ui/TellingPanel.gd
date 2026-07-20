class_name TellingPanel
extends RefCounted

## The Telling (docs/plan_the_telling_book_ux.md) — the narrative panel as a BOOK that assembles itself
## as the narrator's medium matures (oral recitation → painted wall → written record).
##
## WHY THIS EXISTS. Narrative beats used to render in the command feed, and two of them filled it
## entirely and pushed ordinary command receipts off screen. A command receipt is a TRANSACTIONAL
## ACKNOWLEDGEMENT (worthless after seconds); a narrative beat is THE STORY SO FAR. Opposite retention,
## density and reading behaviour, so the narrative kinds moved here and the command feed went back to
## being a command log. `CommandFeedController` SKIPS whatever `handles_kind()` claims, so the split has
## exactly one definition (below) and the two surfaces can never both render — or drop — a kind.
##
## THE BOOK MODEL. The controller keeps its full retention buffer (`_entries`, cap 40 — the
## backfill/dedup source of truth) and DERIVES the display from it every render. A **page = one speaking
## turn's beats** (the entries sharing a `tick`); `_pages` is rebuilt on each render. The card is a
## **fixed height** (`PAGE_HEIGHT`) — no `DockScrollFit`, no grow-and-cap; the inner `ScrollContainer`
## is kept only as an overflow fallback for a rare over-long single page. Capabilities grow with the
## medium (the per-medium `MODE_TABLE`, `oral` fallback):
##   • `oral`    — current utterance only; no furniture, no back, the page is pinned to the newest.
##   • `painted` — the accumulating wall; walk FORWARD one page at a time, no back, a marks/position cue.
##   • `written` — the full book; a page number + ‹ › leaf controls, leaf freely both ways.
## The visible page NEVER moves on its own: new beats set the UNREAD mark, they do not turn the page
## (the page-turn twin of the feed's tail-scroll-yields-to-reader rule). The player turns via the leaf
## controls; `reveal_newest()` catches them up on turn-advance.

const HudStyle := preload("res://src/scripts/ui/HudStyle.gd")

# ---- the kinds this panel OWNS ---------------------------------------------
const KIND_NARRATIVE_BEAT := "narrative_beat"
const KIND_NARRATIVE_FORK := "narrative_fork"

## The single definition of the feed/telling split. `CommandFeedController` asks this rather than
## keeping its own list, so a kind can never land in both surfaces or in neither.
static func handles_kind(kind: String) -> bool:
	return kind == KIND_NARRATIVE_BEAT or kind == KIND_NARRATIVE_FORK

# ---- retention -------------------------------------------------------------
# Far deeper than the command feed's 6. These are short strings, the panel IS this arc's product, and
# `painted`/`written` reveal history the sim was holding all along ("the voice never lies"). The
# server's own `commandEvents` ring is only 32, so backfill on connect is partial by nature.
const ENTRY_LIMIT := 40

# ---- geometry / typography (named constants; no magic literals) ------------
# The FIXED page height: sized to hold ~3 short prose beats + their gloss. The card never grows — that
# is the whole point — so the dock's own scroll trivially stacks Telling + Victory + Terrain Types with
# no bespoke height math. A rare over-long single page overflows into the inner ScrollContainer.
const PAGE_HEIGHT := 150.0
# Prose, so it is set a touch larger than the command feed's UI copy and given real leading.
const NARRATION_FONT_SIZE := 14
const GLOSS_FONT_SIZE := 12
const ENTRY_SEPARATION := "\n\n"
const ACCENT_RULE_HEIGHT := 1.0
# The accent rule is a hairline, not a band: it should register as texture, not as a second border.
const ACCENT_RULE_ALPHA := 0.55
const COLLAPSE_FONT_SIZE := 12
const COLLAPSE_LABEL_EXPANDED := "▾  hide"
const COLLAPSE_LABEL_COLLAPSED := "▸  show %d"
const COLLAPSE_TOOLTIP := "Fold the telling away — the story is kept either way."
const EMPTY_TEXT := "[i]Nothing has been told yet.[/i]"
# A fork is a QUESTION put to the people, so it keeps the mark the orb's decision row wears; an ordinary
# beat needs no glyph. Line art, never emoji — an emoji glyph blobs at card size (the MagnifierButton
# hazard).
const FORK_GLYPH := "?"
const FORK_LINE_FORMAT := "[color=#%s]%s[/color]  %s"

# ---- book furniture (RESTRAINT: the dark HUD stays dark) -------------------
# The medium's capabilities are carried by line-art edges, a page-number/position label, the accent and
# the leaf glyphs — NOT by a parchment background (which would read as a rendering bug). Leaf glyphs are
# text-presentation angle quotes (‹ ›), not emoji, and are judged at true size in ui_preview.
const FURNITURE_NONE := 0        # oral: no page chrome at all
const FURNITURE_MARKS := 1       # painted: accumulation marks + position, no back
const FURNITURE_BOOK := 2        # written: page number + ‹ › leaf controls
## `mediumId` is FREE-FORM by design (a new medium needs no schema change), so this is a TABLE WITH AN
## `oral` FALLBACK, never a match assuming the shipped three are exhaustive — the same discipline as
## `MEDIUM_STYLES`.
const MODE_TABLE := {
	"oral": {"furniture": FURNITURE_NONE, "leaf_back": false, "retain_pages": false},
	"painted": {"furniture": FURNITURE_MARKS, "leaf_back": false, "retain_pages": true},
	"written": {"furniture": FURNITURE_BOOK, "leaf_back": true, "retain_pages": true},
}
const FURNITURE_FONT_SIZE := 12
const LEAF_FONT_SIZE := 16
const LEAF_PREV_GLYPH := "‹"
const LEAF_NEXT_GLYPH := "›"
const LEAF_PREV_TOOLTIP := "Leaf back a page"
const LEAF_NEXT_TOOLTIP := "Leaf forward a page"
const PAGE_NUMBER_FORMAT := "Page %d / %d"
const PAGE_POSITION_FORMAT := "%s   %d / %d"
const PAGE_MARK_FILLED := "▮"
const PAGE_MARK_EMPTY := "▯"
# Cap the mark run so a long chronicle does not overflow the row (the position number still reads exact).
const PAGE_MARKS_MAX := 12
const UNREAD_CUE_TEXT := "a new telling waits"

# ---- the maturing voice ----------------------------------------------------
# The medium is PRESENTATIONAL ONLY — it changes the title, the accent and the book's CAPABILITIES,
# never which copy is rendered; per-medium copy is a deliberate non-goal, documented server-side.
const MEDIUM_ORAL := "oral"
const MEDIUM_STYLES := {
	MEDIUM_ORAL: {"title": "AT THE FIRE", "accent": HudStyle.WARN},
	"painted": {"title": "ON THE WALL", "accent": HudStyle.VOICE_PIGMENT},
	"written": {"title": "THE RECORD", "accent": HudStyle.VOICE_INK},
}

# ---- collapsed-state preference --------------------------------------------
# Reuses the file + section `NarrativeForkPanel` already writes the voice register into — one narrative
# prefs file, not two. The key is ours; the path/section are deliberately borrowed.
const CONFIG_KEY_COLLAPSED := "telling_collapsed"

var _panel: PanelCard = null
var _scroll: ScrollContainer = null
var _label: RichTextLabel = null
var _collapse_button: Button = null
var _accent_rule: ColorRect = null
var _furniture_row: HBoxContainer = null
var _leaf_prev: Button = null
var _leaf_next: Button = null
var _page_label: Label = null
var _unread_label: Label = null

# The full retention buffer: one dict `{tick, bbcode}` per beat, in ingest order. Pages derive from it.
var _entries: Array = []
var _signatures: Dictionary = {}
# The derived book: one dict `{tick, beats}` per speaking turn, ascending. Rebuilt every render.
var _pages: Array = []
var _page_index: int = 0
var _medium_id: String = MEDIUM_ORAL
var _collapsed: bool = false

func _init(panel: PanelCard, scroll: ScrollContainer, label: RichTextLabel) -> void:
	_panel = panel
	_scroll = scroll
	_label = label
	_collapsed = load_collapsed()
	_build_chrome()
	_apply_medium()

# ---- public API ------------------------------------------------------------

## Merge a batch of server command-event dicts (`{tick, kind, label, detail}`), keeping only the
## narrative kinds and de-duplicating by signature, then re-render.
##
## This is ALSO the backfill path: a full snapshot carries the server's whole `commandEvents` ring, so
## a player opening the client mid-session sees recent history. The panel is deliberately NOT reset on a
## full snapshot — the signature de-dup makes re-ingesting the ring harmless, and resetting would throw
## away everything that has already scrolled past the 32-entry ring.
func ingest_events(events_variant: Variant) -> void:
	if _label == null or not (events_variant is Array):
		return
	var appended := false
	for entry_variant in (events_variant as Array):
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var kind := String(entry.get("kind", "")).strip_edges()
		if not handles_kind(kind):
			continue
		var tick: int = int(entry.get("tick", -1))
		var label := String(entry.get("label", "")).strip_edges()
		var detail := String(entry.get("detail", "")).strip_edges()
		var signature := "%d|%s|%s|%s" % [tick, kind, label, detail]
		if _signatures.has(signature):
			continue
		_signatures[signature] = true
		_append_entry(tick, kind, label, detail)
		appended = true
	if appended:
		render()

func reset() -> void:
	_entries.clear()
	_signatures.clear()
	_pages.clear()
	_page_index = 0
	render()

## Set the player faction's narrator medium. Unknown/absent ids fall back to `oral` styling, so a medium
## the client has never heard of degrades to the first rung instead of rendering unstyled.
func set_voice_medium(medium_id: String) -> void:
	var resolved := medium_id.strip_edges()
	if not MEDIUM_STYLES.has(resolved):
		resolved = MEDIUM_ORAL
	if resolved == _medium_id:
		return
	_medium_id = resolved
	_apply_medium()

## The accent for a medium id — the ONE lookup, so the fork panel's header and this panel's title can
## never drift to different colours for the same medium.
static func accent_for(medium_id: String) -> Color:
	var style: Dictionary = MEDIUM_STYLES.get(medium_id, MEDIUM_STYLES[MEDIUM_ORAL])
	return style["accent"]

## Turn the page by `delta`, clamped into range. The player's own leaf controls call this (‹ = -1,
## › = +1); it never runs on its own (new beats only mark UNREAD).
func leaf(delta: int) -> void:
	if _pages.is_empty():
		return
	_page_index = clampi(_page_index + delta, 0, _pages.size() - 1)
	_render_page()

## Catch the reader up to the latest telling — the turn-advance path (`Hud._on_turn_orb_advance`). A
## player who moves on is shown the newest page; mid-turn beats only mark unread, advancing reveals.
func reveal_newest() -> void:
	if _pages.is_empty():
		return
	_page_index = _pages.size() - 1
	_render_page()

# ---- collapsed-state preference --------------------------------------------

## Fails silently: a missing or malformed prefs file must never surface to the player.
static func load_collapsed() -> bool:
	var cfg := ConfigFile.new()
	if cfg.load(NarrativeForkPanel.config_path()) != OK:
		return false
	return bool(cfg.get_value(NarrativeForkPanel.CONFIG_SECTION, CONFIG_KEY_COLLAPSED, false))

static func save_collapsed(collapsed: bool) -> void:
	var cfg := ConfigFile.new()
	cfg.load(NarrativeForkPanel.config_path())   # preserve the voice register; ignore load errors
	cfg.set_value(NarrativeForkPanel.CONFIG_SECTION, CONFIG_KEY_COLLAPSED, collapsed)
	cfg.save(NarrativeForkPanel.config_path())

# ---- chrome ----------------------------------------------------------------

## Build the runtime rows the scene does not author: the medium accent rule, the book furniture row
## (leaf controls + page-number/position + unread cue), and the collapse toggle. Order inside
## CardContent: header(0) · accent(1) · furniture(2) · scroll(3) · collapse(4).
func _build_chrome() -> void:
	if _panel == null:
		return
	var content := _panel.get_content()
	if content == null:
		return

	_accent_rule = ColorRect.new()
	_accent_rule.name = "TellingAccentRule"
	_accent_rule.custom_minimum_size = Vector2(0.0, ACCENT_RULE_HEIGHT)
	_accent_rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	content.add_child(_accent_rule)
	content.move_child(_accent_rule, 1)

	_furniture_row = HBoxContainer.new()
	_furniture_row.name = "TellingFurniture"
	_furniture_row.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	_leaf_prev = Button.new()
	_leaf_prev.name = "TellingLeafPrev"
	_leaf_prev.text = LEAF_PREV_GLYPH
	_leaf_prev.tooltip_text = LEAF_PREV_TOOLTIP
	_leaf_prev.add_theme_font_size_override("font_size", LEAF_FONT_SIZE)
	_leaf_prev.pressed.connect(_on_leaf_prev_pressed)
	_furniture_row.add_child(_leaf_prev)

	_page_label = Label.new()
	_page_label.name = "TellingPageLabel"
	_page_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_page_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_page_label.add_theme_font_size_override("font_size", FURNITURE_FONT_SIZE)
	_furniture_row.add_child(_page_label)

	_unread_label = Label.new()
	_unread_label.name = "TellingUnreadCue"
	_unread_label.text = UNREAD_CUE_TEXT
	_unread_label.add_theme_font_size_override("font_size", FURNITURE_FONT_SIZE)
	_furniture_row.add_child(_unread_label)

	_leaf_next = Button.new()
	_leaf_next.name = "TellingLeafNext"
	_leaf_next.text = LEAF_NEXT_GLYPH
	_leaf_next.tooltip_text = LEAF_NEXT_TOOLTIP
	_leaf_next.add_theme_font_size_override("font_size", LEAF_FONT_SIZE)
	_leaf_next.pressed.connect(_on_leaf_next_pressed)
	_furniture_row.add_child(_leaf_next)

	content.add_child(_furniture_row)
	content.move_child(_furniture_row, 2)

	_collapse_button = Button.new()
	_collapse_button.name = "TellingCollapse"
	_collapse_button.tooltip_text = COLLAPSE_TOOLTIP
	_collapse_button.focus_mode = Control.FOCUS_NONE
	_collapse_button.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	_collapse_button.add_theme_font_size_override("font_size", COLLAPSE_FONT_SIZE)
	HudStyle.apply_link_button(_collapse_button, HudStyle.INK_FAINT)
	_collapse_button.pressed.connect(_on_collapse_pressed)
	content.add_child(_collapse_button)

## Push the medium's title, accent and book capabilities. Deliberately only the title, the accent, the
## hairline rule and the furniture set: the panel keeps the dark card chrome every other HUD surface
## wears, so the voice reads as the SAME voice grown older rather than three different applications.
func _apply_medium() -> void:
	var style: Dictionary = MEDIUM_STYLES.get(_medium_id, MEDIUM_STYLES[MEDIUM_ORAL])
	var accent: Color = style["accent"]
	if _panel != null:
		_panel.set_card_title(String(style["title"]))
		_panel.set_title_color(accent)
	if _accent_rule != null:
		_accent_rule.color = Color(accent.r, accent.g, accent.b, ACCENT_RULE_ALPHA)
	# The leaf glyphs + labels wear the medium accent — the book's furniture ages with the voice.
	if _leaf_prev != null:
		HudStyle.apply_link_button(_leaf_prev, accent)
	if _leaf_next != null:
		HudStyle.apply_link_button(_leaf_next, accent)
	if _page_label != null:
		_page_label.add_theme_color_override("font_color", HudStyle.INK_DIM)
	if _unread_label != null:
		_unread_label.add_theme_color_override("font_color", accent)
	# Full render, not just a repaint: the medium change flips `retain_pages`, so the visible page must
	# be reconciled (oral re-pins to the newest, a retaining medium re-clamps) — not left where the
	# previous medium's rules put it.
	render()

# ---- entries + pages -------------------------------------------------------

## One entry: the narration as PROSE, with the gloss as a dim secondary line. No `Turn N` prefix and no
## bold-label/italic-detail split — those are command-RECEIPT affordances and they fight the prose.
func _append_entry(tick: int, kind: String, label: String, detail: String) -> void:
	var line := label if label != "" else detail
	var message := "[font_size=%d]%s[/font_size]" % [NARRATION_FONT_SIZE, line]
	if kind == KIND_NARRATIVE_FORK:
		message = "[font_size=%d]%s[/font_size]" % [
			NARRATION_FONT_SIZE,
			FORK_LINE_FORMAT % [HudStyle.SIGNAL_HEX, FORK_GLYPH, line],
		]
	if detail != "" and detail != line:
		message += "\n[font_size=%d][i][color=#%s]%s[/color][/i][/font_size]" % [
			GLOSS_FONT_SIZE, HudStyle.INK_DIM_HEX, detail,
		]
	_entries.append({"tick": tick, "bbcode": message})
	while _entries.size() > ENTRY_LIMIT:
		_entries.pop_front()

## Group the retention buffer by `tick`, ascending, into `_pages`. A turn with no beats produces no
## page ("page = a turn that had something to say"), and multiple beats on one tick share one page.
func _rebuild_pages() -> void:
	_pages.clear()
	var by_tick: Dictionary = {}
	var order: Array = []
	for entry in _entries:
		var tick: int = int(entry["tick"])
		if not by_tick.has(tick):
			by_tick[tick] = []
			order.append(tick)
		(by_tick[tick] as Array).append(entry["bbcode"])
	order.sort()
	for tick in order:
		_pages.append({"tick": tick, "beats": by_tick[tick]})

# ---- rendering -------------------------------------------------------------

## Rebuild the pages from the retention buffer, reconcile the visible page (never moving it on its own
## for a retaining medium; pinning to the newest for `oral`), and paint it.
func render() -> void:
	if _panel == null or _label == null:
		return
	_panel.visible = true
	_rebuild_pages()
	_reconcile_page_index()
	_render_page()

## Re-clamp the visible page against the (possibly grown) page list. `retain_pages = false` (oral) pins
## it to the newest — oral memory does not keep the previous telling. For a retaining medium the index
## is only CLAMPED, never advanced: new beats grow the range but leave the reader where they were (the
## yields-to-reader rule). "Unread" is then simply "you are not on the newest page".
func _reconcile_page_index() -> void:
	if _pages.is_empty():
		_page_index = 0
		return
	var last := _pages.size() - 1
	if not bool(_mode()["retain_pages"]):
		_page_index = last
	else:
		_page_index = clampi(_page_index, 0, last)

## The fixed-size page: one turn's beats, the book furniture for the current medium, and the collapse
## toggle. The card height never depends on content — only the inner scroll absorbs a rare over-long
## page.
func _render_page() -> void:
	if _panel == null or _label == null or _scroll == null:
		return
	var mode: Dictionary = _mode()
	if _pages.is_empty():
		_label.text = EMPTY_TEXT
	else:
		_label.text = ENTRY_SEPARATION.join(_pages[_page_index]["beats"])
	_scroll.custom_minimum_size.y = PAGE_HEIGHT
	_scroll.scroll_vertical = 0
	_scroll.visible = not _collapsed
	_update_furniture(mode)
	_refresh_collapse()

## Show/hide and populate the furniture row for the current medium. Hidden entirely at `oral` (no page
## chrome) and while collapsed (the header + collapse control stay so the player can expand it back).
func _update_furniture(mode: Dictionary) -> void:
	if _furniture_row == null:
		return
	var furniture := int(mode["furniture"])
	var showing := (not _collapsed) and furniture != FURNITURE_NONE and not _pages.is_empty()
	_furniture_row.visible = showing
	if not showing:
		return
	var last := _pages.size() - 1
	var unread := _has_unread()

	_leaf_prev.visible = bool(mode["leaf_back"])
	_leaf_prev.disabled = _page_index <= 0
	# A retaining medium always offers a forward affordance (walk the wall / leaf the book); it goes
	# dead on the newest page.
	_leaf_next.visible = true
	_leaf_next.disabled = _page_index >= last

	if furniture == FURNITURE_BOOK:
		_page_label.text = PAGE_NUMBER_FORMAT % [_page_index + 1, _pages.size()]
	else:
		_page_label.text = PAGE_POSITION_FORMAT % [_marks_string(), _page_index + 1, _pages.size()]

	_unread_label.visible = unread

## A row of marks that ACCUMULATES as the wall fills — the first sense that the surface remembers. One
## mark per page (capped), filled up to the visible page.
func _marks_string() -> String:
	var shown := mini(_pages.size(), PAGE_MARKS_MAX)
	var marks := ""
	for i in range(shown):
		marks += PAGE_MARK_FILLED if i <= _page_index else PAGE_MARK_EMPTY
	return marks

## True when a page newer than the visible one exists and the reader has not turned to it. `oral` pins
## to the newest, so it is never unread.
func _has_unread() -> bool:
	if _pages.is_empty() or not bool(_mode()["retain_pages"]):
		return false
	return _page_index < _pages.size() - 1

func _mode() -> Dictionary:
	return MODE_TABLE.get(_medium_id, MODE_TABLE[MEDIUM_ORAL])

## No-op now that the card is a fixed size — a sibling's visibility flip no longer changes the telling's
## height. Kept so `Hud._refit_right_dock`'s call stays valid; it only re-clamps the inner scroll.
func refit() -> void:
	if _scroll != null:
		_scroll.scroll_vertical = 0

func _refresh_collapse() -> void:
	if _collapse_button == null:
		return
	_collapse_button.text = (COLLAPSE_LABEL_COLLAPSED % _entries.size()) if _collapsed else COLLAPSE_LABEL_EXPANDED

func _on_leaf_prev_pressed() -> void:
	leaf(-1)

func _on_leaf_next_pressed() -> void:
	leaf(1)

func _on_collapse_pressed() -> void:
	_collapsed = not _collapsed
	save_collapsed(_collapsed)
	_render_page()
