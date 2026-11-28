//! **The bench marks what it makes or holds for the tribe** (#864,
//! `.claude/rules/core_sim/crafting.md` -> "What is for the tribe").
//!
//! A tribe's work arrives as levy rows on a member band's own list; that band's bench makes their
//! gear. Every claim here reads the ENCODED frame: `CraftSuggestion.forTribeCount`,
//! `CraftSuggestionSource.forTribe` and `BenchOrder.forTribe`.

use bevy::prelude::{App, Entity, UVec2, With};

use core_sim::auto_craft::auto_craft_fill;
use core_sim::staffing::restaff_band;
use core_sim::{
    build_test_app, recapture_snapshot_in_place, scalar_from_f32, BandBench, BandEquipment,
    BenchOrder, BuildSource, BuildToeLine, KeepingIssue, LaborAllocation, LaborTarget,
    MaterialsConfigHandle, PopulationCohort, ResidentBand, SnapshotHistory, SourcePriority,
    TakeSelection, TribeId,
};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use std::collections::BTreeMap;

const HOES: &str = "hoes";
const BONE: &str = "bone";
const FIBRE: &str = "fibre";
/// Stock enough for any number of items used here.
const PLENTY: f32 = 400.0;
const BONE_AXES: [(&str, f32); 2] = [("density", 0.5), ("length", 0.5)];
const FIBRE_AXES: [(&str, f32); 2] = [("fineness", 0.5), ("strength", 0.5)];
/// Hands the fixture band has.
const HANDS: u32 = 20;
/// The patch the levy row works and the declared build stands on.
const PATCH: UVec2 = UVec2::new(2, 3);
/// The tribe the levy row belongs to.
const TRIBE: TribeId = TribeId(0);
/// Hoes the levy row's build lacks.
const TRIBE_SHORT: f32 = 3.0;
/// Hoes the band's own keeping crew lacks.
const OWN_SHORT: f32 = 2.0;

fn bank(app: &mut App, band: Entity, material: &str, amount: f32, axes: &[(&str, f32)]) {
    let materials = app.world.resource::<MaterialsConfigHandle>().get();
    let readings: BTreeMap<String, f32> = axes
        .iter()
        .map(|(axis, value)| ((*axis).to_string(), *value))
        .collect();
    let key = materials
        .band_key(material, &readings)
        .expect("the shipped table carries this material");
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .stores
        .deposit_material(material, key, scalar_from_f32(amount), &readings);
}

/// A band with `HANDS` working people, stocked bone and fibre, an empty bench, and ONE row on the
/// patch: a levy row when `levied`, the band's own otherwise. The band's build on that patch lacks
/// `TRIBE_SHORT` hoes; with `own_short` its keeping crew lacks `OWN_SHORT` more.
fn world(levied: bool, own_short: bool) -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    let band = app
        .world
        .query_filtered::<Entity, With<ResidentBand>>()
        .iter(&app.world)
        .next()
        .expect("the headless world spawns a resident band");
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.working = scalar_from_f32(HANDS as f32);
        cohort.children = scalar_from_f32(0.0);
        cohort.elders = scalar_from_f32(0.0);
        cohort.stores.clear_materials();
    }
    *app.world.get_mut::<BandEquipment>(band).unwrap() = BandEquipment::default();
    *app.world.get_mut::<BandBench>(band).unwrap() = BandBench::default();
    bank(&mut app, band, BONE, PLENTY, &BONE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        allocation.assignments.clear();
        allocation.set_assignment(
            LaborTarget::Forage {
                tile: PATCH,
                floor: 0.5,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
            1,
            1,
            None,
        );
        if levied {
            for row in &mut allocation.assignments {
                row.for_tribe = Some(TRIBE);
            }
        }
    }
    restaff_band(&mut app.world, band);
    set_shortfalls(&mut app, band, own_short);
    (app, band)
}

/// The settled lines the capture reads (a restaff does not rewrite them; a turn does).
fn set_shortfalls(app: &mut App, band: Entity, own_short: bool) {
    let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
    allocation.last_pool_toe.clear();
    allocation.last_keeping_issued = if own_short {
        vec![KeepingIssue {
            // A different patch: no levy row works it.
            source: BuildSource::Patch(UVec2::new(9, 9)),
            item: HOES.to_string(),
            units: 0.0,
            required: OWN_SHORT,
        }]
    } else {
        Vec::new()
    };
    allocation.last_build_toe = vec![BuildToeLine {
        source: BuildSource::Patch(PATCH),
        item: HOES.to_string(),
        required: TRIBE_SHORT,
        filled: 0.0,
    }];
}

