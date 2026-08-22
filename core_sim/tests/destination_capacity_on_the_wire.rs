//! **A BUILD SAYS WHERE IT IS TAKING THE LAND, NOT ONLY THAT IT IS MOVING.**
//!
//! A take is held above `floor_fraction × K`, and a rung **raises `K`** — the plant web through
//! `cultivation.field_capacity_gain`, the animal web through the per-species `pastoral_density` /
//! `pen_density`, all of them interpolated on the source's ladder position. So while a build runs the
//! escapement floor climbs every turn underneath the player and their take falls. With only the live
//! `carryingCapacity` on the wire the client can mark that the floor is *moving* and nothing more, so
//! a reduced take reads as the source being poor rather than as the player's own investment arriving.
//!
//! `buildDestinationCapacity` is where it is going: **the capacity the source will have at the rung
//! its build is heading for**. Not next turn's — next turn's position depends on work nobody has
//! banked yet — but the destination's, which is exact, because the queue entry already names the rung
//! its climb ends on.
//!
//! # WHAT THESE TESTS PIN
//!
//! 1. **The advertised destination is the delivered capacity**, on both webs, driven through a **real
//!    build** run to its destination rather than by calling the formula twice. That is what makes the
//!    field a promise instead of an estimate.
//! 2. **A source with no build in flight publishes the does-not-apply reading**, and it is
//!    distinguishable from a real capacity of zero — asserted against a source that has a build *and*
//!    a genuinely zero destination capacity, in the same test, so the two readings are compared
//!    rather than merely described.
//! 3. **The advertised figure is strictly above the live one while a capacity-raising build runs** —
//!    with the rung's own gain asserted as a **precondition**, because a `Cultivate` raises no `K`
//!    and a fixture built on one would compare two equal numbers and pass with the field wired to
//!    anything at all.
//! 4. **A `Corral` quotes the fenced land, not the roam range with a density on it** — the one leg
//!    on which the advertised figure sits *below* the live one, and the arm that pins the footprint
//!    half of the reading.
//!
//! **Read off the ENCODED buffer**, the discipline `build_turns_on_the_wire.rs` follows: a field can
//! be right in the capture and wrong in the envelope, and the schema/codec path is what a client
//! actually sees.
//!
//! **The land is held still in both build fixtures, deliberately.** The destination figure is struck
//! over the land as it stands *today*, exactly as the live capacity is — the rung moves, the land does
//! not — so a fixture whose ground drifted underneath it would be measuring the weather rather than
//! the promise. Each fixture therefore pins the thing its web's capacity is summed from (a tile's own
//! terrain is already fixed; a herd's range is held at full graze and the herd is held on one hex).

use bevy::app::App;
use bevy::math::UVec2;

use core_sim::{
    build_test_app, recapture_snapshot_in_place, run_turn, scalar_from_f32, scalar_one,
    scalar_zero, FactionId, FaunaConfigHandle, ForageRegistry, GenerationId, GrazeRegistry,
    HerdRegistry, Improvement, LaborAllocation, LaborAssignment, LaborTarget, LadderConfigHandle,
    LocalStore, MoraleCause, PopulationCohort, ResidentBand, RungKey, SnapshotHistory,
    StartingUnit, TakeSelection, TileRegistry, DEFAULT_ESCAPEMENT_FLOOR,
};
use sim_schema::NO_BUILD_DESTINATION_CAPACITY;

/// f32 slack for two readings that are the **same expression** evaluated twice — they differ only by
/// the order the multiplications landed in, never by a term.
const SAME_NUMBER: f32 = 1e-3;

/// **Stock well above the escapement floor**, so the crew is genuinely working the source and the
/// rung's own gate is about the staffing rather than about an empty patch.
const STOCKED_STANDING_CROP: f32 = 0.8;

/// A build pool big enough to lay a rung's work units in a handful of turns — these tests are about
/// the number a build advertises, not about its pace.
const A_LARGE_BUILD_POOL: u32 = 30;

/// One gatherer beside the build, so the patch is a worked source.
const A_GATHERER: u32 = 1;

/// **Food enough that nobody in the fixture band goes hungry** — the fixtures run longer than a
/// starting larder lasts, and a famine would trim the very crews under measurement.
const A_FULL_LARDER: f32 = 10_000.0;

/// **A band with room for every crew it staffs**, restated each turn for the same reason.
const A_BAND_THAT_AFFORDS_ITS_CREWS: u32 = 40;

/// **A crew that will not finish a plant rung in one turn** — the arm that reads a *standing* entry
/// needs the entry to still be standing when the row is captured.
const A_SLOW_BUILD_POOL: u32 = 2;

