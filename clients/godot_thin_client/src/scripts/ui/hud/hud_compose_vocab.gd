class_name HudComposeVocab

## Compose / party / send-expedition vocabulary — the tile & herd compose sheets, the parties zone,
## the hunt/forage previews, the investment-forecast strings and the cancel-scope grammar.

# Verb prefixes for the optimistic in-flight label on the disabled cancel button,
# composed with the task action phrase as "<verb> <phrase>…" (e.g. "Cancelling
# Market Hunt…", "Starting Foraging…"). Shown from dispatch until the snapshot
# confirms the band's `activity` CHANGED from its value at dispatch.
const CANCEL_ORDER_PENDING_VERB := "Cancelling"

const START_ORDER_PENDING_VERB := "Starting"

# Forage take policies reuse the hunt picker, but carry forage-appropriate behaviour hints
# (gathering a plant patch's regrowth, not culling a herd).
const FORAGE_POLICY_HINTS := {
    "sustain": "Sustain — gather at the patch's regrowth; it stays healthy.",
    "surplus": "Surplus — gather more now; the patch declines.",
    "market": "Market — gather for trade goods; faster decline.",
    "eradicate": "Eradicate — strip the patch bare.",
    "cultivate": "Cultivate — prepare this patch: low yield while you work it, then a much higher tended yield. It must stay staffed or it goes feral.",
    # Sow is plant RUNG 3 — the twin of Corral. Its hint must carry the two things that make it a
    # different bargain from Cultivate: it pays ~nothing while the crop is in the ground (there is no
    # standing stand to take a fraction of), and it out-yields a tended patch ~2×. The "goes feral"
    # warning is one rule for the whole plant web — an abandoned patch bleeds BOTH meters, so a
    # neglected Field reverts to WILD, not to a free tended patch.
    "sow": "Sow — plant a Field on this ground: almost no food while the crop grows, then twice a tended patch's yield. It must stay staffed or it goes feral all the way back to wild.",
}

# Taming pauses (it does not fail, and it does not lose progress) while the herd is not Thriving. The
# verb is deliberately NOT gated on that — a herd's phase swings as you hunt it — so this line is the
# only thing standing between the player and a hidden rule. %s = the herd's live `ecology_phase`.
const TAME_STALLED_HINT_FORMAT := "⚠ Taming is paused — the herd is %s, and it only gentles while Thriving. Progress is not lost: ease your hunters off and it resumes as the herd recovers."

# Every policy button's tooltip leads with this — the policy name + its full metric ("Sustain — up to
# +0.90/turn"), since the compact button face no longer carries the name. A gated button appends its
# gate reasons below (one per line), so a hover names the rung AND explains any lock.
const POLICY_TOOLTIP_NAME_FORMAT := "%s — %s"

