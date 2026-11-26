//! **The `bands` section of a scenario** (`docs/plan_scenarios.md` section 2.3, slice 1): where a
//! faction's bands stand, how many people, their age shape, founding lines, larder and opening outfit.
//!
//! The file's shape, the name resolution (pass 1), and the setup resource worldgen reads while it
//! spawns the bands. The spawn itself lives in `systems/worldgen.rs`, because a scenario band is
//! spawned by the very function that spawns a profile's, with no special case after it.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::bench::BenchEntry;
use super::labor::LaborEntry;
use super::tiles::TileOverride;
use super::{
    band_scenario_id, round_export, ExportIds, ExportedBand, ScenarioContext, ScenarioRefusal,
};
use crate::components::{
    BandBench, BandEquipment, BandId, Expedition, PopulationCohort, ResidentBand, Tile, FODDER,
};
use crate::culture::{CultureManager, CultureOwner, CultureTraitAxis};
use crate::dealings::PendingDealings;
use crate::equipment_config::{EquipmentConfig, EquipmentConfigHandle};
use crate::goodwill::Goodwill;
use crate::intensification::{knows, LadderConfigHandle};
use crate::leader_names::LeaderNameCatalogHandle;
use crate::leadership::{emergence_entry, BandLeadership};
use crate::leadership_config::LeadershipConfigHandle;
use crate::orders::{FactionId, FactionRegistry};
use crate::resources::{
    CommandEventLog, DiscoveryProgressLedger, SimulationConfig, SimulationTick,
};
use crate::start_profile::{ActiveStartProfile, StartProfileKnowledgeTagsHandle};
use crate::starting_loadout::{
    allocation_load, faction_carry, party_held_items, window_people, LoadoutSupply, LoadoutWindow,
};
use crate::treaties::Treaties;
use crate::TileImprovementRegistry;

/// How far the three age shares may sit from summing to one. Shares are written by hand to two
/// decimals, so a sum of `0.99` or `1.01` is a typo to forgive, and `0.9` is a mistake to name.
pub const AGES_SUM_TOLERANCE: f32 = 0.011;

/// One faction's section.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactionSection {
    /// The faction index (`0` is the human).
    pub faction: u32,
    /// Knowledge tags the faction already has, added to its archetype's `starting_knowledge_tags`.
    pub knowledge: Vec<String>,
    /// The faction's bands, in spawn order. The FIRST holds the faction's opening outfit window.
    pub bands: Vec<BandEntry>,
    /// The faction's bench commands, replayed in file order. A required key: `[]` for none.
    pub bench: Vec<BenchEntry>,
    /// Improvements the faction already holds and builds already declared (`rungs.rs`).
    pub rungs: Vec<super::rungs::RungEntry>,
    /// The faction's opening work rows, replayed in order through `assign_labor` and `work_priority`.
    /// A required key: `[]` for none.
    pub labor: Vec<LaborEntry>,
    /// The faction's pen verbs, replayed after the builds. A required key: `[]` for none.
    pub pens: Vec<super::pens::PenEntry>,
    /// The road rungs the faction holds at the start (`roads.rs`). A required key: `[]` for none.
    pub roads: Vec<super::roads::RoadEntry>,
}

/// One band.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BandEntry {
    /// The band's id inside this file. Every later entry names its band by it.
    pub id: String,
    /// The tile the band stands on, `[x, y]`. Used exactly: no offset, no nearest-land search.
    pub at: [u32; 2],
    /// Head count.
    pub size: u32,
    pub ages: Ages,
    /// `L`: the founding lines the band is founded with.
    pub founding_lines: u16,
    /// The larder, exactly: an empty list is an empty larder.
    pub larder: Vec<LarderEntry>,
    /// The ONE optional key: absent keeps the default outfit fitted to this band's size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outfit: Option<Outfit>,
    /// `true` gives the band a leader ([`apply_leaders`]); absent leaves it to emergence. The file names
    /// neither the kind nor the name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leader: Option<bool>,
    /// Absent leaves the band's bench auto-craft off, `true` switches it
    /// on after the outfit, as the server's `set_auto_craft` does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_craft: Option<bool>,
}

/// The shares of a band in each age bracket.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ages {
    pub children: f32,
    pub working: f32,
    pub elders: f32,
}