/// **A build crew that takes several turns over the animal rung**, so the advertised figure is read
/// on more than one frame and its *stability* is actually asserted. A pool that landed a `Tame` in
/// one turn would leave that assertion dead.
const A_STEADY_BUILD_POOL: u32 = 8;

/// The turns a fixture waits for its build to land before calling it stuck. Generous: the point of
/// the bound is to fail with a message rather than to hang.
const BUILD_HORIZON: u32 = 60;

// ---------------------------------------------------------------------------------------------
// The plant web
// ---------------------------------------------------------------------------------------------

/// **A watered gathering site whose basket can climb to `plant:field`** — every conjunct of the Sow
/// gate but the staffing, so a refusal cannot be mistaken for the number under test. The same scan
/// `build_turns_on_the_wire.rs` runs, in a totally-ordered `(y, x)` pass rather than map iteration
/// order.
fn a_sowable_site(app: &mut App) -> UVec2 {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let config = app.world.resource::<core_sim::SimulationConfig>();
    let map_seed = config.map_seed;
    let wrap = config.map_topology.wrap_horizontal;
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let tiles: std::collections::HashMap<UVec2, core_sim::Tile> = {
        let mut query = app.world.query::<&core_sim::Tile>();
        query
            .iter(&app.world)
            .map(|tile| (tile.position, tile.clone()))
            .collect()
    };
    for y in 0..height {
        for x in 0..width {
            let coord = UVec2::new(x, y);
            let Some(ground) = tiles.get(&coord) else {
                continue;
            };
            if app
                .world
                .resource::<ForageRegistry>()
                .patch(coord)
                .is_none()
            {
                continue;
            }
            let fresh_water =
                core_sim::tile_is_fresh_watered(ground, width, height, wrap, |neighbor| {
                    tiles.get(&neighbor).map(|tile| tile.terrain_tags)
                });
            let refusal = core_sim::rung_site_refusal(
                ladder.rung(RungKey::PlantField),
                ground,
                &labor.forage,
                app.world
                    .resource::<core_sim::FoodSiteRegistry>()
                    .is_site(coord),
                fresh_water,
            );
            if refusal.is_some() {
                continue;
            }
            let composition =
                core_sim::tile_flora_composition(&flora, &labor.forage, ground, map_seed);
            if core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantField)
                .is_some()
            {
                return coord;
            }
        }
    }
    panic!("the shipped map must carry sowable ground — rung 3 is unreachable without it");
}

/// A headless world whose `source` tile carries a **tended** patch with a staffed `Sow` queued on it:
/// the position is seated at the Field rung's own base, so the whole of
/// `cultivation.field_capacity_gain` is still ahead of it and the advertised figure has somewhere to
/// travel from.
fn world_with_a_sow_in_flight() -> (App, UVec2) {
    // At the **foot of the Field rung** — the Cultivate leg is paid, so the entry's remaining climb
    // is exactly the one rung that raises `K`, and the whole gain is still ahead of it.
    let seat =
        |ladder: &core_sim::LadderConfig| core_sim::plant_rung_span(RungKey::PlantField, ladder).0;
    world_with_a_plant_build(
        Improvement::Sow,
        RungKey::PlantField,
        A_LARGE_BUILD_POOL,
        &seat,
    )
}

/// [`world_with_a_sow_in_flight`]'s rung-2 twin: the same ground, a meter **half-way up the tended
/// rung**, and a `Cultivate` in the queue — a destination the patch has not arrived at, so the entry
/// stands and the row quotes it.
fn world_with_a_cultivate_in_flight() -> (App, UVec2) {
    let seat = |ladder: &core_sim::LadderConfig| {
        let (base, width) = core_sim::plant_rung_span(RungKey::PlantTended, ladder);
        base + width * HALF_BUILT
    };
    // **A crew small enough that the rung does NOT land in the fixture's one turn** — the entry
    // has to still be standing when the row is read, or the arm measures the unqueued case.
    world_with_a_plant_build(
        Improvement::Cultivate,
        RungKey::PlantTended,
        A_SLOW_BUILD_POOL,
        &seat,
    )
}

/// **Half-way up the rung under measurement** — genuinely mid-build, so the entry is a running climb
/// rather than a projection.
const HALF_BUILT: f32 = 0.5;

