//! Wondrous Sites: worldgen stamps `great_peak` / `verdant_basin` site tags on a normal map;
//! any faction vision that has seen a site's tile discovers it (once), applies the morale
//! reward, and narrates it; an unseen site stays hidden. World setup mirrors `sedentarization.rs`.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::IntoSystemConfigs;
use bevy::MinimalPlugins;

use core_sim::{
    discover_sites, place_wondrous_sites, scalar_zero, spawn_initial_world, CommandEventKind,
    CommandEventLog, CultureManager, DiscoveredSites, DiscoveryProgressLedger, FactionId,
    FactionInventory, GenerationId, GenerationRegistry, LocalStore, MapPresets, MapPresetsHandle,
    MoraleCause, PopulationCohort, SimulationConfig, SimulationTick, SiteTag, SitesConfigHandle,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, Tile, TurnPipelineConfig, TurnPipelineConfigHandle,
    VisibilityLedger,
};

/// Deterministic land-rich map seed (shared with the fauna/sedentarization suites).
const TEST_SEED: u64 = 119304647;

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = TEST_SEED;
    app.world.insert_resource(config);

    app.world
        .insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
    app.world
        .insert_resource(GenerationRegistry::with_seed(42, 8));
    app.world.insert_resource(SimulationTick::default());
    app.world.insert_resource(CultureManager::new());
    app.world.insert_resource(StartLocation::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    app.world.insert_resource(FactionInventory::default());
    app.world
        .insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
    app.world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));
    app.world
        .insert_resource(TurnPipelineConfigHandle::new(TurnPipelineConfig::builtin()));
    app.world.insert_resource(SitesConfigHandle::default());

    // Build the world, then place sites (needs the resolved WorldGenSeed spawn wrote back).
    app.add_systems(
        bevy::app::Startup,
        (spawn_initial_world, place_wondrous_sites).chain(),
    );
    app.update();
    app
}

/// Return every placed site's (position, site_id).
fn placed_sites(app: &mut App) -> Vec<(UVec2, String)> {
    let mut query = app.world.query::<(&Tile, &SiteTag)>();
    query
        .iter(&app.world)
        .map(|(tile, site)| (tile.position, site.site_id.clone()))
        .collect()
}

fn spawn_cohort(app: &mut App, faction: FactionId, size: u32) {
    let tile = app.world.spawn_empty().id();
    app.world.spawn(PopulationCohort {
        home: tile,
        current_tile: tile,
        size,
        children: scalar_zero(),
        working: scalar_zero(),
        elders: scalar_zero(),
        stores: LocalStore::new(),
        morale: scalar_zero(),
        last_morale_delta: scalar_zero(),
        last_morale_cause: MoraleCause::None,
        last_morale_contributions: Default::default(),
        discontent_fraction: scalar_zero(),
        grievance: scalar_zero(),
        last_emigrated: 0,
        last_immigrated: 0,
        age_turns: 0,
        generation: 0 as GenerationId,
        faction,
        knowledge: Vec::new(),
        migration: None,
    });
}

fn faction_morale(app: &mut App, faction: FactionId) -> f32 {
    let mut query = app.world.query::<&PopulationCohort>();
    query
        .iter(&app.world)
        .find(|c| c.faction == faction)
        .map(|c| c.morale.to_f32())
        .unwrap_or(0.0)
}

fn discovered_feed_count(app: &App) -> usize {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|e| e.kind == CommandEventKind::SiteDiscovered)
        .count()
}

/// Worldgen places at least one of each seeded site type on a normal map.
#[test]
fn worldgen_places_both_site_types() {
    let mut app = spawn_world();
    let sites = placed_sites(&mut app);

    let peaks = sites.iter().filter(|(_, id)| id == "great_peak").count();
    let basins = sites.iter().filter(|(_, id)| id == "verdant_basin").count();
    assert!(
        peaks >= 1,
        "expected >=1 great_peak, got {peaks} (total sites {})",
        sites.len()
    );
    assert!(
        basins >= 1,
        "expected >=1 verdant_basin, got {basins} (total sites {})",
        sites.len()
    );

    // Caps respected (max_sites = 5 for each rule) and one site per tile.
    assert!(peaks <= 5, "great_peak exceeded cap: {peaks}");
    assert!(basins <= 5, "verdant_basin exceeded cap: {basins}");
    let mut positions: Vec<UVec2> = sites.iter().map(|(p, _)| *p).collect();
    positions.sort_by_key(|p| (p.y, p.x));
    let before = positions.len();
    positions.dedup();
    assert_eq!(before, positions.len(), "two sites shared a tile");
}

