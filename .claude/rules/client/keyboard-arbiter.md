---
paths:
  - "clients/godot_thin_client/src/scripts/KeyboardArbiter.gd"
  - "clients/godot_thin_client/src/scripts/TextEntryFocus.gd"
  - "clients/godot_thin_client/src/scripts/MapView.gd"
  - "clients/godot_thin_client/src/scripts/Main.gd"
  - "clients/godot_thin_client/src/scripts/ui/MenuShell.gd"
  - "clients/godot_thin_client/project.godot"
  # The gate. It lives with the arc it guards rather than with the other headless guards, because
  # the registry it walks and the policy it asserts are this file's subject; gating it elsewhere
  # would put the rationale for the assertions a file away from the assertions.
  - "clients/godot_thin_client/tools/hotkey_guard.gd"
  - "clients/godot_thin_client/tools/hotkey_guard.tscn"
  - "xtask/src/hotkey_guard.rs"
---

# Who owns the keyboard

**Its own file, deliberately narrow.** The consumers live in `MapView.gd` and `Main.gd`, the
predicate and the arbiter in `KeyboardArbiter.gd` / `TextEntryFocus.gd`, and the focus release in
`MenuShell.gd`; folding this into `map-renderers.md` would drag the whole MapView
renderer-decomposition doc onto every `Main.gd` edit to carry one section.

## The bug that took four attempts, and why the first three could not have worked

Typing a save's name **panned the map**. That was fixed. Then typing `r` **toggled the event dock**.
That was fixed. Then **`Ctrl+C` re-centred the map**. Each fix was correct about the key it named and
wrong about the shape of the problem: there were **five independent keyboard consumers and no
arbiter**, each deciding for itself whether it should act, by a different mechanism.

