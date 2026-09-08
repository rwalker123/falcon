//! **Worldgen places every faction, not just faction 0.**
//!
//! Before this, `spawn_initial_world` hard-wired the starting spawn, the seeded stockpile, the
//! seeded knowledge and the campaign start marker to `FactionId(0)`: a second faction in the roster
//! was awaited by the turn queue and had no land, no people and nothing to play with. These tests
//! are the two-faction arm and its one-faction control, side by side — the second arm is what makes
//! *"the second faction got its own"* distinguishable from *"the first faction got it twice"*, and
//! from *"nothing changed at all"*.

use bevy::prelude::*;

mod faction_support;

use core_sim::{
    BandId, FactionInventory, InventoryEntry, PopulationCohort, StartLocation, StartingLoadout,
    StartingUnit,
};
use faction_support::{one_faction_world, world_with, HOME, NO_RIVALS, ONE_RIVAL, RIVAL};

/// **The tile a one-faction world has always opened on**, at `HARNESS_MAP_SEED` on the shipped
/// earthlike preset.
///
/// A pinned coordinate rather than a relation, because the thing being guarded is that generalising
/// the picker to N factions did not perturb the **scoring** — and a scoring change moves this tile
/// while satisfying every relational assertion in this file.
const HARNESS_SEED_START: UVec2 = UVec2::new(28, 18);

/// The shipped `faction_start_min_separation`, restated so the assertions below read against a
/// number rather than against the config they are checking. Kept in step with
/// `simulation_config.json` by [`the_pinned_separation_is_the_shipped_one`].
const SHIPPED_SEPARATION: u32 = 20;

/// **What "spread out, not stacked" has to mean to be falsifiable.** When the separation cannot be
/// met, worldgen maximises the minimum distance to every placed start — so the achieved distance is
/// bounded by the land, which no test can predict, but it must be a real *share* of what was on
/// offer rather than the adjacency a highest-score fallback produces. Half is the floor these
/// assertions read against; the old fallback achieved 1.00 tile and fails it by an order of
/// magnitude, so this is a tripwire, not a tuned threshold.
const RELAXED_SHARE_OF_THE_AVAILABLE_GROUND: f64 = 0.5;

fn distance(a: UVec2, b: UVec2) -> f64 {
    let dx = a.x as f64 - b.x as f64;
    let dy = a.y as f64 - b.y as f64;
    (dx * dx + dy * dy).sqrt()
}

fn bands_by_faction(app: &mut App) -> Vec<(core_sim::FactionId, BandId)> {
    let mut rows: Vec<_> = app
        .world
        .query_filtered::<(&BandId, &PopulationCohort), With<StartingUnit>>()
        .iter(&app.world)
        .map(|(band, cohort)| (cohort.faction, *band))
        .collect();
    rows.sort();
    rows
}

#[test]
fn the_pinned_separation_is_the_shipped_one() {
    let world = one_faction_world();
    assert_eq!(
        world
            .world
            .resource::<core_sim::SimulationConfig>()
            .faction_start_min_separation,
        SHIPPED_SEPARATION,
        "the constant these assertions read must track `simulation_config.json`"
    );
}

/// ⛔ **THE SCORING GUARD.** Selection was generalised to N factions; scoring was not touched, and
/// this is what says so.
#[test]
fn a_one_faction_world_opens_on_the_tile_it_has_always_opened_on() {
    let world = one_faction_world();
    let starts = world.world.resource::<StartLocation>();
    assert_eq!(
        starts.position_for(HOME),
        Some(HARNESS_SEED_START),
        "the single faction's start must be the argmax the scoring has always produced"
    );
    assert_eq!(starts.len(), 1, "a one-faction world places one start");
}

#[test]
fn a_two_faction_world_places_two_starts_at_least_the_separation_apart() {
    let world = two_faction_world_app();
    let starts = world.world.resource::<StartLocation>();

    let home = starts.position_for(HOME).expect("the human is placed");
    let rival = starts.position_for(RIVAL).expect("the AI is placed");
    assert_ne!(home, rival, "two peoples do not open on the same hex");
    assert_eq!(
        home, HARNESS_SEED_START,
        "the first pick is still the argmax, so adding a faction cannot move the human's start"
    );
    assert!(
        distance(home, rival) >= SHIPPED_SEPARATION as f64,
        "starts {home:?} and {rival:?} are {:.1} apart, under the configured {SHIPPED_SEPARATION}",
        distance(home, rival)
    );
}

