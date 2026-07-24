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
# Penning on a tamed one. Format args: %d = the live faction progress percent, %s = the Sustain glyph.
const GATE_REASON_CULTIVATION_KNOWLEDGE_FORMAT := "Your people know Cultivation %d%% — %s Sustain-forage a wild patch to learn it"

const GATE_REASON_HERDING_KNOWLEDGE_FORMAT := "Your people know Herding %d%% — %s Sustain-hunt a wild herd to learn it"

# The two knowledges slice 4 added. The §4.3 reshuffle put ONE knowledge on each transition, so these
# gate the rung-3 verbs and their remedies point at working the rung-2 source — the ladder's
# "practise this rung to unlock the next" rule, stated in the place the player is blocked.
const GATE_REASON_SEED_SELECTION_KNOWLEDGE_FORMAT := "Your people know Seed Selection %d%% — %s Sustain-forage a Tended Patch to learn it"

const GATE_REASON_PENNING_KNOWLEDGE_FORMAT := "Your people know Penning %d%% — %s Sustain-hunt a tamed herd to learn it"

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

# A COMPLETED investment rung is a dead-end no-op — the build is DONE, so re-running the verb only pays
# the low prep dip forever. The rung is greyed (like Sow is greyed when gated) and the reason points the
# player at the ♻ Sustain that now HARVESTS the finished ground, where the real payoff lives. Mirrors the
# SOURCE-reason voice ("This herd is 40% tamed — ◎ Tame it to finish") for a state that is already there.
const GATE_REASON_ALREADY_TENDED_FORMAT := "Already a Tended Patch — %s Sustain-forage it to harvest"

const GATE_REASON_ALREADY_FIELD_FORMAT := "Already a Field — %s Sustain-forage it to harvest"

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

# A single-reason gate reads as a compact one-liner under the picker row ("🌱 Cultivate — <reason>").
const GATE_REASON_LINE_FORMAT := "%s — %s"

# Two or more reasons are far too long for one line, so they render as a header + one bullet each
# ("🌱 Cultivate needs:" / "   · <reason>").
const GATE_REASON_HEADER_FORMAT := "%s needs:"

const GATE_REASON_BULLET_FORMAT := "   · %s"

# COLLAPSING ANOTHER RUNG'S REASONS — OPT-IN, and deliberately narrow. Three wrapped paragraphs
# explaining why *Sow* is refused while the player composes a *Cultivate* answer a question they did
# not ask and cost about a third of the compose card; the crop picker, the stepper and the commit
# button are what paid. But spelled-out reasons are also how the ladder TEACHES — several frames exist
# precisely to show a NON-composed rung's full prerequisites (`forage_cultivate_locked`,
# `forage_sow_locked`, `herd_corral_locked*`, and `two_meter_split`, whose whole subject is the gated
# Corral's reason line while Tame is composed). So this is NOT the shared default: `HudWidgets.build_policy_picker`
# collapses only when its caller asks, and the only caller that asks is the forage compose while a
# COMMITTING rung is selected — i.e. exactly when the crop picker is on the card competing for height.
# Every other picker (hunt, expedition, work board) is byte-for-byte unchanged.
const GATE_REASON_COLLAPSED_ONE_FORMAT := "%s — locked (1 requirement unmet)"

const GATE_REASON_COLLAPSED_MANY_FORMAT := "%s — locked (%d requirements unmet)"

# The disabled button's tooltip carries every reason, one per line.
const GATE_REASON_TOOLTIP_SEPARATOR := "\n"

# The build-verb for the in-progress Cultivate rung — the plant twin of Husbandry's "Domesticating".
const CULTIVATION_PREPARING_LABEL := "Preparing"

# Tile card "Field" row — plant RUNG 3, the patch twin of the herd's "Corral" row and the rung above
# "Cultivation". Its own row (never merged with Cultivation): a patch carries BOTH meters, and a Field
# may stand on ground that was never tended. "Sowing N%" follows the pen's "Building N%" / the fence's
# "Fencing N%" build-verb convention; the completed badge is a Field — deliberately a different WORD
# and a different glyph from "🌾 Tended Patch", because rung 3 is a different thing, not a bigger number.
const FIELD_ROW := "Field"

