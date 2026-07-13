use std::collections::HashMap;
use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};
use sim_runtime::TerrainTags;
use sim_schema::HerdState;
use tracing::info;

use std::hash::{Hash, Hasher};

use crate::{
    components::{FollowPolicy, PopulationCohort, Tile, FOOD},
    fauna_config::{EcologyConfig, FaunaConfig, FaunaConfigHandle, SizeClass, SpeciesDef},
    food::{classify_food_module, FoodModule},
    grid_utils::{hex_distance_wrapped, hex_neighbor, HEX_DIRECTION_COUNT},
    hashing::FnvHasher,
    mapgen::WorldGenSeed,
    orders::FactionId,
    resources::{SimulationConfig, SimulationTick, StartLocation, TileRegistry},
    scalar::{scalar_from_f32, scalar_zero, Scalar},
    systems::output_multiplier,
    wellbeing_config::WellbeingConfigHandle,
};

/// RNG salt for per-turn immigration, kept distinct from the initial-spawn salt so the
/// two streams don't correlate.
const IMMIGRATION_SEED_SALT: u64 = 0xFA1A_B0B0;

/// RNG salt for per-turn herd graze-wander / loiter movement, distinct from the immigration
/// stream. Combined with `map_seed ^ tick ^ hash(herd.id)` so each herd's wander is deterministic
/// under rollback (mirrors `repopulate_fauna`'s seeding).
const HERD_MOVEMENT_SEED_SALT: u64 = 0x4D0E_9A17_C0FF_EE21;

/// Id prefix marking a short-range wild-game group (migratory herds use `herd_`). The
/// `abundance.max_total_game` cap applies to these groups only — both at initial spawn
/// (`placed.len()`) and per-turn immigration.
const GAME_ID_PREFIX: &str = "game_";

pub const HERD_DENSITY_REFERENCE_BIOMASS: f32 = 8_000.0;

/// Discovery id for the faction-level **Herding** knowledge (Intensification Rung 1c — the
/// earned-knowledge gate on the animal-pen path, `docs/plan_intensification.md` §4b; the animal
/// mirror of `forage::CULTIVATION_DISCOVERY_ID`). Knowledge is **earned by doing**: a band
/// Sustain-hunting a Thriving herd accrues this discovery in the per-faction
/// `DiscoveryProgressLedger` (`advance_labor_allocation`), and the `corral` command is refused until
/// the faction knows Herding. Declared as a start-profile knowledge tag (`herding` → this id in
/// `data/start_profile_knowledge_tags.json`) purely so it is mappable; it is deliberately **not**
/// listed in any start profile's `starting_knowledge_tags`, so no faction starts knowing it. Note
/// the asymmetry vs. Cultivation: mobile *domestication* (pastoralism) stays ungated — only
/// **corralling** (pinning a domesticated herd) needs Herding. Next free id after
/// `cultivation` (2003).
pub const HERDING_DISCOVERY_ID: u32 = 2004;

/// Coarse ecological health band derived from a group's biomass vs its carrying
/// capacity (thresholds in `EcologyConfig`). Surfaced to the client as an early
/// overhunting warning, and the seam the later domestication / industrialized-hunting
/// arc keys off (e.g. a long Sustain-follow on a `Thriving` herd → husbandry progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EcologyPhase {
    /// At or above the stressed band — a healthy, self-sustaining group.
    #[default]
    Thriving,
    /// Depleted but above the collapse threshold — still able to recover if left alone.
    Stressed,
    /// Below the Allee threshold — non-viable and crashing to local extinction
    /// regardless of whether hunting continues (the point of no return).
    Collapsing,
}

impl EcologyPhase {
    /// Stable string key (also the snapshot `ecologyPhase` field).
    pub fn as_str(self) -> &'static str {
        match self {
            EcologyPhase::Thriving => "thriving",
            EcologyPhase::Stressed => "stressed",
            EcologyPhase::Collapsing => "collapsing",
        }
    }

    /// Parse the stable string key back into a phase (inverse of `as_str`; the rollback restore
    /// path). Unknown/empty strings resolve to the `Default` (`Thriving`).
    pub fn from_key(key: &str) -> Self {
        match key {
            "stressed" => EcologyPhase::Stressed,
            "collapsing" => EcologyPhase::Collapsing,
            _ => EcologyPhase::Thriving,
        }
    }
}

/// Classify a group's ecological phase from its biomass fraction of carrying capacity.
pub(crate) fn classify_ecology_phase(
    biomass: f32,
    cap: f32,
    ecology: &EcologyConfig,
) -> EcologyPhase {
    if cap <= 0.0 {
        return EcologyPhase::Collapsing;
    }
    let frac = biomass / cap;
    if frac < ecology.collapse_fraction {
        EcologyPhase::Collapsing
    } else if frac < ecology.stressed_fraction {
        EcologyPhase::Stressed
    } else {
        EcologyPhase::Thriving
    }
}

/// A herd's per-turn movement mode (graze-wander + loiter-then-migrate, `advance_herds`).
/// Game groups graze-wander their local cluster forever; migratory groups alternate loitering near
/// a route anchor and a directed 1-hex/turn migration to the next anchor. See
/// `docs/plan_wildlife_hunting_overlay.md` "Herd Movement".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoamState {
    /// Wild game (`Big`/`Small`): permanent graze-wander toward the current cluster waypoint.
    GrazeWander,
    /// Migratory: loitering near the current anchor for `turns_left` more turns.
    Loiter { turns_left: u32 },
    /// Migratory: a directed leg toward the next anchor at 1 hex/turn, no grazing pause.
    Migrate,
}

/// Stable string keys for `RoamState`, shared by the snapshot capture (`HerdRoamState.mode`) and
/// the rollback restore (`RoamState::from_mode`) so the mapping lives in one place.
const ROAM_MODE_GRAZE_WANDER: &str = "graze_wander";
const ROAM_MODE_LOITER: &str = "loiter";
const ROAM_MODE_MIGRATE: &str = "migrate";

