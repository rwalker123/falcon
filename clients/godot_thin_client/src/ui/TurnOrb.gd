extends Control
class_name TurnOrb

## The bottom-right "turn orb": a 4X-style circular widget that replaces the old
## default-theme "Advance Turn" button.
##
## It **calm-pulses only when nothing needs the player** (an empty attention
## registry) and otherwise stops pulsing, wears a count badge tinted by the
## highest-severity item, and becomes the hub for typed **attention reasons** —
## each a popover row that jumps the camera to the thing on the map.
##
## The orb is deliberately generic: it renders a list of `Attention` dictionaries
## (see the contract below) and knows nothing about who produced them, so new
## producers (wars / decisions / awaiting expeditions) slot in with no orb change.
##   Attention := {
##     kind:     String   # "idle_workers" | "war" | "decision" | …
##     severity: String   # "info" | "warn" | "critical"  → color + badge tint
##     label:    String   # "3 idle workers"      one-line summary
##     detail:   String   # "Band 2"              secondary context
##     x: int, y: int     # map focus for the jump; (-1, -1) = non-locating
##     blocking: bool     # OPTIONAL, default false — see below
##   }
##
## `blocking` is the end-turn GATE: while ANY entry carries it, the popover's `Advance ▸`
## button is disabled and wears the reason instead of its label. The orb stays generic —
## it never learns what a narrative fork is, only that *something* is holding the turn —
## and every existing producer is unaffected because the field defaults to false.
##
## THE SECOND GATE IS THE ORB'S OWN: while a turn is RESOLVING the face is inert, dimmed, and its
## number is scattered onto an orbiting ring until the answer lands. See the "resolving gate"
## constants below — `_resolving` is the one flag, and it has exactly one exit.
##
## Palette comes entirely from HudStyle (no hardcoded hexes).

const HudStyle := preload("res://src/scripts/ui/HudStyle.gd")
# Only for the awaiting-orders row icon: the orb stays producer-agnostic, but that glyph must be the
# SAME one the Band panel's awaiting row wears, so it comes from the shared registry, not a copy.
const FoodIcons := preload("res://src/scripts/ui/FoodIcons.gd")

## Jump the camera to (x, y) — reuses the Alerts-panel focus wiring in Hud/Main.
signal focus_requested(x: int, y: int)
## Advance the turn (the popover footer's Advance button).
signal advance_requested
## A NON-LOCATING row was activated: the thing it names lives in a panel, not on the map.
## Carries the entry's `kind` so the orb stays producer-agnostic — the Hud decides which
## panel a kind opens, and a new non-locating producer needs no orb change.
signal panel_requested(kind: String)

# ---- severity model --------------------------------------------------------
const SEVERITY_INFO := "info"
const SEVERITY_WARN := "warn"
const SEVERITY_CRITICAL := "critical"
const SEVERITY_RANK := {SEVERITY_CRITICAL: 3, SEVERITY_WARN: 2, SEVERITY_INFO: 1}

const KIND_IDLE_WORKERS := "idle_workers"
const KIND_STARVING := "starving"
const KIND_LOSING_POPULATION := "losing_population"
const KIND_AWAITING_ORDERS := "awaiting_orders"
# A penned herd its keeper cannot feed — it shrinks every turn (docs/plan_corral_managed_population.md).
# The icon is the corral glyph the whole husbandry ladder already wears (herd drawer badge, Corral
# policy button, the band ledger's pen-feed row), so the row reads as "your pen" at a glance.
const KIND_STARVING_PEN := "starving_pen"
# A narrative fork awaiting an answer (The Telling). Non-locating — it opens a panel, not a hex —
# and the only `blocking` producer today.
const KIND_DECISION := "decision"
# A built rung the band's keeping pool did not cover — a Tended Patch / Field whose Agriculture pool
# came up short, or a tamed herd whose Husbandry pool did. Two kinds rather than one so each web wears
# the glyph its own ladder already uses (the compose sheet's rung faces, the work board's rung mark),
# which is what lets the row say WHICH investment is bleeding before the label is read.
const KIND_UNDER_KEPT_RUNG := "under_kept_rung"
const KIND_UNDER_KEPT_HERD := "under_kept_herd"
# A finished build's crew moved — onto the new rung's keeping, or back to the idle pool
# (`docs/plan_standing_upkeep.md` §2.3). The icon is the workers' own glyph rather than a rung's: the
# row is about the HANDS, and which rung finished is already in the sim's own sentence on it.
const KIND_CREW_HANDOFF := "crew_handoff"
const KIND_ICON := {
	KIND_IDLE_WORKERS: "🛠",
	KIND_STARVING: "🍖",
	KIND_LOSING_POPULATION: "📉",
	KIND_AWAITING_ORDERS: FoodIcons.STATUS_ICONS[FoodIcons.STATUS_AWAITING],
	KIND_STARVING_PEN: FoodIcons.POLICY_ICONS[FoodIcons.POLICY_CORRAL],
	KIND_UNDER_KEPT_RUNG: FoodIcons.POLICY_ICONS[FoodIcons.POLICY_SOW],
	KIND_UNDER_KEPT_HERD: FoodIcons.POLICY_ICONS[FoodIcons.POLICY_TAME],
	KIND_CREW_HANDOFF: "🛠",
	# A question put to the people, awaiting their answer. Line art, NOT the ❔ emoji: emoji
	# presentation renders as tofu/a blob at row size (the hazard that forced MagnifierButton and
	# the policy icons to hand-draw). Verified at true size in `turn_orb_fork_blocks.png`.
	KIND_DECISION: "?",
}
const KIND_ICON_FALLBACK := "●"

# ---- geometry (named constants; no magic literals) -------------------------
# The cluster is the last, right-flush BottomBar child, sitting on the window's
# bottom-right corner. Inset the orb from those edges so the full ring
# and count badge stay on-screen with a comfortable margin.
const EDGE_MARGIN_RIGHT := 16
const EDGE_MARGIN_TOP := 14
const EDGE_MARGIN_BOTTOM := 14
const ORB_DIAMETER := 100.0
## The cluster is the ORB plus its right inset now — the `Turn N` caption that used to sit to its left is
## gone (the number lives IN the face), and the count badge is drawn INSIDE `_orb_area`
## (`_orb_area.size.x - BADGE_RADIUS - BADGE_INSET`), so nothing overhangs the orb and no extra width is
## needed for it. A 260px-wide cluster around a 100px orb made the orb read visibly off-centre in the
## dock row's rail. `EDGE_MARGIN_RIGHT` is IN the width rather than dropped: the cluster is the
## right-flush `BottomBar` child, so that inset is what keeps the ring off the window edge — and at a bare
## `ORB_DIAMETER` the `_layout`'s own right offset would instead SQUEEZE the orb by 16px.
## Declared AFTER the two it is built from — a `const` initializer is evaluated at class load.
const CLUSTER_WIDTH := ORB_DIAMETER + float(EDGE_MARGIN_RIGHT)
const CLUSTER_HEIGHT := 128.0
const FACE_DIAMETER := 74.0
const FACE_BORDER_WIDTH := 2
const RING_RADIUS := 47.0
const RING_WIDTH := 2.0
const RING_SEGMENTS := 64
# ---- the turn number ON the face -------------------------------------------
## The turn number's type size is MEASURED, never tabled: step down from the max until the string fits
## `FACE_DIAMETER * TURN_TEXT_WIDTH_FRACTION`, floored at the min. A 4-digit turn must stay legible
## inside the 74px face and must never overflow the circle, and the fraction is what keeps the fit
## derived from the face's own diameter rather than re-tuned per digit count.
const TURN_FONT_SIZE_MAX := 30
const TURN_FONT_SIZE_MIN := 13
## How much of the face's diameter the number may span. Well under 1.0: the face is a CIRCLE, so a
## chord at full diameter would touch the border, and the border is the severity tint.
const TURN_TEXT_WIDTH_FRACTION := 0.72

