//! **The opening outfitting window** — the one source of a campaign's starting gear and material.
//!
//! Nothing at the *spawn* grants a band anything, so everything here is about the turn-one
//! allocation: what the sim applies by default, what a player's order buys, what it refuses, and
//! when it stops being available. The refusal cases each assert **nothing changed**, because a
//! loadout is one composition against one carry budget — honouring the legal half would spend the
//! player's carry on something they did not choose.

use bevy::prelude::*;

use core_sim::{
    apply_starting_loadout, build_test_app, carry_capacity, fit_to_carry, order_load, run_turn,
    BandEquipment, BandId, CarryConfig, EquipmentConfigHandle, FactionId, KitAllocation,
    LoadoutRejection, LoadoutSupply, MaterialAllocation, MaterialsConfigHandle, PopulationCohort,
    ResidentBand, Scalar, StartingLoadout, OPENING_MATERIAL_READING,
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
fn open_window() -> (App, Entity, BandId) {
    let mut app = build_test_app();
    app.world.insert_resource(EquipmentConfigHandle::default());
    app.update();
    let (band, band_id) = app
        .world
        .query_filtered::<(Entity, &BandId), With<ResidentBand>>()
        .iter(&app.world)
        .next()
        .map(|(entity, id)| (entity, *id))
        .expect("the campaign spawns a resident band");
    assert!(
        app.world.resource::<StartingLoadout>().is_open(band_id),
        "fixture: the window must be open on the world-build turn, or every case here is vacuous"
    );
    (app, band, band_id)
}

/// The spawned band's grant — its carry budget, in load units. Panics on a window that is not a
/// grant, which is the whole subject of this suite.
fn grant(app: &App, band: BandId) -> Scalar {
    match &app
        .world
        .resource::<StartingLoadout>()
        .window(band)
        .expect("the band has a window")
        .supply
    {
        LoadoutSupply::Grant { carry_budget } => *carry_budget,
        other => panic!("the spawned band's window must carry a grant, got {other:?}"),
    }
}

/// The live carry tuning an order is weighed with.
fn carry_cfg(app: &App) -> CarryConfig {
    app.world
        .resource::<core_sim::ExpeditionConfigHandle>()
        .get()
        .carry
        .clone()
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
    band_id: BandId,
    allocation: (Vec<KitAllocation>, Vec<MaterialAllocation>),
) -> LoadoutRejection {
    let before_gear = owned(app, band);
    let before_wood = held(app, band, WOOD);
    let before_bone = held(app, band, BONE);
    let before_window = app.world.resource::<StartingLoadout>().clone();
    let reason = apply_starting_loadout(
        &mut app.world,
        PLAYER,
        band_id,
        &allocation.0,
        &allocation.1,
    )
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

/// ⛔ **THE BUDGET IS WHAT THE BAND'S WORKERS CAN CARRY, DERIVED FROM THE BAND THAT SPAWNED.**
///
/// `working-age hands × carry.per_worker_carry`, and **its larder does not count** — the band has
/// not walked anywhere. It is deliberately not a config lever of its own: a dial would be a second
/// statement of how many people the band has, free to disagree with the band the moment a band size
/// or a working share is retuned. So this asserts the identity against the cohort's own workers, and
/// pins the shipped figure (17 × 8.0 = 136) beside it.
#[test]
fn the_carry_budget_is_the_starting_bands_own_workers_times_one_pack() {
    let (app, band, band_id) = open_window();
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
    let budget = grant(&app, band_id);
    assert_eq!(
        budget,
        carry_capacity(expected, &carry_cfg(&app)),
        "workers × one pack, off the band's own head count"
    );
    const SHIPPED_OPENING_HANDS: u32 = 17;
    const SHIPPED_OPENING_CARRY: u32 = 136;
    assert_eq!(
        (expected, budget),
        (
            SHIPPED_OPENING_HANDS,
            Scalar::from_u32(SHIPPED_OPENING_CARRY)
        ),
        "the shipped 30-person band fields 17 hands, and 17 × 8.0 is 136"
    );
    assert!(
        cohort.stores.get(core_sim::FOOD) > Scalar::zero(),
        "**LIVENESS**: the band holds a larder, and the budget above did not subtract it"
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
    let (mut app, band, band_id) = open_window();
    assert!(
        !owned(&app, band).is_empty(),
        "fixture: a band is created holding its applied default, so the counts below are what the \
         REPLACEMENT bought rather than an addition to nothing"
    );
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        band_id,
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
    let (mut app, band, band_id) = open_window();
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        band_id,
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
    let (mut app, _, band_id) = open_window();
    let before = grant(&app, band_id);
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        band_id,
        &kits(&[(BIG_GAME, 1)]),
        &[],
    )
    .expect("one kit is inside the budget");
    assert!(
        app.world.resource::<StartingLoadout>().is_open(band_id),
        "a commit is not a close - the turn advance is the only thing that shuts this"
    );
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        band_id,
        &kits(&[(TRAPPING, 2)]),
        &[],
    )
    .expect("a revision inside the budget is accepted, not refused as already spent");
    assert_eq!(
        grant(&app, band_id),
        before,
        "and the grant is UNCHANGED - the budget does not shrink as drafts are committed, because \
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
    let (mut app, band, band_id) = open_window();
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        band_id,
        &kits(&[(BIG_GAME, 6)]),
        &materials(&[(BONE, 20)]),
    )
    .expect("the first draft is inside the carry");
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        band_id,
        &kits(&[(BIG_GAME, 4)]),
        &materials(&[(FIBRE, 10)]),
    )
    .expect("the revision is inside the carry");

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
    let (mut app, band, band_id) = open_window();
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
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        band_id,
        &[],
        &materials(&[(BONE, 5)]),
    )
    .expect("the first draft is inside the budget");
    apply_starting_loadout(
        &mut app.world,
        PLAYER,
        band_id,
        &[],
        &materials(&[(FIBRE, 5)]),
    )
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
///
/// **What the band HOLDS survives the shut**, which is the other half: the sim applied the default
/// at creation, so a player who never touched the card still walks away outfitted. Only the
/// *unspent* remainder of the budget is forfeited.
#[test]
fn the_window_shuts_on_the_first_turn_advance_and_a_later_loadout_is_refused() {
    let (mut app, band, band_id) = open_window();
    let held = owned(&app, band);
    assert!(
        !held.is_empty(),
        "**LIVENESS**: the band must hold its applied default, or the survival claim below is the \
         old owns-nothing claim under a new name"
    );
    run_turn(&mut app);
    assert!(
        !app.world.resource::<StartingLoadout>().is_open(band_id),
        "the first turn advance shuts the window"
    );
    let reason = refused(
        &mut app,
        band,
        band_id,
        (kits(&[(BIG_GAME, 1)]), materials(&[(BONE, 1)])),
    );
    assert_eq!(reason, LoadoutRejection::WindowClosed);
    assert_eq!(
        owned(&app, band),
        held,
        "the band keeps exactly what it held when the window shut - a refused order changes \
         nothing, and the applied default is not forfeited with the unspent budget"
    );
}