fn queue(app: &mut App, band: Entity, count: u32) {
    let mut bench = app.world.get_mut::<BandBench>(band).unwrap();
    let mut order = BenchOrder::new(HOES, count);
    order.priority = SourcePriority::Normal;
    bench.orders.push(order);
}

/// What the encoded frame says about the hoes suggestion and the bench.
struct Frame {
    /// `(count, forTribeCount, [forTribe of each source])` of the hoes suggestion, if published.
    hoes: Option<(u32, u32, Vec<bool>)>,
    /// `BenchOrder.forTribe` of each order, queue order.
    orders: Vec<u32>,
}

fn read(app: &mut App, band: Entity, item: &str) -> Frame {
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let band_id = app.world.get::<core_sim::BandId>(band).unwrap().0;
    let cohort = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.bandId() == band_id)
        .expect("the band is published");
    let hoes = cohort.craftSuggestions().and_then(|rows| {
        rows.iter()
            .find(|row| row.itemId() == Some(item))
            .map(|row| {
                (
                    row.count(),
                    row.forTribeCount(),
                    row.sources()
                        .map(|sources| sources.iter().map(|source| source.forTribe()).collect())
                        .unwrap_or_default(),
                )
            })
    });
    let orders = cohort
        .bench()
        .and_then(|bench| bench.orders())
        .map(|rows| rows.iter().map(|row| row.forTribe()).collect())
        .unwrap_or_default();
    Frame { hoes, orders }
}

/// **A tribe-only shortfall is all for the tribe**, and the source line says so.
#[test]
fn a_tribe_only_shortfall_is_all_for_the_tribe() {
    let (mut app, band) = world(true, false);
    let frame = read(&mut app, band, HOES);
    let (count, for_tribe, sources) = frame.hoes.expect("hoes are suggested");
    assert_eq!(count, TRIBE_SHORT as u32);
    assert_eq!(for_tribe, count, "every unit is owed to the levy row");
    assert_eq!(sources, vec![true]);
}

/// **A mixed shortfall splits**: the levy build's units are the tribe's, the keeping crew's the band's.
#[test]
fn a_mixed_shortfall_splits_between_the_band_and_the_tribe() {
    let (mut app, band) = world(true, true);
    let frame = read(&mut app, band, HOES);
    let (count, for_tribe, mut sources) = frame.hoes.expect("hoes are suggested");
    assert_eq!(count, (TRIBE_SHORT + OWN_SHORT) as u32);
    assert_eq!(for_tribe, TRIBE_SHORT as u32);
    sources.sort();
    assert_eq!(sources, vec![false, true], "one own line, one levy line");
}

/// **The queue meets the band's own demand first, then the tribe's** - on the suggestion's count and
/// on each order's `forTribe`.
#[test]
fn the_queue_covers_the_bands_own_demand_before_the_tribes() {
    // S = 5, T = 3, B = 2. One order of 3: its 3 units are the band's 2 and the tribe's first.
    let (mut app, band) = world(true, true);
    queue(&mut app, band, 3);
    let frame = read(&mut app, band, HOES);
    let (count, for_tribe, _) = frame.hoes.expect("two hoes are still short");
    assert_eq!(count, 2);
    assert_eq!(for_tribe, 2, "the tribe's other two are still unmade");
    assert_eq!(
        frame.orders,
        vec![1],
        "one of the order's three is for the tribe"
    );

    // A second order of 1: Q = 4, the tribe's covered units are 2 and the last one lands there.
    queue(&mut app, band, 1);
    let frame = read(&mut app, band, HOES);
    let (count, for_tribe, _) = frame.hoes.expect("one hoe is still short");
    assert_eq!((count, for_tribe), (1, 1));
    assert_eq!(frame.orders, vec![1, 1]);

    // Enough queued to cover everything: no suggestion, and the orders carry the tribe's whole share.
    queue(&mut app, band, 1);
    let frame = read(&mut app, band, HOES);
    assert!(frame.hoes.is_none(), "the queue covers the shortfall");
    assert_eq!(frame.orders, vec![1, 1, 1]);
    assert_eq!(frame.orders.iter().sum::<u32>(), TRIBE_SHORT as u32);
}