# ---- the curved word ABOVE the number, inside the face ----------------------
## The number is the information; the word is only its LABEL, so it is small, subordinate in alpha, and
## follows the face's own curve instead of sitting as a straight caption. Curved text is not a `Label` —
## it is per-glyph drawing with a per-glyph rotation, the same "hand-draw it rather than fight a font"
## idiom `MagnifierButton` establishes.
## UPPERCASE, matching this HUD's existing eyebrow vocabulary (`WORK`, `PARTIES`, `AT THE FIRE`);
## lowercase would be the odd one out.
const TURN_WORD := "TURN"
## TUNED BY RENDERING, and this is the TOP of the usable range — do not raise it without re-measuring.
## At 10px the run was legible but thin at a 1:1 (non-HiDPI) raster; 11px gives the letterforms body while
## staying obviously subordinate to a 23–30px number. Measured at 11px: the run spans 84° of the circle,
## its ink reaches 31.1 of the face's 37px radius (≈4px of dark clear of the 2px border), and the arc
## sits 8px above a 30px number's cap line (11px above a 4-digit 23px one). The ascent-based ceiling the
## ui_preview guard checks lands at 34.9 of 37 — ~2px of headroom, so a bump needs the frames re-read.
const TURN_WORD_FONT_SIZE := 11
## The arc's radius as a fraction of the face's OWN radius — pulled well inside the border so the glyph
## tops clear it, and high enough that the run sits above the number's cap line at every digit count.
const TURN_WORD_ARC_FRACTION := 0.62
## Extra arc length (px) inserted between glyphs. Curved text at this small an inner radius reads
## cramped with zero tracking, which is why this is a real lever rather than an implicit 0.
const TURN_WORD_TRACKING := 1.2
## The word wears the current accent — the same calm-cyan / severity tint `_style_face` gives the number
## — at reduced alpha, so it inherits the state without competing with the thing being read.
const TURN_WORD_ALPHA := 0.75
## Sanity ceiling on the COMPUTED run: a word wrapping past a third of the circle is a bug (a font or
## tracking change gone wrong), not a style choice. Asserted by ui_preview rather than clamped here —
## silently squeezing a broken layout would hide the fault instead of reporting it.
const TURN_WORD_MAX_ARC_ANGLE := TAU / 3.0

# ---- the hover hint BELOW the number, inside the face ----------------------
## THE NUMBER NEVER LEAVES THE FACE — it is the information the orb exists to show — so the click
## affordance is a small hint glyph that appears BELOW it on hover, keyed to what a click actually
## DOES (`_on_face_pressed` branches on the registry):
##   • registry EMPTY     → `HINT_GLYPH_ADVANCE`, a fast-forward pair — "this advances".
##   • registry NON-EMPTY → `HINT_GLYPH_REVIEW`, an UP-caret: the reasons popover is positioned ABOVE
##     the orb, so the caret points at where the click makes something appear. Deliberately NOT the
##     advance pair — that would promise an advance the click does not perform (it opens the popover).
##   • while RESOLVING    → no hint at all; the face is not clickable.
##
## BOTH ARE GEOMETRIC-SHAPES TRIANGLES (U+25B8 / U+25B4), and that is a rendering decision, not a
## style one. The face's old hover affordance was `‣‣` (U+2023, the TRIANGULAR BULLET) at 26px, which
## it could afford because it had the whole face to itself; a bullet's ink is only ~0.2em, so at hint
## size it rasterizes to two featureless blobs — rendered, seen, replaced. `▸` is also the glyph
## `ADVANCE_LABEL` already wears, so the hint and the popover's footer now speak the same vocabulary.
const HINT_GLYPH_ADVANCE := "▸▸"
const HINT_GLYPH_REVIEW := "▴"
## TUNED BY RENDERING at true size. The 74px face now stacks THREE things — the curved `TURN`, the
## number, and this hint — so the size and the baseline were chosen together, against the two
## clearances that can fail. MEASURED on `turn_orb_hint_advance` (turn 42, a 30px number: the TIGHTEST
## vertical case, since the number is at its largest): the hint's ink sits **5.1px below the number's
## baseline** and **6.9px clear of the 2px border**, out of 17.9px of usable band — i.e. the slack is
## split about evenly, which is what the baseline fraction was moved to buy. On `turn_orb_hint_4digit`
## (turn 1200, where the number steps down to 23px) the gap above opens to **8.1px**; the border
## clearance is unchanged, because the hint is positioned off the FACE, not off the number.
const HINT_FONT_SIZE := 22
## The hint's baseline as a fraction of the face's own DIAMETER — derived from the face rather than a
## tuned pixel offset (the `TURN_WORD_ARC_FRACTION` idiom), so a face resize carries it along.
const HINT_BASELINE_FRACTION := 0.89

# ---- the resolving gate and its in-progress animation ----------------------
## `_resolving` is THE ONE FLAG: the click gate, the face's `disabled` state and the animation's
## liveness all read it, so they cannot disagree. It is raised the moment an advance is SENT and
## lowered when the RE-FORM COMPLETES — not when the snapshot lands — so the lifetime has exactly one
## exit. The answer it waits for is "a `set_turn` with a value DIFFERENT from `_resolve_from_turn`".
##
## FAIL-OPEN, and that is the point: a rejected or dropped advance (server down, command never
## applied) produces no new snapshot EVER, and a permanently dead orb is unrecoverable for the
## player. A measured turn round-trip is ~10ms of sim plus tens of ms of client apply
## (`.claude/rules/client/turn-profiling.md`), so 8s is ~100x the healthy cost and cannot fire on a
## real turn. The timeout is NOT a special case — it re-forms the UNCHANGED number in place through
## the same path, i.e. "the answer was: nothing moved".
const RESOLVE_TIMEOUT_SEC := 8.0
## Longest frame `delta` allowed to DRIVE the animation clocks. A frame longer than this was a hitch,
## not motion — a full snapshot, a world reveal, the window losing focus — and feeding its raw delta
## in makes the animation TELEPORT: a single 2s frame consumes the whole re-form in one step and the
## digits jump to their resting places. Clamped, a hitch plays as one step and the motion is merely
## slower, never skipped. At a genuine sustained 20fps the clamp IS `delta`, so nothing changes.
## NOT applied to `_resolve_elapsed` — see `_advance_resolve_animation`.
const RESOLVE_MAX_STEP_SEC := 0.05
## The old number flies apart onto the orbit ring. Short: it is the acknowledgement of the click.
##
## **It is always seen in full.** A healthy turn answers in 0–57ms (measured, live stack), i.e. two
## or three frames in, so an answer that started the re-form the instant it landed made the flight
## imperceptible — and an acknowledgement too brief to perceive is not an acknowledgement. The answer
## is therefore HELD (`_resolve_answered`) until the scatter completes, which spends this whole
## duration on a fast turn. **This constant is the lever for that cost**: shorten it and the orb
## acknowledges sooner, at the price of a fainter break-apart.
const RESOLVE_SCATTER_SEC := 0.30
## The NEW number flies back in. Slightly longer than the scatter because the arrival IS the
## information (the turn advanced), so it settles rather than snaps.
const RESOLVE_REFORM_SEC := 0.34
## Seconds per revolution, shared by the orbiting glyphs and the ring's sweep arc — they read as ONE
## motion because they run off the SAME angular clock (`_orbit_phase`).
const RESOLVE_ORBIT_PERIOD := 1.6
## The orbit ring's radius as a fraction of the FACE's own radius (the `TURN_WORD_ARC_FRACTION`
## idiom — derived from the face, not a tuned pixel offset). TUNED BY RENDERING, together with
## `RESOLVE_DIGIT_FONT_SIZE`, on `turn_orb_resolving`: 0.58 puts the ring at 21.5 of the face's 37px
## radius, so a glyph's ink reaches ~27.6 — **7.4px inside the 2px border**. At 0.52 the digits read
## as huddled around the centre rather than circling. The `TURN` word's arc is not a constraint here:
## it is hidden for the whole animation (`_show_turn_word`), so the border is.
const RESOLVE_ORBIT_RADIUS_FRACTION := 0.58
## Type size of a glyph riding the ring — the size the scatter lerps DOWN to and the re-form lerps
## back UP from. Large enough to stay legible at 1:1; small enough that a 4-digit turn does not crowd
## the ring (four slots on a 135px ring = a 34px pitch against a ~10px glyph).
const RESOLVE_DIGIT_FONT_SIZE := 17
## Glyphs stay UPRIGHT for the whole flight — no per-glyph rotation, unlike the curved `TURN` word.
## That is a choice, not an omission: the orbit itself carries the motion, and a rotating digit is
## harder to read than a moving one.
## How much of the circle the ring's rotating "working" arc spans. A quarter reads unmistakably as a
## sweep; much more and it stops looking like it is going anywhere.
const RESOLVE_SWEEP_ARC_FRACTION := 0.25
## Thicker than the static base ring (`RING_WIDTH`) so the moving arc reads ON TOP of it rather than
## as a brightness change in it.
const RESOLVE_SWEEP_WIDTH := 3.0
const RESOLVE_SWEEP_SEGMENTS := 24
## A glyph's optical centre sits about this fraction of an ASCENT above its baseline (digits have no
## descender, so their ink runs from the baseline to roughly the cap line). Used only to convert a
## ring POINT into a baseline draw origin, so a flying glyph is centred on its slot rather than
## hanging below it.
const GLYPH_OPTICAL_CENTRE_ASCENT_FRACTION := 0.35
## The face is `disabled` while resolving, so the default theme's disabled look would leak in without
## an override. It is the `normal` box with the border (the severity tint) faded — that IS the
## "dimmed face" the gate needs: same shape, same weight, visibly not yours to press.
const FACE_DISABLED_BORDER_ALPHA := 0.45

