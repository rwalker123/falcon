//! **THE HAY BILL IS PUBLISHED AS THE GAP, AND THE SIM DOES ITS ARITHMETIC.**
//!
//! A pen eats the grass its fenced footprint grows and the hay its keeper carries in. Grazing is
//! free; **hay is the thing the player has to farm**, so the number a readout has to carry is the
//! *gap the footprint leaves* — `max(0, fodder_per_biomass × biomass − footprint_intake)` — and not
//! the gross demand, which `penPastureFraction` already states the land's share of.
//!
//! Four fields carry it, and this file asserts every one of them **off the encoded envelope** — the
//! artifact a client actually parses, because a field can be right in the capture and wrong in the
//! codec:
//!
//! | field | on | what it says |
//! |---|---|---|
//! | `penFodderShortfall` | the herd row | how much MORE this pen needs — its gap less `fodderDraw` |
//! | `fodderNeed` | the cohort row | the gap itself, summed over every pen the band keeps |
//! | `fodderIncome` | the cohort row | the hay the band's Fields grew this turn |
//! | `turnsOfFodder` | the cohort row | the runway, in the larder runway's own idiom and sentinel |
//!
//! **The per-pen gap is not among them.** It rode the herd row as `penHayNeed` and nothing rendered
//! it, because what a pen row states is how much MORE it needs; the field is `(deprecated)` and the
//! quantity survives only as the band roll-up and as the shortfall's own first term. So a fixture
//! that wants to read one pen's gap carries **nothing in** and reads the shortfall
//! ([`published_hay_need_with_nothing_carried_in`], which pins the draw at zero as it goes).
//!
//! # ⛔ WHAT THESE FIXTURES PIN
//!
//! 1. **The gap, at three coverages in one turn** — a footprint that covers the pen publishes
//!    **zero**, a barren one publishes the **whole** demand, and a part-covered one publishes the
//!    **difference**. Three pens on one band, so a field wired to the gross demand fails on the
//!    first, and one wired to the raw demand-minus-nothing fails on the third.
//! 2. **The band totals every pen it keeps**, over two pens of deliberately **different** sizes — so
//!    a sum is distinguishable from a copy of either term.
//! 3. **A band with no pens owes nothing and has no runway to speak of** — the ∞ sentinel, not a
//!    number of turns.
//! 4. **The need survives the Foddering gate.** A band that cannot draw hay at all still publishes
//!    what its herd is short: the remedy being knowledge rather than hay is exactly the case the
//!    player most needs to see, and the fixture pins the draw at `0` beside the need to prove the
//!    gate really is shut.
//! 5. **A growing herd on a fixed footprint publishes a rising need** — the slow trap the whole
//!    readout exists for, pinned rather than described.
//! 6. **The runway is the larder's**, sentinel and all, on a band whose hay Field genuinely
//!    out-grows the pen it feeds — which is also what pins `fodderIncome` as a live field rather
//!    than a published zero.
//! 7. **The shortfall is the sim's subtraction, not the client's** — `max(0, gap − fodderDraw)` at
//!    four coverages (fully fed, part-supplied, nothing carried in, over-served by the fixed-point
//!    quantisation), with the gap and the draw held non-zero and *different* so no arm can pass on a
//!    copy of either term. The gap it is differenced against is the **fixture's own** figure, not a
//!    second published field, so the arithmetic is checked rather than restated.
//! 8. **The runway counts down the DRAIN, not the need.** A band that cannot draw hay at all empties
//!    nothing however short its pens are, so it publishes the no-drain sentinel while its need still
//!    states the gap — the same band with Foddering gets a finite runway off the same store.
//! 9. **The ledger dies with the band.** A band that loses its last working hand sheds every row and
//!    leaves the labor pass early; it publishes `0` need, `0` income and the sentinel, never last
//!    turn's figures for pens it no longer keeps.
//!
//! **The footprint intake is posed directly** (skipping `advance_herd_grazing`), the fixture idiom
//! `grazing_f3_fodder.rs` and `pen_feed_priority.rs` already use: the coverage under test is then
//! exactly the number stated, rather than whatever the map's grass happened to hand over.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::{
    advance_labor_allocation, build_test_app, recapture_snapshot_in_place, scalar_from_f32,
    scalar_one, scalar_zero, FactionId, ForageRegistry, GenerationId, Herd, HerdRegistry,
    LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget, LocalStore, MoraleCause,
    PopulationCohort, ResidentBand, SimulationConfig, SizeClass, SnapshotHistory, SourcePriority,
    StartingUnit, TakeSelection, TileRegistry, TransferLink, FODDER, FODDERING_DISCOVERY_ID,
    NOT_FOOD_LIMITED_TURNS,
};

/// The keeper faction — the capture's default viewer, so its own pens are on the wire whatever the
/// fog says.
const FACTION: FactionId = FactionId(0);

/// A head-count big enough that nothing in these fixtures is ever worker-limited: what is under test
/// is a published number, never a staffing.
const KEEPERS: u32 = 5_000;

/// The fixture species' metabolic demand — fodder eaten per unit of biomass per turn, the same rate
/// `pen_feed_priority.rs` authors. A pen's gross demand is this × its biomass.
const FODDER_RATE: f32 = 0.10;
/// The fixture species' wild breeding rate and body mass — rabbit-class, matching [`FODDER_RATE`].
const WILD_R: f32 = 0.35;
const PEN_BODY_MASS: f32 = 2.0;
/// A pen ceiling far above every biomass seated here, so no fixture is capacity-limited.
const PEN_CAPACITY: f32 = 4_000.0;

/// The Sustain floor every keeper works its pen at.
const SUSTAIN: f32 = 0.5;

/// `f32` slack for two readings of one expression — a few ULPs of `Scalar` quantisation, no more.
const EPSILON: f32 = 1e-4;

/// The three coverages, as biomasses and posed intakes. The **fed** pen's footprint hands over its
/// whole demand, the **barren** one hands over nothing, and the **partly grazed** one covers exactly
/// half — three different answers from one turn.
const FED_PEN: &str = "pen_fed_by_its_land";
const BARREN_PEN: &str = "pen_on_barren_ground";
const PART_PEN: &str = "pen_half_grazed";
const FED_BIOMASS: f32 = 100.0;
const BARREN_BIOMASS: f32 = 150.0;
const PART_BIOMASS: f32 = 200.0;

/// The two pens of the roll-up fixture — **different sizes on purpose**, so their sum is not any
/// pen's own figure and a total that merely copied one term cannot pass.
const BIG_PEN: &str = "pen_big";
const SMALL_PEN: &str = "pen_small";
const BIG_BIOMASS: f32 = 300.0;
const SMALL_BIOMASS: f32 = 80.0;

/// A pen small enough that a real hay Field out-grows it — the runway fixture's subject, and the
/// reason its no-drain arm is a genuine *income covers the need*, not a degenerate zero-need band.
const TINY_PEN: &str = "pen_tiny";
const TINY_BIOMASS: f32 = 4.0;

/// The band's hay store, stated wherever a runway has to divide by something.
const A_HAY_RESERVE: f32 = 60.0;

/// A barren footprint — the drylot case, where the whole grass demand is a gap.
const BARREN: f32 = 0.0;

/// The gross fodder a pen at `biomass` demands in a turn — reconstructed from the species rate, not
/// read back off the sim, so the assertions have an independent number to compare against.
fn gross_demand(biomass: f32) -> f32 {
    FODDER_RATE * biomass
}

// --------------------------------------------------------------------------------------------
// Fixtures
// --------------------------------------------------------------------------------------------

/// A world with its Startup chain run — the tile registry, the forage patches and the graze layer
/// the fixtures below stand on.
fn a_world() -> App {
    let mut app = build_test_app();
    app.update();
    app
}

