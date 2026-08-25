//! **A SOW IS PRICED BY HOW MUCH OF THE TILE IT HAS TO REPLACE**
//! (`docs/plan_standing_upkeep.md` §4.15).
//!
//! Sowing a crop that already holds most of the ground is tidying; sowing one that holds a tenth is
//! replacing the tile. The `plant:field` rung's declared `work_cost` is what that job costs on
//! *reference* ground, and any particular patch pays it times its own multiplier — the ladder's one
//! per-source price hook (`RungStanding::at`'s `cost_at`), which the animal web already uses for a
//! species' `taming_cost_multiplier`.
//!
//! # WHAT THIS FILE PINS, AND WHY IT IS A SEPARATE FILE FROM THE UNIT TESTS
//!
//! The arithmetic — the anchor, the two clamps, the per-leg freeze — is pinned in
//! `forage::tests`, where the seams are reachable directly. What only a **whole turn loop** can
//! answer is the pair below, and both are about the price the player is *shown*:
//!
//! 1. **A two-leg Sow re-quotes its Field leg after the Cultivate leg beneath it**, and the re-quote
//!    reflects the weeding that leg did — while being the *same* number the entry was quoted at
//!    declaration, which is what keeps §4.6b's chained finish date exact rather than an estimate
//!    that drifts under the player.
//! 2. **The published price is the work the sim charges.** §4.3's rule is that a quote is asserted
//!    against the payoff function; a multiplier that landed on the charge and not on the forecast
//!    would have the compose sheet quoting a price the job does not take, which is a defect class
//!    this arc has shipped before.
//! 3. **The PER-CROP price and the patch's own price are one number.** The crop picker states a work
//!    figure against every crop it offers (`FloraShareInfo.sowWorkCost`), and the patch states one
//!    for the crop it is committed to (`fieldWorkCost`) — two published numbers for one fact, which
//!    have to be the same number or the picker is lying about whichever the player picked.
//!
//! **Read off the ENCODED buffer**, the discipline `build_turns_on_the_wire.rs` and
//! `destination_capacity_on_the_wire.rs` follow: a field can be right in the capture and wrong in the
//! envelope, and the envelope is what a client actually sees.

use bevy::app::App;
use bevy::math::UVec2;

use core_sim::{
    build_test_app, recapture_snapshot_in_place, run_turn, scalar_from_f32, scalar_one,
    scalar_zero, FactionId, ForageRegistry, GenerationId, Improvement, LaborAllocation,
    LaborAssignment, LaborTarget, LadderConfigHandle, LocalStore, MoraleCause, PopulationCohort,
    ResidentBand, RungKey, SnapshotHistory, SourcePriority, StartingUnit, TakeSelection,
    TileRegistry, DEFAULT_ESCAPEMENT_FLOOR,
};

/// How close two work-unit figures have to be to be the same job — pure f32 slack.
const SAME_JOB: f32 = 1e-2;

/// **Stock well above the escapement floor**, so the crew is genuinely working the source and the
/// rung's gate is about the staffing rather than about an empty patch.
const STOCKED_STANDING_CROP: f32 = 0.8;

/// One gatherer beside the build, so the patch is a worked source.
const A_GATHERER: u32 = 1;

/// **A build pool that lays each leg over several turns**, so the re-quote happens at a leg boundary
/// the loop can actually observe rather than inside one turn's accrual.
const A_MEASURED_BUILD_POOL: u32 = 6;

/// Keepers enough that an unkept plant meter does not rot out from under the measurement.
const A_FULL_KEEPING_CREW: u32 = 8;

/// Food enough that nobody in the fixture band goes hungry over the run.
const A_FULL_LARDER: f32 = 10_000.0;

/// The turns a fixture waits for its build before calling it stuck. Generous: the bound exists to
/// fail with a message rather than to hang.
const BUILD_HORIZON: u32 = 200;

// ---------------------------------------------------------------------------------------------
// The fixture
// ---------------------------------------------------------------------------------------------

