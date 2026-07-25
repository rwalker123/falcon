---
paths:
  - "core_sim/src/{power,crisis,crisis_config,culture,culture_corruption_config}.rs"
  - "core_sim/src/{knowledge_ledger,espionage,great_discovery,influencers}.rs"
  - "core_sim/src/{visibility,visibility_systems,visibility_config}.rs"
  - "core_sim/src/snapshot/{vision,subsistence}.rs"
  - "core_sim/src/systems/power.rs"
  - "core_sim/tests/capability_gating.rs"
---

<!-- Extracted verbatim from lines 4790-4920 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# ECS systems reference — power, crisis, culture, knowledge, fog of war

## ECS Systems Reference

### Power Systems
Fourth in turn chain. `PowerGridState` resource tracks per-node supply, demand, transmission loss, storage charge, stability score.

**Flow**: `collect_generation_orders` → `resolve_generation` → `route_energy` → `apply_storage_buffers` → `satisfy_demand` → `evaluate_instability` → `export_power_metrics`

**Instability**: Stability bands 0-1. Thresholds: 0.4 (warn), 0.2 (critical). Incident types: brownout/blackout, containment breach, cascading failures.

### Crisis Systems
`TurnStage::Crisis` between Population and Finalize. `ActiveCrisisLedger`, `CrisisModifierLedger`, `CrisisIncidentFeed`.

**Archetypes** (from `crisis_archetypes.json`): `plague_bloom`, `replicator_swarm`, `ai_sovereign`. Each has propagation model, mitigation hooks, telemetry contributions.

**Telemetry**: `CrisisTelemetryState` with EMA-smoothed gauges, trend deltas, warn/critical bands.

### Culture Simulation
`CultureLayer` resources at faction/region/settlement scope. Each stores normalized trait vector (15 axes per manual).

**Flow**: `reconcile_culture_layers` copies global baselines down, blends with local deltas. `CultureDivergence` tracks deviation; crossing thresholds emits `CultureTensionEvent` / `CultureSchismEvent`.

**Config**: `culture_corruption_config.json` governs elasticity, `soft_threshold`/`hard_threshold`, trigger tick counts.

### Knowledge & Espionage
`KnowledgeLedger` tracks per-discovery secrecy posture, leak cadence, espionage pressure.

**Leak Timer**: `knowledge_ledger_tick` runs after `trade_knowledge_diffusion`. Recomputes `half_life_ticks` from base + visibility + security − (spy_pressure + cultural_pressure).

**Espionage**: `EspionageRoster` per faction. Mission lifecycle: Planning → Execution → Resolution. `EspionageProbeEvent` / `CounterIntelSweepEvent`.

### Great Discovery System
Constellation-level leaps from overlapping discoveries.

**Flow**: `collect_observation_signals` → `update_constellation_progress` → `screen_great_discovery_candidates` → `resolve_great_discovery` → `propagate_diffusion_impacts`

**Registry**: `GreatDiscoveryRegistry` loads from `great_discovery_definitions.json`. Fields: `id`, `field`, `requirements`, observation gate, cooldown, effect flags.

### Visibility Systems (Fog of War)

> **Fog of war is the ONLY fog concept in this repo.** There used to be a second, unrelated one —
> `FogMode` / `fogRaster`, the selectable "Fog of Knowledge" *data* overlay — and it is gone
> (`fogMode` and `fogRaster` survive only as `(deprecated)` FlatBuffers slots, since a vtable slot is
> positional). Its orphans went with it: `FogRevealLedger` / `FogReveal`, `decay_fog_reveals`,
> `FogOverlayConfig`, `StartProfileOverrides::fog_mode`, and `survey_radius` (the start-marker reveal
> radius, which only ever fed that overlay's reveal circle — a band's sight is decided entirely by
> `calculate_visibility`). A grep for "fog" now means fog of war in every case *except* those two
> deprecated schema lines.

**The master switch is server-owned: `SimulationConfig::fog_enabled` (default `true`).** It is the
single authority, and it gates BOTH halves so they cannot disagree:
- `visibility_raster_from_ledger` (`snapshot/vision.rs`) returns an all-`Active` raster when it is
  off, *before* consulting the ledger — so a viewer with no faction map still sees everything.
