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
##   • `hunt_*`   — the herd drawer's "assign hunters/herders" block. Three reads leak out of its
##                  builder (`_build_policy_picker`'s no-selection fallback, `_tame_stalled_hint`,
##                  `_herd_crew_noun`) — see `hunt_policy()`.
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

# ---- Forage compose (the tile card's "assign foragers" block) ------------------------------------
# The source this compose belongs to ("x,y"), so a per-snapshot re-render preserves the dialed count
# but a NEW tile re-seeds it from that tile's standing staffing.
var _forage_key: String = ""
var _forage_count: int = 0
# One-shot: set when the player CLICKS a forage policy so the next rebuild auto-fills the worker count
# to the policy's max-useful cap ("give me everything this patch sustains"). Cleared as soon as it is
# consumed, never set by a −/+ tick, so manual counts survive the rebuild.
var _forage_autofill := false
var _forage_policy: String = ""
# The crop the composed Cultivate/Sow will commit this patch to (flora roster S1); "" = send nothing,
# which is VALID and yields the sim's own default (`default_species_for_rung` — the highest-share
# legal plant). Re-resolved every render, so it can never name a plant this tile/rung cannot take.
var _forage_species: String = ""
# The band-picker selection (actor band entity); -1 means "fall back to the resolved band".
var _forage_band: int = -1

# ---- Hunt compose (the herd drawer's "assign hunters/herders" block) -----------------------------
var _hunt_key: String = ""
var _hunt_count: int = 0
var _hunt_policy: String = ""
# The hunt twin of `_forage_autofill` — same one-shot contract.
var _hunt_autofill := false
var _hunt_band: int = -1

# ---- Party compose (the Band panel's PARTIES zone — NOT drawer state) ----------------------------
# The quarry the party compose sheet is aimed at (a world herd id), "" until one is picked. It is the
# FIRST question the hunt form asks, because the herd sets the useful party size, the per-policy take
# and the trip length — everything below it in the form.
var _party_quarry_id: String = ""
# The sheet's OWN autofill one-shot. Deliberately NOT `_hunt_autofill`: sharing it would let one
# surface's policy click refill the other surface's stepper.
var _party_autofill := false

# ---- The open sheet's subject identity -----------------------------------------------------------
var _kind: String = KIND_NONE
var _subject: String = ""

## Both policy fields start on the caller's default rung (`HudLayer.DEFAULT_HUNT_POLICY`), passed in
## rather than mirrored here so the policy vocabulary lives in exactly one place.
func _init(default_policy: String) -> void:
	_forage_policy = default_policy
	_hunt_policy = default_policy

# ---- Forage read accessors -----------------------------------------------------------------------

func forage_key() -> String:
	return _forage_key

func forage_count() -> int:
	return _forage_count

func forage_policy() -> String:
	return _forage_policy

func forage_species() -> String:
	return _forage_species

func forage_band() -> int:
	return _forage_band

# ---- Forage mutators -----------------------------------------------------------------------------

## A DIFFERENT tile is being composed: adopt its key and default the actor band. Re-seeding the
## count/policy/crop is a SECOND step (`seed_forage`) because the caller has to resolve the actual
## band dict — possibly falling back — before it can read that band's standing staffing.
func begin_forage_source(key: String, band_entity: int) -> void:
	_forage_key = key
	_forage_band = band_entity

## Re-seed the composed count + policy from the newly-resolved band's staffing on the new tile. The
## crop selection is cleared with them: a crop pick belongs to the PATCH it was made on, and a new
## tile has a different basket.
func seed_forage(count: int, policy: String) -> void:
	_forage_count = count
	_forage_policy = policy
	_forage_species = ""

## Forget which tile the forage compose belongs to, so the NEXT render takes the source-changed path
## and re-seeds count/policy/crop from the band's standing staffing. (`""` matches no real "x,y" key.)
## The preview harnesses' way of staging a fresh compose without faking a selection change.
func reset_forage_source() -> void:
	_forage_key = ""

func set_forage_band(entity: int) -> void:
	_forage_band = entity

func set_forage_policy(policy: String) -> void:
	_forage_policy = policy

func set_forage_count(count: int) -> void:
	_forage_count = count

func set_forage_species(species: String) -> void:
	_forage_species = species

## Arm the auto-fill one-shot (a policy CLICK, never a stepper tick).
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
## still-legal one (a policy switch changes which plants this rung can take). The read-modify-write is
## the model's; the crop RULES stay with the caller, so this holds no flora knowledge.
func resolve_forage_species(resolver: Callable) -> void:
	_forage_species = String(resolver.call(_forage_species))

# ---- Hunt read accessors -------------------------------------------------------------------------

func hunt_key() -> String:
	return _hunt_key

func hunt_count() -> int:
	return _hunt_count

## PUBLIC beyond the herd drawer, and that is a known cross-boundary read preserved verbatim through
## the lift: `HudLayer._build_policy_picker` falls back to THIS policy when invoked with no explicit
## selection, so a work-inspector or party-compose render can read the drawer's rung. `_tame_stalled_hint`
## and `_herd_crew_noun` read it from outside the builder too. Arguably a latent bug — it is fixed as
## its own labelled change, never as a side effect of an extraction.
func hunt_policy() -> String:
	return _hunt_policy

func hunt_band() -> int:
	return _hunt_band

# ---- Hunt mutators -------------------------------------------------------------------------------

## A DIFFERENT herd is being composed — the hunt twin of `begin_forage_source` (same two-step reason).
func begin_hunt_source(key: String, band_entity: int) -> void:
	_hunt_key = key
	_hunt_band = band_entity

## Re-seed the composed count + policy from the newly-resolved band's staffing on the new herd.
func seed_hunt(count: int, policy: String) -> void:
	_hunt_count = count
	_hunt_policy = policy

## The hunt twin of `reset_forage_source` — forget the herd so the next render re-seeds.
func reset_hunt_source() -> void:
	_hunt_key = ""

func set_hunt_band(entity: int) -> void:
	_hunt_band = entity

func set_hunt_policy(policy: String) -> void:
	_hunt_policy = policy

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

func set_party_quarry(herd_id: String) -> void:
	_party_quarry_id = herd_id

## No quarry: a fresh compose act, a cancelled one, a quarry that left the snapshot or migrated into
## the band's hunt reach, or a panel-band swap (a quarry is chosen FOR a band).
func clear_party_quarry() -> void:
	_party_quarry_id = ""

func arm_party_autofill() -> void:
	_party_autofill = true

func consume_party_autofill() -> bool:
	var armed := _party_autofill
	_party_autofill = false
	return armed

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
