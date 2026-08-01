class_name HudFloraVocab

## Flora-roster + intensification-ladder vocabulary — the crop picker, cultivation/field meters, the
## Sow site refusals, the gate-reason strings and the knowledge tracks. Move LAST: it reads
## `HudWorkVocab.WORK_ROW_FONT_SIZE` (a DOWNWARD alias) and `HudConst.LABOR_POLICY_*`.

# GATES on the investment rungs. The option stays VISIBLE but disabled with its reasons, so the player
# learns the prerequisite BEFORE acting rather than never discovering the rung exists. Both gates
# mirror the sim's `assign_labor` validation (faction knowledge complete + the source ready).
#
# Each reason states WHAT'S MISSING + HOW FAR ALONG IT IS + THE ACTION THAT CLOSES IT — naming the
# prerequisite alone ("Herd must be domesticated") tells the player a door is locked without saying
# where the key is.
#
# THIS IS WHERE THE TWO-METER SPLIT IS TAUGHT (docs/plan_intensification_ladder.md §4.1). A gated
# verb has at most two kinds of reason, and they are DIFFERENT KINDS OF THING:
#   • a KNOWLEDGE reason — "your PEOPLE haven't learned this craft yet". Faction-wide, permanent,
#     earned by cumulative practice on the rung BELOW. Its meter lives in the top-bar knowledge
#     strip, never in this source's drawer, and the remedy names the PRACTICE that fills it.
#   • a SOURCE reason — "you haven't done it to THIS herd/patch yet". Local, decays if abandoned.
#     Its meter is the source's own drawer row, and the remedy names the VERB that fills it.
# One line teaches the whole ladder: practise this rung → fill that knowledge meter → unlock that
# verb. The remedies therefore name a glyph pulled from the shared `FoodIcons.POLICY_ICONS` map, so
# each is literally the icon on a button beside it.
#
# The KNOWLEDGE reasons. Practice teaches the NEXT rung up (§4), and the rule keys off the rung the
# source STANDS on, not the verb — so the same Sustain hunt teaches Herding on a wild herd and
# Penning on a tamed one. **The remedies name the FLOOR, not a stance** — `intensification::
# learn_multiplier` scales practice by `floor / the food peak`, so leaving more standing is literally
# how you learn faster, and there is no rung to name. Format args: %d = the live faction progress
# percent, %s = the food-peak glyph.
const GATE_REASON_CULTIVATION_KNOWLEDGE_FORMAT := "Your people know Cultivation %d%% — %s forage a wild patch to learn it, faster the more you leave standing"

const GATE_REASON_HERDING_KNOWLEDGE_FORMAT := "Your people know Herding %d%% — %s hunt a wild herd to learn it, faster the more you leave standing"

# The two knowledges slice 4 added. The §4.3 reshuffle put ONE knowledge on each transition, so these
# gate the rung-3 verbs and their remedies point at working the rung-2 source — the ladder's
# "practise this rung to unlock the next" rule, stated in the place the player is blocked.
const GATE_REASON_SEED_SELECTION_KNOWLEDGE_FORMAT := "Your people know Seed Selection %d%% — %s forage a Tended Patch to learn it, faster the more you leave standing"

const GATE_REASON_PENNING_KNOWLEDGE_FORMAT := "Your people know Penning %d%% — %s hunt a tamed herd to learn it, faster the more you leave standing"

# The SOURCE reasons — this one animal/patch's own build meter. `Corral`'s remedy now names the
# `Tame` VERB (glyph %s), not "Sustain-hunt this Thriving herd": since slice 3a, Sustain tames
# nothing. That correction is the single most load-bearing copy fix in this slice — the old sentence
# is the exact hidden rule the arc exists to kill.
const GATE_REASON_HERD_DOMESTICATED_FORMAT := "This herd is %d%% tamed — %s Tame it to finish"

