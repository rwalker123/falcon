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
# Penning on a tamed one. **A remedy names the PRACTICE, and stops there** — it used to close with
# "faster the more you leave standing", which the compose sheet's aside now says live and quantified
# ("Teaching cultivation at ×1.44 — a higher floor teaches faster"), so carrying it here too stated
# one fact twice in one panel. What a reason must still carry is what is missing, how far along it
# is, and the action that fixes it: naming the prerequisite alone tells a player a door is locked
# without saying where the key is, which is why these are not trimmed further. The surfaces WITHOUT
# an aside — the work board, the map marks, the work inspector — are exactly why the remedy stays.
# Format args: %d = the live faction progress percent, %s = the food-peak glyph.
const GATE_REASON_CULTIVATION_KNOWLEDGE_FORMAT := "Your people know Cultivation %d%% — %s forage a wild patch to learn it"

const GATE_REASON_HERDING_KNOWLEDGE_FORMAT := "Your people know Herding %d%% — %s hunt a wild herd to learn it"

# The two knowledges slice 4 added. The §4.3 reshuffle put ONE knowledge on each transition, so these
# gate the rung-3 verbs and their remedies point at working the rung-2 source — the ladder's
# "practise this rung to unlock the next" rule, stated in the place the player is blocked.
const GATE_REASON_SEED_SELECTION_KNOWLEDGE_FORMAT := "Your people know Seed Selection %d%% — %s forage a Tended Patch to learn it"

const GATE_REASON_PENNING_KNOWLEDGE_FORMAT := "Your people know Penning %d%% — %s hunt a tamed herd to learn it"

# THE WILD FODDER LOCK — the one gate reason on either web that carries TWO remedies, because there
# genuinely are two and they are reached from different ends of the game. Foddering is what KEEPING A
# PENNED HERD teaches, so a pre-pastoral band cannot have it at any price; but the sim credits a
# COMMITTED patch's hay unconditionally, committing being the bid. Naming only the knowledge would
# tell a forager band the hay is out of reach for another whole ladder, while the improvement control
# that fixes it sits directly below on the same sheet.
# Format args: %d = the live Foddering percent, then the CORRAL glyph and the CULTIVATE glyph — the
# rung each remedy is reached through, the `GATE_REASON_HERD_DOMESTICATED_FORMAT` idiom.
#
# **THE KNOWLEDGE HALF IS ITS OWN CONST BECAUSE A SECOND SURFACE SAYS IT.** The band's `Fodder:` row
# states the same lock when the row is dormant for want of the craft, and it has no patch to offer
# the second remedy about — so the shared clause is the part that is true wherever Foddering is
# missing, and the patch-only remedy stays on this format. Spelled by CONCATENATION rather than
# copied (`BandDetailLines.BAND_FODDER_ROW_FORMAT`'s idiom), so the sentence the forage panel renders
# is byte-identical to the one it always rendered and cannot drift from the row's.
# Format args: %d = the live Foddering percent, then the CORRAL glyph.
const FODDERING_NOT_LEARNED_CLAUSE := "your people know Foddering %d%% — %s keep a penned herd to learn it"

const GATE_REASON_WILD_FODDER_FORMAT := "Hay stays in the field: " + FODDERING_NOT_LEARNED_CLAUSE + ", or %s commit this patch to its crop."

# The SOURCE reasons — this one animal/patch's own build meter. `Corral`'s remedy now names the
# `Tame` VERB (glyph %s), not "Sustain-hunt this Thriving herd": since slice 3a, Sustain tames
# nothing. That correction is the single most load-bearing copy fix in this slice — the old sentence
# is the exact hidden rule the arc exists to kill.
const GATE_REASON_HERD_DOMESTICATED_FORMAT := "This herd is %d%% tamed — %s Tame it to finish"

# **THE TWO REFUSALS THAT NAME NO REMEDY, because there is not one** — the destination picker's, and
# its alone. Every other reason here says what is missing AND how to fix it; these say the ladder
# stops here for this animal or this ground, and inventing a remedy for them would promise a door
# that does not exist.
#
# **THEY EXIST BECAUSE THE PICKER SHOWS A BARRED RUNG RATHER THAN HIDING IT**
# (`docs/plan_standing_upkeep.md` §2.8). `RungGates.hunt_rungs_admitted` withholds an
# above-ceiling rung OUTRIGHT — correctly, for a MARK, which promises the verb is available — but a
# track exists to say what the branch HOLDS, and a rung silently missing from it reads as a ladder
# with two steps rather than as one this species cannot climb.
const GATE_REASON_SPECIES_NEVER_TAMED := "This animal will never be tamed — no craft and no work changes that."

