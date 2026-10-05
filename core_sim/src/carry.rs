//! **ONE carry system for every material movement** (#732) — the pack a person carries, what a load
//! weighs, and what a band leaves behind when it walks farther than it can ferry.
//!
//! Whatever the reason goods move — a band splitting, a band walking a long way, a party walking a
//! shipment to another band — they are carried by people, and this module is the one statement of
//! how much people carry ([`carry_capacity`], through the [`per_worker_carry`] seam) and of what a
//! load weighs ([`CarryLoad`]). A future carrier — a cart, a pack animal, a truck — attaches at
//! [`per_worker_carry`] and every consumer benefits through this one code path.
//!
//! The levers live in `expedition_config.json`'s top-level `carry` block ([`CarryConfig`]), loaded
//! with the rest of that file on the shared boot seam.
//!
//! **The long move** is the last part: a short move keeps everything — within
//! [`move_ferry_reach_tiles`] the band can ferry its goods across in trips. A move farther than that
//! sheds the band down to its carry **as it departs** — the order's first step, in
//! `systems::advance_band_movement` — and what is left behind is
//! **lost**: there is no storage object to leave it in. The shedding order is **food first**: if
//! the food tier alone (`food + fodder_carry_weight × fodder`) overfills the packs, food and hay
//! scale down to fit and every item and material is left. **An overage smaller than one whole unit
//! comes off the food**, so a hair of drift never costs a tool. Otherwise the goods are cut **tools
//! before materials** in the room the food leaves ([`goods_cut`]): the materials go first, and the
//! items scale down only if they alone do not fit, whole units apportioned across the rows by largest
//! remainder — and the units left are the **most worn**: the band carries its best gear
//! ([`BandEquipment::shed_units`]). One function plans the shed ([`plan_long_move_shed`]); the move
//! applies its plan and the snapshot publishes the same plan as the band's long-move forecast, so the
//! two can never disagree.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::components::{BandEquipment, LocalStore, PopulationCohort, FODDER, FOOD};
use crate::scalar::{scalar_zero, Scalar};
use crate::supply_network_config::SupplyNetworkConfig;

/// **THE pack — every carrier's levers** (`expedition_config.json` → `carry`). One person's pack
/// space and what each kind of good costs in it, for a band split, a long move and a trade shipment
/// alike.
///
/// **There is deliberately no friction or loss lever here.** What a long haul costs is already paid
/// in the right currency: a trade party is provisioned like a scout, so a farther destination draws a
/// bigger launch larder and burns more upkeep on the road; a band that walks farther than it can
/// ferry leaves what it cannot carry. A percentage-lost-per-tile dial on top would price distance
/// twice.
#[derive(Debug, Clone, Deserialize)]
pub struct CarryConfig {
    /// One person's pack space, in the same units the food larder is counted in. A band carries
    /// `working-age hands × this`, a shipment party `party × this` — through [`per_worker_carry`],
    /// never read directly.
    pub per_worker_carry: f32,
    /// **What one whole ITEM unit costs in pack space, relative to one unit of food.** A kit's load
    /// is the load of the items it expands to, so `big_game` (spears + sled) weighs two of these.
    /// A shipment carries no gear today, so on that carrier the term is zero by construction.
    ///
    /// A flat cost per unit, made tunable rather than hardcoded: an item's real bulk is a property of
    /// the item, and when `equipment.json` grows a weight axis this lever is the seam that reads it.
    /// `0.0` is legal and means *"gear is weightless"*, so it is bounded below at zero.
    pub item_carry_weight: f32,
    /// **What one unit of a material costs in pack space, relative to one unit of food**, so at
    /// `1.0` a unit of hide and a unit of provisions occupy the same pack.
    ///
    /// A v1 **simplification made tunable rather than hardcoded**: a material's real bulk is a
    /// property of the material, and when `materials.json` grows a density axis this lever is the
    /// seam that reads it. `0.0` is legal and means *"materials are weightless"*, which is why it is
    /// bounded below at zero rather than above it.
    pub material_carry_weight: f32,
    /// **What one unit of HAY costs in pack space, relative to one unit of food.**
    ///
    /// **The shipped `0.5` is a bale priced against a meal, in TURNS OF KEEP.** Food is the
    /// numéraire at `1.0`, and one worker's load of it is `per_worker_carry / demographics
    /// consumption.per_capita_draw` = `7.0 / 0.16` = **43.75 person-turns**. One animal's feed per
    /// turn is `fodder_per_biomass × body_mass` (`fauna_config`), which for the mid-sized pennable
    /// animals hay is historically for — `crag_goat` (0.05 × 6 = 0.30) and `wild_sheep`
    /// (0.05 × 5.6 = 0.28) — averages ~0.29. So `7.0 / (43.75 × 0.29)` = 0.55, and `0.5` is still the
    /// clean dial beside it (the ratio is independent of the pack, which cancels): a one-worker load
    /// is **47 goat-turns / 50 sheep-turns**, the same order as food's 43.75 person-turns by
    /// construction.
    ///
    /// **The spread across the roster is honest and intended** — the same one-worker load is about
    /// 1,200 turns of keep for a fowl and 2.3 turns for an aurochs, because that is what those
    /// animals eat. It is a
    /// **playtest dial**. `0.0` is legal and means *"hay is weightless"*.
    pub fodder_carry_weight: f32,
}