# The patch-ecology gate is a STOCK condition, not a policy one, so its remedy is the opposite advice:
# a fully staffed Sustain takes the whole regrowth and holds a Stressed patch Stressed forever. The
# patch only climbs back to Thriving when the take is LESS than the growth — fewer workers, or none.
# %s = the live `patch_ecology_phase`, capitalized.
const GATE_REASON_PATCH_THRIVING_FORMAT := "Patch is %s — ease workers off and let it regrow to Thriving"

# **THE THREE "already built" GATE REASONS ARE GONE** (issue #442). They greyed a completed rung
# that was still standing in the policy picker — "Already a Tended Patch — Sustain-forage it to
# harvest" — a sentence only a UI that models a finished build as a selectable rung ever needs to
# say. An improvement that completes now becomes a static DONE LABEL, so there is no control left to
# grey and no dead end to explain: the state says what it is, and the next rung's checkbox sits
# beneath it. `SourceForecast.improvement_is_done` is the one test that retires a rung.

# THE SOW SITE GATE — "why can't I sow HERE?" is *the* question rung 3 provokes, because only ~1% of
# the map will take seed (46 of 4160 tiles on the standard map: alluvial plain + river delta). The
# client cannot re-derive this — it holds neither the per-biome capacity table nor the hydrology — so
# the sim ships the VERDICT as a stable key and these turn it into the manual's voice. Never show a
# Sow button that just fails, and never answer with a bare "you can't": each line names the fault AND
# points at the rung that lifts it (Worked Land — irrigation and the plough — is a future arc, so the
# promise is deliberately "not yet", not a date).
#
# Rung 3 moves seed but cannot FERTILIZE, so the land itself must do it: the ground has to be rich
# already and near fresh water. Salt coast does not count.
const SOW_REFUSAL_TOO_POOR := "too_poor"

const SOW_REFUSAL_TOO_DRY := "too_dry"

const SOW_REFUSAL_TOO_POOR_AND_TOO_DRY := "too_poor_and_too_dry"

const SOW_REFUSAL_REASONS := {
    "too_poor": "This ground is too thin to take a crop — your people can carry seed, but not yet feed the soil. Look to the river valleys, until they learn to work poorer land.",
    "too_dry": "This ground is rich but too dry to farm — your people can carry seed, but not yet carry water to it. Sow beside fresh water, until they learn to bring it here.",
    "too_poor_and_too_dry": "This ground is both too thin and too dry to take a crop — your people can carry seed, but neither feed the soil nor water it yet. The river valleys will take it; this ground will not, until they learn to work the land.",
}

# An unrecognized refusal key still refuses (fail CLOSED — the sim gates the command regardless, so a
# button offered here would simply fail), and says the one thing we do know.
const SOW_REFUSAL_FALLBACK := "This ground will not take seed — your people cannot yet work land like this."

# A patch with no streamed phase (redacted remembered tile) still fails the Thriving
# test; it reads as unknown rather than asserting a phase we don't have.
const GATE_PHASE_UNKNOWN_LABEL := "not Thriving"

# **THE GATE-REASON LAYOUT VOCABULARY IS GONE** (issue #442) — the one-liner + header/bullet pair,
# the two COLLAPSED formats and the tooltip separator all served `HudWidgets.build_policy_picker`'s
# greyed-and-explained rung. A harvest stance has no prerequisite, so no picker rung is ever gated
# now, and the reasons themselves (which are still very much alive) render beneath the IMPROVEMENT
# checkbox in the shared hint style — one reason per line, no collapsing, because the control is one
# rung and not six and there is no longer a height problem to solve.

# The build-verb for the in-progress Cultivate rung — the plant twin of Husbandry's "Domesticating".
const CULTIVATION_PREPARING_LABEL := "Preparing"

