extends RefCounted
class_name WorkbenchVocab

## ALL-const vocabulary for the Workbench — labels, glyphs, geometry, font sizes.
##
## **A new label, glyph or threshold goes HERE, never as a fresh `const` on `WorkbenchShell`.**
## That is the whole reason this file exists: the const block on a coordinator is what grew into
## `Hud.gd`'s merge-conflict surface, and the HUD arc spent a decomposition removing it. This module
## has zero functions and zero vars by design — if something here needs logic, it belongs in
## `WorkbenchWidgets` instead.

# ---- surface ---------------------------------------------------------------
## Title in the rail's header. The Workbench is the designer/dev surface, distinct from the player
## HUD's "command console" language and from the legacy Inspector it replaces.
const SURFACE_TITLE := "Workbench"
## Shown under the title while the rail is expanded.
const SURFACE_SUBTITLE := "designer tools"
## The hotkey hint rendered in the rail header. Backquote is the conventional dev-surface key and
## costs the game no letter it may still want.
const SURFACE_HOTKEY := "`"

# ---- geometry --------------------------------------------------------------
## Nominal width of the whole surface when it reserves its edge, before any drag-resize.
const SURFACE_WIDTH := 560.0
## Bounds on that drag-resize, so the surface can never be dragged to uselessness or over the map.
const SURFACE_MIN_WIDTH := 380.0
const SURFACE_MAX_WIDTH := 900.0
## Rail width in each of its two states. Collapsed shows glyphs only.
const RAIL_WIDTH := 168.0
const RAIL_WIDTH_COLLAPSED := 52.0
## Padding inside the content column, and the gap between stacked blocks in it.
const CONTENT_PADDING := 16
const CONTENT_GAP := 12
## Vertical padding inside one rail entry, and the gap between entries.
const RAIL_ENTRY_PADDING_V := 7
const RAIL_ENTRY_GAP := 2
## Left accent bar marking the active rail entry.
const RAIL_ACTIVE_BAR_WIDTH := 2.0
const RAIL_SECTION_GAP := 14

# ---- type scale ------------------------------------------------------------
# Set directly with `add_theme_font_size_override` — `Typography.gd` is a no-op shim and styling
# through it fails SILENTLY (see the client hub's Typography note).
const FONT_SIZE_SURFACE_TITLE := 17
const FONT_SIZE_PAGE_TITLE := 19
const FONT_SIZE_SECTION := 12
const FONT_SIZE_BODY := 13
const FONT_SIZE_HINT := 11
const FONT_SIZE_RAIL := 13
const FONT_SIZE_GLYPH := 15
const FONT_SIZE_VALUE := 13

