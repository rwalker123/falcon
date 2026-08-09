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
use bevy::prelude::Entity;

use core_sim::{
    build_headless_app, recapture_snapshot_in_place, scalar_from_f32, BandBench, BandEquipment,
    EquipmentConfigHandle, MaterialsConfigHandle, PopulationCohort, ResidentBand, SnapshotHistory,
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
const SLED_ITEM: &str = "sled";
const SPEARS_ITEM: &str = "spears";
const TANNING_FRAME_ITEM: &str = "tanning_frame";

/// Enough hide that a `work: 8` sled recipe's 6-unit draw is comfortably covered.
const PLENTY: f32 = 40.0;

/// A world with a resident band, captured once so the fixtures have a frame to read.
fn world() -> (App, Entity) {
    let mut app = build_headless_app();
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
        wear.wear_item(&equipment, item, f32::MAX / 2.0);
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
/// `(recipe id, workers, progress, work, teaches, blocked reason)`.
type PublishedBench = (String, u32, f32, f32, String, String);

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
}

#[derive(Clone, Debug)]
struct PublishedItem {
    item_id: String,
    tier_id: String,
    /// A start-stocked unit has none — a shipped kit has a tier but was never on anyone's bench —
    /// so it is decoded and asserted `""` rather than left off the struct: *"it decodes"* is half
    /// the claim this file makes about every field.
    grade: String,
    count: u32,
    remaining: f32,
    quanta_left: f32,
    quantum_noun: String,
    life: String,
    life_severity: String,
}

/// Recapture and decode. **Encoded from the ring entry's snapshot** rather than through
/// `StoredSnapshot::encode_flat`, for the reason `kit_selection.rs` states: the cached bytes are the
/// ones published when the entry was first written, so a fixture that banks material and recaptures
/// would otherwise be decoding the frame from before it banked anything.
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
    let bench = (
        bench_row.recipeId().unwrap_or_default().to_string(),
        bench_row.workers(),
        bench_row.progress(),
        bench_row.work(),
        bench_row.teaches().unwrap_or_default().to_string(),
        bench_row.blockedReason().unwrap_or_default().to_string(),
    );
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
    assert_eq!(
        owned[0].count, 1,
        "a spawn stocks one unit of every kit item"
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
    assert!(remaining > 0.0 && count == 1, "owned and with life left");
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
            wear.wear_item(&equipment, item, 1.0);
        }
    }
    let published = publish(&mut app, band);

    let spears = rows_for(&published, SPEARS_ITEM);
    let clubs = rows_for(&published, "clubs");
    assert_eq!(
        spears[0].quantum_noun, "kills",
        "spears wear per animal killed"
    );
    assert_eq!(
        clubs[0].quantum_noun, "raids",
        "clubs wear per fight resolved"
    );
    assert_ne!(
        spears[0].quantum_noun, clubs[0].quantum_noun,
        "the noun is the ITEM's, not one word for every row — that is what the client must not \
         re-derive"
    );
    for row in [&spears[0], &clubs[0]] {
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
        "coarse",
        "worst-first: the 0.14 hide is spent before the 0.92 one, so the next sled is coarse"
    );
}

/// **The bench publishes what is being made, against what work, and why it is stopped** — and an
/// idle bench is a different state from a blocked one.
#[test]
fn the_bench_publishes_its_job_its_work_and_its_refusal() {
    let (mut app, band) = world();
    let idle = publish(&mut app, band);
    assert_eq!(idle.bench.0, "", "an idle bench names no recipe");
    assert_eq!(
        idle.bench.5, "",
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
    let (recipe_id, workers, _progress, work, teaches, blocked_reason) = blocked.bench.clone();
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
        running.bench.5, "",
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
        published.bench.5, "No one at the bench",
        "the bench states the crew's refusal even though the craft itself is fine"
    );
    assert!(
        offer(&published, SLED_RECIPE).available,
        "the OFFER is still available — staffing is the player's next move, not a refusal"
    );
}

/// **The three per-world catalogues round-trip**, and the rating vocabulary rides once rather than
/// per material.
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
    assert_eq!(
        published.materials.len(),
        3,
        "only the three organics ship — an unreachable material is dead content"
    );

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

/// **The offer row names the item it makes**, which is the join a ledger row is drawn from: the
/// offer supplies the name and the rebuild cost, the equipment batch supplies the tier and the life.
#[test]
fn an_offer_names_the_item_it_makes_so_the_ledger_can_join_the_two_halves() {
    let (mut app, band) = world();
    let published = publish(&mut app, band);
    let sled = offer(&published, SLED_RECIPE);
    assert_eq!(sled.output_item_id, SLED_ITEM);
    assert_eq!(sled.display_name, "Sled");
    assert!(
        rows_for(&published, &sled.output_item_id)
            .iter()
            .any(|row| row.tier_id == "flint" && row.grade.is_empty()),
        "the item the offer names has ledger rows carrying the tier the material bought — and no \
         GRADE, because a start-stocked unit was never on anyone's bench"
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
