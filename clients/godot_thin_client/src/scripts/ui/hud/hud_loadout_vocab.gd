class_name HudLoadoutVocab

## The OUTFITTING picker's vocabulary leaf (issue #629) — the wire keys it reads, the words it
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

# ---- the wire's own keys: THE CAMPAIGN'S HALF ----------------------------------------------------
# `CampaignSection.openingLoadout`, decoded onto the snapshot dict as `opening_loadout`
# (`native/src/dict/campaign.rs`). It is one per WORLD: the pick list and the two pre-fills, and
# nothing that is true of one band.

## The pick list, in the order the profile declares it — **and that order is also the draw order**,
## shared by the resources column and the legend, so the two cannot disagree.
const PICKABLE_MATERIALS_KEY := "pickable_materials"
## The allocation the window OPENS on: a suggestion, never a grant.
const MATERIAL_DEFAULTS_KEY := "material_defaults"
const MATERIAL_DEFAULT_ID_KEY := "material_id"
const MATERIAL_DEFAULT_UNITS_KEY := "units"
## The KIT column's pre-fill, the material one's twin. **ALREADY CLAMPED to `kit_budget` sim-side** —
## that budget is the spawned band's working-age head count rather than a config number, so the
## profile cannot sum-check its own pre-fill and the sim scales it proportionally at publish time.
## **Draw these counts as-is**: re-fitting them against the budget here would be a second clamp, and
## two clamps disagree.
const KIT_DEFAULTS_KEY := "kit_defaults"
const KIT_DEFAULT_ID_KEY := "kit_id"
const KIT_DEFAULT_COUNT_KEY := "count"
## **THE RECIPES THIS FACTION COULD PUT ON A BENCH TODAY, AS IDS.** It is what keeps the
## knowledge-gated bench tools off the "what this builds" list. The client must never work that out
## by sniffing a craft offer's refusal SENTENCE — that would make a player-facing string into a
## machine contract.
const CRAFTABLE_RECIPE_IDS_KEY := "craftable_recipe_ids"

# ---- the wire's own keys: ONE BAND'S WINDOW ------------------------------------------------------
# `PopulationCohortState.loadoutWindow`, decoded onto each cohort dict as `loadout_window`
# (`native/src/dict/population.rs`). **EVERY BAND GETS ONE** — the spawned band's, and one on every
# splinter a split makes — so the budgets and the open flag are facts about a BAND and were deleted
# from the campaign section above rather than deprecated in it.

