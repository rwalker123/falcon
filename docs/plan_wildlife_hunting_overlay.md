# Plan: Wildlife & Hunting Overlay

Status: **Largely implemented.** The fauna herd layer (game-as-herds), the hunt/follow/ecology/husbandry
verbs, market hunting, and herd movement (graze-wander / loiter-then-migrate) have all shipped; remaining
deferred items (`Camp`-entity/corrals, wiring the sedentarization hard prompt to `found_settlement`) are
called out inline. This is the authoritative spec for unifying wild game into the fauna system and giving
hunting/gathering their own verbs. Superseded the static `game_trail` food-site flag.

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

## Herd Movement — graze-wander + loiter-then-migrate

> **Status: implemented** (PR #100 — graze-wander + loiter-then-migrate in `advance_herds`; cross-refs
> `docs/plan_exploration_and_sites.md` §2b hunt expedition). Fixed a latent bug and made hunting
> *migratory* game actually possible.

**The bug this fixed.** Previously `advance_herds` called `Herd::advance()` **every turn
unconditionally**, stepping `step_index` one waypoint along the route. But migratory routes
(`build_route`) are a **sparse spiral of waypoints 4–12 tiles apart**, so a migratory herd effectively
**teleported 4–12 tiles per turn** — which is why an equal-speed hunt party could never catch one, and
why the map trail looked jumpy.

**One primitive: graze-wander** (dwell a turn, then step ≤1 tile). Two behaviours built from it, split by
`Herd.size_class`:
- **Wild game** (`Big`/`Small` — deer/boar/rabbit/fowl): *permanent* graze-wander in its local cluster —
  graze the current tile for `dwell_turns` (~1), then step one tile. Effectively half-speed → an
  equal-speed party catches it during a graze turn.
- **Migratory** (`Migratory` — mammoth/steppe-runner/marsh-grazer): alternates two modes —
  - **Loiter**: graze-wander confined to ±1–2 tiles of an **anchor** for `loiter_turns` (many). The
    sparse route waypoints become the loiter anchors.
  - **Migrate**: a directed leg to the *next* anchor at **1 hex every turn, no grazing pause** ("they
    don't stop until their destination"). Requires **densifying** the sparse route into an adjacent
    hex-line path at spawn so migration is 1-hex/turn, not a teleport.

**Hunting works** (mechanically): a party catches the herd **during a loiter** (it's slow), and once
adjacent it **keeps pace through a migrate leg** — both move 1/turn, so it trails one tile behind, still
within reach, following in the herd's wake. A band's *leashed* Hunt still lapses when a herd migrates a
long leg away (graceful — feed entry, workers return) — which is exactly what hunting **expeditions**
are for.

**Config: per-species** on `SpeciesDef` (cadence differs — deer vs mammoth): `dwell_turns` (game/loiter
grazing pause), `loiter_turns [min,max]` (migratory dwell at an anchor), `loiter_radius` (±1–2 local
wander), plus migrate-leg behaviour. `#[serde(default)]` so no config-migration churn.

### Future concepts (documented now, built later)
- **Hunt difficulty = danger, not movement.** Large/migratory animals are hard to hunt because a
  mammoth can *kill your party*, not because it's fast — so mechanically-easy hunting now is intended;
  the challenge lands with the **expedition risk/failure layer** (`docs/plan_exploration_and_sites.md`
  §2b deferred). Do **not** tune pursuit speed to make hunting "hard."
- **Game trails → travel roads.** Historically the first human paths followed game/herd trails. A future
  slice: repeated herd movement accumulates a **trail** on the tiles it crosses that, over time, becomes
  cheaper-to-traverse terrain — an emergent road network feeding movement/logistics. Build the movement
  so the herd's repeated positions are a clean signal a trail-accumulation system can later consume
  (they already are — `advance_herds` visits `position()` each turn). Retire the per-herd "next-position
  heading arrow" in favour of the accumulated trail when that lands; keep the arrow/breadcrumb as-is for now.

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
  `SedentarizationScore` reads.
- **Market hunting.** The depletion-side counterpart to domestication: a **`Market`
  Follow policy** (Sustain | Surplus | **Market** | Eradicate) that commercially
  over-harvests a herd — takes `market.take_fraction × biomass` each turn for a boosted
  trade-goods windfall (`market.trade_goods_multiplier`), declining the group fast into the
  Phase D collapse. Reuses the Follow verb + policy picker + command plumbing (the policy is
  a free string, no schema change). Completes the overlay's "domestication *or* market
  hunting take hold" design.
- **Sedentarization Score.** The first slice of the pastoral→settlement chain and the
  consumer of the domestication seam: an emergent per-faction 0–100 "pressure to settle"
  (`sedentarization_tick`) blending domestication (`domesticated_count`) + surplus + resource
  density + population, EMA-smoothed, firing **soft (~40)** / **hard (~70)** command-feed
  prompts and exported as a HUD meter (`SedentarizationState`). Tunables in
  `sedentarization_config.json`. *Still deferred:* the `Camp` entity + corrals, and wiring the
  hard prompt to an actual `found_settlement`.

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
