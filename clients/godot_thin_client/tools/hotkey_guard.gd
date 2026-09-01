extends Node

## **THE GATE ON THE KEYBOARD** — the assertion that did not exist while `Ctrl+C` re-centred the map.
##
## Three commits on this branch each fixed the key the player happened to press next. Nothing failed
## when a new consumer appeared, because the hotkeys were literals scattered across two files and
## **nothing could enumerate them**. `src/scripts/KeyboardArbiter.gd` is the roster that fixed that;
## this is the thing that walks it. It is deliberately shaped like
## `core_sim/tests/sim_state_coverage.rs`: it enumerates the live registry and **fails on anything
## unclassified**, so adding a hotkey extends the test whether the author meant to or not.
##
## Five parts, each a different way the bug came back:
##
##   A. **THE SOURCE SCAN** — every keyboard read in `src/` must be accounted for by a registry row
##      (or by one of the three named exceptions). This is the part that catches the NEXT consumer:
##      a new `Input.is_action_just_pressed("toggle_thing")` fails here on the day it is written.
##   B. **THE POLICY** — every registry row against every owner. The one place "gameplay keys are
##      suppressed" is stated as a fact rather than implied by five separate `if`s.
##   C. **THE MODIFIER AXIS** — every registry row, bare and under each of the four modifiers,
##      through the REAL matchers. `Ctrl+C` is asserted by name.
##   D. **THE LIVE POLL** — the modifier axis again, but through `Input` itself, so the claim that
##      `exact_match = true` is what saves us is measured rather than reasoned about.
##   E. **THE SITE** — `MapView._process` driven with real `Input` state, so the suppression is
##      observed on `pan_offset` and not merely asserted about the predicate it branches on.
##
## Run as a scene (NOT `--script`: `MapView.gd` reaches autoloads that only register when the project
## is loaded). No GPU needed — no pixels are captured:
##   godot --headless --path clients/godot_thin_client res://tools/hotkey_guard.tscn
## Exits 0 on PASS, 1 on FAIL (CI-usable). `cargo xtask hotkey-guard` is the wrapper.

const KeyboardArbiter := preload("res://src/scripts/KeyboardArbiter.gd")
const MAP_VIEW := preload("res://src/scripts/MapView.gd")

# ---- part A: what the source scan reads and what it allows ---------------------------------------

## Every `.gd` beneath this is scanned. The harnesses in `tools/` are NOT: they exist to drive input
## and would have to be exempted line by line, which is how an exemption list stops meaning anything.
const SRC_ROOT := "res://src"
const GDSCRIPT_SUFFIX := ".gd"

## `Input.<member>` reads that consume an InputMap ACTION. Each one must name a registry row and pass
## `exact_match`.
const POLLED_ACTION_READS := [
	"is_action_pressed", "is_action_just_pressed", "is_action_just_released",
	"get_action_strength", "get_action_raw_strength", "get_axis", "get_vector",
]
## `Input.<member>` reads that bypass the InputMap entirely. There is no legitimate use in this
## client: a raw device query cannot be arbitrated, because nothing names which key it is asking for.
const RAW_KEY_READS := ["is_key_pressed", "is_physical_key_pressed"]
## `Input.<member>` calls that are WRITES or pointer state, not keyboard reads. Anything not in one of
## these three lists is UNCLASSIFIED and fails — the point of the model this guard copies.
const NEUTRAL_INPUT_MEMBERS := [
	"parse_input_event", "warp_mouse", "get_mouse_button_mask", "get_last_mouse_velocity",
	"set_default_cursor_shape", "set_custom_mouse_cursor", "flush_buffered_events",
	"set_use_accumulated_input", "action_press", "action_release",
]

