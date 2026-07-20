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
## height that **grows to fit the current page, capped at `PAGE_MAX_HEIGHT`** — no `DockScrollFit`, no
## unbounded grow-and-cap; a PAGE is bounded (one turn's beats), so fit-to-content is safe, and the inner
## `ScrollContainer` only engages BEYOND the cap (the extreme: many long beats on one tick). Capabilities
## grow with the
## medium (the per-medium `MODE_TABLE`, `oral` fallback):
##   • `oral`    — current utterance only; no furniture, no back, the page is pinned to the newest.
##   • `painted` — the accumulating wall; walk FORWARD one page at a time, no back, a marks/position cue.
##   • `written` — the full book; a page number + ‹ › leaf controls, leaf freely both ways.
## The visible page NEVER moves on its own: new beats set the UNREAD mark, they do not turn the page
## (the page-turn twin of the feed's tail-scroll-yields-to-reader rule). The player turns via the leaf
## controls; `reveal_newest()` catches them up on turn-advance.
##
## THE PAGE-TURN ANIMATION. Motion MATURES with the medium, mirroring the furniture, and plays ONLY when
## the player turns the page (leaf / reveal_newest catch-up / oral's utterance-replacement) — never on a
## beat merely arriving to a retaining medium (that only marks unread, so animating it would fight the
## yields rule). Each medium's motion is a short, snappy tween of a single 0→1 progress
## (`PAGE_TURN_DURATION`), applied by `_apply_turn_frame` to an outgoing snapshot (`_outgoing`) and the
## incoming page (`_scroll`), both clipped to the fixed `_page_frame`:
##   • `oral`    — a CROSSFADE in place (one recitation dissolving into the next; oral keeps no prior page).
##   • `painted` — the incoming page RISES from just below with a fade (new marks drifting onto the wall).
##   • `written` — a horizontal SLIDE in the leaf direction (the real page turn; the earned book).
## Interruption-safe by construction: every turn `_kill_tween()`s the running one and re-paints the final
## page statically first, so a rapid second turn / medium change / collapse always settles to the correct
## static state — never a half-slid page.

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
# The page GROWS TO FIT its current content, capped at PAGE_MAX_HEIGHT. A page is BOUNDED (one turn's
# beats, itself bounded by the beat budget) — unlike the old scroll log that accumulated all 40 entries —
# so fit-to-content is safe and does NOT reopen the dock-sizing problem the cap is the backstop for. Only
# a genuinely extreme page (many long beats on one tick) exceeds the cap; the inner ScrollContainer
# scrolls only THEN. PAGE_MAX_HEIGHT is additionally clamped to PAGE_MAX_HEIGHT_VIEWPORT_CEIL of the
# viewport so it can never dominate the dock on a short window. PAGE_MIN_HEIGHT keeps a one-line page from
# collapsing; PAGE_FIT_PADDING is the hairline below the last line so descenders are not clipped.
const PAGE_MAX_HEIGHT := 320.0
const PAGE_MAX_HEIGHT_VIEWPORT_CEIL := 0.5
const PAGE_MIN_HEIGHT := 48.0
const PAGE_FIT_PADDING := 4.0
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

# ---- page-turn animation ---------------------------------------------------
# Snappy: a slide/rise is a state change, not a transition to sit through — 0.18s for painted/written.
const PAGE_TURN_DURATION := 0.18
# ORAL is SLOWER and eased-in-out on purpose. A pure alpha crossfade over 0.18s (front-loaded EASE_OUT)
# on dark text is a flicker, not a motion — it read as a "hard switch" in playtest. Oral wants a
# perceptible dissolve, so it gets its own longer duration AND a positional DRIFT (below) so the eye can
# track one recitation settling in as the last drifts off.
const PAGE_TURN_DURATION_ORAL := 0.42
# The painted page rises from this many px below its resting spot (a modest offset — an accumulation cue,
# not a scroll). A fixed px, so it reads the same whatever the fitted page height.
const PAGE_RISE_OFFSET := 30.0
# Oral's drift: the incoming utterance settles DOWN into place from this many px above rest as it fades in,
# the outgoing drifts the same distance further down as it fades out. Small — a settle, not a scroll — and
# DOWNWARD, deliberately opposite painted's rise-from-below, so the two mediums never read alike.
const PAGE_ORAL_DRIFT := 16.0
const MOTION_CROSSFADE := 0   # oral: dissolve-in-place + gentle downward settle
const MOTION_RISE := 1        # painted: lift from below with a fade
const MOTION_SLIDE := 2       # written: horizontal slide in the leaf direction