- `HerdSnapshotInputs::herd_is_visible` (`snapshot/subsistence.rs`) stops filtering when it is off.

It has to live on the server because unseen herds are dropped from the payload **before it is
encoded** — a client-local render flag can dim tiles but can never put back an entity the sim never
sent. The client is told the state via `VisionSection.fogEnabled` (published on every snapshot and
every delta, never diffed, because the schema default is `true` and an omitted value would silently
re-enable fog one delta after it was turned off) and renders what it is told.

Toggled by the `set_fog <on|off>` command (alias `fog`), which mutates the resource; the server's
post-command `recapture_and_broadcast` makes it visible on the same round trip. `SimulationConfig` is
deliberately **not** rollback state — `restore_world_from_snapshot` does not re-insert it — so the
setting survives a rewind, which is correct for a display preference.

Per-faction visibility tracking with three states: `Unexplored` (never seen), `Discovered` (previously seen), `Active` (currently visible).

**Files**: `visibility.rs` (state + ledger), `visibility_systems.rs` (ECS systems), `visibility_config.rs` (config loading)

**Turn Flow** (`TurnStage::Visibility` after Population, before Crisis):
1. `clear_active_visibility` - Reset Active tiles to Discovered
2. `prune_sweep_tracker` - Forget sweep positions of despawned cohorts
3. `calculate_visibility` - Compute visibility from units/settlements
4. `apply_trade_route_visibility` - Mark active trade-route tiles as Active
5. `apply_visibility_decay` - Decay old Discovered tiles to Unexplored (disabled by default; permanent memory)
6. `discover_sites` - Record any `SiteTag` tile a faction has ever seen into `DiscoveredSites`, apply the reward, push a `SiteDiscovered` feed entry (see "Wondrous Sites")

**Visibility Sources**:
- **Units**: `PopulationCohort` with `StartingUnit` marker provides sight from its
  `current_tile`. Because a unit can move several tiles in one turn (see
  `estimate_travel_turns`, travel interpolation), `calculate_visibility` reveals
  the whole **corridor** it swept from its previous position (tracked in
  `VisibilitySweepTracker`) to the current one — not just the endpoint — so
  passed-over tiles are seen (`corridor_tiles`).
- **Settlements**: `Settlement` with `TownCenter` provides sight from settlement position
- **Worked sources** (labor): a band's workers are physically out at the sources they
  work, so those spots provide fog reveal too. For each assignment in the cohort's
  `LaborAllocation`, `calculate_visibility` adds a worked source tile — a **Forage**
  assignment's `tile`, or a **Hunt** assignment's herd's **current tile** (resolved live
  from `HerdRegistry`; an unresolved/extinct herd is skipped, no panic). Each worked source
  reveals at `worked_source_sight_range` via the *same* `reveal_tiles_in_range` LOS path the
  band center and scout vantages use — additive, re-marked Active every turn while the
  assignment is staffed. Scout/Warrior are band-wide roles, not tile sources. Config:
  `labor_config.json` `worked_source_sight_range`.