/// Seat one penned, tamed herd per `(id, biomass)` on `tile`, clearing whatever worldgen left. Every
/// pen is corralled where its keeper stands, so all of them are inside the hunt leash and the only
/// thing separating them is the footprint intake posed onto each.
fn seat_pens(app: &mut App, tile: UVec2, pens: &[(&str, f32)]) {
    let ladder = core_sim::LadderConfig::builtin();
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.clear();
    for (id, biomass) in pens.iter().copied() {
        let mut herd = Herd::new(
            id.to_string(),
            format!("Fixture {id}"),
            SizeClass::Small,
            vec![tile],
            biomass,
            PEN_CAPACITY,
            FODDER_RATE,
            WILD_R,
            PEN_BODY_MASS,
        );
        herd.tame_outright(FACTION, &ladder);
        assert!(
            herd.corral_at(tile, &ladder),
            "the fixture species must be pennable"
        );
        registry.herds.push(herd);
    }
}

/// Pose one pen's grazed footprint directly — the coverage under test, exact.
fn pose_intake(app: &mut App, herd_id: &str, intake: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry
        .herds
        .iter_mut()
        .find(|herd| herd.id == herd_id)
        .expect("the fixture pen is seated");
    herd.footprint_intake = intake;
}

/// One keeper row: tend the pen `herd_id` at the Sustain floor.
fn keeper_row(herd_id: &str) -> LaborAssignment {
    LaborAssignment {
        target: LaborTarget::Hunt {
            fauna_id: herd_id.to_string(),
            floor: SUSTAIN,
        },
        workers: KEEPERS,
        kit: None,
        priority: SourcePriority::default(),
        upkeep_kit: None,
    }
}

/// A resident band standing on `tile` holding `assignments`.
fn spawn_band(app: &mut App, tile: UVec2, assignments: Vec<LaborAssignment>) -> Entity {
    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .expect("the fixture tile resolves");
    app.world
        .spawn((
            ResidentBand,
            PopulationCohort {
                home: tile_entity,
                current_tile: tile_entity,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(KEEPERS as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FACTION,
                knowledge: Vec::new(),
                migration: None,
            },
            StartingUnit {
                kind: "BandKeeper".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments,
                ..Default::default()
            },
        ))
        .id()
}

/// **The one hex every pen fixture stands on** — the band and its pens share it, so every pen is
/// inside the hunt leash and the only thing separating them is the coverage posed onto each. Checked
/// against the map rather than trusted, so a grid-size change fails loudly instead of silently
/// standing the fixture off the world.
fn pen_tile(app: &App) -> UVec2 {
    let tile = UVec2::new(1, 1);
    assert!(
        app.world
            .resource::<TileRegistry>()
            .index(tile.x, tile.y)
            .is_some(),
        "the harness map must carry the fixture tile {tile:?}"
    );
    tile
}

/// Grant the keeper faction **Foddering**, so its pens may draw the hay store at all.
fn learn_foddering(app: &mut App) {
    app.world
        .resource_mut::<core_sim::DiscoveryProgressLedger>()
        .add_progress(FACTION, FODDERING_DISCOVERY_ID, scalar_one());
}

/// Resolve the turn's labor pass and publish a frame off the live components — the two steps every
/// assertion in this file reads through.
fn resolve_and_publish(app: &mut App) {
    app.world.run_system_once(advance_labor_allocation);
    // **The published herd list is the display telemetry, not the registry** — `advance_herds`
    // rebuilds it at the end of its own pass, so a fixture that seats herds and captures without
    // driving Logistics has to rebuild it here or the wire carries the map's own animals. Driving
    // the real system instead would regraze and regrow the pens, which is a different fixture.
    let entries = app.world.resource::<HerdRegistry>().snapshot_entries();
    app.world.resource_mut::<core_sim::HerdTelemetry>().entries = entries;
    recapture_snapshot_in_place(&mut app.world);
}

// --------------------------------------------------------------------------------------------
// Readers — every one of them off the ENCODED envelope
// --------------------------------------------------------------------------------------------

/// One field of a herd's row, read out of the encoded buffer.
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
        .expect("the fixture pen is on the wire");
    read(&row)
}

/// **The hay this pen needs per turn, at a fixture that carries nothing in** — the published
/// `penFodderShortfall`, which is the pen's own gap less the hay its keeper drew, read as the gap
/// itself *because the draw is zero*. The gap has no field of its own on the wire (`penHayNeed` is
/// retired: nothing rendered it), so the assertion that keeps this honest travels with the reader —
/// a fixture that ever hands this pen hay fails here rather than quietly reading a difference as a
/// need.
fn published_hay_need_with_nothing_carried_in(app: &App, id: &str) -> f32 {
    let draw = published_hay_draw(app, id);
    assert!(
        draw.abs() < EPSILON,
        "this reader states a pen's GAP, which is its published shortfall only while nothing is          carried in — pen {id} drew {draw}"
    );
    published_hay_shortfall(app, id)
}

/// The hay this pen actually drew, as published — the *draw* beside the *need*, which is what makes
/// the Foddering arm a comparison rather than a claim.
fn published_hay_draw(app: &App, id: &str) -> f32 {
    published_herd_field(app, id, |herd| herd.fodderDraw())
}

/// **How much more fodder this pen still needs**, as published — the readout number the sim differences
/// so the client does not have to: `max(0, gap − fodderDraw)`, and the only term of that subtraction
/// on the wire.
fn published_hay_shortfall(app: &App, id: &str) -> f32 {
    published_herd_field(app, id, |herd| herd.penFodderShortfall())
}

/// The band's whole hay ledger off **its own** encoded cohort row: `(need, income, runway, store)`.
/// Addressed by the band's ECS handle, which is what the row publishes, so the world's own starting
/// bands cannot be mistaken for the fixture's.
fn published_band_ledger(app: &App, band: Entity) -> (f32, f32, f32, f32) {
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
    let cohort = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.entity() == band.to_bits())
        .expect("the fixture band is on the wire");
    (
        cohort.fodderNeed(),
        cohort.fodderIncome(),
        cohort.turnsOfFodder(),
        cohort.fodderStore(),
    )
}

// --------------------------------------------------------------------------------------------
// 1. The gap, at three coverages
// --------------------------------------------------------------------------------------------

/// ⛔ **WHAT A PEN PUBLISHES IS THE HAY ITS OWN LAND DOES NOT GROW.**
///
/// Three pens, one band, one turn, three coverages: fully fed by its footprint (**zero** need),
/// barren (**the whole** demand), and half grazed (**the difference**). The gross demand is
/// reconstructed from the species rate rather than read back off the sim, so each assertion compares
/// the published field against an independent number.
///
/// A field wired to the gross demand fails the first arm; one that forgot to subtract the intake
/// fails the third; one clamped the wrong way round fails the second.
#[test]
fn a_pens_published_need_is_the_gap_its_footprint_leaves() {
    let mut app = a_world();
    let tile = pen_tile(&app);
    learn_foddering(&mut app);
    seat_pens(
        &mut app,
        tile,
        &[
            (FED_PEN, FED_BIOMASS),
            (BARREN_PEN, BARREN_BIOMASS),
            (PART_PEN, PART_BIOMASS),
        ],
    );
    // Coverage: the whole demand, none of it, and exactly half of it.
    pose_intake(&mut app, FED_PEN, gross_demand(FED_BIOMASS));
    pose_intake(&mut app, BARREN_PEN, BARREN);
    pose_intake(&mut app, PART_PEN, gross_demand(PART_BIOMASS) / 2.0);
    spawn_band(
        &mut app,
        tile,
        vec![
            keeper_row(FED_PEN),
            keeper_row(BARREN_PEN),
            keeper_row(PART_PEN),
        ],
    );
    resolve_and_publish(&mut app);

    let fed = published_hay_need_with_nothing_carried_in(&app, FED_PEN);
    let barren = published_hay_need_with_nothing_carried_in(&app, BARREN_PEN);
    let part = published_hay_need_with_nothing_carried_in(&app, PART_PEN);
    println!("published need — fed {fed:.6}, barren {barren:.6}, part-covered {part:.6}");

    assert!(
        fed.abs() < EPSILON,
        "a footprint that covers the pen leaves nothing to grow (published {fed})"
    );
    assert!(
        (barren - gross_demand(BARREN_BIOMASS)).abs() < EPSILON,
        "a barren footprint leaves the WHOLE demand: published {barren} vs {}",
        gross_demand(BARREN_BIOMASS)
    );
    assert!(
        (part - gross_demand(PART_BIOMASS) / 2.0).abs() < EPSILON,
        "a half-grazed footprint leaves half the demand: published {part} vs {}",
        gross_demand(PART_BIOMASS) / 2.0
    );
    // The three are genuinely different readings, which is what makes the three-arm fixture worth
    // more than one arm run three times.
    assert!(
        part > 0.0 && barren > part,
        "the fixture must produce three distinct needs (barren {barren}, part {part}, fed {fed})"
    );
}

