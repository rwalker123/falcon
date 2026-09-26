//! **THE WORK PARTY, AS A CARAVAN** — what a Hunt or Forage row becomes once its source drifts past
//! the distance the band's own hands reach (`docs/plan_civilization_steps.md` §One work party).
//!
//! # A PARTY IS NOT AN ENTITY
//!
//! It is state on the labor assignment that staffed it. An expedition is a *detached* cohort with
//! its own component and no `ResidentBand`; a work party is the opposite by design — *the workers
//! are still the band's, they are just somewhere else* — so there is no split, no move order, no
//! follow order and no merge. The sim stands the party **at the source**: a Hunt party is wherever
//! the herd is this turn, a Forage party stands on its patch. The row that staffed it is the row
//! that reports it.
//!
//! # ⛔ THE LOCAL IDENTITY IS THE WHOLE POINT
//!
//! A source the band's own hands reach acquires **no party at all**, and every number on that row
//! is what it was before this module existed. Far work falls out of the same model rather than
//! sitting beside it, and that is only true while the zero-distance case is bit-for-bit unchanged.
//!
//! # WHAT DISTANCE COSTS — WALKING, AND NOTHING ELSE
//!
//! The party hunts as any hunt does. **When the take fills one hunter's pack, that hunter walks it
//! home, delivers it, walks back and rejoins the hunt**; the others keep working meanwhile. So the
//! share of the party on the road *falls out* of carry, take rate and distance rather than being a
//! tuned lever: in steady state, with a per-hunter take rate `r`, one pack `L` and a one-way walk of
//! `w` turns, the fraction working is
//!
//! ```text
//! L / (L + 2·w·r)
//! ```
//!
//! A slow-filling hunt loses almost nobody to walking; a fast-filling one loses a lot; and more
//! hunters land the first load sooner, because the first pack fills at the whole party's rate.
//!
//! **This replaced a porter fraction and a friction term**, both of which were numbers somebody
//! picked. Distance is paid in walking now, so nothing is lost in transit on top — that would count
//! it twice. Friction still governs band-to-band pooling, untouched.
//!
//! # NO INDIVIDUAL HUNTERS
//!
//! Nobody is simulated as a unit and nothing moves on the map. It is the codebase's ordinary
//! pattern for a fractional flow: a running total that fires an event each time it crosses a whole
//! unit (a pack), plus a short queue of the hunters currently on the road.
//!
//! # ONE STEP, SHARED BY THE TURN AND THE FORECAST
//!
//! [`WorkParty::open_turn`] and [`WorkParty::close_turn`] are the caravan's whole per-turn rule. The
//! turn calls them around its own inline take; [`forecast_caravan`] calls them around a projected
//! take ([`WorkParty::step`]). They are the same two functions, so the forecast runs the turn's own
//! code.

use bevy::math::UVec2;
use serde::{Deserialize, Serialize};

use crate::fauna::{HuntProjection, HuntingParty};
use crate::fauna_config::FaunaConfig;
use crate::flora_config::FloraConfig;
use crate::forage::ForageProjection;
use crate::labor_config::{ForageLaborConfig, LaborConfig};

/// **A walk of no length** — a source the band's own hands reach, or one a road covers the whole
/// way to. A pack delivers the turn it fills and nobody is ever absent.
pub const NO_WALK: u32 = 0;

/// Nobody from this party is on the road.
pub const NOBODY_ON_THE_ROAD: u32 = 0;

/// **No load is being carried home** — the wire's *next load home in* when nobody is walking a
/// pack. A walker already on the way back carries nothing and does not count.
pub const NO_LOAD_ON_THE_ROAD: u32 = 0;

/// **No load lands within the forecast's horizon** — the reply's `first_load_turn` sentinel. A turn
/// index is 1-based (`1` = next turn), so `0` cannot be confused with a real one.
pub const NO_LOAD_WITHIN_HORIZON: u32 = 0;

/// An empty load, or a walker who has handed over what they carried.
pub const NOTHING_CARRIED: f32 = 0.0;

/// The party's upkeep is fully covered — nothing has to come out to it.
pub const NO_DEFICIT: f32 = 0.0;

/// **ONE HUNTER ON THE ROAD** — out with a pack, or on the way back without one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Walker {
    /// **The one-way walk this hunter set out on**, in turns. It is fixed at dispatch: a herd that
    /// drifts further changes the *next* porter's walk, never the one already on the road.
    pub walk_turns: u32,
    /// Turns since this hunter left the party with the pack.
    pub turns_out: u32,
    /// The food in the pack — handed to the home band when `turns_out` reaches `walk_turns`, and
    /// [`NOTHING_CARRIED`] on the way back.
    pub food: f32,
}

impl Walker {
    /// Still walking a load home — the pack has not been handed over yet.
    fn carrying(&self) -> bool {
        self.turns_out < self.walk_turns
    }

    /// **Back with the party.** Out for the whole round trip — `2 · walk_turns` turns absent — and
    /// present for the take on the turn after it.
    fn rejoined(&self) -> bool {
        self.turns_out > 2 * self.walk_turns
    }
}

