use std::cmp::min;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use bevy::{math::UVec2, prelude::*};
use sim_runtime::{
    KnownTechFragment as ContractKnowledgeFragment, RiverChannel, RiverClass, TerrainTags,
    TerrainType,
};

use crate::{
    generations::GenerationId,
    grid_utils::{HEX_CORNER_COUNT, HEX_DIRECTION_COUNT},
    intensification::RungKey,
    mapgen::MountainType,
    orders::FactionId,
    power::PowerNodeId,
    scalar::{scalar_from_f32, scalar_zero, Scalar},
};

/// Represents a discrete tile in the simulation grid.
#[derive(Component, Debug, Clone)]
pub struct Tile {
    pub position: UVec2,
    pub element: ElementKind,
    pub temperature: Scalar,
    pub terrain: TerrainType,
    pub terrain_tags: TerrainTags,
    /// The biome this tile's RESOURCE yields are read from when it is not `terrain` itself.
    ///
    /// Set **only** on a `NavigableRiver` hex, at the hydrology stamp, to the biome the channel was
    /// cut through (`hydrology.rs`). A navigable river stays *mechanically* water — impassable to
    /// land, sailable, bisecting — but a giant river running through a valley still yields the
    /// valley's forage/graze, not open water. `None` everywhere else (the tile just is its
    /// `terrain`). Read through [`Tile::resource_terrain`]; movement/logistics/attrition keep
    /// keying on `terrain`.
    pub underlying_terrain: Option<TerrainType>,
    pub mountain: Option<MountainMetadata>,
    /// Packed per-side river classes — 2 bits per odd-r direction (see `RiverClass`). Populated
    /// by `generate_hydrology` for **both** hexes flanking every traced river edge, so a hex
    /// always agrees with its neighbour about the river between them.
    ///
    /// This is the gameplay primitive a future movement system reads: entering this hex across
    /// direction `d` crosses `river_class_on_side(d)`. Nothing consumes it yet — by design.
    pub river_edges: u16,
    /// Packed per-**corner** river inflow — the same 2-bits-per-slot layout as `river_edges`, but
    /// keyed by hex *corner* (`grid_utils::HEX_CORNER_COUNT`, the client's screen-space vertex
    /// order) instead of by side.
    ///
    /// `generate_hydrology` sets it on the **first hex of a `NavigableRiver` chain** only, at the
    /// corner where the edge-river chain terminated, with the class of the last edge that chain
    /// emitted. An edge river runs corner-to-corner *along* a side, so it ends at a **vertex** —
    /// this field is that vertex. `river_edges` records which sides carry a river and cannot say
    /// this: a trunk hex can flank three river edges, which have two candidate chain-ends between
    /// them, so the renderer would be guessing where the tributary actually arrives.
    ///
    /// Zero on every other tile, and zero for a river that was navigable from its first step (no
    /// edge chain, so no inflow to name).
    pub river_inflow: u16,
    /// Packed per-side **channel exits** — 1 bit per odd-r direction (see `RiverChannel`): does
    /// this hex's navigable channel flow out through side `dir`?
    ///
    /// A navigable river is a chain of water hexes, and a chain is a **path**: a hex connects to its
    /// upstream and downstream neighbours and to nothing else. Terrain alone cannot say which those
    /// are — a renderer that arms every navigable/water neighbour cross-links adjacent chains into a
    /// **web**. Only the tracer knows the chain, so `generate_hydrology` writes it here, symmetric
    /// across each shared side (both hexes of a consecutive pair agree), plus one exit on the final
    /// hex pointing at the water body/delta the river drains into — otherwise the drawn river stops
    /// one hex short of the sea. A confluence hex carries the **union** of the chains through it.
    pub river_channel: u8,
}

impl Tile {
    /// Terrain that drives this tile's RESOURCE yields. A navigable river yields the
    /// valley it cut, not open water; everywhere else it is just `terrain`.
    pub fn resource_terrain(&self) -> TerrainType {
        self.underlying_terrain.unwrap_or(self.terrain)
    }

    /// The class of river running along side `dir` (odd-r direction, `0..6`). An out-of-range
    /// direction reads `None` — this is a lookup, not an assertion site.
    pub fn river_class_on_side(&self, dir: u8) -> RiverClass {
        if usize::from(dir) >= HEX_DIRECTION_COUNT {
            return RiverClass::None;
        }
        RiverClass::from_bits(self.river_edges >> (u32::from(dir) * RiverClass::BITS_PER_DIR))
    }

    /// Set the class of river running along side `dir`. Out-of-range directions are ignored.
    pub fn set_river_class_on_side(&mut self, dir: u8, class: RiverClass) {
        if usize::from(dir) >= HEX_DIRECTION_COUNT {
            return;
        }
        let shift = u32::from(dir) * RiverClass::BITS_PER_DIR;
        self.river_edges &= !(RiverClass::SLOT_MASK << shift);
        self.river_edges |= class.bits() << shift;
    }

    /// Whether any of the six sides carries a river.
    pub fn has_any_river_edge(&self) -> bool {
        self.river_edges != 0
    }

    /// The class of the edge river arriving at hex corner `corner` (`0..6`, see
    /// `grid_utils::HEX_CORNER_COUNT`). An out-of-range corner reads `None` — this is a lookup,
    /// not an assertion site.
    pub fn river_class_at_corner(&self, corner: u8) -> RiverClass {
        if usize::from(corner) >= HEX_CORNER_COUNT {
            return RiverClass::None;
        }
        RiverClass::from_bits(
            self.river_inflow >> (u32::from(corner) * RiverClass::BITS_PER_CORNER),
        )
    }

    /// Set the class of the edge river arriving at hex corner `corner`. Out-of-range corners are
    /// ignored.
    pub fn set_river_class_at_corner(&mut self, corner: u8, class: RiverClass) {
        if usize::from(corner) >= HEX_CORNER_COUNT {
            return;
        }
        let shift = u32::from(corner) * RiverClass::BITS_PER_CORNER;
        self.river_inflow &= !(RiverClass::SLOT_MASK << shift);
        self.river_inflow |= class.bits() << shift;
    }

    /// Whether any of the six corners takes an edge river's inflow.
    pub fn has_any_river_inflow(&self) -> bool {
        self.river_inflow != 0
    }

    /// Whether this hex's navigable channel flows out through side `dir` (odd-r direction, `0..6`).
    /// An out-of-range direction reads `false` — this is a lookup, not an assertion site.
    pub fn channel_exits(&self, dir: u8) -> bool {
        if usize::from(dir) >= HEX_DIRECTION_COUNT {
            return false;
        }
        (self.river_channel >> (u32::from(dir) * RiverChannel::BITS_PER_DIR))
            & RiverChannel::SLOT_MASK
            != 0
    }

    /// Record a channel exit through side `dir`. Out-of-range directions are ignored. Bits are
    /// **OR-ed**: a hex where two chains meet carries the union of their exits, never the last one
    /// written.
    pub fn set_channel_exit(&mut self, dir: u8) {
        if usize::from(dir) >= HEX_DIRECTION_COUNT {
            return;
        }
        self.river_channel |=
            RiverChannel::SLOT_MASK << (u32::from(dir) * RiverChannel::BITS_PER_DIR);
    }

    /// Whether this hex carries a navigable channel at all.
    pub fn has_any_channel_exit(&self) -> bool {
        self.river_channel != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MountainMetadata {
    pub kind: MountainType,
    pub relief: f32,
}

/// Procedural element categories used to vary material behavior.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementKind {
    Ferrite,
    Arborite,
    Zephyrite,
    Lumina,
}

impl ElementKind {
    pub fn thermal_bias(self) -> Scalar {
        match self {
            ElementKind::Ferrite => scalar_from_f32(-6.0),
            ElementKind::Arborite => scalar_from_f32(-2.5),
            ElementKind::Zephyrite => scalar_from_f32(1.5),
            ElementKind::Lumina => scalar_from_f32(4.0),
        }
    }

    pub fn conductivity(self) -> Scalar {
        match self {
            ElementKind::Ferrite => scalar_from_f32(0.35),
            ElementKind::Arborite => scalar_from_f32(0.2),
            ElementKind::Zephyrite => scalar_from_f32(0.65),
            ElementKind::Lumina => scalar_from_f32(0.5),
        }
    }

    pub fn power_profile(self) -> (Scalar, Scalar, Scalar) {
        match self {
            ElementKind::Ferrite => (
                scalar_from_f32(8.0),
                scalar_from_f32(6.0),
                scalar_from_f32(0.95),
            ),
            ElementKind::Arborite => (
                scalar_from_f32(4.0),
                scalar_from_f32(3.5),
                scalar_from_f32(1.05),
            ),
            ElementKind::Zephyrite => (
                scalar_from_f32(6.5),
                scalar_from_f32(4.0),
                scalar_from_f32(1.1),
            ),
            ElementKind::Lumina => (
                scalar_from_f32(10.0),
                scalar_from_f32(7.0),
                scalar_from_f32(0.9),
            ),
        }
    }

    pub fn from_grid(position: UVec2) -> Self {
        match (position.x + position.y) % 4 {
            0 => ElementKind::Ferrite,
            1 => ElementKind::Arborite,
            2 => ElementKind::Zephyrite,
            _ => ElementKind::Lumina,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(ElementKind::Ferrite),
            1 => Some(ElementKind::Arborite),
            2 => Some(ElementKind::Zephyrite),
            3 => Some(ElementKind::Lumina),
            _ => None,
        }
    }
}

impl From<ElementKind> for u8 {
    fn from(value: ElementKind) -> Self {
        value as u8
    }
}

/// Commodity key for a band's food larder. `"provisions"` is the reward name foraging, hunt, and
/// husbandry income deposit into the band's local `stores` — provisions left `FactionInventory`
/// entirely; kept as a stable constant.
pub const FOOD: &str = "provisions";

/// Commodity key for a band's **fodder** larder — the storable hay a fodder crop grows (Flora Roster
/// F3, `docs/plan_flora_roster.md` §5). A second key on the *same* [`LocalStore`] as [`FOOD`], so it
/// round-trips through the snapshot for free and the supply network can already balance it. Hay is
/// animal feed, not human food: a fodder Field credits this key, a pen that knows Foddering draws it,
/// and the two stores **never convert** — feeding a pen bread ([`FOOD`]) stays as lossy as ever.
pub const FODDER: &str = "fodder";

// **`TRADE_GOODS` is RETIRED** (arc #527). It was a third commodity key on this store — pelts, hides,
// ivory, "the tradeable half of every yield vector" — and every producer wrote it while **no consumer
// ever read it**: there was no `take(TRADE_GOODS)` anywhere in the workspace. Beside every one of
// those credits sat a `credit_material_yield` banking the *same* take's concrete hide, bone and fibre
// as [`MaterialBatch`]es, which is the real resource model. A flat scalar collapses exactly the
// distinction the crafting arc exists to preserve — a mammoth hide and a hare pelt are both `hide`
// and are not the same thing — so the duplicate went and the vector-valued account stayed.
//
// The three luxury crops that paid the scalar and nothing else (tobacco, tea, grapevine) carry
// materials of their own now; see `materials.json`'s `_comment_roster`.

/// **One pile of a material at one rating** — a quantity plus the exact reading it stands for.
///
/// The reading is the batch's **amount-weighted average** per axis, in the material's declared axis
/// order, and it is what crafting reads. **Never the band alone**: the band is what decides whether
/// two arrivals merge, and it is derived for display; storing only the band would make two `good`
/// hides interchangeable, which is the whole thing the characteristic vector exists to prevent.
///
/// See `docs/plan_crafting_and_materials.md` §1 → "Bands: categories on screen, exact numbers
/// underneath".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MaterialBatch {
    /// How much of the material this batch holds. Fixed-point, so a fractional per-turn arrival
    /// accumulates toward a whole unit instead of rounding away.
    pub amount: Scalar,
    /// The batch's **exact** amount-weighted-average reading per axis. See the type doc.
    pub characteristics: BTreeMap<String, f32>,
}

/// One batch's contribution to a withdrawal — what came out, and the reading it came out at.
///
/// A partial take leaves the source batch's readings untouched (an average does not move when a
/// uniform part of it is removed), so this reports the batch's reading verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialDraw {
    /// Which batch it came from.
    pub band: crate::materials_config::BandKey,
    /// How much came out of that batch.
    pub amount: Scalar,
    /// That batch's exact reading per axis.
    pub characteristics: BTreeMap<String, f32>,
}

/// A location-local store of goods held by a band (and, later, a populated tile or storage pit).
/// Keyed by commodity so the supply network can balance *any* good; a `BTreeMap` keeps iteration
/// deterministic for balancing and snapshotting. Quantities are fixed-point (`Scalar`) so small
/// per-turn flows accumulate without rounding to zero. An absent key reads as zero, and setting a
/// key to zero prunes it, so two stores with the same goods always compare equal.
///
/// # Materials are a SECOND map, and `goods` is untouched
///
/// Provisions, fodder and trade goods are interchangeable scalars: two units of grain are two units
/// of grain. **A material is not** — a mammoth hide and a hare pelt are both `hide` and are not the
/// same thing — so materials are held as [`MaterialBatch`]es keyed by their per-axis
/// [`crate::materials_config::BandKey`], and a single pooled average would silently drag the one
/// down to the other the moment they met.
///
/// **This store stores; it does not interpret.** Deriving a band from a reading needs the material's
/// axis list, so that lives on [`crate::materials_config::MaterialsConfig`] and the key arrives here
/// already resolved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalStore {
    goods: BTreeMap<String, Scalar>,
    /// `material id → band key → batch`. `BTreeMap` on both levels so the checkpoint and any
    /// published readout iterate in a stable order — the same reason `BandEquipment` is one.
    materials: BTreeMap<String, BTreeMap<crate::materials_config::BandKey, MaterialBatch>>,
}

impl LocalStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current quantity of `item` (zero if absent).
    pub fn get(&self, item: &str) -> Scalar {
        self.goods.get(item).copied().unwrap_or_else(scalar_zero)
    }

    /// Add `amount` (may be negative) to `item`, flooring the result at zero.
    pub fn add(&mut self, item: &str, amount: Scalar) {
        let updated = self.get(item) + amount;
        self.set(item, updated);
    }

    /// Set `item` to `amount` (floored at zero; a zero value prunes the key).
    pub fn set(&mut self, item: &str, amount: Scalar) {
        if amount > scalar_zero() {
            self.goods.insert(item.to_string(), amount);
        } else {
            self.goods.remove(item);
        }
    }

    /// Remove up to `amount` of `item`, returning how much was actually taken.
    pub fn take(&mut self, item: &str, amount: Scalar) -> Scalar {
        let taken = min(amount.max(scalar_zero()), self.get(item));
        self.add(item, -taken);
        taken
    }

    /// `(item, quantity)` pairs in deterministic (sorted-key) order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, Scalar)> {
        self.goods.iter().map(|(k, v)| (k.as_str(), *v))
    }

    /// **THE MERGE RULE.** Add `amount` of `material` at `band`, merging into the batch already
    /// standing at that key: the amounts add, and **each axis's reading becomes the amount-weighted
    /// average of the two**.
    ///
    /// Without the merge a band hunting deer for two hundred turns would hold two hundred piles of
    /// hide; without the *weighted average* the merged pile would claim one of the two arrivals'
    /// readings and quietly re-grade the other. A non-positive amount is a no-op, so a degenerate
    /// take cannot invent a batch with no material in it.
    pub fn deposit_material(
        &mut self,
        material: &str,
        band: crate::materials_config::BandKey,
        amount: Scalar,
        characteristics: &BTreeMap<String, f32>,
    ) {
        if amount <= scalar_zero() {
            return;
        }
        let batch = self
            .materials
            .entry(material.to_string())
            .or_default()
            .entry(band)
            .or_default();
        let existing = batch.amount;
        let total = existing + amount;
        // The blend weights, taken once: `total > 0` because `amount > 0` and `existing >= 0`.
        let (existing_share, arriving_share) = (
            existing.to_f32() / total.to_f32(),
            amount.to_f32() / total.to_f32(),
        );
        // Every axis EITHER side names, so an arrival can neither drop an axis the batch carries nor
        // silently ignore one it brings.
        let axes: BTreeSet<&String> = batch
            .characteristics
            .keys()
            .chain(characteristics.keys())
            .collect();
        let blended: BTreeMap<String, f32> = axes
            .into_iter()
            .map(|axis| {
                let held = batch.characteristics.get(axis).copied().unwrap_or_default();
                let arriving = characteristics.get(axis).copied().unwrap_or_default();
                (
                    axis.clone(),
                    held * existing_share + arriving * arriving_share,
                )
            })
            .collect();
        batch.amount = total;
        batch.characteristics = blended;
    }

    /// Every batch of `material`, in band-key order. Empty for a material the store holds none of.
    pub fn material_batches(
        &self,
        material: &str,
    ) -> impl Iterator<Item = (&crate::materials_config::BandKey, &MaterialBatch)> {
        self.materials
            .get(material)
            .into_iter()
            .flat_map(|batches| batches.iter())
    }

    /// **How much of `material` the store holds in total** — the sum over its batches, which is the
    /// shortfall readout's number. Never stored beside the batches: a cached total is a second
    /// statement of one fact.
    pub fn material_total(&self, material: &str) -> Scalar {
        self.material_batches(material)
            .fold(scalar_zero(), |total, (_, batch)| total + batch.amount)
    }

    /// Every material the store holds, in id order.
    pub fn materials(
        &self,
    ) -> impl Iterator<
        Item = (
            &str,
            &BTreeMap<crate::materials_config::BandKey, MaterialBatch>,
        ),
    > {
        self.materials
            .iter()
            .map(|(id, batches)| (id.as_str(), batches))
    }

    /// **Withdraw up to `amount` of `material`, WORST-FIRST on `axis`** — you spend the poor hide
    /// before the excellent one, which is the only ordering that does not silently burn the player's
    /// best stock on the first thing they make.
    ///
    /// Returns what came out of each batch, so the caller can resolve the drawn reading (the
    /// amount-weighted average of the draws) without the store having to know what a recipe is. A
    /// partial take leaves the batch's readings untouched; an emptied batch is pruned, so two stores
    /// holding the same materials always compare equal.
    ///
    /// A batch that does not carry `axis` sorts **last** — it cannot occur (a source's yield row is
    /// validated to name exactly the material's axes), and spending it last means a mis-named axis
    /// burns the cheap stock rather than the good.
    pub fn take_material(
        &mut self,
        material: &str,
        axis: &str,
        amount: Scalar,
    ) -> Vec<MaterialDraw> {
        let mut remaining = amount.max(scalar_zero());
        let mut drawn = Vec::new();
        if remaining <= scalar_zero() {
            return drawn;
        }
        let Some(batches) = self.materials.get_mut(material) else {
            return drawn;
        };
        let order = spend_order(batches, axis);
        for key in order {
            if remaining <= scalar_zero() {
                break;
            }
            let batch = batches.get_mut(&key).expect("key came from this map");
            let taken = min(remaining, batch.amount);
            if taken <= scalar_zero() {
                continue;
            }
            batch.amount -= taken;
            remaining -= taken;
            drawn.push(MaterialDraw {
                band: key.clone(),
                amount: taken,
                characteristics: batch.characteristics.clone(),
            });
            if batch.amount <= scalar_zero() {
                batches.remove(&key);
            }
        }
        if batches.is_empty() {
            self.materials.remove(material);
        }
        drawn
    }

    /// **Move every material batch out of this store and into `into`**, keeping each batch's exact
    /// readings — an expedition's homecoming, the material twin of the leftover pack.
    ///
    /// **Batch by batch rather than pooled**, for the same reason the supply network balances per
    /// rating: one averaged arrival would drag a mammoth hide down to a hare pelt on the walk home.
    /// The destination's ordinary merge rule then runs per batch, which is where merging belongs.
    pub fn drain_materials_into(&mut self, into: &mut LocalStore) {
        let moving = std::mem::take(&mut self.materials);
        for (material, batches) in moving {
            for (band, batch) in batches {
                into.deposit_material(&material, band, batch.amount, &batch.characteristics);
            }
        }
    }

    /// **Peel `amount` of `material` off the store IN ITS OWN ORDER, splitting the last batch** —
    /// the shipper's draw, and the twin of [`Self::drain_materials_into`] for a *partial* move.
    ///
    /// It is deliberately **not** [`Self::take_material`]: that one sorts worst-first on a named
    /// **axis**, which is the crafting bench's question (*"spend the poor hide before the good
    /// one"*). A shipment names no axis — a trader says *"four hide"*, not *"four hide by
    /// suppleness"* — so the order here is the store's own band-key order, which is deterministic by
    /// construction ([`BTreeMap`]) and the same order the checkpoint and every readout already walk.
    ///
    /// **A SPLIT IS NOT A MERGE.** A batch is a quantity of one identical material, so half of it is
    /// still that material at exactly that rating: each draw carries the source batch's readings
    /// verbatim, and the amounts a caller re-deposits keep every rating that left. Averaging enters
    /// only where two *different* batches meet, which is [`Self::deposit_material`]'s job.
    ///
    /// Returns what actually came out, which is short of `amount` when the store is short — the
    /// availability question is the caller's, asked with [`Self::material_total`].
    pub fn take_material_batches(&mut self, material: &str, amount: Scalar) -> Vec<MaterialDraw> {
        let mut remaining = amount.max(scalar_zero());
        let mut drawn = Vec::new();
        if remaining <= scalar_zero() {
            return drawn;
        }
        let Some(batches) = self.materials.get_mut(material) else {
            return drawn;
        };
        let order: Vec<crate::materials_config::BandKey> = batches.keys().cloned().collect();
        for key in order {
            if remaining <= scalar_zero() {
                break;
            }
            let batch = batches.get_mut(&key).expect("key came from this map");
            let taken = min(remaining, batch.amount);
            if taken <= scalar_zero() {
                continue;
            }
            batch.amount -= taken;
            remaining -= taken;
            drawn.push(MaterialDraw {
                band: key.clone(),
                amount: taken,
                characteristics: batch.characteristics.clone(),
            });
            if batch.amount <= scalar_zero() {
                batches.remove(&key);
            }
        }
        if batches.is_empty() {
            self.materials.remove(material);
        }
        drawn
    }

    /// **Withdraw from ONE named batch** — the supply network's move, which knows exactly which
    /// rating it is shipping and must not re-sort by anything. Returns what was actually taken.
    pub fn take_material_batch(
        &mut self,
        material: &str,
        band: &crate::materials_config::BandKey,
        amount: Scalar,
    ) -> Scalar {
        let Some(batches) = self.materials.get_mut(material) else {
            return scalar_zero();
        };
        let Some(batch) = batches.get_mut(band) else {
            return scalar_zero();
        };
        let taken = min(amount.max(scalar_zero()), batch.amount);
        batch.amount -= taken;
        if batch.amount <= scalar_zero() {
            batches.remove(band);
        }
        if batches.is_empty() {
            self.materials.remove(material);
        }
        taken
    }

    /// **What [`Self::take_material`] WOULD draw, without drawing it** — the readout's twin.
    ///
    /// The published craft offer has to state the grade a pass would come out at, and the grade is a
    /// function of *which piles the draw would spend*: worst-first on the read axis, so a band with
    /// one poor hide and one excellent one makes the poor sled first. Re-deriving that client-side is
    /// impossible (the ordering is the store's) and re-deriving it here with a second walk is how the
    /// preview and the draw come to disagree — so both run [`spend_order`].
    ///
    /// A short store previews what it *has*; the availability test is a separate question the caller
    /// asks with [`Self::material_total`].
    pub fn preview_take_material(
        &self,
        material: &str,
        axis: &str,
        amount: Scalar,
    ) -> Vec<MaterialDraw> {
        let mut remaining = amount.max(scalar_zero());
        let mut drawn = Vec::new();
        let Some(batches) = self.materials.get(material) else {
            return drawn;
        };
        for key in spend_order(batches, axis) {
            if remaining <= scalar_zero() {
                break;
            }
            let batch = &batches[&key];
            let taken = min(remaining, batch.amount);
            if taken <= scalar_zero() {
                continue;
            }
            remaining -= taken;
            drawn.push(MaterialDraw {
                band: key,
                amount: taken,
                characteristics: batch.characteristics.clone(),
            });
        }
        drawn
    }
}

/// **Worst-first on `axis`, ties broken by the band key** — the one spend order, shared by
/// [`LocalStore::take_material`] and [`LocalStore::preview_take_material`] so a published grade
/// cannot disagree with the grade the bench then fixes.
///
/// A batch that does not carry `axis` sorts **last**: it cannot occur (a yield row is validated to
/// name exactly the material's axes), and spending it last means a mis-named axis burns the cheap
/// stock rather than the good.
fn spend_order(
    batches: &BTreeMap<crate::materials_config::BandKey, MaterialBatch>,
    axis: &str,
) -> Vec<crate::materials_config::BandKey> {
    let mut order: Vec<crate::materials_config::BandKey> = batches.keys().cloned().collect();
    order.sort_by(|a, b| {
        let reading = |key: &crate::materials_config::BandKey| {
            batches[key]
                .characteristics
                .get(axis)
                .copied()
                .unwrap_or(f32::INFINITY)
        };
        reading(a)
            .partial_cmp(&reading(b))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(b))
    });
    order
}

