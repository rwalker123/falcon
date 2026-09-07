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
    apply_starting_loadout, build_test_app, run_turn, split_band_from_parent, BandEquipment,
    BandId, KitAllocation, LoadoutRejection, LoadoutSupply, MaterialAllocation, PopulationCohort,
    ResidentBand, Scalar, SettleConfig, StartingLoadout,
};

/// The faction every shipped profile spawns under.
const PLAYER: core_sim::FactionId = core_sim::FactionId(0);

/// The kit every take in this file is composed of, and the two items it puts in a hand. `sled` is
/// the roster's one shared item (`big_game` and `trapping` both use it), which is exactly why a kit
/// ROW cannot be capped on its own and the **expanded item list** is what the sim validates.
const BIG_GAME: &str = "big_game";
const SPEARS: &str = "spears";
const SLED: &str = "sled";

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
/// `bone_awl`, `loom` and `tanning_frame` are the only three items no kit `uses`, and the picker is
/// kit-denominated — so they can never appear in a take. That is a design statement rather than a
/// limitation: shop equipment is not a pair of hands' gear, and the alternative is an item that moves
/// invisibly and cannot be seen, adjusted or kept.
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
            "bone_awl".to_string(),
            "loom".to_string(),
            "tanning_frame".to_string()
        ],
        "the roster's un-kitted items are the three knowledge-gated bench tools; a fourth means the \
         rule needs restating rather than the list extending"
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
