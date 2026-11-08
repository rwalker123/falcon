//! **THE FOOD RUNWAY LIVES ON A BAND'S SHARE OF ITS NETWORK'S NET, NOT ON ONE TURN'S POOLED LUMP.**
//!
//! `turnsOfFood` used to add this turn's pooled food net as a standing per-turn rate. A neighbour's
//! whole-animal kill swings one turn's pooling by several food while the network's drain is steady,
//! so a band read a four-turn runway after pooling out 4.59 in one turn, when the whole network was
//! losing about 1.3 a turn. The runway now adds the steady pooling term
//! `share x network_net - own_net` (`snapshot::population::pooling_steady_nets`), published as
//! `poolingSteadyNet`.
//!
//! Asserted on the exported snapshot after a real split-and-pool fixture, never on the arithmetic.

use bevy::prelude::{App, Entity, With};
use core_sim::{
    build_test_app, recapture_snapshot_in_place, run_turn, scalar_from_f32, split_band_from_parent,
    BandId, LaborAllocation, LaborAssignment, LaborTarget, PopulationCohort, ResidentBand, Scalar,
    SettleConfig, SimulationConfig, SnapshotHistory, SourcePriority, SourceYield, TransferCause,
    TransferCrossing, TransferDirection, FOOD,
};

/// The shipped default `map_seed` is `0` ("seed from entropy"); a test pins its own.
const SEED: u64 = 119_304_647;
/// Workers the parent opens with, and the workers each split takes: three unequal bands.
const PARENT_WORKERS: f32 = 30.0;
const FIRST_SPLIT_WORKERS: u32 = 8;
const SECOND_SPLIT_WORKERS: u32 = 5;
/// Larder per person, so every band holds the same per-capita stock (what pooling levels to).
const LARDER_PER_PERSON: f32 = 20.0;
/// The one-turn lump a neighbour's kill pools: far larger than any steady per-turn rate here.
const LUMPY_POOL: f32 = 40.0;
/// How far the three bands' runways may sit from each other and from the network's.
const RUNWAY_TOLERANCE_FRACTION: f32 = 0.03;
/// The not-food-limited sentinel the runway publishes.
const NOT_FOOD_LIMITED: f32 = 999.0;
/// A rich band's income, as a multiple of the whole network's demand.
const INCOME_MULTIPLE: f32 = 3.0;

#[derive(Debug, Clone, Copy)]
struct Published {
    turns_of_food: f32,
    pooling_steady_net: f32,
}

fn world() -> App {
    let mut app = build_test_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = SEED;
    app.update();
    app
}

fn first_band(app: &mut App) -> Entity {
    let mut query = app
        .world
        .query_filtered::<Entity, (With<PopulationCohort>, With<ResidentBand>)>();
    query.iter(&app.world).next().expect("a resident band")
}

fn entity_for_band(app: &mut App, wanted: BandId) -> Entity {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| **id == wanted)
        .map(|(entity, _)| entity)
        .expect("the split allocated this id")
}

/// Three same-faction bands in one supply network, and one turn run so the network has formed.
fn three_networked_bands(app: &mut App) -> [Entity; 3] {
    let parent = first_band(app);
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(parent).unwrap();
        cohort.working = Scalar::from_f32(PARENT_WORKERS);
        cohort.sync_size();
    }
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let first = split_band_from_parent(&mut app.world, parent, FIRST_SPLIT_WORKERS, &settle)
        .expect("the parent splits");
    let second = split_band_from_parent(&mut app.world, parent, SECOND_SPLIT_WORKERS, &settle)
        .expect("the parent splits again");
    let first = entity_for_band(app, first.band);
    let second = entity_for_band(app, second.band);
    run_turn(app);
    [parent, first, second]
}

/// Every band's larder at the same per-capita stock, no income anywhere, and no pooled crossing.
fn level_larders(app: &mut App, bands: &[Entity]) {
    for &band in bands {
        {
            let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
            let food = LARDER_PER_PERSON * cohort.total().to_f32();
            cohort.stores.reset_food("dry", scalar_from_f32(food));
            cohort.last_turn_transfer_crossings.clear();
        }
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        allocation.assignments.clear();
        allocation.last_yields.clear();
    }
}

fn published(app: &App, band: Entity) -> Published {
    let id = *app.world.get::<BandId>(band).unwrap();
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .clone()
        .expect("a snapshot was captured");
    let cohort = snapshot
        .populations
        .iter()
        .find(|cohort| cohort.band_id == id.0)
        .expect("the band is exported");
    Published {
        turns_of_food: cohort.turns_of_food,
        pooling_steady_net: cohort.pooling_steady_net,
    }
}

fn pooled_out(amount: f32) -> TransferCrossing {
    TransferCrossing::goods(FOOD, TransferDirection::Out, TransferCause::Pooled, amount)
}