/// Population representation bound to a home tile.
#[derive(Component, Debug, Clone)]
pub struct PopulationCohort {
    pub home: Entity,
    /// Current position during travel (equals home when stationary).
    pub current_tile: Entity,
    /// Cached total head-count (`= round(children + working + elders)`), kept in sync by
    /// `simulate_population` so the many `.size` readers stay valid.
    pub size: u32,
    /// Dependents — fed and housed, no labor. Fractional (fixed-point) so small per-turn flows
    /// accumulate without rounding to zero on a small band.
    pub children: Scalar,
    /// Working-age — the labor pool (the only bracket that produces).
    pub working: Scalar,
    /// Elders — dependents again, then mortality.
    pub elders: Scalar,
    /// The band's carried goods store (food under the `FOOD` key, plus any future commodity).
    /// Filled by this band's foraging, drawn down per-capita each turn, and rebalanced with nearby
    /// bands by the supply network. Local from day one — the same store a settlement/storage-pit
    /// will hold later at larger scale (`docs/plan_settlement_population.md`).
    pub stores: LocalStore,
    pub morale: Scalar,
    /// The food the band's people **actually ate** this turn (`min(food_demand, larder)` at the
    /// turn's *opening* brackets — the real `stores` debit `advance_demographics` took, before the
    /// same turn's births/aging change the head-count). This — not a re-derived `food_demand` on the
    /// *post*-turn brackets — is the consumption term of the larder ledger identity
    /// `larder_delta == food_income − food_consumption − pen_feed_upkeep`, so it holds by
    /// construction whether the band is fully fed or starving (the debit symmetry of
    /// `LaborAllocation::last_pen_feed_upkeep`, the food the pen actually paid). Recomputed each turn
    /// by `simulate_population`; on the client wire as `PopulationCohortState.food_consumption`.
    pub last_food_consumption: f32,
    /// **Food this band RECEIVED from another band, as of this turn's frame** — the per-turn twin of
    /// the accumulator [`LaborAllocation::last_transfer_received`], and the reading a client renders.
    ///
    /// Copied off that accumulator by `systems::publish_turn_transfers`, in the Snapshot stage
    /// immediately before the turn's `capture_snapshot`. **The copy exists because the accumulator
    /// resets and this does not.** `systems::reset_transfer_ledger` clears the accumulator *after*
    /// the capture has read it, so a **recapture** — `snapshot::recapture_snapshot_in_place`, which
    /// re-runs the capture against live components after every dispatched command — would republish
    /// the band with the counter already zeroed and blank the row it had just shown. The four
    /// sibling ledger terms (`food_income`, [`Self::last_food_consumption`], `pen_feed_upkeep`,
    /// `raid_forfeit`) are per-turn values re-read unchanged on a recapture, and this pair is what
    /// joins them in that.
    ///
    /// **It neither replaces the accumulator nor changes its window.** At the moment it is copied
    /// the accumulator holds *(command-time draws since the last turn capture) + (this turn's
    /// transfers)* — exactly the interval the ledger identity
    /// `larder_delta == income − consumption − pen_feed − raid_forfeit + received − sent` measures —
    /// so the two readings cannot disagree on a turn frame. On the wire as
    /// `PopulationCohortState.transfer_received_turn`, beside the accumulator's own
    /// `transfer_received`.
    pub last_turn_transfer_received: f32,
    /// **Food this band GAVE UP to another band, as of this turn's frame** — the sent half of
    /// [`Self::last_turn_transfer_received`], on the same copy and for the same reason.
    pub last_turn_transfer_sent: f32,
    /// This turn's signed morale delta (before clamping into `[0, 1]`). Recomputed each turn by
    /// `simulate_population`; on the client wire as `PopulationCohortState.morale_delta`, which the
    /// client renders as a rising/falling trend arrow.
    pub last_morale_delta: Scalar,
    /// The dominant *negative* driver behind `last_morale_delta` when morale fell this turn
    /// (`None` when it rose or held), so the client can name *why* — e.g. "harsh terrain".
    /// Recomputed each turn alongside `last_morale_delta`; on the client wire as
    /// `PopulationCohortState.morale_cause`.
    pub last_morale_cause: MoraleCause,
    /// The Layer-1 named morale contributors whose signed sum IS `last_morale_delta` (the wellbeing
    /// model's per-band morale breakdown — see `docs/plan_civ_wellbeing.md`). Recomputed each turn by
    /// `simulate_population`; on the client wire as `PopulationCohortState.morale_{settling,terrain,
    /// climate,unrest}`.
    pub last_morale_contributions: MoraleContributions,
    /// The three named fertility factors behind this turn's births — `hunger` (did we eat) ×
    /// `reserve` (is there a cushion) × `trend` (is the cushion growing or shrinking), the
    /// `birth_rate` multiplier from `docs/plan_population_growth_model.md`. The birth path's
    /// equivalent of `last_morale_contributions`: growth slows for named reasons, and this is what
    /// lets the client itemize them instead of leaving the player with only the inputs (larder,
    /// Food /turn) and the effect (population). Recomputed each turn by `simulate_population`; on the
    /// client wire as `PopulationCohortState.fertility_{hunger,reserve,trend}`. A cohort that has not
    /// yet been through a turn carries the all-zero **not-projected** default (see
    /// [`FertilityFactors`]).
    pub last_fertility_factors: FertilityFactors,
    /// Layer 2 — the share of the band that is unhappy this turn, `g(morale)` (working-weighted at
    /// the migration/grievance stage). `0` = content, `1` = fully discontented. Drives the
    /// productivity modifier stack and migration. Recomputed each turn by `simulate_population`; on
    /// the client wire as `PopulationCohortState.discontent_fraction`.
    pub discontent_fraction: Scalar,
    /// Layer 2 — the severity × duration grievance accumulator: rises with sustained discontent
    /// (faster when trapped with nowhere to migrate), decays while content. Phase 1 only populates
    /// it (reserved for a future revolution consequence — no consequence reads it yet). Accumulated
    /// by `advance_population_migration`; on the client wire as `PopulationCohortState.grievance`.
    pub grievance: Scalar,
    /// How many people emigrated **from** this band last turn via discontent-driven migration
    /// (relocated to a happier same-faction band). `0` = none. Recomputed each turn by
    /// `advance_population_migration`; on the client wire as `PopulationCohortState.last_emigrated`.
    pub last_emigrated: u32,
    /// How many people immigrated **into** this band last turn (a high-morale band is a magnet).
    /// `0` = none. Recomputed each turn by `advance_population_migration`; on the client wire as
    /// `PopulationCohortState.last_immigrated`.
    pub last_immigrated: u32,
    /// Turns this band has been simulated. Gates knowledge-migration (`simulate_population`) so a
    /// freshly-spawned band must settle for `migration_min_settled_turns` before its population can
    /// emigrate to a neighbor. Incremented each turn by `simulate_population`; on the client wire as
    /// `PopulationCohortState.age_turns`.
    pub age_turns: u32,
    pub generation: GenerationId,
    pub faction: FactionId,
    pub knowledge: Vec<KnowledgeFragment>,
    pub migration: Option<PendingMigration>,
}

/// The dominant negative driver of a cohort's morale on a given turn, surfaced so the client can
/// name *why* morale (and thus population) is falling instead of reporting a vague "low morale".
/// Starvation is deliberately excluded — it is surfaced through the days-of-food path, not morale.
///
/// Snapshot wire encoding (see [`MoraleCause::as_u8`]): `0 = None, 1 = Terrain, 2 = Cold,
/// 3 = Unrest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MoraleCause {
    /// Morale rose or held this turn — no dominant negative driver.
    #[default]
    None,
    /// Terrain attrition + logistics hardness dominated — the hex is harsh to live on.
    Terrain,
    /// The temperature-difference penalty dominated.
    Cold,
    /// Crisis impacts + cultural sentiment (unrest) dominated.
    Unrest,
}

/// Which mortality term did most of the killing in one age bracket on one turn.
///
/// The demographic model kills through a starvation term (scaled by the food deficit and the
/// bracket's vulnerability), a uniform cold term, and — for elders only — the flat
/// `elder_mortality_rate` of simply growing old. Once a death is *reported*, the turn that
/// produced it is gone — post-turn brackets and a refilled larder cannot say what emptied them —
/// so the cause is recorded when the deaths accrue and carried on
/// [`DemographicFlowAccumulator`] until the whole-person event fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeathCause {
    /// The food deficit dominated (or the terms tied — a starving band is the louder reading).
    #[default]
    Hunger,
    /// The cold term dominated.
    Cold,
    /// Old age dominated — the `elder_mortality_rate` term, which only the elder bracket carries.
    /// A band with a full larder in fair weather still buries its elders, and reporting that as
    /// `Hunger` would tell the player a falsehood about their food every few turns.
    Age,
}

impl DeathCause {
    /// The `cause=` token on a `died` event's detail string. One word, lowercase, stable — the
    /// client keys off it, so it is a wire contract and not prose.
    pub fn as_str(self) -> &'static str {
        match self {
            DeathCause::Hunger => "hunger",
            DeathCause::Cold => "cold",
            DeathCause::Age => "age",
        }
    }

    /// The phrase inside "died of …" in the event's **label**, which is prose and free to read
    /// better than the token: "died of old age", not "died of age".
    pub fn label_phrase(self) -> &'static str {
        match self {
            DeathCause::Hunger => "hunger",
            DeathCause::Cold => "cold",
            DeathCause::Age => "old age",
        }
    }
}

/// The fractional carry that turns a demographic **rate** into whole-person **events**.
///
/// `births = working × fertility` is a `Scalar`: a band of thirty earns a fraction of a birth per
/// turn. Rounding that per turn either invents a birth in a band too small to have had one, or
/// reports none for the whole game. So each flow accrues here, and an event fires only when the
/// carry crosses a whole person — the remainder staying put is what makes a small band's births
/// *late* rather than *absent*.
///
/// One per band, alongside the cohort. Checkpointed with it (`sim_state::BandRecord`): the carry is
/// real state, and a rollback that dropped it would re-time every event after the restore.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct DemographicFlowAccumulator {
    /// Fractional births not yet reported.
    pub births: Scalar,
    /// Fractional children→working transitions not yet reported.
    pub maturations: Scalar,
    /// Fractional working→elder transitions not yet reported. A bracket **move**, not a loss — the
    /// band's head-count is unchanged — but it is the other end of the same working life, and a
    /// workforce that shrinks with the feed silent about why is the same bug `maturations` exists
    /// to prevent at the young end.
    pub agings: Scalar,
    /// Fractional deaths not yet reported — **one carry for all three brackets**, so every whole
    /// person the band loses is announced exactly once however the loss was spread.
    ///
    /// Three per-bracket carries is the intuitive shape and it is wrong: each one keeps its own
    /// sub-person remainder, and **a remainder is stranded the moment its flow stops**. A cold snap
    /// that kills 0.4 of a bracket and then ends leaves 0.4 of a person carried forever, with
    /// nothing further accruing to push it over — three brackets, three permanent leaks. Pooled,
    /// any later death from any bracket pays the remainder off.
    pub deaths: Scalar,
    /// Each bracket's share of the deaths accrued **since the last crossing**. Pure labelling: the
    /// largest contributor names the row (`bracket=`) and supplies its `cause=`. Reset when a
    /// crossing reports, so the label describes the deaths that event is actually announcing.
    pub child_death_contribution: Scalar,
    pub working_death_contribution: Scalar,
    pub elder_death_contribution: Scalar,
    /// The cause recorded on the last turn that killed anyone in each bracket — read at the
    /// crossing, never re-derived afterwards.
    pub child_death_cause: DeathCause,
    pub working_death_cause: DeathCause,
    pub elder_death_cause: DeathCause,
}

impl DemographicFlowAccumulator {
    /// Add one turn's fractional `flow` to `carry` and hand back the **whole people** it now owes,
    /// leaving the remainder in place.
    ///
    /// This is the whole honesty of the model in three lines: the returned count is `floor` of the
    /// carry, and what is subtracted is exactly that count — so nothing is invented and nothing is
    /// lost, however many turns a crossing takes.
    pub fn accrue(carry: &mut Scalar, flow: Scalar) -> u32 {
        *carry += flow;
        let whole = carry.raw().div_euclid(Scalar::SCALE);
        if whole <= 0 {
            return 0;
        }
        *carry -= Scalar::from_i64(whole);
        whole as u32
    }
}

impl MoraleCause {
    /// Encode for the snapshot's `moraleCause:ubyte` field: `0=None, 1=Terrain, 2=Cold, 3=Unrest`.
    pub fn as_u8(self) -> u8 {
        match self {
            MoraleCause::None => 0,
            MoraleCause::Terrain => 1,
            MoraleCause::Cold => 2,
            MoraleCause::Unrest => 3,
        }
    }
}

/// Layer 1 of the Civilization Wellbeing model (`docs/plan_civ_wellbeing.md`): the named factors
/// that converge into a band's morale. Morale trends by the **signed sum** of the active
/// contributions each turn; adding a future factor (nutrition/education/technology/government/…)
/// is a new variant plus one contribution — the morale update itself never gets rewritten. The
/// contribution set *is* the per-band morale breakdown the client can itemize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoraleFactor {
    /// Base settling growth (`+population_growth_rate`) — always non-negative.
    Settling,
    /// Terrain attrition + logistics hardness drain (≤ 0).
    Terrain,
    /// Temperature-vs-tolerance climate drain (≤ 0).
    Climate,
    /// Crisis impacts + cultural sentiment (signed).
    Unrest,
}

/// The Phase-1 named morale contributions for a cohort this turn (each signed; their sum IS
/// `last_morale_delta`). A fixed struct rather than a `Vec` to stay allocation-free; a future
/// factor adds a field + a `MoraleFactor` variant. Surfaced in the snapshot so the client can
/// itemize *why* morale is moving.
#[derive(Debug, Clone, Copy, Default)]
pub struct MoraleContributions {
    /// `+population_growth_rate` (base settling growth).
    pub settling: Scalar,
    /// `−terrain pressure` (≤ 0).
    pub terrain: Scalar,
    /// `−climate/cold pressure` (≤ 0).
    pub climate: Scalar,
    /// crisis impacts + cultural sentiment bias (signed).
    pub unrest: Scalar,
}

/// The three named fertility factors behind a cohort's births this turn — the `birth_rate`
/// multiplier `hunger × reserve × trend` (`docs/plan_population_growth_model.md`). It lives here
/// beside [`MoraleContributions`] and for the same reason: growth, like morale, moves for named
/// reasons, and a fixed allocation-free struct makes a future driver one more field rather than a
/// rewrite of the birth path. Surfaced in the snapshot so the client can itemize *why* growth is
/// slow.
///
/// **`Default` is all-zero, and that is the NOT-PROJECTED sentinel** — what a cohort carries until
/// its first `simulate_population` tick writes a real reading.
/// It is unambiguous because a *computed* set can never carry `reserve == 0`: `reserve` is
/// `1 + bonus × ramp` with both terms ≥ 0, so it is ≥ 1 by construction. `hunger` and `trend` both
/// legitimately reach 0 (an empty larder, a `deficit_penalty` of 1.0), so neither could serve.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FertilityFactors {
    /// **Gate** — did the band eat this turn (`consumed / demand`).
    pub hunger: Scalar,
    /// **Stock** — how deep is the larder.
    pub reserve: Scalar,
    /// **Flow** — is the larder growing or shrinking.
    pub trend: Scalar,
}

impl FertilityFactors {
    /// The `birth_rate` multiplier: the product of the three factors.
    ///
    /// Only `hunger` can reach 0 — it is the gate that makes an empty larder yield zero births.
    /// `reserve` and `trend` are modifiers bracketing 1.0 (`[1, 1.5]` and `[0.25, 1.25]` at shipped
    /// defaults), so neither can zero the product alone and the stack needs no floor lever; how far
    /// a collapsed income may damp growth is `trend.deficit_penalty`'s job.
    pub fn multiplier(&self) -> Scalar {
        self.hunger * self.reserve * self.trend
    }
}

impl MoraleContributions {
    /// The active contributions as `(factor, signed value)` pairs — the itemized breakdown the
    /// client can render and the single source both `total` and cause attribution iterate. Ordered
    /// by the historical tie-break priority (Terrain ≥ Climate ≥ Unrest) so the dominant-cause scan
    /// is a stable first-max.
    pub fn contributions(&self) -> [(MoraleFactor, Scalar); 4] {
        [
            (MoraleFactor::Terrain, self.terrain),
            (MoraleFactor::Climate, self.climate),
            (MoraleFactor::Unrest, self.unrest),
            (MoraleFactor::Settling, self.settling),
        ]
    }

    /// The signed morale delta this turn — the sum of every contribution.
    pub fn total(&self) -> Scalar {
        self.contributions()
            .iter()
            .fold(scalar_zero(), |acc, (_, value)| acc + *value)
    }

    /// The dominant *negative* contributor as a [`MoraleCause`] (the "why morale fell" label). The
    /// most-negative labeled contribution wins; `Settling` is base growth (never a negative cause),
    /// and ties resolve by `contributions()` order (Terrain ≥ Climate ≥ Unrest).
    pub fn dominant_negative_cause(&self) -> MoraleCause {
        let mut best: Option<(MoraleFactor, Scalar)> = None;
        for (factor, value) in self.contributions() {
            if matches!(factor, MoraleFactor::Settling) || value >= scalar_zero() {
                continue;
            }
            if best.is_none_or(|(_, worst)| value < worst) {
                best = Some((factor, value));
            }
        }
        match best {
            Some((MoraleFactor::Terrain, _)) => MoraleCause::Terrain,
            Some((MoraleFactor::Climate, _)) => MoraleCause::Cold,
            Some((MoraleFactor::Unrest, _)) => MoraleCause::Unrest,
            _ => MoraleCause::None,
        }
    }
}

impl PopulationCohort {
    /// Fixed-point sum of the three age brackets (the authoritative head-count; `size` is its
    /// rounded `u32` cache).
    pub fn total(&self) -> Scalar {
        self.children + self.working + self.elders
    }

    /// Split a head-count into the three brackets by the configured fractions and resync `size`.
    /// Used when spawning a fresh cohort (rehydration restores exact brackets from the snapshot).
    pub fn set_brackets_from_size(&mut self, size: u32, children: f32, working: f32, elders: f32) {
        let total = Scalar::from_u32(size);
        self.children = total * scalar_from_f32(children);
        self.working = total * scalar_from_f32(working);
        self.elders = total * scalar_from_f32(elders);
        self.size = self.total().to_u32();
    }

    /// Recompute the `size` cache from the current brackets.
    pub fn sync_size(&mut self) {
        self.size = self.total().to_u32();
    }

    /// **Remove combat dead from the working-age bracket** (Predators Phase 0 — the new casualty
    /// mortality path, `docs/plan_predators.md`). Hunters and warriors are working-age, so a dangerous
    /// hunt's `killed` come out of `working` (floored at 0), and `size` is resynced. This is the
    /// `death_fraction` seam's combat twin — a net-new way people die, beside starvation, cold and
    /// elder mortality. Casualties are working-age only in Phase 0.
    pub fn apply_combat_casualties(&mut self, killed: Scalar) {
        if killed <= scalar_zero() {
            return;
        }
        self.working = (self.working - killed).max(scalar_zero());
        self.sync_size();
    }
}

/// Power node metadata bound to a tile entity.
#[derive(Component, Debug, Clone)]
pub struct PowerNode {
    pub id: PowerNodeId,
    pub base_generation: Scalar,
    pub base_demand: Scalar,
    pub generation: Scalar,
    pub demand: Scalar,
    pub efficiency: Scalar,
    pub storage_capacity: Scalar,
    pub storage_level: Scalar,
    pub stability: Scalar,
    pub surplus: Scalar,
    pub deficit: Scalar,
    pub incident_count: u32,
}

/// Marks a starting population cohort spawned from a scenario profile.
#[derive(Component, Debug, Clone)]
pub struct StartingUnit {
    pub kind: String,
    pub tags: Vec<String>,
}

impl StartingUnit {
    pub fn new(kind: String, tags: Vec<String>) -> Self {
        Self { kind, tags }
    }
}

/// Positive marker for a **real** band — one that participates in the population/settlement arc
/// (demographics, migration, sedentarization, startup seeding, supply networks, default-band
/// command pickers). Attached to every band spawned by worldgen. A detached [`Expedition`]
/// deliberately **lacks** this marker, so it is excluded from those systems *by construction* — the
/// safe default survives new systems added to the settlement arc.
///
/// **It is a membership switch, not a label.** Gaining it puts a band into every one of those
/// systems at once, which is what makes band fission cheap: `split_band` divides a resident band in
/// two on the tile it stands on, and the half that leaves is spawned carrying this marker from its
/// first turn ([`crate::systems::split_band_from_parent`], `docs/plan_band_fission.md`). The other
/// two places it is inserted are worldgen and checkpoint restore.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ResidentBand;

/// A band's durable identity, stable across a checkpoint restore.
///
/// **`Entity` is not an identity.** Restoring a checkpoint despawns and respawns every cohort, and
/// bevy hands back a fresh generation, so `Entity::to_bits()` names a different band before and
/// after — which is why anything that stored one (an expedition's `home_band`, the visibility
/// sweep tracker's previous positions) could not survive a rollback. Every other durable thing in
/// the sim already has an id of its own — [`PowerNodeId`], `InfluentialId`, `GreatDiscoveryId` —
/// and a band is the one that did not.
///
/// Bands are the only entity class with **no natural key**: tiles are `(x, y)`, logistics links are
/// their endpoint pair, power nodes are `y * width + x`, but a band splits, merges, migrates and
/// dies, and several can stand on one hex. So the id is explicit and allocated from
/// [`crate::resources::BandIdAllocator`], which is itself checkpoint state — restoring the counter
/// is what stops a replay from re-issuing an id that is already in use.
///
/// It is also the key a band's **culture layer** is filed under
/// (`CultureOwner::from_band`) — which only works because the id survives a restore; an entity-bit
/// key would orphan every band's culture on a rollback, exactly as it did for tiles.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BandId(pub u64);

/// What an expedition was sent to do: `Scout` (explore + report the map, PR 1) or `Hunt` (follow a
/// migratory herd, harvest food, deliver it, PR 2) — two verbs on one traveling-party system.
// `Eq` is deliberately absent: the mission carries an `f32` floor, and float equality is not an
// equivalence relation. Nothing compares missions for identity — `same_source` keys on the herd id.
#[derive(Debug, Clone, PartialEq)]
pub enum ExpeditionMission {
    /// Explore toward a target and report the map + any Wondrous Sites it uncovers.
    Scout,
    /// Follow the herd `fauna_id`, harvest a **productive** hunt's worth of food each turn into the
    /// party's larder, and deliver it back to the band. `fauna_id` keys `HerdRegistry::find`.
    Hunt {
        fauna_id: String,
        /// **The quarry's species display name, resolved ONCE at launch** — what the client names
        /// this party's target on screen (`Red Deer`), never the `fauna_id` beside it.
        ///
        /// **It is carried rather than looked up because the herd is the one thing a party outlives**
        /// (issue #378). Herd telemetry is fog-filtered to tiles the viewer can see *right now* and
        /// pruned at local extinction, and a detached party is deliberately **not** a vision source
        /// ([`crate::visibility_systems::calculate_visibility`], `Without<Expedition>`) — so a
        /// party's own quarry routinely leaves the published herd list while the party is still bound
        /// to it. A client joining `fauna_id` against that list had nothing left to join against and
        /// fell back to rendering the raw id.
        ///
        /// **Launch is the moment the name is reliable**: the herd is in [`crate::fauna::HerdRegistry`]
        /// by construction there (the command resolved it to forecast the trip), and it can never be
        /// again once the herd is gone. Resolving at capture time instead would have survived fog and
        /// still gone blank on extinction, which prunes the registry itself.
        target_species: String,
        /// **WHERE THE RAID STOPS, as a fraction of the herd's `K`** — chosen at launch, and the
        /// whole of what the party's orders say about pressure (`docs/plan_harvest_floor.md` §1).
        /// The raid takes the stock standing above it as fast as it can carry it, then comes home;
        /// the floor therefore governs both the take and the trip's shape
        /// ([`crate::systems::raid_is_recurring`]).
        ///
        /// **Floor `0` takes everything** — nothing is left standing, the herd falls under
        /// `extinction_floor`, and the party banks the whole-stock windfall on the way (an end
        /// state, not an empty pack). That reading is a *consequence* of the number here rather than
        /// a mission kind.
        ///
        /// **It is maximal *harvest*, and that is not denial** (`docs/plan_denial_raid.md` §0): the
        /// take is still bounded by what the party can **carry**, so erasing a herd this way is as
        /// slow and as crew-hungry as eating it. Denial is a mission of its own with the carry bound
        /// removed, at which point this field means only "how deep a harvest".
        ///
        /// **The floor is the ONLY number a hunt carries.** A party-side `fill_target` ("take ≈50
        /// and come home") shipped beside it and was retired — see
        /// `docs/plan_hunt_through_combat.md` §5.2, marked retired in place.
        floor: f32,
    },
    /// **Erase the herd `fauna_id`** — the denial raid (`docs/plan_denial_raid.md` §1). The party
    /// works the herd until it is past the point of no return
    /// ([`crate::fauna::herd_past_recovery`]) and then walks away.
    ///
    /// **It carries no floor and no rate, and that is the whole reason it is a mission** rather than
    /// a number on the assign dialog. There is nothing to tune: you choose a herd and a party size.
    /// [`Self::hunt_floor`] reports [`STRIP_IT_BARE`] for it — the escapement ceiling is the herd's
    /// whole standing stock — and `floor` never appears in its command text or its UI.
    ///
    /// **One line of behaviour differs from a hunt** ([`Self::engagement_stop`]): a hunting party
    /// stops engaging once its pack is full, a denial party never stops. `carried` keeps the hunt's
    /// formula exactly, so the raid still banks whatever it can haul on the way home — a rounding
    /// error against what it killed, which is the point. Everything else is reused unchanged:
    /// [`ExpeditionPhase`], party outfitting, travel, and
    /// [`crate::fauna::AnimalTake`], which already models kill ≠ carry.
    ///
    /// **No target faction** (§2). Denial is aimed at a herd, not at a player.
    Deny {
        fauna_id: String,
        /// The quarry's species display name, resolved once at launch — see
        /// [`ExpeditionMission::Hunt::target_species`]. A denial raid needs it *more* than a hunt
        /// does: the mission's whole purpose is to drive the herd past recovery, so its target is
        /// pruned from the herd list by the raid succeeding.
        target_species: String,
    },
    /// **Carry a shipment to another band** — the first rider on the connection primitive
    /// (`docs/plan_contact_and_logistics.md` §Q5, arc #527, slice #517).
    ///
    /// **A shipment is a party that walks it.** There is deliberately no persistent link component
    /// underneath: what maintains a link is a *route*, the route ladder holds that state, and
    /// building link state before any route exists to hold it would be inventing the ladder's model
    /// in advance. So the rider is an expedition verb, and the cargo is [`Expedition::cargo`].
    ///
    /// **It is one-way.** The party carries goods out, deposits them, and walks home empty; a priced
    /// return flow is a later slice, not an omission here.
    ///
    /// **There is no faction on it, and no same-faction branch anywhere it is read.** Faction is a
    /// property of the endpoint (`.claude/rules/core_sim/connections.md`), so a shipment to another
    /// people works by construction rather than by a clause.
    Trade {
        /// **The destination band's durable id** — the key, never rendered. A [`BandId`] rather than
        /// an `Entity` for the reason every other durable handle in this file is one: the band
        /// outlives any entity index, and the party must still name it after a rollback.
        destination_band: BandId,
        /// **The destination's display name, resolved ONCE at launch — and EMPTY today, because
        /// bands have no names in this game.**
        ///
        /// The field exists for the reason [`ExpeditionMission::Hunt::target_species`] does: the
        /// party outlives its target's presence in the viewer's world, so a name that can only be
        /// resolved at launch has to be *carried*. The moment a second faction lands (#513) a
        /// foreign band's name must come from here, because the client has no roster to resolve one
        /// from.
        ///
        /// **It is empty rather than guessed.** It was briefly filled from `StartingUnit.kind`,
        /// which is the unit *archetype* (`"BandForager"`) and not a name at all: every band in the
        /// game published the same string, and it disagreed with the positional label
        /// ("Band 2") the rest of the HUD uses for the same band. Empty means **"no name"** — the
        /// same *"empty is no row, never a zero"* contract this arc's material readouts use — and a
        /// client falls back to whatever it calls that band everywhere else. Inventing a naming
        /// scheme to fill it is a separate piece of design, not a field default.
        destination_name: String,
    },
}