// --------------------------------------------------------------------------------------------
// 2. The band roll-up
// --------------------------------------------------------------------------------------------

/// ⛔ **THE BAND'S FIGURE IS THE SUM OF ITS PENS, AND THE SIM IS WHAT SUMS IT.**
///
/// Two pens of **different** sizes on one band, both on barren ground so each owes its whole demand.
/// The published band figure has to equal the two published pen figures added together — and must
/// equal **neither** of them, which is what a copy of one term would produce.
#[test]
fn the_bands_need_is_every_pen_it_keeps_added_up() {
    let mut app = a_world();
    let tile = pen_tile(&app);
    learn_foddering(&mut app);
    seat_pens(
        &mut app,
        tile,
        &[(BIG_PEN, BIG_BIOMASS), (SMALL_PEN, SMALL_BIOMASS)],
    );
    pose_intake(&mut app, BIG_PEN, BARREN);
    pose_intake(&mut app, SMALL_PEN, BARREN);
    let band = spawn_band(
        &mut app,
        tile,
        vec![keeper_row(BIG_PEN), keeper_row(SMALL_PEN)],
    );
    resolve_and_publish(&mut app);

    let big = published_hay_need_with_nothing_carried_in(&app, BIG_PEN);
    let small = published_hay_need_with_nothing_carried_in(&app, SMALL_PEN);
    let (band_need, _, _, _) = published_band_ledger(&app, band);
    println!("published need — big pen {big:.6}, small pen {small:.6}, band {band_need:.6}");

    assert!(
        big > 0.0 && small > 0.0 && (big - small).abs() > EPSILON,
        "the two pens must ask for materially different amounts (big {big}, small {small})"
    );
    assert!(
        (band_need - (big + small)).abs() < EPSILON,
        "the band owes the sum of its pens: published {band_need} vs {} + {}",
        big,
        small
    );
    assert!(
        band_need > big && band_need > small,
        "a sum is above either term — a total that copied one pen would fail here \
         (band {band_need}, big {big}, small {small})"
    );
}

/// **A BAND WITH NO PENS OWES NOTHING, AND ITS RUNWAY MEANS NOTHING.**
///
/// The need is `0` — not a stale figure from a pen it no longer keeps — and the runway is the
/// larder's own *nothing is draining* sentinel rather than some number of turns.
#[test]
fn a_band_with_no_pens_owes_no_hay_and_has_no_runway() {
    let mut app = a_world();
    let tile = pen_tile(&app);
    // A herd exists and is penned, but nobody keeps it: no Hunt row names it, so the band holds no
    // pen at all. (The band still holds a row, so it is the working cohort the reader finds.)
    seat_pens(&mut app, tile, &[(BARREN_PEN, BARREN_BIOMASS)]);
    pose_intake(&mut app, BARREN_PEN, BARREN);
    let band = spawn_band(&mut app, tile, Vec::new());
    stock_hay(&mut app, band, A_HAY_RESERVE);
    resolve_and_publish(&mut app);

    let (need, _, runway, store) = published_band_ledger(&app, band);
    println!("no pens — need {need:.6}, runway {runway}, store {store:.3}");
    assert!(
        need.abs() < EPSILON,
        "a band keeping no pen owes no hay (published {need})"
    );
    assert!(
        store > 0.0,
        "the fixture stocks hay, so the runway has something to divide"
    );
    assert_eq!(
        runway, NOT_FOOD_LIMITED_TURNS,
        "nothing is draining, so the runway is the larder's own ∞ sentinel and not a turn count"
    );
}

// --------------------------------------------------------------------------------------------
// 3. The Foddering gate moves the DRAW, never the NEED
// --------------------------------------------------------------------------------------------

/// ⛔ **A BAND THAT CANNOT HAY A HERD STILL PUBLISHES WHAT THE HERD IS SHORT.**
///
/// Without Foddering the pen draws nothing however full the store is — and the herd is starving for
/// exactly the same amount it would be if the band could act. Zeroing the need behind the capability
/// gate would hide the one case the player most needs to see, so the fixture asserts the **draw** at
/// zero beside the **need** at its full value: the gate really is shut, and the need crossed anyway.
#[test]
fn a_band_without_foddering_still_publishes_what_its_pen_needs() {
    let mut app = a_world();
    let tile = pen_tile(&app);
    // Deliberately NO `learn_foddering`.
    seat_pens(&mut app, tile, &[(BARREN_PEN, BARREN_BIOMASS)]);
    pose_intake(&mut app, BARREN_PEN, BARREN);
    let band = spawn_band(&mut app, tile, vec![keeper_row(BARREN_PEN)]);
    stock_hay(&mut app, band, A_HAY_RESERVE);
    resolve_and_publish(&mut app);

    let need = published_hay_need_with_nothing_carried_in(&app, BARREN_PEN);
    let draw = published_hay_draw(&app, BARREN_PEN);
    let (band_need, _, _, store) = published_band_ledger(&app, band);
    println!("un-foddered — need {need:.6}, draw {draw:.6}, band {band_need:.6}");

    assert!(
        draw.abs() < EPSILON,
        "the capability gate is shut: an un-foddered pen draws no hay (published {draw})"
    );
    assert!(
        store > 0.0,
        "and the store it could not draw from is genuinely stocked ({store})"
    );
    assert!(
        (need - gross_demand(BARREN_BIOMASS)).abs() < EPSILON,
        "the need is stated in full anyway: published {need} vs {}",
        gross_demand(BARREN_BIOMASS)
    );
    assert!(
        (band_need - need).abs() < EPSILON,
        "and it reaches the band roll-up too (band {band_need} vs pen {need})"
    );
}

// --------------------------------------------------------------------------------------------
// 4. The slow trap: a fixed footprint under a growing herd
// --------------------------------------------------------------------------------------------

/// ⛔ **A PEN THAT FEEDS ITSELF TODAY BECOMES HAY-DEPENDENT AS ITS HERD GROWS.**
///
/// A footprint's carrying capacity is fixed and a herd is not, so the need rises turn on turn with
/// nothing on the map having changed. The fixture holds the intake at exactly the same number across
/// two readings and grows the herd between them; the published need has to rise by the whole of the
/// demand's rise.
///
/// **It is not vacuous on a gross-demand field**, which would also rise — the third assertion
/// requires the need to stay *strictly under* the gross demand at both readings, so a field that
/// forgot the footprint fails here as well as in the coverage fixture.
#[test]
fn a_growing_herd_publishes_a_rising_need_on_a_fixed_footprint() {
    /// The land's ceiling: what this footprint hands over, whatever the herd's appetite is. It sits
    /// below the smaller herd's demand already, so the pen starts out part-covered rather than
    /// crossing from free to short mid-fixture.
    const FOOTPRINT_CEILING: f32 = 5.0;
    /// The herd before and after a season of breeding.
    const HERD_BEFORE: f32 = 100.0;
    const HERD_AFTER: f32 = 260.0;

    let mut app = a_world();
    let tile = pen_tile(&app);
    learn_foddering(&mut app);
    seat_pens(&mut app, tile, &[(PART_PEN, HERD_BEFORE)]);
    pose_intake(&mut app, PART_PEN, FOOTPRINT_CEILING);
    spawn_band(&mut app, tile, vec![keeper_row(PART_PEN)]);
    resolve_and_publish(&mut app);
    let before = published_hay_need_with_nothing_carried_in(&app, PART_PEN);

    // The herd grows; the land does not. The intake is re-posed to the SAME ceiling, which is the
    // whole claim: a fixed footprint under a bigger appetite.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == PART_PEN)
            .expect("the pen survives the turn");
        herd.biomass = HERD_AFTER;
    }
    pose_intake(&mut app, PART_PEN, FOOTPRINT_CEILING);
    resolve_and_publish(&mut app);
    let after = published_hay_need_with_nothing_carried_in(&app, PART_PEN);

    println!("growing herd — need before {before:.6}, after {after:.6}");
    assert!(
        after > before,
        "a bigger herd on the same land needs more hay (before {before}, after {after})"
    );
    assert!(
        (after - before - (gross_demand(HERD_AFTER) - gross_demand(HERD_BEFORE))).abs() < EPSILON,
        "and it rises by the whole of the demand's rise: {after} − {before} vs {} − {}",
        gross_demand(HERD_AFTER),
        gross_demand(HERD_BEFORE)
    );
    assert!(
        before < gross_demand(HERD_BEFORE) - EPSILON && after < gross_demand(HERD_AFTER) - EPSILON,
        "the land is still feeding part of the pen at both readings — a need equal to the gross \
         demand would mean the footprint was never subtracted (before {before}, after {after})"
    );
}

