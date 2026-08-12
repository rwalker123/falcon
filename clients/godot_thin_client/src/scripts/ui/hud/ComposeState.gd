class_name ComposeState
extends RefCounted

## "What the player is dialing but has not committed" — the in-progress compose state shared by the
## selection card's two compose builders and the Band panel's party sheet (HUD decomposition Phase
## 2c-1, `docs/plan_hud_decomposition.md`). Pure DATA: it never holds a scene node or a `%Name`
## lookup, which is exactly why the `ComposeSheet` node handle itself stays on `HudLayer` — a model
## owns the composition, not the surface it is drawn on.
##
## THREE GROUPS, deliberately kept apart:
##   • `forage_*` — the tile card's "assign foragers" block. Read/written inside ONE builder.
##   • `hunt_*`   — the herd drawer's "assign hunters/herders" block. Two reads leak out of its
##                  builder (`_herd_crew_noun`, the improvement control's deal) — see `hunt_floor()`.
##   • `party_*`  — the Band panel's PARTIES-zone compose sheet (quarry + autofill). It is NOT drawer
##                  state, so it sits in its own section and can travel to a band-panel extraction
##                  without unpicking the drawer's.
## `kind()` / `subject()` are the compose sheet's SUBJECT identity, not a group of their own: they say
## which source the open sheet is composing so a per-snapshot refresh can tell "the same source,
## restated" from "a different source, gone".
##
## There is deliberately NO `changed` signal (unlike `HudSelectionState`): nothing subscribes — the
## compose builders re-render themselves explicitly on every edit — and an unused API is a liability.

# Which source kind the open compose sheet is composing. Aliased back onto `HudLayer` as
# `COMPOSE_KIND_*` so every existing reference works.
const KIND_NONE := ""
const KIND_FORAGE := "forage"
const KIND_HERD := "herd"

## "No band" for every band-ENTITY slot here — the picked actor and the band a composition was last
## seeded for. Distinct from `HudConst.NO_BAND_ID`, which is the WIRE's band id; these hold the
## cohort's `entity`, and a resolver that finds no band answers a dict with no `entity` key at all.
const NO_BAND_ENTITY := -1

# ---- Forage compose (the tile card's "assign foragers" block) ------------------------------------
# The source this compose belongs to ("x,y"), so a per-snapshot re-render preserves the dialed count
# but a NEW tile re-seeds it from that tile's standing staffing.
var _forage_key: String = ""
var _forage_count: int = 0
# One-shot: set when the player CLICKS a forage FLOOR PRESET so the next rebuild auto-fills the worker
# count to that floor's max-useful cap ("give me everything this patch sustains"). Cleared as soon as it is
# consumed, never set by a −/+ tick, so manual counts survive the rebuild.
var _forage_autofill := false
var _forage_floor: float = SourceForecast.DEFAULT_HARVEST_FLOOR
# **THE SECOND AXIS** (issue #442) — the improvement this compose will COMMIT TO on the patch, or
# `IMPROVEMENT_NONE` for "build nothing". Seeded from the standing assignment's own `improvement` when
# the source changes, then toggled by the improvement checkbox; committing sends its own verb beside
# `assign_labor`, which never touches this axis.
var _forage_improvement: String = ""
# The crop the composed Cultivate/Sow will commit this patch to (flora roster S1); "" = send nothing,
# which is VALID and yields the sim's own default (`default_species_for_rung` — the highest-share
# legal plant). Re-resolved every render, so it can never name a plant this tile/rung cannot take.
var _forage_species: String = ""
# The band-picker selection (actor band entity); `NO_BAND_ENTITY` means "fall back to the resolved band".
var _forage_band: int = NO_BAND_ENTITY
# WHICH BAND THE COMPOSITION ABOVE WAS SEEDED FOR. A crew, a floor and a build are all facts about ONE
# band's standing assignment, so switching the actor invalidates them exactly as switching the source
# does — see `seed_forage`.
var _forage_seeded_band: int = NO_BAND_ENTITY

