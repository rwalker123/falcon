//! **SEED SELECTION AND WORKED LAND LOOSEN THE SITE RULE - THEY DO NOT ADD A STEP.** Cultivate
//! (`plant:tended`) and Sow (`plant:farm`) stand on a gathering site. Seed Selection lets a people
//! work ground that is already very fertile and near fresh water ANYWHERE; Worked Land (plows and
//! irrigation) opens fertile ground away from water too (`site_relaxations` on both rungs, resolved
//! in the one `forage::rung_site_refusal` seam every gate and the wire share). Foraging itself is
//! site-bound: a crew on off-site ground still at the wild rung takes nothing.
//!
//! The standard map (`earthlike`, 80x52, seed 119304647) is pinned, and the tiles are found by
//! asking the ground, never by naming coordinates.

use bevy::app::App;
use bevy::math::UVec2;
use core_sim::shipped;
use core_sim::{
    build_test_app, rung_site_refusal, tile_forage_capacity, tile_is_fresh_watered, BuildJob,
    BuildSource, DiscoveryProgressLedger, FoodModuleTag, FoodSiteRegistry, ForageRegistry,
    LaborAllocation, LaborConfigHandle, LaborTarget, LadderConfigHandle, PopulationCohort,
    SimulationConfig, SiteRefusal, SnapshotHistory, TakeSelection, Tile, TileRegistry,
    ViewerFaction, CULTIVATION_DISCOVERY_ID, NO_DEPOSIT_FLOOR, SEED_SELECTION_DISCOVERY_ID,
    WORKED_LAND_DISCOVERY_ID,
};

const MAP_SEED: u64 = 119_304_647;
const GRID: UVec2 = UVec2::new(80, 52);

const CREW: u32 = 4;
const BUILDERS: u32 = 8;
const ALL_HANDS: u32 = CREW + BUILDERS;
const FOOD_PEAK: f32 = 0.5;
const TURN_BUDGET: usize = 160;
/// Turns a crew is given to bank a take before it is read.
const TAKE_TURNS: usize = 3;

const SEED_SELECTION: &str = "seed_selection";
const WORKED_LAND: &str = "worked_land";

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

/// One patch tile and the readings the site rule judges.
#[derive(Clone, Copy)]
struct Ground {
    position: UVec2,
    on_site: bool,
    watered: bool,
    capacity: f32,
    has_module: bool,
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
        .filter_map(|entity| app.world.get::<Tile>(*entity).map(|tile| (*entity, tile)))
        .filter(|(_, tile)| patches.patch(tile.position).is_some())
        .map(|(entity, tile)| Ground {
            position: tile.position,
            on_site: sites.is_site(tile.position),
            watered: tile_is_fresh_watered(tile, width, height, wrap, |coord| {
                registry
                    .index(coord.x, coord.y)
                    .and_then(|entity| app.world.get::<Tile>(entity))
                    .map(|neighbor| neighbor.terrain_tags)
            }),
            capacity: tile_forage_capacity(&labor.forage, tile),
            has_module: app.world.get::<FoodModuleTag>(entity).is_some(),
        })
        .collect();
    ground.sort_by_key(|g| (g.position.y, g.position.x));
    ground
}

/// The fertility floor of the named lesson's relaxation on the Farm, read off the shipped record.
fn floor_of(app: &App, lesson: &str) -> f32 {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    ladder
        .rung(shipped::plant_farm())
        .site_relaxations
        .iter()
        .find(|relaxation| relaxation.knowledge == lesson)
        .unwrap_or_else(|| panic!("the Farm declares the {lesson} relaxation"))
        .site_requirement
        .min_forage_capacity
}

/// The fertility floor both lessons share on the shipped map (the two entries may differ; the
/// stricter admits ground both open).
fn shared_floor(app: &App) -> f32 {
    floor_of(app, SEED_SELECTION).max(floor_of(app, WORKED_LAND))
}

fn refusal_on(
    app: &App,
    rung: core_sim::RungId,
    ground: &Ground,
    held: &[&str],
) -> Option<SiteRefusal> {
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
        ladder.rung(rung),
        tile,
        &labor.forage,
        ground.on_site,
        ground.watered,
        NO_DEPOSIT_FLOOR,
        core_sim::NO_BELIEF,
        &|name| held.contains(&name),
    )
}

