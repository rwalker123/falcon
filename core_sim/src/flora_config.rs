//! Data-driven **flora species table** — the plant twin of [`crate::fauna_config`].
//!
//! Loaded from `data/flora_config.json`. Where fauna names the animals you herd, this names the
//! plants you gather: each row carries a display name, a [`FloraRole`] display tag, a
//! [`CultivationCeiling`] (how far up the plant ladder it climbs), a per-biome **affinity weight**
//! table, a [`YieldVector`] and a per-species regrowth rate. Mirrors the `fauna_config.rs` loader
//! pattern (baked-in builtin + optional file/env override, `validate()` inside `from_json_str`).
//!
//! # Naming decomposes, it does not add
//!
//! A roster entry says what a tile's **existing** `forage.capacity_by_biome` capacity *is made of*;
//! it never adds capacity on top (`docs/plan_flora_roster.md` §2). The per-biome composition
//! ([`FloraConfig::composition`]) is derived by normalizing the affinity weights, so the shares sum
//! to `1.0` **by construction** and a tile's total can never drift from the human food web's table.
//! That is what makes slice F1 provably economy-neutral rather than neutral-by-promise.
//!
//! **Nothing in the sim reads the yield vector or the ceiling yet** — F1 ships the shape (parsed,
//! validated, exported) and later slices read it, the same "ship the layer, look at a real map, then
//! bet on it" discipline the graze layer and the ladder's behavior primitives used.

use std::{
    collections::{BTreeMap, HashMap},
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use sim_runtime::TerrainType;
use thiserror::Error;

use crate::labor_config::{ForageLaborConfig, NO_FORAGE_CAPACITY};

pub const BUILTIN_FLORA_CONFIG: &str = include_str!("data/flora_config.json");

/// **What a plant is FOR** — a display tag, derived from which component of its [`YieldVector`]
/// dominates. Deliberately **never branched on in the sim**: the vector is the behaviour, and the
/// role is only how the client labels it. Modeling the three "roles" (staple / fodder / cash) as
/// three shapes of one vector rather than three categories is what gives a future market a real data
/// surface instead of a fourth thing to invent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FloraRole {
    /// Human food dominant — the shipped forage path. Every F1 row.
    #[default]
    Staple,
    /// Animal feed dominant — the storable hay that decouples herd size from standing pasture.
    Fodder,
    /// Trade dominant — a crop that occupies food-bearing ground and pays no calories.
    Cash,
}

impl FloraRole {
    /// Stable string key (also the wire `role` field).
    pub fn as_str(&self) -> &'static str {
        match self {
            FloraRole::Staple => "staple",
            FloraRole::Fodder => "fodder",
            FloraRole::Cash => "cash",
        }
    }

    /// Parse the stable string key back (inverse of `as_str`). Unknown/empty strings resolve to the
    /// `Default` (`Staple`) — the shape every shipped row has.
    pub fn from_key(key: &str) -> Self {
        match key {
            "fodder" => FloraRole::Fodder,
            "cash" => FloraRole::Cash,
            _ => FloraRole::Staple,
        }
    }
}

/// **How far up the cultivation ladder a species can climb** — the exact twin of
/// [`crate::fauna_config::HusbandryCeiling`] (`docs/plan_flora_roster.md` §2). The ladder is a
/// *sequence* (wild → tended → field), so a species' reach is a single ceiling rather than two
/// independent flags, which makes the incoherent "sowable but not tendable" state unrepresentable
/// (no `validate()` combination guard needed). `Wild` is a gather-forever stand — an oak's mast is a
/// wild harvest and you do not sow an oak forest on a five-turn horizon. **Default `Field`**
/// (the full ladder), mirroring `HusbandryCeiling::Pen`, so an untagged/future species keeps the
/// pre-ceiling behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CultivationCeiling {
    /// Gather-only. `Cultivate` and `Sow` both refuse.
    Wild,
    /// Reaches the tended rung but never the Field (`Sow` refuses).
    Tended,
    /// The full ladder — the default.
    #[default]
    Field,
}

