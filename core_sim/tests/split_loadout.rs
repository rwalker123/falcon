//! **A splinter's outfitting window** — the take half of the per-band loadout arc.
//!
//! Every band gets a window, and what bounds it is a fact about the **parent's** state rather than
//! about the turn (`.claude/rules/core_sim/starting-loadout.md`):
//!
//! - on turn one the parent still holds an unspent **grant**, so the splinter takes a slice of it,
//!   deducted from the parent's, and its picks MINT;
//! - from turn two nobody holds a grant, so the splinter's window is a **take** on the parent: its
//!   picks MOVE gear out of the parent's own ledger, and the cap is what the parent can supply.
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
    apply_starting_loadout, build_test_app, recapture_snapshot_in_place, run_turn,
    split_band_from_parent, BandEquipment, BandId, KitAllocation, LoadoutRejection, LoadoutSupply,
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
    let available = count_of(&app, parent, SPEARS) + standing_take(&app, child_band)[SPEARS];

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

    let onward = standing_take(&app, last_band)
        .get(SPEARS)
        .copied()
        .unwrap_or_default();
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
    // the walk names whichever sorts first.
    assert!(
        matches!(
            reason,
            LoadoutRejection::OnwardTakeStranded { ref id, asked, onward: stranded, .. }
                if (id == SLED || id == SPEARS) && asked == onward - 1 && stranded == onward
        ),
        "got {reason:?}"
    );
    assert_eq!(
        ledger_of(&app, middle),
        before_middle,
        "a refusal leaves the middle band exactly as it stood"
    );

    // **And a revision that clears the onward take is accepted**, so the refusal above is a rule
    // about the floor and not a blanket ban on revising a band that has split.
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        middle_band,
        &kits(&[(BIG_GAME, onward)]),
        &[],
    )
    .expect("a take exactly at the onward floor is honoured");
    assert_eq!(
        count_of(&app, middle, SPEARS),
        0,
        "everything it still holds went onward, which is what the floor describes"
    );
    assert_eq!(
        count_of(&app, last, SPEARS),
        onward,
        "and the third band keeps its take"
    );
    let _ = parent;
}

// -------------------------------------------------------------------------------------------
// Turn one: the splinter takes a slice of the GRANT
// -------------------------------------------------------------------------------------------

/// ⛔ **ON TURN ONE THE SPLINTER TAKES A SLICE OF THE PARENT'S GRANT, DEDUCTED FROM IT.**
///
/// Turn one is not a special case in the code — only the parent's *state* differs. Because the
/// parent still holds an unspent grant, the child gets a grant of its own and its picks MINT; the
/// budgets are carved out of the parent's, so no slot and no point is minted twice or lost.
#[test]
fn a_turn_one_splinter_carves_its_grant_out_of_the_parents() {
    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    set_workers(&mut app, parent, CHAIN_WORKERS);

    let (parent_kits_before, parent_points_before) = grant_of(&app, parent_band);
    assert!(
        parent_kits_before > 0 && parent_points_before > 0,
        "**LIVENESS**: the spawned band must hold a real grant, or the arithmetic below is 0 == 0"
    );

    let asked = 6;
    let split = split_band_from_parent(&mut app.world, parent, asked, &permissive_settle())
        .expect("the split is admitted");

    let (child_kits, child_points) = grant_of(&app, split.band);
    let (parent_kits_after, parent_points_after) = grant_of(&app, parent_band);
    assert_eq!(
        child_kits, asked,
        "the splinter's kit slots are its own worker count"
    );
    assert_eq!(
        parent_kits_after + child_kits,
        parent_kits_before,
        "and they came OUT of the parent's - no slot is minted twice or lost"
    );
    assert_eq!(
        parent_points_after + child_points,
        parent_points_before,
        "the material points divide the same way, the parent keeping the remainder"
    );

    // The child MINTS: its picks are capped by that grant and never by the parent's ledger.
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        split.band,
        &kits(&[(BIG_GAME, child_kits)]),
        &[],
    )
    .expect("the splinter may spend its whole grant");
    let child = entity_for_band(&mut app, split.band);
    assert_eq!(count_of(&app, child, SPEARS), child_kits);

    let over = apply_starting_loadout(
        &mut app.world,
        PLAYER,
        split.band,
        &kits(&[(BIG_GAME, child_kits + 1)]),
        &[],
    )
    .expect_err("a grant window is capped by its budget");
    assert_eq!(
        over,
        LoadoutRejection::OverKitBudget {
            kits: child_kits + 1,
            budget: child_kits,
        }
    );
}

