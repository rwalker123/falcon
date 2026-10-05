---
paths:
  - "core_sim/src/belief.rs"
  - "core_sim/src/belief_config.rs"
  - "core_sim/src/data/belief_config.json"
  - "core_sim/src/systems/population.rs"
  - "core_sim/src/systems/labor.rs"
  - "core_sim/src/systems/fission.rs"
  - "core_sim/src/snapshot/capture.rs"
  - "core_sim/src/snapshot/population.rs"
  - "core_sim/src/wellbeing_config.rs"
  - "core_sim/src/data/wellbeing_config.json"
  - "core_sim/tests/belief.rs"
  - "core_sim/tests/belief_culture.rs"
---

# Belief on a tile — a per-PLACE stock that never decays

Design of record: `docs/plan_civilization_steps.md` §"Belief is a property of a place" and §"The
first pulls are not productive" (issue #697). Engine: `core_sim/src/belief.rs` (the store),
`core_sim/src/belief_config.rs` (the lever), the deaths source in `systems/population.rs` and the
combat sites in `systems/labor.rs`. What belief DOES — the culture morale term (issue #699,
§"What belief does, through seams that exist") — is "The culture morale term" below.

## Config files

| File | Purpose |
|------|---------|
| `src/data/wellbeing_config.json` → `culture` | `near_bonus` (**0.01**) — morale per turn a band gains standing within walking reach of a saturated anchor. PLAYTEST DIAL; validated finite and `>= 0` at parse |
| | `away_drag` (**0.02**) — morale per turn a band loses standing beyond reach of a saturated anchor. PLAYTEST DIAL; validated finite and `>= 0` |
| | `belief_half_saturation` (**10.0**) — the belief at which the anchor weighs one half, in dead-equivalents: ten people's worth of ancestors. PLAYTEST DIAL; validated finite and `> 0` (it is the weight's denominator at zero belief). Loader `wellbeing_config.rs` (`CultureConfig`), env override `WELLBEING_CONFIG_PATH` |
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

## The culture morale term — near / far from the ancestors

Belief's first consumer: a Layer-1 morale contributor (`MoraleContributions::culture`,
`MoraleFactor::Culture`) computed in `simulate_population` beside terrain, climate and unrest.

### Each band remembers ONE place: its anchor

`PopulationCohort::belief_anchor: Option<UVec2>` — the strongest belief tile the band has stood
within walking reach of. **The band holds it, not the registry**: belief stays ownerless (nothing on
the registry names a people), and which place a band counts as its ancestors' is a fact about the
band. It is a tile **position**, not an `Entity`, so the checkpoint carries it inside the cohort with
no remap (`BandRecord::cohort`; `SAVE_FORMAT_VERSION` 22). It is published, with the ground near it,
so a player told "far from the ancestors" can find where they are — see "On the wire and in the
checkpoint".

