use super::*;
use crate::fauna::AnimalTake;

/// Advance any `move_band` order one step toward its target. The band travels at
/// `band_move_tiles_per_turn` tiles/turn; `current_tile` (and `home`, since a nomad band has no
/// fixed origin) follow it so labor reads the updated in-range source set, and on arrival the
/// `BandTravel` component is removed. Movement is the only way a band repositions — hunting uses a
/// bounded leash, never a whole-band chase.
pub fn advance_band_movement(
    mut commands: Commands,
    labor_config: Res<LaborConfigHandle>,
    sim_config: Res<SimulationConfig>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    mut cohorts: Query<(Entity, &mut PopulationCohort, &BandTravel)>,
) {
    let labor = labor_config.get();
    let width = tile_registry.width;
    let wrap_horizontal = sim_config.map_topology.wrap_horizontal;
    for (entity, mut cohort, travel) in cohorts.iter_mut() {
        let current = tiles
            .get(cohort.current_tile)
            .map(|tile| tile.position)
            .unwrap_or(travel.target);
        if current == travel.target {
            commands.entity(entity).remove::<BandTravel>();
            continue;
        }
        let next = step_toward(
            current,
            travel.target,
            labor.band_move_tiles_per_turn,
            width,
            wrap_horizontal,
        );
        if let Some(tile_entity) = tile_registry.index(next.x, next.y) {
            cohort.current_tile = tile_entity;
            cohort.home = tile_entity;
        }
        if next == travel.target {
            commands.entity(entity).remove::<BandTravel>();
        }
    }
}

