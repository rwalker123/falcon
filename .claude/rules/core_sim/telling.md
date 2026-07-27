---
paths:
  - "core_sim/src/telling/**"
  - "core_sim/src/data/beat_*.json"
  - "core_sim/tests/telling*.rs"
  - "core_sim/tests/telling_support/**"
---

<!-- Extracted verbatim from lines 46-47;3569-4003 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# The Telling — the narrative beat engine

## Config files

| File | Purpose |
|------|---------|
| `src/data/beat_definitions.json` | **The Telling** beat catalog — the narrative *content* layer and the mod surface (see "The Telling"). One entry per beat: `id`, `tier` (`ambient`\|`beat`\|`fork`), `soul`, `when` (the predicate grammar), `nouns` (slot → resolver, with `fallback`), `wardrobe` (per-register templates + `fit` + `stance_affinity`), **`choices`** (fork-tier only — per-register `label`/`echo`, `writes.stance`/`writes.flags`, optional `rearm_after_turns`), `gloss`, **`remembers`** (slot → memory-thread kind — see "Memory threads"), `cooldown_turns`, `once`. Loader `telling/catalog.rs`, env override `BEAT_DEFINITIONS_PATH`. **Validated** — `BeatCatalog::validate()` runs inside `from_json_str` (every load path), and is refused at **error** level (`beat_catalog.invalid_rejected`) in favour of the builtin |
| `src/data/beat_config.json` | **The Telling** tunables: `budget` (`max_per_turn` / `global_cooldown_turns`, per tier), **`fork_expire_turns`** (30 — the safety valve, see "The fork tier")), `selection` (`novelty_window_turns`, `novelty_floor`, `fit_soft_tag_weight`, `stance_affinity_weight`, **`stance_affinity_floor`** (0.1 — the re-colouring term's positive floor), `min_selection_weight`), `trend` (`max_history_turns` **16** — must exceed every authored `trend` window, validated at catalog load, `min_delta`), `voice` (`default_register`, `registers`, **`mediums`** — the ordered oral→painted→written ladder, see "The maturing voice"), **`memory.max_threads_per_kind`** (8), `stance.axes` (id → backing signal + range; ids must be unique, and a backing signal may not itself be a `stance.*` signal). Loader `telling/config.rs`, env override `BEAT_CONFIG_PATH`. **Validated** inside `from_json_str`; refused at **error** level (`beat_config.invalid_rejected`) |
## The Telling — the narrative beat engine

Watches sim state, fires **edge-gated** narrative beats, dresses them in nouns drawn from the live
world, and pushes them through the existing `CommandEventLog`. The feed stops saying
"Sedentarization available" and starts saying *"The river-bend remembers us now."* Authoritative
design: `docs/plan_the_telling.md` (§2 schema, §3 runtime); concept `docs/Emergent Narrative.md`.

**Shipped: PR-A (the ambient/beat tiers) + PR-B (the fork tier, stance, and re-colouring) + PR-C
(memory threads, callback predicates, and the maturing voice).** All three tiers are live; see "The
fork tier", "Stance", "Memory threads", "The `answered` gate" and "The maturing voice" below.

### Three layers, and the mod surface is content

| Layer | Home | Moddable | Deterministic |
|---|---|---|---|
| **Engine** — trigger eval, edge gating, fired-set, selection, noun resolution | `core_sim/src/telling/` (Rust) | no | yes, seeded |
| **Content** — souls, wardrobes, conditions | `src/data/beat_*.json` | **yes — this is the mod API** | yes, by construction |
| **Presentation** — how a beat renders | Godot client (generic feed today) | yes | n/a |

### Module layout (`core_sim/src/telling/`)

| Module | Owns |
|---|---|
| `mod.rs` | `BeatLedger`, `telling_tick`, the snapshot round-trip, re-exports |
| `config.rs` | `BeatConfig` + `BeatConfigHandle`/`BeatConfigMetadata` (`sedentarization_config.rs` loader convention) |
| `catalog.rs` | `BeatCatalog` / `BeatDefinition` / `WardrobeEntry` + the load-time validation |
| `predicate.rs` | the `when` grammar and its evaluator |
| `signals.rs` | **the signal registry** — extension point |
| `nouns.rs` | **the noun resolver registry** + template rendering — extension point |
| `select.rs` | weighted wardrobe selection + the determinism recipe |
| `stance.rs` | **stance** — signal normalization, declared offsets, the effective value, the re-colouring term |
| `memory.rs` | **memory threads** — the durable nouns callbacks are made of, their selectors, and eviction |
| `medium.rs` | **the maturing voice** — the medium ladder and its never-regress rule |

### Turn-pipeline placement

**`TurnStage::Telling`, between `Crisis` and `Finalize`** — it must see population, fauna,
sedentarization and crisis output, and must land **before `Snapshot`** so a beat reaches the client
the same turn it fires. `telling_tick` carries **no `run_if` capability gate**: narration is always
on. (The ordering that actually binds is the tuple position in `.configure_sets(Update, (…).chain())`,
not the enum declaration order.)

### The two extension points (the engine/content boundary)

Content **composes** signals and resolvers; it cannot invent them. Adding either is a deliberate
engine change, which keeps the surface auditable and every condition cheap.

**Signals** (`signals.rs`) — sampled to `f64` **once per turn**, at the top of `telling_tick`, into
one `SignalSample`, so every predicate in a turn sees a consistent snapshot and each source is read
once. Sampled for the **player faction** = `FactionRegistry`'s first entry in `FactionId` order
(there is no `player_faction` accessor; `FactionId(0)` is effectively the player).

| Signal | Source |
|---|---|
| `turn.index` | `SimulationTick.0` |
| `band.count` | summed `PopulationCohort.size` over `With<ResidentBand>` — **total people**, not a band count (the name is historical; the content reads "There are {count} of us") |
| `provisions.total` | **band-local larders, summed** — `Σ cohort.stores[FOOD]` over `With<ResidentBand>`. Provisions left `FactionInventory` entirely in the population-economy arc |
| `sedentarization.score` | `SedentarizationScore::score(faction)` |
| `sites.discovered_this_turn` | `DiscoveredSites`' per-faction record count diffed against last turn's, retained in the ledger under the reserved key `internal.sites.discovered_total` (**not** a registered signal — content sees only the diff) |
| `discovery.progress.cultivation` / `.herding` | `DiscoveryProgressLedger::get_progress` on `CULTIVATION_DISCOVERY_ID` (2003) / `HERDING_DISCOVERY_ID` (2004), 0..1 |
| `fauna.collapsing_group_count` | `HerdRegistry::entries()` in `EcologyPhase::Collapsing` |
| `culture.axis.<snake_axis>` | one per `CultureTraitAxis`, off the **faction culture rollup** — `CultureManager::faction_trait_average`, a **population-weighted average of the faction's resident bands' local layers** (`CultureOwner::from_entity`, weighted by `PopulationCohort.size` over `With<ResidentBand>`; expeditions are detached and don't vote), falling back to the global layer when a faction has no local layers. PR-A read the global layer directly because no rollup existed |
| `voice.medium_index` | the narrator's **attained** medium as a 0-based index into `voice.mediums`. Injected by `telling_tick` (not `sample_signals`) right after the stance axes, so it is in the sample before anything evaluates — the `voice.medium_*` beats gate on it with `crosses` |
| `stance.<axis>` | one per configured `stance.axes[]` — the axis's **effective** value, `clamp(normalize(signal) + declared_offset, -1, 1)`. **Config-driven, so it resolves through `BeatConfig`, not the static registry** (`stance::is_stance_signal`); a stance axis may *not* be backed by a `stance.*` signal, which would define its accreted value in terms of itself |

**Noun resolvers** (`nouns.rs`) — each returns `Option<Noun>` (`Named { name, plural, adjective }`
or `Scalar`). `None` is normal early-game. Ties on every scan break by **species name ascending**,
so the result is order-independent.

| Resolver | Behaviour |
|---|---|
| `band.count` | `Scalar` |
| `biome.current_dominant` | the primary band's `current_tile` → `Tile::resource_terrain()` → `TerrainType::as_adjective()` |
| `site.last_discovered` | the faction's most recent `DiscoveredSites` record, named from the sites catalog |
| `fauna.most_hunted` | species with the most workers assigned as a `LaborTarget::Hunt` target |
| `fauna.most_domesticated` | highest `domestication_progress` among herds the faction owns |
| `fauna.most_collapsed` | species of the largest `Collapsing` herd |
| `thread.<kind>.oldest` / `.recent` | the remembered noun with the earliest / latest `first_seen_tick` of that thread kind, ties by `key` ascending. **Registered generically over the kinds the catalog's `remembers` entries declare** — a modder adding a kind needs no engine change. Using one refreshes that thread's eviction clock. See "Memory threads" |

**Fauna word forms are data, not a heuristic.** `SpeciesDef.plural` / `.adjective`
(`fauna_config.json`, both `Option`, defaulting to `display_name`) supply the forms copy needs.
Deliberately **no English pluralisation** — many of these names are already collective ("aurochs",
"deer", "fowl") and a naive `+s` gives "deers". `TerrainType::as_adjective()` (`sim_schema`) is the
terrain equivalent, written out rather than derived from the enum's debug name.

### The `when` grammar (`predicate.rs`)

Combinators `all` / `any` / `not`; leaves dispatch on which keys are present, with a clear error on
an unrecognised shape (content authors will hit this).

| Form | Meaning |
|---|---|
| `{ "signal": S, "gt"\|"gte"\|"lt"\|"lte"\|"eq": x }` | comparison on this turn's sample. `eq` compares within a named epsilon, never `==` |
| `{ "signal": S, "crosses": "rising"\|"falling", "threshold": x }` | **the edge** |
| `{ "signal": S, "trend": "rising"\|"falling", "over_turns": n }` | the sample `n` turns ago vs now, beyond `trend.min_delta` |
| `{ "flag": F }` | a consequence flag (nothing writes one in PR-A) |
| `{ "fired": B, "within_turns": n }` | callback to a prior beat |
| `{ "thread": K, "min_count": n, "older_than_turns": t }` | at least `n` memory threads of kind `K`, each first seen ≥ `t` turns ago. Both bounds default (`1` / `0`), but a *present but malformed* bound still fails at load |
| `{ "answered": B, "choice": C, "min_turns_since": n }` | the player answered fork `B` with choice `C`, at least `n` turns ago (`n` defaults to 0). **The elapsed-time half is usually load-bearing** — see "The `answered` gate" |

> **`Crosses` is the correctness heart of the engine.** It is true **only on the turn the value
> crosses**, computed from the previous turn's stored sample (rising = `prev < threshold <= now`),
> and it **re-arms only after the value falls back**. That is what makes a beat fire once per
> crossing instead of every turn the condition holds. Two consequences that are behaviour, not bugs:
> a signal's **first-ever sample is never a crossing** (there is no `prev` — `opening.cold_open`
> uses `eq`, which is why it still fires on turn 0), and a signal that is **already above its
> threshold the first time it is sampled never fires that beat** until it falls back below.
>
> The bookkeeping order in `telling_tick` is load-bearing: sample → append history → **evaluate
> against the ledger's still-previous `edge_state`** → emit → *then* commit this turn's samples as
> next turn's `prev`. Committing before evaluation would make `Crosses` structurally unfireable.

### Selection, budget, emission

Per turn: sample → candidate filter (in **catalog order**) → resolve nouns → weigh wardrobe →
seeded draw → render → emit.

`weight = fit × novelty × stance_affinity`:
- **fit** — `0` (excluded) if a `requires_noun` slot is unresolved, or the entry is biome-gated and
  the band's ground carries **none** of its tags (an any-of hard gate); else
  `1.0 + fit_soft_tag_weight × matched_biome_tags`. The biome-tag vocabulary is narrative and lives
  in `nouns.rs` (one biome reads as several words a writer would reach for), and an unknown tag is
  a **load-time** failure.
- **novelty** — `1.0` if never used; else ramping linearly from `novelty_floor` back to `1.0` over
  `novelty_window_turns` since last use.
- **stance_affinity** — the **re-colouring** term (`stance::affinity_term`):
  `1.0 + stance_affinity_weight × Σ_axis (affinity[axis] × effective_stance[axis])`, floored at
  `selection.stance_affinity_floor` (0.1). The floor is deliberate: a wrong-stance dressing should
  become *unlikely*, not impossible — the wardrobe pool is small and hard exclusion risks a beat
  with nothing left to dress it in. An entry with **no** `stance_affinity` gets exactly `1.0`, so
  uncoloured beats are bit-identical to PR-A.

Entries below `min_selection_weight` are dropped. **If every entry is excluded the beat emits
nothing and is NOT marked fired** — it can still land later, once the world can dress it.

> **Determinism — the recipe.** Selection RNG is seeded **per decision** as
> `world_seed ^ tick ^ TELLING_SEED_SALT ^ FnvHasher(beat.id)` (the `fauna.rs` per-herd recipe),
> **never a rolling stream**. So a roll is reproducible *and* independent of evaluation order:
> **adding a beat to the catalog cannot perturb another beat's roll.** Pinned by
> `select::tests::selection_is_stable_when_an_unrelated_beat_is_added_to_the_catalog`.

**Budget.** Per-tier `max_per_turn` plus a per-tier `global_cooldown_turns`, on top of each beat's
own `cooldown_turns` / `once`. Per-turn budget counters are **scratch** — recomputed each turn,
never persisted, so a rehydrated ledger starts neutral (the `fauna.rs` convention).

**Command feed.** `CommandEventKind::NarrativeBeat` (`"narrative_beat"`) for the ambient/beat tiers,
and `CommandEventKind::NarrativeFork` (`"narrative_fork"`) for a resolved fork's echo line. The wire field is
already a string, so there is **no schema change and no client change** — an unknown kind renders
generically. `label` = the rendered line; `detail` = the **gloss**: each `gloss` signal id and its
**real sampled value**, plus `tier=…`. That is the concept doc's *"the voice never lies"* proof, so
it must show the actual numbers. Analytics mirror:
`info!(target: "shadow_scale::analytics", event = "telling_beat", beat, wardrobe, tier)`.

> `CommandEventLog`'s bound is **32 entries, drop-oldest**, unchanged in PR-A. Measured on a
> 40-turn probe the engine emits ~8 beats, so ambient narration is **not** the thing that will
> overrun the feed — but it now shares that budget with every command echo, and if the feed starts
> churning, this bound is the first lever to look at.

### Validation — content typos fail at LOAD, not at render

`validate()` runs inside `from_json_str` for **both** files, so every load path (builtin, default
file, env override) is covered; a break is logged at **error** level and the known-good builtin is
used. Rendering is therefore infallible by construction.

Catalog invariants: beat ids unique; wardrobe entry ids unique within a beat; ≥1 wardrobe entry per
beat; the config's `default_register` present in every `voice`; **every `{slot}` / `{slot.field}`
placeholder resolves to a declared noun slot with a legal field** (the single most valuable check
here); every `fit.requires_noun` names a declared slot; every `fit.biome` tag is a known tag; every
signal in `when` and `gloss` is registered; every `from`/`fallback` names a registered resolver.

Config invariants: `novelty_window_turns > 0`; `0 ≤ novelty_floor ≤ 1`; every weight and
`trend.min_delta` finite and `≥ 0`; `trend.max_history_turns > 0`; `registers` non-empty and
`default_register` among them; every `stance.axes[].signal` registered with `range[0] < range[1]`.

`BeatTier` round-trips by string key (`as_str`/`from_key`) with an **unknown key erroring at load**,
per the persisted-enum convention.

### Stance — accreted signal + declared offset

The concept's flow is *accrete → name → ratify or resist → steer*, so an axis has **two** halves and
the engine keeps them apart:

```text
effective_stance(axis) = clamp(normalize(signal) + declared_offset(axis), -1.0, 1.0)
```

- the **accreted value** — what the player's behaviour actually says, read from the axis's backing
  signal (`beat_config.json` `stance.axes[].signal`) and normalized from that axis's `range` to
  `[-1, 1]`;
- the **declared offset** — what the player *said* when they answered a fork, accumulated from each
  choice's `writes.stance` (each write clamped so an offset **alone** can never leave the axis).

> **`BeatLedger.stance` stores ONLY the declared offsets** — the part that is not derivable — and
> the effective value is recomputed every turn in `telling_tick`. **This is what makes *resist*
> representable.** A player whose sedentarization is climbing but who answered "we are the trail"
> carries a negative offset pulling against their own behaviour, and that tension survives; it
> would not if stance were a single stored number, or a bare signal reading. Do not "simplify" the
> two halves into one field.

Each axis is also a readable `stance.<axis>` signal (injected into the sample immediately after the
base signals, *before* any predicate or gloss evaluates), so content can gloss it — the shipped fork
glosses `stance.roam_settle` — and future beats can gate on it. The effective values are also kept
on the ledger as **derived scratch** (`last_effective_stance`, not persisted, excluded from
equality — the `LaborAllocation::last_yields` convention) purely so the snapshot can export them
without re-sampling.

### The fork tier — the decision surface

A `fork`-tier beat carries a **`choices` array**. It does *not* push a line to the feed; it posts a
`PendingFork` onto `BeatLedger.pending` for the faction, which the client renders and answers.

**Posting.** Same candidate filter, noun resolution, weighing and seeded selection as any other
beat, and it spends the fork tier's `max_per_turn` / `global_cooldown_turns` budget identically.
Then:
- **Every configured register is rendered at post time** — narration, each choice's `label`, and
  each choice's `echo`. The register is a **live user toggle**, so storing one rendered string would
  freeze the fork in whichever voice happened to be active when it fired. Rendering all of them also
  pins noun resolution to the moment the fork fired, which is correct: the herd you were chasing
  *then* is what the question is about (and it is why the `echo` is read off the pending fork at
  answer time, not off the catalog).
- The dressing is marked **used** (novelty), because the player is looking at it.
- **The beat is deliberately NOT marked fired.** A fork is fired when **answered** — so one that
  expires unanswered can legitimately re-post. A fork already on the table is not re-asked each turn.

**Answering** — `answer_fork <faction> <beat_id> <choice_id>` (full proto/runtime/text/server
plumbing; `AnswerForkCommand` proto field **40**; `handle_answer_fork`). On a valid answer
(`BeatLedger::answer_fork`, the single path both the command and the expiry valve take):
1. apply `writes.stance` into the ledger's declared offsets (clamped to `[-1, 1]`);
2. apply `writes.flags` into `BeatLedger.flags` (the `{ "flag": F }` predicate's first real writer);
3. **mark the beat fired at the current tick** and record the answer in `BeatLedger.answers`
   (beat id → `Answer { choice, tick }`) — the concept's memory ledger in its smallest useful form,
   so later beats can call back to what was decided **and how long ago**. The expiry valve's
   auto-defer resolves through this same path, so it records a tick too;
4. if the choice carries `rearm_after_turns`, record a re-arm tick that lifts the `once` guard after
   that many turns — the defer branch's "it returns, sharper";
5. drop the fork from `pending`;
6. push the choice's **echo** to the feed under `CommandEventKind::NarrativeFork`, so the answer is
   part of the story record and not a silent state change.

Rejections are distinct and legible: unknown beat id, no pending fork of that beat for that faction,
unknown choice id.

**Expiry — the safety valve.** At the top of `telling_tick`, a pending fork older than
`beat_config.budget.fork_expire_turns` (**30** — three fork cooldowns' patience: generous under a
player taking their time, short enough that an unattended list cannot grow) **auto-resolves to its
defer choice**, through exactly the same `answer_fork` path, with the same echo and the same re-arm
(detail `resolved=expired`). Forks post for **every** faction, AI and unattended ones included, so
without this a fork with no client would sit in `pending` forever and accumulate.

> **The server NEVER blocks turn resolution on a pending fork.** The turn gate is **client-side**;
> the expiry valve is the server's whole answer. There is no gating in the turn queue or `run_turn`,
> and none should be added — a fork posted to a faction with no client would deadlock the game.

**Validation** (at catalog load, so a broken fork never ships): a `fork` beat needs **≥2 choices**
and a non-fork beat **none**; choice ids unique within a beat; `label`/`echo` carry the
`default_register` and their placeholders resolve to declared noun slots; every `writes.stance` axis
and any `soul.fork` names a configured stance axis; and **exactly one choice per fork must be a
defer** (an empty `writes`). That last one is load-bearing: it is the explicit out the client's turn
gate depends on, and a fork without it would trap the player in an unanswerable-without-committing
decision.

### The fork tier on the wire

Unlike `BeatLedger` (sim-only serde), the fork tier **is** client-consumed, so it follows the
per-faction `SedentarizationState` / `DiscoveredSitesState` pattern on **both** `WorldSnapshot` and
`WorldDelta` (append-only, in `CampaignSection`):

```
pendingForks:[PendingForksState{ faction:uint, forks:[PendingForkState{
  beatId, wardrobeId, postedTick,
  narration:[VoiceLine{ register:string, text:string }],
  choices:[ForkChoiceState{ choiceId, label:[VoiceLine], isDefer:bool }],
  gloss:[GlossEntry{ signal:string, value:double }] }] }]
stanceAxes:[StanceState{ faction:uint, axes:[StanceAxisState{ axis:string, value:float }] }]
```

`register` and `axis` are **strings** (the `species` / `policy` convention), so adding a voice
register or a stance axis needs no schema change. **`isDefer` is computed server-side** (the choice
writes nothing) rather than re-derived client-side — the client must not have to know what makes a
choice a defer, and its turn gate depends on the answer. `stanceAxes` carries the **effective**
value, so the client shows what the player's identity currently reads as, drift and declaration
together.

### Memory threads — what the story remembers

The concept's memory ledger: *a small set of durable threads (a rival tribe, a sacred mountain, a
valley you refused) so later beats can call back. **Callbacks are what make a 200-turn emergent game
feel authored.*** Lives in `telling/memory.rs`; held on `BeatLedger.threads` as
`BTreeMap<kind, Vec<Thread>>`.

A `Thread` is `{ kind, key, name, plural, adjective, first_seen_tick, last_referenced_tick }`. The
`kind` is **free-form and content-defined** — the engine has no thread enum.

> **A thread snapshots its noun at first sight and is NEVER re-resolved.** The whole point is that
> the story remembers a thing that may no longer exist — the herd went extinct, the site is four
> hundred turns behind you. Re-resolving would make a callback silently vanish exactly when it would
> land hardest. Pinned by
> `memory::tests::a_threads_noun_is_snapshotted_at_first_sight_and_never_re_resolved` and
> `telling_memory::a_thread_survives_its_source_disappearing`.

- **Writing** — a beat's `remembers: [{ "slot": S, "kind": K }]` promotes slot `S`'s resolved noun
  into a thread of kind `K` when the beat **lands**. "Lands" means *emits*, or — for a fork —
  *posts*: a fork's nouns are pinned at post time for exactly the same reason a thread's are, so the
  two agree by construction. It is an **upsert keyed by the noun's `name`**, never a push:
  rediscovering the same site updates `last_referenced_tick` and leaves the snapshot and
  `first_seen_tick` alone. A `Scalar` noun has no identity to remember and is silently not a thread.
- **Reading** — the `thread.<kind>.<selector>` noun resolvers (`oldest` / `recent`, ties by `key`
  ascending). An empty kind resolves to `None`, which the existing machinery already handles: a
  wardrobe entry requiring the slot is excluded, and a `fallback` chain moves on (the shipped
  `identity.trail_endures` uses a thread as a *fallback* behind `fauna.most_hunted`). Registration is
  **generic over the kinds the catalog declares**, not a hardcoded list.
- **Eviction is by least recently REFERENCED, not oldest first-seen.** A thread the story keeps
  returning to is the one worth keeping; the one nothing has called back to in two hundred turns is
  the one to drop. `last_referenced_tick` is refreshed when a resolver *wins a slot* on a beat that
  lands — a thread merely sitting in a fallback chain that the primary resolver beat does not count.
  Ties by `key` ascending, for determinism. Cap: `memory.max_threads_per_kind` (**8**).
- **Validated at load** — `remembers.slot` must be a declared noun slot and its `kind` non-empty; a
  `thread.*` resolver and a `{ "thread": K }` gate must both name a kind some beat's `remembers`
  actually declares (otherwise they could never resolve or be satisfied).

### The `answered` gate — one sim, two stories

`{ "answered": B, "choice": C, "min_turns_since": n }` is true when the player answered fork `B` with
choice `C` at least `n` turns ago. PR-B stored `answers` and nothing read it; this is what makes it
load-bearing. The fork promised *"the stories that will find you now"*, and `identity.trail_endures` /
`identity.walls_promised` are that promise being kept: **a player who answered `yes_trail` gets a beat
20 turns later that the `no_root` player never sees, out of the same simulation.** Pinned by
`telling_memory::one_sim_two_stories_the_answer_decides_which_beat_finds_you`.

> **`min_turns_since` is the elapsed-time half, and it is not decoration.** A callback almost always
> means *"some time after you said that"* — `identity.trail_endures` says *"we have kept our word to
> it"*, which is absurd the turn after the word is given. **Do NOT reach for a `turn.index` trend to
> express this.** `turn.index` rises unconditionally, so `{"signal":"turn.index","trend":"rising",
> "over_turns":20}` does not mean "20 turns after the answer" — it only ever means "we are past turn
> ~20", and combined with an `answered` gate the beat fires the turn *after* the player answers. Both
> identity beats were authored that way and are now `min_turns_since: 20` with the trend clause gone.
> This is why `BeatLedger.answers` stores an [`Answer { choice, tick }`](#ledger--rollback) rather
> than a bare choice id.

> **A gate on the answer is not enough — the voice must not claim what the sim contradicts.** The
> identity beats gate on the **declared stance still holding**, not merely on the answer:
> `trail_endures` needs `stance.roam_settle < 0`, `walls_promised` needs `> 0`, and
> **`identity.trail_forsaken`** (`>= 0`) is the honest beat for a player who declared the trail and
> then settled anyway — *"No one lied and no one decided — we simply stopped, one season at a time,
> and kept the name."* Without it, elapsed time alone would have the narrator tell a settled people
> *"we have kept our word to it"* — **concept pillar 2, the one thing this layer must never do.**
> `trail_endures` (`< 0`) and `trail_forsaken` (`>= 0`) are exactly complementary, so a player who
> declared the trail always gets **precisely one** of them. Pinned by
> `telling_memory::{keeping,breaking}_the_word_brings_*`, which drive the two regimes explicitly
> (stop driving settlement → `roam_settle` ≈ −0.40; keep driving → ≈ +0.20).

> **The target is validated hard**: the referenced beat must exist, be `fork` tier, and declare that
> choice id. A typo there silently produces a beat that can *never fire* — nothing errors, nothing
> logs, the beat is simply never seen — which is the worst failure mode a content system has. The
> same reasoning added a **`trend` window check**: a `trend` predicate whose `over_turns` is `>=`
> `trend.max_history_turns` can only ever read false, so it is now a load failure too. (It first fired
> on the mis-authored identity beats above; with those fixed the widest authored window is 5, so
> `trend.max_history_turns` stays at its original **16**.)

### The maturing voice — medium

Concept §7: the voice changes **medium** as the civilization crosses milestones — oral saga → painted
chronicle → written record — and *"the narrator maturing is itself a narrative arc that makes
progression felt."* Lives in `telling/medium.rs`.

> **Medium is PRESENTATIONAL and does NOT multiply wardrobe copy.** It changes how the telling
> *looks* to the client and fires a beat when it advances; that is the whole of it. It deliberately
> does **not** select different copy — 4 mediums × 2 registers per wardrobe entry is an **8×**
> authoring cost for the layer's thinnest payoff, and the concept doc names authoring cost as a real
> risk. **Do not "complete" this later by authoring per-medium strings.**

- **Config-driven, reusing the predicate evaluator.** `voice.mediums` is an ordered
  least→most-advanced list of `{ id, when }`; the first entry is the default and needs no `when`,
  and the **highest satisfied** rung wins. Evaluated once per turn in `telling_tick`, right after
  the stance axes and *before* any beat evaluates.
- **Shipped ladder** (`beat_config.json`): `oral` (default) → `painted`
  (`sedentarization.score >= 40` — the soft-drift moment, when a people has been in one place long
  enough to have walls to paint) → `written` (`sedentarization.score >= 70` **and**
  `discovery.progress.cultivation >= 1.0` — a settled people that has learned to farm is the one that
  needs to count a harvest). **The concept's fourth rung, `archive`, is deliberately NOT shipped**:
  the only plausible source for an institutional-archive gate is the `CapabilityFlags` bits, which
  are not registered signals, and adding a signal whose sole purpose is to make a medium reachable
  would be inventing state to satisfy content. Three reachable mediums beat four with a dead rung.
- **It never regresses.** If a signal falls back below its threshold the medium does **not** step
  down — a people that learned to write does not forget — so the attained rung is persisted per
  faction on the ledger and each turn's evaluation takes the max.
- **Readable as `voice.medium_index`**, which is how the authored `voice.medium_painted` /
  `voice.medium_written` beats gate themselves with `crosses`.
- **On the wire** (append-only, `CampaignSection`, both `WorldSnapshot` and `WorldDelta`):
  `voiceMedium:[VoiceMediumState{ faction:uint, mediumId:string, mediumIndex:uint }]`. `mediumId` is
  a **string** (the `species` / `policy` / `register` convention) so adding a rung needs no schema
  change. Threads are deliberately **not** on the client stream — nothing renders them; they stay in
  the ledger's sim-side serde state.
- **Validated at load**: at least one medium, no `when` on the default rung, unique ids, every gate
  on a registered signal; and `memory.max_threads_per_kind > 0` (a zero cap would silently discard
  every thread, making every `thread` gate and `thread.*` resolver permanently unsatisfiable).

### Ledger + rollback

`BeatLedger` (resource) holds `fired` (beat → ticks), `edge_state` (signal → last turn's sample),
`history` (signal → rolling, capped at `trend.max_history_turns`), `wardrobe_usage` (novelty),
`flags`, `stance` (the **declared offsets only**), `pending` (posted, unanswered forks — every
register already rendered), `answers` (beat → **`Answer { choice, tick }`** — the choice *and the
turn it was made on*, which is what `answered`'s `min_turns_since` reads), `rearm_at` (beat → the tick
its `once` guard lifts), `threads` (kind → the durable memory threads) and `mediums` (faction → the
attained voice medium — persisted because it is monotone). `BTreeMap`/`BTreeSet` throughout for deterministic iteration
and stable snapshot ordering, and **`Scalar`, not `f32`**, for everything persisted — sampled as
`f64`, stored fixed-point, the same rollback-bit-exactness argument the rest of the codebase makes.

It round-trips through the rollback snapshot as `WorldSnapshot.beat_ledger`
(`sim_schema::BeatLedgerState`) on the `HerdRegistry` pattern: sim-side serde only, **not** on the
FlatBuffers client stream (beats reach the client as `CommandEvent`s).

> **Capture AND restore.** `restore_sim_state` rebuilds the resource via
> `BeatLedger::from_state`. This is deliberately **not** the `SedentarizationScore` shape, which is
> captured but never restored (`capture.rs` has no rebuild for it — a latent bug; do not copy it).
> A ledger that was only captured would leave a beat marked fired after a rollback past it, so it
> could never fire again, and its stale `edge_state` would make `crosses` misfire. The fork tier
> rides the same round-trip: a fork **answered after the rollback point is un-answered by the
> rollback** — the declared stance offset, the fired mark, the recorded answer and the pending list
> all rewind together, so a player never carries a stance they did not declare in the timeline they
> rolled back into. **PR-C rides it too**: a thread written after the rollback point is
> un-remembered, and a narrator that advanced its medium after the rollback point does not carry that
> medium into a timeline where it never learned it. Guarded by
> `integration_tests/tests/telling_rollback.rs` (all three halves).

See Also: `docs/plan_the_telling.md` (design + PR slicing), `docs/Emergent Narrative.md` (concept),
"Sedentarization" (the rising-edge pattern `crosses` generalises).

Test suites: `core_sim/tests/telling.rs` (ambient/beat), `telling_fork.rs` (the fork tier),
**`telling_memory.rs`** (threads, callbacks, the `answered` payoff, and the medium),
`integration_tests/tests/telling_rollback.rs` (the round-trip).

---

