//! **ONE BAND OF A PEOPLE PER TAKE SOURCE** (`core_sim::source_holders`, `.claude/rules/core_sim/work-party.md`
//! -> "One band of a people per source"): a chief never lists a patch, herd or deposit another band of
//! his people holds, and retires the duplicate row he loses. Two bands of one people stand together
//! on the harness world, both with a chief, only the food order listed.

use bevy::prelude::{App, Entity, UVec2, With};
use core_sim::keep_on_hand::source_key;
use core_sim::{
    build_test_app, scalar_from_f32, BandId, BandLeader, BandLeadership, BandOrder,
    LaborAllocation, LaborConfigHandle, LaborTarget, LeaderKind, OrderOwner, PopulationCohort,
    ResidentBand, SimulationConfig, SourcePriority, TakeSelection, DEFAULT_ESCAPEMENT_FLOOR,
};

const FOOD: &str = "food";
const NEED: f32 = 4.0;
const MANY_HANDS: f32 = 400.0;
const SPLIT_WORKERS: u32 = 6;

fn chief() -> BandLeadership {
    BandLeadership {
        leader: Some(BandLeader {
            name: "Ash".to_string(),
            kind: LeaderKind::Chief,
            since_turn: 0,
        }),
        ..Default::default()
    }
}

/// The harness world with every patch a gathering site, a chief on the opening band, and a second
/// band of the same people (also chiefed) split off it. Returns `(app, parent, child)`; the child's
/// `BandId` is the higher.
fn two_chiefs() -> (App, Entity, Entity) {
    let mut app = build_test_app();
    app.update();
    let template = app
        .world
        .resource::<core_sim::FoodSiteRegistry>()
        .sites()
        .first()
        .cloned()
        .expect("the world carries a gathering site");
    let mut sites = app
        .world
        .resource::<core_sim::FoodSiteRegistry>()
        .sites()
        .to_vec();
    let patches: Vec<UVec2> = app
        .world
        .resource::<core_sim::ForageRegistry>()
        .patches
        .keys()
        .copied()
        .collect();
    for position in patches {
        if !sites.iter().any(|site| site.position == position) {
            let mut site = template.clone();
            site.position = position;
            sites.push(site);
        }
    }
    app.world
        .resource_mut::<core_sim::FoodSiteRegistry>()
        .set_sites(sites);
    app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
    {
        let mut handle = app.world.resource_mut::<LaborConfigHandle>();
        let mut config = (*handle.0).clone();
        config.band_orders.ordered_kinds = vec![FOOD.to_string()];
        handle.0 = std::sync::Arc::new(config);
    }
    let parent = {
        let mut query = app.world.query_filtered::<Entity, With<ResidentBand>>();
        query.iter(&app.world).next().expect("a resident band")
    };
    app.world.entity_mut(parent).insert(chief());
    app.world
        .get_mut::<PopulationCohort>(parent)
        .unwrap()
        .working = scalar_from_f32(MANY_HANDS);
    let settle = core_sim::SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = core_sim::split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle)
        .expect("the split is admitted");
    let child = {
        let mut query = app.world.query::<(Entity, &BandId)>();
        query
            .iter(&app.world)
            .find(|(_, id)| **id == split.band)
            .map(|(entity, _)| entity)
            .expect("the child band")
    };
    app.world.entity_mut(child).insert(chief());
    for band in [parent, child] {
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.working = scalar_from_f32(MANY_HANDS);
        cohort.last_food_need = NEED;
        cohort
            .stores
            .reset_food("fresh_plant", scalar_from_f32(NEED));
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        allocation.orders.insert(
            FOOD.to_string(),
            BandOrder {
                target: 1000,
                priority: SourcePriority::Normal,
                owner: OrderOwner::Chief,
            },
        );
    }
    (app, parent, child)
}

fn id_of(app: &App, band: Entity) -> u64 {
    app.world.get::<BandId>(band).unwrap().0
}

fn restaff(app: &mut App, band: Entity) {
    core_sim::staffing::restaff_band(&mut app.world, band);
}

