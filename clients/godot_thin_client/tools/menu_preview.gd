extends Node

## Dev-only preview harness for the shared MenuShell (landing + pause). Instances the real
## MenuShell scene, renders it in each mode, and dumps a PNG to `ui_preview_out/`. No server,
## no network — the actual render code against the real HudStyle. Run from the repo root:
##
##   godot --headless --path clients/godot_thin_client --import     # if scenes/scripts changed
##   scripts/preview.sh res://tools/menu_preview.tscn                       # NOT --headless
##
## then read ui_preview_out/menu_landing.png and menu_pause.png.

const MENU_SHELL := preload("res://src/ui/MenuShell.tscn")
## **`Main`'s WIRE BUILDER, called rather than restated.** `new_game_line` is a static function, so
## the rule that decides whether the player's count reaches the socket is reachable here without
## standing a client up — the same preload the other harnesses take for `Main`'s constants.
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")
## The SHIPPED predicate both polled-input sites ask. Asserted directly rather than restated here —
## a harness that re-spelled the expression would keep passing after the real one drifted.
const TextEntryFocus := preload("res://src/scripts/TextEntryFocus.gd")
const OUT_DIR := "res://ui_preview_out"

# Window the shell renders into.
const PREVIEW_SIZE := Vector2i(1500, 900)
# Ground behind the landing shell: `HudStyle.GROUND` itself, READ at its use site. It was a
# hand-copied literal of the console palette's value, so these frames kept rendering the retired
# palette's backdrop under every theme — the harness telling the same lie the shipped code was fixed
# for. A mid terrain tone stands in behind the pause scrim so the scrim + card chrome read against
# something non-black; that one is a stand-in for the WORLD, not a palette entry.
const MAP_TONE := Color(0.10, 0.15, 0.16)
# Nav id of the client-settings pane in MenuShell.ITEMS.
const OPTIONS_PANE_ID := "options"
# Nav ids of the two saves panes, driven through the same `_activate_item` the nav rail calls.
const LOAD_PANE_ID := "load"
const SAVE_PANE_ID := "save"
# …and of the setup pane, whose rival-peoples control is fed from the capacity seam below.
const NEW_GAME_PANE_ID := "new_game"
# The setup pane's primary action, matched on rather than retyped at each assertion: the frames below
# claim things about whether it is OFFERED, and a typo would silently assert nothing.
const BEGIN_BUTTON_LABEL := "Begin the trail"

# ---- faction-capacity fixtures --------------------------------------------------------------------
# The ceiling is a property of the GRID (`core_sim`'s `faction_start_capacity`: a hex-packing count
# over the map's LAND, at the shipped `faction_start_min_separation`), and the offer is derived from
# it. These are the numbers a real server answers for the shipped grids:
#
#     grid          Tiny  Small  Standard  Large  Huge
#     ceiling          3      4         6     11    17
#     pre-selects      2      3         4      6     9
#
# **They are fixtures for the ROW's states, not a restatement of the rule** — the client never
# computes this, which is the whole reason the query exists. They are written down anyway because
# this harness ANSWERS THE QUERY ITSELF: a drifted fixture stays perfectly green while rendering a
# state no player can reach, which is worth less than no frame at all. Both numbers have moved once
# already; re-derive them from `faction_start_capacity` rather than trusting this block.
## **EVERY ANSWER IS FOR A GRID, so the pairs are per grid.** Answering a Tiny map with Standard's
## ceiling is the same drift as a stale constant and one step harder to see, because the numbers are
## individually right.
const CAPACITY_MAX_SMALLEST := 3
const CAPACITY_DEFAULT_SMALLEST := 2
const CAPACITY_MAX_STANDARD := 6
const CAPACITY_DEFAULT_STANDARD := 4
# What the player drags the slider to, for the frame that shows a CHOSEN count. Well below the
# Standard offer of 4 and nowhere near the ceiling of 6, so the grabber sits visibly left of where the
# answer put it — a one-step nudge would read as a rounding artefact rather than as a decision. It is
# also the one value that exercises the readout's singular form ("1 rival", not "1 rivals").
const CAPACITY_PICKED := 1
# A genuine 0 ceiling — a grid with no room for a second start — which the row must render as "you
# will be alone" rather than as a failure. **No offered map size produces it**: the smallest, Tiny,
# still seats 3 rivals. So it is answered here rather than reached — a heavier separation or a preset
# with very little land is what would make it real, and the row has to be right when it does.
const CAPACITY_MAX_ALONE := 0
# The roomiest offered grid, for the re-ask frames: the pick made against it has to survive the
# switch to a smaller map, clamped rather than reset.
const CAPACITY_MAX_ROOMIEST := 17
const CAPACITY_DEFAULT_ROOMIEST := 9
# The two map sizes the frames switch between, named from the shared registry rather than typed as
# ids: switching size is what re-asks the ceiling, and `MapSizes` is the one list of them.
const SIZE_KEY_SMALLEST := "tiny"
const SIZE_KEY_ROOMIEST := "huge"
const SIZE_KEY_STANDARD := "standard"

# ---- save-channel fixtures ------------------------------------------------------------------------
# The `SaveSlots` seam is fed through its REAL `deliver` path with dicts shaped exactly as
# `bridge/query.rs` composes them, so these frames exercise the actual decode/route/format code with
# no server. `modified_unix_seconds` is expressed as an AGE for the three relative buckets — a fixed
# stamp there would drift into another bucket as the branch aged — and as a FIXED stamp for the
# fourth, which is the one that renders the absolute-date branch.
const AGE_RECENT_SECONDS := 12 * 60
const AGE_HOURS_SECONDS := 5 * 3600
const AGE_DAYS_SECONDS := 3 * 86400
const FIXED_STAMP_UNIX := 1768726920  # 2026-01-18 09:02 UTC, rendered in the machine's local zone
const FIXTURE_SIZE_AUTOSAVE := 1257874   # the measured 160x104 blob
const FIXTURE_SIZE_MIDWINTER := 1198336
const FIXTURE_SIZE_FIRST := 902144
const FIXTURE_SIZE_TINY := 41984         # small enough to render in KB, the other size branch
const FIXTURE_TITLE := "Trail Sovereigns"
const FIXTURE_SLOT_MIDWINTER := "midwinter camp"
const FIXTURE_SLOT_FIRST := "first winter"
const FIXTURE_SLOT_SCRATCH := "scratch_2"
# The name typed into the Save pane's field for the "new slot" frame, and the one that is already on
# disk for the OVERWRITE frame.
const TYPED_NEW_NAME := "before the thaw"
# --- the mid-string edit the caret assertion drives ------------------------------------------------
# A name already in the field, then a character inserted between its first and second letters and a
# second one straight after it. Written out rather than derived, so the expected text is a statement
# about what the player sees and not a restatement of the code under test.
const CARET_BASE_TEXT := "abcd"
const CARET_INSERT_COLUMN := 1
const CARET_FIRST_CHAR := "x"
const CARET_SECOND_CHAR := "y"
const CARET_AFTER_FIRST_TEXT := "axbcd"
const CARET_AFTER_FIRST_COLUMN := 2
const CARET_AFTER_SECOND_TEXT := "axybcd"
# What a player might reasonably type that the whitelist refuses — the reserved slot, which is the
# one refusal the pane exists to make unreachable rather than merely reported.
const TYPED_RESERVED_NAME := "autosave"

# The drift rows, one per (saved -> live) pair the notice words differently, so one frame covers the
# whole vocabulary.
const DRIFT_FIXTURE := [
	{"file_name": "fauna_config.json", "saved": "file", "live": "file"},
	{"file_name": "simulation_config.json", "saved": "builtin", "live": "file"},
	{"file_name": "recipes.json", "saved": "file", "live": "builtin"},
]

