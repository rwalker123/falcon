use super::*;
use crate::components::{BandName, RaidOrders};
use crate::connections::Sighting;
use crate::fauna::AnimalTake;

/// **The reason token for a SHIPMENT whose destination vanished under it** — the band it was bound
/// for despawned while the party walked, so there is nobody left to hand the cargo to. The party
/// turns for home still carrying it; the trade verb's twin of the raid's lost-herd exit, which
/// reports [`DenialOutcome::HerdLost`].
const DESTINATION_GONE_MID_TRADE: &str = "destination_gone";

/// **Everything one detached party is, as a query tuple.** Named because the tuple grew past the
/// point of readability when the party's own kit joined it: **a detached party carries its OWN kit**
/// (`docs/plan_denial_raid.md` §1.2). It leaves outfitted — a party with no ledger of its own falls
/// back to [`BandEquipment::start_stocked`], **not** `Default`, which owns nothing — and, since the
/// take resolves through the fight (`docs/plan_hunt_through_combat.md` §4), it must
/// also *wear* that kit — a raid on free, immortal equipment is denial for nothing, and its `attack`
/// tier is what the fight's gate compares against.
type ExpeditionParty = (
    Entity,
    &'static mut PopulationCohort,
    Option<&'static BandTravel>,
    &'static mut Expedition,
    Option<&'static mut BandEquipment>,
    // **The party's own durable id** — the key its crossings name it by (`TransferCrossing::party`),
    // so a client groups one shipment's goods under the party that carried them.
    Option<&'static BandId>,
);

/// **A party [`advance_party_defection`] may carry over to another people** — its people, its
/// mission, its id, and the gear ledger it takes with it.
type DefectingParty = (
    Entity,
    &'static mut PopulationCohort,
    &'static mut Expedition,
    Option<&'static BandId>,
    Option<&'static mut BandEquipment>,
);

/// **The RESIDENT bands [`advance_expeditions`] reaches**, named for the reason [`ExpeditionParty`]
/// is. Three jobs at once: the home band the party reports to and delivers into (`&mut`), the
/// [`BandId`] a contact report is filed under (an `Entity` is not an identity), and the
/// [`ResidentBand`] marker that makes a band a legal contact *subject* — seeing another people's
/// scouts is a different beat and is out of scope for the connection primitive.
type ExpeditionHomeBands = (
    Entity,
    &'static mut PopulationCohort,
    Option<&'static BandId>,
    Option<&'static ResidentBand>,
    // The band's food ledger. A party handing its pack (or a shipment) over crosses two larders
    // through neither income nor consumption, so the crossing is recorded on
    // `LaborAllocation::last_food_transfers`. `Option`, matching how the sibling ledger terms
    // are read at capture.
    Option<&'static mut LaborAllocation>,
    // **The name a contact report records the band under** — read at the moment a party sees it,
    // so the report carries what the band was called then (clock 1 of the connection it founds).
    Option<&'static BandName>,
    // **The band's gear ledger** — a party coming home, or going over to this band, places the
    // kit it carries back here ([`fold_party_into_band`]).
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
    /// The materials table — a raid's take is a yield edge like any other, so the party banks hide,
    /// bone and fibre off what it carries and hands them to the band on arrival.
    pub materials: Res<'w, crate::materials_config::MaterialsConfigHandle>,
    /// **The flora roster** — a ranging party gathers off the stands it passes, and what a stand
    /// converts to food is its tile's realized basket. In the bundle rather than at top level for
    /// the reason the bundle exists: the system is at Bevy's 16-parameter ceiling.
    pub flora: Res<'w, FloraConfigHandle>,
    /// **How food keeps** (#706) — a party eats its pack fastest-rotting first, and what it gathers
    /// or kills on the road lands in a keeping class. In the bundle for the 16-parameter reason.
    pub demographics: Option<Res<'w, DemographicsConfigHandle>>,
}

/// The configs and logs [`advance_band_movement`] reads, bundled (the [`ExpeditionConfigs`] idiom).
#[derive(bevy::ecs::system::SystemParam)]
pub struct BandMovementParams<'w> {
    pub labor: Res<'w, LaborConfigHandle>,
    pub ladder: Res<'w, LadderConfigHandle>,
    pub sim: Res<'w, SimulationConfig>,
    pub tile_registry: Res<'w, TileRegistry>,
    /// The ferry reach a long move is measured against (`carry::move_ferry_reach_tiles`).
    pub supply: Res<'w, crate::supply_network_config::SupplyNetworkConfigHandle>,
    /// The carry a long move sheds down to (`expedition_config.json` → `carry`).
    pub expedition: Res<'w, crate::expedition_config::ExpeditionConfigHandle>,
    pub tick: Res<'w, SimulationTick>,
    pub event_log: ResMut<'w, CommandEventLog>,
    pub route_traffic: ResMut<'w, crate::routes::RouteTrafficLog>,
}

/// One band on the move, with what a departure's long-move shed touches.
type MovingBand = (
    Entity,
    &'static mut PopulationCohort,
    &'static mut BandTravel,
    Option<&'static mut BandEquipment>,
    Option<&'static mut LaborAllocation>,
    Option<&'static BandId>,
    Has<ResidentBand>,
    Has<Expedition>,
);

/// Advance any `move_band` order one step toward its target. The band travels at
/// `band_move_tiles_per_turn` tiles/turn; `current_tile` (and `home`, since a nomad band has no
/// fixed origin) follow it so labor reads the updated in-range source set, and on arrival the
/// `BandTravel` component is removed. Movement is the only way a band repositions — a far hunt is a
/// work party, never a whole-band chase.
///
/// # ⛔ THIS IS ALSO THE WHOLE OF THE ROUTE BRANCH'S "PEOPLE" TRAFFIC, AND ONE HOOK IS ENOUGH
///
/// **Every travelling thing in the game is a `PopulationCohort` carrying a `BandTravel`** — a band
/// moving camp, a scout, a raiding party, and a **trade shipment** (`handle_send_trade_expedition`
/// spawns an `Expedition` + `BandTravel` like every other party) — and this is the single system
/// that steps all of them. So `docs/plan_standing_upkeep.md` §4.13's two remaining traffic rows,
/// *a shipment walking a connection* and *ordinary band / expedition movement*, are filled by **one
/// hook**. That is a fact about the code rather than a shortcut, and building two paths to make the
/// table look symmetrical would be inventing a second producer of the same number.
///
/// The journey recorded is `current → next` — `next` is the position after the **whole** turn's
/// movement, so `routes::advance_roads` traces the tiles between them and banks
/// `work_per_worker_tile × workers` on each. The head count is
/// [`crate::components::available_workers`], the same seam the labour pass spends.
///
/// ## ⛔ THE ONE-TURN LAG IS THE ARRANGEMENT, NOT A DEFECT
///
/// This runs in `TurnStage::Population` and `routes::advance_roads` drains the log in
/// `TurnStage::Logistics`, so a **march** is banked in the **next** turn's Logistics while a
/// **pooling link** is banked in the same turn's. Each entry is banked exactly once — the log has
/// one drain — so nothing is lost and nothing doubles. **Do not reorder a stage for it**; it is the
/// same shape as every other lag in this arc.
///
/// ## ⛔ THE LONG-MOVE SHED HAPPENS HERE, AT DEPARTURE — never when the order is accepted
///
/// A band leaves when the turn advances, so what it cannot carry is left behind then, measured from
/// where it stands to that order's target. It runs in `TurnStage::Population`, after
/// `starting_loadout::close_opening_window` (registered before `TurnStage::Influence`), so a turn-one
/// outfitting window is already shut when the shed applies and no grant can re-mint what was left.
/// An order cancelled or replaced before the turn advances sheds nothing.
pub fn advance_band_movement(
    mut commands: Commands,
    mut params: BandMovementParams,
    tiles: Query<&Tile>,
    mut cohorts: Query<MovingBand>,
) {
    let labor = params.labor.get();
    let ladder = params.ladder.get();
    let carry = params.expedition.get().carry.clone();
    let reach = crate::carry::move_ferry_reach_tiles(&params.supply.get());
    let width = params.tile_registry.width;
    let wrap_horizontal = params.sim.map_topology.wrap_horizontal;
    let tick = params.tick.0;
    for (entity, mut cohort, mut travel, equipment, allocation, band_id, resident, expedition) in
        cohorts.iter_mut()
    {
        let current = tiles
            .get(cohort.current_tile)
            .map(|tile| tile.position)
            .unwrap_or(travel.target);
        if current == travel.target {
            commands.entity(entity).remove::<BandTravel>();
            continue;
        }
        // **THE DEPARTURE** — this order's first step. Only a RESIDENT band sheds: a detached party
        // keeps its own rules (its pack is its pack).
        if !travel.departed {
            travel.departed = true;
            let long = crate::grid_utils::hex_distance_wrapped(
                current,
                travel.target,
                width,
                wrap_horizontal,
            ) > reach;
            if long && resident && !expedition {
                if let Some(entry) =
                    shed_at_departure(&mut cohort, equipment, allocation, band_id, &carry, tick)
                {
                    params.event_log.push(entry);
                }
            }
        }
        let next = step_toward(
            current,
            travel.target,
            labor.band_move_tiles_per_turn,
            width,
            wrap_horizontal,
        );
        if let Some(tile_entity) = params.tile_registry.index(next.x, next.y) {
            cohort.current_tile = tile_entity;
            cohort.home = tile_entity;
        }
        // **The boots that actually crossed the ground**, recorded only where the party moved —
        // `marched` carries the same `from != to` guard, and a party held at its own tile wears
        // nothing.
        params.route_traffic.marched(
            current,
            next,
            crate::components::available_workers(cohort.working),
            &ladder,
        );
        if next == travel.target {
            commands.entity(entity).remove::<BandTravel>();
        }
    }
}

/// One resident band as [`follow_hunted_herds`] reads it: where it stands, what it hunts, and the
/// movement order it already carries.
type FollowingBand = (
    Entity,
    &'static PopulationCohort,
    &'static LaborAllocation,
    Option<&'static mut BandTravel>,
);

/// **MIGRATION MODE — a band whose hunt row says `move_with_herd` walks toward that herd**
/// (`docs/plan_roaming_bands.md`, `.claude/rules/core_sim/work-party.md`).
///
/// Runs in `TurnStage::Population` **immediately before** [`advance_band_movement`]: the herds have
/// already moved this turn (`advance_herds`, Logistics), so the band re-aims at where its herd now
/// stands and steps right after it. A band camped in the herd therefore stays on the herd's tile
/// through loiter and migration, and every kill is a camp kill.
///
/// It issues **no new movement**: it only writes the same [`BandTravel`] a `move_band` order does.
///
/// # ⛔ RE-AIMING KEEPS `departed`
///
/// A band already carrying a `BandTravel` has its `target` updated **in place**. A fresh
/// [`BandTravel::to`] is `departed: false`, and `advance_band_movement` re-runs the long-move carry
/// shed on a not-yet-departed order that starts beyond the ferry reach — so replacing the order each
/// turn would shed the pack again every step of a far catch-up. Only a band with no order gets a
/// fresh one (a far start sheds once, as any long move does).
pub fn follow_hunted_herds(
    mut commands: Commands,
    herds: Res<HerdRegistry>,
    tiles: Query<&Tile>,
    mut bands: Query<FollowingBand, (With<ResidentBand>, Without<Expedition>)>,
) {
    for (entity, cohort, allocation, travel) in bands.iter_mut() {
        let Some(herd) = allocation.followed_herd().and_then(|id| herds.find(id)) else {
            continue;
        };
        let target = herd.current_pos;
        let Ok(here) = tiles.get(cohort.current_tile).map(|tile| tile.position) else {
            continue;
        };
        if here == target {
            // In the herd already: nothing to walk.
            if travel.is_some() {
                commands.entity(entity).remove::<BandTravel>();
            }
            continue;
        }
        match travel {
            Some(mut travel) => travel.target = target,
            None => {
                commands.entity(entity).insert(BandTravel::to(target));
            }
        }
    }
}

