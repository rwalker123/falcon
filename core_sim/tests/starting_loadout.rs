//! **The opening outfitting window** — the one source of a campaign's starting gear and material.
//!
//! A spawning band owns nothing, so everything here is about the turn-one allocation: what it buys,
//! what it refuses, and when it stops being available. The refusal cases each assert **nothing
//! changed**, because a loadout is one composition against two budgets — honouring the legal half
//! would spend the player's points on something they did not choose.

use bevy::prelude::*;

use core_sim::{
    apply_starting_loadout, build_test_app, run_turn, BandEquipment, EquipmentConfigHandle,
    FactionId, KitAllocation, LoadoutRejection, MaterialAllocation, MaterialsConfigHandle,
    PopulationCohort, ResidentBand, StartingLoadout, OPENING_MATERIAL_READING,
};

/// The faction every shipped profile spawns under.
const PLAYER: FactionId = FactionId(0);

/// Two hunting kits that **share the sled**, which is what makes the adding rule visible: three of
/// each buys three spears, three traps and *six* sleds.
const BIG_GAME: &str = "big_game";
const TRAPPING: &str = "trapping";
/// The third kit the shipped profile pre-fills — Harvesting, the forage side of the opening column.
const GATHERING: &str = "gathering";
const SPEARS: &str = "spears";
const TRAPS: &str = "traps";
const SLED: &str = "sled";

/// Four materials the shipped profile offers, and one it deliberately does not.
const BONE: &str = "bone";
const FIBRE: &str = "fibre";
const HIDE: &str = "hide";
const WOOD: &str = "wood";
const HURDLES: &str = "hurdles";

/// A world one update old — Startup worldgen has run and the window has been stamped open, but no
/// turn has been advanced, so the window is still open.
///
/// **The SHIPPED equipment config, put back deliberately**: `build_test_app` installs
/// `for_a_stocked_fixture` so unrelated fixtures get bands that own gear, and this suite's whole
/// subject is the shipped opening (`equipment.md` → "A FIXTURE DECLARES THE STOCK").
fn open_window() -> (App, Entity) {
    let mut app = build_test_app();
    app.world.insert_resource(EquipmentConfigHandle::default());
    app.update();
    let band = app
        .world
        .query_filtered::<Entity, With<ResidentBand>>()
        .iter(&app.world)
        .next()
        .expect("the campaign spawns a resident band");
    assert!(
        app.world.resource::<StartingLoadout>().open,
        "fixture: the window must be open on the world-build turn, or every case here is vacuous"
    );
    (app, band)
}