# A roster theme that is NOT the one this harness pins as applied, so the Theme row renders its
# CHANGED state — caption in WARN, "Apply now" button present. Any id but `HudPalette.DEFAULT_THEME`
# would do; the frames are named for the state, not for this palette.
const PENDING_THEME := "kiln"

## The run's exit status. **A clean run exits 0 and a run with any `FAIL` in it exits non-zero**, so
## the status and the output agree — a harness that printed an error and still exited 0 was
## indistinguishable from a green one to anything but a human reading stdout.
## The world parameters the wire assertion builds a line from. Any values would do — the claim is
## about the trailing count — so they are the dev default's, which is a line a developer will
## recognise if one is ever printed by a failure.
const WIRE_PRESET := "earthlike"
const WIRE_WIDTH := 80
const WIRE_HEIGHT := 52
const WIRE_SEED := 0
const WIRE_PROFILE := "late_forager_tribe"

const EXIT_OK := 0
const EXIT_FAILED := 1

var _root: Control
var _bg: ColorRect
var _shell: MenuShell
var _failures := 0
## The save channel, driven with no server: the harness IS the transport. `_last_request_id` is what
## the fake sender captured, and it is what the canned replies correlate against — so the seam's real
## in-flight bookkeeping is exercised rather than bypassed.
var _save_seam: SaveSlots
## The New Game pane's capacity seam, driven the same way and off the same fake sender.
var _capacity_seam: FactionCapacity
var _last_request_id := 0
var _drift_notice: ConfigDriftNotice


func _ready() -> void:
	get_window().size = PREVIEW_SIZE
	# PIN THE INTERFACE SCALE, the same determinism source `ui_preview` / `map_preview` /
	# `band_panel_preview` pin: `ClientSettings` is an autoload that has already read the developer's
	# real `user://client_settings.cfg` and `UiScaler` has already pushed it onto the window's
	# `content_scale_factor`. This harness sizes `_root` to a fixed PREVIEW_SIZE, so a moved slider
	# would leave that root larger than the logical viewport and push the Options pane out of frame —
	# in the ONE PNG that exists to show the Options pane. Reading the real config for the row VALUES
	# is deliberate here (see the docstring); rendering at the real config's SCALE is not.
	# Assign the MEMBER, never `set_ui_scale` (the setter `_save`s over that file), then re-emit
	# `changed` so `UiScaler` applies the pin through its own real path.
	ClientSettings.ui_scale = ClientSettings.UI_SCALE_DEFAULT
	ClientSettings.changed.emit()
	# PIN THE PALETTE, the theme half of the same contamination. `ClientSettings` read the developer's
	# real `user://client_settings.cfg` at boot and `HudPalette.apply()` has ALREADY installed whatever
	# theme it found, so a developer running Kiln would re-tint every frame in this set. Re-applying the
	# default here is safe at any point before UI is built: `HudStyle`/`MapView` and the vocabulary
	# modules are all re-derived by `apply`, and nothing on screen has read a colour yet.
	HudPalette.apply(HudPalette.DEFAULT_THEME)
	# …AND THE SAVED PICK, which is a SECOND setting from the same contaminated file. The Theme row
	# compares the saved pick against the applied palette, so pinning only the palette left the row
	# rendering whatever the developer last chose: on a machine saved to any non-default theme every
	# Options frame came out in the row's PENDING state — not-applied caption, Apply button — and the
	# settled state had no frame at all. Same MEMBER assignment, for the same reason: `set_theme`
	# would write the developer's config.
	ClientSettings.theme = HudPalette.DEFAULT_THEME
	DirAccess.make_dir_absolute(OUT_DIR)

	_root = Control.new()
	_root.position = Vector2.ZERO
	_root.size = Vector2(PREVIEW_SIZE)
	add_child(_root)

	_bg = ColorRect.new()
	_bg.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_bg.color = HudStyle.GROUND
	_root.add_child(_bg)

	_shell = MENU_SHELL.instantiate()
	_root.add_child(_shell)
	await get_tree().process_frame

	# Landing: full-bleed over the dark ground.
	_bg.color = HudStyle.GROUND
	_shell.mode = MenuShell.LANDING
	await _settle()
	await _save("menu_landing")

	# Pause: centered card over the scrim, mid-tone "map" behind so the scrim reads.
	_bg.color = MAP_TONE
	_shell.mode = MenuShell.PAUSE
	await _settle()
	await _save("menu_pause")

	# Options pane — the client-settings rows (Fog of war toggle + the two speed sliders). Driven
	# through the same `_activate_item` the nav rail calls, so the pane is built exactly as a click
	# builds it. Rendered from the PAUSE mode; the shared ITEMS registry gives the landing menu the
	# identical pane, so one frame covers both.
	_shell._activate_item(OPTIONS_PANE_ID)
	await _settle()
	# The saved pick and the applied palette agree here, so there is nothing to apply and the Theme row
	# must offer nothing to press. This is the settled half of the pair asserted below.
	if _shell._theme_apply != null and _shell._theme_apply.visible:
		_fail("theme row: the Apply now button is showing with no pending pick")
	await _save("menu_options")

	# …AND THE THEME DROPDOWN OPEN. Its own frame because a dropdown is TWO surfaces: the popup is a
	# `PopupMenu` on a separate embedded `Window`, which nothing set on the face reaches, so an
	# unstyled one renders Godot's stock light-grey menu over the console and no closed-face frame can
	# show it. It lands in the capture the way `band_panel_preview`'s confirm dialogs do.
	_shell._theme_picker.show_popup()
	await _settle()
	await _save("menu_options_theme_popup")
	_shell._theme_picker.get_popup().hide()

	# …AND THE ROW IN ITS CHANGED STATE — the only state in which the "Apply now" button exists, so
	# it is the only frame that can show it. The pick is made by assigning the MEMBER
	# `ClientSettings.theme` and rebuilding the pane, never by driving `_on_theme_selected`: that path
	# calls `set_theme`, which SAVES over the developer's real `user://client_settings.cfg` — the same
	# contamination the interface-scale pin above exists to avoid, and this one would overwrite the
	# developer's own saved theme. Rendered from PAUSE, where the button is `armed` and says the
	# run will be lost. NOTHING PRESSES IT: applying reloads the current scene, which would tear down
	# the tree this harness is capturing.
	ClientSettings.theme = PENDING_THEME
	_shell._activate_item(OPTIONS_PANE_ID)
	await _settle()
	_assert_apply_visible("pause")
	await _save("menu_options_theme_pending_pause")

	# The same row in LANDING mode, where no run exists: shorter label, `primary` variant, and the
	# caption without the run-loss clause. `set_mode` rebuilds the active pane, so the row re-derives
	# its wording for the new mode rather than keeping the pause one.
	_bg.color = HudStyle.GROUND
	_shell.mode = MenuShell.LANDING
	await _settle()
	_assert_apply_visible("landing")
	await _save("menu_options_theme_pending_landing")

	await _run_saves_states()
	await _run_new_game_states()

	_finish()


