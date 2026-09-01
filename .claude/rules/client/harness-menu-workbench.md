---
paths:
  # Script AND scene for each, for the reason `harness-headless-guards.md` states: the `.tscn` is
  # what an invocation names, and this file's own headings are written `<script>.gd` / `.tscn`.
  - "clients/godot_thin_client/tools/menu_preview.gd"
  - "clients/godot_thin_client/tools/menu_preview.tscn"
  - "clients/godot_thin_client/tools/workbench_preview.gd"
  - "clients/godot_thin_client/tools/workbench_preview.tscn"
  - "clients/godot_thin_client/tools/workbench_shell_budget.gd"
  - "clients/godot_thin_client/tools/workbench_shell_budget.tscn"
---

<!-- Split out of .claude/rules/client/test-harnesses.md, which was itself extracted from
     clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e.
     The pseudo-table cells this file carries were re-wrapped at 100 columns; no wording changed. -->

# The `menu_preview` and `workbench_preview` harnesses

The shell harnesses: the shared `MenuShell` and the workbench plus its budget gate.

## `tools/menu_preview.gd` / `.tscn`

Dev-only preview harness for the shared **`MenuShell`** (landing + pause), the menu twin of
`ui_preview`: instances the real `MenuShell.tscn` over a flat ground/scrim ColorRect, walks it
through its states and dumps `menu_landing.png` / `menu_pause.png` / **`menu_options.png`**, the
theme dropdown open (`menu_options_theme_popup.png`) and the Theme row's CHANGED state in both modes
(`menu_options_theme_pending_pause.png` / `..._landing.png`) to `ui_preview_out/`. No server, no network. The **Options** state drives `_activate_item("options")` —
the same entry point the nav rail calls — so the pane is built exactly as a click builds it, and it
is rendered from PAUSE mode only because the shared `ITEMS` registry gives the landing menu the
identical pane. It is the frame the client-settings rows are judged on (the **Fog of war** toggle +
the two speed sliders + Restore defaults), and it reads whatever is in the real
`user://client_settings.cfg` — the slider readouts vary by machine, so judge the ROWS, not the
values.

**The interface scale and BOTH halves of the theme are pinned, by MEMBER assignment.** The Theme row
compares the saved pick against the applied palette, so pinning only `HudPalette.apply()` left the
row rendering the developer's own choice: on a machine saved to a non-default theme every Options
frame came out PENDING — not-applied caption, Apply button — and the settled state had no frame at all.
`ClientSettings.theme` is therefore pinned beside it, and the pending frames set it explicitly. Never
through a `ClientSettings.set_*` setter: those `_save` over the developer's real config, which is the
same contamination one step worse. The pending pair also carries the button's two ASSERTIONS —
visible when the pick differs, absent when it does not — because a frame that quietly lost the button
still looks like a deliberate layout. **Nothing here ever presses it**: it relaunches the process.

**It also drives the SAVE CHANNEL, and it is the transport itself.** `_run_saves_states` injects a
real `SaveSlots` seam whose sender only records the request id, then answers it with canned dicts
shaped exactly as `bridge/query.rs` composes them — so the seam's real decode, routing and formatting
run, with no server. That is what makes the failure states renderable at all: **no saves yet**
(`menu_load_empty`), **no server** (`menu_load_no_server`) and a **refused slot name**
(`menu_save_reserved_name`) are unreachable from a healthy stack, and they are precisely the states a
player meets first. The set is `menu_load_list` / `_selected` / `_delete_confirm` / `_in_run`,
`menu_save` / `_overwrite` / `_reserved_name`, `menu_load_empty` / `_no_server`, and `config_drift`
— the last of which is not the shell at all but `ConfigDriftNotice`, added to the harness root,
because this is the only harness that can see it.

Three assertions ride with them: a delivered answer must leave the seam `LIST_READY` and a refusal
`LIST_FAILED` (a pane that renders its own placeholder either way looks identical in a PNG), and **the
armed delete button must NAME the slot it would destroy** — that label IS the confirmation, so a
generic "Confirm" would quietly remove the only thing between a click and a lost save. The fixture's
four rows cover both size units and all three time buckets plus the absolute-date branch; the ages are
expressed as OFFSETS from now (a fixed stamp would drift into another bucket as the branch aged) and
only the date row carries a fixed stamp, since that branch has no bucket to drift out of.

