use super::*;
use crate::combat;
use crate::components::RaidOrders;
use crate::fauna::AnimalTake;
use crate::intensification::NO_BUILD_UNDERWAY_DIP;

/// **The reason token for a HUNTING party whose quarry vanished under it** — the herd went extinct,
/// or dispersed, while the party was still working it, so the trip ends with nothing left to take.
/// A denial raid reaching the same exit reports [`DenialOutcome::HerdLost`] instead, because for it
/// the empty range is the mission accomplished; keeping the hunt's own token distinct is what lets a
/// reader tell the failure from the success.
const HERD_GONE_MID_HUNT: &str = "herd_gone";

/// **Everything one detached party is, as a query tuple.** Named because the tuple grew past the
/// point of readability when the party's own kit joined it: **a detached party carries its OWN kit**
/// (`docs/plan_denial_raid.md` §1.2). It leaves outfitted (`BandEquipment::default()` is zero wear)
/// and, since the take resolves through the fight (`docs/plan_hunt_through_combat.md` §4), it must
/// also *wear* that kit — a raid on free, immortal equipment is denial for nothing, and its `attack`
/// tier is what the fight's gate compares against.
type ExpeditionParty = (
    Entity,
    &'static mut PopulationCohort,
    Option<&'static BandTravel>,
    &'static mut Expedition,
    Option<&'static mut BandEquipment>,
);

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
    /// The TOE kit table — a detached party resolves its own attack/haul tiers off it and wears them.
    pub equipment: Res<'w, EquipmentConfigHandle>,
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
///   arrival feed line; `Returning` → chase the home band's live tile and, once within comm range
///   (or the moment that band cannot be resolved at all), fold back through
///   [`fold_party_into_band`] and despawn (fold-back happens after the flush so the final findings
///   report); `AwaitingOrders` waits (relaunched by `move_band`).
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
    mut expeditions: Query<ExpeditionParty>,
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
    // **The minimal TOE** — the two-tier table and the durability dials, resolved once. What varies
    // per party is only its `BandEquipment` *wear*.
    let equipment_cfg = configs.equipment.get();
    // The **equipped** per-hunter haul rate; the SLED kit names the step down. A raid is a hunt, so
    // baskets never enter this path (§4.8's one kit, one job) — an expedition has no gather mission.
    let equipped_haul_rate = labor.hunt.per_worker_biomass_capacity;
    let map_seed = sim_config.map_seed;
    let wrap_horizontal = sim_config.map_topology.wrap_horizontal;
    let grid_width = tile_registry.width;
    let current_turn = tick.0;
    let comm_range = cfg.effective_comm_range();

    // Shared LOS inputs (built once per turn for the few expeditions).
    let terrain_tags = crate::visibility_systems::build_terrain_tags_grid(
        &tiles,
        elevation.width,
        elevation.height,
    );
    let blocking_tags = crate::visibility_systems::parse_blocking_tags(
        &vis_cfg.line_of_sight.blocking_terrain_tags,
    );

    for (entity, mut cohort, travel, mut expedition, mut party_equipment) in expeditions.iter_mut()
    {
        let Ok(exp_pos) = tiles.get(cohort.current_tile).map(|tile| tile.position) else {
            continue;
        };
        let faction = cohort.faction;
        let workers = available_workers(cohort.working);
        // **This party's two kit tiers, resolved ONCE per party per turn** — the same discipline
        // `advance_labor_allocation` applies to a resident band, through the same
        // `EquipmentConfig` seams. An absent component reads as a full kit (wear, not stock).
        let party_wear = party_equipment.as_deref().cloned().unwrap_or_default();
        // **The kit this party was SENT OUT WITH** — stored on the `Expedition` at launch and read
        // from there, never re-resolved against the home band's current stock. A party sent out with
        // `none` stays bare-handed for its whole life; re-reading the band's spears each turn would
        // silently re-arm it.
        let party_kit = expedition.kit.clone();
        // **Every tier resolved once per party per turn**, through the kit mask and the party's own
        // wear — so a party using nothing runs unequipped and, because wear rides the same mask,
        // spends nothing either.
        let per_worker_biomass = equipment_cfg.hunt_per_worker_biomass_capacity(
            equipped_haul_rate,
            &party_kit,
            &party_wear,
        );
        // The weapon decides what the party can hurt at all (§4.2's gate), so it is resolved here and
        // not left at the intrinsic bare-handed tier. `exposure`, `engage_multiplier` and
        // `dispersion` ride beside it — a raid carrying a stand-off kit takes no injuries and scares
        // nothing off, exactly as a resident band with the same kit does.
        // **A FACTORY, for the reason `advance_labor_allocation`'s is** — a mass-bounded weapon is
        // only a weapon against quarry it can hold, so the attack tier waits for the target.
        let party_for = |body_mass: f32| fauna::HuntingParty {
            hunter: equipment_cfg.hunter_profile_against(
                person_profile,
                &party_kit,
                &party_wear,
                body_mass,
            ),
            tuning: combat_tuning,
            injury_damage_per_animal: combat_config.hunt_injury_damage_per_animal
                * equipment_cfg.exposure(&party_kit, &party_wear),
            engage_multiplier: equipment_cfg.engage_multiplier(&party_kit, &party_wear),
            dispersion: equipment_cfg.dispersion(&party_kit, &party_wear),
        };
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

        // A raiding party whose herd is lost/extinct flips to Returning (folds back via the shared
        // arm below), with a feed line — knowledge/food it carries still comes home. **A denial
        // raid reaches this the same way a hunt does**, and for it a lost herd is the mission
        // succeeding outright rather than the target slipping away.
        if let Some(orders) = mission.raid_orders() {
            let fauna_id = orders.fauna_id;
            if herds.find(fauna_id).is_none()
                && !matches!(expedition.phase, ExpeditionPhase::Returning)
            {
                expedition.phase = ExpeditionPhase::Returning;
                // **The two missions read the same exit in opposite directions**, so they cannot
                // share a line. `DenialOutcome::HerdLost` is one of the two verdicts
                // `DenialOutcome::succeeded` returns true for and the launch sheet quotes it as a
                // win, so a raid reporting a *lost quarry* here would tell the player their raid
                // failed on one of its two success paths. The `done` arm below states the same
                // split for the other one (`past_recovery`); the reasons are the `DenialOutcome`
                // keys, so the exit and the pre-launch verdict spell the outcome the same way.
                let (message, reason) = match orders.stop {
                    fauna::EngagementStop::Never => (
                        format!("Denial raid wiped out the {} — returning home", fauna_id),
                        DenialOutcome::HerdLost.as_str(),
                    ),
                    fauna::EngagementStop::WhenPackFull => (
                        format!("Hunting expedition lost the {} — returning home", fauna_id),
                        HERD_GONE_MID_HUNT,
                    ),
                };
                event_log.push(CommandEventEntry::new(
                    current_turn,
                    CommandEventKind::Hunt,
                    faction,
                    message,
                    Some(format!(
                        "status=returning reason={} expedition={}",
                        reason,
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
                    let carry_room = carry_room_biomass(room, &scout_yield);
                    // The quarry's mass, read before the mutable borrow — a mass-bounded weapon is
                    // only a weapon against animals it can hold, so the party's attack tier waits
                    // for it exactly as the resident band's does.
                    let scout_quarry_mass = herds.herds[idx].body_mass;
                    // Composed BEFORE the mutable borrow — the seed reads the herd's id, and the
                    // take needs the herd mutably.
                    let seed = fauna::retreat_seed(
                        sim_config.map_seed,
                        tick.0,
                        &herds.herds[idx].id,
                        workers,
                    );
                    let outcome = hunt_take(
                        &mut herds.herds[idx],
                        workers,
                        // A scout's roadside kill is a **restrained** one: it stops at the food peak,
                        // the same floor a fresh assignment gets, so replenishing on the march can
                        // never be the thing that ruins a herd.
                        DEFAULT_ESCAPEMENT_FLOOR,
                        NO_IMPROVEMENT_UNDERWAY,
                        per_worker_biomass,
                        &party_for(scout_quarry_mass),
                        &fauna,
                        &ladder,
                        carry_room,
                        fauna::HuntDraw::Seeded(seed),
                    );
                    let take = outcome.take;
                    // **A roadside kill wears the scout's kit like any other** — the hunting kit per
                    // animal killed, the SLED per biomass hauled (`docs/plan_denial_raid.md`
                    // §1.2: wear tracks USE, never turns elapsed). No baskets: nothing was gathered.
                    // Each charge gated on the predicate that chose its own tier: a party using
                    // no spears blunts none, and a party dragging by hand wears no sled.
                    if let Some(kit) = party_equipment.as_mut() {
                        // **Named by QUANTUM, not by item.** Every item in the party's kit that
                        // wears per kill is charged for the kills, every item that wears per
                        // biomass hauled for the haul — so an item added to a kit is charged
                        // here without editing this call, and an item the kit does not carry is
                        // never charged at all.
                        kit.wear_kit(
                            &equipment_cfg,
                            &party_kit,
                            crate::equipment_config::WearQuantum::Kill,
                            take.killed as f32,
                        );
                        kit.wear_kit(
                            &equipment_cfg,
                            &party_kit,
                            crate::equipment_config::WearQuantum::BiomassHauled,
                            take.carried,
                        );
                    }
                    // A scout that picked a fight it could not win still pays for it. Gated on a
                    // **death**, like the resident band's line: the hunt's baseline injury risk
                    // (§4.6) makes `casualties.any()` true on every engagement.
                    if outcome.fight.casualties.killed > fauna::NO_DEATHS_TO_REPORT {
                        cohort.apply_combat_casualties(scalar_from_f32(
                            outcome.fight.casualties.killed,
                        ));
                    }
                    // **A roadside kill is a hunt and reports as one** (§6.6) — it engages animals,
                    // wastes what one scout cannot haul, and hurts people, and none of that was
                    // visible anywhere before the report existed.
                    if let Some(entry) = hunt_report_event(
                        tick.0,
                        faction,
                        &fauna
                            .species_by_display(&herds.herds[idx].species)
                            .map(|def| def.display_name.clone())
                            .unwrap_or_else(|| herds.herds[idx].species.clone()),
                        &outcome,
                    ) {
                        event_log.push(entry);
                    }
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
                // **There is nowhere left to walk to when the home band cannot be resolved**, so an
                // orphan folds back where it stands rather than waiting for a rendezvous that can
                // never happen. `near_home` answers "am I close enough to hand things over?" and
                // `home_pos` answers "is there anyone to hand them to?"; reading only the first left
                // an orphan permanently `false` on the fold-back **and** on the retarget below,
                // stranding a live party on the map for the rest of the game with its workers,
                // pack and pelts held out of the economy. The fold-back already handles a missing
                // home (the haul is simply lost, exactly as its carried food is) — it was merely
                // unreachable.
                if near_home || home_pos.is_none() {
                    // Close enough to run home: fold workers + carried food back in (after the scout
                    // flush above, so the final findings reported), then despawn.
                    // **The other half of the haul settles into the SAME store as the meat.** Trade
                    // goods are band-local (see [`TRADE_GOODS`]), so the pelts land in the home
                    // band's larder alongside the provisions — the last chance before the party
                    // despawns and the bank goes with it. No home band left to receive them means
                    // the haul is simply lost, exactly as the carried food is.
                    let mut banked_trade = scalar_zero();
                    if let Ok(mut home) = bands.get_mut(expedition.home_band) {
                        banked_trade = fold_party_into_band(&cohort, &mut expedition, &mut home);
                    }
                    event_log.push(expedition_returned_event(
                        current_turn,
                        faction,
                        exp_pos,
                        banked_trade,
                        entity,
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
                if let Some(orders) = mission.raid_orders() {
                    let fauna_id = orders.fauna_id;
                    if let Some(idx) = herds.herds.iter().position(|herd| herd.id == *fauna_id) {
                        let RaidOrders { floor, stop, .. } = orders;
                        let herd_pos = herds.herds[idx].position();
                        // The herd's OWN capacity — the single source of the husbandry ladder's
                        // rung → `K` mapping (`herd_capacity`); a party hunting a tamed or penned herd
                        // raids *its* stock, not a wild counterfactual's.
                        let carrying_capacity = herd_capacity(&herds.herds[idx], &fauna);
                        // The herd's OWN ecology — the phase bands a denial raid aims to cross
                        // (`fauna::herd_past_recovery`). Resolved here, beside the capacity it is
                        // read against, so the completion below cannot re-derive either.
                        let ecology = herd_ecology(&herds.herds[idx], &fauna);
                        // **The party-side stop is the pack, and only the pack** — the same load the
                        // forecast projects (`hunt_trip_forecast_seeded`), so the raid comes home on
                        // the load it was quoted.
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
                        // grabs the herd's standing surplus above the **mission's own floor** as fast
                        // as its throughput allows, so more hunters take more animals in
                        // fewer-or-equal turns (a resident band's throttled per-turn rate was
                        // worker-independent — a second hunter only added pack to fill, lengthening the
                        // trip). The launch forecast SIMULATES this same helper, so the preview can't
                        // quote a different raid than this take. An inedible quarry carries no food,
                        // and that is a fact about the species, never about the floor.
                        let herd_biomass_before = herds.herds[idx].biomass;
                        // The surplus the raid may take — kept for the empty-pack diagnosis below
                        // (`<= 0` → the herd is at/below the mission's floor and yields nothing).
                        let standing_surplus =
                            (herd_biomass_before - floor * carrying_capacity.max(0.0)).max(0.0);
                        let quarry_yield = herd_hunt_yield(&herds.herds[idx], &fauna);
                        // A party carrying food home can only take the biomass it has room for. The
                        // room bounds the party's **collection** (invert the species' own
                        // `provisions_per_biomass`), so a nearly-full pack kills fewer animals rather
                        // than slaughtering one it cannot haul.
                        //
                        // **EVERY mission passes its real pack** — the floor does not enter here at
                        // all. A floor-`0` raid used to pass [`NO_CARRY_BOUND`] on the premise that
                        // driving the herd extinct makes the meat incidental, which recorded the party
                        // as hauling home everything it killed: `wasted_biomass = 0` on its hunt report
                        // and pelts accrued off the whole kill. **How deep a raid draws the herd and
                        // how much it can haul are separate questions**, and denial is what made the
                        // separation explicit: it drops the pack as a bound on what it **engages**
                        // (`stop`) and keeps it as a bound on what it **hauls**. Only an **inedible**
                        // quarry is unbounded here, and that is a fact about the *product* — see
                        // [`carry_room_biomass`].
                        let carry_room =
                            carry_room_biomass(cap - cohort.stores.get(FOOD), &quarry_yield);
                        // The quarry's engagement/retreat/fight dials, and the per-event seed —
                        // composed BEFORE the mutable borrow, exactly as the scout replenish does.
                        let engage_rate = fauna.engage_rate_for(&herds.herds[idx].species);
                        let wariness = fauna.wariness_for(&herds.herds[idx].species);
                        // The herd's own accumulated wounds ride in with the species body, so a raid
                        // spanning turns wears the quarry down (`fauna::herd_quarry_fight`).
                        let quarry_fight = fauna::herd_quarry_fight(&herds.herds[idx], &fauna);
                        let species_name = fauna
                            .species_by_display(&herds.herds[idx].species)
                            .map(|def| def.display_name.clone())
                            .unwrap_or_else(|| herds.herds[idx].species.clone());
                        let seed = fauna::retreat_seed(
                            map_seed,
                            current_turn,
                            &herds.herds[idx].id,
                            workers,
                        );
                        let herd = &mut herds.herds[idx];
                        let body_mass = herd.body_mass;
                        let outcome = expedition_take_biomass(
                            workers,
                            per_worker_biomass,
                            floor,
                            herd_biomass_before,
                            carrying_capacity,
                            body_mass,
                            carry_room,
                            engage_rate,
                            wariness,
                            quarry_fight,
                            &party_for(body_mass),
                            fauna::HuntDraw::Seeded(seed),
                            stop,
                            &mut herd.hunt_credit,
                        );
                        let take = outcome.take;
                        // The herd loses every animal killed, carried home or not (slice 8) — and
                        // keeps the damage that did not finish a body (§4.2).
                        herd.wounds = outcome.fight.wounds;
                        herd.biomass -= take.killed_biomass();
                        let herd_biomass_after = herd.biomass;
                        // **BOTH KITS ARE CHARGED FOR USE, AND ONLY FOR USE** — the resident band's
                        // rule (`docs/plan_denial_raid.md` §1.2), which the raid path did not apply
                        // at all until the take became a fight. A party that marches all turn without
                        // engaging, or waits out a herd too thin to spare a body, spends nothing;
                        // one that slaughters pays per animal killed and per unit hauled home.
                        // Each charge gated on the predicate that chose its own tier — a party
                        // sent out with no kit spends no durability on any component.
                        if let Some(kit) = party_equipment.as_mut() {
                            // **Named by QUANTUM, not by item.** Every item in the party's kit that
                            // wears per kill is charged for the kills, every item that wears per
                            // biomass hauled for the haul — so an item added to a kit is charged
                            // here without editing this call, and an item the kit does not carry is
                            // never charged at all.
                            kit.wear_kit(
                                &equipment_cfg,
                                &party_kit,
                                crate::equipment_config::WearQuantum::Kill,
                                take.killed as f32,
                            );
                            kit.wear_kit(
                                &equipment_cfg,
                                &party_kit,
                                crate::equipment_config::WearQuantum::BiomassHauled,
                                take.carried,
                            );
                        }
                        // **The fight already happened — inside the take** (§0.1). This path used
                        // to resolve the party's casualties in a *second* `resolve_fight` beside a
                        // take computed from carrying capacity, so a raid could succeed on one path
                        // while the other said the mammoth routed it. There is one resolution now,
                        // and this is where its band-side result is applied; the animal side is
                        // already off the herd as `take.killed_biomass()`. A detached party still
                        // fights at the `expedition_danger_multiplier`-scaled lethality — that rides
                        // `hunting_party.tuning`.
                        // Gated on a **death** — see the resident band's arm in `systems::labor`.
                        if outcome.fight.casualties.killed > fauna::NO_DEATHS_TO_REPORT {
                            let killed_f = outcome.fight.casualties.killed;
                            let wounded_f = outcome.fight.casualties.wounded;
                            cohort.apply_combat_casualties(scalar_from_f32(killed_f));
                            let killed_r = killed_f.round() as u32;
                            event_log.push(CommandEventEntry::new(
                                current_turn,
                                CommandEventKind::HuntDanger,
                                faction,
                                format!(
                                    "The {} hunt cost the expedition {} lives",
                                    species_name, killed_r
                                ),
                                Some(format!(
                                    "killed={:.3} wounded={:.3} species={}",
                                    killed_f, wounded_f, species_name
                                )),
                            ));
                        }
                        // **The raid's own hunt report** (§6.6) — the same facts a resident band
                        // publishes, so a consumer reads one shape whichever way the hunt was run.
                        if let Some(entry) =
                            hunt_report_event(current_turn, faction, &species_name, &outcome)
                        {
                            event_log.push(entry);
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
                        // is spent — the herd is within one body of the mission's floor, so no whole
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
                        //
                        // **A DENIAL raid has a completion of its own, and it is not zero**
                        // (`docs/plan_denial_raid.md` §1.1): the party works the herd until it is
                        // **past the point of no return** — under `ecology.collapse_fraction`, where
                        // `net_biomass_delta` zeroes the growth flow and the herd declines
                        // irreversibly with no further pressure — and then walks away. It never
                        // relaunches: there is nothing to come back for, and the pack it filled on
                        // the way is what comes home.
                        let past_recovery = fauna::herd_past_recovery(
                            herd_biomass_after,
                            carrying_capacity,
                            &ecology,
                        );
                        let (done, relaunch) = if stop == fauna::EngagementStop::Never {
                            (past_recovery, false)
                        } else if floor <= STRIP_IT_BARE {
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
                            let (message, reason) = if stop == fauna::EngagementStop::Never {
                                // **A denial raid reports the verdict, never a harvest** — it
                                // succeeded when the herd went past recovery, and what it hauled
                                // home is an aside. `floor` appears nowhere in its line
                                // (`docs/plan_denial_raid.md` §1).
                                (
                                    format!(
                                        "Denial raid drove the {} past recovery — returning home \
                                         with {}",
                                        fauna_id,
                                        describe_haul(carried.to_i64_whole(), pelts)
                                    ),
                                    "past_recovery",
                                )
                            } else if carried > scalar_zero() || pelts > 0.0 {
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
                            // **`floor` is omitted for a denial raid**, not printed as `0` — the
                            // mission carries no floor, and a `0` on the line would read as a value
                            // the player chose.
                            let detail = match stop {
                                fauna::EngagementStop::Never => {
                                    format!("status={} expedition={}", reason, entity.to_bits())
                                }
                                fauna::EngagementStop::WhenPackFull => format!(
                                    "status={} floor={} expedition={}",
                                    reason,
                                    floor,
                                    entity.to_bits()
                                ),
                            };
                            event_log.push(CommandEventEntry::new(
                                current_turn,
                                CommandEventKind::Hunt,
                                faction,
                                message,
                                Some(detail),
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

/// **No carry bound at all**, the sentinel [`fauna::quantise_animal_take`] reads as *"the pack cannot
/// be the thing that stops this"*.
///
/// It has exactly **one** meaning on the expedition path — an **INEDIBLE** quarry, whose
/// `provisions_per_biomass` is `0`, so there is no *food* pack to fill and nothing may divide through
/// the rate (`YieldAccounts::ratio_axis`'s rule: never convert through a component you have not
/// established is positive). That is a fact about the **product**.
///
/// **It is never an INTENSITY fact.** A floor-`0` raid used to pass it too, on the premise that
/// driving a herd extinct makes the meat incidental — which recorded the party as hauling home
/// *everything it killed*, so its hunt report published `wasted_biomass = 0` for a raid that left a
/// range full of carcasses and its [`Expedition::carried_trade`] accrued pelts off the whole kill.
/// **When a party stops engaging and how much it can haul are separate questions**
/// ([`fauna::EngagementStop`], `docs/plan_denial_raid.md` §1): denial answers the first and leaves
/// carry alone, and carry is never unbounded for a real party at any floor.
const NO_CARRY_BOUND: f32 = f32::INFINITY;

/// **The biomass a party still has room to haul home** — the one conversion from pack room to a
/// carry bound, shared by every take on the expedition path (the scout's roadside kill, the live
/// `Hunting` arm, and both forward simulations), so a forecast cannot bound the carry differently
/// from the take it projects.
///
/// `room` is the pack's remaining **provisions**; the species' own `provisions_per_biomass` inverts
/// it into the biomass that fits, so a nearly-full pack kills fewer animals rather than slaughtering
/// one it cannot seat. An inedible quarry answers [`NO_CARRY_BOUND`] — see there for why that is the
/// only case that does.
fn carry_room_biomass(room: Scalar, hunt_yield: &HuntYield) -> f32 {
    if hunt_yield.edible() {
        room.max(scalar_zero()).to_f32() / hunt_yield.provisions_per_biomass
    } else {
        NO_CARRY_BOUND
    }
}

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

/// **THE fold-back — the one settlement routine for a party that has come home**, shared by the
/// `Returning` arm of [`advance_expeditions`] and by an at-home `recall_expedition`, which cancels a
/// party where it stands rather than sending it on a round trip it never started.
///
/// Everything the party holds goes back into the band it was drawn from: its `working` returns to
/// the band's pool, the leftover pack lands in the band's larder, and the trade half settles through
/// [`settle_carried_trade`] into that **same** store. Returns what was banked in trade goods, for the
/// feed line.
///
/// `party` is read, never written: the caller despawns it immediately after, so writing the pack back
/// to zero would only be bookkeeping for a corpse. **Two call sites, one routine** — the two paths
/// differ only in *when* they fire, never in what a homecoming pays.
pub fn fold_party_into_band(
    party: &PopulationCohort,
    expedition: &mut Expedition,
    home: &mut PopulationCohort,
) -> Scalar {
    home.working += party.working;
    let leftover = party.stores.get(FOOD);
    if leftover > scalar_zero() {
        home.stores.add(FOOD, leftover);
    }
    let banked_trade = settle_carried_trade(expedition, home);
    home.sync_size();
    banked_trade
}

/// The `ExpeditionReturned` feed line a fold-back publishes, built in one place so the two call
/// sites of [`fold_party_into_band`] cannot describe the same event differently.
///
/// **Its detail stays `status=returned` for a cancel too.** Nothing about the *world* differs
/// between a cancel and a homecoming — the same workers, pack and pelts land in the same band — so a
/// second status word here would encode *how the fold-back was triggered* into a field that
/// otherwise reports *what happened*, and every reader would then have to know both. The cancel is
/// named where it belongs, on the `ExpeditionRecalled` **ack** that answers the button press
/// (`status=cancelled`), which is a fact about the order rather than about the world.
pub fn expedition_returned_event(
    turn: u64,
    faction: FactionId,
    at: UVec2,
    banked_trade: Scalar,
    entity: Entity,
) -> CommandEventEntry {
    CommandEventEntry::new(
        turn,
        CommandEventKind::ExpeditionReturned,
        faction,
        format!(
            "Expedition folded back into the band at ({}, {})",
            at.x, at.y
        ),
        Some(format!(
            "status=returned trade_goods={:.2} expedition={}",
            banked_trade.to_f32(),
            entity.to_bits()
        )),
    )
}

/// **Whether this party still owes its band a report.** The one thing an out-of-band fold-back cannot
/// do is flush the private [`Expedition::pending_reveal`] buffer to the faction map — that promotion
/// lives inside [`advance_expeditions`], where the visibility ledger and the elevation field are in
/// scope — so a party still holding observed tiles must take the ordinary `Returning` path, which
/// flushes and *then* folds.
///
/// Food and trade are deliberately **not** part of this test: [`fold_party_into_band`] settles both
/// exactly as the `Returning` arm does, so making a party standing in camp with a full pack wait a
/// turn would reintroduce the round trip a cancel exists to remove.
pub fn party_owes_a_report(expedition: &Expedition) -> bool {
    !expedition.pending_reveal.is_empty()
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
/// `floor` its mission named — so *more hunters take more animals in fewer-or-equal turns*, the whole
/// point of the fix (the resident band's ceiling was a per-turn *rate* then, so it was
/// worker-independent and a second hunter only added pack to fill, making the trip *longer*). When
/// the surplus is spent the herd sits at that floor and the raid comes home (the
/// `hunt_trip_forecast` / `Hunting`-arm completion checks own that).
///
/// **The floor is a continuous `f32` fraction of `K` the mission carries**, not one of four named
/// stances — `0.50` (the food peak) settles the herd on its most productive biomass, `0` strips it
/// bare. There is no table to look a stance up in: `hunt_expedition_floor` was deleted with the
/// stances (`docs/plan_harvest_floor.md` §1).
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
///
/// **The engagement bound applies to a raid exactly as it does to a resident band**
/// (`docs/plan_hunt_through_combat.md` §1 — the stages are the hunt's, not the band's; §10 exempts
/// only the pen). Without it the *same* party on the *same* herd took a different number of animals
/// purely by choosing the expedition verb — five hunters on a Red Deer herd killed 5 a turn from camp
/// and 13 a turn as a raid. So the party's reach (`fauna::animals_engaged`) and the quarry's retreat
/// (`fauna::animals_that_stay`) are resolved here and handed to the one quantiser, which is also what
/// retired this function's hand-rolled copy of the `max(1, carryable)` arithmetic.
///
/// **A detached party builds nothing**, so the engagement carries the identity build dip
/// ([`NO_BUILD_UNDERWAY_DIP`]) — a rung transition is place-bound work, and since issue #442 an
/// `ExpeditionMission::Hunt` cannot even name an improvement.
#[allow(clippy::too_many_arguments)] // the herd's state and the party's caps are all inputs
fn expedition_take_biomass(
    workers: u32,
    per_worker_biomass_capacity: f32,
    floor: f32,
    biomass: f32,
    carrying_capacity: f32,
    body_mass: f32,
    carry_room_biomass: f32,
    // The quarry's engagement/retreat/fight dials, resolved by the caller off the species — this
    // function takes resolved scalars, never a config handle (as it already does for `body_mass`).
    engage_rate: f32,
    wariness: f32,
    quarry: fauna::QuarryFight,
    // The party's own strength — kit composed in — and the tuning it fights at. **A raid is not
    // exempt from the gate**: a detached party that cannot beat the quarry's `defense` spends its
    // whole trip taking casualties and killing nothing.
    party: &fauna::HuntingParty,
    // **Live or forecast** — a live raid draws the retreat and the attack rolls from its per-event
    // seed ([`fauna::retreat_seed`]), never a shared RNG stream, or raid ordering would change
    // outcomes and rollback would stop reproducing (§6.2); a forecast reads both off their own
    // binomials, because a projection has no tick to seed with (`fauna::HuntDraw`).
    draw: fauna::HuntDraw,
    // **Does a full pack stop this party engaging?** — the one line a denial raid changes
    // (`docs/plan_denial_raid.md` §1). It reaches only the quantiser and the bound reading; every
    // other term above is the hunt's, unchanged.
    stop: fauna::EngagementStop,
    credit: &mut f32,
) -> HuntOutcome {
    if !body_mass.is_finite() || body_mass <= 0.0 {
        debug_assert!(
            false,
            "body_mass must be finite and positive; got {body_mass}"
        );
        return HuntOutcome {
            take: AnimalTake::default(),
            fight: fauna::HuntFight {
                brought_down: 0.0,
                casualties: fauna::FightCasualties::default(),
                fought: false,
                wounds: quarry.wounds,
            },
            engaged: NOTHING_ENGAGED,
            fled: NOTHING_ENGAGED,
            bound: fauna::HuntTakeBound::Floor,
        };
    }
    // The standing surplus above the mission's floor — everything the raid may take.
    let floor = floor * carrying_capacity.max(0.0);
    let standing_surplus = (biomass - floor).max(0.0);
    // Bank the party's processing throughput; the bank meters WHEN the next whole animal is ready,
    // never how much of it is carried. Capped at the surplus so it never funds a kill below the floor.
    let throughput = (workers as f32 * per_worker_biomass_capacity).max(0.0);
    let rate = throughput.min(standing_surplus);
    let ceiling = (*credit + rate).clamp(0.0, standing_surplus);
    let room = carry_room_biomass.max(0.0);
    // **Engagement, then retreat, then the quantiser** — stages 1 and 2 of
    // `docs/plan_hunt_through_combat.md` §1, in the same order `systems::hunt_take` runs them.
    // Wariness `0` makes the retreat an exact identity that consumes no randomness, so a raid is
    // byte-identical until values are authored.
    let engaged = fauna::animals_engaged(workers, engage_rate, NO_BUILD_UNDERWAY_DIP)
        // **Restraint is free** — the mission's floor bounds what the party goes after, so a raid at
        // its floor takes no casualties for animals it was never going to kill (§1).
        .min(fauna::animals_affordable(ceiling, body_mass));
    let stayed = fauna::animals_that_stay(engaged, wariness, draw);
    // **The fight decides the kill** (§4) — the same resolution the resident band runs.
    // A detached party builds nothing, so its crew carries the identity dip ([`NO_BUILD_UNDERWAY_DIP`]).
    let fight = fauna::resolve_hunt_fight(
        stayed,
        workers as f32 * NO_BUILD_UNDERWAY_DIP,
        party,
        &quarry,
        draw,
    );
    // Whole animals through **the** quantiser: as many as the bank has readied, bounded by what the
    // party brought down and by what the pack can seat but never below one — so if the pack cannot
    // seat one (`carryable == 0`) while the herd has banked one, the party still kills ONE and wastes
    // what it cannot haul, and with no banked animal it kills nothing and waits (the true no-surplus
    // case).
    let take = fauna::quantise_animal_take(ceiling, room, body_mass, fight.brought_down, stop);
    // Drain the bank by what was KILLED (carried + wasted), not merely carried — you cannot un-kill the
    // animal you could not haul. Cap at the surplus so it can't grow unbounded at the floor (surplus <
    // body ⇒ no kill ⇒ the bank would otherwise climb every turn). `0 ≤ credit ≤ surplus`.
    *credit = (*credit + rate - take.killed_biomass())
        .max(0.0)
        .min(standing_surplus);
    HuntOutcome {
        take,
        fight,
        engaged,
        fled: (engaged - stayed).max(NOTHING_ENGAGED),
        // Read off the very terms the quantiser above was handed, so the report cannot name a bound
        // the take did not hit — plus `standing_surplus`, which is what separates *the herd has
        // nothing left* from *the bank has not readied a body yet*. The two are the same number only
        // once the bank has caught up with the surplus; until then `ceiling` is the party's limit and
        // reporting it as the floor would blame the herd for the party's own throughput.
        bound: fauna::hunt_take_bound(
            ceiling,
            standing_surplus,
            room,
            body_mass,
            stayed,
            fight.brought_down,
            stop,
        ),
    }
}

/// The **provisions a hunting party actually lands in its larder per turn** at a herd's current state
/// — the real take ([`expedition_take_biomass`] through the species' [`HuntYield`], no output
/// multiplier), ignoring only carry room (which bites solely on the final partial turn, and `ceil()`
/// already accounts for that). `0` for an **inedible** species (a wolf is not food) — since #337 that
/// is a fact about the *species*, never about the policy: **Eradicate pays the windfall** like every
/// other rung. This is what the client's pre-launch readout is pinned to
/// (`core_sim/tests/expedition_hunt.rs`).
/// The quarry's engagement/retreat dials come in resolved (`FaunaConfig::engage_rate_for` /
/// `wariness_for`) alongside its [`HuntYield`], and the caller composes the retreat seed the way the
/// take path does (`fauna::HuntDraw::Seeded`) — this function reads no config handle, only numbers.
#[allow(clippy::too_many_arguments)] // the herd's state, the labor tier and the species vector are all inputs
pub fn expedition_take_provisions(
    workers: u32,
    floor: f32,
    biomass: f32,
    carrying_capacity: f32,
    body_mass: f32,
    labor: &LaborConfig,
    hunt_yield: HuntYield,
    engage_rate: f32,
    wariness: f32,
    quarry: fauna::QuarryFight,
    party: &fauna::HuntingParty,
    draw: fauna::HuntDraw,
) -> f32 {
    // A single-turn preview starting from an empty bank (this readout is the client's per-turn rate,
    // not a specific banked turn) — the forward-sim `hunt_trip_forecast` is the one pinned to actual.
    let mut credit = 0.0_f32;
    let outcome = expedition_take_biomass(
        workers,
        labor.hunt.per_worker_biomass_capacity,
        floor,
        biomass,
        carrying_capacity,
        body_mass,
        // Carry room bites only on the final partial turn, and `ceil()` already accounts for it.
        f32::INFINITY,
        engage_rate,
        wariness,
        quarry,
        party,
        draw,
        // **This readout is a HUNT's per-turn rate** — the client's per-herd preview, quoted for the
        // hunting verb. A denial raid's readout is its own forecast (`denial_forecast`).
        fauna::EngagementStop::WhenPackFull,
        &mut credit,
    );
    // Quantized onto the larder's `Scalar` grid, exactly as the real take lands there.
    scalar_from_f32(
        hunt_yield
            .apply(outcome.take.carried, EXPEDITION_OUTPUT_MULTIPLIER)
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
/// **One turn's hunt, both sides of it** — what came home, and what the fight cost.
///
/// The two used to be resolved by two unrelated code paths that could disagree
/// (`docs/plan_hunt_through_combat.md` §0.1); they are one resolution now, so they come back
/// together and no caller can apply one without the other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HuntOutcome {
    /// Killed / carried / wasted, in biomass.
    pub take: AnimalTake,
    /// The fight the take resolved through — its casualties, and whether it was a fight at all.
    pub fight: fauna::HuntFight,
    /// **Animals the party brought into contact** (`fauna::animals_engaged`, floored by the
    /// escapement room) — the first of the hunt report's facts
    /// (`docs/plan_hunt_through_combat.md` §6.6).
    ///
    /// It is on the outcome rather than on [`fauna::HuntFight`] because engagement happens **before**
    /// the fight and is not the fight's to know: the resolver is handed the animals that *stayed*.
    pub engaged: f32,
    /// **Animals that broke off before contact** — `engaged − stayed`, the retreat stage's own
    /// output (§3). Real on every wild hunt since slice 7 authored the roster's `wariness` (§3.1);
    /// `0` only where the retreat is an identity — a pen, a plant, or a species held at `0` by
    /// config.
    pub fled: f32,
    /// **Which of the four bounds ended the take** ([`fauna::hunt_take_bound`]) — engagement, the
    /// floor, carry, or the fight.
    pub bound: fauna::HuntTakeBound,
}

/// A party that engaged nothing — the degenerate reading of [`HuntOutcome::engaged`] / `fled`, named
/// because a bare `0.0` beside a biomass field reads as "no biomass" rather than "no animals".
pub(crate) const NOTHING_ENGAGED: f32 = 0.0;

/// **THE HUNT REPORT** (`docs/plan_hunt_through_combat.md` §6.6) — one hunt's facts as a feed entry.
/// `None` when no hunt happened (nothing was engaged), which is a **fact** gate and not an
/// importance one.
///
/// # Facts, never a composed judgement
///
/// Issue #272's notification system owns importance and phrasing; the hunt owns what happened. So
/// every number rides the `key=value` detail — the form the feed already parses — and the **label
/// composes nothing but the species**: no adjective, no "successful", no severity. Emitting
/// presentation-ready text here would bake this arc's guesses about an importance ladder into the
/// sim, and #272 would then have to unpick prose to recover the numbers.
///
/// | token | meaning |
/// |---|---|
/// | `engaged` | animals brought into contact (§2) |
/// | `fled` | of those, how many broke off before contact (§3) — real since the roster's wariness was authored |
/// | `killed` | whole animals put down |
/// | `carried_biomass` / `wasted_biomass` | what came home, and what was left on the range |
/// | `hunters_killed` / `hunters_wounded` | what it cost the party, fractional as the resolver reports it |
/// | `bound` | **which of the four limits ran out first** ([`fauna::HuntTakeBound`]) |
/// | `species` | the display name, never the internal herd id |
///
/// **`species` is LAST, and it has to be.** A display name contains spaces, so in a space-delimited
/// `key=value` grammar it can only be the trailing remainder — which is where the `HuntDanger` line
/// beside it already puts the same value. A consumer reads it as *everything after `species=`*.
///
/// **`carried_biomass` / `wasted_biomass` are BIOMASS, and the token says so.** Provisions is a
/// *conversion* of it that differs by path (a raid applies no output multiplier, a band applies its
/// own), and the food a band actually banked is already reported on its assignment row; the biomass
/// is the unambiguous physical fact this event owes.
///
/// **`hunters_wounded` is why [`CommandEventKind::HuntDanger`] did not have to widen.** That line is
/// gated on a **death** because the hunt's baseline injury risk (§4.6) makes *every* engagement
/// produce some `wounded`, so gating it on any casualty would push a "cost 0 lives" line for every
/// band every turn. The wounded are not invisible — they are here, on every hunt, as a number.
pub fn hunt_report_event(
    tick: u64,
    faction: FactionId,
    species_name: &str,
    outcome: &HuntOutcome,
) -> Option<CommandEventEntry> {
    if outcome.engaged <= NOTHING_ENGAGED || !outcome.engaged.is_finite() {
        // Nothing was stalked — a pen's tend branch, or a turn the party never reached an animal.
        return None;
    }
    Some(CommandEventEntry::new(
        tick,
        CommandEventKind::HuntReport,
        faction,
        format!("The {species_name} hunt"),
        Some(format!(
            "engaged={:.0} fled={:.0} killed={} carried_biomass={:.3} wasted_biomass={:.3} \
hunters_killed={:.3} hunters_wounded={:.3} bound={} species={}",
            outcome.engaged,
            outcome.fled,
            outcome.take.killed,
            outcome.take.carried,
            outcome.take.wasted,
            outcome.fight.casualties.killed,
            outcome.fight.casualties.wounded,
            outcome.bound.as_str(),
            species_name,
        )),
    ))
}

#[allow(clippy::too_many_arguments)] // the ecology, the ladder and the caller's caps are all levers
pub fn hunt_take(
    herd: &mut Herd,
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
    per_worker_biomass_capacity: f32,
    // The hunters' own strength — kit composed in — and the tuning they fight at. The take's kill
    // arm IS this fight (`docs/plan_hunt_through_combat.md` §4), so a party that cannot beat the
    // quarry's `defense` comes home with nothing however much the herd could spare.
    party: &fauna::HuntingParty,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    carry_room_biomass: f32,
    // **Live or forecast** — a live hunt draws the retreat and the attack rolls from its per-event
    // seed (`fauna::retreat_seed`), never a shared RNG stream, or hunt ordering would change outcomes
    // and rollback would stop reproducing (§6.2). See `fauna::HuntDraw`.
    draw: fauna::HuntDraw,
) -> HuntOutcome {
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
    // **Engagement, then retreat, then the quantiser** — stages 1 and 2 of
    // `docs/plan_hunt_through_combat.md` §1. Wariness `0` makes the retreat an exact identity that
    // consumes no randomness, so this is byte-identical until values are authored.
    let engaged = fauna::animals_engaged(
        workers,
        fauna.engage_rate_for(&herd.species),
        ladder.build_dip(improvement),
    );
    // **Restraint is FREE, and the floor is what makes it so** (`docs/plan_hunt_through_combat.md`
    // §1): the escapement floor bounds what the party *goes after*, not what it declines to kill
    // afterwards. A crew at its floor that engaged normally would take casualties and wear its kit
    // and then hand nothing back — and killing without taking is denial, not restraint.
    let engaged = engaged.min(fauna::animals_affordable(ceiling, herd.body_mass));
    let stayed = fauna::animals_that_stay(engaged, fauna.wariness_for(&herd.species), draw);
    // **The fight decides the kill** — stage 3, and the arm that used to be a bespoke hunt formula.
    // The quarry comes in carrying **this herd's** accumulated wounds (`fauna::herd_quarry_fight`),
    // so a party below the one-turn threshold wears the animal down over several turns instead of
    // bouncing off it forever (`docs/plan_hunt_through_combat.md` §4.2).
    let fight = fauna::resolve_hunt_fight(
        stayed,
        workers as f32 * ladder.build_dip(improvement),
        party,
        &fauna::herd_quarry_fight(herd, fauna),
        draw,
    );
    // ...and the ledger goes straight back onto the herd, before anything can early-return past it.
    herd.wounds = fight.wounds;
    let take = fauna::quantise_animal_take(
        ceiling,
        collection,
        herd.body_mass,
        fight.brought_down,
        // A resident band (and a scout's roadside kill) hunts: hunters do not kill what they cannot
        // use. Denial removes exactly this clause, and it is a *mission*, so it never reaches here.
        fauna::EngagementStop::WhenPackFull,
    );
    // **The herd loses every animal KILLED, not merely what was carried** — you cannot un-kill the
    // mammoth you could not haul. That is the waste, and it is `take.wasted`.
    herd.biomass -= take.killed_biomass();
    HuntOutcome {
        take,
        fight,
        engaged,
        fled: (engaged - stayed).max(NOTHING_ENGAGED),
        // The same four terms the quantiser was handed — one reading, not a second computation of
        // what "affordable" and "carryable" mean. **The ceiling is passed twice on purpose**: a
        // resident band banks no throughput, so the number bounding its take *is* the herd's
        // escapement room, and `HuntTakeBound::Throughput` is unreachable here by construction.
        bound: fauna::hunt_take_bound(
            ceiling,
            ceiling,
            collection,
            herd.body_mass,
            stayed,
            fight.brought_down,
            fauna::EngagementStop::WhenPackFull,
        ),
    }
}

/// **WHICH of the raid's four stops actually ended the trip.** A trip length alone cannot say which
/// of them bound — *"you fill the pack in 4 turns; the herd never reaches the floor"* and *"you
/// reach the floor in 2 turns with the pack a third full"* are different decisions carrying the same
/// kind of number. The forecast names the bound so the client composes nothing.
///
/// The variants are the raid's existing completion terms, not new ones: the party-side stop
/// ([`Self::PackFull`]), the herd-side stop ([`Self::Floor`]), the herd's death
/// ([`Self::HerdLost`]), and running out of horizon ([`Self::Horizon`]).
///
/// **A fifth variant, `FillTarget`, was retired with the player-set fill target it named** — see
/// `docs/plan_hunt_through_combat.md` §5.2, marked retired in place. It was the *same* stop as
/// [`Self::PackFull`] under a player's name (a target replaced the pack's capacity rather than
/// adding a stop), so removing the lever removed the variant with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HuntTripBound {
    /// The **pack** filled — no room left.
    PackFull,
    /// The herd reached the mission's **floor** — the standing surplus is spent, and the party comes
    /// home with whatever it has.
    Floor,
    /// The herd was driven under its `extinction_floor` and is **gone**; there is nothing left to
    /// raid. **It carries a completion turn like every other stop** — the live arm's lost-herd guard
    /// turns the party for home in the same turn's Population stage that Logistics despawned the herd
    /// in, so the raid *did* end, by emptying the range rather than by filling a pack. This is the
    /// ordinary end of a floor-`0` raid, which has no party-side stop at all.
    HerdLost,
    /// **None of the above within `hunt.forecast_horizon_turns`** — the raid was still going when the
    /// projection ran out. This is the **only** bound with no completion turn, and therefore exactly
    /// what a `turns_to_fill` of `None` means.
    Horizon,
}

impl HuntTripBound {
    /// Stable wire/snapshot key (client discriminator), the `as_str` convention every wire enum in
    /// this crate uses.
    pub fn as_str(self) -> &'static str {
        match self {
            HuntTripBound::PackFull => "pack_full",
            HuntTripBound::Floor => "floor",
            HuntTripBound::HerdLost => "herd_lost",
            HuntTripBound::Horizon => "horizon",
        }
    }
}

/// What a hunting party can expect from a herd at a given **floor**, computed **at launch** so the
/// player sees the trip's economics before committing workers (`handle_send_hunt_expedition`), and
/// exported per herd × **sampled floor** × party size in the snapshot (the floor is continuous, so
/// the wire carries marks on it) so the outfit UI can show it *before* the commit.
/// Produced by [`hunt_trip_forecast`], a **bounded forward simulation** of the trip.
pub struct HuntTripForecast {
    /// Turns of hunting (once in reach — travel is **not** counted) until the **raid completes**. A
    /// greedy raid ends when the pack fills **OR** the standing surplus is spent (the herd sits at the
    /// mission's floor) **OR** the herd is lost — whichever comes first — so this is *"turns until the
    /// party comes home"*, **not** *"turns until the pack is full"* (a full-herd Sustain raid for a big
    /// party leaves `K/2` with a partial pack, and that is a *successful* short trip). **A raid that
    /// ends by driving the herd extinct reports its turn like any other** — see
    /// [`HuntTripBound::HerdLost`]; `None` is reserved for the raid that was still going when the
    /// horizon ran out, which is [`HuntTripBound::Horizon`] and nothing else. The caller distinguishes the honest cases
    /// via the other fields: it **brings home no food** (`delivers_food == false` — an *inedible*
    /// quarry, e.g. a wolf), the herd had **no surplus to take** (`animals_taken == 0` — at/below the
    /// mission's floor), or it only trickle-fills off regrowth (a slow breeder a big party can neither
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
    /// wasted). `0` = the herd is at/below the mission's floor and has no surplus to raid (the honest
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
    /// **Which stop ended the trip** — see [`HuntTripBound`]. Paired with `turns_to_fill`, which says
    /// *when*: the two together are the readout the fill target turns on, because the same "4 turns"
    /// means "you got the animals you asked for" or "the herd ran out" depending on this.
    ///
    /// [`HuntTripBound::Horizon`] is **exactly** the `turns_to_fill == None` case, with no exception:
    /// [`HuntTripBound::HerdLost`] reports the turn the herd went, because the live arm's lost-herd
    /// guard turns the party for home on that same turn. A raid that finishes by emptying the range
    /// finishes; only a raid that was still going when the projection ran out has no turn to give.
    pub bound: HuntTripBound,
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

/// **The draw a raid FORECAST resolves at — the expectation, never a stand-in seed**
/// (`docs/plan_hunt_through_combat.md` §6.4).
///
/// This replaced a `forecast_retreat_seed(herd, workers)` that composed a real per-event seed out of
/// zeros for the two world terms a projection cannot know (`map_seed` and the tick). That was stable
/// and reproducible, and it was still **wrong in kind**: it drew a *sample* and presented it as the
/// answer, so as soon as a stochastic stage was authored the preview would report one draw while the
/// take paid a different one, with no way for a reader to tell them apart.
///
/// A projection cannot know a future tick — that is a fact about time, not a gap to be filled — so
/// it reads the take's distribution instead. When it landed, at `wariness 0` / `hit_chance 1.0`, both
/// stages took their exact identities, so it was bit-identical to what the old seed produced and no
/// pinned raid number moved. Slice 7 authored the roster's wariness (§3.1), so the retreat half is
/// now a real distribution and a raid preview quotes its **expectation** — which is precisely the
/// promise the retired seed could not have kept.
const RAID_FORECAST_DRAW: fauna::HuntDraw = fauna::HuntDraw::EXPECTED;

/// Forecast a hunting **raid** by simulating it forward turn by turn against the herd's own ecology,
/// on the sim's arithmetic, until the party comes home — the pack fills, the **standing surplus is
/// spent** (the herd sits at the mission's floor), or the herd is lost — or `hunt.forecast_horizon_turns`
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
    // **The party's per-hunter HAUL rate** — its chosen kit's *sled* tier, resolved by the caller
    // through `EquipmentConfig::hunt_per_worker_biomass_capacity`. It replaced the whole
    // `LaborConfig` this projection used to take purely to read the **equipped** rate off it: a raid
    // quoted for a sledless party must project the sledless haul, and this is the only term in the
    // projection the kit moves that `party` does not already carry.
    per_worker_haul: f32,
    expedition: &ExpeditionConfig,
    // The party that would go — its per-hunter profile and the tuning it fights at. The take resolves
    // through the fight (`docs/plan_hunt_through_combat.md` §4), so a raid quoted for a bare-handed
    // party must project the bare-handed take.
    party: &fauna::HuntingParty,
) -> HuntTripForecast {
    // The pre-launch estimate: an EMPTY pack (the party has not left yet). See
    // `hunt_trip_forecast_seeded` for the in-flight (partial-pack) variant.
    hunt_trip_forecast_seeded(
        workers,
        herd,
        floor,
        fauna,
        per_worker_haul,
        expedition,
        party,
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
    // The party's per-hunter haul rate — see `hunt_trip_forecast`.
    per_worker_haul: f32,
    expedition: &ExpeditionConfig,
    party: &fauna::HuntingParty,
    initial_larder: Scalar,
) -> HuntTripForecast {
    // The quarry's yield vector — **the species decides the product**, the policy only the intensity.
    let hunt_yield = fauna::herd_hunt_yield(herd, fauna);
    let delivers_food = hunt_yield.edible();
    let delivers_trade = hunt_yield.tradeable();
    // The party-side stop — the pack, resolved exactly as the `ExpeditionPhase::Hunting` arm
    // resolves it, so the projection cannot quote a different load than the raid brings home.
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
            // No party, no projection — the raid does not end, it never starts.
            bound: HuntTripBound::Horizon,
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
    // The quarry's engagement/retreat dials, resolved ONCE: the crew does not change size
    // mid-projection and the quarry is never re-speciated. The retreat is read at its expectation
    // rather than drawn — see [`RAID_FORECAST_DRAW`] for why a projection cannot draw it at all.
    let engage_rate = fauna.engage_rate_for(&quarry.species);
    let wariness = fauna.wariness_for(&quarry.species);
    // **The wounds are NOT resolved once** — they are the one term that changes every projected turn,
    // and a projection that froze them could not see a multi-turn kill at all (§4.2). Seeded from the
    // live herd, then re-carried from each simulated turn's result below.
    let mut quarry_fight = fauna::herd_quarry_fight(&quarry, fauna);
    let mut larder = initial_larder;
    let mut first_turn_provisions = 0.0_f32;
    let mut animals_taken = 0u32;
    let mut delivered_food = 0.0_f32;
    let mut delivered_trade = 0.0_f32;
    let mut wasted_food = 0.0_f32;
    // Which stop the projection ran into. It starts at the honest "still going when the projection
    // ran out" and is overwritten by whichever of the raid's stops fires first.
    let mut bound = HuntTripBound::Horizon;
    // The turn the party comes home on, for the stop that `break`s out of the loop rather than
    // returning from inside it. `None` until one fires — and it stays `None` for exactly one stop,
    // [`HuntTripBound::Horizon`], which is what makes `turns_to_fill == None` and "ran out of
    // horizon" the same statement.
    let mut completed_on: Option<u32> = None;

    for turn in 1..=horizon {
        // Logistics: the herd's ecology moves first (regrowth, or the depensation decline), exactly
        // as `advance_herds` runs before the Population stage's take.
        fauna::regrow_biomass(&mut quarry, fauna);
        if quarry.biomass <= ecology.extinction_floor * capacity {
            // `advance_herds` would despawn it here — a lost herd ends the raid.
            //
            // **And it ends it ON THIS TURN, which is a completion like any other.** The live arm's
            // lost-herd guard flips the party to `Returning` the moment `HerdRegistry::find` comes
            // back empty, and that happens in the *same* turn's Population stage as the Logistics
            // despawn — so the party does come home, and there is a turn to name. Reporting `None`
            // here (as this branch used to) published the wire's "never completes" sentinel for the
            // one raid whose whole purpose is to finish by emptying the range: a floor-`0` row read
            // as a doomed trip while the sim ran it to a successful extinction. The denial twin
            // ([`DenialOutcome::HerdLost`]) always reported its turn; this is the hunt saying the
            // same thing.
            bound = HuntTripBound::HerdLost;
            completed_on = Some(turn);
            break;
        }

        // Population: the `Hunting` arm's greedy take, through the same helper, bounded by the carry
        // room left in the pack — converted back into biomass through the **same**
        // [`carry_room_biomass`] the arm converts it with, so the projection cannot bound the haul
        // differently from the take it projects. The floor is not a term in it at any depth: a
        // floor-`0` raid hauls its real pack like every other, and only an **inedible** quarry is
        // unbounded (a *product* fact, stated rather than left to the `x / 0.0 = inf` the division
        // would otherwise produce).
        let carry_room = carry_room_biomass(cap - larder, &hunt_yield);
        let outcome = expedition_take_biomass(
            workers,
            per_worker_haul,
            floor,
            quarry.biomass,
            capacity,
            quarry.body_mass,
            carry_room,
            engage_rate,
            wariness,
            quarry_fight,
            party,
            RAID_FORECAST_DRAW,
            // This is the **hunt's** projection; a denial raid is projected by [`denial_forecast`].
            fauna::EngagementStop::WhenPackFull,
            &mut quarry.hunt_credit,
        );
        let take = outcome.take;
        // Carry the fight forward: this turn's unfinished damage is next turn's head start.
        quarry_fight = quarry_fight.with_wounds(outcome.fight.wounds);
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
        //
        // **A floor-`0` raid has NO party-side stop**, which is what `raid_ends_on_its_pack` gates:
        // the live arm answers `(done, relaunch) = (false, false)` there and grinds on until the
        // lost-herd guard, so a projection that came home on a full pack would quote a homecoming
        // the raid does not make — and, worse, would stop counting the moment the party stops
        // *delivering*, hiding every carcass it goes on to leave on the range. The herd-side stop is
        // gated on the same fact one line below, and always was.
        let raid_ends_on_its_pack = floor > STRIP_IT_BARE;
        let food_per_animal = hunt_yield
            .apply(quarry.body_mass, EXPEDITION_OUTPUT_MULTIPLIER)
            .provisions;
        let pack_full = raid_ends_on_its_pack && larder >= cap;
        let pack_cannot_seat_another = raid_ends_on_its_pack
            && larder > scalar_zero()
            && (cap - larder).to_f32() < food_per_animal;
        // Mirrors the live arm's completion exactly: Eradicate's floor is `0`, so it has no standing
        // surplus to spend and ends via the herd-lost break above instead.
        let surplus_spent =
            floor > STRIP_IT_BARE && (quarry.biomass - floor_biomass) < quarry.body_mass;
        if pack_full || pack_cannot_seat_another || surplus_spent {
            return HuntTripForecast {
                turns_to_fill: Some(turn),
                delivers_food,
                delivers_trade,
                first_turn_provisions,
                animals_taken,
                delivered_food,
                wasted_food,
                delivered_trade,
                // **The herd-side stop wins a tie**, mirroring the live arm testing `done` before
                // `relaunch`: when the pack fills on the very turn the surplus runs out, the fact
                // that decides whether to send the party back is that there is nothing left to send
                // it for. Otherwise it is the party-side stop, which is the pack.
                bound: if surplus_spent {
                    HuntTripBound::Floor
                } else {
                    HuntTripBound::PackFull
                },
            };
        }
    }

    HuntTripForecast {
        // `Some` when the loop broke on a **lost herd** (the party comes home on that turn), `None`
        // when it simply ran out of horizon. `bound` names which, and the two can no longer be
        // confused: `turns_to_fill == None` is now exactly [`HuntTripBound::Horizon`].
        turns_to_fill: completed_on,
        delivers_food,
        delivers_trade,
        first_turn_provisions,
        animals_taken,
        delivered_food,
        wasted_food,
        delivered_trade,
        bound,
    }
}

/// **How a denial raid ended** — the denial twin of [`HuntTripBound`], and the reason
/// `turns_to_collapse` is never a silent `None` (`docs/plan_denial_raid.md` §3): *"when the party
/// cannot get there at all, it must say **that**, not show a blank."*
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialOutcome {
    /// **The herd went past the point of no return** — under `ecology.collapse_fraction`, where the
    /// growth flow is zeroed and it declines irreversibly at `collapse_rate` with the party gone.
    /// This is the mission succeeding, and it is what `turns_to_collapse` counts to.
    PastRecovery,
    /// The herd was driven under its `extinction_floor` and **despawned** in the same projection —
    /// the raid succeeding outright rather than walking away from a doomed remnant. It reports a
    /// completion turn like [`Self::PastRecovery`], because the party comes home on it.
    HerdLost,
    /// **The party cannot get there** — at the end of the projection its kills per turn are at or
    /// below the herd's own regrowth (§3), so the herd sits at an equilibrium the raid cannot push
    /// past. A wary herd is the shipped way to reach this: the animals that break off before contact
    /// cost the party hunter-turns and the herd nothing.
    Repelled,
    /// **Still grinding it down when the projection ran out** — the raid is winning (kills outpace
    /// regrowth) but had not crossed `collapse_fraction` within `hunt.forecast_horizon_turns`.
    /// Distinct from [`Self::Repelled`], which is a verdict about the party rather than about the
    /// clock.
    Horizon,
}

impl DenialOutcome {
    /// **Every variant**, and therefore the list [`Self::from_wire`] parses against. It is the one
    /// place the set is enumerated: `as_str` is an exhaustive `match`, so a new variant must be
    /// given a key to compile, and adding it here is what makes that key *readable* again.
    pub const ALL: [DenialOutcome; 4] = [
        DenialOutcome::PastRecovery,
        DenialOutcome::HerdLost,
        DenialOutcome::Repelled,
        DenialOutcome::Horizon,
    ];

    /// Stable wire/snapshot key (client discriminator), the `as_str` convention every wire enum in
    /// this crate uses.
    pub fn as_str(self) -> &'static str {
        match self {
            DenialOutcome::PastRecovery => "past_recovery",
            DenialOutcome::HerdLost => "herd_lost",
            DenialOutcome::Repelled => "repelled",
            DenialOutcome::Horizon => "horizon",
        }
    }

    /// **The inverse of [`Self::as_str`]** — `None` for a key no variant publishes.
    ///
    /// It exists so a consumer holding the wire `String` (a `DenialEstimateState::outcome` row) can
    /// ask the enum's own questions — [`Self::succeeded`] above all — instead of hand-writing a
    /// second list of keys at the call site. That second list is exactly how the two directions
    /// drift: `snapshot::subsistence::seeded_denial_party` once tested `!= "repelled"`, which
    /// silently counted a [`Self::Horizon`] row (a raid the projection never saw finish) as a party
    /// that works, and the launch sheet opened on it.
    ///
    /// **It reads [`Self::ALL`] rather than matching the strings**, so the round trip is total by
    /// construction and no key is spelled twice.
    pub fn from_wire(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|outcome| outcome.as_str() == key)
    }

    /// Did the raid achieve what it was sent to do? Both success readings answer the same question
    /// the player asked, so no consumer has to enumerate them.
    pub fn succeeded(self) -> bool {
        matches!(self, DenialOutcome::PastRecovery | DenialOutcome::HerdLost)
    }
}

/// **What a denial raid does to a herd, before it is launched** — the mission's readout and the
/// denial analogue of [`HuntTripForecast`] (`docs/plan_denial_raid.md` §1.1). Produced by
/// [`denial_forecast`], a bounded forward simulation on the *same* [`expedition_take_biomass`] the
/// live raid resolves through, so the preview cannot quote a raid the sim does not run.
///
/// **The headline is `turns_to_collapse`, not a food total.** A raid delivers a rounding error and
/// wastes the rest; what the player is deciding is whether this party can push this herd past
/// recovery, and how long it takes.
pub struct DenialForecast {
    /// **Turns until the herd is past recovery** at the take's expectation — and therefore turns
    /// until the party comes home, because that is when a denial raid completes. `None` = it never
    /// got there within `hunt.forecast_horizon_turns`; [`Self::outcome`] says which kind of never.
    pub turns_to_collapse: Option<u32>,
    /// The **optimistic** end of the range — the take resolved `+forecast_range_sigmas`, so more
    /// animals stay and more strikes land, and the herd falls **sooner**
    /// (`docs/plan_hunt_through_combat.md` §6.4).
    pub turns_to_collapse_low: Option<u32>,
    /// The **pessimistic** end — `−forecast_range_sigmas`, fewer kills, later or never. A `None`
    /// here beside a `Some` likely is the honest *"on a bad run this party does not get there"*.
    pub turns_to_collapse_high: Option<u32>,
    /// **Why the projection ended** — never a silent `None`.
    pub outcome: DenialOutcome,
    /// Whole animals the raid **kills** before it walks away. The number the mission is really
    /// about.
    pub animals_killed: u32,
    /// Food the party lands in its pack over the raid — **small, and non-zero**: the raid banks
    /// whatever it can haul on the way home.
    pub delivered_food: f32,
    /// Food killed and left on the range — **the bulk of a raid's take**, and stated rather than
    /// hidden (§3).
    pub wasted_food: f32,
    /// The trade half of the same carried biomass. For an inedible quarry (a wolf — a legitimate
    /// denial target) this is the whole payload.
    pub delivered_trade: f32,
    /// **Trade goods killed and left on the range** — the twin of [`Self::wasted_food`], and on an
    /// **inedible** quarry the only one of the two that can be non-zero.
    ///
    /// It is what makes the readout honest on the target the mission is clearest about: a Grey Wolf
    /// Pack pays `provisions_per_biomass == 0`, so a food-only waste line reports `0` beside a large
    /// [`Self::animals_killed`] on a raid whose waste is *total*. Denial's whole readout is what it
    /// destroys and does not bring home (`docs/plan_denial_raid.md` §3), so it has to be stated per
    /// **product** — the same widening issue #337 made everywhere else.
    pub wasted_trade: f32,
}

/// One quantile's worth of [`denial_forecast`]'s forward simulation.
struct DenialProjection {
    turns: Option<u32>,
    outcome: DenialOutcome,
    animals_killed: u32,
    delivered_food: f32,
    wasted_food: f32,
    delivered_trade: f32,
    wasted_trade: f32,
}

/// **The pre-launch denial readout**, evaluated at three quantiles of the take's own distribution —
/// the shape slice 6 established for every yield readout (`docs/plan_hunt_through_combat.md` §6.4),
/// applied to a turn count instead of a biomass.
///
/// **`low` is the FEWEST turns.** More animals staying and more strikes landing is the *optimistic*
/// draw for a raid, and it drives the herd under sooner — so the `+sigmas` run produces
/// [`DenialForecast::turns_to_collapse_low`] and the `−sigmas` run the high end. Getting that
/// backwards would report a range that widens in the wrong direction on a wary herd, which is
/// exactly the quarry the range exists for.
///
/// **A range is a POINT when the three agree**, the same reading every other range on the wire asks
/// for: at `wariness 0` and `hit_chance 1.0` every stage takes its exact identity and all three runs
/// return the same turn.
///
/// `range_sigmas` is `combat_config.forecast_range_sigmas`, a **readout width** — nothing the sim
/// resolves reads it, so widening the band cannot move an animal.
#[allow(clippy::too_many_arguments)] // every config the forward simulation reads is a lever
pub fn denial_forecast(
    workers: u32,
    herd: &Herd,
    fauna: &FaunaConfig,
    // The party's per-hunter haul rate — its kit's *sled* tier, in the slot the whole `LaborConfig`
    // used to occupy purely to be read for it. It moves only what comes home (`delivered_food` /
    // `wasted_food`); the verdict is decided by kills, which the fight owns.
    per_worker_haul: f32,
    expedition: &ExpeditionConfig,
    // The party that would go — its per-hunter profile (kit composed in) and the tuning it fights
    // at. A raid quoted for a bare-handed party must project the bare-handed slaughter, which on a
    // defended quarry is none at all.
    party: &fauna::HuntingParty,
    range_sigmas: f32,
) -> DenialForecast {
    let at = |sigmas: f32| {
        denial_projection_at(
            workers,
            herd,
            fauna,
            per_worker_haul,
            expedition,
            party,
            fauna::HuntDraw::Quantile { sigmas },
        )
    };
    let likely = at(combat::EXPECTED_STRIKES);
    DenialForecast {
        turns_to_collapse: likely.turns,
        turns_to_collapse_low: at(range_sigmas.abs()).turns,
        turns_to_collapse_high: at(-range_sigmas.abs()).turns,
        outcome: likely.outcome,
        animals_killed: likely.animals_killed,
        delivered_food: likely.delivered_food,
        wasted_food: likely.wasted_food,
        delivered_trade: likely.delivered_trade,
        wasted_trade: likely.wasted_trade,
    }
}

/// **How much of the projection the headway verdict is read over** — the second half
/// (`forecast_horizon_turns / 2`). Expressed as a divisor of the horizon rather than as a turn count
/// so it scales with the one lever that sets the projection's length, and wide enough that the
/// float noise around a converged equilibrium cannot decide the verdict.
const DENIAL_PROGRESS_WINDOW_DIVISOR: u32 = 2;

/// One quantile of the denial projection — the `Logistics` regrowth then the `Population` take, turn
/// by turn, exactly as [`hunt_trip_forecast_seeded`] does for a hunt, until the herd is **past
/// recovery** or the horizon runs out.
///
/// **The pack does not short-circuit it.** A hunt's projection bails on an empty pack because a
/// raid with nowhere to put the meat has no trip to project; a denial raid has no such dependence —
/// the pack decides only what comes home, so a party that can carry nothing still erases the herd
/// and simply wastes all of it.
#[allow(clippy::too_many_arguments)] // every config the forward simulation reads is a lever
fn denial_projection_at(
    workers: u32,
    herd: &Herd,
    fauna: &FaunaConfig,
    // The party's per-hunter haul rate — see `denial_forecast`.
    per_worker_haul: f32,
    expedition: &ExpeditionConfig,
    party: &fauna::HuntingParty,
    draw: fauna::HuntDraw,
) -> DenialProjection {
    let hunt_yield = fauna::herd_hunt_yield(herd, fauna);
    // The party's pack — a **carry** bound only, never a stop. There is no fill target to resolve:
    // a raid that does not clamp to carry has no pack-fill stop for one to replace.
    let cap = scalar_from_f32(workers as f32 * expedition.hunt.per_worker_carry);
    let horizon = expedition.hunt.forecast_horizon_turns;
    // The projection runs on a private copy — the caller's live herd is never touched.
    let mut quarry = herd.clone();
    let ecology = herd_ecology(&quarry, fauna);
    let capacity = herd_capacity(&quarry, fauna);
    let engage_rate = fauna.engage_rate_for(&quarry.species);
    let wariness = fauna.wariness_for(&quarry.species);
    // The one term that changes every projected turn (§4.2) — a raid spanning turns wears the quarry
    // down, and a projection that froze the wounds could not see a multi-turn kill at all.
    let mut quarry_fight = fauna::herd_quarry_fight(&quarry, fauna);
    let mut larder = scalar_zero();
    let mut animals_killed = 0u32;
    let mut delivered_food = 0.0_f32;
    let mut delivered_trade = 0.0_f32;
    let mut wasted_food = 0.0_f32;
    let mut wasted_trade = 0.0_f32;
    // **The headway window** (§3's *"its kills per turn below the herd's regrowth"*): the herd's
    // biomass halfway through the projection, so the verdict at the horizon is read over the whole
    // second half rather than off one turn. A single turn cannot answer it — at the equilibrium a
    // repelled raid settles into, one turn's kills and one turn's regrowth are equal by definition,
    // and which side of the comparison the float lands on is noise.
    let progress_window_opens = horizon / DENIAL_PROGRESS_WINDOW_DIVISOR;
    let mut biomass_at_window_open = quarry.biomass;

    for turn in 1..=horizon {
        if turn == progress_window_opens {
            biomass_at_window_open = quarry.biomass;
        }
        fauna::regrow_biomass(&mut quarry, fauna);
        if quarry.biomass <= ecology.extinction_floor * capacity {
            // `advance_herds` would despawn it here, and the live party's lost-herd guard turns it
            // for home on the same turn.
            return DenialProjection {
                turns: Some(turn),
                outcome: DenialOutcome::HerdLost,
                animals_killed,
                delivered_food,
                wasted_food,
                delivered_trade,
                wasted_trade,
            };
        }

        // The pack's remaining room, through the same [`carry_room_biomass`] the live arm and the
        // hunt's projection use. **A denial party's pack is a real carry bound** — only its
        // *engagement* is unbounded (`EngagementStop::Never` below) — so this is the ordinary
        // conversion, and an inedible quarry is unbounded here for the ordinary *product* reason.
        let carry_room = carry_room_biomass(cap - larder, &hunt_yield);
        let outcome = expedition_take_biomass(
            workers,
            per_worker_haul,
            // The escapement ceiling is the herd's whole standing stock.
            STRIP_IT_BARE,
            quarry.biomass,
            capacity,
            quarry.body_mass,
            carry_room,
            engage_rate,
            wariness,
            quarry_fight,
            party,
            draw,
            fauna::EngagementStop::Never,
            &mut quarry.hunt_credit,
        );
        let take = outcome.take;
        quarry_fight = quarry_fight.with_wounds(outcome.fight.wounds);
        quarry.biomass -= take.killed_biomass();
        animals_killed += take.killed;
        let landed = hunt_yield.apply(take.carried, EXPEDITION_OUTPUT_MULTIPLIER);
        delivered_food += landed.provisions;
        delivered_trade += landed.trade_goods;
        // **BOTH products of the wasted biomass, off ONE conversion** — the rule the delivered pair
        // above already follows. A food-only waste line reports `0` on an inedible quarry, which is
        // exactly the raid whose waste is total.
        let left_on_the_range = hunt_yield.apply(take.wasted, EXPEDITION_OUTPUT_MULTIPLIER);
        wasted_food += left_on_the_range.provisions;
        wasted_trade += left_on_the_range.trade_goods;
        let room = (cap - larder).max(scalar_zero());
        larder += scalar_from_f32(landed.provisions).min(room);

        if fauna::herd_past_recovery(quarry.biomass, capacity, &ecology) {
            return DenialProjection {
                turns: Some(turn),
                outcome: DenialOutcome::PastRecovery,
                animals_killed,
                delivered_food,
                wasted_food,
                delivered_trade,
                wasted_trade,
            };
        }
    }

    DenialProjection {
        turns: None,
        // §3's verdict, stated as the design states it: a party whose kills do not outpace the
        // herd's regrowth is not slow, it is **repelled** — the herd sits at an equilibrium above the
        // line and waiting longer changes nothing. Measured as *net progress against the herd over
        // the projection's second half*, in the herd's own quantum: a raid that could not take even
        // **one more animal's worth** off the standing stock in half a horizon is not winning
        // slowly, it is not winning.
        outcome: if biomass_at_window_open - quarry.biomass < quarry.body_mass {
            DenialOutcome::Repelled
        } else {
            DenialOutcome::Horizon
        },
        animals_killed,
        delivered_food,
        wasted_food,
        delivered_trade,
        wasted_trade,
    }
}

/// The in-flight delivery forecast for a live **hunting** party — the client's
/// "Next delivery: ~X food in ~N turns" drawer line, the in-flight twin of the pre-launch
/// `hunt_trip_forecast`/`huntTripEstimates`. Scouts deliver map data, not food, so they → `None`.
///
/// **A DENIAL party is `None` too, and that is a statement rather than a gap** — its readout is the
/// collapse verdict, not a delivery ETA (`docs/plan_denial_raid.md` §3). Quoting *"next delivery"*
/// for a raid whose whole point is that nothing comes home would be the food-only blindness the
/// mission exists to reverse; the in-flight collapse line is the client slice's.
///
/// `eta_turns` decomposes as remaining-travel-to-herd + hunting-turns-to-complete + walk-home; it is
/// an APPROXIMATION (the home band is nomadic and may move, and the walk-home is measured from the
/// herd) — honest for a "~N turns" readout, deliberately not turn-perfect. `None` when the raid can't
/// complete within the forecast horizon (a trickle-fill): the client shows the amount without an ETA.
pub struct ExpeditionDelivery {
    pub eta_turns: Option<u32>,
    pub projected_food: f32,
    pub recurring: bool,
    /// **Which stop will end THIS party's raid** — [`HuntTripBound`], read off the same seeded
    /// forward simulation the ETA comes from, so it answers for the party's *actual* orders (its own
    /// floor, against the herd's live stock) rather than for the band-agnostic pre-launch table.
    ///
    /// `None` for a party that is no longer raiding — already `Delivering`/`Returning`, or its herd
    /// is gone — where there is no forward projection to name a stop in. That is a different
    /// statement from [`HuntTripBound::Horizon`], which means the projection ran and found no stop.
    pub trip_bound: Option<HuntTripBound>,
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
    // The in-flight party's own strength — kit resolved by the caller, so the ETA projects the take
    // this party can actually make (`docs/plan_hunt_through_combat.md` §4).
    party: &fauna::HuntingParty,
    // And its per-hunter haul rate, on the same rule: the party's own kit decides what it drags home.
    per_worker_haul: f32,
    grid_width: u32,
    wrap_horizontal: bool,
) -> Option<ExpeditionDelivery> {
    let ExpeditionMission::Hunt { fauna_id, floor } = &expedition.mission else {
        // Scouts deliver map data, not food; a denial raid delivers a rounding error and is read by
        // its collapse verdict instead.
        return None;
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
            // Nothing is being raided — this leg is a walk, so there is no stop to name.
            trip_bound: None,
        }),
        // Still working toward the kill.
        ExpeditionPhase::Hunting | ExpeditionPhase::Outbound | ExpeditionPhase::AwaitingOrders => {
            let Some(herd) = herds.find(fauna_id) else {
                // Herd lost → the party will fold home carrying what it has.
                return Some(ExpeditionDelivery {
                    eta_turns: home_pos.map(|h| travel(party_pos, h)),
                    projected_food: carried,
                    recurring: false,
                    // The raid is already over — the herd it named is gone.
                    trip_bound: Some(HuntTripBound::HerdLost),
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
                per_worker_haul,
                expedition_cfg,
                party,
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
                trip_bound: Some(fc.bound),
            })
        }
    }
}

#[cfg(test)]
mod denial_outcome_tests {
    //! The wire round trip of [`DenialOutcome`] — the enum a denial row publishes as a `String` and
    //! that a consumer has to get *back* to in order to ask [`DenialOutcome::succeeded`].

    use super::DenialOutcome;

    /// **Every variant survives the round trip, and the keys are distinct.**
    ///
    /// The failure this guards is one-directional drift: a fifth verdict added to
    /// [`DenialOutcome::as_str`] (which the compiler forces) but left out of
    /// [`DenialOutcome::ALL`] (which it does not) would publish a key nothing can parse, and every
    /// consumer asking `succeeded` about that row would quietly read *"it did not"*. The sweep runs
    /// over `ALL`, so it also states that `ALL` is what `from_wire` searches.
    #[test]
    fn every_denial_outcome_round_trips_through_its_wire_key() {
        for outcome in DenialOutcome::ALL {
            assert_eq!(
                DenialOutcome::from_wire(outcome.as_str()),
                Some(outcome),
                "{outcome:?} publishes `{}`, which must parse back to it",
                outcome.as_str()
            );
        }

        let mut keys: Vec<&'static str> = DenialOutcome::ALL.iter().map(|o| o.as_str()).collect();
        keys.sort_unstable();
        let distinct = keys.len();
        keys.dedup();
        assert_eq!(
            keys.len(),
            distinct,
            "two verdicts sharing a wire key make the round trip lossy: {keys:?}"
        );

        assert_eq!(
            DenialOutcome::from_wire("collapsed"),
            None,
            "a key no variant publishes is `None`, never a plausible-looking default"
        );
    }

    /// **Success is `past_recovery` or `herd_lost`, and `horizon` is NOT success** — the distinction
    /// the launch sheet's seed turns on (`snapshot::subsistence::seeded_denial_party`). `Horizon`
    /// says the projection ran its whole length with the herd still standing, which is the *absence*
    /// of a verdict about the party, not a win.
    #[test]
    fn only_a_finished_raid_counts_as_success() {
        assert!(DenialOutcome::PastRecovery.succeeded());
        assert!(DenialOutcome::HerdLost.succeeded());
        assert!(!DenialOutcome::Repelled.succeeded());
        assert!(
            !DenialOutcome::Horizon.succeeded(),
            "a raid still grinding when the forecast ran out has not driven the herd down"
        );
    }
}