/// Everything the band owns, as `(item, units)` — summed over batches, because a stock call appends
/// rather than merging.
fn owned(app: &App, band: Entity) -> Vec<(String, u32)> {
    app.world
        .get::<BandEquipment>(band)
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

fn held(app: &App, band: Entity, material: &str) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the fixture band keeps a cohort")
        .stores
        .material_total(material)
        .to_f32()
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

fn materials(pairs: &[(&str, u32)]) -> Vec<MaterialAllocation> {
    pairs
        .iter()
        .map(|(material_id, units)| MaterialAllocation {
            material_id: (*material_id).to_string(),
            units: *units,
        })
        .collect()
}

/// **A refusal changes NOTHING** — not the ledger, not the store, and not the window. Every
/// rejection case asserts through this, because "the command failed" and "the command half-landed"
/// look identical from an error return alone.
fn refused(
    app: &mut App,
    band: Entity,
    allocation: (Vec<KitAllocation>, Vec<MaterialAllocation>),
) -> LoadoutRejection {
    let before_gear = owned(app, band);
    let before_wood = held(app, band, WOOD);
    let before_bone = held(app, band, BONE);
    let before_window = *app.world.resource::<StartingLoadout>();
    let reason = apply_starting_loadout(&mut app.world, PLAYER, &allocation.0, &allocation.1)
        .expect_err("this loadout must be refused");
    assert_eq!(owned(app, band), before_gear, "a refusal grants no gear");
    assert_eq!(
        held(app, band, WOOD),
        before_wood,
        "a refusal grants no wood"
    );
    assert_eq!(
        held(app, band, BONE),
        before_bone,
        "a refusal grants no bone"
    );
    assert_eq!(
        *app.world.resource::<StartingLoadout>(),
        before_window,
        "a refusal moves neither budget - the window is untouched, exactly as a SUCCESS leaves it"
    );
    reason
}

/// ⛔ **THE BUDGET IS ONE KIT PER WORKING-AGE HAND, DERIVED FROM THE BAND THAT SPAWNED.**
///
/// It is deliberately not a config lever: a dial would be a second statement of how many people the
/// band has, free to disagree with the band the moment a band size or a working share is retuned.
/// So this asserts the identity against the cohort's own workers rather than against a literal.
#[test]
fn the_kit_budget_is_the_starting_bands_own_worker_count() {
    let (app, band) = open_window();
    let cohort = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the fixture band keeps a cohort");
    let working_fraction = app
        .world
        .resource::<core_sim::DemographicsConfigHandle>()
        .get()
        .initial_distribution
        .working;
    let expected = (cohort.size as f32 * working_fraction).floor() as u32;
    assert!(
        expected > 0,
        "**LIVENESS**: the shipped band must field somebody, or the equality below is 0 == 0"
    );
    assert_eq!(
        app.world.resource::<StartingLoadout>().kit_budget,
        expected,
        "one kit per working-age hand, off the band's own head count"
    );
    assert_eq!(
        app.world.resource::<StartingLoadout>().material_budget,
        app.world
            .resource::<core_sim::ActiveStartProfile>()
            .profile()
            .overrides()
            .opening_loadout
            .material_points,
        "the material budget is the profile's, because no head count says how much bone a band \
         walked in with"
    );
}

/// ⛔ **TWO KITS THAT SHARE AN ITEM ADD** — 6 `big_game` + 3 `trapping` is 6 spears, 3 traps and
/// **9** sleds, not 6.
///
/// An allocation buys a kit's worth of gear per hand, and two hands carrying a sled are two sleds
/// however they were bought. Every batch is stamped with the anchor grade — the band a bare-handed
/// craft of that item comes out at — because a start-stocked unit *is* an anchor-grade craft, and an
/// ungraded batch publishes a bare `×1` beside rows reading `×3 good`.
#[test]
fn allocated_kits_stock_every_item_they_use_and_shared_items_add() {
    let (mut app, band) = open_window();
    assert!(
        owned(&app, band).is_empty(),
        "fixture: a spawning band owns nothing, or the counts below are not what the loadout bought"
    );
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        &kits(&[(BIG_GAME, 6), (TRAPPING, 3)]),
        &[],
    )
    .expect("nine kits is inside the shipped band's budget");

    let ledger = owned(&app, band);
    let count = |item: &str| {
        ledger
            .iter()
            .find(|(id, _)| id == item)
            .map(|(_, units)| *units)
            .unwrap_or(0)
    };
    assert_eq!(count(SPEARS), 6, "six stalking kits carry six spears");
    assert_eq!(count(TRAPS), 3, "three trapping kits carry three traps");
    assert_eq!(
        count(SLED),
        9,
        "BOTH kits carry a sled, and the two allocations ADD - nine hands, nine sleds"
    );

    // The grade, resolved through the one seam a spawn resolves it through.
    let recipes = app.world.resource::<core_sim::RecipesConfigHandle>().get();
    let materials_table = app.world.resource::<MaterialsConfigHandle>().get();
    let expected = BandEquipment::anchor_grade(&recipes, &materials_table, SPEARS)
        .expect("the shipped book makes spears, so there is an anchor grade to claim");
    let stamped = app
        .world
        .get::<BandEquipment>(band)
        .expect("the outfitted band carries a ledger")
        .batches()
        .find(|(id, _)| *id == SPEARS)
        .and_then(|(_, batches)| batches.first().and_then(|batch| batch.grade.clone()))
        .expect("an allocated batch is stamped");
    assert_eq!(
        stamped.id, expected.id,
        "an allocated batch carries the same anchor grade a spawn would have stamped"
    );
}

