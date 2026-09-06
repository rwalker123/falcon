---
paths:
  - "core_sim/src/band_names.rs"
  - "core_sim/src/data/band_names.json"
  - "core_sim/src/components.rs"
  - "core_sim/tests/band_names.rs"
---

# A band's name: the sim owns it, and it is identity

A band used to have no name. The Godot client fabricated one from a **row number**, in two places
that counted differently — `HudFormat.band_display_name` numbered the expedition-filtered roster
while `MapView` ran its own counter over the raw wire array — so one live expedition made the map
call a band *"Band 5"* while the hunt picker called the same band *"Band 4"*. Worse, a band dying
shifted every row after it, silently renaming bands the player had learned.

**The cure is not a better counter, it is that a name is not a position.** The sim mints a name at
founding, publishes it on every cohort, and carries it through the checkpoint (issue #615).

## The three properties, and where each one comes from

| Property | What makes it true |
|---|---|
| **Durable** — a band's name never changes | It is a `BandName` component minted once at founding and never rewritten. Nothing recomputes it per frame, so nothing about another band can move it. |
| **Deterministic** — a save, a reload and a replay agree | The name is a pure function of `(map_seed, faction, slot)`. See the permutation below. |
| **Unique within a faction** — no two of a faction's bands share a name | `slot` is strictly increasing per faction and `(slot / len, permutation[slot % len])` recovers `slot`, so the map from slot to string is injective. No probing, no collision check, anywhere. |

## The permutation

`BandNameCatalog::name_for_slot` shuffles `0..len` with
`SmallRng::seed_from_u64(splitmix64(map_seed ^ BAND_NAME_SALT ^ faction))` — the repo's
domain-subseed idiom (cf. `PALETTE_SEED_SALT`) — and takes `permutation[slot % len]`. Past the end
of the pool the cycle number is appended as a roman numeral (`Ashfell`, then `Ashfell II`), decimal
above 3999 where roman has no standard spelling. Exhaustion needs 193 bands in one faction; it is
correct rather than merely unlikely.

**The permutation is rebuilt per mint rather than cached.** Minting happens a handful of times per
game, and a cache would be extra checkpoint state earning nothing.

**`splitmix64` lives in `hashing.rs` and there is exactly one of it.** It had been copy-pasted into
`hydrology.rs` and `flora_config.rs`; a mixer is a contract about bits, and three worlds that must
reproduce from a seed cannot each own a copy that could drift by one shift.

## Four founding sites: mint at two, INHERIT at two

| Site | What it does |
|---|---|
| `systems/worldgen.rs` — the campaign's opening bands | **Mints.** Worldgen creates the world, so it opens the name space too: it builds `BandNameAllocator` locally and `insert_resource`s it beside `BandIdAllocator`, for the same reason — a `ResMut` param would oblige every hand-rolled test `World` to remember to install one. |
| `systems/fission.rs` — a splinter | **Mints a fresh name.** A splinter walks out with the parent's food, kit and culture, but the parent is still standing and two living bands may not answer to one name. It is a new band. |
| `bin/server.rs` — a scout party | **Inherits the home band's `BandName`.** |
| `bin/server.rs` — a hunt / trade / raid party | **Inherits the home band's `BandName`.** |

> **A detached party must NOT mint.** A party is the home band's people walking somewhere, not a
> second identity. Minting there would consume a name slot the faction never founded a band for, and
> put two names on screen for one group of people. The party still takes its own **`BandId`** — the
> id is a handle for addressing it in a command, the name is who the people are, and those are
> different questions.

**Both allocators are threaded through worldgen's spawn helpers as one `BandIdentitySource`**, on
`StartKit`'s rationale: four more parameters on four signatures would say the same thing four times,
and keeping the two together makes it structurally hard to hand a band an id and forget its name.

## The checkpoint carries the name AND the counters

`BandRecord::name` and `SimState::band_names`. The counters matter as much as the strings: a restore
that put the bands back but reset a faction's slot would mint an already-issued name for the next
band founded — the aliasing case `BandIdAllocator` documents. `restore_sim_state` merges them under
`BandNameAllocator::restore`'s **no-going-backwards** rule rather than overwriting, so a rollback can
never lower a counter below a slot a band alive in this process already holds.

`core_sim/tests/band_names.rs` drives the real capture/restore path and **kills a band between the
two**, asserting the survivors against the pre-death capture — a positional scheme sneaking back in
fails there.

## On the wire

`PopulationCohortState.name`, appended last (the wire is append-only and field order is the
contract). Always written, even when empty: a field the sim leaves out and a field the sim says is
blank must not be the same frame.

**Empty means the sim has no name for this cohort**, which is only reachable from a hand-built
fixture, and a client renders that as its `Band #<id>` fallback — *never* by counting rows.
`bin/server.rs`'s `every_published_cohort_names_itself_and_a_party_carries_its_home_bands_name`
asserts on the **encoded envelope**, both halves together: every row carries a name, and the party's
equals its home band's. Either half alone is unfalsifiable — "every row has a name" passes on a party
that minted a second one, and "the party matches its home band" passes on a frame where both are
blank.

## Config files

| File | Key | Purpose |
|---|---|---|
| `src/data/band_names.json` | `names` (**193 entries**) | The pool a band's name is drawn from. **Content, not tuning** — curated words evoking a stone-age people and the land they live on, not a syllable-masher. Loads on the shared boot seam (`config-loading.md`) with a `BAND_NAMES_PATH` override. **Entries must be distinct**: the uniqueness guarantee is a property of this list having no repeats, so `BandNameCatalog::from_json_str` returns `BandNamesError::Duplicate` rather than silently deduplicating, and an empty list is `BandNamesError::Empty`. Entries carry no digits — a decimal cycle suffix must not be confusable with a name |

**Editing the list renumbers the world.** File order is the index space the permutation shuffles, so
adding or removing an entry changes which name a given slot resolves to, and a world generated before
the edit reads differently after it.

## See also

- `.claude/rules/core_sim/checkpoints.md` — what a checkpoint carries and the rules it is built by
- `.claude/rules/core_sim/fission.md` — the split verb whose splinter mints here
- `.claude/rules/core_sim/config-loading.md` — the boot seam the catalog loads on
