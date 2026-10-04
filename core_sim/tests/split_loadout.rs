//! **A splinter's outfitting window** — the take half of the per-band loadout arc.
//!
//! Every band gets a window, and what bounds it is a fact about the **parent's** state rather than
//! about the turn (`.claude/rules/core_sim/starting-loadout.md`):
//!
//! - on turn one the parent still holds an unspent **grant**, so the splinter takes a slice of it,
//!   deducted from the parent's, and its picks MINT;
//! - from turn two nobody holds a grant, so the splinter's window is a **take** on the parent: its
//!   picks MOVE gear out of the parent's own ledger, and the cap is what the parent can supply
//!   **and** what the splinter can carry.
//!
//! **One carry rule on every turn** (#732): a departing band takes what its workers can carry,
//! `workers × per_worker_carry`, food first — turn one and turn fifty evaluate the same expression.
//!
//! Two properties are pinned throughout and are the whole point of the suite:
//!
//! - **A revision moves the DELTA, in whichever direction it points.** `3 → 5 → 2` must leave the
//!   two ledgers summing to what one ledger held, at every step.
//! - **A refusal leaves the world byte-identical.** Everything is checked before anything is
//!   written, so an order that cannot be honoured cannot half-land.

use std::collections::BTreeMap;

use bevy::prelude::*;

use core_sim::{
    allocation_load, apply_starting_loadout, build_test_app, carry_capacity, order_load,
    recapture_snapshot_in_place, run_turn, split_band_from_parent, split_default_outfit,
    BandEquipment, BandId, CarryConfig, CarryLoad, KitAllocation, LoadoutRejection, LoadoutSupply,
    MaterialAllocation, PopulationCohort, ResidentBand, Scalar, SettleConfig, SnapshotHistory,
    StartingLoadout,
};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// The faction every shipped profile spawns under.
const PLAYER: core_sim::FactionId = core_sim::FactionId(0);

/// The kit every take in this file is composed of, and the two items it puts in a hand. `sled` is
/// the roster's one shared item (`big_game` and `trapping` both use it), which is exactly why a kit
/// ROW cannot be capped on its own and the **expanded item list** is what the sim validates.
const BIG_GAME: &str = "big_game";
const SPEARS: &str = "spears";
const SLED: &str = "sled";

/// A material the shipped profile offers, so a grant window may spend points on it.
const BANKED_MATERIAL: &str = "hide";

/// The working-age head count the fixture parents are set to, chosen so a 12-worker split leaves a
/// half that can itself split 6 off — the three-band chain the onward-take refusal exists for.
const CHAIN_WORKERS: f32 = 24.0;

/// Both floors wide open, so a fixture's split is refused for loadout reasons or not at all.
fn permissive_settle() -> SettleConfig {
    SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    }
}

/// A world one update old, with the windows still open — the turn-one state.
fn world_on_the_build_turn() -> App {
    let mut app = build_test_app();
    app.update();
    app
}

/// The first resident band's entity and durable id.
fn home_band(app: &mut App) -> (Entity, BandId) {
    app.world
        .query_filtered::<(Entity, &BandId), With<ResidentBand>>()
        .iter(&app.world)
        .next()
        .map(|(entity, id)| (entity, *id))
        .expect("the campaign spawns a resident band")
}

fn entity_for_band(app: &mut App, band: BandId) -> Entity {
    app.world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .find(|(_, id)| **id == band)
        .map(|(entity, _)| entity)
        .expect("that band exists")
}

fn set_workers(app: &mut App, entity: Entity, workers: f32) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(entity)
        .expect("the band keeps a cohort");
    cohort.working = Scalar::from_f32(workers);
    cohort.sync_size();
}

/// **Empty a band's larder**, for a fixture whose subject is the GEAR a split hands over. Food loads
/// first into a splinter's packs, so a full larder leaves the gear only what the food does not fill;
/// the food half is pinned by its own tests below.
fn empty_the_larder(app: &mut App, entity: Entity) {
    app.world
        .get_mut::<PopulationCohort>(entity)
        .expect("the band keeps a cohort")
        .stores
        .reset_food("dry", Scalar::zero());
}

/// The live shipment/carry tuning a band's load is measured with.
fn carry_cfg(app: &App) -> CarryConfig {
    app.world
        .resource::<core_sim::ExpeditionConfigHandle>()
        .get()
        .carry
        .clone()
}

/// **The food mass a band is carrying** — `food + fodder_carry_weight × fodder`, the tier that loads
/// first.
fn food_mass_of(app: &App, entity: Entity) -> Scalar {
    let stores = &app
        .world
        .get::<PopulationCohort>(entity)
        .expect("the band keeps a cohort")
        .stores;
    CarryLoad {
        food: stores.get(core_sim::FOOD),
        fodder: stores.get(core_sim::FODDER),
        ..CarryLoad::default()
    }
    .food_mass(&carry_cfg(app))
}

/// **Re-declare the fixture band's gear, after the opening outfit has landed on it.**
///
/// `build_test_app` installs `for_a_stocked_fixture` and worldgen stocks the band from it — and then
/// the sim applies that band's **default outfit**, which is a *replacement* and rebuilds the ledger
/// from the profile's three default kits alone. A fixture whose subject is a *take* against a
/// well-stocked parent has to declare that stock again (`equipment.md` → "A FIXTURE DECLARES THE
/// STOCK").
fn restock_the_fixture_band(app: &mut App, band: Entity) {
    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    let recipes = app.world.resource::<core_sim::RecipesConfigHandle>().get();
    let materials = app
        .world
        .resource::<core_sim::MaterialsConfigHandle>()
        .get();
    let workers = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .working
        .to_f32();
    let ledger = BandEquipment::start_stocked_owned(&equipment, &recipes, &materials, workers);
    app.world.entity_mut(band).insert(ledger);
}

fn count_of(app: &App, entity: Entity, item: &str) -> u32 {
    app.world
        .get::<BandEquipment>(entity)
        .map(|ledger| ledger.count_of(item))
        .unwrap_or(0)
}