/// Per-turn logic for detached expeditions (traveling parties). Runs right after
/// `advance_band_movement` (so it reads the party's fresh position) and before the Visibility
/// stage's `discover_sites`. For each expedition:
/// - **Observe + comm-flush is SHARED by every mission (scout AND hunt)** — a ranging party maps the
///   terrain it crosses regardless of verb. Each turn it observes the tiles in `observe_sight_range`
///   LOS of its current tile into a **private** pending-reveal buffer (it does NOT touch the faction
///   map — it is `Without<Expedition>` in `calculate_visibility`); and when within the effective comm
///   range of the home band's live tile, promotes every buffered tile to `Discovered` on the faction
///   map (never downgrading a live `Active` tile) and clears the buffer. For a hunt party this fires
///   at each Delivering drop-off / Returning fold-back. Site discovery rides the flushed tiles for
///   free via the Visibility stage's `discover_sites`.
/// - **Provisions** drain by `party × provision_upkeep_per_worker` (scouts only — hunt lives off its
///   kills); non-fatal at zero in v1.
/// - **Phase transitions**: `Outbound` + arrived (no `BandTravel`) → `AwaitingOrders` + a one-shot
///   arrival feed line; `Returning` → chase the home band's live tile and, once within comm range,
///   fold workers + leftover provisions back into the band and despawn (fold-back happens after the
///   flush so the final findings report); `AwaitingOrders` waits (relaunched by `move_band`).
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_expeditions(
    mut commands: Commands,
    expedition_config: Res<crate::expedition_config::ExpeditionConfigHandle>,
    visibility_config: Res<crate::visibility_config::VisibilityConfigHandle>,
    fauna_config: Res<FaunaConfigHandle>,
    labor_config: Res<LaborConfigHandle>,
    ladder_config: Res<LadderConfigHandle>,
    sim_config: Res<SimulationConfig>,
    tile_registry: Res<TileRegistry>,
    tick: Res<SimulationTick>,
    elevation: Option<Res<ElevationField>>,
    mut ledger: ResMut<crate::visibility::VisibilityLedger>,
    mut event_log: ResMut<CommandEventLog>,
    mut herds: ResMut<HerdRegistry>,
    tiles: Query<&Tile>,
    mut expeditions: Query<(
        Entity,
        &mut PopulationCohort,
        Option<&BandTravel>,
        &mut Expedition,
    )>,
    mut bands: Query<&mut PopulationCohort, Without<Expedition>>,
) {
    // The common turn has zero expeditions — bail before building the O(w×h) terrain grid so a
    // normal game pays nothing for this system.
    if expeditions.is_empty() {
        return;
    }
    // No elevation field means worldgen hasn't run — nothing to observe from (mirrors
    // `calculate_visibility`'s early bail).
    let Some(elevation) = elevation else {
        return;
    };
    let cfg = expedition_config.get();
    let fauna = fauna_config.get();
    let labor = labor_config.get();
    let ladder = ladder_config.get();
    let vis_cfg = visibility_config.0.as_ref();
    let wrap_horizontal = sim_config.map_topology.wrap_horizontal;
    let grid_width = tile_registry.width;
    let current_turn = tick.0;
    let comm_range = cfg.effective_comm_range();
    let per_worker_biomass = labor.hunt.per_worker_biomass_capacity;

    // Shared LOS inputs (built once per turn for the few expeditions).
    let terrain_tags = crate::visibility_systems::build_terrain_tags_grid(
        &tiles,
        elevation.width,
        elevation.height,
    );
    let blocking_tags = crate::visibility_systems::parse_blocking_tags(
        &vis_cfg.line_of_sight.blocking_terrain_tags,
    );

    for (entity, mut cohort, travel, mut expedition) in expeditions.iter_mut() {
        let Ok(exp_pos) = tiles.get(cohort.current_tile).map(|tile| tile.position) else {
            continue;
        };
        let faction = cohort.faction;
        let workers = available_workers(cohort.working);
        // Home band's LIVE tile (bands are nomadic): drives the comm check, the return target, and
        // the hunt drop-off. An orphaned expedition (home band gone) simply can't report/deliver.
        let home_pos = bands
            .get(expedition.home_band)
            .ok()
            .and_then(|band| tiles.get(band.current_tile).ok())
            .map(|tile| tile.position);
        // "Near enough to run home" — the shared proximity for the scout fold-back, hunt delivery,
        // and comm-range flush.
        let near_home = home_pos
            .map(|home| {
                crate::grid_utils::hex_distance_wrapped(exp_pos, home, grid_width, wrap_horizontal)
                    <= comm_range
            })
            .unwrap_or(false);
        let mission = expedition.mission.clone();

        // A hunt party whose herd is lost/extinct flips to Returning (folds back via the shared
        // arm below), with a feed line — knowledge/food it carries still comes home.
        if let ExpeditionMission::Hunt { fauna_id, .. } = &mission {
            if herds.find(fauna_id).is_none()
                && !matches!(expedition.phase, ExpeditionPhase::Returning)
            {
                expedition.phase = ExpeditionPhase::Returning;
                event_log.push(CommandEventEntry::new(
                    current_turn,
                    CommandEventKind::Hunt,
                    faction,
                    format!("Hunting expedition lost the {} — returning home", fauna_id),
                    Some(format!(
                        "status=returning reason=herd_gone expedition={}",
                        entity.to_bits()
                    )),
                ));
            }
        }

        // ---- Map documentation (SHARED — all missions, scout AND hunt) ----
        // A ranging party maps the terrain it crosses regardless of verb, so observe + comm-flush is
        // mission-agnostic. Scout-specific bits (upkeep, replenish, awaiting-orders) stay below.
        // a. Observe into the private buffer — no faction-map mutation here. Dedup against an
        // O(1) `HashSet` scratch (built once) instead of an O(n) `Vec::contains` per tile.
        let mut seen: HashSet<UVec2> = expedition.pending_reveal.iter().copied().collect();
        for pos in crate::visibility_systems::visible_tiles_in_range(
            exp_pos,
            cfg.observe_sight_range,
            &elevation,
            vis_cfg.line_of_sight.enabled,
            &terrain_tags,
            &vis_cfg.terrain_modifiers,
            blocking_tags,
            wrap_horizontal,
        ) {
            if seen.insert(pos) {
                expedition.pending_reveal.push(pos);
            }
        }

        // b. Comm check + flush: in range of home → report the buffer as Discovered, then clear.
        // For a hunt party this naturally fires at each Delivering drop-off and on Returning
        // fold-back (it's near the band then), so its findings report home with the food; sites on
        // the flushed tiles ride `discover_sites` for free, same as the scout.
        if near_home {
            let map = ledger.ensure_faction(faction, elevation.width, elevation.height);
            for pos in expedition.pending_reveal.drain(..) {
                map.discover(pos.x, pos.y, current_turn);
            }
        }

        // ---- Scout-only: provisions upkeep + opportunistic replenish (hunt lives off its kills) ----
        if matches!(mission, ExpeditionMission::Scout) {
            // c. Provisions depletion (scouts only — hunt parties live off their kills). Non-fatal.
            let upkeep = scalar_from_f32(workers as f32 * cfg.provision_upkeep_per_worker);
            if upkeep > scalar_zero() {
                cohort.stores.take(FOOD, upkeep);
            }

            // Opportunistic replenish: when provisions fall below `party × upkeep × low_turns` and a
            // huntable herd is within reach, top up off it via the shared `hunt_take` primitive
            // (capped at the low-water buffer so it doesn't overfill). Same code path as the hunt.
            let low_buffer = scalar_from_f32(
                workers as f32 * cfg.provision_upkeep_per_worker * cfg.replenish.low_turns as f32,
            );
            if cohort.stores.get(FOOD) < low_buffer {
                // First huntable herd within replenish reach (not necessarily the closest —
                // `position` returns the first match).
                let in_range = herds.herds.iter().position(|herd| {
                    crate::grid_utils::hex_distance_wrapped(
                        exp_pos,
                        herd.position(),
                        grid_width,
                        wrap_horizontal,
                    ) <= cfg.replenish.reach_tiles
                });
                if let Some(idx) = in_range {
                    // A scout only nibbles the sustainable surplus off passing game (the Sustain
                    // escapement), not the productive hunt the hunt verb runs. The room the scout has
                    // to top up with bounds its **collection** (invert `provisions_per_biomass`), so a
                    // nearly-topped-up scout takes fewer animals rather than killing one it has no
                    // room for.
                    //
                    // **A scout can still waste** — one worker cannot carry a whole aurochs, and it
                    // does not get to half-kill one. Nothing reports that waste (a scout keeps no
                    // per-source yield row), which is honest as far as it goes: an opportunistic
                    // roadside kill is exactly where a party leaves most of the carcass.
                    let room = (low_buffer - cohort.stores.get(FOOD)).max(scalar_zero());
                    let provisions_per_biomass = fauna.hunt.provisions_per_biomass;
                    let carry_room_biomass = if provisions_per_biomass > 0.0 {
                        room.to_f32() / provisions_per_biomass
                    } else {
                        f32::INFINITY
                    };
                    let take = hunt_take(
                        &mut herds.herds[idx],
                        workers,
                        FollowPolicy::Sustain,
                        per_worker_biomass,
                        &fauna,
                        &ladder,
                        carry_room_biomass,
                    );
                    let provisions =
                        scalar_from_f32(fauna::hunt_provisions(take.carried, &fauna, 1.0));
                    let added = provisions.min(room);
                    if added > scalar_zero() {
                        cohort.stores.add(FOOD, added);
                    }
                }
            }
        }

        // ---- Phase machine ----
        match expedition.phase {
            ExpeditionPhase::Outbound => {
                // Scout arrived when `advance_band_movement` (earlier this turn) removed the travel
                // order → awaiting orders (the decision point) + a one-shot feed line.
                if travel.is_none() {
                    expedition.phase = ExpeditionPhase::AwaitingOrders;
                    if !expedition.announced {
                        event_log.push(CommandEventEntry::new(
                            current_turn,
                            CommandEventKind::ExpeditionArrived,
                            faction,
                            format!(
                                "Expedition reached ({}, {}) — awaiting orders",
                                exp_pos.x, exp_pos.y
                            ),
                            Some(format!("status=awaiting expedition={}", entity.to_bits())),
                        ));
                        expedition.announced = true;
                    }
                }
            }
            ExpeditionPhase::AwaitingOrders => {
                // Wait — a `move_band` order flips the party back to Outbound (server-side hook).
            }
            ExpeditionPhase::Returning => {
                if near_home {
                    // Close enough to run home: fold workers + carried food back in (after the scout
                    // flush above, so the final findings reported), then despawn.
                    if let Ok(mut home) = bands.get_mut(expedition.home_band) {
                        home.working += cohort.working;
                        let leftover = cohort.stores.get(FOOD);
                        if leftover > scalar_zero() {
                            home.stores.add(FOOD, leftover);
                        }
                        home.sync_size();
                    }
                    event_log.push(CommandEventEntry::new(
                        current_turn,
                        CommandEventKind::ExpeditionReturned,
                        faction,
                        format!(
                            "Expedition folded back into the band at ({}, {})",
                            exp_pos.x, exp_pos.y
                        ),
                        Some(format!("status=returned expedition={}", entity.to_bits())),
                    ));
                    commands.entity(entity).despawn();
                } else if let Some(home) = home_pos {
                    // Chase the band's live tile each turn (retargets any stale travel order).
                    commands.entity(entity).insert(BandTravel { target: home });
                }
            }
            ExpeditionPhase::Hunting => {
                // Chase the herd and, when in reach, take a **productive** hunt's worth of biomass
                // (`workers × per_worker_biomass_capacity`, capped per policy) → provisions up to the
                // carry cap. Then, per policy, decide whether the trip is complete. The
                // trip-completion decision lives INSIDE the in-reach guard: a party still walking to
                // its herd must never conclude the trip.
                if let ExpeditionMission::Hunt { fauna_id, policy } = &mission {
                    if let Some(idx) = herds.herds.iter().position(|herd| herd.id == *fauna_id) {
                        let policy = *policy;
                        let herd_pos = herds.herds[idx].position();
                        // The herd's OWN ecology + capacity — the single source of the husbandry
                        // ladder's rung → growth-rate mapping (`herd_ecology` / `herd_capacity`); a
                        // party hunting a tamed or penned herd draws on *its* curve, not the wild one.
                        let carrying_capacity = herd_capacity(&herds.herds[idx], &fauna);
                        let ecology = herd_ecology(&herds.herds[idx], &fauna);
                        let cap = scalar_from_f32(workers as f32 * cfg.hunt.per_worker_carry);
                        let in_reach = crate::grid_utils::hex_distance_wrapped(
                            exp_pos,
                            herd_pos,
                            grid_width,
                            wrap_horizontal,
                        ) <= cfg.hunt.reach_tiles;
                        if !in_reach {
                            // Still walking — chase the herd's live tile.
                            commands
                                .entity(entity)
                                .insert(BandTravel { target: herd_pos });
                            continue;
                        }

                        // Productive take: the greedy raid (`expedition_take_biomass`) — the party
                        // grabs the herd's standing surplus above the policy's `hunt_expedition_floor`
                        // as fast as its throughput allows, so more hunters take more animals in
                        // fewer-or-equal turns (a resident band's throttled per-turn rate was
                        // worker-independent — a second hunter only added pack to fill, lengthening the
                        // trip). The launch forecast SIMULATES this same helper, so the preview can't
                        // quote a different raid than this take. Eradicate carries no food (denial).
                        let herd_biomass_before = herds.herds[idx].biomass;
                        // The surplus the raid may take — kept for the empty-pack diagnosis below
                        // (`<= 0` → the herd is at/below the policy's floor and yields nothing).
                        let standing_surplus = (herd_biomass_before
                            - hunt_expedition_floor(policy, carrying_capacity, &ecology, &fauna))
                        .max(0.0);
                        let provisions_per_biomass = fauna.hunt.provisions_per_biomass;
                        // A delivering party can only take home the biomass it has room for. The room
                        // bounds the party's **collection** (invert `provisions_per_biomass`), so a
                        // nearly-full pack kills fewer animals rather than slaughtering one it cannot
                        // haul. Eradicate is uncapped (it's driving the herd extinct, not eating).
                        let carry_room_biomass =
                            if !policy.delivers_food() || provisions_per_biomass <= 0.0 {
                                f32::INFINITY
                            } else {
                                (cap - cohort.stores.get(FOOD)).max(scalar_zero()).to_f32()
                                    / provisions_per_biomass
                            };
                        let herd = &mut herds.herds[idx];
                        let body_mass = herd.body_mass;
                        let take = expedition_take_biomass(
                            workers,
                            per_worker_biomass,
                            policy,
                            herd_biomass_before,
                            carrying_capacity,
                            body_mass,
                            &ecology,
                            &fauna,
                            carry_room_biomass,
                            &mut herd.hunt_credit,
                        );
                        // The herd loses every animal killed, carried home or not (slice 8).
                        herd.biomass -= take.killed_biomass();
                        let herd_biomass_after = herd.biomass;
                        if policy.delivers_food() {
                            let carried = cohort.stores.get(FOOD);
                            let room = (cap - carried).max(scalar_zero());
                            let provisions = scalar_from_f32(fauna::hunt_provisions(
                                take.carried,
                                &fauna,
                                EXPEDITION_OUTPUT_MULTIPLIER,
                            ));
                            let added = provisions.min(room);
                            if added > scalar_zero() {
                                cohort.stores.add(FOOD, added);
                            }
                        }

                        // Trip-completion + early-delivery decision (arrived parties only).
                        let carried = cohort.stores.get(FOOD);
                        let full = carried >= cap;
                        let min_deliver = scalar_from_f32(
                            workers as f32
                                * cfg.hunt.per_worker_carry
                                * cfg.hunt.min_deliver_fraction,
                        );
                        let herd_near_band = home_pos
                            .map(|home| {
                                crate::grid_utils::hex_distance_wrapped(
                                    herd_pos,
                                    home,
                                    grid_width,
                                    wrap_horizontal,
                                ) <= cfg.hunt.drop_off_within_tiles
                            })
                            .unwrap_or(false);
                        // Worthwhile-load early delivery: fixes the empty-larder flip-flop bug.
                        let near_band_gate = herd_near_band && carried >= min_deliver;

                        // **The load-bearing completion fix.** A raid is over when the standing surplus
                        // is spent — the herd is within one body of the policy's floor, so no whole
                        // animal is left to raid from standing stock (only the regrowth trickle the raid
                        // deliberately stops at). Without this a Sustain raid that grabs its surplus and
                        // hits K/2 would HANG, taking 0 every turn. Delivering policies only — Eradicate
                        // grinds to extinction via the lost-herd guard, not a floor.
                        let surplus_spent = policy.delivers_food()
                            && (herd_biomass_after
                                - hunt_expedition_floor(
                                    policy,
                                    carrying_capacity,
                                    &ecology,
                                    &fauna,
                                ))
                                < body_mass;

                        // `done` = deliver then fold back + despawn (one trip); `relaunch` = deliver
                        // then resume Hunting. A raid ends when the pack fills, a worthwhile near-band
                        // delivery is possible, OR the standing surplus is spent (Sustain leaves K/2,
                        // Surplus 0.30·K). Market makes repeated FULL-cap trips while the herd still has
                        // surplus (relaunch), but once it is stripped to its floor it comes home for
                        // good (`surplus_spent ⇒ done`) rather than trickle-churning at the floor.
                        let (done, relaunch) = match policy {
                            FollowPolicy::Sustain | FollowPolicy::Surplus => {
                                (full || near_band_gate || surplus_spent, false)
                            }
                            FollowPolicy::Market => (surplus_spent, full || near_band_gate),
                            // Eradicate never delivers — it grinds to extinction (→ lost-herd guard).
                            FollowPolicy::Eradicate => (false, false),
                            // The investment policies are **not an expedition concept**: every
                            // rung-transition is place-bound work a resident band does, and
                            // `send_hunt_expedition` rejects them at launch — so this is unreachable (and
                            // `hunt_expedition_floor` `debug_assert!`s if it ever is reached). It takes
                            // nothing (infinite floor ⇒ zero surplus), so end the trip immediately rather
                            // than loop forever taking zero: the party comes home empty and says so.
                            //
                            // **Listed rather than `is_investment()`-derived, deliberately**: this is an
                            // EXHAUSTIVE match, so a new `FollowPolicy` **fails to compile** here until
                            // someone says how a trip under it ends. A predicate guard would need a
                            // catch-all and would silently hand a new verb this behaviour — trading the
                            // compiler's forcing for the very rot that broke `hunt_expedition_floor`.
                            // Exhaustive matches don't drift; `matches!` lists do.
                            FollowPolicy::Cultivate
                            | FollowPolicy::Sow
                            | FollowPolicy::Tame
                            | FollowPolicy::Corral => (true, false),
                        };

                        if done {
                            // Deliver + fold back via the shared Returning arm (deposits carried food).
                            expedition.phase = ExpeditionPhase::Returning;
                            // Never report a cheerful zero: an empty pack must name its cause.
                            let (message, reason) = if carried > scalar_zero() {
                                (
                                    format!(
                                        "Hunting expedition harvested {} provisions — returning home",
                                        carried.to_i64_whole()
                                    ),
                                    "harvest_complete",
                                )
                            } else if standing_surplus <= 0.0 {
                                (
                                    format!(
                                        "Hunting expedition returning EMPTY — the {} is at its {} floor and has no surplus to raid",
                                        fauna_id,
                                        policy.as_str()
                                    ),
                                    "empty_no_surplus",
                                )
                            } else {
                                (
                                    format!(
                                        "Hunting expedition returning EMPTY — no take was possible from the {}",
                                        fauna_id
                                    ),
                                    "empty_no_take",
                                )
                            };
                            event_log.push(CommandEventEntry::new(
                                current_turn,
                                CommandEventKind::Hunt,
                                faction,
                                message,
                                Some(format!(
                                    "status={} policy={} expedition={}",
                                    reason,
                                    policy.as_str(),
                                    entity.to_bits()
                                )),
                            ));
                            if let Some(home) = home_pos {
                                commands.entity(entity).insert(BandTravel { target: home });
                            }
                        } else if relaunch {
                            expedition.phase = ExpeditionPhase::Delivering;
                            if let Some(home) = home_pos {
                                commands.entity(entity).insert(BandTravel { target: home });
                            }
                        } else {
                            // Keep hunting: chase the herd's live tile.
                            commands
                                .entity(entity)
                                .insert(BandTravel { target: herd_pos });
                        }
                    }
                }
            }
            ExpeditionPhase::Delivering => {
                // Market only: run carried food to the band's live tile; on arrival deposit it and
                // auto-relaunch to Hunting (repeated trips). Sustain/Surplus deliver via Returning.
                if let Some(home) = home_pos {
                    commands.entity(entity).insert(BandTravel { target: home });
                }
                if near_home {
                    let delivered = {
                        let carried = cohort.stores.get(FOOD);
                        cohort.stores.take(FOOD, carried)
                    };
                    if let Ok(mut home) = bands.get_mut(expedition.home_band) {
                        if delivered > scalar_zero() {
                            home.stores.add(FOOD, delivered);
                        }
                    }
                    event_log.push(CommandEventEntry::new(
                        current_turn,
                        CommandEventKind::Hunt,
                        faction,
                        format!(
                            "Hunting expedition dropped off {} provisions",
                            delivered.to_i64_whole()
                        ),
                        Some(format!("status=delivered expedition={}", entity.to_bits())),
                    ));
                    // Auto-relaunch: back to Hunting (retargets the herd next turn).
                    expedition.phase = ExpeditionPhase::Hunting;
                }
            }
        }
    }
}

