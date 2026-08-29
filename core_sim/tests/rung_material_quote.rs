//! **THE `⌃` TRACK CAN PRICE THE RUNG IT IS OFFERING, IN GOODS AS WELL AS IN WORK**
//! (`docs/plan_standing_upkeep.md` §2.7 / §4.9 item 12).
//!
//! The track's third aside reads *"then 1.00 work · 0.05 hurdles a turn to hold"* — the standing
//! price of the rung the player is being **offered**, before they commit. The work half has had a
//! per-rung pre-commit quote for exactly this (`tameUpkeepDemand` / `corralUpkeepDemand`,
//! `cultivationUpkeepDemand` / `fieldUpkeepDemand`). This file is the material twin's proof.
//!
//! # ⛔ THE CATCHING CASE IS A **PASTORAL** HERD, AND NOTHING ELSE REACHES IT
//!
//! The stamped `upkeepMaterialDemand` answers *"what was this source **billed**"* — through the rung
//! it stands **on**. On the shipped ladder that makes the material half of the aside **unreachable**
//! without this pair:
//!
//! - `animal:pen` is the **top of its branch**, so no track ever opens on a penned herd; and
//! - a **pastoral** herd — the only source a track offers the Pen rung from — stands on a rung that
//!   declares **no material at all**, so its stamped bill is empty.
//!
//! So a player looking at the Pen row was told it costs `1.0` work a turn to hold and was **not told
//! about the hurdles**, which is precisely the fact this slice exists to put on screen. A fixture
//! that quoted a *penned* herd would prove nothing: there the stamp and the quote agree, and the
//! defect hides.
//!
//! # ⛔ AND THE BUILD HALF HAS THE SAME HOLE, ONE RUNG UP
//!
//! `buildMaterialCost` publishes the pile of the rung **above** the one a source stands on, so on a
//! **corralled** herd — the top of its branch — it is empty. That is the honest reading of *"what
//! would you climb to next"*, and it is the wrong answer to *"what does another fence RING eat"*,
//! which is a job a penned herd can repeat forever. `corralBuildMaterialCost` answers that one, and
//! the fixtures below pin it against what a ring is actually charged.
//!
//! Every assertion is off the **encoded envelope** — a field that never reached the codec still
//! satisfies an in-process one.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use bevy::prelude::Entity;

use core_sim::{
    advance_labor_allocation, build_test_app, recapture_snapshot_in_place, scalar_from_f32,
    BuildJob, BuildSource, EquipmentConfig, FactionId, FaunaConfigHandle, Herd, HerdRegistry,
    KitChoice, LaborAllocation, LaborAssignment, LaborTarget, LadderConfig, MaterialsConfig,
    PopulationCohort, RecipesConfig, RungKey, SizeClass, SnapshotHistory, SourcePriority,
    TileRegistry, RUNG_COST_UNSCALED,
};

/// The keeper faction — the capture's default viewer, so its own herds are on the wire whatever the
/// fog says.
const FACTION: FactionId = FactionId(0);

/// Rabbit-class dials, matching `pen_feed_priority.rs`, so the fixture species is pennable.
const FODDER_RATE: f32 = 0.10;
const WILD_R: f32 = 0.35;
const BODY_MASS: f32 = 2.0;
const CAPACITY: f32 = 4_000.0;
/// Two herds of **different sizes**, because the rate is `scaled_by: source_load`: a quote that
/// ignored the scale would publish one number for both.
const BIG_HERD: &str = "herd_big";
const SMALL_HERD: &str = "herd_small";
const BIG_BIOMASS: f32 = 400.0;
const SMALL_BIOMASS: f32 = 200.0;

/// The material the `animal:pen` rung eats, on both its build pile and its upkeep rate.
const PEN_MATERIAL: &str = "hurdles";

const EPSILON: f32 = 1e-4;

/// A world with its Startup chain run.
fn a_world() -> App {
    let mut app = build_test_app();
    app.update();
    app
}

