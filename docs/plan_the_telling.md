# The Telling — Engineering Plan v0.1

Companion to `docs/Emergent Narrative.md` (the concept doc). That document owns
the *why* and the authorial voice; this one owns the *how* — data model, module
boundaries, and the PR slicing.

Status: **design, pre-implementation.** Schema below is the first concrete
engineering artifact called for in concept §15.3.

---

## 1. Decisions settled before writing this

### 1a. The engine lives in `core_sim`, not the script sandbox

Concept §14 floats the QuickJS sandbox as a possible alternate host. A scan of
the sandbox rules it out:

- It lives **entirely in the Godot native extension**
  (`clients/godot_thin_client/native/src/runtime.rs`); `core_sim` has no script
  code at all.
- Each script runs on its **own OS thread, ticked from Godot's `_process`** —
  framerate-dependent cadence, unsynchronised with sim turns.
- Scripts get the raw QuickJS globals (`Context::full`), including **unseeded
  `Math.random()` and `Date`**.
- **`ui.compose` is declared but unimplemented** — it exists in
  `sim_runtime/src/scripting.rs:147-192` and in the docs, but has no handler arm
  in `handle_host_request`. Calling it today logs "Unhandled host request".

Concept §13 lists determinism as a hard requirement ("narrative RNG must be
seeded like everything else so replays match"). A client-side, framerate-ticked,
unseeded host cannot satisfy it — and would not even hand us the fork UI for
free. **Strike the alt-host line from concept §14.**

### 1b. Three layers, and the mod surface is content

| Layer | Home | Moddable | Deterministic |
|---|---|---|---|
| **Engine** — trigger eval, edge gating, fired-set, selection, noun resolution, stance write-back | `core_sim` (Rust) | no | yes, seeded |
| **Content** — souls, wardrobes, conditions, consequences | `core_sim/src/data/*.json` | **yes — this is the mod API** | yes, by construction |
| **Presentation** — how a beat renders | Godot client; script sandbox optional later | yes | n/a |

The mod surface people want for narrative is *content, not code*. CK3, Fallen
London, and RimWorld all expose declarative event data, not a scripting
language, and their modding scenes are large because of it — the vocabulary is
learnable in an afternoon and cannot hang the game. We ship our own beats in the
same format we hand modders, so the base game dogfoods the authoring API.

Scripting keeps a real but *later* role: presentation-side restyling and Voice
packs, once `ui.compose` exists. We deliberately do **not** build a
script escape hatch for trigger conditions in v1 — ship the predicate grammar,
playtest it, and let the conditions we actually fail to express specify the
hatch. Designing a sim-state API for scripts before knowing which state matters
is speculative work.

### 1c. Stance is its own vector, not a culture-axis alias

Concept §4 says stance "rides on the existing culture trait stack." It cannot,
as written: `CultureTraitAxis` (`core_sim/src/culture.rs:51`) defines exactly 15
axes and **none is roam↔settle** — the axis §10's flagship Fork writes to.

Resolution: **stance is a narrative-layer vector whose axes each name a backing
signal.** Some project onto culture axes, some onto other sim state, some are
narrative-only:

| Stance axis | Backing signal | Substrate |
|---|---|---|
| `roam_settle` | `sedentarization.score` | `SedentarizationScore` 0–100, already thresholded 40/70 |
| `appetite_restraint` | `culture.axis.ascetic_indulgent` | culture axis |
| `open_closed` | `culture.axis.open_closed` | culture axis |
| `sacred_secular` | `culture.axis.secular_devout` | culture axis |

This avoids adding a 16th culture axis (which would ripple through the trait
vector, snapshot, and client) and lets stance axes exist that no culture axis
models. Axis list is config, not code.

---

## 2. The beat definition schema

Catalog lives at `core_sim/src/data/beat_definitions.json`, mirroring
`great_discovery_definitions.json` in shape and load path. A beat:

```json
{
  "id": "opening.long_chase_fork",
  "tier": "fork",
  "soul": {
    "question": "Three seasons of chasing herds — is the trail who we are?",
    "fork": "roam_settle"
  },
  "when": {
    "all": [
      { "signal": "sedentarization.score", "crosses": "rising", "threshold": 40 },
      { "signal": "turn.index", "gte": 12 }
    ]
  },
  "nouns": {
    "beast":  { "from": "fauna.most_hunted",       "fallback": "fauna.dominant_local" },
    "ground": { "from": "biome.current_dominant" }
  },
  "wardrobe": [
    {
      "id": "long_chase.herds",
      "fit": { "requires_noun": ["beast"], "biome": ["steppe", "savanna"] },
      "voice": {
        "mythic": "Three seasons, and each one we chased the {beast.plural} and left the seed-ground unturned. At the fires, they have begun to call us the People of the Long Chase. Is that who we are?",
        "warm":   "Three seasons now, all of them spent following the {beast.plural}, and nobody's turned the seed-ground once. People have started calling us the People of the Long Chase. Is that us?"
      }
    }
  ],
  "choices": [
    {
      "id": "yes_trail",
      "label": { "mythic": "We are the trail", "warm": "Yes — we're trail people" },
      "writes": { "stance": { "roam_settle": -0.4 } },
      "foreshadow": ["hunting_expeditions", "roaming_rights", "seasonal_dilemma"]
    },
    {
      "id": "no_root",
      "label": { "mythic": "We were meant to root", "warm": "No — we were meant to settle" },
      "writes": { "stance": { "roam_settle": 0.4 } },
      "foreshadow": ["fields_granaries", "walls_wardens"]
    },
    {
      "id": "defer",
      "label": { "mythic": "Say nothing", "warm": "Let it lie for now" },
      "writes": {},
      "rearm_after_turns": 15
    }
  ],
  "gloss": ["sedentarization.score", "stance.roam_settle"],
  "cooldown_turns": 0,
  "once": true
}
```

### 2a. `when` — the predicate grammar

Deliberately small. Combinators `all` / `any` / `not`; leaves are comparisons
over a **named signal**:

| Form | Meaning |
|---|---|
| `{ "signal": S, "gt"/"gte"/"lt"/"lte"/"eq": x }` | scalar comparison |
| `{ "signal": S, "crosses": "rising"\|"falling", "threshold": x }` | **edge** — true only on the tick the value crosses |
| `{ "signal": S, "trend": "rising"\|"falling", "over_turns": n }` | sustained direction |
| `{ "flag": F }` | a consequence flag written by an earlier beat |
| `{ "fired": B, "within_turns": n }` | callback to a prior beat (the §4 memory ledger) |

**The signal registry is the engine/content boundary.** Signals are named
accessors implemented in Rust and enumerated in one table; content composes
them but cannot invent them. Adding a signal is a deliberate engine change,
which keeps the surface auditable and every condition cheap to evaluate.

`crosses` is the generalisation of the `sedentarization_tick` rising-edge
pattern concept §14 identifies as the template. Edge state lives in the ledger
(§3), so a beat fires once per crossing rather than every tick the condition
holds — the single most important correctness property in the whole layer.

### 2b. `nouns` — where emergent dressing enters

The freshness pillar (concept §11) only works if an expression can say
"ash-elk" *because this world generated an ash-elk*. So expressions are
**templates**, and `nouns` binds slots to live sim state before rendering.

Each binding resolves to a noun record carrying `{ name, plural, adjective }`
so copy reads naturally. `fallback` chains handle thin early-game state; a
wardrobe entry whose `requires_noun` slot fails to resolve is **excluded from
selection**, so we never render a line with a hole in it.

Resolvers ship as a registry alongside signals: `fauna.most_hunted`,
`fauna.dominant_local`, `site.last_discovered`, `biome.current_dominant`,
`settlement.largest`, etc.

### 2c. `wardrobe` — fit, novelty, stance

Selection weight per concept §4, computed as a product:

```
weight = fit × novelty × stance_affinity
```

- **fit** — 0 if a `requires_noun` slot is unresolved or a hard tag mismatches;
  otherwise scales with how many soft tags match current world state.
- **novelty** — decays for recently-used entries, recovering over a configured
  window. Usage memory is per-save, in the ledger.
- **stance_affinity** — entries may carry `stance_affinity` so a nomad and a
  farmer preferentially hear different dressings of the same soul.

All weights and the novelty window are config, no literals in code.

### 2d. Voice variants

Every player-visible string is an object keyed by register
(`mythic` / `warm`), not a bare string. Per concept §15.1 the register choice is
deferred — carrying both costs one JSON key and makes it a **user-facing
toggle** rather than a design commitment. A missing register falls back to
`mythic` with a load-time warning, so partial mod catalogs still load.

### 2e. Re-coloring, not just unlocking

Concept §6's "key nuance": stance must re-color *shared* events, not only
unlock its own. A wardrobe entry may therefore attach to a beat that any
player hits, gated by `stance_affinity` alone — one herd migration, narrated as
"the great chase calls" or "the beasts threaten the fields." This is the
cheapest characterful win in the design and the schema supports it with no
extra machinery.

---

## 3. Engine runtime

New module `core_sim/src/telling/` with a `BeatLedger` resource, saved and
restored with the sim:

- `fired: HashMap<BeatId, Vec<TurnIndex>>` — the fired-set, for `once`,
  cooldowns, and `fired within` callbacks.
- `edge_state: HashMap<SignalId, Scalar>` — previous-tick values backing
  `crosses` / `trend`.
- `wardrobe_usage: HashMap<WardrobeId, TurnIndex>` — novelty memory.
- `stance: StanceVector` — the player-authored axes.
- `flags: HashSet<FlagId>` — consequence flags.
- `pending: Vec<PendingFork>` — forks awaiting a player answer.

Turn-pipeline stage `telling_tick`, after the systems whose state it reads:

1. Sample all signals; update `edge_state`.
2. Evaluate candidate beats (filtered by tier budget and cooldown).
3. For survivors, resolve nouns; drop beats whose required slots fail.
4. Weight and select a wardrobe entry.
5. Emit: **ambient/beat** tiers → `CommandEventLog` (renders today, no client
   work); **fork** tier → `pending`, surfaced in the snapshot.
6. Apply consequence flags.

**Determinism.** Selection RNG is seeded per decision from
`hash(world_seed, turn_index, beat_id)` — never from a rolling global stream —
so selection is reproducible and, critically, independent of beat *evaluation
order*. Adding a beat to the catalog cannot perturb an unrelated beat's roll.

**Budget.** Per-tier per-turn caps plus global cooldowns (concept §3.6) live in
`beat_config.json`. Ambient is cheap and frequent; forks are rare and interrupt.

---

## 4. PR slicing

Every PR playtestable on its own.

### PR-A — engine + ambient (no new client code) — **SHIPPED**

> **See Also:** the authoritative implementation spec now lives in `core_sim/CLAUDE.md`
> → "The Telling — the narrative beat engine" (module layout, `TurnStage::Telling`
> placement, config levers + invariants, the signal/noun registries as the extension
> points, and the determinism recipe). This document keeps the *design rationale*.


Beat registry, ledger, signal + noun resolver registries, predicate grammar,
catalog loader, seeded selection, and the ambient/beat tiers surfaced through
the existing `CommandEventLog`. Ships with enough opening-arc content to feel
alive.

Playtestable immediately: the feed stops saying "Sedentarization available" and
starts saying "The river-bend remembers us now." Proves the risky half — that
emergent noun dressing produces copy worth reading — with zero UI risk.

### PR-B — the fork tier

The one genuinely new client component (concept §14): a decision surface, the
snapshot field carrying `pending` forks, the choice-submission command path,
stance write-back, and stance re-coloring of shared events. Delivers §10's
opening arc end to end.

### PR-C — voice evolution and callbacks

Medium progression (oral → chronicle → record) tied to capability milestones,
the memory-ledger threads, and callback beats. The payoff layer, once there is
history to call back to.

---

## 5. Open questions for the concept doc

1. **Voice register** (§15.1) — carried as data, deferred to playtest. No
   decision needed to build.
2. **Stance axis list** — the four in §1c are a starting set; the full list
   wants a design pass once PR-A's content exists.
3. **Tent-pole authoring** — §5's top tier is craft, not code. PR-A's catalog
   format should be proven before committing writing effort at volume.