# Calm pulse (only while the registry is empty).
const PULSE_PERIOD := 2.6            # seconds for a full breath
const PULSE_ALPHA_MIN := 0.30
const PULSE_ALPHA_MAX := 0.85
const PULSE_RADIUS_MIN := 44.0
const PULSE_RADIUS_MAX := 47.0
const PULSE_WIDTH := 1.5
const PULSE_DASH_COUNT := 22
const PULSE_DASH_FRACTION := 0.42    # portion of each dash slot that's stroked
const PULSE_ARC_SEGMENTS := 4

# Count badge (only while the registry is non-empty).
const BADGE_RADIUS := 13.0
const BADGE_INSET := 3.0
const BADGE_FONT_SIZE := 13

# Popover. WIDTH is sized to the widest producer row — `awaiting_orders`, whose detail names the
# mission AND its objective ("Hunting expedition · Red Deer"). A row's inner HBox is anchored to its
# Button (not a container child), so its min size does NOT grow the Button: an over-wide row used to
# spill its `Jump →` OUTSIDE the card instead of widening it. The labels below therefore also clip
# (ellipsis), which bounds ANY future producer's text to the card rather than letting it escape.
const POPOVER_WIDTH := 420
const POPOVER_GAP := 14.0
const ROW_MIN_HEIGHT := 52.0
const ROW_H_PADDING := 12
const ROW_SEPARATION := 12
const SEV_STRIPE_WIDTH := 3
const ROW_ICON_SIZE := 30
# **THE DETAIL IS SMALL PRINT AND NOW SAYS SO.** It took the theme's default size, i.e. the LABEL's,
# and read as a second headline in a fainter ink — while the rows CLIP, so every point it did not
# need was a word of the detail cut off the card. Under-kept rows carry three facts (the pool, its
# bill and the countdown) and lost the countdown to that clip; at 12 the widest of them fits with the
# card, its position and its row height all unchanged, which is what makes this cheaper than widening
# `POPOVER_WIDTH`. `ROW_MIN_HEIGHT` floors the row, so the shorter line costs no height either.
const ROW_DETAIL_FONT_SIZE := 12

# The end-turn gate. When a `blocking` entry is present the footer button wears the reason in
# place of `Advance ▸` — an unexplained dead button is worse than no button at all.
## The face's tooltips — one per click semantic (see `_refresh_face_text`). The number on the face is
## self-evident and needs no "Turn" word beside it; what is NOT self-evident is what a click does.
const TOOLTIP_ADVANCE_FORMAT := "Advance to turn %d"
const TOOLTIP_REVIEW_FORMAT := "%d item%s need your attention — click to review"
## While the gate is up the face names the turn IN FLIGHT, not the one on the face: what the player
## needs to know is that their click was taken and is being worked.
const TOOLTIP_RESOLVING_FORMAT := "Resolving turn %d…"
const ADVANCE_LABEL := "Advance ▸"
# Deliberately NOT the same string as the entry row's label (`Hud.ATTENTION_DECISION_LABEL`):
# the row states what is waiting, the footer states why you cannot advance. Repeating the row
# verbatim reads like a rendering bug rather than a reason.
const ADVANCE_BLOCKED_LABEL := "Answer first to advance"
## The SECOND reason the footer's advance can be dead. Both reasons go through one
## `_advance_block_label()`, so the footer has a single "why not" channel rather than a bool per cause.
const ADVANCE_RESOLVING_LABEL := "Resolving…"

## The in-progress animation's phases. `_anim_time` is seconds INSIDE the current phase; `_orbit_phase`
## is the shared angular clock (radians) and advances through all of them.
enum { ANIM_NONE, ANIM_SCATTER, ANIM_ORBIT, ANIM_REFORM }

var _entries: Array = []
var _accent_color: Color = HudStyle.SIGNAL
var _turn: int = 0
var _pulse_time: float = 0.0

## THE ONE FLAG (see the resolving-gate constants): true from the moment an advance is sent until the
## re-form animation finishes — or the fail-open timeout fires.
var _resolving: bool = false
## The turn number at request time. The answer the orb is waiting for is a `set_turn` with a
## DIFFERENT value, so this is what "different" is measured against.
var _resolve_from_turn: int = 0
## Seconds spent AWAITING the answer (scatter + orbit). Not accumulated during the re-form, which is
## the answer already landing.
var _resolve_elapsed: float = 0.0
## The answer landed while the digits were still flying OUT. The re-form cannot start from there —
## it begins at `k = 1.0` (fully on the ring), so entering it mid-scatter teleports the glyphs the
## rest of the way. Instead the answer is held here and the scatter's own completion branch routes
## to `ANIM_REFORM` rather than `ANIM_ORBIT`, so the re-form is only ever entered from a state where
## the digits genuinely ARE on the ring and no start-`k` bookkeeping is needed.
var _resolve_answered: bool = false
var _anim_phase: int = ANIM_NONE
var _anim_time: float = 0.0
var _orbit_phase: float = 0.0

var _layout: HBoxContainer
var _orb_area: Control
var _face: Button
## The curved-`TURN` overlay — a child of `_face`, NOT of `_orb_area`. See `_ready` for why.
var _turn_word: Control
## The hover-hint overlay (the glyph BELOW the number) — a child of `_face`, same reason.
var _face_hint: Control
## The in-flight digits overlay — a child of `_face`, same reason. While any animation phase runs it
## draws the number itself and the Button holds no text.
var _face_digits: Control
## True while the pointer is over the face. Tracked (rather than read off the Button) because the hint
## must be re-evaluated when the attention registry changes, not only on enter/exit — see
## `_refresh_face_text`.
var _face_hovered: bool = false
## The hint glyph the face currently shows below the number, `""` for none. Written by the one place
## that decides it (`_refresh_face_text`) and read by the hint overlay, so the two cannot disagree.
var _hint_glyph: String = ""

