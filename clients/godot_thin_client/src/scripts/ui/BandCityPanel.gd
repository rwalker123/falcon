extends CanvasLayer
class_name BandCityPanel

## The dockable Band / City panel (docs/plan_band_city_dock.md §"Architecture 2").
##
## A CanvasLayer that renders a card against one screen edge and *reserves* that
## strip via the slice-1 reservation API (Main fans `reservation_changed` out to
## `MapView`/`Hud`, so the map + HUD reflow off the edge rather than being
## overlaid). Chrome: settlement header (stage glyph + name + stage label), a
## settlement cycler, a 4-cell dock chooser, and a collapse toggle, plus dock +
## tab persistence.
##
## THE BODY IS **AN ORDERED LIST OF NAMED ZONES AT A FIXED CROSS-AXIS SIZE**, declared by the SUBJECT
## (`set_zone_layout`) and filled by it (`set_zones`). A band declares three — `band` (vitals), `work`
## (the paged work board) and `parties`; the faction page declares four, adding `knowledge`. Nothing is
## balanced, so no content can migrate between zones; nothing is fitted to
## content, so the reservation this panel reports changes ONLY on dock / collapse
## / hide / viewport-resize — never on a content edit. That is the whole point of
## the model: the previous block-packing body re-measured on every render and
## re-emitted `reservation_changed`, which invalidated the map cache and flickered
## the map on every `+` press.
##
## Two SHELLS host those zones, chosen by the panel's own WIDTH (never by dock
## edge, so a resizable dock needs no special case):
##   * WIDE  (width >= `wide_shell_min_width()`, DERIVED from what the LIVE zone list needs — in
##     practice a T/B dock on a wide window): every
##     zone side by side, the flanks fixed-width, work taking the rest,
##     hairline separators between. No tab bar.
##   * NARROW (otherwise, in practice a L/R dock): a tab bar under the header and
##     exactly one zone beneath it filling the panel.
##
## **THE THRESHOLD IS PER-SUBJECT, and that follows from it being derived.** A four-zone page needs a
## wider panel than a three-zone one, so on a window between the two the faction page correctly tabs
## while a band's page stays abreast. That is why the layout is DECLARED BEFORE the zones are built —
## see `set_zone_layout`.
##
## There is deliberately **no ScrollContainer anywhere in this panel except the
## parties zone's list** — the design is no-scroll (the work zone pages itself
## against `work_zone_size()`), because a scroll whose content height reached the
## panel would silently reintroduce content-dependent sizing. The one exception
## declares a fixed minimum and so reaches nothing; see
## `BandPanelController.build_parties_zone`.
##
## All geometry/typography flows from named constants + `HudStyle` (no magic
## numbers, one visual-language source).

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

# ---- geometry (canvas-space px) --------------------------------------------
## Cross-axis size of the expanded panel when docked L/R. FIXED — it is never fitted to the band
## content, so the reserved strip (and therefore the map inset) cannot move when a section grows.
const PANEL_WIDTH := 380.0
## Cross-axis size of the expanded panel when docked T/B. Likewise fixed; tall enough for the three
## zones' rows without eating the map.
##
## **IT IS THE BODY'S BUDGET, NOT THE STRIP'S HEIGHT** — `_horizontal_panel_height()` adds whatever the
## ACTIVE SHELL spends on its own chrome (the narrow shell's tab bar) before clamping to
## `MAX_WIDE_HEIGHT_FRACTION`, so both shells hand their zones the same box. Read as a flat strip
## height it is 35px short in the narrow shell, which is the zone the tab bar used to be paid out of.
##
## **IT IS THE ONE-COLUMN BUDGET SINCE THE FLANK LEARNED TO WIDEN.** A two-column flank needs far less
## stacking room than a one-column one, so `_horizontal_panel_height()` picks
## `PANEL_HEIGHT_WIDE_TWO_COLUMN` there instead — declared below, beside the column cap it keys off.
##
## **IT WAS 360, AND §4.7's POOLS BLOCK IS WHAT RAISED IT.** That block costs the WORK zone 82px (110
## with its fund-mode row), which is more than any cap on the BUILD QUEUE could give back: at 360 the
## zone's own worst case — a band short of keepers AND with a build queued — wanted 331px of the 300px
## box it was handed, and a four-entry queue could draw ONE row. The number is MEASURED against two
## criteria on the 1920 bottom dock, not rounded to taste:
##
##   1. the worst case fits with `HudWorkVocab.build_queue_rows_max` resolving to the authored ceiling
##      of 3 — it needs a **348px** box;
##   2. the board still pages at least the **2 rows** it drew before the slice — the binding one, at
##      **376px**: a four-entry queue at the full ceiling costs 132, and the board's second row has to
##      clear the pager as well.
##
## **440 hands the zones a 380px box**, which clears both with 4px of slack, and is deliberately still
## short of `HudWorkVocab.BAND_ZONE_TALL_MIN_HEIGHT` (420): a one-column flank's tier is the box times
## one, so a box at or over that flips the band zone to TALL — which would restore the food-outlook
## chart and the role-card hints on a horizontal dock, and costs another 40px of strip on top of this.
## **The raise crosses `BAND_ZONE_CHART_MIN_HEIGHT` (340) either way**, so a one-column horizontal dock
## renders at COMPACT rather than SHORT now and the vitals rows no longer merge there.
##
## At a 1080-high viewport the strip is **41% of the window** against the old 33%, inside
## `MAX_WIDE_HEIGHT_FRACTION` (0.6) with room.
const PANEL_HEIGHT_WIDE := 440.0
## FLOOR on the cross-axis size when collapsed to a thin rail (both orientations) — the rail is at
## least this thin, and thicker when its own chrome needs more (`_collapsed_cross_axis_size`).
const COLLAPSED_SIZE := 46.0
## Render above the map (and the HUD/Inspector) so the panel owns its reserved strip.
const LAYER_INDEX := 103
## Accent seam thickness on the panel's map-facing edge (the prototype's SIGNAL_DEEP border).
const SEAM_THICKNESS := 2.0

# ---- chrome typography / sizing --------------------------------------------
const STAGE_GLYPH_FONT_SIZE := 20
## Bundled stage sprite box, sized to the glyph label's font size so swapping a `Label` for a
## `TextureRect` leaves the header's height and the rows beside it exactly where they were.
const STAGE_SPRITE_SIZE := Vector2(STAGE_GLYPH_FONT_SIZE, STAGE_GLYPH_FONT_SIZE)
const NAME_FONT_SIZE := 15
const STAGE_LABEL_FONT_SIZE := 10
## Gap between the stage word and the band's hex coordinates on the header's second line. Its own
## const rather than the borrowed `HEADER_SEPARATION`: that one spaces the header's top-level
## CLUSTERS (subject / cycler / dock chooser / collapse), and this spaces two words inside one of
## them, which reads too loose at the cluster gap.
const STAGE_ROW_SEPARATION := 6
const CYCLER_FONT_SIZE := 13
const COUNT_FONT_SIZE := 11
const ICON_BUTTON_FONT_SIZE := 13
const HEADER_SEPARATION := 8
const COLUMN_SEPARATION := 0
# Clickable subject cluster ("jump to my band"): a subtle rounded hover tint (transparent
# otherwise); same content margins in both states so hover doesn't shift the header layout.
const SUBJECT_HOVER_CORNER_RADIUS := 5
const SUBJECT_HOVER_PADDING_H := 4
const SUBJECT_HOVER_PADDING_V := 2
const ICON_BUTTON_SIZE := 24.0
const DOCK_CELL_SIZE := 16.0
const DOCK_CELL_SEPARATION := 3
const DOCK_ACCENT_WIDTH := 4
const CORNER_RADIUS := 3
const COUNT_MIN_WIDTH := 30.0
const BODY_EMPTY_TEXT := "No band selected"
const BODY_SEPARATION := 8
## Card inner padding (the PanelContainer content margins). Named so the wide-dock fit-to-content
## height math reuses the exact same paddings the card draws with (no magic 12/10 duplicated).
const PANEL_CONTENT_MARGIN_H := 12
const PANEL_CONTENT_MARGIN_V := 10
## Card border thickness (`panel_card_stylebox`), subtracted alongside the content margins when the
## panel reports the interior box its Work zone may fill. Declared here, beside the margins it is
## always summed with, so `PANEL_CHROME_H` below can be a `const`.
const PANEL_BORDER_WIDTH := 1.0
## What the card's own horizontal chrome costs — the border plus the content margins the card draws
## with, i.e. exactly what `_interior_size()` subtracts from `_panel_extent().x`. Named so the shell
## threshold (which is tested against the panel's OUTER width) can add it back, and so
## `ZONE_PARTY_WIDTH` below can state the narrow shell's zone width as a derivation. Declared HERE,
## with the two terms it sums, because a `const` may not reference one declared further down.
const PANEL_CHROME_H := 2.0 * (float(PANEL_CONTENT_MARGIN_H) + PANEL_BORDER_WIDTH)
# ---- responsive body layout (wide 3-column shell vs narrow tabbed shell) -----
## Fixed widths of the two flanking zones in the wide shell; Work takes whatever is left.
## **THE BAND ZONE IS `PANEL_WIDTH` WIDE, DELIBERATELY THE SAME NUMBER**: the NARROW shell hands its
## one zone the panel's strip less chrome (`ZONE_PARTY_WIDTH` below is exactly that), so a band column
## narrower than that gave the layout with a whole screen to spend LESS width for the same rows than
## the layout squeezed into a side dock — and the band zone CLIPS rather than scrolls, so the width it
## lacks comes straight off its vitals rows as wraps. It takes the full 380 rather than that floor
## because it is the zone whose rows are widest (the merged Food line measures 353px) and, unlike the
## parties zone, 380 here still leaves the work board two columns at 1920 on every shipped map. 380 is
## already this file's vocabulary (`PANEL_WIDTH`, `ZONE_WORK_MIN_WIDTH`): one readable column of rows.
const ZONE_BAND_WIDTH := 380.0
## The most columns the BAND flank will lay its blocks out across on a horizontal dock.
##
## **GROW THE LONG AXIS, NEVER THE RESERVED ONE.** The panel reserves its CROSS axis — width on a
## vertical dock, height on a horizontal one — so growth along the reserved axis re-emits
## `reservation_changed`, re-insets `MapView` and invalidates its cache, which is the map flicker the
## fixed cross-axis size exists to prevent. Growth along the LONG axis costs nothing. A horizontal
## dock therefore spends a wide monitor on band COLUMNS rather than on a taller strip. It does NOT buy
## the strip's height back — see `PANEL_HEIGHT_WIDE` for the two constraints that turned out to pin
## that, neither of them this flank.
##
## **TWO, because the split is AUTHORED** (`BandPanelController.build_band_zone`): the blocks are
## heterogeneous — a wrapped vitals label, two composition bars, a row of role cards — so a generic
## reflow produces nonsense, and a third column would need a third authored split with nothing
## measured to put in it. Measured at 380px on the SHORT tier: vitals 52, PEOPLE 58, WORKFORCE 139.
const BAND_ZONE_MAX_COLUMNS := 2
## **THE PARTIES ZONE IS EXACTLY THE NARROW SHELL'S ZONE WIDTH** — the panel's strip less the card
## chrome — which IS the requirement: the wide shell must never hand a zone LESS room than the side
## dock does for the same content, and this zone's four-rung compose picker is already 2×2 at that
## width because 3-across does not fit. It is deliberately NOT `ZONE_BAND_WIDTH`'s 380, and the
## difference is MEASURED, not taste: the flanks come out of the work zone, and at 1920 in a bottom
## dock on the widest shipped map (Large, whose chrome rail is 308px against Standard's 296) a 380px
## parties zone leaves the board 751px — under 2 × `ZONE_WORK_MIN_WIDTH`, so the work board drops to
## ONE column, which is the very thing the wide shell exists to prevent. The ceiling that keeps two
## columns there is 371; 354 clears it by 17px. Raise this only with that measurement re-run.
const ZONE_PARTY_WIDTH := PANEL_WIDTH - PANEL_CHROME_H
## The faction page's KNOWLEDGE column takes the SAME floor as the parties column, and for the same
## rule rather than by imitation: no wide-shell zone may be narrower than the one the NARROW shell
## hands a side dock, because a layout with a whole screen to spend must never give the same rows less
## room than a 380px strip does. Written as `ZONE_PARTY_WIDTH` so the floor has one home — if that
## measurement ever moves, both flanks move with it.
const ZONE_KNOWLEDGE_WIDTH := ZONE_PARTY_WIDTH
## The NARROWEST the WORK zone may be for the wide shell to be worth choosing: one readable board
## column. MIRRORS Hud's `WORK_COLUMN_MIN_WIDTH` (380) — the width below which `_work_board_capacity`
## clamps to a single column, and a single column crammed into less than that clips its row labels.
## Kept here rather than read from Hud — the panel must not depend on its content's internals — but
## this and `ZONE_WORK_MAX_WIDTH` are a PAIR with Hud's column consts: change the board's column
## width or cap and change both of these with it.
const ZONE_WORK_MIN_WIDTH := 380.0
## The widest the WORK zone can use: Hud's board stops adding columns at `WORK_MAX_COLUMNS` (4) of
## `WORK_COLUMN_MIN_WIDTH` (380), so past this a wider zone only stretches the same rows. Kept here
## rather than read from Hud — the panel must not depend on its content's internals — but the two are
## a PAIR: change the board's column cap and change this with it (and `ZONE_WORK_MIN_WIDTH` with it).
const ZONE_WORK_MAX_WIDTH := 1520.0
## The most board columns the work zone will ever draw — the pair above stated as a count, since since
## issue #377 the card's width is built UP from a column count rather than clamped DOWN from a width.
## Derived, so it cannot drift from the two widths it comes from.
const WORK_MAX_COLUMNS := int(ZONE_WORK_MAX_WIDTH / ZONE_WORK_MIN_WIDTH)
## Hairline separator drawn between adjacent zones in the wide shell.
const ZONE_SEPARATOR_THICKNESS := 1.0
## Gap either side of a zone separator, so the hairline is not flush against zone content.
const ZONE_SEPARATION := 12
## What ONE separator + its gaps cost — the hairline plus a `ZONE_SEPARATION` gap either side. Named
## because the trailing chrome rail is separated from the content column by exactly one of these
## (`_rail_span()`), so the rail's gutter costs the same as any inter-zone gutter by construction
## rather than by a matching pair of literals.
const RAIL_SEPARATOR_SPAN := ZONE_SEPARATOR_THICKNESS + 2.0 * float(ZONE_SEPARATION)
## The tallest content a **TWO-column** band flank has to hold: its CHARTED split (vitals + outlook |
## PEOPLE + WORKFORCE) at the TALL tier, measured **263px** on `band_panel_band_columns_two_charted`.
##
## **MEASURED, never derived.** The authored splits' separations and label wrapping differ per
## grouping, so the flank's height does not decompose by subtracting a block from another split's
## total — both attempts to predict one from the other came out ~12px wrong. Re-measure all four
## candidates (`BandPanelController.build_band_zone`) before moving it.
const BAND_ZONE_TWO_COLUMN_EXTENT := 263.0
## The slack the two-column body budget carries over that extent. **A measurement tolerance, not
## padding**: 263 is a laid-out extent off float rects summed through nine block separations, and a
## budget set to exactly it would fail `band_panel_preview._assert_zone_content_fits` on sub-pixel
## drift in any one of them. Stated as one `ZONE_SEPARATION` — the smallest unit of vertical air this
## panel already spends — rather than as a fresh number of its own.
const BAND_ZONE_TWO_COLUMN_SLACK := float(ZONE_SEPARATION)
## What a horizontal strip spends before a zone sees its box: the header row plus the card's own
## vertical chrome (`_interior_size`'s `chrome_v` = 2 × (`PANEL_CONTENT_MARGIN_V` +
## `PANEL_BORDER_WIDTH`) = 22, and a 38px header). **The header is PURE CHROME** — two text rows
## beside fixed icon controls — which is what lets its height be a constant here at all, and it is
## the same 60px `PANEL_HEIGHT_WIDE`'s 360 already spends to hand its zones a 300px box.
const HORIZONTAL_BODY_CHROME := 60.0
## **THE BODY'S BUDGET WHEN THE BAND FLANK HAS TWO COLUMNS**, chosen by `_horizontal_panel_height()`.
##
## **STILL CONTENT-INDEPENDENT, which is the only reason it may exist.** The selector is
## `band_zone_columns()`, a function of the viewport, the dock edge, the rail's declared span and the
## lateral bounds — not one term is content — so the strip's height stays off the snapshot's critical
## path, exactly as `_shell_chrome_height()`'s geometric term does. A budget that tracked what the
## band HOLDS would re-emit `reservation_changed` → `MapView.set_reserved_inset` on every `+` press,
## which is the map flicker the fixed cross-axis size exists to prevent.
##
## **DERIVED FROM WHAT THE ZONES NEED AT TWO COLUMNS, never a round number.** The saving is unlocked
## by the parties LIST learning to scroll (`BandPanelController.build_parties_zone`), which retires
## the 294px worst case that used to pin this budget for both counts. What is left, per zone:
##
##   * **band flank** — `BAND_ZONE_TWO_COLUMN_EXTENT`, the binding one.
##   * **parties zone** — its fixed chrome only now: the head, the pinned Scout/Hunt/Deny row and
##     `HudWorkVocab.PARTIES_LIST_MIN_HEIGHT`. Measured well under the flank, so it cannot bind.
##   * **work zone** — ⛔ **IT BINDS NOW, AND THIS BULLET USED TO SAY IT COULD NOT.** The old reading
##     was *"pages itself against `work_zone_size()`, so a shorter box costs it a board row rather than
##     overflowing — it never binds by construction"*, and that was TRUE while the zone was head +
##     chips + board + pager: every part of it either paged or was one row, so a shorter box only ever
##     meant fewer rows. `docs/plan_standing_upkeep.md` §4.7 put two FIXED-height blocks in it — the
##     POOLS block (82px, 110 with its fund-mode row) and the BUILD QUEUE block — and neither pages.
##     The zone therefore has a HARD FLOOR: its head, the pools block, the queue's head and one entry
##     row and the overflow row, the chips, one board row, the pager and the gaps between them.
##     **Measured at 284px** on the worst case a band can reach (short of keepers AND something
##     queued), against the 275px this budget offered — so it was clipping, silently, on exactly the
##     wide monitors the two-column flank exists for. `_body_budget()` takes the MAX of the two budgets
##     for that reason, and `band_panel_pools_wide_two_column` is the frame that fails if it stops.
##
##     **The reasoning that expired is the thing to watch, not the number**: "it pages, so it cannot
##     bind" holds only while every part of a zone either pages or is one row. Adding a block that does
##     neither ends it, and the block will look harmless because the ONE-column budget absorbs it.
##
## **A ONE-column horizontal dock keeps `PANEL_HEIGHT_WIDE` EXACTLY**, and must: its flank still
## stacks 299px into the 300px box, and a flat lower budget would slice it. Top docks are one column
## (the lateral bounds cost them 704px of span), so no top-dock frame moves.
##
## **THE FLOOR IS THE PARKED CHROME STACK, and it is asserted rather than assumed.** On a BOTTOM dock
## `DockRowController._required_height()` — the nav cluster + the turn cluster + `RAIL_SLOT_SEPARATION`
## + the card's chrome, **322px measured** — is the strip below which the reflow gate DECLINES,
## `BottomBar` keeps the minimap and orb, and issue #324's whole dock-row reflow silently un-does
## itself. And it does not merely un-do: the gate FEEDS BACK, because a declined park restores the
## HUD's lateral bounds, which costs the span, which drops the flank to one column, which restores the
## 360 strip, which parks again. `band_panel_preview._assert_parked_chrome_margin` pins the margin on
## a two-column bottom dock, and `_assert_band_columns_converge` is what would catch the oscillation.
const PANEL_HEIGHT_WIDE_TWO_COLUMN := BAND_ZONE_TWO_COLUMN_EXTENT + BAND_ZONE_TWO_COLUMN_SLACK \
	+ HORIZONTAL_BODY_CHROME
