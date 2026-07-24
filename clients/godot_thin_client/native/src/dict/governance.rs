//! `governance` section -- power nodes and incidents, the crisis gauges/overlay, and
//! the corruption ledger.

use flatbuffers::Vector;
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

use crate::dict::{fixed64_to_f32, fixed64_to_f64};
use crate::snapshot::delta::CrisisAnnotationRecord;
use crate::snapshot::raster::packed_from_slice;

fn corruption_subsystem_label(subsystem: fb::CorruptionSubsystem) -> &'static str {
    match subsystem {
        fb::CorruptionSubsystem::Logistics => "Logistics",
        fb::CorruptionSubsystem::Trade => "Trade",
        fb::CorruptionSubsystem::Military => "Military",
        fb::CorruptionSubsystem::Governance => "Governance",
        _ => "Unknown",
    }
}

fn corruption_entry_to_dict(entry: fb::CorruptionEntry<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("subsystem", corruption_subsystem_label(entry.subsystem()));
    let _ = dict.insert("intensity", fixed64_to_f64(entry.intensity()));
    let _ = dict.insert("incident_id", entry.incidentId() as i64);
    let _ = dict.insert("exposure_timer", entry.exposureTimer() as i64);
    let _ = dict.insert("restitution_window", entry.restitutionWindow() as i64);
    let _ = dict.insert("last_update_tick", entry.lastUpdateTick() as i64);
    dict
}

pub(crate) fn corruption_to_dict(ledger: fb::CorruptionLedger<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let mut entries = VarArray::new();
    if let Some(list) = ledger.entries() {
        for entry in list {
            let dict = corruption_entry_to_dict(entry);
            let variant = dict.to_variant();
            entries.push(&variant);
        }
    }
    let _ = dict.insert("entries", &entries);
    let _ = dict.insert(
        "reputation_modifier",
        fixed64_to_f64(ledger.reputationModifier()),
    );
    let _ = dict.insert("audit_capacity", ledger.auditCapacity() as i64);
    dict
}

fn power_node_to_dict(node: fb::PowerNodeState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("entity", node.entity() as i64);
    let _ = dict.insert("node_id", node.nodeId() as i64);

    let generation_raw = node.generation();
    let demand_raw = node.demand();
    let efficiency_raw = node.efficiency();
    let storage_level_raw = node.storageLevel();
    let storage_capacity_raw = node.storageCapacity();
    let stability_raw = node.stability();
    let surplus_raw = node.surplus();
    let deficit_raw = node.deficit();

    let _ = dict.insert("generation", fixed64_to_f64(generation_raw));
    let _ = dict.insert("generation_raw", generation_raw);
    let _ = dict.insert("demand", fixed64_to_f64(demand_raw));
    let _ = dict.insert("demand_raw", demand_raw);
    let _ = dict.insert("efficiency", fixed64_to_f64(efficiency_raw));
    let _ = dict.insert("efficiency_raw", efficiency_raw);
    let _ = dict.insert("storage_level", fixed64_to_f64(storage_level_raw));
    let _ = dict.insert("storage_level_raw", storage_level_raw);
    let _ = dict.insert("storage_capacity", fixed64_to_f64(storage_capacity_raw));
    let _ = dict.insert("storage_capacity_raw", storage_capacity_raw);
    let _ = dict.insert("stability", fixed64_to_f64(stability_raw));
    let _ = dict.insert("stability_raw", stability_raw);
    let _ = dict.insert("surplus", fixed64_to_f64(surplus_raw));
    let _ = dict.insert("surplus_raw", surplus_raw);
    let _ = dict.insert("deficit", fixed64_to_f64(deficit_raw));
    let _ = dict.insert("deficit_raw", deficit_raw);
    let _ = dict.insert("incident_count", node.incidentCount() as i64);

    dict
}

pub(crate) fn power_nodes_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::PowerNodeState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for node in list {
        let dict = power_node_to_dict(node);
        array.push(&dict.to_variant());
    }
    array
}