/// **The resolved carry of ONE worker** — what the sim says a person hauls, not a lever read. Today
/// a person carries what their back carries, so it resolves to [`CarryConfig::per_worker_carry`]
/// alone.
///
/// **It exists so that carry has one home.** A carrier-side model — a cart kit's carry stat, a pack
/// animal, a tech factor, a road grade — attaches *here*, and every consumer picks it up without an
/// edit: a band's outfitting and split budgets, a long move's shed, the launch refusal
/// (`resolve_shipment`), the per-mission `expedition_carry_cap` a live party publishes, and the
/// `carryPerWorker` echo a client multiplies by the party it is composing. The wire promises the
/// client this resolved number, so a client that runs `workers × it` gets the sim's own answer
/// whatever the model grows into.
///
/// **It must stay positive.** The lever is validated `> 0` at load, and a `0` reaching the wire lets
/// a client render a zero cap and refuse every manifest a player could build — so any term a future
/// model multiplies or adds here has to preserve that.
pub fn per_worker_carry(carry: &CarryConfig) -> f32 {
    carry.per_worker_carry
}

/// **How much `workers` people can carry** — `workers × `[`per_worker_carry`], in the food-unit load
/// [`CarryLoad`] measures. The one capacity rule for every carrier: an opening band's outfitting
/// budget, a splinter's carry, what a band keeps on a long move, a shipment party's cargo cap.
///
/// **Workers carry; dependants do not.** Children and elders travel with a band and add no
/// capacity, so splitting off the elders buys no cargo space.
///
/// A whole head count is the right input where whole people are being counted out — the workers a
/// split sends across, a shipment's party. A band carrying its own goods is priced on its actual
/// working-age value instead: [`band_carry_capacity`].
pub fn carry_capacity(workers: u32, carry: &CarryConfig) -> Scalar {
    carry_capacity_of(Scalar::from_u32(workers), carry)
}

/// **How much a continuous `working` count carries** — `working × `[`per_worker_carry`]. The one
/// multiplication both [`carry_capacity`] and [`band_carry_capacity`] go through.
pub fn carry_capacity_of(working: Scalar, carry: &CarryConfig) -> Scalar {
    working.max(scalar_zero()) * Scalar::from_f32(per_worker_carry(carry))
}

/// **The worker count a band's OWN carry is priced on** — its actual working-age value, unfloored.
///
/// Carry is a continuous quantity, so it is struck on the continuous count. Flooring it, as the
/// commands' assignable count does, made one turn of demographic drift cost a whole pack: a splinter
/// sent out with 4 workers held `3.99` a turn later, floored to 3, and its next long move priced it
/// at 24 against the 32 it was split with — and left two of its three baskets behind.
pub fn band_carry_workers(cohort: &PopulationCohort) -> Scalar {
    cohort.working
}

/// **What a band can carry of its own goods** — [`carry_capacity_of`] its [`band_carry_workers`].
/// Read by the long move, the published `carryCapacity` and the long-move forecast alike.
pub fn band_carry_capacity(cohort: &PopulationCohort, carry: &CarryConfig) -> Scalar {
    carry_capacity_of(band_carry_workers(cohort), carry)
}

/// **Where a fit over its room cuts** — the one materials-before-tools staging, decided on the two
/// goods loads. Tools feed a band; materials can be gathered again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoodsCut {
    /// Everything fits.
    KeepAll,
    /// The items fit whole; the materials are fitted into `room`, what the items leave.
    CutMaterials { room: Scalar },
    /// The items alone are over: every material goes and the items are fitted into `room`.
    CutItems { room: Scalar },
}

/// **Decide the cut** for `item_load` and `material_load` against `room`. [`fit_to_carry`]
/// (`starting_loadout`) and [`plan_long_move_shed`] both stage through this, so a default outfit, a
/// split's default take and a long move cut in the same order.
///
/// [`fit_to_carry`]: crate::starting_loadout::fit_to_carry
pub fn goods_cut(item_load: Scalar, material_load: Scalar, room: Scalar) -> GoodsCut {
    if item_load + material_load <= room {
        GoodsCut::KeepAll
    } else if item_load <= room {
        GoodsCut::CutMaterials {
            room: room - item_load,
        }
    } else {
        GoodsCut::CutItems { room }
    }
}

