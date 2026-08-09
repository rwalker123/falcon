extends Node

## Headless golden gate for the **client decode path** — `SnapshotDecoder.decode_snapshot`
## (`native/src/bridge/decoder.rs`) → `snapshot_to_dict` → the nine `dict/*` builders.
##
## ## The gap this closes
##
## That path had no automated coverage at all, and it looked covered. `ui_preview` and
## `map_preview` build hand-written GDScript fixture dicts and hand them straight to
## `Hud`/`MapView`; neither file so much as names `SnapshotDecoder`, so a fully green PNG run is
## compatible with a completely broken decoder. The only in-process guard was
## `dict::population::cohort_decode_tests`, which covers exactly one struct's fixed-point scale and
## exists precisely because `VarDictionary` cannot be constructed outside a live engine. So a wrong
## dictionary key, a dropped `fixed64_to_f32` divide, or a section accessor wired to the wrong
## builder reached the running client with every CI signal green.
##
## The engine requirement is why this is a Godot scene rather than a `cargo test`: the decoder's
## return type is a `VarDictionary`, so the only way to exercise it is from inside a running engine.
##
## ## How it works
##
## Decode the generated fixture envelope (`cargo xtask decode-fixture` writes it from a synthetic
## `WorldSnapshot` in which every section is non-empty), canonicalize the resulting `Dictionary` to
## JSON, and diff it against `tests/golden/snapshot_dict.json`.
##
## **The golden is STRUCTURAL, deliberately — it is not a byte-exact dump.** Long rasters are
## recorded as `{type, len, head, tail, checksum}` rather than sample-by-sample, and floats are
## rounded to `FLOAT_DECIMALS`. This repo appends `.fbs` fields constantly, and a golden that
## rewrites thousands of lines on every tuning change trains its readers to accept the diff blind;
## a summary still fails on a raster wired to the wrong channel, which is the failure that matters.
##
## Every string in the fixture is its own wire path (`"populations[0].id"`), so the golden reads as
## a **map from wire field to dictionary key**. That is what makes a mis-wired accessor legible
## rather than merely different: the value tells you where it actually came from.
##
## A **second** fixture is decoded first and has no golden — a deliberately MALFORMED snapshot with
## no `header`, asserting the decoder DROPS that frame rather than panicking or inventing defaults.
## See `_assert_headerless_frame_is_dropped`, including what it can and cannot see.
##
## A **third and fourth** fixture are DELTAS, decoded after the baseline and then after each other:
## delta 1 applies to the snapshot, delta 2 applies to DELTA 1's output. Neither has a golden, and
## deliberately so: a delta frame is mostly the merged baseline, so recording it would triple the
## golden's surface and make it churn on every unrelated schema edit. Their properties are asserted
## directly instead — see `_assert_delta_chain`, which is the guard that would have caught `tiles`
## going stale on every delta, and (frame 2) a merge that re-bases on the original baseline.
##
##   cargo xtask decode-guard                  # regenerate fixture, build native, diff
##   cargo xtask decode-guard --write-golden   # re-record instead of diffing
##   cargo xtask decode-guard --no-build       # skip the native rebuild, when you just built it
##   godot --headless --path . res://tools/decode_guard.tscn    # after decode-fixture has run
##
## Exits 0 on PASS, 1 on FAIL (CI-usable).

const FIXTURE_PATH := "res://tests/fixtures/snapshot_envelope.bin"
const GOLDEN_PATH := "res://tests/golden/snapshot_dict.json"

## A second, deliberately MALFORMED envelope: a `WorldSnapshot` with a real map section and no
## `header`. It has no golden — it is decoded for the assertion below, not for a diff.
const HEADERLESS_FIXTURE_PATH := "res://tests/fixtures/snapshot_headerless_envelope.bin"

## A DELTA envelope whose header names `FIXTURE_PATH`'s frame as its base. Decoded after the
## baseline, never diffed — see `_assert_delta_chain`.
const DELTA_FIXTURE_PATH := "res://tests/fixtures/snapshot_delta_envelope.bin"

## A SECOND delta, whose header names DELTA 1's frame as its base and whose rows are DISJOINT from
## delta 1's. One delta only ever tests baseline → delta; the client takes delta → delta on every
## turn after the first.
const DELTA2_FIXTURE_PATH := "res://tests/fixtures/snapshot_delta2_envelope.bin"

## Frame identity, as `native/src/bridge/decoder.rs` publishes it.
const FRAME_KIND_KEY := "frame_kind"
const FRAME_KIND_DELTA := "delta"
const FRAME_SEQ_KEY := "frame_seq"
const BASE_FRAME_SEQ_KEY := "base_frame_seq"

## The keyed (diff-carried) sections whose merge is asserted, mirroring `KEYED_SECTIONS` in
## `native/src/snapshot/cache.rs`. Each rides the frame twice — as the COMPLETE array under `key`
## (patched from the baseline) and as the delta's sparse changed rows under `updates` — and the two
## must agree. `id` is the section's identity field; `probe` is any field the fixture's delta moves
## on the rows it carries.
##
## Three of the ten, not all ten: these are the ones the delta fixture moves, and the ones with live
## consumers reading the base key — `Main`'s band alerts (`populations`) and `MapView`'s per-tile
## lookups, harvest sites and culture overlay. The machinery under all ten is the same
## `SectionCache`, so a break in it fails here.
const MERGED_SECTIONS := [
	{"key": "tiles", "updates": "tile_updates", "id": "entity", "probe": "graze_biomass"},
	{"key": "populations", "updates": "population_updates", "id": "entity", "probe": "size"},
	{"key": "culture_layers", "updates": "culture_layer_updates", "id": "id", "probe": "parent"},
]

