extends RefCounted
class_name ScenarioSchema

## All-`static` reading of the scenario file's JSON Schema (draft 2020-12, fully inlined, generated
## server-side from the loader structs — `core_sim::scenario::scenario_schema`).
##
## **It names no scenario key.** Every question here is about the SHAPE of a schema node — "is this a
## map", "is this a oneOf of single-key objects" — so a key added to any section of the file appears in
## the form with no client edit, and a renamed one renames itself. The skeleton a `+ add` inserts is
## derived the same way.

const KEY_TYPE := "type"
const KEY_PROPERTIES := "properties"
const KEY_REQUIRED := "required"
const KEY_ITEMS := "items"
const KEY_ONE_OF := "oneOf"
const KEY_CONST := "const"
const KEY_ADDITIONAL := "additionalProperties"
const KEY_MINIMUM := "minimum"
const KEY_MAXIMUM := "maximum"
const KEY_MIN_ITEMS := "minItems"
const KEY_DESCRIPTION := "description"

const TYPE_OBJECT := "object"
const TYPE_ARRAY := "array"
const TYPE_STRING := "string"
const TYPE_INTEGER := "integer"
const TYPE_NUMBER := "number"
const TYPE_BOOLEAN := "boolean"
const TYPE_NULL := "null"


## The node's `type`, always as an array (`"string"` and `["string","null"]` both read the same way).
static func types(node: Dictionary) -> Array:
	var raw: Variant = node.get(KEY_TYPE, null)
	if raw is Array:
		return raw
	if raw is String:
		return [raw]
	return []


static func is_nullable(node: Dictionary) -> bool:
	return types(node).has(TYPE_NULL)


## The node's first non-null type, or `""` for a node with no `type` (a `oneOf`).
static func base_type(node: Dictionary) -> String:
	for t in types(node):
		if t != TYPE_NULL:
			return String(t)
	return ""


static func description(node: Dictionary) -> String:
	return String(node.get(KEY_DESCRIPTION, ""))


static func one_of(node: Dictionary) -> Array:
	var raw: Variant = node.get(KEY_ONE_OF, [])
	return raw if raw is Array else []


## A plain string enum: a non-empty `oneOf` whose every entry is a `const`.
static func is_const_choice(node: Dictionary) -> bool:
	var options := one_of(node)
	if options.is_empty():
		return false
	for option in options:
		if not (option is Dictionary) or not (option as Dictionary).has(KEY_CONST):
			return false
	return true


static func const_values(node: Dictionary) -> Array:
	var out: Array = []
	for option in one_of(node):
		out.append((option as Dictionary).get(KEY_CONST))
	return out


## An externally-tagged enum: a non-empty `oneOf` whose every entry is an object with ONE property.
static func is_variant_choice(node: Dictionary) -> bool:
	var options := one_of(node)
	if options.is_empty():
		return false
	for option in options:
		if not (option is Dictionary):
			return false
		var props: Variant = (option as Dictionary).get(KEY_PROPERTIES, null)
		if not (props is Dictionary) or (props as Dictionary).size() != 1:
			return false
	return true


## The single key a variant entry carries.
static func variant_key(variant: Dictionary) -> String:
	var props: Dictionary = variant.get(KEY_PROPERTIES, {})
	for key in props:
		return String(key)
	return ""


## The variant entry named `key`, or an empty dictionary.
static func variant_named(node: Dictionary, key: String) -> Dictionary:
	for option in one_of(node):
		if variant_key(option) == key:
			return option
	return {}


static func variant_keys(node: Dictionary) -> Array:
	var out: Array = []
	for option in one_of(node):
		out.append(variant_key(option))
	return out


## A map: an object whose values are one schema (`outfit.kits`), as opposed to a struct.
static func is_map(node: Dictionary) -> bool:
	return base_type(node) == TYPE_OBJECT and node.get(KEY_ADDITIONAL, false) is Dictionary


static func map_value_schema(node: Dictionary) -> Dictionary:
	var raw: Variant = node.get(KEY_ADDITIONAL, {})
	return raw if raw is Dictionary else {}


static func items(node: Dictionary) -> Dictionary:
	var raw: Variant = node.get(KEY_ITEMS, {})
	return raw if raw is Dictionary else {}


static func properties(node: Dictionary) -> Dictionary:
	var raw: Variant = node.get(KEY_PROPERTIES, {})
	return raw if raw is Dictionary else {}


static func required_keys(node: Dictionary) -> Array:
	var raw: Variant = node.get(KEY_REQUIRED, [])
	return raw if raw is Array else []