/// **The orders a party works a herd under** — what [`ExpeditionMission::Hunt`] and
/// [`ExpeditionMission::Deny`] have in common, resolved once so the `Hunting` phase arm and the
/// forecasts branch on data rather than re-matching the mission at every seam.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaidOrders<'a> {
    /// The herd the party named — keys `HerdRegistry::find`.
    pub fauna_id: &'a str,
    /// Where the raid stops, as a fraction of `K`. [`STRIP_IT_BARE`] for a denial raid.
    pub floor: f32,
    /// **The one line that differs** — whether a full pack stops the party engaging.
    pub stop: crate::fauna::EngagementStop,
}

impl ExpeditionMission {
    /// Stable wire/snapshot key for the mission (client discriminator).
    pub fn as_str(&self) -> &'static str {
        match self {
            ExpeditionMission::Scout => "scout",
            ExpeditionMission::Hunt { .. } => "hunt",
            ExpeditionMission::Deny { .. } => "deny",
            ExpeditionMission::Trade { .. } => "trade",
        }
    }

    /// Parse a mission from its wire keys (snapshot restore). `"hunt"` reconstructs
    /// `Hunt { fauna_id, target_species, floor }` from `target_herd` + `target_species` + `floor`;
    /// `"deny"` reconstructs `Deny { fauna_id, target_species }` from the two strings alone — it
    /// carries no number; `"trade"` reconstructs `Trade { destination_band, destination_name }` from
    /// the destination pair, which shares nothing with the herd pair (a shipment names a *people*);
    /// anything else is `Scout`.
    pub fn from_wire(
        kind: &str,
        target_herd: &str,
        target_species: &str,
        floor: f32,
        destination_band: u64,
        destination_name: &str,
    ) -> Self {
        match kind {
            "hunt" => ExpeditionMission::Hunt {
                fauna_id: target_herd.to_string(),
                target_species: target_species.to_string(),
                floor,
            },
            "deny" => ExpeditionMission::Deny {
                fauna_id: target_herd.to_string(),
                target_species: target_species.to_string(),
            },
            "trade" => ExpeditionMission::Trade {
                destination_band: BandId(destination_band),
                destination_name: destination_name.to_string(),
            },
            _ => ExpeditionMission::Scout,
        }
    }

    /// **The band a shipment is bound for**, for the one mission that names one — `None` for every
    /// other verb. The key the command addresses and the connection gate is keyed on; its display
    /// twin is [`Self::destination_display`].
    pub fn destination_band(&self) -> Option<BandId> {
        match self {
            ExpeditionMission::Trade {
                destination_band, ..
            } => Some(*destination_band),
            _ => None,
        }
    }

    /// **The destination's name as it will be PUBLISHED** — [`Self::destination_band`]'s display
    /// twin, and empty when there is none. `""` for every non-`Trade` mission.
    ///
    /// **This is what crosses the wire, and it is deliberately not
    /// [`Self::destination_display`].** The display form falls back to the band's raw id so the
    /// sim's own event feed always has *something* to print; a wire field must not, because the
    /// client already has a label for a band and an id-shaped string would fight it. Empty means
    /// "no name", and the client uses whatever it calls that band everywhere else.
    pub fn destination_name(&self) -> &str {
        match self {
            ExpeditionMission::Trade {
                destination_name, ..
            } => destination_name,
            _ => "",
        }
    }

    /// **The string the SIM'S OWN EVENT FEED prints for a shipment's destination** — the name when
    /// there is one, the band's id as a last resort, exactly as [`Self::target_display`] falls back
    /// for a quarry. Empty for every non-`Trade` mission.
    ///
    /// **The id tier is the normal path today**, not an edge case: bands have no names, so every
    /// live shipment prints `band <id>`. That is the honest floor for a line the sim has to be able
    /// to write on its own — and the `detail` token beside it carries `destination=<id>`, so a
    /// client that would rather print its own label for that band has the key to do it with.
    /// **Never published**: the wire takes [`Self::destination_name`].
    pub fn destination_display(&self) -> String {
        match self {
            ExpeditionMission::Trade {
                destination_band,
                destination_name,
            } => {
                if destination_name.is_empty() {
                    format!("band {}", destination_band.0)
                } else {
                    destination_name.clone()
                }
            }
            _ => String::new(),
        }
    }

    /// The target herd id for a `Hunt`/`Deny` mission (empty for `Scout`) — the snapshot
    /// `expeditionTargetHerd`.
    pub fn target_herd(&self) -> &str {
        match self {
            ExpeditionMission::Hunt { fauna_id, .. } | ExpeditionMission::Deny { fauna_id, .. } => {
                fauna_id
            }
            ExpeditionMission::Scout | ExpeditionMission::Trade { .. } => "",
        }
    }

    /// The target herd's species display name for a `Hunt`/`Deny` mission (empty for `Scout`) — the
    /// snapshot `expeditionTargetSpecies`. **This is the name the client renders**; `target_herd` is
    /// the key it addresses commands by, and the two are not interchangeable.
    ///
    /// Empty is possible and means only "launched against a herd the registry could not resolve" —
    /// the client keeps its own herd-list join for that, so an empty string here costs nothing that
    /// was not already missing.
    pub fn target_species(&self) -> &str {
        match self {
            ExpeditionMission::Hunt { target_species, .. }
            | ExpeditionMission::Deny { target_species, .. } => target_species,
            ExpeditionMission::Scout | ExpeditionMission::Trade { .. } => "",
        }
    }

    /// **The string a player is shown for this mission's quarry** — [`Self::target_species`] when it
    /// resolved, else [`Self::target_herd`]. Every player-facing line about the target goes through
    /// here (the sim's own event-feed prose included), so no call site re-implements the fallback and
    /// none of them can reach for the raw id by accident.
    ///
    /// **The id tier is a last resort, not a normal path.** A launch resolves the species name from
    /// [`crate::fauna::HerdRegistry`] before it builds the mission (`outfit_raiding_party`), so every
    /// real party carries one; what falls through to the key is a mission hand-built without it — a
    /// test fixture, or a snapshot restored from a frame that carried no species. That mirrors the
    /// client's own last tier, which renders the id when neither the mission's name nor its herd-list
    /// join has anything to show.
    pub fn target_display(&self) -> &str {
        let species = self.target_species();
        if species.is_empty() {
            self.target_herd()
        } else {
            species
        }
    }

    /// The raid's escapement floor for a `Hunt` mission — the snapshot `expeditionFloor`. A `Scout`
    /// party harvests nothing, so it reports the floor that takes nothing.
    ///
    /// **A `Deny` mission reports [`STRIP_IT_BARE`]** (`docs/plan_denial_raid.md` §1) — its ceiling
    /// is the herd's whole standing stock, and it carries no floor of its own to report. `0` is the
    /// honest reading rather than a stand-in: nothing is meant to be left standing. It is a
    /// *derived* number, never a lever — the mission has no floor to set, which is the point of it
    /// being a mission.
    pub fn hunt_floor(&self) -> f32 {
        match self {
            ExpeditionMission::Hunt { floor, .. } => *floor,
            ExpeditionMission::Deny { .. } => STRIP_IT_BARE,
            ExpeditionMission::Scout | ExpeditionMission::Trade { .. } => NO_RAID_FLOOR,
        }
    }

    /// **Does a full pack stop this party engaging?** — the one line of behaviour a denial raid
    /// changes (`docs/plan_denial_raid.md` §1), stated here so every take and forecast path reads it
    /// from the mission rather than re-deriving it from a floor.
    pub fn engagement_stop(&self) -> crate::fauna::EngagementStop {
        match self {
            ExpeditionMission::Deny { .. } => crate::fauna::EngagementStop::Never,
            ExpeditionMission::Hunt { .. }
            | ExpeditionMission::Scout
            | ExpeditionMission::Trade { .. } => crate::fauna::EngagementStop::WhenPackFull,
        }
    }

    /// **The orders a party works a herd under**, for the two missions that work one — `None` for a
    /// `Scout`, which raids nothing. One seam, so the `Hunting` phase arm handles a hunt and a
    /// denial raid through the same code with the differences carried as data.
    pub fn raid_orders(&self) -> Option<RaidOrders<'_>> {
        match self {
            ExpeditionMission::Scout | ExpeditionMission::Trade { .. } => None,
            _ => Some(RaidOrders {
                fauna_id: self.target_herd(),
                floor: self.hunt_floor(),
                stop: self.engagement_stop(),
            }),
        }
    }
}

/// The expedition's lifecycle phase. Scout: `Outbound` toward a target; `AwaitingOrders` parked at
/// the target (the decision point — chain a `move_band` waypoint or `recall_expedition`). Hunt:
/// `Hunting` (chase the herd + harvest) and `Delivering` (run carried food to the band, then
/// auto-relaunch). Shared: `Returning` chasing the home band's live tile to fold back on recall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpeditionPhase {
    Outbound,
    AwaitingOrders,
    Returning,
    Hunting,
    Delivering,
}

impl ExpeditionPhase {
    /// Stable wire/snapshot key for the phase (client marker state discriminator).
    pub fn as_str(&self) -> &'static str {
        match self {
            ExpeditionPhase::Outbound => "outbound",
            ExpeditionPhase::AwaitingOrders => "awaiting",
            ExpeditionPhase::Returning => "returning",
            ExpeditionPhase::Hunting => "hunting",
            ExpeditionPhase::Delivering => "delivering",
        }
    }

    /// Parse a phase from its wire key (snapshot restore). Unknown keys default to `Outbound`.
    pub fn from_wire(s: &str) -> Self {
        match s {
            "awaiting" => ExpeditionPhase::AwaitingOrders,
            "returning" => ExpeditionPhase::Returning,
            "hunting" => ExpeditionPhase::Hunting,
            "delivering" => ExpeditionPhase::Delivering,
            _ => ExpeditionPhase::Outbound,
        }
    }
}

/// Marks a detached traveling party (a scouting/hunting expedition). Reuses `PopulationCohort` +
/// `BandTravel` + `LaborAllocation` + `StartingUnit` machinery, but is excluded from the
/// population/settlement arc (it lacks [`ResidentBand`]) and from live faction fog reveal
/// (`Without<Expedition>` in `calculate_visibility`). Discovery is **communication-range gated**: it
/// buffers the tiles it observes in `pending_reveal` and `advance_expeditions` flushes them to the
/// faction map as `Discovered` only while within comm range of the home band. Snapshot-persisted so
/// a rollback preserves an in-flight expedition and its unreported findings.
#[derive(Component, Debug, Clone)]
pub struct Expedition {
    /// The real band that outfitted this party. `Returning` chases this band's **live** tile (bands
    /// are nomadic), and fold-back deposits the party's workers + leftover provisions here.
    pub home_band: Entity,
    pub mission: ExpeditionMission,
    pub phase: ExpeditionPhase,
    /// Whether the arrival ("reached X — awaiting orders") feed line has fired for the current
    /// `AwaitingOrders` latch; reset to `false` when a new `move_band` order relaunches the party.
    pub announced: bool,
    /// Observed-but-unreported tile coordinates (deduped). Flushed to the faction map as
    /// `Discovered` when the party is within comm range of its home band, then cleared.
    pub pending_reveal: Vec<UVec2>,
    /// **Peoples the party has found and not yet reported** — subject band → (where it was seen,
    /// the turn it was seen). Comm-gated exactly like [`Self::pending_reveal`] beside it: a
    /// scouting party extends its home band's range, and what it finds reaches the faction through
    /// the same flush.
    ///
    /// **Most-recent observation per subject wins** (re-observing overwrites), and the flush
    /// credits the **home band** with *one* contact per subject however many turns the party
    /// watched them — a report that comes home is one report, and crediting thirty turns of
    /// retroactive contact would let a stale report peg a tie at full strength.
    ///
    /// A `BTreeMap` for the reason [`crate::connections::ConnectionLedger`] is one: the flush order
    /// reaches a checkpointed ledger, so it must be an order rather than an accident.
    pub pending_contacts: std::collections::BTreeMap<BandId, (UVec2, u64)>,
    // **`carried_trade` is RETIRED** (arc #527) with the trade-goods axis it banked. What a raid
    // physically carries home is provisions in `stores[FOOD]` and **material batches** in that same
    // `LocalStore`, moved by `LocalStore::drain_materials_into` batch by batch — so a mammoth hide
    // is never averaged into a hare pelt on the walk home. There is nothing left to flatten onto a
    // scalar. `PopulationCohortState.expedition_carried_trade` went with it.
    /// **The kit this party was SENT OUT WITH**, resolved from the roster at launch and carried for
    /// the party's whole life (`equipment.json`'s `kits`).
    ///
    /// **It is never re-resolved against the home band's stock.** A party sent out with `none` is
    /// bare-handed until it folds back — re-reading the band's spears each turn would silently
    /// re-arm it, and a bare-handed comparison that quietly re-arms is not a comparison. The party's
    /// own [`BandEquipment`] wear still moves under it, so a `big_game` party still steps down when
    /// its spears run out; what is fixed is *which components it reaches for*.
    ///
    /// A scouting party carries the hunt job's default: its roadside kills resolve through the same
    /// hunt seams, and `send_expedition` names no kit.
    pub kit: crate::equipment_config::KitChoice,
    /// **The shipment this party is carrying**, for an [`ExpeditionMission::Trade`] party (empty for
    /// every other verb) — food under the [`FOOD`] key and materials as batches, exactly as a band's
    /// store holds them.
    ///
    /// **It is a SECOND store, not the party's own `stores`, and that separation is the whole
    /// point.** A party eats out of `cohort.stores` — the scout upkeep drains it every turn — so a
    /// shipment parked there would be quietly eaten by the people hauling it, one turn at a time,
    /// with nothing to notice until it arrived short.
    ///
    /// **Materials ride as batches with their exact readings.** Drawing the cargo off the sending
    /// band peels whole batches in the store's own order and splits only the last one
    /// ([`LocalStore::take_material_batches`]), so two ratings of one material leave as two batches
    /// and land as two batches — a mammoth hide is never averaged into a hare pelt by being shipped.
    ///
    /// **It rides the checkpoint whole**, like every other field here: `capture_sim_state` clones
    /// the entire `Expedition` into `ExpeditionRecord` and restore clones it back, so a rollback
    /// cannot silently zero a shipment in flight.
    ///
    /// **An undeliverable shipment comes home in it.** If the destination cannot be resolved the
    /// party turns for home still carrying the cargo, and [`crate::systems::fold_party_into_band`]
    /// settles it into the home band beside the party's own pack.
    pub cargo: LocalStore,
}

/// Permanent settlement seeded by a founding action.
#[derive(Component, Debug, Clone)]
pub struct Settlement {
    pub faction: FactionId,
    pub position: UVec2,
}

/// Anchor component for the initial hub within a settlement.
#[derive(Component, Debug, Clone)]
pub struct TownCenter {
    pub construction_radius: u32,
    pub logistics_radius: u32,
}

/// Whole assignable workers a band's working-age bracket supplies this turn. Only *whole*
/// people can be staffed onto a source, so this floors the fractional `working` Scalar (which
/// otherwise carries sub-person demographic precision). The `Σ assignments ≤ available` invariant
/// on [`LaborAllocation`] is enforced against this count.
pub fn available_workers(working: Scalar) -> u32 {
    (working.raw().max(0) / Scalar::SCALE) as u32
}

/// **WHICH PLANTS A GATHERING CREW CARRIES HOME** — the take selection a `Forage` assignment rides
/// (`docs/plan_flora_roster.md`; the selective-gather slice). A tile's basket mixes food with fibre,
/// so *what am I here for* is a decision beside *how hard do I press* (the harvest floor).
///
/// **Empty means take EVERYTHING**, which is the default and is exactly today's behaviour — a crew
/// that names nothing fills its baskets from the whole stand. Naming one or more species leaves the
/// rest standing: only the named plants' share of the biomass is available, and only their rows are
/// converted and drawn down.
///
/// **Sorted and deduplicated by construction, not by presentation.** The selection reaches the
/// snapshot, and a set whose iteration order varies between two builds has already cost this repo a
/// ~50%-of-runs determinism flake (`flora.md` → the share-denominator note). A `BTreeSet` makes the
/// unsorted state unrepresentable rather than merely unusual, so no call site has to remember to
/// sort. Blank keys are dropped at construction for the same reason a blank
/// [`LaborTarget::Forage::species`] is: `""` is not a plant.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TakeSelection {
    /// Private so the ordered, blank-free invariant has exactly one enforcing constructor.
    species: BTreeSet<String>,
}

impl TakeSelection {
    /// **The whole basket** — what a crew that named nothing carries home, and the default. Named
    /// so a quote that is deliberately about the *land* rather than about one crew says which of the
    /// two it means, instead of passing an anonymous empty set.
    pub const EVERYTHING: Self = Self {
        species: BTreeSet::new(),
    };

    /// Build a selection from whatever the player named, trimming blanks and folding duplicates.
    pub fn from_keys<I, S>(keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            species: keys
                .into_iter()
                .map(|key| key.as_ref().trim().to_string())
                .filter(|key| !key.is_empty())
                .collect(),
        }
    }

    /// **Is this the whole basket?** — the empty selection, and the only reading `is_empty` has.
    pub fn is_everything(&self) -> bool {
        self.species.is_empty()
    }

    /// Whether a named species is one this crew carries home. Always `true` on the whole basket.
    pub fn takes(&self, species: &str) -> bool {
        self.is_everything() || self.species.contains(species)
    }

    /// The named keys, in the collection's own ascending order — what the wire publishes.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.species.iter().map(String::as_str)
    }
}

/// A single labor demand a band can staff from its working-age pool (Early-Game Labor, slice 3a):
/// an in-range food source (Forage tile / Hunt herd) or a band-wide role (Scout / Warrior).
/// The band is a labor pool drawing subsistence from many sources at once
/// (`docs/plan_early_game_labor.md`).
#[derive(Debug, Clone, PartialEq)]
pub enum LaborTarget {
    /// Gather food from a food-module tile within `band_work_range`, stopping at a **floor**. Stored
    /// as coordinates (not an entity) so a moving band re-resolves the tile each turn — and a tile
    /// that re-resolves out of range is **abandoned**, the plant twin of the Hunt leash lapse: the
    /// assignment drops and its workers return to the pool. The asymmetry is deliberate — a herd
    /// moves, so `hunt_leash_tiles` buys the band time to follow it, but a patch is fixed, so
    /// out-of-range can only mean the band walked away from it.
    Forage {
        tile: UVec2,
        /// **WHERE THE GATHER STOPS, as a fraction of the patch's `K`** — the whole of what the
        /// player decides about pressure (`docs/plan_harvest_floor.md` §1). The take is
        /// `min(crew throughput, max(0, B − floor·K) × build dip)`, so this one number replaced the
        /// four-stance axis: `0.5` holds the patch on its most productive biomass, `0` strips it.
        /// [`DEFAULT_ESCAPEMENT_FLOOR`] when the player named none; validated `0.0..=1.0` at the
        /// command boundary ([`floor_is_valid`]) and never clamped silently.
        floor: f32,
        /// **Which named plant a `Cultivate`/`Sow` on this tile should commit the patch to** — a
        /// `flora_config.json` species key, or `None` for *"pick the tile's dominant legal plant"*
        /// (`docs/plan_flora_roster.md` §4.3). It rides the *target*, beside the floor, because it
        /// is the same kind of thing: a mutable property of the same source, replaced rather than
        /// duplicated by a re-assignment (see [`LaborTarget::same_source`]).
        ///
        /// Inert at every floor — the patch records the commitment, so changing the selection after
        /// the ground is committed does nothing until the patch goes feral.
        species: Option<String>,
        /// **WHICH PLANTS THIS CREW CARRIES HOME** — see [`TakeSelection`]. Empty (the default)
        /// takes the whole basket, exactly as every assignment did before the selective gather.
        ///
        /// **It is NOT [`LaborTarget::Forage::species`]**, which is the *commit* crop a
        /// `Cultivate`/`Sow` names and which is inert until an improvement completes. This one is
        /// live at rung 1, on the take itself. Like the floor, it is a mutable property of the same
        /// source: changing it on the same tile replaces the assignment rather than adding a second
        /// (see [`LaborTarget::same_source`]).
        take_species: TakeSelection,
    },
    /// Hunt a fauna group by id, stopping at a **floor**. The band tracks a roaming herd up to
    /// `band_work_range + hunt_leash_tiles` (leashed follow); past that the assignment lapses.
    Hunt {
        fauna_id: String,
        /// **WHERE THE HUNT STOPS, as a fraction of the herd's `K`** — see
        /// [`LaborTarget::Forage::floor`]. `0.5` settles the herd on `K/2`; `0` takes it under
        /// `extinction_floor` and the herd is gone.
        floor: f32,
    },
    /// Reveal fog outward from the band (band-wide role, no food yield).
    Scout,
    /// Guard the band (band-wide role). Inert until the predator slice consumes it — it only
    /// occupies workers against the Σ invariant.
    Warrior,
    /// **KEEP THE BAND'S PLANT IMPROVEMENTS** — the agriculture standing role
    /// (`docs/plan_standing_upkeep.md` §2.5). Its workers are a **pool** that supplies every tended
    /// patch and Field this band works, against the summed
    /// [`crate::forage::patch_upkeep_demand`] of all of them.
    ///
    /// # WHY MAINTENANCE LEFT THE TILE
    ///
    /// It was a per-source crew (`maintain <faction> forage <x> <y> <n>`), and an **indivisible
    /// supplier meeting a per-source demand wastes whatever it does not spend**: a patch asking for
    /// `2.0` work staffed by three hands throws one away, and the waste grows as gear makes a hand
    /// worth more. A pool has no leftover by construction — every unit either meets a demand or is
    /// still in the pool — and the band's demand is simply the sum over what it holds.
    ///
    /// **One role per WEB, because the two webs are already separate ladders** — this is their
    /// existing split, not a new axis. See [`LaborTarget::Husbandry`] for the animal half.
    Agriculture,
    /// **KEEP THE BAND'S HERDS** — the husbandry standing role, the animal twin of
    /// [`LaborTarget::Agriculture`]: one pool against the summed
    /// [`crate::fauna::herd_upkeep_demand`] of every pastoral herd and pen this band works.
    Husbandry,
    /// **RAISE WHATEVER THIS BAND HAS QUEUED** — the builders standing role
    /// (`docs/plan_standing_upkeep.md` §2.5). Its workers are a **pool** whose whole output goes on
    /// the **head** of [`LaborAllocation::build_queue`] until that entry's meter fills, then on the
    /// next one.
    ///
    /// # ONE POOL FOR BOTH WEBS, unlike the keeping
    ///
    /// The two keeping roles split because the two webs are separate ladders and a keeping demand
    /// is a *standing charge* on everything a band holds there. A build is a **job**, and the queue
    /// already says which one is being worked — so a second axis would only ask the player to say
    /// the same thing twice.
    ///
    /// # A VERB DECLARES; IT DOES NOT STAFF
    ///
    /// `cultivate` / `sow` / `tame` / `corral` / `extend_pen` append a
    /// [`BuildQueueEntry`]; none of them names a crew. The hands are here, and `0` is how the
    /// player says *stop building* — for the whole band rather than for one source.
    ///
    /// **Spread is deliberately not offered.** An under-kept improvement has something to ride out,
    /// so spreading a short keeping pool loses nothing; splitting a builder pool across three jobs
    /// just means nothing finishes.
    Builders,
}