## The section whose staleness was MEASURED in the live client, named in the failure text so the
## reader gets the story rather than just the mismatch.
const TILES_KEY := "tiles"

## The WHOLE-SECTION witness, carried by delta 1 and NOT by delta 2.
##
## **It covers a hole `MERGED_SECTIONS` structurally cannot**, and the hole was measured rather than
## reasoned about. A keyed section's base key is rebuilt out of `SectionCaches` and republished on
## every frame, so it survives even a merge that re-bases the frame DICTIONARY on the original
## baseline — mutating `decode_frame`'s `cache.dict.duplicate_shallow()` that way left this guard
## PASSING while only keyed sections were probed. A whole-section field lands in the merged dict
## once, on the frame that carries it, and stays only because the NEXT delta merges into the frame
## before it. So this is the only witness here that can testify about that line.
const WHOLE_SECTION_WITNESS := {"key": "demographics", "id": "faction", "probe": "children"}

## **THE SECOND WHOLE-SECTION WITNESS, and it pins one thing `demographics` cannot: that the decoder
## republishes this field OPAQUE.**
##
## `SubsistenceSection.equipmentConfigJson` is the sim's whole effective `EquipmentConfig` as one
## `serde_json` string, and the Workbench's Equipment and Kits pages parse it themselves and walk it
## blind — which is what lets a field added to `equipment.json` reach the surface with no client edit
## (`.claude/rules/client/workbench.md` → "The two config pages PRINT the config"). So it is asserted
## by **equality on the whole string**, never `contains`: the delta's value is a JSON OBJECT, and its
## braces and quotes surviving verbatim IS the "never parsed, never re-serialised, never trimmed"
## contract. A decoder that unpacked it into typed keys and rebuilt it would fail here even if every
## field came back.
##
## It is also the whole-section field the CLIENT's own gate depends on: both pages read
## `SnapshotSections.changed(data, "equipment_config_json")`, which is the delta manifest, so the
## manifest naming it on the frame that carries it — and NOT naming it on the frame that does not — is
## asserted alongside the value.
const OPAQUE_WITNESS_KEY := "equipment_config_json"
## What the BASELINE states it as: the decode fixture's saturation-path sentinel, which gives every
## string field its own wire path as its value.
const OPAQUE_WITNESS_BASELINE := "equipment_config_json"
## …and what DELTA 1 replaces it with. **Deliberately not the baseline's value**: if the two matched,
## a decoder that ignored the delta entirely and republished the baseline would satisfy every claim
## below.
const OPAQUE_WITNESS_AFTER_DELTA := "{\"fixture\":\"delta.equipment_config_json\"}"

## The delta frame's change manifest, and the section the fixture's delta deliberately leaves
## untouched — absence from the manifest is only meaningful if something IS absent from it.
const CHANGED_SECTIONS_KEY := "changed_sections"
const UNTOUCHED_SECTION := "forage_patches"

## The two tile-derived splatmap concerns, which the decoder derives by comparing each changed tile
## with the entry it replaced. The fixture moves them on exactly one of its changed tiles.
const SECTION_TILES_RIVERS := "tiles.rivers"
const SECTION_TILES_CULTURE_LAYER := "tiles.culture_layer"

## Decimals kept on every float. The decoder divides fixed-point by 1e6 and stores some values as
## `f32`, so the last bits are not stable enough to pin — but a dropped divide moves a value by
## six orders of magnitude, which survives any rounding.
const FLOAT_DECIMALS := 6

## Packed arrays at or under this length are recorded in full; longer ones are summarized. The
## fixture's grid is 12 cells, so every raster lands under it today — the summary path exists for
## the day someone grows the fixture grid, and is exercised by nothing until then.
const INLINE_ARRAY_MAX := 64

## How many head/tail samples a summarized array keeps beside its checksum.
const SUMMARY_EDGE := 4

## Separator fed between elements when checksumming, so `[1, 23]` and `[12, 3]` cannot collide.
## An ASCII unit separator rather than a NUL: GDScript's parser rejects a `\u0000` string literal.
const ELEMENT_SEPARATOR := "\u001f"

## Differing lines reported before the diff is truncated. Enough to show a pattern, few enough that
## a wholesale change does not bury the terminal.
const MAX_DIFF_LINES := 40

var _write_golden := false

## Set by `_die`. `get_tree().quit()` only SCHEDULES the exit — the rest of the calling function
## still runs — so every caller past a possible failure checks this rather than reporting twice.
var _died := false


