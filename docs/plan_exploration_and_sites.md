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
  treks toward a target. It is a **persistent detached unit you drive** — aimed via the *reused
  move-band click flow* (Hud pending-move → destination click on the expedition entity). Reuses
  move-band's travel stepping; it is the **temporary cousin of the deferred breakaway-to-new-band**
  (same detached-party machinery — one returns, one settles), so building it de-risks that later
  split feature.
- **Discovery is gated by communication range — the expedition *reports*, it doesn't live-stream.**
  The Columbus/Lewis-and-Clark beat: an expedition carries a **communication range** (a flat config
  lever, default 2 tiles early game, with a stubbed tech-scaling hook — no comm/tech signal exists
  yet, mirroring migration's movement-tech `TODO(phase2)`). While **in comm range** of the band its
  findings are known; **out of comm range** it keeps exploring but the faction learns nothing until
  it comes **back into range to report** — the map lights up as a lump on return. Mechanically the
  expedition is **not** a live faction vision source (`Without<Expedition>` in `calculate_visibility`);
  each turn it accumulates the tiles it observes (reusing `reveal_tiles_in_range`) into a **private
  pending-reveal buffer** on the `Expedition` component, and `advance_expeditions` **flushes that
  buffer into the faction map as `Discovered`** (remembered, not `Active` — you read a report, you
  aren't standing there) whenever it is within comm range of the band. **Site discovery rides the
  existing path for free**: once the flush marks tiles `Discovered`, the normal `discover_sites`
  (Visibility stage) records any `SiteTag` on them + fires the usual feed/reward. In v1 you still
  see and command your *own* expedition marker regardless of range — comm-range gates **world
  discovery**, not the expedition's own status (fate-uncertainty is the deferred risk layer, below).
- **Arrival is a decision point, not an auto-turnaround.** On reaching its objective the expedition
  enters an *awaiting-orders* state (marker state + a feed line, "Expedition reached X — awaiting
  orders" — you command your own unit, so this fires regardless of comm range). The player then
  **sends it onward to a new target** (chain waypoints; deciding *without yet knowing what's out
  there* if it's beyond comm range — the core tension) or **orders it home**. Left alone it waits.
  "Return" targets the band's **live** tile (the band is nomadic), and on arrival home the workers +
  leftover provisions fold back into the band, and the pending-reveal buffer makes its final flush.
- **Returns with findings — deterministic from where it goes.** An expedition reveals whatever
  Wondrous Sites actually lie along its path/objective — exploration is **skill** (aim it well,
  uncover the real map), not a slot machine. A *small* random flavor find may ride on top, but the
  core prize is the real sites it uncovers. Plus the permanent (`Discovered`) map reveal of
  everything it crossed — delivered when it reports back in comm range, not live along the path.
- **Provisions deplete.** The party carries larder-drawn provisions (scaled by party size ×
  distance) that **drain per turn**. In v1 running dry is **non-fatal** (deterministic success), but
  the number is live — and a party that runs low can **opportunistically replenish** off game it
  passes (see §2b; the replenish primitive lands with the hunt verb and is retrofitted here). This
  is the seam the deferred **risk/failure** layer plugs into (can't replenish, runs out → peril).
- **Deferred but documented — risk/failure + knowledge as dowry.** v1 is deterministic success; the
  peril layer lands later and plugs into the *same* comm-range/pending-reveal seams v1 builds:
  - **Dies out of contact → its knowledge is lost.** Death is simply "despawn without flushing the
    pending-reveal buffer" — everything it saw beyond comm range was never reported, so it never
    reaches the faction map. (In-comm losses have already reported.)
  - **Defects / founds a new tribe / joins another civ → that tribe inherits its parent's discovered
    (not active) view.** Knowledge travels with the people: the same flush operation, aimed at a
    *different* faction's map (the new/joined tribe gains the parent's `Discovered` knowledge as of
    when the party left). This is the deferred **breakaway/split** (an expedition that drops
    `Expedition`, gains `ResidentBand`, and keeps its map) — needs other civilizations (tribes
    unbuilt) and the breakaway machinery, so it is target-concept only. v1 shapes the seams
    (pending-reveal buffer, per-expedition observation, the flush-to-a-faction-map operation) to
    accept it without a rewrite.
  - **Fate-uncertainty** (you lose contact and don't know if they live or will return) is part of
    this layer; v1 keeps the player's own expedition marker/status visible and gates only discovery.

### 2b. Hunting expedition — a long-range hunt that follows migratory game (resolved; PR 2)

The hunting analogue of the scouting expedition, and the answer to migratory herds the **leashed
follow can't reach**. Today a Hunt assignment lapses once the herd roams past `band_work_range +
hunt_leash_tiles`. A hunting expedition instead sends a **detached hunting party** that *keeps
following* a migratory herd far from the band, accumulates food, and **drops it off** at the band —
so you can exploit a herd's whole migratory circuit instead of only what wanders into work range.

- **Same machinery as the scouting expedition** (§2): a visible detached party (own marker) that
  travels, using move-band stepping + the detached-party model. Built as PR 2 on PR 1's system —
  **one traveling-party system, two verbs** (scout / hunt).
- **The shared primitive is "take food from a nearby source."** The hunt verb does it *continuously
  and deliberately*; the scout does it *opportunistically* when its provisions run low and it passes
  game (the real-life hunt-while-traveling). PR 2 introduces the primitive and applies it to both
  verbs — so the scout's replenish and the hunt's harvest are the same code.
- **Follows the herd** beyond the leash — retargeting travel to the herd's current tile each turn —
  taking food each turn it's in reach (reusing the per-turn Hunt take math into its own store), and
  **carries** what it takes up to a **carry capacity**.
- **Drops off food at the band when EITHER:** (a) the herd's migratory circuit brings it back
  **near the band** (party is close enough to run the food home), OR (b) the party is **full**
  (carry capacity reached) — at which point it returns to the band's live tile, deposits into the
  larder, and **auto-relaunches** back to the herd.

**Resolved decisions (were open questions):**
- **Provisions vs. kills:** the hunt verb **lives off its own kills** — no separate provision
  concept; its carried food is also its sustenance (one `LocalStore` under the reuse model). Only the
  scout verb carries larder-drawn provisions (it has no food source), which deplete and can be
  topped up via the shared replenish primitive.
- **"Full":** its own lever, **`party_workers × per_worker_carry`** (mirrors the existing Hunt
  `per_worker_biomass_capacity` idiom) — *not* tied to the band carry-capacity cap, which is an
  unbuilt slice.
- **Drop-off behavior:** **auto-relaunch** — loops the herd's whole migratory circuit until the
  player **recalls/disbands** it or the herd goes extinct.
- **Risk/failure:** **deferred**, deterministic success in v1, same as the scouting expedition; the
  depleting-provisions / can't-replenish seam is where it plugs in later.

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
3. **Scouting expedition** (§2) — **PR 1**: the traveling-party system + the scout verb (the mobile
   discovery vector — visible detached unit + provisioning, layered on the sites discovery).
4. **Hunting expedition** (§2b) — **PR 2**: the second verb on PR 1's system + the shared
   take-food-from-a-nearby-source primitive (hunt harvest + scout opportunistic replenish).
5. **Deferred / documented:** expedition **risk/failure**; scouting-**TOE** gating;
   **regional (multi-tile) sites**; richer per-category rewards; **tribes as real civilizations**.

### Implementation model (expeditions)

An expedition is **another `StartingUnit` band** — it reuses `PopulationCohort` + `BandTravel` /
`advance_band_movement` + `LaborAllocation` + `StartingUnit`, tagged with a new **`Expedition`**
marker and (deliberately) **lacking `ResidentBand`**. Carrying `StartingUnit` is required, not
incidental: `calculate_visibility` gates its vision sources on `StartingUnit` (visibility_systems.rs),
so the marker is what makes the party reveal fog (and thus discover sites) for free. Consequences of
being a full `StartingUnit` band: it's a moving snapshot marker for free, it's a `Hunt`-labor food
producer for free (the hunt verb), and **retargeting a waypoint is just `move_band` on the expedition
entity** — the only genuinely-new commands are `send_expedition` (spawn + outfit: draw N workers off
the band's `working` and provisions off its larder) and `recall_expedition` (set phase → Returning +
fold back on arrival).

**Isolation via a positive `ResidentBand` marker.** To keep expedition cohorts from bleeding into the
*growing* settlement/population arc, real bands get a `ResidentBand` marker and the systems that must
*not* see expeditions filter `With<ResidentBand>`:
- **Demographics** (`simulate_population`), **migration** (`advance_population_migration`),
  **sedentarization** (`sedentarization_tick`), **startup seeding** (`apply_starting_inventory_effects`),
  **supply network** (`balance_supply_networks` — an expedition manages its own larder; drop-off is the
  explicit fold-back, not a passive supply-network leak), and the **default-band command pickers**
  (`select_starting_band` / `select_founder_band` `None`-bits branch — so a band-less command never
  auto-grabs an expedition).
- **Stays bare** (expeditions *included*): `advance_band_movement` (travel), `advance_labor_allocation`
  (labor/food — the hunt verb; a scout's empty allocation is a harmless no-op), `capture_snapshot`
  (marker), `collect_metrics`, `discover_sites` (rides the flushed `Discovered` tiles).
- **Special case — `calculate_visibility` (`Without<Expedition>`).** The expedition keeps
  `StartingUnit` (for `move_band` retargeting + selection) but is **excluded from live faction fog
  reveal** — comm-range gating means it must *not* light up the faction map from wherever it stands.
  Instead it observes into its own **pending-reveal buffer** and `advance_expeditions` flushes that to
  the faction map (as `Discovered`) only when in comm range (see §2). This is the one place the
  "expedition = another `StartingUnit` band" equivalence is deliberately broken.

**Map documentation is a *shared* expedition behaviour (every mission, not scout-only).** The
observe-into-`pending_reveal` + comm-range flush-to-`Discovered` step in `advance_expeditions` is
mission-agnostic — a hunting party ranging after a herd obviously maps the terrain it crosses, just
like a scout, and its findings report home (flush) whenever it is back within comm range: naturally at
each **Delivering** drop-off and on **Returning** fold-back, so the map lights up when it delivers the
food. Sites on the flushed tiles ride `discover_sites` for free. Only the **scout-specific** bits stay
scout-only — provisions upkeep, opportunistic replenish, and the awaiting-orders flow (a hunt party
lives off its kills and never idles).

Expeditions are excluded **by construction** (absence of `ResidentBand`), so the safe default survives
new systems added to the settlement arc. A breakaway-to-new-band is the same detached party that simply
drops its `Expedition` marker and gains `ResidentBand` instead of returning. Expedition-specific
per-turn logic (accumulate the pending-reveal buffer, comm-range check + flush-to-`Discovered`,
return-retarget to the band's live tile, arrival→awaiting-orders feed, fold-back on arrival, provision
depletion) lives in a new `advance_expeditions` system in the Population stage, right after
`advance_band_movement`. The `Expedition` component (incl. its pending-reveal buffer) is
**snapshot-persisted** so a rollback preserves an in-flight expedition and its unreported findings.

**Hunt verb — implementation model (PR 2).** The second verb rides the same detached-party machinery.
- **Mission + command.** `ExpeditionMission::Hunt { fauna_id }` (alongside `Scout`); a new
  `send_hunt_expedition <faction> <band> <party_workers> <fauna_id>` command (targets a *herd*, not a
  tile). `fauna_id` is persisted via a new snapshot field `expeditionTargetHerd` (also shown in the
  client panel).
- **Phases.** Two hunt-specific `ExpeditionPhase` variants: **`Hunting`** — `advance_expeditions`
  retargets `BandTravel` to `herd.position()` (from `HerdRegistry`) each turn and, when within
  `hunt.reach_tiles`, takes a **productive hunt's worth** of food (see take model) into the party's
  `LocalStore` up to **`party_workers × hunt.per_worker_carry`**; and **`Delivering`** — heads for the
  band's live tile and deposits once within communication range of home (`near_home`, the shared
  comm-range proximity), not necessarily on the exact live tile. A lost/extinct herd (`HerdRegistry::find` → none) flips the party to
  `Returning` (fold back). Recall → the shared `Returning` phase.
- **Take model — productive hunt, not a sustainable skim.** Per turn in reach the party takes ~a real
  hunt's worth (`workers × per_worker_biomass_capacity`, the same worker productivity as a band Hunt),
  drawn from herd biomass and converted to provisions — **not** the near-zero net-regrowth skim (a herd
  at carrying capacity has ~0 surplus, which made the party economically inert). The **policy** (the
  band's existing `FollowPolicy`, chosen at launch) governs the floor + trip behaviour:
  - **Sustain** — take down only to the herd's **sustainable floor**, then return with the load and
    **done** (leaves the herd to recover). Load may be < a full cap for a small herd. Delivers food.
  - **Surplus** — fill the **full carry cap** (drawing past the sustain floor but stopping above the
    extinction floor), return once, **done**. Delivers food.
  - **Market** — full-cap loads, **repeated trips** (auto-relaunch), grinding the herd down until it
    collapses or you recall. Delivers food, ongoing.
  - **Eradicate** — hunt to **extinction as denial**: **no food delivered** (the point is eliminating
    the herd, historically the bison-slaughter pattern); the party only self-feeds en route, ends when
    the herd is gone. Meaningful once there are rival peoples; scorched-earth option today.
- **Deliver trigger (fixes the empty flip-flop bug).** Flip to `Delivering` only with a worthwhile
  load: on the policy's completion condition (Sustain: herd hit its floor; Surplus/Market: cap reached)
  **or** `herd_near_band && carried ≥ hunt.min_deliver_fraction × cap`. The prior bug flipped to
  `Delivering` every turn whenever the herd sat within `drop_off_within_tiles` of the band regardless of
  carried amount, so the party oscillated in place and never gathered.
- **The shared `hunt_take` primitive.** The band's per-turn Hunt take math (extracted from
  `advance_labor_allocation`) is called from **both** the band Hunt path (behaviour unchanged) and
  `advance_expeditions`. The **scout's opportunistic replenish** uses it too: when a scout party's
  provisions fall below `party_workers × provision_upkeep_per_worker × replenish.low_turns` **and** a
  huntable herd is within `replenish.reach_tiles`, it tops up. One code path — one system, two verbs.
- **Lives off its own kills** (no separate provisions); v1 stays deterministic (no starvation/risk).
- **Catching migratory herds depends on the fauna-movement redesign** (next slice — see TASKS.md):
  herds today step 1 tile every turn, so an equal-speed party can't close on a long one-directional
  route. Once wild game grazes a tile (~1 turn dwell) before stepping and migratory herds loiter for
  many turns before a directed migration, the party catches them naturally during the dwell/loiter.
  Domestication accrual (a band-Hunt/husbandry interaction) is *not* wired to the expedition take in v1 —
  a documented later interaction. New tunables live in `hunt`/`replenish` blocks of `expedition_config.json`.

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
