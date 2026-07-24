//! Governance-section FlatBuffers serialization.

use crate::codec::{create_scalar_raster, FbBuilder};
use crate::state::governance::{
    CorruptionLedger, CorruptionSubsystem, CrisisGaugeState, CrisisMetricKind,
    CrisisOverlayAnnotationState, CrisisOverlayState, CrisisSeverityBand, CrisisTelemetryState,
    CrisisTrendSample, PowerIncidentSeverity, PowerIncidentState, PowerNodeState,
    PowerTelemetryState,
};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::{ForwardsUOffset, WIPOffset};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

pub(crate) fn serialize_governance_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::GovernanceSection<'a>> {
    let power = create_power(builder, &snapshot.power);
    let power_metrics = create_power_metrics(builder, &snapshot.power_metrics);
    let corruption = create_corruption(builder, &snapshot.corruption);
    let corruption_raster = create_scalar_raster(builder, &snapshot.corruption_raster);
    let crisis_telemetry = create_crisis_telemetry(builder, &snapshot.crisis_telemetry);
    let crisis_overlay = create_crisis_overlay(builder, &snapshot.crisis_overlay);
    fb::GovernanceSection::create(
        builder,
        &fb::GovernanceSectionArgs {
            power: Some(power),
            powerMetrics: Some(power_metrics),
            corruption: Some(corruption),
            corruptionRaster: Some(corruption_raster),
            crisisTelemetry: Some(crisis_telemetry),
            crisisOverlay: Some(crisis_overlay),
            removedPower: None,
        },
    )
}

pub(crate) fn serialize_governance_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::GovernanceSection<'a>> {
    let power = create_power(builder, &delta.power);
    let removed_power = builder.create_vector(&delta.removed_power);
    let power_metrics = delta
        .power_metrics
        .as_ref()
        .map(|metrics| create_power_metrics(builder, metrics));
    let corruption = delta
        .corruption
        .as_ref()
        .map(|c| create_corruption(builder, c));
    let corruption_raster = delta
        .corruption_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let crisis_telemetry = delta
        .crisis_telemetry
        .as_ref()
        .map(|telemetry| create_crisis_telemetry(builder, telemetry));
    let crisis_overlay = delta
        .crisis_overlay
        .as_ref()
        .map(|overlay| create_crisis_overlay(builder, overlay));
    fb::GovernanceSection::create(
        builder,
        &fb::GovernanceSectionArgs {
            power: Some(power),
            powerMetrics: power_metrics,
            corruption,
            corruptionRaster: corruption_raster,
            crisisTelemetry: crisis_telemetry,
            crisisOverlay: crisis_overlay,
            removedPower: Some(removed_power),
        },
    )
}