## **THE THREE ALLOWED EXCEPTIONS**, each with the reason it is one. `site` is `<file stem>.<func>`,
## the same spelling the registry's `SITE_*` constants use.
const EXCEPTIONS := [
	{
		"site": "HarvestFloorChart._handle_key",
		"pattern": "keycode",
		"reason": "a Control's own `_gui_input`: the arrows/Home/End that drive the floor dial only "
			+ "ever reach it while it holds focus, so it is scoped by the GUI pass, not by the arbiter",
	},
	{
		"site": "MapView._process",
		"pattern": "is_mouse_button_pressed",
		"reason": "the drag-latch release reads a MOUSE button, and no text field competes for one",
	},
	{
		"site": "Main._unhandled_input",
		"pattern": "is_action_pressed",
		"reason": "`ui_cancel` — ESCAPE is allowed under every owner and is routed by "
			+ "`Main.escape_claimant`, which is the arbiter for that one key",
	},
]

## Every spelling of a keycode comparison. `physical_keycode` contains `keycode`, and `match
## key.keycode:` is the same read wearing different punctuation, so the scan looks for the SUBSTRING
## rather than for `keycode ==` — a sweep for the spellings that come to mind is exactly what let the
## second polled site through last time.
const KEYCODE_TOKEN := "keycode"
## The one file allowed to compare keycodes: the matcher itself (`is_bare_key` / `is_escape_key`) and
## the binding table it registers from.
const KEYCODE_MATCHER_FILE := "KeyboardArbiter.gd"

# ---- parts C/D/E fixtures -------------------------------------------------------------------------

## The four modifiers a key can be pressed under, by the `InputEventKey` property that sets each. Held
## as data so every registry row is tested against every one of them.
const MODIFIER_PROPERTIES := ["ctrl_pressed", "alt_pressed", "meta_pressed", "shift_pressed"]
## The combination the player actually reported, asserted by name so it can never regress quietly.
const REPORTED_CTRL_C := {"property": "ctrl_pressed", "keycode": KEY_C, "label": "Ctrl+C"}
## The other two the player hit: `Ctrl+R` (event dock) and `Cmd+W` (pan).
const REPORTED_CTRL_R := {"property": "ctrl_pressed", "action": "toggle_event_dock", "label": "Ctrl+R"}
const REPORTED_CMD_W := {"property": "meta_pressed", "action": "map_pan_up", "label": "Cmd+W"}

## Part E's map. Big enough that the pan is not clamped away to nothing — on a grid that fits the
## viewport `_clamp_pan_offset` pins `pan_offset` at zero and every leg would read "no pan", passing
## the suppression legs for the wrong reason. The liveness leg (bare key DOES pan) is what catches
## that, and it is asserted first.
const PAN_GRID_WIDTH := 400
const PAN_GRID_HEIGHT := 400
## One frame at 60 Hz — `_process` integrates the pan by delta, so it needs a plausible one.
const FRAME_DELTA := 1.0 / 60.0

var _failures: Array[String] = []
var _checks := 0


func _ready() -> void:
	_bind_actions()
	_scan_sources()
	_assert_project_declares_no_input_map()
	_assert_policy()
	_assert_modifier_axis()
	_assert_live_poll()
	await _assert_map_view_site()
	_finish()


## **THE BINDINGS ARE REGISTERED THROUGH THE SHIPPED PATH**, not stubbed here. Neither `Main._ready`
## nor `MapView._ready` has run at this point, so nothing else would have bound a key and every leg
## below would fail on a missing action — and going through `ensure_action_bindings` means the
## registration itself is under test: a registry row whose keycode does not reach the InputMap fails
## part C's liveness leg.
func _bind_actions() -> void:
	KeyboardArbiter.ensure_action_bindings(KeyboardArbiter.CLASS_MAP_MOTION)
	KeyboardArbiter.ensure_action_bindings(KeyboardArbiter.CLASS_PANEL_TOGGLE)


# =====================================================================================================
# PART A — the source scan
# =====================================================================================================

## Walk every `.gd` under `src/` and classify every keyboard read in it. A read that no registry row
## and no exception accounts for is a failure, whatever it does.
func _scan_sources() -> void:
	var files := _gd_files_under(SRC_ROOT)
	if files.is_empty():
		_fail("scan: found no `%s` files under %s — the scan proved nothing"
			% [GDSCRIPT_SUFFIX, SRC_ROOT])
		return
	for path in files:
		_scan_file(path)


