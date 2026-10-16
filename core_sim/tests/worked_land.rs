//! **WORKED LAND LOOSENS THE FARM'S SITE RULE - IT DOES NOT ADD A STEP.** The plant ladder ends at the
//! Farm (`plant:farm`). Working a Farm teaches `worked_land`, and a people that holds the lesson may
//! `sow` on fertile, fresh-watered ground that is NOT a gathering site (`site_relaxations` on the
//! rung, resolved in the one `forage::rung_site_refusal` seam every gate and the wire share).
//!
//! The standard map (`earthlike`, 80x52, seed 119304647) is pinned, and the tiles are found by
//! asking the ground, never by naming coordinates.

use bevy::app::App;
use bevy::math::UVec2;
use core_sim::shipped;
use core_sim::{
    build_test_app, rung_site_refusal, tile_forage_capacity, tile_is_fresh_watered, BuildJob,
    BuildSource, DiscoveryProgressLedger, FoodSiteRegistry, ForageRegistry, LaborAllocation,
    LaborConfigHandle, LaborTarget, LadderConfigHandle, PopulationCohort, SimulationConfig,
    SiteRefusal, SnapshotHistory, TakeSelection, Tile, TileRegistry, ViewerFaction,
    CULTIVATION_DISCOVERY_ID, NO_DEPOSIT_FLOOR, SEED_SELECTION_DISCOVERY_ID,
    WORKED_LAND_DISCOVERY_ID,
};

const MAP_SEED: u64 = 119_304_647;
const GRID: UVec2 = UVec2::new(80, 52);

/// The Farm's `worked_land` fertility floor as shipped (`intensification_ladder.json`), read back
/// from the ladder in the tests that care rather than restated - this names the tile classes.
const CREW: u32 = 4;
const BUILDERS: u32 = 8;
const ALL_HANDS: u32 = CREW + BUILDERS;
const FOOD_PEAK: f32 = 0.5;
const TURN_BUDGET: usize = 160;

fn spawn_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    config.grid_size = GRID;
    app.world.insert_resource(config);
    app.update();
    app
}

/// One patch tile and the three readings the Farm's site rule judges.
#[derive(Clone, Copy)]
struct Ground {
    position: UVec2,
    on_site: bool,
    watered: bool,
    capacity: f32,
}

fn patch_ground(app: &App) -> Vec<Ground> {
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let registry = app.world.resource::<TileRegistry>();
    let (width, height) = (registry.width, registry.height);
    let sites = app.world.resource::<FoodSiteRegistry>();
    let patches = app.world.resource::<ForageRegistry>();
    let mut ground: Vec<Ground> = registry
        .tiles
        .iter()
        .filter_map(|entity| app.world.get::<Tile>(*entity))
        .filter(|tile| patches.patch(tile.position).is_some())
        .map(|tile| Ground {
            position: tile.position,
            on_site: sites.is_site(tile.position),
            watered: tile_is_fresh_watered(tile, width, height, wrap, |coord| {
                registry
                    .index(coord.x, coord.y)
                    .and_then(|entity| app.world.get::<Tile>(entity))
                    .map(|neighbor| neighbor.terrain_tags)
            }),
            capacity: tile_forage_capacity(&labor.forage, tile),
        })
        .collect();
    ground.sort_by_key(|g| (g.position.y, g.position.x));
    ground
}

/// The Farm's fertility floor under `worked_land`, read off the shipped record.
fn worked_land_floor(app: &App) -> f32 {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    ladder
        .rung(shipped::plant_farm())
        .site_relaxations
        .iter()
        .find(|relaxation| relaxation.knowledge == "worked_land")
        .expect("the Farm declares the worked_land relaxation")
        .site_requirement
        .min_forage_capacity
}

fn refusal(app: &App, ground: &Ground, held: &[&str]) -> Option<SiteRefusal> {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let tile = app
        .world
        .get::<Tile>(
            app.world
                .resource::<TileRegistry>()
                .index(ground.position.x, ground.position.y)
                .expect("on the map"),
        )
        .expect("a tile");
    rung_site_refusal(
        ladder.rung(shipped::plant_farm()),
        tile,
        &labor.forage,
        ground.on_site,
        ground.watered,
        NO_DEPOSIT_FLOOR,
        &|name| held.contains(&name),
    )
}

