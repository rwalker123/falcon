//! **A band living hand-to-mouth is starving, and the frame has to say so.**
//!
//! Found in a live playtest's run record: a band whose larder sat at ~3–6 against a need of ~3.6
//! lost people to starvation every lumpy low-income turn while its Food line read `+0.64` and its
//! runway read the `999` "not draining" sentinel. The meal is eaten in `simulate_population`, BEFORE
//! the turn's take lands, and `food_consumption` publishes what was **eaten** —
//! `min(need, larder)` — so when the larder is short, eaten < need, income − eaten reads healthy,
//! and a runway that let income land before the meal never saw the gap.
//!
//! What ships now: `foodNeed` (what the meal was measured against) and `foodShortfall`
//! (`need − eaten`, this turn's hunger) beside `foodConsumption`, and a runway that eats before the
//! take lands. `foodConsumption` itself is untouched — it is the larder identity's term.
//!
//! Asserted on the **encoded** envelope: the shipped representation is what a client reads.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

mod faction_support;

use core_sim::{
    publish_baseline_snapshot, scalar_from_f32, simulate_population, BandId, LaborAllocation,
    LaborAssignment, LaborTarget, PopulationCohort, ResidentBand, SnapshotHistory, SourcePriority,
    SourceYield, NOT_FOOD_LIMITED_TURNS,
};
use faction_support::{one_faction_world, HOME};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// The band's income, as a multiple of its need — comfortably above, so "the average covers it" is
/// true and cannot be what keeps the band fed.
const INCOME_OVER_NEED: f32 = 2.0;
/// The larder at meal time, as a fraction of need — short of one meal.
const SHORT_LARDER: f32 = 0.5;
/// A larder many meals deep.
const DEEP_LARDER: f32 = 10.0;
/// How many turns of arrivals the staged schedule projects.
const ARRIVAL_TURNS: usize = 12;
/// f32 tolerance for the need/eaten comparisons.
const EPSILON: f32 = 1e-3;

struct Published {
    need: f32,
    shortfall: f32,
    eaten: f32,
    income: f32,
    runway: f32,
}

fn home_band(app: &mut App) -> (Entity, BandId) {
    app.world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == HOME)
        .map(|(entity, band, _)| (entity, *band))
        .min_by_key(|(_, band)| *band)
        .expect("the home faction has a band")
}

/// Stage `larder_in_meals` meals in the larder, eat one turn's meal, give the band an income of
/// `INCOME_OVER_NEED` × its need, and read the band's row off the encoded frame.
fn eat_and_publish(larder_in_meals: f32) -> Published {
    let mut app = one_faction_world();
    let (band, band_id) = home_band(&mut app);
    // The need this band's meal will be measured against: run one meal on a deep larder to read it,
    // then stage the real larder against it.
    let need = {
        let mut probe = one_faction_world();
        let (probe_band, _) = home_band(&mut probe);
        probe
            .world
            .get_mut::<PopulationCohort>(probe_band)
            .unwrap()
            .stores
            .reset_food("dry", scalar_from_f32(1.0e6));
        probe.world.run_system_once(simulate_population);
        probe
            .world
            .get::<PopulationCohort>(probe_band)
            .unwrap()
            .last_food_need
    };
    assert!(need > 0.0, "fixture: the band has mouths to feed");
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .stores
        .reset_food("dry", scalar_from_f32(need * larder_in_meals));

    app.world.run_system_once(simulate_population);

    // An income that beats need on average, landing every turn — the take the turn credits after
    // the meal. A scout row carries it so no source lookup is needed to publish it.
    let income = need * INCOME_OVER_NEED;
    let row = LaborAssignment {
        party: None,
        muster_crew: 0,
        target: LaborTarget::Scout,
        workers: 0,
        kit: None,
        priority: SourcePriority::default(),
    };
    let yields = SourceYield {
        actual: income,
        realized: income,
        arrivals: vec![income; ARRIVAL_TURNS],
        ..SourceYield::ZERO
    };
    app.world.entity_mut(band).insert(LaborAllocation {
        assignments: vec![row],
        last_yields: vec![yields],
        ..Default::default()
    });

    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section is published")
        .iter()
        .find(|row| row.bandId() == band_id.0)
        .expect("the band's row is published");
    Published {
        need: row.foodNeed(),
        shortfall: row.foodShortfall(),
        eaten: row.foodConsumption(),
        income: row.foodIncome(),
        runway: row.turnsOfFood(),
    }
}

#[test]
fn a_band_whose_larder_is_short_at_meal_time_publishes_its_hunger_and_a_real_runway() {
    let published = eat_and_publish(SHORT_LARDER);
    assert!(
        published.income > published.need,
        "fixture: the income beats the need on average"
    );
    assert!(
        published.eaten < published.need - EPSILON,
        "they ate less than they needed: eaten {} need {}",
        published.eaten,
        published.need
    );
    assert!(published.shortfall > 0.0, "the hunger is on the wire");
    assert!(
        (published.shortfall - (published.need - published.eaten)).abs() < EPSILON,
        "shortfall is need − eaten"
    );
    assert!(
        published.runway < NOT_FOOD_LIMITED_TURNS,
        "a band that cannot cover its next meal is draining: runway {}",
        published.runway
    );
}

#[test]
fn a_well_stocked_band_publishes_no_shortfall_and_eats_what_it_needs() {
    let published = eat_and_publish(DEEP_LARDER);
    assert!(published.need > 0.0);
    assert_eq!(published.shortfall, 0.0, "a fed turn has no hunger");
    assert!(
        (published.eaten - published.need).abs() < EPSILON,
        "a full larder feeds the whole need: eaten {} need {}",
        published.eaten,
        published.need
    );
    assert_eq!(
        published.runway, NOT_FOOD_LIMITED_TURNS,
        "income beats need and the larder covers the meal: not draining"
    );
}