## Safety net so a short window can never let the T/B strip eat the screen: the reserved wide-dock
## height is the body budget clamped to this fraction of the window height.
const MAX_WIDE_HEIGHT_FRACTION := 0.6
## Header height used for the interior maths before the header has laid out once (it is pure chrome —
## two text rows beside `ICON_BUTTON_SIZE` controls — so this is a bootstrap value, not a guess about
## content).
const HEADER_HEIGHT_FALLBACK := 44.0

# ---- the zone layout + the narrow-shell tab bar ------------------------------
## Zone keys. The same keys index the layout's slots, `set_zones`' contents, the tab bar and
## `set_tab_badge`. The panel owns the KEYS (they index a persisted preference and a badge table);
## the SUBJECT owns the words — see `set_zone_layout`.
const ZONE_BAND := &"band"
const ZONE_WORK := &"work"
const ZONE_KNOWLEDGE := &"knowledge"
const ZONE_PARTIES := &"parties"
## Every zone key the panel knows. **The persisted tab is validated against THIS rather than against
## the live layout**, because prefs load before any subject has declared one — a player who left on
## the faction page's `knowledge` tab must not have that selection thrown away by the bootstrap
## layout, which is a band's. A key the live layout lacks is handled by `_effective_tab`, which falls
## back to the first zone that has content.
const ZONE_KEYS: Array[StringName] = [ZONE_BAND, ZONE_WORK, ZONE_KNOWLEDGE, ZONE_PARTIES]
## A zone descriptor's fields (`set_zone_layout`). Named consts rather than bare strings, the
## `HudWidgets.MENU_ENTRY_*` idiom: a mistyped key in a Dictionary literal is silent.
const ZONE_SPEC_KEY := "key"
const ZONE_SPEC_LABEL := "label"
const ZONE_SPEC_WIDTH := "width"
## **HOW MANY COLUMNS OF `ZONE_SPEC_WIDTH` THIS ZONE MAY LAY ITS BLOCKS OUT ACROSS**, when the row can
## pay for them. Absent means ONE, so every zone that has never asked is bit-identical to before.
##
## **A DECLARED WIDTH IS A BASE, NOT AN EXTENT — that is the whole of how a variable-width zone fits
## the ordered-list model.** `ZONE_SPEC_WIDTH` stays the width of ONE readable column and is what
## `wide_shell_min_width()` sums, because the threshold asks the minimum the shell needs to be worth
## choosing; `_zone_span()` multiplies it by what `zone_columns()` GRANTED and is what every width
## reader takes. Writing the granted extent into the spec instead would put a function of the card's
## own span inside the sum the shell test reads, i.e. a cycle.
##
## **THE CAP IS THE SUBJECT'S TO DECLARE, because the SPLIT IS AUTHORED.** A band's `band` zone
## declares `BAND_ZONE_MAX_COLUMNS` and `BandPanelController.build_band_zone` authors a two-way split
## to fill it; the faction page's `band` zone is `FactionRollup`'s, which authors none, so it declares
## nothing and stays at one — a widened flank with a one-column builder in it renders half a box of
## blank card, which is the emptiness the widening exists to remove rather than move.
const ZONE_SPEC_MAX_COLUMNS := "max_columns"
## What an undeclared `ZONE_SPEC_MAX_COLUMNS` means, and the floor `zone_columns()` clamps to.
const ZONE_COLUMNS_MIN := 1
## A zone whose wide-shell width is `ZONE_WIDTH_EXPAND` takes whatever the fixed flanks leave. Exactly
## one zone in a layout should carry it — the work board, whose column count is what the card's width
## is built up from (`set_work_columns`).
const ZONE_WIDTH_EXPAND := 0.0
## The BOOTSTRAP layout, standing until the first subject declares one. It exists so the panel has a
## sane geometry (hosts, card width, shell threshold) from `_build` — before any controller has
## rendered — and it carries **no labels at all**, deliberately: the tab bar is never visible before a
## subject has spoken (`_update_body_visibility` gates it on `_band_present`, which only `set_zones`
## turns on), and a default word here would be a second home for a label the subject already owns.
const DEFAULT_ZONE_LAYOUT: Array[Dictionary] = [
	{ZONE_SPEC_KEY: ZONE_BAND, ZONE_SPEC_LABEL: "", ZONE_SPEC_WIDTH: ZONE_BAND_WIDTH,
		ZONE_SPEC_MAX_COLUMNS: BAND_ZONE_MAX_COLUMNS},
	{ZONE_SPEC_KEY: ZONE_WORK, ZONE_SPEC_LABEL: "", ZONE_SPEC_WIDTH: ZONE_WIDTH_EXPAND},
	{ZONE_SPEC_KEY: ZONE_PARTIES, ZONE_SPEC_LABEL: "", ZONE_SPEC_WIDTH: ZONE_PARTY_WIDTH},
]
## The tab a fresh session opens on: work is the zone the player acts in.
const DEFAULT_TAB := ZONE_WORK
const TAB_FONT_SIZE := 12
const TAB_BADGE_FONT_SIZE := 10
const TAB_SEPARATION := 4
const TAB_PADDING_H := 10
const TAB_PADDING_V := 5
## Thickness of the active tab's underline (the prototype's SIGNAL rule under the selected tab).
const TAB_UNDERLINE_THICKNESS := 2
const TAB_BADGE_CORNER_RADIUS := 7
const TAB_BADGE_PADDING_H := 5
const TAB_BADGE_PADDING_V := 1
const CYCLE_PREV := -1
const CYCLE_NEXT := 1

# ---- the ACTION REGISTRY (one list, three mount points) ---------------------
## Where the registered actions are CURRENTLY built. It is a LAYOUT answer, taken from the panel's own
## state — the dock's orientation and whether it is collapsed — never from the registration, which is
## why no caller may pass or read it.
const ACTION_MOUNT_NONE := 0
## The subject ROW, beside the cycler: an EXPANDED HORIZONTAL dock's mount. Width is plentiful across a
## whole monitor and height is the axis the strip reserves, so a row of its own is map given up.
const ACTION_MOUNT_SUBJECT_ROW := 1
## The BAR under the subject row: an EXPANDED VERTICAL dock's mount. There is no strip to grow — the
## card is the window's height — so the row is nearly free, while the 380px width is the axis that binds.
const ACTION_MOUNT_BAR := 2
## The COLLAPSED RAIL, in BOTH orientations: the verbs stay reachable from a railed panel, which is
## what makes collapsing it a way to keep working rather than a way to put the panel away. The rail
## runs along the dock's plentiful axis (`_apply_header_rail_orientation`), so the actions cost the
## `COLLAPSED_SIZE` cross axis nothing whichever edge it is on.
const ACTION_MOUNT_RAIL := 3
## Gap between two action glyphs — `HEADER_SEPARATION`, because the bar is a second row of the SAME
## chrome and its buttons must read as members of the header's icon family, not as a new control set.
## The subject-row mount inherits the same separation from the header it sits on.
const ACTION_BAR_SEPARATION := HEADER_SEPARATION
## Breathing room above and below the bar, so it reads as its own row rather than as a second line of
## the subject block. HALF the body gutter: this is chrome sitting next to chrome, not a separated
## region — the full `BODY_SEPARATION` is what the narrow shell puts between its tab bar and content.
const ACTION_BAR_MARGIN_V := BODY_SEPARATION / 2
## Registry keys of an action descriptor (`register_action` builds them; nothing else writes one).
const ACTION_SPEC_ID := "id"
const ACTION_SPEC_GLYPH := "glyph"
const ACTION_SPEC_TOOLTIP := "tooltip"
const ACTION_SPEC_ENABLED := "enabled"
## The Materials & Crafting launcher's registry id — the panel registers its own ⚒ through the same
## seam every other action uses, so there is no privileged action.
const ACTION_CRAFTING := &"crafting"

# ---- chrome glyphs (geometric — render reliably, unlike emoji magnifiers) ---
const COLLAPSE_GLYPH := "▾"   # ▾  minimize
const EXPAND_GLYPH := "▸"     # ▸  restore
const CYCLE_PREV_GLYPH := "◀" # ◀
const CYCLE_NEXT_GLYPH := "▶" # ▶
## The Materials & Crafting launcher. Its glyph and tooltip are the crafting panel's own vocabulary,
## read back from the leaf that owns them so the header and the panel it opens cannot drift apart.
const CRAFTING_GLYPH := HudCraftingVocab.LAUNCH_GLYPH
const CRAFTING_TOOLTIP := HudCraftingVocab.LAUNCH_TOOLTIP
const DEFAULT_STAGE_GLYPH := "⛺" # ⛺  nomadic fallback
## The subject cluster's affordance, cleared while the subject is not jumpable (`set_subject_jumpable`).
const SUBJECT_JUMP_TOOLTIP := "Jump to this band on the map"

# ---- persistence (decision 5 — first client user-pref file) ----------------
const CONFIG_PATH := "user://band_city_dock.cfg"
const CONFIG_SECTION := "dock"
const CONFIG_KEY_EDGE := "edge"
const CONFIG_KEY_COLLAPSED := "collapsed"
## The narrow shell's selected tab, so a reopened session lands where the player left it.
const CONFIG_KEY_TAB := "tab"
## The WORK board's chosen sort, stored as an OPAQUE string: the sort vocabulary belongs to
## `BandPanelController`, so this panel persists the word without ever knowing what it means.
const CONFIG_KEY_WORK_SORT := "work_sort"
## Preview harnesses point this at a scratch file so a render can neither READ nor WRITE the
## player's real dock prefs — the same isolation `NarrativeForkPanel.config_path_override` gives the
## HUD-panel prefs, and for the same reason: without it a harness renders whatever tab the LAST run
## happened to leave selected, and then saves its own tab walk back over the player's.
static var config_path_override: String = ""

## The four dock edges, in the prototype's 2×2 chooser order (row-major:
## left/top on the first row, bottom/right on the second).
const DOCK_EDGES: Array[int] = [SIDE_LEFT, SIDE_TOP, SIDE_BOTTOM, SIDE_RIGHT]
## The two SLOTS of the row's trailing chrome rail (issue #324), stacked top-to-bottom: the HUD parks
## its nav cluster in the top one and its turn cluster in the bottom one. ONE column at the trailing
## end, never a gutter at each end — two opposite gutters cost ~562px of row, pushed the band zone
## inward AND stranded dead space around the orb; one column costs `max(nav, turn)` ≈ 296–302 depending
## on map aspect (296 Standard, 302 Large) instead.
const RAIL_SLOT_TOP := 0
const RAIL_SLOT_BOTTOM := 1
## Slot order, top-to-bottom — also what `_apply_rail` and the HUD's restore iterate.
const RAIL_SLOT_ORDER: Array[int] = [RAIL_SLOT_TOP, RAIL_SLOT_BOTTOM]
## Gap between the two stacked clusters. Its own const rather than a borrowed `HEADER_SEPARATION` /
## `BODY_SEPARATION`: those are the header's and the tab bar's gaps, and this is read by
## `DockRowController._required_height` as part of the stack's measured height.
const RAIL_SLOT_SEPARATION := 8

signal reservation_changed(edge: int, size: float)
signal cycle_requested(delta: int)
## The header subject cluster (stage glyph + name + stage label) was clicked — "jump to my band".
signal subject_activated
## The panel's registered `⚒` was pressed — open the Materials & Crafting panel
## (`.claude/rules/client/crafting-panel.md`).
##
## **IT CARRIES NO SUBJECT, and that is the point of putting it on subject-independent chrome.** ONE
## button serves a band page and the faction page and the band zone's 300px budget is untouched;
## which band it opens on is `BandPanelController`'s answer, not this panel's. It is a RELAY of
## `action_invoked(ACTION_CRAFTING)` — the ⚒ is registered like any other action — kept as its own
## named edge so the crafting controller connects to a signal that says what happened rather than
## filtering an id.
signal crafting_requested
## A registered action was pressed, named by the id it was registered under. THE registry's one
## outbound edge: a caller that registers an action listens here and filters on its own id, and never
## has to know which of the two mounts the press came off.
signal action_invoked(id: StringName)
## `work_zone_size()` changed — a shell flip, dock change, collapse or viewport resize. Hud re-pages
## its work board on this rather than re-rendering everything.
signal zones_resized

var _dock_edge: int = SIDE_LEFT
var _collapsed: bool = false
var _shown: bool = true
## The cross-axis size last published through `reservation_changed`, so `_republish_reservation_if_changed`
## can tell a size the panel merely re-derived from one nobody downstream has been told about. Seeded to a
## value no reservation can take, so the first republish after a declared input arrives is never suppressed
## by a coincidence with an unset member.
var _published_reservation: float = -1.0
# Leading (inboard) offset from the docked edge, pushed by Main = Σ sizes of co-edge reservers
# inboard of this panel (today: the Inspector's strip when both dock left). Keeps co-edge panels
# stacked, not overlapping. Does NOT change what this panel reserves (the map/HUD inset is the
# per-edge SUM), only where its own Control anchors.
var _edge_offset: float = 0.0

