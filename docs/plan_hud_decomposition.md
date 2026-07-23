# HUD Decomposition — plan

`clients/godot_thin_client/src/scripts/Hud.gd` is a ~9,850-line, 436-function,
95-member `class_name HudLayer`. It is the last monolith in the client: the
Inspector already completed this exact journey (28 panel scripts under
`ui/inspector/`, coordinator down from ~6,500 → ~880 lines, `apply_update`/`reset`
contract), and much of the HUD's own furniture is already extracted
(`LegendController`, `CommandFeedController`, `TellingPanel`, `ComposeSheet`,
`BandCityPanel`, …). What is left in `Hud.gd` is the selection/labor core, and it
is welded together not by structure but by **shared mutable state** — a handful of
god-variables that four different responsibility clusters read and write directly.

This document plans the arc that finishes the job. It is deliberately **phased so
each phase lands on its own**, and the phases are ordered so the hardest coupling
is dissolved **first**, in place, before any code moves files.

## The arc at a glance

- **Phase 0 — State models (this document's focus).** Extract the cross-cluster
  god-variables into two encapsulated `RefCounted` state models, *in place* inside
  `Hud.gd`. No file split, no behaviour change. This is the durable artifact every
  later phase consumes, and the one piece that is never rewritten by extraction.
- **Phase 1 — Low-friction controllers**, split into two PRs after the cluster map
  showed the turn-orb cluster is more coupled than the readouts:
  - **1a — Top-bar readouts.** Sedentarization / demographics / discoveries /
    intensification / stockpiles → one `RefCounted` controller on the
    `LegendController` idiom. Genuinely self-contained; the pattern-proving PR.
  - **1b — Turn orb / attention / fork.** The orb + fork panel + attention
    *assembly* extract, but the cluster keeps two seams to band/labor:
    `_on_turn_orb_focus` routes into band-panel helpers, and `_band_attention` is
    written by `update_band_alerts` and only read by the orb. Done with a
    `set_band_attention()` seam + relayed signals — its own focused PR.
- **Phase 2 — The selection core.** Extract the SelectionCard (chips / subject
  list / drawer) and the drawer/allocation builders it calls, reacting to the
  Phase-0 selection model's `changed` signal instead of being imperatively
  re-rendered. **The tile-inspector flash fix lands here**, as a property of the
  new `apply_update`-style contract (diff-and-update) rather than the current
  unconditional teardown-and-rebuild.
- **Phase 3+ — Band panel bridge, labor model, targeting.** As they come.

Phases 1–3 are sketched at the end; the body below specifies **Phase 0**.

---

## Phase 0 — State models

### Goal & non-goals

**Goal:** replace the scattered god-variables with two cohesive, encapsulated
state objects so that (a) every state transition happens in exactly one place, and
(b) there is a `changed` signal seam that later-extracted controllers can subscribe
to instead of being called imperatively.

**Non-goals — Phase 0 changes no behaviour.**
- `Hud.gd` itself is not carved up. The two models are new small `RefCounted`
  scripts under `src/scripts/ui/hud/`, held as members of `HudLayer`. See "Where
  the models live".
- The `changed` signal is **emitted but not yet consumed.** The existing
  direct-call refresh path (`_render_selection_panel`, `update_band_alerts`) is
  untouched. Consuming the signal to decouple refresh — and fix the flash — is
  Phase 2.
- Only the **shared cross-cluster** state moves. The other ~80 members of
  `Hud.gd` are node handles and cluster-private flags; they stay where they are.
  Moving them would be churn for churn's sake.
- Every method name and signal on `HudLayer` that `Main.gd` reaches by reflection
  (`has_method`/`callv`, `has_signal`/`connect`) stays exactly as-is. Phase 0 does
  not touch the public surface at all — it is purely internal.

### The smell we are fixing (and the trap we are avoiding)

The god-variables, by in-file whole-word reference count:

| refs | var | decl | clusters that touch it |
|-----:|-----|-----:|------------------------|
| 46 | `_selected_tile_info` | 390 | targeting, selection card, morale/terrain label |
| 37 | `_selected_unit` | 391 | targeting, allocation, selection card, occupant detail |
| 35 | `_selected_herd` | 392 | targeting, herd assign, selection card, occupant detail |
| 31 | `_panel_band` | 1760 | allocation, band panel, focus-restore |
| 22 | `_player_bands` | 1726 | band picker, panel cycler, alerts |
| 19 | `_selected_subject` | 395 | which KIND of row is lit |
| 15 | `_player_band` | 1720 | labor model, assign resolution |
| 13 | `_roster_units` | 407 | roster assembly/render |
| 13 | `_pending_labor` | 1779 | optimistic feedback across labor + steppers |
| 12 | `_roster_herds` | 408 | roster assembly/render |
| 9 | `_world_herds` | 414 | hunt-source live position, herd-label fallback |
| 9 | `_current_turn` | 1770 | pending reconciliation, alerts |
| 6 | `_grid_width` | 1765 | hex-distance / wrap |
| 2 | `_grid_height` | 1766 | hex-distance / wrap |

**The trap:** a bag object — `state._selected_tile_info` as a public field everyone
reaches into — **relocates the smell, it does not fix it.** The coupling is
identical with a longer prefix. Phase 0 is worth doing only if it delivers the
three properties below.

### The two models

Split by **cohesion**, not into one new god-object. There are genuinely two
concerns here — *what the player is looking at* vs *the digested per-snapshot
player-faction world* — and one object holding all 14 vars would just be the
monolith's god-variable in a smaller file.

#### Model A — `HudSelectionState` ("what is the player looking at")

Owns the selection triplet, the lit-row kind, the assembled roster, and the
sticky-selection guard.

Fields (private; `_`-prefixed backing vars accessed only within the model):
- `selected_tile_info: Dictionary`
- `selected_unit: Dictionary`
- `selected_herd: Dictionary`
- `selected_subject: String` (`SUBJECT_LAND | SUBJECT_UNIT | SUBJECT_HERD`)
- `roster_units: Array`
- `roster_herds: Array`
- `subject_choice_tile: Vector2i` (the sticky-selection guard, `(-1,-1)` = none;
  travels with the model because it is meaningless apart from the selection —
  currently `Hud.gd:401`)

API (encapsulated mutation — no external write to a backing field):
- `select_tile(tile_info: Dictionary) -> void`
- `select_unit(unit: Dictionary) -> void`
- `select_herd(herd: Dictionary) -> void`
- `set_subject(kind: String) -> void`
- `set_roster(units: Array, herds: Array) -> void`
- `note_choice_tile(tile: Vector2i) -> void`
- `clear() -> void`
- read-only accessors: `tile_info()`, `unit()`, `herd()`, `subject()`,
  `roster_units()`, `roster_herds()`, `choice_tile()`, plus derived predicates the
  clusters already ask for (`has_unit()`, `has_herd()`, `is_land()` …).
- `signal changed(reason: StringName)` — emitted by every mutator. `reason`
  distinguishes an **identity** change (a different subject) from a **restate**
  (same subject, refreshed dict) so the Phase-2 consumer can diff-update vs rebuild.

#### Model B — `HudBandLaborState` ("the digested per-snapshot player world + optimistic overlay")

Owns the player-faction bands/expeditions captured each snapshot, the herds/patch
lookups the labor UI reads, the grid scalars for hex math, and the optimistic
pending-labor overlay.

Fields:
- `player_bands: Array`, `player_band: Dictionary` (the first, "one player band
  today"), `panel_band: Dictionary`, `player_expeditions: Array`
- `world_herds: Array`
- `pending_labor: Dictionary` (optimistic overlay, keyed by band entity)
- `current_turn: int`
- `grid_width: int`, `grid_height: int`
- satellites that travel with it because they are read/written in the same loop and
  nowhere else: `prev_band_sizes` (`Hud.gd:389`, the losing-population diff),
  `forage_patch_lookup` (`3168`), `food_module_by_tile` (`3146`)

API:
- ingest side (called from the `update_*` snapshot handlers):
  `ingest_snapshot_bands(...)`, `set_world_herds(...)`,
  `set_grid(width, height)`, `set_turn(turn)`, `set_forage_patches(...)`,
  `set_food_modules(...)`, `set_panel_band(...)`
- pending overlay: `record_pending_assign(...)`, `record_pending_move(...)`,
  `reconcile_pending(turn)` (drops entries from an older turn — the existing
  turn-based rule), plus the two derived readers **moved onto the model**:
  `effective_worker_map(band)` (currently `Hud.gd:3458`) and `effective_idle(band)`
  (`3535`) — they are pure functions of `pending_labor` + a band, so they belong
  with the data.
- read-only accessors mirroring the fields.
- `signal changed(reason: StringName)` — emitted on snapshot ingest and on a
  pending mutation, again with a `reason` so a consumer can tell "new snapshot"
  from "optimistic edit".

> **On a third model:** `pending_labor` is an optimistic overlay and could be its
> own object. It is not, because it is meaningless without the bands it overlays and
> every reader touches both. Two models, not three.

### The three properties that make this a fix, not a rename

1. **Encapsulated mutation.** Backing fields are private to the model; the 118
   selection-triplet writes and the pending writes become calls to the mutators
   above. After Phase 0 there is exactly one line where each transition happens.
2. **A `changed` signal.** A `RefCounted` can declare and emit signals (the
   `LegendController` idiom). Its mere existence enforces (1) — you cannot emit a
   signal from 46 scattered assignments — and it is the seam Phase 2 subscribes to.
   **Emitted in Phase 0, consumed in Phase 2.**
3. **Cohesive split.** Two models by concern, so neither is a god-object.

### Where the models live

Two new scripts, matching the established controller location and idiom:
- `src/scripts/ui/hud/HudSelectionState.gd` — `class_name HudSelectionState extends RefCounted`
- `src/scripts/ui/hud/HudBandLaborState.gd` — `class_name HudBandLaborState extends RefCounted`

`HudLayer` holds one of each (`_selection: HudSelectionState`,
`_band_labor: HudBandLaborState`), constructed in `_ready` beside `_legend` /
`_command_feed`. Every former `self._selected_*` / `self._player_*` / etc. access
becomes `_selection.…` / `_band_labor.…`. No node handles move; the models hold
**pure data**, never scene references, so they carry no `%Name` hazard.

### Invariants the migration MUST preserve

These are the documented behaviours (see `clients/godot_thin_client/CLAUDE.md`)
that the encapsulation must not perturb. Each becomes a mutator's contract:

1. **Sticky selection.** Choosing the LAND row clears both occupant dicts;
   `subject_choice_tile` distinguishes a fresh hex (auto-select default) from a
   decided one (preserve). `select_tile`/`set_subject`/`note_choice_tile` must
   reproduce the exact ordering. Guarded by `tile_panel_land_sticky`.
2. **Auto-select rule.** First roster unit → else first herd → else land, but only
   where the player has not chosen on THIS hex.
3. **Fog re-resolve.** A selected unit/herd that walks into fog drops its
   selection (via `clear()` on the relevant field) — the current
   `refresh_selection_payload` behaviour.
4. **Pending reconciliation is turn-based.** `reconcile_pending(turn)` drops
   entries tagged with an older turn; a newer-turn snapshot is authoritative and
   absorbs server clamping.
5. **`effective_idle` overlays pending.** The `+` steppers gate on optimistic idle;
   the count must match before and after.
6. **`panel_band` persistence.** Selecting a herd/empty tile leaves `panel_band`
   intact; it re-resolves by entity each snapshot, falling back to the first band.
7. **`_player_band` = first player cohort; `_player_bands` = all.** Assign/move/
   clear target the resolved band, not the faction default, on a multi-band hex.
8. **Attention concat gotcha.** `set_attention` is a full replace; the alert loop
   caches `_band_attention` and concatenates `_pending_fork_attention()`. This
   member is attention-plumbing, **not** selection/labor state — it does **not**
   move into either model in Phase 0. Called out so it is not swept in by mistake.

### Migration order (within `Hud.gd`, one reviewable sequence)

Do it as a mechanical, compilable-at-each-step sequence — this touches ~270
reference sites, so it must stay a straight-line refactor a reviewer can follow:

1. Add the two model scripts with fields + accessors + mutators + `signal changed`.
   Emit nothing yet is impossible (mutators emit) — but nothing is connected, so
   the signal is inert.
2. Construct `_selection` / `_band_labor` in `_ready`.
3. Migrate **Model A** first (selection is the more contained cluster): replace
   every `_selected_tile_info` / `_selected_unit` / `_selected_herd` /
   `_selected_subject` / `_roster_*` / `_subject_choice_tile` read with an
   accessor and every write with a mutator. Delete the old members. Build + run the
   ui_preview suite; it must be behaviour-identical.
4. Migrate **Model B**: bands, pending, world_herds, turn, grid, satellites; move
   `_effective_worker_map` / `_effective_idle` onto the model as methods. Delete the
   old members. Build + ui_preview again.
5. Grep-sweep for any surviving bare `_selected_`/`_player_`/`_panel_band`/
   `_pending_labor`/`_world_herds`/`_current_turn`/`_grid_` reference in `Hud.gd`.

### Verification — behaviour-neutral by the ui_preview harness

Phase 0 renders **identically** to before, so the guard is the existing PNG suite,
not new assertions:
- `godot --path . res://tools/ui_preview.tscn` — the `tile_panel_*`, `herd_*`,
  `food_*`, `forage_*`, `turn_orb_*` states must render the same content. The
  sticky-selection assertion (`tile_panel_land_sticky`) and the move-band-targets-
  the-selected-band assertion (`tile_panel_band`) drive the REAL path and so
  directly guard the selection-model migration.
- `godot --path . res://tools/band_panel_preview.tscn` — the `band_panel_*` states
  (incl. `band_panel_people_map_path`, which asserts the age brackets sum correctly
  through the real marker→panel path) guard the band/labor-model migration.
- `cargo xtask godot-build` stays green (the native extension is untouched, but the
  build must load the new scripts).

Because the change is structural, a clean run of the suite **is** the proof. Any
frame that moves is a migration bug, not an intended change.

### Landing

One focused PR: the two model scripts + the full in-file migration + this design
doc. It touches the whole of `Hud.gd`, so it is a **merge-conflict grenade** on a
shared branch — land it on its own, fast, when the tree is quiet, with nothing
stacked behind it. (Per the repo's git rules, the human owns branch/PR topology;
this lands on the `hud-decompose-phase0-state` branch already cut for it.)

---

## Phases 1–3 (sketch — specified when their turn comes)

- **Phase 1 — low-friction controllers (two PRs).** The cluster map showed the
  turn-orb cluster is more coupled to band/labor than the readouts, so:
  - **1a** — top-bar readouts (sedentarization, demographics, discoveries,
    intensification strip, stockpiles) → one `RefCounted` controller handed its
    top-bar label nodes in a constructor, `HudLayer` keeping thin delegators (the
    `LegendController` template). The build-stamp overlay and zoom rail stay on
    `HudLayer` (node-lifecycle / HudLayer-signal-bound; distinct concerns).
    `update_overlay` stays a `HudLayer` fan-out (labels via the controller, plus
    `_band_labor.set_turn` + `turn_orb.set_turn`).
  - **1b** — turn orb / attention / fork. The controller owns the orb wiring, the
    fork panel, and the attention *assembly* (`_push_attention` /
    `_pending_fork_attention`); `update_band_alerts` stays on `HudLayer` and feeds
    the band half via `set_band_attention()`; the orb's outward signals
    (`answer_fork_requested`, `next_turn_requested`, focus routing into the
    band-panel helpers) are relayed through `HudLayer`. The `_band_attention`
    plumbing from Phase-0 invariant 8 settles here.
- **Phase 2 — the selection core + the flash fix.** The SelectionCard (chips /
  subject list / drawer) and the drawer/allocation builders extract into a
  controller that **subscribes to `HudSelectionState.changed`** and updates
  in place — diffing subject identity vs restate — instead of the current
  unconditional teardown-and-rebuild in `_render_selection_panel`. This is where
  the tile-inspector flash is fixed, as a consequence of the new contract. Depends
  on Phase 0 having funnelled every selection mutation through the model.
- **Phase 3+ — band panel bridge, labor model, targeting.** Extract cluster H
  (band/city bridge + occupant detail), the labor model (D), and targeting (A),
  each reading `HudBandLaborState` rather than shared members.

## See also

- `clients/godot_thin_client/CLAUDE.md` — the HUD subsystem contracts these models
  must preserve (selection card, labor allocation, band panel, turn orb).
- Inspector decomposition (`docs/godot_inspector_plan.md`) — the completed
  precedent for this exact move, incl. the `apply_update`/`reset` panel contract
  Phase 2 mirrors.
