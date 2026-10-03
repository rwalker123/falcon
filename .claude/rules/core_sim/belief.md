---
paths:
  - "core_sim/src/belief.rs"
  - "core_sim/src/belief_config.rs"
  - "core_sim/src/data/belief_config.json"
  - "core_sim/src/systems/population.rs"
  - "core_sim/src/systems/labor.rs"
  - "core_sim/tests/belief.rs"
---

# Belief on a tile — a per-PLACE stock that never decays

Design of record: `docs/plan_civilization_steps.md` §"Belief is a property of a place" and §"The
first pulls are not productive" (issue #697). Engine: `core_sim/src/belief.rs` (the store),
`core_sim/src/belief_config.rs` (the lever), the deaths source in `systems/population.rs` and the
combat sites in `systems/labor.rs`.

## Config files

| File | Purpose |
|------|---------|
| `src/data/belief_config.json` | `belief_per_death` (**1.0**) — belief added to the tile a band stands on, per person who dies there. At `1.0` the unit of belief **is** the dead-equivalent: a place reading `12` holds twelve people's worth of ancestors, and later sources are priced in that unit. Loader `belief_config.rs` on the shared boot seam (`config-loading.md`), env override `BELIEF_CONFIG_PATH`. No hot-reload kind. **There is deliberately no decay lever** |

## The store: `BeliefRegistry`

A resource keyed by tile, `RoadRegistry`'s shape: a `BTreeMap<(y, x), f32>`, so iteration is
row-major and the checkpoint observes an order rather than an accident. **Sparse** — only a tile
holding belief has an entry, so an untouched map checkpoints an empty map.

- **Belief is a PLACE's, not a faction's.** Nothing on the registry names a people. Whose dead lie
  there is not recorded; that they lie there is.
- **It is monotone.** Nothing subtracts from it and nothing decays it: an abandoned place keeps its
  dead, and a band that walks away leaves the stock where it accrued. The registry has no `remove` or
  `set`.
- **`add(position, amount)` is THE seam every source writes through** — deaths now; gatherings
  (#698) and the monument (#692) are further sources on the same seam. An `amount <= 0` is ignored,
  which is what keeps the stock monotone and the map sparse. `get(position)` answers `NO_BELIEF`
  (`0.0`) where nothing has accrued.
- **`credit_deaths(position, deaths, config)`** is the deaths source's one expression,
  `deaths × belief_per_death`, so no call site restates it.

## The deaths source: where the band STANDS, never its home

The dead are buried where the band is, so every credit goes to `cohort.current_tile`'s position, not
`cohort.home`. The two differ whenever a band is travelling, and crediting home would let a band
accrue a cemetery at a place it has left.

| Site | Counts? | What is credited |
|---|---|---|
| `simulate_population` — starvation, temperature and old age | **yes** | `DemographicFlows::total_deaths()`, the turn's **fractional** death total — not the whole-person `died` events. Belief is a continuous stock, so a third of a person three turns running is one death's worth, and a band with no `BandId` or flow carry still buries its dead |
| `settle_hunt_band_side` — a resident band's own hunt, both rungs (range arm and pen tend), worked from the band's own range | **yes** | the people actually lost, at the band's tile |
| `settle_hunt_band_side` — the same hunt or tend carried out by a far **work party** (`work-party.md`) | **no** | the party fights at the herd, away from where the band stands — the expedition's reason. Its workers stay the band's; only where they died decides the credit |
| `advance_predator_raids` — a raid on a resident band | **yes** | the people actually lost, at the band's tile; the pack came to the band, so the band's tile is where they died |
| `advance_expeditions` — a detached party's raid (`Deny`) or a scout's roadside kill | **no** | those people died at the party's position, not where the band stands |

**"Actually lost" is `apply_combat_casualties`' return value**: the removal floored at the working
bracket that was there. A fight asking for more dead than the band holds credits only the bracket.
The two labor sites pass `systems::labor::BeliefSink` — the band's position, the registry and the
config — so the hunt seam names one place its dead go rather than taking three loose arguments.

## On the wire and in the checkpoint

- **`TileState.belief:float`** (appended last on `snapshot.fbs`'s `TileState`) carries the registry's
  value for every tile, read in the capture's one tile sweep beside graze. It is in
  `TileState::same_published_state` at the hundredths every other float uses, so a tile whose belief
  moved rides the next delta. Belief is not fog-gated, which is graze's arrangement.
- **`SimState::belief`** carries the registry whole; a restore inserts it back. It is state, not
  derived, for the road's reason: nothing can rebuild it. `SAVE_FORMAT_VERSION` 20.
- Classified in `sim_state_coverage.rs` — `BeliefRegistry` as sim state, the config handle and
  metadata as config resources.

## Tests

| Test | Claim |
|---|---|
| `belief::deaths_on_a_band_away_from_home_credit_the_tile_it_stands_on` | a starving band off its home credits the tile it stands on and leaves home's belief unchanged |
| `belief::belief_never_decays_after_the_band_walks_away` | five full turns after the band leaves, the place reads exactly what it held |
| `belief::belief_per_death_scales_the_accrual` | the same deaths at a doubled lever credit twice the belief |
| `belief::a_tiles_belief_is_on_the_frame_and_a_change_produces_a_tile_delta` | on the encoded envelope and on the stream's tile delta |
| `belief::belief_round_trips_the_checkpoint_and_the_save` | `SimState` capture → restore, and the save payload |
| `predator_raid::a_raids_dead_credit_belief_to_the_tile_the_band_stands_on` | a raid credits the lost head-count at the band's tile, not its home |
| `raiding_party::a_raiding_partys_dead_credit_no_belief` | a lethal raid by a detached `Deny` party leaves the registry untouched (with a liveness assertion that people died) |
| `labor_yield_tests::a_resident_hunts_dead_credit_belief_to_the_tile_the_band_stands_on` | the hunt seam credits the band's tile, and only the people actually lost |
| `labor_allocation::a_far_work_partys_hunt_dead_credit_no_belief` | a party posted past `band_work_range` loses people and the registry stays empty — neither the camp nor the herd tile gains belief |
