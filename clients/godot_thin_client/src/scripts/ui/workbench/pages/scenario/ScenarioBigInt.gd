extends RefCounted
class_name ScenarioBigInt

## All-`static` lossless carrying of integers Godot's JSON cannot hold.
##
## `JSON.parse_string` reads EVERY number as a double, so an integer literal beyond 2^53 loses its low
## bits on the way in (`14083920937384765431` -> `9223372036854775807`) — and exports of random-seeded
## worlds carry full-range `u64` seeds, so an open -> Save would silently change the world. So the text is
## scanned BEFORE it is parsed: every integer literal (outside a string) whose magnitude exceeds 2^53-1 is
## replaced by a marker STRING carrying its digits, the document holds that string, and the serializer
## turns `"<marker>digits"` back into bare digits. Ordinary integers never go near any of this.

## Prefix of a big-int marker string. Printable ASCII with no JSON escaping, so `JSON.stringify` writes
## it back verbatim and `restore` can find it.
const MARKER := "~~bigint~~"
## The largest magnitude a double holds exactly (2^53 - 1), as digits. Anything above travels as a marker.
const EXACT_LIMIT := "9007199254740991"
## The widest values a scenario integer can take: `u64` max, and the magnitude of `i64` min.
const U64_MAX := "18446744073709551615"
const I64_MIN_MAGNITUDE := "9223372036854775808"
const DIGITS := "0123456789"
## What can continue a number token after its integer digits (a fraction or exponent makes it not an integer).
const NUMBER_TAIL_CHARS := "+-.eE0123456789"
const NON_INTEGER_MARKS := ".eE"

static var _restore_regex: RegEx = null


static func is_big(value: Variant) -> bool:
	return value is String and (value as String).begins_with(MARKER)


## The digits (with a leading `-` if negative) a marker carries.
static func digits(value: String) -> String:
	return value.trim_prefix(MARKER)


static func make(signed_digits: String) -> String:
	return MARKER + signed_digits


## Replace every integer literal outside a JSON string whose magnitude exceeds 2^53-1 with its marker string.
static func protect(text: String) -> String:
	var out := ""
	var flushed := 0
	var i := 0
	var count := text.length()
	var in_string := false
	while i < count:
		var c := text[i]
		if in_string:
			if c == "\\":
				i += 2
				continue
			if c == "\"":
				in_string = false
			i += 1
			continue
		if c == "\"":
			in_string = true
			i += 1
			continue
		if c == "-" or DIGITS.contains(c):
			var start := i + 1 if c == "-" else i
			var j := start
			while j < count and DIGITS.contains(text[j]):
				j += 1
			var run := j - start
			var integral := run > 0 and not (j < count and NON_INTEGER_MARKS.contains(text[j]))
			if integral and exceeds_exact(text.substr(start, run)):
				out += text.substr(flushed, i - flushed) + "\"" + MARKER + text.substr(i, j - i) + "\""
				flushed = j
			while j < count and NUMBER_TAIL_CHARS.contains(text[j]):
				j += 1
			i = maxi(j, i + 1)
			continue
		i += 1
	return out + text.substr(flushed)


## Turn every `"<marker>digits"` in serialized JSON back into bare digits.
static func restore(json_text: String) -> String:
	if _restore_regex == null:
		_restore_regex = RegEx.new()
		_restore_regex.compile("\"%s(-?[0-9]+)\"" % MARKER)
	return _restore_regex.sub(json_text, "$1", true)


## Does this digit string (no sign) exceed what a double holds exactly?
static func exceeds_exact(unsigned_digits: String) -> bool:
	return compare_digits(unsigned_digits, EXACT_LIMIT) > 0


## Compare two unsigned digit strings without leading zeros: -1, 0 or 1.
static func compare_digits(a: String, b: String) -> int:
	if a.length() != b.length():
		return -1 if a.length() < b.length() else 1
	if a == b:
		return 0
	return -1 if a < b else 1


## What a typed value becomes: an `int` when it is exact, a marker string when it is not, or `null` when it
## is not a valid value for this integer schema node (not digits, below its `minimum`, past `maximum`, or
## wider than `u64` / `i64`).
static func parse_edit(text: String, node: Dictionary) -> Variant:
	var t := text.strip_edges()
	var negative := t.begins_with("-")
	var d := t.trim_prefix("-")
	if d.is_empty():
		return null
	for k in d.length():
		if not DIGITS.contains(d[k]):
			return null
	d = d.lstrip("0")
	if d.is_empty():
		d = "0"
		negative = false
	if negative:
		if node.has(ScenarioSchema.KEY_MINIMUM) and float(node[ScenarioSchema.KEY_MINIMUM]) >= 0.0:
			return null
		if compare_digits(d, I64_MIN_MAGNITUDE) > 0:
			return null
	elif compare_digits(d, U64_MAX) > 0:
		return null
	var signed := ("-" if negative else "") + d
	if exceeds_exact(d):
		return make(signed)
	var value := int(signed)
	if node.has(ScenarioSchema.KEY_MINIMUM) and float(value) < float(node[ScenarioSchema.KEY_MINIMUM]):
		return null
	if node.has(ScenarioSchema.KEY_MAXIMUM) and float(value) > float(node[ScenarioSchema.KEY_MAXIMUM]):
		return null
	return value
