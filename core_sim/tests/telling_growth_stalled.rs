//! **The Telling tells the player growth has stalled for want of other peoples** (issue #761).
//!
//! `guidance.growth_needs_other_peoples` rides the `lineage.breeding_capped` signal — the count of
//! the player's bands whose breeding population the inbreeding ceiling cut births for this turn
//! (`BreedingReading::capped`). Driven through the whole turn pipeline on a generated world: the
//! real population system parks the reading and the real `telling_tick` samples it.

mod faction_support;

use bevy::app::App;
use bevy::prelude::{Entity, With};

use core_sim::{
    run_turn, scalar_from_f32, BandId, BeatLedger, CommandEventKind, CommandEventLog,
    DemographicsConfigHandle, FoundingLines, PopulationCohort, ResidentBand,
};
use faction_support::{one_faction_world, HOME};

const BEAT_ID: &str = "guidance.growth_needs_other_peoples";
/// A phrase only this beat's wardrobe carries.
const BEAT_PHRASE: &str = "too closely";
/// A larder many lifetimes deep, restocked each turn, so the band is never hungry.
const DEEP_LARDER: f32 = 1.0e6;
const LARDER_CLASS: &str = "dry";
/// Turns the band is held at its ceiling.
const CAPPED_TURNS: usize = 12;
/// How far under its ceiling the band is seated, in people.
const SEAT_UNDER_CEILING: f32 = 1.0;

fn home_band(app: &mut App) -> (Entity, BandId) {
    app.world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == HOME)
        .min_by_key(|(_, id, _)| **id)
        .map(|(entity, id, _)| (entity, *id))
        .expect("worldgen spawns a home band")
}

fn told(app: &App) -> usize {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|e| e.kind == CommandEventKind::NarrativeBeat && e.label.contains(BEAT_PHRASE))
        .count()
}

#[test]
fn an_isolated_band_at_its_ceiling_is_told_once_and_not_again_while_it_stays_capped() {
    let mut app = one_faction_world();
    let (band, id) = home_band(&mut app);
    let lineage = app
        .world
        .resource::<DemographicsConfigHandle>()
        .get()
        .lineage
        .clone();
    let lines = lineage.founding_lines.get();
    let ceiling = core_sim::breeding_ceiling(usize::from(lines), lineage.people_per_line);
    let dist = app
        .world
        .resource::<DemographicsConfigHandle>()
        .get()
        .initial_distribution
        .clone();

    for _ in 0..CAPPED_TURNS {
        let people = ceiling as f32 - SEAT_UNDER_CEILING;
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.founding_lines = FoundingLines::founded(id, lines);
        cohort.children = scalar_from_f32(people * dist.children);
        cohort.working = scalar_from_f32(people * dist.working);
        cohort.elders = scalar_from_f32(people * dist.elders);
        cohort.sync_size();
        cohort
            .stores
            .reset_food(LARDER_CLASS, scalar_from_f32(DEEP_LARDER));
        run_turn(&mut app);
    }

    assert!(
        app.world
            .get::<PopulationCohort>(band)
            .unwrap()
            .last_breeding
            .capped,
        "liveness: the band was capped"
    );
    assert!(app.world.resource::<BeatLedger>().has_fired(BEAT_ID));
    assert_eq!(told(&app), 1, "it is told once while the cap holds");
}
