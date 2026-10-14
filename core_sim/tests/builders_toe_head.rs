//! **THE BUILDERS' TOOL LINE NAMES THE QUEUE HEAD, OR NOTHING** (reported from play: a Cultivate
//! finished, the Tame behind it took the head, and the Work tab went on saying "builders short of
//! hoes" for two turns before it said crooks).
//!
//! Driven through real turns on a band with a Cultivate at the head of its queue and a Tame behind
//! it.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;
use core_sim::shipped;

use core_sim::{
    build_test_app, recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero,
    BuildJob, BuildQueueEntry, BuildSource, FactionId, GenerationId, LaborAllocation,
    LaborAssignment, LaborTarget, LocalStore, MoraleCause, PopulationCohort, ResidentBand,
    SourcePriority, TakeSelection, TileRegistry,
};

const BUILDERS: u32 = 2;
const GATHERERS: u32 = 2;
const HUNTERS: u32 = 2;
const FOOD_PEAK: f32 = 0.5;
const FIXTURE_BAND: u64 = 9_001;
const FIXTURE_SPECIES: &str = "Rabbit Warren";
const WORKFORCE_HEADROOM: u32 = 6;

fn a_cultivable_site(app: &mut App) -> UVec2 {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
    let tiles: std::collections::HashMap<UVec2, core_sim::Tile> = {
        let mut query = app.world.query::<&core_sim::Tile>();
        query
            .iter(&app.world)
            .map(|tile| (tile.position, tile.clone()))
            .collect()
    };
    let registry = app.world.resource::<core_sim::ForageRegistry>();
    let mut cultivable: Vec<UVec2> = app
        .world
        .resource::<core_sim::FoodSiteRegistry>()
        .sites()
        .iter()
        .map(|site| site.position)
        .filter(|position| registry.patch(*position).is_some())
        .filter(|position| {
            let Some(tile) = tiles.get(position) else {
                return false;
            };
            let composition =
                core_sim::tile_flora_composition(&flora, &labor.forage, tile, map_seed);
            core_sim::default_species_for_rung(
                &composition,
                &flora,
                shipped::ladder(),
                shipped::plant_tended(),
            )
            .is_some()
        })
        .collect();
    cultivable.sort_by_key(|tile| (tile.y, tile.x));
    cultivable[0]
}

fn well_fed() -> LocalStore {
    let mut stores = LocalStore::new();
    stores.add_food("dry", scalar_from_f32(50_000.0));
    stores
}

