---
paths:
  - "core_sim/src/orders.rs"
  - "core_sim/src/start_profile.rs"
  - "core_sim/src/data/start_profiles.json"
  - "core_sim/src/bin/server.rs"
  - "core_sim/src/systems/mod.rs"
---

# Factions — who plays the world, and who is allowed to command

`FactionRegistry` (`orders.rs`) is the answer to *"which factions does this world have, and how is
each one driven"*. Everything downstream — the turn queue's await set, the espionage rosters, the
counter-intel budgets, the security policies — is built from its `factions` list, so the registry is
the single place a world's roster is decided.

## Ids are POSITIONAL, never authored

A start profile declares a list of `FactionSpec`s; entry `i` **is** `FactionId(i)`. Nothing in the
JSON names an id.

That is deliberate. An authored id can be written twice or leave a gap, and the registry's `control`
map is keyed by id — a duplicate would silently collapse two factions into one map entry, and a gap
would produce an id that is in `factions` but has no control. With positional ids both are
unrepresentable rather than merely validated against.

```jsonc
// core_sim/src/data/start_profiles.json — top level of a profile object
"factions": [ { "control": "human" }, { "control": "ai" } ]   // human = 0, ai = 1
```

## The registry's invariant, and its one constructor

```rust
pub struct FactionRegistry {
    pub factions: Vec<FactionId>,
    pub control: BTreeMap<FactionId, FactionControl>,
}
```

**`control` is keyed by exactly the ids in `factions`.** `FactionRegistry::new(&[FactionSpec])` is
the only constructor that can produce a non-default registry, and it derives *both* fields from one
declaration list, so the two cannot be written apart; a `debug_assert!` there states the invariant.
Readers ask through `control_of`, `is_ai` and `contains` rather than indexing the map, and an
unregistered faction answers `None` / `false` rather than panicking — a faction nobody declared is
not the sim's to drive.

`FactionControl` is `Human | Ai`. There is no third arm: every faction is either somebody's to play
or something the sim drives. It lives in `start_profile.rs` with `FactionSpec`, because the profile
is where control is *declared*; `orders.rs` imports it.

## Where the roster comes from

- **`build_headless_app`** validates the active profile's roster and then seeds the registry from
  it, before `TurnQueue::new(registry.factions.clone())`. Raising the count therefore extends the
  await set with no further edit.
- **`FactionRegistry::default()`** is one human faction — `FactionId(0)`, `control { 0: Human }` —
  and is what a test harness or any other non-profile construction path gets. It shares
  `start_profile::default_factions()` with the profile layer's default so the two statements of
  "the default world" cannot drift.
- **`StartProfileOverrides` implements `Default` by hand** for exactly this reason. A derived
  `Default` gives `factions` an *empty* `Vec` regardless of the serde `default =` attribute, and an
  empty roster has no faction 0 for worldgen to place, nobody to play and nobody to await. The empty
  roster has to be unrepresentable from every construction path, not only the deserialising one.

### Validation is a boot panic

`StartProfileOverrides::validate_factions(profile_id)` enforces two rules and panics naming the
profile when either breaks:

| Rule | Why |
|---|---|
| the list is non-empty | a world with no factions has nobody to place, play or await |
| at least one entry is `human` | a world nobody plays is not a world |

This is the `config-loading.md` boot rule applied to a profile key: **absent means the builtin
default, present-but-broken stops the boot.** The shipped profile omits `factions` entirely, so the
builtin can never trip it.

## Command authorization

Two gates, at two different distances from the player.

- **Membership, once, at `apply_command`** (`bin/server.rs`). `commanding_faction(&Command)` names
  the faction *issuing* each command — deliberately exhaustive with no `_` arm, so a new verb must
  state whether it is somebody's order or the server's own business. A command whose faction is not
  in the registry is dropped with `warn!(target: "shadow_scale::command", …
  "command.rejected=unknown_faction")` and never reaches its handler. It emits **no**
  `CommandEventFailure`: the feed is per-faction, and filing a failure under a faction that does not
  exist is the exact defect the gate removes (before it, such a command ran to its handler and was
  refused downstream by `no_such_band` / `wrong_faction`, which reads as a real faction that happens
  to own nothing).