/// A hunting expedition's take applies **no** productivity multiplier: a detached party is not a
/// band, so it carries no morale/discontent output modifier (unlike the band Hunt arm, which passes
/// `output_multiplier(cohort, ..)`). Named so the forecast and the take can't disagree.
const EXPEDITION_OUTPUT_MULTIPLIER: f32 = 1.0;

// **Retired in slice 7: `TENDED_SOURCE_WORKERS_NEEDED = 1`.** A managed source used to define its
// `SourceYield.workers_needed` as a hardcoded one worker ("maintenance labor — a tending presence, not
// a headcount"), which quietly asserted that **one worker could carry home whatever the land offered**.
// It is the same claim `SourceYieldForecast::tended`'s `per_worker_yield = production` made, and it was
// wrong at both ends: the payout was uncapped by labor, and the "max N useful here" readout said `1` on
// a Field producing ten workers' worth. Every rung now derives it through `workers_needed_for_take`
// against the crew's real throughput — a rich source genuinely needs more hands, and says so.

/// `SourceYield.workers_needed` — the **minimum** assigned workers that would have produced `take`
/// biomass this turn at `per_worker_capacity` biomass/worker (the overstaffing signal; see
/// `SourceYield`). `0` when nothing was taken; otherwise `ceil(take / per_worker_capacity)` clamped
/// into `[1, assigned]`. For forage `per_worker_capacity` is the **effective** per-turn throughput
/// `per_worker_biomass_capacity × seasonal_weight` (mirroring `forage_take`'s worker cap), so a
/// low-season, fully-labor-bound patch is not falsely flagged overstaffed; hunt has no seasonal
/// factor. `per_worker_capacity ≤ 0` (a zero-throughput turn that somehow still took biomass) can't
/// be inverted, so it conservatively reports `assigned` (no overstaffing flagged).
pub(crate) fn workers_needed_for_take(take: f32, per_worker_capacity: f32, assigned: u32) -> u32 {
    if take <= 0.0 {
        return 0;
    }
    if per_worker_capacity <= 0.0 {
        return assigned;
    }
    ((take / per_worker_capacity).ceil() as u32).clamp(1, assigned)
}