# ---- Hunt compose (the herd drawer's "assign hunters/herders" block) -----------------------------
var _hunt_key: String = ""
var _hunt_count: int = 0
var _hunt_floor: float = SourceForecast.DEFAULT_HARVEST_FLOOR
# The hunt twin of `_forage_improvement` — same contract, the animal ladder's verbs.
var _hunt_improvement: String = ""
# The hunt twin of `_forage_autofill` — same one-shot contract.
var _hunt_autofill := false
var _hunt_band: int = NO_BAND_ENTITY
# The hunt twin of `_forage_seeded_band` — same contract.
var _hunt_seeded_band: int = NO_BAND_ENTITY

# ---- Party compose (the Band panel's PARTIES zone — NOT drawer state) ----------------------------
# The quarry the party compose sheet is aimed at (a world herd id), "" until one is picked. It is the
# FIRST question the hunt form asks, because the herd sets the useful party size, the per-floor take
# and the trip length — everything below it in the form.
var _party_quarry_id: String = ""
# The sheet's OWN autofill one-shot. Deliberately NOT `_hunt_autofill`: sharing it would let one
# surface's floor click refill the other surface's stepper.
var _party_autofill := false
# ---- THE KIT each group is composing (`docs/plan_denial_raid.md`) ---------------------------------
# The roster id the crew will be sent out with, per compose group — `KitRoster.NO_KIT_ID` until the
# first render resolves one.
#
# **THREE FIELDS, NOT ONE, AND THE SPLIT FOLLOWS THE GROUPS ABOVE.** A kit is dialed on a SHEET, and
# the three sheets are independently open: composing `none` in the herd drawer must not silently
# re-arm the dock's party form with it. The hunt and denial missions share `_party_kit_id` because
# they are one sheet under two missions and both send on the `hunt` job — `KitRoster.resolve_selection`
# re-validates against the mission's own job on every render, so a selection can never survive into a
# verb that would refuse it.
var _forage_kit_id: String = KitRoster.NO_KIT_ID
var _hunt_kit_id: String = KitRoster.NO_KIT_ID
var _party_kit_id: String = KitRoster.NO_KIT_ID

# ---- The open sheet's subject identity -----------------------------------------------------------
var _kind: String = KIND_NONE
var _subject: String = ""

## Both floors start on the caller's default (`SourceForecast.DEFAULT_HARVEST_FLOOR` — the food peak,
## which is also what the sim assumes for a command carrying no floor token), passed in rather than
## mirrored here so the number lives in exactly one place.
func _init(default_floor: float) -> void:
	_forage_floor = SourceForecast.clamp_floor(default_floor)
	_hunt_floor = SourceForecast.clamp_floor(default_floor)

# ---- Forage read accessors -----------------------------------------------------------------------

func forage_key() -> String:
	return _forage_key

func forage_count() -> int:
	return _forage_count

func forage_floor() -> float:
	return _forage_floor

func forage_species() -> String:
	return _forage_species

## The improvement this compose will commit to on the patch — `IMPROVEMENT_NONE` for none.
func forage_improvement() -> String:
	return _forage_improvement

func forage_band() -> int:
	return _forage_band

## The band the composed count/floor/crop/improvement were last seeded from — `NO_BAND_ENTITY` until a
## first seed. The compose builder re-seeds when this disagrees with the band it has resolved, which is
## how switching the ACTOR re-reads the standing assignment the same way switching the SOURCE does.
func forage_seeded_band() -> int:
	return _forage_seeded_band

# ---- Forage mutators -----------------------------------------------------------------------------

## A DIFFERENT tile is being composed: adopt its key and default the actor band. Re-seeding the
## count/floor/crop is a SECOND step (`seed_forage`) because the caller has to resolve the actual
## band dict — possibly falling back — before it can read that band's standing staffing.
func begin_forage_source(key: String, band_entity: int) -> void:
	_forage_key = key
	_forage_band = band_entity

## Re-seed the composed count + floor from the newly-resolved band's staffing on the tile. The
## crop selection is cleared with them: a crop pick belongs to the PATCH it was made on, and a new
## tile has a different basket.
## `improvement` re-seeds from the standing assignment's OWN second-axis field, so a patch already
## being cultivated opens with its box checked rather than looking untouched.
##
## **IT RECORDS THE BAND IT SEEDED FROM**, because a composition is a statement about ONE band's
## standing assignment: the crew, the floor and the build all come from that band's row, so the actor
## changing invalidates them exactly as the source changing does. Without the record the sheet went on
## showing the previous band's crew — most damagingly a 0, which turns the commit into an Unassign
## against the crew the newly-picked band really has on the tile.
func seed_forage(count: int, floor: float, improvement: String) -> void:
	_forage_count = count
	_forage_floor = SourceForecast.clamp_floor(floor)
	_forage_improvement = improvement
	_forage_species = ""
	_forage_seeded_band = _forage_band