/// **Shed a resident band down to what its workers can carry, as it departs on a long move**
/// (#732, [`crate::carry`]).
///
/// Past the ferry reach the band walks off with [`crate::carry::band_carry_capacity`] — priced on
/// its actual working-age value, never a floored head count: food loads first, then materials are
/// cut before tools in what the food leaves, the **most worn** units are the ones dropped, and what
/// is left behind is **lost**. The dropped food is booked on the food ledger's `left_behind` term so
/// the identity still closes. Returns the feed line naming roughly what was left, or `None` when
/// nothing is shed.
fn shed_at_departure(
    cohort: &mut PopulationCohort,
    mut equipment: Option<Mut<BandEquipment>>,
    allocation: Option<Mut<LaborAllocation>>,
    band_id: Option<&BandId>,
    carry: &crate::carry::CarryConfig,
    tick: u64,
) -> Option<CommandEventEntry> {
    let plan = crate::carry::plan_long_move_shed(
        &cohort.stores,
        equipment.as_deref(),
        crate::carry::band_carry_workers(cohort),
        carry,
    );
    if plan.is_empty() {
        return None;
    }
    let food_left =
        crate::carry::shed_for_long_move(&mut cohort.stores, equipment.as_deref_mut(), &plan);
    // **Booked on the food ledger, or the identity is false on the turn a band walks away** — the
    // larder fell by food that passed through no income, meal, rot or transfer.
    if let Some(mut allocation) = allocation {
        allocation.last_food_left_behind += food_left.to_f32();
    }
    let food = food_left.to_f32();
    let items = plan.item_units();
    let materials = plan.material_units().to_f32();
    // `band=` is the durable `BandId`, never the entity — the token the client joins on.
    let (label, band_token) = band_id.map_or_else(
        || ("A band".to_string(), String::new()),
        |band| {
            (
                super::population::band_label(*band),
                format!(" band={}", band.0),
            )
        },
    );
    let entry = CommandEventEntry::new(
        tick,
        CommandEventKind::CancelOrder,
        cohort.faction,
        format!(
            "{label} left behind {food:.0} food, {items} gear, {materials:.0} material - too far \
             to carry"
        ),
        Some(format!(
            "status=left_behind action=move_band food={food:.2} items={items} \
             materials={materials:.2}{band_token}"
        )),
    );
    Some(match band_id {
        Some(band) => entry.with_band(*band),
        None => entry,
    })
}

/// **Which bands each detached party saw on its own sweep THIS turn** — derived, cleared and rebuilt
/// every turn, never checkpointed.
///
/// Written by [`advance_expeditions`] from inside the observe loop it already runs (so "what the
/// party can see" has one answer), and drained by [`advance_party_defection`] later in the same
/// Population chain. It exists beside `Expedition::pending_contacts` rather than reading it because
/// that buffer is a *report* — it holds whatever the party saw since it last came within comm range
/// and is emptied by the flush — so it can neither say "seen this turn" nor survive a flush on the
/// turn a party walks home past a camp.
///
/// Keyed by the party entity and holding [`BandId`]s in `BTreeSet`s, so a reader walks it in one
/// order.
#[derive(Resource, Default, Debug, Clone)]
pub struct PartySightings(BTreeMap<Entity, BTreeSet<BandId>>);

impl PartySightings {
    /// Record that `party` saw `subject` this turn.
    pub fn record(&mut self, party: Entity, subject: BandId) {
        self.0.entry(party).or_default().insert(subject);
    }

    /// The bands `party` saw this turn — empty when it saw none.
    pub fn seen_by(&self, party: Entity) -> impl Iterator<Item = BandId> + '_ {
        self.0.get(&party).into_iter().flatten().copied()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The config handles [`advance_party_defection`] reads, bundled (the [`ExpeditionConfigs`] idiom).
#[derive(bevy::ecs::system::SystemParam)]
pub struct PartyDefectionConfigs<'w> {
    /// The pull test's dials — one rule, one set of dials (`migration.*`).
    pub wellbeing: Res<'w, WellbeingConfigHandle>,
    /// `defection.party_pull_threshold`.
    pub expedition: Res<'w, crate::expedition_config::ExpeditionConfigHandle>,
    pub sim: Res<'w, SimulationConfig>,
}

/// A detached party that has decided to go, and the band it goes to.
struct PartyDefection {
    party: Entity,
    destination: Entity,
    destination_band: BandId,
    destination_people: FactionId,
}

/// **A detached party goes whole — `docs/plan_band_fission.md` §"Scouts: a party goes whole".**
///
/// A party is a few of its home band's people far from home, so it carries **the home band's
/// morale**. Each turn the home band is below `migration.morale_threshold` **and** the party saw, on
/// its own sweep this turn ([`PartySightings`]), a band of another people within
/// `migration.base_reach` of its own tile that passes the trickle's pull test against the home
/// band's morale and belongs to a people with Open Borders ([`FactionBorderPolicies`]), it accrues
/// `migration_move_fraction(home morale)` of [`Expedition::defection_pull`]; any other turn resets
/// it to zero. At `expedition_config.json` → `defection.party_pull_threshold` the **whole party**
/// joins the best such band (highest morale; ties to the lowest `BandId`, the order it is walked in),
/// through the same [`fold_party_into_band`] a homecoming uses — people, pack and any cargo — and the
/// party despawns exactly as a fold-back despawns it.
///
/// **Its kit does not transfer**, exactly as it does not on a homecoming: `fold_party_into_band`
/// moves people and stores, and the party's own [`BandEquipment`] wear ledger goes with the entity.
///
/// **The losing people is told one generic line and nothing else** — no reason, no place, no
/// destination. The receiving people is told a party joined one of its bands.
///
/// Parties are walked in entity order and every decision is taken before any is applied, so the
/// result does not depend on query order.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_party_defection(
    mut commands: Commands,
    configs: PartyDefectionConfigs,
    tile_registry: Res<TileRegistry>,
    tick: Res<SimulationTick>,
    borders: Res<FactionBorderPolicies>,
    mut sightings: ResMut<PartySightings>,
    mut event_log: ResMut<CommandEventLog>,
    tiles: Query<&Tile>,
    mut parties: Query<DefectingParty>,
    // The homecoming's own band query: a party joins a resident band exactly as it would fold home.
    mut bands: Query<ExpeditionHomeBands, Without<Expedition>>,
) {
    // Drained whatever happens below: it is this turn's sightings and nothing else.
    let seen = std::mem::take(&mut *sightings);
    if parties.is_empty() {
        return;
    }
    let wellbeing = configs.wellbeing.get();
    let mig_cfg = &wellbeing.migration;
    let pull_threshold = scalar_from_f32(configs.expedition.get().defection.party_pull_threshold);
    let attractive_morale = scalar_from_f32(mig_cfg.attractive_morale);
    let min_gap = scalar_from_f32(mig_cfg.min_morale_gap);
    // The trickle's reach in hex steps, from the party's own tile — and WITHOUT the road bonus the
    // trickle adds: a party in the field is off the road.
    let width = tile_registry.width;
    let wrap = configs.sim.map_topology.wrap_horizontal;

    // Every resident band a party could join, by its durable id: where it stands, whose it is, how
    // well off it is. A band with no id cannot have been sighted, so it is not a candidate.
    struct Candidate {
        entity: Entity,
        faction: FactionId,
        pos: UVec2,
        morale: Scalar,
    }
    let candidates: BTreeMap<BandId, Candidate> = bands
        .iter()
        .filter_map(|(entity, cohort, band_id, resident, _, _, _)| {
            resident?;
            let pos = tiles.get(cohort.current_tile).ok()?.position;
            Some((
                *band_id?,
                Candidate {
                    entity,
                    faction: cohort.faction,
                    pos,
                    morale: cohort.morale,
                },
            ))
        })
        .collect();

    let mut order: Vec<Entity> = parties.iter().map(|(entity, ..)| entity).collect();
    order.sort_by_key(|entity| entity.to_bits());
    let mut defections: Vec<PartyDefection> = Vec::new();
    for party in order {
        let Ok((_, cohort, mut expedition, _, _)) = parties.get_mut(party) else {
            continue;
        };
        // The home band's morale is the party's: they are that band's people. An orphaned party has
        // no home to be unhappy about, and a home at or above the push threshold pushes nobody.
        let home_morale = bands
            .get(expedition.home_band)
            .ok()
            .map(|(_, home, _, _, _, _, _)| home.morale);
        let rate = home_morale
            .map(|morale| migration_move_fraction(morale, mig_cfg))
            .unwrap_or(scalar_zero());
        let party_pos = tiles
            .get(cohort.current_tile)
            .ok()
            .map(|tile| tile.position);
        let best = match (home_morale, party_pos) {
            (Some(home_morale), Some(party_pos)) if rate > scalar_zero() => {
                let mut best: Option<(BandId, &Candidate)> = None;
                for band_id in seen.seen_by(party) {
                    let Some(candidate) = candidates.get(&band_id) else {
                        continue;
                    };
                    // Another people's band, better off by the trickle's own test, within reach of
                    // where the party stands, and whose people will have them.
                    if candidate.faction == cohort.faction
                        || candidate.morale < attractive_morale
                        || candidate.morale <= home_morale + min_gap
                        || crate::grid_utils::hex_distance_wrapped(
                            party_pos,
                            candidate.pos,
                            width,
                            wrap,
                        ) as f32
                            > mig_cfg.base_reach
                        || !borders.is_open(candidate.faction)
                    {
                        continue;
                    }
                    if best.is_none_or(|(_, current)| candidate.morale > current.morale) {
                        best = Some((band_id, candidate));
                    }
                }
                best
            }
            _ => None,
        };
        let Some((destination_band, destination)) = best else {
            expedition.defection_pull = scalar_zero();
            continue;
        };
        expedition.defection_pull += rate;
        if expedition.defection_pull >= pull_threshold {
            defections.push(PartyDefection {
                party,
                destination: destination.entity,
                destination_band,
                destination_people: destination.faction,
            });
        }
    }

    for defection in defections {
        let Ok((_, mut party_cohort, mut expedition, party_band, mut party_gear)) =
            parties.get_mut(defection.party)
        else {
            continue;
        };
        let lost_people = party_cohort.faction;
        let noun = expedition.mission.party_noun();
        // Where they came from, named on the receiver's crossings — the band that sent them out.
        let origin = bands
            .get(expedition.home_band)
            .ok()
            .and_then(|(_, _, band_id, _, _, _, _)| band_id.copied())
            .map(|band| TransferCounterparty {
                band,
                faction: lost_people,
            });
        let Ok((_, mut destination, _, _, allocation, _, destination_gear)) =
            bands.get_mut(defection.destination)
        else {
            continue;
        };
        let head_count = available_workers(party_cohort.working);
        // **A party that goes over takes its gear with it** — it lands in the band it joins.
        let fold = fold_party_into_band(
            &mut party_cohort,
            &mut expedition.cargo,
            &mut destination,
            PartyGear {
                party: party_gear.as_deref_mut(),
                home: destination_gear.map(|gear| gear.into_inner()),
            },
        );
        // Food crossing into the receiver's larder through neither income nor consumption — booked
        // so the ledger identity still closes, under a cause that does not claim they are its own.
        if let Some(mut allocation) = allocation {
            fold.book_defected(&mut allocation, party_band.copied(), origin);
        }
        event_log.push(CommandEventEntry::new(
            tick.0,
            CommandEventKind::PartyDefected,
            lost_people,
            format!("Your {noun} party has left your control."),
            Some(format!(
                "side=lost expedition={}",
                defection.party.to_bits()
            )),
        ));
        event_log.push(CommandEventEntry::new(
            tick.0,
            CommandEventKind::PartyDefected,
            defection.destination_people,
            format!(
                "A party of {head_count} from {} joined {}",
                crate::systems::population::faction_label(lost_people),
                crate::systems::population::band_label(defection.destination_band)
            ),
            Some(format!(
                "band={} count={} from={} side=gained",
                defection.destination_band.0, head_count, lost_people.0
            )),
        ));
        commands.entity(defection.party).despawn();
    }
}