/// A land tile the harness map really carries.
fn a_tile(app: &App) -> UVec2 {
    app.world
        .resource::<core_sim::GrazeRegistry>()
        .richest_patch()
        .expect("the harness map seeds graze patches")
        .0
}

/// **Seat two PASTORAL herds** — tamed and owned, and deliberately **not** corralled: this is the
/// rung a `⌃` track opens the Pen offer from, and the one whose own bill names no material.
fn seat_pastoral_herds(app: &mut App, tile: UVec2) {
    let ladder = core_sim::LadderConfig::builtin();
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.clear();
    for (id, biomass) in [(BIG_HERD, BIG_BIOMASS), (SMALL_HERD, SMALL_BIOMASS)] {
        let mut herd = Herd::new(
            id.to_string(),
            format!("Fixture {id}"),
            SizeClass::Small,
            vec![tile],
            biomass,
            CAPACITY,
            FODDER_RATE,
            WILD_R,
            BODY_MASS,
        );
        herd.tame_outright(FACTION, &ladder);
        assert!(
            !herd.is_corralled(),
            "fixture: the herd must be PASTORAL — a penned one is the state where the stamp and \
             the quote agree, and the defect hides"
        );
        registry.herds.push(herd);
    }
}

/// Resolve a turn and publish a frame off the live components.
fn resolve_and_publish(app: &mut App) {
    app.world.run_system_once(advance_labor_allocation);
    // The published herd list is the display telemetry, not the registry — see
    // `grazing_hay_readout.rs` for why a fixture that seats herds has to rebuild it.
    let entries = app.world.resource::<HerdRegistry>().snapshot_entries();
    app.world.resource_mut::<core_sim::HerdTelemetry>().entries = entries;
    recapture_snapshot_in_place(&mut app.world);
}