/// A hunting expedition's **standing-surplus escapement floor**, by policy — the biomass line a
/// greedy raid grabs the herd *down to* and no further.
///
/// **The expedition is a RAID, not a resident band's throttled skim** (the playtest fix). A resident
/// band takes its policy's per-turn *rate* into a kill-credit bank ([`fauna::hunt_policy_rate`],
/// untouched); a detached party instead **grabs the whole standing surplus above this floor in a
/// burst and comes home**, so more hunters take more animals in fewer-or-equal turns. The floors are
/// ordered so a deeper policy leaves a leaner herd — and that ordering is `FaunaConfig::validate`-pinned
/// (`collapse_fraction < surplus_escapement_fraction < MSY_BIOMASS_FRACTION`):
///
/// | policy | floor | leaves |
/// |---|---|---|
/// | **Sustain** | `MSY_BIOMASS_FRACTION · K` | `K/2` — the sustainable operating point |
/// | **Surplus** | `hunt.surplus_escapement_fraction · K` | `0.30·K` |
/// | **Market** | `ecology.collapse_fraction · K` | `0.15·K` (the Allee brink) |
/// | **Eradicate** | `0` | nothing — the whole stock is surplus |
///
/// The two **investment** policies are **not an expedition concept at all**: `Cultivate`/`Corral` are
/// place-bound work a *resident* band does (prepare a patch, build a pen and then tend it) — a
/// detached party cannot pen a herd and walk home — so `send_hunt_expedition` **rejects** them at
/// launch and this arm is unreachable. It deliberately returns a floor of **`f32::INFINITY`** (⇒ zero
/// standing surplus ⇒ the party takes *nothing*) rather than a real floor: if that launch validation
/// ever regresses, the trip is empty and the hole is loud, instead of a plausible-looking raid hiding
/// it. `debug_assert!` makes a debug build scream; release degrades safely rather than panicking.
fn hunt_expedition_floor(
    policy: FollowPolicy,
    carrying_capacity: f32,
    ecology: &EcologyConfig,
    fauna: &FaunaConfig,
) -> f32 {
    let k = carrying_capacity.max(0.0);
    match policy {
        FollowPolicy::Sustain => k * fauna::MSY_BIOMASS_FRACTION,
        FollowPolicy::Surplus => k * fauna.hunt.surplus_escapement_fraction,
        FollowPolicy::Market => k * ecology.collapse_fraction,
        FollowPolicy::Eradicate => 0.0,
        // Investment / plant-only policies are launch-rejected (send_hunt_expedition + valid_for_hunt).
        // An INFINITE floor means "no surplus to take" — the party takes nothing and the regressed
        // guard is loud, exactly as the retired `0.0` *ceiling* was.
        FollowPolicy::Tame | FollowPolicy::Corral | FollowPolicy::Cultivate | FollowPolicy::Sow => {
            debug_assert!(
                false,
                "non-extractive policy {} reached a hunting expedition — send_hunt_expedition must reject it",
                policy.as_str()
            );
            f32::INFINITY
        }
    }
}

