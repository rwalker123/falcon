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
| `src/data/expedition_config.json` | Expedition tuning. Scout: `max_party_size`, `comm_range_tiles` (discovery-report range), `comm_range_tech_factor` (stubbed 1.0 tech hook), `observe_sight_range` (per-turn LOS radius, matches band base sight), `provision_draw_per_worker_per_tile` (launch larder draw = party × distance × this), `provision_upkeep_per_worker` (per-turn drain = party × this, scouts only). Hunt (PR 2) `hunt` block: `per_worker_carry` (carry cap = party × this), `reach_tiles` (how close to the herd to take), `drop_off_within_tiles` (herd-near-band delivery gate), `min_deliver_fraction` (herd-near-band early delivery needs carried ≥ this × cap), `viability_warn_turns` (**20** — a client display threshold on `turnsToFill`; = 4× the throughput-implied trip length `per_worker_carry / (per_worker_biomass_capacity × provisions_per_biomass)` = 5 turns), `forecast_horizon_turns` (**60** — how far `hunt_trip_forecast` simulates the raid before giving up on completion; a raid is short — grab the surplus, come home — so simulating each to completion is cheap). The retired `sustain_floor_fraction` is **gone**: a hunting expedition is a **greedy raid** — it grabs the herd's standing surplus above the mission's **floor**. See "Scouting & Hunting Expeditions". The floor is **not** a config lever — it is chosen at launch via the optional trailing arg of `send_hunt_expedition` (any fraction of `K` in `0.0..=1.0`; default `DEFAULT_ESCAPEMENT_FLOOR`, the food peak). Scout replenish `replenish` block: `low_turns` (top up below party × upkeep × this), `reach_tiles`. Loader `expedition_config.rs`, env override `EXPEDITION_CONFIG_PATH`. Not on the `reload_config` hot-reload path (mirrors `sites_config.json`). **Validated** — `ExpeditionConfig::validate()` runs inside `from_json_str`, so *every* load path (builtin, default file, `EXPEDITION_CONFIG_PATH` override) is covered, following the `crisis_config.rs` convention; a broken invariant is logged at **error** level (`expedition_config.invalid_rejected`) and the config is refused, falling back to the known-good builtin rather than silently disabling a feature. Enforced: `max_party_size ≥ 1`, `comm_range_tech_factor` finite & `> 0`, `observe_sight_range ≥ 1`, `provision_draw_per_worker_per_tile`/`provision_upkeep_per_worker` finite & `≥ 0`, `hunt.per_worker_carry` finite & `> 0`, `hunt.reach_tiles ≥ 1`, `0 < hunt.min_deliver_fraction ≤ 1`, `hunt.viability_warn_turns ≥ 1`, **`hunt.forecast_horizon_turns ≥ max(1, hunt.viability_warn_turns)`** (at `0` the forecast's `1..=horizon` loop runs zero turns and *every* hunting expedition silently reports "won't fill"; below the warn threshold, a trip the player would be told is viable can never be discovered), `replenish.low_turns ≥ 1`, `replenish.reach_tiles ≥ 1`. Deliberately **left free**: `comm_range_tiles` (`0` = "walk back into camp to report"), `hunt.drop_off_within_tiles` (`0` = no early drop-off; a full pack still delivers), and the *upper* end of `max_party_size`/`forecast_horizon_turns` (they only cost snapshot time — the estimate table is `O(policies × max_party_size × horizon)` per herd — an operator's call, not an invariant) |
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
settlement-arc systems. (A future breakaway-to-new-band is an expedition that drops `Expedition` and
gains `ResidentBand`.)

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
(refresh `BandTravel`) and, once within comm range, fold workers + leftover provisions back into the
band + despawn (`ExpeditionReturned`, after the flush so the final findings report); `AwaitingOrders`
waits.

**Hunt verb (PR 2)** — `ExpeditionMission::Hunt { fauna_id, floor: f32, fill_target: u32 }` on the
same party; **both numbers are chosen at launch** (`send_hunt_expedition <faction> <band>
<party_workers> <fauna_id> [floor] [fill_target]`) and neither is a config lever. The floor is a
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
> #### The FILL TARGET is the party-side twin of the floor (`docs/plan_hunt_through_combat.md` §5.2)
>
> **The floor says how deep to draw the herd; the fill target says how long you will wait.** It is a
> count of **whole animals** on `ExpeditionMission::Hunt`, and [`components::NO_FILL_TARGET`] (`0`)
> means *"fill the pack"* — the pre-target behaviour, and what the launch command uses when the player
> names none. Any count is legal, so unlike the floor there is **no validation and no rejection path**:
> a target at or above what the pack holds is simply the untargeted raid.
>
> **It exists because a raid's length was otherwise a species constant with no lever** (§5.1). The pack
> is measured in **carry** and the take, since the engagement stage, in **reach** — so
> `turns_to_fill = per_worker_carry / (engage_rate × body_mass × provisions_per_biomass)` and **party
> size cancels out entirely**. Eight hunters after Wild Fowl reported *"away ≈43 turns — 31 hunting, 12
> travel"*; four hunters take 31 turns and sixteen take 31 turns. §4.6's ceiling table was silently
> also the trip-length table, and the small end of it was not merely unattractive but unusable.
>
> **It adds NO termination logic — it replaces the pack's capacity in a condition the raid already
> evaluates.** `systems::expeditions::raid_load` is the one seam: it takes the party's pack cap
> (`workers × hunt.per_worker_carry`) and the target converted **once**, through the species' own
> `HuntYield::apply(fill_target × body_mass)` — the same call the completion's `food_per_animal` uses,
> so `fill_target` animals of room and the target cap are the same number by construction — and returns
> the smaller of the two **plus which one bound**. The live `Hunting` arm and `hunt_trip_forecast` both
> resolve it, so the raid cannot come home on a load different from the one it was quoted.
>
> **Wherever the pack is ignored, so is the target**, because it is the same stop under a player's
> name. Two cases, and they are different kinds of fact: an **INEDIBLE** quarry (a *product* fact —
> `provisions_per_biomass == 0`, so a converted target would read as an instantly-full pack, and the
> food pack is already inert there) and **`STRIP_IT_BARE`** (an *intensity* fact — a floor-`0` raid
> never consults the pack at all, `done`/`relaunch` both `false`). **Denial** — the mission with the
> carry bound removed (`docs/plan_denial_raid.md`) — is where a target on those will need a meaning of
> its own.
>
> Pinned by `expedition_hunt::{a_fill_target_below_capacity_shortens_the_trip_and_above_it_is_an_identity,
> trip_length_responds_to_the_fill_target_but_not_to_party_size_without_one}` — the shortening, the
> exact identity at/above capacity, and, deliberately beside them, the party-size invariance that
> *caused* the defect and is still true.

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
>   (`fauna::animals_engaged`, at the identity build dip — a detached party builds nothing) and the
>   quarry's retreat (`fauna::animals_that_stay`, under the caller's `fauna::HuntDraw` — a per-event
>   seed live, a quantile in a forecast) and hands the count to **the**
>   quantiser, which also retired its hand-rolled copy of the `max(1, carryable)` arithmetic. The bound
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
>   remainder** — exactly the resident band's `max(1, carryable)` rule (`fauna::quantise_animal_take`): a
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
  end state, not an empty pack).
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
  model the near-band drop-off**, and that is structural, not an omission: the `huntTripEstimates` table
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
  - **`delivered_trade`** — the trade half of the same carried biomass, the whole payload of an
    **inedible** raid. **The forecast simulates an inedible quarry like any other** (#337): it used to
    short-circuit to an all-zero projection on the premise "a wolf trip is not a food trip", which also
    zeroed `animals_taken` and `delivered_trade` — the client quoted `⇄ ~0` on a wolf while the sim
    banked real pelts. Only an **empty party** (`cap <= 0`) short-circuits now; a wolf raid gets a real
    ETA (it ends when the standing surplus is spent) and its food fields fall out at `0` on their own.
  - **`bound`** (`HuntTripBound`) — **WHICH stop ended the trip**: `PackFull` / `FillTarget` / `Floor`
    / `HerdLost` / `Horizon`, wire keys `"pack_full"` / `"fill_target"` / `"floor"` / `"herd_lost"` /
    `"horizon"`. A trip *length* alone cannot tell the player's two levers apart — *"you come home on
    your fill target in 4 turns; the herd never reaches the floor"* and *"you reach the floor in 2
    turns with the pack a third full"* are different decisions carrying the same kind of number — so
    the sim names it and the client composes nothing. `Horizon` is exactly the `turns_to_fill == None`
    case except for `HerdLost`, which also reports no completion turn: the projection stops where
    `advance_herds` would despawn the herd rather than claim a homecoming turn. **A tie goes to the
    herd side** (`Floor` over the party-side stop), mirroring the live arm testing `done` before
    `relaunch`. Pinned against the raid the systems actually run by
    `expedition_hunt::the_exported_bound_names_the_stop_that_ends_the_raid`.
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
  tile is known. The per-herd `huntTripEstimates` table is **band-agnostic** (one row per herd serves
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
  had rotted (the gate silently accepted `tame`, which then took a plausible pastoral-dip ceiling).
  Guarded by `server::tests::send_hunt_expedition_rejects_a_floor_outside_the_dial`.
- **Shared take helpers** (`fauna.rs`): **`hunt_escapement_ceiling(floor, biomass, carrying_capacity)`**
  is THE take ceiling on the animal web — `max(0, B − floor·K)`, the stock standing above the
  assignment's or mission's floor — and `quantise_animal_take` rounds it to whole animals. It takes
  **no ecology, no `FaunaConfig`, no `improvement` and no ladder**, which is what makes the take
  `r`-independent structurally rather than by convention; see "The hunt policy axis" in `fauna.md`.
  **The build dip is NOT a term here** — it multiplies the crew, so this signature has nowhere to put
  it (`docs/plan_harvest_floor.md` §3.1). The `improvement`/`ladder` parameters this file used to list
  are exactly what slice 3 removed.
  The expedition keeps its own `credit` accumulator for the *party's* processing throughput
  (`expedition_take_biomass`), which is a different quantity from the retired resident bank.
  **`HuntYield::apply(take, output_multiplier)`** (via `FaunaConfig::hunt_yield_for`, which retired the global `hunt_provisions`) is the single per-species biomass→(food, trade) conversion (an
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
  - **Where each product lands — ONE store, both products.** Provisions go into the party's pack
    (`stores[FOOD]`, carry-capped) and fold into the home band's larder. Trade goods accrue
    **fractionally** on `Expedition::carried_trade` and settle into that **same home band's**
    `stores[TRADE_GOODS]` at the next arrival — a `Delivering` drop-off or a `Returning` fold-back
    (`systems::expeditions::settle_carried_trade`). Trade goods are band-local like grain
    (`yield-forecast.md` → "Trade goods are a BAND-LOCAL store"), so a haul that arrives with no home
    band left to receive it is simply lost, exactly as the carried food is. Both scale off the biomass
    the party **carried**, never what it killed.
  - **Why banked rather than paid per kill**: a raid's promised `HuntTripForecast::delivered_trade` is
    a sum over the **whole trip**, and the pack has to physically reach the band before anyone can hold
    what is in it. **Nothing rounds at either end** — the band store is fixed-point — so the exact
    carried fraction lands and `forecast == actual` holds without a remainder being dropped per trip.
    (The settle used to `round()` to whole goods because `FactionInventory` is an `i64` account; the
    feed line still prints the haul, now to 2 dp, and "returning EMPTY" is a claim about the raw
    `carried_trade` so a sub-unit pack of pelts is never called empty.)
    `carried_trade` is snapshot-persisted (`PopulationCohortState.expedition_carried_trade`,
    persistence-only, not on the FlatBuffers wire) so a rollback does not silently drop the pelts while
    restoring the meat.
  - **The scout's opportunistic replenish banks its hides too** — a roadside kill is skinned as well
    as butchered — so it is no longer a pure waste of animals on an inedible herd.
  - **Still expedition-side gaps:** no **husbandry/domestication accrual** (a Sustain *expedition*
    builds no domestication — that is place-bound work a resident band does), and what trade goods
    ultimately *do* stays economically thin (the deferred half of issue #213 /
    `docs/plan_hunt_yield_model.md` "Deferred"). Catching a *migratory* herd depends on the deferred
    fauna-movement redesign (herds step 1 tile/turn today, so an equal-speed party can't close a long
    one-directional route).

**Commands** (full proto/runtime/text/server plumbing, mirroring `move_band`):
- `send_expedition <faction> <band> <party_workers> <x> <y>` — validates land target + `1 ≤
  party_workers ≤ min(available_workers, max_party_size)`, draws `party × distance ×
  provision_draw_per_worker_per_tile` provisions from the band larder (partial OK), removes the
  workers from `band.working`, and spawns the detached `Expedition` cohort. Feed `ExpeditionSent`.
- `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> [floor] [fill_target]` — same
  resident-band gate + party validation, validates `fauna_id` resolves to a live herd, draws **no**
  provisions, removes the workers, spawns a `Hunt`-mission party in `Hunting` phase heading for the
  herd. Feed `ExpeditionSent` (hunt flavor), whose detail carries `floor=… fill_target=… bound=…`.
  The two optional tokens are **positional in that order** (proto fields `floor = 6`,
  `fill_target = 7`) — the floor shipped first, and a positional grammar is append-only for the same
  reason a wire is. The **floor fails closed** (out of `0.0..=1.0` → command failure, never clamped);
  the **fill target cannot fail** — every count is a legal order, and a target at or above the pack's
  capacity is simply the untargeted raid.
- `recall_expedition <faction> <expedition_band_id>` — resolves the entity via
  `resolve_expedition_entity` (checks the `Expedition` component + faction), sets `phase = Returning`
  (works for both verbs). Feed `ExpeditionRecalled`.
- **Retargeting a scout waypoint is just `move_band` on the expedition entity** — `handle_move_band`
  has a hook that re-arms a moved expedition to `Outbound` + `announced = false`.
- New `CommandEventKind` variants: `ExpeditionSent`, `ExpeditionArrived`, `ExpeditionRecalled`,
  `ExpeditionReturned` (in `as_str` + the server label map); the hunt drop-off / lost-herd feed lines
  reuse `Hunt`.

**Snapshot.** `PopulationCohortState` gains client discriminators `isExpedition` / `expeditionMission`
(`"scout"`|`"hunt"`) / `expeditionPhase` (`outbound`|`awaiting`|`returning`|`hunting`|`delivering`) /
`expeditionTargetHerd` (hunt fauna_id — a **string**, since herd ids are non-numeric) /
**`expeditionFloor:float`** (the raid's escapement floor as a fraction of `K` — the live
discriminator, defaulting to `1` so an absent floor reads "take nothing" rather than "take
everything"; `expeditionHuntPolicy` is the retired `(deprecated)` slot it replaced and has no
accessor) / **`expeditionFillTarget:uint`** (the raid's fill target in **whole animals** — the
party-side twin of the floor; `0` = [`NO_FILL_TARGET`], "fill the pack", which is what a scout and a
resident band report) / **`expeditionTripBound:string`** (which stop will end *this* party's raid —
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
running after a rollback. `PopulationCohortState` also echoes `maxExpeditionPartySize` per cohort
(from `expedition_config.max_party_size`, same idiom as `workRange` — a global lever surfaced
per-band, populated for every cohort) so the client outfit stepper pre-clamps to
`min(idle_workers, max_expedition_party_size)`.

**In-flight next-delivery forecast — the twin of the pre-launch estimate, for a party already on the
map** (`systems::expeditions::expedition_delivery`). The pre-launch `huntTripEstimates` answer "if I
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
player commits workers, as they pick party size / herd / policy. The expedition's trip length is **not
a formula** (see the forecast above: a small-herd Surplus party exhausts *stock*, so no per-turn rate
describes the trip), so the sim exports the **answer** it simulated, and the client's job is a **table
lookup**:
- `HerdTelemetryState.huntTripEstimates:[HuntTripEstimate{ floor:float, partyWorkers:uint,
  turnsToFill:uint, deliversFood:bool, animalsTaken:uint, deliveredFood:float, wastedFood:float }]` —
  per **huntable** herd, one entry per **sampled floor** (`snapshot::RAID_FORECAST_FLOOR_SAMPLES`,
  **5 samples** `[0.0, 0.15, 0.30, 0.50, 0.80]` — marks on a continuum, NOT a set of options: the
  launch command takes any floor) × every legal party size
  (`1..=expedition_config.max_party_size`, so **5 × 8 = 40 rows/herd**). The row's `policy:string` is a
  retired `(deprecated)` slot; the live discriminator is `floor:float`, so the client interpolates
  between marks rather than matching a name. **An improvement is not a floor** (issue #442), so a
  build-verb row is unrepresentable rather than merely omitted. **`turnsToFill`** is turns until the raid **completes** (comes home — pack full OR
  surplus spent), **`0` = never completed** within `hunt.forecast_horizon_turns`. **`animalsTaken`**
  (append-only) is now a **KILL count** — a party too small to seat a whole animal kills one and wastes
  the rest (like the resident band), so the delivered payload is **`deliveredFood`** (`Σ
  HuntYield::apply(carried)`, appended strictly after `animalsTaken`), NOT `animalsTaken × foodPerAnimal`.
  **`wastedFood`** (`Σ HuntYield::apply(wasted)`, appended) gives the waste fraction `wastedFood /
  (deliveredFood + wastedFood)`. **"Too lean to raid" is `deliveredFood == 0`** (no surplus at any party
  size); a herd at/below its floor reads `0` on all three. Because the take is bounded by the standing
  surplus, `deliveredFood`/`animalsTaken` **plateau** with `partyWorkers` once the surplus binds — that
  plateau is the max-useful party size (`ceil(surplus_food / per_worker_carry)`) the stepper caps at.
  **`bound:string`** (appended) names WHICH stop ended the sampled trip — the `HuntTripBound` key.
  **Every row here is the UNTARGETED raid**, so no row reads `"fill_target"`: a fill target is chosen
  at launch and this table is band-agnostic, sampling floor × party size only. A launched party's own
  bound is `PopulationCohortState.expeditionTripBound`.
  `deliversFood == false` means the **species** is inedible (a wolf), not that the policy denies — such
  a row still carries a real `turnsToFill` and a `deliveredTrade` payload. **Travel is excluded** — the
  number means "turns spent hunting once you arrive".
- `HerdTelemetryState.{provisionsPerBiomass, fodderPerBiomass, tradePerBiomass}` — the **BAND /
  local-hunt** terms, from which the client composes the ceiling at **any** floor:
  `max(0, B − floor·K) × rate`. **UNDIPPED** — `<rung>BuildFraction` belongs to the CREW term, not to
  this, and a client that folds it in here discounts a build twice (the shipped GDScript composes
  ceilings undipped; see `labor-ui.md`). `huntPolicyCeilings` is a retired
  `(deprecated)` slot: four rows cannot answer a continuous dial (`yield-forecast.md` → "the sim
  exports the answer" and its one narrow exception). A herd below a floor composes `0` for it, which
  is the escapement rule rather than a special case. The dip still ships as
  `tameBuildFraction` / `corralBuildFraction` (see "Pre-commit Yield Forecast"). **Formerly sourced by
  projecting the herd's `fauna::hunt_forecast`** (`SourceYieldForecast::ceiling_for`) —
  the **only** wire representation of a herd's per-policy ceilings (the scalar
  `ceilingSustain`/…/`ceilingCorral` twins, which carried literally the same numbers, are now retired
  `(deprecated)` slots), and the take path pays exactly them
  (forecast == actual). That also makes `Corral` **phase-correct for free**: the
  the `animal:pen` rung's `yield_fraction_while_building × MSY` dip while the pen is being built, and the **full corral yield**
  once `is_corralled()` (a penned herd forecasts as `SourceYieldForecast::tended` — every ceiling is
  its managed yield, one keeper suffices). There is **no expedition ceiling field** — the retired
  `expeditionProvisionsPerTurn` was exactly the "one number that means a flow for Sustain and a stock
  for Surplus/Deplete" design smell the estimate table replaces.
- `PopulationCohortState.huntPerWorkerProvisions:float` (one hunter's
  provisions/turn throughput = `labor_config.hunt.per_worker_biomass_capacity ×
  fauna_config.hunt.provisions_per_biomass`) and `.expeditionViabilityWarnTurns:uint`
  (`expedition_config.hunt.viability_warn_turns` — the NOT-VIABLE threshold the client applies to
  `turnsToFill`) — global levers echoed onto **every** cohort (the `maxExpeditionPartySize` idiom; the
  outfit UI lives on the resident-band panel).

**The two hunt readouts, and what each reads:**
- **Expedition (pre-launch raid)** — a lookup: `huntTripEstimates[(policy, partyWorkers)]` →
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
`huntTripEstimates` row against the two accounts a real driven raid actually credits — the home band's
own store, `FOOD` for provisions and `TRADE_GOODS` for pelts — over an edible × an inedible species ×
Sustain/Surplus/Deplete, and `expedition_hunt::a_far_just_launched_party_projects_the_estimate_delivery`
pins the in-flight projection to the exported row for the same `(policy, party size)`. For the **band**
readout, `expedition_hunt::exported_snapshot_fields_reproduce_band_hunt_take` does the same against
`hunt_take(..)` (healthy / clamp-binding depleted / collapsing herd × every worker count × all four
policies × a unit and a discontent-reduced output multiplier). If either readout ever drifts from the
sim, those tests fail.

See Also: `docs/plan_exploration_and_sites.md` §2 (design), "Wondrous Sites" (discovery rides the
flushed tiles), "Visibility Systems" (the `Without<Expedition>` gate).

---