/// **The goods being carried, by kind** — the four quantities a load is measured on.
///
/// `items` is whole units summed over every item (a kit counts the items it expands to);
/// `materials` is units summed over every material, in the store's fixed point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CarryLoad {
    pub food: Scalar,
    pub fodder: Scalar,
    pub items: u32,
    pub materials: Scalar,
}

impl CarryLoad {
    /// Goods only — an outfitting order, which carries no food.
    pub fn goods(items: u32, materials: Scalar) -> Self {
        Self {
            items,
            materials,
            ..Self::default()
        }
    }

    /// **The food tier's load** — `food + fodder_carry_weight × fodder`, priced alone wherever one
    /// tier loads before the other (a long move's food, a split's food share).
    pub fn food_mass(&self, carry: &CarryConfig) -> Scalar {
        self.food + self.fodder * Scalar::from_f32(carry.fodder_carry_weight)
    }

    /// **The goods tier's load** — `item_carry_weight × items + material_carry_weight × materials`.
    pub fn goods_load(&self, carry: &CarryConfig) -> Scalar {
        Scalar::from_u32(self.items) * Scalar::from_f32(carry.item_carry_weight)
            + self.materials * Scalar::from_f32(carry.material_carry_weight)
    }

    /// **THE LOAD** — `food + fodder_carry_weight × fodder + item_carry_weight × items +
    /// material_carry_weight × materials`, in food units. The one statement of the formula; every
    /// carrier measures through it.
    pub fn load(&self, carry: &CarryConfig) -> Scalar {
        self.food_mass(carry) + self.goods_load(carry)
    }
}

/// **`amount × numerator ÷ denominator`, floored, in exact fixed point** — the one scaling every
/// carry fit divides by (`budget ÷ load`). Done in `i128` on the raw values so a ratio never hops
/// through `f32`, for the reason `fission::whole_share_of` states. A non-positive denominator scales
/// to nothing rather than dividing by zero.
pub fn scale_by_ratio(amount: Scalar, numerator: Scalar, denominator: Scalar) -> Scalar {
    if denominator.raw() <= 0 || numerator.raw() <= 0 || amount.raw() <= 0 {
        return Scalar::zero();
    }
    let scaled =
        i128::from(amount.raw()) * i128::from(numerator.raw()) / i128::from(denominator.raw());
    Scalar::from_raw(scaled.clamp(0, i128::from(i64::MAX)) as i64)
}

/// **`count × numerator ÷ denominator`, floored to whole units** — [`scale_by_ratio`] for a whole
/// count (a kit row, an item stack, a material's whole units).
pub fn scale_units_by_ratio(count: u32, numerator: Scalar, denominator: Scalar) -> u32 {
    if denominator.raw() <= 0 || numerator.raw() <= 0 {
        return 0;
    }
    let scaled = i128::from(count) * i128::from(numerator.raw()) / i128::from(denominator.raw());
    scaled.clamp(0, i128::from(u32::MAX)) as u32
}

/// ⛔ **HOW FAR A BAND MAY MOVE AND STILL KEEP EVERYTHING — READ THROUGH THIS FUNCTION, NEVER AS A
/// BARE CONFIG FIELD**, in hex steps.
///
/// Within this reach a band ferries its goods across in trips, so it keeps them all; past it, it
/// walks off with what it can carry. The base is `supply_network_config.json` `reach_tiles` — the
/// radius within which same-faction bands already pool their stores for free, which is the same
/// statement: goods move freely that far. **No lever of its own.**
///
/// It is a seam on `routes::road_keeping_range`'s discipline: the config holds a base and every
/// caller (the move, the published forecast) asks here, so a reach that later grows — with roads,
/// pack animals or a cart — is this body changing and no call site moving.
pub fn move_ferry_reach_tiles(supply: &SupplyNetworkConfig) -> u32 {
    supply.reach_tiles
}

/// **Everything a band holds, as a load** — its larder, its hay, every whole item unit in its ledger
/// (bench tools included — a loom weighs like anything else) and every material unit.
pub fn held_load(stores: &LocalStore, equipment: Option<&BandEquipment>) -> CarryLoad {
    CarryLoad {
        food: stores.get(FOOD),
        fodder: stores.get(FODDER),
        items: equipment.map_or(0, BandEquipment::total_units),
        materials: stores
            .materials()
            .fold(scalar_zero(), |total, (material, _)| {
                total + stores.material_total(material)
            }),
    }
}

