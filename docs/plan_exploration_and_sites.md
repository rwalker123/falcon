# Plan: Exploration & Wondrous Sites

Status: **Design approved; partially implemented** (local scout shipped — see §1; scouting
expeditions and Wondrous Sites not yet implemented). The exploration layer of the early game:
**local scouting** (forward-observer vantages that see around obstacles), **scouting expeditions** (a provisioned
traveling party that explores distant territory and returns), and the **Wondrous Sites**
subsystem — a data-driven catalog of things worth finding that tiles can hold and that
exploration discovers. Sites are the *reason* to scout and to send expeditions. Companion to
`docs/plan_early_game_labor.md` (the Scout role lives there); this doc owns the exploration
actions and the discovery subsystem.

## Motivation

Scouting was shipped as a labor role but is a **no-op**: the Scout role queues a radius-2 fog
pulse while the band already passively sees radius 6 (`visibility_config.json` BandScout
`base_range` 6), and it doesn't scale with the number of scouts. So staffing scouts does nothing
observable. And more deeply: exploration had **nothing to find** — the fog hid only terrain, not
*things*. Both are fixed here.

Two distinct exploration actions, and the subsystem that gives them a point:

## 1. Local scout — forward observers that see *around* obstacles (a fix, not a new system)

> **Status: implemented (vantage model).** Staffed scouts are **forward observers**: with ≥1 scout,
> `calculate_visibility` posts vantage points out from the band in all six hex directions and
> computes line-of-sight from each, so scouts reveal *around* ridges/forest — not merely farther.
> Config in `labor_config.json` `scout` (`vantage_distance_base`, `vantage_distance_per_scout`,
> `vantage_distance_max`, `vantage_range`). The old radius-2 `FogRevealLedger` pulse and
> `reveal_radius`/`reveal_duration_turns` are retired, as is the earlier pure range-bump
> (`sight_bonus_per_scout`/`max_sight_bonus`). The snapshot `scoutRevealRadius` field now carries the
> effective **vantage distance** (0 with no scouts). The client Scout row hint reads "Posts scouts
> that see around obstacles — more scouts range farther."

The earlier "just add to `base_range`" fix was insufficient: visibility still cast LOS from the
single band-center hex, so the extra range was eaten by any blocking ridge/elevation between the
band and the frontier — scouts saw "slightly more," nothing past a ridge. Extending the radius from
one vantage point cannot see around obstacles.

The Scout role instead **posts forward observers**: with scouts staffed, place vantage tiles *out*
from the band and compute the band's normal LOS reveal **from those tiles**, unioned into the
faction visibility map and re-marked **Active** every turn while scouts are assigned. The band's own
base-range LOS from its center is unchanged; scouts are additive.

