use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::{Res, ResMut, Resource};
use serde::Deserialize;
use thiserror::Error;

use crate::{
    crisis::CrisisMetricKind, metrics::SimulationMetrics, orders::FactionId, SimulationTick,
};

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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash, Default)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Default)]
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

#[derive(Resource, Debug, Clone)]
pub struct VictoryState {
    pub modes: Vec<VictoryModeState>,
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
            modes: Vec::new(),
            winner: None,
            continue_after_win,
        }
    }
}

#[derive(Debug, Clone)]
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
            threshold: def.threshold.max(0.0001),
            achieved: false,
        }
    }
}

#[derive(Debug, Clone)]
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

pub fn load_victory_config_from_env() -> Arc<VictoryConfig> {
    let override_path = env::var("VICTORY_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/victory_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match read_victory_config_from_file(&path) {
            Ok(config) => {
                return Arc::new(config);
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::victory",
                    path = %path.display(),
                    error = %err,
                    "victory_config.load_failed"
                );
            }
        }
    }

    let config = read_victory_config_from_str(BUILTIN_VICTORY_CONFIG)
        .expect("builtin victory config should parse");
    Arc::new(config)
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

pub fn victory_tick(
    config: Res<VictoryConfigHandle>,
    metrics: Res<SimulationMetrics>,
    mut state: ResMut<VictoryState>,
    tick: Res<SimulationTick>,
) {
    let cfg = config.get();
    state.continue_after_win = cfg.continue_after_win;
    if state.winner.is_some() && !state.continue_after_win {
        return;
    }
    let mut next_modes = Vec::with_capacity(cfg.modes.len());

    for def in cfg.modes.iter().filter(|mode| mode.enabled) {
        let mut entry = state
            .modes
            .iter()
            .find(|mode| mode.id == def.id)
            .cloned()
            .unwrap_or_else(|| VictoryModeState::from_definition(def));

        entry.threshold = def.threshold.max(0.0001);

        let evaluated = evaluate_mode_progress(&entry, def, &metrics);
        entry.progress = evaluated.clamp(0.0, entry.threshold);
        entry.achieved = entry.progress >= entry.threshold;

        if entry.achieved && state.winner.is_none() {
            state.winner = Some(VictoryResult {
                mode: entry.id.clone(),
                faction: FactionId(0),
                tick: tick.0,
            });
            tracing::info!(
                target: "shadow_scale::victory",
                mode = %entry.id.0,
                kind = %entry.kind.as_str(),
                tick = tick.0,
                "victory.mode.achieved"
            );
            tracing::info!(
                target: "shadow_scale::campaign",
                mode = %entry.id.0,
                kind = %entry.kind.as_str(),
                tick = tick.0,
                "campaign.victory"
            );
            tracing::info!(
                target: "shadow_scale::analytics",
                event = "victory",
                mode = %entry.id.0,
                kind = %entry.kind.as_str(),
                tick = tick.0,
                faction = 0,
                "analytics.victory"
            );
        }

        next_modes.push(entry);
    }

    state.modes = next_modes;
}