/// Per-turn logic for detached expeditions (traveling parties). Runs right after
/// `advance_band_movement` (so it reads the party's fresh position) and before the Visibility
/// stage's `discover_sites`. For each expedition:
/// - **Observe + comm-flush is SHARED by every mission** — a ranging party maps the
///   terrain it crosses regardless of verb. Each turn it observes the tiles in LOS of its current
///   tile — at the radius its own kit resolves through
///   [`crate::equipment_config::EquipmentStat::ExpeditionSightRange`], `observe_sight_range` being
///   the *equipped* tier — into a **private** pending-reveal buffer (it does NOT touch the faction
///   map — it is `Without<Expedition>` in `calculate_visibility`); and when within the effective comm
///   range of the home band's live tile, promotes every buffered tile to `Discovered` on the faction
///   map (never downgrading a live `Active` tile) and clears the buffer. For a raiding party this
///   fires on its Returning fold-back. Site discovery rides the flushed tiles for
///   free via the Visibility stage's `discover_sites`. The gear is charged
///   [`crate::equipment_config::WearQuantum::TileRevealed`] **at observe time**, for tiles neither
///   already buffered nor already on the faction map — never at the flush, which would be a turn
///   clock.
/// - **Provisions** drain by `party × provision_upkeep_per_worker` (scouts and shipments only — a
///   raid lives off its kills); non-fatal at zero in v1.
/// - **Both halves of a kill's [`HuntYield`] come home** (#337) — the provisions into the party's
///   larder, the **material batches** into that same `LocalStore` and thence into the **home band's**
///   store at the fold-back, batch by batch through
///   [`crate::LocalStore::drain_materials_into`]. The retired `Expedition::carried_trade` /
///   `settle_carried_trade` pair banked the same haul as a scalar (arc #527).
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
    // **Where a party's findings about PEOPLE land**, comm-gated exactly like the map reveals
    // beside them. Consumed later the same turn by `connections::advance_connections`.
    mut contacts: ResMut<crate::connections::ContactsThisTurn>,
    // **What each party saw THIS turn**, for `advance_party_defection` — rebuilt here every turn,
    // so it is cleared before either early return below.
    mut sightings: ResMut<PartySightings>,
    mut event_log: ResMut<CommandEventLog>,
    mut herds: ResMut<HerdRegistry>,
    // **The stands a ranging party gathers off** — the plant half of how a provisioned party feeds
    // itself, drawn down through the same `forage_take` primitive a resident band's gatherers use.
    mut forage_registry: ResMut<ForageRegistry>,
    tiles: Query<&Tile>,
    // **The gather's SEASON** — a stand's per-worker throughput is scaled by its tile's food module
    // (`NO_FORAGE_SEASON` where there is none), the same reading the Forage arm of
    // `advance_labor_allocation` takes. Its own query rather than a column on `tiles`, because
    // `build_terrain_tags_grid` above takes that query whole.
    food_modules: Query<&FoodModuleTag>,
    mut expeditions: Query<ExpeditionParty>,
    mut bands: Query<ExpeditionHomeBands, Without<Expedition>>,
) {
    sightings.clear();
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
    let vis_cfg = configs.visibility.0.as_ref();
    // **Predators Phase 0 — the expedition-hunt danger seam** (`docs/plan_predators.md`). A hunting
    // party takes casualties like a resident band, but **bloodier**: far from home and unsupported, so
    // the same beast costs it more. The resolver tuning is scaled by `expedition_danger_multiplier`;
    // the base human's intrinsic profile is the same `person` the resident band fields. Resolved once.
    let combat_config = configs.combat.get();
    // Through the named constructor, not an open-coded multiply: the pre-launch forecasts resolve the
    // *same* party and one of them had already forgotten the scaling. See
    // `CombatConfig::expedition_tuning`.
    let combat_tuning = combat_config.expedition_tuning();
    let materials_cfg = configs.materials.get();
    let flora = configs.flora.get();
    let demographics_cfg = demographics_or_builtin(configs.demographics.as_deref());
    let keeping = &demographics_cfg.keeping;
    let person_profile = configs.creatures.get().person();
    // **The minimal TOE** — the two-tier table and the durability dials, resolved once. What varies
    // per party is only its `BandEquipment` *wear*.
    let equipment_cfg = configs.equipment.get();
    // The **equipped** per-hunter haul rate; the SLED kit names the step down.
    let equipped_haul_rate = labor.hunt.per_worker_biomass_capacity;
    // **And the bare-handed per-gatherer GATHER rate**, the baseline the BASKETS step a party up
    // from — the plant twin of the line above, resolved through the same seam
    // `advance_labor_allocation` reads it through. A provisioned party *does* have a gather mission:
    // it replenishes off the stands it passes before it will pick a fight ([`KitJob::Expedition`],
    // `equipment.json`'s `ranging` kit). (This used to read *"a raid is a hunt, so baskets never
    // enter this path — an expedition has no gather mission"*, which the ranging kit made false: the
    // two feeding paths are one kit's two items.)
    let baseline_gather_rate = labor.forage.per_worker_biomass_capacity;
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

    // **Who a party can FIND** — every resident band and where it stands, built once per system run
    // and probed per observed tile below. `HashMap` because it is only ever probed: the order that
    // has to be deterministic is the per-party `pending_contacts` buffer's, which is keyed.
    let mut occupancy: HashMap<UVec2, Vec<BandId>> = HashMap::new();
    // **Where each resident band stands, by the durable id a shipment addresses it with** — built in
    // the same pass, and probed rather than iterated (hence `HashMap`, the occupancy rule beside it).
    // A trade party retargets its destination's LIVE tile every turn, because bands are nomadic and a
    // shipment aimed at where a people used to camp arrives nowhere.
    let mut resident_positions: HashMap<BandId, (Entity, UVec2)> = HashMap::new();
    // **What each resident band is called**, for the contact a party records on sighting it. Owned
    // rather than borrowed, because `bands` is re-borrowed mutably below; a band with no
    // `BandName` is recorded under the empty name, which the wire reads as "unknown".
    let mut resident_names: HashMap<BandId, String> = HashMap::new();
    // **How many people each tile holds** (all resident bands, any faction), for the size sight
    // bonus — summed in the same pass.
    let mut tile_people = crate::visibility_systems::TilePeople::new();
    for (band_entity, cohort, band_id, resident, _, name, _) in bands.iter() {
        let (Some(id), Some(_)) = (band_id, resident) else {
            continue;
        };
        if let Ok(tile) = tiles.get(cohort.current_tile) {
            crate::visibility_systems::add_people(&mut tile_people, tile.position, cohort.size);
            occupancy.entry(tile.position).or_default().push(*id);
            resident_positions.insert(*id, (band_entity, tile.position));
            resident_names.insert(*id, name.map(|name| name.0.clone()).unwrap_or_default());
        }
    }
    // The same terrain term and size bonus the live sweep uses — sight is sight.
    let sight_modifiers = crate::visibility_systems::SightModifiers::build(
        &tiles,
        elevation.width,
        elevation.height,
        &vis_cfg.terrain_detection,
        crate::visibility_systems::size_sight_bonus_map(&tile_people, &vis_cfg.size_sight_bonus),
    );

    for (entity, mut cohort, travel, mut expedition, mut party_equipment, party_band) in
        expeditions.iter_mut()
    {
        let party_band = party_band.copied();
        let Ok(exp_pos) = tiles.get(cohort.current_tile).map(|tile| tile.position) else {
            continue;
        };
        let faction = cohort.faction;
        let workers = available_workers(cohort.working);
        // **This party's two kit tiers, resolved ONCE per party per turn** — the same discipline
        // `advance_labor_allocation` applies to a resident band, through the same
        // `EquipmentConfig` seams.
        // An absent **component** means the party's ledger was never built (a hand-rolled fixture),
        // which reads as outfitted — the state every launch path actually inserts. An absent
        // **entry inside** a ledger is *not owned*; see `BandEquipment`.
        let party_wear = party_equipment
            .as_deref()
            .cloned()
            .unwrap_or_else(|| BandEquipment::start_stocked_for(&equipment_cfg, workers as f32));
        // **The kit this party was SENT OUT WITH** — stored on the `Expedition` at launch and read
        // from there, never re-resolved against the home band's current stock. A party sent out with
        // `none` stays bare-handed for its whole life; re-reading the band's spears each turn would
        // silently re-arm it.
        let party_kit = expedition.kit.clone();
        // **Every tier resolved once per party per turn**, through the kit mask and the party's own
        // wear — so a party using nothing runs unequipped and, because wear rides the same mask,
        // spends nothing either.
        // **How this party's gear divides its people** (`equipment.md` → "the partly-equipped
        // party") — resolved once beside the tiers, so the haul it drags and the fight it puts up
        // describe the same crews.
        let coverage = equipment_cfg.coverage(&party_kit, workers as f32, &party_wear);
        let per_worker_biomass = coverage.weighted_rate(|kit| {
            equipment_cfg.hunt_per_worker_biomass_capacity(equipped_haul_rate, kit, &party_wear)
        });
        // **The GATHER twin of the haul tier above**, resolved through the same coverage: a party of
        // ten with four baskets gathers with four baskets and by hand on the other six. It is what
        // makes the `ranging` kit's third item real — the rate a bare-handed party replenishes at is
        // the unequipped one, and the difference is the whole reason the kit exists.
        let per_worker_gather_biomass = coverage.weighted_rate(|kit| {
            equipment_cfg.forage_per_worker_biomass_capacity(baseline_gather_rate, kit, &party_wear)
        });
        // **How far this party sees, resolved ONCE per party per turn like every tier beside it** —
        // `expedition_config.observe_sight_range` is the *equipped* radius and the `wayfinding`
        // item's `expedition_sight_range` declares the bare one, exactly the way a resident band's
        // posted vantage splits `labor_config.scout.vantage_range` against the same item's
        // `scout_vantage_range` in `calculate_visibility`.
        //
        // **NOT resolved through `coverage`, and that is the difference from the two carries above.**
        // A carry is per worker, so a party of ten with four sleds hauls at two rates; sight is one
        // question asked of one marching party, so it takes the kit's own tier the way the vantage
        // does. Rounded here because the reveal geometry is a tile radius; the effects axis stays
        // continuous so a designer can tune it.
        let observe_sight_range = equipment_cfg
            .expedition_sight_range(cfg.observe_sight_range as f32, &party_kit, &party_wear)
            .max(0.0)
            .round() as u32;
        // The weapon decides what the party can hurt at all (§4.2's gate), so it is resolved here and
        // not left at the intrinsic bare-handed tier. `exposure` and `dispersion` ride beside it —
        // a raid carrying a stand-off kit takes no injuries and scares nothing off, exactly as a
        // resident band with the same kit does.
        // **A FACTORY, for the reason `advance_labor_allocation`'s is** — a mass-bounded weapon is
        // only a weapon against quarry it can hold, so the attack tier waits for the target.
        let party_resolution = fauna::PartyResolution {
            equipment: &equipment_cfg,
            coverage: &coverage,
            wear: &party_wear,
            intrinsic: person_profile,
            tuning: combat_tuning,
            hunt_injury_damage_per_animal: combat_config.hunt_injury_damage_per_animal,
        };
        let party_for = |body_mass: f32| {
            party_resolution.party_against(crate::equipment_config::Quarry::Mass(body_mass))
        };
        // Home band's LIVE tile (bands are nomadic): drives the comm check and the return target.
        // An orphaned expedition (home band gone) simply can't report/deliver.
        let home_pos = bands
            .get(expedition.home_band)
            .ok()
            .and_then(|(_, band, _, _, _, _, _)| tiles.get(band.current_tile).ok())
            .map(|tile| tile.position);
        // **Who a contact report is filed under.** A party's findings belong to the band that
        // outfitted it, never to the party — the party is a detached crew and owns nothing.
        let home_band_id = bands
            .get(expedition.home_band)
            .ok()
            .and_then(|(_, _, band_id, _, _, _, _)| band_id.copied());
        // "Near enough to run home" — the shared proximity for the fold-back, a shipment's
        // hand-over, and the comm-range flush.
        let near_home = home_pos
            .map(|home| {
                crate::grid_utils::hex_distance_wrapped(exp_pos, home, grid_width, wrap_horizontal)
                    <= comm_range
            })
            .unwrap_or(false);
        let mission = expedition.mission.clone();

        // A raiding party whose herd is lost/extinct flips to Returning (folds back via the shared
        // arm below), with a feed line — what it carries still comes home. For a denial raid a lost
        // herd is the mission succeeding outright rather than the target slipping away.
        if let Some(orders) = mission.raid_orders() {
            let fauna_id = orders.fauna_id;
            if herds.find(fauna_id).is_none()
                && !matches!(expedition.phase, ExpeditionPhase::Returning)
            {
                expedition.phase = ExpeditionPhase::Returning;
                // **The reason is `DenialOutcome::HerdLost`'s own key** — one of the two verdicts
                // `DenialOutcome::succeeded` returns true for, and the launch sheet quotes it as a
                // win. The completion below states the other (`past_recovery`), so the exit and the
                // pre-launch verdict spell the outcome the same way.
                // **The line names the species, never the `fauna_id`.** This exit is also the one
                // place the name cannot be looked up: the herd is gone from the registry by the time
                // we get here (that is the condition above), so the only name left is the one the
                // mission carried from launch — which is what `target_display` reads.
                let target = mission.target_display();
                event_log.push(CommandEventEntry::new(
                    current_turn,
                    CommandEventKind::Hunt,
                    faction,
                    format!("Denial raid wiped out the {} — returning home", target),
                    Some(format!(
                        "status=returning reason={} expedition={}",
                        DenialOutcome::HerdLost.as_str(),
                        entity.to_bits()
                    )),
                ));
            }
        }

        // **A shipment whose destination cannot be resolved turns for home CARRYING THE CARGO** —
        // the trade verb's twin of the lost-herd guard above, and the same shape: the party is not
        // stranded waiting for a rendezvous that can never happen, and what it holds is not
        // destroyed. The goods settle back into the band that sent them on fold-back
        // ([`fold_party_into_band`]).
        //
        // **The kind is `ExpeditionRecalled`, which means "this party has been turned for home"** —
        // the same state change the recall verb makes, arrived at for a different reason, exactly as
        // the lost-herd guard reuses `Hunt`. `TradeDelivered` would be a lie about a shipment that
        // has not been delivered.
        if let Some(destination) = mission.destination_band() {
            if !resident_positions.contains_key(&destination)
                && !matches!(expedition.phase, ExpeditionPhase::Returning)
            {
                expedition.phase = ExpeditionPhase::Returning;
                event_log.push(CommandEventEntry::new(
                    current_turn,
                    CommandEventKind::ExpeditionRecalled,
                    faction,
                    format!(
                        "Trade party lost {} — returning home with its cargo",
                        mission.destination_display()
                    ),
                    // **`destination=` rides along, exactly as on the launch and delivery lines.**
                    // The label above names the band through `destination_display()`, whose
                    // fallback is the sim's positional `band <id>` spelling; the client swaps that
                    // for its own roster label by keying on this token
                    // (`EventDockPanel::_swap_band_label`). Omitting it is what makes one row of a
                    // shipment's life print a raw id while its siblings print the band's name.
                    Some(format!(
                        "status=returning reason={} destination={} expedition={}",
                        DESTINATION_GONE_MID_TRADE,
                        destination.0,
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
        // **What this party will be charged for on `WearQuantum::TileRevealed`** — counted here, at
        // the moment of looking, and only for ground that is genuinely new. See the charge below.
        let mut newly_seen: u32 = 0;
        for pos in crate::visibility_systems::visible_tiles_in_range(
            exp_pos,
            observe_sight_range,
            &elevation,
            vis_cfg.line_of_sight.enabled,
            &terrain_tags,
            &sight_modifiers,
            blocking_tags,
            wrap_horizontal,
        ) {
            if seen.insert(pos) {
                expedition.pending_reveal.push(pos);
                // **FIRST-EVER-REVEALED, on the party's OWN buffer AND on the faction map.** The
                // buffer half alone would still charge a party walking home over ground its band
                // mapped turns ago — a turn clock in a per-use costume, which
                // `docs/plan_denial_raid.md` §1.2 forbids and which the `wayfinding` item's own
                // comment calls out. `is_discovered` is true for `Discovered` *and* `Active`, i.e.
                // "already on the faction map", and it is a **read**: the flush below still owns
                // every mutation of the ledger.
                if !ledger.is_discovered(faction, pos.x, pos.y) {
                    newly_seen += 1;
                }
            }
            // **Peoples found on the march go into the private buffer beside the tiles.** Not gated
            // on `seen`: a party that has already mapped a tile can still find somebody standing on
            // it, and the most recent observation is the one that comes home.
            if let Some(occupants) = occupancy.get(&pos) {
                for subject in occupants {
                    // The party's own home band is not a stranger. Without this a party camped
                    // beside home would report its own band as a people it had found.
                    if Some(*subject) != home_band_id {
                        let name = resident_names.get(subject).cloned().unwrap_or_default();
                        expedition
                            .pending_contacts
                            .insert(*subject, Sighting::new(pos, current_turn, name));
                        sightings.record(entity, *subject);
                    }
                }
            }
        }

        // a2. **The wayfinding gear is charged for the ground this party mapped**, per tile revealed
        // for the FIRST time — the same quantum a resident band's posted vantage pays on, named by
        // quantum rather than by item so a kit carrying none of it wears nothing.
        //
        // # ⛔ CHARGED AT OBSERVE TIME, NOT AT THE COMM FLUSH, AND NOT PER TILE BUFFERED
        //
        // The buffer is a report, not work: flushing it charges the turn the party walked home, and
        // charging every buffered tile bills a party for re-crossing ground it already mapped. Both
        // are turn clocks wearing a per-use costume (`docs/plan_denial_raid.md` §1.2).
        //
        // Charging while the party is *out doing the looking* also settles two things the flush
        // could not: an orphaned party that never reports still wore its gear, and a long trip does
        // not land its whole bill in one lump on the turn it walks back into camp.
        //
        // **Accrue AFTER the take**, the ordering every wear site uses — this turn's ground was seen
        // at the tier it was priced with above, so a step-down lands on the next turn.
        if newly_seen > 0 {
            if let Some(kit) = party_equipment.as_mut() {
                kit.wear_kit(
                    &equipment_cfg,
                    &party_kit,
                    crate::equipment_config::WearQuantum::TileRevealed,
                    newly_seen as f32,
                );
            }
        }

        // b. Comm check + flush: in range of home → report the buffer as Discovered, then clear.
        // For a raiding party this naturally fires on its Returning fold-back (it's near the band
        // then), so its findings report home with the haul; sites on the flushed tiles ride
        // `discover_sites` for free, same as the scout.
        if near_home {
            let map = ledger.ensure_faction(faction, elevation.width, elevation.height);
            for pos in expedition.pending_reveal.drain(..) {
                map.discover(pos.x, pos.y, current_turn);
            }
            // **ONE contact event per subject per flush**, however many turns the party watched
            // them: what came home is one report, and crediting thirty turns of retroactive contact
            // would let a stale report peg a tie at full strength. The report is credited to the
            // HOME BAND — an orphaned party has nobody to report to, so its findings are simply
            // lost, exactly as its carried food is.
            let reports = std::mem::take(&mut expedition.pending_contacts);
            if let Some(observer) = home_band_id {
                for (subject, sighting) in reports {
                    contacts.record(observer, subject, sighting);
                }
            }
        }

        // ---- Provisioned parties: upkeep + opportunistic replenish (a raid lives off its kills) ----
        // **A trade party is provisioned like a SCOUT**, and takes this arm whole rather than a
        // trade-shaped copy of it. It is a walking party carrying no quarry, so what feeds it is the
        // larder it left with and whatever game it meets — the same two facts about a scout. This is
        // also where the trip's cost lives: a farther destination draws a bigger launch larder and
        // burns more turns of upkeep, which is why the trade block has no friction lever of its own.
        //
        // **The upkeep drains `cohort.stores`, never `expedition.cargo`.** The two are separate
        // stores precisely so a hungry party cannot eat the shipment it is hauling.
        if matches!(
            mission,
            ExpeditionMission::Scout | ExpeditionMission::Trade { .. }
        ) {
            // c. Provisions depletion (a raid lives off its kills instead). Non-fatal.
            let upkeep = scalar_from_f32(workers as f32 * cfg.provision_upkeep_per_worker);
            if upkeep > scalar_zero() {
                // The party eats its pack fastest-rotting class first, as a band does (#706).
                cohort.stores.eat_food(upkeep, &keeping.eat_order());
            }

            // ---- Opportunistic replenish: GATHER FIRST, THEN HUNT ---------------------------
            //
            // When provisions fall below `party × upkeep × low_turns`, a ranging party tops itself
            // up off the ground it is standing on — first from a stand in reach, and only if that
            // was not enough, off the game it meets. Both draws go through the primitives a
            // resident band's crews use (`forage_take` / `hunt_take`), capped at the low-water
            // buffer so neither overfills.
            //
            // # ⛔ IT IS AN ORDER, NOT A SCORE, AND MUST NOT BECOME ONE
            //
            // **Gathering costs no lives, no animals and no weapon wear** — the party spends baskets
            // and walks on — while a kill costs casualties, spears and a herd. So the party
            // exhausts the safe option before it picks a fight, and that single rule is the whole
            // model: a ranking pass between the two would let a fat herd outbid a stand the party
            // could have stripped for nothing, which is exactly the trade nobody would make.
            let low_buffer = scalar_from_f32(
                workers as f32 * cfg.provision_upkeep_per_worker * cfg.replenish.low_turns as f32,
            );
            if cohort.stores.get(FOOD) < low_buffer {
                // **The nearest stand in reach with something takeable standing above the floor.**
                // Keyed on `(distance, y, x)` rather than "the first match" because
                // `ForageRegistry::patches` is a `HashMap` whose iteration order is not
                // deterministic — a party standing between two stands must not pick by hash order.
                let stand = forage_registry
                    .patches
                    .iter()
                    .filter_map(|(tile, patch)| {
                        let distance = crate::grid_utils::hex_distance_wrapped(
                            exp_pos,
                            *tile,
                            grid_width,
                            wrap_horizontal,
                        );
                        (distance <= cfg.replenish.reach_tiles
                            && crate::forage::patch_take_room(patch, DEFAULT_ESCAPEMENT_FLOOR)
                                > NOTHING_TO_GATHER)
                            .then_some((distance, tile.y, tile.x))
                    })
                    .min()
                    .map(|(_, y, x)| UVec2::new(x, y));
                let ground = stand.and_then(|tile| {
                    let entity = tile_registry.index(tile.x, tile.y)?;
                    let ground = tiles.get(entity).ok()?;
                    Some((tile, ground, food_modules.get(entity).ok()))
                });
                if let Some((tile, ground, module)) = ground {
                    // **What is actually growing here**, through the one `tile_flora_composition`
                    // seam (never `FloraConfig::composition` on a raw terrain), and the gather's
                    // season, which is the food module's — a tile carrying none offers no wild
                    // gather at all (`NO_FORAGE_SEASON`), exactly as on the resident Forage arm.
                    let composition =
                        tile_flora_composition(&flora, &labor.forage, ground, map_seed);
                    let seasonal =
                        module.map_or(NO_FORAGE_SEASON, |module| module.seasonal_weight.max(0.0));
                    let room = (low_buffer - cohort.stores.get(FOOD)).max(scalar_zero());
                    let patch = forage_registry
                        .patch_mut(tile)
                        .expect("the stand was just found in this registry");
                    // **The room converts to a collection bound, exactly as the roadside kill's
                    // does** ([`carry_room_biomass`]): the pack's remaining provisions inverted
                    // through this stand's own conversion rate, so a nearly-topped-up party draws
                    // less off the ground rather than gathering food it must drop.
                    //
                    // It rides the CREW'S THROUGHPUT because that is the only bound `forage_take`
                    // takes — the whole crew term is `workers × capacity × seasonal`, so capping
                    // the capacity caps the draw, and the take path stays the one the band uses.
                    let per_biomass = forage_provisions(
                        crate::fauna::ONE_UNIT_OF_BIOMASS,
                        patch_provisions_per_biomass_taking(
                            patch,
                            &composition,
                            &flora,
                            &labor.forage,
                            &TakeSelection::EVERYTHING,
                        ),
                        EXPEDITION_OUTPUT_MULTIPLIER,
                    );
                    let crew = workers as f32 * seasonal;
                    let capped_per_worker =
                        if per_biomass > NOTHING_TO_GATHER && crew > NOTHING_TO_GATHER {
                            per_worker_gather_biomass.min(room.to_f32() / per_biomass / crew)
                        } else {
                            NOTHING_TO_GATHER
                        };
                    // **What the crew took off the stand**, read either side of the take — the wear
                    // quantum below is biomass, not provisions.
                    let standing_before = patch.biomass;
                    let gathered = forage_take(
                        patch,
                        &composition,
                        workers as f32,
                        // **The restrained floor**, the same one the roadside kill takes at: a
                        // party replenishing on the march can never be the thing that ruins a
                        // stand.
                        DEFAULT_ESCAPEMENT_FLOOR,
                        // A ranging party carries home whatever the stand offers — it is eating,
                        // not choosing a crop.
                        &TakeSelection::EVERYTHING,
                        &labor.forage,
                        &flora,
                        EXPEDITION_OUTPUT_MULTIPLIER,
                        capped_per_worker,
                        seasonal,
                    );
                    let gathered_biomass = standing_before - patch.biomass;
                    if gathered > scalar_zero() {
                        // **By keeping class, off the stand's own basket** (#706) — the same
                        // decomposition the resident gather credits through.
                        cohort.stores.add_food_mix(&crate::forage::patch_food_mix(
                            patch,
                            &composition,
                            &flora,
                            &labor.forage,
                            &TakeSelection::EVERYTHING,
                            gathered,
                            keeping,
                        ));
                    }
                    // **The BASKETS are charged per USE, never per turn** — the biomass this crew
                    // actually took off the stand, the same quantum the resident Forage arm
                    // charges. Named by quantum rather than by item, so an item added to the
                    // ranging kit that wears per biomass gathered is charged here without editing
                    // this call, and a party whose kit carries no baskets gathered by hand and
                    // wears nothing.
                    if let Some(kit) = party_equipment.as_mut() {
                        kit.wear_kit(
                            &equipment_cfg,
                            &party_kit,
                            crate::equipment_config::WearQuantum::BiomassGathered,
                            gathered_biomass,
                        );
                    }
                }
            }
            // **Only a party the stand could not fill picks a fight.** Re-read rather than reusing
            // the test above: the gather may have topped the pack up, and a party that no longer
            // needs food does not kill for it.
            if cohort.stores.get(FOOD) < low_buffer {
                // First **edible** herd within replenish reach (not necessarily the closest —
                // `position` returns the first match).
                //
                // # ⛔ AN INEDIBLE HERD IS SKIPPED OUTRIGHT, NOT RANKED LAST (issue #373)
                //
                // This whole arm is triggered by the FOOD low-water mark and exists to feed the
                // party, so a quarry that pays no provisions cannot answer the question that was
                // asked. A starving party used to kill a wolf pack it could not eat — spending
                // casualties, spears and a herd to bank pelts — and then walk on still starving.
                // Materials are a byproduct of a food take here and never the reason for one; the
                // hunt verb remains the way to go after pelts deliberately.
                let in_range = herds.herds.iter().position(|herd| {
                    herd_hunt_yield(herd, &fauna).edible()
                        && crate::grid_utils::hex_distance_wrapped(
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
                    // The **species'** food rate, not the global one — and edible by
                    // construction (the search above), so the room really does invert into a
                    // finite biomass collection rather than [`NO_CARRY_BOUND`].
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
                        workers as f32,
                        // A scout's roadside kill is a **restrained** one: it stops at the food peak,
                        // the same floor a fresh assignment gets, so replenishing on the march can
                        // never be the thing that ruins a herd.
                        DEFAULT_ESCAPEMENT_FLOOR,
                        per_worker_biomass,
                        &party_for(scout_quarry_mass),
                        &fauna,
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
                        // wears per biomass hauled is charged for the haul — so an item added to a
                        // kit is charged here without editing this call, and an item the kit does
                        // not carry is never charged at all.
                        //
                        // **The WEAPON is charged per crew, for the blows it landed** — a run that
                        // could not clear the quarry's defence swung at nothing and pays nothing.
                        outcome.fight.charge_strike_wear(kit, &equipment_cfg);
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
                    // The food tops the pack up to `room`.
                    let landed = scout_yield.apply(take.carried, EXPEDITION_OUTPUT_MULTIPLIER);
                    let provisions = scalar_from_f32(landed.provisions);
                    let added = provisions.min(room);
                    if added > scalar_zero() {
                        // Into the quarry's own keeping class (#706).
                        let class = fauna
                            .keeping_for(&herds.herds[idx].species)
                            .unwrap_or(&keeping.kill_fallback_class);
                        cohort.stores.add_food(class, added);
                    }
                    // **The MATERIAL account of the same roadside kill** — a scout's kill is skinned
                    // as well as butchered. Off `take.carried`, like the food above it and like
                    // every resident seam: you cannot tan a hide you left on the range, so a scout
                    // that hauled nothing banks nothing. The hides are a **byproduct** of a food
                    // take and never the reason for one — the search above never reaches an
                    // inedible herd (#373).
                    crate::materials_config::credit_material_yield(
                        &mut cohort.stores,
                        &materials_cfg,
                        fauna.hunt_materials_for(&herds.herds[idx].species),
                        take.carried,
                        EXPEDITION_OUTPUT_MULTIPLIER,
                    );
                }
            }
        }

        // ---- Phase machine ----
        match expedition.phase {
            ExpeditionPhase::Outbound if mission.destination_band().is_some() => {
                // **A shipment walks its destination down and hands the cargo over.** It reuses the
                // scout's `Outbound` and the shared `Returning`; there is deliberately no trade
                // phase, because the party does exactly two things — carry the goods there, walk
                // home — and both already have a phase.
                //
                // The destination resolves here by construction: the guard above turned the party
                // for home if it did not.
                let destination = mission
                    .destination_band()
                    .expect("this arm is gated on a trade mission");
                if let Some(&(host_entity, host_pos)) = resident_positions.get(&destination) {
                    // **RETARGET EVERY TURN, at the destination's LIVE tile** — bands are nomadic,
                    // so a shipment aimed once at where a people were camped arrives nowhere. Same
                    // rule as the `Hunting` arm's herd retarget.
                    let arrived = crate::grid_utils::hex_distance_wrapped(
                        exp_pos,
                        host_pos,
                        grid_width,
                        wrap_horizontal,
                    ) <= comm_range;
                    if !arrived {
                        commands.entity(entity).insert(BandTravel::to(host_pos));
                    } else {
                        // **ARRIVAL IS NOT RE-GATED ON THE TIE.** If the connection decayed to
                        // nothing while the party walked, the shipment still lands: the party is
                        // standing in their camp, and presence beats the ledger. The tie gates the
                        // *launch* — deciding to send goods to a people you have no dealings with —
                        // and that decision was made turns ago.
                        let carried_food = expedition.cargo.get(FOOD);
                        let carried_fodder = expedition.cargo.get(FODDER);
                        let carried_materials = materials_carried(&expedition.cargo);
                        // **The sender, named on the host's row** — the band that outfitted the
                        // party, in the party's own faction (a party is its home band's people).
                        let sender =
                            home_band_id.map(|band| TransferCounterparty { band, faction });
                        let landed = bands.get_mut(host_entity).ok().map(
                            |(_, mut host, _, _, allocation, _, _)| {
                                // The shipment lands class by class (#706): the host receives the
                                // flesh, greens and grain that were loaded, not a classless total.
                                let moved_mix = expedition.cargo.take_food_mix(carried_food);
                                host.stores.add_food_mix(&moved_mix);
                                let moved = moved_mix.total();
                                // **The hay lands in the host's OWN hay account** — `FODDER` is a
                                // second key on the same store, so the hand-over is the food
                                // hand-over verbatim and the two never convert on the way in.
                                let moved_fodder = expedition.cargo.take(FODDER, carried_fodder);
                                if moved_fodder > scalar_zero() {
                                    host.stores.add(FODDER, moved_fodder);
                                }
                                // **Batch by batch into the HOST's store** — the shipment's ratings
                                // are what make it a shipment of goods rather than of a number, so
                                // two ratings of one material arrive as two batches.
                                let moved_materials =
                                    expedition.cargo.drain_materials_into(&mut host.stores);
                                // The receiving half of a shipment — [`TransferCause::ShipmentIn`],
                                // on the `TransferLink::Route` arm, because a party carried this
                                // here. The *sending* half was booked at launch, against the band
                                // the party was drawn off, as `ShipmentOut`.
                                //
                                // **The hay books on its own ledger, never the food one**
                                // (`book_crossing` routes each key to its account): the food
                                // identity closes over the FOOD larder, which a bale never enters.
                                // Materials book per batch, at the rating each one moved at.
                                if let Some(mut allocation) = allocation {
                                    for (commodity, amount) in
                                        [(FOOD, moved), (FODDER, moved_fodder)]
                                    {
                                        allocation.book_crossing(
                                            TransferCrossing::goods(
                                                commodity,
                                                TransferDirection::In,
                                                TransferCause::ShipmentIn,
                                                amount.to_f32(),
                                            )
                                            .with_counterparty(sender)
                                            .with_party(party_band),
                                        );
                                    }
                                    allocation.book_material_draws(
                                        &moved_materials,
                                        TransferDirection::In,
                                        TransferCause::ShipmentIn,
                                        sender,
                                        party_band,
                                    );
                                }
                                (moved, moved_fodder)
                            },
                        );
                        if let Some((moved, moved_fodder)) = landed {
                            event_log.push(CommandEventEntry::new(
                                current_turn,
                                CommandEventKind::TradeDelivered,
                                faction,
                                format!(
                                    "Trade party delivered {} to {}",
                                    describe_haul(
                                        moved.to_i64_whole(),
                                        moved_fodder.to_i64_whole(),
                                        carried_materials
                                    ),
                                    mission.destination_display()
                                ),
                                Some(format!(
                                    "status=delivered destination={} fodder={} materials={:.*} \
                                     expedition={}",
                                    destination.0,
                                    moved_fodder.to_i64_whole(),
                                    HAUL_MATERIAL_DECIMALS,
                                    carried_materials,
                                    entity.to_bits()
                                )),
                            ));
                            // One-way in this slice: the party walks home empty rather than
                            // carrying a priced return flow, which is a later slice's model.
                            expedition.phase = ExpeditionPhase::Returning;
                        }
                    }
                }
            }
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
                    // **The other half of the haul goes into the SAME store as the meat** — the
                    // party's material batches move into the home band's store, the last chance
                    // before the party despawns and its pack goes with it. No home band left to
                    // receive them means the haul is simply lost, exactly as the carried food is.
                    let mut banked_materials = 0.0;
                    if let Ok((_, mut home, _, _, allocation, _, home_gear)) =
                        bands.get_mut(expedition.home_band)
                    {
                        // **The undelivered shipment comes home too** — a party that turned back
                        // because its destination could not be resolved is still carrying real
                        // goods, and they settle into the band that sent them.
                        // **And the kit it carried goes back on the band's shelf**, worn as it is.
                        let fold = fold_party_into_band(
                            &mut cohort,
                            &mut expedition.cargo,
                            &mut home,
                            PartyGear {
                                party: party_equipment.as_deref_mut(),
                                home: home_gear.map(|gear| gear.into_inner()),
                            },
                        );
                        banked_materials = fold.materials;
                        // The pack and the cargo landing in the band's larder is food crossing from
                        // a party into a band, which is neither income nor consumption. A party
                        // carried it, so the arm is `TransferLink::Route` whatever its mission was.
                        // The cause splits by store: the pack is [`TransferCause::PartyHome`], the
                        // band's own people coming back; undelivered cargo is
                        // [`TransferCause::ShipmentReturned`], naming the destination it never
                        // reached.
                        if let Some(mut allocation) = allocation {
                            fold.book_home(
                                &mut allocation,
                                party_band,
                                expedition.mission.consignee(),
                            );
                        }
                    }
                    event_log.push(expedition_returned_event(
                        current_turn,
                        faction,
                        exp_pos,
                        banked_materials,
                        entity,
                    ));
                    commands.entity(entity).despawn();
                } else if let Some(home) = home_pos {
                    // Chase the band's live tile each turn (retargets any stale travel order).
                    commands.entity(entity).insert(BandTravel::to(home));
                }
            }
            ExpeditionPhase::Hunting => {
                // Chase the herd and, when in reach, work it — the raid's whole crew engages, and
                // what it carries is bounded by the pack. The trip-completion decision lives INSIDE
                // the in-reach guard: a party still walking to its herd must never conclude the trip.
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
                        // **The pack** — a carry bound, never a stop: the same load the denial
                        // forecast projects (`denial_projection_at`).
                        let cap = scalar_from_f32(workers as f32 * cfg.hunt.per_worker_carry);
                        let in_reach = crate::grid_utils::hex_distance_wrapped(
                            exp_pos,
                            herd_pos,
                            grid_width,
                            wrap_horizontal,
                        ) <= cfg.hunt.reach_tiles;
                        if !in_reach {
                            // Still walking — chase the herd's live tile.
                            commands.entity(entity).insert(BandTravel::to(herd_pos));
                            continue;
                        }

                        // The raid's take (`expedition_take_biomass`) — the party works the herd's
                        // whole standing stock as fast as its throughput allows. The launch forecast
                        // (`denial_forecast`) SIMULATES this same helper, so the preview can't quote
                        // a different raid than this take. An inedible quarry carries no food, and
                        // that is a fact about the species.
                        let herd_biomass_before = herds.herds[idx].biomass;
                        let quarry_yield = herd_hunt_yield(&herds.herds[idx], &fauna);
                        // The keeping class the raid's meat lands in (#706), read while the herd
                        // is still borrowed only immutably.
                        let quarry_class = fauna
                            .keeping_for(&herds.herds[idx].species)
                            .unwrap_or(&keeping.kill_fallback_class)
                            .to_string();
                        // A party carrying food home can only take the biomass it has room for. The
                        // room bounds the party's **collection** (invert the species' own
                        // `provisions_per_biomass`), so a nearly-full pack kills fewer animals rather
                        // than slaughtering one it cannot haul.
                        //
                        // **The raid passes its real pack.** How deep a raid draws the herd and how
                        // much it can haul are separate questions: denial drops the pack as a bound on
                        // what it **engages** (`stop`) and keeps it as a bound on what it **hauls**.
                        // Only an **inedible** quarry is unbounded here, and that is a fact about the
                        // *product* — see [`carry_room_biomass`].
                        let carry_room =
                            carry_room_biomass(cap - cohort.stores.get(FOOD), &quarry_yield);
                        // The quarry's engagement/retreat/fight dials, and the per-event seed —
                        // composed BEFORE the mutable borrow, exactly as the scout replenish does.
                        let engage_rate = fauna.engage_rate_for(&herds.herds[idx].species);
                        // **The retreat at the herd's OWN rung** ([`fauna::herd_wariness`]) — an
                        // identity on a wild quarry, and the same term the `quarry_fight` below
                        // carries, so a raid on a managed herd cannot retreat one set of animals and
                        // fight another.
                        let wariness = fauna::herd_wariness(&herds.herds[idx], &fauna);
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
                            RaidRoll::Live(seed),
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
                            // **The WEAPON is charged per crew, for the blows it landed** — a
                            // run that could not clear the quarry's defence swung at nothing and
                            // pays nothing.
                            outcome.fight.charge_strike_wear(kit, &equipment_cfg);
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
                        // **The raid is paid its species' vector** (#337). Denial is the END STATE
                        // (the species is gone, for you and everyone else), never a promise that the
                        // party threw the carcasses away; the take is a windfall the party banks up to
                        // its pack.
                        //
                        // **BOTH accounts come out of ONE conversion of the same carried biomass**,
                        // exactly as `denial_forecast` projects the food — the raid cannot pay
                        // food it did not promise, nor pocket the hides it did. Both scale off what
                        // the party **carries**, never what it killed: you cannot tan a hide you
                        // left on the range. They then part ways only in the pack: the meat is
                        // bounded by `room`, the materials are not.
                        {
                            let carried = cohort.stores.get(FOOD);
                            let room = (cap - carried).max(scalar_zero());
                            let landed =
                                quarry_yield.apply(take.carried, EXPEDITION_OUTPUT_MULTIPLIER);
                            let provisions = scalar_from_f32(landed.provisions);
                            let added = provisions.min(room);
                            if added > scalar_zero() {
                                cohort.stores.add_food(&quarry_class, added);
                            }
                            // **The MATERIAL account of the raid** — and, on an inedible quarry, the
                            // whole of what a raid brings home. Credited on the SAME `take.carried`
                            // the food is, so a party that hauled nothing yields nothing, exactly as
                            // the resident seams do. It banks into the PARTY's own store, because a
                            // material is a batch with a characteristic vector and there is nothing
                            // to flatten it to; it travels home in `LocalStore` and merges into the
                            // band's stock at the fold-back ([`LocalStore::drain_materials_into`]).
                            crate::materials_config::credit_material_yield(
                                &mut cohort.stores,
                                &materials_cfg,
                                fauna.hunt_materials_for(&species_name),
                                take.carried,
                                EXPEDITION_OUTPUT_MULTIPLIER,
                            );
                        }

                        // **A denial raid's completion is not zero** (`docs/plan_denial_raid.md`
                        // §1.1): the party works the herd until it is **past the point of no return**
                        // — under `ecology.collapse_fraction`, where `net_biomass_delta` zeroes the
                        // growth flow and the herd declines irreversibly with no further pressure —
                        // and then walks away. It never delivers mid-trip: there is nothing to come
                        // back for, and the pack it filled on the way is what comes home.
                        let past_recovery = fauna::herd_past_recovery(
                            herd_biomass_after,
                            carrying_capacity,
                            &ecology,
                        );

                        if past_recovery {
                            // Deliver + fold back via the shared Returning arm (deposits carried food).
                            expedition.phase = ExpeditionPhase::Returning;
                            let carried = cohort.stores.get(FOOD);
                            // **The haul names BOTH accounts** (#337) — a wolf raid comes home with no
                            // meat and a pack full of hides. Since arc #527 the second account is the
                            // party's own material batches.
                            let pelts = materials_carried(&cohort.stores);
                            // The quarry by NAME, never by `fauna_id` — same rule as the lost-herd
                            // guard above, and the `detail` below carries no id at all.
                            let target = mission.target_display();
                            // **A denial raid reports the verdict, never a harvest** — it succeeded
                            // when the herd went past recovery, and what it hauled home is an aside.
                            // `floor` appears nowhere in its line (`docs/plan_denial_raid.md` §1).
                            event_log.push(CommandEventEntry::new(
                                current_turn,
                                CommandEventKind::Hunt,
                                faction,
                                format!(
                                    "Denial raid drove the {} past recovery — returning home with {}",
                                    target,
                                    describe_haul(carried.to_i64_whole(), HUNT_HAUL_FODDER, pelts)
                                ),
                                Some(format!(
                                    "status={} expedition={}",
                                    DenialOutcome::PastRecovery.as_str(),
                                    entity.to_bits()
                                )),
                            ));
                            if let Some(home) = home_pos {
                                commands.entity(entity).insert(BandTravel::to(home));
                            }
                        } else {
                            // Keep raiding: chase the herd's live tile.
                            commands.entity(entity).insert(BandTravel::to(herd_pos));
                        }
                    }
                }
            }
        }
    }
}

/// A raiding party's take applies **no** productivity multiplier: a detached party is not a
/// band, so it carries no morale/discontent output modifier (unlike the band Hunt arm, which passes
/// `output_multiplier(cohort, ..)`). Named so the forecast and the take can't disagree.
const EXPEDITION_OUTPUT_MULTIPLIER: f32 = 1.0;

/// **"There is nothing here to gather"** — the zero the replenish gather measures three of its
/// terms against: the stand's room above the floor, what one unit of its biomass converts to in
/// food, and the crew term (`workers × seasonal`) that would carry it home.
///
/// One constant for three tests because they state the same thing in three units: any of them at
/// zero means the gather cannot pay, so the party leaves the stand untouched and falls through to
/// the hunt arm. It is deliberately **not** [`NO_FORAGE_SEASON`] — that names one *input* being
/// absent, while this is the outcome the three share.
const NOTHING_TO_GATHER: f32 = 0.0;

/// **No carry bound at all**, the sentinel [`fauna::quantise_animal_take`] reads as *"the pack cannot
/// be the thing that stops this"*.
///
/// It has exactly **one** meaning on the expedition path — an **INEDIBLE** quarry, whose
/// `provisions_per_biomass` is `0`, so there is no *food* pack to fill and nothing may divide through
/// the rate (`YieldAccounts::ratio_axis`'s rule: never convert through a component you have not
/// established is positive). That is a fact about the **product**.
///
/// **It is never an INTENSITY fact.** **When a party stops engaging and how much it can haul are
/// separate questions** ([`fauna::EngagementStop`], `docs/plan_denial_raid.md` §1): denial answers
/// the first and leaves carry alone, so a raid that leaves a range full of carcasses reports them as
/// `wasted_biomass` rather than as a haul.
const NO_CARRY_BOUND: f32 = f32::INFINITY;

/// **The biomass a party still has room to haul home** — the one conversion from pack room to a
/// carry bound, shared by every take on the expedition path (the scout's roadside kill, the live
/// `Hunting` arm, and the denial projection), so a forecast cannot bound the carry differently
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

/// **How much material a party is carrying, summed over every batch** — the *"is the pack really
/// empty"* reading, and the second half of a haul's prose.
///
/// It is a bare total across materials on purpose: the question it answers is whether the party is
/// bringing anything home at all, and for that a hare pelt and a mammoth hide are both *something*.
/// Nothing downstream of it makes a quality claim — the batches themselves carry the readings, and
/// they move home unaveraged ([`crate::LocalStore::drain_materials_into`]).
///
/// It replaced `Expedition::carried_trade`, the retired scalar that banked the same haul as a number
/// (arc #527).
fn materials_carried(store: &crate::LocalStore) -> f32 {
    store
        .materials()
        .flat_map(|(_, batches)| batches.values())
        .fold(scalar_zero(), |total, batch| total + batch.amount)
        .to_f32()
}

/// **THE fold-back — the one settlement routine for a party that has come home**, shared by the
/// `Returning` arm of [`advance_expeditions`] and by an at-home `recall_expedition`, which cancels a
/// party where it stands rather than sending it on a round trip it never started.
///
/// Everything the party holds goes back into the band it was drawn from: its `working` returns to
/// the band's pool, the leftover pack lands in the band's larder, and its material batches move into
/// that **same** store. Returns how much material came home, for the feed line.
///
/// The pack is read rather than emptied: the caller despawns the party immediately after, so writing
/// it back to zero would only be bookkeeping for a corpse. The **materials** are genuinely drained,
/// because a batch carries a characteristic vector and "hand it over" is a move rather than a copy of
/// a number. **Two call sites, one routine** — the two paths differ only in *when* they fire, never
/// in what a homecoming pays.
///
/// **`cargo` is the UNDELIVERED SHIPMENT, and it settles here too.** A trade party turned back
/// because its destination could not be resolved — or cancelled in camp before it ever left — is
/// still holding real goods, and the one thing a homecoming must not do is quietly destroy them. It
/// is a *separate* store from the pack on the way out (a hungry party must not eat its own
/// shipment); on the way in, both land in the same band store, which is where the distinction stops
/// mattering.
///
/// ⛔ **THE GUARANTEE COVERS ALL THREE ACCOUNTS — food, FODDER and materials.** A shipment's manifest
/// takes hay lines, so a homecoming that settled only the first and the third would silently destroy
/// the bales: every account the load path can fill, this path has to empty.
///
/// **The two stores land together but are REPORTED apart** ([`FoldBack`]'s `pack_*` / `cargo_*`
/// fields): the pack is the band's own party coming home, the cargo is a shipment that never
/// arrived, and the ledger books them under different causes ([`FoldBack::book_home`]).
pub fn fold_party_into_band(
    party: &mut PopulationCohort,
    cargo: &mut crate::LocalStore,
    home: &mut PopulationCohort,
    gear: PartyGear<'_>,
) -> FoldBack {
    gear.hand_back();
    home.working += party.working;
    // The pack lands class by class (#706) — read, not emptied, for the reason above.
    let leftover_mix = party.stores.food().clone();
    home.stores.add_food_mix(&leftover_mix);
    let leftover = leftover_mix.total();
    // The cargo's food is genuinely taken rather than read: unlike the pack, the caller may hold the
    // party a moment longer, and a shipment counted twice is a shipment invented.
    let undelivered_mix = cargo.take_food_mix(cargo.get(FOOD));
    home.stores.add_food_mix(&undelivered_mix);
    let undelivered = undelivered_mix.total();
    // And the hay, on the same take-don't-read rule. The party's own pack is deliberately NOT read
    // for fodder: a pack is a walking larder for people, and only the shipment can hold hay.
    let undelivered_fodder = cargo.take(FODDER, cargo.get(FODDER));
    if undelivered_fodder > scalar_zero() {
        home.stores.add(FODDER, undelivered_fodder);
    }
    let materials = materials_carried(&party.stores) + materials_carried(cargo);
    let pack_materials = party.stores.drain_materials_into(&mut home.stores);
    let cargo_materials = cargo.drain_materials_into(&mut home.stores);
    home.sync_size();
    FoldBack {
        pack_food: leftover,
        cargo_food: undelivered,
        cargo_fodder: undelivered_fodder,
        materials,
        pack_materials,
        cargo_materials,
    }
}

/// **A party's gear and the ledger it goes back into** — a parameter of [`fold_party_into_band`] so
/// no fold-back path can forget it.
///
/// ⛔ **A party's kit is its home band's, taken off the band's shelf at launch** (`take_units`,
/// freshest first), so every way a party rejoins a band — a homecoming, a cancel in camp, going over
/// to another people — places its batches into the receiving band's ledger with `place_batches`,
/// **keeping their wear**. A party that is lost, or whose band is gone, loses its gear with it.
pub struct PartyGear<'a> {
    /// The party's own ledger, emptied by the hand-back.
    pub party: Option<&'a mut BandEquipment>,
    /// The receiving band's ledger.
    pub home: Option<&'a mut BandEquipment>,
}

impl PartyGear<'_> {
    /// Move every batch the party carries into the receiving band's ledger, wear and all.
    fn hand_back(self) {
        let (Some(party), Some(home)) = (self.party, self.home) else {
            return;
        };
        let items: Vec<String> = party.batches().map(|(item, _)| item.to_string()).collect();
        for item in items {
            let units = party.count_of(&item);
            let batches = party.take_units(&item, units);
            home.place_batches(&item, batches);
        }
    }
}

/// **What a homecoming handed over**, so a caller can both narrate it and book it.
///
/// The material total is the feed line's *"is the pack really empty"* reading; the food is the
/// **food ledger's** transfer term ([`LaborAllocation::last_food_transfers`]) — a party's pack
/// landing in a band's larder passes through neither income nor consumption, exactly like a
/// supply-network move. Returning both from one routine is what stops the prose and the ledger
/// disagreeing about one arrival.
///
/// **Pack and cargo are separate fields because they are separate stores** — the party's own
/// `stores` and [`Expedition::cargo`] — and the ledger books them under different causes.
pub struct FoldBack {
    /// The party's own leftover pack — provisions or a hunt's take.
    pub pack_food: Scalar,
    /// The undelivered shipment's food.
    pub cargo_food: Scalar,
    /// **The undelivered HAY that came home** — the fodder ledger's route term, on its own field
    /// rather than summed into the food, because the two accounts never convert and the food
    /// identity closes over the food larder alone. Cargo only: a pack never holds hay.
    pub cargo_fodder: Scalar,
    pub materials: f32,
    /// **The pack's material batches**, at the reading each carried — a raid's hides.
    pub pack_materials: Vec<(String, crate::components::MaterialDraw)>,
    /// **The undelivered shipment's material batches**, at the reading each carried.
    pub cargo_materials: Vec<(String, crate::components::MaterialDraw)>,
}

impl FoldBack {
    /// ⛔ **BOOK A HOMECOMING — ONE ROUTINE FOR BOTH FOLD-BACK SITES** (the `Returning` arm and a
    /// cancel in camp), for the reason [`fold_party_into_band`] is one routine: the two differ only in
    /// *when* they fire. Every row names the party; the two stores book under two causes:
    ///
    /// - **the pack** is [`TransferCause::PartyHome`], with no counterparty — the band's own people
    ///   coming back, not another band;
    /// - **the cargo** is [`TransferCause::ShipmentReturned`], naming `consignee` — the destination
    ///   the launch's `ShipmentOut` named ([`ExpeditionMission::consignee`]), so the returned
    ///   shipment answers its launch instead of reading as the band's own haul.
    pub fn book_home(
        &self,
        allocation: &mut LaborAllocation,
        party: Option<BandId>,
        consignee: Option<TransferCounterparty>,
    ) {
        allocation.book_crossing(
            TransferCrossing::goods(
                FOOD,
                TransferDirection::In,
                TransferCause::PartyHome,
                self.pack_food.to_f32(),
            )
            .with_party(party),
        );
        allocation.book_material_draws(
            &self.pack_materials,
            TransferDirection::In,
            TransferCause::PartyHome,
            None,
            party,
        );
        for (commodity, amount) in [(FOOD, self.cargo_food), (FODDER, self.cargo_fodder)] {
            allocation.book_crossing(
                TransferCrossing::goods(
                    commodity,
                    TransferDirection::In,
                    TransferCause::ShipmentReturned,
                    amount.to_f32(),
                )
                .with_counterparty(consignee)
                .with_party(party),
            );
        }
        allocation.book_material_draws(
            &self.cargo_materials,
            TransferDirection::In,
            TransferCause::ShipmentReturned,
            consignee,
            party,
        );
    }
}

impl FoldBack {
    /// **Book a defection on the RECEIVING band** — everything [`fold_party_into_band`] handed over,
    /// pack and cargo alike, under [`TransferCause::PartyDefected`], naming `origin` (the band the
    /// party was sent out from) and the party. One cause for both stores, because to the receiver
    /// neither is a shipment it was owed nor its own people's haul: it is what strangers arrived
    /// carrying.
    pub fn book_defected(
        &self,
        allocation: &mut LaborAllocation,
        party: Option<BandId>,
        origin: Option<TransferCounterparty>,
    ) {
        for (commodity, amount) in [
            (FOOD, self.pack_food + self.cargo_food),
            (FODDER, self.cargo_fodder),
        ] {
            allocation.book_crossing(
                TransferCrossing::goods(
                    commodity,
                    TransferDirection::In,
                    TransferCause::PartyDefected,
                    amount.to_f32(),
                )
                .with_counterparty(origin)
                .with_party(party),
            );
        }
        for draws in [&self.pack_materials, &self.cargo_materials] {
            allocation.book_material_draws(
                draws,
                TransferDirection::In,
                TransferCause::PartyDefected,
                origin,
                party,
            );
        }
    }
}

/// The `ExpeditionReturned` feed line a fold-back publishes, built in one place so the two call
/// sites of [`fold_party_into_band`] cannot describe the same event differently.
///
/// **Its detail stays `status=returned` for a cancel too.** Nothing about the *world* differs
/// between a cancel and a homecoming — the same workers, pack and hides land in the same band — so a
/// second status word here would encode *how the fold-back was triggered* into a field that
/// otherwise reports *what happened*, and every reader would then have to know both. The cancel is
/// named where it belongs, on the `ExpeditionRecalled` **ack** that answers the button press
/// (`status=cancelled`), which is a fact about the order rather than about the world.
pub fn expedition_returned_event(
    turn: u64,
    faction: FactionId,
    at: UVec2,
    banked_materials: f32,
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
            "status=returned materials={:.*} expedition={}",
            HAUL_MATERIAL_DECIMALS,
            banked_materials,
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
/// Food and materials are deliberately **not** part of this test: [`fold_party_into_band`] settles
/// both exactly as the `Returning` arm does, so making a party standing in camp with a full pack wait
/// a turn would reintroduce the round trip a cancel exists to remove.
pub fn party_owes_a_report(expedition: &Expedition) -> bool {
    !expedition.pending_reveal.is_empty()
}

/// A haul as feed-line prose — *"12 provisions"*, *"4.00 materials"*, *"12 provisions and 4.00
/// materials"*, or, for a shipment, *"12 provisions, 5 fodder and 4.00 materials"*. **A zero
/// component is omitted, never printed** (the render-only-when-non-zero rule the whole yield-vector
/// arc runs on): a wolf raid does not report "0 provisions", and a species nothing is made out of
/// does not report "0 materials". All three zero is not this function's case — the caller reports an
/// empty pack with its cause instead.
///
/// **Three accounts, three terms, never a total.** A sum of bread, hay and hide would be the retired
/// trade axis wearing the retired `upkeep_per_biomass` as a hat.
///
/// **Hay prints as a WHOLE COUNT, like the provisions beside it and unlike the materials** — the two
/// commodity accounts are the same kind of thing, drawn from one store and rounded the same way, and
/// a sentence that gave one of them decimals and not the other would read as a claim about hay that
/// is not true. The rounding is [`Scalar::to_i64_whole`]'s, so a sub-unit delivery reads `0` exactly
/// as a sub-unit food delivery always has.
///
/// Materials print to [`HAUL_MATERIAL_DECIMALS`] rather than as a whole count: the batch store is
/// fixed-point, so a raid can honestly come home with a *fraction* of a hide, and a whole-count
/// readout would print "0 materials" over a pack that really did bank pelts.
fn describe_haul(provisions: i64, fodder: i64, materials: f32) -> String {
    // The parts a haul actually has, in a fixed order, so two hauls of the same shape read the same
    // way. A zero account contributes nothing rather than a "0" — the render-only-when-non-zero rule
    // the whole yield-vector arc runs on.
    let mut parts: Vec<String> = Vec::new();
    if provisions > 0 {
        parts.push(format!("{provisions} provisions"));
    }
    if fodder > 0 {
        parts.push(format!("{fodder} fodder"));
    }
    if materials > 0.0 {
        parts.push(format!("{materials:.*} materials", HAUL_MATERIAL_DECIMALS));
    }
    match parts.len() {
        // Nothing at all is not this function's case; the caller reports an empty pack with its
        // cause. Reaching here anyway must say something, and "0 provisions" is what it has always
        // said.
        0 => format!("{provisions} provisions"),
        1 => parts.remove(0),
        _ => {
            let last = parts.remove(parts.len() - 1);
            format!("{} and {last}", parts.join(", "))
        }
    }
}

/// **A hunting or denial party's pack holds no hay, ever.** Fodder reaches a party's hands only as
/// trade cargo, which rides `Expedition::cargo` rather than the pack, so the raid feed lines quote
/// this rather than reading an account that cannot be non-zero. It is a *fact about the mission*,
/// not a placeholder: a raid that came home with hay would be a bug, not a prose case.
const HUNT_HAUL_FODDER: i64 = 0;

/// Decimal places a feed line prints a fractional material haul to — enough to show a sub-unit pack
/// (a wolf raid's ~0.4 hides) without turning the line into a float dump.
const HAUL_MATERIAL_DECIMALS: usize = 2;

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

/// **THE** raiding party's per-turn take, in *biomass*. The `ExpeditionPhase::Hunting` arm and the
/// denial projection ([`denial_forecast`]) both resolve through this one function, so a preview can
/// never quote a different take than the raid.
///
/// **The party works the stock standing above `floor`** (a fraction of `K`) as fast as its
/// throughput allows. A denial raid passes [`STRIP_IT_BARE`]: its ceiling is the herd's whole
/// standing stock.
///
/// **A raid brings home a PARTIAL when it must, and wastes the rest — reconciled with the band.** The
/// `credit` accumulator meters *when* the next whole animal is **ready** (a body heavier than one
/// turn's processing `throughput` takes `body / throughput` turns). Once the standing stock has banked
/// one whole animal (`affordable >= 1`) the party kills what it brought down, carries the pack's
/// worth, and **wastes the remainder** ([`fauna::quantise_animal_take`]). When it has NOT banked an
/// animal (`affordable == 0`) the party kills nothing and waits.
///
/// **The engagement bound applies to a raid exactly as it does to a resident band**
/// (`docs/plan_hunt_through_combat.md` §1 — the stages are the hunt's, not the band's; §10 exempts
/// only the pen). So the party's reach (`fauna::animals_engaged`) and the quarry's retreat
/// (`fauna::animals_that_stay`) are resolved here and handed to the one quantiser.
///
/// **A detached party builds nothing and holds nothing**, so its whole crew engages — a rung
/// transition is place-bound work no mission can name.
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
    // **Live or forecast** — see [`RaidRoll`].
    roll: RaidRoll,
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
                expected_brought_down: 0.0,
                casualties: fauna::FightCasualties::default(),
                fought: false,
                wounds: quarry.wounds,
                strike_charges: Vec::new(),
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
    let engaged = fauna::animals_engaged(workers as f32, engage_rate)
        // **Restraint is free** — the mission's floor bounds what the party goes after, so a raid at
        // its floor takes no casualties for animals it was never going to kill (§1).
        .min(fauna::animals_affordable(ceiling, body_mass));
    // **Through the PARTY, so the kit's `dispersion` reaches the retreat** — the same seam
    // `systems::hunt_take` uses; see the note there. A raid quoted for a trapping party must project
    // the trap's stand-off, and `expedition_take_biomass` is both the raid's take and its own
    // forecast, so the two cannot diverge once this is right.
    let (stayed, fight) = match roll {
        RaidRoll::Live(seed) => {
            let draw = fauna::HuntDraw::Seeded(seed);
            let stayed = party.stayers(engaged, wariness, draw);
            // **The fight decides the kill** (§4) — the same resolution the resident band runs.
            // A detached party builds nothing, so its whole crew fights.
            (
                stayed,
                fauna::resolve_hunt_fight(stayed, workers as f32, party, &quarry, draw),
            )
        }
        // **A forecast reads the kill over the retreat's OUTCOMES, never at their mean** — the
        // fight clamps each turn's blow to the bodies standing and that clamp is concave, so the
        // blow at the mean head count over-reads the take (`fauna::retreat_outcomes`). The same
        // seam the resident band's projections read ([`fauna::kill_over_retreat`]), banked into
        // whole bodies on the quarry's own wound ledger exactly as `fauna::KillCarry` banks them.
        RaidRoll::Forecast(reading) => {
            let outcomes = party.stayer_outcomes(engaged, wariness);
            let bodies = fauna::kill_over_retreat(
                engaged,
                wariness,
                workers as f32,
                party,
                Some(&quarry),
                quarry.wounds,
                fauna::EngagementQuantum::WholeAnimals,
                reading,
            );
            let mut wounds = quarry.wounds;
            let brought_down = wounds.bank_units(bodies, &quarry.profile);
            (
                fauna::expected_stayers(&outcomes),
                fauna::HuntFight {
                    brought_down,
                    expected_brought_down: bodies,
                    // A forecast charges nothing and reports no battle — nothing it reads needs
                    // either, and a projected casualty is not one the band has taken.
                    casualties: fauna::FightCasualties::default(),
                    fought: false,
                    wounds,
                    strike_charges: Vec::new(),
                },
            )
        }
    };
    // Whole animals through **the** quantiser: as many as the bank has readied, bounded by what the
    // party brought down and by what the pack can seat but never below one — so if the pack cannot
    // seat one (`carryable == 0`) while the herd has banked one, the party still kills ONE and wastes
    // what it cannot haul, and with no banked animal it kills nothing and waits (the true no-surplus
    // case).
    let take = fauna::quantise_animal_take(room, body_mass, fight.brought_down, stop);
    // Drain the bank by what was KILLED (carried + wasted), not merely carried — you cannot un-kill the
    // animal you could not haul. Cap at the surplus so it can't grow unbounded at the floor (surplus <
    // body ⇒ no kill ⇒ the bank would otherwise climb every turn). `0 ≤ credit ≤ surplus`.
    *credit = (*credit + rate - take.killed_biomass())
        .max(0.0)
        .min(standing_surplus);
    let brought_down = fight.brought_down;
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
            brought_down,
            stop,
        ),
    }
}