# The DECAYING state's verb, shared by BOTH plant rungs (`DetailFormat.cultivation_label` /
# `field_label`) — a meter below complete that nobody is building. It is the third state those rows
# used to lack: a bleeding 99% wore "Preparing 99%" in neutral ink, identical to a fresh build one
# turn from done, so the card said *gaining* while the player was *losing*. The word is the SIM's own
# ("gone feral — untended, the ground is reverting"), so the tile card and the command-feed receipt
# that fires when the meter finally empties name the same event. ONE word for both rungs on purpose:
# the fact is identical and the ROW's name already says which rung is losing.
const RUNG_REVERTING_LABEL := "Reverting"

# Tile card "Field" row — plant RUNG 3, the patch twin of the herd's "Corral" row and the rung above
# "Cultivation". Its own row (never merged with Cultivation): a patch carries BOTH meters, and a Field
# may stand on ground that was never tended. "Sowing N%" follows the pen's "Building N%" / the fence's
# "Fencing N%" build-verb convention; the completed badge is a Field — deliberately a different WORD
# and a different glyph from "🌾 Tended Patch", because rung 3 is a different thing, not a bigger number.
const FIELD_ROW := "Field"

# Tile card "What grows here" SECTION HEADER (flora roster F1/F5) — the quiet label above the per-plant
# 🌿 rows `DetailFormat.flora_composition_lines` renders. Colon-less on purpose: `detail_bbcode` prints
# it as a dim section header (the `_split_kv` sentence path), the plants themselves following as their
# own indented rows below. Names the plants this tile's forage capacity is MADE OF — naming DECOMPOSES,
# it never adds (the shares sum to 1) — and derived from the biome, so it is descriptive, not a state.
const FLORA_COMPOSITION_ROW := "What grows here"

# One plant's row within that section — `Wild Grain 45%`. Shared with the crop picker, which prints its
# own rows with it too (beside the `· N.N×` payoff term the picker adds).
const FLORA_SHARE_FORMAT := "%s %d%%"

# Tile card "Crop" row (flora roster S1) — the row that appears ABOVE FLORA_COMPOSITION_ROW once a
# band commits the patch to one species under Cultivate/Sow. The two are NOT mutually exclusive and
# never were after issue #433: a commitment REWEIGHTS the basket over the build (a Tended Patch weeds
# the favored share up toward `min(1, share x tended_weeding_gain)`, a Field forces it to 1.0) instead
# of displacing it, and the species is recorded on the first worked turn — ~25 turns before any of
# that lands. So this row says WHAT WAS COMMITTED TO and the section below says WHAT IS GROWING, which
# are different facts for most of a build. Kept well under `DetailFormat`'s 16-char key limit so it
# aligns as a normal table row.
const FLORA_CROP_ROW := "Crop"

# THE CROP PICKER (flora roster S1) — the compose control that makes committing a DECISION instead of
# a server default. It renders under the IMPROVEMENT control, since which plant a patch is committed to
# is part of the same decision as which rung to build; a harvest stance gathers the whole basket and
# chooses nothing. `FLORA_COMMITTING_POLICIES` used to name that pair here and is gone — the plant
# ladder is `SourceForecast.FORAGE_IMPROVEMENTS`, and a second list of the same two verbs was one more
# thing to keep in step (issue #442).

const FLORA_CROP_PICKER_HEADER := "Crop to commit to"

# An entry the SPECIES can never climb this rung with stays VISIBLE and disabled, never hidden: that a
# tile carries Oak Mast you cannot farm is information about the LAND, and hiding it would make the
# tile read poorer than it is. `can_cultivate` / `can_sow` are species-GLOBAL — "can this plant ever
# climb this rung" — so the reason names the plant, not the ground.
const FLORA_CROP_NO_CULTIVATE_FORMAT := "%s cannot be tended — it is a wild harvest only."

const FLORA_CROP_NO_SOW_FORMAT := "%s cannot be sown — its seed is not yours to move."