func _gd_files_under(dir_path: String) -> Array[String]:
	var found: Array[String] = []
	var dir := DirAccess.open(dir_path)
	if dir == null:
		return found
	dir.list_dir_begin()
	var name := dir.get_next()
	while name != "":
		var child := dir_path.path_join(name)
		if dir.current_is_dir():
			found.append_array(_gd_files_under(child))
		elif name.ends_with(GDSCRIPT_SUFFIX):
			found.append(child)
		name = dir.get_next()
	dir.list_dir_end()
	return found


func _scan_file(path: String) -> void:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		_fail("scan: cannot read %s" % path)
		return
	var stem := path.get_file().trim_suffix(GDSCRIPT_SUFFIX)
	var text := file.get_as_text()
	file.close()
	var lines := text.split("\n")
	# The whole function body is needed before its reads can be judged (the arbiter call may sit above
	# or below them), so the file is split into functions first and each is scanned as a unit.
	var func_name := "<file scope>"
	var body := PackedStringArray()
	for raw_line in lines:
		var line := String(raw_line)
		if line.begins_with("func ") or line.begins_with("static func "):
			_scan_function(path, stem, func_name, body)
			func_name = _function_name(line)
			body = PackedStringArray()
		body.append(line)
	_scan_function(path, stem, func_name, body)


func _function_name(declaration: String) -> String:
	var open_paren := declaration.find("(")
	if open_paren < 0:
		return declaration.strip_edges()
	var head := declaration.substr(0, open_paren)
	return head.replace("static func ", "").replace("func ", "").strip_edges()


func _scan_function(path: String, stem: String, func_name: String, body: PackedStringArray) -> void:
	if body.is_empty():
		return
	var site := "%s.%s" % [stem, func_name]
	var consults_arbiter := false
	for line in body:
		if line.contains("KeyboardArbiter.allows("):
			consults_arbiter = true
			break
	for line in body:
		var code := String(line).strip_edges()
		if code.begins_with("#"):
			continue
		_scan_line(path, stem, site, code, consults_arbiter)


func _scan_line(path: String, stem: String, site: String, code: String, consults_arbiter: bool) -> void:
	_scan_input_singleton(path, site, code, consults_arbiter)
	_scan_event_action(path, site, code)
	_scan_keycode(path, stem, site, code)


## Reads of the `Input` SINGLETON. The sweep is of the singleton, not of the four spellings that come
## to mind: the second polled site survived the first fix because the sweep that scoped it searched
## `is_action_pressed` / `get_action_strength` / `is_key_pressed` and omitted
## `is_action_just_pressed`, which is how every HUD hotkey is read.
func _scan_input_singleton(path: String, site: String, code: String, consults_arbiter: bool) -> void:
	var from := 0
	while true:
		var at := code.find("Input.", from)
		if at < 0:
			return
		from = at + 1
		var member := _identifier_at(code, at + "Input.".length())
		if member == "":
			continue
		if NEUTRAL_INPUT_MEMBERS.has(member):
			continue
		if RAW_KEY_READS.has(member):
			_fail("%s (%s): `Input.%s` bypasses the InputMap, so no registry row can govern it — "
				% [site, path, member]
				+ "declare the key in KeyboardArbiter.REGISTRY and read it as an action")
			continue
		if POLLED_ACTION_READS.has(member):
			_check_polled_read(path, site, code, at, member, consults_arbiter)
			continue
		if _exception_for(site, member) != "":
			_checks += 1
			continue
		_fail("%s (%s): `Input.%s` is an UNCLASSIFIED keyboard read — add it to one of "
			% [site, path, member]
			+ "hotkey_guard's three `Input` member lists, with the reason it belongs there")


