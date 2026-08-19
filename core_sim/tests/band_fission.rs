//! **Band fission — a band splits in two where it stands** (`docs/plan_band_fission.md`, issue #511).
//!
//! The player names a worker count and every other quantity divides on the share it implies. Two
//! things are worth pinning beyond the arithmetic:
//!
//! - **The two halves must SUM to what the band held.** Every conservation test below asserts on the
//!   pair, not on the new band alone: a split that quietly mints or loses people would pass any test
//!   that only looked at the side it was handed.
//! - **Each floor is tested as a PAIR.** A one-sided test passes against a gate that always refuses,
//!   so every refusal fixture has an admission twin built from the same world with one number
//!   changed.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::{
    available_workers, split_band_from_parent, split_refusals, BandEquipment, BandId,
    DemographicFlowAccumulator, ExpeditionConfigHandle, FactionId, PopulationCohort, ResidentBand,
    Scalar, SettleConfig, SimulationConfig, Tile, FODDER, FOOD,
};

/// Tolerance for a fixed-point round trip through a fractional share. `Scalar` carries far more
/// precision than this; the slack is here so an assertion fails on a *modelling* mistake rather than
/// on the last bit of the representation.
const EPSILON: f32 = 0.01;

/// The start-stocked item this test wears down before splitting. Any item a fresh band carries
/// would do; spears are the one every kit reaches for.
const WORN_ITEM: &str = "spears";

/// Condition to spend on the in-hand unit of [`WORN_ITEM`], on the config's 0–100 scale. A
/// mid-life reading, chosen only so the ledger can be mistaken for neither a fresh unit (`0`) nor a
/// retired one, and comfortably under the flint tier's `starting_durability`.
const WORN_CONDITION: f32 = 37.0;

/// Build a headless world on a pinned earthlike map — one `update()` runs the whole Startup worldgen
/// chain and resolves turn 1, so there is a real resident band standing on real terrain.
fn spawn_world() -> App {
    let mut app = core_sim::build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

/// The first resident band: its entity, faction and the tile it stands on.
fn home_band(app: &mut App) -> (Entity, FactionId, UVec2) {
    let (entity, faction, tile) = {
        let mut query = app
            .world
            .query_filtered::<(Entity, &PopulationCohort), With<ResidentBand>>();
        let (entity, cohort) = query
            .iter(&app.world)
            .next()
            .expect("the campaign spawns at least one resident band");
        (entity, cohort.faction, cohort.current_tile)
    };
    let position = app
        .world
        .get::<Tile>(tile)
        .expect("a band stands on a real tile")
        .position;
    (entity, faction, position)
}

/// The config the split runs against, with both floors wide open unless a test narrows them.
fn permissive_settle() -> SettleConfig {
    SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    }
}

/// A band's brackets as plain floats, for arithmetic the assertions can read.
fn brackets(app: &App, entity: Entity) -> (f32, f32, f32) {
    let cohort = app
        .world
        .get::<PopulationCohort>(entity)
        .expect("the band still exists");
    (
        cohort.children.to_f32(),
        cohort.working.to_f32(),
        cohort.elders.to_f32(),
    )
}

/// Find the band carrying `band_id`. The split allocates a fresh id, so this is how a test gets hold
/// of the half it just made.
fn entity_for_band(app: &mut App, band_id: BandId) -> Entity {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| **id == band_id)
        .map(|(entity, _)| entity)
        .expect("the split allocated this id")
}

/// Give the parent enough people that a meaningful split is possible on any seeded map, and a store
/// worth dividing. Returns the brackets it was set to.
fn stock_the_parent(app: &mut App, parent: Entity) -> (f32, f32, f32) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(parent)
        .expect("the home band exists");
    cohort.children = Scalar::from_f32(9.0);
    cohort.working = Scalar::from_f32(16.5);
    cohort.elders = Scalar::from_f32(4.5);
    cohort.stores.set(FOOD, Scalar::from_f32(96.0));
    cohort.stores.set(FODDER, Scalar::from_f32(8.0));
    cohort.sync_size();
    (9.0, 16.5, 4.5)
}

// -------------------------------------------------------------------------------------------
// The share
// -------------------------------------------------------------------------------------------