# ---- config tuning page ----------------------------------------------------
const TUNING_PAGE_TITLE := "Config Tuning"
const TUNING_PAGE_SUBTITLE := "Edit sim tunables and start a run on them"
## The restart-scoped contract, said on the surface rather than left in a doc — a player of this
## panel must never wonder why the running world did not change.
const TUNING_BANNER := "Changes apply on the NEXT New Game — the running world is not retuned."
const TUNING_APPLY_LABEL := "Apply & New Game"
const TUNING_REVERT_LABEL := "Revert all"
## THE ACTION BAR'S THREE STATES. Their job is that the surface never says one thing while the
## SERVER holds another — "edited" and "applied" are different facts and are worded as such.
##   CLEAN — nothing edited, nothing on the server.
##   UNSENT — `%d` rows differ from what the server was last told. The only state Apply acts in.
##   STAGED — nothing unsent; `%d` overrides are in force on the server for the next new game.
##   STAGED_CLEARED — staged, but every row is back at its default, so the overrides in force are the
##       defaults themselves. Its own line because "0 override(s) applied" reads as a bug.
const TUNING_CLEAN_STATUS := "No overrides — every value is the shipped default."
const TUNING_UNSENT_STATUS := "%d edit(s) not applied yet"
const TUNING_STAGED_STATUS := "%d override(s) applied — the next new game uses them"
const TUNING_STAGED_CLEARED_STATUS := "Applied — every value is back to the shipped default"
const TUNING_DEFAULT_PREFIX := "default "
## Marks a row whose value no longer matches the shipped config.
const MODIFIED_GLYPH := "●"
const TUNING_MANIFEST_PATH := "res://src/config/tuning_manifest.json"
## Said in place of the groups when the manifest is missing or unparseable. The page renders this and
## nothing else rather than crashing — a dev surface that cannot read its own data file must still
## come up, or the one page that could explain the problem is the one that will not open. `%s` is the
## manifest path, so the reader is told which file to go and look at.
const TUNING_MANIFEST_UNREADABLE := "Tuning manifest could not be read (%s). No parameters to show."
## The line `Apply` writes to the surface log. `%d` is the count of overridden parameters, `%s` the
## comma-joined config kinds they land in.
const TUNING_APPLY_LOG_FORMAT := "Config tuning: staged %d override(s) across %s — starting a new game."
## Said when `Revert all` has dropped the staged overrides on the server too.
const TUNING_REVERT_LOG := "Config tuning: overrides cleared."
## Said when there is no command transport (or it refused). The rows are left exactly as they are —
## the edits are still there to re-apply once a server is attached, and silently reverting them
## would look like the apply had worked. `%s` is the command that could not go.
const TUNING_OFFLINE_LOG := "Config tuning: no command transport — '%s' was not sent."
## Said when the overrides went but the new game could not be started, which is the one outcome the
## page must not report as success: the server holds the patch and the running world does not use it.
const TUNING_NO_NEW_GAME_LOG := "Config tuning: overrides sent, but no new game could be started."
## **THE SECOND LINE OF A PARAMETER ROW IS ONE SENTENCE, NOT TWO COLUMNS.** The hint and the default
## first shared that line as a wrapped caption on the left and a right-aligned readout on the right;
## at this surface's width the hint wrapped under the readout and the two collided. They are one
## wrapped run now, the default a trailing clause after a separator — so the hint gets the full row
## width and the default is found by its TINT rather than by a column that has to be kept clear.
## `%s` is the hint, `%s` the default readout (prefix and unit included).
const TUNING_HINT_LINE_FORMAT := "[color=#%s]%s[/color]  [color=#%s]%s  %s[/color]"
## What separates the hint from the default clause. A mid dot, not a comma: the two are separate
## statements about the parameter, not one sentence.
const TUNING_HINT_SEPARATOR := "·"
## A row counts as overridden once its value has moved at least this fraction of its own step away
## from the default. A `SpinBox` snaps to its step, so any real edit clears half a step, and a float
## default that does not land exactly on the step grid cannot make an untouched row read as dirty.
const TUNING_MODIFIED_STEP_FRACTION := 0.5

# ---- the equipment config, and the two pages that print it -----------------
## The snapshot key the sim's whole effective `EquipmentConfig` rides on, `serde_json`-serialized and
## decoded onto the frame as a plain `String`. Both config pages parse it themselves; neither holds
## any state derived from it beyond the parsed object, which is what makes their `reset()` real.
const CONFIG_JSON_KEY := "equipment_config_json"

## **THE ONLY TWO CONFIG KEY NAMES ANY CLIENT CODE KNOWS, AND THE ONE PLACE TO CHANGE THEM.**
##
## They exist to decide WHICH PAGE a top-level entry lands on and nothing else: the Kits page draws
## these two, the Equipment page draws every other top-level entry whatever it is, and everything
## below the top level is walked blind by `WorkbenchWidgets.build_config_object`. So a fourth gear
## block added to `equipment.json` appears on the Equipment page with no edit here, a renamed gear
## block changes nothing at all, and only a restructuring of the config's TOP LEVEL — the kit roster
## or the job defaults moving or being renamed — reaches this file. A page holding a list of field
## names is what this arrangement exists to prevent; it goes stale the moment the sim renames one,
## and it does so silently, by simply drawing nothing.
const CONFIG_KITS_KEY := "kits"
const CONFIG_DEFAULT_KITS_KEY := "default_kits"

## **THE ONE BOUNDED EXCEPTION, AND IT MUST NOT BECOME A WHITELIST.** The Kits page PROMOTES these two
## keys of a roster entry into the block's title: `kits[0]` is a coordinate that tells a reader
## nothing, while `Stalking kit (big_game)` names the thing they are looking at. Having said them in
## the title the page then hides **only the keys the title actually used** — every other key in the
## entry still renders blind, so a field added to a kit definition tomorrow still appears with no edit
## here. Promotion is a rendering of two values the page already drew, not a list of the fields a kit
## may have; the moment a third key is added because it "should be shown", that distinction is gone.
##
## With `CONFIG_KITS_KEY` / `CONFIG_DEFAULT_KITS_KEY` above, these are the ONLY config key names any
## client code holds, and this is the one place to change them.
const CONFIG_KIT_DISPLAY_NAME_KEY := "display_name"
const CONFIG_KIT_ID_KEY := "id"
## `display_name (id)` — both, when the entry states both. A roster entry may be edited into any
## shape, so the page degrades rather than assuming: only `id` → the id alone, only `display_name` →
## the name alone, neither → the walker's own `kits[N]`, with only the promoted key(s) suppressed in
## each case.
const KITS_TITLE_FORMAT := "%s (%s)"

