extends Node

## **THE ONE HARNESS THAT NEEDS A SERVER — the seated command link, end to end, against a live sim.**
##
## ## What it needs
##
## A **running server** on a port block this process can see, and nothing else. It stands the REAL
## `Main.tscn` up as a child and lets `Main` do its own boot: resolve the endpoints, claim the seat,
## greet the snapshot stream with the token, and send the dev-default `new_game`. So the server may be
## freshly booted and idle — the probe asks for its own world.
##
##     # server only, on this checkout's own port block (never the default 41000-41003)
##     scripts/run_stack.sh --server-only --port-base 41040
##     # then, in another shell, with the SAME block:
##     godot --headless --path clients/godot_thin_client --import
##     env STREAM_ENABLED=true STREAM_HOST=127.0.0.1 STREAM_PORT=41042 \
##         COMMAND_HOST=127.0.0.1 COMMAND_PORT=41041 COMMAND_PROTO_PORT=41041 \
##         scripts/preview.sh res://tools/live_seat_probe.tscn
##     echo $?   # 0 = pass. THE STATUS IS THE VERDICT (`.claude/rules/client/test-harnesses.md`)
##
## ⛔ **Through `scripts/preview.sh`, and never with `--headless`.** It stands up the whole client
## scene — `MapView` and its terrain shaders included — so it needs a real renderer, and the wrapper is
## what stops the window stealing the keyboard from another session. The env vars carry the port block;
## the client's own precedence is env → ports file → default, so passing them is what keeps a probe run
## off the default block.
##
## ## What it proves, and why a rendered fixture cannot
##
## Three things, in the order a session does them, each an assertion with the exit status behind it:
##
##   1. **The seat handshake.** The claim is granted, it carries a token, and per-seat frames addressed
##      by that token arrive — which is what `_world_revealed` means.
##   2. **One faction-bearing command, through the UI.** The band's hex is clicked
##      (`MapView.handle_hex_click`), Split is pressed in its card through the viewport, the sheet is
##      composed to the founding floor and confirmed, and the world comes back holding one more band.
##      The server takes the acting faction from the seat the *connection* holds, so an unseated link
##      answers this with `command.rejected=not_this_connections_seat` and no new band.
##   3. **One turn submission.** `order <faction> ready` through `Main._on_hud_next_turn`, and the turn
##      number advances.
##
## **This path broke twice on one branch** — a per-command connection that killed the seat, and then a
## missing stream greeting — and each time the only thing that caught it was a real client against a
## real server. `ui_preview` cannot: it feeds canned fixtures to the HUD and opens no socket. The
## command is driven through the HUD rather than `Main`'s handler because a HUD-side defect — every
## band verb closing as it opened — passed the handler path green.
##
## **It is deliberately three assertions wide.** The seat, one command that names a faction, one turn.
## Anything else about gameplay belongs in `core_sim`'s tests, where it costs no window.

## The real client, unmodified. Instanced as a child rather than made the scene root so this node
## keeps its own `_ready` (and its watchdog sibling) while `Main` boots exactly as it does in play.
const MAIN_SCENE := preload("res://src/Main.tscn")

## Prefix on every line, so a probe line is greppable and cannot be mistaken for engine output.
const TAG := "live_seat_probe"
## The failure token, spelled as every other harness spells it (`<name>: FAIL — <text>`).
const FAIL_FORMAT := "%s: FAIL — %s"

const EXIT_OK := 0
const EXIT_FAILED := 1

## **HOW LONG THE WORLD MAY TAKE TO ARRIVE.** It covers a cold server: the connect, the claim, the
## `new_game` this probe's boot sends, a full worldgen and the first full frame. Sized off the client's
## own `Main.NEW_GAME_ANSWER_TIMEOUT` (30 s, itself ~7x the measured worst-case worldgen) plus room for
## one of its re-sends, because a re-sent `new_game` is a legitimate way for this to succeed.
const REVEAL_TIMEOUT_MSEC := 75_000

## **HOW LONG A COMMAND'S EFFECT MAY TAKE TO COME BACK.** The server re-captures and broadcasts
## immediately after applying a command, so this is a round trip and not a turn: generous, but nowhere
## near the reveal budget, because a command that has not landed in this long has not landed.
const COMMAND_EFFECT_TIMEOUT_MSEC := 15_000

## **HOW LONG A SUBMITTED TURN MAY TAKE TO RESOLVE.** One turn of the whole sim plus the frame that
## reports it. Vacant seats never hold a turn, so with one seated player this is the resolve alone.
const TURN_TIMEOUT_MSEC := 30_000

## Polled, not awaited on a signal: `Main` exposes no "world revealed" signal, and a poll over
## `process_frame` is what the rest of the tools do (`turn_orb_click_probe`).
const POLL_FRAMES := 1