| Consumer | Mechanism | Was |
|---|---|---|
| `MapView._process` — pan/zoom | polled `Input.get_action_strength` | guarded by focus only |
| `Main._process` — I V R F `` ` `` | polled `Input.is_action_just_pressed` | guarded by focus only |
| `MapView._unhandled_input` — C H T | raw `event.keycode ==` | **unguarded** |
| `Main._unhandled_input` — Escape | `ui_cancel` → `escape_claimant` | correct by design |
| `HarvestFloorChart._gui_input` | a Control's own input | correctly scoped |

Three defects compounded, and each one is a different reason a *"just guard this site too"* fix ran
out:

1. **A `LineEdit` filters only PARTIALLY.** It consumes the events it USES — printable keys, arrows,
   backspace — and lets everything else fall through to `_unhandled_input`. Focus was never a gate,
   and reading it as one is what made the first two fixes look complete.
2. **Nothing checked modifiers, anywhere.** `event.keycode == KEY_C` is true for `Ctrl+C`. The polled
   side was worse: bindings are registered with a bare keycode and no modifier flags, and
   **`Input.is_action_just_pressed` defaults to NON-exact matching**, so the bare-`R` action fired
   for `Ctrl+R` and the bare-`W` action for `Cmd+W`.
3. **There was no modality.** The pause menu is `pause_layer.visible = true`; the tree is never
   paused, and no surface could declare that it owned the keyboard — so `r` toggled the event dock
   *behind* an open pause menu.

**`escape_claimant` was already the answer, for one key.** A pure static taking booleans and
returning who should receive Escape: someone hit this exact problem and solved it correctly, once.
`KeyboardArbiter` is that shape generalised to all of them.

## The registry — every gameplay key in ONE table

`KeyboardArbiter.REGISTRY` is one row per gameplay key: its `id`, its **class**, how it is read
(`kind`), its keycode, and the `site` that consumes it. Sixteen rows in four classes —
`CLASS_MAP_MOTION` (W/A/S/D, Q·E), `CLASS_MAP_VIEW` (C fit, H grid, T textures), `CLASS_PANEL_TOGGLE`
(I V R F and the backquote Workbench), `CLASS_ESCAPE`.

**It is the roster a test can walk, and that is its whole point.** Nothing could enumerate the
hotkeys before — they were literals in two files — which is precisely why no test could have caught
`Ctrl+C`, and why adding a hotkey used to extend nothing. Adding a row now extends the gate
automatically.

**The bindings are registered FROM the registry** (`ensure_action_bindings(key_class)`, called by
`MapView._ready` for map motion and `Main._ready` for the panel toggles). The two hand-rolled
`_ensure_*_binding` copies that used to hold their own key tables are gone: a second copy of the
roster is how a key ends up governed by a class it is not in.

**`project.godot` DECLARES NO ACTIONS, and the section that used to is a cautionary tale.** Its
`[input]` block held all six pan/zoom actions plus `toggle_inspector` as hand-written Dictionaries —
and **every one of them loaded with ZERO events.** `InputMap` deserialises an event entry only from
the `Object(...)` form the editor writes; a `{"type": "InputEventKey", …}` dictionary parses as a
plain Dictionary, `Ref<InputEvent>` comes back null, and `action_add_event` is never reached. So
`has_action("map_pan_left")` answered `true` and `action_get_events("map_pan_left")` answered empty,
for the whole life of the file — the keys worked only because `MapView` and `Main` re-registered them
at runtime. Measured, not inferred, while the gate's first run failed on it. `hotkey_guard` now fails
if an `[input]` section reappears.

## The arbiter — three owners, one pure function

```
keyboard_owner(text_entry_focused, modal_menu_open) -> OWNER_TEXT_ENTRY | OWNER_MODAL_MENU | OWNER_GAMEPLAY
allows(owner, key_class) -> bool
```

| Owner | When | May act |
|---|---|---|
| `TEXT_ENTRY` | a `LineEdit`/`TextEdit` holds this viewport's focus | nothing but `ESCAPE` |
| `MODAL_MENU` | the pause overlay is open | nothing but `ESCAPE` |
| `GAMEPLAY` | otherwise | everything |

**Text entry outranks the modal menu** because the field that started all of this lives *inside* the
pause menu: asking "is a menu open?" first would hand a typed `r` back to the event dock.

**`MODAL_MENU` is a deliberate behaviour change.** Before it, every panel toggle and the whole map
answered the keyboard through an open pause menu. A modal surface that owns the keyboard is what
stops the next report.

**`ESCAPE` is allowed under every owner** — it is how the player leaves a surface — and *which*
surface it reaches stays `Main.escape_claimant`'s four-way decision. The arbiter does not re-litigate
that.

**NOT A CONTEXT STACK, on purpose.** Push/pop trades this bug class for a worse one: an unbalanced
pop leaves the keyboard owned by a surface that is gone, which is the stuck-focus failure with no
focus owner to inspect. The surfaces here — a `CanvasLayer` toggled by `visible`, a `LineEdit` that
is rebuilt on every keystroke — have no reliable lifecycle hooks to hang a stack off. A **derived**
predicate cannot go out of balance, because it is recomputed from the world every frame; and being
pure is what lets the whole policy be enumerated without standing up a scene.

**Targeting, the compose sheet and the work inspector are NOT owners.** They permit map motion
because you pan while choosing a target. That is reasonable and was left alone.

**The modal-menu flag is PUSHED, not discovered.** The overlay is `Main`'s `$PauseLayer`, so
`Main._set_pause_menu_open` — the one writer of `pause_layer.visible` — pushes it to
`MapView.set_modal_menu_open`. `Main` reads its own layer live. Reaching up the tree for it would tie
the map to a scene layout the preview harnesses do not have, and routing all three call sites through
one setter is what keeps the push from being forgotten at one of them.

## Exact matching

- **Polled reads pass `exact_match = true`** to `is_action_just_pressed` / `get_action_strength`, so
  a bare-key binding stops matching a modified combination. Confirmed rather than assumed: the
  registry's events carry no modifier flags, and `hotkey_guard`'s part E drives a real
  `MapView._process` and asserts **bare WASD still pans** before it asserts anything about
  suppression.
- **Raw key checks go through `KeyboardArbiter.is_bare_key(event, keycode)`** — pressed, not an echo
  repeat, and no ctrl/alt/meta/shift. No `.gd` under `src/` outside `KeyboardArbiter.gd` may compare
  a keycode at all.
- **`is_escape_key` is deliberately LOOSE**, matching Escape with any modifier held. It is the one
  key where a loose match costs nothing and a strict one strands the player inside a surface.

## The guard — `cargo xtask hotkey-guard`

`tools/hotkey_guard.tscn`, headless, exits 0/1. Shaped like `core_sim/tests/sim_state_coverage.rs`:
it walks the live thing and **fails on anything unclassified**. Five parts, each covering a different
way the bug came back.

- **A. The source scan.** Every `Input.<member>` call in `src/` is classified against three lists
  (polled action reads, raw device reads, neutral writes); anything in none of them fails as
  unclassified. **The sweep is of the `Input` SINGLETON, not of the spellings that come to mind** —
  the second polled site survived the first fix because that sweep searched `is_action_pressed` /
  `get_action_strength` / `is_key_pressed` and omitted `is_action_just_pressed`, which is how every
  HUD hotkey is read. A polled read must name a registry row, sit at that row's declared site, pass
  `exact_match = true`, and live in a function that calls `KeyboardArbiter.allows()`.
- **B. The policy.** Every registry row against every owner, plus the six reported cases by name.
- **C. The modifier axis.** Every row bare and under each of the four modifiers, through the real
  matchers, with a liveness leg on each (the bare press must match, and the modified press must still
  match NON-exactly — otherwise the exact-match assertion would pass on a malformed event).
- **D. The live poll.** The same axis through `Input` itself. **`Input.parse_input_event` does not
  reach the action state on its own**: with `use_accumulated_input` on — Godot's default — it appends
  to a buffer the main loop drains once per iteration, so a probe that pressed a key and polled it in
  the same call read the state from before the press. `Input.flush_buffered_events()` is what makes
  the probe measure `Input` rather than the buffer.
- **E. The site.** A real `MapView` in the tree, driven by real `Input` state, observed on
  `pan_offset`. Parts B–D would all still pass if a site simply stopped calling the arbiter; this is
  the leg that would not.

**The three allowed exceptions**, each declared with its reason in `EXCEPTIONS`:
`HarvestFloorChart._handle_key` (a Control's own `_gui_input`, scoped by the GUI pass),
`MapView._process`'s `Input.is_mouse_button_pressed` drag latch (a mouse button — no text field
competes for one), and `Main._unhandled_input`'s `ui_cancel`.

**Sabotage-verified eight ways**, each failing and naming what it found: dropping `exact_match` from
one polled read (A); `MapView._process` no longer calling the arbiter (A + E, independently);
`is_bare_key` ignoring modifiers again — the original `Ctrl+C` (C, 13 legs); `keyboard_owner` no
longer returning `MODAL_MENU` (B + E); a new unregistered hotkey appearing in `Main._process` (A); an
`[input]` section returning to `project.godot`; a raw `keycode ==` creeping back into `MapView` (A);
and `set_modal_menu_open` dropping its push (E alone).

## What this design still CANNOT catch

- **`Main._process` has no behavioural leg.** Standing `Main` up headless means sockets, autoloads
  and a world request, so its five toggles are covered by the source scan (arbitrated, exact,
  registered) and by the live poll of the same expression — not by observing a panel fail to toggle.
- **The scan proves the enclosing FUNCTION consults the arbiter, not that the guard WRAPS the read.**
  A read moved above the `if` inside `MapView._process` would pass part A. Part E catches that one for
  `MapView`; nothing catches it for `Main`.
- **Only `src/` is scanned.** A keyboard read added under `tools/` or in the native extension is
  invisible to it.
- **A new OWNER is not automatically enforced.** The policy table is enumerated over
  `KeyboardArbiter.OWNERS`, so a new owner gets its rows checked against `allows()` — but nothing
  proves a call site ever computes it.
- **Godot's own built-in `ui_*` actions are out of scope.** They are matched by focused Controls
  through the GUI pass, not by the arbiter, and a Control that binds one is not visible to the scan.
- **The registry is a roster, not a conflict checker.** It would happily hold two rows on the same
  keycode in different classes.

## The mirror-image failure is FOCUS LEFT STUCK

A field that keeps focus after its surface is dismissed leaves the map unresponsive to WASD and every
panel toggle dead, with nothing on screen to explain why — strictly worse than the bug the guard
exists for. So the surface that owns a text field owns handing the keyboard back:
`MenuShell.release_text_focus()`, called on every pane change, after a save is submitted, and by
`Main._hide_pause_menu`. **Hiding a `CanvasLayer` does not do it for you**: `CanvasLayer` is not a
`CanvasItem`, so its `visible` never reaches the Controls under it as a visibility change. Neither
does `queue_free` within the frame it is called — the node holds focus until it actually leaves the
tree.

Verified by sabotage, three ways, in `menu_preview._assert_text_focus_is_handed_back`: widening the
predicate to `is Control` fails the narrowness leg; making it always false fails all three
focus-is-taken legs; stubbing `release_text_focus` fails all three release legs — which is also the
proof that neither `queue_free` nor hiding the layer releases focus on its own. `hotkey_guard` part E
carries the same claim at the site: the map must pan again once the field lets go.

**Scoped to TEXT ENTRY, never to "anything focused".** A focused Button does not consume letters, so
suppressing input whenever a button held focus would kill the map and every panel toggle after each
click on a HUD control.

**The guard covers the keystroke reads and NOTHING around them.** In `MapView._process` the mouse-pan
release latch and the targeting / expedition pulses stay outside it; in `Main._process` the query
pump, the connection poll, the world-request retry and the snapshot drain stay outside it — that pump
is what carries the answer to the save being named, so a guard around the whole function would stall
the socket the save depends on.

## Key scripts

| Script | Purpose |
|--------|---------|
| `KeyboardArbiter.gd` | **The registry and the arbiter.** `REGISTRY` (every gameplay key, its class, kind, keycode and site), `keyboard_owner` / `allows` / `owner_for`, `is_bare_key` / `is_escape_key`, `ensure_action_bindings`. All-static, no `class_name`, `preload`ed by its callers (the `ClientBuild` / `ServerPortsFile` pattern) |
| `TextEntryFocus.gd` | The ONE *"is the player typing?"* predicate — `is_text_entry(node)` and `held_in(viewport)`. It stays its own file because `MenuShell.release_text_focus` asks it about a single node, which is not an arbitration question; `KeyboardArbiter.owner_for` is its only other caller |
| `tools/hotkey_guard.gd` / `.tscn` | The gate: the source scan, the policy enumeration, the modifier axis, the live poll and the `MapView` site. `cargo xtask hotkey-guard` |