## Said in place of a value that has nothing in it — an empty object, an empty array, an empty string,
## a JSON `null`. Rendered explicitly rather than left blank, because a blank right-hand column reads
## as a rendering bug rather than as the config's own answer.
const CONFIG_EMPTY := "—"
## Said in place of a subtree deeper than `CONFIG_MAX_DEPTH`. The tree walker knows nothing about the
## config's shape, so the depth is bounded rather than trusted: a self-referential or pathologically
## nested document would otherwise build controls until the client died.
const CONFIG_TOO_DEEP := "…"
## How many nesting levels the walker will descend. The shipped config is three deep
## (`kits[0].uses`), so this is roughly double it — deep enough that no honest config is truncated,
## shallow enough that a runaway one stops on screen instead of in a crash report.
const CONFIG_MAX_DEPTH := 6
## How one element of an array of objects is named: `kits[0]`, `kits[1]`. The INDEX is the only
## identity the walker has — it cannot know that a kit calls itself `display_name` — and it doubles as
## the reader's own coordinate into the file they are about to go and search.
const CONFIG_INDEX_FORMAT := "%s[%d]"
## What joins an array of scalars rendered onto one row (`hunt, forage`).
const CONFIG_LIST_SEPARATOR := ", "

const EQUIPMENT_PAGE_TITLE := "Equipment"
const EQUIPMENT_PAGE_SUBTITLE := "Every config block outside the kit roster"
const EQUIPMENT_HEADING := "CONFIG"
## Said when the config carries nothing but the kit roster and the job defaults, i.e. the Kits page
## has everything. Its own line rather than a bare "—", so the page is not mistaken for one that
## failed to read the wire.
const EQUIPMENT_NO_BLOCKS := "This config carries nothing outside the kit roster and the job defaults."

const KITS_PAGE_TITLE := "Kits"
const KITS_PAGE_SUBTITLE := "The kit roster and each job's default"
const KITS_ROSTER_HEADING := "KIT ROSTER"
const KITS_DEFAULTS_HEADING := "JOB DEFAULTS"

## The degraded state both pages owe. They must come up with no server at all (the preview harness
## runs without one), so each says what is missing rather than rendering an empty well. Worded per
## page rather than shared, so the reader is told which surface is empty.
const EQUIPMENT_NO_CONFIG := "No equipment config on the wire yet — it arrives with the first "\
	+ "snapshot of a world."
const KITS_NO_CONFIG := "No kit roster on the wire yet — it arrives with the first snapshot of a world."
## …and the Kits page's SECOND well, which is degraded by the same absence but must not print the same
## sentence twice on one page: two wells repeating one line reads as a rendering fault.
const KITS_NO_DEFAULTS := "No job defaults yet — they arrive with the roster."

# ---- services --------------------------------------------------------------
## Names under which the host (`Main`) files the Callables it lends the surface, and the ONLY thing
## a page and its host have to agree on.
##
## **This is why the shell never grows a parameter.** The transport arrived as two arguments on a
## `set_command_hooks(send, append_log)`; the tuning page's `Apply` then needed a third (start a new
## game, which only `Main` can build), and a fourth would have followed it. A page now asks for a
## capability BY NAME out of one dictionary, so the next page needing one adds a const here and a
## row to `Main`'s services — and edits no shell code at all.
##
##   SEND_COMMAND — `(line: String) -> bool`; false when nothing is attached or the socket refused.
##   APPEND_LOG   — `(text: String) -> void`; a line for the surface's status log.
##   NEW_GAME     — `() -> void`; re-issue the current world's `new_game`, which is what makes a
##                  restart-scoped override take effect.
const SERVICE_SEND_COMMAND := &"send_command"
const SERVICE_APPEND_LOG := &"append_log"
const SERVICE_NEW_GAME := &"new_game"
## SCENARIO_REQUEST — `(kind: String, args: Dictionary, on_reply: Callable) -> bool`; sends one
##                    scenario verb (`SCENARIO_KIND_*`) over the query bridge and later calls
##                    `on_reply(reply: Dictionary)`. False when there is no server to ask.
## PICK_TILE        — `(on_pick: Callable, on_cancel: Callable = Callable()) -> bool`; arms a ONE-SHOT
##                    map pick: the next hex click calls `on_pick(x: int, y: int)`; Escape or a second
##                    arm calls `on_cancel()`. False when there is no map to click.
const SERVICE_SCENARIO_REQUEST := &"scenario_request"
const SERVICE_PICK_TILE := &"pick_tile"