// --------------------------------------------------------------------------------------------
// 5. The runway, and the hay income under it
// --------------------------------------------------------------------------------------------

/// ⛔ **THE FODDER RUNWAY IS THE LARDER RUNWAY, IN THE OTHER CURRENCY.**
///
/// A band farming a real hay Field beside a pen small enough that the Field out-grows it: nothing is
/// draining, so the published runway is the **larder's own sentinel** — the same reading
/// `turnsOfFood` gives a band whose income beats its consumption, asserted as the constant rather
/// than as a hand-picked number, because the client must not need a second way to say *"turns of
/// buffer left"*.
///
/// **It is also what pins `fodderIncome` as a live field.** The Field's harvest is a real credit to
/// the band's `FODDER` store this turn, so the published income is required to be positive **and**
/// to equal the store it landed in — a published zero would fail both, and would then make the
/// sentinel arm above pass for the wrong reason.
#[test]
fn the_fodder_runway_reads_the_larders_own_no_drain_sentinel() {
    let mut app = a_world();
    // The band stands ON its hay Field, and its pen is corralled there too: the Field has to be in
    // work range and the pen inside the hunt leash for one band to hold both.
    let hay = a_hay_field(&mut app);
    learn_foddering(&mut app);
    seat_pens(&mut app, hay, &[(TINY_PEN, TINY_BIOMASS)]);
    pose_intake(&mut app, TINY_PEN, BARREN);
    let band = spawn_band(&mut app, hay, vec![forager_row(hay), keeper_row(TINY_PEN)]);
    resolve_and_publish(&mut app);

    let need = published_hay_need_with_nothing_carried_in(&app, TINY_PEN);
    let (band_need, income, runway, store) = published_band_ledger(&app, band);
    println!("hay Field — need {need:.6}, income {income:.6}, store {store:.6}, runway {runway}");

    assert!(
        need > 0.0 && (band_need - need).abs() < EPSILON,
        "the fixture pen genuinely owes hay (pen {need}, band {band_need})"
    );
    assert!(
        income > 0.0,
        "the fixture's hay Field must actually harvest fodder — a zero income would make the \
         sentinel below pass for the wrong reason (published {income})"
    );
    assert!(
        (income - store).abs() < EPSILON,
        "the published income is the hay that really landed in the store this turn: {income} vs \
         {store}"
    );
    assert!(
        income >= band_need,
        "the fixture requires the Field to out-grow the pen it feeds (income {income}, need \
         {band_need})"
    );
    assert_eq!(
        runway, NOT_FOOD_LIMITED_TURNS,
        "income that meets the need is NOT a runway of some number of turns — it is the larder's \
         own ∞ sentinel"
    );
}

/// **AND A BAND THAT IS GENUINELY SHORT GETS A NUMBER.** The twin of the arm above: with no Field
/// and a pen on barren ground the store is draining at the whole need, so the runway is the store
/// over that drain — finite, and strictly under the sentinel.
#[test]
fn a_band_short_of_hay_publishes_a_finite_runway() {
    let mut app = a_world();
    let tile = pen_tile(&app);
    learn_foddering(&mut app);
    seat_pens(&mut app, tile, &[(SMALL_PEN, SMALL_BIOMASS)]);
    pose_intake(&mut app, SMALL_PEN, BARREN);
    let band = spawn_band(&mut app, tile, vec![keeper_row(SMALL_PEN)]);
    stock_hay(&mut app, band, A_HAY_RESERVE);
    resolve_and_publish(&mut app);

    let (need, income, runway, store) = published_band_ledger(&app, band);
    println!("short band — need {need:.6}, income {income:.6}, store {store:.6}, runway {runway}");
    assert!(
        income.abs() < EPSILON && need > 0.0,
        "the fixture grows no hay and owes some (income {income}, need {need})"
    );
    assert!(
        runway < NOT_FOOD_LIMITED_TURNS,
        "a draining store has a real runway, not the ∞ sentinel (published {runway})"
    );
    assert!(
        (runway - store / (need - income)).abs() < EPSILON,
        "and it is the store over the net drain: published {runway} vs {}",
        store / (need - income)
    );
}

/// ⛔ **A BAND THAT CANNOT DRAW HAY IS NOT DRAINING ITS STORE.**
///
/// The runway says *turns until `fodderStore` empties*, so what it counts down is the **drain** —
/// and the pens' draw is gated on Foddering while the need is deliberately not. A band can hold hay
/// without Foddering: the harvest credit lifts on the **commitment** to a fodder crop, so a band that
/// starts a Cultivate on `hay_grass` banks hay turns before it can feed any of it out. Counting such
/// a store down against the ungated need published a few turns' runway for a store that never moved,
/// and seven turns later it still read full.
///
/// Two worlds, identical but for the knowledge. The un-foddered band draws nothing and reads the
/// larder's own **no-drain sentinel** — the existing ∞, not a second phrasing for *"cannot draw"* —
/// while its `fodderNeed` still states the gap in full, because the need is what carries the alarm.
/// The foddered twin, same pen and same store, gets a finite number off the same arithmetic.
#[test]
fn a_band_that_cannot_draw_hay_publishes_the_no_drain_sentinel() {
    // World A — no Foddering, a full store, a pen short its whole demand.
    let mut gated = a_world();
    let tile = pen_tile(&gated);
    // Deliberately NO `learn_foddering`.
    seat_pens(&mut gated, tile, &[(SMALL_PEN, SMALL_BIOMASS)]);
    pose_intake(&mut gated, SMALL_PEN, BARREN);
    let gated_band = spawn_band(&mut gated, tile, vec![keeper_row(SMALL_PEN)]);
    stock_hay(&mut gated, gated_band, A_HAY_RESERVE);
    resolve_and_publish(&mut gated);
    let gated_draw = published_hay_draw(&gated, SMALL_PEN);
    let (gated_need, gated_income, gated_runway, gated_store) =
        published_band_ledger(&gated, gated_band);

    // World B — the same band, one knowledge richer.
    let mut foddered = a_world();
    learn_foddering(&mut foddered);
    seat_pens(&mut foddered, tile, &[(SMALL_PEN, SMALL_BIOMASS)]);
    pose_intake(&mut foddered, SMALL_PEN, BARREN);
    let foddered_band = spawn_band(&mut foddered, tile, vec![keeper_row(SMALL_PEN)]);
    stock_hay(&mut foddered, foddered_band, A_HAY_RESERVE);
    resolve_and_publish(&mut foddered);
    let (foddered_need, _, foddered_runway, foddered_store) =
        published_band_ledger(&foddered, foddered_band);

    println!(
        "un-foddered — need {gated_need:.6}, draw {gated_draw:.6}, store {gated_store:.3}, \
         runway {gated_runway}; foddered — need {foddered_need:.6}, store {foddered_store:.3}, \
         runway {foddered_runway}"
    );

    assert!(
        gated_draw.abs() < EPSILON && gated_store > 0.0 && gated_income.abs() < EPSILON,
        "the un-foddered band holds hay it cannot touch and grows none (draw {gated_draw}, store \
         {gated_store}, income {gated_income})"
    );
    assert!(
        gated_need > 0.0 && (gated_need - foddered_need).abs() < EPSILON,
        "and its NEED is untouched by the gate — the same figure its foddered twin publishes \
         (un-foddered {gated_need}, foddered {foddered_need})"
    );
    assert_eq!(
        gated_runway, NOT_FOOD_LIMITED_TURNS,
        "a store nothing draws lasts forever: the runway is the larder's own ∞ sentinel, not the \
         {gated_need}/turn the band is short"
    );
    assert!(
        foddered_runway < NOT_FOOD_LIMITED_TURNS
            && (foddered_runway - foddered_store / foddered_need).abs() < EPSILON,
        "while the twin that CAN draw empties its store at the need: published {foddered_runway} \
         vs {} (store {foddered_store}, need {foddered_need})",
        foddered_store / foddered_need
    );
}

