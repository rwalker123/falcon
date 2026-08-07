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