## The window, on the cohort dict. Absent means this band has nothing to outfit.
const WINDOW_KEY := "loadout_window"
## The band's DURABLE id, on the cohort dict — the handle `set_starting_loadout` names, never
## `entity`, which is ECS allocation state a rollback renumbers.
const BAND_ID_KEY := "band_id"
## False once THIS band's window has shut. It shuts on the turn advance and on nothing else, so it is
## never a success signal: a committed order leaves it open, which is what lets a pick be revised.
const OPEN_KEY := "open"
## One kit per working-age hand of the band — derived sim-side, never configured. `0` on a TAKE
## window, which mints nothing.
const KIT_BUDGET_KEY := "kit_budget"
## `start_profiles.json` `opening_loadout.material_points`. One point buys one unit; `0` on a take.
const MATERIAL_BUDGET_KEY := "material_budget"
## ⛔ **WHICH OF THE TWO WINDOWS THIS IS, and it decides everything the card draws.**
## `GRANT_PARENT_BAND_ID` means the picks MINT against the two budgets above. Anything else is the
## id of the band this take is drawn FROM: the picks MOVE gear out of that band's ledger, the budgets
## are `0` and mean nothing, and the cap is the two supplies below.
const PARENT_BAND_ID_KEY := "parent_band_id"
## The value of [PARENT_BAND_ID_KEY] on a grant window. `0` is "no band" throughout this client.
const GRANT_PARENT_BAND_ID := 0
## **THE ACCEPTED ALLOCATION** — the rows this band's last accepted order named, in the pre-fills' own
## row shape, and **what the card opens on**. Empty only for a grant window nobody has ordered against
## yet, which is what makes the campaign pre-fill that window's seed.
##
## ⛔ **A FRESH SPLINTER'S ARE NOT EMPTY.** The split's default take is kit-denominated and published
## here, so the take card opens on the allocation the band is already standing on and an untouched
## `Set out` re-sends it unchanged — an exact no-op. While these were empty on a splinter (the take
## being a bare per-item manifest then), that same press ordered *take nothing* and handed the whole
## dowry back to the parent, an apply being a REPLACEMENT.
const WINDOW_KITS_KEY := "kits"
const WINDOW_MATERIALS_KEY := "materials"
## ⛔ **A TAKE'S CAP, PER ITEM — AND A KIT ROW CANNOT BE CAPPED ON ITS OWN AGAINST IT.** `sled` is
## used by both `big_game` and `trapping`, so five of each needs TEN sleds: the order has to be
## EXPANDED to items and the whole expansion checked against this, which is what the sim refuses on.
## Each row is already `the parent's holdings + this take's standing units`, so a revision from 3 to
## 5 is not refused for the 3 that already moved. Empty on a grant window.
##
## **It lists only items some kit carries.** `bone_awl`, `loom` and `tanning_frame` are the three no
## kit `uses` — the knowledge-gated bench tools — and shop equipment stays with the workshop that
## built it rather than walking out with a splinter, so a client is never told they are claimable and
## never has to re-derive that filter.
const PARENT_ITEM_SUPPLY_KEY := "parent_item_supply"
## The material twin, in whole units — the currency the command spends.
const PARENT_MATERIAL_SUPPLY_KEY := "parent_material_supply"
const SUPPLY_ID_KEY := "id"
const SUPPLY_UNITS_KEY := "units"

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

## The title of a card whose band has no name (only reachable from a hand-built fixture — the sim
## always names a band).
const PANEL_TITLE := "Outfit the band"
## **THE CARD NAMES ITS BAND, because there can be more than one window open at once.** A split opens
## a window on the splinter while the parent's may still be open, and a title that said only
## *"the band"* would leave the player composing an order for a band they cannot identify.
const PANEL_TITLE_FORMAT := "Outfit %s"
## **THE CARD MAKES NO FORFEITURE CLAIM, and this line is where one used to be.** It read
## *"…and anything unspent is lost when the turn advances"*; the unspent warning lives on the TURN
## ORB, which is the surface that already counts down to the thing that causes it, and a second copy
## on the card was both a duplicate and the longest sentence on the screen.
const PANEL_SUBTITLE := "What your people carry when they set out."
## **THE TAKE'S SUBTITLE, and the one fact that separates it from a grant**: this gear is not minted,
## it walks out of the home band's own ledger. Ray's register — one short declarative, one fact — and
## it names the band so the sentence answers *whose* without a second clause.
const PANEL_SUBTITLE_TAKE_FORMAT := "What they take from %s."

const KITS_HEAD := "Kits"
## **THE NOTES SAY WHAT THE COLUMN IS FOR AND NOTHING ELSE.** Each of these three replaced a sentence
## explaining the model behind the column — what a kit budget is derived from, what a point buys, how
## the third column's number is computed. They read as an essay beside three lists a player is trying
## to use. **Do not restore an explanatory clause to any of them**; the meter, the swatch and the `×N`
## are the explanation.
const KITS_NOTE := "Select kits your band will start with"
const RESOURCES_HEAD := "Resources"
const RESOURCES_NOTE := "Select crafting resources the band will start with"
## It named no subject — *"what THIS builds"*, beside two other columns.
const BUILDS_HEAD := "What the resources can build"
## **THE BUILDS COLUMN HAS NO NOTE, deliberately, and `_column` DRAWS NO LABEL for an empty one** —
## an empty caption node is a blank row of layout, which is the ragged third column this deletion was
## meant to remove.
const BUILDS_NOTE := ""

