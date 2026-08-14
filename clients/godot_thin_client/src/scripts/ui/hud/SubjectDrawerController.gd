class_name SubjectDrawerController
extends RefCounted

## The selection card's DRAWER RENDER DISPATCH (HUD decomposition Phase 2c-3, docs/plan_hud_decomposition.md):
## the last piece of the selection card to leave `Hud.gd`, after `SelectionCardController` took the
## identity/list half and `DrawerComposeController` took the compose half. It owns the one-drawer
## dispatch (`render_subject_drawer` → land vs occupant), the land-drawer content producer
## (`_tile_terrain_lines` + its `_forage_stock_lines` / `_graze_stock_lines` / `_stock_value` leaves — the
## two webs' rows are one rule rendered twice), the occupant/expedition/band-move `%AllocationPanel`
## branches, and the height-capping fit path.
##
## Built on the LegendController / SelectionCardController / DrawerComposeController / BandPanelController
## idiom: `HudLayer` holds one as `_drawer`, hands it the shared `RefCounted` state models BY REFERENCE
## (the SAME `HudSelectionState` / `HudBandLaborState` instances) and the sibling controllers it dispatches
## into, keeps the reflectively-reached coordinator core (`_render_selection_panel`) and the two-host
## disclosure fan-out (`_refresh_disclosure_hosts`) on the HUD node calling IN, and connects the two fit
## signals + `_refit_left_dock` to this controller's `fit_subject_drawer`.
##
## THE MOVE VERB IS A TYPED COLLABORATOR, not a Callable — the drawer's Move button `.connect()`s
## straight to `TargetingController.begin_move_band`, which owns the `_pending_move_band` state and the
## banner (the targeting machinery, with three other modes). `_is_player_unit` is a trivial private COPY
## (the SelectionCardController / BandPanelController precedent).
##
## THE FIT PATH IS THE HIGH-RISK PIECE. `fit_subject_drawer` does `await _host.get_tree().process_frame`
## — a `RefCounted` has no `get_tree()`, so the frame wait is threaded through the injected HOST node
## (the HUD CanvasLayer). A mis-threaded host or a dropped signal reconnection sizes/scrolls the drawer
## wrong while it still RENDERS, so the failure is silent; the fit path and its two signal reconnections
## (`subject_body.minimum_size_changed` and `viewport.size_changed.bind(true)` — the force-past-the-gate
## flag) are wired in HudLayer's `_ready` exactly as before, only repointed here.
##
## Word tables, formats and thresholds live in the topic vocab modules (`HudConst` / the matching
## `Hud*Vocab`) and the shared `DetailFormat` layer, read as `Module.X` — this file reaches for
## `HudConst` / `HudSelectionVocab` / `HudWorkVocab` / `HudExpeditionVocab` / `HudFloraVocab` /
## `DetailFormat` — so a phrase is still typed in exactly one place.

# --- Collaborators handed in by HudLayer (the SAME instances it holds) ---
var _selection: HudSelectionState = null
var _band_labor: HudBandLaborState = null
# Roster/emptiness reads (`tile_contents_unseen`) + the vitals row's `selected_terrain_label`.
var _selectioncard: SelectionCardController = null
# The compose half's drawer-action fill (`build_forage_drawer_actions` / `build_herd_drawer_actions`)
# and the per-snapshot sheet refresh (`refresh_compose_sheet`).
var _drawercompose: DrawerComposeController = null
# The band/city panel: the player-band fork (`has_panel` / `render_band`), the flat host's three zone
# builders, and the parties recall confirm.
var _bandpanel: BandPanelController = null
# The stateful band/party detail-line producer behind the occupant drawer (`unit_summary_lines`).
var _banddetail: BandDetailLines = null
# The HUD CanvasLayer — a `RefCounted` has no `get_tree()`, so the fit's frame wait goes through it.
var _host: Node = null

# --- The command-targeting cluster (see the class header) ---
# The drawer's Move button `.connect()`s straight to `_targeting.begin_move_band`.
var _targeting: TargetingController = null

# --- Scene nodes (handed in by HudLayer; they keep their `@onready` there — a `%Name` node loses
#     `unique_name_in_owner` if reparented, so the nodes stay put and the controller only writes them) ---
var _tile_detail: RichTextLabel = null
var _occupant_detail: RichTextLabel = null
var _allocation_panel: VBoxContainer = null
var _herd_assign_controls: VBoxContainer = null
var _forage_assign_controls: VBoxContainer = null
var _subject_body: VBoxContainer = null
var _subject_scroll: ScrollContainer = null
# The fit ceiling — read only, the room the drawer may claim in the dock beneath the card.
var _left_dock_scroll: ScrollContainer = null

# --- Owned state (moved off HudLayer, all drawer-only) ---
# One drawer fit in flight at a time — see `fit_subject_drawer`.
var _subject_fit_pending: bool = false
# The last land-drawer BBCode line array (skips a same-lines BBCode reparse) and the last-applied
# drawer content height (skips a same-height reflow).
var _tile_detail_lines_cache: Array = []
var _subject_fit_last_height: float = NAN

func _init(selection: HudSelectionState, band_labor: HudBandLaborState,
        selectioncard: SelectionCardController, drawercompose: DrawerComposeController,
        bandpanel: BandPanelController, banddetail: BandDetailLines, host: Node,
        tile_detail: RichTextLabel, occupant_detail: RichTextLabel, allocation_panel: VBoxContainer,
        herd_assign_controls: VBoxContainer, forage_assign_controls: VBoxContainer,
        subject_body: VBoxContainer, subject_scroll: ScrollContainer, left_dock_scroll: ScrollContainer,
        targeting: TargetingController) -> void:
    _selection = selection
    _band_labor = band_labor
    _selectioncard = selectioncard
    _drawercompose = drawercompose
    _bandpanel = bandpanel
    _banddetail = banddetail
    _host = host
    _tile_detail = tile_detail
    _occupant_detail = occupant_detail
    _allocation_panel = allocation_panel
    _herd_assign_controls = herd_assign_controls
    _forage_assign_controls = forage_assign_controls
    _subject_body = subject_body
    _subject_scroll = subject_scroll
    _left_dock_scroll = left_dock_scroll
    _targeting = targeting

