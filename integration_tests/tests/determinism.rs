mod common;

use core_sim::{build_headless_app, SimulationConfig, SimulationConfigMetadata, SnapshotHistory};
use sim_runtime::WorldSnapshot;

fn run_simulation(ticks: usize) -> WorldSnapshot {
    common::ensure_test_config();
    let mut app = build_headless_app();
    if let Some(mut metadata) = app.world.get_resource_mut::<SimulationConfigMetadata>() {
        metadata.set_seed_random(false);
    }
    if let Some(mut config) = app.world.get_resource_mut::<SimulationConfig>() {
        if config.map_seed == 0 {
            config.map_seed = 0x5EED_F00D;
        }
    }
    for _ in 0..ticks {
        app.update();
    }
    app.world
        .resource::<SnapshotHistory>()
        .last_snapshot
        .as_ref()
        .map(|snapshot| (**snapshot).clone())
        .expect("snapshot available")
}

// Keep tick count low so CI doesn't spend minutes marching the full simulation.
// Half a dozen updates is sufficient to populate the snapshot history for comparison.
const SNAPSHOT_TICKS: usize = 6;

#[test]
fn deterministic_snapshots_match() {
    let snapshot_a = run_simulation(SNAPSHOT_TICKS);
    let snapshot_b = run_simulation(SNAPSHOT_TICKS);

    let mut normalized_a = snapshot_a.clone();
    normalized_a.influencers.clear();
    normalized_a.header.hash = 0;

    let mut normalized_b = snapshot_b.clone();
    normalized_b.influencers.clear();
    normalized_b.header.hash = 0;

    assert_eq!(
        sim_runtime::hash_snapshot(&normalized_a),
        sim_runtime::hash_snapshot(&normalized_b)
    );

    assert_eq!(
        snapshot_a.header.population_count,
        snapshot_b.header.population_count
    );
    assert_eq!(snapshot_a.header.power_count, snapshot_b.header.power_count);
    assert_eq!(
        snapshot_a.header.influencer_count,
        snapshot_b.header.influencer_count
    );
    assert_eq!(
        snapshot_a.header.campaign_label,
        snapshot_b.header.campaign_label
    );
    assert_eq!(snapshot_a.tiles, snapshot_b.tiles);
    assert_eq!(snapshot_a.logistics, snapshot_b.logistics);
    assert_eq!(snapshot_a.populations, snapshot_b.populations);
    assert_eq!(snapshot_a.power, snapshot_b.power);
    assert_eq!(snapshot_a.axis_bias, snapshot_b.axis_bias);
    assert_eq!(snapshot_a.sentiment, snapshot_b.sentiment);
    assert_eq!(snapshot_a.influencers.len(), snapshot_b.influencers.len());
    assert_eq!(snapshot_a.generations, snapshot_b.generations);
    assert_eq!(snapshot_a.corruption, snapshot_b.corruption);
    assert_eq!(snapshot_a.trade_links, snapshot_b.trade_links);
    assert_eq!(snapshot_a.power_metrics, snapshot_b.power_metrics);
    assert_eq!(
        snapshot_a.great_discovery_definitions,
        snapshot_b.great_discovery_definitions
    );
    assert_eq!(snapshot_a.great_discoveries, snapshot_b.great_discoveries);
    assert_eq!(
        snapshot_a.great_discovery_progress,
        snapshot_b.great_discovery_progress
    );
    assert_eq!(
        snapshot_a.great_discovery_telemetry,
        snapshot_b.great_discovery_telemetry
    );
    assert_eq!(snapshot_a.knowledge_ledger, snapshot_b.knowledge_ledger);
    assert_eq!(snapshot_a.sentiment_raster, snapshot_b.sentiment_raster);
    assert_eq!(snapshot_a.corruption_raster, snapshot_b.corruption_raster);
    assert_eq!(snapshot_a.fog_raster, snapshot_b.fog_raster);
    assert_eq!(snapshot_a.culture_raster, snapshot_b.culture_raster);
    assert_eq!(snapshot_a.military_raster, snapshot_b.military_raster);
    assert_eq!(snapshot_a.culture_layers, snapshot_b.culture_layers);
    assert_eq!(snapshot_a.culture_tensions, snapshot_b.culture_tensions);
    assert_eq!(snapshot_a.discovery_progress, snapshot_b.discovery_progress);
    assert_eq!(snapshot_a.knowledge_metrics, snapshot_b.knowledge_metrics);
    assert_eq!(snapshot_a.knowledge_timeline, snapshot_b.knowledge_timeline);
    assert_eq!(snapshot_a.victory, snapshot_b.victory);
    assert_eq!(snapshot_a.crisis_telemetry, snapshot_b.crisis_telemetry);
    assert_eq!(snapshot_a.crisis_overlay, snapshot_b.crisis_overlay);
    assert_eq!(snapshot_a.campaign_profiles, snapshot_b.campaign_profiles);
    assert_eq!(snapshot_a.command_events, snapshot_b.command_events);
    assert_eq!(snapshot_a.herds, snapshot_b.herds);
    assert_eq!(snapshot_a.food_modules, snapshot_b.food_modules);
    assert_eq!(snapshot_a.faction_inventory, snapshot_b.faction_inventory);
    assert_eq!(snapshot_a.moisture_raster, snapshot_b.moisture_raster);
    assert_eq!(snapshot_a.hydrology_overlay, snapshot_b.hydrology_overlay);
    assert_eq!(snapshot_a.elevation_overlay, snapshot_b.elevation_overlay);
    assert_eq!(snapshot_a.start_marker, snapshot_b.start_marker);
    assert_eq!(snapshot_a.terrain, snapshot_b.terrain);
    assert_eq!(snapshot_a.logistics_raster, snapshot_b.logistics_raster);
}