const GATE_REASON_SPECIES_NEVER_PENNED := "This animal will never be penned — no craft and no work changes that."

const GATE_REASON_CROP_CANNOT_CLIMB_FORMAT := "Nothing growing here can be taken to a %s."

# …and the one a DESTINATION earns from a rung BELOW it. A climb lays every leg between where the
# source stands and where it was sent, so a barred rung on the way bars everything above it — and the
# honest thing to state is the rung that actually refused rather than a second refusal invented for
# the destination.
const GATE_REASON_PATH_BLOCKED_FORMAT := "%s must be raised first: %s"

# **THE PATCH-ECOLOGY GATE REASON IS GONE** ("Patch is Stressed — ease workers off and let it regrow
# to Thriving"), with `GATE_PHASE_UNKNOWN_LABEL`, the "not Thriving" phrase it fell back to on a
# redacted tile. No rung on either web gates on a source's health: a crew drawing the ground down
# builds slowly in proportion to its escapement floor rather than not at all
# (docs/plan_harvest_floor.md §3.2), and the sheet's teaching line states that pace live. A reason
# has nothing left to refuse.

# **THE THREE "already built" GATE REASONS ARE GONE** (issue #442). They greyed a completed rung
# that was still standing in the policy picker — "Already a Tended Patch — Sustain-forage it to
# harvest" — a sentence only a UI that models a finished build as a selectable rung ever needs to
# say. An improvement that completes now becomes a static DONE LABEL, so there is no control left to
# grey and no dead end to explain: the state says what it is, and the next rung's own line sits
# beneath it. `SourceForecast.improvement_is_done` is the one test that retires a rung.

# THE SOW SITE GATE — "why can't I sow HERE?" is *the* question rung 3 provokes, because only ~1% of
# the map will take seed (46 of 4160 tiles on the standard map: alluvial plain + river delta). The
# client cannot re-derive this — it holds neither the per-biome capacity table nor the hydrology — so
# the sim ships the VERDICT as a stable key and these turn it into the manual's voice. Never show a
# Sow button that just fails, and never answer with a bare "you can't": each line names the fault AND
# what the player can do about it. For the two GROUND readings that is the rung that lifts it (Worked
# Land — irrigation and the plough — is a future arc, so the promise is deliberately "not yet", not a
# date); for the gathering-site verdict below there is no such rung, so it points at other GROUND.
#
# Rung 3 moves seed but cannot FERTILIZE, so the land itself must do it: the ground has to be rich
# already and near fresh water. Salt coast does not count.
const SOW_REFUSAL_TOO_POOR := "too_poor"

# Nobody gathers here — the tile is not one of the curated gathering sites, so no plant rung below 4
# can stand on it whatever the soil says. It **supersedes** the two ground readings rather than
# joining them: the sim short-circuits on it, so a tile that is also thin and dry still ships THIS
# verdict alone, and its line names one fault because the other two are moot while there is no way to
# work the ground at all. It is also the one refusal that is NOT a "not yet" — no rung below Farm
# relaxes the requirement, so the answer is a different tile, not a later turn.
#
# LATENT BUT NOT DEAD: today the compose sheet never opens on such ground
# (`DrawerComposeController._forage_compose_available` gates on the same `tile_is_gathering_site`
# test), so this reason is unreachable through the sheet — while being the verdict the sim ships for
# the large majority of patch tiles. It becomes reachable the moment rung 4 (Farm) drops
# `requires_gathering_site` and the compose block returns on the ground that rung unlocks. Deleting
# it as unreachable would hand that ground `SOW_REFUSAL_FALLBACK`, which is written for a key we do
# not recognize.
const SOW_REFUSAL_NOT_GATHERING_SITE := "not_gathering_site"

const SOW_REFUSAL_TOO_DRY := "too_dry"

const SOW_REFUSAL_TOO_POOR_AND_TOO_DRY := "too_poor_and_too_dry"

const SOW_REFUSAL_REASONS := {
    "not_gathering_site": "Nobody gathers this ground — your people cannot sow land they do not already work. Sow where they gather, or move a band to ground they can.",
    "too_poor": "This ground is too thin to take a crop — your people can carry seed, but not yet feed the soil. Look to the river valleys, until they learn to work poorer land.",
    "too_dry": "This ground is rich but too dry to farm — your people can carry seed, but not yet carry water to it. Sow beside fresh water, until they learn to bring it here.",
    "too_poor_and_too_dry": "This ground is both too thin and too dry to take a crop — your people can carry seed, but neither feed the soil nor water it yet. The river valleys will take it; this ground will not, until they learn to work the land.",
}