/// The shared plant fixture: a staffed build on sowable ground, seated wherever `seat` says and
/// committed to a crop that can climb `crop_rung`.
fn world_with_a_plant_build(
    declared: Improvement,
    crop_rung: RungKey,
    builders: u32,
    seat: &dyn Fn(&core_sim::LadderConfig) -> f32,
) -> (App, UVec2) {
    let mut app = build_test_app();
    app.update();
    let source = a_sowable_site(&mut app);
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(source.x, source.y)
        .expect("the fixture tile resolves");
    for knowledge in [
        core_sim::CULTIVATION_DISCOVERY_ID,
        core_sim::SEED_SELECTION_DISCOVERY_ID,
    ] {
        app.world
            .resource_mut::<core_sim::DiscoveryProgressLedger>()
            .add_progress(FactionId(0), knowledge, scalar_one());
    }
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let position = seat(&ladder);
    let crop = {
        let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let mut query = app.world.query::<&core_sim::Tile>();
        let ground = query
            .iter(&app.world)
            .find(|tile| tile.position == source)
            .expect("the source tile exists")
            .clone();
        let composition =
            core_sim::tile_flora_composition(&flora, &labor.forage, &ground, map_seed);
        core_sim::default_species_for_rung(&composition, &flora, crop_rung)
            .expect("the site was chosen for having one")
    };
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patch_mut(source)
            .expect("the site carries a patch");
        patch.set_ladder_position(position, &core_sim::LadderConfig::builtin());
        patch.owner = Some(FactionId(0));
        patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
        patch.species = Some(crop);
    }
    spawn_the_farming_band(&mut app, tile, source, A_GATHERER, builders, declared);
    (app, source)
}

/// A resident band gathering `source`, holding it, and standing a `builders` pool on a queued `Sow`.
fn spawn_the_farming_band(
    app: &mut App,
    tile: bevy::prelude::Entity,
    source: UVec2,
    gatherers: u32,
    builders: u32,
    declared: Improvement,
) {
    // **The keeping is staffed generously**, because an unkept plant meter *rots* — this fixture is
    // about where the build is going, not about whether it survives neglect.
    let keepers = 8;
    app.world.spawn((
        PopulationCohort {
            home: tile,
            current_tile: tile,
            size: 60,
            children: scalar_zero(),
            working: scalar_from_f32((gatherers + builders + keepers) as f32),
            elders: scalar_zero(),
            stores: LocalStore::new(),
            morale: scalar_one(),
            last_food_consumption: 0.0,
            last_turn_transfer_received: 0.0,
            last_turn_transfer_sent: 0.0,
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
            faction: FactionId(0),
            knowledge: Vec::new(),
            migration: None,
        },
        StartingUnit {
            kind: "BandForager".to_string(),
            tags: Vec::new(),
        },
        ResidentBand,
        LaborAllocation {
            assignments: vec![
                LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: source,
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                        species: None,
                        take_species: TakeSelection::EVERYTHING,
                    },
                    workers: gatherers,
                    kit: None,
                },
                LaborAssignment {
                    target: LaborTarget::Builders,
                    workers: builders,
                    kit: None,
                },
                LaborAssignment {
                    target: LaborTarget::Agriculture,
                    workers: keepers,
                    kit: None,
                },
            ],
            build_queue: vec![core_sim::BuildQueueEntry {
                source: core_sim::BuildSource::Patch(source),
                declared: core_sim::BuildJob::Rung(declared),
                kit: None,
            }],
            ..Default::default()
        },
    ));
}