## **THE NEW GAME PANE'S RIVAL COUNT, over the same fake transport.** The seam is real and so is
## every state below; the only thing standing in for a server is `_send`, which records the request id
## and answers nothing until this harness says so. That is what makes the two states no healthy stack
## can reach — a capacity ask that never answers, and a grid with no room for a second people —
## renderable at all.
func _run_new_game_states() -> void:
	_bg.color = HudStyle.GROUND
	_shell.mode = MenuShell.LANDING
	_capacity_seam = FactionCapacity.new()
	_capacity_seam.set_sender(_send)
	_shell.set_faction_capacity(_capacity_seam)
	_shell._activate_item(NEW_GAME_PANE_ID)
	await _settle()

	# --- the ask in flight: no slider, and a caption saying so -----------------------------------
	if _capacity_seam.state != FactionCapacity.STATE_PENDING:
		_fail("rivals: opening the setup pane put no capacity ask in flight (%s)" % _capacity_seam.state)
	_assert_no_rival_slider("pending")
	await _save("menu_new_game_rivals_pending")

	# --- answered: the slider, opened on the server's default ------------------------------------
	_answer_capacity(CAPACITY_DEFAULT_STANDARD, CAPACITY_MAX_STANDARD)
	await _settle()
	if _shell._rival_count != CAPACITY_DEFAULT_STANDARD:
		_fail("rivals: an answered ask left the control on %d, not the server's default %d"
			% [_shell._rival_count, CAPACITY_DEFAULT_STANDARD])
	await _save("menu_new_game_rivals")

	# --- the player picks some. The count is what "Begin the trail" would carry. ------------------
	_drag_rival_slider(CAPACITY_PICKED)
	await _settle()
	if _shell._resolved_rival_count() != CAPACITY_PICKED:
		_fail("rivals: a picked %d resolved to %d on the wire"
			% [CAPACITY_PICKED, _shell._resolved_rival_count()])
	await _save("menu_new_game_rivals_picked")

	# --- A GRID WITH NO ROOM. Not a failure, and not rendered as one: no slider, and a caption that
	# says the player will be alone. The pick made above is clamped away by the new ceiling, and the
	# re-ask is driven by a real size click.
	_pick_map_size(SIZE_KEY_SMALLEST)
	if _capacity_seam.state != FactionCapacity.STATE_PENDING:
		_fail("rivals: changing the map size did not re-ask the ceiling (%s)" % _capacity_seam.state)
	_answer_capacity(0, CAPACITY_MAX_ALONE)
	await _settle()
	_assert_no_rival_slider("ceiling 0")
	if _shell._resolved_rival_count() != 0:
		_fail("rivals: a 0 ceiling left %d on the wire instead of an explicit none"
			% _shell._resolved_rival_count())
	await _save("menu_new_game_rivals_alone")

	# --- A SERVER ANSWERED AND STILL GAVE NO COUNT, and the screen starts a game anyway. The argument
	# is omitted entirely (`NO_COUNT`), which the server answers with its unattended roster — no
	# rivals — rather than with a number this screen guessed. The caption is what tells the player
	# that. **The token matters here**: it is a refusal a SERVER sent (`wrong_answerer`), which is a
	# different fact from nothing answering at all — that state is `_run_server_unreachable_states`
	# below, and collapsing the two is the defect this pair pins.
	_pick_map_size(SIZE_KEY_ROOMIEST)
	_fail_capacity(FactionCapacity.ERROR_WRONG_ANSWERER)
	await _settle()
	_assert_no_rival_slider("failed")
	if _shell._resolved_rival_count() != FactionCapacity.NO_COUNT:
		_fail("rivals: a failed ask put %d on the wire instead of omitting the argument"
			% _shell._resolved_rival_count())
	_assert_says_none("a server answered without a count")
	_assert_begin_is_offered("a server answered without a count")
	await _save("menu_new_game_rivals_unavailable")

	await _assert_the_shown_count_is_the_count_sent()
	await _run_rivals_reask_states()
	await _assert_row_height_is_stable()
	await _assert_an_answer_survives_leaving_the_pane()
	_assert_capacity_ids_are_disjoint_from_the_save_seam()
	await _run_server_unreachable_states()
	await _run_landing_notice_state()


## **NOTHING IS LISTENING — the state the New Game screen used to blame on the rival count.** With no
## server the ask cannot be answered at all, so no world can be built: the caption names the server and
## "Begin the trail" is DISABLED. Three claims a frame cannot make, in the order the states arrive:
##
##   * a merely PENDING ask disables nothing (the normal case for a moment at every startup);
##   * the retry the shell runs on its own clock does not FLICKER the state it is retrying;
##   * a server that comes back unlocks the screen — the caption, the slider and the button all return.
##
## Driven through the shipped paths throughout: a real map-size click puts the ask in flight, and the
## retry goes through `MenuShell._on_capacity_retry_timeout`, which is what the shell's `Timer` calls.
func _run_server_unreachable_states() -> void:
	_bg.color = HudStyle.GROUND
	_shell.mode = MenuShell.LANDING
	_shell._activate_item(NEW_GAME_PANE_ID)
	await _settle()

	# --- an ask in flight over an answered row: still startable. The ⛔ case — a button that blinked
	# disabled on every open would be worse than the bug being fixed.
	_pick_map_size(SIZE_KEY_STANDARD)
	if _capacity_seam.state != FactionCapacity.STATE_PENDING:
		_fail("rivals: the size click did not put an ask in flight (%s)" % _capacity_seam.state)
	await _settle()
	_assert_begin_is_offered("an ask merely in flight")

	# --- …and nothing answers it. The cause on the caption, the run withheld.
	_fail_capacity(FactionCapacity.ERROR_TRANSPORT)
	await _settle()
	_assert_no_rival_slider("no server")
	_assert_says_none("no server")
	_assert_begin_is_withheld("no server")
	_assert_notice_reads("no server", MenuShell.NOTICE_NO_SERVER)
	if _shell._capacity_retry.is_stopped():
		_fail("rivals: an unreachable server left no clock re-asking, so the screen cannot recover")
	await _save("menu_new_game_rivals_no_server")

	# --- THE RETRY IS NOT A STATE CHANGE. It leaves the seam PENDING, and rendering that as "asking…"
	# would put a caption back under the row and unlock the button every `RIVALS_RETRY_SECONDS`. No
	# PNG: the claim is about the frame NOT changing, which is what a second identical picture cannot
	# show.
	_shell._on_capacity_retry_timeout()
	if _capacity_seam.state != FactionCapacity.STATE_PENDING:
		_fail("rivals: the retry clock put no fresh ask in flight (%s)" % _capacity_seam.state)
	await _settle()
	_assert_says_none("a retry of an unreachable server")
	_assert_begin_is_withheld("a retry of an unreachable server")
	_assert_notice_reads("a retry of an unreachable server", MenuShell.NOTICE_NO_SERVER)

	# --- THE SERVER CAME BACK. The unreachable state must not outlive the problem.
	_answer_capacity(CAPACITY_DEFAULT_STANDARD, CAPACITY_MAX_STANDARD)
	await _settle()
	if _shell._rival_caption.text != MenuShell.RIVALS_CAPTION_CEILING_FORMAT % CAPACITY_MAX_STANDARD:
		_fail("rivals: an answer after an unreachable server left the caption reading %s"
			% _shell._rival_caption.text)
	_assert_begin_is_offered("the server came back")
	_assert_notice_reads("the server came back", "")
	if _find_slider(_shell._rivals_box) == null:
		_fail("rivals: an answer after an unreachable server offered no slider")
	if not _shell._capacity_retry.is_stopped():
		_fail("rivals: the re-ask clock is still running against a server that answered")
	await _save("menu_new_game_rivals_recovered")
	await _assert_a_bounced_sessions_notice_is_retracted_too()


## **THE LINE A FAILED RUN LEFT BEHIND GOES WITH THE SAME EVIDENCE.** `Main` hands the landing screen
## the shell's own `NOTICE_NO_SERVER` after an unanswered seat claim, so a server that comes up while
## the player is still on this screen must clear THAT copy as well — a stale "cannot connect" over a
## working New Game pane is the defect the retry clock exists to prevent. No PNG: the claim is that a
## box is gone, and the `_recovered` still already shows an empty rail.
func _assert_a_bounced_sessions_notice_is_retracted_too() -> void:
	_shell.set_notice(MenuShell.NOTICE_NO_SERVER)
	_pick_map_size(SIZE_KEY_SMALLEST)
	_fail_capacity(FactionCapacity.ERROR_TRANSPORT)
	await _settle()
	_assert_notice_reads("a bounced session, still unreachable", MenuShell.NOTICE_NO_SERVER)
	if _count_notice_boxes(_shell) != 1:
		_fail("landing notice: the shell says the same thing in %d boxes at once"
			% _count_notice_boxes(_shell))
	_capacity_seam.retry()
	_answer_capacity(CAPACITY_DEFAULT_SMALLEST, CAPACITY_MAX_SMALLEST)
	await _settle()
	_assert_notice_reads("a bounced session, after the server answered", "")