/// **WHERE A BAND'S WORKERS ARE STANDING WHEN THEY ARE NOT STANDING WITH THE BAND** — one per far
/// Hunt/Forage row, on the row that staffed it.
///
/// The geometry ([`Self::position`], [`Self::walk_tiles`], [`Self::walk_turns`]) and the crew are
/// **restamped every turn** from the source's live position, which is why a Hunt party follows its
/// herd without a follow order. The caravan's own state — the walk out, the load and the road — is
/// carried from turn to turn and is what makes this a pipeline rather than a trip.
///
/// It is **outside [`crate::components::LaborAssignment`]'s equality** for `last_yields`' reason:
/// where the workers are standing is a fact about the world, not about the order the player gave.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkParty {
    /// **The tile the workers stand on** — the source's own position, re-read every turn.
    pub position: UVec2,
    /// Every hand this posting holds — the row's `workers`, at the source and on the road alike.
    pub workers: u32,
    /// **THE ONE-WAY WALK, IN TILES** ([`walk_tiles`]) — measured from the band's apron and
    /// shortened by any road between the band and the source.
    pub walk_tiles: u32,
    /// The same walk in turns ([`walk_turns`]) — what the next porter to leave will walk.
    pub walk_turns: u32,
    /// **Turns of the walk out still to go.** The whole party walks out once, when it is posted:
    /// `walk_turns` turns with no take. It is never re-raised — a herd that drifts further out
    /// changes the next porter's walk, not a second walk out.
    pub walk_out_remaining: u32,
    /// **THE LOAD** — take gathered at the source and not yet carried, in **biomass**, the carry
    /// unit a pack is measured in.
    pub load_biomass: f32,
    /// The food [`Self::load_biomass`] converts to, riding alongside it so a pack carries its own
    /// share of it home.
    pub load_food: f32,
    /// **THE HUNTERS ON THE ROAD** — at most the party, in the order they left.
    pub on_the_road: Vec<Walker>,
    /// **What the party ate out of its own take this turn** — `min(take, upkeep)`.
    ///
    /// ⛔ **It is not a second meal, and it is credited home.** These are the band's own people and
    /// the band's population consumption already feeds them wherever they stand; food eaten *at the
    /// source* is food the band did not have to carry out. Charging it again would bill the band
    /// twice for the same mouths.
    pub ate: f32,
    /// **What the party's upkeep still wants after its own take** — the food the band has to supply.
    /// `0` on a posting that feeds itself; the whole of the upkeep for one whose take is not edible,
    /// which is the case the rule produces with no per-job exemption anywhere.
    pub deficit: f32,
    /// **THE PER-TURN RATE ARRIVING AT THE HOME BAND** — the food home per turn over the forecast
    /// horizon, stepped from this party's state ([`forecast_caravan`]). The number the work row
    /// prints, so a near row and a far row are comparable figures on one board.
    pub net_rate_home: f32,
}

/// **WHAT THE SOURCE GAVE UP THIS TURN** — the take of the hunters present, in the two units the
/// caravan reads: food (what eats and what goes home) and biomass (what a pack is measured in).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SourceTake {
    pub food: f32,
    pub biomass: f32,
}

/// **The top of a caravan turn** ([`WorkParty::open_turn`]): what walked in, and who is left to work.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurnOpen {
    /// Food the walkers handed to the home band this turn.
    pub delivered: f32,
    /// **The hunters at the source** — the crew the take is priced at. `0` while the party is still
    /// walking out.
    pub present: u32,
}

/// **The foot of a caravan turn** ([`WorkParty::close_turn`]): what went home without being walked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurnClose {
    /// Food the party ate out of its own take — credited home ([`WorkParty::ate`]).
    pub ate: f32,
    /// Packs that went home on a walk of no length and so landed the turn they filled.
    pub delivered: f32,
}

impl TurnClose {
    /// Everything this half of the turn credited to the home band.
    pub fn home(&self) -> f32 {
        self.ate + self.delivered
    }
}

impl WorkParty {
    /// **Post a party at `position`.** Nothing is loaded, nobody is on the road, and the walk out
    /// starts now.
    pub fn posted(position: UVec2, walk_tiles: u32, walk_turns: u32) -> Self {
        Self {
            position,
            walk_tiles,
            walk_turns,
            walk_out_remaining: walk_turns,
            ..Self::default()
        }
    }

    /// **Restamp the geometry and the crew from today's world.** The caravan's own state — the walk
    /// out, the load, the road — is left exactly as it was.
    pub fn restamp(&mut self, position: UVec2, workers: u32, walk_tiles: u32, walk_turns: u32) {
        self.position = position;
        self.workers = workers;
        self.walk_tiles = walk_tiles;
        self.walk_turns = walk_turns;
    }

    /// Is the whole party still on its walk out?
    pub fn is_walking_out(&self) -> bool {
        self.walk_out_remaining > NO_WALK
    }

    /// **Hunters on the road, this turn** — out with a pack or on the way back. Live: it moves `0,
    /// 1, 1, 0, 2…` as packs fill and hunters rejoin, and that is the honest reading.
    pub fn hunters_on_the_road(&self) -> u32 {
        self.on_the_road.len() as u32
    }

    /// The hunters at the source — every hand not on the road, and nobody while walking out.
    pub fn hunters_present(&self) -> u32 {
        if self.is_walking_out() {
            return NOBODY_ON_THE_ROAD;
        }
        self.workers.saturating_sub(self.hunters_on_the_road())
    }

