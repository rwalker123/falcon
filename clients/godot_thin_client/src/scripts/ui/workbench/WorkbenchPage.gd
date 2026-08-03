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
##   - drops all its state in `reset()`
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


## Drop all page state so the shell can re-seed from a clean slate.
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
func send_command(line: String) -> bool:
	var send := service(WorkbenchVocab.SERVICE_SEND_COMMAND)
	if not send.is_valid():
		return false
	var result: Variant = send.call(line)
	return not (result is bool) or bool(result)


## Append one line to the surface's status log if a sink is attached.
func log_line(text: String) -> void:
	var sink := service(WorkbenchVocab.SERVICE_APPEND_LOG)
	if sink.is_valid():
		sink.call(text)