# nodes
var _root: Control
var _panel: PanelContainer
var _seam: ColorRect
var _header_full: HBoxContainer
## THE COLLAPSED RAIL. A bare `BoxContainer`, never an `HBox`/`VBox`: its `vertical` flips with the
## dock's orientation (`_apply_header_rail_orientation`), and the two subclasses REFUSE that write
## ("Can't change orientation of VBoxContainer").
var _header_rail: BoxContainer
## The rail's justification spacer, expanding on the LONG axis so the restore button lands at the far
## end of a horizontal rail. Hidden on a vertical one, where a `BoxContainer` charges nothing for a
## hidden child — the same rule the two action mounts rely on.
var _rail_spacer: Control
var _subject_cluster: PanelContainer
var _stage_glyph_label: Label
var _rail_glyph_label: Label
## Bundled-sprite siblings of the two glyph labels. Exactly one of each pair is visible at a time
## (see `set_header`): the sprite when the stage has bundled art, else the emoji label.
var _stage_glyph_sprite: TextureRect
var _rail_glyph_sprite: TextureRect
var _name_label: Label
var _stage_label: Label
## The band's hex coordinates, beside the stage word on the header's second line. IDENTITY, not a
## vital: it answers "which band am I looking at", exactly as the name and the stage do, so it lives
## in the header rather than as a row in the band zone's vitals grid (where it cost that
## height-capped zone a row it could not spare, and rendered only on the map-click path). Hidden when
## the caller passes no coordinates, so an empty value costs no gap.
var _position_label: Label
var _count_label: Label
var _collapse_button: Button
var _rail_expand_button: Button
## THE ACTION BAR — the VERTICAL dock's mount. `_action_bar` is the outer MarginContainer (what is
## hidden, and what `_action_bar_height` measures — the margins are part of the row's cost);
## `_action_row` is the HBox the buttons live in.
var _action_bar: MarginContainer
var _action_row: HBoxContainer
## THE SUBJECT ROW's mount — the HORIZONTAL dock's. It sits between the cycler and the dock chooser,
## so the row's own contents never move; it is HIDDEN when it is not the live mount or holds nothing,
## because a `BoxContainer` skips its separation only around a hidden child.
var _header_action_row: HBoxContainer
## THE COLLAPSED RAIL's mount, between the stage glyph and the justification spacer, so the restore
## button stays at the rail's trailing end however many verbs are registered. A bare `BoxContainer`
## for `_header_rail`'s reason: its `vertical` flips with the rail's.
var _rail_action_row: BoxContainer
## The registered actions in DECLARED order — one `{id, glyph, tooltip, enabled}` descriptor each.
## The live mount is rebuilt from this list, never edited in place, so registration order is the only
## thing that decides the row's order.
var _actions: Array[Dictionary] = []
## id:StringName -> Button, so `refresh_actions` can re-evaluate a predicate without a rebuild.
var _action_buttons: Dictionary = {}
## Which mount the buttons are built into right now, so an ordinary layout pass does not rebuild them
## and a dock change to the OTHER orientation does. `ACTION_MOUNT_NONE` until the first build.
var _action_mount: int = ACTION_MOUNT_NONE
# Body layout: `_body_host` holds the two alternative SHELLS, exactly one visible at a time (chosen by
# panel width — see `_shell_is_wide`). The wide shell is an HBox of one zone host per DECLARED zone
# (the flanks fixed-width, work expanding) with hairline separators; the narrow shell is a tab bar
# over a single zone host. `_zones` holds the Hud-built zone Controls the panel OWNS (freed on the
# next `set_zones` or layout change); `_reparent_zones` homes them into whichever hosts are active, so
# a shell flip needs no Hud re-render. Nothing here measures content — the shells fill a card whose
# size is fixed per dock.
## The card's whole content column (header + body). It always FILLS its card: since issue #377 the CARD
## is the thing sized to its content, and the centring happens one level up on the card as a whole.
var _panel_column: VBoxContainer
## The card's row, holding just the content column. The chrome rail was its trailing cell until issue
## #377 made the two separate islands, so the row is a single-cell wrapper now — see `_build`.
var _card_row: HBoxContainer
## The strip's TRAILING chrome rail (issue #324): one column at the strip's right end holding the HUD's
## bottom-bar chrome stacked vertically — nav cluster on top, turn cluster below — so it shares the
## panel's row instead of stacking against it. A SIBLING of the card under `_root` since issue #377, not
## a cell of `_card_row`. The panel owns the rail, its stack and the two slot HOSTS, and
## NOTHING inside those hosts — contrast `set_zones`, which owns and frees the zone contents it is
## handed. Never add to, read, or free a slot's children here.
## `_rail` is a PLAIN `Control` and that is load-bearing: it blocks the stack's minimum size from
## propagating out to the card, which is what lets everything INSIDE it be ordinary containers.
var _rail: Control
## The rail's two slots, stacked and centred inside it (`_build_rail` states why the centring is a
## container's job and not anchor arithmetic).
var _rail_stack: VBoxContainer
var _rail_slots: Dictionary = {}          # slot:int (RAIL_SLOT_*) -> Control host
## The rail column's width, DECLARED by the HUD (`set_rail_width`) — never measured from the content.
var _rail_declared_width: float = 0.0
## What the card must LEAVE at each end of a horizontal strip, declared by `Main` (`set_lateral_bounds`)
## — the HUD's left and right column widths. An edge has bounds exactly when the HUD does NOT yield its
## strip there (`Main._reserver_overlays_hud`): always on a TOP dock, and on a BOTTOM dock whenever the
## card can afford them (`affords_wide_shell_with_bounds`). When the HUD yields, it has moved out of the
## row entirely and both are 0.
var _bound_leading: float = 0.0
var _bound_trailing: float = 0.0
## How many columns the WORK board wants, DECLARED by `BandPanelController` (`set_work_columns`). It is
## what the card's wide-shell width is built from. Seeded at the maximum so the first layout pass — which
## happens before any controller has counted anything — draws the widest card rather than a one-column
## sliver that then jumps wider.
var _work_columns: int = WORK_MAX_COLUMNS
var _body_host: VBoxContainer
var _wide_shell: HBoxContainer
var _wide_zone_hosts: Dictionary = {}   # zone:StringName -> Control (a plain, clipping zone host)
var _narrow_shell: VBoxContainer
var _tab_bar: HBoxContainer
var _narrow_zone_host: Control
var _body_is_wide: bool = false
var _band_present: bool = false
var _empty_state: Label
## The zone contents the panel currently owns (zone:StringName -> Control). A zone may be absent
## or null → that zone renders empty.
var _zones: Dictionary = {}
## The LIVE zone layout — the subject's ordered descriptors (`set_zone_layout`). It is what the wide
## shell's columns, the tab bar, the fixed-width sum and the shell threshold are all read off, so the
## body has exactly one statement of "which zones are there, in what order, how wide".
var _zone_layout: Array[Dictionary] = DEFAULT_ZONE_LAYOUT
## Narrow-shell tab state: the selected zone key (persisted) and each tab's badge (`{text, hot}`).
var _active_tab: StringName = DEFAULT_TAB
## The WORK board's persisted sort, opaque to this panel. `""` = the player has never chosen one, so
## the controller keeps its own default.
var _work_sort_pref: String = ""
var _tab_badges: Dictionary = {}
## Whether the header's subject cluster offers a map jump (`set_subject_jumpable`). A band does; the
## pinned faction page does not, having no tile.
var _subject_jumpable: bool = true
var _tab_buttons: Dictionary = {}   # zone:StringName -> Control (the tab cell)
## The last `work_zone_size()` reported, so `zones_resized` fires on a real change only.
var _last_work_zone_size: Vector2 = Vector2.ZERO
## The last `band_zone_columns()` reported, beside it — see `_notify_zones_resized` for why the work
## box alone is not enough of a trigger.
var _last_band_columns: int = 0
var _dock_cells: Dictionary = {}   # edge:int -> Button

func _ready() -> void:
	layer = LAYER_INDEX
	_load_prefs()
	_build()
	_apply_dock_layout()
	_refresh_collapse_state()
	_refresh_dock_cells()
	# A window resize changes the T/B panel width (hence the shell) and the clamped wide height, so
	# re-choose the shell and re-report both the reservation and the work-zone box.
	var vp := get_viewport()
	if vp != null:
		vp.size_changed.connect(_on_viewport_resized)
	_notify_zones_resized()

# ---- public API ------------------------------------------------------------

## Push the header subject: settlement stage id (the server's stable key), its emoji glyph
## fallback, display name, stage label, and the band's preformatted hex coordinates. The stage
## renders as bundled art when `StageSprites` has a texture for the id; a stage with no bundled art
## (the config is user-editable) keeps its emoji.
## `position_label` is a preformatted `String` the CALLER resolves (the panel never reads a band
## dict), and `""` renders nothing — the caller could not resolve the coordinates.
func set_header(stage_id: String, glyph: String, subject_name: String, stage_label: String,
		position_label: String = "") -> void:
	var resolved_glyph := glyph if not glyph.is_empty() else DEFAULT_STAGE_GLYPH
	var sprite := StageSprites.for_stage(stage_id)
	_apply_stage_visual(_stage_glyph_label, _stage_glyph_sprite, sprite, resolved_glyph)
	_apply_stage_visual(_rail_glyph_label, _rail_glyph_sprite, sprite, resolved_glyph)
	if _name_label != null:
		_name_label.text = subject_name
	if _stage_label != null:
		_stage_label.text = stage_label
	if _position_label != null:
		_position_label.text = position_label
		_position_label.visible = not position_label.is_empty()

## Update the cycler readout ("index+1 / count"). count <= 0 blanks it.
func set_cycler(index: int, count: int) -> void:
	if _count_label == null:
		return
	if count <= 0:
		_count_label.text = "–"   # en-dash placeholder
	else:
		_count_label.text = "%d / %d" % [index + 1, count]

## Declare the BODY the current subject wants: an ordered list of DESCRIPTORS — one wide-shell column
## and one narrow-shell tab each, in the order given.
##
## **THE SUBJECT NAMES ITS OWN KEYS AND ITS OWN LABELS.** A band declares Band · Work · Parties; the
## faction page declares Faction · Work · Know · Parties. That is what replaced a per-zone label
## OVERRIDE (`set_tab_label`, which existed only to rename `Band` to `Faction`): a tab bar picks a
## ZONE, and a zone's name states the scope its content is at, which is a fact about the subject
## rather than a patch on a default.
##
## **IT MUST BE CALLED BEFORE THE ZONE CONTENTS ARE BUILT, NOT WITH THEM.** The layout is what
## `wide_shell_min_width()` sums over, so declaring it can flip the shell — and the contents are paged
## against `zone_size()`, which the flip moves. A subject that built its zones first and declared the
## layout second would page a board against the previous subject's shell.
##
## **IT REFRESHES THE CACHED ZONE SIZE SILENTLY — no `zones_resized`** — which is `set_work_columns`'
## contract for the same reason: the caller is the controller at the START of a render and is about to
## build every zone against the new box, and emitting would re-enter that render from inside itself.
##
## A repeat declaration early-outs, so a band-to-band cycle costs one Array compare.
func set_zone_layout(specs: Array) -> void:
	var next: Array[Dictionary] = []
	for spec_variant in specs:
		if not (spec_variant is Dictionary):
			continue
		var spec: Dictionary = spec_variant
		if ZONE_KEYS.has(StringName(spec.get(ZONE_SPEC_KEY, &""))):
			next.append(spec)
	if next.is_empty() or next == _zone_layout:
		return
	# The previous subject's contents belong to the previous subject's HOSTS, which are about to be
	# freed — so they are dropped here rather than left to `set_zones`, whose free would otherwise run
	# after their parents had gone.
	_free_zones()
	_zone_layout = next
	_rebuild_wide_shell()
	_rebuild_tab_bar()
	_apply_dock_layout()
	_last_work_zone_size = work_zone_size()

## Hand the panel the zone CONTENTS for the layout it was just given, keyed by zone. The panel takes
## OWNERSHIP (frees the previous set) and parents them into whichever shell is active. A zone the
## dictionary omits (or maps to null) renders empty.
##
## **A CONTENT FOR AN UNDECLARED ZONE IS FREED, NOT DROPPED.** Ownership passes on the call, so
## silently ignoring it would leak a whole zone's control tree — which is exactly what a subject that
## built four zones and declared three would do, once per render.
func set_zones(contents: Dictionary) -> void:
	_free_zones()
	var declared := _zone_order()
	var any := false
	for key in contents:
		var content_variant: Variant = contents[key]
		if not (content_variant is Control):
			continue
		if declared.has(StringName(key)):
			_zones[StringName(key)] = content_variant
			any = true
		else:
			(content_variant as Node).queue_free()
	if not any:
		set_band_present(false)
		return
	_band_present = true
	if _empty_state != null:
		_empty_state.visible = false
	if not _body_is_wide:
		_rebuild_tab_bar()   # which zones exist can move `_effective_tab`, hence the highlight
	_reparent_zones()
	_update_body_visibility()

## The box a ZONE's content may fill, in canvas px — the zone's INTERIOR, after the panel's own
## chrome (card border + content margins + header, and in the narrow shell the tab bar). Purely a
## function of the layout, the dock edge, the collapse state and the window; it never consults the
## content.
##
## **KEYED, not one accessor per zone.** Every wide-shell zone shares the card's one body HEIGHT and
## differs only in WIDTH — a fixed flank states its own, and the expanding zone takes what the flanks
## and separators leave — so there is one answer with one parameter rather than a named function per
## zone that a fourth zone would have to add a fifth of. The NARROW shell hands its one zone the whole
## interior, so every key answers the same box there.
func zone_size(zone: StringName) -> Vector2:
	if _collapsed or not _shown:
		return Vector2.ZERO
	var interior := _interior_size()
	# The action bar is card chrome in both SHELLS (unlike the tab bar), so it comes off here rather
	# than out of `_shell_chrome_height` — and it measures 0 on a horizontal dock, where the actions
	# ride the subject row and `_header_height()` has already counted them.
	var body_height: float = maxf(interior.y - _header_height() - _action_bar_height(), 0.0)
	if not _shell_is_wide():
		return Vector2(interior.x, maxf(body_height - _tab_bar_height(), 0.0))
	# A FIXED zone's width is its declared column times however many columns it was GRANTED, so this is
	# also the answer for the band flank once it widens — there is no second accessor for that.
	var fixed := _zone_fixed_width(zone)
	if fixed > 0.0:
		return Vector2(fixed, body_height)
	# The card is built UP from the declared column count (issue #377), so its interior is the fixed
	# flanks + the separators + the board — no cap to clamp against, and this comes back as exactly
	# `_work_columns × ZONE_WORK_MIN_WIDTH`. The `max` still guards the clamped-card case, where a
	# window narrower than the content leaves less than the flanks alone want.
	return Vector2(maxf(interior.x - _fixed_zone_span() - _wide_separator_span(), 0.0), body_height)

## The WORK zone's box — the one `zones_resized` reports and Hud pages its board against. A named
## reader of `zone_size` rather than a second answer: this zone is the expanding one, so its width is
## the only one that moves with the card, which makes it the box worth watching.
func work_zone_size() -> Vector2:
	return zone_size(ZONE_WORK)

## The CARD's global rect — the island the strip holds, not the strip (`_root`) itself. Published for
## the free-floating compose card, which anchors itself to the card's map-facing edge and must never
## overlap it; every other reader of this geometry lives inside this file. See `_position_card_and_rail`
## for why the two rects stopped being the same one.
func card_rect() -> Rect2:
	return _panel.get_global_rect() if _panel != null else Rect2()

## Declare whether the header's subject cluster is a "jump to it on the map" affordance.
##
## **A FACTION HAS NO TILE**, so the pinned faction page (issue #450) turns this off: the cluster stops
## taking the mouse (hence no hover tint and no `subject_activated`), drops the pointing-hand cursor and
## drops the tooltip that promises a jump. Leaving it live and no-oping the handler would have left a
## header that offers a jump, lights up under the pointer and then does nothing — the worst of the
## three states.
##
## The emit is gated on the flag as well as on the filter, so the rule reads at the emit rather than
## depending on a `mouse_filter` value set 300 lines away.
func set_subject_jumpable(jumpable: bool) -> void:
	if jumpable == _subject_jumpable:
		return
	_subject_jumpable = jumpable
	if _subject_cluster == null:
		return
	_subject_cluster.mouse_filter = Control.MOUSE_FILTER_STOP if jumpable else Control.MOUSE_FILTER_IGNORE
	_subject_cluster.mouse_default_cursor_shape = \
		Control.CURSOR_POINTING_HAND if jumpable else Control.CURSOR_ARROW
	_subject_cluster.tooltip_text = SUBJECT_JUMP_TOOLTIP if jumpable else ""
	# An IGNORE cluster receives no `mouse_exited`, so a hover latched at the moment of the swap would
	# never be cleared and the header would keep its tint for the rest of the session.
	if not jumpable:
		_set_subject_hover(false)

## A tab's label — the word the SUBJECT declared for that zone (`set_zone_layout`). There is no
## default and no override: a zone not in the live layout has no tab to label.
func _tab_label_text(zone: StringName) -> String:
	for spec in _zone_layout:
		if StringName(spec.get(ZONE_SPEC_KEY, &"")) == zone:
			return String(spec.get(ZONE_SPEC_LABEL, ""))
	return ""

## Push a tab's badge (narrow shell only; ignored in the wide shell, which has no tab bar).
## `hot` tints it WARN. An empty `text` clears the badge.
##
## Keyed on `ZONE_KEYS` rather than on the LIVE layout, so a subject may push its badges in any order
## relative to its layout; the tab bar iterates the layout, so a badge for a zone that is not on it
## simply never renders.
func set_tab_badge(zone: StringName, text: String, hot: bool) -> void:
	if not ZONE_KEYS.has(zone):
		return
	_tab_badges[zone] = {"text": text, "hot": hot}
	if not _body_is_wide:
		_rebuild_tab_bar()

## Toggle between the band-detail content and the empty-state placeholder. `false` also frees any
## owned zones (no band → nothing to show).
func set_band_present(present: bool) -> void:
	_band_present = present
	if not present:
		_free_zones()
	if _empty_state != null:
		_empty_state.visible = not present
	_update_body_visibility()

## Free (and detach) the zone contents from the previous render. Ownership is unambiguous: the panel
## owns exactly what it was last handed, and drops it here before taking the next set.
func _free_zones() -> void:
	for key in _zones:
		var zone_variant: Variant = _zones[key]
		if zone_variant is Node:
			_detach(zone_variant)
			(zone_variant as Node).queue_free()
	_zones.clear()

## Dock the panel to an edge (a Godot SIDE_* const). Re-anchors, persists, and
## re-emits the reservation so the map + HUD reflow.
func set_dock(edge: int) -> void:
	if not DOCK_EDGES.has(edge):
		return
	if edge == _dock_edge:
		return
	_dock_edge = edge
	# BEFORE the layout: the two orientations mount the registered actions on different rows, and both
	# the header's height and the bar's feed the cross-axis size this then re-emits.
	_refresh_action_mount()
	_apply_dock_layout()
	_refresh_dock_cells()
	_save_prefs()
	_emit_reservation()
	_notify_zones_resized()

func get_dock() -> int:
	return _dock_edge

## Set the leading (inboard) offset from the docked edge so this panel stacks outboard of any
## co-edge reserver (Main computes it = Σ sizes of inboard co-edge reservers). Re-anchors only;
## does NOT re-emit the reservation (the size this panel reserves is unchanged).
func set_edge_offset(px: float) -> void:
	var offset: float = maxf(px, 0.0)
	if is_equal_approx(offset, _edge_offset):
		return
	_edge_offset = offset
	_apply_dock_layout()

## Rail the panel to a thin strip (or restore it). Persists + re-emits the
## reservation so the map + HUD reflow to the collapsed size.
func set_collapsed(collapsed: bool) -> void:
	if collapsed == _collapsed:
		return
	_collapsed = collapsed
	# BEFORE the layout, exactly as `set_dock` does it: collapsing moves the registered actions to the
	# rail and expanding moves them back to the orientation's own mount, and both the bar's height and
	# the subject row's feed the cross-axis size the layout then re-emits.
	_refresh_action_mount()
	_refresh_collapse_state()
	_apply_dock_layout()
	_save_prefs()
	_emit_reservation()
	_notify_zones_resized()

func is_collapsed() -> bool:
	return _collapsed

## Show/hide the panel; hiding releases its reserved strip (slice 3 gates this on
## band selection). Emits the reservation change.
func set_shown(shown: bool) -> void:
	if shown == _shown:
		return
	_shown = shown
	if _root != null:
		_root.visible = shown
	_emit_reservation()
	_notify_zones_resized()

## The strip the panel currently reserves (0 hidden, COLLAPSED_SIZE collapsed,
## else the cross-axis size). Main queries this to seed the initial reservation.
func current_reservation_size() -> float:
	if not _shown:
		return 0.0
	return _cross_axis_size()

# ---- construction ----------------------------------------------------------