var _popover: PanelContainer = null
var _catcher: Control = null
var _popover_open: bool = false


func _ready() -> void:
	custom_minimum_size = Vector2(CLUSTER_WIDTH, CLUSTER_HEIGHT)
	# Fill the bottom bar's height so the orb can center within it (the bar grows to
	# the tallest corner widget); the edge margins below keep it off the window edges.
	size_flags_vertical = Control.SIZE_FILL
	mouse_filter = Control.MOUSE_FILTER_IGNORE

	_layout = HBoxContainer.new()
	_layout.set_anchors_preset(Control.PRESET_FULL_RECT)
	_layout.offset_top = EDGE_MARGIN_TOP
	_layout.offset_bottom = -EDGE_MARGIN_BOTTOM
	_layout.offset_right = -EDGE_MARGIN_RIGHT
	# One child (the orb) now that the caption is gone, so there is no separation to set; END keeps it
	# flush to the inset right edge, which is where the bottom-bar corner wants it.
	_layout.alignment = BoxContainer.ALIGNMENT_END
	_layout.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_layout)

	_orb_area = Control.new()
	_orb_area.custom_minimum_size = Vector2(ORB_DIAMETER, ORB_DIAMETER)
	_orb_area.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_orb_area.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_orb_area.draw.connect(_on_orb_area_draw)
	_orb_area.resized.connect(_position_face)
	_layout.add_child(_orb_area)

	_face = Button.new()
	_face.focus_mode = Control.FOCUS_NONE
	_face.custom_minimum_size = Vector2(FACE_DIAMETER, FACE_DIAMETER)
	_face.size = Vector2(FACE_DIAMETER, FACE_DIAMETER)
	_face.pressed.connect(_on_face_pressed)
	# The hover swap follows the CLICK semantics (see `_refresh_face_text`), so it is re-evaluated on
	# enter/exit AND whenever the registry changes.
	_face.mouse_entered.connect(func() -> void: _set_face_hovered(true))
	_face.mouse_exited.connect(func() -> void: _set_face_hovered(false))
	_orb_area.add_child(_face)

	# The curved word gets its OWN overlay, a child of `_face`. `_face` is itself a child of `_orb_area`,
	# so EVERY draw command `_orb_area` issues renders BEHIND the face's stylebox (the count badge
	# included) — drawing the word in `_on_orb_area_draw` would bury it under the filled face. A child of
	# the face renders above it, and this reuses the same `draw.connect` idiom `_orb_area` already uses:
	# no new script file, nothing relocated.
	_turn_word = Control.new()
	_turn_word.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_turn_word.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_turn_word.draw.connect(_on_turn_word_draw)
	_face.add_child(_turn_word)

	# Two more overlays on the SAME idiom, and for the same reason: the hover hint and the in-flight
	# digits both have to render ABOVE the face's stylebox.
	_face_hint = Control.new()
	_face_hint.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_face_hint.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_face_hint.draw.connect(_on_face_hint_draw)
	_face.add_child(_face_hint)

	_face_digits = Control.new()
	_face_digits.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_face_digits.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_face_digits.draw.connect(_on_face_digits_draw)
	_face.add_child(_face_digits)

	_position_face()
	_refresh_face_text()

	set_turn(_turn)
	_recompute()

func _process(delta: float) -> void:
	# The frame clock now serves TWO things — the calm pulse (only while nothing needs the player) and
	# the resolve animation — so `_recompute` enables it for EITHER (`ready or _resolving`).
	_pulse_time += delta
	_advance_resolve_animation(delta)
	_orb_area.queue_redraw()

func _position_face() -> void:
	if _face == null or _orb_area == null:
		return
	_face.position = (_orb_area.size - Vector2(FACE_DIAMETER, FACE_DIAMETER)) * 0.5

# ---- public API ------------------------------------------------------------

## Replace the attention registry, recompute ready/badge/tint, restart or stop
## the pulse, and (if open) rebuild the popover.
func set_attention(entries: Array) -> void:
	_entries = entries.duplicate(true)
	_entries.sort_custom(_sort_by_severity_desc)
	_recompute()
	if _popover_open:
		# If the registry emptied while the popover was open, there is nothing left
		# to triage — close it rather than rebuild an empty reasons list (an empty
		# popover has no purpose, and the orb face now advances directly instead).
		if _entries.is_empty():
			_close_popover()
		else:
			_rebuild_popover()

## The turn number the face carries — and, while the gate is up, THE ANSWER the orb is waiting for.
## A `set_turn` with a value different from `_resolve_from_turn` is the only thing that says the
## server resolved the turn. The gate lifts when the re-form COMPLETES, not here: one flag, one
## lifetime, one exit.
func set_turn(turn: int) -> void:
	_turn = turn
	# WHY `ANIM_REFORM` IS EXCLUDED FROM THE GUARD: a re-form already in flight is left running.
	# `_digits_text()` reads `_turn` live, so a newer turn arriving mid-re-form is absorbed by the same
	# animation instead of throwing the glyphs back out. Everything below is therefore the
	# NOT-re-forming case.
	if _resolving and turn != _resolve_from_turn and _anim_phase != ANIM_REFORM:
		if _anim_phase == ANIM_SCATTER:
			# Mid-flight OUT: record the answer and let the scatter finish. Its completion branch
			# routes to the re-form. See `_resolve_answered`.
			_resolve_answered = true
		else:
			_begin_reform()
		return
	_refresh_face_text()

## Open the popover programmatically (used by the ui_preview harness).
func open_popover() -> void:
	if not _popover_open:
		toggle_popover()

## Orb-face click. Advancing the turn must ALWAYS be possible from the orb, so the
## click behaviour depends on the attention registry:
##   • empty ("nothing needs you") → advance the turn directly, exactly as the
##     popover's `Advance ▸` footer would, and open NO popover (an empty reasons
##     list has nothing to review — and, unpositioned, rendered as a blank box that
##     pushed its own Advance affordance off-screen, trapping the player).
##   • non-empty → toggle the reasons popover so the player can triage first.
## THE FACE'S HINT AND TOOLTIP FOLLOW THE CLICK SEMANTICS, because `_on_face_pressed` BRANCHES on the
## registry: empty → advance the turn directly; non-empty → toggle the reasons popover; resolving →
## nothing at all. The NUMBER is not part of that branch — it never leaves the face — so what carries
## the affordance is the hint glyph BELOW it (see `HINT_GLYPH_REVIEW`):
##   * registry EMPTY — hover shows the advance pair, tooltip names the turn it would advance TO.
##   * registry NON-EMPTY — hover shows the up-caret (the popover opens above), tooltip names the
##     count and says the click reviews.
##   * RESOLVING — no hint, and the tooltip names the turn in flight.
## Called from `set_turn`, `_recompute` (registry changed), the hover handlers and every animation
## transition, so no path can strand a hint or a stale number on the face.
func _refresh_face_text() -> void:
	if _face == null:
		return
	var advances := _entries.is_empty()
	# While the digits are in flight the OVERLAY draws them, so the Button must hold NO text or the
	# two would render on top of each other. The string is handed back when the re-form completes.
	if _anim_phase == ANIM_NONE:
		var text := str(_turn)
		_face.text = text
		_face.add_theme_font_size_override("font_size", _turn_font_size(text))
	else:
		_face.text = ""
	if _resolving or not _face_hovered:
		_hint_glyph = ""
	else:
		_hint_glyph = HINT_GLYPH_ADVANCE if advances else HINT_GLYPH_REVIEW
	_queue_turn_word_redraw()
	_queue_face_overlay_redraw()
	if _resolving:
		_face.tooltip_text = TOOLTIP_RESOLVING_FORMAT % (_resolve_from_turn + 1)
	elif advances:
		_face.tooltip_text = TOOLTIP_ADVANCE_FORMAT % (_turn + 1)
	else:
		var n := _entries.size()
		_face.tooltip_text = TOOLTIP_REVIEW_FORMAT % [n, "" if n == 1 else "s"]