/// Every item either band holds, as `(item, units)` — the ledger a conservation assertion sums.
fn ledger_of(app: &App, entity: Entity) -> BTreeMap<String, u32> {
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

/// The two ledgers summed item by item — invariant across every revision, because a take MOVES.
fn combined(app: &App, left: Entity, right: Entity) -> BTreeMap<String, u32> {
    let mut total = ledger_of(app, left);
    for (item, units) in ledger_of(app, right) {
        *total.entry(item).or_default() += units;
    }
    total
}

fn kits(pairs: &[(&str, u32)]) -> Vec<KitAllocation> {
    pairs
        .iter()
        .map(|(kit_id, count)| KitAllocation {
            kit_id: (*kit_id).to_string(),
            count: *count,
        })
        .collect()
}

/// **This band's standing take**, `item → units` — the bookkeeping a revision is priced against.
fn standing_take(app: &App, band: BandId) -> BTreeMap<String, u32> {
    match &app
        .world
        .resource::<StartingLoadout>()
        .window(band)
        .expect("the band has a window")
        .supply
    {
        LoadoutSupply::Parent { items, .. } => items.clone(),
        other => panic!("expected a take window, got {other:?}"),
    }
}

/// A world where the windows have shut and `parent` has been split off the home band with `asked`
/// workers — the turn-two shape, in which every pick MOVES.
fn a_settled_split(asked: u32, parent_workers: f32) -> (App, Entity, BandId, Entity, BandId) {
    let mut app = world_on_the_build_turn();
    // A turn advance shuts every window, so the split below opens a TAKE rather than a grant.
    run_turn(&mut app);
    let (parent, parent_band) = home_band(&mut app);
    set_workers(&mut app, parent, parent_workers);
    // **The fixture declares the gear it needs.** A band is created holding its *default outfit*
    // (`starting-loadout.md` → "A default is applied, never suggested"), which is the profile's
    // three kits and no more — far short of the stock these take fixtures revise against. The
    // shipped `for_a_stocked_fixture` roster is what they were written for, so it is re-declared.
    restock_the_fixture_band(&mut app, parent);
    assert!(
        count_of(&app, parent, SPEARS) > 0,
        "**LIVENESS**: the fixture band must own gear, or every take below moves nothing"
    );

    let split = split_band_from_parent(&mut app.world, parent, asked, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);
    (app, parent, parent_band, child, split.band)
}

// -------------------------------------------------------------------------------------------
// A splinter's window
// -------------------------------------------------------------------------------------------

/// ⛔ **A SPLIT OPENS A WINDOW ON THE CHILD, STANDING AT THE DEFAULT TAKE IT WAS JUST GIVEN.**
///
/// The window is what makes the split's proportional share a *starting point* rather than a verdict:
/// the player revises it for the rest of the turn.
#[test]
fn a_split_opens_a_take_window_standing_at_the_default_share() {
    let (app, parent, parent_band, child, child_band) = a_settled_split(12, CHAIN_WORKERS);

    assert!(
        app.world.resource::<StartingLoadout>().is_open(child_band),
        "the splinter's window is open until the turn advances"
    );
    assert!(
        !app.world.resource::<StartingLoadout>().is_open(parent_band),
        "the parent's own window shut on the turn advance and a split does not re-open it"
    );

    let take = standing_take(&app, child_band);
    assert!(
        !take.is_empty(),
        "**LIVENESS**: the default take must have moved something: {take:?}"
    );
    for (item, units) in &take {
        assert_eq!(
            count_of(&app, child, item),
            *units,
            "'{item}': the recorded take is what actually crossed"
        );
    }
    let _ = parent;
}

/// ⛔ **A REVISION MOVES THE RIGHT DELTA IN BOTH DIRECTIONS.**
///
/// `3 → 5 → 2`: the raise pulls two more units off the parent, the cut hands three back. What makes
/// the raise possible at all is that the units **already taken** are priced as available — otherwise
/// going from 3 to 5 would be refused for the 3 that are already here.
#[test]
fn a_revision_moves_the_delta_in_both_directions() {
    let (mut app, parent, _, child, child_band) = a_settled_split(12, CHAIN_WORKERS);
    let whole = combined(&app, parent, child);

    for wanted in [3u32, 5, 2] {
        apply_starting_loadout(
            &mut app.world,
            PLAYER,
            child_band,
            &kits(&[(BIG_GAME, wanted)]),
            &[],
        )
        .unwrap_or_else(|reason| {
            panic!("a take of {wanted} is inside the parent's stock: {reason}")
        });

        assert_eq!(
            count_of(&app, child, SPEARS),
            wanted,
            "the splinter holds exactly the take it last named"
        );
        assert_eq!(
            count_of(&app, child, SLED),
            wanted,
            "and one sled per hand with it - `big_game` uses both"
        );
        assert_eq!(
            combined(&app, parent, child),
            whole,
            "a take MOVES: the two ledgers still sum to the one ledger the split divided"
        );
        assert_eq!(
            standing_take(&app, child_band)
                .get(SPEARS)
                .copied()
                .unwrap_or_default(),
            wanted,
            "the standing take is the record a further revision is priced against"
        );
    }
}

/// ⛔ **A PICK THE PARENT CANNOT COVER REFUSES THE WHOLE ORDER, AND THE WORLD IS BYTE-IDENTICAL.**
///
/// A loadout is one composition against one supply, so honouring the lines that happened to fit
/// would spend the take on something the player did not choose.
#[test]
fn a_pick_the_parent_cannot_cover_refuses_the_whole_order() {
    let (mut app, parent, _, child, child_band) = a_settled_split(12, CHAIN_WORKERS);
    let available = count_of(&app, parent, SPEARS)
        + standing_take(&app, child_band)
            .get(SPEARS)
            .copied()
            .unwrap_or(0);

    let before_parent = ledger_of(&app, parent);
    let before_child = ledger_of(&app, child);
    let before_windows = app.world.resource::<StartingLoadout>().clone();

    let reason = apply_starting_loadout(
        &mut app.world,
        PLAYER,
        child_band,
        &kits(&[(BIG_GAME, available + 1)]),
        &[],
    )
    .expect_err("a take beyond the parent's stock must be refused");
    // The kit puts a spear and a sled in the same hand and the parent holds one unit of each per
    // hand, so BOTH lines are short by one; the validation walks items in id order, so it names the
    // first of them. What matters is that the sentence points at a real line with real numbers.
    assert!(
        matches!(
            reason,
            LoadoutRejection::ParentCannotSupply { ref id, asked, available: had }
                if (id == SLED || id == SPEARS) && asked == available + 1 && had == available
        ),
        "got {reason:?}"
    );

    assert_eq!(
        ledger_of(&app, parent),
        before_parent,
        "the parent is untouched"
    );
    assert_eq!(
        ledger_of(&app, child),
        before_child,
        "the splinter is untouched"
    );
    assert_eq!(
        *app.world.resource::<StartingLoadout>(),
        before_windows,
        "and no window moved - a refusal changes nothing at all"
    );
}

/// ⛔ **A REVISION THAT WOULD STRAND AN ONWARD TAKE IS REFUSED, NOT CLAMPED.**
///
/// Ray's case: split 12, pick, split 6 off the splinter, pick — then revise the *first* take
/// downward. The middle band has already handed part of its stock onward, so lowering its take under
/// what it passed on would leave the third band holding units nothing accounts for. A silent clamp
/// would move a number the player did not name.
#[test]
fn a_revision_that_would_strand_an_onward_take_is_refused() {
    let (mut app, parent, _, middle, middle_band) = a_settled_split(12, CHAIN_WORKERS);

    // The middle band takes six kits off its parent, then sheds half its workers to a third band.
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        middle_band,
        &kits(&[(BIG_GAME, 6)]),
        &[],
    )
    .expect("six kits are inside the parent's stock");
    let split = split_band_from_parent(&mut app.world, middle, 6, &permissive_settle())
        .expect("the second split is admitted");
    let last_band = split.band;
    let last = entity_for_band(&mut app, last_band);

    // The third band's default take is kit-denominated off the middle band's six spears and six
    // sleds, and more than one roster kit carries a sled — so the onward take can hold more sleds
    // than spears. The floor is per ITEM: whatever went onward of each item.
    let onward_take = standing_take(&app, last_band);
    let onward = onward_take.get(SPEARS).copied().unwrap_or_default();
    assert!(
        onward > 0,
        "**LIVENESS**: the third band must have taken something, or the refusal below is vacuous"
    );
    assert_eq!(count_of(&app, last, SPEARS), onward);

    let before_middle = ledger_of(&app, middle);
    let reason = apply_starting_loadout(
        &mut app.world,
        PLAYER,
        middle_band,
        &kits(&[(BIG_GAME, onward - 1)]),
        &[],
    )
    .expect_err("a revision below the onward take must be refused");
    // Same reason as above: `big_game` moves a spear and a sled together, so both lines strand and
    // the walk names whichever sorts first. Either way the refusal quotes what the revision asked of
    // THAT item (one of each per kit, so `onward - 1`) against what went onward of THAT item.
    assert!(
        matches!(
            reason,
            LoadoutRejection::OnwardTakeStranded { ref id, asked, onward: stranded, .. }
                if (id == SLED || id == SPEARS)
                    && asked == onward - 1
                    && Some(&stranded) == onward_take.get(id.as_str())
        ),
        "got {reason:?} against an onward take of {onward_take:?}"
    );
    assert_eq!(
        ledger_of(&app, middle),
        before_middle,
        "a refusal leaves the middle band exactly as it stood"
    );

    // **And a revision that clears the onward take is accepted**, so the refusal above is a rule
    // about the floor and not a blanket ban on revising a band that has split. The revision that
    // sits exactly on the floor is the third band's own kit manifest: it expands to precisely what
    // went onward, item by item.
    let onward_kits = app
        .world
        .resource::<StartingLoadout>()
        .window(last_band)
        .expect("the third band has a window")
        .kits
        .clone();
    apply_starting_loadout(&mut app.world, PLAYER, middle_band, &onward_kits, &[])
        .expect("a take exactly at the onward floor is honoured");
    for (item, went_onward) in &onward_take {
        assert_eq!(
            count_of(&app, middle, item),
            0,
            "'{item}': everything the middle band still holds went onward, which is what the \
             floor describes"
        );
        assert_eq!(
            count_of(&app, last, item),
            *went_onward,
            "'{item}': and the third band keeps its take"
        );
    }
    let _ = parent;
}

// -------------------------------------------------------------------------------------------
// Turn one: each band's grant is its own carry
// -------------------------------------------------------------------------------------------

/// ⛔ **ON TURN ONE EACH BAND'S GRANT IS ITS OWN CARRY, AND THE TWO ADD UP TO THE PARENT'S BEFORE.**
///
/// Turn one is not a special case in the code — only the parent's *state* differs. Because the
/// parent still holds an unspent grant, the child gets a grant of its own and its picks MINT. Its
/// carry is `carry_capacity(asked)` — the workers who crossed — and the parent's is recomputed to
/// what its remaining workers carry. Carry is linear in workers, so the two add up to the parent's
/// carry before the split: no load unit is minted twice or lost. The parent's larder is fixed and
/// counts against its carry; the splinter's goods load first and its food fills what they leave.
#[test]
fn a_turn_one_splits_two_carries_add_up_to_the_parents_before_it() {
    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    set_workers(&mut app, parent, CHAIN_WORKERS);

    let parent_before = carry_of(&app, parent_band);
    assert!(
        parent_before > Scalar::zero(),
        "**LIVENESS**: the spawned band must hold a real carry, or the arithmetic below is 0 == 0"
    );

    let asked = 6;
    let split = split_band_from_parent(&mut app.world, parent, asked, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);

    let child_carry = carry_of(&app, split.band);
    let parent_after = carry_of(&app, parent_band);
    let child_food = food_mass_of(&app, child);
    assert!(
        child_carry > Scalar::zero() && child_food > Scalar::zero(),
        "**LIVENESS**: the splinter must hold a carry AND carry food: carry {child_carry}, food \
         {child_food}"
    );
    assert_eq!(
        child_carry,
        carry_capacity(asked, &carry_cfg(&app)),
        "the splinter's carry is the workers who crossed"
    );
    assert_eq!(
        parent_after + child_carry,
        parent_before,
        "the two carries add up to the parent's before the split - no load unit is minted twice \
         or lost"
    );
    // Σ(goods allowance + food mass): the parent's goods get its carry less its fixed larder, and the
    // splinter's goods and food share its whole carry.
    let parent_larder = food_mass_of(&app, parent);
    assert_eq!(
        grant_of(&app, parent_band) + parent_larder + child_carry,
        parent_before,
        "goods allowance plus larder on the parent, plus the splinter's carry, is the carry before"
    );
    assert!(
        allocated(&app, split.band) + child_food <= child_carry,
        "and the splinter's goods and food together fit its carry"
    );
    let child_budget = grant_of(&app, split.band);

    // The child MINTS: its picks are capped by its own carry and never by the parent's ledger.
    let per_kit = order_load(&carry_cfg(&app), &big_game_items(1), 0);
    let fits = (child_budget.raw() / per_kit.raw()) as u32;
    assert!(fits > 0, "**LIVENESS**: the grant must buy a kit");
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        split.band,
        &kits(&[(BIG_GAME, fits)]),
        &[],
    )
    .expect("the splinter may spend its whole carry");
    assert_eq!(count_of(&app, child, SPEARS), fits);

    let over = apply_starting_loadout(
        &mut app.world,
        PLAYER,
        split.band,
        &kits(&[(BIG_GAME, fits + 1)]),
        &[],
    )
    .expect_err("a grant window is capped by its carry");
    assert_eq!(
        over,
        LoadoutRejection::OverCarry {
            load: order_load(&carry_cfg(&app), &big_game_items(fits + 1), 0),
            capacity: child_budget,
        }
    );
}

/// `big_game × count`, expanded — a spear and a sled per kit.
fn big_game_items(count: u32) -> BTreeMap<String, u32> {
    [(SPEARS.to_string(), count), (SLED.to_string(), count)]
        .into_iter()
        .collect()
}

/// A band's cohort, by its durable id.
fn cohort_of(app: &App, band: BandId) -> &PopulationCohort {
    app.world
        .iter_entities()
        .find(|entity| entity.get::<BandId>() == Some(&band))
        .and_then(|entity| entity.get::<PopulationCohort>())
        .expect("that band keeps a cohort")
}

/// **A grant band's goods allowance** — what its goods may mint: its whole carry when its food
/// yields (a splinter), its carry less its fixed larder otherwise. Panics on a take window.
fn grant_of(app: &App, band: BandId) -> Scalar {
    let window = app
        .world
        .resource::<StartingLoadout>()
        .window(band)
        .expect("the band has a window");
    assert!(
        matches!(window.supply, LoadoutSupply::Grant { .. }),
        "expected a grant window, got {:?}",
        window.supply
    );
    window.goods_allowance(cohort_of(app, band), &carry_cfg(app))
}

/// **A band's whole carry** as its window reads it — goods and food together.
fn carry_of(app: &App, band: BandId) -> Scalar {
    app.world
        .resource::<StartingLoadout>()
        .window(band)
        .expect("the band has a window")
        .carry(cohort_of(app, band), &carry_cfg(app))
}