/// ⛔ **MATERIALS LAND AT THE ALLOCATED UNITS AND A 0.5 READING ON EVERY DECLARED AXIS.**
///
/// One point buys one unit, and every axis reads the middle of the range: what the band scavenged
/// before setting out is unremarkable, and a spread would be a claim nothing makes.
#[test]
fn allocated_materials_land_at_their_units_and_the_opening_reading() {
    let (mut app, band) = open_window();
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        &[],
        &materials(&[(BONE, 3), (HIDE, 8), (WOOD, 19)]),
    )
    .expect("thirty units is exactly the shipped profile's budget");

    assert_eq!(held(&app, band, BONE), 3.0);
    assert_eq!(held(&app, band, HIDE), 8.0);
    assert_eq!(held(&app, band, WOOD), 19.0);

    let materials_table = app.world.resource::<MaterialsConfigHandle>().get();
    let cohort = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the fixture band keeps a cohort");
    let mut axes_checked = 0;
    for material in [BONE, HIDE, WOOD] {
        let declared = &materials_table
            .material(material)
            .expect("the shipped roster carries it")
            .characteristics;
        let batch = cohort
            .stores
            .material_batches(material)
            .next()
            .unwrap_or_else(|| panic!("'{material}' must have landed as one batch"));
        for axis in declared {
            axes_checked += 1;
            let reading = batch
                .1
                .characteristics
                .get(axis)
                .copied()
                .unwrap_or_else(|| panic!("'{material}' must state a reading on '{axis}'"));
            assert!(
                (reading - OPENING_MATERIAL_READING).abs() < 1e-6,
                "'{material}.{axis}' reads {reading}, not the opening {OPENING_MATERIAL_READING}"
            );
        }
    }
    assert!(
        axes_checked >= 6,
        "**LIVENESS**: three two-axis materials means six readings, got {axes_checked}"
    );
}

/// ⛔ **COMMITTING A LOADOUT DOES NOT SHUT THE WINDOW.**
///
/// The whole of turn one is a working surface: the player looks around the map they were just given,
/// tries a pick, and revises it. Only the turn advance closes the window, so a second apply must be
/// *accepted*, not refused as an already-spent budget.
#[test]
fn applying_a_loadout_leaves_the_window_open_for_a_revision() {
    let (mut app, _) = open_window();
    let before = *app.world.resource::<StartingLoadout>();
    apply_starting_loadout(&mut app.world, PLAYER, &kits(&[(BIG_GAME, 1)]), &[])
        .expect("one kit is inside the budget");
    assert!(
        app.world.resource::<StartingLoadout>().open,
        "a commit is not a close - the turn advance is the only thing that shuts this"
    );
    apply_starting_loadout(&mut app.world, PLAYER, &kits(&[(TRAPPING, 2)]), &[])
        .expect("a revision inside the budget is accepted, not refused as already spent");
    assert_eq!(
        *app.world.resource::<StartingLoadout>(),
        before,
        "and the window is UNCHANGED - the budgets do not shrink as drafts are committed, because \
         each apply is measured against the whole budget it replaces rather than adds to"
    );
}

