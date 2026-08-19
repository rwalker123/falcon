//! **Crafting on the wire** (`docs/plan_crafting_and_materials.md` §7).
//!
//! Every assertion here reads off the **encoded envelope**, never the in-process state: the claim is
//! about what the wire carries, and a field that never reached the codec still satisfies an
//! in-process check. That is the same rule `kit_selection.rs` follows for `kitTiers`.
//!
//! The subject is one idea — **the sim resolves the refusal and the client renders it** — so every
//! test here is about a *distinction the panel has to be able to draw*, and each carries its
//! liveness half:
//!
//! | the distinction | why a boolean cannot draw it |
//! |---|---|
//! | a shortage vs a shrug | *"Not needed yet"* and *"Short 4.9 bone"* are both "you are not making one" |
//! | never made vs worn dry | both read `count 0` — a batch with no units left is removed |
//! | a band's exact reading vs its band name | two `good` hides are not interchangeable |

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::Entity;

use core_sim::{
    advance_crafting, build_test_app, recapture_snapshot_in_place, scalar_from_f32, BandBench,
    BandEquipment, BatchGrade, DiscoveryProgressLedger, EquipmentConfig, EquipmentConfigHandle,
    LadderConfigHandle, MaterialsConfig, MaterialsConfigHandle, PopulationCohort,
    RecipesConfigHandle, ResidentBand, SnapshotHistory,
};
use std::collections::BTreeMap;

const HIDE: &str = "hide";
const BONE: &str = "bone";
const TOUGHNESS: &str = "toughness";
const SUPPLENESS: &str = "suppleness";
/// The recipe every fixture puts on the bench or asks about — it reads `hide.toughness`, so a
/// deposit with a chosen reading decides its published grade.
const SLED_RECIPE: &str = "sled";
/// A **tool** recipe: gated on two crafts none of which ships known, so it is the honest subject for
/// the knowledge refusal.
const TANNING_FRAME_RECIPE: &str = "tanning_frame";
/// The two recipes the tier-head pairing runs on: one gains a second tier, the other does not.
const SPEARS_RECIPE: &str = "spears";
const CLUBS_RECIPE: &str = "clubs";
const SLED_ITEM: &str = "sled";
const SPEARS_ITEM: &str = "spears";
const TANNING_FRAME_ITEM: &str = "tanning_frame";
/// The one tier every shipped item ships, and the fixture's two metal ones.
const FLINT_TIER: &str = "flint";
const BRONZE_TIER: &str = "bronze";
const IRON_TIER: &str = "iron";

/// Enough hide that a `work: 8` sled recipe's 6-unit draw is comfortably covered.
const PLENTY: f32 = 40.0;
/// What one bare-handed pass of [`SLED_RECIPE`] withdraws, per `recipes.json` — the whole cost, at
/// the hand-working material efficiency of `1.0`.
const ONE_SLED_PASS_OF_HIDE: f32 = 6.0;
const ONE_SLED_PASS_OF_FIBRE: f32 = 2.0;
/// **Left over after the draw, and nowhere near another pass** — so the store is genuinely short
/// rather than empty, and the published `Short …` quotes a number a player would recognise.
const A_CRUMB: f32 = 0.5;
/// A crew large enough that a bench visibly progresses in one pass and small enough that it does not
/// finish the sled — `3 × 1.0 × 2.0` tooled is `6.0` against a `work` of `8.0`.
const BENCH_CREW: u32 = 3;
/// **One pass in one turn, bare-handed** — `16 × 1.0 × 0.5` is exactly the sled's `work` of `8.0`,
/// so the fixture gets a delivered batch without looping a system to watch progress accumulate.
const CREW_THAT_FINISHES_A_SLED_BARE_HANDED: u32 = 16;
/// **Nobody at the bench** — the crew half of a zero rate, named so the zero reads as a state rather
/// than as an arbitrary staffing number.
const NO_ONE_AT_THE_BENCH: u32 = 0;
/// **What a bench that cannot move publishes**: no crew, no recipe, or a craft speed of zero.
const NO_ACCRUAL: f32 = 0.0;
/// The two halves of the craft-speed pairing, spelled as words so the call site says which band it
/// is standing up.
const NO_BENCH_TOOL: bool = false;
/// See [`NO_BENCH_TOOL`].
const A_LIVE_TANNING_FRAME: bool = true;
/// **How close two readings of one fixed-point store have to be to be the same number.** A
/// [`core_sim::Scalar`] keeps six decimals and every value here rides an `f32` through the wire, so
/// this is the width of the representation rather than a tolerance for disagreement.
const SCALAR_TOLERANCE: f32 = 1e-4;

/// A world with a resident band, captured once so the fixtures have a frame to read.
fn world() -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    let band = app
        .world
        .query_filtered::<Entity, bevy::prelude::With<ResidentBand>>()
        .iter(&app.world)
        .next()
        .expect("the headless world spawns a resident band");
    (app, band)
}

/// Bank `amount` of `material` on the band at an exact per-axis reading — the same seam a take
/// credits through, so the batch merges by the store's ordinary rule.
fn deposit(app: &mut App, band: Entity, material: &str, amount: f32, axes: &[(&str, f32)]) {
    let materials = app.world.resource::<MaterialsConfigHandle>().get();
    let readings: BTreeMap<String, f32> = axes
        .iter()
        .map(|(axis, value)| ((*axis).to_string(), *value))
        .collect();
    let key = materials
        .band_key(material, &readings)
        .expect("the shipped table carries this material");
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort");
    cohort
        .stores
        .deposit_material(material, key, scalar_from_f32(amount), &readings);
}

/// Empty the band's store of one material — so a fixture can state a shortage without depending on
/// what worldgen happened to bank.
fn strip(app: &mut App, band: Entity, material: &str) {
    let axis = {
        let materials = app.world.resource::<MaterialsConfigHandle>().get();
        materials
            .material(material)
            .expect("the shipped table carries this material")
            .characteristics
            .first()
            .expect("every material declares an axis")
            .clone()
    };
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort");
    let held = cohort.stores.material_total(material);
    cohort.stores.take_material(material, &axis, held);
}

/// Put the sled on the band's bench with `workers` on it, discarding whatever was there — so a
/// fixture can vary the **crew** alone across successive publishes.
fn set_bench_crew(app: &mut App, band: Entity, workers: u32) {
    let mut bench = app
        .world
        .get_mut::<BandBench>(band)
        .expect("a spawned band carries a bench");
    bench.set_job(SLED_RECIPE, workers);
}

/// What the band's store holds of one material, summed over its batches.
fn held(app: &App, band: Entity, material: &str) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .stores
        .material_total(material)
        .to_f32()
}

/// Bank enough hide and fibre for several sled passes, at a fixed reading so the grade is the
/// fixture's rather than worldgen's.
fn stock_a_sleds_worth_of_material(app: &mut App, band: Entity) {
    strip(app, band, HIDE);
    strip(app, band, "fibre");
    deposit(
        app,
        band,
        HIDE,
        PLENTY,
        &[(TOUGHNESS, 0.9), (SUPPLENESS, 0.2)],
    );
    deposit(
        app,
        band,
        "fibre",
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
}

/// Put the sled on this band's bench with `workers` on it, discarding whatever was there.
fn set_bench(app: &mut App, band: Entity, workers: u32) {
    app.world
        .get_mut::<BandBench>(band)
        .expect("a spawned band carries a bench")
        .set_job(SLED_RECIPE, workers);
}

/// The sled recipe's input materials **in the book's own order** — asked of the loaded book rather
/// than listed here, so a retuned recipe cannot leave the ordering claim asserting a stale list.
fn sled_input_materials(app: &App) -> Vec<String> {
    app.world
        .resource::<RecipesConfigHandle>()
        .get()
        .recipe(SLED_RECIPE)
        .expect("the shipped book carries the sled")
        .inputs
        .iter()
        .map(|input| input.material.clone())
        .collect()
}

/// **Stand a sled up on a fresh world's bench, publish the rate it promises, then buy exactly one
/// turn of it and publish the progress that bought.** `(rate, progress)`.
///
/// The crew is [`BENCH_CREW`] on both halves of the pairing and neither completes a pass — a
/// completion resets `progress` to zero, which would make the comparison meaningless rather than
/// merely wrong.
fn rate_then_one_turn_of_progress(tooled: bool) -> (f32, f32) {
    let (mut app, band) = world();
    if tooled {
        app.world
            .get_mut::<BandEquipment>(band)
            .expect("a spawned band carries an equipment ledger")
            .stock(TANNING_FRAME_ITEM, 1, FLINT_TIER, None);
    }
    stock_a_sleds_worth_of_material(&mut app, band);
    set_bench(&mut app, band, BENCH_CREW);

    let promised = publish(&mut app, band).bench;
    assert_eq!(
        promised.progress, 0.0,
        "a fresh job has accrued nothing, so the turn's progress IS the delta"
    );
    app.world.run_system_once(advance_crafting);
    let accrued = publish(&mut app, band).bench;
    assert!(
        accrued.progress < accrued.work,
        "the fixture must not finish a pass — a completion resets the progress this reads"
    );
    (promised.rate_per_turn, accrued.progress)
}

/// **Wear one item until the band owns none of it** — the state a *"Worn out"* row is about. Charged
/// through `wear_item`, the one seam that retires a unit, so the retired tally is written the way
/// the game writes it.
fn wear_out(app: &mut App, band: Entity, item: &str) {
    let equipment = app.world.resource::<EquipmentConfigHandle>().get();
    let mut wear = app
        .world
        .get_mut::<BandEquipment>(band)
        .expect("a spawned band carries an equipment ledger");
    // One charge per whole unit's durability, plus a margin, until nothing is left.
    while wear.count_of(item) > 0 {
        wear.wear_item(
            &equipment,
            item,
            equipment
                .item(item)
                .expect("the fixture names a roster item")
                .headline_wear()
                .per,
            f32::MAX / 2.0,
        );
    }
}

/// **Give `spears` a bronze AND an iron tier** — the state the day metal lands, and the only way any
/// of the tier-head readout can fire at all: no shipped item ships a second tier, because an
/// unreachable one is dead content the Workbench catalogue publishes.
///
/// **Three tiers, not two, deliberately.** With only flint and bronze, *"the tier that wore out"* and
/// *"the tier below what I can now make"* are the same answer, so a two-tier fixture passes either
/// implementation and proves nothing about which one the note is reading.
fn give_spears_two_metal_tiers(app: &mut App, edit: impl FnOnce(&mut serde_json::Value)) {
    let mut json: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_EQUIPMENT_CONFIG).expect("the TOE is json");
    let tiers = json["items"][SPEARS_ITEM]["tiers"]
        .as_array_mut()
        .expect("spears declare tiers");
    tiers.push(serde_json::json!({
        "id": BRONZE_TIER,
        "starting_durability": 140.0,
        "requires_knowledge": "bone_working",
        "effects": [{ "stat": "attack", "equipped": 30.0 }]
    }));
    tiers.push(serde_json::json!({
        "id": IRON_TIER,
        "starting_durability": 180.0,
        "requires_knowledge": "weaving",
        "effects": [{ "stat": "attack", "equipped": 40.0 }]
    }));
    edit(&mut json);
    let config = EquipmentConfig::from_json_str(&json.to_string())
        .expect("extra tiers are a legal item table");
    app.world
        .resource_mut::<EquipmentConfigHandle>()
        .replace(std::sync::Arc::new(config));
}

