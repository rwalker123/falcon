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

# ---- equipment page --------------------------------------------------------
const EQUIPMENT_PAGE_TITLE := "Equipment"
const EQUIPMENT_PAGE_SUBTITLE := "TOE roster and the band's live kit state"
## The one thing a reader of this page has to hold on to: the roster's numbers are a FRESH kit's, the
## band's are what it resolves to now. Said on the surface because the two sets of tiers sit a
## centimetre apart and are otherwise indistinguishable — same axes, same units, same formats.
const EQUIPMENT_BANNER := "Roster tiers are what a FRESH kit grants. A band's own tiers are what it "\
	+ "resolves to after wear — a dry component drops that role to the bare-handed tier and stays there."
const EQUIPMENT_ROSTER_HEADING := "KIT ROSTER"
const EQUIPMENT_BANDS_HEADING := "BAND KIT STATE"

## What separates the clauses of a caption line. A mid dot with air either side: the clauses are
## separate statements about one subject, not a sentence.
const EQUIPMENT_PART_SEPARATOR := "  ·  "
## Decimals on a tier. **A DOWNWARD ALIAS of the HUD's own answer**, not a second decision — the tiers
## span 1.0 to 40.0 and are authored as small round numbers, so the compose sheet and this page must
## not be able to disagree about how many places state one. (`hud-modules.md` records the alias idiom;
## `HudComposeVocab` reads nothing, so there is no const cycle to make.)
const EQUIPMENT_TIER_DECIMALS := HudComposeVocab.KIT_TIER_DECIMALS

## The roster group's first line: the BARE-HANDED tier on each axis, read off the roster itself.
## Every kit publishes the unequipped tier on each axis it does not use, so the minimum across the
## roster IS that axis's bare-handed number — no second copy of the TOE table lives here.
const EQUIPMENT_BARE_LABEL := "bare-handed"
const EQUIPMENT_ATTACK_FORMAT := "attack %s"
## An axis NO kit in the roster states — `KitRoster.unequipped_tier` answers `INF` there, and the
## page renders that rather than substituting a number the sim never sent.
const EQUIPMENT_TIER_UNSTATED := "—"
const EQUIPMENT_HUNT_CARRY_FORMAT := "hunt carry %s"
const EQUIPMENT_FORAGE_CARRY_FORMAT := "forage carry %s"

## A roster entry's identity line: its id, the verbs it may be sent on, and whether a job defaults to
## it. The id rides a CAPTION rather than the name row because a caption wraps and a row label does
## not — see "A row that does not fit swells the whole column" in the rule.
const EQUIPMENT_JOBS_FORMAT := "jobs %s"
const EQUIPMENT_JOBS_SEPARATOR := ", "
const EQUIPMENT_NO_JOBS := "no jobs"
const EQUIPMENT_HUNT_DEFAULT_TAG := "hunt default"
const EQUIPMENT_FORAGE_DEFAULT_TAG := "forage default"
## Which components a kit actually consumes — the axes on which it beats the bare-handed tier. It is
## a DISPLAY answer only: `none` spends nothing, and saying so is the point of the line.
const EQUIPMENT_CONSUMES_FORMAT := "consumes %s"
const EQUIPMENT_CONSUMES_NOTHING := "consumes nothing"

## A band's head row and the three component conditions beneath it. The condition formats are
## downward aliases of the compose sheet's, for the same reason as the decimals above: one wording for
## one fact. Condition is on `equipment.json`'s 0-100 scale and **performance is FLAT until expiry**,
## so nothing on this page scales a tier by what is left.
const EQUIPMENT_BAND_HEAD_FORMAT := "Band #%d"
const EQUIPMENT_PARTY_HEAD_FORMAT := "Party #%d"
const EQUIPMENT_BAND_SIZE_FORMAT := "%d people"
const EQUIPMENT_CONDITION_FORMAT := HudComposeVocab.KIT_HINT_CONDITION_FORMAT
const EQUIPMENT_CONDITION_DRY_FORMAT := HudComposeVocab.KIT_HINT_DRY_FORMAT
const EQUIPMENT_COMPONENT_SPEARS := HudComposeVocab.KIT_COMPONENT_SPEARS
const EQUIPMENT_COMPONENT_SLED := HudComposeVocab.KIT_COMPONENT_SLED
const EQUIPMENT_COMPONENT_BASKETS := HudComposeVocab.KIT_COMPONENT_BASKETS

## **THE TWO TIER LINES, AND THEY NAME THEIR KITS SEPARATELY.** `PopulationCohortState.kitId` answers
## for the HUNT tiers; a resident band's forage tier resolves through the world's forage default, so
## quoting it under `kitId` reads a gathering rate off a kit with no basket component at all. The
## suffixes exist so the page SAYS which kit each line is quoted at rather than leaving the reader to
## assume they share one.
const EQUIPMENT_HUNT_TIER_FORMAT := "hunt — attack %s  ·  carry %s per hunter"
const EQUIPMENT_FORAGE_TIER_FORMAT := "forage — carry %s per gatherer"
const EQUIPMENT_QUOTED_AT_FORMAT := "  ·  at %s"
const EQUIPMENT_QUOTED_HUNT_DEFAULT := " (the band's hunt default)"
const EQUIPMENT_QUOTED_FORAGE_DEFAULT := " (the world's forage default)"
## An in-flight party carries ONE kit, decided at launch, so it covers that party's forage tier too —
## the single case in which both lines honestly name the same id.
const EQUIPMENT_QUOTED_PARTY_KIT := " (the party's own kit)"

## Handles the preview harness reaches the two tier lines by. **Identity, never face** — both lines
## carry live numbers and a kit's display name, so a text search finds either or neither. The meta's
## VALUE is the cohort's entity id, so an assertion can say which band it is talking about.
const EQUIPMENT_HUNT_TIER_META := "workbench_equipment_hunt_tier"
const EQUIPMENT_FORAGE_TIER_META := "workbench_equipment_forage_tier"

## The per-assignment line: what each crew's yields are priced at, already resolved. `""` is a
## band-wide role (scout / warrior) — no kit component consumed, hence no kit axis, which is a
## different statement from "no kit".
const EQUIPMENT_CREWS_PREFIX := "crews  ·  "
const EQUIPMENT_CREW_FORMAT := "%s → %s"
const EQUIPMENT_CREW_NO_KIT := "no kit axis"
const EQUIPMENT_NO_CREWS := "no assignments"

## The degraded states. The page must come up with no server at all (the preview harness runs without
## one), so each group says what is missing rather than rendering an empty well.
const EQUIPMENT_NO_ROSTER := "No kit roster yet — it arrives with the first snapshot of a world."
const EQUIPMENT_NO_BANDS := "No player bands on the wire yet."

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
