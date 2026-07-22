//! **The band's exported larder runway must be the turn the larder actually empties.**
//!
//! `PopulationCohortState.turnsOfFood` used to be `larder / food_demand` — i.e. it assumed the band
//! **stops gathering and hunting**. For a resident band with real income that is badly pessimistic,
//! and it visibly contradicted the FOOD OUTLOOK chart drawn directly beneath it (header "4" over a
//! chart showing ~9). It is now the honest runway `larder / (consumption + pen feed − income)`,
//! walked off the sources' arrival schedules.
//!
//! This pins the claim the way the arc's other forecasts are pinned — **against the real sim, not
//! against another forecast**: publish the runway, then drive the real turn loop forward and assert
//! the larder really does run out on the turn that was published (±1 for the walk's clamp).

use bevy::prelude::Entity;
use core_sim::{
    build_headless_app, run_turn, scalar_from_f32, FollowPolicy, ForageRegistry, LaborAllocation,
    LaborAssignment, LaborTarget, PopulationCohort, SimulationConfig, SnapshotHistory, Tile, FOOD,
};

/// The shipped default `map_seed` is `0` ("seed from entropy"), so a test must pin its own or every
/// run lands on a different map.
const SEED: u64 = 119_304_647;
/// One gatherer only: enough income that the runway is genuinely longer than `larder / consumption`,
/// far too little to make the band net-positive (which would report the not-food-limited sentinel
/// and leave nothing to count down).
const GATHERERS: u32 = 1;
/// A larder that empties within a handful of turns, so the band's demand cannot drift much (births
/// and deaths move it) between the published runway and the turn it is checked against.
const TEST_LARDER: f32 = 20.0;
/// **"Empty" means the larder can no longer feed the band** — the first turn the people cannot eat
/// their fill. It is not "the float hits 0.0": once the deficit starts, the sim eats only what is
/// there, deaths shrink the band, and a starving remnant can hover above zero for another twenty
/// turns on a trickle of income. A fed band consumes exactly its demand, so the famine turn is the
/// one where consumption **collapses** relative to the turn before.
const FAMINE_CONSUMPTION_RATIO: f32 = 0.9;
/// How far past the published runway to keep driving before giving up.
const DRIVE_SLACK_TURNS: u32 = 30;

/// The band's current larder.
fn larder(app: &bevy::app::App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("band")
        .stores
        .get(FOOD)
        .to_f32()
}

/// `(turns_of_food, food_consumption)` off the **exported** snapshot — the numbers the client reads.
fn exported(app: &bevy::app::App) -> (f32, f32) {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot
        .clone()
        .expect("a snapshot was captured");
    let cohort = snapshot
        .populations
        .iter()
        .find(|c| !c.is_expedition)
        .expect("the resident band is exported");
    (cohort.turns_of_food, cohort.food_consumption)
}

#[test]
fn the_published_runway_is_the_turn_the_larder_really_empties() {
    let mut app = build_headless_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = SEED;
    app.update();

    let (band, band_tile_entity) = {
        let mut q = app.world.query::<(Entity, &PopulationCohort)>();
        let (e, c) = q.iter(&app.world).next().expect("a starting band");
        (e, c.current_tile)
    };
    let band_pos = app
        .world
        .get::<Tile>(band_tile_entity)
        .expect("band tile")
        .position;

    // Forage a patch on the band's own tile or right next to it — certainly inside `band_work_range`,
    // so the assignment actually yields (and projects an arrival schedule) instead of lapsing.
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patches
        .keys()
        .copied()
        .filter(|p| p.x.abs_diff(band_pos.x).max(p.y.abs_diff(band_pos.y)) <= 1)
        .min_by_key(|p| (p.y, p.x))
        .expect("the starting band must sit on or beside a forage patch");

    app.world.entity_mut(band).insert(LaborAllocation {
        assignments: vec![LaborAssignment {
            target: LaborTarget::Forage {
                tile: patch,
                policy: FollowPolicy::Sustain,
                species: None,
            },
            workers: GATHERERS,
        }],
        ..Default::default()
    });

    // Turn 1 resolves the assignment, which is what projects the arrival schedules the runway walks.
    run_turn(&mut app);
    // Now pin the larder and publish a runway against it.
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("band")
        .stores
        .set(FOOD, scalar_from_f32(TEST_LARDER));
    run_turn(&mut app);

    let (runway, consumption) = exported(&app);
    let published_larder = larder(&app, band);
    assert!(
        consumption > 0.0,
        "the people must be eating for a runway to mean anything"
    );
    assert!(
        runway > 0.0 && runway < 999.0,
        "a band this thin must report a finite runway, got {runway}"
    );

    // (a) Income makes it LONGER than the retired "we stop gathering and hunting" reading.
    let pessimistic = published_larder / consumption;
    assert!(
        runway > pessimistic,
        "the honest runway must beat larder / consumption: got {runway}, pessimistic {pessimistic}"
    );

    // (b) …and it is the turn the larder really runs out, driven through the real turn loop.
    let mut fed = consumption;
    let mut emptied_on = None;
    for turn in 1..=(runway as u32 + DRIVE_SLACK_TURNS) {
        run_turn(&mut app);
        let (_, ate) = exported(&app);
        if ate < fed * FAMINE_CONSUMPTION_RATIO {
            emptied_on = Some(turn);
            break;
        }
        fed = ate;
    }
    let emptied_on = emptied_on.expect("the larder must actually empty — the runway said it would");
    let drift = (emptied_on as i64 - runway as i64).abs();
    assert!(
        drift <= 1,
        "the published runway must be the turn the larder empties (\u{00b1}1 for the walk's clamp): \
         published {runway}, band went hungry on turn {emptied_on}"
    );
}