/// The keys of the rows the band's chief lists under food.
fn listed(app: &App, band: Entity) -> Vec<String> {
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    allocation
        .assignments
        .iter()
        .filter_map(|row| source_key(&row.target))
        .filter(|key| allocation.order_rows.get(key).map(String::as_str) == Some(FOOD))
        .collect()
}

fn holds(app: &App, band: Entity, key: &str) -> bool {
    app.world
        .get::<LaborAllocation>(band)
        .unwrap()
        .assignments
        .iter()
        .any(|row| source_key(&row.target).as_deref() == Some(key))
}

fn forage_target(key: &str) -> LaborTarget {
    let coords: Vec<u32> = key
        .trim_start_matches("f:")
        .split(',')
        .map(|part| part.parse().unwrap())
        .collect();
    LaborTarget::Forage {
        tile: UVec2::new(coords[0], coords[1]),
        floor: DEFAULT_ESCAPEMENT_FLOOR,
        species: None,
        take_species: TakeSelection::EVERYTHING,
    }
}

/// The best forage patch the lone parent's chief lists - what the child's chief would pick too.
fn the_parents_best_patch(app: &mut App, parent: Entity) -> String {
    restaff(app, parent);
    listed(app, parent)
        .into_iter()
        .find(|key| key.starts_with("f:"))
        .expect("**LIVENESS**: the chief gathers")
}

/// A player's row (not listed under the order) on `key` for `band`.
fn give_player_row(app: &mut App, band: Entity, key: &str) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .set_staffed_row(forage_target(key), Some(2), None);
}

/// A chief-listed row on `key` for `band`.
fn give_chief_row(app: &mut App, band: Entity, key: &str) {
    let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
    allocation.set_staffed_row(forage_target(key), None, None);
    allocation
        .order_rows
        .insert(key.to_string(), FOOD.to_string());
}

/// **A CHIEF NEVER LISTS A PATCH ANOTHER BAND OF HIS PEOPLE HOLDS - the player's row or a chief's.**
#[test]
fn a_chief_never_lists_a_patch_another_band_of_his_people_holds() {
    // The control: with no other band on it, the child's chief lists the patch the parent's does.
    let (mut app, parent, child) = two_chiefs();
    let best = the_parents_best_patch(&mut app, parent);
    app.world
        .get_mut::<LaborAllocation>(parent)
        .unwrap()
        .clear();
    restaff(&mut app, child);
    assert!(
        listed(&app, child).contains(&best),
        "**LIVENESS**: the chief lists the best patch when nobody else holds it"
    );

    // A player's row on it elsewhere in the people.
    let (mut app, parent, child) = two_chiefs();
    let best = the_parents_best_patch(&mut app, parent);
    app.world
        .get_mut::<LaborAllocation>(parent)
        .unwrap()
        .clear();
    give_player_row(&mut app, parent, &best);
    restaff(&mut app, child);
    assert!(
        !holds(&app, child, &best),
        "the child's chief takes no patch the parent's player row holds"
    );
    assert!(
        !listed(&app, child).is_empty(),
        "**LIVENESS**: he lists other ground"
    );

    // A chief's row there: the lower BandId keeps it, so the child's chief lists something else.
    let (mut app, parent, child) = two_chiefs();
    let best = the_parents_best_patch(&mut app, parent);
    assert!(holds(&app, parent, &best));
    restaff(&mut app, child);
    assert!(
        !holds(&app, child, &best),
        "the child's chief takes no patch the parent's chief lists"
    );
}