/// **A CLOSED WINDOW REFUSES, whoever the band is.** The turn advance shuts every window at once —
/// a splinter's included — and nothing re-opens one.
#[test]
fn the_turn_advance_shuts_every_window_including_a_splinters() {
    let (mut app, _, parent_band, _, child_band) = a_settled_split(12, CHAIN_WORKERS);
    assert!(app.world.resource::<StartingLoadout>().is_open(child_band));

    run_turn(&mut app);

    assert!(!app.world.resource::<StartingLoadout>().is_open(child_band));
    assert!(!app.world.resource::<StartingLoadout>().is_open(parent_band));
    for band in [parent_band, child_band] {
        assert_eq!(
            apply_starting_loadout(&mut app.world, PLAYER, band, &[], &[]),
            Err(LoadoutRejection::WindowClosed),
            "band {} may no longer be outfitted",
            band.0
        );
    }
}

/// **A band nobody opened a window for is `WindowClosed`, not a silent success.** There is no
/// campaign-wide window left to fall back on.
#[test]
fn a_band_with_no_window_is_refused() {
    let mut app = world_on_the_build_turn();
    run_turn(&mut app);
    let (_, band) = home_band(&mut app);
    assert_eq!(
        apply_starting_loadout(
            &mut app.world,
            PLAYER,
            band,
            &kits(&[(BIG_GAME, 1)]),
            &Vec::<MaterialAllocation>::new(),
        ),
        Err(LoadoutRejection::WindowClosed)
    );
}

// -------------------------------------------------------------------------------------------
// The window opens at the allocation, not at zero
// -------------------------------------------------------------------------------------------

/// This band's window's **published allocation** — the kit and material rows a client's card draws
/// itself from, and re-sends unchanged when the player presses `Set out` without touching anything.
fn published_allocation(app: &App, band: BandId) -> (Vec<KitAllocation>, Vec<MaterialAllocation>) {
    let window = app
        .world
        .resource::<StartingLoadout>()
        .window(band)
        .expect("the band has a window");
    (window.kits.clone(), window.materials.clone())
}

/// Every material either band holds, as `(id, units)` rounded to the whole units a card states.
fn materials_of(app: &App, entity: Entity) -> BTreeMap<String, i64> {
    let store = &app
        .world
        .get::<PopulationCohort>(entity)
        .expect("the band keeps a cohort")
        .stores;
    store
        .materials()
        .map(|(id, _)| (id.to_string(), store.material_total(id).0))
        .collect()
}

/// ⛔ **AN UNTOUCHED `Set out` IS A NO-OP, NOT A FORFEIT.**
///
/// The window's accepted rows were **empty** on a splinter for the whole first cut of this arc,
/// because the split's default take was a bare per-item manifest that names no kit. An apply is a
/// replacement, so a player who opened the auto-popped card and committed without touching it ordered
/// *take nothing* — and the proportional gear the split had just moved walked straight back to the
/// parent.
///
/// The fix is that the card is no longer empty when the take is not: the default take is denominated
/// in kits and published. **This asserts the round trip, not the rows** — re-sending exactly what the
/// window says must leave both ledgers and both stores byte-identical.
#[test]
fn re_sending_the_published_allocation_untouched_changes_nothing() {
    let (mut app, parent, _, child, child_band) = a_settled_split(12, CHAIN_WORKERS);

    let (published_kits, published_materials) = published_allocation(&app, child_band);
    assert!(
        !published_kits.is_empty(),
        "**LIVENESS**: the window must open at a real allocation, or the round trip below is \
         vacuous — an empty card commits an empty order and forfeits the dowry"
    );
    let child_before = ledger_of(&app, child);
    let parent_before = ledger_of(&app, parent);
    let child_materials_before = materials_of(&app, child);
    let parent_materials_before = materials_of(&app, parent);
    let food_before = (larder(&app, child), larder(&app, parent));
    assert!(
        child_before.values().sum::<u32>() > 0,
        "**LIVENESS**: the splinter must be holding gear, or the no-op below is trivially true"
    );

    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        child_band,
        &published_kits,
        &published_materials,
    )
    .expect("re-sending the window's own allocation is always affordable");

    assert_eq!(
        ledger_of(&app, child),
        child_before,
        "the splinter keeps every unit the split handed it"
    );
    assert_eq!(
        ledger_of(&app, parent),
        parent_before,
        "and nothing walked back to the parent"
    );
    assert_eq!(materials_of(&app, child), child_materials_before);
    assert_eq!(materials_of(&app, parent), parent_materials_before);
    assert_eq!(
        (larder(&app, child), larder(&app, parent)),
        food_before,
        "and the food the goods left room for is exactly the food already packed"
    );
}

/// **The published allocation IS what the band holds** — `expand_kits` of the kit rows is the
/// splinter's ledger, and the material rows are its store. The two denominations are one move, so a
/// card drawn from the rows describes the band beside it.
#[test]
fn the_published_allocation_expands_to_what_the_splinter_holds() {
    let (app, _, _, child, child_band) = a_settled_split(12, CHAIN_WORKERS);
    let (published_kits, published_materials) = published_allocation(&app, child_band);

    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    let mut expanded: BTreeMap<String, u32> = BTreeMap::new();
    for row in &published_kits {
        let definition = equipment
            .kit_definition(&row.kit_id)
            .expect("a published row names a roster kit");
        for item in &definition.uses {
            *expanded.entry(item.clone()).or_default() += row.count;
        }
    }
    assert_eq!(
        expanded,
        ledger_of(&app, child),
        "the kit rows expand to exactly the ledger the split handed over"
    );

    for row in &published_materials {
        assert_eq!(
            app.world
                .get::<PopulationCohort>(child)
                .expect("the splinter keeps a cohort")
                .stores
                .material_total(&row.material_id),
            Scalar::from_f32(row.units as f32),
            "'{}' is published at exactly the units that moved",
            row.material_id
        );
    }
}

/// ⛔ **A BENCH TOOL STAYS WITH THE WORKSHOP THAT BUILT IT.**
///
/// `billet`, `bone_awl`, `loom` and `tanning_frame` are the only items no kit `uses`, and the picker
/// is kit-denominated — so they can never appear in a take. That is a design statement rather than a
/// limitation: shop equipment is not a pair of hands' gear, and the alternative is an item that moves
/// invisibly and cannot be seen, adjusted or kept.
///
/// **The list grows with the roster and the rule does not.** `validate` rejects a kit that names a
/// bench tool, so *"un-kitted"* and *"bench tool"* are the same set by construction; the assertion
/// below is what makes a **non**-tool falling out of every kit fail loudly instead of quietly
/// leaving the loadout.
///
/// **The pair is the test.** A bench tool staying put proves nothing on its own — a split that moved
/// nothing at all would pass — so the ordinary gear beside it must still cross.
#[test]
fn a_bench_tool_does_not_walk_out_with_a_splinter() {
    let mut app = world_on_the_build_turn();
    run_turn(&mut app);
    let (parent, _) = home_band(&mut app);
    set_workers(&mut app, parent, CHAIN_WORKERS);

    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    let bench_tools: Vec<String> = equipment
        .items()
        .filter(|(id, _)| !equipment.item_is_kit_carried(id))
        .map(|(id, _)| id.to_string())
        .collect();
    assert_eq!(
        bench_tools,
        vec![
            "billet".to_string(),
            "bone_awl".to_string(),
            "loom".to_string(),
            "tanning_frame".to_string()
        ],
        "the roster's un-kitted items are exactly its bench tools; anything else here is an item \
         that fell out of every kit, which the loadout would silently stop carrying"
    );

    // Put a real bench tool in the parent's hands — nothing stocks one at spawn.
    const BENCH_TOOL_UNITS: u32 = 4;
    {
        let tier = equipment
            .item(&bench_tools[0])
            .expect("the roster carries it")
            .default_tier()
            .id
            .clone();
        let mut ledger = app
            .world
            .get_mut::<BandEquipment>(parent)
            .expect("the parent carries a kit");
        ledger.stock(&bench_tools[0], BENCH_TOOL_UNITS, &tier, None);
    }

    let split = split_band_from_parent(&mut app.world, parent, 12, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);

    assert_eq!(
        count_of(&app, parent, &bench_tools[0]),
        BENCH_TOOL_UNITS,
        "every unit of the bench tool stays with the parent"
    );
    assert_eq!(
        count_of(&app, child, &bench_tools[0]),
        0,
        "and none of it walks out with the splinter"
    );
    assert!(
        count_of(&app, child, SPEARS) > 0,
        "**LIVENESS**: ordinary gear must still cross, or a split that moved nothing would pass"
    );
    assert!(
        !standing_take(&app, split.band).contains_key(&bench_tools[0]),
        "and it is not recorded as taken either — the take is composed of kits, which name no bench \
         tool"
    );
}

// -------------------------------------------------------------------------------------------
// A grant split PARTITIONS THE GRANT — it moves nothing, and it charges the parent once
// -------------------------------------------------------------------------------------------

/// Spend the whole of the spawned band's carry, the way the shipped pre-fill invites, and return the
/// band for the assertions to measure against.
///
/// The material pre-fill is `bone 3 / fibre 17 / hide 8` — the composition the reported defect was
/// found on — and the kit column is filled to the brim beside it, so the whole carry is spent and a
/// meter can go negative if the arithmetic is wrong.
fn a_fully_outfitted_parent() -> (App, Entity, BandId) {
    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    let budget = grant_of(&app, parent_band);
    let pre_fill: Vec<MaterialAllocation> = {
        let profile = app.world.resource::<core_sim::ActiveStartProfile>();
        profile
            .profile()
            .overrides()
            .opening_loadout
            .material_defaults
            .iter()
            .map(|(material_id, units)| MaterialAllocation {
                material_id: material_id.clone(),
                units: *units,
            })
            .collect()
    };
    let spent: u32 = pre_fill.iter().map(|row| row.units).sum();
    let material_load = order_load(&carry_cfg(&app), &BTreeMap::new(), spent);
    assert!(
        spent > 0 && material_load <= budget,
        "fixture: the profile's pre-fill must fit the carry it is drawn against ({material_load} \
         of {budget})"
    );
    // Fill the kit column to the brim too, so the whole carry is spent.
    let per_kit = order_load(&carry_cfg(&app), &big_game_items(1), 0);
    let brim = ((budget - material_load).raw() / per_kit.raw()) as u32;
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        parent_band,
        &kits(&[(BIG_GAME, brim)]),
        &pre_fill,
    )
    .expect("the pre-fill and a full kit column both fit the opening grant");
    (app, parent, parent_band)
}

/// **What this band's standing allocation weighs** — what its carry meter reads as *spent*.
fn allocated(app: &App, band: BandId) -> Scalar {
    let window = app
        .world
        .resource::<StartingLoadout>()
        .window(band)
        .expect("the band has a window");
    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    allocation_load(
        &equipment,
        &carry_cfg(app),
        &window
            .kits
            .iter()
            .map(|row| (row.kit_id.clone(), row.count))
            .collect(),
        &window
            .materials
            .iter()
            .map(|row| (row.material_id.clone(), row.units))
            .collect(),
    )
}