# A LEGAL BUT MARGINAL CROP IS NEVER DISABLED. A 20%-share plant is a bad choice, not an illegal one,
# and being free to make it is the decision docs/plan_flora_roster.md §4.3 exists to create — only the
# two species flags disable anything. The warning rides the ROW's own tooltip rather than a standing
# hint line: a line under the list costs the sheet ~40px of height, and the commit button below it is
# what pays (see FLORA_CROP_LIST_MAX_HEIGHT).
# THE VERDICT IS RELATIVE TO 1.0, never to an impression of what the numbers "usually" look like.
# Committing beats gathering wild on most good ground, so ratios above 1.0 are the NORM: "poor" is
# reserved for a crop that genuinely loses to simply gathering the tile, and the tier between break-even
# and FLORA_CROP_STRONG_RATIO is the honest middle — worth doing, not worth celebrating.
const FLORA_CROP_STRONG_RATIO := 1.5

const FLORA_CROP_LOSS_TOOLTIP_FORMAT := "%s yields %.1f× what gathering this tile wild does — it loses to simply gathering here."

const FLORA_CROP_MODEST_TOOLTIP_FORMAT := "%s yields %.1f× what gathering this tile wild does — worth committing to."

const FLORA_CROP_STRONG_TOOLTIP_FORMAT := "%s yields %.1f× what gathering this tile wild does — strong ground for it."

# ---- THE ROW FACE: ONE PLANT, EVERY ACCOUNT IT PAYS (issue #419) ---------------------------------
# A crop row is BUILT, not picked from a menu of whole-row formats. It used to be three mutually
# exclusive ones (`FLORA_CROP_ROW_FORMAT` / `_FODDER_ROW_FORMAT` / `_TRADE_ROW_FORMAT`) chosen by an
# if/elif chain, so a row could state exactly ONE account — and the chain tested "is the trade payoff
# > 0" to detect a cash crop. EVERY staple carries `trade_goods_per_biomass: 0.005`, so that test
# fired on all 27 of them and printed every crop as trade-only: `Wild Emmer 39% · 0.4 trade`, with the
# ratio the rung exists to compare nowhere on the row.
#
# So the row states each account that is actually THERE, in the shared render-only-when-non-zero
# shape (`.claude/rules/client/labor-ui.md` → "A hunt pays TWO products"), gated by the ONE
# `SourceForecast.has_component` — the same gate the hunt faces use, never a bespoke threshold:
#
#   Wild Emmer 39% · 1.4× · 0.11 trade      a staple: food ratio LEADS, its trade token trails
#   Cotton Fields 26% · 0.1× · 4.28 trade   a cash crop: both real, and the food ratio is a LOSS
#   Hay Grass 30% · 1.80 hay                fodder only — no provisions ratio to state
#   Oak Mast 12%                            greyed by the ceiling flags: no account at all
#
# THE COMPARISON IS THE POINT. Cotton pays ~38x the staple's trade and costs most of its calories; a
# row stating one account cannot say that, whichever one it picks. And a cash crop's food ratio being
# a WARN-inked loss is the honest reading of the land-use tension, not a bug to suppress: rung 2
# *weeds* rather than replaces, so a tended cotton patch really does keep paying its volunteers'
# calories at a rate below gathering the tile wild.
#
# The row's BASE is `FLORA_SHARE_FORMAT` above — the same `Wild Grain 45%` the tile card's basket rows
# use, which is what keeps the two surfaces quoting one plant identically; the clauses below append to
# it. A row with no account at all IS that bare base.
#
# The RATIO clause — `cultivate_yield_ratio` / `sow_yield_ratio`, what committing this tile to this
# plant yields RELATIVE to gathering it wild. The sim folds the share AND the species' conversion rate
# into it, so the client only formats. ONE decimal, because the decision is "better or worse than
# wild", not a second significant figure.
const FLORA_CROP_RATIO_CLAUSE_FORMAT := " · %.1f×"

# The two NON-FOOD clauses — absolute per-turn rates, so TWO decimals: unlike the ratio these span two
# orders of magnitude across one basket (0.11 trade for a staple's token beside 4.28 for cotton on the
# same ground), and one decimal would flatten the small end to `0.1` and lose exactly the distinction
# the row is for. It is also the precision the shared two-product joiner already uses
# (`SourceForecast.picker_products` → `0.96 food · 0.24 trade`).
const FLORA_CROP_HAY_CLAUSE_FORMAT := " · %.2f hay"
const FLORA_CROP_TRADE_CLAUSE_FORMAT := " · %.2f trade"

