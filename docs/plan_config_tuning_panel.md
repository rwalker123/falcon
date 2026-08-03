# Config Tuning panel — and the Workbench that hosts it

Design spec for issue #253. Two things land together, and the second is why the first is not simply
another Inspector tab: a **Config Tuning** surface that collapses the playtest turnaround loop, and
the **Workbench** — the replacement designer surface it is page one of.

## The problem

Changing a tuning number today costs a full round trip: edit a JSON under `core_sim/src/data/`,
restart the server, start a new game, and play back to whatever situation the number was supposed to
affect. The numbers that most want iterating — forage throughput, birth rate, lethality, the climate
cut points — are exactly the ones whose effect only shows up after a world has run for a while, so
the loop is paid repeatedly.

The goal is: change the number in the client, start a run on it, see it. Without leaving the client
and without restarting the server.

## Why a new surface, not an Inspector tab

The Inspector is legacy scaffolding and is slated for replacement
(`.claude/rules/client/inspector-panels.md` carries the standing decision). Adding the first genuinely
new designer tool in a year to a panel that is being deleted would mean building it twice. So the
tuning page is page one of its replacement.

**The Workbench is the designer/dev surface.** It is not the player HUD and not the Inspector: it
reserves a screen edge through the shared reservation registry (reserver id `&"workbench"`), draws in
`HudStyle`'s existing dark console language rather than a second visual system, and opens on `` ` ``.
The Inspector is untouched by this work and keeps `I` until its tabs are replaced page by page.

### The decomposition contract, decided up front

The Inspector's failure mode is on record twice — `Hud.gd` reached ~9,850 lines and `Inspector.gd`
~6,500, both one reasonable-looking method at a time. The Workbench is therefore specified as a
coordinator with **nowhere to put feature code**:

| Module | Holds |
|---|---|
| `WorkbenchShell.gd` | Coordinator ONLY: rail, content host, pinned footer region, page routing, edge reservation, snapshot fan-out. It never names or `preload`s a page |
| `WorkbenchPages.gd` | The registry. Rows of `{id, title, subtitle, section, glyph, script}` |
| `WorkbenchPage.gd` | The page contract: `build`, `build_actions`, `apply_update`, `reset`, the command hooks |
| `WorkbenchWidgets.gd` | All-`static`, stateless shared drawing. State threaded in as PARAMETERS, never a back-ref |
| `WorkbenchVocab.gd` | ALL-`const` labels, glyphs, geometry, font sizes |
| `pages/*.gd` | One page each. A page that grows gets sub-controllers under `pages/<id>/`, not more methods |

**The load-bearing property is that adding a page is one registry row plus one file** — there is no
shell edit in that list, so the shell has no reason to grow when the surface does. A row with an
empty `script` is a declared-but-unbuilt page: the rail shows it and the shell renders a placeholder,
which is how the intended shape of the surface stays visible while it is built out.

That is enforced rather than documented. `tools/workbench_shell_budget.gd` fails the build if the
shell outgrows its line budget, if it names a page, or if a registry row points at a script that is
missing or does not extend `WorkbenchPage` — the last one because a typo'd path degrades *silently*
to the placeholder. A rule in a document is what did not work the previous two times.

## The Config Tuning page

### The manifest, and why it is curated

`src/config/tuning_manifest.json` declares what is tunable: per config kind a `kind`/`label`/`env_var`
and a list of `{pointer, label, type, min, max, step, default, unit?, hint}`.

It is a **curated list, not a generic JSON tree editor**, for two reasons. The command channel is
one-way — `CommandBridge.send_line` returns transport success, not a server reply — so the client
cannot ask what the current values are and must carry its own. And a designer looking for
`regrowth_rate` should not have to scroll 33 flora species to find it. A hand-picked list of levers
that actually move a playtest is the more useful artifact anyway.

**Edits produce a sparse patch.** A parameter the designer did not touch never appears in the payload,
so the server's real values always win and manifest drift can only ever stale a *displayed hint* —
never the running simulation.

That drift is still worth catching, so: **one manifest, two readers.** A `core_sim` test loads the
same file and asserts every entry's pointer resolves, in the shipped config, to a scalar of the
declared type inside the declared range. A renamed config key fails CI instead of silently rendering
a dead row.

### Layout

One group per config kind, each a sunk well of rows. A row is two lines: the label with its number
field, then the hint and the shipped default as one wrapped caption. A modified row wears an amber
dot, and the dot's width is reserved on every row so becoming dirty changes a colour and never shifts
the label.

The status line and the Apply / Revert buttons sit in the shell's **pinned footer**, below the scroll
and outside it — a designer partway down a long page can always see how many overrides are staged and
reach the button.

A standing banner states the restart-scoped contract on the surface itself: **changes apply on the
next New Game**. That belongs in the UI rather than in this document — someone who edits a value and
watches the running world not change must not have to come here to find out why.

## The server seam

Two findings shape it, and both are load-bearing:

**`config_load.rs` is the only place that decides which file loads.** All ~26 configs route through
`load_config_from_env(env_var, label, default_rel_path, builtin, from_file)`, and one `env::var` call
picks the path. So the override goes in as an **in-process registry consulted ahead of the env var** —
precedence becomes registry → `*_CONFIG_PATH` → default file → builtin. Not `std::env::set_var`: the
server is multithreaded and live, and mutating its environment to pass a value between two parts of
the same process is the wrong tool for a decision that already has a home.

**`new_game` already re-reads every config.** `handle_new_game` → `rebuild_world_from_config` →
`build_headless_app()`, which is a wall of `load_*_from_env()` calls. So "restart the sim with
overrides" needs no process restart — a New Game inside the live server picks up the new paths for
free. This is what makes the loop cheap enough to be worth building.

The handler therefore: merges the sparse patch into the kind's current effective JSON, parses it
through that kind's `from_json_str` — **which is where `validate()` already runs** — and only on
success writes the file and registers the path. A rejected patch changes nothing and logs.

**Validating before installing is not defensive coding, it is the whole safety argument.**
`load_config_from_env` panics on a present-but-broken file, by deliberate design
(`.claude/rules/core_sim/config-loading.md` — a config the operator asked for must never be silently
replaced). An override installed without validation would therefore panic the server at the *next New
Game*, arbitrarily far from the edit that caused it. The boot path's strictness is correct and stays;
this seam has to be worthy of it.

The asymmetry that rule records — boot panics, hot-reload does not — is left alone. This is a third
path with its own answer: **reject at the edit, where there is a human watching and a UI to tell.**

## Scope

In: the Workbench shell, its decomposition guard, the Config Tuning page, the manifest and its drift
test, the override registry and the command that drives it.

Out: migrating any Inspector tab. The Inspector keeps working on `I` and is retired page by page as
the Workbench grows, which is what the placeholder rows in the registry are marking out.