func _check_polled_read(path: String, site: String, code: String, at: int, member: String,
		consults_arbiter: bool) -> void:
	_checks += 1
	var args := _call_arguments(code, at + "Input.".length() + member.length())
	if args.is_empty():
		_fail("%s (%s): could not read the arguments of `Input.%s` — the scan cannot judge it"
			% [site, path, member])
		return
	var action := _string_literal(args[0])
	if action == "":
		_fail("%s (%s): `Input.%s` is called with %s, not a string literal — the registry cannot be "
			% [site, path, member, args[0]]
			+ "cross-checked against a name the scan cannot see")
		return
	if not KeyboardArbiter.polled_action_ids().has(action):
		_fail("%s (%s): `Input.%s(\"%s\")` reads an action that is NOT in KeyboardArbiter.REGISTRY"
			% [site, path, member, action])
		return
	var entry: Dictionary = KeyboardArbiter.entry_for(action)
	if entry.get("site", "") != site:
		_fail("%s (%s): reads `%s`, whose registry row names the site `%s`"
			% [site, path, action, entry.get("site", "?")])
	if args.size() < 2 or args[1] != "true":
		_fail("%s (%s): `Input.%s(\"%s\")` does not pass `exact_match = true`, so the bare-key "
			% [site, path, member, action]
			+ "binding also fires for every modified combination of it (Ctrl+R, Cmd+W)")
	if not consults_arbiter:
		_fail("%s (%s): polls `%s` but never calls `KeyboardArbiter.allows()` — the read is "
			% [site, path, action] + "unarbitrated")


## `event.is_action_pressed(...)` and friends — the EVENT-driven half. Matched by looking for the
## method on something that is not the `Input` singleton.
func _scan_event_action(path: String, site: String, code: String) -> void:
	for member in POLLED_ACTION_READS:
		var needle: String = "." + str(member) + "("
		var from := 0
		while true:
			var at := code.find(needle, from)
			if at < 0:
				break
			from = at + 1
			if code.substr(0, at).ends_with("Input"):
				continue  # the singleton, handled above
			_checks += 1
			var args := _call_arguments(code, at + needle.length() - 1)
			var action := "" if args.is_empty() else _string_literal(args[0])
			var entry: Dictionary = KeyboardArbiter.entry_for(action) if action != "" else {}
			if entry.is_empty():
				_fail("%s (%s): `.%s(%s)` matches an action that is NOT in KeyboardArbiter.REGISTRY"
					% [site, path, member, action if action != "" else "?"])
			elif entry.get("site", "") != site:
				_fail("%s (%s): matches `%s`, whose registry row names the site `%s`"
					% [site, path, action, entry.get("site", "?")])


## Raw keycode comparisons. Allowed in exactly one file (the matcher) and at one exempted site.
func _scan_keycode(path: String, stem: String, site: String, code: String) -> void:
	if not code.contains(KEYCODE_TOKEN):
		return
	if path.get_file() == KEYCODE_MATCHER_FILE:
		return
	_checks += 1
	if _exception_for(site, KEYCODE_TOKEN) != "":
		return
	_fail("%s (%s): compares a raw `%s`, which is true for the MODIFIED combination too "
		% [site, path, KEYCODE_TOKEN]
		+ "(`event.keycode == KEY_C` is satisfied by Ctrl+C) — use "
		+ "`KeyboardArbiter.is_bare_key()` / `is_escape_key()`")


func _exception_for(site: String, pattern: String) -> String:
	for exception in EXCEPTIONS:
		if exception["site"] == site and exception["pattern"] == pattern:
			return exception["reason"]
	return ""


## The identifier starting at `from` — `[a-z_0-9]`, which is every `Input` member name.
func _identifier_at(code: String, from: int) -> String:
	var end := from
	while end < code.length():
		var ch := code[end]
		if ch == "_" or (ch >= "a" and ch <= "z") or (ch >= "A" and ch <= "Z") \
				or (ch >= "0" and ch <= "9"):
			end += 1
		else:
			break
	return code.substr(from, end - from)


## The arguments of the call whose `(` is at or after `from`, split on top-level commas. Deliberately
## simple — it handles one line and no nested parentheses in the first two arguments, which is every
## call shape the registry permits; a call it cannot parse FAILS rather than passing silently.
func _call_arguments(code: String, from: int) -> Array[String]:
	var args: Array[String] = []
	var open_paren := code.find("(", from)
	if open_paren < 0:
		return args
	var depth := 0
	var current := ""
	var in_string := false
	var i := open_paren
	while i < code.length():
		var ch := code[i]
		if ch == "\"":
			in_string = not in_string
			current += ch
		elif in_string:
			current += ch
		elif ch == "(":
			depth += 1
			if depth > 1:
				current += ch
		elif ch == ")":
			depth -= 1
			if depth == 0:
				args.append(current.strip_edges())
				return args
			current += ch
		elif ch == "," and depth == 1:
			args.append(current.strip_edges())
			current = ""
		else:
			current += ch
		i += 1
	return args