// --------------------------------------------------------------------------------------------
// 6. The ledger does not outlive the band
// --------------------------------------------------------------------------------------------

/// ⛔ **A BAND THAT LOSES ITS LAST WORKER PUBLISHES NO HAY LEDGER, NOT LAST TURN'S.**
///
/// `fodderNeed` and `fodderIncome` are plain accumulators summed as the labor pass walks a band's
/// rows and written at the foot of it — but a band with no working-age hands sheds **every** row and
/// leaves that pass early, never reaching the write. Left unzeroed they would republish the previous
/// turn's figures indefinitely, for pens the band no longer keeps and Fields it no longer works, and
/// the runway derived from them with it. (`foodIncome` has never had the defect: its container is
/// resized to the surviving assignments.)
///
/// The fixture builds a band with a real ledger — a hay Field it farms and a pen it keeps, so both
/// terms are genuinely non-zero — then takes its workers away and publishes again.
#[test]
fn a_band_that_loses_its_last_worker_publishes_no_hay_ledger() {
    let mut app = a_world();
    let hay = a_hay_field(&mut app);
    learn_foddering(&mut app);
    // The BIG pen deliberately: its demand has to out-run the Field's harvest, or the band's runway
    // is the no-drain sentinel while it is still alive and the reading below proves nothing.
    seat_pens(&mut app, hay, &[(BIG_PEN, BIG_BIOMASS)]);
    pose_intake(&mut app, BIG_PEN, BARREN);
    let band = spawn_band(&mut app, hay, vec![forager_row(hay), keeper_row(BIG_PEN)]);
    stock_hay(&mut app, band, A_HAY_RESERVE);
    resolve_and_publish(&mut app);

    let (need_alive, income_alive, runway_alive, _) = published_band_ledger(&app, band);
    assert!(
        need_alive > 0.0 && income_alive > 0.0 && runway_alive < NOT_FOOD_LIMITED_TURNS,
        "the fixture must publish a real ledger first, or its disappearance proves nothing (need \
         {need_alive}, income {income_alive}, runway {runway_alive})"
    );

    // The last working-age hand is gone. Nothing else about the band, its pen or its Field changes.
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(band)
            .expect("the fixture band exists");
        cohort.working = scalar_zero();
    }
    resolve_and_publish(&mut app);

    assert!(
        app.world
            .get::<LaborAllocation>(band)
            .expect("the fixture band keeps its allocation")
            .assignments
            .is_empty(),
        "the premise of this fixture is that the band sheds every row and leaves the pass early"
    );
    let (need_dead, income_dead, runway_dead, store_dead) = published_band_ledger(&app, band);
    println!(
        "worked -> unworked — need {need_alive:.6} -> {need_dead:.6}, income {income_alive:.6} -> \
         {income_dead:.6}, runway {runway_alive} -> {runway_dead} (store {store_dead:.3})"
    );

    assert!(
        need_dead.abs() < EPSILON,
        "a band keeping no pens owes no hay — it must not republish last turn's {need_alive} \
         (published {need_dead})"
    );
    assert!(
        income_dead.abs() < EPSILON,
        "and it grows none either — not last turn's {income_alive} (published {income_dead})"
    );
    assert!(
        store_dead > 0.0,
        "its store is still stocked, so the sentinel below is about the DRAIN and not an empty \
         store ({store_dead})"
    );
    assert_eq!(
        runway_dead, NOT_FOOD_LIMITED_TURNS,
        "and the runway that follows from them is the no-drain sentinel, not last turn's \
         {runway_alive}"
    );
}

// --------------------------------------------------------------------------------------------
// 7. The shortfall — how much MORE fodder the pen needs
// --------------------------------------------------------------------------------------------
//
// The pen's GAP is what the *land* leaves and `fodderDraw` is what the keeper actually carried in.
// What the player acts on is neither: it is what is **still missing**, so the sim differences the two
// and publishes `penFodderShortfall` — `max(0, gap − fodderDraw)` — and a pen row reads "40% pasture ·
// 7% fodder · needs 11.3 more/turn" without a client subtracting two figures sitting on the same
// line. It is the ONLY term of that subtraction on the wire (the gap's own `penHayNeed` is retired,
// unread), so every arm below differences against the **fixture's** gap rather than a second
// published field — which is what makes these assertions check the arithmetic instead of restating
// it. Every reading is off the encoded envelope, like the rest of this file.

/// The hay a fixture puts in a band's store when it wants a **part-served** pen: materially under
/// [`BARREN_BIOMASS`]'s whole demand, and materially different from the shortfall it leaves, so no
/// assertion here can pass on a copy of either term.
const A_SHORT_HAY_RESERVE: f32 = 4.0;

/// ⛔ **A PEN ITS OWN LAND FEEDS NEEDS NOTHING MORE.**
///
/// The covered pen publishes `0` — and the fixture keeps a **barren** pen beside it, in the same
/// band and the same turn, whose shortfall is genuinely positive. Without that second pen a
/// published-zero field would pass this arm for the wrong reason.
#[test]
fn a_pen_its_own_land_feeds_publishes_no_shortfall() {
    let mut app = a_world();
    let tile = pen_tile(&app);
    learn_foddering(&mut app);
    seat_pens(
        &mut app,
        tile,
        &[(FED_PEN, FED_BIOMASS), (BARREN_PEN, BARREN_BIOMASS)],
    );
    pose_intake(&mut app, FED_PEN, gross_demand(FED_BIOMASS));
    pose_intake(&mut app, BARREN_PEN, BARREN);
    let band = spawn_band(
        &mut app,
        tile,
        vec![keeper_row(FED_PEN), keeper_row(BARREN_PEN)],
    );
    stock_hay(&mut app, band, A_SHORT_HAY_RESERVE);
    resolve_and_publish(&mut app);

    let covered = published_hay_shortfall(&app, FED_PEN);
    let covered_draw = published_hay_draw(&app, FED_PEN);
    let barren = published_hay_shortfall(&app, BARREN_PEN);
    println!(
        "covered pen — draw {covered_draw:.6}, shortfall {covered:.6}; barren pen {barren:.6}"
    );

    assert!(
        covered.abs() < EPSILON && covered_draw.abs() < EPSILON,
        "a pen its footprint covers is short nothing — and asked for nothing, so the zero is the \
         gap and not a draw that closed it (draw {covered_draw}, shortfall {covered})"
    );
    assert!(
        barren > 0.0,
        "the same turn must publish a real shortfall somewhere, or the zero above proves nothing \
         (barren pen published {barren})"
    );
}

