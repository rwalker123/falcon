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
//! Every assertion is off the **encoded envelope** — a field that never reached the codec still
//! satisfies an in-process one.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use core_sim::{
    advance_labor_allocation, build_test_app, recapture_snapshot_in_place, FactionId, Herd,
    HerdRegistry, SizeClass, SnapshotHistory,
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