/// One field of a herd's row, **off the encoded buffer**.
fn published_herd_field<T>(
    app: &App,
    id: &str,
    read: impl Fn(&shadow_scale_flatbuffers::generated::shadow_scale::sim::HerdTelemetryState<'_>) -> T,
) -> T {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

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
        .subsistence()
        .and_then(|section| section.herds())
        .expect("the subsistence section carries the herd list")
        .iter()
        .find(|herd| herd.id().unwrap_or_default() == id)
        .expect("the fixture herd is on the wire");
    read(&row)
}

/// One `[MaterialPayoff]` vector off the encoded frame, as `(material id, amount)`.
///
/// Generic over the vector rather than naming the FlatBuffers type, which is a mouthful and would
/// pull the `flatbuffers` crate into this file's dependency list for one signature.
fn payoffs<'a, V>(rows: Option<V>) -> Vec<(String, f32)>
where
    V: IntoIterator<
        Item = shadow_scale_flatbuffers::generated::shadow_scale::sim::MaterialPayoff<'a>,
    >,
{
    rows.map(|rows| {
        rows.into_iter()
            .map(|row| {
                (
                    row.materialId().unwrap_or_default().to_string(),
                    row.amount(),
                )
            })
            .collect()
    })
    .unwrap_or_default()
}

fn amount_of(rows: &[(String, f32)], material: &str) -> Option<f32> {
    rows.iter()
        .find(|(id, _)| id == material)
        .map(|(_, amount)| *amount)
}

/// ⛔ **A PASTORAL HERD IS QUOTED THE PEN RUNG'S HURDLES, AND ITS STAMPED BILL IS EMPTY.**
///
/// The two together are the whole claim, and each is vacuous alone: an empty stamp passes on a wire
/// that publishes nothing at all, and a populated quote passes on one that merely echoes the stamp.
/// **They must disagree**, and on a pastoral herd they do — one says *what you were billed*, the
/// other *what this rung costs*.
#[test]
fn a_pastoral_herd_is_quoted_the_pen_rungs_material_though_its_own_bill_names_none() {
    let mut app = a_world();
    let tile = a_tile(&app);
    seat_pastoral_herds(&mut app, tile);
    resolve_and_publish(&mut app);

    let quoted = published_herd_field(&app, BIG_HERD, |row| {
        payoffs(row.corralUpkeepMaterialDemand())
    });
    let hurdles = amount_of(&quoted, PEN_MATERIAL).unwrap_or_else(|| {
        panic!(
            "the Pen rung's own rate must be quoted on a herd that has not reached it — that is \
             the only row a `⌃` track ever offers it from, got {quoted:?}"
        )
    });
    assert!(
        hurdles > 0.0,
        "…and it is a real rate, not a row of zero: {hurdles}"
    );

    // **THE PAIRING**: the stamped bill on the same row, the same turn, is empty — which is the
    // state that made the aside unreachable and is why the quote cannot be routed through it.
    let billed = published_herd_field(&app, BIG_HERD, |row| payoffs(row.upkeepMaterialDemand()));
    assert!(
        amount_of(&billed, PEN_MATERIAL).is_none(),
        "**A PASTORAL HERD IS BILLED NO HURDLES** — its own rung declares none, so a quote read off \
         the stamp would publish nothing on exactly the row the player is deciding on: {billed:?}"
    );

    // …and the rung below it quotes nothing, so the pair is not one number echoed twice.
    let tame_quote = published_herd_field(&app, BIG_HERD, |row| {
        payoffs(row.tameUpkeepMaterialDemand())
    });
    assert!(
        tame_quote.is_empty(),
        "`animal:pastoral` declares no material, so its quote is EMPTY — never a row of zero, and \
         never the Pen rung's number copied down: {tame_quote:?}"
    );
}

/// **THE QUOTE IS SCALED BY THE SOURCE, exactly as its work twin is** — the rung reads one
/// `scaled_by` for both currencies, so a herd twice the size is quoted twice the hurdles.
///
/// Asserted as a **ratio between the two herds** rather than against a remembered number, so a
/// retune of the rung moves the fixture with the game; and paired with the *work* quote's own ratio,
/// which is the liveness half — a ratio of `1` would mean the two herds do not differ at all.
#[test]
fn the_material_quote_scales_with_the_herd_exactly_as_the_work_quote_does() {
    let mut app = a_world();
    let tile = a_tile(&app);
    seat_pastoral_herds(&mut app, tile);
    resolve_and_publish(&mut app);

    let material = |id: &str| {
        amount_of(
            &published_herd_field(&app, id, |row| payoffs(row.corralUpkeepMaterialDemand())),
            PEN_MATERIAL,
        )
        .expect("both fixture herds are quoted the Pen rung's material")
    };
    let work = |id: &str| published_herd_field(&app, id, |row| row.corralUpkeepDemand());

    let herds = BIG_BIOMASS / SMALL_BIOMASS;
    assert!(
        (material(BIG_HERD) / material(SMALL_HERD) - herds).abs() < EPSILON,
        "the material quote is linear in the herd: {} / {} against {herds}",
        material(BIG_HERD),
        material(SMALL_HERD)
    );
    assert!(
        (work(BIG_HERD) / work(SMALL_HERD) - herds).abs() < EPSILON,
        "**LIVENESS**: so is the WORK quote beside it, on the same measure — if this ratio is `1` \
         the fixture's two herds do not differ and the claim above is about nothing"
    );
    assert!(
        material(BIG_HERD) > material(SMALL_HERD),
        "…and the two herds really are quoted different numbers"
    );
}

/// **THE PLANT WEB PUBLISHES THE PAIR AND IT IS HONESTLY EMPTY** — both plant rungs declare no
/// material today, so *"empty means no row and never zero"* is the whole reading.
///
/// It is asserted rather than left implicit because the seam exists for the **route** branch's
/// stone: a client that learned to render a `0.0` here would render it wrong the day a plant rung
/// declares one.
#[test]
fn the_plant_rungs_quote_no_material_and_publish_no_row_for_it() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = a_world();
    resolve_and_publish(&mut app);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let patches = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.foragePatches())
        .expect("the subsistence section carries the patch list");
    let row = patches
        .iter()
        .next()
        .expect("the harness map seeds patches");

    // **LIVENESS**: the WORK quote beside it is a real number on this same row, so *"empty"* below
    // is a statement about the material term and not about a row nobody filled in.
    assert!(
        row.cultivationUpkeepDemand() > 0.0 && row.fieldUpkeepDemand() > 0.0,
        "fixture: the patch's work quotes must be live, or the emptiness below proves nothing"
    );
    assert!(
        payoffs(row.cultivationUpkeepMaterialDemand()).is_empty(),
        "no plant rung declares a material, so the quote is EMPTY — never a row of zero"
    );
    assert!(
        payoffs(row.fieldUpkeepMaterialDemand()).is_empty(),
        "…and the same on the Field rung"
    );
}