#[test]
fn each_faction_gets_its_own_starting_band_around_its_own_start() {
    let mut world = two_faction_world_app();
    let starts = world.world.resource::<StartLocation>().clone();
    let bands = bands_by_faction(&mut world);

    assert_eq!(
        bands.len(),
        2,
        "the profile's one starting unit is spawned once per faction, got {bands:?}"
    );
    assert_eq!(bands[0].0, HOME);
    assert_eq!(bands[1].0, RIVAL);

    // Each band stands within the profile's placement radius of *its own* faction's start, which is
    // the assertion that fails if both rosters were spawned around one point.
    for (faction, band) in bands {
        let start = starts.position_for(faction).expect("the faction is placed");
        let home_tile = world
            .world
            .query::<(&BandId, &PopulationCohort)>()
            .iter(&world.world)
            .find(|(id, _)| **id == band)
            .map(|(_, cohort)| cohort.home)
            .expect("the band has a home tile");
        let position = world
            .world
            .get::<core_sim::Tile>(home_tile)
            .expect("the home tile is a tile")
            .position;
        let own = distance(position, start);
        let other = distance(
            position,
            starts
                .position_for(if faction == HOME { RIVAL } else { HOME })
                .expect("the other faction is placed"),
        );
        assert!(
            own < other,
            "{faction:?}'s band at {position:?} is nearer the other faction's start ({other:.1}) \
             than its own ({own:.1})"
        );
    }
}

/// **One opening window per people.** The window used to be opened for the globally lowest `BandId`
/// carrying `StartingUnit`, so a second faction's band got none — no kits, no material, and nothing
/// on screen to say why.
#[test]
fn each_faction_gets_its_own_opening_loadout_window() {
    let mut world = two_faction_world_app();
    let bands = bands_by_faction(&mut world);
    let loadout = world.world.resource::<StartingLoadout>();

    assert_eq!(
        loadout.open_count(),
        2,
        "one window per faction, got {:?}",
        loadout.iter().map(|(band, _)| band).collect::<Vec<_>>()
    );
    for (faction, band) in bands {
        let window = loadout
            .window(band)
            .unwrap_or_else(|| panic!("{faction:?}'s opening band {band:?} has no window"));
        assert!(window.open, "{faction:?}'s window must be open");
    }
}

#[test]
fn each_faction_gets_its_own_seeded_stockpile_and_knowledge() {
    // The shipped profile ships an empty `inventory`, so the stockpile half needs a grant to exist
    // at all — stated here rather than in the shipped config, which is not this test's subject.
    let world = world_with(ONE_RIVAL, |config| {
        config.start_profile_overrides.inventory = vec![InventoryEntry {
            item: "provisions".to_string(),
            quantity: 7,
        }];
    });

    let inventory = world.world.resource::<FactionInventory>();
    for faction in [HOME, RIVAL] {
        assert_eq!(
            inventory
                .stockpile(faction)
                .and_then(|pile| pile.get("provisions").copied()),
            Some(7),
            "{faction:?} must open with the profile's stockpile, not share the other's"
        );
    }

    let discovery = world.world.resource::<core_sim::DiscoveryProgressLedger>();
    let tags = &world
        .world
        .resource::<core_sim::SimulationConfig>()
        .start_profile_overrides
        .starting_knowledge_tags;
    assert!(
        !tags.is_empty(),
        "the shipped profile seeds knowledge, or this assertion proves nothing"
    );
    let catalog = world
        .world
        .resource::<core_sim::StartProfileKnowledgeTagsHandle>()
        .get();
    for tag in tags {
        let definition = catalog
            .get(tag.as_str())
            .unwrap_or_else(|| panic!("the catalog defines {tag}"));
        for faction in [HOME, RIVAL] {
            assert!(
                discovery.get_progress(faction, definition.discovery_id())
                    > core_sim::scalar_zero(),
                "{faction:?} must be seeded with {tag}"
            );
        }
    }
}