impl CultivationCeiling {
    /// Stable string key (also the wire `cultivationCeiling` field).
    pub fn as_str(&self) -> &'static str {
        match self {
            CultivationCeiling::Wild => "wild",
            CultivationCeiling::Tended => "tended",
            CultivationCeiling::Field => "field",
        }
    }

    /// Parse the stable string key back (inverse of `as_str`; the rollback restore path). Unknown or
    /// empty strings resolve to the `Default` (`Field`), preserving the full ladder.
    pub fn from_key(key: &str) -> Self {
        match key {
            "wild" => CultivationCeiling::Wild,
            "tended" => CultivationCeiling::Tended,
            _ => CultivationCeiling::Field,
        }
    }

    /// Can this species be **cultivated** (the tended rung)? True for `Tended` and `Field`.
    ///
    /// The twin of `HusbandryCeiling::allows_domestication`. **Unused in F1** — `Cultivate` starts
    /// gating on it in F2, exactly as `HusbandryCeiling` shipped its accessors ahead of its gates.
    pub fn allows_cultivate(&self) -> bool {
        !matches!(self, CultivationCeiling::Wild)
    }

    /// Can this species be **sown** (the Field rung)? True only for `Field`. The twin of
    /// `HusbandryCeiling::allows_pen`. **Unused in F1** — see [`Self::allows_cultivate`].
    pub fn allows_sow(&self) -> bool {
        matches!(self, CultivationCeiling::Field)
    }
}

/// **What one unit of a species' biomass pays, into three different accounts**
/// (`docs/plan_flora_roster.md` §3). A harvest of `B` biomass pays `B × yield.*` into provisions
/// (human food), fodder (animal feed) and trade goods.
///
/// The three "roles" the design names are not three subsystems — they are three characteristic
/// *shapes* of this one vector, which is what lets a future market read a per-species trade rate
/// without re-cutting the schema. **In F1 every row carries today's flat values verbatim**
/// (`labor_config`'s `forage.provisions_per_biomass` and `forage.market.trade_goods_per_biomass`,
/// fodder `0.0`), so today's behaviour is the degenerate case and the slice cannot move the economy.
/// **Parsed and validated only** — nothing reads it in F1.
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(default)]
pub struct YieldVector {
    /// Human food per unit biomass — the shipped forage path.
    pub provisions_per_biomass: f32,
    /// Animal feed per unit biomass — the storable hay a fodder crop grows.
    pub fodder_per_biomass: f32,
    /// Trade value per unit biomass — what differentiates today's single flat scalar.
    pub trade_goods_per_biomass: f32,
}

impl YieldVector {
    /// Does this vector pay **anything at all**? An all-zero vector is a plant that produces nothing
    /// into any account — a row that parses perfectly and can never matter, which is exactly the
    /// class of silently-inert config [`FloraConfig::validate`] exists to reject.
    fn pays_something(&self) -> bool {
        self.provisions_per_biomass > 0.0
            || self.fodder_per_biomass > 0.0
            || self.trade_goods_per_biomass > 0.0
    }

    fn is_finite(&self) -> bool {
        self.provisions_per_biomass.is_finite()
            && self.fodder_per_biomass.is_finite()
            && self.trade_goods_per_biomass.is_finite()
    }
}

/// One species row in the flora table.
#[derive(Debug, Clone, Deserialize)]
pub struct FloraDef {
    /// Player-facing name (also the wire `displayName`).
    pub display_name: String,
    /// **Plural form**, lowercase, reading naturally mid-sentence. Data rather than a heuristic, for
    /// the same reason `SpeciesDef::plural` is: many of these are already collective ("oak mast",
    /// "hazel") and a naive `+s` would produce "hazels".
    pub plural: String,
    /// **Adjectival form**, lowercase ("*hazel* groves").
    pub adjective: String,
    /// **Display tag only** — see [`FloraRole`]. Never branched on in the sim.
    #[serde(default)]
    pub role: FloraRole,
    /// How far up the plant ladder this species climbs — see [`CultivationCeiling`].
    #[serde(default)]
    pub cultivation_ceiling: CultivationCeiling,
    /// **Biome → relative affinity WEIGHT, not a capacity.** A weight is meaningful only against the
    /// other weights on the *same* biome: the engine normalizes them into the shares
    /// [`FloraConfig::composition`] publishes, so the only thing an edit here can do is move share
    /// *between* the named plants of one biome. Retuning the human food web is a `labor_config.json`
    /// edit and must never ride in on a roster change.
    #[serde(default)]
    pub host_biomes: HashMap<TerrainType, f32>,
    /// What one unit of this species' biomass pays — see [`YieldVector`].
    #[serde(rename = "yield")]
    pub yield_: YieldVector,
    /// **Per-species logistic regrowth rate**, the plant twin of `SpeciesDef::regrowth_rate`. Every
    /// F1 row carries `labor_config`'s `forage.ecology.regrowth_rate` verbatim, so regrowth is
    /// unmoved along with everything else; per-species divergence is a later slice.
    pub regrowth_rate: f32,
}