impl LaborTarget {
    /// The stable role key (also the snapshot `kind` string and the `activity` summary).
    pub fn kind(&self) -> &'static str {
        match self {
            LaborTarget::Forage { .. } => "forage",
            LaborTarget::Hunt { .. } => "hunt",
            LaborTarget::Scout => "scout",
            LaborTarget::Warrior => "warrior",
            LaborTarget::Agriculture => "agriculture",
            LaborTarget::Husbandry => "husbandry",
            LaborTarget::Builders => "builders",
        }
    }

    /// **Which TOE job this target draws a tier from.** The one mapping between a labor role and
    /// `equipment.json`'s `jobs` list, so the command's refusal and the turn's pricing agree.
    ///
    /// **Every role answers `Some` now.** It used to be `None` for the two band-wide roles, on the
    /// grounds that they consumed no component — but scouts have posted forward-observer vantages
    /// (`calculate_visibility`) and warriors have been the band's defending contingent
    /// (`advance_predator_raids`) for some time, so that was a fact about the shipped roster rather
    /// than about the sim. It stopped being true when the roster gained gear for them.
    pub fn kit_job(&self) -> crate::equipment_config::KitJob {
        match self {
            LaborTarget::Forage { .. } => crate::equipment_config::KitJob::Forage,
            LaborTarget::Hunt { .. } => crate::equipment_config::KitJob::Hunt,
            LaborTarget::Scout => crate::equipment_config::KitJob::Scout,
            LaborTarget::Warrior => crate::equipment_config::KitJob::Warrior,
            LaborTarget::Agriculture => crate::equipment_config::KitJob::Agriculture,
            LaborTarget::Husbandry => crate::equipment_config::KitJob::Husbandry,
            LaborTarget::Builders => crate::equipment_config::KitJob::Builders,
        }
    }

    /// Whether two targets name the **same source** (so re-assigning replaces rather than
    /// duplicates). Forage is keyed by tile and Hunt by herd id — for both, the **floor** is a
    /// mutable property of the same source (dragging the floor on the same tile/herd replaces the
    /// assignment rather than adding a second one) — and the band-wide roles are singletons.
    pub fn same_source(&self, other: &LaborTarget) -> bool {
        match (self, other) {
            (LaborTarget::Forage { tile: a, .. }, LaborTarget::Forage { tile: b, .. }) => a == b,
            (LaborTarget::Hunt { fauna_id: a, .. }, LaborTarget::Hunt { fauna_id: b, .. }) => {
                a == b
            }
            (LaborTarget::Scout, LaborTarget::Scout) => true,
            (LaborTarget::Warrior, LaborTarget::Warrior) => true,
            (LaborTarget::Agriculture, LaborTarget::Agriculture) => true,
            (LaborTarget::Husbandry, LaborTarget::Husbandry) => true,
            (LaborTarget::Builders, LaborTarget::Builders) => true,
            _ => false,
        }
    }

    /// **Is this a SOURCE on the map, or a band-wide standing role?** — the split that decides
    /// whether a row survives losing its take crew (`docs/plan_standing_upkeep.md` §2.2).
    ///
    /// A **role** *is* its head count: `assign_labor … scout 0` says *stop scouting*, and there is
    /// nothing else for the row to carry. A **source** row is the band's **holding** of a patch or a
    /// herd, and the take crew is only one of the three allocations on it — so zeroing the gatherers
    /// is *"stop gathering here"*, never *"this band has nothing here"*. Dropping the row on that
    /// edge is what made a finished Field ineligible for its own web's keeping pool: no row, no
    /// demand, no share, and no command the player could issue to fix it.
    ///
    /// Stated exhaustively so a new target has to answer the question rather than inherit a default.
    pub fn is_source(&self) -> bool {
        match self {
            LaborTarget::Forage { .. } | LaborTarget::Hunt { .. } => true,
            LaborTarget::Scout
            | LaborTarget::Warrior
            | LaborTarget::Agriculture
            | LaborTarget::Husbandry
            | LaborTarget::Builders => false,
        }
    }
}

/// One staffed labor demand: a target and the whole-worker head-count assigned to it.
///
/// **The build is not one of its axes any more** (`docs/plan_standing_upkeep.md` §2.5). A row used
/// to carry the improvement the crew was building and that build's own crew; both retired with the
/// per-source build crew. What a band is building is its **ordered queue**
/// ([`LaborAllocation::build_queue`]), funded by the band-level [`LaborTarget::Builders`] pool — so
/// there is exactly one authority for *"what is being raised here"*, and a row states only the
/// **take**.
///
/// The *pressure* — where the crew stops — still rides the target as its **floor**, and **the sim
/// never writes it**.
#[derive(Debug, Clone, PartialEq)]
pub struct LaborAssignment {
    pub target: LaborTarget,
    pub workers: u32,
    /// **The kit this crew works under** (`equipment.json`'s roster), chosen at assign time and
    /// re-resolved from *here* every turn — never from whatever the band happens to hold.
    ///
    /// `None` = **no kit was named**, which reads as the job's default
    /// ([`crate::equipment_config::EquipmentConfig::default_kit`]) and is the only reading for the
    /// band-wide roles: Scout and Warrior consume no component, so they have no kit axis to choose
    /// along. `assign_labor` stores the *resolved* choice for a Forage/Hunt row, so a replayed
    /// command lands on the kit it named rather than on whatever the default is today.
    pub kit: Option<crate::equipment_config::KitChoice>,
}

// **RETIRED: `ActivityCrew` / `LaborAssignment::improvement_workers` / `LaborAllocation::idle_for`**
// — the per-source BUILD crew and the axis a command gave back before counting what was free.
//
// **The build left the tile too** (`docs/plan_standing_upkeep.md` §2.5), for the reason the keeping
// did one slice earlier: the hands stand on [`LaborTarget::Builders`], one band-level pool, and a
// verb names no crew at all — it appends a [`BuildQueueEntry`]. With nothing per-source to restate
// there is no "which of this row's crews am I overwriting" question left to answer, so the enum, the
// `idle_for` it parameterised and the five verbs' affordability gate all go together. A role's
// stepper clamps on idle exactly as scout's and warrior's do, and `assign_labor` is the one
// enforcement.
//
// **RETIRED: `ActivityCrew::Maintain` / `LaborAssignment::maintain_workers`** — the per-source
// keeper crew (`maintain <faction> <source…> <workers>`), which left the tile one slice before the
// build did, for the same waste-of-an-indivisible-supplier reason.

impl LaborAssignment {
    /// **EVERY HAND THIS ROW HOLDS** — the take crew, and nothing else.
    ///
    /// **Neither standing commitment is a term here any more.** The keeping is a band-level role
    /// ([`LaborTarget::Agriculture`] / [`LaborTarget::Husbandry`]) and so, since
    /// `docs/plan_standing_upkeep.md` §2.5, is the building ([`LaborTarget::Builders`]) — each a
    /// *row* in the same list, counted by the same sum one level up
    /// ([`LaborAllocation::assigned_total`]). It survives as a named seam rather than collapsing
    /// into `workers` because *"every hand this row holds"* is the question `assigned_total` asks,
    /// and a future third allocation on a source would answer it here.
    pub fn staffed_total(&self) -> u32 {
        self.workers
    }

    /// **The kit this row is priced at** — its own choice, or the job's default when it named none.
    /// The one seam the resolved take, the assign-time seed and the wire all read, so a row cannot
    /// be quoted at one tier and paid at another. Infallible now that every role has a job.
    pub fn kit_choice(
        &self,
        config: &crate::equipment_config::EquipmentConfig,
    ) -> crate::equipment_config::KitChoice {
        self.kit
            .clone()
            .unwrap_or_else(|| config.default_kit(self.target.kit_job()))
    }
}

/// Retained per-source food-yield telemetry for one labor assignment this turn (derived, not
/// persisted). `actual` = the provisions the source actually produced this turn; `sustainable` =
/// the provisions it could yield *without drawing down its stock*. Forage is inexhaustible in
/// today's model so its `sustainable` is defined equal to `actual`; a Hunt's `sustainable` is the
/// herd's net regrowth this turn (`net_biomass_delta(..).max(0) × provisions_per_biomass`, scaled
/// by the same output multiplier). A per-turn `actual > sustainable` is the (client-derived)
/// overhunting signal — a *leading* flow indicator, distinct from the stock-based `ecology_phase`.
///
/// `workers_needed` = the **minimum** assigned workers that would have produced the same take — the
/// **overstaffing** signal. A source's take is `min(production, workers × per_worker_capacity)`; when
/// the binding constraint is NOT labor, the extra workers were idle. It is
/// `ceil(actual / per_worker_capacity)` clamped into `[1, assigned]` when anything was taken, else
/// `0`. `workers_needed < assigned` ⇒ the source is overstaffed (client flags the wasted labor).
/// **Derived at every rung** since slice 7 — the hardcoded `1` a managed source used to report
/// (`TENDED_SOURCE_WORKERS_NEEDED`) claimed one worker could carry home whatever the land offered, so
/// "max N useful here" read `1` on a Field paying ten workers' worth.
///
/// `wasted` = **the understaffing signal, the exact mirror of `workers_needed`'s overstaffing one**:
/// `production − actual`, the food this source offered that the crew could not collect (`0` when
/// collection was not the binding constraint). *Production* is what the source hands over this turn —
/// the policy ceiling at rungs 1–2, the managed rate at rung 3 — and *collection* is
/// `workers × per_worker_capacity`, so the two signals answer the two halves of "is this source
/// correctly staffed?": `workers_needed < workers` ⇒ drop some, `wasted > 0` ⇒ add some. Derived
/// per-turn; on rung 3 (a Field) it is genuinely food left standing, on the drawn-down plant rungs it
/// stays in the stock and regrows, and **on any animal rung it is meat left to rot** — a hunt kills
/// *whole animals* (slice 8), so a party that cannot haul a whole one still takes it and wastes the
/// rest. On an animal source *production* is therefore the biomass of the animals **killed**, not the
/// escapement the herd could have spared: an animal you didn't kill was never produced, it is still
/// alive (`fauna::forecast_production_and_take`).
///
/// `overdraws` = **does this take draw the stock below what it sustains** — THE ⚠ ([`take_overdraws`]),
/// answered by the sim
/// rather than derived by the client from `actual > sustainable`. That comparison stopped working when
/// the hunt began taking whole animals (slice 8): a Sustain hunt is **escapement to `K/2`**, so it
/// lands the herd exactly on its most-productive biomass and is *sustainable by construction* — but it
/// pays in **lumps** (nothing for 6 turns, then a whole mammoth), so `actual > sustainable` fires on
/// every kill turn. A ⚠ on the turn you correctly harvest a mammoth trains the player to ignore the
/// one signal that matters. So `sustainable` keeps reporting the honest **long-run MSY rate** ("this
/// herd sustains ~0.78/turn on average"), `actual` swings — that swing is *true*, and it is the
/// mechanic — and this flag says whether the take overdraws at all. It is false at any floor on or
/// above the food peak and for every managed rung-3 source; below the peak it is **also** a question
/// about the crew, because a floor these hands cannot reach is a floor nothing is drawn below. See
/// [`take_overdraws`], which is the only thing that may write this field.
///
/// `realized` = **the steady headline yield**, a **FORWARD PROJECTION**: the average food/turn this
/// source will deliver over the next `labor_config.yield_average_horizon_turns` turns, computed by
/// simulating the herd/patch forward from its CURRENT state under the assignment's policy + worker
/// count ([`fauna::project_realized_hunt`] / [`forage::project_realized_forage`]). A **pure function of
/// state** — no history, no persistence — so the assign-time seed and the resolved row compute the
/// identical number (exact forecast == actual, true no-jump). It is simulated **rate-based, without the
/// kill-credit bank**: the bank only quantises *when* whole animals arrive, never the N-turn total, so
/// projecting the smooth policy rate gives the smooth average directly. That is the whole point — the
/// lumpy bank-quantised take is what `actual` already reports, and averaging the instantaneous
/// `sustainable_yield(current biomass)` instead would *sawtooth* with the biomass (drops one body per
/// kill, regrows between). So on a mammoth's six wait turns `actual` is `0` and on the seventh it
/// spikes, while `realized` reads flat ≈ `MSY`. A self-terminating policy (Eradicate/Deplete) breaks the
/// projection early and divides by the turns actually simulated, so it reads the rate it delivers
/// *while the source lasts* rather than a horizon-diluted average. On a **continuous** source (forage
/// patch / Field) the projection reuses `forage_take` directly. `actual` and the ledger identity are
/// unchanged — this is a parallel steady value, added beside them, never replacing them.
///
/// `arrivals` = **when the food actually lands** — the other half of the same question `realized`
/// answers, from the same forward simulation run **WITH** the kill-credit bank
/// ([`fauna::project_arrivals_hunt`] / [`forage::project_arrivals_forage`]). Index `i` is the food
/// delivered `i + 1` turns from now, over `labor_config.arrivals_horizon_turns` turns; `0.0` where
/// nothing lands. `realized` deliberately *omits* the bank because the bank decides **when** a whole
/// animal arrives and not **how much** arrives over the window; this is the value that keeps the
/// timing. So a big-game Sustain hunt reads a lumpy schedule (six zeros, then a mammoth) whose total
/// is ≈ `realized × horizon`, while a forage patch — or fast game whose MSY clears a body every turn —
/// is positive in every slot, which is a **continuous** source correctly rendered as a solid run.
/// Projected from the source's **post-take** state, so slot 0 is genuinely the *next* delivery and not
/// the one this turn already paid.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceYield {
    pub actual: f32,
    pub sustainable: f32,
    pub wasted: f32,
    pub workers_needed: u32,
    pub overdraws: bool,
    /// The steady headline per-turn yield — the forward-projected average food/turn over the next
    /// `labor_config.yield_average_horizon_turns` turns, a pure function of the source's current state.
    /// See the struct-level doc above.
    pub realized: f32,
    /// **The discrete arrival schedule** — `arrivals[i]` = the food landing `i + 1` turns from now,
    /// `labor_config.arrivals_horizon_turns` entries long, `0.0` on a turn nothing lands. See the
    /// struct-level doc above.
    ///
    /// **There is deliberately NO fodder arrivals schedule.** `arrivals` is a *larder* concept — it
    /// answers *"when does food land so my people eat"*, a question with a consumption clock ticking
    /// against it. Nothing consumes the fodder store on a per-turn clock the way people eat, so a
    /// per-turn arrival timetable for it would answer a question nobody is asking. The omission is
    /// a decision, not an oversight (`docs/plan_hunt_yield_model.md` §9).
    pub arrivals: Vec<f32>,
    /// **Fodder this source produced this turn** — the feed-currency twin of [`SourceYield::actual`],
    /// and *literally* the `min(production, collection)` the band's
    /// `FODDER` [`LocalStore`] was credited with on this turn's resolution (issue #449). Reported,
    /// never recomputed: a readout that re-derived its own number would drift from what the band was
    /// actually paid, and the knowledge gate on the wild credit (`FODDERING_DISCOVERY_ID`) is part of
    /// what it was paid.
    ///
    /// **Plant-only, and that is structural rather than a gap**: no animal pays fodder
    /// ([`crate::fauna_config::YieldAccounts`] carries the component, the roster never populates it),
    /// so every hunt row reports an honest `0.0`. What this field exists for is the opposite case — a
    /// **sown hay Field** (`flora_config.json`'s `hay_grass`: no provisions, positive fodder) whose
    /// compact readout said `+0.00` while it fed the band's herds every turn.
    ///
    /// **It is NOT food income.** `PopulationCohortState.food_income` stays `Σ actual` and must never
    /// include this — fodder credits the band's `FODDER` store and never touches the larder, so
    /// folding it in would break the larder identity
    /// `larder_delta == food_income − food_consumption − pen_feed_upkeep`.
    ///
    /// **There is deliberately NO `realized_fodder` twin.** The plant web's forward projection is
    /// food-only ([`crate::forage::plant_food_only`]) and fodder is paid by the plant web **alone**,
    /// so a projected-fodder field would be a constant zero on the only web that can pay it — dead
    /// weight the client would have to fall back off anyway. The client reads the actual.
    ///
    /// **No [`YieldRange`] fodder bounds either**, for a sharper reason: every forage row reports
    /// [`YieldRange::certain`] — no engagement, no retreat, no fight, nothing stochastic anywhere on
    /// the plant web — so a fodder band would be a point at every source that could ever carry one.
    pub fodder: f32,
    /// **The MATERIALS this source credited this turn**, one entry per material id (arc #527) — the
    /// account a cash crop and an inedible quarry are paid *entirely* in, and the third thing a
    /// harvest of `B` biomass pays.
    ///
    /// **Reported, never recomputed.** It is exactly what
    /// [`crate::materials_config::credit_material_yield`] returned at the credit site, so a readout
    /// states the deposit rather than re-deriving it — the same discipline [`Self::fodder`] carries,
    /// and it matters more here: the credit skips a sub-quantum amount and an unknown material, and
    /// neither skip is visible to a second derivation.
    ///
    /// **EMPTY IS "NO ROW", NOT ZERO.** Most sources pay no material at all, and a client renders one
    /// row per entry — a published `0` would read as a source that pays badly rather than one that
    /// pays in something else. It is why this is a vector of named amounts and not a scalar.
    ///
    /// **Never summed into one number**, on any surface: that is the retired trade-goods axis under a
    /// new name. And never into `food_income` — a material is not food, exactly as [`Self::fodder`]
    /// is not.
    pub materials: Vec<crate::materials_config::MaterialPayoff>,
    /// **The band around [`SourceYield::actual`]** — *"6–11, likely 9"*
    /// (`docs/plan_hunt_through_combat.md` §6.4). See [`YieldRange`].
    pub range: YieldRange,
}

/// **The distribution a [`SourceYield`]'s `actual` sits in the middle of**, in the same currency and
/// the same units (`docs/plan_hunt_through_combat.md` §6.4).
///
/// A hunt has two stochastic stages — the quarry's retreat (`fauna::animals_that_stay`) and the
/// fight's per-unit attack rolls — so a **pre-commit** row states an expectation, not a promise, and
/// this is the band the sim will actually pay inside. A **resolved** row is a fact rather than a
/// forecast, so it reports [`YieldRange::certain`]: the take has happened and there is no
/// distribution left.
///
/// **It is an ANSWER, not a term the client composes**, and that follows the boundary rule
/// `.claude/rules/core_sim/yield-forecast.md` already draws: the take goes through
/// `fauna::quantise_animal_take`'s `floor()`, so a band on the animals brought down is **not** a band
/// on the food — on a slow breeder both bounds routinely land on the same whole animal and the range
/// is a point at a staffing where the underlying draw genuinely varies. Publishing `wariness` and
/// `hit_chance` as terms instead would put a second, non-linear copy of the model in a language with
/// no tests over it, which is the same reason `regrowthSamples` ships sampled.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct YieldRange {
    /// The pessimistic bound on the provisions component.
    pub low: f32,
    /// The optimistic bound on the provisions component.
    pub high: f32,
}

impl YieldRange {
    /// A row that produced nothing.
    pub const ZERO: Self = Self {
        low: 0.0,
        high: 0.0,
    };

    /// **A range that is a point** — what a *resolved* row reports (the take happened; there is
    /// nothing left to be uncertain about), and what a *forecast* row reports wherever no stage is
    /// stochastic: the whole plant web (no engagement, no retreat, no fight), a pen, and a species
    /// held at `wariness 0`. Since slice 7 authored the roster's wariness
    /// (`docs/plan_hunt_through_combat.md` §3.1) a **wild hunt's** forecast is no longer one of
    /// them.
    pub fn certain(provisions: f32) -> Self {
        Self {
            low: provisions,
            high: provisions,
        }
    }
}

impl SourceYield {
    /// A source that has produced nothing: the row every assignment starts each turn's resolution
    /// with (so an arm that bails — out of range, module lost, herd gone — leaves a correct 0-yield
    /// row), and the row a freshly-staffed assignment carries until it is seeded from its pre-commit
    /// forecast (`set_source_yield`) or resolved by a turn.
    pub const ZERO: Self = Self {
        actual: 0.0,
        sustainable: 0.0,
        wasted: 0.0,
        workers_needed: 0,
        // Nothing was taken, so nothing was overdrawn.
        overdraws: false,
        // Nothing was taken, so the steady average is zero too.
        realized: 0.0,
        // …nor in the feed currency: nothing was harvested, so nothing was foddered.
        fodder: 0.0,
        // Nothing was harvested, so nothing was made of anything. An **empty** list, not a row of
        // zeros — see [`SourceYield::materials`]. `Vec::new` allocates nothing, so this stays a
        // `const`.
        materials: Vec::new(),
        // Nothing is coming either. An **empty** schedule, not a run of zeros: a source with no row
        // has not been projected at all, and the client renders "no data" rather than "famine".
        // `Vec::new` allocates nothing, so this stays a `const`.
        arrivals: Vec::new(),
        // Nothing was taken, so there is nothing to be uncertain about either.
        range: YieldRange::ZERO,
    };
}

/// **The craft quality one batch came out at** — the grade the bench selected, and the absolutes it
/// declared.
///
/// **The effects are resolved at CRAFT TIME and carried, not looked up later**, which is what makes
/// *"the grade is fixed at craft time and never moves"* structural rather than remembered: a recipe
/// retuned under a running world cannot re-grade a sled already in the band's hands, and neither can
/// the recipe being swapped off the bench. It is the same reason [`DrawnInputs`] carries its reading.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchGrade {
    /// The grade id the draw selected — a `characteristic_bands` name (`poor` / `fair` / `good` /
    /// `excellent`), because there is **one quality ladder for the whole game**: the same four words
    /// rate the hide and the sled made out of it. The readout's word.
    pub id: String,
    /// The absolutes this grade declares, copied from the recipe at the moment of the craft.
    pub effects: Vec<crate::equipment_config::EquipmentEffect>,
}

/// **One batch of an item a band owns** — a count of interchangeable units, what they were made at,
/// and the condition spent on the one currently in hand.
///
/// **Ten spears made together wear together**: the batch carries **one** wear number, and it is the
/// unit in use that is spending it. A batch of `count` therefore holds `count ×
/// starting_durability` of life, spent one unit at a time — crossing the durability retires a unit
/// and starts the next, exactly as a fractional flow becomes an event on a whole-unit crossing.
///
/// **Idle stock does not rot.** Nothing charges a batch that did not go out, and nothing decays one
/// over turns, so stockpiling ahead of a hard season is a real strategy rather than a slow loss.
#[derive(Debug, Clone, PartialEq)]
pub struct EquipmentBatch {
    /// Whole units left in this batch, the one in hand included. A batch that reaches `0` is
    /// removed rather than kept at zero, so *"does the band own one"* is a question about presence.
    pub count: u32,
    /// The [`crate::equipment_config::EquipmentTier`] id these units were made at.
    pub tier: String,
    /// The craft grade, or `None` for a **start-stocked** unit: a shipped kit has a tier but was
    /// never on anyone's bench, so it has no grade to carry.
    pub grade: Option<BatchGrade>,
    /// Condition spent on the unit currently in hand, on the config's 0–100 scale. Kept strictly
    /// below the tier's `starting_durability` while `count > 0`.
    pub wear: f32,
}

/// **A band's TOE, as BATCHES** — what it owns of each consumable item and how worn the unit in hand
/// is (the TOE, `docs/plan_early_game_labor.md` → "Equipment / TOE",
/// `docs/plan_hunt_through_combat.md` §4.8). What each item does, how long each tier lasts and what
/// wears it are config ([`crate::equipment_config::EquipmentConfig`]).
///
/// > **AN ABSENT ENTRY IS *NOT OWNED*.** It used to read as a *full* item, which was correct for
/// > exactly as long as nothing could make a second spear: crafting can introduce an item a band has
/// > never had, and the old reading made that state unrepresentable. Every spawn and restore path
/// > therefore **inserts explicitly** — `spawn_profile_population`, both expedition-outfitting paths
/// > in `bin/server.rs`, and `sim_state.rs` — and
/// > [`crate::equipment_config::EquipmentConfig::start_stocked_items`] is what they stock.
///
/// **A spawn inserts `count: 1` per item, at the tier that ships known, and that is what preserves
/// the shipped opening exactly**: one unit is one item's `starting_durability`, which is the life
/// the game has always had. A count above 1 is something crafting bought — counts do **not**
/// multiply the shipped kit's life.
///
/// An item is **equipped while some batch's wear is strictly below its tier's
/// `starting_durability`**; with none left the role steps down to its unequipped tier and stays
/// there until a bench makes another.
///
/// **Wear is charged for USE, never for turns elapsed** (`docs/plan_denial_raid.md` §1.2) — a turn
/// clock would charge an idle march the same as a slaughter and make denial free. **Each item has its
/// own quantum** ([`crate::equipment_config::WearQuantum`]): spears and traps wear per **animal
/// killed**, the sled per **biomass hauled home from a hunt**, baskets per **biomass gathered**. The
/// ledgers are independent by construction, so a band that only hunts wears no baskets and one that
/// only gathers wears no sled.
///
/// **Persisted** (`SimState`'s `BandRecord::equipment`) — a checkpoint that forgot how worn your
/// spears were would silently re-stock them on rollback.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct BandEquipment {
    /// Batches per item id. A `BTreeMap` rather than a `HashMap` so the checkpoint and the wire
    /// serialize in a stable order — a rollback that reordered this would diff as a change every
    /// frame — and a `Vec` inside it because batch order is insertion order, which is what makes
    /// *"the most worn first, earliest batch on a tie"* a deterministic rule.
    batches: std::collections::BTreeMap<String, Vec<EquipmentBatch>>,
    /// **Whole units this band has WORN OUT, per item and PER TIER**, ever. Monotonic; only
    /// [`Self::wear_item`] raises it, and that seam already holds the tier of the unit it is
    /// destroying, so the key costs nothing to record.
    ///
    /// **It exists because a batch that runs out of units is REMOVED**, so *"the sled broke"* and
    /// *"we have never had a sled"* are the same empty ledger — and they are not the same sentence to
    /// a player. Without this the panel's `Worn out` wording is unrepresentable and every count of
    /// zero has to read as *never made*, which is wrong for exactly the item the player just lost.
    ///
    /// **The TIER is part of the key because the readout names it out loud.** *"last flint set wore
    /// out"* is a claim about which tier was lost, and an item-wide tally could only *infer* one —
    /// the day iron ships beside bronze and flint, inferring *"the tier below what I can now make"*
    /// names bronze for a flint set that actually wore out. A published string asserting the wrong
    /// tier is worse than saying nothing.
    ///
    /// An item with no entry has retired none. **Not gameplay**: nothing in the sim branches on it,
    /// and it must not become a repair discount or a durability bonus — it is the readout's memory.
    retired: std::collections::BTreeMap<String, std::collections::BTreeMap<String, u32>>,
}

impl BandEquipment {
    /// **A fully start-stocked band** — one unit of every item some kit carries, at that item's
    /// default tier and unworn.
    ///
    /// It is what a spawn inserts, and it is also the **fresh reference ledger** every
    /// quarry-scoring and roster-quoting surface resolves against (`kit_supplying`,
    /// `quarry_default_hunt_kit`, the published kit roster, a launch forecast): *which* kit supplies
    /// a stat is a property of quarry × roster and must not move as one band wears its gear down.
    ///
    /// **`Default` is an EMPTY ledger and now means "owns nothing"**, so a site that wants the fresh
    /// tier has to say so with this — which is precisely the flip, made impossible to miss.
    ///
    /// **A SPAWN wants [`Self::start_stocked_owned`]** — this one leaves every batch *ungraded*,
    /// which is right for a reference (a scoring pass reads stats, and a grade name is a label) and
    /// wrong for gear a band actually owns and a ledger names out loud.
    pub fn start_stocked(config: &crate::equipment_config::EquipmentConfig) -> Self {
        let mut stocked = Self::default();
        for (id, item) in config.start_stocked_items() {
            stocked.stock(id, ONE_UNIT, &item.default_tier().id, None);
        }
        stocked
    }