# Tile card "What grows here" row (flora roster F1) — the named plants this tile's forage capacity is
# MADE OF. Naming DECOMPOSES, it never adds: the shares sum to 1, so this says what the Forage number
# already on the card consists of. Derived from the biome, so it is descriptive, not a state.
const FLORA_COMPOSITION_ROW := "What grows here"

# (The row's own ` · ` separator is `DetailFormat.FLORA_SHARE_SEPARATOR` — only the composition
# formatter uses it. This FORMAT stays: the crop picker prints its rows with it too.)
const FLORA_SHARE_FORMAT := "%s %d%%"

# Tile card "Crop" row (flora roster S1) — the row FLORA_COMPOSITION_ROW becomes once a band commits
# the patch to one species under Cultivate/Sow. The basket is displaced (that is the cost of tending
# — docs/plan_flora_roster.md §4.3), so the two rows are mutually exclusive: a committed tile is one
# plant, and showing the wild mix beside it would state what no longer grows there. Kept well under
# `DetailFormat`'s 16-char key limit so it aligns as a normal table row, like the row it replaces.
const FLORA_CROP_ROW := "Crop"

# THE CROP PICKER (flora roster S1) — the compose control that makes committing a DECISION instead of
# a server default. It renders only under the two rungs that actually commit a patch to one plant; the
# extractive rungs gather the whole basket and choose nothing, so a crop control there would be noise.
const FLORA_COMMITTING_POLICIES := [HudConst.LABOR_POLICY_CULTIVATE, HudConst.LABOR_POLICY_SOW]

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

# THE PAYOFF, beside the share — `cultivate_yield_ratio` / `sow_yield_ratio`: what committing this tile
# to this plant yields RELATIVE to gathering it wild. The sim folds the share AND the species'
# conversion rate into it, so the client only formats. `Wild Emmer 34% · 1.35×` — one decimal, because
# the decision is "better or worse than wild", not a second significant figure.
const FLORA_CROP_ROW_FORMAT := "%s %d%% · %.1f×"

# A FODDER crop (hay) pays HAY, not provisions, so its provisions ratio is 0 and the `N.N×` row would
# read it as worthless (Flora roster F3). When `sow_fodder_payoff > 0` the row instead states the hay
# value in its own account — `Hay Grass 30% · 1.8 hay` — so a valuable feed crop never reads as a loss.
const FLORA_CROP_FODDER_ROW_FORMAT := "%s %d%% · %.1f hay"

const FLORA_CROP_FODDER_TOOLTIP_FORMAT := "%s pays %.1f fodder/turn as a sown field — feed for penned animals, not food for people."

# The break-even: at or above this, committing beats gathering wild; below it the rung is a LOSS and
# the row is inked as one — while staying fully pressable, because a marginal crop is a legal bad idea
# and the ratio exists to stop that being invisible, not to prevent it.
const FLORA_CROP_BREAK_EVEN_RATIO := 1.0

# THE LIST SCROLLS WITHIN ITSELF so a long basket can never push the commit button below the sheet's
# fold. The sheet's own `CARD_MAX_HEIGHT` is deliberately NOT raised — that cap belongs to every
# compose card, not just this one — so the picker has to live inside the room the sheet has left, and
# the budget is TIGHT: a Cultivate compose already spends most of the card on the rung gates. Hence
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
# 2 rows until the OTHER rung's gate reasons were collapsed (see GATE_REASON_COLLAPSED_ONE_FORMAT),
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

const FLORA_CROP_COMMITTED_HINT := "Already committed — this patch stays this crop until it lapses back to wild."

# Herd drawer "Herders" row — a MANAGED herd's staffing (intensification ladder). A domesticated herd
# needs `herders_needed` herders every turn to HOLD its tameness; understaffed (`herded_fraction < 1`)
# it DECAYS out of the pastoral rung, slips back to wild, and stops earning Penning — the silent stall
# a playtest hit ("🐄 Domesticated" with no signal that Penning had stopped). The row makes the deficit
# visible; the under-herded value is WARN-tinted via `DetailFormat.herders_value_hex`, and the slipping consequence
# is spelled out below it so the player knows WHY Penning stalled and how to fix it.
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
