class_name HudDisclosureVocab

## Shared Food/Morale disclosure protocol — the row keys + breakdown-kind + [url] meta prefix that
## BOTH `DetailFormat` (emits the meta) and `DisclosureController` (parses it) speak. Owned by neither.

# Band food flow lives on the Food summary line: `Food 15 (19 turns) · −0.77 /turn` (net =
# food_income − food_consumption, sign-tinted), with a click-to-expand category breakdown
# (Gathered/Hunted/Consumed) underneath — mirroring the morale breakdown. `SourceForecast.FOOD_FLOW_MIN` gates both
# the net readout and each breakdown category (below it → absent, not shown as a zero).
# Click-to-open disclosure shared by the Food + Morale summary rows: a ▸/▾ caret on the row label and
# a clickable `[url]` meta = `<prefix><kind>:<entity>` dispatched by `DisclosureController`.
#
# THE BREAKDOWN OPENS IN A POPOVER, NEVER INLINE. Expanding it in place grew the vitals label — a
# `fit_content` RichTextLabel — by several lines AFTER `build_band_zone` had already chosen
# its height tier from the zone box, and the zone box is fixed by design with `clip_contents` hosts,
# so the extra lines silently sliced the WORKFORCE row and ate the role cards. A Window cannot change
# a zone's height, which is the same reason the section `⋯` menus are `MenuButton`s and the
# destructive confirms are `ConfirmationDialog`s. The work board's budgeted inline inspector strip is
# the other idiom and does not apply here: in the SHORT tier the chart is already dropped and the role
# cards are already hint-less, so there is nothing left to spend but PEOPLE/WORKFORCE — the content.
# The `[url]` meta prefix stays HERE: the formatter emits it, the disclosure controller parses it, and
# both preview harnesses build one — shared vocabulary rather than either half's own. (The ▸/▾ carets
# themselves are `DetailFormat`'s, and the popover's geometry `DisclosureController`'s.)
const BREAKDOWN_TOGGLE_META_PREFIX := "breakdown:"

## The FACTION page's drill-down rows are LINKS TO BANDS, which is the second meta this popover
## carries. A distinct prefix rather than a reuse: `_on_meta_clicked` dispatches on it, and a band
## jump is not a breakdown toggle — one changes the panel's subject, the other opens a popover.
## The payload is the band's ENTITY, which is what the panel resolves a subject by.
const FACTION_BAND_JUMP_META_PREFIX := "faction_band:"

const BREAKDOWN_KIND_FOOD := "food"

const BREAKDOWN_KIND_MORALE := "morale"

# The band's birth rate vs its normal rate, itemized into the three named fertility factors
# (`docs/plan_population_growth_model.md`) — the birth path's parallel of the morale breakdown, and
# the same click-to-open popover. Its rows are MULTIPLIERS, not signed deltas, because the factors
# combine by product; see `DetailFormat.fertility_breakdown_row`.
const BREAKDOWN_KIND_GROWTH := "growth"

# **`BREAKDOWN_KIND_TRADE` IS RETIRED** (arc #527) with the row it drilled into: the band's trade
# goods were the second product of the same worked sources the Food breakdown itemizes, and that
# account no longer exists.

# **`BREAKDOWN_KIND_KIT` IS RETIRED** (`docs/plan_standing_upkeep.md` §4.9 item 12) with the `Gear`
# row it opened. A band's item conditions do not compress to a vitals line and the CRAFTING panel's
# kit ledger already owns them in full — the Builders card's own gear line was retired in §4.7 for
# exactly this reason. What replaces the row is NOTIFICATION: `equipment.json`'s `life_readout` seams
# now reach the event dock as `kit_life` (warn → Notable, danger → Alert).
# **`DisclosureController.kit_breakdown_lines` and the `DetailFormat` kit LEAVES stay** — the crafting
# panel and the compose sheet still read them.

# The band's STANDING MATERIAL BILL (`docs/plan_standing_upkeep.md` §2.7) — what its holdings swallow
# per turn in goods, what arrives, and what is on the shelf, ONE BLOCK PER GOOD. It is a disclosure
# for the reason Food and Fodder are: the ROW answers "which good runs out first and how long have I
# got" in one glance, and the three terms that explain it are a table.
#
# ⛔ **THE POPOVER IS WHERE THE PER-GOOD DETAIL LIVES, AND THAT IS NOT A LAYOUT PREFERENCE.** Inline
# growth in a fixed-height zone is what clipped `Zone_band` once already, and this ledger grows with
# the number of goods the band owes — which is config, so the next material would do it again. See
# `DisclosureController.material_upkeep_breakdown_lines`.
const BREAKDOWN_KIND_UPKEEP := "upkeep"

# The band's FODDER larder, itemized into the two flows that move it — what its Fields grew and what
# its pens ate. It is a disclosure for the same reason Food is: the ROW answers "how long have I got"
# in one glance, and the flows that explain it are a table. Its row carried those two rates INLINE
# once (`Fodder: 100.0 · need 6.0/turn · growing 5.0/turn · 100 turns`) and wrapped to two lines in
# the narrow drawer for it — inline growth in a fixed-height zone being the very mistake the popover
# exists to prevent. See `DisclosureController.fodder_breakdown_lines`.
const BREAKDOWN_KIND_FODDER := "fodder"

# The detail-row labels the disclosure attaches to (must equal the `Key` the detail formatter splits out).
const DETAIL_ROW_FOOD := "Food"

const DETAIL_ROW_MORALE := "Morale"

const DETAIL_ROW_GROWTH := "Growth"

# The band's fodder larder, beneath Food and spelled exactly as its own summary row is — the row
# label IS the registration key, so the two cannot drift.
const DETAIL_ROW_FODDER := "Fodder"

# **`DETAIL_ROW_KIT` ("Gear") IS RETIRED** (`docs/plan_standing_upkeep.md` §4.9 item 12) from both the
# band page and the faction page. See `BREAKDOWN_KIND_UPKEEP` above for why, and for what took its
# discovery path.

# The band's standing material bill, beneath its two larders and spelled exactly as its own summary
# row is — the row label IS the registration key, so the two cannot drift. **"Upkeep" names the
# question the row answers** (*what do the things I have built cost me to keep?*); the value cell
# names ONE GOOD, which is what keeps it from reading as the summed materials scalar this arc refuses.
const DETAIL_ROW_UPKEEP := "Upkeep"