## Forget which tile the forage compose belongs to, so the NEXT render takes the source-changed path
## and re-seeds count/floor/crop from the band's standing staffing. (`""` matches no real "x,y" key.)
## The preview harnesses' way of staging a fresh compose without faking a selection change.
func reset_forage_source() -> void:
	_forage_key = ""

## The picked actor band, and NOTHING ELSE. It deliberately does not re-seed: the caller has to resolve
## the band dict before it can read that band's standing staffing, which is the same two-step
## `begin_forage_source` + `seed_forage` exists for. `forage_seeded_band` is what lets the builder
## notice the write on the next render.
func set_forage_band(entity: int) -> void:
	_forage_band = entity

func set_forage_floor(floor: float) -> void:
	_forage_floor = SourceForecast.clamp_floor(floor)

func set_forage_count(count: int) -> void:
	_forage_count = count

func set_forage_species(species: String) -> void:
	_forage_species = species

func set_forage_improvement(improvement: String) -> void:
	_forage_improvement = improvement

## Arm the auto-fill one-shot (a floor CLICK, never a stepper tick).
func arm_forage_autofill() -> void:
	_forage_autofill = true

## Consume the auto-fill one-shot: true exactly once per arming, disarming it as it answers.
func consume_forage_autofill() -> bool:
	var armed := _forage_autofill
	_forage_autofill = false
	return armed

## Clamp the dialed count into the current cap. A read-modify-write, so it lives here rather than
## being spelled out at the call site where the field could be read and written apart.
func clamp_forage_count(cap: int) -> void:
	_forage_count = clampi(_forage_count, 0, cap)

## Re-resolve the composed crop through `resolver`, which takes the CURRENT selection and answers the
## still-legal one (a rung switch changes which plants it can take). The read-modify-write is
## the model's; the crop RULES stay with the caller, so this holds no flora knowledge.
func resolve_forage_species(resolver: Callable) -> void:
	_forage_species = String(resolver.call(_forage_species))

# ---- Hunt read accessors -------------------------------------------------------------------------

func hunt_key() -> String:
	return _hunt_key

func hunt_count() -> int:
	return _hunt_count

## PUBLIC beyond the herd drawer: `_herd_crew_noun` and the improvement control's deal (which prices
## its dip against the held STANCE — issue #442) read it from outside the
## builder. (An earlier note here also claimed the picker fell back to THIS value when
## given no explicit selection, letting a work-inspector or party-compose render inherit the drawer's
## rung. That branch was DEAD CODE and is gone — `selected` is a required param and the picker reads
## no compose state at all. The claim was never true in practice; it was asserted from the branch's
## presence without checking any caller could reach it.)
func hunt_floor() -> float:
	return _hunt_floor

func hunt_band() -> int:
	return _hunt_band

## The hunt twin of `forage_seeded_band` — same contract.
func hunt_seeded_band() -> int:
	return _hunt_seeded_band

## The improvement this compose will commit to on the herd — `IMPROVEMENT_NONE` for none.
func hunt_improvement() -> String:
	return _hunt_improvement

# ---- Hunt mutators -------------------------------------------------------------------------------

## A DIFFERENT herd is being composed — the hunt twin of `begin_forage_source` (same two-step reason).
func begin_hunt_source(key: String, band_entity: int) -> void:
	_hunt_key = key
	_hunt_band = band_entity

## **A DIFFERENT ANIMAL HAS A DIFFERENT DEFAULT KIT, so the composed one is dropped with the count and
## the floor.** Called from the same `source_changed` branch `seed_hunt` is, and it is what makes the
## per-herd default reachable at all: every render writes the RESOLVED id back here, so without this a
## selection resolved once on a Red Deer would be the "player's own choice" on every warren after it,
## and the herd's own default would be taken exactly once per session.
##
## **It resets rather than seeds, because the default is not this model's to know** — the roster and
## the herd both live outside it, and `KitRoster.resolve_selection` answers from them on the next
## render. `NO_KIT_ID` is the same "nothing picked yet" this field is born holding.
func reset_hunt_kit() -> void:
	_hunt_kit_id = KitRoster.NO_KIT_ID