/// The shared **"take food from a nearby source"** primitive (`docs/plan_exploration_and_sites.md`
/// §2b). Resolves the stance's escapement ceiling ([`fauna::hunt_escapement_ceiling`] — the single
/// source), rounds it to **whole animals** against the party's collection
/// ([`fauna::quantise_animal_take`] — the single quantiser), and **subtracts every animal killed from
/// the herd**. One code path for two callers: the band Hunt labor (`advance_labor_allocation`,
/// which additionally accrues husbandry from the same take) and the scout's opportunistic replenish
/// (`advance_expeditions`, `output_multiplier = 1.0`). **Both credit
/// both components of the species' [`HuntYield`]** (#337) — they differ only in *when* the trade half
/// is banked: the band rounds it per turn, a detached party carries the batches home in its own
/// [`LocalStore`] ([`LocalStore::drain_materials_into`]).
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
/// answer, and `core_sim/tests/band_hunt_preview.rs` pins it to this function.
/// **One turn's hunt, both sides of it** — what came home, and what the fight cost.
///
/// The two used to be resolved by two unrelated code paths that could disagree
/// (`docs/plan_hunt_through_combat.md` §0.1); they are one resolution now, so they come back
/// together and no caller can apply one without the other.
// **Clone, not `Copy`** — it carries a [`fauna::HuntFight`], which carries the strike charges the
// party's crews are billed for.
#[derive(Debug, Clone, PartialEq)]
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
/// | `engaged` | animals brought into contact (§2) — a **rate**, printed fractional |
/// | `fled` | of those, how many broke off before contact (§3) — real since the roster's wariness was authored, and fractional for the same reason |
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
/// **⛔ `engaged` AND `fled` ARE FRACTIONAL, AND ARE PRINTED TO THREE PLACES LIKE EVERY OTHER RATE
/// ON THE LINE.** `fauna::animals_engaged` is `workers × engage_rate` — no floor, no `.max(1)` — so
/// one hunter on a wary quarry reaches a third of an animal. Rounding the token to whole animals
/// (`{:.0}`) printed `engaged=0 fled=0 killed=0` on every waiting turn, an entry that passed its own
/// `engaged > NOTHING_ENGAGED` gate while asserting the party reached nothing — exactly the *"we
/// never got near them"* reading the fractional reach exists to replace.
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
            "engaged={:.3} fled={:.3} killed={} carried_biomass={:.3} wasted_biomass={:.3} \
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
    workers: f32,
    floor: f32,
    per_worker_biomass_capacity: f32,
    // The hunters' own strength — kit composed in — and the tuning they fight at. The take's kill
    // arm IS this fight (`docs/plan_hunt_through_combat.md` §4), so a party that cannot beat the
    // quarry's `defense` comes home with nothing however much the herd could spare.
    party: &fauna::HuntingParty,
    fauna: &FaunaConfig,
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
    // **THE TAKE'S BOUND IS THE ROOM *OR* THE GROWTH SHARE** (`fauna::take_room`), not the raw
    // escapement room: a source pushed below its own floor by the `K` its improvement raised still
    // hands over the share of this turn's growth the player's floor left takeable. At `floor = 1.0`
    // the share is `x 0`, so "leave the whole herd standing" is unchanged.
    // **Engagement, retreat, fight — through the ONE seam that defines them**
    // ([`fauna::resolve_hunt_engagement`]), so the pre-commit crew-take curve the Assign Herders
    // panel reads resolves the very stages this take runs rather than a client-side echo of two of
    // them. The escapement room the engagement is clamped by comes back on the same value, because
    // the quantiser below needs the identical number.
    // **BODIES** — this resolves a turn, so the room and the retreat are floored to whole animals
    // exactly as they always were. Only the crew curve asks for a rate.
    let engagement = fauna::resolve_hunt_engagement(
        herd,
        fauna,
        party,
        workers,
        floor,
        draw,
        fauna::EngagementQuantum::WholeAnimals,
    );
    let fauna::HuntEngagement {
        ceiling,
        engaged,
        stayed,
        fight,
    } = engagement;
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
    // **`workers` IS THE TAKE CREW, and nothing scales it** (`docs/plan_standing_upkeep.md` §2.2):
    // a resident band gentling or fencing this herd staffs that build in its own right, so the
    // hunters here are only ever hunters. An expedition passes [`NO_IMPROVEMENT_UNDERWAY`] anyway,
    // because a rung transition is place-bound work a detached party cannot do — and since #442 its
    // mission type cannot even name one.
    // **Through the one seam** ([`fauna::herd_collection`]) — `workers × per_worker` on the range and
    // on a halter, and no bound at all at a pen once `husbandry.pen_is_a_larder` is on. The caller's
    // `carry_room_biomass` still clamps it: a resident band passes `f32::INFINITY`, and a detached
    // party's pack is a real limit whatever rung the quarry stands on.
    let collection = fauna::herd_collection(herd, fauna, workers, per_worker_biomass_capacity)
        .min(carry_room_biomass.max(0.0));
    // **The ledger goes straight back onto the herd**, before anything can early-return past it. The
    // seam above returns it by value rather than mutating, so a forecast can resolve the same fight
    // and drop it; a live take is the caller that keeps it.
    herd.wounds = fight.wounds;
    let take = fauna::quantise_animal_take(
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
    let brought_down = fight.brought_down;
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
            brought_down,
            fauna::EngagementStop::WhenPackFull,
        ),
    }
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
/// component for every species. This constant survives only as the resident band's rough per-hunter
/// arithmetic before a herd is chosen.
///
/// **Snapped to the `Scalar` grid** the larder actually accumulates on — the take path quantizes
/// every take through `Scalar::from_f32`, so the honest per-worker constant is the *quantized* one.
/// The raw `f32` product runs a hair low (40 × 0.02 = 3.1999999, not 3.2, once scaled by a
/// 4-worker party), and that sliver is enough to turn an exactly-divisible trip into a phantom extra
/// turn in any `ceil()` downstream — including the client's, which multiplies this constant by the
/// party size. Snapping here keeps the exported constant on the same grid as the sim's reality.
pub fn hunt_per_worker_provisions(equipped_haul_rate: f32, fauna: &FaunaConfig) -> f32 {
    scalar_from_f32(
        equipped_haul_rate * fauna.hunt.provisions_per_biomass * EXPEDITION_OUTPUT_MULTIPLIER,
    )
    .to_f32()
}