## **THE OTHER SCREEN THIS PAIR OF DEFECTS OWNS: a run that could not start at all.** `Main` bounces
## back here when the seat claim goes unanswered, and the reason is shown ON THE RAIL beside New Game
## and Load Game rather than centred alone on a black loading overlay with nothing to press. The
## sentence is the shell's own `NOTICE_NO_SERVER`, which is exactly what `Main` hands back for an
## unanswered claim — one constant, so the frame renders what a player reads and the two paths cannot
## stack into two boxes.
##
## Rendered last, and with the capacity ask still failed, because that is the true shape of the session
## it reports: nothing is listening, so the notice and the withheld Begin are on screen together.
func _run_landing_notice_state() -> void:
	_shell._activate_item(NEW_GAME_PANE_ID)
	# A real size click is what re-asks, and nothing answers it — the same shipped pair the state above
	# uses, rather than poking the seam.
	_pick_map_size(SIZE_KEY_ROOMIEST)
	_fail_capacity(FactionCapacity.ERROR_TRANSPORT)
	_shell.set_notice(MenuShell.NOTICE_NO_SERVER)
	await _settle()
	if not _shell._notice_panel.visible:
		_fail("landing notice: a session's failure was handed in and nothing showed it")
	await _save("menu_landing_seat_refused")


## **AN ANSWER CAN LAND ON A PANE THAT IS GONE**, and it must not take the shell with it. No PNG: the
## failure is an error on a freed node, which either aborts the run or prints and leaves a frame that
## looks entirely normal. The pane is left with an ask in flight, swapped away, given a frame for the
## `queue_free`s to land, and only then answered.
func _assert_an_answer_survives_leaving_the_pane() -> void:
	_pick_map_size(SIZE_KEY_SMALLEST)
	_shell._activate_item(OPTIONS_PANE_ID)
	await _settle()
	_answer_capacity(CAPACITY_DEFAULT_SMALLEST, CAPACITY_MAX_SMALLEST)
	await _settle()
	if _capacity_seam.state != FactionCapacity.STATE_READY:
		_fail("rivals: an answer delivered after a pane change left the seam in %s" % _capacity_seam.state)
	# …and the pane still builds afterwards, so the shell is not merely quiet but intact.
	_shell._activate_item(NEW_GAME_PANE_ID)
	await _settle()
	if _find_slider(_shell._rivals_box) == null:
		_fail("rivals: reopening the pane after an off-pane answer offered no slider")


## **THE COUNT ON SCREEN IS THE COUNT ON THE WIRE — including the one nobody touched.**
##
## The row opens on the server's map-scaled offer (4 rivals on a Standard grid), and a player who
## never drags the slider still SENDS that number: `_on_capacity_changed` seeds the pick from the
## answer and nothing downstream re-derives it. Since an absent count no longer means "the server's
## configured default" but its UNATTENDED roster — **zero rivals** — a break in that chain is the
## difference between the world the screen promised and an empty one, and it would look completely
## normal in every frame. No PNG for the same reason.
##
## The chain is walked at both ends: the shell's own `new_game_requested`, which is what
## `LandingScreen` stashes, and `Main.new_game_line`, which is what actually reaches the socket.
func _assert_the_shown_count_is_the_count_sent() -> void:
	# **THE STATE A SCREEN NOBODY HAS TOUCHED IS IN**, staged explicitly: the frames above dragged the
	# slider, and a pick is deliberately kept across pane changes and re-asks, so it would otherwise be
	# inherited here and this assertion would be about a player who DID choose. These two fields are
	# the whole of "untouched" — the pick, and the flag that says an answer may seat it.
	_shell._rival_picked = false
	_shell._rival_count = FactionCapacity.NO_COUNT
	_pick_map_size(SIZE_KEY_STANDARD)
	_answer_capacity(CAPACITY_DEFAULT_STANDARD, CAPACITY_MAX_STANDARD)
	await _settle()
	var slider := _find_slider(_shell._rivals_box)
	if slider == null:
		_fail("shown-is-sent: the answered row offered no control to read")
		return
	if int(slider.value) != CAPACITY_DEFAULT_STANDARD:
		_fail("shown-is-sent: the row opened on %d, not the server's offer of %d"
			% [int(slider.value), CAPACITY_DEFAULT_STANDARD])

	# THE SHELL'S OUTPUT. Driven through the handler the Begin button is connected to — the button
	# itself sits below this harness window's fold, and a click that lands on nothing would assert
	# nothing.
	var emitted: Array = []
	var sink := func(_preset: String, _w: int, _h: int, _seed: int, _profile: String, count: int) -> void:
		emitted.append(count)
	_shell.new_game_requested.connect(sink)
	_shell._on_begin_pressed()
	_shell.new_game_requested.disconnect(sink)
	if emitted.size() != 1:
		_fail("shown-is-sent: Begin emitted %d requests, not one" % emitted.size())
	elif int(emitted[0]) != CAPACITY_DEFAULT_STANDARD:
		_fail("shown-is-sent: the row showed %d and the request carried %d"
			% [CAPACITY_DEFAULT_STANDARD, int(emitted[0])])

	# THE WIRE. `Main` appends the count only when there is one, and that rule is the last place the
	# player's pick can be dropped.
	var line: String = MAIN_SCRIPT.new_game_line(
		WIRE_PRESET, WIRE_WIDTH, WIRE_HEIGHT, WIRE_SEED, WIRE_PROFILE, CAPACITY_DEFAULT_STANDARD)
	if not line.ends_with(" %d" % CAPACITY_DEFAULT_STANDARD):
		_fail("shown-is-sent: the new_game line does not carry the count (%s)" % line)
	# …and the absent case still carries NOTHING, which is a different request from a trailing 0.
	var omitted: String = MAIN_SCRIPT.new_game_line(
		WIRE_PRESET, WIRE_WIDTH, WIRE_HEIGHT, WIRE_SEED, WIRE_PROFILE, FactionCapacity.NO_COUNT)
	if omitted != "new_game %s %d %d %d %s" % [WIRE_PRESET, WIRE_WIDTH, WIRE_HEIGHT, WIRE_SEED, WIRE_PROFILE]:
		_fail("shown-is-sent: an unanswered ask put something on the line (%s)" % omitted)
	# Left as it was found: untouched, so the next state stages its own starting point.
	_shell._rival_picked = false