impl Ages {
    /// The three shares divided by their sum, so they total exactly one. Pass 1 has already refused a
    /// negative share and a sum outside `AGES_SUM_TOLERANCE` of one, so the sum is never zero.
    pub fn normalised(&self) -> [f32; 3] {
        let sum = self.children + self.working + self.elders;
        [self.children / sum, self.working / sum, self.elders / sum]
    }
}

/// Food of one keeping class.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LarderEntry {
    pub class: String,
    pub amount: f32,
}

/// An opening outfit, as `SetStartingLoadout` carries it.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Outfit {
    #[serde(deserialize_with = "unique_keys")]
    pub kits: BTreeMap<String, u32>,
    #[serde(deserialize_with = "unique_keys")]
    pub materials: BTreeMap<String, u32>,
}

/// **A JSON object into a map that REFUSES a repeated key.** serde_json's own map handling keeps the
/// last of two equal keys without a word, so a scenario that names a kit twice would load the later
/// count and drop the earlier one. The error carries serde_json's `line L, column C`.
pub fn unique_keys<'de, D>(deserializer: D) -> Result<BTreeMap<String, u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct UniqueKeys;
    impl<'de> serde::de::Visitor<'de> for UniqueKeys {
        type Value = BTreeMap<String, u32>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("an object of name to count")
        }

        fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut map = BTreeMap::new();
            while let Some((key, value)) = access.next_entry::<String, u32>()? {
                if map.contains_key(&key) {
                    return Err(serde::de::Error::custom(format!("'{key}' is listed twice")));
                }
                map.insert(key, value);
            }
            Ok(map)
        }
    }
    deserializer.deserialize_map(UniqueKeys)
}

/// Pass 1 for the factions section: names resolved against the loaded configs, every fault collected.
pub(super) fn check(
    factions: &[FactionSection],
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let mut seen_factions: BTreeSet<u32> = BTreeSet::new();
    let mut seen_bands: BTreeSet<&str> = BTreeSet::new();
    for (i, section) in factions.iter().enumerate() {
        let at = format!("factions[{i}]");
        if !seen_factions.insert(section.faction) {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.faction"),
                format!("faction {} is listed twice", section.faction),
            ));
        }
        if section.bands.is_empty() {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.bands"),
                "a listed faction needs at least one band",
            ));
        }
        let mut seen_tags: BTreeSet<&str> = BTreeSet::new();
        for (k, tag) in section.knowledge.iter().enumerate() {
            if !seen_tags.insert(tag.as_str()) {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.knowledge[{k}]"),
                    format!("'{tag}' is listed twice"),
                ));
            }
            if ctx.tags.get(tag).is_none() {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.knowledge[{k}]"),
                    format!("'{tag}' is not a knowledge tag"),
                ));
            }
        }
        for (j, band) in section.bands.iter().enumerate() {
            let at = format!("{at}.bands[{j}]");
            if !seen_bands.insert(band.id.as_str()) {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.id"),
                    format!("band id '{}' is used twice", band.id),
                ));
            }
            check_band(band, &at, ctx, refusals);
        }
    }
}

fn check_band(
    band: &BandEntry,
    at: &str,
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    if band.size == 0 {
        refusals.push(ScenarioRefusal::new(
            format!("{at}.size"),
            "a band needs at least one person",
        ));
    }
    if band.founding_lines == 0 {
        refusals.push(ScenarioRefusal::new(
            format!("{at}.founding_lines"),
            "a band is founded with at least one line",
        ));
    }
    let ages = band.ages;
    let shares = [ages.children, ages.working, ages.elders];
    if shares
        .iter()
        .any(|share| !share.is_finite() || *share < 0.0)
    {
        refusals.push(ScenarioRefusal::new(
            format!("{at}.ages"),
            "an age share cannot be negative",
        ));
    } else if (shares.iter().sum::<f32>() - 1.0).abs() > AGES_SUM_TOLERANCE {
        refusals.push(ScenarioRefusal::new(
            format!("{at}.ages"),
            format!(
                "the shares sum to {:.3}, and must sum to 1",
                shares.iter().sum::<f32>()
            ),
        ));
    }
    let mut seen_classes: BTreeSet<&str> = BTreeSet::new();
    for (l, entry) in band.larder.iter().enumerate() {
        let at = format!("{at}.larder[{l}]");
        if !ctx
            .demographics
            .keeping
            .classes
            .iter()
            .any(|class| class.id == entry.class)
        {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.class"),
                format!("'{}' is not a keeping class", entry.class),
            ));
        }
        if !seen_classes.insert(entry.class.as_str()) {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.class"),
                format!("'{}' is listed twice", entry.class),
            ));
        }
        if !entry.amount.is_finite() || entry.amount < 0.0 {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.amount"),
                "an amount cannot be negative",
            ));
        }
    }
    let Some(outfit) = &band.outfit else {
        return;
    };
    let at = format!("{at}.outfit");
    for kit in outfit.kits.keys() {
        match ctx.equipment.kit_definition(kit) {
            None => refusals.push(ScenarioRefusal::new(
                format!("{at}.kits.{kit}"),
                format!("'{kit}' is not a kit on the equipment roster"),
            )),
            Some(definition) if definition.uses.is_empty() => refusals.push(ScenarioRefusal::new(
                format!("{at}.kits.{kit}"),
                format!("'{kit}' carries no items, so allocating it would buy nothing"),
            )),
            Some(_) => {}
        }
    }
    for material in outfit.materials.keys() {
        if ctx.materials.material(material).is_none() {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.materials.{material}"),
                format!("'{material}' is not a material"),
            ));
        }
    }
}

