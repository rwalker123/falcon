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
| `src/data/wellbeing_config.json` → `culture` | `near_bonus` (**0.01**) — morale per turn a band gains at full strength (`r = 1`, within its own walking reach) of a saturated anchor; scaled by `r` through kin. PLAYTEST DIAL; validated finite and `>= 0` at parse |
| | `away_drag` (**0.02**) — morale per turn a band loses when nothing ties it to a saturated anchor (`r = 0`); scaled by `1 − r` through kin. PLAYTEST DIAL; validated finite and `>= 0` |
| | `belief_half_saturation` (**10.0**) — the belief at which the anchor weighs one half, in dead-equivalents: ten people's worth of ancestors. PLAYTEST DIAL; validated finite and `> 0` (it is the weight's denominator at zero belief). Loader `wellbeing_config.rs` (`CultureConfig`), env override `WELLBEING_CONFIG_PATH` |
| | `min_anchor_belief` (**1.0**) — the least belief a place must hold before a band adopts it as its anchor: one whole death's worth. Gates adoption only. PLAYTEST DIAL; validated finite and `>= 0` |
| | `relay_per_hop` (**0.5**) — how much of the reach each hop of kin relays: a band tied in through `n` bands of its own people is near at `relay_per_hop ^ n`. PLAYTEST DIAL; validated finite and in `[0, 1]`; `0` turns relaying off and reproduces the unrelayed term exactly |
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
no remap (`BandRecord::cohort`; `SAVE_FORMAT_VERSION` 24). It is published, with the ground near it,
so a player told "far from the ancestors" can find where they are — see "On the wire and in the
checkpoint".