fn create_power<'a>(
    builder: &mut FbBuilder<'a>,
    power_nodes: &[PowerNodeState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PowerNodeState<'a>>>> {
    let offsets: Vec<_> = power_nodes
        .iter()
        .map(|node| {
            fb::PowerNodeState::create(
                builder,
                &fb::PowerNodeStateArgs {
                    entity: node.entity,
                    nodeId: node.node_id,
                    generation: node.generation,
                    demand: node.demand,
                    efficiency: node.efficiency,
                    storageLevel: node.storage_level,
                    storageCapacity: node.storage_capacity,
                    stability: node.stability,
                    surplus: node.surplus,
                    deficit: node.deficit,
                    incidentCount: node.incident_count,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_power_incidents<'a>(
    builder: &mut FbBuilder<'a>,
    incidents: &[PowerIncidentState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PowerIncidentState<'a>>>> {
    let offsets: Vec<_> = incidents
        .iter()
        .map(|incident| {
            fb::PowerIncidentState::create(
                builder,
                &fb::PowerIncidentStateArgs {
                    nodeId: incident.node_id,
                    severity: match incident.severity {
                        PowerIncidentSeverity::Warning => fb::PowerIncidentSeverity::Warning,
                        PowerIncidentSeverity::Critical => fb::PowerIncidentSeverity::Critical,
                    },
                    deficit: incident.deficit,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_power_metrics<'a>(
    builder: &mut FbBuilder<'a>,
    metrics: &PowerTelemetryState,
) -> WIPOffset<fb::PowerTelemetryState<'a>> {
    let incidents = create_power_incidents(builder, &metrics.incidents);
    fb::PowerTelemetryState::create(
        builder,
        &fb::PowerTelemetryStateArgs {
            totalSupply: metrics.total_supply,
            totalDemand: metrics.total_demand,
            totalStorage: metrics.total_storage,
            totalCapacity: metrics.total_capacity,
            gridStressAvg: metrics.grid_stress_avg,
            surplusMargin: metrics.surplus_margin,
            instabilityAlerts: metrics.instability_alerts,
            incidents: Some(incidents),
        },
    )
}

fn to_fb_crisis_metric_kind(kind: CrisisMetricKind) -> fb::CrisisMetricKind {
    match kind {
        CrisisMetricKind::R0 => fb::CrisisMetricKind::R0,
        CrisisMetricKind::GridStressPct => fb::CrisisMetricKind::GridStressPct,
        CrisisMetricKind::UnauthorizedQueuePct => fb::CrisisMetricKind::UnauthorizedQueuePct,
        CrisisMetricKind::SwarmsActive => fb::CrisisMetricKind::SwarmsActive,
        CrisisMetricKind::PhageDensity => fb::CrisisMetricKind::PhageDensity,
    }
}

fn to_fb_crisis_severity_band(band: CrisisSeverityBand) -> fb::CrisisSeverityBand {
    match band {
        CrisisSeverityBand::Safe => fb::CrisisSeverityBand::Safe,
        CrisisSeverityBand::Warn => fb::CrisisSeverityBand::Warn,
        CrisisSeverityBand::Critical => fb::CrisisSeverityBand::Critical,
    }
}

fn create_crisis_trend_samples<'a>(
    builder: &mut FbBuilder<'a>,
    samples: &[CrisisTrendSample],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CrisisTrendSample<'a>>>> {
    let offsets: Vec<_> = samples
        .iter()
        .map(|sample| {
            fb::CrisisTrendSample::create(
                builder,
                &fb::CrisisTrendSampleArgs {
                    tick: sample.tick,
                    value: sample.value,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_crisis_gauges<'a>(
    builder: &mut FbBuilder<'a>,
    gauges: &[CrisisGaugeState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CrisisGaugeState<'a>>>> {
    let offsets: Vec<_> = gauges
        .iter()
        .map(|gauge| {
            let history = create_crisis_trend_samples(builder, &gauge.history);
            fb::CrisisGaugeState::create(
                builder,
                &fb::CrisisGaugeStateArgs {
                    kind: to_fb_crisis_metric_kind(gauge.kind),
                    raw: gauge.raw,
                    ema: gauge.ema,
                    trend5t: gauge.trend_5t,
                    warnThreshold: gauge.warn_threshold,
                    criticalThreshold: gauge.critical_threshold,
                    lastUpdatedTick: gauge.last_updated_tick,
                    staleTicks: gauge.stale_ticks,
                    band: to_fb_crisis_severity_band(gauge.band),
                    history: Some(history),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_crisis_telemetry<'a>(
    builder: &mut FbBuilder<'a>,
    telemetry: &CrisisTelemetryState,
) -> WIPOffset<fb::CrisisTelemetryState<'a>> {
    let gauges = create_crisis_gauges(builder, &telemetry.gauges);
    fb::CrisisTelemetryState::create(
        builder,
        &fb::CrisisTelemetryStateArgs {
            gauges: Some(gauges),
            modifiersActive: telemetry.modifiers_active,
            foreshockIncidents: telemetry.foreshock_incidents,
            containmentIncidents: telemetry.containment_incidents,
            warningsActive: telemetry.warnings_active,
            criticalsActive: telemetry.criticals_active,
        },
    )
}

fn create_crisis_overlay_annotations<'a>(
    builder: &mut FbBuilder<'a>,
    annotations: &[CrisisOverlayAnnotationState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CrisisOverlayAnnotationState<'a>>>> {
    let offsets: Vec<_> = annotations
        .iter()
        .map(|annotation| {
            let path = builder.create_vector(&annotation.path);
            let label = builder.create_string(&annotation.label);
            fb::CrisisOverlayAnnotationState::create(
                builder,
                &fb::CrisisOverlayAnnotationStateArgs {
                    label: Some(label),
                    severity: to_fb_crisis_severity_band(annotation.severity),
                    path: Some(path),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_crisis_overlay<'a>(
    builder: &mut FbBuilder<'a>,
    overlay: &CrisisOverlayState,
) -> WIPOffset<fb::CrisisOverlayState<'a>> {
    let heatmap = create_scalar_raster(builder, &overlay.heatmap);
    let annotations = create_crisis_overlay_annotations(builder, &overlay.annotations);
    fb::CrisisOverlayState::create(
        builder,
        &fb::CrisisOverlayStateArgs {
            heatmap: Some(heatmap),
            annotations: Some(annotations),
        },
    )
}

fn create_corruption<'a>(
    builder: &mut FbBuilder<'a>,
    ledger: &CorruptionLedger,
) -> WIPOffset<fb::CorruptionLedger<'a>> {
    let entries: Vec<_> = ledger
        .entries
        .iter()
        .map(|entry| {
            fb::CorruptionEntry::create(
                builder,
                &fb::CorruptionEntryArgs {
                    subsystem: to_fb_corruption_subsystem(entry.subsystem),
                    intensity: entry.intensity,
                    incidentId: entry.incident_id,
                    exposureTimer: entry.exposure_timer,
                    restitutionWindow: entry.restitution_window,
                    lastUpdateTick: entry.last_update_tick,
                },
            )
        })
        .collect();
    let entries_vec = builder.create_vector(&entries);
    fb::CorruptionLedger::create(
        builder,
        &fb::CorruptionLedgerArgs {
            entries: Some(entries_vec),
            reputationModifier: ledger.reputation_modifier,
            auditCapacity: ledger.audit_capacity,
        },
    )
}

fn to_fb_corruption_subsystem(subsystem: CorruptionSubsystem) -> fb::CorruptionSubsystem {
    match subsystem {
        CorruptionSubsystem::Logistics => fb::CorruptionSubsystem::Logistics,
        CorruptionSubsystem::Trade => fb::CorruptionSubsystem::Trade,
        CorruptionSubsystem::Military => fb::CorruptionSubsystem::Military,
        CorruptionSubsystem::Governance => fb::CorruptionSubsystem::Governance,
    }
}
