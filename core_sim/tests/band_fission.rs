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
    DemographicFlowAccumulator, EquipmentBatch, ExpeditionConfigHandle, FactionId,
    MaterialsConfigHandle, PopulationCohort, ResidentBand, Scalar, SettleConfig, SimulationConfig,
    Tile, FODDER, FOOD,
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

/// The material the conservation fixture banks on the parent. Any pickable one would do.
const BANKED_MATERIAL: &str = "hide";
/// Two readings a batch of [`BANKED_MATERIAL`] is banked at, far enough apart that an averaging move
/// would land between them rather than on either.
const COARSE_READING: f32 = 0.2;
const FINE_READING: f32 = 0.8;

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

/// Everything a band owns, as `(item, units)` — summed over batches, because a stock call appends
/// rather than merging.
fn owned(app: &App, entity: Entity) -> Vec<(String, u32)> {
    app.world
        .get::<BandEquipment>(entity)
        .map(|ledger| {
            ledger
                .batches()
                .map(|(id, batches)| {
                    (
                        id.to_string(),
                        batches.iter().map(|batch| batch.count).sum::<u32>(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whole units of `item` a band holds — `0` for a band with no ledger at all.
fn count_of(app: &App, entity: Entity, item: &str) -> u32 {
    app.world
        .get::<BandEquipment>(entity)
        .map(|ledger| ledger.count_of(item))
        .unwrap_or(0)
}

/// How much of `material` a band's store holds, in real units.
fn material_total(app: &App, entity: Entity, material: &str) -> f32 {
    app.world
        .get::<PopulationCohort>(entity)
        .expect("the band keeps a cohort")
        .stores
        .material_total(material)
        .to_f32()
}

/// The distinct per-axis readings a band's batches of `material` carry, in batch order — what a
/// merge would collapse to one value.
fn distinct_readings(app: &App, entity: Entity, material: &str) -> Vec<f32> {
    app.world
        .get::<PopulationCohort>(entity)
        .expect("the band keeps a cohort")
        .stores
        .material_batches(material)
        .filter_map(|(_, batch)| batch.characteristics.values().next().copied())
        .collect()
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

    // **A SHARE of the worn ledger, never a copy of it** — see `gear_is_conserved_across_a_split`.
    // What this pins is that the units that walked out carry the parent's real condition rather than
    // a fresh `default()`.
    let carried: Vec<f32> = inherited
        .batches_of(WORN_ITEM)
        .iter()
        .map(|batch| batch.wear)
        .collect();
    assert!(
        !carried.is_empty(),
        "a share of the parent's {WORN_ITEM} walked out with the splinter"
    );
    assert!(
        carried.iter().all(|wear| *wear == WORN_CONDITION),
        "the splinter is exactly as worn out as the people it came from: {carried:?}"
    );
    assert_ne!(
        *inherited, worn,
        "and it is a SHARE, not a copy — a whole-ledger clone mints a second kit every split"
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

// -------------------------------------------------------------------------------------------
// The manifest: gear and material MOVE, they are never copied
// -------------------------------------------------------------------------------------------

/// ⛔ **GEAR IS CONSERVED ACROSS A SPLIT** — the assertion that would have caught the duplication.
///
/// `split_band_from_parent` used to `clone()` the whole `BandEquipment` onto the splinter and never
/// debit the parent, so every split minted a second full kit out of nothing. A test that only looked
/// at the new band passes against that; the sum over **both halves** does not.
#[test]
fn gear_is_conserved_across_a_split() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    stock_the_parent(&mut app, parent);

    let before: Vec<(String, u32)> = owned(&app, parent);
    assert!(
        before.iter().any(|(_, units)| *units > 1),
        "**LIVENESS**: the fixture band must own several units of something, or a proportional \
         manifest has nothing to divide: {before:?}"
    );

    let split = split_band_from_parent(&mut app.world, parent, 6, &permissive_settle())
        .expect("the split is admitted");
    let child_entity = entity_for_band(&mut app, split.band);

    for (item, whole) in &before {
        let kept = count_of(&app, parent, item);
        let taken = count_of(&app, child_entity, item);
        assert_eq!(
            kept + taken,
            *whole,
            "'{item}': the two halves hold {kept} + {taken} of what was one ledger of {whole}"
        );
    }
    let moved: u32 = before
        .iter()
        .map(|(item, _)| count_of(&app, child_entity, item))
        .sum();
    assert!(
        moved > 0,
        "**LIVENESS**: a share of 6/16.5 must move something, or conservation is trivially true"
    );
}

/// ⛔ **THE MANIFEST IS A KIT ALLOCATION, AND IT FITS THE SHARE.**
///
/// It was a bare per-item `floor(share × count)`, which no kit allocation can express — so the
/// splinter's outfitting card opened empty while the band held the gear, and an untouched commit
/// handed the whole dowry back (`split_loadout.rs`). What crosses now is the expansion of a kit
/// allocation, and two properties follow:
///
/// - **no item exceeds its own share** of what the parent held; and
/// - **the sled identity holds** — `big_game` grants a spear and a sled, `trapping` a trap and a
///   sled, and `sled` is the roster's only shared item, so a kit-denominated take satisfies
///   `sleds == spears + traps` exactly. A per-item manifest does not, which is what makes this the
///   sharp statement of the denomination.
#[test]
fn the_manifest_is_a_kit_allocation_that_fits_the_share() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    let (_, working, _) = stock_the_parent(&mut app, parent);

    let before: Vec<(String, u32)> = owned(&app, parent);
    let asked = 6;
    let split = split_band_from_parent(&mut app.world, parent, asked, &permissive_settle())
        .expect("the split is admitted");
    let child_entity = entity_for_band(&mut app, split.band);

    for (item, whole) in &before {
        let budget = ((*whole as f64) * (asked as f64) / (working as f64)).floor() as u32;
        let taken = count_of(&app, child_entity, item);
        assert!(
            taken <= budget,
            "'{item}': {taken} taken against a share budget of {budget} out of {whole}"
        );
    }

    let spears = count_of(&app, child_entity, WORN_ITEM);
    let traps = count_of(&app, child_entity, "traps");
    let sleds = count_of(&app, child_entity, "sled");
    assert!(
        spears + traps > 0,
        "**LIVENESS**: the hunting kits must have moved, or the identity below is 0 == 0"
    );
    assert_eq!(
        sleds,
        spears + traps,
        "a kit-denominated take carries one sled per hunting hand, however the two kits split it"
    );
}

/// ⛔ **MATERIALS ARE CONSERVED TOO, WITH EVERY BATCH'S READINGS INTACT.**
///
/// They were not divided at *all* before this: the child's store was rebuilt from `LocalStore::new()`
/// plus the parent's `iter()`, which walks the **commodity** account only — so a splinter of a band
/// sitting on twenty hides opened with none of them, and the parent kept the lot.
#[test]
fn materials_are_conserved_across_a_split_with_their_readings() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    let (_, working, _) = stock_the_parent(&mut app, parent);

    // Two batches of one material at *different* readings, so an averaging move would be visible.
    let table = app.world.resource::<MaterialsConfigHandle>().get();
    let axes_of = |value: f32| -> std::collections::BTreeMap<String, f32> {
        table
            .material(BANKED_MATERIAL)
            .expect("the roster carries the banked material")
            .characteristics
            .iter()
            .map(|axis| (axis.clone(), value))
            .collect()
    };
    let banked: Vec<_> = [(COARSE_READING, 12.0_f32), (FINE_READING, 8.0)]
        .into_iter()
        .map(|(value, amount)| {
            let axes = axes_of(value);
            let key = table
                .band_key(BANKED_MATERIAL, &axes)
                .expect("the reading resolves to a band");
            (key, amount, axes)
        })
        .collect();
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(parent)
            .expect("the home band exists");
        for (key, amount, axes) in banked {
            cohort
                .stores
                .deposit_material(BANKED_MATERIAL, key, Scalar::from_f32(amount), &axes);
        }
    }
    let whole = 20.0_f32;

    // **A share big enough to reach BOTH batches** — 11 of 16.5 floors to 13 units, so the take spans
    // the 12-unit coarse batch and bites into the fine one. A smaller share would come entirely out
    // of the first batch and the no-averaging claim below would be vacuous.
    let asked = 11;
    let split = split_band_from_parent(&mut app.world, parent, asked, &permissive_settle())
        .expect("the split is admitted");
    let child_entity = entity_for_band(&mut app, split.band);

    let kept = material_total(&app, parent, BANKED_MATERIAL);
    let taken = material_total(&app, child_entity, BANKED_MATERIAL);
    assert!(
        (kept + taken - whole).abs() < EPSILON,
        "the material is conserved: {kept} kept + {taken} taken against {whole}"
    );
    // **Whole units, floored** — the take is published as an allocation and a card states `units:u32`,
    // so a fractional share is one it could not show and re-sending what it showed would hand the
    // remainder back (`split_loadout::re_sending_the_published_allocation_untouched_changes_nothing`).
    // The remainder stays with the parent, where the player can take it deliberately.
    let expected = ((whole as f64) * (asked as f64) / (working as f64)).floor() as f32;
    assert!(
        (taken - expected).abs() < EPSILON,
        "the material divides on the same share as everything else, floored to whole units: \
         {taken} against {expected}"
    );

    // **The readings survive.** A split is a move, not a merge, so the child holds batches at the
    // parent's own two ratings rather than one averaged pile.
    let child_readings = distinct_readings(&app, child_entity, BANKED_MATERIAL);
    assert!(
        child_readings.len() >= 2,
        "the splinter holds batches at BOTH ratings, not one averaged pile: {child_readings:?}"
    );
    for reading in child_readings {
        assert!(
            (reading - COARSE_READING).abs() < EPSILON || (reading - FINE_READING).abs() < EPSILON,
            "every rating that walked out is one the parent actually held, got {reading}"
        );
    }
}

/// ⛔ **THE FRESHEST UNITS LEAVE; THE PARENT KEEPS THE WORN STOCK.**
///
/// Ray's call, and it is the *opposite* of the ledger's internal wear order (`wear_item` spends the
/// most worn batch first): a new venture is outfitted properly. The fixture gives the parent one
/// worn batch and one fresh one and asserts on which side each ends up.
#[test]
fn the_freshest_units_leave_with_the_splinter() {
    let mut app = spawn_world();
    let (parent, _, _) = home_band(&mut app);
    stock_the_parent(&mut app, parent);

    // One worn batch and one fresh one, of equal size, so the share takes exactly one batch's worth
    // and which one it took is unambiguous.
    const BATCH_UNITS: u32 = 6;
    let tier = {
        let mut equipment = app
            .world
            .get_mut::<BandEquipment>(parent)
            .expect("a band carries a kit");
        let tier = equipment
            .batches_of(WORN_ITEM)
            .first()
            .map(|batch| batch.tier.clone())
            .expect("a start-stocked band carries the worn item");
        equipment.restore_batches(
            WORN_ITEM,
            vec![
                EquipmentBatch {
                    count: BATCH_UNITS,
                    tier: tier.clone(),
                    grade: None,
                    wear: WORN_CONDITION,
                },
                EquipmentBatch {
                    count: BATCH_UNITS,
                    tier: tier.clone(),
                    grade: None,
                    wear: 0.0,
                },
            ],
        );
        tier
    };
    let _ = tier;

    // Half the workers, so exactly `BATCH_UNITS` of the twelve move.
    let asked = {
        let cohort = app
            .world
            .get::<PopulationCohort>(parent)
            .expect("the home band exists");
        (cohort.working.to_f32() / 2.0).floor() as u32
    };
    let split = split_band_from_parent(&mut app.world, parent, asked, &permissive_settle())
        .expect("the split is admitted");
    let child_entity = entity_for_band(&mut app, split.band);

    let taken: Vec<f32> = app
        .world
        .get::<BandEquipment>(child_entity)
        .expect("the splinter carries a kit")
        .batches_of(WORN_ITEM)
        .iter()
        .map(|batch| batch.wear)
        .collect();
    assert!(
        !taken.is_empty() && taken.iter().all(|wear| *wear == 0.0),
        "the fresh units are the ones that walked out: {taken:?}"
    );
    let kept: Vec<f32> = app
        .world
        .get::<BandEquipment>(parent)
        .expect("the parent still carries a kit")
        .batches_of(WORN_ITEM)
        .iter()
        .map(|batch| batch.wear)
        .collect();
    assert!(
        kept.contains(&WORN_CONDITION),
        "the parent keeps the worn stock: {kept:?}"
    );
}
