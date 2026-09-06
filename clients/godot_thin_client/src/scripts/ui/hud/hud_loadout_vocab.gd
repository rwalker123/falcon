class_name HudLoadoutVocab

## The OPENING LOADOUT picker's vocabulary leaf (issue #629) — the wire keys it reads, the words it
## says, and the geometry it is laid out on. A DECLARATION BLOCK exactly like `HudCraftingVocab` and
## its siblings: a new label, format or measurement goes HERE rather than as a fresh `const` on the
## panel, which is the rule that keeps a panel's const block from regrowing into a merge-conflict
## surface. Its one function is `apply_palette` — the tints below are `static var` because
## `HudStyle`'s palette is themed, and they are ASSIGNED there rather than initialized (the
## derive-inside-apply rule in `HudPalette`).
##
## **THIS PANEL HAS ONE COLOUR VOCABULARY AND IT MEANS "MATERIAL".** A swatch is a material's
## identity, drawn identically in the resources column, in the legend above the recipe list and on
## every recipe's cost row, so the three read as one key. Nothing else in the panel is tinted by
## identity — the kit meter's bar is a single spent segment against its remainder, kits being
## interchangeable hands with nothing to tell apart — because a second colour vocabulary beside the
## legend would be read as part of it.
##
## DEPENDENCY DIRECTION: a vocab leaf reads nothing but `HudStyle`, which is a leaf too, so the pair
## stays acyclic.

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

# ---- the wire's own keys ------------------------------------------------------------------------
# `CampaignSection.openingLoadout`, decoded onto the snapshot dict as `opening_loadout`
# (`native/src/dict/campaign.rs`).

## The section as the decoder publishes it.
const SNAPSHOT_KEY := "opening_loadout"
## False once the window has shut. The picker draws nothing and the sim refuses the command.
const OPEN_KEY := "open"
## One kit per working-age hand of the starting band — derived sim-side, never configured.
const KIT_BUDGET_KEY := "kit_budget"
## `start_profiles.json` `opening_loadout.material_points`. One point buys one unit.
const MATERIAL_BUDGET_KEY := "material_budget"
## The pick list, in the order the profile declares it — **and that order is also the draw order**,
## shared by the resources column and the legend, so the two cannot disagree.
const PICKABLE_MATERIALS_KEY := "pickable_materials"
## The allocation the window OPENS on: a suggestion, never a grant.
const MATERIAL_DEFAULTS_KEY := "material_defaults"
const MATERIAL_DEFAULT_ID_KEY := "material_id"
const MATERIAL_DEFAULT_UNITS_KEY := "units"
## **THE RECIPES THIS FACTION COULD PUT ON A BENCH TODAY, AS IDS.** It is what keeps the
## knowledge-gated bench tools off the "what this builds" list. The client must never work that out
## by sniffing a craft offer's refusal SENTENCE — that would make a player-facing string into a
## machine contract.
const CRAFTABLE_RECIPE_IDS_KEY := "craftable_recipe_ids"

# The KIT ROSTER rides `SubsistenceSection.equipmentConfigJson` and the RECIPE COSTS ride
# `SubsistenceSection.recipes`; neither is copied into the loadout section, so the picker joins onto
# what is already published.

## The `kits` array inside the parsed `equipment_config_json`.
const CONFIG_KITS_KEY := "kits"
const KIT_ID_KEY := "id"
const KIT_DISPLAY_NAME_KEY := "display_name"
const KIT_JOBS_KEY := "jobs"
## The equipment ids a kit grants, one unit of each per kit. **A kit whose `uses` is EMPTY is the
## `none` kit** — the roster's "carry nothing" entry, which is what the picker excludes rather than
## matching on its id. A roster that renamed it would still be excluded.
const KIT_USES_KEY := "uses"