fn nearest_lesson(app: &App, rung: core_sim::RungId, ground: &Ground) -> Option<String> {
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
    ladder
        .rung(rung)
        .lesson_that_would_admit(
            &|_| false,
            ground.on_site,
            tile_forage_capacity(&labor.forage, tile),
            ground.watered,
            NO_DEPOSIT_FLOOR,
            core_sim::NO_BELIEF,
        )
        .map(str::to_string)
}

/// **THE SITE RULE, BY TILE CLASS, ON BOTH RUNGS.** Each class beside its control:
/// - an off-site watered fertile tile: refused without Seed Selection, admitted with it (and with
///   Worked Land, the looser rule);
/// - an off-site DRY fertile tile: refused with Seed Selection alone, admitted with Worked Land;
/// - a thin tile (below the floor): refused by both lessons;
/// - a gathering site: admitted by the rung's own rule whatever is learned.
#[test]
fn the_lessons_open_fertile_ground_by_tile_class() {
    let app = spawn_world();
    let floor = shared_floor(&app);
    let ground = patch_ground(&app);
    let watered = ground
        .iter()
        .find(|g| !g.on_site && g.watered && g.capacity >= floor)
        .expect("off-site fertile watered ground");
    let dry = ground
        .iter()
        .find(|g| !g.on_site && !g.watered && g.capacity >= floor)
        .expect("off-site fertile dry ground");
    let thin_watered = ground
        .iter()
        .find(|g| !g.on_site && g.watered && g.capacity < floor)
        .expect("off-site thin watered ground");
    let thin_dry = ground
        .iter()
        .find(|g| !g.on_site && !g.watered && g.capacity < floor)
        .expect("off-site thin dry ground");
    let on_site = ground
        .iter()
        .find(|g| g.on_site && g.watered)
        .expect("a sowable gathering site");

    for rung in [shipped::plant_tended(), shipped::plant_farm()] {
        // The gathering site is admitted either way.
        assert_eq!(refusal_on(&app, rung, on_site, &[]), None);
        assert_eq!(refusal_on(&app, rung, on_site, &[WORKED_LAND]), None);

        // Fertile and watered: refused for want of a lesson - the NEAREST one - then admitted.
        assert_eq!(
            refusal_on(&app, rung, watered, &[]),
            Some(SiteRefusal::NeedsLesson)
        );
        assert_eq!(
            nearest_lesson(&app, rung, watered).as_deref(),
            Some(SEED_SELECTION),
            "watered ground names Seed Selection"
        );
        assert_eq!(refusal_on(&app, rung, watered, &[SEED_SELECTION]), None);
        assert_eq!(
            refusal_on(&app, rung, watered, &[WORKED_LAND]),
            None,
            "the looser rule admits watered ground too"
        );

        // Fertile and dry: Seed Selection alone does not open it, Worked Land does.
        assert_eq!(
            refusal_on(&app, rung, dry, &[]),
            Some(SiteRefusal::NeedsLesson)
        );
        assert_eq!(
            nearest_lesson(&app, rung, dry).as_deref(),
            Some(WORKED_LAND),
            "dry ground names Worked Land"
        );
        assert_eq!(
            refusal_on(&app, rung, dry, &[SEED_SELECTION]),
            Some(SiteRefusal::NeedsLesson),
            "Seed Selection alone is refused dry ground"
        );
        assert_eq!(refusal_on(&app, rung, dry, &[WORKED_LAND]), None);
        assert_eq!(
            refusal_on(&app, rung, dry, &[SEED_SELECTION, WORKED_LAND]),
            None,
            "a people holding both gets the looser Worked Land rule"
        );

        // Thin ground: no lesson would admit it; with the lessons it is refused for its thinness.
        assert_eq!(
            refusal_on(&app, rung, thin_watered, &[]),
            Some(SiteRefusal::NotGatheringSite)
        );
        assert_eq!(
            refusal_on(&app, rung, thin_watered, &[SEED_SELECTION]),
            Some(SiteRefusal::TooPoor)
        );
        assert_eq!(
            refusal_on(&app, rung, thin_dry, &[SEED_SELECTION, WORKED_LAND]),
            Some(SiteRefusal::TooPoor),
            "a poor tile below the floor is refused by both lessons"
        );
    }

    // Foraging never relaxes: the wild rung stays site-bound whatever is learned.
    assert_eq!(
        refusal_on(
            &app,
            shipped::plant_wild(),
            watered,
            &[SEED_SELECTION, WORKED_LAND]
        ),
        Some(SiteRefusal::NotGatheringSite)
    );
    assert_eq!(refusal_on(&app, shipped::plant_wild(), on_site, &[]), None);

    let admitted_by = |held: &[&str]| {
        ground
            .iter()
            .filter(|g| refusal_on(&app, shipped::plant_farm(), g, held).is_none())
            .count()
    };
    println!(
        "patch tiles {} | open now {} | with Seed Selection {} | with Worked Land {}",
        ground.len(),
        admitted_by(&[]),
        admitted_by(&[SEED_SELECTION]),
        admitted_by(&[WORKED_LAND])
    );
    assert!(admitted_by(&[SEED_SELECTION]) > admitted_by(&[]));
    assert!(admitted_by(&[WORKED_LAND]) > admitted_by(&[SEED_SELECTION]));
}