    /// **What an ABSENT COMPONENT reads as** — [`Self::start_stocked`] sized to a party of
    /// `workers`, ungraded.
    ///
    /// A band with no `BandEquipment` at all has no ledger rather than an empty one, and the four
    /// readers of that state (`advance_labor_allocation`, `advance_expeditions`,
    /// `snapshot/capture.rs`, `snapshot/population.rs`) agree it reads as *outfitted* — **what every
    /// spawn path inserts**. Since [`Self::start_stocked_owned`] began stocking a party's worth,
    /// one unit each no longer says that: under
    /// [`crate::equipment_config::EquipmentConfig::coverage`] it arms one person and sends the rest
    /// of a hand-rolled fixture's band out bare-handed. This keeps the four fallbacks meaning what
    /// they say.
    ///
    /// **The grades are still absent, and that is the split with `start_stocked_owned`**: a band
    /// that never had a ledger has no craft history to name, so the fallback states the stock and
    /// nothing about its quality.
    pub fn start_stocked_for(
        config: &crate::equipment_config::EquipmentConfig,
        workers: f32,
    ) -> Self {
        let mut stocked = Self::default();
        for (id, item) in config.start_stocked_items() {
            stocked.stock(
                id,
                config.start_stock_units(item, workers),
                &item.default_tier().id,
                None,
            );
        }
        stocked
    }

    /// **What a band or a detached party actually OWNS at spawn** — [`Self::start_stocked`] with
    /// every batch stamped with the grade a bare-handed craft of that item comes out at
    /// ([`crate::recipes_config::RecipesConfig::anchor_grade_for_item`]).
    ///
    /// **A start-stocked unit IS an anchor-grade craft, so it says so.** A spawn stocks the item's
    /// default tier (`equipment.md` → *"flint is today's spear, verbatim"*) and `validate` requires
    /// the anchor grade to agree with that tier for every stat it declares — the two perform
    /// identically, and the ledger simply was not saying which. An unstamped batch published a bare
    /// `×1` beside rows reading `×3 good`, which is indistinguishable from a panel that failed to
    /// draw something.
    ///
    /// **The NAME only; the effects payload stays empty.** A start-stocked unit's stats come from
    /// the tier and must keep coming from there — the grade name is a label `validate` already ties
    /// to those numbers, not a second home for them. An empty grade payload resolves through
    /// [`crate::equipment_config::LiveItem::effect_entry`] exactly as `None` does, so the shipped
    /// opening is unchanged by construction.
    ///
    /// An item no recipe makes keeps `None`: there is no crafted equivalent to claim. Every shipped
    /// start-stocked item has a recipe, so that is unreachable today.
    ///
    /// # A PARTY'S WORTH, not one unit — and that is why this one takes a head count
    ///
    /// A unit arms `workers_per_unit` people
    /// ([`crate::equipment_config::EquipmentConfig::coverage`]), so one unit of everything is one
    /// armed hunter and sixteen bare hands on the shipped band. It stocks
    /// `ceil(workers × start_stock_fraction / workers_per_unit)`
    /// ([`crate::equipment_config::EquipmentConfig::start_stock_units`]) — a party's worth plus the
    /// opening reserve that keeps the first break from disarming anyone.
    ///
    /// **[`Self::start_stocked`] takes no head count and still stocks one unit each**, deliberately:
    /// it is the fresh *reference* ledger every quarry-scoring and roster-quoting surface resolves
    /// against, where only liveness is read and a count would be noise.
    pub fn start_stocked_owned(
        equipment: &crate::equipment_config::EquipmentConfig,
        recipes: &crate::recipes_config::RecipesConfig,
        materials: &crate::materials_config::MaterialsConfig,
        // **The party's WORKERS, not its head count.** Children and elders hold nothing; the people
        // a kit has to reach are the ones who go out on a job.
        workers: f32,
    ) -> Self {
        let mut stocked = Self::default();
        for (id, item) in equipment.start_stocked_items() {
            let grade = recipes
                .anchor_grade_for_item(id, materials)
                .map(|band| BatchGrade {
                    id: band.to_string(),
                    effects: Vec::new(),
                });
            stocked.stock(
                id,
                equipment.start_stock_units(item, workers),
                &item.default_tier().id,
                grade,
            );
        }
        stocked
    }

    /// **Add a batch of `count` units** — a spawn's start kit, or the bench delivering a finished
    /// craft. Unworn, because nothing has used it yet.
    ///
    /// This is the **one** seam in the sim that adds condition, and it is what ends *"start-stocked
    /// and NOT craftable"*: running dry stopped being a one-way door the moment a bench could
    /// replace the thing. It never touches an existing batch — *"the next ten are their own batch"*
    /// is what keeps a fresh craft from averaging into a half-spent pile.
    pub fn stock(&mut self, item: &str, count: u32, tier: &str, grade: Option<BatchGrade>) {
        if count == 0 {
            return;
        }
        self.batches
            .entry(item.to_string())
            .or_default()
            .push(EquipmentBatch {
                count,
                tier: tier.to_string(),
                grade,
                wear: 0.0,
            });
    }

    /// Every batch this band holds, item by item in id order — the checkpoint's and the wire's
    /// iteration.
    pub fn batches(&self) -> impl Iterator<Item = (&str, &[EquipmentBatch])> {
        self.batches
            .iter()
            .map(|(id, batches)| (id.as_str(), batches.as_slice()))
    }

    /// **This item's batches**, in insertion order — empty for one the band does not own. The
    /// direct lookup beside [`Self::batches`]'s full walk, so a per-item readout does not scan the
    /// whole ledger once per item.
    pub fn batches_of(&self, item: &str) -> &[EquipmentBatch] {
        self.batches
            .get(item)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Restore an item's batches verbatim — the checkpoint's setter. Not for gameplay: the only
    /// gameplay-side mutations are [`Self::wear_item`], which can never *reduce* wear, and
    /// [`Self::stock`], which is a spawn or the bench delivering a finished item.
    pub fn restore_batches(&mut self, item: &str, batches: Vec<EquipmentBatch>) {
        if batches.is_empty() {
            self.batches.remove(item);
        } else {
            self.batches.insert(item.to_string(), batches);
        }
    }

    /// **Condition spent on the units the band still holds** — the sum of every batch's `wear`.
    ///
    /// **A retired unit is not counted**, because a batch that ran out of units is gone: this reads
    /// what is *in hand*, which is what a wear-charge assertion over a single unbroken batch wants
    /// and what the *"how much of this quantum was charged"* arithmetic divides.
    pub fn wear_of(&self, item: &str) -> f32 {
        self.batches
            .get(item)
            .map(|batches| batches.iter().map(|batch| batch.wear).sum())
            .unwrap_or(0.0)
    }

    /// **Whole units of `item` the band owns**, across every batch — the count a *"turns left"*
    /// readout and a stockpiling decision both need.
    pub fn count_of(&self, item: &str) -> u32 {
        self.batches
            .get(item)
            .map(|batches| batches.iter().map(|batch| batch.count).sum())
            .unwrap_or(0)
    }

    /// **Whole units of `item` a party could actually be handed** — [`Self::count_of`] restricted to
    /// batches with condition left.
    ///
    /// **This is what COVERAGE counts** ([`crate::equipment_config::EquipmentConfig::coverage`]):
    /// how many people the band can arm is a question about units in usable condition, not about
    /// units owned. The two agree for every batch the sim can currently produce — [`Self::wear_item`]
    /// retires a unit the moment its condition is spent, so a batch with `count > 0` always has wear
    /// left — and they are still written apart, because the predicate is the claim being made and a
    /// future repair or salvage state that parked a spent unit in the ledger would silently arm
    /// somebody with it.
    pub fn live_units(&self, item: &str, config: &crate::equipment_config::EquipmentConfig) -> u32 {
        let Some(def) = config.item(item) else {
            return 0;
        };
        self.batches
            .get(item)
            .map(|batches| {
                batches
                    .iter()
                    .filter(|batch| {
                        batch.wear < def.tier_or_default(&batch.tier).starting_durability
                    })
                    .map(|batch| batch.count)
                    .sum()
            })
            .unwrap_or(0)
    }

    /// **The batch actually in use** — the **most worn** one that still has condition, earliest on a
    /// tie.
    ///
    /// Worst-first is what makes the stock run out **one batch at a time** rather than all at once,
    /// which is what makes *"turns left"* a real readout; and because the same batch answers for the
    /// party's tier ([`crate::equipment_config::EquipmentConfig::live_item`]), what the party is
    /// priced at is always what the party is spending.
    ///
    /// **An item the config does not carry has no serving batch**, rather than an immortal one: a
    /// kit cannot reference one (validate rejects it), so this arm is reachable only by a band
    /// restored from a checkpoint written against a config that has since dropped the item.
    pub(crate) fn serving_batch(
        &self,
        item: &str,
        config: &crate::equipment_config::EquipmentConfig,
    ) -> Option<&EquipmentBatch> {
        let def = config.item(item)?;
        self.serving_index(item, def)
            .map(|index| &self.batches[item][index])
    }

    /// The index of [`Self::serving_batch`] within the item's own `Vec`.
    fn serving_index(
        &self,
        item: &str,
        def: &crate::equipment_config::ItemDefinition,
    ) -> Option<usize> {
        let batches = self.batches.get(item)?;
        batches
            .iter()
            .enumerate()
            .filter(|(_, batch)| {
                batch.count > 0 && batch.wear < def.tier_or_default(&batch.tier).starting_durability
            })
            // The greatest wear wins; `>` rather than `>=` keeps the earliest batch on a tie, the
            // same tie-break the kit roster's file order uses.
            .max_by(|(_, a), (_, b)| {
                a.wear
                    .partial_cmp(&b.wear)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index)
    }

    /// **Does the band still have condition in this item?** — half of the effective predicate, never
    /// the whole of it. **Strictly below** the tier's `starting_durability`, so a unit worn exactly
    /// to its limit is spent: the cliff lands on the turn the last charge is used, not one turn
    /// later.
    ///
    /// **Crate-private on purpose.** Whether that condition is *serving* also depends on whether the
    /// party's chosen kit reaches for the item at all, and
    /// [`crate::equipment_config::KitChoice::item_live`] is the one place the two halves are joined. A
    /// caller that could ask this directly would be a second way to ask the question — and the one
    /// that silently re-arms a party sent out with no kit.
    pub(crate) fn has_condition(
        &self,
        item: &str,
        config: &crate::equipment_config::EquipmentConfig,
    ) -> bool {
        self.serving_batch(item, config).is_some()
    }

    /// Remaining condition on the unit in hand, clamped at `0` — the wire readout, and the number a
    /// player watches run down. **`0` for an item the band does not own**, which is the flip: an
    /// absent entry is not a fresh item.
    pub fn remaining(&self, item: &str, config: &crate::equipment_config::EquipmentConfig) -> f32 {
        let Some(def) = config.item(item) else {
            return 0.0;
        };
        self.serving_batch(item, config).map_or(0.0, |batch| {
            (def.tier_or_default(&batch.tier).starting_durability - batch.wear).max(0.0)
        })
    }

    /// **Charge `item` for `uses` of its own quantum**, against the batch in hand. One entry point
    /// for every item, so no item can grow a private flooring rule: a non-finite or negative `uses`
    /// reads as **no use**, because a degenerate take must never *restore* a kit.
    ///
    /// **A crossing retires a unit rather than the batch.** Wear past the tier's durability consumes
    /// the unit in hand and carries the remainder onto the next one, so ten spears really are ten
    /// spears' worth of life; a batch that runs out of units is dropped, and the charge stops there
    /// rather than spilling onto stock that never went out.
    ///
    /// A no-op for an item the band does not own or the config does not carry — there is no rate to
    /// bill at, and inventing one would be a silent second source for a number that lives in config.
    pub fn wear_item(
        &mut self,
        config: &crate::equipment_config::EquipmentConfig,
        item: &str,
        quantum: crate::equipment_config::WearQuantum,
        uses: f32,
    ) -> &mut Self {
        let Some(def) = config.item(item) else {
            return self;
        };
        // **THE CALLER NAMES WHAT IT IS PAYING FOR, and an item not worn by it pays nothing.** An
        // item may declare several quanta ([`crate::equipment_config::ItemDefinition::wear`]), so
        // the amount is no longer a property of the item alone — reading a single rate off the
        // definition would bill a `Tame` at the butchering rate the moment handling gear grew its
        // second entry. `None` is the same no-op as an item the band does not own.
        let Some(wear) = def.wear_for(quantum) else {
            return self;
        };
        let charged = usable_uses(uses) * wear.amount;
        if charged <= 0.0 {
            return self;
        }
        let Some(index) = self.serving_index(item, def) else {
            return self;
        };
        let Some(batches) = self.batches.get_mut(item) else {
            return self;
        };
        let batch = &mut batches[index];
        let durability = def.tier_or_default(&batch.tier).starting_durability;
        batch.wear += charged;
        let mut retired = 0u32;
        while batch.count > 0 && batch.wear >= durability {
            batch.count -= 1;
            batch.wear -= durability;
            retired += 1;
        }
        // **The tier is read off the batch being spent**, before it can be dropped — this seam knows
        // which tier it is destroying, so the readout never has to guess one later.
        let tier = batch.tier.clone();
        let emptied = batch.count == 0;
        if emptied {
            batches.remove(index);
            if batches.is_empty() {
                self.batches.remove(item);
            }
        }
        // **The readout's memory, written on the one seam that destroys a unit.** See the field's
        // docs: an emptied batch is removed, so without this the panel could not say *"worn out"*.
        if retired > 0 {
            *self
                .retired
                .entry(item.to_string())
                .or_default()
                .entry(tier)
                .or_default() += retired;
        }
        self
    }

    /// **Whole units of `item` this band has worn out**, ever, across every tier. `0` for an item it
    /// has never retired — including one it has never owned.
    ///
    /// Paired with [`Self::count_of`] it separates the two states a count of zero collapses:
    /// `count 0, retired > 0` is **worn out**, `count 0, retired 0` is **never made**.
    ///
    /// **The checkpoint carries it for free**: `SimState`'s `BandRecord::equipment` clones the whole
    /// [`BandEquipment`], so there is no restore setter beside [`Self::restore_batches`] to forget.
    pub fn retired_of(&self, item: &str) -> u32 {
        self.retired
            .get(item)
            .map(|tiers| tiers.values().sum())
            .unwrap_or(0)
    }

    /// **Which TIERS of `item` this band has worn out, and how many of each** — in tier-id order,
    /// empty for an item it has never retired.
    ///
    /// The readout's join: *"last flint set wore out"* names a tier, and this is the only record of
    /// which one it was. [`Self::retired_of`] is the same tally summed for a caller that only asks
    /// *whether* anything broke.
    pub fn retired_tiers_of(&self, item: &str) -> impl Iterator<Item = (&str, u32)> {
        self.retired
            .get(item)
            .into_iter()
            .flat_map(|tiers| tiers.iter().map(|(tier, count)| (tier.as_str(), *count)))
    }

    /// **Charge every item in `kit` whose quantum is `quantum`.** The seam every wear site calls, and
    /// the reason a site cannot forget an item: it names the *quantum* it just spent, not the items,
    /// so an item added to a kit is charged without editing a single call site.
    ///
    /// **Only items the kit actually USES are charged**, which is the pairing that makes the
    /// bare-handed comparison free to run — otherwise running the comparison would consume the very
    /// kit it is being compared against.
    pub fn wear_kit(
        &mut self,
        config: &crate::equipment_config::EquipmentConfig,
        kit: &crate::equipment_config::KitChoice,
        quantum: crate::equipment_config::WearQuantum,
        uses: f32,
    ) -> &mut Self {
        // **The collect is a BORROW break, not a copy** — the filter reads `self.has_condition` and
        // the loop below needs `&mut self`, so the immutable borrow has to end first. The items
        // themselves are borrowed from `kit`, which the loop never touches, so nothing needs owning:
        // an earlier `.map(str::to_string)` here allocated a `String` per charged item purely to
        // satisfy a borrow that `&str` already satisfies.
        let items: Vec<&str> = kit
            .uses()
            .filter(|item| {
                config
                    .item(item)
                    .is_some_and(|def| def.wears_on(quantum))
                    // **WEAR RIDES THE SAME PREDICATE THAT CHOSE THE TIER.** A spent item is already
                    // paying its cost — the role has stepped down — so charging it again would let a
                    // ledger run arbitrarily far past its own durability, and any future
                    // repair/crafting would then have to buy back that invisible overdraft before the
                    // item came back at all. Pinned by
                    // `kit_selection::a_kitted_partys_own_wear_still_steps_it_down`.
                    && self.has_condition(item, config)
            })
            .collect();
        for item in items {
            self.wear_item(config, item, quantum, uses);
        }
        self
    }
}

/// **What a spawn stocks of each start-kit item: one.** One unit is one item's
/// `starting_durability`, which is the life the shipped game has always had — so the opening is
/// preserved exactly and a count above `1` is something crafting bought.
const ONE_UNIT: u32 = 1;

/// The uses a wear charge may actually bill for: non-finite or negative input reads as **no use**.
/// One helper for every item and every quantum, so none can grow its own flooring rule — a negative
/// take must never *restore* a kit, because nothing in this slice replenishes one.
fn usable_uses(uses: f32) -> f32 {
    if uses.is_finite() {
        uses.max(0.0)
    } else {
        0.0
    }
}

/// **What one draw of a recipe's inputs came out at** — fixed when the materials leave the store and
/// never touched again.
///
/// **The grade is resolved HERE, at draw time, and never moves.** It is not a taper: an item made
/// from the last good hide in the pile is that good however poor the pile gets while it is being
/// made, and a tool that runs dry mid-craft does not retroactively coarsen the thing on the bench.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnInputs {
    /// The amount-weighted average reading of everything drawn on the recipe's `reads` axis.
    /// `None` for a recipe that reads nothing (an alloy).
    pub reading: Option<f32>,
    /// The grade that reading selected, **after** the tool's quality ceiling. `None` for a recipe
    /// that declares no grades.
    pub grade: Option<String>,
    /// **What actually came out of the store**, one row per input material in the recipe's own input
    /// order — the pile a clear or a swap destroys.
    ///
    /// It is the **withdrawn** amount rather than the recipe's stated one, because a bench tool's
    /// `craft_material_efficiency` sits between them, and the readout that names what will be lost
    /// has to name what was really taken.
    pub withdrawn: Vec<DrawnMaterial>,
}

/// **One material's share of a draw** — a row of [`DrawnInputs::withdrawn`].
///
/// Distinct from [`MaterialDraw`], which is one *batch* of one material: a draw walks the store
/// worst-first and may take from several batches, and what the bench holds afterwards is the total.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnMaterial {
    /// The `materials.json` id — **generic**, `hide` and never `deer_hide`.
    pub material: String,
    /// What the store actually lost on this row, summed over every batch the draw touched.
    pub amount: Scalar,
}

/// **A band's crafting bench — ONE job at a time.**
///
/// Design: `docs/plan_crafting_and_materials.md` §5/§7. **Make IS the assignment**: putting a recipe
/// on the bench draws idle workers onto it, so there is no Crafter role card and no
/// [`LaborTarget`] variant. Crafting always has a subject, so it is staffed like a worked source
/// rather than like a standing role.
///
/// **The crew is its own number and it comes out of the same pool `assign_labor` spends**
/// ([`crate::components::available_workers`] minus what the bench holds), so a band cannot staff the
/// bench and the range with the same people. Clearing the job returns them.
///
/// **Persisted** (`SimState`'s `BandRecord::bench`) — a checkpoint that forgot a half-finished craft
/// would silently hand back the materials it had already drawn.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct BandBench {
    /// The recipe on the bench, or `None` for an idle bench. An id from `recipes.json`, resolved at
    /// the command boundary so an unknown one is a command failure rather than a bench that quietly
    /// does nothing.
    pub recipe_id: Option<String>,
    /// How many of the band's workers are on it.
    pub workers: u32,
    /// Progress toward this pass's `work`. Fixed-point, so a slow bench accumulates instead of
    /// rounding to nothing each turn.
    pub progress: Scalar,
    /// The materials already withdrawn for the pass in flight, and the grade they fixed. `None`
    /// before the draw — a **short draw withdraws nothing at all**, so this stays `None` and the
    /// turn is a no-op rather than a half-spent pile.
    pub drawn: Option<DrawnInputs>,
    /// **How many items this bench has finished on the current job** — the same count the wear and
    /// the lesson were charged, so a readout of one is a readout of the others.
    pub items_completed: u32,
    /// The grade of the last item this bench finished, for the readout. Cleared with the job.
    pub last_output_grade: Option<String>,
}

impl BandBench {
    /// **Put a recipe on the bench**, discarding whatever was there. Progress and the drawn pile go
    /// with it: a job swapped out mid-pass has to draw again, because the materials it drew were for
    /// the thing it is no longer making.
    pub fn set_job(&mut self, recipe_id: &str, workers: u32) {
        self.recipe_id = Some(recipe_id.to_string());
        self.workers = workers;
        self.progress = scalar_zero();
        self.drawn = None;
        self.items_completed = 0;
        self.last_output_grade = None;
    }

    /// Take the job off the bench and hand the crew back.
    pub fn clear_job(&mut self) {
        *self = Self::default();
    }

    /// Whether anything is on the bench at all.
    pub fn is_running(&self) -> bool {
        self.recipe_id.is_some()
    }
}

/// **A band's head-count, resolved in ONE place for every reader of it.**
///
/// A band's working-age people are spent on exactly two things — the [`LaborAllocation`]'s
/// assignments and the [`BandBench`]'s crew — and every question anyone asks about staffing is one
/// of the three differences below. They live together because they are the same arithmetic read
/// three ways: the command clamps, the published `idleWorkers`, and the client readouts that size a
/// stepper must agree by construction.
///
/// **The bench is netted out exactly once, here.** It used to be subtracted at each command site
/// and *not* at the publish site, so a band with four hands at the bench published them as idle —
/// and it over-reported in the reassuring direction, telling the player it had hands free that were
/// already busy. Two authorities over one number is how they drift; this is the one authority.
pub struct BandWorkforce {
    /// Every whole working-age person the band has, before anything is staffed
    /// ([`available_workers`] of the cohort's `working` bracket).
    pub pool: u32,
    /// What the labor allocation already staffs across all its sources and roles.
    pub assigned: u32,
    /// What is standing at the bench. Not a [`LaborTarget`] — a bench is not an in-range source, and
    /// giving it one would put a fictitious row on every yield readout in the game — so it is never
    /// part of `assigned` and has to be subtracted on its own.
    pub benched: u32,
}

impl BandWorkforce {
    /// Read the three numbers off a band's components. Each is optional because a hand-rolled
    /// fixture (and a band spawned before a component existed) may carry none of them; an absent
    /// component reads as zero, which is the same fallback the rest of the band pass takes.
    pub fn resolve(
        cohort: Option<&PopulationCohort>,
        allocation: Option<&LaborAllocation>,
        bench: Option<&BandBench>,
    ) -> Self {
        Self {
            pool: cohort.map(|c| available_workers(c.working)).unwrap_or(0),
            // **Every committed hand, not just the take crews** — a band's builders and keepers
            // are as unavailable as its gatherers, and `idle()` would lie about a band mid-Cultivate
            // if it counted only the take (`docs/plan_standing_upkeep.md` §2.2).
            assigned: allocation.map(|a| a.assigned_total()).unwrap_or(0),
            benched: bench.map(|b| b.workers).unwrap_or(0),
        }
    }

    /// **Free hands: staffed on neither the range nor the bench.** The published
    /// `PopulationCohortState.idleWorkers`, and the number every "n idle of m" readout in the game
    /// shows.
    pub fn idle(&self) -> u32 {
        self.pool
            .saturating_sub(self.assigned)
            .saturating_sub(self.benched)
    }

    /// **The ceiling an `assign_labor` clamps against** — the pool the *range* may spend, which the
    /// bench's crew has already left. Passed to [`LaborAllocation::set_assignment`], which nets out
    /// the other assignments itself (and lets a re-staffed source reuse its own crew), so this must
    /// NOT have `assigned` taken off it.
    pub fn assignable(&self) -> u32 {
        self.pool.saturating_sub(self.benched)
    }

    /// **The ceiling a bench command clamps against** — `idle` plus the crew already at the bench,
    /// because a band's own crew stays put while its job is swapped and must not be counted twice.
    pub fn benchable(&self) -> u32 {
        self.pool.saturating_sub(self.assigned)
    }
}