/// **A watered gathering site whose basket can climb to `plant:field` AND whose crop is priced
/// strictly between the clamps.**
///
/// The second half is what makes this file's assertions mean something. On ground whose dominant crop
/// weeds all the way to the whole basket the price sits flat on its floor, and a test run there would
/// compare the floor with the floor and pass with the measure wired to anything. So the scan skips
/// those tiles by name rather than hoping.
fn an_informatively_priced_sowable_site(app: &mut App) -> UVec2 {
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
            let Some(patch) = app.world.resource::<ForageRegistry>().patch(coord) else {
                continue;
            };
            let fresh_water =
                core_sim::tile_is_fresh_watered(ground, width, height, wrap, |neighbor| {
                    tiles.get(&neighbor).map(|tile| tile.terrain_tags)
                });
            if core_sim::rung_site_refusal(
                ladder.rung(RungKey::PlantField),
                ground,
                &labor.forage,
                app.world
                    .resource::<core_sim::FoodSiteRegistry>()
                    .is_site(coord),
                fresh_water,
            )
            .is_some()
            {
                continue;
            }
            let composition =
                core_sim::tile_flora_composition(&flora, &labor.forage, ground, map_seed);
            if core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantField)
                .is_none()
            {
                continue;
            }
            let multiplier = core_sim::patch_field_cost_multiplier(
                patch,
                &composition,
                &flora,
                &labor.forage,
                &ladder,
            );
            let cultivation = &labor.forage.cultivation;
            if multiplier <= cultivation.field_share_cost_floor + SAME_JOB
                || multiplier >= cultivation.field_share_cost_ceiling - SAME_JOB
            {
                continue;
            }
            return coord;
        }
    }
    panic!(
        "the shipped map must carry sowable ground whose crop is priced off both clamps — without \
         one, every assertion in this file would compare a clamp with itself"
    );
}

/// A wild patch on `source` with a staffed `Sow` queued on it: **two legs**, because the ground has
/// not been cleared. Returns the app and the tile.
fn a_two_leg_sow() -> (App, UVec2) {
    let mut app = build_test_app();
    app.update();
    let source = an_informatively_priced_sowable_site(&mut app);
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
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patch_mut(source)
            .expect("the site carries a patch");
        // **Untouched ground** — the two-leg case the re-quote exists for.
        patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
    }
    spawn_the_farming_band(&mut app, tile, source);
    recapture_snapshot_in_place(&mut app.world);
    (app, source)
}

/// A resident band gathering `source`, holding it, and standing a builders pool on a queued `Sow`.
fn spawn_the_farming_band(app: &mut App, tile: bevy::prelude::Entity, source: UVec2) {
    let mut stores = LocalStore::new();
    stores.add(core_sim::FOOD, scalar_from_f32(A_FULL_LARDER));
    app.world.spawn((
        PopulationCohort {
            home: tile,
            current_tile: tile,
            size: 60,
            children: scalar_zero(),
            working: scalar_from_f32(
                (A_GATHERER + A_MEASURED_BUILD_POOL + A_FULL_KEEPING_CREW) as f32,
            ),
            elders: scalar_zero(),
            stores,
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
                    workers: A_GATHERER,
                    kit: None,
                    priority: SourcePriority::default(),
                },
                LaborAssignment {
                    target: LaborTarget::Builders,
                    workers: A_MEASURED_BUILD_POOL,
                    kit: None,
                    priority: SourcePriority::default(),
                },
                LaborAssignment {
                    target: LaborTarget::Agriculture,
                    workers: A_FULL_KEEPING_CREW,
                    kit: None,
                    priority: SourcePriority::default(),
                },
            ],
            build_queue: vec![core_sim::BuildQueueEntry {
                source: core_sim::BuildSource::Patch(source),
                declared: core_sim::BuildJob::Rung(Improvement::Sow),
                kit: None,
            }],
            ..Default::default()
        },
    ));
}

/// **One patch row as the wire carries it**, copied out of the envelope so the borrowed buffer can
/// be dropped. Every figure this file asserts on is read here and nowhere else: a price can be right
/// in the capture and wrong in the envelope, and the envelope is what a client sees.
struct PublishedPatch {
    tile: UVec2,
    /// The patch's own `plant:field` price — the one crop it is committed to (or the rung's
    /// auto-pick), in work units.
    field_work_cost: f32,
    /// `""` when the patch is still the wild mixed basket.
    committed_species: String,
    crops: Vec<PublishedCrop>,
}