/// One named plant's **share of a biome's forage capacity** — the normalized reading of the affinity
/// weights (`share = weight / Σ weights hosting this biome`). Derived at load, never authored.
#[derive(Debug, Clone, PartialEq)]
pub struct FloraShare {
    /// The species' config key (the stable id; `display_name` is the player-facing string).
    pub species: String,
    /// This species' fraction of the biome's basket. The shares of any hosted biome sum to `1.0`.
    pub share: f32,
}

/// Root flora configuration: the species table plus the **derived** per-biome composition.
///
/// The composition table is built by [`FloraConfig::from_species`], which every construction path —
/// including `Deserialize` — routes through, so **a `FloraConfig` whose share table is stale is
/// unrepresentable**. That matters because the table feeds the wire: it must be identical run to run.
#[derive(Debug, Clone, Default, Resource)]
pub struct FloraConfig {
    pub species: HashMap<String, FloraDef>,
    /// Biome → its composition, each list sorted **weight DESC, then species key ASC**. Private and
    /// derived; read through [`FloraConfig::composition`].
    composition_by_biome: HashMap<TerrainType, Vec<FloraShare>>,
}

/// The literal JSON shape. `FloraConfig`'s own `Deserialize` goes through this and then builds the
/// derived share table, which is what makes a stale table unrepresentable — a bare
/// `#[derive(Deserialize)]` on `FloraConfig` with a `#[serde(skip)]` side table would leave a public
/// path (`serde_json::from_str::<FloraConfig>`) that yields an empty one.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FloraConfigRaw {
    species: HashMap<String, FloraDef>,
}

impl<'de> Deserialize<'de> for FloraConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = FloraConfigRaw::deserialize(deserializer)?;
        Ok(FloraConfig::from_species(raw.species))
    }
}

