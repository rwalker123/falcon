---
paths:
  - "clients/godot_thin_client/tools/**"
  - "clients/godot_thin_client/tests/**"
  # The two scripts the harnesses are RUN by. `preview.sh` is the wrapper that gives a render
  # harness its quiet window, and `run_stack.sh` carries the third leg of the stranded-override
  # defense — both are specified in "The harness window is quiet, the GAME's is not" below, and
  # without these globs that section never loads for the code it describes.
  - "scripts/preview.sh"
  - "scripts/run_stack.sh"
---

<!-- Extracted verbatim from lines 214-219 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Headless verification harnesses (tools/)

The shared contract every harness obeys. The per-harness rationale lives in the `harness-*.md` files routed below, each gated to the harness it describes.

## Where the per-harness rationale lives

Each harness owns its own rule file, gated with `paths:` so it loads only when you touch that
harness. **A tally, a new state or a new assertion goes in the harness's own file** — that is what
keeps two worktrees adding states to different harnesses off the same file.

| Rule file | Covers | Loads when you touch |
|---|---|---|
| `harness-ui-preview.md` | The HUD PNG walk, its chapters, its frame/`PASS` tally | `ui_preview.gd`, `ui_preview/**` |
| `harness-band-panel.md` | The Band/City panel walk, the denial-raid and recall arcs, and `command_guard`'s shared kit roster | `band_panel_preview.gd`, `command_guard.gd`, `ui_preview/chapters/band_expedition.gd` |
| `harness-map-probes.md` | `map_preview` marker states, `blend_probe` edge blending | `map_preview.gd`, `blend_probe.gd` |
| `harness-menu-workbench.md` | `MenuShell`, the workbench, the shell budget gate | `menu_preview.gd`, `workbench_*.gd` |
| `harness-headless-guards.md` | The `--headless` decode/field/alias guards | `decode_guard.gd` + the five other `tools/*_guard.gd`/`.tscn` pairs it lists |

## `tools/preview_watchdog.gd`

**The hang guard for the PNG preview harnesses** — a `Watchdog` sibling node in `ui_preview.tscn`
and `band_panel_preview.tscn`. Both harnesses render their whole walk from one long `await`ing
`_ready()` whose last line is `get_tree().quit()`, so **any** runtime error in it aborts the run
without ever exiting, and a coroutine awaiting the aborted one is never resumed either: the process
idles forever, having stopped writing PNGs long before, leaving a stale partial frame set that looks
like a completed run (measured once at 59 minutes).

**It lives OUTSIDE the harness script because the failure that prompted it killed the harness
script** — a `preload`ed chapter with a parse error took `ui_preview.gd`'s own parse down, so the
root node came up scriptless and nothing it could have done internally would have run; a sibling
node's `_ready` still runs there. It **preloads nothing** for the same reason — a guard that can
fail to compile alongside the thing it guards is not a guard. It is a PROGRESS timer, not a
deadline: the harness calls `note_progress()` from `_settle` (which every state reaches, including
the PNG-less assertion blocks) and as each chapter starts, so `PROGRESS_STALL_LIMIT_MSEC` (180 s)
bounds the gap between two signs of life rather than the run, which grows with every state added.

**Wall clock, never `delta`** — these harnesses freeze `Engine.time_scale` at 0, so a `delta`-driven
timer would never expire, and neither would a default `SceneTree` timer. On firing it prints the
harnesses' own `FAIL` token and quits non-zero; `_finish()` disarms it so a slow shutdown is not a
stall. Verified by sabotage in both directions: a parse error in a file `ui_preview.gd` `preload`s
(i.e. the harness script itself dead) is killed at 181 s with `FAIL watchdog` and exit 1, and a
clean run is untouched — 274/274 frames byte-identical

## The exit status IS the verdict

The six render harnesses — `ui_preview`, `band_panel_preview`, `workbench_preview`, `map_preview`,
`blend_probe`, `menu_preview` — share one failure contract, in two halves:

- **ONE sink.** `_fail(message)` increments `_failures` and prints the harness's `FAIL` line, and it
  holds the file's only `push_error`. A second reporting path is a failure the tally cannot see, so
  the sink's whole value is that `_failures` cannot drift from what was printed.
- **ONE exit.** `_finish()` prints the run's summary and quits
  `EXIT_FAILED if _failures > 0 else EXIT_OK`. Every path that ends a run comes through it, so the
  status is derived in exactly one place and the hang guard is disarmed there (a slow shutdown is not
  a stall).

**So the check is `$?`, and a green log is not evidence.** The `assert OK` / `PASS` tallies recorded
against each harness in its own `harness-*.md` answer a different question — whether an assertion
was LOST — and they stay for that; they are not how a run is judged clean.

**The token is `<name>: FAIL — <text>`, spelled identically in all six sinks.** The `ui_preview`
categories (`hud — `, `turn-orb — `, `chapter — `, `herd fields — `) keep a separator of their own,
which is why one of its failures reads `ui_preview: FAIL — hud — <label>`: the first dash belongs to
the token, the second to the category. They are not redundant — the category mirrors the
`PASS <category> — ` line it fails against, so a category's passes and failures stay greppable as a
pair.

**But the sinks are not the only reporter, which is the second reason not to grep.** The hang guard
prints `<name>: FAIL watchdog — ` and quits 1 on its own, from outside the harness script (see
"`tools/preview_watchdog.gd`" above) — so a scanner keyed to `: FAIL — ` reads a 180 s stall as a clean run,
while `$?` reads it correctly. Any reporter that has to live outside a `_fail` sink is in the same
position, and the status is what covers them all.

**A condition that fails only because there is no renderer is a `push_warning` and a skip, never a
counted failure.** Under `--headless` Godot selects the dummy renderer: the viewport hands back a
null image and the window never holds a pinned canvas. Those are facts about the driver, not about
the code under test, so `_capture`, `_stabilize_canvas` / `_pin_window` and the one chapter that
saves outside `_save` all warn and skip there rather than reaching `_fail`. This is what keeps the
`--headless` compile check meaningful — `harness-ui-preview.md` → "The `--headless` run is a
COMPILE check": on a clean tree it still exits 0.

**Godot prints `ERROR:` lines on a PASSING run.** Shutdown reports `N resources still in use at exit`
and `RID allocations … leaked at exit` after the harness's own summary, so counting `ERROR:` lines
answers "the engine tore down" rather than "an assertion failed" — which is exactly why the status,
not a grep, is the verdict.

**A sink that exists is not a sink everything uses.** `ui_preview` held `_fail` and `_finish` for a
whole arc with three of its own failures still reporting *around* them — two canvas-drift reports and
a failed `_save` — each a bare `push_error` that printed loudly and counted for nothing. A
`push_error` written beside the sink is the one way to reopen the gap, which is why the sink holds
the file's only one.

**`event_dock_ultrawide` is the ONE frame written outside `_save`** (`chapters/event_dock.gd`): it
wants the ultrawide canvas that state has just pinned, and `_save` captures against the PINNED canvas
and would reject that very frame, so the save's error handling is restated inline rather than
inherited — a null image and a failed write both go through `h._fail`. The arithmetic of a whole-run
sabotage is what shows the two paths agree: inverting the `err != OK` test on both reports **275
`FAIL` lines against 275 frames**, not 274.

**`command_guard` and `turn_orb_click_probe` aggregate their own verdict and quit `0`/`1`**, as does
every headless guard in `harness-headless-guards.md` (`decode_guard`, `stream_frame_guard`, `marker_field_guard`,
`snapshot_alias_guard`, `party_removal_guard`, `inspector_hidden_guard`, `workbench_shell_budget`).
They sit outside the `_fail`/`_finish` shape and are correct that way — a gate whose entire output is
one verdict has nothing for a tally to add.

## The harness window is quiet, the GAME's is not

**Run a render harness through `scripts/preview.sh`, not bare `godot`** — from the **repo root**,
which is a change from the old `godot --path .` form those commands used to take (that `.` meant the
CLIENT directory). The wrapper `cd`s to the root itself, but the path you type to reach it does not.

