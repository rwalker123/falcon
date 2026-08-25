class_name TurnOrbController
extends RefCounted

## Owns the TURN-ORB / ATTENTION / FORK cluster (HUD decomposition Phase 1b, docs/plan_hud_decomposition.md):
## the bottom-right turn orb's wiring, the narrative-fork panel (The Telling), and the attention-registry
## ASSEMBLY (three halves: the band/expedition rows fed in by HudLayer, the knowledge rows fed in the same
## way, and the snapshot-driven fork producer this controller owns outright). Built on the
## LegendController/FactionReadouts idiom — HudLayer holds one as `_turnorb`, keeps thin
## reflective delegators for the five methods Main reaches by reflection, and RELAYS this controller's own
## signals onto the HudLayer signals Main connects to (the controller never emits a HudLayer signal directly).
##
## Two seams to band/labor stay outside this controller: `HudLayer.update_band_alerts` FEEDS the band half
## here via `set_band_attention`, and the orb's "Jump →" band routing (`AttentionController.on_turn_orb_focus`)
## lives on that controller, reached through the relayed `focus_requested`. Holds PURE data + the orb node +
## the lazily-created fork panel; the
## controller is a RefCounted and cannot `add_child`, so it parents the fork panel into the `_host` node.

# --- The controller's OWN signals (HudLayer connects + relays each; see the class header) ---
# The player answered a fork — relayed to HudLayer.answer_fork_requested.
signal answer_fork_requested(payload: Dictionary)
# The orb face advanced the turn (after catching the book up) — relayed to HudLayer.next_turn_requested(1).
signal advance_requested
# An orb row's "Jump →" — HudLayer wires this to AttentionController.on_turn_orb_focus (the band-routing handler).
signal focus_requested(x: int, y: int)

# --- Collaborators (handed in by HudLayer) ---
var _turn_orb: TurnOrb = null
# The HUD CanvasLayer, so the RefCounted controller has a node to parent the fork panel into.
var _host: Node = null
var _telling: TellingPanel = null
## The knowledge screen, so a knowledge attention row can open it on the filter that matches the
## question the row asked. **Handed over AFTER construction** (`set_knowledge_panel`), not through
## `_init`: this controller is built in `_ready` well before `KnowledgePanelController` is, and a
## constructor argument would force one of the two to move for no reason but the wiring. The same
## late hand-over `_bandpanel.set_attention(_attention)` makes, for the same shape of reason.
var _knowledge: KnowledgePanelController = null
## Where a client-side note goes. It was the retired left-dock command feed; it is
## `HudLayer.note_system_event` now (→ `system_note_requested` → the event dock's System channel),
## injected as a Callable because the panel is `Main`'s and neither this controller nor the HUD owns
## it. Injected rather than reached for through `_host`, which is a PARENTING host, not a back-ref.
var _note_sink: Callable

# --- Owned state (moved off HudLayer) ---
# The player faction's pending forks + stance axes, cached from the snapshot. `_band_attention` is the
# band/expedition half of the orb registry, cached so a fork arriving on its own delta can rebuild the
# registry WITHOUT re-running the band producers (set_attention is a full replace, and the band producers
# consume the losing-population diff, so re-invoking update_band_alerts would silently eat that alert).
var _pending_forks: Array = []
var _stance_axes: Array = []
var _band_attention: Array = []
## The KNOWLEDGE half of the registry (`docs/plan_knowledge_screen.md` §5), cached beside the band
## half and for the same reason: `set_attention` is a full replace, so a fork arriving on its own
## delta has to be able to rebuild the whole registry without re-running the other producers.
var _knowledge_attention: Array = []
# Beats already auto-opened this session, so a fork the player dismissed does not re-open on every
# subsequent snapshot. Keyed by beat_id.
var _auto_opened_forks: Dictionary = {}
var _fork_panel: NarrativeForkPanel = null

func _init(turn_orb: TurnOrb, host: Node, telling: TellingPanel, note_sink: Callable) -> void:
	_turn_orb = turn_orb
	_host = host
	_telling = telling
	_note_sink = note_sink
	_connect_turn_orb()
	_ensure_fork_panel()