## The recipe book's rows (`snapshot["recipes"]`).
const RECIPE_ID_KEY := "id"
const RECIPE_DISPLAY_NAME_KEY := "display_name"
const RECIPE_WORK_KEY := "work"
const RECIPE_INPUTS_KEY := "inputs"
const RECIPE_INPUT_MATERIAL_ID_KEY := "material_id"
const RECIPE_INPUT_AMOUNT_KEY := "amount"

# ---- the words ----------------------------------------------------------------------------------

const PANEL_TITLE := "Outfit the band"
## **THE FORFEIT IS SAID BEFORE THE FIRST CLICK, not only on the commit button.** The window shuts
## when the turn advances whether or not anything was spent, and a player who dismissed the picker to
## look at the map has to already know that.
const PANEL_SUBTITLE := "What your people carry when they set out. Nothing is theirs until you commit, and anything unspent is lost when the turn advances."

const KITS_HEAD := "Kits"
## Says what the budget IS rather than restating its number — the meter beside it already has the
## count, and the model fact (a kit is a pair of hands) is the thing a player cannot derive.
const KITS_NOTE := "One for every pair of working hands."
const RESOURCES_HEAD := "Resources"
const RESOURCES_NOTE := "One point buys one unit. Everything arrives middling."
const BUILDS_HEAD := "What this builds"
## The column is a READOUT, and this is the sentence that stops it being read as a build plan.
const BUILDS_NOTE := "If you spent the whole pile on one thing."

## The budget meters say the REMAINDER and nothing else. A second clause ("28 of 30 packed") is the
## same fact subtracted from itself, and the bar beside it already draws the spent half.
const BUDGET_REMAINING_FORMAT := "%d / %d left"

const KIT_JOBS_SEPARATOR := " · "
const KIT_USES_SEPARATOR := ", "
## `Hunt · Builders` over `Spears, Sled` — the jobs a kit may be sent on, then what it puts in hands.
const KIT_JOBS_FORMAT := "%s"
const KIT_USES_FORMAT := "%s"

## The reachable count on a recipe row. `×3` reads as a MULTIPLICITY, which is what it is.
const RECIPE_COUNT_FORMAT := "×%d"
## …and a recipe the pile cannot reach at all says so with the same dash every empty cell in this HUD
## uses, rather than `×0`, which invites the reader to look for the zero's cause on the row.
const RECIPE_COUNT_NONE := "—"
## One input's cost beside its swatch. Amounts are small fractions of a unit as often as whole ones,
## so the format keeps one decimal and trims a trailing `.0` (see `amount_text`).
const RECIPE_INPUT_AMOUNT_FORMAT := "%s"
const RECIPE_WORK_FORMAT := "%s work"
const RECIPE_COST_SEPARATOR := "   "

## The legend above the recipe list: the key for every swatch drawn on a row, in the resources
## column's own order.
const LEGEND_HEAD := "Materials"

const COMMIT_CLEAR_LABEL := "Set out"
## **THE CONSEQUENCE, NOT A WARNING.** The button never refuses; it states what walking away costs.
const COMMIT_FORFEIT_FORMAT := "Set out — forfeit %s"
const COMMIT_FORFEIT_SEPARATOR := " and "
const COMMIT_FORFEIT_KITS_ONE := "1 kit"
const COMMIT_FORFEIT_KITS_MANY := "%d kits"
const COMMIT_FORFEIT_UNITS_ONE := "1 unit"
const COMMIT_FORFEIT_UNITS_MANY := "%d units"
const COMMIT_TOOLTIP := "Send the order. The window shuts and whatever is left of either budget is gone."

## The window is dismissible so the player can pan, zoom and read tiles before committing — the sim
## keeps it open until the turn advances, and this control is how it comes back.
const DISMISS_GLYPH := "✕"
const DISMISS_TOOLTIP := "Look around first. The picker stays available until the turn advances."
const REOPEN_LABEL := "⚑  Outfit the band"
const REOPEN_TOOLTIP := "Finish outfitting the band before the turn advances."