func _ready() -> void:
	for arg in OS.get_cmdline_user_args():
		if arg == "--write-golden":
			_write_golden = true

	# Two predicates, because registration does not imply instantiability: Godot ships abstract
	# classes (Shape2D, MultiplayerPeer) that pass `class_exists` and return null from `instantiate`.
	# Calling a method on that null raises a GDScript error that aborts `_ready` before
	# `get_tree().quit()` runs — so skipping the second check costs this tool its exit code and
	# HANGS instead of reporting.
	if not ClassDB.class_exists("SnapshotDecoder"):
		_die("SnapshotDecoder class is not registered — build the native extension first (cargo xtask godot-build).")
		return

	if not ClassDB.can_instantiate("SnapshotDecoder"):
		_die("SnapshotDecoder is registered but NOT instantiable — this gate constructs it directly, so the class must stay concrete and default-constructible: check that it is still declared #[class(init, base=RefCounted)] in native/src/bridge/decoder.rs.")
		return

	var payload := _read_fixture()
	if payload.is_empty():
		return

	var decoder: Object = ClassDB.instantiate("SnapshotDecoder")
	if decoder == null:
		_die("ClassDB.instantiate(\"SnapshotDecoder\") returned null despite the class reporting instantiable — the native extension is likely half-loaded; rebuild it with cargo xtask godot-build.")
		return

	if not _assert_headerless_frame_is_dropped(decoder):
		return

	var decoded := _decode_or_die(decoder, payload, "the fixture envelope")
	if _died:
		return

	# An empty dict is what `decode_snapshot` returns for a payload it could not parse
	# (`unwrap_or_default`), so it is the one failure that must be reported on its own terms rather
	# than as a diff against the golden.
	if decoded.is_empty():
		_die("decode_snapshot returned an EMPTY dictionary for a %d-byte envelope — the payload did not parse as a snapshot. Regenerate the fixture (cargo xtask decode-fixture) if the schema moved." % payload.size())
		return

	# Rendered BEFORE the delta is decoded: the merged frame shares its untouched values with this
	# dictionary, so the golden is taken while the baseline is provably still the baseline.
	var rendered := _canonical_json(decoded)

	if not _assert_delta_chain(decoder, decoded):
		return

	if _write_golden:
		_write_text(GOLDEN_PATH, rendered)
		print("decode_guard: WROTE golden to %s (%d keys)" % [GOLDEN_PATH, decoded.size()])
		get_tree().quit(0)
		return

	if not FileAccess.file_exists(GOLDEN_PATH):
		_die("no golden at %s — record one with: cargo xtask decode-guard --write-golden" % GOLDEN_PATH)
		return

	var golden := FileAccess.get_file_as_string(GOLDEN_PATH)
	if golden == rendered:
		print("decode_guard: PASS — decoded snapshot dictionary matches the golden (%d top-level keys)" % decoded.size())
		get_tree().quit(0)
		return

	_report_diff(golden, rendered)


## Decodes the malformed HEADERLESS envelope and asserts the decoder DROPS that frame.
##
## `header` carries no `required` attribute in `sim_schema/schemas/snapshot.fbs` and
## `root_as_envelope` verifies table STRUCTURE only, so a snapshot can parse cleanly with the field
## absent; `snapshot_to_dict` used to `unwrap()` it and take the whole client down. The contract is
## that such a frame decodes to an EMPTY dictionary — the "no frame" value
## `SnapshotLoader.poll_stream` already skips — and NOT to a dictionary filled in with header
## defaults, which would publish a world whose `tick`, `world_epoch` and (worst) `wrap_horizontal`
## are guesses rather than the server's.
##
## **The limit of this assertion, MEASURED rather than assumed, so nobody trusts it further than it
## goes:** gdext catches a Rust panic at the FFI boundary and the call still comes back with the
## method's DEFAULT value — for `decode_snapshot`, an empty `Dictionary`, the very thing asserted
## here (the engine logs `ERROR: [panic …]` and a `SCRIPT ERROR: Bug: Invalid call error code 1337`
## beside it, but the script sees a plain empty dict and sails on). So this cannot by itself tell a
## clean drop from a re-introduced `unwrap()`, and a type check on the result does not help — the
## returned Variant IS a Dictionary in both cases (that was tried). That half is caught one level up:
## `cargo xtask decode-guard` greps the run for the engine's panic report and fails on it.
func _assert_headerless_frame_is_dropped(decoder: Object) -> bool:
	if not FileAccess.file_exists(HEADERLESS_FIXTURE_PATH):
		_die("no headerless fixture at %s — generate it with: cargo xtask decode-fixture" % HEADERLESS_FIXTURE_PATH)
		return false
	var payload := FileAccess.get_file_as_bytes(HEADERLESS_FIXTURE_PATH)
	if payload.is_empty():
		_die("headerless fixture at %s is empty" % HEADERLESS_FIXTURE_PATH)
		return false

	var decoded := _decode_or_die(decoder, payload, "the HEADERLESS envelope")
	if _died:
		return false
	if not decoded.is_empty():
		_die("decode_snapshot returned %d keys for a HEADERLESS snapshot (%s) — a frame with no header carries no tick, no worldEpoch and no wrapHorizontal, so it must be DROPPED (an empty dictionary the loader skips), never decoded with header defaults." % [decoded.size(), str(decoded.keys())])
		return false

	print("decode_guard: headerless snapshot correctly dropped (empty dictionary, %d bytes in)" % payload.size())
	return true