## Wire the turn orb: it re-emits the existing advance/jump signals, so the Main wiring
## (next_turn_requested / alert_focus_requested → MapView.focus_on_tile) is unchanged — the orb just
## replaces the old advance-turn button as their source.
func _connect_turn_orb() -> void:
	if _turn_orb == null:
		return
	if not _turn_orb.is_connected("focus_requested", Callable(self, "_on_turn_orb_focus")):
		_turn_orb.focus_requested.connect(_on_turn_orb_focus)
	if not _turn_orb.is_connected("advance_requested", Callable(self, "_on_turn_orb_advance")):
		_turn_orb.advance_requested.connect(_on_turn_orb_advance)
	if not _turn_orb.is_connected("panel_requested", Callable(self, "_on_turn_orb_panel_requested")):
		_turn_orb.panel_requested.connect(_on_turn_orb_panel_requested)

# ---- The Telling: the fork decision surface --------------------------------

## Build the narrative-fork panel once. It is parented into the HUD CanvasLayer (`_host`), NOT into
## `layout_root`: the panel is a modal overlay over the whole window, so it must not inset with the reserved
## docks the way the layout column does. Added last so it draws above every card. The controller is a
## RefCounted (no `add_child`), so the host node does the parenting.
func _ensure_fork_panel() -> void:
	if _fork_panel != null:
		return
	_fork_panel = NarrativeForkPanel.new()
	_fork_panel.name = "NarrativeForkPanel"
	_fork_panel.answer_selected.connect(_on_fork_answer_selected)
	_host.add_child(_fork_panel)

## Cache the PLAYER faction's pending forks and rebuild the orb registry.
##
## The caller only invokes this when the snapshot actually CARRIES the key (deltas omit an unchanged
## field), so this is never reached with "absent means cleared" semantics.
func update_pending_forks(forks_variant: Variant) -> void:
	_pending_forks = _faction_rows(forks_variant, "forks")
	_push_attention()
	_maybe_auto_open_fork()

## Cache the PLAYER faction's stance axes — what the people's behaviour and their answers together say
## about who they are. Displayed by the fork panel; no other consumer yet.
func update_stance_axes(axes_variant: Variant) -> void:
	_stance_axes = _faction_rows(axes_variant, "axes")

## The player faction's narrator MEDIUM — oral saga → painted chronicle → written record.
##
## Follows the `sedentarization` ingest precedent exactly: a per-faction wire array, scanned for
## PLAYER_FACTION_ID. The caller only invokes this when the snapshot actually CARRIES the key (a delta
## omits an unchanged field), so absence means "unchanged" and never "cleared".
##
## Presentational only — it never selects different copy. Both narrative surfaces take it, so the panel's
## title/accent and the fork panel's header age together rather than drifting apart.
func update_voice_medium(medium_variant: Variant) -> void:
	if not (medium_variant is Array):
		return
	var medium_id := ""
	for entry_variant in (medium_variant as Array):
		if entry_variant is Dictionary and int((entry_variant as Dictionary).get("faction", -1)) == HudConst.PLAYER_FACTION_ID:
			medium_id = String((entry_variant as Dictionary).get("medium_id", ""))
			break
	if _telling != null:
		_telling.set_voice_medium(medium_id)
	if _fork_panel != null:
		_fork_panel.set_voice_medium(medium_id)

## Pull the player faction's nested child array out of a per-faction wire array
## (`[{faction, <key>: [...]}, …]`), the shape `discovered_sites`/`sedentarization` also use.
func _faction_rows(variant: Variant, key: String) -> Array:
	if not (variant is Array):
		return []
	for entry_variant in (variant as Array):
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		if int(entry.get("faction", -1)) != HudConst.PLAYER_FACTION_ID:
			continue
		var rows: Variant = entry.get(key, [])
		return rows if rows is Array else []
	return []

## Store the band/expedition half of the orb registry (assembled by HudLayer.update_band_alerts) and push
## the whole registry. This is the write half of the `_band_attention` seam; the two INTERNAL callers
## (`update_pending_forks`, `_on_fork_answer_selected`) call `_push_attention` directly.
func set_band_attention(attention: Array) -> void:
	_band_attention = attention
	_push_attention()