# The pen as a managed POPULATION (docs/plan_corral_managed_population.md). A penned herd cannot
# graze: its keeper hauls it `pen_upkeep` food/turn off the band larder. `pen_fed_fraction` is the
# share of that demand the keeper actually paid last turn — anything below fully-fed means the herd
# is SHRINKING and its yield with it, so the Corral row swaps its penned badge for a loud starving
# state and the herd's map glyph tints red. `PenStatus` owns that test (shared with MapView); the two
# starving LABELS are `DetailFormat.PEN_STARVING_LABEL` / `PEN_FEED_STARVING_FORMAT`, beside the row
# builders that are their only readers.
# The pen's feed row in the herd drawer — the NET food-larder bill THIS pen draws per turn
# (`pen_larder_bill`, after pasture + hay), and whether it is being paid. The same bill the feed-split's
# "larder Y.Y" term states, so the two never disagree. The band's own ledger row is the sim-summed
# `pen_feed_upkeep` across all its pens; this is the per-herd figure, which is why the two are never added.
# Grazing 2d-γ — the pen is fenced LAND that grazes itself. Two herd-drawer rows state it:
#   • the FOOTPRINT — "Pen: radius R · N tiles" (`pen_radius` + the SERVER's in-bounds
#     `pen_footprint_tiles` count, displayed VERBATIM — the closed-form hex-disk count is wrong at map
#     edges, so the client never recomputes it).
#   • the FEED SPLIT — "Fed by pasture NN% · hay X.X · larder Y.Y food/turn". The three render-ready
#     terms the sim partitions the pen's GROSS demand into, ALL in food units, ZERO client arithmetic:
#     `pen_pasture_fraction` × 100 (grazed free), `pen_hay_food` (hay's food-equivalent draw), and
#     `pen_larder_bill` (the NET bread bill after pasture + hay). NOTE the larder term reads
#     `pen_larder_bill`, NOT `pen_upkeep` — `pen_upkeep` is the GROSS projection (`upkeep_per_biomass ×
#     biomass`, same basis as `corral_yield`, used only for the pre-commit Corral decision, pinned by
#     `core_sim` `snapshot/mod.rs` `pen_upkeep_*` tests); the honest bill the keeper actually hauls is
#     `pen_larder_bill`. Sim-pinned invariant: `pen_upkeep × pen_pasture_fraction + pen_hay_food +
#     pen_larder_bill == pen_upkeep`. The hay segment shows ONLY when `pen_hay_food >= SourceForecast.FOOD_FLOW_MIN` (a
#     pre-Foddering / no-hay pen renders the two-term form); a self-feeding pen reads "100% · larder
#     0.0", a scrub pen "0% · larder N.N". The Pen-feed row below still carries the debit + starving detail.
# The Extend-pen affordance (Grazing 2d-γ; command `extend_pen <faction> <x> <y>` at the pen anchor).
# On a built pen with no ring in flight it offers "Extend pen"; while a ring is being worked off
# (`pen_extend_progress > 0`) it is replaced by a "Fencing N%" badge — the pen twin of the corral-build
# "Building N%" meter. The server rejects an extend at max radius / unowned / Herding-unknown with a
# feed message, so the client does not pre-gate on those (max radius is not on the wire).
const PEN_EXTEND_LABEL := "Extend pen"

const PEN_EXTEND_TOOLTIP := "Fence another ring around the pen: the keeper works it off over ~25 turns at a reduced take, then the pen grazes more land and feeds itself further. Rejected at the pen-radius maximum."

const PEN_FENCING_LABEL := "Fencing %d%%"