/// **THE** expedition's per-turn take, in *biomass* — the greedy raid (the playtest fix). The
/// `ExpeditionPhase::Hunting` arm, the launch forecast, and its provisions wrapper below all resolve
/// through this one function, so a preview can never quote a different take than the raid.
///
/// **A raid grabs the standing surplus, it does not skim a rate.** Each turn the party takes as much
/// biomass as its throughput allows off the herd's **standing surplus** — the stock above the policy's
/// [`hunt_expedition_floor`] — so *more hunters take more animals in fewer-or-equal turns*, the whole
/// point of the fix (a resident band's throttled `hunt_policy_rate` ceiling was worker-independent, so
/// a second hunter only added pack to fill and made the trip *longer*). When the surplus is spent the
/// herd sits at the floor and the raid comes home (the `hunt_trip_forecast` / `Hunting`-arm completion
/// checks own that); Sustain leaves `K/2`, Surplus `0.30·K`, Market `0.15·K`.
///
/// **The accumulator is what turns sub-body throughput into whole animals + waste.** For a body
/// heavier than one turn's throughput (a boar at 50 vs one hunter's 40) `floor(throughput / body) = 0`
/// would take *nothing forever*; banking the throughput onto the herd's `credit` until it clears a body
/// (the `Herd::hunt_credit` field the resident band also uses — one hunter per herd, so they never
/// collide) makes the party kill one every `body / throughput` turns. It is capped at the standing
/// surplus so the bank never funds biomass below the floor. **It is a whole-animal take**
/// ([`fauna::quantise_animal_take`], slice 8): `carry_room_biomass` bounds the party's collection, so a
/// heavy kill it cannot fully haul is `wasted`, not left standing.
#[allow(clippy::too_many_arguments)] // the ecology, the floor levers and the party's caps are all levers
fn expedition_take_biomass(
    workers: u32,
    per_worker_biomass_capacity: f32,
    policy: FollowPolicy,
    biomass: f32,
    carrying_capacity: f32,
    body_mass: f32,
    ecology: &EcologyConfig,
    fauna: &FaunaConfig,
    carry_room_biomass: f32,
    credit: &mut f32,
) -> AnimalTake {
    // The standing surplus above the policy's floor — everything the raid may take this turn.
    let floor = hunt_expedition_floor(policy, carrying_capacity, ecology, fauna);
    let standing_surplus = (biomass - floor).max(0.0);
    // The party's per-turn grab: its whole throughput, never more than the surplus still standing.
    let throughput = (workers as f32 * per_worker_biomass_capacity).max(0.0);
    let rate = throughput.min(standing_surplus);
    // Bank the throughput; this turn's affordable biomass is `credit + rate`, capped at the surplus (so
    // the bank never funds animals below the floor). A body heavier than `rate` accumulates until
    // affordable; a light-bodied herd clears several bodies at once.
    let ceiling = (*credit + rate).clamp(0.0, standing_surplus);
    // What the party can actually take home this turn: its throughput, bounded by the room left in the
    // pack. Folding the carry room in HERE (not clamping afterwards) is what stops a nearly-full pack
    // killing a whole animal it has no room for.
    let collection = throughput.min(carry_room_biomass.max(0.0));
    let take = fauna::quantise_animal_take(ceiling, collection, body_mass);
    // Drain the bank by what was killed, and cap it at the (pre-kill) surplus so it can never grow
    // unbounded while the herd sits at its floor (surplus < body ⇒ no kill ⇒ credit would otherwise
    // climb every turn). `0 ≤ credit ≤ standing_surplus ≤ biomass` holds by construction.
    *credit = (*credit + rate - take.killed_biomass())
        .max(0.0)
        .min(standing_surplus);
    take
}

