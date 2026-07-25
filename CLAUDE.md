# Agent Collaboration Guide

Contributors are expected to follow DRY and SOLID principles to ensure code quality, maintainability, and a strong user experience. Avoid shortcuts and prioritize best practices in both development and design.

## Document Hierarchy

This repository uses a layered documentation structure:

### Design Documents
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` — Authoritative game manual. Weaves narrative, player-facing systems, and intended gameplay experience.
- `docs/architecture.md` — System-wide implementation overview. Cross-system data flow, extensibility patterns, and configuration reference.

### Subsystem Documentation
- `core_sim/CLAUDE.md` — Simulation engine **hub**: build/config/ports, shared food-module
  vocabulary, turn loop, and a routing table to the per-arc rules below
- `.claude/rules/core_sim/*.md` — the per-arc engineering rationale for `core_sim`
  (worldgen, fauna, husbandry, flora, graze, telling, campaign, …). Each carries `paths:`
  frontmatter and **loads only when you touch the code it describes**. Put new per-arc
  rationale in the rule file that owns the arc — that is what keeps two concurrent
  worktrees off the same file.
- `clients/godot_thin_client/CLAUDE.md` — Godot client **hub**: build/verify commands, the
  `Hud.gd` decomposition invariant, boot/menu scripts, scene structure, theming, hotkeys
- `.claude/rules/client/*.md` — the per-arc rationale for the Godot client (HUD modules,
  labor UI, terrain shader, panels, overlays, test harnesses, …), same `paths:` gating.
  **The per-script index went with them**: each rule carries a `## Key scripts` table for
  the scripts it covers, so a new script's row goes there, not in the hub
- `sim_schema/README.md` — FlatBuffers schema contracts
- `sim_runtime/README.md` — Shared runtime utilities

### Task Tracking — GitHub Issues, not a file in the repo
The backlog lives in **GitHub Issues + the Falcon Backlog project**:
→ https://github.com/users/rwalker123/projects/2

- **Arcs** are `type:arc` parent issues; their phases/slices are sub-issues of that parent.
- Every issue carries exactly one `type:*` and at least one `sys:*` label; the Project
  carries Status / Priority / Subsystem fields.
- **Design specs stay in `docs/plan_*.md`** — issues *link* to them, never copy them.
- Use the skills rather than hand-written `gh` commands — they know the field and option
  ids (`.claude/skills/`, ids in `.claude/skills/falcon-tracker-reference.md`):

  | Skill | Use it to |
  |---|---|
  | `/task-add` | file a new item, labelled and on the board |
  | `/task-start` | take an item In Progress → worktree → PR |
  | `/task-status` | move Status/Priority/Subsystem, toggle `blocked` / `good-next` |
  | `/task-report` | read the board — in progress, blocked, ready to pick up |

> There is **no `TASKS.md`**. It was the backlog until 2026-07-23, when the open items were
> migrated to Issues; the file was retained as a historical record for a day and then deleted,
> because a frozen backlog beside a live one only invites edits to the wrong place. Its
> engineering reasoning was already duplicated in the subsystem `CLAUDE.md` files, which is
> where as-built notes belong. `git log -- TASKS.md` still has it if you need the history.

---

## When Updating Documents
- Add new concepts first to the **manual** if they affect gameplay communication.
- Add implementation details to the **rule file that owns the arc** (`.claude/rules/core_sim/*.md`,
  `.claude/rules/client/*.md`) — **not** to the subsystem `CLAUDE.md`, which is a hub. See "The hub
  files are not where rationale goes" below; it is the single easiest mistake to make in this repo.
- Keep `docs/architecture.md` focused on cross-system concerns and overview.
- Extract concrete tasks into **GitHub Issues** via `/task-add` — never into a file.
- Cross-link between documents when gameplay description references technical constraints and vice versa.

### The hub files are not where rationale goes

`core_sim/CLAUDE.md` and `clients/godot_thin_client/CLAUDE.md` are **hubs**: a short landing page
per subsystem holding what is true of *all* work in it — build commands, the global/boot config
list, the shared vocabulary, and a routing table to the rules. Every subsystem `CLAUDE.md` is loaded
into context **on every session in this repo**; a rule file loads only when you touch the code it
describes. So prose added to a hub is paid for by every session forever, whether or not it is
relevant — which is why the hubs were split in the first place.

**The test for a new paragraph** — ask *"is this true of all work in this subsystem?"*
- Yes (a build command, an environment override, a cross-cutting invariant, a new rule's routing
  row) → the hub.
- No (why one arc's system works, a config file's key table, a per-script row, an as-built note, a
  bug's mechanism and its guard) → the rule file that owns the arc, in its `## Config files` or
  `## Key scripts` table where one exists.

If the rule file's `paths:` frontmatter already covers the code you changed, the hub copy is pure
duplication: anyone who can break the invariant loads the rule anyway. Adding a new arc means
adding a **row to the routing table**, not a section to the hub. If the rationale genuinely has no
owning rule file, create one with `paths:` frontmatter rather than parking it in the hub.

### Cross-linking Convention
- Define authoritative specs in the **rule file** that owns the arc (or the hub, for genuinely
  subsystem-wide facts) — exactly one home per fact
- Add "See Also" cross-references in dependent documentation
- Avoid duplicating implementation details across files — a pointer from the hub to a rule is
  still duplication if the rule's `paths:` already load it for the reader who needs it

---

## Git, Branches & PRs — READ BEFORE ANY GIT COMMAND

This repo is worked by **multiple concurrent sessions**, so the unit of isolation is the
worktree: each piece of work gets its own checkout, its own branch, and its own PR. The
human owns the merge. Violating the rules below has cost real work.

- **The worktree flow is the default.** New work starts by creating a fresh worktree
  branched off `origin/main` (`EnterWorktree`, or `/task-start` for a tracked issue). That
  branch is yours: **commit, push, and open the PR without asking** — implementing the work
  authorizes the git that carries it. See "Working from a Git Worktree" below for the
  isolation rules that make this safe.
- **Landing work anywhere else needs an explicit, current "yes".** Committing onto a branch
  you didn't create — the checked-out branch of a shared session, an existing PR, `main`,
  or stacking on someone else's branch — is a topology decision, not an implementation
  detail. "Do the work", "go implement", "fix this" do **not** authorize it, and announcing
  a plan ("I'll branch off X and stack it…") is **not** approval. Stop and ask.
- **Never `git add` broad paths** — no `git add -A`, `git add .`, or `git add <dir>`.
  Another session (or the human) often has unrelated uncommitted edits in the same working
  tree; a broad add silently sweeps their work into your commit and onto the wrong branch.
  **Stage only the specific files you changed, by explicit path.** If unsure what's yours,
  run `git status` and ask.
- **The human merges PRs** through their own review flow — you never merge, close, or
  reopen a PR.
- Before every commit, `git status` and confirm each staged path is one you intended, and
  `git rev-parse --show-toplevel` to confirm you are committing from the worktree you meant.

## PR Expectations for Agents
- Mention in summaries which document(s) were touched and why
- Verify narrative additions remain consistent with implementation notes
- When modifying subsystem code, check if the corresponding CLAUDE.md needs updates

Stay consistent with this flow to keep design intent and engineering execution aligned.

---

## Working from a Git Worktree

Multiple checkouts of this repo (git worktrees) can be developed in parallel, but
they must stay independent. Worktrees created by the Claude Code harness live under
`.claude/worktrees/<name>/` — *inside* the main checkout — which makes two mistakes
easy:

- **Run every tool from the worktree root you intend to change.** A session's Bash
  calls and its `server-dev`/`client-dev` subagents all operate in that session's
  *primary working directory*. If a session meant for a worktree is rooted at the
  main checkout, `cargo fmt`/`clippy`/`build`/`test` and every edit silently hit
  **main** instead — this is the "commands are global / only affect main" symptom.
  Confirm with `git rev-parse --show-toplevel` before builds, edits, or commits.
- **Ports are isolated per checkout.** Launch the stack with `scripts/run_stack.sh`,
  which auto-assigns each checkout its own block of four TCP ports via
  `SIM_PORT_BASE` (see `core_sim/CLAUDE.md` → Environment Overrides). Never run two
  servers on the default 41000–41003 block at once.
- **Builds are already isolated** — each worktree has its own `target/`, so cargo
  artifacts don't collide (at the cost of disk).
- `.claude/worktrees/` is gitignored so searches, `git status`, and `git add` from
  the main checkout don't descend into nested worktrees.

---

## Delegating Implementation to Coder Agents

Long sessions fill the orchestrator's context fast because writing code churns
through file reads, builds, and test output. Two subagents in `.claude/agents/`
absorb that churn — they do the read → edit → build → test loop in their own
context and return only a terse report:

- **`server-dev`** — Rust side (`core_sim`, `sim_runtime`, `sim_schema`, `xtask`).
  Self-verifies with `cargo fmt` + `clippy -D warnings` + `cargo test`.
- **`client-dev`** — Godot/GDScript + native extension (`clients/godot_thin_client`).
  Self-verifies with `cargo xtask godot-build` + the ui_preview PNG harness (it
  reads the rendered frames).

**The workflow:** the orchestrator owns design and produces a *complete,
comprehensive spec* — decided approach, files to touch, contracts, edge cases,
config levers — and the agent just implements it. Design and architecture
decisions stay with the orchestrator; only settled specs are delegated. Do **not**
hand an agent an open-ended or ambiguous task — if the spec isn't complete enough
to implement without further design judgment, it isn't ready to delegate.

Guidance:
- Split cross-cutting work: `server-dev` does the schema/sim half, `client-dev`
  consumes it; each flags the other's remaining work in its report.
- Continue the *same* agent (via SendMessage) for iterative follow-ups so its
  context persists, rather than cold-starting a fresh one and re-explaining.
- Agents don't branch or commit — they leave the tree changed and report; git
  stays with the orchestrator.

---

## UI Panel Sizing
**The rule is "never reimplement bespoke height/scroll logic."** Which shared
helper you reuse depends on what the panel *is* — there are two, and picking the
wrong one silently misbehaves rather than failing:

- **Free-floating panels** (anchored against the viewport — selection panel,
  hex-info widgets): `clients/godot_thin_client/src/scripts/ui/AutoSizingPanel.gd`.
  Attach the script to the panel node and call `fit_to_content`. It sizes against
  the *viewport*, using `global_position` + anchors + `offset_bottom`.
- **Dock cards** (children of a `PanelDock` `VBoxContainer` — command feed,
  Telling panel): `src/scripts/ui/hud/DockScrollFit.gd`, via `PanelCard`. A dock
  child has its size overwritten by the container on every layout pass, and the
  ceiling that matters is the *dock's* remaining height, not the window's — so
  `AutoSizingPanel` is the wrong tool there and will fight the container.

If you find yourself writing height math by hand, you have picked the wrong
helper or found a third case worth extracting — extract it rather than
open-coding it.