# The policy hint under a LOCAL (resident-band) hunt's picker. The live yield line above it already
# carries the NUMBER; these carry the CONSEQUENCE, which is otherwise invisible — above all Sustain's,
# because a resident Sustain hunt on a thriving herd accrues HUSBANDRY toward livestock (and feeds
# Sedentarization's `domestication` input), the single most under-communicated payoff in the system.
#
# These are the BAND's payoffs and must not be reused for an expedition: the Hunting expedition arm
# credits FOOD ONLY — no husbandry accrual, no trade goods — so `SEND_HUNT_POLICY_HINTS` below
# deliberately promises neither. Do not merge the two sets; the asymmetry is real (a known v1 gap,
# tracked server-side), and a hint that claims a payoff the sim never pays is a lie to the player.
#
# Corral (the herd-side INVESTMENT rung) lives HERE and only here — it is a LOCAL-hunt policy: a
# detached party follows the herd and builds no pen, so the expedition set has no Corral entry (and
# the sim rejects a Corral expedition outright). This is also the local set `_policy_hint` spells out
# on a worked Hunt row's tooltip — those rows are always a resident band's.
const LOCAL_HUNT_POLICY_HINTS := {
    # Sustain USED to claim it tamed the herd ("on a thriving herd the hunt also tames it… livestock
    # that pays food every turn without being hunted down"). BOTH halves of that are now false and
    # the sentence is the reason this whole arc exists: slice 3a split the conflated branch, so
    # Sustain TEACHES the faction Herding but tames nothing (the `tame` verb fills the herd's own
    # meter), and slice 3b retired passive-free pastoral, so a tamed herd pays only through workers.
    # What Sustain honestly does is teach — which is exactly the ladder's first rung, so it says so.
    "sustain": "Sustain — takes only the herd's renewable yield, so it stays healthy forever. Working a herd this way is also how your people learn the next rung's craft: Herding on a wild herd, Penning on a tamed one.",
    "surplus": "Surplus — more food now; the herd slowly declines. The fuller larder pushes the band toward settling.",
    "market": "Market — sells the take as trade goods rather than eating it; the herd declines fast. Trade has little effect yet.",
    "eradicate": "Eradicate — hunts the herd toward extinction. No food, no craft learned, no trade — denial only.",
    # Tame is animal RUNG 2 — the verb that replaced the hidden Sustain side effect. Its payoff is
    # NOT "free food": 3b retired the passive rung, so the honest promise is yield PER WORKER (~1.5×
    # off the same crew) plus proximity (the herd drifts to the band instead of being chased).
    "tame": "Tame — gentle this herd into livestock: a reduced take while you work it, then it keeps to your band instead of roaming, and the same hunters bring back about half again as much. Your people still work it every turn.",
    # Corral is the ladder's best yield AND its only rung with a running cost. The hint has to carry
    # all three halves of that bargain — the ~25-turn investment dip, the top payoff, and the fact
    # that a penned herd is a POPULATION YOU FEED: its food comes off your larder every turn, and an
    # underfed herd shrinks (and takes its yield down with it). It also still escapes if unstaffed.
    "corral": "Corral — pen this herd: half yield for ~25 turns while you build, then the best yield of any herd. But penned animals can't graze: you feed them from your larder every turn, and an underfed herd shrinks. It must stay staffed or the herd goes wild again.",
}

# Overhunting flag: a worked source whose actual take exceeds its renewable-sustainable ceiling by
# more than this epsilon is overdrawing (depletable herds only — forage is renewable, actual ==
# sustainable, so it never trips). Shown as a WARN-tinted ⚠ on the row + spelled out in the tooltip.
const OVERHUNT_EPSILON := 0.001

const OVERHUNT_FLAG := "⚠"

# A MANAGED hunt source's crew are HERDERS, not a hunt party (`workersNeeded` = max(herders, haulers),
# scaling with herd size). The local stepper labels them so a pen needing several keepers doesn't read
# as a hunt-party bug. See `SourceForecast.is_managed_hunt_source`.
const HUNT_CREW_LABEL := "Hunters"

const HERD_CREW_LABEL := "Herders"

# A policy button carries its per-policy metric TWICE: a bare COMPACT string on the one-line button face
# (glyph + metric, no name — so all six rungs fit one docked row) and the VERBOSE full string in the
# tooltip (led by the policy name). Each `*_policy_takes` helper emits both as a `{compact, full}` pair.
# The INVESTMENT rungs (Cultivate/Sow, Tame/Corral) wear a metric too, but it is not an immediate take
# like the extractive rate — it is the PAYOFF the preparation builds TOWARD (the tended/field/pastoral/
# corral yield). A leading arrow marks it on the compact face (`→+1.20`, distinct from an extractive
# rate and never a rung you'd out-earn today); the full tooltip spells it "builds toward X/turn".
const POLICY_PAYOFF_COMPACT := "→%s"

const POLICY_PAYOFF_FULL_FORMAT := "builds toward %s/turn"

# The EXPEDITION picker wears the SAME "up to X/turn" cap metric as the local hunt + forage pickers
# (`POLICY_CAP_FORMAT` via `SourceForecast.extractive_take`): each policy's MAX obtainable food/turn, computed in
# `SourceForecast.expedition_policy_takes` as the max over party sizes of delivered_food / trip_turns. No bespoke
# raid-animals face any more — the three pickers read identically.
# The INVESTMENT rungs by name — "does this rung trade a dip now for a better source later?". This is
# the test for *which yield row a rung gets*, and it is deliberately NOT `policy in
# FORECAST_PAYOFF_KEYS`: `tame` is an investment rung that has no quotable payoff (above), so the
# payoff map cannot answer this question. An investment rung must never render the extractive
# "renewable / ⚠ overdraws the herd" preview — it is drawn sustainably by construction, and the
# verdict would argue with the dip row.
const INVESTMENT_POLICIES := ["cultivate", "sow", "tame", "corral"]