/// **The grade a bare-handed craft of `item` comes out at**, asked of the shipped book itself.
/// Never the literal `"good"`: a test comparing against the word would pass a stamp that had been
/// hard-coded, which is exactly the thing the derived anchor exists not to be.
fn anchor_grade(app: &App, item: &str) -> String {
    let recipes = app.world.resource::<RecipesConfigHandle>().get();
    let materials = app.world.resource::<MaterialsConfigHandle>().get();
    recipes
        .anchor_grade_for_item(item, &materials)
        .expect(
            "every shipped start-stocked item is made by a recipe over a hand-workable material",
        )
        .to_string()
}

/// **Make `hide` a material nobody can work bare-handed** — the state a bench tool exists to lift,
/// and the only way `craft_speed` can fall to `0` under a job whose pile is already cut. No shipped
/// material is like this (all three organics declare `hand_working`), for the same reason no shipped
/// item ships a second tier: the ones that will be are the minerals arc's, which has no producer yet.
fn hide_cannot_be_worked_bare_handed(app: &mut App) {
    let mut json: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_MATERIALS_CONFIG).expect("the table is json");
    json["materials"][HIDE]
        .as_object_mut()
        .expect("hide is a material")
        .remove("hand_working");
    let config = MaterialsConfig::from_json_str(&json.to_string())
        .expect("a material with no bare-handed rate is a legal table");
    app.world
        .resource_mut::<MaterialsConfigHandle>()
        .replace(std::sync::Arc::new(config));
}

/// Credit this band's faction with a craft, past the ladder's completion threshold — what makes a
/// knowledge-gated tier reachable.
fn learn(app: &mut App, band: Entity, craft: &str) {
    let threshold = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .knowledge
        .completion_threshold;
    let faction = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .faction;
    let id = core_sim::crafting::craft_discovery_id(craft).expect("a shipped craft");
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(faction, id, scalar_from_f32(threshold));
}

/// Replace an item's batches with exactly `(tier, grade)` rows of one unit each — so a fixture can
/// state *which* tiers and grades a band is holding rather than depending on what a spawn stocked.
fn restock(app: &mut App, band: Entity, item: &str, batches: &[(&str, &str)]) {
    let mut wear = app
        .world
        .get_mut::<BandEquipment>(band)
        .expect("a spawned band carries an equipment ledger");
    wear.restore_batches(item, Vec::new());
    for (tier, grade) in batches {
        wear.stock(
            item,
            1,
            tier,
            Some(BatchGrade {
                id: (*grade).to_string(),
                effects: Vec::new(),
            }),
        );
    }
}

// --- reading the envelope ----------------------------------------------------------------------

/// One published axis of one batch: `(axis, exact value, band name)`. Both halves ride, which is the
/// claim [`a_batchs_exact_reading_survives_the_wire_beside_its_band_name`] is about.
type PublishedReading = (String, f32, String);
/// One published batch: `(material id, its readings in declared axis order)`.
type PublishedBatch = (String, Vec<PublishedReading>);
/// `(recipe id, amount, the axis this row is judged by — `""` on every row but one)`.
type PublishedRecipeInput = (String, f32, String);
/// `(display name, group, work, inputs)` — the static half of a recipe, off the world catalogue.
type PublishedRecipe = (String, String, f32, Vec<PublishedRecipeInput>);
/// `(id, craft, axes, hand-workable, the tool that bounds it)`.
type PublishedMaterial = (String, String, Vec<String>, bool, String);
/// The bench row as it comes off the wire. `blocked_reason` and `shortfalls` ride together because
/// their **disagreement** is the claim [`a_drawn_pile_is_not_blocked_by_a_store_too_poor_to_draw_again`]
/// makes: a drawn job publishes its shortfalls and no refusal.
#[derive(Clone, Debug, Default)]
struct PublishedBench {
    recipe_id: String,
    workers: u32,
    progress: f32,
    work: f32,
    teaches: String,
    blocked_reason: String,
    /// Whether that reason is a fault or a prompt — `danger` / `neutral` / `good`, `""` when
    /// nothing blocks. Rides beside the reason because the **pairing** of the two is the claim
    /// [`a_crewless_bench_is_a_prompt_and_a_short_bench_is_a_fault`] makes.
    blocked_severity: String,
    /// The material ids of the shortfall rows — what the **next** draw is short of.
    shortfalls: Vec<String>,
    /// What one turn at this bench will accrue, tool join and all.
    rate_per_turn: f32,
    /// `(material id, withdrawn amount)` for the pile already cut, in the recipe's input order.
    drawn_inputs: Vec<(String, f32)>,
}

/// The band's published crafting rows, decoded off the **encoded** frame.
struct Published {
    material_batches: Vec<PublishedBatch>,
    bench: PublishedBench,
    offers: BTreeMap<String, PublishedOffer>,
    equipment: Vec<PublishedItem>,
    kit_item_counts: BTreeMap<String, (f32, u32)>,
    materials: Vec<PublishedMaterial>,
    bands: Vec<(String, f32)>,
    recipes: BTreeMap<String, PublishedRecipe>,
    craft_knowledge: Vec<(String, bool, f32)>,
}

#[derive(Clone, Debug)]
struct PublishedOffer {
    display_name: String,
    group: String,
    output_item_id: String,
    available: bool,
    reason: String,
    severity: String,
    shortfalls: Vec<(String, f32, f32, f32)>,
    output_grade: String,
    on_bench: bool,
    output_tier_name: String,
    output_tier_rank: u32,
    owned_note: String,
}

#[derive(Clone, Debug)]
struct PublishedItem {
    item_id: String,
    tier_id: String,
    /// A start-stocked unit carries the **anchor** grade — it is the item's default tier, which is
    /// what an anchor-grade craft reproduces — so this is decoded and compared against
    /// [`anchor_grade`] rather than left off the struct: *"it decodes"* is half the claim this file
    /// makes about every field.
    grade: String,
    count: u32,
    remaining: f32,
    quanta_left: f32,
    quantum_noun: String,
    life: String,
    life_severity: String,
}