## What the panel says while it is open on a world that published no pick list and no kit roster —
## a frame between the section arriving and the catalogues arriving, not an error.
const EMPTY_NOTICE := "Waiting for the world's kit roster."

# ---- the turn orb's row -------------------------------------------------------------------------
# The orb is the generic attention hub, so an unfinished loadout is one more producer on it. The
# row is NON-LOCATING (an opening loadout is a faction fact and no hex holds it) and it does NOT
# block `Advance ▸`: closing the window is the SIM's business and the client's job is to warn.

const ATTENTION_LABEL := "Band not outfitted"
## Both remainders in one line, because the two budgets are one decision. A budget already clear is
## dropped from it rather than printed as a zero.
const ATTENTION_DETAIL_SEPARATOR := ", "
const ATTENTION_DETAIL_KITS_ONE := "1 kit unspent"
const ATTENTION_DETAIL_KITS_MANY := "%d kits unspent"
const ATTENTION_DETAIL_UNITS_ONE := "1 unit unspent"
const ATTENTION_DETAIL_UNITS_MANY := "%d units unspent"
## Both budgets are clear and the order simply has not been sent yet.
const ATTENTION_DETAIL_READY := "ready to set out"

# ---- geometry (measured, not guessed — every number here is read back off a rendered frame) ------

const PANEL_WIDTH := 900.0
const PANEL_MIN_HEIGHT := 340.0
const VIEWPORT_MARGIN := 24.0

const HEADER_PADDING_H := 16
const HEADER_PADDING_V := 12
const HEADER_SEPARATION := 12
const TITLE_FONT_SIZE := 16
const SUBTITLE_FONT_SIZE := 11

const COLUMNS_PADDING_H := 16
const COLUMNS_PADDING_V := 12
const COLUMN_SEPARATION := 18
const COLUMN_MIN_WIDTH := 258.0
const COLUMN_HEAD_FONT_SIZE := 12
const COLUMN_NOTE_FONT_SIZE := 10
const COLUMN_BLOCK_SEPARATION := 8
const COLUMN_ROW_SEPARATION := 6

## The horizontal rules above and below the body.
const RULE_THICKNESS := 1.0
## **THE VERTICAL SEAM IS TWO PIXELS AND THE HORIZONTAL RULE IS ONE, and that asymmetry is
## MEASURED.** The HUD renders at a fractional canvas scale (`window/stretch` is
## `canvas_items`/`expand`), so a 1px vertical line lands on a sub-pixel boundary whose coverage
## depends on where its column happens to fall — and with three equal-stretch columns the two seams
## fall differently: the first rendered and the second vanished entirely. A rule that draws on one
## side of a card and not the other is worse than no rule, so the seam is given a thickness that
## survives any offset. The horizontal rules span the card and are unaffected.
const COLUMN_SEAM_THICKNESS := 2.0

const BUDGET_FONT_SIZE := 12
const BUDGET_BAR_HEIGHT := 8.0
const BUDGET_ROW_SEPARATION := 4

const ROW_FONT_SIZE := 12
const ROW_NOTE_FONT_SIZE := 10
const ROW_SEPARATION := 8
## The stepper's `−`/`+` faces are square-ish; the value cell holds two digits with room to spare.
const STEPPER_BUTTON_WIDTH := 26.0
const STEPPER_VALUE_WIDTH := 26.0
const STEPPER_PADDING_H := 4
const STEPPER_FONT_SIZE := 12
const STEPPER_PADDING_V := 2

const SWATCH_SIZE := Vector2(10.0, 10.0)
const SWATCH_SEPARATION := 6
const LEGEND_SEPARATION := 10
const LEGEND_FONT_SIZE := 10

const FOOTER_PADDING_H := 16
const FOOTER_PADDING_V := 12
const FOOTER_SEPARATION := 12
const COMMIT_FONT_SIZE := 13

const REOPEN_PADDING := 10
const REOPEN_FONT_SIZE := 12