func _build() -> void:
	_root = Control.new()
	_root.name = "PanelRoot"
	_root.visible = _shown
	# **THE LAYOUT REGION IS TRANSPARENT TO THE POINTER** (issue #377) — the exact OPPOSITE call from
	# `EventDockPanel._build`, and for the same mechanism read the other way round. `_root` spans the
	# WHOLE reserved strip (`_apply_root_anchors`), but since the card became a floating island most of
	# that strip is LIVE MAP. `MapView` picks hexes out of `_unhandled_input`, and a `STOP` control under
	# the pointer — the `Control` default — makes the Viewport mark the press handled before it ever gets
	# there. Without `IGNORE` the ~1929px of open map either side of the card on a 3440 bottom dock can be
	# neither clicked to select a hex, nor right/middle-dragged to pan, nor wheel-zoomed. The two things
	# that must still eat their own clicks are `STOP` in their own right below — the card and the chrome
	# cluster — so this costs the islands nothing; an `IGNORE` parent does not stop its children being
	# picked. A vertical dock is unaffected either way: the card fills that strip edge to edge.
	_root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_root)

	_panel = PanelContainer.new()
	_panel.name = "PanelCard"
	_panel.add_theme_stylebox_override("panel", panel_card_stylebox())
	_panel.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	# The card DOES eat its own clicks — a press on the panel must never also select the hex behind it.
	# `STOP` is the `Control` default, set explicitly because with `_root` on `IGNORE` it is the surface
	# that carries the whole claim, exactly as `EventDockPanel` sets its own card's filter.
	_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	_root.add_child(_panel)

	# The card's row. It has exactly ONE child — the content column — since issue #377 moved the chrome
	# rail out to be a sibling of the card rather than this row's trailing cell. The HBox is kept because
	# the column's fill/expand flags and the card's inner padding are stated through it, and because a
	# second cell inside the card is a thing this row can grow again without a structural change.
	_card_row = HBoxContainer.new()
	_card_row.name = "CardRow"
	# `ZONE_SEPARATION`, the same gutter the wide shell puts either side of a zone separator. With one
	# child it costs nothing today; it is here so a second cell would be spaced like every other region.
	_card_row.add_theme_constant_override("separation", ZONE_SEPARATION)
	_card_row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_card_row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_panel.add_child(_card_row)

	var column := VBoxContainer.new()
	column.name = "PanelColumn"
	column.add_theme_constant_override("separation", COLUMN_SEPARATION)
	# The column simply FILLS its card — the card is the thing that narrows now (`_card_width`), and the
	# centring happens one level up on the whole card (`_position_card_and_rail`). Set once, at
	# construction: no layout pass changes it.
	column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	column.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_card_row.add_child(column)
	_panel_column = column

	_build_rail()

	_header_full = _build_header_full()
	column.add_child(_header_full)

	_header_rail = _build_header_rail()
	column.add_child(_header_rail)

	# TITLE -> ACTIONS -> TABS -> CONTENT, on a VERTICAL dock (a horizontal one mounts its actions on
	# the title row itself and this bar takes no height). The bar acts on the SUBJECT whichever view is
	# showing, so it sits above the tab strip; the tabs select a view and must stay adjacent to the
	# content they switch (the strip is built inside `_narrow_shell`, one level down).
	_action_bar = _build_action_bar()
	column.add_child(_action_bar)

	# The body host holds both alternative shells + the empty-state; only one shell is visible at a
	# time. Collapse hides the whole host.
	_body_host = VBoxContainer.new()
	_body_host.name = "BandBodyHost"
	_body_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body_host.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(_body_host)

	# Empty state (shown only when no band is resolved — the panel otherwise hides outright when
	# there are zero player bands). First body child so it occupies the body when no band is present.
	_empty_state = Label.new()
	_empty_state.text = BODY_EMPTY_TEXT
	_empty_state.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_empty_state.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_empty_state.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body_host.add_child(_empty_state)

	# WIDE shell: every zone side by side, the flanks fixed-width, work taking the rest, hairline
	# separators between. No tab bar — every zone is visible at once. Its columns are built from the
	# LIVE layout, so a subject with a fourth zone needs no structural change here.
	_wide_shell = HBoxContainer.new()
	_wide_shell.name = "WideShell"
	_wide_shell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_wide_shell.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_wide_shell.add_theme_constant_override("separation", ZONE_SEPARATION)
	_wide_shell.visible = false
	_body_host.add_child(_wide_shell)
	_rebuild_wide_shell()

	# NARROW shell: a tab bar directly under the header + exactly one zone filling the rest.
	_narrow_shell = VBoxContainer.new()
	_narrow_shell.name = "NarrowShell"
	_narrow_shell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_narrow_shell.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_narrow_shell.add_theme_constant_override("separation", BODY_SEPARATION)
	_narrow_shell.visible = false
	_body_host.add_child(_narrow_shell)
	_tab_bar = HBoxContainer.new()
	_tab_bar.name = "ZoneTabs"
	_tab_bar.add_theme_constant_override("separation", TAB_SEPARATION)
	_narrow_shell.add_child(_tab_bar)
	_narrow_zone_host = _make_zone_host("NarrowZoneHost", 0.0)
	_narrow_shell.add_child(_narrow_zone_host)
	_rebuild_tab_bar()

	# The accent seam sits on the map-facing edge, above the card fill.
	_seam = ColorRect.new()
	_seam.name = "AccentSeam"
	_seam.color = HudStyle.SIGNAL_DEEP
	_seam.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(_seam)

	# **THE ⚒ GOES THROUGH THE REGISTRY, exactly as any other action would** — and so it lands on
	# whichever mount the dock calls for, with no branch of its own. The panel's own
	# launcher is not privileged: it is a `register_action` call like the ones a caller makes, and its
	# `crafting_requested` edge is a RELAY of `action_invoked`. Registering it here (rather than letting
	# the crafting controller do it) keeps the button's presence a property of the panel — the ⚒ is
	# subject-independent chrome that must exist on a band page and on the faction page alike.
	register_action(ACTION_CRAFTING, CRAFTING_GLYPH, CRAFTING_TOOLTIP)
	action_invoked.connect(_on_action_invoked)

func _build_header_full() -> HBoxContainer:
	var header := HBoxContainer.new()
	header.name = "HeaderFull"
	header.add_theme_constant_override("separation", HEADER_SEPARATION)

	# The subject cluster (stage glyph + name + stage label) is a clickable "jump to my band"
	# affordance: a PanelContainer (STOP + hand cursor + subtle hover tint) wrapping a
	# mouse-transparent HBox so a click anywhere on it reaches `_on_subject_gui_input`. It expands to
	# fill (pushing the cycler/dock-chooser right, as the plain subject VBox used to).
	_subject_cluster = PanelContainer.new()
	_subject_cluster.name = "SubjectCluster"
	_subject_cluster.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_subject_cluster.mouse_filter = Control.MOUSE_FILTER_STOP
	_subject_cluster.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	_subject_cluster.tooltip_text = SUBJECT_JUMP_TOOLTIP
	_subject_cluster.add_theme_stylebox_override("panel", _subject_stylebox(false))
	_subject_cluster.gui_input.connect(_on_subject_gui_input)
	_subject_cluster.mouse_entered.connect(func(): _set_subject_hover(true))
	_subject_cluster.mouse_exited.connect(func(): _set_subject_hover(false))

	var cluster_row := HBoxContainer.new()
	cluster_row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	cluster_row.add_theme_constant_override("separation", HEADER_SEPARATION)
	_subject_cluster.add_child(cluster_row)

	_stage_glyph_label = Label.new()
	_stage_glyph_label.add_theme_font_size_override("font_size", STAGE_GLYPH_FONT_SIZE)
	_stage_glyph_label.text = DEFAULT_STAGE_GLYPH
	_stage_glyph_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	_stage_glyph_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	cluster_row.add_child(_stage_glyph_label)

	_stage_glyph_sprite = _make_stage_glyph_sprite()
	cluster_row.add_child(_stage_glyph_sprite)

	var subject := VBoxContainer.new()
	subject.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	subject.add_theme_constant_override("separation", 0)
	subject.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_name_label = Label.new()
	_name_label.add_theme_font_size_override("font_size", NAME_FONT_SIZE)
	_name_label.add_theme_color_override("font_color", HudStyle.INK)
	_name_label.text = ""
	_name_label.clip_text = true
	_name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_stage_label = Label.new()
	_stage_label.add_theme_font_size_override("font_size", STAGE_LABEL_FONT_SIZE)
	_stage_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_stage_label.text = ""
	_stage_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	# The stage word and the band's coordinates share the header's second line: both are secondary
	# IDENTITY, so they wear the same quiet ink and size, and the coordinates sit AFTER the stage
	# (a band is "Camp" first and "at (68, 30)" second).
	var stage_row := HBoxContainer.new()
	stage_row.add_theme_constant_override("separation", STAGE_ROW_SEPARATION)
	stage_row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_position_label = Label.new()
	_position_label.add_theme_font_size_override("font_size", STAGE_LABEL_FONT_SIZE)
	_position_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_position_label.text = ""
	_position_label.visible = false
	_position_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	stage_row.add_child(_stage_label)
	stage_row.add_child(_position_label)
	subject.add_child(_name_label)
	subject.add_child(stage_row)
	cluster_row.add_child(subject)

	header.add_child(_subject_cluster)

	header.add_child(_build_cycler())

	# **THE HORIZONTAL DOCK'S ACTION MOUNT**, between the cycler and the WINDOW controls — so the row's
	# own contents (subject, cycler, dock chooser, collapse) sit exactly where they always did whether
	# or not it is the live mount. It is hidden when it is not, and when it holds nothing: a
	# `BoxContainer` skips its separation only around a HIDDEN child, so an empty-but-visible row would
	# quietly charge the subject row one `HEADER_SEPARATION` of the width this whole seam protects.
	_header_action_row = HBoxContainer.new()
	_header_action_row.name = "HeaderActionRow"
	_header_action_row.add_theme_constant_override("separation", ACTION_BAR_SEPARATION)
	_header_action_row.visible = false
	header.add_child(_header_action_row)

	var dock_chooser := _build_dock_chooser()
	header.add_child(dock_chooser)

	_collapse_button = _make_icon_button(COLLAPSE_GLYPH, "Collapse")
	_collapse_button.pressed.connect(_on_collapse_pressed)
	header.add_child(_collapse_button)

	return header

