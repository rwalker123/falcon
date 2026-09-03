---
paths:
  - "core_sim/src/{sites,sites_config,expedition_config}.rs"
  - "core_sim/src/systems/expeditions.rs"
  - "core_sim/src/data/{sites_config,expedition_config}.json"
  - "core_sim/tests/expedition_hunt.rs"
---

<!-- Extracted verbatim from lines 52-53;4004-4381 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Expeditions — wondrous sites, scouting, and the hunt

## Config files

| File | Purpose |
|------|---------|
| `src/data/sites_config.json` | Wondrous Sites catalog (`catalog`: per-`site_id` `category`/`display_name`/`glyph`/`placement_rule`/`discovery_reward.morale_bonus`) + `placement` rules (per-rule `max_sites`, `min_spacing`, and the union of rule inputs: `min_relief`, `max_habitability_pressure`, `min_food_weight`). Loader `sites_config.rs`, env override `SITES_CONFIG_PATH`. Not wired into the `reload_config` hot-reload path (mirrors `fauna_config.json`) |
| `src/data/expedition_config.json` | Expedition tuning. Scout: `comm_range_tiles` (discovery-report range), `comm_range_tech_factor` (stubbed 1.0 tech hook), `observe_sight_range` (per-turn LOS radius, matches band base sight), `provision_draw_per_worker_per_tile` (launch larder draw = party × distance × this), `provision_upkeep_per_worker` (per-turn drain = party × this, scouts only). Hunt `hunt` block: `per_worker_carry` (carry cap = party × this), `reach_tiles` (how close to the herd to take), `drop_off_within_tiles` (herd-near-band delivery gate), `min_deliver_fraction` (herd-near-band early delivery needs carried ≥ this × cap), `viability_warn_turns` (**20** — a client display threshold on `turnsToFill`; = 4× the throughput-implied trip length `per_worker_carry / (per_worker_biomass_capacity × provisions_per_biomass)` = 5 turns), `forecast_horizon_turns` (**60** — how far `hunt_trip_forecast` simulates a raid before giving up on completion; a raid is short — grab the surplus, come home — so simulating each to completion is cheap; **echoed onto every cohort as `expeditionForecastHorizonTurns`**, and it bounds the HUNTING only, never the trip). Scout replenish `replenish` block: `low_turns` (top up below party × upkeep × this), `reach_tiles`. Band-fission `settle` block: `min_founding_workers` (**4**) and `parent_min_workers` (**6**) — the two worker floors a `split_band` must clear, one on each half; the arc lives in `.claude/rules/core_sim/fission.md` and is nothing to do with an expedition. **Retired: `estimate_party_sizes` and the whole `deny` block (`requirement_rows`)** — both were sampling axes for the pre-computed estimate tables, and the forecast query answers exactly instead; see "The forecast is ASKED FOR". The retired `sustain_floor_fraction` is **gone** too: a hunting expedition is a **greedy raid** that grabs the standing surplus above the mission's **floor**, and the floor is chosen at launch (any fraction of `K` in `0.0..=1.0`; default `DEFAULT_ESCAPEMENT_FLOOR`, the food peak) rather than configured. Loader `expedition_config.rs`, env override `EXPEDITION_CONFIG_PATH`. Not on the `reload_config` hot-reload path (mirrors `sites_config.json`). **Validated** — `ExpeditionConfig::validate()` runs inside `from_json_str`, so *every* load path is covered; a broken invariant is logged at **error** level (`expedition_config.invalid_rejected`) and the config refused, falling back to the builtin rather than silently disabling a feature. Enforced: `comm_range_tech_factor` finite & `> 0`, `observe_sight_range ≥ 1`, `provision_draw_per_worker_per_tile`/`provision_upkeep_per_worker` finite & `≥ 0`, `hunt.per_worker_carry` finite & `> 0`, `hunt.reach_tiles ≥ 1`, `0 < hunt.min_deliver_fraction ≤ 1`, `hunt.viability_warn_turns ≥ 1`, **`hunt.forecast_horizon_turns ≥ max(1, hunt.viability_warn_turns)`** (at `0` the forecast's `1..=horizon` loop runs zero turns and *every* hunting expedition silently reports "won't fill"; below the warn threshold, a trip the player would be told is viable can never be discovered), `replenish.low_turns ≥ 1`, `replenish.reach_tiles ≥ 1`, `settle.min_founding_workers ≥ 1` (at `0` the gate cannot refuse anything — a silently disabled feature rather than a tuning; the "off for a playtest" value is `1`, which is a real band). **`settle.parent_min_workers` is deliberately NOT bounded below** — `0` there is a real policy ("the parent may give everything"), unlike its sibling, for which a band of nobody is not a band. Deliberately **left free**: `comm_range_tiles` (`0` = "walk back into camp to report"), `hunt.drop_off_within_tiles` (`0` = no early drop-off; a full pack still delivers), and the upper end of `forecast_horizon_turns` (it costs query time, on demand — an operator's call, not an invariant) |
## Wondrous Sites

Data-driven catalog of notable map features tiles can hold, hidden under fog until a faction's
vision reveals them, then recorded in a per-faction registry. v1 = sim + snapshot producer (the
client markers/readout are a separate slice). Authoritative design:
`docs/plan_exploration_and_sites.md` §3. Catalog `src/data/sites_config.json`, loader
`sites_config.rs` (mirrors `fauna_config.rs`: baked-in builtin + `SITES_CONFIG_PATH` override).

**Catalog** (`SitesConfig`): `catalog` keyed by `site_id` — each `SiteDef` carries `category`
(`landmark`/`settle_site`, free-form so new categories need no schema change), `display_name`,
`glyph`, `placement_rule`, and a `discovery_reward` (v1: a single `morale_bonus` lever, a struct
so future per-category rewards slot in). `placement` holds the per-rule tuning (`max_sites`,
`min_spacing`, and the union of rule inputs). Shipped: `great_peak` (landmark, rule
`prominent_mountain`) + `verdant_basin` (settle_site, rule `fertile_settle`).

**Placement** (`sites::place_wondrous_sites`, Startup after `spawn_initial_world` +
`apply_tag_budget_solver`): for each catalog entry, run its `placement_rule` against the tiles and
stamp a `SiteTag { site_id }` on the chosen tile entities, capped at `max_sites`, spaced by
`min_spacing` (Chebyshev), one site per tile. Deterministic under the map seed (`WorldGenSeed ^
SITE_PLACEMENT_SEED_SALT`; idempotent — a world that already carries `SiteTag`s is skipped).
- `prominent_mountain`: tiles whose `Tile.mountain` relief `>= min_relief`, tallest-first (ties by
  position), greedily placed.
- `fertile_settle`: tiles whose habitability pressure (`tile_morale_pressure` total — the same
  helper the snapshot's `habitability` uses) `<= max_habitability_pressure` **and** that carry a
  `FoodModuleTag` with `seasonal_weight >= min_food_weight`, shuffled (seeded) then greedily placed.
- On an 80×52 earthlike map both rules hit their `max_sites` cap (5 `great_peak` + 5 `verdant_basin`).

**Discovery** (`sites::discover_sites`, `TurnStage::Visibility` **after** `calculate_visibility`):
sites are rare, so it iterates the (few) `Query<(&Tile, &SiteTag)>` × the `VisibilityLedger`'s
factions. If a site's tile is `Discovered`/`Active` (ever seen, `is_discovered`) for faction F and
`(F, pos)` not already in `DiscoveredSites` → record it, apply the reward, push a feed entry.
Newly-found sites are processed in a stable `(faction, y, x, site_id)` order so the feed/reward are
deterministic.
- **Reward (v1):** `discovery_reward.morale_bonus` added once to each of F's `PopulationCohort`
  bands (clamped 0..1). Config-driven — the extension hook for settlement/resource/diplomacy rewards.
- **Command feed:** `CommandEventKind::SiteDiscovered` (`site_discovered`) with label = site display
  name, detail = `category=<c> at (x,y)`.

**Registry + persistence.** `DiscoveredSites` resource: per-faction `Vec<DiscoveredSiteRecord {
pos, site_id }>` + a `seen` set backing an O(1) `contains(faction, pos)`. **Snapshot-persisted** —
`restore_sim_state` rebuilds it from the checkpoint so a rollback
neither un-discovers a site nor retains discoveries made after the restore point. (The `SiteTag`s
themselves are worldgen tile tags and, like `FoodModuleTag`, are **not** rebuilt on rollback — the
registry is the durable record.)

**Snapshot (per-faction, no tile leak).** Undiscovered sites are **never** in `TileState`, so the
fog can't leak them. Instead the capture exports a per-faction `discoveredSites`
(`snapshot_discovered_sites`, resolving each record's `category`/`display_name`/`glyph` from the
catalog), mirroring `SedentarizationState`. Wire shape:
`discoveredSites:[DiscoveredSitesState{ faction:uint, sites:[DiscoveredSite{ x, y, site_id,
category, display_name, glyph }] }]` on both `WorldSnapshot` and `WorldDelta` (`snapshot.fbs`,
`sim_schema`). See "Visibility Systems" for the discovery hook in the turn flow.

---

## Scouting & Hunting Expeditions

A **detached traveling party** a faction outfits and drives out — to **explore** (scout) or to
**follow a migratory herd and deliver food** (hunt). One traveling-party system, two verbs. v1 =
sim + snapshot producer (client marker/outfit/recall UI is a separate slice). Authoritative design:
`docs/plan_exploration_and_sites.md` §2 (scout) + §2b (hunt) + the Implementation-model subsection.
Config `src/data/expedition_config.json`, loader `expedition_config.rs` (`EXPEDITION_CONFIG_PATH`
override, not on the hot-reload path).

**An expedition is another `StartingUnit` band.** It reuses `PopulationCohort` + `BandTravel` /
`advance_band_movement` + `LaborAllocation` + `StartingUnit`, tagged with the `Expedition` component
(`components.rs`: `home_band`, `mission: ExpeditionMission::Scout`, `phase: Outbound|AwaitingOrders|
Returning`, `announced`, `pending_reveal: Vec<UVec2>`) and **deliberately lacking `ResidentBand`**.
Carrying `StartingUnit` is required: it makes the party a moving snapshot marker and lets `move_band`
retarget it — but it is **excluded from live faction fog reveal** (`Without<Expedition>` in
`calculate_visibility`), because discovery is comm-range gated.

**Isolation via the positive `ResidentBand` marker.** Every real band gets `ResidentBand` at spawn
(`spawn_population_entity`) and on rollback restore; expeditions never do. Systems that must not see
expeditions filter `With<ResidentBand>`: `simulate_population`, `advance_population_migration`,
`sedentarization_tick`, `apply_starting_inventory_effects`, `balance_supply_networks`, and the
default-band command pickers (`select_starting_band` / `select_founder_band` `None`-bits branch).
Left **bare** (expeditions included): `advance_band_movement`, `advance_expeditions`,
`advance_labor_allocation`, the snapshot capture query, `collect_metrics`, `discover_sites`,
`advance_husbandry`. So expeditions are excluded **by construction** — the safe default survives new
settlement-arc systems. (A **new band never comes from an expedition** — it comes from a resident
band splitting in two; see `.claude/rules/core_sim/fission.md`.)

**`advance_expeditions`** (`systems.rs`, `TurnStage::Population`, registered right after
`advance_band_movement`, before the Visibility stage's `discover_sites`) runs per expedition each
turn. **Map documentation — (a)+(b) — is SHARED by every mission (scout AND hunt):** a ranging party
maps the terrain it crosses regardless of verb. **(a) observe** the tiles in `observe_sight_range` LOS
of its current tile into the private `pending_reveal` buffer (reusing
`visibility_systems::visible_tiles_in_range` — the pure geometry behind `reveal_tiles_in_range` —
**without** touching the faction map); **(b) comm check + flush** — when within `effective_comm_range()`
(= `comm_range_tiles × comm_range_tech_factor`, rounded) hex distance of the home band's **live** tile,
promote every buffered tile to `Discovered` on the faction map (`FactionVisibilityMap::discover`,
Unexplored→Discovered, never downgrading `Active`) and clear the buffer — so the map lights up **as a
lump on return** (for a hunt party, at each `Delivering` drop-off / `Returning` fold-back), and
`discover_sites` records any `SiteTag` on the flushed tiles for free. **Scout-only** below: **(c)
provisions** drain by `party × provision_upkeep_per_worker` (hunt lives off its kills; non-fatal at
zero in v1) + opportunistic replenish; **(d) phase transitions** — `Outbound` + arrived (no `BandTravel`) →
`AwaitingOrders` + one-shot `ExpeditionArrived` feed; `Returning` → chase the home band's live tile
(refresh `BandTravel`) and, once within comm range **or the moment that band cannot be resolved at
all**, fold workers + leftover provisions back into the band + despawn (`ExpeditionReturned`, after
the flush so the final findings report — see "One fold-back, two moments"); `AwaitingOrders` waits.

**Hunt verb (PR 2)** — `ExpeditionMission::Hunt { fauna_id, target_species, floor: f32 }` on the
same party; **the floor is chosen at launch** (`send_hunt_expedition <faction> <band>
<party_workers> <fauna_id> [floor]`) and is not a config lever. It is a
`0.0..=1.0` fraction of `K`, default `DEFAULT_ESCAPEMENT_FLOOR`. **The floor token is a NUMBER**: the
four stance words are refused at parse with `CommandParseError::RetiredStanceToken`, so a `… sustain`
copied from this file's older wording is a hard error rather than a default. `advance_expeditions`
branches on mission:
- **Hunting**: retarget `BandTravel` to the herd's live tile each turn (from `HerdRegistry`). The
  take **and the trip-completion decision both live inside the `hunt.reach_tiles` guard** — a party
  still walking to its herd never concludes the trip. Once in reach, take a **productive** hunt's
  worth of biomass — `workers × per_worker_biomass_capacity`, capped per policy (below) — from the
  herd and convert through the species' `HuntYield::apply` up to the carry cap (`party ×
  hunt.per_worker_carry`). Deliver only with a worthwhile load: a full pack **or** `herd_near_band &&
  carried ≥ hunt.min_deliver_fraction × cap` (the empty-larder flip-flop fix). The near-band case is a
  **drop-off, not a trip end** — the party delivers and resumes hunting (issue #441). An empty pack at
  completion reports **why** (no sustainable take / no take possible), never a cheerful zero.
> #### A RAID'S LENGTH IS A SPECIES-AND-KIT CONSTANT, and there is no player lever on it
>
> The pack is measured in **carry** and the take, since the engagement stage, in **reach** — so
> `turns_to_fill = per_worker_carry / (engage_rate × stay_chance × body_mass × provisions_per_biomass)`
> and **party size cancels out entirely**. Eight hunters after Wild Fowl reported *"away ≈43 turns —
> 31 hunting, 12 travel"*; four hunters take 31 turns and sixteen take 31 turns. §4.6's ceiling table
> is silently also the trip-length table.
>
> **A player-set `fill_target` ("take ≈50 and come home") shipped as the one lever on that constant
> and was RETIRED** (`docs/plan_hunt_through_combat.md` §5.2, marked retired in place). It replaced
> the pack's capacity in the completion the raid already evaluates, so `NO_FILL_TARGET` ("fill the
> pack") was always the default and its removal is bit-identical to that default. It went because the
> unplayably long trips it existed to escape (Wild Fowl 88 turns, Grey Wolf 76, Rabbit 59 against
> Mammoth 1.1) are a **tuning** problem — **issue #491** — and a control that asks the player to work
> around a config error is not a decision. Removed with it: `RaidOrders::fill_target`,
> `NO_FILL_TARGET`, `HuntTripBound::FillTarget`, `systems::expeditions::raid_load`/`RaidLoad`, the
> grammar's second positional tail, and the `fill_target` parameters on `hunt_trip_forecast` /
> `expedition_delivery`. **The wire slots are deprecated in place, never deleted** —
> `snapshot.fbs`'s `expeditionFillTarget` (a FlatBuffers vtable slot is positional) and
> `command.proto`'s `fill_target = 7` (a shipped field number is immutable).
>
> The invariance itself is pinned by
> `expedition_hunt::a_raids_length_is_invariant_in_party_size_while_its_payload_is_not` — deliberately
> paired with the payload half, so it cannot read as "party size does nothing", and carrying the
> `PackFull` bound-naming assertion the retired tests also held. It is the statement the tuning pass
> on #491 has to move.

> #### A hunting expedition is a GREEDY RAID, not a resident band's throttled skim (playtest fix)
>
> A resident band used to take its policy's per-turn **rate** into a kill-credit bank —
> worker-independent, so a second hunter only added pack to fill and made the *trip* longer (the
> playtest bug). A detached party instead **grabs the herd's standing surplus above the policy's floor
> in a burst and comes home**, so more hunters take more animals in **fewer-or-equal** turns.
>
> **Since `docs/plan_harvest_floor.md` slice 1 the resident band uses the raid's SHAPE** — both are
> constant escapement to the **floor its orders name** — so what still separates them is pace, not
> model: a raid works one herd with its whole party until the surplus is gone, a band works it a turn
> at a time. **`hunt_expedition_floor` is DELETED** — it existed only to map a stance onto a number,
> and the mission now carries the number itself. There is one floor for both paths because both read
> the assignment's own `f32`, not because two tables were kept in step.
>
> - **The floor is the MISSION'S**, a fraction of `K` the launch command names — there is no table
>   left to look it up in (`hunt_expedition_floor` is deleted; it existed only to map a stance onto a
>   number). A deeper floor leaves a leaner herd, and **extinction is the floor-`0` case**: any floor
>   above `0` strips the herd to it and stops, on this path and the resident band's alike.
> - **A raid is ENGAGEMENT-BOUNDED exactly as a resident band is** (`docs/plan_hunt_through_combat.md`
>   §1; §10 exempts only the pen). `expedition_take_biomass` resolves the party's reach
>   (`fauna::animals_engaged`, which carries no build term at all since the dip retired) and the
>   quarry's retreat (`fauna::animals_that_stay`, under the caller's `fauna::HuntDraw` — a per-event
>   seed live, a quantile in a forecast) and hands the count to **the**
>   quantiser, which also retired its hand-rolled copy of the pack-seating arithmetic. The bound
>   is what stops the *same* party on the *same* herd taking a different number of animals purely by
>   choosing the expedition verb (five hunters took 5 Red Deer a turn from camp and 13 as a raid);
>   pinned by `expedition_hunt::a_raid_and_a_resident_band_reach_the_same_animals`.
>   **The raid's economics turn on it**: where the herd's regrowth outpaces what a legal party can
>   reach, the surplus is never spent and the raid does not complete inside
>   `hunt.forecast_horizon_turns` — an honest "this party cannot clear this herd", not a stall.
>   **The forecast DRAWS NOTHING — it resolves at the expectation** (`RAID_FORECAST_DRAW` =
>   `fauna::HuntDraw::EXPECTED`). A projection has no tick to name, so it cannot compose the live
>   take's per-event seed at all. This replaced a `forecast_retreat_seed` that built a real seed out
>   of zeros for the two world terms a projection lacks: stable and reproducible, and still wrong in
>   *kind* — it drew a **sample** and presented it as the answer, so the moment a stochastic stage was
>   authored the preview would report one draw while the take paid another, indistinguishably. At the
>   `wariness 0` / `hit_chance 1.0` it landed on, both were bit-identical, so no raid number moved —
>   and once slice 7 authored the roster's wariness the raid preview became a genuine **expectation**
>   rather than a sample, which is exactly the promise the retired seed could not have kept. See
>   `yield-forecast.md` → "THE INVARIANT IS RESTATED".
> - **The take brings home a PARTIAL when it must, and wastes the rest — reconciled with the band.**
>   The party's processing throughput (`workers × per_worker_biomass_capacity`) is banked onto the herd's
>   `hunt_credit` — **the field's one remaining writer**, since the resident band stopped banking — and
>   the bank meters *when* the next whole animal is **ready** (a body heavier than one turn's work takes
>   `body / throughput` turns). Once one is banked (`affordable >= 1`) the party
>   **kills it even if the pack cannot seat it whole**, carries the pack's worth, and **wastes the
>   remainder** — exactly the resident band's pack-seating rule
>   (`fauna::animals_the_pack_seats`): a
>   1-hunter party on an 800-biomass mammoth (16 food) whose pack holds only `per_worker_carry` = 4 food
>   (200 biomass) kills it, delivers ~200 (≈ 25%), wastes ~600. So **`animals_taken` is now a KILL
>   count**, and the delivered payload is `delivered_food` (`Σ HuntYield::apply(carried)`), not
>   `animals_taken × foodPerAnimal`. **"Too lean to raid" means `delivered_food == 0` (no surplus at any
>   party size)**, NOT "party too small to carry a whole animal". This reconciles the expedition with the
>   band's waste model; the earlier no-waste rule (`killed == carried`, `wasted == 0`) is retired.
>   - **This does NOT reintroduce the over-kill bug** the no-waste rule guarded against. That bug was
>     killing *many* animals per trip and carrying a sliver of each; the guard now is the **pack-full
>     completion**. When the pack cannot seat a whole animal, the one forced-partial kill carries
>     `min(body, room) = room` — a **full pack** — so `larder >= cap` fires and the trip ends after that
>     ONE kill. The completion's "can't seat another whole animal" branch is now gated on
>     `larder > 0` (already delivered), so it no longer sends a small-pack party home empty on turn 1
>     before it can bank credit for its forced partial. One hunter per herd, so sharing `hunt_credit`
>     with the band is safe.

- **Per-FLOOR behaviour** — one rule, read off `raid_is_recurring(floor)` rather than four named
  cases. Every raid grabs the standing surplus down to its floor.
  **At or above the food peak** (`floor >= MSY_BIOMASS_FRACTION`) — **one raid**, ending on a **full
  pack or the surplus spent**, then fold home; a herd that wanders within
  `hunt.drop_off_within_tiles` of the band earns an opportunistic **drop-off** (`Delivering`→deposit)
  and the party **resumes hunting** with an empty pack.
  **Below the food peak** — repeated FULL-cap trips (`Delivering`→deposit→**auto-relaunch**) *while
  the herd still has surplus*, because a floor under `K/2` leaves more standing than one pack holds;
  once the herd sits at that floor it comes home for good rather than trickle-churning.
  **At floor `0`** (`floor <= STRIP_IT_BARE`) — nothing to stop at: grinds the herd to extinction
  (→ lost-herd `Returning`), **banking the windfall it can carry** on the way (#337 — denial is the
  end state, not an empty pack). **Its pack does not end the trip but still bounds the haul**, so
  `hunt_trip_forecast` gates the party-side completion on the same `floor > STRIP_IT_BARE` its
  `surplus_spent` always carried, and a floor-`0` row projects through to `herd_lost` (or `horizon`)
  reporting the raid's whole waste — rather than quoting a `pack_full` homecoming the live arm never
  makes. **`herd_lost` names the turn the party comes home**, so a floor-`0` row reads as a raid that
  finishes rather than one that never does.
- **The completion fix** (`ExpeditionPhase::Hunting`, load-bearing): `done = pack full OR standing
  surplus spent (herd within one body of the floor) OR herd lost`. Without the surplus-spent branch a
  raid that grabs its surplus and hits the floor would **hang, taking 0 every turn**. That list is
  literally the whole rule for every delivering policy: the near-band drop-off is **orthogonal to
  completion** — it decides *deliver now*, never *the trip is over*. `done` is tested before
  `relaunch`, so a party at the policy floor comes home for good instead of cycling, and each drop-off
  drains more standing surplus, so the drop-off loop converges on `surplus_spent`.
- **The drop-off radius and the local-hunt leash are DIFFERENT CIRCLES, deliberately.** The drop-off
  radius is `hunt.drop_off_within_tiles` (**3**) measured **herd → home band**; the *local* band-Hunt
  leash is `band_work_range + hunt_leash_tiles` (2 + 3 = **5**, `LaborConfig::hunt_reach()` in
  `labor_config.rs`, whose out-of-leash case lapses a resident band's Hunt assignment). So a herd 4–5
  tiles out is locally huntable with no expedition consequence at all, and a herd ≤ 3 tiles out prompts
  an expedition **drop-off** — not a cancellation of either.
- **Launch forecast — a bounded forward SIMULATION of the raid** (`hunt_trip_forecast`,
  `systems::expeditions`). It runs the raid forward turn by turn — `fauna::regrow_biomass` (Logistics)
  then `expedition_take_biomass` (Population), accumulating the larder on the **fixed-point `Scalar`
  grid** — until the raid completes (fill OR surplus spent OR herd lost) or `hunt.forecast_horizon_turns`
  (**60**). No second copy of the model, and the completion test mirrors the arm's `done`. It **cannot
  model the near-band drop-off**, and that is structural, not an omission: the forecast query
  it feeds is **band-agnostic** (one row per herd serves every band), so there is no band distance to
  measure `hunt.drop_off_within_tiles` against. The resulting approximation is one-directional — a
  drop-off lets a raid deliver **more** than projected (several loads over a longer trip, since the
  party resumes hunting with an empty pack), never less — so the projection is a **lower bound** on a
  near-band raid. It returns:
  - **`turns_to_fill`** — turns until the raid **completes** (*"turns until the party comes home"*, NOT
    *"turns until the pack is full"*: a big party on a full herd leaves a partial pack once it strips
    the surplus, a successful short trip). `None` = never completed within the horizon.
  - **`animals_taken`** — whole animals the raid **kills** (carried whole or partially wasted). `0` = the
    herd is at/below the policy's floor with **no surplus to raid** (the honest non-viable case).
  - **`delivered_food`** — food actually landed in the larder (`Σ HuntYield::apply(carried)`), the primary
    readout. **`wasted_food`** — food killed but not hauled (`Σ HuntYield::apply(wasted)`); the waste
    fraction is `wasted_food / (delivered_food + wasted_food)`. A small party on a big animal now
    delivers a partial with waste, so **"too lean to raid" is `delivered_food == 0`**, not "party too
    small to seat a whole animal".
  - **`delivered_material`** — **what the trip actually lands, per material**, and on an
    **inedible** quarry the *entire* payload: a wolf's `delivered_food` is `0`, so without it the
    launch sheet promised a trip that appeared to bring home nothing while the sim banked real hides
    on fold-back. `[MaterialPayoff { materialId, amount }]`, the shape the crop picker and the herd
    rates already use — **no second table was minted**.

    **Projected off the SAME carried biomass `delivered_food` is**, accumulated turn by turn in one
    `carried_biomass` local beside it and converted once through
    `materials_config::material_yield_totals` — the same expression the live arm's
    `credit_material_yield` is paid on. Converting per turn would have been arithmetically identical
    (the conversion is linear) and given two places to get it wrong; one accumulator is why the two
    readouts of one trip cannot disagree.

    **It replaced `delivered_trade`, which was retired and deliberately not replaced** on the
    reasoning recorded at the site: *a material is a batch with a characteristic vector, not a
    per-turn number this table can sum.* That is right about **merging readings** and wrong about
    **stating a quantity per material id** — which is exactly what `MaterialPayoff` does. The
    readings are not here and do not need to be: they ride the batches the take really creates.

    Same three contracts as every material readout in this arc: **never summed**, **empty is "no
    row" not zero**, **key always present**.

  - **The forecast still simulates an inedible quarry like any other** (#337): it used to
    short-circuit to an all-zero projection on the premise "a wolf trip is not a food trip", which
    also zeroed `animals_taken`. Only an **empty party** (`cap <= 0`) short-circuits now; a wolf raid
    gets a real ETA (it ends when the standing surplus is spent) and its food fields fall out at `0`
    on their own. `animals_taken` remains what `forecast_query::useful_party_cap` scans for its
    plateau — a plateau is a fact about the herd's surplus rather than about a currency.
  > **THE LIVE SURFACE IS THE QUERY REPLY, NOT THE `.fbs` TABLE.** `HuntTripEstimate` /
  > `HerdTelemetryState.huntTripEstimates` are `(deprecated)` and nothing writes them — the client
  > *asks* for a trip forecast (`sim_runtime`'s `QueryCommand` → `forecast_query::hunt_trip_row` →
  > `HuntTripRow` over `command.proto`). So `delivered_material` was appended to the **proto row**
  > (field 11, beside a new `MaterialPayoff` message); adding it to the deprecated table would have
  > shipped a field nobody reads.
  >
  > **The guard is `hunt_yield_vector::an_inedible_raids_promised_material_is_what_the_trip_banks`**,
  > and a wolf is the subject because its entire payload is material — nothing else on the estimate
  > can cover for the vector being wrong. It drives a **real** raid through the real systems until it
  > folds back and asserts the promise against the home band's `LocalStore::material_total`, never
  > against a re-derivation of the projection. Its second half is the one a mis-read row would fail:
  > nothing may come home that was not promised.

  - **`bound`** (`HuntTripBound`) — **WHICH stop ended the trip**: `PackFull` / `Floor` / `HerdLost`
    / `Horizon`, wire keys `"pack_full"` / `"floor"` / `"herd_lost"` / `"horizon"`. **Four, not
    five** — the retired `FillTarget` went with the lever it named (see the callout above). A trip
    *length* alone cannot say which bound it was — *"you fill the pack in 31 turns; the herd never
    reaches the floor"* and *"you reach the floor in 2 turns with the pack a third full"* are
    different decisions carrying the same kind of number — so the sim names it and the client
    composes nothing. **`Horizon` is exactly the
    `turns_to_fill == None` case, with no exception** — it is the only bound with no completion turn,
    because it is the only one where the raid had not ended. **`HerdLost` reports the turn the herd
    went**, like every other stop: the live arm's lost-herd guard turns the party for home in the same
    turn's Population stage that Logistics despawned the herd in, so the raid *did* end — by emptying
    the range rather than by filling a pack. It used to report `None`, which is the wire's
    "never completes" sentinel, so a **floor-`0`** row (the raid whose *only* stop is that guard, since
    it has no party-side stop at all) published a real `bound` beside a `turnsToFill` of `0` and left
    the client nothing true to say about the one raid that reliably finishes. **A tie goes to the
    herd side** (`Floor` over the party-side stop), mirroring the live arm testing `done` before
    `relaunch`. Pinned against the raid the systems actually run by
    `expedition_hunt::{the_exported_bound_names_the_stop_that_ends_the_raid,
    a_floor_zero_raid_reports_the_turn_the_herd_runs_out}` — the latter paired with a `Horizon` case,
    so "always name a turn" cannot pass by naming one for everything.
  - **Travel is not counted**; the herd is assumed stationary and in reach. `delivers_food == false`
    means an **INEDIBLE species** — never a denial *policy*: Eradicate banks its windfall like every
    other rung. Its sibling `deliversTrade` (appended last) is the other component.
  - *(The old O(1) "cannot fill" short-circuit + its `hunt_trip_bound_tests` sweep were **retired** with
    the raid: their premise "won't fill the pack ⇒ doomed trip" is inverted by a raid, where "won't fill
    the pack" is the normal successful short trip. A raid is inherently short — grab the surplus, done —
    so simulating each to completion is already cheap. `surplus_escapement_fraction` replaced the retired
    `hunt_expedition_ceiling`/kill-credit expedition ceiling and the bound's constants — and has since
    been retired itself: the mission's own floor is the whole ceiling now.)*
- **Animals delivered SCALE WITH THE PACK** (`2 × workers` on a heavy-bodied herd with ample surplus,
  since the pack seats `pack ÷ food-per-animal` whole animals) until the surplus caps them — the
  plateau **is** the max-useful party size (`ceil(surplus_food / per_worker_carry)`), which the client
  reads straight off `animalsTaken`. **Measured** (real pack 4 food/worker): a **Marsh Grazer** (body
  100, food/animal 2, big surplus) delivers **2 / 4 / 6 / 8** animals for 1/2/3/4 hunters, ~5 hunting
  turns each, **0 wasted**; a **Wild Boar** (K 1433, B 1010) delivers **4 / 8 / 7** for 1/2/3 hunters
  (5 / 5 / 3 turns) — it goes surplus-bound sooner (only ~5–8 boar of surplus), and a *faster* big party
  harvests slightly less regrowth on the way down.
- **Travel is counted at launch, band-relative.** `hunt_trip_forecast` returns only the HUNTING turns
  (once in reach); `handle_send_hunt_expedition` adds the **round-trip walk** (`ceil(2 ×
  hex_distance(band, herd) / band_move_tiles_per_turn)`) to the feed line, where the launching band's
  tile is known. The forecast query counts only the turns spent working the herd (one answer serves
  every band), so its `turnsToFill` is the hunting turns and the **client** adds the same travel to the
  pre-launch readout from the selected band's tile + `bandMoveTilesPerTurn`.
- `handle_send_hunt_expedition` folds the verdict into the `ExpeditionSent` feed line: **denial**
  (Eradicate) → "delivers NO food"; **no surplus** (`animals_taken == 0`) → "too lean to raid… no
  surplus"; otherwise "est. ~N animals over ~M turns (H hunting + T travel)". It still launches — the
  player's call. `detail` carries `eta_turns=… hunt_turns=… travel_turns=… animals=…`.
- Pinned end-to-end by `expedition_hunt.rs` (`the_raid_forecast_matches_a_real_party_run`), which
  launches a **real party**, runs the sim forward, and asserts the forecast completes on exactly the
  turn the party leaves `Hunting` — across Sustain/Surplus/Deplete × full/depleted herds. The forecast
  is pinned to the sim, never the reverse. The greedy-raid properties (more hunters → fewer turns,
  Sustain leaves K/2, deeper policies raid deeper, surplus caps the take, no-surplus reads 0) are pinned
  by the sibling tests in that file.
- **Lives off its kills** — no launch provisions, no per-turn upkeep (upkeep is scout-only).
- **The improvements are NOT an expedition concept, and since issue #442 that is a TYPE-LEVEL fact.**
  `Cultivate`/`Sow`/`Tame`/`Corral` are place-bound work a *resident* band does (prepare a patch, build
  a pen, then tend it) — a detached party cannot pen a herd and walk home. They are now an
  `Improvement`, and `ExpeditionMission::Hunt` carries a **`floor: f32`** — a *number*, which cannot
  name a verb at all, so the launch token is parsed as one and a word of any kind is refused by the
  ordinary parse rather than by a membership test. Their history is worth keeping: both this launch
  gate and `hunt_expedition_floor`'s unreachable investment arm were hand-written verb lists, and both
  had rotted (the gate silently accepted `tame`, which then took a plausible pastoral ceiling).
  Guarded by `server::tests::send_hunt_expedition_rejects_a_floor_outside_the_dial`.
- **Shared take helpers** (`fauna.rs`): **`hunt_escapement_ceiling(floor, biomass, carrying_capacity)`**
  is THE take ceiling on the animal web — `max(0, B − floor·K)`, the stock standing above the
  assignment's or mission's floor — and `quantise_animal_take` rounds it to whole animals. It takes
  **no ecology, no `FaunaConfig`, no `improvement` and no ladder**, which is what makes the take
  `r`-independent structurally rather than by convention; see "The hunt policy axis" in `fauna.md`.
  **A build is not a term here or anywhere else on the take**: it is raised by the band's own
  `builders` pool (`docs/plan_standing_upkeep.md` §2.5), so there is nothing for this signature to
  put. The
  `improvement`/`ladder` parameters this file used to list are exactly what slice 3 removed.
  The expedition keeps its own `credit` accumulator for the *party's* processing throughput
  (`expedition_take_biomass`), which is a different quantity from the retired resident bank.
  **`HuntYield::apply(take, output_multiplier)`** (via `FaunaConfig::hunt_yield_for`, which retired the global `hunt_provisions`) is the single per-species biomass→food conversion (an
  `f32`; the take path quantizes it onto the larder's `Scalar` grid). The rate is the *building*-phase
  ceiling: a
  **completed** corral is never hunt-drawn at all — the Hunt arm takes the tend branch (paid
  `corral_provisions`, no biomass drawn) — and `fauna::hunt_forecast` is the one place that phase split
  lives (`herd.is_corralled()` → `SourceYieldForecast::tended`). `hunt_take` (`systems.rs` — band Hunt
  labor + the **scout's
  opportunistic replenish**, a Sustain nibble when a scout's provisions fall below `party ×
  provision_upkeep_per_worker × replenish.low_turns` and a herd is within `replenish.reach_tiles`) and
  the hunt expedition both call them, so no formula has a second copy. The expedition applies **no**
  output multiplier (`EXPEDITION_OUTPUT_MULTIPLIER` — a detached party carries no band morale
  modifier). **The expedition take pays BOTH components of the species' `HuntYield`** (#337,
  `docs/plan_hunt_yield_model.md`) — the food-only expedition, and the "known v1 gap" that excused it,
  are **retired**. It was never a gap the client could live with: since #337 the raid *forecast*
  advertises `deliveredTrade`, so a food-only take promised pelts the sim never paid — and an
  **inedible** quarry (a wolf) made a hunting expedition return with literally *nothing*.
  - **Where each product lands — ONE store, both accounts.** Provisions go into the party's pack
    (`stores[FOOD]`, carry-capped) and fold into the home band's larder. **Materials** go into that
    same `LocalStore` as batches and move home with
    `LocalStore::drain_materials_into` at the next arrival — a `Delivering` drop-off or a `Returning`
    fold-back — **batch by batch**, so a mammoth hide is never averaged into a hare pelt on the walk
    home. A haul that arrives with no home band left to receive it is simply lost, exactly as the
    carried food is. Both scale off the biomass the party **carried**, never what it killed.
  - **`Expedition::carried_trade` is RETIRED** (arc #527) with the axis it banked. It existed because
    a scalar had nowhere else to accrue between kills; a material batch has the party's own store,
    which the checkpoint carries whole, so `PopulationCohortState.expedition_carried_trade` went with
    it and there is nothing left for a rollback to silently zero. The feed line's *"returning EMPTY"*
    test reads `systems::expeditions::materials_carried` — the party's own batch total — so a wolf
    raid coming home with a pack full of hides is still never called empty.
  - **The IN-FLIGHT half needs no sim work.** `PopulationCohortState.materialBatches` is resolved
    from `cohort.stores` with **no `ResidentBand` gate**, so a detached party's carried materials are
    already on the wire per batch, with their exact readings, for the whole trip. A scout hauling a
    wolf home is legible today and always was — what was missing was only the *promise* above.
  - **The scout's opportunistic replenish banks its hides too** — a roadside kill is skinned as well
    as butchered — so it is no longer a pure waste of animals on an inedible herd.
  - **Still expedition-side gaps:** no **husbandry/domestication accrual** (a Sustain *expedition*
    builds no domestication — that is place-bound work a resident band does), and the raid forecast
    states no material payload at all (arc #527's open item, above). Catching a *migratory* herd depends on the deferred
    fauna-movement redesign (herds step 1 tile/turn today, so an equal-speed party can't close a long
    one-directional route).

**Commands** (full proto/runtime/text/server plumbing, mirroring `move_band`):
- `send_expedition <faction> <band> <party_workers> <x> <y>` — validates land target + `1 ≤
  party_workers ≤ available_workers` (the band, and nothing else — the retired sampling ladder was a
  sampling ladder, not a ceiling), draws `party × distance ×
  provision_draw_per_worker_per_tile` provisions from the band larder (partial OK), removes the
  workers from `band.working`, and spawns the detached `Expedition` cohort. Feed `ExpeditionSent`.
- `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> [floor] [kit <id>]`
  — same resident-band gate + party validation, validates `fauna_id` resolves to a live herd, draws **no**
  provisions, removes the workers, spawns a `Hunt`-mission party in `Hunting` phase heading for the
  herd. Feed `ExpeditionSent` (hunt flavor), whose detail carries `floor=… bound=…`.
  The **floor is the ONE optional positional token** (proto field `floor = 6`) and it **fails
  closed** — out of `0.0..=1.0` → command failure, never clamped. **Anything after it is refused**
  (`CommandParseError::UnexpectedArgument`): the retired fill target sat in that slot, so a stale
  caller's second number must fail rather than be silently dropped.
- `send_denial_raid <faction> <band> <party_workers> <fauna_id> [kit <id>]` — the **third verb**
  (`SendDenialRaidCommand`, proto field **49**). Shares the whole outfit half with the hunt above —
  `server::outfit_raiding_party` is the one seam for the resident-band gate, the live-herd lookup and
  the party bound, so a third verb could not acquire its own copy of them — and differs only in the
  mission it names and the verdict it quotes. **Its grammar is CLOSED except for the kit**: there is no floor
  to pass, so any other trailing token is a hard parse error rather than a value
  to ignore. The one exception is the named `kit <id>` pair — a kit is a property of the **party**,
  not of the mission, so it is the only order a raid carrying no numbers still has to give (see
  `equipment.md` → "A kit is a MASK"). Feed `ExpeditionSent`, whose detail carries `mission=deny outcome=… turns_to_collapse=…
  low=… high=…` and **no `floor=`**. See "Denial is a MISSION, not a floor".
- `recall_expedition <faction> <expedition_band_id>` — resolves the entity via
  `resolve_expedition_entity` (checks the `Expedition` component + faction), sets `phase = Returning`
  (works for both verbs). Feed `ExpeditionRecalled`. **A party standing in its home band's own camp
  is CANCELLED on the spot** rather than sent on a round trip — see "One fold-back, two moments"
  below.
- **There is no settle verb on this path.** `settle_expedition` held proto field **51** and turned an
  arrived party into a resident band; it is **retired**, and the slot carries `split_band` now — a
  verb about a *resident* band, which is why it is documented in `.claude/rules/core_sim/fission.md`
  rather than here. A scouting party is composed for scouting, so founding a band from one means
  founding it from inputs nobody chose; that rule file carries the argument.
- **Retargeting a scout waypoint is just `move_band` on the expedition entity** — `handle_move_band`
  has a hook that re-arms a moved expedition to `Outbound` + `announced = false`.
- New `CommandEventKind` variants: `ExpeditionSent`, `ExpeditionArrived`, `ExpeditionRecalled`,
  `ExpeditionReturned`, `BandFounded` (in `as_str` + the server label map); the hunt drop-off /
  lost-herd feed lines reuse `Hunt`.

**Snapshot.** `PopulationCohortState` gains client discriminators `isExpedition` / `expeditionMission`
(`"scout"`|`"hunt"`|`"deny"`) / `expeditionPhase` (`outbound`|`awaiting`|`returning`|`hunting`|`delivering`) /
`expeditionTargetHerd` (hunt fauna_id — a **string**, since herd ids are non-numeric; the KEY, never
rendered — its display twin `expeditionTargetSpecies` rides beside it, see "A raiding party carries
its quarry's NAME, not just its key") /
**`expeditionFloor:float`** (the raid's escapement floor as a fraction of `K` — the live
discriminator, defaulting to `1` so an absent floor reads "take nothing" rather than "take
everything"; `expeditionHuntPolicy` is the retired `(deprecated)` slot it replaced and has no
accessor; `expeditionFillTarget` is the other retired `(deprecated)` slot, from the fill target —
the sim never writes it) / **`expeditionTripBound:string`** (which stop will end *this* party's raid —
the `HuntTripBound` key, off the same in-flight forward simulation `expeditionEtaTurns` comes from,
so it answers for the party's **real** orders rather than for the band-agnostic pre-launch table;
`""` = not raiding — a resident band, a scout, or a party already walking a load home, which is a
different statement from `"horizon"`) / `expeditionCarryCap` (hunt carry cap =
`party × per_worker_carry`, `0` otherwise) and persistence-only `homeBandEntity` /
`expeditionAnnounced` / `pendingRevealX` / `pendingRevealY`
(`snapshot.fbs`, `sim_schema`). Capture fills them from `Option<&Expedition>`;
`restore_sim_state` re-attaches `Expedition` for a rolled-back in-flight party (resolving
`home_band` from `homeBandEntity` via the cohort entity-remap; missing home band → log + skip) and
re-attaches `ResidentBand` to every non-expedition cohort so the `With<ResidentBand>` systems keep
running after a rollback.

`PopulationCohortState.maxExpeditionPartySize` is a **retired `(deprecated)` slot**. It echoed the
last rung of the sampling ladder — where the estimate rows stopped — and it **capped nothing**: the
stepper always clamped to `idleWorkers` alone. Every client site that read it said so in capitals,
which is the tell: a field whose name asserts a rule that four comments exist to deny is a field to
delete. See "A raiding party is bounded by the BAND".

**In-flight next-delivery forecast — the twin of the pre-launch estimate, for a party already on the
map** (`systems::expeditions::expedition_delivery`). The pre-launch query answers "if I
launch, what comes back?"; this answers "the party I already sent — when does its food land, and how
much?" It reuses the SAME raid forward-sim: `hunt_trip_forecast_seeded(.., initial_larder)` is the
existing `hunt_trip_forecast` body with a seedable starting larder (the public
`hunt_trip_forecast` is now a thin zero-seed wrapper, so its callers are byte-identical), seeded with
the party's current haul and run against the herd's REAL state (the forecast clones the herd, so it
already starts from the live `hunt_credit`) — **forecast == actual, no second copy of the model.** The
ETA decomposes by `ExpeditionPhase`: `Returning`/`Delivering` → the walk home with what it carries;
`Hunting`/`Outbound`/`Awaiting` → remaining travel-to-herd + the seeded raid's `turns_to_fill` + the
walk home. It is a deliberate **approximation** (the home band is nomadic, and travel is measured as
`hex_distance / band_move_tiles_per_turn` while `advance_band_movement` steps the axes independently)
— honest for a "~N turns" readout, not turn-perfect; the pin test (`expedition_hunt.rs`) puts the home
band on the herd's row so the return leg is exact. Scouts deliver map data, not food → `None`.
Computed at snapshot capture (which has the `HerdRegistry`, the configs, and a cohort→tile map for the
home band's live position) and exported as three **append-only** `PopulationCohortState` fields
(placed after the last field, `foodIncomeAverage`, to keep FlatBuffers ids stable):
**`expeditionEtaTurns:uint`** (turns until the carried food reaches the home larder; `0` =
unknown/n-a — a scout, a normal band, or a trickle-fill raid with no finite ETA),
**`expeditionProjectedDelivery:float`** (`carried + still-to-take`, pack-capped — `0` means the herd
is at/below the mission's floor with no surplus to raid), and **`expeditionRecurring:bool`**
(`systems::raid_is_recurring(floor)` — the single source, `floor < MSY_BIOMASS_FRACTION`, since a
floor below the food peak leaves more standing than one pack holds, so the trip becomes a *series* of
trips. A floor **at or above** the peak makes **one raid** and folds
home. Not the same question as "does it ever pass through `Delivering`": a near-band drop-off is an
incident inside one raid, so a peak-floor party that drops a load off and resumes hunting still reads
`false`).
Client-consumed only (not persisted). See the client's parties inspector strip + "Next delivery" line.

**Pre-launch export — the client does ZERO arithmetic.** The launch forecast above only rides the
*post-commit* `ExpeditionSent` feed line; the outfit UI needs the trip's economics **before** the
player commits workers, as they pick party size / herd / floor. The expedition's trip length is **not
a formula** (see the forecast above: a small-herd Surplus party exhausts *stock*, so no per-turn rate
describes the trip), so the sim simulates it and hands back the **answer** — on demand, for the exact
question asked:
- `HuntTripForecastQuery { faction_id, band_id, herd_id, kit_id, party_workers, floor,
  preset_floors[], max_party_workers }` → `HuntTripForecastReply { at_composed, per_preset[],
  useful_cap }`, each row a `HuntTripRow { floor, party_workers, turns_to_fill, bound, delivers_food,
  animals_taken, delivered_food, wasted_food }`. **The floor and
  party are echoed** so a client can assert the answer is for its own question. `preset_floors`
  answers the sheet's three buttons in the same round trip, in order.
  **`turns_to_fill`** is turns until the raid **completes** (comes home — pack full OR surplus spent
  OR **the herd runs out**), **`0` = never completed** within `hunt.forecast_horizon_turns`, which
  after the `HerdLost` repair means `bound == "horizon"` and nothing else — so a **floor-`0`** raid,
  whose only stop is the herd running out, carries a real turn instead of the never-completes
  sentinel. **`animals_taken`** is a **KILL count** — a party too small to seat a whole animal kills
  one and wastes the rest (like the resident band), so the delivered payload is
  **`delivered_food`** (`Σ HuntYield::apply(carried)`), NOT `animals_taken × foodPerAnimal`.
  **`wasted_food`** (`Σ HuntYield::apply(wasted)`) gives the waste fraction
  `wasted_food / (delivered_food + wasted_food)`. **"Too lean to raid" is `delivered_food == 0`** (no
  surplus at any party size); a herd at/below its floor reads `0` on all three. Because the take is
  bounded by the standing surplus, the payload **plateaus** with party size once the surplus binds —
  and that plateau is **`useful_cap`**, scanned server-side over `1..=max_party_workers`
  contiguously, because the client no longer has a table to find it in.
  **`bound`** names WHICH stop ended the trip — the `HuntTripBound` key, one of the raid's **four**
  stops. Pinned by `expedition_hunt::every_pre_launch_estimate_row_names_one_of_the_raids_four_stops`,
  which sweeps floors × parties through the query and holds the live key set in one place so a fifth
  cannot appear without a client clause. A launched party's own bound is
  `PopulationCohortState.expeditionTripBound`.
  `deliversFood == false` means the **species** is inedible (a wolf), not that the policy denies — such
  a row still carries a real `turnsToFill` and a `deliveredTrade` payload. **Travel is excluded** — the
  number means "turns spent hunting once you arrive".
- **`PopulationCohortState.expeditionForecastHorizonTurns` — the SCALE the "never completed"
  sentinels are relative to** (`expedition_config.hunt.forecast_horizon_turns`, **60**). A global
  lever echoed onto **every** cohort, same idiom as `expeditionViabilityWarnTurns` /
  `huntPerWorkerProvisions` / `expeditionPerWorkerCarry`. Every horizon-relative sentinel this
  subsystem publishes shipped without it, so the client could only word an unbounded forecast as
  *"away many turns"* — which is not a bound a player can compare anything against:
  `HuntTripEstimate.turnsToFill == 0`, `DenialEstimate.turnsToCollapse{,Low,High} == 0`, and
  `PopulationCohortState.expeditionTripBound == "horizon"`.
  **ONE lever serves the hunt table and the denial table** — `denial_projection_at` and
  `hunt_trip_forecast_seeded` both read `hunt.forecast_horizon_turns`, so there is deliberately no
  second horizon on the wire and no way for a client to quote the wrong one. (`labor_config`'s
  `yield_average_horizon_turns` / `arrivals_horizon_turns` answer a *different question* — a
  resident source's steady rate and its arrival schedule — and neither publishes a sentinel: an
  arrivals vector's own length **is** its horizon.)
  > **IT IS NOT A TRIP LENGTH.** The bounded case reads *"Away ≈36 turns — 18 hunting, 18
  > travel"*, so the unbounded case has to be a **lower bound on that same span** or the two are not
  > comparable and the player is worse off than with "many". The horizon bounds the **hunting** only
  > (`turnsToFill` excludes travel, above) and the round-trip travel is a separate, already-known
  > term (`ceil(2 × hex_distance / bandMoveTilesPerTurn)`), so the floor on the whole trip is
  > **`horizon + round-trip travel`** — *"Away more than 78 turns"*, never *"more than 60"*.
  > Quoting the horizon alone understates the trip by the entire walk, and a number wrong in the
  > **reassuring** direction is worse than the "many" it replaces.
  >
  > Pinned on the **exported snapshot** by
  > `expedition_hunt::every_cohort_publishes_the_forecast_horizon_on_the_wire`, which also asserts it
  > is **positive** — a lever published as `0` would let the client render *"more than 0 turns"*,
  > the exact failure the field exists to prevent.
- `HerdTelemetryState.{provisionsPerBiomass, fodderPerBiomass}` — the **BAND /
  local-hunt** terms, from which the client composes the ceiling at **any** floor:
  `max(0, B − floor·K) × rate`. **THERE IS NO BUILD TERM ANYWHERE IN IT** — a build is staffed in
  its own right (`docs/plan_standing_upkeep.md` §2.2), so neither this nor the crew term beside it
  carries one; the `*BuildFraction` fields that used to are `(deprecated)`.
  `huntPolicyCeilings` is a retired
  `(deprecated)` slot: four rows cannot answer a continuous dial (`yield-forecast.md` → "the sim
  exports the answer" and its one narrow exception). A herd below a floor composes `0` for it, which
  is the escapement rule rather than a special case. **Formerly sourced by
  projecting the herd's `fauna::hunt_forecast`** (`SourceYieldForecast::ceiling_for`) —
  the **only** wire representation of a herd's per-policy ceilings (the scalar
  `ceilingSustain`/…/`ceilingCorral` twins, which carried literally the same numbers, are now retired
  `(deprecated)` slots), and the take path pays exactly them
  (forecast == actual). That also makes `Corral` **phase-correct for free**: the ordinary hunt
  ceiling while the pen is being built by the keeper band's own `builders` pool, and the **full
  corral yield**
  once `is_corralled()` (a penned herd forecasts as `SourceYieldForecast::tended` — every ceiling is
  its managed yield). There is **no expedition ceiling field** — the retired
  `expeditionProvisionsPerTurn` was exactly the "one number that means a flow for Sustain and a stock
  for Surplus/Deplete" design smell the estimate table replaces.
- `PopulationCohortState.huntPerWorkerProvisions:float` (one hunter's
  provisions/turn throughput = `labor_config.hunt.per_worker_biomass_capacity ×
  fauna_config.hunt.provisions_per_biomass`) and `.expeditionViabilityWarnTurns:uint`
  (`expedition_config.hunt.viability_warn_turns` — the NOT-VIABLE threshold the client applies to
  `turnsToFill`) — global levers echoed onto **every** cohort (the `workRange` idiom; the
  outfit UI lives on the resident-band panel).

**The two hunt readouts, and what each reads:**
- **Expedition (pre-launch raid)** — a query: `HuntTripForecastQuery` →
  `deliveredFood` (the payload; `0` = too lean, no surplus at any party size), `wastedFood` (the waste
  fraction is `wastedFood / (deliveredFood + wastedFood)`), `animalsTaken` (the KILL count), `turnsToFill`
  (comes home in ~N turns; `0` = never completes in the horizon), `deliversFood`. Headline *"≈deliveredFood
  food over turnsToFill turns"* with the animal count + waste % below; the stepper caps where
  `deliveredFood` plateaus. No arithmetic, no ecology model, no rate.
- **Resident band (local-hunt yield preview)** — pure arithmetic over the **band** ceiling, **× the
  cohort's already-exported `outputMultiplier`** (a band applies its morale/discontent productivity
  modifier at payout): `rate = min(workers × huntPerWorkerProvisions, bandCeiling_for(policy)) ×
  outputMultiplier`. That is arithmetically `hunt_take(.., carry_room_biomass = INFINITY)` — what the
  band's Hunt labor arm really takes (the conversion and the multiplier are linear, so they factor out
  of the `min`, and the exported ceiling is biomass-clamped exactly as the take is).

`core_sim/tests/expedition_hunt.rs` + `core_sim/tests/hunt_yield_vector.rs` pin **both — each to the
sim's REAL behaviour, never to another preview** (the lesson of the ~34-vs-~6-turn Surplus bug: the old
guard compared the client against `hunt_trip_forecast`, so two copies of the same wrong ceiling agreed
with each other while both disagreed with the take). For the **expedition** readout,
`hunt_yield_vector::a_hunting_expedition_delivers_both_products_it_forecast` asserts the **exported**
answered row against the two accounts a real driven raid actually credits — the home band's
own store, `FOOD` for provisions and `TRADE_GOODS` for pelts — over an edible × an inedible species ×
Sustain/Surplus/Deplete, and `expedition_hunt::a_far_just_launched_party_projects_the_estimate_delivery`
pins the in-flight projection to the exported row for the same `(policy, party size)`. For the **band**
readout, `expedition_hunt::exported_snapshot_fields_reproduce_band_hunt_take` does the same against
`hunt_take(..)` (healthy / clamp-binding depleted / collapsing herd × every worker count × all four
policies × a unit and a discontent-reduced output multiplier). If either readout ever drifts from the
sim, those tests fail.

### A raiding party carries its quarry's NAME, not just its key

`ExpeditionMission::Hunt`/`Deny` carry **`target_species`** — the herd's species display name
(`"Red Deer"`) — beside the `fauna_id` that keys it. Published as `expeditionTargetSpecies`, and it is
what the client renders; `expeditionTargetHerd` remains the key every command addresses the herd by,
and a player never sees it.

**The two are not redundant, because the party outlives the herd list.** Herd telemetry is fog-gated
to hexes with `Active` visibility and pruned at local extinction, and a detached expedition is
deliberately **not** a vision source (`calculate_visibility`, `Without<Expedition>` — comm-range gating
means a party must not light up the faction map from wherever it stands). So a hunting party's own
target routinely leaves the published list *while the party is still bound to it*, and a client joining
the id against that list had nothing left to render but the raw id (issue #378).

**Resolved at launch, in `outfit_raiding_party`** — the shared gate that already refuses a raid whose
herd the registry cannot resolve, so a successful launch always has a name and both raiding verbs get
it from one place. That is also the only moment the name is reliable: a capture-time registry lookup
would survive fog and still go blank on extinction, which prunes the registry itself.

**The sim's own event feed obeys the same rule, through `ExpeditionMission::target_display`** — the
species when it resolved, the `fauna_id` only as a last resort, one definition so no call site
re-implements the fallback. Every player-facing expedition line reads it: both launch lines
(`ExpeditionSent`), the shared lost-herd guard, and all four completion lines in the `Hunting` arm.
**The `detail` tokens are untouched** — `herd=<fauna_id>` is the key the client addresses the event by,
so a line names the species and its detail names the herd. The id tier is reachable only from a mission
built without a resolved name (a fixture, or a restore of a frame that carried none), since
`outfit_raiding_party` guarantees one on every real launch.

It rides `ExpeditionRecord` like the rest of the mission, so a rollback restores it — necessarily,
since it cannot be re-derived once the herd is gone
(`harvest_floor_rollback::an_expedition_floor_round_trips_through_the_mission_and_the_rollback`).
`expedition_hunt::a_party_names_its_quarry_when_the_herd_has_left_the_snapshot`
pins the whole chain on the encoded wire, with a positive control: the party's target is absent from
the published herds while its hex is only `Discovered`, and present once the hex is `Active`.

## Denial is a MISSION, not a floor — and it changes ONE line

`ExpeditionMission::Deny { fauna_id, target_species }`, wire key `"deny"`, launched by
`send_denial_raid <faction> <band> <party_workers> <fauna_id>` (`SendDenialRaidCommand`, proto field
**49**). Authoritative design: `docs/plan_denial_raid.md`, which rides on
`docs/plan_hunt_through_combat.md`.

**It carries no floor and no rate, and that is why it is a mission.** `floor = 0` could not do this
job for a reason that has nothing to do with the number: `fauna::quantise_animal_take` bounded the
kill by the party's **carry**, so at any floor a party still only killed what it could haul. That is
the right model of subsistence hunting and exactly the wrong model of denial, whose premise is
killing what you have no intention of using. A *bound* is not reachable by any value of a *number*.

**The one line is `fauna::EngagementStop`**, carried on the mission
(`ExpeditionMission::engagement_stop`) and read by the quantiser and `fauna::hunt_take_bound`
together:

```text
hunt:    killed  = min(animals_the_pack_seats(room), brought_down)   // WhenPackFull
denial:  killed  =                                   brought_down    // Never
both:    carried = min(killed × body_mass, carry_room)               // IDENTICAL
```

**The room is not an arm of either line** — both spend it on `engaged`, before the retreat and the
fight (`fauna::animals_affordable`), which is why a raid at its floor takes no casualties for
animals it was never going to kill. `quantise_animal_take` holds no ceiling at all; see
`fauna.md` → "THE ESCAPEMENT ROOM IS SPENT AT STEP 1".

`carried` is untouched, so a raid still banks whatever it can haul on the way home — a rounding error
against what it killed, which is the point, and the rest is `AnimalTake::wasted`. Nothing else
changes: `ExpeditionPhase`, outfitting, travel, the `Hunting`/`Delivering`/`Returning` cycle and the
whole take path are the hunt's. `ExpeditionMission::raid_orders` is the one seam the `Hunting` arm
resolves both verbs through.

- **`hunt_floor()` reports `STRIP_IT_BARE`** — the escapement ceiling is the herd's whole standing
  stock. It is *derived*, never a lever, and **`floor` appears nowhere in the command, the feed line
  or its detail**: the launch text takes four positional tokens plus an optional named `kit <id>`,
  and refuses anything else (`CommandParseError::UnexpectedArgument`) rather than accepting a number
  and dropping it.
- **A floor-`0` HUNT is still a different thing, and the difference is the ENGAGEMENT, never the
  carry.** It grinds to extinction through the lost-herd guard, one pack-seated animal a turn
  once its pack is full; denial drops the pack as a bound on what it **engages** and kills everything
  it brings down. **Both haul their real pack** — `carry_room_biomass` takes no floor argument and
  `NO_CARRY_BOUND` means *inedible quarry* and nothing else. A floor-`0` hunt used to pass it, so
  `carried = killed × body_mass`: the party was recorded hauling home everything it killed, its hunt
  report published `wasted_biomass = 0` for a raid that left a range of carcasses, and
  its hides accrued off the whole kill — against the "both scale off what the party
  carries, never what it killed" rule two sections above. On a 4-hunter mammoth raid the exported row
  promised **16 food / 0 wasted** while the party banked 3.2 food; it now promises 3.2 / 140.8 and
  pays exactly that. Pinned by
  `denial_raid::{a_floor_zero_hunt_hauls_only_its_pack_and_reports_the_waste,
  denial_and_a_floor_zero_hunt_account_carry_identically}` and
  `hunt_yield_vector::a_floor_zero_raid_delivers_and_wastes_what_its_exported_row_promised`.
- **An INEDIBLE quarry is a legitimate denial target** (a wolf). Nothing on the path divides by a
  food rate it has not established positive: the pack is inert there for the same *product* reason it
  is inert on a hunt, and the raid is paid in pelts.

### A raiding party is bounded by the BAND, not by a config lever

`handle_send_expedition` and `outfit_raiding_party` bound a party by **`available_workers`** and
nothing else, on all three verbs. Authoritative design: `docs/plan_denial_raid.md` §3.1.

The lever they used to also consult (`max_party_size`) was doing two jobs under one name. The
**rules-cap** half had no design note behind it and was deleted: at `8` it refused a party of **9**
from a band holding **16**, against a Red Deer herd needing exactly
`2.91 regrowth / 0.35 kills-per-hunter` = 9 — two unrelated eights, and the config one won. The
**sampling** half (renamed `estimate_party_sizes`) survived it by one arc and is now gone too; see
"The forecast is ASKED FOR" below.

**This changes a HUNT's party sizing too, deliberately.** A hunting party is no longer capped at 8
either; that is the ruling followed to its conclusion, not an accident. Pinned by
`server::tests::a_raiding_party_is_bounded_by_the_band_and_not_by_the_sampling_lever`, which asserts
**both** verbs launch the party the band can field *and* that both still refuse a party past the
band, so "the bound moved" cannot degrade into "the bound vanished".

**Wary herds are therefore expensive, not undeniable.** Wariness raises the requirement; nothing caps
it below what the band can field.

### The forecast is ASKED FOR, not pre-computed

**The client sends a `QueryCommand` and the sim answers it on the same socket** — see
`sim_runtime/proto/command.proto` for the wire and `core_sim/src/forecast_query.rs` for the answer.
One question, one herd, one band, answered from the live world:

```
HuntTripForecastQuery   { faction, band, herd, kit, party_workers, floor, preset_floors[], max_party_workers }
  -> HuntTripForecastReply   { at_composed, per_preset[], useful_cap }
DenialRaidForecastQuery { faction, band, herd, kit, party_workers, max_party_workers }
  -> DenialRaidForecastReply { at_composed, party_needed }
```

**The command socket answers now.** It was always an ordinary bidirectional TCP stream; "one-way" was
a protocol choice. `handle_proto_client` `try_clone`s the stream, spawns a writer thread over a
per-connection reply channel, and `Command::Query` carries a clone of that channel — so an answer
reaches the connection that asked, correlated by `request_id`. A query is dispatched **ahead of** the
generic command arm: it never enters the replay log (replaying a question reproduces nothing, into a
channel whose connection is gone) and it `continue`s past the post-command recapture, because it
changed nothing to republish.

**It fails closed, with a token.** `no_active_world`, `unknown_herd`, `unknown_band`, `unknown_kit`,
`kit_wrong_job`, `invalid_floor`, `invalid_party` — named constants in
`sim_runtime::commands::query_error`, so the client's match arms and the server's answers cannot
drift. A kit is **never** quietly swapped for the job default, the same rule the launch commands
follow: a party silently re-armed answers a different question than the one asked. Every floor is
validated before any is answered, so a bad preset cannot come back as a short `per_preset` list whose
positions no longer line up with what was asked for.

**Every row echoes the floor and party it answered.** That echo is what the retired
`huntTripEstimatesKitId` / `denialEstimatesKitId` disclaimers were compensating for: a client can
assert the answer is for its own question instead of trusting position in a list.

#### What the query replaced, and what it cost

`HerdTelemetryState` used to carry `huntTripEstimates` (floors × party sizes), `denialEstimates` (party
sizes), `denialPartyNeeded` and the two `*_kit_id` disclaimers. All five are `(deprecated)` slots in
`snapshot.fbs` now. They were pre-computed **for every huntable herd, on every frame**, and they were
wrong for anyone who had worn their gear or picked another kit:

- **One kit for every band** — the hunt job's *default*, over a **fresh** component set. A band whose
  spears have run dry hunts at the intrinsic `attack 1`, which against a Red Deer's `defense 1.0` is
  an effective attack of **zero**: no party of any size works, while `denialPartyNeeded` quoted `9`.
- **A detached raid priced at resident-hunt lethality** — the tables read `CombatConfig::tuning()`
  where every other expedition path applies `expedition_danger_multiplier`. That under-states
  casualties and so over-states the take. The multiplication now happens in exactly one place,
  `CombatConfig::expedition_tuning()`, which `advance_expeditions`, the launch line, the in-flight ETA
  and the query all resolve through.
- **Marks on a dial, not the player's numbers** — the client resolved its composed floor and party to
  the nearest sampled rung and quoted *that* row.

**The measurement, which is the whole argument.** Same harness before and after
(`core_sim/tests/capture_cost.rs`, run with `--ignored --nocapture`): a fully-revealed 80×52 map, fog
off, five captures after two warm-up turns, **debug** build.

| phase | with the tables | without |
|---|---|---|
| `snapshot.build` | **49.51 ms** | **3.15 ms** |
| `snapshot.build.herds` | **46.22 ms** (93.4%) | **0.06 ms** (1.8%) |
| `snapshot.build.forage_patches` | 1.35 ms | 1.31 ms |

Capture is **15.7× cheaper** and the herd pass ~770×. The "after" run carried **131** huntable herds
against the "before" run's **128** (the registry moves turn to turn), so the comparison is
conservative. `forage_patches` is now the largest remaining section.

**This reverses the decision this section used to record.** The old argument ran: the two tables are
~95% of capture, a per-(band, herd) answer multiplies that by the band count (three bands ≈ 165 ms per
turn), so repricing forces a structural choice rather than a parameter change — *"move the estimates
off the per-turn capture, which the one-way command channel does not support today"*. That is exactly
what happened: the channel learned to answer, and the multiplication never has to be paid because
nobody asks 131 times a turn. `docs/plan_denial_raid.md` §3.1's three blockers are all resolved.

#### The sampling ladders are gone with the tables

`expedition_config.estimate_party_sizes` and `deny.requirement_rows` are **deleted**, with their
validators and drift tests. Both existed only to make a pre-computed table affordable — sparse where
it was expensive — and a query answers one herd for one band when a player asks, so the sampling buys
nothing. What they were paying for is worth stating, because it is what "exact" now means:

- **`useful_cap` walks `1..=max_party_workers` contiguously.** A sampled scan finds a *sampled*
  plateau: it could only answer "the rung after which the payload stopped rising", so a herd whose
  true plateau was 6 reported 4 and the sheet told the player six hunters were three too many. The
  bound is the band's own idle workers, which is where the stepper stops anyway; `0` scans nothing.
  The scan is the **server's** half only — it needs the table the query replaced. The engagement-crew
  floor the client maxes into it derives from fields the herd row still carries, so it stays
  client-side with the prose that explains it.
- **`party_needed` searches `1..=max_party_workers` upward and stops at the first party that
  succeeds.** It is the forward simulation's answer, not the closed form's — `denial_party_needed` is
  linear in the party and therefore blind to the whole-animal quantiser and to the fight, so it errs
  and was only ever a bound on the search. The walk stops at the first success, so a deniable herd costs a handful of projections; a
  herd nothing can deny costs the whole range, which is exactly the answer that has to be earned.
- **The sentinel CHANGED MEANING, and it is a published number, so say so.** `party_needed == 0` now
  means *"no party YOU can field drives this herd down"* — the search ran to the band's own last
  worker and found none. The retired `denialPartyNeeded` had no notion of who was asking, so it could
  name a party the band had no hope of raising and present that as the answer. Neither reading is
  ever *"send nobody"*. Stated on `DenialRaidForecastReply::party_needed` and in the proto, because a
  client that kept the old reading would render a solvable situation as hopeless or the reverse.

`PopulationCohortState.maxExpeditionPartySize` went too, and it is worth saying why it was harmless
and still wrong: it echoed the ladder's last rung, **capped nothing**, and every client site that read
it said so in capitals ("IS NOT A RULES CAP AND MUST NOT BE APPLIED HERE"). A field whose name asserts
a rule that four comments exist to deny is a field to delete.

### `party_needed` — the party the sheet OPENS on

`DenialRaidForecastReply.party_needed` is the **smallest party whose own raid `succeeded`** —
`past_recovery` or `herd_lost` — so the sheet cannot open on a value whose verdict, one line below it,
refuses to say the herd goes down. The stepper seeds there instead of at an arbitrary default, which
turns the control from a guessing game into an adjustment.

- **The test is `DenialOutcome::succeeded`, NOT "not `repelled`"**, and the two differ on exactly one
  verdict: `horizon`, a raid the projection ran its whole length with the herd still standing. A
  `!= repelled` seed quoted a Wild Aurochs party of **5** under its own verdict line *"Wild Aurochs is
  still standing when the forecast runs out"* — a horizon row presented as the party that works, and
  in play it was short. The gap is not one row: measured over the shipped roster it runs to **21
  hunters** between the first non-repelled party and the first that actually crosses the line (Wild
  Boar / Grey Wolf Pack at full `K`).
- **The wire `String` gets back to the enum through `DenialOutcome::from_wire`, never through a
  second list of keys at the call site.** `from_wire` searches `DenialOutcome::ALL` by `as_str`, so
  the round trip is total by construction and no key is spelled twice — which is the drift that
  produced the bug in the first place. Pinned by
  `systems::expeditions::denial_outcome_tests::every_denial_outcome_round_trips_through_its_wire_key`.
- **The closed form is DELETED, not kept beside the search.** `fauna::denial_party_needed` and its
  input `fauna::herd_replacement_animals` are gone. A `pub fn` returning a *linear approximation* of a
  number the sim now answers exactly is an invitation to call the wrong one — the same rule that
  retired `HuntTripEstimateState`, with a sharper edge. What it knew now lives on
  `forecast_query::seeded_denial_party_for`, because it explains why that walks a projection:
  - **It erred low, being linear:** blind to the whole-animal quantiser and to the fight (a party
    has to *land* its strikes; `defense` and `durability` decide how many turns a kill takes). It also
    used to err *high*, being blind to `animals_engaged`'s `max(1)` floor — that floor is retired, so
    the reach it approximates is now the linear thing it always assumed.
  - **The number it divided was subtler than "the herd's regrowth"** — the replacement a raid must
    out-kill is the **peak on the path down**, not the rate where the herd stands. The logistic curve
    peaks at `K/2`, so a party sized on a *full* herd's instantaneous regrowth (which is **zero**)
    reads one hunter, drives the herd to the food peak, and stalls there forever. Below `K/2` the
    current stock binds and the raid accelerates. The forward simulation gets this for free: it *is*
    the curve, running the same `regrow_biomass` + take pair the live raid does.
  - **The rounding question disappeared with it.** The closed form had to round `floor(x) + 1`, never
    `ceil(x)`, because a party that exactly *ties* with the replacement declines nothing and `ceil`
    is wrong by one at precisely the round number a tuner is most likely to author (the reported Red
    Deer: `2.91 / 0.35 = 8.3`, so **nine**). A search over whole parties never rounds — it asks each
    one whether it succeeded, and a tie does not.

**The party a forecast is quoted for is the ASKING BAND's**, at its own kit and its own live
`BandEquipment` wear, and against **this** quarry — `hunter_profile_against`, not the tables'
quarry-blind `hunter_profile_unbounded`, because a mass-bounded weapon is only a weapon against
animals it can hold. A trapping party after a mammoth is quoted the bare hand's attack, which is the
gate refusing the raid: the same answer the take will give.

Guards: `denial_raid::{the_reported_red_deer_raid_is_staffable_and_its_seeded_party_declines_the_herd,
a_herd_no_quoted_party_can_collapse_reports_no_viable_party_and_still_reads_repelled}` — the first
verifies the seeded party by **driving real raids over seeds** rather than by re-reading the
projection (the retreat is a draw and this herd is a near-run thing), paired with the ordering claim
that one hunter fewer leaves the herd standing higher; the second pairs the sentinel with the
requirement that every party still carries a verdict, so answering `0` by refusing to search would not
pass. The rounding is pinned on the pure helper by
`fauna::tests::a_requirement_of_eight_point_three_hunters_is_nine_and_a_tie_is_never_enough`. The
search itself is pinned by
`forecast_query::tests::{the_seed_is_the_smallest_party_that_actually_drives_the_herd_down,
a_party_the_band_cannot_raise_seeds_the_sentinel, only_a_raid_that_finished_the_herd_counts_as_a_success}`
— the first derives the seed and then re-runs the projection at every party below it, so it is a
statement about the *search* rather than a pinned number.

**Client-side:** every outfit stepper caps at the band's **`idleWorkers`**; the denial stepper
additionally *seeds* at `party_needed`, rendering `0` as *"no party you can field can"* rather than as
a party size. There is no nearest-rung lookup any more — the answer is for the size that was asked
for, and it says so on the row.

### Success is the point of no return, not zero

`fauna::herd_past_recovery(biomass, K, ecology)` — biomass under `ecology.collapse_fraction × K`, read
through the **same** `classify_ecology_phase` comparison the client's ecology band renders, so the
raid's completion and the phase word cannot disagree about where the line is. Below it
`net_biomass_delta` zeroes the growth flow and the herd declines irreversibly at `collapse_rate` with
the party gone.

So the `Hunting` arm's completion for a denial raid is `done = past_recovery`, `relaunch = false`:
**the party pushes the herd under the line and walks away**, rather than killing every animal. It
never delivers mid-trip and never relaunches — there is nothing to come back for, and
`raid_is_recurring` is a question about a floor the mission does not carry. That settles
`plan_denial_raid.md` §6's second open question.

**Why ordinary hunting never does this by accident:** any escapement floor above `collapse_fraction`
stops the take long before, by the arithmetic of `max(0, B − floor·K)`. Pinned by
`denial_raid::a_denial_raid_reaches_collapse_where_a_hunt_does_not`, whose hunting half is given an
*unbounded series* of trips and still cannot cross the line — the floor is what stops it, not the
party's patience.

### The forecast: `turns_to_collapse`, as a range

`systems::denial_forecast` — the denial analogue of `hunt_trip_forecast`, and the same bounded forward
simulation (`fauna::regrow_biomass` then `expedition_take_biomass`, in the live order) through the
**same** helper, so a preview cannot quote a raid the sim does not run. It is evaluated at **three
quantiles** (`±combat_config.forecast_range_sigmas` and the expectation), which is slice 6's shape
applied to a turn count instead of a biomass (`docs/plan_hunt_through_combat.md` §6.4).

- **`low` is the FEWEST turns** — more animals staying and more strikes landing is the *optimistic*
  draw for a raid, so `+sigmas` produces the low end. Getting that backwards would report a band that
  widened in the wrong direction on exactly the wary quarry it exists for.
- **A `None` end is honest, not a gap.** `turns_to_collapse_high = None` beside a `Some` likely reads
  *"only on a good run"*; on the wire both are the `0` sentinel and `outcome` is what disambiguates.
- **`DenialOutcome`** (`"past_recovery"` / `"herd_lost"` / `"repelled"` / `"horizon"`) is why the
  readout is never a blank (§3). **`Repelled`** is the one the design insists on — the party's kills
  do not outpace the herd's regrowth, a verdict about the *party*; `Horizon` is a statement about the
  *clock*. It is measured as **net progress against the herd over the projection's second half, in the
  herd's own body mass**: a raid that could not take one more animal's worth off the standing stock in
  half a horizon is not winning slowly, it is not winning. Read off one turn it would be undecidable —
  at the equilibrium a repelled raid settles into, one turn's kills and one turn's regrowth are equal
  by definition.
- **The projection does not model kit wear**, exactly as `hunt_trip_forecast` does not: both are
  quoted for a `HuntingParty` resolved once. A raid long enough to run its spears dry therefore
  outruns its own forecast — reachable only on a herd holding more animals than
  `hunting_kit.starting_durability / wear_per_kill`.
- **A TINY `K` is its own regime, and it is where `Repelled` was wrong rather than merely coarse.**
  The projection resolves the retreat at its expectation, so on a herd of three animals it presents a
  *fractional* standing count to the fight (`3 × (1 − wariness 0.60) = 1.2`, then `0.8`) — and the
  damage ledger used to clamp its cross-turn bank to `standing × durability`, which below one body is
  a permanent zero. Eight hunters on three Crag Goats were therefore reported repelled by a regrowth
  of under one biomass a turn, while a driven raid erased the herd in two turns. The repair is the
  ledger's (see `combat.md` → "Damage carries between turns"); what belongs here is that **the tiny-`K`
  regime is the one to test a raid readout in** — most fixtures hold herds of dozens, where the mean
  engagement is comfortably above one animal and the stall cannot appear. Guard:
  `denial_raid::a_tiny_wary_herd_is_erased_and_the_forecast_no_longer_calls_it_repelled`, paired in
  the same test with a genuine `Repelled` case so the verdict cannot be fixed by deletion.
- **Whole-animal quantisation still holds a tiny herd above the line, and that is the model, not a
  bug.** With `collapse_fraction × K` under one `body_mass` — three 6-biomass goats give a line at
  `2.7` — the raid cannot cross it by taking a fraction of a goat: it kills whole animals off a stock
  that regrows continuously, so the crossing happens on whichever kill leaves a remainder under the
  line, and a herd standing between the line and one body mass is simply waited out
  (`animals_affordable == 0`, the take reports `HuntTakeBound::Floor` — genuinely the floor here,
  since the bank has caught up with a surplus that holds no whole body). It makes the projected turn
  count lumpy on a herd of two or three, which is honest — a party cannot half-kill a goat.

**Wire:** `DenialRaidForecastQuery` → `DenialRaidForecastReply { at_composed, party_needed }`, with
**no floor axis**, because the mission carries none — you choose a herd and a party size, and that is
the whole of the order. `at_composed` is one `denial_forecast` at the exact party asked for; each
projection costs `3 × hunt.forecast_horizon_turns` turn-steps, the three being the reported band's
quantiles. `party_needed` is the contiguous upward search — see "`party_needed` — the party the sheet
OPENS on".

This used to be `HerdTelemetryState.denialEstimates`, a row per sampled party size on every huntable
herd on every frame.

**The waste is a FOOD SCALAR again, and the gap that leaves is stated rather than hidden.**
`DenialForecast::wasted_trade` and `delivered_trade` are **retired** with the trade axis (arc #527),
and so is `denial_raid::a_denial_raids_waste_is_reported_in_both_products`, the test that pinned
them. What that test said is still true and is no longer measured: **a carcass left on the range
takes its hide with it**, so on an edible quarry whose pack binds hard, the raid's real destruction
is under-reported by everything it did not bring home in materials.

> **The same shape WOULD work here, and it is deliberately not built.** `HuntTripRow` just proved a
> per-material vector states a projection perfectly well (`delivered_material`, above), so the
> original reasoning — *"a material cannot be summed into this table"* — does not survive as an
> argument against a `wasted_material` beside `wasted_food`. **Ray has ruled the waste line out of
> scope**: the waste is already legible as a percentage, so the missing half buys a second reading of
> a fact the sheet states. Recorded so the next person does not re-derive the wrong reason for it
> being absent. What must NOT happen is a flat "wasted materials" scalar — that is the retired trade
> axis under a new name.
>
> The ruling is about the **waste** alone. `server::describe_denial_ledger` states the food ledger
> **and one clause per delivered material**, off `DenialForecast::delivered_material` — the same
> field the client's own take line (`SourceForecast.denial_take_bbcode`) reads off the same
> forecast, so the launch ack and the sheet cannot disagree about one raid. It falls back to
> *"nothing worth hauling from this quarry"* only when there is neither food nor material to weigh.
> Pinned as a pairing by
> `server::tests::an_inedible_raids_ack_names_the_materials_its_forecast_promises`, because *"always
> name the hides"* would otherwise be satisfiable by deleting the fallback.

> **An INEDIBLE quarry is the wrong place to look for this, and not for the obvious reason.**
> `carry_room_biomass` answers `NO_CARRY_BOUND` for a species paying no provisions, so a wolf raid's
> pack **cannot bind**: it hauls every hide it takes and its waste is honestly `0`. The blindness
> lives on an **edible** quarry, where the pack binds hard.

**A LOST HERD IS ONE OF DENIAL'S TWO WINS, and the guard's line says so.** The lost-herd guard is
shared by both raid verbs and read in opposite directions, so it branches on `RaidOrders::stop` for
its message exactly as the `done` arm does: a hunt reports *"Hunting expedition lost the … "* with
`reason=herd_gone` (its quarry slipped away), a denial raid reports *"Denial raid wiped out the …"*
with `reason=` **`DenialOutcome::HerdLost`'s own wire key** — the verdict `DenialOutcome::succeeded`
returns true for and the launch sheet quotes as a win. The reason token is read off the enum rather
than spelled, so the exit and the pre-launch verdict cannot name the outcome two ways. Pinned as a
**pair** by `denial_raid::a_denial_raid_that_loses_its_herd_reports_a_win_and_a_hunt_reports_a_loss`
— a denial-only assertion would pass just as well if the hunt's line had been reworded too, and the
hunt's really is a failure.

### What it costs, and the kit cost needed nothing new

Travel, party exposure and a near-zero return are the listed costs. The fourth is the **kit**, and it
holds for a denial party with no new mechanism: `advance_expeditions` already charges
`wear_hunting(.., take.killed)` per animal **killed** and `wear_sled(.., take.carried)` per unit
**hauled** — wear tracks *use*, never turns elapsed, which is what `plan_denial_raid.md` §1.2
required. A denial raid is by construction the most kill-intensive act in the game, so it burns the
most irreplaceable kit for no food return; a party that engaged nothing spends nothing. Pinned by
`denial_raid::a_denial_raid_burns_more_kit_than_a_hunt_and_only_for_kills`.

**Not in scope, settled rather than deferred:** no target faction (denial aims at a herd, not a
player, so there is no nullable field nothing reads) and no plant twin (`reseed_floor_fraction`
guarantees a stand returns and plants have no Allee term, so a herd can be erased permanently and a
stand only set back).

**The in-flight `expeditionProjectedDelivery` is `None` for a denial party**, deliberately: its
readout is the collapse verdict, not a delivery ETA. Quoting "next delivery" for a raid whose whole
point is that nothing comes home would be the food-only blindness the mission reverses. The client
half — the third launch verb, the range verdict line, the waste readout and the in-flight collapse
line — is slice 2.

## One fold-back, two moments

`systems::expeditions::fold_party_into_band` is **the** settlement routine for a party that has come
home: `working` back into the band's pool, the leftover pack into its larder, its material batches
into that same store, `sync_size`. Its companion
`expedition_returned_event` builds the `ExpeditionReturned` line. Two callers, one routine:

- **`advance_expeditions`'s `Returning` arm**, for a party that walked home; and
- **`handle_recall_expedition`**, for a party recalled while standing on its home band's own tile.

**The recall's condition is positional and state-based, never "turn 0"**: exact co-location with the
band plus `party_owes_a_report(expedition) == false`. Recalling a party that had not moved used to
publish `Returning` and then make the player wait a turn for a fold-back of a party that had gone
nowhere, which read as the order doing nothing.

- **"At home" is exact co-location, not the comm range** the `Returning` arm folds back within. A
  party two tiles out is genuinely away, and settling it from there would *teleport* its workers home
  rather than cancel an order that had not taken effect.
- **"Owes a report" is about the map, not the pack.** The one thing an out-of-band fold-back cannot
  do is promote `Expedition::pending_reveal` to the faction map — that flush needs the visibility
  ledger and the elevation field, which only the system has — so a party still holding observed tiles
  takes the ordinary `Returning` path, which flushes and *then* folds. Food and materials are deliberately
  **not** part of the test: the shared routine settles both identically, so making a party standing in
  camp with a full pack wait a turn would reintroduce the round trip the cancel removes.
- **The cancel emits both the `ExpeditionRecalled` ack and the `ExpeditionReturned` line** — the ack
  answers the button press (`status=cancelled`), the fold-back line reports what happened to the
  world. The `ExpeditionReturned` detail stays `status=returned` in **both** cases: nothing about the
  world differs between a cancel and a homecoming, so encoding *how the fold-back was triggered* into
  a field that otherwise reports *what happened* would force every reader to know both.

**An orphaned party folds back where it stands.** The `Returning` arm now tests
`near_home || home_pos.is_none()`. `near_home` answers *"am I close enough to hand things over?"*;
whether there is anyone to hand them **to** is a different question, and conflating them left a party
whose `home_band` could not be resolved permanently `false` on the fold-back **and** on the
`else if let Some(home)` retarget below it — a live cohort parked on its tile for the rest of the
game, workers, pack and pelts held out of the economy. The arm's own comment already stated the
intent ("no home band left to receive them means the haul is simply lost, exactly as the carried food
is"); it was merely unreachable. Guards:
`server::tests::{a_party_recalled_in_camp_folds_back_without_waiting_a_turn,
a_party_recalled_in_the_field_walks_home_and_folds_back}` — the pair, so "cancel at once" cannot
become the only way a recall ever completes — and
`expedition_hunt::a_returning_party_with_no_home_band_left_does_not_haunt_the_map`.

---

## A shipment is a party that WALKS IT — the trade verb (arc #527, issue #517)

Design of record: `docs/plan_contact_and_logistics.md` §Q5. The **first rider on the connection
primitive** #538 landed. `ExpeditionMission::Trade { destination_band, destination_name }` is the
fourth verb on the one traveling-party system, launched by
`send_trade_expedition <faction> <band> <party_workers> <destination_band_id> [food <amount>]
[material <material_id> <amount>]... [kit <id>]` (`SendTradeExpeditionCommand`, proto field **55**).

**There is deliberately NO persistent link component.** What maintains a link is a *route*, the route
ladder (#532) is what will hold that state, and building link state before any route exists to hold
it would be inventing the ladder's model in advance. So the rider is an expedition, and its state is
`Expedition::cargo`.

**`balance_supply_networks` pools over the same primitive.** Near same-faction bands that hold a
live tie keep auto-pooling exactly as they did; the shipment is what carries mass where `reach_tiles`
does not — and across a faction line, where free equalization deliberately does not reach.

### The connection gates the LAUNCH, and arrival is not re-gated

`ConnectionLedger::get(ConnectionKey::new(home_band, destination_band))` must exist with
`strength > NO_TIE` — the arc's *"at zero, nothing flows"*. A **parked** edge (strength `0`, meaning
*"we know such a people exist and have no current dealings"*) refuses exactly as a missing one does.

**If the tie decayed to nothing while the party walked, the shipment still lands.** The party is
standing in their camp; presence beats the ledger, and the decision to send was made turns ago.

**There is no same-faction check anywhere on this path** — not in the command, not in the arm that
delivers. Faction is a property of the endpoint (`connections.md`), which is what makes #458
(cross-faction trade) nearly free, and `trade_expedition.rs` delivers **cross-faction in every test**
so the claim is exercised rather than asserted.

### Cargo is food, FODDER and materials — and the hay weight is derived, not guessed

Three accounts, and the third arrived late. `docs/plan_contact_and_logistics.md` said the cargo was
"food, fodder and materials" from the start and `TradeCargoItem`'s `{ id, is_material, amount }`
shape was chosen so *"a third account (fodder) is a value rather than a schema change"* — then the
shipping slice built two thirds of it. **Fodder's absence from a manifest was an oversight, never a
decision** (issue #590); nothing about hay and bread being separate currencies ever implied separate
*logistics*, and the currency question itself is settled in
`.claude/rules/core_sim/husbandry.md` → "WHY HAY AND BREAD ARE TWO ACCOUNTS".

**Food is the numéraire at weight 1.0, so every other good's weight is a statement about how it
compares to bread.** `trade.fodder_carry_weight` is **0.5**, and it is not a taste — it is solved
from the only comparison that means anything, *how long one trader's load feeds one mouth*.

> **"Feeds one mouth for N turns" is the unit, and it is a product.** *Enough hay to feed one goat
> for 40 turns* is the same quantity as *40 goats for one turn*, or *four goats for ten* — which is
> exactly why it is the right yardstick. You cannot compare 6 units of bread to 12 units of hay
> directly, because they are not the same stuff; you can compare **how long each load keeps
> something alive**, and that is one sentence on both sides.

| | units per trader | one unit feeds | **one load feeds** |
|---|---|---|---|
| food | `6.0 / 1.0` = **6** | one person for `1 / 0.16` = 6.25 turns | **one person for 37.5 turns** |
| fodder | `6.0 / 0.5` = **12** | one goat for `1 / 0.29` = 3.4 turns | **one goat for 40 turns** |

- The food column is `trade.per_worker_carry` (6.0) against
  `demographics_config.consumption.per_capita_draw` (0.16).
- The fodder column is anchored on a **mid-sized pennable animal**, because one animal's feed is
  `fodder_per_biomass × body_mass` and the roster spans 500× — crag_goat (`0.05 × 6` = 0.30) and
  wild_sheep (`0.05 × 5.6` = 0.28), which are the animals hay is historically *for*. Solving
  `6.0 / (37.5 × 0.29)` gives 0.55; **0.5 is the clean dial beside it**.
- The spread across the rest of the roster is honest and intended: one load feeds a fowl for 1,026
  turns and an aurochs for **2**.
- **In pen-sized terms, which is how a player meets it:** a 20-goat pen eats 6 hay a turn, so one
  trader's load carries it two turns and two traders carry it four.

> **THE DENOMINATOR IS ONE ANIMAL, NOT ONE PEN — and getting that wrong moves the answer 10×.**
> #590's own scoping proposed ~0.05 by measuring against a *whole herd at carrying capacity*: a red
> deer pen's 72 hay/turn is 240 deer eating at once, so "one turn of a pen's hay" is a fundamentally
> different quantity from "one turn of an animal's hay". The per-animal denominator is the one that
> compares to a *person*, which is what the food side is measured in.
>
> **The consequence, stated rather than buried: a one-worker load is well under a turn of feed for a
> full-sized pen.** Shipped hay is for topping up and for relief; a pen lives off its own fenced
> grass and a local hay field, and no convoy will ever sustain one. That is the intended shape — the
> alternative is a weight that makes hay nearly massless and turns every pen into a logistics
> endpoint — but it is a real consequence of the number and it is on the record here.

**`fodder_carry_weight` is a PLAYTEST DIAL and it is coupled.** It is 0.5 only because a hay unit is
worth about half a food unit in feeding value, which is a fact about `flora_config`'s
`hay_grass.fodder_per_biomass` (0.20) and the fauna roster's `fodder_per_biomass` rates. **Retune
either of those and this number is stale** — it is derived, so re-derive it rather than nudging it.

**The food ledger stays food-only, and the fodder ledger's route arm came alive on all three legs.**
A shipment's hay is booked on `last_fodder_transfers`' `TransferLink::Route` arm — **debited at
launch, credited on delivery, and credited again on the fold-back** so a recalled shipment leaves no
phantom sent-but-never-received figure standing. It is never booked on `last_food_transfers`: the
larder identity `larder_delta == foodIncome − foodConsumption − raidForfeit + transferReceived −
transferSent` is about food that entered a *larder*, and hay never enters one. The
`fodderTransferRoute{Received,Sent}Turn` wire fields were minted dead against exactly this day and
now read non-zero; the local-pair-is-a-rate / route-pair-is-an-event distinction beside them is
unchanged.

> **THE HOMECOMING GUARD HAS TO COVER ALL THREE ACCOUNTS.** `fold_party_into_band` carries a comment
> that *"the one thing a homecoming must not do is quietly destroy them"* — and for the whole life of
> the shipping slice it covered food and materials only, because fodder could not be aboard. The
> moment hay became loadable that comment was a promise the code did not keep, and a recalled party's
> hay was **destroyed** rather than returned. The delivery path had the same hole, one degree less
> bad: hay handed over simply never arrived. Both are fixed and both carry a regression test that
> asserts the band's `FODDER` balance across the round trip.
>
> **THE GUARANTEE IS PER-ACCOUNT, NOT GENERAL — which is the part that will bite again.** Nothing in
> `fold_party_into_band` iterates the party's store; each account is moved by a hand-written line, so
> an account with no line is not *dropped*, it is **destroyed silently**, with no test failing and no
> event saying anything. A cargo account therefore has **three** sites, not one: the load site, the
> delivery settle, and the fold-back settle. Miss the third and the bug is invisible until a player
> recalls a loaded party.
>
> **A new cargo account is not done when it can be loaded; it is done when it can come home.**

### Cargo is a SEPARATE store on the party

`Expedition::cargo: LocalStore`, never `cohort.stores`. The party eats out of its pack every turn
(below), so a shipment parked there would be quietly eaten by the people hauling it, arriving short
with nothing to notice.

- **Carry cap** = `party_workers × trade.per_worker_carry`, where a shipment's mass is
  `food + trade.fodder_carry_weight × fodder + trade.material_carry_weight × Σ material amounts`.
  All three are config levers; none is a literal. See "Cargo is food, FODDER and materials" below
  for where the fodder weight's number comes from.
- **Materials are peeled batch by batch** — `LocalStore::take_material_batches`, which walks the
  store's own band-key order and splits only the last batch. **A split is not a merge**: an amount is
  a quantity of one identical material, so each draw carries its source batch's readings verbatim and
  two ratings of one material leave as two batches and arrive as two batches. It is deliberately
  *not* `take_material`, which sorts worst-first on a named **axis** — that is the crafting bench's
  question, and a trader says *"four hide"*, not *"four hide by suppleness"*.
- **It rides the checkpoint whole**, the path `pending_contacts` took: `capture_sim_state` clones the
  entire `Expedition` into `ExpeditionRecord` and restore clones it back. In-flight cargo is real
  state — the goods have already left the sender's store — so a rollback that zeroed it would destroy
  them.
- **An undeliverable shipment comes home in it.** `fold_party_into_band` settles the cargo beside the
  party's own pack and returns a `FoldBack { food, materials }`, so the feed line and the food ledger
  cannot disagree about one arrival.

### The phases are the ones that already exist

`Outbound` → (arrive, deposit) → `Returning` → fold back. **No new `ExpeditionPhase`**: the party does
exactly two things and both already have a phase.

- **Retargets the destination's LIVE tile every turn**, mirroring the `Hunting` arm's herd retarget —
  bands are nomadic, and a shipment aimed once at where a people were camped arrives nowhere.
- **Arrival is the comm-range proximity** the hunt drop-off already uses (*"near enough to hand things
  over"*), not exact co-location, so a chase between two moving bands converges.
- **A destination that cannot be resolved turns the party for home CARRYING THE CARGO**, the twin of
  the lost-herd guard. Its feed line rides `CommandEventKind::ExpeditionRecalled` — the kind that
  means *"this party has been turned for home"*, the same state change the recall verb makes —
  because `TradeDelivered` would be a lie about a shipment that has not been delivered. Its detail
  carries `destination=<id>` like the launch and delivery lines: the label names the band through
  `destination_display()`'s `band <id>` fallback, and that token is the only key the client has to
  swap in its own roster label (`EventDockPanel::_swap_band_label`). Without it this one row of a
  shipment's life prints a raw id beside siblings that print the band's name.
- **One-way in this slice.** The party walks home empty; a priced return flow is a later slice, not an
  omission here.

### A trade party is provisioned like a SCOUT, and that is where the trip's cost lives

It takes the scout provisions arm **whole** rather than a trade-shaped copy: a launch draw of
`party × distance × provision_draw_per_worker_per_tile`, `party × provision_upkeep_per_worker` per
turn, and the same opportunistic replenish off passing game. It is a walking party carrying no
quarry, which is the same two facts about a scout.

**So there is deliberately no friction or loss lever on the `trade` block.** A farther destination
already costs more, in food, and a percentage-lost-per-tile dial on top would price distance twice —
once as something the player can provision for and once as goods vanishing for no stated reason.

### Fails closed, on every axis

Empty cargo, cargo the band does not hold, cargo over the carry cap, an unknown material id, a
commodity key that is not the larder's, a destination that is not a resident band, and a destination
with no tie are each a **command failure with a reason** — never a clamp and never a silently
trimmed manifest. Every check runs before anything is drawn, so a refused shipment leaves the band
exactly as it stood (asserted, not assumed).

The band half of outfitting — the resident-band gate, the party bound, the cohort template — is
`server::outfit_detached_party`, extracted out of `outfit_raiding_party` so a **fourth** verb could
not acquire its own copy of them; the spawn is `launch_party_from_band`, which the raiding verbs now
reach through a thin wrapper. `sim_runtime::FOOD_CARGO_KEY` restates this crate's `FOOD` because
`sim_runtime` does not depend on the sim, and **the server does not trust it**: a non-material line
whose id is not the larder's key is refused, so a drift fails loudly rather than shipping the wrong
good.

### The wire

`expeditionMission` gains `"trade"`, and `PopulationCohortState` gains four appended fields on both
`WorldSnapshot` and `WorldDelta` (one `PopulationSection` serves both):
`expeditionDestinationBand` (the key every command addresses the destination by, never rendered) /
`expeditionDestinationName` (its display twin, on exactly the `expeditionTargetHerd` /
`expeditionTargetSpecies` rule — the party outlives its target's presence in the viewer's world, so a
name resolvable only at launch has to be *carried*) / `expeditionCargoFood` /
`expeditionCargoMaterials`, which **reuses `MaterialPayoff`** rather than minting a second table and
carries the same three contracts as every material readout in this arc: never summed, empty is *"no
row"* not zero, key always present.

> #### `expeditionDestinationName` IS EMPTY, because bands have no names in this game
>
> **Empty means "no name", not "unknown"** — the same *"empty is no row, never a zero"* contract the
> material rows beside it use. The sim declines to guess, and a client renders whatever it already
> calls that band (its own positional label, "Band 2"), joined on `expeditionDestinationBand`.
>
> It first shipped filled from `starting_unit_label` → **`StartingUnit.kind`**, which is the unit
> *archetype* — `"BandForager"` for every seeded band. So an in-flight party's row read *"Bound for
> BandForager"*, for every destination in the game, **and disagreed with the label the rest of the
> HUD gives that same band**. A wrong name is worse than none: none has a fallback, and a
> plausible-looking one does not.
>
> **The field stays, and it is not cosmetic.** When a second faction lands (#513) a foreign band's
> name has to come from the sim — the client holds no roster to resolve one from. Filling it means
> designing a band naming scheme, which is its own piece of work and not a field default.
>
> **`ExpeditionMission::destination_name` is what crosses the wire; `destination_display` is not.**
> The display form falls back to `band <id>` so the sim's own event feed always has something to
> print, and with no names that id tier is the *normal* path rather than an edge case. It is
> deliberately never published: an id-shaped string on the wire would fight the label the client
> already has. Every feed line carries `destination=<id>` in its `detail`, which is the key a client
> needs to substitute its own label.

`CommandEventKind::TradeDelivered` (`trade_delivered`) is the landing beat — its own kind, because it
is the one expedition event that happens where *other people* live.

**The pack is FOUR fields, because the player asks about it twice and the mass rule takes three
terms.**

| field | answers | shape |
|---|---|---|
| `expeditionTradePerWorkerCarry` | *"how big a shipment can I send?"* — **before** there is a party | `expedition_config.trade.per_worker_carry`, echoed onto **every** cohort |
| `expeditionTradeFodderCarryWeight` | *"what does a unit of hay cost me in pack space?"* | `expedition_config.trade.fodder_carry_weight`, same every-cohort echo |
| `expeditionTradeMaterialCarryWeight` | *"what does a unit of hide cost me in pack space?"* | `expedition_config.trade.material_carry_weight`, same every-cohort echo |
| `expeditionCarryCap` | *"how full is this party?"* — a party already on the map | `party_workers ×` the per-worker carry of the pack **its mission** fills |

The three levers are the sim's own mass expression, and the client holds it verbatim:

```text
mass = expeditionCargoFood
     + expeditionTradeFodderCarryWeight   × expeditionCargoFodder
     + expeditionTradeMaterialCarryWeight × Σ material amounts
cap  = party_workers × expeditionTradePerWorkerCarry
```

They have to be *global* echoes rather than per-party fields: the outfit UI prices a manifest for a
party that does not exist yet, and `party_workers` is the number the stepper is *choosing*. Same
idiom as `expeditionPerWorkerCarry` / `huntPerWorkerProvisions` / `expeditionForecastHorizonTurns`.

> **THE MASS LEVER SHIPS BECAUSE THE SIM MUST NOT REFUSE ON A RULE THE CLIENT CANNOT EVALUATE.**
>
> It was first withheld on the reasoning that `material_carry_weight` is a v1 simplification — every
> material weighs the same per unit until the materials arc gives mass a density axis — so a client
> encoding it would encode an assumption rather than a rule. **That is true and it does not decide
> the question**: `per_worker_carry` is no less provisional, and every lever this subsystem echoes is
> a tuning that can move.
>
> What decides it is *"build it, send it, render the refusal"* — which makes the cargo picker a
> guessing game. The player adds hide rows one at a time against a cap meter that cannot move and
> finds out on submit. **A refusal tells the player what went wrong after they got it wrong; a live
> meter stops them getting it wrong.** When the lever gains a real model it changes, the client's
> expression changes with it, and both move in the same PR — the ordinary cost of a client-side
> readout, not a new hazard.
>
> **The server-side refusal is unchanged and remains the authority.** The meter is a courtesy that
> keeps the player from ever meeting it.

**The levers carry different wire bounds, deliberately.** `per_worker_carry` is asserted
**positive** for the horizon's reason — a `0` lets a client render a zero cap and refuse every
manifest a player could build. `material_carry_weight` **and `fodder_carry_weight`** are asserted
only **finite and `>= 0`**, because `0` is a legitimate setting on a *goods* weight (*"materials are
weightless"*) and asserting positivity would pin a tuning as if it were a rule.

**`expeditionCarryCap` resolves per mission**, and that is what stops a client reaching for the hunt
lever: a raid's pack is the provisions ceiling it fills before delivering, a shipment's is what its
people can carry out, and they are different numbers on different levers. `0` stays a scout's and a
resident band's answer. Pinned by
`trade_expedition::{every_cohort_publishes_the_shipment_mass_levers_on_the_wire,
a_trade_partys_carry_cap_is_quoted_at_the_shipment_lever}` — the first composes a real shipment's
mass out of nothing but wire fields and checks it against the published cap, the second asserts the
cap is the trade lever's product **and not** the hunt lever's, after first asserting the two levers
differ so "quoted at the right one" is falsifiable.

### The food ledger gained two terms, and one of the holes was pre-existing

A shipment moves food between larders through neither `foodIncome` nor `foodConsumption`, so
`PopulationCohortState.transferReceived` / `transferSent` were added to close it — and the **same**
pair closes `balance_supply_networks`, which had been moving food between larders untracked since
turn one. The full argument, the identity and the reset window live in
`.claude/rules/core_sim/campaign.md` → the transfer callout; what belongs here is which expedition
seams write it: the launch draw (cargo **and** the walk's larder, for both this verb and the scout's),
the shipment's arrival at the destination, the hunt drop-off, and every fold-back including the
in-camp cancel.