fn power_incident_to_dict(incident: fb::PowerIncidentState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("node_id", incident.nodeId() as i64);
    let severity = match incident.severity() {
        fb::PowerIncidentSeverity::Critical => "critical",
        _ => "warning",
    };
    let _ = dict.insert("severity", severity);
    let deficit_raw = incident.deficit();
    let _ = dict.insert("deficit", fixed64_to_f64(deficit_raw));
    let _ = dict.insert("deficit_raw", deficit_raw);
    dict
}

pub(crate) fn power_metrics_to_dict(metrics: fb::PowerTelemetryState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let total_supply_raw = metrics.totalSupply();
    let total_demand_raw = metrics.totalDemand();
    let total_storage_raw = metrics.totalStorage();
    let total_capacity_raw = metrics.totalCapacity();
    let _ = dict.insert("total_supply", fixed64_to_f64(total_supply_raw));
    let _ = dict.insert("total_supply_raw", total_supply_raw);
    let _ = dict.insert("total_demand", fixed64_to_f64(total_demand_raw));
    let _ = dict.insert("total_demand_raw", total_demand_raw);
    let _ = dict.insert("total_storage", fixed64_to_f64(total_storage_raw));
    let _ = dict.insert("total_storage_raw", total_storage_raw);
    let _ = dict.insert("total_capacity", fixed64_to_f64(total_capacity_raw));
    let _ = dict.insert("total_capacity_raw", total_capacity_raw);
    let _ = dict.insert("grid_stress_avg", metrics.gridStressAvg() as f64);
    let _ = dict.insert("surplus_margin", metrics.surplusMargin() as f64);
    let _ = dict.insert("instability_alerts", metrics.instabilityAlerts() as i64);

    let mut incidents_array = VarArray::new();
    if let Some(incidents) = metrics.incidents() {
        for incident in incidents {
            let dict = power_incident_to_dict(incident);
            incidents_array.push(&dict.to_variant());
        }
    }
    let _ = dict.insert("incidents", &incidents_array);

    dict
}

fn crisis_metric_kind_to_str(kind: fb::CrisisMetricKind) -> &'static str {
    match kind {
        fb::CrisisMetricKind::R0 => "r0",
        fb::CrisisMetricKind::GridStressPct => "grid_stress_pct",
        fb::CrisisMetricKind::UnauthorizedQueuePct => "unauthorized_queue_pct",
        fb::CrisisMetricKind::SwarmsActive => "swarms_active",
        fb::CrisisMetricKind::PhageDensity => "phage_density",
        _ => "unknown",
    }
}

fn crisis_metric_label(kind: fb::CrisisMetricKind) -> &'static str {
    match kind {
        fb::CrisisMetricKind::R0 => "R₀",
        fb::CrisisMetricKind::GridStressPct => "Grid Stress %",
        fb::CrisisMetricKind::UnauthorizedQueuePct => "Unauthorized Queue %",
        fb::CrisisMetricKind::SwarmsActive => "Swarms Active",
        fb::CrisisMetricKind::PhageDensity => "Phage Density",
        _ => "Metric",
    }
}

fn crisis_severity_band_to_str(band: fb::CrisisSeverityBand) -> &'static str {
    match band {
        fb::CrisisSeverityBand::Warn => "warn",
        fb::CrisisSeverityBand::Critical => "critical",
        _ => "safe",
    }
}

fn crisis_history_to_array(
    history: Vector<'_, flatbuffers::ForwardsUOffset<fb::CrisisTrendSample<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for sample in history {
        let mut entry = VarDictionary::new();
        let _ = entry.insert("tick", sample.tick() as i64);
        let _ = entry.insert("value", sample.value());
        array.push(&entry.to_variant());
    }
    array
}