/// A band on a patch, a Rabbit Warren walked onto the same tile, a `Cultivate` at the head of the
/// queue (one work unit from done) and a `Tame` behind it.
fn world(cultivate_is_one_turn_from_done: bool) -> (App, Entity, UVec2, String) {
    let mut app = build_test_app();
    app.update();
    {
        let mut ledger = app
            .world
            .resource_mut::<core_sim::DiscoveryProgressLedger>();
        ledger.add_progress(
            FactionId(0),
            core_sim::CULTIVATION_DISCOVERY_ID,
            scalar_one(),
        );
        ledger.add_progress(FactionId(0), core_sim::HERDING_DISCOVERY_ID, scalar_one());
    }
    let anchor = a_cultivable_site(&mut app);
    let herd_id = {
        let mut registry = app.world.resource_mut::<core_sim::HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| {
                herd.id.starts_with("game_")
                    && herd.route_length() == 1
                    && herd.species == FIXTURE_SPECIES
            })
            .expect("the map places a stationary Rabbit Warren");
        herd.route = vec![anchor];
        herd.biomass = herd.carrying_capacity;
        herd.id.clone()
    };
    // The patch is one work unit short of its Cultivate (or left untouched).
    if cultivate_is_one_turn_from_done {
        let ladder = core_sim::LadderConfig::builtin();
        let mut forage = app.world.resource_mut::<core_sim::ForageRegistry>();
        let patch = forage.patch_mut(anchor).expect("the patch exists");
        let tended_cost = ladder
            .rung(shipped::plant_tended())
            .build_cost(1.0)
            .expect("tended builds");
        let almost = tended_cost - 0.01;
        patch.set_ladder_position(almost, &ladder);
    }
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(anchor.x, anchor.y)
        .expect("tile");
    let assignments = vec![
        LaborAssignment {
            party: None,
            muster_crew: 0,
            planned_crew: 0,
            target: LaborTarget::Forage {
                tile: anchor,
                floor: FOOD_PEAK,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
            workers: GATHERERS + 6,
            kit: None,
            priority: SourcePriority::default(),
        },
        LaborAssignment {
            party: None,
            muster_crew: 0,
            planned_crew: 0,
            target: LaborTarget::Hunt {
                fauna_id: herd_id.clone(),
                floor: FOOD_PEAK,
                move_with_herd: false,
            },
            workers: HUNTERS,
            kit: None,
            priority: SourcePriority::default(),
        },
        LaborAssignment {
            party: None,
            muster_crew: 0,
            planned_crew: 0,
            target: LaborTarget::Builders,
            workers: BUILDERS,
            kit: None,
            priority: SourcePriority::default(),
        },
    ];
    let staffed: u32 = assignments.iter().map(|row| row.workers).sum();
    let band = app
        .world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 200,
                children: scalar_zero(),
                working: scalar_from_f32((staffed * WORKFORCE_HEADROOM) as f32),
                elders: scalar_zero(),
                stores: well_fed(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_food_need: 0.0,
                fed_party_workers: Default::default(),
                last_food_spoiled: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_turn_transfer_crossings: Vec::new(),
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                last_breeding: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FactionId(0),
                knowledge: Vec::new(),
                founding_lines: core_sim::FoundingLines::founded(
                    core_sim::BandId(0),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
                last_belief_relay_hops: 0,
            },
            ResidentBand,
            core_sim::BandId(FIXTURE_BAND),
            LaborAllocation {
                assignments,
                build_queue: vec![
                    BuildQueueEntry {
                        source: BuildSource::Patch(anchor),
                        declared: BuildJob::Climb(shipped::plant_tended()),
                        priority: SourcePriority::default(),
                    },
                    BuildQueueEntry {
                        source: BuildSource::Herd(herd_id.clone()),
                        declared: BuildJob::Climb(shipped::animal_pastoral()),
                        priority: SourcePriority::default(),
                    },
                ],
                ..Default::default()
            },
        ))
        .id();
    (app, band, anchor, herd_id)
}