## What a dimmed (unreachable) recipe row is drawn at. A filter that HID them would take away the
## thing the column exists to teach — that the pile is one material short of a sled.
const UNREACHABLE_ALPHA := 0.45

# ---- meta handles (a harness asks a CONTROL, never a subtree's text) -----------------------------

const KIT_ROW_META := &"loadout_kit_row"
const MATERIAL_ROW_META := &"loadout_material_row"
const RECIPE_ROW_META := &"loadout_recipe_row"
const RECIPE_COUNT_META := &"loadout_recipe_count"
const BUDGET_METER_META := &"loadout_budget_meter"
const COMMIT_BUTTON_META := &"loadout_commit"
const LEGEND_ENTRY_META := &"loadout_legend_entry"

## The two meters, by the budget each reports.
const BUDGET_KITS := "kits"
const BUDGET_MATERIALS := "materials"

# ---- the palette --------------------------------------------------------------------------------

## **THE SWATCH RING — the panel's material colour vocabulary, indexed by a material's position in
## the published pick list.** It is a RING rather than a table keyed by material id: the pick list is
## `start_profiles.json` data, so a profile offering a material this build has never heard of must
## still get a swatch, and a table would silently hand it the fallback ink shared with everything
## else. The ordering makes the shipped five (bone, fibre, hide, wood, stone) land on five inks that
## are separable at `SWATCH_SIZE`, which is the only property a key colour has to have.
##
## Every entry is a themed `HudStyle` ink, so the ring re-derives with the palette.
static var SWATCH_COLORS: Array[Color] = []
## The unspent half of a budget bar, and the ink a swatch falls back to before the palette lands.
static var BUDGET_REMAINDER_COLOR: Color = Color()
## The spent half of the KIT bar — one segment, not a stack. See this file's docstring.
static var BUDGET_SPENT_COLOR: Color = Color()

## Install the current `HudStyle` palette into this file's tints. Called by `HudPalette.apply()`
## after `HudStyle.apply_palette`; it takes no palette of its own, because none of these is a colour
## in its own right — each is one HUD ink re-stated in this panel's vocabulary.
static func apply_palette() -> void:
	SWATCH_COLORS = [
		HudStyle.VOICE_PIGMENT,
		HudStyle.HEALTHY,
		HudStyle.DANGER,
		HudStyle.WARN,
		HudStyle.VOICE_INK,
		HudStyle.SIGNAL,
		HudStyle.SIGNAL_DEEP,
	]
	# **`LINE`, NOT `LINE_SOFT`.** The unspent half of a budget bar is a READING — how much is still
	# there — so it has to be separable from the card behind it; the soft hairline ink measured within
	# a few values of `PANEL_SOLID` and rendered as no bar at all on a budget nothing had been spent
	# from, which is exactly the state the meter is most needed in.
	BUDGET_REMAINDER_COLOR = HudStyle.LINE
	BUDGET_SPENT_COLOR = HudStyle.SIGNAL_DEEP

## The swatch for the material at `index` in the published pick list. Wraps, so a profile offering
## more materials than the ring has inks repeats a colour rather than losing one — a repeat is
## legible beside the legend, an unpainted swatch is not.
static func swatch_color(index: int) -> Color:
	if SWATCH_COLORS.is_empty():
		return BUDGET_REMAINDER_COLOR
	return SWATCH_COLORS[posmod(index, SWATCH_COLORS.size())]

## A material id as a person reads it. The wire carries no display name for a material — every
## surface in this client capitalizes the id, and this is that one idiom, named.
static func material_label(material_id: String) -> String:
	return material_id.capitalize()

## An input amount, trimmed. Recipe inputs are floats and most of them are whole, so `2` beats `2.0`
## while `0.5` must survive.
static func amount_text(amount: float) -> String:
	if is_equal_approx(amount, roundf(amount)):
		return "%d" % int(roundf(amount))
	return "%.1f" % amount