Each turn, before morale is computed, `refresh_belief_anchor` (`systems/population.rs`) walks the
sparse registry and takes every tile within walking reach of where the band **stands**
(`current_tile`, never `home` — the deaths source's reason). The strongest such tile replaces the
anchor only when it holds **strictly more** belief than the anchor holds now (`registry.get(anchor)`;
no anchor holds `NO_BELIEF`). Ties keep the existing anchor; among equal candidates the first in the
registry's row-major order wins. Because belief is monotone, the anchor's own value never falls, so
an anchor only ever moves to a stronger place.

**A stranger's cemetery is not yours.** A band that has never stood within reach of any belief has
no anchor and no term, however much belief lies elsewhere on the map.

### "Within reach" is the migration walk test, road-aware

`supply::WalkReach::within` is the one test of how far a band's people walk:
`hex_distance − road_bonus <= migration.base_reach`, the road bonus being
`supply::free_pooling_reach_tiles − reach_tiles` (the work party's walk seam). It has two readers —
`advance_population_migration` (is a destination near enough to move camp to) and the culture term
(is the band near enough to its dead) — so a paved road lengthens how far a band can stand from its
ancestors exactly as it lengthens how far its people will move. One lever, `base_reach`; there is no
second radius. A pair inside plain reach is never traced, and a pair past
`base_reach + (max_route_reach_tiles − reach_tiles)` is out without a trace. `simulate_population`
builds it from `WalkReachInputs` (roads, supply config, route ladder, tile registry), and every
reader constructs it through the one `WalkReach::for_people`.

### The formula

With `b = registry.get(anchor)` and `s = b / (b + belief_half_saturation)`
(`CultureConfig::anchor_weight`, a saturating `[0, 1)` weight: one death's worth barely registers, a
great cemetery approaches the full term):

| Band | `culture` |
|---|---|
| anchor within walking reach of where it stands | `+near_bonus × s` |
| anchor beyond reach | `−away_drag × s` |
| no anchor | `0` |

**In or out of reach is binary** — the drag does not grow with distance. A band whose standing tile
cannot be resolved keeps its anchor and contributes `0`.

`MoraleCause::Culture` (wire `4`) names it when it is the dominant negative contributor; the
tie-break order is Terrain ≥ Climate ≥ Unrest ≥ Culture (`MoraleContributions::contributions`).

### Who inherits the anchor

- **A fission daughter inherits its parent's anchor** — the same people, the same dead. The split's
  `cohort.clone()` carries it and `split_band_from_parent` deliberately does not reset it.
- **Migration into an existing band leaves the destination's anchor unchanged**: the people join a
  band, and the band's memory is the band's.
- **Every other cohort starts `None`**: the opening bands at worldgen, and a detached party
  (`belief_anchor` cleared at launch — a party keeps no morale of its own).

## On the wire and in the checkpoint

- **`TileState.belief:float`** (appended last on `snapshot.fbs`'s `TileState`) carries the registry's
  value for every tile, read in the capture's one tile sweep beside graze. It is in
  `TileState::same_published_state` at the hundredths every other float uses, so a tile whose belief
  moved rides the next delta. Belief is not fog-gated, which is graze's arrangement.
- **`SimState::belief`** carries the registry whole; a restore inserts it back. It is state, not
  derived, for the road's reason: nothing can rebuild it. `SAVE_FORMAT_VERSION` 20.
- Classified in `sim_state_coverage.rs` — `BeliefRegistry` as sim state, the config handle and
  metadata as config resources.
- **`PopulationCohortState.moraleCulture:long`** (appended last on the table, fixed-point like its
  `moraleSettling/Terrain/Climate/Unrest` siblings) carries the band's culture contribution;
  `moraleCause` `4` is Culture.
- **The anchor**: `hasBeliefAnchor:bool` gating `beliefAnchorX` / `beliefAnchorY:uint` (`0,0` with no
  anchor — the `isTraveling` / `travelTargetX/Y` idiom on the same table).
- **The reach region**: `beliefReachX` / `beliefReachY:[uint]`, zipped and row-major (the
  `pendingRevealX/Y` idiom) — every tile a band could stand on and still count as near its anchor.
  **It is the same test the term runs, not a second rule**: `WalkReach::region_around(roads, anchor)`
  keeps exactly the tiles `t` for which `within(roads, t, anchor)` holds, standing tile first, and
  scans only the hex disk of `WalkReach::max_reach_tiles` (`base_reach` plus the widest road bonus,
  the same bound `within` refuses past). Inside the region the term reads `+near`, outside it `−away`.
  Empty with no anchor.
- Both are **derived at capture** (`snapshot/capture.rs` builds one `WalkReach` per capture from the
  live configs and the live roads) and never checkpointed — the anchor itself rides the cohort. Both
  live on `PopulationCohortState`, whose delta comparison is its derived `PartialEq`, so a road built
  or an anchor moved rides the next delta. A foreign band's redacted row publishes neither (the
  allow-list redaction defaults them).

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
| `belief_culture::a_band_within_reach_of_belief_gains_near_bonus_times_its_weight` | a band standing within reach of a belief tile anchors to it and gains exactly `near_bonus × s` |
| `belief_culture::a_band_beyond_reach_of_its_anchor_loses_away_drag_and_names_culture` | walked beyond reach it keeps the anchor and gains `−away_drag × s`; with the drag dominant the turn's cause is Culture, wire `4` |
| `belief_culture::a_band_that_never_stood_near_belief_has_no_anchor_and_no_term` | the stranger's-cemetery guard: belief out of reach leaves no anchor and a zero term |
| `belief_culture::a_road_brings_a_just_out_of_reach_anchor_into_reach` | a tile one step past `base_reach` is out of reach with no road and in reach over a laid trail |
| `belief_culture::the_anchor_moves_to_a_stronger_tile_but_not_an_equal_one` | an equal tile earlier in row-major order does not take the anchor; a stronger one does |
| `belief_culture::a_fission_daughter_inherits_the_anchor` | the splinter carries its parent's anchor |
| `belief_culture::the_anchor_round_trips_the_checkpoint_and_the_save` | `SimState` capture → restore, and the save payload's `BandRecord` |
| `belief_culture::the_culture_contribution_is_on_the_encoded_snapshot` | `moraleCulture` on the encoded envelope equals the cohort's contribution |
| `belief_culture::the_anchor_and_its_reach_region_are_on_the_encoded_snapshot` | the anchor's tile and its gate on the envelope; the region holds the anchor and the band's own tile and not a tile past `base_reach` |
| `belief_culture::a_band_with_no_anchor_publishes_an_empty_region` | no anchor: the gate is off, `0,0`, and the region is empty |
| `belief_culture::a_road_brings_a_just_out_of_reach_tile_into_the_published_region` | a tile one step past `base_reach` joins the region only once a road connects it, and the change rides the delta |
| `belief_culture::the_published_region_agrees_with_the_term_at_every_tile` | with a road bending the region, a band on every published tile reads near and on every bordering tile reads away |
| `labor_allocation::a_far_work_partys_hunt_dead_credit_no_belief` | a party posted past `band_work_range` loses people and the registry stays empty — neither the camp nor the herd tile gains belief |