/// **One field of a patch's row, read out of the ENCODED buffer** — the artifact a client parses.
fn published_patch_field<T>(
    app: &App,
    source: UVec2,
    read: impl Fn(&shadow_scale_flatbuffers::generated::shadow_scale::sim::ForagePatchState<'_>) -> T,
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
        .and_then(|section| section.foragePatches())
        .expect("the subsistence section carries the patch list")
        .iter()
        .find(|patch| patch.x() == source.x && patch.y() == source.y)
        .expect("the fixture patch is on the wire");
    read(&row)
}

/// The pair this file is about, off one encoded patch row: `(live, destination)`.
fn published_patch_capacities(app: &App, source: UVec2) -> (f32, f32) {
    published_patch_field(app, source, |patch| {
        (patch.carryingCapacity(), patch.buildDestinationCapacity())
    })
}

fn patch_live_capacity(app: &App, source: UVec2) -> f32 {
    app.world
        .resource::<ForageRegistry>()
        .patch(source)
        .expect("the fixture patch survives")
        .carrying_capacity
}

/// ⛔ **THE FIELD A SOW ADVERTISES IS THE FIELD THE SOW DELIVERS.**
///
/// A real `Sow` is run to its destination and the capacity the patch then carries is compared against
/// the number that was being advertised while it built. Two derived numbers agreeing proves nothing
/// on its own — this arc has already shipped an equality that passed under sabotage because both
/// sides were zero — so the delivered capacity is also required to be **strictly above** the one the
/// patch started with, and the advertised figure to have stood **strictly above the live one** for
/// the whole climb.
///
/// The precondition is stated first and by name: only rung 3 raises `K` on this web, so a fixture
/// built on a `Cultivate` would compare a number with itself.
#[test]
fn a_sown_field_delivers_the_capacity_its_build_advertised() {
    let (mut app, source) = world_with_a_sow_in_flight();
    let gain = app
        .world
        .resource::<core_sim::LaborConfigHandle>()
        .get()
        .forage
        .cultivation
        .field_capacity_gain;
    assert!(
        gain > 1.0,
        "PRECONDITION: rung 3 must actually raise `K` ({gain}), or this test compares a number \
         with itself and passes with the field wired to anything"
    );

    let capacity_before = patch_live_capacity(&app, source);
    let mut advertised: Option<f32> = None;
    let mut turns = 0;
    // **Read the frame, THEN advance.** The entry retires on the turn its climb arrives, so a loop
    // that advanced first would only ever see the row after the destination had gone.
    loop {
        let (live, destination) = published_patch_capacities(&app, source);
        if destination != NO_BUILD_DESTINATION_CAPACITY {
            assert!(
                destination > live,
                "while a capacity-raising build runs the destination must stand ABOVE the live \
                 capacity — got destination {destination} against live {live} on turn {turns}"
            );
            if let Some(previous) = advertised {
                assert!(
                    (previous - destination).abs() < SAME_NUMBER,
                    "the destination is the DESTINATION's capacity, not next turn's: it moved \
                     from {previous} to {destination} on ground that did not move"
                );
            }
            advertised = Some(destination);
        }
        if app
            .world
            .resource::<ForageRegistry>()
            .patch(source)
            .expect("the fixture patch survives")
            .is_field()
        {
            break;
        }
        assert!(
            turns < BUILD_HORIZON,
            "fixture: the staffed Sow must land inside {BUILD_HORIZON} turns"
        );
        run_turn(&mut app);
        recapture_snapshot_in_place(&mut app.world);
        turns += 1;
    }

    let advertised = advertised.expect("the build was queued for at least one published frame");
    assert!(
        turns > 1,
        "fixture: the climb must span several turns, or the stability assertion above never runs"
    );
    // One more turn, because the rung lands in the Population stage and the capacity it buys is
    // written by the next Logistics pass.
    run_turn(&mut app);
    let delivered = patch_live_capacity(&app, source);
    assert!(
        delivered > capacity_before + SAME_NUMBER,
        "LIVENESS: the Field must actually have raised the patch's capacity — {capacity_before} \
         to {delivered}"
    );
    assert!(
        (delivered - advertised).abs() < SAME_NUMBER,
        "the capacity advertised while the Sow ran ({advertised}) must be the capacity the Field \
         delivers ({delivered})"
    );
}

/// **A `Cultivate` DESTINATION QUOTES THE CAPACITY THE PATCH ALREADY HAS — and that is the honest
/// answer, not a stale one.**
///
/// Rung 2 buys the ground a faster curve, not a denser one, so its destination capacity is the live
/// one. It is asserted because it is the arm that pins the reading to the **destination rung** rather
/// than to the top of the branch: a quote that reached for `plant:field` whatever the entry named
/// would publish `field_capacity_gain ×` here and be wrong by the whole gain.
#[test]
fn a_cultivate_destination_quotes_the_capacity_rung_two_actually_buys() {
    let (mut app, source) = world_with_a_cultivate_in_flight();
    run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);

    let rung = published_patch_field(&app, source, |patch| {
        patch.buildDestinationRung().unwrap_or_default().to_string()
    });
    assert_eq!(
        rung,
        RungKey::PlantTended.wire_key(),
        "fixture: the entry must be aimed at the tended rung"
    );
    let (live, destination) = published_patch_capacities(&app, source);
    assert!(
        (destination - live).abs() < SAME_NUMBER,
        "a Cultivate raises no `K`, so its destination capacity is the live one — got \
         {destination} against {live}"
    );
    assert_ne!(
        destination, NO_BUILD_DESTINATION_CAPACITY,
        "an equal reading is still a REAL reading: a queued source must never publish the \
         no-destination sentinel"
    );
}

/// ⛔ **A PATCH NOBODY HAS QUEUED PUBLISHES "NO DESTINATION", NOT A CAPACITY.**
///
/// The sentinel has to live outside the range a real answer lives in, because **zero is a real
/// capacity** — barren ground, an overgrazed range, a rock pen — and a `0` here would tell the player
/// that building the thing would leave them with nothing.
#[test]
fn an_unqueued_patch_publishes_no_destination_rather_than_a_capacity() {
    let (mut app, source) = world_with_a_sow_in_flight();
    // A patch on the same map that no band has queued anything on.
    let idle = app
        .world
        .resource::<ForageRegistry>()
        .patches
        .keys()
        .copied()
        .find(|tile| *tile != source)
        .expect("the map carries more than one patch");
    run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);

    let (live, destination) = published_patch_capacities(&app, idle);
    assert_eq!(
        destination, NO_BUILD_DESTINATION_CAPACITY,
        "an unqueued patch has no destination to quote"
    );
    assert!(
        destination < 0.0 && live >= 0.0,
        "the no-destination reading must sit OUTSIDE the range a capacity lives in — sentinel \
         {destination} against a real capacity {live}"
    );
    assert!(
        published_patch_field(&app, idle, |patch| patch
            .buildDestinationRung()
            .unwrap_or_default()
            .is_empty()),
        "fixture: the idle patch must genuinely be in nobody's queue"
    );
}