## The contents of a double-quoted literal, or `""` when the argument is anything else (a constant, a
## variable) — which the caller reports, because a name the scan cannot see is a name the registry
## cannot be checked against.
func _string_literal(argument: String) -> String:
	var text := argument.strip_edges()
	if text.length() < 2 or not text.begins_with("\"") or not text.ends_with("\""):
		return ""
	return text.substr(1, text.length() - 2)


## **`project.godot` MUST DECLARE NO ACTIONS** — the registry is the roster, and a key declared in the
## project file is a key nothing here governs.
##
## This is not hypothetical tidiness. The `[input]` section that used to sit in that file held all six
## pan/zoom actions and `toggle_inspector` as hand-written Dictionaries, and **every one of them
## loaded with ZERO events**: `InputMap` deserialises an event entry only from an `Object(...)` the
## editor writes, so `has_action` answered true and `action_get_events` answered empty for the entire
## life of the file. The keys worked only because `MapView` and `Main` re-registered them at runtime.
## A dead second copy of the roster that reads as authoritative is exactly what this guard exists to
## refuse.
const PROJECT_FILE := "res://project.godot"
const PROJECT_INPUT_SECTION := "[input]"


func _assert_project_declares_no_input_map() -> void:
	var file := FileAccess.open(PROJECT_FILE, FileAccess.READ)
	if file == null:
		_fail("project: cannot read %s" % PROJECT_FILE)
		return
	var text := file.get_as_text()
	file.close()
	_expect(not text.contains(PROJECT_INPUT_SECTION),
		"project: %s declares an `%s` section — a key bound there is outside the registry, and a "
		% [PROJECT_FILE, PROJECT_INPUT_SECTION]
		+ "hand-written entry there silently loads with NO events at all")


# =====================================================================================================
# PART B — the policy, enumerated
# =====================================================================================================

## Every registry row against every owner. This is the assertion that did not exist: one place where
## "a gameplay key is suppressed unless the keyboard is free" is a checked fact.
func _assert_policy() -> void:
	if KeyboardArbiter.REGISTRY.is_empty():
		_fail("policy: the registry is EMPTY, so the enumeration proved nothing")
		return
	# The owner function itself, all four inputs. Text entry must outrank the menu — the field that
	# started this lives INSIDE the pause menu.
	_expect_owner(true, false, KeyboardArbiter.OWNER_TEXT_ENTRY)
	_expect_owner(true, true, KeyboardArbiter.OWNER_TEXT_ENTRY)
	_expect_owner(false, true, KeyboardArbiter.OWNER_MODAL_MENU)
	_expect_owner(false, false, KeyboardArbiter.OWNER_GAMEPLAY)

	var classes_seen := {}
	for entry in KeyboardArbiter.REGISTRY:
		var key_class: String = entry["class"]
		classes_seen[key_class] = true
		for owner in KeyboardArbiter.OWNERS:
			_checks += 1
			var may_act: bool = KeyboardArbiter.allows(owner, key_class)
			var expected: bool = owner == KeyboardArbiter.OWNER_GAMEPLAY \
				or key_class == KeyboardArbiter.CLASS_ESCAPE
			if may_act != expected:
				_fail("policy: `%s` (%s) %s under owner `%s`"
					% [entry["id"], key_class, "acts" if may_act else "is suppressed", owner])
	# Coverage in the other direction: a class nothing declares is a class nothing tests.
	for key_class in [KeyboardArbiter.CLASS_MAP_MOTION, KeyboardArbiter.CLASS_MAP_VIEW,
			KeyboardArbiter.CLASS_PANEL_TOGGLE, KeyboardArbiter.CLASS_ESCAPE]:
		if not classes_seen.has(key_class):
			_fail("policy: no registry row is in class `%s`, so its policy is untested" % key_class)

	# The three reports that produced this work, stated as themselves rather than derived.
	_expect_allows(KeyboardArbiter.OWNER_TEXT_ENTRY, KeyboardArbiter.CLASS_MAP_MOTION, false,
		"typing a save's name must not pan the map")
	_expect_allows(KeyboardArbiter.OWNER_TEXT_ENTRY, KeyboardArbiter.CLASS_PANEL_TOGGLE, false,
		"typing an `r` must not toggle the event dock")
	_expect_allows(KeyboardArbiter.OWNER_MODAL_MENU, KeyboardArbiter.CLASS_PANEL_TOGGLE, false,
		"`r` must not toggle the event dock BEHIND an open pause menu")
	_expect_allows(KeyboardArbiter.OWNER_TEXT_ENTRY, KeyboardArbiter.CLASS_MAP_VIEW, false,
		"`c` typed into a field must not re-centre the map")
	_expect_allows(KeyboardArbiter.OWNER_TEXT_ENTRY, KeyboardArbiter.CLASS_ESCAPE, true,
		"ESCAPE is how the player gets out and must reach `escape_claimant` under every owner")
	_expect_allows(KeyboardArbiter.OWNER_MODAL_MENU, KeyboardArbiter.CLASS_ESCAPE, true,
		"ESCAPE must close the pause menu it is standing in")