# The investment forecast states the DEAL, not a single yield: "Preparing: +0.09 /turn → then +1.20 /turn".
# Tame renders through it too (dip from `hunt_policy_ceilings["tame"]`, payoff = `pastoral_yield`), with
# no feed term (Tame has no running cost).
const INVESTMENT_FORECAST_FORMAT := "Preparing: %s → then %s"

# The same deal for a rung that also carries a running feed cost:
#   "Preparing: +0.75 /turn → then +5.40 /turn − 1.74 feed"
# `pen_upkeep` answers "what would this pen cost?" for an UNPENNED herd too (a projection at the
# herd's current biomass, on the SAME basis `corral_yield` uses — see `fauna::pen_upkeep`), so the
# pre-commit row quotes the real running cost at the moment the player actually decides. The
# subtraction is a pure difference of two numbers the sim exported for THIS herd; the client models
# no ecology. (Before the sim exported that projection this row said "before feed"; it no longer
# has to.) A herd with no `pen_upkeep` (no pen feed to charge) degrades to the plain no-feed format
# above rather than printing a fabricated "− 0.00 feed".
const INVESTMENT_FORECAST_FEED_FORMAT := "Preparing: %s → then %s − %s feed"

# A ZERO PAYOFF IS DATA, NOT A MISSING NUMBER — and it is the single most valuable thing this row can
# say. The pen's harvest is constant ESCAPEMENT (take only the biomass standing above `K/2`), so a
# herd at or below the MSY point honestly pays **0.00** until it rebuilds: penning it would eat feed
# every turn and pay nothing. That must never be suppressed, blanked, or em-dashed away — a player
# who pens a depleted herd because the UI declined to show them a zero has been actively misled. So
# the zero renders in full, and the row EMPHASIZES it: WARN-amber instead of income-green, plus this
# note naming the remedy (let it rebuild). The feed line still shows, because the feed is what makes
# a zero payoff a net LOSS rather than merely a nothing.
# (The "is it zero" floor is the shared `SourceForecast.FOOD_FLOW_MIN` — one definition of "below this, there is no
# flow here", used by the band ledger's rows and by this row alike.)
# AT ZERO WORKERS THERE IS NO "PREPARING" TO STATE. `Preparing` is staffing-scaled (workers ×
# per_worker) while the `→ then` payoff is not, so an unstaffed forecast used to read
# "Preparing: +0.00 /turn → then +1.22 /turn" — a sequence the player is emphatically NOT on track for,
# since an unstaffed build meter never advances at all. The payoff itself stays on screen (it is how you
# decide whether the source is worth staffing), but as a CONDITION rather than an imminent arrival.
# Short on purpose: the moment ONE worker is on it the full "Preparing: … → then …" line renders, so a
# long unstaffed sentence earns nothing. Crew-named, so a herd rung says hunters/herders.
const INVESTMENT_FORECAST_UNSTAFFED_FORMAT := "Assign %s — %s"

const INVESTMENT_FORECAST_UNSTAFFED_FEED_FORMAT := "Assign %s — %s − %s feed"

const INVESTMENT_FORECAST_DEPLETED_NOTE := "⚠ Too depleted to pen — it would eat feed and pay nothing until the herd rebuilds."

