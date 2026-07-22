# core_sim - Simulation Engine

Bevy-based ECS headless simulation that resolves turns via `run_turn`. Systems execute in order: materials → logistics → population → power → tick increment → snapshot capture.

## Quick Reference

```bash
# Build
cargo build -p core_sim

# Test
cargo test -p core_sim

# Benchmark
cargo bench -p core_sim --bench turn_bench

# Run server
cargo run -p core_sim --bin server
```

## Configuration Files

| File | Purpose |
|------|---------|
| `src/data/simulation_config.json` | Grid size, environmental tuning, trade/power/corruption multipliers, TCP bind addresses (see `SIM_PORT_BASE` under Environment Overrides for per-checkout port shifting) |
| `src/data/map_presets.json` | World generation tuning parameters |
| `src/data/start_profiles.json` | Campaign initialization (units, inventory, knowledge tags) |
| `src/data/victory_config.json` | Victory mode thresholds and `continue_after_win` flag |
| `src/data/turn_pipeline_config.json` | Per-phase clamps for logistics, trade, population, power |
| `src/data/knowledge_ledger_config.json` | Leak timers, suspicion decay, countermeasure scaling |
| `src/data/espionage_agents.json` | Agent archetypes and generator templates |
| `src/data/espionage_missions.json` | Mission templates with success/fidelity bands |
| `src/data/espionage_config.json` | Security posture penalties, probe resolution tuning |
| `src/data/crisis_archetypes.json` | Plague, Replicator, AI Sovereign definitions |
| `src/data/crisis_modifiers.json` | Shared modifier definitions with decay models |
| `src/data/crisis_telemetry_config.json` | Gauge thresholds, EMA alpha, trend windows |
| `src/data/great_discovery_definitions.json` | First-wave constellation catalog |
| `src/data/culture_corruption_config.json` | Culture propagation, divergence thresholds, corruption penalties |
| `src/data/influencer_config.json` | Roster caps, decay factors, scope thresholds |
| `src/data/snapshot_overlays_config.json` | Overlay normalization weights |
| `src/data/visibility_config.json` | Fog of War sight ranges, decay, terrain modifiers |
| `src/data/labor_config.json` | Early-Game Labor allocation: `band_work_range` (true odd-r **hex-distance** radius of in-range sources — `grid_utils::hex_distance_wrapped`, wrap-aware), `worked_source_sight_range` (fog reveal range around each worked Forage tile / Hunt herd tile in `calculate_visibility`), `hunt_leash_tiles` (extra leashed-follow reach for Hunt), `band_move_tiles_per_turn` (`move_band` speed), `forage` (**depletable-forage** ecology, §0-ii: **`capacity_by_biome`** — the **human food web's** per-biome capacity table, a **total** table (one row per `TerrainType`) mirroring `fauna_config.json`'s `graze.capacity_by_biome` (the *animal* web) row-for-row and meant to **disagree** with it (see "The two food webs"); it replaces the retired flat `carrying_capacity` of 120 — `per_worker_biomass_capacity` gather throughput, `provisions_per_biomass` biomass→food conversion, and an `ecology` block reusing fauna's `EcologyConfig` — `regrowth_rate` tuned higher than fauna's 0.05, plus `collapse_fraction`/`stressed_fraction` phase bands; supersedes the retired flat `per_worker_yield` — **plus the §0-iii policy axis** `surplus_multiplier` / `market.{take_fraction,trade_goods_multiplier,trade_goods_per_biomass}` / `eradicate.take_fraction`, mirroring fauna's follow/market/hunt levers so forage has Sustain/Surplus/Market/Eradicate parity with hunting — **plus the Phase 1a `cultivation` block** — the plant ladder's **two rung payoffs, in two different currencies (slice 7)**: **`tended_regrowth_gain` (1.5, rung 2 — the plant twin of `husbandry.pastoral_gain`, at its value: a tended patch is STILL A WILD STAND on a boosted curve, gathered under the full policy axis and drawn down)** and **`field_provisions_per_biomass` (0.02, rung 3 — a managed rate on the standing crop, no drawdown, policy axis collapsed, because at rung 3 the source is YOURS)**; both PLAYTEST DIALS, and each rung must beat the one below it or climbing buys nothing (**now `validate()`-enforced, scale-free in `K`**). The retired `tended_provisions_per_biomass` (0.01) made rung 2 a *managed* rate a full rung earlier than the animal side's, so a tended patch could not be over-farmed and every policy paid the identical number (**the plant rung-2 BUILD dials — the old `progress_per_turn`/`decay_per_turn`/`cultivating_yield_fraction` — moved to `intensification_ladder.json`'s `plant:tended` rung**, and in slice 4 **the earned-knowledge levers `knowledge_progress_per_turn`/`knowledge_completion_threshold` moved to that file's ladder-level `knowledge` block** too, so both food webs climb *and learn* on the same numbers) (Rung 1a: cultivation is the explicit **`Cultivate` policy** — while preparing, the patch yields only the `plant:tended` rung's `yield_fraction_while_building × its Sustain/MSY ceiling` (the investment cost) and accrues that rung's `progress_per_turn`; at 1.0 the tended patch pays the tending band `biomass × tended_provisions_per_biomass` place-local, higher than wild MSY, and goes feral if abandoned. Rung 1b: working a **wild** patch under a stewardship policy earns faction **Cultivation** knowledge in the `DiscoveryProgressLedger`, the gate on the Cultivate policy — Sustain itself never tames a patch, and the old `claim_threshold` early-claim is **removed**; the accrual is the ladder's, driven off the rung — see "The knowledge pattern"); see "Cultivation"), `hunt.per_worker_biomass_capacity` (per-hunter take cap; biomass→provisions/trade reuses `fauna_config.hunt.*_per_biomass`), `scout.vantage_distance_base`/`vantage_distance_per_scout`/`vantage_distance_max`/`vantage_range` (staffed scouts post forward-observer vantages in all 6 hex directions and reveal LOS from each in `calculate_visibility`, so they see *around* obstacles). **Validated** — `LaborConfig::validate()` runs inside `from_json_str` (every load path, the `fauna_config.rs` convention), rejecting a **partial / all-zero / negative `forage.capacity_by_biome`** (a missing biome would silently read as an invisible zero-forage dead zone — **zero must be stated, never defaulted**); a broken invariant is logged at **error** level (`labor_config.invalid_rejected`) and the builtin is used |
| `src/data/intensification_ladder.json` | **THE INTENSIFICATION LADDER** — one grammar for both food webs (`intensification.rs`, env override **`INTENSIFICATION_LADDER_PATH`**; design `docs/plan_intensification_ladder.md` §5). A `knowledge` block (**`progress_per_turn` 0.05 / `completion_threshold` 1.0** — the pace of EVERY rung's `earns_knowledge` and the bar at which a faction may act on one, ~20 turns per lesson; **moved here in slice 4 from the two identical per-web copies** in `labor_config`'s `forage.cultivation` and `fauna_config`'s `husbandry`, once the earn path became one rung-driven seam — the number paces *both* webs, so it belongs to the ladder, exactly like the build dials) plus a flat `rungs` list; each record is one rung of one branch (`plant` = forage patches, `animal` = herds): `id`/`branch`/`order`, `verb` (the `FollowPolicy` that fills this rung's per-source build meter — **`null` = no verb drives this rung today, and the engine skips it**), `unlock_knowledge`/`earns_knowledge` (knowledge ids the rung gates on / **teaches when practised** — `null` = ungated / teaches nothing; **both are LIVE**: `unlock_knowledge` is what every gate resolves through, and `earns_knowledge` drives `RungDef::knowledge_earned`, the one earn seam), `requires_rung` (the rung directly below on the ladder — the ladder is strictly sequential; **a claim about the ladder's SHAPE, not a per-source precondition** — no code reads it as one, and the per-source rule differs per branch: `corral` demands a herd you already tamed, `sow` demands no prior patch at all), `ceiling_required` (the per-species `husbandry_ceiling` gate, animal branch only), **`site_requirement`** (`{ min_forage_capacity, requires_fresh_water }` — **what the LAND must be** for the rung to be placed on a tile; the plant twin of `ceiling_required`, keyed on the ground instead of the species. `null` = the rung asks nothing of the site, i.e. every rung but `plant:field`. **Rung 4 (Worked Land) will be a looser copy of this record and nothing else**), `build` (`progress_per_turn`/`decay_per_turn`/**`yield_fraction_while_building`** — the per-source meter's rate, its abandon-decay, and the **investment dip** the source pays while the crew prepares instead of harvests; `null` on a rung with nothing to build), and `behavior` (the bounded coded primitives `movement` ∈ `fixed|roam|drift_to_owner` — **read by `fauna::advance_herds`, the first live primitive (slice 3b)**, `feeding` ∈ `photosynthesis|forage|self_graze`, `harvest` ∈ `worker_take|worker_tend|passive` — the last two still **parsed and validated only**). **Shipped rungs:** plant `wild`(1, earns `cultivation`)/`tended`(2, verb `cultivate`, gate `cultivation`, **earns `seed_selection`**, build `0.04`/`0.01`/`0.25`)/**`field`(3, verb `sow`, gate `seed_selection`, earns nothing, build `0.04`/`0.01`/`0.25`, `fixed`, site `{ min_forage_capacity 195, requires_fresh_water true }` → **49 sowable tiles of 4160** on the standard map)**; animal `wild`(1, earns `herding`, `roam`)/`pastoral`(2, verb `tame`, gate `herding`, ceiling `pastoral`, **earns `penning`**, build `0.04`/`0.01`/`0.50`, **`drift_to_owner` + `worker_take`**)/`pen`(3, verb `corral`, gate **`penning`** (slice 4's §4.3 reshuffle — was `herding`), ceiling `pen`, build `0.04`/`0.0`/`0.50`, `fixed`). **The file describes what the sim does TODAY, deliberately** — later slices change behaviour by *editing it*. **Validated** — `LadderConfig::validate()` runs inside `from_json_str` (every load path, the `fauna_config.rs` convention): unique `(branch, id)` and `(branch, order)`, exactly one order-1 rung per branch, `requires_rung` resolving to a real same-branch rung at `order - 1` (and `null` iff `order == 1`), `verb` parsing to a real `FollowPolicy`, `unlock_knowledge`/`earns_knowledge` resolving to a known discovery id, `0 < progress_per_turn`, `0 <= decay_per_turn < progress_per_turn`, `0 < yield_fraction_while_building < 1`, a `site_requirement`'s `min_forage_capacity` finite & `>= 0` **and the requirement actually requiring something** (a floor of `0` with `requires_fresh_water: false` admits every tile — a placement rule that places no rule, which is how a rung's scarcity evaporates silently; say `null` instead), **`knowledge.progress_per_turn > 0`** (else nothing is ever learned and the ladder silently freezes at rung 1) and **`0 < knowledge.completion_threshold <= 1`** (at `0` every gate opens on turn 1; above `1` no gate can ever open, since the ledger clamps accrual to `1.0`) — both **stated once, for both webs**, having moved from each web's own config — and **every rung the engine names by hand (`RungKey`) present** (so a broken override cannot silently no-op a shipped rung); a broken invariant is logged at **error** level (`intensification_ladder.invalid_rejected`) and the builtin is used. See "The Intensification Ladder" |
| `src/data/fauna_config.json` | Wild-game species table (display, size class, migratory flag, route length = anchor count, biomass, host biomes, + movement cadence `dwell_turns` / migratory `loiter_turns [min,max]` / `loiter_radius`, + **`fodder_per_biomass`** (Grazing 2b-i — graze the herd eats per unit biomass/turn; cached on `Herd` at spawn) + **`regrowth_rate`** (Grazing 2b-ii — per-species WILD breeding rate, `Option`, cached on `Herd`; rabbit/fowl 0.35, deer/boar 0.10, migratory 0.04 — replaces the single global `ecology.regrowth_rate` for wild herds; see "Phase 2b-ii") + **`taming_rate`** (intensification ladder slice 3c — a **per-species multiplier on the `animal:pastoral` rung's BUILD**, default **1.0**; the rung owns the taming mechanic, the species scales it (the `regrowth_rate`/`pastoral_gain` split again). It scales **`progress_per_turn` AND `decay_per_turn`** — a whole **timescale**, so the rung's 4:1 ratio is invariant: *slow to tame, slow to forget*. Roster: rabbit/fowl/crag_goat 1.0 (25 turns), boar 0.8 (~31), aurochs 0.5 (50), steppe_runner/marsh_grazer 0.2 (125); deer/mammoth omit it (`wild` ceiling — never tame). **Playtest dials.** Validated finite & `> 0`; resolved live by display name (`FaunaConfig::taming_rate_for`), *not* cached on `Herd`, so a retune reaches herds already on the map. See "The `Tame` verb") + **`husbandry_ceiling`** (Grazing 2d-δ — `wild`|`pastoral`|`pen`, default `pen`; how far up the ladder the species climbs — mammoth/deer `wild`, steppe_runner/marsh_grazer `pastoral`, boar/rabbit/fowl `pen`; cached on `Herd`, gates domestication + corral/extend; see "Phase 2d") + **`pastoral_density` / `pen_density`** (the per-species husbandry DENSITY (K) multiplier per rung, default **1.0** = neutral; domestication makes the LAND hold more animals, non-linearly by species — DISTINCT from the global r-gains, which scale the breeding rate not the ceiling. Roster: crag_goat/aurochs 2.0/5.0, boar 1.5/4.0, rabbit/fowl 1.1/1.5, steppe_runner/marsh_grazer 1.5/1.0 (pastoral only — pen inert), deer/mammoth omit both (wild → ×1). Applied at the one K seam `ecological_carrying_capacity` via `fauna::herd_density_gain`, resolved live by display name (`FaunaConfig::pen_density_for`/`pastoral_density_for`), *not* cached on `Herd`. **Playtest dials.** Validated finite & `>= 1.0` (a gain below 1 would make domestication reduce capacity). See "The husbandry yield ladder") + **`requires_adjacent_water`** (the **shore predicate**, default **`false`** so every other species is byte-identical — a species that sets it may only spawn on a land tile that **borders open water** on one of its six hex sides (`fauna::has_adjacent_water`), the site rule filtering the short-range spawn's candidate list *before* the pick. Shipped on **`seal`** only, which pairs it with `host_biomes: ["boreal_arctic", "coastal_littoral"]` — **the cold half comes from `host_biomes`, NOT from a climate gate**: `climate::climate_band_for_temperature` is the single climate authority and a second one here would be a parallel authority that drifts from it. It **READS** the coastline geometry the worldgen stamped and never edits terrain. **Validated: `migratory: true` + this flag is REJECTED** — the migratory placement path (`suitable_tiles_for`/`build_migratory_route`) does not apply site rules, so the combination would be *silently ignored*; the unhandled state is made unrepresentable and loud instead. Measured on 6 seeds of the standard map: seals **2 → 14 colonies over the sweep** (0–1 → 0–4 per map), against 44–94 water-adjacent `boreal_arctic` tiles per map — see `core_sim/tests/fauna_coastal_habitat.rs`. **The seal pairs it with `route_len: [1, 1]`, and that is load-bearing, not incidental:** the site rule filters *placement* only — nothing in it stops `advance_herds` walking a colony inland on turn 1, and with the shipped `[1, 2]` it did (measured: a colony drifted `(24,21) → (23,22)`). A single anchor **is** the spawn tile, so `step_index` cycles `(0+1)%1 = 0` and `step_herd_toward` is handed the herd's own position — the colony is a fixed **haul-out**, which is what makes the shore invariant *structural* rather than placement-time. A rookery is a site the animals swim out from, not a herd that wanders overland. **Do not restore a multi-anchor route to a species carrying a site rule** without making roam site-aware, or the rule silently degrades to placement-only)) + per-biome spawn abundance + `hunt` / `follow` / `ecology` (regrowth + depensation collapse thresholds) / `immigration` (respawn) / `husbandry` (**the flow-based yield ladder**: **per-species managed `r`** (Grazing 2d — `pastoral_gain` 2.0 / `pen_gain` 4.0 scale each species' own wild `r`, capped at `husbandry_regrowth_cap` 1.0, retiring the flat `pastoral.ecology.r` 0.25 / `pen.ecology.r` 0.90 which now carry phase bands only) and `pen` (**`upkeep_per_biomass`** — the pen's feed, now footprint-offset — / `starve_shrink_rate`; `capacity_fraction` is **deleted** — a penned herd's `K` is its fenced-footprint graze flow), the **`Corral` policy**'s investment levers having **moved to `intensification_ladder.json`'s `animal:pen` rung** (the old `corralling_yield_fraction` → `yield_fraction_while_building` 0.50, `corral_build_progress_per_turn` → `progress_per_turn` 0.04); every rung pays MSY against its own ecology, see "The husbandry yield ladder" / "Phase 2d") / `market` (commercial-hunt take + trade multiplier) tuning + **`graze`** (the pasture layer, Grazing Phase 2a — `capacity_by_biome` a **total** per-biome table (one row per `TerrainType`), `ecology` (`regrowth_rate` **0.40**, the fastest vegetal stock in the model), `reseed_floor_fraction` 0.02, **`overgraze_escapement_fraction` 0.25** (Grazing 2b-ii — grazing can't draw a patch below this, the constant-escapement floor that keeps the herd↔graze loop convergent); see "The Graze (Pasture) Layer" / "Phase 2b-ii"). **Validated** — `FaunaConfig::validate()` runs inside `from_json_str` (every load path), rejecting a pen that eats more than it yields, an inverted ladder, a dead ecology, or a **partial / all-zero / negative graze table** (a missing biome would silently read as an invisible zero-graze dead zone); a broken invariant is logged at **error** level (`fauna_config.invalid_rejected`) and the builtin is used |
| `src/data/beat_definitions.json` | **The Telling** beat catalog — the narrative *content* layer and the mod surface (see "The Telling"). One entry per beat: `id`, `tier` (`ambient`\|`beat`\|`fork`), `soul`, `when` (the predicate grammar), `nouns` (slot → resolver, with `fallback`), `wardrobe` (per-register templates + `fit` + `stance_affinity`), **`choices`** (fork-tier only — per-register `label`/`echo`, `writes.stance`/`writes.flags`, optional `rearm_after_turns`), `gloss`, **`remembers`** (slot → memory-thread kind — see "Memory threads"), `cooldown_turns`, `once`. Loader `telling/catalog.rs`, env override `BEAT_DEFINITIONS_PATH`. **Validated** — `BeatCatalog::validate()` runs inside `from_json_str` (every load path), and is refused at **error** level (`beat_catalog.invalid_rejected`) in favour of the builtin |
| `src/data/beat_config.json` | **The Telling** tunables: `budget` (`max_per_turn` / `global_cooldown_turns`, per tier), **`fork_expire_turns`** (30 — the safety valve, see "The fork tier")), `selection` (`novelty_window_turns`, `novelty_floor`, `fit_soft_tag_weight`, `stance_affinity_weight`, **`stance_affinity_floor`** (0.1 — the re-colouring term's positive floor), `min_selection_weight`), `trend` (`max_history_turns` **16** — must exceed every authored `trend` window, validated at catalog load, `min_delta`), `voice` (`default_register`, `registers`, **`mediums`** — the ordered oral→painted→written ladder, see "The maturing voice"), **`memory.max_threads_per_kind`** (8), `stance.axes` (id → backing signal + range; ids must be unique, and a backing signal may not itself be a `stance.*` signal). Loader `telling/config.rs`, env override `BEAT_CONFIG_PATH`. **Validated** inside `from_json_str`; refused at **error** level (`beat_config.invalid_rejected`) |
| `src/data/sedentarization_config.json` | Sedentarization Score tuning: soft/hard prompt thresholds, EMA `smoothing`, input `weights` (domestication/surplus/resource_density/population), and saturation `references` |
| `src/data/demographics_config.json` | Demographic population tuning: `initial_distribution` (children/working/elders split), `consumption` (per-capita food draw + per-bracket factors), `startup` (`food_reserve_days` seeded into each band's larder + `well_fed_morale_bonus`), `births` (rate/surplus_bonus; morale-independent), `maturation_rate`/`aging_rate`/`elder_mortality_rate`, `scarcity` (starvation + per-bracket vulnerability, deficit-capped), `cold` (temperature-death) |
| `src/data/supply_network_config.json` | Supply-network tuning: `reach_tiles` (connection radius), `throughput_per_turn` (max goods moved per node/turn), `friction` (fraction lost in transit), `min_transfer` (dead-band) |
| `src/data/wellbeing_config.json` | Civilization Wellbeing tuning: `discontent` (`content_morale`/`floor_morale` productivity curve, `grievance_gain`/`grievance_decay`/`trapped_multiplier`), `productivity` (`floor_mult`, `discontent_weight`), `migration` (own morale-scaled onset: `morale_threshold`, `max_rate`, `base_reach`, `attractive_morale`, `min_morale_gap`, `dependent_weight`) |
| `src/data/sites_config.json` | Wondrous Sites catalog (`catalog`: per-`site_id` `category`/`display_name`/`glyph`/`placement_rule`/`discovery_reward.morale_bonus`) + `placement` rules (per-rule `max_sites`, `min_spacing`, and the union of rule inputs: `min_relief`, `max_habitability_pressure`, `min_food_weight`). Loader `sites_config.rs`, env override `SITES_CONFIG_PATH`. Not wired into the `reload_config` hot-reload path (mirrors `fauna_config.json`) |
| `src/data/expedition_config.json` | Expedition tuning. Scout: `max_party_size`, `comm_range_tiles` (discovery-report range), `comm_range_tech_factor` (stubbed 1.0 tech hook), `observe_sight_range` (per-turn LOS radius, matches band base sight), `provision_draw_per_worker_per_tile` (launch larder draw = party × distance × this), `provision_upkeep_per_worker` (per-turn drain = party × this, scouts only). Hunt (PR 2) `hunt` block: `per_worker_carry` (carry cap = party × this), `reach_tiles` (how close to the herd to take), `drop_off_within_tiles` (herd-near-band delivery gate), `min_deliver_fraction` (herd-near-band early delivery needs carried ≥ this × cap), `viability_warn_turns` (**20** — a client display threshold on `turnsToFill`; = 4× the throughput-implied trip length `per_worker_carry / (per_worker_biomass_capacity × provisions_per_biomass)` = 5 turns), `forecast_horizon_turns` (**60** — how far `hunt_trip_forecast` simulates the raid before giving up on completion; a raid is short — grab the surplus, come home — so simulating each to completion is cheap). The retired `sustain_floor_fraction` is **gone**: a hunting expedition is a **greedy raid** — it grabs the herd's standing surplus above the policy's floor (Sustain `K/2`, Surplus `hunt.surplus_escapement_fraction·K`, Market `ecology.collapse_fraction·K`, Eradicate 0), *not* the resident band's throttled kill-credit rate. See "Scouting & Hunting Expeditions". The take **policy** is **not** a config lever — it is chosen at launch via the optional trailing arg of `send_hunt_expedition` (default `FollowPolicy::Sustain`). Scout replenish `replenish` block: `low_turns` (top up below party × upkeep × this), `reach_tiles`. Loader `expedition_config.rs`, env override `EXPEDITION_CONFIG_PATH`. Not on the `reload_config` hot-reload path (mirrors `sites_config.json`). **Validated** — `ExpeditionConfig::validate()` runs inside `from_json_str`, so *every* load path (builtin, default file, `EXPEDITION_CONFIG_PATH` override) is covered, following the `crisis_config.rs` convention; a broken invariant is logged at **error** level (`expedition_config.invalid_rejected`) and the config is refused, falling back to the known-good builtin rather than silently disabling a feature. Enforced: `max_party_size ≥ 1`, `comm_range_tech_factor` finite & `> 0`, `observe_sight_range ≥ 1`, `provision_draw_per_worker_per_tile`/`provision_upkeep_per_worker` finite & `≥ 0`, `hunt.per_worker_carry` finite & `> 0`, `hunt.reach_tiles ≥ 1`, `0 < hunt.min_deliver_fraction ≤ 1`, `hunt.viability_warn_turns ≥ 1`, **`hunt.forecast_horizon_turns ≥ max(1, hunt.viability_warn_turns)`** (at `0` the forecast's `1..=horizon` loop runs zero turns and *every* hunting expedition silently reports "won't fill"; below the warn threshold, a trip the player would be told is viable can never be discovered), `replenish.low_turns ≥ 1`, `replenish.reach_tiles ≥ 1`. Deliberately **left free**: `comm_range_tiles` (`0` = "walk back into camp to report"), `hunt.drop_off_within_tiles` (`0` = no early drop-off; a full pack still delivers), and the *upper* end of `max_party_size`/`forecast_horizon_turns` (they only cost snapshot time — the estimate table is `O(policies × max_party_size × horizon)` per herd — an operator's call, not an invariant) |

Hot reload: `reload_config [path]` or `reload_config turn|overlay|crisis_archetypes|crisis_modifiers|visibility [path]`

### Environment Overrides

| Var | Effect |
|-----|--------|
| `SIM_CONFIG_PATH` | Load an alternate `simulation_config.json` instead of the baked-in default. |
| `SIM_PORT_BASE` | Shift all four TCP listen ports to a fresh block so multiple checkouts/worktrees don't collide. The base maps to `snapshot=base+0`, `command=base+1`, `snapshot_flat=base+2`, `log=base+3`; `base=41000` reproduces the historical fixed ports (41000–41003). Applied in `load_simulation_config_from_env` (`resources.rs`) over whatever the config JSON specifies, preserving each bind's host. A non-numeric or out-of-range value (needs `1 ≤ base` and `base+3 ≤ 65535`) is warned and ignored rather than fatal. `scripts/run_stack.sh` derives a per-checkout base automatically and forwards the matching `STREAM_PORT`/`COMMAND_PORT`/`LOG_PORT` to the Godot client; `cargo xtask command …` still defaults to `127.0.0.1:41001`, so pass `--port <base+1>` when targeting a shifted server. **Setting this var also makes the base *explicit*, which disables the auto-bump** (see "Port block allocation" below). |
| `SIM_PORTS_FILE` | Full path (not a directory) of the ports handshake file, overriding the per-user default below. Used by tests and by any launcher that wants the handshake somewhere specific. |

Each `*_CONFIG_PATH` var in the tables above overrides its specific config file; those are noted per-row.

### Port block allocation & the ports handshake file

The server binds **all four ports as one block, up front, all-or-nothing** (`port_alloc.rs`,
`port_alloc::allocate`), and hands the already-bound `TcpListener`s to `start_snapshot_server` /
`start_log_stream_server` / `spawn_command_listener`. Previously each subsystem bound its own socket
and failed differently — the command listener **panicked** while snapshot/log streaming merely warned
and disabled themselves, so a conflict on 41000 or 41002 left a *running* server that silently never
streamed. **There is no longer any path where the server runs with a socket disabled because it was in
use.**

- **Allocation policy.** If `SIM_PORT_BASE` was set, the base is honoured **exactly** — a conflict is
  fatal (exit code `2`, with an actionable message), never bumped, because `scripts/run_stack.sh` and
  the per-worktree port assignment depend on an explicit base being deterministic. Otherwise the
  server starts at the configured base (the config's `snapshot_bind` port, default 41000) and, on
  `AddrInUse`, advances by `PORT_BLOCK_STRIDE` (**10**) for up to `PORT_SLOT_COUNT` (**100**) slots —
  the same two constants `scripts/run_stack.sh` uses. Only `AddrInUse` bumps; any other IO error (e.g.
  permission) surfaces immediately. Exhausting all 100 slots is fatal. A bump is logged at **WARN**
  (`port_block.bumped`) and the `server ready` INFO line reports the *actual* bound ports plus
  `port_base_bumped`.
- **The ports handshake file** lets the client discover a bumped block. Path resolution, env-derived
  so it needs no extra crate: `SIM_PORTS_FILE` verbatim if set; else Windows
  `%LOCALAPPDATA%\ShadowScale\ports.json`, macOS `$HOME/Library/Application
  Support/ShadowScale/ports.json`, Linux `$XDG_STATE_HOME/ShadowScale/ports.json` (falling back to
  `$HOME/.local/state/…`). Deliberately **not** the temp dir, where AV heuristics are most aggressive;
  parent dirs are created as needed. Contents (exact key names — a contract with the Godot client's
  reader):

  ```json
  {"host":"127.0.0.1","snapshot":41000,"command":41001,"snapshot_flat":41002,"log":41003,"pid":1234}
  ```

  Written after the block is bound and before the main loop, overwriting unconditionally. **Failure to
  write is never fatal** — it logs a warning and continues (only auto-discovery is lost). A
  `PortsFileGuard` removes it when `main` returns; a file left behind by a crash or a **signal**
  (SIGINT/SIGTERM skip `Drop`) is expected and tolerated — the client validates the file and falls back
  to the default block, which is what the recorded `pid` is for. No liveness machinery lives here.
- **Config hot-reload** re-applies the **resolved** base (the `ResolvedPortBase` resource in
  `server.rs`), not the configured one, so a reload of an unchanged file after a bump keeps the live
  binds and doesn't spuriously trip `socket_changed=restart_required`. Rebinding live sockets is out of
  scope; the reloaded config describes the ports the server actually holds.

---

## World Generation Pipeline

Implements the procedural map pipeline producing terrain, coasts, rivers/lakes, climate bands, resources, and wildlife spawners. Player-facing framing: manual §3a World Bootstrapping, §3b Terrain Palette.

> ### Elevation is the sole authority
>
> **The land mask is a pure derived function of the heightfield — `land[i] = elevation[i] > sea_level`
> — and is never stored and edited. Any stage that wants to move a coastline writes elevation and
> re-derives.** Guarded by `core_sim/tests/elevation_authority.rs`.
>
> This is not style, it is the fix for a real defect: the mask used to be grown as a boolean blob and
> then repainted by later stages, so the published bathymetry and the published terrain disagreed —
> 543 water tiles sat *above* sea level and 218 land tiles *below* it on a sampled map. A water tile
> above sea level is now **unrepresentable** rather than merely rare, because no stage has a way to
> express one. `target_land_pct` is met by *shaping the field* (`anchor_contour_to_sea_level`), and
> `continents` by the continental bias term — never by repainting tiles.
>
> Consequences a new stage must respect: `place_islands` raises a seamount above sea level;
> `connect_inland_seas_via_straits` lowers a corridor below it; both then re-derive. There is no
> `rebalance_land_ratio` and no tag-solver water branch — both were deleted because they corrected a
> quota by repainting the output. Design: `docs/plan_elevation_authority.md`.

### Pipeline Stages
1. **Macro landmask** - `land[i] = elevation[i] > sea_level`, a pure threshold of the heightfield (`generate_land_mask`, `mapgen.rs`). `target_land_pct` is satisfied upstream by `anchor_contour_to_sea_level` putting that quantile exactly on `sea_level`; `continents` is satisfied by the continental bias term in `build_elevation_field`. **No BFS, no seeds, no area quotas, no jitter** — the pre-`elevation-authority` mask grew weighted-BFS blobs from spaced seeds to fixed per-continent area targets, which is what decoupled terrain from elevation (see the callout above).
2. **Tectonics** - Drift vectors, collision belts, fault seams, volcanic arcs, dome plateaus → mountain mask
3. **Polar microplates** - Subdivide polar tiles, converging vectors raise fold strength
4. **Heightfield** - Multi-octave height raster with erosion smoothing → `elevation_m`
5. **Coastal smoothing** - Blend shoreline tiles via 3×3 blur
6. **Ocean/coasts** - Distance-transform bands: Shelf → Slope → Deep Ocean; inland seas. See "Continental shelf width" below — the shelf is a continuous ≥1-tile ring off gentle coasts, gated to deep water at steep/cliff coasts. A **final reconciliation post-pass** (`reconcile_coastal_shelf`, Startup chain after hydrology + tag solver + palette clamp) restamps the shelf so no Deep Ocean touches gentle land on the *final* map, covering coasts created later by deltas/marshes/solver tundra.
7. **Climate** - Temperature (latitude base − elevation lapse + jitter) is computed **first** and the biome band is derived from it via `climate::climate_band_for_temperature` — see "Temperature is the climate authority" below. Latitude is an input to temperature, never a parallel biome gate.
8. **Hydrology** - Rivers on hex **edges** + navigable rivers as water **hexes**. See "Rivers" below. `RiverDelta` is stamped **only here**, at the last **gentle-coast** land hex of each river that ends in a standing water body — the ocean *or* an inland sea/lake (lacustrine deltas). The mouth hex must border that water; the biome picker and tag solver never create deltas (those would scatter them with no river attached). Delta tiles are protected from the tag solver's **reduction *and* addition** passes so genuine river mouths survive — every branch that would restamp a tile carries a `terrain != RiverDelta` guard. This includes the **Fertile-add** branch (both its primary candidate filter and its fallback loop): a delta cut through a **polar/non-fertile** biome lacks the `Fertile` tag, so it is not caught by the Fertile/Water skips and was the one path that clobbered a real mouth back to `AlluvialPlain` (orphaning its `river_channel` bit on dry land). Guarded by `core_sim/tests/navigable_mouth_delta.rs` — the invariant *no hex carries a `river_channel` bit while rendering non-`NavigableRiver`/non-`RiverDelta` terrain*, run through the **real** Startup chain (hydrology → tag solver → palette clamp → reconcile) via `build_headless_app`, so a later-pass clobber cannot hide the way it does in the hydrology-last `hydrology_earthlike.rs` harness.
9. **Biomes** - Stamp `TerrainType` via `terrain_for_position` with micro-variant jitters
10. **Moisture transport** - Humidity blending with wind-driven rain-shadow pass
11. **Resources** - Surface deposits biased by `TerrainDefinition.resource_bias`
12. **Wildlife** - Seed herd spawners, migratory paths, `game_density` raster
13. **Starting areas** - Place candidates respecting World Viability Contract

### Data Shapes
- **Rasters**: `elevation_m: i16`, `climate_band: u8`, `game_density: u8` (the square-8 hex `flow_dir` / `flow_accum` rasters are **deleted** — hydrology routes on the corner graph, see "Rivers")
- **Vectors**: `rivers: [RiverSegment]` — per-edge `RiverEdge { hex, dir, class, discharge: f32 }` chains + a navigable hex tail (see "Rivers")
- **Tiles**: `hydrology_id`, `substrate_material`, `terrain_type`, `TerrainTags`, `river_edges: u16`

### Rivers — a real drainage network on hex EDGES, with a class that grows downstream (`hydrology.rs`)

A river is **not** a polyline through hex centers. Minor/Major rivers run **along hex edges** (so a
future movement system can charge a crossing penalty on exactly the side the river is on), and a
river that outgrows the edge model becomes **water terrain**.

The **routing and extraction** are a real drainage network: steepest descent on a depression-filled,
precipitation-weighted elevation surface, decomposed into main stems and tributaries. Designs:
`docs/plan_rivers.md` (the edge/class/navigable *model*) and
`docs/plan_rivers_drainage_network.md` (the *network* that model expresses).

- **The corner graph.** The dual of "flow along edges" is "route between corners": every
  corner→corner step traverses exactly one hex edge. On a pointy-top odd-r grid each corner is
  shared by exactly 3 hexes, so `V = 6F/3 = 2F` — **two corners per hex**, indexed `(hex_x, hex_y,
  slot)` with `slot ∈ {TOP, BOTTOM}`. Each corner has 3 neighbour corners. A **border corner** (its
  3 hexes are not all on the map) is excluded from routing. Every hex step goes through
  `grid_utils::hex_neighbor`, so horizontal wrap is honored. Corner **elevation is the mean** of its
  3 hexes (not the min — the mean puts a corner low in the *trough* between two low hexes, so rivers
  settle into valleys) **plus a deterministic flat-tie jitter** (below). A corner is a **sink** iff
  any of its 3 hexes is an **OCEAN** hex (`WATER` *without* `FRESHWATER`) — see "Lakes flow through".
- **Canonical edges.** An edge `(H, d)` has two representations — `(H, d)` and `(neighbor,
  opposite(d))`. The canonical one is whichever has `dir ∈ {E, SE, SW}` (`canonical_edge`), so an
  edge has a single key regardless of which hex traced it. An edge exists only if **both** its hexes
  are on the map.
- **The flow field descends the LANDSCAPE, not a cost-to-sea distance transform** (`docs/plan_rivers_drainage_network.md`).
  1. **Jittered elevation.** Corner elevation gets `river_flat_jitter × (hash01(world_seed, corner) − 0.5)`
     — a pure splitmix64 hash, no RNG, no `HashMap`. Pure steepest descent on a plateau picks the same
     direction for every corner and carves artificial parallel channels; the jitter breaks those ties
     into a natural branching pattern, reproducibly. It is `≫ river_fill_epsilon` and `≪` real relief,
     so it decides only ties the terrain does not.
  2. **Priority-flood depression fill** (Barnes + epsilon): seed a min-heap with every sink at its own
     elevation and raise each neighbour to `max(elev[n], filled[popped] + river_fill_epsilon)`. Every
     non-sink corner ends **strictly above** the corner that flooded it, so a **strict descent to a
     sink always exists** — including across the flats of a filled depression, where a naive fill
     stalls. Unreachable corners keep `filled = INFINITY`.
  3. **Downstream = steepest descent on `filled`.** All 3 corner steps are the same length on a regular
     lattice, so "steepest" is simply "lowest filled neighbour"; ties break by corner index ascending.
  4. **Precipitation-weighted accumulation.** Each corner seeds
     `(river_base_runoff + river_moisture_weight × precip) / 2`, where `precip` is the mean of its 3
     hexes' `MoistureRaster` value. Dividing by the 2 corners-per-hex makes **discharge read directly
     as precipitation-weighted upstream drainage area, in HEX-EQUIVALENTS** — a fully-wet hex
     contributes exactly `1.0`. That is the unit the class thresholds live in, which is why they are
     **absolute and map-size independent**. A missing/mis-sized `MoistureRaster` falls back to uniform
     `precip = 1.0` with a warning (never a panic).
- **Extraction: main-stem decomposition, not N independent rivers.** `channel_min =
  river_channel_min_discharge / river_density`; a corner is a **channel** iff it is routable, not a
  sink, and `accumulation ≥ channel_min`. Accumulation is monotone non-decreasing downstream, so the
  channel corners + their descent links form a **forest of trees rooted at outlets, by construction** —
  nothing to reject, space, or count-target. Each outlet (largest first) is then walked **upstream**,
  always taking the largest unclaimed contributor: that path is the classic **main stem** ("the
  Missouri joins the Mississippi"), and every contributor it passes over becomes a tributary stem
  joining at exactly the corner it was passed over at. Every channel corner lands in exactly one river.
  - *Upstream-from-the-outlet, not downstream-from-headwaters*: every headwater's accumulation is
    barely above `channel_min` (nothing upstream of it is a channel), so "the biggest headwater" does
    **not** identify the main stem — but "always take the biggest contributor, walking up from the
    mouth" does, by definition.
  - A stem's final edge (`last corner → terminus`) is what makes a main stem **touch the shore** (the
    terminus is the ocean-touching sink corner) and a tributary **land on its trunk** (the terminus is
    a claimed corner of the parent stem). One uniform rule, no special case.
  - **Strahler order is computed on the real channel tree** (a channel corner with no channel
    contributors is order 1; otherwise `max(contributor orders)`, +1 iff ≥2 share that max) — where it
    is actually defined. The old per-tile computation on the hex flow field is gone.
  - `river_min_length` (in hexes) is the **only** noise gate left: an emitted river shorter than it is
    dropped. There is no spacing, no count target, no source category, and no acceptance loop.
- **Lakes FLOW THROUGH — only the ocean is a sink.** A lake / `InlandSea` corner is an ordinary low
  corner: the fill raises it to its lowest saddle and it **spills**, so the whole upstream catchment
  carries *through* the lake and out a genuine outlet. Real outlet rivers, and a big river below a big
  lake, fall out for free (replacing the old `lake_heads` hack). Two consequences:
  - **A river ENDS at standing water and CONNECTS to it; a new river begins where terrain drains out.**
    The run emits the **first water-touching edge as the mouth** (the connecting edge that reaches the
    water) and terminates there; the *rest* of the consecutive water-touching edges (the shore-hug + the
    submerged stretch) are **skipped, not drawn**, and a new run resumes at the next dry edge. So there
    is exactly **one water-touching edge per river and it is the LAST one** — the river runs *into* the
    lake/sea/trunk and stops rather than hugging the shore, and the drain-out below re-emerges as its own
    segment (connected on its source side, its first corner being water-adjacent). "Standing water" is a
    lake / inland sea / ocean on the terrain map **or** a previously-stamped navigable trunk
    (`StemEmitter::edge_touches_water`, reading `is_water_hex` + `existing_navigable`). The original
    both-banks rule hugged the lakeshore ("V" up a trunk hex); the first fix over-corrected and *dropped*
    the water-touching edge, leaving a visible **gap one step short of the water** — the current rule
    draws the mouth and skips only the shore-hug. The accumulation still flows through underneath
    (discharge/class unchanged), so the outlet stays a big river below a big lake and can independently go
    navigable again below it — **only the rendered segmentation changes.** The split is also required
    because a segment's edge chain and navigable chain are both *paths* — a chain with a water-shaped hole
    in it would be neither contiguous nor drawable. Guarded by
    `hydrology_earthlike::edge_rivers_terminate_at_water_not_along_it` (a river has **at most one**
    terrain-water-touching edge and it is the **last** — the mouth — so no river runs along a shore; the
    navigable-trunk "V" and the shore-hug tile proxy are tracked by the `drainage_census`).
  - **A navigable river must CONNECT to water, or it isn't navigable.** After the split a navigable chain
    must end at the water it connects to (its last hex is standing water, or hex-adjacent to it —
    `StemEmitter::navigable_reaches_water`). A chain that **dead-ends on dry land** (an endorheic run with
    no ocean) is **demoted to the river's edge (Major) form** — re-traced with the navigable model off,
    so the river survives on the edge model rather than stranding a landlocked navigable dead-end. A
    navigable run shorter than **`river_navigable_min_hexes`** (a 1- or 2-hex puddle) is demoted the same
    way. Both demotions run in `StemEmitter::emit_run`; guarded by
    `hydrology_earthlike::navigable_rivers_connect_to_water` (every navigable run reaches standing water
    and is ≥ the lever, swept over `CENSUS_SEEDS`). Aggregate over the 6-seed sweep: **14 navigable
    segments / 68 hexes, min run 3, max run 22, 0 landlocked, all mouth-connected** (the `drainage_census`
    now reports the landlocked count, the run histogram, and the mouth-connection count).
  - **Deltas are PER-TRANSITION, not per-terminus.** A river now both *enters* a standing water body
    and *leaves* it, so the delta scan stamps a delta at **every land→standing-water transition** along
    the river's ordered hex path (plus the mouth, where the path simply ends against the water) — each
    still **gentle-coast gated** and still required to actually border that water. A lacustrine delta
    and the ocean delta are different tiles on the same river. A delta may never take a **mid-chain**
    navigable hex (the channel flows through it; turning it into depositional land would break the
    chain in two).
- **Class is PER-EDGE and grows downstream.** `RiverEdge.discharge` = the corner accumulation at the
  edge's **upstream** corner, which is monotonically non-decreasing downstream — so a river is
  `Minor` at its headwater and `Major` in its lower course, never uniformly wide. `RiverClass`
  (`sim_runtime`) is `None = 0 | Minor = 1 | Major = 2`; **value 3 is reserved** — "navigable" is
  deliberately *not* a class (see below).
- **Navigable rivers are WATER TERRAIN, not edges.** Once discharge crosses
  `river_class_navigable_min_discharge` the river stops emitting edges: the lower **dry** of the two
  hexes flanking the **last emitted edge** becomes the first hex of a `TerrainType::NavigableRiver`
  chain, and the rest of the chain is read straight off the river's **own corner path** — the hex the
  channel is inside at each remaining step (`RiverSegment.navigable_hexes`). Consecutive steps share a
  corner and the three hexes at a corner are pairwise adjacent, so the chain is **contiguous by
  construction**. Two rules keep it a *simple path*: **sticky** (while the current hex still flanks the
  edge being crossed, the river has not left it) and **no self-crossing** (a channel that would double
  back onto a hex it already occupies ends there — a corner path never revisits a corner, but a *hex*
  is touched by many corners, so the hex path can). A giant river is
  a body of water you need a boat to enter, so it reuses every existing water mechanic.
  `NavigableRiver` mirrors `InlandSea` exactly (`WATER | FRESHWATER`, same movement/logistics/
  attrition profile), is in the biome palette's `must_have` set, and is protected from the tag
  solver's water-reduction pass — like `RiverDelta`, otherwise the solver would erase real rivers.
  - **A navigable hex is a valley with a river in it — it keeps the biome it cut.** The stamp
    (`hydrology.rs`) captures the pre-stamp biome into `Tile::underlying_terrain: Option<TerrainType>`
    *before* overwriting `terrain`/`terrain_tags` with `NavigableRiver`, so the tile stays
    **mechanically** water (movement/naval/logistics/attrition/tags/palette all keep keying on
    `terrain == NavigableRiver`, untouched) but its **RESOURCE** reads route through
    `Tile::resource_terrain()` (= `underlying_terrain` on a navigable hex, `terrain` everywhere else).
    So a giant river yields the valley it runs through, not open water: **forage** = the underlying
    biome's `forage.capacity_by_biome` **plus `forage.navigable_river_forage_bonus`** (default **80.0**,
    `labor_config.json` — a navigable river is always a fishery, so a navigable hex *always* seeds a
    forage patch, even over an otherwise-barren biome, at just the bonus there); **graze** = the plain
    underlying `graze.capacity_by_biome` (no bonus — you don't pasture on the channel; a navigable-over-
    grassland hex grazes like grassland). One shared helper `forage::tile_forage_capacity` sizes the
    seeded patch AND the wire's `forageCapacity`, so they can't drift. The `NavigableRiver` rows in
    `labor_config.json` (forage 130) and `fauna_config.json` (graze 0) are now **vestigial** (bypassed
    by the underlying-terrain routing; left only to keep the tables total). Exported as
    `TileState.underlyingTerrain:TerrainType` (append-only, = `resource_terrain()`, so it is the "real
    ground" biome always; the client consults it only when `terrain == NavigableRiver`).
  - **The join invariant: the edge chain and the hex chain share an EDGE, never a bare corner.**
    The hand-off anchors on the last **emitted** edge, *not* on the un-emitted edge whose discharge
    crossed the threshold. Both are incident to the same corner and **three hexes meet at a corner**,
    so anchoring on the un-emitted one could pick the third hex — one the edge chain never touches.
    The chains then met only at a point, the first navigable hex carried **no `river_edges` bits at
    all**, and a tributary visibly dead-ended at the trunk in the client. Anchoring on the last
    emitted edge makes the shared edge true by construction, so the first navigable hex always
    carries that edge's class in its mask. Guarded by
    `hydrology_earthlike::navigable_chain_joins_the_edge_chain_on_a_shared_edge` (asserts the shared
    edge *and* the resulting tile mask across a 6-seed sweep) and the
    `the_navigable_handoff_anchors_on_the_last_emitted_edge` unit test. A river that goes navigable
    on its very first step has emitted nothing to anchor to, so it falls back to the edge it stopped
    at.
  - `hex_contiguous_chain` survives as a belt-and-braces bridge (a waterway whose hexes don't touch is
    not a waterway), but the corner-path construction above already makes it an identity.
  - **Rivers MERGE ON CONTACT — a navigable river is a path, not a blob** (`truncate_at_existing_channel`).
    Stems are emitted **main-stem-first**, so a tributary that reaches its trunk finds it already
    stamped and **joins** it rather than digging a second channel alongside it: the first hex that is
    an already-stamped chain's hex **or adjacent to one** terminates the chain **on that trunk hex**
    (contact is adjacency, not identity — two water hexes that touch are one body of water). The
    confluence is a genuine shared chain hex, so both chains' `river_channel` bits meet there.
    (Historically the un-concentrated flow accumulation made *several branches of one drainage* cross
    the navigable threshold independently and each trace its own chain to the same sink, packing into a
    2–4 hex wide **blob**; with a real drainage tree the branches now merge *upstream* of the threshold
    in the first place, and merge-on-contact is the backstop.)
  - **The path invariant is asserted on the CHANNEL-EXIT MASK, not on terrain adjacency**
    (`hydrology_earthlike::navigable_rivers_are_paths_not_blobs`, swept over `CENSUS_SEEDS` +
    `BLOB_REGRESSION_SEED`): a mid-chain hex links to exactly **2** channel neighbours, an endpoint to
    **1**, a confluence to **3**; 4+ is a 2D water body. *Terrain* adjacency cannot express this — a hex
    chain that turns 60° puts hex `k` adjacent to hex `k+2` (the three hexes at a bend are mutually
    adjacent, unavoidably), so a bending chain with a tributary merging at the bend **touches** 4
    navigable hexes while remaining a perfectly good path. Terrain adjacency is still bounded, at the
    geometric ceiling a chain can reach (2 chain links + one bend skip-adjacency + one merging
    tributary = 4).
  - The chain's **mouth is a `RiverDelta`**, not open water — a river deposits its load where it
    meets the sea — so the delta contract is unchanged.
- **The gameplay primitive: `Tile.river_edges: u16`** — 2 bits per odd-r direction
  (`class = (river_edges >> (2 * dir)) & 0b11`), populated for **both** hexes flanking every river
  edge, so a hex and its neighbour always agree about the river between them. Helpers:
  `river_class_on_side(dir)` / `set_river_class_on_side(dir, class)` / `has_any_river_edge()`. This
  is what a movement system will read: *entering hex H across direction d crosses
  `H.river_class_on_side(d)`*. **Nothing consumes it yet — that is expected**; movement and fertility
  effects are a follow-up. Exported on the wire as `TileState.riverEdges:ushort`.
- **Where the tributary meets the trunk: `Tile.river_inflow: u16`** — the same 2-bits-per-slot
  packing as `river_edges`, but keyed by hex **CORNER** instead of by side. An edge river runs
  *along* a side, corner to corner, so it does not end mid-edge — **it ends at a vertex**, and that
  vertex is where the water enters the navigable hex. The edge mask cannot say where: a trunk hex
  can flank three river edges (the tributary ran along three of its sides before going navigable),
  which leaves two candidate chain-ends, so the client would be guessing and would draw an arm per
  edge. So the sim states it.
  - **Corner index convention (a wire contract).** Corner `i` is the vertex at screen angle
    `60*i + 30`, **+y down** — matching the client's `MapView._hex_points`: `0` lower-right,
    `1` bottom, `2` lower-left, `3` upper-left, `4` top, `5` upper-right. Mapped onto the sim's
    `(hex, TOP|BOTTOM)` corner model by `HEX_CORNER_LAYOUT` /
    `HexGrid::local_corner_index(hex, corner)` (`hydrology.rs`): `0 = TOP(SE(H))`, `1 = BOTTOM(H)`,
    `2 = TOP(SW(H))`, `3 = BOTTOM(NW(H))`, `4 = TOP(H)`, `5 = BOTTOM(NE(H))`. Side `dir` spans
    corners `{dir - 1, dir}` (`grid_utils::hex_edge_corner_indices`).
  - **Both tables are pinned ABSOLUTELY to the client's geometry, not merely to themselves.**
    `local_corner_index_is_a_bijection_on_every_hex` / `hex_edge_corner_indices_match_the_corner_model`
    only prove *internal consistency* (six distinct corners that round-trip) — **a table rotated by one
    position passes both happily** while putting every tributary on the wrong vertex. So
    `hex_corner_layout_matches_the_clients_corner_geometry` and
    `hex_edge_corner_indices_are_the_shared_edges_endpoints` (`hydrology.rs` tests) compute each
    corner's **world position** twice — once through the sim's `(hex, TOP|BOTTOM)` model (centre at
    `x = √3·R·(col + 0.5·(row&1))`, `y = 1.5·R·row`; `TOP = centre + (0,−R)`, `BOTTOM = centre +
    (0,+R)`, +y down) and once through the client's `corner i at angle 60i + 30` circle — and assert
    the two land on the same point. That is what makes the convention a *contract* rather than a
    convention.
  - **The semantics WIDENED with the drainage network** (`docs/plan_rivers_drainage_network.md` §A).
    `river_inflow` no longer means *"this hex is a navigable chain HEAD"* — it means **"a tributary
    hands over to the channel at this vertex."** Same field, same bits, same corner convention, same
    widest-wins rule; only the *meaning* widened. Two hand-overs are recorded:
    1. a river that **outgrows the edge model itself** hands over at the head of its own navigable
       chain (the old case), and
    2. an **edge-only tributary that lands on a navigable trunk** hands over at a vertex of that
       **trunk hex — mid-chain**. That is impossible without a real network (before it, tributaries
       could only meet a trunk at its head), and it is *the* payoff: without recording it, the
       tributary's edge band ends at a bare vertex while the trunk's arms only reach its edge
       *midpoints*, and the tributary visibly dead-ends short of the water it feeds.
    Both carry the class of the **last emitted edge** (the tributary's own width where it arrives). A
    river navigable from its first step emitted no edges, has no tributary, and reports `0` — no
    invented inflow. `RiverInflow` now carries the target `hex` alongside the `corner`/`class`.
  - **The render contract: `river_channel` is load-bearing for the head/mid-chain distinction.**
    The client cannot key its head-taper off `inflow != 0` any more — that was safe only while inflow
    *meant* "chain head". It now **popcounts the `river_channel` exit bits**: **1 exit = a genuine
    chain head** (taper the channel to a point), **≥ 2 = mid-chain** (full width — no hourglass at a
    tributary junction), **3 = a confluence**. The inflow spur is drawn unconditionally. So the
    channel mask is no longer only anti-web link topology: **the sim must keep its exit count exactly
    equal to the chain's real degree at every navigable hex**, or the trunk pinches or bulges in the
    render. Both halves are landed and verified (client: `terrain_blend.gdshader` + the
    `map_rivers_midchain` ui_preview fixture).
  - **Widest-wins on collision.** Three hexes meet at a corner, so two tributaries running down
    either bank can hand over at the *same* vertex of the same hex (a confluence at a corner). One
    slot holds one class, so `widen_tile_river_class` keeps the wider (`Major` > `Minor`), which is
    also emission-order independent.
  - Helpers: `river_class_at_corner(corner)` / `set_river_class_at_corner(corner, class)` /
    `has_any_river_inflow()`. Exported as `TileState.riverInflow:ushort`. Guarded by
    `hydrology_earthlike::every_river_inflow_is_a_real_tributary_handover_vertex` — the tile's inflow
    corners are exactly the hand-overs arriving there, at the widest arriving class, each an endpoint
    of its river's last emitted edge (checked by the **hex triple** that identifies the vertex, so a
    wrong corner cannot pass), and **mid-chain hand-overs must exist** (if none happen, the network is
    still a set of parallel rivers).
- **The trunk channel is a PATH: `Tile.river_channel: u8`** — **1 bit per odd-r direction**
  (`exits(dir) = (river_channel >> dir) & 1`, `RiverChannel::{BITS_PER_DIR, SLOT_MASK}` in
  `sim_schema`): does this hex's navigable channel flow out through side `dir`? Helpers:
  `channel_exits(dir)` / `set_channel_exit(dir)` / `has_any_channel_exit()`.
  - **Why it must exist.** A navigable river is a chain of water *hexes*, and a chain is a **path** —
    a hex links to its upstream and downstream neighbours and to nothing else. **Terrain cannot say
    which those are.** The client used to arm an arm from each navigable hex's centre to *every*
    neighbour that was navigable/water/`RiverDelta`, so wherever two chains ran adjacent (which,
    before merge-on-contact, was everywhere) or a chain doubled back, every hex cross-linked to every
    navigable neighbour and the trunk rendered as a **web with triangular holes** instead of a river.
    Only the tracer knows chain membership, so the sim states it. (Merge-on-contact removes most
    adjacent chains, but the mask is still the right primitive: two *legitimate* parallel rivers, or a
    bending chain, would cross-link without it.)
  - **Populated from each `RiverSegment.navigable_hexes` chain** in `generate_hydrology`, in two
    passes so the result is independent of trace order. **Pass 1 — the chain:** for each consecutive
    pair, the exit bit is set on **both** hexes facing each other (hex `A` → dir toward `B`, hex `B` →
    the opposite dir), symmetric exactly like `river_edges`. **Pass 2 — the mouth:** a chain's final
    hex also exits toward the water it drains into (the ocean, an inland sea, or the `RiverDelta` at
    its own mouth), or the drawn river would stop one hex short of the sea. That mouth bit is the one
    **asymmetric** bit in the mask — open water carries no channel of its own, so it is not mirrored
    back. Only a genuine **dead end** earns it: a tributary that merged into a trunk also *ends* on its
    last hex, but that hex is a confluence the water already flows on through, and a second exit there
    would draw a spurious arm off the side of the trunk ("has no exit but the one back upstream" is
    the test, and it does not depend on segment order).
  - The **head** needs no exit toward its tributary — the inflow SPUR (`river_inflow`) already draws
    that; double-encoding it would put two arms on one vertex. A hex on two chains (a confluence)
    accumulates the **union** of the bits (OR-ed, never overwritten).
  - Exported as `TileState.riverChannel:ubyte`. Guarded by
    `hydrology_earthlike::navigable_channel_exits_are_the_chain_and_only_the_chain`: symmetry,
    end-to-end chain connectivity, every chain reaching its water, and the **anti-web invariant** — *no
    navigable hex exits toward a navigable hex that no chain actually runs between*.
- **Wire format.** The `HydrologyOverlay` / `RiverSegment` / `HydrologyPoint` polyline tables are
  **deleted** from the snapshot and delta. The per-tile `riverEdges` + `riverInflow` + `riverChannel`
  masks plus the `NavigableRiver` terrain fully determine the render, so a parallel polyline overlay
  would be duplicated state. The client draws the trunk channel from **`riverChannel`** (arming *only*
  the sides whose bit is set — never inferring links from terrain), the edge rivers from `riverEdges`,
  and joins a tributary to its trunk hex at the `riverInflow` **corner** — never at a side midpoint,
  and never one arm per flanked edge.
- **Delta placement is gentle-coast gated.** A delta is a depositional fan, so it only forms where
  the river meets the water across low ground — reusing the shelf's own
  `ShelfConfig.coast_height_threshold` rather than inventing a second threshold. A river that meets
  the sea at a cliff has no delta (it is an estuary). This also keeps `reconcile_coastal_shelf`'s
  "no DeepOcean touches gentle land" invariant coherent: every delta is gentle land, so every delta
  gets a shelf seaward of it.
- **Config** (`hydrology` block of `simulation_config.json` → `HydrologyOverrides`, overriding the
  per-preset `river_*` keys in `map_presets.json` — overrides > preset > default):

  | Key | Default | Meaning |
  |---|---|---|
  | `river_density` | 1.0 | How wet the map reads. A **multiplier on the channel threshold**: `effective = river_channel_min_discharge / river_density` (higher density → lower threshold → more channels). Clamped to `[0.1, 5.0]`. |
  | `river_fill_epsilon` | 1e-5 | The depression fill's drainage gradient across flats. Far above `f32` noise at map elevations (~1e-7), far below the jitter. |
  | `river_flat_jitter` | 5e-4 | Elevation tie-break amplitude. **Must stay `≫ river_fill_epsilon`** (so it decides ties the fill cannot) **and `≪` real relief** (so it can never reorder genuine terrain). |
  | `river_base_runoff` | 0.2 | Per-hex runoff floor, so an arid basin still trickles. |
  | `river_moisture_weight` | 0.8 | How hard rainfall drives discharge. With `base_runoff = 0.2` a fully-wet hex contributes exactly **1.0** — which is what makes discharge read as hex-equivalents. |
  | `river_channel_min_discharge` | **3.0** | The network-extraction threshold. |
  | `river_class_major_min_discharge` | **12.0** | Minor → Major. |
  | `river_class_navigable_min_discharge` | **25.0** | Major → `NavigableRiver` hex chain. |
  | `river_navigable_enabled` | true | Kill switch for the navigable tail. |
  | `river_navigable_min_hexes` (`navigable_min_hexes` in the override block) | **3** | Shortest navigable hex chain that still reads as a river; a shorter run is demoted to the edge (`Major`) form (a 1–2 hex navigable is a puddle). |
  | `river_min_length` (`min_length` in the override block) | 2 hexes | The **only** noise gate. Keep it low. |

  **The three discharge thresholds are `f32` and ABSOLUTE.** Discharge means *precipitation-weighted
  upstream drainage area in hex-equivalents*, so a river draining 300 wet hex-equivalents is a big
  river on an 80×52 map and on a 256×192 map alike; a bigger map simply has more of them and longer
  ones. Do **not** re-express them as a fraction of the map maximum — one giant basin would skew it.

  **Determinism** is guarded by `integration_tests/tests/determinism.rs`: no `HashMap`/`HashSet`
  iteration order in the routing or extraction, no unseeded RNG, every sort has an explicit index
  tie-break, and the flat jitter is a pure hash of `(world_seed, corner_index)`.

  **The three discharge thresholds were tuned from a 45-cell sweep**, not guessed:
  `hydrology_earthlike::drainage_threshold_sweep` (`#[ignore]`d) crosses
  `channel × major × navigable` over `CENSUS_SEEDS` and reports rivers/edges/class-split/navigable
  runs per cell. Re-run the sweep before changing any of the three. **They were NOT re-tuned for the
  erosion pass** (below) — they were deliberately held fixed so the erosion A/B is attributable.

  **Measured** shape at those thresholds, on the **eroded** landscape
  (`hydrology_earthlike::drainage_census`, `#[ignore]`d; run with `-- --ignored --nocapture`),
  aggregate over 6 seeds of an 80×52 earthlike map (after the "connect to the mouth + demote landlocked/
  puddle navigable" fix): **14.5 rivers per map**, 81.1% Minor / 18.9% Major, **~2.3 navigable segments
  / ~11 navigable hexes per map** (14 segments / 68 hexes over the 6-seed sweep, min run 3, 0 landlocked
  — the shore-hugging false chains, the landlocked dead-ends, and the 1–2 hex puddles are all gone);
  land-corner accumulation p50 = 0.60 / p95 = 10.2 / p99 = 64.4 / **max 587**; corner confluences
  **11.6%** of land corners (4.1% before the drainage-network rewrite); Strahler on the drainage tree
  o1 = 12366, o2 = 2246, o3 = 837, o4 = 254, o5 = 34 (the accumulation/confluence/Strahler figures read
  off the corner network, which the segmentation fix does not touch). Per-seed spread is large and
  *should* be — see the verdict below.

  > **These figures are PRE-`elevation-authority` and no longer describe an 80×52 map.** Post-arc,
  > that size yields **zero navigable rivers on every seed**, and land-corner accumulation maxes at
  > **10–20**, not 587. This is **not a regression** — measured, the largest basin is ~5% of its
  > landmass both before and after the arc (the drainage surface is ~95% divided into small
  > independent basins either way). What changed is *landmass size*: the old BFS grew an accidental
  > ~1,580-tile supercontinent at 80×52 while the preset asked for 4 continents, and that surplus area
  > was the only thing clearing the navigable discharge threshold of 25.0. It cleared it as a
  > **lottery** — pre-arc counts across six seeds were `0, 1, 1, 6, 5, 1`, with one 41.7%-basin
  > outlier seed carrying most of them. The arc removed the bug that was masking a pre-existing
  > drainage deficiency.
  >
  > **Update (the divides arc).** The dome has been replaced by a warped / tilted / ridged envelope
  > (see `macro_land` below). At 80×52 navigable rivers now appear on **3 of 6 seeds** rather than 1,
  > and the standard map carries **49 sowable tiles** — but the **coherence ratio is unchanged**, and
  > the measured reason is geometric, not tuning: with a mean depth-to-coast of ~2.9 tiles the largest
  > landmass has roughly one ocean-touching (⇒ sink) corner per two interior corners, so a basin
  > cannot grow long enough to clear a discharge of 25 except by luck. **Landmass area remains the
  > binding constraint at this grid size.**
  >
  > **A related correction:** the claim that the arc took sowable tiles "46 → 0" was a **test-harness
  > defect, not a worldgen result**. `core_sim/tests/forage_field.rs` never ran `generate_hydrology`,
  > so its map had no rivers, no `RiverDelta`, and no `river_edges` — and `plant:field`'s site rule
  > requires fresh water, which on that map nothing could satisfy. The harness now runs hydrology and
  > pins its own grid; see the test split below.
  >
  > Consequently the navigable structural invariants run against a **river-capable fixture** at
  > `NAVIGABLE_FIXTURE_GRID` = **128×96** (shipped presets, `continents: 4`, only the grid differs) —
  > the smallest grid producing navigable rivers on every seed (5–9 each). A sweep over
  > 80×52 / 128×96 / 192×128 / 256×192 × `continents` 4 / 2 / 1 showed `continents` barely moves the
  > result, confirming **landmass area** is the binding constraint. See `hydrology_earthlike.rs`.
  >
  > **Do not "fix" a dry map by lowering `river_class_navigable_min_discharge`** or any hydrology
  > threshold. Rivers are emergent; forcing a fixed river share onto whatever terrain exists is the
  > repaint-to-hit-a-quota pattern `elevation-authority` deleted. The input to change is basin
  > coherence or landmass size — `TASKS.md` → "Capture: the divides, not the valleys".

### Fluvial erosion — the heightfield the drainage runs on
The drainage-network rewrite left the *router* correct and the *landscape* wrong: continents were
**sponges** (48–64% of a continent's tiles touched water, because the coastline is an iso-contour of
fractal noise) and they **shed radially** with no trunk valleys to capture drainage across a divide.
`heightfield::apply_fluvial_erosion` attacks the landscape directly, at the end of
`build_elevation_field` — **before** `mapgen::generate_land_mask`, which is the whole point: the mask
is a pure threshold of this field, so the coastline **is** a level set of it, and reshaping the field
reshapes the coast.

> Since `elevation-authority` this is *literally* true rather than approximately so. The passage
> above used to read "the mask **ranks** tiles by elevation" — it ranked them by
> `elevation + macro_land.jitter × noise`, which is a **reordering**, so the coastline was a rank
> contour over a jittered score and not a level set of anything. The conclusion held only by luck.
> `jitter` is retired; its coastline raggedness now lives in the field itself as
> `macro_land.coastline_roughness`, applied *before* `land_contour`, where it perturbs the shoreline
> without decoupling the mask from the surface.

- **The model** is the classic landscape-evolution equation minus uplift: `∂z/∂t = D∇²z − K·A^m·S^n`,
  iterated on the **square raster** (D8 — the hex/corner graph is hydrology's and stays there). Per
  pass: priority-flood the depressions (+`fill_epsilon`), route D8 steepest descent on the *filled*
  surface, accumulate **uniform** unit drainage (this is landscape evolution, *not* the
  precipitation-weighted discharge model), incise, then diffuse. Deterministic: pure arithmetic, no
  RNG, explicit index tie-breaks on every sort and every descent comparison.
- **Both terms are needed, and they do different jobs** (measured, not assumed): **stream power**
  carves the trunk valleys that give a continent *capture* but leaves the coastline noise untouched
  (it is concentrated where `A` is large, which is nowhere near a headwater coast); **diffusion** is
  what planes that noise off and *de-sponges*. Incision alone moved coastal 59.2% → 57.5%; with
  diffusion it reaches **52.8%**.

> #### Two things the pass had to learn the hard way — do not "simplify" them away
>
> **1. Base level is `land_contour`, which the anchor then makes equal to `sea_level`.**
> `apply_fluvial_erosion` still takes its base level from `heightfield::land_contour` (the
> `1 − target_land_pct` quantile) rather than from `sea_level`, and **that ordering still matters**:
> erosion runs *before* `anchor_contour_to_sea_level`, so at the moment it runs the two are not yet
> the same number.
>
> *Historical note — do not restore the old reasoning.* This note used to say base level is the
> "land-mask's **rank** contour, NOT `sea_level`", justified by only **24–37%** of cells sitting above
> `sea_level = 0.62` while the mask claimed **38%** for land, putting the coastline at **0.55–0.61,
> *below* sea level**. That gap was a symptom of the jittered-rank mask, and it is **gone**: realized
> land is now **37.7–37.8%** against a 0.38 target, and after the anchor the contour and `sea_level`
> are identical by construction. The warning the note was protecting is still live — a pass that
> freezes everything under a *wrong* base level freezes the coastal band it exists to reshape and
> measures as a no-op (it did: coastal 59.2% → 58.8%) — but the specific 24–37%-vs-38% discrepancy no
> longer describes this pipeline.
>
> **2. A valley incised *to* base level DROWNS.** Now direct rather than indirect: the mask is
> `elevation > sea_level`, so a trunk cut below the contour simply **is** water on the next derive —
> a sea inlet that takes its basin with it (measured pre-arc: seed 4's biggest basin collapsed
> **546 → 99**). `incision_floor` exists to bound this; it ships at **0.0** because measurement said
> the drowned stretches read as *estuaries* and leave the coast **smoother** — but the lever is there,
> and the failure mode is real.
>
> **3. `anchor_contour_to_sea_level` is what lets the carving reach hydrology at all.**
> `restamp_elevation`'s lowland branch is only order-preserving *above* sea level; below it,
> `((v − sea_level)/(1 − sea_level)).clamp(0,1)` is an **order-destroying clamp** that plates every
> such cell — **a third of all land** — flat onto exactly `sea_level`. Carving valleys there is
> pointless: they are erased before hydrology sees them. So the pass finishes with a strictly
> monotone, piecewise-linear rescale that puts the coastline exactly on `sea_level`, making the
> pipeline's "land ⟺ above sea level" assumption *true*. Monotone ⇒ it cannot reorder the field, so
> the land mask still picks the same tiles.
>
> Since `elevation-authority` the anchor is **load-bearing rather than merely helpful**: it is the
> only thing that makes `target_land_pct` come out right, because nothing downstream repaints the
> mask to hit the target any more. It is also the reason the invariant is exact — the mask thresholds
> the very surface the anchor just aligned. Its own justification finally holds too: monotonicity
> guarantees the mask is unchanged *only* if the mask ranks on elevation, which before this arc it
> did not.

**Config** — the `erosion` block of each preset in `map_presets.json` (`ErosionConfig`):

| Key | Default | Meaning |
|---|---|---|
| `enabled` | true | Kill switch. `false` reproduces the pre-erosion maps **exactly**, and is the A/B control the census measures against. |
| `iterations` | 40 | Passes. Past ~40 the sponge stops improving and the big basins start planing away. |
| `erodibility` | 0.1 | Stream-power `K`. Below ~0.05 nothing carves; above ~0.3 incision **saturates** against the downstream clamp (the result stops depending on `K` at all) and the coast gets *worse*. |
| `area_exponent` | 0.5 | `m` — classic. |
| `slope_exponent` | 1.0 | `n` — classic. |
| `timestep` | 0.1 | `Δt`. Only `K·Δt` matters; split for readability. |
| `min_slope` | 1e-4 | Slope floor, so a filled flat still incises and can cut itself an outlet. |
| `fill_epsilon` | 1e-6 | The priority-flood's gradient across a filled flat. |
| `diffusivity` | 1.0 | Hillslope `D`. **The term that de-sponges.** Past ~2 it planes real relief off the continent. |
| `incision_floor` | 0.0 | How far above base level a valley may cut, as a fraction of the land band. See note 2. |
| `anchor_contour_to_sea_level` | true | See note 3. |

**Measured A/B** (`hydrology_earthlike::drainage_census`, `#[ignore]`d, 6 seeds, 80×52, shipped
river thresholds held at 3.0/12.0/25.0 so the comparison is clean). **Measured PRE-`elevation-authority`**
— the erosion-OFF-vs-ON comparison is still valid (both arms moved together), but the absolute
navigable counts belong to the pre-arc landmass and are ~0 at this size today; see the note above:

| metric | erosion OFF | erosion ON |
|---|---|---|
| coastal tiles of the largest landmass (**SPONGE** — must fall) | **59.2%** (spread 14.3) | **52.8%** (spread **9.6**) |
| biggest basin / largest landmass (**CAPTURE** — must rise) | 11.0% (spread 39.5) | 13.3% (spread 34.1) |
| navigable rivers (post "end-at-water" fix) | 21 segments / 67 hexes / **max run 7** | 21 / 75 / **max run 21** |

> **Honest verdict: one of the two failures is fixed, the other is only dented.** The **sponge is
> genuinely better** — every seed improves and the spread halves — and the **~13-hex navigable
> ceiling is gone** (longest river 7 → **21** hexes post the "end-at-water" fix; the ceiling was never
> the threshold, it was the landscape). **Capture is not fixed.** The mean barely moves and the spread stays huge: seed 5 goes
> 4.7% → 21.0% and seeds 1/3 roughly double (2.2 → 4.2, 3.5 → 5.2), but seeds 1/3/TEST are still
> single-digit while seed 4 still runs at 38%. **Incision deepens the valleys a continent already
> has; it does not move its divides.** The divides come from the continent-scale fbm, so the next
> lever is the *noise*, not the erosion — see `TASKS.md` → "Capture: the divides, not the valleys".
>
> **`elevation-authority` added `continental_weight` / `continental_radius`, and they do NOT fix
> capture.** They make `continents` a real lever for the first time (the old BFS grew a single
> accidental supercontinent at 80×52 while the preset asked for 4), but the bias is a **radial
> falloff — dome-shaped by construction**, so it moves landmasses apart without giving any one of them
> an internal divide structure. A dome sheds radially; that is exactly the "sheds radially with no
> trunk valleys" failure this section opens with, just at continent scale. Measured after the arc: the
> largest basin still tops out at **~5% of its landmass**, statistically unchanged pre/post. Capture
> needs a term that shapes *divides* — anisotropic/warped noise, tectonic uplift fields — not a
> smoother continent outline. Design context: `docs/plan_elevation_authority.md`.
>
> `apply_coastal_smoothing` was **measured, not assumed** (the suspicion was that its 3×3 blur would
> soften the incised valleys right where they matter). It does not blunt the result: the sponge metric
> is **bit-identical** with the blur zeroed (the land mask is decided from the base field *before*
> `restamp_elevation` ever runs), and zeroing it actually made rivers **worse** (max navigable run
> 25 → 15). Leave it alone.

### Tile Temperature — latitude + elevation climate model
`Tile.temperature` is a real climate, **not** the old `(x+y)%4` element checkerboard. The single
source is `systems::climate_temperature(y, grid_height, above_sea_normalized, element, &ClimateConfig)`:

```
temperature = latitude_base(y, H) − elevation_lapse(elev) + element_jitter(element)
```

- **`latitude_base`** — equator-in-the-**middle**: `lat_frac = |y − (H−1)/2| / ((H−1)/2)` ∈ [0,1]
  (0 = center/equator, 1 = top *or* bottom edge/pole), `equator_temp − lat_frac·(equator_temp −
  polar_temp)`. Symmetric: the top and bottom edges are equally cold; the temperate band (~18°)
  lands at mid-latitudes (lat_frac ≈ 0.34).
- **`elevation_lapse`** — `ElevationField::above_sea_normalized` (height above sea remapped to [0,1])
  × `elevation_lapse_span`; higher ground is colder.
- **`element_jitter`** — the element's `thermal_bias` × `element_jitter_scale`, kept small (~±1.5°)
  so it is local texture, not the driver.

Config lives in the `climate` block of `simulation_config.json` (`equator_temp` 30.0, `polar_temp`
-5.0, `elevation_lapse_span` 12.0, `element_jitter_scale` 0.25). Worldgen seeds each tile at exactly
this value **after** elevation exists (a `climate_elevation` field with sea level attached), and
`simulate_materials` relaxes each turn toward the *same* recomputed climate temperature (no longer
the element target), so turn 1 has no jump. On an 80×52 map: equator ≈ 29–30°, mid-latitude ≈ 18°,
pole = −5° at sea level (mountains up to 12° colder).

### Temperature is the climate authority — the biome band is derived from it
Since the **climate-authority arc** (`docs/plan_climate_authority.md`), a biome's climate
eligibility is a function of the tile's **temperature**, never its latitude. Temperature is now
computed in the *first* worldgen loop (before the biome is assigned, `systems/worldgen.rs`), and one
seam — **`climate::climate_band_for_temperature(temp, &ClimateConfig) -> ClimateBand`** — maps it to
a four-rung ladder (**polar ≤ 0° / boreal ≤ 3° / temperate ≤ 18° / tropical**, cut points in the
`climate` block: `polar_max_temp` / `boreal_max_temp` / `temperate_max_temp`). `ClimateBand`'s
`admits_cold_biomes()` (polar **or** boreal) is THE predicate every cold-biome gate reads; **no call
site may re-derive a band or compare a raw temperature to a literal.** The gate reads the **jittered**
temperature deliberately, so band boundaries come out ragged rather than as clean horizontal lines
(design §8.2 — the lever if it is ever too noisy is `climate.element_jitter_scale`, never re-gating on
an un-jittered temperature).

- **What this retired.** The old `terrain_classifier.polar_latitude_cutoff` (0.35) and
  `high_latitude_threshold` (0.15) fields, `systems/mod.rs`'s `POLAR_LATITUDE_THRESHOLD` (which read
  the *default* preset, a latent desync bug), and `worldgen.rs`'s `climate_band_for_position` (a
  third arithmetic copy with a bare `0.18` literal) are all **gone**. The six former latitude-gate
  sites — the base classifier's cold ladder, the mountain glaciation branch, the two palette remaps
  (prototype-loop + post-solver `apply_biome_palette_clamp`), `bias_terrain_for_preset`, and the tag
  solver's wetland/fertile/polar passes — now all read the temperature band.
- **The boreal band is its own rung, not a wider polar.** Polar tiles get the ice ladder
  (Tundra/PeriglacialSteppe/SeasonalSnowfield), boreal tiles get the taiga ladder
  (BorealTaiga/MixedWoodland/PeatHeath). A single polar cut point could not express the boreal-fringe
  incoherence (measured: BorealTaiga was 1,601 of 4,397 warm-polar tiles), which is why the ladder has
  four rungs (§8.1).
- **The tag solver has a climate veto** (§5.4): its polar family pass may only paint a cold biome on a
  tile whose band admits one; where the `Polar` tag target cannot be met without violating climate it
  **under-fills and logs** (`mapgen.tag_solver.under_filled_climate_gated`) rather than repainting —
  the repaint-to-hit-a-quota pattern this repo has rejected before. This closed 64% of the warm-polar
  tiles (the fallback loop had **no** climate test at all).
- **Alpine tundra is now expressible** (§5.3): a cold mid-latitude highland (a mountain at −1.6°)
  glaciates to Glacier/SeasonalSnowfield because the mountain branch reads the band, not the row.
  Measured ~10% of land, up from ~0.
- **`boreal_max_temp` == the client's retired `cool_min` (3.0)** — one boundary, stated once (§5.2/§8.3).
  The sim owns the cut points and **publishes** them in the snapshot (`MapSection.climateBands`, the
  `ClimateBands` table — the same way `seaLevel` rides the elevation overlay) so the client renders the
  band it is told rather than keeping an independent opinion. **Client half (a separate task): consume
  `climateBands`, drop the local `tile_climate_config.json` `cool_min`, and render `Climate:` off the
  published bands.**
- **Secondary fixes that rode along** (§7): `PeatHeath` is now `POLAR`-tagged (it is the cold wetland
  — the only WETLAND+POLAR biome, and the classifier/solver/palette already treated it as such; only
  its tag disagreed); `RiverDelta` now takes its own definition's tags wholesale in `hydrology.rs`
  (it used to OR only WETLAND|FRESHWATER and *keep the underlying biome's tags*, leaking `POLAR`
  through a delta cut through Tundra into `BiomePalette::remap`, `food.rs` and the tag census).
- **Measured before/after** (`core_sim/tests/climate_authority.rs`, ≥5 seeds × 2 grids × both presets,
  run the `#[ignore]`d `climate_band_report` for the full tables): cold-but-temperate **6.9% → 0.16%**
  of land, warm-polar **7.9% → 0.00%** — both directions collapsed, neither traded for the other. Land
  share per band (aggregate): polar 19.5% / boreal 8.7% / temperate 48.1% / tropical 23.7%. The
  worldgen *tectonic* regression baselines (`mapgen.rs` fold/fault/dome counts, land ratio) are
  **unchanged** — this arc touches only which biome a land tile wears, not the elevation/mountain masks.

### Map Presets (`map_presets.json`)
Presets control: `seed_policy`, `dimensions`, `sea_level`, `continent_scale`, `mountain_scale`, `moisture_scale`, `river_density`, `terrain_tag_targets`, `locked_terrain_tags`, `biome_weights`.

**`macro_land` — landmass shape** (`MacroLandConfig`, `map_preset.rs`). Since `elevation-authority`
every one of these is honored by *shaping the heightfield*, never by editing the mask:

| Key | earthlike | Meaning |
|---|---|---|
| `target_land_pct` | 0.38 | Land fraction. Delivered by `anchor_contour_to_sea_level` putting this quantile exactly on `sea_level`; realized **37.7–37.8%** pre-island with nothing correcting it downstream. |
| `continents` | 4 | Number of continental bias centres, chosen deterministically from the world seed with Poisson-ish spacing (wrap-aware in x). Realized landmasses ≥`min_area`: 3–5. |
| `min_area` | 256 | The landmass size that counts as a continent when auditing the above. |
| `continental_weight` | 0.5 | Amplitude of the low-frequency continental bias added before erosion. `0.0` reproduces the pure fractal field — which thresholds into **one dominant supercontinent**, which is why the term exists. |
| `continental_radius` | 0.35 | A continent's radius of influence, as a fraction of the **smaller** grid dimension. Beyond it the bias saturates at its minimum, which is what actively sinks inter-continental gaps rather than merely making them less high. |
| `continental_falloff_exponent` | 1.5 | Shape of the falloff, `bias = 1 − 2·t^exponent` over `t = dist/radius`, taken as a **max over centres, not a sum** (summing fuses adjacent centres into a land bridge). |
| `continental_warp_amplitude` | 0.18 | **Domain warp** — how far the envelope's sample coordinates are displaced by low-frequency noise before the envelope is evaluated, as a fraction of the **smaller** grid dimension. Makes a continent lobed rather than circular. `0.0` restores a perfectly radial envelope. |
| `continental_warp_frequency` | 1.6 | Cycles of warp noise across the map. Low by design: the warp reshapes *landmasses*; fraying the *coastline* is `coastline_roughness`'s job. |
| `continental_tilt_strength` | **0.0 (off)** | **Per-continent tilt** — a directional gradient across each centre, its heading hashed per centre from the world seed, windowed by `1 − t^4` so it vanishes at the rim. A dome sheds water in every direction; a *tilted* surface drains one way. Ships as a **tilted trough**, not a tilted plane: `heightfield::CONTINENT_TROUGH_GAIN` (0.5) lifts the ground away from the drainage axis, because a bare tilt gives **parallel** flow (many short rivers) rather than convergence onto a trunk. **Both presets ship it at `0.0`**: at `2.0` it buys one extra seed-with-a-river in six but fuses continents into a supercontinent, collapsing `polar_contrast`'s fold belts by 85% (see the note below). The machinery is retained, live and inert at zero — raising it is how you get the drainage back, at that cost. |
| `continental_spine_amplitude` | 0.35 | **Ridged spine** — ridged noise gated to the continent interiors (`clamp(bias, 0, 1)`), so a landmass carries an internal **divide** with two drainage sides instead of one summit. Also the term that keeps mountain ranges narrow (see below). |
| `continental_spine_frequency` | 2.2 | Cycles of spine noise across the map — roughly how many range-scale divides a continent can carry. |
| `coastline_roughness` | 0.05 | High-frequency shoreline raggedness, applied to the field **before** `land_contour`. Replaces the retired `macro_land.jitter`, which perturbed the mask's *ranking* instead of the field and thereby decoupled the two. |

> `macro_land.jitter` **no longer exists** and must not be reintroduced — it is the specific lever
> that broke "land ⟺ above sea level". Its intent lives on as `coastline_roughness`.
>
> **The bias is no longer purely radial.** `continental_weight`/`continental_radius`/
> `continental_falloff_exponent` are now only the *base envelope*; the warp, tilt and spine terms above
> shape divides and drainage direction on top of it. **The tilt ships at `0.0`** — the lever is live
> and fully wired, but both shipped presets set it to zero; see the two findings below.
>
> Measured over 6 seeds at 80×52 (`core_sim/tests/relief_sweep.rs`, `-- --ignored --nocapture`),
> dome → warp+spine (**the shipped configuration**): **sowable ground on the standard map 35 → 49
> tiles**, navigable rivers present on **1/6 → 2/6 seeds** (3 segments total), max drainage
> accumulation mean **25.4 → 28.4**, land fraction unchanged (0.387–0.400 → 0.386–0.397), landmasses
> ≥ `min_area` 2.8 → 2.2 per map. Adding the tilt on top buys one further seed with a river
> (3/6, 4 segments) and costs what the second finding below describes.
>
> **What it did NOT fix, and the trade-off it carries — read before retuning:**
> - **Basin coherence is still ~0.02–0.08** (max accumulation ÷ largest landmass), statistically where
>   the dome left it. Measured root cause: a corner is a sink iff any of its 3 hexes is ocean, and the
>   largest landmass has a **mean depth-to-coast of only ~2.9 tiles** with ~360 coastal (⇒ sink) corners
>   against ~800 land corners. Flow terminates within ~3 steps whatever the relief looks like, so at
>   80×52 with `continents: 4` a discharge of 25 is *geometrically marginal* — not a tuning failure.
>   Ruled out by direct measurement: `river_flat_jitter` (5e-4 → 1e-6 moves nothing) and
>   `continental_weight` (0.5 → 2.0 improves compactness but *lowers* max accumulation).
> - **NONE of these terms controls mountain-range WIDTH — the earlier claim that the tilt widens
>   ranges was a metric error.** It rested on mean alpine connected-component **area**, which does not
>   measure width (a long thin cordillera and a fat blob can share an area) and whose per-seed value
>   swings up to **27×** on one configuration — noise, read as signal off a single seed. Measuring
>   thickness directly (every alpine tile's hex distance to the nearest non-alpine tile,
>   `relief_sweep::alpine_thickness`, 6 seeds at 384×288) gives **mean 2.22 / 2.32 / 2.26 / 2.28 /
>   2.43 and p95 4.3–5.0 for dome / warp-only / spine-only / tilt-on / warp+spine** — flat. If ranges
>   read as too wide, the lever is downstream in the mountain mask (`derive_mountain_mask`'s
>   `belt_width_tiles` dilation, `apply_belt_relief`, `terrain_classifier.alpine_relief_threshold`),
>   **not** in the continental envelope. Do not re-derive a width claim from component area.
> - **The tilt FUSES CONTINENTS, and that is why it ships at `0.0`.** On `polar_contrast` it collapsed
>   the five multi-plate land components into **two**, the largest going 9,053 → **18,313 tiles — 85%
>   of all land in one body**. Fold belts form only between plates *within* a component and plate
>   count is area-bucketed with a **cap of 4**, so fusing the map into one supercontinent starves the
>   plate-boundary network: `polar_contrast` fold count fell **3556 → 544 (−85%)**. With the tilt off
>   it recovers to **3004**. This is the same land-bridging failure `CONTINENT_TILT_WINDOW_EXPONENT`
>   was introduced to prevent — the window mitigates it but does not eliminate it on this preset.
>   Measurement: `mapgen::tests::polar_contrast_fold_investigation`.

**Coastline-editing levers** — the two stages permitted to move a coastline, both of which write
elevation and re-derive:

| Key | Block | Default | Meaning |
|---|---|---|---|
| `island_peak_margin` | `islands` | 0.06 | How far above `sea_level` an island's peak is raised. `place_islands` raises a radial dome and the mask is re-derived; this margin is what makes the dome *become* land. Placement (`continental_density`, `oceanic_density`, `min_distance_from_continent`) is unchanged. |
| `strait_depth_margin` | `inland_sea` | 0.02 | How far **below** `sea_level` a strait corridor is cut by `connect_inland_seas_via_straits`. Deep enough to read as water on the re-derive, shallow enough to read as a channel and not a trench. |

The active preset's `sea_level` is carried on the `ElevationField` resource, attached at the field's origin in `build_elevation_field` and propagated through `restamp_elevation` (`heightfield.rs` / `mapgen.rs`; falls back to `DEFAULT_SEA_LEVEL` = 0.6 only when no preset resolves — which also logs a `warn`, because a preset-less field skips erosion and the contour anchor entirely). It is exported in the snapshot as `ElevationOverlay.seaLevel` — **normalized to the overlay's [minValue, maxValue] sample scale AND quantized onto the same u16 lattice as the samples** (`snapshot/map.rs` `elevation_overlay_from_field`, `ELEVATION_SAMPLE_SCALE`) so the Godot client can compare it directly against decoded samples for its relative-height / LOS readout.

> **Samples and the published `sea_level` must share one quantization lattice.** The client decodes
> `sample / 65535` and compares against `seaLevel`; publishing the threshold *unquantized* made every
> tile sitting exactly at sea level decode to `0.6200046 > 0.62` and read as land-height water — 42 of
> them in a live export, all with the identical raw sample `40632 = round(0.62 × 65535)`. Do not
> reintroduce a second `65535.0` literal. Guarded by
> `elevation_authority::the_published_sea_level_lies_on_the_sample_quantization_lattice`, which
> asserts on the **encoded overlay** rather than the in-process `ElevationField` — the earlier test
> read the f32 field, reported 0 violations, and missed all 42.

**Continental shelf width** (`classify_bands` + `effective_shelf_width`, `mapgen.rs`; `ShelfConfig`, `map_preset.rs`): `ContinentalShelf` is the ocean band within a computed distance of the coast (slope collapses to `DeepOcean` downstream, so only the shelf boundary affects ocean composition). The model mirrors real margins — a **continuous ≥1-tile shelf off gentle (passive-margin) coasts, and deep water right at steep/cliff (active-margin) coasts** — via two knobs on top of the width scaling:
- `min_width_tiles` (default **1.0**) — floors the computed width so a qualifying coast gets a *continuous* ≥1-tile ring instead of a sub-tile sparse fringe. Applied after the `width_frac`/`width_exp` (or `width_tiles`) computation, so a preset that bumps `width_frac` still scales the shelf wider than the floor on big maps.
- `coast_height_threshold` (default **0.10**, earthlike **0.10**) — the coast-height gate. A shelf-candidate ocean tile becomes `ContinentalShelf` only if the coast land it abuts rises *gently*: the MIN normalized rise (`elevation.sample − sea_level`) over its immediately-adjacent land tiles is **below** this. Cliff/mountain/highland coasts (rise ≥ threshold) instead show `ContinentalSlope`→deep water at the edge. On earthlike, lowland coasts rise into the compressed band `[sea_level, elevation_base]` (≤ ~0.10) while mountain-mask coasts jump to ≥ ~0.16, so the threshold sits in the bimodal gap and cleanly splits gentle vs. steep. This self-limits the shelf %: steep coasts add zero shelf, so the 1-tile floor doesn't blow the fraction up on small maps the way a blanket ring would.

  **The immediate coastal ring is HEX-aware (odd-r 6-neighbour).** The default 1-tile shelf ring's coast-adjacency uses the authoritative odd-r hex neighbours (`grid_utils::hex_neighbors_wrapped`, wrap-aware — the same adjacency gameplay + the client render), not 4-connected square neighbours. An ocean tile joins the ring iff it is hex-adjacent to ≥1 Land tile **and** the min rise over its Land hex-neighbours is `< coast_height_threshold`. This closes the old hex-diagonal gaps: the 4-cardinal set covers only two (E/W) of the six hex directions, so before the fix a gentle coast could sit directly against DeepOcean on a hex-diagonal (`min_adjacent_coast_rise` + `classify_bands`, `mapgen.rs`). The broader worldgen distance transforms (ocean-distance, mountain masks, rivers) remain **square-grid** — pre-existing modeling, out of scope; only the immediate shelf ring is hex-exact (a full hex distance-transform for `width_frac`-widened shelves, `full > 1`, is the follow-up). Guarded by `mapgen::tests::earthlike_bands_have_no_gentle_coast_shelf_gap` (0 DeepOcean-vs-gentle-Land hex adjacencies over real earthlike coastlines) + `classify_bands_shelf_covers_hex_diagonal_coast`.

  **Final reconciliation pass — the shelf is hex-exact on the *final* map, not just at band time.** `classify_bands` decides the shelf early (stage 6), but later Startup stages repaint terrain near the coast *after* the shelf exists: `generate_hydrology` stamps `RiverDelta`/`Floodplain`/`FreshwaterMarsh` at river mouths, and `apply_tag_budget_solver` paints polar `Tundra` over near-shore ocean — each creating fresh gentle-land-vs-`DeepOcean` adjacencies with no shelf between them (band-level zero-gap ≠ final-map zero-gap). `reconcile_coastal_shelf` (`systems.rs`) is a deterministic post-pass registered in the Startup `.chain()` **right after `apply_biome_palette_clamp`** (so after hydrology + tag solver + palette clamp — the last word on ocean tiles): every `DeepOcean` tile odd-r hex-adjacent (`grid_utils::hex_neighbors_wrapped`, wrap-aware, honoring the active `map_topology.wrap_horizontal`) to a **gentle** land tile — non-`WATER` tags, rise `elevation.sample − sea_level < coast_height_threshold` (the SAME gate + hex convention as `classify_bands`) — is reclassified to `ContinentalShelf` (a `must_have` palette biome, so no palette conflict). So downstream-created coasts (deltas, marshes, solver tundra) all get a shelf seaward, while **steep** coasts (every land hex-neighbour rises `≥` threshold) still keep deep water right at the edge. Guarantees on the final map: **no `DeepOcean` tile touches gentle land.** Guarded by `integration_tests/tests/shelf_ratio.rs::earthlike_no_deep_ocean_touches_gentle_land_on_final_map` (0 gaps across sizes/seeds, + a steep-coast-keeps-deep-water assertion) and `earthlike_delta_and_marsh_coasts_have_shelf_not_deep_water`.
- `width_tiles` (default 2) — legacy absolute band width, used only when `width_frac` is unset (e.g. `polar_contrast`). `width_frac` + `width_exp` (earthlike) scale the pre-floor width with map size as `width_frac * min(w, h)^width_exp`.

  Because the shelf is now a ~1-tile ring off *most* coastline, the fraction is **no longer** the old size-invariant 5-8%: it varies with coastline steepness and **shrinks as the open ocean grows** — measured full-pipeline (slope folded into deep water) with the hex-exact ring **plus** the final reconciliation pass it runs **~15-19% of ocean at 80×52 down to ~8.5% at 256×192** (re-measured after `elevation-authority`; the pre-arc figures were ~29-33% down to ~14%, and the drop is a *consequence* of the derived mask producing fewer, smoother landmasses — less coastline per unit of ocean, with the zero-gap invariant still holding) (a touch higher again than the band-only ring, since the post-pass also stamps shelf on the hydrology/tag-solver coasts; re-measured after the border-ring bathymetry fix below, which removed the orphaned offshore shelf the drowned border land used to strand). Guarded by `integration_tests/tests/shelf_ratio.rs`: a per-map sanity band (6-50%) plus the model assertion that coast land next to shelf tiles is lower than coast land next to deep-water-at-the-edge tiles. This is a pure ocean-tile reclassification — it does **not** touch the land mask, so mountains/rivers/land ratio are unchanged.

  The gate keys off the *immediately-adjacent* (hex-neighbour) coast land, which fully covers the 1-tile default (every shelf tile touches land). Deferred: a preset that widens the shelf past `d==1` leaves outer-ring tiles ungated (they touch no land, so they pass) and those outer rings still ride the square-connected `ocean_distance` — carrying the nearest-coast rise through a hex distance-transform is the follow-up for wide shelves. Also still deferred: a true *depth-based* shelf would need real offshore bathymetry (today ocean elevation is fractal noise with no coast-relative deepening); and if the narrower shelf's reduced `CoastalUpwelling` forage frontage matters for gameplay, lock the `Coastal` tag to stamp compensating `TidalFlat` (the tag solver's coastal pass). Neither shipped preset locks `Coastal` today.

**Elevation ↔ biome coupling** (`restamp_elevation`, `mapgen.rs`): mountain biomes come from the tectonic mountain mask + relief, so the elevation field is tied to that same signal to keep them consistent (mountains genuinely tall — see the `mountain_tiles_out_top_lowland_tiles` regression test). Every mountain-mask tile is floored into `[elevation_base, 1.0]`, ordered by relief and scaled by per-type prominence; non-mountain land is compressed into `[sea_level, elevation_base]`. Tunables live in each preset's `mountains` block: `elevation_base`, `fold_prominence`, `fault_prominence`, `volcanic_prominence`, `dome_prominence`, `belt_texture` (small spine-vs-edge elevation texture added on top of the relief floor; bounded so it never reorders relief bands). The non-mountain `elev ≥ high_dry_elevation → CanyonBadlands` / `elev ≥ high_wet_elevation → RollingHills` cutoffs (`terrain.rs`) live in `terrain_classifier` and default to the top of the compressed lowland band.

**Highland biomes are mask-driven, never noise-driven.** `classify_terrain` (the base climate classifier) does NOT pick AlpineMountain/HighPlateau/CanyonBadlands/etc. — it has no real elevation, so it used to invent them from a tile hash and scatter flat "mountains." Mountain biomes now come only from the tectonic mask (`select_mountain_terrain`) + the real-elevation `terrain.rs` branches. `apply_belt_relief` (`mapgen.rs`) scales belt-tile relief by belt strength (`mountains.relief_belt_gain`, default 1.2) so belt cores clear the AlpineMountain relief threshold (`terrain_classifier.alpine_relief_threshold`, **1.85**) and taper to plateaus/hills — genuine Alpine spines that are also tall. Polar rows are skipped (they keep their low-relief-basin tuning). Regression guards: `mountain_tiles_out_top_lowland_tiles`, `alpine_biome_tiles_are_tall`.

> **`alpine_relief_threshold` is the alpine range's WIDTH lever — and it is the only one that
> narrows the range without also flattening or shrinking it.** Belt relief ramps linearly with belt
> strength (`1 + relief_belt_gain × strength / (belt_width + 1)`, `strength = belt_width + 1 −
> dist`), so the threshold picks an integer distance-from-plate-boundary cutoff `D`; the boundary is
> stamped on **both** plates, so the alpine ribbon is **`2D + 1` tiles wide** and two boundaries
> within `2D + 1` merge into a slab. At earthlike's `belt_width = 4` / `relief_belt_gain = 1.2` the
> bands are `≤ 1.60` ⇒ `D = 3` (a **7-tile slab**), `(1.60, 1.72]` ⇒ 5 tiles, `(1.72, 1.96]` ⇒
> **3 tiles (shipped, 1.85 — mid-band, so a small retune of `relief_belt_gain`/`mountain_scale`
> cannot silently step the ribbon a whole tile wider)**, `> 1.96` ⇒ single-tile peaks.
>
> **Measured** (`core_sim/tests/relief_sweep.rs::belt_sweep`, `--ignored --nocapture`, 6 seeds), the
> shipped `1.45 → 1.85` move at 384×288: alpine **thickness mean 2.43 → 1.57, p95 5.0 → 3.0**;
> alpine **6.1% → 3.0% of land**; connected **components 27.0 → 28.0** — the count *rises* as slabs
> break into distinct ranges, which is the wanted direction. At the shipped 80×52: thickness
> **1.80 → 1.36**, p95 **3.83 → 2.33**, alpine **15.9% → 7.0%** of land. Sowable ground at seed
> 119304647 is **unchanged at 49** and land fraction is unchanged (0.391).
>
> **The other two belt levers were measured and rejected**, both of which reach at best the same
> integer cutoff while costing something else: `mountains.belt_width_tiles` 3 → 2 only reaches
> `D = 2` and shrinks the whole belt — the **foothill skirt** goes with it (it also perturbs
> downstream terrain: sowable 49 → 51); `mountains.relief_belt_gain` 1.2 → 0.70 reaches `D = 1` but
> **lowers the belt core's relief 2.2 → 1.7**, i.e. it makes the mountains *shorter*, when the
> complaint was width. Raising the threshold leaves the relief profile — and so every peak height
> and the `restamp_elevation` relief ordering — **byte-identical**, and merely reclassifies the
> belt's shoulders to HighPlateau/RollingHills/CanyonBadlands. That is what makes a range read as a
> **range with foothills** instead of a slab. **Do not "simplify" this back into `belt_width_tiles`.**
>
> The continental-envelope terms (warp/tilt/spine) **cannot** do this — measured flat at 2.22–2.43
> thickness across every combination; see the `macro_land` note above.

**Number of ranges** is emergent tectonics: land connected-components → plates (area buckets, ≤4/continent) → fold belts form only where two plates' drift *converges* (`dot <= mountains.belt_convergence`, `derive_mountain_mask`). Drift is radial-outward so most boundaries diverge; raising `belt_convergence` toward 0 (earthlike default **0.25**; polar_contrast keeps the tighter **−0.1** to preserve its low-relief-basin contrast) lets more boundaries become ranges. Range count also scales strongly with **map size** — a full 256×192 map has 30+ ranges, an 80×52 "Standard" ~4–13, a 56×36 "Tiny" ~2–6.

**`classify_terrain`'s map-border "edge rings" are LEGACY, preset-less-only.** The classifier opens with three `edge < coastal_deep_ocean_edge / coastal_shelf_edge / coastal_inland_edge` early-returns that stamp DeepOcean / shelf / InlandSea+marsh. `edge` is the distance to the **map frame**, not to a coastline: it was the only coastline proxy the pre-bands (preset-less) world had. Under a preset the map has **real bathymetry** — `classify_bands` already partitioned it into Land / ContinentalShelf / InlandSea / DeepOcean, and `terrain_for_position_with_classifier` is called *only* for band-`Land` tiles — so running the rings there noise-coin-flipped **248–295 band-`Land` tiles per 80×52 map (~16–19% of all land)** into water biomes hugging the map border, deleting the land out from under legitimate shelf rings (118–153 **orphaned** shelf tiles with no land hex-neighbour, sitting 3–7 hexes out) and pinching off isolated deep pockets. The rings are therefore **skipped whenever real bathymetry is present** (`BathymetryContext::Present`, derived from the caller passing `Some(elevation)` — the *context*, never a config flag), and the tile falls through to the normal polar/anomaly/humidity **land** ladder. The preset-less fallback path passes `None` → `BathymetryContext::Absent` and keeps its historical behavior exactly. Invariant: **a band-`Land` tile can never end WATER-tagged.** Guards: `mapgen::tests::earthlike_band_land_never_ends_water_tagged`, `mapgen::tests::earthlike_shelf_is_never_orphaned`.

**Tag Budget Solver**: After biome stamping, iterates locked tag families (water → wetlands → fertile → coastal → highland → polar → arid → volcanic → hazardous) nudging tiles until coverage falls within `tolerance`. Every family that stamps or forbids a **cold** biome (wetland picks `PeatHeath` vs `FreshwaterMarsh`; fertile skips cold tiles; the polar family paints Tundra/SeasonalSnowfield) gates on the tile's **temperature band** (`TileInfo.band`, resolved once from `Tile.temperature`), not its latitude — see "Temperature is the climate authority". The polar family carries a **climate veto**: it under-fills and logs rather than painting a cold biome in warm air (`docs/plan_climate_authority.md` §5.4).

  **The solver has NO water branch, and `terrain_tag_targets.Water` is INERT.** Water share is an
  elevation outcome: the mask is a pure threshold and the contour anchor already places
  `target_land_pct` exactly. `elevation-authority` deleted the branch outright — it converted arbitrary
  land tiles to `DeepOcean` (and ocean back to `Tundra`/`AlluvialPlain`) **with no elevation term at
  all**, which is precisely how a "water" tile ended up above sea level. Listing `Water` in
  `locked_terrain_tags` no longer does anything.

  The target is kept only so the tag census has a reference figure, and should still track
  `1 − macro_land.target_land_pct` for that reading to be meaningful (earthlike `0.62`;
  polar_contrast was corrected `0.64 → 0.58` against its `target_land_pct = 0.42` during the arc —
  the map had been right and the target stale). *Historical:* when the branch existed, a mismatched
  target made the solver invent bathymetry the pipeline never modeled — earthlike's old `Water = 0.65`
  vs `target_land_pct = 0.38` would have drowned ~125 `COASTAL` tiles. That failure mode is now
  structurally impossible rather than avoided by convention.

**Per-Map Biome Palette** (`biome_palette.rs`, design `docs/plan_biome_palette.md`): a curated,
seed-driven, map-size-scaled subset of the 37 biomes chosen at world-gen time — small maps read
legibly, large maps stay rich, and the full library is preserved for replay variety. **This is how
maps generate now, not an opt-in mode.** Each biome carries an intrinsic `BiomeNiche` (8-way
partition) + `must_have` flag (`terrain.rs` `biome_niche`/`biome_must_have`, folded into
`TerrainDefinition` by `def`). The `BiomePalette` resource is built in `spawn_initial_world` from
`world_seed ^ PALETTE_SEED_SALT`: per niche it keeps the `must_have` members and seed-samples up to
`K` (size-interpolated from the preset's `biome_palette` block — `small_map_tiles`/`large_map_tiles`
+ per-niche `k_small`/`k_large`), then force-includes the solver's locked-tag fallback biomes.
Enforcement is a **climate-aware niche-nearest remap** (`BiomePalette::remap(biome, is_polar)`): at
the `bias_terrain_for_preset` seam and again in the post-solver `apply_biome_palette_clamp` system
(inserted in the Startup chain right after `apply_tag_budget_solver`), any off-palette biome is
replaced by an allowed member of the same niche — polar tiles only remap to POLAR-tagged members, so
the palette never stamps temperate plains/marshes at the poles; `RiverDelta` is `must_have` so real
river mouths pass through. **Must-have set** (`biome_must_have`, 9): DeepOcean, ContinentalShelf,
InlandSea, AlluvialPlain, PrairieSteppe, Tundra, RiverDelta, Glacier, **NavigableRiver** (the last
for the same reason as `RiverDelta` — it is hydrology-placed, and off-palette it would remap to
`DeepOcean` and cut the continent in half with open sea; adding it gave the Ocean niche a **fourth**
must-have, so earthlike's Ocean `k_large` was widened 4 → 6 to keep the two *interchangeable* ocean
flavours, CoralShelf and HydrothermalVentField, reachable at all). `must_have` is reserved for a
single *physically-gated* member inside an otherwise-thinnable niche: `InlandSea` in Ocean (else
off-palette inland water renders as DeepOcean) and `Glacier` in PolarLowland (else a tall polar peak
remaps down to flat Tundra — it's the polar analog of AlpineMountain, placed only where relief clears
`alpine_relief_threshold`). **Physically-gated-vs-interchangeable principle** (`docs/plan_biome_palette.md`
§3.2b): thinning only ever applies to interchangeable flat-land climate/flavor niches. The fully
physically-gated niches — `Highland` (relief/elevation/mask regimes) and `Volcanic` (volcanic-arc
mask) — are **never thinned**: their palette `K` is set to full membership at both endpoints
(`Highland` 5/5, `Volcanic` 3/3, in the `BiomePaletteConfig` default + earthlike JSON), so AlpineMountain
and every highland/volcanic member is always available and never remapped away. Un-thinning Volcanic
never forces volcanoes onto a non-volcanic map (the niche is simply absent with no arc + no fumarole
hit). Do **not** add other highland biomes to `must_have` — the niche's full `K` already keeps them
always-available while staying tunable. Reconciled with the
tag solver by construction (force-included fallbacks) plus the clamp as insurance. Also revives 3
previously-unreachable biomes (`§3.6`): Glacier (high-relief polar mountains), BasalticLavaField
(low-relief volcanic mask via `terrain_classifier.basaltic_relief_threshold`), AquiferCeiling (one of
the six anomaly biomes) — so "all 37" is now literal. **Anomaly rarity:** anomaly/"discovery" biomes
(crater/sinkhole/karst-cavern/fumarole/volcano/aquifer) are gated in `classify_terrain` by a config
lever `terrain_classifier.anomaly_fraction` (default 0.04 — ~4% of eligible flat lowland, split evenly
across the six), replacing the old fixed 6-of-16 (~37%) slice that blanketed the land. **Niche note:** BorealTaiga is homed in `PolarLowland` (not `FertileLowland` as
the design table lists) because it is POLAR-tagged — see the comment on `biome_niche`. Biome ids are
unchanged (no client/schema impact). Independent of terrain-texture work.

---

## Ecosystem Food Modules

Pre-agricultural survival modules mapping to worldgen tags, snapshot payloads, and client affordances.

| Module | Primary Inputs | Storage Hooks |
|--------|----------------|---------------|
| Coastal Littoral | Shellfish, tidal fish, kelp | Fish racks, shell middens |
| Riverine / Delta | Freshwater fish, cattail gardens | Smokehouses, tuber pits |
| Savanna Grassland | Herd shadowing, wild yams | Jerky racks, nut caches |
| Temperate Forest | Oak/chestnut groves, berries | Clay-lined nut pits |
| Boreal / Arctic | River/ice fishing, seals | Permafrost pits, pemmican |
| Montane / Highland | Alpine tubers, marmots | Sun-dried meat, stone caches |
| Wetland / Swamp | Cattail rhizomes, amphibians | Mud storage, smoke curing |
| Semi-Arid Scrub | Drought tubers, cactus fruits | Roasting pits, seed cakes |

**Implementation**: `FoodModuleTag` components with tile entity, module id, seasonal weight. `ForageSiteLedger` tracks capacity. Commands: `gather_roots`, `harvest_shellfish`, `dry_fish`, `follow_herd`.

> **Wild game is an overlay, not a tile flag.** Game used to overwrite a food
> tile's gather kind with `FoodSiteKind::GameTrail` (×0.75 weight), but food-site
> curation sorts by weight **descending** so game trails never survived (0 on live
> maps). That upgrade + the `wild_game_*` config + `GameTrail` are **retired**;
> wild game now lives in the fauna herd layer (below), so a tile offers **both**
> gathering and hunting. See "Fauna & Wild Game" and
> `docs/plan_wildlife_hunting_overlay.md`.

---

## Fauna & Wild Game

Mobile animal **groups** (not individuals) graze-wander / migrate across the map
independent of the gather layer (see "Movement" below). One entity = one
band/warren/herd; `biomass` = group size.

**Species table** (`src/data/fauna_config.json`, loader `fauna_config.rs`): the
former hard-coded `HerdSpecies` enum is now a data-driven table. Each row has a
`display_name` (also the snapshot `species` string — it embeds the client icon
keyword, e.g. "Red Deer" → 🦌), `size_class` (`migratory`/`big`/`small`),
`migratory` flag, `route_len` `[min,max]` (= roaming range), `biomass` `[min,max]`
(group size), and `host_biomes` (a list of **`FoodModule` keys**, reusing
`classify_food_module`). Shipped roster (19 rows): **migratory** mammoth/steppe_runner/
marsh_grazer/reindeer/wild_horse (long routes); **big game** deer/boar/aurochs/seal/wild_elk
(2–3 tiles); **small game** rabbit/fowl/crag_goat/wild_sheep/alpine_ibex/gazelle/forest_grouse/
**river_fish**/**snow_hare** (~1 tile, stationary). The `pen`-ceiling **livestock** are
aurochs (🦬, wild r 0.09 → slow ranch cattle) on grass + woodland edge, **Crag Goats** (🐐,
wild r 0.22 → fast hardy hill stock) on highland/dry-upland, plus boar, rabbit, fowl,
wild_sheep and snow_hare.

**Regional signature — every biome offers distinct game, and every land biome offers a pen.**
Three roster rows close the last gaps:
- **`river_fish` ("Silt Catfish")** — the wet biomes' own game, hosting
  `riverine_delta`/`wetland_swamp`/`coastal_littoral`. Structurally the `seal` row: a
  non-grazing, non-migratory colony with `route_len [1,1]`, pinned to the shore by
  **`requires_adjacent_water`**, so it needs no new Rust. A catfish inland is a bug.
- **`snow_hare` ("Snow Hare Warren")** — hosts **`boreal_arctic` alone** at a **`pen`** ceiling.
  Before it, `boreal_arctic` was the **only land biome with no `pen`-ceiling species at all**: the
  mammoth and elk there are `wild`, the reindeer only `pastoral`, so the intensification ladder's
  pen rung was flat unreachable from a northern start. The hare is what makes it reachable.

> **`host_biomes` names a MODULE, not a terrain — and `montane_highland` is the leaky one.**
> `CanyonBadlands` (an arid desert canyon) carries `ARID | HIGHLAND` and reaches the **fallback**
> arm of `classify_food_module_from_traits`, where the `HIGHLAND` test runs **before** the `ARID`
> test — so arid badlands classify as `montane_highland`. A live playtest duly found snow hares
> warrening in a desert canyon. There is no way to target one exact `TerrainType`, so the fix is to
> drop the module: `boreal_arctic` is an **explicit** arm (BorealTaiga | Tundra | PeriglacialSteppe
> | SeasonalSnowfield) and is exactly the hare's range. Nothing is lost but the occasional alpine
> warren, and no gameplay gap opens — `montane_highland` already has `crag_goat` at a `pen` ceiling.
> **The next cold-climate species pointed at `montane_highland` will hit this same trap.** Guarded
> by `fauna_wet_biome_roster::snow_hares_never_warren_the_highlands`, which asserts on the *spawned
> tile's module* — a count floor would not catch it, since re-adding the host raises the count.
- **`boar` gained `riverine_delta`**, giving the delta a big-game row beside the migratory marsh
  grazer and the small fowl/catfish.

Measured over `SWEEP_SEEDS` on the standard 80×52 earthlike map
(`core_sim/tests/fauna_wet_biome_roster.rs`, the guard against the silent never-spawn of an
unmatched `host_biomes` key): **20 catfish colonies** (1–6 per map, all water-adjacent), **37
snow-hare warrens** (4–8), **54 boar groups on delta tiles** (5–15) — each **0** before the
change. **The map-wide game cap is saturated** (122 herds per map against
`abundance.max_total_game` 120 + 2 migratory, identical pre- and post-change), so these three are
**displacing** other short-range game rather than adding to it — the roster shifts composition,
never density. Raise `max_total_game` if the intent is more game, not different game.

**Spawning** (`spawn_initial_herds`, `fauna.rs`): two passes into one
`HerdRegistry`.
1. **Migratory** — a few long-route walkers (`determine_herd_count`,
   `build_migratory_route`), species drawn from the config's `migratory` rows. **`host_biomes` is
   LIVE here** (it was previously ignored): a herd's loiter **anchors** are placed on tiles suitable
   for its species (`module_at ∈ host_biomes`), drawn from a regional home range
   (`MIGRATORY_HOME_RANGE_RADIUS`, spaced by `MIGRATORY_ANCHOR_MIN_SPACING`) around a random suitable
   seed tile and ordered into a walkable circuit by nearest-neighbour chaining — so the migration
   legs cross the less-suitable ground *between* the patches and the herd lives in its biome range
   across the map rather than clustered at the player start. A species whose host biomes the map
   lacks (empty `suitable`) **falls back to the start-anchored spiral** (`build_route`), so it still
   spawns somewhere.
2. **Short-range game** — iterate land tiles, classify each via
   `classify_food_module`, roll `abundance.per_biome[module]`; the map-wide winners
   are shuffled then greedily placed respecting `min_spacing` up to `max_total_game`
   (bounded entity count, spread across the map rather than clustered by scan
   order). Route via `build_short_route` (`route_len == 1` → single stationary
   tile → no client trail).
   **The species candidate list is SITE-FILTERED before the pick** (`spawn_game_group_at`): a
   candidate carrying a site rule — today only **`requires_adjacent_water`** — is dropped unless the
   winner tile satisfies it, and only then is the single `rng.gen_range` draw made (the draw count is
   unchanged; only its bound moves). **An empty filtered list spawns nothing on that tile** — a cold
   *inland* tile whose only candidate is a marine forager correctly stays empty rather than seating a
   seal on the tundra. The immigration path (`repopulate_fauna`) shares the helper, so a respawn
   obeys the same rule. **The migratory pass does NOT apply site rules** — which is why
   `migratory + requires_adjacent_water` is validate-rejected rather than silently ignored.

**Movement — graze-wander + loiter-then-migrate** (`advance_herds`, `docs/plan_wildlife_hunting_overlay.md`
"Herd Movement"). A `Herd` carries a **live `current_pos`** (walked ≤1 hex/turn, land-clamped,
wrap-aware — `position()` returns it) over its sparse `route` (now **anchors**, not a per-turn path),
plus a `RoamState` + `dwell_remaining`. One primitive — **graze-wander** (dwell `dwell_turns`, then
step ≤1 hex) — split by `size_class`:
- **Wild game** (`Big`/`Small`): permanent `GrazeWander` toward the current cluster anchor (cycling);
  ≈ half speed (a `route_len==1` group stays put). Catchable by an equal-speed party during a graze
  turn.
- **Migratory**: a `Loiter { turns_left }` ↔ `Migrate` state machine over the anchors. **Loiter** —
  graze-wander within `loiter_radius` of the current anchor for `loiter_turns` (sampled). **Migrate** —
  1 hex/turn toward the next anchor, **no dwell**, then loiter at the new anchor. Fixes the old bug
  where `Herd::advance()` teleported 4–12 tiles/turn along the sparse route.

**Herd movement is a rung primitive** (intensification ladder slice 3b — the **first** behavior
primitive the engine reads). `advance_herds` resolves the herd's rung (`fauna::herd_rung`: penned →
`animal:pen`, tamed → `animal:pastoral`, else `animal:wild`) and dispatches on its
`behavior.movement`, so §3's proximity spine **far → near → fixed** is *config*, not a branch on
`is_domesticated()`:
- **`roam`** (wild) — the graze-wander / loiter-migrate machine below, over its own full range.
- **`drift_to_owner`** (pastoral) — each turn the herd first tries **one step toward the nearest band
  of its owning faction** (`ResidentBand` only — a camp, not a passing expedition party). It
  **composes with, never replaces, the 2b-i graze-aware roam**: the candidates are exactly the roam's
  own acceptable steps (`acceptable_steps` — land, and not barren), ordered by **(hex distance to the
  nearest camp ASC, graze capacity DESC, y ASC, x ASC)** — a *preference ordering*, so there is **no
  drift-strength lever** to tune. Only a step that genuinely closes the distance counts as a drift;
  once the herd is at the camp or hemmed in, the turn **falls through to the normal roam**, so a tamed
  herd grazes *around* its people instead of freezing on their tile. The species' own `dwell_turns`
  cadence still applies (taming makes an animal *near*, not fast), and the herd never crosses barren
  ground to reach its owner. An unowned herd, or an owner with **no bands**, roams normally. The last
  two sort keys are load-bearing: two candidates can tie on distance *and* capacity, and a tie broken
  by anything incidental is the ~20% flake `GrazeRegistry::richest_patch` already cost us.
  - *Emergent tension, deliberately unsolved (playtest):* a herd that prefers proximity will settle for
    adequate-but-poorer pasture near camp, which lowers its range-derived `K` and shrinks it — real
    pastoral overgrazing. It cannot **strip** the range: 2b-ii's `overgraze_escapement_fraction` floor
    still binds, so the pasture recovers and the herd stabilizes smaller.
- **`fixed`** (pen) — pinned at `corralled_at`, no roam, no heading arrow.

Movement is **deterministic under rollback** — a per-herd/​per-turn `SmallRng` seeded from `map_seed ^
tick ^ HERD_MOVEMENT_SEED_SALT ^ fnv(herd.id)` (mirrors `repopulate_fauna`). Cadence levers are
per-species on `SpeciesDef` (`fauna_config.json`): `dwell_turns` (~1), `loiter_turns [min,max]`
(migratory, e.g. [12,24]), `loiter_radius` (~2), all `#[serde(default)]`. `advance_herds` resolves a
herd's levers via `FaunaConfig::species_by_display`. Movement is **independent of** `regrow_biomass`
(a loitering herd still grazes/regrows — ecology unchanged). Telemetry `next_position` is the next
`Migrate` hex (client heading arrow), `None` while loitering/grazing.

Abundance is a **tuning value, high to start** (design: game plentiful early,
thins under overhunting in later phases). Herds
flow to telemetry, the `HerdDensityMap`, and the snapshot (`HerdTelemetryState`,
which now also carries `size_class` + `huntable` so the client can offer the right
verbs — a free-form `species` string means new species need no schema change).

**Hunt (one-shot)** — the `hunt_fauna <faction> <herd_id> [band_entity_bits]`
command (`handle_hunt_fauna`, `server.rs`; full plumbing in `command.proto` /
`commands.rs` / `command_text.rs`) attaches a `FaunaPursuit` component (`components.rs`)
to a band (auto-picked when no band id is given). Each turn `advance_fauna_pursuits`
(`systems.rs`, `TurnStage::Population`) re-reads the herd's **live** position (herds
already moved in the earlier `Logistics` stage), steps the band up to
`hunt.pursuit_tiles_per_turn` toward it, and on closing to `hunt.pursuit_radius`
(=1, Chebyshev) resolves a one-shot take: `hunt.take_from(biomass)` biomass →
provisions/trade (`hunt.*_per_biomass`), drawn from the group and added to
`FactionInventory`, then removes the component. An elusive herd is abandoned after
`hunt.max_pursuit_turns`. Config lives in the `hunt` block of `fauna_config.json`.

**Follow (persistent, per policy)** — `follow_herd <faction> <herd_id> [policy]
[band_entity_bits]` attaches a `FaunaPursuit { mode: Follow { policy } }`
(`FollowPolicy` ∈ Sustain | Surplus | Market | Eradicate). The same `advance_fauna_pursuits`
system keeps the band within `pursuit_radius` of the moving group and, once adjacent,
**auto-hunts each turn per policy** instead of removing the component. The policy is a
free string parsed via `FollowPolicy::from_str`, so a new policy needs no schema/proto change. Each
turn it also grants a small non-food benefit — a `FogRevealLedger` tracking pulse
(`follow.reveal_radius`/`reveal_duration_turns`) + `follow.morale_gain`. The old one-shot teleport
follow (and its `apply_herd_rewards`/`apply_herd_knowledge` helpers) is retired.

> #### The hunt policy axis: FOUR ASCENDING MULTIPLES OF MSY + a kill-credit bank (slice 8b)
>
> `fauna::hunt_policy_rate` (the per-turn take **rate**) + `hunt_credit_ceiling` (what the herd's banked
> credit can afford this turn) are the one source. Each policy earns a multiple of the sustainable yield
> (`MSY = r·K/4`, `peak_regrowth`), banked into `Herd::hunt_credit`, and a whole animal is killed only
> once the bank clears one `body_mass`:
>
> | policy | rate | herd |
> |---|---|---|
> | **Sustain** | `sustainable_yield` = `min(MSY, regen(B))` | stable, settles at `K/2` |
> | **Surplus** | `hunt.surplus_multiplier × MSY` (**1.5**) | slowly declines (reversible) |
> | **Market** | `hunt.market_multiplier × MSY` (**2.5**) | declines to **extinction** |
> | **Eradicate** | the whole standing stock (bypasses the bank) | gone |
> | **Tame / Corral** | Sustain's rate × the rung's `yield_fraction_while_building` | a dip on a sustainable draw |
>
> **Monotone in take BY CONSTRUCTION.** Surplus/Market are multiples of the *same* MSY base, so
> `Sustain ≤ 1× < 1.5× < 2.5× ≤ B` at every biomass and every species — *"each option takes more than
> the previous."* `FaunaConfig::validate` pins `1 < surplus_multiplier < market_multiplier` (one
> rejection test per bound: `validate_rejects_a_{surplus_multiplier_at_or_below_one,
> market_multiplier_at_or_below_surplus}`), and
> `fauna_market::hunt_policy_takes_are_strictly_ordered_at_every_biomass` sweeps the ordering across
> B × {fast, slow}. **The regression guard against reintroducing a skim** — do not weaken it.
>
> **The kill-credit bank is what makes multiples-of-MSY produce whole lumpy animals.** For 7 of 9
> species MSY < `body_mass`, so a *per-turn ceiling* of that rate would `floor` to **zero forever** (the
> flow trap that made those species unhuntable under the old `r·K/4` Sustain and `1.6 × MSY` Surplus).
> Banked, the fractional rate **accumulates** until a body is affordable — a mammoth is a wait-then-one
> pulse (`body/MSY` turns per kill), a rabbit takes *several per turn* (the credit ceiling never clamps
> it to one). The bank **carries across policy changes** (earned regrowth toward the next animal;
> switching Sustain↔Market must not reset it) and is **capped at the standing biomass** (never bank
> credit for animals that do not exist — that would release a burst on recovery). Measured
> (`fauna_market::the_kill_credit_pays_multiples_for_fast_game_and_a_pulse_for_big_game`): a full **Rabbit**
> (MSY 350, body 2) Sustain-takes ~200 rabbits/turn tapering to `K/2`; a full **Mammoth** (MSY 120, body
> 800) waits ~7 turns then takes one under Sustain, ~4 under Surplus, ~3 under Market.
>
> **Sustain's rate is sized against the PRE-regrowth biomass** (`Herd::biomass_before_regrowth`, captured
> at the top of `regrow_biomass`). The take runs *after* Logistics regrowth, so evaluating
> `sustainable_yield` at the grown stock takes slightly more than the herd grew (`regen(B_post) >
> regen(B_pre)`) and slowly **leaks a below-`K/2` herd down**. Reading the pre-regrowth biomass makes
> Sustain take exactly one turn's growth below `K/2` (the herd **holds/recovers** — pinned by
> `fauna_market::a_below_half_k_herd_under_sustain_recovers_never_declines`) and a full MSY above it (a
> **gentle** decline to `K/2`, no escapement burst).
>
> **Extinction is REAL and on-map.** Constant catch above MSY has no equilibrium: Surplus declines a
> herd (reversible if switched back), **Market drives it extinct** (`market_hunt_drives_collapse`). The
> resident band (`systems::hunt_take`) and the hunting expedition (`expedition_take_biomass`) share the
> same rate + bank, so a herd hunted by either reads one accumulator.
>
> **Shared herd (chosen handling, reported):** credit advances **per hunt resolution** — once per
> resident `hunt_take` and once per expedition take. The intended invariant is **one hunter per herd**
> (a resident band leashes to a nearby herd; expeditions target distant migratory ones), where it is
> exactly correct. Two *concurrent* hunters on one herd would each bank their rate (more pressure, a
> faster harvest — realistic and non-crashing, since the bank is capped at the stock and kills at the
> animal count). Not a per-worker-share split — that would be overbuilding for a case the labor system
> does not normally produce.
>
> **Retired levers** (all stay retired): `follow.surplus_multiplier`, `surplus.take_fraction`,
> `market.take_fraction`, `hunt.take_fraction` / `min_take` / `take_from`; `ecology.
> surplus_escapement_fraction` and `fauna::hunt_policy_floor` were deleted with the ordered-floors cut.
> `ecology.collapse_fraction` is once again **only** the Allee/depensation threshold (it briefly doubled
> as Market's floor). Config: the `hunt.{surplus_multiplier, market_multiplier}` + `market` blocks of
> `fauna_config.json`.

> #### Herding is standing labor, and it scales with the HERD (slice 8)
>
> `fauna::herders_needed` — `ceil((biomass / body_mass) / animals_per_herder)` — is owed **every turn**
> by a pastoral or penned herd, **including wait turns** when it cannot spare an animal. *Just because
> you aren't killing an animal doesn't mean you aren't tending them, keeping them from running off,
> repairing fences.* Before this a pen of 2 and a pen of 200 needed the same single keeper; only the
> feed scaled.
>
> - **Downward hysteresis — staff it once and it holds.** The bare `ceil` is stateless, so a
>   Sustain-hunted slow herd sitting near an `animals_per_herder` multiple (a Wild Aurochs near 12
>   head) **flickers 1↔2 every turn** as the lumpy whole-animal kill breathes its biomass ±1 animal
>   across the boundary — and because the `herded_fraction` decay lags a turn, the player is told
>   "staff all 1", then "staff all 2", satisfies neither, and slips the tameness. So the requirement is
>   now a **persisted, deadband-stabilized `Herd::herders_needed`** (round-tripped through `HerdState`
>   like `corral_progress`), updated every turn by `Herd::stabilize_herders_needed` in
>   `advance_husbandry`: **up immediately** when the raw need rises (under-herding is harmful), **down
>   only once the herd falls below `(current − 1)·animals_per_herder − band`** where `band =
>   animals_per_herder × husbandry.herders_hysteresis_fraction` (**0.25**, `fauna_config.json`, a
>   playtest dial; `0` restores the raw flicker). A herd bumped to 2 stays at 2 through a one-animal dip
>   and drops only on a genuine multi-band fall — wild = 0 unchanged (a wild herd isn't yours to
>   maintain). `herd_herders_needed` reads this stabilized field (falling back to the raw ceil only for
>   a not-yet-stabilized managed herd — the turn it is tamed, or a test fixture), so **every** consumer
>   (`herded_fraction` decay, `managed_crew_needed`, the `herdersNeeded` snapshot field) is steady; the
>   wire field is unchanged, just no longer churning.
> - **Heads, not tonnes.** The denominator is per-**animal** (`SpeciesDef::animals_per_herder`,
>   per-species: fowl/rabbit 200, crag_goat 80, boar 15, steppe_runner/marsh_grazer 15, aurochs 12;
>   deer/mammoth are `wild`-ceiling and omit it). A shepherd minds ~300 sheep, a cowherd ~80 cattle —
>   you watch individuals, and a heavier beast is not proportionally more work. A per-*biomass* dial
>   says "one herder per 100 fowl but one per 2 boar" and invents a 45-herder steppe megaherd that is a
>   pure artifact of the unit (4,560 biomass of Steppe Runner is **38 animals** ⇒ ~3 herders).
> - **ONE need, not two — but "one need" means one CREW, not one formula.** The herders mind the herd
>   *and* butcher it, so a managed rung reports **one** number and staffs **one** team
>   (`systems::labor::managed_crew_needed`) — but that team must be big enough for **both** jobs, which
>   scale on **different units**: herding is per **head** (one herder minds 12 aurochs), hauling is per
>   **biomass** (one hauler carries 40). A shepherd minds ~300 sheep and could not carry three. So
>   `workersNeeded = max(herders_needed, hunt_haul_workers)` — `+` would be two teams; `max` is
>   one crew covering its busiest job. **Neither term dominates across the roster** (measured, settled
>   radius-1 pens): small-bodied species are **herder-bound** (Wild Fowl 9 herders vs 5 haulers; Rabbit
>   5 vs 4), big-bodied ones are **haul-bound** (Crag Goats 2 vs 7; Boar 1 vs 3; Aurochs 2 vs 3). Do not
>   "simplify" the `max()` away.
>
> - **The haul term is the STEADY carry crew, not this turn's `carried`** (`fauna::hunt_haul_workers`).
>   `workers_needed`'s hauling component is the crew that carries home the **peak per-turn animal drop**
>   — `ceil((floor(rate/body) + 1)·body / per_worker)`, off the policy's **steady** `hunt_policy_rate`
>   (not the credit-inclusive `hunt_credit_ceiling` burst) — the **same** count the client's compose
>   panel `_max_useful_workers` caps at. It is deliberately **not** `workers_needed_for_take(take.carried,
>   …)`: a slow breeder whose MSY < `body_mass` (a Wild Aurochs) drops **zero** animals on a wait turn
>   while its kill-credit banks, so inverting `carried` collapses `workers_needed` to `0` — and, for a
>   managed herd, to the bare herder count via `max()`. That made the panel contradict itself:
>   `workersNeeded: 1` beside a 50%-`wastedYield` at one worker — *drop workers* and *add workers* on the
>   same row, with half an aurochs rotting. Sizing the crew off the steady rate makes it **stable across
>   wait and kill turns** (it can't flicker with the pulse) and **equal to `wasted_yield`'s answer**:
>   `workers > workers_needed` ⇒ overstaffed, `wasted_yield > 0` ⇒ understaffed, and the two never
>   disagree. Both hunt sites (wild/pastoral and pen) and the assign-time forecast seed
>   (`fauna::forecast_source_yield`, off `SourceYieldForecast::ceiling_for`) read this one helper. **Wild
>   hunting** gets the same steady haul crew (`herders_needed == 0`, so `max()` collapses to it) — so a
>   wild herd's `workers_needed` is the client max-useful too. **Forage is untouched** — a gather is
>   continuous (`body_mass_yield == 0`, no lumpiness), so it keeps the ordinary `workers_needed_for_take`
>   overstaffing inversion.
> - **Wild hunting is untouched, deliberately.** No maintenance (the herd isn't yours), but it keeps
>   its carry cap. **The models differ because the products differ: hunt = reach + carry; harvest =
>   maintain + take.**
> - **Understaffing degrades PROPORTIONALLY — never a binary escape.** `herded_fraction = min(1,
>   assigned / needed)` scales the `animal:pastoral` rung's `decay_per_turn`, mirroring
>   `pen_fed_fraction → starve_shrink_rate`. Floored, and **recoverable**: re-staffing stops the bleed
>   outright, under *any* policy. Binary escape survives **only** for total abandonment (zero herders).
> - **Maintenance is scoped to `is_corralled() || owner.is_some()`, NOT `is_domesticated()`** — a trap
>   worth naming: `is_domesticated()` is a threshold at `1.0`, so the first under-herded turn drops the
>   herd under it, the proportional rule stops applying, and it decays at the **full** abandonment rate
>   however many herders you assign. The proportional regime would be measure-zero and "recoverable"
>   would be false.
> - **`decay_domestication`'s `is_domesticated()` early return is DELETED.** It made a tamed herd
>   permanently tame, forever, for zero labor — "you need constant herders" was false at the top of the
>   ladder. A properly-herded herd does not decay (including one you are merely *harvesting*); an
>   under-herded one does. Pinned by
>   `fauna_husbandry::{a_properly_herded_tamed_herd_does_not_decay_under_a_harvest_policy,
>   an_under_herded_tamed_herd_decays_proportionally_and_recovers}`.

**Retired: single-task model → labor allocation (Early-Game Labor slice 3a).** The
one-task-per-band model (`reassign_band` + `HarvestAssignment`/`ScoutAssignment`/`FaunaPursuit`
and their systems `advance_harvest_assignments`/`advance_scout_assignments`/`advance_fauna_pursuits`,
plus the `scout`/`forage`/`hunt_fauna`/`follow_herd` command handlers) is **removed**. A band is now a
**labor pool**: a `LaborAllocation` component (`components.rs`) partitions its whole working-age workers
(`available_workers(working)` = `floor`) across `LaborTarget`s — `Forage { tile, policy }`, `Hunt { fauna_id,
policy }`, `Scout`, `Warrior` — with the invariant `Σ workers ≤ available`. `advance_labor_allocation`
(`systems.rs`, Population stage, replacing the three retired systems) resolves per-worker yields each
turn: Forage = `workers × per_worker_yield × seasonal_weight` from an in-range `FoodModuleTag` tile;
Hunt take = `min(workers × per_worker_biomass_capacity, policy_ceiling)` (reusing the per-policy ecology
ceilings — Sustain under-hunting lets a herd grow), tracking a roaming herd out to `band_work_range +
hunt_leash_tiles` before the assignment lapses (feed entry). Scout extends the band's live sight range
in `calculate_visibility` by posting forward-observer vantages (`scout.vantage_distance(scouts)` out
in all 6 hex directions, LOS revealed from each — re-marked Active every turn while scouts are
staffed, scaling with head-count); Warrior is inert until the predator slice. `move_band <faction> <band> <x> <y>` sets a `BandTravel` component that
`advance_band_movement` steps at `band_move_tiles_per_turn`/turn. `assign_labor` sets one target's
worker count (0 unassigns; clamps to free headroom); `cancel_order` clears all assignments + stops
movement (fully idle). The snapshot exports `laborAssignments`/`idleWorkers`/`workingAge`, and still
summarizes `activity` (target-kind with most workers) + `huntMode` (largest Hunt's policy) for the
pre-3b client. Husbandry re-homes here — but **Sustain no longer tames** (slice 3a): a **`Tame`** Hunt
fills the meter, while any stewardship policy on a Thriving source earns the knowledge that source's
current **rung** teaches (slice 4 — see "The knowledge pattern"). The
**investment policies** `Cultivate` (Forage-only) / `Corral` (Hunt-only) also resolve here — a reduced
take while the improvement is prepared, then the managed yield; see "Cultivation" / "Corral". Config:
`labor_config.json`. Client allocation panel is PR 3b.

**Ecology — critical-depensation collapse (Phase D)** — `advance_herds` applies one
turn of `net_biomass_delta` (`fauna.rs`) toward each group's per-species carrying
capacity (`Herd.carrying_capacity` = the species' `biomass[1]`). The curve is **not**
plain logistic: above the Allee threshold (`ecology.collapse_fraction * cap`) the group
regrows logistically at `ecology.regrowth_rate`; **below** it the group is non-viable and
declines by `ecology.collapse_rate` per turn — an **irreversible crash to local
extinction even if hunting stops** (the overhunting point of no return). `advance_herds`
**despawns** any group below the viability floor (`ecology.extinction_floor * cap`), so a
collapse reaches zero in finite turns. So a hunt/follow draws a group down in
`Population`; it regrows (or, past the threshold, collapses) in the next turn's
`Logistics`; sustained overhunting drives it extinct permanently.

**Ecology phase + domestication hook** — each `Herd` carries a coarse `EcologyPhase`
(`Thriving` / `Stressed` / `Collapsing`), recomputed every turn from biomass vs
`ecology.stressed_fraction`/`collapse_fraction` (`classify_ecology_phase`) and exported in
the snapshot (`HerdTelemetryState.ecologyPhase`) so the client warns the player before a
group is doomed. This derived state also **gates domestication** (below): husbandry
progress accrues only while a `Thriving` herd is Sustain-hunted (a Sustain Hunt assignment).

**Immigration** — `repopulate_fauna` (`fauna.rs`, `TurnStage::Logistics` right after
`advance_herds`) gives a low per-turn chance (`immigration.chance_per_turn`) to respawn one
short-range game group up to `abundance.max_total_game`, sampling up to
`immigration.max_attempts` random land tiles that host game and respect `min_spacing`. This
keeps an overhunted map slowly replenishing (early forager play stays game-rich) without
undoing a local extinction (the crashed group is gone; a *new* group may immigrate
elsewhere). Seeded per-turn from `map_seed ^ tick ^ salt` (deterministic under rollback).

**Domestication / husbandry (Phase E)** — the pastoral counter-force to depletion. A
`Herd` carries `domestication_progress` (0–1, `1.0` = domesticated) and `owner:
Option<FactionId>`, exported as `HerdTelemetryState.domestication`.
- *Accrual — the **`Tame`** verb, not a side effect of hunting*: in `advance_labor_allocation`
  (Population), a Hunt assignment carrying **`FollowPolicy::Tame`** on a **Thriving** herd adds the
  `animal:pastoral` rung's `progress_per_turn` × the species' `taming_rate` for the acting faction
  (sets `owner` on first accrual; only the owner accrues; gated on **Herding** + the species'
  husbandry ceiling). At `1.0` the herd domesticates. **A `Sustain` hunt tames nothing** — it only
  *teaches* the faction Herding. That de-conflation is slice 3a; see "The `Tame` verb".
- *Decay*: `advance_husbandry` (`fauna.rs`, `TurnStage::Logistics` after `advance_herds` — runs
  *before* the same turn's accrual, so a `Tame`-worked herd nets `progress_per_turn − decay_per_turn`
  and an abandoned one only decays by the `animal:pastoral` rung's `decay_per_turn`, clearing `owner`
  at 0).
- *Yield*: **none here — passive-free pastoral is RETIRED** (intensification ladder slice 3b, §3:
  every rung is worker-driven). A tamed herd used to pay its owner the pastoral MSY **with no worker
  at all**, split evenly across the owner's bands; `advance_husbandry` now pays **nothing** and a
  pastoral herd yields **only** through a normal `Hunt` assignment, exactly like a wild one. The
  taming payoff is **yield per worker**: `herd_ecology` puts a tamed herd on the pastoral ecology
  (`r` = wild × `pastoral_gain` 2.0), so the *same* hunters take ~2× the sustainable food from the
  same `K` (measured: `fauna_husbandry::the_husbandry_ladder_is_a_per_species_growth_rate_ladder`).
  That also deletes the "you are not paid twice" hazard structurally — with no second payment to
  stack, the `Corral` dip is a real cost again — so the `Herd::worked_this_turn` flag that guarded it
  is **gone**.
- *Collapse immunity*: `regrow_biomass` uses plain `logistic_regrowth` (never the collapse
  branch) for a domesticated herd — a managed group recovers and never crashes.
- *No early claim*: the `domesticate <faction_id> <herd_id>` command, its `husbandry.claim_threshold`
  lever and `Herd::claim_domestication` are **deleted** (slice 3a). Snapping progress to `1.0` let the
  player skip the investment, which is the entire decision — the plant side removed its twin for the
  same reason. **Proto field 30 is reserved and must never be reused.** The `tame` command that
  replaced it *sets the `Tame` policy*; it claims nothing.
- *Practice teaches the next rung*: working this herd under a **stewardship** policy earns the faction
  the knowledge its **current rung** teaches — **Herding** while it is wild, **Penning** once it is
  pastoral (slice 4, §4). See "The knowledge pattern".
- `HerdRegistry::domesticated_count(faction)` is the seam the future `SedentarizationScore`
  (`TASKS.md`) reads for its "domestication progress" input.

### The husbandry yield ladder — every rung pays MSY

Authoritative design: `docs/plan_corral_managed_population.md`. **Management buys a *growth rate*, not
a licence to eat the standing stock.** Every rung of the ladder pays the Maximum Sustainable Yield; the
rungs differ *only* in the **ecology** that MSY is computed against, and in what that ecology costs you.
**Every rung costs a worker** (intensification ladder §3): what climbing buys is **yield per worker**,
not a rung that works itself.

**The husbandry ladder scales BOTH `r` AND `K`, on two orthogonal per-species dials.** `r` (the growth
*rate*) climbs on the global gains `husbandry.pastoral_gain` / `pen_gain` (folded per-species by
`herd_ecology`). `K` (the carrying *ceiling*) climbs on the per-species **density** dials
`SpeciesDef.pastoral_density` / `pen_density` (`fauna_config.json`, default **1.0** = neutral, so a wild
herd is byte-identical) — *domestication makes the land hold more animals, non-linearly by species*.
Without the density axis a species on marginal range (a goat penned at `K≈24`) stayed tiny even penned
and a fast wild breeder out-yielded the prime domesticates at every rung, because taming touched only
`r`. The gain for the herd's **current rung** is `fauna::herd_density_gain` (corralled → `pen_density`,
tamed → `pastoral_density`, wild → `1.0`; mirrors `herd_ecology`'s dispatch, resolved live by display
name via `FaunaConfig::pen_density_for` / `pastoral_density_for`), applied at the **one K seam**
`ecological_carrying_capacity` — it multiplies the final range/footprint-derived `K` (`Some(flow /
fodder × gain)`), so it is recomputed fresh each turn (idempotent, never a compounding read). Roster:
crag_goat/aurochs **2.0 / 5.0**, boar **1.5 / 4.0**, rabbit/fowl **1.1 / 1.5**,
steppe_runner/marsh_grazer **1.5 / 1.0** (pastoral only, so `pen_density` is inert), deer/mammoth omit
both (wild ceiling → always `×1.0`). Validated finite & `>= 1.0` (a gain below 1 would make
domestication *reduce* capacity). **Playtest dials.** The density axis is **scale-free in the pen's
net-positive floor** (`K` cancels in `r·K/4·p` vs `u·K·(2+r)/4`), so it does not interact with that
invariant.

> #### The ladder is monotone in the LONG-RUN rate, NOT in any single turn — do not "fix" this back
>
> Since slice 8 a **Sustain** hunt is **constant escapement on whole animals**: the herd hands over
> `B − K/2`, which is a **stock**, not a rate. At `B = K` Sustain's escapement is `K − K/2`
> = **`K/2` for every rung — `r` cancels out entirely**, so a full herd's first harvest is *identical*
> wild, pastoral and penned. **That is correct and load-bearing.** The surplus standing above the
> escapement point is *accumulated stock*, and stock does not care how fast you breed. What the ladder
> buys is that **the next animal comes sooner** — so *"management buys a growth rate"* is now literally
> and exclusively true, rather than being smeared across a stock term.
>
> A single turn therefore cannot see the ladder at **either** biomass: at `B = K` you read the
> rung-blind stock, and at `B = K/2` you read a **pulse** — zero for any species whose one-turn MSY is
> lighter than one animal (a wild mammoth regrows 120 biomass against an **800-unit** body, so it
> correctly **waits** ~7 turns, then pays 16 provisions at once). Both readings are honest; neither is
> the ladder.
>
> So the invariant is asserted as a **long-run average over enough turns to contain the refills**
> (`fauna_husbandry::the_husbandry_ladder_is_a_per_species_growth_rate_ladder`, 600 turns from `B*`),
> where the pulses and the stock both wash out and what remains is `r·K/4`. **Measured**
> (provisions/turn, barren harness ⇒ the pen is fully larder-fed):
>
> | species | `K` | wild | pastoral | pen gross | upkeep | pen net |
> |---|---|---|---|---|---|---|
> | Rabbit Warren | 200 | 0.350 | 0.700 | 1.000 | 0.280 | **0.720** |
> | Red Deer | 1200 | 0.598 | 1.196 | 2.392 | 1.432 | **0.960** |
> | Thunder Mammoths | 12000 | 2.373 | 4.746 | 9.492 | 13.506 | **−4.014** |
>
> Monotone in gross at every row, and `pastoral / wild` is exactly `pastoral_gain` (2.0). The **rabbit
> pen gross rides the cap** (`r_pen = min(husbandry_regrowth_cap 1.0, 0.35 × pen_gain 4.0) = 1.0`), so
> its pen/wild ratio is `1.0/0.35 ≈ 2.86`, not the full `pen_gain` — a fast breeder is clamped into the
> stable logistic band, the cap's whole job. The mammoth's negative *barren* pen net is the §2.4
> slow-breeder loss **by design** (a placement decision — on real pasture the footprint feeds it and
> `upkeep → 0`), not a regression.

| Rung | Ecology | `r` (Grazing 2d — **per-species**) | Costs |
|---|---|---|---|
| Wild, Sustain hunt | `ecology` | `wild_r` (rabbit 0.35 · deer 0.10 · mammoth 0.04) | a worker |
| Mobile domesticated (**pastoral**) | `husbandry.pastoral.ecology` | `min(cap, wild_r × pastoral_gain)` (gain 2.0) | **a worker** (a Hunt assignment, like a wild herd — passive-free pastoral is retired) |
| Corral, building | the `animal:pen` rung's `yield_fraction_while_building × MSY` | — | a worker, 25 turns |
| Corral, finished (**pen**) | `husbandry.pen.ecology` | `min(cap, wild_r × pen_gain)` (gain 4.0, cap 1.0) | a worker + **feed (footprint-offset)** + pinned |

- **Grazing 2d retired the flat pastoral 0.25 / pen 0.90.** The managed rungs now scale each species'
  **own wild `r`** by `husbandry.pastoral_gain` (2.0) / `pen_gain` (4.0), clamped to
  `husbandry_regrowth_cap` (1.0) — a penned rabbit (`r` 1.0, cap-bound and booming) and a penned mammoth
  (`r` 0.16, a long-haul investment) are different economies. This also fixes the fast-breeder pastoral
  inversion (pastoral `r` = `wild_r × 2.0 > wild_r` for every species). `fauna::herd_ecology` folds the per-species
  rate in; `pen_ecology_for` / `pastoral_ecology_for` are the seams, `managed_regrowth_rate` the `wild_r ×
  gain → capped` map.
- **A penned herd's `K` is its FENCED FOOTPRINT's graze flow** (`hex_range_tiles(corralled_at,
  pen_radius)`), recomputed each turn — penned herds are no longer frozen and `pen.capacity_fraction` /
  `pen_capacity` are **deleted** (a penned herd's `K` is just `herd.carrying_capacity`, so
  `herd_capacity` collapses to that field for every herd). A penned herd **grazes its footprint**
  (escapement-floored, like a wild herd) and the grass it eats **offsets its keeper's larder bill**:
  `larder_upkeep = pen.upkeep_per_biomass × biomass × (1 − pasture_fraction)`, `pasture_fraction =
  clamp(footprint_intake / (fodder_per_biomass × biomass), 0, 1)`. A pen on lush steppe feeds itself for
  free (`pasture_fraction → 1`, larder → 0); a **wholly-barren** footprint keeps the herd's frozen `K`
  and pays the full larder bill (the pre-2d worst case, preserved). See "Phase 2d".
- **`fauna::herd_ecology(herd, fauna)` and `fauna::herd_capacity(herd, fauna)` are THE single source of
  that mapping.** `regrow_biomass`, `hunt_policy_ceiling`, `hunt_forecast`, `refresh_ecology_phase`,
  the expedition ceiling/bound/simulation — **every** consumer resolves through them. **No call site may
  re-derive an ecology or a capacity**: a second copy of this mapping is exactly how a forecast starts
  promising a number the take won't pay (see "Pre-commit Yield Forecast").
- **The managed harvest draws the herd down**, and that is what makes it sustainable: it converges the
  herd on `K/2` and holds it there, paying `r·K/4` forever. Both husbandry rungs take it through the one
  shared helper **`fauna::managed_yield_biomass`**.
- **The pastoral rung is worked, so it cannot be double-paid** (slice 3b). It *used* to pay its owner
  passively, and `advance_husbandry` had to **skip** that payment for any herd a labor assignment
  worked last turn (a `Herd::worked_this_turn` flag) — because without the skip a Red Deer under
  construction collected the `Corral` dip (0.50 × 1.50 = 0.75) **plus** the passive rung (1.50) =
  2.25/turn, *more* than the 1.50 of walking away, turning the pen's investment cost into a profit.
  Retiring the passive rung removes the hazard by construction (there is no second payment left to
  stack), so the flag and the skip are **deleted**. The dip is a real cost measured against the real
  alternative — hunting the same herd
  (`fauna_husbandry::building_a_corral_costs_more_than_hunting_the_same_herd`): building pays
  **0.75/turn for 25 turns (~19 provisions forgone)** against the 1.50 those same hunters would have
  taken, recouped ~9 turns after completion (pen ≈3.66 gross at `B*`).
- **It is constant-*escapement* MSY** — `take = min(peak_regrowth(K), max(0, B − K/2))` — **not** the
  constant-catch `sustainable_yield` a *wild* `Sustain` hunt takes. The sim regrows in Logistics and
  harvests in Population, so a constant-catch take is evaluated at the **post**-regrowth biomass; above
  `K/2` that is harmless (both forms cap at MSY and converge on `K/2`), but **below `K/2` it takes
  `g(B + g(B)) > g(B)`** — strictly more than the herd grew. At the wild `r` = 0.05 that leak is a
  rounding error; at the pen's fast per-species `r` (up to 0.75) it is fatal: a **fully fed** pen knocked below `K/2` spirals
  to zero in ~12 turns and can never recover. Escapement never takes a herd below `K/2`, so a depleted
  managed herd **rebuilds** (yielding less, or nothing, while it does) and then pays `r·K/4` forever —
  stable from *both* sides, same yield at capacity and at the operating point.
- A managed harvest therefore **never overdraws**: its yield telemetry reads `actual == sustainable`
  (no ⚠). Its `workers_needed` is **derived like every other rung's** (slice 7): the pen collapses the
  *policy* axis, never the **collection** cap — the keeper still carries the meat home, so the take is
  `min(pen MSY, hunters × hunt.per_worker_biomass_capacity)` and the surplus it offered beyond that is
  reported as `wasted`. The retired `TENDED_SOURCE_WORKERS_NEEDED = 1` claimed one keeper could collect
  a pen of any size.

Ecology/husbandry tunables live in the `ecology` (`regrowth_rate`, `collapse_fraction`,
`collapse_rate`, `stressed_fraction`, `extinction_floor`), `immigration`, and `husbandry`
(**`pastoral.ecology`**, **`pen`** — see "Corral" — plus the per-species growth gains) blocks of
`fauna_config.json`. **The pen's BUILD dials live in `intensification_ladder.json`** (the `animal:pen`
rung's `build` block), and as of slice 4 so do the **earned-knowledge dials** (the ladder-level
`knowledge` block — the old `knowledge_progress_per_turn` / `knowledge_completion_threshold`, which
`labor_config` duplicated verbatim) — see "The Intensification Ladder": both food webs climb on the same
numbers.
**`FaunaConfig` is validated** (`FaunaConfig::validate`, run inside `from_json_str`, so every load path
— builtin, default file, `FAUNA_CONFIG_PATH` override — is covered; the `expedition_config.rs` /
`crisis_config.rs` convention). A broken invariant is logged at **error** level
(`fauna_config.invalid_rejected`) and the known-good builtin is used instead. Enforced: **the pen's
best-case net-positive floor** (Grazing 2d §2.4 — `pen.upkeep_per_biomass < r_pen · p / (2 + r_pen)`
for the **fastest** species' `r_pen = min(cap, max_wild_r × pen_gain)`; a slow breeder or poor-pasture
pen may run at a **loss by design**, so the old every-pen guarantee is retired for a best-case sanity
floor), **the ladder is monotone as gains** (`pen_gain > pastoral_gain > 1`), ordered ecology phase
bands (`extinction_floor < collapse_fraction < stressed_fraction < 1`) in all three ecologies, every
`regrowth_rate > 0`, `husbandry_regrowth_cap > 0`, `0 ≤ pen.starve_shrink_rate ≤ 1`,
`hunt.provisions_per_biomass > 0`, and the follow/market bounds. (The **knowledge** bounds moved to
`LadderConfig::validate` with the dials in slice 4, where they hold for both webs at once.)

### The `Tame` verb (Intensification rung 2) — the grammar fix

**The animal twin of `Cultivate`**, and the correction the plant side already made
(`docs/plan_intensification_ladder.md` §4.1). Taming used to be a **hidden side effect of a harvest
policy**: one `Sustain` branch in `advance_labor_allocation` advanced Herding knowledge *and*
`accrue_domestication`, so the same action both taught and tamed, invisibly and for free — while the
*visible* verb (`Corral`) was disabled until the herd was already tame. Rung 2 is now an explicit,
gated, **paid** verb, so both food webs read the same:

| | plants | animals |
|---|---|---|
| rung 1 Sustain | earns **Cultivation** — *and tames nothing* | earns **Herding** — *and tames nothing* |
| rung 2 verb | `Cultivate` → tended patch | **`Tame`** → pastoral herd |
| rung 3 verb | *(`Sow` — a later slice)* | `Corral` → pen |

- **`FollowPolicy::Tame`** (wire key `"tame"`) — **Hunt-only** (`valid_for_hunt`; a Forage assignment
  carrying it is rejected at `assign_labor`, exactly as `Corral` is). It is an **investment** rung, so
  it is in [`FollowPolicy::HUNT_POLICIES`] (the herd's policy list the client previews) but **not** in
  `EXTRACTIVE` — an expedition can only take, never invest, so `send_hunt_expedition` rejects it and no
  `huntTripEstimates` row is emitted for it.
- **The investment.** While the meter fills, the take ceiling is the `animal:pastoral` rung's
  `yield_fraction_while_building × the herd's Sustain (MSY) ceiling` (`hunt_policy_ceiling`, through
  the *same* shared MSY helper — the crew is gentling the herd, not harvesting it; a fraction of MSY is
  a sustainable draw, so the herd stays healthy), and `domestication_progress` accrues that rung's
  `progress_per_turn` **× the species' `taming_rate`** (slice 3c — 0.04 × 1.0 → 25 turns for a rabbit, × 0.2
  → 125 for a Steppe Runner) via the shared `RungDef::build_accrual` seam. **Gates:** the
  faction knows **Herding**, the species' `husbandry_ceiling` allows domestication (Grazing 2d-δ), and
  the herd is **Thriving**. A gate that lapses mid-run just **stops accrual that turn** — progress is
  neither lost nor silently switched, and the herd is marked `tamed_this_turn` so it does not decay
  either. Ownership is **not** in the gate: `accrue_domestication` owns the
  `owner is None || owner == faction` rule, exactly as `accrue_cultivation` does on the plant side.
  Accrued **after** the take (mirroring Cultivate/Corral), so the turn pays what the forecast promised.
- **Feral if abandoned.** `advance_husbandry` spares a herd worked under `Tame` **last** turn
  (`Herd::tamed_this_turn` — a transient, non-persisted flag, the animal twin of
  `ForagePatch::tended_this_turn` with the same deliberate Logistics-reads-what-Population-wrote lag);
  an abandoned part-tamed herd bleeds the `animal:pastoral` rung's `decay_per_turn` **× the same
  species `taming_rate`** (slice 3c — the multiplier is a *timescale*: a Steppe Runner forgets 5×
  slower than a rabbit, so the rung's 4:1 build:decay ratio holds for every species and the ladder's
  "taming must out-run its decay" bound needs no per-species restatement — only `taming_rate > 0`,
  which `FaunaConfig::validate` enforces), its owner lapsing at zero. **Distinct from an ordinary hunt
  at any other policy**: a Sustain hunt *harvests* a herd, it must not hold the taming meter up.
- **`tame <faction_id> <herd_id>` command** (`handle_tame`; `TameCommand` proto field **40**,
  `CommandEventKind::Tame`) — **sets the `Tame` policy** on the bands already hunting the herd, the
  command form of the client's policy picker. It **tames nothing outright**. It targets a **herd id**
  (not a tile like `corral`): taming is the verb you reach for on a *roaming* herd, identified by who
  follows it, not by where it stands this turn. Rejections, each distinct (`validate_tame`, shared with
  the `assign_labor … tame` path): faction hasn't learned Herding / no such herd / the species is wild
  game (hunt-only) / already domesticated (corral it instead) / another people are taming it / **no
  band is hunting it** (staff it first). Deliberately **not** gated on Thriving, unlike the patch — a
  herd's phase swings as it is hunted, and the labor arm pauses accrual gracefully.
- **The `domesticate` early-claim is REMOVED** — command, `claim_threshold` lever, its validate bound,
  and `Herd::claim_domestication`. It let the player snap progress to `1.0` and skip the investment,
  which is the entire decision (the plant side removed its twin for that exact reason). **Proto field
  30 is reserved and must never be reused.** Tests that needed a tamed-herd fixture now run the real
  `accrue_domestication(faction, RUNG_COMPLETE)`, which obeys the husbandry ceiling — you cannot
  fabricate a domesticated `wild` herd.
- **Per-species pace (slice 3c).** The rung's single `progress_per_turn` tamed *every* species in the
  same 25 turns — a rabbit cost what a Steppe Runner cost. The species now declares its own
  **`taming_rate`** (`fauna_config.json`, default 1.0), and `RungDef::build_accrual`/`build_decay` take
  it as a **build timescale** — the one seam that honors it, so the Tame arm of
  `advance_labor_allocation` and the decay in `advance_husbandry` cannot disagree. Scaling *both* rates
  is load-bearing, not tidiness: a progress-only multiplier would put a Steppe Runner at
  `0.04 × 0.2 = 0.008`/turn against the rung's `0.01`/turn decay — **literally untameable**. Every
  other rung passes `RUNG_TIMESCALE_UNSCALED` (penning is a flat 25 turns for every species — a fence
  is a fence; only *taming* varies). See the `fauna_config.json` row for the roster.
- **Config** — the whole rung is `intensification_ladder.json`'s `animal:pastoral` record: verb `tame`,
  `unlock_knowledge: "herding"`, **`earns_knowledge: "penning"`** (slice 4 — a config edit, exactly as
  promised),
  `ceiling_required: "pastoral"`, `build: { progress_per_turn 0.04, decay_per_turn 0.01,
  yield_fraction_while_building 0.50 }`. The first two **moved verbatim** from `fauna_config.json`
  `husbandry`; the dip is **new** (a **playtest dial** — 0.50 mirrors the animal-side `corral` precedent).
- **Slice 3b landed the rest of the rung:** passive-free pastoral is **retired** (a tamed herd yields
  only through a worker's Hunt assignment, at the pastoral `r` — see "Domestication / husbandry" and
  "The husbandry yield ladder") and the **`drift_to_owner`** movement primitive is live (see "Herd
  movement is a rung primitive").
- **Slice 4 completed the rung's knowledge half:** practising it (working the resulting **pastoral**
  herd under a stewardship policy) earns **Penning**, which now gates `corral` — so Herding gates
  `tame` and **only** `tame`. See "The knowledge pattern".

See Also: "Cultivation (Intensification Phase 1a)" (the plant rung 2 this now mirrors exactly), "Corral
(Intensification Rung 1c)" (the rung above), "The Intensification Ladder" (the engine + the config).

### Corral (Intensification Rung 1c)

The **animal mirror of the tended patch** (`docs/plan_intensification.md` §4b) — the place-bound form
of the *existing* herd domestication, and the fauna-side twin of "Cultivation" under Depletable
Forage. Taming a herd is *symmetric* with preparing a patch, but the **product differs and that
difference is the settle mechanic**: an *un*corralled domesticated herd stays **mobile** (pastoralism
travels with the band); **corralling pins it**. Like Cultivate, corralling is an **explicit `Corral`
policy with an investment cost** — not a free command. A `Herd` carries `corral_progress: f32` (0–1,
the pen under construction), `corralled_at: Option<UVec2>` (`Some` = penned at that tile) + a transient
`corralled_tended_this_turn` flag. *Sim-only — the client readout is a follow-up (see below).*

- **Rung-3 earned-knowledge gate — PENNING** (slice 4's §4.3 reshuffle; **it was Herding**). *Learned
  by doing* and **never start-granted**: working a **pastoral** (tamed) **Thriving** herd under a
  stewardship policy accrues faction **Penning** knowledge (discovery `PENNING_DISCOVERY_ID` = 2006,
  `fauna.rs`) at the ladder's `knowledge.progress_per_turn` — *"you learn penning by managing tamed
  herds"*. The **`Corral` policy** (and the `corral` / `extend_pen` commands, which ride the same
  `animal:pen` rung) is refused until the faction knows it; every gate resolves the id off the rung
  record, never a literal. The `penning` tag → discovery 2006 mapping is declared in
  `start_profile_knowledge_tags.json` purely so it is mappable; **no start profile lists it**
  (guarded by `start_profile::tests::no_start_profile_grants_a_ladder_knowledge`).
  **The old Cultivation asymmetry is gone:** taming is no longer ungated (Herding gates `Tame`), so
  both webs now gate rung 2 on the knowledge rung 1 teaches, and rung 3 on the knowledge rung 2
  teaches. One knowledge per transition. See "The knowledge pattern".
- **The `Corral` policy — the investment.** In `advance_labor_allocation`'s **Hunt** arm, a herd worked
  under `FollowPolicy::Corral` (Hunt-only) **costs a yield dip while the pen is built**: the take
  ceiling is the `animal:pen` rung's `yield_fraction_while_building × sustainable_yield(..)` (`hunt_policy_ceiling`,
  reusing the **shared** MSY helper — the crew is building, not hunting; a fraction of MSY is a
  sustainable draw, so the herd stays healthy) and `corral_progress` accrues
  that rung's `progress_per_turn` (0.04 → 25 turns). **Gates:** the faction knows **Herding**
  AND owns the **domesticated** herd; a gate that lapses **mid-build** just stops accrual that turn
  (progress is kept — a half-built pen is materials on the ground; unlike cultivation it does **not**
  decay *gradually*). That "progress is kept" applies to a **mid-build** lapse only — a **completed
  pen whose herd escapes loses its progress outright** (reset to `0.0`; see *Escapes-if-untended*
  below). Accrued **after** the take, so the turn pays exactly what the forecast promised. At `1.0`
  `Herd::corral_at` pens it (sets `corralled_at`, stops roaming, grants the one-turn tended grace) and
  pushes a `CommandEventKind::Corral` feed line.
- **`corral` command (repurposed)** — `corral <faction> <x> <y>` (`handle_corral`; unchanged
  proto/runtime/text plumbing, `CommandEventKind::Corral`, `CorralCommand` proto field 38) now **sets
  the `Corral` policy** on the band(s) already hunting the herd standing on that tile — the command
  form of the client's policy picker. It **pens nothing outright**. Rejections: no herd there / faction
  hasn't learned **Penning** ("…have not learned Penning yet. Tame and keep herds to learn it.") / not
  domesticated / not the owner / already corralled / **no band is hunting it** (staff it first). Same
  gates as the `assign_labor … corral` path (`validate_labor_policy`).
- **The pen is a managed POPULATION** (`docs/plan_corral_managed_population.md`): its yield follows the
  animals you actually keep, those animals **eat** every turn, and underfeeding **shrinks** the herd. A
  one-off 25-turn build that then printed food forever is now a **sustained commitment with a running
  cost**. Corralled = fixed + place-local worker-tended + **fed** + escapes-if-untended:
  - *Fixed* — `advance_herds` skips a corralled herd's `advance_herd_roam` (it stays at `corralled_at`,
    no heading arrow); it still grazes its footprint + regrows toward the footprint's `K` (Grazing 2d).
    Since slice 3b this is **read off the `animal:pen` rung's `behavior.movement: fixed`**, not
    hard-coded on `is_corralled()` — see "Herd movement is a rung primitive".
  - *Place-local worker-tended* — a **Hunt assignment on a corralled herd** is herding/tending it, and
    the turn has two halves (the tend branch of `advance_labor_allocation`'s Hunt arm, which `continue`s
    before `hunt_take` — a corralled herd is never both hunt-drawn AND paid):
    1. **FEED (footprint-offset, Grazing 2d §2.3).** The pen grazes its fenced footprint
       (`advance_herd_grazing` → `footprint_intake`), and the larder pays only what the pasture can't
       cover: `demand = pen.upkeep_per_biomass × biomass × (1 − pasture_fraction)`,
       `pasture_fraction = clamp(footprint_intake / (fodder_per_biomass × biomass), 0, 1)`.
       `LocalStore::take` returns what it *actually* took; `pen_fed_fraction = pasture_fraction +
       (1 − pasture_fraction) × (paid / demand)` (the total fed share — pasture plus the paid part of
       the reduced larder bill). A lush footprint feeds the pen for free; a barren one pays the full
       bill — **the tether that gives "the pen pins the band" its teeth**, now cheap on good land.
    2. **HARVEST.** The keeper takes the **pen's MSY** (`fauna::pen_yield_biomass` →
       `managed_yield_biomass` under the herd's per-species pen ecology (`pen_ecology_for`), against its
       footprint `K` = `herd.carrying_capacity`), which **draws the herd
       down** — exactly what makes it sustainable (see "The husbandry yield ladder"). The credited yield
       is **gross**: the feed is a separate debit, so the player sees both halves of the trade rather
       than one netted number.
  - *Starves if underfed* — `advance_husbandry` reads last turn's `pen_fed_fraction` and, if the keeper
    could not pay, shrinks the herd by `pen.starve_shrink_rate × (1 − fed) × biomass`, floored at
    `pen.ecology.extinction_floor × K_pen`. **The pen's growth is what the feed buys**: `regrow_biomass`
    scales a penned herd's growth by `pen_fed_fraction`, so an unfed pen does **not** grow (without this
    the pen's own fast `r` out-runs the 10%/turn wasting several times over — an "unfed" herd would keep
    growing and quietly pay a yield for feed nobody bought). The herd **withers to a remnant and
    recovers when fed again**: it does **not** despawn (a penned herd is exempt from `advance_herds`'
    dispersal retention — dispersal is the *mechanism* of local extinction, and a confined herd cannot
    disperse) and it does **not** lose the pen. Deliberate: a recoverable famine the player can see and
    fix is better play than silently voiding a 25-turn investment. It is **never silent** — an
    edge-gated `CommandEventKind::Corral` feed line fires on the turn the famine *starts*
    (`"The <species> herd is starving — the pen has no feed"`, detail `status=starving fed=<f>
    action=corral herd=<id>`), not every turn it continues. **Starving your animals to feed your people
    becomes a *decision*, not an accident.**
  - *The decision this creates* — the pen stops being a strictly-dominant upgrade and becomes a **wager
    on staying**: it out-pays every other rung, but only while you feed it, every turn, forever — and
    its food cost lands **exactly when food is scarce**, so a bad winter forces a real choice (eat the
    seed corn and lose future yield, or go hungry).
  - *Escapes-if-untended* — in `advance_husbandry` (Logistics, which runs *before* Population — a
    deliberate one-turn-lag flag, exactly like `ForagePatch::tended_this_turn`) a corralled herd
    tended last turn is spared; an **untended** one **escapes**: `corralled_at` is cleared, **and
    `corral_progress` is reset to `0.0`**, reverting it to a mobile domesticated (pastoral) herd —
    which, since slice 3b, pays **nothing at all** until a band hunts it again. **The pen is lost, not
    merely opened** — re-penning pays the
    full 25-turn `Corral` investment again, at the herd's new position. *Why zero, when a patch's
    `cultivation_progress` only decays gradually:* **a patch is a place and a herd is not.**
    `cultivation_progress` can survive partially because the improvement sits on a tile that cannot
    move, so leftover progress still refers to the same patch; `corral_progress` lives on the **herd**,
    which roams — so any retained progress would re-materialize the pen at whatever tile the animal has
    since wandered to (a teleporting corral) and make abandoning a pen cost **one** turn instead of the
    rebuild. Losing the pen is what makes the tending obligation real (the "pins the band" mechanic).
    Because the escape now **destroys a 25-turn investment**, it is **never silent**: it pushes a
    `CommandEventKind::Corral` feed line to the owner — the same kind the pen's *completion* pushes
    (one kind for the pen's whole life) — reading `"The <species> herd broke out — untended, the pen
    is lost"` (human text names the **species**, never the internal herd id) with
    `status=escaped reason=untended action=corral herd=<id> x=<x> y=<y>` in the detail field.
    A corralled herd is exempt from the pastoral even-split here (it's paid place-local by its keeper).
    `corral_at` grants a one-turn grace so a freshly-penned herd doesn't escape before its keeper can
    tend it. **This binary escape is the *no-keeper* case only** — nobody is minding the gate. A keeper
    who is present but *broke* starves the herd instead (above); it never breaks out.
- **Persistence** — `corralled_at`, `corral_progress`, **and `pen_radius` / `pen_extend_progress` /
  `pen_extending` (Grazing 2d)** round-trip through the rollback snapshot on `HerdState` (authoritative
  sim state), so a rollback rewinds a half-built pen (or an in-flight fence extension) rather than losing
  the investment;
  `corralled_tended_this_turn`, **`pen_fed_fraction`, `pen_starving`, `footprint_intake` and
  `pen_pasture_fraction`** are transient (not persisted) — a rehydrated pen reads "tended (a
  deliberate one-turn grace, seeded in `herd_from_state` so the first post-restore Logistics escape
  pass spares a pen a keeper tends every turn), fully fed", so a rollback can only *delay* an escape or
  a starvation turn by one turn, never invent a famine — and never *destroy* a standing pen on restore.
- **Config** (`fauna_config.json` `husbandry`): the **`pen`** block — `ecology` carries **phase bands
  only** now (its `regrowth_rate` is unused; the pen `r` is per-species — Grazing 2d),
  **`upkeep_per_biomass` (0.002 — the feed, now footprint-offset)** and `starve_shrink_rate` (**0.10** —
  a fully-unfed herd loses 10%/turn); `capacity_fraction` is **deleted** (`K_pen` is the fenced
  footprint's graze flow). Plus the **per-species growth gains** `pastoral_gain` (2.0) / `pen_gain`
  (4.0) / `husbandry_regrowth_cap` (1.0), **`pen_radius_max`** (2 — the `ExtendPen` fence cap, 2d-β,
  validated `>= 1`), the **`pastoral`** block (phase bands only). **The pen's
  investment cost and build rate moved to `intensification_ladder.json`'s `animal:pen` rung** — the old
  `corralling_yield_fraction` 0.50 is its `yield_fraction_while_building`, the old
  `corral_build_progress_per_turn` 0.04 its `progress_per_turn` (unchanged values) — and in **slice 4**
  the earned-knowledge levers `knowledge_progress_per_turn` (0.05) / `knowledge_completion_threshold`
  (1.0) moved to that file's ladder-level **`knowledge`** block at the same values, `labor_config`
  having duplicated them verbatim (see "The Intensification Ladder").
  `claim_threshold` is **deleted** with the `domesticate` early-claim it gated (slice 3a — it let the
  player skip the investment). The retired flat rates
  `provisions_per_biomass` (0.01) / `corral_provisions_per_biomass` (0.012) and `fauna::corral_provisions`
  are **deleted**.
  - **Retuned once, against measurement** (a scripted 100-turn campaign on three pinned seeds — the
    default `map_seed` is `0`/entropy, so a probe *must* pin one): the first cut (`pastoral` 0.15,
    `pen` 0.60, dip 0.25) left a freshly-taming band at income **1.275** vs consumption **1.294** — a
    permanent one-day-of-food treadmill, no savings, no affordable expedition — and made the pen
    reachable only through a **~50% population crash** (the build dip had to be paid out of a famine).
    The shipped values put the pastoral rung clearly *above* subsistence (a real surplus) and let the
    pen's dip be paid from it. **`upkeep_per_biomass` was deliberately NOT touched** — the running cost
    is the point of the arc, and weakening it to fix balance would delete the mechanic.
  - **Every invariant above is enforced by `FaunaConfig::validate()`** — most importantly
    the pen's **best-case net-positive floor** (Grazing 2d §2.4 — `upkeep_per_biomass < r_pen · p /
    (2 + r_pen)` for the **fastest** species' `r_pen = min(husbandry_regrowth_cap, max_wild_r ×
    pen_gain)` = `min(1.0, 0.35 × 4.0) = 1.0`, so the bound is `1.0 × 0.02 / 3.0 ≈ 0.0067`; shipped
    0.002): derivation — at the operating point the
    pen yields `r·K/4 · p` and eats `u · K·(2 + r)/4`, so `net > 0 ⟺ u < r·p/(2 + r)`. **This inverts
    the old every-pen guarantee:** with per-species `r` and pasture-dependent feed, a slow breeder or a
    poor-pasture pen may run at a **loss by design** (a placement decision), so validate only guarantees
    the best pen (fastest breeder, fully larder-fed) still pays. See "The husbandry yield ladder".
- **The band's food ledger — `PopulationCohortState.penFeedUpkeep` (the per-band roll-up).** A pen's
  feed is taken straight off `cohort.stores` (`LocalStore::take`, the corral-tend branch), so it lands
  in **neither** `foodIncome` (Σ per-source `actual`) **nor** `foodConsumption` (the food the *people*
  actually ate — `PopulationCohort::last_food_consumption`, the real opening-brackets `stores` debit,
  the symmetric twin of this pen debit; **not** a post-turn `food_demand`, which the same turn's
  births would inflate). A band keeping a pen would therefore display a surplus **overstated by exactly the
  upkeep** — on a Red Deer pen a phantom **+1.74/turn** against a band that eats ~1.2 — and the player
  would watch the larder drain unexplained. `penFeedUpkeep` is **the food the band actually PAID** this
  turn (the summed `LocalStore::take` *return*, not the demand — a band that can only part-pay reports
  only what it handed over, and its herds starve for the rest), carried on
  `LaborAllocation::last_pen_feed_upkeep` (derived per-turn, not persisted, excluded from equality —
  same treatment as `last_yields`). It closes the identity
  ```text
  larder_delta == foodIncome − foodConsumption − penFeedUpkeep
  ```
  which `integration_tests/tests/pen_food_ledger.rs` pins against a **real turn** through the real
  systems and the real snapshot export, both fully-fed and part-fed. **It is deliberately NOT folded
  into `foodConsumption`**: "my people ate X" and "my animals ate Y" are separate lines, and that
  separation is the readout this arc exists to give. The sim answers the number so the **client does
  zero arithmetic** (it must not sum `penUpkeep` across herds itself) — the same discipline as the
  Pre-commit Yield Forecast.
- **Display snapshot (on the wire).** The corral state is exposed to the client stream on both
  `WorldSnapshot` and `WorldDelta` (`snapshot.fbs`, `sim_schema`, `snapshot.rs`
  `herd_snapshot_entries`): `HerdTelemetryState.corralled:bool` (= `Herd::is_corralled()`) and
  **`corralProgress:float`** (0..1, the pen-building meter — the animal twin of
  `ForagePatchState.cultivationProgress`), plus **`penUpkeep:float`** and **`penFedFraction:float`**.
  Both are **per-herd** (the herd drawer + the starving warning):
  - **`penUpkeep`** = the feed this pen **demands, or would demand once built**, at the herd's
    **current** biomass (`pen.upkeep_per_biomass × biomass`) — a *projection* for an unpenned herd, the
    *live* demand for a penned one. It is **always meaningful, never `0`-because-unpenned**, and is
    computed on the **same biomass basis** as `corralYield`, so the two are a **matched pair the client
    subtracts**. That is the point: the pre-commit `Corral` row is by definition looking at a herd that
    is *not yet penned*, so a `0` there would quote the payoff while hiding the running cost at the one
    moment the running cost should drive the decision — the same defect class as advertising the
    **gross** yield (a preview quoting a number the player will never bank). At or below `K/2` the
    projected `corralYield` is honestly `0` (escapement — the pen pays nothing until the herd
    rebuilds).
  - **`penFedFraction`** = last turn's fed fraction (`1.0` = fully fed, `< 1` = **starving** — the herd
    and its yield are shrinking, and it recovers when fed again).
  - **Demanded ≠ paid** (load-bearing): a starving pen demands more than it is paid, and
    `penFedFraction` is that ratio. The band's **actual** ledger debit is the per-band
    `PopulationCohortState.penFeedUpkeep` (the real `LocalStore::take` amount) — the food ledger draws
    **that**, never `penUpkeep`. So no consumer needs a "0 when unpenned" reading, and one field with
    one meaning beats two that must be kept in lockstep.

  Plus the forecast pair `ceilingCorral` / `corralYield` (see
  "Pre-commit Yield Forecast"). See "Intensification display snapshot" under Cultivation for the
  plant-side + faction-knowledge fields.
- **Follow-up (final Phase-1 slice):** the **client _rendering_ for both ladders** — cultivation +
  Cultivation-knowledge + tended-patch on the plant side, and domestication + Herding-knowledge +
  corral on the animal side — is the last remaining client-dev slice (the data is now all on the wire).
  **Phase 1b of the managed-population arc rides with it:** the pen's `penUpkeep` as a *negative* row in
  the band's food ledger, the `penFedFraction` starving warning, and the corrected policy hints.
  `docs/plan_corral_managed_population.md` §6 — **Phase 1a (the sim) must not ship to a player without
  1b**, only to `main`: without the readout the player watches their larder drain with no explanation.
  **Phase 2 (deferred):** the pen's upkeep is drawn *first* from the tile's `ForagePatch` biomass (the
  animals eat grass — a resource humans can't), and only the **shortfall** is hauled from the larder.

See Also: "Cultivation (Intensification Phase 1a)" under Depletable Forage — the plant twin of this
mechanic (the two are near-mechanical transposes).

> `FaunaPursuit` is **not** snapshot-persisted (unlike `HarvestAssignment`): a
> `rollback` mid-pursuit cleanly cancels the in-flight hunt (the rehydrated cohort
> simply lacks the component). Pursuits are short-lived; revisit if needed. Domestication
> state lives on the `Herd` (in `HerdRegistry`), alongside `biomass`.

> **The authoritative `HerdRegistry` *is* rollback-persisted** (as of the intensification
> arc's first slice, `docs/plan_intensification.md` §0-i). Each live `Herd` — identity,
> movement (`route`/`step_index`/`current_pos`/`dwell_remaining`/`roam`/`next_pos`/`corralled_at`),
> **and** its depletable-ecology subset (`biomass`/`carrying_capacity`/`ecology_phase`/
> `domestication_progress`/`owner`) — round-trips through a serde `HerdState` (the ecology subset
> embedded as a shared `EcologyState`) captured into `WorldSnapshot.herd_registry` and rebuilt on
> restore via `HerdRegistry::update_from_states`, following the `GenerationRegistry` round-trip
> convention. This closes a **latent bug**: only the lossy display `HerdTelemetry`
> (`WorldSnapshot.herds`) used to be captured, so herd biomass/position silently kept their
> post-rollback values. Restore rebuilds the derived `HerdDensityMap` + `HerdTelemetry` (as
> `advance_herds` does post-loop) so nothing is stale for a turn. `HerdState` is the sim side; the
> FlatBuffers client stream is untouched (it keeps using the display telemetry). **`EcologyState`
> is the shared depletable-ecology record** the forage-depletion slice (§0-ii) reuses for its
> per-tile `ForageState`.

Market hunting shipped as the `Market` follow policy; `SedentarizationScore` shipped (see
"Sedentarization" under Campaign Loop); **corrals shipped** (Intensification Rung 1c — see "Corral"
below). Still deferred (`docs/plan_wildlife_hunting_overlay.md`): the `Camp` entity, and wiring the
sedentarization hard prompt to an actual `found_settlement`. The tile-based `HuntGame` handler stays
neutralized (its client button no longer surfaces).

---

## The Intensification Ladder

**One grammar for both food webs** (`intensification.rs`, config `src/data/intensification_ladder.json`;
authoritative design: `docs/plan_intensification_ladder.md`). Plants and animals climb the *same*
three-rung ladder — rung 1 you take what's there, rung 2 you manage the wild source in place, rung 3 you
control its reproduction — and every rung-transition is the same **Cultivate-shaped verb**: pick it → the
source pays a **reduced** yield while the crew prepares rather than harvests → a **per-source build
meter** climbs → it decays if you walk away → at `1.0` the source steps up a rung.

**The ladder is DATA over a bounded set of coded primitives.** A rung is a [`RungDef`] record, the ladder
is a list, and adding a rung that recombines existing primitives is a one-record edit. See the
`intensification_ladder.json` row in Configuration Files for the record shape and the shipped rungs.

### The build engine — THE seam both tracks call

`RungDef::build_accrual(policy, eligible)` / `build_decay()` / `yield_fraction_while_building()` are the
**single** source of a rung's build math. Both food webs call them instead of reaching for their own
bespoke accrue/decay/dip levers, so the two ladders **cannot drift apart numerically** — that is the
whole reason the dials moved out of `labor_config`/`fauna_config` and into the ladder.

- **`build_accrual`** returns `progress_per_turn × timescale` **only** when `policy` **is** the rung's
  own `verb` *and* the caller's rung-specific gates hold (`eligible` — knows the unlock knowledge,
  source healthy, species ceiling allows, faction owns it); otherwise `0`. **A rung with `verb: null`
  is never driven** — which is what keeps the two `wild` rungs (nothing to build) out of the engine.
- **`timescale` — the rung owns the mechanic, the source scales it** (slice 3c). `build_accrual` and
  `build_decay` take the **same** factor, so it dilates a source's whole build *timescale* and the
  rung's build:decay ratio is invariant. Today the only scaler is a species' **`taming_rate`** on
  `animal:pastoral` (`FaunaConfig::taming_rate_for`, resolved live by display name); every other
  caller passes **`RUNG_TIMESCALE_UNSCALED`** (the plant `tended` patch, the `pen` and its `ExtendPen`
  rings — penning is a flat build for every species). See "The `Tame` verb" for why scaling both is
  load-bearing.
- **The per-source state does not move.** `ForagePatch::cultivation_progress`,
  `Herd::domestication_progress` and `Herd::corral_progress` stay where they live: the engine supplies the *amount*, and the source owns its meter, the clamp to
  `RUNG_COMPLETE`, and the side-effects of completing it (ownership, `corralled_at`, the feed line).
- **Callers.** Accrual: the `Cultivate`, **`Tame`** and `Corral` arms of `advance_labor_allocation`
  (Population) — the *same* call, once per rung. Decay: `forage::advance_cultivation` and
  `fauna::advance_husbandry` (both Logistics; **the one-turn lag is deliberate** — each reads a flag
  the labor arm wrote *last* turn: `ForagePatch::tended_this_turn` / `Herd::tamed_this_turn`); the pen
  has **no** decay (`decay_per_turn: 0.0` — an untended pen escapes outright rather than bleeding).
  The dip: `forage::forage_policy_ceiling`'s `Cultivate` arm and `fauna::hunt_policy_ceiling`'s
  **`Tame`** and `Corral` arms — so **forecast == actual** for free (see "Pre-commit Yield
  Forecast"). **Extending** a pen (2d-β) reads the *same* `animal:pen` rung, so a ring can never drift
  from the initial build.

### The knowledge pattern — practise rung N, unlock rung N+1

**The one rule** (`docs/plan_intensification_ladder.md` §4, slice 4): **working a source teaches the
knowledge its *current rung* declares in `earns_knowledge`.** "Practising rung N" means *working a
source that stands on rung N* — **not** *"using rung N's verb"*. So the **same Sustain hunt** teaches
**Herding** on a wild herd and **Penning** on a tamed one: *you learn herding by managing wild herds,
penning by managing tamed ones*.

**`RungDef::knowledge_earned(policy, eligible)` is THE earn seam** — the twin of `build_accrual`: the
rung names the lesson, the caller credits the ledger. It replaced the two hard-coded per-web
`Sustain && Thriving → <ID>` branches, so `earns_knowledge` went from declarative (slice 2) to live,
for **every** rung including the wild ones. Callers resolve the rung via `fauna::herd_rung` /
`forage::patch_rung`, both read once per source in `advance_labor_allocation`'s Hunt/Forage arms
(**before** the arms branch, so every rung reaches the earn path uniformly).

Three rules ride the seam:
- **Only stewardship teaches** (§4.2) — `FollowPolicy::teaches_knowledge`, defined against the
  `EXTRACTIVE` grouping: **Sustain** teaches (the one extractive rung that only takes the regrowth)
  and so do the investment verbs (`Cultivate`/`Tame`/`Corral` — managing *is* the practice);
  **Surplus/Market/Eradicate teach nothing, at any rung** (they overdraw — slaughtering isn't
  practice).
- **You learn from a healthy source** — `eligible` is the `EcologyPhase::Thriving` gate both shipped
  earn sites already had, preserved unchanged.
- **The two webs learn separately** (§4.2) — free, not enforced: the lesson is read off the source's
  own rung, and a rung belongs to exactly one branch, so a hunt can only ever reach an `animal`
  knowledge. A master rancher isn't automatically a farmer.

**The gate.** `intensification::knows(ledger, faction, discovery, threshold)` is **THE** knowledge
check — it retired the five inlined `get_progress(faction, ID) >= threshold` spellings (both labor
arms, the `cultivate`/`corral` assignment validators, and `extend_pen`), and the `tame` validator + the
`Tame` labor arm were built on it from the start. `threshold` stays a parameter to keep the helper a
pure comparison, but **there is now exactly one value any caller passes**: the ladder's
`knowledge.completion_threshold`. Every gate resolves its discovery off the **rung record**
(`unlock_discovery_id()`), never a hard-coded id, so a gate cannot drift from the rung the labor arm
accrues against.

**The dials are the ladder's** (`knowledge.progress_per_turn` 0.05 / `completion_threshold` 1.0 →
~20 turns per lesson). They **moved here from the two identical per-web copies** (`labor_config`'s
`forage.cultivation`, `fauna_config`'s `husbandry`) once the earn path became one seam: a number that
paces *both* webs belongs to the ladder, exactly like the build dials. `LadderConfig::validate` now
states each bound **once** for both webs (`progress_per_turn > 0` — else the ladder silently freezes
at rung 1; `0 < completion_threshold <= 1` — at `0` every gate is open on turn 1, above `1` no gate
can ever open since the ledger clamps to `1.0`).

**The pacing consequence — measured** (`fauna_husbandry::the_full_wild_to_pen_climb_is_paced_by_practising_each_rung`,
Wild Boar): a pen is a **four-leg, ~97-turn climb** — Sustain-hunt wild → **Herding** (20) → `Tame`
(32, at the boar's `taming_rate` 0.8) → Sustain-hunt the *pastoral* herd → **Penning** (20) →
`Corral` (25). The **Penning leg is new** (§4.3): pre-slice-4 Herding gated `Corral` directly, so the
climb was ~77 turns. **Intended** — one knowledge per transition, and you cannot skip a rung you have
not practised.

### Behavior primitives — `movement` is live; `feeding`/`harvest` are still declarative

`behavior` is config over **coded** primitives (bounded enums): `movement` ∈ `fixed | roam |
drift_to_owner`, `feeding` ∈ `photosynthesis | forage | self_graze`, `harvest` ∈ `worker_take |
worker_tend | passive`. A rung that recombines existing primitives is pure config; a rung needing a
*new* primitive codes that one primitive once, after which it too is config.

- **`movement` IS READ** (slice 3b — the first primitive the engine applies): `fauna::advance_herds`
  resolves each herd's rung and dispatches on it, which is what makes §3's proximity spine
  (`roam` → `drift_to_owner` → `fixed`) a **config diff** rather than a code branch. `drift_to_owner`
  is the primitive slice 3b coded; see "Herd movement is a rung primitive" under Fauna & Wild Game for
  its ordering, its fallbacks, and the overgrazing tension it creates.
- **`feeding` / `harvest` are parsed and validated only** — the seam later slices switch on. `harvest:
  passive` is now **unused by every shipped rung**: retiring passive-free pastoral (§3, slice 3b) left
  no rung that pays without workers. The variant stays as vocabulary for a future rung that genuinely
  does.

### The config states TODAY's truth, deliberately

The whole thesis is that **later slices change behaviour by editing the JSON**, so the shipped file
describes the sim as it is, not as it will be:

| | rung 1 | rung 2 | rung 3 |
|---|---|---|---|
| **plant** | `wild` — no verb; **earns `cultivation`** | `tended` — verb **`cultivate`**, gate `cultivation`, **earns `seed_selection`** (slice 4) | `field` — verb **`sow`**, gate **`seed_selection`** (slice 5 — the consumer that knowledge was earned for), **`site_requirement` { min_forage_capacity 195, requires_fresh_water }** (the land must already be rich + watered — rung 3 moves seed, it cannot fertilize), earns nothing (`irrigation`/`rotation` = rung 4 Worked Land, parked); `movement: fixed` |
| **animal** | `wild` — no verb; **earns `herding`**; `movement: roam` | `pastoral` — verb **`tame`**, gate `herding`, ceiling `pastoral`, **earns `penning`** (slice 4); **`movement: drift_to_owner`, `harvest: worker_take`** (slice 3b — was `roam`/`passive`) | `pen` — verb **`corral`**, gate **`penning`** (slice 4 — was `herding`), ceiling `pen`, earns nothing (`selective_breeding` = rung 4, parked); `movement: fixed` |

Three consequences to keep straight, **all settled by slice 4**: `earns_knowledge` is **live, not
declarative** — every rung's lesson is read through `RungDef::knowledge_earned` off the rung the source
stands on, and the per-web `knowledge_progress_per_turn` copies that used to drive it are gone (see
"The knowledge pattern"); **one knowledge per transition** — Herding gates `tame` **only**, `penning`
gates `corral` + `extend_pen` (the §4.3 reshuffle; pinned by `builtin_ladder_describes_todays_rungs`,
which asserts no two rungs share an unlock gate); and **both the build dials *and* the knowledge dials
now live here**, so the two webs can only be tuned — and paced — together.

See Also: "Cultivation (Intensification Phase 1a)" (the plant rung 2), "Corral (Intensification Rung 1c)"
(the animal rung 3), "The husbandry yield ladder" (what each rung *pays*, which this arc does **not**
unify — animals pay flow-MSY against `r`, plants pay a flat rate without draw-down).

---

## Depletable Forage (Intensification §0-ii)

Forage tiles are **depletable**, the herd biomass/regrowth model transposed onto plants (design:
`docs/plan_intensification.md` §0). Every `FoodModuleTag` tile carries a live per-patch
`{ biomass, carrying_capacity, ecology_phase }` (`ForagePatch`, `forage.rs`) held in the
authoritative **`ForageRegistry`** resource, keyed by tile coord. Foraging now **draws the stock
down** and the patch **regrows**, so the yield instrument's overdraw ⚠ (PR #110) lights up for
forage exactly as it does for overhunting. *Sim-only — the client already renders forage
`sustainable_yield` from the snapshot.*

- **Seeding** (`spawn_initial_forage`, Startup after `spawn_initial_herds`): one full patch
  (`biomass = carrying_capacity`) per `FoodModuleTag` tile, at **that tile's biome capacity** —
  `forage.capacity_by_biome[terrain]`, the human food web's per-biome table (see "The two food webs"),
  never a global constant. A food-module tile whose biome carries **nothing human-edible** (a stated
  `0` — glacier, salt pan, deep-sea vent field; the module classifier tags these off their *tags*, not
  off anything growing there) is seeded **no patch at all**, exactly as a zero-graze tile holds no
  `GrazePatch`: "no food here" is an *absent* reading, never a zero one. Idempotent (a restored world
  is skipped).
- **Regrowth** (`advance_forage_regrowth`, `TurnStage::Logistics` alongside `advance_herds`): each
  patch regrows toward its cap and refreshes its `EcologyPhase`. Unlike a wild herd, a patch uses
  **pure `logistic_regrowth`** (no Allee / critical-depensation crash) and **never despawns** —
  plants reseed, so a depleted (feral) patch always recovers. Because `logistic_regrowth` is `0` at
  `biomass = 0`, `regrow_patch` first applies a **reseed floor** — it lifts a depleted patch up to
  `reseed_floor_fraction × carrying_capacity` (a small standing crop, `max()` so a healthy patch is
  untouched) *before* regrowth — so a patch driven to exactly `0` (repeated Eradicate + f32
  underflow, `take_fraction = 1.0`, or a restored snapshot carrying `biomass = 0`) still has a seed
  stock and recovers via normal regrowth instead of sticking at `0` forever. The floor is below
  `collapse_fraction`, so Eradicate still crashes a patch hard into the Collapsing band — it just
  can't hold it permanently at `0`.
- **Draw-down** (`forage_take`, the plant mirror of `hunt_take`): resolves the per-policy ecology
  ceiling, caps it by gather throughput (`workers × per_worker_biomass_capacity × seasonal_weight`),
  clamps to the patch's biomass, **subtracts the take**, and converts to provisions
  (`take × provisions_per_biomass × output_multiplier`). Foraging honors the **full policy axis**
  (Sustain/Surplus/Market/Eradicate — §0-iii, **parity with hunting**), mirroring `hunt_take`'s
  rungs: **Sustain** = the **Maximum Sustainable Yield** (`sustainable_yield(..)` — regrowth at the
  most-productive biomass K/2, so a patch *at carrying capacity* still yields a positive skim and a
  collapsed patch yields nothing; Sustain draws the patch toward K/2); **Surplus** = that ×
  `surplus_multiplier` (slow
  decline); **Market** = `market.take_fraction × biomass` (a commercial share → fast depletion) and
  the `Forage` arm sells the take as trade goods (`take × market.trade_goods_per_biomass ×
  market.trade_goods_multiplier × output_mult` → `FactionInventory` — gathered goods sold, **Market
  only**); **Eradicate** = `eradicate.take_fraction × biomass` (strip the patch, no floor, no trade
  goods — denial). The `Forage` arm of `advance_labor_allocation` (Population) passes the
  assignment's policy into `forage_take` and writes the real `sustainable =
  sustainable_yield(biomass_before, cap, forage.ecology) × provisions_per_biomass ×
  output_multiplier` (MSY-based) into the
  yield telemetry, so a non-Sustain gather reads `actual > sustainable` (the over-forage ⚠) exactly
  as an over-hunt does.
- **Config** (`labor_config.json` `forage`): **`capacity_by_biome`** (the per-biome capacity table —
  see "The two food webs"; **validated total** over every `TerrainType` by `LaborConfig::validate`),
  `per_worker_biomass_capacity`,
  `provisions_per_biomass`, an `ecology` block reusing fauna's `EcologyConfig` (`regrowth_rate` tuned
  higher than fauna's 0.05; `collapse_fraction`/`stressed_fraction` phase bands), a
  `reseed_floor_fraction` (0.02 — the reseed standing crop as a fraction of `carrying_capacity`, so a
  crashed patch recovers from a seed stock rather than sticking at `0`; below `collapse_fraction`),
  plus the **policy axis** levers (§0-iii, mirroring fauna's `follow`/`market`/`hunt`):
  `surplus_multiplier` (1.6),
  `market: { take_fraction 0.20, trade_goods_multiplier 4.0, trade_goods_per_biomass 0.005 }`,
  `eradicate: { take_fraction 0.30 }`. The old flat `forage.per_worker_yield` lever is **retired**,
  and so is the flat `forage.carrying_capacity` (120 on every food-module tile) it was replaced by:
  a **constant** human web could not diverge from the spatial animal one, so *"your best farm is not
  your best pasture"* was untrue **by construction**. Per-biome (not per-`FoodModule`) is deliberate —
  the two tables must be comparable tile-for-tile and must be able to disagree *within* a module.
  Because every yield is linear in `K` (MSY = `r·K/4`), the cultivation incentive and every policy
  ceiling scale with the tile and need no re-derivation.
- **Policy plumbing** (§0-iii, the 5-site mirror of Hunt's policy): `LaborTarget::Forage` carries a
  `policy: FollowPolicy` (a policy change on the same tile is the **same source** in `same_source`,
  a mutable property); the `assign_labor forage <x> <y> [policy] <workers>` command-text parse takes
  an optional policy token; `handle_assign_labor` builds it via `parse_follow_policy`; and the
  policy round-trips through the rollback snapshot (`LaborAssignmentState.policy`, no schema change).
- **Persistence** — `ForageRegistry` round-trips through the rollback snapshot exactly like the
  `HerdRegistry` (the §0-i pattern): a per-tile `ForageState` (= tile key + the shared
  `sim_schema::EcologyState`) captured coord-sorted into `WorldSnapshot.forage_registry` and rebuilt
  on restore via `ForageRegistry::update_from_states`. `progress`/`owner` on `EcologyState` now carry
  **cultivation** (Phase 1a, below) — a mutate-then-restore rewinds it like biomass. Not wired to the
  FlatBuffers client stream.
- **Companion client slice:** the sim side of the forage policy axis (§0-iii) is complete — the
  client `%ForageAssignControls` policy picker (mirroring `%HerdAssignControls`) that emits the
  policy in the `assign_labor forage` command is a **client-dev follow-up**. A client patch-ecology
  readout (thriving/stressed/collapsing on the map/tile, like herds) is a possible later slice.

### Cultivation (Intensification Phase 1a)

The **plant analog of animal husbandry** (`docs/plan_intensification.md` §3), evolved past the
mechanical husbandry transpose into **Rung 1a — the worker-tended, place-local tended patch**, and now
into an **explicit policy with an investment cost**. A patch carries `cultivation_progress` (0–1,
`1.0` = cultivated) + `owner: Option<FactionId>` on `ForagePatch`, mirroring a `Herd`'s
`domestication_progress`/`owner`, and rides the shared `EcologyState` (`progress`/`owner`) through the
rollback snapshot. A completed patch is a **tended patch**: **worker-tended + place-local +
higher-output + feral-if-abandoned**. *Sim-only — the client readout is a follow-up.*

> **The free path is gone (design fix).** Cultivation used to accrue **silently and for free** under
> Sustain: same labor, same tile, no cost ⇒ cultivating was always correct and there was **no
> decision**. It is now the **`Cultivate` policy** (`FollowPolicy::Cultivate`, Forage-only) with a real
> up-front cost, and the **early-claim `claim_threshold` is removed** (it would let the player skip the
> investment — the whole point). Sustain still *teaches* the faction Cultivation knowledge; it just
> never tames a patch. The animal twin is the **`Corral` policy** — see "Corral".
- **Rung 1b — the earned-knowledge gate (`docs/plan_intensification.md` §4b).** Cultivation is a
  faction-level knowledge *learned by doing*, **never start-granted**: a **Sustain** forage on a
  **Thriving** patch accrues faction **Cultivation** knowledge (discovery `CULTIVATION_DISCOVERY_ID`
  = 2003, `forage.rs`) in the per-faction `DiscoveryProgressLedger` at
  `cultivation.knowledge_progress_per_turn` (`add_progress`, clamped to `1.0`). **A patch cannot accrue
  `cultivation_progress` until the faction *knows* Cultivation** — `advance_labor_allocation` only calls
  `accrue_cultivation` once `ledger.get_progress(faction, 2003) >= knowledge_completion_threshold`.
  Knowledge is all Sustain earns — it **never** accrues `cultivation_progress`. The `cultivation` tag →
  discovery 2003 mapping is declared in `start_profile_knowledge_tags.json` purely so it is mappable;
  **no start profile lists it**, so no faction begins knowing Cultivation.
- **The `Cultivate` policy — the investment.** In `advance_labor_allocation`'s **Forage** arm
  (Population), a patch worked under `FollowPolicy::Cultivate`:
  - **Costs a yield dip while preparing.** Its take ceiling is
    the `plant:tended` rung's `yield_fraction_while_building × sustainable_yield(..)` — a *fraction of the MSY ceiling*
    (`forage_policy_ceiling`, reusing the **shared** `sustainable_yield` helper, never a second
    formula). The crew is clearing and planting, not gathering. Because the take is a fraction of MSY
    it is **sustainable**, so the patch stays Thriving (which the accrual gate requires) — the cost is
    a pure yield dip, not a depletion.
  - **Accrues `progress_per_turn`** toward `1.0` (sets `owner` on first accrual; only the owner
    accrues), **gated** on the faction *knowing Cultivation* AND the patch being **Thriving**. If a
    gate lapses mid-run (another band overdraws the patch to Stressed) progress simply **stops accruing
    that turn** — it is not lost and the policy is not silently switched; the patch is still marked
    worked, so it doesn't decay either, and accrual resumes when it recovers.
  - **Accrues AFTER the turn's take**, so the turn pays exactly what the pre-commit forecast promised
    (forecast == actual). The turn progress reaches `1.0` is the last preparing take; the full tended
    yield starts the next turn.
  - **Marks the patch `tended_this_turn`**, so `advance_cultivation` spares a patch under active
    preparation — the investment accrues at the **full** `progress_per_turn` (25 turns at the default),
    not net-of-decay.
  - **Break-even** (defaults `fraction` 0.25, `progress_per_turn` 0.04): the dip costs ~75% of that
    patch's Sustain yield for ~25 turns ≈ `0.75 × 0.375 × 25` ≈ **7 prov** forgone; the tended patch
    then out-pays wild Sustain by `1.2 − 0.375` = **0.825 prov/turn**, recouping the investment ~8–9
    turns after completion. Cultivating is correct only if you intend to stay — the decision the free
    auto-accrual erased.
  - `ForagePatch` methods: `is_cultivated`/`accrue_cultivation`/`decay_cultivation` (the early-claim
    `claim_cultivation` is **removed**).
- **Tended yield — a WILD STAND ON A BOOSTED CURVE, gathered place-local** (slice 7 — the rung-2
  correction). A tended patch is **worked, not passive**, and it is **still wild**: what tending buys is
  a faster curve (`cultivation.tended_regrowth_gain` 2.0, folded in by **`forage::patch_ecology`** — the
  plant twin of `fauna::herd_ecology`, and the one seam every consumer resolves a patch's ecology
  through). It is therefore gathered by the **ordinary `forage_take` path**, exactly like rung 1:
  **policy-live** (Sustain/Surplus/Market/Eradicate are four different takes off its boosted MSY),
  **worker-capped**, and **drawn down** — so a tended patch **can be over-farmed** and the overdraw ⚠
  fires on it. This is the exact shape a **pastoral** herd already had (`hunt_policy_ceiling` on the
  boosted pastoral `r`); the plant web used to collapse a rung *earlier* than the animal web, and that
  asymmetry was the bug. **It still out-yields the same patch's wild Sustain** — the intensification
  incentive, now carried by the curve rather than a flat rate, and **scale-free by construction**: it
  *is* the gain (measured on `AlluvialPlain`, K = 195: wild **0.61** → tended **1.22** prov/turn).
  Working a completed improvement at either rung marks it `tended_this_turn` (a transient,
  non-persisted per-turn flag) so the decay pass can tell tended from abandoned. The old
  even-split-across-all-the-owner's-bands payment in `advance_cultivation` is **retired**, as is the
  flat `tended_provisions_per_biomass` managed rate.
  - **`Cultivate` on a COMPLETED patch still pays its dip.** The dip means "the crew is preparing
    ground, not gathering", which does not stop being true once the ground is ready — so the player
    switches to a harvest policy to collect the payoff. The animal side has always behaved this way
    (`Tame` on an already-tamed herd pays the pastoral dip); slice 7 made the plant side agree, because
    the retired managed branch ignored the policy entirely.
- **Feral if unworked** — `advance_cultivation` (`forage.rs`, `TurnStage::Logistics` alongside
  `advance_forage_regrowth`) is the **decay/feral** pass only. A patch **worked as an improvement this
  turn** (`tended_this_turn` — tending a completed patch *or* preparing one under Cultivate) is
  **spared**; everything else decays by `decay_per_turn`. So an **untended cultivated** patch **goes
  feral** (drops below `1.0` → reverts to a wild gather patch, then decays to 0 over
  ~`1/decay_per_turn` turns; owner clears at 0) and an **abandoned part-prepared** patch loses its
  investment the same way. **Stage-ordering:** Logistics runs *before* Population, so the
  `tended_this_turn` flag this pass reads was written by the labor arm **last** turn (a deliberate
  one-turn-lag carry-across-turns signal; the flag is cleared here and re-set next Population stage).
  Net: a patch worked every turn never decays; a patch whose band leaves reverts one turn later.
- **The loop (the settle pull).** Sustain-forage a thriving patch → *learn* Cultivation → **choose** to
  pay the Cultivate dip for ~25 turns → the patch becomes tended → a band tending it collects the
  higher tended yield **place-locally** → move the band away and it goes feral, reverting to wild.
  Place-locality + feral + a sunk investment = the band is **pinned near its farm**: intensifying
  raises output *and* deepens the anchor.
- **`cultivate` command (repurposed)** — `cultivate <faction> <x> <y>` (`handle_cultivate`; unchanged
  proto/runtime/text plumbing, `CommandEventKind::Cultivate`) now **sets the `Cultivate` policy** on
  the band(s) already foraging that tile (`set_policy_on_working_bands`) — the command form of what the
  client's policy picker does. It **claims nothing**. Gates (shared with `assign_labor` via
  `validate_labor_policy`): faction knows Cultivation, patch is **Thriving**, not already cultivated,
  not another faction's; plus a rejection when **no band is foraging** the tile (staff it first).
- **Policy validation** — `FollowPolicy::valid_for_forage` / `valid_for_hunt`: `Cultivate` is
  Forage-only and `Corral` Hunt-only. `handle_assign_labor` rejects an invalid combo (and a failed
  gate) with a clear failure event before touching the allocation; unassigning (`workers == 0`) is
  always allowed, so a player can always abandon an investment.
- **Sedentarization (folded)** — `sedentarization_tick` reads `herds.domesticated_count(faction) +
  forage.cultivated_count(faction)` for its **domestication** input: plant + animal domestication
  share the one driver (no new weight, no re-balance).
- **Config.** The plant rung-2 **build dials moved to `intensification_ladder.json`**'s `plant:tended`
  rung (`build`: `progress_per_turn` 0.04 → 25 turns to prepare, `decay_per_turn` 0.01 the
  feral-reversion rate, **`yield_fraction_while_building` 0.25** — the old `cultivating_yield_fraction`,
  the investment cost: the preparing take ceiling as a fraction of the patch's Sustain/MSY ceiling), so
  the plant and animal ladders can only be tuned together (see "The Intensification Ladder"). What stays
  in `labor_config.json` `forage.cultivation` (`CultivationConfig`): **`tended_regrowth_gain`** (1.5 —
  the rung-2 payoff, the plant twin of `husbandry.pastoral_gain` at its value; a tended patch's stock
  regrows `gain ×` as fast as the same patch wild, so its MSY — and every policy ceiling on it — scales
  with it; keep it `> 1.0` or Cultivate buys nothing), plus the
  **Rung 1b earned-knowledge** levers `knowledge_progress_per_turn` (0.05 — faction Cultivation earned
  per Sustain-forage-Thriving turn, ~20 turns to know) and `knowledge_completion_threshold` (1.0 = the
  ledger's completion value). The early-claim `claim_threshold` is **removed**. The build dials'
  invariants (`0 < progress_per_turn`, `0 <= decay_per_turn < progress_per_turn`,
  `0 < yield_fraction_while_building < 1`) are now **enforced on every load path** by
  `LadderConfig::validate()`, which owns them — as are the **knowledge** invariants
  (`knowledge_progress_per_turn > 0`, `0 < knowledge_completion_threshold <= 1`), which moved to the
  ladder with those dials in slice 4. **The levers homed here are now validated on every load path**
  (slice 7 — the old "asserted over the *builtin* only, so a `LABOR_CONFIG_PATH` override that breaks it
  is accepted silently" gap is **closed**): `LaborConfig::validate()` enforces the **plant ladder's
  monotonicity** — `tended_regrowth_gain > 1` (wild < tended) and `field_provisions_per_biomass >
  gain × regrowth_rate/4 × provisions_per_biomass` (tended < field), both scale-free in `K`, the payoff
  twin of `FaunaConfig::validate`'s `pen_gain > pastoral_gain > 1`.
- **Intensification display snapshot (on the wire, consumed by the client-dev rendering slice next).**
  The intensification-ladder state is now exported to the FlatBuffers client stream (append-only per
  the schema discipline; `snapshot.fbs`, `sim_schema`, `snapshot.rs`), on both `WorldSnapshot` and
  `WorldDelta`:
  - **Forage patch cultivation** — a new per-tile `foragePatches:[ForagePatchState]` list
    (`snapshot_forage_patches`, from the `ForageRegistry`, stable `(y, x)` order). Per patch: tile
    `(x, y)`, `cultivationProgress:float` (0..1), `isCultivated:bool` (tended = progress ≥ 1.0),
    `owner`/`hasOwner` (tending faction; `hasOwner = false` = wild), plus `biomass`/`carryingCapacity`/
    `ecologyPhase` for optional patch-health. This is the client's first per-tile forage-patch payload
    (previously forage was visible only via `laborAssignments`).
  - **Faction ladder knowledge** — a per-faction
    `intensificationKnowledge:[IntensificationKnowledgeState{ faction, cultivation, herding,
    seedSelection, penning }]` list (`snapshot_intensification_knowledge`, from the
    `DiscoveryProgressLedger`), mirroring `sedentarization[]`. **One field per rung-transition**, so it
    reads as the ladder itself — `wild --cultivation--> tended --seedSelection--> field` and
    `wild --herding--> pastoral --penning--> pen` — each the 0..1 progress on discoveries 2003 / 2004 /
    **2005** / **2006** (the last two appended in slice 4, **append-only**: `cultivation`/`herding`
    keep their shipped slots). A faction is emitted only once it has begun learning *something* (all
    zero → skipped). Client renders these as learning/known meters like the sedentarization meter;
    the **two-meter split** (faction knowledge vs per-source build progress, §4.1 — the root UX fix)
    is the client slice, and both meters are already distinctly on the wire.
  - **Herd corral** — `HerdTelemetryState.corralled` (see the corral section above).
- **Follow-ups:** **Rung 1c — corral** (the fauna-side pen behind a `herding` gate) **shipped** — see
  "Corral (Intensification Rung 1c)" under Fauna & Wild Game. The **client _rendering_ for both ladders**
  (tile-card cultivation N% / tended-patch + Cultivation/Herding knowledge meters + herd corral
  indicator) is the **final Phase-1 slice** and remains a client-dev follow-up; the sim/schema data is
  now all on the wire (fields above).

### The `Sow` verb + the Field (Intensification rung 3) — the plant twin of the pen

**Rung 3 places a food source where you want it** (`docs/plan_intensification_ladder.md` §2, slice 5).
Once a faction knows **Seed Selection** (`SEED_SELECTION_DISCOVERY_ID` = 2005 — earned by *working
tended patches*, slice 4's `plant:tended` `earns_knowledge`; earned then, spent here), a crew working
a tile under **`FollowPolicy::Sow`** builds a **Field** on it. A Field is not a new entity: it is a
`ForagePatch` **at rung 3**, carrying its own `field_progress` meter beside `cultivation_progress` —
exactly as a `Herd` carries `corral_progress` beside `domestication_progress`. There is **no "extend
the field"**: each tile is its own patch, so you sow another field (the pen extends only because one
herd has one appetite).

- **Placed, not conjured — and SCARCITY IS THE POINT.** Rung 3 is *"I know how to take seed from a
  plant and put it somewhere else — but I do not know fertilization, so the land must already be very
  fertile, and near fresh water"*. That rule is the rung's **`site_requirement`** on the ladder record
  (`RungSiteRequirement` — the plant twin of `ceiling_required`, keyed on the **land** instead of the
  species), and both dials are levers:
  - **`min_forage_capacity: 195`** — a floor on the tile's own `tile_forage_capacity` (the *same*
    helper that sizes a wild patch and the wire's `forageCapacity`, never a Field-specific table). It
    admits exactly the **river-deposit class** — RiverDelta 210, Floodplain 205, AlluvialPlain 195 —
    and stops just above ordinary MixedWoodland (190).
  - **`requires_fresh_water: true`** — the tile must be on or beside **fresh** water
    (`forage::tile_is_fresh_watered`): `TerrainTags::FRESHWATER` on the tile, **or** a river along one
    of its six sides (`Tile::has_any_river_edge` — the hydrology edge primitive, set on *both* flanking
    hexes, so the riverbank needs no neighbour lookup), **or** a fresh-water hex next door (odd-r
    `hex_neighbors_wrapped`). A **salt coast is not water** for this — you do not farm sea spray.
  - **Measured on the standard map** (earthlike 80×52, seed 119304647): **49 sowable tiles of 4160
    (1.2%)** post the "divides, not valleys" arc — **35** on the pre-arc dome — against **2328** tiles
    that merely bear food. (The historical **46** figure predates that arc.) **The measurement only
    means anything with `generate_hydrology` run**: the rule wants fresh water, and rivers/deltas are
    hydrology's, so a fixture that skips it measures 0 at every grid size and every seed. The
    **conjunction is still doing the work** (pre-arc measurement: 337 tiles cleared the fertility
    floor and the water rule cut 291 of them, 86%). Few sowable tiles ⇒ *which* tile matters ⇒ a band may have to **move** to
    farm at all. That friction is the design pillar, not a side effect.
  - **The refusal names the fault** (`SiteRefusal::{TooPoor, TooDry, TooPoorAndTooDry}` — the rung
    judges, the caller phrases) and points at **rung 4, Worked Land** (plows/irrigation, a future arc):
    *"Your people can carry seed, but not yet water or feed the land…until they learn to work the land
    itself."* Too poor and too dry are different problems with different answers (move, or wait).
  - **Rung 4 will be a LOOSER COPY of this record and nothing else** — a lower floor,
    `requires_fresh_water: false`. That is the arc's config-driven thesis paying out: a rung whose
    *placement rule* differs is a config edit (pinned by
    `a_looser_site_requirement_is_a_pure_config_edit`).
- **It needs no source below it** — the one place the two webs legitimately differ (§2). Seed travels:
  qualifying ground carrying *no forage site at all* is a legal target, and sowing it **creates** the
  patch (`ForagePatch::sown` — the tile's own biome capacity, biomass at the reseed floor, normal
  logistic regrowth). `Corral`, by contrast, needs a herd you already tamed. *(Reachability caveat,
  measured: worldgen seeds a patch on **every** food-bearing tile — `classify_food_module` tags
  essentially every biome — so on a generated map `Sow` always **upgrades an existing wild patch**. The
  create-from-nothing path is live and tested against constructed bare ground, but its input does not
  occur today. This is also the claim that the stale "~95% of tiles carry no `ForagePatch`" note above
  had made look true.)*
- **Not gated on Thriving, unlike Cultivate** — load-bearing, not a relaxation: sown ground starts at
  the reseed floor, i.e. *Collapsing* by construction, so a health gate would forbid the case the rung
  exists for. You *tend* a healthy wild stand; you *plant* bare ground. (`Tame` draws the same line.)
- **The investment.** The `plant:field` rung's `yield_fraction_while_building` (0.25) × what the ground
  would otherwise pay: the MSY dip on a wild patch (via `forage_policy_ceiling`), and the **managed**
  dip on a tended patch being upgraded (0.25 × its tended harvest — `forage_forecast` and the labor
  arm both read the one shared `field_yield_fraction_while_building`). On **bare** ground that is a
  fraction of nothing, so a bare-ground sow is near-pure investment: **~0.13 prov/turn across its
  25-turn build against the 2.1/turn the Field then pays** (measured, `forage_field.rs`).
- **The payout — rung 3 out-yields rung 2, or the rung is pointless.** A completed Field pays its
  workers `biomass × cultivation.field_provisions_per_biomass` (**0.02**, `labor_config.json`), the
  tended patch's *shape* at **2×** its rate, place-local and without drawing biomass down.
  `sustainable == actual` (no ⚠). **But the collection cap still binds** (slice 7): rung 3 collapses the
  *policy* axis, never the worker cap — you always carry the harvest home — so the actual take is
  `min(production, workers × per-worker throughput)`, `workers_needed` is derived, and the crop the crew
  could not carry is reported as `wasted`. **Measured production/turn on `AlluvialPlain` (K 195):**
  wild Sustain **0.61** → tended **1.22** → Field **3.90**, needing **2 / 4 / 10** gatherers
  respectively at 0.40 prov/worker.
- **Feral if abandoned — one rule for the whole plant web.** `advance_cultivation` bleeds **both**
  improvement meters at their own rung's `decay_per_turn` on any untended turn, so an abandoned Field
  reverts to a **wild** gather patch (after the pass's deliberate one-turn lag) and both meters lapse
  to zero over ~100 turns, ownership clearing only once nothing is left. It does **not** step down to a
  tended patch on the way: that would pay the deserter rung 2's managed yield for free, and *an
  improvement you stop working goes back to the wild* is the plant web's only story here.
- **`sow <faction> <x> <y>` command** (`handle_sow`; `SowCommand` proto field **41**,
  `CommandEventKind::Sow`) — **sets the `Sow` policy** on the bands already foraging that tile, the
  command form of the client's policy picker. It sows nothing outright; the seed goes in when the crew
  works the ground, so `assign_labor … sow` places a Field on identical terms. Rejections, each
  distinct (`validate_sow`, shared with the `assign_labor` path): no such tile / **the land will not
  take seed** — *too thin*, *too dry*, or both, each naming the fault and pointing at rung 4 / faction
  hasn't learned **Seed Selection** ("Work tended patches to learn it") / already a Field / another
  people's ground / **no band is foraging it**. The site rule gates the **labor arm** too (both the
  seed placement and the build accrual), so `assign_labor … sow` cannot farm ground the command
  refuses.
- **`cultivated_count` counts Fields** (`ForagePatch::is_managed`), so the sedentarization
  domestication signal cannot read rung 3 as *less* domesticated than rung 2 (a bare-ground Field
  carries no cultivation meter at all).
- **Persistence** — `field_progress` rides `ForageState` (its own field beside the shared
  `EcologyState`'s cultivation `progress`/`owner`, mirroring `HerdState.corral_progress`), so a
  rollback rewinds a half-sown Field.
- **On the wire (slice 6a — append-only, slots 36–44):** `ForagePatchState` carries
  `fieldProgress:float` + `isField:bool` (the rung-3 meter and the completed rung — read the *bool*,
  never infer a rung from the float) beside the already-shipped `cultivationProgress`/`isCultivated`,
  so the client has **both** plant meters for the §4.1 two-meter split; `ceilingSow:float` +
  `fieldYield:float` (Sow's "preparing X → then Y" pair, the twins of `ceilingCultivate`/`tendedYield`
  — `ceilingSow` is its **own** field for `ceilingTame`'s reason: two investment rungs on one branch
  must never share a ceiling); and **`sowSiteRefusal:string`** — `""` when the ground takes seed, else
  `"too_poor"` / `"too_dry"` / `"too_poor_and_too_dry"` ([`SiteRefusal::as_str`], free-form per the
  `species`/`ecologyPhase` convention). That last one ships **the answer, not a bool**: only ~1% of
  tiles are sowable, so *"why can't I sow here?"* is the live question, and the client can re-derive
  nothing (it holds neither the capacity table nor the hydrology). The capture resolves it through the
  **same** `RungSiteRequirement::refusal` seam the command and the labor arm gate on — pinned by
  `the_exported_sow_site_refusal_is_the_verdict_the_command_acts_on`, so the wire cannot disagree with
  the gate.
- **Client follow-up (slice 6):** the native reader
  (`clients/godot_thin_client/native/src/lib.rs::forage_patches_to_array`) does not yet surface the
  five new fields as dict keys, and no panel renders them.

See Also: "Cultivation (Intensification Phase 1a)" (the rung below), "Corral (Intensification Rung 1c)"
(the animal rung 3 this mirrors), "The Intensification Ladder" (the engine + the config).

---

## The Graze (Pasture) Layer (Grazing Phase 2a)

**Humans and animals do not eat the same things.** The land carries **two vegetal stocks, on two food
webs** (authoritative design: `docs/plan_grazing_foundation.md`):

| | `ForagePatch.biomass` (Depletable Forage) | **`GrazePatch.biomass`** |
|---|---|---|
| Who eats it | **humans** (Forage assignments) | **animals** (herds, wild and penned) |
| Where it is | `FoodModuleTag` tiles — **not sparse**: nearly every biome is tagged, so in practice every food-bearing tile | **any vegetated land**, by biome (dense) |
| What it is | seeds, nuts, tubers, fruit, shellfish | grass, browse, forbs — **cellulose humans cannot digest** |
| Its capacity | `forage.capacity_by_biome` (`labor_config.json`) | `graze.capacity_by_biome` (`fauna_config.json`) |

That is not flavor: it is the economic basis of herding (a pastoralist converts a resource
**worthless to humans** into meat and milk), and it is why *your best farm is usually not your best
pasture*. `graze.rs` mirrors `forage.rs` (which mirrors the herd model) exactly — the proven,
rollback-persisted pattern.

### The two food webs — two tables, meant to disagree

**Both webs are per-biome tables over the same `TerrainType` set, in the same shape, with the same `validate()`
discipline** (total table required; a missing row would read as an invisible zero and **zero must be
stated**). They are per-**biome**, not per-`FoodModule`, precisely so they are comparable tile-for-tile
and can disagree *within* a module — **that disagreement is the agropastoral decision.** The
`FoodModuleTag` model is untouched: the module still decides what *kind* of gathering a tile offers
(and its `seasonal_weight`); the table decides *how much* is there.

| biome | graze (animals) | forage (humans) | the story |
|---|---|---|---|
| `PrairieSteppe` | **240** (the reference pasture) | 70 | grass: the animals feast, humans get seed heads |
| `RiverDelta` / `Floodplain` | 130 | **210 / 205** | the richest human ground there is |
| `AlluvialPlain` | **110** | **195** | silt + water = **cropland**. The FARM, not the pasture |
| `MixedWoodland` | **55** | **190** | nuts, mast, berries under a canopy that shades out the ground cover — **the flagship inversion** |
| `Tundra` / `AlpineMountain` | 100 / 65 | 25 / 20 | **rangeland**: pastoralism lives exactly where farming can't |
| `ContinentalShelf` / `CoralShelf` | **0** (water) | 130 / 180 | the coastal larder — a fishery is a food module on *water* |
| `RollingHills` / `PeatHeath` | 150 / 135 | 80 / 55 | |
| glacier / lava / salt flat / deep ocean | **0** | **0** | a *stated* zero |

**The silt lowlands were LOWERED on the graze side** (`AlluvialPlain` 230 → **110**, `Floodplain`/
`RiverDelta` 230/220 → 130): a river plain is prime *cropland*, not prime range, and its value moved to
the human web where it belongs. `AlluvialPlain` is additionally the tag solver's universal fallback
biome (~25% of all land even after the `FertileLowland` palette fix), so leaving it tied with prairie
for best pasture baked a **worldgen artifact into the fauna model**.

**Measured, not asserted** (`integration_tests/tests/graze_distribution.rs::two_food_web_report`,
earthlike 80×52, seeds 11/4242/90210 — run with `--nocapture` for the joint histogram):
- **Correlation between the two webs across living land: −0.11 / +0.03 / −0.01.** Near zero, as
  intended: knowing a tile's pasture tells you almost nothing about its farm. (Across *all* land it is
  +0.13…+0.24 — bare rock is a shared **zero**, an irreducible positive term that says nothing about
  the design claim; a farm-vs-pasture decision needs land that can feed *somebody*.)
- **Land that is top-decile in BOTH webs: 0.0% on every seed** (independence would give 1%). *Your best
  farm is not your best pasture* — measured, not claimed. (The top-**quartile** overlap is printed too
  but **not** guarded: `AlluvialPlain` is ~25% of land, so the 75th-percentile graze cut lands *inside
  that one biome* and the number flips 0% ↔ 24% on a hair. That is a cliff, not a measurement — do not
  tune a capacity table to it.)
- **Balance impact on the human food economy: map-wide capacity −18…−20%, but the early game is flat.**
  The mean capacity of patches within `band_work_range` of the start is **123 / 128 / 99** vs the
  retired flat **120** (mean 117 across seeds, −3%). The map-wide drop is almost all tundra, bare rock
  and scrub — land nobody starts on, which the old flat 120 was pricing as richly as a river delta.
  Individual starts *do* move (a grassland/tundra start is thinner, a river-valley start richer): that
  spatial variance is the feature, and it is the thing to watch in a live campaign.

> **Phase 2a ships this layer INERT.** It seeds, regrows, persists and exports — and **nothing reads
> it for gameplay**. No herd behaviour changes, zero balance impact. Herd carrying capacity,
> competition, overgrazing, migration and spawn placement all become functions of it in Phase 2b/2c;
> the layer ships inert first so its *distribution can be looked at on a real map* before the fauna
> model is bet on it.

- **`GrazeRegistry`** (resource, `graze.rs`) — per-land-tile `GrazePatch { biomass, carrying_capacity,
  ecology_phase }`, keyed by tile coord. **Only tiles with a positive capacity hold a patch**, so
  "this biome has no pasture" is an *absent* reading, never a zero one.
- **Seeding** (`spawn_initial_graze`, Startup right after `spawn_initial_forage`): one full patch
  (`biomass = carrying_capacity`) per non-`WATER` land tile whose biome has a positive
  `graze.capacity_by_biome`. Idempotent (a restored world is skipped) — the `spawn_initial_forage`
  guard.
- **Regrowth** (`advance_graze_regrowth`, `TurnStage::Logistics` right after
  `advance_forage_regrowth`): **pure logistic regrowth over a reseed floor**, then a phase refresh.
  **No Allee / collapse branch — grass has no depensation**, and it **never despawns**: an eaten-out
  tile always recovers (slowly). Shares the one plant curve `fauna::reseeding_logistic_regrowth` with
  `forage::regrow_patch`, so the two stocks can never drift apart. Permanent degradation
  (desertification) is a deliberate later lever, not this arc.
- **Capacity is a property of the LAND, not the animal** — `graze.capacity_by_biome`, a **data table
  over every `TerrainType`, not a formula**, and **read against its twin** `forage.capacity_by_biome` (see
  "The two food webs" above, which owns the joint tuning table and the measurements). Anchor:
  `PrairieSteppe` = **240** is *the* reference pasture; every other row is a claim relative to it.
  `MixedWoodland` (55) / `BorealTaiga` (40) are deliberately **poor** — a closed canopy shades out the
  ground cover, the inversion the two-stock split exists to create. Cold/high **rangeland** (Tundra
  100, AlpineMountain 65, HighPlateau 75, SemiAridScrub 100) is deliberately *better for animals than
  for humans*: pastoralism exists precisely where farming cannot. Water / glacier / lava / salt flat
  are a **stated 0**. The absolute scale is a free parameter; only the ratios matter until Phase 2b's
  `fodder_per_biomass` denominates it into animals.
- **Config** (`fauna_config.json` `graze` — homed here, not in a file of its own, because graze is the
  *substrate of the fauna model*: every consumer of it is a fauna system, and it lets the block reuse
  `FaunaConfig::validate` verbatim): `capacity_by_biome`, `ecology` (`regrowth_rate` **0.40** —
  **grass is the fastest-renewing vegetal stock in the model**: wild fauna 0.05 ≪ forage 0.25 <
  **graze 0.40** ≪ a fed pen 0.90; `collapse_rate` is *inert* for graze, as it is for forage — pure
  logistic never reads it; `collapse_fraction`/`stressed_fraction` are the phase bands the overgrazing
  readout uses), `reseed_floor_fraction` (0.02, mirroring forage's — kept **below**
  `collapse_fraction` so the floor stops *permanent death* without *hiding overgrazing*).
- **Validated** (`FaunaConfig::validate`, so every load path is covered): the table must be **total**
  over every `TerrainType` (a missing row silently reads `0` — an invisible dead zone nothing would ever
  explain: **zero must be stated, never defaulted**), every row finite and `>= 0`, **at least one row
  positive** (an all-zero table disables the whole layer while parsing perfectly), the graze ecology
  live and phase-ordered, and `reseed_floor_fraction < collapse_fraction`.
- **Persistence** — `GrazeRegistry` round-trips through the rollback snapshot exactly like
  `ForageRegistry`/`HerdRegistry`: a per-tile `GrazeState` (tile key + the shared
  `sim_schema::EcologyState`) captured coord-sorted into `WorldSnapshot.graze_registry`, rebuilt on
  restore via `GrazeRegistry::update_from_states`. Graze is **wild ground** — never owned, tended or
  improved — so `EcologyState`'s `progress`/`owner` ride at their defaults.
- **Wire — on `TileState`, not a patch list.** `TileState.grazeBiomass:float` /
  `grazeCapacity:float` / `grazeEcologyPhase:ubyte` (`0` = none, `1` thriving, `2` stressed, `3`
  collapsing — the `moraleCause:ubyte` idiom; `none` is the default so "no pasture" can never be
  misread as "healthy pasture"). **Measured, not assumed** (earthlike 80×52, 1511 patches): the
  TileState fields cost **+12.9 KB** on a 3.63 MB FlatBuffers snapshot (**+0.36%**) and **+0.58 ms**
  on a ~22 ms turn; the rollback record costs +55.9 KB (+1.6%). A `ScalarRaster` channel — the obvious
  alternative for a dense per-tile scalar — would cost **33.3 KB** (2.6× more: it pays for all 4160
  tiles, water included), carry **one** scalar instead of three (no capacity → no % → no overgrazing
  signal on the tile card), and re-ship **whole** on any single tile's change, where `TileState` is
  **per-entity diffed** and so costs *zero* delta bytes on an ungrazed turn. The dense shape is the
  one place graze deliberately diverges from `ForagePatchState`.
- **Forage-potential twin — `TileState.forageCapacity:float`** (append-only, beside the graze fields on
  both `WorldSnapshot` and `WorldDelta`). The exact human-food mirror of `grazeCapacity`, so the client
  can draw a **Forage overlay** the same way it draws the pasture one. Sourced **directly from
  `forage.capacity_by_biome` (`ForageLaborConfig::capacity_for(tile.terrain)`)** for *every* tile —
  **not** from the `ForageRegistry` — so the biome's potential shows on **every** tile, including the
  water and the bare rock that hold no `ForagePatch` at all. (**Corrected:** this used to claim the
  registry was *sparse* — "~95% of tiles, all the best cropland, carry no `ForagePatch`". That is
  **false** and was measured false: `classify_food_module` tags essentially every biome, so
  `spawn_initial_forage` seeds a patch on **every** food-bearing tile — standard map, **2328
  food-bearing tiles, 2328 patches, zero bare**. The claim predates the per-biome capacity table, and
  it is what the `Sow` design originally reasoned from; see "The `Sow` verb + the Field".) Consequence, preserved deliberately: it is
  **non-zero on fishery water** (`ContinentalShelf` 130 / `CoralShelf` 180 / `InlandSea` 110 — a fishery
  is a food module on water), a real divergence from graze where all water is 0; only a *stated-zero*
  biome (deep ocean, glacier, lava, salt flat) reads 0. On a food-module tile that *does* hold a
  `ForagePatch`, that patch was seeded at the same `capacity_for(biome)`, so `forageCapacity` equals the
  patch's `carryingCapacity` — no drift between the potential and the realized patch. Cost: **+1 float
  per tile** (per-entity diffed, so zero delta bytes on an unchanged tile). Populated at capture beside
  the graze fields in `snapshot.rs::tile_state`.
- **Distribution, measured on real maps** (`integration_tests/tests/graze_distribution.rs` — run with
  `--nocapture` for the histogram; the guards keep the model claims true under retuning). Earthlike
  80×52, three seeds: ~1500–1560 land tiles carry ~162–177 k total graze capacity, and only
  **0.8–1.0% of land is zero-graze** (glacier / volcanic / fumarole). Prairie is the richest per-tile
  pasture (240), as intended. Two earlier findings are now **closed**: the `FertileLowland` palette
  niche is no longer thinned (`k_small` 2 → 4, `map_presets.json`), so **forest and floodplain exist on
  the standard map** — the flagship inversion is observable in play — and `AlluvialPlain`, which was
  absorbing both of them as their niche-mate, no longer carries the map's pasture: at graze 110 its
  share of total graze falls to ~16–24% (from 37–48%), and the *dominant* pasture is the steppe again,
  not the fallback biome. See "The two food webs" for the joint (graze + forage) measurement.
- **Follow-ups:** the **client** pasture overlay + tile-card readout — and the twin **Forage overlay**
  off `TileState.forageCapacity` (both are client-dev slices: the data is on the wire; note each overlay
  must be built from `TileState`, since neither graze nor forage is a raster channel). **Phase 2b**
  (herds eat it, `K_herd` = `range graze flow / fodder_per_biomass`) and **Phase 2d** (the pen becomes
  fenced land, retiring `pen.capacity_fraction`) have since landed.

### Phase 2b-i — herds eat their range, movement is graze-aware (INERT on K)

The first 2b slice (`docs/plan_grazing_2b.md` §8). Herds now **draw the graze layer down** on the
tiles they occupy, and **movement avoids barren ground** — but **carrying capacity is still the
species constant**, so the hunting economy (hunt/forecast yields) is byte-identical to 2a. This
de-risks the K change (2b-ii) by proving the eating + movement first, exactly as 2a shipped the graze
layer inert.

- **`grid_utils::hex_range_tiles(center, radius, w, h, wrap)`** — every tile within odd-r hex distance
  `radius` (the hex disk: `1, 7, 19, …`), wrap-aware horizontally + pole-clamped. Bounding-box scan +
  exact `hex_distance_wrapped` filter. Shared by the herd range (and the pen/anything later).
- **`SpeciesDef.fodder_per_biomass`** (`fauna_config.json`, `#[serde(default)]`) — the fodder one unit
  of animal biomass demands per turn. **Cached onto `Herd` at spawn** (mirroring `carrying_capacity`)
  and round-tripped through the rollback snapshot (`HerdState.fodder_per_biomass`, sim-side only — not
  on the client wire). Shipped anchors (smaller animals eat MORE per unit biomass; **inert this slice**,
  retuned from a measured anchor in 2b-ii): rabbit **0.10** / fowl **0.09** / boar **0.06** / deer
  **0.05** / steppe_runner **0.05** / marsh_grazer **0.03** / mammoth **0.011**. Each is
  `range_tiles × per-tile MSY (0.1·capacity) ÷ species K`, so a herd near its constant K eats ~its
  range's sustainable graze flow and holds the range near half capacity.
- **`Herd::graze_range_radius(&SpeciesDef)`** — the footprint a herd grazes, from `size_class`: Small
  → **0** (its one tile), Big → **1**, Migratory → **loiter_radius** (the current loiter cluster, not
  the whole route).
- **`advance_herd_grazing`** (Logistics, registered **after `advance_herds`** and **before
  `advance_graze_regrowth`**) — the `forage_take`-style draw-down: each **mobile, non-corralled** herd
  demands `fodder_per_biomass × biomass` and draws it from its range's `GrazeRegistry` patches,
  **proportional to each tile's available graze** and floored at each patch's `reseed_floor_fraction ×
  capacity` (never permanently kills a tile). Herds draw **sequentially in `HerdRegistry` order** (that
  Vec is rollback-persisted in a fixed order), so a shared tile is order-independent under rollback.
  Corralled herds are fed from the larder (`pen_upkeep`), not the land, so they are skipped.
- **Graze-aware movement** (§4.1): `advance_herd_roam` (`best_land_neighbor_toward` /
  `wander_near_anchor`) **never steps onto a zero-graze tile** (no patch / zero capacity) and **biases
  toward higher graze capacity** among candidates, folding graze into the *existing* per-turn seeded
  RNG (deterministic under rollback). A herd hemmed in by barren stays put. `build_route` (spawn-time)
  biases migratory anchors onto the most fertile nearby ground, reading capacity **directly from
  `graze.capacity_by_biome`** (graze patches don't exist yet — `spawn_initial_herds` runs before
  `spawn_initial_graze`). Movement keys off **capacity** (stable land fertility), *not* live biomass —
  chasing *receding* grass (leaving a cluster because it was eaten out) is the emergent 2c dynamic,
  deliberately deferred. `advance_herds` takes the graze layer as `Option<Res<GrazeRegistry>>`: a
  `None`/empty registry falls back to plain land movement (the isolated fauna test harnesses).
- **Measured** (`core_sim/tests/grazing_2b.rs`, earthlike seed 119304647): herd-occupied pasture sits
  below untouched pasture (grazing visibly draws range down); a vacated cluster recovers to capacity
  once herds leave; ~0 herds end a turn on a zero-graze tile (movement avoids barren). NB the 2b-i
  draw-down floor moved from the reseed floor to `graze.overgraze_escapement_fraction` in 2b-ii.

See Also: `docs/plan_grazing_foundation.md` (design), `docs/plan_grazing_2b.md` (the 2b arc),
"Depletable Forage" (the human-edible twin and the `ForageRegistry` pattern this mirrors), "Fauna &
Wild Game" (the model this becomes the substrate of in Phase 2b).

### Phase 2b-ii — carrying capacity becomes ecological; `regrowth_rate` becomes per-species

The big rebalance (`docs/plan_grazing_2b.md` §2/§3/§5). A mobile herd's `K` is **no longer the species
constant** — it is derived each turn from the graze its range yields, and each wild species breeds at
its **own** rate. Gated by a convergence test (§2.2), because a coupled consumer–resource system
oscillates or crashes if built carelessly.

- **`K` is range-derived, recomputed in `advance_herds`.** After a mobile (non-corralled) herd roams,
  `ecological_carrying_capacity` sets `herd.carrying_capacity =
  Σ_range graze_sustainable_flow(G_tile) / fodder_per_biomass` over `hex_range_tiles(current_pos,
  graze_range_radius)` — the **same** tiles `advance_herd_grazing` eats, at their **current** (drawn-
  down) biomass. So overgrazing a range lowers its flow → lowers `K` → shrinks the herd (the emergent
  overgrazing spiral); a range held at/above its MSY point yields full flow → `K` at max. This is the
  **one** write; `herd_capacity(herd, fauna)` still reads the cached field, so **every downstream
  consumer is unchanged** (no `&GrazeRegistry` threaded through the ~15 capacity call sites). Since
  **Grazing 2d** a **corralled** herd's `K` is likewise recomputed — over its *fenced footprint*
  (`hex_range_tiles(corralled_at, pen_radius)`), via the same `ecological_carrying_capacity` seam (a
  wholly-barren footprint keeps the frozen `K` and is fully larder-fed). A non-grazing herd
  (`fodder ≤ 0`) or an absent graze layer keeps the constant `K`.
- **`graze_sustainable_flow` — NOT `sustainable_yield`.** The K flow is pure logistic at the MSY-clamped
  biomass (`logistic_regrowth(min(G, cap/2), cap, r_graze)`), deliberately **without** the Allee cutoff
  `sustainable_yield` applies — **grass has no depensation**, so a heavily-but-recoverably grazed tile
  must still yield a positive `K` (the design's formula named `sustainable_yield`, but that would read
  `K = 0` below `collapse_fraction` and crash a herd on ground that in fact regrows).
- **Per-species `regrowth_rate` (`SpeciesDef.regrowth_rate: Option<f32>`, `#[serde(default)]`).** Cached
  on `Herd` at spawn (`regrowth_rate_or(fauna.ecology.regrowth_rate)`), round-tripped through
  `HerdState.regrowth_rate` (sim-side only). **`herd_ecology` now returns an owned `EcologyConfig`**
  with the wild curve's `regrowth_rate` swapped for the herd's own (phase bands stay shared); pastoral
  (0.25) / pen (0.90) keep their rung's rate. This is still THE single seam — every consumer reads the
  folded rate there. Anchors: rabbit/fowl **0.35**, deer/boar **0.10**, migratory **0.04** (was one
  global 0.05). **This is the PR #117 fix**: small game bred at a mammoth's rate was the artifact behind
  "a rabbit warren can't provision an expedition."
- **The convergence gate — `graze.overgraze_escapement_fraction` (0.25).** Grazing (`graze_take`) may
  draw a patch down to this fraction of capacity but **no lower** in a turn — constant-*escapement*, the
  same lesson the corral learned (`docs/plan_corral_managed_population.md` §3). Without it the herd's
  constant-*catch* demand strips an over-subscribed range into a permanently-stripped attractor at the
  reseed floor (a stunted remnant on dead ground); with it an **overgrazed range recovers** to a stable
  smaller herd. Validated `>` `reseed_floor_fraction` and `< 0.5` (the graze MSY point — overgrazing
  below the productive intensity stays possible/visible). It bounds `K` below at ≈ 0.84·`K_max`, so
  overgrazing shrinks a herd by ≤ ~16% — a modest but stable force; lower it for deeper overgrazing at
  rising crash risk.
- **Turn order (discretization that converges):** recompute `K` from **pre-eat** graze → herd grows
  toward it (clamped) → herd eats (`advance_herd_grazing`) → graze regrows (`advance_graze_regrowth`).
  The hard clamp `biomass ≤ K` plus the flat-K plateau above `cap/2` plus the escapement floor make it
  converge monotonically (no growing oscillation) from **every** start.
- **Measured — the convergence gate** (`core_sim/tests/grazing_2b_convergence.rs`, ≥300 turns, pinned):
  every regime (rabbit `r`=0.35, deer 0.10, mammoth 0.04, and the hottest `r`=0.40 = graze) reaches a
  **stable fixed point** from under-grazed / over-populated / over-grazed / two-herds-sharing starts;
  under- and over-populated starts converge to the **same** `K`; an overgrazed range (graze 0.12)
  **recovers** to graze ~0.33–0.61 / herd 88–100% `K_max`, never the stripped floor; the coupled system
  is deterministic (two runs bit-identical). Biomass tail bands are 0; the graze fraction holds a fixed
  ≤0.7% micro-2-cycle (a small band, not growing).
- **Measured — the K distribution + hunting economy** (`grazing_2b::the_2b_ii_measurement_report`,
  earthlike seed 119304647, 120 turns): Red Deer `K` mean **1352** (460 forest → 2150 steppe) vs the
  retired **1200**; Rabbit **163** (48–240) vs 200; Wild Boar **1049** vs 1000 — the sedentary species
  land near their old constants with real biome spread. Migratory `K` came in **below** the old
  constants (Steppe Runners 3212 vs 9000, Marsh Grazers 5629 vs 9000) — their loiter-cluster range ×
  cap doesn't reach the old biomass-max, a **retune flag** (lower migratory `fodder` to raise `K` if
  the megafauna hunting economy wants it). Sustain MSY (`r·K/4·p`) roughly **doubled** for deer/boar
  (both `r` and `K` up) and rose **~5.7×** for rabbit (**0.05 → 0.285** food/turn) — the **small-game
  viability reversal**: a rabbit warren is now a fast provisioner (and the small/Market hunting
  expedition, which never filled under the old uniform `r`, now completes).
- **The fast-breeder ladder inversion — FIXED in 2d.** A wild rabbit's `r`=0.35 exceeded the retired
  flat pastoral 0.25, so taming a rabbit *used* to be a growth downgrade. Grazing 2d makes the managed
  rungs a *multiple* of each species' own wild `r` (§ "Phase 2d"), so pastoral `r = wild_r × 2.0 >
  wild_r` for every species and the inversion is gone.
  `fauna_husbandry::the_husbandry_ladder_is_a_per_species_growth_rate_ladder` asserts the per-species
  gross growth-rate ladder — as a **long-run average**, for the reason set out in "The husbandry yield
  ladder" (escapement makes a single turn read stock, not rate).

See Also: `docs/plan_grazing_2b.md` §2.2 (the convergence risk), §9 (the measure list),
`docs/plan_corral_managed_population.md` §3 (the constant-escapement lesson this reuses).

### Phase 2d — the pen economy: a pen becomes fenced land

The pen slice (`docs/plan_grazing_2d.md`). A pen stops being a special case (a single frozen tile fed
entirely from the larder) and becomes **a piece of fenced land the herd grazes**:

- **`Herd.pen_radius`** (default `0` = today's single tile) — the pen's footprint is
  `hex_range_tiles(corralled_at, pen_radius)`. All footprint logic (`herd_footprint`) reads it; the
  `ExtendPen` command grows it (2d-β, below).
- **Footprint `K`** — `advance_herds` recomputes a penned herd's `K` over its footprint via the same
  `ecological_carrying_capacity` seam a mobile herd uses (penned herds stop being frozen). A
  **wholly-barren** footprint keeps the frozen `K` and is fully larder-fed (§2.3's preserved worst case).
- **Penned grazing** — `advance_herd_grazing` no longer skips corralled herds; a pen draws its footprint
  down with the same `graze_take` + `overgraze_escapement_fraction` (0.25) floor as a wild herd,
  capturing `footprint_intake`.
- **The larder offset** (§2.3) — the FEED phase pays only `pen.upkeep_per_biomass × biomass ×
  (1 − pasture_fraction)`, `pasture_fraction = clamp(footprint_intake / (fodder_per_biomass × biomass),
  0, 1)`; `pen_fed_fraction` = the total fed share (pasture + the paid part of the reduced bill). The
  food-ledger identity (`penFeedUpkeep`) is untouched — it draws the *actual* paid amount.
- **Per-species husbandry `r`** (§3) — retires flat pastoral 0.25 / pen 0.90 for `min(cap, wild_r ×
  gain)` (`pastoral_gain` 1.5, `pen_gain` 3.0, `husbandry_regrowth_cap` 0.75). `capacity_fraction` /
  `pen_capacity` are **deleted**; `herd_capacity` collapses to `herd.carrying_capacity`.
- **Per-species husbandry DENSITY (K)** — the ceiling twin of the `r`-gains: the per-species
  `SpeciesDef.pastoral_density` / `pen_density` (default **1.0**) multiply a tamed / penned herd's
  range-or-footprint-derived `K` at the one seam `ecological_carrying_capacity` (via
  `fauna::herd_density_gain`), so domestication makes the land hold *more* animals, big for the prime
  grazer domesticates (goat/aurochs **2.0 / 5.0**). Orthogonal to `r`, byte-identical for a wild herd
  (`×1.0`), and scale-free in the pen net-positive floor. See "The husbandry yield ladder" for the
  roster and the resolver.
- **The net-positive invariant** is reworked to a **best-case floor** (§2.4): validate guarantees only
  the *fastest* species' pen nets positive when fully larder-fed; a slow breeder or poor-pasture pen may
  run at a **loss by design** (it pays off only when self-feeding drives upkeep → 0).
- **Wire** (append-only on `HerdTelemetryState`): `penRadius`, `penFootprintTiles` (server in-bounds
  count), `penPastureFraction`, `penExtendProgress`. Convergence gated by
  `core_sim/tests/grazing_2d_pen.rs` (a pen converges at radius 0/1; lush → free, barren → full bill).

**2d-β — the `ExtendPen` command + build ladder** (§4). Growing a pen's fenced footprint is a labor
investment worked off over turns, reusing the corral build ladder — no materials economy:

- **`Command::ExtendPen { faction, target_x, target_y }`** (full proto/runtime/text/server plumbing —
  `ExtendPenCommand` proto field **39**, verb `extend_pen <faction> <x> <y>`), routed like `Corral`
  through `handle_extend_pen`. It reuses `CommandEventKind::Corral` (one kind for the pen's whole life).
  Validation (each with a clear rejection): a herd **penned exactly at that tile** (`corralled_at`, the
  fixed anchor — *not* the roaming `position()` `corral` keys off), owned by the faction, the faction
  knows **Herding**, `pen_radius < husbandry.pen_radius_max`, **no extension already in flight**, and a
  band is **keeping** it (a Hunt assignment on the herd — else the ring never accrues and an untended
  pen escapes anyway). On success it sets the herd's **`pen_extending`** state via
  `Herd::begin_pen_extension` (which re-checks penned / not-extending / below-max, so the command's
  validation and the mutation can never disagree).
- **The build ladder** rides the corral-tend branch of `advance_labor_allocation`: while `pen_extending`,
  the keeper's HARVEST is **dipped to the `animal:pen` rung's `yield_fraction_while_building`** (the forgone yield *is* the labor
  cost of the ring, the same dip the corral *build* pays), and `Herd::accrue_pen_extension` adds
  that same rung's `progress_per_turn` (0.04 → ~25 turns/ring) to `pen_extend_progress` **after**
  the take. At `1.0` the ring completes: `pen_radius += 1` (saturating at `pen_radius_max`),
  `pen_extend_progress` resets, `pen_extending` clears, and a `Corral` feed line fires; the larger
  footprint's higher K arrives on the next `advance_herds`. The FEED (larder offset) is unchanged while
  extending — self-feeding and the harvest dip are orthogonal.
- **Config:** `husbandry.pen_radius_max` (**2** → up to a 19-tile footprint; validated `>= 1`). The only
  new lever. **`pen_extending`** persists on `HerdState` alongside `pen_radius` / `pen_extend_progress`,
  so a rollback rewinds an in-flight extension. `penExtendProgress` on the wire now carries the live ring
  meter (α left it at 0) for a client "Fencing N%" badge.
- **Tests:** `grazing_2d_pen::extend_pen_accrues_a_ring_flips_the_radius_raises_k_and_caps_at_max` (the
  ring accrues over ~25 turns, flips `pen_radius` 0→1, K rises with the 7-tile footprint, and caps at
  `pen_radius_max`); `server::tests::extend_pen_*` (the five validation rejections + the happy path).
- **Deferred (2d-γ, client):** the footprint highlight, the feed-split readout (`penPastureFraction` +
  `penUpkeep`), and the extend affordance / "Fencing N%" badge (`penExtendProgress`).

**2d-δ — the husbandry ceiling: which species climb the ladder** (§4a). Not every animal can be herded,
and not every herdable one can be penned. The ladder is a **sequence** (wild → pastoral → pen), so a
species' reach is a single **enum** (`fauna_config::HusbandryCeiling` = `Wild | Pastoral | Pen`), not
two flags — which makes the incoherent "pennable but not tameable" state unrepresentable (no
`validate()` combo guard).

- **`SpeciesDef.husbandry_ceiling`** (`#[serde(default)]` = `Pen`, so an untagged/future species keeps
  the full ladder) is **cached onto `Herd` at spawn** (mirroring `regrowth_rate`/`fodder_per_biomass`),
  round-tripped through `HerdState.husbandry_ceiling`, and read by the gates via `Herd::can_domesticate()`
  / `can_pen()`. Roster: **mammoth/deer = `wild`** (hunt-only), **steppe_runner/marsh_grazer =
  `pastoral`** (nomadic herding — follow, don't fence), **boar/rabbit/fowl = `pen`** (pigs/hutches/poultry).
- **Three gates.** (1) **Domestication accrual** — `Herd::accrue_domestication` self-guards on
  `can_domesticate()`, so a `wild` species never tames and never picks up an `owner` (robust regardless
  of call site). (2) **The `domesticate` claim** — `handle_domesticate` rejects a `wild` species
  ("{Species} is wild game — hunt-only…"). (3) **The `corral` / `extend_pen` commands + the `Corral`
  policy accrual** — `validate_labor_policy` (shared by `handle_corral` and `assign_labor … corral`) and
  the `Corral` accrual in `advance_labor_allocation` both require `can_pen()` (only `pen`), so a
  `pastoral` species tames and roams but the pen path is closed ("{Species} cannot be penned").
  `handle_extend_pen` carries the same check belt-and-braces (unreachable via the gated corral path).
- **Wire:** `HerdTelemetryState.husbandryCeiling:string` (`wild`|`pastoral`|`pen`; append-only, mirrors
  `sizeClass`/`ecologyPhase`) so the client can hide the corral/extend affordance on a non-`pen` herd and
  the whole domestication track on a `wild` one.
- **Note — a mid-build gate change:** the `Corral` accrual gate is checked each turn, so a
  (command-unreachable) non-`pen` herd mid-corral-build would simply **stop progressing** — a soft
  stall, not a crash — and there are no shipped saves to carry such a state.

---

## Pre-commit Yield Forecast (per-source, on the wire)

The **retained yield telemetry** (`SourceYield.actual/sustainable/workers_needed`, above) is
**post-hoc** — the player only learns they over-assigned *after* committing and advancing a turn. The
forecast is its pre-commit twin: per in-range source, the snapshot exposes enough for the client to
show a live **"Expected yield: +X.XX /turn"** and **cap its worker stepper at the max-useful count
while the player is composing an assignment**.

**Wire fields** (append-only, on both `WorldSnapshot` and `WorldDelta`) — the same shape on
`ForagePatchState` (per tile) and `HerdTelemetryState` (per herd):
`perWorkerYield:float` + `ceilingSustain` / `ceilingSurplus` / `ceilingMarket` / `ceilingEradicate`
(all `float`, **food/turn**, at the source's CURRENT biomass), **plus the investment rung**:

> **The hunt ceilings are the STEADY sustainable per-turn rate — the credit bank drives the lumpy
> TAKE, not the displayed readout.** `hunt_forecast`'s `ceiling` closure passes `credit = 0.0` to
> `hunt_credit_ceiling`, so each extractive/investment ceiling is `min(hunt_policy_rate, biomass)` —
> the sustainable rate the confirmed-allocation row already headlines (`sustainable_yield`) — **not**
> the credit-inclusive `min(credit + rate, biomass)` this-turn burst the take path cashes. For a slow
> breeder whose MSY < `body_mass` (e.g. Wild Aurochs, `r ≈ 0.09`) the bank accumulates ~a whole animal,
> and quoting `credit + rate` inflated every extractive ceiling by that banked amount — reading the Tame
> dip *above* its own payoff and Sustain *above* Tame, inverting the ladder. Steady, the compose
> forecast agrees with the resolved headline (no jump between them) and the aurochs ladder reads in
> order: `Sustain 0.72 < Surplus 1.08 < Market 1.80`, Tame dip `+0.36 → payoff +1.44`, Corral payoff
> `2.88`. **Eradicate is unchanged** (its rate is the whole stock `B`; it bypasses the bank). The take
> path (`hunt_take` / `hunt_credit_ceiling`) keeps the bank untouched — only the readout is steady.
> Pinned by `fauna::tests::the_forecast_ceilings_are_the_steady_rate_not_the_banked_burst` (a full-bank
> herd) and the empty-bank `forecast == actual` tests. **Forage has no credit bank** (foraging is
> continuous), so `forage_forecast`'s ceilings were already steady `sustainable_yield` — unchanged.

`ForagePatchState.ceilingCultivate` + `tendedYield` and `HerdTelemetryState.ceilingCorral` +
`corralYield`. The investment policy's `ceiling*` is the **preparing** yield
(`fraction × ceilingSustain` — the dip); `tendedYield`/`corralYield` is what the source will pay
**once the improvement completes**, so the client can show **"preparing X → then Y"** *before* the
player commits to the cost. (Sim-side both live on the shared `SourceYieldForecast` as
`ceiling_prepare` / `managed_yield` — the two investment policies are kind-exclusive, so one field
serves both.) **The `Tame` rung has its own payoff twin: `HerdTelemetryState.pastoralYield`** (sim
`SourceYieldForecast::pastoral_yield`) — what a Sustain hunt pays **once the herd is tamed**, so the
client can render Tame's `→ +Y` instead of quoting only its during-building dip (`ceiling_tame`, which
reads *below* wild Sustain and hides that taming out-yields wild hunting). `0` on a source that never
offers Tame (a forage patch, or a herd already penned/forage-tended). **Both `pastoralYield` and the
un-penned `corralYield` projection (`managed_yield`) are the SUSTAINED MSY on the improved ecology** —
`hunt_provisions(sustainable_yield(biomass_before_regrowth, carrying_capacity, &{pastoral,pen}_ecology_for(..)))`,
the long-run rate — **NOT** the one-turn constant-escapement take. Because MSY is `r`-dependent while
escapement (`max(0, B − K/2)`) is `r`-independent, the sustained form is what makes the ladder visible
at a single turn: **`ceiling_sustain < pastoral_yield < managed_yield`** (wild `r·K/4` < pastoral
`r×2.0` < pen `r×4.0`, MSY-capped; measured ≈ 0.5 < 1.0 < 2.0 on a full Wild Boar). The old escapement
projection read `pastoral_yield == managed_yield` (≈ 10 = 10) and could not show the ladder the field
exists for. **The penned-herd `managed_yield` stays the escapement take** — a live corralled herd hits
`hunt_forecast`'s `is_corralled()` early-return, which returns `corral_provisions` (the actual
constant-escapement corral yield), so forecast == actual for a real pen; only the *un-penned
projection* is the sustained MSY. Pinned by
`fauna::tests::the_tame_rung_advertises_its_payoff_above_the_dip_and_wild_sustain`.
- `perWorkerYield` = food/turn one worker contributes (throughput → provisions; **forage folds in the
  tile's `seasonal_weight`**, as `forage_take` does — it can be `0` in a dead season, so consumers must
  not divide by it; hunt has no seasonal factor).
- Each `ceiling*` = that policy's food/turn cap, **already clamped to the source's remaining biomass**.
- Captured at `output_multiplier = 1.0` (the productivity multiplier is per-band): the client scales
  every field by the acting band's `PopulationCohortState.outputMultiplier` — a linear factor, so
  `max_useful_workers` is invariant to it.
- Client composition: `expected(workers, policy) = min(workers × perWorkerYield, ceiling[policy])`,
  `max_useful_workers(policy) = ceil(ceiling[policy] / perWorkerYield)`.
- A **rung-3 managed source** (a sown **Field** / a **corralled herd**) is *yours*, so **the policy axis
  collapses**: every ceiling is its managed yield (`SourceYieldForecast::managed`). **The worker cap does
  not collapse** — `perWorkerYield` is the crew's real throughput, so `max_useful_workers =
  ceil(production / perWorkerYield)` is an honest count that grows with the source (slice 7; it used to
  be a hardcoded `1`, which claimed one worker could carry home whatever the land offered). A **tended
  patch is NOT this shape** — it is rung 2, a wild stand on a boosted curve, and forecasts policy-live
  like a wild patch.

**Invariant: forecast == actual — no duplicated yield math.** The forecast and the take path read the
*same* pure helpers, so the UI can never promise a number the sim won't pay:
- forage (`forage.rs`): `forage_policy_ceiling` (the 4 extractive rungs **+ Cultivate**, biomass) · `forage_per_worker_biomass`
  (`per_worker_biomass_capacity × seasonal`) · `forage_provisions` (biomass→provisions ×
  `output_multiplier`) · `tended_provisions` (the tended-patch managed harvest) — all called by both
  `forage_take` / the tended-patch arm of `advance_labor_allocation` **and** `forage_forecast`.
- fauna (`fauna.rs`): `hunt_policy_ceiling` (the 4 extractive rungs **+ Corral**) · `hunt_provisions` ·
  **`managed_yield_biomass`** (the husbandry harvest, via `pen_yield_biomass`) · **`herd_ecology` /
  `herd_capacity`** (which ecology/capacity a herd lives under — *no call site may re-derive either*) —
  called by both `systems::hunt_take` / the corral arm of `advance_labor_allocation` **and**
  `hunt_forecast`. The shared `SourceYieldForecast` struct (with `::tended`) is the common return shape.
  A corralled herd's `managed_yield` is **gross**; its `penUpkeep` is exported separately.
- Guarded by `systems::labor_yield_tests::{forage,hunt}_forecast_equals_actual_take_for_every_policy_and_staffing`
  (every policy × labor-bound/ceiling-bound staffing, comparing against the payout of a real
  `advance_labor_allocation` run) and `tended_patch_and_corral_forecast_full_yield_with_one_worker`.
  **Any change to the take math must go through these helpers** — never re-derive a ceiling or a
  biomass→provisions conversion at a call site.

Capture: `snapshot_forage_patches` / `herd_snapshot_entries` (`snapshot.rs`); the herd's
`carrying_capacity` (absent from the display telemetry) is resolved from the authoritative
`HerdRegistry`, and the per-tile `seasonal_weight` from the `FoodModuleTag` query.
**Client follow-up:** rendering the live "Expected yield" line + the worker-stepper cap in the
forage/herd assign controls.

### Assign-time yield seeding (the `+0.00` fix)

The retained `SourceYield` telemetry used to be written **only** during turn resolution, so between
"player assigns workers" and "player advances the turn" a brand-new source had no row and the display
snapshot serialized `actual_yield = 0.0` — the map annotation and the Band panel read **`+0.00`** for
every fresh assignment, and the client cannot distinguish "0 because not computed yet" from "0 because
the source is barren". Fixed server-side: `handle_assign_labor` (and the `cultivate`/`corral` policy
shorthands, via `set_policy_on_working_bands`) **seeds the touched source's `SourceYield` from its
pre-commit forecast** right after mutating the `LaborAllocation` (`server.rs::seed_source_yield` →
`LaborAllocation::set_source_yield`). Because forecast == actual (above), the seeded number is exactly
what the turn then pays under unchanged conditions — **no jump** — and it is the same number the
client's compose-time "Expected yield" row promises. Shape:
- **The expected take** is the one shared helper `fauna::forecast_expected_take(&SourceYieldForecast,
  workers, policy) = min(workers × per_worker_yield, forecast.ceiling_for(policy))`
  (`SourceYieldForecast::ceiling_for` is the `ceiling[policy]` lookup; the two investment policies
  share `ceiling_prepare`, the reduced `yield_fraction_while_building` bite of the rung being built —
  once the improvement *completes* the source is `::tended`, whose every ceiling already **is**
  `managed_yield`). The client preview, the seed, and the forecast==actual tests all call it.
- The kind-specific seeds `forage::forage_source_yield_preview` / `fauna::hunt_source_yield_preview`
  compose the full row through the shared `forecast_source_yield`: `actual` = the expected take,
  `sustainable` = the same MSY value the resolution path records (a *managed* source — **rung 3 only**
  — reads `sustainable == actual`, no ⚠), `workers_needed` = the same overstaffing signal the resolution
  path writes (the continuous inversion for a forage patch; the **steady peak-drop carry crew**
  `hunt_haul_workers` off `SourceYieldForecast::ceiling_for` for a whole-animal source, so the seed
  matches the client's max-useful cap), and `wasted` = the understaffing mirror. No new formula, no new
  config lever.
- **Only the source the command touched** is seeded (other sources keep their real actuals), and only
  where the turn would actually pay: out of `band_work_range` / past the hunt leash, an unseeded patch
  or a vanished herd keeps its zero row, and a **genuinely barren source still seeds `0.0`** — `+0.00`
  stays reachable, and correct, there. Consequence (intended): a fresh assignment now *previews* its
  contribution to the Food-line net rate + the Gathered/Hunted breakdown, and can pre-trip the
  overdraw ⚠ if the chosen policy would overdraw — ⚠ is a leading flow signal by design.
- `LaborAllocation` now keeps `last_yields` **index-aligned with `assignments`** across every mutation
  (`set_assignment`/`normalize`/`clear` — the snapshot zips the two by index, so a row left behind by a
  removed assignment used to be attributed to the *next* source). New rows default to
  `SourceYield::ZERO`.
- Guarded by `server::tests::{assigning_forage,assigning_hunt}_workers_seeds_the_expected_yield_before_the_turn`,
  `resolved_{forage,hunt}_yield_equals_the_seeded_yield` (the no-jump property),
  `changing_the_policy_reseeds_the_expected_yield`, `a_barren_source_seeds_zero`,
  `unassigning_a_source_drops_its_yield_row`.

---

## The Telling — the narrative beat engine

Watches sim state, fires **edge-gated** narrative beats, dresses them in nouns drawn from the live
world, and pushes them through the existing `CommandEventLog`. The feed stops saying
"Sedentarization available" and starts saying *"The river-bend remembers us now."* Authoritative
design: `docs/plan_the_telling.md` (§2 schema, §3 runtime); concept `docs/Emergent Narrative.md`.

**Shipped: PR-A (the ambient/beat tiers) + PR-B (the fork tier, stance, and re-colouring) + PR-C
(memory threads, callback predicates, and the maturing voice).** All three tiers are live; see "The
fork tier", "Stance", "Memory threads", "The `answered` gate" and "The maturing voice" below.

### Three layers, and the mod surface is content

| Layer | Home | Moddable | Deterministic |
|---|---|---|---|
| **Engine** — trigger eval, edge gating, fired-set, selection, noun resolution | `core_sim/src/telling/` (Rust) | no | yes, seeded |
| **Content** — souls, wardrobes, conditions | `src/data/beat_*.json` | **yes — this is the mod API** | yes, by construction |
| **Presentation** — how a beat renders | Godot client (generic feed today) | yes | n/a |

### Module layout (`core_sim/src/telling/`)

| Module | Owns |
|---|---|
| `mod.rs` | `BeatLedger`, `telling_tick`, the snapshot round-trip, re-exports |
| `config.rs` | `BeatConfig` + `BeatConfigHandle`/`BeatConfigMetadata` (`sedentarization_config.rs` loader convention) |
| `catalog.rs` | `BeatCatalog` / `BeatDefinition` / `WardrobeEntry` + the load-time validation |
| `predicate.rs` | the `when` grammar and its evaluator |
| `signals.rs` | **the signal registry** — extension point |
| `nouns.rs` | **the noun resolver registry** + template rendering — extension point |
| `select.rs` | weighted wardrobe selection + the determinism recipe |
| `stance.rs` | **stance** — signal normalization, declared offsets, the effective value, the re-colouring term |
| `memory.rs` | **memory threads** — the durable nouns callbacks are made of, their selectors, and eviction |
| `medium.rs` | **the maturing voice** — the medium ladder and its never-regress rule |

### Turn-pipeline placement

**`TurnStage::Telling`, between `Crisis` and `Finalize`** — it must see population, fauna,
sedentarization and crisis output, and must land **before `Snapshot`** so a beat reaches the client
the same turn it fires. `telling_tick` carries **no `run_if` capability gate**: narration is always
on. (The ordering that actually binds is the tuple position in `.configure_sets(Update, (…).chain())`,
not the enum declaration order.)

### The two extension points (the engine/content boundary)

Content **composes** signals and resolvers; it cannot invent them. Adding either is a deliberate
engine change, which keeps the surface auditable and every condition cheap.

**Signals** (`signals.rs`) — sampled to `f64` **once per turn**, at the top of `telling_tick`, into
one `SignalSample`, so every predicate in a turn sees a consistent snapshot and each source is read
once. Sampled for the **player faction** = `FactionRegistry`'s first entry in `FactionId` order
(there is no `player_faction` accessor; `FactionId(0)` is effectively the player).

| Signal | Source |
|---|---|
| `turn.index` | `SimulationTick.0` |
| `band.count` | summed `PopulationCohort.size` over `With<ResidentBand>` — **total people**, not a band count (the name is historical; the content reads "There are {count} of us") |
| `provisions.total` | **band-local larders, summed** — `Σ cohort.stores[FOOD]` over `With<ResidentBand>`. Provisions left `FactionInventory` entirely in the population-economy arc |
| `sedentarization.score` | `SedentarizationScore::score(faction)` |
| `sites.discovered_this_turn` | `DiscoveredSites`' per-faction record count diffed against last turn's, retained in the ledger under the reserved key `internal.sites.discovered_total` (**not** a registered signal — content sees only the diff) |
| `discovery.progress.cultivation` / `.herding` | `DiscoveryProgressLedger::get_progress` on `CULTIVATION_DISCOVERY_ID` (2003) / `HERDING_DISCOVERY_ID` (2004), 0..1 |
| `fauna.collapsing_group_count` | `HerdRegistry::entries()` in `EcologyPhase::Collapsing` |
| `culture.axis.<snake_axis>` | one per `CultureTraitAxis`, off the **faction culture rollup** — `CultureManager::faction_trait_average`, a **population-weighted average of the faction's resident bands' local layers** (`CultureOwner::from_entity`, weighted by `PopulationCohort.size` over `With<ResidentBand>`; expeditions are detached and don't vote), falling back to the global layer when a faction has no local layers. PR-A read the global layer directly because no rollup existed |
| `voice.medium_index` | the narrator's **attained** medium as a 0-based index into `voice.mediums`. Injected by `telling_tick` (not `sample_signals`) right after the stance axes, so it is in the sample before anything evaluates — the `voice.medium_*` beats gate on it with `crosses` |
| `stance.<axis>` | one per configured `stance.axes[]` — the axis's **effective** value, `clamp(normalize(signal) + declared_offset, -1, 1)`. **Config-driven, so it resolves through `BeatConfig`, not the static registry** (`stance::is_stance_signal`); a stance axis may *not* be backed by a `stance.*` signal, which would define its accreted value in terms of itself |

**Noun resolvers** (`nouns.rs`) — each returns `Option<Noun>` (`Named { name, plural, adjective }`
or `Scalar`). `None` is normal early-game. Ties on every scan break by **species name ascending**,
so the result is order-independent.

| Resolver | Behaviour |
|---|---|
| `band.count` | `Scalar` |
| `biome.current_dominant` | the primary band's `current_tile` → `Tile::resource_terrain()` → `TerrainType::as_adjective()` |
| `site.last_discovered` | the faction's most recent `DiscoveredSites` record, named from the sites catalog |
| `fauna.most_hunted` | species with the most workers assigned as a `LaborTarget::Hunt` target |
| `fauna.most_domesticated` | highest `domestication_progress` among herds the faction owns |
| `fauna.most_collapsed` | species of the largest `Collapsing` herd |
| `thread.<kind>.oldest` / `.recent` | the remembered noun with the earliest / latest `first_seen_tick` of that thread kind, ties by `key` ascending. **Registered generically over the kinds the catalog's `remembers` entries declare** — a modder adding a kind needs no engine change. Using one refreshes that thread's eviction clock. See "Memory threads" |

**Fauna word forms are data, not a heuristic.** `SpeciesDef.plural` / `.adjective`
(`fauna_config.json`, both `Option`, defaulting to `display_name`) supply the forms copy needs.
Deliberately **no English pluralisation** — many of these names are already collective ("aurochs",
"deer", "fowl") and a naive `+s` gives "deers". `TerrainType::as_adjective()` (`sim_schema`) is the
terrain equivalent, written out rather than derived from the enum's debug name.

### The `when` grammar (`predicate.rs`)

Combinators `all` / `any` / `not`; leaves dispatch on which keys are present, with a clear error on
an unrecognised shape (content authors will hit this).

| Form | Meaning |
|---|---|
| `{ "signal": S, "gt"\|"gte"\|"lt"\|"lte"\|"eq": x }` | comparison on this turn's sample. `eq` compares within a named epsilon, never `==` |
| `{ "signal": S, "crosses": "rising"\|"falling", "threshold": x }` | **the edge** |
| `{ "signal": S, "trend": "rising"\|"falling", "over_turns": n }` | the sample `n` turns ago vs now, beyond `trend.min_delta` |
| `{ "flag": F }` | a consequence flag (nothing writes one in PR-A) |
| `{ "fired": B, "within_turns": n }` | callback to a prior beat |
| `{ "thread": K, "min_count": n, "older_than_turns": t }` | at least `n` memory threads of kind `K`, each first seen ≥ `t` turns ago. Both bounds default (`1` / `0`), but a *present but malformed* bound still fails at load |
| `{ "answered": B, "choice": C, "min_turns_since": n }` | the player answered fork `B` with choice `C`, at least `n` turns ago (`n` defaults to 0). **The elapsed-time half is usually load-bearing** — see "The `answered` gate" |

> **`Crosses` is the correctness heart of the engine.** It is true **only on the turn the value
> crosses**, computed from the previous turn's stored sample (rising = `prev < threshold <= now`),
> and it **re-arms only after the value falls back**. That is what makes a beat fire once per
> crossing instead of every turn the condition holds. Two consequences that are behaviour, not bugs:
> a signal's **first-ever sample is never a crossing** (there is no `prev` — `opening.cold_open`
> uses `eq`, which is why it still fires on turn 0), and a signal that is **already above its
> threshold the first time it is sampled never fires that beat** until it falls back below.
>
> The bookkeeping order in `telling_tick` is load-bearing: sample → append history → **evaluate
> against the ledger's still-previous `edge_state`** → emit → *then* commit this turn's samples as
> next turn's `prev`. Committing before evaluation would make `Crosses` structurally unfireable.

### Selection, budget, emission

Per turn: sample → candidate filter (in **catalog order**) → resolve nouns → weigh wardrobe →
seeded draw → render → emit.

`weight = fit × novelty × stance_affinity`:
- **fit** — `0` (excluded) if a `requires_noun` slot is unresolved, or the entry is biome-gated and
  the band's ground carries **none** of its tags (an any-of hard gate); else
  `1.0 + fit_soft_tag_weight × matched_biome_tags`. The biome-tag vocabulary is narrative and lives
  in `nouns.rs` (one biome reads as several words a writer would reach for), and an unknown tag is
  a **load-time** failure.
- **novelty** — `1.0` if never used; else ramping linearly from `novelty_floor` back to `1.0` over
  `novelty_window_turns` since last use.
- **stance_affinity** — the **re-colouring** term (`stance::affinity_term`):
  `1.0 + stance_affinity_weight × Σ_axis (affinity[axis] × effective_stance[axis])`, floored at
  `selection.stance_affinity_floor` (0.1). The floor is deliberate: a wrong-stance dressing should
  become *unlikely*, not impossible — the wardrobe pool is small and hard exclusion risks a beat
  with nothing left to dress it in. An entry with **no** `stance_affinity` gets exactly `1.0`, so
  uncoloured beats are bit-identical to PR-A.

Entries below `min_selection_weight` are dropped. **If every entry is excluded the beat emits
nothing and is NOT marked fired** — it can still land later, once the world can dress it.

> **Determinism — the recipe.** Selection RNG is seeded **per decision** as
> `world_seed ^ tick ^ TELLING_SEED_SALT ^ FnvHasher(beat.id)` (the `fauna.rs` per-herd recipe),
> **never a rolling stream**. So a roll is reproducible *and* independent of evaluation order:
> **adding a beat to the catalog cannot perturb another beat's roll.** Pinned by
> `select::tests::selection_is_stable_when_an_unrelated_beat_is_added_to_the_catalog`.

**Budget.** Per-tier `max_per_turn` plus a per-tier `global_cooldown_turns`, on top of each beat's
own `cooldown_turns` / `once`. Per-turn budget counters are **scratch** — recomputed each turn,
never persisted, so a rehydrated ledger starts neutral (the `fauna.rs` convention).

**Command feed.** `CommandEventKind::NarrativeBeat` (`"narrative_beat"`) for the ambient/beat tiers,
and `CommandEventKind::NarrativeFork` (`"narrative_fork"`) for a resolved fork's echo line. The wire field is
already a string, so there is **no schema change and no client change** — an unknown kind renders
generically. `label` = the rendered line; `detail` = the **gloss**: each `gloss` signal id and its
**real sampled value**, plus `tier=…`. That is the concept doc's *"the voice never lies"* proof, so
it must show the actual numbers. Analytics mirror:
`info!(target: "shadow_scale::analytics", event = "telling_beat", beat, wardrobe, tier)`.

> `CommandEventLog`'s bound is **32 entries, drop-oldest**, unchanged in PR-A. Measured on a
> 40-turn probe the engine emits ~8 beats, so ambient narration is **not** the thing that will
> overrun the feed — but it now shares that budget with every command echo, and if the feed starts
> churning, this bound is the first lever to look at.

### Validation — content typos fail at LOAD, not at render

`validate()` runs inside `from_json_str` for **both** files, so every load path (builtin, default
file, env override) is covered; a break is logged at **error** level and the known-good builtin is
used. Rendering is therefore infallible by construction.

Catalog invariants: beat ids unique; wardrobe entry ids unique within a beat; ≥1 wardrobe entry per
beat; the config's `default_register` present in every `voice`; **every `{slot}` / `{slot.field}`
placeholder resolves to a declared noun slot with a legal field** (the single most valuable check
here); every `fit.requires_noun` names a declared slot; every `fit.biome` tag is a known tag; every
signal in `when` and `gloss` is registered; every `from`/`fallback` names a registered resolver.

Config invariants: `novelty_window_turns > 0`; `0 ≤ novelty_floor ≤ 1`; every weight and
`trend.min_delta` finite and `≥ 0`; `trend.max_history_turns > 0`; `registers` non-empty and
`default_register` among them; every `stance.axes[].signal` registered with `range[0] < range[1]`.

`BeatTier` round-trips by string key (`as_str`/`from_key`) with an **unknown key erroring at load**,
per the persisted-enum convention.

### Stance — accreted signal + declared offset

The concept's flow is *accrete → name → ratify or resist → steer*, so an axis has **two** halves and
the engine keeps them apart:

```text
effective_stance(axis) = clamp(normalize(signal) + declared_offset(axis), -1.0, 1.0)
```

- the **accreted value** — what the player's behaviour actually says, read from the axis's backing
  signal (`beat_config.json` `stance.axes[].signal`) and normalized from that axis's `range` to
  `[-1, 1]`;
- the **declared offset** — what the player *said* when they answered a fork, accumulated from each
  choice's `writes.stance` (each write clamped so an offset **alone** can never leave the axis).

> **`BeatLedger.stance` stores ONLY the declared offsets** — the part that is not derivable — and
> the effective value is recomputed every turn in `telling_tick`. **This is what makes *resist*
> representable.** A player whose sedentarization is climbing but who answered "we are the trail"
> carries a negative offset pulling against their own behaviour, and that tension survives; it
> would not if stance were a single stored number, or a bare signal reading. Do not "simplify" the
> two halves into one field.

Each axis is also a readable `stance.<axis>` signal (injected into the sample immediately after the
base signals, *before* any predicate or gloss evaluates), so content can gloss it — the shipped fork
glosses `stance.roam_settle` — and future beats can gate on it. The effective values are also kept
on the ledger as **derived scratch** (`last_effective_stance`, not persisted, excluded from
equality — the `LaborAllocation::last_yields` convention) purely so the snapshot can export them
without re-sampling.

### The fork tier — the decision surface

A `fork`-tier beat carries a **`choices` array**. It does *not* push a line to the feed; it posts a
`PendingFork` onto `BeatLedger.pending` for the faction, which the client renders and answers.

**Posting.** Same candidate filter, noun resolution, weighing and seeded selection as any other
beat, and it spends the fork tier's `max_per_turn` / `global_cooldown_turns` budget identically.
Then:
- **Every configured register is rendered at post time** — narration, each choice's `label`, and
  each choice's `echo`. The register is a **live user toggle**, so storing one rendered string would
  freeze the fork in whichever voice happened to be active when it fired. Rendering all of them also
  pins noun resolution to the moment the fork fired, which is correct: the herd you were chasing
  *then* is what the question is about (and it is why the `echo` is read off the pending fork at
  answer time, not off the catalog).
- The dressing is marked **used** (novelty), because the player is looking at it.
- **The beat is deliberately NOT marked fired.** A fork is fired when **answered** — so one that
  expires unanswered can legitimately re-post. A fork already on the table is not re-asked each turn.

**Answering** — `answer_fork <faction> <beat_id> <choice_id>` (full proto/runtime/text/server
plumbing; `AnswerForkCommand` proto field **40**; `handle_answer_fork`). On a valid answer
(`BeatLedger::answer_fork`, the single path both the command and the expiry valve take):
1. apply `writes.stance` into the ledger's declared offsets (clamped to `[-1, 1]`);
2. apply `writes.flags` into `BeatLedger.flags` (the `{ "flag": F }` predicate's first real writer);
3. **mark the beat fired at the current tick** and record the answer in `BeatLedger.answers`
   (beat id → `Answer { choice, tick }`) — the concept's memory ledger in its smallest useful form,
   so later beats can call back to what was decided **and how long ago**. The expiry valve's
   auto-defer resolves through this same path, so it records a tick too;
4. if the choice carries `rearm_after_turns`, record a re-arm tick that lifts the `once` guard after
   that many turns — the defer branch's "it returns, sharper";
5. drop the fork from `pending`;
6. push the choice's **echo** to the feed under `CommandEventKind::NarrativeFork`, so the answer is
   part of the story record and not a silent state change.

Rejections are distinct and legible: unknown beat id, no pending fork of that beat for that faction,
unknown choice id.

**Expiry — the safety valve.** At the top of `telling_tick`, a pending fork older than
`beat_config.budget.fork_expire_turns` (**30** — three fork cooldowns' patience: generous under a
player taking their time, short enough that an unattended list cannot grow) **auto-resolves to its
defer choice**, through exactly the same `answer_fork` path, with the same echo and the same re-arm
(detail `resolved=expired`). Forks post for **every** faction, AI and unattended ones included, so
without this a fork with no client would sit in `pending` forever and accumulate.

> **The server NEVER blocks turn resolution on a pending fork.** The turn gate is **client-side**;
> the expiry valve is the server's whole answer. There is no gating in the turn queue or `run_turn`,
> and none should be added — a fork posted to a faction with no client would deadlock the game.

**Validation** (at catalog load, so a broken fork never ships): a `fork` beat needs **≥2 choices**
and a non-fork beat **none**; choice ids unique within a beat; `label`/`echo` carry the
`default_register` and their placeholders resolve to declared noun slots; every `writes.stance` axis
and any `soul.fork` names a configured stance axis; and **exactly one choice per fork must be a
defer** (an empty `writes`). That last one is load-bearing: it is the explicit out the client's turn
gate depends on, and a fork without it would trap the player in an unanswerable-without-committing
decision.

### The fork tier on the wire

Unlike `BeatLedger` (sim-only serde), the fork tier **is** client-consumed, so it follows the
per-faction `SedentarizationState` / `DiscoveredSitesState` pattern on **both** `WorldSnapshot` and
`WorldDelta` (append-only, in `CampaignSection`):

```
pendingForks:[PendingForksState{ faction:uint, forks:[PendingForkState{
  beatId, wardrobeId, postedTick,
  narration:[VoiceLine{ register:string, text:string }],
  choices:[ForkChoiceState{ choiceId, label:[VoiceLine], isDefer:bool }],
  gloss:[GlossEntry{ signal:string, value:double }] }] }]
stanceAxes:[StanceState{ faction:uint, axes:[StanceAxisState{ axis:string, value:float }] }]
```

`register` and `axis` are **strings** (the `species` / `policy` convention), so adding a voice
register or a stance axis needs no schema change. **`isDefer` is computed server-side** (the choice
writes nothing) rather than re-derived client-side — the client must not have to know what makes a
choice a defer, and its turn gate depends on the answer. `stanceAxes` carries the **effective**
value, so the client shows what the player's identity currently reads as, drift and declaration
together.

### Memory threads — what the story remembers

The concept's memory ledger: *a small set of durable threads (a rival tribe, a sacred mountain, a
valley you refused) so later beats can call back. **Callbacks are what make a 200-turn emergent game
feel authored.*** Lives in `telling/memory.rs`; held on `BeatLedger.threads` as
`BTreeMap<kind, Vec<Thread>>`.

A `Thread` is `{ kind, key, name, plural, adjective, first_seen_tick, last_referenced_tick }`. The
`kind` is **free-form and content-defined** — the engine has no thread enum.

> **A thread snapshots its noun at first sight and is NEVER re-resolved.** The whole point is that
> the story remembers a thing that may no longer exist — the herd went extinct, the site is four
> hundred turns behind you. Re-resolving would make a callback silently vanish exactly when it would
> land hardest. Pinned by
> `memory::tests::a_threads_noun_is_snapshotted_at_first_sight_and_never_re_resolved` and
> `telling_memory::a_thread_survives_its_source_disappearing`.

- **Writing** — a beat's `remembers: [{ "slot": S, "kind": K }]` promotes slot `S`'s resolved noun
  into a thread of kind `K` when the beat **lands**. "Lands" means *emits*, or — for a fork —
  *posts*: a fork's nouns are pinned at post time for exactly the same reason a thread's are, so the
  two agree by construction. It is an **upsert keyed by the noun's `name`**, never a push:
  rediscovering the same site updates `last_referenced_tick` and leaves the snapshot and
  `first_seen_tick` alone. A `Scalar` noun has no identity to remember and is silently not a thread.
- **Reading** — the `thread.<kind>.<selector>` noun resolvers (`oldest` / `recent`, ties by `key`
  ascending). An empty kind resolves to `None`, which the existing machinery already handles: a
  wardrobe entry requiring the slot is excluded, and a `fallback` chain moves on (the shipped
  `identity.trail_endures` uses a thread as a *fallback* behind `fauna.most_hunted`). Registration is
  **generic over the kinds the catalog declares**, not a hardcoded list.
- **Eviction is by least recently REFERENCED, not oldest first-seen.** A thread the story keeps
  returning to is the one worth keeping; the one nothing has called back to in two hundred turns is
  the one to drop. `last_referenced_tick` is refreshed when a resolver *wins a slot* on a beat that
  lands — a thread merely sitting in a fallback chain that the primary resolver beat does not count.
  Ties by `key` ascending, for determinism. Cap: `memory.max_threads_per_kind` (**8**).
- **Validated at load** — `remembers.slot` must be a declared noun slot and its `kind` non-empty; a
  `thread.*` resolver and a `{ "thread": K }` gate must both name a kind some beat's `remembers`
  actually declares (otherwise they could never resolve or be satisfied).

### The `answered` gate — one sim, two stories

`{ "answered": B, "choice": C, "min_turns_since": n }` is true when the player answered fork `B` with
choice `C` at least `n` turns ago. PR-B stored `answers` and nothing read it; this is what makes it
load-bearing. The fork promised *"the stories that will find you now"*, and `identity.trail_endures` /
`identity.walls_promised` are that promise being kept: **a player who answered `yes_trail` gets a beat
20 turns later that the `no_root` player never sees, out of the same simulation.** Pinned by
`telling_memory::one_sim_two_stories_the_answer_decides_which_beat_finds_you`.

> **`min_turns_since` is the elapsed-time half, and it is not decoration.** A callback almost always
> means *"some time after you said that"* — `identity.trail_endures` says *"we have kept our word to
> it"*, which is absurd the turn after the word is given. **Do NOT reach for a `turn.index` trend to
> express this.** `turn.index` rises unconditionally, so `{"signal":"turn.index","trend":"rising",
> "over_turns":20}` does not mean "20 turns after the answer" — it only ever means "we are past turn
> ~20", and combined with an `answered` gate the beat fires the turn *after* the player answers. Both
> identity beats were authored that way and are now `min_turns_since: 20` with the trend clause gone.
> This is why `BeatLedger.answers` stores an [`Answer { choice, tick }`](#ledger--rollback) rather
> than a bare choice id.

> **A gate on the answer is not enough — the voice must not claim what the sim contradicts.** The
> identity beats gate on the **declared stance still holding**, not merely on the answer:
> `trail_endures` needs `stance.roam_settle < 0`, `walls_promised` needs `> 0`, and
> **`identity.trail_forsaken`** (`>= 0`) is the honest beat for a player who declared the trail and
> then settled anyway — *"No one lied and no one decided — we simply stopped, one season at a time,
> and kept the name."* Without it, elapsed time alone would have the narrator tell a settled people
> *"we have kept our word to it"* — **concept pillar 2, the one thing this layer must never do.**
> `trail_endures` (`< 0`) and `trail_forsaken` (`>= 0`) are exactly complementary, so a player who
> declared the trail always gets **precisely one** of them. Pinned by
> `telling_memory::{keeping,breaking}_the_word_brings_*`, which drive the two regimes explicitly
> (stop driving settlement → `roam_settle` ≈ −0.40; keep driving → ≈ +0.20).

> **The target is validated hard**: the referenced beat must exist, be `fork` tier, and declare that
> choice id. A typo there silently produces a beat that can *never fire* — nothing errors, nothing
> logs, the beat is simply never seen — which is the worst failure mode a content system has. The
> same reasoning added a **`trend` window check**: a `trend` predicate whose `over_turns` is `>=`
> `trend.max_history_turns` can only ever read false, so it is now a load failure too. (It first fired
> on the mis-authored identity beats above; with those fixed the widest authored window is 5, so
> `trend.max_history_turns` stays at its original **16**.)

### The maturing voice — medium

Concept §7: the voice changes **medium** as the civilization crosses milestones — oral saga → painted
chronicle → written record — and *"the narrator maturing is itself a narrative arc that makes
progression felt."* Lives in `telling/medium.rs`.

> **Medium is PRESENTATIONAL and does NOT multiply wardrobe copy.** It changes how the telling
> *looks* to the client and fires a beat when it advances; that is the whole of it. It deliberately
> does **not** select different copy — 4 mediums × 2 registers per wardrobe entry is an **8×**
> authoring cost for the layer's thinnest payoff, and the concept doc names authoring cost as a real
> risk. **Do not "complete" this later by authoring per-medium strings.**

- **Config-driven, reusing the predicate evaluator.** `voice.mediums` is an ordered
  least→most-advanced list of `{ id, when }`; the first entry is the default and needs no `when`,
  and the **highest satisfied** rung wins. Evaluated once per turn in `telling_tick`, right after
  the stance axes and *before* any beat evaluates.
- **Shipped ladder** (`beat_config.json`): `oral` (default) → `painted`
  (`sedentarization.score >= 40` — the soft-drift moment, when a people has been in one place long
  enough to have walls to paint) → `written` (`sedentarization.score >= 70` **and**
  `discovery.progress.cultivation >= 1.0` — a settled people that has learned to farm is the one that
  needs to count a harvest). **The concept's fourth rung, `archive`, is deliberately NOT shipped**:
  the only plausible source for an institutional-archive gate is the `CapabilityFlags` bits, which
  are not registered signals, and adding a signal whose sole purpose is to make a medium reachable
  would be inventing state to satisfy content. Three reachable mediums beat four with a dead rung.
- **It never regresses.** If a signal falls back below its threshold the medium does **not** step
  down — a people that learned to write does not forget — so the attained rung is persisted per
  faction on the ledger and each turn's evaluation takes the max.
- **Readable as `voice.medium_index`**, which is how the authored `voice.medium_painted` /
  `voice.medium_written` beats gate themselves with `crosses`.
- **On the wire** (append-only, `CampaignSection`, both `WorldSnapshot` and `WorldDelta`):
  `voiceMedium:[VoiceMediumState{ faction:uint, mediumId:string, mediumIndex:uint }]`. `mediumId` is
  a **string** (the `species` / `policy` / `register` convention) so adding a rung needs no schema
  change. Threads are deliberately **not** on the client stream — nothing renders them; they stay in
  the ledger's sim-side serde state.
- **Validated at load**: at least one medium, no `when` on the default rung, unique ids, every gate
  on a registered signal; and `memory.max_threads_per_kind > 0` (a zero cap would silently discard
  every thread, making every `thread` gate and `thread.*` resolver permanently unsatisfiable).

### Ledger + rollback

`BeatLedger` (resource) holds `fired` (beat → ticks), `edge_state` (signal → last turn's sample),
`history` (signal → rolling, capped at `trend.max_history_turns`), `wardrobe_usage` (novelty),
`flags`, `stance` (the **declared offsets only**), `pending` (posted, unanswered forks — every
register already rendered), `answers` (beat → **`Answer { choice, tick }`** — the choice *and the
turn it was made on*, which is what `answered`'s `min_turns_since` reads), `rearm_at` (beat → the tick
its `once` guard lifts), `threads` (kind → the durable memory threads) and `mediums` (faction → the
attained voice medium — persisted because it is monotone). `BTreeMap`/`BTreeSet` throughout for deterministic iteration
and stable snapshot ordering, and **`Scalar`, not `f32`**, for everything persisted — sampled as
`f64`, stored fixed-point, the same rollback-bit-exactness argument the rest of the codebase makes.

It round-trips through the rollback snapshot as `WorldSnapshot.beat_ledger`
(`sim_schema::BeatLedgerState`) on the `HerdRegistry` pattern: sim-side serde only, **not** on the
FlatBuffers client stream (beats reach the client as `CommandEvent`s).

> **Capture AND restore.** `restore_world_from_snapshot` rebuilds the resource via
> `BeatLedger::from_state`. This is deliberately **not** the `SedentarizationScore` shape, which is
> captured but never restored (`capture.rs` has no rebuild for it — a latent bug; do not copy it).
> A ledger that was only captured would leave a beat marked fired after a rollback past it, so it
> could never fire again, and its stale `edge_state` would make `crosses` misfire. The fork tier
> rides the same round-trip: a fork **answered after the rollback point is un-answered by the
> rollback** — the declared stance offset, the fired mark, the recorded answer and the pending list
> all rewind together, so a player never carries a stance they did not declare in the timeline they
> rolled back into. **PR-C rides it too**: a thread written after the rollback point is
> un-remembered, and a narrator that advanced its medium after the rollback point does not carry that
> medium into a timeline where it never learned it. Guarded by
> `integration_tests/tests/telling_rollback.rs` (all three halves).

See Also: `docs/plan_the_telling.md` (design + PR slicing), `docs/Emergent Narrative.md` (concept),
"Sedentarization" (the rising-edge pattern `crosses` generalises).

Test suites: `core_sim/tests/telling.rs` (ambient/beat), `telling_fork.rs` (the fork tier),
**`telling_memory.rs`** (threads, callbacks, the `answered` payoff, and the medium),
`integration_tests/tests/telling_rollback.rs` (the round-trip).

---

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
`restore_world_from_snapshot` rebuilds it from the snapshot (like the fog reset) so a rollback
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

**Hunt verb (PR 2)** — `ExpeditionMission::Hunt { fauna_id, policy: FollowPolicy }` on the same party;
the take **policy is chosen at launch** (`send_hunt_expedition <faction> <band> <party_workers>
<fauna_id> [policy]`, default **Sustain** — not a config lever). `advance_expeditions` branches on
mission:
- **Hunting**: retarget `BandTravel` to the herd's live tile each turn (from `HerdRegistry`). The
  take **and the trip-completion decision both live inside the `hunt.reach_tiles` guard** — a party
  still walking to its herd never concludes the trip. Once in reach, take a **productive** hunt's
  worth of biomass — `workers × per_worker_biomass_capacity`, capped per policy (below) — from the
  herd and convert to provisions (`fauna::hunt_provisions`) up to the carry cap (`party ×
  hunt.per_worker_carry`). Deliver only with a worthwhile load: a full pack **or** `herd_near_band &&
  carried ≥ hunt.min_deliver_fraction × cap` (the empty-larder flip-flop fix). An empty pack at
  completion reports **why** (no sustainable take / no take possible), never a cheerful zero.
> #### A hunting expedition is a GREEDY RAID, not a resident band's throttled skim (playtest fix)
>
> A resident band (`systems::hunt_take`) takes its policy's per-turn **rate** into the kill-credit
> bank — worker-independent, so a second hunter only added pack to fill and made the *trip* longer
> (the playtest bug). A detached party instead **grabs the herd's standing surplus above the policy's
> floor in a burst and comes home**, so more hunters take more animals in **fewer-or-equal** turns.
> This replaces the MSY-rate ceiling on the **expedition path only** (`expedition_take_biomass` /
> `hunt_trip_forecast`); `hunt_take` and `hunt_policy_rate` are untouched.
>
> - **The floor is per-policy** (`hunt_expedition_floor`, `FaunaConfig::validate`-ordered
>   `collapse_fraction < surplus_escapement_fraction < MSY_BIOMASS_FRACTION`): Sustain `K/2` (0.50·K),
>   Surplus `hunt.surplus_escapement_fraction·K` (0.30), Market `ecology.collapse_fraction·K` (0.15),
>   Eradicate `0`. A deeper policy leaves a leaner herd — *"Surplus/Market raid deeper"*. (Expedition
>   Market no longer drives extinction — it strips to 0.15·K and stops; extinction is the *resident*
>   band's multiples-of-MSY axis, unchanged.)
> - **The take brings home a PARTIAL when it must, and wastes the rest — reconciled with the band.**
>   The party's processing throughput (`workers × per_worker_biomass_capacity`) is banked onto the herd's
>   `hunt_credit`, and the bank meters *when* the next whole animal is **ready** (a body heavier than one
>   turn's work takes `body / throughput` turns). Once one is banked (`affordable >= 1`) the party
>   **kills it even if the pack cannot seat it whole**, carries the pack's worth, and **wastes the
>   remainder** — exactly the resident band's `max(1, carryable)` rule (`fauna::quantise_animal_take`): a
>   1-hunter party on an 800-biomass mammoth (16 food) whose pack holds only `per_worker_carry` = 4 food
>   (200 biomass) kills it, delivers ~200 (≈ 25%), wastes ~600. So **`animals_taken` is now a KILL
>   count**, and the delivered payload is `delivered_food` (`Σ hunt_provisions(carried)`), not
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

- **Per-policy behaviour**: all four grab the standing surplus down to the floor above.
  **Sustain/Surplus** — one raid: deliver on a full pack, a worthwhile near-band delivery, **or the
  surplus spent**, then fold home. **Market** — repeated FULL-cap trips (`Delivering`→deposit→
  **auto-relaunch**) *while the herd still has surplus*; once stripped to `0.15·K` (surplus spent) it
  comes home for good rather than trickle-churning at the floor. **Eradicate** — no floor, **delivers
  no food** (denial): grinds the herd to extinction (→ lost-herd `Returning`).
- **The completion fix** (`ExpeditionPhase::Hunting`, load-bearing): `done = pack full OR standing
  surplus spent (herd within one body of the floor) OR herd lost`. Without the surplus-spent branch a
  raid that grabs its surplus and hits the floor would **hang, taking 0 every turn**.
- **Launch forecast — a bounded forward SIMULATION of the raid** (`hunt_trip_forecast`,
  `systems::expeditions`). It runs the raid forward turn by turn — `fauna::regrow_biomass` (Logistics)
  then `expedition_take_biomass` (Population), accumulating the larder on the **fixed-point `Scalar`
  grid** — until the raid completes (fill OR surplus spent OR herd lost) or `hunt.forecast_horizon_turns`
  (**60**). No second copy of the model, and the completion test mirrors the arm's `done`. It returns:
  - **`turns_to_fill`** — turns until the raid **completes** (*"turns until the party comes home"*, NOT
    *"turns until the pack is full"*: a big party on a full herd leaves a partial pack once it strips
    the surplus, a successful short trip). `None` = never completed within the horizon.
  - **`animals_taken`** — whole animals the raid **kills** (carried whole or partially wasted). `0` = the
    herd is at/below the policy's floor with **no surplus to raid** (the honest non-viable case).
  - **`delivered_food`** — food actually landed in the larder (`Σ hunt_provisions(carried)`), the primary
    readout. **`wasted_food`** — food killed but not hauled (`Σ hunt_provisions(wasted)`); the waste
    fraction is `wasted_food / (delivered_food + wasted_food)`. A small party on a big animal now
    delivers a partial with waste, so **"too lean to raid" is `delivered_food == 0`**, not "party too
    small to seat a whole animal".
  - **Travel is not counted**; the herd is assumed stationary and in reach. **Eradicate** delivers no
    food (`delivers_food == false`) → no ETA.
  - *(The old O(1) "cannot fill" short-circuit + its `hunt_trip_bound_tests` sweep were **retired** with
    the raid: their premise "won't fill the pack ⇒ doomed trip" is inverted by a raid, where "won't fill
    the pack" is the normal successful short trip. A raid is inherently short — grab the surplus, done —
    so simulating each to completion is already cheap. `surplus_escapement_fraction` replaced the retired
    `hunt_expedition_ceiling`/kill-credit expedition ceiling and the bound's constants.)*
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
  turn the party leaves `Hunting` — across Sustain/Surplus/Market × full/depleted herds. The forecast
  is pinned to the sim, never the reverse. The greedy-raid properties (more hunters → fewer turns,
  Sustain leaves K/2, deeper policies raid deeper, surplus caps the take, no-surplus reads 0) are pinned
  by the sibling tests in that file.
- **Lives off its kills** — no launch provisions, no per-turn upkeep (upkeep is scout-only).
- **The investment policies are NOT an expedition concept.** `Cultivate`/`Corral` are place-bound work
  a *resident* band does (prepare a patch, build a pen, then tend it) — a detached party cannot pen a
  herd and walk home. `handle_send_hunt_expedition` **rejects** them at launch (alongside an
  unparseable token), so the expedition's whole axis is `FollowPolicy::EXTRACTIVE` (the four extractive
  rungs). `systems::hunt_expedition_floor`'s investment arm is therefore **unreachable**, and yields
  **`f32::INFINITY` (⇒ zero surplus ⇒ the party takes *nothing*) + a `debug_assert!`** rather than
  quietly falling back to a real floor: if that validation ever regresses the party takes *nothing* and
  the hole is loud, instead of a plausible-looking trip hiding it. (An unreachable arm must fail loudly,
  never quietly do something plausible.) Guarded by
  `server::tests::send_hunt_expedition_rejects_the_investment_policies`.
- **Shared take helpers** (`fauna.rs`, slice 8b): **`hunt_policy_rate(policy, biomass_before_regrowth,
  cap, ecology, fauna, ladder)`** is THE per-turn take **rate** (Sustain `sustainable_yield`, Surplus/
  Market `mult × MSY`, Eradicate the whole stock, Tame/Corral the dip × Sustain's rate, Cultivate/Sow
  `0`), and **`hunt_credit_ceiling(policy, biomass, credit, rate)`** turns it into this turn's affordable
  whole-animal take against the herd's banked `hunt_credit` — see "The hunt policy axis" for the model.
  **`hunt_provisions(take, fauna, output_multiplier)`** is the single biomass→provisions conversion (an
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
  modifier). **The expedition take is FOOD-ONLY — a known, tracked gap.** The band's Hunt arm credits
  food **+ trade goods** (Market) **+ husbandry/domestication accrual** (Sustain on a Thriving herd)
  from the same take; the expedition credits food and nothing else, so a Sustain *expedition* builds no
  domestication and a Market *expedition* yields no trade goods. Whether a detached party *should* earn
  those side-effects — and what Market's goods and Eradicate's denial are ultimately *for* — is the
  **"Hunt policy payoffs"** arc in `TASKS.md` (design: `docs/plan_exploration_and_sites.md` §2b).
  Catching a *migratory* herd depends on the deferred fauna-movement redesign (herds step 1 tile/turn
  today, so an equal-speed party can't close a long one-directional route).

**Commands** (full proto/runtime/text/server plumbing, mirroring `move_band`):
- `send_expedition <faction> <band> <party_workers> <x> <y>` — validates land target + `1 ≤
  party_workers ≤ min(available_workers, max_party_size)`, draws `party × distance ×
  provision_draw_per_worker_per_tile` provisions from the band larder (partial OK), removes the
  workers from `band.working`, and spawns the detached `Expedition` cohort. Feed `ExpeditionSent`.
- `send_hunt_expedition <faction> <band> <party_workers> <fauna_id>` — same resident-band gate +
  party validation, validates `fauna_id` resolves to a live herd, draws **no** provisions, removes
  the workers, spawns a `Hunt`-mission party in `Hunting` phase heading for the herd. Feed
  `ExpeditionSent` (hunt flavor).
- `recall_expedition <faction> <expedition_entity_bits>` — resolves the entity via
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
`expeditionHuntPolicy` (`sustain|surplus|market|eradicate`) / `expeditionCarryCap` (hunt carry cap =
`party × per_worker_carry`, `0` otherwise) and persistence-only `homeBandEntity` /
`expeditionAnnounced` / `pendingRevealX` / `pendingRevealY`
(`snapshot.fbs`, `sim_schema`). Capture fills them from `Option<&Expedition>`;
`restore_world_from_snapshot` re-attaches `Expedition` for a rolled-back in-flight party (resolving
`home_band` from `homeBandEntity` via the cohort entity-remap; missing home band → log + skip) and
re-attaches `ResidentBand` to every non-expedition cohort so the `With<ResidentBand>` systems keep
running after a rollback. `PopulationCohortState` also echoes `maxExpeditionPartySize` per cohort
(from `expedition_config.max_party_size`, same idiom as `workRange` — a global lever surfaced
per-band, populated for every cohort) so the client outfit stepper pre-clamps to
`min(idle_workers, max_expedition_party_size)`.

**Pre-launch export — the client does ZERO arithmetic.** The launch forecast above only rides the
*post-commit* `ExpeditionSent` feed line; the outfit UI needs the trip's economics **before** the
player commits workers, as they pick party size / herd / policy. The expedition's trip length is **not
a formula** (see the forecast above: a small-herd Surplus party exhausts *stock*, so no per-turn rate
describes the trip), so the sim exports the **answer** it simulated, and the client's job is a **table
lookup**:
- `HerdTelemetryState.huntTripEstimates:[HuntTripEstimate{ policy:string, partyWorkers:uint,
  turnsToFill:uint, deliversFood:bool, animalsTaken:uint, deliveredFood:float, wastedFood:float }]` —
  per **huntable** herd, one entry per `FollowPolicy::EXTRACTIVE` × every legal party size
  (`1..=expedition_config.max_party_size`, so 4 × 8 = 32 rows/herd; `policy` is a free-form string like
  `species`, so a new policy needs no schema change). **The four extractive rungs ONLY** — the investment
  policies are launch-rejected (above), so a `Cultivate`/`Corral` row would be a number for a trip that
  cannot be launched. **`turnsToFill`** is turns until the raid **completes** (comes home — pack full OR
  surplus spent), **`0` = never completed** within `hunt.forecast_horizon_turns`. **`animalsTaken`**
  (append-only) is now a **KILL count** — a party too small to seat a whole animal kills one and wastes
  the rest (like the resident band), so the delivered payload is **`deliveredFood`** (`Σ
  hunt_provisions(carried)`, appended strictly after `animalsTaken`), NOT `animalsTaken × foodPerAnimal`.
  **`wastedFood`** (`Σ hunt_provisions(wasted)`, appended) gives the waste fraction `wastedFood /
  (deliveredFood + wastedFood)`. **"Too lean to raid" is `deliveredFood == 0`** (no surplus at any party
  size); a herd at/below its floor reads `0` on all three. Because the take is bounded by the standing
  surplus, `deliveredFood`/`animalsTaken` **plateau** with `partyWorkers` once the surplus binds — that
  plateau is the max-useful party size (`ceil(surplus_food / per_worker_carry)`) the stepper caps at.
  `deliversFood == false` (Eradicate) → "no food delivered (denial)", never an ETA. **Travel is
  excluded** — the number means "turns spent hunting once you arrive".
- `HerdTelemetryState.huntPolicyCeilings:[HuntPolicyCeiling{ policy:string, provisionsPerTurn:float }]`
  — the **BAND / local-hunt** ceiling only, one row per `FollowPolicy::HUNT_POLICIES`: the four
  extractive rungs **plus `Corral`** (a legitimate *band* Hunt policy — its deliberately dipped yield
  is exactly what the player must see before committing to a 25-turn pen). `Cultivate` is Forage-only,
  so a herd has **no** cultivate row. Each is the worker-independent ceiling for the herd's current
  state, in provisions/turn, **clamped to the herd's remaining biomass** — a tautology now (every floor
  is `≥ 0`, so `B − floor ≤ B`), kept as belt-and-braces against a hot-reloaded floor above `1`. A herd
  below a policy's floor exports `0` for it (a herd at the brink spares nothing to Sustain *or* Surplus).
  **Sourced by projecting the herd's `fauna::hunt_forecast`** (`SourceYieldForecast::ceiling_for`) —
  the *same* object the scalar `ceilingSustain`/…/`ceilingCorral` fields export, so the list and the
  scalars are literally the same numbers and cannot drift, and the take path pays exactly them
  (forecast == actual). That also makes `Corral` **phase-correct for free**: the
  the `animal:pen` rung's `yield_fraction_while_building × MSY` dip while the pen is being built, and the **full corral yield**
  once `is_corralled()` (a penned herd forecasts as `SourceYieldForecast::tended` — every ceiling is
  its managed yield, one keeper suffices). There is **no expedition ceiling field** — the retired
  `expeditionProvisionsPerTurn` was exactly the "one number that means a flow for Sustain and a stock
  for Surplus/Market" design smell the estimate table replaces.
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

`core_sim/tests/expedition_hunt.rs` pins **both — each to the sim's REAL behaviour, never to another
preview** (the lesson of the ~34-vs-~6-turn Surplus bug: the old guard compared the client against
`hunt_trip_forecast`, so two copies of the same wrong ceiling agreed with each other while both
disagreed with the take). `exported_hunt_trip_estimates_match_a_real_party_run` asserts every exported
estimate (small-game / big-game / collapsing herd × all four policies × every legal party size) equals
what a **real party run forward through the real systems** actually does — including the
stock-exhaustion case that motivated the rewrite; `exported_snapshot_fields_reproduce_band_hunt_take`
does the same for the band arithmetic against `hunt_take(..)` (healthy / clamp-binding depleted /
collapsing herd × every worker count × all four policies × a unit and a discontent-reduced output
multiplier). If either readout ever drifts from the sim, that test fails.

See Also: `docs/plan_exploration_and_sites.md` §2 (design), "Wondrous Sites" (discovery rides the
flushed tiles), "Visibility Systems" (the `Without<Expedition>` gate).

---

## Campaign Loop & System Activation

### Start Flow
- **Boot idle → `new_game`**: `bin/server.rs` boots **IDLE** — it binds its ports and command
  listener but does **not** run the Startup worldgen, so no world exists and nothing is captured or
  broadcast (Bevy's `Startup` schedule only fires on the first `app.update()`, so simply not calling
  `run_turn` leaves the world ungenerated; `ElevationField` stays uninserted, so the Snapshot stage
  must never run on the empty world — see the `world_active` guard). A world is generated **on
  demand** by `new_game <preset_id> <width> <height> <seed> <profile_id>` (proto field **43**; `seed
  == 0` randomizes, mirroring `map_size`/ResetMap; an unknown `profile_id` is rejected without
  building, an unknown `preset_id` falls through to the worldgen default). `new_game` and `map_size`
  (ResetMap) share one world-build helper (`rebuild_world_from_config`). A `turn` sent **before** a
  world exists is rejected with a warning. See `server-dev`'s boot flow in `bin/server.rs`.
- **Data**: `StartProfile` records with `starting_units`, `starting_knowledge_tags`, `inventory`, `survey_radius`, `fog_mode`
- **Spawn**: Worldgen seeds the profile's `starting_units`, unlocks `ScoutArea`, `FollowHerd`. Each spawned band's head-count comes from its unit's `band_size` (config lever in `start_profiles.json`; falls back to `DEFAULT_STARTING_BAND_SIZE` = 30 in `start_profile.rs`) — no hardcoded size. `late_forager_tribe` ships a **single ~30-person band** (labor-pool scale per `docs/plan_early_game_labor.md`), not the retired four-band/900-person opening.
- **Camps**: Transient settlement-likes with `PortableBuildings`, `CampStorage`, `DecayOnAbandon` (backlog — not yet built)
- **Sedentarization**: implemented — see the dedicated section below.
- **Founding**: `Command::FoundSettlement { q, r }` requires Founders unit, consumes provisions, spawns Settlement

### Population & Demographics (Settlement & Population Economy — Phase 1)
The bedrock number the rest of the economy builds on. Each `PopulationCohort` (a band — the first
"location"; tile-housed population arrives in Phase 3) carries three fixed-point **age brackets** —
**children / working-age / elders** — plus a local **`stores`** larder (food under the `FOOD` key).
`size` is a derived
`u32` cache of the bracket sum. Design: `docs/plan_settlement_population.md`.

`simulate_population` (`systems.rs`, `TurnStage::Population`) delegates each cohort to the pure
`advance_demographics` (config: `demographics_config.json`):
1. **Consume** — draw `per_capita_draw × weighted_mouths` (dependents eat less) from the band's
   own larder; shortfall is the food **deficit**.
2. **Deaths** — starvation scales with the deficit (dependents more vulnerable via `scarcity`
   weights); cold kills across brackets past `cold.temp_tolerance`.
3. **Births → children** — `birth_rate × working × fed_ratio × (1 + surplus_bonus × surplus_ratio)`.
   Births are **morale-independent** (Civilization Wellbeing — see below): contentment doesn't
   change procreation, and morale **never** causes faction population loss. `advance_demographics`
   no longer takes morale; the retired `births.morale_floor` lever is gone.
4. **Maturation** children→working, **aging** working→elders, **elder mortality**. All flows use
   the turn's *opening* values and apply together (a newborn doesn't mature the same turn); the
   total is clamped to `population_cap`. The **dependency ratio** `(children+elders)/working` is
   the core tension.

**Morale attribution (why morale/population falls).** Morale is now computed as the signed sum of a
**named contributor set** (`MoraleContributions` on the cohort — the Layer-1 spine of Civilization
Wellbeing, below): `settling` (`+population_growth_rate`), `terrain` (`−terrain pressure`),
`climate` (`−cold pressure`), `unrest` (crisis impacts + cultural sentiment, signed). Their sum IS
`last_morale_delta`; adding a future factor is a new `MoraleFactor` variant + one field, not a
rewrite of the morale update. The dominant *negative* contributor becomes `last_morale_cause`
(`MoraleCause` ∈ `None | Terrain | Cold | Unrest`) when the delta is negative, else `None`. Drivers:
`Terrain` = terrain attrition + logistics hardness, `Cold` = temperature-difference penalty,
`Unrest` = crisis impacts + cultural sentiment.
Starvation is deliberately **not** a morale cause — it stays on the days-of-food path. The two
place-based (negative) terms come from the shared **`tile_morale_pressure(terrain, temperature,
&MoralePressureConfig)`** helper (`systems.rs`), which returns the tile-intrinsic per-turn morale
drain (terrain + cold, ≥ 0; KarstCavernMouth ≈ 0.0825 at ambient temperature) so the sim and the
snapshot read from one source. The cold term has a **tolerance dead-band**: `max(0, |temp − ambient|
− temperature_morale_tolerance) × temperature_morale_penalty` (config `temperature_morale_tolerance`
= 9.0 in `simulation_config.json`), so temperate mid-latitudes (|Δ| ≤ 9°) bleed **zero** climate
morale and only genuine extremes (poles/high-alt/equator) drain — e.g. at ambient 18° a −5° pole
(|Δ| = 23°) drains `(23−9)·0.004 = 0.056`, a 30° equator (|Δ| = 12°) drains `0.012`. Habitability
reuses this helper, so most of the map rates Hospitable/Fair and only extremes read Harsh/Hostile. These fields are **derived per-turn, not snapshot-persisted** (a
rehydrated cohort reads `0`/`None` until the next turn). Exported as `PopulationCohortState.moraleDelta`
(fixed-point `long`, `FIXED_POINT_SCALE` = 1e6) + `moraleCause:ubyte` (`0=None, 1=Terrain, 2=Cold,
3=Unrest`). `TileState.habitability:long` carries the band-independent `tile_morale_pressure` total
for the tile (same fixed-point scale) so the client can rate a hex's harshness. All three are wired
through `sim_schema`/`snapshot.rs`; the client consumes them for a morale trend arrow + named cause
and a Tile-card Habitability line (client half).

**Food is band-local from day one** (the same store a settlement/storage-pit will hold later at
scale). Provisions **left `FactionInventory` entirely**: labor income (forage + hunt, in
`advance_labor_allocation`) and husbandry (`advance_husbandry`, split across the
owner's bands) income now credit the acting band's local `stores` (food under the `FOOD` key). At Startup
(`seed_cohort_demographics`) each band is seeded with `startup.food_reserve_days` turns of its own
demand (`food_demand`, shared with the consumption path) plus a well-fed morale bonus — no faction
provisions grant to distribute. Bands **share** via the supply network (below); storage-pit
distribution is a later addition. Starvation is deficit-capped (a 10% shortfall kills at most 10%)
so a dry larder bleeds down over several turns rather than in one.

Each band's goods live in a `LocalStore` (`components.rs`) — a commodity-keyed bag (food under the
`FOOD` = `"provisions"` key) held on `PopulationCohort.stores`, so the same store carries any future
good. Brackets + store persist in the snapshot (`PopulationCohortState.stores`) so rollback restores
the exact larder. A per-faction age-structure + dependency-ratio HUD readout ships as
`PopulationDemographicsState` (new `.fbs` table aggregated at capture, wired through
sim_schema/snapshot/native/`Hud.gd` exactly like `SedentarizationState`).

### Supply Network (logistics from turn 0)
Bands are small logistics nodes: `balance_supply_networks` (`supply.rs`, `TurnStage::Logistics`,
before Population consumes) connects **same-faction** bands within `reach_tiles` (via
`grid_utils::wrapped_distance_sq`) into **supply networks** (union-find connected components) and
each turn moves every commodity toward a **population-weighted per-capita balance** across the
network. Transfers are **throughput-limited** (`throughput_per_turn` per node) and lose `friction`
in transit; sub-`min_transfer` moves are dropped. So a gatherer band automatically feeds a scout
band it's near (you can specialize labor), while a band beyond reach lives off its own larder.
Reach decides *who* shares, throughput *how fast*, friction the leak — "free neighbor sharing" is
just the high-throughput/low-friction limit. The per-commodity math is the pure, unit-tested
`balance_commodity`. Config: `supply_network_config.json`.

Each turn the same pass also records **network membership** in the `SupplyNetworkMembership`
resource (`entity → id`, cleared and rebuilt every turn): each connected component with ≥ 2 bands
gets a stable id (`1, 2, …` in the BTreeMap's sorted-root order), singletons get none. The capture
reads it into each cohort's snapshot field `supplyNetworkId:uint` (`0` = not in a multi-band
network, `>= 1` = shared id) so the client can draw supply links between co-networked bands. It is
derived, not snapshot-persisted — a rehydrated cohort reads `0` until the next turn's balance.

The cohort snapshot also carries two derived per-band food-readout fields the client renders:
`daysOfFood:float` — **the honest larder runway: TURNS until the larder is empty, income
included** — and `activity:string` (`idle | forage | hunt | scout | warrior`, the target-kind
with the most workers in the band's `LaborAllocation`). Both are computed at capture in
`population_state`.

> #### `daysOfFood` is `larder / net drain` — ONE formula for a band and an expedition
>
> **`runway = larder / (consumption + penFeedUpkeep − income)`.** An expedition has no labor income
> and keeps no pens, so it reduces to `provisions / consumption` — **exactly** the historical
> reading, unchanged (pinned by `snapshot::population::tests::an_expedition_reports_provisions_over_consumption`).
> A resident band with real income gets the honest number instead of the old `larder / demand`, which
> **assumed the band stops gathering and hunting** and so read badly pessimistic — a header saying "4"
> above a FOOD OUTLOOK chart showing ~9. Do not special-case the two actors.
>
> It is resolved the way that chart resolves it (`snapshot::population::larder_runway_turns`), so
> they cannot disagree by a turn or two on the same panel: (1) walk the larder forward over the
> **merged per-source `arrivals` schedules**, debiting `consumption + penFeedUpkeep` per turn and
> clamping at 0 — the first turn to reach 0 is the answer; (2) it survives the horizon (or **no
> source was projected at all** — an empty schedule is *no data*, never a famine): fall back to the
> smooth `larder / net_drain` on the **steady** income (Σ per-source `realized`, computed locally at
> capture — see the retirement note below), capped at the sentinel; (3)
> `net_drain <= 0` (net-positive): the `999.0` **not-food-limited** sentinel, which the client
> renders as ∞.
>
> **Consumption here is the forward `food_demand`** (what the people will *want* to eat), not
> `last_food_consumption`: `demand` is always resolvable, where the actual debit is `0` on a
> rehydrated save and falls short of demand in a famine. The client's chart drains by
> `foodConsumption` instead, so the two differ **only for a band already eating short** — where the
> sim is the pessimistic (correct) one.
>
> **Consequence, intended:** a band with strong income now reads healthier and **stops tripping
> starvation alerts it should never have tripped** (the map food dot, the turn-orb `starving`
> producer and `_food_is_concerning` all key off this field). Measured on a fed 30-person forage
> band: **4 turns → ∞**. A genuinely starving band is unchanged to within the walk's ±1 clamp. The
> UI thresholds (`band_status_config.json` warn 10 / critical 5) are now measured against a runway
> that is *income-inclusive*, so they fire later by construction — retune there if red arrives too
> late to act on.
>
> **The `days` in the name is a MISNOMER pending a rename** (the sim counts turns; the client already
> renders "turns"). Renaming it across schema/native/client is a mechanical sweep held out of this arc.

Alongside them the snapshot exports `laborAssignments`/`idleWorkers`/`workingAge`,
plus `workRange` (from `labor_config.json` `band_work_range`, global config today, surfaced per-band
for the work-range ring) and `scoutRevealRadius` (**repurposed**: now carries the band's effective
**scout vantage distance** — `scout.vantage_distance(scouts)` = `min(vantage_distance_base + scouts ×
vantage_distance_per_scout, vantage_distance_max)`, `0` with no scouts — since scouts now reveal by
posting forward-observer vantages that see around obstacles; field name kept for wire compat).

**Per-source food-income breakdown (retained yield telemetry).** `advance_labor_allocation` rebuilds
`LaborAllocation.last_yields` each turn — one `SourceYield { actual, sustainable, wasted, workers_needed, overdraws, realized }`
(f32 provisions + a worker count)
per assignment, **in the same index order** as `assignments` (so the snapshot zips by index — every
`LaborAllocation` mutator keeps the two aligned; see "Assign-time yield seeding"). It is
**derived, not persisted**: it is out of rollback (`#[serde]` never sees it; `labor_allocation_from_state`
restores only the assignments, leaving it empty until the next tick) and is **excluded from
`LaborAllocation`'s equality** (manual `PartialEq` compares assignments only) so it can't perturb the
persisted-intent comparison. A row is also written **at assign time**, seeded from the source's
pre-commit forecast, so a brand-new assignment shows its expected yield instead of `+0.00` before the
turn resolves (see "Assign-time yield seeding (the `+0.00` fix)" under Pre-commit Yield Forecast). Definitions: **`actual`** = the provisions the source produced this turn
(the value added to the larder); **`sustainable`** = what it could yield without drawing down its
stock. As of §0-ii **forage is depletable too**, so a forage `sustainable =
sustainable_yield(biomass_before, carrying_capacity, forage.ecology) × forage.provisions_per_biomass ×
output_multiplier`** (**MSY** — regrowth at the most-productive biomass K/2, so a *full* patch still
reads a positive sustainable harvest, no longer 0) — the plant mirror of the
**hunt `sustainable = sustainable_yield(biomass_before, carrying_capacity, ecology) ×
hunt.provisions_per_biomass × output_multiplier`** (MSY at the *pre-take* biomass). `sustainable_yield`
is shared by hunt + forage (`fauna.rs`); `net_biomass_delta` remains the **actual** per-turn biomass
evolution used by `regrow_biomass`/`advance_herds` (0 at K — correct there, unchanged).
A Sustain gather/hunt reads `actual ≈ sustainable`; an over-draw reads `actual > sustainable` (the
overdraw ⚠). Scout/Warrior push `{0,0,0}`. **`workers_needed`** is the parallel **overstaffing**
signal — and it has **two shapes, because the two webs' products differ**:
- **Forage (continuous)** — the *minimum* assigned workers that would have produced the same take:
  `ceil(take_biomass / per_worker_capacity)` clamped into `[1, assigned]` when anything was taken, else
  `0`, via the shared `workers_needed_for_take` helper (capacity = `forage.per_worker_biomass_capacity ×
  seasonal_weight`, matching `forage_take`'s worker cap so a low-season labor-bound patch isn't falsely
  flagged).
- **Hunt (whole animals)** — the **STEADY carry crew for the peak per-turn animal drop**
  (`fauna::hunt_haul_workers`, `ceil((floor(rate/body)+1)·body / hunt.per_worker_biomass_capacity)`, off
  the policy's steady `hunt_policy_rate`), **NOT** the lumpy `workers_needed_for_take(take.carried, …)`.
  A slow breeder whose MSY < `body_mass` drops **0** animals on a wait turn, so inverting `carried` would
  collapse `workers_needed` (to `0`, or the herder count for a managed herd) and **contradict the same
  row's `wasted_yield`** on that turn. The steady crew is stable across wait/kill turns and equals the
  client compose panel's `_max_useful_workers` cap by construction. A managed herd wraps it in
  `max(herders_needed, hunt_haul_workers)`; a wild herd (`herders_needed == 0`) reports it directly. See
  "Herding is standing labor" for the full note.
**Every rung derives it** (slice 7 — a managed source used to be fixed at `1`, which asserted that one
worker could carry home whatever the land offered). When the binding constraint on a source's take is **not** labor
(policy ceiling / biomass / regrowth), `workers_needed < assigned` → the source is overstaffed and the
extra workers were idle. The snapshot surfaces all of this: each `LaborAssignment` row
carries `actualYield`/`sustainableYield`/**`workersNeeded`** (client accessor `workersNeeded()`), and
each `PopulationCohortState` carries band-level
`foodIncome` (Σ per-source `actual`) + `foodConsumption` (the food the people **actually ate** this
turn — `PopulationCohort::last_food_consumption`, the real `stores` debit at the turn's *opening*
brackets, **not** a `food_demand` re-derived at capture on the post-turn brackets; the same turn's
births would inflate that and break the larder ledger identity by exactly the growth. `daysOfFood`
drains by the post-turn `food_demand` instead — a forward "turns I can last", a different question;
see the runway callout above).
All derived at capture (0 on a rehydrated save before the next tick). **The client
consumes these next** (allocation-panel rows + tooltip + ledger footer, a follow-up PR): a per-turn
`actual > sustainable` is the client-derived **overhunting signal** — a *leading* flow indicator,
distinct from the stock-based `ecology_phase` — and `workers > workersNeeded` is the **overstaffing**
indicator (flag the wasted labor on the source row + the forage biomass/cap tile-card row).

**The steady headline — `realized` / `realizedYield`.** The lumpy per-source
`actual` makes the band panel's "Food /turn" **swing** turn-to-turn (a whole-animal hunt pays 0 for
~6 turns then a spike). So each `SourceYield` also carries **`realized`** — a **FORWARD PROJECTION**:
the average food/turn the source will deliver over the next `labor_config.yield_average_horizon_turns`
(default **40**) turns, computed by simulating the herd/patch forward from its CURRENT state under the
assignment's policy + worker count (`fauna::project_realized_hunt` / `forage::project_realized_forage`,
mirroring the real turn order Logistics-regrow → Population-take, exactly as
`systems::expeditions::hunt_trip_forecast` does). It is a **pure function of state** — no history, no
persistence — so the assign-time seed and the resolved row compute the identical number (exact
forecast == actual, the true no-jump: `resolved_hunt_realized_equals_the_seeded_realized`). **Simulated
rate-based, WITHOUT the kill-credit bank:** the bank only quantises *when* whole animals arrive, never
the N-turn total, so projecting the smooth `hunt_policy_rate` gives the smooth average directly. **Why
not the instantaneous rate** (the bug this replaced): the instantaneous steady rate is
`sustainable_yield(current biomass)`, and biomass *sawtooths* every time a whole animal is killed
(drops one body, regrows between), so an instantaneous reading tracks that sawtooth — the projection's
N-turn average does not. It **uses the assignment's actual policy**, so switching Sustain↔Market
re-projects (a settled Sustain herd reads flat ≈ MSY over the full horizon; a Surplus/Market herd
declines within the window and the average honestly reflects it). A **self-terminating** policy
(Eradicate strips the herd in ~1 turn, Market drives it extinct) **breaks the loop early and divides by
the turns ACTUALLY simulated** (not the full cap), so it reads the high strip-rate it delivers *while
the source lasts* instead of a horizon-diluted average (`REALIZED_PROJECTION_TAKE_EPSILON` is the
negligible-take floor that ends the loop). Reuses the shared model helpers (`regrow_biomass`,
`hunt_policy_rate`, `pen_yield_biomass`, `hunt_provisions`, `forage_take`, `herd_ecology`/`herd_capacity`)
— no second copy of the ecology or take math. On the wire, `LaborAssignment.realizedYield` is appended
(append-only). **The `actual` value and the ledger identity are unchanged — `realized` is a parallel
steady value, never a replacement.** `PopulationCohortState.foodIncome` = Σ `actual` stays exactly as
it is: it is the real arrivals and is load-bearing for the
`larder_delta == foodIncome − foodConsumption − penFeedUpkeep` ledger identity.

> **RETIRED: the band-level `PopulationCohortState.foodIncomeAverage` (= Σ `realized`).** The client
> sums the Food line's income half **itself**, from the per-source `realizedYield` of the breakdown
> rows it renders, so the headline equals the Gathered + Hunted rows it sits above **by construction**
> rather than being a second, independently-computed total that could drift from them. That made a
> band-level duplicate redundant, and it was read by nobody. Marked `(deprecated)` in `snapshot.fbs`
> rather than deleted — deleting frees the field id for the next appender, and this repo is worked by
> concurrent sessions that append to these tables, so a freed slot is exactly how two branches collide
> on one id. **Do not re-add it**: if a band-level steady income is ever wanted again, sum the rows.
> The Σ `realized` value still exists as a *local* in `snapshot::population`, because
> `larder_runway_turns` needs a steady income term; it is simply not exported.

**WHEN the food lands — `arrivals` / `arrivalSchedule`.** The discrete twin of `realized`, from the
**same** forward simulation run **WITH** the kill-credit bank (`fauna::project_arrivals_hunt` /
`forage::project_arrivals_forage`) — because the two answer opposite halves of one question: the bank
decides *when* a whole animal lands and never *how much* lands over the window, so `realized` drops it
to get the smooth average and this keeps it to get the timing. `SourceYield.arrivals[i]` is the food
delivered `i + 1` turns from now, `labor_config.arrivals_horizon_turns` (**20**) entries long, `0.0` on
a turn nothing lands. A big-game Sustain hunt reads genuinely lumpy (zeros between hauls); a forage
patch — or fast game whose MSY clears a body every turn — is positive in **every** slot, a *continuous*
source the client renders as a solid run, with no special case in the projection. Their totals agree by
construction: `Σ arrivals ≈ realized × horizon`, to within the partial body still banked at the end.
- **It starts from the herd's REAL `hunt_credit`, never zero**, and it is projected from the source's
  **POST-take** state — so slot 0 is genuinely the *next* delivery, not the one this turn already paid.
  Both are load-bearing and both are pinned: zeroing the bank, or projecting pre-take, shifts the
  **first** arrival — the one the player cares most about — and
  `labor_allocation::the_arrival_schedule_matches_a_real_driven_hunt` fails on the exact turn index.
- **Pinned to real behaviour, not to another forecast** (the ~34-vs-~6-turn lesson): that test reads
  the schedule published on turn 0, then drives the **real** systems forward `horizon` turns and
  asserts the sim delivered on exactly the named turns in exactly the named amounts.
- Reuses the shared take helpers verbatim (`regrow_biomass`, `hunt_policy_rate`,
  `hunt_credit_ceiling`, `quantise_animal_take`, `pen_yield_biomass`, `hunt_provisions`,
  `forage_take`, `herd_ecology`/`herd_capacity`) — **no second copy of the take math** — and simulates
  on a clone, never the live source. Unlike the `realized` projection it does **not** break on a
  zero take: there a zero means *spent* and would dilute an average; here it is a **wait** turn, which
  is the entire mechanic. Only the extinction floor ends a hunt schedule early.
- **`arrivals_horizon_turns` is its OWN lever** (`labor_config.json`, default **20**, validated `> 0`),
  deliberately separate from `yield_average_horizon_turns` (40): this is a *display span* the client
  charts turn-by-turn, that one is a *smoothing window*.
- On the wire: `LaborAssignment.arrivalSchedule:[float]` (append-only, after `realizedYield`), on both
  `WorldSnapshot` and `WorldDelta`. A flat `[float]` rather than a vector of `{turn, amount}` tables is
  deliberate — **the index IS the turn offset**, so it needs no per-entry table and stays compact. An
  **empty** vector means *not projected* (Scout/Warrior, or a rehydrated `SourceYield::ZERO`), which the
  client must read as "no data", never as famine. **Client follow-up:** nothing renders it yet; the
  merged per-band larder projection is the client's to compose from these plus consumption — the sim
  owns the model (when + how much), walking the larder is presentation.

**The understaffing mirror — `wastedYield`** (slice 7, appended to `LaborAssignment`). `workersNeeded`
only ever answered *"are there too many workers here?"*; nothing answered *"too few?"*. `SourceYield.wasted`
= `production − actual` — what the source **offered that the crew could not collect**, where *production*
is what it hands over this turn (the policy ceiling on a wild/tended source, the managed rate on a
Field/pen) and *collection* is `workers × per-worker throughput`. The pair now answers both halves:
`workers > workersNeeded` ⇒ drop some, `wastedYield > 0` ⇒ add some. On a **Field or a pen** it is
genuinely food left standing (the crop rots / the meat stays on the hoof); on the **drawn-down rungs** it
simply stays in the stock and regrows. **Client follow-up:** nothing renders it yet.

All of the above is **post-hoc** (it reports what a committed turn produced). Its **pre-commit** twin —
the per-source `perWorkerYield` + policy ceilings the client uses to show an expected yield and cap the
worker stepper *before* the player commits — is the "Pre-commit Yield Forecast" section below, which
shares the take path's yield helpers so forecast == actual.

This is the general mechanism the arc scales: raise reach/throughput for settlements/cities, and a
future **trade policy** adds a consent gate + a priced return flow on *cross-faction* edges (see the
Trade note below). *v1:* population is the universal balancing weight, so a zero-population storage
node would compute a 0 fair share — revisit (→ capacity weight) when storage-pits land. The
connected-components pass is also what Phase 4 will use to derive settlement clusters.

### Sedentarization
The emergent per-faction "pressure to root in place" — the first slice of the pastoral→
settlement chain, and the consumer of Phase E's domestication seam.

`sedentarization_tick` (`sedentarization.rs`, `TurnStage::Population` after
`advance_labor_allocation`) computes a per-faction 0–100 **`SedentarizationScore`** each turn as
a config-weighted blend of normalized inputs, then **EMA-smooths** it (`smoothing`):
- **domestication** = `(HerdRegistry::domesticated_count(faction) +
  ForageRegistry::cultivated_count(faction)) / references.domesticated_herds` (the Phase E seam +
  the Phase 1a cultivation fold-in — plant + animal domestication share one driver; see "Cultivation"),
- **surplus** = Σ band `stores` food larders / `references.surplus` (band-local food, Phase 1),
- **resource density** = `HerdDensityMap::normalized_average()` (map-wide game richness — a v1
  baseline; per-faction-local density is a future refinement),
- **population** = Σ cohort size / `references.population`.

On a **rising** crossing of `soft_threshold` (~40, "establish a seasonal base?") or
`hard_threshold` (~70, "settle?") it pushes a `CommandEventKind::SedentarizationPrompt` to the
command feed (edge-gated on the stored `SedentarizationStage` so it doesn't re-fire; a fall
lowers the stage silently). The score is exported per-faction in the snapshot
(`SedentarizationState`, mirroring `factionInventory`) and shown as a HUD meter. Tunables live
in `data/sedentarization_config.json` (`sedentarization_config.rs`).

> **Reframed by the Settlement & Population Economy arc** (`docs/plan_settlement_population.md`):
> settlements are *derived* from clustered populated tiles + tended improvements (there is no
> discrete founding), and `SedentarizationScore` becomes an emergent readout of accumulated
> *tether* rather than a gate. See that design doc for the population/labor/improvement model
> this score ultimately feeds.

### Civilization Wellbeing (Morale → Discontent → Consequences)
The three-layer spine **factors → morale → discontent → consequences** (Phase 1). Authoritative
design: `docs/plan_civ_wellbeing.md`. Config: `wellbeing_config.rs` / `data/wellbeing_config.json`.
Extension seams are present and empty — future factors/consequences slot in without a rewrite.

- **Layer 1 — factors → morale.** `simulate_population` builds `MoraleContributions` (see morale
  attribution above); morale trends by their signed sum. Adding a factor = a new `MoraleFactor`
  variant + one field. The contributor set doubles as the client's itemized morale breakdown.
- **Layer 2 — discontent state (productivity only).** Each turn the cohort's `discontent_fraction =
  clamp((content_morale − morale) / (content_morale − floor_morale), 0, 1)` (0 at ≥`content_morale`
  0.6, 1 at ≤`floor_morale` 0.1). This drives **productivity only** — migration has its own onset
  (Layer 3b). A `grievance` accumulator (severity × duration) rises by `grievance_gain ×
  discontent_fraction` (× `trapped_multiplier` when *trapped* — below the migration threshold with no
  reachable destination) and decays by `grievance_decay` while content. **Phase 1 only populates
  `grievance`** — no consequence reads it (reserved for a future revolution trigger); it IS
  snapshot-**persisted** (like `age_turns`) so a rollback preserves brewing unrest.
- **Layer 3a — productivity modifier stack.** `output_multiplier(cohort, cfg) = Π(modifiers)`
  (`systems.rs`). Phase 1 has one entry, `discontent_output_modifier = max(floor_mult, 1 −
  discontent_fraction × discontent_weight)` (floor 0.5, weight 1.0). Applied at **payout** at every
  yield site via a single `output_multiplier` call — forage + hunt take (`advance_labor_allocation`),
  husbandry (`advance_husbandry`, `fauna.rs`). Adding
  an education/tech/government modifier is one line in `output_multiplier`, not per-site edits.
- **Layer 3b — tech-gated migration (own morale onset).** `advance_population_migration`
  (`systems.rs`, `TurnStage::Population`, **after** demographics + this turn's payouts).
  **Decoupled from `discontent_fraction`** — migration has its own morale-scaled onset at
  `migration.morale_threshold` (0.25): each band sheds `total × move_fraction`, where
  `move_fraction = max_rate × clamp((morale_threshold − morale) / morale_threshold, 0, 1)` — 0 at
  morale ≥ 0.25, 7.5% at 0.125, up to `max_rate` (0.15) at rock-bottom (gentle at onset, ramping to
  the cap). The total is split across brackets ∝ `bracket_size × weight` (working = 1.0, dependents
  = `dependent_weight` 0.4), so leavers are mostly workers while the headline fraction stays exact.
  They seek the **highest-morale eligible same-faction band within reach** (`base_reach` 4 hexes ×
  a movement-tech factor). *No concrete movement/transport tech signal exists yet, so the factor is
  stubbed at 1.0 with a `TODO(phase2)` hook.* Eligible = `morale ≥ attractive_morale` (0.5) AND
  `morale > source + min_morale_gap` (0.05). Found → **relocate** (source shrinks, destination
  grows; `last_emigrated`/`last_immigrated` recorded); none reachable → **stay** (grievance accrues
  faster via the trapped bonus). **Morale never causes faction population loss** — population is
  conserved within the faction; loss stays with starvation/cold only. Destinations are chosen from
  one pre-migration snapshot and all moves are computed before any is applied, so relocation is
  order-independent.
- **Snapshot.** `PopulationCohortState` gains `outputMultiplier`, `discontentFraction`, `grievance`,
  `lastEmigrated`/`lastImmigrated`, and the four itemized contributions
  `moraleSettling/Terrain/Climate/Unrest` (surfaced so the client can render the breakdown). All
  fixed-point except the two head-counts; all derived per-turn except `grievance` (persisted).

### Capability Flags
`CapabilityFlags` bitflags: `AlwaysOn`, `Construction`, `IndustryT1/T2`, `Power`, `NavalOps`, `AirOps`, `EspionageT2`, `Megaprojects`. Systems are inert until corresponding flag is set.

### Victory Engine
`VictoryState` with per-mode progress meters. Modes: Hegemony, Ascension, Economic, Diplomatic, Stewardship, Survival. `victory_tick` runs after end-of-turn accounting.

---

## Turn Loop

```
per-faction orders -> command server -> turn queue -> run_turn -> snapshot -> broadcaster -> clients
```

### Phases
1. **Collect** - `TurnQueue` awaits faction submissions
2. **Resolve** - Apply directives, execute `run_turn`, capture metrics, broadcast delta
3. **Advance** - Reset queue for next turn

### Turn Pipeline Config (`turn_pipeline_config.json`)
- **Logistics**: `flow_gain_min/max`, `effective_gain_min`, `penalty_min`, `capacity_min`, `attrition_max`
- **Trade**: `tariff_min`, `tariff_max_scalar`
- **Population**: Attrition scaling, temperature penalty, morale weighting, growth clamp, migration thresholds
- **Power**: `efficiency_adjust_scale`, `efficiency_floor`, storage efficiency/bleed clamps

---

## Snapshot History & Rollback

`SnapshotHistory` retains ring buffer of `WorldSnapshot` + `WorldDelta` pairs (default 256). `rollback <tick>` rewinds simulation, resets ECS world, truncates history.

The rollback snapshot round-trips the **authoritative `HerdRegistry`** (via `HerdState` + the shared `EcologyState` record in `WorldSnapshot.herd_registry`), not just the lossy display telemetry — see the herd-persistence note under "Fauna & Wild Game" for details and the bug it fixed. The **`ForageRegistry`** rides the same pattern (per-tile `ForageState` = tile key + the shared `EcologyState`, in `WorldSnapshot.forage_registry`) so a rollback rewinds forage depletion — see "Depletable Forage".

**Map export**: the `export_map [path]` command (`write_map_export` in `bin/server.rs`) writes the latest `SnapshotHistory.last_snapshot` plus the resolved `SimulationConfig.map_seed`/`map_preset_id` to disk as a `sim_schema::MapExport` JSON (default `exports/map-tick<t>-seed<s>.json`, gitignored). No new protocol — it rides the existing one-way command channel; the seed makes the dumped map reproducible, and the JSON doubles as an offline-inspectable, test-loadable fixture.

---

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

## See Also

- `docs/architecture.md` - System-wide data flow and extensibility
- `sim_schema/README.md` - FlatBuffers schema contracts
- `sim_runtime/README.md` - Shared runtime utilities
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` - Game manual