- Mechanism (in `calculate_visibility`, reading the cohort's `LaborAllocation` Scout head-count):
  - **Vantages ring the band in all 6 hex directions** (perimeter awareness — the standing role,
    not an aimed expedition). More scouts push the ring farther: `vantage_distance =
    min(vantage_distance_base + scouts × vantage_distance_per_scout, vantage_distance_max)`.
  - **Placement** steps `vantage_distance` tiles along each hex direction (reusing `hex_neighbor`),
    **pulling back** to the last on-map, passable (non-water) tile so foot scouts stop at
    ocean/edge; a boxed-in direction collapses to the band tile (no-op).
  - **Each vantage reveals with `vantage_range`** via the *same* per-source LOS reveal
    (`reveal_tiles_in_range`, elevation/terrain modifiers included) the band already uses.
  - Config levers in `labor_config.json` `scout` (`vantage_distance_base` 2, `vantage_distance_per_scout`
    1, `vantage_distance_max` 6, `vantage_range` 2). The old `reveal_radius`/`reveal_duration_turns`
    and the range-bump levers are obsolete.
- Client: the Scout row hint changes to "Posts scouts that see around obstacles; more scouts range
  farther."
- Small, no new UI; completes the Scout role that Early-Game Labor shipped inert.

### 1a. Worked sources provide visibility (implemented)

Scouting isn't the only source of sight. A band's **workers are physically out at the sources they
work** — foragers stand on the forage tile, hunters are at the herd — so those spots should reveal
fog too, just like the band center and scout vantages. `calculate_visibility` adds, **for each
assignment** in the cohort's `LaborAllocation`, a worked source tile: a **Forage** assignment's
`tile`, or a **Hunt** assignment's herd's **current tile** (resolved live from `HerdRegistry`; an
unresolved/extinct herd is skipped). Each reveals at `worked_source_sight_range` (`labor_config.json`,
default 2) via the *same* `reveal_tiles_in_range` LOS path — additive to the band center + vantages,
re-marked Active every turn while staffed. Scout/Warrior are band-wide roles, not tile sources.

## 2. Scouting expedition — a provisioned party that goes out and comes back

The Lewis-and-Clark action: a deliberate, outfitted venture, distinct from the standing role.

- **Outfit before sending:** allocate a party of workers (they leave the band's labor pool for
  the duration) **plus provisions** it carries (drawn from the larder, scaled by party size ×
  distance). *Scouting-TOE (wayfinding gear) that improves range/speed/safety lands with the TOE
  slice — not v1.*
- **A visible traveling party:** a lightweight detached entity with its **own map marker** that
  treks toward a target/direction, **revealing fog along its actual path** (Active as it passes,
  remembered after), reaches its objective, and **returns to the band**. Reuses move-band's travel
  stepping and the `FogRevealLedger`/knowledge-fragment machinery from the retired follow-herd;
  it is the **temporary cousin of the deferred breakaway-to-new-band** (same detached-party
  machinery — one returns, one settles), so building it de-risks that later split feature.
- **Returns with findings — deterministic from where it goes.** An expedition reveals whatever
  Wondrous Sites actually lie along its path/objective — exploration is **skill** (aim it well,
  uncover the real map), not a slot machine. A *small* random flavor find may ride on top, but the
  core prize is the real sites it uncovers. Plus the permanent map reveal of everything it crossed.
- **Deferred but documented:** **risk/failure** (peril — losing members to starvation/threats,
  an expedition that never returns) is a great later layer; v1 is deterministic success.

### 2b. Hunting expedition — a long-range hunt that follows migratory game (deferred, documented)

The hunting analogue of the scouting expedition, and the answer to migratory herds the **leashed
follow can't reach**. Today a Hunt assignment lapses once the herd roams past `band_work_range +
hunt_leash_tiles`. A hunting expedition instead sends a **detached hunting party** that *keeps
following* a migratory herd far from the band, accumulates food, and **drops it off** at the band —
so you can exploit a herd's whole migratory circuit instead of only what wanders into work range.

- **Same machinery as the scouting expedition** (§2): a visible detached party (own marker) that
  travels, using move-band stepping + the detached-party model. Build it as part of the expedition
  work — one traveling-party system, two verbs (scout / hunt).
- **Follows the herd** beyond the leash, taking food each turn it's in reach (reusing the per-turn
  Hunt take math), and **carries** what it takes up to a **carry capacity** ("full").
- **Drops off food at the band when EITHER:** (a) the herd's migratory circuit brings it back
  **near the band** (party is close enough to run the food home), OR (b) the party is **full**
  (carry capacity reached) — at which point it returns, deposits into the larder, and (design
  choice at build time) either re-launches or ends.
- **Open questions to settle at build time (not now):** does the party carry **provisions** for
  itself (like the scout expedition) or live off its own kills; is "full" tied to the same
  **carry-capacity** concept as the band; does it auto-relaunch on drop-off or require re-issue;
  and **risk/failure** (a party that starves or is lost) — deferred like the scouting expedition's.

## 3. Wondrous Sites — the data-driven catalog of what exploration finds

The hub subsystem. Exploration is the input; config-defined rewards fan out into settlement,
resources, diplomacy, and culture — without coupling those systems together.

### Built for "add easily"
- **Catalog** (`sites_config.json`, same loader pattern as `fauna_config.json` / food modules):
  each entry `{ id, category, display/icon, placement_rule, discovery_reward }`. Adding "Salt
  Flats" or "Ancient Ruin" is a new row, no code.
- **Tiles hold a site** — an optional site reference on the tile (new schema field + component),
  hidden under fog until discovered. **This is the core new tile concept.**
- **Generic discovery** — *any* vision source reveals sites in range: the band's passive sight,
  local scout, or an expedition passing through. On reveal → a **discovered-sites registry** →
  map marker + a **Discoveries** readout. One mechanism, all sources feed it.
- **Per-category reward hooks** — config-driven per category, the seam that lets sites touch every
  other system:

| Category | Discovery reward (feeds…) |
|----------|---------------------------|
| **Settle site** | a flagged good-to-root spot → the sedentarization/settlement arc |
| **Riches** | a known resource concentration → the resource system |
| **Tribe** | a diplomacy contact → future civilizations |
| **Landmark** (Rockies, Great Lakes) | morale / culture / naming → identity & flavor |

### Design decisions
- **Point sites in v1; regions later.** Riches / ruins / settle-sites are naturally **single-tile
  points**. Landmarks like the Rockies/Great Lakes are inherently **multi-tile regions** — v1
  treats a landmark as a **named point on its most prominent tile** (the peak, the lake center);
  **regional footprints** (a named area spanning tiles) are a documented later extension.
- **Placement origin differs per category — accepted heterogeneity.** Landmarks are **emergent
  from terrain** (detect a prominent range/lake at worldgen → name it); riches from **resource
  deposits**; settle sites **derived from habitability + food**; tribes **seeded like player
  starts**. Placement is a per-category rule in the catalog; some computed at worldgen, some seeded.
- **Deterministic finds** (see §2): a site exists on the map before it's found; exploration
  reveals it. The map is worth knowing.

## Sequencing

The sites subsystem is the **foundation** — it makes even normal exploration (moving the band,
local scout) pay off, before expeditions exist.

1. **Local scout** (§1) — the small fix; makes the Scout role real. *Next.*
2. **Wondrous Sites (minimal)** (§3) — catalog + tile-hold (schema) + discovery-on-vision + a
   Discoveries readout, with 2–3 seeded site types to prove the loop. Immediately valuable.
3. **Scouting expedition** (§2) — the mobile discovery vector: the visible traveling party +
   provisioning, layered on the sites discovery.
4. **Deferred / documented:** expedition **risk/failure**; scouting-**TOE** gating;
   **regional (multi-tile) sites**; richer per-category rewards; **tribes as real civilizations**.

## Cross-cutting touchpoints

- **Visibility** (`visibility_systems.rs`): local scout posts forward-observer vantages (LOS from each); expeditions
  and sites hook the reveal path (`FogRevealLedger` / `FactionVisibilityMap`).
- **Tiles / schema** (`sim_schema`): the per-tile site reference + the discovered-sites registry,
  wired through the snapshot like other tile fields; a new `sites_config.json` + loader.
- **Worldgen** (`mapgen.rs`): emergent landmark placement (prominent ranges/lakes), riches from
  deposits, settle-site derivation.
- **Client**: site map markers (fogged until found), a Discoveries readout, the expedition party
  marker + its outfit UI, the updated Scout hint.
- **Arcs it feeds:** settlement (settle sites), resources (riches), diplomacy (tribes), culture
  (landmarks), TOE (scouting gear), and the deferred breakaway/split (shared detached-party).

## See Also

- `docs/plan_early_game_labor.md` — the Scout labor role (local scout is its completion); the
  band/labor-pool model expeditions draw workers from; the TOE slice that later gates scouting gear.
- `docs/plan_settlement_population.md` — settle-site discoveries feed the emergent-settlement arc.
- `docs/plan_wildlife_hunting_overlay.md` — the fauna-config/loader pattern the sites catalog mirrors.
- `core_sim/CLAUDE.md` — Visibility Systems (sight ranges, fog states), the config-loader pattern.
- `clients/godot_thin_client/CLAUDE.md` — fog rendering (Active/Discovered/Unexplored), the
  allocation panel Scout row.