fn crisis_gauge_to_dict(gauge: fb::CrisisGaugeState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let kind = gauge.kind();
    let _ = dict.insert("kind", crisis_metric_kind_to_str(kind));
    let _ = dict.insert("label", crisis_metric_label(kind));
    let _ = dict.insert("raw", gauge.raw());
    let _ = dict.insert("ema", gauge.ema());
    let _ = dict.insert("trend_5t", gauge.trend5t());
    let _ = dict.insert("warn_threshold", gauge.warnThreshold());
    let _ = dict.insert("critical_threshold", gauge.criticalThreshold());
    let _ = dict.insert("last_updated_tick", gauge.lastUpdatedTick() as i64);
    let _ = dict.insert("stale_ticks", gauge.staleTicks() as i64);
    let _ = dict.insert("band", crisis_severity_band_to_str(gauge.band()));
    if let Some(history) = gauge.history() {
        let _ = dict.insert("history", &crisis_history_to_array(history));
    } else {
        let _ = dict.insert("history", &VarArray::new());
    }
    dict
}

pub(crate) fn crisis_telemetry_to_dict(telemetry: fb::CrisisTelemetryState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let mut gauges_array = VarArray::new();
    if let Some(gauges) = telemetry.gauges() {
        for gauge in gauges {
            let dict = crisis_gauge_to_dict(gauge);
            gauges_array.push(&dict.to_variant());
        }
    }
    let _ = dict.insert("gauges", &gauges_array);
    let _ = dict.insert("modifiers_active", telemetry.modifiersActive() as i64);
    let _ = dict.insert("foreshock_incidents", telemetry.foreshockIncidents() as i64);
    let _ = dict.insert(
        "containment_incidents",
        telemetry.containmentIncidents() as i64,
    );
    let _ = dict.insert("warnings_active", telemetry.warningsActive() as i64);
    let _ = dict.insert("criticals_active", telemetry.criticalsActive() as i64);
    dict
}

pub(crate) fn crisis_overlay_to_dict(overlay: fb::CrisisOverlayState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let mut heatmap_dict = VarDictionary::new();
    if let Some(raster) = overlay.heatmap() {
        let width = raster.width();
        let height = raster.height();
        let mut data = Vec::new();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            data = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    data[idx] = fixed64_to_f32(value);
                }
            }
        }
        let _ = heatmap_dict.insert("width", width as i64);
        let _ = heatmap_dict.insert("height", height as i64);
        let _ = heatmap_dict.insert("samples", &packed_from_slice(&data));
    } else {
        let _ = heatmap_dict.insert("width", 0);
        let _ = heatmap_dict.insert("height", 0);
        let _ = heatmap_dict.insert("samples", &PackedFloat32Array::new());
    }
    let _ = dict.insert("heatmap", &heatmap_dict);

    let mut annotations = VarArray::new();
    if let Some(entries) = overlay.annotations() {
        for entry in entries {
            let mut annotation = VarDictionary::new();
            if let Some(label) = entry.label() {
                let _ = annotation.insert("label", label);
            }
            let _ = annotation.insert("severity", crisis_severity_band_to_str(entry.severity()));
            if let Some(path) = entry.path() {
                let mut packed = PackedInt32Array::new();
                packed.resize(path.len());
                let slice = packed.as_mut_slice();
                for (idx, value) in path.iter().enumerate() {
                    slice[idx] = value as i32;
                }
                let _ = annotation.insert("path", &packed);
            } else {
                let _ = annotation.insert("path", &PackedInt32Array::new());
            }
            annotations.push(&annotation.to_variant());
        }
    }
    let _ = dict.insert("annotations", &annotations);
    dict
}

pub(crate) fn crisis_annotation_to_dict(record: &CrisisAnnotationRecord) -> VarDictionary {
    let mut dict = VarDictionary::new();
    if let Some(label) = &record.label {
        let _ = dict.insert("label", label.clone());
    }
    let _ = dict.insert("severity", crisis_severity_band_to_str(record.severity));
    if record.path.is_empty() {
        let _ = dict.insert("path", &PackedInt32Array::new());
    } else {
        let mut packed = PackedInt32Array::new();
        packed.resize(record.path.len());
        let slice = packed.as_mut_slice();
        slice.copy_from_slice(&record.path);
        let _ = dict.insert("path", &packed);
    }
    dict
}