Each turn, before morale is computed, `refresh_belief_anchor` (`systems/population.rs`) walks the
sparse registry and takes every tile within walking reach of where the band **stands**
(`current_tile`, never `home` — the deaths source's reason) that holds at least
`culture.min_anchor_belief` (one whole death's worth). The strongest such tile replaces the
anchor only when it holds **strictly more** belief than the anchor holds now (`registry.get(anchor)`;
no anchor holds `NO_BELIEF`). Ties keep the existing anchor; among equal candidates the first in the
registry's row-major order wins. Because belief is monotone, the anchor's own value never falls, so
an anchor only ever moves to a stronger place.

**A place qualifies only once it holds one whole death's worth.** The deaths source credits the
turn's FRACTIONAL deaths every turn, old age included, so the tile a band stands on reads above zero
after its first turn. Without the line, "strictly more than the anchor holds" against a band holding
nothing adopted that tile at once — every band carried an anchor, and the map an urn, on its start
tile after two quiet turns. The line gates **adoption only**: accrual is unchanged, so a tile can
still read under one dead on its card, and the reach region and the anchor on the wire follow the
anchor (no anchor, no region).

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

### Kin relay the reach

A band of the SAME people standing within walking reach of a band that is near is itself near, at
`relay_per_hop` (0.5) per hop; each further hop multiplies by it again, with no cap — the strength
dies out. The relay strength `r` toward a band's OWN anchor is `relay_per_hop ^ hops`: `1` direct,
`0.5` one hop, `0` unreached.

**`core_sim/src/belief_relay.rs` is the one search**, `resolve_belief_relay`. For each people and
each distinct anchor any of its bands holds, a breadth-first search over that people's resident
bands: hop 0 is every band with `walk.within(roads, its standing, anchor)` — the direct test — and a
band joins at hop `h + 1` when `walk.within(roads, its standing, a hop-h band's standing)` (standing
tile first, the term's argument order). A relayer need not hold that anchor itself. The input is
ordered by `relay_order_key` (`BandId`, then entity), and `simulate_population` and the capture both
call it.

**What is guaranteed to agree, and what is not.** The hop count and the term come from ONE search on
the same turn: `simulate_population` stores the count it priced the term from on the cohort
(`PopulationCohort::last_belief_relay_hops`, beside `last_morale_contributions`), and the frame
publishes that stored count. It runs before `advance_band_movement` and
`advance_population_migration`, so a recount at capture on the moved positions would contradict
`moraleCulture` on any turn a band or its kin walk — which is why nothing recounts it. The drawn
regions answer a different question — where could this band walk NOW and be near or tied in — so
they are the same test applied to the frame's positions, struck at capture.

- **Same people only.** Another people's band beside you is not kin. Crossing between peoples belongs
  to the single neighbour-mixing system #765 calls for — culture, belief and later quantities
  spreading between neighbouring bands through one sim system — and `belief_relay.rs` is its first
  special case.
- **Resident bands only.** A detached party (no `ResidentBand`) is not a camp a band is tied in
  through — `simulate_population`'s own filter, and the capture's `resident_bands` query.
- **Relaying is never adoption.** A band reaches only toward an anchor it already holds; adoption
  stays direct (`refresh_belief_anchor`). A band with no anchor has no term, however near its kin.

`simulate_population` runs it in a pre-pass (`resolve_culture_terms`, beside
`resolve_breeding_ceilings`): refresh every band's anchor, then one relay search, then price every
band's term — all before any of the turn's deaths are credited, so no band's term depends on query
order.

### The formula

With `b = registry.get(anchor)`, `s = b / (b + belief_half_saturation)`
(`CultureConfig::anchor_weight`, a saturating `[0, 1)` weight: one death's worth barely registers, a
great cemetery approaches the full term) and `r` the relay strength:

```text
culture = s × (r × near_bonus − (1 − r) × away_drag)
```

| Band | `r` | `culture` |
|---|---|---|
| anchor within walking reach of where it stands | `1` | `+near_bonus × s` |
| tied in through `n` bands of kin | `relay_per_hop ^ n` | the blend |
| reached by no chain | `0` | `−away_drag × s` |
| no anchor | — | `0` |

The blend softens the drag as well as the bonus: a band one hop out is half near and half away. At
`r = 1` and `r = 0` it is exactly the two unrelayed values, and `relay_per_hop = 0` reproduces the
unrelayed term. The drag does not grow with distance. A band whose standing tile cannot be resolved
keeps its anchor, relays nothing and contributes `0`.

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
  the same bound `within` refuses past). Standing inside it a band is direct (`r = 1`,
  `+near_bonus × s`); outside it the band reads the blend if its kin tie it in (the relayed region
  below) and `−away_drag × s` if nothing reaches it. Empty with no anchor.
- The anchor and both regions are **derived at capture** (`snapshot/capture.rs` builds one
  `WalkReach` per capture from the
  live configs and the live roads) and never checkpointed — the anchor itself rides the cohort. Both
  live on `PopulationCohortState`, whose delta comparison is its derived `PartialEq`, so a road built
  or an anchor moved rides the next delta. A foreign band's redacted row publishes neither (the
  allow-list redaction defaults them).
- **The relay**: `beliefRelayHops:ubyte` — the hop count `moraleCulture` was priced from, published
  off `PopulationCohort::last_belief_relay_hops` (per-turn derived telemetry like the morale
  contributions; it rides the cohort into the checkpoint, `SAVE_FORMAT_VERSION` 25). `0` direct (and
  `0` with no anchor; read `hasBeliefAnchor`), `n` reached through `n` bands of kin, **`255`
  unreached** (`sim_schema::BELIEF_RELAY_UNREACHED`: an anchor is held and no chain reaches it); a
  chain longer than `254` publishes `254` (`BELIEF_RELAY_MAX_HOPS`). One encoding,
  `belief_relay::wire_hops`. A split's daughter and a launched party start at `0`.
- **The relayed region**: `beliefRelayReachX` / `beliefRelayReachY:[uint]`, zipped and row-major —
  every tile NOT in the direct region from which this band would be tied in through its OTHER kin:
  the union of `region_around(relayer)` over every band the anchor's reach gets to **with this band
  left out of the search** (`BeliefRelay::relayers_without`). Leaving the band out is what makes the
  region a statement about where it could walk: a band cannot relay to itself, nor through bands
  reached only through it. Derived at capture from the same search on the frame's positions, never
  saved; own bands only.

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
| `belief_culture::a_band_beyond_reach_of_its_anchor_loses_away_drag_and_names_culture` | walked beyond reach with no kin to tie it in (`r = 0`) it keeps the anchor and gains `−away_drag × s`; with the drag dominant the turn's cause is Culture, wire `4` |
| `belief_culture::a_band_that_never_stood_near_belief_has_no_anchor_and_no_term` | the stranger's-cemetery guard: belief out of reach leaves no anchor and a zero term |
| `belief_culture::a_road_brings_a_just_out_of_reach_anchor_into_reach` | a tile one step past `base_reach` is out of reach with no road and in reach over a laid trail |
| `belief_culture::the_anchor_moves_to_a_stronger_tile_but_not_an_equal_one` | an equal tile earlier in row-major order does not take the anchor; a stronger one does |
| `belief_culture::a_fission_daughter_inherits_the_anchor` | the splinter carries its parent's anchor |
| `belief_culture::the_anchor_round_trips_the_checkpoint_and_the_save` | `SimState` capture → restore, and the save payload's `BandRecord` |
| `belief_culture::the_culture_contribution_is_on_the_encoded_snapshot` | `moraleCulture` on the encoded envelope equals the cohort's contribution |
| `belief_culture::fractional_deaths_on_the_start_tile_do_not_anchor_the_band` | the reported bug: two quiet turns leave the start tile holding a fraction of a death, and the band has no anchor, no term, no anchor on the wire and no region |
| `belief_culture::a_place_is_adopted_only_once_it_holds_one_whole_death` | a place at 0.9 in reach gives no anchor and a zero term; at 1.0 it is adopted |
| `belief_culture::min_anchor_belief_scales_the_adoption_line` | at a lever of 3.0, a place holding 2.0 is refused and one holding 3.0 is adopted |
| `belief_culture::the_anchor_and_its_reach_region_are_on_the_encoded_snapshot` | the anchor's tile and its gate on the envelope; the region holds the anchor and the band's own tile and not a tile past `base_reach` |
| `belief_culture::a_band_with_no_anchor_publishes_an_empty_region` | no anchor: the gate is off, `0,0`, and the region is empty |
| `belief_culture::a_road_brings_a_just_out_of_reach_tile_into_the_published_region` | a tile one step past `base_reach` joins the region only once a road connects it, and the change rides the delta |
| `belief_culture::the_published_region_agrees_with_the_term_at_every_tile` | with a road bending the region, a band on every published tile reads near and on every bordering tile reads away |
| `belief_culture::a_band_within_reach_of_a_near_kin_band_is_near_at_half_strength` | A direct reads `w × near`; B, beyond the anchor but within A's reach, reads `w × (0.5 × near − 0.5 × away)` |
| `belief_culture::a_three_band_chain_reads_a_quarter_at_its_end` | the third band of a chain reads `r = 0.25` |
| `belief_culture::another_peoples_band_relays_nothing_either_way` | a rival band beside A is not reached through A, and a home band beyond the rival is not reached through it |
| `belief_culture::a_detached_party_does_not_relay` | a cohort with no `ResidentBand` between A and a kin band ties it in to nothing |
| `belief_culture::relay_per_hop_zero_reproduces_the_unrelayed_term` | at `relay_per_hop = 0` the direct band reads `w × near` and the kin band `−w × away` |
| `belief_culture::a_band_does_not_adopt_a_place_through_kin` | an anchorless band within reach of a near kin band gains no anchor and no term |
| `belief_culture::the_relay_hops_and_region_are_on_the_encoded_snapshot` | on the envelope: A `0`, B `1` with its own tile in a relayed region disjoint from the direct one, a far kin band `255` |
| `belief_culture::the_published_hop_count_is_the_one_the_term_was_priced_from` | B, one hop out, walks into direct reach during the turn: the encoded frame publishes hops `1` beside the one-hop blend in `moraleCulture`, not the `0` a recount on its new tile would give |
| `belief_culture::the_published_relayed_region_agrees_with_the_term_at_every_tile` | a kin band on every published relayed tile reads `r > 0`, and on every bordering tile outside both regions `r == 0` |
| `belief_relay::tests::*` | the search on its own: a chain halves per hop, another people relays nothing, an anchorless band reaches nothing, a band is not its own relayer |
| `labor_allocation::a_far_work_partys_hunt_dead_credit_no_belief` | a party posted past `band_work_range` loses people and the registry stays empty — neither the camp nor the herd tile gains belief |