/// **A herd nobody has tamed is quoted the Pen rung's material too** — the `*UpkeepDemand` pair's own
/// *"always meaningful, never a sentinel"* rule, and `herders_needed_if_managed`'s reason: **a quote
/// has to exist before the herd is anyone's.**
///
/// It reads one of **worldgen's own** wild herds rather than seating a fixture one, because the
/// published herd list is fog-filtered: a hand-seated unowned herd on a tile the viewer has not
/// revealed is simply not on the wire, and the test would fail on its reader rather than on its
/// claim.
#[test]
fn a_wild_herd_is_quoted_the_pen_rungs_material_too() {
    let mut app = a_world();
    resolve_and_publish(&mut app);

    let wild = published_wild_herd_id(&app);
    let quoted = published_herd_field(&app, &wild, |row| payoffs(row.corralUpkeepMaterialDemand()));
    assert!(
        amount_of(&quoted, PEN_MATERIAL).is_some_and(|amount| amount > 0.0),
        "a quote has to exist before the herd is anyone's: {quoted:?}"
    );
    // **LIVENESS**: the work quote beside it is live on the same row, so the material one is not
    // reading a row the capture never filled in.
    assert!(
        published_herd_field(&app, &wild, |row| row.corralUpkeepDemand()) > 0.0,
        "fixture: the wild herd's own work quote must be live too"
    );
}

/// The id of a wild herd **that actually reached the wire** — worldgen's own, picked off the encoded
/// frame so the fog has already had its say.
fn published_wild_herd_id(app: &App) -> String {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.herds())
        .expect("the subsistence section carries the herd list")
        .iter()
        .find(|herd| herd.domestication() <= 0.0 && !herd.corralled())
        .map(|herd| herd.id().unwrap_or_default().to_string())
        .expect("the harness map seeds wild herds the viewer can see")
}

// ---------------------------------------------------------------------------------------------
// **THE RING'S OWN PILE** (`docs/plan_standing_upkeep.md` §4.9 item 12c)
// ---------------------------------------------------------------------------------------------
//
// The ring price card opens on a **corralled** herd and nowhere else, and `buildMaterialCost`
// publishes the rung *above* the one the herd stands on — which, at the top of the animal branch,
// is none. So the card could state a work price and a standing bill and **not the pile the ring
// eats**, which is the one number a player short of hurdles needs.

/// One herd, penned — the only row a ring is ever offered from, and the row where
/// `buildMaterialCost` is honestly empty.
const CORRALLED_HERD: &str = "herd_penned";

/// The escapement floor a keeper row carries, matching `pen_material_priority.rs`'s fixture.
const KEEPER_FLOOR: f32 = 0.5;

/// A builders crew large enough to close a whole ring in **one turn**, so the fixture can compare
/// the published pile against a *completed* ring rather than against a fraction of one. A bare
/// builder banks `PER_WORKER_OUTPUT` work units a turn at full discipline and the pen rung's span is
/// a few dozen of them, so this carries a wide margin for a floor's worth of slack — and the fixture
/// asserts the ring really did close rather than trusting the margin.
const RING_BUILDERS: u32 = 400;

/// The band's working head-count, sized past every row this fixture hands it so `normalize` sheds
/// nobody — a trimmed builders row would bank a fraction and the ring would not close.
const BAND_WORKERS: f32 = 5_000.0;

/// A store no claim on it can bind, so the turn measures the **ring** rather than the shelf.
const AMPLE_MATERIAL: f32 = 10_000.0;