/// Recapture and decode. **Encoded from the ring entry's snapshot** rather than through
/// `StoredSnapshot::encode_flat`, for the reason `kit_selection.rs` states: this file asserts on
/// frame content, and `encode_flat` is a read of stored bytes carrying a stored sequence number.
fn publish(app: &mut App, band: Entity) -> Published {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

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
    let payload = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot");
    let cohort = payload
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.entity() == band.to_bits())
        .expect("the band is on the wire");
    let subsistence = payload
        .subsistence()
        .expect("the envelope carries a subsistence section");

    let material_batches = cohort
        .materialBatches()
        .expect("a cohort always publishes its batch list")
        .iter()
        .map(|batch| {
            (
                batch.materialId().unwrap_or_default().to_string(),
                batch
                    .readings()
                    .expect("a batch publishes a reading per axis")
                    .iter()
                    .map(|reading| {
                        (
                            reading.axis().unwrap_or_default().to_string(),
                            reading.value(),
                            reading.bandName().unwrap_or_default().to_string(),
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    let bench_row = cohort
        .bench()
        .expect("a cohort always publishes a bench row");
    let bench = PublishedBench {
        recipe_id: bench_row.recipeId().unwrap_or_default().to_string(),
        workers: bench_row.workers(),
        progress: bench_row.progress(),
        work: bench_row.work(),
        teaches: bench_row.teaches().unwrap_or_default().to_string(),
        blocked_reason: bench_row.blockedReason().unwrap_or_default().to_string(),
        blocked_severity: bench_row.blockedSeverity().unwrap_or_default().to_string(),
        shortfalls: bench_row
            .shortfalls()
            .map(|rows| {
                rows.iter()
                    .map(|row| row.materialId().unwrap_or_default().to_string())
                    .collect()
            })
            .unwrap_or_default(),
        rate_per_turn: bench_row.ratePerTurn(),
        drawn_inputs: bench_row
            .drawnInputs()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        (
                            row.materialId().unwrap_or_default().to_string(),
                            row.amount(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    let offers = cohort
        .craftOffers()
        .expect("a cohort publishes one row per recipe, always")
        .iter()
        .map(|offer| {
            (
                offer.recipeId().unwrap_or_default().to_string(),
                PublishedOffer {
                    display_name: offer.displayName().unwrap_or_default().to_string(),
                    group: offer.group().unwrap_or_default().to_string(),
                    output_item_id: offer.outputItemId().unwrap_or_default().to_string(),
                    available: offer.available(),
                    reason: offer.reason().unwrap_or_default().to_string(),
                    severity: offer.severity().unwrap_or_default().to_string(),
                    shortfalls: offer
                        .shortfalls()
                        .map(|rows| {
                            rows.iter()
                                .map(|row| {
                                    (
                                        row.materialId().unwrap_or_default().to_string(),
                                        row.required(),
                                        row.held(),
                                        row.short(),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    output_grade: offer.outputGrade().unwrap_or_default().to_string(),
                    on_bench: offer.onBench(),
                    output_tier_name: offer.outputTierName().unwrap_or_default().to_string(),
                    output_tier_rank: offer.outputTierRank(),
                    owned_note: offer.ownedNote().unwrap_or_default().to_string(),
                },
            )
        })
        .collect();
    let equipment = cohort
        .equipmentBatches()
        .expect("a cohort publishes an equipment row per item")
        .iter()
        .map(|batch| PublishedItem {
            item_id: batch.itemId().unwrap_or_default().to_string(),
            tier_id: batch.tierId().unwrap_or_default().to_string(),
            grade: batch.grade().unwrap_or_default().to_string(),
            count: batch.count(),
            remaining: batch.remaining(),
            quanta_left: batch.quantaLeft(),
            quantum_noun: batch.quantumNoun().unwrap_or_default().to_string(),
            life: batch.life().unwrap_or_default().to_string(),
            life_severity: batch.lifeSeverity().unwrap_or_default().to_string(),
        })
        .collect();
    let kit_item_counts = cohort
        .kitItemConditions()
        .expect("a cohort publishes one condition row per config item")
        .iter()
        .map(|row| {
            (
                row.itemId().unwrap_or_default().to_string(),
                (row.remaining(), row.count()),
            )
        })
        .collect();
    let materials = subsistence
        .materials()
        .expect("the world publishes its materials catalogue")
        .iter()
        .map(|material| {
            (
                material.id().unwrap_or_default().to_string(),
                material.craft().unwrap_or_default().to_string(),
                material
                    .axes()
                    .map(|axes| axes.iter().map(|axis| axis.to_string()).collect())
                    .unwrap_or_default(),
                material.handWorkable(),
                material.toolItemId().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let bands = subsistence
        .characteristicBands()
        .expect("the world publishes the rating vocabulary")
        .iter()
        .map(|band| (band.name().unwrap_or_default().to_string(), band.from()))
        .collect();
    let recipes = subsistence
        .recipes()
        .expect("the world publishes its recipe book")
        .iter()
        .map(|recipe| {
            (
                recipe.id().unwrap_or_default().to_string(),
                (
                    recipe.displayName().unwrap_or_default().to_string(),
                    recipe.group().unwrap_or_default().to_string(),
                    recipe.work(),
                    recipe
                        .inputs()
                        .map(|rows| {
                            rows.iter()
                                .map(|row| {
                                    (
                                        row.materialId().unwrap_or_default().to_string(),
                                        row.amount(),
                                        row.readsAxis().unwrap_or_default().to_string(),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                ),
            )
        })
        .collect();
    let craft_knowledge = subsistence
        .craftKnowledge()
        .expect("the world publishes a craft-knowledge row per faction per craft")
        .iter()
        .map(|row| {
            (
                row.craftId().unwrap_or_default().to_string(),
                row.known(),
                row.progress(),
            )
        })
        .collect();

    Published {
        material_batches,
        bench,
        offers,
        equipment,
        kit_item_counts,
        materials,
        bands,
        recipes,
        craft_knowledge,
    }
}

fn offer<'a>(published: &'a Published, recipe: &str) -> &'a PublishedOffer {
    published
        .offers
        .get(recipe)
        .unwrap_or_else(|| panic!("every recipe gets a row, including '{recipe}'"))
}

fn rows_for<'a>(published: &'a Published, item: &str) -> Vec<&'a PublishedItem> {
    published
        .equipment
        .iter()
        .filter(|row| row.item_id == item)
        .collect()
}

// --- the tests ----------------------------------------------------------------------------------

/// **A shortage publishes its NUMBER, and a band with plenty publishes availability.**
///
/// The pairing is the test. *"the row is unavailable"* passes on a wire that refuses everything, and
/// *"the row is available"* passes on one that refuses nothing — so both arms of the same recipe run
/// against each other in one fixture.
#[test]
fn a_shortage_publishes_the_number_and_a_band_with_plenty_publishes_availability() {
    let (mut app, band) = world();
    strip(&mut app, band, HIDE);
    let short = publish(&mut app, band);
    let short_row = offer(&short, SLED_RECIPE);

    assert!(
        !short_row.available,
        "a band with no hide cannot make a sled"
    );
    assert_eq!(
        short_row.severity, "danger",
        "a shortage is a problem, and the severity is what says so"
    );
    assert!(
        short_row.reason.starts_with("Short ") && short_row.reason.contains(HIDE),
        "a refusal names its NUMBER — got {:?}",
        short_row.reason
    );
    let (material, required, held, missing) = short_row
        .shortfalls
        .iter()
        .find(|(material, ..)| material == HIDE)
        .cloned()
        .expect("a refused row publishes the shortfall it is refusing on");
    assert_eq!(material, HIDE);
    assert!(
        missing > 0.0 && (required - held - missing).abs() < 1e-3,
        "the published shortfall is required − held: {required} − {held} ≠ {missing}"
    );

    // THE LIVENESS HALF: bank the hide and the very same row must flip.
    deposit(
        &mut app,
        band,
        HIDE,
        PLENTY,
        &[(TOUGHNESS, 0.9), (SUPPLENESS, 0.2)],
    );
    deposit(
        &mut app,
        band,
        "fibre",
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    let stocked = publish(&mut app, band);
    let stocked_row = offer(&stocked, SLED_RECIPE);
    assert!(
        stocked_row.available,
        "a band holding {PLENTY} hide can make a sled — got {:?}",
        stocked_row.reason
    );
    assert!(
        stocked_row.shortfalls.is_empty(),
        "an available row publishes no shortfall"
    );
    assert_ne!(
        stocked_row.severity, "danger",
        "a buildable row is not a problem"
    );
}

/// **"Not needed yet" and a shortage are DIFFERENT STRINGS on the same wire**, and different
/// severities — one is a shrug and the other is a problem. A client deriving both from a boolean
/// cannot tell them apart, which is the whole reason the reason is published.
#[test]
fn not_needed_yet_and_a_shortage_are_different_strings_and_severities() {
    let (mut app, band) = world();
    // Stock everything, so the only thing that can refuse a row is knowledge or need.
    deposit(
        &mut app,
        band,
        HIDE,
        PLENTY,
        &[(TOUGHNESS, 0.9), (SUPPLENESS, 0.2)],
    );
    deposit(
        &mut app,
        band,
        "fibre",
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    deposit(
        &mut app,
        band,
        BONE,
        PLENTY,
        &[("density", 0.5), ("length", 0.5)],
    );
    let plentiful = publish(&mut app, band);
    // A spawned band is start-stocked and has spent nothing, so its sled is untouched.
    let shrug = offer(&plentiful, SLED_RECIPE);
    assert_eq!(
        shrug.reason, "Not needed yet",
        "an untouched item is a shrug, not a shortage"
    );
    assert_eq!(shrug.severity, "neutral");
    assert!(
        shrug.available,
        "\"not needed yet\" is a row you COULD make — the refusal vocabulary is not the availability flag"
    );

    // THE OTHER HALF, on the same frame: a row the band genuinely cannot make reads differently.
    let (mut short_app, short_band) = world();
    strip(&mut short_app, short_band, HIDE);
    let short = publish(&mut short_app, short_band);
    let problem = offer(&short, SLED_RECIPE);
    assert_ne!(
        problem.reason, shrug.reason,
        "a shortage must not read as a shrug"
    );
    assert_ne!(
        problem.severity, shrug.severity,
        "a shortage must not be styled as a shrug — this is the distinction a boolean loses"
    );
}

/// **A refusal on knowledge names the CRAFT, hyphenated and capitalized.** None of the three crafts
/// ships known, so a tool recipe is refused on knowledge from turn one — and the same row's
/// availability is the liveness half: an ordinary kit recipe beside it is *not* knowledge-gated.
#[test]
fn a_knowledge_refusal_names_the_craft_and_an_ungated_recipe_beside_it_does_not() {
    let (mut app, band) = world();
    deposit(
        &mut app,
        band,
        HIDE,
        PLENTY,
        &[(TOUGHNESS, 0.9), (SUPPLENESS, 0.2)],
    );
    deposit(
        &mut app,
        band,
        "fibre",
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    deposit(
        &mut app,
        band,
        BONE,
        PLENTY,
        &[("density", 0.5), ("length", 0.5)],
    );
    let published = publish(&mut app, band);

    let tool = offer(&published, TANNING_FRAME_RECIPE);
    assert!(
        !tool.available,
        "no craft ships known, so no tool is buildable"
    );
    assert_eq!(tool.severity, "danger");
    assert!(
        tool.reason.contains("Needs Weaving") && tool.reason.contains("Needs Bone-working"),
        "a tool is gated on the crafts of what it is MADE FROM, and both are named — got {:?}",
        tool.reason
    );
    assert!(
        tool.reason.contains(" · "),
        "two reasons at once are joined with ' · ' — got {:?}",
        tool.reason
    );
    assert_eq!(
        tool.group, "tool",
        "a recipe whose output bounds a material is a tool row"
    );

    // LIVENESS: the same wire, the same frame, an ungated recipe is offered.
    assert!(
        offer(&published, SLED_RECIPE).available,
        "**tools are earned, never a prerequisite** — a sled is craftable bare-handed on turn one"
    );
    assert_eq!(offer(&published, SLED_RECIPE).group, "kit");
}

/// **An item the band has NEVER OWNED is distinguishable from one worn dry** — both read `count 0`,
/// because a batch that runs out of units is removed from the ledger.
#[test]
fn a_never_made_item_is_distinguishable_from_one_worn_dry() {
    let (mut app, band) = world();
    let before = publish(&mut app, band);

    // A bench tool is start-stocked by nothing, so the band has never had one.
    let never = rows_for(&before, TANNING_FRAME_ITEM);
    assert_eq!(
        never.len(),
        1,
        "an unowned item still gets exactly one ledger row"
    );
    assert_eq!(never[0].count, 0);
    assert_eq!(never[0].life, "Never made");

    // LIVENESS, on the same frame: a start-stocked item reads as owned and untouched, so "count 0"
    // is not simply what every row says.
    let owned = rows_for(&before, SPEARS_ITEM);
    assert_eq!(owned.len(), 1);
    assert!(
        owned[0].count > 0,
        "a spawn stocks a PARTY'S worth of every kit item — the count is the band's head count \
         times `start_stock_fraction`, so what this half asserts is ownership, not a literal 1"
    );
    assert_eq!(owned[0].life, "Untouched");
    assert_eq!(owned[0].life_severity, "healthy");

    // Now wear that same item out, and its row must say something else again.
    wear_out(&mut app, band, SPEARS_ITEM);
    let after = publish(&mut app, band);
    let dry = rows_for(&after, SPEARS_ITEM);
    assert_eq!(dry.len(), 1);
    assert_eq!(
        dry[0].count, 0,
        "a worn-out item leaves the band owning none"
    );
    assert_eq!(
        dry[0].life, "Worn out",
        "the band HAD one and it broke — a different sentence from never having had one"
    );
    assert_ne!(
        dry[0].life, never[0].life,
        "this is the whole point: two states that both read `count 0` must not read the same"
    );
    assert_eq!(dry[0].life_severity, "danger");

    // And the ownership statement rides `kitItemConditions` too, so no client infers ownership from
    // a condition of zero.
    let (remaining, count) = before.kit_item_counts[SPEARS_ITEM];
    assert!(remaining > 0.0 && count > 0, "owned and with life left");
    let (dry_remaining, dry_count) = after.kit_item_counts[SPEARS_ITEM];
    assert_eq!(
        (dry_remaining, dry_count),
        (0.0, 0),
        "`remaining == 0` means OWNS NONE, and `count` is what says so out loud"
    );
}

/// **The life wording is in USE QUANTA, never percent**, and the quantum's noun comes from the
/// item's own `wear.per` — resolved sim-side, because a client must not map quanta to English.
#[test]
fn the_life_wording_is_in_the_items_own_use_quanta_and_the_noun_comes_from_its_quantum() {
    let (mut app, band) = world();
    // Charge one use of each, so no row is `Untouched` and every one has to state a count.
    {
        let equipment = app.world.resource::<EquipmentConfigHandle>().get();
        let mut wear = app
            .world
            .get_mut::<BandEquipment>(band)
            .expect("a spawned band carries an equipment ledger");
        for item in [SPEARS_ITEM, "clubs", SLED_ITEM] {
            wear.wear_item(
                &equipment,
                item,
                equipment
                    .item(item)
                    .expect("the fixture names a roster item")
                    .headline_wear()
                    .per,
                1.0,
            );
        }
    }
    let published = publish(&mut app, band);

    let spears = rows_for(&published, SPEARS_ITEM);
    let clubs = rows_for(&published, "clubs");
    let sled = rows_for(&published, SLED_ITEM);
    assert_eq!(
        spears[0].quantum_noun, "blows",
        "spears wear per landed strike"
    );
    assert_eq!(
        clubs[0].quantum_noun, "blows",
        "and so does a club — a weapon is charged for what it SWINGS, whichever role swings it, \
         which is why `Kill` and `Fight` collapsed into one quantum"
    );
    assert_eq!(
        sled[0].quantum_noun, "biomass hauled",
        "the sled is not swung, so it keeps its own quantum"
    );
    assert_ne!(
        spears[0].quantum_noun, sled[0].quantum_noun,
        "the noun is the ITEM's, not one word for every row — that is what the client must not \
         re-derive"
    );
    for row in [&spears[0], &clubs[0], &sled[0]] {
        assert!(
            row.life.ends_with(&format!(" {} left", row.quantum_noun)) || row.life.starts_with('~'),
            "a worn row reads in its own quanta — got {:?}",
            row.life
        );
        assert!(
            !row.life.contains('%'),
            "the life meter is a fuel gauge, never a percentage — got {:?}",
            row.life
        );
        assert!(
            row.quanta_left > 0.0,
            "a row with life left publishes the number the wording counts"
        );
    }

    // The two are on the same durability scale but different quanta, so they must NOT publish the
    // same count — the liveness half, since "both say 'N left'" passes on a table of one number.
    assert_ne!(
        spears[0].quanta_left.round(),
        clubs[0].quanta_left.round(),
        "a spear's 250 kills and a club's 50 raids are the same durability at different quanta"
    );
    // And `remaining` is still the 0–100 condition, which is deliberately NOT what `life` reads in.
    assert!(spears[0].remaining > 0.0 && spears[0].remaining < 100.0);
}

/// **A batch's EXACT reading survives the round trip alongside its band name.** Bands are the merge
/// key and the panel's word; the exact value is what crafting reads, so two `good` hides are not
/// interchangeable and a wire carrying only the band could not say why.
#[test]
fn a_batchs_exact_reading_survives_the_wire_beside_its_band_name() {
    let (mut app, band) = world();
    strip(&mut app, band, HIDE);
    // Two hides that land in DIFFERENT bands on the read axis, so the batch map keeps them apart.
    deposit(
        &mut app,
        band,
        HIDE,
        10.0,
        &[(TOUGHNESS, 0.92), (SUPPLENESS, 0.10)],
    );
    deposit(
        &mut app,
        band,
        HIDE,
        10.0,
        &[(TOUGHNESS, 0.14), (SUPPLENESS, 0.92)],
    );
    let published = publish(&mut app, band);

    let hides: Vec<_> = published
        .material_batches
        .iter()
        .filter(|(material, _)| material == HIDE)
        .collect();
    assert_eq!(
        hides.len(),
        2,
        "same material, DIFFERENT per-axis band ⇒ two batches"
    );
    let readings: Vec<(f32, String)> = hides
        .iter()
        .map(|(_, axes)| {
            let (_, value, band_name) = axes
                .iter()
                .find(|(axis, _, _)| axis == TOUGHNESS)
                .expect("a hide publishes its toughness");
            (*value, band_name.clone())
        })
        .collect();
    assert!(
        readings
            .iter()
            .any(|(value, name)| (*value - 0.92).abs() < 1e-4 && name == "excellent"),
        "the exact 0.92 rides beside the word 'excellent' — got {readings:?}"
    );
    assert!(
        readings
            .iter()
            .any(|(value, name)| (*value - 0.14).abs() < 1e-4 && name == "poor"),
        "the exact 0.14 rides beside the word 'poor' — got {readings:?}"
    );
    // LIVENESS: the axis ORDER is the material's declared one, not the map's.
    let (_, axes) = hides[0];
    assert_eq!(
        axes.iter()
            .map(|(axis, _, _)| axis.as_str())
            .collect::<Vec<_>>(),
        vec![TOUGHNESS, SUPPLENESS],
        "a hide's axes ride in its declared order, which is what a batch's readings are keyed by"
    );

    // **The grade the draw WOULD select** comes off the same worst-first order the bench spends in,
    // so the poor hide is what the next sled is made of.
    deposit(
        &mut app,
        band,
        "fibre",
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    let with_fibre = publish(&mut app, band);
    assert_eq!(
        offer(&with_fibre, SLED_RECIPE).output_grade,
        "poor",
        "worst-first: the 0.14 hide is spent before the 0.92 one, so the next sled is poor - and the \
         grade is the BAND word, the same one the reading above published"
    );
}

/// **The bench publishes what is being made, against what work, and why it is stopped** — and an
/// idle bench is a different state from a blocked one.
#[test]
fn the_bench_publishes_its_job_its_work_and_its_refusal() {
    let (mut app, band) = world();
    let idle = publish(&mut app, band);
    assert_eq!(idle.bench.recipe_id, "", "an idle bench names no recipe");
    assert_eq!(
        idle.bench.blocked_reason, "",
        "an idle bench is not blocked — it is idle"
    );

    // Put a job on with a crew, and strip the material out from under it.
    strip(&mut app, band, HIDE);
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("a spawned band carries a bench");
        bench.set_job(SLED_RECIPE, 3);
    }
    let blocked = publish(&mut app, band);
    let PublishedBench {
        recipe_id,
        workers,
        work,
        teaches,
        blocked_reason,
        ..
    } = blocked.bench.clone();
    assert_eq!(recipe_id, SLED_RECIPE);
    assert_eq!(workers, 3);
    assert!(work > 0.0, "the bench states the work one pass costs");
    assert_eq!(
        teaches, "tanning",
        "crafting is the fourth teacher, and the lesson is the RECIPE's craft"
    );
    assert!(
        blocked_reason.starts_with("Short ") && blocked_reason.contains(HIDE),
        "a stopped bench says why, in the same vocabulary the offers use — got {blocked_reason:?}"
    );
    assert!(
        offer(&blocked, SLED_RECIPE).on_bench,
        "the running row is marked so its button can be spent"
    );

    // LIVENESS: with the material back, the same bench publishes no refusal.
    deposit(
        &mut app,
        band,
        HIDE,
        PLENTY,
        &[(TOUGHNESS, 0.9), (SUPPLENESS, 0.2)],
    );
    deposit(
        &mut app,
        band,
        "fibre",
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    let running = publish(&mut app, band);
    assert_eq!(
        running.bench.blocked_reason, "",
        "a bench with its pile and its crew is not blocked"
    );
}

/// **A bench with a full pile and NOBODY on it is stopped too** — a crew refusal, which is not a
/// craft-offer question: the offer answers *"could this be made"*, not *"is anyone making it"*.
#[test]
fn a_crewless_bench_publishes_its_own_refusal_while_the_offer_stays_available() {
    let (mut app, band) = world();
    deposit(
        &mut app,
        band,
        HIDE,
        PLENTY,
        &[(TOUGHNESS, 0.9), (SUPPLENESS, 0.2)],
    );
    deposit(
        &mut app,
        band,
        "fibre",
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("a spawned band carries a bench");
        bench.set_job(SLED_RECIPE, 0);
    }
    let published = publish(&mut app, band);
    assert_eq!(
        published.bench.blocked_reason, "No one at the bench",
        "the bench states the crew's refusal even though the craft itself is fine"
    );
    assert!(
        offer(&published, SLED_RECIPE).available,
        "the OFFER is still available — staffing is the player's next move, not a refusal"
    );
}

/// **A bench waiting for its crew is a PROMPT; a bench short of material is a FAULT** — and the
/// wire says which, so a client cannot paint the expected state in the alarm colour.
///
/// The player staffs the bench and the sim never does, so *"No one at the bench"* is the normal
/// state one click after **Make**. Asserted as a **pairing** over one band and one recipe, because
/// a wire that stamped a single severity on every blocked bench passes neither half — the crewless
/// bench must read `neutral` while the *same* bench, stripped of its hide, reads `danger`. The
/// joined case rides along: a reason with a fault in it is a fault.
#[test]
fn a_crewless_bench_is_a_prompt_and_a_short_bench_is_a_fault() {
    let (mut app, band) = world();
    deposit(
        &mut app,
        band,
        HIDE,
        PLENTY,
        &[(TOUGHNESS, 0.9), (SUPPLENESS, 0.2)],
    );
    deposit(
        &mut app,
        band,
        "fibre",
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );

    // LIVENESS: a stocked bench with a crew is not blocked at all, so it states no severity either.
    set_bench_crew(&mut app, band, BENCH_CREW);
    let working = publish(&mut app, band);
    assert_eq!(working.bench.blocked_reason, "");
    assert_eq!(
        working.bench.blocked_severity, "",
        "nothing is blocking, so there is no severity to state — the two empties go together"
    );

    // THE PROMPT: the pile is there and nobody is on it, which is what a staged Make looks like.
    set_bench_crew(&mut app, band, 0);
    let crewless = publish(&mut app, band);
    assert_eq!(crewless.bench.blocked_reason, "No one at the bench");
    assert_eq!(
        crewless.bench.blocked_severity, "neutral",
        "the player is being told what to do next, not what went wrong"
    );

    // THE FAULT: the same bench with a crew and no hide.
    strip(&mut app, band, HIDE);
    set_bench_crew(&mut app, band, BENCH_CREW);
    let short = publish(&mut app, band);
    assert!(
        short.bench.blocked_reason.starts_with("Short ")
            && short.bench.blocked_reason.contains(HIDE),
        "the fixture's other half must actually be a shortage — got {:?}",
        short.bench.blocked_reason
    );
    assert_eq!(
        short.bench.blocked_severity, "danger",
        "a shortage is a problem, and it must not read the same as a bench awaiting its crew"
    );

    // AND BOTH AT ONCE takes the alarm: a reason with a fault in it is a fault.
    set_bench_crew(&mut app, band, 0);
    let both = publish(&mut app, band);
    assert!(
        both.bench.blocked_reason.contains("No one at the bench")
            && both.bench.blocked_reason.starts_with("Short "),
        "the joined case must carry both halves — got {:?}",
        both.bench.blocked_reason
    );
    assert_eq!(
        both.bench.blocked_severity, "danger",
        "joined reasons take danger if any component is"
    );
}

/// **A DRAWN pile's materials are already in hand, so a shortage in the store cannot stop it** — the
/// shortage is about the NEXT draw, and the offer row for that same recipe is where it belongs.
///
/// Asserted as a **pairing** on one store, because a one-sided check passes on a bench that never
/// reports anything: the drawn bench is silent while the offer beside it still says `Short …`, and
/// the *same* store with the pile put back on the shelf says it on the bench.
#[test]
fn a_drawn_pile_is_not_blocked_by_a_store_too_poor_to_draw_again() {
    let (mut app, band) = world();
    strip(&mut app, band, HIDE);
    strip(&mut app, band, "fibre");
    // One pass and a crumb: the draw takes the whole cost and leaves a store that cannot fund
    // another, which is exactly the state the defect published as a running job's refusal.
    deposit(
        &mut app,
        band,
        HIDE,
        ONE_SLED_PASS_OF_HIDE + A_CRUMB,
        &[(TOUGHNESS, 0.9), (SUPPLENESS, 0.2)],
    );
    deposit(
        &mut app,
        band,
        "fibre",
        ONE_SLED_PASS_OF_FIBRE + A_CRUMB,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("a spawned band carries a bench");
        bench.set_job(SLED_RECIPE, BENCH_CREW);
    }
    app.world.run_system_once(advance_crafting);

    let drawn = publish(&mut app, band);
    assert!(
        app.world
            .get::<BandBench>(band)
            .expect("a spawned band carries a bench")
            .drawn
            .is_some(),
        "the fixture's whole subject is a bench whose pile is already cut"
    );
    assert!(
        drawn.bench.progress > 0.0,
        "the job is progressing — that is what makes a shortage the wrong thing to say about it"
    );
    assert_eq!(
        drawn.bench.blocked_reason, "",
        "a drawn pile is in HAND: what the store is short of cannot stop the item in flight"
    );
    let drawn_offer = offer(&drawn, SLED_RECIPE).clone();
    assert!(
        drawn_offer.reason.starts_with("Short "),
        "the shortage did not vanish — the offer row answers *could I start another* — got {:?}",
        drawn_offer.reason
    );
    assert!(
        !drawn.bench.shortfalls.is_empty(),
        "the bench keeps publishing its shortfall rows: honest data about the next draw, which the \
         client does not render as blocking"
    );

    // THE PAIRING: put the pile back on the shelf without touching the store — `set_job` clears the
    // drawn pile and the progress with it — and the identical store now blocks the bench.
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("a spawned band carries a bench");
        bench.set_job(SLED_RECIPE, BENCH_CREW);
    }
    let undrawn = publish(&mut app, band);
    assert_eq!(
        undrawn.material_batches, drawn.material_batches,
        "the two halves of the pairing read the SAME store — otherwise the bench rows differ for a \
         reason that is not the draw"
    );
    assert!(
        undrawn.bench.blocked_reason.starts_with("Short ")
            && undrawn.bench.blocked_reason.contains(HIDE),
        "an undrawn bench is exactly what the field is for: a shortage is why it has not drawn — \
         got {:?}",
        undrawn.bench.blocked_reason
    );
    assert_eq!(
        offer(&undrawn, SLED_RECIPE).reason,
        drawn_offer.reason,
        "the offer says the same thing either way — the fact has one home and it is that row"
    );
}

/// **A start-stocked unit IS an anchor-grade craft, and the ledger says so** — an unstamped batch
/// published a bare `×1` beside rows reading `×3 good`, which a player cannot tell from a panel that
/// failed to draw something.
///
/// Asserted as a **pairing**, because *"the row has a grade"* is satisfied by any stamp at all: the
/// band's spawned sled and a sled its own bench makes off hide richer than the bare hand can reach
/// carry the **same** grade — which is the claim, rather than merely that a grade is present.
#[test]
fn a_start_stocked_batch_carries_the_grade_a_bare_handed_craft_of_it_comes_out_at() {
    let (mut app, band) = world();
    let anchor = anchor_grade(&app, SLED_ITEM);
    assert!(
        !anchor.is_empty(),
        "the shipped book resolves an anchor for the sled — without one the pairing below is vacuous"
    );
    let spawned = publish(&mut app, band);
    let spawned_rows = rows_for(&spawned, SLED_ITEM);
    assert_eq!(
        spawned_rows.len(),
        1,
        "a spawn stocks exactly one batch of the item, which is the row under test"
    );
    assert_eq!(
        spawned_rows[0].grade, anchor,
        "a spawned unit is the item's default tier, and `validate` ties the anchor grade to that \
         same tier — so it performs as an anchor-grade craft and now says which"
    );

    // THE PAIRING: the same band's own bench, off hide far richer than the bare hand can reach, so
    // the ceiling — not the pile — decides the grade, exactly as the anchor is defined.
    deposit(
        &mut app,
        band,
        HIDE,
        PLENTY,
        &[(TOUGHNESS, 0.95), (SUPPLENESS, 0.2)],
    );
    deposit(
        &mut app,
        band,
        "fibre",
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("a spawned band carries a bench");
        bench.set_job(SLED_RECIPE, CREW_THAT_FINISHES_A_SLED_BARE_HANDED);
    }
    app.world.run_system_once(advance_crafting);

    let crafted = publish(&mut app, band);
    let crafted_rows = rows_for(&crafted, SLED_ITEM);
    assert_eq!(
        crafted_rows.len(),
        2,
        "the bench delivered a SECOND batch — *the next ten are their own batch* — so there are two \
         rows to compare"
    );
    let grades: Vec<&str> = crafted_rows.iter().map(|row| row.grade.as_str()).collect();
    assert_eq!(
        grades,
        vec![anchor.as_str(), anchor.as_str()],
        "the spawned unit and the bare-handed craft beside it are the same grade, which is what \
         makes the stamp a statement of fact rather than a display default"
    );
}

/// **A tool that runs dry mid-craft DOES stop a drawn job**, and it is the one refusal that has to
/// survive the rule above: `craft_speed` is `0`, so progress genuinely cannot accrue, and a dead
/// bench reading as healthy is the failure that costs the player turns.
#[test]
fn a_drawn_job_whose_tool_ran_dry_still_publishes_its_refusal() {
    let (mut app, band) = world();
    hide_cannot_be_worked_bare_handed(&mut app);
    app.world
        .get_mut::<BandEquipment>(band)
        .expect("a spawned band carries an equipment ledger")
        .stock(TANNING_FRAME_ITEM, 1, FLINT_TIER, None);
    deposit(
        &mut app,
        band,
        HIDE,
        PLENTY,
        &[(TOUGHNESS, 0.9), (SUPPLENESS, 0.2)],
    );
    deposit(
        &mut app,
        band,
        "fibre",
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("a spawned band carries a bench");
        bench.set_job(SLED_RECIPE, BENCH_CREW);
    }
    app.world.run_system_once(advance_crafting);

    // LIVENESS: with the frame alive the same drawn job publishes nothing, so the assertion below
    // is about the tool rather than about a bench that says something whatever happens.
    let working = publish(&mut app, band);
    assert_eq!(
        working.bench.blocked_reason, "",
        "a drawn job on a live tool is not blocked"
    );

    wear_out(&mut app, band, TANNING_FRAME_ITEM);
    let stalled = publish(&mut app, band);
    assert!(
        app.world
            .get::<BandBench>(band)
            .expect("a spawned band carries a bench")
            .drawn
            .is_some(),
        "the pile is still cut — this is a running job, not a fresh one"
    );
    assert_eq!(
        stalled.bench.blocked_reason, "No tanning frame",
        "a zero craft rate is the real *a drawn job is stopped* case, and it names the tool to build"
    );
}

/// **The rate is the accrual the sim applies, and the tool join is inside it.**
///
/// Asserted as a **pairing over the two craft speeds** — bare-handed and with a live tanning frame,
/// the same recipe and the same crew — because the confusion this field exists to end is exactly a
/// player reading `workers` as the rate: bare-handed organics work at `hand_working.rate 0.5`, so
/// three crafters deliver one and a half worker-turns, not three. A wire that published the crew
/// alone would carry the *same* number for both halves and match neither band's progress.
#[test]
fn the_published_rate_is_the_turn_of_progress_it_promises_bare_handed_and_tooled() {
    let (bare_rate, bare_progress) = rate_then_one_turn_of_progress(NO_BENCH_TOOL);
    let (tooled_rate, tooled_progress) = rate_then_one_turn_of_progress(A_LIVE_TANNING_FRAME);

    assert!(
        (bare_progress - bare_rate).abs() < SCALAR_TOLERANCE,
        "a bare-handed bench accrued {bare_progress} against a published rate of {bare_rate}"
    );
    assert!(
        (tooled_progress - tooled_rate).abs() < SCALAR_TOLERANCE,
        "a tooled bench accrued {tooled_progress} against a published rate of {tooled_rate}"
    );
    assert!(
        tooled_rate > bare_rate + SCALAR_TOLERANCE,
        "the two publish DIFFERENT rates ({tooled_rate} tooled, {bare_rate} bare-handed) — the tool \
         join is the whole reason a client cannot re-derive this"
    );
    assert!(
        bare_rate < BENCH_CREW as f32 - SCALAR_TOLERANCE,
        "the reported confusion, pinned: a worker-turn is NOT a worker's turn, so {BENCH_CREW} \
         crafters bare-handed deliver {bare_rate} and not {BENCH_CREW}"
    );
}

/// **A bench that cannot accrue publishes a zero, not a missing field** — and each zero is paired
/// with the same bench lifted off it, so *"it always reports something"* cannot pass.
#[test]
fn a_bench_that_cannot_accrue_publishes_a_zero_rate() {
    let (mut app, band) = world();
    assert_eq!(
        publish(&mut app, band).bench.rate_per_turn,
        NO_ACCRUAL,
        "an idle bench has no recipe to accrue toward"
    );
    stock_a_sleds_worth_of_material(&mut app, band);

    // No crew: the pile is there and the craft is fine, and nobody is standing at it.
    set_bench(&mut app, band, NO_ONE_AT_THE_BENCH);
    assert_eq!(
        publish(&mut app, band).bench.rate_per_turn,
        NO_ACCRUAL,
        "no crew is no accrual"
    );
    // LIVENESS: the identical bench and the identical store, with hands on it.
    set_bench(&mut app, band, BENCH_CREW);
    assert!(
        publish(&mut app, band).bench.rate_per_turn > NO_ACCRUAL,
        "a crewed bench over the same store publishes a real rate"
    );

    // No way to work the material: hide cannot be worked bare-handed and the band owns no frame.
    let (mut app, band) = world();
    hide_cannot_be_worked_bare_handed(&mut app);
    stock_a_sleds_worth_of_material(&mut app, band);
    set_bench(&mut app, band, BENCH_CREW);
    let handless = publish(&mut app, band);
    assert_eq!(
        handless.bench.rate_per_turn, NO_ACCRUAL,
        "a material with no bare-handed rate and no tool is the zero that IS the refusal"
    );
    assert_eq!(
        handless.bench.blocked_reason, "No tanning frame",
        "and the zero is published beside the words that say what to build"
    );
    // LIVENESS: one frame lifts the same bench off zero.
    app.world
        .get_mut::<BandEquipment>(band)
        .expect("a spawned band carries an equipment ledger")
        .stock(TANNING_FRAME_ITEM, 1, FLINT_TIER, None);
    assert!(
        publish(&mut app, band).bench.rate_per_turn > NO_ACCRUAL,
        "the tool is what the zero was about"
    );
}

/// **The drawn pile names what the store really lost**, which is neither the recipe's stated inputs
/// nor the shortfall's `required`: the fixture runs on a **tooled** bench, whose
/// `craft_material_efficiency` sits between the book's cost and the withdrawal.
///
/// Paired with the same bench **before** its draw, publishing an empty list — a one-sided assertion
/// passes on a wire that reports the recipe's inputs whether or not anything was cut.
#[test]
fn the_published_drawn_pile_is_what_the_store_actually_lost() {
    let (mut app, band) = world();
    app.world
        .get_mut::<BandEquipment>(band)
        .expect("a spawned band carries an equipment ledger")
        .stock(TANNING_FRAME_ITEM, 1, FLINT_TIER, None);
    stock_a_sleds_worth_of_material(&mut app, band);
    set_bench(&mut app, band, BENCH_CREW);

    let undrawn = publish(&mut app, band);
    assert!(
        undrawn.bench.drawn_inputs.is_empty(),
        "nothing has been cut yet, and an empty list is the honest answer — got {:?}",
        undrawn.bench.drawn_inputs
    );

    let inputs = sled_input_materials(&app);
    let before: Vec<f32> = inputs.iter().map(|m| held(&app, band, m)).collect();
    app.world.run_system_once(advance_crafting);
    let after: Vec<f32> = inputs.iter().map(|m| held(&app, band, m)).collect();
    let drawn = publish(&mut app, band);

    assert_eq!(
        drawn
            .bench
            .drawn_inputs
            .iter()
            .map(|(material, _)| material.clone())
            .collect::<Vec<_>>(),
        inputs,
        "one row per input material, in the recipe's own input order"
    );
    for (index, (material, published)) in drawn.bench.drawn_inputs.iter().enumerate() {
        let lost = before[index] - after[index];
        assert!(
            (published - lost).abs() < SCALAR_TOLERANCE,
            "the {material} row publishes {published} against a store that lost {lost}"
        );
    }
    let (_, hide_drawn) = drawn
        .bench
        .drawn_inputs
        .iter()
        .find(|(material, _)| material == HIDE)
        .expect("the sled is made of hide");
    assert!(
        (hide_drawn - ONE_SLED_PASS_OF_HIDE).abs() > SCALAR_TOLERANCE,
        "the tool's material efficiency sits between the book's {ONE_SLED_PASS_OF_HIDE} and the \
         {hide_drawn} the store really lost — a client reading `recipes.json` would name the wrong \
         number"
    );
}

/// How far a scaled material row may drift from `materialPerBiomass × factor` — **relative**,
/// because the rung rows are a rate multiplied by a pen's whole MSY biomass and an absolute epsilon
/// tuned for a per-biomass rate would be meaningless three orders of magnitude up.
const MATERIAL_SCALE_TOLERANCE: f32 = 1e-3;

/// One published material vector, decoded to `(id, amount)` in wire order. Takes the vector's own
/// iterator so the caller never has to name a FlatBuffers lifetime.
fn material_rows<'a>(
    published: impl Iterator<
        Item = shadow_scale_flatbuffers::generated::shadow_scale::sim::MaterialPayoff<'a>,
    >,
) -> Vec<(String, f32)> {
    published
        .map(|row| {
            (
                row.materialId().unwrap_or_default().to_string(),
                row.amount(),
            )
        })
        .collect()
}

/// **The ONE biomass factor a published vector is `materialPerBiomass` scaled by** — derived from
/// the first row and then required to reproduce *every* row, which is what makes this a statement
/// about one conversion of one harvest rather than a per-row coincidence. Every material vector on
/// these two tables is the source's own species rows through `material_yield_totals`, so they differ
/// only in the biomass term handed to it.
///
/// `None` for an empty vector — the wire's "no row", never a zero.
fn one_biomass_scaling(
    per_biomass: &[(String, f32)],
    scaled: &[(String, f32)],
    what: &str,
) -> Option<f32> {
    if scaled.is_empty() {
        return None;
    }
    assert_eq!(
        scaled.len(),
        per_biomass.len(),
        "{what}: a scaled row set is the SAME species vector — one entry per material, no more"
    );
    let factor = scaled[0].1 / per_biomass[0].1;
    for (index, (id, amount)) in scaled.iter().enumerate() {
        assert_eq!(
            *id, per_biomass[index].0,
            "{what}: the rows are the source's own vector, merged and in material-id order"
        );
        let want = per_biomass[index].1 * factor;
        assert!(
            (amount - want).abs() <= MATERIAL_SCALE_TOLERANCE * want.abs().max(1.0),
            "{what}: {id} publishes {amount} against {want} — one biomass term scales the whole \
             vector or the row is describing a different harvest than its siblings"
        );
        assert!(
            *amount > 0.0,
            "{what}: a published rate is a rate that pays"
        );
    }
    assert!(
        factor > 0.0,
        "{what}: the biomass a vector is priced on is positive, or there is no row to publish"
    );
    Some(factor)
}

/// **The three per-world catalogues round-trip**, and the rating vocabulary rides once rather than
/// per material.
/// **EVERY SOURCE'S MATERIAL RATE REACHES THE WIRE, on the shipped map** (arc #527).
///
/// Six per-source vectors — a herd's `materialPerBiomass` and a patch's, **both per-worker twins**,
/// and the herd's two investment rungs (`corralMaterial` / `pastoralMaterial`) — are each computed
/// correctly *and then written onto a row*, and the second half is the one nothing else catches: a
/// rate derived right and published nowhere looks exactly like the retired trade field's absence,
/// which is the shape this arc has now been asked to fix three times.
///
/// Asserted on the **decoded FlatBuffers** over the real headless world, as a *relation* against the
/// sim's own seams rather than a recorded number, so a rate retune moves both sides at once. The
/// per-biomass rate is checked against `material_yield_totals`; every other vector is checked as
/// **that rate scaled by one biomass term**, and the term is then pinned to a number the same row
/// already publishes:
///
/// - **`perWorkerMaterial` is `materialPerBiomass × perWorkerBiomass`**, which is the assertion a
///   codec that dropped the field fails (an empty vector beside a live rate), and also the one a
///   codec that aliased it onto `materialPerBiomass` fails (the factor would be `1`, not a hunter's
///   ~20 biomass of carry). It is the field the compose sheet's clamp reads, so its silent loss
///   would zero every material row on the sheet with the whole suite green.
/// - **`corralMaterial` / `pastoralMaterial` are the same vector at the two rungs' MSY biomass**,
///   and each rung's factor `×` the row's own `provisionsPerBiomass` must reproduce its food
///   sibling (`corralYield` / `pastoralYield`) — the `.fbs`'s claim that a rung's two readouts
///   describe *one* harvest. A rung priced off a second `sustainable_yield` call, or the two slots
///   swapped, fails it; the pen breeds at `r × 4` against the pastoral rung's `r × 2`, so the
///   ordering assertion catches the swap even where the food tie cannot (an inedible quarry).
///
/// The liveness half is the closing block: the shipped map must publish at least one non-empty
/// vector of **each** kind, or every comparison above was between empty vectors.
#[test]
fn every_sources_material_rate_reaches_the_wire() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let (mut app, _band) = world();
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
    let subsistence = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .subsistence()
        .expect("a subsistence section");

    let fauna = app.world.resource::<core_sim::FaunaConfigHandle>().get();
    let mut live_herd_rate = false;
    let mut live_herd_per_worker = false;
    let mut live_rung_material = false;
    let mut saw_the_rung_ladder = false;
    for herd in subsistence.herds().expect("herds present").iter() {
        let species = herd.species().unwrap_or_default();
        let expected = core_sim::material_yield_totals(fauna.hunt_materials_for(species), 1.0, 1.0);
        let published = herd
            .materialPerBiomass()
            .expect("the key is always written, empty or not");
        assert_eq!(
            published.len(),
            expected.len(),
            "{species}: the row must carry the seam's own material rows"
        );
        for (index, want) in expected.iter().enumerate() {
            let row = published.get(index);
            assert_eq!(row.materialId().unwrap_or_default(), want.material);
            assert!((row.amount() - want.amount).abs() < 1e-6);
            assert!(row.amount() > 0.0, "a published rate is a rate that pays");
        }
        live_herd_rate |= !published.is_empty();

        let per_biomass = material_rows(published.iter());
        let per_worker = material_rows(
            herd.perWorkerMaterial()
                .expect("the key is always written, empty or not")
                .iter(),
        );
        let corral = material_rows(
            herd.corralMaterial()
                .expect("the key is always written, empty or not")
                .iter(),
        );
        let pastoral = material_rows(
            herd.pastoralMaterial()
                .expect("the key is always written, empty or not")
                .iter(),
        );
        if per_biomass.is_empty() {
            // A species nothing is made of publishes no row on ANY of the four — "no row" is one
            // answer, not one per field.
            assert!(
                per_worker.is_empty() && corral.is_empty() && pastoral.is_empty(),
                "{species}: a herd made of nothing cannot pay a material at any rung"
            );
            continue;
        }

        // **The per-worker twin — the field the compose sheet clamps with.** A hunter's carry is the
        // row's own `perWorkerBiomass`, so the vector is exactly the per-biomass rate through it.
        let carry = herd.perWorkerBiomass();
        assert!(
            carry > 0.0,
            "{species}: a herd quotes the EQUIPPED haul rate, which is never zero"
        );
        let per_worker_factor = one_biomass_scaling(
            &per_biomass,
            &per_worker,
            &format!("{species}: perWorkerMaterial"),
        )
        .unwrap_or_else(|| {
            panic!(
                "{species}: a herd that pays a material per biomass pays one per hunter too — an \
                 empty perWorkerMaterial beside a live rate is the field silently not reaching the \
                 wire"
            )
        });
        assert!(
            (per_worker_factor - carry).abs() <= MATERIAL_SCALE_TOLERANCE * carry,
            "{species}: perWorkerMaterial is priced on {per_worker_factor} biomass against the \
             perWorkerBiomass of {carry} published beside it"
        );
        live_herd_per_worker = true;

        // **The two investment rungs, each priced on its own MSY biomass** — and each tied to its
        // food sibling, which is the whole of the `.fbs`'s "a rung's two readouts describe one
        // harvest" claim. Both are empty on a herd that offers neither rung (an already-penned one
        // reports no *projection*), which is a real answer rather than a gap.
        let corral_factor =
            one_biomass_scaling(&per_biomass, &corral, &format!("{species}: corralMaterial"));
        let pastoral_factor = one_biomass_scaling(
            &per_biomass,
            &pastoral,
            &format!("{species}: pastoralMaterial"),
        );
        let food_rate = herd.provisionsPerBiomass();
        for (factor, food, rung) in [
            (corral_factor, herd.corralYield(), "corral"),
            (pastoral_factor, herd.pastoralYield(), "pastoral"),
        ] {
            let Some(factor) = factor else { continue };
            live_rung_material = true;
            if food_rate <= 0.0 {
                // An inedible quarry's food sibling is honestly `0`, so there is nothing to tie to
                // — which is exactly why the material rung exists. The ordering check below is what
                // covers it.
                continue;
            }
            let want = factor * food_rate;
            assert!(
                (food - want).abs() <= MATERIAL_SCALE_TOLERANCE * want.abs().max(1.0),
                "{species}: the {rung} rung's material is priced on {factor} biomass, which pays \
                 {want} food against the {food} its own food row publishes — the two readouts must \
                 be one harvest"
            );
        }
        if let (Some(corral_factor), Some(pastoral_factor)) = (corral_factor, pastoral_factor) {
            assert!(
                corral_factor >= pastoral_factor,
                "{species}: a pen breeds at r×4 against the pastoral rung's r×2, so its harvest \
                 cannot be the smaller one — the two slots are swapped"
            );
            saw_the_rung_ladder |= corral_factor > pastoral_factor;
        }
    }

    let mut live_patch_rate = false;
    let mut live_patch_per_worker = false;
    for patch in subsistence.foragePatches().expect("patches present").iter() {
        let published = patch
            .materialPerBiomass()
            .expect("the key is always written, empty or not");
        // A patch's rows are a decomposition of its own basket, so the relation this can state
        // cheaply is the one that matters: every published row is positive and named once.
        let mut seen: Vec<&str> = Vec::new();
        for index in 0..published.len() {
            let row = published.get(index);
            let id = row.materialId().unwrap_or_default();
            assert!(row.amount() > 0.0, "a published rate is a rate that pays");
            assert!(
                !seen.contains(&id),
                "a patch must MERGE two species giving {id} into one rate, not publish it twice"
            );
            seen.push(id);
        }
        live_patch_rate |= !published.is_empty();

        // **The gatherer's twin, with the tile's seasonal weight already folded in** — so the factor
        // is the patch's own `perWorkerBiomass`, and a **dead season** publishes no row at all
        // rather than a column of zeros.
        let per_biomass = material_rows(published.iter());
        let per_worker = material_rows(
            patch
                .perWorkerMaterial()
                .expect("the key is always written, empty or not")
                .iter(),
        );
        let carry = patch.perWorkerBiomass();
        if per_biomass.is_empty() || carry <= 0.0 {
            assert!(
                per_worker.is_empty(),
                "a patch that pays nothing, or one a dead season stops a gatherer working, is \
                 EMPTY — never a published zero"
            );
            continue;
        }
        let factor = one_biomass_scaling(&per_biomass, &per_worker, "patch: perWorkerMaterial")
            .expect("a live patch rate with a working season pays a gatherer too");
        assert!(
            (factor - carry).abs() <= MATERIAL_SCALE_TOLERANCE * carry,
            "patch: perWorkerMaterial is priced on {factor} biomass against the perWorkerBiomass of \
             {carry} published beside it"
        );
        live_patch_per_worker = true;
    }

    assert!(
        live_herd_rate,
        "the shipped map must publish at least one herd material rate, or the herd loop compared \
         empty vectors"
    );
    assert!(
        live_patch_rate,
        "…and at least one patch material rate, which is the rung-1 gap this closed"
    );
    assert!(
        live_herd_per_worker && live_patch_per_worker,
        "…and at least one per-worker twin on EACH web, or the field the compose sheet clamps with \
         was never compared to anything"
    );
    assert!(
        live_rung_material,
        "…and at least one investment rung's material payoff, or a herd could publish none at all \
         and pass"
    );
    assert!(
        saw_the_rung_ladder,
        "…on at least one herd the pen must out-yield the pastoral rung, or the ordering assertion \
         above is comparing two copies of one number"
    );
}

#[test]
fn the_per_world_catalogues_round_trip() {
    let (mut app, band) = world();
    let published = publish(&mut app, band);

    // --- materials --------------------------------------------------------------------------
    let hide = published
        .materials
        .iter()
        .find(|(id, ..)| id == HIDE)
        .expect("the shipped catalogue carries hide");
    assert_eq!(
        hide.1, "tanning",
        "a material names the craft that works it"
    );
    assert_eq!(
        hide.2,
        vec![TOUGHNESS.to_string(), SUPPLENESS.to_string()],
        "the axes ride in declared order"
    );
    assert!(
        hide.3,
        "hide is hand-workable — that is what makes turn one playable"
    );
    assert_eq!(
        hide.4, "tanning_frame",
        "the material names the tool that bounds it, which is what the 'No loom' refusal reads"
    );
    // **Six ship: the three organics and the three uncrafted luxury crops** (arc #527). An
    // *unreachable* material would still be dead content the catalogue publishes; these three are
    // *uncrafted* — a plant grows them, a band banks them, and no bench works them yet — so their
    // published `craft` is the empty string, which is what tells a client there is nothing to make.
    assert_eq!(published.materials.len(), 6);
    let uncrafted: Vec<&str> = published
        .materials
        .iter()
        .filter(|(_, craft, ..)| craft.is_empty())
        .map(|(id, ..)| id.as_str())
        .collect();
    assert_eq!(
        uncrafted,
        vec!["grape", "tea", "tobacco"],
        "exactly the three luxury crops publish no craft"
    );
    for (id, craft, _, hand_workable, tool) in &published.materials {
        if !craft.is_empty() {
            continue;
        }
        assert!(
            !hand_workable && tool.is_empty(),
            "{id}: a material nothing works has no bench of any kind — not a bare hand, not a tool"
        );
    }

    // --- bands -------------------------------------------------------------------------------
    assert_eq!(
        published
            .bands
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["poor", "fair", "good", "excellent"],
        "the shared rating vocabulary, once for the world"
    );
    assert_eq!(published.bands[0].1, 0.0, "the first band opens at 0.0");

    // --- recipes -----------------------------------------------------------------------------
    let (display_name, group, work, inputs) = published
        .recipes
        .get(SLED_RECIPE)
        .expect("the shipped book carries the sled")
        .clone();
    assert_eq!(display_name, "Sled");
    assert_eq!(group, "kit");
    assert!(work > 0.0);
    let read_rows: Vec<_> = inputs
        .iter()
        .filter(|(_, _, axis)| !axis.is_empty())
        .collect();
    assert_eq!(
        read_rows.len(),
        1,
        "exactly one input carries the axis the recipe judges — one reading answers one question"
    );
    assert_eq!(read_rows[0].0, HIDE);
    assert_eq!(read_rows[0].2, TOUGHNESS);
    assert_eq!(
        published.recipes.len(),
        published.offers.len(),
        "the catalogue and the per-band offers cover the same book, row for row"
    );

    // --- craft knowledge ---------------------------------------------------------------------
    assert_eq!(
        published.craft_knowledge.len(),
        3,
        "one row per craft the materials table declares"
    );
    assert!(
        published.craft_knowledge.iter().all(|(_, known, _)| !known),
        "**none ships known** — a band learns Tanning by tanning"
    );
    assert!(
        published
            .craft_knowledge
            .iter()
            .any(|(craft, ..)| craft == "bone_working"),
        "the third organic craft is on the wire beside the other two"
    );
}

/// **THE GROUP HEAD SAYS WHAT A ROW WOULD BE MADE AT; THE NOTE SAYS WHAT THE BAND HAS — AND THE NOTE
/// IS PUBLISHED ONLY WHEN THE TWO DISAGREE.**
///
/// Exercised by a **two-tier fixture**, because every shipped item ships one tier and none of this
/// can fire on the shipped roster — the same treatment
/// `a_tier_switches_an_items_attack_without_touching_its_shared_effects` gets, and the reason
/// `ownedNote` is `""` on every shipped row.
///
/// Pinned as a **pairing**: the upgraded row against an un-upgraded one beside it on the same frame.
/// Asserting one row's wording alone would pass on a wire that said the same thing everywhere.
#[test]
fn the_owned_note_is_published_only_when_the_band_carries_something_older() {
    let (mut app, band) = world();
    give_spears_two_metal_tiers(&mut app, |_| {});
    learn(&mut app, band, "bone_working");
    learn(&mut app, band, "weaving");
    // Two flint batches at different grades — the note must name the WORST, because naming the best
    // is the one the player would be told about last.
    restock(
        &mut app,
        band,
        SPEARS_ITEM,
        &[(FLINT_TIER, "good"), (FLINT_TIER, "poor")],
    );
    let published = publish(&mut app, band);

    let upgraded = offer(&published, SPEARS_RECIPE);
    assert_eq!(
        (
            upgraded.output_tier_name.as_str(),
            upgraded.output_tier_rank
        ),
        (IRON_TIER, 2),
        "the head is what a craft would be made at NOW, and heads order by rank descending"
    );
    assert_eq!(
        upgraded.owned_note, "carrying flint · poor",
        "the band holds an older tier, so the cell says so - and it names the worst grade it holds"
    );

    // THE PAIRING, on the same frame: an item with nothing newer to be made at says nothing.
    let current = offer(&published, CLUBS_RECIPE);
    assert_eq!(
        (current.output_tier_name.as_str(), current.output_tier_rank),
        (FLINT_TIER, 0),
        "clubs gained no tier, so their head is still the one that ships known"
    );
    assert_eq!(
        current.owned_note, "",
        "there is nothing older in hand, so there is no news - and \"\" is what the whole shipped \
         roster publishes"
    );
    assert_ne!(
        upgraded.owned_note, current.owned_note,
        "this is the whole point: a wire that emitted the same note everywhere cannot pass"
    );

    // **WORN OUT NAMES THE TIER THAT ACTUALLY WORE OUT.** The band loses its FLINT spears while a
    // bronze tier sits between them and the iron it could now make — so *"the tier below craftable"*
    // would say **bronze**, a set this band never owned. Only a three-tier fixture can tell the two
    // rules apart; at two tiers they agree, so a two-tier fixture proves nothing here.
    wear_out(&mut app, band, SPEARS_ITEM);
    let after = publish(&mut app, band);
    let dry = offer(&after, SPEARS_RECIPE);
    assert_eq!(
        dry.owned_note, "last flint set wore out",
        "the note names the tier `wear_item` retired, never the neighbour of what could be made"
    );
    assert!(
        !dry.owned_note.contains(BRONZE_TIER),
        "bronze sits between flint and iron and this band never held one - naming it would be a \
         published string asserting the wrong tier"
    );
    assert_ne!(
        dry.owned_note, upgraded.owned_note,
        "\"we still have the old ones\" and \"the old ones broke\" are not the same sentence"
    );
}

/// **A FIRST TOOL ADVERTISES THE BAND ITS OWN CEILING REACHES, not the top of the ladder.**
///
/// The shipped tools all declare `craft_quality_ceiling 0.90`, which lands in the top band — so a
/// fixture at `0.90` passes both *"quote the top band"* and *"quote this tool's band"* and proves
/// nothing. This one drops the tanning frame's ceiling into `good`, where the two answers differ:
/// advertising `excellent` there is the panel promising a grade the bench cannot produce.
#[test]
fn a_first_tools_invitation_names_the_band_its_own_quality_ceiling_reaches() {
    // A ceiling inside `good` (0.55..0.80) and clear of `excellent` (0.80).
    const MODEST_CEILING: f32 = 0.70;

    let (mut app, band) = world();
    give_spears_two_metal_tiers(&mut app, |json| {
        let effects = json["items"][TANNING_FRAME_ITEM]["tiers"][0]["effects"]
            .as_array_mut()
            .expect("the frame's tier declares craft stats");
        for effect in effects.iter_mut() {
            if effect["stat"] == serde_json::json!("craft_quality_ceiling") {
                effect["equipped"] = serde_json::json!(MODEST_CEILING);
            }
        }
    });
    // The frame is gated on Weaving and Bone-working and needs its pile — an invitation is only ever
    // published on a buildable row.
    learn(&mut app, band, "weaving");
    learn(&mut app, band, "bone_working");
    deposit(
        &mut app,
        band,
        "fibre",
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    deposit(
        &mut app,
        band,
        BONE,
        PLENTY,
        &[("density", 0.5), ("length", 0.5)],
    );
    let published = publish(&mut app, band);

    let tool = offer(&published, TANNING_FRAME_RECIPE);
    assert!(
        tool.available,
        "the crafts are known and the pile is there - got {:?}",
        tool.reason
    );
    assert_eq!(
        tool.reason, "Unlocks good hide work",
        "a 0.70 ceiling falls in `good`, and that is the grade this frame will actually produce"
    );
    assert!(
        !tool.reason.contains("excellent"),
        "the top band must never be quoted for a tool that does not reach it - that is the panel \
         promising a grade the bench cannot make"
    );

    // LIVENESS: `excellent` is a word this wire really can say, so "good" is not simply what every
    // string says. The band's own hide reads excellent on the rail beside the frame's modest offer.
    deposit(
        &mut app,
        band,
        HIDE,
        PLENTY,
        &[(TOUGHNESS, 0.95), (SUPPLENESS, 0.2)],
    );
    let stocked = publish(&mut app, band);
    assert!(
        stocked
            .material_batches
            .iter()
            .filter(|(material, _)| material == HIDE)
            .any(|(_, axes)| axes
                .iter()
                .any(|(axis, _, band_name)| axis == TOUGHNESS && band_name == "excellent")),
        "the vocabulary really does carry `excellent` on this frame"
    );
}

/// **The offer row names the item it makes**, which is the join a ledger row is drawn from: the
/// offer supplies the name and the rebuild cost, the equipment batch supplies the tier and the life.
#[test]
fn an_offer_names_the_item_it_makes_so_the_ledger_can_join_the_two_halves() {
    let (mut app, band) = world();
    let published = publish(&mut app, band);
    let sled = offer(&published, SLED_RECIPE);
    assert_eq!(sled.output_item_id, SLED_ITEM);
    assert_eq!(sled.display_name, "Sled");
    let anchor = anchor_grade(&app, SLED_ITEM);
    assert!(
        rows_for(&published, &sled.output_item_id)
            .iter()
            .any(|row| row.tier_id == FLINT_TIER && row.grade == anchor),
        "the item the offer names has ledger rows carrying the tier the material bought and the \
         grade a bare-handed craft of it comes out at"
    );
    // LIVENESS: not every offer names an item — but on the shipped book every one does, and each
    // names a DIFFERENT one, so the join key is a key.
    let named: std::collections::BTreeSet<&str> = published
        .offers
        .values()
        .map(|offer| offer.output_item_id.as_str())
        .collect();
    assert_eq!(
        named.len(),
        published.offers.len(),
        "each shipped recipe makes its own item, so the join is one-to-one"
    );
}