impl FloraConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_FLORA_CONFIG)
                .expect("builtin flora config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, FloraConfigError> {
        let config: FloraConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, FloraConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| FloraConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        FloraConfig::from_json_str(&contents)
    }

    /// Build the config **and its derived share table** from a species map. The one constructor, so
    /// the table can never be stale.
    fn from_species(species: HashMap<String, FloraDef>) -> Self {
        let composition_by_biome = build_composition(&species);
        Self {
            species,
            composition_by_biome,
        }
    }

    /// **What grows on a `terrain` tile, as normalized shares of its forage capacity.** Empty for a
    /// biome no species hosts (which, after [`FloraConfig::validate_against_forage`], means a biome
    /// that carries no human-edible forage at all). The list is sorted **weight DESC, then species
    /// key ASC** — a total order, so it is identical run to run and safe to put on the wire.
    pub fn composition(&self, terrain: TerrainType) -> &[FloraShare] {
        self.composition_by_biome
            .get(&terrain)
            .map_or(&[], |shares| shares.as_slice())
    }

    /// **What grows on a NAVIGABLE-RIVER hex** — the flora twin of
    /// [`crate::labor_config::ForageLaborConfig::navigable_forage_capacity`], and it mirrors that
    /// function's **two-term** structure exactly, because otherwise the decomposition silently stops
    /// being total.
    ///
    /// A navigable hex's forage capacity is **not** `capacity_by_biome[NavigableRiver]` (that row is
    /// vestigial and bypassed — `labor_config.json`'s own `_comment_navigable_river` says so). It is
    /// `capacity_for(underlying) + navigable_river_forage_bonus`: the valley the channel cut, **plus**
    /// the fishery the channel itself is. So decomposing only the underlying biome would leave the
    /// whole bonus term **unnamed** — precisely the "nameless food"
    /// [`FloraConfig::validate_against_forage`] exists to forbid, leaking in through a path that
    /// validator cannot see, and it would break `Σ share × capacity == capacity` on every navigable
    /// tile.
    ///
    /// So the two baskets are blended, **each weighted by its own capacity term**, then renormalized:
    ///
    /// ```text
    /// weight(s) = share_underlying(s) × capacity_for(underlying)
    ///           + share_channel(s)    × navigable_river_forage_bonus
    /// share(s)  = weight(s) / Σ weights
    /// ```
    ///
    /// A species appearing in **both** terms is **merged into one entry** — no roster edit can produce
    /// two rows for the same plant. Sorted by the same total order [`FloraConfig::composition`] uses.
    ///
    /// Never call this directly from a snapshot/sim path: go through
    /// [`crate::forage::tile_flora_composition`], the single seam that decides *which* of the two
    /// shapes a tile has (the twin of `forage::tile_forage_capacity`).
    pub fn navigable_composition(
        &self,
        underlying: TerrainType,
        forage: &ForageLaborConfig,
    ) -> Vec<FloraShare> {
        let terms = [
            (
                self.composition(underlying),
                forage.capacity_for(underlying),
            ),
            (
                self.composition(TerrainType::NavigableRiver),
                forage.navigable_river_forage_bonus,
            ),
        ];

        // Species key → its absolute biomass across both terms. Merged, so a species hosting the
        // underlying biome *and* the channel lands in exactly one row.
        //
        // A `BTreeMap`, and it must stay one: `HashMap` iteration order is randomized per instance,
        // and f32 addition is not associative, so both the per-species `+=` merge below and the
        // `total` sum would land a ULP apart between two runs in the same process. That ULP divides
        // into every share and changes the snapshot hash — it made `deterministic_snapshots_match`
        // fail roughly one run in four. Sorting `blended` at the end does not save it: the damage is
        // done in the accumulator, before the sort.
        let mut weights: BTreeMap<&str, f32> = BTreeMap::new();
        for (shares, capacity) in terms {
            if !capacity.is_finite() || capacity <= NO_FORAGE_CAPACITY {
                continue;
            }
            for share in shares {
                *weights.entry(share.species.as_str()).or_insert(0.0) += share.share * capacity;
            }
        }

        let total: f32 = weights.values().copied().sum();
        if total <= 0.0 {
            return Vec::new();
        }

        let mut blended: Vec<FloraShare> = weights
            .into_iter()
            .map(|(species, weight)| FloraShare {
                species: species.to_string(),
                share: weight / total,
            })
            .collect();
        blended.sort_by(|a, b| {
            b.share
                .total_cmp(&a.share)
                .then_with(|| a.species.cmp(&b.species))
        });
        blended
    }

    /// The invariants a species row must satisfy **on its own** — the ones that would otherwise make
    /// a row silently inert or the share table incoherent. Runs inside [`FloraConfig::from_json_str`],
    /// so every load path (builtin, default file, `FLORA_CONFIG_PATH` override) is covered — the
    /// `fauna_config.rs` convention.
    ///
    /// The **cross-web** invariants (total coverage of the non-zero forage biomes, and no species
    /// claiming barren ground) need the human food web's capacity table and therefore live in
    /// [`FloraConfig::validate_against_forage`], which the loader runs with `labor_config`'s table.
    pub fn validate(&self) -> Result<(), FloraConfigError> {
        // Iterate in key order so a config with several faults always names the same one first.
        let mut keys: Vec<&String> = self.species.keys().collect();
        keys.sort_unstable();

        for key in keys {
            let def = &self.species[key];

            // A nameless plant cannot be rendered, told about, or picked in a UI.
            if def.display_name.trim().is_empty() {
                return Err(FloraConfigError::EmptyDisplayName {
                    species: key.clone(),
                });
            }

            // A species hosting nowhere can never appear on any map — it parses perfectly and is
            // permanently invisible.
            if def.host_biomes.is_empty() {
                return Err(FloraConfigError::NoHostBiomes {
                    species: key.clone(),
                });
            }

            // Weights are normalized, so a zero weight is a species that hosts a biome and takes
            // none of it, and a negative one would make the shares meaningless (and could cancel a
            // biome's total to zero, dividing by it).
            let mut biomes: Vec<&TerrainType> = def.host_biomes.keys().collect();
            biomes.sort_unstable_by_key(|terrain| **terrain as u8);
            for terrain in biomes {
                let weight = def.host_biomes[terrain];
                if !weight.is_finite() || weight <= 0.0 {
                    return Err(FloraConfigError::NonPositiveWeight {
                        species: key.clone(),
                        biome: *terrain,
                        weight,
                    });
                }
            }

            // A plant that pays nothing into any of the three accounts is a name with no economy
            // behind it.
            if !def.yield_.is_finite() || !def.yield_.pays_something() {
                return Err(FloraConfigError::ZeroYield {
                    species: key.clone(),
                });
            }

            // At `r = 0` the stand's MSY is zero forever: every rung that reads this ecology pays
            // nothing and the species is a dead resource (the `validate_ecology` argument).
            if !def.regrowth_rate.is_finite() || def.regrowth_rate <= 0.0 {
                return Err(FloraConfigError::NonPositiveRegrowth {
                    species: key.clone(),
                    regrowth_rate: def.regrowth_rate,
                });
            }
        }

        Ok(())
    }

    /// **The cross-web invariants** — the roster read against the human food web's own table
    /// (`labor_config.json` → `forage.capacity_by_biome`), which is the only place the decomposition
    /// ruling can actually be enforced:
    ///
    /// - **No nameless food.** A biome with a **non-zero** capacity that *no* species hosts is a tile
    ///   whose food has no name. Rejecting it is what forces breadth before depth: the roster must
    ///   cover every food-bearing biome or not ship. A permissive "unnamed remainder" would quietly
    ///   become permanent.
    /// - **No claiming barren ground.** A species hosting a biome whose capacity is **zero** would
    ///   take a share of nothing — a row that reads as coverage in the table and delivers none, and
    ///   the exact mirror of the "zero must be stated" discipline `capacity_by_biome` enforces.
    ///
    /// Called from the flora load path with `labor_config`'s table passed in, so the table has
    /// exactly one copy.
    pub fn validate_against_forage(
        &self,
        capacity_by_biome: &HashMap<TerrainType, f32>,
    ) -> Result<(), FloraConfigError> {
        for terrain in TerrainType::VALUES {
            let capacity = capacity_by_biome
                .get(&terrain)
                .copied()
                .unwrap_or(NO_FORAGE_CAPACITY);
            let hosted = !self.composition(terrain).is_empty();

            if capacity > NO_FORAGE_CAPACITY && !hosted {
                return Err(FloraConfigError::NamelessBiome {
                    biome: terrain,
                    capacity,
                });
            }
        }

        // Species-major, key-sorted, so the reported fault is stable across runs.
        let mut keys: Vec<&String> = self.species.keys().collect();
        keys.sort_unstable();
        for key in keys {
            let mut biomes: Vec<&TerrainType> = self.species[key].host_biomes.keys().collect();
            biomes.sort_unstable_by_key(|terrain| **terrain as u8);
            for terrain in biomes {
                let capacity = capacity_by_biome
                    .get(terrain)
                    .copied()
                    .unwrap_or(NO_FORAGE_CAPACITY);
                if capacity <= NO_FORAGE_CAPACITY {
                    return Err(FloraConfigError::HostsBarrenBiome {
                        species: key.clone(),
                        biome: *terrain,
                    });
                }
            }
        }

        Ok(())
    }
}