/// The slack a claim routed through the **store** costs: a `LocalStore` holds fixed-point `Scalar`
/// batches, so stating an amount and reading it back quantises twice. [`EPSILON`] is the right bar
/// for the exact arithmetic; this is the right one for a figure measured as a fall in the shelf.
const STORE_QUANTUM_SLACK: f32 = 1e-2;

/// The `animal:pen` rung's whole build **pile** of the good, read off the shipped ladder — never a
/// number copied out of the JSON, so a retune moves the fixture with the game.
fn pen_build_pile() -> f32 {
    LadderConfig::builtin()
        .rung(RungKey::AnimalPen)
        .build_materials()
        .find(|(id, _)| *id == PEN_MATERIAL)
        .map(|(_, amount)| amount)
        .expect("the shipped pen rung declares the good on its build pile")
}

/// The `animal:pen` rung's work span, **unscaled** — the width `head_ring_leg` prices a ring at.
fn pen_build_cost() -> f32 {
    LadderConfig::builtin()
        .rung(RungKey::AnimalPen)
        .build_cost(RUNG_COST_UNSCALED)
        .expect("the shipped pen rung carries a build meter")
}

/// **Seat one CORRALLED herd** — the rung a ring is offered from, and the top of its branch.
fn seat_a_corralled_herd(app: &mut App, tile: UVec2) {
    let ladder = LadderConfig::builtin();
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.clear();
    let mut herd = Herd::new(
        CORRALLED_HERD.to_string(),
        format!("Fixture {CORRALLED_HERD}"),
        SizeClass::Small,
        vec![tile],
        BIG_BIOMASS,
        CAPACITY,
        FODDER_RATE,
        WILD_R,
        BODY_MASS,
    );
    herd.tame_outright(FACTION, &ladder);
    assert!(
        herd.corral_at(tile, &ladder),
        "fixture: the herd must be PENNED — a pastoral one is the row where `buildMaterialCost` \
         already carries the pen's pile and the gap hides"
    );
    registry.herds.push(herd);
}

/// The harness world's own band, moved onto `tile` and widened past every row this fixture hands
/// it. Reusing worldgen's band rather than spawning one keeps the fixture to the state under test.
fn the_band_on(app: &mut App, tile: UVec2) -> Entity {
    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .expect("the fixture tile resolves");
    let mut bands = app.world.query::<(Entity, &mut PopulationCohort)>();
    let (entity, mut cohort) = bands
        .iter_mut(&mut app.world)
        .next()
        .expect("the harness campaign seats a band");
    cohort.home = tile_entity;
    cohort.current_tile = tile_entity;
    cohort.working = scalar_from_f32(BAND_WORKERS);
    entity
}

/// The empty kit, so the ring's pace is the pool's own and no start-stocked tool moves it.
fn bare_builders() -> KitChoice {
    EquipmentConfig::builtin()
        .kit("none")
        .expect("the shipped roster carries the empty kit")
}

/// Put a ring in flight on the penned herd and staff it, with a keeper row beside it so the pen is
/// held exactly as a played one is.
fn begin_a_ring(app: &mut App, band: Entity) {
    let radius_max = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .husbandry
        .pen_radius_max;
    let began = app
        .world
        .resource_mut::<HerdRegistry>()
        .herds
        .iter_mut()
        .find(|herd| herd.id == CORRALLED_HERD)
        .expect("the pen is seated")
        .begin_pen_extension(radius_max);
    assert!(began, "a built pen below the radius cap may begin a ring");

    let source = BuildSource::Herd(CORRALLED_HERD.to_string());
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation");
    allocation.assignments.clear();
    allocation.assignments.push(LaborAssignment {
        target: LaborTarget::Hunt {
            fauna_id: CORRALLED_HERD.to_string(),
            floor: KEEPER_FLOOR,
        },
        workers: RING_BUILDERS,
        kit: None,
        priority: SourcePriority::default(),
        upkeep_kit: None,
    });
    allocation.assignments.push(LaborAssignment {
        target: LaborTarget::Builders,
        workers: RING_BUILDERS,
        kit: None,
        priority: SourcePriority::default(),
        upkeep_kit: None,
    });
    allocation.build_queue.clear();
    assert!(
        allocation.enqueue_build(source.clone(), BuildJob::ExtendPen),
        "the band works the pen it is ringing"
    );
    assert!(
        allocation.set_build_entry_kit(&source, Some(bare_builders())),
        "the entry just declared takes the bare kit"
    );
}