# An unrecognized refusal key still refuses (fail CLOSED — the sim gates the command regardless, so a
# button offered here would simply fail), and says the one thing we do know.
const SOW_REFUSAL_FALLBACK := "This ground will not take seed — your people cannot yet work land like this."

# **THE GATE-REASON LAYOUT VOCABULARY IS GONE** (issue #442) — the one-liner + header/bullet pair,
# the two COLLAPSED formats and the tooltip separator all served `HudWidgets.build_policy_picker`'s
# greyed-and-explained rung. A harvest stance has no prerequisite, so no picker rung is ever gated
# now, and the reasons themselves (which are still very much alive) render beneath the IMPROVEMENT
# control in the shared hint style — one reason per line, no collapsing, because the control is one
# rung and not six and there is no longer a height problem to solve.

# RETIRED — the Cultivate rung's build verb AND the decaying state's verb (issue #545). Both were
# headline words on a card row that now leads with a NUMBER: a build states its turn count
# (`HudSelectionVocab.RUNG_TURNS_FORMAT`), and the word `Reverting` moved to
# `RUNG_REVERTING_FORMAT` — which has itself since retired (`docs/plan_standing_upkeep.md` §4.6a: a
# meter nobody is building is HELD where the keeping covers it and LOSING GROUND where it does not,
# and the wire says which). The command-feed receipt that fires when a meter finally empties still
# carries the sim's own word for the event; no card row spells it any more.

# Tile card "Field" row — plant RUNG 3, the patch twin of the herd's "Corral" row and the rung above
# "Cultivation". Its own row (never merged with Cultivation): a patch carries BOTH meters, and a Field
# may stand on ground that was never tended. "Sowing N%" follows the pen's "Building N%" / the fence's
# "Fencing N%" build-verb convention; the completed badge is a Field — deliberately a different WORD
# and a different glyph from "🌾 Tended Patch", because rung 3 is a different thing, not a bigger number.
const FIELD_ROW := "Field"

# One plant's row within the basket — `Wild Grain 45%`. Shared with the crop picker, which prints its
# own rows with it too (beside the `· N.N×` payoff term the picker adds).
#
# THERE IS NO LONGER A "What grows here" HEADING ABOVE THESE ROWS on the tile card. The heading made
# the basket read as a fourth resource sitting beside the stocks; the rows are now an always-visible
# INDENTED list directly under the `Foraging` row, and the indent is what says they decompose it.
const FLORA_SHARE_FORMAT := "%s %d%%"

# …and what that share is IN — the plant's own standing biomass, `share × the patch's carrying
# capacity`, rounded. A percentage alone cannot be added to anything; stating the absolute beside it
# is what lets the three rows visibly sum to the `Foraging` stock they sit under. Parenthesised and
# trailing so the share stays the row's headline and the absolute reads as its expansion.
const FLORA_SHARE_BIOMASS_CLAUSE_FORMAT := "  (%d)"

# The role-icon SLOT on a basket row, blank when the wire states no role. `FloraShareInfo.role` is
# `""` for a species this server's roster no longer knows, and that means UNSTATED, never "staple" —
# so the row renders no icon rather than claiming a category. The slot still holds its width, so one
# untagged plant cannot shift the whole list's names out of column.
const FLORA_ROLE_ICON_UNSTATED := "  "

# Tile card "Crop" row (flora roster S1) — the row a band's commitment to one species under
# Cultivate/Sow puts on the card. It and the basket are NOT mutually exclusive and never were after
# issue #433: a commitment REWEIGHTS the basket over the build (a Tended Patch weeds the favored share
# up toward `min(1, share x tended_weeding_gain)`, a Field forces it to 1.0) instead of displacing it,
# and the species is recorded on the first worked turn — ~25 turns before any of that lands. So this
# row says WHAT WAS COMMITTED TO and the basket says WHAT IS GROWING, which are different facts for
# most of a build. It reads with the BUILD METERS, below the two stock rows, because what it states is
# the standing investment on this ground rather than part of the stock pair; the SIGNAL mark on the
# committed member inside the basket is what joins the two. Kept well under `DetailFormat`'s 16-char
# key limit so it aligns as a normal table row.
const FLORA_CROP_ROW := "Crop"

# THE CROP PICKER (flora roster S1) — the compose control that makes committing a DECISION instead of
# a server default. It renders under the IMPROVEMENT control, since which plant a patch is committed to
# is part of the same decision as which rung to build; a harvest stance gathers the whole basket and
# chooses nothing. `FLORA_COMMITTING_POLICIES` used to name that pair here and is gone — the plant
# ladder is `SourceForecast.FORAGE_IMPROVEMENTS`, and a second list of the same two verbs was one more
# thing to keep in step (issue #442).