/// The material units this band's standing allocation names.
fn allocated_units(app: &App, band: BandId) -> u32 {
    app.world
        .resource::<StartingLoadout>()
        .window(band)
        .expect("the band has a window")
        .materials
        .iter()
        .map(|row| row.units)
        .sum()
}

/// Every material this band holds, summed — in whole units, which is the currency the meter counts.
fn material_units_held(app: &App, entity: Entity) -> u32 {
    let store = &app
        .world
        .get::<PopulationCohort>(entity)
        .expect("the band keeps a cohort")
        .stores;
    store
        .materials()
        .map(|(id, _)| store.material_total(id).to_f32().round() as u32)
        .sum()
}

/// **What a band is holding, as a goods load** — its ledger's items and its material units, priced
/// the way an order is.
fn goods_held(app: &App, entity: Entity) -> Scalar {
    order_load(
        &carry_cfg(app),
        &ledger_of(app, entity),
        material_units_held(app, entity),
    )
}

/// ⛔ **A GRANT SPLIT PAYS ONCE — THE PAIR HOLDS EXACTLY THE PARENT'S OUTFIT.**
///
/// The splinter MINTS its default out of the parent's standing allocation, and exactly those rows
/// leave the parent's allocation, so the two bands together hold what the parent held — no more. It
/// once did the opposite of each half in turn: walked the manifest out of the parent's ledger *and*
/// deducted a slice of its budget (charged twice), and later let the parent keep its whole outfit
/// while the splinter minted its share on top (the pair held more than the parent ever chose).
///
/// **The fixture leaves the parent inside its reduced carry**, so no re-fit bites and the pair's sum
/// is exactly the deduction.
#[test]
fn a_grant_split_conserves_the_parents_outfit_across_the_pair() {
    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    // Gear is the subject; food loads first, so the larder would leave the splinter no room to mint.
    empty_the_larder(&mut app, parent);
    const ASKED: u32 = 5;
    const MODEST_KITS: u32 = 5;
    const MODEST_UNITS: u32 = 10;
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        parent_band,
        &kits(&[(BIG_GAME, MODEST_KITS)]),
        &[MaterialAllocation {
            material_id: BANKED_MATERIAL.to_string(),
            units: MODEST_UNITS,
        }],
    )
    .expect("a modest allocation fits the opening grant");

    let gear_before = ledger_of(&app, parent);
    let materials_before = material_units_held(&app, parent);
    let allocation_before = allocated(&app, parent_band);
    assert!(gear_before.values().sum::<u32>() > 0 && materials_before > 0);

    let split = split_band_from_parent(&mut app.world, parent, ASKED, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);
    assert!(
        allocation_before <= grant_of(&app, parent_band),
        "fixture: the parent must still fit what the split left it, or this is the re-fit's test"
    );

    assert!(
        ledger_of(&app, child).values().sum::<u32>() > 0,
        "**LIVENESS**: the splinter minted gear, or conservation is trivially true"
    );
    assert_eq!(
        combined(&app, parent, child),
        gear_before,
        "the pair holds exactly the parent's outfit - what the splinter minted left the parent"
    );
    assert_eq!(
        material_units_held(&app, parent) + material_units_held(&app, child),
        materials_before,
        "and so do its materials"
    );
    assert_eq!(
        allocated(&app, parent_band) + allocated(&app, split.band),
        allocation_before,
        "the two cards sum to the parent's card before the split"
    );
    assert_eq!(
        ledger_of(&app, child).values().sum::<u32>(),
        expanded_units(&app, &published_allocation(&app, split.band).0),
        "the splinter holds exactly its own MINTED default - nothing walked over from the parent"
    );
    assert_eq!(
        material_units_held(&app, child),
        allocated_units(&app, split.band),
        "and its material is what its own card claims"
    );
    assert!(
        allocated(&app, split.band) <= grant_of(&app, split.band),
        "minted against its own carry, never over it"
    );
}

/// Every item a kit allocation expands to, summed — the ledger a minted outfit comes to.
fn expanded_units(app: &App, kits: &[KitAllocation]) -> u32 {
    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    kits.iter()
        .map(|row| {
            equipment
                .kit_definition(&row.kit_id)
                .map(|definition| definition.uses.len() as u32 * row.count)
                .unwrap_or(0)
        })
        .sum()
}

/// ⛔ **THE METER CANNOT READ NEGATIVE.**
///
/// The reported symptom: a parent spent its whole opening allocation, split 5 workers — and its
/// resources meter read **`-6 / 22 left`**. The budget had been reduced and the standing allocation
/// had not.
#[test]
fn a_grant_split_leaves_the_parents_meter_non_negative() {
    let (mut app, parent, parent_band) = a_fully_outfitted_parent();
    let spent_before = allocated(&app, parent_band);
    let per_kit = order_load(&carry_cfg(&app), &big_game_items(1), 0);
    assert!(
        spent_before + per_kit > grant_of(&app, parent_band),
        "fixture: the parent must have spent its whole carry, or a negative meter is unreachable"
    );

    let split = split_band_from_parent(&mut app.world, parent, 5, &permissive_settle())
        .expect("the split is admitted");

    let spent_after = allocated(&app, parent_band);
    let budget = grant_of(&app, parent_band);
    assert!(
        spent_after <= budget,
        "the parent claims {spent_after} against a carry of {budget}"
    );
    assert!(allocated(&app, split.band) <= grant_of(&app, split.band));
}

/// ⛔ **THE GRANT IS CONSERVED ACROSS A GRANT SPLIT** — the assertion that would have caught the
/// duplication.
///
/// The parent's standing allocation was never re-fitted, so it still claimed what it had before
/// against a reduced budget. An apply is a **replacement built from empty**, so the parent's next
/// revision re-minted all of it while what had walked to the splinter stayed with it: material out
/// of nothing, on every turn-one split.
///
/// **The invariant is over the CARRY, not over the ledgers**, because on turn one a carry is the
/// currency and a ledger is a draft against it: the two bands' carries add up to the parent's
/// before, each band holds what its card claims, and each band's goods fit its own allowance.
#[test]
fn a_grant_split_conserves_the_grant() {
    let (mut app, parent, parent_band) = a_fully_outfitted_parent();
    let carry_before = carry_of(&app, parent_band);
    let held_before = goods_held(&app, parent);

    let split = split_band_from_parent(&mut app.world, parent, 5, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);

    let parent_budget = grant_of(&app, parent_band);
    let child_budget = grant_of(&app, split.band);
    assert_eq!(
        carry_of(&app, parent_band) + carry_of(&app, split.band),
        carry_before,
        "the two carries add up to the parent's before the split"
    );

    let parent_held = goods_held(&app, parent);
    let child_held = goods_held(&app, child);
    assert_eq!(
        parent_held,
        allocated(&app, parent_band),
        "the parent holds what its card claims"
    );
    assert_eq!(
        child_held,
        allocated(&app, split.band),
        "and so does the splinter"
    );
    assert!(
        parent_held + child_held <= held_before,
        "nothing is minted out of nothing: {parent_held} + {child_held} against {held_before}"
    );
    assert!(
        parent_held <= parent_budget,
        "the parent's goods fit what its carry leaves its larder: {parent_held} against \
         {parent_budget}"
    );
    assert!(
        child_held + food_mass_of(&app, child) <= child_budget,
        "and the splinter's goods and food together fit its carry"
    );

    // ⛔ **AND IT SURVIVES THE PARENT'S NEXT REVISION**, which is where the duplication actually
    // landed: a replacement rebuilt from empty re-minted the whole pre-split allocation.
    let (revised_kits, revised_materials) = {
        let window = app
            .world
            .resource::<StartingLoadout>()
            .window(parent_band)
            .expect("the parent's window is still open");
        (window.kits.clone(), window.materials.clone())
    };
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        parent_band,
        &revised_kits,
        &revised_materials,
    )
    .expect("re-sending the parent's own fitted allocation always fits");
    assert_eq!(
        goods_held(&app, parent) + goods_held(&app, child),
        parent_held + child_held,
        "the parent's next revision re-mints its FITTED allocation, not the one it had before the \
         split"
    );
}

/// ⛔ **THE SPLINTER'S OUTFIT IS ITS OWN DEFAULT, NOT THE PARENT'S LEFTOVERS.**
///
/// The splinter **mints its own default** against its own carry, which is a sensible opening outfit
/// rather than whatever a heavily-committed parent happened to be over by.
///
/// **Nothing is destroyed by not handing it over**, and the pairing says so: the parent sheds, the
/// splinter holds its default, and the two carries still add up to the parent's before.
#[test]
fn the_splinters_outfit_is_its_own_default_rather_than_the_parents_leftovers() {
    let (mut app, parent, parent_band) = a_fully_outfitted_parent();
    let spent_before = allocated(&app, parent_band);
    let carry_before = carry_of(&app, parent_band);

    let split = split_band_from_parent(&mut app.world, parent, 5, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);

    assert!(
        allocated(&app, parent_band) < spent_before,
        "**LIVENESS**: the re-fit must actually bite, or there is nothing to tell the two rules \
         apart"
    );
    let child_spent = allocated(&app, split.band);
    let child_budget = grant_of(&app, split.band);
    assert!(
        child_spent > Scalar::zero(),
        "the splinter is outfitted from creation: {child_spent} against a carry of {child_budget}"
    );
    assert!(
        child_spent <= child_budget,
        "and never over its own carry: {child_spent} against {child_budget}"
    );
    assert_eq!(
        goods_held(&app, child),
        child_spent,
        "it is actually holding what its card claims - applied, not suggested"
    );
    assert_eq!(
        carry_of(&app, parent_band) + carry_of(&app, split.band),
        carry_before,
        "and the two carries add up to the parent's before - no load unit is minted twice or lost"
    );
}

/// ⛔ **THE PARENT'S ALLOCATION LANDS INSIDE ITS REDUCED BUDGET WITH NOBODY COMMANDING ANYTHING.**
///
/// The reported `-6 / 22 left` was a *standing* allocation measured against a budget a split had
/// just shrunk. Now that the sim applies a band's default at creation, the parent's rows are a real
/// accepted allocation from turn one — so the re-fit has something to fit, and the meter cannot go
/// negative even for a player who never opened a card. **No command is sent in this test.**
#[test]
fn a_split_leaves_an_uncommanded_parent_inside_its_reduced_budget() {
    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    assert!(
        allocated(&app, parent_band) > Scalar::zero(),
        "**LIVENESS**: the parent must be standing on an applied default, or there is no \
         allocation for the re-fit to fit"
    );

    let split = split_band_from_parent(&mut app.world, parent, 5, &permissive_settle())
        .expect("the split is admitted");

    let budget = grant_of(&app, parent_band);
    let spent = allocated(&app, parent_band);
    assert!(
        spent <= budget,
        "the parent claims {spent} against a carry of {budget}"
    );
    assert_eq!(
        material_units_held(&app, parent),
        allocated_units(&app, parent_band),
        "and it is holding exactly what the re-fitted card claims"
    );
    assert!(
        allocated(&app, split.band) <= grant_of(&app, split.band),
        "the splinter's own card fits its own carry too"
    );
}