## The budget meters say the REMAINDER and nothing else. A second clause ("28 of 30 packed") is the
## same fact subtracted from itself, and the bar beside it already draws the spent half.
const BUDGET_REMAINING_FORMAT := "%d / %d left"
## ⛔ **A TAKE HAS NO BUDGET, SO ITS METER READS THE HOME BAND'S SUPPLY** — and what is left of it is
## not forfeited when the turn advances, it simply stays where it is. Hence *"left at home"* rather
## than the grant's bare *"left"*: the grant's remainder is lost on the advance and the orb says so,
## and reusing that wording here would state a loss that does not happen.
const SUPPLY_REMAINING_FORMAT := "%d / %d left at home"

## `Hunt · Builders` over `Spears, Sled` — the jobs a kit may be sent on, then what it puts in hands.
const KIT_JOBS_SEPARATOR := " · "
const KIT_USES_SEPARATOR := ", "

## The reachable count on a recipe row. `×3` reads as a MULTIPLICITY, which is what it is.
const RECIPE_COUNT_FORMAT := "×%d"
## …and a recipe the pile cannot reach at all says so with the same dash every empty cell in this HUD
## uses, rather than `×0`, which invites the reader to look for the zero's cause on the row.
const RECIPE_COUNT_NONE := "—"
## One input's cost beside its swatch. Amounts are small fractions of a unit as often as whole ones,
## so the format keeps one decimal and trims a trailing `.0` (see `amount_text`).
const RECIPE_INPUT_AMOUNT_FORMAT := "%s"
const RECIPE_WORK_FORMAT := "%s work"

## The legend above the recipe list: the key for every swatch drawn on a row, in the resources
## column's own order.
const LEGEND_HEAD := "Materials"

## ⛔ **THE COMMIT CONTROL IS UNCONDITIONAL, AND A FORFEIT VARIANT OF IT WOULD BE A LIE.**
## **Committing does not shut the window.** The sim treats an apply as a REPLACEMENT rather than an
## addition, so the order may be sent, revised and sent again as often as the player likes; only the
## TURN ADVANCE closes the window and forfeits what is left. A label reading *"Set out — forfeit 17
## kits and 2 units"* therefore described a consequence that pressing it does not have — it named the
## cost of ending the turn on a button that does not end the turn.
##
## The remainder is still worth saying and is said ONCE, on the **turn orb**, which is the surface
## that already counts down to the advance. Nothing on this card may state it a second time.
## **THE FOOTER'S LEADING TEXT — two short declaratives, one fact each.** The card is dismissible and
## the orb's row is the way back to it; the TURN is what ends that, and a player who has put the card
## away has no other way to learn either.
##
## ⛔ **IT IS NOT THE RETIRED FORFEITURE CLAIM.** That one said COMMITTING shuts the window, which is
## false — an apply is a replacement and the order may be revised as often as the player likes. This
## names the TURN, which is true: `close_opening_window` is the only writer that ever clears `open`.
## Do not let the two drift back together.
##
## The quiet ink, the subtitle's register: it is guidance rather than a warning, and this card carries
## no warning ink anywhere else. **If it ever has to be trimmed, the SECOND sentence is the one that
## survives** — the way back is discoverable by pressing things; the deadline is not.
const FOOTER_NOTE := "The turn orb reopens this. Ending the turn closes it for good."

const COMMIT_CLEAR_LABEL := "Set out"
const COMMIT_TOOLTIP := "You can change this until you end the turn."

## **THE BAND SWITCHER, drawn ONLY while more than one window is open.** One button per band with an
## open window; the subject's is pressed. It earns no row in the ordinary single-window case, and it is
## ONE of the two ways to a second band's card — that band's own orb row is the other, carrying its
## subject (`HudAttentionVocab.ATTENTION_PANEL_SUBJECT`). This was the only one while a row carried a
## KIND and no band.
const BAND_TAB_TOOLTIP_FORMAT := "Outfit %s."
const BAND_TAB_SEPARATION := 6
const BAND_TAB_FONT_SIZE := 11

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