## Store the KNOWLEDGE half of the registry and push the whole thing — the `set_band_attention` seam,
## one field over.
##
## **IT IS A SEPARATE HALF BECAUSE OF WHEN ITS ROWS CAN BE BUILT, not for tidiness.** The band
## producers run inside `HudLayer.update_band_alerts`, BEFORE that method ingests the snapshot; the
## knowledge producer reads a roster that is only correct AFTER the screen's turn diff has rolled,
## thirty lines later — and on a delta carrying knowledge but no populations, `update_band_alerts`
## does not run at all. Folded into the band half they would have to be built at a moment that is
## wrong for one of the two; as its own half each is built where its own inputs are ready.
##
## **AND IT ONLY PUSHES WHEN THE ROWS ACTUALLY CHANGED, which is rarely.** A discovery finishes on a
## handful of turns in a campaign, so this half is EMPTY on nearly every snapshot — and re-pushing an
## unchanged half costs a deep copy of the band half, a re-sort and an orb redraw, on a seam that runs
## on every delta carrying populations, knowledge or catalogues. Unguarded, that is a SECOND full
## registry push per snapshot beside `set_band_attention`'s.
##
## **The cost showed up as a FLAKE, not as a number.** `band_panel_preview`'s queue drag gestures are
## bounded by a frame budget, and the extra push was enough to drop build orders the same harness
## never drops on `main` — measured across paired runs, since a gesture that lands 53px of the 56px
## it drives for reports as a mysterious intermittent failure rather than as a slowdown.
func set_knowledge_attention(attention: Array) -> void:
	if _knowledge_attention == attention:
		return
	_knowledge_attention = attention
	_push_attention()

## The knowledge screen, handed over once `HudLayer` has built it. See `_knowledge`.
func set_knowledge_panel(knowledge: KnowledgePanelController) -> void:
	_knowledge = knowledge

## Forward the authoritative snapshot turn to the orb, so HudLayer.update_overlay's fan-out no longer
## touches the orb node directly.
func set_turn(turn: int) -> void:
	if _turn_orb != null:
		_turn_orb.set_turn(turn)

## The orb registry = the cached band/expedition producers + the cached knowledge producer + the fork
## producer, pushed as ONE replace. `TurnOrb.set_attention` replaces wholesale, so every half must fold
## in here rather than call it separately — a second call would wipe every row the others produced.
func _push_attention() -> void:
	if _turn_orb == null:
		return
	var attention: Array = _band_attention.duplicate(true)
	attention.append_array(_knowledge_attention)
	attention.append_array(_pending_fork_attention())
	_turn_orb.set_attention(attention)

## Producer 6 — a narrative fork awaiting an answer. One row per pending fork, `blocking` so the orb
## disables its `Advance ▸` footer (the client-side end-turn gate; the server never blocks).
func _pending_fork_attention() -> Array:
	var rows: Array = []
	var register := NarrativeForkPanel.load_voice_register()
	for fork_variant in _pending_forks:
		if not (fork_variant is Dictionary):
			continue
		var fork: Dictionary = fork_variant
		rows.append({
			"kind": HudAttentionVocab.ATTENTION_KIND_DECISION,
			"severity": HudAttentionVocab.ATTENTION_SEVERITY_CRITICAL,
			"blocking": true,
			"label": HudAttentionVocab.ATTENTION_DECISION_LABEL,
			"detail": _fork_row_detail(fork, register),
			"x": HudAttentionVocab.ATTENTION_NON_LOCATING, "y": HudAttentionVocab.ATTENTION_NON_LOCATING,
		})
	return rows

## The orb row's one-line taste of the question. The narration is a paragraph and orb rows CLIP, so it is
## truncated here on purpose — the full telling is the panel's job.
func _fork_row_detail(fork: Dictionary, register: String) -> String:
	var narration := NarrativeForkPanel.text_in_register(fork.get("narration", []), register)
	if narration.length() <= HudAttentionVocab.ATTENTION_DECISION_DETAIL_MAX_CHARS:
		return narration
	return narration.substr(0, HudAttentionVocab.ATTENTION_DECISION_DETAIL_MAX_CHARS).strip_edges() + HudAttentionVocab.ATTENTION_DECISION_DETAIL_ELLIPSIS