/// **THE SITE RULE, BY TILE CLASS.** Without the lesson the Farm's rule is the old Field's (a
/// gathering site that is fresh-watered); an off-site fertile tile is refused with the lesson to
/// learn; with the lesson it is admitted, and ground below the floor is still refused for its
/// thinness. A tile no lesson would help keeps its ordinary refusal.
#[test]
fn worked_land_opens_fertile_watered_ground_that_is_not_a_gathering_site() {
    let app = spawn_world();
    let floor = worked_land_floor(&app);
    let ground = patch_ground(&app);
    let off_site_rich = ground
        .iter()
        .find(|g| !g.on_site && g.watered && g.capacity >= floor)
        .expect("the standard map carries off-site fertile watered ground");
    let off_site_thin = ground
        .iter()
        .find(|g| !g.on_site && g.watered && g.capacity < floor)
        .expect("the standard map carries off-site thin watered ground");
    let off_site_dry = ground
        .iter()
        .find(|g| !g.on_site && !g.watered && g.capacity >= floor)
        .expect("the standard map carries off-site fertile dry ground");
    let on_site = ground
        .iter()
        .find(|g| g.on_site && g.watered)
        .expect("the standard map carries a sowable gathering site");

    // The gathering site is sowable either way.
    assert_eq!(refusal(&app, on_site, &[]), None);
    assert_eq!(refusal(&app, on_site, &["worked_land"]), None);

    // Fertile, watered, off-site: refused for want of the lesson, then admitted.
    assert_eq!(
        refusal(&app, off_site_rich, &[]),
        Some(SiteRefusal::NeedsLesson),
        "the refusal names the lesson, not the ground"
    );
    assert_eq!(refusal(&app, off_site_rich, &["worked_land"]), None);

    // Thin ground: no lesson would admit it, so without it the refusal is the ordinary one, and
    // with it the ground is refused for its thinness.
    assert_eq!(
        refusal(&app, off_site_thin, &[]),
        Some(SiteRefusal::NotGatheringSite)
    );
    assert_eq!(
        refusal(&app, off_site_thin, &["worked_land"]),
        Some(SiteRefusal::TooPoor),
        "a poor tile below the floor is refused even with the lesson"
    );

    // Dry ground keeps the water rule under the lesson.
    assert_eq!(
        refusal(&app, off_site_dry, &["worked_land"]),
        Some(SiteRefusal::TooDry)
    );
}

/// **THE WIRE SAYS IT TOO**: a patch row's Farm quote carries `needs_lesson` for the viewer who does
/// not hold the lesson and the empty answer once they do.
#[test]
fn the_farm_quote_names_the_lesson_until_the_people_hold_it() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = spawn_world();
    let faction = {
        let mut query = app.world.query::<&PopulationCohort>();
        query.iter(&app.world).next().expect("a band").faction
    };
    app.world.insert_resource(ViewerFaction(faction));
    let floor = worked_land_floor(&app);
    let target = *patch_ground(&app)
        .iter()
        .find(|g| !g.on_site && g.watered && g.capacity >= floor)
        .expect("off-site fertile watered ground");

    let quote_at = |app: &App| -> String {
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
        let patch = envelope
            .payload_as_snapshot()
            .expect("a snapshot")
            .subsistence()
            .expect("a subsistence section")
            .foragePatches()
            .expect("the forage patches")
            .iter()
            .find(|p| p.x() == target.position.x && p.y() == target.position.y)
            .expect("the patch is on the wire");
        sim_schema::wire_rung_quote(patch.rungQuotes(), "plant:farm")
            .expect("the Farm is quoted")
            .siteRefusal()
            .unwrap_or_default()
            .to_string()
    };

    app.update();
    assert_eq!(quote_at(&app), "needs_lesson");

    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(faction, WORKED_LAND_DISCOVERY_ID, core_sim::scalar_one());
    app.update();
    assert_eq!(quote_at(&app), "", "the lesson opens the ground");
}

