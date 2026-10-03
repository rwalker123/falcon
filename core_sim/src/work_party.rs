//! **THE WORK PARTY, AS A CARAVAN** — what a Hunt, Forage or Extract row becomes once its source
//! lies past the distance the band's own hands reach (`docs/plan_civilization_steps.md` §One work
//! party).
//!
//! # A PARTY IS NOT AN ENTITY
//!
//! It is state on the labor assignment that staffed it. An expedition is a *detached* cohort with
//! its own component and no `ResidentBand`; a work party is the opposite by design — *the workers
//! are still the band's, they are just somewhere else* — so there is no split, no move order, no
//! follow order and no merge. The sim stands the party **at the source**: a Hunt party is wherever
//! the herd is this turn, a Forage party stands on its patch, an Extract party on its deposit. The
//! row that staffed it is the row that reports it.
//!
//! # WHAT GOES HOME IS CARGO, AND A PACK IS MEASURED IN BULK
//!
//! The caravan does not know what it carries. **Cargo** is what lands at home — food on the two
//! food webs, material units on the deposit web — and **bulk** is what a pack is measured in —
//! biomass on the food webs, material units on the deposit web, where the two are one number. A
//! deposit's pack is the hunt's own haul carry over the material's weight ([`material_pack`]), so
//! wood and stone travel by exactly the rule meat does, with no material-specific path anywhere.
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
//! picked. Distance is paid in walking now, so no FRACTION is lost in transit on top — that would
//! count it twice. Friction still governs band-to-band pooling, untouched.
//!
//! # WHAT THE WALK DOES COST: FOOD THAT DOES NOT KEEP THAT LONG (#706)
//!
//! **A pack rots by its walk.** Every pack carries the keeping classes of the food in it
//! ([`CargoClasses`]), and when it lands any class whose shelf life is shorter than the walk the
//! porter set out on is lost on the way — entirely, not as a share (`crate::spoilage::rots_in_transit`).
//! That is not friction under another name: it is a property of the *food* (flesh, greens, grain),
//! so the same walk costs a meat hunt everything and a nut gather nothing, and it gives a far hunt a
//! natural range that preservation extends. The landed pack is credited as income in full and the
//! rotten share debited as spoilage the same turn, so the take's one producer stays the row's
//! `actual` and the loss is the ledger's one `spoiled` term.
//!
//! # A POSTING THAT ENDS WALKS HOME — NOTHING ARRIVES AT ONCE
//!
//! **A stood-down party** (an unassign, a lapse, a herd gone or back inside the apron, a shed row,
//! `cancel_order`) becomes the band's [`HomewardWalk`]s ([`WorkParty::walk_home`]): every porter
//! carrying a pack finishes its walk, the hands at the source carry the load home over the whole
//! walk, a porter heading back out turns round, and a party still walking out walks back what it
//! has covered. Each lands when its walk ends and rots by that walk exactly as a live pack does,
//! and its hands are **away from the band until they arrive** — the list rides
//! `LaborAllocation::homeward`, outliving the row.
//!
//! # NOTHING ABOUT FEEDING IS MODELLED AT THE SOURCE
//!
//! **The home band feeds its party through its ordinary consumption**, which already charges those
//! workers wherever they stand, and supplies ride the porters' return leg. So the whole take goes
//! into the load and walks home; nothing is eaten out of it, and a party cannot be short of food
//! separately from its band.
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
//! take, striking each landing pack for transit rot as the turn does. They are the same two
//! functions, so the forecast runs the turn's own code.

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

/// **What a load or a pack is MADE OF, by keeping class** (#706) — cargo per class, in the cargo's
/// own unit. The caravan's arithmetic runs on the scalar `cargo`; this rides beside it as the
/// composition that cargo lands as, split in the same proportion every time a pack leaves the load.
/// Empty on the deposit web, whose cargo is a material and keeps for ever.
pub type CargoClasses = std::collections::BTreeMap<String, f32>;

/// **One pack that reached home this turn**, with the walk it was carried on — the two facts the
/// transit rot reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LandedPack {
    /// The one-way walk the porter set out on, in turns. [`NO_WALK`] for a pack landed without
    /// being walked (a local load, or a road the whole way).
    pub walk_turns: u32,
    /// The cargo it carried.
    pub cargo: f32,
    /// What that cargo is made of.
    pub classes: CargoClasses,
    /// **The pack's bulk** — what it weighed in the carry's unit, whatever its cargo is worth.
    pub bulk: f32,
    /// **Its fodder and materials** (#706) — landed with it, never rotted.
    pub goods: CarriedGoods,
}

/// `classes × share`, the part of a composition a fraction of its cargo carries.
fn scaled_classes(classes: &CargoClasses, share: f32) -> CargoClasses {
    classes
        .iter()
        .map(|(class, amount)| (class.clone(), amount * share))
        .collect()
}

/// Add `from` into `into`, class by class.
fn merge_classes(into: &mut CargoClasses, from: &CargoClasses) {
    for (class, amount) in from {
        *into.entry(class.clone()).or_insert(NOTHING_CARRIED) += amount;
    }
}

/// ⛔ **WHAT A PACK CARRIES BESIDES FOOD** (#706) — a forage take's fodder and flora materials, a
/// hunt's or pen's hide, bone, sinew and fleece, riding the load and every pack in the same
/// proportion as the food, and landing at home when the pack does. They do not rot. The deposit
/// web's material is the pack's cargo itself and never rides here.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CarriedGoods {
    /// Fodder (hay) — what lands in the band's `FODDER` store.
    pub fodder: f32,
    /// Each material at its exact reading — what lands through `LocalStore::deposit_material`, the
    /// one deposit a caravan's material makes.
    pub materials: Vec<CarriedMaterial>,
}

/// **One material in a pack, at one reading** — the arguments `LocalStore::deposit_material` takes,
/// carried until the pack lands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CarriedMaterial {
    pub material: String,
    pub band: crate::materials_config::BandKey,
    pub characteristics: std::collections::BTreeMap<String, f32>,
    pub amount: f32,
}

impl CarriedGoods {
    /// Nothing in it.
    pub fn is_empty(&self) -> bool {
        self.fodder <= NOTHING_CARRIED
            && self
                .materials
                .iter()
                .all(|carried| carried.amount <= NOTHING_CARRIED)
    }

    /// Add `other` in — one entry per material at one reading, so a load filled turn after turn off
    /// one basket holds one entry per material it yields.
    pub fn merge(&mut self, other: &CarriedGoods) {
        self.fodder += other.fodder;
        for carried in &other.materials {
            match self.materials.iter_mut().find(|held| {
                held.material == carried.material
                    && held.band == carried.band
                    && held.characteristics == carried.characteristics
            }) {
                Some(held) => held.amount += carried.amount,
                None => self.materials.push(carried.clone()),
            }
        }
    }

    /// **Take `share` of everything out**, for one pack — the same split the food's classes get.
    pub fn take_share(&mut self, share: f32) -> CarriedGoods {
        let fodder = self.fodder * share;
        self.fodder = (self.fodder - fodder).max(NOTHING_CARRIED);
        let mut materials = Vec::new();
        for held in &mut self.materials {
            let amount = held.amount * share;
            held.amount = (held.amount - amount).max(NOTHING_CARRIED);
            materials.push(CarriedMaterial {
                amount,
                ..held.clone()
            });
        }
        self.materials.retain(|held| held.amount > NOTHING_CARRIED);
        CarriedGoods { fodder, materials }
    }
}

/// The whole of a load or pack — [`CarriedGoods::take_share`] of this takes all of it.
const WHOLE_LOAD: f32 = 1.0;