fn within(a: f32, b: f32) -> bool {
    (a - b).abs() <= RUNWAY_TOLERANCE_FRACTION * a.abs().max(b.abs())
}

/// **A neighbour's lumpy turn does not move anyone's runway.** Three bands at the same per-capita
/// larder and no income: the network drains at the sum of its demands and every band, giver or
/// receiver, lives on its share of that, so all three runways agree. One band's one-turn pooled lump
/// (several times the steady drain) moves none of them.
#[test]
fn a_neighbours_lumpy_turn_does_not_move_the_runway() {
    let mut app = world();
    let bands = three_networked_bands(&mut app);
    level_larders(&mut app, &bands);
    recapture_snapshot_in_place(&mut app.world);
    let calm: Vec<Published> = bands.iter().map(|&b| published(&app, b)).collect();

    // The same world, with one band's turn having pooled a big lump away.
    app.world
        .get_mut::<PopulationCohort>(bands[1])
        .unwrap()
        .last_turn_transfer_crossings = vec![pooled_out(LUMPY_POOL)];
    recapture_snapshot_in_place(&mut app.world);
    let lumpy: Vec<Published> = bands.iter().map(|&b| published(&app, b)).collect();

    for (calm, lumpy) in calm.iter().zip(&lumpy) {
        assert_eq!(
            calm.turns_of_food, lumpy.turns_of_food,
            "one turn's pooled crossing must not move the runway"
        );
        assert!(
            calm.turns_of_food > 0.0 && calm.turns_of_food < NOT_FOOD_LIMITED,
            "liveness: a finite runway ({})",
            calm.turns_of_food
        );
    }
    // And they agree with each other (the network's runway: sum of larders over the network drain),
    // because pooling keeps per-capita larders level.
    let network = calm[0].turns_of_food;
    for band in &calm {
        assert!(
            within(band.turns_of_food, network),
            "every band lives on its share of the network: {:?}",
            calm.iter().map(|b| b.turns_of_food).collect::<Vec<_>>()
        );
    }
    assert!(
        calm.iter().any(|b| b.pooling_steady_net.abs() > 0.0),
        "liveness: the term is live for a networked band"
    );
}

/// **A band fed by its neighbours reads fed.** One band earns three times the network's demand; the
/// other two have no income of their own. Their own net is negative and the network's is positive,
/// so their steady pooling term more than covers their demand and the runway is the sentinel.
#[test]
fn a_band_fed_by_its_neighbours_reads_the_sentinel() {
    let mut app = world();
    let bands = three_networked_bands(&mut app);
    level_larders(&mut app, &bands);
    // The parent earns; use a role row, whose retained yield counts as standing income.
    let demand: f32 = {
        recapture_snapshot_in_place(&mut app.world);
        // Network demand from the calm runway: larder / drain, summed over the bands.
        bands
            .iter()
            .map(|&b| {
                let cohort = app.world.get::<PopulationCohort>(b).unwrap();
                cohort.stores.get(FOOD).to_f32() / published(&app, b).turns_of_food
            })
            .sum()
    };
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(bands[0]).unwrap();
        allocation.assignments.push(LaborAssignment {
            party: None,
            muster_crew: 0,
            planned_crew: 0,
            scouting_range: 0,
            scouting_reach: 0,
            max_workers: None,
            staffing: Default::default(),
            target: LaborTarget::Warrior,
            workers: 1,
            kit: None,
            priority: SourcePriority::default(),
        });
        allocation.last_yields.push(SourceYield {
            realized: demand * INCOME_MULTIPLE,
            ..SourceYield::ZERO
        });
    }
    recapture_snapshot_in_place(&mut app.world);
    for &fed in &bands[1..] {
        let row = published(&app, fed);
        assert!(
            row.pooling_steady_net > 0.0,
            "liveness: the network feeds this band ({})",
            row.pooling_steady_net
        );
        assert_eq!(
            row.turns_of_food, NOT_FOOD_LIMITED,
            "a band fed by its neighbours is not starving"
        );
    }
}

/// **A lone band has no pooling term and a pooled crossing cannot move its runway.**
#[test]
fn a_lone_band_reads_zero_and_an_unchanged_runway() {
    let mut app = world();
    let band = first_band(&mut app);
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.stores.reset_food("dry", scalar_from_f32(100.0));
        cohort.last_turn_transfer_crossings.clear();
    }
    recapture_snapshot_in_place(&mut app.world);
    let calm = published(&app, band);
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .last_turn_transfer_crossings = vec![pooled_out(LUMPY_POOL)];
    recapture_snapshot_in_place(&mut app.world);
    let lumpy = published(&app, band);
    assert_eq!(calm.pooling_steady_net, 0.0);
    assert_eq!(lumpy.pooling_steady_net, 0.0);
    assert_eq!(calm.turns_of_food, lumpy.turns_of_food);
}