## Subject-cluster background: transparent normally, a subtle SIGNAL_WASH tint on hover. Same
## content margins in both states so hovering never shifts the header.
func _subject_stylebox(hover: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.SIGNAL_WASH if hover else Color(0.0, 0.0, 0.0, 0.0)
	sb.set_corner_radius_all(SUBJECT_HOVER_CORNER_RADIUS)
	sb.content_margin_left = SUBJECT_HOVER_PADDING_H
	sb.content_margin_right = SUBJECT_HOVER_PADDING_H
	sb.content_margin_top = SUBJECT_HOVER_PADDING_V
	sb.content_margin_bottom = SUBJECT_HOVER_PADDING_V
	return sb

func _set_subject_hover(hover: bool) -> void:
	if _subject_cluster != null:
		_subject_cluster.add_theme_stylebox_override("panel", _subject_stylebox(hover))

## Left-click anywhere on the subject cluster → "jump to my band". Silent while the subject is not
## jumpable (the faction page has no tile) — an `IGNORE` cluster gets no input anyway, and the gate is
## here so the rule reads where the promise is made.
func _on_subject_gui_input(event: InputEvent) -> void:
	if not _subject_jumpable:
		return
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
		subject_activated.emit()

func _build_cycler() -> HBoxContainer:
	var cycler := HBoxContainer.new()
	cycler.name = "Cycler"
	cycler.add_theme_constant_override("separation", 4)

	var prev := _make_icon_button(CYCLE_PREV_GLYPH, "Previous settlement")
	prev.pressed.connect(func(): _on_cycle_pressed(CYCLE_PREV))
	cycler.add_child(prev)

	_count_label = Label.new()
	_count_label.add_theme_font_size_override("font_size", COUNT_FONT_SIZE)
	_count_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_count_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_count_label.custom_minimum_size = Vector2(COUNT_MIN_WIDTH, 0.0)
	_count_label.text = "–"
	cycler.add_child(_count_label)

	var nxt := _make_icon_button(CYCLE_NEXT_GLYPH, "Next settlement")
	nxt.pressed.connect(func(): _on_cycle_pressed(CYCLE_NEXT))
	cycler.add_child(nxt)

	return cycler

func _build_dock_chooser() -> GridContainer:
	var grid := GridContainer.new()
	grid.name = "DockChooser"
	grid.columns = 2
	grid.add_theme_constant_override("h_separation", DOCK_CELL_SEPARATION)
	grid.add_theme_constant_override("v_separation", DOCK_CELL_SEPARATION)
	for edge in DOCK_EDGES:
		var cell := Button.new()
		cell.custom_minimum_size = Vector2(DOCK_CELL_SIZE, DOCK_CELL_SIZE)
		cell.focus_mode = Control.FOCUS_NONE
		cell.tooltip_text = "Dock %s" % _edge_name(edge)
		cell.pressed.connect(func(): set_dock(edge))
		_dock_cells[edge] = cell
		grid.add_child(cell)
	return grid

## **THE COLLAPSED RAIL RUNS ALONG THE DOCK'S PLENTIFUL AXIS** — the action registry's two mount
## points, one level up. A left/right rail is tall and narrow, so the glyph and the restore button
## STACK; a top/bottom rail is `COLLAPSED_SIZE` tall and a screen wide, so they share ONE LINE with
## the button justified to the trailing end, exactly where the expanded header keeps its window
## controls. Built horizontal and re-oriented by `_apply_header_rail_orientation` before the first
## layout pass.
func _build_header_rail() -> BoxContainer:
	var rail := BoxContainer.new()
	rail.name = "HeaderRail"
	rail.alignment = BoxContainer.ALIGNMENT_CENTER
	rail.add_theme_constant_override("separation", HEADER_SEPARATION)

	_rail_glyph_label = Label.new()
	_rail_glyph_label.add_theme_font_size_override("font_size", STAGE_GLYPH_FONT_SIZE)
	_rail_glyph_label.text = DEFAULT_STAGE_GLYPH
	_rail_glyph_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	# Centred on the cross axis of a HORIZONTAL rail, where the glyph's line box is taller than the
	# rail's interior — the same treatment the expanded header's glyph gets beside its icon controls.
	_rail_glyph_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	rail.add_child(_rail_glyph_label)

	_rail_glyph_sprite = _make_stage_glyph_sprite()
	rail.add_child(_rail_glyph_sprite)

	# The registry's THIRD mount, next to the subject the same way the other two are — the glyph is all
	# the subject a collapsed rail shows. Hidden when it is not the live mount or holds nothing.
	_rail_action_row = BoxContainer.new()
	_rail_action_row.name = "RailActionRow"
	_rail_action_row.alignment = BoxContainer.ALIGNMENT_CENTER
	# Centred on BOTH axes for the restore button's reason: the rail's interior is narrower than one
	# icon square on a vertical dock, and a row that filled it instead would seat its buttons a couple
	# of pixels off the centre line the glyph and the restore toggle sit on.
	_rail_action_row.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	_rail_action_row.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_rail_action_row.add_theme_constant_override("separation", ACTION_BAR_SEPARATION)
	_rail_action_row.visible = false
	rail.add_child(_rail_action_row)

	_rail_spacer = Control.new()
	_rail_spacer.name = "RailSpacer"
	_rail_spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_rail_spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	rail.add_child(_rail_spacer)

	_rail_expand_button = _make_icon_button(EXPAND_GLYPH, "Expand")
	# Centred on the cross axis for the glyph's reason, and in the vertical rail so the button never
	# stretches past the icon square it is styled as.
	_rail_expand_button.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	_rail_expand_button.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_rail_expand_button.pressed.connect(_on_collapse_pressed)
	rail.add_child(_rail_expand_button)

	rail.visible = false
	return rail

## Point the collapsed rail along the dock's PLENTIFUL axis. **The two orientations have OPPOSITE
## SCARCE AXES**, so a control stacked down the rail is right on a vertical dock and unreachable on a
## horizontal one: a T/B rail is only `COLLAPSED_SIZE` (46px) tall, less the card's own 22px of
## vertical chrome, which one `ICON_BUTTON_SIZE` button fills on its own — stacking the stage glyph
## above it pushed the button clean off the bottom of the card, i.e. off the screen edge on a bottom
## dock, leaving no way back from a collapsed panel.
##
## The spacer is what justifies the button to the trailing end, and it is HIDDEN rather than
## zero-sized on a vertical rail: a `BoxContainer` skips its separation only around a hidden child, so
## a visible-but-empty spacer would charge the vertical rail a `HEADER_SEPARATION` gap it draws
## nothing in.
func _apply_header_rail_orientation() -> void:
	if _header_rail == null:
		return
	var vertical := _is_vertical_edge(_dock_edge)
	_header_rail.vertical = vertical
	# The rail's action mount runs the SAME way the rail does — the verbs grow along the plentiful axis
	# in both orientations, which is what keeps a growing registry off the 46px cross axis.
	if _rail_action_row != null:
		_rail_action_row.vertical = vertical
	if _rail_spacer != null:
		_rail_spacer.visible = not vertical

# ---- the action registry ---------------------------------------------------
#
# **IT IS A REGISTRY, NOT A LAYOUT, AND THAT IS THE WHOLE POINT OF IT.** Actions register — id,
# glyph, tooltip, an enabled predicate — and the panel decides where they are drawn, so a new action
# is a one-line entry that no caller has to think about the panel's geometry to add. Same shape as the
# reserved-edge registry, and for the same reason.
#
# **ONE REGISTRY, THREE MOUNT POINTS, CHOSEN BY THE PANEL'S OWN STATE.** The two docks have OPPOSITE
# scarce axes. On a vertical dock the width is a fixed `PANEL_WIDTH` and the subject row's minimum is
# the subject plus every control on it — so an action there makes the card's floor a function of its
# CHROME — while the card's height is the window's and a row of its own is nearly free: the actions go
# on the BAR. On a horizontal dock that is reversed — width is a whole monitor and the strip's HEIGHT
# is what the panel reserves off the map, so a 44px row is real map given up while the subject row has
# width to spare: the actions go on the SUBJECT ROW, and the bar takes zero height there.
#
# **A COLLAPSED PANEL KEEPS ITS VERBS** — the third mount. Both of the mounts above go with the
# chrome that carries them when the panel rails, and a rail showing only a glyph and a restore toggle
# makes collapsing an all-or-nothing choice between the map and the band's actions. On the rail they
# run along whichever axis the rail runs along, so they are free on the axis `COLLAPSED_SIZE` binds.

## Register an action. Re-registering a live id REPLACES its descriptor and keeps its place in the
## row, so a caller may restate one without duplicating the button. **WHERE it renders is not part of
## this contract** — the panel mounts it on the subject row, on the bar or on the collapsed rail per
## its own orientation and collapse state, and re-homes it when either changes.
##
## - `id` — the stable key the press comes back on (`action_invoked`), and the handle
##   `unregister_action` takes.
## - `glyph` / `tooltip` — the face. Built with `_make_icon_button`, the same builder the collapse
##   toggle and the cycler arrows use, so every action reads as a member of the panel's icon family.
## - `enabled` — a zero-argument `Callable` answering `bool`, re-asked by `refresh_actions()`. An
##   EMPTY Callable means always enabled; a predicate is never called during layout, only when the
##   caller says the world moved, so the bar's geometry can never become a function of band state.
##
## Registration can move the card's chrome — the bar's height on a vertical dock, the subject row's on
## a horizontal one — and a dock's cross-axis size IS its reservation, so this republishes it, the
## `set_rail_width` contract. It is a DECLARED input, made at wiring time and not per snapshot, so it
## cannot put the reservation on the render's hot path.
func register_action(id: StringName, glyph: String, tooltip: String,
		enabled: Callable = Callable()) -> void:
	if id.is_empty():
		return
	var spec := {
		ACTION_SPEC_ID: id,
		ACTION_SPEC_GLYPH: glyph,
		ACTION_SPEC_TOOLTIP: tooltip,
		ACTION_SPEC_ENABLED: enabled,
	}
	var at := _action_index(id)
	if at >= 0:
		_actions[at] = spec
	else:
		_actions.append(spec)
	_apply_action_registry()

## Retire an action. Silent on an id that was never registered — a caller tearing down does not have
## to remember what it managed to register.
func unregister_action(id: StringName) -> void:
	var at := _action_index(id)
	if at < 0:
		return
	_actions.remove_at(at)
	_apply_action_registry()

## Re-ask every registered `enabled` predicate. The CALLER's cue, not the panel's: the panel has no
## idea when a band's stores changed, and a predicate asked from `_process` or from a layout pass
## would make the bar's state (and, through a disabled face's own minimum, its geometry) content-driven.
func refresh_actions() -> void:
	for spec in _actions:
		var button_variant: Variant = _action_buttons.get(spec[ACTION_SPEC_ID])
		if not (button_variant is Button):
			continue
		var button: Button = button_variant
		button.disabled = not _action_is_enabled(spec)

## Is `id` REGISTERED? A question about the list, never about which mount is drawing it. For callers
## that register conditionally, and for the harness.
func has_action(id: StringName) -> bool:
	return _action_index(id) >= 0

func _action_index(id: StringName) -> int:
	for i in range(_actions.size()):
		if StringName(_actions[i].get(ACTION_SPEC_ID, &"")) == id:
			return i
	return -1

## An empty predicate means "always" — the common case, so a caller with no gate writes nothing.
func _action_is_enabled(spec: Dictionary) -> bool:
	var enabled: Variant = spec.get(ACTION_SPEC_ENABLED, Callable())
	if not (enabled is Callable) or not (enabled as Callable).is_valid():
		return true
	return bool((enabled as Callable).call())

## The bar's row, empty at construction. **An empty bar takes NO vertical space** — the outer
## MarginContainer is hidden, and a hidden child contributes neither its own height nor the column's
## separation — so a panel with no actions pays nothing for the seam existing, and neither does ANY
## horizontal dock, where the bar is never the live mount.
func _build_action_bar() -> MarginContainer:
	var bar := MarginContainer.new()
	bar.name = "ActionBar"
	bar.add_theme_constant_override("margin_top", ACTION_BAR_MARGIN_V)
	bar.add_theme_constant_override("margin_bottom", ACTION_BAR_MARGIN_V)
	bar.visible = false
	_action_row = HBoxContainer.new()
	_action_row.name = "ActionRow"
	_action_row.add_theme_constant_override("separation", ACTION_BAR_SEPARATION)
	bar.add_child(_action_row)
	return bar

## Rebuild the live mount from `_actions`, then republish the reservation the new chrome may have moved.
func _apply_action_registry() -> void:
	_rebuild_action_mount()
	if _root == null:
		return   # registered before `_build` ran: `_ready` lays the panel out itself
	_apply_dock_layout()
	_republish_reservation_if_changed()

## Which mount the panel's STATE calls for — collapse first, then the dock's orientation. The one
## place the three are chosen between, so nothing else in the panel tests an edge or a collapse flag
## to answer "where do the verbs go".
func _action_mount_for_state() -> int:
	if _collapsed:
		return ACTION_MOUNT_RAIL
	return ACTION_MOUNT_BAR if _is_vertical_edge(_dock_edge) else ACTION_MOUNT_SUBJECT_ROW

## RE-HOME the actions if the panel's state now calls for a different mount. Called on a dock change
## and on a collapse, so moving a panel from the left edge to the top — or railing it — moves the
## glyphs with no reload; a no-op when the answer is unchanged, which is what keeps an ordinary layout
## pass from rebuilding (and so re-disabling) every button.
func _refresh_action_mount() -> void:
	if _action_mount_for_state() == _action_mount:
		return
	_rebuild_action_mount()

## The buttons, in declared order, in whichever row the dock calls for. Rebuilt wholesale rather than
## patched: the row is a handful of icon buttons, and a rebuild is the one arrangement in which the
## row's order cannot drift from the registry's — and the one that cannot leave a button behind on the
## mount the panel just moved off.
func _rebuild_action_mount() -> void:
	if _action_row == null or _header_action_row == null or _rail_action_row == null:
		return
	_action_mount = _action_mount_for_state()
	_clear_children(_action_row)
	_clear_children(_header_action_row)
	_clear_children(_rail_action_row)
	_action_buttons.clear()
	var host: BoxContainer = _action_host_for(_action_mount)
	for spec in _actions:
		var id := StringName(spec[ACTION_SPEC_ID])
		var button := _make_icon_button(String(spec[ACTION_SPEC_GLYPH]), String(spec[ACTION_SPEC_TOOLTIP]))
		button.disabled = not _action_is_enabled(spec)
		button.pressed.connect(func(): action_invoked.emit(id))
		host.add_child(button)
		_action_buttons[id] = button
	_refresh_action_mount_visibility()

## The row a mount id names. Beside `_action_mount_for_state`, so the choice and the hosts it chooses
## between cannot drift apart.
func _action_host_for(mount: int) -> BoxContainer:
	match mount:
		ACTION_MOUNT_BAR:
			return _action_row
		ACTION_MOUNT_RAIL:
			return _rail_action_row
		_:
			return _header_action_row

func _clear_children(host: Node) -> void:
	for child in host.get_children():
		host.remove_child(child)
		child.queue_free()

## **A MOUNT THAT IS NOT CARRYING THE ACTIONS IS HIDDEN, NOT MERELY EMPTY** — a `BoxContainer` skips
## its separation only around a HIDDEN child, so an empty-but-visible row costs its parent a gap it
## draws nothing in: the bar would take a slice of the strip on every horizontal dock, and the header
## row a slice of the width the seam exists to protect, and the rail a gap between its glyph and its
## restore toggle. **The collapse test is inside the MOUNT, not repeated here**: a collapsed panel's
## live mount is the rail, so the other two answer false without a second reading of `_collapsed`.
func _refresh_action_mount_visibility() -> void:
	var carrying: bool = not _actions.is_empty()
	if _action_bar != null:
		_action_bar.visible = carrying and _action_mount == ACTION_MOUNT_BAR
	if _header_action_row != null:
		_header_action_row.visible = carrying and _action_mount == ACTION_MOUNT_SUBJECT_ROW
	if _rail_action_row != null:
		_rail_action_row.visible = carrying and _action_mount == ACTION_MOUNT_RAIL

## The bar's own contribution to the card's chrome — its MARGINS INCLUDED, since they are part of what
## the row costs. Zero while it is hidden, which is what makes an unregistered bar free and what makes
## a horizontal dock pay nothing at all for the registry (its actions ride the subject row, whose
## height `_header_height()` already measures).
func _action_bar_height() -> float:
	if _action_bar == null or not _action_bar.visible:
		return 0.0
	return _action_bar.get_combined_minimum_size().y

## The ⚒'s relay. The registry's outbound edge is `action_invoked(id)`; `crafting_requested` is a
## named alias of the one entry this panel registers itself, so `BandPanelController` connects to a
## signal that names the act rather than filtering ids it did not register.
func _on_action_invoked(id: StringName) -> void:
	if id == ACTION_CRAFTING:
		crafting_requested.emit()

# ---- layout ----------------------------------------------------------------

func _apply_dock_layout() -> void:
	# FIRST, and that is load-bearing: the collapsed strip is sized from the header rail's own minimum
	# (`_collapsed_cross_axis_size`), so anchoring before the rail has been re-pointed reserves the
	# OTHER orientation's rail — measured, a bottom dock reserved 128px (the tall rail's stacked
	# minimum) while reporting the 56 it had re-measured by the time anyone asked.
	_apply_header_rail_orientation()
	_apply_root_anchors()
	# BEFORE `_relayout_body`: a dock change can retire the rail (a vertical strip has none), and the
	# shell is chosen from the width the rail leaves.
	_apply_rail()
	_relayout_body()

## Re-anchor `_root` to the active edge at the current cross-axis size, and pin the seam. Split out of
## `_apply_dock_layout` so a wide-dock fit-to-content height recompute can resize the card WITHOUT
## re-arranging the body (which would recurse back into the packer).
func _apply_root_anchors() -> void:
	if _root == null:
		return
	var cross := _cross_axis_size()
	# `_edge_offset` shifts the panel INBOARD from the docked edge, so a co-edge reserver
	# (e.g. the Inspector, which is always the inboard screen-edge reserver) sits between the
	# screen edge and this panel — the two stack instead of overlapping. The near offset is
	# `_edge_offset`, the far offset `_edge_offset + cross`.
	var near := _edge_offset
	var far := _edge_offset + cross
	# Re-anchor _root to the active edge, fixed on the cross axis, filling the rest.
	match _dock_edge:
		SIDE_LEFT:
			_set_root_anchors(0.0, 0.0, 0.0, 1.0)
			_set_root_offsets(near, 0.0, far, 0.0)
		SIDE_RIGHT:
			_set_root_anchors(1.0, 0.0, 1.0, 1.0)
			_set_root_offsets(-far, 0.0, -near, 0.0)
		SIDE_TOP:
			_set_root_anchors(0.0, 0.0, 1.0, 0.0)
			_set_root_offsets(0.0, near, 0.0, far)
		SIDE_BOTTOM:
			_set_root_anchors(0.0, 1.0, 1.0, 1.0)
			_set_root_offsets(0.0, -far, 0.0, -near)
	_position_card_and_rail()
	_position_seam()

## Place the CARD and the CHROME CLUSTER inside `_root`'s strip (issue #377).
##
## **A horizontal dock is TWO FLOATING ISLANDS, not one bar.** The card used to be `PRESET_FULL_RECT`
## with the chrome as its last cell, deliberately, so a bottom dock read as one continuous bar — and
## that is exactly what made it span two feet of an ultrawide with the work zone stretched across the
## middle. The card is now sized to its CONTENT (`_card_width`) and centred in the room the chrome
## leaves; the chrome is pinned to the strip's trailing edge with a bare gutter between them. The
## reference is the tile bar at the top of the screen: a card as wide as what it has to say, over live
## map, with the readouts as their own cluster beside it.
##
## **A VERTICAL dock is untouched** — the card fills its strip exactly as before and there is no rail.
func _position_card_and_rail() -> void:
	if _root == null or _panel == null:
		return
	if _is_vertical_edge(_dock_edge):
		_panel.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		return
	# The chrome cluster: flush to the STRIP's trailing edge — i.e. the screen's — full strip height.
	# Anchored by hand rather than laid out, because it is no longer any container's child.
	#
	# **`_bound_trailing` IS DELIBERATELY ABSENT FROM BOTH OFFSETS.** It was briefly in them, to hold the
	# rail off the HUD's right-hand column, and that inset the parked minimap and turn orb by the
	# column's whole width — leaving a visible band of dead map between the chrome and the screen edge on
	# every bottom dock past the fork. The clearance runs the other way now: when the HUD keeps this
	# strip, `Main._update_right_column_bottom_clearance` stops the right dock's CARDS above the strip's
	# top edge, so the corner is the chrome's alone and there is nothing here to be drawn over. The CARD
	# is a different island and still pays both bounds — it shares the columns' vertical band, the rail
	# does not.
	var rail_width := _rail_width()
	if _rail != null:
		_rail.anchor_left = 1.0
		_rail.anchor_right = 1.0
		_rail.anchor_top = 0.0
		_rail.anchor_bottom = 1.0
		_rail.offset_left = -rail_width
		_rail.offset_right = 0.0
		_rail.offset_top = 0.0
		_rail.offset_bottom = 0.0
	# The card: its content width, centred in what the chrome leaves. Clamped to the available room so a
	# window narrower than the content can never slide the card under the chrome.
	# Centred in the room the chrome cluster and the HUD columns leave — and OFFSET past the leading
	# bound, so "centred" means centred in the gap rather than centred on the screen with a column
	# underneath one end of it.
	#
	# **THE CENTRING IS ONLY AS TRUE AS `_available_card_span()`.** The gap runs from the leading bound to
	# whatever really stands at the trailing end, and this centres in `available` starting at that bound —
	# so any term in the span the strip does not actually charge shows up here as an off-centre card, not
	# as a narrow one. A trailing bound the right column no longer needs put the card 419px short of its
	# own gap at 2560, i.e. ~210px off centre; `_trailing_bound_for` is where that was fixed, and nothing
	# in this block had to change for it.
	var available: float = _available_card_span()
	var card_width: float = minf(_card_width(), available)
	var lead: float = _bound_leading + 0.5 * maxf(available - card_width, 0.0)
	_panel.anchor_left = 0.0
	_panel.anchor_right = 0.0
	_panel.anchor_top = 0.0
	_panel.anchor_bottom = 1.0
	_panel.offset_left = lead
	_panel.offset_right = lead + card_width
	_panel.offset_top = 0.0
	_panel.offset_bottom = 0.0

## Choose the shell for the panel's current WIDTH and home the zones into it. Called on every
## dock-layout pass; cheap and idempotent.
func _relayout_body() -> void:
	if _wide_shell == null or _narrow_shell == null:
		return
	var was_wide := _body_is_wide
	_body_is_wide = _shell_is_wide()
	if _body_is_wide != was_wide:
		_rebuild_tab_bar()
	# A multi-column zone's host width is not a constant — it is its declared column times the count
	# `zone_columns()` grants — so every FIXED host's pinned minimum is re-declared on each layout pass
	# rather than only at `_build`. A single-column zone re-declares the same number it already had, so
	# this is a no-op for the parties and knowledge flanks.
	for spec in _zone_layout:
		var host: Control = _wide_zone_hosts.get(StringName(spec.get(ZONE_SPEC_KEY, &"")))
		if host != null and _spec_width(spec) > 0.0:
			host.custom_minimum_size.x = _zone_span(spec)
	_reparent_zones()
	_update_body_visibility()

## What the wide shell's separators + their gaps cost its interior width: **ONE `RAIL_SEPARATOR_SPAN`
## per GAP between adjacent columns**, so the term follows the live zone list instead of being pinned
## at the two gaps three zones happen to have. Shared by the threshold, `_card_width`,
## `_affordable_work_columns` and `zone_size`, so none of them can disagree about how much width the
## chrome eats.
func _wide_separator_span() -> float:
	return float(maxi(_zone_layout.size() - 1, 0)) * RAIL_SEPARATOR_SPAN

## The panel switches to the wide (zones-side-by-side) shell once its own WIDTH reaches this; below it
## the narrow (tabbed, one-zone) shell is used. A WIDTH test, never a dock-edge test, so a resizable
## dock or a narrow window needs no special case.
##
## **DERIVED FROM THE LIVE ZONE LIST, never hand-picked and never a fixed set of terms.** The wide
## shell is only worth choosing when it can still give the expanding zone one readable column, so this
## is exactly what every zone + the separators + the card chrome need: a band's three come to
## `380 + 380 + 354 + 2×25 + 26 = 1190`, and the faction page's four to
## `380 + 380 + 354 + 354 + 3×25 + 26 = 1569`. It is compared against the OUTER `_panel_extent().x`,
## hence the chrome term — below it the narrow shell would hand the board the panel's whole interior,
## so flipping wide too early makes the board several times NARROWER, degrading the very thing the
## wide shell exists to improve. It shipped hand-picked at 900 once and silently broke every window
## between 900 and 1055.
##
## An EXPANDING zone contributes `ZONE_WORK_MIN_WIDTH` — the one readable board column that is the
## whole point of the test — and a fixed one contributes ONE of its declared columns.
##
## **IT SUMS `_spec_width`, NEVER `_zone_span`, and that is what keeps the layout acyclic.** A
## multi-column zone's granted count is a function of `_available_card_span()`, which is tested against
## THIS — so folding the grant in would make the threshold call the count that calls the threshold. It
## is also the right answer on its own terms: this is the MINIMUM the shell needs to be worth
## choosing, and one band column is exactly that minimum.
func wide_shell_min_width() -> float:
	var span := _wide_separator_span() + PANEL_CHROME_H
	for spec in _zone_layout:
		var width := _spec_width(spec)
		span += width if width > 0.0 else ZONE_WORK_MIN_WIDTH
	return span

## Declare the room the card must leave at each end of a horizontal strip — the HUD columns it must not
## be drawn over. `Main` owns the widths; the panel owns what to do with them.
##
## **They are the columns' LIVE widths, not the HUD's authored minimums** (`HudLayer.lateral_column_widths`),
## which is the opposite of the bound the event dock takes and deliberately so. That one fixes an EDGE
## that must not jitter from turn to turn, and a column drawing wider than its minimum merely overlaps it
## a little. This one decides whether a CARD is drawn THROUGH the readouts, where being a little wrong is
## not cosmetic — a readout line longer than its column's authored minimum would be overdrawn. So the
## bound moves when the columns do — `Main` re-pushes it per snapshot, and this early-outs on an
## unchanged pair. (The authored minimums are sized to their worst case now, so the live term is a net
## rather than the usual answer — see `affords_wide_shell_with_bounds` for why that distinction matters.)
##
## **Without this the top-dock HUD exemption is only correct for a SPARSE band.** A band with no worked
## sources makes a narrow card with room either side, which is what made the fix look complete; a band
## with 34 sources makes a 1570px card in a 1920px strip and puts it straight through the readouts.
func set_lateral_bounds(leading: float, trailing: float) -> void:
	var lead: float = maxf(leading, 0.0)
	var trail: float = maxf(trailing, 0.0)
	if is_equal_approx(lead, _bound_leading) and is_equal_approx(trail, _bound_trailing):
		return
	_bound_leading = lead
	_bound_trailing = trail
	_apply_dock_layout()
	_republish_reservation_if_changed()
	_notify_zones_resized()

## The span of strip a horizontal card may actually use: the whole row less the chrome cluster and less
## whatever HUD column sits at either end. The ONE definition, so `_card_width`, `_interior_size` and
## `_position_card_and_rail` cannot disagree about how much room there is.
##
## **It is also what makes the card CENTRED**: `_position_card_and_rail` centres in this span offset past
## the leading bound, so the span and the gap the card really has must be the same number. A trailing
## term the strip does not actually charge would centre the card in a sub-region of its own gap and leave
## it visibly off to one side.
func _available_card_span() -> float:
	return maxf(_panel_width_extent() - _rail_span() - _bound_leading
		- _trailing_bound_for(_dock_edge, _bound_trailing), 0.0)

## What the card must really leave at the TRAILING end of a horizontal strip — **ZERO on a BOTTOM dock**.
##
## `Main` declares both HUD columns (`set_lateral_bounds`); this is where the panel decides which of them
## it is actually charged for, and it is the ONE definition, read by `_available_card_span()` and by
## `affords_wide_shell_with_bounds()` alike.
##
## **The right-hand column cannot reach a BOTTOM dock's strip in EITHER branch of the yield rule**, so a
## bound against it reserves room for a collision that cannot happen. Where the HUD yields, `LayoutRoot`
## is inset wholesale and the column stops at the strip's top edge; where it keeps, the right dock's cards
## are held above that edge by `Hud.set_right_column_bottom_clearance` — which exists because the parked
## chrome owns that corner. The top-bar readouts, the column's other region, are at the far end of the
## screen and share no vertical band with the strip at all. Measured, the bound was costing the card 419px
## at 2560 and leaving it centred ~90px left of its own gap.
##
## **The LEADING bound stays, and the asymmetry is the design.** The left column deliberately runs to the
## window's bottom edge wherever the HUD keeps its strip — that is the whole point of the conditional
## inset — so the card really does have to clear it.
##
## **A TOP dock pays BOTH**, unchanged: the HUD is exempt there (issue #377) and the top-right readout
## block genuinely shares that strip's row with the card.
##
## Both terms are PARAMETERS rather than reads of `_dock_edge` / `_bound_trailing`, because the two
## callers hold different ones: the layout asks about the bound `Main` last pushed, while the
## affordability predicate asks about a CEILING it was handed and must not read live state for (see
## `affords_wide_shell_with_bounds`).
func _trailing_bound_for(edge: int, trailing: float) -> float:
	if edge == SIDE_BOTTOM:
		return 0.0
	return maxf(trailing, 0.0)

## **COULD the card stand in the WIDE shell if it had to keep clear of these two columns?** The question
## `Main.band_dock_overlays_hud` asks before it lets the HUD keep its strip on a BOTTOM dock: yielding the
## strip costs the HUD's left column its full height, and not yielding costs the card the two bounds — so
## the trade is only worth taking while the card can still pay them and stay in the three-zone shell.
##
## **EVERY TERM MUST BE ONE THE INSET CANNOT MOVE, and that is the whole reason this is a separate
## question from `_shell_is_wide()`.** The caller passes the HUD's AUTHORED column widths
## (`left_column_width` / `right_column_width`, scene constants) and the rail width the HUD's chrome WILL
## declare, never `lateral_column_widths()`'s `max(authored, live)`: a live width follows a column's
## rendered extent, the inset decides that column's height, and reading it here would make the predicate
## depend on its own output. `_panel_width_extent()` is the viewport. So the answer is a function of the
## window, two constants and a declared width — no path back to the inset, hence no cycle.
##
## The rail width is a PARAMETER for the same order-independence reason: `_rail_declared_width` is pushed
## by `DockRowController` on the *second* listener of `reservation_changed`, so reading it here would
## answer against the rail the panel had a moment ago rather than the one it is about to be given.
##
## **THE ANSWER IS ONLY HONEST WHILE THE BOUNDS PASSED IN COVER THE ONES THE CARD IS PLACED AGAINST.**
## This asks whether the card can pay `leading + trailing`; `_available_card_span()` then lays the card
## out against the bounds `Main` actually pushes, which are `HudLayer.lateral_column_widths()`'s
## `max(authored, live)`. Any pixel by which those exceed what was passed here is a band of window
## widths where this says "afford" and the shell comes out NARROW — the trade the rule exists to
## refuse, taken silently. It shipped that way, passed the columns' authored RESERVATIONS (344 on the
## trailing one) against a live 419, and the band was 75px wide. The caller passes ceilings now
## (`Hud.right_column_ceiling`); what belongs here is why the caller may not pass anything smaller.
##
## **IT ASKS `wide_shell_min_width()`, SO THE VERDICT IS PER-SUBJECT — and it must be.** The threshold
## is a sum over the LIVE zone list (issue #450), so a four-zone faction page needs 379px more than a
## band's three. A predicate frozen at the three-zone number would tell the HUD to keep its strip while
## the four-zone page then laid out NARROW — the predicate/consumer mismatch this rule already shipped
## once with the columns' reservations, one subject along. The consequence is that the fork MOVES when
## the player cycles onto the faction page, which is correct: a wider body genuinely needs a wider
## window before the trade is worth taking.
##
## A collapsed or hidden panel draws no card at all, and a vertical dock's card is a fixed `PANEL_WIDTH`
## strip — none of them is a wide shell, so none of them can afford one.
func affords_wide_shell_with_bounds(leading: float, trailing: float, rail_width: float) -> bool:
	if _collapsed or not _shown or _is_vertical_edge(_dock_edge):
		return false
	# `_trailing_bound_for` is what makes this and `_available_card_span()` ask the SAME question: a
	# predicate charging the card for a column the layout does not charge it for would refuse the trade
	# on exactly the widths where the card could in fact have paid. It drops the trailing term on a
	# BOTTOM dock, which is what moved the fork from a logical 2432 to 1871.
	var span: float = _panel_width_extent() - _rail_span_of(rail_width) \
		- maxf(leading, 0.0) - _trailing_bound_for(_dock_edge, trailing)
	return span >= wide_shell_min_width()

## How wide the CARD draws. A vertical dock is the fixed strip; a horizontal one is exactly what its
## content needs, which is what stops a bottom dock spanning an ultrawide.
##
## **The wide shell's width is DECLARED, not measured** — the flanks are fixed, and the work board's
## column count arrives through `set_work_columns` from the controller that knows how many sources
## there are. That direction is the `set_rail_width` contract again, and it is what keeps this
## acyclic: the SHELL is still chosen from the room the strip has (`_shell_is_wide`), never from the
## card, so nothing here can feed back into the choice that produced it.
##
## The narrow shell takes the whole available strip: it is reached only when there is too little room
## for the declared zones abreast, i.e. exactly when there is nothing to give back.
func _card_width() -> float:
	if _is_vertical_edge(_dock_edge):
		return PANEL_WIDTH
	var available: float = _available_card_span()
	if not _shell_is_wide():
		return available
	var work: float = float(clampi(_work_columns, 1, WORK_MAX_COLUMNS)) * ZONE_WORK_MIN_WIDTH
	return _fixed_zone_span() + work + _wide_separator_span() + PANEL_CHROME_H

## Declare how many columns the WORK board wants — the count `BandPanelController` derives from its
## source list and the zone's HEIGHT. The panel then draws a card exactly that wide.
##
## **DECLARED, never measured here**, the `set_rail_width` contract: the controller owns the sources, so
## the controller counts them. It is also what keeps the handshake ACYCLIC — the column count follows
## from the zone's height and the source count, neither of which depends on the width this sets. A
## repeat declaration early-outs, so the `zones_resized` → repage → declare loop settles in one pass.
##
## It can NEVER re-emit `reservation_changed`: the reservation is the CROSS axis, and this spends the
## long one.
##
## **AND IT MUST NOT EMIT `zones_resized` EITHER**, which is the one non-obvious part. The caller is the
## controller in the middle of BUILDING the board, and `zones_resized` is what makes it re-page — so
## emitting here would re-enter `_fill_work_zone` from inside itself. It is also unnecessary: the
## controller already holds the column count (it is the call's return value) and is about to fill the
## resized host with exactly that many columns. So the cached size is refreshed SILENTLY, which both
## breaks the loop and stops the next genuine resize firing a spurious re-page against a stale value.
## **It RETURNS the count it actually applied, and the caller must build to that** — a want is not a
## grant. The board can ask for four columns on a band with 34 sources while the strip can only pay for
## one (a narrow side dock, or the wide shell at its own minimum width), and a board built to the want
## rather than the grant overflows its zone by ~190px in a 380px dock and ~725px at the shell
## threshold — silently, since the zone hosts CLIP.
func set_work_columns(columns: int) -> int:
	var want := clampi(columns, 1, WORK_MAX_COLUMNS)
	want = maxi(mini(want, _affordable_work_columns()), 1)
	if want != _work_columns:
		_work_columns = want
		_apply_dock_layout()
		_last_work_zone_size = work_zone_size()
	return _work_columns

## The most board columns the STRIP can pay for, whatever the source count wants. The card grows to fit
## its content, but only up to the room it actually has — past that the board has to page instead.
##
## The narrow shell hands its ONE zone the whole interior, so the question there is simply how many
## readable columns that interior holds. The wide shell has to pay for both flanks and the separators
## out of the same strip first, and the chrome cluster before any of it.
func _affordable_work_columns() -> int:
	if not _shell_is_wide():
		return maxi(int(_interior_size().x / ZONE_WORK_MIN_WIDTH), 1)
	# `_available_card_span()`, not the raw strip: the HUD columns a top dock must keep clear of come off
	# the card's room BEFORE the board gets any of it, so counting columns against the whole row builds a
	# board the clamped card cannot hold — measured as 135px of it hanging out of a clipping zone host.
	var room: float = _available_card_span() - PANEL_CHROME_H - _fixed_zone_span() \
		- _wide_separator_span()
	return maxi(int(room / ZONE_WORK_MIN_WIDTH), 1)

## True when the panel is wide enough for the LIVE zone list side by side. A WIDTH test, never a
## dock-edge test — see `wide_shell_min_width()`.
## The panel must NEVER enter the wide shell with a work zone below `ZONE_WORK_MIN_WIDTH` — the exact
## failure the hand-picked 900 threshold caused. The threshold is zones + separators +
## `PANEL_CHROME_H` and is tested against the OUTER width, so the chrome rail (which spends that same
## outer width before the zones see any of it — its column AND its separator gutter) must come off first.
func _shell_is_wide() -> bool:
	if _is_vertical_edge(_dock_edge):
		# `_panel_width_extent()`, never `_panel_extent()`: the cross axis now depends on which shell is
		# active (`_shell_chrome_height`), so building the whole extent here would call the height,
		# which calls this test.
		return _panel_width_extent() - _rail_span() >= wide_shell_min_width()
	# `_available_card_span()` on a horizontal dock, because the HUD columns a top dock keeps clear of
	# come off the CARD's room before any zone sees it (issue #377). Testing the raw strip put the panel
	# into the wide shell on a 1920 top dock whose card could only have 1141 — a 331px work zone against
	# a 380px minimum, i.e. exactly the invariant `wide_shell_min_width()` was derived to protect.
	return _available_card_span() >= wide_shell_min_width()

## The RESERVED STRIP for the current dock: fixed on the cross axis, the window on the other. It was the
## card's outer size until issue #377, when the card stopped filling a horizontal strip; it is the region
## `_root` spans, and what `_available_card_span()` subtracts the chrome cluster and the HUD columns from
## to get the room the card may actually use. On a VERTICAL dock the card still fills it, so the two
## readings coincide there.
func _panel_extent() -> Vector2:
	var window := _viewport_size()
	if _is_vertical_edge(_dock_edge):
		return Vector2(PANEL_WIDTH, window.y)
	return Vector2(window.x, _horizontal_panel_height())

## The strip's extent along the axis the CARD's WIDTH is measured on — `_panel_extent().x`, split out
## as its own reader.
##
## **The split is what keeps the layout ACYCLIC**, not tidiness: the cross axis now depends on which
## SHELL is active (`_shell_chrome_height`), and the shell is chosen from this width — so a shell test
## that built the whole extent would call the height, which calls the shell test, which builds the
## extent. Every width reader takes this; only `_panel_extent()` itself pairs it with the height.
func _panel_width_extent() -> float:
	return PANEL_WIDTH if _is_vertical_edge(_dock_edge) else _viewport_size().x

## The T/B cross-axis size: the fixed body budget `PANEL_HEIGHT_WIDE` PLUS whatever the ACTIVE SHELL
## spends on its own chrome, clamped to a fraction of the window so a short window can never let the
## strip eat the screen.
##
## **THE SHELL TERM IS THE FIX FOR A ZONE SLICED BY THE STRIP'S OWN TAB BAR.** `PANEL_HEIGHT_WIDE` is
## the budget the zones' content is tuned against — the band zone's SHORT tier reads 299px of the
## 300px box a wide horizontal dock offers — and it was spent as a FLAT strip height, so the narrow
## shell paid for its tab bar out of the zone: measured, a 265px box for content that needs 273
## (`band_panel_scale_bottom`), which the `clip_contents` zone host then sliced with no scrollbar and
## no affordance. On a BOTTOM dock that cut lands at the window's own edge, which is what makes it
## read as the panel "running off the bottom of the screen" rather than as a clipped zone.
##
## It is deliberately NOT a scale question — the narrow shell is reached at `ui_scale` 1.0 in a window
## under ~1511px too, and paid the same 35px there. The two shells now hand their zones the SAME box,
## which is the only arrangement under which one set of tier thresholds can be right for both.
##
## **AND THE COLUMN TERM IS THE SAME ARGUMENT AGAIN.** `_body_budget()` picks the budget from
## `band_zone_columns()`, which is as purely geometric as `_shell_chrome_height()` is — so the strip
## can get shorter when the flank widens without the height ever becoming a function of content.
##
## **THE ACTION BAR IS NOT A TERM HERE, AND ITS ABSENCE IS THE POINT.** A horizontal dock mounts the
## registered actions on the SUBJECT ROW, so the bar is hidden and measures 0 — the strip reads the
## same 360 / 335 it did before the registry existed, and every pixel of it is still the zones' box
## plus the active shell's chrome. Height is the axis this dock reserves off the map, so a row of
## glyphs charged here would be map given up for chrome the subject row had width to carry.
func _horizontal_panel_height() -> float:
	return minf(_body_budget() + _shell_chrome_height(),
		_viewport_size().y * MAX_WIDE_HEIGHT_FRACTION)

## The body budget for the CURRENT flank layout: a two-column flank stacks its blocks side by side and
## needs a shorter box than the one-column stack does, so it may take `PANEL_HEIGHT_WIDE_TWO_COLUMN`.
##
## ⛔ **BUT NEVER LESS THAN THE ONE-COLUMN BUDGET, and that MAX is the whole of this function's job.**
## The two-column saving is a claim about the BAND FLANK alone, and it hands its shorter box to EVERY
## zone. That was safe while the WORK zone could absorb it — it paged, so a shorter box cost it a board
## row — and `docs/plan_standing_upkeep.md` §4.7 ended that: the POOLS block and the BUILD QUEUE block
## are FIXED-height blocks that do not page, so the work zone now has a hard floor and can BIND. It
## does: its worst case measures **284px** and the two-column budget offers **275**, which the zone
## would have CLIPPED silently — no overflow, no warning, just fewer board rows than the pager thinks
## it drew. See `PANEL_HEIGHT_WIDE_TWO_COLUMN` for the floor and for why its old "never binds by
## construction" reasoning expired.
##
## **AT THE CURRENT NUMBERS THE TWO-COLUMN BRANCH IS INERT** (329 against 440) and this always answers
## `PANEL_HEIGHT_WIDE`. It is written as a MAX rather than deleted because the branch is a live
## derivation whose terms move — `BAND_ZONE_TWO_COLUMN_EXTENT` is re-measured whenever the flank's
## blocks change — and the max is what makes it unable to come back as a REGRESSION rather than as a
## saving. **Do not "simplify" it to the constant**: that would delete the guard along with the branch.
##
## **BOTH TERMS ARE GEOMETRIC, so the flicker invariant is untouched.** `band_zone_columns()` reads the
## span, the dock edge, the rail and the lateral bounds and nothing else, and `PANEL_HEIGHT_WIDE` is a
## constant — a max over two content-independent numbers is content-independent, so the strip's height
## still cannot become a function of what the band holds.
##
## A vertical dock and the narrow shell both answer ONE column by construction, so neither is touched.
func _body_budget() -> float:
	if band_zone_columns() > 1:
		return maxf(PANEL_HEIGHT_WIDE_TWO_COLUMN, PANEL_HEIGHT_WIDE)
	return PANEL_HEIGHT_WIDE

## How many columns of its DECLARED width a zone lays its blocks out across, capped by the
## `ZONE_SPEC_MAX_COLUMNS` its subject declared (see that const for why the cap is the subject's).
##
## **PURELY GEOMETRIC — what the span AFFORDS, never what the content holds.** That is the whole
## safety argument. Every term below is a function of the viewport, the dock edge, the rail's declared
## span and the lateral bounds; not one of them is content, so the strip's height (which keys off this)
## stays content-independent and the reservation cannot move when the player edits the band. A count
## that grew with the roster would put `MapView`'s inset on the snapshot's critical path — the flicker
## bug the fixed cross-axis size exists to prevent.
##
## The room measured is what is left AFTER EVERY OTHER declared zone is paid at ONE column: the wide
## shell exists to give the work board a readable column, so a second band column is only affordable
## once the parties flank (and, on the faction page, knowledge) and one `ZONE_WORK_MIN_WIDTH` are
## already covered. `_affordable_work_columns()`'s idiom, one flank over.
##
## **THAT SUM IS `wide_shell_min_width()` LESS THIS ZONE'S OWN COLUMN, never a hand-listed pair of
## flanks** — which is what makes the count correct for a four-zone page without a second formula, and
## what stops the two drifting the way the retired `WIDE_SEPARATOR_SPAN` pair did.
##
## ONE on a vertical dock and one in the narrow shell, both by construction: those layouts hand a zone
## a single strip-width column, which is what `PANEL_WIDTH` means. ONE also for any zone that declared
## no `ZONE_SPEC_MAX_COLUMNS`, and for the EXPANDING zone, which spends the row's remainder rather than
## a count of columns (`set_work_columns`).
func zone_columns(zone: StringName) -> int:
	var spec := _spec_for(zone)
	var cap: int = maxi(int(spec.get(ZONE_SPEC_MAX_COLUMNS, ZONE_COLUMNS_MIN)), ZONE_COLUMNS_MIN)
	var column: float = _spec_width(spec)
	if cap <= ZONE_COLUMNS_MIN or column <= 0.0:
		return ZONE_COLUMNS_MIN
	if _is_vertical_edge(_dock_edge) or not _shell_is_wide():
		return ZONE_COLUMNS_MIN
	var room: float = _available_card_span() - (wide_shell_min_width() - column)
	return clampi(int(room / column), ZONE_COLUMNS_MIN, cap)

## How many `ZONE_BAND_WIDTH` columns the BAND flank lays its blocks out across — a NAMED reader of the
## generic answer above, kept because `BandPanelController.build_band_zone` authors its split against
## this one zone and `_body_budget()` keys the strip's height off it.
func band_zone_columns() -> int:
	return zone_columns(ZONE_BAND)

## What the ACTIVE shell spends on the strip's CROSS axis before any zone sees the box. The wide shell
## spends none — its separators are vertical hairlines, paid out of the width — while the narrow shell
## carries the tab bar and the gap beneath it. Read through `_shell_is_wide()` rather than the cached
## `_body_is_wide`, so a dock layout that arrives before `_relayout_body` has re-chosen the shell still
## sizes the strip for the shell it is about to get.
func _shell_chrome_height() -> float:
	return 0.0 if _shell_is_wide() else _tab_bar_height()

## The card's INTERIOR box — what the card DRAWS AT, less the border and the content margins it draws
## with (`panel_card_stylebox`). Chrome only; never content.
## `work_zone_size()` and `_affordable_work_columns()` read this, so both follow the card's width with no
## edit of their own.
func _interior_size() -> Vector2:
	var chrome_v := 2.0 * (PANEL_CONTENT_MARGIN_V + PANEL_BORDER_WIDTH)
	# The WIDTH comes off the CARD, not the strip (issue #377): the card no longer spans a horizontal
	# dock, so the strip's width stopped being what the zones have to spend. `_card_width()` has already
	# subtracted the chrome cluster and its gutter, which is why `_rail_span()` does not appear again
	# here. The HEIGHT is still the strip's — the card is full-height on both axes of a horizontal dock.
	var card_width: float = minf(_card_width(), _available_card_span())
	return Vector2(maxf(card_width - PANEL_CHROME_H, 0.0), maxf(_panel_extent().y - chrome_v, 0.0))

## Height of the header row — pure chrome (two text rows beside the icon controls), so measuring it
## keeps the interior maths content-independent. Falls back before the first layout pass.
func _header_height() -> float:
	if _header_full == null:
		return HEADER_HEIGHT_FALLBACK
	var measured := _header_full.get_combined_minimum_size().y
	return measured if measured > 0.0 else HEADER_HEIGHT_FALLBACK

## Height the narrow shell's tab bar takes off the body (plus the gap under it).
func _tab_bar_height() -> float:
	if _tab_bar == null:
		return 0.0
	var measured := _tab_bar.get_combined_minimum_size().y
	return measured + float(BODY_SEPARATION)

## The tab the narrow shell actually shows: the selected one when it has content, else the first zone
## that does. A selected tab whose zone was handed in as null must not black the panel out — and this
## is what keeps the Part-1 shim (which fills only the BAND zone) previewable under the `work` default.
## **It is also what makes a per-subject layout safe**: the persisted tab may name a zone this
## subject does not declare (`knowledge` on a band's page), and that resolves to a zone it does.
func _effective_tab() -> StringName:
	if _zones.get(_active_tab) is Control:
		return _active_tab
	for zone in _zone_order():
		if _zones.get(zone) is Control:
			return zone
	return _active_tab

## Home each owned zone Control into the active shell's host: every zone side by side in the wide
## shell, only the selected tab's zone in the narrow one (the rest are detached but still owned,
## so a tab switch is a reparent rather than a Hud re-render).
func _reparent_zones() -> void:
	for zone in _zone_order():
		var zone_variant: Variant = _zones.get(zone)
		if not (zone_variant is Control):
			continue
		var control: Control = zone_variant
		var host: Control = null
		if _body_is_wide:
			host = _wide_zone_hosts.get(zone)
		elif zone == _effective_tab():
			host = _narrow_zone_host
		if host == null:
			_detach(control)
			continue
		if control.get_parent() != host:
			_detach(control)
			host.add_child(control)
		# The host is a plain Control, so the zone content anchors itself to fill it.
		control.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

## Show the active shell when a band is present, else neither (the empty-state placeholder shows
## instead). Collapse is handled separately by `_refresh_collapse_state` hiding the whole `_body_host`.
func _update_body_visibility() -> void:
	if _wide_shell != null:
		_wide_shell.visible = _band_present and _body_is_wide
	if _narrow_shell != null:
		_narrow_shell.visible = _band_present and not _body_is_wide

# ---- wide shell scaffolding ------------------------------------------------

## Rebuild the wide shell's columns from the LIVE layout: one host per zone in declared order, with a
## hairline separator in every gap between them. Called from `_build` and again whenever
## `set_zone_layout` changes the list.
##
## **THE ZONE CONTENTS MUST ALREADY BE GONE.** A host owns whatever `_reparent_zones` put in it, so
## freeing the hosts with content still parented would take the contents with them behind the panel's
## own ownership bookkeeping. `set_zone_layout` frees them first, which is why this is private.
func _rebuild_wide_shell() -> void:
	if _wide_shell == null:
		return
	for child in _wide_shell.get_children():
		_wide_shell.remove_child(child)
		child.queue_free()
	_wide_zone_hosts.clear()
	for i in range(_zone_layout.size()):
		if i > 0:
			_wide_shell.add_child(_make_zone_separator())
		var zone := StringName(_zone_layout[i].get(ZONE_SPEC_KEY, &""))
		_wide_zone_hosts[zone] = _add_wide_zone_host(zone, _spec_width(_zone_layout[i]))

## The layout's zone keys, in declared order — what the tab bar, the reparenting and the effective-tab
## fallback all walk.
func _zone_order() -> Array[StringName]:
	var keys: Array[StringName] = []
	for spec in _zone_layout:
		keys.append(StringName(spec.get(ZONE_SPEC_KEY, &"")))
	return keys

## A descriptor's ONE-column wide-shell width: its declared flank, or `ZONE_WIDTH_EXPAND` for the
## expanding zone. **The BASE, not the extent** — `_zone_span()` below is what the zone actually
## spends; see `ZONE_SPEC_MAX_COLUMNS` for why the two are separate.
func _spec_width(spec: Dictionary) -> float:
	return maxf(float(spec.get(ZONE_SPEC_WIDTH, ZONE_WIDTH_EXPAND)), 0.0)

## The live descriptor for a zone key, or an empty Dictionary where the layout does not declare it —
## so every reader takes the same defaults through `get`.
func _spec_for(zone: StringName) -> Dictionary:
	for spec in _zone_layout:
		if StringName(spec.get(ZONE_SPEC_KEY, &"")) == zone:
			return spec
	return {}

## What ONE fixed zone actually spends of the row: its declared column times the count `zone_columns()`
## granted it. **The ONE definition**, so `_card_width`, `zone_size` and `_affordable_work_columns`
## cannot disagree about how wide a widened flank is — exactly as `_wide_separator_span()` is the one
## definition of what the separators cost. The expanding zone answers `ZONE_WIDTH_EXPAND` unchanged.
func _zone_span(spec: Dictionary) -> float:
	var column := _spec_width(spec)
	if column <= 0.0:
		return ZONE_WIDTH_EXPAND
	return column * float(zone_columns(StringName(spec.get(ZONE_SPEC_KEY, &""))))

## One zone's FIXED wide-shell width, or `ZONE_WIDTH_EXPAND` if it is the expanding one (or absent).
func _zone_fixed_width(zone: StringName) -> float:
	return _zone_span(_spec_for(zone))

## What the layout's FIXED flanks cost the wide shell's interior width, summed over the live list —
## never a pair of named constants, which is what made a fourth zone a rewrite rather than a row.
func _fixed_zone_span() -> float:
	var span := 0.0
	for spec in _zone_layout:
		span += _zone_span(spec)
	return span

## One wide-shell zone column. `fixed_width > 0` pins the column (band / parties / knowledge);
## `ZONE_WIDTH_EXPAND` makes it the expanding one (work).
func _add_wide_zone_host(zone: StringName, fixed_width: float) -> Control:
	var host := _make_zone_host("Zone_%s" % String(zone), fixed_width)
	_wide_shell.add_child(host)
	return host

## A zone host. Deliberately a PLAIN `Control`, not a container: a container reports its children's
## combined minimum size, so an over-wide zone content would push the whole card past its FIXED
## cross-axis size (a 380 L/R strip rendering 456px wide, spilling over the map) — the very
## content-dependence this rework removes. A plain Control reports no minimum, so the zone stays the
## size the shell gave it, and `clip_contents` keeps anything that does not fit inside its own zone
## instead of painting over its neighbour. Zone content is anchored full-rect into it by
## `_reparent_zones`.
func _make_zone_host(host_name: String, fixed_width: float) -> Control:
	var host := Control.new()
	host.name = host_name
	host.clip_contents = true
	host.size_flags_vertical = Control.SIZE_EXPAND_FILL
	if fixed_width > 0.0:
		host.custom_minimum_size = Vector2(fixed_width, 0.0)
		host.size_flags_horizontal = Control.SIZE_FILL
	else:
		host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return host

# ---- the trailing chrome rail (the HUD's bottom-bar chrome shares the row) ----

## Build the row's trailing rail: a plain clipping `Control` holding a centred vertical stack of the two
## slot hosts. THREE deliberate choices, each load-bearing:
## 1. `_rail` is a PLAIN `Control`, not a container, for the same reason `_make_zone_host` is one — a
##    container reports its children's combined minimum size, so the HUD's chrome could push the card
##    past its FIXED cross-axis size. Its width is DECLARED by the HUD (`set_rail_width`), never
##    measured from the content, which is what keeps `work_zone_size()` content-independent.
## 2. Because that wrapper blocks propagation, everything INSIDE it can be an ordinary container — which
##    is why the slots are `MarginContainer`s rather than plain Controls. A slot must report the height
##    of the cluster parked in it or the stack collapses to nothing, and a container reading its own
##    child's minimum is exactly the mechanism for that. Nobody has to measure the chrome twice.
## 3. The stack CENTRES via `ALIGNMENT_CENTER`, not by anchor arithmetic. Anchors were tried and are a
##    trap here: `set_anchors_and_offsets_preset` derives its offsets from `get_minimum_size()` — the
##    *virtual* one, which ignores `custom_minimum_size` — so it centres a `Container` and does NOT
##    centre a plain `Control`. Measured on the bottom dock: `PRESET_HCENTER_WIDE` gave `NavBacking` (a
##    `PanelContainer`) offsets `[0, -76, 0, +76]` = ±152/2, correctly centred, and gave `TurnCluster` (a
##    plain `Control` whose 128px height is only `custom_minimum_size`) offsets `[0, 0, 0, 0]` — its TOP
##    edge pinned to the mid-line, then grown DOWNWARD by `_size_changed`'s minimum clamp, rendering 64px
##    low (rect y 900–1028 in a host spanning 730–1070). A container-driven stack has no such asymmetry.
func _build_rail() -> void:
	_rail = Control.new()
	_rail.name = "ChromeRail"
	_rail.clip_contents = true
	_rail.visible = false
	# The chrome island eats its own clicks, for the same reason the card does: `_root` is `IGNORE` now,
	# so a press on the rail's bare column would otherwise fall straight through to the hex behind it.
	# `STOP` is the `Control` default; explicit because here it is load-bearing rather than incidental.
	_rail.mouse_filter = Control.MOUSE_FILTER_STOP
	# **A SIBLING OF THE CARD, NOT A CHILD OF IT** (issue #377). The rail used to be the last cell of
	# `_card_row`, i.e. chrome sitting ON the card — which is what made a horizontal dock read as ONE
	# continuous bar spanning the monitor. The card is an ISLAND now (`_position_card_and_rail`), so the
	# chrome is its own island beside it: parented to `_root`, right-anchored, and positioned by hand.
	# It keeps `NavBacking`'s own backing panel, so it reads as chrome floating over the map exactly as
	# the top-bar readouts do.
	_root.add_child(_rail)

	_rail_stack = VBoxContainer.new()
	_rail_stack.name = "ChromeRailStack"
	_rail_stack.alignment = BoxContainer.ALIGNMENT_CENTER
	_rail_stack.add_theme_constant_override("separation", RAIL_SLOT_SEPARATION)
	_rail_stack.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_rail.add_child(_rail_stack)

	for slot in RAIL_SLOT_ORDER:
		var host := MarginContainer.new()
		host.name = "ChromeRailSlot_%d" % slot
		# SHRINK_CENTER so a cluster narrower than the rail (the rail is the WIDER of the two) sits
		# centred in the column rather than ragged against its leading edge.
		host.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
		host.size_flags_vertical = Control.SIZE_FILL
		_rail_slots[slot] = host
		_rail_stack.add_child(host)

## The slot the HUD parks one bottom-bar chrome cluster into on a horizontal dock: `RAIL_SLOT_TOP` (the
## nav cluster) above `RAIL_SLOT_BOTTOM` (the turn cluster). The panel owns this host and NOTHING inside
## it: the HUD adds and removes its own nodes. Always non-null for a valid slot (the hosts exist from
## `_build`); the rail as a whole is hidden while it carries no declared width.
func rail_slot_host(slot: int) -> Control:
	return _rail_slots.get(slot)

## Declare the rail column's width — the `max` over the parked clusters, computed by the HUD, which owns
## them. NOT measured here. `width <= 0` retires the rail. Re-chooses the shell and re-reports
## `work_zone_size()`.
##
## **IT COULD ONCE NEVER RE-EMIT `reservation_changed`, AND THAT IS NO LONGER TRUE.** The claim rested
## on the reservation being a pure function of the collapse flag, the dock edge and the viewport — the
## rail spending only the LONG axis while the reservation is the CROSS one. Since the cross axis carries
## the active shell's own chrome (`_shell_chrome_height`), and the shell is chosen from the span the rail
## comes out of, a rail width can flip the shell and move the strip by the tab bar's height. So it
## republishes through `_republish_reservation_if_changed`, which read the old guarantee's PURPOSE rather
## than its letter: the loop it prevented (HUD pushes a width -> panel relayouts -> emit -> `Main` fan-out
## -> HUD reflow -> pushes a width) still terminates, because that second push lands on the same number
## and is dropped by the early-out above, and the republish itself is silent on an unchanged size.
func set_rail_width(width: float) -> void:
	var declared: float = maxf(width, 0.0)
	if is_equal_approx(declared, _rail_declared_width):
		return
	_rail_declared_width = declared
	# The FULL dock layout, not just the rail: since issue #377 the rail's own rect is written by
	# `_position_card_and_rail` (it is anchored, not laid out by a container), and the CARD's width and
	# centring are both computed against `_rail_span()`. Calling `_apply_rail` alone left the cluster
	# anchored at whatever width it had when the dock was last applied — measured as a 296px rail
	# hanging 180px off the end of a 1920px strip.
	_apply_dock_layout()
	_republish_reservation_if_changed()
	_notify_zones_resized()

## Size + show the rail for the current dock. The rail exists only on a HORIZONTAL dock: a vertical strip
## is `PANEL_WIDTH` (380) wide and has no room beside the zones for a ~300px chrome column.
## The rail's RECT is written by `_position_card_and_rail`, which runs on the same layout pass.
func _apply_rail() -> void:
	if _rail == null:
		return
	var width := _rail_width()
	_rail.custom_minimum_size.x = width
	_rail.visible = width > 0.0

## The rail's effective width: the declared value on a horizontal dock, 0 on a vertical one. Forcing 0 by
## EDGE rather than trusting the declared value keeps the panel correct whatever order the dock change
## and the HUD's push arrive in.
## Only the BOTTOM dock carries a rail: a vertical strip has no room beside its zones, and a TOP dock
## never displaces `BottomBar` in the first place, so its chrome stays home (`DockRowController.REFLOW_EDGES`).
## Forcing 0 by EDGE rather than trusting the declared value keeps the panel correct whatever order the
## dock change and the HUD's push arrive in.
func _rail_width() -> float:
	if _dock_edge != SIDE_BOTTOM:
		return 0.0
	return maxf(_rail_declared_width, 0.0)

## What the rail takes off the strip ALTOGETHER — its declared width PLUS the gutter beside it.
## The long-axis twin of `_wide_separator_span()`, and the value the width maths must use: subtracting
## `_rail_width()` alone would silently over-report the usable width by `RAIL_SEPARATOR_SPAN` (25px).
## **A retired rail contributes ZERO on both terms**, which is what keeps a vertical dock bit-identical
## to before the rail existed.
## Since issue #377 the gutter is BARE — the room between two floating islands, not a drawn hairline
## between two regions of one card. The `ChromeRailSeparator` `ColorRect` went with the merged bar; a
## rule down the gap between the card and the chrome would re-assert the very join that was removed.
func _rail_span() -> float:
	return _rail_span_of(_rail_width())

## What a rail of `width` would take off the strip. Split out so `affords_wide_shell_with_bounds` can ask
## the question about a width that has not been declared yet without restating the "+ gutter, or zero"
## rule — the two must never disagree about what a rail costs.
func _rail_span_of(width: float) -> float:
	if width <= 0.0:
		return 0.0
	return width + RAIL_SEPARATOR_SPAN

## The hairline rule between two adjacent zones in the wide shell.
func _make_zone_separator() -> ColorRect:
	var rule := ColorRect.new()
	rule.name = "ZoneSeparator"
	rule.color = HudStyle.LINE_SOFT
	rule.custom_minimum_size = Vector2(ZONE_SEPARATOR_THICKNESS, 0.0)
	rule.size_flags_horizontal = Control.SIZE_FILL
	rule.size_flags_vertical = Control.SIZE_EXPAND_FILL
	rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return rule

# ---- narrow shell tab bar --------------------------------------------------

## Rebuild the tab row from the LIVE layout + the current selection + badges. Cheap enough to redo
## wholesale, and it keeps the active/inactive styling in exactly one place.
##
## **`remove_child` BEFORE `queue_free`.** A queued node stays a child until the frame's idle pass, so
## a second rebuild in the SAME frame appends its tabs beside the dying ones and the bar renders every
## tab twice. That was latent while the only same-frame pair was `_relayout_body` + `set_zones`, which
## both build the identical row; a subject switch now rebuilds it from `set_zone_layout` and again
## from `set_zones`, with a DIFFERENT row on either side of the pair.
func _rebuild_tab_bar() -> void:
	if _tab_bar == null:
		return
	for child in _tab_bar.get_children():
		_tab_bar.remove_child(child)
		child.queue_free()
	_tab_buttons.clear()
	for zone in _zone_order():
		var tab: Control = _make_tab_button(zone)
		_tab_bar.add_child(tab)
		_tab_buttons[zone] = tab

## One tab. A `PanelContainer` (not a `Button`) wrapping a mouse-transparent row, exactly as the
## header's subject cluster does — a Button is not a Container, so a label+badge row parented to one
## is never laid out and the tabs pile up at the origin.
func _make_tab_button(zone: StringName) -> Control:
	var active := zone == _effective_tab()
	var tab := PanelContainer.new()
	tab.name = "Tab_%s" % String(zone)
	tab.mouse_filter = Control.MOUSE_FILTER_STOP
	tab.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	tab.tooltip_text = _tab_label_text(zone)
	tab.add_theme_stylebox_override("panel", _tab_stylebox(active))
	tab.gui_input.connect(func(event: InputEvent): _on_tab_gui_input(event, zone))

	# A mouse-transparent row inside the tab so the label + badge read (and click) as one tab.
	var row := HBoxContainer.new()
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_theme_constant_override("separation", TAB_SEPARATION)
	tab.add_child(row)

	var label := Label.new()
	label.text = _tab_label_text(zone)
	label.add_theme_font_size_override("font_size", TAB_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.SIGNAL if active else HudStyle.INK_FAINT)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)

	var badge := _make_tab_badge(zone)
	if badge != null:
		row.add_child(badge)
	return tab