/// **How a raid take resolves its two stochastic stages** — drawn, or read off their distribution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RaidRoll {
    /// **A live raid** — the retreat and the attack rolls drawn from this per-event seed
    /// ([`fauna::retreat_seed`]), never a shared RNG stream, or raid ordering would change outcomes
    /// and rollback would stop reproducing (§6.2).
    Live(u64),
    /// **A forecast** — a projection has no tick to seed with (`fauna::HuntDraw`), so it reads the
    /// kill off the retreat's outcomes: their mean, or one edge of the band.
    Forecast(fauna::TakeReading),
}

/// **How a denial raid ended** — and the reason
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
    /// It exists so a consumer holding the wire `String` (a `DenialRow::outcome`) can
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

/// **What a denial raid does to a herd, before it is launched** — the mission's readout
/// (`docs/plan_denial_raid.md` §1.1). Produced by
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
    /// **What the raid actually LANDS, per material** — the same haul
    /// [`Self::delivered_food`] converts, and on an **inedible** quarry the whole of it.
    ///
    /// Projected off the same `take.carried` accumulator `delivered_food` is, through the expression
    /// the live arm's `credit_material_yield` is paid on — so a denial launch sheet's promise is
    /// checkable against what the party brings home.
    ///
    /// **Its WASTE twin is deliberately absent**, and not for the reason the retired `wasted_trade`
    /// was: a per-material vector states a projection perfectly well (this field proves it). Ray has
    /// ruled the waste line out of scope — the waste is already legible as a percentage, so a
    /// `wasted_material` buys a second reading of a fact the sheet states. Do **not** add a flat
    /// "wasted materials" scalar: that is the retired trade axis under a new name.
    pub delivered_material: Vec<crate::materials_config::MaterialPayoff>,
    // **RETIRED: `delivered_trade` / `wasted_trade`** (arc #527), with the trade axis they were the
    // two halves of. The delivered half was replaced by [`Self::delivered_material`] above; the
    // waste half was **not**, and the reason is the ruling recorded on that field rather than an
    // expressiveness problem — a per-material vector states a projection perfectly well, which is
    // precisely what the field above does. What remains true is the consequence: on an inedible
    // quarry a denial raid's whole destruction is in HIDES, and `wasted_food` reports `0` beside a
    // large `animals_killed` for it.
}