## Decodes both DELTA fixtures — the first against the baseline, the second against the FIRST's
## output — and asserts each merged frame is an honest complete world.
##
## **The section assertion is the reason this function exists.** A delta carries only the rows that
## changed; the decoder published them under the `*_updates` key and left the base key standing, so
## every consumer reading the base key was frozen at the baseline snapshot for the life of the
## world. It was nine sections, not one. Measured on `tiles`: `graze_biomass` summed over `tiles`
## was byte-identical for nine consecutive turns while `tile_updates` carried 400-600 moved tiles
## per turn — and `Main`'s band alerts read `populations` the same way, so food warnings, idle
## workers and predator-nearby were frozen too. Nothing caught it because nothing in this gate had
## ever decoded a delta.
##
## **And nothing decoded TWO, which is a different hole.** One delta exercises baseline → delta; the
## client takes delta → delta on every turn after the first. `decode_frame` merges into
## `cache.dict.duplicate_shallow()`, where `cache` is replaced after each merge — re-base that on
## the ORIGINAL baseline and delta 2 silently discards delta 1's changes, with no error and no
## symptom. Frame 2's assertions below are what catch it, and they can only do so because the two
## fixtures move DISJOINT rows: if delta 2 rewrote delta 1's rows, losing delta 1 would leave no
## trace.
##
## Properties, each failing on its own terms:
## 1. each frame identifies as a delta and names its predecessor's frame as its base (if it did not,
##    the decoder would have DROPPED it and every assertion below would be vacuous);
## 2. for every section in `MERGED_SECTIONS`: each changed row carries that delta's value under the
##    BASE key and not the baseline's, and the base key still holds the baseline's row COUNT — a
##    delta patches the world, it never shrinks it;
## 3. **after frame 2, delta 1's rows still carry DELTA 1's values** — the cumulative assertion;
## 4. `changed_sections` names what each frame moved, plus the two splatmap concerns the one
##    river/culture tile moved, and does not name a section the delta left alone.
func _assert_delta_chain(decoder: Object, baseline: Dictionary) -> bool:
	# Captured before any merge. A patched array holds NEW dictionaries at the changed slots and
	# shares the untouched ones, so these references keep answering the pre-delta values.
	var baseline_rows := {}
	for section in MERGED_SECTIONS:
		baseline_rows[section["key"]] = _rows_by_id(baseline.get(section["key"], []), section["id"])

	# THE OPAQUE WITNESS'S PRECONDITION. Both claims about it below are "the delta's value replaced
	# the baseline's and then stayed", which say nothing at all unless the baseline established one
	# first — and unless the two values genuinely differ.
	var opaque_baseline := str(baseline.get(OPAQUE_WITNESS_KEY, ""))
	if opaque_baseline != OPAQUE_WITNESS_BASELINE:
		_die("the baseline frame states %s as '%s', not the fixture's '%s' — the delta assertions below would be comparing against nothing. Regenerate with cargo xtask decode-fixture." % [OPAQUE_WITNESS_KEY, opaque_baseline, OPAQUE_WITNESS_BASELINE])
		return false
	if OPAQUE_WITNESS_BASELINE == OPAQUE_WITNESS_AFTER_DELTA:
		_die("the fixture's baseline and delta values for %s are the SAME string — a decoder that ignored the delta entirely and republished the baseline would pass both assertions below." % OPAQUE_WITNESS_KEY)
		return false

	var frame1 := _decode_delta_or_die(decoder, DELTA_FIXTURE_PATH, "the DELTA envelope", baseline)
	if _died:
		return false
	if not _assert_merged_frame(frame1, baseline_rows, baseline, "delta 1"):
		return false

	# What delta 1 moved, read out as plain values BEFORE frame 2 exists — the merged arrays share
	# row dictionaries between frames, so a captured reference would be ambiguous evidence.
	var delta1_values := {}
	for section in MERGED_SECTIONS:
		delta1_values[section["key"]] = _probe_values(
			frame1.get(section["updates"], []), section["id"], section["probe"]
		)
	# The whole-section witness rides the base key itself (there is no sparse `*_updates` twin), and
	# delta 1 must have MOVED it — otherwise the persistence check below proves nothing.
	var witness_key: String = WHOLE_SECTION_WITNESS["key"]
	var witness_id: String = WHOLE_SECTION_WITNESS["id"]
	var witness_probe: String = WHOLE_SECTION_WITNESS["probe"]
	var witness_after_1 := _probe_values(frame1.get(witness_key, []), witness_id, witness_probe)
	var witness_baseline := _probe_values(baseline.get(witness_key, []), witness_id, witness_probe)
	if witness_after_1.is_empty():
		_die("delta 1's merged frame carries no %s rows — the whole-section witness is gone, and with it the only assertion that can see a frame-dictionary re-base (cargo xtask decode-fixture)." % witness_key)
		return false
	if witness_after_1 == witness_baseline:
		_die("delta 1 did not MOVE %s (still %s) — the persistence check after delta 2 would pass vacuously. Regenerate with cargo xtask decode-fixture." % [witness_key, str(witness_baseline)])
		return false
	if not (frame1.get(CHANGED_SECTIONS_KEY, PackedStringArray()) as PackedStringArray).has(witness_key):
		_die("delta 1's %s does not name %s even though it carried it — the manifest must name a whole-section replacement." % [CHANGED_SECTIONS_KEY, witness_key])
		return false

	# --- ASSERTION 1: the delta's own value reached the merged frame, VERBATIM ----------------
	# This is the leg that a delta path never wired up fails on: `snapshot_to_dict` alone leaves the
	# baseline's value standing for the life of the world, which is the `food_modules` /
	# `faction_inventory` staleness reached one more way. Equality on the WHOLE string, not
	# `contains` — the value is a JSON object, so its braces and quotes surviving is also the
	# "republished opaque, never parsed" contract.
	var opaque_after_1 := str(frame1.get(OPAQUE_WITNESS_KEY, ""))
	if opaque_after_1 != OPAQUE_WITNESS_AFTER_DELTA:
		var stale := opaque_after_1 == OPAQUE_WITNESS_BASELINE
		var note := " — that is the BASELINE's value, so the delta path never read this field at all and the client would run the whole world on the config it booted with" if stale else ""
		_die("delta 1's merged frame states %s as '%s', not the '%s' the delta carried%s. Every whole-section field must be decoded in BOTH snapshot_to_dict and decode_delta_against." % [OPAQUE_WITNESS_KEY, opaque_after_1, OPAQUE_WITNESS_AFTER_DELTA, note])
		return false
	if not (frame1.get(CHANGED_SECTIONS_KEY, PackedStringArray()) as PackedStringArray).has(OPAQUE_WITNESS_KEY):
		_die("delta 1's %s does not name %s even though it carried it — the Workbench's config pages gate on SnapshotSections.changed() for exactly this key, so an unnamed replacement is one they skip." % [CHANGED_SECTIONS_KEY, OPAQUE_WITNESS_KEY])
		return false

	var frame2 := _decode_delta_or_die(decoder, DELTA2_FIXTURE_PATH, "the SECOND DELTA envelope", frame1)
	if _died:
		return false
	if not _assert_merged_frame(frame2, baseline_rows, frame1, "delta 2"):
		return false

	# --- the cumulative assertion -----------------------------------------------------------
	var carried := 0
	for section in MERGED_SECTIONS:
		var key: String = section["key"]
		var id_key: String = section["id"]
		var probe_key: String = section["probe"]
		var merged_by_id := _rows_by_id(frame2.get(key, []), id_key)
		var second_ids := _probe_values(frame2.get(section["updates"], []), id_key, probe_key)
		for row_id in delta1_values[key]:
			if second_ids.has(row_id):
				_die("the two delta fixtures both moved %s %s=%d, so 'delta 1's value survived delta 2' cannot fail — the fixtures must move DISJOINT rows (cargo xtask decode-fixture; see DeltaPlan::overlaps)." % [key, id_key, row_id])
				return false
			if not merged_by_id.has(row_id):
				_die("%s %s=%d was moved by delta 1 but is missing from the merged %s array after delta 2." % [key, id_key, row_id, key])
				return false
			var still := float(merged_by_id[row_id].get(probe_key, 0.0))
			var want := float(delta1_values[key][row_id])
			if still != want:
				var baseline_value := float(baseline_rows[key].get(row_id, {}).get(probe_key, 0.0))
				var note := " — that is the BASELINE's value, so the second merge re-based on the original baseline instead of the running cache and discarded delta 1 wholesale" if still == baseline_value else ""
				_die("%s %s=%d reads %s=%f after the second delta but delta 1 had set it to %f%s. Each delta must merge into the frame BEFORE it (decode_frame's cache.dict.duplicate_shallow(), where the cache is replaced after every merge), or the world silently drifts from the server's." % [key, id_key, row_id, probe_key, still, want, note])
				return false
			carried += 1

	# --- the whole-section half of the cumulative assertion ----------------------------------
	# Delta 2 does not carry `demographics` at all, so the ONLY way its value can still be delta 1's
	# is that the second merge started from delta 1's frame.
	if (frame2.get(CHANGED_SECTIONS_KEY, PackedStringArray()) as PackedStringArray).has(witness_key):
		_die("delta 2's %s names %s, but the fixture's delta 2 must not carry it — that absence is what makes the persistence assertion below meaningful (cargo xtask decode-fixture)." % [CHANGED_SECTIONS_KEY, witness_key])
		return false
	var witness_after_2 := _probe_values(frame2.get(witness_key, []), witness_id, witness_probe)
	if witness_after_2 != witness_after_1:
		var reverted := witness_after_2 == witness_baseline
		var note := " — that is the BASELINE's value, so the second merge re-based the frame DICTIONARY on the original baseline instead of the frame before it" if reverted else ""
		_die("%s reads %s after the second delta but delta 1 had set it to %s%s. Delta 2 does not carry this section at all, so its value can only come from the frame delta 1 published — every merge must start from `cache.dict` AFTER the previous merge replaced it." % [witness_key, str(witness_after_2), str(witness_after_1), note])
		return false

	# --- ASSERTION 2: the delta's value SURVIVED a delta that does not carry it ---------------
	# **This is the one that earns its keep.** Delta 2 carries no `equipment_config_json` at all, so
	# the only thing that can keep delta 1's value on the frame is the second merge starting from the
	# frame BEFORE it rather than re-basing on the baseline. Assertion 1 passes against a re-based
	# merge; this one sees it — the same property `demographics` is the existing witness for, on a
	# field whose staleness would be silent for the life of the world.
	if (frame2.get(CHANGED_SECTIONS_KEY, PackedStringArray()) as PackedStringArray).has(OPAQUE_WITNESS_KEY):
		_die("delta 2's %s names %s, but the fixture's delta 2 must not carry it — that absence is what makes the persistence assertion below meaningful (cargo xtask decode-fixture)." % [CHANGED_SECTIONS_KEY, OPAQUE_WITNESS_KEY])
		return false
	var opaque_after_2 := str(frame2.get(OPAQUE_WITNESS_KEY, ""))
	if opaque_after_2 != opaque_after_1:
		var reverted := opaque_after_2 == OPAQUE_WITNESS_BASELINE
		var note := " — that is the BASELINE's value, so the second merge re-based the frame DICTIONARY on the original baseline instead of the frame before it" if reverted else ""
		_die("%s reads '%s' after the second delta but delta 1 had set it to '%s'%s. Delta 2 does not carry this field at all, so its value can only come from the frame delta 1 published." % [OPAQUE_WITNESS_KEY, opaque_after_2, opaque_after_1, note])
		return false

	print("decode_guard: delta chain merged (frame %d then %d on the baseline's %d; %d of delta 1's rows survived delta 2, and so did %s and %s)" % [
		int(frame1.get(FRAME_SEQ_KEY, -1)), int(frame2.get(FRAME_SEQ_KEY, -1)),
		int(baseline.get(FRAME_SEQ_KEY, -1)), carried, witness_key, OPAQUE_WITNESS_KEY,
	])
	return true