/// ⛔ **AN ALLOCATION REPLACES THE LAST ONE — IT DOES NOT ADD TO IT.**
///
/// This is the invariant the open window forces: after applying `A`, the band holds exactly what `A`
/// describes, however many drafts preceded it. `6 big_game` revised to `4 big_game` leaves **four**
/// kits' worth of gear, not ten — an additive apply would refill the budget every time the player
/// changed their mind.
#[test]
fn a_revised_loadout_replaces_the_one_before_it() {
    let (mut app, band) = open_window();
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        &kits(&[(BIG_GAME, 6)]),
        &materials(&[(BONE, 20)]),
    )
    .expect("the first draft is inside both budgets");
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        &kits(&[(BIG_GAME, 4)]),
        &materials(&[(FIBRE, 10)]),
    )
    .expect("the revision is inside both budgets");

    let ledger = owned(&app, band);
    let count = |item: &str| {
        ledger
            .iter()
            .find(|(id, _)| id == item)
            .map(|(_, units)| *units)
            .unwrap_or(0)
    };
    assert_eq!(count(SPEARS), 4, "four stalking kits, not ten");
    assert_eq!(count(SLED), 4, "four stalking kits, not ten");
    assert_eq!(
        held(&app, band, FIBRE),
        10.0,
        "ten fibre, not twenty and not thirty"
    );
    assert_eq!(
        held(&app, band, BONE),
        0.0,
        "the bone the FIRST draft bought is gone - the revision did not name it, so the band does \
         not hold it"
    );
}

/// ⛔ **THE RESET IS THE MATERIAL ACCOUNT ONLY** — the band's opening food reserve shares the same
/// `LocalStore`, and clearing the store wholesale would starve it.
///
/// `LocalStore::clear_materials` is what expresses that distinction, inside the store, so the call
/// site never has to name the commodities it means to spare.
#[test]
fn a_revised_loadout_does_not_touch_the_bands_provisions() {
    let (mut app, band) = open_window();
    let provisions = |app: &App| {
        app.world
            .get::<PopulationCohort>(band)
            .expect("the fixture band keeps a cohort")
            .stores
            .get(core_sim::FOOD)
            .to_f32()
    };
    let before = provisions(&app);
    assert!(
        before > 0.0,
        "**LIVENESS**: the spawn seeds a food reserve, or this asserts 0.0 == 0.0"
    );
    apply_starting_loadout(&mut app.world, PLAYER, &[], &materials(&[(BONE, 5)]))
        .expect("the first draft is inside the budget");
    apply_starting_loadout(&mut app.world, PLAYER, &[], &materials(&[(FIBRE, 5)]))
        .expect("the revision is inside the budget");
    assert_eq!(
        provisions(&app),
        before,
        "the material reset must leave the commodity account exactly as it found it"
    );
}

/// ⛔ **THE WINDOW SHUTS ON THE FIRST TURN ADVANCE, AND UNSPENT BUDGET IS FORFEITED.**
///
/// The world-build pass does **not** shut it — that turn is what draws the map the loadout is
/// composed against.
#[test]
fn the_window_shuts_on_the_first_turn_advance_and_a_later_loadout_is_refused() {
    let (mut app, band) = open_window();
    run_turn(&mut app);
    assert!(
        !app.world.resource::<StartingLoadout>().open,
        "the first turn advance shuts the window"
    );
    let reason = refused(
        &mut app,
        band,
        (kits(&[(BIG_GAME, 1)]), materials(&[(BONE, 1)])),
    );
    assert_eq!(reason, LoadoutRejection::WindowClosed);
    assert!(
        owned(&app, band).is_empty(),
        "unspent budget grants nothing - the band that never outfitted owns nothing for ever"
    );
}

#[test]
fn an_unknown_kit_is_refused() {
    let (mut app, band) = open_window();
    assert_eq!(
        refused(&mut app, band, (kits(&[("ballista", 1)]), Vec::new())),
        LoadoutRejection::UnknownKit("ballista".to_string())
    );
}

/// The roster's `none` kit carries nothing, so allocating it would spend a point on air. It is
/// refused by **what makes it `none`** — an empty `uses` — rather than by its id.
#[test]
fn the_empty_kit_is_refused() {
    let (mut app, band) = open_window();
    assert_eq!(
        refused(&mut app, band, (kits(&[("none", 1)]), Vec::new())),
        LoadoutRejection::KitBuysNothing("none".to_string())
    );
}