/// One quantile's worth of [`denial_forecast`]'s forward simulation.
struct DenialProjection {
    turns: Option<u32>,
    outcome: DenialOutcome,
    animals_killed: u32,
    delivered_food: f32,
    wasted_food: f32,
    /// The biomass the party carried home over the raid — what the material projection is the
    /// conversion of, kept in biomass so there is one accumulator beside `delivered_food` and the
    /// two readouts describe one haul.
    carried_biomass: f32,
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
/// **Each run reads the kill off the retreat's outcomes** ([`RaidRoll::Forecast`]): the middle run at
/// their mean, the two ends at [`fauna::retreat_band_edge`]'s quantiles of them, each turn — so an
/// end of the band is a projection in which every turn's kill is one the retreat can produce, never a
/// head count between two outcomes.
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
    let at = |reading: fauna::TakeReading| {
        denial_projection_at(
            workers,
            herd,
            fauna,
            per_worker_haul,
            expedition,
            party,
            RaidRoll::Forecast(reading),
        )
    };
    let likely = at(fauna::TakeReading::Mean);
    DenialForecast {
        turns_to_collapse: likely.turns,
        turns_to_collapse_low: at(fauna::TakeReading::Edge {
            sigmas: range_sigmas.abs(),
        })
        .turns,
        turns_to_collapse_high: at(fauna::TakeReading::Edge {
            sigmas: -range_sigmas.abs(),
        })
        .turns,
        outcome: likely.outcome,
        animals_killed: likely.animals_killed,
        delivered_food: likely.delivered_food,
        wasted_food: likely.wasted_food,
        // **The material half of the same haul** — the species' own rows over the biomass the
        // likely projection carried, through the one seam the live credit is paid on.
        delivered_material: crate::materials_config::material_yield_totals(
            fauna.hunt_materials_for(&herd.species),
            likely.carried_biomass,
            EXPEDITION_OUTPUT_MULTIPLIER,
        ),
    }
}