## **THE MAP-SIZE CLICK, WHICH IS THE ONE THE PLAYER MAKES REPEATEDLY.** A re-ask must not take the
## control away and put it back: the row was destroyed and redrawn on every click, and because the
## pending caption is a different height from the slider row, every row below it jumped — a visible
## flash, reported from a playtest.
##
## The two frames are the ask IN FLIGHT over a previous answer and the new answer landed, so the
## before/after is readable rather than inferred. **The identity check is what a frame cannot show**:
## a torn-down-and-rebuilt row renders identically to a preserved one, so the slider's instance id is
## carried across the click, and its RECT is compared too — the flash was layout, not just identity.
func _run_rivals_reask_states() -> void:
	# Start from a genuinely answered row, with a pick on it. The incoming size is the roomiest (the
	# failed state left it there), and `_on_size_input` ignores a click on the size already selected,
	# so this walk moves standard -> roomiest.
	_pick_map_size(SIZE_KEY_STANDARD)
	_answer_capacity(CAPACITY_DEFAULT_STANDARD, CAPACITY_MAX_STANDARD)
	await _settle()
	_drag_rival_slider(CAPACITY_PICKED)
	await _settle()
	var before := _find_slider(_shell._rivals_box)
	if before == null:
		_fail("rivals re-ask: no slider to preserve before the size click")
		return
	var before_id := before.get_instance_id()
	var before_rect := before.get_global_rect()
	var before_height := _shell._rivals_box.size.y
	# The summary reads the same resolved count the wire would carry, so it flickers with it.
	var before_summary := _shell._rivals_summary_text()

	# THE CLICK. The ask is now in flight and the answer has not landed.
	_pick_map_size(SIZE_KEY_ROOMIEST)
	await _settle()
	var during := _find_slider(_shell._rivals_box)
	if during == null:
		_fail("rivals re-ask: the control vanished while the new ceiling was in flight")
	elif during.get_instance_id() != before_id:
		_fail("rivals re-ask: the control was rebuilt (%d -> %d) rather than left alone"
			% [before_id, during.get_instance_id()])
	elif during.get_global_rect() != before_rect:
		_fail("rivals re-ask: the control moved during the ask (%s -> %s)"
			% [str(before_rect), str(during.get_global_rect())])
	if _shell._rivals_box.size.y != before_height:
		_fail("rivals re-ask: the row changed height during the ask (%f -> %f)"
			% [before_height, _shell._rivals_box.size.y])
	if _shell._rivals_summary_text() != before_summary:
		_fail("rivals re-ask: the summary flipped to %s during the ask (was %s)"
			% [_shell._rivals_summary_text(), before_summary])
	if _shell._resolved_rival_count() != CAPACITY_PICKED:
		_fail("rivals re-ask: the pick became %d while the new ceiling was in flight"
			% _shell._resolved_rival_count())
	await _save("menu_new_game_rivals_reask")

	# …and the answer lands, updating the SAME nodes: new ceiling, pick clamped to it. The ROOMIEST
	# grid's own pair — the click above asked about that map, so this is the answer it would get.
	_answer_capacity(CAPACITY_DEFAULT_ROOMIEST, CAPACITY_MAX_ROOMIEST)
	await _settle()
	var after := _find_slider(_shell._rivals_box)
	if after == null:
		_fail("rivals re-ask: the answer left no control at all")
	elif after.get_instance_id() != before_id:
		_fail("rivals re-ask: the answer replaced the control instead of updating it")
	elif int(after.max_value) != CAPACITY_MAX_ROOMIEST:
		_fail("rivals re-ask: the control kept the old ceiling %d, not the answered %d"
			% [int(after.max_value), CAPACITY_MAX_ROOMIEST])
	await _save("menu_new_game_rivals_reasked")


## **THE ROW HOLDS ITS HEIGHT WHATEVER STATE IT IS IN**, so the seed field and the actions row below
## it do not move as answers land. No PNG of its own: it is a comparison BETWEEN states, which is
## exactly what a still cannot carry.
func _assert_row_height_is_stable() -> void:
	# Incoming size is the roomiest; each step names a different one so every click really re-asks,
	# and each is answered with ITS OWN grid's numbers.
	_pick_map_size(SIZE_KEY_SMALLEST)
	_answer_capacity(CAPACITY_DEFAULT_SMALLEST, CAPACITY_MAX_SMALLEST)
	await _settle()
	var with_slider := _shell._rivals_box.size.y
	_pick_map_size(SIZE_KEY_STANDARD)
	_answer_capacity(0, CAPACITY_MAX_ALONE)
	await _settle()
	var without_slider := _shell._rivals_box.size.y
	if with_slider != without_slider:
		_fail("rivals: the row is %f tall with a slider and %f without, so everything below it jumps"
			% [with_slider, without_slider])


## No slider means no range was invented. Checked rather than eyeballed: a control that quietly
## appeared with a 0..0 range would look like a deliberate layout in the frame.
func _assert_no_rival_slider(state_name: String) -> void:
	if _find_slider(_shell._rivals_box) != null:
		_fail("rivals (%s): a slider is offered with no ceiling to offer it against" % state_name)


## **ONLY AN UNREACHABLE SERVER BLOCKS THE RUN.** Every other unanswered state still owes the player a
## game they can start — a count is simply omitted — so this asserts the button is both THERE and live.
func _assert_begin_is_offered(state_name: String) -> void:
	var begin := _find_button(_shell, BEGIN_BUTTON_LABEL)
	if begin == null:
		_fail("rivals (%s): there is no way to begin the run at all" % state_name)
	elif begin.disabled:
		_fail("rivals (%s): the run is blocked by a state that can still start one" % state_name)


## …and its twin: with nothing listening, the button must be present and LOCKED. Present, because a
## vanished action is a layout the player cannot ask about; locked, because pressing it swapped to a
## `Main` that sat on a black loading screen forever.
func _assert_begin_is_withheld(state_name: String) -> void:
	var begin := _find_button(_shell, BEGIN_BUTTON_LABEL)
	if begin == null:
		_fail("rivals (%s): the primary action vanished instead of locking" % state_name)
	elif not begin.disabled:
		_fail("rivals (%s): a run that cannot work is still offered" % state_name)


## **THE ONE PLACE THIS SCREEN EXPLAINS ITSELF.** A greyed-out "Begin the trail" with nothing saying
## why is the state this notice exists to prevent, and `""` is the assertion that it CLEARS — which is
## the half that pins a stale "cannot connect" cannot outlive the server coming back.
func _assert_notice_reads(state_name: String, expected: String) -> void:
	var panel := _shell._notice_panel
	if panel == null or not is_instance_valid(panel):
		_fail("notice (%s): the rail has no notice box at all" % state_name)
		return
	if expected.is_empty():
		if panel.visible:
			_fail("notice (%s): a notice is still on screen reading %s"
				% [state_name, _shell._notice_label.text])
		return
	if not panel.visible:
		_fail("notice (%s): nothing on screen says why" % state_name)
	elif _shell._notice_label.text != expected:
		_fail("notice (%s): the rail reads %s" % [state_name, _shell._notice_label.text])


## How many VISIBLE boxes carry the notice sentence. One is the contract: the shell's own unreachable
## state and a bounced session's handed-in line are the same fact, and a player must not read it twice.
func _count_notice_boxes(node: Node) -> int:
	var found := 0
	if node is Label and (node as Label).text == MenuShell.NOTICE_NO_SERVER and (node as Label).is_visible_in_tree():
		found += 1
	for child in node.get_children():
		found += _count_notice_boxes(child)
	return found


## **AN UNANSWERED ASK STATES ITS COUNT AND EXPLAINS NOTHING.** Two claims in one, because they are one
## decision: the readout reads `None` — the count the world will actually be built with — and the
## caption is EMPTY, no sentence about servers or capacity queries. Asserted rather than eyeballed
## because a paragraph creeping back under this row looks like a deliberate caption in a frame.
func _assert_says_none(state_name: String) -> void:
	if _shell._rival_readout == null or not is_instance_valid(_shell._rival_readout):
		_fail("rivals (%s): the row shows no count at all" % state_name)
	elif _shell._rival_readout.text != MenuShell.RIVALS_READOUT_NONE:
		_fail("rivals (%s): the readout reads %s, not the count that will be used"
			% [state_name, _shell._rival_readout.text])
	if _shell._rival_caption == null or not is_instance_valid(_shell._rival_caption):
		_fail("rivals (%s): the caption node is gone, so the row's height is not held" % state_name)
	elif _shell._rival_caption.text != "":
		_fail("rivals (%s): an unanswered ask explains itself on screen: %s"
			% [state_name, _shell._rival_caption.text])