/// Normalize the affinity weights into per-biome shares. `share = weight / Σ weights hosting the
/// biome`, so **the shares of any hosted biome sum to exactly 1** — the decomposition ruling made
/// structural rather than promised.
///
/// The sort (**weight DESC, then species key ASC**) is a *total* order, deliberately: `HashMap`
/// iteration order is unstable, and this table is published on the wire, so ties broken by anything
/// incidental would make the snapshot vary run to run.
fn build_composition(species: &HashMap<String, FloraDef>) -> HashMap<TerrainType, Vec<FloraShare>> {
    let mut weights: HashMap<TerrainType, Vec<(String, f32)>> = HashMap::new();
    for (key, def) in species {
        for (terrain, weight) in &def.host_biomes {
            if !weight.is_finite() || *weight <= 0.0 {
                // `validate()` rejects these; skipping keeps the table coherent for the (test-only)
                // path that builds a config it then rejects.
                continue;
            }
            weights
                .entry(*terrain)
                .or_default()
                .push((key.clone(), *weight));
        }
    }

    weights
        .into_iter()
        .filter_map(|(terrain, mut rows)| {
            // Sort BEFORE summing. `rows` was pushed in `species` HashMap iteration order, which is
            // randomized per instance, and f32 addition is not associative — so a `total` summed
            // here would land a ULP apart between two builds of the same config, and that ULP
            // divides into every published share. The sort is load-bearing for the arithmetic, not
            // just for the wire order.
            rows.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let total: f32 = rows.iter().map(|(_, weight)| *weight).sum();
            if total <= 0.0 {
                return None;
            }
            let shares = rows
                .into_iter()
                .map(|(species, weight)| FloraShare {
                    species,
                    share: weight / total,
                })
                .collect();
            Some((terrain, shares))
        })
        .collect()
}

