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
//! sheds the band down to its carry **the moment the order is accepted**, and what is left behind is
//! **lost**: there is no storage object to leave it in. The shedding order is **food first**: if
//! the food tier alone (`food + fodder_carry_weight × fodder`) overfills the packs, food and hay
//! scale down to fit and every item and material is left. Otherwise items and materials scale down
//! together by `(cap − food mass) ÷ goods load`, floored to whole units — and the units left are the
//! **most worn**: the band carries its best gear ([`BandEquipment::shed_units`]). One function plans
//! the shed ([`plan_long_move_shed`]); the move applies its plan and the snapshot publishes the same
//! plan as the band's long-move forecast, so the two can never disagree.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::components::{BandEquipment, LocalStore, FODDER, FOOD};
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
    /// consumption.per_capita_draw` = `8.0 / 0.16` = **50 person-turns**. One animal's feed per turn
    /// is `fodder_per_biomass × body_mass` (`fauna_config`), which for the mid-sized pennable animals
    /// hay is historically for — `crag_goat` (0.05 × 6 = 0.30) and `wild_sheep` (0.05 × 5.6 = 0.28)
    /// — averages ~0.29. So `8.0 / (50 × 0.29)` = 0.55, and `0.5` is still the clean dial beside it
    /// (the ratio is independent of the pack, which cancels): a one-worker load is **53 goat-turns /
    /// 57 sheep-turns**, the same order as food's 50 person-turns by construction.
    ///
    /// **The spread across the roster is honest and intended** — the same bale is 1,026 turns of
    /// keep for a fowl and 2 turns for an aurochs, because that is what those animals eat. It is a
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
pub fn carry_capacity(workers: u32, carry: &CarryConfig) -> Scalar {
    Scalar::from_u32(workers) * Scalar::from_f32(per_worker_carry(carry))
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

/// **Plan what a band of `workers` hands leaves behind on a long move** — the one shedding rule, read
/// by the move and by the snapshot's forecast alike.
///
/// - **The food tier loads first.** If `food + fodder_carry_weight × fodder` exceeds the band's
///   carry, both scale by `cap ÷ food mass` (the excess is left) and every item and material is
///   left.
/// - **Otherwise goods share what the food leaves.** When the goods load exceeds
///   `cap − food mass`, every item's units and every material's units scale by that ratio and floor
///   to whole units kept; the rest is left.
pub fn plan_long_move_shed(
    stores: &LocalStore,
    equipment: Option<&BandEquipment>,
    workers: u32,
    carry: &CarryConfig,
) -> LongMoveShed {
    let cap = carry_capacity(workers, carry);
    let held = held_load(stores, equipment);
    if held.load(carry) <= cap {
        return LongMoveShed::default();
    }
    let food_mass = held.food_mass(carry);
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
    if food_mass > cap {
        return LongMoveShed {
            food: held.food - scale_by_ratio(held.food, cap, food_mass),
            fodder: held.fodder - scale_by_ratio(held.fodder, cap, food_mass),
            items: every_item(),
            materials: stores
                .materials()
                .map(|(material, _)| (material.to_string(), stores.material_total(material)))
                .filter(|(_, amount)| *amount > scalar_zero())
                .collect(),
        };
    }
    let allowance = cap - food_mass;
    let goods_load = held.goods_load(carry);
    let items = every_item()
        .into_iter()
        .filter_map(|(item, units)| {
            let left = units - scale_units_by_ratio(units, allowance, goods_load);
            (left > 0).then_some((item, left))
        })
        .collect();
    let materials = stores
        .materials()
        .filter_map(|(material, _)| {
            let held = stores.material_total(material);
            let kept = floor_to_whole(scale_by_ratio(held, allowance, goods_load));
            let left = held - kept;
            (left > scalar_zero()).then(|| (material.to_string(), left))
        })
        .collect();
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

    /// **Workers × one pack, and dependants add nothing** — 17 hands on the shipped 8.0 carry 136,
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
        let plan = plan_long_move_shed(&stores, None, 4, &carry());
        assert!(plan.is_empty(), "{plan:?}");
    }

    /// **Food over the cap**: 4 hands carry 24; a 40-food larder keeps 24 and every item is left.
    #[test]
    fn food_over_the_cap_sheds_food_and_every_item() {
        let mut stores = larder(40);
        let mut ledger = BandEquipment::default();
        ledger.stock("spears", 3, "stone", None);
        let plan = plan_long_move_shed(&stores, Some(&ledger), 4, &carry());
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
        let plan = plan_long_move_shed(&stores, Some(&ledger), 4, &carry());
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
}