# How a forecast dict SPELLS its field keys — a key spelling, nothing more.
#
# Two dict shapes carry them BARE and so share one prefix: a herd dict, and the RAW wire
# forage-patch dict (decoded in native `forage_patches_to_array`, stored in `_band_labor.forage_patch_lookup()`,
# and read by the Current-actions Forage row). Only `tile_info` carries the patch's fields under a
# `patch_` prefix, because that is a cross-ref MapView stamps on in `_tile_info_at`.
#
# ⚠ A PREFIX CANNOT IDENTIFY A SOURCE KIND — that is why the bare case is ONE const and not two
# same-valued ones. It used to be two (a `HERD_*` and a `WIRE_FORAGE_PATCH_*`, both `""`), and
# having a herd-sounding name for the empty string invited `prefix == HERD_…` as an "is this a
# herd?" test; it read as discriminating and was not, so it silently routed forage patches down the
# herd branch and left the `+` button dead on every Current-actions Forage row. Pass `SOURCE_KIND_*`
# when you need the kind; a prefix only ever tells you how to spell a key.
const BARE_FORECAST_PREFIX := ""

const FORAGE_FORECAST_PREFIX := "patch_"

const SEND_EXPEDITION_HINT := "Detach a party to scout distant territory, then click a target tile."

const SEND_EXPEDITION_BUTTON := "Send scouting party…"

# Hunting expedition (PR 2, docs/plan_exploration_and_sites.md §2b): a detached party that follows a
# migratory herd, accumulates food, and drops it at the band. Launched from a resident band by
# picking a herd (herd-target click, not a tile), and Recalled like a scout expedition.
const SEND_HUNT_EXPEDITION_HINT := "Detach a party to follow a migratory herd, then click on the herd."

# Distance-aware herd-hunt affordance (docs/plan_exploration_and_sites.md §2b): clicking a herd
# offers a LOCAL hunt when it's within the SELECTED band's hunt_reach, or a hunting EXPEDITION when
# it's beyond. One compose control (worker/party stepper + policy), two labels/commands keyed off the
# wrap-aware hex distance from the selected band's own tile.
const ASSIGN_LOCAL_HUNT_BUTTON := "Assign Local Hunt"

# Range-aware forage assign: foraging is stationary gathering (NO expedition fallback), so a tile
# beyond the selected band's `work_range` disables the button rather than offering an alternative.
const FORAGE_ASSIGN_BUTTON := "Forage"

# `workers == 0` IS THE SIM'S UNASSIGN (server.rs: "Unassigning (workers == 0) is always allowed — a
# player must be able to abandon a source"), and the Work zone's unassign paths depend on it. So the
# submit is gated on whether it would CHANGE anything, never on the raw count: at 0 on a tile this band
# already works it is a legitimate unassign and says so, and at 0 on a tile it does not work it is a
# no-op and the button is dead. A client-side floor of 1 would fix the no-op and break the unassign.
const FORAGE_UNASSIGN_BUTTON := "Unassign"

const FORAGE_NOOP_HINT := "Nobody assigned yet — send at least one forager."

# ---- THE COMPOSE SHEET (docs/plan_tile_panel_layout.md §10-§15) -------------------------------
# Composing is modal by nature — open, decide, commit, done — so the two ~270px compose blocks live
# in a floating sheet (`ui/hud/ComposeSheet.gd`) rather than permanently in the drawer. The drawer
# keeps the detail rows, gains a one-line STANDING-ASSIGNMENT summary, and ends in the button below.
const FORAGE_CREW_LABEL := "Foragers"

# `Assign foragers ▸` / `Assign hunters ▸` / `Assign herders ▸` — the noun is the same one the
# sheet's stepper uses, so the drawer and the sheet can never disagree about who is being staffed.
const COMPOSE_OPEN_BUTTON_FORMAT := "Assign %s ▸"

const COMPOSE_SHEET_EYEBROW_FORMAT := "Assign %s"

# The standing staffing being edited, shown INSIDE the sheet (the header carries verb + subject).
const COMPOSE_NOW_STAFFED_FORMAT := "Now %d%s"

const COMPOSE_PENDING_SUFFIX := " · pending"

# The drawer's one-line summary of what is ALREADY standing on this source: `♻ 3 foragers · +2.74
# /turn`. The rate comes from `SourceForecast.source_yield_readout` — never recomputed here.
const STANDING_SUMMARY_FORMAT := "%s %d %s"