// ---------------------------------------------------------------------------------------------
// The animal web
// ---------------------------------------------------------------------------------------------

/// **The species the herd fixture is reshaped into** — a roster row that will actually tame and that
/// declares a `pastoral_density` above 1, which is the whole quantity under test. The same row
/// `neglect_countdown_on_the_wire.rs` pins.
const PASTORAL_SPECIES: &str = "Wild Boar";

/// A headless world whose one game herd is a [`PASTORAL_SPECIES`] flock with a staffed `Tame` queued
/// on it, standing still on a range held at full graze.
fn world_with_a_tame_in_flight() -> (App, String) {
    world_with_an_animal_build(Improvement::Tame, ALREADY_TAMED_NO)
}

/// [`world_with_a_tame_in_flight`]'s rung-3 twin: the same flock, **already tamed**, knowing Penning,
/// with a `Corral` in the queue.
fn world_with_a_corral_in_flight() -> (App, String) {
    world_with_an_animal_build(Improvement::Corral, ALREADY_TAMED_YES)
}

/// Whether the fixture hands the band a herd it has already tamed — rung 3's precondition, and the
/// flag is named rather than a bare bool at the call sites.
const ALREADY_TAMED_YES: bool = true;
const ALREADY_TAMED_NO: bool = false;

/// The shared animal fixture.
fn world_with_an_animal_build(declared: Improvement, already_tamed: bool) -> (App, String) {
    let mut app = build_test_app();
    app.update();

    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|herd| herd.id.starts_with("game_"))
            .map(|herd| herd.id.clone())
            .expect("the map seeded short-range game")
    };
    let (body_mass, ceiling, taming_cost) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let species = fauna
            .species_by_display(PASTORAL_SPECIES)
            .expect("the shipped roster carries the fixture species");
        (
            species.body_mass,
            species.husbandry_ceiling,
            fauna.taming_cost_multiplier_for(PASTORAL_SPECIES),
        )
    };
    app.world
        .resource_mut::<core_sim::DiscoveryProgressLedger>()
        .add_progress(FactionId(0), core_sim::HERDING_DISCOVERY_ID, scalar_one());

    let pos = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the herd the id came from");
        herd.species = PASTORAL_SPECIES.to_string();
        herd.body_mass = body_mass;
        herd.husbandry_ceiling = ceiling;
        herd.taming_cost_multiplier = taming_cost;
        herd.biomass = herd.carrying_capacity;
        // **HELD ON ONE HEX** — a single-anchor route is the roam's own "stays put" case
        // (`advance_herd_roam`), so the range the capacity is summed over is the same range every
        // turn. A herd that wandered would change the land under both readings and the equality
        // would be measuring the walk.
        herd.route = vec![herd.current_pos];
        herd.step_index = 0;
        herd.current_pos
    };
    if already_tamed {
        app.world
            .resource_mut::<core_sim::DiscoveryProgressLedger>()
            .add_progress(FactionId(0), core_sim::PENNING_DISCOVERY_ID, scalar_one());
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the fixture herd survives");
        assert!(
            herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin()),
            "fixture: the species must actually tame, or the Corral's rung-below gate refuses"
        );
    }
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("the herd's tile resolves");
    spawn_the_herding_band(&mut app, tile, &id, declared);
    (app, id)
}