func _set_face_hovered(hovered: bool) -> void:
	if hovered == _face_hovered:
		return
	_face_hovered = hovered
	_refresh_face_text()

## The largest size in `[TURN_FONT_SIZE_MIN, TURN_FONT_SIZE_MAX]` at which `text` fits the face's usable
## chord. MEASURED against the button's own font — a per-digit-count table would drift the moment the
## theme font changed, and a single fixed size either clips turn 1200 or wastes the face on turn 1.
func _turn_font_size(text: String) -> int:
	var font := _face.get_theme_font("font")
	if font == null:
		return TURN_FONT_SIZE_MIN
	var budget := FACE_DIAMETER * TURN_TEXT_WIDTH_FRACTION
	var size := TURN_FONT_SIZE_MAX
	while size > TURN_FONT_SIZE_MIN:
		if font.get_string_size(text, HORIZONTAL_ALIGNMENT_CENTER, -1, size).x <= budget:
			return size
		size -= 1
	return TURN_FONT_SIZE_MIN

func _on_face_pressed() -> void:
	# THE GATE. Without it, mashing the face queues N advances while the server is still resolving
	# turn 1. The Button is `disabled` while resolving too, so this is the second line of defence —
	# and the one that holds for any caller reaching the handler directly.
	if _resolving:
		return
	if _entries.is_empty():
		emit_signal("advance_requested")
		_begin_resolving()
		return
	toggle_popover()

# ---- the resolving gate ----------------------------------------------------

## Is a turn in flight? True from the advance being SENT to the re-form completing.
func is_resolving() -> bool:
	return _resolving

## Raise the gate: an advance has just gone out. Everything that reads `_resolving` is refreshed
## through `_recompute` (the face's `disabled` state, its text/tooltip/hint, the frame clock, the ring).
func _begin_resolving() -> void:
	_resolving = true
	_resolve_from_turn = _turn
	_resolve_elapsed = 0.0
	_resolve_answered = false
	_anim_phase = ANIM_SCATTER
	_anim_time = 0.0
	# Start every cycle at a known angle. The animation is then a pure function of its OWN elapsed
	# time — which is what makes the ui_preview frame reproducible instead of a function of how many
	# turns happened to have been advanced before it.
	_orbit_phase = 0.0
	_recompute()
	if _popover_open:
		_rebuild_popover()

## The answer landed (or the fail-open timeout said "nothing moved"): fly the digits back in. ONE path
## for both, deliberately — the timeout is not a special case, it is an answer of "no change", so it
## re-forms the unchanged number in place and the gate lifts through the same exit.
func _begin_reform() -> void:
	_anim_phase = ANIM_REFORM
	_anim_time = 0.0
	_refresh_face_text()

## The one exit. The Button takes its string back here, which is why the re-form's resting positions
## must match where the Button draws — otherwise the hand-back visibly pops.
func _finish_resolving() -> void:
	_anim_phase = ANIM_NONE
	_anim_time = 0.0
	_resolving = false
	_resolve_elapsed = 0.0
	_resolve_answered = false
	_recompute()
	if _popover_open:
		_rebuild_popover()

## Advance the animation's clocks by `delta` and run its phase transitions. Split out of `_process` so
## the ui_preview harness can step it directly: that harness freezes `Engine.time_scale`, so `_process`
## sees `delta == 0` and nothing here would ever move on its own.
func _advance_resolve_animation(delta: float) -> void:
	if _anim_phase == ANIM_NONE:
		return
	# TWO clocks are driven from here by TWO DIFFERENT deltas, and that split is the subtle part.
	# The ANIMATION clocks take the CLAMPED step (`RESOLVE_MAX_STEP_SEC`): a hitch must play as one
	# frame of motion rather than teleport through a whole phase.
	var step: float = minf(delta, RESOLVE_MAX_STEP_SEC)
	# ONE angular clock for the flying glyphs AND the ring's sweep arc, so they cannot drift apart.
	_orbit_phase = fposmod(_orbit_phase + step * TAU / RESOLVE_ORBIT_PERIOD, TAU)
	_anim_time += step
	match _anim_phase:
		ANIM_SCATTER:
			if _anim_time >= RESOLVE_SCATTER_SEC:
				# Carry the remainder so a coarse step cannot lose motion at the seam.
				var overshoot: float = _anim_time - RESOLVE_SCATTER_SEC
				if _resolve_answered:
					# The answer arrived while the digits were still flying OUT and was held until
					# now (see `_resolve_answered`). They are on the ring, so the re-form is entered
					# truthfully at `k = 1.0` instead of teleporting the rest of the way.
					_begin_reform()
				else:
					_anim_phase = ANIM_ORBIT
				_anim_time = overshoot
		ANIM_REFORM:
			if _anim_time >= RESOLVE_REFORM_SEC:
				_finish_resolving()
				return
	# The timeout counts only while AWAITING the answer. During the re-form the answer has already
	# landed, so a slow frame there must not restart the flight.
	#
	# IT TAKES THE RAW `delta`, NOT `step`. This is a WALL-CLOCK safety net, not motion: clamping it
	# would make a stalled client's dead orb sit far past the real 8s before failing open — the
	# longer the stall, the later the rescue, which is exactly backwards.
	if _anim_phase != ANIM_REFORM:
		_resolve_elapsed += delta
		if _resolve_elapsed >= RESOLVE_TIMEOUT_SEC:
			_begin_reform()
	_queue_face_overlay_redraw()

func toggle_popover() -> void:
	if _popover_open:
		_close_popover()
	else:
		_open_popover()

# ---- recompute + draw ------------------------------------------------------

func _sort_by_severity_desc(a: Variant, b: Variant) -> bool:
	return _rank(a) > _rank(b)

func _rank(entry: Variant) -> int:
	if entry is Dictionary:
		return int(SEVERITY_RANK.get(String(entry.get("severity", SEVERITY_INFO)), 0))
	return 0

func _severity_color(severity: String) -> Color:
	match severity:
		SEVERITY_CRITICAL:
			return HudStyle.DANGER
		SEVERITY_WARN:
			return HudStyle.WARN
		_:
			return HudStyle.SIGNAL

func _highest_severity_color() -> Color:
	var best_rank := 0
	var color := HudStyle.SIGNAL
	for entry in _entries:
		var r := _rank(entry)
		if r > best_rank:
			best_rank = r
			color = _severity_color(String(entry.get("severity", SEVERITY_INFO)))
	return color

func _kind_icon(kind: String) -> String:
	return String(KIND_ICON.get(kind, KIND_ICON_FALLBACK))

func _recompute() -> void:
	var ready := _entries.is_empty()
	_accent_color = HudStyle.SIGNAL if ready else _highest_severity_color()
	_style_face(_accent_color)
	# ONE flag: the click gate, the Button's own `disabled` state and the animation all read `_resolving`.
	if _face != null:
		_face.disabled = _resolving
	# The registry decides what a click DOES, so it also decides which hint hovering may show.
	# Re-evaluated here so entries arriving while the pointer rests on the face cannot strand a hint.
	_refresh_face_text()
	# A frame clock is needed by the calm pulse (all-clear only) AND by the resolve animation.
	set_process(ready or _resolving)
	if _orb_area != null:
		_orb_area.queue_redraw()