/// **A DRY GATHERING SITE TAKES THE FARM, AND DRY NEW GROUND NEEDS WORKED LAND.** The Farm's own
/// rule asks no water of a site; away from the sites Seed Selection asks for water and Worked Land
/// does not.
#[test]
fn a_dry_gathering_site_takes_the_farm_and_dry_new_ground_needs_worked_land() {
    let app = spawn_world();
    let ground = patch_ground(&app);
    let dry_site = ground
        .iter()
        .find(|g| g.on_site && !g.watered)
        .expect("a dry gathering site");
    assert_eq!(refusal_on(&app, shipped::plant_farm(), dry_site, &[]), None);
    let dry_new = ground
        .iter()
        .find(|g| !g.on_site && !g.watered && g.capacity >= shared_floor(&app))
        .expect("off-site fertile dry ground");
    assert_eq!(
        refusal_on(&app, shipped::plant_farm(), dry_new, &[SEED_SELECTION]),
        Some(SiteRefusal::NeedsLesson)
    );
    assert_eq!(
        refusal_on(&app, shipped::plant_farm(), dry_new, &[WORKED_LAND]),
        None
    );
}

fn quote_at(app: &App, target: UVec2, rung_key: &str) -> String {
    quote_fields_at(app, target, rung_key).0
}

/// The lesson the quote says the ground waits on (`needsLesson`), on the encoded envelope.
fn lesson_at(app: &App, target: UVec2, rung_key: &str) -> String {
    quote_fields_at(app, target, rung_key).1
}

/// `(siteRefusal, needsLesson)` of a patch's rung quote, read off the encoded envelope.
fn quote_fields_at(app: &App, target: UVec2, rung_key: &str) -> (String, String) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
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
        .find(|p| p.x() == target.x && p.y() == target.y)
        .expect("the patch is on the wire");
    let quote =
        sim_schema::wire_rung_quote(patch.rungQuotes(), rung_key).expect("the rung is quoted");
    (
        quote.siteRefusal().unwrap_or_default().to_string(),
        quote.needsLesson().unwrap_or_default().to_string(),
    )
}

fn the_viewer_faction(app: &mut App) -> core_sim::FactionId {
    let faction = {
        let mut query = app.world.query::<&PopulationCohort>();
        query.iter(&app.world).next().expect("a band").faction
    };
    app.world.insert_resource(ViewerFaction(faction));
    faction
}

fn learn(app: &mut App, faction: core_sim::FactionId, lesson: u32) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(faction, lesson, core_sim::scalar_one());
}