/// A band's partition of its working-age pool across labor demands. Replaces the retired
/// single-task model (`HarvestAssignment`/`ScoutAssignment`/`FaunaPursuit`): a band now draws from
/// many sources at once, with the invariant `Σ assignments.workers ≤ available_workers(working)`.
/// Unassigned workers are **idle** — they eat but produce nothing (no auto-forage).
#[derive(Component, Debug, Clone, Default)]
pub struct LaborAllocation {
    pub assignments: Vec<LaborAssignment>,
    /// Per-turn, per-source yield telemetry — one entry per `assignments` in the **same iteration
    /// order** (so the snapshot zips by index). Rebuilt from scratch each turn in
    /// `advance_labor_allocation`, and on the client wire as the per-row yield fields of
    /// `LaborAssignmentState`. **Excluded from equality** (see the manual `PartialEq` below) so
    /// telemetry can never perturb a comparison of two allocations' intent.
    pub last_yields: Vec<SourceYield>,
    /// **The food this band actually PAID for pen feed this turn** — the summed `paid` returned by
    /// `LocalStore::take` in the corral-tend branch of `advance_labor_allocation`, across every pen it
    /// keeps. The *real debit*, not the demanded amount: a band that could only part-pay records only
    /// what it handed over (and its herds starve for the rest).
    ///
    /// **Why it must exist.** A pen's feed is taken straight off `cohort.stores`, so it appears in
    /// **neither** `food_income` (Σ per-source `actual`) nor `food_consumption` (the food the
    /// *people* actually ate, `PopulationCohort::last_food_consumption`). Without exporting it the
    /// band's net-food readout overstates the surplus by
    /// exactly the upkeep and the player watches the larder drain with no explanation. Exported as
    /// `PopulationCohortState.pen_feed_upkeep` so the client can render "my people ate X" and "my
    /// animals ate Y" as **separate lines** (deliberately NOT folded into `food_consumption` — that
    /// separation is the readout the corral arc exists to give), and so the sim, not the client, is the
    /// one doing the arithmetic. It closes the identity
    ///
    /// ```text
    /// larder_delta == food_income − food_consumption − pen_feed_upkeep
    /// ```
    ///
    /// which `core_sim/tests/fauna_husbandry.rs` pins against a real turn.
    ///
    /// Same treatment as `last_yields`: rebuilt from scratch each turn, and **excluded from equality**
    /// below so it can never perturb a comparison of two allocations' intent.
    pub last_pen_feed_upkeep: f32,
    /// **The food this band forfeited to a predator raid this turn** (Predators Phase 3) — the actual
    /// `LocalStore::take` debit `advance_predator_raids` levies on a **casualty-causing** raid (the
    /// band's people were defending or fleeing, not gathering, so they forfeit
    /// `predators.raid_yield_forfeit_fraction` of that turn's food income, capped at the larder). `0.0`
    /// on a band not raided this turn.
    ///
    /// Exported as `PopulationCohortState.raid_forfeit` — a negative food-ledger row, the raid twin of
    /// `last_pen_feed_upkeep`. It is a **past** larder debit (a stochastic event), so it extends only the
    /// reconciliation identity, NOT the forward runway drain:
    ///
    /// ```text
    /// larder_delta == food_income − food_consumption − pen_feed_upkeep − raid_forfeit
    /// ```
    ///
    /// Same treatment as `last_pen_feed_upkeep`: reset then re-levied each turn by
    /// `advance_predator_raids`, and **excluded from equality** below.
    pub last_raid_forfeit: f32,
    /// **Food this band RECEIVED from another band this turn** — supply-network balancing, an
    /// arriving trade shipment, or an expedition of its own handing its pack back.
    ///
    /// # It closes a hole the two terms above left open
    ///
    /// Food that crosses between two larders passes through **neither** `food_income` (Σ per-source
    /// `actual`, which is what this band's own workers produced) nor `food_consumption` (what its
    /// people ate) — exactly the situation [`Self::last_pen_feed_upkeep`] and
    /// [`Self::last_raid_forfeit`] were each minted for. The identity is therefore
    ///
    /// ```text
    /// larder_delta == food_income − food_consumption − pen_feed_upkeep − raid_forfeit
    ///                 + transfer_received − transfer_sent
    /// ```
    ///
    /// pinned against real turns by `integration_tests/tests/transfer_food_ledger.rs`.
    ///
    /// **ONE pair of terms for every band-to-band movement, not one per producer.** A supply-network
    /// transfer and a trade shipment are the same fact — *food that crossed between bands outside
    /// income and consumption* — and minting a term per mechanism is how a ledger acquires five
    /// fields that answer one question.
    ///
    /// **Two named magnitudes rather than one signed net**, matching the style of the two terms
    /// above: a band that both sends and receives in one turn is doing something, and a signed net
    /// would render that as nothing happening.
    ///
    /// # The window is the SNAPSHOT window, not the turn
    ///
    /// Unlike its two siblings, this pair has writers **outside** `run_turn`: a
    /// `send_trade_expedition` (or `send_expedition`) command debits the larder when it is applied,
    /// which is between one capture and the next. So it accumulates — every writer **adds** — and
    /// `systems::reset_transfer_ledger` clears it in the Snapshot stage *after* the capture has read
    /// it. That makes the window exactly the interval a client sees between two published frames,
    /// which is the interval its `larder_delta` measures.
    ///
    /// Excluded from equality below, like the rest of the per-turn telemetry.
    pub last_transfer_received: f32,
    /// **Food this band GAVE UP to another band this turn** — supply-network balancing, or a party
    /// drawn off it walking away with cargo and provisions. See [`Self::last_transfer_received`] for
    /// the identity both terms close and why there is one pair rather than one per producer.
    pub last_transfer_sent: f32,
    /// **HOW THIS BAND SPLITS A MAINTENANCE POOL IT CANNOT STRETCH** — the player's own choice
    /// between *everything degrades a little* and *the biggest investments stay whole*
    /// ([`crate::intensification::UpkeepFundMode`], `docs/plan_standing_upkeep.md` §2.5).
    ///
    /// **It is intent, not telemetry**, so — unlike every field above it — it is part of this
    /// allocation's *identity* and rides the manual `PartialEq` below. It is `SimState` by the same
    /// route the assignments are: `capture_sim_state` clones the whole component.
    pub upkeep_fund_mode: crate::intensification::UpkeepFundMode,
    /// **THE BUILDS THIS BAND HAS DECLARED, IN THE ORDER IT WILL RAISE THEM**
    /// (`docs/plan_standing_upkeep.md` §2.5). The whole [`LaborTarget::Builders`] pool goes on
    /// `build_queue[0]` until that entry's meter fills, then on the next.
    ///
    /// # THE QUEUE IS THE DECLARATION
    ///
    /// It replaces the retired per-row `improvement`, and there is exactly **one** authority
    /// (§2.4): two would drift, and the meter would go on creating entries nobody asked for. What
    /// stays derived is *which rung* an entry names — `forage::patch_build_verb` /
    /// `fauna::herd_build_verb` read the meters, so an entry on ground that has moved on is **dead
    /// rather than stalled**, and the declaration answers only for a meter at zero.
    ///
    /// **Nothing enrols itself, and a rung that erodes back below its cost is not re-adopted.**
    /// Repairing it is a fresh decision the player makes by queueing it again — which is what keeps
    /// a one-percent-eroded Field from displacing the build they actually ordered off the head.
    ///
    /// **It is intent, not telemetry**, so it rides the manual `PartialEq` below beside the
    /// assignments and the fund mode, and it is `SimState` by the same route they are
    /// (`capture_sim_state` clones the whole component).
    pub build_queue: Vec<BuildQueueEntry>,
}

/// Equality is **intent only** — two allocations with equal `assignments`, `upkeep_fund_mode` and
/// `build_queue` are equal regardless of the derived `last_yields` telemetry. This keeps the
/// per-turn telemetry out of any state comparison (it is deliberately not part of the assignment's
/// identity).
///
/// ⛔ **The queue counts, and its ORDER counts.** It is what the band is building and in what order,
/// which is as much the player's intent as the head-counts are: leave it out and two allocations
/// with different queues compare equal, so the rollback record and the command no-op guard both
/// report *nothing changed* on the one input the whole funding rule reads.
impl PartialEq for LaborAllocation {
    fn eq(&self, other: &Self) -> bool {
        self.assignments == other.assignments
            && self.upkeep_fund_mode == other.upkeep_fund_mode
            && self.build_queue == other.build_queue
    }
}

/// **WHICH SOURCE A BUILD QUEUE ENTRY NAMES** — a patch by its tile, a herd by its id.
///
/// **Deliberately NOT a [`LaborTarget`]**, though the two name the same things. A target carries the
/// take crew's **floor** and its **species selection**, which are facts about *gathering* the
/// source and not about building on it; keying the queue by one would make "is this the entry for
/// that patch" depend on a stance the player might have changed since, and would invite the floor
/// to be read at a build site that has no business with it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BuildSource {
    /// A forage patch, by tile.
    Patch(UVec2),
    /// A fauna herd, by id.
    Herd(String),
}

impl BuildSource {
    /// The source a labor row names, or `None` for a band-wide standing role — the one mapping from
    /// the take's vocabulary to the queue's.
    pub fn of(target: &LaborTarget) -> Option<BuildSource> {
        match target {
            LaborTarget::Forage { tile, .. } => Some(BuildSource::Patch(*tile)),
            LaborTarget::Hunt { fauna_id, .. } => Some(BuildSource::Herd(fauna_id.clone())),
            LaborTarget::Scout
            | LaborTarget::Warrior
            | LaborTarget::Agriculture
            | LaborTarget::Husbandry
            | LaborTarget::Builders => None,
        }
    }

    /// Whether this source is the one `target` works.
    pub fn names(&self, target: &LaborTarget) -> bool {
        match (self, target) {
            (BuildSource::Patch(tile), LaborTarget::Forage { tile: other, .. }) => tile == other,
            (BuildSource::Herd(id), LaborTarget::Hunt { fauna_id, .. }) => id == fauna_id,
            _ => false,
        }
    }
}

/// **WHAT AN ENTRY SAYS IT IS RAISING.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildJob {
    /// One of the four rung verbs. It is the **declaration**, and it answers only while the meter it
    /// names is at zero: `forage::patch_build_verb` / `fauna::herd_build_verb` derive the live rung
    /// from the meters otherwise, exactly as they already did.
    Rung(Improvement),
    /// **The pen ring** — `extend_pen`. It is fencing work on the `animal:pen` rung, but the pen is
    /// already built, so there is no meter for a verb to name and the derived rung cannot say it.
    /// A queue kind states it instead, which is what the ring lost when it stopped naming a crew.
    ExtendPen,
}

impl BuildJob {
    /// **WHERE THE PLAYER SAID THE LAND SHOULD END UP** — the rung this entry climbs *to*, which is
    /// not the same thing as the rung it is raising this turn.
    ///
    /// # A QUEUE ENTRY NAMES A DESTINATION, NOT A RUNG
    ///
    /// The four verbs always were destinations — `cultivate` means *take it to Cultivated*, `sow`
    /// means *take it to Field* — and with one position per source (`docs/plan_standing_upkeep.md`
    /// §2.8) that reading becomes literal: an entry lays **every leg between where the source stands
    /// and here**, in order, and stays at the head until it arrives. So `sow` on untended ground is
    /// two legs and costs the whole branch, where it used to skip the tended rung.
    ///
    /// # ⛔ IT IS DERIVED, NOT STORED, AND THAT IS DELIBERATE
    ///
    /// A destination stored beside `declared` would be a **second authority for one fact**: the map
    /// from verb to rung is total and exhaustive ([`crate::intensification::RungKey::built_by`]), so
    /// the two could only ever agree or drift, and this arc has already shipped three defects of
    /// exactly that shape. If a future order ever names a rung its verb does not determine — *"take
    /// it to rung 4"* with one verb serving several — this becomes a field, and the one call site
    /// that reads it is here.
    pub fn destination(self) -> RungKey {
        match self {
            BuildJob::Rung(improvement) => RungKey::built_by(improvement),
            // A ring is fencing work on a pen that already stands, so its destination is the rung it
            // widens: there is no leg to climb, only more of the one the source is already on.
            BuildJob::ExtendPen => RungKey::AnimalPen,
        }
    }
}

/// One declared build: which source, **where the player said the land should end up**
/// ([`BuildJob::destination`]), and **what this job is raised with** ([`Self::kit`]).
#[derive(Debug, Clone, PartialEq)]
pub struct BuildQueueEntry {
    pub source: BuildSource,
    pub declared: BuildJob,
    /// **The kit the player NAMED for THIS job, or `None` for "whatever this entry's web wants"** —
    /// the same distinction [`LaborAllocation::named_kit_on`] draws for a singleton role, moved to
    /// the one place the builders' default actually varies (`docs/plan_standing_upkeep.md` §4.7a ②).
    ///
    /// A single stored id per **band** cannot be right for both food webs — a hoe for a Cultivate,
    /// hurdles for a `Tame` — so the derivation is per **entry**, and `None` is what leaves it
    /// reachable. An absent choice already means *"the job's default"* everywhere else; this is what
    /// lets that default vary per job.
    ///
    /// **`none` is a real selection and answers `Some`**, which is what preserves deliberately
    /// sending the builders out bare-handed on one job to conserve gear.
    pub kit: Option<crate::equipment_config::KitChoice>,
}

impl LaborAllocation {
    /// Total workers currently staffed across all assignments.
    /// **EVERY HAND THIS BAND HAS COMMITTED** — every row's take, across the worked sources **and**
    /// the band-wide standing roles, which are rows in the same list
    /// ([`LaborAssignment::staffed_total`]).
    ///
    /// **There is ONE sum, and it counts every row, because they all draw on ONE finite band**
    /// (`docs/plan_standing_upkeep.md` §2.2). That is the whole of what makes the player's split a
    /// decision: if the builders could be staffed out of hands the band does not have, there is no
    /// competition between the activities and no opportunity cost — a band of five would produce
    /// fifteen worker-turns.
    ///
    /// **"No cap" means no cap on ONE ROLE, never a licence to exceed the pool.** Fifty builders may
    /// finish a Cultivate in a turn; fifty hands a band of five does not have may not.
    /// **`assign_labor` is the one enforcement** — it *clamps* each row against the band's idle
    /// hands — and [`Self::normalize`] enforces the same bound against a band that *shrank*. The
    /// build verbs' own affordability refusal retired with the crew they used to name (§2.5).
    pub fn assigned_total(&self) -> u32 {
        self.assignments.iter().map(|a| a.staffed_total()).sum()
    }

    /// Total workers staffed on the given source (matched by [`LaborTarget::same_source`], so a
    /// singleton role like `Scout`/`Warrior` sums its one assignment). Used by the visibility pass
    /// to read a band's Scout head-count for its sight-range bonus.
    pub fn workers_on(&self, target: &LaborTarget) -> u32 {
        self.assignments
            .iter()
            .filter(|a| a.target.same_source(target))
            .map(|a| a.workers)
            .sum()
    }

    /// **Total workers staffed on a JOB**, summed across every source of it — the head count a
    /// job's gear has to cover ([`crate::equipment_config::EquipmentConfig::coverage`]).
    ///
    /// **A job rather than a source, because gear is owned by the BAND.** Two hunt assignments on
    /// two herds draw their spears from one ledger, so *"how many of this band's hunters is there a
    /// spear for"* is a question about the job's whole head count and not about either herd's crew.
    /// [`Self::workers_on`] answers the per-source question and is not this.
    pub fn workers_on_job(&self, job: crate::equipment_config::KitJob) -> u32 {
        self.assignments
            .iter()
            .filter(|assignment| assignment.target.kit_job() == job)
            .map(|assignment| assignment.workers)
            .sum()
    }

    /// **The kit staffed on a SINGLETON source**, resolved through the same seam every priced row
    /// reads ([`LaborAssignment::kit_choice`]) — or the job's default when the role is unstaffed.
    ///
    /// For the two band-wide roles only: they are singletons, so "the kit on this role" is a
    /// question with one answer, where a Forage/Hunt target could be staffed twice on the same tile
    /// only by being the same assignment. It exists because the two consumers of those roles live
    /// **outside** the labor loop — `calculate_visibility` and `advance_predator_raids` — and each
    /// holds a `LaborAllocation` rather than an assignment.
    ///
    /// **An unstaffed role still resolves its default rather than the empty kit**, which costs
    /// nothing (both consumers gate on the head-count first) and keeps a zero-worker row from
    /// answering a different tier than the same row with one worker on it.
    pub fn kit_on(
        &self,
        target: &LaborTarget,
        config: &crate::equipment_config::EquipmentConfig,
    ) -> crate::equipment_config::KitChoice {
        self.assignments
            .iter()
            .find(|a| a.target.same_source(target))
            .map(|a| a.kit_choice(config))
            .unwrap_or_else(|| config.default_kit(target.kit_job()))
    }

    /// **The kit the player NAMED on a singleton role, or `None` for "whatever the job wants"** —
    /// [`Self::kit_on`]'s other half, and the distinction the two **keeping** pools' per-web
    /// derivation turns on (`docs/plan_standing_upkeep.md` §4.8).
    ///
    /// `kit_on` collapses *"the player chose `none`"* and *"the player chose nothing"* into one
    /// answer, which is right for every role whose default is a single id. A keeping role's default
    /// is the **roster's** answer for its web, so the pool needs to know whether the row carries a
    /// real selection to override that derivation with. An absent choice already means *"the job's
    /// default"* everywhere else; this is what lets that default be derived.
    ///
    /// ⛔ **The `builders` row is NOT one of these.** Its kit is per **queue entry**
    /// ([`BuildQueueEntry::kit`]), because one stored id per band cannot be right for both webs —
    /// see [`Self::builders_kit`].
    ///
    /// **`none` is a real selection and answers `Some`**, which is what preserves deliberately
    /// sending the builders out bare-handed to conserve gear.
    pub fn named_kit_on(&self, target: &LaborTarget) -> Option<crate::equipment_config::KitChoice> {
        self.assignments
            .iter()
            .find(|a| a.target.same_source(target))
            .and_then(|a| a.kit.clone())
    }

    /// **THE WEB THE BAND'S BUILDERS ARE ACTUALLY WORKING ON** — the head entry's, since the whole
    /// pool goes on the head. `None` when the queue is empty, which is *"nothing is being raised"*
    /// rather than a web.
    pub fn head_build_branch(&self) -> Option<crate::intensification::RungBranch> {
        self.build_queue.first().map(|entry| match entry.source {
            BuildSource::Patch(_) => crate::intensification::RungBranch::Plant,
            BuildSource::Herd(_) => crate::intensification::RungBranch::Animal,
        })
    }

    /// **The kit this band's builders are working with**, resolved through the one seam
    /// ([`crate::equipment_config::EquipmentConfig::builders_kit_for`]) — the **head entry's** own
    /// choice, else the kit the roster says that entry's web wants, else the job default.
    ///
    /// **It reads the ENTRY, never the `builders` row.** The whole pool goes on the head, so *"what
    /// are the builders holding"* is a question about the job they are standing on; a kit stored on
    /// the row would be one per **band** and would pin the animal web's tool onto a plant build with
    /// no way back (`docs/plan_standing_upkeep.md` §4.7a ②).
    ///
    /// **The wire states this rather than a stored id**, on `kit_id`'s existing rule (*"the wire
    /// states the kit rather than 'the player named none'"*): the builders' default is per entry, so
    /// an entry that named nothing would otherwise publish `none` while the pool was out with
    /// hurdles.
    pub fn builders_kit(
        &self,
        config: &crate::equipment_config::EquipmentConfig,
    ) -> crate::equipment_config::KitChoice {
        let head = self.build_queue.first();
        config.builders_kit_for(
            head.and_then(|entry| entry.kit.as_ref()),
            self.head_build_branch(),
        )
    }

    /// Keep the derived `last_yields` the same length as `assignments` — the snapshot **zips the two
    /// by index**, so a mutation that adds/removes an assignment without touching the telemetry would
    /// hand one source's yield row to another. Padding with [`SourceYield::ZERO`] is the correct
    /// default: a source with no telemetry has produced nothing yet.
    fn align_yields(&mut self) {
        self.last_yields
            .resize(self.assignments.len(), SourceYield::ZERO);
    }

    /// Set/replace the **take** crew for `target`, keeping `Σ ≤ available`. An over-budget request
    /// is **clamped** to the free headroom (not rejected). Returns the worker count actually applied
    /// so the caller can report a clamp.
    ///
    /// # `workers == 0` UNSTAFFS THE TAKE; IT DOES NOT ERASE THE BAND FROM THE SOURCE
    ///
    /// A **role** row ([`LaborTarget::is_source`] `== false`) *is* its head count, so zero drops it —
    /// that is what *"stop scouting"* means and there is nothing else for the row to carry.
    ///
    /// A **source** row is the band's *holding* of that patch or herd, and the take crew is only
    /// what the band is **gathering** there (`docs/plan_standing_upkeep.md` §2.2). So an existing
    /// source row **survives at zero**, carrying its kit, and the sim retires it once the ground has
    /// nothing left to hold (`advance_labor_allocation`).
    ///
    /// Dropping it here is what made a finished Field ineligible for its own web's keeping pool: a
    /// band that moved its gatherers to a richer patch lost the row, so the Field contributed no
    /// demand to `agriculture`, drew no share, and bled its full rate with keepers standing idle in
    /// the role and no command that could direct them at it. **A source row is never created at
    /// zero** — `assign_labor … 0` on ground the band never worked still says nothing.
    ///
    /// The touched source's yield telemetry is dropped alongside its assignment and a freshly-staffed
    /// source gets a [`SourceYield::ZERO`] row, which the command handler immediately overwrites with
    /// the source's pre-commit forecast (`set_source_yield`) so the client never displays `+0.00` for
    /// an assignment that will in fact produce next turn.
    ///
    /// **A build already in flight on this source SURVIVES**, and now does so by construction: the
    /// build lives in the band's [`Self::build_queue`], which this function does not touch at all
    /// (`docs/plan_standing_upkeep.md` §2.5). Editing the stance or the take crew is a stance-side
    /// edit; it must not re-assert — or silently drop — a build the player committed 25 turns to.
    /// Walking away from one is `unqueue` (drop the declaration) or `abandon` (put the whole source
    /// down), which is where §2.4/§2.5 put it.
    pub fn set_assignment(
        &mut self,
        target: LaborTarget,
        workers: u32,
        available: u32,
        kit: Option<crate::equipment_config::KitChoice>,
    ) -> u32 {
        // Free headroom excludes any existing assignment on the same source (it is being replaced).
        // **Every hand on every OTHER row** — one band, one pool
        // (`docs/plan_standing_upkeep.md` §2.2).
        //
        // **There is nothing of this source's own to give back any more.** The clause that netted
        // out its build crew retired with that crew (§2.5): a row carries only the take, so its own
        // whole staffing is what is being replaced.
        let others: u32 = self
            .assignments
            .iter()
            .filter(|a| !a.target.same_source(&target))
            .map(|a| a.staffed_total())
            .sum();
        let headroom = available.saturating_sub(others);
        let applied = workers.min(headroom);
        self.align_yields();
        // Drop any prior assignment on this source (and its now-stale telemetry row), then re-add if
        // non-zero (captures a new stance).
        // The kit the row was already working under. It is re-used only when the take crew goes to
        // **zero**, where the caller resolves no kit at all: an unstaffed row must not silently
        // forget what it was equipped with.
        let mut standing_kit = None;
        let mut had_row = false;
        if let Some(idx) = self
            .assignments
            .iter()
            .position(|a| a.target.same_source(&target))
        {
            standing_kit = self.assignments[idx].kit.clone();
            had_row = true;
            self.assignments.remove(idx);
            self.last_yields.remove(idx);
        }
        // **A source the band already held keeps its row at zero** — see the doc above. A role's row
        // goes, and a source the band never worked is not conjured into existence by an unassign.
        let keep_holding = applied == 0 && had_row && target.is_source();
        if applied > 0 || keep_holding {
            self.assignments.push(LaborAssignment {
                target,
                workers: applied,
                // **The kit is a property of the ORDER, so a re-assignment replaces it.** Naming a
                // kit is the whole of what this command decides about tier; silently keeping the
                // previous one would make the selection unchangeable.
                //
                // **A row held at zero is not an order about tier**, so it keeps what it had: the
                // command deliberately resolves no kit when it is unstaffing, and writing that
                // `None` onto a surviving row would forget the tier the band was working at.
                kit: if keep_holding { standing_kit } else { kit },
            });
            self.last_yields.push(SourceYield::ZERO);
        }
        applied
    }

    /// **Remove a source's row outright**, telemetry included — the deliberate end of a band's
    /// holding, as opposed to [`Self::set_assignment`]'s zero, which only unstaffs the take.
    ///
    /// It exists because *"is there still anything of ours here"* is a question about the **ground**
    /// (`systems::source_has_a_meter_at_risk`), which this type cannot see. The command asks it the
    /// moment the take crew goes to zero, so unstaffing a wild stand clears the row on the spot
    /// rather than leaving a `+0.00` row to age out on the next turn; the labor pass asks it again
    /// every turn, for the holding whose meter finally rots away.
    ///
    /// **The declaration goes with the holding.** An entry requires a row (§3.2), so dropping the
    /// row drops the entry on the spot rather than leaving the builders funding ground the band no
    /// longer holds until the next turn's prune catches it.
    ///
    /// Returns whether a row was found.
    pub fn drop_source_row(&mut self, target: &LaborTarget) -> bool {
        self.align_yields();
        let Some(idx) = self
            .assignments
            .iter()
            .position(|a| a.target.same_source(target))
        else {
            return false;
        };
        self.assignments.remove(idx);
        self.last_yields.remove(idx);
        let _ = self.prune_build_queue();
        true
    }

    /// **Put a source in this band's build queue, or restate what it is building there** — the
    /// whole of what the five build verbs do (`docs/plan_standing_upkeep.md` §2.5). Returns `false`
    /// when the band has no row for that source, which the caller reports as *"staff it first"*.
    ///
    /// # RE-ISSUING A VERB KEEPS THE ENTRY'S PLACE IN THE LINE
    ///
    /// At most **one** entry per source per band, so a second verb on an already-queued source
    /// replaces [`BuildQueueEntry::declared`] **in place**. Changing `cultivate` → `sow` is a
    /// correction, and costing the player their position for it would make the queue punish the
    /// thing it exists to let them steer.
    ///
    /// **AND IT KEEPS THE ENTRY'S KIT**, for the same reason it keeps its place: re-declaring is a
    /// correction to *what* is being raised, and silently clearing the tool the player picked for
    /// that job is the same loss as sending it to the back of the line. A new entry starts with
    /// `kit: None`, i.e. on its own web's derivation.
    ///
    /// # AN ENTRY REQUIRES A ROW
    ///
    /// The row is the band's *holding* of the source; an entry against ground the band does not work
    /// would draw the whole builders pool onto something no crew is standing on. Nothing enrols
    /// itself either — a meter is never the thing that creates an entry (§2.4).
    pub fn enqueue_build(&mut self, source: BuildSource, declared: BuildJob) -> bool {
        if !self.holds_build_source(&source) {
            return false;
        }
        match self
            .build_queue
            .iter_mut()
            .find(|entry| entry.source == source)
        {
            Some(entry) => entry.declared = declared,
            None => self.build_queue.push(BuildQueueEntry {
                source,
                declared,
                kit: None,
            }),
        }
        true
    }