/// **ONE HUNTER ON THE ROAD** — out with a pack, or on the way back without one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Walker {
    /// **The one-way walk this hunter set out on**, in turns. It is fixed at dispatch: a herd that
    /// drifts further changes the *next* porter's walk, never the one already on the road.
    pub walk_turns: u32,
    /// Turns since this hunter left the party with the pack.
    pub turns_out: u32,
    /// **The cargo in the pack** — what goes home: food on the two food webs, material units on
    /// the deposit web. Handed to the home band when `turns_out` reaches `walk_turns`, and
    /// [`NOTHING_CARRIED`] on the way back.
    pub cargo: f32,
    /// **What the pack is made of, by keeping class** — split off the load with the cargo.
    pub classes: CargoClasses,
    /// The pack's bulk, in the carry's unit.
    pub bulk: f32,
    /// **Its fodder and materials** (#706) — split off the load with the cargo.
    pub goods: CarriedGoods,
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
/// Hunt/Forage/Extract row, on the row that staffed it.
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
    /// **THE LOAD** — take gathered at the source and not yet carried, in **bulk**: the unit a pack
    /// is measured in. Biomass on the two food webs; material units on the deposit web.
    pub load_bulk: f32,
    /// **The cargo [`Self::load_bulk`] converts to** — what goes home (food, or material units on the
    /// deposit web, where cargo and bulk are one number), riding alongside it so a pack carries its
    /// own share of it home.
    pub load_cargo: f32,
    /// **What [`Self::load_cargo`] is made of, by keeping class** (#706) — what each take added,
    /// split off with every pack in the cargo's own proportion.
    pub load_classes: CargoClasses,
    /// **The load's fodder and materials** (#706) — what each forage take added beside its food,
    /// split off with every pack in the same proportion.
    pub load_goods: CarriedGoods,
    /// **THE HUNTERS ON THE ROAD** — at most the party, in the order they left.
    pub on_the_road: Vec<Walker>,
    /// **THE PER-TURN RATE ARRIVING AT THE HOME BAND** — the cargo home per turn over the forecast
    /// horizon, stepped from this party's state ([`forecast_caravan`]). The number the work row
    /// prints, so a near row and a far row are comparable figures on one board. **Net of transit
    /// rot** (#706): a pack whose class rots on the walk lands nothing here.
    pub net_rate_home: f32,
    /// **THE PER-TURN RATE LOST ON THE WALK HOME** (#706) — what the same forecast's landing packs
    /// lose to transit rot, averaged over the same turns. `net_rate_home + spoiled_rate_home` is
    /// what the porters carry in.
    pub spoiled_rate_home: f32,
    /// **The shortest shelf life among the classes this row's cargo is made of that rot on this
    /// walk**, in turns — `0` when nothing rots. The number a "keeps N turns, walk is M" line reads.
    pub transit_keeps_turns: f32,
    /// **THE FODDER PER TURN ARRIVING HOME** (#706) — `net_rate_home`'s twin for a far forage row's
    /// hay, off the same forecast ([`CaravanForecast::fodder_rate_home`]): smoothed, where the row's
    /// per-turn `fodder` reads only what landed.
    pub fodder_rate_home: f32,
    /// **THE MATERIALS PER TURN ARRIVING HOME**, one row per material id, off the same forecast — a
    /// basket's, or a hunt's hide, bone and sinew.
    pub materials_rate_home: Vec<crate::materials_config::MaterialPayoff>,
}

/// **What a caravan's cargo is made of, and how long each part keeps** (#706) — what the forecast
/// needs to strike transit rot on every landing pack. `shares` are fractions of the cargo by keeping
/// class (summing to 1); the deposit web carries none and is forecast with no rot at all.
#[derive(Debug, Clone)]
pub struct TransitRot<'a> {
    pub keeping: &'a crate::demographics_config::KeepingConfig,
    pub shares: CargoClasses,
}

/// No transit rot — the deposit web's cargo is a material, and a material keeps.
pub const NO_TRANSIT_ROT: Option<&TransitRot<'static>> = None;

impl TransitRot<'_> {
    /// One class, the whole cargo.
    pub fn single<'a>(
        keeping: &'a crate::demographics_config::KeepingConfig,
        class: &str,
    ) -> TransitRot<'a> {
        TransitRot {
            keeping,
            shares: std::iter::once((class.to_string(), WHOLE_CARGO)).collect(),
        }
    }

    /// The share of a pack's cargo that rots on a walk of `walk_turns` —
    /// [`crate::spoilage::rots_in_transit`] per class, the rule the turn's landing applies.
    pub fn rotten_share(&self, walk_turns: u32) -> f32 {
        self.shares
            .iter()
            .filter(|(class, _)| crate::spoilage::rots_in_transit(class, walk_turns, self.keeping))
            .map(|(_, share)| *share)
            .sum::<f32>()
            .min(WHOLE_CARGO)
    }

    /// The shortest shelf life among the classes that rot on a walk of `walk_turns`, or
    /// [`NOTHING_ROTS_ON_THE_WALK`].
    pub fn keeps_turns(&self, walk_turns: u32) -> f32 {
        self.shares
            .iter()
            .filter(|(class, share)| {
                **share > NOTHING_CARRIED
                    && crate::spoilage::rots_in_transit(class, walk_turns, self.keeping)
            })
            .filter_map(|(class, _)| self.keeping.shelf_life(class))
            .fold(None, |shortest: Option<f32>, life| {
                Some(shortest.map_or(life, |s| s.min(life)))
            })
            .unwrap_or(NOTHING_ROTS_ON_THE_WALK)
    }
}

/// The whole of a cargo, as a share.
const WHOLE_CARGO: f32 = 1.0;

/// [`WorkParty::transit_keeps_turns`] when no class of the cargo rots on the walk.
pub const NOTHING_ROTS_ON_THE_WALK: f32 = 0.0;

/// **WHAT THE SOURCE GAVE UP THIS TURN** — the take of the hunters present, in the two units the
/// caravan reads: **cargo** (what goes home — food on the two food webs, material units on the
/// deposit web) and **bulk** (what a pack is measured in — biomass on the food webs, material units
/// on the deposit web, where the two are one number).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SourceTake {
    pub cargo: f32,
    pub bulk: f32,
}