/// **THE WIRE SAYS IT TOO, ON BOTH RUNGS**, on the encoded envelope: a watered off-site patch quotes
/// `needs_lesson` until Seed Selection and then the empty answer; a dry one still quotes
/// `needs_lesson` with Seed Selection alone and opens with Worked Land.
#[test]
fn the_quotes_name_a_lesson_until_the_people_hold_the_one_that_opens_the_ground() {
    let mut app = spawn_world();
    let faction = the_viewer_faction(&mut app);
    let floor = shared_floor(&app);
    let ground = patch_ground(&app);
    let watered = ground
        .iter()
        .find(|g| !g.on_site && g.watered && g.capacity >= floor)
        .expect("off-site fertile watered ground")
        .position;
    let dry = ground
        .iter()
        .find(|g| !g.on_site && !g.watered && g.capacity >= floor)
        .expect("off-site fertile dry ground")
        .position;
    let thin = ground
        .iter()
        .find(|g| !g.on_site && g.watered && g.capacity < floor)
        .expect("off-site thin ground")
        .position;

    app.update();
    for key in ["plant:tended", "plant:farm"] {
        assert_eq!(quote_at(&app, watered, key), "needs_lesson");
        assert_eq!(quote_at(&app, dry, key), "needs_lesson");
        assert_eq!(quote_at(&app, thin, key), "not_gathering_site");
        // **The wire names the very lesson the gate picks** (`lesson_that_would_admit`), per tile:
        // Seed Selection on watered ground, Worked Land on dry ground, none where no lesson admits.
        let rung = if key == "plant:tended" {
            shipped::plant_tended()
        } else {
            shipped::plant_farm()
        };
        let ground_of = |position: UVec2| {
            *ground
                .iter()
                .find(|g| g.position == position)
                .expect("the tile")
        };
        assert_eq!(lesson_at(&app, watered, key), SEED_SELECTION, "{key}");
        assert_eq!(lesson_at(&app, dry, key), WORKED_LAND, "{key}");
        assert_eq!(
            lesson_at(&app, thin, key),
            "",
            "{key}: no lesson admits thin ground"
        );
        for position in [watered, dry] {
            assert_eq!(
                Some(lesson_at(&app, position, key)),
                nearest_lesson(&app, rung, &ground_of(position)),
                "{key}: the wire carries the gate's own pick"
            );
        }
    }

    learn(&mut app, faction, SEED_SELECTION_DISCOVERY_ID);
    app.update();
    for key in ["plant:tended", "plant:farm"] {
        assert_eq!(
            quote_at(&app, watered, key),
            "",
            "Seed Selection opens watered ground on {key}"
        );
        assert_eq!(
            lesson_at(&app, watered, key),
            "",
            "an admitted quote names no lesson on {key}"
        );
        assert_eq!(lesson_at(&app, dry, key), WORKED_LAND, "{key}");
        assert_eq!(
            quote_at(&app, dry, key),
            "needs_lesson",
            "Seed Selection alone does not open dry ground on {key}"
        );
        assert_eq!(
            quote_at(&app, thin, key),
            "too_poor",
            "a thin tile is refused for its thinness on {key}"
        );
    }

    learn(&mut app, faction, WORKED_LAND_DISCOVERY_ID);
    app.update();
    for key in ["plant:tended", "plant:farm"] {
        assert_eq!(quote_at(&app, dry, key), "", "Worked Land opens dry ground");
        assert_eq!(lesson_at(&app, dry, key), "", "{key}");
    }
}