func _style_face(accent: Color) -> void:
	if _face == null:
		return
	var normal := StyleBoxFlat.new()
	normal.bg_color = HudStyle.PANEL_SOLID
	normal.set_corner_radius_all(int(FACE_DIAMETER * 0.5))
	normal.set_border_width_all(FACE_BORDER_WIDTH)
	normal.border_color = accent
	# Subtle cyan inner glow: a faint signal-wash highlight sitting inside the face.
	normal.shadow_color = HudStyle.SIGNAL_WASH
	normal.shadow_size = 4
	var hover := normal.duplicate() as StyleBoxFlat
	hover.border_color = HudStyle.SIGNAL if accent == HudStyle.SIGNAL else accent
	hover.shadow_size = 8
	# The gate disables the face, so `disabled` MUST be overridden too or the default theme's own
	# disabled look leaks in beside four hand-styled states. It is `normal` with the border faded.
	var dimmed := accent
	dimmed.a *= FACE_DISABLED_BORDER_ALPHA
	var disabled := normal.duplicate() as StyleBoxFlat
	disabled.border_color = dimmed
	_face.add_theme_stylebox_override("normal", normal)
	_face.add_theme_stylebox_override("hover", hover)
	_face.add_theme_stylebox_override("pressed", hover)
	_face.add_theme_stylebox_override("disabled", disabled)
	_face.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	_face.add_theme_color_override("font_color", accent)
	_face.add_theme_color_override("font_hover_color", accent)
	_face.add_theme_color_override("font_pressed_color", accent)
	# The Button carries no text while disabled (the overlay draws the digits), but override the
	# colour anyway so the default theme's grey can never appear if that ever changes.
	_face.add_theme_color_override("font_disabled_color", dimmed)
	# The word carries the same accent, so a severity change must repaint it too — a stale overlay beside
	# a re-tinted number is the likeliest bug here.
	_queue_turn_word_redraw()

func _on_orb_area_draw() -> void:
	var center := _orb_area.size * 0.5
	# Static base ring behind the face.
	_orb_area.draw_arc(center, RING_RADIUS, 0.0, TAU, RING_SEGMENTS, HudStyle.LINE_SOFT, RING_WIDTH, true)
	if _resolving:
		# INSTEAD of the calm pulse, never beside it: the pulse means "nothing needs you", which is
		# exactly the wrong thing to say while the turn is being worked.
		_draw_resolve_sweep(center)
	elif _entries.is_empty():
		_draw_pulse(center)
	# The badge is orthogonal to both — it counts the registry, which the gate does not change.
	if not _entries.is_empty():
		_draw_badge()

func _draw_pulse(center: Vector2) -> void:
	# 0..1 breath from a cosine so it eases at both ends.
	var t := 0.5 - 0.5 * cos(_pulse_time * TAU / PULSE_PERIOD)
	var col := HudStyle.SIGNAL
	col.a = lerpf(PULSE_ALPHA_MIN, PULSE_ALPHA_MAX, t)
	var radius := lerpf(PULSE_RADIUS_MIN, PULSE_RADIUS_MAX, t)
	var span := TAU / float(PULSE_DASH_COUNT)
	for i in PULSE_DASH_COUNT:
		var a0 := float(i) * span
		_orb_area.draw_arc(center, radius, a0, a0 + span * PULSE_DASH_FRACTION, PULSE_ARC_SEGMENTS, col, PULSE_WIDTH, true)

## The ring's "working" read: one arc riding the base ring at the SAME angular clock as the orbiting
## glyphs, so the two are visibly one motion.
func _draw_resolve_sweep(center: Vector2) -> void:
	var span := TAU * RESOLVE_SWEEP_ARC_FRACTION
	_orb_area.draw_arc(center, RING_RADIUS, _orbit_phase, _orbit_phase + span,
		RESOLVE_SWEEP_SEGMENTS, _accent_color, RESOLVE_SWEEP_WIDTH, true)

func _draw_badge() -> void:
	var badge_center := Vector2(_orb_area.size.x - BADGE_RADIUS - BADGE_INSET, BADGE_RADIUS + BADGE_INSET)
	_orb_area.draw_circle(badge_center, BADGE_RADIUS, _accent_color)
	var font := _orb_area.get_theme_default_font()
	if font == null:
		return
	var text := str(_entries.size())
	var text_size := font.get_string_size(text, HORIZONTAL_ALIGNMENT_CENTER, -1, BADGE_FONT_SIZE)
	var origin := badge_center + Vector2(-text_size.x * 0.5, BADGE_FONT_SIZE * 0.35)
	_orb_area.draw_string(font, origin, text, HORIZONTAL_ALIGNMENT_LEFT, -1, BADGE_FONT_SIZE, HudStyle.GROUND)

# ---- the curved word -------------------------------------------------------

## The face's font, for the curved word. Falls back to the engine default so the word is never silently
## dropped when the button carries no theme font of its own.
func _word_font() -> Font:
	var font: Font = _face.get_theme_font("font") if _face != null else null
	return font if font != null else ThemeDB.fallback_font

## THE WORD'S WHOLE LAYOUT ARITHMETIC, in ONE place — the draw and the ui_preview assertion both read it,
## so the guard can never measure something the renderer does not.
##
## Per-glyph advances come from the FONT (`get_char_size`), never an assumed uniform width: a fixed
## advance spaces the letters unevenly around the arc and the word looks drunk. From those:
##   arc_length = Σ advances + TURN_WORD_TRACKING × (glyph_count − 1)
##   arc_angle  = arc_length / radius        (what `TURN_WORD_MAX_ARC_ANGLE` ceilings)
##   glyph_height = the font's ascent, i.e. how far a glyph reaches RADIALLY OUTWARD from the arc, since
##                  each glyph is drawn on its baseline with the arc as its baseline.
func turn_word_metrics() -> Dictionary:
	var font := _word_font()
	var advances := PackedFloat32Array()
	var arc_length := 0.0
	for i in TURN_WORD.length():
		var advance: float = font.get_char_size(TURN_WORD.unicode_at(i), TURN_WORD_FONT_SIZE).x
		advances.append(advance)
		arc_length += advance
	if advances.size() > 1:
		arc_length += TURN_WORD_TRACKING * float(advances.size() - 1)
	var radius := FACE_DIAMETER * 0.5 * TURN_WORD_ARC_FRACTION
	return {
		"font": font,
		"advances": advances,
		"radius": radius,
		"arc_length": arc_length,
		"arc_angle": arc_length / radius,
		"glyph_height": font.get_ascent(TURN_WORD_FONT_SIZE),
	}

## Does the word draw at all? It LABELS the number, and the number is now ALWAYS on the face except
## while it is scattered onto the orbit ring — so the word is shown iff nothing is animating. (It no
## longer keys on hover: hovering used to swap the number out for a glyph, and no longer does.)
## Deliberately ONE named branch, so flipping to a "TURN ‣‣" verb phrase later is a one-line change.
func _show_turn_word() -> bool:
	return _anim_phase == ANIM_NONE

func _queue_turn_word_redraw() -> void:
	if _turn_word != null:
		_turn_word.queue_redraw()

## The two face overlays that are NOT the word. Kept together because every state change touches both:
## the hint hides while resolving and the digits only draw while resolving.
func _queue_face_overlay_redraw() -> void:
	if _face_hint != null:
		_face_hint.queue_redraw()
	if _face_digits != null:
		_face_digits.queue_redraw()

