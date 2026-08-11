use super::*;

#[derive(Event, Debug, Clone)]
pub struct TradeDiffusionEvent {
    pub tick: u64,
    pub from: FactionId,
    pub to: FactionId,
    pub discovery_id: u32,
    pub delta: Scalar,
    pub via_migration: bool,
}

/// Relax material temperatures using deterministic rules. The relaxation target is the tile's
/// latitude + elevation + jitter climate temperature (recomputed deterministically from its
/// position/elevation/element), so the field converges to the climate model rather than the old
/// element checkerboard. Worldgen seeds each tile at exactly this value, so turn 1 has no jump.
///
/// This used to move `Tile.mass` down the same loop; the mass economy was demolished with the rest
/// of the dead logistics slice (`docs/plan_contact_and_logistics.md` §As-built) and the temperature
/// half — which population cold-morale, sites and power all read — is what remains.
pub fn simulate_materials(
    config: Res<SimulationConfig>,
    elevation: Res<ElevationField>,
    mut tiles: Query<&mut Tile>,
) {
    let grid_height = config.grid_size.y;
    for mut tile in tiles.iter_mut() {
        let above_sea = elevation.above_sea_normalized(tile.position.x, tile.position.y);
        let target = climate_temperature(
            tile.position.y,
            grid_height,
            above_sea,
            tile.element,
            &config.climate,
        );
        let delta = (target - tile.temperature) * config.temperature_lerp;
        let conductivity = tile.element.conductivity();
        tile.temperature += delta * conductivity;
    }
}

/// Publish trade telemetry counters for downstream logging/metrics.
pub fn publish_trade_telemetry(telemetry: Res<TradeTelemetry>, tick: Res<SimulationTick>) {
    let snapshot = json!({
        "tick": tick.0,
        "tech_diffusion_applied": telemetry.tech_diffusion_applied,
        "migration_transfers": telemetry.migration_transfers,
        "records": telemetry
            .records
            .iter()
            .take(24)
            .map(|record| {
                    json!({
                        "from": record.from.0,
                        "to": record.to.0,
                        "discovery": record.discovery_id,
                        "delta": record.delta.to_f32(),
                        "via_migration": record.via_migration,
                        "herd_density": record.herd_density,
                    })
            })
            .collect::<Vec<_>>(),
        "records_truncated": telemetry.records.len().saturating_sub(24),
    });

    match serde_json::to_string(&snapshot) {
        Ok(payload) => debug!("trade.telemetry {}", payload),
        Err(_) => debug!(
            "trade.telemetry tick={} trade.tech_diffusion_applied={} trade.migration_transfers={} records={}",
            tick.0,
            telemetry.tech_diffusion_applied,
            telemetry.migration_transfers,
            telemetry.records.len()
        ),
    }
}
