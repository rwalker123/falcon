extends RefCounted
class_name SnapshotSections

## Reads the `changed_sections` manifest a DELTA frame carries, so a consumer can skip rebuilding
## what did not move.
##
## ## Why a manifest at all
##
## Once the client renders from merged delta frames, every key is present on every frame — the
## decoder patches the cached world and republishes it whole (`native/src/snapshot/cache.rs`), which
## is what makes a merged delta indistinguishable from a full snapshot. That is correct, and it
## destroys `snapshot.has(key)` as a change signal: the block that used to skip because the key was
## absent now runs every turn. The manifest is the replacement signal, published by the native
## decoder as a `PackedStringArray` of the sections that actually MOVED — not the ones transmitted.
## A measured steady-state delta (80x52, late forager tribe) names sixteen sections and leaves
## `forage_patches`, `food_modules`, `discovered_sites`, `culture_layers`,
## `overlays.terrain`, `overlays.elevation`, `overlays.visibility`, `tiles.rivers` and
## `tiles.culture_layer` out — which is between 10 and 20 ms of `MapView` work per turn.
##
## ## The three states, and why conflating two of them breaks the world
##
## | `changed_sections` | means | [`changed`] answers |
## |---|---|---|
## | **absent** | a FULL snapshot (or a native build older than the manifest) | `true` — everything changed |
## | present, non-empty | a delta; these sections moved | membership |
## | present and **EMPTY** | a delta in which nothing moved | `false` |
##
## **Absent must not be read as "nothing changed" and empty must not be read as "absent."** The
## first mistake freezes the world on every full snapshot — the first frame, every resync, every
## new game — which is precisely the frame that has to repaint. The second is the live case: the
## server publishes a recapture delta immediately after a baseline whose manifest is genuinely
## `[]`, and GDScript's `if array:` is false for an empty array exactly as it is for `null`, so the
## natural-looking test collapses the two. Hence the explicit `has()` before the membership test.
##
## ## A static helper, not an autoload
##
## Holds no state at all — same call as `ServerPortsFile` and `ClientBuild`. It carries a
## `class_name` (unlike those two) because it is used from four different scripts and reads as
## vocabulary at the call site: `SnapshotSections.changed(snapshot, "forage_patches")`.

## The key the native decoder publishes the manifest under. Delta frames only; see the table above.
const CHANGED_SECTIONS_KEY := "changed_sections"


## Did `section` move in this frame?
##
## `section` is the dictionary key a consumer READS the world under (`populations`, never
## `population_updates`) or one of the manifest's own derived names (`overlays.terrain`,
## `tiles.rivers`). Answers `true` for any frame carrying no manifest, so a consumer gated on this
## can never be starved by a full snapshot or an older native build.
static func changed(snapshot: Dictionary, section: String) -> bool:
	if not snapshot.has(CHANGED_SECTIONS_KEY):
		return true
	var sections: Variant = snapshot[CHANGED_SECTIONS_KEY]
	if sections is PackedStringArray:
		return (sections as PackedStringArray).has(section)
	# A plain Array is not what the decoder emits, but a fixture or a future producer may hand one
	# over; treating it as "everything changed" would silently disable every gate, so read it.
	if sections is Array:
		return (sections as Array).has(section)
	# Present but not a list at all: unreadable, so fall back to the safe answer rather than
	# skipping work on the strength of a value we do not understand.
	return true


## Did ANY of `sections` move? The disjunction several call sites need — the terrain splatmaps are
## rebuilt from four independent inputs, so any one of them moving is enough.
##
## Takes a plain `Array` because the caller's list wants to be a `const`, and a
## `PackedStringArray(...)` constructor call is not a constant expression in GDScript.
static func any_changed(snapshot: Dictionary, sections: Array) -> bool:
	if not snapshot.has(CHANGED_SECTIONS_KEY):
		return true
	for section in sections:
		if changed(snapshot, section):
			return true
	return false


## Does this frame carry a manifest at all? I.e. is it a delta from a decoder that publishes one.
##
## Only for a consumer that needs the DISTINCTION rather than the answer — `MapView`'s incremental
## tile path, which may read `tile_updates` instead of `tiles` only on a frame that is genuinely a
## delta. Anything merely deciding whether to redo work wants [`changed`], which folds the absent
## case into "yes" for it.
static func has_manifest(snapshot: Dictionary) -> bool:
	return snapshot.has(CHANGED_SECTIONS_KEY)