## ⛔ **THE ROW IS PRESENT FOR THE WHOLE WINDOW, spent or not, and its WORDING is what changes.** It
## is the only guaranteed way back to a dismissed card, so a row that vanished once both budgets were
## clear would strand a player who had finished picking, put the card away and then wanted to revise.
## What moves is the label and the severity: a finished loadout reads as DONE and paints the orb
## `READY`, an unfinished one reads as a warning and paints it `WARN`.
const ATTENTION_LABEL_UNSPENT := "Band not outfitted"
const ATTENTION_LABEL_READY := "Band outfitted"
## ⛔ **A THIRD RUNG, BECAUSE OVER-BUDGET AND FULLY-SPENT ARE NOT THE SAME ANSWER.** The completeness
## test was `remaining <= 0`, so a band holding MORE than its budget allows passed it and the orb
## called it done — reported from a live run as a card reading `-6 / 22 left` beside a row saying
## *everything is picked*. A state this row cannot word is exactly the state it must not paint green,
## so a negative remainder reads `warn` and says which way it is wrong.
const ATTENTION_LABEL_OVER := "Band over budget"
## Both remainders in one line, because the two budgets are one decision. A budget already clear is
## dropped from it rather than printed as a zero.
const ATTENTION_DETAIL_SEPARATOR := ", "
const ATTENTION_DETAIL_KITS_ONE := "1 kit unspent"
const ATTENTION_DETAIL_KITS_MANY := "%d kits unspent"
## **`resources`, NEVER `units`** — the picker's own second column is headed `RESOURCES`, and the orb
## naming the same budget something else made a player ask what a "unit" was. A budget is called
## whatever the control that spends it is called.
const ATTENTION_DETAIL_UNITS_ONE := "1 resource unspent"
const ATTENTION_DETAIL_UNITS_MANY := "%d resources unspent"
## …and the over-budget arm's own tail, built from the bare counts below so the two nouns are typed
## once. `2 kits, 6 resources over budget`.
const ATTENTION_DETAIL_OVER_FORMAT := "%s over budget"
## Both budgets are clear. It reads as a statement of fact rather than as an instruction, because at
## this point there is nothing the player still has to do.
const ATTENTION_DETAIL_READY := "everything is picked"

## ⛔ **THE ROW NAMES ITS OWN BAND, and the band leads.** Every window is one band's, so with two open
## the rows were identical and unattributable — *"Band outfitted / everything is picked"*, twice. The
## band rides the DETAIL, which is where the idle-worker rows directly above these put theirs, rather
## than in a label this arc would have had to invent a fourth spelling of.
const ATTENTION_DETAIL_BAND_FORMAT := "%s — %s"

## **THE BARE COUNT PHRASES, typed once and used by two arms** — a take's *what is taken* and a
## grant's *what is over budget*. They carry the noun and nothing else, so the arm supplies the verb.
const ATTENTION_COUNT_KITS_ONE := "1 kit"
const ATTENTION_COUNT_KITS_MANY := "%d kits"
const ATTENTION_COUNT_RESOURCES_ONE := "1 resource"
const ATTENTION_COUNT_RESOURCES_MANY := "%d resources"
## ⛔ **A TAKE FORFEITS NOTHING, SO ITS ROW NEVER SAYS `unspent`.** What a grant leaves unspent is
## gone on the turn advance, which is the whole reason that word is on the orb; supply a take leaves
## behind just stays with the home band. So the take's arms report what IS taken, and this is the arm
## with no order standing yet.
##
## **It no longer names the home band.** It did, because two open windows put two identically-worded
## rows on one popover — and the row names its OWN band now, which is both the fix for that and the
## convention every other producer already follows.
const ATTENTION_DETAIL_TAKE_NONE := "nothing taken yet"

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

const BAND_TAB_META := &"loadout_band_tab"
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