#[derive(Debug, Error)]
pub enum FloraConfigError {
    #[error("failed to read flora config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse flora config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid flora config: species `{species}` has an empty display_name")]
    EmptyDisplayName { species: String },
    #[error(
        "invalid flora config: species `{species}` hosts no biomes, so it can never appear on any map"
    )]
    NoHostBiomes { species: String },
    #[error(
        "invalid flora config: species `{species}` has a non-positive affinity weight on {biome:?} \
         ({weight}); weights are normalized, so a weight must be finite and greater than 0"
    )]
    NonPositiveWeight {
        species: String,
        biome: TerrainType,
        weight: f32,
    },
    #[error(
        "invalid flora config: species `{species}` has an all-zero yield vector; it would pay \
         nothing into provisions, fodder or trade goods"
    )]
    ZeroYield { species: String },
    #[error(
        "invalid flora config: species `{species}` has a non-positive regrowth_rate \
         ({regrowth_rate}); a stand that never regrows pays nothing forever"
    )]
    NonPositiveRegrowth { species: String, regrowth_rate: f32 },
    #[error(
        "invalid flora config: species `{species}` hosts {biome:?}, which carries no forage at all \
         (forage.capacity_by_biome is 0) — a share of nothing"
    )]
    HostsBarrenBiome { species: String, biome: TerrainType },
    #[error(
        "invalid flora config: {biome:?} carries forage ({capacity}) but no species hosts it — that \
         tile's food has no name; every non-zero forage biome must be covered"
    )]
    NamelessBiome { biome: TerrainType, capacity: f32 },
}

/// Handle for accessing the flora configuration.
#[derive(Resource, Debug, Clone)]
pub struct FloraConfigHandle(pub Arc<FloraConfig>);

impl FloraConfigHandle {
    pub fn new(config: Arc<FloraConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<FloraConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<FloraConfig>) {
        self.0 = config;
    }
}

impl Default for FloraConfigHandle {
    fn default() -> Self {
        Self(FloraConfig::builtin())
    }
}

/// Metadata about the flora configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct FloraConfigMetadata {
    path: Option<PathBuf>,
}

impl FloraConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }
}