/// `hurdles` is a real material the roster carries and the profile deliberately does not offer — so
/// this refusal is about the PICK LIST, not about the materials table.
#[test]
fn a_material_the_profile_does_not_offer_is_refused() {
    let (mut app, band) = open_window();
    assert!(
        app.world
            .resource::<MaterialsConfigHandle>()
            .get()
            .material(HURDLES)
            .is_some(),
        "fixture: the refused material must EXIST, or this tests the wrong rule"
    );
    assert_eq!(
        refused(&mut app, band, (Vec::new(), materials(&[(HURDLES, 1)]))),
        LoadoutRejection::UnpickableMaterial(HURDLES.to_string())
    );
}

#[test]
fn a_loadout_over_the_kit_budget_is_refused() {
    let (mut app, band) = open_window();
    let budget = app.world.resource::<StartingLoadout>().kit_budget;
    let reason = refused(
        &mut app,
        band,
        (kits(&[(BIG_GAME, budget), (TRAPPING, 1)]), Vec::new()),
    );
    assert_eq!(
        reason,
        LoadoutRejection::OverKitBudget {
            kits: budget + 1,
            budget
        },
        "the budget is checked on the SUM across kits, not per line"
    );
}

#[test]
fn a_loadout_over_the_material_budget_is_refused() {
    let (mut app, band) = open_window();
    let budget = app.world.resource::<StartingLoadout>().material_budget;
    let reason = refused(
        &mut app,
        band,
        (Vec::new(), materials(&[(BONE, budget), (HIDE, 1)])),
    );
    assert_eq!(
        reason,
        LoadoutRejection::OverMaterialBudget {
            units: budget + 1,
            budget
        },
        "the budget is checked on the SUM across materials, not per line"
    );
}

/// A repeated line is an order that contradicts itself — two rows spending one budget — so it is
/// refused rather than resolved by a summing or last-wins rule nobody stated.
#[test]
fn a_duplicate_kit_line_is_refused() {
    let (mut app, band) = open_window();
    assert_eq!(
        refused(
            &mut app,
            band,
            (kits(&[(BIG_GAME, 1), (BIG_GAME, 1)]), Vec::new())
        ),
        LoadoutRejection::DuplicateAllocation(BIG_GAME.to_string())
    );
}

#[test]
fn a_duplicate_material_line_is_refused() {
    let (mut app, band) = open_window();
    assert_eq!(
        refused(
            &mut app,
            band,
            (Vec::new(), materials(&[(BONE, 1), (BONE, 1)]))
        ),
        LoadoutRejection::DuplicateAllocation(BONE.to_string())
    );
}