// -------------------------------------------------------------------------------------------
// A GRANT splinter's card opens on the campaign pre-fill — and it is asserted ON THE WIRE
// -------------------------------------------------------------------------------------------

/// One band's outfitting window **as it reaches a client**, decoded off the encoded envelope.
///
/// The resource is not the artifact: a row that never reaches the codec still satisfies an
/// in-process assertion, and the card the player stares at is built from the published frame.
#[derive(Debug, Clone, PartialEq)]
struct PublishedWindow {
    open: bool,
    carry_capacity: f32,
    food_share: f32,
    food_carried: f32,
    parent_band_id: u64,
    kits: Vec<(String, u32)>,
    materials: Vec<(String, u32)>,
}

impl PublishedWindow {
    /// What the published rows weigh, through the sim's own load formula.
    fn load(&self, app: &App) -> Scalar {
        let equipment = app
            .world
            .resource::<core_sim::EquipmentConfigHandle>()
            .get();
        allocation_load(
            &equipment,
            &carry_cfg(app),
            &self.kits.iter().cloned().collect(),
            &self.materials.iter().cloned().collect(),
        )
    }
}

/// Recapture the frame and read `band`'s published window out of it.
///
/// A split is a command, so it lands *between* two captures — hence the recapture, which is the same
/// refresh the server runs after every dispatched command. `run_turn` would shut the window instead.
fn published_window(app: &mut App, band: BandId) -> PublishedWindow {
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section is published")
        .iter()
        .find(|row| row.bandId() == band.0)
        .unwrap_or_else(|| panic!("band {} publishes a row", band.0));
    let window = row
        .loadoutWindow()
        .unwrap_or_else(|| panic!("band {} publishes an outfitting window", band.0));
    PublishedWindow {
        open: window.open(),
        carry_capacity: window.carryCapacity(),
        food_share: window.foodShare(),
        food_carried: window.foodCarried(),
        parent_band_id: window.parentBandId(),
        kits: window
            .kits()
            .map(|rows| {
                rows.iter()
                    .map(|row| (row.kitId().unwrap_or_default().to_string(), row.count()))
                    .collect()
            })
            .unwrap_or_default(),
        materials: window
            .materials()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        (
                            row.materialId().unwrap_or_default().to_string(),
                            row.units(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// ⛔ **A TURN-ONE SPLINTER IS CREATED ALREADY HOLDING ITS OWN DEFAULT, AND NOBODY COMMANDED IT.**
///
/// Reported from a live server: a band split on turn one published a real budget with **`kits: []`
/// and `materials: []`**, and the player — who had composed an outfit and never pressed *Set out* —
/// ended the turn with the band holding nothing. The record showed exactly one
/// `set_starting_loadout` that game, for the parent.
///
/// **A default that exists only as a client-side suggestion cannot survive a card nobody commits**,
/// so the sim applies it: the splinter mints the split's default — one kit per worker in its
/// parent's mix, its food, then materials — through the same accepted-order path a player's own
/// commit takes. No command is sent anywhere in this test.
///
/// Asserted on the **encoded envelope**, because the card is drawn from the published frame — but
/// the ledger and the store are asserted too, since a published row the band does not hold is
/// exactly the state this replaces.
#[test]
fn a_turn_one_splinter_is_created_already_holding_its_own_default() {
    let mut app = world_on_the_build_turn();
    let (parent, _) = home_band(&mut app);
    set_workers(&mut app, parent, CHAIN_WORKERS);
    // Gear is the subject, so the splinter's food share is emptied out of its carry.
    empty_the_larder(&mut app, parent);
    let (parent_kits, parent_materials) = {
        let (_, band) = home_band(&mut app);
        let window = app
            .world
            .resource::<StartingLoadout>()
            .window(band)
            .expect("the parent holds a window")
            .clone();
        let kits: BTreeMap<String, u32> = window
            .kits
            .iter()
            .map(|row| (row.kit_id.clone(), row.count))
            .collect();
        let materials: BTreeMap<String, u32> = window
            .materials
            .iter()
            .map(|row| (row.material_id.clone(), row.units))
            .collect();
        (kits, materials)
    };
    const ASKED: u32 = 5;

    let split = split_band_from_parent(&mut app.world, parent, ASKED, &permissive_settle())
        .expect("the split is admitted");
    let window = published_window(&mut app, split.band);

    assert!(window.open, "the splinter's window is open");
    assert_eq!(
        window.parent_band_id, 0,
        "the parent still granted, so this is a grant of the splinter's own"
    );
    assert!(
        window.carry_capacity > 0.0,
        "**LIVENESS**: the splinter must hold a real carry, or a blank card is the honest answer \
         and this test proves nothing: {window:?}"
    );
    assert_eq!(
        window.carry_capacity,
        grant_of(&app, split.band).to_f32(),
        "the published cap is the one the server refuses on"
    );
    assert!(
        !window.kits.is_empty() && !window.materials.is_empty(),
        "both columns publish rows against a carry of {}: {window:?}",
        window.carry_capacity
    );
    assert!(
        window.load(&app) <= grant_of(&app, split.band),
        "the rows fit the carry they are drawn against: {window:?}"
    );

    // **The rows are the split's default drawn from the parent's allocation** — its proportional
    // share of the parent's kits, `floor(kits × asked ÷ whole hands)`, in the parent's mix, then
    // materials in the room left — by the sim's own rule, called rather than restated.
    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    let parent_kit_total: u32 = parent_kits.values().sum();
    let share = parent_kit_total * ASKED / CHAIN_WORKERS as u32;
    let fitted = split_default_outfit(
        &parent_kits,
        &parent_materials,
        share,
        Scalar::zero(),
        carry_of(&app, split.band),
        &equipment,
        &carry_cfg(&app),
    );
    assert_eq!(
        (window.kits.clone(), window.materials.clone()),
        (fitted.kits, fitted.materials),
        "the rows are the parent's allocation, its proportional kit share, then materials"
    );
    assert_eq!(
        window.kits.iter().map(|(_, count)| *count).sum::<u32>(),
        share,
        "the splinter's proportional share of the parent's kits"
    );

    // ⛔ **AND THE BAND IS ACTUALLY STANDING IN IT.** A published row the band does not hold is the
    // suggestion this model replaced.
    let child = entity_for_band(&mut app, split.band);
    let mut expected: BTreeMap<String, u32> = BTreeMap::new();
    for (kit_id, count) in &window.kits {
        let definition = equipment
            .kit_definition(kit_id)
            .expect("a published row names a roster kit");
        for item in &definition.uses {
            *expected.entry(item.clone()).or_default() += count;
        }
    }
    assert_eq!(
        ledger_of(&app, child),
        expected,
        "the splinter's ledger is the expansion of its published kit rows - minted, not suggested"
    );
    for (material_id, units) in &window.materials {
        assert_eq!(
            app.world
                .get::<PopulationCohort>(child)
                .expect("the splinter keeps a cohort")
                .stores
                .material_total(material_id),
            Scalar::from_f32(*units as f32),
            "'{material_id}' is held at exactly the units its card claims"
        );
    }

    // ⛔ **AND IT WAS MINTED, NOT MOVED** — the parent's own ledger is untouched by the splinter's
    // outfit, which is what keeps a grant split from charging the parent twice.
    assert!(
        !ledger_of(&app, parent).is_empty(),
        "**LIVENESS**: the parent is standing on its own default too, so `MINTED not MOVED` is a \
         real claim rather than a statement about an empty ledger"
    );
}

/// **The same holds when the parent has spent its whole grant** — the splinter still mints its own
/// default, rather than being handed the parent's leftovers.
#[test]
fn a_splinter_of_a_fully_committed_parent_is_outfitted_too() {
    let (mut app, parent, parent_band) = a_fully_outfitted_parent();
    empty_the_larder(&mut app, parent);
    assert!(
        allocated(&app, parent_band) > Scalar::zero(),
        "fixture: the parent must have committed, or this is the other test"
    );

    let split = split_band_from_parent(&mut app.world, parent, 5, &permissive_settle())
        .expect("the split is admitted");
    let window = published_window(&mut app, split.band);

    assert!(window.open && window.parent_band_id == 0, "{window:?}");
    assert!(window.carry_capacity > 0.0, "**LIVENESS**: {window:?}");
    assert!(
        !window.kits.is_empty() && !window.materials.is_empty(),
        "the splinter of a committed parent publishes both halves: {window:?}"
    );
    assert!(
        window.load(&app) <= grant_of(&app, split.band),
        "and they fit the carry they are drawn against: {window:?}"
    );
}

/// ⛔ **THE TAKE PATH** — a turn-two splinter's card is the default take it was handed, not a
/// pre-fill, and its cap is the splinter's whole carry: goods first, food in the room they leave.
///
/// The rows expanding to exactly the ledger the split moved is what says they are the take.
#[test]
fn a_turn_two_splinters_card_is_still_the_take_it_was_handed() {
    let (mut app, _, parent_band, child, child_band) = a_settled_split(12, CHAIN_WORKERS);
    let window = published_window(&mut app, child_band);

    assert!(window.open, "{window:?}");
    assert_eq!(
        window.parent_band_id, parent_band.0,
        "and it names the band it is drawn from: {window:?}"
    );
    let carry = carry_capacity(12, &carry_cfg(&app));
    assert_eq!(
        window.carry_capacity,
        carry.to_f32(),
        "a take's cap is the splinter's whole carry: {window:?}"
    );
    assert!(
        !window.kits.is_empty() && window.food_carried > 0.0,
        "**LIVENESS**: the default take must move gear AND food: {window:?}"
    );
    assert!(
        window.load(&app) + food_mass_of(&app, child) <= carry,
        "the default take and the food it left room for fit the carry: {window:?}"
    );
    assert_eq!(
        window.food_carried,
        food_mass_of(&app, child).to_f32(),
        "the published food is the food the splinter holds"
    );

    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    let mut expanded: BTreeMap<String, u32> = BTreeMap::new();
    for (kit_id, count) in &window.kits {
        let definition = equipment
            .kit_definition(kit_id)
            .expect("a published row names a roster kit");
        for item in &definition.uses {
            *expanded.entry(item.clone()).or_default() += count;
        }
    }
    assert_eq!(
        expanded,
        ledger_of(&app, child),
        "the published kit rows expand to exactly the ledger the split moved - they are the take, \
         not a suggestion"
    );
    for (material_id, units) in &window.materials {
        assert_eq!(
            app.world
                .get::<PopulationCohort>(child)
                .expect("the splinter keeps a cohort")
                .stores
                .material_total(material_id),
            Scalar::from_f32(*units as f32),
            "'{material_id}' is published at exactly the units that moved"
        );
    }
}

// -------------------------------------------------------------------------------------------
// One carry rule on every turn (#732)
// -------------------------------------------------------------------------------------------

/// ⛔ **THE #732 PLAYTEST CASE: A TURN-TWO SPLINTER CANNOT TAKE MORE THAN IT CAN CARRY.**
///
/// The take used to be capped only by what the parent held, so two workers could dial the take up
/// to the parent's whole stock. Its goods are now capped by `workers × carry` too — the same
/// expression a turn-one grant is struck from — even when the parent holds far more.
#[test]
fn a_turn_two_splinter_cannot_take_more_than_it_can_carry() {
    const ASKED: u32 = 4;
    let (mut app, parent, _, child, child_band) = a_settled_split(ASKED, CHAIN_WORKERS);
    let allowance = carry_capacity(ASKED, &carry_cfg(&app));
    let per_kit = order_load(&carry_cfg(&app), &big_game_items(1), 0);
    let over = (allowance.raw() / per_kit.raw()) as u32 + 1;
    let supply = count_of(&app, parent, SPEARS)
        + standing_take(&app, child_band)
            .get(SPEARS)
            .copied()
            .unwrap_or(0);
    let sleds = count_of(&app, parent, SLED)
        + standing_take(&app, child_band)
            .get(SLED)
            .copied()
            .unwrap_or(0);
    assert!(
        over <= supply.min(sleds),
        "fixture: the parent must hold more than the splinter can carry ({over} kits against \
         {supply} spears / {sleds} sleds), or the refusal below is a supply refusal"
    );
    let before_windows = app.world.resource::<StartingLoadout>().clone();
    let before_child = ledger_of(&app, child);

    let reason = apply_starting_loadout(
        &mut app.world,
        PLAYER,
        child_band,
        &kits(&[(BIG_GAME, over)]),
        &[],
    )
    .expect_err("a take beyond the splinter's carry must be refused");
    assert_eq!(
        reason,
        LoadoutRejection::OverCarry {
            load: order_load(&carry_cfg(&app), &big_game_items(over), 0),
            capacity: allowance,
        }
    );
    assert_eq!(ledger_of(&app, child), before_child, "nothing moved");
    assert_eq!(
        *app.world.resource::<StartingLoadout>(),
        before_windows,
        "and no window moved - a refusal changes nothing at all"
    );

    // And the take that exactly fills the packs is honoured.
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        child_band,
        &kits(&[(BIG_GAME, over - 1)]),
        &[],
    )
    .expect("a take inside the carry is honoured");
}

/// ⛔ **A LARDER BIGGER THAN THE PACKS STAYS WITH THE PARENT.**
///
/// The proportional share of a huge larder dwarfs a splinter's carry. The split's default loads one
/// kit per worker first, then the food fills the rest of the packs, no materials ride, and the rest
/// of the larder stays home.
#[test]
fn a_splinter_of_a_huge_larder_takes_food_up_to_its_carry() {
    const HUGE_LARDER: u32 = 10_000;
    const ASKED: u32 = 6;
    let mut app = world_on_the_build_turn();
    run_turn(&mut app);
    let (parent, _) = home_band(&mut app);
    set_workers(&mut app, parent, CHAIN_WORKERS);
    app.world
        .get_mut::<PopulationCohort>(parent)
        .expect("the band keeps a cohort")
        .stores
        .reset_food("dry", Scalar::from_u32(HUGE_LARDER));
    let food_before = Scalar::from_u32(HUGE_LARDER);

    let split = split_band_from_parent(&mut app.world, parent, ASKED, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);
    let cap = carry_capacity(ASKED, &carry_cfg(&app));

    let child_food = app
        .world
        .get::<PopulationCohort>(child)
        .expect("the splinter keeps a cohort")
        .stores
        .get(core_sim::FOOD);
    let parent_food = app
        .world
        .get::<PopulationCohort>(parent)
        .expect("the parent keeps a cohort")
        .stores
        .get(core_sim::FOOD);
    let goods = goods_held(&app, child);
    let packed = food_mass_of(&app, child) + goods;
    assert!(
        goods > Scalar::zero(),
        "**LIVENESS**: the kits ride first, so the splinter holds gear"
    );
    assert!(
        packed <= cap && cap - packed < Scalar::one(),
        "the splinter's packs are full - its kits, then food - and no fuller: {packed} against \
         {cap}"
    );
    assert_eq!(
        parent_food + child_food,
        food_before,
        "the excess stays with the parent - nothing is lost"
    );
    assert_eq!(
        carry_of(&app, split.band),
        cap,
        "the window's cap is the splinter's whole carry"
    );
    assert!(
        material_units_held(&app, child) == 0,
        "and the food took the room the materials would have had"
    );
}

// -------------------------------------------------------------------------------------------
// A split loads GOODS FIRST; food fills the room left (#732)
// -------------------------------------------------------------------------------------------

/// A band's whole larder, food and hay — what a dowry moves.
fn larder(app: &App, entity: Entity) -> (Scalar, Scalar) {
    let stores = &app
        .world
        .get::<PopulationCohort>(entity)
        .expect("the band keeps a cohort")
        .stores;
    (stores.get(core_sim::FOOD), stores.get(core_sim::FODDER))
}

/// The splinter's dowry as its window records it.
fn dowry(app: &App, band: BandId) -> core_sim::SplitDowry {
    app.world
        .resource::<StartingLoadout>()
        .window(band)
        .expect("the splinter has a window")
        .dowry
        .clone()
        .expect("a split's window carries its dowry")
}

/// The food ledger's net received over this window — what the identity's transfer pair says moved.
fn food_ledger_net(app: &App, entity: Entity) -> f32 {
    let allocation = app
        .world
        .get::<core_sim::LaborAllocation>(entity)
        .expect("a band keeps an allocation");
    allocation.last_food_transfers.received() - allocation.last_food_transfers.sent()
}

/// Fixed-point food crosses the ledger as `f32`; this is the slack on comparing the two.
const LEDGER_EPSILON: f32 = 1e-3;

/// ⛔ **AN UNTOUCHED TURN-ONE SPLINTER ON THE SHIPPED PROFILE TAKES A KIT AND FOOD BOTH.**
/// Food-first left a four-worker splinter holding food alone; goods-first scales both tiers by
/// `carry ÷ (food share + goods)` when they do not both fit. On the shipped 7.0 pack a four-worker
/// splinter carries 28 against a 17.4 food share; materials are cut before tools, so it walks out
/// with `big_game`, `trapping` and `gathering` three each, no material, and 13 of food.
#[test]
fn an_untouched_turn_one_splinter_holds_a_kit_and_some_food() {
    let mut app = world_on_the_build_turn();
    let (parent, _) = home_band(&mut app);
    const ASKED: u32 = 4;
    let split = split_band_from_parent(&mut app.world, parent, ASKED, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);
    let window = published_window(&mut app, split.band);
    assert!(
        !window.kits.is_empty() && ledger_of(&app, child).values().sum::<u32>() > 0,
        "the splinter holds at least one kit: {window:?}"
    );
    assert!(
        larder(&app, child).0 > Scalar::zero() && window.food_carried > 0.0,
        "and some food: {window:?}"
    );
    assert!(
        window.food_carried <= window.food_share,
        "never more than its share"
    );
    assert!(
        window.load(&app) + food_mass_of(&app, child) <= grant_of(&app, split.band),
        "and the two together fit the slice"
    );
}

/// ⛔ **RAISING THE GOODS HANDS FOOD BACK TO THE PARENT; LOWERING THEM TAKES IT AGAIN, NEVER PAST
/// THE SHARE** — and the food ledger books every move, so the identity holds mid-window.
#[test]
fn a_revision_trades_food_for_goods_in_both_directions() {
    const ASKED: u32 = 4;
    let (mut app, parent, _, child, child_band) = a_settled_split(ASKED, CHAIN_WORKERS);
    let share = dowry(&app, child_band);
    let full = share.share_mass(&carry_cfg(&app));
    let carry = carry_capacity(ASKED, &carry_cfg(&app));
    let combined = |app: &App| larder(app, parent).0 + larder(app, child).0;
    let food_total = combined(&app);
    let parent_before_split_net = food_ledger_net(&app, parent);
    let child_food_at_split = larder(&app, child).0;

    // Lower the goods to nothing: the food rises to the whole share (it fits).
    apply_starting_loadout(&mut app.world, PLAYER, child_band, &[], &[])
        .expect("an empty take is always honoured");
    assert!(
        full <= carry,
        "fixture: the share must fit the carry for this case"
    );
    assert_eq!(
        food_mass_of(&app, child),
        full,
        "with no goods the splinter takes its whole share - and no more"
    );
    assert!(larder(&app, child).0 >= child_food_at_split);
    assert_eq!(combined(&app), food_total, "food moved, none was made");

    // Raise the goods past what leaves room for the share: food goes back to the parent.
    let per_kit = order_load(&carry_cfg(&app), &big_game_items(1), 0);
    let raise = ((carry - full).raw() / per_kit.raw()) as u32 + 2;
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        child_band,
        &kits(&[(BIG_GAME, raise)]),
        &[],
    )
    .expect("the raise is inside the carry and the parent's stock");
    let goods = order_load(&carry_cfg(&app), &big_game_items(raise), 0);
    assert!(
        food_mass_of(&app, child) < full,
        "raising the goods handed food back"
    );
    assert!(
        (food_mass_of(&app, child) - (carry - goods)).to_f32().abs() < LEDGER_EPSILON,
        "the food is exactly the room the goods leave"
    );
    assert_eq!(combined(&app), food_total, "food moved, none was lost");

    // The identity: each band's larder moved by exactly what its transfer ledger booked.
    let child_food_now = larder(&app, child).0.to_f32();
    assert!(
        (food_ledger_net(&app, child) - child_food_now).abs() < LEDGER_EPSILON,
        "the splinter's food is what its ledger received net: {} vs {child_food_now}",
        food_ledger_net(&app, child)
    );
    let parent_moved = food_ledger_net(&app, parent) - parent_before_split_net;
    assert!(
        (parent_moved + (child_food_now - child_food_at_split.to_f32())).abs() < LEDGER_EPSILON,
        "the parent's ledger books the other side of every revision"
    );

    // A goods load over the whole carry is refused.
    let over = (carry.raw() / per_kit.raw()) as u32 + 1;
    assert!(matches!(
        apply_starting_loadout(
            &mut app.world,
            PLAYER,
            child_band,
            &kits(&[(BIG_GAME, over)]),
            &[]
        ),
        Err(LoadoutRejection::OverCarry { .. }) | Err(LoadoutRejection::ParentCannotSupply { .. })
    ));
}

// -------------------------------------------------------------------------------------------
// When a fit has to cut, it cuts MATERIALS before TOOLS (#732 follow-up 3)
// -------------------------------------------------------------------------------------------

/// The workers the over-carry take below sends off. Six hands carry 48 at the shipped pack — room
/// for the proportional kit share of a 24-worker parent in its default outfit, with room left for
/// materials.
const OVER_CARRY_ASKED: u32 = 6;

/// A material heap far bigger than any splinter can carry: a quarter of it is 600 units against a
/// 48-load pack, so the proportional default is over the carry by the materials alone.
const MATERIAL_HEAP: f32 = 2_400.0;

/// **A turn-two take off a parent holding its default outfit, an empty larder and `hide_units` of
/// hide**, split `OVER_CARRY_ASKED` off. Returns the app and the splinter's id.
fn a_take_with_a_material_heap(hide_units: f32) -> (App, BandId) {
    let mut app = world_on_the_build_turn();
    run_turn(&mut app);
    let (parent, _) = home_band(&mut app);
    set_workers(&mut app, parent, CHAIN_WORKERS);
    // The parent keeps the default outfit it was created holding — not the restocked fixture
    // roster, whose kit share alone nearly fills a splinter's packs and leaves no room to show where
    // the materials go. The subject is what the GOODS budget is spent on, so the food tier is
    // emptied out of it.
    empty_the_larder(&mut app, parent);
    let table = app
        .world
        .resource::<core_sim::MaterialsConfigHandle>()
        .get();
    let readings: BTreeMap<String, f32> = table
        .material(BANKED_MATERIAL)
        .expect("the roster carries the banked material")
        .characteristics
        .iter()
        .map(|axis| (axis.clone(), core_sim::OPENING_MATERIAL_READING))
        .collect();
    let key = table
        .band_key(BANKED_MATERIAL, &readings)
        .expect("the opening reading resolves to a band");
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(parent)
            .expect("the band keeps a cohort");
        cohort.stores.clear_materials();
        cohort.stores.deposit_material(
            BANKED_MATERIAL,
            key,
            Scalar::from_f32(hide_units),
            &readings,
        );
    }
    let split = split_band_from_parent(
        &mut app.world,
        parent,
        OVER_CARRY_ASKED,
        &permissive_settle(),
    )
    .expect("the split is admitted");
    (app, split.band)
}

/// ⛔ **A TAKE WHOSE PROPORTIONAL DEFAULT IS OVER THE CARRY KEEPS EVERY KIT ROW WHOLE AND CUTS THE
/// MATERIALS.**
///
/// The seed-37 shape: a turn-five splinter's proportional default (kits + fibre 14 + hide 9) was
/// heavier than its packs, the uniform scale shrank its three baskets to nothing while the fibre
/// barely moved, and the band foraged at half the rate of the same crew with baskets. Tools feed a
/// band; materials can be gathered again. The control is the same split with no material at all —
/// its kits are what the share gives when nothing competes for the packs.
#[test]
fn a_take_over_its_carry_keeps_its_kits_whole_and_cuts_the_materials() {
    let (mut control, control_band) = a_take_with_a_material_heap(0.0);
    let control_window = published_window(&mut control, control_band);
    let cap = carry_capacity(OVER_CARRY_ASKED, &carry_cfg(&control));
    assert!(
        !control_window.kits.is_empty(),
        "**LIVENESS**: the control splinter must take kits, or there is nothing to keep whole"
    );
    assert!(
        control_window.load(&control) < cap,
        "fixture: the kit share alone must leave room for materials ({} against {cap})",
        control_window.load(&control)
    );

    let (mut app, band) = a_take_with_a_material_heap(MATERIAL_HEAP);
    let window = published_window(&mut app, band);
    assert_eq!(
        window.kits, control_window.kits,
        "every kit row the share gives is kept whole when the materials are what overflow"
    );
    let hide = window
        .materials
        .iter()
        .find(|(id, _)| id == BANKED_MATERIAL)
        .map(|(_, units)| *units)
        .unwrap_or(0);
    let proportional_share = (MATERIAL_HEAP as u32) * OVER_CARRY_ASKED / CHAIN_WORKERS as u32;
    assert!(
        hide > 0 && hide < proportional_share,
        "the materials fill the room the kits leave, cut from their share of {proportional_share}: \
         {hide} ({window:?} vs control {control_window:?}, cap {cap})"
    );
    assert!(
        window.load(&app) <= cap,
        "and the whole take fits the packs: {} against {cap}",
        window.load(&app)
    );
}

/// The working-age value a splinter of 4 drifts to after a turn of demographic flow — the seed-37
/// case, which floored to 3 workers.
const DRIFTED_WORKING: f32 = 3.99;

/// ⛔ **A SPLINTER WHOSE WORKING DRIFTS FROM 4.0 TO 3.99 KEEPS ITS 4-WORKER CARRY, ON THE WIRE.**
///
/// The published `carryCapacity` and the long-move forecast are priced on the band's actual
/// working-age value, so a drift of a hundredth of a worker costs a hundredth of a pack — never the
/// 32 → 24 step a floored head count took, which made a fresh splinter's first long move leave two
/// of its three baskets behind.
#[test]
fn a_splinter_drifting_below_four_workers_keeps_its_carry_on_the_wire() {
    const ASKED: u32 = 4;
    let (mut app, _, _, child, child_band) = a_settled_split(ASKED, CHAIN_WORKERS);
    set_workers(&mut app, child, DRIFTED_WORKING);
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section is published")
        .iter()
        .find(|row| row.bandId() == child_band.0)
        .expect("the splinter publishes a row");
    let per_worker = carry_cfg(&app).per_worker_carry;
    let floored = (ASKED - 1) as f32 * per_worker;
    assert!(
        (row.carryCapacity() - DRIFTED_WORKING * per_worker).abs() < 1e-3,
        "carry is the actual working value times one pack: {} (a floored count would read {floored})",
        row.carryCapacity()
    );
    // The split packs a splinter to its full carry, so a hundredth of a worker's drift is a
    // hundredth of a pack over — under one whole unit, never the 8-load step a floored count took.
    let overage = row.carryLoad() - row.carryCapacity();
    assert!(
        overage > 0.0 && overage < 1.0,
        "**LIVENESS**: the drift leaves the packed splinter a fraction of a unit over: load {} \
         against {}",
        row.carryLoad(),
        row.carryCapacity()
    );
    // ⛔ **An overage under one whole unit comes off the food** — a rounding drift does not cost a
    // tool. The published forecast is the plan the move would run.
    assert_eq!(row.longMoveLeavesItems(), 0, "no tool is left behind");
    assert_eq!(row.longMoveLeavesMaterials(), 0.0, "nor any material");
    assert!(
        (row.longMoveLeavesFood() - overage).abs() < 1e-3,
        "the {overage} of overage comes off the food: {}",
        row.longMoveLeavesFood()
    );
}

/// The published row of `band`, read off a fresh capture's encoded envelope.
fn with_published_row<R>(
    app: &mut App,
    band: BandId,
    read: impl FnOnce(fb::PopulationCohortState<'_>) -> R,
) -> R {
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section is published")
        .iter()
        .find(|row| row.bandId() == band.0)
        .unwrap_or_else(|| panic!("band {} publishes a row", band.0));
    read(row)
}

/// ⛔ **A FRESH SPLINTER IS NOT STARVING.** The split cleared the splinter's last meal but kept the
/// parent's last need, so the published `foodShortfall = need − eaten` read the parent's whole need
/// against a meal of nothing: "Short 4.09 food last turn — people are starving" on a band holding 22
/// food. The need is cleared with the meal it is measured against.
#[test]
fn a_fresh_splinter_publishes_no_food_shortfall() {
    let (mut app, parent, parent_band, _, child_band) = a_settled_split(4, CHAIN_WORKERS);
    let parent_need = app
        .world
        .get::<PopulationCohort>(parent)
        .expect("the parent keeps a cohort")
        .last_food_need;
    assert!(
        parent_need > 0.0,
        "**LIVENESS**: the parent must have eaten a turn, or there is no need to inherit"
    );
    let (child_shortfall, child_need) = with_published_row(&mut app, child_band, |row| {
        (row.foodShortfall(), row.foodNeed())
    });
    assert_eq!(
        child_shortfall, 0.0,
        "a band that has not yet eaten is short nothing"
    );
    assert_eq!(
        child_need, 0.0,
        "and it has no meal to measure a need against"
    );
    let _ = parent_band;
}

/// ⛔ **THE OPENING BAND'S LARDER IS FIXED, AND IT COUNTS AGAINST THE CARRY ON THE WIRE.**
///
/// What a band carries includes its food. The opening band cannot trade its spawned larder for tools
/// (there is nowhere to leave it), so its window publishes `foodFixed`, its `foodShare` and
/// `foodCarried` are both the larder's mass, and the goods allowance a client draws is
/// `carryCapacity − foodCarried` — the same number `OverCarry` refuses on. Its `carryCapacity` is
/// the band panel's own `carryCapacity`, so the two never disagree.
#[test]
fn the_opening_bands_window_publishes_a_fixed_larder_inside_its_carry() {
    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    let (window_carry, food_share, food_carried, food_fixed, band_carry) =
        with_published_row(&mut app, parent_band, |row| {
            let window = row.loadoutWindow().expect("the opening band has a window");
            (
                window.carryCapacity(),
                window.foodShare(),
                window.foodCarried(),
                window.foodFixed(),
                row.carryCapacity(),
            )
        });
    assert!(food_fixed, "the opening band's larder is fixed");
    let larder = food_mass_of(&app, parent).to_f32();
    assert!(larder > 0.0, "**LIVENESS**: the band holds a larder");
    assert_eq!((food_share, food_carried), (larder, larder));
    assert_eq!(
        window_carry, band_carry,
        "the card's carry is the band panel's carry"
    );
    assert!(
        (window_carry - food_carried - grant_of(&app, parent_band).to_f32()).abs() < 1e-3,
        "the goods allowance is the carry less the fixed larder"
    );
    assert!(
        allocated(&app, parent_band).to_f32() <= window_carry - food_carried,
        "and the default outfit the band holds fits it"
    );
}

// -------------------------------------------------------------------------------------------
// A split's default: one kit per worker, then food, then materials (#732 follow-up 7)
// -------------------------------------------------------------------------------------------

/// The playtest's opening outfit: 17 kits for 17 hands (Stalking 5 / Trapping 5 / Harvesting 7) and
/// 20 material units — a load of 47, inside the opening band's goods allowance.
fn the_playtest_outfit() -> (Vec<KitAllocation>, Vec<MaterialAllocation>) {
    (
        kits(&[(BIG_GAME, 5), ("trapping", 5), ("gathering", 7)]),
        vec![
            MaterialAllocation {
                material_id: "bone".to_string(),
                units: 2,
            },
            MaterialAllocation {
                material_id: "fibre".to_string(),
                units: 12,
            },
            MaterialAllocation {
                material_id: "hide".to_string(),
                units: 6,
            },
        ],
    )
}

/// The kits a band's window publishes, summed.
fn kit_count(window: &PublishedWindow) -> u32 {
    window.kits.iter().map(|(_, count)| *count).sum()
}

/// ⛔ **A SPLIT'S KITS ARE PROPORTIONAL, THEN THE SPLINTER'S FOOD, THEN MATERIALS.**
///
/// The playtest: Hornbeam outfitted 17 kits for 17 hands and 20 materials, then split 6 workers.
/// The splinter took 12 kits for 6 workers and walked out with 18 of its 26.1 food, while the parent
/// was re-fitted to 14 kits and no materials. The maintainer: *"match the number of kits to workers
/// and choose resources so we hit the food."* The splinter's kits are its proportional share —
/// `17 × 6 ÷ 17` = 6, a kit per worker because the parent had one per hand — then its whole food
/// share, then materials in what is left; the parent keeps what it has, fitted to the goods its
/// fixed larder leaves.
#[test]
fn a_playtest_split_gives_each_band_a_kit_per_worker_and_the_splinter_its_whole_food() {
    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    let (opening_kits, opening_materials) = the_playtest_outfit();
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        parent_band,
        &opening_kits,
        &opening_materials,
    )
    .expect("the playtest's outfit fits the opening band's goods allowance");
    const ASKED: u32 = 6;
    let split = split_band_from_parent(&mut app.world, parent, ASKED, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);

    let splinter = published_window(&mut app, split.band);
    assert_eq!(
        kit_count(&splinter),
        ASKED,
        "one kit per worker: {splinter:?}"
    );
    assert!(
        splinter.food_share > 0.0 && (splinter.food_carried - splinter.food_share).abs() < 1e-3,
        "the splinter carries its whole food share: {splinter:?}"
    );
    assert!(
        !splinter.materials.is_empty(),
        "materials fill the room the kits and the food leave: {splinter:?}"
    );
    assert!(
        allocated(&app, split.band) + food_mass_of(&app, child) <= carry_of(&app, split.band),
        "and kits, food and materials together fit its carry"
    );

    let parent_window = published_window(&mut app, parent_band);
    let mut pair_kits: BTreeMap<String, u32> = parent_window.kits.iter().cloned().collect();
    for (id, count) in &splinter.kits {
        *pair_kits.entry(id.clone()).or_default() += count;
    }
    let original: BTreeMap<String, u32> = opening_kits
        .iter()
        .map(|row| (row.kit_id.clone(), row.count))
        .collect();
    assert_eq!(
        pair_kits, original,
        "the pair's kits are the parent's 5/5/7 - the splinter's came out of them"
    );
    assert_eq!(kit_count(&parent_window), 11, "Hornbeam keeps 11 of its 17");
    for row in &opening_materials {
        let held = |window: &PublishedWindow| {
            window
                .materials
                .iter()
                .find(|(id, _)| *id == row.material_id)
                .map_or(0, |(_, units)| *units)
        };
        assert!(
            held(&parent_window) + held(&splinter) <= row.units,
            "'{}': the pair never holds more than the parent's {}",
            row.material_id,
            row.units
        );
    }
    assert!(
        allocated(&app, parent_band) <= grant_of(&app, parent_band),
        "fitted inside the goods its fixed larder leaves"
    );
}

/// ⛔ **A PARENT WITH SPARE KITS SHARES THE SPARES TOO.** 20 kits on 17 hands, splitting 6:
/// `floor(20 × 6 ÷ 17)` = 7 — there is no cap at one per worker.
#[test]
fn a_parent_with_spare_kits_shares_the_spares() {
    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    const SURPLUS_KITS: u32 = 20;
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        parent_band,
        &kits(&[("gathering", SURPLUS_KITS)]),
        &[],
    )
    .expect("twenty one-item kits fit the opening band's goods allowance");
    assert_eq!(
        core_sim::available_workers(cohort_of(&app, parent_band).working),
        17,
        "fixture: the shipped opening band has 17 whole hands"
    );
    let split = split_band_from_parent(&mut app.world, parent, 6, &permissive_settle())
        .expect("the split is admitted");
    let splinter = published_window(&mut app, split.band);
    assert_eq!(
        kit_count(&splinter),
        7,
        "floor(20 × 6 ÷ 17) = 7: {splinter:?}"
    );
}