/// Load flora configuration from environment (`FLORA_CONFIG_PATH`) or the default data path, falling
/// back to the baked-in builtin.
///
/// Every candidate goes through [`FloraConfig::from_json_str`] **and**
/// [`FloraConfig::validate_against_forage`] against the caller's `forage.capacity_by_biome`, so a
/// roster that would leave a food-bearing biome nameless — or claim barren ground — is rejected and
/// logged at **error** level before it can reach the sim, and the known-good builtin is used instead.
/// The forage table is taken as an argument rather than re-read here so it has exactly one copy.
pub fn load_flora_config_from_env(
    forage_capacity_by_biome: &HashMap<TerrainType, f32>,
) -> (Arc<FloraConfig>, FloraConfigMetadata) {
    let override_path = env::var("FLORA_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/flora_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match FloraConfig::from_file(&path).and_then(|config| {
            config
                .validate_against_forage(forage_capacity_by_biome)
                .map(|()| config)
        }) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "flora_config.loaded=file"
                );
                return (Arc::new(config), FloraConfigMetadata::new(Some(path)));
            }
            // A broken invariant is an operator error, not a missing file: the config that *was*
            // found says something incoherent. Shout about it.
            Err(err @ (FloraConfigError::Read { .. } | FloraConfigError::Parse(_))) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "flora_config.load_failed"
                );
            }
            Err(err) => {
                tracing::error!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "flora_config.invalid_rejected"
                );
            }
        }
    }

    let config = FloraConfig::builtin();
    // The builtin is checked too: it is the fallback, so if the *forage* table drifted out from
    // under the roster the coverage hole is here as well and must be loud rather than silent.
    if let Err(err) = config.validate_against_forage(forage_capacity_by_biome) {
        tracing::error!(
            target: "shadow_scale::config",
            error = %err,
            "flora_config.builtin_coverage_broken"
        );
    }
    tracing::info!(
        target: "shadow_scale::config",
        "flora_config.loaded=builtin"
    );
    (config, FloraConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labor_config::LaborConfig;

    /// The shipped human food web, as the roster must be read against it.
    fn builtin_forage_capacities() -> HashMap<TerrainType, f32> {
        LaborConfig::from_json_str(crate::labor_config::BUILTIN_LABOR_CONFIG)
            .expect("builtin labor config should parse and validate")
            .forage
            .capacity_by_biome
    }

    /// A minimal, valid one-species config to mutate in the rejection tests.
    fn one_species_json(body: &str) -> String {
        format!("{{ \"species\": {{ \"probe\": {body} }} }}")
    }

    const VALID_BODY: &str = r#"{
        "display_name": "Probe",
        "plural": "probes",
        "adjective": "probe",
        "role": "staple",
        "cultivation_ceiling": "tended",
        "host_biomes": { "AlluvialPlain": 1.0 },
        "yield": { "provisions_per_biomass": 0.05, "fodder_per_biomass": 0.0, "trade_goods_per_biomass": 0.005 },
        "regrowth_rate": 0.25
    }"#;

    #[test]
    fn builtin_parses_and_validates() {
        let config = FloraConfig::builtin();
        assert_eq!(
            config.species.len(),
            13,
            "the F1 roster is 13 broad families (12 keyed on biome + river_fish, the channel itself)"
        );
        // The channel is named separately from the valley it cut — see `navigable_composition`.
        assert_eq!(
            config.species["river_fish"]
                .host_biomes
                .keys()
                .collect::<Vec<_>>(),
            vec![&TerrainType::NavigableRiver]
        );
        assert!(config.species.contains_key("hazel"));
        assert!(config.species.contains_key("shellfish_beds"));
        assert!(config.species.contains_key("arctic_greens"));
        // The ceilings are declared, not defaulted.
        assert_eq!(
            config.species["oak_mast"].cultivation_ceiling,
            CultivationCeiling::Wild
        );
        assert_eq!(
            config.species["wild_emmer"].cultivation_ceiling,
            CultivationCeiling::Field
        );
        config
            .validate_against_forage(&builtin_forage_capacities())
            .expect("the builtin roster must cover the shipped forage table");
    }

    #[test]
    fn the_probe_fixture_is_valid() {
        let config = FloraConfig::from_json_str(&one_species_json(VALID_BODY))
            .expect("the fixture the rejection tests mutate must itself be valid");
        assert_eq!(config.composition(TerrainType::AlluvialPlain).len(), 1);
    }

    #[test]
    fn validate_rejects_an_empty_display_name() {
        let body = VALID_BODY.replace("\"Probe\"", "\"\"");
        assert!(matches!(
            FloraConfig::from_json_str(&one_species_json(&body)),
            Err(FloraConfigError::EmptyDisplayName { .. })
        ));
    }

    #[test]
    fn validate_rejects_empty_host_biomes() {
        let body = VALID_BODY.replace("{ \"AlluvialPlain\": 1.0 }", "{}");
        assert!(matches!(
            FloraConfig::from_json_str(&one_species_json(&body)),
            Err(FloraConfigError::NoHostBiomes { .. })
        ));
    }

    #[test]
    fn validate_rejects_a_non_positive_weight() {
        let body = VALID_BODY.replace("\"AlluvialPlain\": 1.0", "\"AlluvialPlain\": 0.0");
        assert!(matches!(
            FloraConfig::from_json_str(&one_species_json(&body)),
            Err(FloraConfigError::NonPositiveWeight { .. })
        ));
    }

    #[test]
    fn validate_rejects_an_all_zero_yield_vector() {
        let body = VALID_BODY.replace("0.05", "0.0").replace("0.005", "0.0");
        assert!(matches!(
            FloraConfig::from_json_str(&one_species_json(&body)),
            Err(FloraConfigError::ZeroYield { .. })
        ));
    }

    #[test]
    fn validate_rejects_a_non_positive_regrowth_rate() {
        let body = VALID_BODY.replace("\"regrowth_rate\": 0.25", "\"regrowth_rate\": 0.0");
        assert!(matches!(
            FloraConfig::from_json_str(&one_species_json(&body)),
            Err(FloraConfigError::NonPositiveRegrowth { .. })
        ));
    }

    #[test]
    fn validate_rejects_a_nameless_food_bearing_biome() {
        // One species on one biome cannot cover the other 29 food-bearing biomes.
        let config = FloraConfig::from_json_str(&one_species_json(VALID_BODY)).expect("valid");
        assert!(matches!(
            config.validate_against_forage(&builtin_forage_capacities()),
            Err(FloraConfigError::NamelessBiome { .. })
        ));
    }

    #[test]
    fn validate_rejects_a_species_hosting_a_zero_capacity_biome() {
        // The builtin roster, plus one row claiming a stated-zero biome (Glacier).
        let mut config = FloraConfig::builtin().as_ref().clone();
        let mut def = config.species["arctic_greens"].clone();
        def.host_biomes.insert(TerrainType::Glacier, 0.3);
        let mut species = config.species.clone();
        species.insert("arctic_greens".to_string(), def);
        config = FloraConfig::from_species(species);
        assert!(matches!(
            config.validate_against_forage(&builtin_forage_capacities()),
            Err(FloraConfigError::HostsBarrenBiome { .. })
        ));
    }

    #[test]
    fn composition_is_sorted_weight_desc_then_key_asc() {
        let config = FloraConfig::builtin();
        for terrain in TerrainType::VALUES {
            let shares = config.composition(terrain);
            for pair in shares.windows(2) {
                let ordered = pair[0].share > pair[1].share
                    || (pair[0].share == pair[1].share && pair[0].species < pair[1].species);
                assert!(
                    ordered,
                    "{terrain:?} composition is not deterministically ordered"
                );
            }
        }
    }

    /// The per-biome share table must be **bit-identical** build to build, not merely sorted.
    ///
    /// The twin of the navigable guard below, one layer down: `build_composition` collects each
    /// biome's rows in `species` HashMap order, so its `Σ weights` denominator is exposed to the
    /// same non-associative-f32 drift. Sorting the rows for the wire does not fix the arithmetic —
    /// the sum has to happen after the sort — and this is what says so.
    #[test]
    fn the_share_table_is_bit_identical_across_builds() {
        const REPEATS: usize = 64;

        let baseline = FloraConfig::builtin();
        for repeat in 1..REPEATS {
            let again = FloraConfig::builtin();
            for terrain in TerrainType::VALUES {
                let first = baseline.composition(terrain);
                let second = again.composition(terrain);
                assert_eq!(
                    first.len(),
                    second.len(),
                    "{terrain:?} composition changed length on build {repeat}"
                );
                for (a, b) in first.iter().zip(second) {
                    assert_eq!(
                        a.species, b.species,
                        "{terrain:?} composition reordered on build {repeat}"
                    );
                    assert_eq!(
                        a.share.to_bits(),
                        b.share.to_bits(),
                        "{terrain:?} share for {} drifted on build {repeat}: {} vs {}",
                        a.species,
                        a.share,
                        b.share
                    );
                }
            }
        }
    }

    /// The navigable blend must be **bit-identical** call to call, not merely sorted.
    ///
    /// It merges two baskets through a map keyed by species. With a `HashMap` there, iteration
    /// order is randomized per instance, and since f32 addition is not associative the merged
    /// weights and their `total` land a ULP apart between calls — which divides into every share
    /// and changes the published snapshot hash. That is the shape of the flake this guards:
    /// `deterministic_snapshots_match` failed on roughly a quarter of runs because two simulations
    /// in one process disagreed in the last digit of a river tile's flora shares.
    ///
    /// Repeating the call is what exercises it: each call builds a fresh map, so a randomized
    /// container gives a fresh order every time.
    #[test]
    fn navigable_composition_is_bit_identical_across_calls() {
        const REPEATS: usize = 64;

        let config = FloraConfig::builtin();
        let forage = LaborConfig::from_json_str(crate::labor_config::BUILTIN_LABOR_CONFIG)
            .expect("builtin labor config should parse and validate")
            .forage;

        for terrain in TerrainType::VALUES {
            let baseline = config.navigable_composition(terrain, &forage);
            for repeat in 1..REPEATS {
                let again = config.navigable_composition(terrain, &forage);
                assert_eq!(
                    baseline.len(),
                    again.len(),
                    "{terrain:?} navigable blend changed length on repeat {repeat}"
                );
                for (first, second) in baseline.iter().zip(&again) {
                    assert_eq!(
                        first.species, second.species,
                        "{terrain:?} navigable blend reordered on repeat {repeat}"
                    );
                    assert_eq!(
                        first.share.to_bits(),
                        second.share.to_bits(),
                        "{terrain:?} navigable share for {} drifted on repeat {repeat}: \
                         {} vs {}",
                        first.species,
                        first.share,
                        second.share
                    );
                }
            }
        }
    }
}