/// The **provisions a hunting party actually lands in its larder per turn** at a herd's current state
/// — the real take ([`expedition_take_biomass`] → [`fauna::hunt_provisions`], no output multiplier),
/// ignoring only carry room (which bites solely on the final partial turn, and `ceil()` already
/// accounts for that). `0` for a policy that [`FollowPolicy::delivers_food`] says carries nothing
/// home (Eradicate — denial). This is what the client's pre-launch readout is pinned to
/// (`core_sim/tests/expedition_hunt.rs`).
#[allow(clippy::too_many_arguments)] // the ecology, the floor levers and the labor tier are all levers
pub fn expedition_take_provisions(
    workers: u32,
    policy: FollowPolicy,
    biomass: f32,
    carrying_capacity: f32,
    body_mass: f32,
    ecology: &EcologyConfig,
    fauna: &FaunaConfig,
    labor: &LaborConfig,
) -> f32 {
    if !policy.delivers_food() {
        return 0.0;
    }
    // A single-turn preview starting from an empty bank (this readout is the client's per-turn rate,
    // not a specific banked turn) — the forward-sim `hunt_trip_forecast` is the one pinned to actual.
    let mut credit = 0.0_f32;
    let take = expedition_take_biomass(
        workers,
        labor.hunt.per_worker_biomass_capacity,
        policy,
        biomass,
        carrying_capacity,
        body_mass,
        ecology,
        fauna,
        // Carry room bites only on the final partial turn, and `ceil()` already accounts for it.
        f32::INFINITY,
        &mut credit,
    );
    // Quantized onto the larder's `Scalar` grid, exactly as the real take lands there.
    scalar_from_f32(fauna::hunt_provisions(
        take.carried,
        fauna,
        EXPEDITION_OUTPUT_MULTIPLIER,
    ))
    .to_f32()
}

