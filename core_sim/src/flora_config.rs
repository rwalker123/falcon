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
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::math::UVec2;
use bevy::prelude::Resource;
use serde::Deserialize;
use sim_runtime::TerrainType;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};
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

/// **What one unit of a species' biomass pays, into every account it pays into**
/// (`docs/plan_flora_roster.md` §3). A harvest of `B` biomass pays `B × yield.*` into provisions
/// (human food) and fodder (animal feed), plus `B × per_biomass` of each material it names.
///
/// **The `trade_goods_per_biomass` scalar is RETIRED** (arc #527). It was written on every harvest
/// and read by nothing — no `take(TRADE_GOODS)` existed anywhere — while the `materials` list beside
/// it credited the same take's concrete stuff. A scalar collapses exactly the distinction the
/// crafting arc exists to keep (cotton fibre and hay straw are both `fibre` and are not the same
/// thing), so the flattened duplicate went and the vector-valued account stayed.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct YieldVector {
    /// Human food per unit biomass — the shipped forage path.
    pub provisions_per_biomass: f32,
    /// Animal feed per unit biomass — the storable hay a fodder crop grows.
    pub fodder_per_biomass: f32,
    /// **What the plant is MADE OF** — bast, cordage fibre — per unit of biomass gathered
    /// (`docs/plan_crafting_and_materials.md` §2). **The same type, and the same shape, the fauna
    /// roster's `hunt_yield.materials` carries**: the yield edge is deliberately neither
    /// plant-shaped nor animal-shaped, and a deposit's will be the same again.
    ///
    /// An empty list is the ordinary case — most plants are food and nothing else. Validated
    /// against the materials table at load
    /// ([`crate::materials_config::MaterialsConfig::validate_yield`]).
    pub materials: Vec<crate::materials_config::MaterialYieldDef>,
}

impl YieldVector {
    /// Does this vector pay **anything at all**? A vector paying into no account is a plant that
    /// produces nothing — a row that parses perfectly and can never matter, which is exactly the
    /// class of silently-inert config [`FloraConfig::validate`] exists to reject.
    ///
    /// **A material row counts.** The five cash crops read `0` provisions and `0` fodder and are
    /// paid entirely in stuff (cotton and flax in fibre; tobacco, tea and grapes in their own
    /// materials), so testing the two scalars alone would reject every one of them at boot. This is
    /// the assertion that turns *"did we silently stop paying a species"* into a load failure rather
    /// than a quiet zero, so it has to count every account a harvest can land in.
    fn pays_something(&self) -> bool {
        self.provisions_per_biomass > 0.0
            || self.fodder_per_biomass > 0.0
            || !self.materials.is_empty()
    }