**One check takes no picture at all.** `_assert_text_focus_is_handed_back` covers the keyboard
handover behind the client's two polled-input guards
(`.claude/rules/client/polled-input-focus.md`): typing must leave the
Save pane's name field holding focus; `release_text_focus`, a pane switch and a submit must each hand
it back; and a focused **Button** must NOT read as text entry, since widening the predicate would
kill WASD and every panel toggle after each click on a HUD control. The POSITIVE legs are what keep
the negative ones honest — an assertion that focus was released passes trivially if focus was never
taken, and the Button leg first asserts the probe really took focus.

**It calls the SHIPPED predicate** (`TextEntryFocus.held_in`) rather than restating the expression, so
a drift in it fails here. Neither `MapView` nor `Main` is instantiated in this harness, so neither
`_process` runs and the suppression itself — that a guarded hotkey does not fire — is NOT tested;
what is tested is the predicate those two branch on, against a really focused field.

Sabotage-verified three ways: widening the predicate to `is Control` fails the Button leg; making it
always false fails all three focus-is-taken legs; stubbing `release_text_focus` fails all three
release legs, which is also the proof that neither `queue_free` nor hiding the layer releases focus on
its own.

Same `_settle` (`process_frame` → `force_draw` → `process_frame`) + `_save` contract as
`ui_preview`, and the same rule: `scripts/preview.sh res://tools/menu_preview.tscn`, **NOT
`--headless`** (the dummy renderer yields a null viewport texture and the frames are skipped with a
warning)

## `tools/workbench_preview.gd` / `.tscn`

Dev-only preview harness for the **Workbench**, the designer surface
(`.claude/rules/client/workbench.md`): builds the real `WorkbenchShell` in code over a map tone,
renders the Config Tuning page, its unapplied-edit and applied-override states, the collapsed rail,
an unbuilt page's placeholder and **both equipment-config pages, each empty and fixtured**, and
dumps nine PNGs to `ui_preview_out/`.

**It carries ten ASSERTIONS, not just frames.** The tuning trio: `_assert_rows_fit` measures the
content column against `SURFACE_WIDTH - RAIL_WIDTH - 2 * CONTENT_PADDING` across every manifest row
plus per-row wrap/clip/under-the-scrollbar checks — a label too long for its row does not fail, it
silently widens the whole column over the map (see workbench.md → "A row that does not fit swells
the whole column"); a second asserts an Apply payload is **sparse** (a parameter never touched must
not appear in the patch); and `_assert_staged_survives_un_edit` has two legs pinning workbench.md →
"Edited is not the same as sent".

**The config-page SEVEN.** `_assert_equipment_fits` is the same column measurement plus every
autowrap-OFF label — which is the group HEADINGS plus the tree's BLOCK NAMES, not its rows — and it
takes the PAGE as a parameter and is asked of both, the roster and the gear blocks nesting
differently and reaching different widths.

**`_assert_the_pages_print_a_config_no_script_names` is the one the whole design rests on.** The
fixture carries a field inside a real gear block (`wear_per_turn_carried`), a whole top-level block
(`windbreak_kit`) and a field inside a ROSTER ENTRY (`morale_bonus`) that **no shipped GDScript
names**, and all three must render with their own values; a page walking a hand-written list of
fields would draw the three real keys of `hunting_kit` and silently drop the fourth, which looks
entirely correct in every frame. The kit one carries its own weight: the other two both live on the
EQUIPMENT page, so before it existed a `KitsPage` simplified down to a fixed jobs+uses body — which
its title promotion makes tempting — would have passed every assertion in this file. A scan of the
four shipped scripts rides with them, confirming none of the strings appears in any of them.

