use std::{
    collections::{HashMap, HashSet},
    env, fs, io,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use bevy::{math::UVec2, prelude::*};
use serde::{Deserialize, Serialize};
use sim_runtime::{CorruptionLedger, CorruptionSubsystem, FloatRasterState};
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::{
    culture::CultureTensionRecord,
    orders::FactionId,
    scalar::{scalar_from_f32, Scalar},
    start_profile::StartProfileOverrides,
    FoodModule, FoodSiteKind,
};
use bitflags::bitflags;

/// Per-map hydrology levers, each overriding the active preset's `river_*` key (overrides > preset
/// > default). Every field is documented on `MapPreset`.
#[derive(Debug, Clone, Default)]
pub struct HydrologyOverrides {
    /// How wet the map reads: a multiplier on the channel-extraction threshold.
    pub river_density: Option<f32>,
    /// The noise gate, in hexes.
    pub min_length: Option<usize>,
    /// The depression fill's drainage gradient across flats.
    pub fill_epsilon: Option<f32>,
    /// Elevation tie-break amplitude on flats.
    pub flat_jitter: Option<f32>,
    /// Per-hex runoff floor.
    pub base_runoff: Option<f32>,
    /// How hard rainfall drives discharge.
    pub moisture_weight: Option<f32>,
    /// Discharge at which a corner becomes a channel (the network-extraction threshold).
    pub channel_min_discharge: Option<f32>,
    /// Discharge at which a river edge becomes `Major`.
    pub class_major_min_discharge: Option<f32>,
    /// Discharge at which a river becomes a `NavigableRiver` hex chain.
    pub class_navigable_min_discharge: Option<f32>,
    /// Kill switch for navigable rivers.
    pub navigable_enabled: Option<bool>,
    /// The shortest navigable hex chain that still reads as a river; below this it is demoted to the
    /// river's edge (`Major`) form.
    pub navigable_min_hexes: Option<usize>,
}

/// Configuration for map topology (wrapping behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MapTopology {
    /// Whether the map wraps horizontally (east-west edges connect).
    pub wrap_horizontal: bool,
    /// Whether the map wraps vertically (north-south edges connect). Reserved for future use.
    pub wrap_vertical: bool,
}

/// Latitude + elevation climate model levers. Tile temperature is
/// `latitude_base(y) − elevation_lapse(elev) + element_jitter(element)` (see `systems::climate_temperature`),
/// replacing the old `(x+y)%4` element checkerboard. Stored as `f32` because the climate math is
/// pure floating-point and only the final temperature is converted to `Scalar`.
#[derive(Debug, Clone, Copy)]
pub struct ClimateConfig {
    /// Temperature (°) at the equator (center row).
    pub equator_temp: f32,
    /// Temperature (°) at the poles (top/bottom rows).
    pub polar_temp: f32,
    /// How much colder (°) a full-height mountain is than sea level at the same latitude.
    pub elevation_lapse_span: f32,
    /// Multiplier applied to the element's `thermal_bias` to keep it a small local jitter (~±1.5°)
    /// rather than the temperature driver.
    pub element_jitter_scale: f32,
    /// **Climate band ladder cut points** (`docs/plan_climate_authority.md` §8.1) — the inclusive
    /// upper bound of each band, read through the single seam
    /// [`crate::climate::climate_band_for_temperature`]. Since this arc these decide **biome
    /// eligibility**, not merely a client label, so they are worldgen levers: moving one moves the
    /// map. See that module for why the ladder has four rungs rather than one polar cut point.
    ///
    /// Polar: at or below freezing. Ice, tundra, glacier — the cold ladder's core.
    pub polar_max_temp: f32,
    /// Boreal: the taiga/subarctic fringe. Cold enough for the cold biome ladder, above freezing.
    pub boreal_max_temp: f32,
    /// Temperate: Köppen's classic temperate/tropical boundary. Above it a tile is tropical.
    pub temperate_max_temp: f32,
}

/// Global configuration parameters for the headless simulation prototype.
#[derive(Resource, Debug, Clone)]
pub struct SimulationConfig {
    pub grid_size: UVec2,
    pub map_topology: MapTopology,
    pub map_preset_id: String,
    pub map_seed: u64,
    pub start_profile_id: String,
    pub start_profile_overrides: StartProfileOverrides,
    pub hydrology: HydrologyOverrides,
    pub ambient_temperature: Scalar,
    pub temperature_lerp: Scalar,
    /// Latitude + elevation climate model levers (see `ClimateConfig`).
    pub climate: ClimateConfig,
    pub population_growth_rate: Scalar,
    pub temperature_morale_penalty: Scalar,
    /// Dead-band (°) around `ambient_temperature` within which climate contributes **zero** morale
    /// drain — only the excess beyond this tolerance is penalized.
    pub temperature_morale_tolerance: Scalar,
    pub population_cluster_stride: u32,
    pub population_cap: u32,
    pub power_adjust_rate: Scalar,
    pub max_power_generation: Scalar,
    pub max_power_efficiency: Scalar,
    pub min_power_influence: f32,
    pub max_power_influence: f32,
    pub power_generation_adjust_rate: f32,
    pub power_demand_adjust_rate: f32,
    pub power_storage_stability_bonus: f32,
    pub power_line_capacity: Scalar,
    pub power_storage_efficiency: Scalar,
    pub power_storage_bleed: Scalar,
    pub power_instability_warn: Scalar,
    pub power_instability_critical: Scalar,

    pub migration_fragment_scaling: Scalar,
    pub migration_fidelity_floor: Scalar,
    pub corruption_military_penalty: Scalar,
    /// Host and **base port** of the server's port block. Slot 0 itself is
    /// **reserved and never bound** — it carried the retired bincode snapshot
    /// socket (#388) — so this field names where the block starts, not a
    /// listener: the bound sockets are `command_bind` (+1), `snapshot_flat_bind`
    /// (+2) and `log_bind` (+3). `port_alloc::allocate` reads the base from
    /// here.
    pub port_base_bind: SocketAddr,
    pub snapshot_flat_bind: SocketAddr,
    pub command_bind: SocketAddr,
    pub log_bind: SocketAddr,
    pub crisis_auto_seed: bool,
    /// Fog of war master switch — the SINGLE authority for it, and deliberately server-owned. It
    /// gates both the herd display list (`herd_snapshot_entries`) and the visibility raster, so the
    /// two cannot disagree; a client-local render flag could not do the first, because unseen herds
    /// are already dropped from the payload. Not part of `WorldSnapshot`, so it survives a rollback.
    pub fog_enabled: bool,
    /// How many turns of world events the [`CommandEventLog`] keeps (and therefore how far back the
    /// client's event dock can scroll without a resync). Long enough to answer "what happened while
    /// I was away", short enough to bound a full snapshot. Published as
    /// `CampaignSection.commandEventsRetentionTurns` so the client can say how much history exists.
    pub command_events_retention_turns: u64,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct MoistureRaster {
    pub width: u32,
    pub height: u32,
    pub values: Vec<f32>,
}

impl MoistureRaster {
    pub fn new(width: u32, height: u32, values: Vec<f32>) -> Self {
        Self {
            width,
            height,
            values,
        }
    }

    pub fn from_state(state: &FloatRasterState) -> Self {
        Self {
            width: state.width,
            height: state.height,
            values: state.samples.clone(),
        }
    }

    pub fn as_state(&self) -> FloatRasterState {
        FloatRasterState {
            width: self.width,
            height: self.height,
            samples: self.values.clone(),
        }
    }
}

pub const BUILTIN_SIMULATION_CONFIG: &str = include_str!("data/simulation_config.json");

impl Default for SimulationConfig {
    fn default() -> Self {
        SimulationConfig::builtin()
    }
}

impl SimulationConfig {
    pub fn builtin() -> Self {
        SimulationConfig::from_json_str(BUILTIN_SIMULATION_CONFIG)
            .expect("builtin simulation config should parse")
    }

    pub fn from_json_str(json: &str) -> Result<Self, SimulationConfigError> {
        let data: SimulationConfigData = serde_json::from_str(json)?;
        data.into_config()
    }

    pub fn from_file(path: &Path) -> Result<Self, SimulationConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| SimulationConfigError::ReadFailed {
                path: path.to_path_buf(),
                source,
            })?;
        let config = SimulationConfig::from_json_str(&contents)?;
        Ok(config)
    }
}

#[derive(Debug, Error)]
pub enum SimulationConfigError {
    #[error("failed to parse simulation config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid socket address for `{field}`: {source}")]
    InvalidSocket {
        field: &'static str,
        #[source]
        source: std::net::AddrParseError,
    },
    #[error("failed to read simulation config from {path:?}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "`command_events_retention_turns` must be at least 1: it is published as \
         `CampaignSection.commandEventsRetentionTurns`, where 0 is the FlatBuffers default and the \
         client reads it as \"the sim did not state a window\" — so a configured 0 would leave the \
         sim and the event dock silently running different windows"
    )]
    ZeroCommandEventsRetentionTurns,
}