# **THE HEADER IS PER RUNG, because "commit" is true of ONE of the two committing rungs.** Sow
# forces the favored species to 100% of the stand (`forage.rs::planted` — a Field has no volunteers),
# so the patch really does become that crop and nothing else: committing is exactly what the picker
# does there. Cultivate only weeds the favored share UPWARD by `tended_weeding_gain` and leaves the
# rest of the basket standing (`forage.rs::weeded`), so a tended patch keeps growing everything it
# grew before — calling that a commitment overstates it, and it is the belief issue #433 already had
# to delete from the tile card (a 64/36 tile reading as 100% one crop the moment a crop was picked).
const FLORA_CROP_PICKER_HEADER := "Crop to commit to"

const FLORA_CROP_TEND_HEADER := "Crop to tend to"

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

# ---- THE ROW FACE: ONE PLANT, EVERY ACCOUNT IT PAYS (issue #419, arc #527) -----------------------
# A crop row is BUILT, not picked from a menu of whole-row formats. It used to be three mutually
# exclusive ones (`FLORA_CROP_ROW_FORMAT` / `_FODDER_ROW_FORMAT` / `_TRADE_ROW_FORMAT`) chosen by an
# if/elif chain, so a row could state exactly ONE account — and the chain tested "is the trade payoff
# > 0" to detect a cash crop. EVERY staple carried the flat `trade_goods_per_biomass` token, so that
# test fired on all 27 of them and printed every crop as trade-only: `Wild Emmer 39% · 0.4 trade`,
# with the ratio the rung exists to compare nowhere on the row.
#
# So the row states each account that is actually THERE, in the shared render-only-when-non-zero
# shape (`.claude/rules/client/labor-ui.md` → "A source pays a VECTOR OF ACCOUNTS"), gated by the ONE
# `SourceForecast.has_component` — the same gate the hunt faces use, never a bespoke threshold:
#
#   Wild Emmer 39% · 1.4×                   a staple: the food ratio, and nothing else to state
#   Cotton Fields 26% · 0.1× · 0.29 fibre   a cash crop: both real, and the food ratio is a LOSS
#   Hay Grass 30% · 1.80 hay                fodder only — no provisions ratio to state
#   Oak Mast 12%                            greyed by the ceiling flags: no account at all
#
# THE COMPARISON IS THE POINT. Cotton costs most of its calories and pays fibre for them; a row
# stating one account cannot say that, whichever one it picks. And a cash crop's food ratio being a
# WARN-inked loss is the honest reading of the land-use tension, not a bug to suppress: rung 2
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

# The NON-FOOD clauses — absolute per-turn rates, so TWO decimals: unlike the ratio these span two
# orders of magnitude across one basket (a staple's incidental 0.03 fibre beside cotton's 0.29 on the
# same ground), and one decimal would flatten the small end to `0.0` and lose exactly the distinction
# the row is for. It is also the precision the shared product joiner already uses
# (`SourceForecast.picker_products` → `0.96 food · 0.40 fodder`).
const FLORA_CROP_HAY_CLAUSE_FORMAT := " · %.2f hay"

# The separator every clause on a crop row leads with, named because a row that begins WITH one has
# to trim it and two spellings of the same three characters is how the trim comes to miss.
const FLORA_CROP_CLAUSE_LEAD := " · "

# > #### ⛔ THE ONE CLAUSE ON A CROP ROW THAT RENDERS **AT ZERO**
#
# Every other clause here obeys the render-only-when-non-zero rule, and this one must not: the crop
# picker exists because nothing on the path from *this ground is fertile* to *this field feeds nobody*
# stated the zero. A cash crop's `sowPayoff` is exactly `0` (a sown Field is 100% its crop), so an
# omitted clause there is the silence that let a fertile tile be committed to tobacco.
#
# It is TWO decimals like its non-food neighbours rather than the ratio's one, because it is an
# absolute per-turn rate and is read against them.
const FLORA_CROP_FOOD_CLAUSE_FORMAT := FLORA_CROP_CLAUSE_LEAD + "%.2f food"

# > #### THE ONE CLAUSE ON A CROP ROW THAT IS A **COST**
#
# Every clause above states what a rung would PAY; this states what committing to this crop would
# cost in WORK, and it LEADS the row for that reason — a price is read before the payoffs it is
# weighed against. It takes the same ` · ` lead as its neighbours because the row trims one leading
# separator and only one, whichever clause happens to come first.
#
# The figure is `DetailFormat.format_work_units`' — whole numbers bare, fractions to one place, the
# spelling every other work quantity in this client already uses — so the crop rows and the rung
# track's own `38 work` cannot round one price two ways.
const FLORA_CROP_WORK_CLAUSE_FORMAT := FLORA_CROP_CLAUSE_LEAD + "%s work"