const STANDING_SUMMARY_SEPARATOR := " ·"

## The parties inspector strip's two inline links (mirrors the work inspector's Jump/Unassign).
const PARTY_INSPECT_JUMP := "Jump to party"

const PARTY_INSPECT_RECALL := "Recall"

## PARTIES zone.
const PARTIES_HEADER_FORMAT := "%d out · %d workers"

const PARTIES_EMPTY_HINT := "No parties in the field."

const PARTY_MENU_TOOLTIP := "Bulk actions for parties in the field."

const PARTY_RECALL_GLYPH := "✕"

const PARTY_RECALL_TOOLTIP := "Recall — the party walks home"

const PARTY_RECALL_WIDTH := 24.0

## The per-row recall stays VISIBLE (parties have no other removal path) but rests dimmed, so it
## reads as available without competing with the row it sits on.
const PARTY_RECALL_REST_ALPHA := 0.45

const PARTY_RECALL_ALL_FORMAT := "Recall all parties (%d)"

const PARTY_RECALL_CONFIRM_FORMAT := "Recall all %d parties? They walk home carrying what they have."

const PARTY_RECALL_CONFIRM_OK := "Recall all"

## Single-party recall confirm — wraps each BUTTON handler (row ✕, inspector Recall, drawer Recall), NOT
## the shared emit `_on_recall_expedition_pressed` (which "Recall all" already loops under its OWN one
## confirm — confirming inside the emit would pop N prompts after a confirmed "Recall all").
const PARTY_RECALL_ONE_CONFIRM_FORMAT := "Recall the %s party? It walks home carrying what it has."

const PARTY_RECALL_ONE_CONFIRM_OK := "Recall"

## The %s a scout party fills into the recall prompt — a bare word, since "Recall the Scouting
## expedition party?" (the full mission label) reads doubled; a hunt party fills its herd name.
const PARTY_RECALL_SCOUT_LABEL := "scouting"

## The parties inspector strip is DENSER than the work inspector (up to 6 detail lines vs ~1), and the
## T/B parties zone is height-capped at ~300px, so its detail lines are tightened a touch below
## HudWorkVocab.ZONE_BLOCK_SEPARATION to keep the strip + a party row + the bottom-pinned footer inside the box.
const PARTIES_INSPECTOR_LINE_SEPARATION := 4

const SEND_PARTY_NO_IDLE_REASON := "No idle workers to spare. Free some from Work."

## The compose sheet — MISSION FIRST: the footer launches straight into a mission, so the sheet is
## always already on one and the policy picker is unreachable except under Hunt.
const COMPOSE_MISSION_SCOUT := "scout"

const COMPOSE_MISSION_HUNT := "hunt"

const COMPOSE_MISSION_LABEL_SCOUT := "⚑ Scout"

const COMPOSE_MISSION_LABEL_HUNT := "🏹 Hunt"

const COMPOSE_TITLE_SCOUT := "Setup a scouting party…"

const COMPOSE_TITLE_HUNT := "Setup a hunting party…"

const COMPOSE_FIELD_PARTY := "Party"

const COMPOSE_FIELD_POLICY := "Policy"

## The QUARRY is the hunt form's FIRST question: the herd sets the useful party size, the per-policy
## take and the trip length, so every field below it is unanswerable until it is picked.
const COMPOSE_FIELD_QUARRY := "Quarry"

const COMPOSE_QUARRY_CHOOSE := "Choose…"

const COMPOSE_QUARRY_HINT := "Choose a quarry — the rest of the form follows from it."

const COMPOSE_QUARRY_TOOLTIP_FORMAT := "%s (%d, %d)\nClick to choose a different herd."

const COMPOSE_QUARRY_LABEL_FORMAT := "%s %s"

## The refusal when the player picks a herd the band can already work from home. The hunt_reach split
## is a rule the map does not spell out, so the refusal is where it gets taught — it names the herd,
## the distance, the reach that binds and the local alternative.
const QUARRY_WITHIN_REACH_FORMAT := "%s is %d tiles away — inside %s's hunt reach (%d). Hunt it from the herd itself instead of sending a party."

