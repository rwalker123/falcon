# AI opponents — what fills a seat, and how you swap it

**Built on `docs/plan_multiplayer_seats.md`, which is not restated here.** That document says the sim
knows seats and never who fills one. This document is about the things that fill them.

Issue #287 decided the architecture; #645 builds the first one.

---

## 1. There is no AI system in the sim

**An AI is a player process.** It connects to the command and stream ports, claims a seat, receives
that seat's frames, and sends commands. It is the Godot client with a different thing where the human
is.

That is the entire integration story, and it is short on purpose. Everything below happens **inside a
player process**, where the simulation cannot see it and does not care.

```
                 ┌──────────────────┐
   Godot client ─┤                  │
   AI player   ──┤   sim server     │   one command intake, one seat per faction,
   AI player   ──┤  (orchestrator)  │   one frame per seat. No branch on occupant.
   remote human ─┤                  │
                 └──────────────────┘
```

### The crate boundary that makes this compiler-enforced

A new binary crate, `sim_ai`, that depends on **`sim_runtime` only** — the wire types — and **not on
`core_sim`**. It cannot reach into the world because it cannot link the world. "The AI may not read
the simulation directly" stops being a review comment and becomes a build error.

`core_sim` does not depend on `sim_ai` either. Nothing in the workspace connects them but a socket.

## 2. What a player process is

Four stages, and they are the same four in every strategy AI ever shipped. Naming them separately is
what makes a brain swappable, because a new AI usually replaces only one.

| Stage | Here |
|---|---|
| **Perceive** | Decode the seat's frame. The frame **is** the view — there is no second representation of "what this faction can see", and there must not be, because a parallel one drifts from the one the player is served. |
| **Evaluate** | Score what's possible. Personality lives here. |
| **Select** | Pick. |
| **Act** | Emit `CommandPayload`s. |