## The snapshot section every band rides in, and the fields this probe reads off a band row. Spelled
## from `native/src/dict/population.rs`, which is the contract — a rename there must break here.
const POPULATIONS_KEY := "populations"
const BAND_ID_KEY := "band_id"
const FACTION_KEY := "faction"
const IS_EXPEDITION_KEY := "is_expedition"
const WORKING_AGE_KEY := "working_age"
const SPLIT_MIN_WORKERS_KEY := "founding_min_workers"
const SPLIT_PARENT_MIN_WORKERS_KEY := "founding_parent_min_workers"
const TURN_KEY := "turn"
const NO_TURN := -1
const ENTITY_KEY := "entity"
const CURRENT_X_KEY := "current_x"
const CURRENT_Y_KEY := "current_y"
const NO_ENTITY := -1
const NO_TILE := -1
## How long a UI press gets to settle into the panel before its effect is read — a render, not a
## round trip, so a handful of frames.
const UI_SETTLE_FRAMES := 5
## The most `+` presses the split sheet's stepper may take to reach the sim's founding floor, so a
## stepper that never moves fails rather than spinning.
const SPLIT_STEPPER_MAX_PRESSES := 32

var _main: Node = null
var _failures: int = 0
var _watchdog: Node = null


func _ready() -> void:
	_watchdog = get_node_or_null("Watchdog")
	print("%s: booting the real Main.tscn against a live server." % TAG)
	_main = MAIN_SCENE.instantiate()
	add_child(_main)
	await _run()
	_finish()


## **THE ONE FAILURE SINK**, holding this file's only `push_error` — the same contract the render
## harnesses keep, so the tally can never drift from what was printed.
func _fail(message: String) -> void:
	_failures += 1
	push_error(FAIL_FORMAT % [TAG, message])


## **THE ONE EXIT**, so the status is derived in exactly one place. Disarms the hang guard on the way
## out: a slow shutdown is not a stall.
func _finish() -> void:
	if _watchdog != null and _watchdog.has_method("disarm"):
		_watchdog.call("disarm")
	if _failures > 0:
		print("%s: %d failure(s)." % [TAG, _failures])
		get_tree().quit(EXIT_FAILED)
		return
	print("%s: PASS — seat granted and streaming, split_band obeyed, turn submitted and resolved." % TAG)
	get_tree().quit(EXIT_OK)


func _note_progress() -> void:
	if _watchdog != null and _watchdog.has_method("note_progress"):
		_watchdog.call("note_progress")


## The last frame the loader published, i.e. what the client currently believes the world is.
func _snapshot() -> Dictionary:
	var loader: Object = _main.get("snapshot_loader")
	if loader == null:
		return {}
	var last: Variant = loader.get("last_stream_snapshot")
	return last if last is Dictionary else {}


## The player's own RESIDENT bands — parties are excluded exactly as `Hud.update_band_alerts` excludes
## them, because a split makes a band and a detached party would flatter the count.
func _player_bands(snapshot: Dictionary) -> Array:
	var out: Array = []
	var rows: Variant = snapshot.get(POPULATIONS_KEY, [])
	if not (rows is Array):
		return out
	for row in rows:
		if not (row is Dictionary):
			continue
		var band: Dictionary = row
		if int(band.get(FACTION_KEY, HudConst.NO_FACTION_ID)) != HudConst.PLAYER_FACTION_ID:
			continue
		if bool(band.get(IS_EXPEDITION_KEY, false)):
			continue
		out.append(band)
	return out


## Poll `predicate` until it answers true or `timeout_msec` elapses. Wall clock, so a slow frame rate
## cannot shorten a budget the way a frame count would.
func _await_until(what: String, timeout_msec: int, predicate: Callable) -> bool:
	var deadline := Time.get_ticks_msec() + timeout_msec
	_note_progress()
	while Time.get_ticks_msec() < deadline:
		if bool(predicate.call()):
			return true
		for _i in range(POLL_FRAMES):
			await get_tree().process_frame
	print("%s: timed out after %d s waiting for %s." % [TAG, timeout_msec / 1000, what])
	return false


