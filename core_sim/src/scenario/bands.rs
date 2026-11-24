//! **The `bands` section of a scenario** (`docs/plan_scenarios.md` section 2.3, slice 1): where a
//! faction's bands stand, how many people, their age shape, founding lines, larder and opening outfit.
//!
//! The file's shape, the name resolution (pass 1), and the setup resource worldgen reads while it
//! spawns the bands. The spawn itself lives in `systems/worldgen.rs`, because a scenario band is
//! spawned by the very function that spawns a profile's, with no special case after it.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use serde::Deserialize;

use super::labor::LaborEntry;
use super::tiles::TileOverride;
use super::{ScenarioContext, ScenarioRefusal};
use crate::components::BandId;
use crate::orders::FactionId;

/// How far the three age shares may sit from summing to one. Shares are written by hand to two
/// decimals, so a sum of `0.99` or `1.01` is a typo to forgive, and `0.9` is a mistake to name.
pub const AGES_SUM_TOLERANCE: f32 = 0.011;

/// One faction's section.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactionSection {
    /// The faction index (`0` is the human).
    pub faction: u32,
    /// Knowledge tags the faction already has, added to its archetype's `starting_knowledge_tags`.
    pub knowledge: Vec<String>,
    /// The faction's bands, in spawn order. The FIRST holds the faction's opening outfit window.
    pub bands: Vec<BandEntry>,
    /// Improvements the faction already holds and builds already declared (`rungs.rs`).
    pub rungs: Vec<super::rungs::RungEntry>,
    /// The faction's opening work rows, replayed in order through `assign_labor` and `work_priority`.
    /// A required key: `[]` for none.
    pub labor: Vec<LaborEntry>,
    /// The faction's pen verbs, replayed after the builds. A required key: `[]` for none.
    pub pens: Vec<super::pens::PenEntry>,
}

/// One band.
#[derive(Debug, Clone, Deserialize)]
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
    pub outfit: Option<Outfit>,
}

/// The shares of a band in each age bracket.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LarderEntry {
    pub class: String,
    pub amount: f32,
}

/// An opening outfit, as `SetStartingLoadout` carries it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
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
            check_band(band, j == 0, &at, ctx, refusals);
        }
    }
}

fn check_band(
    band: &BandEntry,
    is_first: bool,
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
    if !is_first {
        refusals.push(ScenarioRefusal::new(
            &at,
            "only a faction's first band holds the opening outfit window",
        ));
    }
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
    pub rungs: Vec<super::rungs::RungEntry>,
    pub labor: Vec<LaborEntry>,
    /// The faction's pen verbs, replayed after the builds. A required key: `[]` for none.
    pub pens: Vec<super::pens::PenEntry>,
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
                    rungs: section.rungs.clone(),
                    labor: section.labor.clone(),
                    pens: section.pens.clone(),
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