    /// **Turns until the soonest pack on the road reaches home**, or [`NO_LOAD_ON_THE_ROAD`] when
    /// nobody is carrying one.
    pub fn next_load_home_in(&self) -> u32 {
        self.on_the_road
            .iter()
            .filter(|walker| walker.carrying())
            .map(|walker| walker.walk_turns - walker.turns_out)
            .min()
            .unwrap_or(NO_LOAD_ON_THE_ROAD)
    }

    /// **STEPS 1 AND 2 OF A CARAVAN TURN** — advance the road, then the walk out.
    ///
    /// 1. Every walker takes a step. One who reaches home hands over the pack; one back from the
    ///    whole round trip rejoins the party.
    /// 2. A party still walking out counts one turn of it off and has nobody at the source.
    pub fn open_turn(&mut self) -> TurnOpen {
        let mut delivered = NOTHING_CARRIED;
        for walker in self.on_the_road.iter_mut() {
            walker.turns_out += 1;
            if walker.turns_out == walker.walk_turns {
                delivered += std::mem::replace(&mut walker.food, NOTHING_CARRIED);
            }
        }
        self.on_the_road.retain(|walker| !walker.rejoined());
        if self.is_walking_out() {
            self.walk_out_remaining -= 1;
            return TurnOpen {
                delivered,
                present: NOBODY_ON_THE_ROAD,
            };
        }
        TurnOpen {
            delivered,
            present: self.hunters_present(),
        }
    }

    /// **STEPS 4 AND 5 OF A CARAVAN TURN** — the party eats, then the packs go.
    ///
    /// 4. **The party eats first, from the take, and the eaten share goes home at once** (see
    ///    [`Self::ate`]). Only the **surplus** goes into the load, so a party on thin game eats most
    ///    of its take and walks little home.
    /// 5. **While the load holds a pack and a hunter is at the source, one hunter leaves with one
    ///    pack.** Departures therefore never exceed the hunters present. On a walk of no length the
    ///    pack lands this turn and nobody leaves at all.
    ///
    /// `pack_biomass` is one hunter's carry, seated by the web's own rule
    /// ([`crate::fauna::one_pack_biomass`] on the animal web, continuous on the plant web). What one
    /// pack cannot hold stays in the load for the next porter — nobody walks away from it, so
    /// nothing a resident band would waste is wasted here. An unbounded carry takes the whole load.
    pub fn close_turn(&mut self, take: SourceTake, upkeep: f32, pack_biomass: f32) -> TurnClose {
        let produced = take.food.max(NOTHING_CARRIED);
        let ate = produced.min(upkeep.max(NOTHING_CARRIED));
        let surplus = produced - ate;
        // The biomass that goes with the surplus food. A take worth no food at all (an inedible
        // quarry, a fibre crop) loads the whole of its biomass — the porters still walk it.
        let surplus_biomass = if produced > NOTHING_CARRIED {
            take.biomass * (surplus / produced)
        } else {
            take.biomass
        };
        self.load_biomass += surplus_biomass.max(NOTHING_CARRIED);
        self.load_food += surplus;
        self.ate = ate;
        self.deficit = (upkeep - ate).max(NO_DEFICIT);

        let mut delivered = NOTHING_CARRIED;
        let mut present = self.hunters_present();
        let loads_a_pack = |load: f32| {
            load > NOTHING_CARRIED && (!pack_biomass.is_finite() || load >= pack_biomass)
        };
        if pack_biomass > NOTHING_CARRIED {
            while present > NOBODY_ON_THE_ROAD && loads_a_pack(self.load_biomass) {
                let carried = self.load_biomass.min(pack_biomass);
                let food = self.load_food * (carried / self.load_biomass);
                self.load_biomass = (self.load_biomass - carried).max(NOTHING_CARRIED);
                self.load_food = (self.load_food - food).max(NOTHING_CARRIED);
                if self.walk_turns == NO_WALK {
                    delivered += food;
                    continue;
                }
                self.on_the_road.push(Walker {
                    walk_turns: self.walk_turns,
                    turns_out: 0,
                    food,
                });
                present -= 1;
            }
        }
        TurnClose { ate, delivered }
    }

    /// **ONE WHOLE CARAVAN TURN around a take `take` prices at the hunters present** — the forecast's
    /// entry point, and exactly [`Self::open_turn`] → the take → [`Self::close_turn`], which is the
    /// turn's own sequence. Returns everything credited home this turn and whether a load landed.
    pub fn step(
        &mut self,
        upkeep: f32,
        pack_biomass: f32,
        take: impl FnOnce(u32) -> SourceTake,
    ) -> (f32, bool) {
        let open = self.open_turn();
        let taken = take(open.present);
        let close = self.close_turn(taken, upkeep, pack_biomass);
        let load_landed = open.delivered + close.delivered > NOTHING_CARRIED;
        (open.delivered + close.home(), load_landed)
    }

    /// **EVERYTHING THE PARTY HAS, BROUGHT HOME** — the load and every walker's pack, with the road
    /// and the load emptied. What an unassign, an abandon and the unsupplied fold-back all settle
    /// into the band: a caravan that ends early must not lose what is on the road.
    pub fn hand_over_everything(&mut self) -> f32 {
        let on_the_road: f32 = self.on_the_road.iter().map(|walker| walker.food).sum();
        let load = std::mem::replace(&mut self.load_food, NOTHING_CARRIED);
        self.load_biomass = NOTHING_CARRIED;
        self.on_the_road.clear();
        load + on_the_road
    }
}