## Player-faction check for a roster/drawer band (a trivial private copy of HudLayer's, the
## SelectionCardController / BandPanelController precedent — a one-line predicate is not worth a Callable).
func _is_player_unit(unit: Dictionary) -> bool:
    return int(unit.get("faction", HudConst.PLAYER_FACTION_ID)) == HudConst.PLAYER_FACTION_ID

# ---- The drawer render dispatch --------------------------------------------------------------

## The single drawer, filled by whichever subject row is lit. Exactly one of the three content
## paths is visible at a time — that is what bounds the card's height.
##
## **`from_selection` SAYS WHETHER THE PLAYER JUST PICKED THIS OCCUPANT**, and it is the whole of what
## the band branch below uses to tell a SELECTION from a RESTATE. Only `Hud.show_unit_selection`'s
## render passes `true`; every re-render for a reason other than the player's pick — a disclosure
## caret flipping its hosts, the per-snapshot `reapply_selection`, a pending-edit restate — takes the
## default, because the faction page must survive all three.
func render_subject_drawer(from_selection: bool = false) -> void:
    if _selection.subject() == HudSelectionState.SUBJECT_LAND:
        _render_land_drawer()
    else:
        _render_occupant_drawer(from_selection)
    # An OPEN compose sheet re-renders IN PLACE against the fresh subject. This is the SNAPSHOT path
    # (`reapply_selection` → here, every turn), and it must NOT close the sheet — closing would make
    # it unusable under autoplay (§15). A SELECTION change has already closed the sheet by the time it
    # reaches here, so this is a no-op there.
    _drawercompose.refresh_compose_sheet()
    fit_subject_drawer()

## **A FORECAST ANSWER LANDED — redraw only if it is about the party this drawer is showing.**
##
## A query triggers no snapshot, so `ForecastQuery.answered` is the only thing that can tell the
## occupant drawer its `Collapse:` row has an answer. It is deliberately SUBJECT-SCOPED rather than an
## unconditional `render_subject_drawer()`: the same signal fires on every reply to a compose sheet's
## stepper, and rebuilding this drawer (which re-runs the height fit) on each of those would reflow the
## card under a player who is composing something else entirely.
##
## The subject is recomposed from the shown party the same way the ask composes it, so a renamed or
## re-targeted party simply does not match and nothing is redrawn.
func on_forecast_answered(subject: String) -> void:
    var unit := _selection.unit()
    if unit.is_empty() or not bool(unit.get("is_expedition", false)):
        return
    if String(unit.get("expedition_mission", "")) != HudExpeditionVocab.EXPEDITION_MISSION_DENY:
        return
    var shown := ForecastQuery.subject_of(ForecastQuery.KIND_DENIAL_RAID,
        int(unit.get("band_id", HudConst.NO_BAND_ID)),
        String(unit.get("expedition_target_herd", "")).strip_edges())
    if shown == subject:
        render_subject_drawer()

## The LAND drawer: the terrain rows + the "Assign foragers" compose block (the land's only action).
## On a hex the player cannot see it also carries the unknown-contents statement — see below.
func _render_land_drawer() -> void:
    if _tile_detail == null:
        return
    # Skip the `.text` reassignment (and its implicit BBCode reparse + `minimum_size_changed`) when
    # the terrain lines are identical to last render — the common per-snapshot restate of the same
    # hex, where only numbers on OTHER widgets moved.
    var lines := _tile_terrain_lines(_selection.tile_info())
    # HIDDEN WHEN IT HAS NO ROWS, which since the FoW copy pass is a state that actually occurs: an
    # UNEXPLORED hex produces none at all (nothing about that ground is knowable, and its one
    # sentence is the roster note below). A visible empty RichTextLabel is not free — it still takes
    # its line height and the drawer's separation, so it would read as a blank gap between the land
    # row and that note.
    _tile_detail.visible = not lines.is_empty()
    if lines != _tile_detail_lines_cache:
        # No context: the LAND has no band behind it, and every tint its rows take (Sight,
        # Habitability, Ecology, Cultivation, Field) is a pure function of the row's own value.
        _tile_detail.text = DetailFormat.detail_bbcode(lines)
        _tile_detail_lines_cache = lines.duplicate()
    _drawercompose.build_forage_drawer_actions(_selection.tile_info())
    if _allocation_panel != null:
        _allocation_panel.visible = false
    if _herd_assign_controls != null:
        _herd_assign_controls.visible = false
    # FORCED when the terrain rows came back empty — see `_render_unknown_contents_note`. With no
    # rows, no compose block and no note, every child of the drawer is hidden and the land subject
    # renders as a blank capped area under the divider.
    _render_unknown_contents_note(lines.is_empty())

## An EMPTY occupant list is a claim of emptiness the client cannot back up, so on a hex the player
## cannot see the list carries the land row and nothing else, and the drawer says so out loud. This
## is the whole point of the fog gate — silence would read as "nothing here".
##
## TWO INVARIANTS MEET HERE, AND `force` IS WHAT SATISFIES BOTH: the card never states the same
## unseen-contents sentence twice, AND the LAND drawer on an unseen hex is never empty.
##
## Skipped when the list DOES carry occupant rows, because there the sentence is already said: that
## only happens for your own party on an unseen hex, and `_rebuild_subject_list` appends
## `OCCUPANTS_UNSEEN_OTHERS_HINT` to the list in exactly that case.
##
## **UNLESS `force`, which `_render_land_drawer` passes when the drawer produced NO terrain rows.**
## An UNEXPLORED hex produces none at all, and it routinely carries roster rows — the sim excludes
## expeditions from fog reveal, so your own party stands on unexplored ground as a matter of course.
## Suppressing the note there hid the last visible child of the drawer and left a blank gap where the
## land's whole content should be, so with nothing else to render the note renders regardless of the
## roster; the hint on the list above is a different sentence about the OTHER occupants.
func _render_unknown_contents_note(force: bool) -> void:
    if _occupant_detail == null:
        return
    var unseen := _selectioncard.tile_contents_unseen(_selection.tile_info())
    var roster_empty := _selection.roster_units().is_empty() and _selection.roster_herds().is_empty()
    if not unseen or not (roster_empty or force):
        _occupant_detail.visible = false
        _occupant_detail.text = ""
        return
    _occupant_detail.visible = true
    var message := HudConst.OCCUPANTS_UNKNOWN_UNEXPLORED \
        if String(_selection.tile_info().get("visibility_state", "")) == HudConst.VISIBILITY_UNEXPLORED \
        else HudConst.OCCUPANTS_UNKNOWN_REMEMBERED
    _occupant_detail.text = DetailFormat.detail_bbcode([message])