    fn is_finite(&self) -> bool {
        self.provisions_per_biomass.is_finite() && self.fodder_per_biomass.is_finite()
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
    /// **Does this species stand in the soil a crew tending or sowing this tile is actually
    /// working?** `true` for everything rooted in ground — the default, and every row but three.
    ///
    /// **It answers a different question from [`CultivationCeiling`], which is why it is its own
    /// field.** The ceiling says *how far up the ladder you can take this plant*; this says *whether
    /// working the ground can take the plant away*. Ten shipped rows are `Wild`, and they split
    /// cleanly on this second question: you genuinely can clear oak mast, pine nut, mesquite,
    /// cloudberry, rock tripe and arctic greens off ground you are tending, so gating on the ceiling
    /// would shield those six too and quietly weaken every Cultivate on woodland and scrub. Kelp,
    /// shellfish and river fish are the ones the ceiling cannot speak for: they are not growing in
    /// the soil at all, and no amount of weeding a riverbank thins the fish in the channel.
    ///
    /// **The two reweight seams both honour it** (`crate::forage`): weeding takes share only from
    /// members standing in the worked ground and, if those cannot cover the gain, the favored share
    /// rises by only what was there to take; a Field's crop takes the **remainder** left by the
    /// members that stand outside it, not the whole basket. Without this, a Sow on a navigable hex
    /// deleted a river's whole fishery outright.
    ///
    /// A species that stands outside the worked ground can never be *committed* to — a crew cannot
    /// favor what it cannot clear — which [`FloraConfig::validate`] enforces at load rather than
    /// leaving to a runtime surprise.
    #[serde(default = "default_stands_in_worked_ground")]
    pub stands_in_worked_ground: bool,
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

/// **THE WIRE'S TOTAL ORDER FOR A BASKET** — share DESC, then species key ASC — applied in place.
///
/// Stated once because it is a *total* order that other code depends on being one: the first entry
/// of a sorted basket is its dominant plant, which is what `forage::default_species_for_rung` reads
/// without a second sort, and a basket reaches the snapshot, where a differently-ordered f32
/// addition is a hash flake. Every seam that builds or reshapes a basket — the affinity blend, the
/// per-tile realization, both rung reweights and the standing interpolation — ends on this call, so
/// none of them can come to disagree about what "sorted" means.
pub fn sort_basket(shares: &mut [FloraShare]) {
    shares.sort_by(|a, b| {
        b.share
            .total_cmp(&a.share)
            .then_with(|| a.species.cmp(&b.species))
    });
}

/// Root flora configuration: the species table plus the **derived** per-biome composition.
///
/// The composition table is built by [`FloraConfig::from_species`], which every construction path —
/// including `Deserialize` — routes through, so **a `FloraConfig` whose share table is stale is
/// unrepresentable**. That matters because the table feeds the wire: it must be identical run to run.
#[derive(Debug, Clone, Resource)]
pub struct FloraConfig {
    pub species: HashMap<String, FloraDef>,
    /// Biome → its composition, each list sorted **weight DESC, then species key ASC**. Private and
    /// derived; read through [`FloraConfig::composition`]. This is the **affinity** table — *what CAN
    /// grow here* — not what any one tile actually grows (that is per-tile realization, §10).
    composition_by_biome: HashMap<TerrainType, Vec<FloraShare>>,
    /// The per-tile realization draws `k ∈ [min, max]` species from a biome's affinity roster
    /// (clamped to how many it hosts), so some alluvial tiles are wheat and others tobacco rather than
    /// every tile carrying a diluted slice of all of them (§10). Playtest dials; validated
    /// `1 <= min <= max`.
    pub realized_species_min: usize,
    pub realized_species_max: usize,
}

impl Default for FloraConfig {
    fn default() -> Self {
        Self {
            species: HashMap::new(),
            composition_by_biome: HashMap::new(),
            realized_species_min: DEFAULT_REALIZED_SPECIES_MIN,
            realized_species_max: DEFAULT_REALIZED_SPECIES_MAX,
        }
    }
}

/// Default lower/upper bound on the per-tile realized species count (§10 dials).
const DEFAULT_REALIZED_SPECIES_MIN: usize = 2;
const DEFAULT_REALIZED_SPECIES_MAX: usize = 4;
fn default_realized_species_min() -> usize {
    DEFAULT_REALIZED_SPECIES_MIN
}
fn default_realized_species_max() -> usize {
    DEFAULT_REALIZED_SPECIES_MAX
}

/// **A plant is in the ground unless the row says otherwise** — the default for
/// [`FloraDef::stands_in_worked_ground`], so every row that omits it keeps the pre-guard behaviour
/// (weedable, and displaced by a Field) byte for byte. Only the handful of members that are not
/// growing in soil at all state the exception.
const DEFAULT_STANDS_IN_WORKED_GROUND: bool = true;
fn default_stands_in_worked_ground() -> bool {
    DEFAULT_STANDS_IN_WORKED_GROUND
}

/// A fixed salt so a tile's realization draw is decorrelated from every other per-tile hash keyed on
/// the same `(map_seed, tile)` (e.g. hydrology's flat-tie jitter). Any non-zero constant works; this
/// one is arbitrary.
const FLORA_REALIZATION_SALT: u64 = 0xF10A_4EA1_5EED_0010;

/// The literal JSON shape. `FloraConfig`'s own `Deserialize` goes through this and then builds the
/// derived share table, which is what makes a stale table unrepresentable — a bare
/// `#[derive(Deserialize)]` on `FloraConfig` with a `#[serde(skip)]` side table would leave a public
/// path (`serde_json::from_str::<FloraConfig>`) that yields an empty one.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FloraConfigRaw {
    species: HashMap<String, FloraDef>,
    #[serde(default = "default_realized_species_min")]
    realized_species_min: usize,
    #[serde(default = "default_realized_species_max")]
    realized_species_max: usize,
}

impl<'de> Deserialize<'de> for FloraConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = FloraConfigRaw::deserialize(deserializer)?;
        let mut config = FloraConfig::from_species(raw.species);
        config.realized_species_min = raw.realized_species_min;
        config.realized_species_max = raw.realized_species_max;
        Ok(config)
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
            realized_species_min: DEFAULT_REALIZED_SPECIES_MIN,
            realized_species_max: DEFAULT_REALIZED_SPECIES_MAX,
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
        self.blend_navigable(
            self.composition(underlying),
            forage.capacity_for(underlying),
            forage.navigable_river_forage_bonus,
        )
    }

    /// **The two-term navigable blend**, factored out so the affinity path
    /// ([`Self::navigable_composition`]) and the per-tile realized path
    /// ([`Self::realized_navigable_composition`]) share one implementation and cannot drift. The
    /// `underlying_shares` are whatever basket the caller wants blended in — the affinity roster, or a
    /// tile's realized subset of it — while the **channel term is always the un-realized
    /// `NavigableRiver` basket** (§10: realize the underlying valley, leave the fishery as-is).
    fn blend_navigable(
        &self,
        underlying_shares: &[FloraShare],
        underlying_capacity: f32,
        channel_bonus: f32,
    ) -> Vec<FloraShare> {
        let terms = [
            (underlying_shares, underlying_capacity),
            (self.composition(TerrainType::NavigableRiver), channel_bonus),
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
        sort_basket(&mut blended);
        blended
    }

    /// **What is *actually growing* on this tile** — the per-tile REALIZATION of an ordinary biome's
    /// affinity roster (§10). A deterministic, seeded, weighted subset of [`Self::composition`]: for
    /// tile `(x, y)` under `map_seed` it draws `k ∈ [realized_species_min, realized_species_max]`
    /// species (clamped to how many the biome hosts) by weighted sampling without replacement
    /// (probability ∝ affinity share), then renormalizes the picked shares to sum to `1`. So two
    /// alluvial tiles carry *different* baskets — one wheat, one tobacco — instead of every tile
    /// carrying a diluted slice of all of them.
    ///
    /// Pure function of `(map_seed, tile, terrain, affinities)`: no stored state, so it is
    /// deterministic under rollback for free and adds nothing to the snapshot/wire (realization is
    /// *derived*). Read through [`crate::forage::tile_flora_composition`], the single seam.
    pub fn realized_composition(
        &self,
        terrain: TerrainType,
        tile: UVec2,
        map_seed: u64,
    ) -> Vec<FloraShare> {
        self.realize_shares(self.composition(terrain), tile, map_seed)
    }

    /// The navigable-hex twin of [`Self::realized_composition`]: realize the **underlying** valley's
    /// basket per tile, then blend the un-realized channel term (`river_fish`) back in — the fishery a
    /// giant river *always* is (§10). Same two-term blend the affinity path uses, so capacity and
    /// composition still cannot drift.
    pub fn realized_navigable_composition(
        &self,
        underlying: TerrainType,
        forage: &ForageLaborConfig,
        tile: UVec2,
        map_seed: u64,
    ) -> Vec<FloraShare> {
        let realized_underlying = self.realize_shares(self.composition(underlying), tile, map_seed);
        self.blend_navigable(
            &realized_underlying,
            forage.capacity_for(underlying),
            forage.navigable_river_forage_bonus,
        )
    }

    /// The seeded weighted-subset draw shared by both realized paths.
    ///
    /// **Efraimidis–Spirakis** weighted sampling without replacement: for each hosted species draw
    /// `u ∈ (0, 1]` from a hash of `(tile_hash, species_key)`, key it `u^(1/weight)`, and take the `k`
    /// species with the **largest** keys — ~unbiased, so the map-wide mix tracks the affinity table.
    /// Determinism discipline (the codebase's share-table rule): the input is already a total-ordered
    /// list, ties break by species key ascending, and nothing reads `HashMap` iteration order.
    fn realize_shares(
        &self,
        affinity: &[FloraShare],
        tile: UVec2,
        map_seed: u64,
    ) -> Vec<FloraShare> {
        let hosted = affinity.len();
        // Nothing to subset: a one-species (or empty) biome realizes to itself, byte-identical.
        if hosted <= 1 {
            return affinity.to_vec();
        }
        // Per-tile entropy — a pure splitmix hash, salted so it cannot correlate with another per-tile
        // hash keyed on the same `(map_seed, tile)`.
        let tile_hash = splitmix64(map_seed ^ FLORA_REALIZATION_SALT ^ fnv_tile(tile.x, tile.y));

        // Draw k ∈ [min, max], clamped to what the biome actually hosts. Uses a distinct mix of the
        // tile hash so the count draw does not correlate with the per-species keys below.
        let lo = self.realized_species_min.max(1);
        let hi = self.realized_species_max.max(lo);
        let span = (hi - lo + 1) as u64;
        let k = (lo + (splitmix64(tile_hash) % span) as usize).min(hosted);

        // Key every hosted species; take the k largest. `weight` is the affinity share (> 0 for every
        // hosted row, so the exponent is finite).
        let mut keyed: Vec<(f64, &FloraShare)> = affinity
            .iter()
            .map(|share| {
                let u = hash_unit_f64(tile_hash, &share.species);
                let key = u.powf(1.0 / share.share.max(f32::MIN_POSITIVE) as f64);
                (key, share)
            })
            .collect();
        keyed.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| a.1.species.cmp(&b.1.species))
        });

        // Renormalize the picked shares to sum to 1 (per-tile neutrality: the tile still yields its
        // full biome capacity gathered wild, just composed of different species).
        let total: f32 = keyed[..k].iter().map(|(_, s)| s.share).sum();
        let mut realized: Vec<FloraShare> = keyed[..k]
            .iter()
            .map(|(_, s)| FloraShare {
                species: s.species.clone(),
                share: s.share / total,
            })
            .collect();
        // Publish in the wire order every basket uses: share DESC, then species key ASC.
        sort_basket(&mut realized);
        realized
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

            // **A CREW CANNOT FAVOR WHAT IT CANNOT CLEAR.** Committing a patch to a species means
            // weeding the rest of the basket toward it (rung 2) or sowing over the clearable
            // remainder (rung 3) — both of which presuppose the crop itself stands in the ground
            // being worked. A row that is cultivable *and* stands outside the worked ground is
            // therefore incoherent rather than merely odd, and it would surface as a favored share
            // that can never be raised. The shipped roster satisfies this by construction (all
            // three protected rows are `wild`, which cannot be committed to at all), so this is a
            // load-time rejection rather than a runtime branch nothing exercises.
            if !def.stands_in_worked_ground && def.cultivation_ceiling.allows_cultivate() {
                return Err(FloraConfigError::CultivableOutsideWorkedGround {
                    species: key.clone(),
                    ceiling: def.cultivation_ceiling,
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

        // The per-tile realization draws `k ∈ [min, max]` (§10). `min < 1` would let a tile realize
        // *no* species (an empty basket — nameless food, silently); `max < min` is an empty range the
        // draw's modulo would panic on. `1 <= min <= max` is the only coherent shape.
        if self.realized_species_min < 1 || self.realized_species_min > self.realized_species_max {
            return Err(FloraConfigError::InvalidRealizedSpeciesRange {
                min: self.realized_species_min,
                max: self.realized_species_max,
            });
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

    /// Reconcile every species' material yield with the materials table — the plant twin of
    /// [`crate::fauna_config::FaunaConfig::validate_against_materials`], and the *same* check, since
    /// the yield edge is the same type on both configs. Run by [`load_flora_config_from_env`] with
    /// the loaded table passed in so it keeps one copy.
    pub fn validate_against_materials(
        &self,
        materials: &crate::materials_config::MaterialsConfig,
    ) -> Result<(), crate::materials_config::MaterialYieldError> {
        let mut keys: Vec<&String> = self.species.keys().collect();
        keys.sort_unstable();
        for key in keys {
            materials.validate_yield(
                &format!("species.{key}.yield"),
                &self.species[key].yield_.materials,
            )?;
        }
        Ok(())
    }

    /// **Can working this tile's ground clear this species out of the basket?** — the roster lookup
    /// behind both reweight seams (`crate::forage::patch_composition`). See
    /// [`FloraDef::stands_in_worked_ground`] for what the property means and why it is not the
    /// cultivation ceiling.
    ///
    /// **A species the roster does not name reads `true`.** That is the same answer
    /// `crate::forage::basket_rate` and `materials_for` give an unknown key — the pre-guard
    /// behaviour — so a synthetic fixture species weeds exactly as it did before this existed. The
    /// protection is a stated exception, never an inference from absence.
    pub fn stands_in_worked_ground(&self, species: &str) -> bool {
        self.species
            .get(species)
            .map(|def| def.stands_in_worked_ground)
            .unwrap_or(DEFAULT_STANDS_IN_WORKED_GROUND)
    }

    /// **The material rows a species gives**, by config key — the plant twin of
    /// [`crate::fauna_config::FaunaConfig::hunt_materials_for`]. Empty for an unknown key, which is
    /// the honest answer rather than a panic (a test fixture may name a synthetic species).
    pub fn materials_for(&self, species: &str) -> &[crate::materials_config::MaterialYieldDef] {
        self.species
            .get(species)
            .map(|def| def.yield_.materials.as_slice())
            .unwrap_or_default()
    }
}

/// Normalize the affinity weights into per-biome shares. `share = weight / Σ weights hosting the
/// biome`, so **the shares of any hosted biome sum to exactly 1** — the decomposition ruling made
/// structural rather than promised.
///
/// The sort (**weight DESC, then species key ASC**) is a *total* order, deliberately: `HashMap`
/// iteration order is unstable, and this table is published on the wire, so ties broken by anything
/// incidental would make the snapshot vary run to run.
/// splitmix64 — a pure, deterministic 64-bit mixer (the same recipe `hydrology.rs` uses for its
/// flat-tie jitter). No state, no RNG: the same input always produces the same output.
#[inline]
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// FNV-1a over a tile's `(x, y)` — a deterministic, order-sensitive hash of the coordinate, so two
/// tiles get uncorrelated realization draws.
#[inline]
fn fnv_tile(x: u32, y: u32) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for byte in x.to_le_bytes().into_iter().chain(y.to_le_bytes()) {
        h ^= byte as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// FNV-1a over a species key.
#[inline]
fn fnv_str(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for byte in s.bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A deterministic hash of `(tile_hash, species_key)` into `(0, 1]` — the Efraimidis–Spirakis
/// `u_i`. Never `0`, so the `u^(1/weight)` key is always finite.
#[inline]
fn hash_unit_f64(tile_hash: u64, species: &str) -> f64 {
    let bits = splitmix64(tile_hash ^ fnv_str(species));
    // 53-bit mantissa in [1, 2^53] → (0, 1]. The `+ 1` keeps it strictly positive.
    ((bits >> 11) as f64 + 1.0) / ((1u64 << 53) as f64)
}

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
    /// The cross-config half: a species' `yield.materials` row that the materials table refuses.
    #[error("invalid flora config: {0}")]
    MaterialYield(#[from] crate::materials_config::MaterialYieldError),
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
    #[error(
        "invalid flora config: per-tile realization range is incoherent (min {min}, max {max}); it \
         must satisfy 1 <= min <= max"
    )]
    InvalidRealizedSpeciesRange { min: usize, max: usize },
    #[error(
        "invalid flora config: species `{species}` does not stand in the worked ground yet carries \
         cultivation_ceiling `{}` — a crew cannot favor a crop it cannot clear the ground for",
        ceiling.as_str()
    )]
    CultivableOutsideWorkedGround {
        species: String,
        ceiling: CultivationCeiling,
    },
}

impl ConfigLoadError for FloraConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
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

/// Load flora configuration from environment (`FLORA_CONFIG_PATH`) or the default data path.
///
/// The loaded file goes through [`FloraConfig::from_json_str`] **and**
/// [`FloraConfig::validate_against_forage`] against the caller's `forage.capacity_by_biome`, so a
/// roster that would leave a food-bearing biome nameless — or claim barren ground — is a boot
/// panic, not a silent swap to the builtin ([`crate::config_load::resolve_config`]). The forage
/// table is taken as an argument rather than re-read here so it has exactly one copy — and so is the
/// materials table, against which every species' `yield.materials` is reconciled
/// ([`FloraConfig::validate_against_materials`]).
pub fn load_flora_config_from_env(
    forage_capacity_by_biome: &HashMap<TerrainType, f32>,
    materials: &crate::materials_config::MaterialsConfig,
) -> (Arc<FloraConfig>, FloraConfigMetadata) {
    let (config, source) = load_config_from_env(
        "FLORA_CONFIG_PATH",
        "flora_config",
        "src/data/flora_config.json",
        FloraConfig::builtin,
        |path| -> Result<FloraConfig, FloraConfigError> {
            let config = FloraConfig::from_file(path)?;
            config.validate_against_forage(forage_capacity_by_biome)?;
            config.validate_against_materials(materials)?;
            Ok(config)
        },
    );

    if source.is_none() {
        // The builtin is checked too: it is the fallback, so if the *forage* table drifted out from
        // under the roster the coverage hole is here as well and must be loud rather than silent.
        // Deliberately not fatal — unlike a file the operator edited, there is no alternative
        // roster to point at, and `builtin_parses_and_validates` already pins the shipped pair.
        if let Err(err) = config.validate_against_forage(forage_capacity_by_biome) {
            tracing::error!(
                target: "shadow_scale::config",
                error = %err,
                "flora_config.builtin_coverage_broken"
            );
        }
        if let Err(err) = config.validate_against_materials(materials) {
            tracing::error!(
                target: "shadow_scale::config",
                error = %err,
                "flora_config.builtin_material_yield_broken"
            );
        }
    }

    (config, FloraConfigMetadata::new(source))
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

    /// The same, with an explicit per-tile realization range (§10).
    fn one_species_json_with_range(min: i64, max: i64) -> String {
        format!(
            "{{ \"species\": {{ \"probe\": {VALID_BODY} }}, \
             \"realized_species_min\": {min}, \"realized_species_max\": {max} }}"
        )
    }

    #[test]
    fn realized_species_range_defaults_to_two_through_four() {
        let config = FloraConfig::from_json_str(&one_species_json(VALID_BODY))
            .expect("a config with no realization dials should default them");
        assert_eq!(config.realized_species_min, 2);
        assert_eq!(config.realized_species_max, 4);
    }

    #[test]
    fn validate_rejects_an_inverted_realized_species_range() {
        assert!(matches!(
            FloraConfig::from_json_str(&one_species_json_with_range(5, 2)),
            Err(FloraConfigError::InvalidRealizedSpeciesRange { min: 5, max: 2 })
        ));
    }

    #[test]
    fn validate_rejects_a_realized_species_min_below_one() {
        assert!(matches!(
            FloraConfig::from_json_str(&one_species_json_with_range(0, 4)),
            Err(FloraConfigError::InvalidRealizedSpeciesRange { min: 0, max: 4 })
        ));
    }

    const VALID_BODY: &str = r#"{
        "display_name": "Probe",
        "plural": "probes",
        "adjective": "probe",
        "role": "staple",
        "cultivation_ceiling": "tended",
        "host_biomes": { "AlluvialPlain": 1.0 },
        "yield": { "provisions_per_biomass": 0.05, "fodder_per_biomass": 0.0 },
        "regrowth_rate": 0.25
    }"#;

    #[test]
    fn builtin_parses_and_validates() {
        let config = FloraConfig::builtin();
        assert_eq!(
            config.species.len(),
            33,
            "18 F1–F4 families (12 biome staples + river_fish + hay_grass + cotton/flax/tobacco/tea) \
             + the 15 F5 fine-grained fill (kelp, sea_kale, wild_rice, cattail, chestnut, \
             wild_orchard, sunflower, wild_pulses, mesquite, wild_fig, cloudberry, rock_tripe, \
             alpine_herbs, cave_fungi, grapevine)"
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

    /// **A crew cannot favor a crop it cannot clear the ground for.** The `VALID_BODY` probe is
    /// `tended`-ceiling, so marking it as standing outside the worked ground makes it a species the
    /// player could commit a `Cultivate` to and then never weed toward — the incoherent pair
    /// [`FloraConfig::validate`] exists to catch at load rather than at play.
    #[test]
    fn validate_rejects_a_favored_species_that_stands_outside_the_worked_ground() {
        let body = VALID_BODY.replace(
            "\"cultivation_ceiling\": \"tended\"",
            "\"cultivation_ceiling\": \"tended\", \"stands_in_worked_ground\": false",
        );
        assert!(matches!(
            FloraConfig::from_json_str(&one_species_json(&body)),
            Err(FloraConfigError::CultivableOutsideWorkedGround {
                ceiling: CultivationCeiling::Tended,
                ..
            })
        ));
    }

    /// The same row at `wild` is **accepted** — the shipped shape of all three protected species.
    /// Without this the rejection above would be indistinguishable from a blanket ban on the flag.
    #[test]
    fn a_wild_ceiling_species_may_stand_outside_the_worked_ground() {
        let body = VALID_BODY.replace(
            "\"cultivation_ceiling\": \"tended\"",
            "\"cultivation_ceiling\": \"wild\", \"stands_in_worked_ground\": false",
        );
        let config = FloraConfig::from_json_str(&one_species_json(&body))
            .expect("a gather-only species outside the worked ground is a coherent row");
        assert!(!config.stands_in_worked_ground("probe"));
    }

    /// **THE FISHERIES ARE MARKED AND THE CLEARABLE GATHERS ARE NOT** — the split the flag exists
    /// to make, asserted against the shipped roster rather than against a fixture, because the six
    /// `wild`-ceiling rows on the clearable side are exactly what a ceiling-based guard would have
    /// shielded by mistake.
    #[test]
    fn only_the_fisheries_stand_outside_the_worked_ground() {
        let config = FloraConfig::builtin();
        let outside: Vec<&str> = {
            let mut keys: Vec<&str> = config
                .species
                .iter()
                .filter(|(_, def)| !def.stands_in_worked_ground)
                .map(|(key, _)| key.as_str())
                .collect();
            keys.sort_unstable();
            keys
        };
        assert_eq!(outside, vec!["kelp", "river_fish", "shellfish_beds"]);
        // The six gather-only rows a crew genuinely *can* clear off ground it is tending. Each is
        // `wild`-ceiling, so gating the reweights on the ceiling would have protected all of them.
        for key in [
            "oak_mast",
            "pine_nut",
            "cloudberry",
            "mesquite",
            "rock_tripe",
            "arctic_greens",
        ] {
            assert_eq!(
                config.species[key].cultivation_ceiling,
                CultivationCeiling::Wild,
                "{key} is the gather-only case the ceiling and the flag disagree about"
            );
            assert!(
                config.stands_in_worked_ground(key),
                "{key} grows in soil — tending the ground genuinely clears it"
            );
        }
        // Samphire is a salt-marsh plant rooted in ground, unlike the mussels beside it.
        assert!(config.stands_in_worked_ground("sea_kale"));
    }

    /// A species the roster does not name reads as clearable — the pre-guard behaviour, so a
    /// synthetic fixture species weeds exactly as it always did.
    #[test]
    fn an_unknown_species_stands_in_the_worked_ground() {
        assert!(FloraConfig::builtin().stands_in_worked_ground("no_such_plant"));
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