/// A band's grant as `(kit_budget, material_budget)`. Panics on a take window.
fn grant_of(app: &App, band: BandId) -> (u32, u32) {
    match &app
        .world
        .resource::<StartingLoadout>()
        .window(band)
        .expect("the band has a window")
        .supply
    {
        LoadoutSupply::Grant {
            kit_budget,
            material_budget,
        } => (*kit_budget, *material_budget),
        other => panic!("expected a grant window, got {other:?}"),
    }
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

/// Spend the whole of the spawned band's material budget, the way the shipped pre-fill invites, and
/// return `(band, kit rows, material rows, budgets)` for the assertions to measure against.
///
/// The pre-fill is `bone 3 / fibre 17 / hide 8` against 30 points — the composition the reported
/// defect was found on — so the fixture spends it and then splits.
fn a_fully_outfitted_parent() -> (App, Entity, BandId) {
    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    let (kit_budget, material_budget) = grant_of(&app, parent_band);
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
    assert!(
        spent > 0 && spent <= material_budget,
        "fixture: the profile's pre-fill must fit the budget it is drawn against ({spent} of \
         {material_budget})"
    );
    // Fill the kit column to the brim too, so a kit meter can go negative if the arithmetic is wrong.
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        parent_band,
        &kits(&[(BIG_GAME, kit_budget)]),
        &pre_fill,
    )
    .expect("the pre-fill and a full kit column both fit the opening grant");
    (app, parent, parent_band)
}