## Cap the drawer against the room left in the dock beneath the card, so a crowded hex scrolls
## INSIDE the drawer rather than dragging the whole dock.
##
## WAITS A WHOLE FRAME, not just `call_deferred`, and that is load-bearing. The drawer's content
## height is a function of its WIDTH — the detail label wraps, and the card's width is itself set by
## whichever compose block is showing — so a measurement taken before the new subject has been laid
## out reports the PREVIOUS subject's wrapping. On a card that just got narrower that under-reports
## the height and the drawer caps short with a scrollbar over content that would have fit. A
## deferred call is flushed inside the same frame and is not enough; one `process_frame` is.
## Coalesced, so the render + the body's own `minimum_size_changed` collapse into one fit. The frame
## wait is threaded through the injected HOST — a `RefCounted` has no `get_tree()`.
func fit_subject_drawer(force: bool = false) -> void:
    if _subject_scroll == null or _subject_body == null or _subject_fit_pending:
        return
    _subject_fit_pending = true
    await _host.get_tree().process_frame
    _subject_fit_pending = false
    if _subject_scroll == null or _subject_body == null:
        return
    # Once the teardown/rebuild flash is gone, a same-structure restate settles to the SAME content
    # height, so the awaited resize (which reflows the drawer) is pure churn — skip it unless the
    # height actually moved, or a caller FORCES it because the dock ROOM changed (window resize, feed
    # toggle) while the content did not.
    var content_height := _subject_body.get_combined_minimum_size().y
    if not force and is_equal_approx(content_height, _subject_fit_last_height):
        return
    _subject_fit_last_height = content_height
    DockScrollFit.fit_height(
        _subject_scroll,
        content_height,
        _left_dock_scroll,
        HudSelectionVocab.SUBJECT_DRAWER_MIN_HEIGHT,
        HudSelectionVocab.SUBJECT_DRAWER_BOTTOM_MARGIN,
    )

# ---- The land-drawer content producer -------------------------------------------------------

## The LAND DRAWER's rows: only what a CHIP CANNOT CARRY.
##
## The pinned chip strip above the list already states this tile's standing condition — Sight,
## Habitability, Climate, Tags, Site — so printing those as rows here restated the strip verbatim,
## and `Biome` restated the land ROW's own label (the "no restated identity" rule,
## docs/plan_tile_panel_layout.md §8). The chips REPLACE those rows; what is left is the numbers and
## the stocks, whose subject is the land: Height · the rivers · the two food webs' stocks and the
## basket that decomposes the human one · the committed crop and the two build meters.
##
## **AND NO FoW SENTENCE AT ALL.** This producer emits ROWS; each unseen state's one sentence is the
## roster's own unknown-contents note (`_render_unknown_contents_note`, rendered directly beneath
## this label), so a sentence here would be that sentence twice — see the two branch comments below.
##
## **THE TWO FOOD WEBS READ AS A PAIR, FORAGING THEN GRAZING, WITH NOTHING BETWEEN THEM.** Each is one
## row named for who eats it, carrying its stock and its ecology phase; the human layer's basket hangs
## indented beneath its row. They used to be four rows interleaved around the module row and the
## basket — the human layer split in half by the animal one, under names that inverted each other —
## and a playtest reader mistook one for the other three times. See `HudFloraVocab.FORAGING_KEY`.
##
## **THE `Forage:` MODULE ROW IS GONE.** `Riverine / Delta — River Garden` named a CATEGORY the player
## can neither choose nor change, and the basket beneath it says the same thing in the terms a decision
## is made in (which plants, what share, how much of each). Nothing replaced it; the module still
## drives the land row's glyph and the sim's yield.
##
## `_render_land_drawer` is the ONE caller (the map hover tooltip builds its own text in
## `show_tooltip`), so the trim is local to the drawer.


## The box the basket rows' crop-role art is drawn in — the HOST LABEL's own resolved font size, so
## a mark is the size of the text it leads and tracks it if that size ever changes.
##
## **DERIVED, NEVER A CONSTANT** — the same rule the discoveries strip follows when it boxes a
## `WonderSprites` texture ("boxed to the label's derived `get_theme_font_size`, never a hardcoded
## pixel size"). `%TileDetail` sets no font-size override, so this reads the stock theme value;
## writing that number here instead would silently stop tracking the day one is added, and it is
## also the figure the art's own legibility was judged at.
##
## `0` when there is no label yet, which `FoodIcons.for_crop_role` reads as "text only" — the
## honest answer, since with no label there is no size to match and the emoji still renders.
func _role_icon_px() -> int:
    if _tile_detail == null:
        return 0
    return _tile_detail.get_theme_font_size("normal_font_size")