#[test]
fn an_unknown_kit_is_refused() {
    let (mut app, band, band_id) = open_window();
    assert_eq!(
        refused(
            &mut app,
            band,
            band_id,
            (kits(&[("ballista", 1)]), Vec::new())
        ),
        LoadoutRejection::UnknownKit("ballista".to_string())
    );
}

/// The roster's `none` kit carries nothing, so allocating it would spend a point on air. It is
/// refused by **what makes it `none`** — an empty `uses` — rather than by its id.
#[test]
fn the_empty_kit_is_refused() {
    let (mut app, band, band_id) = open_window();
    assert_eq!(
        refused(&mut app, band, band_id, (kits(&[("none", 1)]), Vec::new())),
        LoadoutRejection::KitBuysNothing("none".to_string())
    );
}

/// `hurdles` is a real material the roster carries and the profile deliberately does not offer — so
/// this refusal is about the PICK LIST, not about the materials table.
#[test]
fn a_material_the_profile_does_not_offer_is_refused() {
    let (mut app, band, band_id) = open_window();
    assert!(
        app.world
            .resource::<MaterialsConfigHandle>()
            .get()
            .material(HURDLES)
            .is_some(),
        "fixture: the refused material must EXIST, or this tests the wrong rule"
    );
    assert_eq!(
        refused(
            &mut app,
            band,
            band_id,
            (Vec::new(), materials(&[(HURDLES, 1)]))
        ),
        LoadoutRejection::UnpickableMaterial(HURDLES.to_string())
    );
}