/// A 4/4/4 outfit — 12 kits on the shipped band's 17 hands, short of one per hand.
fn twelve_kits() -> Vec<KitAllocation> {
    kits(&[(BIG_GAME, 4), ("trapping", 4), ("gathering", 4)])
}

/// ⛔ **A 4/4/4 OUTFIT IS 12 KITS ON 17 HANDS, AND A SPLIT TAKES ITS SHARE OF THEM.**
/// A split of 6 gets `floor(12 × 6 ÷ 17)` = 4 kits, a split of 4 gets `floor(12 × 4 ÷ 17)` = 2.
#[test]
fn a_twelve_kit_outfit_split_takes_its_proportional_share_of_the_kits() {
    for (asked, share) in [(6, 4), (4, 2)] {
        let mut app = world_on_the_build_turn();
        let (parent, parent_band) = home_band(&mut app);
        apply_starting_loadout(&mut app.world, PLAYER, parent_band, &twelve_kits(), &[])
            .expect("twelve kits fit the opening band's goods allowance");
        let split = split_band_from_parent(&mut app.world, parent, asked, &permissive_settle())
            .expect("the split is admitted");
        let splinter = published_window(&mut app, split.band);
        assert_eq!(
            kit_count(&splinter),
            share,
            "a split of {asked} gets {share} kits: {splinter:?}"
        );
    }
}

