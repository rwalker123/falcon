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
## Decode the committed fixture envelope (`cargo xtask decode-fixture` writes it from a synthetic
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
##   cargo xtask decode-guard                  # regenerate fixture, build native, diff
##   cargo xtask decode-guard --write-golden   # re-record instead of diffing
##   godot --headless --path . res://tools/decode_guard.tscn    # standalone, current fixture
##
## Exits 0 on PASS, 1 on FAIL (CI-usable).

const FIXTURE_PATH := "res://tests/fixtures/snapshot_envelope.bin"
const GOLDEN_PATH := "res://tests/golden/snapshot_dict.json"

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


func _ready() -> void:
	for arg in OS.get_cmdline_user_args():
		if arg == "--write-golden":
			_write_golden = true

	if not ClassDB.class_exists("SnapshotDecoder"):
		_die("SnapshotDecoder class is not registered — build the native extension first (cargo xtask godot-build).")
		return

	var payload := _read_fixture()
	if payload.is_empty():
		return

	var decoder: Object = ClassDB.instantiate("SnapshotDecoder")
	var decoded: Dictionary = decoder.decode_snapshot(payload)

	# An empty dict is what `decode_snapshot` returns for a payload it could not parse
	# (`unwrap_or_default`), so it is the one failure that must be reported on its own terms rather
	# than as a diff against the golden.
	if decoded.is_empty():
		_die("decode_snapshot returned an EMPTY dictionary for a %d-byte envelope — the payload did not parse as a snapshot. Regenerate the fixture (cargo xtask decode-fixture) if the schema moved." % payload.size())
		return

	var rendered := _canonical_json(decoded)

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
	get_tree().quit(1)