/// **The top of a caravan turn** ([`WorkParty::open_turn`]): what walked in, and who is left to work.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnOpen {
    /// Cargo the walkers handed to the home band this turn.
    pub delivered: f32,
    /// **Each pack those walkers handed over**, with the walk it came on — what the transit rot
    /// reads. Their cargo sums to [`Self::delivered`].
    pub packs: Vec<LandedPack>,
    /// **The hunters at the source** — the crew the take is priced at. `0` while the party is still
    /// walking out.
    pub present: u32,
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
        let mut packs = Vec::new();
        for walker in self.on_the_road.iter_mut() {
            walker.turns_out += 1;
            if walker.turns_out == walker.walk_turns {
                let cargo = std::mem::replace(&mut walker.cargo, NOTHING_CARRIED);
                delivered += cargo;
                packs.push(LandedPack {
                    walk_turns: walker.walk_turns,
                    cargo,
                    classes: std::mem::take(&mut walker.classes),
                    bulk: std::mem::replace(&mut walker.bulk, NOTHING_CARRIED),
                    goods: std::mem::take(&mut walker.goods),
                });
            }
        }
        self.on_the_road.retain(|walker| !walker.rejoined());
        if self.is_walking_out() {
            self.walk_out_remaining -= 1;
            return TurnOpen {
                delivered,
                packs,
                present: NOBODY_ON_THE_ROAD,
            };
        }
        TurnOpen {
            delivered,
            packs,
            present: self.hunters_present(),
        }
    }

    /// **STEPS 4 AND 5 OF A CARAVAN TURN** — the take is loaded, then the packs go. Returns the cargo
    /// that landed home this turn without being walked (a walk of no length).
    ///
    /// 4. **The whole take goes into the load.** Nothing is eaten out of it at the source: the home
    ///    band feeds its party through its ordinary consumption, and supplies ride the porters'
    ///    return leg (see the module doc).
    /// 5. **While the load holds a pack and a hunter is at the source, one hunter leaves with one
    ///    pack.** Departures therefore never exceed the hunters present. On a walk of no length the
    ///    pack lands this turn and nobody leaves at all.
    ///
    /// `pack_bulk` is one hunter's carry, seated by the web's own rule
    /// ([`crate::fauna::one_pack_biomass`] on the animal web, continuous on the plant web, the haul
    /// carry over the material's weight on the deposit web — [`material_pack`]). What one
    /// pack cannot hold stays in the load for the next porter — nobody walks away from it, so
    /// nothing a resident band would waste is wasted here. An unbounded carry takes the whole load.
    pub fn close_turn(&mut self, take: SourceTake, pack_bulk: f32) -> f32 {
        self.close_turn_classed(
            take,
            &CargoClasses::new(),
            &CarriedGoods::default(),
            pack_bulk,
        )
        .map_or(NOTHING_CARRIED, |pack| pack.cargo)
    }

    /// **[`Self::close_turn`] with the take's keeping classes** (#706) — the take site's form. The
    /// take's `classes` join the load's, and every pack that leaves carries its share of them. The
    /// pack landed without a walk (if any) comes back whole, classes and all, so the take site can
    /// credit it by class; its cargo is exactly what [`Self::close_turn`] returns.
    ///
    /// `goods` are the take's fodder and materials (#706): they join the load and leave in every
    /// pack in the same proportion as its bulk, so they land when the pack does.
    pub fn close_turn_classed(
        &mut self,
        take: SourceTake,
        classes: &CargoClasses,
        goods: &CarriedGoods,
        pack_bulk: f32,
    ) -> Option<LandedPack> {
        // A take worth no cargo at all (an inedible quarry, a fibre crop) still loads its bulk —
        // the porters walk it whatever it is worth.
        self.load_bulk += take.bulk.max(NOTHING_CARRIED);
        self.load_cargo += take.cargo.max(NOTHING_CARRIED);
        merge_classes(&mut self.load_classes, classes);
        self.load_goods.merge(goods);

        let mut delivered = NOTHING_CARRIED;
        let mut delivered_classes = CargoClasses::new();
        let mut delivered_bulk = NOTHING_CARRIED;
        let mut delivered_goods = CarriedGoods::default();
        let mut landed_now = false;
        let mut present = self.hunters_present();
        let loads_a_pack =
            |load: f32| load > NOTHING_CARRIED && (!pack_bulk.is_finite() || load >= pack_bulk);
        if pack_bulk > NOTHING_CARRIED {
            while present > NOBODY_ON_THE_ROAD && loads_a_pack(self.load_bulk) {
                let carried = self.load_bulk.min(pack_bulk);
                let share = carried / self.load_bulk;
                let cargo = self.load_cargo * share;
                let pack_classes = scaled_classes(&self.load_classes, share);
                let pack_goods = if share >= WHOLE_LOAD {
                    std::mem::take(&mut self.load_goods)
                } else {
                    self.load_goods.take_share(share)
                };
                self.load_bulk = (self.load_bulk - carried).max(NOTHING_CARRIED);
                self.load_cargo = (self.load_cargo - cargo).max(NOTHING_CARRIED);
                if self.load_cargo <= NOTHING_CARRIED {
                    self.load_classes.clear();
                } else {
                    for (class, amount) in &pack_classes {
                        if let Some(held) = self.load_classes.get_mut(class) {
                            *held = (*held - amount).max(NOTHING_CARRIED);
                        }
                    }
                }
                if self.load_bulk <= NOTHING_CARRIED {
                    self.load_goods = CarriedGoods::default();
                }
                if self.walk_turns == NO_WALK {
                    delivered += cargo;
                    merge_classes(&mut delivered_classes, &pack_classes);
                    delivered_bulk += carried;
                    delivered_goods.merge(&pack_goods);
                    landed_now = true;
                    continue;
                }
                self.on_the_road.push(Walker {
                    walk_turns: self.walk_turns,
                    turns_out: 0,
                    cargo,
                    classes: pack_classes,
                    bulk: carried,
                    goods: pack_goods,
                });
                present -= 1;
            }
        }
        landed_now.then_some(LandedPack {
            walk_turns: NO_WALK,
            cargo: delivered,
            classes: delivered_classes,
            bulk: delivered_bulk,
            goods: delivered_goods,
        })
    }

    /// **ONE WHOLE CARAVAN TURN around a take `take` prices at the hunters present** — exactly [`Self::open_turn`] → the take → [`Self::close_turn`], which is the
    /// turn's own sequence. Returns everything credited home this turn and whether a load landed.
    pub fn step(&mut self, pack_bulk: f32, take: impl FnOnce(u32) -> SourceTake) -> (f32, bool) {
        let open = self.open_turn();
        let taken = take(open.present);
        let landed_now = self.close_turn(taken, pack_bulk);
        let home = open.delivered + landed_now;
        (home, home > NOTHING_CARRIED)
    }

    /// ⛔ **THE PARTY, STOOD DOWN, WALKING HOME** — what an unassign, an abandon, a lapse, a shed
    /// and `cancel_order` all turn a posting into. Nothing arrives at once:
    ///
    /// - **a porter carrying a pack** finishes its walk (`walk_turns − turns_out` turns) and lands
    ///   it, rotting by the walk it set out on;
    /// - **a porter heading back out empty** turns round and walks back the way it has come
    ///   (`turns_out − walk_turns` turns);
    /// - **the hands at the source** carry the load home over the whole walk, and it rots by that
    ///   walk. **A load with no hand at the source (every porter on the road) is ABANDONED there** —
    ///   nobody carries it, so it never lands. It was never income nor in the larder, so no ledger
    ///   term (and no spoilage) answers for it;
    /// - **a party still walking out** turns round and walks back the turns it has covered, empty.
    ///
    /// Every hand is away from the band until its walk ends. A group already home (a porter that
    /// landed this very turn, a party posted and stood down before it took a step) is not listed:
    /// it is back now.
    pub fn walk_home(self, target: &crate::components::LaborTarget) -> Vec<HomewardWalk> {
        let mut walks = Vec::new();
        if self.is_walking_out() {
            walks.push(HomewardWalk::empty_handed(
                target,
                self.workers,
                self.walk_turns.saturating_sub(self.walk_out_remaining),
            ));
        } else {
            let at_the_source = self.hunters_present();
            for walker in self.on_the_road {
                walks.push(HomewardWalk::of_porter(target, walker));
            }
            // Cargo walks only with a carrier: no hand at the source, no source group, and the
            // leftover load — food, fodder and materials alike — stays where it is.
            if at_the_source > NOBODY_ON_THE_ROAD {
                walks.push(HomewardWalk {
                    target: target.clone(),
                    workers: at_the_source,
                    turns_left: self.walk_turns,
                    walk_turns: self.walk_turns,
                    cargo: self.load_cargo,
                    classes: self.load_classes,
                    goods: self.load_goods,
                });
            }
        }
        walks.retain(HomewardWalk::is_on_the_road);
        walks
    }

    /// ⛔ **A CREW CUT SHORT OF ZERO SENDS THE DROPPED HANDS WALKING HOME** — `workers → to`, `to`
    /// above zero (zero is a stand-down, [`Self::walk_home`]). Nobody is back in the band at once:
    ///
    /// - **the hands at the source go first**, each walking the whole walk home carrying nothing —
    ///   the load stays with those who remain;
    /// - past them, **porters on the road**, nearest home first: one carrying a pack finishes its
    ///   walk and lands it but does not walk back out; one heading back out empty turns round;
    /// - a party **still walking out** sends the dropped hands back the turns it has covered.
    ///
    /// Answers the walks home; the party keeps `to` hands. A crew raise is not a cut and returns
    /// nothing.
    pub fn cut_crew(
        &mut self,
        to: u32,
        target: &crate::components::LaborTarget,
    ) -> Vec<HomewardWalk> {
        if to >= self.workers {
            return Vec::new();
        }
        let dropped = self.workers - to;
        let mut walks = Vec::new();
        if self.is_walking_out() {
            walks.push(HomewardWalk::empty_handed(
                target,
                dropped,
                self.walk_turns.saturating_sub(self.walk_out_remaining),
            ));
        } else {
            let from_the_source = dropped.min(self.hunters_present());
            walks.push(HomewardWalk::empty_handed(
                target,
                from_the_source,
                self.walk_turns,
            ));
            let mut from_the_road = (dropped - from_the_source) as usize;
            // Nearest home first: a pack about to land, or a porter just out of the door.
            let mut order: Vec<usize> = (0..self.on_the_road.len()).collect();
            order.sort_by_key(|&i| HomewardWalk::turns_home_of(&self.on_the_road[i]));
            let mut leaving: Vec<usize> = order.into_iter().take(from_the_road).collect();
            from_the_road -= leaving.len();
            debug_assert_eq!(from_the_road, 0, "a cut never exceeds the party");
            leaving.sort_unstable_by(|a, b| b.cmp(a));
            for i in leaving {
                let walker = self.on_the_road.remove(i);
                walks.push(HomewardWalk::of_porter(target, walker));
            }
        }
        self.workers = to;
        walks.retain(HomewardWalk::is_on_the_road);
        walks
    }
}