## Left-click anywhere on a tab selects it.
func _on_tab_gui_input(event: InputEvent, zone: StringName) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
		set_active_tab(zone)

## The tab's small rounded count pill — WARN-filled when the caller marked it hot, else a quiet
## LINE_SOFT chip. Returns null when this tab carries no badge.
func _make_tab_badge(zone: StringName) -> Control:
	var badge_variant: Variant = _tab_badges.get(zone)
	if not (badge_variant is Dictionary):
		return null
	var badge_data: Dictionary = badge_variant
	var text := String(badge_data.get("text", ""))
	if text.is_empty():
		return null
	var hot := bool(badge_data.get("hot", false))
	var pill := PanelContainer.new()
	pill.mouse_filter = Control.MOUSE_FILTER_IGNORE
	pill.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	pill.add_theme_stylebox_override("panel", _tab_badge_stylebox(hot))
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", TAB_BADGE_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.GROUND if hot else HudStyle.INK_DIM)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	pill.add_child(label)
	return pill

## Tab background: transparent either way (the tab is text, not a box); the ACTIVE one wears a SIGNAL
## underline. Identical content margins in both states so selection never shifts the row.
func _tab_stylebox(active: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.0)
	sb.content_margin_left = TAB_PADDING_H
	sb.content_margin_right = TAB_PADDING_H
	sb.content_margin_top = TAB_PADDING_V
	sb.content_margin_bottom = TAB_PADDING_V
	if active:
		sb.border_width_bottom = TAB_UNDERLINE_THICKNESS
		sb.border_color = HudStyle.SIGNAL
	return sb