/// ⛔ **THE PICKER'S ROW REACHES THE CLIENT** — asserted on the encoded FlatBuffer, because a field
/// appended behind an existing one is exactly the shape that silently fails to serialize.
///
/// **`craftableRecipeIds` is the load-bearing half.** It is what excludes the three
/// knowledge-gated bench tools (tanning frame, loom, bone awl) from the picker's *"what this
/// builds"* readout, and it is published as ids rather than left for the client to infer from a
/// craft offer's refusal sentence — which would make a player-facing string into a machine contract.
/// The kit roster and the recipe input costs are deliberately absent: both already ride the wire, in
/// `equipmentConfigJson` and `craftOffers`.
#[test]
fn the_opening_loadout_reaches_the_client() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let (mut app, _) = open_window();
    core_sim::recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<core_sim::SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let published = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .campaign()
        .expect("the envelope carries a campaign section")
        .openingLoadout()
        .expect("the campaign section carries the opening loadout");

    let window = *app.world.resource::<StartingLoadout>();
    assert!(
        published.open(),
        "the window is open on the world-build turn"
    );
    assert_eq!(published.kitBudget(), window.kit_budget);
    assert_eq!(published.materialBudget(), window.material_budget);

    let pickable: Vec<String> = published
        .pickableMaterials()
        .expect("the pick list is published")
        .iter()
        .map(|id| id.to_string())
        .collect();
    assert_eq!(
        pickable,
        app.world
            .resource::<core_sim::ActiveStartProfile>()
            .profile()
            .overrides()
            .opening_loadout
            .pickable_materials,
        "the pick list is published in the order the profile declares it"
    );

    let defaults: Vec<(String, u32)> = published
        .materialDefaults()
        .expect("the pre-fill is published")
        .iter()
        .map(|entry| {
            (
                entry.materialId().unwrap_or_default().to_string(),
                entry.units(),
            )
        })
        .collect();
    assert!(
        !defaults.is_empty() && defaults.iter().all(|(id, _)| pickable.contains(id)),
        "the pre-fill names only pickable materials: {defaults:?}"
    );

    // **The kit column's pre-fill, published at the shipped 4/4/4** — the picker opens on a
    // plausible band rather than a column of zeros. Twelve against ~17 hands, so the clamp does not
    // bind here and these are the profile's numbers verbatim (the clamp has its own test).
    let kit_defaults: Vec<(String, u32)> = published
        .kitDefaults()
        .expect("the kit pre-fill is published")
        .iter()
        .map(|entry| (entry.kitId().unwrap_or_default().to_string(), entry.count()))
        .collect();
    assert_eq!(
        kit_defaults,
        vec![
            (BIG_GAME.to_string(), 4),
            (GATHERING.to_string(), 4),
            (TRAPPING.to_string(), 4),
        ]
    );
    assert!(
        kit_defaults.iter().map(|(_, count)| count).sum::<u32>() <= published.kitBudget(),
        "a published pre-fill always fits the budget it is drawn against"
    );

    let craftable: Vec<String> = published
        .craftableRecipeIds()
        .expect("the craftable list is published")
        .iter()
        .map(|id| id.to_string())
        .collect();
    assert!(
        !craftable.is_empty(),
        "**LIVENESS**: some recipe requires no knowledge, or the exclusion below is vacuous"
    );
    let recipes = app.world.resource::<core_sim::RecipesConfigHandle>().get();
    let mut gated = 0;
    for (id, recipe) in recipes.recipes() {
        if recipe.requires_knowledge.is_empty() {
            assert!(
                craftable.iter().any(|listed| listed == id),
                "'{id}' needs no knowledge, so a fresh faction can bench it"
            );
        } else {
            gated += 1;
            assert!(
                !craftable.iter().any(|listed| listed == id),
                "'{id}' needs {:?}, which no fresh faction has learned",
                recipe.requires_knowledge
            );
        }
    }
    assert!(
        gated >= 3,
        "**LIVENESS**: the three bench tools are knowledge-gated, got {gated} gated recipes"
    );
}

/// ⛔ **A REFUSAL IS WHOLE.** The kit half of this loadout is perfectly legal and the material half
/// is not; nothing lands, because a loadout is one composition against two budgets.
#[test]
fn a_legal_half_beside_an_illegal_half_lands_nothing() {
    let (mut app, band) = open_window();
    let reason = refused(
        &mut app,
        band,
        (kits(&[(BIG_GAME, 2)]), materials(&[(HURDLES, 1)])),
    );
    assert_eq!(
        reason,
        LoadoutRejection::UnpickableMaterial(HURDLES.to_string())
    );
}

/// ⛔ **THE PRE-FILL IS A CLIENT SEED AND MUST NEVER BECOME A BACK-DOOR SPAWN STOCK.**
///
/// The shipped profile pre-fills twelve kits and twenty-eight material points, and a band that never
/// receives a `SetStartingLoadout` must still own **nothing at all** — the whole arc rests on the
/// spawn granting no gear and no material, and a default that quietly applied itself would undo that
/// while looking like a UI convenience.
#[test]
fn the_published_defaults_grant_the_band_nothing() {
    let (mut app, band) = open_window();
    let (kit_defaults, material_defaults) = {
        let loadout = &app
            .world
            .resource::<core_sim::ActiveStartProfile>()
            .profile()
            .overrides()
            .opening_loadout;
        (loadout.kit_defaults.len(), loadout.material_defaults.len())
    };
    assert!(
        kit_defaults > 0 && material_defaults > 0,
        "**LIVENESS**: the shipped profile must pre-fill something, or this asserts nothing"
    );

    // A turn passes and the window shuts with the defaults never committed.
    run_turn(&mut app);
    assert!(!app.world.resource::<StartingLoadout>().open);

    assert!(
        owned(&app, band).is_empty(),
        "a pre-filled kit column is a SUGGESTION - a band that never sent a loadout owns no gear"
    );
    let materials_table = app.world.resource::<MaterialsConfigHandle>().get();
    for (id, _) in materials_table.materials() {
        assert_eq!(
            held(&app, band, id),
            0.0,
            "'{id}' is pre-filled or pickable, and the band still holds none of it - nothing is \
             stocked at spawn and a default applies itself to nobody"
        );
    }
}