/// **The player's one input lands exactly, and everything else divides on the share it implies.**
///
/// Workers are the number asked for — not a share of anything — because that is the quantity the
/// player chose. Children and elders are `share × parent`, which is what makes the new band a
/// smaller copy of the one it came from rather than a party with a composition of its own
/// (`docs/plan_band_fission.md` §Q3).
#[test]
fn dependants_divide_on_the_worker_share() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    let (children, working, elders) = stock_the_parent(&mut app, parent);

    let asked = 6;
    let split = split_band_from_parent(&mut app.world, parent, asked, &permissive_settle())
        .expect("a permissive config admits a six-worker split");
    let share = asked as f32 / working;

    let child_entity = entity_for_band(&mut app, split.band);
    let (new_children, new_working, new_elders) = brackets(&app, child_entity);

    assert!(
        (new_working - asked as f32).abs() < EPSILON,
        "the new band holds exactly the workers asked for: {new_working} vs {asked}"
    );
    assert!(
        (new_children - children * share).abs() < EPSILON,
        "children divide on the worker share: {new_children} vs {}",
        children * share
    );
    assert!(
        (new_elders - elders * share).abs() < EPSILON,
        "elders divide on the worker share: {new_elders} vs {}",
        elders * share
    );
}

/// **The two halves sum to what the band held.** People are not minted and not lost, and the
/// assertion is on the *pair* — a test that only measured the new band would pass against a split
/// that forgot to debit the parent at all.
#[test]
fn the_two_halves_conserve_the_band() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    let (children, working, elders) = stock_the_parent(&mut app, parent);

    let split = split_band_from_parent(&mut app.world, parent, 6, &permissive_settle())
        .expect("the split is admitted");
    let child_entity = entity_for_band(&mut app, split.band);

    let (pc, pw, pe) = brackets(&app, parent);
    let (cc, cw, ce) = brackets(&app, child_entity);

    assert!((pc + cc - children).abs() < EPSILON, "children conserved");
    assert!((pw + cw - working).abs() < EPSILON, "workers conserved");
    assert!((pe + ce - elders).abs() < EPSILON, "elders conserved");
}

/// **A proportional split cannot move the parent's dependency ratio**, which is the whole reason
/// there is no ratio ceiling in the config (`docs/plan_band_fission.md` §Q2). This is the guard on
/// that claim: if per-bracket allocation ever comes back, this test fails and the deleted gate has
/// to come back with it.
#[test]
fn the_parents_dependency_ratio_does_not_move() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    let (children, working, elders) = stock_the_parent(&mut app, parent);
    let before = (children + elders) / working;

    split_band_from_parent(&mut app.world, parent, 6, &permissive_settle())
        .expect("the split is admitted");

    let (pc, pw, pe) = brackets(&app, parent);
    let after = (pc + pe) / pw;
    assert!(
        (after - before).abs() < EPSILON,
        "a proportional split leaves the ratio where it found it: {before} → {after}"
    );
}

/// **Every store divides on the same share, and both halves still sum to the whole.** Provisions are
/// the line the player reads, but the rule is "everything on the same fraction" — a store that was
/// special-cased would be a second answer to a question the share already answers.
#[test]
fn stores_divide_on_the_same_share_and_conserve() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    let (_, working, _) = stock_the_parent(&mut app, parent);

    let asked = 6;
    let split = split_band_from_parent(&mut app.world, parent, asked, &permissive_settle())
        .expect("the split is admitted");
    let share = asked as f32 / working;
    let child_entity = entity_for_band(&mut app, split.band);

    for (item, whole) in [(FOOD, 96.0_f32), (FODDER, 8.0_f32)] {
        let kept = app
            .world
            .get::<PopulationCohort>(parent)
            .expect("parent")
            .stores
            .get(item)
            .to_f32();
        let taken = app
            .world
            .get::<PopulationCohort>(child_entity)
            .expect("child")
            .stores
            .get(item)
            .to_f32();
        assert!(
            (taken - whole * share).abs() < EPSILON,
            "{item} divides on the share: {taken} vs {}",
            whole * share
        );
        assert!(
            (kept + taken - whole).abs() < EPSILON,
            "{item} is conserved across the split"
        );
    }
    assert!(
        (split.provisions.to_f32() - 96.0 * share).abs() < EPSILON,
        "the reported provisions are the ones actually handed over"
    );
}

// -------------------------------------------------------------------------------------------
// What the new band IS
// -------------------------------------------------------------------------------------------