/// **What worldgen reads, while it spawns, to put a scenario's bands in place of a profile's.**
///
/// Installed before `Startup` and **removed once applied** ([`super::finish_scenario`]): it is not
/// `SimState`, it is never saved and it is never on a frame, so nothing scenario-only survives into
/// the running world.
#[derive(Resource, Debug, Clone, Default)]
pub struct ScenarioSetup {
    /// The listed factions, in file order.
    pub factions: Vec<FactionSetup>,
    /// The file's tile overrides, applied by `apply_scenario_tile_overrides` during Startup.
    pub tiles: Vec<TileOverride>,
    /// The scenario id each spawned band answers to, for the sections that name one.
    pub(super) spawned: BTreeMap<String, BandId>,
    /// Which (faction section, band entry) a spawned band came from, for the demographics seeding.
    by_band: BTreeMap<BandId, (usize, usize)>,
    /// Faults only the built world can see (a tile that is water), collected as worldgen goes.
    refusals: Vec<ScenarioRefusal>,
}

/// One listed faction, resolved.
#[derive(Debug, Clone)]
pub struct FactionSetup {
    /// Its position in the file's `factions` array, for refusal paths.
    pub file_index: usize,
    pub faction: FactionId,
    pub knowledge: Vec<String>,
    pub bands: Vec<BandEntry>,
    pub bench: Vec<BenchEntry>,
    pub rungs: Vec<super::rungs::RungEntry>,
    pub labor: Vec<LaborEntry>,
    /// The faction's pen verbs, replayed after the builds. A required key: `[]` for none.
    pub pens: Vec<super::pens::PenEntry>,
    pub roads: Vec<super::roads::RoadEntry>,
}

impl ScenarioSetup {
    pub fn from_sections(sections: &[FactionSection], tiles: &[TileOverride]) -> Self {
        Self {
            tiles: tiles.to_vec(),
            factions: sections
                .iter()
                .enumerate()
                .map(|(file_index, section)| FactionSetup {
                    file_index,
                    faction: FactionId(section.faction),
                    knowledge: section.knowledge.clone(),
                    bands: section.bands.clone(),
                    bench: section.bench.clone(),
                    rungs: section.rungs.clone(),
                    labor: section.labor.clone(),
                    pens: section.pens.clone(),
                    roads: section.roads.clone(),
                })
                .collect(),
            ..Self::default()
        }
    }

    /// The listing for `faction`, if the scenario names it.
    pub fn faction(&self, faction: FactionId) -> Option<&FactionSetup> {
        self.factions.iter().find(|setup| setup.faction == faction)
    }

    /// The scenario's own entry for a spawned band, if it came from one.
    pub fn band_entry(&self, band: BandId) -> Option<&BandEntry> {
        let (faction, entry) = self.by_band.get(&band)?;
        self.factions.get(*faction)?.bands.get(*entry)
    }

    /// Record a spawned band against the entry it came from.
    pub fn record_band(&mut self, file_index: usize, entry: usize, id: &str, band: BandId) {
        self.spawned.insert(id.to_string(), band);
        self.by_band.insert(band, (file_index, entry));
    }

    /// Record a fault only the built world could see.
    pub fn refuse(&mut self, path: impl Into<String>, reason: impl Into<String>) {
        self.refusals.push(ScenarioRefusal::new(path, reason));
    }