/// **A DUPLICATE CHIEF ROW IS RETIRED WHEN THE OTHER ROW IS THE PLAYER'S - on a tended patch too.**
#[test]
fn a_duplicate_chief_row_is_retired_when_the_other_row_is_the_players_even_on_a_tended_patch() {
    let (mut app, parent, child) = two_chiefs();
    let best = the_parents_best_patch(&mut app, parent);
    app.world
        .get_mut::<LaborAllocation>(parent)
        .unwrap()
        .clear();
    // The child's chief lists a row there (an old save, or he got there first), and the patch is
    // tended: an improvement row is normally never retired.
    give_chief_row(&mut app, child, &best);
    let tile = match forage_target(&best) {
        LaborTarget::Forage { tile, .. } => tile,
        _ => unreachable!(),
    };
    {
        let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
        let mut forage = app.world.resource_mut::<core_sim::ForageRegistry>();
        let patch = forage.patches.get_mut(&tile).expect("the patch");
        let position = patch.ladder_position() + 1.0;
        patch.set_ladder_position(position, &ladder);
    }
    // Control: nobody else holds it, so the improvement row stays.
    restaff(&mut app, child);
    assert!(
        holds(&app, child, &best),
        "**LIVENESS**: the improvement row stays while nobody else holds the patch"
    );
    // The parent's player now holds it: the chief's duplicate goes.
    give_player_row(&mut app, parent, &best);
    restaff(&mut app, child);
    assert!(
        !holds(&app, child, &best),
        "the duplicate is retired though the patch is tended"
    );
    assert!(holds(&app, parent, &best), "the player's row stays");
}

/// **BETWEEN TWO CHIEF ROWS THE HIGHER BANDID RETIRES AND THE LOWER KEEPS IT.**
#[test]
fn between_two_chief_rows_the_higher_band_id_retires_and_the_lower_keeps_it() {
    let (mut app, parent, child) = two_chiefs();
    assert!(
        id_of(&app, parent) < id_of(&app, child),
        "fixture: the child is the higher"
    );
    let best = the_parents_best_patch(&mut app, parent);
    give_chief_row(&mut app, child, &best);
    restaff(&mut app, parent);
    assert!(
        holds(&app, parent, &best),
        "the lower band keeps its listed row"
    );
    restaff(&mut app, child);
    assert!(
        !holds(&app, child, &best),
        "the higher band retires its duplicate"
    );
    assert!(holds(&app, parent, &best), "and the lower keeps the ground");
}

/// **A PINNED DUPLICATE STAYS UNTIL UNPINNED, THEN RETIRES.**
#[test]
fn a_pinned_duplicate_stays_until_it_is_unpinned_then_retires() {
    let (mut app, parent, child) = two_chiefs();
    let best = the_parents_best_patch(&mut app, parent);
    give_chief_row(&mut app, child, &best);
    let target = forage_target(&best);
    app.world
        .get_mut::<LaborAllocation>(child)
        .unwrap()
        .homeward
        .push(core_sim::work_party::HomewardWalk {
            target,
            workers: 1,
            turns_left: 2,
            walk_turns: 2,
            cargo: 5.0,
            classes: Default::default(),
            goods: Default::default(),
            rejoins_row: false,
        });
    restaff(&mut app, child);
    assert!(
        holds(&app, child, &best),
        "cargo is walking home from the source: the row is pinned and stays"
    );
    app.world
        .get_mut::<LaborAllocation>(child)
        .unwrap()
        .homeward
        .clear();
    restaff(&mut app, child);
    assert!(
        !holds(&app, child, &best),
        "the first pass that finds it unpinned retires it"
    );
}

/// **A BAND OF ANOTHER PEOPLE ON THE PATCH DOES NOT STOP THE CHIEF.**
#[test]
fn a_band_of_another_people_on_the_patch_does_not_stop_the_chief() {
    let (mut app, parent, child) = two_chiefs();
    let best = the_parents_best_patch(&mut app, parent);
    app.world
        .get_mut::<LaborAllocation>(parent)
        .unwrap()
        .clear();
    // The parent's people is another people now.
    app.world
        .get_mut::<PopulationCohort>(parent)
        .unwrap()
        .faction = core_sim::FactionId(1);
    give_player_row(&mut app, parent, &best);
    restaff(&mut app, child);
    assert!(
        listed(&app, child).contains(&best),
        "a rival people's row excludes nothing"
    );
}