## Draw `TURN` along the top of the face's circle, one rotated glyph at a time. Face-local coordinates;
## canvas +y is DOWN, so the circle's apex is at `-PI/2` and the run is centred on it.
func _on_turn_word_draw() -> void:
	if _turn_word == null or not _show_turn_word():
		return
	var metrics := turn_word_metrics()
	var font: Font = metrics["font"]
	var advances: PackedFloat32Array = metrics["advances"]
	var radius: float = metrics["radius"]
	var total_angle: float = metrics["arc_angle"]
	var center := Vector2(FACE_DIAMETER, FACE_DIAMETER) * 0.5
	var color := _accent_color
	color.a *= TURN_WORD_ALPHA
	var tracking_angle := TURN_WORD_TRACKING / radius
	var angle := -PI * 0.5 - total_angle * 0.5
	for i in advances.size():
		var advance := advances[i]
		var half_angle := advance / radius * 0.5
		angle += half_angle
		var pos := center + radius * Vector2(cos(angle), sin(angle))
		# `angle + PI/2` puts the baseline tangent to the arc with the glyph upright relative to it.
		_turn_word.draw_set_transform(pos, angle + PI * 0.5)
		_turn_word.draw_char(
			font, Vector2(-advance * 0.5, 0.0), TURN_WORD.substr(i, 1), TURN_WORD_FONT_SIZE, color)
		angle += half_angle + tracking_angle
	# MANDATORY: a transform left set corrupts every subsequent draw call on this canvas item.
	_turn_word.draw_set_transform_matrix(Transform2D.IDENTITY)

# ---- the hover hint --------------------------------------------------------

## Draw the hint glyph centred under the number, its baseline derived from the face's own diameter.
## No transform is set here, so none has to be cleared.
func _on_face_hint_draw() -> void:
	if _face_hint == null or _hint_glyph.is_empty():
		return
	var font := _word_font()
	var width := font.get_string_size(_hint_glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, HINT_FONT_SIZE).x
	var origin := Vector2((FACE_DIAMETER - width) * 0.5, FACE_DIAMETER * HINT_BASELINE_FRACTION)
	_face_hint.draw_string(
		font, origin, _hint_glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, HINT_FONT_SIZE, _accent_color)

# ---- the in-flight digits --------------------------------------------------

## Which number is in flight. Scattering/orbiting carries the OLD one; the re-form is built from the
## NEW one, which is what makes a digit-count change (9 → 10, 999 → 1000) need no old-to-new matching
## at all — the slot count simply follows the string being drawn. On the fail-open timeout the two are
## the same number, so it re-forms in place.
func _digits_text() -> String:
	return str(_turn) if _anim_phase == ANIM_REFORM else str(_resolve_from_turn)

## Where the BUTTON would draw each character of `text`: one baseline origin per character, centring
## the run horizontally and the line box vertically, exactly as a centred Button does. This has to
## match, or the hand-back at the end of the re-form visibly pops — hence the same font and the same
## MEASURED size (`_turn_font_size`) the Button itself uses.
func _rest_glyph_origins(text: String, font: Font, size: int) -> PackedVector2Array:
	var origins := PackedVector2Array()
	var total := 0.0
	for i in text.length():
		total += font.get_char_size(text.unicode_at(i), size).x
	var ascent := font.get_ascent(size)
	var baseline := (FACE_DIAMETER - (ascent + font.get_descent(size))) * 0.5 + ascent
	var x := (FACE_DIAMETER - total) * 0.5
	for i in text.length():
		origins.append(Vector2(x, baseline))
		x += font.get_char_size(text.unicode_at(i), size).x
	return origins

## Cubic ease-OUT — leaves the number fast and settles onto the ring. Used by the scatter.
func _ease_out(t: float) -> float:
	var u := 1.0 - clampf(t, 0.0, 1.0)
	return 1.0 - u * u * u

## Ease in AND out — the re-form leaves the ring and arrives at the number gently, because the
## arrival is the information.
func _ease_in_out(t: float) -> float:
	return smoothstep(0.0, 1.0, clampf(t, 0.0, 1.0))

## The characters of the in-flight number, each somewhere between its resting place on the face and
## its evenly-spaced slot on the orbit ring. `k` is that journey: 0 = at rest, 1 = on the ring.
##
## Glyphs stay UPRIGHT throughout (see `RESOLVE_DIGIT_FONT_SIZE`'s note) — no `draw_set_transform` is
## used here at all, so there is no transform to clear.
func _on_face_digits_draw() -> void:
	if _face_digits == null or _anim_phase == ANIM_NONE:
		return
	var text := _digits_text()
	if text.is_empty():
		return
	var font := _word_font()
	var rest_size := _turn_font_size(text)
	var rest_origins := _rest_glyph_origins(text, font, rest_size)
	var count := text.length()
	var center := Vector2(FACE_DIAMETER, FACE_DIAMETER) * 0.5
	var ring_radius := FACE_DIAMETER * 0.5 * RESOLVE_ORBIT_RADIUS_FRACTION
	var slot_span := TAU / float(count)
	var k := 1.0
	match _anim_phase:
		ANIM_SCATTER:
			k = _ease_out(_anim_time / RESOLVE_SCATTER_SEC)
		ANIM_REFORM:
			k = 1.0 - _ease_in_out(_anim_time / RESOLVE_REFORM_SEC)
	for i in count:
		var ch := text.substr(i, 1)
		var size := int(round(lerpf(float(rest_size), float(RESOLVE_DIGIT_FONT_SIZE), k)))
		var angle := _orbit_phase + float(i) * slot_span
		var slot := center + ring_radius * Vector2(cos(angle), sin(angle))
		# The slot is where the glyph's optical CENTRE goes; `draw_char` takes a baseline origin.
		var ring_origin := slot - Vector2(
			font.get_char_size(ch.unicode_at(0), size).x * 0.5,
			-font.get_ascent(size) * GLYPH_OPTICAL_CENTRE_ASCENT_FRACTION)
		_face_digits.draw_char(font, rest_origins[i].lerp(ring_origin, k), ch, size, _accent_color)

# ---- popover ---------------------------------------------------------------

func _open_popover() -> void:
	_close_popover()
	# Standard modal pattern: a full-screen dismiss layer (STOP) with the popover nested INSIDE
	# it (a CHILD, not a sibling). A child renders + picks ABOVE its parent, so the popover's own
	# buttons (STOP) consume their clicks and the popover PanelContainer consumes background
	# clicks; only clicks in the catcher area OUTSIDE the popover reach `_on_catcher_input` →
	# dismiss. The catcher is a `top_level` child of the orb — full-screen in the orb's own
	# CanvasLayer (so it sits at the HUD's z, not behind it) but escaping the orb's small rect.
	# (Previously catcher + popover were SIBLING top_level children: ambiguous ordering let the
	# catcher pick above the popover and swallow the Advance/Jump clicks — "Advance did nothing".)
	_catcher = Control.new()
	_catcher.top_level = true
	_catcher.mouse_filter = Control.MOUSE_FILTER_STOP
	_catcher.global_position = Vector2.ZERO
	_catcher.size = get_viewport_rect().size
	_catcher.gui_input.connect(_on_catcher_input)
	add_child(_catcher)

	_popover = _build_popover()
	_popover.resized.connect(_position_popover)
	_catcher.add_child(_popover)
	_popover_open = true
	_position_popover()

func _close_popover() -> void:
	# Freeing the catcher frees the nested popover too.
	if _catcher != null and is_instance_valid(_catcher):
		_catcher.queue_free()
	_catcher = null
	_popover = null
	_popover_open = false

func _on_catcher_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed:
		_close_popover()

func _position_popover() -> void:
	if _popover == null or _orb_area == null:
		return
	var orb_rect := _orb_area.get_global_rect()
	var pw := _popover.size.x
	var ph := _popover.size.y
	_popover.global_position = Vector2(orb_rect.end.x - pw, orb_rect.position.y - ph - POPOVER_GAP)