const COMPOSE_OF_IDLE_FORMAT := "of %d idle"

const COMPOSE_CANCEL_TOOLTIP := "Cancel"

const CANCEL_SCOPE_ALL := "all"

const CANCEL_SCOPE_WORK := "work"

const CANCEL_SCOPE_ROLES := "roles"

# The launch policy (Sustain/Surplus/Market/Eradicate) chosen for a hunting EXPEDITION, with a
# one-line behaviour hint so the choice is legible. Reuses `SourceForecast.LABOR_HUNT_POLICIES` for the option set.
#
# An expedition's Hunting arm credits **FOOD ONLY** — no husbandry accrual, no trade goods (a known v1
# gap, tracked server-side). So these hints promise NEITHER, even though the resident-band versions of
# Sustain and Market do exactly those things (see `LOCAL_HUNT_POLICY_HINTS`). The asymmetry is real;
# blurring it would have the UI promise the player a payoff the sim never pays.
const SEND_HUNT_POLICY_HINTS := {
	# Sustain is the MAXIMUM SUSTAINABLE YIELD flow — the same per-turn skim a resident band's Sustain
	# hunt takes, so the herd stays healthy indefinitely. The trade-off is speed: MSY is a small flow,
	# so a party fills slowly, and on a small herd the trip may not be worth sending. The per-herd
	# turns-to-fill forecast (shown at the herd-targeting step) is the number that decides it. It does
	# NOT tame the herd — only a resident band's Sustain hunt builds husbandry.
	"sustain": "Sustain — takes only the herd's sustainable yield; it stays healthy forever, but the party fills slowly on a small herd.",
	"surplus": "Surplus — takes the herd's spare stock, so the party fills fast; the herd declines.",
	"market": "Market — grinds the herd down with repeated trips; the party still hauls home food, not trade goods.",
	"eradicate": "Eradicate — denial mission: hunts the herd toward extinction and delivers no food.",
}

# A resident BAND and a detached EXPEDITION are told apart by the sim, and the client reads a DIFFERENT
# herd field for each — never one for the other:
#   `hunt_policy_ceilings`  {policy → provisions/turn} — the BAND's renewable FLOW ceiling. With the
#       cohort's levers this makes the LOCAL hunt preview pure arithmetic (see `_hunt_take_rate`).
#   `hunt_trip_estimates`   {"<policy>:<workers>" → {turns_to_fill, delivers_food}} — the sim's
#       PRE-LAUNCH TRIP ESTIMATE, forward-simulated server-side. An expedition's trip length is NOT a
#       rate division: on Surplus/Market the ceiling is a *stock*, so the party strips the headroom in
#       a turn or two and then crawls at the herd's regrowth trickle. A re-derived `carryCap / rate`
#       closed form is wrong, and wrong by a lot — on a FULL Rabbit Warren under Surplus only a LONE
#       hunter fills at all (23 turns); a party of 4 never fills within the sim's horizon. So the
#       client does ZERO arithmetic here — it looks the answer up. Never re-derive it.
# (The denial case — an Eradicate party hunts the herd toward extinction and carries NOTHING home —
# is NOT inferred from the policy string: the estimate itself carries `delivers_food = false`, so the
# sim, not the client, decides which policies are denial missions.)
# Pre-launch hunt-trip forecast (shown in the targeting banner while a hunt expedition is armed and
# the player hovers a herd, and live above the herd panel's Send button). It is a PURE TABLE LOOKUP
# into the sim-exported per-(policy, party-size) `hunt_trip_estimates` carried on the herd — each cell
# {policy, party_workers, turns_to_fill, delivers_food}, where `turns_to_fill == 0` means the party
# does NOT fill within the sim's `forecast_horizon_turns`. The client reads the cell and stops (see
# `SourceForecast.hunt_trip_forecast`); the only thing it computes is the display verdict:
#     viable = turns <= expedition_viability_warn_turns   (the band's own exported lever)
# THE CLIENT DOES ZERO ARITHMETIC FOR AN EXPEDITION, and must NEVER divide a carry cap by a take rate.
# The sim FORWARD-SIMULATES the trip — the herd's state moves under the party, its stock exhausts, and
# a horizon bounds the answer — so any client-side re-derivation drifts from the take the sim actually
# performs. That forward simulation is the only honest number (pinned by core_sim/tests/expedition_hunt.rs).
# This does NOT mean the client does no math anywhere: the LOCAL (resident band) per-turn yield preview
# IS legitimate arithmetic — `min(workers × hunt_per_worker_provisions, band_ceiling) × output_multiplier`
# over `hunt_policy_ceilings`, the BAND flow ceiling (`_hunt_take_rate` / `_local_hunt_preview_bbcode`,
# pinned by exported_snapshot_fields_reproduce_band_hunt_take). Band = flow arithmetic; expedition = lookup.
# Live per-turn yield preview for the LOCAL hunt branch. A resident hunt has no carry cap, so
# turns-to-fill is meaningless there; the number that decides a standing assignment is the food/turn
# it will produce — the sim's hunt take:
#     rate = min(workers × hunt_per_worker_provisions, ceiling_for(policy)) × output_multiplier
# The band applies its morale/discontent productivity modifier (`output_multiplier`) at payout; a
# detached expedition does not, which is why the two branches show different numbers from the same
# exported fields. (pinned sim-side by core_sim/tests/expedition_hunt.rs.)
const LOCAL_HUNT_YIELD_FORMAT := "≈ %s"

