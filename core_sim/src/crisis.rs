use std::{
    collections::{hash_map::DefaultHasher, HashMap, VecDeque},
    f32::consts::PI,
    hash::{Hash, Hasher},
};

use bevy::prelude::*;
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};
use serde::Deserialize;
use tracing::{info, warn};

use crate::{
    crisis_config::{
        CrisisArchetype, CrisisArchetypeCatalog, CrisisArchetypeCatalogHandle, CrisisModifier,
        CrisisModifierCatalog, CrisisModifierCatalogHandle, CrisisTelemetryConfig,
        CrisisTelemetryConfigHandle, CrisisTelemetryThreshold,
    },
    fauna::HerdDensityMap,
    orders::FactionId,
    resources::{PendingCrisisSeeds, PendingCrisisSpawns, SimulationConfig, SimulationTick},
    scalar::Scalar,
};
use sim_runtime::{
    CrisisOverlayAnnotationState, CrisisSeverityBand as SchemaCrisisSeverityBand, ScalarRasterState,
};

const MIN_GRID_DIMENSION: u32 = 1;
const HERD_DENSITY_CRISIS_WEIGHT: f32 = 0.35;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrisisSeverityBand {
    Safe,
    Warn,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrisisMetricKind {
    R0,
    GridStressPct,
    UnauthorizedQueuePct,
    SwarmsActive,
    PhageDensity,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrisisTrendSample {
    pub tick: u64,
    pub value: f32,
}

impl Default for CrisisTrendSample {
    fn default() -> Self {
        Self {
            tick: 0,
            value: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CrisisGaugeSnapshot {
    pub kind: CrisisMetricKind,
    pub raw: f32,
    pub ema: f32,
    pub trend_5t: f32,
    pub band: CrisisSeverityBand,
    pub last_updated_tick: u64,
    pub stale_ticks: u64,
    pub warn_threshold: f32,
    pub critical_threshold: f32,
    pub history: Vec<CrisisTrendSample>,
}

impl Default for CrisisGaugeSnapshot {
    fn default() -> Self {
        Self {
            kind: CrisisMetricKind::R0,
            raw: 0.0,
            ema: 0.0,
            trend_5t: 0.0,
            band: CrisisSeverityBand::Safe,
            last_updated_tick: 0,
            stale_ticks: 0,
            warn_threshold: 0.0,
            critical_threshold: 0.0,
            history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CrisisMetricsSnapshot {
    pub gauges: Vec<CrisisGaugeSnapshot>,
    pub modifiers_active: u32,
    pub foreshock_incidents: u32,
    pub containment_incidents: u32,
    pub warnings_active: u32,
    pub criticals_active: u32,
}

impl CrisisMetricsSnapshot {
    pub fn gauge(&self, kind: CrisisMetricKind) -> Option<&CrisisGaugeSnapshot> {
        self.gauges.iter().find(|g| g.kind == kind)
    }

    pub fn modifiers_active(&self) -> u32 {
        self.modifiers_active
    }
}

#[derive(Debug, Clone, Default)]
pub struct CrisisTelemetrySample {
    pub r0: Option<f32>,
    pub grid_stress_pct: Option<f32>,
    pub unauthorized_queue_pct: Option<f32>,
    pub swarms_active: Option<f32>,
    pub phage_density: Option<f32>,
    pub modifiers_active: Option<u32>,
    pub foreshock_incidents: Option<u32>,
    pub containment_incidents: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
struct GaugeParameters {
    alpha: f32,
    history_depth: usize,
    trend_window: usize,
}

#[derive(Resource, Debug, Clone)]
pub struct CrisisTelemetry {
    r0: CrisisGauge,
    grid_stress_pct: CrisisGauge,
    unauthorized_queue_pct: CrisisGauge,
    swarms_active: CrisisGauge,
    phage_density: CrisisGauge,
    modifiers_active: u32,
    foreshock_incidents: u32,
    containment_incidents: u32,
    stale_warning: u64,
    stale_critical: u64,
    alert_cooldown_ticks: u64,
    config_hash: u64,
}

impl CrisisTelemetry {
    pub fn from_config(config: &CrisisTelemetryConfig) -> Self {
        let params = GaugeParameters {
            alpha: config.ema_alpha,
            history_depth: config.history_depth.max(1),
            trend_window: config.trend_window.max(1),
        };
        let thresholds = |key: &str, warn: f32, critical: f32| {
            config
                .gauges
                .get(key)
                .cloned()
                .unwrap_or_else(|| CrisisTelemetryThreshold {
                    warn,
                    critical,
                    ..Default::default()
                })
        };

        Self {
            r0: CrisisGauge::new(CrisisMetricKind::R0, thresholds("r0", 0.9, 1.2), params),
            grid_stress_pct: CrisisGauge::new(
                CrisisMetricKind::GridStressPct,
                thresholds("grid_stress_pct", 70.0, 85.0),
                params,
            ),
            unauthorized_queue_pct: CrisisGauge::new(
                CrisisMetricKind::UnauthorizedQueuePct,
                thresholds("unauthorized_queue_pct", 10.0, 25.0),
                params,
            ),
            swarms_active: CrisisGauge::new(
                CrisisMetricKind::SwarmsActive,
                thresholds("swarms_active", 2.0, 5.0),
                params,
            ),
            phage_density: CrisisGauge::new(
                CrisisMetricKind::PhageDensity,
                thresholds("phage_density", 0.35, 0.6),
                params,
            ),
            modifiers_active: 0,
            foreshock_incidents: 0,
            containment_incidents: 0,
            stale_warning: config.stale_tick_warning,
            stale_critical: config.stale_tick_critical,
            alert_cooldown_ticks: config.alert_cooldown_ticks,
            config_hash: hash_telemetry_config(config),
        }
    }

    pub fn ensure_config(&mut self, config: &CrisisTelemetryConfig) {
        let hash = hash_telemetry_config(config);
        if self.config_hash != hash {
            *self = CrisisTelemetry::from_config(config);
        }
    }

    pub fn apply_config(&mut self, config: &CrisisTelemetryConfig) {
        *self = CrisisTelemetry::from_config(config);
    }

    pub fn record_sample(&mut self, tick: u64, sample: CrisisTelemetrySample) {
        if let Some(value) = sample.r0 {
            let transition = self.r0.update(tick, value);
            self.log_transition(transition, tick);
        }
        if let Some(value) = sample.grid_stress_pct {
            let transition = self.grid_stress_pct.update(tick, value);
            self.log_transition(transition, tick);
        }
        if let Some(value) = sample.unauthorized_queue_pct {
            let transition = self.unauthorized_queue_pct.update(tick, value);
            self.log_transition(transition, tick);
        }
        if let Some(value) = sample.swarms_active {
            let transition = self.swarms_active.update(tick, value);
            self.log_transition(transition, tick);
        }
        if let Some(value) = sample.phage_density {
            let transition = self.phage_density.update(tick, value);
            self.log_transition(transition, tick);
        }
        if let Some(value) = sample.modifiers_active {
            self.modifiers_active = value;
        }
        if let Some(value) = sample.foreshock_incidents {
            self.foreshock_incidents = value;
        }
        if let Some(value) = sample.containment_incidents {
            self.containment_incidents = value;
        }
    }

    pub fn record_metric(&mut self, tick: u64, kind: CrisisMetricKind, value: f32) {
        let transition = match kind {
            CrisisMetricKind::R0 => self.r0.update(tick, value),
            CrisisMetricKind::GridStressPct => self.grid_stress_pct.update(tick, value),
            CrisisMetricKind::UnauthorizedQueuePct => {
                self.unauthorized_queue_pct.update(tick, value)
            }
            CrisisMetricKind::SwarmsActive => self.swarms_active.update(tick, value),
            CrisisMetricKind::PhageDensity => self.phage_density.update(tick, value),
        };
        self.log_transition(transition, tick);
    }

    pub fn snapshot(&self, current_tick: u64) -> CrisisMetricsSnapshot {
        let mut gauges = vec![
            self.r0.snapshot(current_tick),
            self.grid_stress_pct.snapshot(current_tick),
            self.unauthorized_queue_pct.snapshot(current_tick),
            self.swarms_active.snapshot(current_tick),
            self.phage_density.snapshot(current_tick),
        ];

        for gauge in &mut gauges {
            if gauge.stale_ticks >= self.stale_critical {
                gauge.band = CrisisSeverityBand::Critical;
            } else if gauge.stale_ticks >= self.stale_warning
                && matches!(gauge.band, CrisisSeverityBand::Safe)
            {
                gauge.band = CrisisSeverityBand::Warn;
            }
        }

        let warnings_active = gauges
            .iter()
            .filter(|gauge| matches!(gauge.band, CrisisSeverityBand::Warn))
            .count() as u32;
        let criticals_active = gauges
            .iter()
            .filter(|gauge| matches!(gauge.band, CrisisSeverityBand::Critical))
            .count() as u32;

        CrisisMetricsSnapshot {
            gauges,
            modifiers_active: self.modifiers_active,
            foreshock_incidents: self.foreshock_incidents,
            containment_incidents: self.containment_incidents,
            warnings_active,
            criticals_active,
        }
    }

    pub fn log_telemetry(&self, current_tick: u64) {
        let snapshot = self.snapshot(current_tick);
        for gauge in &snapshot.gauges {
            info!(
                target: "crisis.telemetry",
                tick = current_tick,
                metric = ?gauge.kind,
                raw = gauge.raw,
                ema = gauge.ema,
                trend_5t = gauge.trend_5t,
                warn_threshold = gauge.warn_threshold,
                critical_threshold = gauge.critical_threshold,
                band = ?gauge.band,
                last_updated_tick = gauge.last_updated_tick,
                stale_ticks = gauge.stale_ticks,
                stale_warn = self.stale_warning,
                stale_critical = self.stale_critical,
                history = ?gauge.history,
                modifiers_active = snapshot.modifiers_active,
                foreshock_incidents = snapshot.foreshock_incidents,
                containment_incidents = snapshot.containment_incidents,
                warnings_active = snapshot.warnings_active,
                criticals_active = snapshot.criticals_active,
                alert_cooldown_ticks = self.alert_cooldown_ticks,
                "crisis.telemetry"
            );
        }
    }

    fn log_transition(
        &self,
        transition: Option<(CrisisMetricKind, CrisisSeverityBand, CrisisSeverityBand)>,
        tick: u64,
    ) {
        if let Some((kind, previous, current)) = transition {
            if previous == current {
                return;
            }
            let status = if previous == CrisisSeverityBand::Critical
                && current == CrisisSeverityBand::Warn
            {
                "downgraded"
            } else {
                match current {
                    CrisisSeverityBand::Critical => "critical",
                    CrisisSeverityBand::Warn => "warn",
                    CrisisSeverityBand::Safe => "resolved",
                }
            };
            info!(
                target: "crisis.alerts",
                tick,
                metric = ?kind,
                previous = ?previous,
                current = ?current,
                status,
                "crisis.alert_transition"
            );
        }
    }
}

impl Default for CrisisTelemetry {
    fn default() -> Self {
        CrisisTelemetry::from_config(&CrisisTelemetryConfig::default())
    }
}

#[derive(Debug, Clone)]
struct CrisisGauge {
    kind: CrisisMetricKind,
    warn_threshold: f32,
    critical_threshold: f32,
    alpha: f32,
    history_depth: usize,
    trend_window: usize,
    raw: f32,
    ema: Option<f32>,
    history: VecDeque<(u64, f32)>,
    last_updated_tick: u64,
    last_band: CrisisSeverityBand,
}

impl CrisisGauge {
    fn new(
        kind: CrisisMetricKind,
        threshold: CrisisTelemetryThreshold,
        params: GaugeParameters,
    ) -> Self {
        Self {
            kind,
            warn_threshold: threshold.warn,
            critical_threshold: threshold.critical,
            alpha: params.alpha,
            history_depth: params.history_depth,
            trend_window: params.trend_window,
            raw: 0.0,
            ema: None,
            history: VecDeque::with_capacity(params.history_depth),
            last_updated_tick: 0,
            last_band: CrisisSeverityBand::Safe,
        }
    }

    fn update(
        &mut self,
        tick: u64,
        value: f32,
    ) -> Option<(CrisisMetricKind, CrisisSeverityBand, CrisisSeverityBand)> {
        let previous_band = self.last_band;
        self.raw = value;
        self.ema = Some(match self.ema {
            Some(previous) => self.alpha.mul_add(value, (1.0 - self.alpha) * previous),
            None => value,
        });
        self.history.push_back((tick, value));
        while self.history.len() > self.history_depth {
            self.history.pop_front();
        }
        self.last_updated_tick = tick;
        let current_band = self.classify(self.raw);
        self.last_band = current_band;
        if current_band != previous_band {
            Some((self.kind, previous_band, current_band))
        } else {
            None
        }
    }

    fn snapshot(&self, current_tick: u64) -> CrisisGaugeSnapshot {
        let ema = self.ema.unwrap_or(self.raw);
        let baseline_tick = current_tick.saturating_sub(self.trend_window as u64);
        let baseline = self
            .history
            .iter()
            .find(|(tick, _)| *tick <= baseline_tick)
            .map(|(_, value)| *value)
            .or_else(|| self.history.front().map(|(_, value)| *value))
            .unwrap_or(self.raw);
        let trend = self.raw - baseline;
        let band = self.classify(self.raw);
        let stale_ticks = current_tick.saturating_sub(self.last_updated_tick);
        let history = self
            .history
            .iter()
            .rev()
            .take(self.history_depth)
            .map(|(tick, value)| CrisisTrendSample {
                tick: *tick,
                value: *value,
            })
            .collect::<Vec<_>>();

        CrisisGaugeSnapshot {
            kind: self.kind,
            raw: self.raw,
            ema,
            trend_5t: trend,
            band,
            last_updated_tick: self.last_updated_tick,
            stale_ticks,
            warn_threshold: self.warn_threshold,
            critical_threshold: self.critical_threshold,
            history,
        }
    }

    fn classify(&self, value: f32) -> CrisisSeverityBand {
        if value >= self.critical_threshold {
            CrisisSeverityBand::Critical
        } else if value >= self.warn_threshold {
            CrisisSeverityBand::Warn
        } else {
            CrisisSeverityBand::Safe
        }
    }
}

#[derive(Debug, Clone, Default)]
struct CrisisTelemetryWeights {
    r0_weight: f32,
    grid_stress_weight: f32,
    queue_pressure_weight: f32,
    swarms_active_weight: f32,
    phage_density_weight: f32,
}

impl CrisisTelemetryWeights {
    fn apply_defaults(mut self) -> Self {
        if self.r0_weight == 0.0 {
            self.r0_weight = 1.0;
        }
        if self.grid_stress_weight == 0.0 {
            self.grid_stress_weight = 0.2;
        }
        if self.queue_pressure_weight == 0.0 {
            self.queue_pressure_weight = 0.35;
        }
        if self.swarms_active_weight == 0.0 {
            self.swarms_active_weight = 3.0;
        }
        if self.phage_density_weight == 0.0 {
            self.phage_density_weight = 0.8;
        }
        self
    }
}

#[derive(Debug, Clone)]
struct CrisisIncidentTemplate {
    id: String,
    label: String,
    severity: CrisisSeverityBand,
    cooldown_ticks: u32,
    trigger_intensity: f32,
}

impl CrisisIncidentTemplate {
    fn trigger_threshold(&self) -> f32 {
        self.trigger_intensity
    }
}

#[derive(Debug, Clone)]
struct CrisisArchetypeRuntime {
    id: String,
    name: String,
    base_r0: f32,
    max_r0: f32,
    base_growth: f32,
    incident_acceleration: f32,
    telemetry: CrisisTelemetryWeights,
    incidents: Vec<CrisisIncidentTemplate>,
    _overlay_palette: Option<String>,
    _annotation_glyph: Option<String>,
}

#[derive(Debug, Clone)]
struct CrisisHotspot {
    position: UVec2,
    radius: f32,
}

#[derive(Debug, Clone, Default)]
struct ModifierEffects {
    r0_delta: f32,
    grid_stress_pct: f32,
    queue_pressure_pct: f32,
    swarms_active_bonus: f32,
    phage_density_bonus: f32,
    overlay_multiplier: f32,
}

impl ModifierEffects {
    fn accumulate(&mut self, other: &ModifierEffects) {
        self.r0_delta += other.r0_delta;
        self.grid_stress_pct += other.grid_stress_pct;
        self.queue_pressure_pct += other.queue_pressure_pct;
        self.swarms_active_bonus += other.swarms_active_bonus;
        self.phage_density_bonus += other.phage_density_bonus;
        self.overlay_multiplier += other.overlay_multiplier;
    }
}

#[derive(Debug, Clone)]
struct ActiveModifier {
    _id: String,
    effects: ModifierEffects,
}

#[derive(Debug, Clone)]
struct CrisisAnnotationMarker {
    label: String,
    severity: CrisisSeverityBand,
    coords: Vec<UVec2>,
    ttl: u8,
}

impl CrisisAnnotationMarker {
    fn to_state(&self) -> CrisisOverlayAnnotationState {
        CrisisOverlayAnnotationState {
            label: self.label.clone(),
            severity: severity_to_schema(self.severity),
            path: self
                .coords
                .iter()
                .flat_map(|coord| [coord.x, coord.y])
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveCrisis {
    _id: String,
    name: String,
    _faction: FactionId,
    _seed_tick: u64,
    runtime: CrisisArchetypeRuntime,
    centers: Vec<CrisisHotspot>,
    intensity: f32,
    r0: f32,
    grid_stress_pct: f32,
    queue_pressure_pct: f32,
    swarms_active: f32,
    phage_density: f32,
    incident_timers: HashMap<String, u32>,
    annotations: Vec<CrisisAnnotationMarker>,
    modifiers: Vec<ActiveModifier>,
}

impl ActiveCrisis {
    fn new(
        faction: FactionId,
        seed_tick: u64,
        runtime: CrisisArchetypeRuntime,
        centers: Vec<CrisisHotspot>,
        modifiers: Vec<ActiveModifier>,
    ) -> Self {
        Self {
            _id: runtime.id.clone(),
            name: runtime.name.clone(),
            _faction: faction,
            _seed_tick: seed_tick,
            runtime,
            centers,
            intensity: 0.18,
            r0: 0.0,
            grid_stress_pct: 0.0,
            queue_pressure_pct: 0.0,
            swarms_active: 0.0,
            phage_density: 0.0,
            incident_timers: HashMap::new(),
            annotations: Vec::new(),
            modifiers,
        }
    }

    fn advance(&mut self) -> CrisisAdvanceOutput {
        let mut output = CrisisAdvanceOutput::default();
        let growth = self.runtime.base_growth + self.runtime.incident_acceleration * self.intensity;
        self.intensity = (self.intensity + growth).clamp(0.0, 1.0);

        let mut modifier_effects = ModifierEffects::default();
        for modifier in &self.modifiers {
            modifier_effects.accumulate(&modifier.effects);
        }

        self.r0 = (self.runtime.base_r0
            + (self.runtime.max_r0 - self.runtime.base_r0) * self.intensity)
            + modifier_effects.r0_delta;

        let telemetry = self.runtime.telemetry.clone().apply_defaults();
        self.grid_stress_pct = (self.intensity * 100.0 * telemetry.grid_stress_weight
            + modifier_effects.grid_stress_pct)
            .clamp(0.0, 100.0);
        self.queue_pressure_pct = (self.intensity * 100.0 * telemetry.queue_pressure_weight
            + modifier_effects.queue_pressure_pct)
            .clamp(0.0, 100.0);
        self.swarms_active = (self.intensity * telemetry.swarms_active_weight
            + modifier_effects.swarms_active_bonus)
            .clamp(0.0, 10.0);
        self.phage_density = (self.intensity * telemetry.phage_density_weight
            + modifier_effects.phage_density_bonus)
            .clamp(0.0, 2.0);

        for timer in self.incident_timers.values_mut() {
            if *timer > 0 {
                *timer -= 1;
            }
        }

        self.annotations.retain_mut(|marker| {
            if marker.ttl > 0 {
                marker.ttl -= 1;
            }
            marker.ttl > 0
        });

        for template in &self.runtime.incidents {
            let triggered = {
                let timer_entry = self.incident_timers.entry(template.id.clone()).or_insert(0);
                if *timer_entry > 0 || self.intensity < template.trigger_threshold() {
                    false
                } else {
                    *timer_entry = template.cooldown_ticks.max(2);
                    true
                }
            };

            if !triggered {
                continue;
            }

            let label = format!("{} · {}", self.name, template.label);
            let coords = vec![self.primary_coordinate()];
            self.annotations.push(CrisisAnnotationMarker {
                label,
                severity: template.severity,
                coords,
                ttl: 6,
            });
            match template.severity {
                CrisisSeverityBand::Critical => output.critical_events += 1,
                CrisisSeverityBand::Warn => output.warn_events += 1,
                CrisisSeverityBand::Safe => {}
            }
        }

        output
    }

    fn overlay_multiplier(&self) -> f32 {
        1.0 + self.modifiers.len() as f32 * 0.05
            + self
                .modifiers
                .iter()
                .map(|modifier| modifier.effects.overlay_multiplier)
                .sum::<f32>()
    }

    fn overlay_value_at(&self, position: UVec2) -> f32 {
        let mut value = 0.0f32;
        for hotspot in &self.centers {
            let dx = position.x as f32 - hotspot.position.x as f32;
            let dy = position.y as f32 - hotspot.position.y as f32;
            let distance_sq = dx * dx + dy * dy;
            let sigma = (hotspot.radius * (0.5 + self.intensity)).max(1.2);
            let gaussian =
                (-distance_sq / (2.0 * sigma * sigma)).exp() / (2.0 * PI * sigma * sigma);
            value += gaussian;
        }
        (value * self.intensity * self.runtime.telemetry.r0_weight).min(1.0)
    }

    fn primary_coordinate(&self) -> UVec2 {
        self.centers
            .first()
            .map(|hotspot| hotspot.position)
            .unwrap_or(UVec2::new(0, 0))
    }
}

#[derive(Debug, Default)]
struct CrisisAdvanceOutput {
    warn_events: u32,
    critical_events: u32,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct ActiveCrisisLedger {
    entries: Vec<ActiveCrisis>,
}

impl ActiveCrisisLedger {
    fn entries(&self) -> &[ActiveCrisis] {
        &self.entries
    }

    fn entries_mut(&mut self) -> &mut [ActiveCrisis] {
        &mut self.entries
    }

    fn push(&mut self, crisis: ActiveCrisis) {
        self.entries.push(crisis);
    }

    fn total_modifiers(&self) -> usize {
        self.entries.iter().map(|entry| entry.modifiers.len()).sum()
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct CrisisOverlayCache {
    pub raster: ScalarRasterState,
    pub annotations: Vec<CrisisOverlayAnnotationState>,
}

impl CrisisOverlayCache {
    pub fn reset(&mut self, width: u32, height: u32) {
        let count = (width as usize).saturating_mul(height as usize).max(1);
        self.raster = ScalarRasterState {
            width,
            height,
            samples: vec![0; count],
        };
        self.annotations.clear();
    }

    pub fn update(
        &mut self,
        width: u32,
        height: u32,
        samples: Vec<i64>,
        annotations: Vec<CrisisOverlayAnnotationState>,
    ) {
        self.raster = ScalarRasterState {
            width,
            height,
            samples,
        };
        self.annotations = annotations;
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ArchetypePropagationConfig {
    #[serde(default)]
    base_r0: Option<f32>,
    #[serde(default)]
    max_r0: Option<f32>,
    #[serde(default)]
    base_growth: Option<f32>,
    #[serde(default)]
    incident_acceleration: Option<f32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ArchetypeTelemetryConfig {
    #[serde(default)]
    r0_weight: Option<f32>,
    #[serde(default)]
    grid_stress_weight: Option<f32>,
    #[serde(default)]
    queue_pressure_weight: Option<f32>,
    #[serde(default)]
    swarms_active_weight: Option<f32>,
    #[serde(default)]
    phage_density_weight: Option<f32>,
}

impl ArchetypeTelemetryConfig {
    fn into_weights(self) -> CrisisTelemetryWeights {
        CrisisTelemetryWeights {
            r0_weight: self.r0_weight.unwrap_or(0.0),
            grid_stress_weight: self.grid_stress_weight.unwrap_or(0.0),
            queue_pressure_weight: self.queue_pressure_weight.unwrap_or(0.0),
            swarms_active_weight: self.swarms_active_weight.unwrap_or(0.0),
            phage_density_weight: self.phage_density_weight.unwrap_or(0.0),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ArchetypeOverlayConfig {
    palette: Option<String>,
    annotation_glyph: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ArchetypeIncidentConfig {
    id: Option<String>,
    label: Option<String>,
    severity: Option<String>,
    #[serde(default)]
    cooldown_ticks: u32,
    #[serde(default)]
    trigger_intensity: Option<f32>,
}

fn archetype_runtime(archetype: &CrisisArchetype) -> Option<CrisisArchetypeRuntime> {
    let propagation_value = archetype.extra.get("propagation")?;
    let propagation: ArchetypePropagationConfig =
        serde_json::from_value(propagation_value.clone()).ok()?;
    let telemetry_cfg = archetype
        .extra
        .get("telemetry")
        .and_then(|value| serde_json::from_value::<ArchetypeTelemetryConfig>(value.clone()).ok())
        .unwrap_or_default();
    let overlay_cfg = archetype
        .extra
        .get("overlay")
        .and_then(|value| serde_json::from_value::<ArchetypeOverlayConfig>(value.clone()).ok())
        .unwrap_or_default();
    let incidents_cfg = archetype
        .extra
        .get("incident_table")
        .and_then(|value| {
            serde_json::from_value::<Vec<ArchetypeIncidentConfig>>(value.clone()).ok()
        })
        .unwrap_or_default();

    let incidents = incidents_cfg
        .into_iter()
        .map(build_incident_template)
        .collect::<Vec<_>>();

    Some(CrisisArchetypeRuntime {
        id: archetype.id.clone(),
        name: archetype.name.clone(),
        base_r0: propagation.base_r0.unwrap_or(0.94),
        max_r0: propagation.max_r0.unwrap_or(1.35),
        base_growth: propagation.base_growth.unwrap_or(0.045),
        incident_acceleration: propagation.incident_acceleration.unwrap_or(0.02),
        telemetry: telemetry_cfg.into_weights(),
        incidents,
        _overlay_palette: overlay_cfg.palette,
        _annotation_glyph: overlay_cfg.annotation_glyph,
    })
}

fn build_incident_template(config: ArchetypeIncidentConfig) -> CrisisIncidentTemplate {
    let severity = severity_from_str(config.severity.as_deref());
    let id = config.id.clone().unwrap_or_else(|| "incident".to_string());
    let label_source = config
        .label
        .as_deref()
        .unwrap_or_else(|| config.id.as_deref().unwrap_or(&id));
    let label = title_case_label(label_source);
    let trigger = config
        .trigger_intensity
        .unwrap_or_else(|| default_trigger_for(severity));

    CrisisIncidentTemplate {
        id,
        label,
        severity,
        cooldown_ticks: config.cooldown_ticks.max(2),
        trigger_intensity: trigger.clamp(0.05, 1.0),
    }
}

fn select_archetype(
    catalog: &CrisisArchetypeCatalog,
    discovery_id: u16,
) -> Option<&CrisisArchetype> {
    if catalog.archetypes.is_empty() {
        return None;
    }
    let idx = (discovery_id as usize) % catalog.archetypes.len();
    catalog.archetypes.get(idx)
}

fn compose_seed(faction: FactionId, discovery_id: u16, tick: u64) -> u64 {
    let faction_component = (faction.0 as u64) << 32;
    let discovery_component = discovery_id as u64;
    faction_component
        ^ discovery_component.wrapping_mul(0x9E37_79B9)
        ^ tick.wrapping_mul(0xC2B2_AE35)
}

fn hash_identifier(identifier: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    identifier.hash(&mut hasher);
    hasher.finish()
}

fn choose_modifiers(rng: &mut SmallRng, catalog: &CrisisModifierCatalog) -> Vec<ActiveModifier> {
    if catalog.modifiers.is_empty() {
        return Vec::new();
    }

    let max_select = catalog.modifiers.len().min(2);
    let count = rng.gen_range(0..=max_select);
    if count == 0 {
        return Vec::new();
    }

    let mut indices: Vec<usize> = (0..catalog.modifiers.len()).collect();
    indices.shuffle(rng);

    indices
        .into_iter()
        .take(count)
        .filter_map(|idx| catalog.modifiers.get(idx))
        .map(|modifier| ActiveModifier {
            _id: modifier.id.clone(),
            effects: parse_modifier_effects(modifier),
        })
        .collect()
}

fn parse_modifier_effects(modifier: &CrisisModifier) -> ModifierEffects {
    let mut effects = ModifierEffects::default();
    if let Some(map) = modifier
        .extra
        .get("effects")
        .and_then(|value| value.as_object())
    {
        if let Some(value) = map.get("r0_delta").and_then(|v| v.as_f64()) {
            effects.r0_delta += value as f32;
        }
        if let Some(value) = map.get("grid_stress_pct").and_then(|v| v.as_f64()) {
            effects.grid_stress_pct += value as f32;
        }
        if let Some(value) = map.get("queue_pressure_pct").and_then(|v| v.as_f64()) {
            effects.queue_pressure_pct += value as f32;
        }
        if let Some(value) = map.get("swarms_active_bonus").and_then(|v| v.as_f64()) {
            effects.swarms_active_bonus += value as f32;
        }
        if let Some(value) = map.get("phage_density").and_then(|v| v.as_f64()) {
            effects.phage_density_bonus += value as f32;
        }
        if let Some(value) = map.get("overlay_multiplier").and_then(|v| v.as_f64()) {
            effects.overlay_multiplier += value as f32;
        }
    }
    effects
}

fn generate_hotspots(rng: &mut SmallRng, grid_size: UVec2) -> Vec<CrisisHotspot> {
    let width = grid_size.x.max(MIN_GRID_DIMENSION);
    let height = grid_size.y.max(MIN_GRID_DIMENSION);
    let center = UVec2::new(rng.gen_range(0..width), rng.gen_range(0..height));
    let mut hotspots = Vec::with_capacity(3);
    hotspots.push(CrisisHotspot {
        position: center,
        radius: rng.gen_range(2.0..=4.5),
    });
    let additional = rng.gen_range(1..=2);
    for _ in 0..additional {
        let offset_x = rng.gen_range(-4i32..=4);
        let offset_y = rng.gen_range(-4i32..=4);
        let x = (center.x as i32 + offset_x).clamp(0, width.saturating_sub(1) as i32) as u32;
        let y = (center.y as i32 + offset_y).clamp(0, height.saturating_sub(1) as i32) as u32;
        hotspots.push(CrisisHotspot {
            position: UVec2::new(x, y),
            radius: rng.gen_range(1.5..=3.5),
        });
    }
    hotspots
}

fn rebuild_overlay(
    ledger: &mut ActiveCrisisLedger,
    grid_size: UVec2,
) -> (Vec<f32>, Vec<CrisisOverlayAnnotationState>, u32, u32) {
    let width = grid_size.x.max(MIN_GRID_DIMENSION);
    let height = grid_size.y.max(MIN_GRID_DIMENSION);
    let total = (width as usize).saturating_mul(height as usize).max(1);
    let mut samples = vec![0.0f32; total];
    let mut annotations = Vec::new();
    let mut warn_events_total = 0u32;
    let mut critical_events_total = 0u32;

    for crisis in ledger.entries_mut() {
        let result = crisis.advance();
        warn_events_total += result.warn_events;
        critical_events_total += result.critical_events;
        let multiplier = crisis.overlay_multiplier();

        for y in 0..height {
            for x in 0..width {
                let idx = (y as usize) * width as usize + x as usize;
                let value = crisis.overlay_value_at(UVec2::new(x, y)) * multiplier;
                samples[idx] += value;
            }
        }

        for marker in &crisis.annotations {
            annotations.push(marker.to_state());
        }
    }

    let mut max_sample = 0.0f32;
    for value in &samples {
        if *value > max_sample {
            max_sample = *value;
        }
    }
    let normalization = if max_sample > 1.0 {
        1.0 / max_sample
    } else {
        1.0
    };
    for value in &mut samples {
        *value = (*value * normalization).clamp(0.0, 1.0);
    }

    (
        samples,
        annotations,
        warn_events_total,
        critical_events_total,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn advance_crisis_system(
    config: Res<SimulationConfig>,
    tick: Res<SimulationTick>,
    herd_density: Res<HerdDensityMap>,
    mut pending_seeds: ResMut<PendingCrisisSeeds>,
    mut pending_spawns: ResMut<PendingCrisisSpawns>,
    archetypes: Res<CrisisArchetypeCatalogHandle>,
    modifiers: Res<CrisisModifierCatalogHandle>,
    telemetry_config: Res<CrisisTelemetryConfigHandle>,
    mut ledger: ResMut<ActiveCrisisLedger>,
    mut overlay: ResMut<CrisisOverlayCache>,
    mut telemetry: ResMut<CrisisTelemetry>,
) {
    let grid_size = config.grid_size;
    let telemetry_cfg = telemetry_config.get();
    telemetry.ensure_config(telemetry_cfg.as_ref());

    let catalog = archetypes.get();
    let modifier_catalog = modifiers.get();

    if config.crisis_auto_seed
        && ledger.entries().is_empty()
        && pending_seeds.seeds.is_empty()
        && pending_spawns.spawns.is_empty()
        && !catalog.archetypes.is_empty()
    {
        if let Some(first) = catalog.archetypes.first() {
            pending_spawns.push(FactionId(0), first.id.clone());
            info!(
                target: "shadow_scale::crisis",
                archetype = %first.id,
                faction = 0,
                "crisis.autoseed.enqueued"
            );
        }
    }

    let seeds = pending_seeds.drain();
    for (faction, discovery_id) in seeds {
        if let Some(archetype) = select_archetype(&catalog, discovery_id) {
            if let Some(runtime) = archetype_runtime(archetype) {
                let seed = compose_seed(faction, discovery_id, tick.0);
                let mut rng = SmallRng::seed_from_u64(seed);
                let hotspots = generate_hotspots(&mut rng, grid_size);
                let assigned_modifiers = choose_modifiers(&mut rng, &modifier_catalog);
                ledger.push(ActiveCrisis::new(
                    faction,
                    tick.0,
                    runtime,
                    hotspots,
                    assigned_modifiers,
                ));
                info!(
                    target: "shadow_scale::crisis",
                    %discovery_id,
                    faction = %faction.0,
                    archetype = %archetype.id,
                    "crisis.spawn.discovery"
                );
            }
        } else {
            warn!(
                target: "shadow_scale::crisis",
                %discovery_id,
                "crisis.spawn.discovery_unknown"
            );
        }
    }

    let manual_spawns = pending_spawns.drain();
    for (faction, archetype_id) in manual_spawns {
        let normalized = archetype_id.to_ascii_lowercase();
        if let Some(archetype) = catalog.archetype(&normalized) {
            if let Some(runtime) = archetype_runtime(archetype) {
                let seed = compose_seed(faction, 0, tick.0 ^ hash_identifier(&normalized));
                let mut rng = SmallRng::seed_from_u64(seed);
                let hotspots = generate_hotspots(&mut rng, grid_size);
                let assigned_modifiers = choose_modifiers(&mut rng, &modifier_catalog);
                ledger.push(ActiveCrisis::new(
                    faction,
                    tick.0,
                    runtime,
                    hotspots,
                    assigned_modifiers,
                ));
                info!(
                    target: "shadow_scale::crisis",
                    faction = %faction.0,
                    archetype = %normalized,
                    "crisis.spawn.manual"
                );
            }
        } else {
            warn!(
                target: "shadow_scale::crisis",
                faction = %faction.0,
                archetype = %normalized,
                "crisis.spawn.manual.unknown_archetype"
            );
        }
    }

    let herd_density_signal = herd_density.normalized_average();

    if ledger.entries().is_empty() {
        overlay.reset(
            grid_size.x.max(MIN_GRID_DIMENSION),
            grid_size.y.max(MIN_GRID_DIMENSION),
        );
        telemetry.record_sample(
            tick.0,
            CrisisTelemetrySample {
                r0: Some(0.0),
                grid_stress_pct: Some(0.0),
                unauthorized_queue_pct: Some(0.0),
                swarms_active: Some(0.0),
                phage_density: Some(herd_density_signal),
                modifiers_active: Some(0),
                foreshock_incidents: Some(0),
                containment_incidents: Some(0),
            },
        );
        return;
    }

    let (samples, annotations, warn_events, critical_events) =
        rebuild_overlay(&mut ledger, grid_size);

    let width = grid_size.x.max(MIN_GRID_DIMENSION);
    let height = grid_size.y.max(MIN_GRID_DIMENSION);
    let scalar_samples: Vec<i64> = samples
        .into_iter()
        .map(|value| Scalar::from_f32(value.clamp(0.0, 1.0)).raw())
        .collect();
    overlay.update(width, height, scalar_samples, annotations);

    let crisis_count = ledger.entries().len() as f32;
    let mut total_r0 = 0.0;
    let mut total_grid = 0.0;
    let mut total_queue = 0.0;
    let mut total_swarms = 0.0;
    let mut total_phage = 0.0;
    for crisis in ledger.entries() {
        total_r0 += crisis.r0;
        total_grid += crisis.grid_stress_pct;
        total_queue += crisis.queue_pressure_pct;
        total_swarms += crisis.swarms_active;
        total_phage += crisis.phage_density;
    }

    let sample = CrisisTelemetrySample {
        r0: Some(total_r0 / crisis_count),
        grid_stress_pct: Some(total_grid / crisis_count),
        unauthorized_queue_pct: Some(total_queue / crisis_count),
        swarms_active: Some(total_swarms),
        phage_density: Some(
            (total_phage / crisis_count) + herd_density_signal * HERD_DENSITY_CRISIS_WEIGHT,
        ),
        modifiers_active: Some(ledger.total_modifiers() as u32),
        foreshock_incidents: Some(warn_events),
        containment_incidents: Some(critical_events),
    };
    telemetry.record_sample(tick.0, sample);
}

fn severity_from_str(input: Option<&str>) -> CrisisSeverityBand {
    match input.map(|value| value.to_ascii_lowercase()) {
        Some(ref value) if value == "critical" => CrisisSeverityBand::Critical,
        Some(ref value) if value == "warn" || value == "warning" => CrisisSeverityBand::Warn,
        _ => CrisisSeverityBand::Safe,
    }
}

fn default_trigger_for(severity: CrisisSeverityBand) -> f32 {
    match severity {
        CrisisSeverityBand::Critical => 0.68,
        CrisisSeverityBand::Warn => 0.38,
        CrisisSeverityBand::Safe => 0.2,
    }
}

fn title_case_label(value: &str) -> String {
    value
        .split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    let mut word = first.to_uppercase().collect::<String>();
                    word.push_str(&chars.as_str().to_lowercase());
                    word
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn severity_to_schema(band: CrisisSeverityBand) -> SchemaCrisisSeverityBand {
    match band {
        CrisisSeverityBand::Critical => SchemaCrisisSeverityBand::Critical,
        CrisisSeverityBand::Warn => SchemaCrisisSeverityBand::Warn,
        CrisisSeverityBand::Safe => SchemaCrisisSeverityBand::Safe,
    }
}

fn hash_telemetry_config(config: &CrisisTelemetryConfig) -> u64 {
    let mut hasher = DefaultHasher::new();
    config.ema_alpha.to_bits().hash(&mut hasher);
    config.history_depth.hash(&mut hasher);
    config.trend_window.hash(&mut hasher);
    config.stale_tick_warning.hash(&mut hasher);
    config.stale_tick_critical.hash(&mut hasher);
    config.alert_cooldown_ticks.hash(&mut hasher);

    for (key, threshold) in &config.gauges {
        key.hash(&mut hasher);
        threshold.warn.to_bits().hash(&mut hasher);
        threshold.critical.to_bits().hash(&mut hasher);
        if let Some(delta) = threshold.escalation_delta {
            delta.to_bits().hash(&mut hasher);
        }
    }

    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crisis_config::{CrisisArchetypeCatalog, CrisisModifierCatalog};
    use bevy_ecs::system::RunSystemOnce;

    #[test]
    fn ema_progression_matches_alpha() {
        let threshold = CrisisTelemetryThreshold {
            warn: 0.9,
            critical: 1.2,
            ..Default::default()
        };
        let params = GaugeParameters {
            alpha: 0.35,
            history_depth: 6,
            trend_window: 5,
        };
        let mut gauge = CrisisGauge::new(CrisisMetricKind::R0, threshold, params);
        gauge.update(1, 1.0);
        assert_eq!(gauge.ema, Some(1.0));

        gauge.update(2, 2.0);
        let expected = params.alpha * 2.0 + (1.0 - params.alpha) * 1.0;
        assert!((gauge.ema.unwrap() - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn trend_uses_baseline_within_window() {
        let threshold = CrisisTelemetryThreshold {
            warn: 70.0,
            critical: 85.0,
            ..Default::default()
        };
        let params = GaugeParameters {
            alpha: 0.35,
            history_depth: 6,
            trend_window: 5,
        };
        let mut gauge = CrisisGauge::new(CrisisMetricKind::GridStressPct, threshold, params);
        gauge.update(1, 10.0);
        gauge.update(2, 20.0);
        gauge.update(3, 30.0);
        gauge.update(4, 40.0);
        gauge.update(5, 50.0);

        let snapshot = gauge.snapshot(5);
        assert!((snapshot.trend_5t - 40.0).abs() < f32::EPSILON);
    }

    #[test]
    fn snapshot_counts_warnings_and_criticals() {
        let mut telemetry = CrisisTelemetry::default();
        telemetry.record_sample(
            10,
            CrisisTelemetrySample {
                r0: Some(1.3),
                grid_stress_pct: Some(80.0),
                unauthorized_queue_pct: Some(5.0),
                swarms_active: Some(1.0),
                phage_density: Some(0.2),
                modifiers_active: Some(3),
                foreshock_incidents: Some(2),
                containment_incidents: Some(1),
            },
        );

        let snapshot = telemetry.snapshot(10);
        assert_eq!(snapshot.criticals_active, 1);
        assert_eq!(snapshot.warnings_active, 1);
        assert_eq!(snapshot.modifiers_active, 3);
        assert_eq!(snapshot.foreshock_incidents, 2);
        assert_eq!(snapshot.containment_incidents, 1);
        assert!(snapshot
            .gauge(CrisisMetricKind::R0)
            .map(|g| matches!(g.band, CrisisSeverityBand::Critical))
            .unwrap_or(false));
    }

    #[test]
    fn crisis_overlay_generation() {
        let mut app = App::new();
        let config = SimulationConfig {
            grid_size: UVec2::new(8, 6),
            ..SimulationConfig::default()
        };
        app.insert_resource(config);
        app.insert_resource(SimulationTick(0));
        app.insert_resource(PendingCrisisSeeds::default());
        app.insert_resource(PendingCrisisSpawns::default());
        app.insert_resource(ActiveCrisisLedger::default());
        app.insert_resource(CrisisOverlayCache::default());

        let archetypes = CrisisArchetypeCatalog::builtin();
        let modifiers = CrisisModifierCatalog::builtin();
        let telemetry_cfg = CrisisTelemetryConfig::builtin();

        app.insert_resource(CrisisArchetypeCatalogHandle::new(archetypes.clone()));
        app.insert_resource(CrisisModifierCatalogHandle::new(modifiers.clone()));
        app.insert_resource(CrisisTelemetryConfigHandle::new(telemetry_cfg.clone()));
        app.insert_resource(CrisisTelemetry::from_config(telemetry_cfg.as_ref()));

        {
            let mut spawns = app.world.resource_mut::<PendingCrisisSpawns>();
            spawns.push(FactionId(0), "plague_bloom");
        }

        app.world.run_system_once(advance_crisis_system);

        let overlay = app.world.resource::<CrisisOverlayCache>();
        let non_zero = overlay
            .raster
            .samples
            .iter()
            .filter(|value| **value != 0)
            .count();
        assert!(
            non_zero > 0,
            "crisis overlay should emit non-zero samples after seeding"
        );
    }

    #[test]
    fn crisis_auto_seeds_when_empty() {
        let mut app = App::new();
        let config = SimulationConfig {
            grid_size: UVec2::new(6, 4),
            crisis_auto_seed: true,
            ..SimulationConfig::default()
        };
        app.insert_resource(config);
        app.insert_resource(SimulationTick(0));
        app.insert_resource(PendingCrisisSeeds::default());
        app.insert_resource(PendingCrisisSpawns::default());
        app.insert_resource(ActiveCrisisLedger::default());
        app.insert_resource(CrisisOverlayCache::default());

        let archetypes = CrisisArchetypeCatalog::builtin();
        let modifiers = CrisisModifierCatalog::builtin();
        let telemetry_cfg = CrisisTelemetryConfig::builtin();

        app.insert_resource(CrisisArchetypeCatalogHandle::new(archetypes.clone()));
        app.insert_resource(CrisisModifierCatalogHandle::new(modifiers.clone()));
        app.insert_resource(CrisisTelemetryConfigHandle::new(telemetry_cfg.clone()));
        app.insert_resource(CrisisTelemetry::from_config(telemetry_cfg.as_ref()));

        app.world.run_system_once(super::advance_crisis_system);

        let overlay = app.world.resource::<CrisisOverlayCache>();
        let has_signal = overlay.raster.samples.iter().any(|value| *value != 0);
        assert!(
            has_signal,
            "crisis overlay auto-seeding should produce non-zero samples"
        );
    }
}
