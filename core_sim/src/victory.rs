use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::{Res, ResMut, Resource};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::{
    crisis::CrisisMetricKind,
    metrics::SimulationMetrics,
    orders::{FactionId, FactionRegistry},
    SimulationTick,
};

/// **The smallest threshold a mode may be scored against.** A configured `0.0` would make every
/// mode achieved on the turn it appears (progress `>= 0` always), and dividing a normalized score by
/// it would be a division by zero — so a mode that asks for nothing is scored as asking for
/// *almost* nothing instead of being unwinnable-by-being-instantly-won.
const MIN_THRESHOLD: f32 = 0.0001;

pub const BUILTIN_VICTORY_CONFIG: &str = include_str!("data/victory_config.json");

#[derive(Debug, Clone, Deserialize)]
pub struct VictoryConfigFile {
    #[serde(default = "default_continue_after_win")]
    pub continue_after_win: bool,
    pub modes: Vec<VictoryModeDefinition>,
}

fn default_continue_after_win() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VictoryModeKind {
    #[default]
    Hegemony,
    Ascension,
    Economic,
    Diplomatic,
    Stewardship,
    Survival,
}

impl VictoryModeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            VictoryModeKind::Hegemony => "hegemony",
            VictoryModeKind::Ascension => "ascension",
            VictoryModeKind::Economic => "economic",
            VictoryModeKind::Diplomatic => "diplomatic",
            VictoryModeKind::Stewardship => "stewardship",
            VictoryModeKind::Survival => "survival",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct VictoryModeDefinition {
    pub id: VictoryModeId,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub threshold: f32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub kind: VictoryModeKind,
    #[serde(default)]
    pub requires_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Default, Serialize)]
#[serde(transparent)]
pub struct VictoryModeId(pub String);

#[derive(Resource, Debug, Clone)]
pub struct VictoryConfigHandle(Arc<VictoryConfig>);

impl VictoryConfigHandle {
    pub fn new(config: Arc<VictoryConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<VictoryConfig> {
        self.0.clone()
    }
}

#[derive(Debug, Clone)]
pub struct VictoryConfig {
    pub modes: Vec<VictoryModeDefinition>,
    pub continue_after_win: bool,
}

impl VictoryConfig {
    /// The compiled-in copy of `data/victory_config.json` — the only config the boot loader is
    /// allowed to substitute, and only when that file is absent entirely.
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            read_victory_config_from_str(BUILTIN_VICTORY_CONFIG)
                .expect("builtin victory config should parse"),
        )
    }
}

/// **Where each faction stands, and who has won.**
///
/// # ⛔ PROGRESS IS PER FACTION
///
/// A victory threshold is a claim about **one people** — "control a dominant share of population",
/// "keep your people alive" — so there is one row set per registered faction, evaluated against that
/// faction's own metrics. It used to be a single list scored from the world's totals, which on a
/// map with rivals meant their people counted toward your hegemony and their misery dragged your
/// morale score, and the winner was recorded as `FactionId(0)` whoever actually crossed the line.
///
/// The published frame carries **the viewer's rows only** (`snapshot/campaign.rs`); the winner is
/// public, because a winner is public by definition.
#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct VictoryState {
    /// Per faction, in id order (`BTreeMap`, so a checkpoint encodes byte-reproducibly).
    pub modes: BTreeMap<FactionId, Vec<VictoryModeState>>,
    pub winner: Option<VictoryResult>,
    pub continue_after_win: bool,
}

impl Default for VictoryState {
    fn default() -> Self {
        Self::new(true)
    }
}

impl VictoryState {
    pub fn new(continue_after_win: bool) -> Self {
        Self {
            modes: BTreeMap::new(),
            winner: None,
            continue_after_win,
        }
    }