/// ⛔ **A PART-SUPPLIED PEN PUBLISHES EXACTLY WHAT IS STILL MISSING.**
///
/// A barren pen with a keeper who *can* hay it and a store that covers only part of the gap: the
/// need, the draw and the shortfall are all non-zero and all **different from each other**, so a
/// field wired to either term — or one that dropped the subtraction — fails here rather than passing
/// on a copy.
#[test]
fn a_part_supplied_pen_publishes_what_is_still_missing() {
    let mut app = a_world();
    let tile = pen_tile(&app);
    learn_foddering(&mut app);
    seat_pens(&mut app, tile, &[(BARREN_PEN, BARREN_BIOMASS)]);
    pose_intake(&mut app, BARREN_PEN, BARREN);
    let band = spawn_band(&mut app, tile, vec![keeper_row(BARREN_PEN)]);
    stock_hay(&mut app, band, A_SHORT_HAY_RESERVE);
    resolve_and_publish(&mut app);

    // The gap the pen's barren footprint leaves, from the fixture's own arithmetic — the term the
    // published shortfall is differenced from, and deliberately not a second reading off the wire.
    let gap = gross_demand(BARREN_BIOMASS);
    let draw = published_hay_draw(&app, BARREN_PEN);
    let short = published_hay_shortfall(&app, BARREN_PEN);
    println!("part-supplied — gap {gap:.6}, draw {draw:.6}, shortfall {short:.6}");

    assert!(
        (draw - A_SHORT_HAY_RESERVE).abs() < EPSILON,
        "the keeper carried in every unit the store held: published {draw} vs \
         {A_SHORT_HAY_RESERVE}"
    );
    assert!(
        (short - (gap - draw)).abs() < EPSILON,
        "the shortfall is the gap less the draw: published {short} vs {gap} − {draw}"
    );
    assert!(
        gap > 0.0
            && draw > 0.0
            && short > 0.0
            && (short - gap).abs() > EPSILON
            && (short - draw).abs() > EPSILON,
        "all three terms must be non-zero and mutually distinct, or a copy of one of them would \
         pass (gap {gap}, draw {draw}, shortfall {short})"
    );
    // The band roll-up is the gap, un-differenced — the one place the gap itself is still published,
    // and the reason the sim keeps computing it.
    let (band_need, _, _, _) = published_band_ledger(&app, band);
    assert!(
        (band_need - gap).abs() < EPSILON,
        "and the band's own need is the GAP, not the shortfall: published {band_need} vs {gap} \
         (shortfall {short})"
    );
}

/// ⛔ **A PEN NOTHING IS CARRIED INTO IS SHORT ITS WHOLE NEED — AND KNOWING NOTHING READS THE SAME.**
///
/// Two worlds, one figure. In the first the band **can** hay its pen and simply has no hay; in the
/// second the store is full and the band has never learned **Foddering**, so the capability gate
/// shuts the draw. The remedy differs — buy hay, or learn to make it — but the herd is short exactly
/// the same amount, and the readout says so in both.
///
/// A shortfall zeroed behind the Foddering gate would read `0` in the second world and hide the one
/// case the field exists for: the herd is dying and no amount of hay is the answer.
#[test]
fn a_pen_with_no_hay_is_short_its_whole_need_gate_shut_or_store_empty() {
    let whole_need = gross_demand(BARREN_BIOMASS);

    // World A — Foddering known, nothing in the store.
    let mut empty_store = a_world();
    let tile = pen_tile(&empty_store);
    learn_foddering(&mut empty_store);
    seat_pens(&mut empty_store, tile, &[(BARREN_PEN, BARREN_BIOMASS)]);
    pose_intake(&mut empty_store, BARREN_PEN, BARREN);
    spawn_band(&mut empty_store, tile, vec![keeper_row(BARREN_PEN)]);
    resolve_and_publish(&mut empty_store);
    let starved_short = published_hay_shortfall(&empty_store, BARREN_PEN);
    let starved_draw = published_hay_draw(&empty_store, BARREN_PEN);

    // World B — a stocked store the band has no idea what to do with.
    let mut no_knowledge = a_world();
    // Deliberately NO `learn_foddering`.
    seat_pens(&mut no_knowledge, tile, &[(BARREN_PEN, BARREN_BIOMASS)]);
    pose_intake(&mut no_knowledge, BARREN_PEN, BARREN);
    let band = spawn_band(&mut no_knowledge, tile, vec![keeper_row(BARREN_PEN)]);
    stock_hay(&mut no_knowledge, band, A_HAY_RESERVE);
    resolve_and_publish(&mut no_knowledge);
    let ungated_short = published_hay_shortfall(&no_knowledge, BARREN_PEN);
    let ungated_draw = published_hay_draw(&no_knowledge, BARREN_PEN);
    let (_, _, _, ungated_store) = published_band_ledger(&no_knowledge, band);

    println!(
        "no hay — empty store: draw {starved_draw:.6} shortfall {starved_short:.6}; \
         no Foddering: draw {ungated_draw:.6} shortfall {ungated_short:.6} store {ungated_store:.3}"
    );

    assert!(
        starved_draw.abs() < EPSILON && ungated_draw.abs() < EPSILON,
        "neither pen is carried anything (empty store {starved_draw}, un-foddered {ungated_draw})"
    );
    assert!(
        ungated_store > 0.0,
        "and the un-foddered band's store is genuinely full, so its zero draw is the GATE and not \
         an empty larder ({ungated_store})"
    );
    assert!(
        (starved_short - whole_need).abs() < EPSILON,
        "a pen carried nothing is short its whole need: published {starved_short} vs {whole_need}"
    );
    assert!(
        (ungated_short - whole_need).abs() < EPSILON,
        "and the shortfall crosses the Foddering gate exactly as the need does: published \
         {ungated_short} vs {whole_need}"
    );
}

/// ⛔ **THE SHORTFALL NEVER GOES NEGATIVE.**
///
/// A pen cannot be settled more hay than its own gap, so an over-supply is not reachable through the
/// allocation — but the draw is spent through the store's **fixed-point** `Scalar`, which rounds to
/// the nearest millionth and can hand a fully-served pen a take a hair *above* the `f32` need it was
/// settled from. This fixture builds exactly that pen: a biomass whose gross demand does not land on
/// a `Scalar` step, on barren ground with hay to spare.
///
/// The first assertion is what keeps it honest — it requires the draw to genuinely **exceed** the
/// need, so the clamp is really under test rather than the arm passing because nothing overshot.
#[test]
fn an_over_supplied_pen_floors_at_zero_instead_of_publishing_a_negative() {
    /// A biomass whose gross demand (`FODDER_RATE ×` this) is *not* representable on a `Scalar`
    /// step, so quantising the settled hay rounds the draw a fraction of a unit above the need.
    const UNQUANTISED_BIOMASS: f32 = 10.025;

    let mut app = a_world();
    let tile = pen_tile(&app);
    learn_foddering(&mut app);
    seat_pens(&mut app, tile, &[(PART_PEN, UNQUANTISED_BIOMASS)]);
    pose_intake(&mut app, PART_PEN, BARREN);
    let band = spawn_band(&mut app, tile, vec![keeper_row(PART_PEN)]);
    stock_hay(&mut app, band, A_HAY_RESERVE);
    resolve_and_publish(&mut app);

    // The gap, from the fixture's own arithmetic — the same expression the sim settles the draw
    // from, so a draw above it is the quantisation and nothing else.
    let gap = gross_demand(UNQUANTISED_BIOMASS);
    let draw = published_hay_draw(&app, PART_PEN);
    let short = published_hay_shortfall(&app, PART_PEN);
    println!(
        "over-supplied — gap {gap:.9}, draw {draw:.9}, difference {:.3e}",
        draw - gap
    );

    assert!(
        draw > gap,
        "the fixture must really over-serve its pen, or the clamp below is not under test \
         (gap {gap}, draw {draw})"
    );
    assert!(
        short >= 0.0,
        "a shortfall is never negative — an unclamped difference would publish {} here",
        gap - draw
    );
    assert!(
        short.abs() < EPSILON,
        "and a pen carried everything it asked for is short nothing (published {short})"
    );
}