    /// **Name the kit ONE queued job is raised with** — the `build_kit` command's whole effect
    /// (`docs/plan_standing_upkeep.md` §4.7a ②). Returns whether an entry was there to set it on.
    ///
    /// `None` **clears the override** back to the entry's own web derivation, which is the existing
    /// *"an absent `kitId` means the job's default"* rule and is what lets a client say *"back to
    /// default"* with no new vocabulary. `Some(<the bare kit>)` is a real selection and stays.
    ///
    /// Nothing is invented for a source with no entry: a kit is a property of a declared job, and
    /// minting an entry here would enrol a build the player never declared — the same refusal
    /// [`Self::move_build_entry`] makes.
    pub fn set_build_entry_kit(
        &mut self,
        source: &BuildSource,
        kit: Option<crate::equipment_config::KitChoice>,
    ) -> bool {
        let Some(entry) = self
            .build_queue
            .iter_mut()
            .find(|entry| &entry.source == source)
        else {
            return false;
        };
        entry.kit = kit;
        true
    }

    /// **Take a source out of this band's build queue**, leaving the row, its take crew, its kit and
    /// the meter exactly as they are — the `unqueue` command's whole effect. Returns whether an
    /// entry was there.
    ///
    /// It is the **undo a declaration never had**. The verbs used to be the only way to state one
    /// and had no zero that cleared it, so an unwanted `cultivate` was stuck on the row for the life
    /// of the band; the queue makes withdrawing it an ordinary list edit.
    pub fn unqueue_build(&mut self, source: &BuildSource) -> bool {
        let Some(idx) = self
            .build_queue
            .iter()
            .position(|entry| &entry.source == source)
        else {
            return false;
        };
        self.build_queue.remove(idx);
        true
    }

    /// **Move a queued source to `position`** (0-based, clamped to the queue's length) — the
    /// `build_order` command. The queue's defining input: with the whole pool on the head, the order
    /// *is* the funding decision (§2.5).
    ///
    /// Returns `false` when the source is not queued; there is nothing to move and inventing an
    /// entry here would enrol a build the player never declared.
    pub fn move_build_entry(&mut self, source: &BuildSource, position: usize) -> bool {
        let Some(from) = self
            .build_queue
            .iter()
            .position(|entry| &entry.source == source)
        else {
            return false;
        };
        let entry = self.build_queue.remove(from);
        let to = position.min(self.build_queue.len());
        self.build_queue.insert(to, entry);
        true
    }

    /// This source's 0-based place in the queue, or `None` when it is not in it — the wire's
    /// `buildQueuePosition`, and what the funding pass tests the head against.
    pub fn build_queue_position(&self, source: &BuildSource) -> Option<usize> {
        self.build_queue
            .iter()
            .position(|entry| &entry.source == source)
    }

    /// The entry naming this source, if the band has one.
    pub fn build_queue_entry(&self, source: &BuildSource) -> Option<&BuildQueueEntry> {
        self.build_queue
            .iter()
            .find(|entry| &entry.source == source)
    }

    /// **Drop every entry whose source this band no longer works** — the per-turn sweep that makes
    /// *"an entry requires a row"* an invariant rather than a rule five seams have to remember
    /// (§3.2). A row dies on a lapse, a drop, a `cancel_order`, a `normalize` eviction and the
    /// turn's holding retirement; each of those could clear the entry itself, and one of them
    /// eventually would not.
    ///
    /// Returns the entries it dropped, so a caller that wants to narrate the loss can.
    pub fn prune_build_queue(&mut self) -> Vec<BuildQueueEntry> {
        let mut dropped = Vec::new();
        let held: Vec<bool> = self
            .build_queue
            .iter()
            .map(|entry| self.holds_build_source(&entry.source))
            .collect();
        let mut index = 0;
        self.build_queue.retain(|entry| {
            let keep = held[index];
            index += 1;
            if !keep {
                dropped.push(entry.clone());
            }
            keep
        });
        dropped
    }

    /// Whether this band has a row on the named source — the membership test the queue is gated on.
    fn holds_build_source(&self, source: &BuildSource) -> bool {
        self.assignments
            .iter()
            .any(|assignment| source.names(&assignment.target))
    }

    // **RETIRED: `add_role_workers`** — put more hands on a band-wide standing role, creating its
    // row if the band had none. Its only caller was the completion hand-off that moved a finished
    // build's crew onto its web's keeping role, and that hand-off is retired
    // (`docs/plan_standing_upkeep.md` §2.3): the keeping bill starts at the first work banked, so
    // the failure it guarded against — a brand-new improvement decaying on turn one — cannot happen.
    // **Adding hands off-command is what it existed for**, and with nothing taking hands *off* an
    // allocation to hand on, an unclamped add would be hands the band does not have. Every remaining
    // path staffs a role through [`Self::set_assignment`], where the band's headroom is enforced.

    // **RETIRED: `set_maintain_workers`** — the `maintain` command's whole effect.
    //
    // The keeping is a band-level standing role now (`docs/plan_standing_upkeep.md` §2.5), so it is
    // staffed by `assign_labor <faction> <band> agriculture|husbandry <workers>` through
    // [`Self::set_assignment`] like any other row, and `0` still means *"stop maintaining"* — for the
    // whole web rather than for one source.

    /// Overwrite one source's derived yield telemetry row (assign-time **forecast seeding**: the row
    /// is set to what the source is expected to produce next turn, so the map annotation and the band
    /// panel show the real number the moment workers are committed instead of `+0.00`). A no-op when
    /// the source is not staffed.
    pub fn set_source_yield(&mut self, target: &LaborTarget, yields: SourceYield) {
        self.align_yields();
        if let Some(idx) = self
            .assignments
            .iter()
            .position(|a| a.target.same_source(target))
        {
            self.last_yields[idx] = yields;
        }
    }

    /// Trim assignments so `Σ ≤ available` (called each turn in case `working` shrank). Reduces
    /// from the last assignment(s) first, dropping any that reach zero.
    ///
    /// **It trims ONE allocation per row, because a row now carries one** — the take crew. The
    /// build→take shedding order went with the per-source build crew
    /// (`docs/plan_standing_upkeep.md` §2.5); the building and the keeping are **rows** of their own
    /// ([`LaborTarget::Builders`] / [`LaborTarget::Agriculture`] / [`LaborTarget::Husbandry`]), so
    /// where each falls in the shedding order is where the player put it in the list.
    ///
    /// **Returns the assignments it dropped, because dropping one silently was a defect.** Every
    /// other path that gives up work tells the player (the out-of-range Forage lapse, the hunt leash
    /// lapse, `cancel_order`); this one said nothing at all, and a tended patch that quietly ended
    /// up with zero workers is what it looks like from the outside. The caller owns the feed line —
    /// `LaborAllocation` has no event log and should not grow one — so this hands back the evidence
    /// and `advance_labor_allocation` narrates it.
    ///
    /// **A dropped row takes its queue entry with it**, on the next
    /// [`Self::prune_build_queue`]: an entry requires a row (§3.2), and a band that has lost the
    /// people to gather a patch has not thereby declared it is still building one.
    ///
    /// **They are dropped, not zeroed, and that is deliberate.** A zero-worker assignment is this
    /// system's own word for *abandon it*, so zeroing would keep a row the map still renders as
    /// worked while paying nothing — the same "correct `+0.00` forever" state the out-of-range lapse
    /// exists to avoid. Dropping returns the slot to the pool and matches every other give-up path.
    #[must_use = "a dropped assignment must be announced — see the doc comment"]
    pub fn normalize(&mut self, available: u32) -> Vec<LaborAssignment> {
        let mut dropped = Vec::new();
        let mut total = self.assigned_total();
        while total > available {
            let excess = total - available;
            let Some(last) = self.assignments.last_mut() else {
                break;
            };
            if last.workers > excess {
                last.workers -= excess;
            } else {
                // Nothing of this row survives — drop it whole, so no source is left rendered as
                // worked by a crew of nobody.
                if let Some(assignment) = self.assignments.pop() {
                    dropped.push(assignment);
                }
            }
            total = self.assigned_total();
        }
        self.align_yields();
        dropped
    }

    /// Clear every assignment (the repurposed `cancel_order` — band goes fully idle).
    ///
    /// **The build queue goes with them**: an entry requires a row (§3.2), and a band with no rows
    /// at all holds nothing to build.
    pub fn clear(&mut self) {
        self.assignments.clear();
        self.last_yields.clear();
        self.build_queue.clear();
    }

    /// Drop every assignment `keep` rejects, retaining the rest — the scoped counterpart of
    /// [`Self::clear`] (`cancel_order … work` clears the worked sources, `… roles` the standing
    /// roles). Returns the workers freed, so the caller can report what it unassigned.
    ///
    /// `last_yields` is realigned first and then filtered **by the same index**, because the
    /// snapshot zips the two vectors positionally: a partial clear that dropped an assignment while
    /// leaving its telemetry row behind would re-attribute that row to the next source.
    pub fn clear_kinds(&mut self, keep: impl Fn(&LaborTarget) -> bool) -> u32 {
        self.align_yields();
        // Decide once per index, then apply the *same* mask to both vectors so they stay aligned.
        let keep_mask: Vec<bool> = self
            .assignments
            .iter()
            .map(|assignment| keep(&assignment.target))
            .collect();
        let freed: u32 = self
            .assignments
            .iter()
            .zip(keep_mask.iter())
            .filter(|(_, retain)| !**retain)
            .map(|(assignment, _)| assignment.workers)
            .sum();
        let mut index = 0;
        self.assignments.retain(|_| {
            let retain = keep_mask[index];
            index += 1;
            retain
        });
        let mut index = 0;
        self.last_yields.retain(|_| {
            let retain = keep_mask[index];
            index += 1;
            retain
        });
        // A cleared row takes its declaration with it — the same rule the turn's prune enforces,
        // applied on the spot so `cancel_order … work` does not leave the band funding a build on
        // ground it no longer holds.
        let _ = self.prune_build_queue();
        freed
    }
}

/// A pending `move_band` order: the band advances toward `target` at
/// `band_move_tiles_per_turn`/turn, updating `current_tile`/`home` until it arrives, then the
/// component is removed. On the client wire as `PopulationCohortState.is_traveling` +
/// `travel_target_x`/`travel_target_y`, so the client can draw the destination it is walking to.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BandTravel {
    pub target: UVec2,
}

/// **THE floor a fresh assignment gets when the player named none** — `0.50`, the food peak, so the
/// common case is one click and the default is the *sustainable* one.
///
/// This is the arc's answer to `docs/plan_harvest_floor.md` §10 Q3. It is a named constant rather
/// than a config lever because there is exactly one right answer to *"where does the dial start"*
/// while `r·fK·(1−f)` peaks at `f = 0.5`; promoting it to `labor_config.json` is a one-line change if
/// playtest wants to move it off the peak.
pub const DEFAULT_ESCAPEMENT_FLOOR: f32 = crate::fauna::MSY_BIOMASS_FRACTION;

/// **Is a floor a legal one?** Finite and in `0.0..=1.0` — a floor is a fraction of `K`, so anything
/// outside that names a stock the source cannot have.
///
/// The command boundary **fails closed** on a floor that is not valid (the `cancel_order` scope
/// precedent) rather than clamping: a clamp turns a typo into a quiet policy change, and the one
/// number the whole harvest model now turns on is the last place to guess at intent.
pub fn floor_is_valid(floor: f32) -> bool {
    floor.is_finite() && (0.0..=1.0).contains(&floor)
}

/// **Is this floor set below the food peak?** — the INTENT half of the ⚠ predicate, and on its own
/// **not** the ⚠: publish [`SourceYield::overdraws`] through [`take_overdraws`], never through this.
///
/// It is `floor < MSY_BIOMASS_FRACTION`: *"you are asking to draw this below the food peak"*. The
/// sustained take `r·fK·(1−f)` peaks at `f = 0.5`, so a floor at or above the peak cannot be an
/// overdraw whatever the crew, and a floor below it is one the moment a crew can get there.
///
/// **Deliberately NOT `actual > sustainable`.** A first harvest of a stocked source is its
/// accumulated stock and exceeds one turn's regrowth under *every* floor, the peak included, so the
/// comparison mis-fires exactly where the player most needs the ⚠ to be trustworthy. That is why the
/// ability half below is a question about the crew's **throughput**, not about the take it happened
/// to land this turn.
pub fn floor_overdraws(floor: f32) -> bool {
    floor < crate::fauna::MSY_BIOMASS_FRACTION
}

/// **Does this take draw the stock below what it sustains?** — THE ⚠ predicate
/// ([`SourceYield::overdraws`]), and the **only** thing that may write that field.
///
/// Two conjuncts, and the ⚠ is a lie without either:
///
/// - **INTENT** — [`floor_overdraws`]: the dial is set below the food peak.
/// - **ABILITY** — this crew can actually get the stock down there. It must out-take the biggest
///   one-turn regrowth anywhere in the band it has to cross (`peak_regrowth_in_band`, from the
///   source's own curve): while the crew's throughput is smaller than the regrowth at some stock in
///   `floor·K ..= B`, the stock stalls at that stock and holds, and a floor it never reaches is a
///   floor nothing is being overdrawn to.
///
/// Both terms are **biomass per turn**, so a caller must not hand one of them a provisions rate.
///
/// **The regrowth is floored at zero, and that is what keeps a crew of NOBODY out of the ⚠.** A herd
/// past its Allee threshold regrows *negatively* — it declines whether or not anyone hunts it — so an
/// unfloored comparison makes the empty crew's `0.0` "out-take" the decline and warn about a source
/// nobody is touching. Floored, the statement reads *"a declining stock needs only a positive take to
/// be drawn to the floor, and no take at all draws nothing"*, which is the honest one.
///
/// **Why ability is not "is the stock falling this turn".** The regrowth curve peaks at `K/2`, and an
/// overdraw floor is by definition below it — so a crew descending from a full source has the peak
/// still to cross, and one that merely out-takes today's regrowth can settle *at* the peak and hold
/// there forever. Both surfaces used to disagree about that case ([reported from play] a herd at
/// `81%` of `K` with four herders and a `39%` floor: the tile card said *overdrawing*, the compose
/// sheet said *settles at 92%*). The whole point of stating the predicate here is that **every
/// surface that says "overdrawing" reads one function** — the mark, the tooltip, the map badge and
/// the compose sheet's verdict are readings of this, not four opinions about it.
pub fn take_overdraws(floor: f32, crew_biomass_per_turn: f32, peak_regrowth_in_band: f32) -> bool {
    floor_overdraws(floor) && crew_biomass_per_turn > peak_regrowth_in_band.max(0.0)
}

/// **Is a raid at this floor a SERIES of trips** — repeated full-cap runs to the band and back until
/// the herd is drawn to the floor — rather than one raid? See the `relaunch` arm of
/// `advance_expeditions` (Population). Stated here as the single source so the snapshot's in-flight
/// delivery forecast cannot drift from the phase machine.
///
/// It is `floor < MSY_BIOMASS_FRACTION`: any floor below the food peak leaves more standing stock
/// than one pack can carry, so the party is running a campaign rather than making a trip. That is a
/// **widening** of the rule it replaced (which was the `Deplete` stance alone), and it is safe for
/// one reason worth stating: **`done` is tested BEFORE `relaunch`**, so a party that has drawn the
/// herd to its floor comes home for good instead of cycling on an empty surplus.
///
/// **Not the same question as "does it ever pass through `Delivering`"** (issue #441): a party whose
/// herd wanders within `hunt.drop_off_within_tiles` of camp drops its load off and resumes hunting
/// too. That is an incident *inside* one raid, not a new trip, so this still reads `false` for it.
pub fn raid_is_recurring(floor: f32) -> bool {
    floor < crate::fauna::MSY_BIOMASS_FRACTION
}

/// **WHAT IS BEING RAISED ON A SOURCE** — *what am I building here?* — the axis that is independent
/// of the take crew's pressure (issue #442, `docs/plan_investment_rung_toggle.md` §2).
///
/// These are the intensification ladder's **rung-transition verbs**. **A verb DECLARES; it does not
/// staff** (`docs/plan_standing_upkeep.md` §2.5): issuing one appends a
/// [`BuildQueueEntry`] to the band's [`LaborAllocation::build_queue`], and the hands are the
/// band-level [`LaborTarget::Builders`] pool, whose **whole** output goes on the head of that queue.
/// Those work units bank into the source's build meter (`ForagePatch::cultivation_progress` /
/// `field_progress`, `Herd::domestication_progress` / `corral_progress`), and at the job's own cost
/// the source becomes a **tended patch / Field / pastoral herd / penned herd**, pays the full managed
/// yield, and its entry **leaves the queue**.
///
/// **THE BUILDERS OWE NOTHING TOWARD THE RUNG'S STANDING UPKEEP** (§4.6a). The band's *keeping* pool
/// (`agriculture` / `husbandry`) owes that rate for every meter carrying work, at any fullness — so
/// the meter takes the pool's whole output, and the only term that can eat a build is the **rot**,
/// what the keeping failed to cover ([`crate::intensification::RungDef::meter_rot`]).
///
/// **THERE IS NO YIELD DIP, and its retirement is what makes the verb legible** (§2.2). A rung's
/// `yield_fraction_while_building` used to scale what the crew carried, on the reasoning *"they are
/// preparing the ground, not gathering"* — true of a **shared** crew and of nothing else. With the
/// gatherers on the source and the builders on the band, what a build costs is simply the people who
/// are clearing instead, so the gatherers beside it carry exactly what they always did and the price
/// is the same statement at every staffing. Under the dip it was not: a crew big enough to saturate
/// the source's standing stock paid *nothing*, because the ceiling bound it either way.
///
/// **At most one is ever in flight, and it is always the source's next rung** — the rungs are
/// strictly ordered, so you cannot Sow ground you have not tended and a tended patch has nothing left
/// to cultivate.
///
/// **Each is kind-specific** (validated at `assign_labor` and at each verb's own command):
/// `Cultivate`/`Sow` are plant-only, `Tame`/`Corral` animal-only — see [`Improvement::valid_for_forage`]
/// / [`Improvement::valid_for_hunt`].
///
/// **Any floor is LEGAL beside any of these** (§2.1), and **the floor is not a term in the build at
/// all**: `build_accrual` takes no floor, because the builders are not the people pulling on the
/// source — see `.claude/rules/core_sim/intensification.md` → "THE FLOOR CAME OFF THE BUILD RATE".
/// What a deep floor still costs a build is indirect and real: the rung's own gate reads the
/// **escapement room** the take leaves (`systems::labor::crew_is_working_the_source`), so a crew
/// stripping the ground it is improving can close that gate. **The `Thriving` gate is gone**
/// (`docs/plan_harvest_floor.md` §3.2) — it stopped accrual outright, which under a continuous dial
/// would have made a whole stretch of the dial silently inert, with no lapse state left to explain
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Improvement {
    /// **Plant-only.** Prepare the patch into a tended crop (plant rung 2).
    Cultivate,
    /// **Plant-only.** Sow a **Field** — the plant rung-3 verb, the twin of `Corral`
    /// (`docs/plan_intensification_ladder.md` §2). It **places** a food source: it builds a Field
    /// *even where no patch existed*, because seed travels — the one asymmetry with the animal
    /// branch, where `Corral` needs a herd you already tamed.
    ///
    /// **But only on ground the land itself will farm, and that is SCARCE.** Rung 3 knows how to move
    /// seed, not how to *fertilize*, so the ground must already do the fertilizing: the
    /// `plant:field` rung's **`site_requirement`** demands **very fertile** ground
    /// (`min_forage_capacity` 195 — the river-deposit class: delta / floodplain / alluvial plain) that
    /// is **near fresh water** (`requires_fresh_water` — a river along one of its sides, fresh-water
    /// ground, or a lake/channel next door; a salt coast does **not** count). Merely bearing *some*
    /// food is nowhere near enough. Making thin or dry ground farmable is rung 4 (Worked Land), a
    /// later arc — which will be a **looser copy of that same record**.
    ///
    /// The rule lives on the rung, never here: `forage::rung_site_refusal` is the one seam the `sow`
    /// command, the labor arm and the wire all resolve through.
    Sow,
    /// **Animal-only.** Tame a wild herd into pastoral livestock — the animal rung-2 verb. A Sustain
    /// hunt tames nothing: it only *teaches* the faction Herding.
    Tame,
    /// **Animal-only.** Build the pen for a domesticated herd (animal rung 3).
    Corral,
}

/// **A floor of `0` — "leave nothing standing."** Named because a bare `0.0` at a comparison site
/// reads as an absent value rather than as the deliberate instruction it is, and because the
/// behaviour that hangs off it (a raid that never delivers and grinds a herd to extinction) is a
/// consequence of *this exact number* rather than of a mission kind.
pub const STRIP_IT_BARE: f32 = 0.0;

/// **A party with no herd to stop short of** — a `Scout` expedition's reported raid floor. `1.0`, not
/// `0`: an absent floor must not read as *"take everything"*, which is the one value that would be a
/// dangerous default if a reader ever acted on it.
pub const NO_RAID_FLOOR: f32 = 1.0;

/// **No improvement in flight** — what a pure harvest passes for the improvement axis. Named because
/// the take/ceiling seams take it positionally in long argument lists, where a bare `None` says
/// nothing about which of the two axes it is answering.
pub const NO_IMPROVEMENT_UNDERWAY: Option<Improvement> = None;

impl Improvement {
    /// Stable wire/config key — the `as_str` convention every wire enum here uses, and the value
    /// `LaborAssignmentState.improvement` carries (`""` for [`None`]).
    pub fn as_str(self) -> &'static str {
        match self {
            Improvement::Cultivate => "cultivate",
            Improvement::Sow => "sow",
            Improvement::Tame => "tame",
            Improvement::Corral => "corral",
        }
    }

    /// The improvements a **Forage** assignment accepts — the plant branch's two rung-transition
    /// verbs. Exhaustive rather than a `!matches!` complement so a new verb must **fail to compile**
    /// here until someone states which web it belongs to; the old hand-written complements defaulted
    /// a new verb to legal on *both* kinds.
    pub fn valid_for_forage(self) -> bool {
        match self {
            Improvement::Cultivate | Improvement::Sow => true,
            Improvement::Tame | Improvement::Corral => false,
        }
    }

    /// The improvements a **Hunt** assignment accepts — the animal branch's two rung-transition
    /// verbs. The exact twin of [`Improvement::valid_for_forage`], and exhaustive for the same reason.
    ///
    /// Note this is the **band's** axis. An *expedition* has no improvement slot **at all** — every
    /// rung-transition is place-bound work a resident band does — and since issue #442 that is a
    /// type-level fact (`ExpeditionMission::Hunt` carries a **floor**, a number, which can no longer
    /// name a build verb) rather than a runtime gate that could rot.
    pub fn valid_for_hunt(self) -> bool {
        match self {
            Improvement::Tame | Improvement::Corral => true,
            Improvement::Cultivate | Improvement::Sow => false,
        }
    }
}

impl FromStr for Improvement {
    type Err = ();

    /// Parse a verb from its wire/config key. **An empty string is an error, not a default** — unlike
    /// the assignment's floor, which defaults to the food peak because every assignment has *some*
    /// pressure. An absent improvement is `None`, and the caller says so; there is no "default
    /// improvement".
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cultivate" => Ok(Improvement::Cultivate),
            "sow" => Ok(Improvement::Sow),
            "tame" => Ok(Improvement::Tame),
            "corral" => Ok(Improvement::Corral),
            _ => Err(()),
        }
    }
}

impl Default for PowerNode {
    fn default() -> Self {
        Self {
            id: PowerNodeId(0),
            base_generation: scalar_zero(),
            base_demand: scalar_zero(),
            generation: scalar_zero(),
            demand: scalar_zero(),
            efficiency: Scalar::one(),
            storage_capacity: scalar_zero(),
            storage_level: scalar_zero(),
            stability: Scalar::one(),
            surplus: scalar_zero(),
            deficit: scalar_zero(),
            incident_count: 0,
        }
    }
}

/// Knowledge fragment payload carried between factions by migration.
#[derive(Debug, Clone, PartialEq)]
pub struct KnowledgeFragment {
    pub discovery_id: u32,
    pub progress: Scalar,
    pub fidelity: Scalar,
}

impl KnowledgeFragment {
    pub fn new(discovery_id: u32, progress: Scalar, fidelity: Scalar) -> Self {
        Self {
            discovery_id,
            progress,
            fidelity,
        }
    }

    pub fn from_contract(fragment: &ContractKnowledgeFragment) -> Self {
        Self {
            discovery_id: fragment.discovery_id,
            progress: Scalar::from_raw(fragment.progress),
            fidelity: Scalar::from_raw(fragment.fidelity),
        }
    }

    pub fn to_contract(&self) -> ContractKnowledgeFragment {
        ContractKnowledgeFragment {
            discovery_id: self.discovery_id,
            progress: self.progress.raw(),
            fidelity: self.fidelity.raw(),
        }
    }
}

pub fn fragments_to_contract(fragments: &[KnowledgeFragment]) -> Vec<ContractKnowledgeFragment> {
    fragments
        .iter()
        .map(|fragment| fragment.to_contract())
        .collect()
}

pub fn fragments_from_contract(fragments: &[ContractKnowledgeFragment]) -> Vec<KnowledgeFragment> {
    fragments
        .iter()
        .map(KnowledgeFragment::from_contract)
        .collect()
}

/// Pending migration payload queued on a population cohort.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingMigration {
    pub destination: FactionId,
    pub eta: u16,
    pub fragments: Vec<KnowledgeFragment>,
}

