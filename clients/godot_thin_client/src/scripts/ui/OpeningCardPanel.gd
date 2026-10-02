extends ProseOverlay
class_name OpeningCardPanel

## The Telling's OPENING CARD — the world's first lines, said on a card the player cannot miss, which
## then hands the player to the opening band's outfitting card.
##
## The sim says the opening as ordinary `narrative_beat` command events on tick 0 (the cold open, then
## the guidance line that ends by telling the player to choose what to carry). They reach the
## right-dock `TellingPanel` like every beat; this card is the SECOND reader of them, shown once per
## world by `OpeningCardController` in place of the outfitting card's own first auto-open.
##
## The look is `ProseOverlay`'s — the fork's catcher, scrim and auto-sized card — with the beats as
## prose paragraphs in arrival order and ONE primary button. No gloss and no voice toggle: a feed beat
## carries one rendered string.
##
## ⛔ **EVERY WAY OFF THIS CARD HANDS OFF.** The button, a click outside the card and ESC
## (`Main.escape_claimant` → `HudLayer.dismiss_opening_card`) all emit `handed_off`, and the controller
## opens the outfitting card on it. A dismissal that merely closed would drop the outfitting window
## out of the opening without the player ever having seen it.

## The card was put away — by its button, a catcher click or ESC. All three mean "on to outfitting".
signal handed_off

## The nested card's node name (`ProseOverlay._card_name`).
const OPENING_CARD_NAME := "OpeningCard"
## The hand-off button's height — the fork's choice row, so the two cards' buttons read as one family.
const HANDOFF_MIN_HEIGHT := NarrativeForkPanel.CHOICE_MIN_HEIGHT
const HANDOFF_FONT_SIZE := NarrativeForkPanel.CHOICE_FONT_SIZE

var _lines: Array[String] = []
var _handoff_button: Button = null

# ---- public API ------------------------------------------------------------

## Show the opening lines, one paragraph each, in the order given.
func show_lines(lines: Array[String]) -> void:
	_lines = lines.duplicate()
	_show_and_render()
	# The HUD parents several free-floating surfaces into one CanvasLayer, and the outfitting card may
	# have been added AFTER this one — so the card is raised on every show rather than trusting the
	# order the nodes were built in.
	move_to_front()

## The paragraphs on the card, for the harnesses.
func lines() -> Array[String]:
	return _lines.duplicate()

## The hand-off button, for the harnesses.
func handoff_button() -> Button:
	return _handoff_button

## Put the card away AND hand off — what ESC reaches.
func dismiss() -> void:
	_on_dismiss()

# ---- ProseOverlay hooks ----------------------------------------------------

func _card_name() -> String:
	return OPENING_CARD_NAME

func _build_body(body: VBoxContainer) -> void:
	for line in _lines:
		body.add_child(_build_narration(line))
	_handoff_button = Button.new()
	_handoff_button.text = HudLoadoutVocab.OPENING_HANDOFF_LABEL
	_handoff_button.focus_mode = Control.FOCUS_NONE
	_handoff_button.custom_minimum_size = Vector2(0, HANDOFF_MIN_HEIGHT)
	_handoff_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_handoff_button.add_theme_font_size_override("font_size", HANDOFF_FONT_SIZE)
	HudStyle.apply_button(_handoff_button, "primary")
	_handoff_button.pressed.connect(_on_dismiss)
	body.add_child(_handoff_button)

func _on_dismiss() -> void:
	if not visible:
		return
	close()
	handed_off.emit()