## Open the panel automatically the FIRST time a fork appears — an identity-defining moment should not
## wait behind a click. Tracked by beat_id so a dismissed fork does not re-open every snapshot.
func _maybe_auto_open_fork() -> void:
	var fork := _first_pending_fork()
	if fork.is_empty():
		return
	var beat_id := String(fork.get("beat_id", ""))
	if beat_id == "" or _auto_opened_forks.has(beat_id):
		return
	_auto_opened_forks[beat_id] = true
	_open_fork_panel()

func _first_pending_fork() -> Dictionary:
	for fork_variant in _pending_forks:
		if fork_variant is Dictionary:
			return fork_variant
	return {}

func _open_fork_panel() -> void:
	_ensure_fork_panel()
	var fork := _first_pending_fork()
	if fork.is_empty():
		return
	_fork_panel.show_fork(fork)

## A non-locating orb row was activated. The orb reports only the KIND; this controller owns which panel a
## kind opens, so a future non-locating producer needs no orb change.
##
## **A KIND WITH NO BRANCH HERE RENDERS AN AFFORDANCE THAT DOES NOTHING** — the row still wears
## `Open ▸` if `HudAttentionVocab.ATTENTION_KINDS_WITH_A_PANEL` lists it, and pressing it reaches this
## method and falls off the end. So the two lists are one decision made twice, and adding a
## non-locating producer means both.
##
## **THE KNOWLEDGE ROW OPENS ITS SCREEN ON `New this turn`** — the list holding the discovery it just
## named. A row that opened on whatever filter the player last set would make them hunt for it.
func _on_turn_orb_panel_requested(kind: String) -> void:
	if kind == HudAttentionVocab.ATTENTION_KIND_DECISION:
		_open_fork_panel()
		return
	if _knowledge == null:
		return
	if kind == HudAttentionVocab.ATTENTION_KIND_KNOWLEDGE_LEARNED:
		_knowledge.open_on_filter(HudKnowledgeVocab.FILTER_NEW)

## The player answered. Drop the fork from the local cache OPTIMISTICALLY (so the orb's gate lifts
## immediately) and let the next snapshot be authoritative.
func _on_fork_answer_selected(payload: Dictionary) -> void:
	var beat_id := String(payload.get("beat_id", ""))
	var choice_id := String(payload.get("choice_id", ""))
	if beat_id == "" or choice_id == "":
		return
	var remaining: Array = []
	for fork_variant in _pending_forks:
		if fork_variant is Dictionary and String((fork_variant as Dictionary).get("beat_id", "")) == beat_id:
			continue
		remaining.append(fork_variant)
	_pending_forks = remaining
	_push_attention()
	emit_signal("answer_fork_requested", {
		"faction": HudConst.PLAYER_FACTION_ID,
		"beat_id": beat_id,
		"choice_id": choice_id,
	})

## Is a fork holding the turn? Read by the Inspector-path advance note (the dev toolbar and autoplay are
## deliberately NOT gated — see docs/plan_the_telling.md).
func has_pending_fork() -> bool:
	return not _pending_forks.is_empty()

## The dev toolbar / autoplay advanced past an unanswered fork. Not a gate — a RECEIPT: the server will
## expire the fork to its defer branch, which is a real narrative outcome, so a developer who skipped the
## question must be able to see that they did. Routed through the same note sink every other
## client-side note takes, which lands it on the event dock's System channel.
func note_unanswered_fork() -> void:
	_note_sink.call(HudAttentionVocab.UNANSWERED_FORK_LABEL, HudAttentionVocab.UNANSWERED_FORK_DETAIL)

## An orb row's "Jump →". The band routing stays on HudLayer (`_on_turn_orb_focus` reaches into band/labor
## helpers), so the controller just re-emits its own `focus_requested`, which HudLayer relays into that
## retained handler.
func _on_turn_orb_focus(x: int, y: int) -> void:
	emit_signal("focus_requested", x, y)

func _on_turn_orb_advance() -> void:
	# Advancing the turn is "catch me up": reveal the newest telling so a player who moves on isn't left
	# parked on an old page (mid-turn beats only mark the book unread — see TellingPanel.reveal_newest).
	if _telling != null:
		_telling.reveal_newest()
	emit_signal("advance_requested")