## Decode one delta envelope and pin that it was ACCEPTED against `previous` — a dropped frame
## answers an empty dictionary, and every assertion after it would pass vacuously.
func _decode_delta_or_die(decoder: Object, path: String, what: String, previous: Dictionary) -> Dictionary:
	if not FileAccess.file_exists(path):
		_die("no delta fixture at %s — generate it with: cargo xtask decode-fixture" % path)
		return {}
	var payload := FileAccess.get_file_as_bytes(path)
	if payload.is_empty():
		_die("delta fixture at %s is empty" % path)
		return {}

	var merged := _decode_or_die(decoder, payload, what)
	if _died:
		return {}
	if merged.is_empty():
		_die("decode_snapshot returned an EMPTY dictionary for %s (%d bytes) — the frame was DROPPED. Its header must name the preceding frame's frame_seq (%s) as baseFrameSeq and carry the same worldEpoch; regenerate with cargo xtask decode-fixture." % [what, payload.size(), str(previous.get(FRAME_SEQ_KEY))])
		return {}
	if str(merged.get(FRAME_KIND_KEY, "")) != FRAME_KIND_DELTA:
		_die("%s reports %s=%s, not %s — the frame kind is read straight off Envelope::payload_type, so this means the fixture is not a delta envelope at all." % [what, FRAME_KIND_KEY, str(merged.get(FRAME_KIND_KEY)), FRAME_KIND_DELTA])
		return {}
	if int(merged.get(BASE_FRAME_SEQ_KEY, -1)) != int(previous.get(FRAME_SEQ_KEY, -2)):
		_die("%s's %s is %d but the frame before it published %s %d — a delta applied to a frame the client never held merges into the wrong state, and a chained fixture that names the BASELINE re-tests nothing." % [what, BASE_FRAME_SEQ_KEY, int(merged.get(BASE_FRAME_SEQ_KEY, -1)), FRAME_SEQ_KEY, int(previous.get(FRAME_SEQ_KEY, -2))])
		return {}
	return merged


