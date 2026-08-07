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
| `src/data/expedition_config.json` | Expedition tuning. Scout: **`estimate_party_sizes`** (**an ascending LADDER of sampled party sizes, never a cap on a party** — shipped `[1, 2, 3, 4, 8, 16, 32, 64]`. It is the party axis of `huntTripEstimates` and the base of the denial table's own axis, and it is *marks on a dial* exactly as `RAID_FORECAST_FLOOR_SAMPLES` is: dense at the low end where one hunter is a large proportional change, sparse at the top where it is not. Its **last rung is the only quoting bound there is** — it absorbed the retired `deny.max_party_quoted`. It is the renamed `max_party_size`, which was doing two jobs under one name; the *rules cap* half is deleted and **every** launch verb now bounds a party by the band's own `available_workers` — see "A raiding party is bounded by the BAND" and "The party axis is SAMPLED"), `comm_range_tiles` (discovery-report range), `comm_range_tech_factor` (stubbed 1.0 tech hook), `observe_sight_range` (per-turn LOS radius, matches band base sight), `provision_draw_per_worker_per_tile` (launch larder draw = party × distance × this), `provision_upkeep_per_worker` (per-turn drain = party × this, scouts only). Hunt (PR 2) `hunt` block: `per_worker_carry` (carry cap = party × this), `reach_tiles` (how close to the herd to take), `drop_off_within_tiles` (herd-near-band delivery gate), `min_deliver_fraction` (herd-near-band early delivery needs carried ≥ this × cap), `viability_warn_turns` (**20** — a client display threshold on `turnsToFill`; = 4× the throughput-implied trip length `per_worker_carry / (per_worker_biomass_capacity × provisions_per_biomass)` = 5 turns), `forecast_horizon_turns` (**60** — how far `hunt_trip_forecast` simulates the raid before giving up on completion; a raid is short — grab the surplus, come home — so simulating each to completion is cheap). The retired `sustain_floor_fraction` is **gone**: a hunting expedition is a **greedy raid** — it grabs the herd's standing surplus above the mission's **floor**. See "Scouting & Hunting Expeditions". The floor is **not** a config lever — it is chosen at launch via the optional trailing arg of `send_hunt_expedition` (any fraction of `K` in `0.0..=1.0`; default `DEFAULT_ESCAPEMENT_FLOOR`, the food peak). Denial `deny` block: **`requirement_rows`** (**5** — how many *contiguous* rows the denial table samples starting at the herd's own closed-form requirement, on top of the shared ladder. It exists because `denialPartyNeeded` is read off the **forward simulation**, which lands at the requirement or a little above it, so a sparse ladder alone would round the sheet's opening party up to its next rung. The retired **`max_party_quoted`** is gone: the ladder's last rung already names the quoting bound, and two numbers for one bound can disagree. A requirement past that rung contributes no rows and the herd reports **no viable party** (`denialPartyNeeded == 0`) — the sim will not quote a raid it declines to simulate; the shipped roster's worst case is ~32–35 hunters, comfortably inside 64). Scout replenish `replenish` block: `low_turns` (top up below party × upkeep × this), `reach_tiles`. Loader `expedition_config.rs`, env override `EXPEDITION_CONFIG_PATH`. Not on the `reload_config` hot-reload path (mirrors `sites_config.json`). **Validated** — `ExpeditionConfig::validate()` runs inside `from_json_str`, so *every* load path (builtin, default file, `EXPEDITION_CONFIG_PATH` override) is covered, following the `crisis_config.rs` convention; a broken invariant is logged at **error** level (`expedition_config.invalid_rejected`) and the config is refused, falling back to the known-good builtin rather than silently disabling a feature. Enforced: **`estimate_party_sizes` is non-empty, starts at `1`, and ascends strictly** (empty → every herd publishes empty estimate tables and no launch sheet can quote anything; missing `1` → a lone hunter is answered with a row computed for several; unsorted → "nearest rung" is undefined; repeated → the same forward simulation runs twice per herd per snapshot and two rows carry one party size — all four are authorable in the file and all four fail silently), **`deny.requirement_rows ≥ 1`** (at `0` the herd's own requirement is never sampled, so `denialPartyNeeded` is rounded up to whichever ladder rung sits above it), `comm_range_tech_factor` finite & `> 0`, `observe_sight_range ≥ 1`, `provision_draw_per_worker_per_tile`/`provision_upkeep_per_worker` finite & `≥ 0`, `hunt.per_worker_carry` finite & `> 0`, `hunt.reach_tiles ≥ 1`, `0 < hunt.min_deliver_fraction ≤ 1`, `hunt.viability_warn_turns ≥ 1`, **`hunt.forecast_horizon_turns ≥ max(1, hunt.viability_warn_turns)`** (at `0` the forecast's `1..=horizon` loop runs zero turns and *every* hunting expedition silently reports "won't fill"; below the warn threshold, a trip the player would be told is viable can never be discovered), `replenish.low_turns ≥ 1`, `replenish.reach_tiles ≥ 1`. Deliberately **left free**: `comm_range_tiles` (`0` = "walk back into camp to report"), `hunt.drop_off_within_tiles` (`0` = no early drop-off; a full pack still delivers), and the *length* of `estimate_party_sizes` / the upper end of `forecast_horizon_turns` (they only cost snapshot time — the hunt table is `O(floors × ladder rungs × horizon)` per herd — an operator's call, not an invariant) |
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
(refresh `BandTravel`) and, once within comm range **or the moment that band cannot be resolved at
all**, fold workers + leftover provisions back into the band + despawn (`ExpeditionReturned`, after
the flush so the final findings report — see "One fold-back, two moments"); `AwaitingOrders` waits.

**Hunt verb (PR 2)** — `ExpeditionMission::Hunt { fauna_id, floor: f32 }` on the
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
  party_workers ≤ available_workers` (the band, and nothing else — `estimate_party_sizes` is a
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
- **Retargeting a scout waypoint is just `move_band` on the expedition entity** — `handle_move_band`
  has a hook that re-arms a moved expedition to `Outbound` + `announced = false`.
- New `CommandEventKind` variants: `ExpeditionSent`, `ExpeditionArrived`, `ExpeditionRecalled`,
  `ExpeditionReturned` (in `as_str` + the server label map); the hunt drop-off / lost-herd feed lines
  reuse `Hunt`.

**Snapshot.** `PopulationCohortState` gains client discriminators `isExpedition` / `expeditionMission`
(`"scout"`|`"hunt"`|`"deny"`) / `expeditionPhase` (`outbound`|`awaiting`|`returning`|`hunting`|`delivering`) /
`expeditionTargetHerd` (hunt fauna_id — a **string**, since herd ids are non-numeric) /
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
running after a rollback. `PopulationCohortState` also echoes `maxExpeditionPartySize` per cohort
(the **last rung** of `expedition_config.estimate_party_sizes`, same idiom as `workRange` — a global
lever surfaced per-band, populated for every cohort). **It is NOT a stepper cap**: the stepper clamps
to `idle_workers` alone, and this says only where the quoted estimate rows stop. The rows below it
are a **ladder**, not a contiguous run, so a client resolves a party against the herd's own table
rather than assuming every size under this has a row. See "A raiding party is
bounded by the BAND"; the field name predates that split and is kept because renaming a wire slot
costs a client decode change for no behaviour.

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
  launch command takes any floor) × every **sampled party size** (`expedition_config
  .estimate_party_sizes`, the ladder `[1, 2, 3, 4, 8, 16, 32, 64]`, so **5 × 8 = 40 rows/herd** — the
  same budget the retired contiguous `1..=8` axis spent, now spanning eight times the range). **Both
  axes are marks on a dial**: a party between two rungs, or past the last one, reads the nearest row
  — see "The party axis is SAMPLED" and "A raiding party is bounded by the BAND". The row's `policy:string` is a
  retired `(deprecated)` slot; the live discriminator is `floor:float`, so the client interpolates
  between marks rather than matching a name. **An improvement is not a floor** (issue #442), so a
  build-verb row is unrepresentable rather than merely omitted. **`turnsToFill`** is turns until the raid **completes** (comes home — pack full OR
  surplus spent OR **the herd runs out**), **`0` = never completed** within
  `hunt.forecast_horizon_turns`, which after the `HerdLost` repair means `bound == "horizon"` and
  nothing else — so a **floor-`0`** row, whose only stop is the herd running out, now carries a real
  turn instead of the never-completes sentinel. **`animalsTaken`**
  (append-only) is now a **KILL count** — a party too small to seat a whole animal kills one and wastes
  the rest (like the resident band), so the delivered payload is **`deliveredFood`** (`Σ
  HuntYield::apply(carried)`, appended strictly after `animalsTaken`), NOT `animalsTaken × foodPerAnimal`.
  **`wastedFood`** (`Σ HuntYield::apply(wasted)`, appended) gives the waste fraction `wastedFood /
  (deliveredFood + wastedFood)`. **"Too lean to raid" is `deliveredFood == 0`** (no surplus at any party
  size); a herd at/below its floor reads `0` on all three. Because the take is bounded by the standing
  surplus, `deliveredFood`/`animalsTaken` **plateau** with `partyWorkers` once the surplus binds — that
  plateau is the max-useful party size (`ceil(surplus_food / per_worker_carry)`) the stepper caps at.
  **`bound:string`** (appended) names WHICH stop ended the sampled trip — the `HuntTripBound` key,
  one of the raid's **four** stops. Pinned by
  `expedition_hunt::every_pre_launch_estimate_row_names_one_of_the_raids_four_stops`, which holds the
  live key set in one place so a fifth cannot appear without a client clause. A launched party's own
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

## Denial is a MISSION, not a floor — and it changes ONE line

`ExpeditionMission::Deny { fauna_id }`, wire key `"deny"`, launched by
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
hunt:    killed  = min(affordable, max(1, carryable), brought_down)   // WhenPackFull
denial:  killed  = min(affordable,                    brought_down)   // Never
both:    carried = min(killed × body_mass, carry_room)                // IDENTICAL
```

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
  carry.** It grinds to extinction through the lost-herd guard, one `max(1, carryable)` animal a turn
  once its pack is full; denial drops the pack as a bound on what it **engages** and kills everything
  it brings down. **Both haul their real pack** — `carry_room_biomass` takes no floor argument and
  `NO_CARRY_BOUND` means *inedible quarry* and nothing else. A floor-`0` hunt used to pass it, so
  `carried = killed × body_mass`: the party was recorded hauling home everything it killed, its hunt
  report published `wasted_biomass = 0` for a raid that left a range of carcasses, and
  `carried_trade` accrued pelts off the whole kill — against the "both scale off what the party
  carries, never what it killed" rule two sections above. On a 4-hunter mammoth raid the exported row
  promised **16 food / 4 trade / 0 wasted** while the party banked 3.2 food and **36** trade; it now
  promises 3.2 / 0.8 / 140.8 and pays exactly that. Pinned by
  `denial_raid::{a_floor_zero_hunt_hauls_only_its_pack_and_reports_the_waste,
  denial_and_a_floor_zero_hunt_account_carry_identically}` and
  `hunt_yield_vector::a_floor_zero_raid_delivers_and_wastes_what_its_exported_row_promised`.
- **An INEDIBLE quarry is a legitimate denial target** (a wolf). Nothing on the path divides by a
  food rate it has not established positive: the pack is inert there for the same *product* reason it
  is inert on a hunt, and the raid is paid in pelts.

### A raiding party is bounded by the BAND, not by a config lever

`handle_send_expedition` and `outfit_raiding_party` bound a party by **`available_workers`** and
nothing else, on all three verbs. Authoritative design: `docs/plan_denial_raid.md` §3.1.

**The lever they used to also consult was doing two jobs under one name** (`max_party_size`), and
only one had a justification:

- **A sampling bound, which is real.** `huntTripEstimates` / `denialEstimates` hang off
  `HerdTelemetryState` — per *herd*, not per band — so the sim cannot know which band is asking or
  how many workers it has. Those tables need a fixed axis. Renamed **`estimate_party_sizes`**.
- **A rules cap on the legal party, which had none.** No design note ever backed it, and the honest
  bound is the one the band panel already displays. At `8` it refused a party of **9** from a band
  holding **16**, against a Red Deer herd at 51 of 119 head needing exactly
  `2.91 regrowth / 0.35 kills-per-hunter` = 9 — two unrelated eights, and the config one won. Deleted.

**This changes a HUNT's party sizing too, deliberately.** A hunting party is no longer capped at 8
either; that is the ruling followed to its conclusion, not an accident. Pinned by
`server::tests::a_raiding_party_is_bounded_by_the_band_and_not_by_the_sampling_lever`, which asserts
**both** verbs launch past the sampling bound *and* that both still refuse a party past the band, so
"the bound moved" cannot degrade into "the bound vanished".

**Wary herds are therefore expensive, not undeniable.** Wariness raises the requirement; nothing
caps it below what the band can field.

**A party between two rungs has no pre-computed estimate, and the client must not compose one.** The
take passes through `fauna::quantise_animal_take`'s `floor()`, so it is non-linear and ships as an
**answer** (`yield-forecast.md` → "THE BOUNDARY"). The sheet quotes the **nearest sampled row, naming
the party size it was sampled for**. `PopulationCohortState.maxExpeditionPartySize` is where the rows
stop; **its name is now wrong** and survives only because renaming a wire slot costs a client decode
change for no behaviour.

### The party axis is SAMPLED, and that is what fixed the blank sheet

**Both estimate tables used to walk their party axis contiguously from `1`, and the client's lookup
demanded an exact match.** A party past the last row therefore found nothing and *every* readout on
that sheet went silent — no verdict, no range, no take, no turn count. It was easy to reach: the
denial axis was `closed_form_requirement + 8`, while the compose sheet's stepper caps at the band's
**idle workers**, and those two numbers are unrelated. A band holding 16 idle, raiding a herd whose
requirement is 1, got rows for 1–9 and a stepper that reached 16.

**The floor axis never had that problem, and its own comment says why: the samples are marks on a
dial.** Nobody computes every floor; the client takes the nearest mark. `estimate_party_sizes` is now
the same shape — an explicit ascending list rather than a count — and the cost objection dissolves
with it, because a *sparse* ladder spans the whole dialable range in **fewer** rows than the
contiguous run spent on a ninth of it.

- **The shipped ladder is `[1, 2, 3, 4, 8, 16, 32, 64]`.** Unit steps through 4, where one hunter is
  a +100% / +50% / +33% / +25% change; doubling above it, where it is not. It ends on **64**, the
  bound the retired `deny.max_party_quoted` named, so no herd lost a quote it used to have.
- **The hunt table is the binding budget**, because its axis is `floors × parties`: eight rungs × the
  five floor samples is 40 rows/herd, exactly what `1..=8` cost. Adding a ninth rung costs five rows
  per herd, not one — trim before extending.
- **`1` is always sampled**, and validation enforces it: the client resolves by *nearest*, so a
  ladder starting at 2 would answer a lone hunter with a row computed for two.
- **The denial axis additionally samples a contiguous run at the herd's own requirement**
  (`deny.requirement_rows`, 5) — see "`denialPartyNeeded` — the party the sheet OPENS on".
- **Measured on a fresh 80×52 map (133 huntable herds, debug).** Hunt rows **5,320 → 5,320**
  (unchanged, 40/herd). Denial rows **2,354 → 1,530** (−35%; per herd 9–40 → 9–13). Timing the denial
  half over the two axes on the same herds: **39.9 ms → 26.2 ms**. Whole-capture ≈ **50 ms/frame**,
  against the ~59 ms the contiguous herd-sized axis measured at.
- **A broken ladder is a boot panic** (`config-loading.md`): empty, missing `1`, unsorted or repeated.
  All four are authorable in the file and all four fail silently at runtime.

### `denialPartyNeeded` — the party the sheet OPENS on

`HerdTelemetryState.denialPartyNeeded` (appended last) is the **smallest row in `denialEstimates`
whose outcome `succeeded`** — `past_recovery` or `herd_lost` — read off the rows rather than
recomputed, so the sheet cannot open on a value whose verdict one line below refuses to say the herd
goes down. The stepper seeds there instead of at an arbitrary default, which turns the control from a
guessing game into an adjustment.

- **The test is `DenialOutcome::succeeded`, NOT "not `repelled`"**, and the two differ on exactly one
  verdict: `horizon`, a raid the projection ran its whole length with the herd still standing. A
  `!= repelled` seed quoted a Wild Aurochs party of **5** under its own verdict line *"Wild Aurochs is
  still standing when the forecast runs out"* — a horizon row presented as the party that works, and
  in play it was short. The gap is not one row: measured over the shipped roster it runs to **21
  hunters** between the first non-repelled row and the first row that actually crosses the line (Wild
  Boar / Grey Wolf Pack at full `K`).
- **The wire `String` gets back to the enum through `DenialOutcome::from_wire`, never through a
  second list of keys at the call site.** `from_wire` searches `DenialOutcome::ALL` by `as_str`, so
  the round trip is total by construction and no key is spelled twice — which is the drift that
  produced the bug in the first place. Pinned by
  `systems::expeditions::denial_outcome_tests::every_denial_outcome_round_trips_through_its_wire_key`.

- **The requirement rounds UP, always.** `fauna::denial_party_needed(replacement, engage_rate,
  wariness)` is `floor(replacement / (engage_rate × (1 − wariness))) + 1` — **not `ceil`**, because a
  party that exactly *ties* with the regrowth declines nothing and `ceil` is wrong by one at
  precisely the round number a tuner is most likely to author. Same `floor(x) + 1` idiom, and the
  same reason, as `fauna::peak_animal_drop`. `None` (⇒ the wire's `0`) for a quarry no number of
  hunters brings into contact (`wariness >= 1`, `engage_rate <= 0`); `Some(1)` for a source with
  **no engagement stage** (`f32::INFINITY`), which is the opposite reading and must not be confused
  with it.
- **The replacement it outpaces is the PEAK on the path down**, `fauna::herd_replacement_animals` =
  `sustainable_yield(B, K) / body_mass`. Not the regrowth where the herd *stands*: the logistic
  curve peaks at `K/2`, so a full herd's instantaneous regrowth is `0` and a party sized on it would
  read *one hunter*, drive the herd to the food peak, and stall there forever. Below `K/2` the
  current stock binds instead, and the raid accelerates as it works. Reading it through
  `sustainable_yield` keeps the requirement on the **same** curve `regrow_biomass` advances the herd
  with, rather than opening a second copy of the model.
- **The closed form is a BOUND on the search, never the answer.** It is linear in the party and
  therefore blind to the whole-animal quantiser, to the fight, and to `animals_engaged`'s `max(1)`
  floor (which lets a lone hunter reach one mammoth where the arithmetic reads `0.05`). Its job is to
  size `snapshot::subsistence::denial_party_axis`; which of those rows *actually* declines the herd
  is the forward simulation's.
- **`0` = no quoted party drives this herd down**, and it is never *"send nobody"*. **Four**
  situations reach it — an unreachable quarry, a requirement past the ladder's last rung, a herd
  whose regrowth out-runs the whole table, and a quoted axis whose rows never reach a **success**
  (every party either repelled or still grinding at the horizon) — and the rows' own `outcome` says
  which. The fourth is the one the `succeeded` test added, and it is the honest reading of a sheet
  that holds no row the sim will vouch for. A requirement larger than the band's idle workers is
  **not** one of them: that is reported honestly as the number, and the panel already shows both.
- **The table's axis is the shared ladder PLUS a contiguous run of `deny.requirement_rows` (5)
  starting at the herd's own requirement**, bounded by the ladder's last rung. The run is what keeps
  the seed off a rung: the closed form is only a bound on the search, so the row that actually
  declines the herd sits at the requirement or a little above it, and a sparse ladder alone would
  round the sheet's opening party up. Everything *above* the run is the **how fast** decision
  (measured on the reported herd: 9 hunters grind past the horizon, 16 cross the line in 11 turns),
  and a sparse rung answers that perfectly well.
  The retired contiguous `1..=requirement + 8` axis spent its rows on the **expensive** end — every
  sub-requirement party is a raid that gets repelled and therefore runs the whole forecast horizon,
  and there was one per hunter. Measured on a fresh 80×52 map (133 huntable herds, debug): denial
  rows **2,354 → 1,530**, and the denial half of capture **39.9 ms → 26.2 ms** over the same herds.
  Snapshot capture is the hot half of a turn, which is why the axis is herd-sized rather than flat.
- **The axis does not guarantee a success row, because it is sized by the closed form.** Swept over
  the shipped roster (every generated herd × five stock fractions, ~670 samples per map), **0–5 rows
  per map** hold a first-success party **1–4 above** the axis, and a Thunder Mammoth herd at full `K`
  ran **9** above it (the closed form asks 4 where the simulation needs 21 — a heavy body is where
  the quantiser and the fight diverge from it hardest). Those herds report `0` rather than a party
  the sim would in fact vouch for. Widening the headroom is the lever if play says it matters; it
  costs `3 × rows × forecast_horizon_turns` turn-steps per huntable herd.

**THE WHOLE HERD TABLE IS PRICED AT THE HUNT JOB'S DEFAULT KIT, AND SO IS THIS FIELD.**
`snapshot/capture.rs` builds the `HerdSnapshotInputs::party` from
`EquipmentConfig::default_kit(KitJob::Hunt)` over a fresh set of components — bit-identical to the
hardcoded `equipped = true` it replaced, because the shipped `big_game` default masks in exactly the
two hunt components. A herd row is a fact about the *herd* and the table has no band to ask.
**Which kit it is quoted at is now PUBLISHED** (`huntTripEstimatesKitId` / `denialEstimatesKitId`),
so a client whose player has selected another kit can refuse to present the table as an answer for
it — see `equipment.md` → "The two estimate tables are NOT repriced per kit". Since TOE the take resolves through the fight, so it depends
on the band's own `hunterAttack` **and** its resolved carry tier — both per band. A band whose spears
have run dry hunts at the intrinsic `attack 1`, which against a Red Deer's `defense 1.0` is an
effective attack of **zero**, so no party of any size works while `denialPartyNeeded` quotes `9`. The
same is true of `huntTripEstimates`, `perWorkerYield` and every other field on the row.

**A per-band answer cannot be a straight repricing, and the reason is measured.** The two estimate
tables are **~95% of snapshot capture**: on a fully-revealed 80×52 map with 132 huntable herds
(debug) a capture runs **57.5 ms** with both, **22.5 ms** with the denial table stripped, **39.0 ms**
with the hunt table stripped, and **2.9 ms** with neither. A per-(band, herd) answer multiplies that
by the band count — three bands ≈ 165 ms per turn, on the path `turn-profiling.md` already measures
at 94% of turn time. So repricing forces a structural choice (collapse the axes, or move the
estimates off the per-turn capture — which the one-way command channel does not support today) rather
than a parameter change. `docs/plan_denial_raid.md` §3.1 and §6 question 4 own that.

Guards: `denial_raid::{the_reported_red_deer_raid_is_staffable_and_its_seeded_party_declines_the_herd,
a_herd_no_quoted_party_can_collapse_reports_no_viable_party_and_still_reads_repelled}` — the first
verifies the seeded party by **driving real raids over seeds** rather than by re-reading the
projection (the retreat is a draw and this herd is a near-run thing), paired with the ordering claim
that one hunter fewer leaves the herd standing higher; the second pairs the sentinel with the
requirement that every row still carries a verdict, so answering `0` by emptying the table would not
pass. The rounding itself is pinned on the pure helper by
`fauna::tests::a_requirement_of_eight_point_three_hunters_is_nine_and_a_tie_is_never_enough`. The
**predicate** is pinned on the pure seed by
`snapshot::subsistence::tests::{the_seeded_party_is_the_smallest_row_whose_raid_succeeded,
an_axis_with_no_success_row_seeds_no_viable_party}` — the first puts a `horizon` row *below* a
`past_recovery` one, which is the only shape where `succeeded` and `!= repelled` disagree; the second
pairs an all-`repelled` axis with an all-`horizon` one so the sentinel cannot be reached by answering
`0` for everything.

**Client-side (slice 2):** every outfit stepper caps at the band's **`idleWorkers`**, never at
`maxExpeditionPartySize`; the denial stepper additionally *seeds* at `denialPartyNeeded`, rendering
`0` as *"no party can"* rather than as a party size; and a selected size with **no exact row** —
between two rungs, or past the last one — shows the **nearest** row **with the size it was quoted
for**, on both tables. An exact-match lookup is what blanked the sheet; see "The party axis is
SAMPLED".

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

**Wire:** `HerdTelemetryState.denialEstimates` — one `DenialEstimate` per party size on
`snapshot::subsistence::denial_party_axis`, with **no floor axis**, because
the mission carries none. `denial_estimate_entries` builds it beside `denialPartyNeeded` (they are
one struct, `DenialTable`, because the second is read off the first), gated on `huntable` exactly as
`huntTripEstimates` is; cost is `3 × axis rows × hunt.forecast_horizon_turns` turn-steps per huntable
herd, the three being the reported band's quantiles. **The axis is not the bare
`estimate_party_sizes` ladder** — it carries a run at the herd's own requirement on top of it; see
"`denialPartyNeeded` — the party the sheet OPENS on".

**The waste is a PAIR, not a food scalar** — `wastedFood` **and `wastedTrade`** (appended last;
`DenialForecast::wasted_trade`), both out of one `HuntYield::apply` of the same wasted biomass,
exactly as `deliveredFood`/`deliveredTrade` already were. Denial's whole readout is what it destroys
and does not bring home, and a food-only waste line states half of it: a carcass left on the range
takes its hide with it, so on any tradeable quarry the raid's real waste was under-reported by the
whole trade component. Same widening as issue #337 everywhere else — see `yield-forecast.md` → "THE
FORECAST IS A PAIR". The launch line's prose and its `detail` carry both too (`trade=` /
`wasted_trade=`, and `server::describe_denial_ledger`, which **omits a zero component rather than
printing `~0.0`** — the `describe_haul` rule).

> **An INEDIBLE quarry is the wrong place to look for this, and not for the obvious reason.**
> `carry_room_biomass` answers `NO_CARRY_BOUND` for a species paying no provisions, so a wolf raid's
> pack **cannot bind**: it hauls every pelt it takes and its waste is honestly `0` in *both*
> components. The blindness lives on an **edible** quarry, where the pack binds hard. Pinned by
> `denial_raid::a_denial_raids_waste_is_reported_in_both_products`, whose third assertion ties the
> two exported components to one conversion of one biomass through the species' own `HuntYield` — so
> an accumulator summing some other quantity would still be positive and would still fail.

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
home: `working` back into the band's pool, the leftover pack into its larder, the trade half through
`settle_carried_trade` into that same store, `sync_size`. Its companion
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
  takes the ordinary `Returning` path, which flushes and *then* folds. Food and trade are deliberately
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

See Also: `docs/plan_exploration_and_sites.md` §2 (design), `docs/plan_denial_raid.md` (the third
verb), "Wondrous Sites" (discovery rides the flushed tiles), "Visibility Systems" (the
`Without<Expedition>` gate).

---