/// Stock the band with the pen's good, in the band and characteristics the shipped book makes it in.
fn stock_hurdles(app: &mut App, band: Entity, units: f32) {
    let materials = MaterialsConfig::builtin();
    let recipes = RecipesConfig::builtin();
    let characteristics = recipes
        .recipes()
        .find_map(|(_, recipe)| {
            recipe
                .outputs
                .iter()
                .find(|output| output.material_id() == Some(PEN_MATERIAL))
                .map(|output| output.characteristics.clone())
        })
        .expect("the shipped book makes the pen's material");
    let band_key = materials
        .band_key(PEN_MATERIAL, &characteristics)
        .expect("the shipped roster rates the pen's material");
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band persists");
    cohort.stores.deposit_material(
        PEN_MATERIAL,
        band_key,
        scalar_from_f32(units),
        &characteristics,
    );
}

/// What the band still holds of the good.
fn store_holds(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band persists")
        .stores
        .material_total(PEN_MATERIAL)
        .to_f32()
}

/// What the pen was **paid** of the good toward its standing keeping this turn — the only other
/// claim on the shelf in this fixture, and therefore the only term to net off the fall.
fn upkeep_paid(app: &App) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(CORRALLED_HERD)
        .expect("the pen is still seated")
        .upkeep_materials_supplied
        .get(PEN_MATERIAL)
        .copied()
        .unwrap_or(0.0)
}

/// ⛔ **A CORRALLED HERD IS QUOTED THE RING'S PILE, THOUGH ITS BUILD COST IS EMPTY.**
///
/// The two halves are the whole claim and each is vacuous alone: the emptiness passes on a wire that
/// publishes nothing at all, and the quote passes on one that merely echoes `buildMaterialCost`.
/// **They must disagree**, and on a penned herd they do — one says *what you would climb to next*
/// (nothing; this is the top of the branch), the other *what another ring eats*.
#[test]
fn a_corralled_herd_is_quoted_the_rings_pile_though_its_build_cost_is_empty() {
    let mut app = a_world();
    let tile = a_tile(&app);
    seat_a_corralled_herd(&mut app, tile);
    resolve_and_publish(&mut app);

    let quoted = published_herd_field(&app, CORRALLED_HERD, |row| {
        payoffs(row.corralBuildMaterialCost())
    });
    let hurdles = amount_of(&quoted, PEN_MATERIAL).unwrap_or_else(|| {
        panic!(
            "a penned herd is the only row a RING is ever offered from, so the pen rung's own pile \
             has to be quoted there, got {quoted:?}"
        )
    });
    assert!(
        (hurdles - pen_build_pile()).abs() < EPSILON,
        "…and it is the LADDER's whole pile for that rung, unscaled: {hurdles} against {}",
        pen_build_pile()
    );

    // **THE PAIRING**: the field a client used to read is empty on this same row, the same turn.
    // Without it the claim above passes on a fixture where the two are indistinguishable and proves
    // nothing about the gap it closes.
    let above = published_herd_field(&app, CORRALLED_HERD, |row| payoffs(row.buildMaterialCost()));
    assert!(
        above.is_empty(),
        "**`animal:pen` IS THE TOP OF ITS BRANCH** — the rung above it is none, so the pile of the \
         rung the `⌃` track would offer is honestly EMPTY here, which is why a ring card cannot be \
         built out of it: {above:?}"
    );

    // **LIVENESS**: the WORK half of the same card is a real number on the same row, so the
    // emptiness above is a statement about that field and not about a row the capture never filled.
    assert!(
        published_herd_field(&app, CORRALLED_HERD, |row| row.corralWorkCost()) > 0.0,
        "fixture: the ring's work price must be live, or neither claim above is about anything"
    );
}