/// **How much of the projection the headway verdict is read over** — the second half
/// (`forecast_horizon_turns / 2`). Expressed as a divisor of the horizon rather than as a turn count
/// so it scales with the one lever that sets the projection's length, and wide enough that the
/// float noise around a converged equilibrium cannot decide the verdict.
const DENIAL_PROGRESS_WINDOW_DIVISOR: u32 = 2;

/// One quantile of the denial projection — the `Logistics` regrowth then the `Population` take, turn
/// by turn, in the live order, until the herd is **past recovery** or the horizon runs out.
///
/// **The pack does not short-circuit it.** The pack decides only what comes home, so a party that
/// can carry nothing still erases the herd and simply wastes all of it.
#[allow(clippy::too_many_arguments)] // every config the forward simulation reads is a lever
fn denial_projection_at(
    workers: u32,
    herd: &Herd,
    fauna: &FaunaConfig,
    // The party's per-hunter haul rate — see `denial_forecast`.
    per_worker_haul: f32,
    expedition: &ExpeditionConfig,
    party: &fauna::HuntingParty,
    roll: RaidRoll,
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
    // The rung's own retreat ([`fauna::herd_wariness`]) — an identity on a wild quarry.
    let wariness = fauna::herd_wariness(&quarry, fauna);
    // The one term that changes every projected turn (§4.2) — a raid spanning turns wears the quarry
    // down, and a projection that froze the wounds could not see a multi-turn kill at all.
    let mut quarry_fight = fauna::herd_quarry_fight(&quarry, fauna);
    let mut larder = scalar_zero();
    let mut animals_killed = 0u32;
    let mut delivered_food = 0.0_f32;
    // The biomass the party carries home — see `DenialProjection::carried_biomass`.
    let mut carried_biomass = 0.0_f32;
    let mut wasted_food = 0.0_f32;
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
                carried_biomass,
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
            roll,
            fauna::EngagementStop::Never,
            &mut quarry.hunt_credit,
        );
        let take = outcome.take;
        quarry_fight = quarry_fight.with_wounds(outcome.fight.wounds);
        quarry.biomass -= take.killed_biomass();
        animals_killed += take.killed;
        let landed = hunt_yield.apply(take.carried, EXPEDITION_OUTPUT_MULTIPLIER);
        delivered_food += landed.provisions;
        carried_biomass += take.carried;
        // **The waste is FOOD ONLY, and that is a ruling rather than an omission.** The delivered
        // pair above accumulates both products off one conversion; this deliberately does not, so an
        // inedible quarry's waste reads `0` beside a large `animals_killed`. Ray ruled the
        // per-material waste out of scope: the waste is already legible as a percentage, so a
        // `wasted_material` vector buys a second reading of a fact the sheet states. **A flat
        // "wasted materials" scalar is the one thing that must NOT be added here** — that is the
        // retired trade axis under a new name. See `DenialForecast::delivered_material`.
        let left_on_the_range = hunt_yield.apply(take.wasted, EXPEDITION_OUTPUT_MULTIPLIER);
        wasted_food += left_on_the_range.provisions;
        let room = (cap - larder).max(scalar_zero());
        larder += scalar_from_f32(landed.provisions).min(room);

        if fauna::herd_past_recovery(quarry.biomass, capacity, &ecology) {
            return DenialProjection {
                turns: Some(turn),
                outcome: DenialOutcome::PastRecovery,
                animals_killed,
                delivered_food,
                wasted_food,
                carried_biomass,
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
        carried_biomass,
    }
}

