class_name HudConst

## Universal HUD constants — the core leaf every cluster reads. Reads nothing (no cross-module
## const initializer), so it can never take part in a load-time const cycle.

const PLAYER_FACTION_ID := 0

# THE ONE HANDLE A COMMAND MAY NAME A BAND BY: `PopulationCohortState.bandId`, decoded onto every
# cohort dict as `band_id`. It is durable — a rollback rebuilds the ECS world and renumbers every
# `entity`, so a command addressed by entity bits resolved to nothing when replayed, silently. The
# sim's `resolve_starting_unit_entity` accepts `band_id` and nothing else, and both are `u64`, so
# sending the wrong one fails NOWHERE until a player notices their orders do nothing.
# `entity` remains correct for client-local identity (selection, marker keys, the pending-labor
# overlay) — it simply never travels to the server.
# The wire's "no id" value, which does not occur for a real band; used as the absent-band sentinel
# so an unresolvable band emits no command at all rather than one naming band 0.
const NO_BAND_ID := 0

# `roster_occupant_selected`'s id for the LAND kind: the land has no entity, and the signal's id is
# a Variant, so it carries the same "no occupant" sentinel the rest of the client uses.
const LAND_SUBJECT_ID := -1

# Provisions is the food item under a band's larder `stores`.
const STORE_ITEM_PROVISIONS := "provisions"

# **A TIE AT THIS STRENGTH IS PARKED, AND AT ZERO NOTHING FLOWS** (arc #527) — the client's reading of
# the sim's own `connections::NO_TIE` gate. A parked edge is a published row meaning "we know such a
# people exist and have no current dealings", NOT an absent one, so a reader that dropped it would
# hide the very fact that the tie is what gates a shipment. It is a floor to compare ABOVE, never a
# tolerance: the sim gates on `strength > NO_TIE` and the client must ask the same question.
const TIE_STRENGTH_NONE := 0.0

# **`STORE_ITEM_TRADE_GOODS` IS RETIRED** (arc #527) along with the `TRADE_GOODS` commodity itself:
# the sim writes no such key onto a band's `stores` map any more. What a harvest pays beyond food and
# fodder is MATERIALS, which are batches on the cohort (`material_batches`) rather than a scalar in
# the larder, and which the Crafting panel reads.

# Early-Game Labor (docs/plan_early_game_labor.md, slice 3b). Assignment kinds mirror
# the sim's LaborAssignment.kind; the source-centric allocation targets the single
# player band captured from each snapshot (there is exactly one player band today).
const LABOR_KIND_SCOUT := "scout"

const LABOR_KIND_WARRIOR := "warrior"

# INVESTMENT rungs (the Intensification Ladder, docs/plan_intensification_ladder.md §2): an up-front
# cost — the source pays only its dip ceiling (the patch's `ceiling_cultivate` / `ceiling_sow`
# scalars; for a herd, the `tame` / `corral` rows of its `hunt_policy_ceilings` list) while the
# workers prepare it, then flips to the much higher managed yield. Kind-specific, and the sim REJECTS the cross pairing: Cultivate + Sow are forage-only, Tame
# + Corral are hunt-only. Each ladder now runs its verb TWICE — one verb per rung-transition:
#   plants:  wild --cultivate--> Tended Patch --sow--> Field
#   animals: wild --tame------> Pastoral herd --corral--> Pen
const LABOR_POLICY_CULTIVATE := "cultivate"

const LABOR_POLICY_SOW := "sow"

const LABOR_POLICY_TAME := "tame"

# 0..1 progress tracks (knowledge, domestication) render as whole percents.
const PROGRESS_PERCENT_SCALE := 100.0

# A knowledge track (0..1) is usable only once fully learned; a domestication track likewise.
const KNOWLEDGE_COMPLETE := 1.0

# One worker per −/+ stepper press.
const WORKER_STEP := 1

# Tile-card SIGHT row — the player must ALWAYS be able to tell "there is nothing here" apart from
# "I cannot see what is here". Herds/bands are LIVE state and are fog-gated out of `tile_info`
# (MapView._herds_on_tile), so on a remembered hex an empty Occupants list would otherwise read as
# "empty" when the truth is "unknown". These three rows name the hex's sight state in plain words,
# and `OCCUPANTS_UNKNOWN_*` replaces the roster with the honest statement.
# Sim-side the states are Active / Discovered / Unexplored.
# The FoW states MapView tags onto `tile_info.visibility_state` (mirrors `_visibility_state_at`).
# Empty string = FoW disabled → everything is in sight, and the Sight row is omitted entirely.
const VISIBILITY_ACTIVE := "active"

const VISIBILITY_DISCOVERED := "discovered"

const VISIBILITY_UNEXPLORED := "unexplored"

const TILE_SIGHT_KEY := "Sight"

const TILE_SIGHT_ACTIVE := "In sight"

const TILE_SIGHT_REMEMBERED := "Remembered — not in sight now"

# The chip FACE for the remembered state. A pill states a condition in a word; the full sentence
# above is a sentence — it was the widest element in the strip — so it rides the chip's tooltip.
const TILE_SIGHT_REMEMBERED_SHORT := "Remembered"

const TILE_SIGHT_UNEXPLORED := "Unexplored"

# Shown INSTEAD of the Occupants roster on a hex the player cannot currently see. Never render an
# empty roster there — an absent list is a claim of emptiness the client cannot back up.
const OCCUPANTS_UNKNOWN_TITLE := "out of sight"

# **IT PROMISES NOTHING, AND THAT IS DELIBERATE.** It used to close with "Scout it to see", which is
# a promise scouting cannot keep: scouting makes a hex DISCOVERED, and this sentence is what a
# Discovered hex says — current contents need SIGHT, i.e. a band standing there now. Reported from
# play by a player who scouted the hex and found it already back to `Remembered` by the time they
# reached camp. The unexplored twin below still names the verb, because there the verb does work.
const OCCUPANTS_UNKNOWN_REMEMBERED := "You remember the ground here, but not what's on it now."

const OCCUPANTS_UNKNOWN_UNEXPLORED := "Nobody has been here. Send a band to reveal what's on this ground."

# Your OWN party can stand on a hex it cannot see (a scouting expedition doesn't reveal fog — discovery
# is comm-range gated), so the roster CAN be non-empty on an unseen hex while still hiding everything
# that isn't ours. Listing only your own party without a word would quietly imply it's alone there.
const OCCUPANTS_UNSEEN_OTHERS_HINT := "Out of sight — you can't see anything here but your own."