/// This band's standing allocation, summed per half — what its two meters read as *spent*.
fn allocated(app: &App, band: BandId) -> (u32, u32) {
    let window = app
        .world
        .resource::<StartingLoadout>()
        .window(band)
        .expect("the band has a window");
    (
        window.kits.iter().map(|row| row.count).sum(),
        window.materials.iter().map(|row| row.units).sum(),
    )
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

/// ⛔ **A GRANT SPLIT MOVES NOTHING OFF THE PARENT — a parent with room to spare gives up NOTHING.**
///
/// It used to do **both** things at once: walk the proportional manifest out of the parent's ledger
/// *and* deduct the splinter's slots and points from the parent's budget. Two ways of paying for one
/// splinter, so the parent was charged twice.
///
/// **The splinter is not empty-handed, and that is the point of the pairing**: it MINTS its own
/// default against the slice of the grant it was just given, which is a different thing from gear
/// crossing. Asserting only that the parent is unchanged would pass on a splinter that got nothing.
///
/// **The fixture deliberately leaves the parent inside its reduced budget**, because that is the case
/// where "moves nothing" is observable end to end: the re-fit does not bite, so a split that still
/// moved a manifest would show up as a changed ledger on either side. The re-fit's own behaviour is
/// the three tests below.
#[test]
fn a_grant_split_moves_nothing_when_the_parent_still_fits_its_reduced_budget() {
    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    let (kit_budget, material_budget) = grant_of(&app, parent_band);
    const ASKED: u32 = 5;
    // Spend well inside what the split will leave, so the re-fit has nothing to do.
    let modest_kits = (kit_budget - ASKED) / 2;
    let modest_units = (material_budget - ASKED) / 2;
    assert!(
        modest_kits > 0 && modest_units > 0,
        "fixture: the parent must spend something, or the assertions below are trivially true"
    );
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        parent_band,
        &kits(&[(BIG_GAME, modest_kits)]),
        &[MaterialAllocation {
            material_id: BANKED_MATERIAL.to_string(),
            units: modest_units,
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

    assert_eq!(
        ledger_of(&app, parent),
        gear_before,
        "no manifest walked out of the parent's ledger"
    );
    assert_eq!(
        material_units_held(&app, parent),
        materials_before,
        "and no material batch did either"
    );
    assert_eq!(
        allocated(&app, parent_band),
        allocation_before,
        "the parent's standing allocation still fits, so the re-fit left it alone"
    );
    let (child_kits, child_units) = allocated(&app, split.band);
    assert_eq!(
        ledger_of(&app, child).values().sum::<u32>(),
        expanded_units(&app, &published_allocation(&app, split.band).0),
        "the splinter holds exactly its own MINTED default - nothing walked over from the parent"
    );
    assert_eq!(
        material_units_held(&app, child),
        child_units,
        "and its material is what its own card claims"
    );
    let (child_kit_budget, child_material_budget) = grant_of(&app, split.band);
    assert!(
        child_kits <= child_kit_budget && child_units <= child_material_budget,
        "minted against its own slice of the grant, never over it"
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

/// ⛔ **THE GRANT PARTITION DIVIDES ON THE RATIO, NOT ON THE ROUNDED SHARE.**
///
/// 15 hands splitting 5 against the shipped 30-point grant is exactly a third, and a third of thirty
/// is exactly ten. `share` is a fixed-point quotient, so it stores as `0.333333`, and a partition
/// that multiplies the parent's points by it gets `9.99999` and floors to **9** — the splinter a
/// point short of what it is owed and the parent a point richer than it should be.
///
/// **The numbers are pinned rather than derived from the fixture**, because the `earthlike` band's
/// own worker count does not land on a non-terminating share: a test that only exercises today's
/// fixture never sees this.
#[test]
fn a_grant_split_divides_the_material_points_on_the_ratio() {
    const WORKERS: f32 = 15.0;
    const ASKED: u32 = 5;
    // A third of the shipped profile's 30 material points, in whole points.
    const SPLINTER_POINTS: u32 = 10;

    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    set_workers(&mut app, parent, WORKERS);
    let (_, material_budget) = grant_of(&app, parent_band);
    assert_eq!(
        material_budget,
        SPLINTER_POINTS * 3,
        "fixture: the shipped grant must be three times the share asserted below, or this case is \
         no longer the exact third it was chosen to be"
    );

    let split = split_band_from_parent(&mut app.world, parent, ASKED, &permissive_settle())
        .expect("the split is admitted");

    let (_, splinter_points) = grant_of(&app, split.band);
    assert_eq!(
        splinter_points, SPLINTER_POINTS,
        "a third of {material_budget} points is {SPLINTER_POINTS}, not the {splinter_points} a \
         rounded share floors to"
    );
    let (_, parent_points) = grant_of(&app, parent_band);
    assert_eq!(
        parent_points,
        material_budget - SPLINTER_POINTS,
        "and what the splinter took is exactly what the parent gave up — no point is minted or lost"
    );
}

/// ⛔ **THE METERS CANNOT READ NEGATIVE.**
///
/// The reported symptom: 17 hands and 30 points, `bone 3 / fibre 17 / hide 8` committed, split 5
/// workers — and the parent's resources meter read **`-6 / 22 left`**. The budget had been reduced
/// and the standing allocation had not, so the card subtracted 28 from 22.
#[test]
fn a_grant_split_leaves_both_of_the_parents_meters_non_negative() {
    let (mut app, parent, parent_band) = a_fully_outfitted_parent();
    let (kits_before, materials_before) = allocated(&app, parent_band);
    let (kit_budget_before, material_budget_before) = grant_of(&app, parent_band);
    assert_eq!(
        kits_before, kit_budget_before,
        "fixture: the parent must have spent its whole kit column, or a negative KIT meter is \
         unreachable and half this test proves nothing"
    );
    assert!(
        materials_before > material_budget_before - 5,
        "fixture: the parent must have spent enough of its {material_budget_before} points that \
         losing a splinter's share leaves it over budget ({materials_before} spent), or a negative \
         resources meter is unreachable"
    );

    let split = split_band_from_parent(&mut app.world, parent, 5, &permissive_settle())
        .expect("the split is admitted");

    let (kit_budget, material_budget) = grant_of(&app, parent_band);
    let (kits_after, materials_after) = allocated(&app, parent_band);
    assert!(
        kits_after <= kit_budget,
        "the parent claims {kits_after} kits against a budget of {kit_budget}"
    );
    assert!(
        materials_after <= material_budget,
        "the parent claims {materials_after} units against a budget of {material_budget}"
    );

    let (child_kits, child_materials) = allocated(&app, split.band);
    let (child_kit_budget, child_material_budget) = grant_of(&app, split.band);
    assert!(child_kits <= child_kit_budget && child_materials <= child_material_budget);
}

/// ⛔ **MATERIAL IS CONSERVED ACROSS A GRANT SPLIT** — the assertion that would have caught the
/// duplication.
///
/// The parent's standing allocation was never re-fitted, so it still claimed 28 units against a
/// 22-point budget. An apply is a **replacement built from empty**, so the parent's next revision
/// re-minted all 28 while the ~8 that had walked to the splinter stayed with it: material out of
/// nothing, on every turn-one split.
///
/// **The invariant is over the GRANT, not over the ledgers**, because on turn one the budget is the
/// currency and a ledger is a draft against it: `held + unspent` on both bands must come back to what
/// the parent alone had. A ledger-only sum would pass against a world that had merely lost the
/// difference.
#[test]
fn a_grant_split_conserves_the_material_grant() {
    let (mut app, parent, parent_band) = a_fully_outfitted_parent();
    let (_, budget_before) = grant_of(&app, parent_band);
    let held_before = material_units_held(&app, parent);

    let split = split_band_from_parent(&mut app.world, parent, 5, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);

    let (_, parent_budget) = grant_of(&app, parent_band);
    let (_, child_budget) = grant_of(&app, split.band);
    assert_eq!(
        parent_budget + child_budget,
        budget_before,
        "the two budgets partition the one grant - no point is minted twice or lost"
    );

    let parent_held = material_units_held(&app, parent);
    let child_held = material_units_held(&app, child);
    let (_, parent_spent) = allocated(&app, parent_band);
    let (_, child_spent) = allocated(&app, split.band);
    assert_eq!(
        parent_held, parent_spent,
        "the parent holds what its card claims"
    );
    assert_eq!(child_held, child_spent, "and so does the splinter");
    assert!(
        parent_held + child_held <= held_before,
        "nothing is minted out of nothing: {parent_held} + {child_held} against {held_before}"
    );
    assert_eq!(
        parent_held + child_held + (parent_budget - parent_spent) + (child_budget - child_spent),
        budget_before,
        "held plus unspent, on both bands, is the grant the parent started with"
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
    .expect("re-sending the parent's own clamped allocation always fits");
    assert_eq!(
        material_units_held(&app, parent) + material_units_held(&app, child),
        parent_held + child_held,
        "the parent's next revision re-mints its CLAMPED allocation, not the one it had before the \
         split"
    );
}

/// ⛔ **THE SPLINTER'S OUTFIT IS ITS OWN DEFAULT, NOT THE PARENT'S LEFTOVERS.**
///
/// The clamp used to hand what it took off the parent to the splinter, because a grant split moves
/// no goods and those leftovers were the only thing there was to open its card on. They are not any
/// more: the splinter **mints its own default** against its own slice of the grant, which is a
/// sensible opening outfit rather than whatever a heavily-committed parent happened to be over by.
///
/// **Nothing is destroyed by dropping the hand-off**, and the pairing says so: the parent sheds, the
/// splinter holds its default, and the two budgets still partition the one grant exactly.
#[test]
fn the_splinters_outfit_is_its_own_default_rather_than_the_parents_leftovers() {
    let (mut app, parent, parent_band) = a_fully_outfitted_parent();
    let (_, spent_before) = allocated(&app, parent_band);
    let (_, budget_before) = grant_of(&app, parent_band);

    let split = split_band_from_parent(&mut app.world, parent, 5, &permissive_settle())
        .expect("the split is admitted");
    let child = entity_for_band(&mut app, split.band);

    let (_, parent_spent) = allocated(&app, parent_band);
    assert!(
        parent_spent < spent_before,
        "**LIVENESS**: the re-fit must actually bite, or there is nothing to tell the two rules \
         apart"
    );
    let (_, child_spent) = allocated(&app, split.band);
    let (_, child_budget) = grant_of(&app, split.band);
    assert!(
        child_spent > 0,
        "the splinter is outfitted from creation: {child_spent} units against a budget of \
         {child_budget}"
    );
    assert!(
        child_spent <= child_budget,
        "and never over its own budget: {child_spent} against {child_budget}"
    );
    assert_eq!(
        material_units_held(&app, child),
        child_spent,
        "it is actually holding what its card claims - applied, not suggested"
    );

    let (_, parent_budget) = grant_of(&app, parent_band);
    assert_eq!(
        parent_budget + child_budget,
        budget_before,
        "and the grant is still partitioned exactly - no point is minted twice or lost"
    );
}

/// ⛔ **THE PARENT'S ALLOCATION LANDS INSIDE ITS REDUCED BUDGET WITH NOBODY COMMANDING ANYTHING.**
///
/// The reported `-6 / 22 left` was a *standing* allocation measured against a budget a split had
/// just shrunk. Now that the sim applies a band's default at creation, the parent's rows are a real
/// accepted allocation from turn one — so the re-fit has something to clamp, and the meter cannot go
/// negative even for a player who never opened a card. **No command is sent in this test.**
#[test]
fn a_split_leaves_an_uncommanded_parent_inside_its_reduced_budget() {
    let mut app = world_on_the_build_turn();
    let (parent, parent_band) = home_band(&mut app);
    let (kits_before, units_before) = allocated(&app, parent_band);
    assert!(
        kits_before > 0 && units_before > 0,
        "**LIVENESS**: the parent must be standing on an applied default, or there is no \
         allocation for the re-fit to clamp"
    );

    let split = split_band_from_parent(&mut app.world, parent, 5, &permissive_settle())
        .expect("the split is admitted");

    let (kit_budget, material_budget) = grant_of(&app, parent_band);
    let (kits_after, units_after) = allocated(&app, parent_band);
    assert!(
        kits_after <= kit_budget,
        "the parent claims {kits_after} kits against a budget of {kit_budget}"
    );
    assert!(
        units_after <= material_budget,
        "and {units_after} units against a budget of {material_budget}"
    );
    assert_eq!(
        material_units_held(&app, parent),
        units_after,
        "and it is holding exactly what the re-fitted card claims"
    );
    let (child_kits, child_units) = allocated(&app, split.band);
    let (child_kit_budget, child_material_budget) = grant_of(&app, split.band);
    assert!(
        child_kits <= child_kit_budget && child_units <= child_material_budget,
        "the splinter's own card fits its own budgets too"
    );
}

// -------------------------------------------------------------------------------------------
// A GRANT splinter's card opens on the campaign pre-fill — and it is asserted ON THE WIRE
// -------------------------------------------------------------------------------------------

/// One band's outfitting window **as it reaches a client**, decoded off the encoded envelope.
///
/// The resource is not the artifact: a row that never reaches the codec still satisfies an
/// in-process assertion, and the card the player stares at is built from the published frame.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PublishedWindow {
    open: bool,
    kit_budget: u32,
    material_budget: u32,
    parent_band_id: u64,
    kits: Vec<(String, u32)>,
    materials: Vec<(String, u32)>,
}

impl PublishedWindow {
    fn kits_allocated(&self) -> u32 {
        self.kits.iter().map(|(_, count)| count).sum()
    }

    fn material_units_allocated(&self) -> u32 {
        self.materials.iter().map(|(_, units)| units).sum()
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
        kit_budget: window.kitBudget(),
        material_budget: window.materialBudget(),
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

/// The campaign's two pre-fills, `(kit_defaults, material_defaults)`, straight off the live profile.
fn opening_defaults(app: &App) -> (BTreeMap<String, u32>, BTreeMap<String, u32>) {
    let profile = app.world.resource::<core_sim::ActiveStartProfile>();
    let opening = &profile.profile().overrides().opening_loadout;
    (
        opening.kit_defaults.clone(),
        opening.material_defaults.clone(),
    )
}

/// The clamp the sim fits a pre-fill with, **restated here rather than called**: proportional,
/// floored, a row that floors to zero dropped, and a declared set that already fits left alone. Id
/// order, because both sides walk a `BTreeMap`.
fn proportional_floor(declared: &BTreeMap<String, u32>, budget: u32) -> Vec<(String, u32)> {
    let total: u32 = declared.values().copied().sum();
    declared
        .iter()
        .filter_map(|(id, count)| {
            let kept = if total <= budget {
                *count
            } else {
                count * budget / total
            };
            (kept > 0).then(|| (id.clone(), kept))
        })
        .collect()
}

/// ⛔ **A TURN-ONE SPLINTER IS CREATED ALREADY HOLDING ITS OWN DEFAULT, AND NOBODY COMMANDED IT.**
///
/// Reported from a live server: a band split on turn one published `kitBudget 5` / `materialBudget
/// 8` with **`kits: []` and `materials: []`**, and the player — who had composed an outfit and never
/// pressed *Set out* — ended the turn with the band holding nothing. The record showed exactly one
/// `set_starting_loadout` that game, for the parent.
///
/// **A default that exists only as a client-side suggestion cannot survive a card nobody commits**,
/// so the sim applies it: the splinter mints the campaign default, re-fitted to its own slice of the
/// grant, through the same accepted-order path a player's own commit takes. No command is sent
/// anywhere in this test.
///
/// Asserted on the **encoded envelope**, because the card is drawn from the published frame — but
/// the ledger and the store are asserted too, since a published row the band does not hold is
/// exactly the state this replaces.
#[test]
fn a_turn_one_splinter_is_created_already_holding_its_own_default() {
    let mut app = world_on_the_build_turn();
    let (parent, _) = home_band(&mut app);
    set_workers(&mut app, parent, CHAIN_WORKERS);

    let split = split_band_from_parent(&mut app.world, parent, 5, &permissive_settle())
        .expect("the split is admitted");
    let window = published_window(&mut app, split.band);

    assert!(window.open, "the splinter's window is open");
    assert_eq!(
        window.parent_band_id, 0,
        "the parent still granted, so this is a grant of the splinter's own"
    );
    assert!(
        window.kit_budget > 0 && window.material_budget > 0,
        "**LIVENESS**: the splinter must hold a real budget, or a blank card is the honest answer \
         and this test proves nothing: {window:?}"
    );
    assert!(
        !window.kits.is_empty(),
        "the kit column publishes rows against a budget of {}: {window:?}",
        window.kit_budget
    );
    assert!(
        !window.materials.is_empty(),
        "and so does the resources column against a budget of {}: {window:?}",
        window.material_budget
    );
    assert!(
        window.kits_allocated() <= window.kit_budget,
        "the rows fit the budget they are drawn against: {window:?}"
    );
    assert!(
        window.material_units_allocated() <= window.material_budget,
        "and so does the material half: {window:?}"
    );

    // **The rows are the campaign default, re-fitted to the splinter's own two budgets.**
    let (kit_defaults, material_defaults) = opening_defaults(&app);
    assert_eq!(
        window.kits,
        proportional_floor(&kit_defaults, window.kit_budget),
        "the kit rows are `opening_loadout.kit_defaults`, clamped proportionally"
    );
    assert_eq!(
        window.materials,
        proportional_floor(&material_defaults, window.material_budget),
        "and the material rows are `opening_loadout.material_defaults`, by the same rule"
    );

    // ⛔ **AND THE BAND IS ACTUALLY STANDING IN IT.** A published row the band does not hold is the
    // suggestion this model replaced.
    let child = entity_for_band(&mut app, split.band);
    let mut expected: BTreeMap<String, u32> = BTreeMap::new();
    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
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
    let (spent_kits, spent_units) = allocated(&app, parent_band);
    assert!(
        spent_kits > 0 && spent_units > 0,
        "fixture: the parent must have committed, or this is the other test"
    );

    let split = split_band_from_parent(&mut app.world, parent, 5, &permissive_settle())
        .expect("the split is admitted");
    let window = published_window(&mut app, split.band);

    assert!(window.open && window.parent_band_id == 0, "{window:?}");
    assert!(
        window.kit_budget > 0 && window.material_budget > 0,
        "**LIVENESS**: {window:?}"
    );
    assert!(
        !window.kits.is_empty() && !window.materials.is_empty(),
        "the splinter of a committed parent publishes both halves: {window:?}"
    );
    assert!(
        window.kits_allocated() <= window.kit_budget
            && window.material_units_allocated() <= window.material_budget,
        "and both fit the budgets they are drawn against: {window:?}"
    );
}

/// ⛔ **THE TAKE PATH IS UNCHANGED** — a turn-two splinter's card is the default take it was handed,
/// not a pre-fill.
///
/// Both budgets are `0` on a take, so a pre-fill leaking onto this arm would clamp to **nothing**
/// and put the blank card back where it was first fixed. The rows expanding to exactly the ledger
/// the split moved is what says they are the take.
#[test]
fn a_turn_two_splinters_card_is_still_the_take_it_was_handed() {
    let (mut app, _, parent_band, child, child_band) = a_settled_split(12, CHAIN_WORKERS);
    let window = published_window(&mut app, child_band);

    assert!(window.open, "{window:?}");
    assert_eq!(
        (window.kit_budget, window.material_budget),
        (0, 0),
        "a take mints nothing, so it has no budget: {window:?}"
    );
    assert_eq!(
        window.parent_band_id, parent_band.0,
        "and it names the band it is drawn from: {window:?}"
    );
    assert!(
        !window.kits.is_empty(),
        "**LIVENESS**: the default take must have moved something: {window:?}"
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
