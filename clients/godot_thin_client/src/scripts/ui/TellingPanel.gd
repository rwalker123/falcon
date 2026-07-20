class_name TellingPanel
extends RefCounted

## The Telling (docs/plan_the_telling.md) — the narrative panel: the story so far.
##
## WHY THIS EXISTS. Narrative beats used to render in the command feed, and two of them filled it
## entirely and pushed ordinary command receipts off screen. The binding limit was never
## `COMMAND_FEED_LIMIT` — it is CARD HEIGHT: a wrapped prose line plus its gloss is roughly 3× a
## command receipt, and this arc only adds more of them (callback, identity and medium-advance
## beats). The root problem is that two different things shared one widget:
##
##   • a command receipt is a TRANSACTIONAL ACKNOWLEDGEMENT — "your order was accepted", relevant
##     for seconds, worthless after; and
##   • a narrative beat is THE STORY SO FAR — worth scrolling back through, and the whole output
##     of this arc.
##
## They want opposite retention, opposite density and opposite reading behaviour, so the narrative
## kinds moved here and the command feed went back to being a command log. `CommandFeedController`
## SKIPS whatever `handles_kind()` claims, so the split has exactly one definition (below) and the
## two surfaces can never both render — or both drop — a kind.
##
## Structurally this is a controller like `CommandFeedController` (Hud owns one and delegates), over
## a `PanelCard` authored in `HudLayer.tscn`. Height/scroll math is the shared `DockScrollFit` —
## see its header for why `AutoSizingPanel` is the wrong helper for a card inside the dock.

const HudStyle := preload("res://src/scripts/ui/HudStyle.gd")
const DockScrollFit := preload("res://src/scripts/ui/hud/DockScrollFit.gd")

# ---- the kinds this panel OWNS ---------------------------------------------
const KIND_NARRATIVE_BEAT := "narrative_beat"
const KIND_NARRATIVE_FORK := "narrative_fork"

## The single definition of the feed/telling split. `CommandFeedController` asks this rather than
## keeping its own list, so a kind can never land in both surfaces or in neither.
static func handles_kind(kind: String) -> bool:
	return kind == KIND_NARRATIVE_BEAT or kind == KIND_NARRATIVE_FORK

# ---- retention -------------------------------------------------------------
# Far deeper than the command feed's 6. These are short strings, the panel IS this arc's product,
# and the whole point of a separate surface is that the story so far is worth scrolling back
# through. The server's own `commandEvents` ring is only 32, so backfill on connect is partial by
# nature — this cap is about what we keep once we are running, not about that.
const ENTRY_LIMIT := 40

# ---- geometry / typography (named constants; no magic literals) ------------
const PANEL_MIN_HEIGHT := 96.0
const PANEL_BOTTOM_MARGIN := 12.0
# Prose, so it is set a touch larger than the command feed's UI copy and given real leading —
# the same reasoning as the fork panel's narration, scaled down for a dock card.
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
# A fork is a QUESTION put to the people, so it keeps the mark the orb's decision row wears; an
# ordinary beat is just the voice speaking and needs no glyph in its own panel. Line art, never
# emoji — an emoji-presentation glyph blobs at card size (the MagnifierButton hazard).
const FORK_GLYPH := "?"
const FORK_LINE_FORMAT := "[color=#%s]%s[/color]  %s"

# ---- the maturing voice ----------------------------------------------------
# `mediumId` is a FREE-FORM string by design (schema note: adding a medium needs no schema change),
# so this is a TABLE WITH A FALLBACK and never a match assuming the shipped three are exhaustive.
# The medium is PRESENTATIONAL ONLY — it changes the title and the accent, never which copy is
# rendered; per-medium copy is a deliberate non-goal, documented server-side.
const MEDIUM_ORAL := "oral"
const MEDIUM_STYLES := {
	MEDIUM_ORAL: {"title": "AT THE FIRE", "accent": HudStyle.WARN},
	"painted": {"title": "ON THE WALL", "accent": HudStyle.VOICE_PIGMENT},
	"written": {"title": "THE RECORD", "accent": HudStyle.VOICE_INK},
}

# ---- collapsed-state preference --------------------------------------------
# Reuses the file + section `NarrativeForkPanel` already writes the voice register into — one
# narrative prefs file, not two. The key is ours; the path/section are deliberately borrowed.
const CONFIG_KEY_COLLAPSED := "telling_collapsed"

# How close to the bottom still counts as "reading the tail". Anything short of this means the
# player has scrolled UP to read, and being yanked back down mid-sentence is worse than not
# auto-scrolling at all — so an append leaves their position alone.
const SCROLL_TAIL_EPSILON := 24.0

var _panel: PanelCard = null
var _scroll: ScrollContainer = null
var _label: RichTextLabel = null
var _dock_scroll: ScrollContainer = null
var _collapse_button: Button = null
var _accent_rule: ColorRect = null

var _entries: Array = []
var _signatures: Dictionary = {}
var _medium_id: String = MEDIUM_ORAL
var _collapsed: bool = false
# Captured BEFORE a re-render, because the render itself changes the scroll geometry.
var _was_at_tail: bool = true

func _init(panel: PanelCard, scroll: ScrollContainer, label: RichTextLabel, dock_scroll: ScrollContainer) -> void:
	_panel = panel
	_scroll = scroll
	_label = label
	_dock_scroll = dock_scroll
	_collapsed = load_collapsed()
	_build_chrome()
	_apply_medium()

# ---- public API ------------------------------------------------------------

