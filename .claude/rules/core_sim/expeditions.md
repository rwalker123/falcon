---
paths:
  - "core_sim/src/{sites,sites_config,expedition_config}.rs"
  - "core_sim/src/systems/expeditions.rs"
  - "core_sim/src/data/{sites_config,expedition_config}.json"
  - "core_sim/tests/{raiding_party,denial_raid}.rs"
---

<!-- Extracted verbatim from lines 52-53;4004-4381 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Expeditions — wondrous sites, scouting, raiding parties and shipments

## Config files

| File | Purpose |
|------|---------|
| `src/data/sites_config.json` | Wondrous Sites catalog (`catalog`: per-`site_id` `category`/`display_name`/`glyph`/`placement_rule`/`discovery_reward.morale_bonus`) + `placement` rules (per-rule `max_sites`, `min_spacing`, and the union of rule inputs: `min_relief`, `max_habitability_pressure`, `min_food_weight`). Loader `sites_config.rs`, env override `SITES_CONFIG_PATH`. Not wired into the `reload_config` hot-reload path (mirrors `fauna_config.json`) |
| `src/data/expedition_config.json` | Expedition tuning. Scout: `comm_range_tiles` (discovery-report range), `comm_range_tech_factor` (stubbed 1.0 tech hook), `observe_sight_range` (**9** — the per-turn LOS radius a party maps at, and it is the **EQUIPPED** tier: the party resolves it through `EquipmentStat::ExpeditionSightRange` off the kit it launched with, and the `wayfinding` item declares the **bare** `6.0` that this key used to be flatly. **PLAYTEST DIAL**; see "A party's reach is its KIT's" below), `provision_draw_per_worker_per_tile` (launch larder draw = party × distance × this), `provision_upkeep_per_worker` (per-turn drain = party × this, scouts only). The `hunt` block is the **raiding party's** (the hunt *job's* party, whichever mission sends it — today the denial raid): `per_worker_carry` (carry cap = party × this — a carry bound on what a raid hauls home, never a stop), `reach_tiles` (how close to the herd to take), `forecast_horizon_turns` (**60** — how far `denial_forecast` simulates a raid before reporting `horizon`/`repelled`; each projection costs `3 ×` this turn-steps; **echoed onto every cohort as `expeditionForecastHorizonTurns`**, and it bounds the raiding only, never the trip). A far **hunt** is not an expedition: it is a work party (`work-party.md`), tuned in `labor_config.json`. Scout replenish `replenish` block: `low_turns` (top up below party × upkeep × this), `reach_tiles`. Band-fission `settle` block: `min_founding_workers` (**4**) and `parent_min_workers` (**6**) — the two worker floors a `split_band` must clear, one on each half; the arc lives in `.claude/rules/core_sim/fission.md` and is nothing to do with an expedition (`parent_min_workers` is also the floor below which a band that lost people to another people goes over with them — `factions.md`). Party-defection `defection` block: `party_pull_threshold` (**0.3**) — its row is in `factions.md`'s config table. **Retired: `estimate_party_sizes` and the whole `deny` block (`requirement_rows`)** — both were sampling axes for the pre-computed estimate tables, and the forecast query answers exactly instead; see "The forecast is ASKED FOR". Loader `expedition_config.rs`, env override `EXPEDITION_CONFIG_PATH`. Not on the `reload_config` hot-reload path (mirrors `sites_config.json`). **Validated** — `ExpeditionConfig::validate()` runs inside `from_json_str`, so *every* load path is covered; a broken invariant is logged at **error** level (`expedition_config.invalid_rejected`) and the config refused, falling back to the builtin rather than silently disabling a feature. Enforced: `comm_range_tech_factor` finite & `> 0`, `observe_sight_range ≥ 1`, `provision_draw_per_worker_per_tile`/`provision_upkeep_per_worker` finite & `≥ 0`, `hunt.per_worker_carry` finite & `> 0`, `hunt.reach_tiles ≥ 1`, **`hunt.forecast_horizon_turns ≥ 1`** (at `0` the forecast's `1..=horizon` loop runs zero turns and every raid reports a verdict it never ran), `replenish.low_turns ≥ 1`, `replenish.reach_tiles ≥ 1`, `settle.min_founding_workers ≥ 1` (at `0` the gate cannot refuse anything — a silently disabled feature rather than a tuning; the "off for a playtest" value is `1`, which is a real band). **`settle.parent_min_workers` is deliberately NOT bounded below** — `0` there is a real policy ("the parent may give everything"), unlike its sibling, for which a band of nobody is not a band. Deliberately **left free**: `comm_range_tiles` (`0` = "walk back into camp to report") and the upper end of `forecast_horizon_turns` (it costs query time, on demand — an operator's call, not an invariant) |
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

## Scouting expeditions and raiding parties

A **detached traveling party** a faction outfits and drives out — to **explore** (scout), to
**erase a herd** (the denial raid, "Denial is a MISSION" below) or to **carry a shipment** (trade,
"A shipment is a party that WALKS IT" below). One traveling-party system, three verbs. Authoritative
design: `docs/plan_exploration_and_sites.md` §2 (scout) + the Implementation-model subsection.
Config `src/data/expedition_config.json`, loader `expedition_config.rs` (`EXPEDITION_CONFIG_PATH`
override, not on the hot-reload path).

**A far HUNT is not an expedition.** A hunt that leaves the band's apron is a **work party** posted
by the band's own labour (`work-party.md`): it follows its herd anywhere, its take flows home, and
the band feeds it. There is no hunting mission, no hunting launch verb and no hunt-trip forecast on
this path — `ExpeditionMission` is `Scout | Deny | Trade`.

**An expedition is another `StartingUnit` band.** It reuses `PopulationCohort` + `BandTravel` /
`advance_band_movement` + `LaborAllocation` + `StartingUnit`, tagged with the `Expedition` component
(`components.rs`: `home_band`, `mission`, `phase: Outbound|AwaitingOrders|Returning|Hunting`,
`announced`, `pending_reveal: Vec<UVec2>`) and **deliberately lacking `ResidentBand`**.
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
turn. **Map documentation — (a)+(b) — is SHARED by every mission:** a ranging party maps the terrain
it crosses regardless of verb. **(a) observe** the tiles in LOS of its current tile — at **the radius
its own kit resolves**, see "A party's reach is its KIT's" below — into the private `pending_reveal`
buffer (reusing `visibility_systems::visible_tiles_in_range` — the pure geometry behind
`reveal_tiles_in_range` — **without** touching the faction map), and charge the wayfinding gear for
whatever of it was genuinely new; **(b) comm check + flush** — when within `effective_comm_range()`
(= `comm_range_tiles × comm_range_tech_factor`, rounded) hex distance of the home band's **live**
tile, promote every buffered tile to `Discovered` on the faction map (`FactionVisibilityMap::discover`,
Unexplored→Discovered, never downgrading `Active`) and clear the buffer — so the map lights up **as a
lump on return** (for a raiding party, at its `Returning` fold-back), and `discover_sites` records any
`SiteTag` on the flushed tiles for free. **Provisioned parties** (scout, trade) then **(c)** drain
provisions by `party × provision_upkeep_per_worker` (a raid lives off its kills; non-fatal at zero
in v1) + opportunistic replenish — **gather first, then hunt**, see below; **(d) phase transitions**
— `Outbound` + arrived (no `BandTravel`) → `AwaitingOrders` + one-shot `ExpeditionArrived` feed;
`Returning` → chase the home band's live tile (refresh `BandTravel`) and, once within comm range **or
the moment that band cannot be resolved at all**, fold workers + leftover provisions back into the
band + despawn (`ExpeditionReturned`, after the flush so the final findings report — see "One
fold-back, two moments"); `AwaitingOrders` waits. A raiding party's own arm is `Hunting` — see
"Denial is a MISSION".

**The raid's take** (`systems::expeditions::expedition_take_biomass`) is the one function the live
`Hunting` arm and the denial projection (`denial_forecast`) both resolve through, so a preview cannot
quote a raid the sim does not run. It is **engagement-bounded exactly as a resident band is**
(`docs/plan_hunt_through_combat.md` §1; §10 exempts only the pen): the party's reach
(`fauna::animals_engaged`), the quarry's retreat (`fauna::animals_that_stay`, under the caller's
`RaidRoll` — a per-event seed live, a reading of the distribution in a forecast) and the fight decide
the kill, and the one quantiser seats it. A `credit` accumulator meters *when* the next whole animal
is ready (a body heavier than one turn's processing throughput takes `body / throughput` turns) —
the party's own processing bank, a different quantity from the retired resident one. Pinned by
`denial_raid::a_denial_raid_and_a_resident_band_reach_the_same_animals`, which holds the fighting
party fixed (the raid's own kit at its `expedition_tuning`) so the engagement stage is the only thing
that could differ.

- **Shared take helpers** (`fauna.rs`): **`hunt_escapement_ceiling(floor, biomass,
  carrying_capacity)`** is THE take ceiling on the animal web — `max(0, B − floor·K)` — and
  `quantise_animal_take` rounds it to whole animals. It takes **no ecology, no `FaunaConfig`, no
  `improvement` and no ladder**, which is what makes the take `r`-independent structurally rather
  than by convention; see "The hunt policy axis" in `fauna.md`. **`HuntYield::apply(take,
  output_multiplier)`** (via `FaunaConfig::hunt_yield_for`) is the single per-species biomass→food
  conversion. `hunt_take` (`systems.rs` — band Hunt labor + the **scout's opportunistic replenish**)
  and the raid both call them, so no formula has a second copy. A raid applies **no** output
  multiplier (`EXPEDITION_OUTPUT_MULTIPLIER` — a detached party carries no band morale modifier).
- **A raid pays BOTH components of the species' `HuntYield`** (#337,
  `docs/plan_hunt_yield_model.md`). Provisions go into the party's pack (`stores[FOOD]`,
  carry-capped) and fold into the home band's larder. **Materials** go into that same `LocalStore`
  as batches and move home with `LocalStore::drain_materials_into` at the `Returning` fold-back —
  **batch by batch**, so a mammoth hide is never averaged into a hare pelt on the walk home. A haul
  that arrives with no home band left to receive it is simply lost, exactly as the carried food is.
  Both scale off the biomass the party **carried**, never what it killed.
- **`Expedition::carried_trade` is RETIRED** (arc #527) with the axis it banked; a material batch has
  the party's own store, which the checkpoint carries whole. The feed line's haul text reads
  `systems::expeditions::materials_carried` — the party's own batch total — so a wolf raid coming home
  with a pack full of hides is never called empty.
- **The IN-FLIGHT half needs no sim work.** `PopulationCohortState.materialBatches` is resolved from
  `cohort.stores` with **no `ResidentBand` gate**, so a detached party's carried materials are on the
  wire per batch, with their exact readings, for the whole trip.
- **The scout's opportunistic replenish banks its hides too** — a roadside kill is skinned as well as
  butchered. The hides are a **byproduct** of a food take and never the reason for one: the replenish
  search skips inedible herds outright (issue #373, "Gather first, then hunt" below).
- **The improvements are NOT an expedition concept, and that is a TYPE-LEVEL fact.**
  `Cultivate`/`Sow`/`Tame`/`Corral` are place-bound work a *resident* band does; no
  `ExpeditionMission` variant can name a build verb.

**Commands** (full proto/runtime/text/server plumbing, mirroring `move_band`):
- `send_expedition <faction> <band> <party_workers> <x> <y> [kit <id>]` — validates land target + `1 ≤
  party_workers ≤ available_workers` (the band, and nothing else), draws `party × distance ×
  provision_draw_per_worker_per_tile` provisions from the band larder (partial OK), removes the
  workers from `band.working`, and spawns the detached `Expedition` cohort. Feed `ExpeditionSent`.
  The **kit is a named trailing pair** (`kit <id>`, proto field `kit_id = 6`), on
  `send_denial_raid`'s shape, and it **fails closed** — an unknown id, or one whose `jobs` does not
  list `expedition`, is a command failure before anything is drawn off the band. Absent = the
  `expedition` job's default (`ranging`). Nothing else may follow it.
- `send_denial_raid <faction> <band> <party_workers> <fauna_id> [kit <id>]` (`SendDenialRaidCommand`,
  proto field **49**) — `server::outfit_raiding_party` is the one seam for the resident-band gate, the
  live-herd lookup and the party bound. **Its grammar is CLOSED except for the kit**: there is no
  floor to pass, so any other trailing token is a hard parse error rather than a value to ignore. The
  named `kit <id>` pair is a property of the **party**, not of the mission, so it is the only order a
  raid carrying no numbers still has to give (see `equipment.md` → "A kit is a MASK"). Feed
  `ExpeditionSent`, whose detail carries `mission=deny outcome=… turns_to_collapse=… low=… high=…`
  and **no `floor=`**. See "Denial is a MISSION, not a floor".
- `recall_expedition <faction> <expedition_band_id>` — resolves the entity via
  `resolve_expedition_entity` (checks the `Expedition` component + faction), sets `phase = Returning`
  (works for every verb). Feed `ExpeditionRecalled`. **A party standing in its home band's own camp
  is CANCELLED on the spot** rather than sent on a round trip — see "One fold-back, two moments"
  below.
- **There is no hunting launch verb.** `send_hunt_expedition` (and the retired `hunt_game` /
  `hunt_fauna` single-task orders) are gone from the text grammar, the proto and the server; a far
  hunt is `assign_labor … hunt …` and posts a work party.
- **There is no settle verb on this path.** `settle_expedition` held proto field **51** and turned an
  arrived party into a resident band; it is **retired**, and the slot carries `split_band` now — a
  verb about a *resident* band, which is why it is documented in `.claude/rules/core_sim/fission.md`
  rather than here. A scouting party is composed for scouting, so founding a band from one means
  founding it from inputs nobody chose; that rule file carries the argument.
- **Retargeting a scout waypoint is just `move_band` on the expedition entity** — `handle_move_band`
  has a hook that re-arms a moved expedition to `Outbound` + `announced = false`.
- `CommandEventKind` variants: `ExpeditionSent`, `ExpeditionArrived`, `ExpeditionRecalled`,
  `ExpeditionReturned`, `BandFounded` (in `as_str` + the server label map); a raid's lost-herd and
  completion feed lines reuse `Hunt`.

**Snapshot.** `PopulationCohortState` carries client discriminators `isExpedition` /
`expeditionMission` (`"scout"`|`"deny"`|`"trade"`) / `expeditionPhase`
(`outbound`|`awaiting`|`returning`|`hunting`) / `expeditionTargetHerd` (the raid's fauna_id — a
**string**, since herd ids are non-numeric; the KEY, never rendered — its display twin
`expeditionTargetSpecies` rides beside it, see "A raiding party carries its quarry's NAME") /
`expeditionCarryCap` (`party ×` the per-worker carry of the pack the **mission** fills, `0` for a
scout) and persistence-only `homeBandEntity` / `expeditionAnnounced` / `pendingRevealX` /
`pendingRevealY` (`snapshot.fbs`, `sim_schema`). Capture fills them from `Option<&Expedition>`;
`restore_sim_state` re-attaches `Expedition` for a rolled-back in-flight party (resolving
`home_band` from `homeBandEntity` via the cohort entity-remap; missing home band → log + skip) and
re-attaches `ResidentBand` to every non-expedition cohort so the `With<ResidentBand>` systems keep
running after a rollback.

**No hunt-expedition field is on the wire.** `huntReach`, `expeditionViabilityWarnTurns`,
`expeditionPerWorkerCarry`, `expeditionEtaTurns`, `expeditionProjectedDelivery`,
`expeditionRecurring`, `expeditionFloor` and `expeditionTripBound` were deleted with the hunting
expedition, and so was the `HuntTripForecastQuery` / `HuntTripForecastReply` / `HuntTripRow` query.
**`PopulationCohortState` carries no `maxExpeditionPartySize`** either — see "A raiding party is
bounded by the BAND".

- **`PopulationCohortState.expeditionForecastHorizonTurns` — the SCALE the "never completed"
  sentinels are relative to** (`expedition_config.hunt.forecast_horizon_turns`, **60**). A global
  lever echoed onto **every** cohort, same idiom as `huntPerWorkerProvisions`. It is what lets the
  client put a number on `DenialRow.turns_to_collapse{,_low,_high} == 0` rather than "away many
  turns".
  > **IT IS NOT A TRIP LENGTH.** The horizon bounds the **raiding** only and the round-trip travel is
  > a separate, already-known term (`ceil(2 × hex_distance / bandMoveTilesPerTurn)`), so the floor on
  > the whole trip is **`horizon + round-trip travel`** — *"Away more than 78 turns"*, never *"more
  > than 60"*. Quoting the horizon alone understates the trip by the entire walk.
  >
  > Pinned on the **exported snapshot** by
  > `raiding_party::every_cohort_publishes_the_forecast_horizon_on_the_wire`, which also asserts it
  > is **positive** — a lever published as `0` would let the client render *"more than 0 turns"*.
- `HerdTelemetryState.{provisionsPerBiomass, fodderPerBiomass}` — the **band / local-hunt** terms,
  from which the client composes the ceiling at **any** floor: `max(0, B − floor·K) × rate`. **THERE
  IS NO BUILD TERM ANYWHERE IN IT** — a build is staffed in its own right
  (`docs/plan_standing_upkeep.md` §2.2). There is no `huntPolicyCeilings` row list: four rows cannot
  answer a continuous dial (`yield-forecast.md` → "the sim exports the answer"). A herd below a floor
  composes `0` for it, which is the escapement rule rather than a special case. A **completed** corral
  is never hunt-drawn at all — the Hunt arm takes the tend branch — and `fauna::hunt_forecast` is the
  one place that phase split lives (`herd.is_corralled()` → `SourceYieldForecast::tended`).
- `PopulationCohortState.huntPerWorkerProvisions:float` (one hunter's provisions/turn throughput =
  `labor_config.hunt.per_worker_biomass_capacity × fauna_config.hunt.provisions_per_biomass`) — a
  global lever echoed onto **every** cohort (the `workRange` idiom). **Species-blind**: never clamp a
  per-herd preview with it; the per-herd rates are `HerdTelemetryState.perWorkerYield` /
  `perWorkerTrade`.

**The resident band's local-hunt yield preview is an exported ANSWER**, not client arithmetic: a
whole-animal take runs through `floor()`, so the sim exports the number
(`fauna::hunt_source_yield_preview` → `SourceYield.actual`) and
`band_hunt_preview::exported_snapshot_fields_reproduce_band_hunt_take` pins it to what `hunt_take`
really pays the band — healthy / clamp-binding depleted / collapsing herd × every worker count × every
swept floor × a unit and a discontent-reduced output multiplier. If the preview ever drifts from the
take, that test fails.

### A raiding party carries its quarry's NAME, not just its key

`ExpeditionMission::Deny` carries **`target_species`** — the herd's species display name
(`"Red Deer"`) — beside the `fauna_id` that keys it. Published as `expeditionTargetSpecies`, and it is
what the client renders; `expeditionTargetHerd` remains the key every command addresses the herd by,
and a player never sees it.

**The two are not redundant, because the party outlives the herd list.** Herd telemetry is fog-gated
to hexes with `Active` visibility and pruned at local extinction, and a detached expedition is
deliberately **not** a vision source (`calculate_visibility`, `Without<Expedition>` — comm-range gating
means a party must not light up the faction map from wherever it stands). So a raiding party's own
target routinely leaves the published list *while the party is still bound to it*, and a client joining
the id against that list had nothing left to render but the raw id (issue #378).

**Resolved at launch, in `outfit_raiding_party`** — the shared gate that already refuses a raid whose
herd the registry cannot resolve, so a successful launch always has a name, from one place. That is also the only moment the name is reliable: a capture-time registry lookup
would survive fog and still go blank on extinction, which prunes the registry itself.

**The sim's own event feed obeys the same rule, through `ExpeditionMission::target_display`** — the
species when it resolved, the `fauna_id` only as a last resort, one definition so no call site
re-implements the fallback. Every player-facing raid line reads it: the launch line
(`ExpeditionSent`), the lost-herd guard, and the completion line in the `Hunting` arm.
**The `detail` tokens are untouched** — `herd=<fauna_id>` is the key the client addresses the event by,
so a line names the species and its detail names the herd. The id tier is reachable only from a mission
built without a resolved name (a fixture, or a restore of a frame that carried none), since
`outfit_raiding_party` guarantees one on every real launch.

It rides `ExpeditionRecord` like the rest of the mission, so a rollback restores it — necessarily,
since it cannot be re-derived once the herd is gone
(`harvest_floor_rollback::a_raids_target_round_trips_through_the_rollback`).
`raiding_party::a_party_names_its_quarry_when_the_herd_has_left_the_snapshot`
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

**The one line is `fauna::EngagementStop`**, carried on the mission's orders
(`ExpeditionMission::raid_orders` → `RaidOrders::stop`, `Never` for a denial raid) and read by the
quantiser and `fauna::hunt_take_bound` together:

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
against what it killed, which is the point, and the rest is `AnimalTake::wasted`. Everything else —
outfitting, travel, the `Hunting`/`Returning` phases and the take path — is the shared detached-party
machinery. `ExpeditionMission::raid_orders` is the one seam the `Hunting` arm resolves the raid's terms
through.

- **`raid_orders()` reports `STRIP_IT_BARE`** — the escapement ceiling is the herd's whole standing
  stock. It is *derived*, never a lever, and **`floor` appears nowhere in the command, the feed line
  or its detail**: the launch text takes four positional tokens plus an optional named `kit <id>`,
  and refuses anything else (`CommandParseError::UnexpectedArgument`) rather than accepting a number
  and dropping it.
- **A raid hauls its real pack, however much it kills.** `carry_room_biomass` takes no floor argument
  and `NO_CARRY_BOUND` means *inedible quarry* and nothing else; an unbounded pack would record the
  party hauling home everything it killed, publish `wasted_biomass = 0` for a raid that left a range of
  carcasses, and accrue hides off the whole kill — against the "both scale off what the party carries,
  never what it killed" rule above. Pinned by
  `denial_raid::a_denial_raid_hauls_only_its_pack_and_reports_the_waste`.
- **An INEDIBLE quarry is a legitimate denial target** (a wolf). Nothing on the path divides by a
  food rate it has not established positive: the pack is inert there for a *product* reason, and the
  raid is paid in pelts. Pinned by `denial_raid::an_inedible_denial_raid_comes_home_with_pelts_and_no_food`
  (hides bank into the party's store, no food, the completion line names materials and no provisions,
  and the `Returning` fold-back drains them into the home band) and
  `denial_raid::an_inedible_denial_raids_promised_material_is_what_the_trip_banks` (the answered
  `DenialRow.delivered_material` is what the home band holds, per material, both directions). Both
  hold the wolf's `ferocity` at `0` and its diet herbivore: the projection resolves the party once and
  holds the herd and its `K` fixed, and a shipped wolf pack bleeds the party and pursues prey — eight
  hunters quoted `past_recovery` in 10 turns were still short of the line after 60.

### A raiding party is bounded by the BAND, not by a config lever

`handle_send_expedition` and `outfit_raiding_party` bound a party by **`available_workers`** and
nothing else, on every verb. Authoritative design: `docs/plan_denial_raid.md` §3.1.

The lever they used to also consult (`max_party_size`) was doing two jobs under one name. The
**rules-cap** half had no design note behind it and was deleted: at `8` it refused a party of **9**
from a band holding **16**, against a Red Deer herd needing exactly
`2.91 regrowth / 0.35 kills-per-hunter` = 9 — two unrelated eights, and the config one won. The
**sampling** half (renamed `estimate_party_sizes`) survived it by one arc and is now gone too; see
"The forecast is ASKED FOR" below.

Pinned by `server::tests::a_raiding_party_is_bounded_by_the_band_and_not_by_the_sampling_lever`,
which asserts a raid launches the party the band can field *and* still refuses a party past the band,
so "the bound moved" cannot degrade into "the bound vanished".

**Wary herds are therefore expensive, not undeniable.** Wariness raises the requirement; nothing caps
it below what the band can field.

### The forecast is ASKED FOR, not pre-computed

**The client sends a `QueryCommand` and the sim answers it on the same socket** — see
`sim_runtime/proto/command.proto` for the wire and `core_sim/src/forecast_query.rs` for the answer.
One question, one herd, one band, answered from the live world:

```
DenialRaidForecastQuery { faction, band, herd, kit, party_workers, max_party_workers }
  -> DenialRaidForecastReply { at_composed, party_needed }
```

(The hunting expedition's `HuntTripForecastQuery` went with the mission; the resident band's own
questions — `HuntCrewTakeQuery`, `ForageCrewTakeQuery`, `DepositCrewTakeQuery`,
`WorkPartyForecastQuery` — ride the same channel and are documented with their arcs.)

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
follow: a party silently re-armed answers a different question than the one asked.

**Every row echoes the party it answered.** That echo is what the retired
`huntTripEstimatesKitId` / `denialEstimatesKitId` disclaimers were compensating for: a client can
assert the answer is for its own question instead of trusting position in a list.

#### What the query replaced, and what it cost

`HerdTelemetryState` used to carry `huntTripEstimates` (floors × party sizes), `denialEstimates` (party
sizes), `denialPartyNeeded` and the two `*_kit_id` disclaimers. None of the five is on the wire.
They were pre-computed **for every huntable herd, on every frame**, and they were
wrong for anyone who had worn their gear or picked another kit:

- **One kit for every band** — the hunt job's *default*, over a **fresh** component set. A band whose
  spears have run dry hunts at the intrinsic `attack 1`, which against a Red Deer's `defense 1.0` is
  an effective attack of **zero**: no party of any size works, while `denialPartyNeeded` quoted `9`.
- **A detached raid priced at resident-hunt lethality** — the tables read `CombatConfig::tuning()`
  where every other expedition path applies `expedition_danger_multiplier`. That under-states
  casualties and so over-states the take. The multiplication now happens in exactly one place,
  `CombatConfig::expedition_tuning()`, which `advance_expeditions`, the launch line and the query all
  resolve through.
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

- **Every party size is answered exactly.** A sampled axis could only quote the nearest rung; the
  query is asked for the party the player dialled, bounded by the band's own idle workers.
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

So the `Hunting` arm's completion for a denial raid is `past_recovery`: **the party pushes the herd
under the line and walks away**, rather than killing every animal. It never delivers mid-trip and
never relaunches — there is nothing to come back for. That settles `plan_denial_raid.md` §6's second
open question. Pinned by `denial_raid::a_denial_raid_reaches_collapse_and_the_herd_stays_down`.

**Why ordinary hunting never does this by accident:** any escapement floor above `collapse_fraction`
stops the take long before, by the arithmetic of `max(0, B − floor·K)`.

### The forecast: `turns_to_collapse`, as a range

`systems::denial_forecast` — a bounded forward simulation (`fauna::regrow_biomass` then
`expedition_take_biomass`, in the live order) through the
**same** helper, so a preview cannot quote a raid the sim does not run. It is evaluated at **three
readings** of each turn's kill over the retreat's outcomes — their mean, and
`fauna::retreat_band_edge` at `±combat_config.forecast_range_sigmas` — which is slice 6's shape
applied to a turn count instead of a biomass (`docs/plan_hunt_through_combat.md` §6.4). An end of
the band is a projection in which every turn's kill is one the retreat can produce.

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
- **The projection does not model kit wear**: it is quoted for a `HuntingParty` resolved once. A raid long enough to run its spears dry therefore
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

> **The same shape WOULD work here, and it is deliberately not built.** `DenialForecast::delivered_material`
> proves a per-material vector states a projection perfectly well, so the
> original reasoning — *"a material cannot be summed into this table"* — does not survive as an
> argument against a `wasted_material` beside `wasted_food`. **Ray has ruled the waste line out of
> scope**: the waste is already legible as a percentage, so the missing half buys a second reading of
> a fact the sheet states. Recorded so the next person does not re-derive the wrong reason for it
> being absent. What must NOT happen is a flat "wasted materials" scalar — that is the retired trade
> axis under a new name.
>
> The ruling is about the **waste** alone. `server::describe_denial_ledger` states the food ledger
> **and one clause per delivered material**, off `DenialForecast::delivered_material` — the same
> field the client's denial forecast (`SourceForecast.denial_forecast`) reads off the same reply. It
> falls back to *"nothing worth hauling from this quarry"* only when there is neither food nor material
> to weigh. Pinned as a pairing by
> `server::tests::an_inedible_raids_ack_names_the_materials_its_forecast_promises`, because *"always
> name the hides"* would otherwise be satisfiable by deleting the fallback.

> **An INEDIBLE quarry is the wrong place to look for this, and not for the obvious reason.**
> `carry_room_biomass` answers `NO_CARRY_BOUND` for a species paying no provisions, so a wolf raid's
> pack **cannot bind**: it hauls every hide it takes and its waste is honestly `0`. The blindness
> lives on an **edible** quarry, where the pack binds hard.

**A LOST HERD IS ONE OF DENIAL'S TWO WINS, and the guard's line says so.** The lost-herd guard
reports *"Denial raid wiped out the …"* with `reason=` **`DenialOutcome::HerdLost`'s own wire key**
— the verdict `DenialOutcome::succeeded` returns true for and the launch sheet quotes as a win. The
completion line's `status=` is `DenialOutcome::PastRecovery`'s key the same way. Both tokens are read
off the enum rather than spelled, so the exit and the pre-launch verdict cannot name the outcome two
ways. Pinned by `denial_raid::a_denial_raid_that_loses_its_herd_reports_a_win`.

### What it costs, and the kit cost needed nothing new

Travel, party exposure and a near-zero return are the listed costs. The fourth is the **kit**, and it
holds for a denial party with no new mechanism: `advance_expeditions` already charges
`wear_hunting(.., take.killed)` per animal **killed** and `wear_sled(.., take.carried)` per unit
**hauled** — wear tracks *use*, never turns elapsed, which is what `plan_denial_raid.md` §1.2
required. A denial raid is by construction the most kill-intensive act in the game, so it burns the
most irreplaceable kit for no food return; a party that engaged nothing spends nothing. Pinned by
`denial_raid::a_denial_raid_burns_kit_and_only_for_kills`.

**Not in scope, settled rather than deferred:** no target faction (denial aims at a herd, not a
player, so there is no nullable field nothing reads) and no plant twin (`reseed_floor_fraction`
guarantees a stand returns and plants have no Allee term, so a herd can be erased permanently and a
stand only set back).

**A denial party publishes no delivery forecast**, deliberately: its readout is the collapse
verdict, not a delivery ETA. Quoting "next delivery" for a raid whose whole point is that nothing
comes home would be the food-only blindness the mission reverses.

## A party's reach is its KIT's — `observe_sight_range` is the EQUIPPED tier

A detached party's observation radius was a **flat** `expedition_config.observe_sight_range` with no
kit term in it at all, while the *resident* band's posted vantage had been kit-aware for a long time
(`visibility_systems.rs` resolves `scout_vantage_range(labor.scout.vantage_range, &scout_kit, wear)`,
so the wayfinding kit buys 2 tiles against 1 bare-handed). The party is priced the same way now:
`advance_expeditions` resolves

```text
equipment_cfg.expedition_sight_range(cfg.observe_sight_range as f32, &party_kit, &party_wear)
```

**once per party per turn**, exactly as it resolves the haul and gather tiers beside it, and rounds
the answer for the reveal geometry.

**`observe_sight_range` is now the EQUIPPED value and ships `9`; the `wayfinding` item declares the
bare `6.0`** — which is what that key used to be flatly. So **nothing regresses**: a party carrying
no wayfinding gear sees exactly what it saw before, and a kitted one sees three tiles further. It is
pure upside for carrying the gear, and `ranging` carries it (`equipment.md` → "The `expedition` job").

### ⛔ IT IS A SECOND STAT, NOT THE VANTAGE'S

`EquipmentStat::ExpeditionSightRange` is its own stat rather than a reuse of `ScoutVantageRange`, and
the reason is mechanical: `rate_tier` takes the **equipped** side from the caller's baseline and the
**unequipped** side from the item, and an item declares **one** unequipped side per stat. Reusing the
vantage's stat would therefore drag a bare-handed *party* down to the vantage's bare `1.0` — wrong,
because a band standing still sees `observe_sight_range` far with no gear at all, and a detached party
is the same people with the same eyes. **What the gear buys is reach beyond unaided sight, never the
ability to see at all.**

A posted vantage (one or two people on a hilltop, equipped range 2) and a whole ranging party are two
different observers with two different **bare** values, so they are two stats. One item may legally
declare both, and `wayfinding` does.

### The gear is charged AT OBSERVE TIME, on tiles genuinely new

A party buffers its observations in `pending_reveal` and flushes them to the faction map later, so
there are three places the charge could have gone and only one of them is a *use*:

| where | what it would actually price |
|---|---|
| at the **comm flush** | the turn the party walked back into camp — and a long trip lands its whole bill in one lump |
| per **buffered tile** | re-crossing ground the party already mapped — a turn clock in a per-use costume, which `docs/plan_denial_raid.md` §1.2 forbids outright |
| **at observe, on new ground** | the looking that was actually done ✅ |

So a tile is counted only if it is **not already in the party's own `pending_reveal` buffer** *and*
**not already `Discovered`/`Active` on the party's faction map** (`ledger.is_discovered`, a **read** —
the flush still owns every mutation of the ledger). The count is accumulated across the observe loop
and charged once after it, `wear_kit(.., WearQuantum::TileRevealed, newly_seen)` — **accrue after
take**, the ordering every wear site uses, so this turn's ground was seen at the tier it was priced
with and any step-down lands next turn.

Charging while the party is *out doing the looking* settles two things the flush could not: an
**orphaned** party that never reports still wore its gear, and a long trip does not bill its whole
march on one turn.

**Pinned in `core_sim/tests/expedition_sight.rs`**, whose important test is
`a_party_re_walking_mapped_ground_wears_nothing` — the turn-clock guard, asserted together with its
liveness half (`a_party_mapping_new_ground_wears_the_kit_down`), because a charge that never fired at
all would pass the guard on its own.

### The resolved reach rides the wire per kit

`KitOption.expeditionSightRange` (appended last, subsistence section) carries what a party carrying
that kit observes at, so a launch sheet's gear line quotes the sim's own number. **It is not
`scoutVantageRange` read twice** — the two observers have different bare readings, so a sheet quoting
the vantage's would tell a bare ranging party it sees one tile when it sees six.

## Gather first, then hunt — how a provisioned party feeds itself

A **provisioned** party (`Scout` and `Trade`, the two missions that drain
`provision_upkeep_per_worker`) tops itself up when its larder falls below
`party × provision_upkeep_per_worker × replenish.low_turns`, off the ground within
`replenish.reach_tiles`. It does so in **one order**:

1. **Gather.** The nearest `ForagePatch` in reach with room above `DEFAULT_ESCAPEMENT_FLOOR` is drawn
   through the same `forage::forage_take` primitive a resident band's gatherers use, at that same
   restrained floor — so replenishing on the march can never be the thing that ruins a stand. The
   draw is bounded by the party's **room to the low-water mark**, inverted through the stand's own
   conversion rate (the plant twin of the roadside kill's `carry_room_biomass`), so a nearly
   topped-up party takes less off the ground rather than gathering food it must drop. The kit's
   baskets are charged `WearQuantum::BiomassGathered` for the biomass actually taken.
2. **Hunt.** Only if the party is *still* below the mark does it fall through to the opportunistic
   roadside kill, unchanged.

> ### ⛔ IT IS AN ORDER, NOT A SCORE — do not add a ranking pass between the two
>
> Gathering costs no lives, no animals and no weapon wear; the party spends baskets and walks on. A
> kill costs casualties, spears and a herd. So a party exhausts the safe option before it picks a
> fight, and that single rule is the whole model. A scoring pass would let a fat herd outbid a stand
> the party could have stripped for nothing — the one trade nobody would make.
>
> The stand is chosen by `(distance, y, x)` rather than by first match, because
> `ForageRegistry::patches` is a `HashMap` and its iteration order is not deterministic; the herd
> search keeps its "first match" walk over the herd `Vec`, which is ordered.

**The gather rate is kit-resolved, like every other carry on both webs.** It is
`coverage.weighted_rate(forage_per_worker_biomass_capacity(labor.forage.per_worker_biomass_capacity,
…))` — the bare-handed baseline `1.6` stepped up to the baskets' own `8.0` tier, averaged over the
crews the party's gear actually covers. A party sent out with `none` gathers bare-handed for its whole
life, which is what the kit choice at launch is *for* (`equipment.md` → "The `expedition` job").

### The replenish hunt skips INEDIBLE herds outright (issue #373)

The herd search tests `HuntYield::edible()` (`provisions_per_biomass > 0`) before distance. A starving
party used to kill a Grey Wolf Pack it could not eat — spending casualties, spears and a whole herd to
bank pelts — and then walk on still starving.

**Skipped, not ranked last**, and the reason is what the arm is *for*: it is triggered by the **food**
low-water mark, so a quarry that pays no provisions cannot answer the question that was asked.
Materials are a byproduct of a food take here and never the reason for one; the hunt verb remains the
way to go after pelts deliberately.

Pinned by `core_sim/tests/expedition_replenish.rs` — four fixtures: a party the stand can fill does
not hunt, a party with no stand in reach does, a starving party leaves an inedible herd standing, and
the `ranging` kit gathers strictly more than bare hands.

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
`raiding_party::a_returning_party_with_no_home_band_left_does_not_haunt_the_map`.

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
larder identity `larder_delta == foodIncome − foodConsumption − raidForfeit − foodSpoiled +
transferReceived − transferSent` is about food that entered a *larder*, and hay never enters one. The
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

- **Carry cap** = `expedition_config::shipment_carry_cap` — `party_workers ×` the **resolved**
  per-worker carry (`trade_per_worker_carry`, today `trade.per_worker_carry` and nothing else) —
  where a shipment's mass is
  `food + trade.fodder_carry_weight × fodder + trade.material_carry_weight × Σ material amounts`.
  The two weights are config levers, the carry is resolved rather than read, and none of the three is
  a literal. See "Cargo is food, FODDER and materials" below
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
  party's own pack and returns a `FoldBack` that keeps the two stores apart — `pack_food` /
  `pack_materials` from the party's `stores`, `cargo_food` / `cargo_fodder` / `cargo_materials` from
  `Expedition::cargo` — so the feed line and the ledger cannot disagree about one arrival, and
  `FoldBack::book_home` can book each store under its own cause, per rating for the batches.

### The launch books TWO causes: the cargo is the shipment, the walking larder is the party's own

Every route writer books through `LaborAllocation::book_crossing` with a `TransferCause`
(`campaign.md` → "The cause key and the crossings list"), and the trade launch is the one writer that
moves two different things at once. The debit used to be `cargo + provisions` in one number, so a row
reading *"Shipment to Bitterbrook — 12.0"* overstated the shipment by what the party eats on the
road. It is now two crossings:

| What left | Cause | Counterparty | Why |
|---|---|---|---|
| the cargo — food, hay, each material batch at its rating | `ShipmentOut` | the destination band, with its faction | a transfer to another band |
| the walking larder (`provisions`) | `PartyProvisions` | none | the party's own rations, eaten on the road and folded back if unspent — the scout's launch-larder cause |

**The food ledger's route arm is unchanged** — it still carries cargo plus larder, because both left
the larder through neither consumption nor a pen; the split is in the cause. Both are booked **after**
`launch_party_from_band`, so every row names the party's `BandId` (`TransferCrossing::party`), the key
a client groups one shipment under. `ResolvedShipment` carries the destination's faction for the
counterparty, since a foreign destination may have no row on the sender's wire. Pinned by
`server::tests::a_shipment_launch_books_cargo_as_shipment_out_and_the_larder_as_party_provisions`.

The other route writers, by cause:

| Site | Cause | Counterparty / party |
|---|---|---|
| a scout's launch larder (`handle_send_expedition`) | `PartyProvisions` | none / the scout party |
| a shipment landing (`advance_expeditions`, `Outbound`) | `ShipmentIn` — food, hay, and each drained batch at the rating it moved at | the sender (the party's `home_band`) / the party |
| the `Returning` fold-back, and a cancel in camp — **the pack** | `PartyHome`, through the one `FoldBack::book_home` | none / the party |
| the `Returning` fold-back, and a cancel in camp — **the undelivered cargo** | `ShipmentReturned` — food, hay, and each batch at its rating, through the same `FoldBack::book_home` | the destination (`ExpeditionMission::consignee`, the counterparty its `ShipmentOut` named) / the party |

**The destination's faction rides `ExpeditionMission::Trade::destination_faction`**, fixed at launch,
because the cargo comes home *because* the destination is gone often enough that re-reading it off
the live band would leave the returned rows unable to name it. It is never branched on. Pinned by
`server::tests::a_shipment_cancelled_in_camp_comes_home_as_shipment_returned` (food and hay, a cancel
in camp),
`transfer_fodder_ledger::undelivered_hay_coming_home_is_a_shipment_returned_row_naming_the_destination`
and `trade_expedition::a_destination_that_vanishes_sends_the_party_home_with_its_cargo` (hay and
material batches, the `Returning` fold-back after the destination died).

**`PartyHome` and `PartyProvisions` are not trade.** They ride the route arm because a party carried
the goods, which keeps the ledger whole, but the other end is the band's own people. The cause is what
lets a trade readout leave them out and a food readout put a homecoming beside the hunts it came from.

### The phases are the ones that already exist

`Outbound` → (arrive, deposit) → `Returning` → fold back. **No new `ExpeditionPhase`**: the party does
exactly two things and both already have a phase.

- **Retargets the destination's LIVE tile every turn**, mirroring the `Hunting` arm's herd retarget —
  bands are nomadic, and a shipment aimed once at where a people were camped arrives nowhere.
- **Arrival is the comm-range proximity** the fold-back already uses (*"near enough to hand things
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
turn, and the same opportunistic replenish off the ground it crosses — **gathering before hunting**,
on the `expedition` kit job like the scout. It is a walking party carrying no
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
| `expeditionTradePerWorkerCarry` | *"how big a shipment can I send?"* — **before** there is a party | `expedition_config::trade_per_worker_carry` — the **resolved** per-worker carry, never the raw lever — published onto **every** cohort |
| `expeditionTradeFodderCarryWeight` | *"what does a unit of hay cost me in pack space?"* | `expedition_config.trade.fodder_carry_weight`, same every-cohort echo |
| `expeditionTradeMaterialCarryWeight` | *"what does a unit of hide cost me in pack space?"* | `expedition_config.trade.material_carry_weight`, same every-cohort echo |
| `expeditionCarryCap` | *"how full is this party?"* — a party already on the map | `party_workers ×` the per-worker carry of the pack **its mission** fills |

The three published terms are the sim's own mass expression, and the client holds it verbatim:

```text
mass = expeditionCargoFood
     + expeditionTradeFodderCarryWeight   × expeditionCargoFodder
     + expeditionTradeMaterialCarryWeight × Σ material amounts
cap  = party_workers × expeditionTradePerWorkerCarry
```

They ride **every cohort** rather than only the parties: the outfit UI prices a manifest for a party
that does not exist yet, and `party_workers` is the number the stepper is *choosing*. Same idiom as
`huntPerWorkerProvisions` / `expeditionForecastHorizonTurns`.

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
> meter stops them getting it wrong.** When a *goods* weight gains a real model — the density axis
> `materials.json` does not author yet — that lever changes, the client's expression changes with it,
> and both move in the same PR: the ordinary cost of a client-side readout, not a new hazard.
>
> **The carrier side is deliberately NOT left on those terms**, because it is the half expected to
> grow — see the callout below.
>
> **The server-side refusal is unchanged and remains the authority.** The meter is a courtesy that
> keeps the player from ever meeting it.

> #### ⛔ THE CARRY IS RESOLVED BY THE SIM; THE CLIENT OWNS ONLY THE MULTIPLICATION
>
> `expeditionTradePerWorkerCarry` publishes **what one worker on this shipment carries**, not
> `trade.per_worker_carry`. Today they are the same number — a party carries what its people can
> carry — and the field is still deliberately not documented as the second one (issue #626).
>
> **What the distinction buys is a bound on what the client's copy of the rule contains.** `cap =
> party_workers × per_worker_carry` is a *formula*, and a client holding a formula holds an
> assumption about what carry depends on: today, *"workers, and nothing else"*. Carry is the term
> expected to grow a carrier-side model — a cart, a wagon, a `trade_carry` equipment stat, a road
> grade — and the day one lands, a client multiplying a raw lever renders a cap the sim does not
> enforce, in whichever direction the model moved, **with nothing failing**: the meter just lies.
> Publishing the resolved number leaves the client owning one multiplication, which no carrier model
> can invalidate.
>
> **The goods weights stay raw lever echoes, deliberately.** What a unit of hay or hide costs in pack
> space is a property of the *cargo*, not of the carrier, so they acquire no carrier-side model to
> resolve and gain nothing from the same treatment.
>
> **TWO EXPRESSIONS, EACH WRITTEN ONCE, AND EVERY CONSUMER CALLS ONE.**
> `expedition_config::trade_per_worker_carry` answers the per-worker question and
> `shipment_carry_cap` is its product with the party. The launch refusal (`resolve_shipment`) and the
> per-mission `expeditionCarryCap` are both the second; the every-cohort echo is the first. **Neither
> is ever restated at a call site** — and that is a rule about the model that is *not* per-worker: a
> wagon holds what it holds however many people walk beside it, so it attaches inside
> `shipment_carry_cap`, where a consumer that multiplied a per-worker number itself could never see
> it. The refusal would enforce the wagon while the published cap went on quoting `workers ×
> per-worker`, and nothing would fail. So the snapshot takes the **config** down to the cohort row
> (`ExpeditionLevers::trade`, the one borrow in a struct of scalars) rather than a pre-multiplied
> scalar, and resolves **per band row** rather than once per capture: an input that comes to vary by
> band — a cart kit — is then a change to the resolver's arguments and nothing else.
>
> **What the WIRE shape still assumes is that carry is linear in the party.** The client is handed a
> per-worker number and multiplies, so a model that is not linear — that same wagon — is a change to
> what the field *publishes*, not merely to what resolves it: a cap-shaped answer would have to cross
> instead, and before launch there is no party to hang one on. That is a wire question for the day a
> wagon exists, and a different question from the one this seam settles.
>
> Pinned by `bin/server.rs`'s `the_published_per_worker_carry_is_the_cap_the_launch_command_enforces`,
> which reads the carry off the **encoded envelope** and asserts both sides of the boundary — a
> manifest of exactly `party × published` launches whole, one epsilon over is refused with the larder
> untouched. It is asserted against the *published* number rather than against the config because a
> carry model that grew server-side but skipped the resolver would satisfy an equality with
> `trade.per_worker_carry` and still mis-meter every client. It lives with the launch command rather
> than in `tests/trade_expedition.rs` because that harness spawns its party by hand and never
> consults the cap, so neither half of the boundary is observable from it.

**The three carry different wire bounds, deliberately.** The published carry is asserted
**positive** for the horizon's reason — a `0` lets a client render a zero cap and refuse every
manifest a player could build, so the resolver must preserve the lever's validated positivity
whatever it later multiplies it by. `material_carry_weight` **and `fodder_carry_weight`** are asserted
only **finite and `>= 0`**, because `0` is a legitimate setting on a *goods* weight (*"materials are
weightless"*) and asserting positivity would pin a tuning as if it were a rule.

**`expeditionCarryCap` resolves per mission**, and that is what stops a client reaching for the hunt
lever: a raid's pack is the provisions ceiling of what it hauls home, and it is filled by the raw
`hunt.per_worker_carry`; a shipment's is what its people can carry out, and it is
`shipment_carry_cap` — the **resolved** carry times the party, per the callout above. Two packs,
arrived at two different ways, and the asymmetry is deliberate: the carrier side is the half expected
to grow a model, the raid's provisions ceiling is not. `0` stays a scout's and a resident band's
answer. Pinned by `trade_expedition::{every_cohort_publishes_the_shipment_mass_levers_on_the_wire,
a_trade_partys_carry_cap_is_quoted_at_the_resolved_shipment_carry}` — the first composes a real
shipment's mass out of nothing but wire fields and checks it against the published cap, the second
asserts the cap is `shipment_carry_cap`'s product **and not** the hunt lever's, after first asserting
the two numbers differ so "quoted at the right one" is falsifiable.

### The food ledger gained two terms, and one of the holes was pre-existing

A shipment moves food between larders through neither `foodIncome` nor `foodConsumption`, so
`PopulationCohortState.transferReceived` / `transferSent` were added to close it — and the **same**
pair closes `balance_supply_networks`, which had been moving food between larders untracked since
turn one. The full argument, the identity and the reset window live in
`.claude/rules/core_sim/campaign.md` → the transfer callout; what belongs here is which expedition
seams write it: the launch draw (cargo **and** the walk's larder, for both this verb and the scout's),
the shipment's arrival at the destination, and every fold-back including the in-camp cancel.