func _run() -> void:
	# ---- 1. THE SEAT HANDSHAKE -----------------------------------------------------------------
	var revealed: bool = await _await_until("the world to arrive", REVEAL_TIMEOUT_MSEC,
		func() -> bool: return bool(_main.get("_world_revealed")))
	var seat: Object = _main.get("seat_claim")
	var seated := seat != null and bool(seat.get("seated_now"))
	var token := int(seat.get("seat_token")) if seat != null else SnapshotStream.NO_SEAT_TOKEN
	# **THE TOKEN IS A SECRET AND A HARNESS LOG IS A LOG** — whether one was granted is the whole of
	# what this assertion needs, and printing the value here would reopen the hole every client-side
	# log line was just redacted to close (`SnapshotStream.SEAT_TOKEN_LOG_REDACTION`).
	print("%s: seated=%s token_granted=%s world_revealed=%s" % [
		TAG, seated, token != SnapshotStream.NO_SEAT_TOKEN, revealed])
	if not seated:
		var refusal := String(seat.get("refusal")) if seat != null else "no seat seam"
		_fail("the seat was never granted (%s) — every faction-bearing command would be refused" % refusal)
		return
	if token == SnapshotStream.NO_SEAT_TOKEN:
		_fail("the seat was granted with no token, so the snapshot stream has nothing to greet with")
		return
	if not revealed:
		_fail("no full snapshot for this seat ever arrived — the seat is held but frames are not reaching this stream socket")
		return

	var before := _snapshot()
	var bands := _player_bands(before)
	var turn_before := int(before.get(TURN_KEY, NO_TURN))
	print("%s: turn=%d player_bands=%d" % [TAG, turn_before, bands.size()])
	if bands.is_empty():
		_fail("the revealed world holds no band for faction %d, so there is nothing to command" % HudConst.PLAYER_FACTION_ID)
		return

	# ---- 2. ONE FACTION-BEARING COMMAND --------------------------------------------------------
	var band: Dictionary = bands[0]
	var band_id := int(band.get(BAND_ID_KEY, HudConst.NO_BAND_ID))
	# The ask is the sim's OWN floor, read off the cohort rather than copied from `expedition_config`
	# — the same numbers the compose sheet states — so a retuned floor moves this probe with it.
	var workers := int(band.get(SPLIT_MIN_WORKERS_KEY, 0))
	var parent_floor := int(band.get(SPLIT_PARENT_MIN_WORKERS_KEY, 0))
	var working_age := int(band.get(WORKING_AGE_KEY, 0))
	if workers <= 0 or working_age - workers < parent_floor:
		# Not a transport fault: the starting band cannot legally split, so this probe cannot prove
		# anything about the command path. Loud rather than skipped — a gate that quietly proves less
		# than it claims is worse than a failing one.
		_fail(("band %d cannot legally split (%d working-age, a new band needs %d and the parent must "
			+ "keep %d), so the faction-bearing command could not be tested — the starting loadout or "
			+ "the settle floors moved") % [band_id, working_age, workers, parent_floor])
		return
	print("%s: split_band band=%d workers=%d (of %d working-age, parent floor %d)" % [
		TAG, band_id, workers, working_age, parent_floor])
	var bands_before := bands.size()
	# **THROUGH THE UI THE PLAYER USES, NOT `Main`'s HANDLER.** The player selects the band by clicking
	# its hex and presses Split in that hex's card; a regression that closed the verb the instant the
	# press opened it (the jump back to the band's hex re-clicked the SELECTED hex and cycled the
	# selection onto the land) left the handler path green while every band verb did nothing.
	if not await _split_through_the_ui(band, workers):
		return
	var split_landed: bool = await _await_until("split_band to reach the world",
		COMMAND_EFFECT_TIMEOUT_MSEC,
		func() -> bool: return _player_bands(_snapshot()).size() > bands_before)
	if not split_landed:
		_fail(("split_band changed nothing: still %d band(s) after the command. The seat is held, so "
			+ "either the command never reached the seated link or the server refused it — check the "
			+ "server log for `command.rejected`") % bands_before)
		return
	print("%s: split_band obeyed — player_bands %d -> %d" % [
		TAG, bands_before, _player_bands(_snapshot()).size()])

	# ---- 3. ONE TURN SUBMISSION ----------------------------------------------------------------
	# `_on_hud_next_turn` sends `order <faction> ready`, which is the SEAT's verb; the host verb
	# (`turn N`) rides a throwaway connection and would prove nothing about the seat.
	print("%s: submitting the turn (order %d ready)" % [TAG, HudConst.PLAYER_FACTION_ID])
	_main.call("_on_hud_next_turn", 1)
	var advanced: bool = await _await_until("the turn to resolve", TURN_TIMEOUT_MSEC,
		func() -> bool: return int(_snapshot().get(TURN_KEY, NO_TURN)) > turn_before)
	if not advanced:
		_fail(("the turn never advanced past %d after `order %d ready`. With one seated player and "
			+ "vacant rivals the submission alone resolves it, so the submission did not arrive") % [
			turn_before, HudConst.PLAYER_FACTION_ID])
		return
	print("%s: turn %d -> %d" % [TAG, turn_before, int(_snapshot().get(TURN_KEY, NO_TURN))])