## Merge a batch of server command-event dicts (`{tick, kind, label, detail}`), keeping only the
## narrative kinds and de-duplicating by signature, then re-render.
##
## This is ALSO the backfill path: a full snapshot carries the server's whole `commandEvents` ring,
## so a player opening the client mid-session sees recent history instead of an empty panel. The
## panel is deliberately NOT reset on a full snapshot (unlike the command feed) — the signature
## de-dup makes re-ingesting the ring harmless, and resetting would throw away everything that has
## already scrolled past the 32-entry ring.
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
		_append_entry(kind, label, detail)
		appended = true
	if appended:
		render()

func reset() -> void:
	_entries.clear()
	_signatures.clear()
	render()

## Set the player faction's narrator medium. Unknown/absent ids fall back to `oral` styling, so a
## medium the client has never heard of degrades to the first rung instead of rendering unstyled.
func set_voice_medium(medium_id: String) -> void:
	var resolved := medium_id.strip_edges()
	if not MEDIUM_STYLES.has(resolved):
		resolved = MEDIUM_ORAL
	if resolved == _medium_id:
		return
	_medium_id = resolved
	_apply_medium()

## The accent for a medium id — the ONE lookup, so the fork panel's header and this panel's title
## can never drift to different colours for the same medium.
static func accent_for(medium_id: String) -> Color:
	var style: Dictionary = MEDIUM_STYLES.get(medium_id, MEDIUM_STYLES[MEDIUM_ORAL])
	return style["accent"]

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

## Build the two runtime rows the scene does not author: the medium accent rule and the collapse
## toggle. Both sit between PanelCard's own header (index 0) and the scroll.
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

	_collapse_button = Button.new()
	_collapse_button.name = "TellingCollapse"
	_collapse_button.tooltip_text = COLLAPSE_TOOLTIP
	_collapse_button.focus_mode = Control.FOCUS_NONE
	_collapse_button.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	_collapse_button.add_theme_font_size_override("font_size", COLLAPSE_FONT_SIZE)
	HudStyle.apply_link_button(_collapse_button, HudStyle.INK_FAINT)
	_collapse_button.pressed.connect(_on_collapse_pressed)
	content.add_child(_collapse_button)
	content.move_child(_collapse_button, 2)

## Push the medium's title + accent. Deliberately only the title, the accent and the hairline rule:
## the panel keeps the dark card chrome every other HUD surface wears, so the voice reads as the
## SAME voice grown older rather than as three different applications.
func _apply_medium() -> void:
	var style: Dictionary = MEDIUM_STYLES.get(_medium_id, MEDIUM_STYLES[MEDIUM_ORAL])
	var accent: Color = style["accent"]
	if _panel != null:
		_panel.set_card_title(String(style["title"]))
		_panel.set_title_color(accent)
	if _accent_rule != null:
		_accent_rule.color = Color(accent.r, accent.g, accent.b, ACCENT_RULE_ALPHA)

# ---- entries ---------------------------------------------------------------

## One entry: the narration as PROSE, with the gloss as a dim secondary line. No `Turn N` prefix
## and no bold-label/italic-detail split — those are command-RECEIPT affordances and they fight the
## prose (a receipt wants scanning, a telling wants reading).
func _append_entry(kind: String, label: String, detail: String) -> void:
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
	_entries.append(message)
	while _entries.size() > ENTRY_LIMIT:
		_entries.pop_front()

# ---- rendering -------------------------------------------------------------

func render() -> void:
	if _panel == null or _label == null:
		return
	_panel.visible = true
	# Capture the read position BEFORE the text swap: once the label re-lays out, the scrollbar's
	# max moves and "were they at the bottom?" can no longer be answered.
	_was_at_tail = _is_at_tail()
	_label.text = EMPTY_TEXT if _entries.is_empty() else ENTRY_SEPARATION.join(_entries)
	_refresh_collapse()
	# Newest-at-the-bottom, so the panel reads forward like a log.
	if _scroll != null:
		_scroll.visible = not _collapsed
	# The label needs a frame to re-lay out before its content height (and hence the fit and the
	# tail position) are accurate.
	call_deferred("_resize")

## Re-run the dock fit without re-rendering the entries. The cap depends on what the cards BELOW
## this one in the dock need (see DockScrollFit), so a sibling being shown or hidden changes this
## panel's height even though its own content did not move.
func refit() -> void:
	call_deferred("_resize")

func _refresh_collapse() -> void:
	if _collapse_button == null:
		return
	_collapse_button.text = (COLLAPSE_LABEL_COLLAPSED % _entries.size()) if _collapsed else COLLAPSE_LABEL_EXPANDED

## Grow to fit, capped to the dock (shared with the command feed), then — and ONLY then — snap to
## the tail if the player was already reading it.
func _resize() -> void:
	if _scroll == null or _label == null:
		return
	if _collapsed:
		_scroll.custom_minimum_size.y = 0.0
		return
	DockScrollFit.fit(_scroll, _label, _dock_scroll, PANEL_MIN_HEIGHT, PANEL_BOTTOM_MARGIN)
	if _was_at_tail:
		_scroll.set_deferred("scroll_vertical", int(_label.get_content_height()))

## True when the view is at (or within a hair of) the newest entry — i.e. the player has NOT
## scrolled up to read back through the story. An empty/unscrollable panel is trivially at the tail.
func _is_at_tail() -> bool:
	if _scroll == null:
		return true
	var bar := _scroll.get_v_scroll_bar()
	if bar == null or bar.max_value <= bar.page:
		return true
	return bar.value + bar.page >= bar.max_value - SCROLL_TAIL_EPSILON

func _on_collapse_pressed() -> void:
	_collapsed = not _collapsed
	save_collapsed(_collapsed)
	render()
