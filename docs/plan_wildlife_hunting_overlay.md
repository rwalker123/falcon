# Plan: Wildlife & Hunting Overlay

Status: **Design approved, not yet implemented.** This is the authoritative spec
for unifying wild game into the fauna system and giving hunting/gathering their
own verbs. Supersedes the static `game_trail` food-site flag.

## Motivation

Today a food tile carries a **single** `FoodModuleTag { module, kind }`. Worldgen's
"wild game" step does not *add* game to a tile — it **overwrites** the gathering
kind with `FoodSiteKind::GameTrail` (`core_sim/src/systems.rs`), so a tile is
gatherable *xor* huntable. That is unrealistic (a forest has nuts *and* deer) and
it interacts badly with food-site curation: game trails are given a reduced
`seasonal_weight` (×0.75) while curation sorts candidates by weight **descending**
(`compare_food_site`), so game trails are always outranked by their full-weight
gathering siblings and never survive into the surfaced set. Result on a live map:
**0 game trails** despite the config asking for ~24.

## Core idea: game is a short-range herd

The fauna system (`core_sim/src/fauna.rs`) already models mobile animal **groups**
as `Herd { species, route: Vec<UVec2>, step_index, biomass }` that walk a cyclic
route (`advance()`), independent of the tile gather layer. **The route length is
the migratory range.** Wild game is simply a herd with a short route:

| Class          | Range (route length) | Examples            | Interaction         |
|----------------|----------------------|---------------------|---------------------|
| Migratory herd | long (cross-region)  | mammoth, aurochs    | Follow, then Hunt   |
| Big game       | ~2–3 tiles           | deer, boar          | Hunt (Follow opt.)  |
| Small game     | ~1 tile (loiters)    | rabbit, fowl        | Hunt (Follow opt.)  |

Because fauna already lives independently of the gather layer, a `forest_forage`
tile with a deer group walking across it *already* offers both — we just have to
populate the overlay and expose the verbs. This retires `game_trail` entirely and
**deletes the curation bug** (game is no longer a curated food-site competing for
slots).

Model game as **groups, not individuals** (one entity = a deer band / rabbit
warren / mammoth herd; `biomass` = group size) so entity counts stay bounded.

## Three verbs, decoupled

- **Harvest** — the tile's gather module (plants/fish/shellfish). Unchanged.
- **Follow(policy)** — band pursues a fauna group and stays within 1 tile as it
  moves. Positioning, plus an **auto-hunt policy** applied each turn once adjacent:
  - **Sustain** — take only enough to feed the band; `take ≈ regrowth`, group ~stable.
  - **Surplus** — take extra → provisions/trade goods to sell; group slowly declines.
  - **Eradicate** — take maximum → drive the group toward local extinction (the
    American-Buffalo scenario).
- **Hunt** — a single portion, then release. Opportunistic food without committing
  a band to follow.

**Pursuit:** both verbs pursue the group, re-targeting its current tile each turn
until within **1 tile** ("close enough"), then resolve. Following keeps that
distance ≈ 0 so a subsequent Hunt is immediate; hunting an un-followed migratory
animal means chasing it down first.

**Portion:** how much a hunt takes is driven by the player's selection (the Follow
policy, or a Hunt intensity). This is the substrate for the long-term overhunting
arc: `take > regrowth` over time → collapse → local extinction.

## Ecology

- `biomass` draws down on each hunt and **regrows** per turn toward a per-species
  carrying capacity (logistic). Hitting 0 → the group disperses/despawns (local
  extinction).
- **Abundance is a config value, high to start.** Design intent: game is plentiful
  early and thins as population pressure / overhunting grow, unless domestication /
  industrialized farming & hunting take hold (future arc). Respawn/immigration rate
  is tunable so early forager play is game-rich.

## Implementation phases

Each phase is independently shippable.

- **Phase A — game exists and is visible.** Expand `HerdSpecies` into a data-driven
  **species table** (display, icon key, range, group biomass, host biomes,
  `migratory` flag, size class). Spawn game as short-route groups by biome density
  from a new `core_sim/src/data/fauna_config.json` (abundance high). Retire the
  `game_trail` upgrade + `FoodSiteKind::GameTrail`. Client already renders herd
  species icons (`FoodIcons.for_herd`), so game appears immediately. *This alone
  fixes "no game on the map."*
- **Phase B — Hunt (one-shot).** Generalize hunting to target a fauna **group**
  (not a tile): band pursues to ≤1 tile, takes a portion of biomass → provisions/
  trade via existing food-yield config; add biomass regrowth. New assignment
  component (e.g. `FaunaPursuit { fauna_id, mode }`) reusing the
  `HarvestAssignment` travel/work machinery.
- **Phase C — Follow + policy.** Generalize `FollowHerd` → `Follow { fauna_id,
  policy: Sustain|Surplus|Eradicate }`: pursue-to-adjacent, then auto-hunt per
  policy each turn. Follow keeps a small non-food benefit (tracking / fog pulse /
  Fauna Lore) but the food comes from the policy's take.
- **Phase D — ecology.** Overhunting → collapse/extinction when sustained take >
  regrowth; abundance/regrowth tunables; scaffolding for the later domestication /
  industrialized-hunting arc.
- **Phase E — domestication (husbandry core).** The pastoral counter-force to
  depletion. A **sustained Sustain-follow on a `Thriving` herd** accrues husbandry
  progress on the herd (emergent), decaying when untended; at full progress — or via an
  explicit `domesticate` command that claims it early once past `claim_threshold` — the
  herd becomes **domesticated**: owned by that faction, yielding steady provisions each
  turn (proportional to biomass, without depleting it) and **immune to the overhunting
  collapse**. `HerdRegistry::domesticated_count` is the value the future
  `SedentarizationScore` reads. *Deferred beyond E:* the **industrialized / market-hunting**
  counterpart, and the pastoral→corral→settlement chain (`Camp`, `SedentarizationScore`).

## Cross-cutting touchpoints

- **Worldgen / ECS** (`fauna.rs`, `systems.rs`): species table, short-route game
  spawning, roaming (reuse `advance_herds`), pursuit + biomass dynamics, remove the
  `game_trail` upgrade.
- **Schema** (`sim_schema`): fauna state gains `size_class` / `biomass` /
  `huntable` / `followable` so the client can offer the right verbs; drop
  `game_trail` from the food-module contract.
- **Server commands** (`bin/server.rs`): generalize `FollowHerd` to carry a policy,
  add a fauna-targeted `Hunt`, retire tile-based `HuntGame` / `game_trail`.
- **Client** (`clients/godot_thin_client`): selection panel shows **Harvest /
  Hunt / Follow** as applicable — including when a hex has both a gather module and
  a fauna group (fix the current "entity selection shadows tile selection" so all
  applicable verbs surface); a Follow **policy picker**; Hunt/Follow reuse the
  targeting-mode banner/reticle. Species icons already handled.
- **Config**: `fauna_config.json` — abundance per biome, species table params,
  regrowth rates, policy draw rates, pursuit radius (=1).

## Open items / defaults

- Policy names default to **Sustain / Surplus / Eradicate**.
- Pursuit "close enough" = **1 tile**.
- Follow retains a *small* non-food benefit (not a dead prefix to Hunt), but all
  food comes from the hunt take.

## See Also

- `core_sim/CLAUDE.md` — Ecosystem Food Modules (gather layer), fauna system.
- Manual §"Start of Game — Nomadic Default" — Follow Herd (current interim
  behavior) and the forager loop this feeds.