/// **The new band is a resident band on the parent's tile, with its own id and a flow accumulator.**
///
/// The accumulator is not decoration: every resident band needs one or its births and deaths are
/// unreportable, and the split is a path that creates a band without going through worldgen.
#[test]
fn the_new_band_is_an_ordinary_resident_band_beside_its_parent() {
    let mut app = spawn_world();
    let (parent, faction, position) = home_band(&mut app);
    stock_the_parent(&mut app, parent);
    let parent_id = *app
        .world
        .get::<BandId>(parent)
        .expect("the parent has an id");

    let split = split_band_from_parent(&mut app.world, parent, 6, &permissive_settle())
        .expect("the split is admitted");
    let child_entity = entity_for_band(&mut app, split.band);

    assert_ne!(split.band, parent_id, "the new band takes a fresh id");
    assert_eq!(split.at, position, "both halves stand where the band stood");
    assert!(
        app.world.get::<ResidentBand>(child_entity).is_some(),
        "it is a resident band from the moment the command resolves"
    );
    assert!(
        app.world
            .get::<DemographicFlowAccumulator>(child_entity)
            .is_some(),
        "every resident band carries a flow accumulator or its births are unreportable"
    );
    let cohort = app
        .world
        .get::<PopulationCohort>(child_entity)
        .expect("child");
    assert_eq!(cohort.faction, faction, "a split is always same-faction");
    assert_eq!(
        cohort.age_turns, 0,
        "this band's life starts now — inheriting the parent's settled duration would let it \
         bleed people out on its first turn"
    );
}

/// **The kit is inherited WORN** (`docs/plan_band_fission.md` §Q4). `BandEquipment` is a wear ledger,
/// so handing the new band a `default()` would mint a fresh kit out of nothing every time a band
/// splits — which trivially defeats the pull into the crafting economy that running your kit dry is
/// supposed to be.
#[test]
fn the_kit_is_inherited_worn_rather_than_minted_fresh() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    stock_the_parent(&mut app, parent);

    // Wear the parent's kit into a state a fresh ledger could not be mistaken for.
    let worn = {
        let mut equipment = app
            .world
            .get_mut::<BandEquipment>(parent)
            .expect("a band carries a kit");
        // `restore_batches` is the direct setter the checkpoint path uses — the kit-driven
        // `wear_item` would need a config and a job, neither of which this test is about.
        let mut batches = equipment.batches_of(WORN_ITEM).to_vec();
        assert!(
            !batches.is_empty(),
            "a start-stocked band carries {WORN_ITEM}, which is what this fixture wears down"
        );
        batches[0].wear = WORN_CONDITION;
        equipment.restore_batches(WORN_ITEM, batches);
        equipment.clone()
    };
    assert_ne!(
        worn,
        BandEquipment::default(),
        "the fixture must actually have worn something, or this test proves nothing"
    );

    let split = split_band_from_parent(&mut app.world, parent, 6, &permissive_settle())
        .expect("the split is admitted");
    let child_entity = entity_for_band(&mut app, split.band);
    let inherited = app
        .world
        .get::<BandEquipment>(child_entity)
        .expect("the new band carries a kit");
    assert_eq!(
        *inherited, worn,
        "the splinter is exactly as worn out as the people it came from"
    );
}

/// **Grievance is inherited, not zeroed.** These are the same people who were unhappy a moment ago,
/// and a split that reset it would make forming a band a way to launder discontent — the same class
/// of move the proportional share exists to close.
#[test]
fn grievance_travels_with_the_people_who_hold_it() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    stock_the_parent(&mut app, parent);
    let grievance = Scalar::from_f32(0.4);
    app.world
        .get_mut::<PopulationCohort>(parent)
        .expect("parent")
        .grievance = grievance;

    let split = split_band_from_parent(&mut app.world, parent, 6, &permissive_settle())
        .expect("the split is admitted");
    let child_entity = entity_for_band(&mut app, split.band);
    assert_eq!(
        app.world
            .get::<PopulationCohort>(child_entity)
            .expect("child")
            .grievance,
        grievance,
        "the splinter carries the discontent it left with"
    );
}

// -------------------------------------------------------------------------------------------
// The floors
// -------------------------------------------------------------------------------------------

/// **`min_founding_workers` refuses a band too small to staff itself — and admits one that clears
/// it.** The pair is the test; the refusal alone would pass against a gate that always refuses.
#[test]
fn the_new_band_floor_refuses_below_it_and_admits_at_it() {
    let settle = SettleConfig {
        min_founding_workers: 4,
        parent_min_workers: 0,
    };
    let refused = split_refusals(3, 16, &settle);
    assert_eq!(
        refused.len(),
        1,
        "three workers is one thing wrong, not two: {refused:?}"
    );
    assert_eq!(refused[0].token(), "new_band_too_small");
    assert!(
        split_refusals(4, 16, &settle).is_empty(),
        "exactly the floor is admitted — the gate is `<`, not `<=`"
    );
}