/// One entry of a published basket — the crop picker's row.
struct PublishedCrop {
    species: String,
    share: f32,
    can_sow: bool,
    /// What a Sow committing this tile to *this* crop would be charged, in work units. `0` is the
    /// wire's "no figure" for a crop that cannot climb to a Field here.
    sow_work_cost: f32,
}

/// Every patch row on the wire, in the order the snapshot states them.
fn published_patches(app: &App) -> Vec<PublishedPatch> {
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
        .and_then(|section| section.foragePatches())
        .expect("the subsistence section carries the patch list")
        .iter()
        .map(|row| PublishedPatch {
            tile: UVec2::new(row.x(), row.y()),
            field_work_cost: row.fieldWorkCost(),
            committed_species: row.committedSpecies().unwrap_or_default().to_string(),
            crops: row
                .composition()
                .map(|basket| {
                    basket
                        .iter()
                        .map(|entry| PublishedCrop {
                            species: entry.species().unwrap_or_default().to_string(),
                            share: entry.share(),
                            can_sow: entry.canSow(),
                            sow_work_cost: entry.sowWorkCost(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

/// The fixture patch's row, by coord — the rows are sorted by `(y, x)`, so the lookup is never by
/// position in the list.
fn published_patch(app: &App, source: UVec2) -> PublishedPatch {
    published_patches(app)
        .into_iter()
        .find(|patch| patch.tile == source)
        .expect("the fixture patch is on the wire")
}

/// The `plant:field` price this patch's row publishes, read off the encoded envelope.
fn published_field_work_cost(app: &App, source: UVec2) -> f32 {
    published_patch(app, source).field_work_cost
}

fn ladder_position(app: &App, source: UVec2) -> f32 {
    app.world
        .resource::<ForageRegistry>()
        .patch(source)
        .expect("the fixture patch survives")
        .ladder_position()
}

/// **The wire's "this crop cannot be sown here"** — a Sow price is never legitimately `0`, so the
/// schema default reads as *no figure* and a client renders no row rather than a free Sow.
const NO_SOW_WORK_COST: f32 = 0.0;

/// The `plant:field` rung's **declared** `work_cost` — the price on reference ground, which every
/// per-patch and per-crop figure published here is a multiple of.
fn field_declared_cost(app: &App) -> f32 {
    core_sim::plant_rung_span(
        RungKey::PlantField,
        &app.world.resource::<LadderConfigHandle>().get(),
    )
    .1
}

fn field_base(app: &App) -> f32 {
    core_sim::plant_rung_span(
        RungKey::PlantField,
        &app.world.resource::<LadderConfigHandle>().get(),
    )
    .0
}

// ---------------------------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------------------------

/// ⛔ **A TWO-LEG SOW RE-QUOTES ITS FIELD LEG WHEN THAT LEG STARTS, AND THE RE-QUOTE REFLECTS THE
/// WEEDING THE CULTIVATE LEG DID.**
///
/// A `Sow` on untended ground is two legs, and the Cultivate leg beneath genuinely raises the crop's
/// share before the Field leg begins. The mechanism does not care that it was a Cultivate — it reads
/// the ground as it stands when the leg starts — so the stamp is the **weeded** share's price and not
/// the wild basket's.
///
/// **And the re-quote is the number the entry was already quoted at**, which is the other half: the
/// Field leg can only begin from a full tended rung, and a full tended rung's mix is the weeded one by
/// construction. So this is a discrete re-quote at a leg boundary rather than a price that drifts, and
/// §4.6b's chained finish date stays exact.
#[test]
fn a_two_leg_sow_re_quotes_its_field_leg_off_the_weeded_ground() {
    let (mut app, source) = a_two_leg_sow();
    let (weeded_price, wild_price) = {
        let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let entity = app
            .world
            .resource::<TileRegistry>()
            .index(source.x, source.y)
            .expect("the fixture tile resolves");
        let ground = app
            .world
            .get::<core_sim::Tile>(entity)
            .expect("the fixture tile");
        let composition = core_sim::tile_flora_composition(&flora, &labor.forage, ground, map_seed);
        let crop = core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantField)
            .expect("the site was chosen for having one");
        let wild_share = composition
            .iter()
            .find(|entry| entry.species == crop)
            .map_or(0.0, |entry| entry.share);
        // **The share the Cultivate leg leaves behind** — `min(1, share x tended_weeding_gain)`
        // bounded by what working the ground can clear, which on this basket is the unbounded
        // product (asserted as a precondition below by comparing the two prices).
        let weeded = (wild_share * labor.forage.cultivation.tended_weeding_gain).min(1.0);
        assert!(
            weeded > wild_share + 1e-3,
            "PRECONDITION: the Cultivate leg must actually raise the crop's share ({wild_share} \
             to {weeded}), or there is no weeding for the re-quote to reflect"
        );
        (
            core_sim::field_cost_multiplier_at_share(weeded, &labor.forage.cultivation),
            core_sim::field_cost_multiplier_at_share(wild_share, &labor.forage.cultivation),
        )
    };
    assert!(
        weeded_price < wild_price - SAME_JOB,
        "PRECONDITION: weeding must make the Sow cheaper ({weeded_price} against {wild_price}), \
         or this test cannot tell the two measures apart"
    );

    // **The quote struck before a single turn of work** — what the compose sheet and the queue row
    // are showing the player while the Cultivate leg is still ahead of them.
    let declared_price = published_field_work_cost(&app, source);

    // Run until the Field leg has genuinely started.
    let base = field_base(&app);
    let mut turns = 0;
    while ladder_position(&app, source) <= base {
        assert!(
            turns < BUILD_HORIZON,
            "fixture: the Cultivate leg must land inside {BUILD_HORIZON} turns"
        );
        run_turn(&mut app);
        turns += 1;
    }
    assert!(
        turns > 1,
        "fixture: the first leg must span several turns, or the two legs are one event"
    );
    recapture_snapshot_in_place(&mut app.world);

    let stamped = app
        .world
        .resource::<ForageRegistry>()
        .patch(source)
        .expect("the fixture patch survives")
        .quoted_field_cost_multiplier()
        .expect("a leg that has taken work carries the price it was quoted at");
    assert!(
        (stamped - weeded_price).abs() < SAME_JOB,
        "the Field leg is priced off the WEEDED ground the Cultivate leg left ({weeded_price}), \
         not the wild basket ({wild_price}): got {stamped}"
    );

    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let quoted_at_declaration = ladder
        .rung(RungKey::PlantField)
        .build_cost(weeded_price)
        .expect("the Field rung builds");
    assert!(
        (declared_price - quoted_at_declaration).abs() < SAME_JOB,
        "…and that is the number the entry was quoted at from the first frame ({declared_price} \
         against {quoted_at_declaration}) — a chained date that moved at the leg boundary would \
         be an estimate, not a construction"
    );
}

/// ⛔ **THE PUBLISHED PRICE IS THE WORK THE SIM CHARGES.**
///
/// A real Sow is run to its destination and the work the Field rung actually took is compared with the
/// figure its row was publishing the whole way. Two derived numbers agreeing proves nothing on its
/// own, so the published figure is also required to differ from the ladder's declared `work_cost` —
/// otherwise the test passes with the multiplier dropped on the floor.
#[test]
fn the_published_sow_price_is_the_work_the_field_rung_takes() {
    let (mut app, source) = a_two_leg_sow();
    let base = field_base(&app);
    let declared = core_sim::plant_rung_span(
        RungKey::PlantField,
        &app.world.resource::<LadderConfigHandle>().get(),
    )
    .1;

    let mut published: Option<f32> = None;
    let mut turns = 0;
    loop {
        recapture_snapshot_in_place(&mut app.world);
        let quote = published_field_work_cost(&app, source);
        if let Some(previous) = published {
            assert!(
                (previous - quote).abs() < SAME_JOB,
                "the published price must not drift while the job runs: {previous} to {quote} \
                 on turn {turns}"
            );
        }
        published = Some(quote);
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
        turns += 1;
    }

    let published = published.expect("the row publishes a price on every frame");
    assert!(
        (published - declared).abs() > SAME_JOB,
        "PRECONDITION: the fixture ground must be priced away from the ladder's declared \
         {declared}, or a published figure that ignored the measure would pass: got {published}"
    );
    let charged = ladder_position(&app, source) - base;
    assert!(
        (charged - published).abs() < SAME_JOB,
        "the Field rung took {charged} work units against a published price of {published} — a \
         sheet that quotes one number and a job that charges another is the defect §4.3's rule \
         exists to catch"
    );
}

/// ⛔ **THE PER-CROP PRICE AND THE PATCH'S OWN PRICE ARE ONE NUMBER.**
///
/// The crop picker states a work figure against every crop it offers, and the patch states one for
/// the crop it is committed to. Two published numbers for one fact that disagree is the defect class
/// §4.3's rule exists to catch — so the entry for the committed crop must be *exactly*
/// `fieldWorkCost`, before a single turn of work and again once the Field leg has started and been
/// stamped.
///
/// The second reading is the one that matters: a stamped leg answers the patch's own price list, so
/// a per-crop figure derived from anywhere else would drift apart from it at the leg boundary.
#[test]
fn the_per_crop_sow_price_is_the_price_this_patch_is_charged() {
    let (mut app, source) = a_two_leg_sow();
    let declared = field_declared_cost(&app);

    // **① Untouched ground, where the patch prices the rung's AUTO-PICK.** The row a player is
    // shown before anyone has worked here, which is where the picker is actually read.
    let at_declaration = published_patch(&app, source);
    assert!(
        at_declaration.committed_species.is_empty(),
        "PRECONDITION: the fixture patch must still be the wild basket on the first frame, or this \
         reading is the committed one below"
    );
    assert!(
        (at_declaration.field_work_cost - declared).abs() > SAME_JOB,
        "PRECONDITION: the fixture ground must be priced away from the ladder's declared \
         {declared}, or an entry that published the bare rung cost would pass: got {}",
        at_declaration.field_work_cost
    );
    let picked = auto_picked_crop(&app, source);
    assert_crop_is_priced_as_the_patch(&at_declaration, &picked, "on untouched ground");

    // **② One turn in, where the crew's first work COMMITS the patch to that crop.**
    run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);
    let committed = published_patch(&app, source);
    assert!(
        !committed.committed_species.is_empty(),
        "PRECONDITION: a worked Sow must commit the patch to a crop, or there is no commitment to \
         price against"
    );
    let crop = committed.committed_species.clone();
    assert_crop_is_priced_as_the_patch(&committed, &crop, "once committed");

    // **③ With the Field leg running, where the patch answers its STAMPED price** — the reading
    // that matters, because a per-crop figure derived from anywhere else would drift apart from
    // the stamp at the leg boundary.
    let base = field_base(&app);
    let mut turns = 0;
    while ladder_position(&app, source) <= base {
        assert!(
            turns < BUILD_HORIZON,
            "fixture: the Cultivate leg must land inside {BUILD_HORIZON} turns"
        );
        run_turn(&mut app);
        turns += 1;
    }
    recapture_snapshot_in_place(&mut app.world);
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(source)
            .expect("the fixture patch survives")
            .quoted_field_cost_multiplier()
            .is_some(),
        "PRECONDITION: the Field leg must carry a stamp, or this reading is the same live measure \
         the ones above already made"
    );
    let running = published_patch(&app, source);
    assert_crop_is_priced_as_the_patch(&running, &crop, "with the Field leg running");
}

/// The invariant itself, stated once so all three readings above assert the same thing.
fn assert_crop_is_priced_as_the_patch(patch: &PublishedPatch, crop: &str, when: &str) {
    let row = patch
        .crops
        .iter()
        .find(|entry| entry.species == crop)
        .unwrap_or_else(|| panic!("{when}: `{crop}` is not in the patch's published basket"));
    assert!(
        (row.sow_work_cost - patch.field_work_cost).abs() < SAME_JOB,
        "{when}: the picker quotes {} work units for `{crop}` while the patch priced on that very \
         crop is charged {} — one fact, two published numbers",
        row.sow_work_cost,
        patch.field_work_cost
    );
}

/// **The crop an unworked patch is priced on** — the `plant:field` rung's own auto-pick over the
/// tile's basket, resolved through the same `forage::default_species_for_rung` seam the readout and
/// the labor arm both commit through.
fn auto_picked_crop(app: &App, source: UVec2) -> String {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(source.x, source.y)
        .expect("the fixture tile resolves");
    let ground = app
        .world
        .get::<core_sim::Tile>(entity)
        .expect("the fixture tile");
    let composition = core_sim::tile_flora_composition(&flora, &labor.forage, ground, map_seed);
    core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantField)
        .expect("the site was chosen for having a crop that climbs to a Field")
}

/// ⛔ **A CROP THAT ALREADY HOLDS THE GROUND IS CHEAPER TO SOW THAN A MARGINAL ONE**, and every
/// published figure sits inside the two clamps.
///
/// This is the whole point of the per-crop figure: sowing a crop that holds most of the tile is
/// tidying, sowing one that holds a tenth is replacing the tile, and the picker exists so a player
/// can weigh that against the payoff before committing. Asserted across every patch on the map
/// rather than on one fixture tile, so the ordering is a property of the measure and not of one
/// basket — with a liveness half, because a column of identical prices would satisfy
/// "non-increasing" everywhere.
#[test]
fn a_dominant_crop_is_cheaper_to_sow_than_a_marginal_one() {
    let (app, _) = a_two_leg_sow();
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let cultivation = &labor.forage.cultivation;
    let declared = field_declared_cost(&app);
    let floor = declared * cultivation.field_share_cost_floor;
    let ceiling = declared * cultivation.field_share_cost_ceiling;

    let mut saw_a_pair_priced_apart = false;
    let mut saw_a_crop_that_cannot_sow = false;
    for patch in published_patches(&app) {
        // **The WILD rows only.** A committed patch publishes its reweighted basket, so its shares
        // are the crop's *after* the commitment while the price is struck on the ground the tile
        // actually holds — a comparison there would be between two different baskets.
        if !patch.committed_species.is_empty() {
            continue;
        }
        for crop in &patch.crops {
            if !crop.can_sow {
                assert_eq!(
                    crop.sow_work_cost, NO_SOW_WORK_COST,
                    "`{}` cannot climb to a Field, so its row must carry NO figure rather than a \
                     price a player could act on",
                    crop.species
                );
                saw_a_crop_that_cannot_sow = true;
                continue;
            }
            assert!(
                crop.sow_work_cost >= floor - SAME_JOB && crop.sow_work_cost <= ceiling + SAME_JOB,
                "`{}` is priced at {} work units, outside the clamps [{floor}, {ceiling}] — both \
                 are load-bearing: without the floor a Sow on ground already wholly the crop is \
                 free, and without the ceiling a marginal crop's price is bounded only by the \
                 anchor",
                crop.species,
                crop.sow_work_cost
            );
        }
        // The basket is published share DESC, so each window reads dominant-then-marginal.
        for pair in patch.crops.windows(2) {
            let (dominant, marginal) = (&pair[0], &pair[1]);
            if !dominant.can_sow || !marginal.can_sow {
                continue;
            }
            assert!(
                dominant.share >= marginal.share,
                "PRECONDITION: the published basket must be ordered by share"
            );
            assert!(
                dominant.sow_work_cost <= marginal.sow_work_cost + SAME_JOB,
                "`{}` holds {} of the tile and is quoted {} work units, while `{}` holds {} and is \
                 quoted {} — a Sow is priced by what it REPLACES, so more ground already the crop \
                 can never cost more",
                dominant.species,
                dominant.share,
                dominant.sow_work_cost,
                marginal.species,
                marginal.share,
                marginal.sow_work_cost
            );
            saw_a_pair_priced_apart |= dominant.sow_work_cost < marginal.sow_work_cost - SAME_JOB;
        }
    }

    assert!(
        saw_a_pair_priced_apart,
        "no two crops anywhere on the map were priced apart — the figure is not reading the crop's \
         own share, and every ordering assertion above compared a number with itself"
    );
    assert!(
        saw_a_crop_that_cannot_sow,
        "no published basket named a plant that cannot climb to a Field — the 'no figure' half is \
         untested here"
    );
}