/// **Queued units short of the band's own demand cover none of the tribe's.**
#[test]
fn a_queue_smaller_than_the_bands_own_demand_marks_nothing_for_the_tribe() {
    let (mut app, band) = world(true, true);
    queue(&mut app, band, 2);
    let frame = read(&mut app, band, HOES);
    let (count, for_tribe, _) = frame.hoes.expect("three are still short");
    assert_eq!((count, for_tribe), (3, 3));
    assert_eq!(frame.orders, vec![0]);
}

/// **A band in no tribe publishes zero and false everywhere**, with the same shortfall.
#[test]
fn a_band_in_no_tribe_marks_nothing_for_the_tribe() {
    let (mut app, band) = world(false, true);
    queue(&mut app, band, 3);
    let frame = read(&mut app, band, HOES);
    let (count, for_tribe, sources) = frame.hoes.expect("two are still short");
    assert_eq!(count, 2);
    assert_eq!(for_tribe, 0);
    assert!(sources.iter().all(|for_tribe| !for_tribe));
    assert_eq!(frame.orders, vec![0]);
}

/// **An order auto-craft adds from a tribe-only suggestion is marked for the tribe.**
#[test]
fn an_auto_order_made_for_a_tribe_only_suggestion_is_marked_for_the_tribe() {
    let (mut app, band) = world(true, false);
    app.world.get_mut::<BandBench>(band).unwrap().auto = true;
    assert!(
        auto_craft_fill(&mut app.world, band),
        "the suggestion is queued"
    );
    // The fill restaffs; the settled lines are a turn's, so state them again.
    set_shortfalls(&mut app, band, false);
    let frame = read(&mut app, band, HOES);
    assert_eq!(frame.orders, vec![TRIBE_SHORT as u32]);
    assert!(frame.hoes.is_none(), "the order covers the whole shortfall");
}

const BASKETS: &str = "baskets";
/// A forage row's crew: enough hands that the row asks for baskets.
const FORAGE_CREW: u32 = 4;

/// A band with ONE forage row on the real patch nearest its camp, no baskets, and no declared build or
/// keeping line: its only shortfall is the row's own kit. The row is a levy row when `levied`.
fn forage_world(levied: bool) -> (App, Entity) {
    let (mut app, band) = world(false, false);
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        allocation.last_build_toe.clear();
        allocation.assignments.clear();
    }
    let band_tile = app
        .world
        .get::<PopulationCohort>(band)
        .unwrap()
        .current_tile;
    let band_pos = app.world.get::<core_sim::Tile>(band_tile).unwrap().position;
    let patch = app
        .world
        .resource::<core_sim::ForageRegistry>()
        .patches
        .keys()
        .copied()
        .min_by_key(|tile| {
            (
                tile.x.abs_diff(band_pos.x) + tile.y.abs_diff(band_pos.y),
                tile.x,
                tile.y,
            )
        })
        .expect("the world has forage patches");
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        allocation.set_assignment(
            LaborTarget::Forage {
                tile: patch,
                floor: 0.5,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
            FORAGE_CREW,
            FORAGE_CREW,
            None,
        );
        if levied {
            for row in &mut allocation.assignments {
                row.for_tribe = Some(TRIBE);
            }
        }
    }
    restaff_band(&mut app.world, band);
    (app, band)
}

/// **THE TAKE-ROW PATH, with no declared build in sight**: a levy forage row short of baskets marks its
/// suggestion's take source and every unit of the count for the tribe; the same row in a band in no tribe
/// marks nothing.
#[test]
fn a_levy_take_rows_short_kit_is_marked_for_the_tribe() {
    let (mut app, band) = forage_world(true);
    let frame = read(&mut app, band, BASKETS);
    let (count, for_tribe, sources) = frame.hoes.expect("baskets are suggested");
    assert!(count > 0);
    assert_eq!(for_tribe, count, "every basket is for the levy row");
    assert!(!sources.is_empty() && sources.iter().all(|for_tribe| *for_tribe));

    let (mut app, band) = forage_world(false);
    let frame = read(&mut app, band, BASKETS);
    let (count, for_tribe, sources) = frame.hoes.expect("baskets are suggested");
    assert!(count > 0, "liveness: the same shortfall is there");
    assert_eq!(for_tribe, 0);
    assert!(sources.iter().all(|for_tribe| !for_tribe));
}