## Per-section assertions for ONE merged delta frame: its own changed rows landed under the base
## key, the row counts survived, and the manifest names what it moved.
func _assert_merged_frame(merged: Dictionary, baseline_rows: Dictionary, previous: Dictionary, what: String) -> bool:
	if not merged.has(CHANGED_SECTIONS_KEY):
		_die("%s carries no %s — the change manifest rides EVERY delta (its absence is reserved for a full snapshot, where it means 'everything changed')." % [what, CHANGED_SECTIONS_KEY])
		return false
	var changed: PackedStringArray = merged.get(CHANGED_SECTIONS_KEY, PackedStringArray())

	for section in MERGED_SECTIONS:
		var key: String = section["key"]
		var id_key: String = section["id"]
		var probe_key: String = section["probe"]
		var baseline_by_id: Dictionary = baseline_rows[key]

		var merged_array: Array = merged.get(key, [])
		if merged_array.size() != baseline_by_id.size():
			_die("%s carries %d %s rows but the baseline carried %d — a delta patches a section, it never shrinks it." % [what, merged_array.size(), key, baseline_by_id.size()])
			return false
		var merged_by_id := _rows_by_id(merged_array, id_key)

		var updates: Array = merged.get(section["updates"], [])
		if updates.is_empty():
			_die("%s changed NO %s rows (%s is empty), so the merge assertion for that section would pass vacuously — regenerate it with cargo xtask decode-fixture." % [what, key, section["updates"]])
			return false
		if not changed.has(key):
			_die("%s's %s does not name %s (%s) even though it changed %d of its rows — an UNDER-complete manifest makes consumers skip a section that really moved." % [what, CHANGED_SECTIONS_KEY, key, str(changed), updates.size()])
			return false

		for update in updates:
			var row_id := int(update.get(id_key, -1))
			if not merged_by_id.has(row_id):
				_die("%s: %s %s=%d was changed but is missing from the merged %s array — the patched section must contain every row the delta carried." % [what, key, id_key, row_id, key])
				return false
			var merged_value := float(merged_by_id[row_id].get(probe_key, 0.0))
			var delta_value := float(update.get(probe_key, 0.0))
			var previous_value := float(_rows_by_id(previous.get(key, []), id_key).get(row_id, {}).get(probe_key, 0.0))
			if previous_value == delta_value:
				_die("%s's %s %s=%d carries the same %s as the frame before it (%f), so 'the merged frame moved' cannot be observed — regenerate it with cargo xtask decode-fixture." % [what, key, id_key, row_id, probe_key, delta_value])
				return false
			if merged_value != delta_value:
				_die("%s: %s %s=%d reads %s=%f in the merged %s array but the delta carried %f (the frame before had %f) — the merged world is STALE: the decoder published the sparse %s list without patching %s, so every consumer reading %s is frozen." % [what, key, id_key, row_id, probe_key, merged_value, key, delta_value, previous_value, section["updates"], key, key])
				return false

	for section_name in [SECTION_TILES_RIVERS, SECTION_TILES_CULTURE_LAYER]:
		if not changed.has(section_name):
			_die("%s's %s does not name %s (%s) even though one of its tiles moved that field — the terrain splatmaps would never be rebuilt, so a river or a culture border would appear only after the next full snapshot." % [what, CHANGED_SECTIONS_KEY, section_name, str(changed)])
			return false
	if changed.has(UNTOUCHED_SECTION):
		_die("%s's %s names %s, which it does not carry (%s) — a manifest that names everything says nothing, and consumers gain no work to skip." % [what, CHANGED_SECTIONS_KEY, UNTOUCHED_SECTION, str(changed)])
		return false
	return true