impl RoamState {
    /// Stable string key for the movement mode (snapshot `HerdRoamState.mode`).
    pub fn mode_key(self) -> &'static str {
        match self {
            RoamState::GrazeWander => ROAM_MODE_GRAZE_WANDER,
            RoamState::Loiter { .. } => ROAM_MODE_LOITER,
            RoamState::Migrate => ROAM_MODE_MIGRATE,
        }
    }

    /// The loiter countdown (`0` for graze-wander / migrate).
    pub fn loiter_turns_left(self) -> u32 {
        match self {
            RoamState::Loiter { turns_left } => turns_left,
            _ => 0,
        }
    }

    /// Reconstruct from the stable string key + loiter countdown (rollback restore; inverse of
    /// `mode_key` + `loiter_turns_left`). Unknown/empty keys resolve to `GrazeWander`.
    pub fn from_mode(mode: &str, loiter_turns_left: u32) -> Self {
        match mode {
            ROAM_MODE_LOITER => RoamState::Loiter {
                turns_left: loiter_turns_left,
            },
            ROAM_MODE_MIGRATE => RoamState::Migrate,
            _ => RoamState::GrazeWander,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Herd {
    pub id: String,
    pub label: String,
    /// Species display name (also the snapshot `species` string; drives the client
    /// icon via keyword match). Sourced from the data-driven `fauna_config.json`.
    pub species: String,
    /// Coarse size band (snapshot `size_class`); lets the client offer the right verbs.
    pub size_class: SizeClass,
    /// Sparse anchor list (was a dense per-turn path). Game: the small local cluster it wanders;
    /// migratory: the loiter anchors a migration cycles through. `step_index` is the current one.
    pub route: Vec<UVec2>,
    pub step_index: usize,
    /// Live position — walked one hex per move by `advance_herds` (no longer `route[step_index]`).
    pub current_pos: UVec2,
    /// Grazing pause countdown (graze-wander cadence); moves only when this hits 0.
    pub dwell_remaining: u32,
    /// Current movement mode (graze-wander for game, loiter/migrate for migratory).
    pub roam: RoamState,
    /// Next intended hex (client heading arrow): the tile a `Migrate` leg heads to next, else `None`
    /// (loitering/grazing herds show no arrow).
    pub next_pos: Option<UVec2>,
    pub biomass: f32,
    /// Per-species carrying capacity (= table biomass max) that biomass regrows toward.
    pub carrying_capacity: f32,
    /// Coarse health band (Thriving/Stressed/Collapsing), recomputed each turn from
    /// biomass vs `carrying_capacity`. Surfaced to the client and the domestication hook.
    pub ecology_phase: EcologyPhase,
    /// Husbandry progress in `[0.0, 1.0]`; `1.0` = domesticated. Accrues while a band
    /// Sustain-follows this (Thriving) group and decays otherwise (see `advance_husbandry`).
    pub domestication_progress: f32,
    /// Faction tending/owning this group (`Some` iff `domestication_progress > 0`).
    pub owner: Option<FactionId>,
    /// Corral (Rung 1c): the tile a **penned** herd is fixed at, or `None` for a mobile herd.
    /// `Some` = the herd does NOT roam (`advance_herds` skips its movement — it stays put) and is
    /// paid its keeper **place-local** at the higher corral rate (via the tending Hunt assignment in
    /// `advance_labor_allocation`), not the mobile even-split husbandry yield. Only a *domesticated*
    /// herd whose owner knows Herding can be corralled (`corral` command). Authoritative sim state —
    /// snapshot-persisted. The animal mirror of a cultivated patch being a fixed tended patch;
    /// contrast the deliberate asymmetry — an *un*corralled domesticated herd stays mobile
    /// (pastoralism travels with the band).
    pub corralled_at: Option<UVec2>,
    /// Transient per-turn flag: a Hunt assignment tended this corralled herd this turn (set in
    /// `advance_labor_allocation`, Population). `advance_husbandry` (Logistics, the *next* turn —
    /// Logistics runs before Population) reads it: a corralled herd tended this turn is spared, an
    /// untended one **escapes** (reverts to mobile). Mirrors `ForagePatch::tended_this_turn`. **Not**
    /// snapshot-persisted (derived) — a rehydrated corralled herd reads `false` until tended again,
    /// so a rollback can only *delay* an escape by one turn, never resurrect a broken-out herd.
    pub corralled_tended_this_turn: bool,
}

impl Herd {
    pub fn new(
        id: String,
        species_display: String,
        size_class: SizeClass,
        route: Vec<UVec2>,
        biomass: f32,
        carrying_capacity: f32,
    ) -> Self {
        let label = format!("{} ({})", species_display, id);
        let current_pos = route.first().copied().unwrap_or_else(|| UVec2::new(0, 0));
        // Migratory groups start loitering at their spawn anchor (the caller samples the real
        // `turns_left` from the species' `loiter_turns`); game groups graze-wander their cluster.
        let roam = if size_class == SizeClass::Migratory {
            RoamState::Loiter { turns_left: 0 }
        } else {
            RoamState::GrazeWander
        };
        Self {
            id,
            label,
            species: species_display,
            size_class,
            route,
            step_index: 0,
            current_pos,
            dwell_remaining: 0,
            roam,
            next_pos: None,
            biomass,
            carrying_capacity,
            // Refreshed against the ecology config at spawn/each turn; Thriving until then.
            ecology_phase: EcologyPhase::Thriving,
            domestication_progress: 0.0,
            owner: None,
            corralled_at: None,
            corralled_tended_this_turn: false,
        }
    }

    /// Recompute `ecology_phase` from the current biomass against the ecology config.
    pub(crate) fn refresh_ecology_phase(&mut self, ecology: &EcologyConfig) {
        self.ecology_phase = classify_ecology_phase(self.biomass, self.carrying_capacity, ecology);
    }

    /// A fully-tamed (managed livestock) group: yields provisions each turn and is
    /// immune to the overhunting collapse.
    pub fn is_domesticated(&self) -> bool {
        self.domestication_progress >= 1.0
    }

    /// Accrue husbandry progress for `faction` (the tending band). Sets ownership on the
    /// first accrual; only the owner makes progress. Clamped to 1.0 (auto-domestication).
    pub(crate) fn accrue_domestication(&mut self, faction: FactionId, amount: f32) {
        if self.is_domesticated() {
            return;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner == Some(faction) {
            self.domestication_progress = (self.domestication_progress + amount).min(1.0);
        }
    }

    /// Decay husbandry progress toward zero when the group isn't being actively tended;
    /// ownership lapses once progress reaches zero. A domesticated group is left alone.
    pub(crate) fn decay_domestication(&mut self, amount: f32) {
        if self.is_domesticated() {
            return;
        }
        self.domestication_progress = (self.domestication_progress - amount).max(0.0);
        // Reconcile the `owner is Some ⟺ progress > 0` invariant unconditionally, so a
        // group that reaches (or somehow sits at) zero progress never keeps a stale owner
        // — which would otherwise block another faction from ever tending it.
        if self.domestication_progress <= 0.0 {
            self.owner = None;
        }
    }

    /// Finalize domestication for `faction` (the `domesticate` command's early claim): set
    /// ownership and snap progress to 1.0 so `is_domesticated()` latches. Taking the faction
    /// here makes the `owner is Some ⟺ progress > 0` invariant impossible to violate (no
    /// ownerless domesticated herd).
    pub fn claim_domestication(&mut self, faction: FactionId) {
        self.owner = Some(faction);
        self.domestication_progress = 1.0;
    }

    /// A **corralled** (penned) herd: fixed at `corralled_at`, doesn't roam, and is paid its keeper
    /// place-local at the higher corral rate. The animal mirror of `ForagePatch::is_cultivated`
    /// gating the tended-patch behaviour.
    pub fn is_corralled(&self) -> bool {
        self.corralled_at.is_some()
    }

    /// Pen the herd at `tile` (the `corral` command). Fixes its position and grants a one-turn
    /// "tended" grace (`corralled_tended_this_turn = true`) so the first `advance_husbandry` pass
    /// after corralling spares it — the keeper's Hunt assignment then re-marks it tended each
    /// Population stage to keep it penned. Caller gates on domesticated + owner + Herding known.
    pub fn corral_at(&mut self, tile: UVec2) {
        self.corralled_at = Some(tile);
        self.current_pos = tile;
        self.next_pos = None;
        self.corralled_tended_this_turn = true;
    }

    /// The herd's live tile — walked one hex per move by `advance_herds` (graze-wander /
    /// loiter-migrate), no longer a teleport to `route[step_index]`.
    pub fn position(&self) -> UVec2 {
        self.current_pos
    }

    pub fn route_length(&self) -> usize {
        self.route.len()
    }

    /// The herd's next intended hex — the client heading arrow. `Some` only during a `Migrate` leg
    /// (one hex toward the target anchor); `None` while loitering/grazing (no misleading arrow).
    pub fn next_position(&self) -> Option<UVec2> {
        self.next_pos
    }
}

#[derive(Debug, Clone, Default)]
pub struct HerdTelemetryEntry {
    pub id: String,
    pub label: String,
    pub species: String,
    pub size_class: String,
    pub huntable: bool,
    /// Ecological health band string (see `EcologyPhase::as_str`).
    pub ecology_phase: String,
    /// Husbandry progress in `[0.0, 1.0]` (`1.0` = domesticated).
    pub domestication: f32,
    /// Rung 1c corral state: `true` iff the herd is penned (`Herd::is_corralled`). Client shows a
    /// place-bound corral indicator distinct from a mobile domesticated herd.
    pub corralled: bool,
    pub position: UVec2,
    pub biomass: f32,
    pub route_length: u32,
    pub next_position: Option<UVec2>,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct HerdRegistry {
    pub herds: Vec<Herd>,
}

impl HerdRegistry {
    pub fn clear(&mut self) {
        self.herds.clear();
    }

    pub fn find(&self, id: &str) -> Option<&Herd> {
        self.herds.iter().find(|herd| herd.id == id)
    }

    pub fn entries(&self) -> &[Herd] {
        &self.herds
    }

    pub fn snapshot_entries(&self) -> Vec<HerdTelemetryEntry> {
        self.herds.iter().map(to_entry).collect()
    }

    /// Number of domesticated groups owned by `faction`. The seam the future
    /// `SedentarizationScore` reads for its "domestication progress" input (`TASKS.md`).
    pub fn domesticated_count(&self, faction: FactionId) -> usize {
        self.herds
            .iter()
            .filter(|herd| herd.is_domesticated() && herd.owner == Some(faction))
            .count()
    }

    /// Rebuild the authoritative herd list from a rollback snapshot's `HerdState`s (clear + rebuild,
    /// mirroring `GenerationRegistry::update_from_states`). Restores biomass / position / movement /
    /// ecology so a rollback rewinds herd sim state, not just display telemetry.
    pub fn update_from_states(&mut self, states: &[HerdState]) {
        self.herds = states.iter().map(herd_from_state).collect();
    }

    /// Construct a registry directly from snapshot `HerdState`s (mirrors
    /// `GenerationRegistry::from_states`).
    pub fn from_states(states: &[HerdState]) -> Self {
        let mut registry = Self::default();
        registry.update_from_states(states);
        registry
    }
}

/// Reconstruct a live `Herd` from its snapshot mirror (the rollback restore side of `herd_state`
/// in `snapshot.rs`). Parses the `ecology_phase` / `size_class` / `roam` string keys back to their
/// live enums.
fn herd_from_state(state: &HerdState) -> Herd {
    Herd {
        id: state.id.clone(),
        label: state.label.clone(),
        species: state.species.clone(),
        size_class: SizeClass::from_key(&state.size_class),
        route: state.route.iter().map(|&(x, y)| UVec2::new(x, y)).collect(),
        step_index: state.step_index as usize,
        current_pos: UVec2::new(state.current_pos.0, state.current_pos.1),
        dwell_remaining: state.dwell_remaining,
        roam: RoamState::from_mode(&state.roam.mode, state.roam.loiter_turns_left),
        next_pos: state.next_pos.map(|(x, y)| UVec2::new(x, y)),
        biomass: state.ecology.biomass,
        carrying_capacity: state.ecology.carrying_capacity,
        ecology_phase: EcologyPhase::from_key(&state.ecology.ecology_phase),
        domestication_progress: state.ecology.progress,
        owner: state.ecology.owner.map(FactionId),
        corralled_at: state.corralled_at.map(|(x, y)| UVec2::new(x, y)),
        // Transient (not persisted) — a rehydrated corralled herd is "untended" until worked again.
        corralled_tended_this_turn: false,
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct HerdTelemetry {
    pub entries: Vec<HerdTelemetryEntry>,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct HerdDensityMap {
    pub width: u32,
    pub height: u32,
    samples: Vec<f32>,
}

impl HerdDensityMap {
    pub fn rebuild(&mut self, grid_size: UVec2, registry: &HerdRegistry) {
        let samples: Vec<(UVec2, f32)> = registry
            .herds
            .iter()
            .map(|herd| (herd.position(), herd.biomass))
            .collect();
        self.rebuild_from_samples(grid_size, &samples);
    }

    pub fn rebuild_from_samples(&mut self, grid_size: UVec2, herds: &[(UVec2, f32)]) {
        let width = grid_size.x.max(1);
        let height = grid_size.y.max(1);
        let total = width.saturating_mul(height).max(1);
        if self.width != width || self.height != height || self.samples.len() != total as usize {
            self.width = width;
            self.height = height;
            self.samples = vec![0.0; total as usize];
        } else {
            self.samples.fill(0.0);
        }

        for (pos, biomass) in herds {
            if pos.x >= self.width || pos.y >= self.height {
                continue;
            }
            let idx = (pos.y as usize) * self.width as usize + pos.x as usize;
            self.samples[idx] += *biomass;
        }
    }

    pub fn density_at(&self, pos: UVec2) -> f32 {
        if self.samples.is_empty() || pos.x >= self.width || pos.y >= self.height {
            return 0.0;
        }
        let idx = (pos.y as usize) * self.width as usize + pos.x as usize;
        self.samples.get(idx).copied().unwrap_or(0.0)
    }

    pub fn normalized_density_at(&self, pos: UVec2) -> f32 {
        normalize_density(self.density_at(pos))
    }

    pub fn normalized_pair_average(&self, a: UVec2, b: UVec2) -> f32 {
        let avg = 0.5 * (self.density_at(a) + self.density_at(b));
        normalize_density(avg)
    }

    pub fn normalized_average(&self) -> f32 {
        normalize_density(self.average_density())
    }

    pub fn average_density(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let total: f32 = self.samples.iter().copied().sum();
        total / (self.samples.len() as f32)
    }

    pub fn max_density(&self) -> f32 {
        self.samples
            .iter()
            .copied()
            .fold(0.0f32, |acc, value| acc.max(value))
    }
}

fn normalize_density(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else {
        (value / HERD_DENSITY_REFERENCE_BIOMASS).clamp(0.0, 1.0)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_initial_herds(
    mut registry: ResMut<HerdRegistry>,
    mut telemetry: ResMut<HerdTelemetry>,
    mut density: ResMut<HerdDensityMap>,
    config: Res<SimulationConfig>,
    start_location: Res<StartLocation>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    world_seed: Option<Res<WorldGenSeed>>,
    fauna_config: Res<FaunaConfigHandle>,
) {
    if !registry.herds.is_empty() {
        telemetry.entries = registry.herds.iter().map(to_entry).collect();
        density.rebuild(config.grid_size, &registry);
        return;
    }

    let fauna = fauna_config.get();
    let seed = world_seed
        .map(|seed| seed.0)
        .unwrap_or_else(|| config.map_seed);
    let mut rng = if seed == 0 {
        SmallRng::from_entropy()
    } else {
        SmallRng::seed_from_u64(seed ^ 0xFA1A_FEED)
    };

    let width = config.grid_size.x.max(4);
    let height = config.grid_size.y.max(4);
    let base = start_location
        .position()
        .unwrap_or(UVec2::new(width / 2, height / 2));

    let mut herds = Vec::new();
    // 1. Long-range migratory herds — start-anchored, species/biomass from config.
    spawn_migratory_herds(
        &fauna,
        base,
        width,
        height,
        &tile_registry,
        &tiles,
        &mut rng,
        &mut herds,
    );
    // 2. Short-range wild game — biome-density placement across the whole map.
    spawn_short_range_game(
        &fauna,
        width,
        height,
        &tile_registry,
        &tiles,
        &mut rng,
        &mut herds,
    );

    registry.herds = herds;
    telemetry.entries = registry.snapshot_entries();
    density.rebuild(config.grid_size, &registry);
}

fn log_herd_spawn(herd: &Herd) {
    let position = herd.position();
    info!(
        target: "shadow_scale::analytics",
        event = "herd_spawn",
        herd = %herd.id,
        label = %herd.label,
        species = %herd.species,
        x = position.x,
        y = position.y,
        biomass = herd.biomass,
        route_length = herd.route_length(),
    );
}

/// Long-range migratory herds: a handful of cross-region walkers anchored on the
/// start area, one per `determine_herd_count`, species drawn from the config's
/// migratory rows.
#[allow(clippy::too_many_arguments)]
fn spawn_migratory_herds(
    fauna: &FaunaConfig,
    base: UVec2,
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
    herds: &mut Vec<Herd>,
) {
    let migratory = fauna.migratory_species();
    if migratory.is_empty() {
        return;
    }
    let herd_target = determine_herd_count(width, height);
    for idx in 0..herd_target {
        let (key, def) = migratory[rng.gen_range(0..migratory.len())];
        let steps = def.sample_route_len(rng);
        let Some(route) = build_route(base, width, height, tile_registry, tiles, rng, steps) else {
            continue;
        };
        let biomass = def.sample_biomass(rng);
        let carrying_capacity = def.carrying_capacity();
        let id = format!("herd_{key}_{idx:02}");
        let mut herd = Herd::new(
            id,
            def.display_name.clone(),
            def.size_class,
            route,
            biomass,
            carrying_capacity,
        );
        // Start loitering at the spawn anchor for a randomized window (rather than migrating off
        // immediately from `Loiter { turns_left: 0 }`).
        herd.roam = RoamState::Loiter {
            turns_left: def.sample_loiter_turns(rng),
        };
        herd.refresh_ecology_phase(&fauna.ecology);
        log_herd_spawn(&herd);
        herds.push(herd);
    }
}

/// Short-range wild game (big + small): iterate land tiles, roll the per-biome
/// abundance, then greedily place bounded, spaced-out groups from a shuffled pool
/// so placement is spread across the map rather than clustered by scan order.
fn spawn_short_range_game(
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
    herds: &mut Vec<Herd>,
) {
    // Collect every tile where the abundance roll succeeds (map-wide).
    let mut winners: Vec<(UVec2, &'static str)> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let pos = UVec2::new(x, y);
            let Some(module) = module_at(pos, tile_registry, tiles) else {
                continue;
            };
            let module_key = module.as_str();
            let prob = fauna.abundance.probability_for(module_key);
            if prob <= 0.0 {
                continue;
            }
            if rng.gen::<f32>() < prob {
                winners.push((pos, module_key));
            }
        }
    }
    // Shuffle so the cap + spacing thin the pool uniformly, not top-to-bottom.
    winners.shuffle(rng);

    let max_total = fauna.abundance.max_total_game;
    let min_spacing = fauna.abundance.min_spacing;
    let mut placed: Vec<UVec2> = Vec::new();
    let mut game_idx = 0u32;
    for (pos, module_key) in winners {
        if placed.len() >= max_total {
            break;
        }
        if placed
            .iter()
            .any(|p| chebyshev_distance(*p, pos) < min_spacing)
        {
            continue;
        }
        let Some(herd) = spawn_game_group_at(
            pos,
            module_key,
            game_idx,
            fauna,
            width,
            height,
            tile_registry,
            tiles,
            rng,
        ) else {
            continue;
        };
        game_idx += 1;
        log_herd_spawn(&herd);
        placed.push(pos);
        herds.push(herd);
    }
}

/// Build a single short-range game group at `pos`: pick a species hosting `module_key`,
/// roll its route/biomass, and stamp its initial `ecology_phase`. Returns `None` if no
/// species hosts the biome or the origin is not land. Shared by initial spawn and
/// per-turn immigration.
// Placement needs the config, grid bounds, both tile resources, and the RNG; grouping
// them into a struct would just move the noise without improving clarity.
#[allow(clippy::too_many_arguments)]
fn spawn_game_group_at(
    pos: UVec2,
    module_key: &str,
    game_idx: u32,
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
) -> Option<Herd> {
    let candidates = fauna.game_species_for_biome(module_key);
    if candidates.is_empty() {
        return None;
    }
    let (key, def) = candidates[rng.gen_range(0..candidates.len())];
    let steps = def.sample_route_len(rng);
    let route = build_short_route(pos, steps, width, height, tile_registry, tiles, rng)?;
    let biomass = def.sample_biomass(rng);
    let carrying_capacity = def.carrying_capacity();
    let id = format!("{GAME_ID_PREFIX}{key}_{game_idx:02}");
    let mut herd = Herd::new(
        id,
        def.display_name.clone(),
        def.size_class,
        route,
        biomass,
        carrying_capacity,
    );
    herd.refresh_ecology_phase(&fauna.ecology);
    Some(herd)
}

#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_herds(
    mut registry: ResMut<HerdRegistry>,
    mut telemetry: ResMut<HerdTelemetry>,
    mut density: ResMut<HerdDensityMap>,
    config: Res<SimulationConfig>,
    fauna_config: Res<FaunaConfigHandle>,
    tick: Res<SimulationTick>,
    world_seed: Option<Res<WorldGenSeed>>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
) {
    if registry.herds.is_empty() {
        telemetry.entries.clear();
        density.width = 0;
        density.height = 0;
        density.samples.clear();
        return;
    }
    let fauna = fauna_config.get();
    let ecology = &fauna.ecology;
    let width = config.grid_size.x.max(1);
    let height = config.grid_size.y.max(1);
    let wrap = config.map_topology.wrap_horizontal;
    let base_seed = world_seed.map(|s| s.0).unwrap_or(config.map_seed) ^ tick.0;
    for herd in registry.herds.iter_mut() {
        // Deterministic per-herd, per-turn RNG (rollback-stable): map_seed ^ tick ^ salt ^ id-hash.
        let mut hasher = FnvHasher::new();
        herd.id.hash(&mut hasher);
        let mut rng =
            SmallRng::seed_from_u64(base_seed ^ HERD_MOVEMENT_SEED_SALT ^ hasher.finish());
        // Movement cadence levers for this species (fall back to a slow game default if unresolved).
        let def = fauna.species_by_display(&herd.species);
        // A corralled (penned) herd is fixed at `corralled_at` — it does NOT roam (Rung 1c). It
        // still grazes/regrows (ecology is independent of movement); only its wander is skipped.
        if herd.is_corralled() {
            herd.next_pos = None;
        } else {
            advance_herd_roam(
                herd,
                def,
                &tile_registry,
                &tiles,
                &mut rng,
                width,
                height,
                wrap,
            );
        }
        regrow_biomass(herd, ecology);
        let position = herd.position();
        info!(
            target: "shadow_scale::analytics",
            event = "herd_migrate",
            herd = %herd.id,
            label = %herd.label,
            x = position.x,
            y = position.y,
            step_index = herd.step_index,
            route_length = herd.route_length(),
            biomass = herd.biomass,
            ecology_phase = herd.ecology_phase.as_str(),
        );
    }
    // Local extinction: a group hunted to zero, or a collapsing remnant that has fallen
    // below the viability floor, disperses and despawns.
    registry
        .herds
        .retain(|herd| herd.biomass > ecology.extinction_floor * herd.carrying_capacity);
    telemetry.entries = registry.snapshot_entries();
    density.rebuild(config.grid_size, &registry);
}

/// One turn of graze-wander / loiter-migrate movement (`docs/plan_wildlife_hunting_overlay.md`
/// "Herd Movement"). Deterministic under the per-turn seeded `rng`. Mutates the herd's
/// `current_pos` / `dwell_remaining` / `roam` / `step_index` / `next_pos`. `def` supplies the
/// species' cadence levers (`None` → a slow game default). Movement is ≤ 1 hex/turn and land-clamped;
/// it never touches `biomass` (ecology stays independent — a loitering herd still grazes/regrows).
// Args are the herd + its cadence levers + the grid/tile context needed to land-clamp a hex step;
// bundling them adds noise without clarity (matches the other fauna spawn/movement helpers).
#[allow(clippy::too_many_arguments)]
fn advance_herd_roam(
    herd: &mut Herd,
    def: Option<&SpeciesDef>,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
    width: u32,
    height: u32,
    wrap: bool,
) {
    let dwell_turns = def.map(|d| d.dwell_turns).unwrap_or(1);
    let loiter_radius = def.map(|d| d.loiter_radius).unwrap_or(2);
    herd.next_pos = None;

    match herd.roam {
        RoamState::GrazeWander => {
            // Wild game: graze `dwell_turns`, then step one hex toward the current cluster
            // waypoint, advancing to the next when reached (a route_len==1 group stays put).
            if herd.dwell_remaining > 0 {
                herd.dwell_remaining -= 1;
                return;
            }
            let target = herd
                .route
                .get(herd.step_index)
                .copied()
                .unwrap_or(herd.current_pos);
            if herd.current_pos == target && !herd.route.is_empty() {
                herd.step_index = (herd.step_index + 1) % herd.route.len();
            }
            let target = herd
                .route
                .get(herd.step_index)
                .copied()
                .unwrap_or(herd.current_pos);
            step_herd_toward(herd, target, registry, tiles, width, height, wrap);
            herd.dwell_remaining = dwell_turns;
        }
        RoamState::Loiter { turns_left } => {
            if turns_left == 0 {
                // Loiter expired — commit to migrating to the next anchor (starts next turn).
                herd.roam = RoamState::Migrate;
                return;
            }
            let anchor = herd
                .route
                .get(herd.step_index)
                .copied()
                .unwrap_or(herd.current_pos);
            // Graze-wander confined to `loiter_radius` of the anchor: dwell, then a ≤1-hex nudge.
            if herd.dwell_remaining > 0 {
                herd.dwell_remaining -= 1;
            } else {
                wander_near_anchor(
                    herd,
                    anchor,
                    loiter_radius,
                    registry,
                    tiles,
                    rng,
                    width,
                    height,
                    wrap,
                );
                herd.dwell_remaining = dwell_turns;
            }
            herd.roam = RoamState::Loiter {
                turns_left: turns_left - 1,
            };
        }
        RoamState::Migrate => {
            // Directed leg to the next anchor at 1 hex/turn, no grazing pause.
            let next_index = if herd.route.is_empty() {
                0
            } else {
                (herd.step_index + 1) % herd.route.len()
            };
            let target = herd
                .route
                .get(next_index)
                .copied()
                .unwrap_or(herd.current_pos);
            let moved = step_herd_toward(herd, target, registry, tiles, width, height, wrap);
            if herd.current_pos == target || !moved {
                // Arrived (or hemmed in) → loiter at the new anchor for a fresh window.
                herd.step_index = next_index;
                let turns = def.map(|d| d.sample_loiter_turns(rng)).unwrap_or(16);
                herd.roam = RoamState::Loiter { turns_left: turns };
                herd.dwell_remaining = 0;
            } else {
                // Heading arrow: where it will step next turn.
                herd.next_pos = best_land_neighbor_toward(
                    herd.current_pos,
                    target,
                    registry,
                    tiles,
                    width,
                    height,
                    wrap,
                );
            }
        }
    }
}

/// Step the herd one hex toward `target`, choosing the land neighbour that most reduces hex
/// distance (deterministic tie-break by direction order). Returns whether it moved (`false` = no
/// land neighbour gets closer, so it stays — avoids marching into water / off the map).
fn step_herd_toward(
    herd: &mut Herd,
    target: UVec2,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    width: u32,
    height: u32,
    wrap: bool,
) -> bool {
    if herd.current_pos == target {
        return false;
    }
    match best_land_neighbor_toward(
        herd.current_pos,
        target,
        registry,
        tiles,
        width,
        height,
        wrap,
    ) {
        Some(next) => {
            herd.current_pos = next;
            true
        }
        None => false,
    }
}

/// The land neighbour of `from` with the smallest hex distance to `target`, but only if it is
/// strictly closer than `from` (so a herd never oscillates or backtracks into water).
fn best_land_neighbor_toward(
    from: UVec2,
    target: UVec2,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    width: u32,
    height: u32,
    wrap: bool,
) -> Option<UVec2> {
    let cur_dist = hex_distance_wrapped(from, target, width, wrap);
    let mut best: Option<(UVec2, u32)> = None;
    for dir in 0..HEX_DIRECTION_COUNT {
        let Some((nx, ny)) = hex_neighbor(from.x, from.y, dir, width, height, wrap) else {
            continue;
        };
        let np = UVec2::new(nx, ny);
        if !is_land_tile(np, registry, tiles) {
            continue;
        }
        let d = hex_distance_wrapped(np, target, width, wrap);
        if d < cur_dist && best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((np, d));
        }
    }
    best.map(|(pos, _)| pos)
}

/// Nudge the herd ≤1 hex to a random land tile within `loiter_radius` of `anchor` (deterministic
/// via the seeded `rng`); if it is hemmed in, it stays.
#[allow(clippy::too_many_arguments)]
fn wander_near_anchor(
    herd: &mut Herd,
    anchor: UVec2,
    loiter_radius: u32,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
    width: u32,
    height: u32,
    wrap: bool,
) {
    let mut options: Vec<UVec2> = Vec::new();
    for dir in 0..HEX_DIRECTION_COUNT {
        let Some((nx, ny)) = hex_neighbor(
            herd.current_pos.x,
            herd.current_pos.y,
            dir,
            width,
            height,
            wrap,
        ) else {
            continue;
        };
        let np = UVec2::new(nx, ny);
        if is_land_tile(np, registry, tiles)
            && hex_distance_wrapped(np, anchor, width, wrap) <= loiter_radius
        {
            options.push(np);
        }
    }
    if options.is_empty() {
        return;
    }
    herd.current_pos = options[rng.gen_range(0..options.len())];
}

/// Per-turn immigration: with probability `immigration.chance_per_turn`, respawn one
/// short-range game group up to the abundance cap so an overhunted map slowly
/// replenishes (early forager play stays game-rich). Samples up to
/// `immigration.max_attempts` random land tiles hosting game, respecting `min_spacing`
/// from existing groups. Runs in `TurnStage::Logistics` right after `advance_herds`.
// Bevy system signature: each param is a distinct resource/query the immigration roll
// needs (registry + telemetry/density outputs, config, tick+seed for the RNG, tiles);
// they can't be collapsed without a container resource that adds no clarity.
#[allow(clippy::too_many_arguments)]
pub fn repopulate_fauna(
    mut registry: ResMut<HerdRegistry>,
    mut telemetry: ResMut<HerdTelemetry>,
    mut density: ResMut<HerdDensityMap>,
    config: Res<SimulationConfig>,
    fauna_config: Res<FaunaConfigHandle>,
    tick: Res<SimulationTick>,
    world_seed: Option<Res<WorldGenSeed>>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
) {
    let fauna = fauna_config.get();
    let imm = &fauna.immigration;
    // `max_total_game` caps short-range game groups only (matching spawn's `placed`
    // counter); migratory `herd_*` are spawned separately and don't count against it.
    let game_count = registry
        .herds
        .iter()
        .filter(|herd| herd.id.starts_with(GAME_ID_PREFIX))
        .count();
    if imm.chance_per_turn <= 0.0 || game_count >= fauna.abundance.max_total_game {
        return;
    }

    let width = config.grid_size.x.max(4);
    let height = config.grid_size.y.max(4);
    let seed = world_seed.map(|s| s.0).unwrap_or(config.map_seed);
    let mut rng = SmallRng::seed_from_u64(seed ^ tick.0 ^ IMMIGRATION_SEED_SALT);

    // Roll the per-turn immigration chance.
    if rng.gen::<f32>() >= imm.chance_per_turn {
        return;
    }

    // Ids past the initial cap + tick keep immigrants from colliding with spawn ids
    // (only one group immigrates per turn, so `tick` disambiguates across turns).
    let idx = fauna.abundance.max_total_game as u32 + tick.0 as u32;
    let min_spacing = fauna.abundance.min_spacing;
    let existing: Vec<UVec2> = registry.herds.iter().map(|herd| herd.position()).collect();

    for _ in 0..imm.max_attempts {
        let pos = UVec2::new(rng.gen_range(0..width), rng.gen_range(0..height));
        let Some(module) = module_at(pos, &tile_registry, &tiles) else {
            continue;
        };
        let module_key = module.as_str();
        if fauna.abundance.probability_for(module_key) <= 0.0 {
            continue;
        }
        if existing
            .iter()
            .any(|p| chebyshev_distance(*p, pos) < min_spacing)
        {
            continue;
        }
        if let Some(herd) = spawn_game_group_at(
            pos,
            module_key,
            idx,
            &fauna,
            width,
            height,
            &tile_registry,
            &tiles,
            &mut rng,
        ) {
            info!(
                target: "shadow_scale::analytics",
                event = "immigration",
                herd = %herd.id,
                species = %herd.species,
                x = pos.x,
                y = pos.y,
                biomass = herd.biomass,
            );
            registry.herds.push(herd);
            telemetry.entries = registry.snapshot_entries();
            density.rebuild(config.grid_size, &registry);
            return;
        }
    }
}

/// Per-turn husbandry upkeep (`TurnStage::Logistics`, after `advance_herds`): pay each
/// domesticated group's owner a steady provisions yield — proportional to biomass and
/// **without** depleting the herd (sustainable managed harvest) — and decay husbandry
/// progress on any not-yet-tamed group. Runs before the same turn's accrual in
/// `advance_fauna_pursuits` (`Population`), so a Sustain-followed group nets
/// `progress_per_turn - decay_per_turn` while an untended one only decays.
///
/// **Corral (Rung 1c).** A **corralled** herd is exempt from the mobile even-split yield here — its
/// keeper is paid place-local by the tending Hunt assignment (`advance_labor_allocation`) — and this
/// pass instead runs its **escape** check: a corralled herd tended last turn is spared; an untended
/// one clears `corralled_at` and reverts to a mobile domesticated herd. The animal mirror of
/// `forage::advance_cultivation`'s feral pass.
pub fn advance_husbandry(
    mut registry: ResMut<HerdRegistry>,
    fauna_config: Res<FaunaConfigHandle>,
    wellbeing_config: Res<WellbeingConfigHandle>,
    mut cohorts: Query<&mut PopulationCohort>,
) {
    let fauna = fauna_config.get();
    let wellbeing = wellbeing_config.get();
    let husbandry = &fauna.husbandry;
    // Accumulate each owner's managed-livestock yield, then feed it into that faction's bands'
    // larders — the pastoral counterpart of foraging income. Food is band-local from day one;
    // an even split across the owner's bands is a v1 (Phase 3 corrals will make it place-local).
    // FOOD income is fully fractional: accumulate each owner's yield as `Scalar` so a small or
    // near-cap herd whose per-turn yield is < 1 provisions still credits the larder (rounding to an
    // i64 dropped it entirely).
    let mut yields: HashMap<FactionId, Scalar> = HashMap::new();
    for herd in registry.herds.iter_mut() {
        if herd.is_domesticated() {
            // Corral (Rung 1c): a penned herd is paid its keeper **place-local** by the tending Hunt
            // assignment (`advance_labor_allocation`, at the higher `corral_provisions_per_biomass`),
            // NOT the mobile even-split below — and it **escapes** if left untended. Logistics runs
            // before Population, so the `corralled_tended_this_turn` flag read here was written last
            // turn (a one-turn lag, mirroring `ForagePatch::tended_this_turn`): a herd tended every
            // turn is always spared; a herd whose keeper leaves breaks out one turn later, reverting
            // to a mobile domesticated herd (which resumes the even-split yield next turn).
            if herd.is_corralled() {
                if herd.corralled_tended_this_turn {
                    herd.corralled_tended_this_turn = false;
                } else {
                    herd.corralled_at = None;
                    info!(
                        target: "shadow_scale::analytics",
                        event = "corral_escape",
                        herd = %herd.id,
                        faction = herd.owner.map(|f| f.0).unwrap_or_default(),
                    );
                }
                continue;
            }
            let Some(owner) = herd.owner else {
                continue;
            };
            let provisions = scalar_from_f32(herd.biomass * husbandry.provisions_per_biomass);
            if provisions > scalar_zero() {
                *yields.entry(owner).or_insert_with(scalar_zero) += provisions;
                info!(
                    target: "shadow_scale::analytics",
                    event = "husbandry_yield",
                    herd = %herd.id,
                    faction = owner.0,
                    provisions = %provisions,
                );
            }
        } else {
            herd.decay_domestication(husbandry.decay_per_turn);
        }
    }
    if yields.is_empty() {
        return;
    }
    let mut band_counts: HashMap<FactionId, u32> = HashMap::new();
    for cohort in cohorts.iter() {
        if yields.contains_key(&cohort.faction) {
            *band_counts.entry(cohort.faction).or_insert(0) += 1;
        }
    }
    for mut cohort in cohorts.iter_mut() {
        if let (Some(&total), Some(&count)) = (
            yields.get(&cohort.faction),
            band_counts.get(&cohort.faction),
        ) {
            if count > 0 {
                // Productivity modifier stack (wellbeing): a discontented band tends the herd
                // less effectively — scale its even share by its output multiplier at PAYOUT.
                let share = total / Scalar::from_u32(count);
                let mult = output_multiplier(&cohort, &wellbeing);
                cohort.stores.add(FOOD, share * mult);
            }
        }
    }
}

/// Pre-commit **yield forecast** for one worked source (a herd or a forage patch), as the client
/// needs it to show "Expected yield: +X.XX /turn" and cap its worker stepper *while the player is
/// composing an assignment* — before anything is committed (the `SourceYield` telemetry is
/// post-hoc). Every field is **provisions (food) per turn** at the source's CURRENT biomass, with
/// the caller's `output_multiplier` already folded in (the snapshot exports it at `1.0`, so the
/// client scales by the band's `outputMultiplier` — a linear factor on every field, which leaves
/// `max_useful_workers` invariant).
///
/// The consumer composes:
/// - `expected(workers, policy) = min(workers × per_worker_yield, ceiling(policy))`
/// - `max_useful_workers(policy) = ceil(ceiling(policy) / per_worker_yield)`
///
/// Each `ceiling_*` is the policy ceiling **already clamped to the source's remaining biomass**, so
/// that `min` IS the take the sim pays. **Forecast == actual is an invariant**: the forecast and
/// the take path (`hunt_take` / `forage::forage_take`) share the same ceiling + conversion helpers
/// (`hunt_policy_ceiling`/`hunt_provisions`, `forage_policy_ceiling`/`forage_provisions`) — never
/// duplicate the formulas, or the UI will lie.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SourceYieldForecast {
    /// Food/turn one worker contributes at this source (throughput → provisions), before the policy
    /// ceiling binds. `0.0` means no worker can extract anything this turn (e.g. a zero seasonal
    /// weight) — consumers must not divide by it.
    pub per_worker_yield: f32,
    /// Food/turn cap under **Sustain** (the MSY skim).
    pub ceiling_sustain: f32,
    /// Food/turn cap under **Surplus**.
    pub ceiling_surplus: f32,
    /// Food/turn cap under **Market**.
    pub ceiling_market: f32,
    /// Food/turn cap under **Eradicate**.
    pub ceiling_eradicate: f32,
}

impl SourceYieldForecast {
    /// A **tended** improvement — a corralled herd or a cultivated (tended) patch. It is maintenance
    /// labor, not scaling gather: a single worker (`TENDED_SOURCE_WORKERS_NEEDED`) collects the whole
    /// managed yield and the policy is irrelevant. So every ceiling *is* that yield and
    /// `per_worker_yield` equals it — the client's `max_useful_workers` then falls out as `1`.
    pub(crate) fn tended(yield_per_turn: f32) -> Self {
        Self {
            per_worker_yield: yield_per_turn,
            ceiling_sustain: yield_per_turn,
            ceiling_surplus: yield_per_turn,
            ceiling_market: yield_per_turn,
            ceiling_eradicate: yield_per_turn,
        }
    }
}

/// The per-policy **biomass** ceiling on a hunt take at the herd's current stock — the single source
/// of the Sustain/Surplus/Market/Eradicate rungs, shared by `systems::hunt_take` (the take path) and
/// `hunt_forecast` (the pre-commit forecast). Sustain = the Maximum Sustainable Yield (regrowth at
/// K/2, so a herd at capacity still yields and a collapsing one yields nothing), Surplus = that ×
/// `follow.surplus_multiplier`, Market = `market.take_fraction × biomass` (a commercial share),
/// Eradicate = `hunt.take_from(biomass)` (max take). Not yet clamped to biomass — callers do that
/// alongside their own throughput cap.
pub(crate) fn hunt_policy_ceiling(
    policy: FollowPolicy,
    biomass: f32,
    carrying_capacity: f32,
    fauna: &FaunaConfig,
) -> f32 {
    match policy {
        FollowPolicy::Sustain => sustainable_yield(biomass, carrying_capacity, &fauna.ecology),
        FollowPolicy::Surplus => {
            sustainable_yield(biomass, carrying_capacity, &fauna.ecology)
                * fauna.follow.surplus_multiplier
        }
        FollowPolicy::Market => fauna.market.take_fraction * biomass,
        FollowPolicy::Eradicate => fauna.hunt.take_from(biomass),
    }
}

/// Biomass → provisions for a hunt take (× the caller's productivity multiplier) — the one
/// conversion `hunt_take` pays, shared with the forecast so the two can't drift.
pub(crate) fn hunt_provisions(
    biomass_take: f32,
    fauna: &FaunaConfig,
    output_multiplier: f32,
) -> f32 {
    biomass_take * fauna.hunt.provisions_per_biomass * output_multiplier
}

/// The place-local managed yield a **corralled** herd pays its tending keeper band each turn
/// (`biomass × corral_provisions_per_biomass`, no biomass drawn down). Shared by the Hunt arm of
/// `advance_labor_allocation` (the payout) and `hunt_forecast` (the forecast).
pub(crate) fn corral_provisions(biomass: f32, fauna: &FaunaConfig, output_multiplier: f32) -> f32 {
    biomass * fauna.husbandry.corral_provisions_per_biomass * output_multiplier
}

/// Pre-commit yield forecast for hunting `herd` with `per_worker_biomass_capacity` biomass/hunter
/// (`labor_config.json` `hunt.per_worker_biomass_capacity`). Mirrors `systems::hunt_take` exactly:
/// same per-policy ceilings, same biomass clamp, same biomass→provisions conversion. A **corralled**
/// herd forecasts its corral yield with one worker (see `SourceYieldForecast::tended`). The band
/// Hunt labor has no carry limit (it passes `carry_room_biomass = f32::INFINITY` to `hunt_take`), so
/// the forecast models no carry clamp either — a hunting *expedition*'s carry cap is out of scope.
pub(crate) fn hunt_forecast(
    herd: &Herd,
    fauna: &FaunaConfig,
    per_worker_biomass_capacity: f32,
    output_multiplier: f32,
) -> SourceYieldForecast {
    if herd.is_corralled() {
        return SourceYieldForecast::tended(corral_provisions(
            herd.biomass,
            fauna,
            output_multiplier,
        ));
    }
    let ceiling = |policy| {
        hunt_provisions(
            hunt_policy_ceiling(policy, herd.biomass, herd.carrying_capacity, fauna)
                .clamp(0.0, herd.biomass),
            fauna,
            output_multiplier,
        )
    };
    SourceYieldForecast {
        per_worker_yield: hunt_provisions(
            per_worker_biomass_capacity.max(0.0),
            fauna,
            output_multiplier,
        ),
        ceiling_sustain: ceiling(FollowPolicy::Sustain),
        ceiling_surplus: ceiling(FollowPolicy::Surplus),
        ceiling_market: ceiling(FollowPolicy::Market),
        ceiling_eradicate: ceiling(FollowPolicy::Eradicate),
    }
}

/// One turn's positive logistic regrowth increment (>= 0) for a group of `biomass`
/// toward `cap`. The healthy branch of `net_biomass_delta`. Also the forage patch's
/// regrowth curve (`forage::regrow_patch`) — plants have no Allee crash, so a depleted
/// patch always recovers via this branch (see `forage.rs`).
pub(crate) fn logistic_regrowth(biomass: f32, cap: f32, regrowth_rate: f32) -> f32 {
    if cap <= 0.0 || biomass <= 0.0 {
        return 0.0;
    }
    (regrowth_rate * biomass * (1.0 - biomass / cap)).max(0.0)
}

/// Net per-turn biomass change with **critical depensation**. Above the Allee
/// threshold (`collapse_fraction * cap`) the group regrows logistically; below it the
/// group is non-viable and declines by `collapse_rate` of its biomass each turn — an
/// irreversible crash to local extinction even without further hunting (the overhunting
/// point of no return). Also sizes a Sustain/Surplus follow's take (via `.max(0.0)`):
/// a collapsing group yields no surplus.
pub(crate) fn net_biomass_delta(biomass: f32, cap: f32, ecology: &EcologyConfig) -> f32 {
    if cap <= 0.0 || biomass <= 0.0 {
        return 0.0;
    }
    let allee = ecology.collapse_fraction * cap;
    if biomass < allee {
        -(ecology.collapse_rate * biomass)
    } else {
        logistic_regrowth(biomass, cap, ecology.regrowth_rate)
    }
}

/// The most-productive biomass for logistic regrowth is K/2 (the Maximum Sustainable
/// Yield point), where `r·B·(1−B/K)` peaks.
const MSY_BIOMASS_FRACTION: f32 = 0.5;

/// Max Sustainable Yield ceiling: regrowth evaluated at the most-productive biomass (K/2),
/// so a resource AT carrying capacity still has a positive sustainable harvest (Sustain draws it
/// down to K/2 and holds it there). Below the Allee threshold this is 0 (don't harvest a
/// collapsing resource — inherited from net_biomass_delta's negative branch, clamped). Distinct
/// from net_biomass_delta, which stays the ACTUAL per-turn biomass change used by regrow_biomass.
pub(crate) fn sustainable_yield(biomass: f32, cap: f32, ecology: &EcologyConfig) -> f32 {
    net_biomass_delta(biomass.min(cap * MSY_BIOMASS_FRACTION), cap, ecology).max(0.0)
}

/// Apply one turn of critical-depensation dynamics toward the herd's carrying capacity
/// and refresh its `ecology_phase`. A sub-threshold group declines instead of regrowing;
/// the caller despawns it once it falls below the viability floor.
fn regrow_biomass(herd: &mut Herd, ecology: &EcologyConfig) {
    let cap = herd.carrying_capacity;
    // A domesticated (managed) group is immune to the overhunting collapse: it always
    // regrows logistically toward capacity and never crosses into the depensation crash.
    let delta = if herd.is_domesticated() {
        logistic_regrowth(herd.biomass, cap, ecology.regrowth_rate)
    } else {
        net_biomass_delta(herd.biomass, cap, ecology)
    };
    herd.biomass = (herd.biomass + delta).clamp(0.0, cap);
    herd.refresh_ecology_phase(ecology);
}

fn to_entry(herd: &Herd) -> HerdTelemetryEntry {
    HerdTelemetryEntry {
        id: herd.id.clone(),
        label: herd.label.clone(),
        species: herd.species.clone(),
        size_class: herd.size_class.as_str().to_string(),
        // All fauna are huntable in Phase B; Phase C/D may differentiate.
        huntable: true,
        ecology_phase: herd.ecology_phase.as_str().to_string(),
        domestication: herd.domestication_progress,
        corralled: herd.is_corralled(),
        position: herd.position(),
        biomass: herd.biomass,
        route_length: herd.route_length() as u32,
        next_position: herd.next_position(),
    }
}

fn determine_herd_count(width: u32, height: u32) -> u32 {
    let area = width.saturating_mul(height).max(1);
    let baseline = area / 3000;
    baseline.clamp(2, 6)
}

/// Long migratory route: a jittered spiral of `steps` waypoints around `origin`,
/// keeping only land tiles. Returns `None` if fewer than 3 distinct points land.
fn build_route(
    origin: UVec2,
    width: u32,
    height: u32,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
    steps: u32,
) -> Option<Vec<UVec2>> {
    let mut points = Vec::new();
    let radius = rng.gen_range(4..=12) as f32;
    let mut angle = rng.gen_range(0.0..TAU);
    for _ in 0..steps {
        let dx = angle.cos() * radius;
        let dy = angle.sin() * radius;
        angle = (angle + rng.gen_range(0.4..=1.2)) % TAU;
        let candidate = clamp_to_grid(
            origin.x as i32 + dx.round() as i32,
            origin.y as i32 + dy.round() as i32,
            width,
            height,
        );
        if let Some(pos) = candidate {
            if is_land_tile(pos, registry, tiles) && points.last().copied() != Some(pos) {
                points.push(pos);
            }
        }
    }
    if points.len() < 3 {
        None
    } else {
        Some(points)
    }
}

/// Short roaming route for wild game: `steps` waypoints within a small radius of
/// `origin` (radius grows with route length). `steps == 1` yields a single-tile,
/// stationary group (which the client draws with no trail). Returns `None` only if
/// `origin` itself is not land.
fn build_short_route(
    origin: UVec2,
    steps: u32,
    width: u32,
    height: u32,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
) -> Option<Vec<UVec2>> {
    if !is_land_tile(origin, registry, tiles) {
        return None;
    }
    let mut points = vec![origin];
    let target = steps.max(1) as usize;
    if target <= 1 {
        return Some(points);
    }
    // Wander radius scales with route length (big game ~2-3 tiles, small ~1).
    let radius = target.saturating_sub(1).max(1) as i32;
    let max_attempts = target * 4;
    let mut attempts = 0;
    while points.len() < target && attempts < max_attempts {
        attempts += 1;
        let dx = rng.gen_range(-radius..=radius);
        let dy = rng.gen_range(-radius..=radius);
        let Some(pos) = clamp_to_grid(origin.x as i32 + dx, origin.y as i32 + dy, width, height)
        else {
            continue;
        };
        if is_land_tile(pos, registry, tiles) && !points.contains(&pos) {
            points.push(pos);
        }
    }
    Some(points)
}

/// Food module for a tile position, or `None` for water / unclassified tiles.
fn module_at(position: UVec2, registry: &TileRegistry, tiles: &Query<&Tile>) -> Option<FoodModule> {
    let entity = registry.index(position.x, position.y)?;
    let tile = tiles.get(entity).ok()?;
    classify_food_module(tile)
}

fn chebyshev_distance(a: UVec2, b: UVec2) -> u32 {
    let dx = a.x.abs_diff(b.x);
    let dy = a.y.abs_diff(b.y);
    dx.max(dy)
}

fn clamp_to_grid(x: i32, y: i32, width: u32, height: u32) -> Option<UVec2> {
    let max_x = width as i32 - 1;
    let max_y = height as i32 - 1;
    if max_x < 0 || max_y < 0 {
        return None;
    }
    let clamped_x = x.clamp(0, max_x) as u32;
    let clamped_y = y.clamp(0, max_y) as u32;
    Some(UVec2::new(clamped_x, clamped_y))
}

fn is_land_tile(position: UVec2, registry: &TileRegistry, tiles: &Query<&Tile>) -> bool {
    registry
        .index(position.x, position.y)
        .and_then(|entity| tiles.get(entity).ok())
        .map(|tile| !tile.terrain_tags.contains(TerrainTags::WATER))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecology_phase_string_roundtrips() {
        for phase in [
            EcologyPhase::Thriving,
            EcologyPhase::Stressed,
            EcologyPhase::Collapsing,
        ] {
            assert_eq!(EcologyPhase::from_key(phase.as_str()), phase);
        }
    }

    #[test]
    fn ecology_phase_from_unknown_key_defaults_thriving() {
        assert_eq!(EcologyPhase::from_key(""), EcologyPhase::Thriving);
        assert_eq!(EcologyPhase::from_key("bogus"), EcologyPhase::Thriving);
    }

    #[test]
    fn roam_state_string_roundtrips() {
        for roam in [
            RoamState::GrazeWander,
            RoamState::Loiter { turns_left: 7 },
            RoamState::Migrate,
        ] {
            let restored = RoamState::from_mode(roam.mode_key(), roam.loiter_turns_left());
            assert_eq!(restored, roam);
        }
    }

    #[test]
    fn size_class_string_roundtrips() {
        for size in [SizeClass::Small, SizeClass::Big, SizeClass::Migratory] {
            assert_eq!(SizeClass::from_key(size.as_str()), size);
        }
    }
}
