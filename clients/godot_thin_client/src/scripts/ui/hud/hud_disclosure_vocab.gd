class_name HudDisclosureVocab

## Shared Food/Morale disclosure protocol — the row keys + breakdown-kind + [url] meta prefix that
## BOTH `DetailFormat` (emits the meta) and `DisclosureController` (parses it) speak. Owned by neither.

# Band food flow lives on the Food summary line: `Food 15 (19 turns) · −0.77 /turn` (net =
# food_income − food_consumption, sign-tinted), with a click-to-expand category breakdown
# (Gathered/Hunted/Eaten) underneath — mirroring the morale breakdown. `SourceForecast.FOOD_FLOW_MIN` gates both
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

const BREAKDOWN_KIND_FOOD := "food"

const BREAKDOWN_KIND_MORALE := "morale"

# The band's birth rate vs its normal rate, itemized into the three named fertility factors
# (`docs/plan_population_growth_model.md`) — the birth path's parallel of the morale breakdown, and
# the same click-to-open popover. Its rows are MULTIPLIERS, not signed deltas, because the factors
# combine by product; see `DetailFormat.fertility_breakdown_row`.
const BREAKDOWN_KIND_GROWTH := "growth"

# The band's trade goods — the SECOND product of the same worked sources the Food breakdown itemizes
# (issue #381), so its popover carries the same two income rows (Gathered/Hunted) and no debit row at
# all: nothing consumes trade goods. See `DisclosureController.trade_breakdown_lines`.
const BREAKDOWN_KIND_TRADE := "trade"

# The band's THREE consumable kits (`docs/plan_hunt_through_combat.md` §4.8) — spears, a sled, and
# baskets — and the tier each one currently resolves its role to. It is a disclosure rather than three
# more vitals rows because the ROW answers "how long have I got" in one glance and the popover answers
# "and what happens when it runs out", which is a sentence and a table. See
# `DisclosureController.kit_breakdown_lines`.
const BREAKDOWN_KIND_KIT := "kit"

# The detail-row labels the disclosure attaches to (must equal the `Key` the detail formatter splits out).
const DETAIL_ROW_FOOD := "Food"

const DETAIL_ROW_MORALE := "Morale"

const DETAIL_ROW_GROWTH := "Growth"

const DETAIL_ROW_TRADE := "Trade"

const DETAIL_ROW_KIT := "Kit"