## Re-seed the composed count + floor + improvement from the newly-resolved band's staffing on the
## herd — the hunt twin of `seed_forage`, including the seeded-band record.
func seed_hunt(count: int, floor: float, improvement: String) -> void:
	_hunt_count = count
	_hunt_floor = SourceForecast.clamp_floor(floor)
	_hunt_improvement = improvement
	_hunt_seeded_band = _hunt_band

## The hunt twin of `reset_forage_source` — forget the herd so the next render re-seeds.
func reset_hunt_source() -> void:
	_hunt_key = ""

## The hunt twin of `set_forage_band` — a bare write, for the same reason.
func set_hunt_band(entity: int) -> void:
	_hunt_band = entity

func set_hunt_floor(floor: float) -> void:
	_hunt_floor = SourceForecast.clamp_floor(floor)

func set_hunt_improvement(improvement: String) -> void:
	_hunt_improvement = improvement

func set_hunt_count(count: int) -> void:
	_hunt_count = count

func arm_hunt_autofill() -> void:
	_hunt_autofill = true

func consume_hunt_autofill() -> bool:
	var armed := _hunt_autofill
	_hunt_autofill = false
	return armed

## The hunt twin of `clamp_forage_count` — the same read-modify-write, kept off the call site.
func clamp_hunt_count(cap: int) -> void:
	_hunt_count = clampi(_hunt_count, 0, cap)

# ---- Party compose (parties zone) ----------------------------------------------------------------

func party_quarry_id() -> String:
	return _party_quarry_id

## The quarry is now `herd_id` — a fresh pick, a re-pick on the map, or a switch between the herds
## sharing one hex.
##
## **THE COMPOSED KIT GOES WITH IT**, the dock's twin of `reset_hunt_kit`: the default is a fact about
## the quarry now, so a kit resolved against the last animal is not the player's choice about this one.
## Reached only from the targeting pick (never per render), so a live selection is never wiped
## mid-compose.
func set_party_quarry(herd_id: String) -> void:
	_party_quarry_id = herd_id
	_party_kit_id = KitRoster.NO_KIT_ID

## No quarry: a fresh compose act, a cancelled one, a quarry that left the snapshot or migrated into
## the band's hunt reach, or a panel-band swap (a quarry is chosen FOR a band).
func clear_party_quarry() -> void:
	_party_quarry_id = ""
	_party_kit_id = KitRoster.NO_KIT_ID

func arm_party_autofill() -> void:
	_party_autofill = true

func consume_party_autofill() -> bool:
	var armed := _party_autofill
	_party_autofill = false
	return armed

# ---- The composed KIT, one accessor pair per group ------------------------------------------------
# Read RAW: the value the player last picked, which may name a kit the current roster/job no longer
# offers. Every render passes it through `KitRoster.resolve_selection` before using it, so the
# fall-back to the job default lives in ONE place rather than in each accessor.

func forage_kit_id() -> String:
	return _forage_kit_id

func set_forage_kit_id(kit_id: String) -> void:
	_forage_kit_id = kit_id

func hunt_kit_id() -> String:
	return _hunt_kit_id

func set_hunt_kit_id(kit_id: String) -> void:
	_hunt_kit_id = kit_id

func party_kit_id() -> String:
	return _party_kit_id

func set_party_kit_id(kit_id: String) -> void:
	_party_kit_id = kit_id

# ---- The open sheet's subject ---------------------------------------------------------------------

func kind() -> String:
	return _kind

func subject() -> String:
	return _subject

func set_composing(compose_kind: String, compose_subject: String) -> void:
	_kind = compose_kind
	_subject = compose_subject

## The sheet closed — `ComposeSheet.closed` is what drives this, so the node and the model can never
## disagree about whether a sheet is open.
func clear_composing() -> void:
	_kind = KIND_NONE
	_subject = ""