    /// Refuse every listed faction the built world does not have.
    pub fn refuse_absent_factions(&mut self, roster: &[FactionId]) {
        let absent: Vec<(usize, FactionId)> = self
            .factions
            .iter()
            .filter(|setup| !roster.contains(&setup.faction))
            .map(|setup| (setup.file_index, setup.faction))
            .collect();
        for (index, faction) in absent {
            self.refuse(
                format!("factions[{index}].faction"),
                format!(
                    "the world has no faction {} (it was built with {} factions)",
                    faction.0,
                    roster.len()
                ),
            );
        }
    }

    pub(super) fn take_refusals(&mut self) -> Vec<ScenarioRefusal> {
        std::mem::take(&mut self.refusals)
    }
}

// =================================================================================================
// EXPORT (docs/plan_scenarios.md section 2.10)
// =================================================================================================

/// **The bands the export will name**, per faction in ascending `BandId` order, each with its
/// positional scenario id. A faction the world seats beyond the `world.archetypes` counts (a people
/// that broke away mid-game) and a faction with no band are left out and noted: a listed faction
/// needs at least one band, and the roster comes only from the archetype counts.
pub(super) fn export_ids(world: &World, notes: &mut Vec<String>) -> ExportIds {
    let config = world.resource::<SimulationConfig>();
    let seated = config
        .start_archetype_assignment
        .counts(config.start_profile_overrides.archetypes.len())
        .iter()
        .sum::<u32>()
        + 1;
    let roster: Vec<FactionId> = world
        .get_resource::<FactionRegistry>()
        .map(|registry| registry.factions().to_vec())
        .unwrap_or_else(|| vec![FactionId(0)]);
    let mut by_faction: BTreeMap<FactionId, Vec<(BandId, Entity)>> = BTreeMap::new();
    for entity in world.iter_entities() {
        let (Some(id), Some(cohort), Some(_)) = (
            entity.get::<BandId>(),
            entity.get::<PopulationCohort>(),
            entity.get::<ResidentBand>(),
        ) else {
            continue;
        };
        if world.get::<Tile>(cohort.current_tile).is_none() {
            notes.push(format!(
                "band {} stands on no tile and is not exported",
                id.0
            ));
            continue;
        }
        by_faction
            .entry(cohort.faction)
            .or_default()
            .push((*id, entity.id()));
    }
    let mut ids = ExportIds::default();
    for faction in &roster {
        let mut bands = by_faction.remove(faction).unwrap_or_default();
        if faction.0 >= seated {
            notes.push(format!(
                "faction {}: a people the world gained after it was built cannot be named by \
                 world.archetypes, so its {} bands are not exported",
                faction.0,
                bands.len()
            ));
            continue;
        }
        if bands.is_empty() {
            notes.push(format!(
                "faction {} has no band, and a listed faction needs one: it is not exported",
                faction.0
            ));
            continue;
        }
        bands.sort_by_key(|(id, _)| *id);
        let mut listed = Vec::new();
        for (index, (id, entity)) in bands.into_iter().enumerate() {
            let scenario_id = band_scenario_id(*faction, index);
            ids.bands.insert(id, scenario_id.clone());
            listed.push(ExportedBand {
                entity,
                scenario_id,
            });
        }
        ids.faction_bands.insert(*faction, listed);
    }
    ids
}