/// **`target` whole units in the mix of `source`**, by largest remainder: each row's exact quota is
/// `target × count ÷ total`; every row takes its floor, and the units left over go one each to the
/// rows with the largest remainders, ties to the lower id. A source holding `target` units or fewer
/// gives every one of them, so no row ever exceeds its source count. A split's kit share and a long
/// move's kept goods are both apportioned through this.
pub fn apportion_largest_remainder(
    source: &BTreeMap<String, u32>,
    target: u32,
) -> BTreeMap<String, u32> {
    let total: u64 = source.values().map(|count| u64::from(*count)).sum();
    if u64::from(target) >= total {
        return source.clone();
    }
    let target = u64::from(target);
    let mut rows: Vec<(&String, u64, u64)> = source
        .iter()
        .map(|(id, count)| {
            let exact = target * u64::from(*count);
            (id, exact / total, exact % total)
        })
        .collect();
    let given: u64 = rows.iter().map(|(_, floor, _)| *floor).sum();
    let mut order: Vec<usize> = (0..rows.len()).collect();
    // Largest remainder first; `rows` is in id order, so a stable sort breaks ties to the lower id.
    order.sort_by(|a, b| rows[*b].2.cmp(&rows[*a].2));
    for index in order.into_iter().take((target - given) as usize) {
        rows[index].1 += 1;
    }
    rows.into_iter()
        .filter(|(_, count, _)| *count > 0)
        .map(|(id, count, _)| (id.clone(), count as u32))
        .collect()
}

/// **The load of the lightest whole unit a band holds** — one item, or one material unit, whichever
/// weighs less among the kinds it holds any of. `None` when it holds no goods, or only weightless
/// ones.
fn lightest_whole_unit(held: &CarryLoad, carry: &CarryConfig) -> Option<Scalar> {
    let item = (held.items > 0).then(|| CarryLoad::goods(1, scalar_zero()).goods_load(carry));
    let material = (held.materials >= Scalar::one())
        .then(|| CarryLoad::goods(0, Scalar::one()).goods_load(carry));
    [item, material]
        .into_iter()
        .flatten()
        .filter(|unit| *unit > scalar_zero())
        .min()
}

/// **How many whole units of `unit_load` fit in `room`** — every unit when a unit weighs nothing.
fn whole_units_in(room: Scalar, unit_load: Scalar) -> u32 {
    if unit_load <= scalar_zero() {
        return u32::MAX;
    }
    u32::try_from(room.max(scalar_zero()).raw() / unit_load.raw()).unwrap_or(u32::MAX)
}

/// **What a long move leaves behind** — the plan [`plan_long_move_shed`] resolves and
/// [`shed_for_long_move`] applies. Empty when the band fits its carry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LongMoveShed {
    /// Food left, in the larder's fixed point. Taken in proportion to the larder's keeping classes.
    pub food: Scalar,
    /// Hay left.
    pub fodder: Scalar,
    /// Whole units left per item — the most worn of each.
    pub items: BTreeMap<String, u32>,
    /// Material left per material, in the store's fixed point.
    pub materials: BTreeMap<String, Scalar>,
}

impl LongMoveShed {
    /// Whether the band keeps everything.
    pub fn is_empty(&self) -> bool {
        self.food <= scalar_zero()
            && self.fodder <= scalar_zero()
            && self.items.values().all(|units| *units == 0)
            && self
                .materials
                .values()
                .all(|amount| *amount <= scalar_zero())
    }

    /// Whole item units left, over every item.
    pub fn item_units(&self) -> u32 {
        self.items.values().copied().sum()
    }

    /// Material units left, over every material.
    pub fn material_units(&self) -> Scalar {
        self.materials
            .values()
            .fold(scalar_zero(), |total, amount| total + *amount)
    }
}

/// **The whole units of a `Scalar` amount**, floored.
fn floor_to_whole(amount: Scalar) -> Scalar {
    Scalar::from_raw(amount.raw() - amount.raw().rem_euclid(Scalar::SCALE))
}

