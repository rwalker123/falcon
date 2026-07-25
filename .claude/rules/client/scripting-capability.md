---
paths:
  - "clients/godot_thin_client/src/scripts/scripting/**"
  - "clients/godot_thin_client/src/scripts/ui/inspector/ScriptManagerPanel.gd"
---

<!-- Extracted verbatim from clients/godot_thin_client/CLAUDE.md lines 4178-4237.
     Routing table and shared vocabulary live in clients/godot_thin_client/CLAUDE.md.
     Regenerate with scripts/split_claude_md.sh -->

# Scripting Capability Model

QuickJS sandbox for user scripts. Implemented in the **Godot native extension**
(`native/src/runtime.rs`, `rquickjs`) — *not* in `core_sim`, which has no script
code. Each script runs on its own OS thread with its own `Runtime`/`Context`,
talking to the host over mpsc channels, ticked from Godot's `_process`.

**Much of the model below is designed but unbuilt.** Status is marked per item;
see issue #235, Script Sandbox Hardening, for the open work. Treat anything marked
_planned_ as a design note, not a description of current behaviour.

## Capability Families
| Capability | Status |
|---|---|
| `telemetry.subscribe` | **live** — snapshot/delta topics, subscription-filtered |
| `storage.session` | **live** — persisted with saves via `SimScriptState` |
| `alerts.emit` | **live** |
| `commands.issue` | **live, but ungated** — see the warning below |
| `ui.compose` | _declared only_ — in the capability registry (`sim_runtime/src/scripting.rs`) but **no handler arm** exists; a call logs "Unhandled host request" |

The JS surface is 8 globals assembled onto `globalThis.host` by a prelude
(`register`, `log`, `request`, `capabilities`, `sessionGet`, `sessionSet`,
`sessionClear`, `emit`). Capability families other than those are **string `op`
values passed to `host.request`**, routed in `handle_host_request`.

> **`commands.issue` is not sandboxed.** The "vetted command endpoints with
> throttle windows" phrasing is aspirational. In practice a script declaring
> this capability may submit **free-form command lines** (`payload.line`) at the
> same privilege as the player's own console — either via GDScript or, if a
> command endpoint is configured, over a raw `TcpStream` straight from the
> script thread, bypassing Godot entirely. There is no allowlist and no throttle.

## Determinism
Scripts are **not deterministic and not replay-safe**: they receive the raw
QuickJS globals (`Context::full`, so unseeded `Math.random()` and `Date`), and
tick off Godot's frame loop rather than sim turns. Do not host
simulation-authoritative or replay-sensitive logic here — see
`docs/plan_the_telling.md` §1a, where this ruled the sandbox out as a host for
the narrative beat engine.

## Script Distribution
- Discovery: recursive scan of `res://addons/shared_scripts` and `user://scripts`
  for `manifest.json`. **live**
- `.sscmod` bundles (zip), Ed25519 signatures, workshop feeds — _planned_,
  none implemented.

## Lifecycle
- Manifest validation on load (unknown capabilities rejected, subscriptions must
  be covered by a declared capability). **live**, in Rust
  (`sim_runtime/src/scripting.rs`).
- Explicit user-driven enable/disable/reload via `ScriptManagerPanel.gd`. **live**
- Hot reload via esbuild-lite bundling — _planned_.
- Suspension on sandbox violations — _planned_. There is a soft 8 ms tick budget
  (`SCRIPT_TICK_BUDGET_MS`) that is measured **after the fact** and only logs a
  warning; there is no memory limit, stack limit, interrupt handler, or
  preemption, so an infinite loop in `onTick` hangs that script's thread
  permanently.

---

