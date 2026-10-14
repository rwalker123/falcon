//! **The done-when test of the start-archetype slice**: a Standard-preset world with N majors and M
//! minors opens with each people stocked and coloured by ITS archetype and ITS personality.
//!
//! - majors' starting bands are 30 strong, minors' are 12;
//! - minors open with no kits in their default outfit, majors with theirs;
//! - every faction's starting bands carry its drawn personality lean on the culture MODIFIER, and the
//!   transient `StartingCultureLean` component has been consumed;
//! - majors keep their separation from each other, and every minor clears the minor separation from
//!   every start (the placement half lives in `worldgen`'s unit tests; this reads the built world).
//!
//! See `.claude/rules/core_sim/factions.md` -> "Start archetypes" and "Personalities".

mod faction_support;

use bevy::prelude::*;

use core_sim::{
    seeded_modifiers_for_band, BandId, CultureManager, CultureOwner, FactionId, PersonalityCatalog,
    PopulationCohort, SimulationConfig, StartArchetypeAssignment, StartLocation,
    StartingCultureLean, StartingUnit, CULTURE_TRAIT_AXES, CULTURE_TRAIT_SPAN,
};
use faction_support::{world_with, HOME};

/// Major rivals beside the human, and minor peoples after them.
const MAJOR_RIVALS: u32 = 2;
const MINORS: u32 = 3;

/// The shipped band sizes (`start_profiles.json`), restated as the figures the done-when names.
const MAJOR_BAND_SIZE: u32 = 30;
const MINOR_BAND_SIZE: u32 = 12;

fn mixed_world() -> App {
    world_with(MAJOR_RIVALS + MINORS, |config: &mut SimulationConfig| {
        config.start_archetype_assignment =
            StartArchetypeAssignment::from_counts(&[MAJOR_RIVALS, MINORS]);
    })
}

/// Whether `faction` is a major people: the human plus the first `MAJOR_RIVALS` rivals.
fn is_major(faction: FactionId) -> bool {
    faction.0 <= MAJOR_RIVALS
}

fn starting_bands(app: &mut App) -> Vec<(FactionId, BandId, u32)> {
    let mut rows: Vec<_> = app
        .world
        .query_filtered::<(&BandId, &PopulationCohort), With<StartingUnit>>()
        .iter(&app.world)
        .map(|(band, cohort)| (cohort.faction, *band, cohort.size))
        .collect();
    rows.sort();
    rows
}

#[test]
fn majors_open_with_thirty_and_minors_with_twelve() {
    let mut app = mixed_world();
    let bands = starting_bands(&mut app);
    assert_eq!(
        bands.len() as u32,
        1 + MAJOR_RIVALS + MINORS,
        "one starting band per people, got {bands:?}"
    );
    for (faction, _, size) in bands {
        let expected = if is_major(faction) {
            MAJOR_BAND_SIZE
        } else {
            MINOR_BAND_SIZE
        };
        assert_eq!(size, expected, "{faction:?} opens with the wrong band size");
    }
}

#[test]
fn minors_open_with_no_kits_and_majors_with_theirs() {
    let mut app = mixed_world();
    let bands = starting_bands(&mut app);
    let loadout = app.world.resource::<core_sim::StartingLoadout>();
    let mut major_kits = 0;
    for (faction, band, _) in bands {
        let window = loadout
            .window(band)
            .unwrap_or_else(|| panic!("{faction:?}'s opening band has no window"));
        if is_major(faction) {
            assert!(
                !window.kits.is_empty(),
                "{faction:?} is a major: its default outfit carries kits"
            );
            major_kits += 1;
        } else {
            assert!(
                window.kits.is_empty(),
                "{faction:?} is a minor: its default outfit carries no kits, got {:?}",
                window.kits
            );
        }
    }
    assert_eq!(
        major_kits,
        1 + MAJOR_RIVALS,
        "liveness: every major was checked"
    );
}

/// Every faction's starting band carries its drawn personality on the culture MODIFIER, on top of
/// the band's own seeded character, and the transient component is gone.
#[test]
fn every_starting_band_carries_its_peoples_drawn_lean() {
    let mut app = mixed_world();
    let bands = starting_bands(&mut app);
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let amplitude = app
        .world
        .resource::<core_sim::CultureCorruptionConfigHandle>()
        .get()
        .culture()
        .propagation()
        .band_character_amplitude();
    let catalog = PersonalityCatalog::builtin();
    let span = core_sim::scalar_from_f32(CULTURE_TRAIT_SPAN);

    let mut leaning = 0;
    for (faction, band, _) in bands {
        let lean = catalog.draw(faction, map_seed).lean_vector();
        if lean.iter().any(|offset| *offset != core_sim::scalar_zero()) {
            leaning += 1;
        }
        let seeded = seeded_modifiers_for_band(band, amplitude);
        let modifier = *app
            .world
            .resource::<CultureManager>()
            .band_layer_by_owner(CultureOwner::from_band(band))
            .expect("the band's layer exists after the first update")
            .traits
            .modifier();
        for axis in 0..CULTURE_TRAIT_AXES {
            assert_eq!(
                modifier[axis],
                (seeded[axis] + lean[axis]).clamp(-span, span),
                "{faction:?} axis {axis}: the modifier is the band's character plus its people's lean"
            );
        }
    }
    assert!(
        leaning > 0,
        "liveness: at least one people drew a non-neutral personality, or this proves nothing"
    );
    assert_eq!(
        app.world
            .query::<&StartingCultureLean>()
            .iter(&app.world)
            .count(),
        0,
        "the lean is consumed by the band's first layer"
    );
}

/// The built world honours both separations: majors keep the major one from each other; each minor
/// clears the minor one from every start (relaxation can only shrink it, and warns - on a Standard
/// map this roster fits).
#[test]
fn starts_honour_each_archetypes_separation() {
    let app = mixed_world();
    let config = app.world.resource::<SimulationConfig>();
    let archetypes = &config.start_profile_overrides.archetypes;
    let (major_sep, minor_sep) = (
        f64::from(archetypes[0].min_separation),
        f64::from(archetypes[1].min_separation),
    );
    let starts = app.world.resource::<StartLocation>();
    let positions: Vec<(FactionId, UVec2)> = (0..=MAJOR_RIVALS + MINORS)
        .map(|id| {
            let faction = FactionId(id);
            (faction, starts.position_for(faction).expect("placed"))
        })
        .collect();
    let distance = |a: UVec2, b: UVec2| {
        let (dx, dy) = (a.x as f64 - b.x as f64, a.y as f64 - b.y as f64);
        (dx * dx + dy * dy).sqrt()
    };
    assert_eq!(positions[0].0, HOME);
    for (index, (faction, position)) in positions.iter().enumerate() {
        for (other, other_position) in positions.iter().take(index) {
            let needed = if is_major(*faction) {
                major_sep
            } else {
                minor_sep
            };
            assert!(
                distance(*position, *other_position) >= needed,
                "{faction:?} at {position:?} is closer than {needed} to {other:?} at {other_position:?}"
            );
        }
    }
}