/// **The `leader` setter**: each band the file says holds a leader is given one through
/// [`BandLeadership::appoint`], the body emergence runs, so the kind is the band's own
/// `HierarchicalEgalitarian` reading's and the name is the catalog's for the band. A band that already
/// has one (the build turn can make one) is left alone, and the practice bar is not touched. A band
/// with no culture layer yet is refused: there is no axis to read, and nothing is defaulted. The same
/// `LeaderEmerged` feed line emergence writes is pushed.
pub(super) fn apply_leaders(
    world: &mut World,
    setup: &ScenarioSetup,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let tick = world.resource::<SimulationTick>().0;
    let map_seed = world.resource::<SimulationConfig>().map_seed;
    let names = world.resource::<LeaderNameCatalogHandle>().get();
    for faction_setup in &setup.factions {
        for (j, entry) in faction_setup.bands.iter().enumerate() {
            if entry.leader != Some(true) {
                continue;
            }
            let Some(band) = setup.spawned.get(&entry.id).copied() else {
                continue;
            };
            let path = format!("factions[{}].bands[{j}].leader", faction_setup.file_index);
            let axis = world
                .resource::<CultureManager>()
                .band_layer_by_owner(CultureOwner::from_band(band))
                .map(|layer| {
                    layer.traits.values()[CultureTraitAxis::HierarchicalEgalitarian.index()]
                        .to_f32()
                });
            let Some(axis) = axis else {
                refusals.push(ScenarioRefusal::new(
                    path,
                    "the band has no culture layer yet, so there is no hierarchical/egalitarian \
                     axis to read its leader's kind from",
                ));
                continue;
            };
            let Some(entity) = world
                .query::<(Entity, &BandId)>()
                .iter(world)
                .find(|(_, id)| **id == band)
                .map(|(entity, _)| entity)
            else {
                continue;
            };
            let Some(mut leadership) = world.get_mut::<BandLeadership>(entity) else {
                refusals.push(ScenarioRefusal::new(
                    path,
                    "the band carries no leadership record",
                ));
                continue;
            };
            if leadership.leader.is_some() {
                continue;
            }
            let leader = leadership.appoint(axis, tick, names.name_for_band(map_seed, band));
            world
                .resource_mut::<CommandEventLog>()
                .push(emergence_entry(tick, faction_setup.faction, band, &leader));
        }
    }
}

/// The shares of a band's brackets, rounded to the export precision. The elders share is the remainder,
/// so the three always sum to one and the loader's division by their sum changes nothing.
fn export_ages(cohort: &PopulationCohort) -> Ages {
    let children = cohort.children.to_f32();
    let working = cohort.working.to_f32();
    let elders = cohort.elders.to_f32();
    let total = children + working + elders;
    if total <= 0.0 {
        return Ages {
            children: 0.0,
            working: 1.0,
            elders: 0.0,
        };
    }
    let children = round_export(children / total);
    let working = round_export(working / total);
    let elders = round_export((1.0 - children - working).max(0.0));
    Ages {
        children,
        working,
        elders,
    }
}

/// The items a band owns, `item -> units`, its detached parties' gear counted back in (a party's kit
/// is the band's, as the outfitting window counts it).
fn band_items(world: &World, entity: Entity) -> BTreeMap<String, u32> {
    let mut items: BTreeMap<String, u32> = BTreeMap::new();
    if let Some(ledger) = world.get::<BandEquipment>(entity) {
        for (item, batches) in ledger.batches() {
            let units: u32 = batches.iter().map(|batch| batch.count).sum();
            if units > 0 {
                *items.entry(item.to_string()).or_default() += units;
            }
        }
    }
    for (item, units) in party_held_items(world, entity) {
        if units > 0 {
            *items.entry(item).or_default() += units;
        }
    }
    items
}

/// **Whole kits a band's items are made of**, greedy in roster order. Each kit takes as many copies
/// as every one of its items still allows, so expanding the result reproduces the items it covers
/// and a second pass over that expansion finds the same kits. The items no kit covers come back too.
fn kits_of(
    equipment: &EquipmentConfig,
    items: &BTreeMap<String, u32>,
) -> (BTreeMap<String, u32>, BTreeMap<String, u32>) {
    let mut remaining = items.clone();
    let mut kits: BTreeMap<String, u32> = BTreeMap::new();
    for kit in &equipment.kits {
        if kit.uses.is_empty() {
            continue;
        }
        let count = kit
            .uses
            .iter()
            .map(|item| remaining.get(item).copied().unwrap_or(0))
            .min()
            .unwrap_or(0);
        if count == 0 {
            continue;
        }
        for item in &kit.uses {
            if let Some(units) = remaining.get_mut(item) {
                *units -= count;
            }
        }
        kits.insert(kit.id.clone(), count);
    }
    remaining.retain(|_, units| *units > 0);
    (kits, remaining)
}

/// The whole units of each material the band holds.
fn band_materials(cohort: &PopulationCohort) -> BTreeMap<String, u32> {
    cohort
        .stores
        .materials()
        .filter_map(|(material, _)| {
            let units = cohort.stores.material_total(material).to_f32().round() as u32;
            (units > 0).then(|| (material.to_string(), units))
        })
        .collect()
}