/// ⛔ **THE DISTANCE PAST WHICH A ROW POSTS A PARTY — `band_work_range`, THE SAME FOR EVERY JOB.**
///
/// It is the band's apron: the distance its own hands reach without anybody walking goods. Hunt does
/// not get its own, longer threshold — the retired `hunt_reach` was a patch over the wrong model, and
/// a party that follows its herd never roams out of range.
pub fn party_begins_past(labor: &LaborConfig) -> u32 {
    labor.band_work_range
}

/// **THE ONE-WAY WALK, IN TILES** — `max(0, hex_distance − band_work_range − road_bonus)`.
///
/// Measured from the **apron**, never from the band's hex and never from the supply network's
/// `reach_tiles`: an 8-hex source walks `8 − 2 = 6` each way and `12` for the round trip. The road
/// bonus is how far a road between the endpoints widens the free pooling reach, so a road that
/// covers the whole run takes the walk to zero.
pub fn walk_tiles(distance: u32, band_work_range: u32, road_bonus: u32) -> u32 {
    distance
        .saturating_sub(band_work_range)
        .saturating_sub(road_bonus)
}

/// **THE WALK IN TURNS** — `ceil(walk_tiles / tiles_per_turn)`. A band and its party walk at the
/// same pace, `band_move_tiles_per_turn`, which `labor_config` validates positive; a zero rate is
/// read as *no walk* rather than divided by.
pub fn walk_turns(walk_tiles: u32, tiles_per_turn: u32) -> u32 {
    if walk_tiles == NO_WALK || tiles_per_turn == 0 {
        return NO_WALK;
    }
    walk_tiles.div_ceil(tiles_per_turn)
}

/// **THE WALK FROM THIS BAND TO THIS SOURCE, OR `None` INSIDE THE APRON.** The one resolver the
/// turn, the assign-time seed and the compose-sheet query all read, so the three cannot quote a
/// posting three different distances.
///
/// The road bonus reads [`crate::supply::free_pooling_reach_tiles`] — the seam two camps pool
/// through — less the unwidened `reach_tiles`, so what a road does for a caravan and what it does
/// for pooling are one reading of one road.
#[allow(clippy::too_many_arguments)] // the geometry and the three config blocks it reads
pub fn resolve_walk(
    band_pos: UVec2,
    source_pos: UVec2,
    labor: &LaborConfig,
    supply: &crate::supply_network_config::SupplyNetworkConfig,
    roads: &crate::routes::RoadRegistry,
    widest_route_reach: u32,
    geometry: (u32, u32, bool),
) -> Option<(u32, u32)> {
    let (width, height, wrap) = geometry;
    let distance = crate::grid_utils::hex_distance_wrapped(band_pos, source_pos, width, wrap);
    if distance <= party_begins_past(labor) {
        return None;
    }
    let road_bonus = crate::supply::free_pooling_reach_tiles(
        roads,
        band_pos,
        source_pos,
        supply.reach_tiles,
        widest_route_reach,
        width,
        height,
        wrap,
    )
    .saturating_sub(supply.reach_tiles);
    let tiles = walk_tiles(distance, labor.band_work_range, road_bonus);
    Some((tiles, walk_turns(tiles, labor.band_move_tiles_per_turn)))
}

/// **THE PARTY'S OWN UPKEEP** — `workers × the per-worker draw population.rs already charges`.
/// There is deliberately no second food rate for a party: it is the same people eating the same
/// amount, somewhere else.
pub fn party_upkeep(workers: u32, per_worker_draw: f32) -> f32 {
    workers as f32 * per_worker_draw
}

/// ⛔ **CAN THE HOME BAND KEEP THIS PARTY FED?** — the deficit must be coverable from the home
/// larder, or the posting folds back. Distance is paid in walking, so the deficit is **not** grossed
/// up by a transit loss; supplies walking out to a crew are the storage arc's accounting.
pub fn larder_supplies(deficit: f32, larder: f32) -> bool {
    deficit <= NO_DEFICIT || larder >= deficit
}

/// ⛔ **WHAT A CARAVAN FORECAST IS PRICED AT — THE ROW'S STAFFED CREW, OFF THE BAND'S SHARE OF ITS
/// GEAR.** The one construction the turn, the assign-time seed and the compose-sheet query all make,
/// so the three quote one number.
///
/// **The staffed crew, not the hunters present.** The forecast steps a crew that moves every turn,
/// and neither the seed nor the query knows who is on the road this turn; the row's own head count
/// is the one input all three resolve identically. The turn's own *take* is still priced at the
/// hunters present — this is the forecast's pricing only.
///
/// **The share is struck against the band's OTHER rows plus this one**
/// ([`crate::equipment_config::BandItemBudget::with_prospective_row`]) — the construction the
/// compose-sheet curves already use, so a committed row and a prospective one are armed alike.
pub struct CaravanPricing {
    coverage: crate::equipment_config::KitCoverage,
    /// One hunter's haul at this coverage — the hunt pack's carry and the projection's crew term.
    pub hunt_carry: f32,
    /// One gatherer's basket at this coverage — the gather pack and the projection's crew term.
    pub forage_carry: f32,
}