static func is_required(node: Dictionary, key: String) -> bool:
	return required_keys(node).has(key)


## Property names in the order the form draws them: the REQUIRED ones first in `required`'s order (the
## loader struct's field order — the schema's own `properties` are alphabetical), then the optional
## ones in schema order.
static func ordered_keys(node: Dictionary) -> Array:
	var props := properties(node)
	var out: Array = []
	for key in required_keys(node):
		if props.has(key):
			out.append(key)
	for key in props:
		if not out.has(key):
			out.append(key)
	return out


## The `x-pick` hint, or `""`.
static func pick_hint(node: Dictionary) -> String:
	return String(node.get(WorkbenchVocab.SCHEMA_PICK_KEY, ""))


## The two keys of an `x-pick-tile` object (`[x_key, y_key]`), or an empty array.
static func pick_tile_keys(node: Dictionary) -> Array:
	var raw: Variant = node.get(WorkbenchVocab.SCHEMA_PICK_TILE_KEYS, [])
	return raw if raw is Array and (raw as Array).size() == 2 else []


## The value a freshly added node of this schema holds: required keys only; string `""`; a number its
## minimum (clamped under its maximum) or 0; bool false; array `[]` (or `minItems` skeleton items, so
## a tile is `[0, 0]`); object recursed; oneOf-of-objects its first variant; const-oneOf its first
## const; a node whose only type is `null` is null.
static func skeleton(node: Dictionary, depth: int = 0) -> Variant:
	if depth > WorkbenchVocab.FORM_MAX_DEPTH:
		return null
	if is_const_choice(node):
		return const_values(node)[0]
	var options := one_of(node)
	if not options.is_empty():
		return skeleton(options[0], depth + 1)
	match base_type(node):
		TYPE_OBJECT:
			var out := {}
			if is_map(node):
				return out
			var props := properties(node)
			for key in ordered_keys(node):
				if is_required(node, key):
					out[key] = skeleton(props[key], depth + 1)
			return out
		TYPE_ARRAY:
			var list: Array = []
			var count := int(node.get(KEY_MIN_ITEMS, 0))
			for _i in count:
				list.append(skeleton(items(node), depth + 1))
			return list
		TYPE_STRING:
			return ""
		TYPE_BOOLEAN:
			return false
		TYPE_INTEGER, TYPE_NUMBER:
			return number_skeleton(node)
	return null


static func number_skeleton(node: Dictionary) -> Variant:
	var value := 0.0
	if node.has(KEY_MINIMUM):
		value = float(node[KEY_MINIMUM])
	if node.has(KEY_MAXIMUM):
		value = minf(value, float(node[KEY_MAXIMUM]))
	return int(value) if base_type(node) == TYPE_INTEGER else value


## Reorder `object`'s keys into the schema's order (unknown keys last, in their own order), in place.
## Called when a key is ADDED, so a new optional key lands where the file's authors would put it.
static func reorder(object: Dictionary, node: Dictionary) -> void:
	var rest := object.duplicate()
	object.clear()
	for key in ordered_keys(node):
		if rest.has(key):
			object[key] = rest[key]
			rest.erase(key)
	for key in rest:
		object[key] = rest[key]


## Make a parsed document honest about integers. `JSON.parse_string` reads EVERY number as a float, and
## `JSON.stringify` writes `5.0` for one — which the server's `u32` fields refuse. Every node the schema
## types `integer` becomes an `int`; everything else is left exactly as parsed. Returns the value
## (a float becomes a new value, so the caller assigns the result).
static func normalize(value: Variant, node: Dictionary, depth: int = 0) -> Variant:
	if depth > WorkbenchVocab.FORM_MAX_DEPTH:
		return value
	if value is float:
		if types(node).has(TYPE_INTEGER) and not types(node).has(TYPE_NUMBER):
			return int(value)
		return value
	if value is Array:
		var list: Array = value
		var item_schema := items(node)
		for i in list.size():
			list[i] = normalize(list[i], item_schema, depth + 1)
		return list
	if value is Dictionary:
		var object: Dictionary = value
		var options := one_of(node)
		if not options.is_empty() and is_variant_choice(node):
			for key in object:
				var variant := variant_named(node, String(key))
				if not variant.is_empty():
					var inner: Dictionary = properties(variant).get(key, {})
					object[key] = normalize(object[key], inner, depth + 1)
			return object
		var props := properties(node)
		var map_schema := map_value_schema(node)
		for key in object:
			var child: Dictionary = props.get(key, map_schema)
			object[key] = normalize(object[key], child, depth + 1)
		return object
	return value
