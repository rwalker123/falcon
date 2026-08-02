use super::*;
use crate::fauna::AnimalTake;

/// The config handles [`advance_expeditions`] reads, bundled into one `SystemParam` so the system
/// stays under Bevy's 16-parameter ceiling once the combat + creatures handles join it (Predators
/// Phase 0 — the expedition-hunt danger adapter).
#[derive(bevy::ecs::system::SystemParam)]
pub struct ExpeditionConfigs<'w> {
    pub expedition: Res<'w, crate::expedition_config::ExpeditionConfigHandle>,
    pub visibility: Res<'w, crate::visibility_config::VisibilityConfigHandle>,
    pub fauna: Res<'w, FaunaConfigHandle>,
    pub labor: Res<'w, LaborConfigHandle>,
    pub ladder: Res<'w, LadderConfigHandle>,
    pub combat: Res<'w, CombatConfigHandle>,
    pub creatures: Res<'w, CreaturesConfigHandle>,
}

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
/// - **Both halves of a kill's [`HuntYield`] come home** (#337) — the provisions into the party's
///   larder, the trade goods onto `Expedition::carried_trade` and into the **home band's** store at
///   the next drop-off/fold-back. See [`settle_carried_trade`].
/// - **Phase transitions**: `Outbound` + arrived (no `BandTravel`) → `AwaitingOrders` + a one-shot
///   arrival feed line; `Returning` → chase the home band's live tile and, once within comm range,
///   fold workers + leftover provisions back into the band and despawn (fold-back happens after the
///   flush so the final findings report); `AwaitingOrders` waits (relaunched by `move_band`).
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_expeditions(
    mut commands: Commands,
    configs: ExpeditionConfigs,
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
    let cfg = configs.expedition.get();
    let fauna = configs.fauna.get();
    let labor = configs.labor.get();
    let ladder = configs.ladder.get();
    let vis_cfg = configs.visibility.0.as_ref();
    // **Predators Phase 0 — the expedition-hunt danger seam** (`docs/plan_predators.md`). A hunting
    // party takes casualties like a resident band, but **bloodier**: far from home and unsupported, so
    // the same beast costs it more. The resolver tuning is scaled by `expedition_danger_multiplier`;
    // the base human's intrinsic profile is the same `person` the resident band fields. Resolved once.
    let combat_config = configs.combat.get();
    let mut combat_tuning = combat_config.tuning();
    combat_tuning.lethality *= combat_config.expedition_danger_multiplier;
    let person_profile = configs.creatures.get().person();
    let map_seed = sim_config.map_seed;
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
                    // The **species'** food rate, not the global one: an inedible quarry never fills
                    // the pack, so the room converts to an unbounded biomass collection.
                    let scout_yield = herd_hunt_yield(&herds.herds[idx], &fauna);
                    let carry_room_biomass = if scout_yield.edible() {
                        room.to_f32() / scout_yield.provisions_per_biomass
                    } else {
                        f32::INFINITY
                    };
                    let take = hunt_take(
                        &mut herds.herds[idx],
                        workers,
                        // A scout's roadside kill is a **restrained** one: it stops at the food peak,
                        // the same floor a fresh assignment gets, so replenishing on the march can
                        // never be the thing that ruins a herd.
                        DEFAULT_ESCAPEMENT_FLOOR,
                        NO_IMPROVEMENT_UNDERWAY,
                        per_worker_biomass,
                        &fauna,
                        &ladder,
                        carry_room_biomass,
                    );
                    // **One conversion, both products** — a roadside kill is skinned as well as
                    // butchered (#337). The food tops the pack up to `room`; the hides ride home on
                    // `carried_trade` like the hunt party's, so an opportunistic take on an
                    // *inedible* herd is no longer a pure waste of animals.
                    let landed = scout_yield.apply(take.carried, EXPEDITION_OUTPUT_MULTIPLIER);
                    let provisions = scalar_from_f32(landed.provisions);
                    let added = provisions.min(room);
                    if added > scalar_zero() {
                        cohort.stores.add(FOOD, added);
                    }
                    expedition.carried_trade += landed.trade_goods;
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
                    // **The other half of the haul settles into the SAME store as the meat.** Trade
                    // goods are band-local (see [`TRADE_GOODS`]), so the pelts land in the home
                    // band's larder alongside the provisions — the last chance before the party
                    // despawns and the bank goes with it. No home band left to receive them means
                    // the haul is simply lost, exactly as the carried food is.
                    let mut banked_trade = scalar_zero();
                    if let Ok(mut home) = bands.get_mut(expedition.home_band) {
                        home.working += cohort.working;
                        let leftover = cohort.stores.get(FOOD);
                        if leftover > scalar_zero() {
                            home.stores.add(FOOD, leftover);
                        }
                        banked_trade = settle_carried_trade(&mut expedition, &mut home);
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
                        Some(format!(
                            "status=returned trade_goods={:.2} expedition={}",
                            banked_trade.to_f32(),
                            entity.to_bits()
                        )),
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
                if let ExpeditionMission::Hunt { fauna_id, floor } = &mission {
                    if let Some(idx) = herds.herds.iter().position(|herd| herd.id == *fauna_id) {
                        let floor = *floor;
                        let herd_pos = herds.herds[idx].position();
                        // The herd's OWN capacity — the single source of the husbandry ladder's
                        // rung → `K` mapping (`herd_capacity`); a party hunting a tamed or penned herd
                        // raids *its* stock, not a wild counterfactual's.
                        let carrying_capacity = herd_capacity(&herds.herds[idx], &fauna);
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
                        let standing_surplus =
                            (herd_biomass_before - floor * carrying_capacity.max(0.0)).max(0.0);
                        let quarry_yield = herd_hunt_yield(&herds.herds[idx], &fauna);
                        // A party carrying food home can only take the biomass it has room for. The
                        // room bounds the party's **collection** (invert the species' own
                        // `provisions_per_biomass`), so a nearly-full pack kills fewer animals rather
                        // than slaughtering one it cannot haul.
                        //
                        // Two INDEPENDENT reasons the cap does not bite, and they are different kinds
                        // of thing — keep them apart:
                        // - an **inedible** species (a wolf) never fills a *food* pack, so there is no
                        //   room to run out of. That is a **product** fact.
                        // - **Eradicate** ignores the pack entirely: driving the herd extinct is the
                        //   point and the meat is incidental. That is an **intensity** fact, and it is
                        //   deliberately NOT expressed as "denial delivers nothing" — since #337 an
                        //   Eradicate raid banks the windfall it can carry.
                        let carry_room_biomass = if floor <= STRIP_IT_BARE || !quarry_yield.edible()
                        {
                            f32::INFINITY
                        } else {
                            (cap - cohort.stores.get(FOOD)).max(scalar_zero()).to_f32()
                                / quarry_yield.provisions_per_biomass
                        };
                        let herd = &mut herds.herds[idx];
                        let body_mass = herd.body_mass;
                        let take = expedition_take_biomass(
                            workers,
                            per_worker_biomass,
                            floor,
                            herd_biomass_before,
                            carrying_capacity,
                            body_mass,
                            carry_room_biomass,
                            &mut herd.hunt_credit,
                        );
                        // The herd loses every animal killed, carried home or not (slice 8).
                        herd.biomass -= take.killed_biomass();
                        let herd_biomass_after = herd.biomass;
                        // **Predators Phase 0 — the hunt turns dangerous, bloodier for a detached
                        // party** (`docs/plan_predators.md`). After the take, a herd whose species can
                        // fight back (`combat.attack > 0` — mammoth, ox) turns on the party. The
                        // expedition answers with its OWN hunters (bare-hands `person` today), at the
                        // expedition-scaled lethality, and applies **only the band side's** casualties
                        // (the take already removed the animal's biomass). Fires only on an engagement
                        // turn (inside the `in_reach` guard).
                        if let Some(species) = fauna.species_by_display(&herd.species) {
                            // Danger = strength × BEHAVIOUR: the beast fights back at `attack × ferocity`
                            // (a fleeing animal barely scratches the party). See the labor.rs adapter.
                            let effective_attack = species.combat.attack * species.ferocity;
                            if effective_attack > 0.0 {
                                let animal_profile = CombatStats {
                                    attack: effective_attack,
                                    ..species.combat
                                };
                                let mut hasher = crate::hashing::FnvHasher::new();
                                std::hash::Hash::hash(&herd.id, &mut hasher);
                                let seed =
                                    map_seed ^ current_turn ^ std::hash::Hasher::finish(&hasher);
                                let payload = FightPayload {
                                    sides: vec![
                                        Force {
                                            id: ForceId(0),
                                            posture: Posture::Aggressor,
                                            contingents: vec![Contingent {
                                                kind: ContingentId::from("person"),
                                                count: workers as f32,
                                                profile: person_profile,
                                            }],
                                        },
                                        Force {
                                            id: ForceId(1),
                                            posture: Posture::Defender,
                                            contingents: vec![Contingent {
                                                kind: ContingentId(herd.species.clone()),
                                                count: 1.0,
                                                profile: animal_profile,
                                            }],
                                        },
                                    ],
                                    terrain: vec![TerrainContext {
                                        hex: (exp_pos.x, exp_pos.y),
                                    }],
                                    seed,
                                };
                                let outcome = resolve_fight(&payload, &combat_tuning);
                                let (killed_f, wounded_f) = outcome
                                    .results
                                    .iter()
                                    .find(|r| r.force == ForceId(0))
                                    .map(|r| (r.killed, r.wounded))
                                    .unwrap_or((0.0, 0.0));
                                if killed_f + wounded_f > 0.0 {
                                    cohort.apply_combat_casualties(scalar_from_f32(killed_f));
                                    let killed_r = killed_f.round() as u32;
                                    event_log.push(CommandEventEntry::new(
                                        current_turn,
                                        CommandEventKind::HuntDanger,
                                        faction,
                                        format!(
                                            "The {} hunt cost the expedition {} lives",
                                            species.display_name, killed_r
                                        ),
                                        Some(format!(
                                            "killed={:.3} wounded={:.3} species={}",
                                            killed_f, wounded_f, species.display_name
                                        )),
                                    ));
                                }
                            }
                        }
                        // **Every rung is paid its species' vector — including Eradicate** (#337).
                        // Denial is the END STATE (the species is gone, for you and everyone else),
                        // never a promise that the party threw the carcasses away; the whole-stock
                        // take is a windfall the party banks up to its pack.
                        //
                        // **BOTH components come out of ONE conversion of the same carried biomass**,
                        // exactly as `hunt_trip_forecast` projects them — the raid cannot pay food it
                        // did not promise, nor pocket the pelts it did. They then part ways: the meat
                        // is bounded by the pack (`room`), the pelts ride home on `carried_trade` and
                        // settle into the home band's store at the drop-off. Both scale off what the
                        // party **carries**, never what it killed: you cannot trade a hide you left on
                        // the range.
                        {
                            let carried = cohort.stores.get(FOOD);
                            let room = (cap - carried).max(scalar_zero());
                            let landed =
                                quarry_yield.apply(take.carried, EXPEDITION_OUTPUT_MULTIPLIER);
                            let provisions = scalar_from_f32(landed.provisions);
                            let added = provisions.min(room);
                            if added > scalar_zero() {
                                cohort.stores.add(FOOD, added);
                            }
                            expedition.carried_trade += landed.trade_goods;
                        }

                        // Trip-completion + early-delivery decision (arrived parties only). The pack is
                        // "full" once it is filled OR, **having already delivered**, cannot seat another
                        // whole animal (a leftover fraction of room the party won't over-kill to top off).
                        // The `carried > 0` gate lets the forced-partial raid work: a pack too small to
                        // seat even one whole animal must not come home empty — it banks credit until it
                        // kills one, whose forced partial FILLS the pack (`carried → cap`), and completes
                        // then. See `hunt_trip_forecast`'s matching completion (the forecast pins to this).
                        let carried = cohort.stores.get(FOOD);
                        let food_per_animal = quarry_yield
                            .apply(body_mass, EXPEDITION_OUTPUT_MULTIPLIER)
                            .provisions;
                        let full = carried >= cap
                            || (carried > scalar_zero()
                                && (cap - carried).to_f32() < food_per_animal);
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
                        // **An opportunistic DROP-OFF, never a trip completion** (issue #441). The
                        // herd has wandered within `drop_off_within_tiles` of the band and the pack
                        // holds a worthwhile load, so the party runs it in rather than hauling it
                        // around — the fix for the empty-larder flip-flop, where a party sat on a
                        // full-ish pack beside a hungry camp. It says *deliver now*, and nothing
                        // about whether the raid is over: that is `full`/`surplus_spent` below. For
                        // **every** delivering policy this feeds `relaunch`, so a committed party
                        // keeps raiding with an empty pack instead of folding home mid-surplus.
                        let near_band_gate = herd_near_band && carried >= min_deliver;

                        // **The load-bearing completion fix.** A raid is over when the standing surplus
                        // is spent — the herd is within one body of the policy's floor, so no whole
                        // animal is left to raid from standing stock (only the regrowth trickle the raid
                        // deliberately stops at). Without this a Sustain raid that grabs its surplus and
                        // hits K/2 would HANG, taking 0 every turn. **Every policy but Eradicate** —
                        // Eradicate's floor is `0`, so it has no standing surplus to spend and grinds
                        // to extinction via the lost-herd guard instead. That is a statement about the
                        // policy's FLOOR (its intensity), NOT about whether it carries food home —
                        // since #337 it does (see the payout above).
                        let surplus_spent = floor > STRIP_IT_BARE
                            && (herd_biomass_after - floor * carrying_capacity.max(0.0))
                                < body_mass;

                        // `done` = deliver then fold back + despawn (the trip is over); `relaunch` =
                        // deliver then resume Hunting. **A raid ends when the pack FILLS or the
                        // standing surplus is SPENT** (Sustain leaves K/2, Surplus 0.30·K) — those are
                        // the only two ways the work runs out. A herd that happens to wander within
                        // `drop_off_within_tiles` of camp is an opportunistic **drop-off**, not a
                        // completion (issue #441): the party is already committed and keeps raiding.
                        // Deplete makes repeated FULL-cap trips while the herd still has surplus
                        // (relaunch), and once it is stripped to its floor it comes home for good
                        // (`surplus_spent ⇒ done`) rather than trickle-churning at the floor.
                        //
                        // **Why the drop-off loop terminates**: `done` is tested *before* `relaunch`
                        // below, so a party at the policy floor never relaunches — once
                        // `surplus_spent` fires it comes home for good. Each drop-off cycle drains
                        // more of the standing surplus, so the loop converges on `surplus_spent` (or
                        // `full`, or the lost-herd guard).
                        let (done, relaunch) = if floor <= STRIP_IT_BARE {
                            // **Floor `0` never delivers** — it grinds the herd to extinction and ends
                            // through the lost-herd guard. There is no surplus to "spend", because
                            // nothing is meant to be left standing.
                            (false, false)
                        } else if raid_is_recurring(floor) {
                            // A floor below the food peak leaves more standing than one pack holds,
                            // so the party runs repeated trips until the herd sits at its floor.
                            (surplus_spent, full || near_band_gate)
                        } else {
                            // At or above the peak the standing surplus is small enough that one raid
                            // (plus any opportunistic drop-off) finishes the job.
                            (full || surplus_spent, near_band_gate)
                        };

                        if done {
                            // Deliver + fold back via the shared Returning arm (deposits carried food).
                            expedition.phase = ExpeditionPhase::Returning;
                            // Never report a cheerful zero: an empty pack must name its cause.
                            // **"Empty" is a claim about BOTH products** (#337) — a wolf raid comes
                            // home with no meat and a pack full of pelts, and calling that EMPTY
                            // would be exactly the food-only blindness this arc removes. The pelts
                            // are still banked on `carried_trade`; the `Returning` arm settles them.
                            let pelts = expedition.carried_trade;
                            let (message, reason) = if carried > scalar_zero() || pelts > 0.0 {
                                (
                                    format!(
                                        "Hunting expedition harvested {} — returning home",
                                        describe_haul(carried.to_i64_whole(), pelts)
                                    ),
                                    "harvest_complete",
                                )
                            } else if standing_surplus <= 0.0 {
                                (
                                    format!(
                                        "Hunting expedition returning EMPTY — the {} is at its {:.2}·K floor and has no surplus to raid",
                                        fauna_id, floor
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
                                    "status={} floor={} expedition={}",
                                    reason,
                                    floor,
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
                // Run carried food to the band's live tile; once within comm range of it, deposit and
                // auto-relaunch to Hunting. **Every extractive delivering policy passes through here** (issue #441),
                // not Deplete alone: Deplete arrives on each FULL pack of its repeated trips, and any
                // policy arrives on a near-band drop-off. What differs between them is only *what ends
                // the trip* (the `Hunting` arm's `done`) — Deplete's series ends on surplus-spent,
                // Sustain/Surplus's single raid on a full pack or surplus-spent, and only those exits
                // route through `Returning`.
                if let Some(home) = home_pos {
                    commands.entity(entity).insert(BandTravel { target: home });
                }
                if near_home {
                    let delivered = {
                        let carried = cohort.stores.get(FOOD);
                        cohort.stores.take(FOOD, carried)
                    };
                    // The trip's pelts settle with its meat — one delivery, both products into the
                    // one band store, so the credit matches the raid forecast this trip was quoted
                    // against.
                    let mut banked_trade = scalar_zero();
                    if let Ok(mut home) = bands.get_mut(expedition.home_band) {
                        if delivered > scalar_zero() {
                            home.stores.add(FOOD, delivered);
                        }
                        banked_trade = settle_carried_trade(&mut expedition, &mut home);
                    }
                    event_log.push(CommandEventEntry::new(
                        current_turn,
                        CommandEventKind::Hunt,
                        faction,
                        format!(
                            "Hunting expedition dropped off {}",
                            describe_haul(delivered.to_i64_whole(), banked_trade.to_f32())
                        ),
                        Some(format!(
                            "status=delivered trade_goods={:.2} expedition={}",
                            banked_trade.to_f32(),
                            entity.to_bits()
                        )),
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

/// Bank everything a party is carrying in [`Expedition::carried_trade`] into the **home band's**
/// store and empty the pack, returning what was credited.
///
/// **Called on ARRIVAL — a `Delivering` drop-off or a `Returning` fold-back — not at the kill**, and
/// that is the whole reason the field exists: a raid's promised `HuntTripForecast::delivered_trade`
/// is a sum over the *whole trip*, and the pack has to physically reach the band before anyone can
/// hold what is in it (`docs/plan_hunt_yield_model.md` Decision 8).
///
/// **Nothing is rounded off any more.** The banked amount used to be `round()`ed to whole goods
/// because `FactionInventory` is an `i64` account; a [`LocalStore`](crate::LocalStore) is
/// fixed-point, so the exact carried fraction lands and `forecast == actual` holds without a
/// remainder being dropped on each trip.
fn settle_carried_trade(expedition: &mut Expedition, home: &mut PopulationCohort) -> Scalar {
    let banked = scalar_from_f32(expedition.carried_trade);
    expedition.carried_trade = 0.0;
    if banked > scalar_zero() {
        home.stores.add(TRADE_GOODS, banked);
    }
    banked
}

/// A haul as feed-line prose — *"12 provisions"*, *"4.00 trade goods"*, or *"12 provisions and 4.00
/// trade goods"*. **A zero component is omitted, never printed** (the render-only-when-non-zero rule
/// the whole yield-vector arc runs on): a wolf raid does not report "0 provisions", and a species with
/// no commercial value does not report "0 trade goods". Both zero is not this function's case — the
/// caller reports an empty pack with its cause instead.
///
/// Trade goods print to [`HAUL_TRADE_DECIMALS`] rather than as a whole count: since they became a
/// fixed-point band store a raid can honestly come home with a *fraction* of a good, and a whole-count
/// readout would print "0 trade goods" over a pack that really did bank pelts.
fn describe_haul(provisions: i64, trade_goods: f32) -> String {
    let trade = format!("{trade_goods:.*} trade goods", HAUL_TRADE_DECIMALS);
    match (provisions > 0, trade_goods > 0.0) {
        (true, true) => format!("{provisions} provisions and {trade}"),
        (false, true) => trade,
        _ => format!("{provisions} provisions"),
    }
}

/// Decimal places a feed line prints a fractional trade haul to — enough to show a sub-unit pack
/// (a wolf raid's ~0.4 pelts) without turning the line into a float dump.
const HAUL_TRADE_DECIMALS: usize = 2;

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

/// **THE** expedition's per-turn take, in *biomass* — the greedy raid (the playtest fix). The
/// `ExpeditionPhase::Hunting` arm, the launch forecast, and its provisions wrapper below all resolve
/// through this one function, so a preview can never quote a different take than the raid.
///
/// **A raid takes the standing surplus as fast as it can carry it.** Each turn the party takes as
/// much biomass as its throughput allows off the herd's **standing surplus** — the stock above the
/// policy's [`hunt_expedition_floor`] — so *more hunters take more animals in fewer-or-equal turns*,
/// the whole point of the fix (the resident band's ceiling was a per-turn *rate* then, so it was
/// worker-independent and a second hunter only added pack to fill, making the trip *longer*). When
/// the surplus is spent the herd sits at the floor and the raid comes home (the `hunt_trip_forecast`
/// / `Hunting`-arm completion checks own that); Sustain leaves `K/2`, Surplus `0.30·K`, Deplete
/// `0.15·K`.
///
/// **The band now shares the raid's SHAPE but not its pace** (`docs/plan_harvest_floor.md` §1): both
/// are constant escapement to the floor their orders name, and what still separates them is
/// that a raid's throughput is its whole party working one herd until the surplus is gone, while a
/// resident band works it a turn at a time.
///
/// **A raid brings home a PARTIAL when it must, and wastes the rest — reconciled with the band.** The
/// `credit` accumulator meters *when* the next whole animal is **ready** (a body heavier than one
/// turn's processing `throughput` takes `body / throughput` turns — the boar at 50 vs one hunter's 40).
/// Once the herd's standing surplus has banked one whole animal (`affordable >= 1`) the party **kills
/// one even if the pack cannot seat it whole**, carries the pack's worth, and **wastes the remainder** —
/// exactly the resident band's `max(1, carryable)` rule ([`fauna::quantise_animal_take`]): a 1-hunter
/// party on an 800-biomass mammoth kills it, keeps ~200, wastes ~600. When the surplus has NOT banked
/// an animal (`affordable == 0`) the party kills nothing and waits — the true "no surplus" case.
///
/// **This does NOT reintroduce the over-kill bug** (the reason the earlier no-waste rule existed). The
/// old bug was killing *many* animals per trip and carrying only a sliver of each; the guard here is the
/// **pack-full completion**, not a no-waste rule. When the pack cannot seat a whole animal
/// (`seatable == 0`) the forced partial carries `min(body, room) = room` — a full pack — so the
/// `hunt_trip_forecast` / `Hunting`-arm pack-full stop fires and the trip ends after that ONE forced
/// partial kill. The party kills 1 and comes home, never many.
#[allow(clippy::too_many_arguments)] // the herd's state and the party's caps are all inputs
fn expedition_take_biomass(
    workers: u32,
    per_worker_biomass_capacity: f32,
    floor: f32,
    biomass: f32,
    carrying_capacity: f32,
    body_mass: f32,
    carry_room_biomass: f32,
    credit: &mut f32,
) -> AnimalTake {
    if !body_mass.is_finite() || body_mass <= 0.0 {
        debug_assert!(
            false,
            "body_mass must be finite and positive; got {body_mass}"
        );
        return AnimalTake::default();
    }
    // The standing surplus above the policy's floor — everything the raid may take.
    let floor = floor * carrying_capacity.max(0.0);
    let standing_surplus = (biomass - floor).max(0.0);
    // Bank the party's processing throughput; the bank meters WHEN the next whole animal is ready,
    // never how much of it is carried. Capped at the surplus so it never funds a kill below the floor.
    let throughput = (workers as f32 * per_worker_biomass_capacity).max(0.0);
    let rate = throughput.min(standing_surplus);
    let ceiling = (*credit + rate).clamp(0.0, standing_surplus);
    // Whole animals: as many as the bank has readied (`affordable`). If the pack cannot seat one
    // (`seatable == 0`) but the herd has banked one (`affordable >= 1`), the party still kills ONE and
    // wastes what it cannot haul — the band's `max(1, carryable)` rule ([`fauna::quantise_animal_take`]).
    // With no banked animal (`affordable == 0`) it kills nothing and waits (the true no-surplus case).
    let room = carry_room_biomass.max(0.0);
    let affordable = (ceiling / body_mass).floor().max(0.0);
    let seatable = (room / body_mass).floor().max(0.0);
    let killed = if affordable >= 1.0 {
        affordable.min(seatable.max(1.0))
    } else {
        0.0
    };
    let killed_biomass = killed * body_mass;
    let carried = killed_biomass.min(room); // carry what the pack holds; a forced partial fills it
    let wasted = (killed_biomass - carried).max(0.0);
    // Drain the bank by what was KILLED (carried + wasted), not merely carried — you cannot un-kill the
    // animal you could not haul. Cap at the surplus so it can't grow unbounded at the floor (surplus <
    // body ⇒ no kill ⇒ the bank would otherwise climb every turn). `0 ≤ credit ≤ surplus`.
    *credit = (*credit + rate - killed_biomass)
        .max(0.0)
        .min(standing_surplus);
    AnimalTake {
        killed: killed as u32,
        carried,
        wasted,
    }
}

/// The **provisions a hunting party actually lands in its larder per turn** at a herd's current state
/// — the real take ([`expedition_take_biomass`] through the species' [`HuntYield`], no output
/// multiplier), ignoring only carry room (which bites solely on the final partial turn, and `ceil()`
/// already accounts for that). `0` for an **inedible** species (a wolf is not food) — since #337 that
/// is a fact about the *species*, never about the policy: **Eradicate pays the windfall** like every
/// other rung. This is what the client's pre-launch readout is pinned to
/// (`core_sim/tests/expedition_hunt.rs`).
#[allow(clippy::too_many_arguments)] // the herd's state, the labor tier and the species vector are all inputs
pub fn expedition_take_provisions(
    workers: u32,
    floor: f32,
    biomass: f32,
    carrying_capacity: f32,
    body_mass: f32,
    labor: &LaborConfig,
    hunt_yield: HuntYield,
) -> f32 {
    // A single-turn preview starting from an empty bank (this readout is the client's per-turn rate,
    // not a specific banked turn) — the forward-sim `hunt_trip_forecast` is the one pinned to actual.
    let mut credit = 0.0_f32;
    let take = expedition_take_biomass(
        workers,
        labor.hunt.per_worker_biomass_capacity,
        floor,
        biomass,
        carrying_capacity,
        body_mass,
        // Carry room bites only on the final partial turn, and `ceil()` already accounts for it.
        f32::INFINITY,
        &mut credit,
    );
    // Quantized onto the larder's `Scalar` grid, exactly as the real take lands there.
    scalar_from_f32(
        hunt_yield
            .apply(take.carried, EXPEDITION_OUTPUT_MULTIPLIER)
            .provisions,
    )
    .to_f32()
}

/// The shared **"take food from a nearby source"** primitive (`docs/plan_exploration_and_sites.md`
/// §2b). Resolves the stance's escapement ceiling ([`fauna::hunt_escapement_ceiling`] — the single
/// source), rounds it to **whole animals** against the party's collection
/// ([`fauna::quantise_animal_take`] — the single quantiser), and **subtracts every animal killed from
/// the herd**. One code path for three callers: the band Hunt labor (`advance_labor_allocation`,
/// which additionally accrues husbandry from the same take), the hunting expedition, and the scout's
/// opportunistic replenish (`advance_expeditions`, `output_multiplier = 1.0`). **All three credit
/// both components of the species' [`HuntYield`]** (#337) — they differ only in *when* the trade half
/// is banked: the band rounds it per turn, a detached party carries it home (`settle_carried_trade`).
///
/// **Returns the [`AnimalTake`] in *biomass*, not provisions** (slice 8): a take is now three numbers
/// — what was killed, what was carried, what rotted — and only the caller knows what to do with each
/// (the band banks `carried` and reports `wasted` on its income breakdown; trade goods scale off the
/// carried meat). Handing back one pre-converted `Scalar` would have forced every caller to
/// re-derive the other two from `herd.biomass` before/after, which is exactly the "second copy of the
/// model" this function exists to prevent. `output_multiplier` therefore no longer belongs here —
/// callers convert with the quarry's own [`HuntYield::apply`].
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
    floor: f32,
    improvement: Option<Improvement>,
    per_worker_biomass_capacity: f32,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    carry_room_biomass: f32,
) -> AnimalTake {
    // **Constant escapement** (`docs/plan_harvest_floor.md` §1): the herd hands over the stock
    // standing above the assignment's floor, at its CURRENT biomass. Resolved against the herd's OWN
    // capacity (`herd_capacity` — the single source of the rung → `K` mapping), never the raw wild
    // field. Shared with the pre-commit forecast (`fauna::hunt_forecast`), which reads the same
    // ceiling, so forecast == actual.
    //
    // **The kill-credit bank is NOT read or advanced here** — see `Herd::hunt_credit`. A ceiling that
    // is a *stock* must not be banked (that compounds it); the wait between kills is now the herd's
    // own biomass climbing back over one `body_mass` above the floor, which pays the same
    // wait-then-one pulse for a slow breeder.
    //
    let ceiling = fauna::hunt_escapement_ceiling(floor, herd.biomass, herd_capacity(herd, fauna));
    // **Whole animals** ([`fauna::quantise_animal_take`], slice 8): the crew kills what the *bank* can
    // afford, bounded by what it can haul but never below one — so a party that cannot carry a whole
    // animal still takes one and wastes the rest, and a bank that cannot yet spare one leaves the herd
    // to keep accumulating.
    //
    // `collection` is the hunting group's throughput, bounded by the biomass the caller can carry home
    // (`carry_room_biomass`); the band Hunt passes `f32::INFINITY` (no carry limit — it eats/banks the
    // whole take). Folding the carry room into the collection rather than clamping afterwards is what
    // keeps a nearly-full party from slaughtering an animal it has no room for.
    //
    // **The build dip rides the CREW, not the ceiling** (`docs/plan_harvest_floor.md` §3.1): a
    // resident band gentling or fencing this herd carries `yield_fraction_while_building ×` what a
    // hunting crew carries; an expedition passes [`NO_IMPROVEMENT_UNDERWAY`], because a
    // rung-transition is place-bound work a detached party cannot do — and since #442 its mission
    // type cannot even name one. On throughput the dip is floor-independent by construction, which
    // is what stops a deep floor from building for free (§0.3).
    let collection = (workers as f32 * per_worker_biomass_capacity * ladder.build_dip(improvement))
        .min(carry_room_biomass.max(0.0));
    let take = fauna::quantise_animal_take(ceiling, collection, herd.body_mass);
    // **The herd loses every animal KILLED, not merely what was carried** — you cannot un-kill the
    // mammoth you could not haul. That is the waste, and it is `take.wasted`.
    herd.biomass -= take.killed_biomass();
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
    /// via the other fields: it **brings home no food** (`delivers_food == false` — an *inedible*
    /// quarry, e.g. a wolf), the herd had **no surplus to take** (`animals_taken == 0` — at/below the
    /// policy's floor), or it only trickle-fills off regrowth (a slow breeder a big party can neither
    /// fill nor exhaust).
    pub turns_to_fill: Option<u32>,
    /// **Does this trip bring home FOOD?** REDEFINED by #337: it is now a fact about the **species**
    /// (`HuntYield::edible`), not about the policy. It used to read `false` for Eradicate on the
    /// premise "denial carries nothing home" — the premise this arc reverses, since an Eradicate raid
    /// now banks the whole-stock windfall. `false` today means *"wolves are not food"*.
    pub delivers_food: bool,
    /// **Does this trip bring home TRADE GOODS?** The sibling of `delivers_food`
    /// (`HuntYield::tradeable`) — the other half of the species' yield vector, so the client can say
    /// "pelts, no meat" instead of inferring a denial mission from a food `false`.
    pub delivers_trade: bool,
    /// Provisions landed on the **first** hunting turn — the trip's opening rate, and (with
    /// `animals_taken`) a "can this herd give me anything at all?" signal.
    pub first_turn_provisions: f32,
    /// **Whole animals the party KILLS over the raid** — the kill count (carried whole or partially
    /// wasted). `0` = the herd is at/below the policy's floor and has no surplus to raid (the honest
    /// non-viable case). A small party on a big animal now kills one and wastes most of it (mirroring
    /// the resident band), so this is a KILL count, not a delivered count — see `delivered_food`.
    pub animals_taken: u32,
    /// **Food the party actually LANDS in its larder over the raid** — `Σ HuntYield::apply(carried)`
    /// on the provisions component.
    /// This is the primary readout: "too lean to raid" means `delivered_food == 0` (no surplus), NOT
    /// "the party was too small to seat a whole animal" (which now delivers a partial).
    pub delivered_food: f32,
    /// **Food KILLED but not carried home over the raid** — `Σ HuntYield::apply(wasted)`. The waste of a
    /// party too small to haul its kills whole; `wasted_food / (delivered_food + wasted_food)` is the
    /// waste fraction the client shows.
    pub wasted_food: f32,
    /// **Trade goods the party actually LANDS over the raid** — `Σ HuntYield::apply(carried)` on the
    /// trade component, projected through the *same* vector the live take pays with (#337). For an
    /// **inedible** quarry this is the whole payload: `delivered_food` is `0` and `delivers_food`
    /// false, while this is what comes home.
    pub delivered_trade: f32,
}

/// One hunter's per-turn **provisions** throughput at the **global** `hunt.provisions_per_biomass`
/// rate: their biomass take capacity converted through it. Worker-scaled (× party size) it is a
/// party's uncapped rate, exported per-cohort in the snapshot
/// (`PopulationCohortState.huntPerWorkerProvisions`).
///
/// # It is SPECIES-BLIND, deliberately and unavoidably — do not use it for a per-herd preview
///
/// This is a **per-cohort** echo of a global lever: the cohort has no herd, so there is no species to
/// resolve a [`HuntYield`] from, and threading one in is not possible rather than merely unwritten.
/// Left un-flagged that is a **contradiction on the wire** (#337): a wolf's per-policy ceilings are
/// all `0` food, while this would quote every hunter a positive food rate against them.
///
/// The **per-herd, species-aware** rates already exist and are what a band preview must clamp with —
/// `HerdTelemetryState.perWorkerYield` / `perWorkerTrade`, straight off that herd's `hunt_forecast`,
/// so `min(workers × perWorkerYield, huntPolicyCeilings[p].provisionsPerTurn)` is honest per
/// component for every species. This constant survives only as the **expedition outfit** lever it was
/// (a party's rough carry arithmetic before a target is chosen); the *answer* for a chosen target is
/// the sim's own `huntTripEstimates` row, which is fully species-aware.
///
/// **Snapped to the `Scalar` grid** the larder actually accumulates on — the take path quantizes
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
/// accumulates on the fixed-point `Scalar` grid**, exactly as the real one does (`HuntYield::apply`
/// quantizes every take), so an evenly-dividing trip cannot invent a phantom extra turn.
///
/// **Travel is not part of this estimate** — it assumes the party is already in reach and stationary,
/// so the number means "turns spent *hunting* once you arrive." An **inedible** quarry gets no food ETA
/// (`delivers_food = false` — a wolf is not food; #337 moved this guard off "denial"). Pinned to a real
/// party run forward through the real systems
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
    floor: f32,
    fauna: &FaunaConfig,
    labor: &LaborConfig,
    expedition: &ExpeditionConfig,
) -> HuntTripForecast {
    // The pre-launch estimate: an EMPTY pack (the party has not left yet). See
    // `hunt_trip_forecast_seeded` for the in-flight (partial-pack) variant.
    hunt_trip_forecast_seeded(
        workers,
        herd,
        floor,
        fauna,
        labor,
        expedition,
        scalar_zero(),
    )
}

/// The raid forward-simulation, seeded with an `initial_larder` — the twin of [`hunt_trip_forecast`]
/// for a party **already in flight** carrying a partial pack. `delivered_food` accumulates only the
/// NEW food taken (room-capped against the seeded larder), so a caller's total delivery is
/// `initial_larder + delivered_food`. Passing `scalar_zero()` reproduces the pre-launch estimate
/// byte-for-byte.
#[allow(clippy::too_many_arguments)] // every config the forward simulation reads is a lever
fn hunt_trip_forecast_seeded(
    workers: u32,
    herd: &Herd,
    floor: f32,
    fauna: &FaunaConfig,
    labor: &LaborConfig,
    expedition: &ExpeditionConfig,
    initial_larder: Scalar,
) -> HuntTripForecast {
    // The quarry's yield vector — **the species decides the product**, the policy only the intensity.
    let hunt_yield = fauna::herd_hunt_yield(herd, fauna);
    let delivers_food = hunt_yield.edible();
    let delivers_trade = hunt_yield.tradeable();
    let cap = scalar_from_f32(workers as f32 * expedition.hunt.per_worker_carry);
    // **An empty party has no pack**, so nothing about its trip can be projected. That is the only
    // case that short-circuits.
    //
    // It used to short-circuit an **INEDIBLE** quarry too ("a wolf trip is not a food trip"), and
    // before #337 a *denial* one — but zeroing the whole projection to say "no food ETA" also zeroed
    // `animals_taken` and `delivered_trade`, i.e. the entire payload of the one raid that is paid
    // *only* in trade. It made the client quote `⇄ ~0` on a wolf while the sim banked real pelts:
    // `forecast == actual` broken from the forecast's side. A wolf trip is a real trip — it ends when
    // the standing surplus is spent — so the loop below projects it honestly, and the food fields
    // fall out at `0` on their own because the species' `provisions_per_biomass` is `0`.
    if cap <= scalar_zero() {
        return HuntTripForecast {
            turns_to_fill: None,
            delivers_food,
            delivers_trade,
            first_turn_provisions: 0.0,
            animals_taken: 0,
            delivered_food: 0.0,
            wasted_food: 0.0,
            delivered_trade: 0.0,
        };
    }

    let horizon = expedition.hunt.forecast_horizon_turns;
    // The forecast runs on a private copy of the herd — the caller's live herd is never touched.
    let mut quarry = herd.clone();
    // The herd's OWN ecology + capacity (resolved once — neither can change under the party's take:
    // the quarry is never tamed or penned mid-trip).
    let ecology = herd_ecology(&quarry, fauna);
    let capacity = herd_capacity(&quarry, fauna);
    let floor_biomass = floor * capacity.max(0.0);
    let mut larder = initial_larder;
    let mut first_turn_provisions = 0.0_f32;
    let mut animals_taken = 0u32;
    let mut delivered_food = 0.0_f32;
    let mut delivered_trade = 0.0_f32;
    let mut wasted_food = 0.0_f32;

    for turn in 1..=horizon {
        // Logistics: the herd's ecology moves first (regrowth, or the depensation decline), exactly
        // as `advance_herds` runs before the Population stage's take.
        fauna::regrow_biomass(&mut quarry, fauna);
        if quarry.biomass <= ecology.extinction_floor * capacity {
            // `advance_herds` would despawn it here — a lost herd ends the raid.
            break;
        }

        // Population: the `Hunting` arm's greedy take, through the same helper, bounded by the carry
        // room left in the pack — converted back into biomass **exactly** as the arm converts it,
        // including both of its unbounded cases, or the forecast would quote a different raid than
        // the take: **Eradicate** ignores the pack (an intensity fact) and an **inedible** quarry
        // never fills a *food* pack (a product fact). The second is stated rather than left to the
        // `x / 0.0 = inf` the division would otherwise produce.
        let carry_room_biomass = if floor <= STRIP_IT_BARE || !delivers_food {
            f32::INFINITY
        } else {
            (cap - larder).max(scalar_zero()).to_f32() / hunt_yield.provisions_per_biomass
        };
        let take = expedition_take_biomass(
            workers,
            labor.hunt.per_worker_biomass_capacity,
            floor,
            quarry.biomass,
            capacity,
            quarry.body_mass,
            carry_room_biomass,
            &mut quarry.hunt_credit,
        );
        quarry.biomass -= take.killed_biomass();
        // The kill count — a raid may now kill a partial (one it cannot seat whole) and waste the rest,
        // exactly like the resident band; the delivered payload is `delivered_food`, not this count.
        animals_taken += take.killed;
        // Delivered food (carried) + wasted food (killed but not hauled), matching the per-turn
        // provisions conversion (no output multiplier — `EXPEDITION_OUTPUT_MULTIPLIER` is 1.0).
        // **Both products come from ONE conversion of the same carried biomass** — the raid's forecast
        // cannot promise food it will not pay, nor forget the pelts it will (#337).
        let landed = hunt_yield.apply(take.carried, EXPEDITION_OUTPUT_MULTIPLIER);
        delivered_food += landed.provisions;
        delivered_trade += landed.trade_goods;
        wasted_food += hunt_yield
            .apply(take.wasted, EXPEDITION_OUTPUT_MULTIPLIER)
            .provisions;

        let provisions = scalar_from_f32(
            hunt_yield
                .apply(take.carried, EXPEDITION_OUTPUT_MULTIPLIER)
                .provisions,
        );
        let room = (cap - larder).max(scalar_zero());
        larder += provisions.min(room);
        if turn == FIRST_HUNTING_TURN {
            first_turn_provisions = provisions.to_f32();
        }

        // The raid completes when the pack is full — or, **having already delivered**, cannot seat
        // another whole animal (a leftover fraction of room the party won't over-kill to top off) — OR
        // the standing surplus is spent (the herd is within one body of its floor, only the regrowth
        // trickle left, which the raid deliberately stops at). Whichever fires, the party comes home;
        // this mirrors the `ExpeditionPhase::Hunting` arm's `done`. **The `larder > 0` gate is what lets
        // the forced-partial raid work**: a pack too small to seat even one whole animal (`cap <
        // food_per_animal`) must NOT come home empty on turn 1 — it banks credit until it can kill one,
        // fills the pack with that forced partial (`larder → cap`), and *then* completes. Once a delivery
        // exists, the can't-seat check resumes its old job (no over-killing a fractional gap).
        //
        // **The near-band drop-off is deliberately NOT modelled here, and cannot be**: the pre-launch
        // `huntTripEstimates` table is **band-agnostic** (one row per herd serves every band), so this
        // forecast has no band position to measure `hunt.drop_off_within_tiles` against. The
        // approximation is one-directional and therefore safe: a drop-off lets a raid deliver **more**
        // than projected (several loads over a longer trip, since the party resumes hunting with an
        // empty pack), never less, so this projection is a **lower bound** on a near-band raid. Before
        // issue #441 the same gate made Sustain/Surplus *end* the trip, so the forecast erred in the
        // unhelpful direction — quoting a trip that came home early with less than promised.
        let food_per_animal = hunt_yield
            .apply(quarry.body_mass, EXPEDITION_OUTPUT_MULTIPLIER)
            .provisions;
        let pack_cannot_seat_another =
            larder > scalar_zero() && (cap - larder).to_f32() < food_per_animal;
        // Mirrors the live arm's completion exactly: Eradicate's floor is `0`, so it has no standing
        // surplus to spend and ends via the herd-lost break above instead.
        let surplus_spent =
            floor > STRIP_IT_BARE && (quarry.biomass - floor_biomass) < quarry.body_mass;
        if larder >= cap || pack_cannot_seat_another || surplus_spent {
            return HuntTripForecast {
                turns_to_fill: Some(turn),
                delivers_food,
                delivers_trade,
                first_turn_provisions,
                animals_taken,
                delivered_food,
                wasted_food,
                delivered_trade,
            };
        }
    }

    HuntTripForecast {
        turns_to_fill: None,
        delivers_food,
        delivers_trade,
        first_turn_provisions,
        animals_taken,
        delivered_food,
        wasted_food,
        delivered_trade,
    }
}

/// The in-flight delivery forecast for a live **hunting** party — the client's
/// "Next delivery: ~X food in ~N turns" drawer line, the in-flight twin of the pre-launch
/// `hunt_trip_forecast`/`huntTripEstimates`. Scouts deliver map data, not food, so they → `None`.
///
/// `eta_turns` decomposes as remaining-travel-to-herd + hunting-turns-to-complete + walk-home; it is
/// an APPROXIMATION (the home band is nomadic and may move, and the walk-home is measured from the
/// herd) — honest for a "~N turns" readout, deliberately not turn-perfect. `None` when the raid can't
/// complete within the forecast horizon (a trickle-fill): the client shows the amount without an ETA.
pub struct ExpeditionDelivery {
    pub eta_turns: Option<u32>,
    pub projected_food: f32,
    pub recurring: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn expedition_delivery(
    expedition: &Expedition,
    carried: f32,
    workers: u32,
    party_pos: UVec2,
    home_pos: Option<UVec2>,
    herds: &HerdRegistry,
    fauna: &FaunaConfig,
    labor: &LaborConfig,
    expedition_cfg: &ExpeditionConfig,
    grid_width: u32,
    wrap_horizontal: bool,
) -> Option<ExpeditionDelivery> {
    let ExpeditionMission::Hunt { fauna_id, floor } = &expedition.mission else {
        return None; // Scouts deliver map data, not food.
    };
    let floor = *floor;

    let speed = labor.band_move_tiles_per_turn.max(1);
    let travel = |a: UVec2, b: UVec2| {
        crate::grid_utils::hex_distance_wrapped(a, b, grid_width, wrap_horizontal).div_ceil(speed)
    };
    let recurring = raid_is_recurring(floor);

    match expedition.phase {
        // Already heading home with its haul: the delivery is what it carries, ETA is the walk home.
        ExpeditionPhase::Returning | ExpeditionPhase::Delivering => Some(ExpeditionDelivery {
            eta_turns: home_pos.map(|h| travel(party_pos, h)),
            projected_food: carried,
            recurring,
        }),
        // Still working toward the kill.
        ExpeditionPhase::Hunting | ExpeditionPhase::Outbound | ExpeditionPhase::AwaitingOrders => {
            let Some(herd) = herds.find(fauna_id) else {
                // Herd lost → the party will fold home carrying what it has.
                return Some(ExpeditionDelivery {
                    eta_turns: home_pos.map(|h| travel(party_pos, h)),
                    projected_food: carried,
                    recurring: false,
                });
            };
            let herd_pos = herd.position();
            let in_reach = crate::grid_utils::hex_distance_wrapped(
                party_pos,
                herd_pos,
                grid_width,
                wrap_horizontal,
            ) <= expedition_cfg.hunt.reach_tiles;
            let travel_to_herd = if in_reach {
                0
            } else {
                travel(party_pos, herd_pos)
            };
            let fc = hunt_trip_forecast_seeded(
                workers,
                herd,
                floor,
                fauna,
                labor,
                expedition_cfg,
                scalar_from_f32(carried),
            );
            let projected_food = carried + fc.delivered_food; // room-capped by construction, ≤ cap
            let travel_home = home_pos.map(|h| travel(herd_pos, h)); // delivers from near the herd
            let eta_turns = match (fc.turns_to_fill, travel_home) {
                (Some(h), Some(t)) => Some(travel_to_herd + h + t),
                _ => None,
            };
            Some(ExpeditionDelivery {
                eta_turns,
                projected_food,
                recurring,
            })
        }
    }
}