```bash
godot --headless --path clients/godot_thin_client --import   # only if scenes/scripts changed
scripts/preview.sh res://tools/ui_preview.tscn
```

**The HEADLESS guards are unaffected and still run bare** (`decode_guard`, `stream_frame_guard`,
`marker_field_guard`, `snapshot_alias_guard`, `party_removal_guard`, `inspector_hidden_guard`,
`turn_orb_click_probe`): `--headless` opens no window, so there is no focus to take and nothing for
the wrapper to do. Sending one through it would only cost it the dummy renderer it wants.

Every harness that captures pixels must run **windowed**: `--headless` selects the headless display
driver, whose only rendering driver is `dummy`, so there is no viewport texture to read back. So
`ui_preview`, `map_preview`, `blend_probe`, `band_panel_preview`, `menu_preview` and
`workbench_preview` each open a real window — and it is `project.godot`'s, which is the PLAYER's:
fullscreen, and focus-grabbing. A repo worked by several worktrees at once opens one of those every
verification pass, in every session, all day.

Measured on macOS with Godot 4.7, probing `DisplayServer.window_is_focused()` from inside the run:
the bare-`godot` harness window takes the keyboard (`IS_FOCUSED=true`), and on exit macOS leaves
**`loginwindow`** frontmost rather than returning focus to the app that had it — so each run costs a
click in whatever *other* session the human was typing in. The wrapper drops an `override.cfg`
carrying `window/size/mode=0` + `window/size/no_focus=true` for the duration of the run, and under
it the window is never key and focus returns to the previous app.

**THE GAME IS DELIBERATELY UNTOUCHED, and that is the whole shape of this.** The obvious fix is to
make the project's own default quiet and have the boot scene promote itself back — and it works, but
the player then watches a windowed frame appear and expand into fullscreen on every launch. Nothing
about the game had to change to fix a harness problem, so nothing does: `project.godot` still boots
`mode=3` straight to fullscreen, and only a harness run overrides it.

**It cannot be a command-line flag, which is the first thing to try and the first thing to fail.**
`-w`, `--position` and `--resolution` are *ignored* when the project declares a fullscreen mode —
`window_get_mode()` still comes back `3`, with the flags placed either side of `--path`. A CUSTOM
flag read through `OS.get_cmdline_user_args()` is worse: it is read after the window already exists,
so the fullscreen window has appeared and taken focus before any script can object. `override.cfg`
is the only per-project-directory config Godot reads, so the wrapper is the mechanism, not a
preference.

**The stranded-override failure is the one to know about.** An `override.cfg` left behind boots the
GAME windowed and *unfocusable* — an app you cannot click into, with nothing on screen saying why.
Three things stop it: the wrapper traps `EXIT INT TERM HUP`, only the run that CREATED the file
removes it (so concurrent harnesses in one worktree do not pull it from under each other), and
`run_stack.sh` clears any override carrying the wrapper's marker line before it launches the client.
The file is gitignored. A wrapper run that finds an override it did not write REFUSES rather than
clobbering it.

Three things the quiet window does not cost, each checked rather than assumed:
- **The frames are identical.** A full `ui_preview` walk under the override came back
  **269/269 PNGs byte-identical** to the run before it, still at the pinned 1500x900.
- **The deliberate maximize survives it.** `_stabilize_canvas` takes a `MODE_MAXIMIZED` on purpose
  and pins back; `IS_FOCUSED` stays false at startup, through that maximize, and after the pin-back.
- **The click probes survive it.** `band_panel_preview` pushes its presses through
  `get_viewport().push_input()`, which is software-side and indifferent to OS focus.

**Minimizing is not the stronger version of it.** A window put in `WINDOW_MODE_MINIMIZED` stops
posting draws, so the first `RenderingServer.frame_post_draw` await never returns and the run hangs.
The window is still VISIBLE for its few seconds; it simply never takes the keyboard.