## `{id: probe value}` for one frame's sparse changed-row list — the values that frame moved,
## flattened out of the dictionaries so a later frame cannot alias them.
func _probe_values(rows: Variant, id_key: String, probe_key: String) -> Dictionary:
	var values := {}
	if rows is Array:
		for row in rows:
			if row is Dictionary:
				values[int(row.get(id_key, -1))] = float(row.get(probe_key, 0.0))
	return values


## Index one section's rows by their identity field, so a merged row can be found by the id the
## delta named it with rather than by position (a patched array need not preserve order).
func _rows_by_id(rows: Variant, id_key: String) -> Dictionary:
	var by_id := {}
	if rows is Array:
		for row in rows:
			if row is Dictionary:
				by_id[int(row.get(id_key, -1))] = row
	return by_id


## Calls the REAL decoder and hands back its dictionary.
##
## **This exists to stop the gate HANGING, and the untyped local is the whole trick.** gdext wraps
## every `#[func]` in a panic guard: a Rust panic inside the decoder does not unwind into the engine,
## it is logged and the call comes back flagged as failed (`SCRIPT ERROR: Bug: Invalid call error
## code 1337`). A failed call assigned straight into a `: Dictionary` local ABORTS the calling
## function — so `get_tree().quit()` never runs and the headless process sits there **forever**
## instead of failing, which is what the old `header().unwrap()` did when it was put back to check
## (measured: 23 minutes, killed by hand). Taking the result as a `Variant` keeps the script alive
## through it, so the run finishes and `cargo xtask decode-guard`'s panic grep gets to speak.
##
## It is deliberately NOT a panic detector: the value that comes back is the method's DEFAULT, i.e.
## an empty `Dictionary`, indistinguishable from a frame the decoder dropped on purpose. Detection
## lives in the xtask runner; this only guarantees there is still a run for it to read.
func _decode_or_die(decoder: Object, payload: PackedByteArray, what: String) -> Dictionary:
	var result: Variant = decoder.decode_snapshot(payload)
	if typeof(result) != TYPE_DICTIONARY:
		_die("decode_snapshot returned %s, not a Dictionary, for %s — the decoder call failed outright. Look for a Rust panic or a signature change in the output above." % [type_string(typeof(result)), what])
		return {}
	return result


func _read_fixture() -> PackedByteArray:
	if not FileAccess.file_exists(FIXTURE_PATH):
		_die("no fixture at %s — generate it with: cargo xtask decode-fixture" % FIXTURE_PATH)
		return PackedByteArray()
	var payload := FileAccess.get_file_as_bytes(FIXTURE_PATH)
	if payload.is_empty():
		_die("fixture at %s is empty" % FIXTURE_PATH)
	return payload


# ---------------------------------------------------------------------------
# Canonical rendering
# ---------------------------------------------------------------------------
# Written by hand rather than through `JSON.stringify` for three reasons the golden depends on:
# key order must be sorted (a `Dictionary`'s insertion order is an implementation detail of the
# decoder, not a contract), floats must be rounded to a stable width, and the Packed* types
# `JSON.stringify` flattens into anonymous arrays must keep their TYPE — a raster arriving as
# `PackedFloat32Array` where the client expects `PackedInt32Array` is exactly the kind of wiring
# error this gate exists to catch.

func _canonical_json(value: Variant) -> String:
	var out := PackedStringArray()
	_render(value, "", out)
	out.append("")  # trailing newline, so the file is diff-friendly
	return "\n".join(out)


func _render(value: Variant, indent: String, out: PackedStringArray) -> void:
	match typeof(value):
		TYPE_DICTIONARY:
			_render_dict(value as Dictionary, indent, out)
		TYPE_ARRAY:
			_render_array(value as Array, indent, out)
		_:
			out.append(indent + _scalar(value))


func _render_dict(dict: Dictionary, indent: String, out: PackedStringArray) -> void:
	if dict.is_empty():
		out.append(indent + "{}")
		return
	var keys := dict.keys()
	keys.sort_custom(func(a: Variant, b: Variant) -> bool: return str(a) < str(b))
	out.append(indent + "{")
	var inner := indent + "  "
	for i in keys.size():
		var key: Variant = keys[i]
		var comma := "," if i < keys.size() - 1 else ""
		var child: Variant = dict[key]
		var head := "%s%s: " % [inner, _json_string(str(key))]
		if typeof(child) == TYPE_DICTIONARY or typeof(child) == TYPE_ARRAY:
			var block := PackedStringArray()
			_render(child, inner, block)
			# Splice the key onto the child's opening line so nesting reads as one structure.
			block[0] = head + block[0].substr(inner.length())
			block[block.size() - 1] += comma
			out.append_array(block)
		else:
			out.append(head + _scalar(child) + comma)
	out.append(indent + "}")


func _render_array(array: Array, indent: String, out: PackedStringArray) -> void:
	if array.is_empty():
		out.append(indent + "[]")
		return
	out.append(indent + "[")
	var inner := indent + "  "
	for i in array.size():
		var block := PackedStringArray()
		_render(array[i], inner, block)
		if i < array.size() - 1:
			block[block.size() - 1] += ","
		out.append_array(block)
	out.append(indent + "]")