impl Default for Tile {
    fn default() -> Self {
        Self {
            position: UVec2::ZERO,
            element: ElementKind::Ferrite,
            temperature: scalar_zero(),
            terrain: TerrainType::AlluvialPlain,
            terrain_tags: TerrainTags::empty(),
            underlying_terrain: None,
            mountain: None,
            river_edges: 0,
            river_inflow: 0,
            river_channel: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The ⚠ predicate is the food peak, stated over the whole legal range** rather than at the
    /// four floors the retired stance axis could reach: a crew that stops at or above the peak
    /// cannot be overdrawing, and one that draws below it always is.
    ///
    /// Its former twin `floor_teaches` was **deleted** with the step it stated: the harvest floor
    /// made learning a *rate* (`intensification::learn_multiplier`), so "does this teach" is no
    /// longer a predicate anyone can ask.
    #[test]
    fn overdrawing_is_exactly_drawing_below_the_food_peak() {
        let peak = crate::fauna::MSY_BIOMASS_FRACTION;
        // The whole legal range at a fine step, plus the boundary itself — the value at which the
        // predicate changes hands, and therefore the only place an off-by-one can hide.
        let mut floors: Vec<f32> = (0..=100).map(|step| step as f32 / 100.0).collect();
        floors.push(DEFAULT_ESCAPEMENT_FLOOR);
        for floor in floors {
            assert_eq!(
                floor_overdraws(floor),
                floor < peak,
                "floor {floor}: the ⚠ is exactly 'below the food peak'"
            );
        }
        assert!(
            !floor_overdraws(DEFAULT_ESCAPEMENT_FLOOR),
            "the default floor is the food peak, so a fresh assignment carries no ⚠"
        );
        assert!(
            floor_overdraws(STRIP_IT_BARE) && !floor_overdraws(1.0),
            "stripping overdraws, leaving it all does not"
        );
    }

    /// *"Take everything"* — the floor-`0` end of the dial, named because `0.0` as a bare argument
    /// reads as an absent value rather than as the deliberate instruction it is.
    const STRIP_IT_BARE: f32 = 0.0;

    /// **The ⚠ boundary is closed above, open below** — pinned at the value itself rather than at a
    /// handful of sample floors, because `>=` versus `>` there is the difference between the default
    /// assignment carrying a warning and not.
    ///
    /// (`the_earn_step_falls_exactly_on_the_food_peak` was deleted with `floor_teaches`: there is no
    /// step left to fall anywhere. Learning is now continuous in the floor — see
    /// `intensification::learn_multiplier`.)
    #[test]
    fn the_overdraw_boundary_falls_exactly_on_the_food_peak() {
        let peak = crate::fauna::MSY_BIOMASS_FRACTION;
        assert!(
            !floor_overdraws(peak),
            "AT the peak is not an overdraw — a take there is exactly the regrowth"
        );
        assert!(
            floor_overdraws(peak - f32::EPSILON),
            "a hair below it is — the boundary is closed above, open below"
        );
        assert!(
            !floor_overdraws(1.0),
            "and no floor above the peak overdraws: under-harvest is restraint too"
        );
    }

    /// **THE ABILITY CONJUNCT, at its three interesting points.** The ⚠ needs both halves, so the
    /// same below-peak floor answers differently for a crew that can make the descent and one that
    /// cannot — and a floor at or above the peak stays dark at any crew size whatever.
    #[test]
    fn the_overdraw_needs_the_crew_to_reach_the_floor_as_well_as_the_dial() {
        /// The biggest one-turn regrowth standing between a fixture's floor and its stock.
        const PEAK_REGROWTH: f32 = 10.0;
        let below_peak = crate::fauna::MSY_BIOMASS_FRACTION - 0.1;

        assert!(
            take_overdraws(below_peak, PEAK_REGROWTH + 1.0, PEAK_REGROWTH),
            "a crew that out-takes the regrowth it has to cross reaches the floor — the ⚠"
        );
        assert!(
            !take_overdraws(below_peak, PEAK_REGROWTH - 1.0, PEAK_REGROWTH),
            "one that does not settles above the floor and overdraws nothing"
        );
        assert!(
            !take_overdraws(
                crate::fauna::MSY_BIOMASS_FRACTION,
                PEAK_REGROWTH * 100.0,
                PEAK_REGROWTH
            ),
            "and no crew makes a take AT the peak an overdraw — the first harvest rationale"
        );
    }

    /// **A SOURCE NOBODY IS WORKING IS NEVER AN OVERDRAW, even one that is dying on its own.** Below
    /// its Allee threshold a herd's one-turn regrowth is *negative*, so an unfloored ability test
    /// would let an empty crew's `0.0` "out-take" the decline and warn about a herd it is not
    /// touching. The regrowth is floored at zero for exactly this.
    #[test]
    fn a_collapsing_source_no_one_works_carries_no_warning() {
        /// A herd past its Allee threshold, losing biomass every turn whether or not it is hunted.
        const DECLINING: f32 = -4.0;
        const NOBODY: f32 = 0.0;
        let below_peak = crate::fauna::MSY_BIOMASS_FRACTION - 0.1;

        assert!(
            !take_overdraws(below_peak, NOBODY, DECLINING),
            "an unstaffed row draws nothing, so it overdraws nothing"
        );
        assert!(
            take_overdraws(below_peak, f32::EPSILON, DECLINING),
            "…while any real take does reach a floor the stock is already falling toward"
        );
    }

    /// **A floor is valid iff it is a finite fraction of `K`.** The command boundary rejects
    /// everything else rather than clamping (`docs/plan_harvest_floor.md` §4).
    #[test]
    fn only_a_finite_fraction_of_capacity_is_a_valid_floor() {
        for legal in [0.0_f32, 0.15, 0.5, 0.999, 1.0] {
            assert!(floor_is_valid(legal), "{legal} is a legal floor");
        }
        for illegal in [-0.01_f32, 1.01, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!floor_is_valid(illegal), "{illegal} must be refused");
        }
    }

    /// A forage assignment on `tile` staffed with `take` gatherers — the **one** allocation a
    /// source row carries since the build left the tile (`docs/plan_standing_upkeep.md` §2.5).
    #[cfg(test)]
    fn staffed_forage(tile: bevy::math::UVec2, take: u32) -> LaborAssignment {
        LaborAssignment {
            target: LaborTarget::Forage {
                tile,
                floor: DEFAULT_ESCAPEMENT_FLOOR,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
            workers: take,
            kit: None,
        }
    }

    /// **A SHRUNK BAND SHEDS FROM THE TAIL, AND EVERY STANDING ROLE IS AN ORDINARY ROW.**
    ///
    /// [`LaborAllocation::normalize`] answers the one question a command-side clamp cannot: the band
    /// **lost people** (a famine, a fission, a raid), so hands already committed have to go
    /// somewhere. It trims **tail-first**, and a row with nothing left of it is dropped whole rather
    /// than left staffed by nobody.
    ///
    /// **There is no within-row shedding order any more** (`docs/plan_standing_upkeep.md` §2.5). The
    /// build→take order existed while a row carried two crews; the building and the keeping are
    /// **rows** now ([`LaborTarget::Builders`] / [`LaborTarget::Agriculture`] /
    /// [`LaborTarget::Husbandry`]), so where each falls in the shedding order is where the player
    /// put it in the list — which is a statement the player can make and the old rule was not.
    #[test]
    fn a_shrunk_band_sheds_from_the_tail_and_the_builders_are_a_row_in_that_line() {
        let head = bevy::math::UVec2::new(1, 1);
        let tail = bevy::math::UVec2::new(2, 2);
        let mut allocation = LaborAllocation {
            assignments: vec![
                staffed_forage(head, 4),
                staffed_forage(tail, 3),
                LaborAssignment {
                    target: LaborTarget::Builders,
                    workers: 2,
                    kit: None,
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            allocation.assigned_total(),
            9,
            "the builders draw on the same finite band as the gatherers"
        );

        // One hand short: the tail row is the builders, and it is what gives.
        assert!(allocation.normalize(8).is_empty(), "nothing is dropped yet");
        assert_eq!(
            allocation.workers_on(&LaborTarget::Builders),
            1,
            "the tail row sheds first"
        );
        assert_eq!(
            (
                allocation.assignments[0].workers,
                allocation.assignments[1].workers
            ),
            (4, 3),
            "the gathering is untouched while the tail still has hands"
        );

        // **A row with nothing left of it is DROPPED WHOLE**, rather than left staffed by nobody —
        // a zero-worker row is this system's own word for *abandon it*, so keeping one would render
        // as work being done by an empty crew.
        let dropped = allocation.normalize(7);
        assert_eq!(dropped.len(), 1, "the emptied builders row is handed back");
        assert_eq!(dropped[0].target, LaborTarget::Builders);
        assert_eq!(allocation.workers_on(&LaborTarget::Builders), 0);

        // Only then does the row above it give.
        assert!(allocation.normalize(6).is_empty());
        assert_eq!(allocation.assignments[1].workers, 2);
        assert_eq!(allocation.assigned_total(), 6);
    }

    /// **A KEEPING ROLE IS A ROW LIKE ANY OTHER** — it counts against the pool, `workers_on` reads
    /// its head count, and it is staffed through `set_assignment` like every other row
    /// (`docs/plan_standing_upkeep.md` §2.5). The hand-off that used to create it off-command is
    /// retired with §2.3's carry-over.
    #[test]
    fn a_maintenance_role_is_an_ordinary_row_the_pool_counts() {
        const BAND: u32 = 12;
        let mut allocation = LaborAllocation {
            assignments: vec![staffed_forage(bevy::math::UVec2::new(1, 1), 4)],
            ..Default::default()
        };
        assert_eq!(allocation.workers_on(&LaborTarget::Agriculture), 0);

        allocation.set_assignment(LaborTarget::Agriculture, 2, BAND, None);
        assert_eq!(allocation.workers_on(&LaborTarget::Agriculture), 2);
        assert_eq!(
            allocation.assigned_total(),
            6,
            "the keeping draws on the same finite band as the gathering"
        );

        // Re-stating the role replaces its head count — a role IS its head count.
        allocation.set_assignment(LaborTarget::Agriculture, 5, BAND, None);
        assert_eq!(allocation.workers_on(&LaborTarget::Agriculture), 5);

        // The two webs are separate pools and never merge.
        allocation.set_assignment(LaborTarget::Husbandry, 1, BAND, None);
        assert_eq!(allocation.workers_on(&LaborTarget::Agriculture), 5);
        assert_eq!(allocation.workers_on(&LaborTarget::Husbandry), 1);
    }

    // **RETIRED: `restating_one_activitys_crew_only_needs_the_difference`** — it pinned
    // `idle_for`'s give-back, and both went with the per-source build crew
    // (`docs/plan_standing_upkeep.md` §2.5). A verb states no crew, so there is no per-source number
    // to restate and no "which of this row's crews am I overwriting" question to answer.
    // `set_assignment`'s own headroom test is the whole enforcement now, and
    // `a_maintenance_role_is_an_ordinary_row_the_pool_counts` above pins it for a role.

    /// **Kind-exclusivity, pinned.** Every [`Improvement`] is place-bound work on ONE food web
    /// (`Cultivate`/`Sow` prepare ground; `Tame`/`Corral` work a herd) — never both, and never
    /// neither. Both predicates are exhaustive matches, so a new verb fails to compile until someone
    /// states its web; this catches a verb whose two arms *agree*.
    #[test]
    fn every_improvement_is_legal_on_exactly_one_food_web() {
        for improvement in [
            Improvement::Cultivate,
            Improvement::Sow,
            Improvement::Tame,
            Improvement::Corral,
        ] {
            assert_ne!(
                improvement.valid_for_forage(),
                improvement.valid_for_hunt(),
                "{improvement:?} is a rung-transition verb — it must be legal on exactly ONE kind"
            );
            // The wire key round-trips, so a snapshot's `improvement` string rebuilds the same verb.
            assert_eq!(
                improvement.as_str().parse::<Improvement>(),
                Ok(improvement),
                "{improvement:?}'s wire key must parse back to it"
            );
        }
        // An absent improvement is `None`, never a defaulted verb: unlike the floor, which has a
        // default (the food peak), there is no "default improvement".
        assert_eq!("".parse::<Improvement>(), Err(()));
        // **A retired stance token is not a build verb either.** The four names the harvest floor
        // replaced are refused at the command boundary (`sim_runtime`'s `reject_retired_stance`);
        // this pins that a stale client's token cannot fall through into the OTHER axis instead.
        for retired in ["sustain", "surplus", "deplete", "eradicate"] {
            assert_eq!(retired.parse::<Improvement>(), Err(()));
        }
    }

    #[test]
    fn workers_on_counts_scout_headcount() {
        let mut allocation = LaborAllocation::default();
        // No Scout assignment → zero scouts.
        assert_eq!(allocation.workers_on(&LaborTarget::Scout), 0);

        let available = 10;
        allocation.set_assignment(LaborTarget::Scout, 3, available, None);
        allocation.set_assignment(LaborTarget::Warrior, 2, available, None);
        // Only the Scout assignment is counted (Warrior is a different singleton source).
        assert_eq!(allocation.workers_on(&LaborTarget::Scout), 3);
        assert_eq!(allocation.workers_on(&LaborTarget::Warrior), 2);
    }

    /// A Forage policy change on the **same tile** is the same source (§0-iii, parity with the Hunt
    /// arm's policy): re-assigning replaces rather than duplicating. A different tile is a different
    /// source regardless of policy.
    #[test]
    fn forage_same_source_ignores_policy_matches_tile() {
        let tile = UVec2::new(3, 4);
        let sustain = LaborTarget::Forage {
            tile,
            floor: DEFAULT_ESCAPEMENT_FLOOR,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        };
        let deplete = LaborTarget::Forage {
            tile,
            floor: 0.15,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        };
        let other_tile = LaborTarget::Forage {
            tile: UVec2::new(5, 6),
            floor: DEFAULT_ESCAPEMENT_FLOOR,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        };
        // Same tile, different FLOOR → same source (the floor is a mutable property).
        assert!(sustain.same_source(&deplete));
        // Different tile → different source even at the same floor.
        assert!(!sustain.same_source(&other_tile));

        // set_assignment on the same tile with a new floor replaces (no duplicate row) and updates
        // the stored floor.
        let mut allocation = LaborAllocation::default();
        allocation.set_assignment(sustain, 4, 10, None);
        allocation.set_assignment(deplete.clone(), 3, 10, None);
        assert_eq!(allocation.assignments.len(), 1, "a floor change replaces");
        assert_eq!(allocation.assignments[0].workers, 3);
        assert_eq!(allocation.assignments[0].target, deplete);
    }

    /// **A floor or crew edit leaves the QUEUED BUILD alone** (issue #442 §6, re-aimed by
    /// `docs/plan_standing_upkeep.md` §2.5) — and now does so **by construction**: the build lives
    /// in [`LaborAllocation::build_queue`] and `set_assignment` does not touch it at all, where it
    /// used to have to carry a field across a row it rebuilds.
    ///
    /// Unstaffing the take still leaves the band's *holding* standing, so the row is still there for
    /// the entry to require.
    #[test]
    fn a_floor_or_crew_edit_keeps_the_queued_build_in_flight() {
        let tile = UVec2::new(3, 4);
        let sustain = LaborTarget::Forage {
            tile,
            floor: DEFAULT_ESCAPEMENT_FLOOR,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        };
        let source = BuildSource::Patch(tile);
        let mut allocation = LaborAllocation::default();
        allocation.set_assignment(sustain.clone(), 4, 10, None);
        assert!(allocation.enqueue_build(source.clone(), BuildJob::Rung(Improvement::Cultivate)));

        // Re-staffing the same source: the declaration survives, and so does its place in the line.
        allocation.set_assignment(sustain.clone(), 2, 10, None);
        assert_eq!(allocation.assignments[0].workers, 2);
        assert_eq!(
            allocation.build_queue_entry(&source).map(|e| e.declared),
            Some(BuildJob::Rung(Improvement::Cultivate)),
            "changing the crew must not abandon the build"
        );

        // Dragging the floor: likewise. A stripping floor beside a Cultivate build is legal (§2.1).
        let deplete = LaborTarget::Forage {
            tile,
            floor: 0.15,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        };
        allocation.set_assignment(deplete.clone(), 2, 10, None);
        assert_eq!(allocation.assignments[0].target, deplete);
        assert_eq!(allocation.build_queue_position(&source), Some(0));

        // Unstaffing the TAKE leaves the band's holding of the source standing — the row survives at
        // zero gatherers with the build still queued on it, which is what lets a finished rung stay
        // eligible for its web's keeping pool. Walking away from the build is `unqueue`.
        allocation.set_assignment(deplete.clone(), 0, 10, None);
        assert_eq!(allocation.assignments.len(), 1);
        assert_eq!(allocation.assignments[0].workers, 0);
        assert_eq!(
            allocation.build_queue_position(&source),
            Some(0),
            "unstaffing the gatherers must not withdraw the declaration beside them"
        );

        // **RE-ISSUING A VERB KEEPS THE ENTRY'S PLACE**, and `unqueue` is the undo.
        assert!(allocation.enqueue_build(source.clone(), BuildJob::Rung(Improvement::Sow)));
        assert_eq!(allocation.build_queue_position(&source), Some(0));
        assert!(allocation.unqueue_build(&source));
        assert_eq!(allocation.build_queue_position(&source), None);
    }

    /// A **role** is its head count, so `assign_labor … scout 0` really does remove the row — the
    /// half of the rule above that did not move. And a source the band never worked is not conjured
    /// into a zero row by an unassign.
    #[test]
    fn a_role_row_still_goes_at_zero_and_an_unworked_source_is_never_created() {
        let mut allocation = LaborAllocation::default();
        allocation.set_assignment(LaborTarget::Scout, 3, 10, None);
        allocation.set_assignment(LaborTarget::Scout, 0, 10, None);
        assert!(
            allocation.assignments.is_empty(),
            "a standing role IS its head count"
        );

        allocation.set_assignment(
            LaborTarget::Forage {
                tile: UVec2::new(9, 9),
                floor: DEFAULT_ESCAPEMENT_FLOOR,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
            0,
            10,
            None,
        );
        assert!(
            allocation.assignments.is_empty(),
            "unassigning ground nobody worked says nothing"
        );
    }

    /// A thirty-person band earns a fraction of a birth per turn. Per-turn rounding would either
    /// invent a birth it never had or report none for the whole game; the carry does neither.
    #[test]
    fn a_band_too_small_for_a_birth_this_turn_reports_one_later() {
        // 30 working × a 0.01 birth rate = 0.3 of a person per turn.
        let flow = scalar_from_f32(0.3);
        let mut carry = scalar_zero();

        assert_eq!(DemographicFlowAccumulator::accrue(&mut carry, flow), 0);
        assert_eq!(DemographicFlowAccumulator::accrue(&mut carry, flow), 0);
        assert_eq!(
            DemographicFlowAccumulator::accrue(&mut carry, flow),
            0,
            "0.9 of a person is still nobody"
        );
        assert_eq!(
            DemographicFlowAccumulator::accrue(&mut carry, flow),
            1,
            "the fourth turn crosses"
        );
    }

    /// A big band crosses several people in one turn, and reports them as ONE event's count rather
    /// than one crossing per turn for the next several turns.
    #[test]
    fn a_large_band_reports_several_people_in_one_turn() {
        let mut carry = scalar_zero();
        assert_eq!(
            DemographicFlowAccumulator::accrue(&mut carry, scalar_from_f32(3.5)),
            3
        );
    }

    /// **The property that makes the whole model honest**: the crossing subtracts exactly the whole
    /// people it reported, so the remainder rides on and nothing is invented or lost.
    #[test]
    fn the_remainder_survives_the_crossing() {
        let flow = scalar_from_f32(0.6);
        let mut carry = scalar_zero();

        assert_eq!(DemographicFlowAccumulator::accrue(&mut carry, flow), 0);
        assert_eq!(DemographicFlowAccumulator::accrue(&mut carry, flow), 1);
        assert!(
            (carry.to_f32() - 0.2).abs() < 1e-5,
            "1.2 reported one person and kept 0.2, not 0: {}",
            carry.to_f32()
        );

        // Five turns of 0.6 owe exactly three people, and only a preserved remainder pays the third.
        let mut reported = 1;
        reported += DemographicFlowAccumulator::accrue(&mut carry, flow);
        reported += DemographicFlowAccumulator::accrue(&mut carry, flow);
        reported += DemographicFlowAccumulator::accrue(&mut carry, flow);
        assert_eq!(reported, 3, "5 × 0.6 = 3.0 people");
    }

    // ---- The material batch map (`docs/plan_crafting_and_materials.md` §1) ----

    /// A fixture material with two axes and two bands, so a reading either side of `0.5` lands in a
    /// different band and the merge rule has something to refuse to merge.
    const AXIS_TOUGH: &str = "toughness";
    const AXIS_SUPPLE: &str = "suppleness";
    const HIDE: &str = "hide";

    fn readings(tough: f32, supple: f32) -> BTreeMap<String, f32> {
        BTreeMap::from([
            (AXIS_TOUGH.to_string(), tough),
            (AXIS_SUPPLE.to_string(), supple),
        ])
    }

    fn reading_of(batch: &MaterialBatch, axis: &str) -> f32 {
        batch.characteristics.get(axis).copied().expect("axis")
    }

    /// **THE MERGE RULE, both halves.** Two arrivals in the same band become **one** batch, and the
    /// surviving reading is the amount-weighted average — *not* either input's.
    ///
    /// The liveness half is the value assertion: a store that simply kept the first arrival, or the
    /// last, would also report one batch, so counting batches alone cannot tell the merge from a
    /// silent overwrite.
    #[test]
    fn two_arrivals_in_one_band_become_one_batch_at_the_weighted_average() {
        let band = crate::materials_config::BandKey(vec![1, 1]);
        let mut store = LocalStore::new();
        store.deposit_material(
            HIDE,
            band.clone(),
            Scalar::from_f32(10.0),
            &readings(0.60, 0.70),
        );
        store.deposit_material(
            HIDE,
            band.clone(),
            Scalar::from_f32(30.0),
            &readings(0.80, 0.50),
        );

        let batches: Vec<_> = store.material_batches(HIDE).collect();
        assert_eq!(batches.len(), 1, "one band, one batch");
        let (_, batch) = batches[0];
        assert_eq!(batch.amount, Scalar::from_f32(40.0));
        // 10 × 0.60 + 30 × 0.80 over 40 = 0.75, which is NEITHER input.
        assert!(
            (reading_of(batch, AXIS_TOUGH) - 0.75).abs() < 1e-5,
            "the batch must read the weighted average, got {}",
            reading_of(batch, AXIS_TOUGH)
        );
        assert!(
            (reading_of(batch, AXIS_SUPPLE) - 0.55).abs() < 1e-5,
            "every axis blends, not just the first"
        );
        assert_eq!(store.material_total(HIDE), Scalar::from_f32(40.0));
    }

    /// The other half of the rule: **different bands never merge**, however close the readings. This
    /// is what stops a mammoth hide being averaged into a hare pelt.
    #[test]
    fn two_arrivals_in_different_bands_stay_two_batches() {
        let mut store = LocalStore::new();
        store.deposit_material(
            HIDE,
            crate::materials_config::BandKey(vec![0, 1]),
            Scalar::from_f32(10.0),
            &readings(0.10, 0.90),
        );
        store.deposit_material(
            HIDE,
            crate::materials_config::BandKey(vec![1, 0]),
            Scalar::from_f32(10.0),
            &readings(0.90, 0.10),
        );
        assert_eq!(store.material_batches(HIDE).count(), 2);
        assert_eq!(store.material_total(HIDE), Scalar::from_f32(20.0));
    }

    /// **Worst-first on the named axis** — the poor hide is spent before the excellent one, and the
    /// axis the caller names is the one that decides. Both halves are asserted against each other:
    /// drawing on `toughness` and drawing on `suppleness` must empty *opposite* batches, so an
    /// implementation that ignored the axis (or sorted by band key) fails one of them.
    #[test]
    fn a_withdrawal_spends_the_worst_batch_on_the_named_axis_first() {
        let tough = crate::materials_config::BandKey(vec![1, 0]);
        let supple = crate::materials_config::BandKey(vec![0, 1]);
        let stocked = || {
            let mut store = LocalStore::new();
            store.deposit_material(
                HIDE,
                tough.clone(),
                Scalar::from_f32(10.0),
                &readings(0.9, 0.1),
            );
            store.deposit_material(
                HIDE,
                supple.clone(),
                Scalar::from_f32(10.0),
                &readings(0.1, 0.9),
            );
            store
        };

        let mut store = stocked();
        let drawn = store.take_material(HIDE, AXIS_TOUGH, Scalar::from_f32(10.0));
        assert_eq!(drawn.len(), 1, "one batch covered the whole draw");
        assert_eq!(drawn[0].band, supple, "the poorest TOUGHNESS goes first");
        assert_eq!(
            store
                .material_batches(HIDE)
                .map(|(band, _)| band.clone())
                .collect::<Vec<_>>(),
            vec![tough.clone()],
            "the tough batch is untouched and the emptied one is pruned"
        );

        let mut store = stocked();
        let drawn = store.take_material(HIDE, AXIS_SUPPLE, Scalar::from_f32(10.0));
        assert_eq!(
            drawn[0].band, tough,
            "on the other axis the ordering must reverse, or the axis is being ignored"
        );
    }

    /// A partial take leaves the batch's readings alone — an average does not move when a uniform
    /// part of it is removed — and reports what it actually took.
    #[test]
    fn a_partial_withdrawal_leaves_the_batchs_reading_untouched() {
        let band = crate::materials_config::BandKey(vec![1, 1]);
        let mut store = LocalStore::new();
        store.deposit_material(
            HIDE,
            band.clone(),
            Scalar::from_f32(10.0),
            &readings(0.62, 0.71),
        );

        let drawn = store.take_material(HIDE, AXIS_TOUGH, Scalar::from_f32(4.0));
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].amount, Scalar::from_f32(4.0));
        assert!((drawn[0].characteristics[AXIS_TOUGH] - 0.62).abs() < 1e-6);

        let batches: Vec<_> = store.material_batches(HIDE).collect();
        assert_eq!(batches[0].1.amount, Scalar::from_f32(6.0));
        assert!(
            (reading_of(batches[0].1, AXIS_TOUGH) - 0.62).abs() < 1e-6,
            "removing a uniform part of a batch must not move its average"
        );
    }

    /// A draw for more than the store holds takes everything and says so — the shortfall is the
    /// caller's to report, and there is no partial-batch residue left behind.
    #[test]
    fn a_short_withdrawal_empties_the_store_and_reports_what_it_took() {
        let mut store = LocalStore::new();
        store.deposit_material(
            HIDE,
            crate::materials_config::BandKey(vec![0, 0]),
            Scalar::from_f32(3.0),
            &readings(0.2, 0.2),
        );
        let drawn = store.take_material(HIDE, AXIS_TOUGH, Scalar::from_f32(8.0));
        let taken = drawn
            .iter()
            .fold(Scalar::zero(), |total, draw| total + draw.amount);
        assert_eq!(taken, Scalar::from_f32(3.0));
        assert_eq!(store.material_total(HIDE), Scalar::zero());
        assert_eq!(
            store.material_batches(HIDE).count(),
            0,
            "an emptied material leaves no husk, so two equal stores compare equal"
        );
    }
}