- **Ownership, per resolver.** `resolve_starting_unit_entity`, `resolve_expedition_entity` and
  `band_entity_and_tile` all require `cohort.faction == faction`: a band id is a durable, guessable
  handle, so a resolver matching on the id alone would hand a caller another faction's band.
  `road_verb_refusal` *also* checks band ownership and is what produces the message a player reads
  ("Band {id} is not one of your people…"); the resolver's gate is defence in depth behind it, not
  its replacement.

**A faction named inside a payload is not a commanding faction and is not gated.** Espionage verbs
legitimately name another faction as owner or target, and `resolve_shipment` deliberately never asks
faction at all — a cross-faction trade destination works by construction.

## The single-faction assumptions that are still live

Raising a profile's faction count above one puts weight on these four places. They are the
tripwires; each is correct today only because there is exactly one faction.

| Site | What it assumes |
|---|---|
| `systems/mod.rs` `const PLAYER_FACTION: FactionId = FactionId(0)` | Worldgen's starting spawn is hard-wired to faction 0 (`systems/worldgen.rs` — starting units, seeded knowledge, the start location, the cohort filter and the stockpile). **Only faction 0 gets land.** |
| `telling/mod.rs` (signal sampling) | Takes the registry's **lowest id** as "the player" and filters every band view to it; its own comment says there is no `player_faction` accessor. |
| `systems/population.rs` (knowledge migration) | Picks a migration destination as *the first faction that is not the cohort's own*, and the band then **changes sides permanently**. Dead at one faction. See below — this one is not a curiosity. |
| `visibility.rs` `ViewerFaction` | A single **global** resource read by `snapshot/capture.rs`, so one snapshot is captured and broadcast to every connected client. |

### ⛔ The migration picker hands your best bands to a stranger

Of the four, this is the one that changes the game rather than merely limiting it, so it is worth
stating at length. The trigger is a settled band with knowledge and **high** morale:

```rust
cohort.age_turns >= migration_min_settled_turns
    && cohort.morale > migration_morale_threshold      // HIGH, not low
    && !cohort.knowledge.is_empty()
```

and the destination is `registry.factions.iter().find(|&&f| f != cohort.faction)` — the first id
that is not yours. On arrival the knowledge transfers **and `cohort.faction = migration.destination`**:
the band is gone for good.

There is **no check on distance, on contact, or on whether the destination is better off**. So the
first two-faction world takes a player's strongest, happiest, most knowledgeable bands and defects
them to a people that player has never met, from anywhere on the map. It reads as correct today only
because `find` is searching a one-element list and can never return `Some`.

Nothing here is a guard that broke — the behaviour was written this way and has never been
reachable. Whatever puts a second faction on the map owns fixing it in the same change, because
placement is what wakes it. The designed replacement is scouts defecting to a *better-off* faction
they have actually met; the contact tie a gate would need already exists in `ConnectionLedger`.

## Saves win over profile edits

`FactionRegistry` is **ground truth in the save** — a field of `WorldStatics`, captured whole and
restored whole. The start profile, by contrast, is re-resolved from **live config by id** on load,
which then overwrites `SimulationConfig::start_profile_overrides`.

So the two can drift, and the intended rule is that **the save's registry wins**: a world loaded
from a save has the roster it was created with, even if `start_profiles.json` has been edited to
declare a different one since. That is the same reason the rest of `WorldStatics` is saved rather
than recomputed — re-deriving a world's ground truth from tuning that has moved produces a
*different world*.

## A free slot

`StartProfileOverrides::ai_profile_overrides` is parsed and read by nothing. It is where per-faction
AI behaviour tuning would attach when there is an AI to tune.