impl ConfigLoadError for SimulationConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::ReadFailed { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

#[derive(Debug, Deserialize, Default)]
struct MapTopologyData {
    #[serde(default)]
    wrap_horizontal: bool,
    #[serde(default)]
    wrap_vertical: bool,
}

#[derive(Debug, Deserialize)]
struct SimulationConfigData {
    grid_size: GridSizeData,
    #[serde(default)]
    map_topology: MapTopologyData,
    #[serde(default = "default_map_preset_id")]
    map_preset_id: String,
    #[serde(default)]
    map_seed: u64,
    #[serde(default = "default_start_profile_id")]
    start_profile_id: String,
    #[serde(default)]
    hydrology: Option<HydrologyOverridesData>,
    ambient_temperature: f32,
    temperature_lerp: f32,
    #[serde(default)]
    climate: ClimateConfigData,
    population_growth_rate: f32,
    temperature_morale_penalty: f32,
    #[serde(default = "default_temperature_morale_tolerance")]
    temperature_morale_tolerance: f32,
    population_cluster_stride: u32,
    population_cap: u32,
    power_adjust_rate: f32,
    max_power_generation: f32,
    max_power_efficiency: f32,
    min_power_influence: f32,
    max_power_influence: f32,
    power_generation_adjust_rate: f32,
    power_demand_adjust_rate: f32,
    power_storage_stability_bonus: f32,
    power_line_capacity: f32,
    power_storage_efficiency: f32,
    power_storage_bleed: f32,
    power_instability_warn: f32,
    power_instability_critical: f32,
    migration_fragment_scaling: f32,
    migration_fidelity_floor: f32,
    corruption_military_penalty: f32,
    port_base_bind: String,
    snapshot_flat_bind: String,
    command_bind: String,
    log_bind: String,
    #[serde(default)]
    crisis_auto_seed: bool,
    /// Defaulted through a function rather than `#[serde(default)]`, which would yield `false` for a
    /// bool — fog of war is ON unless a config or the `set_fog` command says otherwise.
    #[serde(default = "default_fog_enabled")]
    fog_enabled: bool,
    #[serde(default = "default_command_events_retention_turns")]
    command_events_retention_turns: u64,
}

#[derive(Debug, Deserialize)]
struct GridSizeData {
    x: u32,
    y: u32,
}

#[derive(Debug, Deserialize)]
struct ClimateConfigData {
    #[serde(default = "default_equator_temp")]
    equator_temp: f32,
    #[serde(default = "default_polar_temp")]
    polar_temp: f32,
    #[serde(default = "default_elevation_lapse_span")]
    elevation_lapse_span: f32,
    #[serde(default = "default_element_jitter_scale")]
    element_jitter_scale: f32,
    #[serde(default = "default_polar_max_temp")]
    polar_max_temp: f32,
    #[serde(default = "default_boreal_max_temp")]
    boreal_max_temp: f32,
    #[serde(default = "default_temperate_max_temp")]
    temperate_max_temp: f32,
}

impl Default for ClimateConfigData {
    fn default() -> Self {
        Self {
            equator_temp: default_equator_temp(),
            polar_temp: default_polar_temp(),
            elevation_lapse_span: default_elevation_lapse_span(),
            element_jitter_scale: default_element_jitter_scale(),
            polar_max_temp: default_polar_max_temp(),
            boreal_max_temp: default_boreal_max_temp(),
            temperate_max_temp: default_temperate_max_temp(),
        }
    }
}

impl ClimateConfigData {
    fn into_config(self) -> ClimateConfig {
        ClimateConfig {
            equator_temp: self.equator_temp,
            polar_temp: self.polar_temp,
            elevation_lapse_span: self.elevation_lapse_span,
            element_jitter_scale: self.element_jitter_scale,
            polar_max_temp: self.polar_max_temp,
            boreal_max_temp: self.boreal_max_temp,
            temperate_max_temp: self.temperate_max_temp,
        }
    }
}

fn default_equator_temp() -> f32 {
    30.0
}

fn default_polar_temp() -> f32 {
    -5.0
}

fn default_elevation_lapse_span() -> f32 {
    12.0
}

fn default_element_jitter_scale() -> f32 {
    0.25
}

/// Freezing. A tile at or below 0° carries the polar ladder wherever it sits — which is what makes
/// alpine tundra expressible (`docs/plan_climate_authority.md` §5.3).
fn default_polar_max_temp() -> f32 {
    0.0
}

/// The taiga/subarctic fringe — the top of the **cold ladder**, so this is the boundary that
/// decides whether a tile may carry a `POLAR`-tagged biome at all.
///
/// Boreal exists as its own rung because `BorealTaiga` was 1,601 of the 4,397 measured warm-polar
/// tiles — a boreal-band failure a single polar cut point cannot express (§8.1).
///
/// **The value is 3.0 because that is the client's retired `cool_min`** — the temperature below
/// which a tile card already read as cold
/// (`clients/godot_thin_client/src/config/tile_climate_config.json`). §5.2 requires the sim's biome
/// gate and the client's `Climate:` band to be the *same* boundary, or the tile card can still show
/// a biome and a climate that disagree — the exact defect this arc removes. Ceding the number to
/// the client's existing one means the sim now owns it (§8.3) without silently relabelling every
/// tile the player has already learned to read.
fn default_boreal_max_temp() -> f32 {
    3.0
}

/// Köppen's classic temperate/tropical boundary (coldest-month 18°).
fn default_temperate_max_temp() -> f32 {
    18.0
}

fn default_temperature_morale_tolerance() -> f32 {
    9.0
}

#[derive(Debug, Deserialize, Default)]
struct HydrologyOverridesData {
    river_density: Option<f32>,
    min_length: Option<usize>,
    river_fill_epsilon: Option<f32>,
    river_flat_jitter: Option<f32>,
    river_base_runoff: Option<f32>,
    river_moisture_weight: Option<f32>,
    river_channel_min_discharge: Option<f32>,
    river_class_major_min_discharge: Option<f32>,
    river_class_navigable_min_discharge: Option<f32>,
    river_navigable_enabled: Option<bool>,
    navigable_min_hexes: Option<usize>,
}

impl HydrologyOverridesData {
    fn into_overrides(self) -> HydrologyOverrides {
        HydrologyOverrides {
            river_density: self.river_density,
            min_length: self.min_length,
            fill_epsilon: self.river_fill_epsilon,
            flat_jitter: self.river_flat_jitter,
            base_runoff: self.river_base_runoff,
            moisture_weight: self.river_moisture_weight,
            channel_min_discharge: self.river_channel_min_discharge,
            class_major_min_discharge: self.river_class_major_min_discharge,
            class_navigable_min_discharge: self.river_class_navigable_min_discharge,
            navigable_enabled: self.river_navigable_enabled,
            navigable_min_hexes: self.navigable_min_hexes,
        }
    }
}

impl SimulationConfigData {
    fn into_config(self) -> Result<SimulationConfig, SimulationConfigError> {
        // A parsed-but-incoherent config counts as broken (see `config-loading.md`): boot panics
        // through the `config_load.rs` seam, hot reload warns and keeps the live config. Both
        // enter here, which is why the check lives at this seam and not at either call site.
        if self.command_events_retention_turns == 0 {
            return Err(SimulationConfigError::ZeroCommandEventsRetentionTurns);
        }
        Ok(SimulationConfig {
            grid_size: UVec2::new(self.grid_size.x, self.grid_size.y),
            map_topology: MapTopology {
                wrap_horizontal: self.map_topology.wrap_horizontal,
                wrap_vertical: self.map_topology.wrap_vertical,
            },
            map_preset_id: self.map_preset_id,
            map_seed: self.map_seed,
            start_profile_id: self.start_profile_id,
            start_profile_overrides: StartProfileOverrides::default(),
            hydrology: self
                .hydrology
                .map(|d| d.into_overrides())
                .unwrap_or_default(),
            ambient_temperature: scalar_from_f32(self.ambient_temperature),
            temperature_lerp: scalar_from_f32(self.temperature_lerp),
            climate: self.climate.into_config(),
            population_growth_rate: scalar_from_f32(self.population_growth_rate),
            temperature_morale_penalty: scalar_from_f32(self.temperature_morale_penalty),
            temperature_morale_tolerance: scalar_from_f32(self.temperature_morale_tolerance),
            population_cluster_stride: self.population_cluster_stride,
            population_cap: self.population_cap,
            power_adjust_rate: scalar_from_f32(self.power_adjust_rate),
            max_power_generation: scalar_from_f32(self.max_power_generation),
            max_power_efficiency: scalar_from_f32(self.max_power_efficiency),
            min_power_influence: self.min_power_influence,
            max_power_influence: self.max_power_influence,
            power_generation_adjust_rate: self.power_generation_adjust_rate,
            power_demand_adjust_rate: self.power_demand_adjust_rate,
            power_storage_stability_bonus: self.power_storage_stability_bonus,
            power_line_capacity: scalar_from_f32(self.power_line_capacity),
            power_storage_efficiency: scalar_from_f32(self.power_storage_efficiency),
            power_storage_bleed: scalar_from_f32(self.power_storage_bleed),
            power_instability_warn: scalar_from_f32(self.power_instability_warn),
            power_instability_critical: scalar_from_f32(self.power_instability_critical),
            migration_fragment_scaling: scalar_from_f32(self.migration_fragment_scaling),
            migration_fidelity_floor: scalar_from_f32(self.migration_fidelity_floor),
            corruption_military_penalty: scalar_from_f32(self.corruption_military_penalty),
            port_base_bind: parse_socket(self.port_base_bind, "port_base_bind")?,
            snapshot_flat_bind: parse_socket(self.snapshot_flat_bind, "snapshot_flat_bind")?,
            command_bind: parse_socket(self.command_bind, "command_bind")?,
            log_bind: parse_socket(self.log_bind, "log_bind")?,
            crisis_auto_seed: self.crisis_auto_seed,
            fog_enabled: self.fog_enabled,
            command_events_retention_turns: self.command_events_retention_turns,
        })
    }
}

fn default_fog_enabled() -> bool {
    true
}

/// 20 turns of world events: long enough that a player returning from a few quick turns can read
/// what happened, short enough that a full snapshot's event list stays small. The single source of
/// the number — [`CommandEventLog::default`] reads it too, so an untouched config and an absent key
/// cannot disagree.
fn default_command_events_retention_turns() -> u64 {
    20
}

fn default_map_preset_id() -> String {
    "earthlike".to_string()
}

fn default_start_profile_id() -> String {
    "late_forager_tribe".to_string()
}

fn parse_socket(value: String, field: &'static str) -> Result<SocketAddr, SimulationConfigError> {
    SocketAddr::from_str(&value)
        .map_err(|source| SimulationConfigError::InvalidSocket { field, source })
}

#[derive(Resource, Debug, Clone)]
pub struct SimulationConfigMetadata {
    path: Option<PathBuf>,
    seed_random: bool,
}

impl SimulationConfigMetadata {
    pub fn new(path: Option<PathBuf>, seed_random: bool) -> Self {
        Self { path, seed_random }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }

    pub fn seed_random(&self) -> bool {
        self.seed_random
    }

    pub fn set_seed_random(&mut self, value: bool) {
        self.seed_random = value;
    }
}

/// Port offsets from the `SIM_PORT_BASE` base for each listen socket, preserving
/// the historical 41000-based layout (base = 41000 reproduces today's ports).
/// **Slot 0 has no constant because nothing binds it** — it held the retired
/// bincode snapshot socket (#388) and stays reserved so these three keep the
/// numbers every client default and `ports.json` already names.
pub const COMMAND_PORT_OFFSET: u16 = 1;
pub const SNAPSHOT_FLAT_PORT_OFFSET: u16 = 2;
pub const LOG_PORT_OFFSET: u16 = 3;

/// Lowest accepted `SIM_PORT_BASE`. Slot 0 is never bound, so a base of 0 no
/// longer means an ephemeral bind — it is rejected because the base is *stored*
/// in `port_base_bind`'s port and re-read from there (`bin/server.rs`), and 0 is
/// `SocketAddr`'s "let the OS choose" wildcard, indistinguishable from a base
/// nobody configured. The block it would name (bound ports 1-3) is privileged
/// anyway. `scripts/run_stack.sh` applies the same floor.
const MIN_PORT_BASE: u16 = 1;

/// Overrides each bind's port with `base + <offset>`, preserving the host.
/// Returns false (and leaves `config` unchanged) if `base` is below
/// `MIN_PORT_BASE` (0 is the wildcard-port sentinel, not a block) or
/// `base + LOG_PORT_OFFSET` would overflow u16.
pub fn apply_port_base(config: &mut SimulationConfig, base: u16) -> bool {
    if base < MIN_PORT_BASE || base.checked_add(LOG_PORT_OFFSET).is_none() {
        return false;
    }
    config.port_base_bind.set_port(base);
    config.command_bind.set_port(base + COMMAND_PORT_OFFSET);
    config
        .snapshot_flat_bind
        .set_port(base + SNAPSHOT_FLAT_PORT_OFFSET);
    config.log_bind.set_port(base + LOG_PORT_OFFSET);
    true
}

/// Reads and validates the optional `SIM_PORT_BASE` env override. Returns
/// `None` (with a warning) when unset, unparseable, or out of range, so a
/// stray value can't take the server down. A `Some` result also means the
/// operator chose the base *explicitly*, which suppresses port auto-bumping
/// in `port_alloc::allocate`.
pub fn port_base_override() -> Option<u16> {
    let raw = env::var("SIM_PORT_BASE").ok()?;
    match raw.trim().parse::<u16>() {
        Ok(base) if base >= MIN_PORT_BASE && base.checked_add(LOG_PORT_OFFSET).is_some() => {
            Some(base)
        }
        Ok(base) => {
            tracing::warn!(target: "shadow_scale::config", base, "sim_port_base.out_of_range=ignored");
            None
        }
        Err(_) => {
            tracing::warn!(target: "shadow_scale::config", value = %raw, "sim_port_base.invalid=ignored");
            None
        }
    }
}

/// Applies the optional `SIM_PORT_BASE` env override to `config`'s four binds.
/// Leaves `config` unchanged when the override is absent or invalid.
pub fn apply_port_base_override(config: &mut SimulationConfig) {
    let Some(base) = port_base_override() else {
        return;
    };
    if apply_port_base(config, base) {
        tracing::info!(
            target: "shadow_scale::config",
            base,
            command = config.command_bind.port(),
            snapshot_flat = config.snapshot_flat_bind.port(),
            log = config.log_bind.port(),
            "sim_port_base.applied"
        );
    } else {
        tracing::warn!(target: "shadow_scale::config", base, "sim_port_base.out_of_range=ignored");
    }
}

/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `SIM_CONFIG_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_simulation_config_from_env() -> (SimulationConfig, SimulationConfigMetadata) {
    let (loaded, source) = load_config_from_env(
        "SIM_CONFIG_PATH",
        "simulation_config",
        "src/data/simulation_config.json",
        || Arc::new(SimulationConfig::builtin()),
        SimulationConfig::from_file,
    );
    // The helper hands back an `Arc` so it can stay generic; this is the sole reference, so the
    // unwrap is a move, not a copy.
    let mut config = Arc::try_unwrap(loaded).unwrap_or_else(|shared| (*shared).clone());
    apply_port_base_override(&mut config);
    let random_seed = config.map_seed == 0;
    (config, SimulationConfigMetadata::new(source, random_seed))
}

/// The [`SimulationConfig`] a **freshly built world** starts from: whatever would load right now,
/// with the fields the *running process* owns carried over from `outgoing` (the config of the world
/// being replaced).
///
/// Loading afresh is the point. A staged tuning override lives in the load registry
/// (`crate::config_override`), and the only mechanism by which it ever reaches a world is that a New
/// Game re-reads every config — so a rebuild that clones the outgoing world's `SimulationConfig`
/// instead makes every `simulation` lever the client's tuning panel exposes inert.
///
/// Carrying a field is the other half, and the rule is narrow: **the file is the authority at world
/// start; only what the file CANNOT know gets carried.** Anything the file *could* have said is a
/// tunable, and a tunable that is carried is permanently un-overridable — staging it on the tuning
/// panel would install, log, and do nothing, which is precisely the bug this function was written to
/// fix. So the carried set is a consequence of that principle, not a list of conveniences.
///
/// | Carried field | Why the file cannot know it |
/// |---|---|
/// | `fog_enabled` | Not a tunable at all: a **player preference** with its own persisted home in the client (`.claude/rules/client/fog-of-war.md`), pushed to the server as a `set_fog` command. It would never appear on the tuning panel, and resetting it every New Game would be a visible regression. |
/// | the four bind addresses | `crate::port_alloc::allocate` at boot — port allocation auto-bumps on a collision, so after a bump these are *not* what the file says and a fresh load could never reproduce them. The in-world config must describe the ports the process actually holds. |
///
/// **`crisis_auto_seed` is deliberately NOT carried**, though `set_crisis_auto_seed` writes it at
/// runtime: it lives in `simulation_config.json` alongside exactly the levers the tuning panel
/// exists to change, so the file is its authority at world start. The cost is re-issuing one debug
/// command after a New Game; the command still works within a world.
///
/// `start_profile_id` / `start_profile_overrides` are runtime-owned too, but deliberately **not**
/// carried: the new-game path re-applies the profile its command names (`apply_start_profile`) after
/// this config is installed, so a carried value would be dead. The caller likewise supplies
/// `grid_size` / `map_preset_id` / `map_seed`, which are arguments of the rebuild command.
pub fn load_simulation_config_for_new_world(outgoing: &SimulationConfig) -> SimulationConfig {
    let (mut config, _metadata) = load_simulation_config_from_env();

    config.fog_enabled = outgoing.fog_enabled;
    config.port_base_bind = outgoing.port_base_bind;
    config.snapshot_flat_bind = outgoing.snapshot_flat_bind;
    config.command_bind = outgoing.command_bind;
    config.log_bind = outgoing.log_bind;

    config
}

/// Tracks total simulation ticks elapsed.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationTick(pub u64);

/// Monotonic world-build counter, identical for every snapshot within one world and incremented on
/// every world (re)build (`new_game` / `ResetMap`). It lives OUTSIDE the app (the server `main`
/// loop, like `world_active`), because each rebuild constructs a brand-new [`bevy::prelude::App`];
/// the server inserts the current value into each fresh app so `capture_snapshot` can stamp it onto
/// the snapshot header. A client compares it to tell a freshly-generated world from a stale one the
/// snapshot server replays to reconnecting subscribers. The idle boot app carries `0`.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldEpoch(pub u32);

/// Hands out [`crate::components::BandId`]s, and is **checkpoint state in its own right**.
///
/// A monotonic counter is only an identity source if it is restored with the world it numbered.
/// Restore the bands but not the counter and the next band spawned after a rollback re-issues an
/// id that a living band already holds, which is worse than the entity churn this replaces —
/// duplicate ids alias silently rather than failing to resolve.
///
/// Starts at 1 so `BandId(0)` is available as an unmistakable "unset".
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BandIdAllocator {
    next: u64,
}

impl Default for BandIdAllocator {
    fn default() -> Self {
        Self {
            next: FIRST_BAND_ID,
        }
    }
}

/// First id handed out; `BandId(0)` is reserved for "unset".
const FIRST_BAND_ID: u64 = 1;

impl BandIdAllocator {
    /// The next unused id.
    pub fn allocate(&mut self) -> crate::components::BandId {
        let id = crate::components::BandId(self.next);
        self.next = self.next.saturating_add(1);
        id
    }

    /// The counter's current position, for the checkpoint.
    pub fn peek(&self) -> u64 {
        self.next
    }

    /// Restore the counter, refusing to move it backwards.
    ///
    /// A checkpoint is the authority on where the counter was, but a rollback must never lower it
    /// below an id already alive in this process — that is the aliasing case above, and clamping is
    /// cheaper than trusting every caller to restore in the right order.
    pub fn restore(&mut self, next: u64) {
        self.next = self.next.max(next).max(FIRST_BAND_ID);
    }
}

bitflags! {
    #[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct CapabilityFlags: u32 {
        const ALWAYS_ON = 1 << 0;
        const CONSTRUCTION = 1 << 1;
        const INDUSTRY_T1 = 1 << 2;
        const INDUSTRY_T2 = 1 << 3;
        const POWER = 1 << 4;
        const NAVAL_OPS = 1 << 5;
        const AIR_OPS = 1 << 6;
        const ESPIONAGE_T2 = 1 << 7;
        const MEGAPROJECTS = 1 << 8;
    }
}

impl Default for CapabilityFlags {
    fn default() -> Self {
        CapabilityFlags::ALWAYS_ON
    }
}

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct StartLocation {
    position: Option<UVec2>,
}

impl StartLocation {
    pub fn new(position: Option<UVec2>) -> Self {
        Self { position }
    }

    pub fn position(&self) -> Option<UVec2> {
        self.position
    }

    pub fn relocate(&mut self, position: UVec2) {
        self.position = Some(position);
    }
}

/// Authoritative sentiment axis bias values applied across factions.
///
/// Sentiment is composed of three categories of forces:
/// - **Policy levers** (`policy`): long-lived adjustments driven by enacted reforms or manual tweaks.
/// - **Incident deltas** (`incidents`): short-lived shocks produced by exposed scandals, crises, etc.
/// - **Influencer output** (`influencer`): procedurally generated contributions from the influencer roster.
#[derive(Resource, Debug, Clone)]
pub struct SentimentAxisBias {
    policy: [Scalar; 4],
    incidents: [Scalar; 4],
    influencer: [Scalar; 4],
}

impl Default for SentimentAxisBias {
    fn default() -> Self {
        Self {
            policy: [Scalar::zero(); 4],
            incidents: [Scalar::zero(); 4],
            influencer: [Scalar::zero(); 4],
        }
    }
}

impl SentimentAxisBias {
    pub fn set_policy_axis(&mut self, axis: usize, value: Scalar) {
        if let Some(slot) = self.policy.get_mut(axis) {
            *slot = value;
        }
    }

    pub fn set_policy_axes(&mut self, values: [Scalar; 4]) {
        self.policy = values;
    }

    pub fn policy_values(&self) -> [Scalar; 4] {
        self.policy
    }

    pub fn set_influencer(&mut self, deltas: [Scalar; 4]) {
        self.influencer = deltas;
    }

    pub fn influencer_values(&self) -> [Scalar; 4] {
        self.influencer
    }

    pub fn incident_values(&self) -> [Scalar; 4] {
        self.incidents
    }

    pub fn apply_incident_delta(&mut self, axis: usize, delta: Scalar) {
        if let Some(slot) = self.incidents.get_mut(axis) {
            *slot = (*slot + delta).clamp(Scalar::from_f32(-2.0), Scalar::from_f32(2.0));
        }
    }

    pub fn reset_incidents(&mut self) {
        self.incidents = [Scalar::zero(); 4];
    }

    pub fn manual_environment(&self) -> [Scalar; 4] {
        let mut result = self.policy;
        for (idx, incident) in self.incidents.iter().enumerate() {
            result[idx] += *incident;
        }
        result
    }

    pub fn combined(&self) -> [Scalar; 4] {
        let mut result = self.manual_environment();
        for (idx, delta) in self.influencer.iter().enumerate() {
            result[idx] += *delta;
        }
        result
    }

    pub fn as_raw(&self) -> [i64; 4] {
        self.combined().map(Scalar::raw)
    }

    pub fn reset_to_state(&mut self, policy: [Scalar; 4], incidents: [Scalar; 4]) {
        self.policy = policy;
        self.incidents = incidents;
        self.influencer = [Scalar::zero(); 4];
    }
}

/// Index of tile entities for reuse by other systems.
#[derive(Resource, Debug, Clone)]
pub struct TileRegistry {
    pub tiles: Vec<Entity>,
    pub width: u32,
    pub height: u32,
}

impl TileRegistry {
    pub fn index(&self, x: u32, y: u32) -> Option<Entity> {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            self.tiles.get(idx).cloned()
        } else {
            None
        }
    }
}

/// Tracks corruption intensity across subsystems for snapshot export.
#[derive(Resource, Debug, Clone, Default)]
pub struct CorruptionLedgers {
    ledger: CorruptionLedger,
}

impl CorruptionLedgers {
    pub fn ledger(&self) -> &CorruptionLedger {
        &self.ledger
    }

    pub fn ledger_mut(&mut self) -> &mut CorruptionLedger {
        &mut self.ledger
    }

    pub fn total_intensity(&self, subsystem: CorruptionSubsystem) -> i64 {
        self.ledger
            .entries
            .iter()
            .filter(|entry| entry.subsystem == subsystem)
            .map(|entry| entry.intensity.max(0))
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorruptionExposureRecord {
    pub incident_id: u64,
    pub subsystem: CorruptionSubsystem,
    pub intensity: i64,
    pub trust_delta: i64,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct CorruptionTelemetry {
    pub active_incidents: usize,
    pub exposures_this_turn: Vec<CorruptionExposureRecord>,
    pub exposures_total: u64,
}

impl CorruptionTelemetry {
    pub fn reset_turn(&mut self) {
        self.exposures_this_turn.clear();
    }

    pub fn record_exposure(&mut self, record: CorruptionExposureRecord) {
        self.exposures_this_turn.push(record);
        self.exposures_total += 1;
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct DiplomacyLeverage {
    pub recent: Vec<CorruptionExposureRecord>,
    pub max_entries: usize,
    pub culture_signals: Vec<CultureTensionRecord>,
    pub great_discoveries: Vec<(FactionId, u16)>,
}

impl DiplomacyLeverage {
    pub fn push(&mut self, record: CorruptionExposureRecord) {
        if self.max_entries == 0 {
            self.max_entries = 16;
        }
        self.recent.push(record);
        if self.recent.len() > self.max_entries {
            let overflow = self.recent.len() - self.max_entries;
            self.recent.drain(0..overflow);
        }
    }

    pub fn push_culture_signal(&mut self, record: CultureTensionRecord) {
        if self.max_entries == 0 {
            self.max_entries = 16;
        }
        self.culture_signals.push(record);
        if self.culture_signals.len() > self.max_entries {
            let overflow = self.culture_signals.len() - self.max_entries;
            self.culture_signals.drain(0..overflow);
        }
    }

    pub fn push_great_discovery(&mut self, faction: FactionId, discovery_id: u16) {
        if self.max_entries == 0 {
            self.max_entries = 16;
        }
        self.great_discoveries.push((faction, discovery_id));
        if self.great_discoveries.len() > self.max_entries {
            let overflow = self.great_discoveries.len() - self.max_entries;
            self.great_discoveries.drain(0..overflow);
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct PendingCrisisSeeds {
    pub seeds: Vec<(FactionId, u16)>,
}

impl PendingCrisisSeeds {
    pub fn push(&mut self, faction: FactionId, discovery_id: u16) {
        self.seeds.push((faction, discovery_id));
    }

    pub fn drain(&mut self) -> Vec<(FactionId, u16)> {
        std::mem::take(&mut self.seeds)
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct PendingCrisisSpawns {
    pub spawns: Vec<(FactionId, String)>,
}

impl PendingCrisisSpawns {
    pub fn push<S: Into<String>>(&mut self, faction: FactionId, archetype_id: S) {
        self.spawns.push((faction, archetype_id.into()));
    }

    pub fn drain(&mut self) -> Vec<(FactionId, String)> {
        std::mem::take(&mut self.spawns)
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct DiscoveryProgressLedger {
    pub progress: HashMap<FactionId, HashMap<u32, Scalar>>,
}

impl DiscoveryProgressLedger {
    pub fn add_progress(&mut self, faction: FactionId, discovery_id: u32, delta: Scalar) -> Scalar {
        let faction_entry = self.progress.entry(faction).or_default();
        let entry = faction_entry
            .entry(discovery_id)
            .or_insert_with(Scalar::zero);
        *entry = (*entry + delta).clamp(Scalar::zero(), Scalar::one());
        *entry
    }

    pub fn get_progress(&self, faction: FactionId, discovery_id: u32) -> Scalar {
        self.progress
            .get(&faction)
            .and_then(|map| map.get(&discovery_id))
            .copied()
            .unwrap_or_else(Scalar::zero)
    }
}

#[derive(Debug, Clone)]
pub struct TradeDiffusionRecord {
    pub tick: u64,
    pub from: FactionId,
    pub to: FactionId,
    pub discovery_id: u32,
    pub delta: Scalar,
    pub via_migration: bool,
    pub herd_density: f32,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct TradeTelemetry {
    pub tech_diffusion_applied: u32,
    pub migration_transfers: u32,
    pub records: Vec<TradeDiffusionRecord>,
}

impl TradeTelemetry {
    pub fn reset_turn(&mut self) {
        self.tech_diffusion_applied = 0;
        self.migration_transfers = 0;
        self.records.clear();
    }

    pub fn push_record(&mut self, record: TradeDiffusionRecord) {
        self.records.push(record);
    }
}

/// Per-faction stockpile of abstracted inventory items granted by start profiles.
///
/// **Start-profile only.** Nothing in the turn loop credits or reads it: `seed_starting_inventory`
/// writes `StartProfileOverrides::inventory` here at worldgen and the Startup-only
/// `apply_trade_goods_bonus` drains the `TRADE_GOODS` grant into the opening trade-link openness
/// bonus. Ongoing production banks into the producing band's [`crate::LocalStore`] instead.
#[derive(Resource, Debug, Clone, Default)]
pub struct FactionInventory {
    stockpiles: HashMap<FactionId, HashMap<String, i64>>,
}

#[derive(Clone, Debug)]
pub struct FoodSiteEntry {
    pub position: UVec2,
    pub module: FoodModule,
    pub kind: FoodSiteKind,
    pub seasonal_weight: f32,
}

/// **THE GATHERING SITES — the ground a people can actually work.**
///
/// Curated once during worldgen against a latitude-band + spatial-bucket quota with a minimum
/// spacing, sized as a **share of land** and biased toward fresh water (`spawn_initial_world`,
/// #466) — 130–134 markers on a standard map — and thereafter only *reconciled* against repainted
/// terrain, never re-curated. So the set is fixed for the life of a world.
///
/// **This is the plant branch's scarcity, and since the ladder's site rule it is a live gameplay
/// rule rather than a map decoration** (`RungSiteRequirement::requires_gathering_site`): rungs 1–3
/// may only stand on a site, so *which* site a band can reach is the early game's real decision.
/// Do not confuse it with `FoodModuleTag`, which sits on ~every land tile and says only which food
/// web the ground belongs to.
#[derive(Resource, Debug, Clone, Default)]
pub struct FoodSiteRegistry {
    sites: Vec<FoodSiteEntry>,
    /// The positions of `sites`, for the per-command `is_site` test. Rebuilt with the vec by the two
    /// writers below, so it cannot drift out of step with the list it indexes.
    positions: HashSet<UVec2>,
}

/// **What the fresh-water bias pass actually did to the curated marker list** (issue #466).
///
/// The pass is a relocation, so its effect is invisible in any single map: "the markers are where
/// they are" is true whether the pass ran, was switched off, or silently stopped working. This
/// resource is the pass's own report, which is what lets a test assert the *kill switch* rather than
/// mere reproducibility — at `fresh_water_site_weight = 0.0` the claim is `moved == 0`, and there is
/// no "build the world without the pass" arm available to compare against instead.
///
/// Every field is written on **every** run of the pass, including the zero-weight early return: a
/// stale count left over from a previous build would defeat the assertion it exists to support.
#[derive(Resource, Debug, Clone, Default)]
pub struct FoodSiteWaterBiasReport {
    /// Markers relocated to a higher-scoring hex in their own bucket.
    pub moved: usize,
    /// Relocated markers whose destination classified to a different food module than their origin,
    /// so the entry's `module`/`kind` were re-authored by the terrain.
    pub relabelled: usize,
    /// Markers sitting on or beside fresh water **after** the pass — the outcome the bias is for.
    pub watered: usize,
    /// Markers in the registry. Constant across the pass by construction (it never adds or drops).
    pub total: usize,
}

impl FoodSiteRegistry {
    pub fn new(entries: Vec<FoodSiteEntry>) -> Self {
        let positions = entries.iter().map(|entry| entry.position).collect();
        Self {
            sites: entries,
            positions,
        }
    }

    pub fn set_sites(&mut self, entries: Vec<FoodSiteEntry>) {
        self.positions = entries.iter().map(|entry| entry.position).collect();
        self.sites = entries;
    }

    /// **Is this tile a gathering site?** The one test the plant ladder's site rule asks of the map.
    pub fn is_site(&self, position: UVec2) -> bool {
        self.positions.contains(&position)
    }

    pub fn sites(&self) -> &[FoodSiteEntry] {
        &self.sites
    }

    pub fn iter(&self) -> impl Iterator<Item = &FoodSiteEntry> {
        self.sites.iter()
    }
}

impl FactionInventory {
    pub fn add_stockpile<S: Into<String>>(&mut self, faction: FactionId, item: S, quantity: i64) {
        if quantity == 0 {
            return;
        }
        let entry = self.stockpiles.entry(faction).or_default();
        *entry.entry(item.into()).or_insert(0) += quantity;
    }

    pub fn take_stockpile(&mut self, faction: FactionId, item: &str, quantity: i64) -> i64 {
        if quantity <= 0 {
            return 0;
        }
        let Some(entry) = self.stockpiles.get_mut(&faction) else {
            return 0;
        };
        let (removable, cleanup_faction) = {
            let Some(slot) = entry.get_mut(item) else {
                return 0;
            };
            let removable = (*slot).min(quantity);
            *slot -= removable;
            if *slot == 0 {
                entry.remove(item);
            }
            (removable, entry.is_empty())
        };
        if cleanup_faction {
            self.stockpiles.remove(&faction);
        }
        removable
    }

    pub fn stockpile(&self, faction: FactionId) -> Option<&HashMap<String, i64>> {
        self.stockpiles.get(&faction)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&FactionId, &HashMap<String, i64>)> {
        self.stockpiles.iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandEventKind {
    Scout,
    FollowHerd,
    FoundSettlement,
    CampaignFounded,
    CampaignMilestone,
    CampaignVictory,
    Forage,
    Hunt,
    /// The animal rung-2 verb (`tame`) — the whole taming life: the command, and the meter
    /// completing. Replaces the retired `Domesticate` early-claim's kind.
    Tame,
    Cultivate,
    /// The plant rung-3 verb (`sow`) — the whole life of a Field: the command, and the build meter
    /// completing. Its own kind rather than `Cultivate`'s, because a Field and a tended patch are
    /// different rungs the player chooses between (the animal side's `Tame` set the precedent).
    Sow,
    Corral,
    /// **The crafting bench** (`docs/plan_crafting_and_materials.md` §5) — a recipe put on it, taken
    /// off it, or re-crewed. One kind for the bench's whole life, the way [`Self::Corral`] is one
    /// kind for the pen's: the player is looking at one bench, not at three verbs.
    ///
    /// The wire field is already a string, so the feed renders it generically — no schema or client
    /// change (see [`Self::NarrativeBeat`]).
    Craft,
    /// A **dangerous hunt** produced band casualties (Predators Phase 0, `docs/plan_predators.md`). The
    /// hunt-danger combat resolution pushes this whenever hunting an animal that fights back
    /// (`attack × ferocity > 0` — mammoth, ox) costs the party casualties (killed and/or wounded; the
    /// event fires on `killed + wounded > 0`, so a wound-only hunt narrates too). The hunting party
    /// answers the danger with its own hunters — **Warriors do NOT mitigate a hunt**.
    HuntDanger,
    /// **A hunt happened, and these are its facts** (`docs/plan_hunt_through_combat.md` §6.6) —
    /// animals engaged, how many fled before contact, animals killed, hunters lost or wounded,
    /// **which bound ran out first** (engagement / the floor / carry / the fight), and what came home
    /// (carried) against what was left to rot (wasted).
    ///
    /// **Facts, never a composed judgement.** Issue #272's notification system owns importance and
    /// phrasing; the hunt owns what happened. Every number rides the `key=value` detail so a consumer
    /// reads the resolution rather than this arc's guess at an importance ladder — which is also why
    /// the label names only the species and the count, and asserts nothing about whether the turn
    /// went well.
    ///
    /// **It is the visibility half of §11's first open question**: for most species the escapement
    /// floor binds long before engagement does, so an `engage_rate` authored too low silently becomes
    /// a *second* floor. `bound=engagement` is what makes that legible instead of mysterious.
    ///
    /// Gated on the hunt having *happened* (`engaged > 0`) and on nothing else — a fact gate, not an
    /// importance one. It is deliberately **not** the wounded-only twin of [`Self::HuntDanger`]: that
    /// line stays gated on a death, and this carries `hunters_wounded` on every hunt instead.
    HuntReport,
    /// A **carnivore raided a band's camp** (Predators Phase 1b, `docs/plan_predators.md`). A
    /// carnivore with `aggression > 0` within `predators.raid_radius` of a band turns on it, and the
    /// band is defended by its **Warriors** — the Warrior role's **first live consumer**. Fires each
    /// casualty-causing raid turn (edge-gating a repeated raid to one line is deferred to Phase 3).
    PredatorRaid,
    CancelOrder,
    SedentarizationPrompt,
    SiteDiscovered,
    ExpeditionSent,
    ExpeditionArrived,
    ExpeditionRecalled,
    ExpeditionReturned,
    /// **A band split in two where it stood** — the `split_band` verb
    /// (`docs/plan_band_fission.md`). It is also the failure channel for a refused split, so a band
    /// that cannot split says why on the same kind the success would have used.
    ///
    /// Rare, player-initiated and irreversible, which is what separates it from the
    /// expedition lines around it: those report a party's ordinary progress, this reports the map
    /// gaining a second band.
    BandFounded,
    /// A narrative beat from The Telling (`core_sim::telling`). The wire field is already a
    /// string, so the feed renders new kinds generically — no schema or client change.
    NarrativeBeat,
    /// A **fork** from The Telling was answered (by the player, or by the expiry valve resolving
    /// it to its defer choice). The chosen line joins the story record rather than the decision
    /// being a silent state change.
    NarrativeFork,
    /// A managed herd **became under-contained** (neglect-escape slice 2,
    /// `docs/plan_fauna_neglect_escape.md` §4): too few herders to hold all its animals, so it is
    /// shedding whole animals into the wild web. Edge-gated (fires once on the transition, not every
    /// turn), distinct from the pen-*lost* (`Corral`) and pen-*starving* (`Corral`) edges — this is the
    /// herder-shortfall edge, and it applies to pastoral herds too.
    HerdUnderHerded,
    /// **A whole child was born** into a band (`systems::population`). Births are a per-turn *rate*
    /// (`working × fertility`), so this fires when the band's birth accumulator crosses a whole
    /// person — never once per turn, and never rounded per turn. See
    /// [`crate::components::DemographicFlowAccumulator`].
    Born,
    /// **Whole people died** in one age bracket, with the dominant cause (hunger or cold) recorded
    /// on the turn the accumulator crossed. The cause is carried on the event rather than
    /// re-derived later: post-turn state no longer knows which term killed them.
    Died,
    /// **A child reached working age** — the maturation accumulator crossed a whole person, which is
    /// a new pair of hands rather than merely a bigger head-count.
    CameOfAge,
    /// **People left or joined a band** through discontent-driven migration. Whole counts already
    /// (`PopulationCohort::last_emigrated` / `last_immigrated`), so this kind needs no accumulator.
    Migrated,
    /// **Workers reached elderhood** — the aging accumulator crossed a whole person. The twin of
    /// [`CommandEventKind::CameOfAge`] at the other end of a working life: it moves nobody in or
    /// out of the band, but it is a pair of hands the player no longer has, which is why the
    /// workforce shrinking is announced rather than merely happening.
    Aged,
}

impl CommandEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandEventKind::Scout => "scout",
            CommandEventKind::FollowHerd => "follow_herd",
            CommandEventKind::FoundSettlement => "found_settlement",
            CommandEventKind::CampaignFounded => "campaign_founded",
            CommandEventKind::CampaignMilestone => "campaign_milestone",
            CommandEventKind::CampaignVictory => "campaign_victory",
            CommandEventKind::Forage => "forage",
            CommandEventKind::Hunt => "hunt",
            CommandEventKind::Tame => "tame",
            CommandEventKind::Cultivate => "cultivate",
            CommandEventKind::Sow => "sow",
            CommandEventKind::Corral => "corral",
            CommandEventKind::Craft => "craft",
            CommandEventKind::HuntDanger => "hunt_danger",
            CommandEventKind::HuntReport => "hunt_report",
            CommandEventKind::PredatorRaid => "predator_raid",
            CommandEventKind::CancelOrder => "cancel_order",
            CommandEventKind::SedentarizationPrompt => "sedentarization_prompt",
            CommandEventKind::SiteDiscovered => "site_discovered",
            CommandEventKind::ExpeditionSent => "expedition_sent",
            CommandEventKind::ExpeditionArrived => "expedition_arrived",
            CommandEventKind::ExpeditionRecalled => "expedition_recalled",
            CommandEventKind::ExpeditionReturned => "expedition_returned",
            CommandEventKind::BandFounded => "band_founded",
            CommandEventKind::NarrativeBeat => "narrative_beat",
            CommandEventKind::NarrativeFork => "narrative_fork",
            CommandEventKind::HerdUnderHerded => "herd_under_herded",
            CommandEventKind::Born => "born",
            CommandEventKind::Died => "died",
            CommandEventKind::CameOfAge => "came_of_age",
            CommandEventKind::Migrated => "migrated",
            CommandEventKind::Aged => "aged",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandEventEntry {
    pub tick: u64,
    pub kind: CommandEventKind,
    pub faction: FactionId,
    pub label: String,
    pub detail: Option<String>,
    /// Monotonic publication sequence, assigned by [`CommandEventLog::push`] and **never reused**,
    /// so the delta path can ship only the rows appended since a client's cursor
    /// (`snapshot::diff_appended`).
    ///
    /// **One-based**, and that is load-bearing: a cursor is compared with `seq > cursor`, so `0`
    /// has to mean *no event* — a zeroth event would be permanently unsendable to a client whose
    /// cursor starts at `0`. Every `new` leaves it `0` for the same reason: an unpushed entry
    /// carries the "no sequence" value, and the log is the only writer.
    pub seq: u64,
}

impl CommandEventEntry {
    pub fn new<S: Into<String>>(
        tick: u64,
        kind: CommandEventKind,
        faction: FactionId,
        label: S,
        detail: Option<String>,
    ) -> Self {
        Self {
            tick,
            kind,
            faction,
            label: label.into(),
            detail,
            // The real value is stamped by `CommandEventLog::push` — the log owns the sequence, so
            // no call site can hand out a number the log has already issued. `0` is the
            // never-pushed value; real sequences start at 1.
            seq: 0,
        }
    }
}

/// Hard cap on how many entries the log will hold **regardless of the turn window**.
///
/// The window is the real bound; this is the backstop that stops one pathological turn (a
/// crisis firing an event per band per source) from growing the log — and therefore the resync
/// snapshot — without limit. Reaching it means events from *within* the window are dropped, so it
/// is set well above a normal turn's traffic rather than at it.
const MAX_RETAINED_EVENTS: usize = 512;

/// The first sequence a log issues. **Not zero** — a delta ships the rows whose `seq` exceeds the
/// client's cursor, and a fresh cursor is `0`, so a zeroth event could never be sent.
const FIRST_COMMAND_EVENT_SEQ: u64 = 1;

/// The player-facing feed of resolved commands and world events, bounded by a **turn window**.
///
/// It used to keep the newest 32 entries. Once the sim reports births, deaths and coming-of-age
/// per band, a count-bounded ring evicts a wolf raid within two turns — the bound would eat exactly
/// what it exists to preserve. A turn window drops whole turns off the back instead, which is the
/// unit the player (and the client's grouped log) thinks in.
#[derive(Resource, Debug, Clone)]
pub struct CommandEventLog {
    entries: Vec<CommandEventEntry>,
    retention_turns: u64,
    next_seq: u64,
}

impl Default for CommandEventLog {
    fn default() -> Self {
        Self::with_retention_turns(default_command_events_retention_turns())
    }
}

impl CommandEventLog {
    pub fn with_retention_turns(retention_turns: u64) -> Self {
        Self {
            entries: Vec::new(),
            retention_turns,
            // One-based — see `CommandEventEntry::seq`.
            next_seq: FIRST_COMMAND_EVENT_SEQ,
        }
    }

    /// Append an event, stamping it with the next sequence number, then drop every entry that has
    /// fallen out of the turn window.
    ///
    /// The newest entry's tick is the window's anchor: pushes are monotonic in tick (a turn
    /// resolves before the next one starts, and a rollback replaces the whole log), so the entry
    /// just pushed is the latest turn the log knows about.
    pub fn push(&mut self, mut entry: CommandEventEntry) {
        entry.seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        let anchor = entry.tick;
        self.entries.push(entry);
        self.evict(anchor);
    }

    pub fn iter(&self) -> impl Iterator<Item = &CommandEventEntry> {
        self.entries.iter()
    }

    /// How many **distinct turns** the window keeps, counting the newest one — see [`Self::evict`].
    pub fn retention_turns(&self) -> u64 {
        self.retention_turns
    }

    /// Re-window the log (a `simulation_config.json` reload), pruning immediately so the live log
    /// matches the number the snapshot is about to publish.
    pub fn set_retention_turns(&mut self, retention_turns: u64) {
        self.retention_turns = retention_turns;
        if let Some(anchor) = self.entries.last().map(|entry| entry.tick) {
            self.evict(anchor);
        }
    }

    /// The sequence the **next** push will claim — i.e. one past the highest issued.
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Every retained entry appended after `cursor`, oldest first. The delta path's whole reason
    /// for the sequence: a client holding `cursor` needs exactly these rows.
    pub fn appended_since(&self, cursor: u64) -> impl Iterator<Item = &CommandEventEntry> {
        self.entries.iter().filter(move |entry| entry.seq > cursor)
    }

    /// Drop everything older than the window anchored at `anchor_tick`, then apply the backstop.
    ///
    /// The window is **inclusive of the anchor turn**, so `retention_turns` of N keeps ticks
    /// `anchor − (N − 1) ..= anchor` — exactly N distinct turns, which is what the config key's
    /// name promises and what the client's own prune keeps. (It used to reach back N turns *past*
    /// the anchor and so kept N+1, shipping one turn the dock immediately discarded.)
    ///
    /// The `N − 1` leans on `retention_turns` never being zero:
    /// [`SimulationConfigError::ZeroCommandEventsRetentionTurns`] rejects a configured `0` at parse
    /// time, which is also what makes the number representable on the wire. `saturating_sub` keeps
    /// a hand-built zero-turn log (only reachable through [`Self::with_retention_turns`]) at the
    /// anchor turn alone rather than underflowing into "keep everything" — but the config guard is
    /// the invariant, so restore it before removing it.
    fn evict(&mut self, anchor_tick: u64) {
        let turns_before_anchor = self.retention_turns.saturating_sub(1);
        let oldest_kept = anchor_tick.saturating_sub(turns_before_anchor);
        let stale = self
            .entries
            .iter()
            .take_while(|entry| entry.tick < oldest_kept)
            .count();
        if stale > 0 {
            self.entries.drain(0..stale);
        }
        if self.entries.len() > MAX_RETAINED_EVENTS {
            let overflow = self.entries.len() - MAX_RETAINED_EVENTS;
            self.entries.drain(0..overflow);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_load::{clear_override_paths, lock_config_registry_for_test};
    use crate::config_override::install_config_override;
    use sim_runtime::commands::ConfigOverrideKind;
    use std::net::Ipv4Addr;

    /// A climate value the shipped `simulation_config.json` does not carry, so "the staged override
    /// decided this" cannot be confused with "the file did".
    const STAGED_EQUATOR_TEMP: f32 = 33.5;
    /// What the *outgoing* world was running on. Distinct from both the shipped and the staged
    /// value, so a config cloned from the old world is unmistakable in the assertion.
    const OUTGOING_EQUATOR_TEMP: f32 = 11.25;
    /// Stands in for a port block that auto-bumped at boot: a base the file never named, which the
    /// running process nonetheless holds.
    const BUMPED_PORT_BASE: u16 = 41530;

    /// **A staged `simulation` override must reach the world the next New Game builds, and only what
    /// the file cannot know may survive that rebuild.**
    ///
    /// The rebuild used to clone the *outgoing* world's `SimulationConfig`, so every lever the
    /// client's tuning panel exposes under the `simulation` kind installed, logged, and then did
    /// nothing at all. Reloading afresh is only half the answer, though: `fog_enabled` and the bound
    /// ports exist nowhere on disk, and a New Game that took the file's word for them would silently
    /// undo a `set_fog` and start claiming ports the process never bound.
    ///
    /// `crisis_auto_seed` is the counter-example, and it is asserted here for that reason: it is an
    /// ordinary lever in `simulation_config.json`, so carrying it would make it permanently
    /// un-overridable — the exact failure the fresh load exists to fix, one field over.
    #[test]
    fn a_new_world_config_takes_the_file_and_keeps_only_what_the_file_cannot_know() {
        // The override registry is process-global; this is the guard that serializes every test
        // that stages into it.
        let _guard = lock_config_registry_for_test();
        clear_override_paths();

        let dir = std::env::temp_dir().join(format!(
            "shadow_scale_new_world_config_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        install_config_override(
            ConfigOverrideKind::Simulation,
            &format!(r#"{{"climate": {{"equator_temp": {STAGED_EQUATOR_TEMP}}}}}"#),
            &dir,
        )
        .expect("an equator temperature inside its range is valid");

        let shipped = SimulationConfig::builtin();
        assert_ne!(
            shipped.climate.equator_temp, STAGED_EQUATOR_TEMP,
            "the staged value must differ from the shipped one or this test proves nothing"
        );

        let mut outgoing = shipped.clone();
        outgoing.climate.equator_temp = OUTGOING_EQUATOR_TEMP;
        outgoing.fog_enabled = !shipped.fog_enabled;
        // Negated so the outgoing value is unmistakably not the file's: whichever of the two the
        // rebuild ends up with, the assertions below can tell which.
        outgoing.crisis_auto_seed = !shipped.crisis_auto_seed;
        assert!(apply_port_base(&mut outgoing, BUMPED_PORT_BASE));

        let rebuilt = load_simulation_config_for_new_world(&outgoing);

        // Whatever the assertions below find, nothing else in this process may keep loading the
        // staged file.
        clear_override_paths();

        assert_eq!(
            rebuilt.climate.equator_temp, STAGED_EQUATOR_TEMP,
            "the new world must boot on the staged override, not on the outgoing world's config"
        );
        assert_eq!(
            rebuilt.fog_enabled, outgoing.fog_enabled,
            "a New Game must not undo `set_fog`"
        );
        assert_eq!(
            rebuilt.crisis_auto_seed, shipped.crisis_auto_seed,
            "`crisis_auto_seed` is a file lever, so a New Game must take the file's value and not \
             the outgoing world's — carrying it would make it un-overridable"
        );
        for (rebuilt_bind, carried) in [
            (rebuilt.port_base_bind, outgoing.port_base_bind),
            (rebuilt.command_bind, outgoing.command_bind),
            (rebuilt.snapshot_flat_bind, outgoing.snapshot_flat_bind),
            (rebuilt.log_bind, outgoing.log_bind),
        ] {
            assert_eq!(
                rebuilt_bind, carried,
                "the rebuilt config must describe the ports the process actually holds"
            );
        }
    }

    #[test]
    fn apply_port_base_overrides_ports_and_preserves_hosts() {
        let mut config = SimulationConfig::builtin();
        let base: u16 = 42000;
        assert!(apply_port_base(&mut config, base));

        assert_eq!(config.port_base_bind.port(), base);
        assert_eq!(config.command_bind.port(), base + COMMAND_PORT_OFFSET);
        assert_eq!(
            config.snapshot_flat_bind.port(),
            base + SNAPSHOT_FLAT_PORT_OFFSET
        );
        assert_eq!(config.log_bind.port(), base + LOG_PORT_OFFSET);

        for bind in [
            config.port_base_bind,
            config.command_bind,
            config.snapshot_flat_bind,
            config.log_bind,
        ] {
            assert_eq!(bind.ip(), Ipv4Addr::LOCALHOST);
        }
    }

    #[test]
    fn apply_port_base_rejects_overflow_and_leaves_ports_unchanged() {
        let mut config = SimulationConfig::builtin();
        let before = (
            config.port_base_bind.port(),
            config.command_bind.port(),
            config.snapshot_flat_bind.port(),
            config.log_bind.port(),
        );

        // 65533 + LOG_PORT_OFFSET (3) overflows u16.
        assert!(!apply_port_base(&mut config, 65533));

        assert_eq!(
            (
                config.port_base_bind.port(),
                config.command_bind.port(),
                config.snapshot_flat_bind.port(),
                config.log_bind.port(),
            ),
            before
        );
    }

    #[test]
    fn apply_port_base_rejects_zero_and_leaves_ports_unchanged() {
        let mut config = SimulationConfig::builtin();
        let before = (
            config.port_base_bind.port(),
            config.command_bind.port(),
            config.snapshot_flat_bind.port(),
            config.log_bind.port(),
        );

        // base 0 is the wildcard-port sentinel, not a block; below MIN_PORT_BASE.
        assert!(!apply_port_base(&mut config, 0));

        assert_eq!(
            (
                config.port_base_bind.port(),
                config.command_bind.port(),
                config.snapshot_flat_bind.port(),
                config.log_bind.port(),
            ),
            before
        );
    }

    fn event(tick: u64) -> CommandEventEntry {
        CommandEventEntry::new(
            tick,
            CommandEventKind::Born,
            FactionId(0),
            format!("tick {tick}"),
            None,
        )
    }

    /// The sequence numbers a log currently holds, oldest first.
    fn seqs(log: &CommandEventLog) -> Vec<u64> {
        log.iter().map(|entry| entry.seq).collect()
    }

    /// The window drops whole TURNS, which is the point of replacing the 32-entry ring: a turn that
    /// fires twenty events must not evict the previous turn's.
    #[test]
    fn the_turn_window_evicts_by_tick_not_by_count() {
        let mut log = CommandEventLog::with_retention_turns(2);
        for tick in 0..=1u64 {
            for _ in 0..20 {
                log.push(event(tick));
            }
        }
        assert_eq!(log.iter().count(), 40, "two turns inside a 2-turn window");

        log.push(event(2));
        assert!(
            log.iter().all(|entry| entry.tick >= 1),
            "tick 0 fell out of the window whole: {:?}",
            log.iter().map(|entry| entry.tick).collect::<Vec<_>>()
        );
        assert_eq!(
            log.iter().filter(|entry| entry.tick == 1).count(),
            20,
            "…and the turns still inside it are untouched"
        );
    }

    /// **`retention_turns` of N keeps N turns, not N+1.** The key is named for a count of turns and
    /// the client prunes to exactly that many, so an off-by-one here ships a turn the dock discards
    /// on ingest — and makes the expanded log's "showing N of M retained turns" foot describe a
    /// frame that carried M+1.
    #[test]
    fn the_window_keeps_exactly_retention_turns_distinct_turns() {
        for retention_turns in 1..=5u64 {
            let mut log = CommandEventLog::with_retention_turns(retention_turns);
            for tick in 0..20u64 {
                log.push(event(tick));
            }
            let ticks: Vec<u64> = log.iter().map(|entry| entry.tick).collect();
            let expected: Vec<u64> = (20 - retention_turns..20).collect();
            assert_eq!(
                ticks, expected,
                "a {retention_turns}-turn window anchored at tick 19"
            );
        }
    }

    /// Eviction never rewinds the sequence: a client's cursor is a statement about what it has
    /// SEEN, so a reissued number would silently suppress a real event.
    #[test]
    fn the_sequence_is_monotonic_across_eviction() {
        // Two turns, so more than one row survives and the contiguity assertion below has
        // something to measure.
        let mut log = CommandEventLog::with_retention_turns(2);
        for tick in 0..10u64 {
            log.push(event(tick));
        }
        let held = seqs(&log);
        assert!(
            held.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "held sequence is contiguous and rising: {held:?}"
        );
        assert_eq!(
            log.next_seq(),
            11,
            "ten pushes issued ten numbers, one-based"
        );
        assert_eq!(
            *held.last().expect("a retained entry"),
            10,
            "the newest entry carries the newest number, not a recycled one"
        );
    }

    /// The backstop bounds one pathological turn. Everything here shares a tick, so the window
    /// evicts nothing and only `MAX_RETAINED_EVENTS` can.
    #[test]
    fn the_backstop_holds_when_one_turn_floods_the_window() {
        let mut log = CommandEventLog::default();
        for _ in 0..(MAX_RETAINED_EVENTS + 50) {
            log.push(event(7));
        }
        assert_eq!(log.iter().count(), MAX_RETAINED_EVENTS);
        assert_eq!(
            log.iter().next().expect("a retained entry").seq,
            51,
            "the backstop drains from the FRONT — the oldest 50 went, not the newest"
        );
    }

    /// A fresh cursor is `0` and the first event is `1`, so the very first event is sendable. A
    /// zero-based sequence would have made it permanently invisible to every new client.
    #[test]
    fn the_first_event_is_above_a_fresh_cursor() {
        let mut log = CommandEventLog::default();
        log.push(event(0));
        assert_eq!(log.appended_since(0).count(), 1);
    }

    /// `appended_since` answers about the client's cursor, not about the log's own indices, so an
    /// eviction between two frames must not resurrect rows the client already holds.
    #[test]
    fn appended_since_is_correct_across_an_eviction() {
        let mut log = CommandEventLog::with_retention_turns(2);
        for tick in 0..3u64 {
            log.push(event(tick));
        }
        let cursor = log
            .iter()
            .map(|entry| entry.seq)
            .max()
            .expect("three entries");
        assert!(
            log.appended_since(cursor).next().is_none(),
            "a caught-up cursor is owed nothing"
        );

        // Two more turns: the window drops ticks 1 and 2 while two new rows arrive.
        log.push(event(3));
        log.push(event(4));
        let owed: Vec<u64> = log.appended_since(cursor).map(|entry| entry.tick).collect();
        assert_eq!(
            owed,
            vec![3, 4],
            "only the genuinely new rows, and the evicted ones are simply gone"
        );
        assert_eq!(
            log.iter().count(),
            2,
            "the window kept ticks 3..=4 and dropped everything before them"
        );
    }

    /// The window is a config lever, so re-windowing prunes immediately rather than waiting for the
    /// next push — otherwise the published retention and the published rows would disagree.
    #[test]
    fn narrowing_the_window_prunes_immediately() {
        let mut log = CommandEventLog::with_retention_turns(20);
        for tick in 0..=20u64 {
            log.push(event(tick));
        }
        assert_eq!(
            log.iter().count(),
            20,
            "ticks 1..=20 — twenty turns, not 21"
        );

        log.set_retention_turns(2);
        assert_eq!(log.retention_turns(), 2);
        assert_eq!(
            log.iter().map(|entry| entry.tick).collect::<Vec<_>>(),
            vec![19, 20]
        );
    }

    /// **A zero-turn window is rejected at parse time.** Server-side `0` is coherent (the log would
    /// keep the current turn alone), but it is unrepresentable on the wire: it is the FlatBuffers
    /// default for `CampaignSection.commandEventsRetentionTurns` and both client decoders read a
    /// `0` as "the sim did not state a window", falling back to their own default. That divergence
    /// is silent, so the config never gets to express it.
    #[test]
    fn a_zero_retention_window_is_refused_by_the_config() {
        let json = BUILTIN_SIMULATION_CONFIG.replace(
            "\"command_events_retention_turns\": 20",
            "\"command_events_retention_turns\": 0",
        );
        assert!(
            json != BUILTIN_SIMULATION_CONFIG,
            "the builtin config still carries the key this test rewrites"
        );

        let err = SimulationConfig::from_json_str(&json)
            .expect_err("a zero-turn window must not parse into a config");
        assert!(
            matches!(err, SimulationConfigError::ZeroCommandEventsRetentionTurns),
            "rejected for the right reason: {err}"
        );
        assert!(
            !err.is_not_found(),
            "an incoherent value is a file that is there and wrong, so boot must panic rather \
             than fall back to the builtin"
        );
    }
}
