//! **MIGRATION MODE — A BAND THAT CAMPS IN A MIGRATORY HERD STAYS IN IT**
//! (`docs/plan_roaming_bands.md` §Migration mode, `.claude/rules/core_sim/work-party.md`).
//!
//! A hunt row with `move_with_herd` set makes `follow_hunted_herds` aim the band at its herd every
//! turn, using the band movement that already exists. These tests drive the turn's real systems in
//! the order the Population chain runs them — herds move first, then the follow, then the band's
//! movement, then the labor pass — and read the flag back off the ENCODED snapshot.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::{
    advance_band_movement, advance_herds, advance_labor_allocation, build_test_app,
    follow_hunted_herds, recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero,
    BandEquipment, BandId, BandTravel, CommandEventLog, EquipmentConfig, FactionId,
    FaunaConfigHandle, GenerationId, Herd, HerdRegistry, LaborAllocation, LaborAssignment,
    LaborTarget, LocalStore, MoraleCause, PopulationCohort, ResidentBand, RoamState,
    SimulationTick, SizeClass, SnapshotHistory, SourcePriority, Tile, TileRegistry,
};

const MIGRATORY_SPECIES: &str = "Wild Reindeer";
const HERD_ID: &str = "herd_probe";
const BAND: u64 = 23;
const FACTION: FactionId = FactionId(0);
const CREW: u32 = 6;
const FLOOR: f32 = 0.5;
/// The fixture herd keeps its constructor `K`: there is no graze layer under it to derive another
/// from, so it carries no fodder demand (the `herd_game_trails` fixture's choice).
const FODDER_PER_BIOMASS: f32 = 0.0;
const REGROWTH_RATE: f32 = 0.04;
const BODY_MASS: f32 = 20.0;
/// A herd big enough that the crew's take is never what runs out.
const STANDING_STOCK: f32 = 5_000.0;
/// The row every probe herd walks along: a run of dry land on the harness map, so neither the herd
/// nor the band is stopped by water.
const ROW: u32 = 17;
/// A migration leg's two anchors, far enough apart that the herd walks several turns.
const LEG_START: UVec2 = UVec2::new(24, ROW);
const LEG_END: UVec2 = UVec2::new(34, ROW);
/// Turns a camped band is watched through a migration.
const TURNS_ON_THE_LEG: usize = 5;
/// Turns a loitering herd is watched; the loiter window outlasts them so it never tips into a leg.
const TURNS_LOITERING: usize = 12;
const LOITER_WINDOW: u32 = TURNS_LOITERING as u32 + 8;
/// A larder far past what the packs hold, so a long move has something to shed.
const FAT_LARDER: f32 = 100_000.0;
/// Keeping class of the meat the larder holds.
const MEAT_CLASS: &str = "flesh";
/// The status token of the feed line a long move's shed writes.
const LEFT_BEHIND: &str = "status=left_behind";

/// A world with one migratory herd on `roam`, walking `LEG_START` → `LEG_END`, and one band at
/// `band_at` hunting it with migration mode `follow`.
fn world(roam: RoamState, herd_at: UVec2, band_at: UVec2, follow: bool) -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    let herd = {
        let mut herd = Herd::new(
            HERD_ID.to_string(),
            MIGRATORY_SPECIES.to_string(),
            SizeClass::Migratory,
            vec![herd_at, LEG_END],
            STANDING_STOCK,
            STANDING_STOCK,
            FODDER_PER_BIOMASS,
            REGROWTH_RATE,
            BODY_MASS,
        );
        herd.roam = roam;
        herd.dwell_remaining = 0;
        herd
    };
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry.clear();
        registry.herds.push(herd);
    }
    let band = spawn_band(&mut app, band_at, follow);
    (app, band)
}