impl CaravanPricing {
    /// Price a caravan of `workers` carrying `kit`, against `wear`, beside `other_rows`.
    pub fn resolve(
        equipment: &crate::equipment_config::EquipmentConfig,
        kit: &crate::equipment_config::KitChoice,
        workers: u32,
        wear: &crate::components::BandEquipment,
        other_rows: &[(crate::equipment_config::KitChoice, f32)],
        labor: &LaborConfig,
    ) -> Self {
        let crew = workers as f32;
        let budget = crate::equipment_config::BandItemBudget::with_prospective_row(
            other_rows.iter().map(|(kit, held)| (kit, *held)),
            kit,
            crew,
        );
        let coverage =
            equipment.coverage_from_units(kit, crew, wear, budget.share_for(crew, wear, equipment));
        let hunt_carry = coverage.weighted_rate(|kit| {
            equipment.hunt_per_worker_biomass_capacity(
                labor.hunt.per_worker_biomass_capacity,
                kit,
                wear,
            )
        });
        let forage_carry = coverage.weighted_rate(|kit| {
            equipment.forage_per_worker_biomass_capacity(
                labor.forage.per_worker_biomass_capacity,
                kit,
                wear,
            )
        });
        Self {
            coverage,
            hunt_carry,
            forage_carry,
        }
    }

    /// **The hunters as they fight this quarry**, at the resident band's **base** tuning — a party
    /// is the band's own people hunting, not a detached raid.
    pub fn hunters(
        &self,
        equipment: &crate::equipment_config::EquipmentConfig,
        wear: &crate::components::BandEquipment,
        combat: &crate::combat_config::CombatConfig,
        intrinsic: crate::combat::CombatStats,
        body_mass: f32,
    ) -> HuntingParty {
        crate::fauna::PartyResolution {
            equipment,
            coverage: &self.coverage,
            wear,
            intrinsic,
            tuning: combat.tuning(),
            hunt_injury_damage_per_animal: combat.hunt_injury_damage_per_animal,
        }
        .party_against(crate::equipment_config::Quarry::Mass(body_mass))
    }
}

/// ⛔ **A FAR ROW'S FORWARD PROJECTIONS ARE WHAT ARRIVES HOME, NOT WHAT IS TAKEN** — the one place
/// a caravan forecast is written onto a yield row, shared by the turn and the assign-time seed.
///
/// `realized` is the headline the food runway and the work board read, so a far posting publishing
/// its gross take would promise a larder food still being eaten at the source or walking home. The
/// row reads the caravan instead: `realized` is its rate home and the arrival schedule is what it
/// lands turn by turn. **`row.actual` is the caller's** — the turn's real credit, or the seed's
/// first projected turn — and the published split `meat + standing == actual` is kept by scaling the
/// two parts onto it in their own proportion.
///
/// **Nothing a hunting party brings down is wasted** (`keeps_the_carcass`): what one pack cannot
/// seat stays in the load for the next porter. A gather's `wasted` is a different fact — stock the
/// crew could not reach — and stands.
pub fn publish_caravan_projection(
    row: &mut crate::components::SourceYield,
    forecast: &CaravanForecast,
    arrivals_horizon: u32,
    keeps_the_carcass: bool,
) {
    row.realized = forecast.rate_home;
    row.arrivals = (0..arrivals_horizon as usize)
        .map(|turn| {
            forecast
                .home_by_turn
                .get(turn)
                .copied()
                .unwrap_or(NOTHING_CARRIED)
        })
        .collect();
    let parts = row.meat + row.standing;
    if parts > NOTHING_CARRIED {
        let onto_actual = row.actual / parts;
        row.meat *= onto_actual;
        row.standing *= onto_actual;
    }
    if keeps_the_carcass {
        row.wasted = NOTHING_CARRIED;
    }
}

/// **WHAT A CARAVAN DELIVERS, STEPPED FORWARD** — [`forecast_caravan`]'s answer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CaravanForecast {
    /// **Food arriving home per turn**, averaged over the turns stepped — the row's `netRateHome`.
    pub rate_home: f32,
    /// What lands home on each stepped turn, `[0]` being next turn.
    pub home_by_turn: Vec<f32>,
    /// The average number of hunters on the road across the turns stepped.
    pub mean_on_the_road: f32,
    /// **The 1-based turn the first load lands**, or [`NO_LOAD_WITHIN_HORIZON`].
    pub first_load_turn: u32,
}

/// ⛔ **THE CARAVAN, STEPPED FORWARD `horizon` TURNS FROM `start`** — the one function the turn's
/// published `netRateHome`, the assign-time seed and the compose-sheet query all answer through.
///
/// `take` is the source's projected take at a crew: it is asked every turn with the hunters present
/// (`0` while walking out, so the source still regrows), and `None` means the source is spent. The
/// run stops once the source is spent **and** nothing is left on the road or in the load, and the
/// average divides by the turns actually stepped — the rule the smooth `realized` headline follows,
/// so a spent source is not diluted by dead turns after it is gone.
pub fn forecast_caravan(
    start: &WorkParty,
    horizon: u32,
    upkeep: f32,
    pack_biomass: f32,
    mut take: impl FnMut(u32) -> Option<SourceTake>,
) -> CaravanForecast {
    let mut party = start.clone();
    let mut forecast = CaravanForecast::default();
    let mut on_the_road = 0u32;
    for turn in 1..=horizon {
        let mut spent = false;
        let (home, load_landed) = party.step(upkeep, pack_biomass, |present| {
            take(present).unwrap_or_else(|| {
                spent = true;
                SourceTake::default()
            })
        });
        forecast.home_by_turn.push(home);
        on_the_road += party.hunters_on_the_road();
        if load_landed && forecast.first_load_turn == NO_LOAD_WITHIN_HORIZON {
            forecast.first_load_turn = turn;
        }
        if spent && party.on_the_road.is_empty() && party.load_biomass <= NOTHING_CARRIED {
            break;
        }
    }
    let turns = forecast.home_by_turn.len();
    if turns > 0 {
        forecast.rate_home = forecast.home_by_turn.iter().sum::<f32>() / turns as f32;
        forecast.mean_on_the_road = on_the_road as f32 / turns as f32;
    }
    forecast
}