# ---- WHAT A CASH CROP PAYS: ONE CLAUSE PER MATERIAL (arc #527) ----------------------------------
# **A VECTOR, NOT A SCALAR, IS THE WHOLE DIFFERENCE.** The retired `· %.2f trade` clause answered
# *"how much trade"* — a number a market could total and a player could not act on. This answers
# *"0.29 fibre"*, which is what a cash crop IS. **Never sum the rows into one materials/turn figure**:
# that is the retired axis under a new name, and it re-collapses the distinction the materials model
# exists to keep. One clause per material, in the wire's order (merged by id and sorted by it
# sim-side, so the order is stable across turns).
#
# **THE MATERIAL NAMES ITSELF, AND THAT IS THE MARK IT WEARS.** `⇄` used to lead every non-food
# component of a yield, and it could only ever say "this is not food" — the one thing an account with
# no name has to fall back on. A material HAS a name, and it is a short lowercase word on the wire
# (`fibre`, `tobacco`, `grape` — the `materials.json` ids the material catalogue and a band's
# `material_batches` are keyed by), so the row reads `0.29 fibre` exactly as its neighbour reads
# `1.80 hay`: the noun IS the mark, and there is no generic glyph because there is no longer a
# generic account. **Do not add one.**
const FLORA_CROP_MATERIAL_CLAUSE_FORMAT := " · %.2f %s"

# ---- THE TOOLTIP CLAUSES, same composition, same per-rung wording -------------------------------
# `%s` is the RUNG's own noun, because these payoffs are per-rung: a tended patch and a sown field pay
# different amounts from different baskets, and a tooltip naming the wrong one is how the Cultivate
# row came to quote a Field's figures. Fed by FLORA_CROP_RUNG_NOUNS.
const FLORA_CROP_FODDER_TOOLTIP_FORMAT := "%s pays %.2f fodder/turn as %s — feed for penned animals, not food for people."

# ONE LINE PER MATERIAL, for the same reason the face carries one clause per material. `%s` order is
# crop, amount, material, rung noun. It says "for your crafters" rather than "for your stockpile"
# because that is the concrete difference the arc bought: a material is worked at a bench into
# something, where a trade good only ever sat in a pile nothing could read.
const FLORA_CROP_MATERIAL_TOOLTIP_FORMAT := "%s pays %.2f %s/turn as %s — a material for your crafters, not food for people."

# The rung's noun for the tooltips above, keyed by the composing policy. A payoff quoted on the
# Cultivate rung describes A TENDED PATCH; only the Sow rung describes a sown field.
const FLORA_CROP_RUNG_NOUNS := {
    HudConst.LABOR_POLICY_CULTIVATE: "a tended patch",
    HudConst.LABOR_POLICY_SOW: "a sown field",
}

# The fallback noun for a policy absent from the table above — the rungs that commit nothing quote no
# payoff at all, so this is defensive rather than reachable.
#
# **IT SAYS `this ground` BECAUSE `rung` IS OURS AND NOT THE PLAYER'S.** The word is exact internally
# and appears on no control, in no tutorial and in no other sentence a player reads; the two entries
# above it already name what the player sees (*a tended patch*, *a sown field*), and a fallback in a
# private vocabulary would be the one place the ladder's jargon leaked into a tooltip.
const FLORA_CROP_RUNG_NOUN_FALLBACK := "this ground"

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

# A crop-picker row wears its species' bundled ART where there is any (issue #339), as the Button's
# own `icon` — and the cap is what keeps that from breaking the list's arithmetic. The source PNGs
# are 256px, which a Button reserves IN FULL, so an uncapped icon would set the row's minimum height
# and the MEASURED `FLORA_CROP_LIST_MAX_HEIGHT` below — derived from `FLORA_CROP_ROW_HEIGHT` — would
# then describe rows that no longer exist. Held under that 22.0 row height deliberately, with room
# for the row's own padding; `COMPOSE_QUARRY_ICON_MAX_WIDTH` records the same trap on the compose
# row's WIDTH.
const FLORA_CROP_ICON_MAX_WIDTH := 16

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