func _build_popover() -> PanelContainer:
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
	var body := VBoxContainer.new()
	body.custom_minimum_size = Vector2(POPOVER_WIDTH, 0)
	body.add_theme_constant_override("separation", 0)
	panel.add_child(body)

	if _entries.is_empty():
		body.add_child(_popover_header("Nothing pending", ""))
		body.add_child(_all_clear_block())
	else:
		var n := _entries.size()
		body.add_child(_popover_header("Needs your attention", "%d item%s" % [n, "" if n == 1 else "s"]))
		for entry in _entries:
			body.add_child(_reason_row(entry))
	body.add_child(_popover_footer())
	return panel

func _popover_header(title: String, count_text: String) -> Control:
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 8)
	var title_label := Label.new()
	title_label.text = title.to_upper()
	title_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title_label.add_theme_color_override("font_color", HudStyle.INK_DIM)
	header.add_child(title_label)
	if count_text != "":
		var count_label := Label.new()
		count_label.text = count_text
		count_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
		header.add_child(count_label)
	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", ROW_H_PADDING)
	margin.add_theme_constant_override("margin_right", ROW_H_PADDING)
	margin.add_theme_constant_override("margin_top", 4)
	margin.add_theme_constant_override("margin_bottom", 8)
	margin.add_child(header)
	return margin

func _all_clear_block() -> Control:
	var box := VBoxContainer.new()
	box.alignment = BoxContainer.ALIGNMENT_CENTER
	box.add_theme_constant_override("separation", 3)
	var glyph := Label.new()
	glyph.text = "✓"
	glyph.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	glyph.add_theme_font_size_override("font_size", 26)
	glyph.add_theme_color_override("font_color", HudStyle.HEALTHY)
	var title := Label.new()
	title.text = "All clear"
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	title.add_theme_color_override("font_color", HudStyle.INK)
	var sub := Label.new()
	sub.text = "Every band is working and no decision awaits. Advance the turn."
	sub.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	sub.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	sub.add_theme_color_override("font_color", HudStyle.INK_DIM)
	box.add_child(glyph)
	box.add_child(title)
	box.add_child(sub)
	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 20)
	margin.add_theme_constant_override("margin_right", 20)
	margin.add_theme_constant_override("margin_top", 18)
	margin.add_theme_constant_override("margin_bottom", 18)
	margin.add_child(box)
	return margin

func _reason_row(entry: Variant) -> Button:
	var severity := String(entry.get("severity", SEVERITY_INFO))
	var color := _severity_color(severity)
	var x := int(entry.get("x", -1))
	var y := int(entry.get("y", -1))
	var locates := x >= 0 and y >= 0

	var button := Button.new()
	button.focus_mode = Control.FOCUS_NONE
	button.custom_minimum_size = Vector2(0, ROW_MIN_HEIGHT)
	HudStyle.apply_button(button, "ghost")

	var row := HBoxContainer.new()
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.set_anchors_preset(Control.PRESET_FULL_RECT)
	row.offset_left = ROW_H_PADDING
	row.offset_right = -ROW_H_PADDING
	row.add_theme_constant_override("separation", ROW_SEPARATION)

	var stripe := ColorRect.new()
	stripe.custom_minimum_size = Vector2(SEV_STRIPE_WIDTH, 0)
	stripe.color = color
	stripe.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(stripe)

	var icon := Label.new()
	icon.text = _kind_icon(String(entry.get("kind", "")))
	icon.custom_minimum_size = Vector2(ROW_ICON_SIZE, ROW_ICON_SIZE)
	icon.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	icon.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	icon.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	icon.add_theme_color_override("font_color", color)
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(icon)

	var text_box := VBoxContainer.new()
	text_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	text_box.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	text_box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	text_box.add_theme_constant_override("separation", 1)
	# Both labels CLIP (see POPOVER_WIDTH): a Label reports its full text as its minimum width, and
	# this HBox is anchored to a Button (which never grows to it), so an unclipped long label pushed
	# the `Jump →` out past the card's edge. Clipping keeps every row inside the card.
	var label := Label.new()
	label.text = String(entry.get("label", ""))
	label.add_theme_color_override("font_color", HudStyle.INK)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.clip_text = true
	var detail := Label.new()
	detail.text = String(entry.get("detail", ""))
	detail.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	detail.add_theme_font_size_override("font_size", ROW_DETAIL_FONT_SIZE)
	detail.mouse_filter = Control.MOUSE_FILTER_IGNORE
	detail.clip_text = true
	text_box.add_child(label)
	text_box.add_child(detail)
	row.add_child(text_box)

	var jump := Label.new()
	jump.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	jump.mouse_filter = Control.MOUSE_FILTER_IGNORE
	if locates:
		jump.text = "Jump →"
		jump.add_theme_color_override("font_color", HudStyle.SIGNAL)
	elif HudAttentionVocab.ATTENTION_KINDS_WITH_A_PANEL.has(String(entry.get("kind", ""))):
		jump.text = "Open ▸"
		jump.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	# **A ROW THAT CAN NEITHER JUMP NOR OPEN WEARS NO AFFORDANCE.** `crew_handoff` is a NOTICE — the
	# sim's completion event carries no coordinates, and a turn may finish several builds, so there is
	# no one hex or one panel the row could honestly promise. It states where the hands went in words
	# and leaves the label blank rather than rendering an `Open ▸` that does nothing when pressed.
	row.add_child(jump)

	button.add_child(row)
	button.pressed.connect(_on_reason_pressed.bind(x, y, locates, String(entry.get("kind", ""))))
	return button

## Is any entry holding the turn? The orb does not know WHAT blocks — only that something does.
func has_blocking_entry() -> bool:
	for entry in _entries:
		if entry is Dictionary and bool(entry.get("blocking", false)):
			return true
	return false

## WHY the footer's advance cannot be pressed, or `""` while it is live. ONE channel for BOTH reasons:
## `disabled` and the ghost-vs-primary treatment key off this string being non-empty, so a third
## reason later is one more branch here and nothing else. Resolving wins over blocked — a turn already
## in flight is the more immediate truth.
func _advance_block_label() -> String:
	if _resolving:
		return ADVANCE_RESOLVING_LABEL
	if has_blocking_entry():
		return ADVANCE_BLOCKED_LABEL
	return ""

func _popover_footer() -> Control:
	var reason := _advance_block_label()
	var blocked := not reason.is_empty()
	var advance := Button.new()
	advance.text = reason if blocked else ADVANCE_LABEL
	advance.focus_mode = Control.FOCUS_NONE
	advance.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# `ghost` while blocked: the primary (cyan) treatment reads as "do this next", which is
	# exactly the wrong invitation on a button that cannot be pressed.
	HudStyle.apply_button(advance, "ghost" if blocked else "primary")
	advance.disabled = blocked
	advance.pressed.connect(_on_advance_pressed)
	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", ROW_H_PADDING)
	margin.add_theme_constant_override("margin_right", ROW_H_PADDING)
	margin.add_theme_constant_override("margin_top", 8)
	margin.add_theme_constant_override("margin_bottom", 8)
	margin.add_child(advance)
	return margin

func _on_reason_pressed(x: int, y: int, locates: bool, kind: String) -> void:
	if locates:
		emit_signal("focus_requested", x, y)
	else:
		# The thing this row names lives in a panel, not on the map. The orb only says WHICH
		# kind was activated; the Hud owns the kind → panel mapping.
		emit_signal("panel_requested", kind)
	_close_popover()

func _on_advance_pressed() -> void:
	emit_signal("advance_requested")
	# Close BEFORE raising the gate: `_begin_resolving` rebuilds an open popover, and rebuilding this
	# one only to free it a line later is pure waste.
	_close_popover()
	_begin_resolving()

func _rebuild_popover() -> void:
	if not _popover_open or _catcher == null:
		return
	if _popover != null:
		_popover.queue_free()
	_popover = _build_popover()
	_popover.resized.connect(_position_popover)
	_catcher.add_child(_popover)
	_position_popover()