## The capacity seam's ids must not be read by the save seam, which shares its drain. Both are built
## in one `_ready` on the landing screen, so the microsecond clock alone cannot separate them — the
## shared allocator's tie-break count is what does, and this is what fails if it goes away.
func _assert_capacity_ids_are_disjoint_from_the_save_seam() -> void:
	var saves := SaveSlots.new()
	saves.set_sender(_send)
	saves.refresh()
	var save_id := _last_request_id
	var capacity := FactionCapacity.new()
	capacity.set_sender(_send)
	var grid: Dictionary = MapSizes.option_for(MapSizes.DEFAULT_KEY)
	capacity.request(int(grid["width"]), int(grid["height"]))
	if _last_request_id == save_id:
		_fail("rivals: the capacity seam spent id %d, which the save seam still has in flight" % save_id)
	# …and the capacity ANSWER must reach nothing on the save seam, which is the consequence.
	saves.deliver([{
		"request_id": _last_request_id,
		"ok": true,
		"kind": FactionCapacity.KIND_CAPACITY,
		"default_ai_faction_count": CAPACITY_DEFAULT_STANDARD,
		"max_ai_faction_count": CAPACITY_MAX_STANDARD,
	}])
	if saves.list_state != SaveSlots.LIST_PENDING:
		_fail("rivals: a capacity answer moved the save seam's list to %s" % saves.list_state)


## Answer the ask the seam has in flight, in the bridge's own reply shape.
func _answer_capacity(default_count: int, max_count: int) -> void:
	_capacity_seam.deliver([{
		"request_id": _last_request_id,
		"ok": true,
		"kind": FactionCapacity.KIND_CAPACITY,
		"default_ai_faction_count": default_count,
		"max_ai_faction_count": max_count,
	}])


func _fail_capacity(token: String) -> void:
	_capacity_seam.deliver([{
		"request_id": _last_request_id,
		"ok": false,
		"error": token,
	}])


## Click a map size, through the shipped `_on_size_input`. **That is what re-asks**: the ceiling is
## about the grid being made, so a new size is a new question — and it is also how a pick made on a
## roomier map meets a smaller one's ceiling.
func _pick_map_size(key: String) -> void:
	var click := InputEventMouseButton.new()
	click.button_index = MOUSE_BUTTON_LEFT
	click.pressed = true
	_shell._on_size_input(click, key)


## Move the slider through its own `value_changed`, so the pick goes down the shipped path.
func _drag_rival_slider(count: int) -> void:
	var slider := _find_slider(_shell._rivals_box)
	if slider == null:
		_fail("rivals: no slider to drag")
		return
	slider.value = count


func _find_slider(node: Node) -> HSlider:
	if node == null or not is_instance_valid(node):
		return null
	if node is HSlider:
		return node as HSlider
	for child in node.get_children():
		var found := _find_slider(child)
		if found != null:
			return found
	return null


## **THE LOAD / SAVE PANES, over a fake transport.** The seam is real, its decode and routing are
## real, and every frame below is the shipped builder rendering the seam's actual state — the only
## thing standing in for a server is `_send`, which records the request id and answers nothing until
## this harness says so. That is what lets the failure states (no server, no saves, a refused name)
## be rendered at all: none of them is reachable from a healthy stack.
func _run_saves_states() -> void:
	_save_seam = SaveSlots.new()
	_save_seam.set_sender(_send)
	_shell.set_save_slots(_save_seam)

	# --- LOAD, on the landing screen: the list, from a real `list_saves` answer -------------------
	_bg.color = HudStyle.GROUND
	_shell.mode = MenuShell.LANDING
	_shell._activate_item(LOAD_PANE_ID)
	_answer_list(_slot_fixtures())
	await _settle()
	if _save_seam.list_state != SaveSlots.LIST_READY:
		_fail("load list: a delivered answer left the seam in %s" % _save_seam.list_state)
	await _save("menu_load_list")

	# --- …with a row selected, which is what arms the buttons ------------------------------------
	_select(FIXTURE_SLOT_MIDWINTER)
	await _settle()
	await _save("menu_load_selected")

	# --- …and the DELETE two-step armed. The confirm button carries what it will destroy, which is
	#     this shell's whole confirmation pattern — there is no modal.
	_shell._on_delete_pressed()
	await _settle()
	_assert_confirm_names_the_slot()
	await _save("menu_load_delete_confirm")
	_shell._on_delete_cancelled()

	# --- LOAD from inside a run: the same pane, the destructive wording ---------------------------
	_bg.color = MAP_TONE
	_shell.mode = MenuShell.PAUSE
	_shell._activate_item(LOAD_PANE_ID)
	_answer_list(_slot_fixtures())
	_select(FIXTURE_SLOT_MIDWINTER)
	await _settle()
	await _save("menu_load_in_run")

	# --- SAVE: a NEW slot name typed into the field ----------------------------------------------
	_shell._activate_item(SAVE_PANE_ID)
	_answer_list(_slot_fixtures())
	_type(TYPED_NEW_NAME)
	await _settle()
	await _save("menu_save")

	# --- …the same field naming a slot that EXISTS, which is a different act and says so ----------
	_type(FIXTURE_SLOT_MIDWINTER)
	await _settle()
	await _save("menu_save_overwrite")

	# --- …and the reserved name, refused under the field rather than by a round trip --------------
	_type(TYPED_RESERVED_NAME)
	await _settle()
	if SaveSlots.slot_name_error(TYPED_RESERVED_NAME) == "":
		_fail("slot names: the reserved autosave slot was accepted by the whitelist")
	await _save("menu_save_reserved_name")
	_type("")

	# --- NOTHING SAVED YET. `LIST_READY` with no rows, which is an invitation and not a failure ---
	_shell._activate_item(LOAD_PANE_ID)
	_answer_list([])
	await _settle()
	await _save("menu_load_empty")

	# --- NO SERVER. The landing screen is reachable with none, so this is a first-run state, not an
	#     edge case: it must name the problem and offer the ask again.
	_shell._activate_item(LOAD_PANE_ID)
	_fail_list(SaveSlots.ERROR_TRANSPORT)
	await _settle()
	if _save_seam.list_state != SaveSlots.LIST_FAILED:
		_fail("no-server list: a refusal left the seam in %s" % _save_seam.list_state)
	await _save("menu_load_no_server")

	# --- THE CONFIG-DRIFT NOTICE. Not part of the shell — `Main` raises it over the loaded world —
	#     but it is the other half of this feature's UI and this is the harness that can see it.
	_drift_notice = ConfigDriftNotice.new()
	_root.add_child(_drift_notice)
	_drift_notice.show_drift(DRIFT_FIXTURE)
	await _settle()
	if not _drift_notice.visible:
		_fail("config drift: a non-empty drift list rendered nothing")
	await _save("config_drift")

	await _assert_text_focus_is_handed_back()
	await _assert_caret_survives_a_mid_string_edit()
	_assert_request_ids_do_not_repeat_across_seams()