## Renders one non-container value. Packed arrays are containers to Godot but are recorded as a
## single tagged scalar here, so a raster occupies one golden line instead of thousands.
func _scalar(value: Variant) -> String:
	match typeof(value):
		TYPE_NIL:
			return "null"
		TYPE_BOOL:
			return "true" if value else "false"
		TYPE_INT:
			return str(value)
		TYPE_FLOAT:
			return _float(value)
		TYPE_STRING, TYPE_STRING_NAME:
			return _json_string(str(value))
		TYPE_VECTOR2I:
			var vi: Vector2i = value
			return '{"__type": "Vector2i", "x": %d, "y": %d}' % [vi.x, vi.y]
		TYPE_VECTOR2:
			var v: Vector2 = value
			return '{"__type": "Vector2", "x": %s, "y": %s}' % [_float(v.x), _float(v.y)]
		TYPE_COLOR:
			var c: Color = value
			return '{"__type": "Color", "hex": "%s"}' % c.to_html()
		TYPE_PACKED_BYTE_ARRAY:
			return _packed("PackedByteArray", Array(value as PackedByteArray))
		TYPE_PACKED_INT32_ARRAY:
			return _packed("PackedInt32Array", Array(value as PackedInt32Array))
		TYPE_PACKED_INT64_ARRAY:
			return _packed("PackedInt64Array", Array(value as PackedInt64Array))
		TYPE_PACKED_FLOAT32_ARRAY:
			return _packed("PackedFloat32Array", Array(value as PackedFloat32Array))
		TYPE_PACKED_FLOAT64_ARRAY:
			return _packed("PackedFloat64Array", Array(value as PackedFloat64Array))
		TYPE_PACKED_STRING_ARRAY:
			return _packed("PackedStringArray", Array(value as PackedStringArray))
		_:
			# An unhandled Variant type in the decoded dict is itself a finding: the client's
			# consumers only ever expect the types above, so name it rather than stringify it away.
			return '{"__unhandled_variant_type": %d, "as_string": %s}' % [typeof(value), _json_string(str(value))]


func _packed(type_name: String, items: Array) -> String:
	var rendered := PackedStringArray()
	if items.size() <= INLINE_ARRAY_MAX:
		for item in items:
			rendered.append(_scalar(item))
		return '{"__packed": "%s", "len": %d, "items": [%s]}' % [type_name, items.size(), ", ".join(rendered)]

	# Summary form. The checksum is what actually distinguishes two same-length rasters; head and
	# tail are there so a human reading a failure can see WHICH raster this is.
	for i in SUMMARY_EDGE:
		rendered.append(_scalar(items[i]))
	var tail := PackedStringArray()
	for i in range(items.size() - SUMMARY_EDGE, items.size()):
		tail.append(_scalar(items[i]))
	return '{"__packed": "%s", "len": %d, "head": [%s], "tail": [%s], "checksum": "%s"}' % [
		type_name, items.size(), ", ".join(rendered), ", ".join(tail), _checksum(items)
	]


## A stable digest of a summarized array. Hashes the *canonical text* of each element rather than
## the raw bits, so it inherits `_float`'s rounding and cannot fail on last-bit float noise.
func _checksum(items: Array) -> String:
	var ctx := HashingContext.new()
	ctx.start(HashingContext.HASH_SHA256)
	for item in items:
		ctx.update((_scalar(item) + ELEMENT_SEPARATOR).to_utf8_buffer())
	return ctx.finish().hex_encode().substr(0, 16)


func _float(value: float) -> String:
	if is_nan(value):
		return '"NaN"'
	if is_inf(value):
		return '"Inf"' if value > 0.0 else '"-Inf"'
	var text := String.num(value, FLOAT_DECIMALS)
	# Godot renders -0.0 as "-0"; fold it so a sign bit cannot flip the golden.
	if text == "-0" or text == "-0." + "0".repeat(FLOAT_DECIMALS):
		text = "0"
	return text


func _json_string(text: String) -> String:
	var escaped := text.replace("\\", "\\\\").replace("\"", "\\\"")
	escaped = escaped.replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t")
	return "\"%s\"" % escaped


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------

func _report_diff(golden: String, actual: String) -> void:
	var want := golden.split("\n")
	var got := actual.split("\n")
	printerr("decode_guard: FAIL — the decoded snapshot dictionary does not match the golden.")
	printerr("  golden: %s (%d lines)" % [GOLDEN_PATH, want.size()])
	printerr("  actual: %d lines" % got.size())
	printerr("  If the change is intended, re-record with: cargo xtask decode-guard --write-golden")
	printerr("  First differing lines (- golden, + actual):")

	var shown := 0
	var limit: int = maxi(want.size(), got.size())
	for i in limit:
		var a: String = want[i] if i < want.size() else "<missing>"
		var b: String = got[i] if i < got.size() else "<missing>"
		if a == b:
			continue
		if shown >= MAX_DIFF_LINES:
			printerr("    … diff truncated after %d lines" % MAX_DIFF_LINES)
			break
		printerr("    %5d - %s" % [i + 1, a])
		printerr("    %5d + %s" % [i + 1, b])
		shown += 1

	get_tree().quit(1)


func _write_text(path: String, text: String) -> void:
	var dir := path.get_base_dir()
	if not DirAccess.dir_exists_absolute(ProjectSettings.globalize_path(dir)):
		DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(dir))
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		_die("could not open %s for writing (%s)" % [path, error_string(FileAccess.get_open_error())])
		return
	file.store_string(text)
	file.close()


func _die(message: String) -> void:
	printerr("decode_guard: FAIL — ", message)
	_died = true
	get_tree().quit(1)