func _expect_owner(text_entry: bool, modal_menu: bool, want: String) -> void:
	_checks += 1
	var got: String = KeyboardArbiter.keyboard_owner(text_entry, modal_menu)
	if got != want:
		_fail("policy: keyboard_owner(text_entry=%s, modal_menu=%s) answered `%s`, wanted `%s`"
			% [text_entry, modal_menu, got, want])


func _expect_allows(owner: String, key_class: String, want: bool, why: String) -> void:
	_checks += 1
	if KeyboardArbiter.allows(owner, key_class) != want:
		_fail("policy: %s — but `%s` under owner `%s` answered %s"
			% [why, key_class, owner, not want])


# =====================================================================================================
# PART C — the modifier axis, through the real matchers
# =====================================================================================================

## Every registry row, bare and under each of the four modifiers. Two properties per row: the bare
## press MATCHES (without which every suppression leg below would pass vacuously) and every modified
## press does NOT.
func _assert_modifier_axis() -> void:
	for entry in KeyboardArbiter.REGISTRY:
		var keycode: Key = entry["keycode"]
		var kind: String = entry["kind"]
		var id: String = entry["id"]
		var bare := _key_event(keycode, "")
		match kind:
			KeyboardArbiter.KIND_POLLED_ACTION:
				_expect(InputMap.event_is_action(bare, id, true),
					"modifiers: a bare press of `%s` does not match its own action EXACTLY — the "
					% id + "binding and the registry disagree")
				for modifier in MODIFIER_PROPERTIES:
					var modified := _key_event(keycode, modifier)
					_expect(not InputMap.event_is_action(modified, id, true),
						"modifiers: `%s` with %s still matches `%s` exactly" % [id, modifier, id])
					# LIVENESS: the non-exact match must still succeed, or the row above is passing
					# because the event is wrong rather than because exact matching works.
					_expect(InputMap.event_is_action(modified, id, false),
						"modifiers: `%s` with %s does not match NON-exactly either, so the exact-"
						% [id, modifier] + "match assertion above proved nothing")
			KeyboardArbiter.KIND_KEYCODE:
				if entry["class"] == KeyboardArbiter.CLASS_ESCAPE:
					# The ONE deliberately loose match: a stray modifier must not strand the player.
					_expect(KeyboardArbiter.is_escape_key(bare),
						"modifiers: bare Escape is not recognised by `is_escape_key`")
					for modifier in MODIFIER_PROPERTIES:
						_expect(KeyboardArbiter.is_escape_key(_key_event(keycode, modifier)),
							"modifiers: Escape with %s is refused — the player would be stranded"
							% modifier)
				else:
					_expect(KeyboardArbiter.is_bare_key(bare, keycode),
						"modifiers: `%s` does not match its own bare key" % id)
					for modifier in MODIFIER_PROPERTIES:
						_expect(not KeyboardArbiter.is_bare_key(_key_event(keycode, modifier), keycode),
							"modifiers: `%s` still fires with %s held" % [id, modifier])
			KeyboardArbiter.KIND_EVENT_ACTION:
				_expect(bare.is_action_pressed(id),
					"modifiers: a bare press of `%s` does not match its own action" % id)
			_:
				_fail("modifiers: `%s` has kind `%s`, which this guard does not know how to test"
					% [id, kind])

	# **THE COMBINATION THE PLAYER REPORTED**, by name. `Ctrl+C` centred the map because
	# `event.keycode == KEY_C` is true for it.
	var ctrl_c := _key_event(REPORTED_CTRL_C["keycode"], REPORTED_CTRL_C["property"])
	_expect(not KeyboardArbiter.is_bare_key(ctrl_c, KEY_C),
		"%s is still accepted as a bare `C` — it would re-centre the map"
		% REPORTED_CTRL_C["label"])
	_expect(KeyboardArbiter.is_bare_key(_key_event(KEY_C, ""), KEY_C),
		"a bare `C` is refused, so the %s assertion proved nothing" % REPORTED_CTRL_C["label"])
	for reported in [REPORTED_CTRL_R, REPORTED_CMD_W]:
		var event := _key_event(KeyboardArbiter.entry_for(reported["action"])["keycode"],
			reported["property"])
		_expect(not InputMap.event_is_action(event, reported["action"], true),
			"%s still fires `%s`" % [reported["label"], reported["action"]])