# The Sustain ceiling IS the herd's sustainable yield, so a take above it draws the herd down — flagged
# with the same ⚠ / WARN amber. This is the COMPOSE preview, which derives the flag from the steady
# forecast via `_is_overdraw` (there is no assignment yet, so no wire `overdraws` field); the CONFIRMED
# allocation rows instead read the sim-answered `overdraws` bool off the assignment.
const LOCAL_HUNT_OVERDRAW_SUFFIX := " — overdraws the herd"

# The FORAGE twin of the hunt overdraw suffix: a take above the patch's Sustain ceiling draws its
# biomass down. Forage is smooth food (no whole-animal rhythm), so the preview shows a bare rate + this.
const LOCAL_FORAGE_OVERDRAW_SUFFIX := " — overdraws the patch"

# CARRY-AWARE ANIMALS-FIRST preview. A hunt delivers WHOLE animals via a kill-credit bank, so an
# unquantized food/turn rate credits fractional-animal throughput the crew can never carry home (the sim
# itself quantizes to whole bodies). The line instead leads with the honest carry-aware delivered rate in
# ANIMALS: `≈<rate> <animal>/turn`, rate = delivered ÷ food_per_animal (`_hunt_delivered_and_waste`).
const HUNT_DELIVERED_FORMAT := "≈%s %s/turn"

# The delivered animals-per-turn rate is a long-run average of lumpy whole-animal delivery — you take
# WHOLE animals, so per-turn delivery varies. A STABLE, worker-independent disclaimer (always shown on an
# extractive hunt rung) naming the averaging span, computed from the SELECTED policy's ceiling by
# `_hunt_avg_window_turns` so it never blinks out as workers change (a faster policy averages over a
# different span, so the line reflects the composed action, not a Sustain-wide claim).
const HUNT_AVG_WINDOW_FORMAT := "This estimate is a long-run average over ~%d turns — you take whole animals, so per-turn delivery varies."

# The averaging window's upper clamp: near-integer animals/turn rates make the "extra animal" cycle span
# read absurdly long, so cap it at a plausible span.
const HUNT_WINDOW_MAX_TURNS := 12

# Animals-per-turn rate formatting: up to 2 decimals, trailing zeros/dot stripped (1.90→"1.9", 1.00→"1",
# 0.65→"0.65"). `String.num` already trims (unlike the padded food-rate formatter).
const HUNT_ANIMAL_RATE_DECIMALS := 2