/// **`parent_min_workers` refuses a split that hollows out the home band — and admits one that
/// leaves it standing.**
#[test]
fn the_parent_floor_refuses_below_it_and_admits_at_it() {
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 6,
    };
    let refused = split_refusals(11, 16, &settle);
    assert_eq!(refused.len(), 1, "one thing wrong: {refused:?}");
    assert_eq!(refused[0].token(), "parent_too_small");
    assert!(
        split_refusals(10, 16, &settle).is_empty(),
        "leaving exactly the floor is admitted"
    );
}

/// **Every applicable reason, never the first one.** A split that is both too small and leaves the
/// parent short has two things to fix; reporting one at a time teaches the rules one refusal at a
/// time — the player fixes it, presses again, and discovers the next.
#[test]
fn both_floors_report_together_when_both_hold() {
    let settle = SettleConfig {
        min_founding_workers: 4,
        parent_min_workers: 6,
    };
    // Three workers out of eight: too few to found, and it leaves five at home.
    let refused = split_refusals(3, 8, &settle);
    let tokens: Vec<_> = refused.iter().map(|r| r.token()).collect();
    assert_eq!(
        tokens,
        vec!["new_band_too_small", "parent_too_small"],
        "both gates are independent and both are reported"
    );
}

/// **A structural refusal stands alone.** Asking for more workers than the band has makes every
/// floor below it a statement about a split that cannot be made, so they would all fire at once and
/// say the same thing five ways.
#[test]
fn a_structural_refusal_does_not_drag_the_floors_in_with_it() {
    let settle = SettleConfig {
        min_founding_workers: 4,
        parent_min_workers: 6,
    };
    let refused = split_refusals(20, 8, &settle);
    assert_eq!(refused.len(), 1, "one reason: {refused:?}");
    assert_eq!(refused[0].token(), "not_enough_workers");

    let empty = split_refusals(0, 8, &settle);
    assert_eq!(empty.len(), 1, "one reason: {empty:?}");
    assert_eq!(empty[0].token(), "empty_split");
}

/// **A refusal leaves the parent exactly as it stood.** Nothing the player has invested is lost by
/// asking, and no band is created for a split that was refused.
#[test]
fn a_refused_split_writes_nothing() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    let (children, working, elders) = stock_the_parent(&mut app, parent);
    let bands_before = {
        let mut query = app.world.query_filtered::<Entity, With<ResidentBand>>();
        query.iter(&app.world).count()
    };

    let settle = SettleConfig {
        min_founding_workers: 4,
        parent_min_workers: 6,
    };
    let refused = split_band_from_parent(&mut app.world, parent, 2, &settle)
        .expect_err("two workers is below the floor");
    assert_eq!(refused.len(), 1, "one reason: {refused:?}");

    let (pc, pw, pe) = brackets(&app, parent);
    assert!((pc - children).abs() < EPSILON, "children untouched");
    assert!((pw - working).abs() < EPSILON, "workers untouched");
    assert!((pe - elders).abs() < EPSILON, "elders untouched");
    assert!(
        (app.world
            .get::<PopulationCohort>(parent)
            .expect("parent")
            .stores
            .get(FOOD)
            .to_f32()
            - 96.0)
            .abs()
            < EPSILON,
        "the larder is untouched"
    );
    let bands_after = {
        let mut query = app.world.query_filtered::<Entity, With<ResidentBand>>();
        query.iter(&app.world).count()
    };
    assert_eq!(bands_before, bands_after, "no band was created");
}

/// **The floors are counted in ASSIGNABLE workers**, which is the number the player is choosing
/// from. A cohort of 16.5 offers 16, and asking for the 17th is a structural refusal rather than a
/// split that quietly borrows half a person.
#[test]
fn the_choice_is_bounded_by_assignable_workers_not_the_fractional_cohort() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    stock_the_parent(&mut app, parent);
    let assignable = available_workers(
        app.world
            .get::<PopulationCohort>(parent)
            .expect("parent")
            .working,
    );
    assert_eq!(assignable, 16, "16.5 workers offers 16 assignable");

    let refused = split_band_from_parent(&mut app.world, parent, 17, &permissive_settle())
        .expect_err("the 17th worker is not there to give");
    assert_eq!(refused[0].token(), "not_enough_workers");
}

/// **The shipped config is the one the command runs against.** A test that built its own
/// `SettleConfig` everywhere would never notice the JSON drifting away from the struct.
#[test]
fn the_shipped_settle_config_carries_both_floors() {
    let app = spawn_world();
    let settle = app
        .world
        .resource::<ExpeditionConfigHandle>()
        .get()
        .settle
        .clone();
    assert_eq!(settle.min_founding_workers, 4);
    assert_eq!(settle.parent_min_workers, 6);
}