/// ⛔ **ONE CARRY, CHECKED ON THE WHOLE ORDER** — kits and materials are spent from the same
/// budget, so an order whose kit half fits and whose material half fits can still be refused
/// together. The kit weighs its items; a unit of material weighs its own weight.
#[test]
fn a_loadout_over_the_carry_is_refused_on_the_whole_order() {
    let (mut app, band, band_id) = open_window();
    let budget = grant(&app, band_id);
    let per_kit = order_load(
        &carry_cfg(&app),
        &[(SPEARS.to_string(), 1), (SLED.to_string(), 1)]
            .into_iter()
            .collect(),
        0,
    );
    let kits_that_fit = (budget.raw() / per_kit.raw()) as u32;
    let kit_load = order_load(
        &carry_cfg(&app),
        &[
            (SPEARS.to_string(), kits_that_fit),
            (SLED.to_string(), kits_that_fit),
        ]
        .into_iter()
        .collect(),
        0,
    );
    let material_unit = order_load(&carry_cfg(&app), &Default::default(), 1);
    let units_that_fit = ((budget - kit_load).raw() / material_unit.raw()) as u32;
    // The kits alone fit, and the material alone fits; together they are one unit over.
    let order = (
        kits(&[(BIG_GAME, kits_that_fit)]),
        materials(&[(BONE, units_that_fit + 1)]),
    );
    let reason = refused(&mut app, band, band_id, order);
    assert_eq!(
        reason,
        LoadoutRejection::OverCarry {
            load: order_load(
                &carry_cfg(&app),
                &[
                    (SPEARS.to_string(), kits_that_fit),
                    (SLED.to_string(), kits_that_fit),
                ]
                .into_iter()
                .collect(),
                units_that_fit + 1,
            ),
            capacity: budget,
        },
        "the carry is checked on the SUM across kits and materials, not per half"
    );
}

/// A repeated line is an order that contradicts itself — two rows spending one budget — so it is
/// refused rather than resolved by a summing or last-wins rule nobody stated.
#[test]
fn a_duplicate_kit_line_is_refused() {
    let (mut app, band, band_id) = open_window();
    assert_eq!(
        refused(
            &mut app,
            band,
            band_id,
            (kits(&[(BIG_GAME, 1), (BIG_GAME, 1)]), Vec::new())
        ),
        LoadoutRejection::DuplicateAllocation(BIG_GAME.to_string())
    );
}