/// ⛔ **AN OVER-ALLOCATING PRE-FILL IS CLAMPED, PROPORTIONALLY, AND THE REMAINDER IS LEFT UNSPENT.**
///
/// The kit budget is the spawned band's head count, so `start_profiles.json` cannot sum-check its own
/// pre-fill and an over-allocation has to be survivable at runtime. The shipped 12-against-~17 never
/// binds, which is exactly why the rule needs a case that does.
#[test]
fn an_over_allocating_kit_pre_fill_is_clamped_proportionally() {
    let declared: std::collections::BTreeMap<String, u32> =
        [(BIG_GAME, 6u32), (TRAPPING, 3), (GATHERING, 1)]
            .into_iter()
            .map(|(id, count)| (id.to_string(), count))
            .collect();

    // Inside the budget: passed through verbatim, and reported as not clamped.
    let (rows, clamped) = core_sim::clamped_kit_defaults(&declared, 10);
    assert!(!clamped);
    assert_eq!(
        rows,
        vec![
            (BIG_GAME.to_string(), 6),
            (GATHERING.to_string(), 1),
            (TRAPPING.to_string(), 3),
        ]
    );

    // Over the budget: `floor(count × 5 / 10)` — 3 / 0 / 1, and the zero row is DROPPED rather than
    // published as a pre-fill of nothing. The floor's leftover point is deliberately not handed to
    // whichever id sorts first: a suggestion that leaves a hand free beats an arbitrary winner
    // dressed as a rule.
    let (rows, clamped) = core_sim::clamped_kit_defaults(&declared, 5);
    assert!(clamped, "the clamp must report that it bound");
    assert_eq!(
        rows,
        vec![(BIG_GAME.to_string(), 3), (TRAPPING.to_string(), 1)],
        "proportional, floored, and `gathering` floors out entirely"
    );
    assert!(
        rows.iter().map(|(_, count)| count).sum::<u32>() <= 5,
        "a clamped pre-fill never exceeds the budget it was fitted to"
    );

    // A band with no hands pre-fills nothing, with no special case anywhere.
    let (rows, clamped) = core_sim::clamped_kit_defaults(&declared, 0);
    assert!(clamped);
    assert!(rows.is_empty());
}

/// The clamp is wired into the **publish** path, not merely available beside it — asserted through
/// the live capture, with a band whose budget has been forced below what the profile pre-fills.
#[test]
fn the_published_pre_fill_is_the_clamped_one() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    const FORCED_BUDGET: u32 = 6;
    let (mut app, _) = open_window();
    app.world.resource_mut::<StartingLoadout>().kit_budget = FORCED_BUDGET;
    core_sim::recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<core_sim::SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let published = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .campaign()
        .expect("the envelope carries a campaign section")
        .openingLoadout()
        .expect("the campaign section carries the opening loadout");

    let total: u32 = published
        .kitDefaults()
        .expect("the kit pre-fill is published")
        .iter()
        .map(|entry| entry.count())
        .sum();
    assert!(
        total <= FORCED_BUDGET,
        "the shipped 4/4/4 pre-fills 12 kits; against a budget of {FORCED_BUDGET} the PUBLISHED \
         rows must already be fitted to it, and they sum to {total}"
    );
    assert!(
        total > 0,
        "**LIVENESS**: a clamp that published nothing would satisfy the bound above for the wrong \
         reason"
    );
}