/// ⛔ **THE SHORTFALL AND THE GAP IT IS TAKEN FROM DESCRIBE THE SAME TURN.**
///
/// Both are struck by the corral arm on one pass, so they cannot drift apart. The fixture grows the
/// herd between two readings with the land and the hay on hand held *identical* — the draw is
/// re-stocked to the same figure — and requires the published shortfall to rise by the whole of the
/// demand's rise, which only this turn's gap can deliver.
///
/// A shortfall carried over from a previous turn's gap, or differenced against a draw from another
/// pass, breaks it.
#[test]
fn the_shortfall_and_the_need_move_together_on_one_turn() {
    /// The herd before and after a season of breeding, on the same land and the same hay.
    const HERD_BEFORE: f32 = 80.0;
    const HERD_AFTER: f32 = 300.0;

    let mut app = a_world();
    let tile = pen_tile(&app);
    learn_foddering(&mut app);
    seat_pens(&mut app, tile, &[(PART_PEN, HERD_BEFORE)]);
    pose_intake(&mut app, PART_PEN, BARREN);
    let band = spawn_band(&mut app, tile, vec![keeper_row(PART_PEN)]);
    stock_hay(&mut app, band, A_SHORT_HAY_RESERVE);
    resolve_and_publish(&mut app);
    let draw_before = published_hay_draw(&app, PART_PEN);
    let short_before = published_hay_shortfall(&app, PART_PEN);

    // The herd grows; the land grows nothing and the keeper is handed the same hay again.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == PART_PEN)
            .expect("the pen survives the turn");
        herd.biomass = HERD_AFTER;
    }
    pose_intake(&mut app, PART_PEN, BARREN);
    stock_hay(&mut app, band, A_SHORT_HAY_RESERVE);
    resolve_and_publish(&mut app);
    let draw_after = published_hay_draw(&app, PART_PEN);
    let short_after = published_hay_shortfall(&app, PART_PEN);

    // Both pens stand on barren ground, so each turn's gap is that turn's whole gross demand — the
    // fixture's own figure, which is what the published shortfall has to be differenced from.
    let gap_before = gross_demand(HERD_BEFORE);
    let gap_after = gross_demand(HERD_AFTER);
    println!(
        "growing herd — before: gap {gap_before:.6} draw {draw_before:.6} short \
         {short_before:.6}; after: gap {gap_after:.6} draw {draw_after:.6} short {short_after:.6}"
    );

    let demand_rise = gap_after - gap_before;
    assert!(
        (draw_before - draw_after).abs() < EPSILON && draw_before > 0.0,
        "the hay on hand is the same at both readings, and it is not zero (before {draw_before}, \
         after {draw_after})"
    );
    assert!(
        (short_after - short_before - demand_rise).abs() < EPSILON,
        "the shortfall rises by the whole of the demand's rise, because the gap did and the draw \
         did not: {short_after} − {short_before} vs {demand_rise}"
    );
    assert!(
        (short_before - (gap_before - draw_before)).abs() < EPSILON
            && (short_after - (gap_after - draw_after)).abs() < EPSILON,
        "and at both readings it is this turn's gap less this turn's draw (before {short_before} \
         vs {gap_before} − {draw_before}, after {short_after} vs {gap_after} − {draw_after})"
    );
    assert!(
        (short_before - gap_before).abs() > EPSILON,
        "the pen is part-supplied at the first reading, so a shortfall that merely copied the gap \
         would fail (short {short_before}, gap {gap_before})"
    );
}

// --------------------------------------------------------------------------------------------
// Shared fixture parts that need the map
// --------------------------------------------------------------------------------------------

/// Stock a band's `FODDER` store — the buffer a runway divides.
fn stock_hay(app: &mut App, band: Entity, hay: f32) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the fixture band exists");
    cohort.stores.set(FODDER, scalar_from_f32(hay));
}

/// **A tended `hay_grass` Field, and the tile its keeper stands on.** The richest in-season patch
/// whose own basket grows hay, committed to it and put on its escapement floor — the fixture idiom
/// `forage_tended_vector.rs` uses, so the harvest here is the one the sim really pays.
fn a_hay_field(app: &mut App) -> UVec2 {
    let coord = richest_tile_growing(app, "hay_grass");
    {
        let ladder = core_sim::LadderConfig::builtin();
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patch_mut(coord)
            .expect("the resolved tile carries a patch");
        patch.species = Some("hay_grass".to_string());
        patch.complete_cultivation(FACTION, &ladder);
        patch.biomass = patch.carrying_capacity;
    }
    coord
}

/// One gatherer row on `patch`, at the Sustain floor.
fn forager_row(patch: UVec2) -> LaborAssignment {
    LaborAssignment {
        target: LaborTarget::Forage {
            tile: patch,
            floor: SUSTAIN,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        },
        workers: KEEPERS,
        kit: None,
        priority: SourcePriority::default(),
        upkeep_kit: None,
    }
}

/// The richest in-season patch whose realized basket grows `species` — resolved off the map rather
/// than named, the same seam `forage_tended_vector.rs` resolves its fixtures through.
fn richest_tile_growing(app: &mut App, species: &str) -> UVec2 {
    use core_sim::{tile_flora_composition, FoodModuleTag, Tile};

    let labor = app.world.resource::<LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
    let registry = app.world.resource::<ForageRegistry>();
    let coord = query
        .iter(&app.world)
        .filter(|(_, module)| module.seasonal_weight > 0.0)
        .filter(|(tile, _)| {
            tile_flora_composition(&flora, &labor.forage, tile, map_seed)
                .iter()
                .any(|entry| entry.species == species)
        })
        .filter_map(|(tile, _)| registry.patch(tile.position))
        .max_by(|a, b| {
            a.carrying_capacity
                .total_cmp(&b.carrying_capacity)
                .then_with(|| b.tile.y.cmp(&a.tile.y))
                .then_with(|| b.tile.x.cmp(&a.tile.x))
        })
        .unwrap_or_else(|| {
            panic!("the pinned map must carry an in-season patch whose basket grows {species}")
        })
        .tile;
    coord
}

// --------------------------------------------------------------------------------------------
// 7. The runway counts the hay that CROSSES between bands (issue #548)
// --------------------------------------------------------------------------------------------

/// Hay a neighbour hands over each turn, **less** than the pen's own draw — so the store still
/// empties and the runway is a real number that simply got longer.
const A_NEIGHBOURS_SHARE: f32 = 3.0;
/// And a share **larger** than the draw, which is the case the whole fix is about: the store is
/// rising, so there is no turn on which it empties.
const A_GENEROUS_NEIGHBOURS_SHARE: f32 = 20.0;

/// **Hay arriving from a band standing alongside**, exactly as `balance_supply_networks` delivers
/// it: the store rises AND the crossing is booked on the fodder ledger's `local` arm. Booking one
/// without the other is precisely the state that made the runway wrong.
fn receive_hay(app: &mut App, band: Entity, amount: f32) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the fixture band exists")
        .stores
        .add(FODDER, scalar_from_f32(amount));
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the fixture band has an allocation")
        .last_fodder_transfers
        .credit(TransferLink::Local, amount);
    publish_the_turns_transfers(app);
}

/// The mirror: hay pooled AWAY to a shorter neighbour.
fn send_hay(app: &mut App, band: Entity, amount: f32) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the fixture band exists")
        .stores
        .take(FODDER, scalar_from_f32(amount));
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the fixture band has an allocation")
        .last_fodder_transfers
        .debit(TransferLink::Local, amount);
    publish_the_turns_transfers(app);
}

/// The turn-path copy that puts the accumulated crossings onto the cohort as per-turn state. Run
/// explicitly because these fixtures resolve a labor pass rather than a whole turn — and because the
/// runway deliberately reads the per-turn copy, which is what lets it survive a recapture.
fn publish_the_turns_transfers(app: &mut App) {
    app.world.run_system_once(core_sim::publish_turn_transfers);
}