# ---- THE TOOLTIP CLAUSES, same composition, same per-rung wording -------------------------------
# `%s` is the RUNG's own noun, because these payoffs are per-rung: a tended patch and a sown field pay
# different amounts from different baskets, and a tooltip naming the wrong one is how the Cultivate
# row came to quote a Field's trade. Fed by FLORA_CROP_RUNG_NOUNS.
const FLORA_CROP_FODDER_TOOLTIP_FORMAT := "%s pays %.2f fodder/turn as %s — feed for penned animals, not food for people."

const FLORA_CROP_TRADE_TOOLTIP_FORMAT := "%s pays %.2f trade/turn as %s — goods for your stockpile, not food for people."

# The rung's noun for the tooltips above, keyed by the composing policy. A payoff quoted on the
# Cultivate rung describes A TENDED PATCH; only the Sow rung describes a sown field.
const FLORA_CROP_RUNG_NOUNS := {
    HudConst.LABOR_POLICY_CULTIVATE: "a tended patch",
    HudConst.LABOR_POLICY_SOW: "a sown field",
}

# The fallback noun for a policy absent from the table above — the rungs that commit nothing quote no
# payoff at all, so this is defensive rather than reachable.
const FLORA_CROP_RUNG_NOUN_FALLBACK := "this rung"

# The break-even: at or above this, committing beats gathering wild; below it the rung is a LOSS and
# the row is inked as one — while staying fully pressable, because a marginal crop is a legal bad idea
# and the ratio exists to stop that being invisible, not to prevent it.
const FLORA_CROP_BREAK_EVEN_RATIO := 1.0

# THE LIST SCROLLS WITHIN ITSELF so a long basket can never push the commit button below the sheet's
# fold. The sheet grows to fit its content and is bounded only by the room left in the VIEWPORT
# (`ComposeSheet.refit`), so a basket long enough to matter would otherwise walk the card down the
# screen — the picker instead lives inside the room the sheet has left, and the budget is TIGHT: a
# Cultivate compose already spends most of the card on the rung gates. Hence
# the work-board's compact row idiom rather than default button chrome (which pads 9px top AND bottom,
# making a row ~37px and the whole picker unaffordable), and hence a cap DERIVED from the rows it
# shows rather than a picked pixel height: `rows × (row + separation)`, with a partial row deliberately
# NOT budgeted for — the cut-off row is itself the "there is more below" affordance.
const FLORA_CROP_ROW_HEIGHT := 22.0

const FLORA_CROP_ROW_FONT_SIZE := HudWorkVocab.WORK_ROW_FONT_SIZE

const FLORA_CROP_ROW_PADDING_V := HudStyle.WORK_ROW_PADDING_V

# MEASURED, not chosen — and set so that NO SHIPPED BASKET EVER HIDES A CROP. The longest a tile can
# carry today is 5 (a navigable hex blends the valley's basket with the channel's fishery), so at 5 the
# whole basket is on screen and the player compares it rather than peering at it through a slot: a
# picker that hides the best crop behind a scroll is the guess the payoff ratio exists to remove. It was
# 2 rows until the OTHER rung's gate reasons were collapsed (a measurement taken when the picker
# still carried six rungs and their gates),
# which is what bought the other three. The cap is still a real guard, not dead code — F5 refines this
# coarse roster into a fine-grained one and baskets lengthen — and ui_preview's
# `forage_crop_picker_overlong` (a synthetic 8-plant tile, longer than any real one) keeps the scroll
# path RENDERED so it cannot rot unseen. `forage_crop_picker` ASSERTS the sheet has nothing left to
# scroll, i.e. `Forage` is on screen; change this number and let that assertion answer, never assume.
const FLORA_CROP_LIST_VISIBLE_ROWS := 5