func _tile_terrain_lines(tile_info: Dictionary) -> Array[String]:
    var lines: Array[String] = []
    if tile_info.is_empty():
        lines.append("Hover or click a tile to inspect details.")
        return lines
    # Fog of War: never-seen tiles reveal nothing; remembered (Discovered) tiles
    # show only their last-known terrain, not current contents. See MapView
    # _apply_visibility_to_info, which redacts the hidden fields before this runs.
    #
    # NEITHER UNSEEN STATE ADDS A SENTENCE HERE — the drawer emits ROWS, and the one sentence each
    # state gets is the roster's own unknown-contents note (`_render_unknown_contents_note`, which
    # renders directly beneath this label). An unexplored hex used to append `Not yet scouted — send
    # a band to reveal this area.` immediately above `Nobody has been here. Send a band to reveal
    # what's on this ground.`, which is the same sentence twice — with the `Unexplored` chip pinned
    # above saying it a third time. Exactly the duplication the remembered branch below had.
    var visibility_state := String(tile_info.get("visibility_state", ""))
    if visibility_state == HudConst.VISIBILITY_UNEXPLORED:
        return lines
    if tile_info.has("height_display"):
        lines.append("Height: %s" % String(tile_info["height_display"]))
    # Hex-edge rivers — which SIDES of this tile carry water (the sides a crossing cost will
    # apply to). Terrain-intrinsic permanent geography, so it renders before the discovered
    # early-return, like Pasture below. Guarded on the key so a rehydrated snapshot
    # degrades to no row instead of a wrong one; RiverEdges returns [] on a riverless tile, so it
    # never emits an empty "River:" label. Same formatter the map hover tooltip uses.
    if tile_info.has("river_edges"):
        lines.append_array(RiverEdges.summary_lines(int(tile_info["river_edges"])))
    # (A discovered Wondrous Site is a standing condition of the ground — it rides the chip strip.)
    #
    # A REMEMBERED TILE KEEPS BOTH WEBS' CAPACITIES AND LOSES BOTH THEIR STOCKS (issue #462). The rule
    # cuts across the two webs, not between them: a capacity is recomputed from the tile every turn
    # and no player action moves it, so the value we hold for an unseen hex is the value that hex last
    # showed; a biomass moves every turn as the ground is grazed or gathered, so a remembered reading
    # of it is stale by construction. It reads `Foraging: — / 205` over `Grazing: — / 130` — the same
    # two rows in the same order as the live card, which is what keeps the pair's POSITIONAL meaning
    # intact and is why this stays an explicit branch rather than a consequence of the redaction: the
    # card states what it knows because we decided what it knows, not because a key happened to be
    # missing. `MapView.FOW_DISCOVERED_HIDDEN_KEYS` carries the other half of this rule.
    #
    # It renders WITHOUT the basket, and that follows from the same fact. Each basket row states the
    # biomass its share amounts to, so with no stock to decompose the rows would be the free-floating
    # "three more resources" list the layout exists to stop (`land-readouts.md` → the basket).
    #
    # IT ENDS AT THE ROWS — there is no "Last seen — information incomplete. Scout to update." line
    # any more. It was the SECOND sentence on the card saying the hex is remembered (the drawer's own
    # `OCCUPANTS_UNKNOWN_REMEMBERED` note says it below, and the pinned `Remembered` chip says it a
    # third time above), and both sentences closed with a promise scouting cannot keep: scouting makes
    # a hex DISCOVERED, which is the state being described — seeing current contents needs SIGHT, a
    # band standing there now. The `— / K` rows carry "this number is unknown" on the datum itself,
    # which is the job that line was failing to do.
    # **THE REMEMBERED BRANCH DOES NOT TAKE THE GATHERING-SITE GATE BELOW**, and cannot: `food_module`
    # is in `FOW_DISCOVERED_HIDDEN_KEYS`, so a remembered tile has no reading to gate on, and inferring
    # "not a site" from the redacted key would silently drop the `Foraging: — / K` row from every
    # remembered hex — the exact card #462 built `_assert_fog_stock_parity` to prevent. A remembered
    # card states each web's CAPACITY and withholds its stock; whether the ground can be worked is a
    # question about the present, which is precisely what a remembered tile does not know.
    var stock_known := visibility_state != HudConst.VISIBILITY_DISCOVERED
    var graze_lines := _graze_stock_lines(tile_info, stock_known)
    if not stock_known:
        lines.append_array(_forage_stock_lines(tile_info, false))
        lines.append_array(graze_lines)
        return lines
    # FORAGING — the HUMAN-edible stock, and the first of the pair. Standing biomass over the patch's
    # ceiling, with the ecology phase inline: the phase is a condition OF this stock and gates whether
    # cultivation can accrue at all, so it belongs on the stock's own row rather than on a second row
    # named so much like the pasture's that the two inverted each other. The row's own
    # "no patch here → no row" test lives in `_forage_stock_lines`, beside the pasture's; this local
    # copy of the capacity is the BASKET's guard, which needs the same answer one level out.
    # **AND ONLY WHERE ANYONE CAN GATHER** (issue #464). The plant web's stand is a stock a *person*
    # can eat, and the sim's plant rungs 1–3 all require a gathering site — so on ground that is not
    # one, `Foraging 195 / 195 · Thriving` over a basket led by Wild Emmer describes a larder nobody
    # can open. Every signal in that block reads as an invitation (full, Thriving, the best food crop
    # in the game) while the land row two rows up says `No forage` and the drawer offers no way to
    # work it: the card was arguing with itself, and the stand was the half that was lying.
    #
    # **The row is an AFFORDANCE, not a property.** `Foraging` is a VERB, and a verb label on ground
    # where the verb is impossible is the contradiction — so the block appears when the verb does.
    # When rung 4 (Farm) drops `requires_gathering_site`, more ground qualifies and the block returns
    # there, which is the discovery that rung is made of.
    #
    # **The animal web is untouched and that is the point**: `Grazing` still renders here, so the card
    # never goes silent about ground that feeds herds — what disappears is only the claim that PEOPLE
    # can eat it. Fodder needs no forage action, and it keeps its row.
    var gathering_site := DetailFormat.tile_is_gathering_site(tile_info)
    var patch_capacity := float(tile_info.get("patch_carrying_capacity", 0.0))
    # Hoisted because BOTH halves read it: the stock row states it against the capacity, and the
    # basket below decomposes it. Reading it twice is how the row and its own decomposition would
    # start describing different stands.
    var patch_biomass := float(tile_info.get("patch_biomass", 0.0))
    var crop_species := String(tile_info.get("patch_committed_species", "")).strip_edges()
    if gathering_site and patch_capacity > 0.0:
        lines.append_array(_forage_stock_lines(tile_info, true))
        # …AND WHAT THAT STOCK IS MADE OF — one indented row per realized plant, always visible and
        # never behind a disclosure, each led by an icon for what the plant is FOR (staple / cash /
        # fodder) and closing with the biomass its share amounts to. The indent is what says these
        # decompose the row above; the icons are what make "62% of what grows here is not food"
        # legible at a glance; the absolutes are what let the rows visibly sum to the stock. The
        # committed member is marked in SIGNAL so the eye joins it to the `Crop:` row below.
        #
        # NESTED UNDER THE CAPACITY GUARD DELIBERATELY. The basket itself is biome-derived and
        # survives fog, but a share is only a share OF something: with no `Foraging` row above them
        # and no capacity to state each plant's biomass, these rows would be exactly the free-floating
        # "three more resources" list this layout exists to stop being.
        # **THE STANDING STOCK, not the capacity.** These rows say what the `205 / 205` above them is
        # MADE OF, so on a drawn-down patch reading `90 / 100` they must sum to 90 — splitting the
        # ceiling instead would decompose a full patch nobody is looking at, and the card would hold
        # two numbers disagreeing about which stand is under discussion.
        lines.append_array(DetailFormat.flora_composition_lines(
            tile_info.get("patch_composition", []), crop_species, patch_biomass, _role_icon_px()))
    # GRAZING — the animal-edible stock, directly under Foraging. Same shape, same phase-inline rule.
    # The adjacency IS the point: what HUMANS can eat here (seeds, nuts, tubers — food-module tiles
    # only) against what ANIMALS can eat here (grass and browse — cellulose people cannot digest, on
    # nearly every land tile). Your best farm is usually not your best pasture, and a comparison the
    # player cannot make in one glance is not a comparison.
    lines.append_array(graze_lines)
    # THE COMMITTED CROP — what the band committed this patch to, recorded on the FIRST worked turn,
    # ~25 turns before the build lands. It is a different fact from the basket above: committing
    # REWEIGHTS the basket as the build completes (issue #433 — a Tended Patch lifts the favored share
    # to `min(1, share x tended_weeding_gain)` off the least abundant members, a Field forces it to
    # 1.0) and until then the basket has not moved at all, so rendering only this row once claimed a
    # 64/36 tile was 100% emmer the instant the order was given. It reads with the BUILD METERS below
    # rather than beside the basket, because what it states is the standing INVESTMENT on this ground,
    # not part of the stock pair above; the SIGNAL mark inside the basket is what joins the two.
    var crop_name := String(tile_info.get("patch_committed_display_name", "")).strip_edges()
    if crop_species != "" and crop_name != "":
        lines.append("%s: %s" % [HudFloraVocab.FLORA_CROP_ROW, crop_name])
    # Forage-patch intensification ladder: while a patch is being tended it shows the
    # cultivation progress; once cultivated it reads as a "Tended Patch" (SIGNAL tint).
    # Mirrors the herd Husbandry row. Only when the snapshot carries the field so we
    # never invent a state on a patch that isn't being worked.
    # WHICH RUNG THE PLAYER IS ACTUALLY BUILDING HERE — folded across every band, not just the panel
    # one, because a patch several bands can reach may be worked by none of them in particular. It is
    # what separates a meter that is FILLING from one that is BLEEDING, which no percentage can show:
    # an abandoned improvement reverts, and it used to wear the build's own word and ink while doing it.
    var building_rung := String(_band_labor.forage_effort_at(
        int(tile_info.get("x", -1)), int(tile_info.get("y", -1))).get("improvement", ""))
    # **…AND WHETHER ANYBODY IS ACTUALLY ON IT.** A build has its OWN crew now
    # (`docs/plan_standing_upkeep.md` §2.2), so a rung can be declared and unmanned — which is a
    # THIRD thing from filling and from bleeding, and read as neither: at a meter of zero the row
    # below is not rendered at all, and above zero it wore the build's own word and neutral ink.
    # Confirmed-row only, deliberately, so a just-committed build cannot flash this warning — see
    # `HudBandLaborState.unstaffed_build_forage`. It composes safely with the pending-aware
    # `building_rung` above: a fresh commit has no confirmed declaration yet, so this answers nothing.
    var unstaffed_rung := _band_labor.unstaffed_build_forage(
        int(tile_info.get("x", -1)), int(tile_info.get("y", -1)))
    # **THE METER SAYS WORK, NOT JUST PERCENT** (`docs/plan_unit_costed_work.md` §11). A rung declares
    # a fixed size in work units and a crew produces work units per turn, so the same 42% is a
    # different job on every rung — which a bare percentage cannot say. The absolutes come off the
    # patch's own `patch_`-prefixed pair and the percentage stays the meter the row always read.
    var prefix: String = HudComposeVocab.FORAGE_FORECAST_PREFIX
    if bool(tile_info.get("patch_is_cultivated", false)):
        lines.append("Cultivation: %s" % DetailFormat.cultivation_label(1.0, true))
    elif unstaffed_rung == SourceForecast.IMPROVEMENT_CULTIVATE \
            and float(tile_info.get("patch_cultivation_progress", 0.0)) <= 0.0:
        # DECLARED AND UNMANNED, with nothing banked — the row the `> 0` gate below suppressed, which
        # is exactly the state that needs saying (the map was drawing a `0%` badge over it).
        lines.append("Cultivation: %s" % DetailFormat.BUILD_UNSTARTED_VALUE)
    elif tile_info.has("patch_cultivation_progress"):
        var cultivation_progress := float(tile_info["patch_cultivation_progress"])
        if cultivation_progress > 0.0:
            # **BUILDING MEANS *SOMEBODY IS ON IT*, not merely *somebody declared it*.** The build has
            # its own crew, so a declaration with no builders leaves the meter bleeding exactly as an
            # abandoned one does — and wore the filling state's word and ink while doing it.
            var cultivating := building_rung == SourceForecast.IMPROVEMENT_CULTIVATE \
                and unstaffed_rung != SourceForecast.IMPROVEMENT_CULTIVATE
            lines.append("Cultivation: %s" % DetailFormat.cultivation_label(cultivation_progress,
                false, cultivating,
                SourceForecast.build_work_done(
                    tile_info, prefix, SourceForecast.IMPROVEMENT_CULTIVATE),
                SourceForecast.build_work_cost(
                    tile_info, prefix, SourceForecast.IMPROVEMENT_CULTIVATE)))
            # **THE TURN ESTIMATE AND THE GEAR'S SAVING HANG OFF THE RUNG ACTUALLY BEING BUILT.** Both
            # wire fields are per SOURCE (at most one improvement is ever in flight on one), so they
            # attach to whichever meter a crew is filling and to nothing else — under a REVERTING
            # meter they would describe a build nobody is doing.
            if cultivating:
                lines.append_array(DetailFormat.build_estimate_lines(tile_info, prefix))
    # PLANT RUNG 3 — the Field, on its OWN row beside Cultivation. The patch carries TWO independent
    # build meters (a Field may stand on ground that was never tended: seed travels, so `Sow` needs no
    # prior patch), so they are two rows, never one merged "progress" number. This is the per-source
    # half of the two-meter split (§4.1) — the FACTION's Seed Selection knowledge is NOT shown here;
    # it lives in the top-bar knowledge strip, because it is a property of your people, not of this
    # ground. Both rows are the source's own, and both decay if the patch is abandoned.
    if bool(tile_info.get("patch_is_field", false)):
        lines.append("%s: %s" % [HudFloraVocab.FIELD_ROW, DetailFormat.field_label(1.0, true)])
    elif unstaffed_rung == SourceForecast.IMPROVEMENT_SOW \
            and float(tile_info.get("patch_field_progress", 0.0)) <= 0.0:
        # The Sow twin of the Cultivation branch above.
        lines.append("%s: %s" % [HudFloraVocab.FIELD_ROW, DetailFormat.BUILD_UNSTARTED_VALUE])
    elif tile_info.has("patch_field_progress"):
        var field_progress := float(tile_info["patch_field_progress"])
        if field_progress > 0.0:
            var sowing := building_rung == SourceForecast.IMPROVEMENT_SOW \
                and unstaffed_rung != SourceForecast.IMPROVEMENT_SOW
            lines.append("%s: %s" % [HudFloraVocab.FIELD_ROW, DetailFormat.field_label(
                field_progress, false, sowing,
                SourceForecast.build_work_done(tile_info, prefix, SourceForecast.IMPROVEMENT_SOW),
                SourceForecast.build_work_cost(tile_info, prefix, SourceForecast.IMPROVEMENT_SOW))])
            if sowing:
                lines.append_array(DetailFormat.build_estimate_lines(tile_info, prefix))
    # **WHAT IT COSTS TO HOLD WHAT IS BUILT HERE, and how long it has if nobody pays**
    # (`docs/plan_standing_upkeep.md` §2). It sits under the two rung rows rather than beside them
    # because it is a property of the SOURCE at whatever rung it stands on, not of either meter — and
    # a patch with nothing built owes nothing and prints no row at all.
    lines.append_array(DetailFormat.upkeep_lines(tile_info, prefix,
        SourceForecast.SOURCE_KIND_FORAGE))
    return lines