fn spawn_band(app: &mut App, at: UVec2, follow: bool) -> Entity {
    let camp = app
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
        .expect("the harness map carries the camp tile");
    let mut stores = LocalStore::new();
    stores.add_food(MEAT_CLASS, scalar_from_f32(FAT_LARDER));
    app.world
        .spawn((
            ResidentBand,
            BandId(BAND),
            BandEquipment::start_stocked_for(
                &EquipmentConfig::for_a_stocked_fixture(),
                CREW as f32,
            ),
            PopulationCohort {
                home: camp,
                current_tile: camp,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(CREW as f32),
                elders: scalar_zero(),
                stores,
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_food_need: 0.0,
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
                faction: FACTION,
                knowledge: Vec::new(),
                founding_lines: core_sim::FoundingLines::founded(
                    core_sim::BandId(0),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
                last_belief_relay_hops: 0,
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    party: None,
                    muster_crew: 0,
                    target: LaborTarget::Hunt {
                        fauna_id: HERD_ID.to_string(),
                        floor: FLOOR,
                        move_with_herd: follow,
                    },
                    workers: CREW,
                    kit: None,
                    priority: SourcePriority::default(),
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// One turn of the systems under test, in the Population chain's order: the herd moves (Logistics),
/// the band re-aims at it, the band steps, the labor pass resolves, the snapshot is captured.
fn turn(app: &mut App) {
    app.world.resource_mut::<SimulationTick>().0 += 1;
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(follow_hunted_herds);
    app.world.run_system_once(advance_band_movement);
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

fn band_tile(app: &App, band: Entity) -> UVec2 {
    let tile = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band keeps its cohort")
        .current_tile;
    app.world
        .get::<Tile>(tile)
        .expect("the band stands on a tile")
        .position
}

fn herd_tile(app: &App) -> UVec2 {
    app.world
        .resource::<HerdRegistry>()
        .find(HERD_ID)
        .expect("the herd is still in the registry")
        .current_pos
}

fn row_has_party(app: &App, band: Entity) -> bool {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .assignments
        .iter()
        .any(|row| row.party.is_some())
}

fn income(app: &App, band: Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .last_yields
        .iter()
        .map(|row| row.actual)
        .sum()
}

fn left_behind_lines(app: &App) -> usize {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| {
            entry
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains(LEFT_BEHIND))
        })
        .count()
}

/// **A BAND CAMPED ON A LOITERING HERD STAYS ON IT AS IT WANDERS**, and every kill is a camp kill.
#[test]
fn a_band_camped_on_a_loitering_herd_stays_on_its_tile_and_posts_no_party() {
    let (mut app, band) = world(
        RoamState::Loiter {
            turns_left: LOITER_WINDOW,
        },
        LEG_START,
        LEG_START,
        true,
    );
    let mut herd_moved = false;
    let mut took_something = false;
    let mut last = herd_tile(&app);
    for _ in 0..TURNS_LOITERING {
        turn(&mut app);
        let herd = herd_tile(&app);
        herd_moved |= herd != last;
        last = herd;
        assert_eq!(
            band_tile(&app, band),
            herd,
            "the band is on the herd's tile after every turn"
        );
        assert!(!row_has_party(&app, band), "a camp kill posts no party");
        took_something |= income(&app, band) > 0.0;
    }
    assert!(herd_moved, "liveness: the loitering herd really wandered");
    assert!(took_something, "liveness: the camp made kills");
}

/// **A BAND CAMPED ON A MIGRATING HERD MOVES WITH IT**, a hex a turn, with no party across the leg.
#[test]
fn a_band_camped_on_a_migrating_herd_moves_with_it_and_posts_no_party() {
    let (mut app, band) = world(RoamState::Migrate, LEG_START, LEG_START, true);
    let mut took_something = false;
    for step in 1..=TURNS_ON_THE_LEG {
        turn(&mut app);
        let herd = herd_tile(&app);
        assert_eq!(
            herd.x,
            LEG_START.x + step as u32,
            "liveness: the herd walks its leg a hex a turn"
        );
        assert_eq!(
            band_tile(&app, band),
            herd,
            "the band keeps pace with the migrating herd"
        );
        assert!(!row_has_party(&app, band), "a camp kill posts no party");
        took_something |= income(&app, band) > 0.0;
    }
    assert!(took_something, "liveness: the camp made kills on the move");
    assert_eq!(
        left_behind_lines(&app),
        0,
        "a one-hex re-aim is no long move: nothing is shed"
    );
}

/// The same migration with the flag off: the band stays put and the herd walks away from it.
/// Without it the camp-in-the-herd assertions above would pass for any herd that did not move.
#[test]
fn without_the_flag_the_band_stays_and_the_herd_walks_off() {
    let (mut app, band) = world(RoamState::Migrate, LEG_START, LEG_START, false);
    for _ in 0..TURNS_ON_THE_LEG {
        turn(&mut app);
    }
    assert_eq!(band_tile(&app, band), LEG_START, "no flag, no movement");
    assert_ne!(herd_tile(&app), LEG_START);
}

/// ⛔ **A FAR START WALKS A HEX A TURN, SHEDS ONCE, AND POSTS NO PARTY MEANWHILE.** The herd keeps
/// moving, so the target is re-aimed every turn; keeping `departed` is what stops the long-move shed
/// re-running on each re-aim. A follow row is a need row (hunting by need, #798): only a camp kill is
/// ever hunted, so while the band is catching up there is no far hunt and no caravan.
#[test]
fn a_far_start_walks_toward_the_herd_sheds_once_and_posts_no_party_meanwhile() {
    /// The band starts this many hexes west of the herd — past the ferry reach.
    const GAP: u32 = 6;
    let band_start = UVec2::new(LEG_END.x - GAP, ROW);
    // The herd stands on the leg's far end, 'loitering' there: it does not outrun the band.
    let (mut app, band) = world(
        RoamState::Loiter {
            turns_left: LOITER_WINDOW,
        },
        LEG_END,
        band_start,
        true,
    );
    let mut posted_a_party = false;
    for step in 1..=TURNS_ON_THE_LEG {
        let before = band_tile(&app, band).x;
        // **The larder is overfull again at every departure check**, so a shed that re-ran on a
        // re-aimed order would show as a second line; a shed that already ran leaves the packs full
        // and a second one would have nothing to say.
        app.world
            .get_mut::<PopulationCohort>(band)
            .expect("the band keeps its cohort")
            .stores
            .add_food(MEAT_CLASS, scalar_from_f32(FAT_LARDER));
        turn(&mut app);
        assert_eq!(
            band_tile(&app, band).x,
            before + 1,
            "turn {step}: the band steps one hex toward the herd"
        );
        assert_eq!(
            left_behind_lines(&app),
            1,
            "turn {step}: the departure shed ran once and never again while the target moved"
        );
        if let Some(travel) = app.world.get::<BandTravel>(band) {
            assert!(
                travel.departed,
                "turn {step}: the standing order keeps `departed`"
            );
        }
        posted_a_party |= row_has_party(&app, band);
    }
    assert!(
        !posted_a_party,
        "a need row never posts a party: while the band catches up it hunts nothing"
    );
}

/// ⛔ **THE FLAG IS PUBLISHED ON THE ROW, READ BACK OFF THE ENCODED BUFFER.**
#[test]
fn the_flag_reaches_the_encoded_labor_row() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
    let published = |follow: bool| {
        let (mut app, _) = world(RoamState::Migrate, LEG_START, LEG_START, follow);
        turn(&mut app);
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
        envelope
            .payload_as_snapshot()
            .expect("the envelope carries a snapshot")
            .population()
            .and_then(|section| section.populations())
            .expect("the population section carries the cohort list")
            .iter()
            .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
            .find(|row| row.kind().unwrap_or_default() == "hunt")
            .expect("the hunt row is on the wire")
            .moveWithHerd()
    };
    assert!(published(true), "the flag rides the row");
    assert!(!published(false), "and is false when it is not set");
}