Plus two cross-cutting things: **personality** (§4), and **commitment** — what stops it re-deciding
from scratch every turn (§4's ⛔).

### The action vocabulary already exists

`sim_runtime/proto/command.proto` defines ~50 named, faction-tagged, serializable actions —
`MoveBand`, `Cultivate`, `HuntFauna`, `AssignLabor`, `BuildOrder`, `SendTradeExpedition` — with a
text form on top in `command_text.rs`. **That is a finished tool API.** The published Civ VI LLM work
had to hand-build "over 70 tools" to reach the same place.

### The brain interface, inside the process

```rust
pub trait Brain {
    fn decide(&mut self, frame: &Frame, rng: &mut impl Rng) -> Vec<CommandPayload>;
}
```

This is a `sim_ai` concern, not a simulation concern. Swapping brains does not touch the server.

Three ship together, because three is the smallest number that proves the plug is real:

| Brain | Purpose |
|---|---|
| `PassBrain` | Submits end-turn. Today's behaviour, named — the control in every comparison. |
| `ScriptedBrain` | Replays a fixed command list. **The test fixture**: it makes AI-driven turns assertable without asserting on utility scores. |
| `UtilityBrain` | The real one. Reads an `AiProfile`. |

An LLM brain, an RL brain, and a half-finished experiment are further impls — or entirely separate
programs in another language that never link `sim_ai` at all. Both are seats; the sim cannot tell.

## 3. Choosing the brain pattern

| Pattern | For | Against |
|---|---|---|
| **Utility** — score candidates, take the best | Personality is *literally a weight vector*. Degrades gracefully; a new consideration is a new term. Decisions are explainable. | No lookahead. Oscillates without §4's commitment term. |
| **Behavior tree / FSM** — ordered fallback | Cheap, predictable, good tooling elsewhere. | **Personality can only gate branches on/off, never tune degree.** Wrong data model for continuous vectors. Ruled out as the primary layer. |
| **GOAP / HTN** — plan toward a goal state | Real multi-step plans, which utility cannot do. | Expensive per turn, hard to debug, needs an action-precondition model we don't have. A plausible later replacement for the goal layer. |
| **Two-layer: slow goal over fast utility** | What the genre converged on. Makes the LLM tractable (§5) and difficulty honest (§6). | Two things to tune instead of one. |
| **Learned (RL / LLM)** | §5. | Not a shortcut to a competent opponent — the research is unambiguous. |

**The two-layer shape, concretely:** a **goal layer** runs every N turns and picks a stance plus a
few priorities ("expand", "consolidate the herd", "go meet people"); a **utility layer** runs every
turn and scores actions *conditioned on that stance*.

It is not a compromise, it is the convergent answer:

- **Civ V/VI** — grand strategy → strategy → operational → tactical, with flavors selecting the grand
  strategy, and flavors mattering "more in the early game, while later the actual game situation
  becomes more important."
- **Stellaris** — personality archetype → behaviors → numeric modifiers → `ai_budget`.
- **AI War 2** — an AI "consciousness" that "approaches strategy at a grand level rather than
  focusing on individual battles", allocating budgets downward.
- **Vox Deorum (Civ V, Dec 2025)** — LLM on macro-strategy, algorithmic AI on tactics.

**Recommended: the two-layer shape, with the goal layer initially a constant stance.** A utility
layer under a fixed stance is a working opponent and commits nothing about the goal layer's design.

## 4. Personality — the model, and the failure

Stellaris's shipped model separates three things that get conflated, and it is worth copying nearly
verbatim:

- **Archetype** — the goal it pursues. A small named set. Discrete.
- **Behaviors** — booleans. *Will* it raid? *Will* it trade? Gates whole action classes.
- **Modifiers** — numeric weights on evaluation. Continuous. Where "warlike 0.8" lives.

As a config file, `ai_profiles.json`, loaded the way the other `core_sim/src/data` config is — except
it belongs to `sim_ai`, because the simulation has no business knowing it exists:

```json
{
  "raider": {
    "archetype": "expand",
    "behaviors": { "will_raid": true, "will_trade": false },
    "weights": { "food_security": 0.4, "land_claim": 0.9, "contact_seeking": 0.2 },
    "commitment": 0.7
  }
}
```

Widening this is how "many vectors" happens: a new weight is a new key, and no seat, socket or
simulation code changes.

### ⛔ Flavors alone produce noise, not character

Civ V's modders scrapped most of the flavor system because it "largely left AI decision-making up to
chance", and flavors "were generally not good at creating long-term planning or predictive behavior
models". Their fix was **hysteresis** — weight the current decision by what the AI has already been
doing, so a civ that has been fighting keeps leaning toward fighting.

That is the `commitment` term above, and it is not optional. A pure weighted sum re-decides from
scratch every turn, so a small change in the world flips its choice, and a player reads that as
randomness rather than personality — then blames the weights, which are innocent. **Personality is
expressed over time, not per turn.** The term goes in v1 or the whole vector system reads as noise.

### Where this sits on the existing pillars

Scoring against genuinely scarce, unequal land is the opponent-side expression of *scarcity drives
the real decision*. Weights that shape choices — rather than a script that repaints outcomes toward a
target — is *emergent, not quota* applied to a rival.

## 5. The LLM

Three findings, pointing the same way.

**Full-game LLM play is not solved.** CivRealm (Freeciv, ICLR 2024): "both RL- and LLM-based agents
struggle to make substantial progress in the full game" — competent in mini-games, lost in the whole
thing. The stated cause is the state-action space, with the observation space growing from 10^15 to
10^650 across eras, and sparse delayed rewards on top. An LLM handed the whole game is not an
opponent.

**The hybrid works, and produces the thing actually wanted.** *Vox Deorum* (arXiv 2512.18564,
Dec 2025) puts the LLM on macro-strategic reasoning and delegates tactical execution to the existing
algorithmic AI. Across **2,327 complete games** against Vox Populi's enhanced Civ V AI it was
competitive — and produced "distinctive playstyles diverging from both traditional AI and each
other". **That is the prize.** Not a harder opponent: a recognisably *different people*, which is
what personalities are for.

**Cost and latency are the deployment blocker**, named in that paper's own abstract. Per-turn,
per-seat model calls in a game with several rivals do not ship.

### So the LLM is the goal layer, never the action layer

§3's slow layer is the socket. It runs every N turns. It returns a stance and a handful of
priorities — tens of tokens, not thousands. It is cacheable, and on failure or timeout it falls back
to the scripted goal picker and loses only flavour.

**And its non-determinism is free.** Because a seat emits commands into the log, a rollback replays
what the LLM decided and never calls the model again. The determinism suites are untouched.

### A custom model, long term

Fine-tuning needs trajectories, and **the command log already records them**. Every human game and
every utility-AI game produces `(state, action)` pairs in the shape a model trains on. Nothing needs
building for this — it needs *not breaking*, and the two properties that keep the option alive are
already architectural requirements: frames are the perception, commands are the action.

## 6. Difficulty is decision quality

Decided now, because deciding it late produces material advantage by default — the rival gets +25%
food and the player correctly reads it as cheating.

| Lever | Easy | Hard |
|---|---|---|
| **Selection noise** | Sample among the top-k scored actions | Take the argmax |
| **Goal cadence** | Re-plan rarely; stale stances | Re-plan often; reacts to the player |
| **View horizon** | A more decayed memory of what it saw | Full fog-legal knowledge |

The first is the elegant one: the AI evaluates *correctly* at every difficulty and only its
follow-through varies, which is how a weak human plays. No bonuses, no fog exemptions, nothing for a
player to correctly call cheating.

All three are levers on one brain. **Difficulty is not a different AI**, and the seat architecture
makes material advantage awkward to add even if someone wanted it — a seat has no private entry point
to grant a bonus through.

## 7. `StartProfileOverrides::ai_profile_overrides` — delete it

Parsed, ships populated (`scout_bias: 0.2`, `camp_rotation_period: 6` in `late_forager_tribe`), read
by nothing.

It is on the wrong axis twice over. `StartProfileOverrides`' own doc comment says *"a profile does
not say who plays the world"* (`start_profile.rs:127`) — a start profile is **per campaign** and AI
tuning is **per seat**, so two rivals in one game would share one `scout_bias`. And under §1 the
simulation should not carry AI tuning at all; it belongs to `sim_ai`, on the other side of a socket.

Replaced by `ai_profiles.json` (§4). Delete the field and the config block, so no future reader
concludes the AI *is* tuned by scout bias.

## 8. Out of scope

Combat behaviour (#369), diplomacy (#231). Both are seat occupants' concerns once they exist, and
neither changes anything here.

---

## Sources

- [CivRealm: A Learning and Reasoning Odyssey in Civilization for Decision-Making Agents](https://arxiv.org/abs/2401.10568) — the dual tensor/language interface over one state, and the full-game failure result
- [Vox Deorum: A Hybrid LLM Architecture for 4X / Grand Strategy Game AI — Lessons from Civilization V](https://arxiv.org/abs/2512.18564) — LLM-on-macro-strategy, 2,327 games, distinctive playstyles
- [Stellaris AI modding](https://stellaris.paradoxwikis.com/AI_modding) — archetype / behaviors / modifiers, and `ai_budget`
- [The Modders Who Decided to Overhaul the AI in Civilization V](https://www.vice.com/en/article/the-modders-who-decided-to-overhaul-the-ai-in-civilization-v/) — why pure flavors failed, and the hysteresis fix
- [CIV5AIGrandStrategies](https://modiki.civfanatics.com/index.php?title=CIV5AIGrandStrategies) — the layered grand-strategy model and how flavors select into it
- [AI War 2: AI Mechanisms](https://wiki.arcengames.com/index.php?title=AI_War_2%3AAI_Mechanisms) — budget-driven grand-level allocation
- [Game AI Planning: GOAP, Utility, and Behavior Trees](https://tonogameconsultants.com/game-ai-planning/) — the pattern trade-offs in §3