## The FORAGING row (or nothing) — the human-edible web's stock over its ceiling. The exact twin of
## `_graze_stock_lines` below, and it is a helper for that reason: the two webs are one rule rendered
## twice, so each branch of `_tile_terrain_lines` calls the pair and neither can grow a shape the
## other lacks (issue #462 was precisely that drift).
##
## Empty when the ground carries no patch at all (`patch_carrying_capacity <= 0`): a moduleless tile
## prints nothing, never a "0 / 0" that would read as an exhausted stand rather than an absent one.
func _forage_stock_lines(tile_info: Dictionary, stock_known: bool) -> Array[String]:
    var lines: Array[String] = []
    var capacity := float(tile_info.get("patch_carrying_capacity", 0.0))
    if capacity <= 0.0:
        return lines
    lines.append("%s: %s" % [HudFloraVocab.FORAGING_KEY, _stock_value(
        float(tile_info.get("patch_biomass", 0.0)), capacity,
        String(tile_info.get("patch_ecology_phase", "")), stock_known)])
    return lines

## The GRAZING row (or nothing), built once and emitted by BOTH visibility branches — the remembered
## tile, where it states a capacity alone, and the live tile, where it states the full stock under
## `Foraging`. Extracted so the two branches cannot drift into rendering the animal layer two
## different ways.
##
## Empty when the ground carries no pasture at all (`graze_capacity <= 0`, mirroring the sim's
## `GrazeRegistry`): a glacier prints nothing, never a "0 / 0" that would read as a starved pasture
## rather than an absent one.
func _graze_stock_lines(tile_info: Dictionary, stock_known: bool) -> Array[String]:
    var lines: Array[String] = []
    var graze_capacity := float(tile_info.get("graze_capacity", 0.0))
    if graze_capacity <= 0.0:
        return lines
    lines.append("%s: %s" % [HudFloraVocab.GRAZING_KEY, _stock_value(
        float(tile_info.get("graze_biomass", 0.0)), graze_capacity,
        String(tile_info.get("graze_ecology_phase", "")), stock_known)])
    return lines