    /// One faction's mode rows — empty for a faction the last evaluation did not cover, which is
    /// every faction before the first turn resolves.
    pub fn modes_for(&self, faction: FactionId) -> &[VictoryModeState] {
        self.modes
            .get(&faction)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VictoryModeState {
    pub id: VictoryModeId,
    pub kind: VictoryModeKind,
    pub progress: f32,
    pub threshold: f32,
    pub achieved: bool,
}

impl Default for VictoryModeState {
    fn default() -> Self {
        Self {
            id: VictoryModeId::default(),
            kind: VictoryModeKind::Hegemony,
            progress: 0.0,
            threshold: 1.0,
            achieved: false,
        }
    }
}

impl VictoryModeState {
    fn from_definition(def: &VictoryModeDefinition) -> Self {
        Self {
            id: def.id.clone(),
            kind: def.kind.clone(),
            progress: 0.0,
            threshold: def.threshold.max(MIN_THRESHOLD),
            achieved: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VictoryResult {
    pub mode: VictoryModeId,
    pub faction: FactionId,
    pub tick: u64,
}

impl Default for VictoryResult {
    fn default() -> Self {
        Self {
            mode: VictoryModeId::default(),
            faction: FactionId(0),
            tick: 0,
        }
    }
}

#[derive(Debug, Error)]
pub enum VictoryConfigError {
    #[error("failed to parse victory config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read victory config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl ConfigLoadError for VictoryConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `VICTORY_CONFIG_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_victory_config_from_env() -> Arc<VictoryConfig> {
    let (config, _source) = load_config_from_env(
        "VICTORY_CONFIG_PATH",
        "victory_config",
        "src/data/victory_config.json",
        VictoryConfig::builtin,
        read_victory_config_from_file,
    );
    config
}

fn read_victory_config_from_file(path: &Path) -> Result<VictoryConfig, VictoryConfigError> {
    let contents = fs::read_to_string(path).map_err(|source| VictoryConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    read_victory_config_from_str(&contents)
}

fn read_victory_config_from_str(data: &str) -> Result<VictoryConfig, VictoryConfigError> {
    let file: VictoryConfigFile = serde_json::from_str(data)?;
    Ok(VictoryConfig {
        modes: file.modes,
        continue_after_win: file.continue_after_win,
    })
}

/// **Score every registered faction against its own metrics, and record who crossed a line.**
///
/// ⛔ **The walk order is load-bearing and therefore explicit.** Factions in **registry id order**,
/// modes in **config order**, and the first achiever met takes the win — so two peoples crossing on
/// the same tick break to the **lower id**, and a replay of the same world always names the same
/// winner. `FactionRegistry::factions()` is id-ordered by construction; the loop states the
/// dependency rather than inheriting it silently.
pub fn victory_tick(
    config: Res<VictoryConfigHandle>,
    metrics: Res<SimulationMetrics>,
    registry: Res<FactionRegistry>,
    mut state: ResMut<VictoryState>,
    tick: Res<SimulationTick>,
) {
    let cfg = config.get();
    state.continue_after_win = cfg.continue_after_win;
    if state.winner.is_some() && !state.continue_after_win {
        return;
    }
    let mut next_modes: BTreeMap<FactionId, Vec<VictoryModeState>> = BTreeMap::new();

    for &faction in registry.factions() {
        let mut faction_modes = Vec::with_capacity(cfg.modes.len());
        for def in cfg.modes.iter().filter(|mode| mode.enabled) {
            let (mut entry, existed) = match state
                .modes_for(faction)
                .iter()
                .find(|mode| mode.id == def.id)
            {
                Some(mode) => (mode.clone(), true),
                None => (VictoryModeState::from_definition(def), false),
            };

            entry.threshold = def.threshold.max(MIN_THRESHOLD);

            let evaluated = evaluate_mode_progress(&entry, def, &metrics, faction, !existed);
            entry.progress = evaluated.clamp(0.0, entry.threshold);
            entry.achieved = entry.progress >= entry.threshold;

            if entry.achieved && state.winner.is_none() {
                state.winner = Some(VictoryResult {
                    mode: entry.id.clone(),
                    faction,
                    tick: tick.0,
                });
                tracing::info!(
                    target: "shadow_scale::victory",
                    mode = %entry.id.0,
                    kind = %entry.kind.as_str(),
                    tick = tick.0,
                    faction = faction.0,
                    "victory.mode.achieved"
                );
                tracing::info!(
                    target: "shadow_scale::campaign",
                    mode = %entry.id.0,
                    kind = %entry.kind.as_str(),
                    tick = tick.0,
                    faction = faction.0,
                    "campaign.victory"
                );
                tracing::info!(
                    target: "shadow_scale::analytics",
                    event = "victory",
                    mode = %entry.id.0,
                    kind = %entry.kind.as_str(),
                    tick = tick.0,
                    faction = faction.0,
                    "analytics.victory"
                );
            }

            faction_modes.push(entry);
        }
        next_modes.insert(faction, faction_modes);
    }

    state.modes = next_modes;
}

/// **One faction's score for one mode.**
///
/// # Which inputs are scoped to the faction, and which are honestly world-level
///
/// | Term | Scope | Why |
/// |---|---|---|
/// | population, morale | **faction** | how many people *you* have and how they feel — the whole subject of a hegemony or survival claim |
/// | great discoveries | **faction** | a rival's breakthrough is not your ascension; counted off the same ledger the world total counts |
/// | grid stress, surplus margin | **world** | `PowerGridState` is one grid for the map and carries no faction; there is no per-faction figure to scope to |
/// | the crisis gauges (`GridStressPct`, `R0`) | **world** | a crisis is an event on the map, and `ActiveCrisisLedger` is not keyed by faction |
/// | the turn number | **world** | it is the clock |
///
/// **A world-level input is still measured PER FACTION**: two peoples can live through the same
/// plague, and the achiever is whoever meets the bar while doing so. What changed is never the
/// input's scope, only whose threshold it is scored against.
fn evaluate_mode_progress(
    entry: &VictoryModeState,
    def: &VictoryModeDefinition,
    metrics: &SimulationMetrics,
    faction: FactionId,
    fresh: bool,
) -> f32 {
    const HEGEMONY_POP_TARGET: f32 = 5_000.0;
    const ASCENSION_DISCOVERY_TARGET: f32 = 3.0;
    const RAMP_LEN: f32 = 12.0;
    let normalized_turn = (metrics.turn as f32).max(1.0);
    let smoothing = if fresh { 0.0 } else { 0.65 };
    let own = metrics.for_faction(faction);

    let candidate = match def.kind {
        VictoryModeKind::Hegemony => {
            let pop_score = (own.population_total as f32 / HEGEMONY_POP_TARGET).clamp(0.0, 1.5);
            let morale = own.population_morale_avg.clamp(0.0, 1.0);
            let grid_relief = (1.0 - metrics.grid_stress_avg).clamp(0.0, 1.0);
            // The production term used to average `logistics_flow_avg` with the power surplus.
            // The logistics metric counted the tile-pair mass network, which was demolished with
            // the rest of the dead trade slice (`docs/plan_contact_and_logistics.md` §As-built);
            // the term's weight is unchanged and now rests on the surplus alone.
            let surplus = (metrics.grid_surplus_margin + 0.5).clamp(0.0, 1.0);
            0.45 * pop_score + 0.25 * morale + 0.2 * grid_relief + 0.1 * surplus
        }
        VictoryModeKind::Ascension => {
            let discovery_score = (metrics.great_discoveries_for(faction) as f32
                / ASCENSION_DISCOVERY_TARGET)
                .clamp(0.0, 1.5);
            let morale = own.population_morale_avg.clamp(0.0, 1.0);
            0.65 * discovery_score + 0.35 * morale
        }
        // Both of the two modes below were scored mostly on `trade_openness_avg`, which averaged
        // a `TradeLink` set nothing ever populated and so read 0.0 for the whole life of the band
        // game. The metric is gone with the rest of the dead trade slice
        // (`docs/plan_contact_and_logistics.md` §As-built) and each mode's remaining terms are
        // renormalized to sum to 1. Neither mode is enabled in `victory_config.json`; they get a
        // real economy again when the contact/logistics substrate lands one.
        VictoryModeKind::Economic => (metrics.grid_surplus_margin + 0.5).clamp(0.0, 1.25),
        VictoryModeKind::Diplomatic => {
            let morale = own.population_morale_avg.clamp(0.0, 1.0);
            let turn_bonus = (normalized_turn / RAMP_LEN).clamp(0.0, 1.0);
            0.6 * morale + 0.4 * turn_bonus
        }
        VictoryModeKind::Stewardship => {
            let grid_relief = metrics
                .crisis
                .gauge(CrisisMetricKind::GridStressPct)
                .map(|g| (1.0 - g.raw).clamp(0.0, 1.0))
                .unwrap_or(1.0);
            let morale = own.population_morale_avg.clamp(0.0, 1.0);
            0.7 * grid_relief + 0.3 * morale
        }
        VictoryModeKind::Survival => {
            let disease = metrics
                .crisis
                .gauge(CrisisMetricKind::R0)
                .map(|g| (1.0 - g.raw).clamp(0.0, 1.0))
                .unwrap_or(1.0);
            let turn_bonus = (normalized_turn / (RAMP_LEN * 1.5)).clamp(0.0, 1.0);
            0.7 * disease + 0.3 * turn_bonus
        }
    };

    let safe_candidate = if candidate.is_finite() {
        candidate.max(0.0)
    } else {
        0.0
    };

    smoothing * entry.progress + (1.0 - smoothing) * safe_candidate * def.threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::FactionMetrics;
    use bevy::prelude::World;
    use bevy_ecs::system::RunSystemOnce;
    use std::sync::Arc;

    const HOME: FactionId = FactionId(0);
    const RIVAL: FactionId = FactionId(1);

    fn hegemony_definition() -> VictoryModeDefinition {
        VictoryModeDefinition {
            id: VictoryModeId("test_heg".to_string()),
            enabled: true,
            threshold: 1.0,
            description: None,
            kind: VictoryModeKind::Hegemony,
            requires_capabilities: Vec::new(),
        }
    }

    fn config_with_mode(mode: VictoryModeDefinition, continue_after: bool) -> VictoryConfigHandle {
        VictoryConfigHandle::new(Arc::new(VictoryConfig {
            modes: vec![mode],
            continue_after_win: continue_after,
        }))
    }

    /// Metrics carrying both halves the way `collect_metrics` writes them: a per-faction row each,
    /// and the world totals as their true aggregate.
    ///
    /// **The world totals are populated on purpose.** Zeroing them would make a mode that still read
    /// the world figure score everyone at nothing — and *pass* the "a rival's people are not yours"
    /// assertion for the wrong reason. Aggregating them is what makes that test fail when the
    /// per-faction read is sabotaged back to the world one.
    fn metrics_for(rows: &[(FactionId, u64, f32)]) -> SimulationMetrics {
        let population_total = rows.iter().map(|&(_, size, _)| size).sum();
        let population_morale_avg = if rows.is_empty() {
            0.0
        } else {
            rows.iter().map(|&(_, _, morale)| morale).sum::<f32>() / rows.len() as f32
        };
        SimulationMetrics {
            population_by_faction: rows
                .iter()
                .map(|&(faction, population_total, population_morale_avg)| {
                    (
                        faction,
                        FactionMetrics {
                            population_total,
                            population_morale_avg,
                        },
                    )
                })
                .collect(),
            population_total,
            population_morale_avg,
            grid_stress_avg: 0.1,
            grid_surplus_margin: 0.4,
            ..Default::default()
        }
    }

    fn world_with(
        registry: FactionRegistry,
        metrics: SimulationMetrics,
        config: VictoryConfigHandle,
        continue_after: bool,
        tick: u64,
    ) -> World {
        let mut world = World::default();
        world.insert_resource(config);
        world.insert_resource(metrics);
        world.insert_resource(registry);
        world.insert_resource(VictoryState::new(continue_after));
        world.insert_resource(SimulationTick(tick));
        world
    }

    fn progress_of(world: &World, faction: FactionId) -> f32 {
        world
            .resource::<VictoryState>()
            .modes_for(faction)
            .first()
            .expect("the faction was evaluated")
            .progress
    }

    #[test]
    fn victory_tick_sets_winner_for_hegemony() {
        let mut world = world_with(
            FactionRegistry::default(),
            metrics_for(&[(HOME, 10_000, 0.9)]),
            config_with_mode(hegemony_definition(), true),
            true,
            12,
        );
        world.run_system_once(victory_tick);
        let state = world.resource::<VictoryState>();
        assert!(state.winner.is_some());
        assert_eq!(state.winner.as_ref().unwrap().mode.0, "test_heg");
        assert_eq!(
            state.winner.as_ref().unwrap().faction,
            HOME,
            "the only faction on the map is the one that won"
        );
    }

    #[test]
    fn victory_tick_halts_when_continue_disabled() {
        let mut world = world_with(
            FactionRegistry::default(),
            metrics_for(&[(HOME, 10_000, 0.9)]),
            config_with_mode(hegemony_definition(), false),
            false,
            8,
        );
        world.run_system_once(victory_tick);
        {
            let state = world.resource::<VictoryState>();
            assert!(state.winner.is_some());
            assert_eq!(state.winner.as_ref().unwrap().tick, 8);
        }
        // Push metrics to zero and advance tick; with continue disabled nothing should change.
        world.insert_resource(metrics_for(&[(HOME, 0, 0.1)]));
        world.insert_resource(SimulationTick(99));
        world.run_system_once(victory_tick);
        let state = world.resource::<VictoryState>();
        assert_eq!(state.winner.as_ref().unwrap().tick, 8);
    }

    /// ⛔ **A RIVAL'S PEOPLE ARE NOT YOURS.** The home faction's score is identical whether or not a
    /// vastly larger rival stands on the same map — the defect this arc fixed was `population_total`
    /// summed over every cohort, which made a rival's growth advance your hegemony.
    ///
    /// Sabotage: score hegemony off `metrics.population_total` again and the two-faction arm's home
    /// progress jumps, because the world sum is a hundred times the home figure.
    #[test]
    fn a_rivals_population_does_not_move_your_progress() {
        let alone = {
            let mut world = world_with(
                FactionRegistry::with_ai_factions(0),
                metrics_for(&[(HOME, 1_000, 0.5)]),
                config_with_mode(hegemony_definition(), true),
                true,
                4,
            );
            world.run_system_once(victory_tick);
            progress_of(&world, HOME)
        };

        let mut world = world_with(
            FactionRegistry::with_ai_factions(1),
            metrics_for(&[(HOME, 1_000, 0.5), (RIVAL, 100_000, 0.9)]),
            config_with_mode(hegemony_definition(), true),
            true,
            4,
        );
        world.run_system_once(victory_tick);

        assert_eq!(
            progress_of(&world, HOME),
            alone,
            "a neighbour with a hundred times your people must not move your own progress"
        );
        assert!(
            progress_of(&world, RIVAL) > progress_of(&world, HOME),
            "and the rival's own row is the one that carries their people: {:?}",
            world.resource::<VictoryState>().modes
        );
    }

    /// **The winner is whoever crossed the line, and a rival crossing it is reported as the rival.**
    /// Before this, `faction` was hard-coded `FactionId(0)`, so a rival's win was recorded as the
    /// player's — a wrong end screen rather than a missing one.
    #[test]
    fn a_rival_that_meets_the_bar_is_recorded_as_the_winner() {
        let mut world = world_with(
            FactionRegistry::with_ai_factions(1),
            // The home faction is nowhere near the bar; the rival is far past it.
            metrics_for(&[(HOME, 10, 0.1), (RIVAL, 100_000, 0.95)]),
            config_with_mode(hegemony_definition(), true),
            true,
            21,
        );
        world.run_system_once(victory_tick);

        let state = world.resource::<VictoryState>();
        let winner = state.winner.as_ref().expect("somebody won");
        assert_eq!(winner.faction, RIVAL, "the achiever is the winner");
        assert_eq!(winner.tick, 21);
        assert!(
            !state.modes_for(HOME)[0].achieved,
            "and the player, who achieved nothing, is not recorded as having won"
        );
    }

    /// **Ties break by the lowest id**, and the order is the registry's. Both factions cross on the
    /// same tick with identical metrics, so only the walk order can decide — which is exactly the
    /// determinism the replay suites depend on.
    #[test]
    fn two_factions_crossing_on_one_tick_break_to_the_lowest_id() {
        let mut world = world_with(
            FactionRegistry::with_ai_factions(1),
            metrics_for(&[(HOME, 10_000, 0.9), (RIVAL, 10_000, 0.9)]),
            config_with_mode(hegemony_definition(), true),
            true,
            5,
        );
        world.run_system_once(victory_tick);
        let state = world.resource::<VictoryState>();
        assert!(state.modes_for(HOME)[0].achieved && state.modes_for(RIVAL)[0].achieved);
        assert_eq!(state.winner.as_ref().unwrap().faction, HOME);
    }

    /// **A single-faction world scores exactly what it always scored.** The weights are pinned as a
    /// number rather than re-derived, so a change to the formula — or to which figures feed it —
    /// fails here instead of quietly re-balancing every campaign.
    ///
    /// `0.45·(1000/5000) + 0.25·0.50 + 0.20·(1−0.50) + 0.10·(0.00+0.50) = 0.365`.
    #[test]
    fn a_one_faction_world_scores_exactly_what_it_always_did() {
        let mut metrics = metrics_for(&[(HOME, 1_000, 0.5)]);
        metrics.grid_stress_avg = 0.5;
        metrics.grid_surplus_margin = 0.0;
        let mut world = world_with(
            FactionRegistry::default(),
            metrics,
            config_with_mode(hegemony_definition(), true),
            true,
            1,
        );
        world.run_system_once(victory_tick);
        let progress = progress_of(&world, HOME);
        assert!(
            (progress - 0.365).abs() < 1e-5,
            "the shipped hegemony weights scored 0.365 for these inputs, got {progress}"
        );
    }

    /// A world-level input is still measured per faction: two peoples living through the same grid
    /// stress are each scored against their own threshold, and the shared term reaches both rows.
    #[test]
    fn a_world_level_input_reaches_every_factions_row() {
        let easy = {
            let mut metrics = metrics_for(&[(HOME, 1_000, 0.5), (RIVAL, 1_000, 0.5)]);
            metrics.grid_stress_avg = 0.0;
            let mut world = world_with(
                FactionRegistry::with_ai_factions(1),
                metrics,
                config_with_mode(hegemony_definition(), true),
                true,
                3,
            );
            world.run_system_once(victory_tick);
            (progress_of(&world, HOME), progress_of(&world, RIVAL))
        };
        let mut metrics = metrics_for(&[(HOME, 1_000, 0.5), (RIVAL, 1_000, 0.5)]);
        metrics.grid_stress_avg = 1.0;
        let mut world = world_with(
            FactionRegistry::with_ai_factions(1),
            metrics,
            config_with_mode(hegemony_definition(), true),
            true,
            3,
        );
        world.run_system_once(victory_tick);
        assert!(progress_of(&world, HOME) < easy.0);
        assert!(progress_of(&world, RIVAL) < easy.1);
        assert_eq!(
            progress_of(&world, HOME),
            progress_of(&world, RIVAL),
            "with identical people, a shared crisis leaves the two rows identical"
        );
    }

    /// A faction with no cohorts left has no progress rather than no row — the answer
    /// `for_faction`'s zeros give, and the one a survival mode has to be able to state.
    #[test]
    fn a_faction_with_nobody_left_scores_its_population_at_nothing() {
        let mut world = world_with(
            FactionRegistry::with_ai_factions(1),
            // Morale is zero on BOTH sides, so population is the only term that can separate the
            // two rows — otherwise this would pass on the morale difference alone.
            metrics_for(&[(HOME, 5_000, 0.0)]),
            config_with_mode(hegemony_definition(), true),
            true,
            9,
        );
        world.run_system_once(victory_tick);
        let state = world.resource::<VictoryState>();
        assert_eq!(
            state.modes_for(RIVAL).len(),
            1,
            "a wiped-out faction is still evaluated, so its row exists"
        );
        assert!(
            progress_of(&world, RIVAL) < progress_of(&world, HOME),
            "and it scores its missing people as nothing"
        );
    }
}