/// Stand the faction's first band on `tile` with a crew on the patch, the builders on the queue head
/// and a `sow` queued, the faction knowing what gates the verb (and `worked_land` if asked).
fn queue_a_sow_on(app: &mut App, tile: UVec2, worked_land: bool) -> bevy::prelude::Entity {
    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .expect("on the map");
    let (band, faction) = {
        let mut query = app
            .world
            .query::<(bevy::prelude::Entity, &PopulationCohort)>();
        let (entity, cohort) = query.iter(&app.world).next().expect("a band");
        (entity, cohort.faction)
    };
    app.world.insert_resource(ViewerFaction(faction));
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("a cohort")
        .current_tile = tile_entity;
    let mut lessons = vec![CULTIVATION_DISCOVERY_ID, SEED_SELECTION_DISCOVERY_ID];
    if worked_land {
        lessons.push(WORKED_LAND_DISCOVERY_ID);
    }
    for lesson in lessons {
        app.world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(faction, lesson, core_sim::scalar_one());
    }
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(
        LaborTarget::Forage {
            tile,
            floor: FOOD_PEAK,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        },
        CREW,
        ALL_HANDS,
        None,
    );
    allocation.set_assignment(LaborTarget::Builders, BUILDERS, ALL_HANDS, None);
    allocation.enqueue_build(
        BuildSource::Patch(tile),
        BuildJob::Climb(shipped::plant_farm()),
    );
    app.world.entity_mut(band).insert(allocation);
    band
}

fn held_rung(app: &App, tile: UVec2) -> core_sim::RungId {
    app.world
        .resource::<ForageRegistry>()
        .patch(tile)
        .expect("the patch survives")
        .standing()
        .held
}

/// **WITH THE LESSON A SOW GOES ON OFF-SITE GROUND AND CLIMBS TO THE FARM; WITHOUT IT THE BUILD IS
/// REFUSED AND THE PATCH NEVER LEAVES THE WILD.**
#[test]
fn an_off_site_sow_climbs_to_the_farm_only_with_worked_land() {
    let probe = spawn_world();
    let floor = worked_land_floor(&probe);
    let target = patch_ground(&probe)
        .into_iter()
        .find(|g| !g.on_site && g.watered && g.capacity >= floor)
        .expect("off-site fertile watered ground")
        .position;

    // Control: no lesson, the head gate refuses the ground and the patch stays wild.
    let mut without = spawn_world();
    queue_a_sow_on(&mut without, target, false);
    for _ in 0..TURN_BUDGET / 4 {
        without.update();
    }
    assert_eq!(
        held_rung(&without, target),
        shipped::plant_wild(),
        "without Worked Land the off-site sow never starts"
    );

    // With the lesson the same queue climbs to the Farm.
    let mut with = spawn_world();
    queue_a_sow_on(&mut with, target, true);
    let mut turns = 0;
    while held_rung(&with, target) != shipped::plant_farm() {
        assert!(
            turns < TURN_BUDGET,
            "the off-site sow did not reach the Farm in {TURN_BUDGET} turns"
        );
        with.update();
        turns += 1;
    }
}

/// **WORKING A FARM TEACHES WORKED LAND.**
#[test]
fn working_a_farm_earns_worked_land() {
    let mut app = spawn_world();
    let ground = patch_ground(&app);
    let site = ground
        .iter()
        .find(|g| g.on_site && g.watered)
        .expect("a sowable gathering site")
        .position;
    let band = queue_a_sow_on(&mut app, site, false);
    let faction = app
        .world
        .get::<PopulationCohort>(band)
        .expect("cohort")
        .faction;
    // The Farm is already built: no climb is queued, only the crew's work on it.
    {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let (base, width) = core_sim::plant_rung_span(shipped::plant_farm(), &ladder);
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(site).expect("the site carries a patch");
        patch.owner = Some(faction);
        patch.set_ladder_position(base + width, &ladder);
    }
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("an allocation")
        .build_queue
        .clear();

    let learned = |app: &App| {
        app.world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(faction, WORKED_LAND_DISCOVERY_ID)
    };
    assert_eq!(learned(&app), core_sim::scalar_zero());
    for _ in 0..10 {
        app.update();
    }
    assert!(
        learned(&app) > core_sim::scalar_zero(),
        "working a Farm teaches Worked Land"
    );
}