#[test]
fn a_duplicate_material_line_is_refused() {
    let (mut app, band, band_id) = open_window();
    assert_eq!(
        refused(
            &mut app,
            band,
            band_id,
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

    let (mut app, _, band_id) = open_window();
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

    // **THE PER-BAND HALF RIDES THE COHORT.** `open` and the carry left the campaign section
    // when every band gained a window of its own — a splinter's carry is not the spawned band's,
    // so a campaign-wide reading of it could only be right for one band.
    let cohort_window = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the snapshot carries a population section")
        .iter()
        .find(|cohort| cohort.bandId() == band_id.0)
        .expect("the spawned band is published")
        .loadoutWindow()
        .expect("the spawned band carries its outfitting window");
    let budget = grant(&app, band_id);
    assert!(
        cohort_window.open(),
        "the window is open on the world-build turn"
    );
    assert_eq!(
        cohort_window.carryCapacity(),
        budget.to_f32(),
        "the published cap is the one the server refuses on"
    );
    // **And the two weights an order is measured in**, so a client weighs an order the server's way.
    assert_eq!(
        published.itemCarryWeight(),
        carry_cfg(&app).item_carry_weight
    );
    assert_eq!(
        published.materialCarryWeight(),
        carry_cfg(&app).material_carry_weight
    );
    assert_eq!(
        cohort_window.parentBandId(),
        0,
        "the spawned band's window is a GRANT, so it draws on no parent"
    );

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

    // **The kit column's rows, applied at the shipped 4/4/4 and published on the band's own window**
    // — the picker opens on a plausible band rather than a column of zeros. 20 items and 28 material
    // units against 136 of carry, so the fit does not bind here and these are the profile's numbers
    // verbatim (the fit has its own test; a splinter's binding one is asserted in
    // `split_loadout.rs`).
    let kit_rows: Vec<(String, u32)> = cohort_window
        .kits()
        .expect("the band's applied kit rows are published")
        .iter()
        .map(|entry| (entry.kitId().unwrap_or_default().to_string(), entry.count()))
        .collect();
    assert_eq!(
        kit_rows,
        vec![
            (BIG_GAME.to_string(), 4),
            (GATHERING.to_string(), 4),
            (TRAPPING.to_string(), 4),
        ]
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
/// is not; nothing lands, because a loadout is one composition against one carry.
#[test]
fn a_legal_half_beside_an_illegal_half_lands_nothing() {
    let (mut app, band, band_id) = open_window();
    let reason = refused(
        &mut app,
        band,
        band_id,
        (kits(&[(BIG_GAME, 2)]), materials(&[(HURDLES, 1)])),
    );
    assert_eq!(
        reason,
        LoadoutRejection::UnpickableMaterial(HURDLES.to_string())
    );
}

/// ⛔ **THE DEFAULT IS APPLIED, NEVER SUGGESTED — AND NOBODY HAS TO PRESS ANYTHING.**
///
/// The rule this replaces was the exact opposite: the pre-fill was a *client seed*, and a band that
/// never received a `SetStartingLoadout` owned nothing at all. That lost real gear. A player
/// composed an outfit for a band, never committed the card, ended the turn — and the band walked
/// away bare-handed, with the server's record showing no `set_starting_loadout` for it at all. **A
/// default that exists only on the client cannot survive a card nobody commits.**
///
/// So the sim commits it, at creation, through the ordinary accepted-order path: the band's ledger
/// and its material store really hold the outfit, and the window's accepted rows say so because an
/// apply sets them. No command is sent anywhere in this test.
#[test]
fn a_band_is_created_already_holding_its_default_outfit() {
    let (app, band, band_id) = open_window();
    let (kit_defaults, material_defaults) = {
        let loadout = &app
            .world
            .resource::<core_sim::ActiveStartProfile>()
            .profile()
            .overrides()
            .opening_loadout;
        (
            loadout.kit_defaults.clone(),
            loadout.material_defaults.clone(),
        )
    };
    assert!(
        !kit_defaults.is_empty() && !material_defaults.is_empty(),
        "**LIVENESS**: the shipped profile must default something, or this asserts nothing"
    );
    let equipment = app.world.resource::<EquipmentConfigHandle>().get();
    assert!(
        !fit_to_carry(
            &kit_defaults,
            &material_defaults,
            grant(&app, band_id),
            &equipment,
            &carry_cfg(&app),
        )
        .clamped,
        "fixture: the shipped defaults fit the shipped band's carry, so the fit does not bind \
         here and the quantities below are the declared ones (the fit has its own case)"
    );

    // --- what the band HOLDS, with no command sent anywhere -------------------------------------
    let ledger = owned(&app, band);
    let mut expected: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for (kit_id, count) in &kit_defaults {
        let definition = equipment
            .kit_definition(kit_id)
            .expect("a defaulted kit is on the roster");
        for item in &definition.uses {
            *expected.entry(item.clone()).or_default() += count;
        }
    }
    assert_eq!(
        ledger
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>(),
        expected,
        "the band holds exactly the expansion of `kit_defaults` - applied, not suggested"
    );
    let materials_table = app.world.resource::<MaterialsConfigHandle>().get();
    for (id, _) in materials_table.materials() {
        assert_eq!(
            held(&app, band, id),
            material_defaults.get(id).copied().unwrap_or_default() as f32,
            "'{id}' is held at exactly its declared default - and a material nothing defaults is \
             still held at none, because `start_stock` is deleted"
        );
    }

    // --- and the window's ACCEPTED ROWS say so, because an apply is what set them ----------------
    let window = app
        .world
        .resource::<StartingLoadout>()
        .window(band_id)
        .expect("the band has a window");
    assert_eq!(
        window
            .kits
            .iter()
            .map(|row| (row.kit_id.clone(), row.count))
            .collect::<std::collections::BTreeMap<_, _>>(),
        kit_defaults,
        "the card opens on the outfit the band is standing in, not on a suggestion beside it"
    );
    assert_eq!(
        window
            .materials
            .iter()
            .map(|row| (row.material_id.clone(), row.units))
            .collect::<std::collections::BTreeMap<_, _>>(),
        material_defaults,
        "and so does the material half"
    );
}

/// ⛔ **AN OVER-ALLOCATING PRE-FILL IS FITTED, PROPORTIONALLY, AND THE REMAINDER IS LEFT UNSPENT.**
///
/// The carry is the spawned band's head count × one pack, so `start_profiles.json` cannot sum-check
/// its own pre-fill and an over-allocation has to be survivable at runtime. The shipped 48-against-136
/// never binds, which is exactly why the rule needs a case that does. Materials are cut before any
/// kit, and only a kit load over the carry scales the kits.
#[test]
fn an_over_allocating_pre_fill_cuts_materials_before_tools() {
    let app = open_window().0;
    let equipment = app.world.resource::<EquipmentConfigHandle>().get();
    let mut carry_cfg = carry_cfg(&app);
    carry_cfg.item_carry_weight = 1.0;
    carry_cfg.material_carry_weight = 1.0;
    let declared_kits: std::collections::BTreeMap<String, u32> =
        [(BIG_GAME, 6u32), (TRAPPING, 3), (GATHERING, 2)]
            .into_iter()
            .map(|(id, count)| (id.to_string(), count))
            .collect();
    let declared_materials: std::collections::BTreeMap<String, u32> =
        [(BONE.to_string(), 4)].into_iter().collect();
    // 6×2 + 3×2 + 2×1 = 20 items, + 4 bone = 24 of load.

    // Inside the budget: passed through verbatim, and reported as not fitted.
    let fitted = fit_to_carry(
        &declared_kits,
        &declared_materials,
        Scalar::from_u32(24),
        &equipment,
        &carry_cfg,
    );
    assert!(!fitted.clamped);
    assert_eq!(
        fitted.kits,
        vec![
            (BIG_GAME.to_string(), 6),
            (GATHERING.to_string(), 2),
            (TRAPPING.to_string(), 3),
        ]
    );
    assert_eq!(fitted.materials, vec![(BONE.to_string(), 4)]);

    // Over the budget with room for every kit: the kits are kept WHOLE and the bone is cut into
    // what they leave — `floor(4 × 2 / 4)` = 2. Tools feed a band; bone can be gathered again.
    let fitted = fit_to_carry(
        &declared_kits,
        &declared_materials,
        Scalar::from_u32(22),
        &equipment,
        &carry_cfg,
    );
    assert!(fitted.clamped, "the fit must report that it bound");
    assert_eq!(
        fitted.kits,
        vec![
            (BIG_GAME.to_string(), 6),
            (GATHERING.to_string(), 2),
            (TRAPPING.to_string(), 3),
        ],
        "materials are cut before any tool"
    );
    assert_eq!(fitted.materials, vec![(BONE.to_string(), 2)]);

    // The kits alone over the budget: every material goes, and every kit row is
    // `floor(count × 12 / 20)` — 3 / 1 / 1. A row that floors to zero is DROPPED rather than
    // published as a pre-fill of nothing, and the floor's leftover is not handed to whichever id
    // sorts first.
    let fitted = fit_to_carry(
        &declared_kits,
        &declared_materials,
        Scalar::from_u32(12),
        &equipment,
        &carry_cfg,
    );
    assert!(fitted.clamped, "the fit must report that it bound");
    assert_eq!(
        fitted.kits,
        vec![
            (BIG_GAME.to_string(), 3),
            (GATHERING.to_string(), 1),
            (TRAPPING.to_string(), 1),
        ],
        "proportional and floored on the kits alone"
    );
    assert!(
        fitted.materials.is_empty(),
        "no material rides before a tool"
    );

    // A band with no carry pre-fills nothing, with no special case anywhere.
    let fitted = fit_to_carry(
        &declared_kits,
        &declared_materials,
        Scalar::zero(),
        &equipment,
        &carry_cfg,
    );
    assert!(fitted.clamped);
    assert!(fitted.kits.is_empty() && fitted.materials.is_empty());
}