## Echo repeats are dropped by both matchers — a held `C` used to re-fit the map every frame.
func _key_event(keycode: Key, modifier_property: String, echo: bool = false) -> InputEventKey:
	var event := InputEventKey.new()
	event.keycode = keycode
	event.physical_keycode = keycode
	event.pressed = true
	event.echo = echo
	if modifier_property != "":
		event.set(modifier_property, true)
	return event


# =====================================================================================================
# PART D — the same axis, through the live `Input` singleton
# =====================================================================================================

## `InputMap.event_is_action` is what `Input` consults, but part C only asserts about it. This part
## drives `Input` itself — synthesising the press, then asking the EXACT expression `Main._process`
## asks — so the claim "`exact_match = true` is what stops Ctrl+R" is measured.
##
## `is_action_just_pressed` is true only in the frame the press landed, so each probe presses and
## reads without yielding, then releases so the next probe starts clean.
func _assert_live_poll() -> void:
	var action: String = REPORTED_CTRL_R["action"]
	var keycode: Key = KeyboardArbiter.entry_for(action)["keycode"]

	_send(_key_event(keycode, ""))
	var bare_fires := Input.is_action_just_pressed(action, true)
	_release(keycode, "")
	if not bare_fires:
		# Not a silent skip: a `Input` that does not answer here would make every suppression leg
		# below pass for the wrong reason, so it is reported as the failure it is.
		_fail("live poll: a synthesised bare `%s` did not fire `%s` even non-exactly — the polled "
			% [OS.get_keycode_string(keycode), action]
			+ "probes below cannot prove anything, so they are reported as failures too")

	_send(_key_event(keycode, REPORTED_CTRL_R["property"]))
	var modified_exact := Input.is_action_just_pressed(action, true)
	var modified_loose := Input.is_action_just_pressed(action, false)
	_release(keycode, REPORTED_CTRL_R["property"])
	_expect(not modified_exact, "live poll: %s still fires `%s` through `Input`"
		% [REPORTED_CTRL_R["label"], action])
	_expect(modified_loose, "live poll: %s does not fire `%s` even NON-exactly, so the exact-match "
		% [REPORTED_CTRL_R["label"], action]
		+ "assertion above is passing for the wrong reason")


func _release(keycode: Key, modifier_property: String) -> void:
	var event := _key_event(keycode, modifier_property)
	event.pressed = false
	_send(event)


