extends VBoxContainer
class_name WorkbenchPage

## Base class + contract for one Workbench page.
##
## The shell hosts pages and knows NOTHING about them beyond these methods — not their snapshot
## keys, not their controls, not their commands. That asymmetry is what keeps `WorkbenchShell.gd` a
## coordinator: there is no page whose arrival requires editing it.
##
## A page:
##   - is one script under `ui/workbench/pages/`, registered by one row in `WorkbenchPages.PAGES`
##   - builds its own body in `build()` (called once, after the shell parents it)
##   - reads only the snapshot keys it owns in `apply_update()`
##   - drops the state the WORLD gave it in `reset()`
##   - reaches the outside world only through the SERVICES it is HANDED (`service()`), never by
##     reaching back into the shell
##
## **A page that grows gets its own sub-controllers under `ui/workbench/pages/<id>/` — it does not
## get more methods.** Same rule the HUD's controllers live under, for the same reason.

## Set by the shell from the registry row before `build()`, so a page never repeats its own identity.
var page_id: StringName = &""

## The host's lent capabilities, name → `Callable` (`WorkbenchVocab.SERVICE_*`). EMPTY until the
## shell hands them over, and empty forever in a preview harness — so every read goes through
## `service()` and every caller checks `is_valid()`. **A page built with no server attached must
## render, not crash.**
var _services: Dictionary = {}


## Build the page body. Called ONCE, after the shell has parented the page. Overridden by every page.
func build() -> void:
	pass


## Build the page's PINNED action bar, or return null for a page that has no actions. Called ONCE,
## immediately after `build()`, and hosted by the shell BELOW its scroll — so what it returns never
## scrolls away.
##
## It is a separate method rather than a region of `build()` because the two live in different
## parents: a page's body is scrolled content and its actions are chrome. A page keeps its own
## references to whatever it puts in here — the shell parents the control and learns nothing else
## about it.
func build_actions() -> Control:
	return null


## Ingest one snapshot or delta frame. `full_snapshot` distinguishes a complete world from a merged
## delta. Pages with no snapshot inputs (the tuning page is one) leave this alone.
func apply_update(_data: Dictionary, _full_snapshot: bool) -> void:
	pass


## WORLD BOUNDARY: drop the state this page derived from the world that just ended, so the shell can
## re-seed it from the next one. Called by `WorkbenchShell.reset_pages()` at every world change, on
## every built page — hidden ones included, since a page holding a dead world's numbers is just as
## wrong when the rail switches back to it.
##
## **It means "drop what the WORLD gave you", not "return to your initial appearance".** A page whose
## state is the designer's own intent rather than the world's data (the tuning page's staged
## overrides are the standing example) correctly does nothing here: the server keeps holding those
## overrides across the restart, so forgetting them would make the surface disagree with the sim.
func reset() -> void:
	pass


## Hand the page the host's services. Called by the shell before `build()` and again whenever the
## host re-supplies them.
func set_services(services: Dictionary) -> void:
	_services = services


## The Callable filed under `name`, or an INVALID Callable when the host lends no such service. It
## always answers a Callable so a caller has one thing to check (`is_valid()`) rather than two.
func service(name: StringName) -> Callable:
	var value: Variant = _services.get(name, null)
	return value if value is Callable else Callable()


## Enable/disable this page's command controls as the command socket comes and goes.
func set_command_connected(_connected: bool) -> void:
	pass


## Send one command line through the host's transport. Answers whether it went — false covers both
## "no transport" and "the socket refused it", because a page has the same job in either case.
##
## **A service that answers anything but `true` did NOT send.** This used to read a non-bool return
## (a void Callable's `null`) as success, which fails in the one direction the design cannot afford:
## the tuning page asks for a new game only once every override reports sent, so a false "sent"
## restarts the world on overrides that never left the client. `Main._workbench_send_command` was
## already strict; this end now matches it.
func send_command(line: String) -> bool:
	var send := service(WorkbenchVocab.SERVICE_SEND_COMMAND)
	if not send.is_valid():
		return false
	var result: Variant = send.call(line)
	return result is bool and bool(result)


## Append one line to the surface's status log if a sink is attached.
func log_line(text: String) -> void:
	var sink := service(WorkbenchVocab.SERVICE_APPEND_LOG)
	if sink.is_valid():
		sink.call(text)


# ---- number-field focus ------------------------------------------------------

## The `SpinBox` in this page that holds keyboard focus (directly or through its `LineEdit`), or null.
func focused_number_field() -> SpinBox:
	var holder: Control = get_viewport().gui_get_focus_owner() if is_inside_tree() else null
	if holder == null or not is_ancestor_of(holder):
		return null
	if holder is SpinBox:
		return holder
	if holder.get_parent() is SpinBox:
		return holder.get_parent()
	return null


## Commit the focused number field's typed text, then give up its focus. `apply()` comes first so a
## release can never drop the player's edit (focus loss would commit it too, but Apply reads the
## value in the same call and must not depend on that ordering).
func release_field_focus() -> void:
	var spin := focused_number_field()
	if spin == null:
		return
	spin.apply()
	spin.get_line_edit().release_focus()
	spin.release_focus()


## **A NUMBER FIELD MUST GIVE UP FOCUS WHEN THE PLAYER CLICKS AWAY.** The map is not a focus-taking
## Control, so a click there never moves GUI focus and the field's `LineEdit` would swallow every
## later key, game hotkeys included. `_input` (not `_unhandled_input`) because the map consumes
## clicks; the event is never marked handled, so the click still does its job in the game.
## Enter and Escape release too.
func _input(event: InputEvent) -> void:
	var spin := focused_number_field()
	if spin == null:
		return
	if event is InputEventMouseButton and event.pressed:
		if not spin.get_global_rect().has_point(event.global_position):
			release_field_focus()
	elif event is InputEventKey and event.pressed and not event.echo:
		if event.keycode in [KEY_ENTER, KEY_KP_ENTER, KEY_ESCAPE]:
			release_field_focus()
			# Escape leaving a field is the whole of that keypress: it must not also open the pause menu.
			if event.keycode == KEY_ESCAPE:
				get_viewport().set_input_as_handled()