#[cfg(test)]
mod hunt_report_tests {
    //! What [`hunt_report_event`] prints — the feed line a player reads a hunt off.

    use super::{hunt_report_event, HuntOutcome, NOTHING_ENGAGED};
    use crate::fauna::{AnimalTake, FightCasualties, HuntFight, HuntTakeBound};
    use crate::FactionId;

    /// A lone hunter's reach on a wary quarry: under one animal, and every other number of the wait
    /// turn at zero. The numbers are the shape `fauna::animals_engaged` produces, not a re-derivation
    /// of it — this test is about the **printing**.
    const A_THIRD_OF_AN_ANIMAL: f32 = 0.33;
    const A_QUARTER_OF_THAT_FLED: f32 = 0.0825;

    /// **A FRACTIONAL REACH MUST PRINT AS ONE.** `animals_engaged` is a rate (`workers ×
    /// engage_rate`) with no floor and no `.max(1)`, so a lone hunter reaches a third of an animal —
    /// and rounding the token to whole animals published `engaged=0 fled=0 killed=0`, an entry whose
    /// own gate had just certified that something *was* engaged.
    #[test]
    fn a_fractional_reach_prints_as_a_fraction_not_as_nothing() {
        let outcome = HuntOutcome {
            take: AnimalTake::default(),
            fight: HuntFight {
                brought_down: 0.0,
                expected_brought_down: 0.0,
                casualties: FightCasualties::default(),
                fought: false,
                wounds: Default::default(),
                strike_charges: Vec::new(),
            },
            engaged: A_THIRD_OF_AN_ANIMAL,
            fled: A_QUARTER_OF_THAT_FLED,
            bound: HuntTakeBound::Engagement,
        };
        let entry = hunt_report_event(1, FactionId(1), "Wild Boar", &outcome)
            .expect("a reach above nothing is a hunt that happened");
        let detail = entry.detail.expect("the facts ride the detail");
        assert!(
            detail.contains("engaged=0.330") && detail.contains("fled=0.083"),
            "a third of an animal is what the party reached, and the line must say so: {detail}"
        );
        assert!(
            outcome.engaged > NOTHING_ENGAGED,
            "the fixture must clear the event's own gate, or it proves nothing"
        );
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