## **THE FLUSH IS NOT OPTIONAL.** `Input.parse_input_event` does not reach the action state on its
## own: with `use_accumulated_input` on — which is Godot's default — it appends to a buffer the main
## loop drains once per iteration, so a probe that pressed a key and polled it in the same call read
## the state from BEFORE the press and every leg answered "nothing fired". Flushing here is what makes
## the polled probes measure `Input` rather than measure the buffer.
func _send(event: InputEvent) -> void:
	Input.parse_input_event(event)
	Input.flush_buffered_events()


# =====================================================================================================
# PART E — the site itself
# =====================================================================================================

## **THE SUPPRESSION, OBSERVED** — `MapView._process` driven with real `Input` state, reading
## `pan_offset` rather than the predicate it branches on. Parts B–D would all still pass if the site
## simply stopped calling the arbiter; this is the leg that would not.
##
## `Main._process` has no equivalent leg — see the guard's own limits in
## `.claude/rules/client/keyboard-arbiter.md`.
func _assert_map_view_site() -> void:
	var map_view: MapView = MAP_VIEW.new()
	map_view.grid_width = PAN_GRID_WIDTH
	map_view.grid_height = PAN_GRID_HEIGHT
	add_child(map_view)
	await get_tree().process_frame

	var pan_keycode: Key = KeyboardArbiter.entry_for("map_pan_right")["keycode"]

	# LIVENESS FIRST. If a bare press does not pan, every suppression leg below is vacuous — which is
	# how a clamped `pan_offset` would have made this whole part meaningless.
	_expect(_pans(map_view, pan_keycode, ""),
		"site: a bare pan key does not move `MapView.pan_offset`, so the suppression legs below "
		+ "would pass for the wrong reason (is the grid large enough to pan on?)")

	# The three suppressions, at the site.
	_expect(not _pans(map_view, pan_keycode, REPORTED_CMD_W["property"]),
		"site: a modified pan key still pans — `exact_match` is not reaching `_process`")

	map_view.set_modal_menu_open(true)
	_expect(not _pans(map_view, pan_keycode, ""),
		"site: the map pans while a MODAL MENU owns the keyboard")
	map_view.set_modal_menu_open(false)

	var field := LineEdit.new()
	add_child(field)
	field.grab_focus()
	await get_tree().process_frame
	if map_view.get_viewport() == null \
			or map_view.get_viewport().gui_get_focus_owner() != field:
		_fail("site: the LineEdit probe would not take focus, so the typing leg proved nothing")
	else:
		_expect(not _pans(map_view, pan_keycode, ""),
			"site: the map pans while a text field holds the keyboard")
	field.release_focus()
	field.queue_free()

	# …and it pans again once the keyboard is handed back. The mirror-image failure — focus left
	# stuck, the map dead for the rest of the session — is worse than the bug this exists for.
	await get_tree().process_frame
	_expect(_pans(map_view, pan_keycode, ""),
		"site: the map stayed dead after the field released the keyboard")
	map_view.queue_free()


## Hold `keycode` (optionally with one modifier), run ONE `_process` frame, and report whether the
## map moved. The key is released either way, so the next probe starts from a clean device state.
func _pans(map_view: MapView, keycode: Key, modifier_property: String) -> bool:
	map_view.pan_offset = Vector2.ZERO
	_send(_key_event(keycode, modifier_property))
	map_view._process(FRAME_DELTA)
	_release(keycode, modifier_property)
	return map_view.pan_offset != Vector2.ZERO


# =====================================================================================================

func _expect(condition: bool, message: String) -> void:
	_checks += 1
	if not condition:
		_fail(message)


func _fail(message: String) -> void:
	_failures.append(message)


func _finish() -> void:
	if _failures.is_empty():
		print("hotkey_guard: PASS — %d registry row(s), %d check(s): every gameplay key is declared, "
			% [KeyboardArbiter.REGISTRY.size(), _checks]
			+ "arbitrated and exact")
		get_tree().quit(0)
	else:
		printerr("hotkey_guard: FAIL — %d problem(s):" % _failures.size())
		for message in _failures:
			printerr("  - ", message)
		get_tree().quit(1)