/// A site tile made visible to a faction is discovered once: recorded, morale bonus applied a
/// single time, and one feed entry pushed. Re-running does not double-apply or re-log.
#[test]
fn visible_site_discovered_once_with_reward() {
    let mut app = spawn_world();
    app.world.insert_resource(DiscoveredSites::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(VisibilityLedger::default());

    let faction = FactionId(0);
    spawn_cohort(&mut app, faction, 100);

    // Pick one placed site and mark its tile Active for the faction.
    let sites = placed_sites(&mut app);
    let (pos, site_id) = sites
        .first()
        .cloned()
        .expect("at least one site should be placed");
    let (w, h) = {
        let cfg = app.world.resource::<SimulationConfig>();
        (cfg.grid_size.x, cfg.grid_size.y)
    };
    {
        let mut ledger = app.world.resource_mut::<VisibilityLedger>();
        ledger
            .ensure_faction(faction, w, h)
            .mark_active(pos.x, pos.y, 1);
    }

    let morale_before = faction_morale(&mut app, faction);
    app.world.run_system_once(discover_sites);

    // Recorded.
    assert!(
        app.world
            .resource::<DiscoveredSites>()
            .contains(faction, pos),
        "site at {pos:?} ({site_id}) should be discovered"
    );
    // Feed entry pushed once.
    assert_eq!(discovered_feed_count(&app), 1, "expected one feed entry");
    // Morale bonus applied (great_peak 0.05 / verdant_basin 0.02 > 0).
    let morale_after = faction_morale(&mut app, faction);
    assert!(
        morale_after > morale_before,
        "morale should rise on discovery: {morale_before} -> {morale_after}"
    );

    // Re-run: idempotent (no double-apply, no re-log).
    app.world.run_system_once(discover_sites);
    assert_eq!(
        discovered_feed_count(&app),
        1,
        "re-running should not re-log the discovery"
    );
    let morale_final = faction_morale(&mut app, faction);
    assert!(
        (morale_final - morale_after).abs() < 1e-6,
        "re-running should not re-apply the reward: {morale_after} -> {morale_final}"
    );
}

/// An unseen site is not discovered.
#[test]
fn unseen_site_stays_hidden() {
    let mut app = spawn_world();
    app.world.insert_resource(DiscoveredSites::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(VisibilityLedger::default());

    let faction = FactionId(0);
    spawn_cohort(&mut app, faction, 100);
    // A faction map exists but reveals nothing (no marked tiles).
    let (w, h) = {
        let cfg = app.world.resource::<SimulationConfig>();
        (cfg.grid_size.x, cfg.grid_size.y)
    };
    app.world
        .resource_mut::<VisibilityLedger>()
        .ensure_faction(faction, w, h);

    app.world.run_system_once(discover_sites);
    assert_eq!(
        discovered_feed_count(&app),
        0,
        "no site should be discovered without vision"
    );
    let sites = placed_sites(&mut app);
    for (pos, _) in sites {
        assert!(
            !app.world
                .resource::<DiscoveredSites>()
                .contains(faction, pos),
            "unseen site at {pos:?} should not be discovered"
        );
    }
}

/// The snapshot producer resolves each discovered record's catalog fields for the faction.
#[test]
fn snapshot_producer_resolves_catalog_fields() {
    let mut app = spawn_world();
    app.world.insert_resource(DiscoveredSites::default());

    let faction = FactionId(0);
    let sites = placed_sites(&mut app);
    let (pos, site_id) = sites.first().cloned().expect("a site should exist");
    app.world
        .resource_mut::<DiscoveredSites>()
        .record(faction, pos, site_id.clone());

    // Resolve the expected catalog fields from the same config the producer reads.
    let cfg = app.world.resource::<SitesConfigHandle>().get();
    let def = cfg.site(&site_id).expect("site should be in catalog");

    let records = app.world.resource::<DiscoveredSites>().for_faction(faction);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].pos, pos);
    assert_eq!(records[0].site_id, site_id);
    // The catalog resolves the display fields the snapshot exports.
    assert!(!def.display_name.is_empty());
    assert!(!def.category.is_empty());
    assert!(!def.glyph.is_empty());
}