## One food web's stock row VALUE — `205 / 205 · Thriving`, or the bare `205 / 205` where the wire
## states no phase. Shared by both webs so the pair can never read in two different shapes, and the
## phase goes through the same `DetailFormat.ecology_phase_label` a herd's Ecology row uses; the
## amber/red tint follows from `DetailFormat._value_hex`, which keys both row names to
## `ecology_value_hex` and matches the phase word wherever in the value it sits.
##
## `stock_known == false` is the REMEMBERED tile: `— / 205`, the capacity alone. **The phase goes with
## the biomass and is never rendered here**, because a phase is `classify_ecology_phase`'s reading OF
## that biomass — printing `Thriving` beside an unknown stock would state the very thing the em-dash
## just declined to. It is passed as a FLAG rather than inferred from an absent key so that the row
## says what it says by decision; a fixture that leaks a redacted key must not silently restore a
## live-looking reading (`.claude/rules/client/land-readouts.md` → "Fog splits a stock from its
## capacity").
func _stock_value(biomass: float, capacity: float, phase: String, stock_known: bool) -> String:
    if not stock_known:
        return HudFloraVocab.STOCK_UNKNOWN_FORMAT % capacity
    var stock := HudFloraVocab.STOCK_FORMAT % [biomass, capacity]
    var normalized := phase.strip_edges().to_lower()
    if normalized == "":
        return stock
    return HudFloraVocab.STOCK_PHASE_CLAUSE_FORMAT % [
        stock, DetailFormat.ecology_phase_label(normalized)]

