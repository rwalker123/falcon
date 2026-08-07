---
paths:
  - "clients/godot_thin_client/src/scripts/ui/workbench/**"
  - "clients/godot_thin_client/src/config/tuning_manifest.json"
  - "clients/godot_thin_client/tools/workbench_preview.gd"
  - "clients/godot_thin_client/tools/workbench_shell_budget.gd"
---

# The Workbench — the designer surface

The dev/designer surface that replaces the legacy Inspector, opened with `` ` ``. It reserves a
screen edge through the shared reservation registry (reserver id `&"workbench"`), draws in
`HudStyle`'s existing dark console language, and is built page by page — the Inspector keeps working
on `I` until its tabs have somewhere to go (`.claude/rules/client/inspector-panels.md` carries the
standing decision to retire it). Design spec: `docs/plan_config_tuning_panel.md`.

**Opening either dev surface closes the other**, and that is enforced rather than assumed. Both
reserve `SIDE_LEFT` at the same priority, so a pair left open reserves the sum (380 + 560) while only
the higher CanvasLayer draws — a strip of empty background beside the map, and an Inspector that is
invisible yet still holding its reservation. It goes away with the Inspector.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/workbench/WorkbenchShell.gd` | Coordinator ONLY: rail, content host, pinned footer region, page routing, edge reservation, snapshot fan-out with a hidden-surface gate. It never names or `preload`s a page |
| `ui/workbench/WorkbenchPages.gd` | The page registry — rows of `{id, title, subtitle, section, glyph, script}`. An empty `script` is a declared-but-unbuilt page and renders the placeholder |
| `ui/workbench/WorkbenchPage.gd` | The page contract: `build`, `build_actions`, `apply_update`, `reset` (world-scoped — see below), and the service hooks |
| `ui/workbench/WorkbenchWidgets.gd` | All-`static`, stateless shared drawing — surface/rail chrome, `build_group` (the sunk well: heading, hairline, the caller's own body threaded in), the banner, the parameter row, the number field, the modified dot |
| `ui/workbench/WorkbenchVocab.gd` | ALL-`const` labels, glyphs, geometry, font sizes. Zero funcs, zero vars |
| `ui/workbench/pages/ConfigTuningPage.gd` | Page one — the manifest-driven config tuning surface |
| `ui/workbench/pages/EquipmentPage.gd` | Page two — the TOE roster and each player band's live kit state, read side only. A second reader of `ui/hud/KitRoster.gd`, never a second copy of it |
| `src/config/tuning_manifest.json` | What is tunable: per config kind a `kind`/`label`/`env_var` and rows of `{pointer, label, type, min, max, step, default, unit?, hint}` |
| `tools/workbench_preview.gd` / `.tscn` | PNG preview harness for the surface (see `test-harnesses.md`) |
| `tools/workbench_shell_budget.gd` / `.tscn` | The decomposition guard — see "The shell cannot grow" |

## The shell cannot grow, and that is enforced

`Hud.gd` reached ~9,850 lines and `Inspector.gd` ~6,500, both one reasonable-looking method at a
time, and both needed a decomposition arc to undo. The Workbench is shaped so the same accretion has
nowhere to land:

**Adding a page is one registry row plus one file under `pages/`.** There is no shell edit in that
list. `WorkbenchShell` reads `script` out of a `WorkbenchPages.PAGES` row and instantiates it, so it
never names a page, never imports one, and has no reason to gain a line when the surface gains a
page. Everything else follows from that: a feature belongs to a **page**, shared drawing to
**`WorkbenchWidgets`** (state threaded in as PARAMETERS — a back-reference to the shell or a page
turns a shared layer into a second coordinator), and every label, glyph and number to
**`WorkbenchVocab`**. A page that grows gets its own sub-controllers under `pages/<id>/`; it does not
get more methods.

`tools/workbench_shell_budget.gd` fails on three things: the shell exceeding its line budget, the
shell naming a page, and a registry row whose `script` is missing or does not extend `WorkbenchPage`.
The middle one is the real invariant and the budget is its proxy. **When the budget trips, the answer
is to extract — to a page, a widget or the vocab — never to raise the number.** The third exists
because a typo'd path degrades *silently* to the placeholder: the rail entry still renders, the click
still works, and the page is simply never built.

## A page reaches its collaborators through named services, not the shell

A page never holds a handle back to `WorkbenchShell`. It is *given* a `StringName → Callable`
dictionary and reads entries by name; the shell stores it and hands it on, and does not know which
page uses which service. `Main` supplies three today (names are consts in `WorkbenchVocab`):
`send_command` `(String) -> bool`, `append_log` `(String) -> void`, and `new_game` `() -> void`.

**`append_log` lands on the event dock's System channel** (`R`), as a `HudEventVocab.KIND_SYSTEM`
event labelled `Workbench` — not as a command echo, which `HudEventVocab.IGNORED_KINDS` drops
outright. A page's status line reports a state change the surface made on the designer's behalf; the
command *receipts* `send_command` generates are the echoes, and they take the echo default.

**`send_command` answers `bool` so a page can tell "sent" from "there is no server"** — that
distinction is what lets the tuning page abandon an Apply *before* asking for a new game, instead of
starting a fresh world on overrides that never reached the server. `WorkbenchPage.service(name)`
always returns a `Callable` — an invalid one when the service is absent — so a page has exactly one
thing to check.

That indirection is what keeps the "no shell edit" property true for capabilities as well as pages: a
future page needing something new adds a service name at the `Main` end and reads it at the page end,
with nothing in between to change. **A page must degrade when a service is absent rather than
assume it** — the preview harness runs with no server at all, and a page that crashes without one
cannot be iterated on.

## The pinned footer is a region, not a feature

`WorkbenchPage.build_actions()` returns a `Control` or `null`, and the shell parents it **below the
scroll, outside it** — so a page's actions stay on screen however long its body is. The shell learns
nothing about what it parented. The hairline above the region hides for a page with no actions, so an
empty footer draws no chrome.

It is a separate method rather than a region of `build()` because the two live in different parents:
a page's body is scrolled content, its actions are chrome.

**A page is PARENTED before it is built.** `show_page` adds the page to the host and only then calls
`build()` / `build_actions()`, both once-only across reopens. The order is the contract because it is
what a page author will assume: a page that reaches `get_tree()`, `get_window()`, or an
ancestor-derived size or theme while orphaned gets null and engine defaults **with no error**, which
is a failure that shows up as a mis-laid-out page rather than as a crash.

## `reset()` means "drop what the WORLD gave you"

The shell calls `reset_pages()` from `Main`'s per-world reset, so a page's state does not outlive the
world it described. That is the whole scope of the hook: **snapshot-derived state**, not everything a
page holds.

`EquipmentPage` is the ordinary case and shows what the hook is for: its roster is the ended world's
config and its cohorts are that world's bands, so it drops both. What it buys is **the gap** — a
world boundary is not a frame, and between `reset_pages()` and the next snapshot a page that kept its
state would render the dead world's bands as though they were the new one's. The page re-seeds from
that next frame either way; the point is that it says nothing rather than something wrong in between.

`ConfigTuningPage.reset()` is therefore a **documented no-op**, and the exception is the load-bearing
part. Its state is the designer's intent and the server's staged file — neither belongs to the world,
and a New Game is precisely what `Apply` just asked for. Wiping the page there would blank the panel
one frame after the restart it requested, which is the "clean rows, staged server" divergence below
reached by a different road.

## Edited is not the same as sent

Each row tracks **two** values: what it reads now, and what the server was last told (`sent`, seeded
to the manifest default). One flag cannot express the states that matter, and collapsing them
produces a surface that contradicts the sim:

- edited but unsent → `Apply` is live
- sent and unchanged → `Apply` is dead, but `Revert all` must stay live, because the server is still
  holding a file
- **a row typed back to its default after being applied** — clean by every row-level test, while the
  override is still staged. With one flag the page reads "no overrides", disables both buttons, and
  `clear_config_overrides` — reachable only through `Revert all` — becomes unreachable while the next
  New Game still boots on the old value.

**The patch includes a row that is off its default OR whose `sent` value was** — the channel has no
"unset", so a returned-to-default row has to be written back **explicitly** or the server's deep
merge keeps the stale value. Rows never touched still stay out, which is what keeps the payload
sparse and the client's carried defaults safe.

`workbench_preview`'s `_assert_staged_survives_un_edit` pins both legs.

## The surface's width is an OFFSET, never a `size`

The shell is anchored `PRESET_LEFT_WIDE` — left and right anchors equal, top and bottom **not** — so
its height comes from the anchors and only its width is the surface's to set. `_emit_reserved_width`
sets `offset_right`, and the damage a `size` write does is **vertical**: `size` is a `Vector2`, so
`size.x = w` also writes the current — minimum-size-clamped — `size.y` back as an explicit bottom
offset, pinning a height the anchors are supposed to stretch. Horizontally the two agree, which is
what makes this easy to miss — `set_size` recomputes `offset_right` to the same number the offset
write sets, so the width looks right while the height quietly stops stretching.

`Control` warns on any `size` write under unequal opposite anchors ("will have their size overridden
after `_ready()`"), which `Main` trips because it hides the surface from its own `_ready`, before the
deferred callback that clears the warning runs. That warning was the only symptom.

`Main` and `workbench_preview` seed the surface through the same offset, so drag-resize, the
show/hide toggle and construction all move one number. `workbench_shell_budget` asserts the shell's
source carries no bare `size` write, because the failure is otherwise invisible: a revert leaves the
rendered width correct and every existing assertion green.

## A hidden Workbench ingests nothing

`update_snapshot` caches the newest frame **by reference** and skips the fan-out while the surface is
hidden, replaying on show — the same contract `Inspector` needed and got retrofitted
(`inspector-panels.md` → "A hidden Inspector does not render"), applied here from the start. Two
clauses carried over deliberately: the cache holds the frame by reference (deep-copying would cost
exactly the work the gate saves), and `_hidden_frame_pending` is discharged **only by an actual
fan-out**, never by a skip — clearing it on a skipped frame is what made the Inspector's first
version open on stale panels.

Anything that ACCUMULATES must stay above the gate. Nothing on the surface does today; the test for
a new one is not "is it cheap" but "is it reconstructible from the next full frame?"

**The fan-out is at the ACTIVE page only, so `show_page` catches the new one up from the cache.**
Without that, a page activated between two frames sits on its empty state until the next one — and
frames arrive on turn resolution and world-mutating commands, with no heartbeat, so "open the surface,
click a page" means *the rest of the turn*. `ConfigTuningPage` consumes no snapshot, so `EquipmentPage`
was the first page able to show the gap, and it showed it as a page reporting that the world had sent
nothing. The replay is the same shape as the visibility one — cached frame, `full_snapshot = true`,
skipped while hidden, and `_hidden_frame_pending` left alone — and it is **coordinator fan-out, not
page knowledge**, so the shell still names no page. `workbench_preview`'s
`_assert_equipment_catches_up_on_page_switch` pins it by feeding a frame while a *different* page is
active and then switching.

**A page's own ingest gate has to admit that replay**, which is what the "or I am holding nothing"
clause in `EquipmentPage.apply_update` is for — see the next section.

## Presence is not a change signal, on this surface either

**Every baseline key rides every merged delta.** The decoder builds a merged frame as a shallow
duplicate of the cached world and overwrites it with the delta's keys, so `snapshot.has(key)` is true
on every frame whether or not anything moved; `changed_sections` is the manifest, and it is **absent
on a full snapshot, meaning everything changed**. `SnapshotSections.gd` carries the general rule and
`Main` already gated the kit roster this way — the equipment page's first cut did not, and rebuilt
both of its groups every turn on a roster that changes only when the world does.

The gate a snapshot-reading page wants is therefore **"the frame carries it AND (the manifest says it
changed OR this page is holding nothing for it)"**. The second clause is not belt-and-braces: a
replayed cached frame (above) reports the roster unchanged, so a `changed`-only gate would leave a
freshly-switched-to page permanently empty, and the same clause is what re-seeds a page after
`reset()`. The four cases it has to cover are the first full snapshot, a steady delta, a
replay-or-post-reset, and a world rebuild.

## A row that does not fit swells the whole column, silently

Parameter labels have autowrap **off**, so a long one raises its row's minimum width; the
`ScrollContainer` grows its child to that minimum, and the content column widens past
`SURFACE_WIDTH` and draws over the map. It looks like a slightly wide panel, not a broken row —
which is why it is asserted rather than eyeballed: `workbench_preview` measures the content column
against `SURFACE_WIDTH - RAIL_WIDTH - 2 * CONTENT_PADDING` across every manifest row, and per row
checks for wrap, clipping, and disappearing under the scrollbar.

So **a new manifest row is not done until that assertion has run.** The lever when it trips is the
label text or `CONTROL_WIDTH` — the number fields hold at most six characters, so control width is
usually where the slack is.

**Every page owes the same measurement, and the column check is the part that transfers.** The
per-row checks above are shaped around a parameter row; `_assert_equipment_fits` is the equipment
page's own version, measuring the same content column and then walking every label the page draws
with autowrap **off**. On that page they are of three kinds, and the lever differs by kind:

- **A page's own content** — a kit's display name, a band's head row. These are deliberately the only
  two content lines that do not wrap, and **the lever is to put long text in a wrapping caption**,
  which is why every other line there is one.
- **The group headings `build_group` draws.** `WorkbenchWidgets.build_section_label` sets no
  `autowrap_mode`, so a heading is measured like anything else — and a caption is not an option for
  one. **The lever there is the heading text itself**, in `WorkbenchVocab`.

So the count the assertion prints is headings plus content lines, not content alone: a reader
matching it against the page's rows will be over by one per group.

## A new glyph must be RENDERED before it is trusted

The bundled font does not cover every symbol, and an uncovered one does not fail — it draws as a stub
a couple of pixels tall. That is unreadable precisely where the glyph is all there is: the
**collapsed rail**, where it is the only thing identifying an entry. `≡` (U+2261) and `⌁` (U+2301)
both shipped that way and were caught in `workbench_preview`'s collapsed-rail frame. Check a new one
there the same way.

## The equipment page quotes each tier at ITS OWN kit, and they are not the same kit

The page draws two sets of tiers a centimetre apart in identical units and formats — the roster's,
which are what a **fresh** kit grants, and each band's **resolved** ones — so everything about it is
arranged to keep a number from being read against the wrong thing. Three of those arrangements are
load-bearing rather than tidy:

**`PopulationCohortState.kitId` answers for the HUNT tiers only.** For a resident band it is the hunt
job's default; the forage tier resolves through the world's forage default, which rides the wire once
as `SubsistenceSection.defaultForageKitId`. Quoting the gather rate under `kitId` reads a gathering
number off `big_game`, which has no basket component at all — a plausible row that is simply wrong,
and one the sim already refuses to publish (the `kit_selection` test
`a_resident_bands_published_kit_answers_for_the_hunt_tiers_only`).
**An in-flight party is the exception**: it carries one kit, decided at
launch, so that kit covers its forage tier too and `kitId` is the honest answer for both lines. Both
directions are asserted (`_assert_equipment_quotes_each_tier_at_its_own_kit`), and the fixture's hunt
and forage defaults are **different kits** on purpose — with one default for both jobs, the fix and
the bug render the same sentence.

**A cohort's tier keys are NOT the roster's**, and only two thirds of that is obvious. Both carry
axes happen to share a spelling with the roster's keys; the attack axis does not — a kit publishes
`attack`, a cohort publishes `hunterAttack` — so reading all three through `KitRoster`'s keys renders
a whole, plausible line in which the attack is the `0.0` of a missing key, drawn beside two correct
numbers. That shipped for exactly one render and a screenshot caught it. The page spells its three
cohort keys out rather than borrowing them on the strength of two coinciding, and
`_assert_equipment_states_the_bands_own_tiers` pins the rendered numbers against a **worn** fixture
band — a fresh band's resolved tiers equal its kit's, so the assertion would prove nothing on one.

**The two carry tiers are separate fields on the wire and stay separate here.** A band can be out of
baskets with its sled untouched, and rendering one on the other web's row is the defect the sim-side
split corrected.

The page is bounded to the **player's** cohorts (a rival's bands render identically, so the filter is
asserted rather than eyeballed), and its only non-wrapping labels are a kit's display name and a
band's head row — every other line is a wrapping caption, which is what keeps the content column from
swelling. `HerdTelemetryState.huntTripEstimatesKitId` / `denialEstimatesKitId` are **not** consumed
here; they exist so a client can refuse to present an estimate table as an answer for a kit it was not
computed at, which is the compose sheets' job, not this page's.

## The tuning manifest is curated, and its patches are sparse

The manifest is a **hand-picked list of levers, not a generic JSON tree editor**, for two reasons.
The command channel is one-way — `CommandBridge.send_line` answers transport success, not a server
reply — so the client cannot ask what the current values are and must carry its own copy. And a
designer looking for `regrowth_rate` should not have to scroll 33 flora species to reach it.

**A parameter the designer did not touch never appears in the payload.** That is what makes the
carried copy safe: the server's real values always win, so manifest drift can only ever stale a
*displayed hint*, never the running simulation. The drift is still caught — one manifest, two
readers: a `core_sim` test resolves every pointer against the shipped config and asserts the declared
`default` **equals** the shipped value, so a retuned config or a renamed key fails CI instead of
quietly rendering a wrong hint.

The restart-scoped contract is stated **on the surface** (the standing banner), not only in a doc:
someone who edits a value and watches the running world not change must not have to read this file to
find out why.