## **SELECT THE BAND BY ITS HEX, PRESS SPLIT IN ITS CARD, AND COMPOSE THE SHEET** — the player's path
## end to end. `false` (with a named failure) where any step does not happen.
##
## The hex click is `MapView.handle_hex_click`, the function a real left click on the map calls. The
## verb press goes through the viewport (`Viewport.push_input`), so the button is reached by hit test
## as a player's click would be. The sheet's stepper and confirm are pressed by signal: the opening
## outfit card floats over the sheet's right-hand column on a fresh game, and whether a pointer
## reaches them there is a layout question this probe does not own.
func _split_through_the_ui(band: Dictionary, workers: int) -> bool:
	var hud: Node = _main.get("hud")
	var map_view: Node = _main.get("map_view")
	var entity := int(band.get(ENTITY_KEY, NO_ENTITY))
	map_view.call("handle_hex_click", int(band.get(CURRENT_X_KEY, NO_TILE)),
		int(band.get(CURRENT_Y_KEY, NO_TILE)), MOUSE_BUTTON_LEFT)
	await _frames(UI_SETTLE_FRAMES)
	var verb := _verb_button(hud, HudComposeVocab.VERB_SPLIT)
	if verb == null:
		_fail("clicking band %d's hex drew no Split verb in its card" % entity)
		return false
	_press_through_viewport(verb)
	await _frames(UI_SETTLE_FRAMES)
	var panel: Object = hud.get("_bandpanel")
	if not bool(panel.call("verb_is_open")):
		_fail(("pressing Split in the band's card opened no sheet — the verb closed as it opened. "
			+ "The jump back to the band's hex must not move the selection off the band"))
		return false
	for _i in range(SPLIT_STEPPER_MAX_PRESSES):
		if _stepper_count(hud) >= workers:
			break
		var plus := _stepper_plus(hud)
		if plus == null:
			break
		plus.pressed.emit()
		await _frames(UI_SETTLE_FRAMES)
	var confirm := _split_confirm(hud)
	if confirm == null or confirm.disabled:
		_fail("the split sheet never offered an enabled `%s` at %d workers (stepper reads %d)" % [
			HudComposeVocab.SPLIT_BAND_BUTTON, workers, _stepper_count(hud)])
		return false
	confirm.pressed.emit()
	return true


func _frames(count: int) -> void:
	for _i in range(count):
		await get_tree().process_frame


## Every node under `root`, depth first — the probe finds controls by META, never by face.
func _descendants(root: Node, out: Array[Node]) -> Array[Node]:
	out.append(root)
	for child in root.get_children():
		_descendants(child, out)
	return out


func _verb_button(hud: Node, id: StringName) -> Button:
	for node in _descendants(hud, []):
		if node is Button and node.has_meta(HudWidgets.VERB_BUTTON_META) \
				and node.is_visible_in_tree() and node.get_meta(HudWidgets.VERB_BUTTON_META) == id:
			return node
	return null


func _stepper(hud: Node) -> HBoxContainer:
	for node in _descendants(hud, []):
		if node is HBoxContainer and node.has_meta(HudWidgets.PARTY_STEPPER_COUNT_META) \
				and node.is_visible_in_tree():
			return node
	return null


func _stepper_count(hud: Node) -> int:
	var row := _stepper(hud)
	return int(row.get_meta(HudWidgets.PARTY_STEPPER_COUNT_META)) if row != null else 0


## The stepper's `+` — its LAST button (`HudWidgets.add_stepper_controls` lays out `−`, value, `+`).
func _stepper_plus(hud: Node) -> Button:
	var row := _stepper(hud)
	if row == null:
		return null
	var plus: Button = null
	for child in row.get_children():
		if child is Button:
			plus = child
	return plus


func _split_confirm(hud: Node) -> Button:
	for node in _descendants(hud, []):
		if node is Button and node.is_visible_in_tree() \
				and (node as Button).text == HudComposeVocab.SPLIT_BAND_BUTTON:
			return node
	return null


## A left click at the control's centre, pushed into its viewport — press and release in one call
## stack, so nothing can land between them (`test-harnesses.md` → "A SIMULATED GESTURE IS NOT
## HERMETIC").
func _press_through_viewport(control: Control) -> void:
	var viewport: Viewport = control.get_viewport()
	var at: Vector2 = viewport.get_final_transform() * control.get_global_rect().get_center()
	var motion := InputEventMouseMotion.new()
	motion.position = at
	motion.global_position = at
	viewport.push_input(motion)
	for pressed in [true, false]:
		var click := InputEventMouseButton.new()
		click.button_index = MOUSE_BUTTON_LEFT
		click.pressed = pressed
		click.position = at
		click.global_position = at
		viewport.push_input(click)