/// One porter — a [`Walker`] is one hand.
const ONE_PORTER: u32 = 1;

/// ⛔ **ONE GROUP OF A STOOD-DOWN PARTY, WALKING HOME** ([`WorkParty::walk_home`]) — on the band's
/// `LaborAllocation::homeward`, because the row that posted it is gone (or holds no party) and the
/// walk is not over. Its hands are away from the band's pool until it lands; its cargo lands where
/// `target`'s cargo always does, and rots by `walk_turns` exactly as a live pack does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomewardWalk {
    /// **The source it was working** — what decides where its cargo lands (the larder, or a
    /// deposit's material store at the ground's characteristics), read when it arrives.
    pub target: crate::components::LaborTarget,
    /// The hands walking in this group — away from the band until it arrives.
    pub workers: u32,
    /// **Turns of walking still to go.** It lands on the turn this reaches zero.
    pub turns_left: u32,
    /// **The walk its cargo is carried over**, in turns — what the transit rot reads. A porter's own
    /// walk; the whole walk for the load the source hands carry.
    pub walk_turns: u32,
    /// The cargo it carries — food, or a deposit's material units.
    pub cargo: f32,
    /// What that cargo is made of, by keeping class.
    pub classes: CargoClasses,
    /// **Its fodder and materials** (#706) — landed with it, never rotted.
    pub goods: CarriedGoods,
}

impl HomewardWalk {
    /// **Hands walking home with nothing** over `turns_left` turns.
    fn empty_handed(
        target: &crate::components::LaborTarget,
        workers: u32,
        turns_left: u32,
    ) -> Self {
        Self {
            target: target.clone(),
            workers,
            turns_left,
            walk_turns: NO_WALK,
            cargo: NOTHING_CARRIED,
            classes: CargoClasses::new(),
            goods: CarriedGoods::default(),
        }
    }

    /// **The turns a porter is from home** — the rest of its walk with a pack, or the way back it
    /// has come heading out empty.
    fn turns_home_of(walker: &Walker) -> u32 {
        if walker.carrying() {
            walker.walk_turns - walker.turns_out
        } else {
            walker.turns_out - walker.walk_turns
        }
    }

    /// **One porter, sent home** — with its pack it finishes its walk and lands it, rotting by the
    /// walk it set out on; empty it turns round.
    fn of_porter(target: &crate::components::LaborTarget, walker: Walker) -> Self {
        Self {
            target: target.clone(),
            workers: ONE_PORTER,
            turns_left: Self::turns_home_of(&walker),
            walk_turns: walker.walk_turns,
            cargo: walker.cargo,
            classes: walker.classes,
            goods: walker.goods,
        }
    }

    /// **Still has a walk or a load** — a group already home with nothing to land is not listed.
    fn is_on_the_road(&self) -> bool {
        self.workers > NOBODY_ON_THE_ROAD
            && (self.cargo > NOTHING_CARRIED || !self.goods.is_empty() || self.turns_left > NO_WALK)
    }

    /// **One turn of the walk.** Answers whether the group is home now.
    pub fn step(&mut self) -> bool {
        self.turns_left = self.turns_left.saturating_sub(1);
        self.turns_left == NO_WALK
    }

    /// **Is its cargo food?** A deposit's material goes to the store; everything else to the larder.
    pub fn carries_food(&self) -> bool {
        !matches!(self.target, crate::components::LaborTarget::Extract { .. })
    }

    /// **The food it carries that will rot before it lands** — every class whose shelf life is
    /// shorter than its walk ([`crate::spoilage::rots_in_transit`]), the rule its landing applies.
    /// A pack naming no class reads as its web's fallback class, as the larder lands it.
    pub fn food_that_rots(&self, keeping: &crate::demographics_config::KeepingConfig) -> f32 {
        if !self.carries_food() {
            return NOTHING_CARRIED;
        }
        let rots = |class: &str| crate::spoilage::rots_in_transit(class, self.walk_turns, keeping);
        if self.classes.is_empty() {
            let fallback = match self.target {
                crate::components::LaborTarget::Hunt { .. } => &keeping.kill_fallback_class,
                _ => &keeping.plant_fallback_class,
            };
            return if rots(fallback) {
                self.cargo
            } else {
                NOTHING_CARRIED
            };
        }
        self.classes
            .iter()
            .filter(|(class, _)| rots(class))
            .map(|(_, amount)| amount)
            .sum()
    }

    /// The pack it lands as — its cargo and classes, carried over its walk.
    pub fn landed_pack(&self) -> LandedPack {
        LandedPack {
            walk_turns: self.walk_turns,
            cargo: self.cargo,
            classes: self.classes.clone(),
            bulk: NOTHING_CARRIED,
            goods: self.goods.clone(),
        }
    }
}

/// ⛔ **THE DISTANCE PAST WHICH A ROW POSTS A PARTY — `band_work_range`, THE SAME FOR EVERY JOB.**
///
/// It is the band's apron: the distance its own hands reach without anybody walking goods. Hunt does
/// not get its own, longer threshold — a party that follows its herd never roams out of range.
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
    /// **One porter's haul at this coverage, in biomass** — the hunt pack's carry and the hunt
    /// projection's crew term, and the carry a deposit's pack is struck from ([`material_pack`]):
    /// the bare `labor.hunt.per_worker_biomass_capacity`, or the sled's `hunt_carry` where the kit
    /// carries one. One carry, whatever is on the porter's back.
    pub haul_carry: f32,
    /// One gatherer's basket at this coverage — the gather pack and the projection's crew term.
    pub forage_carry: f32,
}