/// The shared **"take food from a nearby source"** primitive (`docs/plan_exploration_and_sites.md`
/// §2b). Resolves the per-policy escapement ceiling ([`fauna::hunt_policy_ceiling`] — the single
/// source), rounds it to **whole animals** against the party's collection
/// ([`fauna::quantise_animal_take`] — the single quantiser), and **subtracts every animal killed from
/// the herd**. One code path for three callers: the band Hunt labor (`advance_labor_allocation`, which
/// additionally credits trade goods + husbandry from the same take), the hunting expedition, and the
/// scout's opportunistic replenish (`advance_expeditions`, `output_multiplier = 1.0`).
///
/// **Returns the [`AnimalTake`] in *biomass*, not provisions** (slice 8): a take is now three numbers
/// — what was killed, what was carried, what rotted — and only the caller knows what to do with each
/// (the band banks `carried` and reports `wasted` on its income breakdown; trade goods scale off the
/// carried meat). Handing back one pre-converted `Scalar` would have forced every caller to
/// re-derive the other two from `herd.biomass` before/after, which is exactly the "second copy of the
/// model" this function exists to prevent. `output_multiplier` therefore no longer belongs here —
/// callers convert with [`fauna::hunt_provisions`].
///
/// **A resident band's take is NO LONGER reproducible by client-side arithmetic** — and that is the
/// point. It used to be `min(workers × huntPerWorkerProvisions, huntPolicyCeilings[policy]) ×
/// outputMultiplier`, because every term was linear and factored out of the `min`. `floor()` is not
/// linear: the client cannot re-derive a whole-animal take from a ceiling and a per-worker rate, so
/// the sim must **export the answer**. `fauna::hunt_source_yield_preview` (→ `SourceYield`) is that
/// answer, and `core_sim/tests/expedition_hunt.rs` pins it to this function.
#[allow(clippy::too_many_arguments)] // the ecology, the ladder and the caller's caps are all levers
pub fn hunt_take(
    herd: &mut Herd,
    workers: u32,
    policy: FollowPolicy,
    per_worker_biomass_capacity: f32,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    carry_room_biomass: f32,
) -> AnimalTake {
    // **The credit-based take** (slice 8b — the kill-credit accumulator). The policy earns its per-turn
    // `hunt_policy_rate` into the herd's banked `hunt_credit`, and this turn's affordable biomass is
    // `min(credit + rate, biomass)` (Eradicate bypasses the bank and takes the whole stock). Resolved
    // against the herd's OWN ecology + capacity (`herd_ecology` / `herd_capacity` — the single source
    // of the rung → growth-rate mapping), never the raw wild pair. Shared with the pre-commit forecast
    // (`fauna::hunt_forecast`), which reads the same credit + rate, so forecast == actual.
    // Sustain's rate is sized against the **pre-regrowth** biomass (slice 8b — so a below-K/2 herd
    // holds, not leaks); the credit ceiling then clamps to the current stock.
    let rate = fauna::hunt_policy_rate(
        policy,
        herd.biomass_before_regrowth,
        herd_capacity(herd, fauna),
        &herd_ecology(herd, fauna),
        fauna,
        ladder,
    );
    let ceiling = fauna::hunt_credit_ceiling(policy, herd.biomass, herd.hunt_credit, rate);
    // **Whole animals** ([`fauna::quantise_animal_take`], slice 8): the crew kills what the *bank* can
    // afford, bounded by what it can haul but never below one — so a party that cannot carry a whole
    // animal still takes one and wastes the rest, and a bank that cannot yet spare one leaves the herd
    // to keep accumulating.
    //
    // `collection` is the hunting group's throughput, bounded by the biomass the caller can carry home
    // (`carry_room_biomass`); the band Hunt passes `f32::INFINITY` (no carry limit — it eats/banks the
    // whole take). Folding the carry room into the collection rather than clamping afterwards is what
    // keeps a nearly-full party from slaughtering an animal it has no room for.
    let collection =
        (workers as f32 * per_worker_biomass_capacity).min(carry_room_biomass.max(0.0));
    let take = fauna::quantise_animal_take(ceiling, collection, herd.body_mass);
    // **The herd loses every animal KILLED, not merely what was carried** — you cannot un-kill the
    // mammoth you could not haul. That is the waste, and it is `take.wasted`.
    herd.biomass -= take.killed_biomass();
    // **Drain the bank by what was killed** (Eradicate never touched it). `credit + rate` is the
    // pre-kill bank, capped at the pre-kill biomass; killing at most `floor(bank / body)` bodies leaves
    // `bank − killed·body ≥ 0`, and ≤ the post-kill biomass (the cap and the kill both fall by the same
    // `killed·body`). So the invariant `0 ≤ hunt_credit ≤ biomass` holds.
    if !matches!(policy, FollowPolicy::Eradicate) {
        herd.hunt_credit = (herd.hunt_credit + rate - take.killed_biomass()).max(0.0);
    }
    take
}

/// What a hunting party can expect from a herd under a policy, computed **at launch** so the player
/// sees the trip's economics before committing workers (`handle_send_hunt_expedition`), and exported
/// per herd × policy × party size in the snapshot so the outfit UI can show it *before* the commit.
/// Produced by [`hunt_trip_forecast`], a **bounded forward simulation** of the trip.
pub struct HuntTripForecast {
    /// Turns of hunting (once in reach — travel is **not** counted) until the **raid completes**. A
    /// greedy raid ends when the pack fills **OR** the standing surplus is spent (the herd sits at the
    /// policy's floor) **OR** the herd is lost — whichever comes first — so this is *"turns until the
    /// party comes home"*, **not** *"turns until the pack is full"* (a full-herd Sustain raid for a big
    /// party leaves `K/2` with a partial pack, and that is a *successful* short trip). `None` = the raid
    /// never completed within `hunt.forecast_horizon_turns`; the caller distinguishes the honest cases
    /// via the other fields: it **delivers no food** (`delivers_food == false` — Eradicate/denial), the
    /// herd had **no surplus to take** (`animals_taken == 0` — at/below the policy's floor), or it only
    /// trickle-fills off regrowth (a slow breeder a big party can neither fill nor exhaust).
    pub turns_to_fill: Option<u32>,
    /// Does this mission bring food home? `false` for Eradicate ([`FollowPolicy::delivers_food`]).
    pub delivers_food: bool,
    /// Provisions landed on the **first** hunting turn — the trip's opening rate, and (with
    /// `animals_taken`) a "can this herd give me anything at all?" signal.
    pub first_turn_provisions: f32,
    /// **Whole animals the party delivers over the raid** — the real payload the client headlines
    /// ("≈N animals over M turns"). `0` = the herd is at/below the policy's floor and has no surplus to
    /// raid (the honest non-viable case). It is bounded by the standing surplus, so it plateaus with
    /// party size once the surplus, not the pack, is the binding constraint — which is how the client
    /// derives the max-useful party size (`ceil(surplus_food / per_worker_carry)`).
    pub animals_taken: u32,
}

/// One hunter's per-turn **provisions** throughput: their biomass take capacity converted through
/// the same linear biomass→provisions rate the take uses. Worker-scaled (× party size) it is the
/// party's uncapped rate — the other half of the forecast, exported per-cohort in the snapshot
/// (`PopulationCohortState.huntPerWorkerProvisions`).
///
/// **Snapped to the `Scalar` grid** the larder actually accumulates on — `hunt_provisions` quantizes
/// every take through `Scalar::from_f32`, so the honest per-worker constant is the *quantized* one.
/// The raw `f32` product runs a hair low (40 × 0.02 = 3.1999999, not 3.2, once scaled by a
/// 4-worker party), and that sliver is enough to turn an exactly-divisible trip into a phantom extra
/// turn in any `ceil()` downstream — including the client's, which multiplies this constant by the
/// party size. Snapping here keeps the exported constant on the same grid as the sim's reality.
pub fn hunt_per_worker_provisions(labor: &LaborConfig, fauna: &FaunaConfig) -> f32 {
    scalar_from_f32(
        labor.hunt.per_worker_biomass_capacity
            * fauna.hunt.provisions_per_biomass
            * EXPEDITION_OUTPUT_MULTIPLIER,
    )
    .to_f32()
}

/// The first hunting turn: the forecast counts turns *in reach of the herd*, starting at 1 (the turn
/// the party makes its first take). Travel is not counted — see [`hunt_trip_forecast`].
const FIRST_HUNTING_TURN: u32 = 1;