const FLORA_CROP_BLOCK_SEPARATION := 2

const FLORA_CROP_LIST_MAX_HEIGHT := FLORA_CROP_ROW_HEIGHT * FLORA_CROP_LIST_VISIBLE_ROWS \
    + float(FLORA_CROP_BLOCK_SEPARATION) * (FLORA_CROP_LIST_VISIBLE_ROWS - 1)

const FLORA_CROP_NONE_LEGAL_HINT := "Nothing growing here can climb this rung."

# A committed patch is one-way until it lapses, so the picker becomes a READ-ONLY readout: an editable
# control here would imply a switch the sim will refuse.
const FLORA_CROP_COMMITTED_HEADER := "Committed crop"

const FLORA_CROP_COMMITTED_HINT := "Already committed — the crop cannot be changed until the patch lapses back to wild."

# Herd drawer "Herders" row — a MANAGED herd's staffing (intensification ladder). A domesticated herd
# needs `herders_needed` herders every turn to HOLD the herd; understaffed it SHEDS whole animals over
# its labor capacity into a nearby wild herd (they drift off — tameness leaves with them, it is never
# decayed; fauna neglect-escape arc). The row makes the deficit visible from the ACTUAL staffed count
# (`assigned_herders`, never a reconstruction from `herded_fraction`); the under-herded value is
# WARN-tinted via `DetailFormat.herders_value_hex`, and the shed consequence (`HERDERS_SHED_FORMAT`) is
# spelled out below it so the player knows the animals are drifting off and how to stop it.
# (Herd drawer combat-component rows, Predators Phase 0 — the whole `DANGER_*` family lives in
# `DetailFormat` with `append_danger_component_lines`, its only reader. Strength is NOT danger: a
# mammoth is deadly to HUNT yet no camp THREAT, so the drawer shows the four RAW components
# Elevation-style, with no verdict word. The roster it normalizes the open-ended bars against is
# threaded IN as `_band_labor.world_herds()`, since that module holds no snapshot state.)
# The one ecology phase a patch can be cultivated from (matches `EcologyPhase::as_str`).
const ECOLOGY_PHASE_THRIVING := "thriving"

# The FOUR intensification knowledge tracks (the `intensification_knowledge[]` row's field names) —
# the FACTION-WIDE half of the two-meter split (§4.1). One per rung-transition, so the list IS the
# ladder, and §4.3 pins "no two rungs share an unlock gate":
#   plant:  wild --cultivation--> tended --seed_selection--> field
#   animal: wild --herding------> pastoral --penning-------> pen
# `seed_selection`/`penning` were appended by slice 4 (discovery ids 2005/2006).
const KNOWLEDGE_TRACK_CULTIVATION := "cultivation"

const KNOWLEDGE_TRACK_HERDING := "herding"

const KNOWLEDGE_TRACK_SEED_SELECTION := "seed_selection"

const KNOWLEDGE_TRACK_PENNING := "penning"

# Tile-card PASTURE rows (the graze layer). The twin of `Forage biomass`, and the pair is the point:
# forage is what HUMANS can eat here (seeds, nuts, tubers — food-module tiles only), pasture is what
# ANIMALS can eat here (grass and browse — cellulose humans cannot digest, on nearly every land tile).
# Your best farm is usually not your best pasture. Rendered ONLY where the ground actually carries
# pasture (`graze_capacity > 0`): on a glacier the card prints nothing, never "0 / 0".
const PASTURE_KEY := "Pasture"

# Its own row key rather than the shared "Ecology" one — a forage tile would otherwise show two rows
# both called "Ecology" (the patch's and the pasture's) with no way to tell them apart. The LABEL and
# the TINT are still the shared `DetailFormat.ecology_phase_label` / `ecology_value_hex` path, so a stressed
# pasture reads exactly like a stressed herd or a stressed patch.
const PASTURE_ECOLOGY_KEY := "Pasture ecology"