/// A resident band hunting the herd (so the source is worked), keeping it, and standing a `builders`
/// pool on a queued `Tame`. Camped **on the herd's own tile**, so the drift a tamed herd gets is a
/// step it has already taken.
fn spawn_the_herding_band(
    app: &mut App,
    tile: bevy::prelude::Entity,
    herd_id: &str,
    declared: Improvement,
) {
    let hunters = 2;
    let keepers = 12;
    app.world.spawn((
        PopulationCohort {
            home: tile,
            current_tile: tile,
            size: 80,
            children: scalar_zero(),
            working: scalar_from_f32((hunters + A_STEADY_BUILD_POOL + keepers) as f32),
            elders: scalar_zero(),
            stores: LocalStore::new(),
            morale: scalar_one(),
            last_food_consumption: 0.0,
            last_turn_transfer_received: 0.0,
            last_turn_transfer_sent: 0.0,
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
            faction: FactionId(0),
            knowledge: Vec::new(),
            migration: None,
        },
        StartingUnit {
            kind: "BandHunter".to_string(),
            tags: Vec::new(),
        },
        ResidentBand,
        LaborAllocation {
            assignments: vec![
                LaborAssignment {
                    // **The food-peak floor, not the shipped default** — a `Tame` needs stock
                    // standing *above* the hunters' floor to accrue (`crew_is_working_the_source`),
                    // and the default floor sits high enough on this flock that the hunt holds it
                    // flat against the line and the build stalls forever. Every husbandry fixture in
                    // the suite hunts at the peak for the same reason.
                    target: LaborTarget::Hunt {
                        fauna_id: herd_id.to_string(),
                        floor: core_sim::MSY_BIOMASS_FRACTION,
                    },
                    workers: hunters,
                    kit: None,
                },
                LaborAssignment {
                    target: LaborTarget::Builders,
                    workers: A_STEADY_BUILD_POOL,
                    kit: None,
                },
                LaborAssignment {
                    target: LaborTarget::Husbandry,
                    workers: keepers,
                    kit: None,
                },
            ],
            build_queue: vec![core_sim::BuildQueueEntry {
                source: core_sim::BuildSource::Herd(herd_id.to_string()),
                declared: core_sim::BuildJob::Rung(declared),
                kit: None,
            }],
            ..Default::default()
        },
    ));
}

/// **Keep the fixture band fed and staffed.** A build long enough to *be* a climb outlives the
/// larder the band spawned with: unfed, it loses people every turn, `LaborAllocation::normalize`
/// trims the tail — the keeping role first, then the builders — and the fixture ends up measuring a
/// famine. Neither the food economy nor the trim is what this file is about, so the band is topped up
/// each turn and its crews restated.
fn keep_the_band_alive(app: &mut App) {
    let mut query = app
        .world
        .query::<(&mut PopulationCohort, &LaborAllocation)>();
    for (mut cohort, allocation) in query.iter_mut(&mut app.world) {
        let staffed: u32 = allocation
            .assignments
            .iter()
            .map(|assignment| assignment.workers)
            .sum();
        if staffed == 0 {
            continue;
        }
        cohort
            .stores
            .set(core_sim::FOOD, scalar_from_f32(A_FULL_LARDER));
        cohort.morale = scalar_one();
        cohort.size = cohort.size.max(A_BAND_THAT_AFFORDS_ITS_CREWS);
        cohort.working = scalar_from_f32(A_BAND_THAT_AFFORDS_ITS_CREWS as f32);
    }
}

/// **Seat this herd's keeping at exactly what it owes**, the way
/// `neglect_countdown_on_the_wire.rs` does. `advance_husbandry` clears `upkeep_supplied` after
/// reading it (the Population→Logistics carry the labor arm writes across), so a herd meant to stay
/// kept has to be re-seated every turn.
///
/// **Without it this fixture measures a shed, not a promise**: an unkept flock loses animals, the
/// hunters' escapement floor closes over what is left, and the `Tame`'s own gate stalls the build at
/// the two-thirds mark — three mechanics, none of them the one under test.
fn seat_the_keeping(app: &mut App, id: &str) {
    let supplied = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry.find(id).expect("the fixture herd survives");
        core_sim::herd_upkeep_demand(herd, &fauna, &ladder)
    };
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry
        .herds
        .iter_mut()
        .find(|herd| herd.id == id)
        .expect("the fixture herd survives")
        .upkeep_supplied = supplied;
}

/// **Hold the land still.** Every graze patch is put back to its own capacity, so the sustainable flow
/// the herd's `K` is summed from is the same flow every turn. It is the animal web's equivalent of a
/// tile's terrain not moving, and without it the equality under test would be competing with the
/// pasture's own economy.
fn refill_the_range(app: &mut App) {
    let mut graze = app.world.resource_mut::<GrazeRegistry>();
    for patch in graze.patches.values_mut() {
        patch.biomass = patch.carrying_capacity;
    }
}

/// **One field of a herd's row, read out of the ENCODED buffer.**
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

fn published_herd_capacities(app: &App, id: &str) -> (f32, f32) {
    published_herd_field(app, id, |herd| {
        (herd.carryingCapacity(), herd.buildDestinationCapacity())
    })
}

fn herd_live_capacity(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .expect("the fixture herd survives")
        .carrying_capacity
}