fn evaluate_mode_progress(
    entry: &VictoryModeState,
    def: &VictoryModeDefinition,
    metrics: &SimulationMetrics,
) -> f32 {
    const HEGEMONY_POP_TARGET: f32 = 5_000.0;
    const ASCENSION_DISCOVERY_TARGET: f32 = 3.0;
    const RAMP_LEN: f32 = 12.0;
    let normalized_turn = (metrics.turn as f32).max(1.0);
    let smoothing = 0.65;

    let candidate = match def.kind {
        VictoryModeKind::Hegemony => {
            let pop_score = (metrics.population_total as f32 / HEGEMONY_POP_TARGET).clamp(0.0, 1.5);
            let morale = metrics.population_morale_avg.clamp(0.0, 1.0);
            let grid_relief = (1.0 - metrics.grid_stress_avg).clamp(0.0, 1.0);
            let logistics = metrics.logistics_flow_avg.clamp(0.0, 1.0);
            let surplus = (metrics.grid_surplus_margin + 0.5).clamp(0.0, 1.0);
            0.45 * pop_score + 0.25 * morale + 0.2 * grid_relief + 0.1 * (logistics + surplus) / 2.0
        }
        VictoryModeKind::Ascension => {
            let discovery_score = (metrics.great_discoveries_total as f32
                / ASCENSION_DISCOVERY_TARGET)
                .clamp(0.0, 1.5);
            let morale = metrics.population_morale_avg.clamp(0.0, 1.0);
            0.65 * discovery_score + 0.35 * morale
        }
        VictoryModeKind::Economic => {
            let trade = metrics.trade_openness_avg.clamp(0.0, 1.0);
            let surplus = (metrics.grid_surplus_margin + 0.5).clamp(0.0, 1.25);
            let logistics = metrics.logistics_flow_avg.clamp(0.0, 1.0);
            0.5 * trade + 0.3 * surplus + 0.2 * logistics
        }
        VictoryModeKind::Diplomatic => {
            let trade = metrics.trade_openness_avg.clamp(0.0, 1.0);
            let morale = metrics.population_morale_avg.clamp(0.0, 1.0);
            let turn_bonus = (normalized_turn / RAMP_LEN).clamp(0.0, 1.0);
            0.5 * trade + 0.3 * morale + 0.2 * turn_bonus
        }
        VictoryModeKind::Stewardship => {
            let grid_relief = metrics
                .crisis
                .gauge(CrisisMetricKind::GridStressPct)
                .map(|g| (1.0 - g.raw).clamp(0.0, 1.0))
                .unwrap_or(1.0);
            let morale = metrics.population_morale_avg.clamp(0.0, 1.0);
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
    use bevy::prelude::World;
    use bevy_ecs::system::RunSystemOnce;
    use std::sync::Arc;

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

    #[test]
    fn victory_tick_sets_winner_for_hegemony() {
        let mut world = World::default();
        world.insert_resource(config_with_mode(hegemony_definition(), true));
        world.insert_resource(SimulationMetrics {
            population_total: 10_000,
            population_morale_avg: 0.9,
            grid_stress_avg: 0.1,
            grid_surplus_margin: 0.4,
            logistics_flow_avg: 0.8,
            ..Default::default()
        });
        world.insert_resource(VictoryState::new(true));
        world.insert_resource(SimulationTick(12));
        world.run_system_once(victory_tick);
        let state = world.resource::<VictoryState>();
        assert!(state.winner.is_some());
        assert_eq!(state.winner.as_ref().unwrap().mode.0, "test_heg");
    }

    #[test]
    fn victory_tick_halts_when_continue_disabled() {
        let mut world = World::default();
        world.insert_resource(config_with_mode(hegemony_definition(), false));
        world.insert_resource(SimulationMetrics {
            population_total: 10_000,
            population_morale_avg: 0.9,
            grid_stress_avg: 0.05,
            grid_surplus_margin: 0.5,
            logistics_flow_avg: 0.9,
            ..Default::default()
        });
        world.insert_resource(VictoryState::new(false));
        world.insert_resource(SimulationTick(8));
        world.run_system_once(victory_tick);
        {
            let state = world.resource::<VictoryState>();
            assert!(state.winner.is_some());
            assert_eq!(state.winner.as_ref().unwrap().tick, 8);
        }
        // Push metrics to zero and advance tick; with continue disabled nothing should change.
        {
            let mut metrics = world.resource_mut::<SimulationMetrics>();
            metrics.population_total = 0;
            metrics.population_morale_avg = 0.1;
            metrics.grid_stress_avg = 0.9;
            metrics.grid_surplus_margin = -0.5;
            metrics.logistics_flow_avg = 0.0;
        }
        world.insert_resource(SimulationTick(99));
        world.run_system_once(victory_tick);
        let state = world.resource::<VictoryState>();
        assert_eq!(state.winner.as_ref().unwrap().tick, 8);
    }
}