**Modifiers**:
- **Elevation**: Higher elevation grants sight bonus (configurable per 100m)
- **Terrain**: Water tiles grant bonus range; forest/wetland tiles apply penalty
- **Line of Sight**: Bresenham ray-cast checks for blocking terrain
- **Local scout** (labor): staffed scouts are **forward observers** — with ≥1 scout (from the
  cohort's `LaborAllocation` head-count, `workers_on(&LaborTarget::Scout)`), `calculate_visibility`
  posts vantage tiles out from the band in all 6 hex directions (`scout_vantage_tiles`, reusing
  `grid_utils::hex_neighbor`) at `scout.vantage_distance(scouts)` = `min(vantage_distance_base +
  scouts × vantage_distance_per_scout, vantage_distance_max)`, pulling each back to the last on-map,
  passable (non-`WATER`) tile. Each vantage reveals with `vantage_range` via the *same* per-source
  LOS reveal the band uses (`reveal_tiles_in_range`), so scouts see **around** ridges/forest, not
  merely farther. The band's own base-range LOS from its center is unchanged (scouts are additive);
  the vantages are re-marked Active every turn while scouts are staffed. Config: `labor_config.json`
  `scout`.

**Config** (`visibility_config.json`):
- `decay`: `enabled` (default `false` — permanent memory; Discovered tiles never revert to Unexplored), `threshold_turns` (turns before Discovered → Unexplored when enabled)
- `sight_ranges`: Per-unit-type `base_range` and `elevation_bonus_factor`
- `elevation`: `enabled`, `bonus_per_100m`, `max_bonus`
- `line_of_sight`: `enabled`, `blocking_terrain_tags`
- `terrain_modifiers`: `forest_penalty`, `water_bonus`
- `movement`: `max_sweep_tiles` (cap on the corridor length revealed for a single-turn move; keep above the real max per-turn move distance so genuine moves sweep fully — see `corridor_tiles`)

**Snapshot Export**: `visibility_raster` emits a per-faction `ScalarRasterState` (fixed-point i64 samples) encoding Unexplored=0.0, Discovered=0.5, Active=1.0; the client decodes these to floats and renders black / cloudy / full-color. (`FactionVisibilityMap::to_byte_raster` still exists as a 0/1/2 byte view, but is not the snapshot export.)

**The ledger also GATES what the snapshot publishes, not just how the map is shaded.** Two payloads
are filtered against it for the `ViewerFaction`, and both must keep reading the *same* ledger the
raster is rendered from or the client will draw something on ground it is painting black:
`discoveredSites` (per-faction — an undiscovered site is never in `TileState` at all; see "Wondrous
Sites") and the **herd display telemetry** (`Active`-or-owned; see "Herd display telemetry is
FOG-FILTERED" under Fauna & Wild Game). Anything else that publishes a live entity position — the
predator-raid layer, a future rival-band marker — needs the same gate; the tile layer itself is
still shipped whole and masked by the raster client-side.

---

## Trade-Fueled Knowledge Diffusion

> **Deprecated / to be replaced.** `TradeLink` is dormant on a live game — nothing attaches it at
> runtime (only snapshot rehydration does; its establishment path was never built), so
> `trade_knowledge_diffusion` iterates an empty set and its test is `#[ignore]`d. The Settlement &
> Population arc reframes this: inter-faction trade becomes a **trade *policy* on the supply
> network** (see "Supply Network") — a consent gate + a priced return flow on cross-faction edges —
> and the knowledge-leak-via-open-trade behavior re-homes onto those rails. `TradeLink` /
> `trade_knowledge_diffusion` are slated for removal in that slice (not now, to avoid schema churn +
> a coherent-behavior gap). Latent bug to fix then: the logistics snapshot query requires
> `TradeLink`, so the logistics overlay is empty on a live game.

`TradeLinkState` carries throughput, tariff, `TradeLinkKnowledge` (openness, leak_timer, decay). `trade_knowledge_diffusion` runs after logistics, emits `TradeDiffusionEvent`s, applies progress to `DiscoveryProgressLedger`.

**Migration**: `PendingMigration` payloads carry scaled knowledge fragments; on arrival they merge
into the destination ledger and the whole band emigrates (`cohort.faction = destination`) — the
high-morale "brain-drain" / Cultural Osmosis vector. `simulate_population` gates it on **both** high
morale (`migration_morale_threshold`) **and** a settled duration: a band must have been simulated at
least `migration_min_settled_turns` turns (`PopulationCohort.age_turns`, incremented each turn and
snapshot-persisted) before its population can emigrate. This stops a freshly-spawned, well-fed
starting band from defecting on turn one (the `well_fed_morale_bonus` alone would otherwise clear the
morale threshold immediately).

**Config**: `trade_leak_min/max_ticks`, `trade_leak_exponent`, `trade_openness_decay`, `migration_fragment_scaling`; migration gating (`migration_morale_threshold`, `migration_eta_ticks`, `migration_min_settled_turns`) lives in the `population` block of `turn_pipeline_config.json`.

---