# ---- book furniture (RESTRAINT: the dark HUD stays dark) -------------------
# The medium's capabilities are carried by line-art edges, a page-number/position label, the accent and
# the leaf glyphs — NOT by a parchment background (which would read as a rendering bug). Leaf glyphs are
# text-presentation angle quotes (‹ ›), not emoji, and are judged at true size in ui_preview.
const FURNITURE_NONE := 0        # oral: no page chrome at all
const FURNITURE_MARKS := 1       # painted: accumulation marks + position, no back
const FURNITURE_BOOK := 2        # written: page number + ‹ › leaf controls
## `mediumId` is FREE-FORM by design (a new medium needs no schema change), so this is a TABLE WITH AN
## `oral` FALLBACK, never a match assuming the shipped three are exhaustive — the same discipline as
## `MEDIUM_STYLES`. Each rung's `motion` matures with its furniture.
const MODE_TABLE := {
	"oral": {"furniture": FURNITURE_NONE, "leaf_back": false, "retain_pages": false, "motion": MOTION_CROSSFADE},
	"painted": {"furniture": FURNITURE_MARKS, "leaf_back": false, "retain_pages": true, "motion": MOTION_RISE},
	"written": {"furniture": FURNITURE_BOOK, "leaf_back": true, "retain_pages": true, "motion": MOTION_SLIDE},
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
# The clipped, fixed-height page frame the incoming page (`_scroll`) and the outgoing snapshot
# (`_outgoing`) animate within. A plain Control, so it never re-lays out its children — the animation
# owns their position/modulate.
var _page_frame: Control = null
var _outgoing: RichTextLabel = null
var _tween: Tween = null

# The full retention buffer: one dict `{tick, bbcode}` per beat, in ingest order. Pages derive from it.
var _entries: Array = []
var _signatures: Dictionary = {}
# The derived book: one dict `{tick, beats}` per speaking turn, ascending. Rebuilt every render.
var _pages: Array = []
var _page_index: int = 0
var _medium_id: String = MEDIUM_ORAL
var _collapsed: bool = false
# The BBCode currently painted in `_label`, so a turn knows what to snapshot as the outgoing page and can
# tell a real page change (animate) from an idempotent re-render (don't).
var _shown_bbcode: String = ""
# False until the first paint, so the initial population never animates.
var _has_painted: bool = false
# The active turn's parameters, read by `_apply_turn_frame` while the tween drives `progress` 0→1.
var _turn_motion: int = MOTION_CROSSFADE
var _turn_dir: int = 1
var _turn_extent: Vector2 = Vector2.ZERO
# The fitted height the frame settles to once the turn ends (the INCOMING page's height, capped). During
# the turn the frame holds max(outgoing, incoming) so neither page clips mid-motion.
var _turn_settle_height: float = PAGE_MIN_HEIGHT

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

## Turn the page by `delta`, clamped into range, with the medium's page-turn animation. The player's own
## leaf controls call this (‹ = -1, › = +1); it never runs on its own (new beats only mark UNREAD).
func leaf(delta: int) -> void:
	if _pages.is_empty():
		return
	var target := clampi(_page_index + delta, 0, _pages.size() - 1)
	if target == _page_index:
		return
	var direction := signi(target - _page_index)
	_page_index = target
	_paint_page(true, direction)

## Catch the reader up to the latest telling — the turn-advance path (`Hud._on_turn_orb_advance`). A
## player who moves on is shown the newest page; mid-turn beats only mark unread, advancing reveals.
func reveal_newest() -> void:
	if _pages.is_empty():
		return
	var last := _pages.size() - 1
	if _page_index == last:
		_render_static()
		return
	# Catch-up reads as a forward turn, whatever medium.
	_page_index = last
	_paint_page(true, 1)

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

## Build the runtime rows the scene does not author: the medium accent rule, the clipped page frame (with
## the authored scroll reparented in + the outgoing snapshot overlay), the book furniture row (leaf
## controls + page-number/position + unread cue), and the collapse toggle. Order inside CardContent:
## header(0) · accent(1) · furniture(2) · page_frame(3) · collapse(4).
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

	# The clipped, fixed-height page frame. The authored ScrollContainer moves inside it so a slide/rise
	# can translate the page without disturbing the dock or the neighbours; the frame — a plain Control —
	# owns the fixed height and never re-lays out its children.
	_page_frame = Control.new()
	_page_frame.name = "TellingPageFrame"
	_page_frame.clip_contents = true
	# Start at the floor; every paint fits it to the current page's content (capped).
	_page_frame.custom_minimum_size = Vector2(0.0, PAGE_MIN_HEIGHT)
	_page_frame.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	content.add_child(_page_frame)
	content.move_child(_page_frame, 3)
	if _scroll != null:
		_scroll.reparent(_page_frame, false)
		_scroll.position = Vector2.ZERO
	_page_frame.resized.connect(_sync_page_geometry)

	# The outgoing-page snapshot, layered over the frame; hidden except during a turn. Same prose styling
	# as the live label so the two halves of a turn match.
	_outgoing = RichTextLabel.new()
	_outgoing.name = "TellingOutgoing"
	_outgoing.bbcode_enabled = true
	_outgoing.fit_content = true
	_outgoing.scroll_active = false
	_outgoing.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_outgoing.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_outgoing.visible = false
	_page_frame.add_child(_outgoing)

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
	# A medium change is NOT a page turn — re-paint statically (the flip of `retain_pages` still needs the
	# visible page reconciled: oral re-pins, a retaining medium re-clamps).
	_render_static()

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

## Beat-arrival / backfill / reset render. Rebuild the pages, reconcile the visible page (never moving it
## on its own for a retaining medium; pinning to the newest for `oral`), and paint. Only `oral` animates
## here — its beat arrival IS the utterance turn (it keeps no prior page); a retaining medium's page does
## not move on arrival, so it re-paints statically (the yields rule).
func render() -> void:
	if _panel == null or _label == null:
		return
	_panel.visible = true
	_rebuild_pages()
	_reconcile_page_index()
	_paint_page(_is_oral(), 0)

## Rebuild + reconcile + paint with NO animation — a medium change / collapse / catch-up-already-newest.
func _render_static() -> void:
	if _panel == null or _label == null:
		return
	_panel.visible = true
	_rebuild_pages()
	_reconcile_page_index()
	_paint_page(false, 0)

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

## Paint the current page, optionally animating the turn to it. A turn animates only when it is a REAL
## page change on a card that has already painted once and is not collapsed; otherwise (initial paint,
## empty↔content, an idempotent re-render, collapsed) it settles statically. Either way the primary label
## ends showing the new page and the furniture/collapse update.
func _paint_page(animate: bool, direction: int) -> void:
	if _panel == null or _label == null or _scroll == null:
		return
	var mode: Dictionary = _mode()
	var new_text := EMPTY_TEXT if _pages.is_empty() else ENTRY_SEPARATION.join(_pages[_page_index]["beats"])
	# Did the VISIBLE page actually move? (Captured before `_shown_bbcode` is updated below.) A real turn
	# changes it; a retaining medium's beat-arrival that only clamped the index — the same page repainted —
	# does not. Gates the inner-scroll reset so a mid-page reader of a beyond-cap page is not yanked to the
	# top by an idempotent repaint (yields-to-reader).
	var page_changed := new_text != _shown_bbcode
	var can_animate := (
		animate
		and _has_painted
		and not _collapsed
		and _page_frame != null
		and new_text != _shown_bbcode
		# Never animate the initial population (empty → first telling) nor a reset (telling → empty).
		and _shown_bbcode != EMPTY_TEXT
		and new_text != EMPTY_TEXT
	)
	if can_animate:
		_begin_turn(_shown_bbcode, new_text, int(mode["motion"]), direction)
	else:
		_kill_tween()
		_end_turn_visuals()
		_label.text = new_text
		# Grow the frame to fit the new page (capped). Sync now, and again deferred: on the very first
		# paint the label has no width yet, so `get_content_height()` is only reliable post-layout.
		_fit_page_height()
		call_deferred("_fit_page_height")
	_shown_bbcode = new_text
	_has_painted = true
	# Start a genuinely-new page at its top; hold the reader's position on an idempotent repaint.
	if _scroll != null and page_changed:
		_scroll.scroll_vertical = 0
	_sync_page_geometry()
	if _page_frame != null:
		_page_frame.visible = not _collapsed
	_update_furniture(mode)
	_refresh_collapse()

## Start a page-turn tween: snapshot the outgoing page, seat the start frame, and drive `progress` 0→1
## into `_apply_turn_frame`. Interruption-safe — kills any running tween first, and the caller has
## already set `_label` to the FINAL page, so a kill settles correctly.
func _begin_turn(old_text: String, new_text: String, motion: int, direction: int) -> void:
	_kill_tween()
	_turn_motion = motion
	_turn_dir = 1 if direction >= 0 else -1
	_outgoing.text = old_text
	_outgoing.visible = true
	_label.text = new_text
	# Pages have different heights: the frame must hold BOTH for the tween (so neither slide/rise/crossfade
	# clips mid-motion), then settle to the incoming page's fitted height. Widths are stable across a turn,
	# so `get_content_height()` measures reliably here.
	_sync_page_geometry()
	var cap := _page_max_height()
	var h_in := _measure_page_height(_label)
	var h_out := _measure_page_height(_outgoing)
	var base := maxf(h_in, h_out)
	if base <= 0.0:
		base = _page_frame.size.y   # not measurable yet — keep what we have
	_page_frame.custom_minimum_size.y = clampf(base + PAGE_FIT_PADDING, PAGE_MIN_HEIGHT, cap)
	_turn_settle_height = clampf(h_in + PAGE_FIT_PADDING, PAGE_MIN_HEIGHT, cap)
	_turn_extent = _page_frame.size
	_apply_turn_frame(0.0)
	# Oral gets a longer, EASE_IN_OUT dissolve (a flicker otherwise); the spatial mediums stay snappy EASE_OUT.
	var is_crossfade := motion == MOTION_CROSSFADE
	var duration := PAGE_TURN_DURATION_ORAL if is_crossfade else PAGE_TURN_DURATION
	var ease := Tween.EASE_IN_OUT if is_crossfade else Tween.EASE_OUT
	_tween = _page_frame.create_tween()
	_tween.set_trans(Tween.TRANS_SINE).set_ease(ease)
	_tween.tween_method(_set_turn_progress, 0.0, 1.0, duration)
	_tween.tween_callback(_end_turn)

func _set_turn_progress(progress: float) -> void:
	_apply_turn_frame(progress)

## Position + fade the outgoing snapshot (`_outgoing`) and the incoming page (`_scroll`) for a turn at
## `progress` ∈ [0,1], per the active medium's motion. The incoming page is the ScrollContainer (its
## label is laid out by it), so it — not `_label` — is what moves.
func _apply_turn_frame(progress: float) -> void:
	if _scroll == null or _outgoing == null:
		return
	var width := _turn_extent.x
	var rise := PAGE_RISE_OFFSET
	match _turn_motion:
		MOTION_RISE:
			_set_page_visual(_outgoing, Vector2.ZERO, 1.0 - progress)
			_set_page_visual(_scroll, Vector2(0.0, rise * (1.0 - progress)), progress)
		MOTION_SLIDE:
			var sign_dir := float(_turn_dir)   # +1 forward (exit left, enter right); -1 the reverse
			_set_page_visual(_outgoing, Vector2(-sign_dir * width * progress, 0.0), 1.0)
			_set_page_visual(_scroll, Vector2(sign_dir * width * (1.0 - progress), 0.0), 1.0)
		_:  # MOTION_CROSSFADE (oral): dissolve + a gentle DOWNWARD settle so the swap reads as motion, not a flicker
			var drift := PAGE_ORAL_DRIFT
			_set_page_visual(_outgoing, Vector2(0.0, drift * progress), 1.0 - progress)
			_set_page_visual(_scroll, Vector2(0.0, -drift * (1.0 - progress)), progress)

func _set_page_visual(node: Control, pos: Vector2, alpha: float) -> void:
	node.position = pos
	node.modulate = Color(1.0, 1.0, 1.0, alpha)

## Tween-finished callback: drop the outgoing snapshot, settle the frame to the incoming page's fitted
## height, and restore the resting transforms.
func _end_turn() -> void:
	_end_turn_visuals()
	_tween = null
	if _page_frame != null:
		_page_frame.custom_minimum_size.y = _turn_settle_height
	_sync_page_geometry()

## Settle the page visuals to their static resting state (incoming full + centred, outgoing hidden). The
## end state after any interruption equals this, so a killed tween never leaves a half-slid page.
func _end_turn_visuals() -> void:
	if _outgoing != null:
		_outgoing.visible = false
		_set_page_visual(_outgoing, Vector2.ZERO, 1.0)
	if _scroll != null:
		_set_page_visual(_scroll, Vector2.ZERO, 1.0)

func _kill_tween() -> void:
	if _tween != null and _tween.is_valid():
		_tween.kill()
	_tween = null

## Size the incoming page + the outgoing snapshot to the frame; reset their positions only when NO turn
## is on screen. The outgoing snapshot being visible means a turn owns the positions — a live tween OR a
## debug freeze — so a `resized` (the frame's height changed for the turn) must re-SIZE the pages without
## stomping their in-flight positions.
func _sync_page_geometry() -> void:
	if _page_frame == null:
		return
	var frame_size := _page_frame.size
	if _scroll != null:
		_scroll.size = frame_size
	if _outgoing != null:
		_outgoing.size = frame_size
	var turn_owns_positions := _outgoing != null and _outgoing.visible
	if not turn_owns_positions:
		if _scroll != null:
			_scroll.position = Vector2.ZERO
		if _outgoing != null:
			_outgoing.position = Vector2.ZERO

## Grow the frame to fit the CURRENT page's content, capped. No-op while a turn owns the height (the turn
## drives the frame height itself and settles it in `_end_turn`). A 0 measurement means the label has not
## laid out yet — leave the height and let the deferred re-fit catch it.
func _fit_page_height() -> void:
	if _page_frame == null:
		return
	if _outgoing != null and _outgoing.visible:
		return
	var height := _measure_page_height(_label)
	if height <= 0.0:
		return
	var target := clampf(height + PAGE_FIT_PADDING, PAGE_MIN_HEIGHT, _page_max_height())
	if absf(_page_frame.custom_minimum_size.y - target) > 0.5:
		_page_frame.custom_minimum_size.y = target
	_sync_page_geometry()

## The content height a page label needs at its current width. `get_content_height()` forces the line
## cache to validate, so it is reliable once the label has a width (stable across a turn / after layout).
func _measure_page_height(rtl: RichTextLabel) -> float:
	if rtl == null:
		return 0.0
	return float(rtl.get_content_height())

## The height ceiling: the named px cap, additionally clamped so it can never take more than half the
## viewport on a short window.
func _page_max_height() -> float:
	var cap := PAGE_MAX_HEIGHT
	if _page_frame != null and _page_frame.is_inside_tree():
		var viewport_height := _page_frame.get_viewport_rect().size.y
		if viewport_height > 0.0:
			cap = minf(cap, viewport_height * PAGE_MAX_HEIGHT_VIEWPORT_CEIL)
	return cap

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

func _is_oral() -> bool:
	return not bool(_mode()["retain_pages"])

## A sibling's visibility flip no longer changes the telling's height (the page fits its own content), so
## this only re-syncs the page geometry and re-fits the current page. Kept so `Hud._refit_right_dock`'s
## call stays valid.
func refit() -> void:
	_sync_page_geometry()
	_fit_page_height()

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
	_render_static()

# ---- ui_preview hooks (dev harness only) -----------------------------------
# The PNG harness renders single frames, so it needs to (a) park the book on a page without playing the
# turn animation, and (b) FREEZE a turn mid-transition to prove the motion is real. These exist for that;
# nothing in the live client calls them.

## Jump straight to a page with no animation (test setup). Bypasses `_reconcile_page_index`, so it can
## hold an older page even under `oral` (whose live reconcile pins to the newest).
func debug_jump_to(index: int) -> void:
	_rebuild_pages()
	if _pages.is_empty():
		return
	_page_index = clampi(index, 0, _pages.size() - 1)
	_paint_page(false, 0)

## Kill the running turn tween and freeze the page at `fraction` of the transition, so the outgoing and
## incoming pages coexist in the captured frame. No-op if no turn is in flight.
func debug_freeze_turn_at(fraction: float) -> void:
	_kill_tween()
	if _outgoing == null or not _outgoing.visible:
		return
	_apply_turn_frame(clampf(fraction, 0.0, 1.0))

## Force the active turn to settle (as a completed tween would) — used to assert the interrupted end state.
func debug_end_turn() -> void:
	_kill_tween()
	_end_turn()

func debug_visible_index() -> int:
	return _page_index

func debug_overlay_visible() -> bool:
	return _outgoing != null and _outgoing.visible

## True while a real page-turn tween is live (created, not paused/finished). Lets the harness assert the
## LIVE ingest→render→_begin_turn path actually created a running tween — proof distinct from the
## `debug_freeze_turn_at` render test, which only shows the tween CAN paint both pages.
func debug_turn_active() -> bool:
	return _tween != null and _tween.is_valid() and _tween.is_running()

## True when the current page overflows its fitted frame and the inner ScrollContainer is scrolling — the
## beyond-cap extreme. A grown-to-fit page (e.g. a two-beat turn under the cap) must report false.
func debug_page_scrolls() -> bool:
	if _scroll == null:
		return false
	var bar := _scroll.get_v_scroll_bar()
	return bar != null and bar.visible