/// **THE PUBLISHED PILE IS WHAT A RING IS ACTUALLY CHARGED** — the claim a presence-check misses,
/// and the reason this slice publishes the pile rather than letting a client re-derive it.
///
/// This drives the **strong** form of the claim: a real `ExtendPen` job with a crew big enough to
/// close the ring in one turn, and the good genuinely taken off the band's shelf over that completed
/// ring compared against the published quote. The narrower alternative — asserting the quote equals
/// `head_ring_leg`'s width put through `build_material_wants` — was not needed, because the harness
/// world already carries a band and clearing the herd registry leaves the pen as the **only** claim
/// on the good, so the fall in the shelf is directly measurable. `pen_material_priority.rs` pins the
/// *partial* form of the same charge, turn by turn.
#[test]
fn the_published_pile_is_what_a_completed_ring_actually_eats() {
    let mut app = a_world();
    let tile = a_tile(&app);
    seat_a_corralled_herd(&mut app, tile);
    let band = the_band_on(&mut app, tile);
    begin_a_ring(&mut app, band);
    stock_hurdles(&mut app, band, AMPLE_MATERIAL);
    resolve_and_publish(&mut app);

    let quoted = amount_of(
        &published_herd_field(&app, CORRALLED_HERD, |row| {
            payoffs(row.corralBuildMaterialCost())
        }),
        PEN_MATERIAL,
    )
    .expect("the penned herd is quoted the ring's pile");

    let (extending, banked) = {
        let herd = app
            .world
            .resource::<HerdRegistry>()
            .find(CORRALLED_HERD)
            .expect("the pen is still seated");
        (herd.pen_extending, herd.pen_extend_progress)
    };
    assert!(
        !extending,
        "fixture: the crew must CLOSE the ring this turn, or this measures a fraction of one \
         (banked {banked} of {})",
        pen_build_cost()
    );

    // The fall in the shelf, less the pen's own standing keeping — the only other claim on the good
    // in this world, because the herd registry was cleared and no plant rung declares a material.
    let drawn = AMPLE_MATERIAL - store_holds(&app, band) - upkeep_paid(&app);
    assert!(
        (drawn - quoted).abs() < STORE_QUANTUM_SLACK,
        "a whole ring swallows exactly the quoted pile: drew {drawn} against a quote of {quoted}"
    );
    // **LIVENESS**: both sides are real quantities, so the equality is not two zeroes agreeing.
    assert!(
        drawn > STORE_QUANTUM_SLACK && quoted > STORE_QUANTUM_SLACK,
        "a ring is not free and the quote is not empty: drew {drawn}, quoted {quoted}"
    );
}

/// **ON A PASTORAL HERD THE TWO FIELDS AGREE, BY CONSTRUCTION** — the same rung's pile reached
/// through two selectors, never a second reading of the ladder.
///
/// The schema comment claims exactly this and a client leans on it: the ring quote and the `⌃`
/// track's pile aside state the same number on the one row where both are offered. If it ever
/// diverges the comment has become a lie, and this says so.
#[test]
fn a_pastoral_herds_ring_quote_and_build_cost_are_the_same_pile() {
    let mut app = a_world();
    let tile = a_tile(&app);
    seat_pastoral_herds(&mut app, tile);
    resolve_and_publish(&mut app);

    let ring = published_herd_field(&app, BIG_HERD, |row| payoffs(row.corralBuildMaterialCost()));
    let climb = published_herd_field(&app, BIG_HERD, |row| payoffs(row.buildMaterialCost()));
    assert_eq!(
        ring, climb,
        "the climb to the Pen rung and another ring of it are the SAME rung's pile"
    );
    // **LIVENESS**: they agree on a real pile rather than on two empty lists — the failure mode this
    // whole file exists to catch.
    assert!(
        amount_of(&ring, PEN_MATERIAL)
            .is_some_and(|amount| (amount - pen_build_pile()).abs() < EPSILON),
        "…and that pile is the ladder's own for `animal:pen`: {ring:?} against {}",
        pen_build_pile()
    );
}