**`_assert_the_kits_page_titles_each_entry_by_its_own_name`** covers the promotion itself
(workbench.md → "The ONE exception"): all four title cases are FIXTURED — both keys, `id` only,
`display_name` only, and an ANONYMOUS entry carried for no reason but to keep the `kits[N]` fallback
REACHABLE, an unreachable branch reading exactly like a covered one — and each asserts that the rows
the title CONSUMED left the body while the rows it could not use stayed, plus that `jobs`/`uses`
survive on every block. Its guard checks each fixture entry really IS the shape its title stands
for, or the branch it claims to cover is untested. `_assert_the_pages_partition_the_config` is its
other half — the roster renders on Kits and NOT on Equipment, a gear block on Equipment and NOT on
Kits, and the roster's `uses` rows carry both array shapes (comma-joined on one row; an explicit `—`
for the bare kit's empty one). No picture can carry either claim: each page renders a plausible tree
of config keys, and which page is a fact about the OTHER one. `_assert_equipment_drops_the_world` is
driven through the SHELL's `reset_pages()`, the way `Main`'s per-world reset reaches it, and asserts
BOTH pages — the Kits page is not even on screen when it runs, which is precisely the case that
fan-out exists for. These are the pages whose `reset()` is REAL, the counter-case to
`ConfigTuningPage`'s documented no-op, and a doc claim with no test is how the two get confused.
`_assert_equipment_catches_up_on_page_switch` covers **a page activated between frames**: the
fan-out is at the ACTIVE page only, so a frame is pushed while a DIFFERENT page is showing and the
switch must catch the page up from the shell's cache (workbench.md → "A hidden Workbench ingests
nothing"). No picture can carry it either — a page that was never fed and a page fed an empty world
render the same degraded line.

**The last two are ordered and the order is the precondition** — the reset empties both pages, which
is exactly the state the catch-up assertion needs, and nothing may read them after it.

**Two harness rules the config fixture depends on.** The pages are found by TYPE **while on screen
and then REMEMBERED**: the shell DETACHES the page it is not showing, so a plain tree search finds
only the active one, and two of the claims above are about a page that is not active. And
`JSON.stringify(config, "", false)` turns `sort_keys` OFF — it defaults to TRUE, which would hand
the pages an alphabetised config no server can produce, since `serde_json` writes a struct in its
declared field order and the pages render in the order they are given. Reading a row's value is
likewise POSITIONAL (the key is the row's first Label, the value its second): a value is often a
string some other row uses as a KEY — the fixture's `kits[0].jobs` renders the value `hunt` while
`default_kits` renders the key `hunt` — so "find the Label reading `hunt`, then its sibling" answers
`jobs` and asserts the opposite of what it was asked. The generic-render assertion is
sabotage-verified: an allow-list of field names in the renderer fails it ALONE, naming
`hunting_kit.wear_per_turn_carried` and the value it could not find, with the other eight assertions
green.

**The frame set is deterministic run-to-run**, but every frame contains the rail, so **adding a
registry row moves all of them**; judge a page change by cropping the content column instead. It
runs with **no server and no command transport**, which is also the degradation path a page must
survive — hence `workbench_equipment_empty` / `workbench_kits_empty`, rendered before the fixture

## `tools/workbench_shell_budget.gd` / `.tscn`

Headless **decomposition guard for `WorkbenchShell.gd`** — the thing that was missing when `Hud.gd`
reached 9,850 lines and `Inspector.gd` 6,500. Four assertions: the shell is within its line budget;
the shell **names no page** (no `pages/` path, no page `class_name` — the real structural invariant,
of which the budget is only a proxy); every `WorkbenchPages.PAGES` row with a non-empty `script`
resolves to a file that exists and extends `WorkbenchPage`, since a typo'd path degrades
**silently** to the placeholder; and the shell **never writes its own `size`** (statement-start
`size` / `size.x` / `self.size.y` assignment, comment lines skipped so the explanation of the rule
does not trip it) — the surface's width is an `offset_right`, and a `size` write both trips Godot's
unequal-opposite-anchors warning and pins a height the anchors should stretch.

**When the budget trips the answer is to extract — to a page, a widget or the vocab — never to raise
the number**