## **THE KEYBOARD IS BORROWED, NOT TAKEN** — the behavioural half of `MapView`'s polled-input guard,
## and it takes no PNG because none of it is visible.
##
## Every gameplay key in the client is arbitrated by `KeyboardArbiter`, and one of the two facts that
## arbiter runs on is the ONE predicate this file also asks: `TextEntryFocus.held_in`. (The other is
## whether a modal menu is open.)
##
## That guard's failure in the other direction is worse than the bug it fixes: focus left STUCK after
## the pane is gone kills WASD *and* every panel toggle for the rest of the session, with nothing on
## screen to explain it. So both halves are asserted — the field TAKES focus, and every exit HANDS IT
## BACK.
##
## **WHAT THIS HARNESS CANNOT PROVE**: neither `MapView` nor `Main` is instantiated here, so the
## suppression itself — that a guarded hotkey does not fire — is untested. What IS tested is the
## predicate the arbiter runs on, called directly, against a really focused field. The suppression is
## `tools/hotkey_guard.gd`'s job.
func _assert_text_focus_is_handed_back() -> void:
	if _drift_notice != null:
		_drift_notice.queue_free()
		_drift_notice = null
	await get_tree().process_frame
	_shell.mode = MenuShell.PAUSE
	_shell._activate_item(SAVE_PANE_ID)
	_answer_list(_slot_fixtures())
	_type(TYPED_NEW_NAME)
	await get_tree().process_frame
	if _shell._save_name_edit == null:
		_fail("focus: the Save pane built no name field")
		return
	# Typing is what focuses the field on the shipped path — the pane is rebuilt on every keystroke,
	# so `_on_save_name_changed` re-grabs onto the NEW node. If that ever stopped working the field
	# would drop the caret mid-word, and every check below would be asserting nothing.
	if not _focused_is_text_entry():
		_fail("focus: typing into the Save pane did not leave the name field holding the keyboard")

	_shell.release_text_focus()
	if _focused_is_text_entry():
		_fail("focus: release_text_focus left a text control focused")

	# Leaving the pane hands it back — the path a player takes by clicking any other nav row.
	_shell._save_name_edit.grab_focus()
	await get_tree().process_frame
	if not _focused_is_text_entry():
		_fail("focus: the name field would not take the keyboard back")
	_shell._activate_item(OPTIONS_PANE_ID)
	if _focused_is_text_entry():
		_fail("focus: switching panes left the Save field holding the keyboard")

	# …and so does submitting, which ends the typing act whether it came from the button or from
	# Enter in the field.
	_shell._activate_item(SAVE_PANE_ID)
	_answer_list(_slot_fixtures())
	_type(TYPED_NEW_NAME)
	await get_tree().process_frame
	if not _focused_is_text_entry():
		_fail("focus: the Save pane did not re-take the keyboard before the submit check")
	_shell._on_save_pressed()
	if _focused_is_text_entry():
		_fail("focus: submitting a save left the name field holding the keyboard")
	# Settle the seam so it is not left holding an op nothing will ever answer.
	_answer_save_op(TYPED_NEW_NAME)

	# **THE PREDICATE MUST STAY NARROW.** A focused Button does not consume letters, so widening this
	# to "anything focused" would kill WASD and every panel toggle after each click on a HUD control
	# — a worse bug than the one the guard exists for, and one no PNG would show.
	var probe := Button.new()
	_root.add_child(probe)
	probe.grab_focus()
	await get_tree().process_frame
	if get_viewport().gui_get_focus_owner() != probe:
		_fail("focus: the Button probe would not take focus, so the narrowness check proved nothing")
	elif _focused_is_text_entry():
		_fail("focus: a focused Button counts as text entry — the guard would kill the hotkeys")
	probe.release_focus()
	probe.queue_free()


## **THE CARET SURVIVES THE REBUILD, so editing is not append-only.** No PNG: a caret is one blinking
## pixel column, and what is being asserted is where the NEXT character lands, which no still shows.
##
## The pane is rebuilt whole on every keystroke, so the field being typed into is a new node each
## time. Restoring that node's caret to the end of the string made only tail editing work: `abcd` with
## `x` typed between `a` and `b` correctly became `axbcd` and then put the caret at column 5, so the
## following character landed at the end instead of after the `x`.
##
## Driven by pushing a real unicode key event at the viewport, not by the harness's `_type` and not
## by `insert_text_at_caret`: only `LineEdit.gui_input` both moves the caret AND emits `text_changed`,
## and it is the ORDER of those two — caret first — that makes carrying the reported column correct.
## `insert_text_at_caret` alone moves the caret and emits nothing, so it would prove neither.
func _assert_caret_survives_a_mid_string_edit() -> void:
	_shell.mode = MenuShell.PAUSE
	_shell._activate_item(SAVE_PANE_ID)
	_answer_list(_slot_fixtures())
	_type(CARET_BASE_TEXT)
	await get_tree().process_frame
	var field := _shell._save_name_edit
	if field == null:
		_fail("caret: the Save pane built no name field")
		return
	# STAGE THE REAL CONDITION FIRST — a populated field with the caret parked mid-string. Asserted,
	# because on an empty field or a caret that would not move every check below passes trivially.
	if field.text != CARET_BASE_TEXT:
		_fail("caret: the name field holds %s, not the text this check edits" % field.text)
		return
	field.grab_focus()
	await get_tree().process_frame
	field.caret_column = CARET_INSERT_COLUMN
	if field.caret_column != CARET_INSERT_COLUMN:
		_fail("caret: the caret would not park mid-string, so this check proves nothing")
		return
	if get_viewport().gui_get_focus_owner() != field:
		_fail("caret: the name field does not hold the keyboard, so a key event would go elsewhere")
		return

	_press_character(CARET_FIRST_CHAR)
	await get_tree().process_frame
	if _shell._save_name_text != CARET_AFTER_FIRST_TEXT:
		_fail("caret: a mid-string insert left the shell holding %s, not %s — the edit never reached the handler" \
			% [_shell._save_name_text, CARET_AFTER_FIRST_TEXT])
		return
	var rebuilt := _shell._save_name_edit
	if rebuilt == null or rebuilt == field:
		_fail("caret: the keystroke did not rebuild the pane, so the carried caret was never exercised")
		return
	if rebuilt.caret_column != CARET_AFTER_FIRST_COLUMN:
		_fail("caret: after inserting at column %d the rebuilt field sits at column %d, not %d" \
			% [CARET_INSERT_COLUMN, rebuilt.caret_column, CARET_AFTER_FIRST_COLUMN])

	# …and the consequence a player actually feels: the NEXT character lands where the caret is.
	if get_viewport().gui_get_focus_owner() != rebuilt:
		_fail("caret: the rebuilt field did not take the keyboard back, so the next key went nowhere")
		return
	_press_character(CARET_SECOND_CHAR)
	await get_tree().process_frame
	if _shell._save_name_text != CARET_AFTER_SECOND_TEXT:
		_fail("caret: typing on after a mid-string insert produced %s, not %s" \
			% [_shell._save_name_text, CARET_AFTER_SECOND_TEXT])
	_type("")
	_shell.release_text_focus()


## One typed character, through `Viewport.push_input` — the same dispatch a keyboard reaches the
## focused `LineEdit` by. The unicode is what `LineEdit` reads; there is deliberately no keycode, so
## the event cannot also match a shortcut on its way in.
func _press_character(ch: String) -> void:
	var key := InputEventKey.new()
	key.pressed = true
	key.unicode = ch.unicode_at(0)
	get_viewport().push_input(key)