/// The working-age hands the short-parent case is struck on: 17 whole hands holding 12 kits.
const SHORT_PARENT_HANDS: f32 = 17.0;

/// ⛔ **A PARENT SHORT OF KITS SPLITS THEM PROPORTIONALLY — BOTH BANDS SHORT IN THE SAME PROPORTION.**
///
/// A split's kits are always the splinter's proportional share. A parent holding 12 kits on 17 hands
/// that sends 6 away gives the splinter `floor(12 × 6 ÷ 17)` = 4 kits — not 6 — and keeps the other
/// 8 for its 11, so neither band is stripped to outfit the other. Struck on the
/// take arm, where the kits physically move and the parent keeps what the splinter did not take.
#[test]
fn a_parent_short_of_kits_splits_them_proportionally() {
    let mut app = world_on_the_build_turn();
    let (_, opening_band) = home_band(&mut app);
    apply_starting_loadout(&mut app.world, PLAYER, opening_band, &twelve_kits(), &[])
        .expect("twelve kits fit the opening band's goods allowance");
    // The turn advance shuts the grant, so the split below is a take on what the parent holds.
    run_turn(&mut app);
    let (parent, _) = home_band(&mut app);
    set_workers(&mut app, parent, SHORT_PARENT_HANDS);
    // One unique item per kit on the opening outfit: a spear per Stalking kit, a trap per Trapping
    // kit, a basket per Harvesting kit — so their sum counts kits.
    let kits_held = |app: &App, entity: Entity| -> u32 {
        ["spears", "traps", "baskets"]
            .into_iter()
            .map(|item| count_of(app, entity, item))
            .sum()
    };
    const SHORT_KITS: u32 = 12;
    assert_eq!(
        kits_held(&app, parent),
        SHORT_KITS,
        "fixture: the band holds a 4/4/4 outfit - 12 kits on 17 hands"
    );

    const ASKED: u32 = 6;
    let split = split_band_from_parent(&mut app.world, parent, ASKED, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);
    let splinter = published_window(&mut app, split.band);
    const SPLINTER_SHARE: u32 = 4;
    assert_eq!(
        kit_count(&splinter),
        SPLINTER_SHARE,
        "floor(12 × 6 ÷ 17) = 4 kits, not one per worker: {splinter:?}"
    );
    assert_eq!(
        kits_held(&app, child),
        SPLINTER_SHARE,
        "and that is what it holds"
    );
    assert_eq!(
        kits_held(&app, parent),
        SHORT_KITS - SPLINTER_SHARE,
        "the parent keeps the other 8 for its 11 hands"
    );
}