# Herd drawer "Keepers" row — a MANAGED herd's staffing (intensification ladder). A domesticated herd
# needs keepers every turn to HOLD the herd; understaffed it SHEDS whole animals over its labor
# capacity into a nearby wild herd (they drift off — tameness leaves with them, it is never decayed;
# fauna neglect-escape arc). The row makes the deficit visible from the ACTUAL staffed count
# (`assigned_keepers` — the MAINTAIN crews, never the hunting one and never a reconstruction from
# `herded_fraction`); the under-herded HERD states it on the shed sentence now, `Keepers:` having
# been retired with issue #545, and
# the shed consequence (`HERDERS_SHED_FORMAT`) is spelled out below it so the player knows the animals
# are drifting off and which crew stops it.
# (Herd drawer combat-component rows, Predators Phase 0 — the whole `DANGER_*` family lives in
# `DetailFormat` with `append_danger_component_lines`, its only reader. Strength is NOT danger: a
# mammoth is deadly to HUNT yet no camp THREAT, so the drawer shows the four RAW components
# Elevation-style, with no verdict word. The roster it normalizes the open-ended bars against is
# threaded IN as `_band_labor.world_herds()`, since that module holds no snapshot state.)
# The one ecology phase a patch can be cultivated from (matches `EcologyPhase::as_str`).
const ECOLOGY_PHASE_THRIVING := "thriving"

# The intensification knowledge tracks the CLIENT NAMES BY HAND — the ones a gate table or a readout
# has to spell. **They are not the whole set**: the ladder's knowledges ride the wire as a roster now
# (`ladder_knowledge`), so the knowledge screen builds itself from that and needs none of these. What
# is named here is what code must MATCH on, and §4.3 pins "no two rungs share an unlock gate".
# The first four are one per rung-transition, so they read as the ladder itself —
# the FACTION-WIDE half of the two-meter split (§4.1). The first FOUR are one per rung-transition, so
# they read as the ladder itself, and §4.3 pins "no two rungs share an unlock gate":
#   plant:  wild --cultivation--> tended --seed_selection--> field
#   animal: wild --herding------> pastoral --penning-------> pen
# `seed_selection`/`penning` were appended by slice 4 (discovery ids 2005/2006).
const KNOWLEDGE_TRACK_CULTIVATION := "cultivation"

const KNOWLEDGE_TRACK_HERDING := "herding"

const KNOWLEDGE_TRACK_SEED_SELECTION := "seed_selection"

const KNOWLEDGE_TRACK_PENNING := "penning"

# **THE ROUTE BRANCH'S TWO** (arc #532), one per built rung: a trail carrying traffic teaches
# `roadbuilding`, which opens `grade`; keeping a dirt road teaches `paving`, which opens `pave`. Their
# discovery ids are 2012/2013 — **2011 is RETIRED, not reused** (`trailcraft` was a lesson for
# something you cannot fail to do and was deleted rather than retuned).
const KNOWLEDGE_TRACK_ROADBUILDING := "roadbuilding"

const KNOWLEDGE_TRACK_PAVING := "paving"

# **THE FIFTH IS DELIBERATELY NOT A RUNG TRANSITION** — no rung waits on it, and nothing about the
# ladder's shape changes when it is learned. It is what the PEN rung TEACHES (the corral rung's
# `earns_knowledge`), and what it buys is every FODDER seam a faction has: the pen's hay draw, the
# pen's `K` fodder term, and the WILD forage patch's fodder credit. So a wild hay meadow can publish a
# positive `fodder_per_biomass` — what the LAND pays — that a pre-pastoral band cannot bank; see
# `GATE_REASON_WILD_FODDER_FORMAT` below, and `RungGates.wild_fodder_reason` which composes it.
const KNOWLEDGE_TRACK_FODDERING := "foddering"

# ---- THE TILE CARD'S TWO FOOD-WEB STOCK ROWS ---------------------------------------------------
# One row per WEB, named for WHO EATS IT, and rendered ADJACENT with Foraging first.
#
# THE NAMES ARE THE FIX. The card carried both stocks under names that inverted each other: the
# stock rows were `Pasture` (bare) and `Forage biomass` (qualified) while the ecology rows were
# `Pasture ecology` (qualified) and `Ecology` (bare) — so the unqualified word meant the ANIMAL layer
# in one pair and the HUMAN layer in the other, and a reader who learned one pattern was mis-taught
# by the other. `Foraging` / `Grazing` are parallel on the one axis that cannot invert: who is doing
# the eating. (Playtest: one reader mistook one layer for the other three times.)
#
# THE ADJACENCY IS THE OTHER HALF. The two used to be interleaved — pasture's pair, then the module
# row, then the basket, then forage's pair — which split the human layer in half around the animal
# one. They are now consecutive rows, because a comparison the player cannot make with one glance is
# not a comparison. What each web offers is still the point: humans eat seeds/nuts/tubers off a
# food-module tile; animals eat grass and browse off nearly every land tile. Your best farm is
# usually not your best pasture.
#
# Each is rendered ONLY where that web has a stock at all — `DetailFormat.patch_capacity > 0` /
# `graze_capacity > 0` — so a glacier prints no Grazing row and a moduleless tile no Foraging row,
# never a "0 / 0" that would read as a starved stock rather than an absent one. The forage side asks
# a HELPER rather than a key because a remembered hex states the tile's ground `K` while a visible one
# states the patch's boosted ceiling (`MapView.FOW_DISCOVERED_HIDDEN_KEYS` header).
const FORAGING_KEY := "Foraging"