/// A band short of hay: a drylot pen, no Field, a stocked store. The shape
/// [`a_band_short_of_hay_publishes_a_finite_runway`] pins, reused so the three arms below differ in
/// exactly one thing — what crossed to or from a neighbour.
fn a_band_draining_its_hay(app: &mut App) -> Entity {
    let tile = pen_tile(app);
    learn_foddering(app);
    seat_pens(app, tile, &[(SMALL_PEN, SMALL_BIOMASS)]);
    pose_intake(app, SMALL_PEN, BARREN);
    let band = spawn_band(app, tile, vec![keeper_row(SMALL_PEN)]);
    stock_hay(app, band, A_HAY_RESERVE);
    band
}

/// ⛔ **A POOLED STORE IS NOT A DRAINING STORE, AND THE RUNWAY HAS TO SAY WHICH.**
///
/// Hay pools between linked camps every turn — `balance_supply_networks` walks a band's whole store
/// and `FODDER` is an ordinary key in it — but the published runway was `store ÷ (pens' draw −
/// Fields' harvest)` and knew nothing about the crossing. So a band on the receiving end watched its
/// hay RISE under a runway counting down, and the band feeding it got a runway that was too
/// generous by exactly what it gave away.
///
/// Three worlds, identical but for what crossed: one on its own, one topped up by a neighbour, one
/// pooling hay away. The middle arm is pinned to the closed form `store ÷ (need − income − local
/// net)` so it cannot pass on a term that merely moved in the right direction.
///
/// **It is the `local` arm that counts, and only that arm** — pooling is a rate two camps keep up
/// every turn. Its `route` twin is an event and is not a term at all; see
/// [`a_route_crossing_is_an_event_and_never_moves_the_runway`].
#[test]
fn the_fodder_runway_counts_the_hay_that_crossed() {
    let mut alone = a_world();
    let alone_band = a_band_draining_its_hay(&mut alone);
    resolve_and_publish(&mut alone);
    let (alone_need, alone_income, alone_runway, _) = published_band_ledger(&alone, alone_band);

    let mut topped_up = a_world();
    let topped_band = a_band_draining_its_hay(&mut topped_up);
    receive_hay(&mut topped_up, topped_band, A_NEIGHBOURS_SHARE);
    resolve_and_publish(&mut topped_up);
    let (topped_need, topped_income, topped_runway, topped_store) =
        published_band_ledger(&topped_up, topped_band);

    let mut pooled_away = a_world();
    let pooled_band = a_band_draining_its_hay(&mut pooled_away);
    send_hay(&mut pooled_away, pooled_band, A_NEIGHBOURS_SHARE);
    resolve_and_publish(&mut pooled_away);
    let (_, _, pooled_runway, _) = published_band_ledger(&pooled_away, pooled_band);

    assert!(
        alone_need > 0.0 && alone_income.abs() < EPSILON && alone_runway < NOT_FOOD_LIMITED_TURNS,
        "liveness: the band on its own is genuinely draining (need {alone_need}, income \
         {alone_income}, runway {alone_runway})"
    );
    assert!(
        (topped_need - alone_need).abs() < EPSILON,
        "the pens are identical, so the NEED must not move — {topped_need} vs {alone_need}"
    );
    assert!(
        topped_runway > alone_runway,
        "hay arriving every turn makes the store last LONGER: {topped_runway} vs {alone_runway}"
    );
    assert!(
        pooled_runway < alone_runway,
        "and hay pooled away makes it run out SOONER: {pooled_runway} vs {alone_runway}"
    );

    let expected = topped_store / (topped_need - topped_income - A_NEIGHBOURS_SHARE);
    assert!(
        (topped_runway - expected).abs() < EPSILON,
        "the runway is the store over the net of every term, the crossing included: published \
         {topped_runway} vs {expected}"
    );
}

/// ⛔ **A STORE THAT IS RISING PUBLISHES NO COUNTDOWN.** The limit case of the arm above, and the
/// reading a player could see was wrong without doing any arithmetic: a neighbour hands over more
/// hay than the pens draw, the store climbs turn on turn, and the row still said *N turns left*.
///
/// It resolves to the larder's own no-drain sentinel — the same ∞ a band with no pens publishes, and
/// deliberately not a second phrasing for *"a neighbour is covering me"*.
#[test]
fn a_hay_store_a_neighbour_is_filling_publishes_the_no_drain_sentinel() {
    let mut app = a_world();
    let band = a_band_draining_its_hay(&mut app);
    receive_hay(&mut app, band, A_GENEROUS_NEIGHBOURS_SHARE);
    resolve_and_publish(&mut app);

    let (need, income, runway, store) = published_band_ledger(&app, band);
    assert!(
        need > 0.0 && need < A_GENEROUS_NEIGHBOURS_SHARE,
        "the fixture's pens really do owe hay, and less than what arrives (need {need})"
    );
    assert!(
        income.abs() < EPSILON,
        "and the band grows none of its own, so the crossing is the only credit ({income})"
    );
    assert_eq!(
        runway, NOT_FOOD_LIMITED_TURNS,
        "nothing is emptying a store of {store} that gains {A_GENEROUS_NEIGHBOURS_SHARE} a turn \
         against a draw of {need}, so the runway is the larder's own ∞ sentinel"
    );
}

/// **A shipment's worth of hay, staged onto the `route` arm.** No path in the sim can produce one
/// today — `ResolvedShipment` refuses any cargo item that is not food or a material — so the arm is
/// written directly, which is the only way to assert the basis *before* the day it starts to matter.
/// The store rises with it, exactly as a real delivery would leave it.
fn take_delivery_of_hay(app: &mut App, band: Entity, amount: f32) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the fixture band exists")
        .stores
        .add(FODDER, scalar_from_f32(amount));
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the fixture band has an allocation")
        .last_fodder_transfers
        .credit(TransferLink::Route, amount);
    publish_the_turns_transfers(app);
}

/// ⛔ **A ROUTE CROSSING IS AN EVENT, SO IT IS NOT A TERM OF THE RUNWAY** — the other half of the
/// rule its `local` twin above states. A neighbour standing alongside pools **every turn** for as
/// long as both camps stay there, which is a rate a forecast should project; a shipment lands
/// **once**, and reading one delivery as a standing per-turn credit is the mistake arc #527 refused
/// on the food side.
///
/// **This asserts a basis that no shipped path can exercise yet**, because a shipment's manifest
/// refuses fodder — which is exactly why it is written now: the day hay gains a shipping currency,
/// a runway wired to `received() − sent()` would silently start annualising deliveries, and nobody
/// would be looking. The staged figure is large enough that a runway counting it would reach the
/// no-drain sentinel, so a regression cannot hide inside the epsilon.
///
/// The store still **rises** by the delivery, as a real arrival would leave it, and the runway
/// therefore gets longer — but only by the buffer, never by a projected rate. Pinned as the closed
/// form against the band's own published store.
#[test]
fn a_route_crossing_is_an_event_and_never_moves_the_runway() {
    let mut app = a_world();
    let band = a_band_draining_its_hay(&mut app);
    take_delivery_of_hay(&mut app, band, A_GENEROUS_NEIGHBOURS_SHARE);
    resolve_and_publish(&mut app);
    let (need, income, runway, store) = published_band_ledger(&app, band);

    assert!(
        need > 0.0 && income.abs() < EPSILON,
        "liveness: the band owes hay and grows none of its own (need {need}, income {income})"
    );
    assert!(
        runway < NOT_FOOD_LIMITED_TURNS,
        "a delivery is a one-off, so the store is still draining and the runway is a real number, \
         not the ∞ sentinel a projected {A_GENEROUS_NEIGHBOURS_SHARE}/turn would have produced \
         (published {runway})"
    );
    assert!(
        (runway - store / (need - income)).abs() < EPSILON,
        "the route arm is not a term: the runway is the (larger) store over the unchanged net \
         drain — published {runway} vs {}",
        store / (need - income)
    );
}