/// Stand the faction's first band on `tile` with a crew on the patch, the builders on the queue head
/// and a `sow` queued, the faction knowing what gates the verb plus `extra` lessons.
fn queue_a_sow_on(app: &mut App, tile: UVec2, extra: &[u32]) -> bevy::prelude::Entity {
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
    lessons.extend_from_slice(extra);
    for lesson in lessons {
        learn(app, faction, lesson);
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
    allocation.declare_build(
        BuildSource::Patch(tile),
        BuildJob::Climb(shipped::plant_farm()),
    );
    app.world.entity_mut(band).insert(allocation);
    core_sim::seat_builders(&mut app.world, band, BUILDERS);
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

fn climbs_to_the_farm(target: UVec2, extra: &[u32]) -> bool {
    let mut app = spawn_world();
    queue_a_sow_on(&mut app, target, extra);
    for _ in 0..TURN_BUDGET {
        app.update();
        if held_rung(&app, target) == shipped::plant_farm() {
            return true;
        }
    }
    false
}

/// Whether some plant in the tile's realized basket climbs all the way to a Farm (a Sow on ground
/// where nothing climbs stalls whatever the lessons say - a crop question, not a site one).
fn a_crop_climbs_here(app: &App, ground: &Ground) -> bool {
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let tile = app
        .world
        .get::<Tile>(
            app.world
                .resource::<TileRegistry>()
                .index(ground.position.x, ground.position.y)
                .expect("on the map"),
        )
        .expect("a tile");
    core_sim::tile_flora_composition(&flora, &labor.forage, tile, map_seed)
        .iter()
        .any(|share| {
            flora.species.get(&share.species).is_some_and(|def| {
                def.cultivation_ceiling
                    .is_none_or(|c| c == shipped::plant_farm())
            })
        })
}

/// **AN OFF-SITE SOW CLIMBS TO THE FARM ONLY ON GROUND THE PEOPLE'S LESSONS OPEN**, each beside its
/// control: watered ground climbs with Seed Selection alone, dry ground does not until Worked Land.
#[test]
fn an_off_site_sow_climbs_to_the_farm_only_on_ground_the_lessons_open() {
    let probe = spawn_world();
    let floor = shared_floor(&probe);
    let ground = patch_ground(&probe);
    let watered = ground
        .iter()
        .find(|g| !g.on_site && g.watered && g.capacity >= floor && a_crop_climbs_here(&probe, g))
        .expect("off-site fertile watered ground a crop climbs on")
        .position;
    let dry = ground
        .iter()
        .find(|g| !g.on_site && !g.watered && g.capacity >= floor && a_crop_climbs_here(&probe, g))
        .expect("off-site fertile dry ground a crop climbs on")
        .position;

    assert!(
        climbs_to_the_farm(watered, &[]),
        "Seed Selection opens watered ground anywhere"
    );
    assert!(
        !climbs_to_the_farm(dry, &[]),
        "Seed Selection alone never raises a Farm on dry off-site ground"
    );
    assert!(
        climbs_to_the_farm(dry, &[WORKED_LAND_DISCOVERY_ID]),
        "Worked Land opens fertile dry ground"
    );
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
    let band = queue_a_sow_on(&mut app, site, &[]);
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
        .builds
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

/// The food one turn's work on `tile` banked, by a crew of `CREW` standing on it with no build.
fn crew_take_on(app: &mut App, tile: UVec2) -> f32 {
    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .expect("on the map");
    let band = {
        let mut query = app
            .world
            .query::<(bevy::prelude::Entity, &PopulationCohort)>();
        query.iter(&app.world).next().expect("a band").0
    };
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("a cohort")
        .current_tile = tile_entity;
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(
        LaborTarget::Forage {
            tile,
            floor: FOOD_PEAK,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        },
        CREW,
        CREW,
        None,
    );
    app.world.entity_mut(band).insert(allocation);
    core_sim::seat_builders(&mut app.world, band, 0);
    let mut best = 0.0_f32;
    for _ in 0..TAKE_TURNS {
        app.update();
        best = best.max(
            app.world
                .get::<LaborAllocation>(band)
                .expect("an allocation")
                .last_yields
                .iter()
                .map(|yield_| yield_.actual)
                .fold(0.0, f32::max),
        );
    }
    best
}

/// **A CREW ON OFF-SITE WILD GROUND TAKES NOTHING; THE SAME CREW ON A GATHERING SITE TAKES FOOD;
/// AND THE OFF-SITE PATCH PAYS AGAIN ONCE IT IS TENDED.** Foraging is site-bound: ground that is not a
/// gathering site yields nothing until Tended.
#[test]
fn a_crew_on_off_site_wild_ground_takes_nothing_until_it_is_tended() {
    let probe = spawn_world();
    let ground = patch_ground(&probe);
    let site = ground
        .iter()
        .find(|g| g.on_site && g.has_module && g.capacity > 0.0)
        .expect("a gathering site with a module")
        .position;
    let off_site = ground
        .iter()
        .find(|g| !g.on_site && g.has_module && g.capacity > 0.0)
        .expect("off-site ground carrying a food module")
        .position;

    let mut on_site = spawn_world();
    let site_take = crew_take_on(&mut on_site, site);
    assert!(site_take > 0.0, "**LIVENESS**: a crew on a site takes food");

    let mut wild = spawn_world();
    assert_eq!(
        crew_take_on(&mut wild, off_site),
        0.0,
        "off-site wild ground yields nothing"
    );

    let mut tended = spawn_world();
    {
        let ladder = tended.world.resource::<LadderConfigHandle>().get();
        let faction = the_viewer_faction(&mut tended);
        let mut registry = tended.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(off_site).expect("a patch");
        patch.complete_cultivation(faction, &ladder);
    }
    assert!(
        crew_take_on(&mut tended, off_site) > 0.0,
        "a Tended patch off a gathering site keeps its season"
    );
}
