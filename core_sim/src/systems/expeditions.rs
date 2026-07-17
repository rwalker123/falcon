use super::*;

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
                    // A scout only nibbles the sustainable surplus off passing game (Sustain
                    // ceiling), not the productive hunt the hunt verb runs. Cap the take at the
                    // biomass the scout can actually top up with (conservation — the herd loses only
                    // what's kept), by inverting `provisions_per_biomass`.
                    let room = (low_buffer - cohort.stores.get(FOOD)).max(scalar_zero());
                    let provisions_per_biomass = fauna.hunt.provisions_per_biomass;
                    let carry_room_biomass = if provisions_per_biomass > 0.0 {
                        room.to_f32() / provisions_per_biomass
                    } else {
                        f32::INFINITY
                    };
                    let provisions = hunt_take(
                        &mut herds.herds[idx],
                        workers,
                        FollowPolicy::Sustain,
                        per_worker_biomass,
                        &fauna,
                        &ladder,
                        1.0,
                        carry_room_biomass,
                    );
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

                        // Productive take: the shared `expedition_take_biomass` — workers ×
                        // per-hunter capacity, capped by the policy's ceiling
                        // (`hunt_expedition_ceiling`: Sustain takes the shared MSY *flow*, the
                        // depleting policies take *stock* headroom down to their floor) and clamped
                        // to the herd. The launch forecast and the exported ceiling resolve through
                        // the SAME helper, so a preview can't quote a different ceiling than this
                        // take. Eradicate carries no food (denial) — it only depletes the herd.
                        let herd_biomass_before = herds.herds[idx].biomass;
                        // Kept for the empty-pack diagnosis below (`<= 0` → the herd yields nothing
                        // under this policy); the take itself goes through the shared helper.
                        let policy_ceiling = hunt_expedition_ceiling(
                            policy,
                            herd_biomass_before,
                            carrying_capacity,
                            &ecology,
                            &fauna,
                            &ladder,
                        );
                        let provisions_per_biomass = fauna.hunt.provisions_per_biomass;
                        // Conservation: a delivering party can only take the biomass it can actually
                        // carry home. Cap the take at the biomass equivalent of the remaining carry
                        // room (invert `provisions_per_biomass`), so the herd loses exactly what the
                        // party keeps — no over-depletion of unhunted biomass. Eradicate is uncapped
                        // (it's driving the herd extinct).
                        let carry_room_biomass =
                            if !policy.delivers_food() || provisions_per_biomass <= 0.0 {
                                f32::INFINITY
                            } else {
                                (cap - cohort.stores.get(FOOD)).max(scalar_zero()).to_f32()
                                    / provisions_per_biomass
                            };
                        let herd = &mut herds.herds[idx];
                        let take_biomass = expedition_take_biomass(
                            workers,
                            per_worker_biomass,
                            policy,
                            herd_biomass_before,
                            carrying_capacity,
                            &ecology,
                            &fauna,
                            &ladder,
                        )
                        .min(carry_room_biomass.max(0.0));
                        herd.biomass -= take_biomass;
                        if policy.delivers_food() {
                            let carried = cohort.stores.get(FOOD);
                            let room = (cap - carried).max(scalar_zero());
                            let provisions = scalar_from_f32(fauna::hunt_provisions(
                                take_biomass,
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

                        // `done` = deliver then fold back + despawn (one trip); `relaunch` = deliver
                        // then resume Hunting (Market's repeated trips). Sustain is a *flow* skim
                        // now, so — like Surplus — it ends on a full pack or a worthwhile near-band
                        // delivery (or a recall / a lost herd), never on a stock line.
                        let (done, relaunch) = match policy {
                            FollowPolicy::Sustain | FollowPolicy::Surplus => {
                                (full || near_band_gate, false)
                            }
                            FollowPolicy::Market => (false, full || near_band_gate),
                            // Eradicate never delivers — it grinds to extinction (→ lost-herd guard).
                            FollowPolicy::Eradicate => (false, false),
                            // The investment policies are **not an expedition concept**: every
                            // rung-transition is place-bound work a resident band does, and
                            // `send_hunt_expedition` rejects them at launch — so this is unreachable (and
                            // `hunt_expedition_ceiling` `debug_assert!`s if it ever is reached). It
                            // takes nothing (ceiling `0.0`), so end the trip immediately rather than
                            // loop forever taking zero: the party comes home empty and says so.
                            //
                            // **Listed rather than `is_investment()`-derived, deliberately**: this is an
                            // EXHAUSTIVE match, so a new `FollowPolicy` **fails to compile** here until
                            // someone says how a trip under it ends. A predicate guard would need a
                            // catch-all and would silently hand a new verb this behaviour — trading the
                            // compiler's forcing for the very rot that broke `hunt_expedition_ceiling`.
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
                            } else if policy_ceiling <= 0.0 {
                                (
                                    format!(
                                        "Hunting expedition returning EMPTY — the {} yields no sustainable take (it is below its collapse threshold)",
                                        fauna_id
                                    ),
                                    "empty_no_sustainable_take",
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

/// A hunting expedition's per-turn **biomass take ceiling**, by policy — the one place the two take
/// models meet, so a policy can never pick up the wrong one.
///
/// **Sustain is a *flow***: it takes the shared MSY ceiling ([`fauna::hunt_policy_ceiling`]) — the
/// same take a resident band's Hunt arm makes from the same herd state, so "Sustain" means one thing
/// across the sim. It is **not** a stock target: the skim equals regrowth, so the herd holds steady
/// and no floor is ever needed (or crossed).
///
/// The **depleting** policies are instead *stock* headroom down to a floor
/// (`docs/plan_exploration_and_sites.md` §2b): **Surplus/Market** stop at the ecology collapse/Allee
/// threshold (`collapse_fraction × carrying_capacity` — draw toward but not below it, so overhunting
/// can't directly trigger the irreversible crash); **Eradicate** has no floor (drives extinction).
///
/// The two **investment** policies are **not an expedition concept at all**: `Cultivate`/`Corral` are
/// place-bound work a *resident* band does (prepare a patch, build a pen and then tend it) — a
/// detached party cannot pen a herd and walk home — so `send_hunt_expedition` **rejects** them at
/// launch and this arm is unreachable. It deliberately yields **`0.0`** rather than quietly falling
/// back to the Sustain flow: if that launch validation ever regresses, the party takes *nothing* and
/// the hole is loud, instead of a plausible-looking Sustain trip hiding it. `debug_assert!` makes a
/// debug build scream; release degrades safely rather than panicking inside the turn loop.
fn hunt_expedition_ceiling(
    policy: FollowPolicy,
    biomass: f32,
    carrying_capacity: f32,
    ecology: &EcologyConfig,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
) -> f32 {
    // **Derived, never re-listed** (`FollowPolicy::is_investment`). The hand-written `matches!` this
    // replaces had rotted: it omitted `Tame`, so a Tame expedition sailed straight past this assert
    // and fell through to `hunt_policy_ceiling`, which handed back a real pastoral-dip ceiling — a
    // *plausible* number hiding the hole this arm exists to make loud. That is the whole reason the
    // grouping has one home now.
    if policy.is_investment() {
        debug_assert!(
            false,
            "investment policy {} reached a hunting expedition — send_hunt_expedition must reject it",
            policy.as_str()
        );
        return 0.0;
    }
    match hunt_expedition_floor(policy, carrying_capacity, ecology) {
        // A flow, not a stock target — defer to the shared per-policy ceiling.
        None => {
            fauna::hunt_policy_ceiling(policy, biomass, carrying_capacity, ecology, fauna, ladder)
        }
        Some(floor) => (biomass - floor).max(0.0),
    }
}

/// The **standing biomass an expedition's take leaves behind** under `policy` — the stock floor the
/// depleting policies draw down toward. `None` for Sustain, whose ceiling is a *flow* (MSY) and
/// therefore has no floor at all. Split out of [`hunt_expedition_ceiling`] so its one caller's twin —
/// the O(1) fill bound ([`hunt_trip_provisions_bound`]) — reads the *same* floor the take obeys
/// instead of re-deriving it.
///
/// The investment policies are launch-rejected and never reach an expedition (see
/// [`hunt_expedition_ceiling`]); they are handled there, before this is called, so that the "cannot
/// be here" decision lives in exactly one place.
fn hunt_expedition_floor(
    policy: FollowPolicy,
    carrying_capacity: f32,
    ecology: &EcologyConfig,
) -> Option<f32> {
    match policy {
        FollowPolicy::Surplus | FollowPolicy::Market => {
            Some(ecology.collapse_fraction * carrying_capacity)
        }
        FollowPolicy::Eradicate => Some(0.0),
        // Sustain is a flow (no floor). The investment rungs are unreachable on an expedition — see
        // `hunt_expedition_ceiling`, which rejects them before this is reached; treating them as
        // "no floor" here keeps the O(1) fill bound conservative (it never under-estimates).
        // Exhaustive, for `hunt_expedition_ceiling`'s reason above: a new verb must fail to compile
        // here rather than inherit "no floor" from a catch-all.
        FollowPolicy::Sustain
        | FollowPolicy::Cultivate
        | FollowPolicy::Sow
        | FollowPolicy::Tame
        | FollowPolicy::Corral => None,
    }
}

/// **THE** expedition's per-turn take, in *biomass*, before carry room: the party's throughput capped
/// by [`hunt_expedition_ceiling`] and clamped to what the herd actually has. The `ExpeditionPhase::
/// Hunting` arm, the launch forecast, and the exported ceiling all resolve through this one function
/// (or its provisions wrappers below), so a preview can never quote a different ceiling than the take
/// — the bug that made a Surplus trip read ~34 turns when it really filled in ~5.
#[allow(clippy::too_many_arguments)] // the ecology, the ladder and the party's caps are all levers
fn expedition_take_biomass(
    workers: u32,
    per_worker_biomass_capacity: f32,
    policy: FollowPolicy,
    biomass: f32,
    carrying_capacity: f32,
    ecology: &EcologyConfig,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
) -> f32 {
    let ceiling =
        hunt_expedition_ceiling(policy, biomass, carrying_capacity, ecology, fauna, ladder);
    (workers as f32 * per_worker_biomass_capacity)
        .min(ceiling)
        .max(0.0)
        .clamp(0.0, biomass.max(0.0))
}

/// The **provisions a hunting party actually lands in its larder per turn** at a herd's current state
/// — the real take ([`expedition_take_biomass`] → [`fauna::hunt_provisions`], no output multiplier),
/// ignoring only carry room (which bites solely on the final partial turn, and `ceil()` already
/// accounts for that). `0` for a policy that [`FollowPolicy::delivers_food`] says carries nothing
/// home (Eradicate — denial). This is what the client's pre-launch readout is pinned to
/// (`core_sim/tests/expedition_hunt.rs`).
#[allow(clippy::too_many_arguments)] // the ecology, the ladder and the labor tier are all levers
pub fn expedition_take_provisions(
    workers: u32,
    policy: FollowPolicy,
    biomass: f32,
    carrying_capacity: f32,
    ecology: &EcologyConfig,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    labor: &LaborConfig,
) -> f32 {
    if !policy.delivers_food() {
        return 0.0;
    }
    let take = expedition_take_biomass(
        workers,
        labor.hunt.per_worker_biomass_capacity,
        policy,
        biomass,
        carrying_capacity,
        ecology,
        fauna,
        ladder,
    );
    // Quantized onto the larder's `Scalar` grid, exactly as the real take lands there.
    scalar_from_f32(fauna::hunt_provisions(
        take,
        fauna,
        EXPEDITION_OUTPUT_MULTIPLIER,
    ))
    .to_f32()
}

/// The shared **"take food from a nearby source"** primitive (`docs/plan_exploration_and_sites.md`
/// §2b). Resolves the per-policy take ceiling ([`fauna::hunt_policy_ceiling`] — the single source),
/// caps it by the hunting group's throughput (`workers × per_worker_biomass_capacity`), clamps to
/// the herd's biomass, **subtracts it from the herd**, and converts the take to provisions
/// ([`fauna::hunt_provisions`], × the caller's productivity `output_multiplier`), returning the
/// provisions taken. One code path for three callers: the band Hunt labor
/// (`advance_labor_allocation`, which additionally credits trade goods + husbandry from the same
/// take — it reads `herd.biomass` before/after for the raw biomass amount), the hunting expedition,
/// and the scout's opportunistic replenish (`advance_expeditions`, `output_multiplier = 1.0`).
///
/// A resident band's take (`carry_room_biomass = f32::INFINITY`) is reproducible from the snapshot
/// alone — `min(workers × huntPerWorkerProvisions, huntPolicyCeilings[policy]) × outputMultiplier` —
/// because the biomass→provisions conversion and the multiplier are linear and factor out of the
/// `min`, and the exported ceiling is biomass-clamped exactly as the take is
/// (the exported `huntPolicyCeilings`, projected from [`fauna::hunt_forecast`]). That is the client's
/// local-hunt yield preview; it is pinned to
/// this function by `core_sim/tests/expedition_hunt.rs`.
#[allow(clippy::too_many_arguments)] // the ecology, the ladder and the caller's caps are all levers
pub fn hunt_take(
    herd: &mut Herd,
    workers: u32,
    policy: FollowPolicy,
    per_worker_biomass_capacity: f32,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    output_multiplier: f32,
    carry_room_biomass: f32,
) -> Scalar {
    // Per-policy ecology ceiling — THE single source ([`fauna::hunt_policy_ceiling`]): Sustain = the
    // MSY flow (a collapsing group gives nothing), Surplus = that × multiplier, Market = a commercial
    // share, Eradicate = max take, Corral = the `animal:pen` rung's `yield_fraction_while_building ×
    // MSY` investment dip while the pen is built. Shared with the pre-commit forecast (`fauna::hunt_forecast`) and the
    // expedition, so no two hunters of the same herd can disagree about what a policy means.
    // The ceiling is resolved against the herd's OWN ecology + capacity (`herd_ecology` /
    // `herd_capacity` — the single source of the husbandry ladder's rung → growth-rate mapping), never
    // the raw wild pair: hunting a *tamed* herd draws on the pastoral curve, and a penned one on the
    // pen's.
    let policy_ceiling = hunt_policy_ceiling(
        policy,
        herd.biomass,
        herd_capacity(herd, fauna),
        &herd_ecology(herd, fauna),
        fauna,
        ladder,
    );
    // The hunting group's throughput caps the take; below the Sustain ceiling the herd nets growth.
    // `carry_room_biomass` additionally caps the take at the biomass the caller can carry home
    // (conservation — the herd loses only what's kept); the band Hunt passes `f32::INFINITY`
    // (no carry limit — it eats/banks the whole take, behaviour unchanged).
    let worker_cap = workers as f32 * per_worker_biomass_capacity;
    let take = worker_cap
        .min(policy_ceiling)
        .max(0.0)
        .clamp(0.0, herd.biomass)
        .min(carry_room_biomass.max(0.0));
    herd.biomass -= take;
    // FOOD income is fully fractional (a few hunters may yield < 1/turn); the larder accumulates on
    // the fixed-point `Scalar` grid, so quantize the shared conversion here.
    scalar_from_f32(hunt_provisions(take, fauna, output_multiplier))
}

/// What a hunting party can expect from a herd under a policy, computed **at launch** so the player
/// sees the trip's economics before committing workers (`handle_send_hunt_expedition`), and exported
/// per herd × policy × party size in the snapshot so the outfit UI can show it *before* the commit.
/// Produced by [`hunt_trip_forecast`], a **bounded forward simulation** of the trip.
pub struct HuntTripForecast {
    /// Turns of hunting (once in reach — travel is **not** counted) before the party's carry cap is
    /// full. `None` = it does not fill within `hunt.forecast_horizon_turns`, which covers three
    /// honestly-different cases the caller distinguishes via the other two fields: the mission
    /// **delivers no food at all** (`delivers_food == false` — Eradicate/denial), the herd yields
    /// **nothing** under this policy (`first_turn_provisions == 0` — a collapsing sub-Allee herd),
    /// or the trip is simply *too long to be worth a number* (a small herd's regrowth trickle).
    pub turns_to_fill: Option<u32>,
    /// Does this mission bring food home? `false` for Eradicate ([`FollowPolicy::delivers_food`]).
    pub delivers_food: bool,
    /// Provisions landed on the **first** hunting turn — the trip's opening rate, and the "can this
    /// herd give me anything at all?" signal (`0` = no take is possible under this policy). It is
    /// deliberately *not* a whole-trip rate: under Surplus/Market on a small herd the party strips
    /// the stock headroom in a turn or two and then crawls at the regrowth trickle, so no single
    /// per-turn number describes the trip — which is exactly why the forecast simulates.
    pub first_turn_provisions: f32,
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

/// Half a `Scalar` tick — the most `Scalar::from_f32`'s round-to-nearest can add to a single take
/// (`fauna::hunt_provisions` quantizes each one). The simulated larder can therefore run up to this
/// much *above* its own real-arithmetic value on every simulated turn, so an upper bound on the trip
/// must carry `horizon ×` it or it would not be an upper bound on what the simulation counts.
const SCALAR_ROUNDING_SLACK: f64 = 0.5 / Scalar::SCALE as f64;

/// Relative cushion baked into [`hunt_trip_provisions_bound`], covering the float rounding between
/// the bound's `f64` arithmetic and the simulation's `f32` arithmetic. It is **load-bearing, not
/// paranoia**: the sim converts a take with the `f32` constant `provisions_per_biomass` (`0.02`
/// stored as `0.019999999552965164`) and lands *exactly* on a carry cap that the same product in
/// `f64` reads a hair *below* — an un-cushioned bound would then reject a trip that fills on turn 1
/// (caught by `hunt_trip_bound_tests`). It costs nothing in rejection power: a trip the bound really
/// rejects misses its cap by *whole provisions*, never by one part in ten thousand.
const CANNOT_FILL_BOUND_MARGIN: f64 = 1e-4;

/// An **O(1) upper bound** on the provisions a party of `workers` could land from `herd` under
/// `policy` across the *entire* forecast horizon — the short-circuit that lets [`hunt_trip_forecast`]
/// answer "this pack cannot possibly fill" without simulating the horizon turn by turn (the
/// overwhelmingly common case: a rabbit warren cannot fill an 8-hunter pack under any policy, and
/// proving that by simulation costs 60 wasted steps per party size).
///
/// It bounds the same two limits the simulated take obeys, and reuses the sim's own helpers rather
/// than re-deriving the ecology:
/// - **Throughput** — the party can never carry off more than `horizon × workers ×
///   per_worker_biomass_capacity`, whatever the herd holds.
/// - **Ecology** — for **Sustain** the ceiling is a per-turn *flow* ([`fauna::hunt_policy_ceiling`],
///   regrowth at K/2 or below), so at most `horizon × peak_regrowth`. For the **depleting** policies
///   it is *stock* headroom down to [`hunt_expedition_floor`], so by conservation of biomass
///   everything the party can ever remove is that standing headroom plus every turn's regrowth —
///   each at most [`fauna::peak_regrowth`], the logistic curve's maximum.
///
/// Both terms over-estimate by construction (a herd's real regrowth is below the peak, its real
/// Sustain ceiling below the peak flow), and the result carries [`CANNOT_FILL_BOUND_MARGIN`] +
/// `horizon ×` [`SCALAR_ROUNDING_SLACK`] on top for the arithmetic the *simulation* rounds its own
/// way, so the bound is **never** an under-estimate. It may say "might fill" about a trip that will
/// not (the simulation then settles it); it may never say "cannot fill" about a trip that would.
/// Pinned exhaustively against the unabridged simulation — shipped levers *and* off-nominal ones —
/// by `hunt_trip_bound_tests`.
fn hunt_trip_provisions_bound(
    workers: u32,
    herd: &Herd,
    policy: FollowPolicy,
    fauna: &FaunaConfig,
    labor: &LaborConfig,
    expedition: &ExpeditionConfig,
) -> f64 {
    let horizon = expedition.hunt.forecast_horizon_turns as f64;
    // The herd's OWN ecology + capacity — a tamed/penned herd regrows far faster, so a bound taken
    // against the wild curve would under-estimate it and could reject a trip that WOULD have filled.
    let ecology = herd_ecology(herd, fauna);
    let capacity = herd_capacity(herd, fauna);
    let peak_regrowth = fauna::peak_regrowth(capacity, &ecology).max(0.0) as f64;
    let throughput =
        horizon * workers as f64 * labor.hunt.per_worker_biomass_capacity.max(0.0) as f64;
    let ecology_bound = match hunt_expedition_floor(policy, capacity, &ecology) {
        None => horizon * peak_regrowth,
        Some(floor) => (herd.biomass - floor).max(0.0) as f64 + horizon * peak_regrowth,
    };
    let provisions = throughput.min(ecology_bound)
        * fauna.hunt.provisions_per_biomass.max(0.0) as f64
        * EXPEDITION_OUTPUT_MULTIPLIER as f64;
    provisions * (1.0 + CANNOT_FILL_BOUND_MARGIN) + horizon * SCALAR_ROUNDING_SLACK
}

/// Forecast a hunting trip by **simulating it** — running the party's take forward turn by turn
/// against the herd's own ecology, on the sim's arithmetic, until the pack is full or
/// `hunt.forecast_horizon_turns` is hit. It does **not** divide a carry cap by a rate.
///
/// *Why not the closed form?* Because there is no single rate. The old forecast divided the carry cap
/// by one per-policy number, which is exact only when that number is a genuine per-turn **flow**
/// (Sustain's MSY) or when the party is throughput-bound for the whole trip (Surplus/Market on a big
/// herd). Under **Surplus/Market on a small herd it is a total *stock***: the party strips the
/// headroom down to the collapse floor in a turn or two and then crawls at the herd's regrowth
/// trickle. Dividing the cap by that stock read a **4-worker party on a full Rabbit Warren (K = 200)
/// under Surplus as a ~5-turn trip**; the simulation says that party **never fills** within the
/// 60-turn horizon (only a *1-worker* party fills, in **23 turns** — a quarter the pack, so the
/// regrowth trickle can still reach it).
/// Simulating collapses both regimes into one honest answer, and there is no second copy
/// of the model to drift: each simulated turn is the *same* pair of calls the live sim makes —
/// [`fauna::regrow_biomass`] (as `advance_herds` does in Logistics) then [`expedition_take_biomass`]
/// (as the `ExpeditionPhase::Hunting` arm does in Population), in that order.
///
/// **The larder accumulates on the fixed-point `Scalar` grid**, exactly as the real one does
/// (`hunt_provisions` quantizes every take): counting in `f32` instead is what once invented a
/// phantom extra turn on an evenly-dividing trip (a 4-hunter Surplus pack is `16 / 3.2` = exactly 5
/// turns, but the unquantized rate 3.1999999 made `ceil()` read 5.0000005 → **6**).
///
/// **Travel is not part of this estimate.** It assumes the party is already in reach of the herd and
/// stationary, so the number means "turns spent *hunting* once you arrive" — the herd's position is
/// never advanced. Eradicate delivers no food at all, so it gets no ETA (`delivers_food = false`).
/// Pinned to a real party run forward through the real systems by `core_sim/tests/expedition_hunt.rs`.
#[allow(clippy::too_many_arguments)] // every config the forward simulation reads is a lever
pub fn hunt_trip_forecast(
    workers: u32,
    herd: &Herd,
    policy: FollowPolicy,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    labor: &LaborConfig,
    expedition: &ExpeditionConfig,
) -> HuntTripForecast {
    let full_horizon = expedition.hunt.forecast_horizon_turns;
    let cap = scalar_from_f32(workers as f32 * expedition.hunt.per_worker_carry);
    // O(1) rejection. Most (herd, policy, party size) triples in the exported estimate table cannot
    // fill their pack *at all* — small game under every policy, Sustain on most herds — and proving
    // that by simulation costs the whole horizon, every time. [`hunt_trip_provisions_bound`] is a
    // true upper bound on the trip's total landing, so a bound below the carry cap settles the
    // question in constant time. The trip is still simulated for its **first** turn: even a rejected
    // trip reports its opening rate (`first_turn_provisions`), and that is one step, not sixty.
    let cannot_fill = policy.delivers_food()
        && cap > scalar_zero()
        && hunt_trip_provisions_bound(workers, herd, policy, fauna, labor, expedition)
            < cap.to_f32() as f64;
    let horizon = if cannot_fill {
        FIRST_HUNTING_TURN.min(full_horizon)
    } else {
        full_horizon
    };
    simulate_hunt_trip(
        workers, herd, policy, fauna, ladder, labor, expedition, horizon,
    )
}

/// The forecast's forward simulation, over an explicit `horizon` — [`hunt_trip_forecast`]'s body,
/// split out so the O(1) short-circuit can run it for a single turn *and* so the bound's safety can
/// be pinned against the **unabridged** run (`hunt_trip_bound_tests`). Everything the doc comment on
/// `hunt_trip_forecast` promises lives here.
#[allow(clippy::too_many_arguments)] // every config the forward simulation reads is a lever
fn simulate_hunt_trip(
    workers: u32,
    herd: &Herd,
    policy: FollowPolicy,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    labor: &LaborConfig,
    expedition: &ExpeditionConfig,
    horizon: u32,
) -> HuntTripForecast {
    let delivers_food = policy.delivers_food();
    let cap = scalar_from_f32(workers as f32 * expedition.hunt.per_worker_carry);
    // Denial carries nothing home, and an empty party has no pack to fill — either way a
    // "turns to fill" number would be a lie.
    if !delivers_food || cap <= scalar_zero() {
        return HuntTripForecast {
            turns_to_fill: None,
            delivers_food,
            first_turn_provisions: 0.0,
        };
    }

    let provisions_per_biomass = fauna.hunt.provisions_per_biomass;
    // The forecast runs on a private copy of the herd — the caller's live herd is never touched.
    let mut quarry = herd.clone();
    // The herd's OWN ecology + capacity (resolved once — neither can change under the party's take:
    // the quarry is never tamed or penned mid-trip).
    let ecology = herd_ecology(&quarry, fauna);
    let capacity = herd_capacity(&quarry, fauna);
    let mut larder = scalar_zero();
    let mut first_turn_provisions = 0.0_f32;

    for turn in 1..=horizon {
        // Logistics: the herd's ecology moves first (regrowth, or the depensation decline), exactly
        // as `advance_herds` runs before the Population stage's take.
        fauna::regrow_biomass(&mut quarry, fauna);
        if quarry.biomass <= ecology.extinction_floor * capacity {
            // `advance_herds` would despawn it here — a lost herd, so the party never fills.
            break;
        }

        // Population: the `Hunting` arm's take, through the same helper, capped by the carry room
        // left in the pack (the arm converts the room back into biomass the same way).
        let carry_room_biomass = if provisions_per_biomass <= 0.0 {
            f32::INFINITY
        } else {
            (cap - larder).max(scalar_zero()).to_f32() / provisions_per_biomass
        };
        let take_biomass = expedition_take_biomass(
            workers,
            labor.hunt.per_worker_biomass_capacity,
            policy,
            quarry.biomass,
            capacity,
            &ecology,
            fauna,
            ladder,
        )
        .min(carry_room_biomass.max(0.0));
        quarry.biomass -= take_biomass;

        let provisions = scalar_from_f32(fauna::hunt_provisions(
            take_biomass,
            fauna,
            EXPEDITION_OUTPUT_MULTIPLIER,
        ));
        let room = (cap - larder).max(scalar_zero());
        larder += provisions.min(room);
        if turn == FIRST_HUNTING_TURN {
            first_turn_provisions = provisions.to_f32();
        }
        if larder >= cap {
            return HuntTripForecast {
                turns_to_fill: Some(turn),
                delivers_food,
                first_turn_provisions,
            };
        }
    }

    HuntTripForecast {
        turns_to_fill: None,
        delivers_food,
        first_turn_provisions,
    }
}

#[cfg(test)]
mod hunt_trip_bound_tests {
    //! The O(1) "this party cannot possibly fill its pack" short-circuit
    //! ([`hunt_trip_provisions_bound`]) must be a **true upper bound**: it may decline to reject a
    //! doomed trip (the simulation then settles it), but it may **never** reject a trip that would
    //! actually have filled. These sweeps pin the short-circuited [`hunt_trip_forecast`] against the
    //! unabridged [`simulate_hunt_trip`] over the whole regime space — every policy, every legal
    //! party size, herds from nearly-extinct to at-capacity, wild and domesticated, across the
    //! shipped levers *and* off-nominal ones (the configs are hot-reloadable, so the bound has to
    //! hold for values we don't ship).

    use super::*;
    use crate::fauna_config::{FaunaConfig, SizeClass};
    use crate::labor_config::LaborConfig;
    use bevy::math::UVec2;

    /// Carrying capacities spanning the shipped species table (rabbit 200 → mammoth 12 000) plus a
    /// degenerate empty herd.
    const CAPS: [f32; 6] = [0.0, 200.0, 600.0, 1_200.0, 9_000.0, 12_000.0];
    /// Biomass as a fraction of capacity: straddles the Allee threshold (0.15), the MSY point (0.5)
    /// and capacity, and includes a herd already under the extinction floor (0.02).
    const BIOMASS_FRACTIONS: [f32; 10] = [0.0, 0.01, 0.02, 0.1, 0.149, 0.15, 0.16, 0.5, 0.9, 1.0];
    /// Party sizes: 0 (no pack) through one past `max_party_size`.
    const WORKER_SWEEP: std::ops::RangeInclusive<u32> = 0..=9;
    /// The sweep's wild herds breed at the builtin wild rate (`fauna.ecology.regrowth_rate`), so the
    /// sweep's `sustainable_yield(&fauna.ecology)` expectations match the herd's per-species curve.
    const WILD_SWEEP_REGROWTH_RATE: f32 = 0.05;

    fn herd(cap: f32, biomass: f32, domesticated: bool) -> Herd {
        let mut herd = Herd::new(
            "game_sweep".to_string(),
            "Sweep Beast".to_string(),
            SizeClass::Small,
            vec![UVec2::new(1, 1)],
            biomass,
            cap,
            0.0,
            WILD_SWEEP_REGROWTH_RATE,
        );
        if domesticated {
            herd.domestication_progress = 1.0;
        }
        herd
    }

    /// Every (config, herd, policy, party size) the sweeps visit, checked two ways: the forecast the
    /// sim actually ships must be **identical** to the unabridged full-horizon simulation, and the
    /// bound must never have rejected a trip that fills. Returns how many trips the bound rejected,
    /// so a caller can assert the sweep is not vacuous.
    fn assert_short_circuit_matches_full_simulation(
        fauna: &FaunaConfig,
        labor: &LaborConfig,
        expedition: &ExpeditionConfig,
        ladder: &LadderConfig,
    ) -> u32 {
        let mut rejected = 0;
        for domesticated in [false, true] {
            for cap in CAPS {
                for fraction in BIOMASS_FRACTIONS {
                    let herd = herd(cap, cap * fraction, domesticated);
                    // Expeditions carry only the extractive rungs (the investment policies
                    // are launch-rejected), so the bound is only ever asked about these.
                    for &policy in FollowPolicy::EXTRACTIVE.iter() {
                        for workers in WORKER_SWEEP {
                            let bounded = hunt_trip_forecast(
                                workers, &herd, policy, fauna, ladder, labor, expedition,
                            );
                            let full = simulate_hunt_trip(
                                workers,
                                &herd,
                                policy,
                                fauna,
                                ladder,
                                labor,
                                expedition,
                                expedition.hunt.forecast_horizon_turns,
                            );
                            let case = format!(
                                "cap={cap} biomass={} domesticated={domesticated} \
                                 policy={} workers={workers}",
                                cap * fraction,
                                policy.as_str()
                            );
                            assert_eq!(
                                bounded.turns_to_fill, full.turns_to_fill,
                                "short-circuit changed turns_to_fill ({case})"
                            );
                            assert_eq!(
                                bounded.delivers_food, full.delivers_food,
                                "short-circuit changed delivers_food ({case})"
                            );
                            assert_eq!(
                                bounded.first_turn_provisions, full.first_turn_provisions,
                                "short-circuit changed first_turn_provisions ({case})"
                            );

                            let cap_provisions =
                                scalar_from_f32(workers as f32 * expedition.hunt.per_worker_carry);
                            let bound = hunt_trip_provisions_bound(
                                workers, &herd, policy, fauna, labor, expedition,
                            );
                            if policy.delivers_food()
                                && cap_provisions > scalar_zero()
                                && bound < cap_provisions.to_f32() as f64
                            {
                                rejected += 1;
                                assert!(
                                    full.turns_to_fill.is_none(),
                                    "bound rejected a trip that actually fills in {:?} turns ({case})",
                                    full.turns_to_fill
                                );
                            }
                        }
                    }
                }
            }
        }
        rejected
    }

    /// **The regression this whole seam exists for.** `hunt_expedition_ceiling`'s hand-written
    /// `matches!` omitted `Tame`, so a Tame expedition sailed *past* the `debug_assert!` meant to
    /// catch it and fell through to `hunt_policy_ceiling`, which handed back a real pastoral-dip
    /// ceiling — a plausible-looking number hiding the hole. Now that the guard derives from
    /// `FollowPolicy::is_investment`, Tame trips it like every other rung-transition verb.
    ///
    /// Asserted as a **panic** because that is the contract: a debug build must scream (release
    /// degrades to `0.0` rather than panicking inside the turn loop). Before the fix this call
    /// returned a positive ceiling and did not panic at all.
    #[test]
    #[should_panic(expected = "investment policy tame reached a hunting expedition")]
    fn a_tame_expedition_trips_the_investment_guard_like_its_siblings() {
        let fauna = FaunaConfig::builtin();
        let ecology = fauna.ecology;
        let _ = hunt_expedition_ceiling(
            FollowPolicy::Tame,
            500.0,
            1000.0,
            &ecology,
            &fauna,
            &LadderConfig::builtin(),
        );
    }

    /// Every investment verb reaches the guard, derived from the grouping rather than re-listed — so
    /// a future rung-transition verb is covered here the day it exists.
    #[test]
    fn every_investment_policy_is_refused_a_hunting_expedition_ceiling() {
        for policy in [
            FollowPolicy::Cultivate,
            FollowPolicy::Sow,
            FollowPolicy::Tame,
            FollowPolicy::Corral,
        ] {
            assert!(
                policy.is_investment(),
                "{policy:?} must read as an investment rung — that predicate IS the guard"
            );
        }
        // ...and no extractive policy is caught by it (the guard must not swallow a real trip).
        for policy in FollowPolicy::EXTRACTIVE {
            assert!(!policy.is_investment());
        }
    }

    #[test]
    fn bound_never_rejects_a_trip_that_actually_fills_on_shipped_levers() {
        let fauna = FaunaConfig::builtin();
        let labor = LaborConfig::builtin();
        let expedition = ExpeditionConfig::builtin();
        let rejected = assert_short_circuit_matches_full_simulation(
            &fauna,
            &labor,
            &expedition,
            &LadderConfig::builtin(),
        );
        // The sweep would be vacuous if the bound never fired — the whole point is that it rejects
        // the bulk of the exported table (small game under every policy, Sustain on most herds).
        assert!(
            rejected > 100,
            "the bound short-circuited only {rejected} trips — the sweep proves nothing"
        );
    }

    #[test]
    fn bound_never_rejects_a_trip_that_actually_fills_on_off_nominal_levers() {
        let base_fauna = FaunaConfig::builtin();
        let base_labor = LaborConfig::builtin();
        let base_expedition = ExpeditionConfig::builtin();
        // Levers the configs expose and a player/designer could hot-reload: a far more productive
        // ecology, a herd that can be stripped to nothing (no collapse floor), a punishing pack,
        // and a hunter throughput at both extremes. Each pushes a different term of the bound.
        for regrowth_rate in [0.0_f32, 0.05, 0.6] {
            for collapse_fraction in [0.0_f32, 0.15, 0.5] {
                for per_worker_carry in [0.5_f32, 4.0, 40.0] {
                    for per_worker_biomass_capacity in [0.0_f32, 40.0, 4_000.0] {
                        let mut fauna = (*base_fauna).clone();
                        fauna.ecology.regrowth_rate = regrowth_rate;
                        fauna.ecology.collapse_fraction = collapse_fraction;
                        let mut labor = (*base_labor).clone();
                        labor.hunt.per_worker_biomass_capacity = per_worker_biomass_capacity;
                        let mut expedition = (*base_expedition).clone();
                        expedition.hunt.per_worker_carry = per_worker_carry;
                        assert_short_circuit_matches_full_simulation(
                            &fauna,
                            &labor,
                            &expedition,
                            &LadderConfig::builtin(),
                        );
                    }
                }
            }
        }
    }
}