fn resolve_a_turn(app: &mut App) {
    app.world.run_system_once(core_sim::advance_herds);
    app.world.run_system_once(core_sim::advance_forage_regrowth);
    app.world.run_system_once(core_sim::advance_cultivation);
    app.world.run_system_once(core_sim::advance_husbandry);
    app.world
        .run_system_once(core_sim::advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

fn head_of(app: &App, band: Entity) -> Option<BuildSource> {
    app.world
        .get::<LaborAllocation>(band)
        .expect("allocation")
        .build_queue
        .first()
        .map(|entry| entry.source.clone())
}

/// The builders' lines as the sim settled them (`last_pool_toe`), unfiltered.
fn settled_builders_items(app: &App, band: Entity) -> Vec<String> {
    app.world
        .get::<LaborAllocation>(band)
        .expect("allocation")
        .last_pool_toe
        .iter()
        .filter(|line| line.pool.as_str() == "builders")
        .map(|line| line.item.clone())
        .collect()
}

/// **The builders' lines the ENCODED frame carries** for the fixture band — `(item, required)`.
fn published_builders_lines(app: &App) -> Vec<(String, f32)> {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<core_sim::SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let cohorts = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list");
    let cohort = cohorts
        .iter()
        .find(|cohort| cohort.bandId() == FIXTURE_BAND)
        .expect("the fixture band is published");
    cohort
        .poolToe()
        .map(|lines| {
            lines
                .iter()
                .filter(|line| line.pool().unwrap_or_default() == "builders")
                .map(|line| {
                    (
                        line.itemId().unwrap_or_default().to_string(),
                        line.required(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

const HOES: &str = "hoes";
const CROOK: &str = "crook";

/// **THE REPORTED CASE.** A Cultivate finishes and the Tame behind it takes the head. The frame of
/// the completion states NO builders' tools (the claim was struck on the finished job — the one
/// stale turn is withheld), and every frame after it states the Tame's tools and never the hoes.
#[test]
fn the_frame_after_a_completion_states_no_builders_tools_and_then_the_new_heads() {
    let (mut app, band, anchor, _herd) = world(true);
    // Liveness: before the completion the head is the Cultivate and its hoes are published.
    assert_eq!(
        head_of(&app, band),
        Some(BuildSource::Patch(anchor)),
        "fixture: the Cultivate is the head"
    );

    resolve_a_turn(&mut app);
    assert!(
        matches!(head_of(&app, band), Some(BuildSource::Herd(_))),
        "fixture: the Cultivate completed and the Tame is the head"
    );
    assert_eq!(
        settled_builders_items(&app, band),
        vec![HOES.to_string()],
        "liveness: the settled lines really are the finished job's — that is the stale turn"
    );
    assert!(
        published_builders_lines(&app).is_empty(),
        "the completion frame states no builders' tools: {:?}",
        published_builders_lines(&app)
    );

    for _ in 0..3 {
        resolve_a_turn(&mut app);
        let lines = published_builders_lines(&app);
        assert_eq!(
            lines
                .iter()
                .map(|(item, _)| item.as_str())
                .collect::<Vec<_>>(),
            vec![CROOK],
            "every frame after it states the Tame's tools, never the hoes"
        );
    }
}

/// **A REORDER BETWEEN TURNS WITHHOLDS THE LINES TOO** — the queue is read live at capture, after
/// the command. Control: the lines are published before the reorder and again once a turn has struck
/// them for the new head.
#[test]
fn a_build_order_withholds_the_builders_tools_until_a_turn_strikes_them_for_the_new_head() {
    let (mut app, band, anchor, herd) = world(false);
    resolve_a_turn(&mut app);
    assert_eq!(
        published_builders_lines(&app)
            .iter()
            .map(|(item, _)| item.as_str())
            .collect::<Vec<_>>(),
        vec![HOES],
        "control: the Cultivate head's hoes are published"
    );

    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("allocation")
        .move_build_entry(&BuildSource::Herd(herd), 0);
    recapture_snapshot_in_place(&mut app.world);
    assert!(
        published_builders_lines(&app).is_empty(),
        "the reorder withholds the old head's hoes"
    );

    resolve_a_turn(&mut app);
    assert_eq!(
        published_builders_lines(&app)
            .iter()
            .map(|(item, _)| item.as_str())
            .collect::<Vec<_>>(),
        vec![CROOK],
        "the next turn strikes and publishes the new head's crook"
    );
    let _ = anchor;
}

/// **AN UNQUEUE OF THE HEAD, AND OF THE WHOLE QUEUE, WITHHOLDS THEM** — and a fresh turn with an
/// empty queue states nothing because no ask is made at all.
#[test]
fn an_unqueue_withholds_the_builders_tools() {
    let (mut app, band, anchor, herd) = world(false);
    resolve_a_turn(&mut app);
    assert!(
        !published_builders_lines(&app).is_empty(),
        "control: the head's tools are published"
    );

    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("allocation")
        .unqueue_build(&BuildSource::Patch(anchor));
    recapture_snapshot_in_place(&mut app.world);
    assert!(
        published_builders_lines(&app).is_empty(),
        "unqueueing the head withholds its tools; the Tame is the head now and has not been struck"
    );

    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("allocation")
        .unqueue_build(&BuildSource::Herd(herd));
    resolve_a_turn(&mut app);
    assert!(
        published_builders_lines(&app).is_empty(),
        "an empty queue states no builders' tools"
    );
}

/// **THE STAMP RIDES THE CHECKPOINT**, so a restored band publishes the same builders' lines the
/// uninterrupted one did.
#[test]
fn the_published_builders_tools_survive_a_checkpoint_round_trip() {
    use core_sim::sim_state::{capture_sim_state, restore_sim_state};

    let (mut app, _band, _anchor, _herd) = world(false);
    resolve_a_turn(&mut app);
    let before = published_builders_lines(&app);
    assert!(!before.is_empty(), "liveness: there are lines to preserve");
    let checkpoint = capture_sim_state(&app.world);
    restore_sim_state(&mut app.world, &checkpoint);
    recapture_snapshot_in_place(&mut app.world);
    assert_eq!(published_builders_lines(&app), before);
}