# ---- The occupant drawer + its %AllocationPanel branches --------------------------------------

## The detail drawer + action buttons for the currently-selected occupant. Shares the one drawer
## with the land, so it hides the land's content first — exactly one subject fills it.
##
## `from_selection` is `render_subject_drawer`'s — see the band branch below, which is its only reader.
func _render_occupant_drawer(from_selection: bool = false) -> void:
    if _occupant_detail == null:
        return
    if _tile_detail != null:
        _tile_detail.visible = false
    if _forage_assign_controls != null:
        _forage_assign_controls.visible = false
    # This render's tint context, constructed LOCALLY: the band line producers below fill it as they
    # emit rows, and it is handed to the formatter at the bottom. Nothing outlives this call.
    var ctx := DetailFormat.Context.new()
    var is_band := not _selection.unit().is_empty()
    var is_herd := not _selection.herd().is_empty()
    var is_expedition := is_band and bool(_selection.unit().get("is_expedition", false))
    var is_player_band := is_band and not is_expedition and _is_player_unit(_selection.unit())
    # A selected player band is the panel's subject: its detail + labor allocation render into the
    # dockable Band/City panel (docs/plan_band_city_dock.md §3), and the Occupants card shows NO
    # band detail (the roster still lists it). Falls back to the legacy in-card drawer only when no
    # panel is injected (e.g. the HUD-only ui_preview harness).
    if is_player_band and _bandpanel.has_panel():
        # **A SELECTION WINS; A PASSIVE RE-RENDER DOES NOT.** This branch re-asserts the selected band
        # as the panel's subject on EVERY render, and the drawer is re-rendered for reasons that have
        # nothing to do with the selection — a disclosure toggle re-renders its hosts so a caret can
        # flip, and a snapshot restates the whole card every turn. So with a band selected, opening any
        # faction row threw the page away and landed on that band. Reported from play; it is the second
        # half of the same defect `_refresh_disclosure_hosts` carried, and it hid behind the first
        # because both need a band SELECTED to show up.
        #
        # **A BARE "not on the faction page" GATE WAS TOO WIDE**, though, and that was the other half of
        # the report: with the page up, clicking a player band's marker moved the map ring and rendered
        # the pointer below while the panel went on showing the rollup, so only the cycler could leave
        # the page. `from_selection` is what tells the two apart — the player picking an occupant is the
        # explicit "make this the subject" act, and it wins exactly as the cycler's ▶ does.
        # `render_band` clears the page flag itself, so nothing else is needed here.
        #
        # The panel is deliberately decoupled from the selection already ("selecting a herd or an empty
        # tile leaves `panel_band` intact — the panel persists across selection changes"); this branch
        # was the one exception, and the faction page is where that exception starts doing damage.
        if from_selection or not _bandpanel.is_faction_page():
            _bandpanel.render_band(_selection.unit())
        # The drawer is now VISIBLE furniture rather than a hidden card, so an empty one reads as a
        # rendering fault. Point at where the band's detail actually went instead of leaving a gap.
        _occupant_detail.visible = true
        _occupant_detail.text = DetailFormat.detail_bbcode([HudSelectionVocab.BAND_PANEL_POINTER_TEXT])
        # The one order that stays HERE (§18): repositioning is a map action. Player resident bands
        # only — this branch is already player-band-gated, and a foreign band's orders aren't ours.
        _build_band_move_actions()
        if _herd_assign_controls != null:
            _herd_assign_controls.visible = false
        return
    # Herd / expedition / non-player band (or no-panel fallback) → the Occupants card drawer,
    # unchanged. Expedition → Recall/Move panel; player band (fallback) → allocation panel; herd →
    # assign-hunters controls. All mutually exclusive with the current selection.
    _occupant_detail.visible = true
    var lines: Array[String] = []
    if not _selection.unit().is_empty():
        # A launched DENIAL party's `Collapse:` row is a forecast QUERY, and the ask belongs to a
        # controller (`BandPanelController.launched_party_denial_view` — one seam, one request-id
        # stream). It answers `{}` for anything else, which renders no row at all.
        lines = _banddetail.unit_summary_lines(
            _selection.unit(), _selectioncard.selected_terrain_label(), ctx, false, true,
            _bandpanel.launched_party_denial_view(_selection.unit()))
    elif not _selection.herd().is_empty():
        # **NO KEEPER COUNT IS THREADED IN ANY MORE** (`docs/plan_standing_upkeep.md` §2.5). The
        # drawer used to be handed the `maintain` crews summed across the player's bands; maintenance
        # left the tile, so the herd's own published upkeep — its share of the band's husbandry pool —
        # is the whole of what the keeping rows read, and the producer resolves it from the herd dict.
        # **THE ONE LABOR FACT THE PURE PRODUCER CANNOT SEE IS THREADED IN**: a rung this faction has
        # declared on the herd and put nobody on, which is what makes the Husbandry / Corral row
        # render at a meter of zero instead of vanishing (`DetailFormat.BUILD_UNSTARTED_VALUE`).
        lines = DetailFormat.herd_summary_lines(_selection.herd(), _band_labor.world_herds(),
            _band_labor.unstaffed_build_hunt(String(_selection.herd().get("id", ""))))
    _occupant_detail.text = DetailFormat.detail_bbcode(lines, ctx)
    if is_expedition:
        _build_expedition_panel(_selection.unit())
    elif is_player_band:
        _build_allocation_panel(_selection.unit())
    elif _allocation_panel != null:
        _allocation_panel.visible = false
    if is_herd:
        _drawercompose.build_herd_drawer_actions(_selection.herd())
    elif _herd_assign_controls != null:
        _herd_assign_controls.visible = false