impl CaravanPricing {
    /// Price a caravan carrying `kit` at the row's `priority`, against `wear`, beside `other_rows`.
    ///
    /// ⛔ **THE KIT IS SPREAD OVER `take_hands`**, the staffed crew less its planned keeping
    /// (`docs/plan_site_crews.md` §2.3): a take kit is carried by the hands taking. `claim` is the
    /// row's take-kit claim ([`crate::take_claims::take_kit_claim`]), never its head count.
    #[allow(clippy::too_many_arguments)] // the ration's own inputs: roster, kit, hands, claim, rank, rows
    pub fn resolve(
        equipment: &crate::equipment_config::EquipmentConfig,
        kit: &crate::equipment_config::KitChoice,
        take_hands: f32,
        claim: f32,
        priority: crate::components::SourcePriority,
        wear: &crate::components::BandEquipment,
        other_rows: &[crate::equipment_config::KittedRow],
        labor: &LaborConfig,
    ) -> Self {
        let budget = crate::equipment_config::BandItemBudget::with_prospective_row(
            other_rows.iter().cloned(),
            kit,
            claim,
            priority,
        );
        let coverage = equipment.coverage_from_units(
            kit,
            take_hands,
            wear,
            budget.share_for_prospective(wear, equipment),
        );
        let haul_carry = coverage.weighted_rate(|kit| {
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
            haul_carry,
            forage_carry,
        }
    }

    /// **What one worker's tools add to a deposit take, crew-weighted at this coverage** — the
    /// `deposit_take` each tool grants on the rungs it names, averaged over the staffed crew exactly
    /// as the carries are, so a caravan forecast adds `present × this` to the bare cut each turn.
    pub fn deposit_gear_per_worker(
        &self,
        equipment: &crate::equipment_config::EquipmentConfig,
        wear: &crate::components::BandEquipment,
        branch: crate::intensification::RungBranch,
        rung: &str,
    ) -> f32 {
        self.coverage
            .weighted_rate(|kit| equipment.deposit_take_per_worker(kit, wear, branch, Some(rung)))
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
/// what it takes this turn would promise a larder food that is still walking home. The
/// row reads the caravan instead: `realized` is its rate home and the arrival schedule is what it
/// lands turn by turn. **`row.actual` is the caller's** — the turn's real credit, or the seed's
/// first projected turn — and the published split `meat + standing == actual` is kept by scaling the
/// two parts onto it in their own proportion.
///
/// **Nothing a hunting party brings down is wasted** (`keeps_the_carcass`): what one pack cannot
/// seat stays in the load for the next porter. A gather's `wasted` is a different fact — stock the
/// crew could not reach — and stands.
///
/// **Food rows only.** Every field written here is a food projection, so a caravan carrying a
/// material (a far working) writes none of them: its rate home rides [`WorkParty::net_rate_home`]
/// alone, in the material's own units.
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
    /// **Cargo arriving home AND KEEPING per turn** (food, or material units on the deposit web),
    /// averaged over the turns stepped — the row's `netRateHome`. Net of transit rot (#706).
    pub rate_home: f32,
    /// **Cargo lost on the walk home per turn**, averaged over the same turns — the row's
    /// `spoiledRateHome`.
    pub spoiled_rate_home: f32,
    /// [`TransitRot::keeps_turns`] at the party's walk — the row's `transitKeepsTurns`.
    pub transit_keeps_turns: f32,
    /// What lands home and keeps on each stepped turn, `[0]` being next turn.
    pub home_by_turn: Vec<f32>,
    /// The average number of hunters on the road across the turns stepped.
    pub mean_on_the_road: f32,
    /// **The bulk arriving home per turn**, averaged over the turns stepped — what every account a
    /// forage pack carries besides food (fodder, materials) arrives at, through the basket's own
    /// per-biomass rates. Not struck for rot: fodder and materials keep.
    pub bulk_rate_home: f32,
    /// **A far forage row's fodder per turn arriving home** (#706) — [`Self::bulk_rate_home`] through
    /// the basket's fodder rate, behind the credit's own Foddering gate. Filled by
    /// [`forecast_forage_caravan`]; `0` on every other web.
    pub fodder_rate_home: f32,
    /// **Its materials per turn arriving home**, one row per material id — filled by
    /// [`forecast_forage_caravan`] and [`forecast_hunt_caravan`]; empty on the deposit web.
    pub materials_rate_home: Vec<crate::materials_config::MaterialPayoff>,
    /// **The 1-based turn the first pack lands** — whatever its cargo is worth, so a basket that
    /// carries no food still reports when its first load walks in — or [`NO_LOAD_WITHIN_HORIZON`].
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
///
/// ⛔ **EVERY LANDING PACK IS STRUCK FOR TRANSIT ROT BY THE TURN'S OWN RULE** (#706): a walked pack
/// loses [`TransitRot::rotten_share`] of its cargo at the walk it set out on, so `rate_home` and
/// `home_by_turn` are what arrives AND keeps — the larder's real gain — and what was lost rides
/// `spoiled_rate_home`. A pack landed without a walk never rots. `rot` is `None` for a material.
pub fn forecast_caravan(
    start: &WorkParty,
    horizon: u32,
    pack_bulk: f32,
    rot: Option<&TransitRot<'_>>,
    mut take: impl FnMut(u32) -> Option<SourceTake>,
) -> CaravanForecast {
    forecast_caravan_carrying(start, horizon, pack_bulk, rot, |present| {
        take(present).map(|taken| (taken, CarriedGoods::default()))
    })
}

/// **[`forecast_caravan`] with the by-products each projected take packs** (#706) — they ride the
/// load and every pack exactly as the turn's do, and what LANDS is averaged into
/// [`CaravanForecast::fodder_rate_home`] / [`CaravanForecast::materials_rate_home`]. The hunt web
/// steps it, because a pen's standing fleece is not proportional to the carcass bulk a per-biomass
/// rate could convert.
pub fn forecast_caravan_carrying(
    start: &WorkParty,
    horizon: u32,
    pack_bulk: f32,
    rot: Option<&TransitRot<'_>>,
    mut take: impl FnMut(u32) -> Option<(SourceTake, CarriedGoods)>,
) -> CaravanForecast {
    let mut party = start.clone();
    let mut forecast = CaravanForecast::default();
    let mut on_the_road = 0u32;
    let mut spoiled = NOTHING_CARRIED;
    let mut bulk_home = NOTHING_CARRIED;
    let mut goods_home = CarriedGoods::default();
    for turn in 1..=horizon {
        let mut spent = false;
        let open = party.open_turn();
        let (taken, goods) = take(open.present).unwrap_or_else(|| {
            spent = true;
            (SourceTake::default(), CarriedGoods::default())
        });
        let now = party.close_turn_classed(taken, &CargoClasses::new(), &goods, pack_bulk);
        let landed_now = now.as_ref().map_or(NOTHING_CARRIED, |pack| pack.cargo);
        bulk_home += open.packs.iter().map(|pack| pack.bulk).sum::<f32>()
            + now.as_ref().map_or(NOTHING_CARRIED, |pack| pack.bulk);
        for pack in open.packs.iter().chain(now.iter()) {
            goods_home.merge(&pack.goods);
        }
        let lost: f32 = rot.map_or(NOTHING_CARRIED, |rot| {
            open.packs
                .iter()
                .map(|pack| pack.cargo * rot.rotten_share(pack.walk_turns))
                .sum()
        });
        let home = open.delivered + landed_now;
        // ⛔ **A LOAD LANDS WHEN A PACK LANDS, WHATEVER IT IS WORTH AS CARGO.** Packs fill by
        // BULK — everything the crew cut — so a basket of hay, fibre or tobacco fills and walks
        // packs exactly as a food basket does, but its `cargo` (food) is zero. Reading the landing
        // off `home > 0` published no first load at all for such a basket.
        let load_landed = !open.packs.is_empty() || now.is_some();
        spoiled += lost;
        forecast.home_by_turn.push(home - lost);
        on_the_road += party.hunters_on_the_road();
        if load_landed && forecast.first_load_turn == NO_LOAD_WITHIN_HORIZON {
            forecast.first_load_turn = turn;
        }
        if spent && party.on_the_road.is_empty() && party.load_bulk <= NOTHING_CARRIED {
            break;
        }
    }
    let turns = forecast.home_by_turn.len();
    if turns > 0 {
        forecast.rate_home = forecast.home_by_turn.iter().sum::<f32>() / turns as f32;
        forecast.spoiled_rate_home = spoiled / turns as f32;
        forecast.mean_on_the_road = on_the_road as f32 / turns as f32;
        forecast.bulk_rate_home = bulk_home / turns as f32;
        forecast.fodder_rate_home = goods_home.fodder / turns as f32;
        forecast.materials_rate_home =
            crate::materials_config::merge_material_payoffs(goods_home.materials.iter().map(
                |carried| crate::materials_config::MaterialPayoff {
                    material: carried.material.clone(),
                    amount: carried.amount / turns as f32,
                },
            ));
    }
    forecast.transit_keeps_turns = rot.map_or(NOTHING_ROTS_ON_THE_WALK, |rot| {
        rot.keeps_turns(start.walk_turns)
    });
    forecast
}

/// **A HUNT CARAVAN'S FORECAST** — [`forecast_caravan`] over [`HuntProjection`], the step
/// `fauna::project_realized_hunt` loops over, so the caravan forecast runs the hunt's own projected
/// take at whatever crew is present each turn.
///
/// The pack is one hunter's carry at this herd's rung ([`crate::fauna::herd_carry_rate`]) seated in
/// whole animals ([`crate::fauna::one_pack_biomass`]).
#[allow(clippy::too_many_arguments)] // the take's full context, plus the caravan's own term
pub fn forecast_hunt_caravan(
    party: &WorkParty,
    herd: &crate::fauna::Herd,
    fauna: &FaunaConfig,
    carry_per_worker: f32,
    hunters: &HuntingParty,
    output_multiplier: f32,
    floor: f32,
    // **What the row's crew spends keeping the herd** — netted off the hands present every turn
    // ([`take_hands_present`]).
    keep_hands: f32,
    horizon: u32,
    // **How food keeps** (#706) — a herd's take is its species' one class.
    keeping: &crate::demographics_config::KeepingConfig,
    // **What a material weighs** — the bulk a pen's fleece fills a pack with ([`standing_stream`]).
    materials: &crate::materials_config::MaterialsConfig,
) -> CaravanForecast {
    let pack = hunt_pack_biomass(herd, fauna, carry_per_worker);
    let mut projection = HuntProjection::new(herd, fauna);
    let class = fauna
        .keeping_for(&herd.species)
        .unwrap_or(&keeping.kill_fallback_class);
    let rot = TransitRot::single(keeping, class);
    // ⛔ **WHAT THE CARCASS AND THE STANDING HERD SEND HOME, EVERY STEPPED TURN** (#706). The cull's
    // meat and the standing food (milk, eggs — `HuntProjection::step` counts both in the turn's
    // provisions, so `rate_home` carries the milk net of the walk's rot, by the herd's one keeping
    // class, as the turn lands it); the carcass's hide, bone and sinew through the species'
    // per-biomass rows off the carcass bulk; and a kept herd's per-head fleece and the BULK its milk
    // and fleece fill packs by ([`standing_stream`]) — without that bulk a pen that culls nothing
    // never fills a pack and its standing yield never leaves. A wild herd's stream is empty. A hunt
    // yields no fodder; materials never rot.
    let carcass_rows = fauna.hunt_materials_for(&herd.species);
    let mut forecast = forecast_caravan_carrying(party, horizon, pack, Some(&rot), |present| {
        let culled = projection.step(
            fauna,
            carry_per_worker,
            hunters,
            output_multiplier,
            take_hands_present(present, keep_hands),
            floor,
            // A party's load waits at the source for the next porter — it keeps every carcass.
            crate::fauna::CarcassKept::Whole,
        );
        let (food, carcass) = match culled {
            Some(turn) => (turn.yields.provisions, turn.biomass),
            // The projection ends only when there is nothing to cull AND no standing yield.
            None => return None,
        };
        // At the PROJECTED head count, as the standing food in the step is.
        let standing = standing_stream(projection.herd(), fauna, materials, output_multiplier);
        let mut goods = CarriedGoods {
            fodder: NOTHING_CARRIED,
            materials: carcass_rows
                .iter()
                .map(|row| CarriedMaterial {
                    material: row.material.clone(),
                    band: crate::materials_config::BandKey::default(),
                    characteristics: Default::default(),
                    amount: carcass * row.per_biomass * output_multiplier,
                })
                .collect(),
        };
        goods.merge(&standing.goods);
        Some((
            SourceTake {
                // `HuntProjection::step` already counts the standing food (milk, eggs) in the turn's
                // provisions, at the projected head count — only its bulk and fleece are added here.
                cargo: food,
                bulk: carcass + standing.bulk,
            },
            goods,
        ))
    });
    forecast
        .materials_rate_home
        .retain(|payoff| payoff.amount > NOTHING_CARRIED);
    forecast
}

/// **A KEPT HERD'S STANDING YIELD, AS A CARAVAN STREAM** (#706) — what it pays per turn for standing
/// there (`fauna::herd_standing_provisions`, `FaunaConfig::standing_materials_for` at
/// `fauna::herd_standing_scale`, times the band's output multiplier — the arithmetic the take site
/// credits by), and the **bulk** it fills a pack with: the milk's biomass-equivalent at the herd's
/// own meat rate, and each fleece at its material's `weight` (biomass-equivalent mass per unit). The
/// turn loads the same bulk, so a pen that culls nothing still fills packs and sends its milk and
/// fleece home. Empty for a wild herd.
pub struct StandingStream {
    pub provisions: f32,
    pub bulk: f32,
    pub goods: CarriedGoods,
}

impl StandingStream {
    /// Pays nothing.
    pub fn is_empty(&self) -> bool {
        self.provisions <= NOTHING_CARRIED && self.goods.is_empty()
    }
}

/// See [`StandingStream`].
pub fn standing_stream(
    herd: &crate::fauna::Herd,
    fauna: &FaunaConfig,
    materials: &crate::materials_config::MaterialsConfig,
    output_multiplier: f32,
) -> StandingStream {
    let scale = crate::fauna::herd_standing_scale(herd, fauna);
    let provisions = crate::fauna::herd_standing_provisions(herd, fauna) * output_multiplier;
    let per_biomass = crate::fauna::herd_hunt_yield(herd, fauna).provisions_per_biomass;
    let food_bulk = if per_biomass > NOTHING_CARRIED {
        provisions / per_biomass
    } else {
        NOTHING_CARRIED
    };
    let goods = CarriedGoods {
        fodder: NOTHING_CARRIED,
        materials: fauna
            .standing_materials_for(&herd.species)
            .iter()
            .map(|row| CarriedMaterial {
                material: row.material.clone(),
                band: crate::materials_config::BandKey::default(),
                characteristics: Default::default(),
                amount: scale * row.per_biomass * output_multiplier,
            })
            .filter(|carried| carried.amount > NOTHING_CARRIED)
            .collect(),
    };
    let material_bulk: f32 = goods
        .materials
        .iter()
        .map(|carried| {
            carried.amount
                * materials
                    .material(&carried.material)
                    .map_or(NOTHING_CARRIED, |def| def.weight)
        })
        .sum();
    StandingStream {
        provisions,
        bulk: food_bulk + material_bulk,
        goods,
    }
}

/// ⛔ **ONE PORTER'S PACK OF A MATERIAL, IN THE MATERIAL'S OWN UNITS** — the haul carry
/// ([`CaravanPricing::haul_carry`], biomass) over the material's `weight` (biomass-equivalent mass per
/// unit, `materials.json`).
///
/// **The same carry the hunt uses**, so a sled that drags 40 of meat drags `40 / weight` of timber,
/// and the whole of what makes stone travel differently from wood is one number on the material.
/// There is no per-material branch here and there must never be one. `weight` is validated positive
/// and finite at load.
pub fn material_pack(haul_carry: f32, weight: f32) -> f32 {
    haul_carry / weight
}

/// **A DEPOSIT CARAVAN'S FORECAST** — [`forecast_caravan`] over
/// [`crate::extraction::DepositProjection`], the step the assign-time seed's first turn and the
/// local row's steady rate read too, so the three quote one take. Cargo and bulk are both the
/// material's own units, and the pack is [`material_pack`].
///
/// A crew of nobody cuts nothing and the working goes on renewing, so a turn the party is walking
/// out or wholly on the road reads as a zero take, never as the end of the run — only a working
/// that will never renew and has nothing left to reach is spent.
///
/// **The hands present bring `gear_per_worker × present` of tools and `pack × present` of carry**
/// ([`crate::extraction::CrewLift`]) — the crew-weighted rates times the hands at the deposit, the
/// carries' own shape. One pack is one porter's carry, and it is also the most one hand can cut and
/// carry off in a turn, as a hunter's haul bounds a kill.
#[allow(clippy::too_many_arguments)] // the take's full context, plus the caravan's own terms
pub fn forecast_extract_caravan(
    party: &WorkParty,
    working: &crate::extraction::DepositSource,
    ground: &crate::components::Tile,
    extraction: &crate::extraction_config::ExtractionConfig,
    ladder: &crate::intensification::LadderConfig,
    pack: f32,
    gear_per_worker: f32,
    floor: f32,
    // **What the row's crew spends keeping the working** — the cutters are the hands present less
    // it ([`take_hands_present`]), and they alone bring tools and carry.
    keep_hands: f32,
    horizon: u32,
) -> CaravanForecast {
    let mut projection = crate::extraction::DepositProjection::new(working);
    forecast_caravan(party, horizon, pack, NO_TRANSIT_ROT, |present| {
        let cutters = take_hands_present(present, keep_hands);
        projection
            .step(
                cutters,
                crate::extraction::CrewLift {
                    tools: gear_per_worker * cutters,
                    carry: pack * cutters,
                },
                floor,
                ground,
                extraction,
                ladder,
            )
            .map(|taken| SourceTake {
                cargo: taken,
                bulk: taken,
            })
    })
}

/// ⛔ **THE HANDS A PRESENT PARTY TAKES WITH, AFTER KEEPING** (`docs/plan_site_crews.md` §2.1) —
/// the hands standing at the source keep it first, up to `keep_hands` (what the row's whole crew
/// spends keeping), and take with the rest. The forecast twin of the turn's
/// `SiteKeeping::at_the_source`: a hand on the road keeps nothing and takes nothing, so every turn
/// of the caravan nets the keeping from whoever is present.
pub fn take_hands_present(present: u32, keep_hands: f32) -> f32 {
    let present = present as f32;
    present - keep_hands.clamp(crate::fauna::NO_HANDS, present)
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
/// at no crew every turn the party is walking out, wholly on the road or wholly keeping, so that
/// case is read as a zero take on a stand that goes on regrowing, never as the end of the run.
#[allow(clippy::too_many_arguments)] // the gather's full context, plus the caravan's own term
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
    // **What the row's crew spends keeping the patch** — netted off the hands present every turn
    // ([`take_hands_present`]).
    keep_hands: f32,
    horizon: u32,
    // **How food keeps** (#706) — the basket's classes, decomposed as the take site credits them.
    keeping: &crate::demographics_config::KeepingConfig,
    // **Does this faction bank the basket's fodder?** — the credit site's own gate (a fodder-crop
    // commitment, or Foddering), so the fodder rate home is what would actually be credited.
    fodder_credited: bool,
) -> CaravanForecast {
    let mut projection = ForageProjection::new(patch);
    let one_unit = crate::scalar::scalar_one();
    let rot = TransitRot {
        keeping,
        shares: crate::forage::patch_food_mix(
            patch,
            tile_composition,
            flora,
            forage,
            take_species,
            one_unit,
            keeping,
        )
        .iter()
        .map(|(class, share)| (class.to_string(), share.to_f32()))
        .collect(),
    };
    let mut forecast = forecast_caravan(party, horizon, carry_per_worker, Some(&rot), |present| {
        let gatherers = take_hands_present(present, keep_hands);
        match projection.step(
            tile_composition,
            forage,
            flora,
            carry_per_worker,
            seasonal,
            output_multiplier,
            gatherers,
            floor,
            take_species,
        ) {
            Some(turn) => Some(SourceTake {
                cargo: turn.provisions,
                bulk: turn.biomass,
            }),
            // Nobody gathering — walking out, all on the road, or every hand present keeping — is
            // a zero take on a stand that goes on regrowing, never the end of the run.
            None if gatherers <= crate::fauna::NO_HANDS => Some(SourceTake::default()),
            None => None,
        }
    });
    // ⛔ **THE BASKET'S OTHER ACCOUNTS ARRIVE WITH ITS PACKS** (#706) — the bulk landing per turn
    // through the basket's own per-biomass rates, the arithmetic the take site packs them by. Never
    // struck for rot: fodder and materials keep.
    if fodder_credited {
        forecast.fodder_rate_home = crate::forage::tended_take_fodder(
            forecast.bulk_rate_home,
            patch,
            tile_composition,
            flora,
            forage,
            output_multiplier,
            take_species,
        );
    }
    forecast.materials_rate_home = crate::materials_config::merge_material_payoffs(
        crate::forage::patch_material_yields_taking(
            patch,
            tile_composition,
            flora,
            forage,
            take_species,
        )
        .into_iter()
        .map(|row| crate::materials_config::MaterialPayoff {
            amount: forecast.bulk_rate_home * row.per_biomass * output_multiplier,
            material: row.material,
        }),
    );
    forecast
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A crew-linear take: `present × rate` biomass, one food per biomass. The regime the
    /// steady-state formula is exact in, with `r` the whole per-hunter take.
    fn crew_linear(rate: f32) -> impl FnMut(u32) -> Option<SourceTake> {
        move |present| {
            let biomass = present as f32 * rate;
            Some(SourceTake {
                cargo: biomass,
                bulk: biomass,
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
            party.close_turn(taken, pack);
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
            forecast_caravan(&party, 60, 12.0, NO_TRANSIT_ROT, crew_linear(0.5)).first_load_turn
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
                cargo: 100.0,
                bulk: 100.0,
            },
            5.0,
        );
        assert_eq!(
            party.hunters_on_the_road(),
            3,
            "every hunter leaves with a pack"
        );
        assert_eq!(party.hunters_present(), 0);
        assert!(
            (party.load_bulk - 85.0).abs() < 1e-3,
            "the packs nobody could carry stay in the load: {}",
            party.load_bulk
        );
    }

    /// ⛔ **A posting that ends WALKS home** — every porter finishes its walk with its pack, the hands
    /// at the source carry the load over the whole walk, and nothing is handed over at once.
    #[test]
    fn a_stood_down_party_walks_every_pack_and_the_load_home() {
        let mut party = WorkParty::posted(UVec2::ZERO, 6, 6);
        party.workers = 4;
        party.walk_out_remaining = NO_WALK;
        party.open_turn();
        party.close_turn(
            SourceTake {
                cargo: 25.0,
                bulk: 25.0,
            },
            10.0,
        );
        party.open_turn();
        assert_eq!(
            party.hunters_on_the_road(),
            2,
            "fixture: two packs on the road, one step out"
        );
        let target = crate::components::LaborTarget::Hunt {
            fauna_id: "boar".to_string(),
            floor: 0.0,
        };
        let walks = party.walk_home(&target);
        let porters: Vec<_> = walks.iter().filter(|w| w.workers == ONE_PORTER).collect();
        assert_eq!(porters.len(), 2);
        assert!(porters
            .iter()
            .all(|w| w.turns_left == 5 && w.walk_turns == 6 && w.cargo == 10.0));
        let source = walks
            .iter()
            .find(|w| w.workers == 2)
            .expect("the two hands at the source walk the load home");
        assert_eq!((source.turns_left, source.walk_turns), (6, 6));
        assert!((source.cargo - 5.0).abs() < 1e-4);
        let carried: f32 = walks.iter().map(|w| w.cargo).sum();
        assert!(
            (carried - 25.0).abs() < 1e-4,
            "nothing on the posting is lost"
        );
    }

    /// ⛔ **A load nobody is at the source to carry is LEFT BEHIND** — no group walks without a
    /// hand, so only the porters' packs go home.
    #[test]
    fn a_load_with_no_hand_at_the_source_is_left_behind() {
        let mut party = WorkParty::posted(UVec2::ZERO, 6, 6);
        party.workers = 2;
        party.walk_out_remaining = NO_WALK;
        party.open_turn();
        party.close_turn(
            SourceTake {
                cargo: 25.0,
                bulk: 25.0,
            },
            10.0,
        );
        assert_eq!(
            party.hunters_present(),
            0,
            "fixture: every hand is on the road"
        );
        assert!(
            party.load_cargo > NOTHING_CARRIED,
            "fixture: a leftover load"
        );
        let walks = party.walk_home(&crate::components::LaborTarget::Scout);
        assert!(walks.iter().all(|w| w.workers == ONE_PORTER));
        let carried: f32 = walks.iter().map(|w| w.cargo).sum();
        assert!(
            (carried - 20.0).abs() < 1e-4,
            "only the two packs walk home: {carried}"
        );
    }

    /// ⛔ **A cut deeper than the hands at the source takes porters, nearest home first** — a
    /// carrying porter finishes its walk with its pack (and lands it by its walk), an empty one turns
    /// round; the hands at the source go first and walk the whole walk with nothing.
    #[test]
    fn a_deep_cut_turns_porters_round_nearest_home_first() {
        let target = crate::components::LaborTarget::Scout;
        let porter = |turns_out: u32, cargo: f32| Walker {
            walk_turns: 6,
            turns_out,
            cargo,
            bulk: cargo,
            ..Walker::default()
        };
        let party_with = |road: Vec<Walker>| {
            let mut party = WorkParty::posted(UVec2::ZERO, 6, 6);
            party.walk_out_remaining = NO_WALK;
            party.workers = 5;
            party.on_the_road = road;
            party.load_cargo = 5.0;
            party
        };
        // Three at the source; an empty porter two turns from home; a carrying one four out.
        let mut party = party_with(vec![porter(2, 10.0), porter(8, 0.0)]);
        let walks = party.cut_crew(1, &target);
        let source = walks
            .iter()
            .find(|w| w.workers == 3)
            .expect("the source hands go first");
        assert_eq!((source.turns_left, source.cargo), (6, NOTHING_CARRIED));
        let turned = walks
            .iter()
            .find(|w| w.workers == ONE_PORTER)
            .expect("one porter too");
        assert_eq!(
            (turned.turns_left, turned.cargo),
            (2, NOTHING_CARRIED),
            "the empty one turns round"
        );
        assert_eq!(party.workers, 1);
        assert_eq!(
            party.hunters_on_the_road(),
            1,
            "the carrying porter stays on"
        );
        assert_eq!(
            party.load_cargo, 5.0,
            "the load stays with those who remain"
        );

        // Now the carrying porter is the nearer: it finishes its walk home with its pack.
        let mut party = party_with(vec![porter(5, 10.0), porter(9, 0.0)]);
        let walks = party.cut_crew(1, &target);
        let carried = walks
            .iter()
            .find(|w| w.workers == ONE_PORTER)
            .expect("a porter");
        assert_eq!(
            (carried.turns_left, carried.walk_turns, carried.cargo),
            (1, 6, 10.0),
            "it lands its pack on its own walk, and does not walk back out"
        );
        assert_eq!(party.on_the_road.len(), 1);
        assert!(
            party.on_the_road[0].turns_out == 9,
            "the empty porter stays on"
        );
    }

    /// A load / walker split carries the fodder and materials in the bulk's proportion, and a pack
    /// that lands hands them over.
    #[test]
    fn a_pack_carries_its_share_of_the_loads_goods() {
        let mut party = WorkParty::posted(UVec2::ZERO, 6, 6);
        party.workers = 2;
        party.walk_out_remaining = NO_WALK;
        party.open_turn();
        let goods = CarriedGoods {
            fodder: 4.0,
            materials: vec![CarriedMaterial {
                material: "fibre".to_string(),
                band: crate::materials_config::BandKey::default(),
                characteristics: Default::default(),
                amount: 2.0,
            }],
        };
        party.close_turn_classed(
            SourceTake {
                cargo: NOTHING_CARRIED,
                bulk: 20.0,
            },
            &CargoClasses::new(),
            &goods,
            10.0,
        );
        assert_eq!(party.hunters_on_the_road(), 2);
        for walker in &party.on_the_road {
            assert!((walker.goods.fodder - 2.0).abs() < 1e-5);
            assert!((walker.goods.materials[0].amount - 1.0).abs() < 1e-5);
        }
        assert!(party.load_goods.is_empty(), "the whole load left");
        let mut landed = Vec::new();
        for _ in 0..6 {
            landed.extend(party.open_turn().packs);
        }
        let fodder: f32 = landed.iter().map(|pack| pack.goods.fodder).sum();
        assert!((fodder - 4.0).abs() < 1e-5, "both packs land their hay");
    }

    /// A party still walking out turns round and walks back what it covered, carrying nothing.
    #[test]
    fn a_party_walking_out_walks_back_what_it_covered() {
        let mut party = WorkParty::posted(UVec2::ZERO, 6, 6);
        party.workers = 3;
        party.open_turn();
        party.open_turn();
        let walks = party.walk_home(&crate::components::LaborTarget::Scout);
        assert_eq!(walks.len(), 1);
        assert_eq!((walks[0].workers, walks[0].turns_left), (3, 2));
        assert_eq!(walks[0].cargo, NOTHING_CARRIED);
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
                cargo: 30.0,
                bulk: 30.0,
            },
            10.0,
        );
        assert_eq!(close, 30.0);
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
                cargo: body,
                bulk: body,
            },
            pack,
        );
        assert_eq!(
            party.hunters_on_the_road(),
            7,
            "seven packs of forty off three hundred"
        );
        let mut home = 0.0_f32;
        for _ in 0..10 {
            home += party.step(pack, |_| SourceTake::default()).0;
        }
        assert!(
            (home + party.load_cargo - body).abs() < 1e-2,
            "every part of the carcass is home or still in the load: {home} + {}",
            party.load_cargo
        );
        assert!(
            party.load_cargo > 0.0,
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
                cargo: 5.0,
                bulk: 5.0,
            },
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
            party.close_turn(SourceTake::default(), 5.0);
        }
        assert_eq!(landed_on, Some(walk));
        assert_eq!(
            back_on,
            Some(2 * walk + 1),
            "absent for the whole round trip"
        );
    }

    /// **A pack carries what its load was made of, and lands with the walk it came on** (#706) —
    /// the two facts the transit rot reads. A load of flesh and grain splits into packs in its own
    /// proportions, each pack is handed over with its porter's walk, and the classes of every pack
    /// home sum to what was loaded.
    #[test]
    fn a_pack_carries_its_loads_classes_home_with_its_walk() {
        const WALK: u32 = 6;
        const PACK: f32 = 10.0;
        const LOAD: f32 = 20.0;
        const FLESH_SHARE: f32 = 0.75;
        let mut party = WorkParty::posted(UVec2::ZERO, WALK, WALK);
        party.workers = 4;
        party.walk_out_remaining = NO_WALK;
        party.open_turn();
        let classes: CargoClasses = [
            ("flesh".to_string(), LOAD * FLESH_SHARE),
            ("dry".to_string(), LOAD * (1.0 - FLESH_SHARE)),
        ]
        .into_iter()
        .collect();
        let landed_now = party.close_turn_classed(
            SourceTake {
                cargo: LOAD,
                bulk: LOAD,
            },
            &classes,
            &CarriedGoods::default(),
            PACK,
        );
        assert!(
            landed_now.is_none(),
            "a walked pack does not land the turn it leaves"
        );
        assert_eq!(
            party.hunters_on_the_road(),
            2,
            "two packs of ten off twenty"
        );
        let mut packs = Vec::new();
        for _ in 0..WALK {
            packs.extend(party.open_turn().packs);
        }
        assert_eq!(packs.len(), 2, "both porters reach home after the walk");
        for pack in &packs {
            assert_eq!(pack.walk_turns, WALK, "each pack lands with its walk");
            assert!(
                (pack.classes["flesh"] / pack.cargo - FLESH_SHARE).abs() < 1e-5,
                "each pack is three-quarters flesh, as the load was"
            );
        }
        let home: f32 = packs.iter().flat_map(|pack| pack.classes.values()).sum();
        assert!(
            (home - LOAD).abs() < 1e-4,
            "every class of the load came home: {home}"
        );
    }
}