/// Does any unit the band holds differ from a fresh one: worn, or made at a tier other than the
/// item's default? A scenario's gear always arrives fresh.
fn gear_is_worn(world: &World, entity: Entity, equipment: &EquipmentConfig) -> bool {
    let Some(ledger) = world.get::<BandEquipment>(entity) else {
        return false;
    };
    ledger.batches().any(|(item, batches)| {
        let default_tier = equipment
            .item(item)
            .map(|def| def.default_tier().id.clone());
        batches
            .iter()
            .any(|batch| batch.wear > 0.0 || default_tier.as_deref() != Some(batch.tier.as_str()))
    })
}

/// Would the load of `kits` and `materials` be refused `OverCarry` on the opening window, with the
/// band's larder counted against its carry? Asked of the same functions the window asks.
fn over_carry(
    world: &World,
    faction: FactionId,
    entity: Entity,
    kits: &BTreeMap<String, u32>,
    materials: &BTreeMap<String, u32>,
) -> bool {
    let equipment = world.resource::<EquipmentConfigHandle>().get();
    let carry_cfg = faction_carry(world, faction);
    let window = LoadoutWindow::opened(LoadoutSupply::Grant { carry_budget: None });
    let Some(people) = window_people(world, entity, &carry_cfg) else {
        return false;
    };
    let capacity = window.goods_allowance(&people, &carry_cfg);
    allocation_load(&equipment, &carry_cfg, kits, materials) > capacity
}

/// One band's entry. `first` is the faction's lowest band, the only one that takes an outfit.
fn export_band(
    world: &World,
    faction: FactionId,
    band: &ExportedBand,
    notes: &mut Vec<String>,
) -> Option<BandEntry> {
    let cohort = world.get::<PopulationCohort>(band.entity)?;
    let at = world.get::<Tile>(cohort.current_tile)?.position;
    let equipment = world.resource::<EquipmentConfigHandle>().get();
    let larder: Vec<LarderEntry> = cohort
        .stores
        .food()
        .iter()
        .map(|(class, amount)| LarderEntry {
            class: class.to_string(),
            amount: round_export(amount.to_f32()),
        })
        .filter(|entry| entry.amount > 0.0)
        .collect();
    if cohort.stores.get(FODDER) > crate::scalar::scalar_zero() {
        notes.push(format!(
            "{}: its hay store is not exported (a scenario larder is food only)",
            band.scenario_id
        ));
    }
    let items = band_items(world, band.entity);
    let materials = band_materials(cohort);
    let (kits, uncovered) = kits_of(&equipment, &items);
    if gear_is_worn(world, band.entity, &equipment) {
        notes.push(format!(
            "{}: gear wear and crafted tiers are not exported (every unit comes back fresh)",
            band.scenario_id
        ));
    }
    let outfit = {
        if !uncovered.is_empty() {
            let listed: Vec<String> = uncovered
                .iter()
                .map(|(item, units)| format!("{units} {item}"))
                .collect();
            notes.push(format!(
                "{}: holds gear no kit is made of ({}), which an outfit cannot say",
                band.scenario_id,
                listed.join(", ")
            ));
        }
        // The grant takes only the materials the human archetype's pick list names (the loader's own
        // rule), so a crafted or gathered material the band holds has no key to go in.
        let pickable: Vec<String> = world
            .resource::<ActiveStartProfile>()
            .profile()
            .overrides()
            .human_archetype()
            .map(|archetype| archetype.opening_loadout.pickable_materials.clone())
            .unwrap_or_default();
        let (materials, unpickable): (BTreeMap<String, u32>, BTreeMap<String, u32>) = materials
            .into_iter()
            .partition(|(material, _)| pickable.contains(material));
        if !unpickable.is_empty() {
            let listed: Vec<String> = unpickable
                .iter()
                .map(|(material, units)| format!("{units} {material}"))
                .collect();
            notes.push(format!(
                "{}: holds material an outfit cannot give ({}): not a pickable material",
                band.scenario_id,
                listed.join(", ")
            ));
        }
        if over_carry(world, faction, band.entity, &kits, &materials) {
            notes.push(format!(
                "{}: its larder and outfit exceed what its carry allows, so it will be refused \
                 OverCarry on load; trim by hand",
                band.scenario_id
            ));
        }
        Some(Outfit { kits, materials })
    };
    let auto_craft = world
        .get::<BandBench>(band.entity)
        .is_some_and(|bench| bench.auto)
        .then_some(true);
    let leader = world
        .get::<BandLeadership>(band.entity)
        .is_some_and(|leadership| leadership.leader.is_some())
        .then_some(true);
    if leader.is_some() {
        notes.push(format!(
            "{}: its leader's name and kind are derived again on load (the name from the band id, the \
             kind from the culture axis then), so they can differ from this game's",
            band.scenario_id
        ));
    } else if let Some(practice) = world
        .get::<BandLeadership>(band.entity)
        .map(BandLeadership::practice)
        .filter(|practice| *practice > 0.0)
    {
        let bar = world
            .resource::<LeadershipConfigHandle>()
            .get()
            .practice_threshold;
        notes.push(format!(
            "{}: its leadership practice {practice:.2} of {bar:.2} is not exported",
            band.scenario_id
        ));
    }
    Some(BandEntry {
        id: band.scenario_id.clone(),
        at: [at.x, at.y],
        size: cohort.size,
        ages: export_ages(cohort),
        founding_lines: u16::try_from(cohort.founding_lines.len()).unwrap_or(u16::MAX),
        larder,
        outfit,
        auto_craft,
        leader,
    })
}