## **A REQUEST ID IS NEVER REUSED ACROSS A SCENE CHANGE.** Also no PNG — this is the seam's
## bookkeeping, and its failure is a wrong answer rather than a wrong picture.
##
## A load swaps the scene and the new world builds a NEW `SaveSlots`, but the native worker's answer
## channel is process-global and hands the new seam whatever the old one left in flight. Every
## instance starting at `REQUEST_ID_BASE` made the stale `list_saves` answer collide with the id the
## new seam had just spent on `load_game` — and a LIST reply says `ok: true`, so a load that was
## refused was reported as having succeeded.
##
## Both seams are built here, in the order a scene change builds them, and driven through the real
## `deliver`.
func _assert_request_ids_do_not_repeat_across_seams() -> void:
	# The seam the old scene leaves behind, with an ask genuinely unanswered: an id in flight is the
	# only kind that can be mis-delivered, so the collision is staged rather than assumed.
	var outgoing := SaveSlots.new()
	outgoing.set_sender(_send)
	outgoing.refresh()
	var stale_id := _last_request_id
	if outgoing.list_state != SaveSlots.LIST_PENDING:
		_fail("request ids: the outgoing seam left no list ask in flight, so nothing could collide")
		return

	# The seam the loaded world builds, whose FIRST ask is the load itself.
	var incoming := SaveSlots.new()
	incoming.set_sender(_send)
	incoming.request_load(FIXTURE_SLOT_MIDWINTER)
	var load_id := _last_request_id
	if incoming.op_in_flight != SaveSlots.KIND_LOAD:
		_fail("request ids: the incoming seam put no load in flight")
		return
	if load_id == stale_id:
		_fail("request ids: a new seam spent id %d, which the previous seam still has in flight" % load_id)

	var finished: Array = []
	incoming.op_finished.connect(
		func(kind: String, _slot: String, ok: bool, _error: String, _drift: Array) -> void:
			finished.append({"kind": kind, "ok": ok}))

	# THE CONSEQUENCE: the outgoing seam's list answer, arriving after the swap, must reach nothing.
	# It says `ok: true`, so under the collision it finished the load as a success.
	incoming.deliver([{
		"request_id": stale_id,
		"ok": true,
		"kind": SaveSlots.KIND_LIST,
		"slots": _slot_fixtures(),
	}])
	if not finished.is_empty():
		_fail("request ids: the outgoing seam's list answer finished %s on the new seam" % str(finished[0]))
	if incoming.op_in_flight != SaveSlots.KIND_LOAD:
		_fail("request ids: the outgoing seam's list answer cleared a load that is still in flight")

	# …and the load's OWN answer still finishes it, so the check above is not passing on a seam that
	# has simply stopped listening.
	incoming.deliver([{
		"request_id": load_id,
		"ok": true,
		"kind": "save_op",
		"slot": FIXTURE_SLOT_MIDWINTER,
		"error": "",
		"config_drift": [],
	}])
	if finished.size() != 1 or String(finished[0]["kind"]) != SaveSlots.KIND_LOAD:
		_fail("request ids: the load's own answer did not finish the load (%s)" % str(finished))


## **THE SHIPPED PREDICATE, CALLED** — not a restatement of it. `KeyboardArbiter.owner_for` makes
## exactly this call to decide whether text entry owns the keyboard, so a drift in it fails here.
func _focused_is_text_entry() -> bool:
	return TextEntryFocus.held_in(get_viewport())


func _answer_save_op(slot: String) -> void:
	_save_seam.deliver([{
		"request_id": _last_request_id,
		"ok": true,
		"kind": "save_op",
		"slot": slot,
		"error": "",
		"config_drift": [],
	}])


## The canned slot list: the reserved autosave row, two named saves, and one small enough to render
## in the OTHER size unit. Newest first, the order the server answers in.
func _slot_fixtures() -> Array:
	var now := int(Time.get_unix_time_from_system())
	return [
		_slot_row(SaveSlots.AUTOSAVE_SLOT, 47, "earthlike", 80, 52,
			FIXTURE_SIZE_AUTOSAVE, now - AGE_RECENT_SECONDS),
		_slot_row(FIXTURE_SLOT_MIDWINTER, 31, "earthlike", 80, 52,
			FIXTURE_SIZE_MIDWINTER, now - AGE_HOURS_SECONDS),
		_slot_row(FIXTURE_SLOT_FIRST, 12, "polar_contrast", 64, 40,
			FIXTURE_SIZE_FIRST, now - AGE_DAYS_SECONDS),
		_slot_row(FIXTURE_SLOT_SCRATCH, 3, "earthlike", 48, 32,
			FIXTURE_SIZE_TINY, FIXED_STAMP_UNIX),
	]


func _slot_row(slot: String, turn: int, preset: String, width: int, height: int,
		size_bytes: int, modified: int) -> Dictionary:
	return {
		"slot": slot,
		"turn": turn,
		"campaign_title": FIXTURE_TITLE,
		"map_preset_id": preset,
		"width": width,
		"height": height,
		"world_seed": 0,
		"start_profile_id": "late_forager_tribe",
		"size_bytes": size_bytes,
		"modified_unix_seconds": modified,
	}


## The fake transport. Records the id so a reply can correlate, and reports the ask as sent.
func _send(request_id: int, _ask: Dictionary) -> bool:
	_last_request_id = request_id
	return true


## Answer the list query that is in flight, through the seam's real `deliver`.
func _answer_list(rows: Array) -> void:
	_save_seam.deliver([{
		"request_id": _last_request_id,
		"ok": true,
		"kind": SaveSlots.KIND_LIST,
		"slots": rows,
	}])


## …and refuse it, with a token from the server's own vocabulary.
func _fail_list(token: String) -> void:
	_save_seam.deliver([{
		"request_id": _last_request_id,
		"ok": false,
		"error": token,
	}])


## Click a row, through the same handler the row's `gui_input` reaches.
func _select(slot: String) -> void:
	var click := InputEventMouseButton.new()
	click.button_index = MOUSE_BUTTON_LEFT
	click.pressed = true
	_shell._on_slot_row_input(click, slot, _shell._active_pane == SAVE_PANE_ID)


## Type into the Save pane's name field, through the field's own `text_changed` handler.
##
## The FIELD is put in the state a finished piece of typing leaves it in first — the text, and the
## caret after its last character — because the shipped handler now reads the caret off the node that
## reported the edit. Assigning `text` emits nothing, so this is still one synthetic `text_changed`.
func _type(text: String) -> void:
	if is_instance_valid(_shell._save_name_edit):
		_shell._save_name_edit.text = text
		_shell._save_name_edit.caret_column = text.length()
	_shell._on_save_name_changed(text)


## The armed delete button must NAME the slot it will destroy — that label IS the confirmation, so a
## generic "Confirm" would quietly remove the only thing standing between a click and a lost save.
func _assert_confirm_names_the_slot() -> void:
	if not _find_button_containing(_shell, FIXTURE_SLOT_MIDWINTER):
		_fail("delete confirm: no button names the slot it would delete")


func _find_button_containing(node: Node, needle: String) -> bool:
	return _find_button(node, needle) != null


## The same walk, handing the BUTTON back: the rival-count states assert on its `disabled` flag, not
## merely on its existence. One traversal, so the two questions cannot drift apart.
func _find_button(node: Node, needle: String) -> Button:
	if node is Button and (node as Button).text.contains(needle):
		return node as Button
	for child in node.get_children():
		var found := _find_button(child, needle)
		if found != null:
			return found
	return null


## The button is BUILT hidden and shown only while the pick differs from what is on screen, so a
## frame that silently lost it would look like a deliberate layout and pass review. Checked, not eyeballed.
func _assert_apply_visible(mode_name: String) -> void:
	if _shell._theme_apply == null or not _shell._theme_apply.visible:
		_fail("theme row (%s): a pending pick did not surface the Apply now button" % mode_name)


## The ONE failure sink, so `_failures` cannot drift from what was printed. Every caller passes the
## text AFTER the `FAIL` token, which is what the output scanning keys on.
func _fail(message: String) -> void:
	_failures += 1
	push_error("menu_preview: FAIL — %s" % message)


## **THE ONLY WAY OUT OF THIS HARNESS.** Every path that ends the run comes through here, so the
## status is derived from the run's own tally in exactly one place.
func _finish() -> void:
	if _failures > 0:
		print("menu_preview: RUN FAILED — %d failure(s); see the FAIL lines above" % _failures)
	else:
		print("menu_preview: run complete — no failures")
	get_tree().quit(EXIT_FAILED if _failures > 0 else EXIT_OK)


func _settle() -> void:
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame


func _save(name: String) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("menu_preview: null image (dummy renderer?) — skipping %s.png; run without --headless to capture" % name)
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		_fail("failed to save %s (err %d)" % [name, err])
	else:
		print("menu_preview: saved ", name, ".png")