func _tab_badge_stylebox(hot: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.WARN if hot else HudStyle.LINE_SOFT
	sb.set_corner_radius_all(TAB_BADGE_CORNER_RADIUS)
	sb.content_margin_left = TAB_BADGE_PADDING_H
	sb.content_margin_right = TAB_BADGE_PADDING_H
	sb.content_margin_top = TAB_BADGE_PADDING_V
	sb.content_margin_bottom = TAB_BADGE_PADDING_V
	return sb

## Select a narrow-shell tab. Persisted, so a reopened session lands where the player left it. The
## wide shell shows every zone, so this only changes what the narrow shell will show.
## Guarded on `ZONE_KEYS`, not on the live layout: the selection outlives a subject switch (see
## `_effective_tab`), so a caller may name a zone the current subject does not declare.
func set_active_tab(zone: StringName) -> void:
	if not ZONE_KEYS.has(zone) or zone == _active_tab:
		return
	_active_tab = zone
	_save_prefs()
	_rebuild_tab_bar()
	_reparent_zones()
	_notify_zones_resized()

## A window resize changes the T/B panel width (hence the shell) and the clamped wide height, so
## re-choose the shell, re-anchor and re-report both the reservation and the work-zone box.
func _on_viewport_resized() -> void:
	_apply_dock_layout()
	_emit_reservation()
	_notify_zones_resized()

## Re-report `work_zone_size()` when it actually moved, so Hud re-pages its work board once per real
## geometry change rather than on every layout pass.
## **THE BAND FLANK'S COLUMN COUNT IS ITS OWN TRIGGER, beside the work box.** The two nearly always
## move together — a wider flank leaves the board less — but not always: at the cap the flank stops
## growing while the board keeps taking the rest, so a span change can hold `work_zone_size()` fixed
## across a count change, and the band zone would then keep a split authored for the other layout.
func _notify_zones_resized() -> void:
	var size := work_zone_size()
	var columns := band_zone_columns()
	if size.is_equal_approx(_last_work_zone_size) and columns == _last_band_columns:
		return
	_last_work_zone_size = size
	_last_band_columns = columns
	zones_resized.emit()

## The current window (viewport) size, the basis for the panel's long-axis extent + the height clamp.
func _viewport_size() -> Vector2:
	var vp := get_viewport()
	if vp != null:
		return vp.get_visible_rect().size
	return Vector2(PANEL_WIDTH, PANEL_HEIGHT_WIDE)

func _detach(node: Node) -> void:
	if node != null and node.get_parent() != null:
		node.get_parent().remove_child(node)

func _set_root_anchors(left: float, top: float, right: float, bottom: float) -> void:
	_root.anchor_left = left
	_root.anchor_top = top
	_root.anchor_right = right
	_root.anchor_bottom = bottom

func _set_root_offsets(left: float, top: float, right: float, bottom: float) -> void:
	_root.offset_left = left
	_root.offset_top = top
	_root.offset_right = right
	_root.offset_bottom = bottom

## Pin the accent seam to the panel's map-facing edge (opposite the dock edge).
func _position_seam() -> void:
	if _seam == null:
		return
	# **THE SEAM IS A VERTICAL-DOCK THING NOW** (issue #377). It accents the map-facing edge of the
	# reserved STRIP, which is right while the card fills that strip and wrong once it does not: on a
	# horizontal dock it would rule a line across the whole monitor with a small card floating under
	# part of it, re-drawing the full-bleed bar the islands replaced. A floating card states its own
	# edge with its border.
	_seam.visible = _is_vertical_edge(_dock_edge)
	if not _seam.visible:
		return
	match _map_facing_edge():
		SIDE_LEFT:
			_seam.anchor_left = 0.0; _seam.anchor_right = 0.0
			_seam.anchor_top = 0.0; _seam.anchor_bottom = 1.0
			_seam.offset_left = 0.0; _seam.offset_right = SEAM_THICKNESS
			_seam.offset_top = 0.0; _seam.offset_bottom = 0.0
		SIDE_RIGHT:
			_seam.anchor_left = 1.0; _seam.anchor_right = 1.0
			_seam.anchor_top = 0.0; _seam.anchor_bottom = 1.0
			_seam.offset_left = -SEAM_THICKNESS; _seam.offset_right = 0.0
			_seam.offset_top = 0.0; _seam.offset_bottom = 0.0
		SIDE_TOP:
			_seam.anchor_left = 0.0; _seam.anchor_right = 1.0
			_seam.anchor_top = 0.0; _seam.anchor_bottom = 0.0
			_seam.offset_left = 0.0; _seam.offset_right = 0.0
			_seam.offset_top = 0.0; _seam.offset_bottom = SEAM_THICKNESS
		SIDE_BOTTOM:
			_seam.anchor_left = 0.0; _seam.anchor_right = 1.0
			_seam.anchor_top = 1.0; _seam.anchor_bottom = 1.0
			_seam.offset_left = 0.0; _seam.offset_right = 0.0
			_seam.offset_top = -SEAM_THICKNESS; _seam.offset_bottom = 0.0

func _refresh_collapse_state() -> void:
	if _header_full != null:
		_header_full.visible = not _collapsed
	if _body_host != null:
		_body_host.visible = not _collapsed
	if _header_rail != null:
		_header_rail.visible = _collapsed
	_refresh_action_mount_visibility()

func _refresh_dock_cells() -> void:
	for edge in _dock_cells:
		var cell: Button = _dock_cells[edge]
		cell.add_theme_stylebox_override("normal", _dock_cell_stylebox(edge, edge == _dock_edge))
		cell.add_theme_stylebox_override("hover", _dock_cell_stylebox(edge, edge == _dock_edge, true))
		cell.add_theme_stylebox_override("pressed", _dock_cell_stylebox(edge, true))

# ---- handlers --------------------------------------------------------------

func _on_collapse_pressed() -> void:
	set_collapsed(not _collapsed)

func _on_cycle_pressed(delta: int) -> void:
	cycle_requested.emit(delta)

func _emit_reservation() -> void:
	_published_reservation = current_reservation_size()
	reservation_changed.emit(_dock_edge, _published_reservation)

## Publish the reservation again IF the cross-axis size has moved since the last emission — and stay
## silent otherwise.
##
## **THE PANEL'S RESERVED SIZE IS WHERE THE EVENT DOCK'S BAR STARTS**, through `_reservations` →
## `Main._update_event_dock_edge_offset`, so a size this panel draws at but never published is a bar
## drawn straight through the card. That could not happen while the cross axis was a pure function of
## the collapse flag, the dock edge and the viewport, because the three paths that change those all
## emit. It can now: the axis carries the active SHELL's chrome, and the shell is chosen from
## `_available_card_span()`, whose other two terms — the lateral bounds and the rail's span — arrive
## DECLARED, on setters that relayout without emitting. Measured on a TOP dock at `ui_scale` 1.35: the
## panel drew 395 while `Main` still held 360, and the bar sat 35px inside the card's lower edge.
##
## **EVERY EXISTING `_emit_reservation()` CALL SITE STAYS UNCONDITIONAL, and that is deliberate.**
## `Main._apply_reservation` is not only how the size travels — it is the hook that re-pushes the
## lateral bounds and recomputes the event dock's perpendicular insets, both of which read live HUD
## geometry that a viewport resize can move without moving this panel at all. Deduplicating those
## emissions would silently stop that recomputation; this is strictly additive.
func _republish_reservation_if_changed() -> void:
	if is_equal_approx(current_reservation_size(), _published_reservation):
		return
	_emit_reservation()

# ---- helpers ---------------------------------------------------------------

## The reserved cross-axis size. **It must never depend on content** — that independence is what
## keeps `current_reservation_size` (and therefore MapView's inset + cache invalidation) constant
## while the player edits the band, so a `+` press cannot flicker the map.
func _cross_axis_size() -> float:
	if _collapsed:
		return _collapsed_cross_axis_size()
	if _is_vertical_edge(_dock_edge):
		return PANEL_WIDTH
	return _horizontal_panel_height()

## The collapsed strip: `COLLAPSED_SIZE`, or what the RAIL's own chrome needs when that is more.
##
## **`COLLAPSED_SIZE` IS A FLOOR, NOT AN ANSWER.** The card is a `PanelContainer` and a `Control`'s
## size is clamped up to its minimum, so a rail needing more than the strip does not clip — it GROWS
## the card past the reservation, off the screen edge on a bottom dock, taking whatever sits at the
## rail's far end with it. Reserving what is actually drawn is what keeps the map's inset, the strip
## the HUD reflows off and the card one number.
##
## **It is CHROME, not content**, so it does not breach the rule above it: the rail holds the stage
## glyph, the registered verbs and the restore toggle, and those move only on a dock change, a
## collapse or a registration — each of which republishes the reservation already. A band edit cannot
## reach it.
##
## The chrome term is the card's CONTENT MARGINS alone, with no border: the stylebox's explicit
## content margins ARE its minimum size (the 1px border is drawn inside them), so this is exactly what
## the `PanelContainer` adds to the rail's own minimum. `PANEL_CHROME_H` is a declared width BUDGET
## and carries the border for its own reasons; using it here would over-reserve by 2px.
func _collapsed_cross_axis_size() -> float:
	if _header_rail == null:
		return COLLAPSED_SIZE
	var vertical := _is_vertical_edge(_dock_edge)
	var margin := float(PANEL_CONTENT_MARGIN_H if vertical else PANEL_CONTENT_MARGIN_V)
	var needed := _header_rail.get_combined_minimum_size()
	return maxf(COLLAPSED_SIZE, (needed.x if vertical else needed.y) + 2.0 * margin)

## True when the dock reserves a vertical strip (left/right → width on the x-axis).
func _is_vertical_edge(edge: int) -> bool:
	return edge == SIDE_LEFT or edge == SIDE_RIGHT

func _map_facing_edge() -> int:
	match _dock_edge:
		SIDE_LEFT:
			return SIDE_RIGHT
		SIDE_RIGHT:
			return SIDE_LEFT
		SIDE_TOP:
			return SIDE_BOTTOM
		_:
			return SIDE_TOP

func _edge_name(edge: int) -> String:
	match edge:
		SIDE_LEFT:
			return "left"
		SIDE_RIGHT:
			return "right"
		SIDE_TOP:
			return "top"
		_:
			return "bottom"

func _make_icon_button(glyph: String, tooltip: String) -> Button:
	var btn := Button.new()
	btn.text = glyph
	btn.tooltip_text = tooltip
	btn.focus_mode = Control.FOCUS_NONE
	btn.custom_minimum_size = Vector2(ICON_BUTTON_SIZE, ICON_BUTTON_SIZE)
	btn.add_theme_font_size_override("font_size", ICON_BUTTON_FONT_SIZE)
	HudStyle.apply_button(btn, "ghost")
	return btn

## A stage-sprite `TextureRect` sized to the glyph label it sits beside, so it occupies the same
## box in the header flow. Starts hidden — `set_header` decides sprite-vs-emoji per stage.
func _make_stage_glyph_sprite() -> TextureRect:
	var rect := TextureRect.new()
	rect.custom_minimum_size = STAGE_SPRITE_SIZE
	rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	# Centred in its box on both axes, mirroring the glyph labels it replaces (the cluster label
	# centres vertically, the rail label horizontally).
	rect.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	rect.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	rect.visible = false
	return rect

## Show exactly ONE of the sprite / emoji pair for a stage: the bundled art when it resolved,
## else the emoji label (a stage defined in config with no bundled art keeps its glyph).
func _apply_stage_visual(label: Label, sprite_rect: TextureRect, sprite: Texture2D, glyph: String) -> void:
	if sprite_rect != null:
		sprite_rect.texture = sprite
		sprite_rect.visible = sprite != null
	if label != null:
		label.text = glyph
		label.visible = sprite == null

## The card's own stylebox. PUBLIC and `static` because a second surface draws in it — the
## free-floating compose card (`BandComposeFloat`), which is this panel's content taken off the panel
## and must therefore read as the panel's own surface rather than as a second kind of card. One
## definition, so the two cannot drift into two looks.
static func panel_card_stylebox() -> StyleBoxFlat:
	# Square-edged card (the strip meets the screen edge — no rounding/shadow).
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.PANEL_SOLID
	sb.set_border_width_all(1)
	sb.border_color = HudStyle.LINE
	sb.content_margin_left = PANEL_CONTENT_MARGIN_H
	sb.content_margin_right = PANEL_CONTENT_MARGIN_H
	sb.content_margin_top = PANEL_CONTENT_MARGIN_V
	sb.content_margin_bottom = PANEL_CONTENT_MARGIN_V
	return sb

func _dock_cell_stylebox(edge: int, active: bool, hovered: bool = false) -> StyleBoxFlat:
	# StyleBoxFlat carries a single border color; a thicker border on the cell's
	# matching side (colored by state) reads as "dock to this edge" like the
	# prototype's edge-cells. Active = SIGNAL wash+border; hover = SIGNAL_DEEP; idle
	# = a faint bar on the LINE frame.
	var sb := StyleBoxFlat.new()
	sb.set_corner_radius_all(CORNER_RADIUS)
	sb.set_border_width_all(1)
	if active:
		sb.bg_color = HudStyle.SIGNAL_WASH
		sb.border_color = HudStyle.SIGNAL
	elif hovered:
		sb.bg_color = HudStyle.GROUND
		sb.border_color = HudStyle.SIGNAL_DEEP
	else:
		sb.bg_color = HudStyle.GROUND
		sb.border_color = HudStyle.INK_FAINT
	match edge:
		SIDE_LEFT:
			sb.border_width_left = DOCK_ACCENT_WIDTH
		SIDE_RIGHT:
			sb.border_width_right = DOCK_ACCENT_WIDTH
		SIDE_TOP:
			sb.border_width_top = DOCK_ACCENT_WIDTH
		SIDE_BOTTOM:
			sb.border_width_bottom = DOCK_ACCENT_WIDTH
	return sb

# ---- persistence -----------------------------------------------------------

func _load_prefs() -> void:
	var cfg := ConfigFile.new()
	if cfg.load(_config_path()) != OK:
		return
	var edge := int(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_EDGE, SIDE_LEFT))
	if DOCK_EDGES.has(edge):
		_dock_edge = edge
	_collapsed = bool(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_COLLAPSED, false))
	var tab := StringName(str(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_TAB, String(DEFAULT_TAB))))
	if ZONE_KEYS.has(tab):
		_active_tab = tab
	# Deliberately UNVALIDATED — the work-sort vocabulary is `BandPanelController`'s, and it is what
	# rejects an unknown value when it adopts this.
	_work_sort_pref = str(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_WORK_SORT, ""))

func _save_prefs() -> void:
	var cfg := ConfigFile.new()
	cfg.load(_config_path())   # preserve any other sections; ignore load errors
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_EDGE, _dock_edge)
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_COLLAPSED, _collapsed)
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_TAB, String(_active_tab))
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_WORK_SORT, _work_sort_pref)
	cfg.save(_config_path())

## The WORK board's persisted sort, or `""` when the player has never chosen one.
func work_sort_pref() -> String:
	return _work_sort_pref

## Remember the WORK board's sort. The value is opaque here — see `CONFIG_KEY_WORK_SORT`.
func set_work_sort_pref(value: String) -> void:
	_work_sort_pref = value
	_save_prefs()

## The prefs file actually used — the scratch override when a harness set one, else the player's.
static func _config_path() -> String:
	return config_path_override if config_path_override != "" else CONFIG_PATH