/// **The knowledge tags a faction holds at completion**, sorted. A tag with partial progress is
/// noted: a scenario names a tag known or not at all.
fn export_knowledge(world: &World, faction: FactionId, notes: &mut Vec<String>) -> Vec<String> {
    let tags = world.resource::<StartProfileKnowledgeTagsHandle>().get();
    let threshold = world
        .resource::<LadderConfigHandle>()
        .get()
        .knowledge
        .completion_threshold;
    let ledger = world.resource::<DiscoveryProgressLedger>();
    let mut held = Vec::new();
    for (tag, definition) in tags.iter() {
        let progress = ledger.get_progress(faction, definition.discovery_id());
        if knows(ledger, faction, definition.discovery_id(), threshold) {
            held.push(tag.to_string());
        } else if progress > crate::scalar::scalar_zero() {
            notes.push(format!(
                "faction {}: knowledge '{tag}' is part-learned ({:.2}) and is not exported",
                faction.0,
                progress.to_f32()
            ));
        }
    }
    held.sort();
    held
}

/// **Every faction's section, with its bands and knowledge**; the other keys are the section modules'.
pub(super) fn export(
    world: &World,
    ids: &ExportIds,
    notes: &mut Vec<String>,
) -> Vec<FactionSection> {
    ids.faction_bands
        .iter()
        .map(|(faction, listed)| FactionSection {
            faction: faction.0,
            knowledge: export_knowledge(world, *faction, notes),
            bands: listed
                .iter()
                .filter_map(|band| export_band(world, *faction, band, notes))
                .collect(),
            bench: Vec::new(),
            rungs: Vec::new(),
            labor: Vec::new(),
            pens: Vec::new(),
            roads: Vec::new(),
        })
        .collect()
}

/// **What no section owns**: parties in the field, pending dealings, treaties, goodwill, the
/// improvements a tile or a band holds, and the people who lead a band. One line each, counted.
pub(super) fn export_unexpressed(world: &World, notes: &mut Vec<String>) {
    let mut parties: BTreeMap<FactionId, usize> = BTreeMap::new();
    for entity in world.iter_entities() {
        if entity.get::<Expedition>().is_some() {
            if let Some(cohort) = entity.get::<PopulationCohort>() {
                *parties.entry(cohort.faction).or_default() += 1;
            }
        }
    }
    for (faction, count) in parties {
        notes.push(format!(
            "faction {}: {count} party(ies) out in the field are not exported",
            faction.0
        ));
    }
    if let Some(dealings) = world.get_resource::<PendingDealings>() {
        if !dealings.is_empty() {
            notes.push(format!(
                "{} pending dealing(s) are not exported",
                dealings.len()
            ));
        }
    }
    if let Some(treaties) = world.get_resource::<Treaties>() {
        if !treaties.is_empty() {
            notes.push(format!("{} treaty(ies) are not exported", treaties.len()));
        }
    }
    if let Some(goodwill) = world.get_resource::<Goodwill>() {
        if !goodwill.is_empty() {
            notes.push(format!(
                "{} goodwill opinion(s) between peoples are not exported",
                goodwill.len()
            ));
        }
    }
    if let Some(improvements) = world.get_resource::<TileImprovementRegistry>() {
        if !improvements.is_empty() || !improvements.bands().is_empty() {
            notes.push(format!(
                "{} tile improvement(s) and {} band improvement(s) are not exported",
                improvements.len(),
                improvements.bands().len()
            ));
        }
    }
}