/// ⛔ **THE PASTURE A TAME ADVERTISES IS THE PASTURE THE TAME DELIVERS.**
///
/// The animal half of the promise, run through a real `Tame` on a herd whose range is held still. The
/// same three guards: the rung's own density gain asserted as a **precondition**, the advertised
/// figure required to stand strictly above the live one for the whole climb, and the delivered
/// capacity required to have actually risen — because an equality between two derived numbers that
/// are both zero is not an equality worth having.
#[test]
fn a_tamed_herd_delivers_the_capacity_its_build_advertised() {
    let (mut app, id) = world_with_a_tame_in_flight();
    let density = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .pastoral_density_for(PASTORAL_SPECIES);
    assert!(
        density > 1.0,
        "PRECONDITION: the fixture species' pastoral rung must actually raise `K` ({density}), or \
         this test compares a number with itself"
    );

    refill_the_range(&mut app);
    seat_the_keeping(&mut app, &id);
    keep_the_band_alive(&mut app);
    run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);
    let capacity_before = herd_live_capacity(&app, &id);
    let anchor = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .expect("the fixture herd survives")
        .position();

    let mut advertised: Option<f32> = None;
    let mut turns = 0;
    // **Read the frame, THEN advance** — see the plant twin: the entry retires on the turn it
    // arrives, so a loop that advanced first would never see the row while it still had a
    // destination.
    loop {
        assert_eq!(
            app.world
                .resource::<HerdRegistry>()
                .find(&id)
                .expect("the fixture herd survives")
                .position(),
            anchor,
            "fixture: the herd must stand still, or the land moved under both readings"
        );
        let (live, destination) = published_herd_capacities(&app, &id);
        if destination != NO_BUILD_DESTINATION_CAPACITY {
            assert!(
                destination > live,
                "while a capacity-raising build runs the destination must stand ABOVE the live \
                 capacity — got destination {destination} against live {live} on turn {turns}"
            );
            if let Some(previous) = advertised {
                assert!(
                    (previous - destination).abs() < SAME_NUMBER,
                    "the destination is the DESTINATION's capacity, not next turn's: it moved \
                     from {previous} to {destination} on a range that was held still"
                );
            }
            advertised = Some(destination);
        }
        if app
            .world
            .resource::<HerdRegistry>()
            .find(&id)
            .expect("the fixture herd survives")
            .is_domesticated()
        {
            break;
        }
        assert!(
            turns < BUILD_HORIZON,
            "fixture: the staffed Tame must land inside {BUILD_HORIZON} turns"
        );
        refill_the_range(&mut app);
        seat_the_keeping(&mut app, &id);
        keep_the_band_alive(&mut app);
        run_turn(&mut app);
        recapture_snapshot_in_place(&mut app.world);
        turns += 1;
    }

    let advertised = advertised.expect("the build was queued for at least one published frame");
    assert!(
        turns > 1,
        "fixture: the climb must span several turns, or the stability assertion above never runs"
    );
    refill_the_range(&mut app);
    seat_the_keeping(&mut app, &id);
    keep_the_band_alive(&mut app);
    run_turn(&mut app);
    let delivered = herd_live_capacity(&app, &id);
    assert!(
        delivered > capacity_before + SAME_NUMBER,
        "LIVENESS: taming must actually have raised the herd's capacity — {capacity_before} to \
         {delivered}"
    );
    assert!(
        (delivered - advertised).abs() < SAME_NUMBER,
        "the capacity advertised while the Tame ran ({advertised}) must be the capacity the \
         pastoral rung delivers ({delivered})"
    );
}