# ---- runtime commands ------------------------------------------------------
## The two verbs the tuning page speaks. **Fixed contract shared with `core_sim`** — the sim parses
## exactly these, so they are named here rather than spelled inline at the call site.
## `set_config_override <kind> <json>`: `kind` is a manifest `kind`, `json` the COMPACT sparse patch.
const COMMAND_SET_CONFIG_OVERRIDE := "set_config_override"
## `clear_config_overrides`: no arguments, drops every staged override on the server.
const COMMAND_CLEAR_CONFIG_OVERRIDES := "clear_config_overrides"

# ---- rail entries ----------------------------------------------------------
## Section headings the rail groups its entries under.
const SECTION_SIM := "SIMULATION"
const SECTION_WORLD := "WORLD"
const SECTION_DIAGNOSTICS := "DIAGNOSTICS"
## Placeholder copy for a registered page that has no implementation yet, so the rail can show the
## intended shape of the surface without a dead click.
const PLACEHOLDER_BODY := "Not built yet. This page is registered so the rail shows where the "\
	+ "replacement for the legacy Inspector's tabs is going."


# ---- the scenario page -------------------------------------------------------
const SCENARIO_PAGE_TITLE := "Scenario"
const SCENARIO_PAGE_SUBTITLE := "Edit a scenario file, save it, load it"
## The four verbs, spelled as `bridge/query.rs` matches them. Fixed contract with the native bridge.
const SCENARIO_KIND_LIST := "list_scenarios"
const SCENARIO_KIND_READ := "read_scenario"
const SCENARIO_KIND_WRITE := "write_scenario"
const SCENARIO_KIND_LOAD := "load_scenario"
## The directory the server accepts scenario files in, as a path prefix a new file's name goes after.
## The FIXTURE directory — the only one `write_scenario` accepts; matches the server's `SCENARIO_DIR`. A
## file whose path does not start with this is an export (`list_scenarios` lists those after the fixtures).
const SCENARIO_DIR_PREFIX := "core_sim/scenarios/"
## A dropdown entry for an exported file: `%s` is its basename.
const SCENARIO_EXPORT_ITEM_FORMAT := "export · %s"
## The footer's line for a file opened from an export: it will save to the fixture path.
const SCENARIO_EXPORT_STATUS := "Opened from export %s — saves to %s. Load after Save."
const SCENARIO_FILE_SUFFIX := ".json"
## The reply keys of the scenario answer (`answer_to_dict`).
const SCENARIO_REPLY_OK := "ok"
const SCENARIO_REPLY_ERROR := "error"
const SCENARIO_REPLY_REFUSALS := "refusals"
const SCENARIO_REPLY_FILES := "files"
const SCENARIO_REPLY_TEXT := "text"
const SCENARIO_REPLY_SCHEMA := "schema"

## **THE ONLY FILE-SHAPE KNOWLEDGE THE FORM HOLDS**: where a scenario DECLARES the ids a `band` /
## `herd` pick offers. Each is a walk through the document: a name descends a key, an array is
## entered implicitly, and the last name is the string read off every element reached.
const DECLARED_BAND_ID_PATH := ["factions", "bands", "id"]
const DECLARED_HERD_ID_PATH := ["herds", "id"]
## The schema's pick hint keys and values (`core_sim::scenario::scenario_schema`).
const SCHEMA_PICK_KEY := "x-pick"
const SCHEMA_PICK_TILE := "tile"
const SCHEMA_PICK_BAND := "band"
const SCHEMA_PICK_HERD := "herd"
const SCHEMA_PICK_TILE_KEYS := "x-pick-tile"