const GRAZING_KEY := "Grazing"

# Standing stock over its ceiling, the shape both webs read in — `205 / 205`. Whole units: these are
# biomass stocks in the hundreds, and a decimal on either side buys no decision.
const STOCK_FORMAT := "%.0f / %.0f"

# THE SAME ROW WITH THE STOCK UNKNOWN — `— / 205`, what BOTH webs read on a remembered tile
# (issue #462). The capacity is still the tile's own and still true; only the standing level is
# unknowable on ground the player cannot see. The em-dash holds the numerator's place rather than the
# row being dropped, so the pair stays positionally parallel with the live card — and it is the one
# glyph that cannot be misread as a quantity, which is the entire failure this form exists to
# prevent: the tile-level "Remembered" chip and unknown-contents note were both already on screen
# when a reader last carried a fogged `130 / 130` into their model of the forage patch. A label on
# the tile does not label the number; this does.
#
# Spelled STRUCTURALLY (the `DetailFormat.RECOVERY_GUIDANCE_TEXT` idiom) so the glyph the harness
# searches for and the glyph the row actually renders are one value and cannot drift apart.
const STOCK_UNKNOWN_GLYPH := "—"

const STOCK_UNKNOWN_FORMAT := STOCK_UNKNOWN_GLYPH + " / %.0f"

# THE ECOLOGY PHASE RIDES THE STOCK ROW, IT IS NO LONGER A ROW OF ITS OWN. Two standing `Ecology` /
# `Pasture ecology` rows doubled the height of a readout whose whole content is one word each, and
# put the second web's stock two rows away from the first's. The phase is a condition OF that stock,
# so it reads after it on the same line — and the tint is unchanged: `DetailFormat.ecology_value_hex`
# still keys the neutral/amber/red off the phase word, now matched inside the composed value.
const STOCK_PHASE_CLAUSE_FORMAT := "%s · %s"

# ---- THE SPECIES CHIPS — which plants this crew carries home (the selective gather) --------------
#
# One row of chips on the forage compose sheet, one per named plant in the tile's basket, standing
# where the retired crop picker stood: under the crew's kit row and above the improvement control, so
# the sheet reads band → floor → crew → kit → WHAT WE CARRY HOME → what we are building → the terms.
#
# **A CHIP IS WRITTEN EXACTLY AS THE TILE CARD WRITES THE SPECIES** — `🌾 Wild Emmer 70% (63)` — and
# it is composed from that card's OWN consts (`FoodIcons.for_crop_role`, `FLORA_SHARE_FORMAT`,
# `FLORA_SHARE_BIOMASS_CLAUSE_FORMAT`), never from a second spelling. One plant reads one way in this
# client, or the card and the sheet start disagreeing about the same stand the first time either moves.
#
# **THE BRACKETED NUMBER IS OFF THE WIRE** (`ForagePatchState.compositionStandingBiomass`, folded onto
# the composition entry by the decoder). The client holds no capacity arithmetic and must not multiply
# a share by a stock to recover it: a selective gather asks *"is there enough emmer here to be worth
# two hands"*, which is a question about a QUANTITY, and the sim states it exactly as it states the
# take. A plant the wire quotes no biomass for renders NO clause rather than a `(0)`.
const TAKE_CHIP_UNKNOWN_BIOMASS := ""