/// ⛔ **A CORRAL QUOTES THE FENCED LAND, NOT THE ROAM RANGE WITH A DENSITY ON IT.**
///
/// A pen is not the range multiplied by `pen_density` — it is a **different piece of land**: the
/// `penRadius` disk the herd is standing on, which `corral_at` anchors where the flock stands when
/// the build lands. Quoting rung 3 over the range the herd walks today would overstate it by the
/// whole ratio between the two footprints, which `pen_density` only partly gives back.
///
/// So this is the arm that pins the **footprint** half of the destination reading, and it is the one
/// case where the advertised figure sits *below* the live one — a fence is a smaller, denser place.
/// The promise is unchanged: run the real `Corral` to its destination and the pen holds what the
/// build said it would.
#[test]
fn a_penned_herd_delivers_the_capacity_its_fence_advertised() {
    let (mut app, id) = world_with_a_corral_in_flight();
    refill_the_range(&mut app);
    seat_the_keeping(&mut app, &id);
    keep_the_band_alive(&mut app);
    run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);

    let mut advertised: Option<f32> = None;
    let mut range_capacity: Option<f32> = None;
    let mut turns = 0;
    loop {
        let (live, destination) = published_herd_capacities(&app, &id);
        if destination != NO_BUILD_DESTINATION_CAPACITY {
            assert!(
                (destination - live).abs() > SAME_NUMBER,
                "LIVENESS: the fence must actually change what the land holds — the pen quote \
                 {destination} is the roam range's own {live}, so nothing about the footprint or \
                 the density reached this number"
            );
            if let Some(previous) = advertised {
                assert!(
                    (previous - destination).abs() < SAME_NUMBER,
                    "the destination is the DESTINATION's capacity, not next turn's: it moved \
                     from {previous} to {destination} on a range that was held still"
                );
            }
            advertised = Some(destination);
            range_capacity = Some(live);
        }
        if app
            .world
            .resource::<HerdRegistry>()
            .find(&id)
            .expect("the fixture herd survives")
            .is_corralled()
        {
            break;
        }
        assert!(
            turns < BUILD_HORIZON,
            "fixture: the staffed Corral must land inside {BUILD_HORIZON} turns"
        );
        refill_the_range(&mut app);
        seat_the_keeping(&mut app, &id);
        keep_the_band_alive(&mut app);
        run_turn(&mut app);
        recapture_snapshot_in_place(&mut app.world);
        turns += 1;
    }

    let advertised = advertised.expect("the build was queued for at least one published frame");
    let range_capacity = range_capacity.expect("the same frame carried the live range capacity");
    assert!(
        turns > 1,
        "fixture: the climb must span several turns, or the stability assertion above never runs"
    );
    refill_the_range(&mut app);
    seat_the_keeping(&mut app, &id);
    keep_the_band_alive(&mut app);
    run_turn(&mut app);
    let delivered = herd_live_capacity(&app, &id);
    assert!(
        (delivered - range_capacity).abs() > SAME_NUMBER,
        "LIVENESS: penning must actually have moved the herd's capacity off the range's — \
         {range_capacity} to {delivered}"
    );
    assert!(
        (delivered - advertised).abs() < SAME_NUMBER,
        "the capacity advertised while the Corral ran ({advertised}) must be the capacity the pen \
         delivers ({delivered})"
    );
}

/// ⛔ **"NO DESTINATION" AND "A DESTINATION WORTH NOTHING" ARE TWO READINGS, AND THIS IS WHERE THEY
/// ARE COMPARED.**
///
/// The same herd is read three ways in one test: queued over a living range (a real, positive
/// capacity), queued over a range with nothing standing on it (a real capacity of **zero**, still
/// queued), and unqueued (the sentinel). Only the third is the absent reading, and a `0` sentinel
/// would collapse the middle state into it.
///
/// The barren arm is staged by emptying the graze and **re-capturing**, never by running a turn: it
/// is the published reading under test, and starving the world would be measuring the fauna
/// pipeline's response instead.
#[test]
fn a_zero_destination_capacity_is_not_the_no_destination_reading() {
    let (mut app, id) = world_with_a_tame_in_flight();
    refill_the_range(&mut app);
    seat_the_keeping(&mut app, &id);
    keep_the_band_alive(&mut app);
    run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);

    let (_, living_range) = published_herd_capacities(&app, &id);
    assert!(
        living_range > 0.0,
        "fixture: a queued herd on a living range must quote a real, positive destination — got \
         {living_range}"
    );

    // **Nothing standing on the range** — the flow the destination is summed from is zero, so the
    // destination capacity is honestly zero while the entry is still in the queue.
    {
        let mut graze = app.world.resource_mut::<GrazeRegistry>();
        for patch in graze.patches.values_mut() {
            patch.biomass = 0.0;
        }
    }
    recapture_snapshot_in_place(&mut app.world);
    let barren = published_herd_field(&app, &id, |herd| herd.buildDestinationCapacity());
    let still_queued = published_herd_field(&app, &id, |herd| {
        herd.buildDestinationRung().unwrap_or_default().to_string()
    });
    assert!(
        !still_queued.is_empty(),
        "fixture: the barren arm must still be a queued build, or it is measuring the unqueued case"
    );
    assert_eq!(
        barren, 0.0,
        "a range with nothing on it holds nothing — the honest destination capacity is zero"
    );
    assert_ne!(
        barren, NO_BUILD_DESTINATION_CAPACITY,
        "a real capacity of zero must NOT read as the absent answer — that ambiguity is the whole \
         reason the sentinel is negative"
    );

    // **And an unqueued herd is the absent answer.** Same herd, entry withdrawn.
    {
        let mut query = app.world.query::<&mut LaborAllocation>();
        for mut allocation in query.iter_mut(&mut app.world) {
            allocation.build_queue.clear();
        }
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        for herd in registry.herds.iter_mut() {
            herd.build_destination = None;
        }
    }
    recapture_snapshot_in_place(&mut app.world);
    assert_eq!(
        published_herd_field(&app, &id, |herd| herd.buildDestinationCapacity()),
        NO_BUILD_DESTINATION_CAPACITY,
        "a herd in nobody's queue has no destination to quote"
    );
}