**The wrapper moves NO frames, including `menu_preview`'s.** That harness is the one that pins a
window size without also pinning `Window.MODE_WINDOWED`, so it looks like the one the boot mode
should reach — and it is not: `get_window().size` takes effect from a fullscreen boot too. Measured
A/B in one worktree, wrapper against bare `godot`: `menu_preview` 3/3 frames identical at 1500x900
both ways, `workbench_preview` 9/9 identical. Do not re-derive that from the window mode; it was
inferred once and was wrong.

## An assertion asks a CONTROL, not the subtree

Three assertions in these harnesses were found passing for the wrong reason, and all three shared one
shape: they searched BROADLY where a specific node was meant. `improvement_done_plant` claimed the next
rung's checkbox sat beneath the done label on a fixture whose ground is `too_dry` and can never take
seed — asserting the gated shape while naming the offered one. `two_meter_split` searched the whole
sheet for "Penning" and matched the Sustain hint's craft clause, not the gate reason it named. And an
abandon test passed under sabotage because its fixture sat in the one state the gate exempts.

**The rule the sweep left behind:**

- **Reach the control by IDENTITY, never by face** — `ForageFx.find_improvement_control` (by
  `HudWidgets.IMPROVEMENT_CONTROL_META`), `Q.find_policy_rung` (by `HudWidgets.POLICY_RUNG_META`),
  `Readout.stepper_value` (structural: the value Label beside a stepper's `−`). A face
  carries live numbers and has already been restyled twice; a text match on one quietly finds nothing
  and passes. **Add a finder rather than widening a search.** A finder used by one chapter is a method
  on that chapter (`_policy_rung_metric` is `chapters/forage_accounts.gd`'s and reachable from nowhere
  else); one used by two belongs in `node_query.gd` / `readouts.gd` as a `static func`.
- **A whole-subtree text search is legitimate only when "this text appears somewhere" IS the claim** —
  typically a NEGATIVE ("no knowledge percent leaks into the drawer") over ONE named node, and it needs
  a positive companion on the same frame, or it also passes on a surface that never rendered.
- **A needle must be one no other copy can satisfy.** `str(4)` matched the meter beside it (`Tame — 4%`)
  and would have matched a coordinate; the Herders row's whole rendered value cannot.
- **Assert the RENDER, not the model the harness just wrote.** `_compose.forage_policy() == "sustain"`
  is a test that the harness can set a field. Read the rung's own selected fill back instead
  (`Readout.rung_is_selected`).
- **Drive the CONTROL, never `emit_signal` its own signal on its behalf.** `picker.emit_signal(
  "item_selected", 0)` calls the connected lambda directly, so it passes on a control whose popup never
  opens, whose entries cannot be reached, and — the case that shipped — whose selection **the engine
  declines to change**, which is precisely the branch where no signal is emitted at all. A test that
  fakes the signal cannot fail for a broken widget; it only asserts that the callback the harness just
  invoked runs. Push real input through `InputProbe` (`ui_preview/input_probe.gd`) instead, and where
  the gesture is more than one press — a popup opens on the press, so the release is a separate
  event — drive the halves apart. `chapters/trade.gd`'s destination pick is the worked example, in
  `harness-ui-preview.md`.
- **Count the terms a "states all THREE" claim names.** Matching the middle term alone survives losing
  either of the others.
- **A fixture that cannot reach the state being claimed makes the assertion decorative** — check the
  fixture can actually produce the shape before trusting a green line.

- **Use `_assert_hud` (`h._assert_hud` from a chapter), not a bare `assert`.** A bare `assert` HALTS this harness on failure rather than
  reporting one: the headless run breaks into the debugger and hangs until it is killed, printing only a
  stack trace on stderr and none of the remaining states. Measured while sabotage-checking a herd stock
  row — the run had to be timed out at ten minutes to learn which line failed. `_assert_hud` names what
  it found, counts toward the PASS/FAIL tallies, and lets the rest of the suite finish. Some older
  assertions are still bare; that is history, not a pattern to copy.

**Sabotage-verify anything you touch**: break the behaviour and watch the assertion fail, naming what it
found. An assertion "fixed" but never seen failing is the same bug again.