# **TWO STATES, AND THE PILL IS THE WHOLE OF THE AFFORDANCE.** A chip is either selected — the filled,
# bordered pill — or it is not, in which case it is plain text with no decoration at all. A source with
# no explicit selection renders as EVERY plant selected, because that is what it means: the crew carries
# the whole basket home.
#
# **THERE WAS A THIRD STATE AND IT ONLY EXISTED TO PAPER OVER THE MODEL.** With the empty selection
# rendered as *faintly included*, clicking one chip made the set exactly `{that plant}` and every other
# chip changed with it — correct by the model and indefensible on screen, since the player pressed one
# thing and a different thing moved. The chips are plain toggles on the thing you clicked now
# (`ComposeState.toggle_forage_take_species` expands the implicit-all before it removes), so a
# default-included chip and a picked one are the same state and there is nothing for a third to say.
#
# **THE CHECKBOX GLYPHS WENT WITH IT.** A tick mark beside a filled pill is a second signal for one
# fact, and the pill is the one the eye reads first. What still tells the two MODES apart is the row's
# own key (`Carry home` against `Crop`), the consequence line beneath it, and the fact that a
# single-pick row never lights more than one chip.
const TAKE_STATE_SELECTED := "selected"
const TAKE_STATE_UNSELECTED := "unselected"

# The chip's own type size — the work row's, like every other compact list on this sheet.
const TAKE_CHIP_FONT_SIZE := HudWorkVocab.WORK_ROW_FONT_SIZE
const TAKE_CHIP_SEPARATION := 4

# The row's own key, in the compose sheet's field-row idiom.
const TAKE_ROW_LABEL := "Carry home"
const TAKE_ROW_LABEL_SINGLE := "Crop"

# ---- THE CONSEQUENCE LINE — CULTIVATE ONLY, and it is the naming that earns it ------------------
#
# A cultivate commits the ground to ONE crop, and with nothing picked the sim settles that crop
# itself. The default line NAMES it, because silence there is the game choosing for the player
# without saying so; the narrowed line says what committing costs the rest of the stand.
#
# **THE FORAGE SIDE HAS NO LINE AT ALL, and the three sentences it had are retired.** Two described
# the selection the chips directly above were already showing — *"The whole basket comes home."* /
# *"Everything else is left standing."* — which is the row restated in prose. The third spoke the
# refusal when the last remaining plant was unticked, and it went for the same reason: a player finds
# out that the last chip will not turn off by pressing it, and a sentence is more verbosity than the
# fact is worth. **The refusal itself is untouched** — `ComposeState.toggle_forage_take_species`
# still declines the edit and the chip still does not move. The click is a silent no-op by intent,
# and the chips are NOT greyed to signal it.
const TAKE_NOTE_CULTIVATE_NARROWED_FORMAT := "%s weeds the rest out of the ground."
const TAKE_NOTE_CULTIVATE_DEFAULT_FORMAT := "Nothing picked — this ground would be committed to %s."

# **THE NUMBERS FOR A NARROWED CREW ARE COMPOSED FROM THE WIRE'S PER-SPECIES RATES.**
# `provisionsPerBiomass` on the patch is the BASKET AVERAGE, which is why this sheet once sat still
# when a chip was ticked; `compositionProvisionsPerBiomass` and its fodder twin state the same
# quantity per plant, index-aligned with the basket, so a selection's rate is
# `Sum(share x rate) / Sum(share)` over the ticked species and the forecast moves the moment a chip
# does — the same `NOW -> AFTER` treatment the worker stepper already gets.
#
# **WHAT IS STILL NOT KNOWN IS SAID OUT LOUD — in ONE sentence, about ONE of the two states.**
#
# `selection_rates` answers its unquotable dict from two arms and only this one has anything to say:
# the other is a selection naming plants this tile no longer grows, where the game KNOWS what
# happened and the sheet simply quotes nothing. A sentence explaining a state the game already
# handles correctly is prose the player has to read to learn nothing. The composer's two-state
# distinction STAYS — it is what stops this sentence being printed about the other state, which is
# the lie it was split out to end.
#
# This one is a narrowing the wire priced NO per-species rate for. The composition is a weighted
# mean, so one missing term is not a term that can be left out of it, and there is nothing else this
# client holds that a rate could be recovered from — so it quotes nothing at all rather than the
# whole basket's numbers under a narrowed heading, which is the quote-vs-payout defect this arc has
# shipped before. **A `0.0` rate is NOT this case**: a cash crop pays no food and says so, and the
# selection is fully quoted.
const TAKE_UNQUOTED_NOTE := "This selection is not priced yet — no take is quoted for it."

# **THE MATERIAL ACCOUNT IS NO LONGER ONE OF THESE SILENCES.** It was, for exactly as long as
# `materialPerBiomass` was the only material rate on the wire — a basket average with nothing finer
# behind it, so a narrowed crew's fibre was a number this client did not have.
# `compositionMaterialPerBiomass` publishes it per plant, and the sheet composes it per MATERIAL ID
# through the same weighted mean the two scalars take, so *tick cotton, see how much fibre* — the
# case the whole feature was argued on — is answered with a real number.