## Stack the three ZONE contents into `target` — the legacy flat host (the Occupants card's
## %AllocationPanel, used by the no-dock `ui_preview` harness). It renders exactly what the dock
## renders, through the SAME three builders (`BandPanelController.build_*_zone`); there is no second
## layout to maintain.
##
## It writes the drawer's `%AllocationPanel` node — HudLayer's, passed in — so it stays with the
## drawer render dispatch; the controller never needs a second host. Its two siblings on the same host
## (`_build_band_move_actions` / `_build_expedition_panel`) are branches of `_render_occupant_drawer`
## and live here for the same reason.
func _build_allocation_panel(band: Dictionary, target: VBoxContainer = null) -> void:
    var container: VBoxContainer = target if target != null else _allocation_panel
    if container == null:
        return
    HudWidgets.clear_children(container)
    var is_player := not band.is_empty() and _is_player_unit(band)
    container.visible = is_player
    if not is_player:
        return
    container.add_child(_bandpanel.build_band_zone(band, false))
    container.add_child(_bandpanel.build_work_zone(band))
    container.add_child(_bandpanel.build_parties_zone(band))
    # The docked path offers Move from `_build_band_move_actions`; this host must offer it too, or a
    # selected player band has no way to be moved at all here (see `_make_band_move_actions`).
    container.add_child(_make_band_move_actions())

## The selected PLAYER band's one drawer action (§18): Move. Shares the allocation-panel host with
## `_build_expedition_panel` and `_build_allocation_panel` — all three branches are mutually
## exclusive on the selected occupant, so the fallback path's own Orders Move is never doubled.
##
## Wired straight to `_targeting.begin_move_band`, which resolves through `_resolve_assign_band()` and so
## already targets the band selected in THIS list — the whole point on a hex carrying several.
## `Clear all` is deliberately NOT here: it returns every worker to idle, a heavier action that
## belongs beside the labor allocation it clears.
func _build_band_move_actions() -> void:
    if _allocation_panel == null:
        return
    for child in _allocation_panel.get_children():
        child.queue_free()
    _allocation_panel.visible = true
    _allocation_panel.add_child(_make_band_move_actions())

## The Move row itself, so the two hosts that offer it build the SAME control rather than two that
## can drift. **Both hosts must offer it**: the docked path adds it beside the panel pointer, and the
## NO-PANEL fallback appends it under the band content — the fallback used to inherit a Move from the
## allocation stack's Orders block, and when the Band panel rework deleted that block the fallback
## silently offered no way to move a band at all. `ui_preview`'s "exactly ONE Move button" assertion
## is what catches either half of that going wrong (none offered, or one offered twice).
func _make_band_move_actions() -> HBoxContainer:
    var actions := HBoxContainer.new()
    actions.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    var move_btn := Button.new()
    move_btn.text = HudSelectionVocab.MOVE_BAND_BUTTON_TEXT
    HudStyle.apply_button(move_btn, "ghost")
    move_btn.tooltip_text = HudSelectionVocab.MOVE_BAND_BUTTON_TOOLTIP
    move_btn.pressed.connect(_targeting.begin_move_band)
    actions.add_child(move_btn)
    return actions

## The dedicated panel for a selected in-flight expedition (no labor in v1): an awaiting-orders
## callout (echoing the pulsing map ring) plus Move (retarget via move_band on the expedition
## entity) and Recall. Reuses the allocation-panel host; player expeditions only.
func _build_expedition_panel(expedition: Dictionary) -> void:
    if _allocation_panel == null:
        return
    for child in _allocation_panel.get_children():
        child.queue_free()
    var is_player := not expedition.is_empty() and _is_player_unit(expedition)
    _allocation_panel.visible = is_player
    if not is_player:
        return
    var phase := String(expedition.get("expedition_phase", "")).strip_edges().to_lower()
    if phase == HudExpeditionVocab.EXPEDITION_PHASE_AWAITING:
        # THE ARRIVAL ANSWERS, named in the order the buttons below sit in.
        var callout := HudWidgets.alloc_hint_label(HudExpeditionVocab.EXPEDITION_AWAITING_CALLOUT)
        callout.add_theme_color_override("font_color", HudStyle.WARN)
        _allocation_panel.add_child(callout)
    var actions := HBoxContainer.new()
    actions.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
    var move_btn := Button.new()
    move_btn.text = "Move"
    HudStyle.apply_button(move_btn, "ghost")
    move_btn.tooltip_text = "Send the expedition onward, then click a target tile."
    move_btn.pressed.connect(_targeting.begin_move_band)
    actions.add_child(move_btn)
    # Already homeward-bound: the button reads its state ("Returning", disabled) rather than a
    # mysterious grayed-out "Recall". Otherwise it takes the verb the SIM will honour — `Cancel` for a
    # party still standing in its home camp, `Recall` for one in the field — off the same
    # `BandPanelController.recall_verb`/`recall_tooltip` pair the parties zone's row and inspector read,
    # so the three surfaces cannot disagree about one press.
    var returning := phase == HudExpeditionVocab.EXPEDITION_PHASE_RETURNING
    var recall_btn := Button.new()
    recall_btn.text = "Returning" if returning else _bandpanel.recall_verb(expedition)
    HudStyle.apply_button(recall_btn, "primary")
    recall_btn.tooltip_text = "Heading home — folds workers + provisions back on arrival." if returning \
        else _bandpanel.recall_tooltip(expedition)
    recall_btn.disabled = returning
    recall_btn.pressed.connect(func() -> void: _bandpanel.confirm_recall_expedition(expedition))
    actions.add_child(recall_btn)
    _allocation_panel.add_child(actions)