/// **A HUNT CARAVAN'S FORECAST** — [`forecast_caravan`] over [`HuntProjection`], the step
/// `fauna::project_realized_hunt` loops over, so the caravan forecast runs the hunt's own projected
/// take at whatever crew is present each turn.
///
/// The pack is one hunter's carry at this herd's rung ([`crate::fauna::herd_carry_rate`]) seated in
/// whole animals ([`crate::fauna::one_pack_biomass`]).
#[allow(clippy::too_many_arguments)] // the take's full context, plus the caravan's two terms
pub fn forecast_hunt_caravan(
    party: &WorkParty,
    herd: &crate::fauna::Herd,
    fauna: &FaunaConfig,
    carry_per_worker: f32,
    hunters: &HuntingParty,
    output_multiplier: f32,
    floor: f32,
    upkeep: f32,
    horizon: u32,
) -> CaravanForecast {
    let pack = hunt_pack_biomass(herd, fauna, carry_per_worker);
    let mut projection = HuntProjection::new(herd, fauna);
    forecast_caravan(party, horizon, upkeep, pack, |present| {
        projection
            .step(
                fauna,
                carry_per_worker,
                hunters,
                output_multiplier,
                present,
                floor,
            )
            .map(|turn| SourceTake {
                food: turn.yields.provisions,
                biomass: turn.biomass,
            })
    })
}

/// **One hunter's pack off this herd** — the carry at the herd's rung, seated in whole animals.
pub fn hunt_pack_biomass(
    herd: &crate::fauna::Herd,
    fauna: &FaunaConfig,
    carry_per_worker: f32,
) -> f32 {
    crate::fauna::one_pack_biomass(
        crate::fauna::herd_carry_rate(herd, fauna, carry_per_worker),
        herd.body_mass,
    )
}