/// **A cramped map still places everybody.** The separation is a target, not a precondition:
/// worldgen relaxes it and warns rather than leaving a faction with no ground.
#[test]
fn a_cramped_map_relaxes_the_separation_instead_of_failing_to_place_a_faction() {
    // A grid whose diagonal is well under the shipped separation, so no pair of land tiles on it can
    // satisfy the constraint and the relaxation branch is the only one that can run.
    const CRAMPED: UVec2 = UVec2::new(12, 10);
    assert!(
        (distance(UVec2::ZERO, CRAMPED)) < SHIPPED_SEPARATION as f64,
        "the fixture grid must be too small for the separation, or this tests nothing"
    );

    let world = world_with(ONE_RIVAL, |config| config.grid_size = CRAMPED);
    let starts = world.world.resource::<StartLocation>();

    let home = starts.position_for(HOME).expect("the human is placed");
    let rival = starts.position_for(RIVAL).expect("the AI is placed anyway");
    assert_ne!(
        home, rival,
        "relaxing the separation must not stack two peoples on one hex"
    );
    assert!(
        distance(home, rival) < SHIPPED_SEPARATION as f64,
        "the fixture is supposed to exercise the relaxed branch"
    );
    // ...and the relaxed branch spreads rather than stacks: the achieved distance must be a real
    // share of the ground available, not the adjacency the old "best remaining tile" fallback gave.
    let achievable = distance(UVec2::ZERO, CRAMPED);
    assert!(
        distance(home, rival) >= achievable * RELAXED_SHARE_OF_THE_AVAILABLE_GROUND,
        "relaxed to {:.1} on a grid whose diagonal is {achievable:.1} — the fallback is stacking, \
         not spreading ({home:?} vs {rival:?})",
        distance(home, rival)
    );
}

/// **The playtest map, and the rival count that broke it** (`map_seed` below, shipped grid and
/// preset).
///
/// A player reported a rival opening about two hexes from their own band on a heavily oceanic map.
/// It was not the *seed* — at 1, 2 and 3 rivals this map clears the separation comfortably (33.1,
/// 24.4 and 23.4 tiles). It was the **rival count**: `max_faction_starts` is land-blind, so the New
/// Game screen offers up to 11 rivals on a Standard grid, and past 7 the land runs out. The old
/// "take the best remaining tile" fallback then put each further start on the next-best hex, which
/// clusters — the achieved minimum collapsed to **1.00 tile** at 9 rivals and stayed there.
///
/// ⛔ **This is a REGRESSION FIXTURE, and its seed is deliberately not `HARNESS_MAP_SEED`.** It is
/// the reported map, pinned because it is the evidence — not a seed shopped for a passing result.
#[test]
fn the_playtest_map_spreads_a_full_rival_roster_instead_of_stacking_it() {
    const PLAYTEST_SEED: u64 = 10954655273796111774;

    let world = world_with(NO_RIVALS, |_| {});
    let grid = world
        .world
        .resource::<core_sim::SimulationConfig>()
        .grid_size;
    // Every rival the New Game screen would let this grid be asked for — the count at which the
    // land, not the lattice, is the binding constraint.
    let full_roster = core_sim::max_faction_starts(grid, SHIPPED_SEPARATION) - 1;

    for rivals in [ONE_RIVAL, ONE_RIVAL + 1, ONE_RIVAL + 2, full_roster] {
        let world = world_with(rivals, |config| config.map_seed = PLAYTEST_SEED);
        let starts = world.world.resource::<StartLocation>();
        let placed: Vec<UVec2> = (0..=rivals)
            .map(|faction| {
                starts
                    .position_for(core_sim::FactionId(faction))
                    .unwrap_or_else(|| panic!("faction {faction} is placed"))
            })
            .collect();

        let achieved = placed
            .iter()
            .enumerate()
            .flat_map(|(index, a)| placed.iter().skip(index + 1).map(|b| distance(*a, *b)))
            .fold(f64::INFINITY, f64::min);

        // Three rivals is well inside what this map's land can seat, so the constraint itself must
        // still hold; the full roster is past it, and there only the degradation is on trial.
        let floor = if rivals < full_roster {
            SHIPPED_SEPARATION as f64
        } else {
            SHIPPED_SEPARATION as f64 * RELAXED_SHARE_OF_THE_AVAILABLE_GROUND
        };
        assert!(
            achieved >= floor,
            "with {rivals} rivals the closest pair is {achieved:.2} apart, under {floor:.2}: \
             {placed:?}"
        );
    }
}

fn two_faction_world_app() -> App {
    world_with(ONE_RIVAL, |_| {})
}

/// A guard on the fixture itself: the control arm must really be one faction, or every comparison
/// above is between two identical worlds.
#[test]
fn the_control_arm_is_a_one_faction_world() {
    let world = world_with(NO_RIVALS, |_| {});
    assert_eq!(
        world
            .world
            .resource::<core_sim::FactionRegistry>()
            .factions(),
        [HOME]
    );
    assert_eq!(world.world.resource::<StartLocation>().len(), 1);
}
