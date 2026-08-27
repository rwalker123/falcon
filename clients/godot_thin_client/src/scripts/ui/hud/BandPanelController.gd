class_name BandPanelController
extends RefCounted

## The BAND/CITY PANEL (HUD decomposition Phase 2d, docs/plan_hud_decomposition.md): the dockable
## command center's whole render path. It owns the panel HANDLE, the three zone builders
## (`band` / `work` / `parties`) and everything under them, the panel's cycler + snapshot refresh, and
## the map-focus routing the panel's own rows use. `HudLayer` keeps the drawer dispatch that calls IN
## here (`_render_occupant_drawer`), the legacy flat `%AllocationPanel` host (`_build_allocation_panel`,
## which now just stacks this controller's three public zone builders), and the targeting machinery.
##
## Built on the LegendController / FactionReadouts / TurnOrbController / SelectionCardController /
## DrawerComposeController idiom: `HudLayer` holds one as `_bandpanel`, hands it the shared `RefCounted`
## state models BY REFERENCE (the SAME `HudBandLaborState` / `ComposeState` instances), keeps thin
## delegators for the three methods reached BY NAME (`set_band_city_panel` / `cycle_panel_band` /
## `focus_panel_band` — `Main._wire_band_city_panel` probes all three with `has_method`, and a failed
## probe fails SILENTLY), and RELAYS this controller's own five signals onto the `HudLayer` signals
## `Main` connects to. The controller never emits a `HudLayer` signal directly.
##
## THE PANEL HANDLE IS PRIVATE. Two non-moving `HudLayer` readers only ever asked "is a panel
## injected?" (`_refresh_disclosure_hosts` and `_render_occupant_drawer`, which forks the band detail
## into the dock when one is), so they ask `has_panel()` instead of holding the node.
##
## THE BOUNDARY BACK TO `HudLayer` IS TWO CALLABLES, each retained there for a reason the
## "an injection you still have to hold is relocated, not eliminated" test settles:
##   • `_emit_assign_labor` — owns the `assign_labor_requested` emit, the optimistic pending write and
##     `_after_pending_change()`. So `assign_labor` stays INDIRECT here, while the three commands with
##     no other emitter (`cancel_order` / `send_hunt_expedition` / `recall_expedition`) are signals.
##   • `_herd_label_for_id` — the herd vocabulary, also read by the targeting banner + command feed.
## The send-expedition + quarry (begin / cancel / eligibility) verbs the parties zone drives are no
## longer four Callables into HudLayer — they are a typed `TargetingController` collaborator now.
##
## Everything else arrives as a collaborator: the two state models, the selection card (roster lookup +
## pinning, for the map-focus routing, and the one selection read the vitals rows need —
## `selected_terrain_label`), the disclosure cluster (`wire_label` for the vitals row), the BAND
## DETAIL-LINE producers (`BandDetailLines`, a typed ref — the three `*_fn` Callables it replaced,
## `_unit_summary_lines` / `_expedition_summary_lines` / `_expedition_row_tooltip`, are gone with their
## adapters; the tooltip is a static `DetailFormat` call now), and a HOST node — a `RefCounted` cannot
## `add_child`, and `_confirm_destructive` parents a `ConfirmationDialog` exactly as
## `TurnOrbController` parents its fork panel.
##
## The word tables, formats and thresholds live in the topic vocab modules (`HudConst` / the matching
## `Hud*Vocab`) and the shared `DetailFormat` layer, read as `Module.X` — so a phrase is still typed in
## exactly one place.

# --- The controller's OWN signals (HudLayer connects + relays each; see the class header) ---
# Standing work was cleared for a whole scope — relayed to HudLayer.cancel_order_requested.
signal cancel_order_requested(band: Dictionary, scope: String)
# The band's KEEPING-POOL split was picked (`docs/plan_standing_upkeep.md` §2.5) — relayed to
# HudLayer.upkeep_mode_requested. Its own signal for `cancel_order_requested`'s reason: this
# controller is its only emitter. It is deliberately NOT routed through `_emit_assign_labor`, which
# staffs a role; this states a policy and carries no worker count at all.
signal upkeep_mode_requested(payload: Dictionary)
# A build was WITHDRAWN from the band's queue (`docs/plan_standing_upkeep.md` §4.6b) — the BUILD
# QUEUE block's row `✕`, relayed to HudLayer.unqueue_requested and formatted by `Main.format_unqueue`.
# **The payload is byte-identical to `DrawerComposeController`'s** ({ faction, x, y, herd_id }), which
# is what lets one command builder serve both surfaces: an unqueue names a SOURCE, and a source has
# one grammar whichever control withdrew it.
signal unqueue_requested(payload: Dictionary)
# The KIT one queued build is raised with was picked (`docs/plan_standing_upkeep.md` §4.7a ②) —
# relayed to HudLayer.build_kit_requested and formatted by `Main.format_build_kit`.
#
# **ITS OWN SIGNAL BECAUSE ITS OWN GRAMMAR AND ITS OWN SCOPE.** `assign_labor` names a BAND and a
# role; this names a SOURCE and sets a property of that source's QUEUE ENTRY, which is the one thing
# a per-band `kit` token could not say — and the sim now refuses a `kit` token on the `builders` row
# outright. `{ faction, x, y, herd_id, kit_id, default_kit_id }`: the last pair is `_kit_token`'s, so
# picking the DERIVED answer omits the token and CLEARS the override rather than pinning it.
signal build_kit_requested(payload: Dictionary)
# The band's build queue was DRAGGED into a new order (`docs/plan_standing_upkeep.md` §4.7b ③) —
# relayed to HudLayer.build_order_requested and formatted by `Main.format_build_order`.
#
# **THE QUEUE IS THE PRIORITY PROPERTY'S STORAGE** (§4.9), so the payload names a POSITION in that one
# list rather than a rank of the client's own: `{ faction, band_id, x, y, herd_id, position }`,
# 0-based, and that is the WHOLE payload. It carries no `pending_entity`: the queue is captured live,
# so there is no optimistic ordering to roll back when a send does not go (§4.9 item 9a).
signal build_order_requested(payload: Dictionary)
# A worked row was given the player's own RANK (`docs/plan_standing_upkeep.md` §4.9 item 9b) — the
# inspector strip's `Priority` picker, relayed to HudLayer.work_priority_requested and formatted by
# `Main.format_work_priority`.
#
# **ITS OWN SIGNAL BECAUSE ITS OWN COMMAND AND ITS OWN SUBJECT.** `assign_labor` states a crew and a
# floor; this states neither, and re-routing it through `_emit_work_assign` would restate a worker
# count the player did not touch. `{ faction, band_id, x, y, herd_id, level }` — a BAND and a SOURCE,
# `build_order`'s shape exactly, because the ordering it feeds is a band's own (the shedding walk
# partitions that band's rows, the pen-feed split serves that band's stores).
#
# It carries no `pending_entity`: the mark is captured LIVE off the allocation, so it arrives on this
# command's own recapture and there is no optimistic copy to roll back (§4.9 item 9a's rule).
signal work_priority_requested(payload: Dictionary)
# A build was DECLARED from a work row's `⌃` (`docs/plan_standing_upkeep.md` §4.7a ①) — relayed to
# HudLayer.improvement_requested and formatted by `Main.format_improvement`, which is unchanged.
#
# **THE COMMAND HALF OF THE PAYLOAD IS `DrawerComposeController`'s WAS, KEY FOR KEY**
# ({ faction, improvement, kind, x, y, herd_id }) — that controller no longer declares at all, so this
# is the verb's ONE emitter, and the builder it feeds never had to change.
#
# **THE THREE REMAINING KEYS ARE THE OPTIMISTIC OVERLAY'S, NOT COMMAND TOKENS** — `workers`, `floor`
# and `pending_entity`, read by no `format_*` builder. The relay records the declaration on the
# pending overlay so the queue row appears on the frame the mark is pressed, and `pending_entity` is
# the client-local handle a FAILED send hands back to `drop_pending_assign` (`assign_labor`'s own
# rollback shape, `hud-modules.md` → "AN OPTIMISTIC WRITE NEEDS A ROLLBACK").
signal improvement_requested(payload: Dictionary)
# A hunting party was dispatched from the parties zone — relayed to HudLayer.send_hunt_expedition_requested.
signal send_hunt_expedition_requested(payload: Dictionary)
# A DENIAL raid was dispatched — relayed to HudLayer.send_denial_raid_requested. **Its own signal, not
# a flag on the hunt one**, because its command grammar is closed at four tokens
# (`send_denial_raid <faction> <band> <party_workers> <fauna_id>`) — a fifth is a hard parse error —
# so a payload that could carry a floor or a fill target would be a payload the parser rejects.
signal send_denial_raid_requested(payload: Dictionary)
# A SHIPMENT was loaded and sent to another band (arc #527) — relayed to
# HudLayer.send_trade_expedition_requested. **Its own signal for the same reason denial has one**:
# `send_trade_expedition`'s tail is a repeated `food <amount>` / `material <id> <amount>` manifest,
# so its payload carries a CARGO LIST that no other party verb's grammar could express.
signal send_trade_expedition_requested(payload: Dictionary)
# A party was ordered home — relayed to HudLayer.recall_expedition_requested.
signal recall_expedition_requested(payload: Dictionary)
# A band was split in two where it stands (issue #511) — relayed to HudLayer.split_band_requested.
# **Its own signal, not a mode on the recall above**: a split makes a band where a recall dissolves a
# party, and their grammars are separate closed verbs.
signal split_band_requested(payload: Dictionary)
# Recenter + select a hex (a zone row / cycler jump) — relayed to HudLayer.alert_focus_requested.
signal alert_focus_requested(x: int, y: int)
# Pin an exact occupant on the map after that recenter — relayed to HudLayer.roster_occupant_selected.
signal roster_occupant_selected(kind: String, id: Variant)
# The header's `⚒` was pressed — open Materials & Crafting on this band. Relayed to HudLayer, which
# hands it to `CraftingPanelController`; the two controllers never talk to each other directly.
signal crafting_requested(band: Dictionary)
## The header's `▲` was pressed — open the knowledge screen. **It carries no subject**, unlike the
## `⚒` beside it: knowledge is per-FACTION, so there is nothing for this relay to resolve.
signal knowledge_requested

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
var _band_labor: HudBandLaborState = null
# The party compose's quarry + autofill one-shots live on the shared compose state.
var _compose: ComposeState = null
# Roster lookup + map pinning, for the band cycler / labor-source / party jump routing.
var _selectioncard: SelectionCardController = null
# Read for `wire_label` ONLY — the vitals row's Food/Morale carets.
var _disclosures: DisclosureController = null
# The band/party detail-line producers behind the vitals label + the parties inspector strip.
var _banddetail: BandDetailLines = null
# The HUD CanvasLayer, so this RefCounted has a node to parent the confirm dialog into and to reach
# the tree through. The compose FLOAT is the one thing that does not hang off it — it goes into
# `HudLayer.compose_host()`, the compose CanvasLayer above the event dock's (see `_mount_compose_float`).
var _host: Node = null

# --- The two retained HudLayer helpers, injected as Callables (see the class header) ---
# Each is reached through a typed adapter below rather than called raw: `Callable.call` returns
# `Variant`, which would push an untyped value into every consumer here.
var _emit_assign_labor_fn: Callable
var _herd_label_for_id_fn: Callable
# The command-targeting cluster. The send-expedition + quarry (begin/cancel/eligibility) verbs the
# parties zone drives now live here, not behind Callables into HudLayer.
var _targeting: TargetingController = null

# --- Owned state (moved off HudLayer) ---
# The dockable Band/City command center (docs/plan_band_city_dock.md §3), injected by Main through
# HudLayer's `set_band_city_panel` delegator. When present, a selected player band's detail renders
# into IT rather than the Occupants card, and the panel persists across selection changes showing the
# panel band. The panel band itself (re-resolved by entity each snapshot) lives on
# `_band_labor.panel_band()`. PRIVATE — outside readers ask `has_panel()`.
var _panel: BandCityPanel = null
# ---- Band/City zone state (persists across renders, so a filter/tab/page survives a snapshot) ----
## Which sources the work board shows, how it orders them, and which page is on screen.
## **THE DEFAULT SORT IS ONE THE PLAYER'S OWN EDIT CANNOT MOVE** (issue #460). Yield scales with
## workers, so a yield-sorted board re-ranked on every `+`/`−` press — `_repage_work_zone` re-sorts
## immediately, the row jumped out from under the pointer, and the next press landed on a different
## source. A name is a fact about the source, not about the edit in flight. `Sort by yield` is still
## one pick away in the `⋯` menu, and `set_panel` adopts the player's persisted choice over this.
var _work_filter: StringName = HudWorkVocab.WORK_FILTER_ALL
var _work_sort: StringName = HudWorkVocab.WORK_SORT_NAME
var _work_page: int = 0
## The source key open in the work inspector strip ("" = none), and WHICH of its expansions is out.
## One row at a time — the strip costs board rows, which `_work_board_capacity` subtracts.
##
## ⛔ **THE TWO PICKERS ARE MUTUALLY EXCLUSIVE, AND THIS IS WHAT MAKES THEM SO.** The strip offers a
## floor picker and a priority picker one link apart; two bools would admit a fourth state — both
## open — that `_work_inspector_height` would have to reserve for, growing the zone's tallest state
## for a combination no reading needs (a floor and a rank answer different questions and are set one
## at a time). A three-valued state cannot express it: opening either CLOSES the other by assignment.
var _work_open_key: String = ""
var _work_picker_open: StringName = HudWorkVocab.WORK_PICKER_NONE
## The party (expedition entity, as a string) whose parties-zone inspector strip is open ("" = none),
## the parties twin of `_work_open_key`. One at a time — clicking a row body toggles it.
var _party_open_key: String = ""
## The BUILD QUEUE entry (the source key) whose SETTINGS strip is open ("" = none)
## (`docs/plan_standing_upkeep.md` §4.7a ②, ③) — the queue's twin of `_work_open_key`, and the same
## contract: one at a time, clicking a row body toggles it, and the strip's height is paid for in
## `build_queue_block_height` because this zone clips.
##
## **AN ENTRY THAT CANNOT BE CONFIGURED IS NEVER OPEN.** Only a plant entry with a basket has a
## setting today (the crop); the key is dropped on any render where its entry has left the queue or
## has nothing to show, exactly as `_work_open_key` is dropped for a source that leaves the board.
var _queue_open_key: String = ""
## **IS THE BUILD QUEUE DRAWN OVER THE WHOLE WORK ZONE?** (`docs/plan_standing_upkeep.md` §4.9 item 9c).
##
## The 3-row block is a SUMMARY — what the pool is funding and what is next — and there is no cap on
## the queue behind it, so an entry past the third had no row to be seen, reordered or withdrawn from.
## Expanded, the zone draws the work head, the POOLS block and EVERY entry in a scrolling list, and
## draws no board, no chips, no pager and no work inspector at all.
##
## **IT IS ZONE MODE, WHICH IS THE PLAYER'S, so it is NOT reset on a band change** — the same
## reasoning `_work_filter` and `_work_sort` are kept under. A player who opened the full list to
## compare two bands' queues would have it fold on the first selection.
##
## ⛔ **AN EMPTY QUEUE IS NOT DRAWN IN THE MODE, AND THE FLAG IS NOT CLEARED FOR IT.** No queue means
## no block, no block means no header, and the header is the only way back out — so `_fill_work_zone`
## falls through to the COLLAPSED path for a band with nothing queued, which draws no block either and
## leaves the player nothing to be stranded in. Clearing the flag instead would cancel the mode for
## EVERY band on the first selection of an idle one, which is the band-change fold the paragraph above
## exists to prevent.
var _queue_expanded: bool = false
## The expanded list's `ScrollContainer` while it is mounted, held for the DRAG's edge auto-scroll
## alone — the pump reads its rect and writes its `scroll_vertical`, and must reach it without
## re-rendering the block the gesture is standing on. `null` in every collapsed render.
var _queue_expanded_scroll: ScrollContainer = null
## ⛔ **HOW FAR DOWN THE EXPANDED LIST THE PLAYER IS, CARRIED ACROSS EVERY REBUILD OF IT.**
##
## Every in-mode interaction frees the zone: a row's settings strip runs `_toggle_queue_settings` →
## `_repage_work_zone`, and an arrow, a drag or a withdrawal takes effect through the returning
## snapshot → `render_band`. A list rebuilt at 0 would throw the player back to the top on each of
## them AND once a turn on the snapshot — and the entries only reachable that far down are precisely
## the ones the mode exists to reach. It is the `CraftingPanel._pending_scroll` contract exactly.
##
## **IT IS AN OFFSET INTO ONE LIST, so entering the mode resets it** (`_toggle_queue_expanded`): a
## fresh expansion opens at the top of the queue, and 0 is therefore both the initial value and a
## restore that costs nothing.
var _queue_expanded_scroll_offset: int = 0
## ⛔ **THE AUTO-SCROLL'S SUB-PIXEL REMAINDER, AND THE DIRECTION IT WAS EARNED IN.**
## `ScrollContainer.scroll_vertical` is an INT and `BUILD_QUEUE_AUTOSCROLL_ROWS_PER_SECOND` at 60fps
## is ~2.8px a frame, so a truncating step would throw away most of the travel. Whole pixels are
## applied and the remainder carried; the carry is zeroed the moment the direction is 0 or flips, or
## a reversal would spend travel earned going the other way.
var _queue_autoscroll_carry: float = 0.0
var _queue_autoscroll_direction: int = 0
## **THE QUEUE ENTRY CURRENTLY BEING DRAGGED ("" = none), AND THE ROW THE DROP WOULD LAND ON**
## (`docs/plan_standing_upkeep.md` §4.7b ③).
##
## ⛔ **`_queue_drag_key` IS ALSO THE RENDER SUPPRESSOR, and a snapshot mid-gesture is why.**
## `Main._apply_snapshot` → `update_band_alerts` → `refresh_snapshot()` → `render_band()` rebuilds all
## three zones, and `populations` / `herds` move on essentially every turn — so freeing the rows under
## a live drag ends the gesture on the first pixel of movement, which is the mechanism
## `DrawerComposeController` already documents for the floor drag. The Work zone's rebuild is held off
## while this is set and runs on the drop or the cancel.
var _queue_drag_key: String = ""
var _queue_drop_key: String = ""
var _queue_drop_above: bool = true
## The drawn queue rows by entry key, kept for the drag alone: the drop indicator is a STYLEBOX swap
## on the target row, so it must reach that row without re-rendering the block it is dragging over.
var _queue_row_nodes: Dictionary = {}
## The one node whose only job is to hear `NOTIFICATION_DRAG_END` — Godot sends it to every Control in
## the tree and to nothing else, and this controller is a `RefCounted`. Without it a CANCELLED drag
## leaves the suppression flag set and the block frozen.
var _queue_drag_watcher: Control = null
## The DESTINATION PICKER's card (`docs/plan_standing_upkeep.md` §2.8), built lazily on the first
## press of a work row's build slot and reused thereafter.
##
## **IT IS A WINDOW AND SO IS NOT ZONE STATE.** Every other member in this block survives a snapshot
## because it decides what the zone RENDERS; this one holds a node that renders over the zone and
## changes no layout at all — which is the whole reason the track is a popover, the work zone having
## no pixels left to give it.
var _rung_track: PopupPanel = null
## …and the card's inner `MarginContainer`, held so a re-open clears the TRACK rather than the chrome
## that hosts it. `queue_free` is deferred, so freeing the margin renders correctly once and opens onto
## an empty card ever after — which is a defect a frame cannot show and a second open can.
var _rung_track_body: MarginContainer = null
## **THE KIT PICKED ON A BAND-WIDE ROLE CARD**, keyed `"<band entity>:<role>"` → roster id.
##
## **THE ROLE CARDS HAD NOWHERE TO KEEP PER-ROW STATE, and this is what was added rather than found.**
## The compose sheets keep theirs on `ComposeState` — the model for "what a SHEET is composing" — and
## a role card is not a sheet: it has no open/closed act to bracket the state, no source, and it
## commits on the press rather than at a Send. What it is instead is one more piece of ZONE state that
## survives a snapshot, which is this controller's own remit (`_work_filter`, `_work_open_key`,
## `_send_hunt_floor`); a field ONE cluster reads is explicitly not a state model's (`hud-modules.md`).
##
## **KEYED BY BAND, because the cycler walks bands and a bare string would carry band A's pick onto
## band B's card.** Seeded from the WIRE — the role's own `LaborAssignment.kitId`, already resolved —
## so a fresh session shows what the sim is actually running, and the composed value only ever
## overrides it after the player has picked. Never cleared: an entry is a few bytes, and
## `KitRoster.resolve_selection` re-validates it against the live roster on every render, so a stale
## id from a rebuilt world falls back to the job default rather than naming a kit the command refuses.
var _role_kit_ids: Dictionary = {}
## The live work-zone column + its band, so `zones_resized` can RE-PAGE the board in place instead of
## re-rendering all three zones.
var _work_zone_host: VBoxContainer = null
var _work_zone_band: Dictionary = {}
## The band-zone height tier the current render was built for. Written by `build_band_zone`, read by
## `_on_zones_resized` — the one straddle the band and work halves shared, resolved by keeping BOTH
## ends in this controller.
var _band_zone_tier: int = HudWorkVocab.BAND_ZONE_TIER_TALL
## How many columns the band zone was last BUILT across, beside the tier and read the same way: a
## column change needs the zones rebuilt (the split is authored, so it cannot be re-flowed in place),
## exactly as a tier change does.
var _band_zone_columns: int = 1
## The band zone's live scrolling host, kept so `_on_zones_resized` can RE-DECLARE its reserved height
## without rebuilding the zone. See `_sync_band_zone_scroll` for why a build-time declaration is not
## final.
var _band_zone_scroll: ScrollContainer = null
## **THE PANEL'S SUBJECT IS THE FACTION PAGE, not a band** (issue #450). The pinned first entry of the
## cycler, and the one bit of state that decides which of `render_band` / `render_faction` every
## re-entry into this panel resolves to — `refresh_snapshot`, `rerender` and `_on_zones_resized`'s
## tier branch all route through it, so a snapshot tick can never drop the player back onto a band.
##
## It lives HERE rather than on `HudBandLaborState` because it is a fact about what this PANEL is
## showing, not about the world — the same test that keeps `_band_zone_tier` and `_work_page` on the
## controller. `_band_labor.panel_band()` is deliberately left ALONE while it is true, so cycling off
## the faction page returns to the band the player was on rather than to the roster's first.
var _panel_is_faction: bool = false
## Which row of the faction page's Work / Parties summaries is expanded, by the entity it is about
## (`FACTION_ROW_NONE` for none). One key for BOTH tabs: the narrow shell shows one zone at a time and
## the wide shell's two lists are about different things, so a row open in each cannot arise.
var _faction_open_row: int = FACTION_ROW_NONE

## No row expanded. Not `AttentionController.OWNER_NONE` (-1), which is a REAL row on this page — the
## faction's own land alerts — so the two sentinels must differ or opening that row is indistinguishable
## from opening none.
## **THE FLOOR SLOT OF A DENIAL QUERY'S KEY, and it is a placeholder rather than an order.** A denial
## raid carries no floor — the mission is to push the herd past recovery and walk away, so there is
## nothing to leave standing — but `ForecastQuery.key_of` takes one, since the hunt question does. It
## is a constant so every denial ask for one (band, herd, kit, party) produces the SAME key; a varying
## value here would make each rebuild a fresh question with no answer.
const DENIAL_QUERY_FLOOR := 0.0

const FACTION_ROW_NONE := -2
## The faction page is PINNED FIRST in the cycler, and costs the walk one entry. Pinned rather than
## merely present so its position cannot drift as bands are founded or lost — a page that moved would
## have to be hunted for, which is the opposite of what a standing overview is for.
const FACTION_CYCLER_INDEX := 0

const FACTION_CYCLER_ENTRIES := 1

## **THE TWO SUBJECTS' BODIES, each naming its own zones, its own tab words and its own column
## widths** (`BandCityPanel.set_zone_layout`). A band's page is three zones — who they are, what they
## are doing, who is out; the faction page is those three one scale up plus a fourth, KNOWLEDGE, and
## its first tab reads `Faction` because that is the scope its content is at.
##
## They live HERE rather than on the panel because the panel is a generic dockable shell: it owns the
## zone KEYS (they index a persisted tab and a badge table) and the geometry, and the subject owns
## everything that says what a zone IS. That is what replaced `set_tab_label`, a per-zone label
## override that existed solely to rename one tab on one page.
##
## Written as `const` literals keyed by `BandCityPanel.ZONE_SPEC_*` — the field-name consts are what
## keep a typo from passing silently, and a builder helper on the panel would be a cross-class static
## CALL inside a `const` initializer, which evaluates at class load and is a load-order dependency
## this file does not need.
##
## **ONLY A BAND'S `band` ZONE DECLARES `ZONE_SPEC_MAX_COLUMNS`, and that is a statement about the
## BUILDER rather than about the width available.** `build_band_zone` below authors a two-way split of
## its blocks (they are heterogeneous, so nothing can reflow them), so a horizontal dock with room may
## hand it two columns; `FactionRollup.build_band_zone` authors no such split, so the faction page's
## `band` zone leaves the field out and stays at one — a two-column host with a one-column builder in
## it renders half a box of blank card, which is the emptiness the widening exists to REMOVE.
const BAND_ZONE_LAYOUT: Array[Dictionary] = [
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_BAND,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_BAND,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_BAND_WIDTH,
        BandCityPanel.ZONE_SPEC_MAX_COLUMNS: BandCityPanel.BAND_ZONE_MAX_COLUMNS},
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_WORK,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_WORK,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_WIDTH_EXPAND},
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_PARTIES,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_PARTIES,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_PARTY_WIDTH},
]

## **THE FACTION PAGE DECLARES THREE ZONES, THE SAME COUNT A BAND DOES.** It declared a fourth,
## `knowledge`, until the knowledge screen took the craft tracks off this panel entirely
## (`docs/plan_knowledge_screen.md` §4). What that zone held besides the tracks — SETTLING and
## DISCOVERIES — is neither earned by practice nor unlocks a verb, so neither followed them out; both
## are blocks of the `band` zone now, whose question ("who is this faction") is theirs.
##
## The shell threshold is a SUM over this list, so dropping a zone lowers the width at which the page
## tabs — from 1569 to the 1190 a band's three cost. That is the derivation working: a page with less
## to lay out needs less room to lay it out abreast.
const FACTION_ZONE_LAYOUT: Array[Dictionary] = [
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_BAND,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_FACTION,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_BAND_WIDTH},
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_WORK,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_WORK,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_WIDTH_EXPAND},
    {BandCityPanel.ZONE_SPEC_KEY: BandCityPanel.ZONE_PARTIES,
        BandCityPanel.ZONE_SPEC_LABEL: HudWorkVocab.ZONE_TAB_PARTIES,
        BandCityPanel.ZONE_SPEC_WIDTH: BandCityPanel.ZONE_PARTY_WIDTH},
]
## --- THE SHIPMENT MANIFEST'S OWN HANDLES (arc #527, see `_fill_trade_compose_sheet`) -------------
## The food row's key in `_trade_cargo_rows`. It is a MANIFEST-ROW handle, not a store key: the food
## row's identity has to be distinguishable from every material batch key, and the batch keys are
## built out of a material id plus its ratings.
const TRADE_FOOD_ROW_KEY := "cargo:food"
## What joins a batch key's parts. `|` because neither a material id nor a rating band name contains
## one, so two different piles can never key to one string.
const TRADE_BATCH_KEY_SEPARATOR := "|"
## The mass meter, as `Label` meta — the stable handle a harness reads it by. Its face carries live
## numbers and a block-glyph bar, so a text search would find whichever Label happened to hold them.
const TRADE_MASS_METER_META := "trade_mass_meter"
## The parties compose sheet: open, and which mission has been picked ("" = none yet, which is what
## keeps the party size / floor / forecast fields hidden until the mission decides them).
var _party_compose_open: bool = false
var _party_compose_mission: String = ""
## The split's ONE input. Kept beside `_send_expedition_count` rather than reusing it: the two are
## bounded by different pools (a party comes out of IDLE workers, a split out of ALL of them), so
## sharing the field would clamp one of them against the other's ceiling.
var _split_workers: int = 1
## --- THE SHIPMENT BEING LOADED (arc #527) ----------------------------------------------------
## The destination band's DURABLE `band_id` — the key `send_trade_expedition` addresses, never a
## rendered label. `NO_BAND_ID` = nothing chosen yet, which is what keeps the Send disabled.
var _trade_destination_band: int = HudConst.NO_BAND_ID
## How much FOOD the manifest carries. One number, because the larder is one commodity.
var _trade_food: float = 0.0
## How much of each MATERIAL BATCH the manifest carries: `batch key -> amount`, where the key
## identifies a pile of one material AT ONE RATING (`_trade_batch_key`). Keyed per BATCH rather than
## per material because that is what the band actually holds — the sim's store is a `BTreeMap` of
## `(material, rating band)` — and because summing two hide piles into one row would rebuild the
## retired trade scalar out of the vector that replaced it.
## The manifest belongs to ONE composing act and never survives one: `_clear_trade_manifest` is
## reached by every teardown path (the ✕, a send, a mission button, a panel-band change), because a
## manifest left standing would offer the next band goods it does not hold.
var _trade_materials: Dictionary = {}
## The live PARTIES zone column, the parties twin of `_work_zone_host` — held so the deferred
## measurement below can read what the zone's content demands off the REAL laid-out tree rather than
## off a detached one. `HudWidgets.wrap_zone` anchors this column full-rect into the panel's zone host,
## so what it demands is exactly what the host must hold.
var _parties_zone_col: VBoxContainer = null
## The compose sheet built by the current render, held so the measurement a frame later can tell that
## the sheet it is measuring is still the one in the zone.
var _party_compose_sheet: Control = null
## **THE FLOAT'S TRIGGER, AND IT IS A MEASUREMENT — never the dock edge.** What the parties zone's
## whole column demanded — head, party rows, open inspector strip AND the composed sheet — the last
## time the sheet was rendered INSIDE it: the column's own combined minimum height. A short vertical
## dock and a small window hit the same wall as a horizontal one, and an edge test misses both.
##
## **IT IS THE COLUMN'S MINIMUM, NOT THE SHEET'S OFFSET PLUS ITS MINIMUM, and the difference is not
## cosmetic.** The footer is bottom-pinned by an `EXPAND_FILL` spacer, so the spacer absorbs exactly
## the slack and `sheet_top + sheet_minimum == box height` holds BY CONSTRUCTION whenever the content
## fits — the positional read is degenerate at the boundary and answered "2px over" on a column with
## room to spare. A container's combined minimum has no such feedback: it is the sum the layout would
## need, spacer contributing nothing.
##
## **IT IS MEASURED LIVE AND A FRAME LATE, because Godot has no synchronous layout.** An unsorted
## control tree reports an autowrap `Label`'s minimum at a wrap width of ZERO — every word on its own
## line — so a build-time measurement of this sheet over-reports by hundreds of pixels and would float
## it in a side dock that holds it comfortably (measured: **1278px against a laid-out 207**).
## `_measure_party_compose` therefore waits for the deferred layout pass and reads the column the panel
## actually laid out — and it decides that the pass has happened by the SHEET's own width, never the
## column's, the column being anchored and so sized synchronously whether or not anything under it is.
##
## **IT IS A HIGH-WATER MARK for one composing act**, and it is reset by every path that ends that act
## — `_close_party_compose`, a panel-band change, the panel losing its last band. The sheet grows as
## the form is answered (a picked quarry adds the policy rungs, the party stepper, the kit row and the
## forecast), and a mark that tracked every shrink would hop the sheet back into the zone the moment a
## field cleared, which is a layout change under the player's hands.
##
## **AND IT BELONGS TO ONE BOX**, which is what `_party_compose_measured_box` records beside it. The
## mark answers "what did this sheet demand of THAT column"; a dock move from a 265px bottom strip to a
## 1055px side dock asks a different question, so the answer is dropped rather than carried into it.
var _party_compose_needed: float = 0.0
## The parties-zone box `_party_compose_needed` was measured against — `Vector2.ZERO` for "no mark".
## Compared every render by `_note_parties_zone_box`, which is what drops a mark the dock outgrew.
var _party_compose_measured_box: Vector2 = Vector2.ZERO
## One deferred measurement in flight at a time.
var _party_compose_measuring: bool = false
## The compose sheet floated off the zone (see `BandComposeFloat`). A node, so a `RefCounted` cannot
## parent it — and its parent is `HudLayer.compose_host()` rather than `_host` itself, because a
## compose surface must draw ABOVE the event dock's overlay (`_mount_compose_float`).
## Built lazily on the first render that needs it, so a session that never overflows never makes one.
var _compose_float: BandComposeFloat = null
# Compose state for the send-expedition party stepper (workers to detach), preserved across the
# resident band's per-snapshot allocation-panel re-renders.
var _send_expedition_count: int = HudConst.WORKER_STEP
# Compose state for the hunt-expedition launch FLOOR — where the raid stops, `0.0..=1.0`. **This zone
# is the SECOND launch site of `send_hunt_expedition`**, and the arc's standing rule is that the two
# entry points cannot offer different orders: a lever present on one sheet and absent on the other is
# the same defect as a lever that does nothing. The floor is the ONLY order a raid now carries — the
# fill target that used to ride beside it is retired (issue #491) — so this is the whole of that state.
var _send_hunt_floor: float = SourceForecast.DEFAULT_HARVEST_FLOOR

## **THE FORECAST QUERY SEAM**, injected by `HudLayer` after construction (`set_forecast_query`) —
## the same instance the herd drawer's expedition branch uses, so one raid asked from two entry points
## is one question with one request-id sequence.
var _forecast_query: ForecastQuery = null

func set_forecast_query(query: ForecastQuery) -> void:
    _forecast_query = query

func _init(band_labor: HudBandLaborState, compose: ComposeState,
        selectioncard: SelectionCardController, disclosures: DisclosureController,
        banddetail: BandDetailLines, host: Node,
        emit_assign_labor: Callable, herd_label_for_id: Callable,
        targeting: TargetingController, topbar: FactionReadouts) -> void:
    _topbar = topbar
    _band_labor = band_labor
    _compose = compose
    _selectioncard = selectioncard
    _disclosures = disclosures
    _banddetail = banddetail
    _host = host
    _emit_assign_labor_fn = emit_assign_labor
    _herd_label_for_id_fn = herd_label_for_id
    _targeting = targeting

## `_topbar` is held for **the player faction's own three readouts and nothing else** — its knowledge
## `faction_tracks` (the rung-ready mark on a work row, the narrow reason `DrawerComposeController`
## holds it), and since the four-zone body its `faction_sedentarization` / `faction_discovered_sites`,
## which are the Knowledge zone's other two blocks. A typed collaborator rather than a Callable
## injection, per the extraction rules.
##
## **The set is bounded by what that cluster IS, not by a count.** It is the FACTION-scope readout
## cluster; a read of anything else — a label node, a per-band figure, the turn — is a different
## collaborator's and does not belong here.
var _topbar: FactionReadouts = null

## `_attention` is held for `build_band_attention` ONLY — the faction page's Work and Parties tabs
## group that array by owner. A typed collaborator, and read for nothing else: the alerts are the
## attention model's answer, and this controller must not grow a second opinion about them.
var _attention: AttentionController = null

## Injected by `HudLayer._ready`, once `_attention` exists there.
func set_attention(attention: AttentionController) -> void:
    _attention = attention

## The player faction's {track: progress} row, threaded into every `RungGates` call.
func _player_knowledge() -> Dictionary:
    return _topbar.faction_tracks(HudConst.PLAYER_FACTION_ID) if _topbar != null else {}

## The player faction's sedentarization entry (`{score, stage}`), for the band zone's SETTLING block.
## `{}` when the snapshot has not carried one — the block renders nothing rather than a zero.
func _faction_settling() -> Dictionary:
    return _topbar.faction_sedentarization() if _topbar != null else {}

## The player faction's discovered Wondrous Sites, for the band zone's DISCOVERIES block. The raw site
## array `FactionReadouts` filters to the player faction, so the two surfaces reading it cannot
## disagree about what has been found.
func _faction_discoveries() -> Array:
    return _topbar.faction_discovered_sites() if _topbar != null else []

## Can the FACTION page's band-zone box hold every one of its blocks? Read off the panel's own answer
## for THAT zone rather than off the dock edge — a short window and a collapsed-to-nothing box hit the
## same wall as a horizontal dock, and an edge test misses both. **An unknown box answers `true`**:
## the no-dock fallback and the frame before the first layout pass are not evidence of a small box,
## and the drastic branch (silently dropping a block) must be positively justified — the same
## asymmetry `_party_compose_floats` takes.
##
## **IT MOVED HERE WITH THE TWO BLOCKS IT GATES.** It asked the same question of the retired KNOWLEDGE
## zone; Settling and Discoveries came to the band zone when the Know tab was deleted, and the tier
## came with them because the reason for it is unchanged — Discoveries is the only list on the page
## with no ceiling of its own.
func _faction_band_zone_is_full() -> bool:
    if _panel == null:
        return true
    var box: Vector2 = _panel.zone_size(BandCityPanel.ZONE_BAND)
    if box.y <= 0.0:
        return true
    return box.y >= HudWorkVocab.FACTION_BAND_FULL_MIN_HEIGHT

# ---- Typed adapters over the two injected HudLayer helpers -------------------------------------

## Issue a labor assignment. Retained on HudLayer because it owns the `assign_labor_requested` emit,
## the optimistic pending-labor write and `_after_pending_change()`.
##
## `improvement` NEVER reaches the command (issue #442) — it is recorded on the OPTIMISTIC PENDING
## overlay alone. The adapter has to carry it anyway: the trailing default is `IMPROVEMENT_NONE`, so
## omitting the argument writes "building nothing" over whatever the source is actually building, and
## `effective_worker_map` then reads that "" back for the rest of the turn.
func _emit_assign_labor(band: Dictionary, kind: String, workers: int, x: int, y: int, herd_id: String,
        floor: float, species: String = "",
        improvement: String = SourceForecast.IMPROVEMENT_NONE,
        kit_id: String = KitRoster.NO_KIT_ID,
        take_species: PackedStringArray = PackedStringArray()) -> void:
    _emit_assign_labor_fn.call(band, kind, workers, x, y, herd_id, floor, species, improvement,
        kit_id, take_species)

## A friendlier label for a herd id. Retained on HudLayer, which also feeds the targeting banner and
## the command feed from it.
func _herd_label_for_id(herd_id: String) -> String:
    return _herd_label_for_id_fn.call(herd_id)

## Player-faction check for a band (a trivial private copy of HudLayer's, the SelectionCardController
## precedent — a one-line predicate is not worth a Callable).
func _is_player_unit(unit: Dictionary) -> bool:
    return int(unit.get("faction", HudConst.PLAYER_FACTION_ID)) == HudConst.PLAYER_FACTION_ID

# ---- The inbound seam: is a panel even injected? ------------------------------------------------

## Is the dockable panel present? The two non-moving HudLayer readers
## (`_refresh_disclosure_hosts`, `_render_occupant_drawer`) only ever asked this, so they ask it here
## rather than holding the node.
func has_panel() -> bool:
    return _panel != null

## Push a registered ACTION's pip count through to the panel. A thin delegator because **the panel
## handle is private to this controller** — `HudLayer` holds no node — and because the pip's producer
## (`KnowledgePanelController.unspent_count`) is nothing this controller knows about. It is the
## `set_tab_badge` shape one seam out: the caller owns the NUMBER, the panel owns the DRAWING, and
## this owns the handle between them.
func set_action_pip(id: StringName, count: int) -> void:
    if _panel == null:
        return
    _panel.set_action_pip(id, count)

## **SHOW THE ACTING BAND'S WORK TAB** — the far end of the compose sheet's `Work tab` link
## (`docs/plan_standing_upkeep.md` §4.7a ①), relayed here by `HudLayer` because a compose sheet must
## not reach the dock itself.
##
## **IT ASSERTS THE BAND AS WELL AS THE TAB, and for one release it asserted only the tab.** Reported
## from play: with the panel on the FACTION page the link switched to the faction's Work tab — a
## rollup reading `Band 1 · 6 sources` — and not to the band's own board, where the `⌃` the sentence
## had just promised actually lives. The link delivered the player to a surface that cannot do the
## thing it said.
##
## **THE BAND IS THE ACTING ONE**, i.e. whichever the sheet's `Band:` picker names — the band whose
## `⌃` will queue the job and whose `builders` pool will pay for it. Not the selected band and not
## whatever the panel happens to be showing.
##
## **`entity`, NOT `band_id`, and that is the two-handles rule read correctly.** This is entirely
## client-side and builds no command, and `player_band_by_entity` is what every overlay reader keys
## on; `band_id` is reserved for the durable id a COMMAND names.
##
## **THE JUMP GOES THROUGH `jump_to_band_entity`**, the faction page's own drill-down path, whose note
## forbids exactly the second route this would otherwise be: *a popover row reaches a band the same
## way the cycler does (recenter, pin, render), rather than by a second path that could drift from
## it.* A third "make this band the subject" would be that drift.
##
## **THE GUARD IS *NOT ALREADY THIS BAND*, NOT *IS THE FACTION PAGE*.** The reported case is the
## faction page, but it is the special case: a panel cycled to a DIFFERENT band is the same defect and
## an `is_faction_page()` test would walk straight past it.
##
## **THE TAB IS SET AFTER THE JUMP, and the order is load-bearing.** `render_band` calls
## `set_zone_layout(BAND_ZONE_LAYOUT)`, so arriving from the four-zone faction page re-declares the
## zones; setting the tab first would have it overwritten by the render that follows.
##
## **AN UNKNOWN BAND STILL GETS THE TAB.** `jump_to_band_entity` no-ops on an entity it cannot
## resolve, and letting that swallow the whole interaction would leave a pressed link doing nothing
## visible at all — the tab switch is the half that is always right.
##
## **A NO-OP WITHOUT A PANEL, and harmless in the WIDE shell** — `set_active_tab` guards on the zone
## key and returns early when the tab is already current, and the wide shell shows every zone at once.
## Nothing here asks which shell is up: the panel owns that question and answers it once.
##
## **WHAT IT STILL DOES NOT DO IS FOCUS THE SOURCE'S ROW** on that board. The band is asserted now, so
## the row could be — it needs a public focus seam on the board keyed by the source, which is a
## different change from this one.
func show_work_tab(band_entity: int) -> void:
    if _panel == null:
        return
    if band_entity >= 0 and (_panel_is_faction \
            or int(_band_labor.panel_band().get("entity", -1)) != band_entity):
        jump_to_band_entity(band_entity)
    _panel.set_active_tab(BandCityPanel.ZONE_WORK)

# ---- Shared section-block helpers -------------------------------------------
#
# Two blocks the band zone and the legacy flat host both build; they sat beside `_build_allocation_panel`
# before the split and travelled with the zone builders that are their only callers.

## "FOOD OUTLOOK" section block: the merged larder projection chart (`FoodOutlookChart`). Returns null
## — the block is omitted — for a non-player band, a band with no real food flow (same gate as the Food
## breakdown), or one whose sources carry no projected schedule. The block is its own section rather
## than a summary line because BBCode cannot host a drawn chart.
func _build_food_outlook_block(band: Dictionary, compact: bool = false) -> VBoxContainer:
    if not (_is_player_unit(band) and DetailFormat.band_has_food_flow(band)):
        return null
    var arrivals := DetailFormat.merged_arrival_schedule(band)
    if arrivals.is_empty():
        return null
    var block := _make_alloc_block()
    block.add_child(HudWidgets.alloc_section_label(HudWorkVocab.ALLOC_HEADER_FOOD_OUTLOOK))
    var chart := FoodOutlookChart.new()
    # Drain = the people's meals, held flat across the horizon (see the chart's header): the same
    # STEADY debit the Food breakdown itemizes, so the two readouts cannot disagree. **The pens' feed
    # is no longer a term** — a pen eats its fenced pasture and its keeper's hay, never the larder — and
    # raids stay out for the reason they always did: an episodic past loss is not a steady drain.
    chart.set_projection(
        DetailFormat.band_provisions(band), arrivals,
        float(band.get("food_consumption", 0.0)), _band_labor.current_turn())
    # A short zone gets a COMPACT chart — same series, same empty marker, less height. This is the
    # whole of what the band zone's tier now buys: the chart is built either way, and drawing it
    # denser is cheaper for the reader than pushing the blocks below it under the scroll.
    if compact:
        chart.custom_minimum_size = Vector2(chart.custom_minimum_size.x, HudWorkVocab.FOOD_CHART_COMPACT_HEIGHT)
    block.add_child(chart)
    return block

## A fresh section-block VBox: the discrete, self-contained unit the Band/City panel arranges (a
## vertical stack when tall, a column-flow when wide). Rows are added into it exactly as they used to
## be added into the flat allocation container — only the parent node changes.
func _make_alloc_block() -> VBoxContainer:
    var block := VBoxContainer.new()
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.add_theme_constant_override("separation", HudWorkVocab.ALLOC_BLOCK_SEPARATION)
    return block

## ============================================================================
## Band/City panel ZONES (docs/band_panel_ux_proposal.html §02/§05)
## ----------------------------------------------------------------------------
## The panel hosts three named zones at a FIXED size (see BandCityPanel): `band`
## (who they are + what they do), `work` (the paged board of worked sources) and
## `parties`. Each builder below returns a bare VBox; `HudWidgets.wrap_zone` anchors it into
## the plain-Control zone host the panel hands out, and the legacy flat host
## (`_build_allocation_panel`, the no-dock ui_preview fallback) simply stacks the
## same three VBoxes — ONE set of builders, never a second layout.
##
## TWO of the three zones scroll — the parties zone's LIST and the BAND zone's stack —
## and the WORK board does not: it is PAGED against `BandCityPanel.work_zone_size()`,
## because rows are homogeneous and a page is a better answer than a gesture for them.
## What the two scrollers have in common is why they are exempt rather than in spite of
## it: a `ScrollContainer` reports no minimum on its scrolling axis, and each declares a
## FIXED one of its own (`HudWorkVocab.PARTIES_LIST_MIN_HEIGHT`; the band zone's own
## box), so what either holds never reaches the panel and the strip's cross-axis size —
## hence `MapView`'s inset, hence its cache — stays off the snapshot's critical path.
## See `_build_parties_list` and `_build_band_zone_scroll`.
## ============================================================================

## The interior box a zone's content may fill, in canvas px. The panel answers it from its FIXED
## geometry (`work_zone_size`), so it is a pure function of dock/collapse/window — never of content.
## The fallback keeps the no-dock ui_preview host laying out sensibly.
func _zone_box() -> Vector2:
    if _panel != null:
        var box: Vector2 = _panel.work_zone_size()
        if box.x > 0.0 and box.y > 0.0:
            return box
    return HudWorkVocab.ZONE_FALLBACK_SIZE

## The PARTIES zone's own box. Its HEIGHT is `_zone_box()`'s — every zone shares the card's one body
## height — but the wide shell's parties flank is a FIXED width where the work board's column expands,
## and the compose sheet is authored for, measured in and floated at THIS column, not that one.
func _parties_zone_box() -> Vector2:
    var box := _parties_zone_box_known()
    return box if box != Vector2.ZERO else HudWorkVocab.ZONE_FALLBACK_SIZE

## The parties zone's box **or `Vector2.ZERO` meaning "the panel cannot answer yet"** — the honest
## reading its guessed-fallback twin above cannot give. `BandCityPanel.zone_size()` returns
## ZERO while the panel is collapsed, hidden, or simply has not laid out yet, which is every frame
## before the first layout pass.
##
## **THE FLOAT DECISION MUST READ THIS ONE, NEVER THE FALLBACK.** `ZONE_FALLBACK_SIZE` is 340×360 —
## fine as a layout guess for a no-dock host, and nothing at all like the ~1055px a tall side dock
## really offers — so deciding against it turns "I do not know yet" into "this sheet overflows", and
## the float latches (see `_party_compose_needed`). Reported from play: an EMPTY compose sheet, a
## couple of hundred px tall, floated out of a left dock that held it four times over.
func _parties_zone_box_known() -> Vector2:
    if _panel == null:
        return Vector2.ZERO
    var box: Vector2 = _panel.zone_size(BandCityPanel.ZONE_PARTIES)
    return box if box.x > 0.0 and box.y > 0.0 else Vector2.ZERO

## Ask before a destructive bulk action. A `ConfirmationDialog` is a Window — like the section menu,
## it cannot disturb any zone's height. The body names what is SPARED, so "unassign all" never reads
## as "undo everything".
##
## **THIS IS THE PANEL'S ONE CONFIRM PATH** — the settle prompt, the recall prompt, `Unassign all`
## and `Recall all` — so `HudStyle.apply_dialog` lands HERE and all four wear the console's surface
## from one call. The `title` is still set although that treatment draws no title bar: it is the
## Window's NAME, which an unembedded (OS-window) dialog would show, and it costs nothing.
func _confirm_destructive(body: String, ok_text: String, on_confirm: Callable) -> void:
    var dialog := ConfirmationDialog.new()
    dialog.dialog_text = body
    dialog.ok_button_text = ok_text
    dialog.title = HudWorkVocab.CONFIRM_DIALOG_TITLE
    HudStyle.apply_dialog(dialog)
    dialog.confirmed.connect(func() -> void:
        on_confirm.call()
        dialog.queue_free())
    dialog.canceled.connect(func() -> void: dialog.queue_free())
    _host.add_child(dialog)
    dialog.popup_centered()

# ---- zone `band` ------------------------------------------------------------

## Zone `band`: vitals · people · food outlook · workforce (+ the two role cards).
## `with_vitals` is false for the legacy flat host, whose Occupants card already renders the very
## same Food/Morale/Position rows in its own `%OccupantDetail` drawer above this.
## **ON A HORIZONTAL DOCK THE BAND ZONE GROWS WIDE, NOT TALL** — the design rule for the whole panel
## ("vertical docking favours height, horizontal favours width"), and it is the architecture's rule as
## much as a preference: the panel reserves its CROSS axis, so growth along the LONG one is the one
## direction that cannot re-emit `reservation_changed` and re-invalidate the map's cache.
##
## **THE SPLIT IS AUTHORED HERE, NOT REFLOWED.** These blocks are heterogeneous — a wrapped BBCode
## label, two composition bars each with its own key, a row of role cards — so a generic "fill column A
## then spill into B" would separate a bar from its key and break the pairs that read together. The
## PANEL decides how many columns there are (`band_zone_columns()`, purely geometric); this decides
## what goes in them.
##
## **THERE ARE TWO AUTHORED SPLITS AND ONE BOOLEAN CHOOSES BETWEEN THEM: does this band have a food
## history to chart?** Both are hand-authored and hand-measured — a declared variant selected by a
## predicate, the same shape as the tier itself, and not a reflow. The split feeds NO geometry: it
## moves no column count and no reservation, so the flicker invariant is not in play here at all.
##
##   * **chart present — THE LARDER | THE PEOPLE** (vitals + outlook | PEOPLE + WORKFORCE), measured
##     **246 / 263** of a 300px box.
##   * **chart absent — vitals + PEOPLE | WORKFORCE**, measured **200 / 193**.
##
## Both pairs are MEASURED, not derived by subtracting the chart's height from the charted numbers:
## the separations and the vitals label's wrapping differ per grouping, so the arithmetic does not
## decompose and the predicted 258 / 188 both came out wrong by ~12px.
##
## PEOPLE is the only block that moves, and that is the entire difference between them.
##
## **THE PAIRING IS FORCED BY MEASUREMENT, not chosen for tidiness.** At 380px on the TALL tier the
## blocks come out vitals 130 · PEOPLE 58 · outlook 116 · WORKFORCE 193, and with the chart present
## only one of the four candidates fits a 300px box:
##
##   * vitals + PEOPLE | WORKFORCE ………………… **316** / 193 — OVERFLOWS by 16 (`band_panel_arrivals_bottom`)
##   * vitals + PEOPLE | outlook + WORKFORCE … 200 / **321** — overflows
##   * vitals | PEOPLE + outlook + WORKFORCE … 130 / **391** — overflows
##   * vitals + outlook | PEOPLE + WORKFORCE … 258 / **263** — fits, and near-balanced
##
## Take the chart away and that winner becomes 130 against 263 — one full column beside a third-full
## one, and **that is turn one**, the first frame a new player ever sees, since a band with no history
## has nothing to chart. The overflowing candidate is the one that balances once the chart's height
## leaves with it, which is why the chartless variant is the split the chart broke.
##
## **ONE column is byte-identical to the flat build**, deliberately: the blocks are emitted in BUILD
## order there, not column order, so the flat stack is still vitals · PEOPLE · outlook · WORKFORCE and
## every existing frame and every tier threshold is untouched.
##
## **EVERY BLOCK IS BUILT AT EVERY TIER, and the stack SCROLLS when the box cannot hold it.** A tier
## chooses DENSITY — the compact chart against the full-height one — and nothing else; it may not
## decide that a block does not exist. The zone used to answer a short box by building no chart and
## hint-less role cards, which at a 1920 logical viewport on a horizontal dock meant the chart and the
## hints were simply absent (one column, 299px of a 300px box) until roughly a 2250 viewport bought
## the flank a second column. See `_build_band_zone_scroll` for why that is safe.
func build_band_zone(band: Dictionary, with_vitals: bool = true) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    # COUNT FIRST, then the tier: the tier is chosen against the flank's whole stacking budget, which
    # is the column count times the box (see `_band_zone_tier_height`).
    _band_zone_columns = _band_zone_column_count()
    _band_zone_tier = _band_zone_tier_for(_band_zone_tier_height())
    # `{control, column}` in BUILD order. The two orders are deliberately separate: the flat
    # one-column stack follows this list as-is, while the split reads the `column` field — so the
    # authored pairing can put two blocks together without reordering the stack that does not use it.
    var vitals: Control = _build_vitals_label(band) if with_vitals else null
    var people := _build_people_block(band)
    # The chart is ALWAYS built; only its height answers the tier. `TALL` gets the full series, every
    # shorter tier the compact one — same series, same empty marker, drawn denser.
    var outlook: Control = _build_food_outlook_block(band,
        _band_zone_tier != HudWorkVocab.BAND_ZONE_TIER_TALL)
    var workforce := _build_workforce_block(band)
    # **ONE AUTHORED SPLIT, RE-AUTHORED AND RE-MEASURED WHEN THE KEEPING BLOCK LEFT THIS FLANK**
    # (`docs/plan_standing_upkeep.md` §4.7). The three pool cards and their fund-mode row moved to the
    # WORK tab, so the flank is back to FOUR blocks and the pairing the builders card forced
    # (`vitals + WORKFORCE | PEOPLE + keeping + outlook`) no longer describes anything. All four
    # candidates were re-measured rather than reasoned, chartless and charted:
    #
    # | split | chartless | charted |
    # |---|---|---|
    # | vitals + PEOPLE \| outlook + WORKFORCE | 174/256 = 68% | 174/372 = **47%** |
    # | vitals + outlook \| PEOPLE + WORKFORCE | 104/326 = **32%** | 220/326 = 67% |
    # | vitals \| PEOPLE + outlook + WORKFORCE | 104/326 = **32%** | 104/442 = **24%** |
    # | **vitals + PEOPLE + outlook \| WORKFORCE** | **174/256 = 68%** | **290/256 = 88%** |
    #
    # The last is the best CHARTED layout by a wide margin and ties the best CHARTLESS one, so ONE
    # split serves both and `people_column` stays retired. It reads as *who they are and where the
    # larder is going | what their hands are doing*, which is defensible on its own terms and is NOT
    # why it was chosen: it is simply the arrangement that fits.
    #
    # **THE FLOOR STAYS AT 0.65.** `band_panel_preview`'s `BAND_FLANK_BALANCE_FLOOR` was lowered
    # 0.75 → 0.65 because no split cleared 0.75 with the keeping block present, and none clears it
    # without one either — 68% is the CHARTLESS flank's arithmetic ceiling here, exactly as it was
    # before, since its three blocks total what they total however they are dealt out. The charted
    # flank has 23 points of slack at 88% and is not the binding case.
    #
    # **RE-MEASURE BEFORE MOVING A BLOCK, AND NEVER DERIVE ONE ROW FROM ANOTHER.** Separations and
    # spacing differ per grouping, and prediction by subtraction has now been wrong three times in this
    # flank's history. **The two column CONSTANTS are named for an older pairing and are positional**,
    # which is why they are not renamed with each re-authoring: they are left and right.
    var blocks: Array[Dictionary] = []
    if vitals != null:
        blocks.append({"control": vitals, "column": BAND_COLUMN_LARDER})
    if people != null:
        blocks.append({"control": people, "column": BAND_COLUMN_LARDER})
    if outlook != null:
        blocks.append({"control": outlook, "column": BAND_COLUMN_LARDER})
    blocks.append({"control": workforce, "column": BAND_COLUMN_PEOPLE})
    # BOTH layouts go inside the scroll — the flat stack and the two-column row alike. A widened flank
    # halves what each column carries but does not make either of them unable to overflow, and a rule
    # that scrolled one layout and clipped the other would be the same content loss on a wider monitor.
    # In the no-dock host there is no scroll and the blocks stack straight into the column, which is
    # what keeps that host byte-identical to the build before this zone could scroll at all.
    var stack := col
    var scroll := _build_band_zone_scroll()
    if scroll != null:
        col.add_child(scroll)
        stack = HudWidgets.make_zone_column()
        # A scrolled child must not claim the viewport's height as its own, or a short stack would
        # space its blocks down the zone; the width still fills, horizontal scrolling being disabled.
        stack.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
        scroll.add_child(stack)
    if _band_zone_columns <= 1:
        for block in blocks:
            stack.add_child(block["control"])
        return col
    var row := HBoxContainer.new()
    row.name = "BandZoneColumns"
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.size_flags_vertical = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", HudWidgets.ZONE_SECTION_SEPARATION)
    row.add_child(_band_zone_column(blocks, BAND_COLUMN_LARDER))
    row.add_child(_band_zone_column(blocks, BAND_COLUMN_PEOPLE))
    stack.add_child(row)
    return col

## The band zone's scrolling host — a `ScrollContainer` whose single child is the stack (flat, or the
## two-column row) that `build_band_zone` fills. `_build_parties_list` one flank over is the idiom, and
## the same three settings carry the same contract:
##   * **vertical AUTO** — the bar appears only when the stack really overflows, so every dock that
##     already held its band zone looks exactly as it did before this zone could scroll at all.
##   * **horizontal DISABLED** — the blocks are fitted to the flank's fixed width; a horizontal bar
##     would mean a block had been built too wide, which is a layout bug rather than something to
##     offer the player a control for. DISABLED also forces the child to the container's width, which
##     is what keeps the vitals label and both composition bars full-bleed.
##   * **a declared `custom_minimum_size.y`** — the ONE number this zone contributes to its column's
##     minimum, and it is the zone's own BOX. A `ScrollContainer` reports nothing on a scrolling axis,
##     so without it the zone would claim to need nothing and `band_panel_preview`'s content-fit walk
##     would descend past it and measure the unbounded stack instead.
##
## **THE DECLARED MINIMUM IS GEOMETRY, NEVER CONTENT, which is what keeps the flicker invariant
## intact.** `_zone_box()` is `BandCityPanel.work_zone_size()` — the panel's fixed answer from its dock
## edge, its collapse flag and the window, exactly the terms `_band_zone_tier_height()` reads — so
## nothing the snapshot says can reach it. It cannot feed back into the reservation either: the zone
## HOST is a plain `Control` that aggregates no child minimum, and this column is anchored full-rect
## into it rather than laid out by a container.
##
## **THE NO-DOCK FLAT HOST GETS NO SCROLL AT ALL — `null`, and that is not a second layout.** This
## control exists to bound a stack against a FIXED box; `_build_allocation_panel`'s host has none, it
## simply grows, and it already sits inside the subject drawer's own `DockScrollFit` scroll. Giving it
## one anyway means reserving `ZONE_FALLBACK_SIZE`'s flat 360px whatever the band holds — measured, a
## strip of dead card under the role cards and the Scout/Hunt/Deny footer pushed off the bottom of the
## drawer. Same builders, same blocks, same order; only the thing that bounds them differs, because
## only one of the two hosts has something to bound against.
func _build_band_zone_scroll() -> ScrollContainer:
    # Dropped BEFORE the no-dock fork, so the flat host cannot leave the previous dock's scroll behind
    # for `_sync_band_zone_scroll` to keep re-declaring.
    _band_zone_scroll = null
    if _panel == null:
        return null
    var scroll := ScrollContainer.new()
    scroll.name = HudWorkVocab.BAND_ZONE_SCROLL_NAME
    scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
    scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
    scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
    scroll.custom_minimum_size = Vector2(0.0, _zone_box().y)
    _band_zone_scroll = scroll
    return scroll

## Re-declare the band zone's reserved height against the box the panel is offering NOW.
##
## **THE BUILD-TIME DECLARATION IS NOT FINAL, and the reason is that `zone_size()` is composed out of
## LIVE sub-measurements.** `BandCityPanel.zone_size()` subtracts `_header_height()` — the header row's
## own combined minimum — from the card's interior, and a zone built before that row has finished
## laying out reads a box several pixels TALLER than the host it is about to be parented into. The
## scroll bakes that read into `custom_minimum_size`, so the zone column's minimum then exceeds its
## CLIPPING host and the bottom of the strip, its scrollbar included, is silently cut. Measured on a
## 1.35-scale bottom dock: **394 declared against a 385px host**, where the settled box reads 383.
##
## **IT BELONGS ON THE RESIZE RATHER THAN IN THE BUILDER**, because the panel emits `zones_resized`
## once the layout it is applying is the one it means — the first moment an honest number exists. A
## full `rerender()` would answer it too and costs three zones to correct one number.
##
## It cannot feed back: the column is anchored full-rect into a plain `Control` host that aggregates no
## child minimum, so shrinking this declaration moves no rect the panel measures, and the equality
## guard keeps an unchanged box from touching the node at all.
func _sync_band_zone_scroll() -> void:
    if not is_instance_valid(_band_zone_scroll):
        return
    var box := _zone_box().y
    if is_equal_approx(_band_zone_scroll.custom_minimum_size.y, box):
        return
    _band_zone_scroll.custom_minimum_size.y = box

## The two authored columns of a widened band flank, named for what they are ABOUT rather than by
## index: what the band has to eat, and who the band is. See `build_band_zone` for the measurement
## that forces this pairing over the other three.
const BAND_COLUMN_LARDER := 0
const BAND_COLUMN_PEOPLE := 1


## One column of a multi-column band zone. `EXPAND_FILL` on both, so the two share the flank evenly
## whatever their content demands: the flank's width is the PANEL's decision and neither column may
## claim more of it by being the busier one.
func _band_zone_column(blocks: Array[Dictionary], which: int) -> VBoxContainer:
    var column := HudWidgets.make_zone_column()
    for block in blocks:
        if int(block["column"]) == which:
            column.add_child(block["control"])
    return column

## How many columns the band zone lays out across. **The PANEL answers it**: the count is geometric —
## what the span affords — and the panel is what knows the span. Asking the CONTENT would put the
## strip's height, and therefore the map's inset, on the snapshot's critical path. One for the no-dock
## fallback host, which has no span to answer with.
func _band_zone_column_count() -> int:
    if _panel == null:
        return 1
    return _panel.band_zone_columns()

## The vitals readout — Food, Fodder, Morale and Growth, of which Food / Morale / Growth carry the
## click-to-expand disclosures (Fodder is a plain row, and there is no Output row: productivity reads
## on the WORK zone's head). Which of the optional rows appear is the producer's
## call — see `BandDetailLines.unit_summary_lines` and the `compact` note below. A
## FRESH RichTextLabel each render, so its `meta_clicked` is wired here (bound to ITSELF as the
## popover's anchor). The tint context is likewise fresh per render: it is built here, filled by
## `BandDetailLines.unit_summary_lines` as it emits the rows, and handed straight to the formatter.
func _build_vitals_label(band: Dictionary) -> RichTextLabel:
    var detail_label := RichTextLabel.new()
    detail_label.bbcode_enabled = true
    detail_label.fit_content = true
    detail_label.scroll_active = false
    detail_label.autowrap_mode = TextServer.AUTOWRAP_WORD
    detail_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    _disclosures.wire_label(detail_label)
    var ctx := DetailFormat.Context.new()
    # The SHORT tier re-spends its optional ROW: the hay larder MERGES onto the Food line. That is a
    # change of shape rather than a loss of content, which is why it survives the rule that a tier may
    # not delete a BLOCK. (It also dropped the Trade row, which arc #527 retired outright.) See
    # `band-readouts.md` for the clause and the width it was measured against.
    # No Position row either: the coordinates are IDENTITY and the panel HEADER states them
    # (`_panel_position_label`), so a vitals row would be a second telling — and one this zone pays
    # for in height. The drawer host keeps it (it has no header and renders foreign bands).
    detail_label.text = DetailFormat.detail_bbcode(
        _banddetail.unit_summary_lines(band, _selectioncard.selected_terrain_label(), ctx,
            _band_zone_tier == HudWorkVocab.BAND_ZONE_TIER_SHORT, false), ctx)
    # **THE HOVER A ROW REGISTERED, ANSWERED BY THE BLOCK** — the dormant `Fodder:` row says why it is
    # dim this way. `[hint=…]` is not parsed by this Godot build (see `DetailFormat.block_tooltip`),
    # so the label carries it; `SubjectDrawerController` does exactly this for the OTHER detail host,
    # and without it the same row is dim with no explanation in the dock alone. Empty for a block
    # whose every row is live, which shows no tooltip at all.
    detail_label.tooltip_text = DetailFormat.block_tooltip(ctx)
    return detail_label

## "PEOPLE" — who the band IS: a stacked children/working-age/elders bar plus its key and the
## dependency ratio. Returns null when the snapshot carries no age structure at all, so the block is
## OMITTED rather than rendered from a fabricated split.
## The palette is deliberately MUTED against the Workforce bar below: the two bars share a shape but
## answer different questions (composition vs allocation) and must not read as the same chart twice.
func _build_people_block(band: Dictionary) -> VBoxContainer:
    # **THE WIRE CARRIES WHOLE PEOPLE, AND THIS PANEL RENDERS THEM.** The sim keeps the brackets
    # fractional internally (the fraction is a growth accumulator), rounds them ONCE, and publishes
    # `children` + `working_age` + `elders == size`. Nothing here re-decides what a fraction means:
    # a client that apportioned the raw Scalars for itself could round a 16.6-worker band UP to 17
    # in this bar while the Workforce header below counted the sim's floored 16, the same frame.
    var children := int(band.get("children", 0))
    var working := int(band.get("working_age", 0))
    var elders := int(band.get("elders", 0))
    # The band's own head count is the header total — it IS the sum, by the sim's construction.
    var total := int(band.get("size", 0))
    if total <= 0:
        return null
    var segments: Array = []
    if children > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_CHILDREN, "count": children,
            "color": HudStyle.VOICE_PIGMENT, "tooltip": "%d %s" % [children, HudWorkVocab.PEOPLE_LABEL_CHILDREN]})
    if working > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_WORKING, "count": working,
            "color": HudStyle.INK_DIM, "tooltip": "%d %s" % [working, HudWorkVocab.PEOPLE_LABEL_WORKING]})
    if elders > 0:
        segments.append({"key": HudWorkVocab.PEOPLE_GLYPH_ELDERS, "count": elders,
            "color": HudStyle.VOICE_INK, "tooltip": "%d %s" % [elders, HudWorkVocab.PEOPLE_LABEL_ELDERS]})
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_PEOPLE, str(total)))
    block.add_child(HudWidgets.build_composition_bar(segments))
    block.add_child(HudWidgets.build_composition_key(segments, _build_dependency_chip(children, working, elders)))
    return block

## The dependency ratio chip: dependents (children + elders) per 100 working-age adults, WARN-tinted
## once the band carries more mouths than hands. Null when there is no working-age cohort to divide by.
func _build_dependency_chip(children: int, working: int, elders: int) -> Control:
    if working <= 0:
        return null
    var dependents := children + elders
    var per_hundred := HudFormat.dependency_per_hundred(dependents, working)
    var chip := Label.new()
    chip.text = HudWorkVocab.PEOPLE_DEPENDENCY_FORMAT % dependents
    chip.add_theme_font_size_override("font_size", HudWorkVocab.COMPOSITION_KEY_FONT_SIZE)
    chip.add_theme_color_override("font_color",
        HudStyle.WARN if per_hundred > HudWorkVocab.PEOPLE_DEPENDENCY_HEAVY else HudStyle.INK_FAINT)
    HudWidgets.set_label_tooltip(chip, HudFormat.dependency_tooltip(dependents, working))
    chip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    chip.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    return chip

## "WORKFORCE" — what the band DOES: a stacked Forage/Hunt/Roles/Parties/Idle bar, its key, and the
## two standing-role CARDS. Saturated against People's muted palette (see `_build_people_block`).
##
## **THE ROLE CARDS ALWAYS CARRY THEIR DESCRIPTIONS.** A `compact_cards` flag used to drop the hint
## line to a tooltip in a short box, and it is gone rather than left unused: the zone scrolls now, so
## there is nothing to buy by deleting the one line that says what a standing role IS.
func _build_workforce_block(band: Dictionary) -> VBoxContainer:
    var idle := _band_labor.effective_idle(band)
    var forage_workers := 0
    var hunt_workers := 0
    # **THE BUILDERS ARE THEIR OWN SEGMENT, ACROSS BOTH WEBS.** They are staffed labor that takes
    # nothing from any source, so they belong in neither the Forage nor the Hunt slice — and they have
    # to be SOMEWHERE, `effective_idle` having netted them out.
    #
    # **THE COUNT IS THE BAND'S `builders` POOL, not a sum of per-source build crews**
    # (`docs/plan_standing_upkeep.md` §2.5). It is a standing ROLE row like scout and warrior, which
    # is also why it is not folded into `role_workers` below: a build is a job the queue names, and
    # the segment says how many hands are on that queue at all.
    var build_workers := int(_band_labor.effective_role_workers(
        band, HudConst.LABOR_KIND_BUILDERS).get("workers", 0))
    var merged := _band_labor.effective_worker_map(band)
    for key in merged:
        var m: Dictionary = merged[key]
        var workers := int(m.get("workers", 0))
        match String(m.get("kind", "")):
            SourceForecast.LABOR_KIND_FORAGE: forage_workers += workers
            SourceForecast.LABOR_KIND_HUNT: hunt_workers += workers
    var scout_eff := _band_labor.effective_role_workers(band, HudConst.LABOR_KIND_SCOUT)
    var warrior_eff := _band_labor.effective_role_workers(band, HudConst.LABOR_KIND_WARRIOR)
    # **THE KEEPING ROLES ARE ROLES, so they are in the Roles SEGMENT even though their CARDS are a
    # block of their own.** The bar's segments partition `working_age`, and hands on `agriculture` /
    # `husbandry` are staffed labor exactly as a scout's are; leaving them out would drop them off the
    # bar entirely while `effective_idle` had already netted them out of Idle, so the key beneath
    # would stop adding up to the head count.
    var role_workers := int(scout_eff.get("workers", 0)) + int(warrior_eff.get("workers", 0)) \
        + int(_band_labor.effective_role_workers(band, HudConst.LABOR_KIND_AGRICULTURE).get("workers", 0)) \
        + int(_band_labor.effective_role_workers(band, HudConst.LABOR_KIND_HUSBANDRY).get("workers", 0))
    # Workers out with a party are NOT a segment — the sim already took them out of `working_age` on
    # launch, so a slice for them overran the denominator the segments partition. They read as the
    # header's "away" clause instead (`WORKFORCE_AWAY_FORMAT`).
    var away_workers := _band_labor.band_party_workers(band)
    var segments: Array = []
    for spec in [
        [HudWorkVocab.WORKFORCE_KEY_FORAGE, forage_workers, HudStyle.HEALTHY],
        [HudWorkVocab.WORKFORCE_KEY_HUNT, hunt_workers, HudStyle.SIGNAL],
        # …and the builders beside the two takes, because a build is staffed ON one of those sources.
        # `SIGNAL_DEEP` is the live cyan a rung under construction already wears, one step down, so it
        # reads as work-in-flight without competing with the Hunt slice it sits next to.
        [HudWorkVocab.WORKFORCE_KEY_BUILD, build_workers, HudStyle.SIGNAL_DEEP],
        [HudWorkVocab.WORKFORCE_KEY_ROLES, role_workers, HudStyle.VOICE_INK],
        # The bench's crew, between the work and the residual: `effective_idle` nets it out (a worker
        # at the bench is assigned labor), so without a segment of its own it would vanish from a bar
        # whose segments are supposed to partition the same `working_age` the header counts against.
        [HudWorkVocab.WORKFORCE_KEY_BENCH, _band_labor.bench_workers(band), HudStyle.VOICE_PIGMENT],
        [HudWorkVocab.WORKFORCE_KEY_IDLE, idle, HudStyle.INK_FAINT],
    ]:
        if int(spec[1]) > 0:
            segments.append({"key": String(spec[0]), "count": int(spec[1]), "color": spec[2],
                "tooltip": "%s: %d" % [String(spec[0]), int(spec[1])]})
    var readout := HudWorkVocab.WORKFORCE_IDLE_FORMAT % [idle, int(band.get("working_age", 0))]
    if away_workers > 0:
        readout += HudWorkVocab.WORKFORCE_AWAY_FORMAT % away_workers
    var block := HudWidgets.make_zone_block()
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_WORKFORCE, readout,
        null, HudStyle.SIGNAL if idle > 0 else HudStyle.INK_DIM,
        HudWorkVocab.WORKFORCE_AWAY_TOOLTIP if away_workers > 0 else ""))
    if not segments.is_empty():
        block.add_child(HudWidgets.build_composition_bar(segments))
        block.add_child(HudWidgets.build_composition_key(segments))
    # The FOUR standing roles as CARDS — a bordered card reads as "a standing role", not as one more
    # worked source in a list (the complaint the card treatment fixes).
    #
    # **TWO ROWS OF TWO, NOT ONE ROW OF FOUR.** A card's own controls (a stepper, and on the scouting
    # pair a kit picker) do not survive being quartered: at the narrow shell's 354px a four-abreast
    # row gives each card ~82px, which clips the kit face and the role name alike. The pairing is
    # the split the roles already have — the two EXPEDITIONARY roles above, the two KEEPING roles
    # below — so the second row reads as its own family rather than as an overflow of the first.
    var scout_row := _build_role_card_row()
    scout_row.add_child(_build_role_card(band, HudWorkVocab.ROLE_NAME_SCOUT, HudWorkVocab.SCOUT_ROLE_HINT, HudConst.LABOR_KIND_SCOUT, scout_eff, idle))
    # A visible predator within raid range turns the Warrior card's static hint into a live crimson
    # alert naming the on-guard count — the guarding role is only legible when the threat it answers is.
    var warrior_threat := _band_predator_threat_present(band)
    var warrior_hint := HudWorkVocab.WARRIOR_ROLE_HINT
    if warrior_threat:
        warrior_hint = HudWorkVocab.WARRIOR_THREAT_ALERT_FORMAT % int(warrior_eff.get("workers", 0))
    scout_row.add_child(_build_role_card(band, HudWorkVocab.ROLE_NAME_WARRIOR, warrior_hint, HudConst.LABOR_KIND_WARRIOR, warrior_eff, idle, warrior_threat))
    block.add_child(scout_row)
    return block

## One ROW of the role-card grid — the shared chrome, so the two rows cannot drift apart in spacing
## or in how they claim the zone's width.
func _build_role_card_row() -> HBoxContainer:
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", HudWorkVocab.ROLE_CARD_SEPARATION)
    return row

## **HOW THIS BAND SPLITS A KEEPING POOL IT CANNOT STRETCH** (`docs/plan_standing_upkeep.md` §2.5) —
## a two-way pick under the keeping cards, `spread` or `priority`, emitting `upkeep_mode`.
##
## **IT RENDERS ONLY WHERE THERE IS SOMETHING TO FUND**, i.e. where either web demands work this
## turn. A band holding nothing on either ladder has no split to choose, and offering the control
## there would read as a setting the player had forgotten to make.
##
## **BOTH MODES ARE ALWAYS PRESSABLE, INCLUDING THE ONE ALREADY ACTIVE.** The active mode is drawn
## `primary` and the other `ghost`, the work-board filter chip's treatment — a disabled active mode
## would leave the player unable to tell "this is selected" from "this is unavailable" on a control
## whose whole content is two words.
##
## **THE LINE BENEATH STATES THE POOL'S OWN ARITHMETIC, in both directions.** When the pools cover
## everything it says so; when they do not it names the shortfall against the demand, so the mode
## reads as an answer to a live question. Both figures are summed from the wire's per-source fields
## and neither is derived from the other (`upkeep_pool_state`).
func _build_upkeep_mode_row(band: Dictionary, plant_pool: Dictionary,
        animal_pool: Dictionary) -> VBoxContainer:
    var demand := float(plant_pool.get("demand", SourceForecast.NO_UPKEEP_DEMAND)) \
        + float(animal_pool.get("demand", SourceForecast.NO_UPKEEP_DEMAND))
    if demand < SourceForecast.UPKEEP_WORK_MIN:
        return null
    var block := HudWidgets.make_zone_block()
    block.set_meta(UPKEEP_MODE_BLOCK_META, true)
    var mode := _band_labor.upkeep_fund_mode(band)
    # **THE ROW IS THE TWO BUTTONS AND NOTHING ELSE.** It was a `Short of keepers` head over the pair
    # over an arithmetic line; the head went first (a label over a control whose whole content is two
    # words), and the line went with the per-web marks — it SUMMED both webs, so it could not name the
    # one that was short, and its covered form announced that nothing was wrong in a noun no control in
    # the game uses. Each pool card carries its own shortfall now, on its own hover.
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudWorkVocab.ROLE_CARD_SEPARATION)
    row.alignment = BoxContainer.ALIGNMENT_BEGIN
    row.add_child(_build_upkeep_mode_button(band, HudConst.UPKEEP_FUND_MODE_SPREAD,
        HudWorkVocab.UPKEEP_MODE_SPREAD_LABEL, HudWorkVocab.UPKEEP_MODE_SPREAD_HINT, mode))
    row.add_child(_build_upkeep_mode_button(band, HudConst.UPKEEP_FUND_MODE_PRIORITY,
        HudWorkVocab.UPKEEP_MODE_PRIORITY_LABEL, HudWorkVocab.UPKEEP_MODE_PRIORITY_HINT, mode))
    block.add_child(row)
    return block

## One mode's button. The press emits unconditionally — including on the active mode — because the
## command is idempotent and a press that silently did nothing is indistinguishable from a broken one.
func _build_upkeep_mode_button(band: Dictionary, mode: String, label: String, hint: String,
        active_mode: String) -> Button:
    var button := Button.new()
    button.text = label
    button.focus_mode = Control.FOCUS_NONE
    button.set_meta(UPKEEP_MODE_BUTTON_META, mode)
    # **CONTENT WIDTH, NOT HALF THE BLOCK.** The pair shares its row with the shortfall line now
    # (§4.7), so a button that expanded would take the width that line has to state a number in.
    button.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
    HudStyle.apply_button(button, "primary" if mode == active_mode else "ghost")
    HudWidgets.compact(button, HudWorkVocab.WORK_CHIP_FONT_SIZE, HudWorkVocab.WORK_CHIP_PADDING_V)
    button.tooltip_text = hint
    button.pressed.connect(func() -> void: _emit_upkeep_mode(band, mode))
    return button

## The fund-mode block and its two buttons (value = the mode each sends) as `Control` metas — the
## harnesses assert this control by ABSENCE as well as by presence, a band with nothing to keep
## rendering no row at all. **`UPKEEP_MODE_NOTE_META` retired with the arithmetic line it tagged.**
const UPKEEP_MODE_BLOCK_META := "upkeep_mode_block"
const UPKEEP_MODE_BUTTON_META := "upkeep_mode_button"

## …and whether a POOL CARD is flying the shortfall mark. The mark is a glyph inside the title's text
## and the figure is on a `tooltip_text`, neither of which a harness can assert without re-spelling the
## vocabulary; this is the card's own answer to *are you short*, so the claim is made against what the
## builder DECIDED rather than against a substring of what it drew.
const POOL_CARD_SHORT_META := "pool_card_short"

## Emit the band's fund-mode pick. Its own signal rather than a Callable into HudLayer, for
## `cancel_order_requested`'s reason: this controller is its only emitter, and the band is named by
## its DURABLE `band_id` — never its ECS entity bits, which a rollback renumbers.
func _emit_upkeep_mode(band: Dictionary, mode: String) -> void:
    emit_signal("upkeep_mode_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
        "mode": mode,
    })

## Predators Phase 3 — is a VISIBLE, camp-threatening predator within exact raid range of this band?
## A predator is any herd with `prey_sense_radius > 0`; it MENACES the camp when `attack × aggression`
## is positive (the same THREAT product the map overlay draws); and it can raid this band's larder when
## its tile is within `raid_radius` (the sim's echoed `predators.raid_radius`, per cohort) hex-distance
## of the band's tile. Herd telemetry is fog-filtered, so `world_herds()` already holds only VISIBLE
## herds — exactly the predators the player can see and should be warned about. Uses the shared wrap-aware
## `SourceForecast.hex_distance_wrapped` (never a hand-rolled distance) with the band's grid dims.
func _band_predator_threat_present(band: Dictionary) -> bool:
    var raid_radius := int(band.get("raid_radius", 0))
    if raid_radius <= 0:
        return false
    var origin := SourceForecast.band_tile(band)
    if origin.x < 0 or origin.y < 0:
        return false
    var grid_width := _band_labor.grid_width()
    var wrap := _band_labor.wrap_horizontal()
    for herd_variant in _band_labor.world_herds():
        if not (herd_variant is Dictionary):
            continue
        var herd: Dictionary = herd_variant
        if int(herd.get("prey_sense_radius", 0)) <= 0:
            continue
        if float(herd.get("attack", 0.0)) * float(herd.get("aggression", 0.0)) <= 0.0:
            continue
        var dist := SourceForecast.hex_distance_wrapped(
            origin.x, origin.y, int(herd.get("x", -1)), int(herd.get("y", -1)), grid_width, wrap)
        if dist >= 0 and dist <= raid_radius:
            return true
    return false

## One standing-role card, top to bottom: **name · the −/+ STEPPER · the KIT PICKER · its gear line ·
## the role's description.** Same `assign_labor` emit and same idle gating the role rows used to carry.
## `alert` (Predators Phase 3) tints the description crimson — the Warrior card wears it when a
## predator is within raid range, so the live "Predator nearby" warning reads as danger, not routine
## guidance.
##
## **THE CONTROLS LEAD AND THE PROSE TRAILS.** A card is read every turn and acted on with two
## controls; the description is what a player reads ONCE, to learn what the role is. Putting the
## sentence between the title and the controls made the two cards' steppers sit at different heights
## (Scout's description wraps to three lines, Warrior's to two), so the pair read as ragged and the
## thing a player actually presses moved with the length of a string. Stepper first also puts both
## cards' steppers on one line as a side effect of the ordering rather than of any alignment code.
##
## **THE KIT ROW IS THE COMPOSE SHEETS' OWN CONTROL** (`KitRoster.build_kit_row`), mounted here for
## the reason it is mounted there: a kit describes the crew, so it sits with the crew and above every
## number it moves. Two differences, both forced by the card rather than chosen — it passes NO field
## key (the card is already headed `Scout`, and ~175px cannot spend 64 of them on a third word), and
## its hint is the band-wide role's own (`KitRoster.role_hint`), because the carry-axis wording under
## a compose sheet says nothing about a vantage.
##
## **THE GEAR LINE IS THE PICKER'S HELP TEXT, so nothing may come between them.** It states what the
## SELECTED kit buys (`2-tile sight per vantage · Wayfinding 100`) and changes when the picker
## changes; `build_kit_row` returns the pair as ONE block, which is what makes that adjacency
## structural rather than a convention this function has to remember.
##
## **THIS BUILDER IS THE WORKFORCE ZONE'S ALONE NOW** (`docs/plan_standing_upkeep.md` §4.7): Scout and
## Warrior are the only two cards left with a kit picker, a gear line and prose. The three POOLS
## (`agriculture` / `husbandry` / `builders`) are the WORK tab's `_build_pool_card` — name and stepper,
## with the hint on the card's tooltip — because that zone clips and the three cards have to sit level
## across a ~354px shell.
func _build_role_card(band: Dictionary, role_name: String, hint: String, kind: String, effective: Dictionary, idle: int, alert: bool = false) -> PanelContainer:
    var workers := int(effective.get("workers", 0))
    var pending := bool(effective.get("pending", false))
    var card := PanelContainer.new()
    card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # **BOTH CARDS DRAW TO THE HEIGHT OF THE TALLER ONE, and NOTHING WAS EVER SHRINKING THEM.** The
    # `HBoxContainer` above stretches a child to the row height wherever the child asks to FILL its
    # cross axis, and `SIZE_FILL` is `Control`'s own default, which `PanelContainer` does not override
    # (measured, not assumed) — so the card RECTS were level before this ordering and are level after.
    # What read as ragged was the CONTENT: with the description second, the Scout's three-line wrap
    # pushed its picker, gear line and stepper 17px below the Warrior's and left dead space under the
    # Warrior's stepper. **It is written down rather than inherited** because it is load-bearing here
    # and free everywhere else — the descriptions and the kit names both wrap, so the alternative is a
    # hardcoded minimum height that would be wrong the next time either string changes.
    # `band_panel_preview._assert_role_cards_are_level` is the guard, sabotage-verified against
    # `SIZE_SHRINK_BEGIN` (Warrior 176px against Scout's 193px).
    card.size_flags_vertical = Control.SIZE_FILL
    card.add_theme_stylebox_override("panel", HudStyle.role_card_stylebox())
    # The description also rides the tooltip, so a long or a newly-alerting one is readable on hover
    # whatever it wraps to.
    card.tooltip_text = hint
    var col := VBoxContainer.new()
    col.add_theme_constant_override("separation", HudWorkVocab.ROLE_CARD_SEPARATION)
    card.add_child(col)
    var title := Label.new()
    title.text = role_name
    title.add_theme_font_size_override("font_size", HudWorkVocab.ROLE_CARD_NAME_FONT_SIZE)
    title.add_theme_color_override("font_color", HudStyle.WARN if pending else HudStyle.INK)
    col.add_child(title)
    # **A ROLE WITH NO PICKER NAMES NO KIT, and that is not the same as naming the default.** The
    # keeping roles mount no picker (below), and the wire names no default kit for their jobs — so a
    # resolved id here would be measured against the HUNT job's default, `_kit_token` would find them
    # unequal and every `assign_labor … agriculture <n>` would carry a `kit` tail the player never
    # chose. `NO_KIT_ID` omits the token and leaves the sim to resolve its own default, which is the
    # honest statement of what this card knows.
    var kit_id := _role_kit_id(band, kind) if _role_states_a_kit(kind) else KitRoster.NO_KIT_ID
    # **WHAT THE CARD SHOWS AND WHAT THE STEPPER SENDS ARE TWO QUESTIONS ON THE BUILDERS ROW.** The
    # gear line states the kit the pool is holding this turn, derivation included; the command carries
    # only a kit the player CHOSE, because a token pins the row against the derivation for good — and
    # with no picker on this card there is nothing to choose, so it carries none.
    var commanded_kit_id := _commanded_role_kit_id(band, kind) if _role_states_a_kit(kind) \
        else KitRoster.NO_KIT_ID
    var stepper := HBoxContainer.new()
    stepper.alignment = BoxContainer.ALIGNMENT_CENTER
    stepper.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    HudWidgets.add_stepper_controls(stepper, workers, idle > 0,
        # A BAND-WIDE ROLE (scout / warrior) works no source, so it has no escapement floor to set.
        # The sim ignores the token on those branches; the default is the honest thing to send.
        # **THE KIT RIDES THE PRESS**: `_kit_token` omits it when it equals the job default, so a
        # player who never touched the picker emits the byte-identical line they always did.
        func(n: int) -> void: _emit_assign_labor(
            band, kind, n, -1, -1, "", SourceForecast.DEFAULT_HARVEST_FLOOR,
            "", SourceForecast.IMPROVEMENT_NONE, commanded_kit_id))
    col.add_child(stepper)
    # **THE KEEPING ROLES MOUNT NO KIT PICKER** (`docs/plan_standing_upkeep.md` §2.5, open item 3).
    #
    # ⛔ **AND NOT BECAUSE A KEEPING KIT MOVES NOTHING — SINCE §4.8 IT MOVES THE KEEPING ITSELF.**
    # `tillage` and `hurdling` carry the keeping jobs, `KeepingGear::resolve` derives the tool per
    # web off the roster, and an equipped keeper supplies its bare output PLUS what the kit delivers
    # against the same unmoved demand — flint hoes are +0.5 work per keeper per turn on the plant
    # web, hurdles the same on the animal one — with `WearQuantum::UpkeepWork` billing them for it.
    # The old note here argued from an inert axis, which stopped being true the turn those two kits
    # took the jobs.
    #
    # **WHAT HOLDS IS THAT THE DERIVATION IS ALREADY THE RIGHT ANSWER AND A PICK COULD ONLY SPOIL
    # IT.** No UI path writes a kit onto a keeping row, so `LaborAllocation::named_kit_on` is empty
    # for every band a player can produce from here, the sim derives that web's own tool with no
    # player action, and `KitRoster.keeping_kit_for` states the same answer on the card. A picker
    # would store a PIN over that derivation — and the wire publishes a keeping row's kit already
    # RESOLVED, so this client cannot tell a pin from a row nobody named and would quote a
    # deliberately bare-handed pin one tool too generous (`KitRoster.keeping_kit_for`'s own note).
    # Beside that, the wire names no default kit for the `agriculture` / `husbandry` jobs (there is
    # no `defaultAgricultureKitId` twin of `defaultScoutKitId`), so the `(default)` mark would be a
    # guess and `default_kit_id` would fall through to the HUNT default.
    #
    # **WHETHER THE PICKER SHOULD EXIST AT ALL IS AN OPEN DESIGN QUESTION** (§2.5's own open item),
    # and this note does not answer it — it states what the absence rests on today, which is a
    # read-back the wire does not offer rather than an axis that does nothing.
    if kind in KIT_PICKER_ROLES:
        var kit_row := KitRoster.build_kit_row(_band_labor.kits(), kind, kit_id,
            _band_labor.default_kit_id(kind), band,
            func(picked: String) -> void: _on_role_kit_picked(band, kind, picked, workers),
            {}, "", ROLE_CARD_KIT_KEY_TEXT, true)
        if kit_row != null:
            col.add_child(kit_row)
            _lift_role_gear_line(kit_row)
    var hint_label := HudWidgets.alloc_hint_label(hint)
    if alert:
        hint_label.add_theme_color_override("font_color", HudStyle.THREAT_ACCENT)
    hint_label.custom_minimum_size = Vector2(0.0, HudWorkVocab.ROLE_CARD_HINT_HEIGHT)
    col.add_child(hint_label)
    return card

## **LIFT THE GEAR LINE OUT OF THE DESCRIPTION IT NOW SITS ON.**
##
## Stacking the two put a LIVE readout and standing boilerplate in one treatment: both go through
## `HudWidgets.alloc_hint_label`, so the card read as one grey paragraph and the tier — the only line
## on it that MOVES as gear wears — was indistinguishable from copy the player reads once. Reported
## on the prototype.
##
## **The gear line is lifted rather than the description dimmed**, because `INK_FAINT` is already the
## faintest ink this HUD has: there is nowhere below it to put the boilerplate, and the readout is the
## half that earns the emphasis anyway.
##
## **Scoped to the role card, and reached by META rather than by position.** The same builder mounts
## this row on four compose sheets, where the hint stands alone with nothing to be confused with, so
## brightening it there would move those frames for no reading. `KitRoster.KIT_HINT_META` is the
## builder's own handle on that label; a child-index walk would silently re-tint whatever the row
## gains next.
func _lift_role_gear_line(kit_row: Control) -> void:
    for child in kit_row.get_children():
        if child is Label and (child as Label).has_meta(KitRoster.KIT_HINT_META):
            (child as Label).add_theme_color_override("font_color", HudStyle.INK_DIM)
            return

## The role card mounts the shared kit row with NO field key — see `_build_role_card`.
const ROLE_CARD_KIT_KEY_TEXT := ""

## The standing roles whose card offers a KIT PICKER.
##
## **THE KEEPING PAIR IS ABSENT, AND NOT BECAUSE A PICK WOULD MOVE NOTHING.** `tillage` and
## `hurdling` carry the keeping jobs as of `docs/plan_standing_upkeep.md` §4.8, so an equipped keeper
## covers strictly more demand than a bare one. What the absence rests on — the per-web derivation
## already being exact for every band reachable from this UI, a pin the wire gives no way to read
## back, and a `(default)` mark that would fall through to the HUNT default — is stated once, on
## `_build_role_card`.
##
## ⛔ **THE BUILDERS ARE ABSENT FOR THE OPPOSITE REASON: a pick there moves too much, permanently.**
## The roster carries two builders kits, one per web, and the sim derives which one a build gets from
## **that queue ENTRY's** own branch — a per-ENTRY fact this per-BAND control could not express. The
## one thing it COULD say was a `kit` token on the `builders` row, and the sim honoured that as an
## override that won over the derivation from then on: reported from play, one click put
## `kit hurdling` on every later builders command and pinned a band raising a plant Cultivate to the
## animal web's tool with no way back (`none` means bare-handed, which is a different statement).
## **THAT TOKEN IS RETIRED — `assign_labor` REFUSES it on this role now** (§4.7a ②), and the per-entry
## override lives on the QUEUE ROW, where each entry answers for itself.
##
## **AND THE CARD NO LONGER STATES THE DERIVED KIT EITHER** (`docs/plan_standing_upkeep.md` §4.7). It
## carried the picker's old help text on a read-only line, which was one of TWO surfaces stating one
## fact: the BUILD QUEUE head one block below reads the same `_role_kit_id` and prints
## `3 builders · Tillage kit` adjacent to the jobs that kit prices. Two surfaces, one fact, and the
## queue head's is the one worth keeping — so the line, `KitRoster.role_gear_line` and the `builders`
## entry of `KitRoster.ROLE_AXES` are all retired with it.
const KIT_PICKER_ROLES := [HudConst.LABOR_KIND_SCOUT, HudConst.LABOR_KIND_WARRIOR]

## Does this role's card NAME a kit at all — the two pickers above, plus the BUILDERS, whose kit is
## still RESOLVED here for the BUILD QUEUE head even though no card renders it. The keeping pair names
## none, and that is not the same as naming the default: see `_build_role_card`.
func _role_states_a_kit(kind: String) -> bool:
    return kind in KIT_PICKER_ROLES or kind == HudConst.LABOR_KIND_BUILDERS

## The `_role_kit_ids` key for one band's one role. Two terms because the cycler walks bands: a
## per-role key alone would carry the pick made on one band onto every other band's card.
func _role_kit_key(band: Dictionary, kind: String) -> String:
    return "%d:%s" % [int(band.get("entity", -1)), kind]

## **THE KIT THIS ROLE CARD STATES** — the player's own pick where they have made one, else the kit
## the SIM is already running this role at (the row's own resolved `LaborAssignment.kitId`), else the
## job default. Resolved through `KitRoster.resolve_selection` like every compose sheet, so an id held
## over from a previous world can never reach a picker, a gear line or the command.
##
## **THE BUILD QUEUE HEADER IS ITS ONE READER ON THE `builders` ROW** (`_build_build_queue_head`) — the
## Builders card's read-only gear line was the second and is retired (§4.7). On that row the player's
## own pick is always absent, no card ever mounting a picker for it, so the wire and the derivations
## below are the whole of the answer there.
##
## **THE WIRE IS THE SEED, NOT THE FALLBACK ORDER'S END.** Reading the assignment first is what makes
## a fresh session show what is actually running rather than what a default would be; an UNSTAFFED
## role has no assignment row at all, and `resolve_selection` then lands on the job default, which is
## exactly what the sim would resolve for the first `+`.
func _role_kit_id(band: Dictionary, kind: String) -> String:
    var branch := _role_build_branch(band, kind)
    var composed := _composed_role_kit_id(band, kind)
    if composed == KitRoster.NO_KIT_ID:
        composed = HudBandLaborState.role_kit_id(band, kind)
    # **AN UNSTAFFED BUILDERS ROW IS DERIVED, not left to the list's first entry.** The wire states a
    # kit only where a row exists, and the roster's own answer for the queue head's web is exactly
    # what the sim will resolve for that role's first `+` — so the card states the kit the pool would
    # actually be handed, rather than whichever builders kit the roster happens to author first.
    if composed == KitRoster.NO_KIT_ID and kind == HudConst.LABOR_KIND_BUILDERS:
        composed = KitRoster.build_kit_for_branch(_band_labor.kits(), branch)
        # **…AND AN EMPTY QUEUE DERIVES NOTHING, WHICH IS THE `No kit` FACE AND NOT ROSTER ORDER.**
        # `resolve_selection`'s terminal fall-through is `selectable[0]` — the first entry the roster
        # authors for the job, `hurdling` on the shipped config — so with nothing composed, nothing on
        # the wire and no queue head to derive from, the card read the ANIMAL web's kit on a band
        # raising a Cultivate. Reported from play. The bare kit is the honest answer to *nothing is
        # chosen and nothing can be derived*, and it is what the sim itself falls back to
        # (`equipment.md` → rule 3, `default_kits.builders` being `none`).
        if composed == KitRoster.NO_KIT_ID:
            composed = KitRoster.bare_kit_id(_band_labor.kits(), kind)
    return KitRoster.resolve_selection(_band_labor.kits(), kind,
        _band_labor.default_kit_id(kind), composed, {}, "", branch)

## The pick the PLAYER made on this card in this session, `NO_KIT_ID` when they have made none — a
## different question from `_role_kit_id`, which falls back to the wire. The BUILDERS row is where
## the two must not be confused; see `_commanded_role_kit_id`.
func _composed_role_kit_id(band: Dictionary, kind: String) -> String:
    return String(_role_kit_ids.get(_role_kit_key(band, kind), KitRoster.NO_KIT_ID))

## **THE KIT THE STEPPER'S COMMAND CARRIES — the player's own pick, and on the BUILDERS row NOTHING
## ELSE.**
##
## Every other role publishes either the kit named on its row or its job's default, so re-stating it
## on a `+` is a no-op. The builders row is not that: the sim resolves it **per queue entry** and
## publishes the DERIVED answer (`equipment.md` → "THE WIRE STATES THE DERIVED KIT"), so echoing the
## derived id back would state a choice the player never made.
##
## **AND THE ROW'S OVERRIDE IS RETIRED OUTRIGHT** (`docs/plan_standing_upkeep.md` §4.7a ②):
## `assign_labor` now REFUSES a `kit` token on the `builders` role, so a token sent here is a command
## failure rather than a pin that wins. The per-entry override lives on the queue row
## (`_emit_build_kit` → `build_kit`), which is where an entry can answer for itself.
##
## `Main._kit_token` omits an empty selection, so the line carries no `kit` token and the sim keeps
## deriving.
##
## ⛔ **ON THE BUILDERS ROW THIS FORK NOW ALWAYS ANSWERS `NO_KIT_ID`, AND THAT IS THE POINT — do not
## "simplify" it away.** With the picker gone nothing writes `_role_kit_ids` for `builders`, so
## `_composed_role_kit_id` has nothing to return and the stepper emits `assign_labor … builders <n>`
## with no tail at all. The fork is what STATES that the omission is deliberate rather than an
## oversight: collapsing it to the other roles' `_role_kit_id` would echo the DERIVED id back — and
## the sim REFUSES a `kit` token on this role now, so the line would fail outright.
func _commanded_role_kit_id(band: Dictionary, kind: String) -> String:
    if kind == HudConst.LABOR_KIND_BUILDERS:
        return _composed_role_kit_id(band, kind)
    return _role_kit_id(band, kind)

## **THE WEB THIS ROLE'S KIT IS RESOLVED AGAINST** — the build branch of the band's queue HEAD on the
## builders row, and `BUILD_BRANCH_NONE` on every other role, none of which raises anything.
##
## It rides `_role_kit_id`'s `KitRoster.resolve_selection`, whose selectable list applies
## `kit_offer`'s build-branch rule: a hoe in front of a `Tame` is as inapplicable as a snare in front
## of a Red Deer, so the card and the queue header land on the kit the SIM would hand this pool
## rather than on one its tool cannot serve.
##
## **AND WHERE THE WIRE HAS PLACED NO HEAD, THE PENDING ONE ANSWERS** (`docs/plan_standing_upkeep.md`
## §4.7a ①). `head_build_branch` needs the entry the SIM put at position 0, and a declaration made
## THIS turn has no position at all — so a band whose only queued job is the one just declared derived
## nothing, fell through to `bare_kit_id`, and the queue head read **`3 builders · No kit`** over a
## Cultivate. Reported from play.
##
## **THE FALL-THROUGH WAS RIGHT WHILE AN EMPTY QUEUE WAS THE ONLY UNDERIVABLE CASE.** The work row's
## `⌃` makes a pending-only queue the ORDINARY state — every declaration passes through it for exactly
## one turn — so the honest answer is the one the client already has: `_build_queue_models` orders the
## confirmed entries and then the pending ones, so its first entry is the head whichever kind it is,
## and a source's web is a property of the source. `bare_kit_id` survives for a queue with genuinely
## nothing in it, which is the case it was written for.
func _role_build_branch(band: Dictionary, kind: String) -> String:
    if kind != HudConst.LABOR_KIND_BUILDERS:
        return KitRoster.BUILD_BRANCH_NONE
    var branch := _band_labor.head_build_branch(band)
    if branch != KitRoster.BUILD_BRANCH_NONE:
        return branch
    return _pending_head_build_branch(band)

## The branch of the FIRST entry in this band's queue as the client renders it — confirmed entries in
## wire order, then the declarations the wire has not placed, which is `_build_queue_models`' own
## ordering. `BUILD_BRANCH_NONE` for a band with nothing queued at all.
##
## **IT IS THE SAME LIST THE BLOCK DRAWS**, deliberately: a second walk of the overlay would be a
## second opinion about which entry is at the head, and the header names the kit the block's own top
## row is being raised with.
func _pending_head_build_branch(band: Dictionary) -> String:
    var queued := _build_queue_models(band, _work_source_models(band, 0))
    if queued.is_empty():
        return KitRoster.BUILD_BRANCH_NONE
    return KitRoster.build_branch_for_kind(String((queued[0] as Dictionary).get("kind", "")))

## A kit picked on a role card. **It EMITS on the press, like the work inspector's policy picker and
## unlike a compose sheet's** — this card has no Send to commit at, so a pick that only sat in client
## state would leave the sim running a different kit than the one the card now names, which is the
## silent substitution the whole kit arc exists to prevent. The command re-states the role's CURRENT
## head count; only the kit token moves.
##
## **AN UNSTAFFED ROLE EMITS NOTHING**, because `assign_labor … <role> 0` drops the assignment and the
## sim resolves no kit for it (`equipment.md` → "Unassigning resolves NO kit"). The pick is held here
## and rides the first `+`, which is the press that creates the row the kit belongs to.
func _on_role_kit_picked(band: Dictionary, kind: String, kit_id: String, workers: int) -> void:
    _role_kit_ids[_role_kit_key(band, kind)] = kit_id
    if workers > 0:
        _emit_assign_labor(band, kind, workers, -1, -1, "",
            SourceForecast.DEFAULT_HARVEST_FLOOR, "", SourceForecast.IMPROVEMENT_NONE, kit_id)
        return
    rerender()

# ---- zone `work` (the paged board) ------------------------------------------

## Zone `work`: header · filter chips · the paged board · pager · inspector strip. The column keeps a
## reference to itself so `zones_resized` can RE-PAGE in place rather than re-render the whole panel.
func build_work_zone(band: Dictionary) -> VBoxContainer:
    var col := HudWidgets.make_zone_column()
    col.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
    _work_zone_host = col
    _work_zone_band = band
    _fill_work_zone(col, band)
    return col

## The panel's `zones_resized` handler. Re-paging the work board is the cheap common case, but the
## BAND zone picks a DENSITY tier at build time (the chart's height), so a tier change needs the zones
## rebuilt rather than the board re-paged — the chart's `custom_minimum_size` is written once and
## cannot be re-flowed in place.
func _on_zones_resized() -> void:
    # The faction page has no height tier and no paged board — its lists are bounded by a row COUNT
    # rather than measured against the box — so a resize is simply a re-render. Falling through would
    # compare against the tier the last BAND render left behind and, on a match, re-page a work host
    # this page does not own.
    if _panel_is_faction:
        rerender()
        return
    # The COLUMN COUNT is the second reason to rebuild rather than re-page, and for the same reason as
    # the tier: the band zone's split across columns is AUTHORED at build time, so a flank that has
    # gained or lost a column cannot be re-flowed in place — it would keep a layout built for a
    # different geometry, one column of it clipped by a host that no longer matches.
    if _band_zone_tier != _band_zone_tier_for(_band_zone_tier_height()) \
            or _band_zone_columns != _band_zone_column_count():
        rerender()
        return
    # The band zone keeps its build, but the height it RESERVED was taken against a box the panel was
    # still settling — see `_sync_band_zone_scroll`. Correcting it is one assignment; rebuilding the
    # zone for it would be three.
    _sync_band_zone_scroll()
    _repage_work_zone()

## The height the band zone's TIER is chosen against: the box times the number of columns the flank
## lays out across.
##
## **A SECOND COLUMN IS A SECOND COLUMN'S WORTH OF STACKING ROOM**, and the tier is the question "how
## much room do these blocks have?" — so the budget it is asked about has to be the whole flank's, not
## one column's. Measured at 1920 on a bottom dock: one column packs vitals + PEOPLE + WORKFORCE into
## 299px of a 300px box and can afford only the densest drawing; two columns carry 148px each and
## leave 152px of every column blank, which is the same emptiness the widening was supposed to remove,
## moved down the card. Times the count, that 300px box reads as the 600px of stacking the flank
## actually offers, and the tier rises to the full-height food-outlook chart.
##
## **WHAT THE TIER NO LONGER DECIDES IS WHETHER A BLOCK EXISTS.** It used to: below the chart
## threshold the zone built no chart and hint-less role cards, so at this very geometry the chart and
## the hints were GONE and came back only when a second column was earned. Every block is built at
## every tier now and the stack scrolls (`_build_band_zone_scroll`); the tier is density alone.
##
## **STILL PURELY GEOMETRIC.** Both terms are: the box is the panel's fixed geometry and the count is
## `band_zone_columns()`, which reads only the span. So the tier cannot become a function of the
## snapshot, and the strip's height — hence `MapView`'s inset — stays off the content's critical path.
##
## **ONE column multiplies by one**, so every vertical dock, every narrow shell and every one-column
## horizontal dock is arithmetically untouched.
func _band_zone_tier_height() -> float:
    return _zone_box().y * float(_band_zone_column_count())

## Which DENSITY tier the band zone's height affords (see `BAND_ZONE_*_MIN_HEIGHT`). It chooses how
## tightly the blocks are drawn and never which of them are built.
func _band_zone_tier_for(zone_height: float) -> int:
    if zone_height >= HudWorkVocab.BAND_ZONE_TALL_MIN_HEIGHT:
        return HudWorkVocab.BAND_ZONE_TIER_TALL
    if zone_height >= HudWorkVocab.BAND_ZONE_CHART_MIN_HEIGHT:
        return HudWorkVocab.BAND_ZONE_TIER_COMPACT
    return HudWorkVocab.BAND_ZONE_TIER_SHORT

## Re-page the live work board against the panel's new zone box. Only the board is rebuilt — the
## other two zones are untouched.
func _repage_work_zone() -> void:
    if _queue_drag_in_flight():
        return
    if _work_zone_host == null or not is_instance_valid(_work_zone_host) or _work_zone_band.is_empty():
        return
    HudWidgets.clear_children(_work_zone_host)
    _fill_work_zone(_work_zone_host, _work_zone_band)

## ⛔ **IS A QUEUE ROW BEING DRAGGED RIGHT NOW? Then this zone MUST NOT REBUILD**
## (`docs/plan_standing_upkeep.md` §4.7b ③). `Main._apply_snapshot` → `update_band_alerts` →
## `refresh_snapshot()` → `render_band()` rebuilds all three zones, and the band's `populations` /
## `herds` move on essentially every turn — so a snapshot arriving mid-gesture FREES the row the
## pointer is holding and Godot ends the drag on the first pixel of movement. It is the same mechanism
## `DrawerComposeController` documents for the floor drag.
##
## **ONE FLAG, READ AT BOTH DOORS.** `_repage_work_zone` is the resize/toggle path (so a window resize
## cannot do it either) and `render_band` is the snapshot path; `_on_queue_drag_end` re-renders once
## the gesture is over, whether it dropped or was cancelled.
func _queue_drag_in_flight() -> bool:
    return _queue_drag_key != ""

func _fill_work_zone(col: VBoxContainer, band: Dictionary) -> void:
    _ensure_queue_drag_watcher()
    # ⛔ **THE PLAYER'S PLACE IN THE LIST IS TAKEN OFF THE OUTGOING NODE, HERE, BECAUSE THIS IS THE
    # LAST MOMENT IT EXISTS.** Both fill paths reach this line with the previous list still readable —
    # `_repage_work_zone` has `queue_free`d it (deferred, so it is still a valid instance) and
    # `render_band` builds the new zone BEFORE `set_zones` frees the old one — and neither offers
    # another hook. `_build_build_queue_expanded` restores it onto the list it is about to build.
    if _queue_expanded_scroll != null and is_instance_valid(_queue_expanded_scroll):
        _queue_expanded_scroll_offset = _queue_expanded_scroll.scroll_vertical
    # The previous fill's nodes are about to be freed, so the auto-scroll's handle is dropped before
    # anything can read a dangling one; the expanded builder is what re-seats it.
    _queue_expanded_scroll = null
    # **THE DESTINATION TRACK IS DISMISSED BY ANY RE-FILL** (`docs/plan_standing_upkeep.md` §2.8). The
    # card is anchored to a row this pass is about to free, and its every figure is a function of the
    # source's position, the faction's knowledge and the queue — so a card left up over the rebuilt
    # board would be pointing at nothing and quoting last turn. It is a momentary picker, not a panel:
    # re-opening it is one press of the mark that is about to be redrawn.
    _dismiss_rung_track()
    var idle := _band_labor.effective_idle(band)
    var models := _work_source_models(band, idle)
    col.add_child(_build_work_head(band, models,
        _work_component_sum(models, "rate"),
        _work_component_sum(models, "fodder_rate")))
    # **THE POOLS COME FIRST, ABOVE THE QUEUE THEY FUND** (`docs/plan_standing_upkeep.md` §4.7): the
    # hands, then the jobs those hands are on, then the sources. It renders on EVERY band, including
    # one whose board is empty — three steppers at 0 is a live control, not furniture.
    # **THE QUEUE IS DERIVED BEFORE THE POOLS BLOCK IS BUILT, and only the DERIVATION moved** — the
    # render order is unchanged (pools, then the queue block below). The block needs the queue's
    # entries to mark a keeping pool a queued job is about to need, which is what makes that warning
    # arrive on the frame the job is declared rather than a turn later.
    var queued := _build_queue_models(band, models)
    var pools := _build_pools_block(band, queued)
    col.add_child(pools)
    # **THE QUEUE SITS ABOVE THE CHIPS, DELIBERATELY.** The chips filter the BOARD; the queue is the
    # band's own ordered list rather than a view of that board, so a block beneath them would read as
    # a filtered subset of it. It is derived from the FULL model set for the same reason — a chip must
    # not be able to move it (that derivation is a few lines up now, the pools block having to read it).
    # **THE ROW CAP IS THE ZONE'S ANSWER, NOT THE CONSTANT'S** (§4.7): the pools block took ~82px out
    # of a 300px horizontal work zone, which is more than the authored ceiling can give back, while
    # the narrow shell's swapped host has 400px spare at that same ceiling. Resolved ONCE here so the
    # block and the capacity below cannot cap differently.
    var pools_fund_mode := bool(pools.get_meta(HudWorkVocab.POOLS_BLOCK_META))
    # **THE EXPANSION IS A FORK, NOT A WIDENING** (§4.9 item 9c). The whole
    # queue takes the whole zone: the work head above it, the POOLS block directly above the list they
    # fund, and every entry in a scrolling list — and NO chips, no board, no pager, no inspector and
    # no `+N more`, so `_work_board_capacity` is not consulted at all in this mode.
    # ⛔ **AN EMPTY QUEUE FALLS THROUGH TO THE COLLAPSED PATH AND THE FLAG IS LEFT ALONE.** No queue
    # means no block, no block means no header, and the header is the only way back — but the fall
    # through already draws no block, so nothing is stranded. CLEARING the flag here (which it did)
    # cancels the mode for every band the moment an idle one is selected, which is exactly the
    # band-change fold `_queue_expanded` is documented as not doing.
    if _queue_expanded and not queued.is_empty():
        col.add_child(_build_build_queue_expanded(band, queued, pools_fund_mode))
        return
    var queue_rows_max := HudWorkVocab.build_queue_rows_max(_zone_box().y,
        pools_fund_mode, queued.size())
    if not queued.is_empty():
        col.add_child(_build_build_queue_block(band, queued, queue_rows_max))
    # BEFORE the chips are built, so the pressed chip is always one that actually renders.
    _reconcile_work_filter(models)
    col.add_child(_build_work_chips(models))
    var filtered := _filter_work_models(models)
    _sort_work_models(filtered)
    # Drop an inspector pinned to a source that has left the filtered set (unassigned, filtered out).
    var inspected := _find_work_model(filtered, _work_open_key)
    if inspected.is_empty():
        _work_open_key = ""
        _work_picker_open = HudWorkVocab.WORK_PICKER_NONE
    if filtered.is_empty():
        var hint := HudWidgets.alloc_hint_label(HudWorkVocab.WORK_EMPTY_HINT)
        hint.size_flags_vertical = Control.SIZE_EXPAND_FILL
        col.add_child(hint)
        return
    # The pools block's own reserved height is read back off the block it just built, so the number
    # subtracted here and the number drawn cannot come from two different answers to "is the fund-mode
    # row rendering?" — `_build_upkeep_mode_row` is the one decider and the meta is its record.
    # **THE SETTINGS STRIP IS CHARGED TO THE BOARD, exactly as the work inspector is.** It is asked
    # of the SAME resolver the block builds from, so the height reserved here and the height drawn
    # there are one decision; asking twice is idempotent (it only prunes a stale key).
    var queue_settings := _queue_settings_state(band, queued, mini(queued.size(), queue_rows_max))
    var capacity := _work_board_capacity(filtered.size(), inspected, queued.size(),
        queue_rows_max, pools_fund_mode,
        int(queue_settings["legs"]), bool(queue_settings["crop"]),
        bool(queue_settings["kit"]), bool(queue_settings["one_line"]))
    var page_size := int(capacity["page_size"])
    var pages := int(capacity["pages"])
    _work_page = clampi(_work_page, 0, maxi(pages - 1, 0))
    var start := _work_page * page_size
    col.add_child(_build_work_board(band, filtered.slice(start, start + page_size),
        int(capacity["cols"]), int(capacity["rows_per_col"])))
    if pages > 1:
        col.add_child(_build_work_pager(pages, start, mini(start + page_size, filtered.size()), filtered.size()))
    if not inspected.is_empty():
        col.add_child(_build_work_inspector(band, inspected))

## Board capacity, derived ENTIRELY from the fixed zone box:
##   cols        = zone width / WORK_COLUMN_MIN_WIDTH, clamped to 1..WORK_MAX_COLUMNS
##   rows_per_col = remaining height / WORK_ROW_TWO_LINE_HEIGHT, after the head, chips, inspector and (when it
##                  is actually needed) the pager — each of which reserves the very height it draws at.
## The pager is circular (it only exists when one page cannot hold everything, but it costs a row), so
## it is resolved in two passes: measure without it, and if that still needs more than one page, remeasure.
## `inspected` is the open inspector's model, EMPTY when none is open.
##
## **THE BUILD QUEUE BLOCK IS PAID FOR HERE OR IT SLICES THE BOARD** (§4.6b). This zone
## `clip_contents`, so a block that draws without being subtracted from the board's room takes its
## height off the bottom of the zone silently — no overflow, no warning, just fewer rows than the
## pager thinks it drew. `queue_rows` is the ENTRY count, and the height comes from the same
## `HudWorkVocab.build_queue_block_height` the builder sizes itself with, plus one more block
## separation for the gap the block adds to the column.
##
## **AND SO IS THE POOLS BLOCK** (§4.7), through the identical arrangement —
## `HudWorkVocab.pools_block_height` is both the block's own `custom_minimum_size` and this term, plus
## one more separation for the gap. Unlike the queue's it is ALWAYS charged: the block always renders.
func _work_board_capacity(count: int, inspected: Dictionary, queue_rows: int, queue_rows_max: int,
        pools_fund_mode: bool, queue_settings_legs: int = 0,
        queue_settings_crop: bool = false, queue_settings_kit: bool = false,
        queue_settings_one_line: bool = true) -> Dictionary:
    var box := _zone_box()
    var inspector_h := 0.0 if inspected.is_empty() else _work_inspector_height(inspected)
    var queue_h := HudWorkVocab.build_queue_block_height(queue_rows, queue_rows_max,
        queue_settings_legs, queue_settings_crop, queue_settings_kit, queue_settings_one_line)
    var pools_h := HudWorkVocab.pools_block_height(pools_fund_mode)
    var gaps := HudWorkVocab.WORK_ZONE_GAP_COUNT + 1.0
    if queue_h > 0.0:
        gaps += 1.0
    var chrome := HudWorkVocab.ZONE_HEAD_HEIGHT + HudWorkVocab.WORK_CHIPS_HEIGHT + inspector_h \
        + queue_h + pools_h + float(HudWorkVocab.ZONE_BLOCK_SEPARATION) * gaps
    var rows := maxi(1, int((box.y - chrome) / HudWorkVocab.WORK_ROW_TWO_LINE_HEIGHT))
    var cols := _declare_work_columns(count, rows)
    var pages := ceili(float(count) / float(maxi(cols * rows, 1)))
    if pages > 1:
        rows = maxi(1, int((box.y - chrome - HudWorkVocab.WORK_PAGER_HEIGHT - float(HudWorkVocab.ZONE_BLOCK_SEPARATION)) / HudWorkVocab.WORK_ROW_TWO_LINE_HEIGHT))
        cols = _declare_work_columns(count, rows)
        pages = ceili(float(count) / float(maxi(cols * rows, 1)))
    return {"cols": cols, "rows_per_col": rows, "page_size": cols * rows, "pages": maxi(pages, 1)}

## How many board columns this band's sources actually WANT, declared to the panel so the card can be
## drawn that wide (issue #377), and answered back for the board to fill.
##
## **THE DIRECTION INVERTED HERE, and that is the whole point.** `cols` used to be read OFF the zone's
## width — the panel spanned the monitor, so on an ultrawide the board got four columns whether or not
## the band had anything to put in them, and a band with no sources at all got an empty zone stretched
## across two feet of screen. It is now derived from the SOURCE COUNT and the rows a column holds, and
## the panel sizes its card to the answer.
##
## **It stays acyclic because `rows` comes from the zone's HEIGHT**, which a horizontal dock fixes and
## which nothing here can move. Width follows count; count never follows width.
##
## Without a panel (the `ui_preview` no-dock fallback) there is nobody to declare to, so it falls back
## to measuring the box exactly as before — that host is a fixed-width card with no card to resize.
## **The panel's ANSWER is what gets built, not the want.** `set_work_columns` clamps to what the strip
## can actually pay for — a 380px side dock affords one column however many sources there are — and a
## board built to the unclamped want overflows its clipping zone host silently.
func _declare_work_columns(count: int, rows: int) -> int:
    if _panel == null:
        return clampi(int(_zone_box().x / HudWorkVocab.WORK_COLUMN_MIN_WIDTH), 1, HudWorkVocab.WORK_MAX_COLUMNS)
    var wanted := clampi(ceili(float(count) / float(maxi(rows, 1))), 1, HudWorkVocab.WORK_MAX_COLUMNS)
    return _panel.set_work_columns(wanted)

## The board itself: `cols` column VBoxes filled COLUMN-MAJOR (top of column 1 to its bottom, then
## column 2), separated by a hairline rule. Fixed-height rows, no scroll — the page IS the limit.
func _build_work_board(band: Dictionary, page: Array, cols: int, rows_per_col: int) -> HBoxContainer:
    var board := HBoxContainer.new()
    board.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    board.size_flags_vertical = Control.SIZE_EXPAND_FILL
    board.add_theme_constant_override("separation", HudWorkVocab.WORK_COLUMN_SEPARATION)
    for c in range(cols):
        if c > 0:
            var rule := ColorRect.new()
            rule.color = HudStyle.LINE_SOFT
            rule.custom_minimum_size = Vector2(HudWorkVocab.WORK_COLUMN_RULE_WIDTH, 0.0)
            rule.size_flags_vertical = Control.SIZE_EXPAND_FILL
            rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
            board.add_child(rule)
        var column := VBoxContainer.new()
        column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        column.size_flags_vertical = Control.SIZE_FILL
        column.add_theme_constant_override("separation", 0)
        board.add_child(column)
        for r in range(rows_per_col):
            var index := c * rows_per_col + r
            if index >= page.size():
                break
            column.add_child(_build_work_row(band, page[index]))
    return board

## The zone's head row: WORK · n sources · the band's total rate(s) · the `⋯` section menu.
func _build_work_head(band: Dictionary, models: Array, income: float,
        fodder_income: float) -> HBoxContainer:
    # The two sorts are a mutually exclusive SET, so they carry the radio mark and `Unassign all` — an
    # action, not a member — does not. Without it the menu offered two sorts and stated neither, which
    # is what made the board's default order unreadable. `_repage_work_zone` rebuilds this head, so a
    # pick refreshes the mark with no extra wiring.
    var menu := HudWidgets.build_section_menu([
        {"label": HudWorkVocab.WORK_MENU_SORT_YIELD,
            HudWidgets.MENU_ENTRY_CHECKED: _work_sort == HudWorkVocab.WORK_SORT_YIELD,
            "on_pick": func() -> void: _set_work_sort(HudWorkVocab.WORK_SORT_YIELD)},
        {"label": HudWorkVocab.WORK_MENU_SORT_NAME,
            HudWidgets.MENU_ENTRY_CHECKED: _work_sort == HudWorkVocab.WORK_SORT_NAME,
            "on_pick": func() -> void: _set_work_sort(HudWorkVocab.WORK_SORT_NAME)},
        {"label": HudWorkVocab.WORK_MENU_UNASSIGN_FORMAT % models.size(), "disabled": models.is_empty(),
            "on_pick": func() -> void: _on_work_unassign_all_pressed(band, models.size())},
    ], HudWorkVocab.WORK_MENU_TOOLTIP)
    var head := HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_WORK, HudWorkVocab.WORK_SOURCES_FORMAT % models.size(), menu)
    # The total sits between the count and the menu, tinted like the Food line's net rate.
    var total := Label.new()
    total.text = SourceForecast.format_yield(income)
    total.add_theme_font_size_override("font_size", HudWorkVocab.ZONE_HEAD_FONT_SIZE)
    total.add_theme_color_override("font_color", HudStyle.HEALTHY if income > 0.0 else HudStyle.INK_DIM)
    HudWidgets.set_label_tooltip(total, HudWorkVocab.WORK_TOTAL_TOOLTIP)
    head.add_child(total)
    head.move_child(total, head.get_child_count() - 2)
    # **THE TRADE TOTAL IS RETIRED** (arc #527) with the account it summed. Its argument survives in
    # the fodder total below, which is the same argument in the account that remains.
    # THE FODDER TOTAL IS A SIBLING, NEVER A SUMMAND (issue #449). The food figure beside it is
    # `actual_yield`-denominated because that is the sim's larder identity, and fodder credits the
    # band's FODDER store and never the larder, so folding it in would break exactly that identity.
    # But leaving it out entirely made the header visibly not add up on a band working a sown hay
    # Field: its one source paid feed every turn and the head read as if it produced nothing.
    # Rendered only when non-zero, so a band growing no feed renders exactly as it did before — and
    # the word rather than a glyph, fodder having none.
    if SourceForecast.has_component(fodder_income):
        var fodder_total := Label.new()
        fodder_total.text = SourceForecast.PICKER_FODDER_PRODUCT_FORMAT % SourceForecast.format_signed(fodder_income)
        fodder_total.add_theme_font_size_override("font_size", HudWorkVocab.ZONE_HEAD_FONT_SIZE)
        fodder_total.add_theme_color_override("font_color", HudStyle.HEALTHY)
        HudWidgets.set_label_tooltip(fodder_total, HudWorkVocab.WORK_FODDER_TOTAL_TOOLTIP)
        head.add_child(fodder_total)
        head.move_child(fodder_total, head.get_child_count() - 2)
    # THE OUTPUT ITEM — a THIRD sibling, and it qualifies the two beside it rather than adding to
    # them: `output_multiplier` is the discontent modifier every rate on this board is already scaled
    # by, so it belongs where its consequence is visible and not as a row of the height-capped band
    # zone. Same gate the vitals row carried — only BELOW full output — because a head item
    # permanently reading `Output 100%` is noise on a row that is otherwise live summary. It trails
    # the rates deliberately: it is a note ABOUT them.
    var output: float = float(band.get("output_multiplier", SourceForecast.OUTPUT_FULL))
    if output < SourceForecast.OUTPUT_FULL:
        var output_item := Label.new()
        output_item.text = HudWorkVocab.WORK_OUTPUT_FORMAT % int(round(output * 100.0))
        output_item.add_theme_font_size_override("font_size", HudWorkVocab.ZONE_HEAD_FONT_SIZE)
        output_item.add_theme_color_override("font_color", BandFoodStatus.color_for_output(output))
        HudWidgets.set_label_tooltip(output_item, HudWorkVocab.WORK_OUTPUT_TOOLTIP)
        head.add_child(output_item)
        head.move_child(output_item, head.get_child_count() - 2)
    return head

## **THE POOLS BLOCK — the band's three standing pools, on the tab that shows what they pay for**
## (`docs/plan_standing_upkeep.md` §4.7). `agriculture` holds the plant web, `husbandry` the animal
## one, and `builders` raises whatever the band has QUEUED; all three are staffed by the same
## `assign_labor <faction> <band> <kind> <workers>` the WORKFORCE zone's Scout and Warrior use.
##
## **IT MOVED HERE FROM THE BAND TAB, and the reason is the whole slice**: the pool was on one tab and
## its consequences — the sources it keeps, the queue it funds, the board that reports both — on
## another, so a playtest never connected them.
##
## **THE HEAD COUNTS ALL THREE ROLES against the band's whole working age.** That is a different
## question from the retired `%d on keeping`, which deliberately excluded the builders because the
## block held the keeping pair alone; this one holds all three cards, so a head naming two of them
## would not add up to what is under it. The counts are `effective_role_workers`', i.e. pending-aware,
## the rule every readout on this panel follows.
##
## **IT ALWAYS RENDERS**, including for a band whose board is empty — these are the controls that
## staff the pools, so three steppers at 0 is a live control rather than furniture explaining an
## absence. That is the opposite of the BUILD QUEUE block one down, where nothing queued genuinely
## means there is nothing to show.
##
## **ITS HEIGHT IS THE ONE `HudWorkVocab.pools_block_height` RESERVES**, written onto the block as a
## minimum so the size it draws at and the size `_work_board_capacity` subtracts are one expression.
## The zone clips, so a block that drew taller than it was paid for would take the difference off the
## bottom of the board with nothing to show for it.
func _build_pools_block(band: Dictionary, queued: Array) -> VBoxContainer:
    var idle := _band_labor.effective_idle(band)
    var agriculture_eff := _band_labor.effective_role_workers(band, HudConst.LABOR_KIND_AGRICULTURE)
    var husbandry_eff := _band_labor.effective_role_workers(band, HudConst.LABOR_KIND_HUSBANDRY)
    var builders_eff := _band_labor.effective_role_workers(band, HudConst.LABOR_KIND_BUILDERS)
    var block := HudWidgets.make_zone_block()
    var on_work := int(agriculture_eff.get("workers", 0)) + int(husbandry_eff.get("workers", 0)) \
        + int(builders_eff.get("workers", 0))
    block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_POOLS,
        HudWorkVocab.POOLS_ZONE_READOUT_FORMAT % [on_work, int(band.get("working_age", 0))]))
    # **ONE ROW OF THREE, through the role cards' own row chrome.** They are one family — the hands the
    # band standing still spends — so they read as one row rather than as a pair and an orphan.
    # **THE TWO POOL STATES ARE RESOLVED ONCE, HERE, AND SPENT TWICE** (§4.7). The fund-mode row below
    # already took them as SEPARATE dicts, so the per-web split needed no new plumbing: each keeping
    # card now wears its OWN web's shortfall, which is the thing the summed line under the buttons
    # could not state even in principle.
    var plant_pool := _band_labor.upkeep_pool_state(band, SourceForecast.LABOR_KIND_FORAGE)
    var animal_pool := _band_labor.upkeep_pool_state(band, SourceForecast.LABOR_KIND_HUNT)
    # **AND WHAT EACH POOL SUPPLIES AGAINST WHAT IT IS ASKED FOR** — the ONE test the mark forks on,
    # live shortfall and queued job alike. `upkeep_pool_state` answers what the band is BILLED for
    # today, which is nothing until the first work is banked; the queue's own entries carry the
    # standing rate they will owe the moment they start, and the pool's own hands are what either is
    # measured against. See `HudWorkVocab.upkeep_pool_coverage_line`.
    var plant_cover := _pool_coverage(band, SourceForecast.LABOR_KIND_FORAGE,
        HudConst.LABOR_KIND_AGRICULTURE, int(agriculture_eff.get("workers", 0)), plant_pool, queued)
    var animal_cover := _pool_coverage(band, SourceForecast.LABOR_KIND_HUNT,
        HudConst.LABOR_KIND_HUSBANDRY, int(husbandry_eff.get("workers", 0)), animal_pool, queued)
    var cards := _build_role_card_row()
    cards.add_child(_build_pool_card(band, HudWorkVocab.ROLE_NAME_AGRICULTURE,
        HudWorkVocab.AGRICULTURE_ROLE_HINT, HudConst.LABOR_KIND_AGRICULTURE, agriculture_eff, idle,
        plant_cover))
    cards.add_child(_build_pool_card(band, HudWorkVocab.ROLE_NAME_HUSBANDRY,
        HudWorkVocab.HUSBANDRY_ROLE_HINT, HudConst.LABOR_KIND_HUSBANDRY, husbandry_eff, idle,
        animal_cover))
    # **THE BUILDERS CARD WEARS NO MARK, and it is not an omission.** It funds a QUEUE, one entry at a
    # time, and an entry that is not being built is not being LOST — the queue block one down states
    # its own blocked head. There is no keeping shortfall for this pool to be short of.
    cards.add_child(_build_pool_card(band, HudWorkVocab.ROLE_NAME_BUILDERS,
        HudWorkVocab.BUILDERS_ROLE_HINT, HudConst.LABOR_KIND_BUILDERS, builders_eff, idle))
    block.add_child(cards)
    # The fund mode renders only where either web demands work this turn — see `_build_upkeep_mode_row`.
    # Its presence is what the reserved height forks on, so the answer is recorded on the block rather
    # than re-derived by the capacity maths.
    var fund_mode := _build_upkeep_mode_row(band, plant_pool, animal_pool)
    if fund_mode != null:
        block.add_child(fund_mode)
    block.set_meta(HudWorkVocab.POOLS_BLOCK_META, fund_mode != null)
    block.custom_minimum_size = Vector2(0.0, HudWorkVocab.pools_block_height(fund_mode != null))
    return block

## **WHAT ONE KEEPING POOL SUPPLIES AGAINST WHAT IT IS ASKED FOR** — `{supply, asked}` in work units,
## the ONE input the pool card's mark and its hover both fork on.
##
## **`asked` IS THE LIVE BILL PLUS THE QUEUED ONE.** The sim bills a source from the first work banked,
## so a job queued this frame is owed nothing yet and is owed its rung's standing rate the moment it
## starts — and a pool the player is staffing NOW is being staffed for both. Summing them is what lets
## one sentence carry the declare-time case and the live one at different numbers.
##
## **`supply` IS PROJECTED FROM THE POOL'S OWN HANDS, never read off `upkeep_supplied`.** That field
## is each source's stamped SHARE, which is capped by its demand — so a pool with hands to spare
## reports exactly its demand and would read as covering nothing more, marking every card the moment
## anything was queued. `SourceForecast.pool_work_supply` is the sim's own expression
## (`intensification::pool_work_supply`), at the bare rate the sources publish plus this web's derived
## keeping gear.
##
## **THE WORKERS ARE THE PENDING-AWARE COUNT**, the rule every readout on this panel follows: a player
## who has just staffed the role must not be told the pool is empty until the turn resolves.
func _pool_coverage(band: Dictionary, source_kind: String, role_kind: String, workers: int,
        pool: Dictionary, queued: Array) -> Dictionary:
    var queued_load := _queued_keeping_load(queued, source_kind)
    var asked := maxf(float(pool.get("demand", SourceForecast.NO_UPKEEP_DEMAND)),
        SourceForecast.NO_UPKEEP_DEMAND) \
        + float(queued_load.get("demand", SourceForecast.NO_UPKEEP_DEMAND))
    # The BARE per-worker rate, off whichever of the two source sets stated one. They publish the same
    # constant, so the `max` is only ever choosing between an answer and a silence — a pool with
    # something to pay for always read at least one source to price it against.
    var per_worker := maxf(
        float(pool.get(HudBandLaborState.POOL_PER_WORKER_TURN_KEY, SourceForecast.BUILD_WORK_NONE)),
        float(queued_load.get(HudBandLaborState.POOL_PER_WORKER_TURN_KEY,
            SourceForecast.BUILD_WORK_NONE)))
    var kit_gear := KitRoster.build_gear(band, KitRoster.keeping_kit_for(_band_labor.kits(),
        role_kind), KitRoster.build_branch_for_kind(source_kind))
    return {
        HudWorkVocab.POOL_COVERAGE_SUPPLY_KEY: SourceForecast.pool_work_supply(workers, per_worker,
            kit_gear),
        HudWorkVocab.POOL_COVERAGE_ASKED_KEY: asked,
    }

## **THE STANDING KEEPING THIS WEB'S QUEUED JOBS WILL OWE** — `{demand, per_worker_turn}`, summed and
## read off the same models.
##
## **SUMMED rather than maxed**, because the pool pays all of them: two Tames queued on one band are
## two standing bills the moment they start, and quoting the larger would understate the commitment
## the card is warning about.
##
## **A PENDING DECLARATION COUNTS, which is the whole point.** `_build_queue_models` carries the
## just-declared entries on its tail, and each model's `build_upkeep_demand` was priced off
## `building_policy` — which `build_verb` answers from the declaration at a zero meter — so a job
## queued this frame is in this sum on this frame.
##
## The per-worker rate rides out beside the demand for `upkeep_pool_state`'s reason: a pool whose only
## bill is a queued job has no BILLED source to price its hands against, and the queued job's own
## source states the same constant.
func _queued_keeping_load(queued: Array, labor_kind: String) -> Dictionary:
    var total := SourceForecast.NO_UPKEEP_DEMAND
    var per_worker := SourceForecast.BUILD_WORK_NONE
    for entry in queued:
        if not (entry is Dictionary):
            continue
        var model: Dictionary = entry
        if String(model.get("kind", "")) != labor_kind:
            continue
        # **A SOURCE THE SIM IS ALREADY BILLING CONTRIBUTES ITS RATE THROUGH `upkeep_pool_state`, NOT
        # HERE.** A queue entry keeps its position while its meter climbs, so counting both would ask
        # the pool for one job's keeping twice the turn after the work starts — the pool's own demand
        # doubling with nothing about the band having changed.
        if float(model.get("live_upkeep_demand", SourceForecast.NO_UPKEEP_DEMAND)) \
                >= SourceForecast.UPKEEP_WORK_MIN:
            continue
        total += maxf(float(model.get("build_upkeep_demand", SourceForecast.NO_UPKEEP_DEMAND)),
            SourceForecast.NO_UPKEEP_DEMAND)
        per_worker = maxf(per_worker, float(model.get("build_work_per_worker_turn",
            SourceForecast.BUILD_WORK_NONE)))
    return {"demand": total, HudBandLaborState.POOL_PER_WORKER_TURN_KEY: per_worker}

## **ONE POOL CARD — the role card with everything but the CONTROL taken off.** Its name, its stepper,
## and its description on the card's own `tooltip_text`.
##
## **NO KIT PICKER, because none of the three has one and neither reason is this card's to revisit**:
## the keeping pair's pick would move no number the player can see and its `(default)` mark would be a
## guess, and a `builders` pick could only ever answer a per-ENTRY question with one standing answer
## (§4.6b deleted that picker rather than leave it harmful, and §4.7a ② gave the override its correct
## home on the queue row — where it changes that job alone). **No gear line either** — the
## fact it stated, which kit the pool is carrying, is stated one block below by the BUILD QUEUE head
## through the same `_role_kit_id`, adjacent to the jobs that kit prices.
##
## **AND NO PROSE.** The hint rides the tooltip instead: the words survive, they stop costing vertical
## space on a zone that clips, and three cards abreast at the narrow shell's ~354px cannot afford two
## wrapped lines each.
##
## **THE COMMAND CARRIES NO `kit` TOKEN FOR ANY OF THE THREE**, and the expression is `_build_role_card`'s
## own so the two cannot drift: the keeping pair names no kit at all, and `_commanded_role_kit_id`
## answers `NO_KIT_ID` on the builders branch deliberately — echoing the DERIVED id back would pin the
## pool to whichever web it happened to be building the moment the player pressed `+`.
##
## **AND WHERE ITS WEB DOES NOT COVER WHAT IT IS ASKED FOR IT WEARS A BARE `⚠`, with the figures on
## the tooltip** (§4.7). The mark is a mark: the card is a role name over a stepper and has no room for
## arithmetic, and the shortfall is the reason a player would open the hover at all. `cover` is that
## web's own `{supply, asked}` (`{}` for the Builders card, which keeps nothing), never the two webs
## summed.
##
## **ONE TEST, ONE MARK, ONE SENTENCE.** The live bill and a job the queue has not started owing yet
## were two triggers wearing one glyph — *"short 2 of 2"* beside *"nobody is on this pool"* — which
## read as one warning misbehaving. `_pool_coverage` folds both into what the pool SUPPLIES against
## what it is ASKED FOR, so there is one thing to say and `HudWorkVocab.upkeep_pool_coverage_line` is
## the one composer that says it.
func _build_pool_card(band: Dictionary, role_name: String, hint: String, kind: String,
        effective: Dictionary, idle: int, cover: Dictionary = {}) -> PanelContainer:
    var workers := int(effective.get("workers", 0))
    var pending := bool(effective.get("pending", false))
    var coverage_line := HudWorkVocab.upkeep_pool_coverage_line(role_name, cover)
    var wants_mark := coverage_line != ""
    var card := PanelContainer.new()
    card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # The role cards' own levelness rule, and it is load-bearing on a row of THREE: the `HBoxContainer`
    # stretches a child to the row height wherever the child asks to FILL its cross axis, and
    # `SIZE_FILL` is `Control`'s own default. Written out rather than inherited because a hardcoded
    # minimum height would be wrong the next time a role name changes length.
    card.size_flags_vertical = Control.SIZE_FILL
    card.add_theme_stylebox_override("panel", HudStyle.role_card_stylebox())
    # The coverage sentence joins the role's own description rather than replacing it: what the pool
    # DOES is the answer to "how much do I need", and the figures are meaningless without it.
    card.tooltip_text = HudFormat.join_tooltip_lines([hint, coverage_line])
    # **THE META IS THE MARK, and the mark is now the coverage answer** — every harness reads it to ask
    # *is this card marked*, which is the question the `⚠` answers, and the composer that decides the
    # sentence is the same one that decides the glyph.
    card.set_meta(POOL_CARD_SHORT_META, wants_mark)
    var col := VBoxContainer.new()
    col.add_theme_constant_override("separation", HudWorkVocab.ROLE_CARD_SEPARATION)
    card.add_child(col)
    var title := Label.new()
    title.text = role_name
    title.add_theme_font_size_override("font_size", HudWorkVocab.ROLE_CARD_NAME_FONT_SIZE)
    # A PENDING edit and a SHORT pool are different news and the pending one is the newer: it says the
    # number under this title is not the sim's yet. WARN carries both, so the ink forks only against
    # the calm card.
    title.add_theme_color_override("font_color",
        HudStyle.WARN if pending or wants_mark else HudStyle.INK)
    if not wants_mark:
        col.add_child(title)
    else:
        # **THE MARK SITS BESIDE THE NAME, NOT INSIDE IT.** A row of its own is what this block cannot
        # afford (the card is budgeted at `POOL_CARD_HEIGHT` and holds a name over a stepper), and
        # welding the glyph into the title's own `text` would make the card unfindable by that title —
        # which is how every harness, and `_role_card_under`, identifies one.
        var name_row := HBoxContainer.new()
        name_row.add_theme_constant_override("separation", HudWorkVocab.ROLE_CARD_SEPARATION)
        name_row.add_child(title)
        var mark := Label.new()
        mark.text = HudWorkVocab.UPKEEP_POOL_SHORT_MARK
        mark.add_theme_font_size_override("font_size", HudWorkVocab.ROLE_CARD_NAME_FONT_SIZE)
        mark.add_theme_color_override("font_color", HudStyle.WARN)
        name_row.add_child(mark)
        col.add_child(name_row)
    var commanded_kit_id := _commanded_role_kit_id(band, kind) if _role_states_a_kit(kind) \
        else KitRoster.NO_KIT_ID
    var stepper := HBoxContainer.new()
    stepper.alignment = BoxContainer.ALIGNMENT_CENTER
    stepper.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    # `compact_chrome`, the work zone's own row treatment: the default button padding alone makes a
    # stepper ~40px tall, which is most of what this block can afford for a whole card.
    HudWidgets.add_stepper_controls(stepper, workers, idle > 0,
        func(n: int) -> void: _emit_assign_labor(
            band, kind, n, -1, -1, "", SourceForecast.DEFAULT_HARVEST_FLOOR,
            "", SourceForecast.IMPROVEMENT_NONE, commanded_kit_id), true)
    col.add_child(stepper)
    return card

## **THE BAND'S BUILD QUEUE, IN THE BAND'S OWN ORDER** — its `PopulationCohortState.buildQueue`
## entries joined to the work-source models, in wire order (`docs/plan_standing_upkeep.md` §4.9
## item 9a).
##
## ⛔ **BOTH THE MEMBERSHIP AND THE ORDER COME FROM THE BAND'S OWN QUEUE, NEVER FROM
## `build_queue_position`.** That field is published per SOURCE and rides the winning band — the one
## with the soonest estimate among the bands working it — so on a shared source it states another
## band's place in another band's line. Ordering on it drew band B's `[X, Y, Z]` as `[Y, X, Z]` when
## band C also held Y, and `_queue_drop` then computed its insert index from that wrong list, so a
## drag landed the entry on the opposite side of its target. `build_queue_position` survives on the
## model as a READOUT (the row's meta, the `✕`'s meta); the RANK is the index into this list.
##
## **IT IS DERIVED FROM THE FULL MODEL SET AND NEVER FROM `filtered`.** The chips filter the BOARD;
## the queue is the band's own list, so a chip press must leave it alone.
##
## ⛔ **AN ENTRY WITH NO MODEL IS SKIPPED, AND THAT IS A REACHABLE, PERSISTENT STATE — which is
## exactly why the RANK IS THE WIRE QUEUE'S INDEX AND NEVER THIS LIST'S.** An earlier note here
## claimed the skip could not normally happen, on the strength of *"an entry requires a row"*. The
## rule is real and the conclusion drawn from it was false: the rule says a ROW, and a row is not a
## CREW.
##
## The path, end to end — a queued source whose take crew the player has taken to zero:
##
## | step | seam |
## |---|---|
## | unstaffing a source the band already held KEEPS its row, at zero workers | `LaborAllocation::set_assignment` → `keep_holding` (`core_sim/src/components.rs:3526`) |
## | …and the command declines to drop that row because the source is QUEUED | `handle_assign_labor` → `if applied == 0 && !source_holds_something && !queued` (`core_sim/src/bin/server.rs:3134`) |
## | the membership test asks only whether a row EXISTS, never how many hands are on it | `holds_build_source` (`core_sim/src/components.rs:3721`), so `prune_build_queue` keeps the entry |
## | …and the turn pass spares it for the same reason, so the state SURVIVES EVERY TURN | `queued.is_none()` guards the lapse (`core_sim/src/systems/labor.rs:1866`) |
## | the client then drops that row: the board admits on the take crew | `_work_source_models` — `if workers <= 0 and not pending: continue` |
##
## So the wire queue `[A, B, C]` legitimately draws as `[B, C]`, and a position counted off the DRAWN
## list is short by every entry hidden above it — `▼` on `B` would send `1`, which the sim resolves
## back to `[A, B, C]`: a button that reads as broken because it silently did nothing.
##
## **THE RANK IS THEREFORE STAMPED HERE, from the entry's place in `build_queue_keys`**
## (`BUILD_QUEUE_ROW_RANK_KEY`), and every `build_order` the block sends — both arrows and the drag —
## is an index into that list. The end-stops key on it too, so with `[A(hidden), B, C]` drawn as
## `[B, C]`, `B`'s `▲` is ENABLED (it can climb above `A`) and only `C`'s `▼` is disabled.
##
## **Skipping rather than inventing a placeholder is still the right fallback**: a placeholder has no
## face, no date, no legs and no price to state. Admitting the zero-crew row instead would put it back
## on the WORK BOARD as well, which §2.5 deliberately reverted — a separate design question, and not
## this list's to decide.
##
## **…AND THE DECLARATIONS THE WIRE HAS NOT PLACED YET RIDE ITS TAIL.** The queue is captured live,
## so a declaration lands in it on its own command's recapture — but that recapture is a network hop
## away and the block is drawn on the frame the `⌃` is pressed. The optimistic overlay carries the
## declaration across that window, so a source that is `pending` with a live rung in flight and NOT
## in the band's wire queue is a pending entry, and `HudWorkVocab`'s "A DECLARATION THE WIRE HAS NOT
## PLACED YET" states the rest of the rule (tail only, no date, no head marker, a full row of height,
## reconciles away for free).
##
## ⛔ **THE TAIL EXCLUDES ANYTHING THE WIRE QUEUE ALREADY CARRIES, AND THAT TEST IS NEW.** The overlay
## outlives the recapture that confirms it (it is keyed on the TURN), and the live capture puts the
## same key in both halves the moment the reply lands — so without this the just-declared entry drew
## TWICE, once in the list and once on the tail. The old turn-written field could not produce that
## state, which is why the test was not needed before.
##
## **THE TAIL IS NOT A GUESS AT A POSITION, it is the refusal to make one.** The sim APPENDS, so the
## end of the list is the only honest place for an entry the wire has not placed; interleaving would
## state a fact the sim has not made. Within the tail they hold DECLARATION ORDER, read off the
## pending overlay's own insertion order (`pending_assigns_for` is written once per declaration and a
## Godot Dictionary keeps its insertion order), with a `key` tiebreak so the tail stays a TOTAL order
## under Godot's unstable sort. The CONFIRMED half needs neither a tiebreak nor a sort now — a vector
## index is already a total order.
##
## **THE RUNG IN FLIGHT IS PART OF THE TEST, not just the declaration.** `record_pending_assign` fires
## on EVERY worker step and carries the improvement forward, so `pending` alone would keep a row here
## after its build completed; `building_glyph` is `RungGates.rung_in_progress`'s already-resolved
## answer, which goes empty the moment the meter does.
func _build_queue_models(band: Dictionary, models: Array) -> Array:
    # **A WITHDRAWAL TAKES ITS ROW OUT ON THE FRAME THE `✕` IS PRESSED**
    # (`docs/plan_standing_upkeep.md` §4.7b ④), and what it covers now is the ROUND TRIP rather than
    # a stale field — see `HudBandLaborState`'s own note, which records that the reason this filter
    # was written for died with the live capture. Keyed on the TURN like every other optimistic
    # write, so it reconciles away with them.
    var entity := int(band.get("entity", -1))
    var withdrawn := _band_labor.pending_unqueues_for(entity)
    # ONE derivation of the band's order, shared with `_queue_drop` and with the block's `▼`
    # end-stop, so the index a drag sends, the index an arrow sends and the row that may not fall
    # any further are all the same list.
    var rank_keys := _queue_rank_keys(band)
    var in_wire_queue: Dictionary = {}
    for key_variant in _band_labor.build_queue_keys(band):
        in_wire_queue[String(key_variant)] = true
    var by_key: Dictionary = {}
    for model_variant in models:
        by_key[String((model_variant as Dictionary).get("key", ""))] = model_variant
    var queued: Array = []
    for rank in range(rank_keys.size()):
        var key := String(rank_keys[rank])
        # ⛔ **AN ENTRY THAT DOES NOT DRAW STILL SPENDS ITS RANK** — that is the whole point of
        # walking the wire's list with its own index rather than appending and counting. Two kinds
        # skip, and the WIRE still carries both: one whose source has no work-source model (the note
        # above has the path, and that state persists across turns), and one the player has just
        # withdrawn, whose `✕` is covered for the round trip by an overlay while `buildQueue` still
        # lists it.
        if withdrawn.has(key) or not by_key.has(key):
            continue
        var entry: Dictionary = by_key[key]
        entry[BUILD_QUEUE_ROW_PENDING_KEY] = false
        entry[BUILD_QUEUE_ROW_RANK_KEY] = rank
        queued.append(entry)
    var declared: Array = _band_labor.pending_assigns_for(entity).keys()
    var awaiting: Array = []
    for model_variant in models:
        var model: Dictionary = model_variant
        var key := String(model.get("key", ""))
        if in_wire_queue.has(key) or withdrawn.has(key):
            continue
        if not bool(model.get("pending", false)) \
                or String(model.get("building_glyph", "")) == "":
            continue
        model[BUILD_QUEUE_ROW_PENDING_KEY] = true
        # **A PENDING ROW HAS NO RANK, because the wire has not placed it** — the same
        # `NOT_IN_ANY_BUILD_QUEUE` its row meta and its `✕` have always worn, stamped once here so
        # the arrows, the drag gate and the metas cannot disagree about it.
        model[BUILD_QUEUE_ROW_RANK_KEY] = SourceForecast.NOT_IN_ANY_BUILD_QUEUE
        awaiting.append(model)
    awaiting.sort_custom(func(a, b):
        var ia := declared.find(String((a as Dictionary).get("key", "")))
        var ib := declared.find(String((b as Dictionary).get("key", "")))
        if ia != ib:
            return ia < ib
        return String((a as Dictionary).get("key", "")) < String((b as Dictionary).get("key", "")))
    queued.append_array(awaiting)
    return queued

## The model slot `_build_queue_models` stamps its verdict into. A MODEL key rather than a node meta
## because the answer is a property of the ENTRY, and three of its four readers hold the model before
## any node exists.
const BUILD_QUEUE_ROW_PENDING_KEY := "queue_row_pending"

## **IS THIS ROW ONE THE BAND'S WIRE QUEUE DOES NOT CARRY?** The one derivation of it, so the block's
## filter, the head marker's suppression, the drag gate and the row's date can never disagree about
## which rows are pending.
##
## ⛔ **IT IS BAND-DEPENDENT NOW, WHICH IS WHY IT READS A STAMP RATHER THAN THE MODEL'S OWN FIELDS.**
## The question used to be *"did the wire give this source a position?"* — answerable from the model
## alone, and wrong for the same reason ordering on that position was wrong, since the position
## belongs to whichever band has the soonest estimate. It is now *"is this key in THIS band's
## queue?"*, which no model can answer by itself; `_build_queue_models` resolves it once against the
## band and writes the answer here, so this stays a SINGLE derivation instead of becoming a band
## argument threaded through five call sites.
##
## ⛔ **`PENDING` NOW MEANS THE PRESS-TO-REPLY WINDOW AND NOTHING WIDER, AND THAT IS DELIBERATE — do
## not "restore" the old meaning.** It used to stretch until the TURN resolved, because
## `buildQueuePosition` was the test and that field is turn-written. The queue is captured live, so an
## entry the sim has accepted is in `buildQueue` on the command's own recapture: from that frame it is
## QUEUED AT A REAL INDEX and draws like one — at that index, with a drag handle, and wearing the `▸`
## if it is index 0. It is not pending, because it is not waiting on us; it is in the line.
##
## **ITS DATE GOES BLANK FOR THE REST OF THE TURN, AND THAT IS THE TRUTHFUL FACE.**
## `buildTurnsRemaining` IS turn-written, so the sim has not yet answered *when* about that entry —
## and `BUILD_TURNS_NO_ESTIMATE`'s own rule is that a missing line is honest where a zero is a
## promise. A row that quoted a number here would be inventing the one thing nobody has computed.
##
## Defaults to `true` for a model nobody stamped: unstamped means *no queue this controller built
## placed it*, and calling that pending withholds a drag handle rather than handing `_queue_drop` a
## row to compute an index from.
static func _build_queue_row_is_pending(model: Dictionary) -> bool:
    return bool(model.get(BUILD_QUEUE_ROW_PENDING_KEY, true))

## The model slot carrying **the entry's place in the BAND'S OWN WIRE QUEUE** —
## `NOT_IN_ANY_BUILD_QUEUE` for a pending tail row the wire has not placed. Stamped once by
## `_build_queue_models`, beside the pending verdict and for the same reason: it is a property of the
## ENTRY, and the readers that need it hold the model before any node exists.
const BUILD_QUEUE_ROW_RANK_KEY := "queue_row_wire_rank"

## ⛔ **THE ONE NUMBER EVERY `build_order` THIS BLOCK SENDS IS COUNTED IN** — the row's index into
## `_queue_rank_keys`, never into the list the block drew.
##
## The two lists are not the same length: a queue entry whose source has no work-source model is
## skipped by `_build_queue_models` (its note says by what path, and that the state persists across
## turns), so the drawn list can be shorter than the wire's with the gaps anywhere in it. Counting a
## position off the drawn list is then wrong by the number of entries hidden above it, and the sim
## resolves the short index against its own full queue — which reads to the player as an arrow that
## does nothing, or as a row that jumped above something they cannot see.
##
## Defaults to `NOT_IN_ANY_BUILD_QUEUE` for a model nobody stamped, matching
## `_build_queue_row_is_pending`'s own default: an unranked row offers no arrows and no drag rather
## than naming a position it cannot vouch for.
static func _build_queue_row_rank(model: Dictionary) -> int:
    return int(model.get(BUILD_QUEUE_ROW_RANK_KEY, SourceForecast.NOT_IN_ANY_BUILD_QUEUE))

## **THE BAND'S OWN QUEUE, AS THE SIM HOLDS IT** — the index space every `build_order` position is
## counted in, and the one derivation of it. Three readers share it: `_build_queue_models` stamps
## each drawn row's rank from it, the block reads its LENGTH for the `▼` end-stop, and `_queue_drop`
## does its removal-and-insert arithmetic in it.
##
## **IT IS THE WIRE'S LIST WHOLE — hidden entries included.** That is the entire correction: the
## block's membership is the drawn list's, but its ARITHMETIC has to be the sim's, because the sim is
## what resolves the number. Deriving the length here rather than counting drawn rows is what makes
## *"the last entry cannot fall further"* a fact about the queue instead of a fact about the page —
## which it already had to be, the page being truncated at `BUILD_QUEUE_ROWS_MAX`.
func _queue_rank_keys(band: Dictionary) -> Array:
    return _band_labor.build_queue_keys(band)

## The BUILD QUEUE block — its head, up to `BUILD_QUEUE_ROWS_MAX` entry rows, and the overflow row
## that stands for the rest.
##
## **ITS HEIGHT IS THE ONE `HudWorkVocab.build_queue_block_height` RESERVES**, written onto the block
## as a minimum so the size it draws at and the size `_work_board_capacity` subtracts are the same
## expression. The zone clips, so a block that drew taller than it was paid for would take the
## difference off the bottom of the board with nothing to show for it.
func _build_build_queue_block(band: Dictionary, queued: Array, rows_max: int) -> VBoxContainer:
    var block := VBoxContainer.new()
    block.set_meta(HudWorkVocab.BUILD_QUEUE_BLOCK_META, queued.size())
    block.add_theme_constant_override("separation", 0)
    var drawn := mini(queued.size(), rows_max)
    # **THE STRIP IS PAID FOR ONLY WHERE IT DRAWS**, and the test is the same one the row's own click
    # target and the strip's contents use — `_queue_settings_state`, resolved ONCE here and spent
    # three times below, so the block cannot reserve a height for a strip it then declines to build.
    # An entry scrolled past the row cap cannot be open either: it has no row to hang beneath.
    var settings := _queue_settings_state(band, queued, drawn)
    var open_index := int(settings["index"])
    # ⛔ **THE `▼` END-STOP IS THE WIRE QUEUE'S LENGTH, AND IT IS READ OFF THE WIRE.** It used to count
    # the block's own non-pending rows, which is the same number only when every entry drew — and an
    # entry with no work-source model does not (`_build_queue_models`' note has the path). The last
    # row a `build_order` can name is the wire's last entry, drawn or not, so the count comes from
    # `_queue_rank_keys` and is handed to every row rather than re-derived per row.
    #
    # Pending entries never enter it: they ride the tail with no rank at all
    # (`NOT_IN_ANY_BUILD_QUEUE`), so they take neither end-stop.
    var confirmed := _queue_rank_keys(band).size()
    block.custom_minimum_size = Vector2(0.0, HudWorkVocab.build_queue_block_height(
        queued.size(), rows_max, int(settings["legs"]), bool(settings["crop"]),
        bool(settings["kit"]), bool(settings["one_line"])))
    # The drag reaches its target rows through this map rather than through the tree, because the
    # drop indicator is a stylebox swap and must not re-render the block it is hovering over.
    _queue_row_nodes.clear()
    # **THE BUILDERS COUNT IS PENDING-AWARE**, the rule every compose sheet on this panel follows: a
    # player who has just staffed the role must not read a header telling them nobody is on it.
    var builders := int(_band_labor.effective_role_workers(
        band, HudConst.LABOR_KIND_BUILDERS).get("workers", 0))
    block.add_child(_build_build_queue_head(band, builders))
    for index in range(drawn):
        var entry: Dictionary = queued[index] as Dictionary
        # **A PENDING ROW NEVER WEARS THE HEAD MARKER, not even when it is the only row.** The head is
        # the entry the builders pool is actually standing on, which the sim decides; an entry the sim
        # has not placed is not that yet, and a `▸` on it would promise funding nobody has committed.
        # **AND THE HEAD MARKER IS THE WIRE'S HEAD, not the page's first row** — for the same reason
        # the ranks are: the `▸` names the entry the builders pool is standing on, which is wire
        # index 0, and an entry hidden above the first drawn row is still the one being funded.
        # `NOT_IN_ANY_BUILD_QUEUE` for a pending row, so the pending case falls out of the test.
        var row := _build_build_queue_row(band, entry,
            _build_queue_row_rank(entry) == SourceForecast.BUILD_QUEUE_HEAD,
            builders, confirmed)
        _queue_row_nodes[String(entry.get("key", ""))] = row
        block.add_child(row)
        # …and its SETTINGS strip directly beneath the row it belongs to, which is what makes the
        # expansion read as that row's rather than as a panel of its own.
        if index == open_index:
            block.add_child(_build_queue_settings_strip(band, entry))
    if queued.size() > drawn:
        block.add_child(_build_build_queue_overflow_row(queued.size() - drawn))
    return block

## **THE QUEUE OVER THE WHOLE WORK ZONE — every entry, in a scrolling list** (§4.9 item 9c).
##
## **IT REUSES THE BLOCK'S OWN BUILDERS AND RE-SPELLS NOTHING.** The head is
## `_build_build_queue_head`, the rows are `_build_build_queue_row` and the strip is
## `_build_queue_settings_strip`, so the arrows, the `✕`, the drag handle, the `▸`, the pending rules
## and the `▼` end-stop are INHERITED rather than restated — which is what keeps the two modes from
## drifting into two answers about the same queue.
##
## **EVERY DRAWABLE ENTRY GETS A ROW, so `_queue_settings_state` is asked with `drawn ==
## queued.size()`**: the cap is what made an entry past the third unopenable, and there is no cap
## here. `is_head` is still the WIRE's head, not the first drawn row — an entry the block cannot draw
## is still the one the builders pool is standing on.
##
## **THE LIST IS THE THIRD SANCTIONED `ScrollContainer` IN THIS PANEL** and it declares a FIXED
## viewport height off the zone's own box (`HudWorkVocab.build_queue_expanded_scroll_height`), so what
## the list holds never reaches the zone's reservation — the same contract the parties list and the
## band zone's stack are sanctioned under.
func _build_build_queue_expanded(band: Dictionary, queued: Array,
        pools_fund_mode: bool) -> VBoxContainer:
    var block := VBoxContainer.new()
    block.set_meta(HudWorkVocab.BUILD_QUEUE_BLOCK_META, queued.size())
    block.add_theme_constant_override("separation", 0)
    block.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    block.size_flags_vertical = Control.SIZE_EXPAND_FILL
    var settings := _queue_settings_state(band, queued, queued.size())
    var open_index := int(settings["index"])
    var confirmed := _queue_rank_keys(band).size()
    var builders := int(_band_labor.effective_role_workers(
        band, HudConst.LABOR_KIND_BUILDERS).get("workers", 0))
    block.add_child(_build_build_queue_head(band, builders))
    var scroll := ScrollContainer.new()
    scroll.name = HudWorkVocab.BUILD_QUEUE_EXPANDED_SCROLL_NAME
    scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
    scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
    scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
    scroll.custom_minimum_size = Vector2(0.0,
        HudWorkVocab.build_queue_expanded_scroll_height(_zone_box().y, pools_fund_mode))
    var list := VBoxContainer.new()
    list.add_theme_constant_override("separation", 0)
    list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # A scrolled child must not claim the viewport's height as its own, or a short queue would stretch
    # its rows down the zone — the parties list's own rule, for the same reason.
    list.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
    scroll.add_child(list)
    # The drop indicator is a stylebox swap on nodes the controller holds, so the map is rebuilt here
    # exactly as the block rebuilds it — a drag over the expanded list reaches its target the same way.
    _queue_row_nodes.clear()
    for index in range(queued.size()):
        var entry: Dictionary = queued[index] as Dictionary
        var row := _build_build_queue_row(band, entry,
            _build_queue_row_rank(entry) == SourceForecast.BUILD_QUEUE_HEAD,
            builders, confirmed)
        _queue_row_nodes[String(entry.get("key", ""))] = row
        list.add_child(row)
        if index == open_index:
            list.add_child(_build_queue_settings_strip(band, entry))
    block.add_child(scroll)
    _queue_expanded_scroll = scroll
    _restore_queue_scroll_offset(scroll)
    return block

## ⛔ **PUT THE PLAYER BACK WHERE HE WAS IN THE LIST — A FRAME LATER, AND THAT IS THE WHOLE TRAP.**
## `ScrollContainer.scroll_vertical` is clamped to its scrollbar's CURRENT range on the way in, and
## the rows added a line above have not been laid out — so an assignment made here is clamped to
## whatever that un-ranged bar happens to allow and the rebuild-forgets-its-place defect survives
## underneath a fix that reads right. **The clamp is not to zero, which is what makes it so easy to
## miss**: a fresh `VScrollBar` is a `Range`, and a `Range` ships `max = 100`, so an offset under 100px
## sails through the naive form intact and only a deeper scroll shows the truncation. One frame is
## what it takes for the container to sort its children and re-range that bar, which is
## `CraftingPanel.refit`'s own reason for awaiting one before restoring `_pending_scroll`.
##
## **THE CLAMP IS THE CONTAINER'S, DELIBERATELY** — a queue that lost entries has a shorter list, and
## the setter's own range check is what stops the restore landing past its new end.
##
## ⛔ **AND IT MUST NEVER LAND UNDER A LIVE DRAG.** The edge auto-scroll writes `scroll_vertical`
## every frame of the gesture; a deferred restore resuming beside it would fight the pump and yank the
## list back to where the rebuild found it. `_queue_drag_in_flight` already holds the rebuild itself
## off, so the only reachable case is a gesture starting in the frame this is waiting out — declined
## here rather than assumed impossible.
func _restore_queue_scroll_offset(scroll: ScrollContainer) -> void:
    if _queue_expanded_scroll_offset <= 0:
        return
    var want := _queue_expanded_scroll_offset
    await _host.get_tree().process_frame
    if not is_instance_valid(scroll) or not scroll.is_inside_tree():
        return
    if _queue_expanded_scroll != scroll or _queue_drag_in_flight():
        return
    scroll.scroll_vertical = want

## Open the whole queue over the Work zone, or fold it back to the summary block — the mode's ONE
## mutator, driven by the BUILD QUEUE header both ways and by the `+N more` row inward.
##
## ⛔ **ENTERING IT CLEARS THE WORK INSPECTOR, and that line is load-bearing rather than tidy.** The
## expanded fill never runs the board's own pruning path, so a `_work_open_key` left set would spring
## the inspector back on the collapse BESIDE an open queue settings strip — which is exactly the
## 460-into-a-396px-box overflow the one-expansion rule closed (§4.7b). It is the same clear
## `_toggle_queue_settings` already carries.
## **ENTERING IT ALSO OPENS THE LIST AT THE TOP.** `_queue_expanded_scroll_offset` is a place in ONE
## list and is carried across that list's rebuilds; a fresh entry into the mode is not one of those.
func _toggle_queue_expanded() -> void:
    _queue_expanded = not _queue_expanded
    if _queue_expanded:
        _work_open_key = ""
        _work_picker_open = HudWorkVocab.WORK_PICKER_NONE
        _queue_expanded_scroll_offset = 0
    _repage_work_zone()

## **THE HEAD IS THE TOGGLE, BOTH WAYS** (§4.9 item 9c) — `+N more` is a second door IN only, the expanded
## view having no overflow row left to press, so this is the only way back. It is available whenever
## the block exists, including a queue short enough to draw no overflow row at all.
##
## **THE GLYPH COMES OUT OF THE HEAD'S EXPANDING SPACER**, inserted straight after the title: the
## right-hand readout states the builders count and their kit and may not give up a character.
##
## ⛔ **IT FIRES ON THE RELEASE, INSIDE THE ROW.** `_toggle_queue_expanded` ends in
## `_repage_work_zone`, which frees every node in the zone — and *any press handler that rebuilds its
## own subtree kills every drag that could start under it* is the general rule PR #574's autopsy
## named, after the queue rows' own toggle shipped on the press and left the reorder gesture dead.
## Inside the row, because `mouse_focus` latches on the press.
func _make_queue_head_a_toggle(head: HBoxContainer) -> void:
    var glyph := Label.new()
    glyph.text = HudWorkVocab.BUILD_QUEUE_DISCLOSURE_EXPANDED if _queue_expanded \
        else HudWorkVocab.BUILD_QUEUE_DISCLOSURE_COLLAPSED
    glyph.add_theme_color_override("font_color", HudStyle.INK_DIM)
    glyph.add_theme_font_size_override("font_size",
        HudWorkVocab.BUILD_QUEUE_DISCLOSURE_FONT_SIZE)
    glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
    glyph.set_meta(HudWorkVocab.BUILD_QUEUE_DISCLOSURE_META, _queue_expanded)
    head.add_child(glyph)
    head.move_child(glyph, 1)
    head.set_meta(HudWorkVocab.BUILD_QUEUE_DISCLOSURE_META, _queue_expanded)
    head.mouse_filter = Control.MOUSE_FILTER_STOP
    head.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
    head.tooltip_text = HudWorkVocab.BUILD_QUEUE_DISCLOSURE_TOOLTIP
    # The readout Label takes `STOP` for its own tooltip (`HudWidgets.set_label_tooltip`), which would
    # swallow a press landing on it. `PASS` is the drag handle's shipped trick: the label is still
    # found for the hover, so `BUILD_QUEUE_BUILDERS_TOOLTIP` survives, and the event carries on up to
    # the row.
    for child in head.get_children():
        if child is Label and (child as Label).mouse_filter == Control.MOUSE_FILTER_STOP:
            (child as Label).mouse_filter = Control.MOUSE_FILTER_PASS
    head.gui_input.connect(func(event: InputEvent) -> void:
        if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT \
                and not event.pressed \
                and Rect2(Vector2.ZERO, head.size).has_point(event.position):
            _toggle_queue_expanded())

## **WHICH DRAWN ENTRY HAS ITS SETTINGS STRIP OPEN, AND WHAT THAT STRIP HOLDS** — `{index, legs,
## crop}`, `index == -1` for none. The block's one answer to that question, so the height it reserves,
## the row it draws beneath, the strip it builds and the board's own capacity are the same decision
## made once.
##
## **IT ANSWERS THE STRIP'S CONTENT RATHER THAN ITS HEIGHT** (§2.8). The strip lists the entry's LEGS
## as well as its crop, so its height varies — and `HudWorkVocab.build_queue_settings_height` stays the
## one arithmetic both the reservation and the render read.
##
## **IT ALSO PRUNES THE KEY**, the way `_work_open_key` is pruned for a source that leaves the board:
## an entry that finished, was withdrawn or scrolled past the row cap takes its expansion with it
## rather than leaving a strip pinned to nothing.
func _queue_settings_state(band: Dictionary, queued: Array, drawn: int) -> Dictionary:
    var closed := {"index": -1, "legs": 0, "crop": false, "kit": false,
        "one_line": _queue_settings_one_line()}
    if _queue_open_key == "":
        return closed
    for index in range(drawn):
        var entry: Dictionary = queued[index] as Dictionary
        if String(entry.get("key", "")) != _queue_open_key:
            continue
        var state := _queue_settings_content(band, entry)
        if int(state["legs"]) <= 0 and not bool(state["crop"]) and not bool(state["kit"]):
            break
        state["index"] = index
        return state
    _queue_open_key = ""
    return closed

## **THE WIDTH THE SETTINGS STRIP ACTUALLY GETS**, and the ONE derivation of it — the zone's own box
## less the strip's chrome (`HudStyle.work_inspector_stylebox` is the role card's, so its content
## margin is `ROLE_CARD_PADDING` on each side). The reservation and the builder both read this, which
## is what stops the flow predicate being asked about one width and answered for another.
func _queue_settings_line_width() -> float:
    return _zone_box().x - float(HudStyle.ROLE_CARD_PADDING) * 2.0

## …and the predicate itself, so no caller spells the arithmetic.
func _queue_settings_one_line() -> bool:
    return HudWorkVocab.queue_settings_one_line(_queue_settings_line_width())

## **WHAT ONE ENTRY'S STRIP WOULD HOLD** — `{legs, crop}`, and `{0, false}` for an entry with nothing
## to show. ONE predicate decides the row's clickability, the strip's existence and its height, so a
## row can never invite a click that opens an empty strip.
##
## **A ONE-LEG ENTRY STILL LISTS ITS LEG.** The list is what says how far this job goes, and an entry
## that showed nothing until it was two legs long would make the single-leg case the odd one out —
## which is the common case on the animal web, whose entries are always one leg.
## **EVERY QUEUED ENTRY HAS A KIT, SO EVERY QUEUE ROW EXPANDS NOW** — including the hunt/tame rows
## that carried `legs == 0, crop == false` and therefore did not (`docs/plan_standing_upkeep.md`
## §4.7a ②). That is not a regression in the *only expandable when there is something to show* rule;
## it is that rule reaching its second setting. A `Tame` commits no species and it is still raised
## with a tool.
func _queue_settings_content(band: Dictionary, model: Dictionary) -> Dictionary:
    return {
        "legs": (model.get("build_legs", []) as Array).size(),
        "crop": not _queue_crop_choices(band, model).is_empty(),
        "kit": not _queue_kit_choices(band, model).is_empty(),
        "one_line": _queue_settings_one_line(),
    }

## Open this entry's settings, or close them if they are already open — the queue's twin of
## `_toggle_work_inspector`, and one at a time for the same reason: the strip costs the block height
## the board would otherwise have as rows.
func _toggle_queue_settings(key: String) -> void:
    _queue_open_key = "" if _queue_open_key == key else key
    # ⛔ **ONE EXPANSION OPEN AT A TIME IN THE WHOLE ZONE**, not one per list
    # (`docs/plan_standing_upkeep.md` §4.7b). See `_toggle_work_inspector` for the defect this closes.
    if _queue_open_key != "":
        _work_open_key = ""
        _work_picker_open = HudWorkVocab.WORK_PICKER_NONE
    _repage_work_zone()

## **THE OPEN ENTRY'S SETTINGS — the crop today, the KIT beside it in §4.7a ②.** That is the reason
## this is a strip rather than another column: the kit override is per QUEUE ENTRY (the sim resolves
## it that way, and the Builders card's per-BAND picker was deleted for being unable to say it), so
## the row would need a sixth column to carry it and already could not afford five.
##
## It wears the work inspector's own stylebox and its own reserved height, so the two expansions in
## this zone read as one idea.
func _build_queue_settings_strip(band: Dictionary, model: Dictionary) -> PanelContainer:
    var content := _queue_settings_content(band, model)
    var strip := PanelContainer.new()
    strip.set_meta(HudWorkVocab.BUILD_QUEUE_SETTINGS_META, String(model.get("key", "")))
    strip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    strip.custom_minimum_size = Vector2(0.0, HudWorkVocab.build_queue_settings_height(
        int(content["legs"]), bool(content["crop"]), bool(content["kit"]),
        bool(content["one_line"])))
    strip.add_theme_stylebox_override("panel", HudStyle.work_inspector_stylebox())
    var column := VBoxContainer.new()
    column.add_theme_constant_override("separation", 0)
    strip.add_child(column)
    # **THE ENTRY'S CLIMB, ONE LINE PER LEG** (`docs/plan_standing_upkeep.md` §2.8). A `sow` declared
    # on untended ground is TWO legs and is still ONE queue row: splitting it would offer two `✕`s for
    # one withdrawal and two places to drag for one reorder, so the entry stays one unit and its legs
    # are what the row opens into. The wire lists them first-incomplete first, so the FIRST is the leg
    # in flight and nothing here decides which.
    var legs: Array = model.get("build_legs", []) as Array
    if not legs.is_empty():
        column.add_child(_build_queue_legs_head())
        for index in range(legs.size()):
            column.add_child(_build_queue_leg_line(legs[index] as Dictionary,
                index == SourceForecast.BUILD_QUEUE_HEAD))
    # **THE TWO CONTROLS FLOW: one line where the strip is wide enough for both, two where it is
    # not** (`docs/plan_standing_upkeep.md` §4.7b ②). Ray, on the layout: *"make it flow, so on
    # horizontal layouts it would be 1 line and vertical 2, most likely because of space available."*
    #
    # ⛔ **THE WRAP IS THE PREDICATE'S, NEVER A CONTAINER'S.** `HudWorkVocab.queue_settings_one_line`
    # answers off the strip's width, and the RESERVATION above reads the same answer — a flow
    # container that wrapped at layout time would leave `build_queue_settings_height` unable to know
    # how many lines were drawn, and this zone takes that difference off the bottom of the board in
    # silence. Neither picker ever shrinks: the widths are fixed and the LINE COUNT is what gives.
    var one_line := bool(content["one_line"])
    var line: HBoxContainer = null
    if bool(content["crop"]):
        line = _build_queue_settings_line(column, HudWorkVocab.BUILD_QUEUE_SETTINGS_CROP_KEY)
        var crop_picker := _build_queue_crop_picker(band, model)
        if crop_picker != null:
            line.add_child(crop_picker)
    if bool(content["kit"]):
        # The kit shares the crop's line only where the pair fits; otherwise it opens its own, and the
        # two KEYS line up because both declare `BUILD_QUEUE_SETTINGS_KEY_WIDTH`.
        if line == null or not one_line:
            line = _build_queue_settings_line(column, HudWorkVocab.BUILD_QUEUE_SETTINGS_KIT_KEY)
        else:
            line.add_child(_build_queue_settings_key(
                HudWorkVocab.BUILD_QUEUE_SETTINGS_KIT_KEY))
        var kit_picker := _build_queue_kit_picker(band, model)
        if kit_picker != null:
            line.add_child(kit_picker)
    # **THE WITHDRAWAL RIDES THE STRIP'S LAST LINE, RIGHT-ALIGNED** (§4.7b ③). The `✕` left the row
    # when the reorder arrows took its 32px column, and the strip is where it went: every queued entry
    # expands, so there is always a line to hang it on, and withdrawing becomes two clicks where
    # reordering is one — the right way round, a reorder being the commoner act.
    #
    # ⛔ **IT ADDS NO LINE, and the predicate is what pays for that.** The strip already stacks to two
    # lines on every shipped dock and this zone reads its full height, so a button that opened a THIRD
    # line would come off the bottom of the board in silence. `queue_settings_one_line_width` counts
    # the button and its separation, so the wrap the reservation reads is the wrap this line draws.
    if line == null:
        # A strip with no pickers at all — legs only, which nothing the sim publishes reaches, every
        # queued entry having a kit. `build_queue_settings_height` takes the same branch, so the line
        # bought here is a line that was reserved.
        line = _build_queue_settings_line(column, "")
    var spacer := Control.new()
    spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(spacer)
    line.add_child(_build_queue_unqueue_button(band, model))
    return strip

## **THE WITHDRAWAL — same button, same command, same optimistic write; only its HOST moved** (§4.7b
## ③). It keeps `BUILD_QUEUE_UNQUEUE_META` and the entry's own rank on it, so every reader that found
## it by name finds it in the strip.
##
## **NO CONFIRM.** `unqueue` withdraws a DECLARATION: the banked meter survives it, the row keeps its
## crew and its kit, and re-declaring is one press of that row's own `⌃`. This panel's confirm path is
## for an act that loses something (`_confirm_destructive`), which is the parties zone's
## cancel-versus-recall rule read one surface over.
func _build_queue_unqueue_button(band: Dictionary, model: Dictionary) -> Button:
    var withdraw := Button.new()
    # The entry's own rank — a FINDER value, never asserted on, and read off the SAME model stamp its
    # row wears, so the two cannot state one entry's place two ways.
    withdraw.set_meta(HudWorkVocab.BUILD_QUEUE_UNQUEUE_META, _build_queue_row_rank(model))
    withdraw.text = HudWorkVocab.BUILD_QUEUE_UNQUEUE_GLYPH
    withdraw.focus_mode = Control.FOCUS_NONE
    withdraw.tooltip_text = HudWorkVocab.BUILD_QUEUE_UNQUEUE_TOOLTIP
    withdraw.custom_minimum_size = Vector2(HudWorkVocab.BUILD_QUEUE_UNQUEUE_WIDTH, 0.0)
    HudStyle.apply_button(withdraw, "ghost")
    # The parties zone's recall treatment: a steady, full-opacity DANGER red, because the steady red
    # already reads as destructive and there is nothing further to brighten to on hover. It squeezes
    # its chrome for the same reason every control in this strip does — the default padding busts the
    # control line's declared height.
    HudWidgets.compact(withdraw, HudWorkVocab.WORK_ROW_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
    withdraw.add_theme_color_override("font_color", HudStyle.DANGER)
    withdraw.pressed.connect(func() -> void: _emit_unqueue(band, model))
    return withdraw

## One control line inside the settings strip: a fixed-height row led by its declared-width key. The
## height is `BUILD_QUEUE_SETTINGS_CONTROL_HEIGHT`, the strip's control half — DECLARED here rather
## than left to whatever the picker inside happens to draw, so the line the reservation prices is the
## line that renders. The strip's own padding is counted once by `build_queue_settings_height`
## (`BUILD_QUEUE_SETTINGS_CHROME`) and must not be charged again per line.
func _build_queue_settings_line(column: VBoxContainer, key_text: String) -> HBoxContainer:
    var line := HBoxContainer.new()
    line.custom_minimum_size = Vector2(0.0, HudWorkVocab.BUILD_QUEUE_SETTINGS_CONTROL_HEIGHT)
    line.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    column.add_child(line)
    line.add_child(_build_queue_settings_key(key_text))
    return line

## A settings key — `CROP` / `KIT` — at the ONE declared width both take, so the two pickers share a
## left edge whether they sit side by side or stacked.
func _build_queue_settings_key(key_text: String) -> Label:
    var key := Label.new()
    key.text = key_text
    key.custom_minimum_size = Vector2(HudWorkVocab.BUILD_QUEUE_SETTINGS_KEY_WIDTH, 0.0)
    key.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    key.add_theme_font_size_override("font_size", HudWorkVocab.WORK_CHIP_FONT_SIZE)
    key.mouse_filter = Control.MOUSE_FILTER_IGNORE
    return key

## The leg list's own key, in the CROP key's register — so the two halves of the strip read as one
## expansion rather than as two panels that happened to open together.
func _build_queue_legs_head() -> Label:
    var head := Label.new()
    head.text = HudWorkVocab.BUILD_QUEUE_LEGS_KEY
    head.custom_minimum_size = Vector2(0.0, HudWorkVocab.BUILD_QUEUE_LEG_HEIGHT)
    head.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    head.add_theme_font_size_override("font_size", HudWorkVocab.WORK_CHIP_FONT_SIZE)
    head.mouse_filter = Control.MOUSE_FILTER_IGNORE
    return head

## One leg: the rung it raises, what it still owes FROM WHERE THE SOURCE STANDS, and its own chained
## date — all three the wire's, none of them re-derived here.
##
## **THE LEG IN FLIGHT WEARS THE QUEUE HEAD'S OWN MARKER**, and every other leg reserves its slot: a
## conditionally-omitted marker would shift the legs behind it sideways, which is the block's standing
## rule one level in. It also takes the cyan a rung under construction wears on the board, so *which
## one is happening now* is answerable without reading the marker column.
##
## **A LEG THE WIRE DATES WITH A SENTINEL STATES ITS WORK AND NO TURN.** A leg cannot be dated when
## the entry carrying it cannot, and a fabricated number here would be worse than the silence.
func _build_queue_leg_line(leg: Dictionary, in_flight: bool) -> HBoxContainer:
    var line := HBoxContainer.new()
    line.custom_minimum_size = Vector2(0.0, HudWorkVocab.BUILD_QUEUE_LEG_HEIGHT)
    line.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    var marker := Label.new()
    marker.text = HudWorkVocab.BUILD_QUEUE_LEG_MARKER if in_flight else ""
    marker.custom_minimum_size = Vector2(HudWorkVocab.BUILD_QUEUE_MARKER_WIDTH, 0.0)
    marker.add_theme_color_override("font_color", HudStyle.SIGNAL)
    marker.add_theme_font_size_override("font_size", HudWorkVocab.BUILD_QUEUE_LEG_FONT_SIZE)
    marker.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(marker)
    var improvement := String(leg.get("improvement", ""))
    var name := HudWorkVocab.RUNG_TRACK_NAME_FORMAT % [
        FoodIcons.for_policy(improvement), DetailFormat.rung_badge_word(improvement)]
    var work := DetailFormat.format_work_units(
        float(leg.get(SourceForecast.BUILD_LEG_WORK_KEY, 0.0)))
    var turns := DetailFormat.build_turns_clause(
        int(leg.get(SourceForecast.BUILD_LEG_TURNS_KEY, SourceForecast.BUILD_TURNS_NO_ESTIMATE)))
    var face := HudWorkVocab.BUILD_QUEUE_LEG_UNDATED_FORMAT % [name, work] if turns == "" \
        else HudWorkVocab.BUILD_QUEUE_LEG_FORMAT % [name, work, turns]
    var label := Label.new()
    label.set_meta(HudWorkVocab.BUILD_QUEUE_LEG_META, improvement)
    label.text = face
    label.clip_text = true
    label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.add_theme_color_override("font_color",
        HudStyle.SIGNAL_DEEP if in_flight else HudStyle.INK_DIM)
    label.add_theme_font_size_override("font_size", HudWorkVocab.BUILD_QUEUE_LEG_FONT_SIZE)
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(label)
    return line

## The block's head — `BUILD QUEUE` and, on the right, who is funding it and with what.
##
## **THE ZERO-BUILDERS BRANCH IS THE POINT OF THE READOUT, not a fallback.** Reported from play: a
## Cultivate that was not progressing, with nothing on any surface saying why. The pool is the whole
## reason a queue moves, so its absence is stated where the queue is, in the WARN ink and naming the
## card that fixes it.
##
## **THE KIT COMES FROM `_role_kit_id`, THE SAME RESOLUTION THE BUILDERS CARD'S GEAR LINE STATES.**
## One call, so the header and the card cannot name two different webs' tools for one pool.
func _build_build_queue_head(band: Dictionary, builders: int) -> HBoxContainer:
    var head: HBoxContainer
    if builders <= 0:
        head = HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_BUILD_QUEUE,
            HudWorkVocab.BUILD_QUEUE_NO_BUILDERS_NOTE, null, HudStyle.WARN,
            HudWorkVocab.BUILD_QUEUE_BUILDERS_TOOLTIP)
    else:
        var kit_face := KitRoster.display_name_for_id(_band_labor.kits(),
            _role_kit_id(band, HudConst.LABOR_KIND_BUILDERS))
        head = HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_BUILD_QUEUE,
            HudWorkVocab.BUILD_QUEUE_BUILDERS_FORMAT % [builders, kit_face], null, HudStyle.INK_DIM,
            HudWorkVocab.BUILD_QUEUE_BUILDERS_TOOLTIP)
    # **THE HEAD IS THE EXPANSION'S TOGGLE IN BOTH MODES AND ON BOTH FORKS** (§4.9 item 9c) — a band with no
    # builders has a queue to read like any other, and the `⚠` head is the one it has.
    _make_queue_head_a_toggle(head)
    return head

## One queue entry: the head marker, the source's mark, the job face, its date, and the withdrawal.
##
## **IT IS EXACTLY `WORK_ROW_HEIGHT` AND WEARS THE BOARD ROW'S STYLEBOX**, deliberately — the two
## lists read at one density, and the capacity arithmetic above divides by that number.
##
## **THE MARKER SLOT IS RESERVED ON EVERY ROW.** A conditionally-omitted Label would shift every row
## behind the head sideways, which reads as a list that has lost its alignment rather than as a head.
## **THE ROW'S RANK IS ITS PLACE IN THIS BAND'S OWN WIRE QUEUE**, read off the model stamp
## (`_build_queue_row_rank`) — `NOT_IN_ANY_BUILD_QUEUE` for a pending tail row, whose place no queue
## has named. It replaced `build_queue_position` on this meta because that field is the WINNING
## band's answer (§4.9 item 9a) and every reader of the meta was taking it for this band's; it is
## **not** the block's index into the list it drew, because the drawn list can be short of the wire's.
## **`confirmed` IS HOW MANY ENTRIES THE BAND'S WIRE QUEUE CARRIES**, which is what the `▼` end-stop
## reads: an entry at the bottom of a TRUNCATED page still has a place below it to fall to, so the
## count is the queue's and never the page's.
func _build_build_queue_row(band: Dictionary, model: Dictionary, is_head: bool,
        builders: int, confirmed: int) -> PanelContainer:
    var row := PanelContainer.new()
    row.set_meta(HudWorkVocab.BUILD_QUEUE_ROW_META, _build_queue_row_rank(model))
    row.custom_minimum_size = Vector2(0.0, HudWorkVocab.WORK_ROW_HEIGHT)
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # **CLICKING AN ENTRY OPENS ITS SETTINGS, and only an entry that HAS any is clickable**
    # (`docs/plan_standing_upkeep.md` §4.7a ②, ③). The crop left the compose sheet for the job it
    # belongs to, and then left the ROW: five columns in the tall LEFT dock ellipsised the job face and
    # the crop into fragments, and a tooltip cannot repair a list a player is reading DOWN. The
    # expansion is the work board's own inspector pattern, one open at a time.
    #
    # **ONE PREDICATE DECIDES BOTH the invitation and the contents** (`_queue_settings_content`), so a
    # row can never offer a click that opens an empty strip. It was the crop alone, and an animal entry
    # — a Tame committing no species — was therefore never expandable; §2.8's LEGS are the second
    # thing an entry always has, so both webs open now and the strip's content is what differs.
    var content := _queue_settings_content(band, model)
    var expandable := int(content["legs"]) > 0 or bool(content["crop"]) or bool(content["kit"])
    var open := expandable and String(model.get("key", "")) == _queue_open_key
    row.add_theme_stylebox_override("panel", HudStyle.work_row_stylebox(open))
    if expandable:
        row.mouse_filter = Control.MOUSE_FILTER_STOP
        row.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
        # ⛔ **ON THE RELEASE, NOT THE PRESS — BECAUSE THE PRESS IS WHERE A DRAG BEGINS.**
        # `_toggle_queue_settings` ends in `_repage_work_zone`, which frees every node in the zone.
        # Fired on the PRESS, that rebuild ran before the pointer had travelled far enough for Godot
        # to ask the marker for drag data: the Viewport's `mouse_focus` was the marker Label, the
        # rebuild took it out of the tree, `_gui_remove_control` nulled the focus, and no drag was
        # ever attempted. The reorder gesture therefore degraded to *a click that opens the settings
        # strip* — which is exactly how it played — and it shipped that way because the harness drove
        # the drag callables directly and never pressed a real mouse button. The reproduction is
        # `band_panel_preview._assert_queue_reorder_by_real_gesture`, which pushes the events.
        #
        # **The release is also the event a completed drag CONSUMES**: the Viewport performs the drop
        # on the button-up and never forwards it to `gui_input`, so a reorder cannot also open the row
        # it moved. **And it must land INSIDE the row** — `mouse_focus` latches on the press, so a
        # press here released three rows away would otherwise still toggle this one, which is the rule
        # `BaseButton` keeps for the same reason.
        row.gui_input.connect(func(event: InputEvent) -> void:
            if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT \
                    and not event.pressed \
                    and Rect2(Vector2.ZERO, row.size).has_point(event.position):
                _toggle_queue_settings(String(model.get("key", ""))))
    var line := HBoxContainer.new()
    line.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    row.add_child(line)
    line.add_child(_build_queue_row_marker(band, model, is_head))
    # **AND THE ROW IS THE DROP TARGET, where the marker alone is the grab** — a drop that only
    # landed on a 10px column would be a gesture the player has to aim at twice.
    if not _build_queue_row_is_pending(model):
        row.set_drag_forwarding(Callable(), _queue_can_drop.bind(String(model.get("key", ""))),
            _queue_drop.bind(band, String(model.get("key", ""))))
    # **THE SOURCE MARK LEFT THIS ROW WHEN THE DATE COLUMN LEARNED A VERB** (§2.8). The queue row was
    # five slots on a ~338px line and the pair of columns that carry information — the job face and
    # the date — measured 291 of the 274 the icon left them: the face is a shipped, play-reported
    # unclipped guarantee (`band_panel_preview._assert_queue_row_settings`) and a clipped date is a
    # date column that has stopped stating a date, so neither could give. **The icon was the one slot
    # with no informational duty**: the face names the source in words (`Sow (28, 19)` /
    # `Corral Red Deer`) and carries the destination's own rung glyph, so a second mark said nothing
    # the row did not. The board's rows keep theirs — they are a list of SOURCES and their names do
    # not lead with a glyph. The `+N more` row's spacer was already reserving the marker alone, so the
    # block's own two rows line up now where they did not before.
    var face := _build_queue_job_face(model)
    var label := Label.new()
    label.set_meta(HudWorkVocab.BUILD_QUEUE_FACE_META, face)
    label.text = face
    label.clip_text = true
    label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.add_theme_color_override("font_color", HudStyle.INK)
    label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(label)
    # **THE DATE IS A COMPLETION TURN HERE, NOT A COUNTDOWN** (`docs/plan_standing_upkeep.md` §4.7).
    # The counts are CHAINED down the queue, so `≈42` / `≈61` / `≈98` read as three independent spans
    # when they are cumulative — reported from the 6b playtest. `DetailFormat.build_completion_value`
    # states the turn instead, and shares `build_sentinel_value` with the countdown every other
    # surface keeps: ONE fork answers every sentinel the wire can put on `buildTurnsRemaining`, which
    # is what stops a newly-spelled value being missed here (it has been, twice).
    #
    # **A PENDING ENTRY STATES NO DATE AT ALL.** The chain has not placed it, so any number here would
    # be invented. The slot carries the client's one spelling of pending instead — `○` in amber,
    # `FoodIcons`' `STATUS_PENDING`, the same mark the work rows' status clause and the map's dashed
    # overlays wear — rather than a second vocabulary for the same idea.
    #
    # **THE PERCENTAGE AND ITS VERB ARE THE LEG IN FLIGHT'S** (`docs/plan_standing_upkeep.md` §2.8).
    # A `sow` on untended ground is a two-leg entry, and quoting the DESTINATION's meter left this
    # column reading `turn 83 (0%)` for the thirty-nine turns the crew spent clearing — a number that
    # answers a question nobody asked, and that contradicts the tile card. So the column reads
    # `Cultivating 18% · turn 83`: the title still names what was ordered, the date is still the whole
    # climb's, and the verb is what makes the percentage attributable to a rung the title does not
    # name. A single-leg entry's leg IS its destination, so it names its own rung unchanged.
    #
    # **AND A RING'S PERCENTAGE IS ITS OWN METER, NOT A LADDER CREDIT.** `BuildJob::ExtendPen` widens
    # the pen rung its herd already stands on, so the entry climbs nothing: the herd reads
    # `Corralled 100%`, the ladder has no leg in flight to credit, and the column read
    # `Corral <herd> — turn 151 (0%)` for the ring's entire life. The wire already distinguishes the
    # two — `resolved_build_job` publishes `extend_pen` rather than a rung verb, because a built pen
    # carries no meter for a verb to name — so the branch is on the token the model already holds, and
    # the number it swaps in is `SourceForecast.pen_extend_fraction`'s, the same division the herd
    # drawer's "Fencing N%" badge quotes. The FACE is untouched: a ring derives the verb of the rung it
    # widens, which is why the row is still titled `Corral <herd>`. The DATE is untouched too — the sim
    # publishes a dedicated ring countdown, and only the percentage was ever missing.
    var pending := _build_queue_row_is_pending(model)
    var turns := int(model.get("build_turns", SourceForecast.BUILD_TURNS_NO_ESTIMATE))
    var is_ring := String(model.get("improvement", "")) == SourceForecast.BUILD_JOB_EXTEND_PEN
    var percent := HudFormat.progress_percent(float(model.get(
        "build_ring_progress" if is_ring else "building_progress", 0.0)))
    var value := FoodIcons.for_status(HudWorkVocab.BUILD_QUEUE_PENDING_STATUS) if pending \
        else DetailFormat.build_completion_value(turns, builders, percent,
            _band_labor.current_turn(), String(model.get("building_policy", "")))
    var date := Label.new()
    date.set_meta(HudWorkVocab.BUILD_QUEUE_DATE_META, value)
    date.text = value
    date.clip_text = true
    date.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    date.custom_minimum_size = Vector2(HudWorkVocab.BUILD_QUEUE_DATE_WIDTH, 0.0)
    date.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    date.add_theme_color_override("font_color",
        HudStyle.WARN if pending else DetailFormat.rung_value_color(value))
    date.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    date.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(date)
    # Both columns clip, so the row's own tooltip carries the pair in full — and the pending row's
    # half is the status glyph's OWN words ("Pending — starts when you advance the turn"), which is
    # where a one-character date column has to say what it means.
    #
    # **ON A REAL COUNT IT CARRIES BOTH READINGS**, which is what actually kills the ambiguity: the
    # column states the date and the hover adds the span, so a player weighing a reorder does not have
    # to subtract. A SENTINEL has no span to state and keeps its single face.
    var date_tooltip := value
    if not pending and DetailFormat.build_sentinel_value(turns, builders, percent) == "":
        date_tooltip = HudWorkVocab.BUILD_QUEUE_ROW_SPAN_FORMAT % [value, turns]
    # **THE CAUSE OF A BLOCK RIDES THE HOVER, AND IT IS THE CARD'S OWN SENTENCE**
    # (`docs/plan_standing_upkeep.md` §4.6b). The date column can only ever say `⚠ Blocked 32%`,
    # which is the state and not the reason; the reason is a sentence, and the one place a 28px row
    # has for one is its tooltip. Empty on every entry that is not blocked, so the hover is unchanged
    # wherever nothing is wrong.
    var blocked_lines: Array = model.get("build_blocked_lines", []) as Array
    var tooltip_lines: Array = [HudWorkVocab.BUILD_QUEUE_ROW_TOOLTIP_FORMAT % [face,
        HudFormat.status_tooltip_line(HudWorkVocab.BUILD_QUEUE_PENDING_STATUS) if pending \
            else date_tooltip]]
    # **AND THE JOB'S FULL PRICE, BOTH HALVES — the work pile and the keeping it will owe.** The row
    # already carries the face and the date; what it never said is that finishing this job commits the
    # band to a standing bill, which is the other half of the decision and the reason the keeping
    # warning used to arrive a turn late. **TOOLTIP ONLY** — the row is four slots and cannot take a
    # sixth. `DetailFormat.build_price_clause` is the same composer the `⌃`'s own hover uses, so the
    # offer and the queued entry quote one price in one wording; `BUILD_TURNS_NO_ESTIMATE` suppresses
    # its turn term deliberately, the date column above being the sim's own chained answer and a
    # second estimate here two producers for one number.
    var price := DetailFormat.build_price_clause(
        float(model.get("build_work_cost", SourceForecast.BUILD_WORK_COST_NONE)),
        SourceForecast.BUILD_TURNS_NO_ESTIMATE,
        float(model.get("build_upkeep_demand", SourceForecast.NO_UPKEEP_DEMAND)),
        SourceForecast.source_kind_for_labor(String(model.get("kind", ""))))
    if price != "":
        tooltip_lines.append(price)
    tooltip_lines.append_array(blocked_lines)
    # **A ROW THAT OPENS HAS TO SAY SO**, the board row's `WORK_ROW_OPEN_HINT` being the pattern. It
    # names the KIT first because every entry has one and only a plant entry has a crop — the sentence
    # that promised the crop alone was false on every animal row the moment §4.7a ②'s kit made those
    # rows expandable.
    if expandable:
        tooltip_lines.append(HudWorkVocab.BUILD_QUEUE_ROW_OPEN_HINT)
    row.tooltip_text = HudFormat.join_tooltip_lines(tooltip_lines)
    line.add_child(_build_queue_reorder_column(band, model, confirmed))
    return row

## **THE REORDER PAIR, IN THE COLUMN THE `✕` USED TO HAVE** (`docs/plan_standing_upkeep.md` §4.7b ③).
##
## **THE DRAG WAS INVISIBLE, WHICH IS WHY THIS EXISTS.** The handle only reveals itself under a press,
## and a control a player cannot see is a control they do not have. The drag survives beside these —
## it works now and it costs the row nothing — but the ARROWS are the primary reorder.
##
## **AND THE PLACEMENT COST ZERO PIXELS, which is the whole reason it is this one.** The row's ~356px
## is spoken for and the job face is already ellipsised at its widest shipped value, so any new column
## comes out of the one column with an unclipped-name guarantee. The `✕` was the only slot with
## somewhere else to go: every queued entry expands into a settings strip, so the withdrawal moves
## THERE and the arrows inherit its 32px. Two clicks to withdraw, one to reorder — which is the right
## way round, a reorder being the commoner act.
##
## **DISABLED, NEVER ABSENT.** The head cannot climb and the last confirmed entry cannot fall, and a
## control that vanished at either end would shift the column under the row beside it.
##
## **A PENDING ROW GETS NEITHER**, the drag handle's own rule: the wire has not placed it, so there is
## no rank for `build_order` to name.
##
## ⛔ **BOTH END-STOPS ARE THE WIRE QUEUE'S, NOT THE DRAWN LIST'S.** `confirmed` is how many entries
## the BAND's wire queue carries and the rank is this entry's index in it, so with `[A, B, C]` drawn
## as `[B, C]` — `A` having no work-source model, which `_build_queue_models`' note shows is a
## reachable and persistent state — `B`'s `▲` is ENABLED, because there really is somewhere above it
## to go. An entry at the bottom of a truncated page likewise still has somewhere to fall to.
func _build_queue_reorder_column(band: Dictionary, model: Dictionary,
        confirmed: int) -> Control:
    var column := HBoxContainer.new()
    column.add_theme_constant_override("separation", HudWorkVocab.BUILD_QUEUE_REORDER_SEPARATION)
    column.custom_minimum_size = Vector2(HudWorkVocab.BUILD_QUEUE_REORDER_WIDTH, 0.0)
    var rank := _build_queue_row_rank(model)
    if _build_queue_row_is_pending(model):
        # The slot is still RESERVED, so the dates above a pending tail row stay in one column.
        column.mouse_filter = Control.MOUSE_FILTER_IGNORE
        return column
    column.add_child(_build_queue_reorder_button(band, model,
        HudWorkVocab.BUILD_QUEUE_PROMOTE_GLYPH, HudWorkVocab.BUILD_QUEUE_PROMOTE_TOOLTIP,
        HudWorkVocab.BUILD_QUEUE_PROMOTE_META,
        rank - HudWorkVocab.BUILD_QUEUE_REORDER_STEP,
        rank <= SourceForecast.BUILD_QUEUE_HEAD))
    column.add_child(_build_queue_reorder_button(band, model,
        HudWorkVocab.BUILD_QUEUE_DEMOTE_GLYPH, HudWorkVocab.BUILD_QUEUE_DEMOTE_TOOLTIP,
        HudWorkVocab.BUILD_QUEUE_DEMOTE_META,
        rank + HudWorkVocab.BUILD_QUEUE_REORDER_STEP,
        rank >= confirmed - HudWorkVocab.BUILD_QUEUE_REORDER_STEP))
    return column

## One arrow. **IT SENDS THE SAME `build_order` THE DRAG DOES**, at the position it is valued with —
## same command, same payload, nothing new on the wire.
##
## ⛔ **AND NO OPTIMISTIC ORDERING, for `_emit_build_order`'s own reason**: `buildQueue` is captured
## live off the allocation the command mutates, so the new order arrives on THIS command's recapture.
## An overlay here would be a second ordering beside the wire's — the drift §4.9 forbids.
##
## **THE SIDES ARE TRIMMED AS WELL AS THE TOP** (`HudWidgets.compact`'s `padding_h`): the ghost
## button's 11px side margins are what made a 9px `✕` need 32, and a pair sharing that 32 has room for
## neither pair of them.
func _build_queue_reorder_button(band: Dictionary, model: Dictionary, glyph: String,
        tooltip: String, meta: String, position: int, disabled: bool) -> Button:
    var button := Button.new()
    button.set_meta(meta, position)
    button.text = glyph
    button.focus_mode = Control.FOCUS_NONE
    button.tooltip_text = tooltip
    button.custom_minimum_size = Vector2(
        HudWorkVocab.BUILD_QUEUE_REORDER_BUTTON_WIDTH, 0.0)
    button.size_flags_vertical = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(button, "ghost")
    HudWidgets.compact(button, HudWorkVocab.BUILD_QUEUE_REORDER_FONT_SIZE,
        HudWorkVocab.WORK_PAGER_PADDING_V, HudWorkVocab.BUILD_QUEUE_REORDER_PADDING_H)
    button.disabled = disabled
    button.pressed.connect(func() -> void: _emit_build_order(band, model, position))
    return button

## **THE MARKER COLUMN, WHICH IS ALSO THE GRAB HANDLE** (`docs/plan_standing_upkeep.md` §4.7b ③).
##
## **NO NEW COLUMN, AND THE ARITHMETIC IS WHY.** The row's ~356px is spoken for — marker 10, face,
## date 168, the reorder pair's 32 and four separations — and the face is already ellipsised at its widest shipped
## value, so a handle of its own would come straight out of the one column with an unclipped-name
## guarantee on it. This slot is reserved on every row already (that is what lines the faces up) and
## holds NOTHING on a non-head row, which makes it the only spare pixels in the row.
##
## **THE HEAD KEEPS `▸`, because that marker is load-bearing**: it names the entry the builders pool
## is standing on. It is still a handle — demoting the head is the most likely reorder there is — it
## simply does not swap its glyph for the grab one.
##
## **A PENDING ROW GETS NEITHER.** The wire has not placed it, so there is no position for
## `build_order` to name and nothing it can be dragged above.
func _build_queue_row_marker(band: Dictionary, model: Dictionary, is_head: bool) -> Label:
    var pending := _build_queue_row_is_pending(model)
    var marker := Label.new()
    marker.set_meta(HudWorkVocab.BUILD_QUEUE_MARKER_META, is_head)
    marker.custom_minimum_size = Vector2(HudWorkVocab.BUILD_QUEUE_MARKER_WIDTH, 0.0)
    marker.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    if is_head:
        marker.text = HudWorkVocab.BUILD_QUEUE_HEAD_MARKER
        marker.add_theme_color_override("font_color", HudStyle.SIGNAL)
    elif pending:
        marker.text = ""
        marker.mouse_filter = Control.MOUSE_FILTER_IGNORE
        return marker
    else:
        marker.text = HudWorkVocab.BUILD_QUEUE_DRAG_HANDLE
        marker.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    if pending:
        marker.mouse_filter = Control.MOUSE_FILTER_IGNORE
        return marker
    marker.tooltip_text = HudWorkVocab.BUILD_QUEUE_DRAG_TOOLTIP
    marker.mouse_default_cursor_shape = Control.CURSOR_MOVE
    # **`PASS`, NOT `STOP`, so the row's own click-to-open-settings still works through it.** Godot
    # only asks for drag data once the pointer has moved past its threshold, so a plain click on the
    # handle is still a click — and with `PASS` the event reaches the row's `gui_input` as well.
    marker.mouse_filter = Control.MOUSE_FILTER_PASS
    # **`set_drag_forwarding` RATHER THAN A SCRIPT PER ROW.** The callables live on this controller,
    # which is where the queue's ordering already lives; a per-node script would put a copy of that
    # knowledge on every row the block rebuilds.
    marker.set_drag_forwarding(
        _queue_drag_data.bind(marker, band, String(model.get("key", ""))),
        Callable(), Callable())
    return marker

# ---- THE REORDER GESTURE (`docs/plan_standing_upkeep.md` §4.7b ③) --------------------------------

## The payload one dragged row carries: its own key, tagged, so a drop target can refuse everything
## that is not one of these rows rather than accepting any Dictionary that happens to have a `key`.
func _queue_drag_data(_at: Vector2, handle: Control, band: Dictionary, key: String) -> Variant:
    if key == "":
        return null
    _queue_drag_key = key
    _queue_drop_key = ""
    var preview := Label.new()
    preview.text = _queue_drag_preview_face(band, key)
    preview.modulate.a = HudWorkVocab.BUILD_QUEUE_DRAG_PREVIEW_ALPHA
    preview.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    handle.set_drag_preview(preview)
    return {"type": HudWorkVocab.BUILD_QUEUE_DRAG_TYPE, "key": key}

## The dragged entry's own job face, so a list of near-identical rows still says which one is in
## flight. Read off the block's own model list rather than off the row's Label, which the next render
## may already have freed.
func _queue_drag_preview_face(band: Dictionary, key: String) -> String:
    for entry_variant in _confirmed_queue_entries(band):
        var entry: Dictionary = entry_variant
        if String(entry.get("key", "")) == key:
            return _build_queue_job_face(entry)
    return key

## **CAN THIS ROW TAKE THE DROP, AND WHICH EDGE WOULD IT LAND ON?** Called on every motion while the
## pointer is over the row, which is also what makes it the indicator's own driver: the answer and the
## mark are one decision, so the line can never point at a row the drop would not use.
func _queue_can_drop(at: Vector2, data: Variant, key: String) -> bool:
    if not (data is Dictionary) or String((data as Dictionary).get(
            "type", "")) != HudWorkVocab.BUILD_QUEUE_DRAG_TYPE:
        return false
    var dragged := String((data as Dictionary).get("key", ""))
    if dragged == "" or dragged == key:
        _queue_show_drop_mark("", true)
        return false
    _queue_show_drop_mark(key, at.y < HudWorkVocab.WORK_ROW_HEIGHT * 0.5)
    return true

## **THE DROP — one `build_order`, and nothing else.**
##
## **THE POSITION IS AN INDEX INTO THE BAND'S OWN QUEUE, NOT A RANK OF OURS** (§4.9): the build queue
## IS the priority property's storage, so the whole gesture is *state the list again* and the number
## is where the dragged entry ended up in it.
##
## ⛔ **AND THE CLIENT DRAWS NOTHING OF ITS OWN WHILE IT WAITS.** The queue is captured live, so the
## reordered list arrives on this command's own recapture; an optimistic ordering here would be a
## second ordering beside the wire's, which is what made a drag paint the requested order and then
## silently jump a turn later.
func _queue_drop(_at: Vector2, data: Variant, band: Dictionary, key: String) -> void:
    if not (data is Dictionary):
        return
    var dragged := String((data as Dictionary).get("key", ""))
    # **THE EDGE IS READ BEFORE THE MARK IS CLEARED.** `_queue_show_drop_mark` owns `_queue_drop_above`
    # and resets it, so taking the flag after the clear reads the reset value — which lands every drop
    # ABOVE its target however the pointer approached it.
    var above := _queue_drop_above
    _queue_show_drop_mark("", true)
    if dragged == "" or dragged == key:
        return
    # ⛔ **THE ARITHMETIC IS DONE IN THE WIRE QUEUE, and the drawn rows only NAME the two entries.**
    # The dragged key and the target key come off rows the player touched; everything after that is
    # counted in `_queue_rank_keys`, because the position is resolved by the sim against its own
    # list. Counted in the drawn list instead, a drag past a hidden entry lands on the wrong side of
    # its target or does nothing at all — with `[A, B, C]` drawn as `[B, C]`, dragging `B` below `C`
    # computed `1`, which the sim resolves straight back to `[A, B, C]`. It is `2`.
    var keys := _queue_rank_keys(band)
    var from := keys.find(dragged)
    var onto := keys.find(key)
    if from < 0 or onto < 0:
        return
    keys.remove_at(from)
    # The target's index moves under the removal when the entry came from above it, and the drop edge
    # is what says which side of the target the row lands on.
    var insert := keys.find(key)
    if not above:
        insert += 1
    var model := {}
    for entry_variant in _confirmed_queue_entries(band):
        if String((entry_variant as Dictionary).get("key", "")) == dragged:
            model = entry_variant as Dictionary
            break
    _emit_build_order(band, model, insert)

## The band's CONFIRMED queue entries, in the order the block draws them — `_queue_drop`'s way of
## recovering the dragged row's MODEL from its key, the payload `build_order` needs to name the
## source.
##
## ⛔ **IT IS NOT WHERE A POSITION IS COUNTED, and it never should have been.** The drop's arithmetic
## moved to `_queue_rank_keys`: this list is the DRAWN one, and it is short of the wire queue by any
## entry whose source has no work-source model (`_build_queue_models`' note has the path). Every
## index counted here was therefore wrong by the number of entries hidden above it — the regression
## this comment used to call "correct all along".
func _confirmed_queue_entries(band: Dictionary) -> Array:
    return _build_queue_models(band, _work_source_models(band, 0)).filter(
        func(m): return not _build_queue_row_is_pending(m as Dictionary))

## **THE DROP INDICATOR, DRAWN INSIDE THE TARGET ROW'S OWN 28px.** The block's rows are flush
## (`separation` 0), so a line drawn BETWEEN two of them would need a height term in
## `build_queue_block_height` — on the reservation side as well as the render side, in a zone that
## clips. Lighting one edge of the row's own stylebox costs the block nothing.
##
## It is a stylebox swap on nodes this controller already holds rather than a re-render, because a
## re-render frees the rows the gesture is standing on.
func _queue_show_drop_mark(key: String, above: bool) -> void:
    if _queue_drop_key == key and _queue_drop_above == above:
        return
    _queue_drop_key = key
    _queue_drop_above = above
    for row_key in _queue_row_nodes:
        var row: Control = _queue_row_nodes[row_key]
        if row == null or not is_instance_valid(row):
            continue
        var open := String(row_key) == _queue_open_key
        row.set_meta(HudWorkVocab.BUILD_QUEUE_DROP_MARK_META,
            "" if String(row_key) != key else (HudWorkVocab.BUILD_QUEUE_HEAD_MARKER if above \
                else HudWorkVocab.BUILD_QUEUE_DRAG_HANDLE))
        if String(row_key) == key:
            row.add_theme_stylebox_override("panel", HudStyle.work_row_drop_stylebox(
                open, above, HudWorkVocab.BUILD_QUEUE_DROP_EDGE_WIDTH))
        else:
            row.add_theme_stylebox_override("panel", HudStyle.work_row_stylebox(open))

## **THE GESTURE ENDED — dropped or cancelled — so the zone may rebuild again.** Both endings come
## through here, which is the whole reason `QueueDragWatcher` exists: a cancel emits nothing at all,
## and a suppression flag lifted only by a successful drop freezes the block for good.
func _on_queue_drag_end() -> void:
    if _queue_drag_key == "":
        return
    _queue_drag_key = ""
    _queue_drop_key = ""
    # The auto-scroll's accumulator is per-GESTURE, not per-panel: a remainder left over from the last
    # drag would be spent on the first frame of the next one.
    _queue_autoscroll_carry = 0.0
    _queue_autoscroll_direction = 0
    _repage_work_zone()

## The watcher node, parented once into the HUD host and reused. Invisible, zero-size and
## input-transparent: it renders nothing and its only job is to hear a notification.
func _ensure_queue_drag_watcher() -> void:
    if _queue_drag_watcher != null and is_instance_valid(_queue_drag_watcher):
        return
    var watcher := QueueDragWatcher.new()
    watcher.name = HudWorkVocab.BUILD_QUEUE_DRAG_TYPE
    watcher.mouse_filter = Control.MOUSE_FILTER_IGNORE
    watcher.custom_minimum_size = Vector2.ZERO
    watcher.on_drag_end = _on_queue_drag_end
    # **AND IT IS THE AUTO-SCROLL'S PUMP TOO** (§4.9 item 9c), for the same reason it is the drag's ear: it
    # is the one `Control` this `RefCounted` controller owns, so it is the only place a per-frame tick
    # bounded by the gesture can live.
    watcher.on_drag_tick = _queue_autoscroll_tick
    _host.add_child(watcher)
    _queue_drag_watcher = watcher

## ⛔ **EDGE AUTO-SCROLL — a drag must be able to reach past the expanded list's viewport** (§4.9 item 9c).
## The arrows name a rank and do not care; the drag names a ROW, and a row it cannot scroll to is a
## row it cannot reorder onto.
##
## **THE PUMP IS PER-FRAME, NOT PER-MOTION.** A player who parks the pointer in the hot band and holds
## still generates no motion events at all, and a scroll that only advanced on movement would be the
## same dead-gesture shape this arc has already shipped once. Two independent guards keep it from
## costing anything otherwise: a drag must be in flight, and the expanded list must be mounted.
##
## **THE DIRECTION TEST READS THE PHYSICAL POINTER** (`Control.get_global_mouse_position`, i.e.
## `Viewport.get_mouse_position`) against the scroll's own rect, not a pushed event's position — which
## is the same quantity Godot localizes a drop with, and therefore the one `Input.warp_mouse` moves
## and a harness can drive. A pointer BEYOND the edge keeps scrolling: a player dragging past the
## bottom of the list expects that.
##
## **IT CHANGES `scroll_vertical` AND NOTHING ELSE.** No rebuild — `_queue_drag_in_flight` already
## holds `_repage_work_zone` and `render_band` off for the duration, and freeing the row under the
## pointer is what ends a drag. Setting the property past its range clamps itself, so the travel stops
## at the bottom with no clamp spelled here.
## ⛔ **`seconds` IS WALL CLOCK, NOT `_process`'s DELTA, AND THAT IS NOT AN OPTIMISATION.** A frame
## delta is scaled by `Engine.time_scale`, and every render harness in `tools/` pins that to **0** for
## determinism (`band_panel_preview`, `ui_preview`, `blend_probe` all do, and `preview_watchdog`
## documents it) — so a pump driven by the frame delta advances by exactly nothing under the only
## thing that can test it, and would have shipped as a feature no frame could see. A pointer-driven
## gesture is wall-clock by nature: it belongs to the player's hand, not to the simulation's clock.
func _queue_autoscroll_tick(seconds: float) -> void:
    if _queue_drag_key == "":
        return
    if _queue_expanded_scroll == null or not is_instance_valid(_queue_expanded_scroll) \
            or not _queue_expanded_scroll.is_inside_tree():
        return
    var scroll := _queue_expanded_scroll
    var rect := scroll.get_global_rect()
    var pointer := scroll.get_global_mouse_position()
    var direction := 0
    if pointer.x >= rect.position.x and pointer.x <= rect.end.x:
        if pointer.y < rect.position.y + HudWorkVocab.BUILD_QUEUE_AUTOSCROLL_MARGIN:
            direction = -1
        elif pointer.y > rect.end.y - HudWorkVocab.BUILD_QUEUE_AUTOSCROLL_MARGIN:
            direction = 1
    if direction != _queue_autoscroll_direction:
        _queue_autoscroll_carry = 0.0
        _queue_autoscroll_direction = direction
    if direction == 0:
        return
    # **AND ONE TICK MAY NEVER MOVE THE LIST MORE THAN ONE ROW.** A hitch — a stalled frame, a window
    # resize, a harness capturing a PNG mid-gesture — hands this an arbitrarily long elapsed time, and
    # a step of several rows teleports the drop target past the row the player was aiming at.
    var travel := _queue_autoscroll_carry + float(direction) \
        * HudWorkVocab.BUILD_QUEUE_AUTOSCROLL_ROWS_PER_SECOND * HudWorkVocab.WORK_ROW_HEIGHT \
        * minf(seconds, HudWorkVocab.BUILD_QUEUE_AUTOSCROLL_MAX_TICK_SECONDS)
    var step := int(travel)
    _queue_autoscroll_carry = travel - float(step)
    if step == 0:
        return
    var before := scroll.scroll_vertical
    scroll.scroll_vertical = before + step
    if scroll.scroll_vertical == before:
        return
    _resolve_queue_drag_hover()

## ⛔ **THE HOVER MUST BE RE-RESOLVED AFTER A STEP, OR THE DROP LANDS ON A STALE ROW.** Godot resolves
## the drag-over control on MOTION, and auto-scrolling under a stationary pointer moves the rows
## without telling it — so both the drop indicator and the drop itself keep naming the row that used
## to be under the pointer. One zero-`relative` motion at the CURRENT pointer, with the left button
## still held so it reads as part of the same gesture, is what makes the engine look again.
##
## At most once per frame and only while a drag is live, both of which the caller's own guards give.
func _resolve_queue_drag_hover() -> void:
    var motion := InputEventMouseMotion.new()
    motion.position = _host.get_viewport().get_mouse_position()
    motion.global_position = motion.position
    motion.relative = Vector2.ZERO
    motion.button_mask = MOUSE_BUTTON_MASK_LEFT
    Input.parse_input_event(motion)

## **THE REORDER COMMAND** — `build_order <faction> <band> <source…> <position>`, 0-based
## (`docs/plan_standing_upkeep.md` §4.7b ③).
##
## ⛔ **NO OPTIMISTIC WRITE AND THEREFORE NO ROLLBACK HANDLE**, which is `build_kit`'s rule reaching
## the second field to be captured live: `PopulationCohortState.buildQueue` comes off the allocation
## the command mutates, and the server re-captures after every command, so the new order arrives on
## THIS command's own recapture. The overlay that used to be recorded here was a second ordering
## beside the wire's — the drift §4.9 forbids — so it went with the defect, and a send that does not
## go now leaves nothing behind to undo.
func _emit_build_order(band: Dictionary, model: Dictionary, position: int) -> void:
    if model.is_empty():
        return
    emit_signal("build_order_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
        "x": int(model.get("x", -1)),
        "y": int(model.get("y", -1)),
        "herd_id": String(model.get("herd_id", "")),
        "position": position,
    })

## `+2 more` — the rest of the queue, at the same row height and in the quiet ink.
##
## **A TRUNCATED LIST WITH NOTHING UNDER IT READS AS THE WHOLE LIST**, which is the faction page's
## standing rule for a capped list applied to the band's own.
##
## **AND IT IS A DOOR NOW** (§4.9 item 9c): pressing it opens the whole queue over the Work zone, where the
## entries it stands for each have a row, both arrows and their own settings strip. It is the SECOND
## door in — the BUILD QUEUE header above is the first, and the only one back out — and it fires on
## the RELEASE INSIDE THE ROW for the reason the header does: the handler rebuilds the zone it is
## standing in.
func _build_build_queue_overflow_row(remaining: int) -> PanelContainer:
    var row := PanelContainer.new()
    row.custom_minimum_size = Vector2(0.0, HudWorkVocab.WORK_ROW_HEIGHT)
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_stylebox_override("panel", HudStyle.work_row_stylebox(false))
    row.set_meta(HudWorkVocab.BUILD_QUEUE_OVERFLOW_META, remaining)
    row.tooltip_text = HudWorkVocab.BUILD_QUEUE_OVERFLOW_TOOLTIP
    row.mouse_filter = Control.MOUSE_FILTER_STOP
    row.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
    row.gui_input.connect(func(event: InputEvent) -> void:
        if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT \
                and not event.pressed \
                and Rect2(Vector2.ZERO, row.size).has_point(event.position):
            _toggle_queue_expanded())
    var line := HBoxContainer.new()
    line.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    row.add_child(line)
    var spacer := Label.new()
    spacer.custom_minimum_size = Vector2(HudWorkVocab.BUILD_QUEUE_MARKER_WIDTH, 0.0)
    spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(spacer)
    var label := Label.new()
    label.text = HudWorkVocab.BUILD_QUEUE_OVERFLOW_FORMAT % remaining
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.add_theme_color_override("font_color", HudStyle.INK_DIM)
    label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(label)
    return row

## The entry's job face — the declared verb plus the source it stands on.
##
## **THE VERB'S WORD AND GLYPH ARE `HudFormat.policy_face`'s**, i.e. the SAME pair the board row's
## in-progress axis states in its own tooltip and the map badge draws. A second table of verb names
## here is how one rung comes to be called two things on one screen.
func _build_queue_job_face(model: Dictionary) -> String:
    # **THE FACE NAMES WHERE THE ENTRY IS GOING, NOT THE LEG IT IS ON** (`docs/plan_standing_upkeep.md`
    # §2.8). An entry climbs every rung between where the source stands and its destination, so a row
    # headed `Cultivate` on a `sow` the player ordered would rename their job to its first leg — and
    # then rename it again when that leg finished. The legs are stated INSIDE the row, where the
    # distinction belongs. A single-leg entry's destination IS its rung, so this reads unchanged there.
    var verb := String(model.get("build_destination", "")).strip_edges().to_lower()
    if verb == SourceForecast.IMPROVEMENT_NONE:
        verb = String(model.get("improvement", "")).strip_edges().to_lower()
    if verb == SourceForecast.IMPROVEMENT_NONE:
        # A queued entry always carries a declaration, but the meter answers for itself if one is
        # ever missing — `building_policy` is `RungGates.rung_in_progress`'s already-resolved rung.
        verb = String(model.get("building_policy", ""))
    var face := HudFormat.policy_face(verb)
    if String(model.get("kind", "")) == SourceForecast.LABOR_KIND_HUNT:
        return HudWorkVocab.BUILD_QUEUE_ANIMAL_FACE_FORMAT % [face,
            _herd_label_for_id(String(model.get("herd_id", "")))]
    return HudWorkVocab.BUILD_QUEUE_PLANT_FACE_FORMAT % [face,
        int(model.get("x", -1)), int(model.get("y", -1))]

## **THE CROPS THIS QUEUE ENTRY MAY COMMIT TO — `[]` for an entry with nothing to choose between**
## (`docs/plan_standing_upkeep.md` §4.7a ③), as `build_option_picker` entries with `Sim picks` leading.
##
## **IT IS THE ONE PREDICATE FOR *IS THIS ROW EXPANDABLE*, and that is why the resolution is split out
## of the builder.** The row's click target, the block's reserved height and the strip's contents all
## have to agree; asking three times is how a row comes to invite a click that opens an empty strip.
##
## **THREE THINGS ANSWER EMPTY, and each is its own refusal.** An ANIMAL entry (taming and penning
## commit no species at all), a patch the client cannot resolve, and a patch whose basket carries no
## plant this rung may LEGALLY take — the last being the same `can_cultivate` / `can_sow` flags the
## compose sheet's retired list greyed a row on.
##
## **THE COMMAND IS THE BAND'S OWN FORAGE ROW, RESTATED.** `_emit_work_assign` re-sends the crew, the
## floor, the improvement and the kit the row already carries and moves the `species` token alone, so
## there is no second builder and no wire change — the crop has always been an `assign_labor` token.
## That is also why the pick is OPTIMISTIC for free: the same emit writes the same pending overlay.
##
## **`""` IS AN ENTRY, NOT AN EMPTY FACE.** It is a real instruction (*take the tile's dominant legal
## plant*), so it leads the list and states itself; `NO_ENTRY_SELECTED` would leave the picker
## claiming nothing had been chosen on the state every patch starts in.
func _queue_crop_choices(band: Dictionary, model: Dictionary) -> Array:
    if String(model.get("kind", "")) != SourceForecast.LABOR_KIND_FORAGE:
        return []
    var rung := String(model.get("improvement", "")).strip_edges().to_lower()
    if rung == SourceForecast.IMPROVEMENT_NONE:
        rung = String(model.get("building_policy", ""))
    var flag := String(RungGates.CROP_LEGALITY_FLAGS.get(rung, ""))
    if flag == "":
        return []
    var x := int(model.get("x", -1))
    var y := int(model.get("y", -1))
    var patch: Dictionary = _band_labor.forage_patch_lookup().get(Vector2i(x, y), {})
    var workers := int(model.get("workers", 0))
    var entries: Array = [{
        "label": HudWorkVocab.BUILD_QUEUE_CROP_DEFAULT_LABEL,
        "species": "",
        "on_pick": func() -> void: _emit_work_assign(
            band, model, workers, RESTATE_STANDING_FLOOR, ""),
    }]
    for entry_variant in SourceForecast.flora_basket_entries(patch.get("composition", [])):
        var entry: Dictionary = entry_variant
        if not bool(entry.get(flag, false)):
            continue
        var species := String(entry.get("species", ""))
        entries.append({
            "label": HudWorkVocab.BUILD_QUEUE_CROP_ENTRY_FORMAT % [
                String(entry.get("display_name", species)), int(entry.get("percent", 0))],
            "species": species,
            "on_pick": func() -> void: _emit_work_assign(
                band, model, workers, RESTATE_STANDING_FLOOR, species),
        })
    # A list holding only `Sim picks` is not a choice, so the row is not expandable — the same answer
    # an animal entry gets, reached by a different route.
    return [] if entries.size() <= 1 else entries

## The crop control itself, for the settings strip. `null` where the entry has no choice to offer,
## which the strip's own host has already checked — a second reading of `_queue_crop_choices` rather
## than a second opinion about it.
func _build_queue_crop_picker(band: Dictionary, model: Dictionary) -> OptionButton:
    var entries := _queue_crop_choices(band, model)
    if entries.is_empty():
        return null
    var chosen := _band_labor.species_for_forage(band,
        int(model.get("x", -1)), int(model.get("y", -1)))
    var selected := 0
    for index in entries.size():
        if String((entries[index] as Dictionary).get("species", "")) == chosen:
            selected = index
    var picker := HudWidgets.build_option_picker(entries, selected,
        String((entries[selected] as Dictionary).get("label", "")),
        HudWorkVocab.BUILD_QUEUE_CROP_TOOLTIP)
    picker.set_meta(HudWorkVocab.BUILD_QUEUE_CROP_PICKER_META, chosen)
    # A DECLARED width rather than the strip's whole span: the key label leads the row and the picker
    # takes what a crop name needs, which is what the retired in-row control could not be given.
    picker.size_flags_horizontal = Control.SIZE_FILL
    picker.custom_minimum_size = Vector2(HudWorkVocab.BUILD_QUEUE_CROP_WIDTH, 0.0)
    HudWidgets.compact(picker, HudWorkVocab.WORK_ROW_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
    return picker

## **THE BUILDERS KITS THIS QUEUE ENTRY MAY BE RAISED WITH — `[]` for an entry with no web at all**
## (`docs/plan_standing_upkeep.md` §4.7a ②), as `build_option_picker` entries.
##
## **IT IS THE ROSTER'S `builders` LIST, BARE-HANDED INCLUDED.** `equipment.json` authors the null kit
## last and the capture preserves that order, so *send them out with nothing* lands at the bottom of
## the menu without this layer knowing which entry is null — and it is a REAL selection here, not the
## absence of one: `kit none` says bare-handed where an omitted token says *derive it*.
##
## **THE `(default)` MARK IS THE DERIVATION, PER ENTRY.** `KitRoster.build_kit_for_branch` off this
## entry's own web is the answer the sim will resolve when the entry reaches the head of the queue —
## the same lookup, asked of the published roster — so the mark names the kit the player would get by
## touching nothing. That is what makes the override legible as an override.
##
## **AND THE OTHER WEB'S TOOL IS GREYED WITH ITS REASON rather than hidden** — `kit_offer`'s builders
## branch, handed this entry's branch. A hoe takes nothing off a `Tame`; a player should learn that
## once, and invisibility is what let the wrong tool be offered in the first place.
func _queue_kit_choices(band: Dictionary, model: Dictionary) -> Array:
    return _queue_kit_listing(band, model)[KitRoster.KIT_ENTRIES_KEY] as Array

## …and the whole listing behind it — the entries, the index to open on and the DERIVED id the pick
## is measured against. One resolution, spent by the predicate above and by the control below, so a
## row cannot offer a choice the picker then marks differently.
func _queue_kit_listing(band: Dictionary, model: Dictionary) -> Dictionary:
    var branch := KitRoster.build_branch_for_kind(String(model.get("kind", "")))
    if branch == KitRoster.BUILD_BRANCH_NONE:
        return {KitRoster.KIT_ENTRIES_KEY: [],
            KitRoster.KIT_ENTRIES_SELECTED_KEY: HudWidgets.NO_ENTRY_SELECTED,
            QUEUE_KIT_DERIVED_KEY: KitRoster.NO_KIT_ID}
    var kits := _band_labor.kits()
    var derived := KitRoster.build_kit_for_branch(kits, branch)
    var listing := KitRoster.kit_entries(kits, KitRoster.JOB_BUILDERS,
        _queue_kit_selection(model, derived), derived,
        func(kit_id: String) -> void: _emit_build_kit(band, model, kit_id, derived),
        {}, "", branch)
    listing[QUEUE_KIT_DERIVED_KEY] = derived
    return listing

## The derived answer's key on `_queue_kit_listing`'s return — the id a pick is compared against, and
## therefore the id whose selection sends NO `kit` token.
const QUEUE_KIT_DERIVED_KEY := "derived"

## **WHAT THE PICKER OPENS ON — the WIRE's resolved kit for this entry, and the derivation while the
## wire has not placed it.** `buildKitId` is captured live and states the RESOLVED kit, so a pick is
## visible on the recapture the command triggers and this control needs no optimistic overlay of its
## own. A row the wire has not seen (a build declared this turn) has no resolved kit at all, and the
## honest face there is the answer the sim is about to reach.
func _queue_kit_selection(model: Dictionary, derived: String) -> String:
    var stated := String(model.get("build_kit_id", "")).strip_edges()
    return stated if stated != "" else derived

## The kit control itself. `null` where the entry offers no choice, which the strip's own host has
## already checked.
##
## ⛔ **IT IS A FIXED-HEIGHT PICKER RATHER THAN `KitRoster.build_kit_row`, and the measurement is
## why.** That helper returns a two-child block whose second child — the `tier_hint` line — is present
## only when the selected kit has something to say, so the row it draws is 22px or ~36px depending on
## the pick. This strip's height is RESERVED before it is drawn in a zone that clips, so a term that
## moves with the selection cannot be in it. The LIST is still the roster's own
## (`KitRoster.kit_entries`); only the chrome is this strip's.
func _build_queue_kit_picker(band: Dictionary, model: Dictionary) -> OptionButton:
    var listing := _queue_kit_listing(band, model)
    var entries: Array = listing[KitRoster.KIT_ENTRIES_KEY]
    if entries.is_empty():
        return null
    var chosen := _queue_kit_selection(model, String(listing[QUEUE_KIT_DERIVED_KEY]))
    var picker := HudWidgets.build_option_picker(entries,
        int(listing[KitRoster.KIT_ENTRIES_SELECTED_KEY]),
        KitRoster.display_name_for_id(_band_labor.kits(), chosen),
        HudWorkVocab.BUILD_QUEUE_KIT_TOOLTIP)
    picker.set_meta(HudWorkVocab.BUILD_QUEUE_KIT_PICKER_META, chosen)
    # A DECLARED width, the crop's own, for the crop's own reason: the key leads the line and the
    # control takes what a kit name needs.
    picker.size_flags_horizontal = Control.SIZE_FILL
    picker.custom_minimum_size = Vector2(HudWorkVocab.BUILD_QUEUE_KIT_WIDTH, 0.0)
    HudWidgets.compact(picker, HudWorkVocab.WORK_ROW_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
    return picker

## **THE PER-ENTRY KIT OVERRIDE** (`docs/plan_standing_upkeep.md` §4.7a ②) — `build_kit`, naming a
## SOURCE and setting a property of that source's queue ENTRY.
##
## ⛔ **PICKING THE DERIVED DEFAULT EMITS NO `kit` TOKEN, AND THAT IS WHAT CLEARS THE OVERRIDE.** The
## sim reads an absent token as *"go back to deriving this entry's kit from its own web"*, so
## `Main._kit_token`'s standing rule — omit the token when the selection equals the default — is
## exactly the right one here and there is no `default` literal to invent. `none` is a different
## statement (bare-handed) and survives the round trip as a real selection.
##
## **NO OPTIMISTIC OVERLAY.** `buildKitId` is captured LIVE rather than turn-written, so the recapture
## this command triggers already carries the new value — the one field in this block that needs no
## client-side shadow.
func _emit_build_kit(band: Dictionary, model: Dictionary, kit_id: String, default_id: String) -> void:
    emit_signal("build_kit_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "x": int(model.get("x", -1)),
        "y": int(model.get("y", -1)),
        "herd_id": String(model.get("herd_id", "")),
        "kit_id": kit_id,
        "default_kit_id": default_id,
    })

## The withdrawal. **The payload is `DrawerComposeController`'s, key for key**, so `Main.format_unqueue`
## serves both surfaces unchanged: `unqueue <faction> <x> <y>` for a patch, `unqueue <faction>
## <herd_id>` for a herd, told apart by a non-empty herd id.
## **THE DECLARATION, FROM THE WORK ROW'S `⌃`** (`docs/plan_standing_upkeep.md` §4.7a ①) — the verb
## the source's next rung names, on the source the row already stands for.
##
## **IT SENDS THE VERB AND NOTHING ELSE.** No `assign_labor` rides with it: this band demonstrably
## works this source (that is why the row exists), so the sim's *"an improvement command reaches only
## bands already working the source"* rule needs no staffing command ahead of it — which is exactly
## what the compose sheet's two-command commit existed to guarantee, and exactly why the declaration
## could move here at all. The crew, the floor, the kit and the crop on the wire are all untouched.
##
## **THE RUNG IS THE DESTINATION THE PLAYER PICKED, AND THE VERB IS ALREADY ITS NAME**
## (`docs/plan_standing_upkeep.md` §2.8). The four verbs always were destinations — `cultivate` means
## *take it to Cultivated*, `sow` means *take it to Field* — and with one position per source that
## reading is literal: the queued entry lays every rung between where the source stands and where it
## was sent. **So a destination picker needed no new command and no new token**: it emits the verb
## naming the chosen rung, and the sim works out the legs.
##
## `rung` is `RungLadder.track`'s already-resolved answer for the row the player pressed — the same
## row the figures they read were drawn from — so the track on screen and the verb on the wire cannot
## name different rungs.
##
## `workers` / `floor` / `pending_entity` ride the payload for the relay's optimistic write; see the
## signal's own note for why they are not command tokens.
func _emit_ready_declaration(band: Dictionary, model: Dictionary, rung: String) -> void:
    if rung == SourceForecast.IMPROVEMENT_NONE:
        return
    var herd_id := String(model.get("herd_id", ""))
    # **A HERD'S RUNG IS ADDRESSED AT ITS LIVE TILE, and the row's own is a launch-time one.**
    # `Main.format_improvement` targets `corral` by PLACE (a herd's rung addressed by the pen's) while
    # the model's `x`/`y` come from the assignment, which does not follow a herd that has migrated —
    # so the verb would name the ground the herd left. The row's `focus` link resolves the same way
    # through `_focus_hunt_source`; the fallback is the assignment's tile, for a herd the snapshot no
    # longer carries.
    var live := _band_labor.find_world_herd(herd_id) if herd_id != "" else {}
    emit_signal("improvement_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "improvement": rung,
        "kind": String(model.get("kind", "")),
        "x": int(live.get("x", model.get("x", -1))),
        "y": int(live.get("y", model.get("y", -1))),
        "herd_id": herd_id,
        # The source's CURRENT take crew and floor, restated so the overlay's row is this row: the
        # pending entry REPLACES the merged row it shadows, so a crew of 0 here would blank the very
        # row the declaration is about.
        "workers": int(model.get("workers", 0)),
        "floor": float(model.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR)),
        "pending_entity": int(band.get("entity", -1)),
    })

## **THE DESTINATION PICKER — the `⌃`'s ladder track, floated ABOVE the zone and never inside it**
## (`docs/plan_standing_upkeep.md` §2.8).
##
## **A `PopupPanel` BECAUSE IT IS A WINDOW.** The work zone is FULL — 396 of 396 in height and 354 of
## 356 in width with a row selected, and both budgets ASSERT rather than clip — so a track drawn as a
## block would fail the harness at best and slice the board at worst. A Window cannot change any
## zone's height, which is exactly why the detail breakdowns are popovers and the destructive confirms
## are `ConfirmationDialog`s. It costs the zone nothing at all.
##
## **THE CARD IS REBUILT PER OPEN, NEVER PATCHED.** The track is a function of the source's position,
## the faction's knowledge and whatever entry is queued, all three of which move per snapshot — and a
## card that survived a render would go on offering a rung the source has since climbed. The panel
## node is reused because a Window is expensive; its CONTENT is not.
##
## `anchor` is the control that was pressed, captured for the popup's rect BEFORE anything re-renders.
func _open_rung_track(band: Dictionary, model: Dictionary, anchor: Control) -> void:
    var kind := String(model.get("kind", ""))
    var source := _rung_track_source(model)
    # **THE BAND RIDES IN FOR ITS SHELF** (§2.7): the track's stall warning weighs a rung's material
    # pile against what this band actually holds, which is the one thing on that card the SOURCE
    # cannot answer for.
    var rows := RungLadder.track(kind, source, HudComposeVocab.BARE_FORECAST_PREFIX,
        String(model.get("improvement", "")), _player_knowledge(), band)
    if not RungLadder.has_track(rows):
        return
    var track := _ensure_rung_track()
    # **THE CARD'S MARGIN IS THE CHROME AND IS NEVER FREED — its CHILDREN are.** Clearing the Window's
    # own children instead frees the margin the very line below reaches for, and `queue_free` is
    # deferred, so the first open renders correctly and every later one opens onto an empty card. That
    # is the exact shape this shipped in for one run of the harness.
    var margin := _rung_track_body
    HudWidgets.clear_children(margin)
    margin.add_child(RungLadder.build_track(rows, func(rung: String) -> void:
        _pick_rung(band, model, source, rung, anchor)))
    track.popup(_rung_track_anchor_rect(anchor))

## **A PICKED RUNG EITHER DECLARES OR ASKS FOR A CROP, and which it does is a property of the RUNG.**
##
## **THE PRESS CLOSES THE CARD BEFORE IT EMITS.** The declaration writes the optimistic overlay and
## re-renders the whole zone, which frees the very row this card is anchored to; a card left up over
## the rebuilt board would be showing the track it had just made stale.
##
## **AN ANIMAL RUNG STAYS ONE CLICK** — `tame` and `corral` commit no species, so there is nothing for
## a second step to ask — and so does a plant rung on a patch whose basket carries no plant it may
## legally take: the sim accepts a Sow with no species token and settles it itself, and a step with
## nothing in it is a click that answers nothing.
func _pick_rung(band: Dictionary, model: Dictionary, source: Dictionary, rung: String,
        anchor: Control) -> void:
    if RungLadder.rung_commits_a_crop(rung):
        var crops := RungLadder.crop_choices(source, HudComposeVocab.BARE_FORECAST_PREFIX, rung)
        if not crops.is_empty():
            _open_crop_step(band, model, source, rung, crops, anchor)
            return
    _dismiss_rung_track()
    _emit_ready_declaration(band, model, rung)

## **THE SECOND PAGE OF THE SAME CARD — the crop the plant rung commits to** (§4.15).
##
## **IT DEFAULTS TO NOTHING AND THE PICK IS THE DECLARATION.** The `⌃` used to declare in one click
## and send no species token, so every Sow took the sim's own default — the highest-share legal plant,
## which considers neither what it pays nor the player's take selection — which is how a fertile tile
## got committed to a zero-food cash crop. `Sim picks` is still on the list, last, stating the plant it
## would land on; it is a deliberate choice now rather than what happens when nobody looks.
##
## **IT REUSES THE SAME WINDOW, which is what keeps it free.** The work zone reads 396 of 396 in
## height and both budgets ASSERT rather than clip, so a step drawn as a block would slice the board;
## a Window costs the zone nothing, and rebuilding its CONTENT is the card's standing rule.
##
## **THE CROP GOES FIRST AND THE RUNG SECOND**, and the order is the commands': the crop rides
## `assign_labor`'s own `species` token on the band's existing forage row (`_emit_work_assign`, the
## queue row's picker's path — there is no second builder and no wire change), and the declaration
## follows so its optimistic overlay is the one the rebuilt board reads.
func _open_crop_step(band: Dictionary, model: Dictionary, source: Dictionary, rung: String,
        crops: Array[Dictionary], anchor: Control) -> void:
    var track := _ensure_rung_track()
    var margin := _rung_track_body
    HudWidgets.clear_children(margin)
    margin.add_child(RungLadder.build_crop_step(crops,
        func(species: String) -> void:
            _dismiss_rung_track()
            _emit_work_assign(band, model, int(model.get("workers", 0)),
                RESTATE_STANDING_FLOOR, species)
            _emit_ready_declaration(band, model, rung),
        # **BACK REBUILDS THE TRACK RATHER THAN RESTORING IT**, the card's own never-patched rule: the
        # source's position, the faction's knowledge and whatever entry is queued all move per
        # snapshot, and a step left up across one of those would offer a rung already climbed.
        func() -> void: _open_rung_track(band, model, anchor),
        rung))
    track.popup(_rung_track_anchor_rect(anchor))

## Take the track down, if one is up. Idempotent, and safe before the card has ever been built.
func _dismiss_rung_track() -> void:
    if _rung_track != null and is_instance_valid(_rung_track) and _rung_track.visible:
        _rung_track.hide()

## The RAW wire source the track reads — the patch off the lookup, or the herd's LIVE dict. Herds
## migrate, so a track must never read the assignment's launch-time target; it is the same resolution
## `_work_source_models` makes for the rung mark, restated here because the model carries the mark's
## ANSWER rather than the dict it came from.
func _rung_track_source(model: Dictionary) -> Dictionary:
    if String(model.get("kind", "")) == SourceForecast.LABOR_KIND_HUNT:
        return _band_labor.find_world_herd(String(model.get("herd_id", "")))
    return _band_labor.forage_patch_lookup().get(
        Vector2i(int(model.get("x", -1)), int(model.get("y", -1))), {})

## Where the card sits: a zero-height rect just under the pressed mark, in SCREEN space (what
## `Popup.popup` wants). `get_screen_transform` folds in the window position and the canvas stretch,
## both of which this HUD has — the `DisclosureController` popover's own anchoring.
func _rung_track_anchor_rect(anchor: Control) -> Rect2i:
    # **AND IT MUST BE IN THE TREE, not merely alive.** A press can arrive on a control the render
    # that answered the previous one has already detached, and `get_screen_transform` on a detached
    # `CanvasItem` is an engine ERROR plus an identity transform — a card at the top-left corner of
    # the screen with nothing to say it came from this row.
    if anchor == null or not is_instance_valid(anchor) or not anchor.is_inside_tree():
        return Rect2i()
    var xform := anchor.get_screen_transform()
    var below := xform * Vector2(0.0, anchor.size.y + HudWorkVocab.RUNG_TRACK_GAP)
    return Rect2i(Vector2i(below), Vector2i.ZERO)

## The track's Window, built once and reused. Styled through `HudStyle` like every other card here.
func _ensure_rung_track() -> PopupPanel:
    if _rung_track != null and is_instance_valid(_rung_track):
        return _rung_track
    var card := PopupPanel.new()
    card.name = HudWorkVocab.RUNG_TRACK_META
    card.set_meta(HudWorkVocab.RUNG_TRACK_META, true)
    card.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
    var margin := MarginContainer.new()
    for side in DisclosureController.POPOVER_MARGIN_SIDES:
        margin.add_theme_constant_override("margin_%s" % side, HudWorkVocab.RUNG_TRACK_PADDING)
    card.add_child(margin)
    _host.add_child(card)
    _rung_track = card
    _rung_track_body = margin
    return card

func _emit_unqueue(band: Dictionary, model: Dictionary) -> void:
    emit_signal("unqueue_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "x": int(model.get("x", -1)),
        "y": int(model.get("y", -1)),
        "herd_id": String(model.get("herd_id", "")),
        # **THE OPTIMISTIC HALF'S TWO KEYS, AND NEITHER IS A COMMAND TOKEN**
        # (`docs/plan_standing_upkeep.md` §4.7b ④). `kind` keys the withdrawal in the overlay
        # (`pending_key`'s own shape) and `pending_entity` is the client-local handle a FAILED send
        # hands back to `drop_pending_unqueue` — `assign_labor`'s rollback shape, exactly. The relay
        # records the withdrawal BEFORE emitting, which is that shape's whole precondition.
        # `Main.format_unqueue` reads neither.
        "kind": String(model.get("kind", "")),
        "pending_entity": int(band.get("entity", -1)),
    })

## The filter chips ARE the summary: counts + per-kind rates, and pressing one filters the board.
## **A chip for an EMPTY set never renders** — a kind the band works none of is dead weight in a row
## that is otherwise live summary, and an always-present `⚠ 0` reads as an alarm. `All` always shows
## (it is the reset), so the row is never empty.
func _build_work_chips(models: Array) -> HFlowContainer:
    var chips := HFlowContainer.new()
    chips.custom_minimum_size = Vector2(0.0, HudWorkVocab.WORK_CHIPS_HEIGHT)
    chips.add_theme_constant_override("h_separation", HudWorkVocab.WORK_CHIP_SEPARATION)
    var forage: Array = models.filter(func(m): return String(m["kind"]) == SourceForecast.LABOR_KIND_FORAGE)
    var hunt: Array = models.filter(func(m): return String(m["kind"]) == SourceForecast.LABOR_KIND_HUNT)
    var attention: Array = models.filter(func(m): return bool(m["attention"]))
    chips.add_child(_build_work_chip(HudWorkVocab.WORK_FILTER_ALL, HudWorkVocab.WORK_CHIP_ALL_FORMAT % models.size(), false))
    # A chip's rate is BOTH products, each only when non-zero (issue #337) — the chip is a per-kind
    # summary of the same rows the head totals, so counting `🦌 2` sources and then quoting only the
    # food-paying one's rate is the same arithmetic that visibly failed in the header.
    if not forage.is_empty():
        chips.add_child(_build_work_chip(HudWorkVocab.WORK_FILTER_FORAGE, HudWorkVocab.WORK_CHIP_KIND_FORMAT % [
            FoodIcons.DEFAULT, forage.size(), _work_chip_rate_text(forage)], false))
    if not hunt.is_empty():
        chips.add_child(_build_work_chip(HudWorkVocab.WORK_FILTER_HUNT, HudWorkVocab.WORK_CHIP_KIND_FORMAT % [
            FoodIcons.HUNT, hunt.size(), _work_chip_rate_text(hunt)], false))
    if not attention.is_empty():
        chips.add_child(_build_work_chip(HudWorkVocab.WORK_FILTER_ATTENTION,
            HudWorkVocab.WORK_CHIP_ATTENTION_FORMAT % attention.size(), true))
    # The READY chip is its own count beside the attention one, never folded into it: trouble and
    # opportunity are different questions, and it is what makes the knowledge-completion moment legible
    # — a track finishes and a dozen rows light up at once.
    var ready: Array = models.filter(func(m): return String(m["ready_policy"]) != "")
    if not ready.is_empty():
        chips.add_child(_build_work_chip(HudWorkVocab.WORK_FILTER_READY,
            HudWorkVocab.WORK_CHIP_READY_FORMAT % ready.size(), false))
    return chips

## A filter chip's rate face: this kind's food and fodder totals, each rendered only when non-zero. A
## forage chip covering only hay-bearing patches states their feed rather than a `0.00` claiming the
## kind produces nothing (issue #449).
func _work_chip_rate_text(models: Array) -> String:
    return SourceForecast.magnitude_components(
        _work_component_sum(models, "rate"), _work_component_sum(models, "fodder_rate"))

## Σ of ONE yield component over a model set — the zone's single summing primitive, so the head's
## three totals and every chip's three totals are added up the same way over the same rows and cannot
## drift. `key` names a model's yield component (`"rate"` = food, `"fodder_rate"`), never a rate
## itself.
func _work_component_sum(models: Array, key: String) -> float:
    var total := 0.0
    for m in models:
        total += float((m as Dictionary).get(key, 0.0))
    return total

func _build_work_chip(filter: StringName, text: String, alert: bool) -> Button:
    var active := _work_filter == filter
    var chip := Button.new()
    chip.text = text
    chip.focus_mode = Control.FOCUS_NONE
    HudStyle.apply_button(chip, "primary" if active else "ghost")
    HudWidgets.compact(chip, HudWorkVocab.WORK_CHIP_FONT_SIZE, HudWorkVocab.WORK_CHIP_PADDING_V)
    if alert and not active:
        chip.add_theme_color_override("font_color", HudStyle.WARN)
    chip.tooltip_text = HudWorkVocab.WORK_CHIP_TOOLTIP
    chip.pressed.connect(func() -> void: _set_work_filter(filter))
    return chip

## TWO-LINE source row. **Line one is IDENTITY AND CONTROLS** — severity stripe · glyph · name ·
## SOURCE-RUNG mark · the rung-on-offer slot · policy/⚠ marks · the −/+ stepper. **Line two is the
## ACCOUNTS**, full width, indented onto the name's own column. Clicking anywhere but the stepper and
## the rung-track button opens the row in the inspector strip.
##
## **THE ACCOUNTS LEFT LINE ONE BECAUSE 356px DOES NOT HOLD BOTH.** The row carries a name, a
## VARIABLE-LENGTH account list, four affordances and a stepper, and everything but the name is
## fixed-width — so the accounts' fixed 46px slot could state one material and count the rest
## (`+0.24 fibre +3`) while the name it was taking pixels from ellipsised on any species longer than
## `Hunt Red Deer`. On its own full-width line the list is stated IN FULL and the name column roughly
## doubles, which is what puts `Hunt Woolly Mammoth` on the board whole. The elide survives on both
## labels as a FLOOR — a `Label` with no overrun behaviour reports its whole text as its minimum
## width, which in this clipping zone lays every row out past the box — but it should not be reachable
## in normal play.
##
## **THE COST IS PAGING, AND IT IS ACCEPTED.** A page falls from 8 rows to 5, so nine sources is two
## pages in most states; the pager already exists for it and the board must not shrink back to fit
## more.
##
## The rung mark and the policy marks are TWO AXES and both are needed: the rung says what the source
## IS (wild / Tended Patch / Field, wild / pastoral / penned), the marks say what is being done to it
## right now. A Tended Patch on Sustain and a Tended Patch on Deplete are different situations.
func _build_work_row(band: Dictionary, model: Dictionary) -> PanelContainer:
    var open := String(model.get("key", "")) == _work_open_key
    var row := PanelContainer.new()
    row.custom_minimum_size = Vector2(0.0, HudWorkVocab.WORK_ROW_TWO_LINE_HEIGHT)
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.mouse_filter = Control.MOUSE_FILTER_STOP
    row.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
    row.tooltip_text = String(model.get("tooltip", ""))
    row.add_theme_stylebox_override("panel", HudStyle.work_row_stylebox(open))
    row.gui_input.connect(func(event: InputEvent) -> void:
        if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
            _toggle_work_inspector(String(model.get("key", ""))))
    # **THE STRIPE IS OUTSIDE BOTH LINES, which is what keeps it the ROW's mark.** It is an
    # `EXPAND_FILL` sibling of the two-line column rather than a child of line one, so it runs the
    # full height of the row; inside line one it would paint the top 28px of a 44px row and read as a
    # mark on the name rather than on the source.
    var body := HBoxContainer.new()
    body.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    row.add_child(body)
    # Severity stripe: WARN when the source is overdrawing or overstaffed, SIGNAL while an edit is
    # still pending, transparent otherwise — so the eye finds trouble without reading a word.
    var stripe := ColorRect.new()
    stripe.custom_minimum_size = Vector2(HudWorkVocab.WORK_ROW_STRIPE_WIDTH, 0.0)
    stripe.size_flags_vertical = Control.SIZE_EXPAND_FILL
    stripe.color = _work_row_stripe_color(model)
    stripe.mouse_filter = Control.MOUSE_FILTER_IGNORE
    body.add_child(stripe)
    # The two lines, stacked. `TWO_LINE_STEPPER_SEPARATION` is the gap `WORK_ROW_TWO_LINE_HEIGHT`
    # reserves, so what the row draws at and what the capacity arithmetic paid for are one expression.
    var col := VBoxContainer.new()
    col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    col.add_theme_constant_override("separation", HudWorkVocab.TWO_LINE_STEPPER_SEPARATION)
    body.add_child(col)
    var line := HBoxContainer.new()
    line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    line.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    col.add_child(line)
    # The SOURCE mark: bundled art where the client has it, the emoji where it does not. The column
    # is the same fixed `WORK_ROW_ICON_WIDTH` either way, so a board mixing art and emoji rows still
    # lines up down the icon column (issue #439).
    line.add_child(HudWidgets.build_marker_icon(
        model.get("icon_texture") as Texture2D, String(model.get("icon", "")),
        HudWorkVocab.WORK_ROW_ICON_WIDTH, HudWorkVocab.WORK_ROW_FONT_SIZE))
    var label := Label.new()
    label.text = String(model.get("label", ""))
    label.clip_text = true
    # A label too long even for the widened column ELLIPSISES rather than hard-cutting: `Hunt Woolly
    # Mamm…` reads as a truncation, `Forage (73, 20` reads as a wrong coordinate.
    label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    label.add_theme_color_override("font_color",
        HudStyle.WARN if bool(model.get("pending", false)) else HudStyle.INK)
    label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    label.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(label)
    # THE SOURCE-RUNG MARK, in its own reserved slot left of the marks — what the source IS, beside
    # what the band is DOING to it. Tinted SIGNAL because a standing rung is a completed investment,
    # the same treatment `DetailFormat.cultivation_value_hex` / `field_value_hex` / `corral_value_hex`
    # give it in the detail readouts; that colour is also what keeps the two glyph families from
    # reading as one compound mark at this size. Empty text on a wild source — the slot stays reserved
    # so the right-anchored furniture lines up down the board.
    var rung := Label.new()
    rung.text = String(model.get("rung_glyph", ""))
    rung.custom_minimum_size = Vector2(HudWorkVocab.WORK_ROW_RUNG_WIDTH, 0.0)
    rung.add_theme_color_override("font_color", HudStyle.SIGNAL)
    rung.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    # PASS, not IGNORE: a Label needs a non-IGNORE filter for its own `tooltip_text` to ever show.
    # Deliberately NOT `HudWidgets.set_label_tooltip`, which sets STOP — the whole row is a click
    # target, and STOP here would make the rung slot a dead 16px hole in it. PASS shows the tooltip
    # AND lets the press bubble to the row's `gui_input`.
    rung.mouse_filter = Control.MOUSE_FILTER_PASS if rung.text != "" else Control.MOUSE_FILTER_IGNORE
    rung.tooltip_text = String(model.get("rung_tooltip", ""))
    rung.set_meta(HudWorkVocab.WORK_ROW_RUNG_META, String(model.get("rung_glyph", "")))
    line.add_child(rung)
    # THE RUNG ON OFFER — the third and last glyph axis on a row, and it is deliberately a SEPARATE
    # slot from the two beside it: `rung_glyph` is what the source IS, `marks` is what the band is
    # DOING, and this is what it COULD BE. Folding it into either would collapse a distinction the
    # whole feature exists to draw.
    #
    # It does NOT touch the severity stripe. That stripe means WARN (overdrawing, overstaffed) or
    # SIGNAL (a pending edit); an opportunity in the same channel would give the one control for
    # finding trouble two meanings.
    var ready_glyph := String(model.get("ready_glyph", ""))
    var building_glyph := String(model.get("building_glyph", ""))
    # UNDER WAY beats ON OFFER in this slot. Before this the slot was empty while a verb was being
    # worked, so a patch you were actively cultivating looked emptier than the untouched one beside it
    # advertising `⌃` — the state the player is WAITING ON was the one state with no mark.
    #
    # **AND A BUILD THAT IS NOT MOVING DOES NOT GET TO WEAR A PERCENT** — the three-way answer the
    # map's source badge has had since `docs/plan_standing_upkeep.md` §4.6a, arriving here. `⚠` and
    # no number when the rung is unstaffed or losing ground; the ordinary face, number and all, when
    # it is climbing OR merely parked with its keeping covered. The verdict is the model's
    # `build_stalled`, which is `SourceForecast.build_is_losing` / `build_is_unstaffed` asked once —
    # never a second reading of the crew count or the percentage sitting right here.
    var ready_face := ""
    var ready_color := HudStyle.SIGNAL
    if building_glyph != "" and bool(model.get("build_stalled", false)):
        ready_face = HudWorkVocab.WORK_ROW_BUILDING_UNSTAFFED_FORMAT % building_glyph
        ready_color = HudStyle.WARN
    elif building_glyph != "":
        ready_face = HudWorkVocab.WORK_ROW_BUILDING_FORMAT % [building_glyph,
            HudFormat.progress_percent(float(model.get("building_progress", 0.0)))]
        ready_color = HudStyle.SIGNAL_DEEP
    elif ready_glyph != "":
        ready_face = HudWorkVocab.WORK_ROW_READY_FORMAT % ready_glyph
    # **THE SLOT OPENS THE DESTINATION TRACK, AND A STALLED BUILD DOES NOT**
    # (`docs/plan_standing_upkeep.md` §2.8, §4.7a ①). Pressing `⌃` used to queue the source's next
    # rung outright; a queue entry names a DESTINATION now and climbs every rung on the way, so the
    # press opens a small ladder track and the PICK is the declaration. The sim's *"an improvement
    # command reaches only bands already working the source"* rule is still satisfied by construction
    # — a row EXISTS because this band works this source.
    #
    # **A RUNNING BUILD OPENS THE SAME TRACK, which is what gives the chosen path a live home.** *How
    # far are we taking this?* is asked most often mid-climb, and until the track existed the only
    # answer was to withdraw the entry and declare again. The work already banked is kept either way
    # — it is a position on the branch, not a purchase of one rung.
    #
    # **A STALLED BUILD STAYS A `Label`, and the shape is the statement.** The `⚠` is a WARNING rather
    # than an offer, and a button under it would invite a click that changes nothing about why the
    # meter is stuck.
    var ready: Control
    var stalled := building_glyph != "" and bool(model.get("build_stalled", false))
    var offers_rung := ready_face != "" and not stalled
    if offers_rung:
        var queue_btn := Button.new()
        queue_btn.text = ready_face
        queue_btn.focus_mode = Control.FOCUS_NONE
        queue_btn.tooltip_text = String(model.get("ready_tooltip", ""))
        HudStyle.apply_button(queue_btn, "ghost")
        # The board's own row treatment — the default button padding alone busts `WORK_ROW_HEIGHT`,
        # which the capacity arithmetic divides by. Same pair the queue row's `✕` takes.
        HudWidgets.compact(queue_btn, HudWorkVocab.WORK_ROW_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
        # AFTER `apply_button`, which writes its own `font_color`: the mark keeps the SIGNAL ink it
        # wore as a Label, and the hover brightening `apply_button` set stays as the affordance.
        queue_btn.add_theme_color_override("font_color", ready_color)
        queue_btn.pressed.connect(func() -> void: _open_rung_track(band, model, queue_btn))
        ready = queue_btn
    else:
        var ready_label := Label.new()
        ready_label.text = ready_face
        ready_label.add_theme_color_override("font_color", ready_color)
        ready_label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
        ready = ready_label
    # The rendered face, on a stable handle: the states differ by one glyph, so an assertion that
    # searched for their text would only confirm the string it had already assumed. It rides whichever
    # node the slot holds, so a harness reading the slot does not have to know which kind it is.
    ready.set_meta(HudWorkVocab.WORK_ROW_BUILD_STATE_META, ready_face)
    # …and WHICH of the three states drew it, because the node TYPE no longer says: a running build's
    # face is a `Button` too now, so `is Button` reads a climb as an offer.
    var build_kind := HudWorkVocab.WORK_ROW_BUILD_KIND_NONE
    if stalled:
        build_kind = HudWorkVocab.WORK_ROW_BUILD_KIND_STALLED
    elif building_glyph != "":
        build_kind = HudWorkVocab.WORK_ROW_BUILD_KIND_BUILDING
    elif ready_glyph != "":
        build_kind = HudWorkVocab.WORK_ROW_BUILD_KIND_OFFER
    ready.set_meta(HudWorkVocab.WORK_ROW_BUILD_KIND_META, build_kind)
    ready.custom_minimum_size = Vector2(HudWorkVocab.WORK_ROW_READY_WIDTH, 0.0)
    # STOP on the button, so the press does not ALSO bubble to the row's `gui_input` and open the
    # inspector under the sheet the click just declared from; IGNORE on a Label, which is the slot's
    # long-standing treatment (the whole row is one click target).
    ready.mouse_filter = Control.MOUSE_FILTER_STOP if offers_rung else Control.MOUSE_FILTER_IGNORE
    line.add_child(ready)
    var marks := Label.new()
    marks.text = String(model.get("marks", ""))
    marks.custom_minimum_size = Vector2(HudWorkVocab.WORK_ROW_MARKS_WIDTH, 0.0)
    # Amber for an overdraw (⚠), an under-KEPT managed herd (its shed ⚠) or a part-built rung nobody
    # is building (its decay ⚠) — all three are trouble the eye must find; INK_DIM otherwise (a plain
    # policy glyph).
    marks.add_theme_color_override("font_color",
        HudStyle.WARN if bool(model.get("warn", false)) or bool(model.get("at_risk", false)) \
            else HudStyle.INK_DIM)
    marks.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    marks.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line.add_child(marks)
    HudWidgets.add_stepper_controls(line, int(model.get("workers", 0)), bool(model.get("can_add", false)),
        func(n: int) -> void: _emit_work_assign(band, model, n), true)
    col.add_child(_build_work_row_accounts(model))
    return row

## LINE TWO — every account this source pays, in full, then the floor, indented onto the name's column.
##
## **IT CARRIES WHAT THE INSPECTOR'S RETIRED SENTENCE CARRIED**, through `_work_row_summary_text`:
## the strip existed because the row could not show everything, and these are the parts it no longer
## has to borrow. See that function for why the FLOOR came with the accounts rather than the accounts
## moving alone.
##
## **IT STATES EVERY ACCOUNT, where the retired 46px slot fell through food → fodder → materials to
## pick ONE.** The fall-through was a width compromise, not a reading: a hay meadow paying meat AND
## feed had to pick, and a four-crop patch could name one material and count three. A full-width line
## has no such choice to make, so it makes none.
##
## **THE ELIDE IS A FLOOR AND SHOULD NOT BE REACHABLE.** A `Label` with autowrap off reports its whole
## text as its minimum width, and this zone's content is anchored full-rect into a host that
## `clip_contents` — so one long line clamps the entire tab's column up to its own width and slices
## the right edge off every row's stepper. `OVERRUN_TRIM_ELLIPSIS` drops that minimum to a pixel; what
## keeps it from being SEEN is the width, which is now the whole row rather than 46px of it.
##
## **THE WHOLE LINE RIDES ITS OWN HOVER, and that is what keeps the elide honest.** The floor's ONLY
## home is this line now — the inspector's sentence is retired and `HudFormat.floor_hint` carries the
## zone's prose rather than the percentage — so a cut that reaches the trailing clause would put a
## number nowhere else on the panel out of reach, which is the one thing an elide may not do. Measured:
## the four-cash-crop worst case asks **332px of a 322px line** and the trim lands on the floor. Every
## other elided line in this zone takes the same treatment (`HudWidgets.build_status_part(…, elide)`
## → `set_label_tooltip`); what is different here is only that the hover is set unconditionally rather
## than on a shortened-string test, since the row cannot know its own allocated width at build time.
##
## **PASS, not the STOP `set_label_tooltip` leaves behind** — the rung slot's rule: the whole row is a
## click target, and STOP across the row's widest control would punch a full-width dead hole in it.
## PASS shows the tooltip AND lets the press bubble to the row's `gui_input`. It does shadow the ROW's
## own tooltip over this line, which is the accepted cost: line one is the row's identity and most of
## its height, and a hover on the accounts that spells the accounts out is coherent on its own terms.
func _build_work_row_accounts(model: Dictionary) -> MarginContainer:
    var margin := MarginContainer.new()
    margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    margin.mouse_filter = Control.MOUSE_FILTER_IGNORE
    margin.add_theme_constant_override("margin_left", HudWorkVocab.WORK_ROW_ACCOUNTS_INDENT)
    # The two parts share one row. Zero separation — the prefix carries the sentence separator in its
    # own text, exactly as the accounts carry the one before the floor clause, so the spacing of this
    # line is stated in ONE place (`WORK_INSPECT_SENTENCE_SEPARATOR`) rather than half in a string and
    # half in a container constant.
    var line_two := HBoxContainer.new()
    line_two.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    line_two.mouse_filter = Control.MOUSE_FILTER_IGNORE
    line_two.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_PRIORITY_SEPARATION)
    # **THE RANK LEADS THE LINE, AND ITS OWN `Label` IS WHY IT CAN** (§4.9 item 9b). A Normal row
    # mounts nothing at all here and its line two is byte-identical to what it printed before the mark
    # existed; a marked one gets a fixed-width part ahead of the accounts.
    #
    # **LEADING, NOT TRAILING, AND THAT IS THE WHOLE PLACEMENT ARGUMENT.** This line already elides on
    # the four-cash-crop worst case and the trim lands on its trailing floor clause — so a mark hung
    # at the end would be cut off exactly on the widest board, which is to say exactly when a famine
    # makes the rank matter. First on the line is the one position the elide cannot reach.
    #
    # **A SEPARATE NODE, NOT A PREFIX SPLICED INTO THE ACCOUNTS STRING.** The accounts carry
    # `OVERRUN_TRIM_ELLIPSIS`, an unconditional hover of their OWN whole text and `MOUSE_FILTER_PASS`;
    # splicing would put the rank inside the string that hover repeats and inside the text the trim
    # measures, so the accounts would start being cut to make room for a word that never needs cutting.
    # Fixed (no expand), so the accounts keep every pixel the prefix does not take.
    var priority := String(model.get("priority", HudWorkVocab.WORK_PRIORITY_NORMAL))
    var prefix_text := HudWorkVocab.work_row_priority_prefix(priority)
    if prefix_text != "":
        var prefix := Label.new()
        prefix.text = prefix_text
        prefix.add_theme_color_override("font_color", HudWorkVocab.work_priority_ink(priority))
        prefix.add_theme_font_size_override("font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
        # IGNORE, not PASS: the accounts beside it own this line's hover, and a second tooltip target
        # over the same line would swap the sentence out under a pointer that never left it.
        prefix.mouse_filter = Control.MOUSE_FILTER_IGNORE
        prefix.set_meta(HudWorkVocab.WORK_ROW_PRIORITY_META, priority)
        line_two.add_child(prefix)
    var accounts := Label.new()
    accounts.text = _work_row_summary_text(model)
    accounts.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    accounts.add_theme_color_override("font_color", HudStyle.INK_DIM)
    accounts.add_theme_font_size_override("font_size", HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
    HudWidgets.set_label_tooltip(accounts, accounts.text)
    accounts.mouse_filter = Control.MOUSE_FILTER_PASS
    accounts.set_meta(HudWorkVocab.WORK_ROW_ACCOUNTS_META, accounts.text)
    accounts.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    line_two.add_child(accounts)
    margin.add_child(line_two)
    return margin

func _work_row_stripe_color(model: Dictionary) -> Color:
    if bool(model.get("warn", false)) or String(model.get("note", "")) != "":
        return HudStyle.WARN
    if bool(model.get("pending", false)):
        return HudStyle.SIGNAL
    return Color(0.0, 0.0, 0.0, 0.0)

## The pager, shown only when one page cannot hold the filtered set.
func _build_work_pager(pages: int, start: int, shown_end: int, total: int) -> HBoxContainer:
    var pager := HBoxContainer.new()
    pager.custom_minimum_size = Vector2(0.0, HudWorkVocab.WORK_PAGER_HEIGHT)
    pager.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    var prev := Button.new()
    prev.text = HudWorkVocab.PAGER_PREV_GLYPH
    prev.focus_mode = Control.FOCUS_NONE
    prev.disabled = _work_page <= 0
    prev.tooltip_text = HudWorkVocab.PAGER_PREV_TOOLTIP
    HudStyle.apply_button(prev, "ghost")
    HudWidgets.compact(prev, HudWorkVocab.WORK_CHIP_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
    prev.pressed.connect(func() -> void: _step_work_page(-1))
    pager.add_child(prev)
    var label := Label.new()
    label.text = HudWorkVocab.PAGER_FORMAT % [_work_page + 1, pages]
    label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_CHIP_FONT_SIZE)
    label.add_theme_color_override("font_color", HudStyle.INK_DIM)
    pager.add_child(label)
    var next := Button.new()
    next.text = HudWorkVocab.PAGER_NEXT_GLYPH
    next.focus_mode = Control.FOCUS_NONE
    next.disabled = _work_page >= pages - 1
    next.tooltip_text = HudWorkVocab.PAGER_NEXT_TOOLTIP
    HudStyle.apply_button(next, "ghost")
    HudWidgets.compact(next, HudWorkVocab.WORK_CHIP_FONT_SIZE, HudWorkVocab.WORK_PAGER_PADDING_V)
    next.pressed.connect(func() -> void: _step_work_page(1))
    pager.add_child(next)
    var range_label := Label.new()
    range_label.text = HudWorkVocab.PAGER_RANGE_FORMAT % [start + 1, shown_end, total]
    range_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    range_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    range_label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_CHIP_FONT_SIZE)
    range_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
    pager.add_child(range_label)
    return pager

## The inspector strip — the row's SECOND and THIRD lines, relocated to one place at the bottom of the
## zone so the board itself stays one line per source. Spells the yield/policy/status out in words,
## carries the warning lines and the arrival strip, and offers the three inline actions.
## `Unassign` lives HERE (not as a hover `✕` on the row) — a destructive control beside the `−`
## stepper would be a mis-click hazard; this is the labelled version.
func _build_work_inspector(band: Dictionary, model: Dictionary) -> PanelContainer:
    var strip := PanelContainer.new()
    strip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var reserved := _work_inspector_height(model)
    strip.custom_minimum_size = Vector2(0.0, reserved)
    # The reserved number rides the strip so a harness can compare it against what the children
    # actually ask for. `custom_minimum_size` cannot answer that on its own — Godot folds it INTO
    # `get_combined_minimum_size`, so a strip that reserved too little still reports the reservation
    # back and every "does it fit" claim passes vacuously.
    strip.set_meta(HudWorkVocab.WORK_INSPECTOR_META, reserved)
    strip.add_theme_stylebox_override("panel", HudStyle.work_inspector_stylebox())
    var col := VBoxContainer.new()
    col.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
    strip.add_child(col)
    var head := HBoxContainer.new()
    head.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    # The mark is its own child rather than a prefix welded into the title's text: a texture cannot
    # live inside a `Label.text`, and splitting it is what lets the strip show the same art as the
    # row it belongs to. `WORK_ROW_SEPARATION` on `head` is what spaces them, as it did the string.
    head.add_child(HudWidgets.build_marker_icon(
        model.get("icon_texture") as Texture2D, String(model.get("icon", "")),
        HudWorkVocab.WORK_ROW_ICON_WIDTH, HudWorkVocab.WORK_ROW_FONT_SIZE))
    var title := Label.new()
    title.text = String(model.get("label", ""))
    title.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    title.clip_text = true
    title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(title)
    var close := Button.new()
    close.text = HudWorkVocab.INSPECTOR_CLOSE_GLYPH
    close.focus_mode = Control.FOCUS_NONE
    close.tooltip_text = HudWorkVocab.INSPECTOR_CLOSE_TOOLTIP
    HudStyle.apply_button(close, "ghost")
    HudWidgets.compact(close, HudWorkVocab.WORK_ROW_FONT_SIZE, HudWorkVocab.INSPECTOR_CLOSE_PADDING_V)
    close.pressed.connect(func() -> void: _toggle_work_inspector(String(model.get("key", ""))))
    head.add_child(close)
    col.add_child(head)
    # **RETIRED — THE ONE-SENTENCE READOUT.** It read *accounts · 50% left standing · ● Working ·
    # 3 assigned*, and the ROW says all four now: the accounts and the floor on its own line two
    # (`_work_row_summary_text`), the crew on its stepper, the pending state on its severity stripe and
    # its name's amber ink. Deleting the whole `Label` is what freed the 20px the two-line row costs
    # this zone — dropping only the accounts clause freed NOTHING, a line surviving three of its four
    # clauses — and it is why the floor travelled with them rather than staying behind.
    #
    # **EVERY SURVIVING LINE STILL ELIDES, and that is a WIDTH decision the zone forces.** The zone's
    # box is reserved (`PANEL_WIDTH` less chrome, 356px on a side dock) and its content is anchored
    # full-rect into a host that clips — so a line reporting its whole text as a minimum width clamps
    # the entire tab's column up to that width, and the POOLS readout, the Builders card's `+`, the
    # BUILD QUEUE head's kit name and every board row's stepper are sliced off the right edge by a
    # line none of them can see. `band_panel_work_inspector_width` is the frame, and
    # `_assert_zone_content_width_fits` the claim.
    if bool(model.get("warn", false)):
        col.add_child(HudWidgets.build_status_part(HudWorkVocab.WORK_INSPECT_OVERDRAW_LINE, HudStyle.WARN, true))
    if String(model.get("note", "")) != "":
        # **THE INK COMES OFF THE MODEL** (§2.7) — DANGER where the missing thing is a good, WARN where
        # it is hands. One table (`HudWorkVocab.note_color`), read here and at the drawer's twin, so
        # the two surfaces cannot colour one note two ways.
        col.add_child(HudWidgets.build_status_part(String(model.get("note", "")),
            HudWorkVocab.note_color(String(model.get("note_severity",
                HudWorkVocab.NOTE_SEVERITY_WARN))), true))
    if String(model.get("muted_note", "")) != "":
        col.add_child(HudWidgets.build_status_part(String(model.get("muted_note", "")), HudStyle.INK_FAINT, true))
    var schedule: PackedFloat32Array = model.get("schedule", PackedFloat32Array())
    if ArrivalStrip.has_gap(schedule):
        var arrivals := ArrivalStrip.new()
        arrivals.set_schedule(schedule, _band_labor.current_turn())
        col.add_child(arrivals)
    var links := HBoxContainer.new()
    links.add_theme_constant_override("separation", HudWorkVocab.COMPOSITION_KEY_SEPARATION)
    links.add_child(HudWidgets.build_inline_link(HudWorkVocab.WORK_INSPECT_JUMP, HudStyle.INK, func() -> void:
        _focus_work_source(model)))
    links.add_child(HudWidgets.build_inline_link(HudWorkVocab.WORK_INSPECT_POLICY, HudStyle.INK, func() -> void:
        _toggle_work_picker(HudWorkVocab.WORK_PICKER_FLOOR)))
    # **THE RANK, BETWEEN THE FLOOR AND THE WITHDRAWAL** (§4.9 item 9b). Placed beside `Change policy`
    # because the two are the same kind of control — a standing property of this row, picked from three
    # buttons — and ahead of `Unassign`, which is the destructive one and stays last.
    links.add_child(HudWidgets.build_inline_link(HudWorkVocab.WORK_INSPECT_PRIORITY, HudStyle.INK, func() -> void:
        _toggle_work_picker(HudWorkVocab.WORK_PICKER_PRIORITY)))
    links.add_child(HudWidgets.build_inline_link(HudWorkVocab.WORK_INSPECT_UNASSIGN, HudStyle.DANGER, func() -> void:
        _work_open_key = ""
        _work_picker_open = HudWorkVocab.WORK_PICKER_NONE
        _emit_work_assign(band, model, 0)))
    col.add_child(links)
    if _work_picker_open == HudWorkVocab.WORK_PICKER_PRIORITY:
        # THE THREE LEVELS AND THE ONE SENTENCE THAT SAYS WHAT THEY DO. Same shape as the floor picker
        # below, deliberately: they are one link apart in one strip, and a second layout for three
        # buttons in a row would be a new thing to learn for no new meaning.
        col.add_child(HudWidgets.build_work_priority_picker(func(level: String) -> void:
            _commit_work_priority(band, model, level),
            String(model.get("priority", HudWorkVocab.WORK_PRIORITY_NORMAL))))
    if _work_picker_open == HudWorkVocab.WORK_PICKER_FLOOR:
        # THE THREE FLOOR PRESETS, and nothing else to say about them. **DELIBERATELY NO SLIDER HERE**:
        # this zone is a fixed-width box the compose sheet is not, and re-pointing a standing crew from
        # the board is a coarse decision — the fine dial lives on the source's own compose sheet, where
        # the forecast that would justify a 5% move renders beside it.
        col.add_child(HudWidgets.build_floor_picker(func(floor: float) -> void:
            _commit_work_floor(band, model, floor),
            float(model.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR)), {}))
    return strip

func _commit_work_floor(band: Dictionary, model: Dictionary, floor: float) -> void:
    _work_picker_open = HudWorkVocab.WORK_PICKER_NONE
    _emit_work_assign(band, model, int(model.get("workers", 0)), floor)

## Open `which`, or close it if it is already the one showing — and close whatever else was open in
## the same assignment. **The mutual exclusion is HERE and only here**: both links route through this,
## so neither can grow a path that leaves the other picker standing.
func _toggle_work_picker(which: StringName) -> void:
    _work_picker_open = HudWorkVocab.WORK_PICKER_NONE if _work_picker_open == which else which
    _repage_work_zone()

## **THE RANK COMMAND** — `work_priority <faction> <band> <source…> high|normal|low`
## (`docs/plan_standing_upkeep.md` §4.9 item 9b).
##
## ⛔ **NO OPTIMISTIC WRITE AND THEREFORE NO ROLLBACK HANDLE**, which is `build_order`'s rule one field
## over: `LaborAssignment.priority` is captured live off the allocation the command mutates and the
## server re-captures after every command, so the new mark arrives on THIS command's own recapture. A
## client-side pending copy would be a second statement of one value — the drift §4.9 forbids — and a
## send that does not go leaves nothing behind to undo.
##
## The picker CLOSES on the pick, exactly as the floor picker does: the strip has said its piece, and
## a picker left open over a value that has not landed yet invites a second press at the same button.
func _commit_work_priority(band: Dictionary, model: Dictionary, level: String) -> void:
    _work_picker_open = HudWorkVocab.WORK_PICKER_NONE
    if model.is_empty():
        return
    emit_signal("work_priority_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
        "x": int(model.get("x", -1)),
        "y": int(model.get("y", -1)),
        "herd_id": String(model.get("herd_id", "")),
        "level": HudWorkVocab.work_priority_of(level),
    })
    _repage_work_zone()

## The height the open inspector reserves — BOTH what `_work_board_capacity` subtracts from the board
## and what the strip actually draws at, so the page can never overflow its zone (the work-board rule).
##
## **IT READS THE MODEL, and for a long time it did not — the parameter was `_model`.** It forked on
## one panel-state value (`_work_picker_open`) and answered one of two totals, while
## `_build_work_inspector` draws FOUR children conditionally on the model: the overdraw line, the
## slipping `note`, the `muted_note` and the `ArrivalStrip`, each with its own
## `ZONE_BLOCK_SEPARATION` gap. Each of them could draw with nothing reserved for it — and the zone
## `clip_contents`, so the overflow was silent. Reported from play as the Band/City panel running past
## the bottom of the screen *only when something is selected in the work list*.
##
## **THE TESTS BELOW ARE THE BUILDER'S OWN, verbatim.** That is the whole of what makes the reserved
## height and the drawn height one answer; a paraphrase (`has("note")` for `note != ""`, say) reserves
## for a child that does not draw or misses one that does, and both fail silently.
func _work_inspector_height(model: Dictionary) -> float:
    var height := HudWorkVocab.WORK_INSPECTOR_HEIGHT
    if bool(model.get("warn", false)):
        height += HudWorkVocab.WORK_INSPECTOR_NOTE_HEIGHT
    if String(model.get("note", "")) != "":
        height += HudWorkVocab.WORK_INSPECTOR_NOTE_HEIGHT
    if String(model.get("muted_note", "")) != "":
        height += HudWorkVocab.WORK_INSPECTOR_NOTE_HEIGHT
    var schedule: PackedFloat32Array = model.get("schedule", PackedFloat32Array())
    if ArrivalStrip.has_gap(schedule):
        height += HudWorkVocab.WORK_INSPECTOR_ARRIVALS_HEIGHT
    # ONE open height for the picker: the standing-investment line the taller variant reserved room
    # for is gone with the axis split (see `_build_work_inspector`), so every open picker is the same
    # four rungs. It is panel state rather than model state, which is why it is the one term here that
    # does not read the dict.
    # ONE open height per picker, and AT MOST ONE of them is open (`_work_picker_open`). They are
    # panel state rather than model state, which is why they are the terms here that do not read the
    # dict; the priority one is the taller of the two by its hint line, and is what
    # `WORK_INSPECTOR_CEILING_HEIGHT` therefore counts.
    if _work_picker_open == HudWorkVocab.WORK_PICKER_FLOOR:
        height += HudWorkVocab.WORK_INSPECTOR_POLICY_PICKER_HEIGHT
    elif _work_picker_open == HudWorkVocab.WORK_PICKER_PRIORITY:
        height += HudWorkVocab.WORK_INSPECTOR_PRIORITY_PICKER_HEIGHT
    return height

## **LINE TWO'S WHOLE STRING — every account this source pays, then the floor the player set it to.**
## The accounts go through `SourceForecast.yield_components`, which already carries the
## render-only-when-non-zero rule, the food-leads order, the fodder WORD (fodder has no glyph, and
## borrowing another account's mark would say the wrong thing) and the per-material terms; a source
## with no confirmed yield at all states no account clause rather than a zero.
##
## **THE FLOOR IS HERE BECAUSE THE INSPECTOR'S SENTENCE IS GONE, AND IT IS WHAT PAID FOR THE ROW'S
## SECOND LINE.** That sentence read *accounts · 50% left standing · ● Working · 3 assigned* on ONE
## `Label`, so dropping the accounts clause alone freed nothing — a line survives three of its four
## clauses. The floor was the only clause the row could not otherwise state (the stepper states the
## crew, the stripe and the name's ink state pending), so it moved HERE and the whole sentence went,
## which is the 20px the two-line row costs the zone. `HudComposeVocab.FLOOR_VALUE_FORMAT` is the
## phrasing the floor presets' tooltips and the chart's caption use, so one number is never worded two
## ways.
##
## **THE JOINER IS THE SENTENCE'S OWN** (`WORK_INSPECT_SENTENCE_SEPARATOR`), because what it joins is
## a CLAUSE to a list rather than one account to another — the two consts hold the same glyph today
## and mean different things, and the accounts' own separator is `SourceForecast.COMPONENT_SEPARATOR`
## one layer down.
##
## **IT REPLACED A ONE-SLOT FALL-THROUGH — food → fodder → materials, picking exactly ONE** — and the
## fall-through was a WIDTH compromise rather than a reading. A board row's accounts had a fixed 46px
## slot beside the marks and the stepper, so a hay meadow paying meat AND feed had to choose, and a
## four-cash-crop patch could name one material and count the other three (`+0.24 fibre +3`). Line two
## is the row's whole width, so there is no choice left to make and none is made.
func _work_row_summary_text(model: Dictionary) -> String:
    var parts: Array[String] = []
    if bool(model.get("has_yield", false)):
        # Both products, each only when non-zero (issue #449): a hay Field leads with its fodder rate
        # instead of asserting "+0.00 /turn".
        parts.append(SourceForecast.yield_components(
            float(model.get("rate", 0.0)), float(model.get("fodder_rate", 0.0)),
            SourceForecast.YIELD_ACCOUNT_FOOD, model.get("material_rows", [])))
    parts.append(HudComposeVocab.FLOOR_VALUE_FORMAT % SourceForecast.floor_percent(
        float(model.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))))
    return HudWorkVocab.WORK_INSPECT_SENTENCE_SEPARATOR.join(parts)

# ---- work-zone models + state ----------------------------------------------

## One dict per worked source, carrying everything the row, the chips and the inspector need — built
## ONCE per render off `_band_labor.effective_worker_map` (confirmed + optimistic pending), so the board, the
## chip counts and the totals can never disagree.
func _work_source_models(band: Dictionary, idle: int) -> Array:
    var models: Array = []
    var merged := _band_labor.effective_worker_map(band)
    # **THE BAND'S BUILDERS POOL, RESOLVED ONCE FOR THE WHOLE BOARD** (`docs/plan_standing_upkeep.md`
    # §2.5): a verb declares and names no hands, so every row's build verdict is asked against the
    # same band-level pool the queue block's own head states. Pending-aware, the rule every readout on
    # this panel follows — a player who has just staffed the role must not read a ⚠ they have fixed.
    var builders := int(_band_labor.effective_role_workers(
        band, HudConst.LABOR_KIND_BUILDERS).get("workers", 0))
    for key in merged:
        var m: Dictionary = merged[key]
        var kind := String(m.get("kind", "")).strip_edges().to_lower()
        var workers := int(m.get("workers", 0))
        # **A ROW IS ADMITTED ON ITS TAKE CREW AGAIN** (`docs/plan_standing_upkeep.md` §2.5). It also
        # admitted on the source's own build crew for one slice — a source with hands raising its
        # meter and nobody gathering had to stay on the board — and that crew no longer exists per
        # source: the builders are one band-level pool on the head of the band's QUEUE, which is a
        # list of its own and is slice 7's to render.
        var pending := bool(m.get("pending", false))
        if not (kind == SourceForecast.LABOR_KIND_FORAGE or kind == SourceForecast.LABOR_KIND_HUNT):
            continue
        if workers <= 0 and not pending:
            continue
        var yld := SourceForecast.source_yield_readout(m, kind)
        var x := int(m.get("x", -1))
        var y := int(m.get("y", -1))
        var herd_id := String(m.get("herd_id", ""))
        var floor := SourceForecast.clamp_floor(
            float(m.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR)))
        # THE SECOND AXIS (issue #442) — what this crew is BUILDING here, "" for nothing. It is what
        # the rung marks and the herding-crew floor key on; the escapement floor is purely how hard
        # this crew pulls.
        var improvement := String(m.get("improvement", "")).strip_edges().to_lower()
        var icon := ""
        # The row's bundled ART, resolved BESIDE the emoji rather than instead of it (issue #439):
        # the emoji stays as the fallback, it is not replaced. BOTH webs fill it, and that is
        # deliberate — hunt and forage rows share ONE list and ONE icon column, so spriting only the
        # hunt half would leave a board that is half art and half emoji, a new inconsistency
        # introduced by the fix. `null` where the client has no art for this source, which is the
        # case `HudWidgets.build_marker_icon` renders the glyph for.
        var icon_texture: Texture2D = null
        var label := ""
        var cap := {}
        var live_herd := {}
        var patch := {}
        if kind == SourceForecast.LABOR_KIND_FORAGE:
            # The board draws the glyph in its OWN fixed column, so it takes the RAW icon — not
            # `HudFormat.source_icon_prefix`, which welds it to the label with a trailing space for the
            # single-label row this replaced.
            icon = _band_labor.food_module_icon(x, y)
            icon_texture = _band_labor.food_module_sprite(x, y)
            # Held in a local because the RUNG mark reads it too — `forage_patch_lookup` spells its keys
            # BARE (`is_cultivated` / `is_field`), unlike the `patch_`-prefixed `tile_info` cross-ref.
            patch = _band_labor.forage_patch_lookup().get(Vector2i(x, y), {})
            # THE ROW'S VERB FOLLOWS THE STANDING RUNG, through the same `HudFormat.plant_crew_label`
            # the compose sheet's noun does: a crew on a Tended Patch or a Field is TENDING, not
            # foraging, so `Forage (27, 26)` would name an activity the sim does not run there. The
            # rung MARK beside it answers a different question (what the source IS) and both stay.
            label = String(HudWorkVocab.WORK_ROW_PLANT_FORMATS.get(
                HudFormat.plant_crew_label(patch, HudComposeVocab.BARE_FORECAST_PREFIX), "")) % [x, y]
            # **THE VERB NO LONGER MOVES THIS ROW'S CAP** (`docs/plan_standing_upkeep.md` §2.2). It
            # used to twice over — the take was dipped while a build ran, and the rung's own
            # `crew_needed` floored the count back up — because one crew did both jobs. The build has
            # its own crew now, so the take is the plain one and the count is the plain quotient.
            cap = SourceForecast.source_worker_cap_state(SourceForecast.forecast_inputs(
                patch, SourceForecast.SOURCE_KIND_FORAGE,
                HudComposeVocab.BARE_FORECAST_PREFIX, floor), workers, idle)
        else:
            var herd_label := _herd_label_for_id(herd_id)
            icon = FoodIcons.for_herd(herd_label)
            icon_texture = FaunaSprites.for_herd(herd_label)
            label = HudWorkVocab.WORK_ROW_HUNT_FORMAT % herd_label
            # Herds MIGRATE, so the cap reads the herd's LIVE dict from `_band_labor.world_herds()` rather than the
            # assignment's launch-time target.
            live_herd = _band_labor.find_world_herd(herd_id)
            # The verb is not a term here either (see the forage branch): a crew building a pen is
            # its own allocation, so the hunters' take is the plain one.
            var hunt_forecast := SourceForecast.forecast_inputs(
                live_herd, SourceForecast.SOURCE_KIND_HERD,
                HudComposeVocab.BARE_FORECAST_PREFIX, floor)
            # **NO KEEPER FLOOR ON THIS ROW'S CEILING EITHER** (`docs/plan_standing_upkeep.md` §2.2)
            # — the compose twin dropped the same term. The keepers a managed herd demands are the
            # MAINTAIN allocation, answered by the compose sheet's keeping row and by `maintain`,
            # so raising the TAKE row's `+` to `herdersNeeded` staffed one crew against another
            # crew's demand.
            #
            # **AND THE CEILING IS THE SIM'S, NOT A QUOTIENT.** This row is priced with no crew-take
            # reply in hand, so `max_useful_workers` used to fall through to the closed form —
            # `take_workers`, which divides by a reach carrying no attack, no defense and no
            # durability — and the board quoted a ceiling the compose sheet's own curve disagreed
            # with on any fight-bound quarry. The wire publishes the plateau of that same curve on
            # every assigned hunt row; carrying it onto the forecast is all it takes for both cap
            # twins to read it. **The forage branch above deliberately does not**: its `0` is a
            # structural *does not apply*, never *no crew is useful here*.
            hunt_forecast = SourceForecast.with_published_useful_crew(hunt_forecast, m)
            cap = SourceForecast.source_worker_cap_state(hunt_forecast, workers, idle)
        var note := String(yld.get("note", ""))
        # **THE NOTE'S SEVERITY IS MODEL STATE, NOT A RENDER-SITE CONSTANT** (§2.7). It was a
        # hard-coded `HudStyle.WARN` at both this board's inspector and the drawer's twin, which is
        # right for a staffing shortfall and wrong for a missing GOOD — a dead kit costs hands, a
        # missing material stops the work outright, and the two must not read alike. The producer that
        # knows which shortfall it is says so here; neither renderer sniffs the sentence.
        var note_severity := HudWorkVocab.NOTE_SEVERITY_WARN
        var rung := _work_source_rung(kind, patch, live_herd)
        # THE RUNG ON OFFER — a third axis, orthogonal to both `marks` (the verb in flight) and
        # `rung_glyph` (the rung the source STANDS on). Same `RungGates` answer the map's badge and the
        # compose sheet's gates use, so the three surfaces cannot disagree about what is climbable.
        var rung_source: Dictionary = patch if kind == SourceForecast.LABOR_KIND_FORAGE else live_herd
        # A rung UNDER WAY takes the slot from a rung on OFFER — they are one axis in two states, and
        # mutually exclusive by construction (`next_rung_ready` excludes the verb in flight).
        #
        var building := RungGates.rung_in_progress(kind, rung_source, improvement)
        # **AN ERODED RUNG NOBODY HAS ORDERED IS AN OFFER, NOT A BUILD UNDER WAY** — the client half of
        # the 99% repair. `build_verb` answers for any meter between zero and its cost, so a Tended
        # Patch that has slipped to 99% reads as *building* here while the offer test filtered it out
        # as *built*: the row said both at once and offered nothing, and the only way to order the
        # repair was to type the command. Dropping `building` on that one case restores the whole
        # existing offer path — the `⌃` face, the button, the price tooltip — with no new glyph and no
        # new slot.
        if not building.is_empty() and _rung_is_an_unordered_repair(rung_source, improvement,
                String(building.get("policy", ""))):
            building = {}
        # **THE RUNG THE ENTRY IS HEADED FOR, HELD BEFORE THE READOUTS MOVE TO THE LEG.** A queue
        # entry's PRICE is the whole climb's, so the two cost fields below stay on the declared rung;
        # what follows the leg is the percentage and its verb. Keeping both is what stops one fix
        # quietly understating a two-leg job's bill.
        var destination_rung := String(building.get("policy", ""))
        # **AND THE READOUTS SHOW THE LEG IN FLIGHT, NOT THE DESTINATION**
        # (`docs/plan_standing_upkeep.md` §2.8). A `sow` ordered on untended ground is one entry and
        # two legs, and `rung_in_progress` names the rung the player DECLARED — whose meter is honestly
        # 0% while the crew is still clearing the ground beneath it. Reported from play: the queue row
        # and this row's rung chip both read `0%` for thirty-nine turns beside a tile card reading
        # `18%`. BOTH Work-tab readouts take the leg from this ONE re-pointing — the chip below, and
        # the queue row's percent and verb, which read `building_glyph` / `building_policy` /
        # `building_progress` off this model.
        building = RungGates.leg_in_progress(rung_source, building)
        var ready := {} if not building.is_empty() \
            else RungGates.next_rung_ready(kind, rung_source, improvement, _player_knowledge())
        # The row's HARVEST mark is the floor's ZONE glyph — where this crew's floor sits relative to
        # the food peak. A continuous number cannot wear one glyph per value, and the zone is the whole
        # of what one mark can honestly say about it; the exact percent is in the row tooltip.
        # **IS THAT RUNG UNSTAFFED, OR LOSING GROUND? — THE MAP BADGE'S OWN TWO QUESTIONS, ASKED OF
        # THE SAME TWO SEAMS** (`docs/plan_standing_upkeep.md` §4.6a). `BandOverlayRenderer` forks its
        # source plate on exactly this pair and drops the percent; this row printed a confident
        # `▦45%` whatever the staffing, so one screen carried two verdicts and the wrong one was the
        # one with a number on it.
        #
        # **IT IS NOT RE-DERIVED HERE — not from the crew count, not from the percentage sitting
        # right there.** `SourceForecast.build_is_stalled` is the ONE producer of this verdict and the
        # map badge calls the same function on the same two inputs, so the two surfaces cannot come
        # apart again. A build merely PARKED with its keeping covered answers `false` and keeps its
        # number, because that number is honest.
        var build_stalled := not building.is_empty() and SourceForecast.build_is_stalled(
            rung_source, float(building.get("progress", 0.0)), builders)
        # …and the hover says the same thing the face does. A tooltip still quoting `45% done` beside
        # a `⚠` would restate the exact number the mark exists to withdraw.
        var building_tooltip := ""
        if not building.is_empty():
            building_tooltip = HudWorkVocab.WORK_ROW_BUILDING_UNSTAFFED_TOOLTIP_FORMAT \
                    % HudFormat.policy_face(String(building.get("policy", ""))) if build_stalled \
                else HudWorkVocab.WORK_ROW_BUILDING_TOOLTIP_FORMAT % [
                    HudFormat.policy_face(String(building.get("policy", ""))),
                    HudFormat.progress_percent(float(building.get("progress", 0.0)))]
        # **THE BUILD SLOT'S HOVER, RESOLVED ONCE FOR BOTH THE MARK AND THE ROW** (§4.7a ①, §2.8).
        # The sentence says what the press does — it OPENS A TRACK now rather than queueing outright,
        # so a hover still promising a one-click declaration would describe a control that no longer
        # exists. The line beneath is the NEXT rung's PRICE — its `<rung>WorkCost` pile and its
        # `<rung>UpkeepDemand` rate — which left the compose sheet on Ray's call that a cost belongs on
        # the surface that queues and funds jobs. **No turn count**: the BUILD QUEUE row's date is the
        # sim's chained answer, and a second estimate here would be two producers for one number.
        #
        # **A RUNNING BUILD'S SLOT OPENS THE SAME TRACK AND SAYS SO INSTEAD.** It quotes no price: the
        # price it would state is the rung already being raised, which the face beside it is metering.
        var ready_tooltip := ""
        if not building.is_empty() and not build_stalled:
            ready_tooltip = HudWorkVocab.WORK_ROW_BUILDING_TRACK_TOOLTIP
        if not ready.is_empty():
            var ready_rung := String(ready.get("policy", ""))
            ready_tooltip = HudFormat.join_tooltip_lines([
                HudWorkVocab.WORK_ROW_READY_TRACK_TOOLTIP,
                DetailFormat.build_price_clause(
                    SourceForecast.build_work_cost(rung_source,
                        HudComposeVocab.BARE_FORECAST_PREFIX, ready_rung),
                    SourceForecast.BUILD_TURNS_NO_ESTIMATE,
                    SourceForecast.build_upkeep_demand(rung_source,
                        HudComposeVocab.BARE_FORECAST_PREFIX, ready_rung),
                    SourceForecast.source_kind_for_labor(kind))])
        var marks := FoodIcons.for_floor_zone(SourceForecast.floor_zone(floor))
        if bool(yld.get("warn", false)):
            marks += " " + HudComposeVocab.OVERHUNT_FLAG
        # **THE ONE AT-RISK MARK, AND IT USED TO BE TWO** (`docs/plan_standing_upkeep.md` §4.6a).
        # `SourceForecast.is_under_kept` is the single test on both webs and on both sides of
        # completion: the band's keeping pool owes an at-risk meter at any fullness, so a half-built
        # rung and a held one are short for the same reason and have the same remedy. It was split on
        # `build_is_in_flight` into a keeper warning and a builder warning — a distinction that existed
        # only while an unbuilt rung was billed to its BUILD crew, and which had the board telling a
        # player to staff builders for a bill the keeping pool owes.
        #
        # **IT MEASURES THIS SOURCE'S SHARE OF THE POOL AGAINST ITS KEEPING DEMAND** (§2.5) — the herd
        # drawer's `Keepers` row and the source card's rung mark call the same test, so no two surfaces
        # can disagree. It counted the per-source `maintain` crew until maintenance left the tile; that
        # crew no longer exists, so the count went to `0` on every managed source and the ⚠ would have
        # been permanently up.
        #
        # **THE NOTE NAMES THE BAND ROLE THAT MOVES IT, NOT THIS ROW'S `+`** — the stepper beside it
        # moves the TAKE crew, so the remedy is the WORKFORCE zone's Husbandry or Agriculture card.
        # The two are the same sentence about different webs, which is why the pair is keyed on `kind`
        # rather than spelled twice.
        var under_kept := SourceForecast.is_under_kept(
            rung_source, HudComposeVocab.BARE_FORECAST_PREFIX)
        # **AND THE COUNTDOWN RIDES THIS ROW'S HOVER — ONLY THIS ONE.** The source's own card states
        # the same first sentence and no figure at all: this board is where staffing is decided this
        # turn, so *how long you have* is actionable here, while a card is where you look at the
        # ground and a number you cannot act on from it is noise. **One producer with a flag**
        # (`HudWorkVocab.under_kept_tooltip`), never two, or the two surfaces word one hazard
        # differently. The rung it names is `at_risk_rung`'s — the newest meter carrying work, the
        # same routing the built row's own mark uses — because a source publishes ONE countdown and
        # can carry two meters.
        #
        # **`at_risk` IS READ BEFORE `grace`, and the two zeros are opposite news** (`upkeep_state`'s
        # own rule): `0` grace on an at-risk source means the penalty is biting THIS turn, while a
        # source that is not at risk states a `0` meaning nothing is at stake — so the flag decides,
        # and an unflagged source counts down from now.
        #
        # **AND WHEN THE MISSING THING IS A GOOD, THE NOTE NAMES IT INSTEAD** (§2.7). The row's own
        # published pair — what its SOURCE was billed in materials and what the band's store paid —
        # is what says so, and it supersedes the staffing sentence because it names a remedy the
        # keeping stepper cannot reach: twelve keepers do not mend a fence with no hurdles. The
        # countdown is unchanged, since either shortfall drives the same decay through the same
        # grace.
        var material_note := HudWorkVocab.material_short_note(kind,
            SourceForecast.material_payoff_rows(m.get(
                SourceForecast.ASSIGNMENT_MATERIAL_UPKEEP_DEMAND_KEY, [])),
            SourceForecast.material_payoff_rows(m.get(
                SourceForecast.ASSIGNMENT_MATERIAL_UPKEEP_SUPPLIED_KEY, [])))
        # ⛔ **A GOOD-SHORT ROW IS UNDER-KEPT WHATEVER ITS WORK ACCOUNT SAYS, and reading only the work
        # gate would have hidden the whole arm.** The two currencies are billed and judged SEPARATELY —
        # that separation is the wire's own rule, so a full store cannot paper over missing hands —
        # which means a pen whose keepers are paid in full and whose fence is not has
        # `upkeep_shortfall == 0` and is losing its rung all the same. One shortfall of EITHER kind
        # trips the same grace and drives the same decay, so it earns the same ⚠, the same slot and
        # the same countdown.
        var at_risk := under_kept or material_note != ""
        var under_kept_hint := ""
        if at_risk:
            var source_kind := SourceForecast.source_kind_for_labor(kind)
            var upkeep := SourceForecast.upkeep_state(rung_source,
                HudComposeVocab.BARE_FORECAST_PREFIX)
            under_kept_hint = HudWorkVocab.under_kept_tooltip(kind,
                DetailFormat.rung_badge_word(SourceForecast.at_risk_rung(
                    rung_source, HudComposeVocab.BARE_FORECAST_PREFIX, source_kind)),
                int(upkeep["grace"]) if bool(upkeep.get("at_risk", false)) else 0, material_note)
        if at_risk:
            if not marks.contains(HudComposeVocab.OVERHUNT_FLAG):
                marks += " " + HudComposeVocab.OVERHUNT_FLAG
            # **IT TAKES THE SLOT, where it used to yield to whatever was already in it.** The two
            # notes could not co-occur while containment came off the hunting crew — a herd could not
            # be short of hunters and overstaffed with them at once — and with the crews split they
            # can: an overstaffed TAKE crew on a source nobody keeps is an ordinary state. They are not
            # equal in weight, so the slot is not first-come: the overstaff note says some hands bring
            # nothing home, and this one says the ground or the flock is being lost.
            note = HudWorkVocab.under_kept_note(kind, material_note)
            note_severity = HudWorkVocab.under_kept_note_severity(material_note)
        models.append({
            "key": String(key), "kind": kind, "icon": icon, "icon_texture": icon_texture,
            "label": label,
            "rate": float(yld.get("rate", 0.0)),
            # The row's FODDER component (issue #449), 0 on every hunt row and on any patch that
            # grows no feed.
            # Carried so the row's one-slot rate, the header total and the inspector sentence all state
            # a hay Field's whole product instead of reading it as a dead tile.
            "fodder_rate": float(yld.get("fodder_rate", 0.0)),
            # …and its MATERIAL component, a VECTOR (arc #527 follow-up). Empty on every row that
            # pays no material; an inedible quarry's whole product is here, which is what stops a
            # hunted wolf pack reading `+0.00 /turn` on the board it is commanded from.
            "material_rows": yld.get("material_rows", []),
            "has_yield": bool(m.get("has_yield", false)),
            "workers": workers, "pending": pending, "warn": bool(yld.get("warn", false)),
            # **ONE FLAG, WHERE THERE WERE TWO** (`docs/plan_standing_upkeep.md` §4.6a). It was
            # `under_herded` beside `unbuilt` — a keeper warning and a builder warning, split on
            # whether the meter was full — and one keeping pool owes both now, so the second key was a
            # distinction with nothing under it.
            #
            # ⛔ **AND THE SURVIVOR IS NO LONGER CALLED `under_herded`, BECAUSE IT STOPPED MEANING
            # THAT.** It is short in EITHER CURRENCY (§2.7): a pen whose keepers are paid in full and
            # whose fence is not is losing its rung exactly as fast, and calling that state
            # *under-herded* named a headcount nothing here reads. What it holds is **this source is
            # losing its rung, for any reason** — the source card's own `At risk:` vocabulary — so it
            # is spelled the way `SourceForecast.upkeep_state`'s gate is, and the row's stripe and its
            # ⚠ answer to it rather than to the work account alone.
            "at_risk": at_risk,
            "note": note, "note_severity": note_severity,
            "muted_note": String(yld.get("muted_note", "")), "marks": marks,
            # The source's STANDING RUNG — orthogonal to `marks`, which carries the verb in flight.
            "rung_glyph": String(rung.get("glyph", "")), "rung_tooltip": String(rung.get("tooltip", "")),
            # The rung this source could climb NOW ("" for none) — see `ready` above.
            "ready_policy": String(ready.get("policy", "")), "ready_glyph": String(ready.get("glyph", "")),
            # …and the hover BOTH the mark and the row state, resolved once above.
            "ready_tooltip": ready_tooltip,
            # The LEG the crew is on right now, and how far into it. Both Work-tab readouts spend
            # this trio — the row's rung chip and the queue row's percent-and-verb — so they cannot
            # name two different rungs. See `RungGates.leg_in_progress` for why it is the leg rather
            # than the entry's destination.
            "building_policy": String(building.get("policy", "")),
            "building_glyph": String(building.get("glyph", "")),
            "building_progress": float(building.get("progress", 0.0)),
            # **A RING QUOTES ITS OWN METER, BECAUSE THE LADDER HAS NOTHING TO QUOTE FOR IT.** An
            # `extend_pen` entry widens the pen rung its herd already stands on — there is no leg to
            # climb, only more of the one the source is already on — so `building` above is empty, the
            # trio beside this reads `0%`, and it would read `0%` for the ring's whole life. The ring's
            # real meter is the herd's own `pen_extend_progress` / `pen_extend_cost` pair, in WORK
            # UNITS, and `SourceForecast.pen_extend_fraction` is the same single division the herd
            # drawer's "Fencing N%" badge comes through — so the badge and the queue row cannot quote
            # one ring two ways. `PEN_EXTEND_EMPTY_METER` on every entry that is not a ring: the field
            # is meaningful only under the ring branch that `improvement` selects.
            "build_ring_progress": SourceForecast.pen_extend_fraction(live_herd) \
                if improvement == SourceForecast.BUILD_JOB_EXTEND_PEN \
                else SourceForecast.PEN_EXTEND_EMPTY_METER,
            # **WHAT THAT RUNG COSTS, BOTH HALVES — the one-off pile and the standing rate.** They ride
            # the model because this is where the raw wire source is in hand, and TWO surfaces spend
            # them: the queue row's tooltip states the job's full price, and the POOLS block reads the
            # rate to mark a keeping pool a queued job is about to need. Composed off the DESTINATION
            # rung so a pending declaration prices identically to a placed entry — which is the whole
            # of what makes the warning arrive at DECLARE time rather than a turn later — and
            # deliberately NOT off the leg in flight, whose cheaper rung would understate a two-leg
            # job's bill on the one surface that quotes it.
            "build_work_cost": SourceForecast.build_work_cost(rung_source,
                HudComposeVocab.BARE_FORECAST_PREFIX, destination_rung),
            "build_upkeep_demand": SourceForecast.build_upkeep_demand(rung_source,
                HudComposeVocab.BARE_FORECAST_PREFIX, destination_rung),
            # …and the BARE per-worker work rate that source publishes, which is what the POOLS block
            # prices a keeping pool's own hands at when the queue is the only thing asking for them.
            # Read here because this is where the raw wire source is in hand, exactly as the two
            # prices above are.
            "build_work_per_worker_turn": SourceForecast.build_work_per_worker_turn(rung_source,
                HudComposeVocab.BARE_FORECAST_PREFIX),
            # **AND WHAT THE SIM IS ALREADY BILLING THIS SOURCE**, which is what stops the POOLS block
            # counting one job twice: a queue entry keeps its position while its meter climbs, so the
            # moment the first work is banked the source's own `upkeepDemand` states the very rate the
            # queued entry is still quoting.
            "live_upkeep_demand": float(SourceForecast.upkeep_state(rung_source,
                HudComposeVocab.BARE_FORECAST_PREFIX).get("demand", SourceForecast.NO_UPKEEP_DEMAND)),
            # …and whether that rung is STALLED — unstaffed, or losing ground. Derived ONCE, above,
            # off the same two `SourceForecast` seams the map badge forks on, and carried as a flag
            # so the row builder cannot ask the question a second way (see `build_stalled`).
            "build_stalled": build_stalled,
            "floor": floor, "improvement": improvement, "x": x, "y": y, "herd_id": herd_id,
            # **WHERE THE PLAYER PUT THIS ROW WHEN THE BAND RUNS SHORT** (`docs/plan_standing_upkeep.md`
            # §4.9 item 9b) — one of the three WORDS, normalized by `effective_worker_map`. It rides
            # beside the floor because it is the same kind of thing: a standing property of the row
            # the player states, which line two prints and the inspector's picker edits.
            "priority": HudWorkVocab.work_priority_of(m.get("priority", "")),
            # **`build_queue_position` IS NOT ON THIS MODEL, AND ITS ABSENCE IS THE POINT**
            # (`docs/plan_standing_upkeep.md` §4.9 item 9a). It rode here as the queue block's rank
            # until the block learned that the field is published per SOURCE and rides the WINNING
            # band — the soonest estimate among the bands working it — so it is routinely another
            # band's place in another band's line. This model is BAND-scoped: every question it can be
            # asked ("where is this in the queue?", "is it the head?", "has the wire placed it?") is a
            # band's question, and none of them can be answered from that number. The rank is the
            # index into `HudBandLaborState.build_queue_keys`, which `_build_queue_models` stamps onto
            # the model as `BUILD_QUEUE_ROW_RANK_KEY` beside `BUILD_QUEUE_ROW_PENDING_KEY`. The field
            # survives on the raw wire SOURCE,
            # where the map annotation and the tile card read it as what it is.
            #
            # The RAW countdown does stay: it is the sim's own chained estimate for the source and the
            # date column's only input. It is a source-addressed readout too — it rides the same
            # winning band, which `snapshot.fbs` states — and the queue row quoting it is the best
            # answer anyone has, which §4.9 records as deliberately out of that item's scope.
            "build_turns": SourceForecast.build_turns_remaining(
                rung_source, HudComposeVocab.BARE_FORECAST_PREFIX),
            # **WHERE THE QUEUED ENTRY IS TAKING THIS SOURCE, AND WHAT IS LEFT OF THE CLIMB**
            # (`docs/plan_standing_upkeep.md` §2.8). An entry names a DESTINATION and lays every rung
            # between where the source stands and there, so it is ONE queue row with its legs INSIDE
            # — the block reads both off the model here, beside the position and the countdown they
            # belong with. `""` / `[]` on a source no band has queued.
            "build_destination": SourceForecast.build_destination_rung(
                rung_source, HudComposeVocab.BARE_FORECAST_PREFIX),
            "build_legs": SourceForecast.build_legs(
                rung_source, HudComposeVocab.BARE_FORECAST_PREFIX),
            # **AND WHAT THE ENTRY IS BEING RAISED WITH** (`docs/plan_standing_upkeep.md` §4.7a ②) —
            # the RESOLVED builders kit of the winning band's queue entry, `""` on a source nobody
            # has queued. It is the queue row's settings strip that spends it; the field is composed
            # here beside the other five build fields because this is where the raw wire source is in
            # hand, and because the five are one reading of one entry.
            "build_kit_id": SourceForecast.build_kit_id(
                rung_source, HudComposeVocab.BARE_FORECAST_PREFIX),
            # **WHY THE BUILDERS ARE HELD ON THIS ENTRY, THROUGH THE ONE PRODUCER THE SOURCE'S OWN
            # CARD USES** (`docs/plan_standing_upkeep.md` §4.6b). The countdown above says the pool is
            # stuck; these say which conjunct of the rung's gate refused, and — where the cause is the
            # escapement floor and the keeping is also short — what frees it. Composed here, beside
            # the other three build fields, because this is where the raw wire source is in hand;
            # the queue row spends it on its TOOLTIP, a one-line row having nowhere to put a
            # sentence. **`DetailFormat.build_blocked_lines` is that producer for BOTH surfaces** —
            # two copies of a refusal is how the card and the queue come to disagree — and the indent
            # is dropped because a tooltip hangs beneath no rung row.
            "build_blocked_lines": DetailFormat.build_blocked_lines(
                rung_source, HudComposeVocab.BARE_FORECAST_PREFIX,
                SourceForecast.source_kind_for_labor(kind),
                HudWorkVocab.BUILD_QUEUE_TOOLTIP_UNINDENTED),
            # **THE KIT THIS CREW IS ALREADY WORKING UNDER** (`LaborAssignment.kitId`, always a real
            # roster id on a forage/hunt row). It rides the model for one reason: `_emit_work_assign`
            # RESTATES it, so a `+`/`−` on the board cannot silently re-kit a crew back to the job
            # default — the same rule, and the same failure, as the improvement axis beside it.
            "kit_id": String(m.get("kit_id", KitRoster.NO_KIT_ID)),
            "can_add": bool(cap.get("can_add", idle > 0)),
            "schedule": HudBandLaborState.as_schedule(m.get("arrival_schedule", null)),
            "tooltip": HudFormat.join_tooltip_lines([String(yld.get("tooltip", "")),
                HudFormat.floor_hint(floor, kind), String(cap.get("note", "")),
                under_kept_hint,
                ready_tooltip,
                building_tooltip,
                HudWorkVocab.WORK_ROW_OPEN_HINT]),
            # A source wants attention when it overdraws, wastes workers, or is still unacknowledged.
            "attention": bool(yld.get("warn", false)) or note != "" or pending,
        })
    return models

## The source's STANDING RUNG as `{glyph, tooltip}` — `{}` for WILD ground / a wild herd, which is the
## honest default and keeps the common row unmarked (see `HudWorkVocab.WORK_ROW_RUNG_TENDED_TOOLTIP`).
##
## **THE HIGHER RUNG IS TESTED FIRST, and that ordering is load-bearing**: a Field is ALSO
## `is_cultivated` and a penned herd is ALSO fully domesticated, so testing rung 2 first would mark
## every rung-3 source as a rung-2 one — collapsing exactly the distinction this mark exists to draw.
##
## The dicts are the RAW wire ones (`forage_patch_lookup` / `world_herds`), so every key is spelled
## BARE. Do NOT reach for the `patch_`-prefixed `tile_info` spellings here.
## **IS THIS SOURCE'S IN-FLIGHT METER A REPAIR NOBODY HAS ORDERED?** — the one case in which the work
## row shows the `⌃` OFFER over the `%` face, and the three terms are each load-bearing:
##
##   * **the rung is achieved and its meter has room** (`SourceForecast.rung_needs_repair`). An
##     ordinary part-built rung — a Cultivate at 45% that has never been finished — is a real build in
##     flight and keeps its number and its stalled `⚠`; nothing about those rows moves.
##   * **nothing is declared on it** — a press of the `⌃` writes the declaration into the optimistic
##     overlay, and this reads that same effective `improvement`, so the mark leaves the slot on the
##     frame it is pressed exactly as it does on an unbuilt rung.
##   * **nothing is queued on it** — the confirmed half of the same fact, once the sim has placed the
##     entry. Without it the row would keep offering a job the band is already funding, and a second
##     press would queue it twice.
func _rung_is_an_unordered_repair(source: Dictionary, improvement: String,
        building_rung: String) -> bool:
    if improvement.strip_edges() != "":
        return false
    if SourceForecast.build_queue_position(source,
            HudComposeVocab.BARE_FORECAST_PREFIX) != SourceForecast.NOT_IN_ANY_BUILD_QUEUE:
        return false
    return SourceForecast.rung_needs_repair(source,
        HudComposeVocab.BARE_FORECAST_PREFIX, building_rung)

func _work_source_rung(kind: String, patch: Dictionary, herd: Dictionary) -> Dictionary:
    if kind == SourceForecast.LABOR_KIND_FORAGE:
        var crop := String(patch.get("committed_display_name", "")).strip_edges()
        if bool(patch.get("is_field", false)):
            return {
                "glyph": DetailFormat.field_glyph(),
                "tooltip": HudWorkVocab.WORK_ROW_RUNG_FIELD_TOOLTIP if crop == "" \
                    else HudWorkVocab.WORK_ROW_RUNG_FIELD_CROP_FORMAT % crop,
            }
        if bool(patch.get("is_cultivated", false)):
            return {
                "glyph": DetailFormat.CULTIVATION_GLYPH,
                "tooltip": HudWorkVocab.WORK_ROW_RUNG_TENDED_TOOLTIP if crop == "" \
                    else HudWorkVocab.WORK_ROW_RUNG_TENDED_CROP_FORMAT % crop,
            }
        return {}
    if bool(herd.get("corralled", false)):
        return {"glyph": DetailFormat.CORRAL_GLYPH, "tooltip": HudWorkVocab.WORK_ROW_RUNG_PENNED_TOOLTIP}
    if float(herd.get("domestication", 0.0)) >= DetailFormat.HUSBANDRY_PROGRESS_COMPLETE:
        return {"glyph": DetailFormat.pastoral_glyph(), "tooltip": HudWorkVocab.WORK_ROW_RUNG_PASTORAL_TOOLTIP}
    return {}

## Reset a filter that now selects nothing back to `All`. A kind/attention chip is hidden once its set
## empties (the last herd is unassigned, the last ⚠ clears), so a standing filter would otherwise
## strand the player on an empty board with no chip left to press to get back out of it.
func _reconcile_work_filter(models: Array) -> void:
    if _work_filter == HudWorkVocab.WORK_FILTER_ALL:
        return
    if _work_models_matching(_work_filter, models).is_empty():
        _work_filter = HudWorkVocab.WORK_FILTER_ALL

func _filter_work_models(models: Array) -> Array:
    return _work_models_matching(_work_filter, models)

func _work_models_matching(filter: StringName, models: Array) -> Array:
    match filter:
        HudWorkVocab.WORK_FILTER_FORAGE:
            return models.filter(func(m): return String(m["kind"]) == SourceForecast.LABOR_KIND_FORAGE)
        HudWorkVocab.WORK_FILTER_HUNT:
            return models.filter(func(m): return String(m["kind"]) == SourceForecast.LABOR_KIND_HUNT)
        HudWorkVocab.WORK_FILTER_ATTENTION:
            return models.filter(func(m): return bool(m["attention"]))
        HudWorkVocab.WORK_FILTER_READY:
            return models.filter(func(m): return String(m["ready_policy"]) != "")
    return models.duplicate()

func _sort_work_models(models: Array) -> void:
    if _work_sort == HudWorkVocab.WORK_SORT_NAME:
        models.sort_custom(_work_name_sorts_before)
    else:
        models.sort_custom(func(a, b): return _work_sorts_before(a as Dictionary, b as Dictionary))

## "Sort by name" — KIND FIRST, then label, then `key`.
##
## **THE LABEL PREFIX IS NOT A PROXY FOR THE KIND, so alphabetical order alone SPLITS A KIND IN TWO.**
## A forage row whose Cultivate improvement is done renders through `WORK_ROW_TEND_FORMAT`
## ("Tend (%d, %d)"), which is display only — its `kind` is still `forage`. With three live prefixes
## and "Forage" < "Hunt" < "Tend", a band working a wild patch, a herd and a Tended Patch would read
## Forage → Hunt → Tend, i.e. the forage block interrupted by the hunt block. The `Forage`/`Hunt`
## filter chips select on `kind` (`_work_models_matching`), so the unsorted-by-kind board would not
## match the blocks those chips name. Leading with the kind makes the board agree with the chips
## whatever a row's label says.
##
## The `key` tiebreak makes it a TOTAL ORDER. `sort_custom` is NOT stable in Godot and a label tie is
## genuinely reachable — two herds of the same species render the identical `WORK_ROW_HUNT_FORMAT`
## label — so without it two tied rows could swap on any unrelated re-render (a snapshot tick, a zone
## resize), which is the same row-jumps-under-the-pointer failure the default sort exists to remove.
## `key` is the source identity `_work_source_models` already assigns, i.e. the one available field no
## game state moves.
func _work_name_sorts_before(a: Dictionary, b: Dictionary) -> bool:
    # A BOOLEAN TIER, the same idiom `_work_sorts_before` uses, because there are exactly two labor
    # kinds. A third kind cannot be expressed this way — it would need an explicit rank table, since
    # a bool can only say "this one first".
    var a_is_forage := String(a.get("kind", "")) == SourceForecast.LABOR_KIND_FORAGE
    var b_is_forage := String(b.get("kind", "")) == SourceForecast.LABOR_KIND_FORAGE
    if a_is_forage != b_is_forage:
        return a_is_forage
    var by_label := String(a.get("label", "")).naturalnocasecmp_to(String(b.get("label", "")))
    if by_label != 0:
        return by_label < 0
    return String(a.get("key", "")) < String(b.get("key", ""))

## "Sort by yield", in TWO TIERS (issues #337 / #449) — the account order the rest of the arc uses:
## every FOOD-paying source first, ordered by its food figure descending; then the rest by their
## FODDER figure descending.
##
## **THIS IS NOT A RAW MAGNITUDE SORT, AND MUST NOT BE "FIXED" INTO ONE.** Ranking a hay Field's 0.40
## fodder above a patch's 0.15 food compares two quantities the sim publishes NO exchange rate
## between, and under a control labelled "sort by yield" that ordering asserts the meadow is the more
## productive source — a claim the game does not make and the player cannot check. Tiering asserts
## nothing about an exchange rate; it only fixes the ORDER OF ATTENTION.
##
## Why food takes the first tier is NOT "food is worth more per unit". It is that the larder is the
## live survival constraint the player is deciding against every turn, while fodder feeds the pens
## rather than the people. (Sorting on food ALONE was the original bug: it interleaved non-food
## sources among the zero-food rows at the bottom of the board, off page one on a busy band, which is
## the same "a source that pays no calories is worth nothing" reading the per-row work removed. A
## THIRD tier stood between these two for the trade account until arc #527 retired it.)
##
## A source paying into NO account has a fodder figure of 0.0 and therefore sorts to the BOTTOM of the
## fodder tier, i.e. last overall — unchanged in every board that grows no hay.
##
## **THE `key` TIEBREAK MAKES IT A TOTAL ORDER, and that is a correctness fix**: `sort_custom` is NOT
## stable in Godot and equal rates are common — two patches at the same food figure inside the food
## tier, and every source paying into no account at all, all of which sit together at 0.0 at the foot
## of the fodder tier. Tied rows could otherwise swap on any unrelated re-render. The tiebreak rides
## BELOW the tier + rate comparisons and changes neither.
func _work_sorts_before(a: Dictionary, b: Dictionary) -> bool:
    var a_pays_food := SourceForecast.has_component(float(a.get("rate", 0.0)))
    var b_pays_food := SourceForecast.has_component(float(b.get("rate", 0.0)))
    if a_pays_food != b_pays_food:
        return a_pays_food
    # Exact `!=` rather than `is_equal_approx`: an epsilon tie test is NOT transitive (a≈b and b≈c
    # without a≈c), which would break the strict weak ordering `sort_custom` requires — the very
    # property this tiebreak exists to establish.
    if a_pays_food:
        if float(a.get("rate", 0.0)) != float(b.get("rate", 0.0)):
            return float(a.get("rate", 0.0)) > float(b.get("rate", 0.0))
        return String(a.get("key", "")) < String(b.get("key", ""))
    if float(a.get("fodder_rate", 0.0)) != float(b.get("fodder_rate", 0.0)):
        return float(a.get("fodder_rate", 0.0)) > float(b.get("fodder_rate", 0.0))
    return String(a.get("key", "")) < String(b.get("key", ""))

func _find_work_model(models: Array, key: String) -> Dictionary:
    if key == "":
        return {}
    for m in models:
        if String((m as Dictionary).get("key", "")) == key:
            return m
    return {}

## Re-send this source's `assign_labor` at a new worker count (and optionally a new policy) — the
## same emit the old Current-actions stepper made.
##
## **THE IMPROVEMENT RIDES EVERY CREW EDIT** (issue #442). `assign_labor` deliberately does not touch
## the second axis, so a `+`/`−`/Unassign/stance pick that let the pending overlay default to
## `IMPROVEMENT_NONE` would blank the axis for the rest of the turn: the row's build badge and its
## `⌃`-vs-progress slot would flip back to advertising the very rung already under way. (It used to
## move the row's worker CAP too, through the retired `herd_crew_floor`; that floor went with the
## keeper crew's move onto its own allocation.) The row MODEL already carries
## the value `effective_worker_map` resolved (confirmed assignment overlaid with any pending edit), so
## it is restated from there rather than re-derived — re-deriving could disagree with the board the
## player is clicking on.
## `floor` defaults to `RESTATE_STANDING_FLOOR` — a sentinel outside the legal `0..1` range, so
## "leave the floor alone" is expressible on an axis where every real value including `0` is a
## meaningful choice. A crew-size edit must not silently re-point the crew.
const RESTATE_STANDING_FLOOR := -1.0

## **`species` IS THE CROP TOKEN, AND ITS DEFAULT IS `RESTATE_STANDING_SPECIES` RATHER THAN `""`**
## (`docs/plan_standing_upkeep.md` §4.7a ③). `""` is a REAL instruction on this axis — *take the
## tile's dominant legal plant* — so it cannot double as *leave the crop alone*, which is the floor's
## own sentinel problem one field over. A `+`/`−` on the board therefore restates the row's committed
## crop, exactly as it restates the improvement and the kit; only the QUEUE ROW's picker states one.
const RESTATE_STANDING_SPECIES := "￿"

func _emit_work_assign(band: Dictionary, model: Dictionary, workers: int,
        floor: float = RESTATE_STANDING_FLOOR,
        species: String = RESTATE_STANDING_SPECIES) -> void:
    var kind := String(model.get("kind", ""))
    var standing := float(model.get("floor", SourceForecast.DEFAULT_HARVEST_FLOOR))
    _emit_assign_labor(band, kind, workers, int(model.get("x", -1)), int(model.get("y", -1)),
        String(model.get("herd_id", "")),
        standing if floor == RESTATE_STANDING_FLOOR else floor,
        _band_labor.species_for_forage(band, int(model.get("x", -1)), int(model.get("y", -1))) \
            if species == RESTATE_STANDING_SPECIES else species,
        String(model.get("improvement", "")),
        # **THE KIT RIDES EVERY CREW EDIT, for the improvement's reason.** An omitted `kit <id>` token
        # means "the job's default" to the parser, so a `+`/`−` that dropped it would re-kit a crew
        # the player deliberately sent out bare-handed. Restated from the row model, which carries the
        # assignment's own `kit_id`.
        String(model.get("kit_id", KitRoster.NO_KIT_ID)),
        # **AND THE TAKE SELECTION RIDES IT TOO, for the identical reason one axis over.** An omitted
        # `take:` token means *the whole basket* to the parser and CLEARS whatever the row carried, so
        # a `+`/`−` on the board that dropped it would silently widen a crew the player had narrowed
        # to one plant. Restated off the band's OWN row rather than re-derived — the work model
        # carries no take selection, and a second derivation could disagree with the board being
        # clicked on.
        _band_labor.take_species_for_forage(band, int(model.get("x", -1)), int(model.get("y", -1))))

## Jump the map to a worked source — a fixed forage tile, or a herd at its LIVE (migrated) tile.
func _focus_work_source(model: Dictionary) -> void:
    if String(model.get("kind", "")) == SourceForecast.LABOR_KIND_HUNT:
        _focus_hunt_source(String(model.get("herd_id", "")), int(model.get("x", -1)), int(model.get("y", -1)))
    else:
        focus_labor_source(int(model.get("x", -1)), int(model.get("y", -1)))

## One inspector row at a time — opening a second closes the first (and opening one costs the board
## rows, which is why `_work_board_capacity` subtracts the strip's height).
## ⛔ **ONE EXPANSION OPEN AT A TIME IN THE WORK ZONE — the queue's strip and the board's inspector
## are MUTUALLY EXCLUSIVE** (`docs/plan_standing_upkeep.md` §4.7b).
##
## **THE DEFECT IT CLOSES SHIPPED AND WAS REACHABLE IN ONE CLICK EACH.** Open a queue row's settings
## AND a work row's inspector on a bottom dock and `Zone_work` drew 426 into a 396 box, with the board
## already at its `maxi(1, …)` floor and nothing left to give back. No frame caught it because every
## strip-open frame had no inspector and every inspector-open frame had no strip — two disjoint frame
## families, the defect living in the gap, which is the same shape §4.7 found in the inspector's own
## height. Each list already enforced this rule INTERNALLY; it is the same rule read one level up, and
## it costs nothing.
func _toggle_work_inspector(key: String) -> void:
    _work_open_key = "" if _work_open_key == key else key
    _work_picker_open = HudWorkVocab.WORK_PICKER_NONE
    if _work_open_key != "":
        _queue_open_key = ""
    _repage_work_zone()

func _set_work_filter(filter: StringName) -> void:
    if _work_filter == filter:
        return
    _work_filter = filter
    _work_page = 0
    _repage_work_zone()

func _set_work_sort(sort: StringName) -> void:
    if _work_sort == sort:
        return
    _work_sort = sort
    # A sort is a standing preference, not a per-session mood — persist it through the panel, which
    # owns the prefs file.
    if _panel != null:
        _panel.set_work_sort_pref(String(sort))
    _work_page = 0
    _repage_work_zone()

func _step_work_page(delta: int) -> void:
    _work_page = maxi(_work_page + delta, 0)
    _repage_work_zone()

## The Work menu's destructive entry. Scoped `work`: Forage + Hunt only — standing roles, parties and
## an in-progress move are untouched, which is exactly what the confirm promises.
func _on_work_unassign_all_pressed(band: Dictionary, count: int) -> void:
    if band.is_empty() or count <= 0:
        return
    _confirm_destructive(HudWorkVocab.WORK_UNASSIGN_CONFIRM_FORMAT % count, HudWorkVocab.WORK_UNASSIGN_CONFIRM_OK,
        func() -> void: _emit_cancel_order(band, HudComposeVocab.CANCEL_SCOPE_WORK))

## Clear labor for a band at `scope` (`all` / `work` / `roles`). Main formats the
## `cancel_order <faction> <band> <scope>` command.
func _emit_cancel_order(band: Dictionary, scope: String) -> void:
    if band.is_empty():
        return
    emit_signal("cancel_order_requested", band, scope)

# ---- zone `parties` ---------------------------------------------------------

## Zone `parties`: head + `⋯` menu · one row per party in the field · the compose footer.
##
## **THE LIST BETWEEN THE HEAD AND THE FOOTER SCROLLS, AND IT IS THE ONLY THING IN THE PANEL THAT
## DOES.** The zone's chrome is fixed — the head names the section and the Scout/Hunt/Deny row is the
## zone's whole purpose, so neither may scroll out of reach — while the rows and the open inspector
## strip are unbounded content, and the strip's seven-line worst case measured **294px of the 300px
## box**, i.e. it was the tallest thing in the panel and what pinned the strip's height for BOTH
## column counts.
##
## **IT DOES NOT REOPEN THE FLICKER BUG, and that is the whole argument for the exception.** The panel
## is no-scroll because a zone whose content height fed back into a FIXED reservation would re-emit
## `reservation_changed` → `MapView.set_reserved_inset` on every edit. This scroll declares
## `HudWorkVocab.PARTIES_LIST_MIN_HEIGHT` and NOTHING ELSE as its minimum, so what the list holds never
## reaches the column's minimum, never reaches the panel, and never reaches the reservation. The work
## zone answers the same requirement by PAGING; this zone cannot page, because the strip is a
## disclosure that must sit under the row it was opened from.
##
## The scroll takes `SIZE_EXPAND_FILL`, which is what the old bottom spacer did — so the footer is
## still pinned to the bottom of the zone and a short list still renders exactly where it did.
func build_parties_zone(band: Dictionary) -> VBoxContainer:
    # BEFORE anything reads the latched float requirement below: a box change invalidates the mark.
    _note_parties_zone_box()
    var col := HudWidgets.make_zone_column()
    col.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
    # Held for the deferred compose-sheet measurement, which needs the zone's own laid-out rect to
    # know where the footer ended up inside it (see `_party_compose_needed`).
    _parties_zone_col = col
    var parties := _band_labor.band_parties(band)
    var menu := HudWidgets.build_section_menu([
        {"label": HudComposeVocab.PARTY_RECALL_ALL_FORMAT % parties.size(), "disabled": parties.is_empty(),
            "on_pick": func() -> void: _on_recall_all_parties_pressed(parties)},
    ], HudComposeVocab.PARTY_MENU_TOOLTIP)
    col.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_PARTIES,
        HudComposeVocab.PARTIES_HEADER_FORMAT % [parties.size(), _band_labor.band_party_workers(band)], menu))
    # **THE EMPTY HINT IS ZONE-LEVEL, NOT A LIST ITEM** — `_fill_work_zone`'s idiom exactly, where the
    # board's empty hint is a sibling of the board rather than a row of it. What the zone says when
    # there are no parties is a statement about the ZONE, and it must never be something the player can
    # scroll away from; the list below it is simply empty.
    if parties.is_empty():
        col.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.PARTIES_EMPTY_HINT))
    var list := _build_parties_list()
    col.add_child(list)
    var rows: VBoxContainer = list.get_child(0)
    for exp in parties:
        rows.add_child(_build_party_row(exp))
    # Order inside the scrolled list: rows → inspector (if open), so the strip sits under the clicked
    # row (the strip is a row → detail disclosure, the parties twin of the work board's inspector).
    # Drop a strip pinned to a party that has left the list (recalled, moved to another band),
    # mirroring `_fill_work_zone`'s stale-key clear. The strip's own line separation stays tightened
    # (PARTIES_INSPECTOR_LINE_SEPARATION) — scrolling to read a strip is a fallback, not the intent.
    var inspected := _party_by_open_key(parties)
    if inspected.is_empty():
        _party_open_key = ""
    else:
        rows.add_child(_build_parties_inspector(inspected))
    col.add_child(_build_party_footer(band))
    return col

## The parties zone's scrolling list host — a `ScrollContainer` whose single child is the VBox the rows
## and the inspector strip go into. Returned with that VBox as child 0, which is what the builder above
## fills.
##
## Three settings carry the contract:
##   * **vertical AUTO** — the bar appears only when the content really overflows, so a band with one
##     party looks exactly as it did before this zone could scroll at all.
##   * **horizontal DISABLED** — the rows are already fitted to the flank's fixed width; a horizontal
##     bar here would mean a row had been built too wide, which is a layout bug rather than something
##     to offer the player a control for. DISABLED also forces the child to the container's width,
##     which is what keeps the rows full-bleed.
##   * **a declared `custom_minimum_size.y`** — the ONE number this zone contributes to the column's
##     minimum. A `ScrollContainer` reports no minimum on a scrolling axis, so without it the zone
##     would claim to need nothing at all and `band_panel_preview`'s content-fit walk would descend
##     past it and measure the unbounded list instead.
func _build_parties_list() -> ScrollContainer:
    var scroll := ScrollContainer.new()
    scroll.name = HudWorkVocab.PARTIES_LIST_NAME
    scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
    scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
    scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
    scroll.custom_minimum_size = Vector2(0.0, HudWorkVocab.PARTIES_LIST_MIN_HEIGHT)
    var rows := HudWidgets.make_zone_column()
    # The zone's own block spacing, not the column's section spacing: these are sibling ROWS plus the
    # strip that belongs to one of them, exactly the spacing they had as direct children of `col`.
    rows.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
    # A scrolled child must not claim the viewport's height as its own, or a short list would stretch
    # its rows down the zone; the width still fills, since horizontal scrolling is disabled.
    rows.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
    scroll.add_child(rows)
    return scroll

## The party in `parties` whose entity matches `_party_open_key`, or `{}` when none is open / the open
## one has left the list (the caller then clears the stale key).
func _party_by_open_key(parties: Array) -> Dictionary:
    if _party_open_key == "":
        return {}
    for exp_variant in parties:
        if exp_variant is Dictionary:
            var exp: Dictionary = exp_variant
            if str(int(exp.get("entity", -1))) == _party_open_key:
                return exp
    return {}

## Toggle the parties inspector strip open/closed for `key` (an expedition entity as a string), then
## re-render the parties zone in place — the same path the footer mission buttons already drive.
func _toggle_parties_inspector(key: String) -> void:
    _party_open_key = "" if _party_open_key == key else key
    rerender()

## The parties inspector strip — the full Mission/Target/Policy/Phase/Carried/Next-delivery/Position
## detail for one party, opened by a row click. Mirrors `_build_work_inspector`: a titled header with a
## close `✕`, the detail lines as dim status parts, and inline Jump/Recall links.
func _build_parties_inspector(exp: Dictionary) -> PanelContainer:
    var strip := PanelContainer.new()
    strip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    strip.add_theme_stylebox_override("panel", HudStyle.work_inspector_stylebox())
    var col := VBoxContainer.new()
    col.add_theme_constant_override("separation", HudComposeVocab.PARTIES_INSPECTOR_LINE_SEPARATION)
    strip.add_child(col)
    var entity := int(exp.get("entity", -1))
    var x := int(exp.get("current_x", -1))
    var y := int(exp.get("current_y", -1))
    var head := HBoxContainer.new()
    head.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    var title := Label.new()
    title.text = HudFormat.panel_expedition_summary(exp, _herd_label_for_id,
        _band_labor.band_label_for_id)
    title.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
    title.clip_text = true
    title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(title)
    var close := Button.new()
    close.text = HudWorkVocab.INSPECTOR_CLOSE_GLYPH
    close.focus_mode = Control.FOCUS_NONE
    close.tooltip_text = HudWorkVocab.INSPECTOR_CLOSE_TOOLTIP
    HudStyle.apply_button(close, "ghost")
    HudWidgets.compact(close, HudWorkVocab.WORK_ROW_FONT_SIZE, HudWorkVocab.INSPECTOR_CLOSE_PADDING_V)
    close.pressed.connect(func() -> void: _toggle_parties_inspector(str(entity)))
    head.add_child(close)
    col.add_child(head)
    # The strip's `Collapse:` row is a forecast QUERY now, and this controller is what may ask one —
    # it holds the seam and it re-renders on `answered`, so the reply lands back here.
    # It elides for the WORK strip's reason, one zone over: `Carried:` carries a term per material
    # batch and the `Collapse:` verdict is a whole sentence, so this strip's longest line is a
    # function of what the party is hauling — and a line wider than the reserved zone would take the
    # tab's own controls off its right edge rather than its own tail. See `build_status_part`.
    for line in _banddetail.expedition_summary_lines(exp, null, launched_party_denial_view(exp)):
        col.add_child(HudWidgets.build_status_part(line, HudStyle.INK_DIM, true))
    var links := HBoxContainer.new()
    links.add_theme_constant_override("separation", HudWorkVocab.COMPOSITION_KEY_SEPARATION)
    links.add_child(HudWidgets.build_inline_link(HudComposeVocab.PARTY_INSPECT_JUMP, HudStyle.INK, func() -> void:
        select_expedition(entity, x, y)))
    links.add_child(HudWidgets.build_inline_link(recall_verb(exp), HudStyle.DANGER, func() -> void:
        confirm_recall_expedition(exp)))
    col.add_child(links)
    return strip

## One party row: mission glyph · subject · phase chip · an always-visible recall `✕` (dimmed at rest,
## bright on hover) as the quick removal path. Clicking the row BODY toggles the parties inspector
## strip (the full Mission/Target/…/Next-delivery detail), mirroring the work board's row → inspector.
func _build_party_row(exp: Dictionary) -> HBoxContainer:
    var phase := HudFormat.expedition_phase_key(exp)
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_SEPARATION)
    var body := Button.new()
    body.text = HudFormat.panel_expedition_summary(exp, _herd_label_for_id,
        _band_labor.band_label_for_id)
    body.alignment = HORIZONTAL_ALIGNMENT_LEFT
    body.focus_mode = Control.FOCUS_NONE
    body.clip_text = true
    body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(body, "ghost")
    if phase == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
        body.add_theme_color_override("font_color", HudStyle.WARN)
    body.tooltip_text = DetailFormat.expedition_row_tooltip(
        exp, phase, _band_labor.expedition_target_herd(exp), launched_party_denial_view(exp))
    var entity := int(exp.get("entity", -1))
    body.pressed.connect(func() -> void: _toggle_parties_inspector(str(entity)))
    row.add_child(body)
    var recall := Button.new()
    recall.text = HudComposeVocab.PARTY_RECALL_GLYPH
    recall.focus_mode = Control.FOCUS_NONE
    # The GLYPH is the same either way (a ✕ removes the row on both branches); the tooltip is what says
    # whether the press cancels an order that never took effect or sends the party on a walk home.
    recall.tooltip_text = recall_tooltip(exp)
    recall.custom_minimum_size = Vector2(HudComposeVocab.PARTY_RECALL_WIDTH, 0.0)
    HudStyle.apply_button(recall, "ghost")
    # DANGER-red like the Work inspector's destructive "Unassign" link — it removes a party. The steady
    # red already reads as destructive, so it rests at full opacity (no alpha dim) and brightens no
    # further on hover. Confirms before recalling (its own single-party prompt, NOT the raw emit).
    recall.add_theme_color_override("font_color", HudStyle.DANGER)
    recall.pressed.connect(func() -> void: confirm_recall_expedition(exp))
    row.add_child(recall)
    return row

## The verb a single-party recall control wears for `exp` — `Cancel` where the sim will fold the party
## back on the spot, `Recall` where it will walk home. The row ✕, the parties inspector link and the
## Occupants drawer's button all read THIS, so they cannot promise different things about one press.
func recall_verb(exp: Dictionary) -> String:
    return HudComposeVocab.PARTY_CANCEL_VERB if _band_labor.party_cancels_in_camp(exp) \
        else HudComposeVocab.PARTY_RECALL_VERB

## The tooltip that goes with `recall_verb` — same fork, same one reading of the predicate.
func recall_tooltip(exp: Dictionary) -> String:
    return HudComposeVocab.PARTY_CANCEL_TOOLTIP if _band_labor.party_cancels_in_camp(exp) \
        else HudComposeVocab.PARTY_RECALL_TOOLTIP

## Act on a SINGLE party's recall. Wraps the button handlers (row ✕, inspector link, drawer button) —
## NOT the shared `_on_recall_expedition_pressed` emit, which "Recall all" loops under its own one
## confirm. The prompt names the party (hunt → its herd, scout → the mission word).
##
## **A CANCEL ASKS NOTHING AND FIRES ON THE PRESS.** `_confirm_destructive` is for an action that LOSES
## something — the work board's unassign-all, a real recall abandoning a trip in progress. A party still
## standing in its home band's camp has spent no travel and abandoned no haul, and re-launching it is
## one press of the same footer button, so a modal there is ceremony over a decision the player can
## simply re-make. The bulk `Recall all` keeps its single confirm regardless: it acts over a MIXED set,
## where the prompt is the only place the whole scope is stated.
func confirm_recall_expedition(exp: Dictionary) -> void:
    if _band_labor.party_cancels_in_camp(exp):
        _on_recall_expedition_pressed(exp)
        return
    _confirm_destructive(HudComposeVocab.PARTY_RECALL_ONE_CONFIRM_FORMAT % _party_confirm_label(exp),
        HudComposeVocab.PARTY_RECALL_ONE_CONFIRM_OK,
        func() -> void: _on_recall_expedition_pressed(exp))

## How a prompt NAMES a party — its herd for a hunt, the bare mission word otherwise. Shared by the
## recall prompt and the founding one so the two cannot name one party two ways.
func _party_confirm_label(exp: Dictionary) -> String:
    var mission := String(exp.get("expedition_mission", "")).strip_edges().to_lower()
    if mission == HudExpeditionVocab.EXPEDITION_MISSION_HUNT:
        return _herd_label_for_id(String(exp.get("expedition_target_herd", "")).strip_edges())
    return HudComposeVocab.PARTY_RECALL_SCOUT_LABEL

## Recall every party in one go — there is no bulk verb on the wire and parties are few, so this is
## one `recall_expedition` per party through the existing signal.
func _on_recall_all_parties_pressed(parties: Array) -> void:
    if parties.is_empty():
        return
    _confirm_destructive(HudComposeVocab.PARTY_RECALL_CONFIRM_FORMAT % parties.size(), HudComposeVocab.PARTY_RECALL_CONFIRM_OK,
        func() -> void:
            for exp in parties:
                _on_recall_expedition_pressed(exp))

## The parties footer: FOUR buttons offered directly — the three expedition missions (Scout / Hunt /
## Deny) and the SPLIT, which is not a mission — each opening the compose sheet already on that
## verb, or the compose sheet in their place. A button with nothing to spend stays VISIBLE and
## DISABLED with its reason: the section vanishing is what made expeditions look like they had been
## removed from the game. **The two gates are different pools** — the three expeditions want idle
## workers, the split wants workers — so the no-idle hint below names the expeditions rather than the
## row, or it would contradict a live Split beside it.
func _build_party_footer(band: Dictionary) -> VBoxContainer:
    var idle := _band_labor.effective_idle(band)
    var foot := HudWidgets.make_zone_block()
    # The three EXPEDITION missions need idle workers to compose with; a split needs workers, which is
    # a different pool — see `_split_worker_pool`.
    var compose_pool := _split_worker_pool(band) \
        if _party_compose_mission == HudComposeVocab.COMPOSE_MISSION_SPLIT else idle
    if _party_compose_open and _party_compose_mission != "" and compose_pool > 0:
        var sheet := _build_compose_sheet(band, idle)
        _party_compose_sheet = sheet
        # **THE ONE FORK, AND IT IS DECIDED BY A MEASUREMENT** (`_party_compose_needed` carries the
        # whole rationale): the sheet the zone cannot hold is the SAME sheet, from the same builders in
        # the same order, rendered in a card floated beside the panel instead of sliced by a
        # `clip_contents` host. Nothing about the form changes — only which node it is parented into.
        if _party_compose_floats():
            _mount_compose_float(sheet)
        else:
            _dismiss_compose_float()
            foot.add_child(sheet)
        return foot
    # No sheet open (or no idle workers to compose one with) ⇒ no float. Every teardown path — the ✕,
    # a cancel, a send, a panel-band change, the last idle worker leaving — reaches the footer builder,
    # so the float dies here rather than on a list of conditionals that can miss one.
    _party_compose_sheet = null
    _dismiss_compose_float()
    # **A GRID, NOT A ROW, SINCE THE FIFTH VERB ARRIVED.** Five buttons across a 354px dock column
    # leave each ~48px, which `📦 Trade` does not fit — and the zone `clip_contents`, so the fifth
    # was sliced off the edge rather than merely cramped. `HudComposeVocab.PARTY_FOOTER_COLUMNS`
    # wraps them 3 + 2, the same treatment `build_floor_picker` gives its six rungs.
    var missions := GridContainer.new()
    missions.columns = HudComposeVocab.PARTY_FOOTER_COLUMNS
    missions.add_theme_constant_override("h_separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    missions.add_theme_constant_override("v_separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    missions.add_child(_build_mission_launch_button(HudComposeVocab.COMPOSE_MISSION_SCOUT,
        HudComposeVocab.COMPOSE_MISSION_LABEL_SCOUT, HudComposeVocab.SEND_EXPEDITION_HINT, idle))
    missions.add_child(_build_mission_launch_button(HudComposeVocab.COMPOSE_MISSION_HUNT,
        HudComposeVocab.COMPOSE_MISSION_LABEL_HUNT, HudComposeVocab.SEND_HUNT_EXPEDITION_HINT, idle))
    # **THE THIRD VERB** (`docs/plan_denial_raid.md` §3). It sits beside the other two rather than
    # inside the hunt form, because what it changes is a BOUND and not a number: `floor = 0` still
    # only kills what the party can haul, so denial had to become a mission to have anything to
    # unclamp. Same button, same idle gate — the difference is entirely in the form it opens.
    missions.add_child(_build_mission_launch_button(HudComposeVocab.COMPOSE_MISSION_DENY,
        HudComposeVocab.COMPOSE_MISSION_LABEL_DENY, HudComposeVocab.SEND_DENIAL_RAID_HINT, idle))
    # **THE FOURTH MISSION** (arc #527, issue #517). A shipment is a party that walks it: it names
    # another BAND rather than a herd, and it carries a manifest drawn off this band's own stores.
    # Same idle gate as the other three — a shipment needs hands to haul it — and the thing it needs
    # BESIDES hands, a live tie, is stated inside the form rather than by greying this button: a band
    # with no ties has a legible empty picker and a sentence saying how ties form, where a dead button
    # would say only that trade is unavailable.
    missions.add_child(_build_mission_launch_button(HudComposeVocab.COMPOSE_MISSION_TRADE,
        HudComposeVocab.COMPOSE_MISSION_LABEL_TRADE, HudComposeVocab.SEND_TRADE_EXPEDITION_HINT,
        idle))
    # **THE FOURTH BUTTON IS NOT A MISSION** (issue #511) — a split makes a band rather than sending a
    # party. It sits here because this is where the player already comes to divide people out of a
    # band, and it is gated on WORKERS rather than on idle workers: splitting is not staffing a job,
    # so a band whose every hand is assigned may still divide (the assignments lapse with the people
    # who held them).
    missions.add_child(_build_mission_launch_button(HudComposeVocab.COMPOSE_MISSION_SPLIT,
        HudComposeVocab.COMPOSE_MISSION_LABEL_SPLIT, HudComposeVocab.SPLIT_BAND_HINT,
        _split_worker_pool(band)))
    foot.add_child(missions)
    if idle <= 0:
        foot.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.SEND_PARTY_NO_IDLE_REASON))
    return foot

## One footer mission button: opens the compose sheet already committed to `mission`.
func _build_mission_launch_button(mission: String, label: String, hint: String,
        idle: int) -> Button:
    var btn := Button.new()
    btn.text = label
    btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(btn, "primary")
    btn.tooltip_text = hint
    btn.disabled = idle <= 0
    btn.set_meta(HudWidgets.MISSION_LAUNCH_META, mission)
    btn.pressed.connect(func() -> void:
        _party_compose_open = true
        _party_compose_mission = mission
        # A fresh compose act starts with no quarry — never a herd left over from a cancelled one.
        _clear_party_quarry()
        # …and with an empty manifest, for exactly the same reason: goods loaded for a shipment the
        # player cancelled are not goods they asked to send now.
        _clear_trade_manifest()
        # **THE DENIAL SHEET ALWAYS OPENS ON THE PARTY THE SIM QUOTES**, so the seed is armed by the
        # sheet OPENING as well as by a quarry being adopted — a sheet that came back up on a quarry
        # it still remembered would otherwise present whatever count the last composition left behind.
        # Same one-shot either way (`consume_party_autofill`), so a manual −/+ tick still survives
        # every rerender while the sheet stays open, and it is still never seeded to 0.
        if mission == HudComposeVocab.COMPOSE_MISSION_DENY:
            _compose.arm_party_autofill()
        rerender())
    return btn

## The compose sheet. The mission is already settled by the footer button that opened it, so the
## sheet titles itself by mission and the policy picker is unreachable except under Hunt (it used to
## sit above the scouting button and read as if it modified it). `✕` is the only way back.
func _build_compose_sheet(band: Dictionary, idle: int) -> VBoxContainer:
    var is_hunt := _party_compose_mission == HudComposeVocab.COMPOSE_MISSION_HUNT
    var is_deny := _party_compose_mission == HudComposeVocab.COMPOSE_MISSION_DENY
    var is_trade := _party_compose_mission == HudComposeVocab.COMPOSE_MISSION_TRADE
    var sheet := HudWidgets.make_zone_block()
    var head := HBoxContainer.new()
    var title := Label.new()
    title.text = HudComposeVocab.COMPOSE_TITLE_SCOUT
    if is_hunt:
        title.text = HudComposeVocab.COMPOSE_TITLE_HUNT
    elif is_deny:
        title.text = HudComposeVocab.COMPOSE_TITLE_DENY
    elif is_trade:
        title.text = HudComposeVocab.COMPOSE_TITLE_TRADE
    elif _party_compose_mission == HudComposeVocab.COMPOSE_MISSION_SPLIT:
        title.text = HudComposeVocab.COMPOSE_TITLE_SPLIT
    title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    head.add_child(title)
    var cancel := Button.new()
    cancel.text = HudWorkVocab.INSPECTOR_CLOSE_GLYPH
    cancel.focus_mode = Control.FOCUS_NONE
    cancel.tooltip_text = HudComposeVocab.COMPOSE_CANCEL_TOOLTIP
    HudStyle.apply_button(cancel, "ghost")
    cancel.pressed.connect(func() -> void:
        _close_party_compose())
    head.add_child(cancel)
    sheet.add_child(head)
    if is_hunt:
        _fill_hunt_compose_sheet(sheet, band, idle)
        return sheet
    if is_deny:
        _fill_denial_compose_sheet(sheet, band, idle)
        return sheet
    if is_trade:
        _fill_trade_compose_sheet(sheet, band, idle)
        return sheet
    if _party_compose_mission == HudComposeVocab.COMPOSE_MISSION_SPLIT:
        _fill_split_compose_sheet(sheet, band)
        return sheet
    # SCOUT — a single input. Its only question is party size, and nothing about a scouting party
    # depends on where it is going, so the destination is still picked on the map after the send.
    # **THE CEILING IS THE BAND'S IDLE WORKERS**, as it is on all three launch verbs: the sim carries no
    # rules cap on party size, and `max_expedition_party_size` — which nothing here reads — echoed how
    # far the retired estimate tables were sampled, never a limit anyone may send under.
    var party_max := idle
    _send_expedition_count = clampi(_send_expedition_count, HudConst.WORKER_STEP, party_max)
    sheet.add_child(HudWidgets.build_party_stepper_row(_send_expedition_count, party_max,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, HudConst.WORKER_STEP, party_max)
            rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_OF_IDLE_FORMAT % idle))
    sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.SEND_EXPEDITION_HINT))
    var confirm := Button.new()
    confirm.text = HudComposeVocab.SEND_EXPEDITION_BUTTON
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(confirm, "primary")
    confirm.tooltip_text = HudComposeVocab.SEND_EXPEDITION_HINT
    confirm.pressed.connect(func() -> void:
        _close_party_compose()
        _targeting.begin_send_expedition(band, _send_expedition_count))
    sheet.add_child(confirm)
    return sheet

## **The SPLIT form** (`docs/plan_band_fission.md` §Q6) — one stepper, two readouts and a verdict.
##
## **THE PLAYER PICKS ONE NUMBER and everything else divides on the share it implies**, so there is
## nothing else to ask. What the sheet spends its room on instead is the consequence: what the new
## band would be, and what this one would be left as.
func _fill_split_compose_sheet(sheet: VBoxContainer, band: Dictionary) -> void:
    var pool := _split_worker_pool(band)
    _split_workers = clampi(_split_workers, HudConst.WORKER_STEP, pool)
    sheet.add_child(HudWidgets.build_party_stepper_row(_split_workers, pool,
        func(n: int) -> void:
            _split_workers = clampi(n, HudConst.WORKER_STEP, pool)
            rerender(),
        HudComposeVocab.SPLIT_STEPPER_LABEL))
    var share := (float(_split_workers) / float(pool)) if pool > 0 else 0.0
    sheet.add_child(HudWidgets.alloc_hint_label(
        HudComposeVocab.SPLIT_SHARE_FORMAT % int(round(share * 100.0))))

    # **BOTH HALVES ARE APPORTIONED IN ONE PASS.** This is the client's OWN arithmetic — dividing
    # whole people by a share the player chose genuinely needs rounding, unlike the brackets
    # themselves, which arrive already whole. Running it separately over each half lets both round
    # the same way and show 31 people leaving a band of 30. The chosen worker count is PINNED to the
    # integer the player picked and left out of the apportionment, so the stepper can never disagree
    # with the readout.
    var whole := _split_apportioned(band, share)
    var new_children: int = whole[0]
    var new_elders: int = whole[1]
    var kept_children: int = whole[2]
    var kept_working: int = whole[3]
    var kept_elders: int = whole[4]
    var new_people := _split_workers + new_children + new_elders

    var provisions := DetailFormat.band_provisions(band)
    sheet.add_child(HudWidgets.alloc_section_label(HudComposeVocab.SPLIT_NEW_BAND_HEADER))
    sheet.add_child(_split_row(HudComposeVocab.SPLIT_ROW_PEOPLE,
        str(new_people)))
    sheet.add_child(_split_row(HudComposeVocab.SPLIT_ROW_BRACKETS,
        HudComposeVocab.SPLIT_BRACKETS_FORMAT % [_split_workers, new_children, new_elders]))
    sheet.add_child(_split_row(HudComposeVocab.SPLIT_ROW_PROVISIONS,
        HudComposeVocab.SPLIT_STOCK_FORMAT % (provisions * share)))

    sheet.add_child(HudWidgets.alloc_section_label(HudComposeVocab.SPLIT_HOME_AFTER_HEADER))
    # **EVERY `now` IS THE TWO HALVES ADDED BACK UP, NEVER A SECOND READING OF THE BAND.** `pool` is
    # the ASSIGNABLE worker count while the `after` side comes out of the apportionment, and the two
    # readings can still drift by a body — quoting `pool` here rendered `16 → 12` beside a new band
    # of 5 and invited the player to find the missing person. Composing it from the halves makes the
    # row sum by construction.
    sheet.add_child(_split_row(HudComposeVocab.SPLIT_ROW_WORKERS,
        HudComposeVocab.SPLIT_BEFORE_AFTER_FORMAT % [
            str(kept_working + _split_workers), str(kept_working)]))
    sheet.add_child(_split_row(HudComposeVocab.SPLIT_ROW_CHILDREN,
        HudComposeVocab.SPLIT_BEFORE_AFTER_FORMAT % [
            str(kept_children + new_children), str(kept_children)]))
    sheet.add_child(_split_row(HudComposeVocab.SPLIT_ROW_ELDERS,
        HudComposeVocab.SPLIT_BEFORE_AFTER_FORMAT % [
            str(kept_elders + new_elders), str(kept_elders)]))
    sheet.add_child(_split_row(HudComposeVocab.SPLIT_ROW_PROVISIONS,
        HudComposeVocab.SPLIT_BEFORE_AFTER_FORMAT % [
            HudComposeVocab.SPLIT_STOCK_FORMAT % provisions,
            HudComposeVocab.SPLIT_STOCK_FORMAT % (provisions * (1.0 - share))]))

    # **THE FLOORS COME FROM THE SIM, THE SENTENCE IS THE CLIENT'S.** The sheet moves a stepper, so a
    # published verdict would need one field per possible composition; what crosses the wire is the
    # pair of thresholds (`SPLIT_MIN_WORKERS_KEY` / `SPLIT_PARENT_MIN_WORKERS_KEY`), never a copy of
    # the rule. Both are checked and BOTH are reported — fixing one otherwise just reveals the other.
    var blocked := split_blocked_reason(band, _split_workers, pool)
    var confirm := Button.new()
    confirm.text = HudComposeVocab.SPLIT_BAND_BUTTON
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    HudStyle.apply_button(confirm, "primary")
    confirm.disabled = blocked != ""
    confirm.tooltip_text = blocked if confirm.disabled else HudComposeVocab.SPLIT_BAND_HINT
    if not confirm.disabled:
        confirm.pressed.connect(func() -> void:
            var workers := _split_workers
            _close_party_compose()
            _on_split_band_pressed(band, workers))
    sheet.add_child(confirm)
    sheet.add_child(HudWidgets.alloc_hint_label(
        blocked if blocked != "" else HudComposeVocab.SPLIT_BAND_AFTER_NOTE))

## One `key   value` line on the split sheet — the `FactionRollup._stat_row` shape, kept local
## because the parties zone has no shared detail-row widget and one sheet does not justify minting a
## shared one.
func _split_row(key: String, value: String) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    var name_label := HudWidgets.build_field_key(key)
    name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    row.add_child(name_label)
    var value_label := Label.new()
    value_label.text = value
    value_label.add_theme_color_override("font_color", HudStyle.INK)
    row.add_child(value_label)
    return row

## **What a split may draw from: the band's WORKERS, not its idle ones.** A split is not staffing a
## job — it divides the band, and an assignment held by someone who leaves lapses with them. This is
## the same quantity the sim bounds the command by (`available_workers`), so the stepper's ceiling
## and the server's refusal cannot disagree.
func _split_worker_pool(band: Dictionary) -> int:
    return int(band.get(HudComposeVocab.SPLIT_WORKING_AGE_KEY, 0))

## Both halves' dependants as WHOLE BODIES, from one apportionment pass — see the call site for why
## it must be one. Returns `[new_children, new_elders, kept_children, kept_working, kept_elders]`.
func _split_apportioned(band: Dictionary, share: float) -> Array[int]:
    var children := float(band.get(HudComposeVocab.SPLIT_AGE_CHILDREN_KEY, 0))
    var elders := float(band.get(HudComposeVocab.SPLIT_AGE_ELDERS_KEY, 0))
    var working := float(band.get(HudComposeVocab.SPLIT_WORKING_AGE_KEY, 0))
    var people := int(band.get("size", 0))
    var parts: Array[float] = [
        children * share,
        elders * share,
        children * (1.0 - share),
        working - float(_split_workers),
        elders * (1.0 - share),
    ]
    return HudFormat.apportion_people_to(parts, people - _split_workers)

## **WHY the sim would refuse this split** — both floors, both reported.
##
## The numbers are the sim's, echoed onto the cohort; only the sentences are the client's, because the
## sim's own refusal strings are log-voice and far too long for a tooltip.
func split_blocked_reason(band: Dictionary, workers: int, pool: int) -> String:
    var lines: Array[String] = []
    var min_new := int(band.get(HudComposeVocab.SPLIT_MIN_WORKERS_KEY, 0))
    var min_parent := int(band.get(HudComposeVocab.SPLIT_PARENT_MIN_WORKERS_KEY, 0))
    if workers < min_new:
        lines.append(HudComposeVocab.SPLIT_BLOCKED_NEW_TOO_SMALL % min_new)
    var remaining := pool - workers
    if remaining < min_parent:
        lines.append(HudComposeVocab.SPLIT_BLOCKED_PARENT_TOO_SMALL % [remaining, min_parent])
    return HudComposeVocab.SPLIT_BLOCKED_SEPARATOR.join(lines)

## The HUNT form, in the order the decision is actually made: QUARRY → POLICY → PARTY → forecast →
## send. The quarry leads because it is what makes every field under it answerable — the per-policy
## metrics on the picker, the max-useful party cap, the trip forecast and the no-surplus verdict are
## all functions of the herd. Every one of those comes from the SAME helper the herd drawer's
## beyond-reach branch uses, so the two entry points cannot quote different numbers.
func _fill_hunt_compose_sheet(sheet: VBoxContainer, band: Dictionary, idle: int) -> void:
    # Re-resolve the quarry LIVE each render: a herd can be hunted out or leave the snapshot while the
    # sheet is open, and rendering a form against a stale id would forecast a herd that is gone. A herd
    # that MIGRATES into the band's hunt reach fails for the same reason — it is no longer a party's
    # job — so it falls back to the `Choose…` empty state rather than forecasting a raid the player
    # should not make.
    var herd := _band_labor.find_world_herd(_compose.party_quarry_id())
    if herd.is_empty() or not _targeting.is_expedition_quarry(band, herd):
        herd = {}
        _clear_party_quarry()
    sheet.add_child(_build_quarry_row(band, herd))
    if _compose.party_quarry_id() == "":
        # Visible-and-disabled-with-its-reason, the same convention as the idle-0 footer: the send is
        # shown so the shape of the form is legible, and it says why it is not yet pressable.
        sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_QUARRY_HINT))
        var blocked := Button.new()
        blocked.text = SourceForecast.SEND_HUNTING_EXPEDITION_BUTTON
        blocked.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        blocked.disabled = true
        blocked.tooltip_text = HudComposeVocab.COMPOSE_QUARRY_HINT
        HudStyle.apply_button(blocked, "ghost")
        sheet.add_child(blocked)
        return
    # **THE KIT, RESOLVED BEFORE ANYTHING IS QUOTED AND MOUNTED UNDER THE PARTY STEPPER.** It is part
    # of the question the sim is asked, so every figure below is priced for it — and the picker's ROW
    # belongs beneath the crew it describes, so the resolve is here and the mount is further down.
    # `party_kit_id` is shared with the denial mission (one sheet, two missions, both on the `hunt`
    # job) and re-validated every render.
    var kits := _band_labor.kits()
    var default_kit := _band_labor.default_kit_id(KitRoster.JOB_HUNT)
    # The HERD is passed so a kit this quarry cannot be worked with is never resolved onto — the
    # drawer's rule, and the same fresh-tier offer test, so the two entry points cannot open on
    # different kits for one animal.
    var kit_id := KitRoster.resolve_selection(kits, KitRoster.JOB_HUNT, default_kit,
        _compose.party_kit_id(), herd, HudComposeVocab.BARE_FORECAST_PREFIX)
    _compose.set_party_kit_id(kit_id)
    # **THE RAID'S NUMBERS ARE ASKED FOR.** The sim forward-simulates THIS band, kit, party and floor
    # and answers; there is no table quoted at one kit to gate against any more. The ask is idempotent
    # on the composed key, so only a rebuild that actually moves it re-queries.
    var raid_view := _raid_forecast_view(band, herd, kit_id, _send_expedition_count,
        _send_hunt_floor, idle)
    var raid_answer: Dictionary = raid_view["answer"]
    var raid_ready := String(raid_view["state"]) == ForecastQuery.STATE_READY
    sheet.add_child(HudWidgets.alloc_section_label(HudComposeVocab.COMPOSE_FIELD_POLICY))
    # With a herd in hand the presets finally carry their metric — the same
    # `SourceForecast.expedition_policy_takes` the herd drawer feeds its picker.
    #
    # **THE METRICS COME FROM THE SAME ANSWER AS EVERYTHING ELSE HERE** — `per_preset`, one row per
    # preset in the order they were asked for, so all four figures on the sheet are priced for the one
    # party and kit it is composing. `{}` until the reply lands, which is the picker's supported degrade
    # (as is a herd the wire does not describe), so the rungs render bare rather than wrong.
    #
    # **THREE ACROSS, the shared default** — the zone's own 2-column clamp is retired. It existed
    # because the long preset faces (`💀 Take everything`) could not fit three in a 354px column and
    # wrapped `↑ Learn from it` onto a second row; the faces are one word each now
    # (`HudComposeVocab.FLOOR_PRESET_LABELS`), so the picker reads as one row here and in the drawer.
    sheet.add_child(HudWidgets.build_floor_picker(func(floor: float) -> void:
        _send_hunt_floor = floor
        # Auto-max on a floor click, exactly as the herd drawer does: "give me everything this herd
        # can spare" — zero waste, full rate. Consumed on the next rebuild, never set by a −/+ tick.
        _compose.arm_party_autofill()
        rerender(), _send_hunt_floor,
        SourceForecast.expedition_policy_takes(band, herd, raid_answer.get("per_preset", []),
            _band_labor.grid_width(), _band_labor.wrap_horizontal())))
    # Party size, capped at the raid's max-useful plateau for THIS herd + floor (the herd drawer's
    # own cap), so extra hunters can no longer be sent to stand idle at the kill. **The SUPPLY side is
    # the band's idle workers alone** — nothing on the wire caps a party — and `expedition_useful_cap`
    # is the DEMAND side the stepper takes the tighter of.
    #
    # **THE DEMAND SIDE RIDES THE ANSWER**, so until one lands the plateau contributes 0 and supply
    # alone binds. That is the honest degrade: a party clamped to a plateau nobody has quoted yet would
    # refuse hands this raid may well need.
    #
    # **THE CAP IS RESOLVED HERE, ABOVE THE CHART, AND THE ROW IT FEEDS IS MOUNTED FURTHER DOWN.** The
    # chart's projection, its two crew targets and its verdict are all read against a CREW, so
    # composing them ahead of the clamp states a verdict for a party the stepper beneath then refuses
    # to show — visible for exactly one frame, on the render where autofill arms (a floor click, a
    # committed drag, a fresh quarry), which is the render a player is always looking at. The forage
    # sheet's twin ordering, and the assertion that judges both, are in `labor-ui.md`.
    var assignable := idle
    var capped := SourceForecast.expedition_useful_cap(band, herd, _send_hunt_floor,
        int(raid_answer.get("useful_cap", 0)), assignable)
    var cap: int = maxi(int(capped["cap"]), HudConst.WORKER_STEP)
    # It does NOT wait for the reply, for the drawer's reason: the `clampi` below re-binds the count to
    # the cap on every render, so a fill spent against the no-answer fallback still converges on the
    # reply's plateau — while holding the one-shot deadlocks a party of 0, whose question is never asked.
    if _compose.consume_party_autofill():
        _send_expedition_count = cap
    _send_expedition_count = clampi(_send_expedition_count, HudConst.WORKER_STEP, cap)
    # **THE CHART AND ITS DRAGGABLE FLOOR — the same builder and the same model the herd drawer's raid
    # uses**, because the two entry points compose one decision and had no business presenting it two
    # ways. `improvement` is `IMPROVEMENT_NONE` and the crew noun is the party's: a detached party
    # builds nothing, exactly as the drawer's expedition branch already assumes.
    #
    # **GATED ON THE ZONE HAVING ROOM.** A horizontal dock's parties zone is height-capped and CLIPS —
    # only its row LIST scrolls, and the compose sheet sits below that list — and the chart is ~150px,
    # so the SHORT tier keeps the presets alone. (The band zone took the same treatment for its own
    # outlook chart until that zone learned to scroll; this one has not, so the gate stays.) The drag
    # goes with it:
    # since slice 4b there is no plain-slider control left to keep, the chart's own floor flag IS the
    # dial (see `HudWidgets.build_floor_chart`).
    #
    # **AND THE KIT REACHES IT CLIENT-SIDE**, through `KitRoster.priced_source` — the same seam the
    # drawer's sheets use, never a second resolve. The chart is composed HERE out of the herd's own wire
    # terms, so it is the one figure on the sheet the client itself has to price for the selected kit;
    # every other number is the sim's answer to a question that already named the kit, and arrives
    # priced. The two must therefore agree by construction rather than by luck, which is why the client
    # side goes through the shared seam.
    #
    # The kit reaches the curve two ways and both are real for a raid: its CARRY scales the party's
    # throughput, and its `dispersion` scales the quarry's retreat (`advance_expeditions` resolves the
    # party's own kit and runs `HuntParty::stayers` exactly as a resident hunt does).
    var priced_herd := KitRoster.priced_source(herd, HudComposeVocab.BARE_FORECAST_PREFIX, kits,
        KitRoster.JOB_HUNT, default_kit, kit_id, band)
    var chart_model := SourceForecast.floor_chart_model(priced_herd,
        SourceForecast.SOURCE_KIND_HERD,
        HudComposeVocab.BARE_FORECAST_PREFIX, _send_hunt_floor, _send_expedition_count, HudComposeVocab.COMPOSE_FIELD_PARTY.to_lower(),
        SourceForecast.rung_lesson_known(SourceForecast.SOURCE_KIND_HERD, herd,
            HudComposeVocab.BARE_FORECAST_PREFIX, _player_knowledge()))
    if bool(chart_model.get("known", false)) and _band_zone_tier != HudWorkVocab.BAND_ZONE_TIER_SHORT:
        sheet.add_child(HudWidgets.build_floor_chart(chart_model,
            func(floor: float, committed: bool) -> void:
                _send_hunt_floor = floor
                # **ONLY A COMMITTED CHANGE REBUILDS**, the drawer's expedition rule: a rebuild frees
                # the chart and the drag in flight dies with it, and this sheet has no live-refresh
                # registry to update in place (the raid's numbers are a lookup into a table sampled at
                # five floors, so most of a drag moves nothing anyway).
                if committed:
                    _compose.arm_party_autofill()
                    rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(
        HudFormat.floor_hint(_send_hunt_floor, SourceForecast.LABOR_KIND_HUNT, true)))
    # The stepper ROW, mounted where the form reads it — under the chart the settled count above was
    # composed into, and above the kit picker the party carries.
    sheet.add_child(HudWidgets.build_party_stepper_row(_send_expedition_count, cap,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, HudConst.WORKER_STEP, cap)
            rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_OF_IDLE_FORMAT % idle))
    var cap_note := String(capped["note"])
    if cap_note != "":
        sheet.add_child(HudWidgets.alloc_hint_label(cap_note))
    _mount_kit_row(sheet, kits, KitRoster.JOB_HUNT, kit_id, default_kit, band,
        func(picked: String) -> void:
            _compose.set_party_kit_id(picked)
            rerender(),
        herd, HudComposeVocab.BARE_FORECAST_PREFIX, _send_expedition_count)
    var quarry_id := _compose.party_quarry_id()
    var confirm := Button.new()
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    confirm.set_meta(HudWidgets.SEND_HUNT_CONFIRM_META, true)
    if not raid_ready:
        # **NO ANSWER YET, OR NONE COMING** — where the kit-mismatch apology stood. Nothing derived
        # renders: no forecast line, no bound clause, no empty-raid refusal. What DOES render is
        # honest with no reply at all — the combat gate, composed from wire terms the band and herd
        # already carry — plus the line saying whether we are waiting or have failed. The send stays
        # LIVE: the raid is perfectly launchable; only its length is unquoted.
        _mount_kit_gate_line(sheet, kits, kit_id, band, herd,
            SourceForecast.herd_display_name(herd))
        sheet.add_child(HudWidgets.alloc_hint_label(
            HudComposeVocab.RAID_FORECAST_PENDING if String(raid_view["state"]) == ForecastQuery.STATE_PENDING
            else HudComposeVocab.FORECAST_FAILED_FORMAT % String(raid_view["error"])))
        SourceForecast.style_send_hunt_button(confirm, {}, "")
    else:
        # **THE TRIP READOUT — the herd drawer's boxed section, from the shared builder.** This zone
        # answered with a one-line bbcode sentence and a standalone bound clause beside it, which is
        # what let the two entry points drift: on a Wild Fowl flock the drawer laid out a full box
        # here and this sheet rendered nothing at all. The box's own VERDICT folds the bound clause
        # in (`SourceForecast.hunt_trip_verdict`), so the standalone line went with the sentence —
        # keeping both would have printed one fact twice.
        var trip := SourceForecast.hunt_trip_forecast(band, herd,
            raid_answer.get("at_composed", {}), _band_labor.grid_width(),
            _band_labor.wrap_horizontal())
        if SourceForecast.hunt_trip_delivers(trip):
            HudWidgets.mount_trip_readout(sheet, trip, SourceForecast.herd_display_name(herd),
                _send_hunt_floor)
        else:
            # A raid with nothing to lay out in rows — no estimate, a denial quarry, a herd at its
            # floor — keeps the ONE-LINE form, exactly as the drawer's branch does. An empty box is
            # worse than the sentence it would replace.
            var forecast_line := SourceForecast.hunt_forecast_line_bbcode(trip,
                SourceForecast.herd_display_name(herd))
            if forecast_line != "":
                sheet.add_child(HudWidgets.forecast_label(forecast_line))
        # WHY an empty raid is empty comes off the sim's `bound`, so the reason takes the TRIP beside
        # the herd — "wait for the herd to rebuild" and "send more hunters" are opposite instructions.
        var returns_empty := SourceForecast.hunt_trip_returns_empty(trip)
        var reason := SourceForecast.hunt_empty_refusal_reason(trip, herd) if returns_empty else ""
        # The button carries the verdict: slow/long/denial raids stay ENABLED and warn-styled, and only
        # a herd with no surplus disables. `style_send_hunt_button` owns the text in every branch.
        SourceForecast.style_send_hunt_button(confirm, trip, reason)
        if returns_empty:
            sheet.add_child(HudWidgets.alloc_hint_label(reason))
    confirm.pressed.connect(func() -> void:
        emit_signal("send_hunt_expedition_requested", {
            "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
            "party_workers": _send_expedition_count,
            "fauna_id": quarry_id,
            "fauna_label": SourceForecast.herd_display_name(herd),
            "floor": _send_hunt_floor,
            # The kit the party walks out with, and the job default `Main` omits the token for.
            "kit_id": kit_id,
            "default_kit_id": default_kit,
        })
        _close_party_compose())
    sheet.add_child(confirm)

## Mount the kit row where a sheet wants it — a no-op when the roster offers this job no kit at all,
## so a sheet rendered before the first snapshot (or against a world whose roster does not cover the
## verb) is byte-identical to what it was before the picker existed.
##
## `quarry` / `prefix` are what a kit's greying is resolved against (`KitRoster.kit_offer`); both
## dock missions have a herd in hand by the time this is reached.
## **`crew` IS THE PARTY STEPPER ABOVE IT**, handed on so the hint can state how far the band's gear
## reaches into the party being composed. Both dock missions have one; a caller with none keeps the
## pre-clause line.
func _mount_kit_row(sheet: VBoxContainer, kits: Array, job: String, kit_id: String,
        default_kit: String, band: Dictionary, on_pick: Callable, quarry: Dictionary = {},
        prefix: String = "", crew: int = KitRoster.KIT_CREW_UNCOMPOSED) -> void:
    var row := KitRoster.build_kit_row(kits, job, kit_id, default_kit, band, on_pick, quarry, prefix,
        HudComposeVocab.COMPOSE_FIELD_KIT, false, crew)
    if row != null:
        sheet.add_child(row)

## **THE ONE FORECAST THAT STAYS HONEST FOR EVERY KIT**, rendered where the estimate tables have been
## suppressed. It is composed from wire terms — `max(0, attack − defense)` against the species'
## durability — at the SELECTED kit's effective attack rather than at the band's default-kit tier, so
## a bare-handed party against a defended species reads the plain refusal instead of a blank sheet.
## Same ink and same meta as the herd drawer's gate line, so the two surfaces cannot state one fight
## two ways.
func _mount_kit_gate_line(sheet: VBoxContainer, kits: Array, kit_id: String, band: Dictionary,
        herd: Dictionary, quarry: String) -> void:
    var gate := SourceForecast.hunt_gate_model_at(KitRoster.effective_attack_against(
        kits, KitRoster.kit_by_id(kits, kit_id), band,
        float(herd.get(KitRoster.QUARRY_BODY_MASS_KEY, 0.0))), herd, quarry)
    # **ONLY THE REFUSAL RENDERS.** The winnable branch used to state the effort in hunter-turns; that
    # face is retired (a species constant beside a forecast that already prices the trip), so a fight
    # this party CAN take says nothing here and the sheet's remaining lines are the answer.
    #
    # **…EXCEPT WHEN THE PARTY IS SPLIT** (issue #520). The gate answers at ONE tier and on a
    # partly-equipped band that tier is the best-armed crew's, so a cleared gate here is the
    # reassuring half. Same complement, same builder as the herd drawer's line.
    #
    # **ASKED ABOUT `_send_expedition_count`, because BOTH sheets that mount this line have a party
    # stepper** — the hunting-party form and the denial form. The gear covers a prefix of whoever is
    # sent, so quoting the band's whole hunt roster here would name more bare hands than the party
    # has people the moment the party is smaller than the armed run.
    if not bool(gate["blocked"]):
        HudWidgets.mount_hunt_crew_split(sheet, band, herd, quarry, kit_id, _send_expedition_count)
        return
    var gate_label := HudWidgets.forecast_label("[color=#%s]%s[/color]" % [
        HudStyle.DANGER_HEX, String(gate["text"])])
    gate_label.set_meta(HudWidgets.HUNT_GATE_META, true)
    sheet.add_child(gate_label)

## The DENIAL form (`docs/plan_denial_raid.md` §3): QUARRY → PARTY → the collapse verdict → send.
##
## **WHAT IS ABSENT IS THE SPECIFICATION.** No floor picker, no floor hint, no fill target, no crew
## preset, no max-useful cap — a denial party never stops engaging, so there is no escapement to dial
## and no pack to fill, and any of those controls would be a lever the command grammar
## (`send_denial_raid`, closed at four tokens) cannot even carry. The player chooses a herd and a
## party size; everything else on this sheet is a READOUT.
##
## The quarry row and its picker are the hunt form's, reused verbatim. **THE BEYOND-REACH RULE IS
## NOT**, and this is the one place the two missions genuinely differ about what a quarry is
## (`TargetingController.is_expedition_quarry`): a hunting party exists for game the band cannot work
## from home, so a nearer herd is a local hunt — but denial is not a way of GETTING food, it is a way
## of ERASING a herd, and hunting the warren next door at floor 0 cannot express that (a hunt is
## carry-bounded and stops at the pack). A denial raid may therefore name any herd the band can see
## and reach. It is still an EXPEDITION and deliberately not a labor assignment: the party detaches,
## spends turns killing and comes back, and it has no floor and no rate to put on the assign dialog.
func _fill_denial_compose_sheet(sheet: VBoxContainer, band: Dictionary, idle: int) -> void:
    # Re-resolved LIVE every render for the hunt form's reasons: a herd can be raided out or leave the
    # snapshot while the sheet is open, and a form rendered against a stale id would forecast a
    # collapse for a herd that is gone. **A herd that MIGRATES INTO REACH no longer clears the form** —
    # under denial that was never a reason to drop it.
    var herd := _band_labor.find_world_herd(_compose.party_quarry_id())
    if herd.is_empty() or not _targeting.is_expedition_quarry(band, herd,
            HudComposeVocab.COMPOSE_MISSION_DENY):
        herd = {}
        _clear_party_quarry()
    sheet.add_child(_build_quarry_row(band, herd))
    if _compose.party_quarry_id() == "":
        # Visible-and-disabled-with-its-reason, the footer's own convention.
        sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_DENY_QUARRY_HINT))
        var blocked := Button.new()
        blocked.text = String(SourceForecast.DENIAL_VERDICTS[
            SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY]["button"])
        blocked.size_flags_horizontal = Control.SIZE_EXPAND_FILL
        blocked.disabled = true
        blocked.tooltip_text = HudComposeVocab.COMPOSE_DENY_QUARRY_HINT
        HudStyle.apply_button(blocked, "ghost")
        sheet.add_child(blocked)
        return
    # **THE PARTY IS CAPPED BY THE BAND'S OWN IDLE WORKERS, AND BY NOTHING ELSE.** There is
    # deliberately no `expedition_useful_cap` twin here: that cap exists because a hunting raid's
    # delivered payload PLATEAUS once the herd's surplus binds, and a denial raid has no payload to
    # plateau. More hands always break the herd sooner, which is the whole lever this form offers.
    #
    # **`max_expedition_party_size` IS NOT A RULES CAP AND MUST NOT BE APPLIED HERE.** It echoed how far
    # the retired pre-launch estimate tables had been sampled, and nothing in the client reads it any
    # more: the sim answers the party this sheet composed, over a search bounded by the band's own idle
    # workforce (`max_party_workers`), and it holds no rules cap on any of the three launch verbs. The
    # client's own clamp was the last thing enforcing one — a band with 16 idle workers was held at 8
    # while this very sheet told it to send more hunters. All three launch forms read the supply the
    # same way now, which is why the `_scout_party_max` helper no longer exists.
    var party_max := idle
    # **SEEDED ON THE SIM'S OWN REQUIREMENT, ONCE PER QUARRY.** Below the reply's `party_needed` a raid
    # accomplishes literally nothing however long it runs, and nothing else on the sheet said which
    # number crossed that line — so the stepper opens there rather than on a guess. The one-shot is
    # the hunt form's `arm_party_autofill` (armed by `TargetingController.choose_quarry`, the ONE
    # adoption of a quarry on either route), so a manual −/+ tick survives every later rerender.
    #
    # **NEVER SEEDED TO 0.** `DENIAL_PARTY_NEEDED_NONE` means the sim quotes no party that drives this
    # herd down at all — it is not "send nobody" — so the count is left where it was and the verdict
    # line carries the answer. And the clamp to `party_max` is deliberate: a requirement ABOVE the
    # band's idle workers opens on the most it can field, which is honest, because the sheet shows
    # both numbers and the verdict still says it is not enough.
    # **THE QUESTION IS COMPOSED BEFORE THE STEPPER, because the stepper's SEED is part of the
    # answer.** The kit row is mounted below (a kit describes the party, so it reads under it), but the
    # kit is resolved here so the ask can carry it — resolving it twice is how the sheet would come to
    # ask about one kit and render another.
    var deny_kits := _band_labor.kits()
    var deny_default_kit := _band_labor.default_kit_id(KitRoster.JOB_HUNT)
    # **RESOLVED AGAINST THE QUARRY, exactly as the row below it is mounted.** `resolve_selection`
    # skips a kit this animal withholds, so asking without the herd can settle on a kit the picker
    # then greys out — the sheet would ask the sim about one kit and offer another.
    var deny_kit_id := KitRoster.resolve_selection(deny_kits, KitRoster.JOB_HUNT, deny_default_kit,
        _compose.party_kit_id(), herd, HudComposeVocab.BARE_FORECAST_PREFIX)
    _compose.set_party_kit_id(deny_kit_id)
    var deny_view := _denial_forecast_view(band, herd, deny_kit_id, _send_expedition_count, idle)
    var deny_answer: Dictionary = deny_view["answer"]
    var deny_ready := String(deny_view["state"]) == ForecastQuery.STATE_READY
    var party_needed := SourceForecast.denial_party_needed(deny_answer)
    # **THE SEED WAITS FOR THE ANSWER IT IS MADE OF** — `party_needed` is the reply's, and the render
    # that ARMS the one-shot (adopting a quarry, opening the sheet) is the render that has just asked.
    # Spending it there consumed the seed against a `party_needed` of 0 and the sheet opened on the
    # stepper's floor instead of on the party the sim quotes. `ForecastQuery.answer_settled` holds the
    # rule; a refusal counts as settled, so a dead socket cannot leave the seed armed for the act.
    #
    # **THIS IS THE SEAM THAT CAN WAIT, AND THE TWO HUNT SHEETS ARE NOT.** A requirement is not a cap:
    # nothing below re-applies it, so a seed spent early is simply lost, where a hunt fill is re-clamped
    # to the cap every render and converges anyway. Waiting is safe here because the party stepper never
    # sits at 0 (`HudConst.WORKER_STEP` is its floor and its initial value), so the question is always
    # asked and the answer always settles — which is exactly the condition a hunt sheet cannot meet.
    if ForecastQuery.answer_settled(deny_view) and _compose.consume_party_autofill():
        if party_needed > SourceForecast.DENIAL_PARTY_NEEDED_NONE:
            _send_expedition_count = clampi(party_needed, HudConst.WORKER_STEP, party_max)
    _send_expedition_count = clampi(_send_expedition_count, HudConst.WORKER_STEP, party_max)
    sheet.add_child(HudWidgets.build_party_stepper_row(_send_expedition_count, party_max,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, HudConst.WORKER_STEP, party_max)
            rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_OF_IDLE_FORMAT % idle))
    var quarry_name := SourceForecast.herd_display_name(herd)
    # **THE KIT, DIRECTLY UNDER THE PARTY IT DESCRIBES.** It is the only order this closed-grammar
    # mission still has to give besides the party size, and it moves every figure below it — a `none`
    # raid against a defended species has an effective attack of ZERO and no party size works at all.
    var kits := deny_kits
    var kit_id := deny_kit_id
    # **A DENIAL RAID IS STILL A FIGHT, so the offer test applies to it unchanged.** Erasing a herd
    # you cannot hurt is not a mission — it is the same zero take with a different name on it, so the
    # quarry and its forecast prefix ride through to `KitRoster.kit_offer`'s greying exactly as they
    # do on the hunt form.
    _mount_kit_row(sheet, kits, KitRoster.JOB_HUNT, kit_id, deny_default_kit, band,
        func(picked: String) -> void:
            _compose.set_party_kit_id(picked)
            rerender(),
        herd, HudComposeVocab.BARE_FORECAST_PREFIX, _send_expedition_count)
    var confirm := Button.new()
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    confirm.set_meta(HudWidgets.SEND_DENIAL_CONFIRM_META, true)
    var reason := ""
    if not deny_ready:
        # **NO ANSWER YET, OR NONE COMING** — where the kit-mismatch apology stood. What survives is the
        # combat GATE, composed from wire terms and therefore honest with no reply at all, plus the
        # line saying whether we are waiting or have failed. The send stays live and plainly styled:
        # the raid launches; we simply cannot say yet how long it takes.
        _mount_kit_gate_line(sheet, kits, kit_id, band, herd, quarry_name)
        sheet.add_child(HudWidgets.alloc_hint_label(
            HudComposeVocab.DENIAL_FORECAST_PENDING if String(deny_view["state"]) == ForecastQuery.STATE_PENDING
            else HudComposeVocab.FORECAST_FAILED_FORMAT % String(deny_view["error"])))
        SourceForecast.style_send_denial_button(confirm, {}, false)
    else:
        # THE COLLAPSE VERDICT — the reply's row for this party size, on the clock the player is on.
        # **The band and the grid pair are passed for the OUTBOUND WALK**: the row counts raiding
        # turns, and this sheet's hunt form has always headlined a round-trip total, so
        # a verdict quoting bare raiding turns beside it named a shorter span in the same words.
        var forecast := SourceForecast.denial_forecast(herd, deny_answer.get("at_composed", {}),
            band, _band_labor.grid_width(), _band_labor.wrap_horizontal())
        var verdict := SourceForecast.denial_verdict_bbcode(forecast, quarry_name)
        if verdict != "":
            sheet.add_child(HudWidgets.forecast_label(verdict))
            # The caveat rides under the verdict WHENEVER THERE IS A NUMBER TO CAVEAT — the band is an
            # integral over many stochastic draws and a lucky run really can finish sooner than the
            # reported low. A verdict with no turn count (a repelled party, an unbounded horizon) has
            # nothing for it to qualify, and a caveat about an absent number reads as one that is there.
            if SourceForecast.denial_turns_phrase(forecast) != "":
                sheet.add_child(HudWidgets.alloc_hint_label(SourceForecast.DENIAL_ESTIMATE_CAVEAT))
        # …and the take beneath it: what the raid kills, what little it hauls, and what it leaves on
        # the range. Quiet ink — the waste IS the mission, not a warning about it.
        var take := SourceForecast.denial_take_bbcode(forecast, quarry_name)
        if take != "":
            sheet.add_child(HudWidgets.forecast_label(take))
        # **THE SHORT-HANDED SENTENCE SUPERSEDES THE REFUSAL, it does not join it.** Both name the
        # party the sim quotes (one reading, `denial_party_needed`), so printing the pair would state
        # the requirement twice; the short-handed form also says what the band actually has.
        var short_handed := SourceForecast.denial_is_short_handed(party_needed, idle)
        reason = SourceForecast.denial_short_handed_reason(herd, party_needed, idle)
        if reason == "":
            reason = SourceForecast.denial_refusal_reason(forecast, herd, party_needed)
        if reason != "":
            sheet.add_child(HudWidgets.alloc_hint_label(reason))
        # The button carries the verdict, and disables in EXACTLY ONE case — a band that cannot field
        # the party this herd requires at all. A party the player CHOSE to under-size still launches:
        # it works the herd until recalled, so that case warns and the player is trusted.
        SourceForecast.style_send_denial_button(confirm, forecast, short_handed)
    confirm.tooltip_text = reason if reason != "" else HudComposeVocab.SEND_DENIAL_RAID_HINT
    var quarry_id := _compose.party_quarry_id()
    confirm.pressed.connect(func() -> void:
        emit_signal("send_denial_raid_requested", {
            "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
            "party_workers": _send_expedition_count,
            "fauna_id": quarry_id,
            "fauna_label": quarry_name,
            # The party's kit, and the job default `Main` omits the `kit <id>` token for — the only
            # order the four-token grammar admits beyond the two it already carries.
            "kit_id": kit_id,
            "default_kit_id": deny_default_kit,
        })
        _close_party_compose())
    sheet.add_child(confirm)

## **THE SHIPMENT FORM** (arc #527, issue #517): DESTINATION → PARTY → CARGO → the mass meter → send.
##
## **IT SHARES NO FIELD WITH THE HUNT FORM, which is why it is a mission and not a mode of one.** No
## quarry, no floor, no policy picker, no trip forecast: what a shipment needs to know is who it is
## for, how many hands carry it and what goes in the packs. Every one of those is a control the hunt
## grammar has nowhere to put, and every hunt control is a lever `send_trade_expedition` cannot carry.
##
## **THE TIE IS THE GATE, AND THE FORM TEACHES IT.** A destination is a band this one holds a live
## connection with; a PARKED tie (strength 0 — "we know such a people exist and have no current
## dealings") is listed DISABLED with that as its reason rather than hidden, because the thing the
## player has to learn is that the tie is what gates trade, not that some bands are missing.
##
## **THE DESTINATION IS REMEMBERED, NEVER SEEN.** A connection can only ever grant `Discovered`
## (`.claude/rules/core_sim/connections.md` → the keystone), so the position under the picker is
## where they WERE and the walk quoted from it wears a `≈`. A remembered band behaves exactly like a
## remembered herd, which the player has already been taught by every herd that moved.
##
## **THE MASS METER IS A COURTESY, NOT THE AUTHORITY.** `send_trade_expedition` refuses an over-cap
## manifest and its refusal names both numbers; this meter exists so the player never meets it. Both
## terms come off the wire (`expedition_trade_per_worker_carry`,
## `expedition_trade_material_carry_weight`) — a lever typed here would be one config edit from a
## meter that disagrees with the refusal it exists to prevent.
func _fill_trade_compose_sheet(sheet: VBoxContainer, band: Dictionary, idle: int) -> void:
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    var ties := _band_labor.connections_for_band(band_id)
    sheet.add_child(_build_destination_row(band, ties))
    if ties.is_empty():
        # Visible-and-disabled-with-its-reason, the hunt form's own convention for a form whose first
        # question has no answer yet. The sentence says how a tie FORMS, because that is the action
        # the player has to take and no control on this sheet can take it for them.
        sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_DESTINATION_NO_TIES))
        sheet.add_child(_blocked_send_button(HudComposeVocab.SEND_TRADE_EXPEDITION_BUTTON,
            HudComposeVocab.COMPOSE_DESTINATION_NO_TIES))
        return
    # Re-resolve the chosen tie LIVE each render, the hunt form's rule: a tie decays, and a band that
    # was a destination when the sheet opened can be parked by the time it is sent. A form rendered
    # against a stale choice would quote a walk to a band nothing can flow to.
    var tie := _live_trade_tie(ties)
    if tie.is_empty():
        sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_DESTINATION_HINT))
        sheet.add_child(_blocked_send_button(HudComposeVocab.SEND_TRADE_EXPEDITION_BUTTON,
            HudComposeVocab.COMPOSE_DESTINATION_HINT))
        return
    for line in _trade_destination_notes(band, tie):
        sheet.add_child(HudWidgets.alloc_hint_label(line))
    # **THE PARTY IS THE CAP'S OTHER TERM**, so it is settled before the manifest is priced — the
    # "resolve the cap above the readout" ordering all three compose sheets follow. Its ceiling is the
    # band's IDLE WORKERS and nothing else: the sim carries no rules cap on party size, and a
    # shipment's own bound is the mass meter below rather than a head count.
    var party_max: int = maxi(idle, HudConst.WORKER_STEP)
    _send_expedition_count = clampi(_send_expedition_count, HudConst.WORKER_STEP, party_max)
    sheet.add_child(HudWidgets.build_party_stepper_row(_send_expedition_count, party_max,
        func(n: int) -> void:
            _send_expedition_count = clampi(n, HudConst.WORKER_STEP, party_max)
            rerender()))
    sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_OF_IDLE_FORMAT % idle))
    sheet.add_child(HudWidgets.alloc_section_label(HudComposeVocab.COMPOSE_CARGO_SECTION))
    var rows := _trade_cargo_rows(band)
    if rows.is_empty():
        sheet.add_child(HudWidgets.alloc_hint_label(HudComposeVocab.COMPOSE_CARGO_NO_STORES))
        sheet.add_child(_blocked_send_button(HudComposeVocab.SEND_TRADE_EXPEDITION_BUTTON,
            HudComposeVocab.COMPOSE_CARGO_NO_STORES))
        return
    for row_variant in rows:
        sheet.add_child(_build_cargo_row(row_variant as Dictionary))
    var mass := _trade_manifest_mass(band, rows)
    var cap := _trade_carry_cap(band)
    sheet.add_child(_build_mass_meter(mass, cap))
    var reason := ""
    if mass <= 0.0:
        reason = HudComposeVocab.COMPOSE_CARGO_EMPTY_REASON
    elif cap > 0.0 and mass > cap:
        reason = HudComposeVocab.COMPOSE_CARGO_OVER_CAP_REASON
    if reason != "":
        sheet.add_child(HudWidgets.alloc_hint_label(reason))
        sheet.add_child(_blocked_send_button(HudComposeVocab.SEND_TRADE_EXPEDITION_BUTTON, reason))
        return
    var confirm := Button.new()
    confirm.text = HudComposeVocab.SEND_TRADE_EXPEDITION_BUTTON
    confirm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    confirm.tooltip_text = HudComposeVocab.SEND_TRADE_EXPEDITION_HINT
    confirm.set_meta(HudWidgets.SEND_TRADE_CONFIRM_META, true)
    HudStyle.apply_button(confirm, "primary")
    var destination_band := int(tie.get("subject_band_id", HudConst.NO_BAND_ID))
    var destination_label := _connection_subject_label(tie)
    confirm.pressed.connect(func() -> void:
        emit_signal("send_trade_expedition_requested", {
            "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": band_id,
            "party_workers": _send_expedition_count,
            # The KEY the command addresses, and beside it the string the feed note renders — the
            # `fauna_id` / `fauna_label` pairing, for the same reason: a raw id is a database handle.
            "destination_band_id": destination_band,
            "destination_label": destination_label,
            "cargo": _trade_manifest_lines(rows),
        })
        _close_party_compose())
    sheet.add_child(confirm)

## A send that cannot be pressed, showing its own reason — the "visible and disabled with its reason"
## convention this zone uses everywhere, in one place because the shipment form reaches it from four
## different dead ends (no ties, no destination, no stores, an unsendable manifest).
func _blocked_send_button(face: String, reason: String) -> Button:
    var blocked := Button.new()
    blocked.text = face
    blocked.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    blocked.disabled = true
    blocked.tooltip_text = reason
    blocked.set_meta(HudWidgets.SEND_TRADE_CONFIRM_META, true)
    HudStyle.apply_button(blocked, "ghost")
    return blocked

## The DESTINATION row — the Band and Kit rows' shape, and a genuine `OptionButton` rather than the
## quarry row's map-pick button: a tie is a row in a list the sim publishes, so the candidates ARE
## enumerable and a dropdown promises exactly what it delivers.
func _build_destination_row(band: Dictionary, ties: Array) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    row.add_child(HudWidgets.build_field_key(HudComposeVocab.COMPOSE_FIELD_DESTINATION))
    var entries: Array = []
    var selected_index := HudWidgets.NO_ENTRY_SELECTED
    var face := HudComposeVocab.COMPOSE_DESTINATION_CHOOSE
    for tie_variant in ties:
        var tie: Dictionary = tie_variant as Dictionary
        var subject := int(tie.get("subject_band_id", HudConst.NO_BAND_ID))
        var label := _connection_subject_label(tie)
        # **A PARKED TIE IS AN ENTRY, DISABLED, CARRYING ITS REASON IN ITS OWN LABEL** — the kit
        # picker's convention for an unavailable choice, and the one that teaches the rule here.
        var parked := not _tie_is_live(tie)
        if parked:
            entries.append({
                "label": HudComposeVocab.COMPOSE_DESTINATION_ENTRY_PARKED_FORMAT % [
                    label, HudComposeVocab.COMPOSE_DESTINATION_PARKED_REASON],
                "disabled": true,
                "tooltip": HudComposeVocab.COMPOSE_DESTINATION_PARKED_REASON,
            })
            continue
        if subject == _trade_destination_band:
            selected_index = entries.size()
            face = label
        entries.append({
            "label": label,
            "on_pick": func() -> void:
                _trade_destination_band = subject
                rerender(),
        })
    row.add_child(HudWidgets.build_option_picker(entries, selected_index, face,
        HudComposeVocab.COMPOSE_DESTINATION_HINT))
    return row

## **A LIVE TIE IS ONE WITH STRENGTH ABOVE ZERO** — the sim's own gate (`strength > NO_TIE`), read in
## one place so the picker's greying, the live re-resolve and the send can never disagree about which
## destinations a shipment may name.
func _tie_is_live(tie: Dictionary) -> bool:
    return float(tie.get("strength", 0.0)) > HudConst.TIE_STRENGTH_NONE

## The chosen tie, re-read out of THIS render's rows — `{}` when nothing is chosen or when the choice
## has since parked or been reaped.
func _live_trade_tie(ties: Array) -> Dictionary:
    if _trade_destination_band == HudConst.NO_BAND_ID:
        return {}
    for tie_variant in ties:
        var tie: Dictionary = tie_variant as Dictionary
        if int(tie.get("subject_band_id", HudConst.NO_BAND_ID)) == _trade_destination_band \
                and _tie_is_live(tie):
            return tie
    return {}

## **WHAT IS KNOWN ABOUT WHERE THEY ARE, WORDED AS SOMETHING REMEMBERED.** One line for the sighting
## the tie recorded, and — only where the band publishes a move rate and both tiles are known — one
## for the approximate walk. Never a live position and never a bare number of turns.
func _trade_destination_notes(band: Dictionary, tie: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    var x := int(tie.get("last_seen_x", -1))
    var y := int(tie.get("last_seen_y", -1))
    if x < 0 or y < 0:
        return lines
    lines.append(HudComposeVocab.COMPOSE_DESTINATION_REMEMBERED_FORMAT % [
        x, y, int(tie.get("last_seen_turn", 0))])
    # **ONE DEFINITION OF TRAVEL IN THIS CLIENT.** `outbound_travel_turns` is the walk OUT, the same
    # reading the denial verdict takes, asked here about the REMEMBERED tile — which is why the
    # sentence says "if they are still there" rather than quoting the arrival as a fact.
    var out_turns := SourceForecast.outbound_travel_turns(band, {"x": x, "y": y},
        _band_labor.grid_width(), _band_labor.wrap_horizontal())
    if out_turns > 0:
        lines.append(HudComposeVocab.COMPOSE_DESTINATION_ETA_FORMAT % out_turns)
    return lines

## **THE NAME A TIE'S SUBJECT IS SHOWN UNDER.** A band this faction still holds in its roster is named
## exactly as the cycler, the band picker and the event dock name it — one band, one name across every
## surface. A subject the roster cannot resolve is a band we only REMEMBER, so it is named by where it
## was: the raw `BandId` is a database key and never reaches a player-facing label.
func _connection_subject_label(tie: Dictionary) -> String:
    var label := _band_labor.band_label_for_id(
        int(tie.get("subject_band_id", HudConst.NO_BAND_ID)))
    if label != "":
        return label
    return HudComposeVocab.COMPOSE_DESTINATION_REMEMBERED_LABEL_FORMAT % [
        int(tie.get("last_seen_x", -1)), int(tie.get("last_seen_y", -1))]

## **THE MANIFEST'S ROWS, ONE PER THING THE BAND ACTUALLY HOLDS** — the food larder as one row (one
## commodity), then one row per MATERIAL BATCH, which is one pile of one material AT ONE RATING.
##
## **A BATCH IS NEVER MERGED WITH ANOTHER OF THE SAME MATERIAL.** A mammoth hide and a hare pelt are
## both `hide`; a row that summed them would offer the player a quantity of something that does not
## exist, and it would be the retired trade scalar rebuilt out of the vector that replaced it.
func _trade_cargo_rows(band: Dictionary) -> Array:
    var rows: Array = []
    var held_food := DetailFormat.band_provisions(band)
    if held_food >= SourceForecast.FOOD_FLOW_MIN:
        rows.append({
            "key": TRADE_FOOD_ROW_KEY,
            "is_material": false,
            "id": HudConst.STORE_ITEM_PROVISIONS,
            "label": HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL,
            "held": held_food,
            "amount": minf(_trade_food, held_food),
        })
    for batch_variant in band.get(HudCraftingVocab.BAND_MATERIAL_BATCHES_KEY, []):
        if not (batch_variant is Dictionary):
            continue
        var batch: Dictionary = batch_variant
        var material_id := String(batch.get(HudCraftingVocab.BATCH_MATERIAL_ID_KEY, "")).strip_edges()
        var held := float(batch.get(HudCraftingVocab.BATCH_AMOUNT_KEY, 0.0))
        if material_id == "" or not SourceForecast.has_component(held):
            continue
        var key := _trade_batch_key(batch)
        rows.append({
            "key": key,
            "is_material": true,
            "id": material_id,
            "label": _trade_material_label(batch),
            "held": held,
            "amount": minf(float(_trade_materials.get(key, 0.0)), held),
        })
    return rows

## A batch's identity across renders: the material AND the rating it is held at, which is exactly how
## the sim's own store keys it (a `BTreeMap` of `(material, rating band)`). The stepper's amount is
## remembered under this, so a snapshot that moves a pile's size cannot silently move the player's
## choice onto a different pile.
func _trade_batch_key(batch: Dictionary) -> String:
    var parts: Array[String] = [String(batch.get(HudCraftingVocab.BATCH_MATERIAL_ID_KEY, ""))]
    for reading_variant in batch.get(HudCraftingVocab.BATCH_READINGS_KEY, []):
        if reading_variant is Dictionary:
            parts.append(String((reading_variant as Dictionary).get(
                HudCraftingVocab.READING_BAND_NAME_KEY, "")))
    return TRADE_BATCH_KEY_SEPARATOR.join(parts)

## `hide · tough: excellent` — **THE RATING IS WHAT MAKES THE ROW MEAN ANYTHING.** The readings are
## the band's own, in the material's declared axis order, spelled with the Crafting panel's keys so
## a pile reads the same wherever it is quoted. A material with no readings reads as its bare id.
func _trade_material_label(batch: Dictionary) -> String:
    var terms: Array[String] = []
    for reading_variant in batch.get(HudCraftingVocab.BATCH_READINGS_KEY, []):
        if not (reading_variant is Dictionary):
            continue
        var reading: Dictionary = reading_variant
        terms.append(HudComposeVocab.COMPOSE_CARGO_READING_FORMAT % [
            String(reading.get(HudCraftingVocab.READING_AXIS_KEY, "")),
            String(reading.get(HudCraftingVocab.READING_BAND_NAME_KEY, ""))])
    var material_id := String(batch.get(HudCraftingVocab.BATCH_MATERIAL_ID_KEY, ""))
    if terms.is_empty():
        return material_id
    return HudComposeVocab.COMPOSE_CARGO_MATERIAL_FORMAT % [material_id,
        HudComposeVocab.COMPOSE_CARGO_READING_SEPARATOR.join(terms)]

## One manifest row: what it is, how much of it is loaded, and how much the band still holds. The
## `+` steps by a whole unit and CLAMPS TO THE PILE, so a 0.6 pile is reachable in one press rather
## than being unshippable for want of a fractional control.
func _build_cargo_row(row: Dictionary) -> HBoxContainer:
    var line := HBoxContainer.new()
    line.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    var name_label := Label.new()
    name_label.text = String(row.get("label", ""))
    name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # **ELLIPSIS, NOT A FLUSH CLIP, AND NEVER A DROPPED AXIS.** A rating vector is verbose by nature
    # (`hide · tough: excellent · supple: poor`) and this row will not get wider, so it has to be
    # SHORTENED — but shortening the STRING is the only thing allowed: the axes are the whole reason a
    # hide and a pelt are different rows, so the underlying label always carries every one of them and
    # the tooltip below states them in full. `clip_text` alone cut mid-word with no mark
    # (`bone · dense: excellent · long: fa`), which reads as a broken label rather than a shortened
    # one; `OVERRUN_TRIM_ELLIPSIS` says "there is more" and the hover says what.
    name_label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
    name_label.add_theme_color_override("font_color", HudStyle.INK)
    # **THE FACE CLIPS AND THE TOOLTIP DOES NOT.** A pile's full rating (`hide · tough: excellent ·
    # supple: poor`) is wider than a 354px dock column, so the row ellipses — and the whole row,
    # rating included, is repeated in the hover text beside what the band still holds. Nothing is
    # unreachable; the narrow surface just says the first axis first.
    HudWidgets.set_label_tooltip(name_label, HudComposeVocab.COMPOSE_CARGO_TOOLTIP_FORMAT % [
        String(row.get("label", "")),
        HudComposeVocab.COMPOSE_CARGO_HELD_FORMAT
            % (HudCraftingVocab.BATCH_AMOUNT_FORMAT % float(row.get("held", 0.0)))])
    line.add_child(name_label)
    var held := float(row.get("held", 0.0))
    var amount := float(row.get("amount", 0.0))
    var key := String(row.get("key", ""))
    var is_material := bool(row.get("is_material", false))
    var minus := Button.new()
    minus.text = HudWorkVocab.STEPPER_MINUS_FACE
    minus.custom_minimum_size = Vector2(HudWorkVocab.WORKER_STEPPER_BUTTON_WIDTH, 0)
    minus.disabled = amount <= 0.0
    HudStyle.apply_button(minus, "ghost")
    minus.pressed.connect(func() -> void:
        _set_cargo_amount(key, is_material, amount - HudComposeVocab.COMPOSE_CARGO_STEP, held))
    line.add_child(minus)
    var value := Label.new()
    value.text = HudCraftingVocab.BATCH_AMOUNT_FORMAT % amount
    value.custom_minimum_size = Vector2(HudWorkVocab.WORKER_STEPPER_VALUE_WIDTH, 0)
    value.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
    value.add_theme_color_override("font_color",
        HudStyle.INK if amount > 0.0 else HudStyle.INK_FAINT)
    line.add_child(value)
    var plus := Button.new()
    plus.text = HudWorkVocab.STEPPER_PLUS_FACE
    plus.custom_minimum_size = Vector2(HudWorkVocab.WORKER_STEPPER_BUTTON_WIDTH, 0)
    plus.disabled = amount >= held
    HudStyle.apply_button(plus, "ghost")
    plus.pressed.connect(func() -> void:
        _set_cargo_amount(key, is_material, amount + HudComposeVocab.COMPOSE_CARGO_STEP, held))
    line.add_child(plus)
    return line

## Load `amount` of one row, clamped to what the band holds. The food row is one commodity and so one
## number; every material row is remembered under its own batch key.
func _set_cargo_amount(key: String, is_material: bool, amount: float, held: float) -> void:
    var loaded := clampf(amount, 0.0, held)
    if is_material:
        _trade_materials[key] = loaded
    else:
        _trade_food = loaded
    rerender()

## What the composed manifest weighs, through the ONE shared expression
## (`DetailFormat.shipment_mass`) — the in-flight `Carrying:` row prices the same pack with it, so the
## meter a player sends against and the row they watch afterwards cannot disagree. This half only
## splits the sheet's mixed row list into the two accounts that expression takes.
func _trade_manifest_mass(band: Dictionary, rows: Array) -> float:
    var food := 0.0
    var material_total := 0.0
    for row_variant in rows:
        var row: Dictionary = row_variant as Dictionary
        var amount := float(row.get("amount", 0.0))
        if bool(row.get("is_material", false)):
            material_total += amount
        else:
            food += amount
    return DetailFormat.shipment_mass(food, material_total,
        float(band.get("expedition_trade_material_carry_weight", 0.0)))

## `party_workers × expedition_trade_per_worker_carry` — the SHIPMENT pack, never the hunt one. A band
## publishing no lever answers 0, which the meter renders as an unknown ceiling rather than as a cap
## of zero that would refuse every manifest.
func _trade_carry_cap(band: Dictionary) -> float:
    return float(_send_expedition_count) \
        * float(band.get("expedition_trade_per_worker_carry", 0.0))

## The live mass meter — `Mass ▰▰▰▱▱ 30.0 / 40.0`, tinted DANGER once the manifest is over the cap the
## server will refuse it at.
func _build_mass_meter(mass: float, cap: float) -> Label:
    var meter := Label.new()
    var filled := clampf(mass / cap, 0.0, 1.0) * HudConst.PROGRESS_PERCENT_SCALE if cap > 0.0 else 0.0
    meter.text = "%s %s" % [HudComposeVocab.COMPOSE_CARGO_MASS_LABEL,
        HudComposeVocab.COMPOSE_CARGO_MASS_FORMAT % [
            HudFormat.meter_bar(filled, HudComposeVocab.COMPOSE_CARGO_MASS_CELLS),
            HudCraftingVocab.BATCH_AMOUNT_FORMAT % mass,
            HudCraftingVocab.BATCH_AMOUNT_FORMAT % cap]]
    meter.add_theme_color_override("font_color",
        HudStyle.DANGER if cap > 0.0 and mass > cap else HudStyle.INK_DIM)
    meter.set_meta(TRADE_MASS_METER_META, true)
    return meter

## The manifest as the command's own repeated tail: one `{id, is_material, amount}` line per LOADED
## row, in the order the sheet lists them. Rows at zero are dropped — a line naming no quantity is not
## a line the player asked for.
func _trade_manifest_lines(rows: Array) -> Array:
    var lines: Array = []
    for row_variant in rows:
        var row: Dictionary = row_variant as Dictionary
        var amount := float(row.get("amount", 0.0))
        if amount <= 0.0:
            continue
        lines.append({
            "id": String(row.get("id", "")),
            "is_material": bool(row.get("is_material", false)),
            "amount": amount,
        })
    return lines

## Empty the manifest and forget the destination. **One act**, for `_clear_party_quarry`'s reason: a
## destination without its cargo, or cargo without its destination, is a half-composed shipment that
## the next composing act would inherit without ever being shown.
func _clear_trade_manifest() -> void:
    _trade_destination_band = HudConst.NO_BAND_ID
    _trade_food = 0.0
    _trade_materials = {}

## Drop the composed quarry AND the fill target it was counted in. **They are one act** — a target is
## a count of a SPECIFIC herd's animals, so a target outliving its quarry would be handed to the next
## one, where `raid_load` answers a target at or above capacity by returning the pack — which is why
## the pairing now lives inside `ComposeState.clear_party_quarry` rather than being spelled out here:
## the map re-pick sets a quarry WITHOUT reaching this function, and did carry the stale target over.
## `ComposeState.seed_hunt` makes the same pairing on the herd drawer's side.
func _clear_party_quarry() -> void:
    _compose.clear_party_quarry()

## The Quarry row — the Band and Kit rows' shape, with a button instead of a picker. Unpicked it
## invites (`Choose…`, primary); picked it states the herd and stays available for a re-pick (ghost).
##
## **IT IS PRESENTED AS ONE OF THAT FAMILY AND IT IS NOT ONE OF THEIR KIND, and both halves of that
## are deliberate.** It takes the shared key label (`HudWidgets.build_field_key`, one declared width),
## the same ghost chrome and therefore the same height and the same left-aligned face — so the three
## field rows on a sheet read as one stack rather than three different-looking widgets. What it must
## NEVER take is dropdown chrome: pressing it ARMS A MAP PICK. Quarries are chosen spatially — glow
## rings on the eligible herds, the targeting banner, the in-reach refusal nudge — and the candidates
## are scattered across the map rather than enumerable in a sensible list, so an arrow here would
## promise a list that never opens, which is worse than the inconsistency it would paper over. The one
## list this row does offer is the `⋯` chooser at the end, and it appears only where a hex genuinely
## holds more than one eligible quarry.
func _build_quarry_row(band: Dictionary, herd: Dictionary) -> HBoxContainer:
    var row := HBoxContainer.new()
    row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    row.add_child(HudWidgets.build_field_key(HudComposeVocab.COMPOSE_FIELD_QUARRY))
    var pick := Button.new()
    pick.focus_mode = Control.FOCUS_NONE
    # EXPAND_FILL is load-bearing on the picked branch: `clip_text` drops the button's minimum width
    # to ~0, so beside the key label it collapses to a sliver. Both branches take it so the row does
    # not resize as a quarry is chosen.
    pick.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    # LEFT, the alignment an `OptionButton` takes for itself — a `Button`'s stock CENTER would put the
    # quarry's name in the middle of its box beside two pickers whose values start hard against the
    # box's leading edge. It also puts the species ART immediately before the name it belongs to
    # rather than at the far end of the button.
    pick.alignment = HORIZONTAL_ALIGNMENT_LEFT
    if herd.is_empty():
        pick.text = HudComposeVocab.COMPOSE_QUARRY_CHOOSE
        pick.tooltip_text = HudComposeVocab.SEND_HUNT_EXPEDITION_HINT
        HudStyle.apply_button(pick, "primary")
    else:
        var name_text := SourceForecast.herd_display_name(herd)
        # The picked quarry wears the species' bundled ART where there is any (issue #439). A Button
        # takes an icon natively, so this is its `icon` PROPERTY rather than a glyph welded into the
        # face — and only the emoji branch keeps the format string, so a species with art loses the
        # leading glyph instead of carrying both. `icon_max_width` is what stops the 256px source
        # setting the button's minimum and dragging the compose row wide; `expand_icon` then fits it
        # to the button's own height. UNTINTED: `apply_button` sets no `icon_*_color`, and the stock
        # theme's is opaque white, so the animal renders in its own colours like every other marker.
        var quarry_sprite := FaunaSprites.for_herd(name_text)
        if quarry_sprite != null:
            pick.icon = quarry_sprite
            pick.expand_icon = true
            pick.add_theme_constant_override("icon_max_width",
                HudComposeVocab.COMPOSE_QUARRY_ICON_MAX_WIDTH)
            pick.text = name_text
        else:
            pick.text = HudComposeVocab.COMPOSE_QUARRY_LABEL_FORMAT % [FoodIcons.for_herd(name_text), name_text]
        pick.clip_text = true
        pick.tooltip_text = HudComposeVocab.COMPOSE_QUARRY_TOOLTIP_FORMAT % [
            name_text, int(herd.get("x", -1)), int(herd.get("y", -1)),
        ]
        HudStyle.apply_button(pick, "ghost")
    # **THE OPEN SHEET'S MISSION DECIDES WHAT COUNTS AS A QUARRY**, so it rides with the pick rather
    # than being re-guessed at the click: a hunt's quarry must lie beyond the band's reach and a
    # denial raid's need not (`TargetingController.is_expedition_quarry`).
    var mission := _party_compose_mission
    pick.pressed.connect(func() -> void: _targeting.begin_pick_quarry(band, mission))
    row.add_child(pick)
    # **THE HEX MAY HOLD MORE THAN ONE HERD, AND THE MAP CANNOT SAY WHICH** — `try_dispatch` is handed
    # a TILE, so a rabbit warren sharing a hex with a wolf pack resolves to whichever the snapshot
    # lists first and re-clicking resolves to the same one. The chooser is the way to the others, and
    # it lives HERE rather than at the click because the choice is made against the forecast: the
    # collapse verdict, the raid payload and the useful party size are all functions of the herd, and
    # they exist only once the form is rendered. Absent with one candidate, so the common case renders
    # exactly as it did.
    if not herd.is_empty():
        var candidates := _targeting.eligible_quarries_on_tile(
            band, int(herd.get("x", -1)), int(herd.get("y", -1)), mission)
        # **THE CHOOSER'S WIDTH COMES OUT OF THE PICK, NOT OUT OF THE KEY**, and that is structural
        # now rather than a per-branch override. `build_field_key` takes a DECLARED width and does not
        # expand, so the pick is the row's only expanding child whether the row has two children or
        # three — the chooser simply takes its own width out of the pick's share. The defect this
        # replaced was the key EXPANDING too, which halved the name's room the moment a third control
        # appeared: `🐇 Rabbit Warren` came back clipped to `Rabbit Warre` on the very frame the
        # chooser exists to serve, and the cure was a `SIZE_FILL` written into this branch alone.
        if candidates.size() > 1:
            row.add_child(_build_quarry_choices_menu(band, herd, candidates, mission))
    return row

## The quarry chooser: the `⋯` menu the zone heads already use, so the panel keeps ONE "there are
## choices here" glyph, with the candidates as radio-check items — a menu of plain items could not say
## which herd is the current one. A pick routes through `TargetingController.choose_quarry`, the SAME
## adoption the map click makes, so switching herds here and picking one there leave the composition
## in one state — which is also why `mission` is threaded down to it rather than defaulted: the
## adoption re-runs the eligibility test, and under denial the candidates include herds a hunt's rule
## would refuse.
func _build_quarry_choices_menu(band: Dictionary, chosen: Dictionary,
        candidates: Array, mission: String) -> MenuButton:
    var chosen_id := String(chosen.get("id", ""))
    var entries: Array = []
    for candidate_variant in candidates:
        var candidate: Dictionary = candidate_variant as Dictionary
        var name_text := SourceForecast.herd_display_name(candidate)
        # The item names the herd exactly as the picked-quarry button does — bundled ART where the
        # species has any, the emoji only where it does not — so the row and the menu cannot describe
        # one herd two ways, and two species sharing an emoji (Unicode ships ONE deer) stay apart.
        var sprite := FaunaSprites.for_herd(name_text)
        var entry := {
            "label": name_text if sprite != null \
                else HudComposeVocab.COMPOSE_QUARRY_LABEL_FORMAT % [FoodIcons.for_herd(name_text), name_text],
            HudWidgets.MENU_ENTRY_CHECKED: String(candidate.get("id", "")) == chosen_id,
            "on_pick": func() -> void: _targeting.choose_quarry(band, candidate, mission),
        }
        if sprite != null:
            entry[HudWidgets.MENU_ENTRY_ICON] = sprite
        entries.append(entry)
    var menu := HudWidgets.build_section_menu(entries,
        HudComposeVocab.COMPOSE_QUARRY_CHOICES_TOOLTIP)
    menu.set_meta(HudWidgets.QUARRY_CHOICES_META, true)
    return menu

## Leave the compose sheet — every flag together, so `open` / `mission` / `quarry` can never disagree.
## Also disarms any in-flight quarry pick: the ✕ can be pressed while a docked-sheet quarry pick is
## armed (the pick leaves this sheet open, unlike the floating one), so closing must tear down the
## targeting banner + herd glow too, else they persist over no sheet and a later click still fills a
## closed sheet. The call no-ops when no pick is armed.
func _close_party_compose() -> void:
    _party_compose_open = false
    _party_compose_mission = ""
    _split_workers = 1
    _clear_party_quarry()
    # …and the manifest with it: goods loaded for a shipment the player cancelled are not goods they
    # asked to send. `_clear_trade_manifest` carries the pairing rule.
    _clear_trade_manifest()
    _targeting.cancel_pick_quarry()
    # The measured requirement belongs to ONE composing act — see `_party_compose_needed`. Carrying a
    # closed form's high-water mark into the next one would float a sheet that has not been measured.
    _party_compose_needed = 0.0
    _party_compose_measured_box = Vector2.ZERO
    # Explicitly, as well as through the render below: `rerender()` is a no-op with no panel or no
    # panel band, and a float outliving its sheet is the worst outcome available here.
    _party_compose_sheet = null
    _dismiss_compose_float()
    rerender()

# ---- the compose sheet's FLOAT (see `ui/hud/BandComposeFloat.gd`) --------------------------------

## Does the composed sheet have to leave the zone? **A MEASUREMENT, never a dock-edge test** —
## `_party_compose_needed` is what the parties column demanded the last time the zone actually held the
## sheet; the box is the zone the panel currently offers.
##
## **AN UNKNOWN BOX ANSWERS `false`, and that asymmetry is deliberate.** Floating is the drastic,
## instantly-visible branch, so it has to be POSITIVELY justified — never taken on a guessed
## `ZONE_FALLBACK_SIZE` that stands in for a box the panel has not laid out yet. The worst case of
## staying inline is one clipped frame, which is what shipped for months and is strictly better than a
## sheet leaping onto the map.
func _party_compose_floats() -> bool:
    var box := _parties_zone_box_known()
    if box == Vector2.ZERO:
        return false
    return _party_compose_needed > box.y + HudComposeVocab.COMPOSE_FLOAT_SLACK

## Float `sheet` beside the panel card. Builds the float on first use — a session whose sheets always
## fit never makes one — and parents it on a `CanvasLayer`, since a `RefCounted` cannot.
##
## **THE COMPOSE LAYER, NOT THE HUD ITSELF** (`HudLayer.compose_host()`). This is the drawer sheet's
## defect at the panel's own entry point to the same surface: on the HUD's own layer the float drew
## under a top-docked event bar, which is `MOUSE_FILTER_STOP`, so the party form under it took no
## clicks either. The compose layer sits one above the dock's — see `HudLayer.COMPOSE_LAYER_INDEX`.
##
## The float still has NO dismiss catcher of its own (its own header says why: the quarry picker needs
## the sheet to survive a map click), so unlike `ComposeSheet` a click on the bar behind it reaches
## the bar rather than putting the sheet away.
func _mount_compose_float(sheet: Control) -> void:
    if _host == null or _panel == null:
        return
    if _compose_float == null or not is_instance_valid(_compose_float):
        _compose_float = BandComposeFloat.new()
        _host.compose_host().add_child(_compose_float)
    _compose_float.mount(sheet, _panel.card_rect(),
        BandComposeFloat.map_facing_side(_panel.get_dock()), _parties_zone_box().x)

func _dismiss_compose_float() -> void:
    if _compose_float != null and is_instance_valid(_compose_float):
        _compose_float.dismiss()

## Is the compose sheet currently floated? Read by `band_panel_preview`, which has to assert BOTH that
## the sheet left the zone and that it fits the viewport beside the card.
func compose_is_floating() -> bool:
    return _compose_float != null and is_instance_valid(_compose_float) and _compose_float.is_floating()

## The float node, or `null` if one was never needed. For the harness's rect assertions.
func compose_float() -> BandComposeFloat:
    return _compose_float

## **MEASURE THE SHEET WHERE THE PANEL ACTUALLY PUT IT, ONCE IT HAS BEEN LAID OUT.** Godot lays out
## through the message queue, so nothing built during a render has a rect (or, for an autowrap `Label`,
## an honest minimum height — a detached one shapes at a wrap width of ZERO and reports every word on
## its own line). Waiting for the deferred sort is what gives the sheet its real width and makes
## `get_combined_minimum_size()` re-shape against it.
##
## Only the IN-ZONE render is measured. A floated sheet is measured at the float's own column, which is
## never narrower than the zone's, so trusting it could report a height the zone would not reproduce
## and hand the sheet back into a box that then clips it — the oscillation this narrow rule removes.
## While floating, the latched requirement stands and the fork is re-decided against the live box, so a
## zone that GROWS (a dock change, a taller window) takes its sheet back on the very next render.
##
## **A READING TAKEN BEFORE THE LAYOUT PASS IS NOT RECORDED AT ALL, AND IT IS THE SHEET THAT SAYS SO.**
## The mark is a high-water mark for one composing act (it must be, or the sheet hops back into the
## zone as a field clears — a layout change under the player's hands), so ONE bad reading latches until
## the sheet closes. The two ways to take one are the two guards in `_party_compose_measurable`: a zone
## box the panel cannot state yet, and a sheet with no honest rect.
##
## **ASKING THE ZONE COLUMN INSTEAD IS WHAT LET THIS DEFECT BE REPORTED TWICE.** The column is anchored
## `PRESET_FULL_RECT` into its zone host, so Godot hands it the host's width SYNCHRONOUSLY the instant
## it is reparented; everything under it is sized by the container sort, which is DEFERRED. So the two
## are established by different mechanisms and the column's width says nothing about whether its
## contents have been laid out — measured on the empty hunt form, `col.size.x == 356` (a wholly
## plausible reading) beside `col.get_combined_minimum_size().y == 1278`, where the laid-out answer is
## 207. 1278 floats that sheet out of every dock this client has, and the high-water mark then holds it
## there for the rest of the composition, which is exactly the reported picture: `Quarry: Choose…`, one
## hint, a disabled Send, floating out of a dock with 800px to spare.
##
## **AND IT WAITS RATHER THAN GIVING UP AFTER ONE FRAME.** One `process_frame` is the normal cost, but
## whether the deferred sort has been flushed by the time this coroutine resumes depends on where in
## the frame the render that armed it ran — so the wait is a bounded RETRY
## (`COMPOSE_MEASURE_MAX_FRAMES`) rather than a single look. Waiting another frame is cheap; recording
## a phantom costs the rest of the composing act, and returning unmeasured leaves the mark to whatever
## render happens to arm it next.
func _measure_party_compose() -> void:
    if _party_compose_measuring or _host == null:
        return
    _party_compose_measuring = true
    var measurable := false
    for _frame in range(HudComposeVocab.COMPOSE_MEASURE_MAX_FRAMES):
        await _host.get_tree().process_frame
        if not _party_compose_still_measuring():
            _party_compose_measuring = false
            return
        if _party_compose_measurable():
            measurable = true
            break
    _party_compose_measuring = false
    if not measurable:
        return
    var needed: float = _parties_zone_col.get_combined_minimum_size().y
    if needed <= _party_compose_needed:
        return
    _party_compose_needed = needed
    _party_compose_measured_box = _parties_zone_box_known()
    if _party_compose_floats():
        rerender()

## Is there still an in-zone sheet to measure? Re-asked every frame of the retry above, because a
## composing act can end (or float) while the coroutine is waiting for a layout pass.
func _party_compose_still_measuring() -> bool:
    if not _party_compose_open or compose_is_floating():
        return false
    if _party_compose_sheet == null or not is_instance_valid(_party_compose_sheet) \
            or not _party_compose_sheet.is_inside_tree():
        return false
    return _parties_zone_col != null and is_instance_valid(_parties_zone_col) \
        and _parties_zone_col.is_inside_tree()

## May the deferred measurement be RECORDED this frame? All three terms are about whether a number
## taken now could be honest at all, never about its size: the panel must be able to state the box the
## mark will be compared against, the parties column must have a rect at all, and **THE SHEET MUST HAVE
## BEEN FITTED TO THAT COLUMN**.
##
## That last term is the one that decides it, and it is a RELATION rather than a floor. The column's
## own width is set synchronously by its anchors and says nothing about the deferred container sort
## (see `COMPOSE_MEASURE_MIN_COLUMN_WIDTH`), and a bare floor on the SHEET does not close it either —
## an unsorted `Control` still clamps its size up to its own combined minimum, so the unlaid-out sheet
## reports a plausible 220px against a 356px column, wide enough to pass any floor and narrow enough
## that its labels are still wrapping at the wrong width. Once the sort has run, a `VBoxContainer`
## fits every child to its own width, so `sheet.size.x >= col.size.x` holds exactly — and it is the
## only reading that distinguishes "laid out" from "clamped to its own minimum".
func _party_compose_measurable() -> bool:
    if _parties_zone_box_known() == Vector2.ZERO:
        return false
    if _party_compose_sheet == null or not is_instance_valid(_party_compose_sheet):
        return false
    if _parties_zone_col.size.x < HudComposeVocab.COMPOSE_MEASURE_MIN_COLUMN_WIDTH:
        return false
    return _party_compose_sheet.size.x >= _parties_zone_col.size.x

## Drop the latched requirement when the parties zone's BOX changes — a dock move, a collapse, a window
## resize. The mark answers "what did this sheet demand of THAT column", so carried across a box change
## it is an answer to a question nobody asked: a mark latched in a 265px bottom dock would keep the
## sheet floating in the 1055px left dock it was just moved into. Called from the zone builder, i.e.
## every render, so it cannot be missed by a path that forgot to call it.
func _note_parties_zone_box() -> void:
    var box := _parties_zone_box_known()
    if box == Vector2.ZERO or box == _party_compose_measured_box:
        return
    _party_compose_needed = 0.0
    _party_compose_measured_box = box

# ---- badges -----------------------------------------------------------------

## Push the narrow shell's tab badges: Work carries its attention count (hot) or its source count,
## Parties its size (hot while any party is awaiting orders). Band carries none — it is always there.
func _push_zone_badges(band: Dictionary) -> void:
    if _panel == null:
        return
    var models := _work_source_models(band, _band_labor.effective_idle(band))
    var attention: Array = models.filter(func(m): return bool(m["attention"]))
    _panel.set_tab_badge(BandCityPanel.ZONE_BAND, "", false)
    _panel.set_tab_badge(BandCityPanel.ZONE_WORK,
        str(attention.size()) if not attention.is_empty() else str(models.size()),
        not attention.is_empty())
    var parties := _band_labor.band_parties(band)
    var awaiting := false
    for exp in parties:
        if HudFormat.expedition_phase_key(exp) == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
            awaiting = true
    _panel.set_tab_badge(BandCityPanel.ZONE_PARTIES,
        str(parties.size()) if not parties.is_empty() else "", awaiting)

## Recall the selected in-flight expedition (folds it home). Emits recall_expedition_requested;
## Main formats the `recall_expedition …` command.
func _on_recall_expedition_pressed(expedition: Dictionary) -> void:
    if expedition.is_empty():
        return
    # A detached party is a band too, and `recall_expedition <faction> <expedition_band_id>` names it
    # by the same durable id — never its ECS entity bits.
    emit_signal("recall_expedition_requested", {
        "faction": int(expedition.get("faction", HudConst.PLAYER_FACTION_ID)),
        "expedition_band_id": int(expedition.get("band_id", HudConst.NO_BAND_ID)),
    })

## Split the panel's band in two. Emits split_band_requested; Main formats the `split_band …`
## command.
func _on_split_band_pressed(band: Dictionary, workers: int) -> void:
    if band.is_empty() or workers <= 0:
        return
    # Same handle rule as the recall above: `split_band <faction> <band> <workers>` names the band by
    # its durable BandId — never its ECS entity bits, which the server would resolve to nothing at
    # all, silently. `cargo xtask command-guard` is what asserts it.
    emit_signal("split_band_requested", {
        "faction": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
        "workers": workers,
    })

## Render a player band's detail + labor allocation into the dockable Band/City panel and
## populate its header/cycler. The single place the panel's subject is set — shared by roster/map
## selection (`_render_occupant_drawer`) and the per-snapshot refresh (`refresh_snapshot`), so
## the panel is a persistent command center that survives selection changes.
func render_band(unit: Dictionary) -> void:
    if _panel == null or unit.is_empty():
        return
    # ⛔ **A SNAPSHOT MID-DRAG WOULD END THE GESTURE** — see `_queue_drag_in_flight`. The frames a
    # drag spans are the frames the panel can afford to skip: it re-renders on the drop or the cancel.
    if _queue_drag_in_flight():
        return
    # Leaving the faction page is a subject change like any other, so the composing act it interrupted
    # is closed on the way back the same way a band-to-band cycle closes one. The page itself composes
    # nothing, but the player may have opened a sheet, cycled away to read the rollup and cycled back.
    if _panel_is_faction:
        _panel_is_faction = false
        _clear_party_quarry()
        _clear_trade_manifest()
        _party_compose_open = false
        _party_compose_mission = ""
        _split_workers = 1
        _party_compose_needed = 0.0
        _party_compose_measured_box = Vector2.ZERO
    # A quarry is chosen FOR a band (its travel time and useful party size are band-relative), so the
    # cycler swapping the panel subject must not carry one across — and neither may the rest of the
    # composing act: the party size, the mission and the MEASURED requirement that floated the sheet
    # all belong to the band that was being composed for. Closed inline rather than through
    # `_close_party_compose`, which re-renders, and this IS the render.
    if int(unit.get("entity", -1)) != int(_band_labor.panel_band().get("entity", -1)):
        _clear_party_quarry()
        _clear_trade_manifest()
        _party_compose_open = false
        _party_compose_mission = ""
        _split_workers = 1
        _party_compose_needed = 0.0
        _party_compose_measured_box = Vector2.ZERO
    # DEEP-COPY the subject: the panel band must NOT alias the selection's unit dict (the
    # selection path passes it in). The panel persists across selection changes, so it needs its
    # own stable copy — a later selection swap (or an in-place edit of the selection's unit dict)
    # must not mutate or blank it. The zone closures below also capture this stable copy, so they
    # keep targeting the panel band regardless of the current selection.
    _band_labor.set_panel_band(unit.duplicate(true))
    # **THE LAYOUT IS DECLARED BEFORE THE ZONES ARE BUILT**, and the order is load-bearing: the shell
    # threshold is a sum over the declared zones, so arriving from the four-zone faction page can flip
    # the shell — and every builder below pages against `zone_size()`, which the flip moves.
    _panel.set_zone_layout(BAND_ZONE_LAYOUT)
    # No tint-context reset here either: `_build_vitals_label` (inside the band zone below) builds its
    # own `DetailFormat.Context` per render, so the context cannot survive from the previous one.
    # The zone contents. Ownership passes to the panel, which frees the previous render's zones
    # and parents these into whichever shell (wide columns / narrow tabs) its width selected.
    _panel.set_zones({
        BandCityPanel.ZONE_BAND: HudWidgets.wrap_zone(build_band_zone(_band_labor.panel_band())),
        BandCityPanel.ZONE_WORK: HudWidgets.wrap_zone(build_work_zone(_band_labor.panel_band())),
        BandCityPanel.ZONE_PARTIES: HudWidgets.wrap_zone(build_parties_zone(_band_labor.panel_band())),
    })
    _push_zone_badges(_band_labor.panel_band())
    # Header: settlement stage + name + stage label. The stage `id` is the panel's sprite key
    # (bundled art), the `icon` its emoji fallback for a stage with no art; both already flow
    # onto the marker/cohort dict. A missing stage falls back to a neutral glyph.
    var stage_id := String(_band_labor.panel_band().get("settlement_stage_id", "")).strip_edges()
    var glyph := String(_band_labor.panel_band().get("settlement_stage_icon", "")).strip_edges()
    var stage_label := String(_band_labor.panel_band().get("settlement_stage_label", "")).strip_edges()
    var index := _index_of_player_band(int(_band_labor.panel_band().get("entity", -1)))
    _panel.set_header(stage_id, glyph, HudFormat.band_display_name(_band_labor.panel_band(), index + 1), stage_label,
        _panel_position_label(_band_labor.panel_band()))
    _panel.set_cycler(_cycler_index_of_band(index), _cycler_count())
    # A band HAS a tile, and its `band` zone is a band's, so both header affordances come back on. Both
    # setters early-out on an unchanged value, so a band-to-band cycle costs nothing.
    _panel.set_subject_jumpable(true)
    # `set_zones` above already flipped the panel to band-present; just make sure it is shown.
    _panel.set_shown(true)
    # THE TRIGGER'S MEASUREMENT, taken a frame from now against the tree this render just handed over
    # — see `_party_compose_needed`. Armed unconditionally: it costs one awaited frame and answers
    # immediately when no sheet is open.
    _measure_party_compose()

## Render the FACTION PAGE — the all-band rollup pinned as the cycler's first entry (issue #450).
##
## It fills the SAME three zones a band does, one scale up: `band` is who the faction is and what it
## holds, `work` is the whole workforce plus where those hands are and what the faction knows, and
## `parties` is everyone who is out. The arithmetic is `FactionRollup`'s — an all-`static` layer, this
## page carrying no state of its own — and every total it prints is a SUM over the per-band answers,
## so a band's page and this one cannot disagree about a number.
##
## **`_band_labor.panel_band()` IS DELIBERATELY LEFT ALONE.** It is what the cycler walks back into, so
## cycling faction → next returns to the band the player was reading rather than to the roster's first,
## and `_resolve_panel_band` still has a subject to re-resolve when the page is left.
func render_faction() -> void:
    if _panel == null or _band_labor.player_bands().is_empty():
        return
    _panel_is_faction = true
    # A composing act belongs to the BAND it was opened on, so leaving that band for this page ends it
    # — the identical rule `render_band` applies to a band-to-band cycle, and the float must come down
    # with it (it lives outside the panel and no zone rebuild reaches it).
    _clear_party_quarry()
    _clear_trade_manifest()
    _party_compose_open = false
    _party_compose_mission = ""
    _split_workers = 1
    _party_compose_needed = 0.0
    _party_compose_measured_box = Vector2.ZERO
    _party_compose_sheet = null
    _dismiss_compose_float()
    # This page builds no work BOARD, so the re-page path must have nothing to re-page: `_on_zones_resized`
    # would otherwise rebuild the previous band's board into a host `set_zones` is about to free.
    _work_zone_host = null
    _work_zone_band = {}
    _parties_zone_col = null
    # The faction's alerts, read from the ONE model the turn orb reads — so the orb, the map and this
    # page can never disagree about which band needs the player.
    var attention := _attention.build_band_attention(
        _band_labor.player_bands(), _band_labor.player_expeditions())
    # THREE zones here, the same count a band declares, and declared BEFORE the builders run — see
    # `render_band`.
    _panel.set_zone_layout(FACTION_ZONE_LAYOUT)
    _panel.set_zones({
        # The band zone's TIER is read off the box the panel is offering it, which the
        # `set_zone_layout` above has just settled — a horizontal dock's ~300px cannot hold every one
        # of its blocks at the page's row size, so DISCOVERIES yields there.
        BandCityPanel.ZONE_BAND:
            HudWidgets.wrap_zone(FactionRollup.build_band_zone(_band_labor, _disclosures,
                _faction_settling(), _faction_discoveries(), _faction_band_zone_is_full(),
                _player_knowledge())),
        BandCityPanel.ZONE_WORK:
            HudWidgets.wrap_zone(FactionRollup.build_work_zone(_band_labor,
                attention, _faction_open_row, _toggle_faction_row, jump_to_band_entity)),
        # **THE PARTIES ROW'S JUMP GOES TO THE HOME BAND, which is what the row is NAMED for.** A
        # party's own tile means nothing without the map and there is nothing to DO to a party from
        # here — acting on it means cycling to the band it left, so that is where its link lands. The
        # row's TOGGLE still keys on the party (`FactionRollup.build_parties_zone`).
        BandCityPanel.ZONE_PARTIES:
            HudWidgets.wrap_zone(FactionRollup.build_parties_zone(_band_labor, _herd_label_for_id,
                attention, _faction_open_row, _toggle_faction_row, jump_to_band_entity)),
    })
    _push_faction_zone_badges()
    # No stage id ⇒ no bundled art resolves and the emoji stands; the band count takes the stage word's
    # slot, and the empty position label hides the coordinate slot outright.
    _panel.set_header("", HudFormat.FACTION_PAGE_GLYPH, HudFormat.FACTION_PAGE_NAME,
        HudFormat.faction_bands_label(_band_labor.player_bands().size()), "")
    _panel.set_cycler(FACTION_CYCLER_INDEX, _cycler_count())
    # A faction has no tile to jump to. (The narrow shell's first tab reads `Faction` rather than
    # `Band` because `FACTION_ZONE_LAYOUT` says so — a subject names its own zone labels.)
    _panel.set_subject_jumpable(false)
    _panel.set_shown(true)

## The tab badges for the faction page: the totals its three zones answer, so the narrow shell states
## them without the player having to open each tab. `work` counts BANDS rather than sources — this
## zone's list is the roster, not a board — and `parties` keeps the band page's `hot` rule, an awaiting
## party being a demand on the player wherever it is standing.
func _push_faction_zone_badges() -> void:
    if _panel == null:
        return
    var population := 0
    for band_variant in _band_labor.player_bands():
        if band_variant is Dictionary:
            population += int((band_variant as Dictionary).get("size", 0))
    _panel.set_tab_badge(BandCityPanel.ZONE_BAND, str(population) if population > 0 else "", false)
    _panel.set_tab_badge(BandCityPanel.ZONE_WORK, str(_band_labor.player_bands().size()), false)
    var parties := _band_labor.player_expeditions()
    var awaiting := false
    for party in parties:
        if HudFormat.expedition_phase_key(party) == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
            awaiting = true
    _panel.set_tab_badge(BandCityPanel.ZONE_PARTIES,
        str(parties.size()) if not parties.is_empty() else "", awaiting)

## The band's hex coordinates for the panel header — the ONE place they are resolved, because the two
## paths that reach this panel spell them DIFFERENTLY and used to render differently because of it.
## The per-snapshot refresh hands over the cohort dict the native decoder built
## (`native/src/dict/population.rs`), which carries `current_x` / `current_y` and NO `pos`; a click on
## the band's map marker hands over MapView's marker copy, which carries a two-element `pos` array.
## So the snapshot path rendered no coordinates at all and the map path did, and a turn tick then took
## them away again. Preferring the cohort keys and falling back to `pos` makes both paths produce the
## identical header; neither resolvable ⇒ `""`, which the panel renders as nothing.
func _panel_position_label(band: Dictionary) -> String:
    if band.has("current_x") and band.has("current_y"):
        return HudFormat.BAND_HEADER_POSITION_FORMAT % [int(band["current_x"]), int(band["current_y"])]
    var pos_array: Array = Array(band.get("pos", []))
    if pos_array.size() == 2:
        return HudFormat.BAND_HEADER_POSITION_FORMAT % [int(pos_array[0]), int(pos_array[1])]
    return ""

## Select an expedition (from the panel's Active-expeditions list) on the map: recenter + select
## its hex (rebuilds that hex's roster), then pin the exact expedition so the map ring moves and the
## Occupants card renders its expedition drawer. Mirrors `cycle_band`'s routing. The Band/City
## panel itself stays on its band (expeditions detail in the Occupants card, per the existing split);
## a co-located band auto-select can't hijack it — we restore the panel band if it changed.
func select_expedition(entity: int, x: int, y: int) -> void:
    var panel_band_keep: Dictionary = _band_labor.panel_band().duplicate(true) if not _band_labor.panel_band().is_empty() else {}
    if x >= 0 and y >= 0:
        emit_signal("alert_focus_requested", x, y)
    if not _selectioncard.find_roster_unit(entity).is_empty():
        _selectioncard.select_roster_occupant("unit", entity)
        emit_signal("roster_occupant_selected", "unit", entity)
    if not panel_band_keep.is_empty() and int(_band_labor.panel_band().get("entity", -1)) != int(panel_band_keep.get("entity", -1)):
        render_band(panel_band_keep)

## A Current-actions row's label was clicked: show the source the band is working. Recenter + select
## its hex (`alert_focus_requested` → `MapView.focus_and_select_tile`) and, for a hunted herd, pin
## the herd itself (`roster_occupant_selected` → `MapView.select_occupant`) so its drawer opens on
## the herd rather than whatever occupant the hex auto-selects. This is exactly the routing the
## Active-expeditions rows and the turn-orb "Jump →" use — no new path. The Band/City panel stays on
## its band: focusing a hex that hosts another band would otherwise hijack the panel.
func focus_labor_source(x: int, y: int, herd_id: String = "") -> void:
    if x < 0 or y < 0:
        return
    var panel_band_keep: Dictionary = _band_labor.panel_band().duplicate(true) if not _band_labor.panel_band().is_empty() else {}
    emit_signal("alert_focus_requested", x, y)
    # The focus above rebuilt the hex's roster, so the subject is resolvable now.
    if herd_id != "" and not _selectioncard.find_roster_herd(herd_id).is_empty():
        _selectioncard.select_roster_occupant("herd", herd_id)
        emit_signal("roster_occupant_selected", "herd", herd_id)
    elif herd_id == "":
        # A FORAGE ROW NAMES THE LAND, exactly as a hunt row names its herd. Focusing the tile alone
        # left the hex's AUTO-PICK to choose the subject, which on a shared hex opens whichever band or
        # herd happens to stand there rather than the patch the player clicked — the row jumping to a
        # place but not to a THING. The land is the patch's subject (its rung rows and its Sow control
        # live on the land card), and `SUBJECT_LAND` is the established third kind on the `(kind, id)`
        # contract, so this is the forage twin of the herd branch above and not a new mechanism.
        _selectioncard.select_land_subject()
        emit_signal("roster_occupant_selected", HudSelectionState.SUBJECT_LAND, HudConst.LAND_SUBJECT_ID)
    if not panel_band_keep.is_empty() and int(_band_labor.panel_band().get("entity", -1)) != int(panel_band_keep.get("entity", -1)):
        render_band(panel_band_keep)

## Show a hunted herd. Herds MIGRATE each turn, so the hunt assignment's `target_x/target_y` is a
## stale launch position: resolve the herd's LIVE tile from the snapshot herd list first, exactly as
## `BandOverlayRenderer.draw_band_work_highlights` resolves the hunted-herd ring (`_herd_by_id`, falling back to
## the assignment target when the herd is unknown — e.g. it left the visible fauna set).
func _focus_hunt_source(herd_id: String, fallback_x: int, fallback_y: int) -> void:
    var herd := _band_labor.find_world_herd(herd_id)
    var x := int(herd.get("x", fallback_x))
    var y := int(herd.get("y", fallback_y))
    focus_labor_source(x, y, herd_id)

## Re-render the panel band into the panel container, keyed off `_band_labor.panel_band()` (never the current
## selection). The panel's own allocation rebuilds (optimistic pending, etc.) route through this so
## they stay pinned to the panel's subject even when a foreign hex is selected.
## **THE DENIAL QUESTION, COMPOSED AND ASKED** — the hunt twin's sibling, and deliberately its own
## function because the mission carries neither a floor nor preset floors: there is nothing to sample
## but party size, which is also why `party_needed` can be searched exactly.
##
## `max_party` is the band's idle workers, and it bounds that search — so `party_needed == 0` means
## "no party YOU can field drives this herd down", a fact the player can act on, rather than "no party
## the sim happened to sample did".
func _denial_forecast_view(band: Dictionary, herd: Dictionary, kit_id: String, party: int,
        max_party: int) -> Dictionary:
    if _forecast_query == null:
        return {"state": ForecastQuery.STATE_PENDING, "answer": {}, "error": ""}
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    var herd_id := String(herd.get("id", ""))
    var subject := ForecastQuery.subject_of(ForecastQuery.KIND_DENIAL_RAID, band_id, herd_id)
    # No floor axis, so the key's floor slot is the one value every denial ask carries.
    var key := ForecastQuery.key_of(subject, kit_id, party, DENIAL_QUERY_FLOOR)
    if party > 0 and band_id != HudConst.NO_BAND_ID and herd_id != "":
        _forecast_query.ask(ForecastQuery.KIND_DENIAL_RAID, subject, key, {
            "faction_id": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": band_id,
            "herd_id": herd_id,
            "kit_id": kit_id,
            "party_workers": party,
            "max_party_workers": max_party,
        })
    return _forecast_query.view(subject, key)

## **THE HUNT QUESTION, COMPOSED AND ASKED.** The dock sheet's own copy of the herd drawer's helper —
## the two entry points compose one raid, and each owns the ask for the sheet it is rendering.
func _raid_forecast_view(band: Dictionary, herd: Dictionary, kit_id: String, party: int,
        floor: float, max_party: int) -> Dictionary:
    if _forecast_query == null:
        return {"state": ForecastQuery.STATE_PENDING, "answer": {}, "error": ""}
    var band_id := int(band.get("band_id", HudConst.NO_BAND_ID))
    var herd_id := String(herd.get("id", ""))
    var subject := ForecastQuery.subject_of(ForecastQuery.KIND_HUNT_TRIP, band_id, herd_id)
    var key := ForecastQuery.key_of(subject, kit_id, party, floor)
    if party > 0 and band_id != HudConst.NO_BAND_ID and herd_id != "":
        _forecast_query.ask(ForecastQuery.KIND_HUNT_TRIP, subject, key, {
            "faction_id": int(band.get("faction", HudConst.PLAYER_FACTION_ID)),
            "band_id": band_id,
            "herd_id": herd_id,
            "kit_id": kit_id,
            "party_workers": party,
            "floor": floor,
            "preset_floors": SourceForecast.preset_floors(),
            "max_party_workers": max_party,
        })
    return _forecast_query.view(subject, key)

## **THE SAME DENIAL QUESTION, ASKED ON BEHALF OF A PARTY ALREADY IN THE FIELD** — what the parties
## strip's `Collapse:` row and the party row's tooltip render (`DetailFormat.expedition_collapse_line`).
##
## **A DETACHED PARTY IS A BAND, so the query takes it unchanged**: its own `band_id` is the durable
## handle the sim resolves, and its `kit_id` is the kit it was OUTFITTED with — an expedition prices its
## whole life from the choice made at launch and never re-resolves against its home band's stock, so
## quoting the home band's kit here would price a raid nobody sent. Its `size` is its worker count (a
## detached party is all hands), which is the reading `HudBandLaborState.band_party_workers` takes.
##
## **`max_party_workers` IS THE PARTY'S OWN SIZE, and that is a statement rather than a stand-in.** That
## argument bounds the sim's search for the smallest party that breaks the herd, and the only party
## this surface is about is the one already out there — a wider search would price hands nobody can send
## to a raid that has already left.
##
## `{}` — never a pending view — for anything that is not a launched denial party with a live target, so
## a hunt party and a scout render no collapse row at all rather than one that waits forever.
##
## **PUBLIC BECAUSE THE OCCUPANTS DRAWER RENDERS THE SAME PARTY**, through the same
## `BandDetailLines.expedition_summary_lines`. `SubjectDrawerController` already holds this controller
## as a typed collaborator and dispatches into it; a second copy of the ask would be a second request-id
## stream and a second staleness rule over one socket, which is exactly what `ForecastQuery` being one
## injected instance exists to prevent.
func launched_party_denial_view(exp: Dictionary) -> Dictionary:
    if _forecast_query == null:
        return {}
    if String(exp.get("expedition_mission", "")) != HudExpeditionVocab.EXPEDITION_MISSION_DENY:
        return {}
    var band_id := int(exp.get("band_id", HudConst.NO_BAND_ID))
    var herd_id := String(exp.get("expedition_target_herd", "")).strip_edges()
    var party := int(exp.get("size", 0))
    if party <= 0 or band_id == HudConst.NO_BAND_ID or herd_id == "":
        return {}
    var kit_id := String(exp.get("kit_id", ""))
    var subject := ForecastQuery.subject_of(ForecastQuery.KIND_DENIAL_RAID, band_id, herd_id)
    var key := ForecastQuery.key_of(subject, kit_id, party, DENIAL_QUERY_FLOOR)
    _forecast_query.ask(ForecastQuery.KIND_DENIAL_RAID, subject, key, {
        "faction_id": int(exp.get("faction", HudConst.PLAYER_FACTION_ID)),
        "band_id": band_id,
        "herd_id": herd_id,
        "kit_id": kit_id,
        "party_workers": party,
        "max_party_workers": party,
    })
    return _forecast_query.view(subject, key)


func rerender() -> void:
    if _panel == null:
        return
    # The faction page is a SUBJECT, not a band, so every re-render path has to ask which one is up:
    # falling through to `render_band` here would drop the player back onto a band on a caret click, a
    # zone resize or any other in-place refresh.
    if _panel_is_faction:
        render_faction()
        return
    if _band_labor.panel_band().is_empty():
        return
    render_band(_band_labor.panel_band())

## Keep the panel a live, persistent command center each snapshot: hide it when there are no
## player bands, else re-resolve the shown band against the fresh snapshot (so steppers/idle stay
## current) and re-render it. Called from update_band_alerts after _band_labor.player_band()(s) refresh.
func refresh_snapshot() -> void:
    if _panel == null:
        return
    if _band_labor.player_bands().is_empty():
        _band_labor.set_panel_band({})
        # A faction with no band has no rollup either — the page is pinned to the cycler, and the
        # cycler is gone with the panel. Cleared here so a later band does not bring the page back as
        # the panel's subject without the player having asked for it.
        _panel_is_faction = false
        _panel.set_band_present(false)
        _panel.set_shown(false)
        # No band ⇒ no zones are rebuilt, so the footer builder's teardown never runs. The float is
        # the one piece of this panel that lives OUTSIDE it, and it must go down with the panel.
        _party_compose_open = false
        _party_compose_mission = ""
        _split_workers = 1
        _clear_trade_manifest()
        _party_compose_needed = 0.0
        _party_compose_measured_box = Vector2.ZERO
        _party_compose_sheet = null
        _dismiss_compose_float()
        return
    # The page SURVIVES a snapshot, exactly as a band subject does — its totals are what the tick just
    # moved, so a tick is precisely when it must re-render rather than hand the panel back to a band.
    if _panel_is_faction:
        render_faction()
        return
    render_band(_resolve_panel_band())

## The band the panel should show: the same one across snapshots (re-fetched live by entity), or
## the first player band (the default actor) when the shown band is gone / unset.
func _resolve_panel_band() -> Dictionary:
    if not _band_labor.panel_band().is_empty():
        var entity := int(_band_labor.panel_band().get("entity", -1))
        for b in _band_labor.player_bands():
            if b is Dictionary and int((b as Dictionary).get("entity", -1)) == entity:
                return b
    return _band_labor.player_bands()[0] if not _band_labor.player_bands().is_empty() else {}

## Index of a band (by entity) within `_band_labor.player_bands()`, or -1 if absent.
func _index_of_player_band(entity: int) -> int:
    for i in range(_band_labor.player_bands().size()):
        if int((_band_labor.player_bands()[i] as Dictionary).get("entity", -1)) == entity:
            return i
    return -1

## Injected by Main: the dockable Band/City panel the band drawer renders into.
## (The Food/Morale disclosure `meta_clicked` is wired per-render on the fresh summary RichTextLabel
## in `render_band`, since main's section-block model rebuilds that label each render.)
func set_panel(panel: BandCityPanel) -> void:
    _panel = panel
    # THE PANEL OWNS THE FILE, THIS CONTROLLER OWNS THE VOCABULARY. The panel stores the work sort as
    # an opaque string, so validating it is this side's job: an empty (never chosen) or unknown value
    # — a hand-edited prefs file, a sort retired since it was written — leaves the default standing.
    # Without the guard it would not produce a broken board but a YIELD-sorted one: `_sort_work_models`
    # branches on `== WORK_SORT_NAME`, so anything else falls through to yield, silently reinstating
    # the re-ranking-under-your-own-edit behaviour issue #460 removed.
    if panel != null:
        var stored := StringName(panel.work_sort_pref())
        if HudWorkVocab.WORK_SORTS.has(stored):
            _work_sort = stored
    # The panel re-reports its zone box on a shell flip / dock change / collapse / window resize.
    # Re-PAGE the work board on it — the other two zones are unaffected by a box change.
    if panel != null and not panel.zones_resized.is_connected(_on_zones_resized):
        panel.zones_resized.connect(_on_zones_resized)
    # The header's `⚒` carries no subject — the header is subject-independent chrome — so WHICH band
    # it opens on is answered here: the panel band. **`render_faction` never touches `_panel_band`**,
    # which is what makes "the last band loaded" already sitting there rather than state this had to
    # add, and `refresh_snapshot` hides the panel outright at zero bands, so a visible header always
    # has a band behind it.
    if panel != null and not panel.crafting_requested.is_connected(_on_crafting_requested):
        panel.crafting_requested.connect(_on_crafting_requested)
    # The header's `▲` needs no such resolution and gets no handler of its own: what your people know
    # is a FACTION fact, so the signal is relayed straight through with nothing attached to it.
    if panel != null and not panel.knowledge_requested.is_connected(_on_knowledge_requested):
        panel.knowledge_requested.connect(_on_knowledge_requested)
    # A faction drill-down row is a link to a BAND, and making that band the panel's subject is this
    # controller's job — the disclosure controller must not know the band panel exists.
    if _disclosures != null:
        _disclosures.set_faction_band_jump(jump_to_band_entity)

## The header's `⚒`. From a band page this is that band; from the FACTION page it is the last band
## loaded, which is sitting on the model already because `render_faction` deliberately leaves
## `_panel_band` alone. An empty subject emits nothing rather than opening an empty panel.
func _on_crafting_requested() -> void:
    var band := _band_labor.panel_band()
    if band.is_empty():
        return
    emit_signal("crafting_requested", band)

## The header's `▲`. **It resolves NOTHING, which is the whole difference from the `⚒` above.** A
## discovery unlocks a verb across the whole map and no band owns it, so there is no subject to look
## up and no empty-subject case to guard — the relay exists only so `HudLayer` connects one controller
## to another the way it does everywhere else.
func _on_knowledge_requested() -> void:
    emit_signal("knowledge_requested")

## Is the panel showing the FACTION page? Asked by the drawer, which otherwise re-asserts the selected
## band as the panel's subject on every render and would steal the page out from under a caret click.
func is_faction_page() -> bool:
    return _panel_is_faction

## Expand or collapse a faction summary row. ONE row open at a time — the zones clip, and two open
## details would push the second list off the bottom of a horizontal dock's box.
func _toggle_faction_row(owner: int) -> void:
    _faction_open_row = FACTION_ROW_NONE if _faction_open_row == owner else owner
    rerender()

## Make a band the panel's subject, by entity — the faction page's drill-down rows route here, so a
## popover row reaches a band the same way the cycler does (recenter, pin, render), rather than by a
## second path that could drift from it. Unknown entity ⇒ no-op.
func jump_to_band_entity(entity: int) -> void:
    var band := _band_labor.player_band_by_entity(entity)
    if band.is_empty():
        return
    _select_band_on_map(band)

## Walk to the next/prev subject (cycler ◀/▶) over `[the faction page] + player_bands()`.
##
## A band routes through the SAME band-selection a roster click uses — recenter + select the band's hex
## (rebuilding that hex's roster), then pin the exact band — so the map ring, Tile card, roster and this
## panel all land on the cycled band.
##
## **THE FACTION PAGE MOVES NO CAMERA, and that is a documented exception to decision 2 of
## `docs/plan_band_city_dock.md`** ("panel cycling recenters the map on the cycled settlement"). It has
## no tile: there is nothing to centre on, and recentring on the band the player happened to leave
## would move the map for a page that says nothing about where it is.
func cycle_band(delta: int) -> void:
    if _panel == null:
        return
    var n := _cycler_count()
    # One band ⇒ two entries, so the cycler is live where it used to be dead: the faction page is
    # reachable from the first band a faction ever has.
    if n <= 1:
        return
    var next := ((_cycler_index() + delta) % n + n) % n
    if next == FACTION_CYCLER_INDEX:
        render_faction()
        return
    _select_band_on_map(_band_labor.player_bands()[next - FACTION_CYCLER_ENTRIES])

## How many entries the cycler walks: every player band plus the pinned faction page.
func _cycler_count() -> int:
    return _band_labor.player_bands().size() + FACTION_CYCLER_ENTRIES

## Where the panel's current subject sits in that walk.
func _cycler_index() -> int:
    if _panel_is_faction:
        return FACTION_CYCLER_INDEX
    return _cycler_index_of_band(_index_of_player_band(int(_band_labor.panel_band().get("entity", -1))))

## A band's roster index as a CYCLER index — the pinned page's entries shift every band along by one.
## A band absent from the roster (mid-swap, or a marker click on one the snapshot has since dropped)
## resolves to the first, which is `_resolve_panel_band`'s own fallback.
func _cycler_index_of_band(roster_index: int) -> int:
    return FACTION_CYCLER_ENTRIES + (roster_index if roster_index >= 0 else 0)

## Jump to the panel band on the map (the header title is a "jump to my band" affordance): recenter
## + select its hex and move the ring, WITHOUT changing which band the panel shows (it's already
## `_band_labor.panel_band()`). No-op when there is no panel band.
##
## Silent on the faction page. The panel already refuses the click there (`set_subject_jumpable`), so
## this is the second half of one rule rather than the only guard — but `focus_panel_band` is reached
## BY NAME through `Main`'s `has_method` probe, so the verb must be safe to call in every state.
func focus_band() -> void:
    if _panel_is_faction:
        return
    _select_band_on_map(_band_labor.panel_band())

## Select a band's hex on the map — recenter + select the hex (rebuilding its roster) via
## `alert_focus_requested` (→ MapView.focus_and_select_tile) then pin the exact band so the map ring,
## Tile card, roster, and panel all agree. Shared by the cycler and the header "jump to band". A band
## with no live roster entry (no tile_info) is rendered directly into the panel instead.
func _select_band_on_map(band: Dictionary) -> void:
    if band.is_empty():
        return
    # **LEAVING THE FACTION PAGE IS THE FIRST THING THIS DOES, and the order matters.** This is the
    # explicit "make this band the subject" act — the cycler's ▶, a drill-down link, the header jump.
    # Its usual route is `roster_occupant_selected` → the drawer → the drawer's band branch, and that
    # branch is now GATED on the panel not being on the faction page (a passive re-render must not
    # steal the page). Clearing the flag here is what tells the gate this render is the wanted one;
    # without it the cycler walked off the page and the panel silently stayed on it.
    _panel_is_faction = false
    var entity := int(band.get("entity", -1))
    var x := int(band.get("current_x", -1))
    var y := int(band.get("current_y", -1))
    if x >= 0 and y >= 0:
        emit_signal("alert_focus_requested", x, y)
    if not _selectioncard.find_roster_unit(entity).is_empty():
        _selectioncard.select_roster_occupant("unit", entity)
        emit_signal("roster_occupant_selected", "unit", entity)
    else:
        render_band(band)

## **THE ONE NODE THE BUILD QUEUE'S DRAG NEEDS, and it exists because a `RefCounted` cannot hear a
## notification** (`docs/plan_standing_upkeep.md` §4.7b ③). Godot announces the END of a drag —
## dropped OR cancelled — as `NOTIFICATION_DRAG_END` to every `Control` in the tree and by no other
## means: there is no signal, and `Viewport.gui_is_dragging()` can only be polled. The suppression
## that keeps a snapshot from freeing the rows mid-gesture has to be lifted on a CANCEL as reliably as
## on a drop, so one invisible, input-transparent Control listens for it.
class QueueDragWatcher extends Control:
    var on_drag_end: Callable
    ## The expanded list's edge auto-scroll pump (§4.9 item 9c). It runs only between Godot's own
    ## `NOTIFICATION_DRAG_BEGIN` and `NOTIFICATION_DRAG_END`, so a panel with no drag in flight — which
    ## is every frame but a handful — processes nothing at all.
    var on_drag_tick: Callable

    func _ready() -> void:
        set_process(false)

    ## When the pump last ran, on the UNSCALED clock — see `_queue_autoscroll_tick` for why the frame
    ## delta is unusable here.
    var _ticked_usec: int = 0

    func _process(_scaled_delta: float) -> void:
        var now := Time.get_ticks_usec()
        var seconds := float(now - _ticked_usec) / HudWorkVocab.MICROSECONDS_PER_SECOND
        _ticked_usec = now
        if on_drag_tick.is_valid():
            on_drag_tick.call(seconds)

    func _notification(what: int) -> void:
        if what == NOTIFICATION_DRAG_BEGIN:
            _ticked_usec = Time.get_ticks_usec()
            set_process(true)
        elif what == NOTIFICATION_DRAG_END:
            set_process(false)
            if on_drag_end.is_valid():
                on_drag_end.call()