const SCENARIO_HEADING_FILE := "FILE"
const SCENARIO_HEADING_NEW := "NEW"
const SCENARIO_FILE_PLACEHOLDER := "— choose a file —"
const SCENARIO_NEW_PLACEHOLDER := "new_name"
const SCENARIO_REFRESH_LABEL := "Refresh"
const SCENARIO_NEW_LABEL := "New"
const SCENARIO_SAVE_LABEL := "Save"
const SCENARIO_LOAD_LABEL := "Load"
## What a switch-while-dirty button says on its first press. The second press discards.
const SCENARIO_DISCARD_LABEL := "Discard edits?"
const SCENARIO_NO_SERVICES := "No server attached — the file list, Save and Load need one."
const SCENARIO_NO_MAP := "No map attached — tile picks are unavailable."
const SCENARIO_NO_SCHEMA := "No schema yet — it arrives with the file list."
const SCENARIO_NO_FILE := "No file open. Choose one above, or create a new one."
const SCENARIO_UNREADABLE := "%s could not be read: %s"
const SCENARIO_BAD_NAME := "A file name is letters, digits, - and _ only."
const SCENARIO_NAME_EXISTS := "%s already exists."
## Said after `SCENARIO_NAME_EXISTS` when an opened export would save over a fixture of the same name.
const SCENARIO_RENAME_HINT := "Type another name above — Save stays off until it is free."
const SCENARIO_PICK_ARMED := "Click a hex on the map to fill the tile… (Esc cancels)"
const SCENARIO_CLEAN_STATUS := "%s — nothing unsaved."
const SCENARIO_DIRTY_STATUS := "%s — unsaved edits."
const SCENARIO_NEW_STATUS := "%s — new, not saved yet."
const SCENARIO_SAVED_LOG := "Scenario: saved %s."
const SCENARIO_LOADED_LOG := "Scenario: loaded %s — the world was rebuilt from it."
const SCENARIO_REFUSED_LOG := "Scenario: %s refused — %d problem(s)."
const SCENARIO_TRANSPORT_LOG := "Scenario: no answer from the server (%s)."
## Banner prefix for refusals that name no node in the form (`<file>: …`, `line L, column C`).
const SCENARIO_BANNER_HEADING := "The server refused %s:"

# ---- the schema-walked form -------------------------------------------------
const FORM_OPEN_GLYPH := "▾"
const FORM_CLOSED_GLYPH := "▸"
const FORM_REMOVE_GLYPH := "×"
const FORM_ADD_FORMAT := "+ %s"
const FORM_ADD_ROW_LABEL := "+ add"
const FORM_PICK_LABEL := "pick"
const FORM_UNSET := "unset"
const FORM_SET_LABEL := "set"
const FORM_UNDECLARED_SUFFIX := " (not declared)"
const FORM_EMPTY_STRING_FACE := "—"
const FORM_NOT_IN_SCHEMA := "not in schema"
const FORM_ITEM_FORMAT := "%s[%d]"
const FORM_VARIANT_TITLE_FORMAT := "%s  %s"
const FORM_COUNT_FORMAT := "%d"
const FORM_TILE_X_PLACEHOLDER := "x"
const FORM_TILE_Y_PLACEHOLDER := "y"
const FORM_MAP_KEY_PLACEHOLDER := "key"
## Geometry. A leaf row's key label wraps down to this width rather than widening the column.
const FORM_KEY_MIN_WIDTH := 48.0
const FORM_VALUE_MIN_WIDTH := 96.0
const FORM_TILE_FIELD_WIDTH := 52.0
const FORM_INDENT := 8
const FORM_BUTTON_FONT_SIZE := 12
const FORM_ROW_GAP := 4
const FORM_NOTE_GAP := 2
## SpinBox limits when the schema states none. The upper bound is 2^53-1, the largest integer a
## double (what a SpinBox and a parsed JSON number both are) holds exactly.
const FORM_FALLBACK_MIN := -9007199254740991.0
const FORM_FALLBACK_MAX := 9007199254740991.0
## A `number` field's step. Small enough that a typed value is not snapped away.
const FORM_FLOAT_STEP := 0.001
const FORM_INT_STEP := 1.0
## Schema nodes deeper than this are drawn as a bare caption, as the config tree does.
const FORM_MAX_DEPTH := 12
## Top-level blocks start open; everything deeper starts closed.
const FORM_OPEN_DEPTH := 0
## JSON text written to disk: two-space indent, and the trailing newline the checked-in files carry.
const SCENARIO_INDENT := "  "
const SCENARIO_TRAILING_NEWLINE := "\n"