/// Forecast a hunting **raid** by simulating it forward turn by turn against the herd's own ecology,
/// on the sim's arithmetic, until the party comes home — the pack fills, the **standing surplus is
/// spent** (the herd sits at the policy's floor), or the herd is lost — or `hunt.forecast_horizon_turns`
/// is hit. It does **not** divide a carry cap by a rate.
///
/// *Why simulate?* A raid has **no single per-turn rate, and two completion conditions that cross over
/// with party size.** A big party on a full herd grabs a lump of standing stock and leaves with a
/// *partial* pack (surplus < pack); a small party fills its pack before the surplus runs out. Only the
/// simulation gives an honest `turns_to_fill` (now *"turns until the party comes home"*, not *"turns to
/// fill the pack"*) **and** `animals_taken` — the real payload the client headlines.
///
/// There is no second copy of the model to drift: each simulated turn is the *same* pair of calls the
/// live sim makes — [`fauna::regrow_biomass`] (as `advance_herds` does in Logistics) then
/// [`expedition_take_biomass`] (as the `ExpeditionPhase::Hunting` arm does in Population), in that
/// order — and the "surplus spent ⇒ come home" completion mirrors that arm's `done`. **The larder
/// accumulates on the fixed-point `Scalar` grid**, exactly as the real one does (`hunt_provisions`
/// quantizes every take), so an evenly-dividing trip cannot invent a phantom extra turn.
///
/// **Travel is not part of this estimate** — it assumes the party is already in reach and stationary,
/// so the number means "turns spent *hunting* once you arrive." Eradicate delivers no food (denial), so
/// it gets no ETA (`delivers_food = false`). Pinned to a real party run forward through the real systems
/// by `core_sim/tests/expedition_hunt.rs`.
///
/// *(The old O(1) "cannot fill" short-circuit — an upper bound on total provisions vs. the carry cap —
/// was **retired** with the raid: its premise "won't fill the pack ⇒ doomed trip" is exactly inverted
/// by a raid, where "won't fill the pack" is the *normal successful short trip* that exhausts a small
/// surplus. A raid is inherently short — grab the surplus, done — so simulating each one to completion
/// is already cheap.)*
#[allow(clippy::too_many_arguments)] // every config the forward simulation reads is a lever
pub fn hunt_trip_forecast(
    workers: u32,
    herd: &Herd,
    policy: FollowPolicy,
    fauna: &FaunaConfig,
    labor: &LaborConfig,
    expedition: &ExpeditionConfig,
) -> HuntTripForecast {
    let delivers_food = policy.delivers_food();
    let cap = scalar_from_f32(workers as f32 * expedition.hunt.per_worker_carry);
    // Denial carries nothing home, and an empty party has no pack — either way a "turns to fill" number
    // would be a lie (a denial raid still reports `animals_taken == 0`: it delivers none).
    if !delivers_food || cap <= scalar_zero() {
        return HuntTripForecast {
            turns_to_fill: None,
            delivers_food,
            first_turn_provisions: 0.0,
            animals_taken: 0,
        };
    }

    let horizon = expedition.hunt.forecast_horizon_turns;
    let provisions_per_biomass = fauna.hunt.provisions_per_biomass;
    // The forecast runs on a private copy of the herd — the caller's live herd is never touched.
    let mut quarry = herd.clone();
    // The herd's OWN ecology + capacity (resolved once — neither can change under the party's take:
    // the quarry is never tamed or penned mid-trip).
    let ecology = herd_ecology(&quarry, fauna);
    let capacity = herd_capacity(&quarry, fauna);
    let floor = hunt_expedition_floor(policy, capacity, &ecology, fauna);
    let mut larder = scalar_zero();
    let mut first_turn_provisions = 0.0_f32;
    let mut animals_taken = 0u32;

    for turn in 1..=horizon {
        // Logistics: the herd's ecology moves first (regrowth, or the depensation decline), exactly
        // as `advance_herds` runs before the Population stage's take.
        fauna::regrow_biomass(&mut quarry, fauna);
        if quarry.biomass <= ecology.extinction_floor * capacity {
            // `advance_herds` would despawn it here — a lost herd ends the raid.
            break;
        }

        // Population: the `Hunting` arm's greedy take, through the same helper, bounded by the carry
        // room left in the pack (the arm converts the room back into biomass the same way).
        let carry_room_biomass = if provisions_per_biomass <= 0.0 {
            f32::INFINITY
        } else {
            (cap - larder).max(scalar_zero()).to_f32() / provisions_per_biomass
        };
        let take = expedition_take_biomass(
            workers,
            labor.hunt.per_worker_biomass_capacity,
            policy,
            quarry.biomass,
            capacity,
            quarry.body_mass,
            &ecology,
            fauna,
            carry_room_biomass,
            &mut quarry.hunt_credit,
        );
        quarry.biomass -= take.killed_biomass();
        animals_taken += take.killed;

        let provisions = scalar_from_f32(fauna::hunt_provisions(
            take.carried,
            fauna,
            EXPEDITION_OUTPUT_MULTIPLIER,
        ));
        let room = (cap - larder).max(scalar_zero());
        larder += provisions.min(room);
        if turn == FIRST_HUNTING_TURN {
            first_turn_provisions = provisions.to_f32();
        }

        // The raid completes when the pack fills OR the standing surplus is spent — the herd is within
        // one body of its floor, so no whole animal is left to raid from *standing stock* (only the
        // regrowth trickle, which the raid deliberately stops at). Whichever fires, the party comes
        // home; this is the forecast twin of the `ExpeditionPhase::Hunting` arm's `done`.
        let surplus_spent = (quarry.biomass - floor) < quarry.body_mass;
        if larder >= cap || surplus_spent {
            return HuntTripForecast {
                turns_to_fill: Some(turn),
                delivers_food,
                first_turn_provisions,
                animals_taken,
            };
        }
    }

    HuntTripForecast {
        turns_to_fill: None,
        delivers_food,
        first_turn_provisions,
        animals_taken,
    }
}