/// **A GATHER CARAVAN'S FORECAST** — [`forecast_caravan`] over [`ForageProjection`], the plant twin
/// of [`forecast_hunt_caravan`]. A basket is continuous, so one pack is one gatherer's carry.
///
/// A gather at no crew takes nothing, which the projection reads as a spent stand; the caravan asks
/// at no crew every turn the party is walking out or wholly on the road, so that case is read as a
/// zero take on a stand that goes on regrowing, never as the end of the run.
#[allow(clippy::too_many_arguments)] // the gather's full context, plus the caravan's two terms
pub fn forecast_forage_caravan(
    party: &WorkParty,
    patch: &crate::forage::ForagePatch,
    tile_composition: &[crate::flora_config::FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    carry_per_worker: f32,
    seasonal: f32,
    output_multiplier: f32,
    floor: f32,
    take_species: &crate::components::TakeSelection,
    upkeep: f32,
    horizon: u32,
) -> CaravanForecast {
    let mut projection = ForageProjection::new(patch);
    forecast_caravan(
        party,
        horizon,
        upkeep,
        carry_per_worker,
        |present| match projection.step(
            tile_composition,
            forage,
            flora,
            carry_per_worker,
            seasonal,
            output_multiplier,
            present,
            floor,
            take_species,
        ) {
            Some(turn) => Some(SourceTake {
                food: turn.provisions,
                biomass: turn.biomass,
            }),
            None if present == NOBODY_ON_THE_ROAD => Some(SourceTake::default()),
            None => None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A crew-linear take with no upkeep: `present × rate` biomass, one food per biomass. The regime
    /// the steady-state formula is exact in.
    fn crew_linear(rate: f32) -> impl FnMut(u32) -> Option<SourceTake> {
        move |present| {
            let biomass = present as f32 * rate;
            Some(SourceTake {
                food: biomass,
                biomass,
            })
        }
    }

    /// **Ray's formula** — an 8-hex source walks `8 − 2 = 6` each way with no road, `12` there and
    /// back. Measured from the apron, never from the supply network's reach.
    #[test]
    fn an_eight_hex_source_walks_six_each_way() {
        assert_eq!(walk_tiles(8, 2, 0), 6);
        assert_eq!(walk_turns(walk_tiles(8, 2, 0), 1), 6);
    }

    /// A road shortens the walk, and one that covers the run takes it to zero.
    #[test]
    fn a_road_shortens_the_walk_and_one_that_covers_it_ends_it() {
        let bare = walk_tiles(8, 2, 0);
        let part = walk_tiles(8, 2, 3);
        let whole = walk_tiles(8, 2, 6);
        assert!(
            part < bare,
            "a road must shorten the walk: {part} vs {bare}"
        );
        assert_eq!(whole, NO_WALK, "a road covering the run takes it to zero");
    }

    /// ⛔ **THE STEADY STATE IS `L / (L + 2·w·r)`** — the share of the party at the source falls out
    /// of carry, take rate and distance, with no lever anywhere. Pinned as a tolerance over a long
    /// run, with a liveness conjunct: "nobody walked" also reads as a full crew at the source.
    #[test]
    fn the_steady_share_at_the_source_is_pack_over_pack_plus_the_round_trip() {
        let (workers, pack, walk, rate) = (40u32, 10.0_f32, 5u32, 1.0_f32);
        let mut party = WorkParty::posted(UVec2::ZERO, walk, walk);
        party.workers = workers;
        let mut take = crew_linear(rate);
        let (warm_up, measured) = (200u32, 2_000u32);
        let mut present_sum = 0.0_f32;
        let mut ever_walked = false;
        for turn in 0..(warm_up + measured) {
            let open = party.open_turn();
            let taken = take(open.present).unwrap();
            party.close_turn(taken, 0.0, pack);
            ever_walked |= party.hunters_on_the_road() > NOBODY_ON_THE_ROAD;
            // **The crew the take was priced at** — who worked the source this turn, which is what
            // the formula's share is a share of.
            if turn >= warm_up {
                present_sum += open.present as f32;
            }
        }
        let share = present_sum / measured as f32 / workers as f32;
        let expected = pack / (pack + 2.0 * walk as f32 * rate);
        assert!(
            ever_walked,
            "liveness: the caravan must actually send somebody home"
        );
        assert!(
            (share - expected).abs() < 0.05,
            "share at the source {share} against L/(L+2wr) = {expected}"
        );
    }

    /// **More hunters land the first load sooner** — the first pack fills at the whole party's rate.
    #[test]
    fn more_hunters_land_the_first_load_sooner() {
        let first = |workers: u32| {
            let mut party = WorkParty::posted(UVec2::ZERO, 4, 4);
            party.workers = workers;
            forecast_caravan(&party, 60, 0.0, 12.0, crew_linear(0.5)).first_load_turn
        };
        let (few, many) = (first(2), first(8));
        assert_ne!(
            few, NO_LOAD_WITHIN_HORIZON,
            "fixture: the small party must land a load"
        );
        assert!(
            many < few,
            "eight hunters must land sooner than two: {many} vs {few}"
        );
    }

    /// ⛔ **Departures never exceed the hunters present** — a take large enough to want more packs
    /// than there are hunters sends every hunter and leaves the rest in the load.
    #[test]
    fn departures_never_exceed_the_hunters_present() {
        let mut party = WorkParty::posted(UVec2::ZERO, 3, 3);
        party.workers = 3;
        party.walk_out_remaining = NO_WALK;
        party.open_turn();
        party.close_turn(
            SourceTake {
                food: 100.0,
                biomass: 100.0,
            },
            0.0,
            5.0,
        );
        assert_eq!(
            party.hunters_on_the_road(),
            3,
            "every hunter leaves with a pack"
        );
        assert_eq!(party.hunters_present(), 0);
        assert!(
            (party.load_biomass - 85.0).abs() < 1e-3,
            "the packs nobody could carry stay in the load: {}",
            party.load_biomass
        );
    }

    /// **The eaten share goes home at once, and only the surplus is loaded.**
    #[test]
    fn the_party_eats_first_and_only_the_surplus_is_carried() {
        let mut party = WorkParty::posted(UVec2::ZERO, 2, 2);
        party.workers = 4;
        party.walk_out_remaining = NO_WALK;
        party.open_turn();
        let close = party.close_turn(
            SourceTake {
                food: 10.0,
                biomass: 20.0,
            },
            4.0,
            1_000.0,
        );
        assert_eq!(close.ate, 4.0);
        assert_eq!(party.deficit, NO_DEFICIT);
        assert!((party.load_food - 6.0).abs() < 1e-5);
        assert!((party.load_biomass - 12.0).abs() < 1e-5);
    }

    /// No per-job exemption: a take worth no food runs the full deficit.
    #[test]
    fn a_take_that_is_not_food_runs_the_full_deficit() {
        let mut party = WorkParty::posted(UVec2::ZERO, 2, 2);
        party.workers = 3;
        let close = party.close_turn(SourceTake::default(), 3.0, 5.0);
        assert_eq!(close.ate, 0.0);
        assert_eq!(party.deficit, 3.0);
        assert!(!larder_supplies(party.deficit, 2.0));
        assert!(larder_supplies(party.deficit, 3.0));
    }

    /// ⛔ **The fold-back brings the road home** — every walker's pack and the load.
    #[test]
    fn handing_over_brings_every_walker_and_the_load_home() {
        let mut party = WorkParty::posted(UVec2::ZERO, 6, 6);
        party.workers = 4;
        party.walk_out_remaining = NO_WALK;
        party.open_turn();
        party.close_turn(
            SourceTake {
                food: 25.0,
                biomass: 25.0,
            },
            0.0,
            10.0,
        );
        assert_eq!(
            party.hunters_on_the_road(),
            2,
            "fixture: two packs on the road"
        );
        assert_eq!(party.hand_over_everything(), 25.0);
        assert_eq!(party.hunters_on_the_road(), NOBODY_ON_THE_ROAD);
        assert_eq!(party.load_food, NOTHING_CARRIED);
    }

    /// A walk of no length — a road covering the run — lands every pack the turn it fills and never
    /// takes anybody away.
    #[test]
    fn a_walk_of_no_length_delivers_the_turn_the_pack_fills() {
        let mut party = WorkParty::posted(UVec2::ZERO, NO_WALK, NO_WALK);
        party.workers = 2;
        let open = party.open_turn();
        assert_eq!(open.present, 2, "nobody walks out on a walk of no length");
        let close = party.close_turn(
            SourceTake {
                food: 30.0,
                biomass: 30.0,
            },
            0.0,
            10.0,
        );
        assert_eq!(close.delivered, 30.0);
        assert_eq!(party.hunters_on_the_road(), NOBODY_ON_THE_ROAD);
    }

    /// ⛔ **THE BIG CARCASS** — a quarry heavier than one pack is carried over several porters, one
    /// pack's worth each, and what none of them could shoulder stays in the load: nothing a
    /// resident band would have wasted is wasted here.
    #[test]
    fn a_carcass_heavier_than_one_pack_goes_home_over_several_porters() {
        let (carry, body) = (40.0_f32, 300.0_f32);
        let pack = crate::fauna::one_pack_biomass(carry, body);
        assert_eq!(
            pack, carry,
            "a carcass too big for one pack goes as a pack's worth"
        );
        let mut party = WorkParty::posted(UVec2::ZERO, 2, 2);
        party.workers = 10;
        party.walk_out_remaining = NO_WALK;
        party.open_turn();
        party.close_turn(
            SourceTake {
                food: body,
                biomass: body,
            },
            0.0,
            pack,
        );
        assert_eq!(
            party.hunters_on_the_road(),
            7,
            "seven packs of forty off three hundred"
        );
        let mut home = 0.0_f32;
        for _ in 0..10 {
            home += party.step(0.0, pack, |_| SourceTake::default()).0;
        }
        assert!(
            (home + party.load_food - body).abs() < 1e-2,
            "every part of the carcass is home or still in the load: {home} + {}",
            party.load_food
        );
        assert!(
            party.load_food > 0.0,
            "the part no pack could take is still there, not wasted"
        );
    }

    /// Smaller than a pack, a quarry is seated in WHOLE animals — the carrying-home reading rounds
    /// down, because nobody walks away from the rest of the load.
    #[test]
    fn a_pack_seats_whole_animals_and_rounds_down() {
        assert_eq!(crate::fauna::one_pack_biomass(40.0, 15.0), 30.0);
        assert_eq!(crate::fauna::one_pack_biomass(40.0, 40.0), 40.0);
    }

    /// ⛔ **A ROAD SHORTENS THE WALK, AND ONE THAT COVERS THE RUN ENDS IT** — through the supply
    /// network's own `free_pooling_reach_tiles`, over a real road registry, so what a road does for
    /// a caravan and what it does for pooling are one reading of one road.
    #[test]
    fn a_road_shortens_the_walk_and_one_covering_the_run_takes_it_to_zero() {
        use crate::intensification::{LadderConfig, RungKey};
        use crate::routes::{road_rung_span, trace_path, traffic_ceiling, RoadRegistry};
        let ladder = LadderConfig::builtin();
        let labor = LaborConfig::builtin();
        let supply = crate::supply_network_config::SupplyNetworkConfig::builtin();
        let widest = crate::routes::max_route_reach_tiles(&ladder);
        let (width, height) = (40, 40);
        let geometry = (width, height, false);
        let (band, source) = (UVec2::new(5, 10), UVec2::new(13, 10));
        let walk = |roads: &RoadRegistry| {
            resolve_walk(band, source, &labor, &supply, roads, widest, geometry)
                .expect("an 8-hex source is past the apron")
                .0
        };
        let run = trace_path(band, source, width, height, false, &RoadRegistry::default());
        let paved_to = |position: f32| {
            let mut roads = RoadRegistry::default();
            for tile in &run {
                roads
                    .road_or_trail(*tile, &ladder)
                    .set_position(position, &ladder);
            }
            roads
        };
        let bare = walk(&RoadRegistry::default());
        assert_eq!(bare, 6, "no road: Ray's formula");
        let trail = walk(&paved_to(traffic_ceiling(&ladder)));
        assert!(
            trail < bare,
            "a worn trail must shorten the walk: {trail} vs {bare}"
        );
        let (base, span) = road_rung_span(
            RungKey::RouteDirtRoad,
            &ladder,
            crate::routes::remoteness_multiplier(0, &ladder),
        );
        let road = walk(&paved_to(base + span));
        assert_eq!(
            road, NO_WALK,
            "a road covering the run takes the walk to zero"
        );
    }

    /// A walker delivers on the turn its walk ends and rejoins after the round trip.
    #[test]
    fn a_walker_delivers_after_the_walk_and_rejoins_after_the_round_trip() {
        let walk = 3u32;
        let mut party = WorkParty::posted(UVec2::ZERO, walk, walk);
        party.workers = 1;
        party.walk_out_remaining = NO_WALK;
        party.open_turn();
        party.close_turn(
            SourceTake {
                food: 5.0,
                biomass: 5.0,
            },
            0.0,
            5.0,
        );
        assert_eq!(party.next_load_home_in(), walk);
        let mut landed_on = None;
        let mut back_on = None;
        for turn in 1..=(3 * walk) {
            let open = party.open_turn();
            if open.delivered > 0.0 {
                landed_on = Some(turn);
            }
            if back_on.is_none() && open.present == 1 {
                back_on = Some(turn);
            }
            party.close_turn(SourceTake::default(), 0.0, 5.0);
        }
        assert_eq!(landed_on, Some(walk));
        assert_eq!(
            back_on,
            Some(2 * walk + 1),
            "absent for the whole round trip"
        );
    }
}