/// **Plan what a band of `working` hands leaves behind on a long move** — the one shedding rule, read
/// by the move and by the snapshot's forecast alike. `working` is the band's
/// [`band_carry_workers`], never a floored head count.
///
/// - **The food tier loads first.** If `food + fodder_carry_weight × fodder` exceeds the band's
///   carry, both scale by `cap ÷ food mass` (the excess is left) and every item and material is
///   left.
/// - **Then the goods, materials before tools** ([`goods_cut`]), in the room the food leaves:
///   - if the items alone fit, every item is kept and the materials keep as many whole units as
///     what the items leave holds, apportioned across them by largest remainder;
///   - otherwise every material is left and the items keep as many whole units as the room holds,
///     apportioned the same way, the most worn units left.
///
///   Apportioned, not floored row by row: a load a hair over its room once lost one unit from
///   EVERY row (nine tools for 0.07 of overage); now it loses the one unit the room cannot hold.
///
/// # ⛔ AN OVERAGE SMALLER THAN ONE WHOLE UNIT COMES OFF THE FOOD
///
/// When the load is over the carry by less than the lightest whole unit the band holds (one item at
/// `item_carry_weight`, or one material unit at `material_carry_weight`), the overage is taken from
/// the food tier — food and hay in their own proportion, which are continuous — and every item and
/// material is kept. A carry drifts with the band's working value (a splinter sent out with 4 holds
/// 3.99 a turn later, its packs filled to the 28 it was split with), so a turn of drift leaves a
/// fully packed band a few hundredths over. Shedding a whole tool for 0.07 of load would cost the
/// band a thing it works with for a rounding error; a few hundredths of a meal is the honest price.
/// An overage of a whole unit or more keeps the order above.
///
/// Tools feed a band; materials can be gathered again. A uniform cut across both left a splinter
/// with one basket of three and most of its fibre.
pub fn plan_long_move_shed(
    stores: &LocalStore,
    equipment: Option<&BandEquipment>,
    working: Scalar,
    carry: &CarryConfig,
) -> LongMoveShed {
    let cap = carry_capacity_of(working, carry);
    let held = held_load(stores, equipment);
    if held.load(carry) <= cap {
        return LongMoveShed::default();
    }
    let food_mass = held.food_mass(carry);
    let overage = held.load(carry) - cap;
    if let Some(unit) = lightest_whole_unit(&held, carry) {
        if overage < unit && overage <= food_mass {
            let kept = food_mass - overage;
            return LongMoveShed {
                food: held.food - scale_by_ratio(held.food, kept, food_mass),
                fodder: held.fodder - scale_by_ratio(held.fodder, kept, food_mass),
                items: BTreeMap::new(),
                materials: BTreeMap::new(),
            };
        }
    }
    let every_item = || -> BTreeMap<String, u32> {
        equipment
            .map(|ledger| {
                ledger
                    .batches()
                    .map(|(item, _)| (item.to_string(), ledger.count_of(item)))
                    .filter(|(_, units)| *units > 0)
                    .collect()
            })
            .unwrap_or_default()
    };
    let every_material = || -> BTreeMap<String, Scalar> {
        stores
            .materials()
            .map(|(material, _)| (material.to_string(), stores.material_total(material)))
            .filter(|(_, amount)| *amount > scalar_zero())
            .collect()
    };
    if food_mass > cap {
        return LongMoveShed {
            food: held.food - scale_by_ratio(held.food, cap, food_mass),
            fodder: held.fodder - scale_by_ratio(held.fodder, cap, food_mass),
            items: every_item(),
            materials: every_material(),
        };
    }
    let item_load = CarryLoad::goods(held.items, scalar_zero()).goods_load(carry);
    let material_load = CarryLoad::goods(0, held.materials).goods_load(carry);
    let (items, materials) = match goods_cut(item_load, material_load, cap - food_mass) {
        GoodsCut::KeepAll => (BTreeMap::new(), BTreeMap::new()),
        GoodsCut::CutMaterials { room } => {
            // Keep as many WHOLE units as the room holds, apportioned across the materials in the
            // mix held by largest remainder — never a unit per row lost to flooring each one.
            let held = every_material();
            let whole: BTreeMap<String, u32> = held
                .iter()
                .map(|(material, amount)| {
                    let units = floor_to_whole(*amount).raw() / Scalar::SCALE;
                    (material.clone(), u32::try_from(units).unwrap_or(u32::MAX))
                })
                .collect();
            let unit = CarryLoad::goods(0, Scalar::one()).goods_load(carry);
            let kept = apportion_largest_remainder(&whole, whole_units_in(room, unit));
            let materials = held
                .into_iter()
                .filter_map(|(material, amount)| {
                    let keep = Scalar::from_u32(kept.get(&material).copied().unwrap_or(0));
                    let left = amount - keep;
                    (left > scalar_zero()).then_some((material, left))
                })
                .collect();
            (BTreeMap::new(), materials)
        }
        GoodsCut::CutItems { room } => {
            // The same: as many whole items as the room holds, in the held mix by largest
            // remainder; the units left are the most worn of each item (`shed_for_long_move`).
            let held = every_item();
            let unit = CarryLoad::goods(1, scalar_zero()).goods_load(carry);
            let kept = apportion_largest_remainder(&held, whole_units_in(room, unit));
            let items = held
                .into_iter()
                .filter_map(|(item, units)| {
                    let left = units - kept.get(&item).copied().unwrap_or(0);
                    (left > 0).then_some((item, left))
                })
                .collect();
            (items, every_material())
        }
    };
    LongMoveShed {
        food: scalar_zero(),
        fodder: scalar_zero(),
        items,
        materials,
    }
}

