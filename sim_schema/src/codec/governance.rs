//! Governance-section FlatBuffers serialization.

use crate::codec::{
    create_scalar_raster, decode_rows, decode_scalar_raster, decode_scalars, map_rows, text,
    unknown_enum, DecodeError, FbBuilder,
};
use crate::state::governance::{
    CorruptionEntry, CorruptionLedger, CorruptionSubsystem, CrisisGaugeState, CrisisMetricKind,
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
                    trendPer100t: gauge.trend_per_100t,
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

// ---------------------------------------------------------------------------
// Decoders — the inverse of every `create_*` / `to_fb_*` above, in the same order.
// ---------------------------------------------------------------------------

pub(crate) fn decode_governance_section(
    section: fb::GovernanceSection<'_>,
    snapshot: &mut WorldSnapshot,
) -> Result<(), DecodeError> {
    snapshot.power = map_rows(section.power(), decode_power_node);
    snapshot.power_metrics = section
        .powerMetrics()
        .map(decode_power_metrics)
        .transpose()?
        .unwrap_or_default();
    snapshot.corruption = section
        .corruption()
        .map(decode_corruption)
        .transpose()?
        .unwrap_or_default();
    snapshot.corruption_raster = section
        .corruptionRaster()
        .map(decode_scalar_raster)
        .unwrap_or_default();
    snapshot.crisis_telemetry = section
        .crisisTelemetry()
        .map(decode_crisis_telemetry)
        .transpose()?
        .unwrap_or_default();
    snapshot.crisis_overlay = section
        .crisisOverlay()
        .map(decode_crisis_overlay)
        .transpose()?
        .unwrap_or_default();
    Ok(())
}

pub(crate) fn decode_governance_section_delta(
    section: fb::GovernanceSection<'_>,
    delta: &mut WorldDelta,
) -> Result<(), DecodeError> {
    delta.power = map_rows(section.power(), decode_power_node);
    delta.removed_power = decode_scalars(section.removedPower());
    delta.power_metrics = section
        .powerMetrics()
        .map(decode_power_metrics)
        .transpose()?;
    delta.corruption = section.corruption().map(decode_corruption).transpose()?;
    delta.corruption_raster = section.corruptionRaster().map(decode_scalar_raster);
    delta.crisis_telemetry = section
        .crisisTelemetry()
        .map(decode_crisis_telemetry)
        .transpose()?;
    delta.crisis_overlay = section
        .crisisOverlay()
        .map(decode_crisis_overlay)
        .transpose()?;
    Ok(())
}

fn decode_power_node(node: fb::PowerNodeState<'_>) -> PowerNodeState {
    PowerNodeState {
        entity: node.entity(),
        node_id: node.nodeId(),
        generation: node.generation(),
        demand: node.demand(),
        efficiency: node.efficiency(),
        storage_level: node.storageLevel(),
        storage_capacity: node.storageCapacity(),
        stability: node.stability(),
        surplus: node.surplus(),
        deficit: node.deficit(),
        incident_count: node.incidentCount(),
    }
}

fn to_state_power_incident_severity(
    severity: fb::PowerIncidentSeverity,
) -> Result<PowerIncidentSeverity, DecodeError> {
    Ok(match severity {
        fb::PowerIncidentSeverity::Warning => PowerIncidentSeverity::Warning,
        fb::PowerIncidentSeverity::Critical => PowerIncidentSeverity::Critical,
        other => return Err(unknown_enum("PowerIncidentSeverity", other.0)),
    })
}

fn decode_power_metrics(
    metrics: fb::PowerTelemetryState<'_>,
) -> Result<PowerTelemetryState, DecodeError> {
    Ok(PowerTelemetryState {
        total_supply: metrics.totalSupply(),
        total_demand: metrics.totalDemand(),
        total_storage: metrics.totalStorage(),
        total_capacity: metrics.totalCapacity(),
        grid_stress_avg: metrics.gridStressAvg(),
        surplus_margin: metrics.surplusMargin(),
        instability_alerts: metrics.instabilityAlerts(),
        incidents: decode_rows(metrics.incidents(), |incident| {
            Ok(PowerIncidentState {
                node_id: incident.nodeId(),
                severity: to_state_power_incident_severity(incident.severity())?,
                deficit: incident.deficit(),
            })
        })?,
    })
}

fn to_state_crisis_metric_kind(
    kind: fb::CrisisMetricKind,
) -> Result<CrisisMetricKind, DecodeError> {
    Ok(match kind {
        fb::CrisisMetricKind::R0 => CrisisMetricKind::R0,
        fb::CrisisMetricKind::GridStressPct => CrisisMetricKind::GridStressPct,
        fb::CrisisMetricKind::UnauthorizedQueuePct => CrisisMetricKind::UnauthorizedQueuePct,
        fb::CrisisMetricKind::SwarmsActive => CrisisMetricKind::SwarmsActive,
        fb::CrisisMetricKind::PhageDensity => CrisisMetricKind::PhageDensity,
        other => return Err(unknown_enum("CrisisMetricKind", other.0)),
    })
}

fn to_state_crisis_severity_band(
    band: fb::CrisisSeverityBand,
) -> Result<CrisisSeverityBand, DecodeError> {
    Ok(match band {
        fb::CrisisSeverityBand::Safe => CrisisSeverityBand::Safe,
        fb::CrisisSeverityBand::Warn => CrisisSeverityBand::Warn,
        fb::CrisisSeverityBand::Critical => CrisisSeverityBand::Critical,
        other => return Err(unknown_enum("CrisisSeverityBand", other.0)),
    })
}

fn decode_crisis_telemetry(
    telemetry: fb::CrisisTelemetryState<'_>,
) -> Result<CrisisTelemetryState, DecodeError> {
    Ok(CrisisTelemetryState {
        gauges: decode_rows(telemetry.gauges(), |gauge| {
            Ok(CrisisGaugeState {
                kind: to_state_crisis_metric_kind(gauge.kind())?,
                raw: gauge.raw(),
                ema: gauge.ema(),
                trend_per_100t: gauge.trendPer100t(),
                warn_threshold: gauge.warnThreshold(),
                critical_threshold: gauge.criticalThreshold(),
                last_updated_tick: gauge.lastUpdatedTick(),
                stale_ticks: gauge.staleTicks(),
                band: to_state_crisis_severity_band(gauge.band())?,
                history: map_rows(gauge.history(), |sample| CrisisTrendSample {
                    tick: sample.tick(),
                    value: sample.value(),
                }),
            })
        })?,
        modifiers_active: telemetry.modifiersActive(),
        foreshock_incidents: telemetry.foreshockIncidents(),
        containment_incidents: telemetry.containmentIncidents(),
        warnings_active: telemetry.warningsActive(),
        criticals_active: telemetry.criticalsActive(),
    })
}

fn decode_crisis_overlay(
    overlay: fb::CrisisOverlayState<'_>,
) -> Result<CrisisOverlayState, DecodeError> {
    Ok(CrisisOverlayState {
        heatmap: overlay
            .heatmap()
            .map(decode_scalar_raster)
            .unwrap_or_default(),
        annotations: decode_rows(overlay.annotations(), |annotation| {
            Ok(CrisisOverlayAnnotationState {
                label: text(annotation.label()),
                severity: to_state_crisis_severity_band(annotation.severity())?,
                path: decode_scalars(annotation.path()),
            })
        })?,
    })
}

fn decode_corruption(ledger: fb::CorruptionLedger<'_>) -> Result<CorruptionLedger, DecodeError> {
    Ok(CorruptionLedger {
        entries: decode_rows(ledger.entries(), |entry| {
            Ok(CorruptionEntry {
                subsystem: to_state_corruption_subsystem(entry.subsystem())?,
                intensity: entry.intensity(),
                incident_id: entry.incidentId(),
                exposure_timer: entry.exposureTimer(),
                restitution_window: entry.restitutionWindow(),
                last_update_tick: entry.lastUpdateTick(),
            })
        })?,
        reputation_modifier: ledger.reputationModifier(),
        audit_capacity: ledger.auditCapacity(),
    })
}

fn to_state_corruption_subsystem(
    subsystem: fb::CorruptionSubsystem,
) -> Result<CorruptionSubsystem, DecodeError> {
    Ok(match subsystem {
        fb::CorruptionSubsystem::Logistics => CorruptionSubsystem::Logistics,
        fb::CorruptionSubsystem::Trade => CorruptionSubsystem::Trade,
        fb::CorruptionSubsystem::Military => CorruptionSubsystem::Military,
        fb::CorruptionSubsystem::Governance => CorruptionSubsystem::Governance,
        other => return Err(unknown_enum("CorruptionSubsystem", other.0)),
    })
}

#[cfg(test)]
mod enum_round_trip_tests {
    use super::*;

    /// Every variant the encoder can write, the decoder reads back as the same variant.
    #[test]
    fn every_governance_enum_variant_round_trips() {
        for kind in CrisisMetricKind::VALUES {
            assert_eq!(
                to_state_crisis_metric_kind(to_fb_crisis_metric_kind(kind)).expect("known"),
                kind
            );
        }
        for band in [
            CrisisSeverityBand::Safe,
            CrisisSeverityBand::Warn,
            CrisisSeverityBand::Critical,
        ] {
            assert_eq!(
                to_state_crisis_severity_band(to_fb_crisis_severity_band(band)).expect("known"),
                band
            );
        }
        for subsystem in [
            CorruptionSubsystem::Logistics,
            CorruptionSubsystem::Trade,
            CorruptionSubsystem::Military,
            CorruptionSubsystem::Governance,
        ] {
            assert_eq!(
                to_state_corruption_subsystem(to_fb_corruption_subsystem(subsystem))
                    .expect("known"),
                subsystem
            );
        }
        for severity in [
            PowerIncidentSeverity::Warning,
            PowerIncidentSeverity::Critical,
        ] {
            let wire = match severity {
                PowerIncidentSeverity::Warning => fb::PowerIncidentSeverity::Warning,
                PowerIncidentSeverity::Critical => fb::PowerIncidentSeverity::Critical,
            };
            assert_eq!(
                to_state_power_incident_severity(wire).expect("known"),
                severity
            );
        }
    }
}
