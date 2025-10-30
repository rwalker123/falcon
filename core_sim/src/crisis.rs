use std::collections::VecDeque;

use bevy::prelude::*;
use tracing::info;

const EMA_ALPHA: f32 = 0.35;
const HISTORY_DEPTH: usize = 6;

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
}

impl Default for CrisisTelemetry {
    fn default() -> Self {
        Self {
            r0: CrisisGauge::new(CrisisMetricKind::R0, 0.9, 1.2),
            grid_stress_pct: CrisisGauge::new(CrisisMetricKind::GridStressPct, 70.0, 85.0),
            unauthorized_queue_pct: CrisisGauge::new(
                CrisisMetricKind::UnauthorizedQueuePct,
                10.0,
                25.0,
            ),
            swarms_active: CrisisGauge::new(CrisisMetricKind::SwarmsActive, 2.0, 5.0),
            phage_density: CrisisGauge::new(CrisisMetricKind::PhageDensity, 0.35, 0.6),
            modifiers_active: 0,
            foreshock_incidents: 0,
            containment_incidents: 0,
        }
    }
}

impl CrisisTelemetry {
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
        let gauges = vec![
            self.r0.snapshot(current_tick),
            self.grid_stress_pct.snapshot(current_tick),
            self.unauthorized_queue_pct.snapshot(current_tick),
            self.swarms_active.snapshot(current_tick),
            self.phage_density.snapshot(current_tick),
        ];

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
                history = ?gauge.history,
                modifiers_active = snapshot.modifiers_active,
                foreshock_incidents = snapshot.foreshock_incidents,
                containment_incidents = snapshot.containment_incidents,
                warnings_active = snapshot.warnings_active,
                criticals_active = snapshot.criticals_active,
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

#[derive(Debug, Clone)]
struct CrisisGauge {
    kind: CrisisMetricKind,
    warn_threshold: f32,
    critical_threshold: f32,
    raw: f32,
    ema: Option<f32>,
    history: VecDeque<(u64, f32)>,
    last_updated_tick: u64,
    last_band: CrisisSeverityBand,
}

impl CrisisGauge {
    fn new(kind: CrisisMetricKind, warn_threshold: f32, critical_threshold: f32) -> Self {
        Self {
            kind,
            warn_threshold,
            critical_threshold,
            raw: 0.0,
            ema: None,
            history: VecDeque::with_capacity(HISTORY_DEPTH),
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
            Some(previous) => EMA_ALPHA.mul_add(value, (1.0 - EMA_ALPHA) * previous),
            None => value,
        });
        self.history.push_back((tick, value));
        while self.history.len() > HISTORY_DEPTH {
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
        let baseline_tick = current_tick.saturating_sub(5);
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
            .take(HISTORY_DEPTH)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ema_progression_matches_alpha() {
        let mut gauge = CrisisGauge::new(CrisisMetricKind::R0, 0.9, 1.2);
        gauge.update(1, 1.0);
        assert_eq!(gauge.ema, Some(1.0));

        gauge.update(2, 2.0);
        let expected = EMA_ALPHA * 2.0 + (1.0 - EMA_ALPHA) * 1.0;
        assert!((gauge.ema.unwrap() - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn trend_uses_baseline_within_five_ticks() {
        let mut gauge = CrisisGauge::new(CrisisMetricKind::GridStressPct, 70.0, 85.0);
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
}