/// **Apply a planned shed to a band's store and ledger.** Returns the food actually removed — the
/// `left_behind` term the caller books on the food ledger.
///
/// Food leaves in proportion to the larder's keeping classes ([`LocalStore::take_food_mix`]), hay
/// off the commodity bag, items most worn first ([`BandEquipment::shed_units`]) and materials in the
/// store's own batch order ([`LocalStore::take_material_batches`]). Nothing is deposited anywhere:
/// what is left behind is lost.
pub fn shed_for_long_move(
    stores: &mut LocalStore,
    equipment: Option<&mut BandEquipment>,
    plan: &LongMoveShed,
) -> Scalar {
    let food_left = stores.take_food_mix(plan.food).total();
    if plan.fodder > scalar_zero() {
        stores.take(FODDER, plan.fodder);
    }
    if let Some(ledger) = equipment {
        for (item, units) in &plan.items {
            ledger.shed_units(item, *units);
        }
    }
    for (material, amount) in &plan.materials {
        stores.take_material_batches(material, *amount);
    }
    food_left
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expedition_config::ExpeditionConfig;

    fn carry() -> CarryConfig {
        let mut carry = ExpeditionConfig::builtin().carry.clone();
        carry.per_worker_carry = 6.0;
        carry.fodder_carry_weight = 0.5;
        carry.item_carry_weight = 1.0;
        carry.material_carry_weight = 1.0;
        carry
    }

    /// **Workers × one pack, and dependants add nothing** — 17 hands carry `17 × per_worker_carry`,
    /// the opening band's budget.
    #[test]
    fn a_bands_carry_is_its_workers_times_one_pack() {
        let config = ExpeditionConfig::builtin();
        assert_eq!(
            carry_capacity(17, &config.carry),
            Scalar::from_u32(17) * Scalar::from_f32(config.carry.per_worker_carry)
        );
        assert_eq!(carry_capacity(0, &config.carry), Scalar::zero());
    }

    /// The load formula, one term at a time: food at 1, hay at its weight, an item at its weight, a
    /// material unit at its weight.
    #[test]
    fn a_load_prices_every_kind_at_its_own_weight() {
        let mut carry = carry();
        carry.fodder_carry_weight = 0.5;
        carry.item_carry_weight = 2.0;
        carry.material_carry_weight = 0.25;
        let load = CarryLoad {
            food: Scalar::from_u32(10),
            fodder: Scalar::from_u32(4),
            items: 3,
            materials: Scalar::from_u32(8),
        };
        assert_eq!(load.food_mass(&carry), Scalar::from_u32(12));
        assert_eq!(load.goods_load(&carry), Scalar::from_u32(8));
        assert_eq!(load.load(&carry), Scalar::from_u32(20));
    }

    /// The ratio scalers floor in exact fixed point and are total at a zero denominator.
    #[test]
    fn the_ratio_scalers_floor_exactly() {
        let third = (Scalar::from_u32(1), Scalar::from_u32(3));
        assert_eq!(scale_units_by_ratio(3, third.0, third.1), 1);
        assert_eq!(scale_units_by_ratio(2, third.0, third.1), 0);
        assert_eq!(
            scale_by_ratio(
                Scalar::from_u32(30),
                Scalar::from_u32(5),
                Scalar::from_u32(15)
            ),
            Scalar::from_u32(10)
        );
        assert_eq!(scale_units_by_ratio(9, Scalar::one(), Scalar::zero()), 0);
        assert_eq!(
            scale_by_ratio(Scalar::from_u32(9), Scalar::one(), Scalar::zero()),
            Scalar::zero()
        );
    }

    fn larder(food: u32) -> LocalStore {
        let mut stores = LocalStore::new();
        stores.reset_food("dry", Scalar::from_u32(food));
        stores
    }

    /// A band that fits its carry keeps everything.
    #[test]
    fn a_band_that_fits_keeps_everything() {
        let stores = larder(10);
        let plan = plan_long_move_shed(&stores, None, Scalar::from_u32(4), &carry());
        assert!(plan.is_empty(), "{plan:?}");
    }

    /// **Food over the cap**: 4 hands carry 24; a 40-food larder keeps 24 and every item is left.
    #[test]
    fn food_over_the_cap_sheds_food_and_every_item() {
        let mut stores = larder(40);
        let mut ledger = BandEquipment::default();
        ledger.stock("spears", 3, "stone", None);
        let plan = plan_long_move_shed(&stores, Some(&ledger), Scalar::from_u32(4), &carry());
        assert_eq!(plan.food, Scalar::from_u32(16));
        assert_eq!(plan.items.get("spears"), Some(&3));
        let left = shed_for_long_move(&mut stores, Some(&mut ledger), &plan);
        assert_eq!(left, Scalar::from_u32(16));
        assert_eq!(stores.get(FOOD), Scalar::from_u32(24));
        assert_eq!(ledger.count_of("spears"), 0);
    }

    /// **Goods share what the food leaves, and the most worn units are left.** 4 hands carry 24; 14
    /// food leaves 10 for goods; 20 spears scale to 10 kept, and the 10 left are the worn batch.
    #[test]
    fn goods_scale_into_what_the_food_leaves_and_the_worn_units_are_left() {
        let mut stores = larder(14);
        let mut ledger = BandEquipment::default();
        ledger.stock("spears", 10, "stone", None);
        ledger.stock("spears", 10, "stone", None);
        ledger.restore_batches(
            "spears",
            ledger
                .batches_of("spears")
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, mut batch)| {
                    batch.wear = if index == 0 { 0.9 } else { 0.0 };
                    batch
                })
                .collect(),
        );
        let plan = plan_long_move_shed(&stores, Some(&ledger), Scalar::from_u32(4), &carry());
        assert_eq!(plan.food, scalar_zero());
        assert_eq!(plan.items.get("spears"), Some(&10));
        shed_for_long_move(&mut stores, Some(&mut ledger), &plan);
        assert_eq!(ledger.count_of("spears"), 10);
        assert!(
            ledger
                .batches_of("spears")
                .iter()
                .all(|batch| batch.wear == 0.0),
            "the band carries its best gear: {:?}",
            ledger.batches_of("spears")
        );
        assert_eq!(stores.get(FOOD), Scalar::from_u32(14));
    }
    /// The unit weights at the 8.0 pack the seed-37 case was measured on — a fixture pack, held
    /// fixed so the case's numbers do not move with the shipped dial.
    fn shipped_pack() -> CarryConfig {
        let mut carry = carry();
        carry.per_worker_carry = 8.0;
        carry
    }

    /// **The seed-37 splinter's holdings**: 15.66 food, three baskets and ten material units — a
    /// load of 28.66.
    fn seed_37_splinter() -> (LocalStore, BandEquipment) {
        let mut stores = LocalStore::new();
        stores.reset_food("dry", Scalar::from_f32(15.66));
        let key = crate::materials_config::BandKey(vec![1]);
        for (material, units) in [("bone", 2), ("fibre", 6), ("hide", 2)] {
            stores.deposit_material(
                material,
                key.clone(),
                Scalar::from_u32(units),
                &BTreeMap::new(),
            );
        }
        let mut ledger = BandEquipment::default();
        ledger.stock("baskets", 3, "plain", None);
        (stores, ledger)
    }

    /// ⛔ **A BAND'S CARRY IS PRICED ON ITS ACTUAL WORKING VALUE, SO DRIFT COSTS NO PACK.** A
    /// splinter sent out with 4 workers held 3.99 a turn later; floored to 3 its long move priced it
    /// at 24 and left two of its three baskets. Unfloored it carries 31.92 and keeps everything.
    #[test]
    fn a_splinter_drifting_below_four_workers_keeps_its_four_worker_carry() {
        let carry = shipped_pack();
        let drifted = Scalar::from_f32(3.99);
        let cap = carry_capacity_of(drifted, &carry);
        assert!(
            cap > carry_capacity(3, &carry) && cap > Scalar::from_f32(31.9),
            "3.99 workers carry 3.99 packs, not 3: {cap}"
        );
        let (stores, ledger) = seed_37_splinter();
        let plan = plan_long_move_shed(&stores, Some(&ledger), drifted, &carry);
        assert!(plan.is_empty(), "the 28.66 load fits 31.92: {plan:?}");
    }

    /// ⛔ **A LONG MOVE CUTS MATERIALS BEFORE TOOLS.** The seed-37 load against three workers' 24:
    /// food 15.66 loads first and leaves 8.34; the three baskets fit in that, so every basket is
    /// kept and the ten material units are cut into the 5.34 left — bone 1, fibre 3, hide 1 kept.
    /// The uniform cut this replaced kept one basket of three.
    #[test]
    fn a_long_move_keeps_its_tools_when_the_materials_alone_make_room() {
        let carry = shipped_pack();
        let (mut stores, mut ledger) = seed_37_splinter();
        let plan = plan_long_move_shed(&stores, Some(&ledger), Scalar::from_u32(3), &carry);
        assert_eq!(plan.food, scalar_zero(), "food loads first");
        assert!(plan.items.is_empty(), "every basket is kept: {plan:?}");
        assert_eq!(
            plan.materials,
            [("bone", 1), ("fibre", 3), ("hide", 1)]
                .into_iter()
                .map(|(id, units)| (id.to_string(), Scalar::from_u32(units)))
                .collect::<BTreeMap<_, _>>()
        );
        shed_for_long_move(&mut stores, Some(&mut ledger), &plan);
        assert_eq!(ledger.count_of("baskets"), 3);
        assert!(
            held_load(&stores, Some(&ledger)).load(&carry) <= carry_capacity(3, &carry),
            "and what is kept fits the packs"
        );
    }

    /// **When the tools alone overfill what the food leaves, every material goes and the tools
    /// scale.** 2 hands carry 16; 14 food leaves 2 for three baskets — two are kept, the worst one
    /// left, and every material unit is left.
    #[test]
    fn tools_over_the_room_drop_every_material_and_scale() {
        let carry = shipped_pack();
        let (mut stores, ledger) = seed_37_splinter();
        stores.reset_food("dry", Scalar::from_u32(14));
        let plan = plan_long_move_shed(&stores, Some(&ledger), Scalar::from_u32(2), &carry);
        assert_eq!(plan.items.get("baskets"), Some(&1));
        assert_eq!(plan.material_units(), Scalar::from_u32(10));
    }

    /// ⛔ **A LOAD OVER ITS ROOM LOSES THE WHOLE UNITS THE ROOM CANNOT HOLD — NOT ONE PER ROW.**
    /// Nine items of nine kinds and 13 food against 20.93 of carry (1.07 over, more than one unit, so
    /// the food does not absorb it): 7.93 of room holds seven whole items, so two are left. Flooring
    /// each row at `0.88` once left all nine.
    #[test]
    fn a_hair_over_its_carry_sheds_one_unit_not_one_per_row() {
        let carry = carry();
        let mut stores = larder(13);
        let mut ledger = BandEquipment::default();
        for item in [
            "spears", "sled", "traps", "baskets", "hoes", "crook", "clubs", "axe", "wedges",
        ] {
            ledger.stock(item, 1, "plain", None);
        }
        let working = Scalar::from_f32(20.93) / Scalar::from_f32(carry.per_worker_carry);
        let plan = plan_long_move_shed(&stores, Some(&ledger), working, &carry);
        assert_eq!(plan.item_units(), 2, "two items left, not nine: {plan:?}");
        assert!(plan.materials.is_empty() && plan.food == scalar_zero());
        shed_for_long_move(&mut stores, Some(&mut ledger), &plan);
        assert_eq!(ledger.total_units(), 7);
    }

    /// ⛔ **A DRIFT OVERAGE UNDER ONE WHOLE UNIT COMES OFF THE FOOD.** Three baskets and 25 food
    /// is a load of 28; at 3.99 workers on a 7.0 pack the carry is 27.93, 0.07 short. The band keeps
    /// every basket and leaves 0.07 food.
    #[test]
    fn an_overage_under_one_unit_comes_off_the_food() {
        let mut carry = carry();
        carry.per_worker_carry = 7.0;
        let mut stores = larder(25);
        let mut ledger = BandEquipment::default();
        ledger.stock("baskets", 3, "plain", None);
        let plan = plan_long_move_shed(&stores, Some(&ledger), Scalar::from_f32(3.99), &carry);
        assert!(
            plan.items.is_empty() && plan.materials.is_empty(),
            "{plan:?}"
        );
        assert!(
            (plan.food.to_f32() - 0.07).abs() < 1e-3,
            "0.07 of food: {plan:?}"
        );
        shed_for_long_move(&mut stores, Some(&mut ledger), &plan);
        assert_eq!(ledger.count_of("baskets"), 3);
        assert!(
            held_load(&stores, Some(&ledger)).load(&carry)
                <= carry_capacity_of(Scalar::from_f32(3.99), &carry)
        );
    }

    /// **An overage of a whole unit or more still sheds whole units, in the old order.** Three
    /// baskets, two spears and 25 food is a load of 30 against 4 × 7.0 = 28 — two whole units over:
    /// the food loads first and stays, and two whole items are left.
    #[test]
    fn an_overage_of_a_whole_unit_still_sheds_whole_units() {
        let mut carry = carry();
        carry.per_worker_carry = 7.0;
        let stores = larder(25);
        let mut ledger = BandEquipment::default();
        ledger.stock("baskets", 3, "plain", None);
        ledger.stock("spears", 2, "plain", None);
        let plan = plan_long_move_shed(&stores, Some(&ledger), Scalar::from_u32(4), &carry);
        assert_eq!(
            plan.food,
            scalar_zero(),
            "food loads first and stays: {plan:?}"
        );
        assert_eq!(plan.item_units(), 2, "two whole items are left: {plan:?}");
    }

    /// The staging itself, in its three cases.
    #[test]
    fn the_goods_cut_keeps_tools_before_materials() {
        let units = Scalar::from_u32;
        assert_eq!(goods_cut(units(3), units(4), units(7)), GoodsCut::KeepAll);
        assert_eq!(
            goods_cut(units(3), units(4), units(5)),
            GoodsCut::CutMaterials { room: units(2) }
        );
        assert_eq!(
            goods_cut(units(3), units(4), units(2)),
            GoodsCut::CutItems { room: units(2) }
        );
    }
}