/// ⛔ **THE SHIPPED OPENING DEFAULT FITS WHOLE, AND IS A KIT PER HAND.**
///
/// The profile's default is fitted on the world-build pass after the meal, against the room the
/// card shows, so the shipped 47-load outfit is held exactly as declared — no unit clamped — and
/// the published window has room to spare (`carryCapacity − foodCarried − goods ≥ 0`). It is a kit
/// per hand, so an untouched split hands its splinter a kit per worker.
#[test]
fn the_shipped_opening_default_fits_whole_and_an_untouched_split_gets_a_kit_per_worker() {
    let mut app = world_on_the_build_turn();
    let (_, parent_band) = home_band(&mut app);
    let (kit_defaults, material_defaults) = {
        let opening = &app
            .world
            .resource::<core_sim::ActiveStartProfile>()
            .profile()
            .overrides()
            .opening_loadout;
        (
            opening.kit_defaults.clone(),
            opening.material_defaults.clone(),
        )
    };
    let window = published_window(&mut app, parent_band);
    assert_eq!(
        window.kits.iter().cloned().collect::<BTreeMap<_, _>>(),
        kit_defaults,
        "the kit default is held whole"
    );
    assert_eq!(
        window.materials.iter().cloned().collect::<BTreeMap<_, _>>(),
        material_defaults,
        "and so is the material default"
    );
    assert!(
        window.carry_capacity - window.food_carried - window.load(&app).to_f32() >= 0.0,
        "the card has free carry left: {window:?}"
    );
    let hands = core_sim::available_workers(cohort_of(&app, parent_band).working);
    assert_eq!(
        kit_count(&window),
        hands,
        "the shipped default is a kit per hand"
    );

    for asked in [6, 4] {
        let mut app = world_on_the_build_turn();
        let (parent, parent_band) = home_band(&mut app);
        let split = split_band_from_parent(&mut app.world, parent, asked, &permissive_settle())
            .expect("the split is admitted");
        let splinter = published_window(&mut app, split.band);
        assert_eq!(
            kit_count(&splinter),
            asked,
            "an untouched split of {asked} gets a kit per worker: {splinter:?}"
        );
        let parent_window = published_window(&mut app, parent_band);
        assert_eq!(
            kit_count(&parent_window),
            hands - asked,
            "and the parent keeps the rest of its kit per hand: {parent_window:?}"
        );
    }
}
